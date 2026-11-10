//! Read models served by `query/*`.

use crate::{StorageError, now_ms};
use aurelius_domain::{EventKind, EventNotice, ProjectId, ProjectStatus, TaskId, job::JobState, project::Project, views::ProjectSummary};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub fn last_seq(conn: &Connection) -> Result<u64, StorageError> {
    Ok(conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |row| row.get::<_, i64>(0))? as u64)
}

fn parse_project_id(text: String) -> rusqlite::Result<ProjectId> {
    ProjectId::parse(&text)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

fn parse_status(text: String) -> rusqlite::Result<ProjectStatus> {
    ProjectStatus::parse(&text).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, format!("bad status {text}").into())
    })
}

/// Projects with their description text. `read_blob` resolves blob storage keys.
pub fn list_projects(
    conn: &Connection,
    read_blob: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<ProjectSummary>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, b.storage_key, p.status, p.version, p.created_at_ms,
                (SELECT COUNT(DISTINCT t.employee_id) FROM tasks t WHERE t.project_id = p.id AND t.employee_id IS NOT NULL
                   AND t.status NOT IN ('succeeded','failed','cancelled')),
                p.folder, p.dispatcher_employee_id, p.styleguide_url, p.ticket_worktrees
         FROM projects p LEFT JOIN blobs b ON b.id = p.description_blob_id
         WHERE p.status <> 'archived'
         ORDER BY p.created_at_ms, p.id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            parse_project_id(row.get(0)?)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            parse_status(row.get(3)?)?,
            row.get::<_, i64>(4)? as u64,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)? as u32,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, bool>(10)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, key, status, version, created_at_ms, employees_active, folder, dispatcher, styleguide_url, ticket_worktrees) = row?;
        out.push(ProjectSummary {
            id,
            name,
            description: key.as_deref().and_then(read_blob),
            status,
            version,
            created_at_ms,
            employees_active,
            folder,
            dispatcher_employee_id: dispatcher.and_then(|d| aurelius_domain::EmployeeId::parse(&d).ok()),
            styleguide_url,
            ticket_worktrees,
        });
    }
    Ok(out)
}

/// Current project state for command validation, read inside the write transaction.
pub fn load_project(tx: &Transaction<'_>, id: ProjectId) -> Result<Option<Project>, StorageError> {
    Ok(tx
        .query_row(
            "SELECT name, status, version FROM projects WHERE id = ?1",
            [id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, parse_status(row.get(1)?)?, row.get::<_, i64>(2)? as u64)),
        )
        .optional()?
        .map(|(name, status, version)| Project { id, name, description: None, status, version }))
}

pub struct JobRow {
    pub id: String,
    pub kind: String,
    pub payload: serde_json::Value,
    pub state: JobState,
    pub lease_epoch: i64,
    pub not_before_ms: i64,
    pub source_event_id: String,
    /// Why a ready job waits (`quota`, `auth`, `transient`, ...).
    pub last_error_code: Option<String>,
}

fn job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRow> {
    let payload: String = row.get(2)?;
    let state: String = row.get(3)?;
    Ok(JobRow {
        id: row.get(0)?,
        kind: row.get(1)?,
        payload: serde_json::from_str(&payload).unwrap_or_default(),
        state: JobState::parse(&state).unwrap_or(JobState::Dead),
        lease_epoch: row.get(4)?,
        not_before_ms: row.get(5)?,
        source_event_id: row.get(6)?,
        last_error_code: row.get(7)?,
    })
}

const JOB_COLUMNS: &str = "id, kind, payload_json, state, lease_epoch, not_before_ms, source_event_id, last_error_code";

pub fn list_jobs(conn: &Connection, limit: u32) -> Result<Vec<JobRow>, StorageError> {
    let mut stmt =
        conn.prepare(&format!("SELECT {JOB_COLUMNS} FROM jobs ORDER BY not_before_ms DESC, id DESC LIMIT ?1"))?;
    let rows = stmt.query_map([limit], job_from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Leased run jobs (chat answers and task starts): what the run limit counts.
pub fn running_runs(conn: &Connection) -> Result<u32, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM jobs WHERE kind IN ('run.chat', 'task.run') AND state = 'leased'", [], |r| r.get::<_, i64>(0))? as u32)
}

/// Whether any work is going or waiting: a run job that is ready or leased,
/// a run that is created, starting or running, or a task that is queued or
/// running. What "when all work is done" (`power/shutdown`) waits for.
pub fn has_work(conn: &Connection) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE kind IN ('run.chat', 'task.run') AND state IN ('ready', 'leased'))
             OR EXISTS(SELECT 1 FROM runs WHERE status IN ('created', 'starting', 'running'))
             OR EXISTS(SELECT 1 FROM employee_inbox WHERE state = 'queued')
             OR EXISTS(SELECT 1 FROM tasks WHERE status IN ('queued', 'running'))",
        [],
        |r| r.get::<_, bool>(0),
    )?)
}

/// Claim the oldest due job at `now`: set leased, bump the epoch (fencing token).
/// Run jobs (chat answers and task starts) respect the global run limit
/// (0 holds them all back) and at most one active run per conversation (one
/// mutating turn per employee).
pub fn claim_due_job(tx: &Transaction<'_>, owner: &str, now: i64, lease_ms: i64, global_runs: u32) -> Result<Option<JobRow>, StorageError> {
    let job = tx
        .query_row(
            &format!(
                "SELECT {JOB_COLUMNS} FROM jobs j WHERE j.state = 'ready' AND j.not_before_ms <= ?1
                 AND (j.kind NOT IN ('run.chat', 'task.run') OR (
                     (SELECT COUNT(*) FROM jobs a WHERE a.kind IN ('run.chat', 'task.run') AND a.state = 'leased') < ?2
                     AND (j.kind <> 'run.chat' OR (NOT EXISTS (SELECT 1 FROM jobs o WHERE o.kind = 'run.chat' AND o.state = 'leased'
                         AND json_extract(o.payload_json, '$.conversation_id') = json_extract(j.payload_json, '$.conversation_id'))
                       -- One run per employee (R2): a retry or a run back from a limit waits for its slot too.
                       AND (json_extract(j.payload_json, '$.employee_id') IS NULL OR NOT EXISTS (SELECT 1 FROM jobs o WHERE o.kind = 'run.chat' AND o.state = 'leased'
                         AND json_extract(o.payload_json, '$.employee_id') = json_extract(j.payload_json, '$.employee_id')
                         AND NOT EXISTS (SELECT 1 FROM approvals a WHERE a.run_id = o.id AND a.status = 'pending')))))))
                 ORDER BY j.priority DESC, j.not_before_ms, j.id LIMIT 1"
            ),
            params![now, global_runs],
            job_from_row,
        )
        .optional()?;
    let Some(job) = job else { return Ok(None) };
    lease(tx, job, owner, now, lease_ms).map(Some)
}

fn lease(tx: &Transaction<'_>, mut job: JobRow, owner: &str, now: i64, lease_ms: i64) -> Result<JobRow, StorageError> {
    tx.execute(
        "UPDATE jobs SET state = 'leased', lease_owner = ?2, lease_until_ms = ?3,
             lease_epoch = lease_epoch + 1, attempts = attempts + 1 WHERE id = ?1",
        params![job.id, owner, now + lease_ms],
    )?;
    job.lease_epoch += 1;
    job.state = JobState::Leased;
    Ok(job)
}

/// One job as the dispatcher reads it.
pub fn job_row(conn: &Connection, job_id: &str) -> Result<JobRow, StorageError> {
    Ok(conn.query_row(&format!("SELECT {JOB_COLUMNS} FROM jobs WHERE id = ?1"), [job_id], job_from_row)?)
}

/// Lease a job that was just made for a run a worker took from the inbox.
pub fn lease_new_job(tx: &Transaction<'_>, job_id: &str, owner: &str, now: i64, lease_ms: i64) -> Result<JobRow, StorageError> {
    let job = tx.query_row(&format!("SELECT {JOB_COLUMNS} FROM jobs WHERE id = ?1"), [job_id], job_from_row)?;
    lease(tx, job, owner, now, lease_ms)
}

/// True while `epoch` is the current lease of a leased job (fencing check for every write).
pub fn holds_lease(tx: &Transaction<'_>, job_id: &str, epoch: i64) -> Result<bool, StorageError> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE id = ?1 AND lease_epoch = ?2 AND state = 'leased')",
        params![job_id, epoch],
        |r| r.get(0),
    )?)
}

/// Put a leased job back for a later attempt (retry), or park it without
/// consuming an attempt (waiting for auth/quota). Fenced by epoch.
pub fn reschedule_job(
    tx: &Transaction<'_>,
    job_id: &str,
    epoch: i64,
    not_before_ms: i64,
    error_code: &str,
    consume_attempt: bool,
) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE jobs SET state = 'ready', lease_owner = NULL, lease_until_ms = NULL, not_before_ms = ?3,
             last_error_code = ?4, attempts = CASE WHEN ?5 THEN attempts ELSE MAX(attempts - 1, 0) END
         WHERE id = ?1 AND lease_epoch = ?2 AND state = 'leased'",
        params![job_id, epoch, not_before_ms, error_code, consume_attempt],
    )? == 1)
}

/// Give up on a job (no more attempts). Fenced by epoch.
pub fn kill_job(tx: &Transaction<'_>, job_id: &str, epoch: i64, error_code: &str) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE jobs SET state = 'dead', lease_owner = NULL, lease_until_ms = NULL, last_error_code = ?3
         WHERE id = ?1 AND lease_epoch = ?2 AND state = 'leased'",
        params![job_id, epoch, error_code],
    )? == 1)
}

/// Attempts used and the maximum, for retry decisions.
pub fn job_attempts(tx: &Transaction<'_>, job_id: &str) -> Result<(u32, u32), StorageError> {
    Ok(tx.query_row("SELECT attempts, max_attempts FROM jobs WHERE id = ?1", [job_id], |r| {
        Ok((r.get::<_, i64>(0)? as u32, r.get::<_, i64>(1)? as u32))
    })?)
}

/// Chat runs that are due and free to start but held back (machine load or
/// the run limit) and not yet marked with `reason`: they get it as their
/// last error code, so each change of reason is told once. Returns their ids.
pub fn mark_held_runs(tx: &Transaction<'_>, reason: &str, now: i64) -> Result<Vec<String>, StorageError> {
    let mut stmt = tx.prepare(
        "SELECT j.id FROM jobs j WHERE j.kind = 'run.chat' AND j.state = 'ready' AND j.not_before_ms <= ?1
         AND j.last_error_code IS NOT ?2
         AND NOT EXISTS (SELECT 1 FROM jobs o WHERE o.kind = 'run.chat' AND o.state = 'leased'
             AND json_extract(o.payload_json, '$.conversation_id') = json_extract(j.payload_json, '$.conversation_id'))",
    )?;
    let ids: Vec<String> = stmt.query_map(params![now, reason], |r| r.get(0))?.collect::<Result<_, _>>()?;
    for id in &ids {
        tx.execute("UPDATE jobs SET last_error_code = ?2 WHERE id = ?1", params![id, reason])?;
    }
    Ok(ids)
}

/// Runs (chat answers and task starts) that are due and free to start, so
/// only the machine's load holds them back.
pub fn held_runs(conn: &Connection, now: i64) -> Result<u32, StorageError> {
    Ok(crate::inbox::held(conn, now)? + conn.query_row(
        "SELECT COUNT(*) FROM jobs j WHERE j.kind IN ('run.chat', 'task.run') AND j.state = 'ready' AND j.not_before_ms <= ?1
         AND (j.kind <> 'run.chat' OR NOT EXISTS (SELECT 1 FROM jobs o WHERE o.kind = 'run.chat' AND o.state = 'leased'
             AND json_extract(o.payload_json, '$.conversation_id') = json_extract(j.payload_json, '$.conversation_id')))",
        [now],
        |r| r.get::<_, i64>(0),
    )? as u32)
}

/// Make parked jobs due now (e.g. after the user logged in again).
pub fn wake_parked(tx: &Transaction<'_>, error_code: &str, now: i64) -> Result<usize, StorageError> {
    Ok(wake_parked_for_login(tx, error_code, None, now)?.len())
}

/// Make parked jobs due now, only those of runs on `login_user`'s Claude login
/// (none: all of them, as `wake_parked`), e.g. when its quota resets or it is
/// linked again. Returns their ids (run id = job id for a chat run).
pub fn wake_parked_for_login(tx: &Transaction<'_>, error_code: &str, login_user: Option<&str>, now: i64) -> Result<Vec<String>, StorageError> {
    let ids: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT j.id FROM jobs j WHERE j.state = 'ready' AND j.last_error_code = ?1
             AND (?2 IS NULL OR EXISTS (SELECT 1 FROM runs r WHERE r.id = j.id AND r.login_user = ?2)
                  OR json_extract(j.payload_json, '$.login_user') = ?2)",
        )?;
        let rows = stmt.query_map(params![error_code, login_user], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<_, _>>()?
    };
    for id in &ids {
        tx.execute("UPDATE jobs SET not_before_ms = ?2, last_error_code = NULL WHERE id = ?1", params![id, now])?;
    }
    crate::inbox::wake(tx, error_code, login_user, now)?;
    Ok(ids)
}

/// The tasks waiting with 'login' for `login_user`'s Claude login: their latest
/// run was on it, or their ticket runs on it now. To resume once it is linked
/// (B11); the caller changes their status and tells it.
pub fn tasks_waiting_for_login(conn: &Connection, login_user: &str) -> Result<Vec<TaskId>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT t.id FROM tasks t WHERE t.status = 'waiting' AND t.waiting_reason = 'login'
         AND ((SELECT r.login_user FROM runs r WHERE r.task_id = t.id ORDER BY r.attempt DESC, r.rowid DESC LIMIT 1) = ?1
              OR EXISTS (SELECT 1 FROM tickets k WHERE k.id = t.ticket_id AND k.login_user = ?1))
         ORDER BY t.created_at_ms, t.id",
    )?;
    let ids: Vec<String> = stmt.query_map([login_user], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(ids.iter().filter_map(|t| TaskId::parse(t).ok()).collect())
}

/// The provider's usage limit is reached until `until_ms`: its runs that are
/// ready (not started) wait for the reset too, instead of each hitting the limit.
/// Returns their ids (run id = job id).
pub fn park_ready_runs_for_quota(tx: &Transaction<'_>, provider: &str, until_ms: i64) -> Result<Vec<String>, StorageError> {
    park_ready_runs_for_quota_of(tx, provider, None, until_ms)
}

/// Like `park_ready_runs_for_quota`, for the limit of one Claude login: only
/// the ready runs on `login_user`'s login wait (none: every run of the
/// provider, the old behaviour).
pub fn park_ready_runs_for_quota_of(tx: &Transaction<'_>, provider: &str, login_user: Option<&str>, until_ms: i64) -> Result<Vec<String>, StorageError> {
    let ids: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT j.id FROM jobs j JOIN runs r ON r.id = j.id
             WHERE j.kind = 'run.chat' AND j.state = 'ready' AND j.not_before_ms < ?1 AND r.runtime_kind = ?2
             AND (?3 IS NULL OR r.login_user = ?3)",
        )?;
        let rows = stmt.query_map(params![until_ms, provider, login_user], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<_, _>>()?
    };
    for id in &ids {
        tx.execute("UPDATE jobs SET not_before_ms = ?2, last_error_code = 'quota' WHERE id = ?1", params![id, until_ms])?;
    }
    Ok(ids)
}

/// Park a leased job that is no run (the dispatcher's, a memory job) until
/// `login_user`'s Claude login is (re)linked: `wake_parked_for_login` finds it by
/// the login in its payload. Fenced by epoch; never uses up an attempt.
pub fn park_job_for_login(tx: &Transaction<'_>, job_id: &str, epoch: i64, login_user: Option<&str>) -> Result<bool, StorageError> {
    tx.execute("UPDATE jobs SET payload_json = json_set(payload_json, '$.login_user', ?3) WHERE id = ?1 AND lease_epoch = ?2 AND state = 'leased'", params![job_id, epoch, login_user])?;
    reschedule_job(tx, job_id, epoch, i64::MAX / 2, "login", false)
}

/// Extend a lease; returns false when the epoch no longer matches.
pub fn renew_lease(tx: &Transaction<'_>, job_id: &str, epoch: i64, lease_ms: i64) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE jobs SET lease_until_ms = ?3 WHERE id = ?1 AND lease_epoch = ?2 AND state = 'leased'",
        params![job_id, epoch, now_ms() + lease_ms],
    )? == 1)
}

/// Mark done only if the caller still holds the current lease epoch.
pub fn complete_job(tx: &Transaction<'_>, job_id: &str, epoch: i64) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE jobs SET state = 'done', lease_owner = NULL, lease_until_ms = NULL
         WHERE id = ?1 AND lease_epoch = ?2 AND state = 'leased'",
        params![job_id, epoch],
    )? == 1)
}

/// Reconciliation for side-effect-free jobs: leases expiring before `cutoff_ms` go back to ready.
/// The daemon passes `i64::MAX` at startup: it holds the instance lock, so no other owner is alive.
pub fn release_leases(tx: &Transaction<'_>, kind: &str, cutoff_ms: i64) -> Result<usize, StorageError> {
    Ok(tx.execute(
        "UPDATE jobs SET state = 'ready', lease_owner = NULL, lease_until_ms = NULL
         WHERE state = 'leased' AND kind = ?1 AND lease_until_ms < ?2",
        params![kind, cutoff_ms],
    )?)
}

pub fn events_after(conn: &Connection, after_seq: u64, limit: u32) -> Result<Vec<EventNotice>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT seq, type, aggregate_id, project_id, occurred_at_ms
         FROM events WHERE seq > ?1 ORDER BY seq LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![after_seq as i64, limit], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?, row.get::<_, i64>(4)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (seq, kind, aggregate_id, project, occurred_at_ms) = row?;
        // A row of a removed feature stays in the log; clients have no kind for it.
        let Some(kind) = EventKind::parse(&kind) else { continue };
        out.push(EventNotice {
            seq: seq as u64,
            kind,
            aggregate_id,
            project_id: project.and_then(|t| ProjectId::parse(&t).ok()),
            occurred_at_ms,
        });
    }
    Ok(out)
}

/// What each ready or leased job works on, by name, keyed by job id: the employee
/// (and conversation title) of an answer, the title of a task. Jobs without a
/// readable name are left out.
pub fn job_label_names(conn: &Connection, rows: &[JobRow]) -> std::collections::HashMap<String, String> {
    let mut names = std::collections::HashMap::new();
    for row in rows.iter().filter(|r| matches!(r.state, JobState::Ready | JobState::Leased)) {
        let name: Option<String> = if let Some(conversation) = row.payload["conversation_id"].as_str() {
            conn.query_row(
                "SELECT COALESCE(m.name || CASE WHEN c.title <> '' AND c.title <> m.name THEN ' · ' || c.title ELSE '' END, NULLIF(c.title, ''))
                 FROM conversations c LEFT JOIN employees m ON m.id = c.employee_id WHERE c.id = ?1",
                [conversation],
                |r| r.get(0),
            )
            .ok()
            .flatten()
        } else if let Some(task) = row.payload["task_id"].as_str() {
            conn.query_row("SELECT title FROM tasks WHERE id = ?1", [task], |r| r.get(0)).ok()
        } else {
            None
        };
        if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
            names.insert(row.id.clone(), name);
        }
    }
    names
}
