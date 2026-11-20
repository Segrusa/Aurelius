//! Work sessions (`migrations/0043`): a conversation of kind `session` and the row
//! that says who started it and where it works. What is done with them is in
//! `aurelius_application::conversation::work_session`.

use crate::{StorageError, now_ms};
use aurelius_domain::history::{self, HistorySummaryState, SessionHistoryEntry};
use aurelius_domain::work_session::{SessionDispatch, WorkSessionView};
use aurelius_domain::{ConversationId, EmployeeId, ProjectId, RunId, UserId};
use rusqlite::types::ToSql;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// Where a session works besides the project: its own worktree and branch, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub project: ProjectId,
    pub started_by: UserId,
    /// `(worktree folder, branch)`; none: the project's folder itself.
    pub worktree: Option<(String, String)>,
    /// The dispatcher still chooses who takes it, or chose nobody (`migrations/0044`).
    pub dispatch: Option<SessionDispatch>,
}

/// Make the session's conversation and its row. `employee`: who works in it, or
/// the dispatcher who chooses (then `dispatch` is `Choosing`).
#[allow(clippy::too_many_arguments)]
pub fn insert(
    tx: &Transaction<'_>,
    id: ConversationId,
    project: ProjectId,
    employee: EmployeeId,
    title: &str,
    started_by: UserId,
    worktree: Option<(&str, &str)>,
    dispatch: Option<SessionDispatch>,
) -> Result<(), StorageError> {
    let now = now_ms();
    tx.execute(
        "INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES (?1, 'session', ?2, ?3, ?4, 1, ?5)",
        params![id.to_string(), project.to_string(), employee.to_string(), title, now],
    )?;
    tx.execute(
        "INSERT INTO work_sessions(conversation_id, started_by, worktree, branch, created_at_ms, dispatch) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id.to_string(), started_by.to_string(), worktree.map(|w| w.0), worktree.map(|w| w.1), now, dispatch.map(SessionDispatch::as_str)],
    )?;
    Ok(())
}

/// project, started_by, worktree, branch, dispatch.
type SessionRow = (String, String, Option<String>, Option<String>, Option<String>);

/// The session a conversation is, if it is one.
pub fn of_conversation(conn: &Connection, conversation: ConversationId) -> Result<Option<Session>, StorageError> {
    let row: Option<SessionRow> = conn
        .query_row(
            "SELECT c.project_id, s.started_by, s.worktree, s.branch, s.dispatch FROM work_sessions s JOIN conversations c ON c.id = s.conversation_id
             WHERE s.conversation_id = ?1 AND c.kind = 'session'",
            [conversation.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((project, started_by, worktree, branch, dispatch)) = row else { return Ok(None) };
    Ok(Some(Session {
        project: ProjectId::parse(&project).map_err(bad)?,
        started_by: UserId::parse(&started_by).map_err(bad)?,
        worktree: worktree.zip(branch),
        dispatch: dispatch.as_deref().and_then(SessionDispatch::parse),
    }))
}

/// The sessions `user` started, the latest activity first; `project`: only those of it.
pub fn list(conn: &Connection, user: UserId, project: Option<ProjectId>) -> Result<Vec<WorkSessionView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, p.id, p.name, e.id, e.name, c.title, s.worktree, s.branch, s.created_at_ms,
                COALESCE((SELECT MAX(m.created_at_ms) FROM messages m WHERE m.conversation_id = c.id), s.created_at_ms) AS last,
                EXISTS(SELECT 1 FROM runs r WHERE r.conversation_id = c.id AND r.status IN ('created','starting','running','waiting','cancelling')),
                s.dispatch
         FROM work_sessions s JOIN conversations c ON c.id = s.conversation_id
         JOIN projects p ON p.id = c.project_id JOIN employees e ON e.id = c.employee_id
         WHERE s.started_by = ?1 AND c.kind = 'session' AND c.archived_at_ms IS NULL AND (?2 IS NULL OR c.project_id = ?2)
         ORDER BY last DESC, c.id DESC",
    )?;
    let rows = stmt.query_map(params![user.to_string(), project.map(|p| p.to_string())], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, i64>(8)?,
            r.get::<_, i64>(9)?,
            r.get::<_, bool>(10)?,
            r.get::<_, Option<String>>(11)?,
        ))
    })?;
    rows.map(|row| {
        let (conversation, project, project_name, employee, employee_name, title, worktree, branch, created_at_ms, last_activity_ms, running, dispatch) = row?;
        let dispatch = dispatch.as_deref().and_then(SessionDispatch::parse);
        Ok(WorkSessionView {
            conversation_id: ConversationId::parse(&conversation).map_err(bad)?,
            project_id: ProjectId::parse(&project).map_err(bad)?,
            project_name,
            employee_id: EmployeeId::parse(&employee).map_err(bad)?,
            employee_name,
            title,
            worktree,
            branch,
            created_at_ms,
            last_activity_ms,
            // A dispatcher that still chooses is working on it too.
            running: running || dispatch == Some(SessionDispatch::Choosing),
            dispatch,
        })
    })
    .collect()
}

/// Set where the dispatcher is with a session (none: an employee works in it).
pub fn set_dispatch(tx: &Transaction<'_>, conversation: ConversationId, dispatch: Option<SessionDispatch>) -> Result<(), StorageError> {
    tx.execute("UPDATE work_sessions SET dispatch = ?2 WHERE conversation_id = ?1", params![conversation.to_string(), dispatch.map(SessionDispatch::as_str)])?;
    Ok(())
}

/// The employee who works in a session from now on (the one the dispatcher or the
/// board member chose).
pub fn set_employee(tx: &Transaction<'_>, conversation: ConversationId, employee: EmployeeId) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE conversations SET employee_id = ?2, version = version + 1 WHERE id = ?1 AND kind = 'session'",
        params![conversation.to_string(), employee.to_string()],
    )?;
    Ok(())
}

/// Mark `run` as a run of the dispatcher of the session `conversation`.
pub fn insert_dispatch_run(tx: &Transaction<'_>, run: RunId, conversation: ConversationId) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO session_dispatch_runs(run_id, conversation_id, created_at_ms) VALUES (?1, ?2, ?3)",
        params![run.to_string(), conversation.to_string(), now_ms()],
    )?;
    Ok(())
}

/// The session whose dispatcher `run` is a run of, if it is one.
pub fn dispatch_run_session(conn: &Connection, run: RunId) -> Result<Option<ConversationId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT conversation_id FROM session_dispatch_runs WHERE run_id = ?1", [run.to_string()], |r| r.get(0)).optional()?;
    id.map(|c| ConversationId::parse(&c).map_err(bad)).transpose()
}

/// The dispatcher's runs for a session so far; one that waited for its Claude login never ran and is no try.
pub fn dispatch_runs_so_far(conn: &Connection, conversation: ConversationId) -> Result<u32, StorageError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM session_dispatch_runs d JOIN runs r ON r.id = d.run_id WHERE d.conversation_id = ?1 AND COALESCE(r.error_code, '') NOT IN ('login', 'quota')",
        [conversation.to_string()],
        |r| r.get(0),
    )?;
    Ok(n as u32)
}

/// Runs of the session's dispatcher that never ended (the daemon stopped during
/// one) end as interrupted; each still counts as a try.
pub fn end_unfinished_dispatch_runs(tx: &Transaction<'_>, conversation: ConversationId) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE runs SET status = 'failed', error_code = 'interrupted', ended_at_ms = ?2
         WHERE id IN (SELECT run_id FROM session_dispatch_runs WHERE conversation_id = ?1) AND status IN ('created','starting','running','waiting','cancelling')",
        params![conversation.to_string(), now_ms()],
    )?;
    Ok(())
}

/// A summary of the session is asked for (`migrations/0046`); a summary already
/// there stays until the new one replaces it.
pub fn mark_summary_pending(tx: &Transaction<'_>, conversation: ConversationId) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE work_sessions SET summary_state = ?2 WHERE conversation_id = ?1 AND summary_text IS NULL",
        params![conversation.to_string(), history::stored::PENDING],
    )?;
    Ok(())
}

/// The session's summary, already through the vault redactor and
/// `history::history_summary`; `through_ms`: the newest message it covers.
pub fn set_summary(tx: &Transaction<'_>, conversation: ConversationId, text: &str, through_ms: i64) -> Result<bool, StorageError> {
    let changed = tx.execute(
        "UPDATE work_sessions SET summary_state = ?2, summary_text = ?3, summary_through_ms = ?4 WHERE conversation_id = ?1",
        params![conversation.to_string(), history::stored::READY, text, through_ms],
    )?;
    Ok(changed > 0)
}

/// The summary failed or gave nothing. An older summary stays as it is. Whether it changed.
pub fn set_summary_failed(tx: &Transaction<'_>, conversation: ConversationId) -> Result<bool, StorageError> {
    let changed = tx.execute(
        "UPDATE work_sessions SET summary_state = ?2 WHERE conversation_id = ?1 AND summary_text IS NULL",
        params![conversation.to_string(), history::stored::FAILED],
    )?;
    Ok(changed > 0)
}

/// What a session's summary goes on: the summary so far and the moment it covers.
pub fn summary_so_far(conn: &Connection, conversation: ConversationId) -> Result<(Option<String>, Option<i64>), StorageError> {
    Ok(conn
        .query_row("SELECT summary_text, summary_through_ms FROM work_sessions WHERE conversation_id = ?1", [conversation.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .unwrap_or((None, None)))
}

/// Mark that the session's title is asked for (`migrations/0047`). Whether it was
/// not asked for before: only then does the caller queue the job, so a session asks once.
pub fn mark_title_asked(tx: &Transaction<'_>, conversation: ConversationId) -> Result<bool, StorageError> {
    let changed = tx.execute(
        "UPDATE work_sessions SET title_asked_at_ms = ?2 WHERE conversation_id = ?1 AND title_asked_at_ms IS NULL",
        params![conversation.to_string(), now_ms()],
    )?;
    Ok(changed > 0)
}

/// Put a made title on a session that is still there (not deleted). Whether it changed.
pub fn set_title(tx: &Transaction<'_>, conversation: ConversationId, title: &str) -> Result<bool, StorageError> {
    let changed = tx.execute(
        "UPDATE conversations SET title = ?2, version = version + 1 WHERE id = ?1 AND kind = 'session' AND archived_at_ms IS NULL AND title <> ?2",
        params![conversation.to_string(), title],
    )?;
    Ok(changed > 0)
}

/// What `work_session.history` narrows on; every given one applies.
#[derive(Debug, Clone, Default)]
pub struct HistoryFilter {
    pub project: Option<ProjectId>,
    /// The employee who works in it.
    pub employee: Option<EmployeeId>,
    /// Last active at or after this moment.
    pub from_ms: Option<i64>,
    /// Last active before this moment.
    pub to_ms: Option<i64>,
    /// In the title or the summary, case-insensitive.
    pub query: Option<String>,
    /// Keyset: only rows after this one (`last_activity_ms`, conversation id) in the order of the read.
    pub before: Option<(i64, ConversationId)>,
}

/// Every board member's sessions that were not deleted, the latest activity
/// first (then the higher id), at most `limit`, and whether there are more.
/// `job_kind`: the summary job's kind, whose ready or leased job for a session
/// makes its row "summarizing". The text comes as stored; the daemon redacts it.
pub fn history(conn: &Connection, filter: &HistoryFilter, limit: u32, job_kind: &str) -> Result<(Vec<SessionHistoryEntry>, bool), StorageError> {
    let mut clauses = vec!["c.kind = 'session'".to_owned(), "c.archived_at_ms IS NULL".to_owned()];
    let mut values: Vec<Box<dyn ToSql>> = vec![Box::new(job_kind.to_owned())];
    let mut add = |clause: &dyn Fn(usize) -> String, value: Box<dyn ToSql>| {
        values.push(value);
        clauses.push(clause(values.len()));
    };
    if let Some(project) = filter.project {
        add(&|n| format!("c.project_id = ?{n}"), Box::new(project.to_string()));
    }
    if let Some(employee) = filter.employee {
        add(&|n| format!("c.employee_id = ?{n}"), Box::new(employee.to_string()));
    }
    if let Some(from) = filter.from_ms {
        add(&|n| format!("x.last >= ?{n}"), Box::new(from));
    }
    if let Some(to) = filter.to_ms {
        add(&|n| format!("x.last < ?{n}"), Box::new(to));
    }
    // Folded like SQLite's lower() (ASCII only), as the ticket History does.
    let needle = filter.query.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(|q| q.chars().take(history::HISTORY_QUERY_MAX_CHARS).collect::<String>().to_ascii_lowercase());
    if let Some(needle) = needle {
        add(&|n| format!("(instr(lower(c.title), ?{n}) > 0 OR instr(lower(COALESCE(s.summary_text, '')), ?{n}) > 0)"), Box::new(needle));
    }
    if let Some((last, id)) = filter.before {
        values.push(Box::new(last));
        let l = values.len();
        values.push(Box::new(id.to_string()));
        let i = values.len();
        clauses.push(format!("(x.last < ?{l} OR (x.last = ?{l} AND c.id < ?{i}))"));
    }
    let sql = format!(
        "SELECT c.id, c.title, p.id, p.name, e.id, e.name, s.started_by, u.username, s.branch, s.created_at_ms, x.last, s.summary_text, s.summary_state,
                EXISTS(SELECT 1 FROM jobs j WHERE j.kind = ?1 AND j.state IN ('ready','leased') AND json_extract(j.payload_json, '$.conversation_id') = c.id)
         FROM work_sessions s JOIN conversations c ON c.id = s.conversation_id
         JOIN (SELECT ws.conversation_id AS id, COALESCE((SELECT MAX(m.created_at_ms) FROM messages m WHERE m.conversation_id = ws.conversation_id), ws.created_at_ms) AS last
               FROM work_sessions ws) x ON x.id = c.id
         JOIN projects p ON p.id = c.project_id JOIN employees e ON e.id = c.employee_id
         LEFT JOIN users u ON u.id = s.started_by
         WHERE {} ORDER BY x.last DESC, c.id DESC LIMIT {}",
        clauses.join(" AND "),
        limit + 1
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values.iter().map(|v| v.as_ref())), |r| {
        Ok(HistoryRow {
            id: r.get(0)?,
            title: r.get(1)?,
            project: r.get(2)?,
            project_name: r.get(3)?,
            employee: r.get(4)?,
            employee_name: r.get(5)?,
            started_by: r.get(6)?,
            started_by_name: r.get(7)?,
            branch: r.get(8)?,
            created_at_ms: r.get(9)?,
            last_activity_ms: r.get(10)?,
            summary: r.get(11)?,
            state: r.get(12)?,
            pending: r.get(13)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        let row = row?;
        let (Ok(conversation_id), Ok(project_id), Ok(employee_id), Ok(started_by)) =
            (ConversationId::parse(&row.id), ProjectId::parse(&row.project), EmployeeId::parse(&row.employee), UserId::parse(&row.started_by))
        else {
            continue;
        };
        out.push(SessionHistoryEntry {
            conversation_id,
            title: row.title,
            project_id,
            project_name: row.project_name,
            employee_id,
            employee_name: row.employee_name,
            started_by,
            started_by_name: row.started_by_name,
            branch: row.branch,
            created_at_ms: row.created_at_ms,
            last_activity_ms: row.last_activity_ms,
            summary_state: HistorySummaryState::of(row.summary.is_some(), row.pending, row.state.as_deref()),
            summary: row.summary,
        });
    }
    let more = out.len() > limit as usize;
    out.truncate(limit as usize);
    Ok((out, more))
}

/// One row of `history` as SQLite gives it.
struct HistoryRow {
    id: String,
    title: String,
    project: String,
    project_name: String,
    employee: String,
    employee_name: String,
    started_by: String,
    started_by_name: Option<String>,
    branch: Option<String>,
    created_at_ms: i64,
    last_activity_ms: i64,
    summary: Option<String>,
    state: Option<String>,
    pending: bool,
}

fn bad(e: impl std::fmt::Display) -> StorageError {
    StorageError::Rejected(e.to_string())
}
