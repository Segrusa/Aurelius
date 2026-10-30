//! Task and dependency projections.

use crate::{StorageError, now_ms};
use aurelius_domain::{
    EmployeeId, ProjectId, TaskId,
    task::TaskStatus,
    views::{LiveRun, TaskView},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

fn parse<T, E: std::error::Error + Send + Sync + 'static>(r: Result<T, E>) -> rusqlite::Result<T> {
    r.map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

#[allow(clippy::too_many_arguments)]
pub fn insert_task(
    tx: &Transaction<'_>,
    id: TaskId,
    project: ProjectId,
    employee: Option<EmployeeId>,
    title: &str,
    brief_blob: &str,
    acceptance_blob: &str,
    status: TaskStatus,
    parent: Option<TaskId>,
) -> Result<(), StorageError> {
    let now = now_ms();
    tx.execute(
        "INSERT INTO tasks(id, project_id, parent_task_id, employee_id, title, brief_blob_id, acceptance_blob_id, status, priority, version, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?9, ?3, ?4, ?5, ?6, ?7, 0, 1, ?8, ?8)",
        params![id.to_string(), project.to_string(), employee.map(|m| m.to_string()), title, brief_blob, acceptance_blob, status.as_str(), now, parent.map(|p| p.to_string())],
    )?;
    Ok(())
}

pub fn insert_dependency(tx: &Transaction<'_>, task: TaskId, requires: TaskId) -> Result<(), StorageError> {
    tx.execute("INSERT INTO task_dependencies(task_id, requires_task_id) VALUES (?1, ?2)", params![task.to_string(), requires.to_string()])?;
    Ok(())
}

pub fn dependency_edges(conn: &Connection) -> Result<Vec<(TaskId, TaskId)>, StorageError> {
    let mut stmt = conn.prepare("SELECT task_id, requires_task_id FROM task_dependencies")?;
    let rows = stmt.query_map([], |r| Ok((parse(TaskId::parse(&r.get::<_, String>(0)?))?, parse(TaskId::parse(&r.get::<_, String>(1)?))?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// (all dependencies succeeded, any dependency failed/cancelled)
pub fn dependency_state(conn: &Connection, task: TaskId) -> Result<(bool, bool), StorageError> {
    let (open, broken): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(t.status <> 'succeeded'), 0), COALESCE(SUM(t.status IN ('failed','cancelled')), 0)
         FROM task_dependencies d JOIN tasks t ON t.id = d.requires_task_id WHERE d.task_id = ?1",
        [task.to_string()],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok((open == 0, broken > 0))
}

pub struct TaskRow {
    pub project_id: ProjectId,
    pub employee_id: Option<EmployeeId>,
    pub title: String,
    pub status: TaskStatus,
    pub version: u64,
    pub goal_key: String,
    pub criteria_key: Option<String>,
    /// The task this one was delegated from.
    pub parent: Option<TaskId>,
}

pub fn load_task(conn: &Connection, id: TaskId) -> Result<Option<TaskRow>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT t.project_id, t.employee_id, t.title, t.status, t.version, gb.storage_key, cb.storage_key, t.parent_task_id
             FROM tasks t JOIN blobs gb ON gb.id = t.brief_blob_id LEFT JOIN blobs cb ON cb.id = t.acceptance_blob_id
             WHERE t.id = ?1",
            [id.to_string()],
            |r| {
                Ok(TaskRow {
                    project_id: parse(ProjectId::parse(&r.get::<_, String>(0)?))?,
                    employee_id: r.get::<_, Option<String>>(1)?.and_then(|m| EmployeeId::parse(&m).ok()),
                    title: r.get(2)?,
                    status: TaskStatus::parse(&r.get::<_, String>(3)?).unwrap_or(TaskStatus::Failed),
                    version: r.get::<_, i64>(4)? as u64,
                    goal_key: r.get(5)?,
                    criteria_key: r.get(6)?,
                    parent: r.get::<_, Option<String>>(7)?.and_then(|p| TaskId::parse(&p).ok()),
                })
            },
        )
        .optional()?)
}

pub fn set_task_status(tx: &Transaction<'_>, id: TaskId, status: TaskStatus, waiting_reason: Option<&str>) -> Result<(), StorageError> {
    let reason = if status == TaskStatus::Waiting { waiting_reason.or(Some("input")) } else { None };
    tx.execute(
        "UPDATE tasks SET status = ?2, waiting_reason = ?3, version = version + 1, updated_at_ms = ?4 WHERE id = ?1",
        params![id.to_string(), status.as_str(), reason, now_ms()],
    )?;
    Ok(())
}

/// A task's status and employee, as stored.
pub fn status_and_employee(conn: &Connection, id: TaskId) -> Result<Option<(TaskStatus, Option<EmployeeId>)>, StorageError> {
    let row: Option<(String, Option<String>)> =
        conn.query_row("SELECT status, employee_id FROM tasks WHERE id = ?1", [id.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    Ok(row.and_then(|(status, employee)| Some((TaskStatus::parse(&status)?, employee.and_then(|m| EmployeeId::parse(&m).ok())))))
}

/// The employee works on one task at a time (`TaskPolicy::sequential_tasks`).
pub fn sequential(conn: &Connection, employee: EmployeeId) -> Result<bool, StorageError> {
    Ok(conn
        .query_row("SELECT COALESCE(json_extract(config_json, '$.task_policy.sequential_tasks'), 0) FROM employees WHERE id = ?1", [employee.to_string()], |r| r.get(0))
        .optional()?
        .unwrap_or(false))
}

/// Queued tasks of the employee that wait for their turn (their job is due or
/// closed, not planned for later), oldest first.
const IN_LINE: &str = "SELECT t.id FROM tasks t JOIN jobs j ON j.id = t.id
     WHERE t.employee_id = ?1 AND t.status = 'queued' AND j.kind = 'task.run'
       AND (j.state IN ('done','leased') OR (j.state = 'ready' AND j.not_before_ms <= ?2))";

/// Whether a task of the employee runs, other than `except`.
pub fn has_running_task(conn: &Connection, employee: EmployeeId, except: Option<TaskId>) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE employee_id = ?1 AND status = 'running' AND (?2 IS NULL OR id <> ?2))",
        params![employee.to_string(), except.map(|t| t.to_string())],
        |r| r.get(0),
    )?)
}

/// Whether an older task of the employee than `task` is in line.
pub fn queued_before(conn: &Connection, employee: EmployeeId, task: TaskId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        &format!("SELECT EXISTS({IN_LINE} AND t.id <> ?3 AND (t.created_at_ms, t.id) < (SELECT created_at_ms, id FROM tasks WHERE id = ?3))"),
        params![employee.to_string(), now_ms(), task.to_string()],
        |r| r.get(0),
    )?)
}

/// The employee's tasks in line, oldest first.
pub fn tasks_in_line(conn: &Connection, employee: EmployeeId) -> Result<Vec<String>, StorageError> {
    let mut stmt = conn.prepare(&format!("{IN_LINE} ORDER BY t.created_at_ms, t.id"))?;
    let ids: Vec<String> = stmt.query_map(params![employee.to_string(), now_ms()], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(ids)
}

/// A task job that was closed (or is about to close after putting its task
/// in line) goes again now.
pub fn wake_job(tx: &Transaction<'_>, task: &str) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE jobs SET state = 'ready', not_before_ms = ?2, lease_owner = NULL, lease_until_ms = NULL, last_error_code = NULL WHERE id = ?1 AND state IN ('done','leased')",
        params![task, now_ms()],
    )?;
    Ok(())
}

/// The report of a finished task.
pub fn set_task_result(tx: &Transaction<'_>, id: TaskId, result_blob: &str) -> Result<(), StorageError> {
    tx.execute("UPDATE tasks SET result_blob_id = ?2, updated_at_ms = ?3 WHERE id = ?1", params![id.to_string(), result_blob, now_ms()])?;
    Ok(())
}

/// Tasks of a project (or all), newest first.
pub fn list_tasks(conn: &Connection, project: Option<ProjectId>, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<TaskView>, StorageError> {
    task_views(conn, project, None, read_blob)
}

/// One task as `list_tasks` shows it, without building the others (and not
/// limited to the newest 500).
pub fn task_view(conn: &Connection, id: TaskId, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Option<TaskView>, StorageError> {
    Ok(task_views(conn, None, Some(id), read_blob)?.pop())
}

fn task_views(conn: &Connection, project: Option<ProjectId>, task: Option<TaskId>, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<TaskView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.project_id, t.employee_id, m.name, t.title, gb.storage_key, cb.storage_key, t.status, t.waiting_reason,
                t.created_at_ms, t.updated_at_ms, rb.storage_key,
                (SELECT COUNT(*) FROM approvals a JOIN runs r ON r.id = a.run_id
                 WHERE r.task_id = t.id AND a.status = 'pending'),
                t.parent_task_id, COALESCE((SELECT pm.name FROM tasks p JOIN employees pm ON pm.id = p.employee_id WHERE p.id = t.parent_task_id),
                                           (SELECT rm.name FROM employees rm WHERE rm.id = t.requested_by),
                                           CASE WHEN t.origin_conversation_id IS NOT NULL THEN 'Project assistant' END),
                -- A deleted (archived) conversation is gone for every client; the task itself stays.
                (SELECT tc.id FROM conversations tc WHERE tc.id = t.conversation_id AND tc.archived_at_ms IS NULL),
                CASE WHEN t.status = 'waiting' AND t.waiting_reason = 'quota' THEN
                    (SELECT MAX(qj.not_before_ms) FROM jobs qj JOIN runs r ON r.id = qj.id
                     WHERE r.task_id = t.id AND qj.state = 'ready' AND qj.last_error_code = 'quota') END,
                j.state, j.not_before_ms, t.created_by_app_key_id, ak.name,
                CASE WHEN t.status IN ('succeeded','failed','cancelled') AND t.conversation_id IS NOT NULL THEN
                    (SELECT CASE
                        WHEN SUM(r.status IN ('starting','running','waiting','cancelling') OR (r.status = 'created' AND rj.state = 'leased')) > 0 THEN 'running'
                        WHEN SUM(r.status = 'created') > 0 THEN 'queued' END
                     FROM runs r LEFT JOIN jobs rj ON rj.id = r.id
                     WHERE r.conversation_id = t.conversation_id AND r.status IN ('created','starting','running','waiting','cancelling')) END
         FROM tasks t JOIN blobs gb ON gb.id = t.brief_blob_id LEFT JOIN blobs cb ON cb.id = t.acceptance_blob_id
         LEFT JOIN blobs rb ON rb.id = t.result_blob_id LEFT JOIN employees m ON m.id = t.employee_id
         LEFT JOIN jobs j ON j.id = t.id AND j.kind = 'task.run'
         LEFT JOIN app_keys ak ON ak.id = t.created_by_app_key_id
         WHERE (?1 IS NULL OR t.project_id = ?1) AND (?2 IS NULL OR t.id = ?2) ORDER BY t.created_at_ms DESC LIMIT 500",
    )?;
    let rows = stmt.query_map([project.map(|p| p.to_string()), task.map(|t| t.to_string())], |r| {
        Ok((
            parse(TaskId::parse(&r.get::<_, String>(0)?))?,
            parse(ProjectId::parse(&r.get::<_, String>(1)?))?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, i64>(9)?,
            r.get::<_, i64>(10)?,
            r.get::<_, Option<String>>(11)?,
            r.get::<_, i64>(12)?,
            r.get::<_, Option<String>>(13)?,
            r.get::<_, Option<String>>(14)?,
            r.get::<_, Option<String>>(15)?,
            r.get::<_, Option<i64>>(16)?,
            r.get::<_, Option<String>>(17)?,
            r.get::<_, Option<i64>>(18)?,
            r.get::<_, Option<String>>(19)?,
            r.get::<_, Option<String>>(20)?,
            r.get::<_, Option<String>>(21)?,
        ))
    })?;
    let edges = dependency_edges(conn)?;
    let now = now_ms();
    let mut out = Vec::new();
    for row in rows {
        let (id, project_id, employee, employee_name, title, goal_key, criteria_key, status, waiting, created, updated, result_key, pending, parent, delegated_by, conversation, waiting_until, job_state, not_before, app_key, app_name, live_run) = row?;
        let status = TaskStatus::parse(&status).unwrap_or(TaskStatus::Failed);
        let employee = employee.and_then(|m| EmployeeId::parse(&m).ok());
        let (waiting, starts_at_ms) = match status {
            TaskStatus::Queued => start_wait(conn, id, employee, job_state.as_deref(), not_before, now)?,
            _ => (waiting, None),
        };
        let criteria: Vec<String> = criteria_key
            .as_deref()
            .and_then(read_blob)
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        out.push(TaskView {
            id,
            project_id,
            employee_id: employee,
            employee_name,
            title,
            goal: read_blob(&goal_key).unwrap_or_default(),
            criteria,
            status,
            waiting_reason: waiting,
            waiting_until_ms: waiting_until,
            starts_at_ms,
            created_at_ms: created,
            updated_at_ms: updated,
            result: result_key.as_deref().and_then(read_blob),
            depends_on: edges.iter().filter(|(t, _)| *t == id).map(|(_, r)| *r).collect(),
            pending_approvals: pending as u32,
            parent_task_id: parent.and_then(|p| TaskId::parse(&p).ok()),
            delegated_by,
            children: children_of(conn, id)?.into_iter().map(|c| c.id).collect(),
            handoffs: handoffs(conn, id)?,
            conversation_id: conversation.and_then(|c| aurelius_domain::ConversationId::parse(&c).ok()),
            run: task_run(conn, id)?,
            attachments: attachments_of(conn, id)?
                .map(|mut a| {
                    aurelius_domain::conversation::with_upload_ids(aurelius_domain::conversation::UploadOwner::Task(id), &mut a.files);
                    a.files
                })
                .unwrap_or_default(),
            schedule_id: schedule_of(conn, id)?,
            function_id: crate::agent_functions::function_of_task(conn, id)?,
            created_by_app: app_name,
            created_by_app_key_id: app_key.and_then(|k| aurelius_domain::AppKeyId::parse(&k).ok()),
            variant_phase: crate::variants::phase(conn, id)?,
            live_run: match live_run.as_deref() {
                Some("running") => Some(LiveRun::Running),
                Some("queued") => Some(LiveRun::Queued),
                _ => None,
            },
        });
    }
    Ok(out)
}

/// Why a queued task does not start now (derived, never stored): `in_line` (a
/// sequential employee works on another task first) or `scheduled` (its job is
/// planned for later: a retry or a parked start, from `starts_at_ms`). None:
/// it is about to start.
pub(crate) fn start_wait(conn: &Connection, task: TaskId, employee: Option<EmployeeId>, job_state: Option<&str>, not_before: Option<i64>, now: i64) -> Result<(Option<String>, Option<i64>), StorageError> {
    if let Some(employee) = employee {
        if sequential(conn, employee)? && (has_running_task(conn, employee, Some(task))? || queued_before(conn, employee, task)?) {
            return Ok((Some("in_line".into()), None));
        }
    }
    Ok(match (job_state, not_before) {
        // Far ahead means "until something else wakes it": no time to show.
        (Some("ready"), Some(at)) if at > now => (Some("scheduled".into()), (at < i64::MAX / 4).then_some(at)),
        _ => (None, None),
    })
}

/// Step runs of a task that are still going (to stop them when it is cancelled).
pub fn active_runs(conn: &Connection, task: TaskId) -> Result<Vec<aurelius_domain::RunId>, StorageError> {
    // The runs of its variants (their own conversations) stop with it too.
    let mut stmt = conn.prepare(
        "SELECT id FROM runs WHERE status IN ('starting','running','waiting')
           AND (task_id = ?1 OR conversation_id IN (SELECT conversation_id FROM task_variants WHERE task_id = ?1))",
    )?;
    let rows = stmt.query_map([task.to_string()], |r| r.get::<_, String>(0))?;
    Ok(rows.filter_map(|r| r.ok()).filter_map(|id| aurelius_domain::RunId::parse(&id).ok()).collect())
}

/// Why the task last started waiting (`stopped`, `not_started`, `escalated`, ...),
/// or `None` when it was resumed since.
pub fn waiting_cause(conn: &rusqlite::Connection, task: TaskId) -> Result<Option<String>, StorageError> {
    let reason: Option<String> = conn
        .query_row(
            "SELECT json_extract(metadata_json, '$.reason') FROM events
             WHERE aggregate_type = 'task' AND aggregate_id = ?1 AND type IN ('task.waiting','task.resumed','task.started')
             ORDER BY seq DESC LIMIT 1",
            [task.to_string()],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(reason)
}

/// A sub-task as its delegating employee follows it up.
pub struct ChildRow {
    pub id: TaskId,
    pub title: String,
    pub employee_name: Option<String>,
    pub status: TaskStatus,
    pub result_key: Option<String>,
}

/// Sub-tasks delegated from `task`, oldest first.
pub fn children_of(conn: &Connection, task: TaskId) -> Result<Vec<ChildRow>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.title, m.name, t.status, rb.storage_key FROM tasks t
         LEFT JOIN employees m ON m.id = t.employee_id LEFT JOIN blobs rb ON rb.id = t.result_blob_id
         WHERE t.parent_task_id = ?1 ORDER BY t.created_at_ms, t.rowid",
    )?;
    let rows = stmt.query_map([task.to_string()], |r| {
        Ok(ChildRow {
            id: parse(TaskId::parse(&r.get::<_, String>(0)?))?,
            title: r.get(1)?,
            employee_name: r.get(2)?,
            status: TaskStatus::parse(&r.get::<_, String>(3)?).unwrap_or(TaskStatus::Failed),
            result_key: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// How many delegations deep this task is (a task the user made: 0).
pub fn depth_of(conn: &Connection, task: TaskId) -> Result<u32, StorageError> {
    Ok(conn.query_row(
        "WITH RECURSIVE up(id, depth) AS (SELECT parent_task_id, 0 FROM tasks WHERE id = ?1
           UNION ALL SELECT t.parent_task_id, up.depth + 1 FROM tasks t JOIN up ON t.id = up.id WHERE up.depth < 32)
         SELECT COUNT(*) FROM up WHERE id IS NOT NULL",
        [task.to_string()],
        |r| r.get::<_, i64>(0),
    )? as u32)
}

/// Give the task to another employee (assignment or hand-over).
pub fn set_task_employee(tx: &Transaction<'_>, task: TaskId, employee: Option<EmployeeId>) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE tasks SET employee_id = ?2, version = version + 1, updated_at_ms = ?3 WHERE id = ?1",
        params![task.to_string(), employee.map(|m| m.to_string()), now_ms()],
    )?;
    Ok(())
}

/// Hand-overs of a task between employees, oldest first.
pub fn handoffs(conn: &Connection, task: TaskId) -> Result<Vec<aurelius_domain::views::TaskHandoff>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT json_extract(metadata_json, '$.from_name'), json_extract(metadata_json, '$.to_name'), json_extract(metadata_json, '$.reason'),
                json_extract(metadata_json, '$.from'), json_extract(metadata_json, '$.to')
         FROM events WHERE aggregate_type = 'task' AND aggregate_id = ?1 AND type = 'task.handed_off' ORDER BY seq",
    )?;
    let rows = stmt.query_map([task.to_string()], |r| {
        Ok(aurelius_domain::views::TaskHandoff {
            from: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
            to: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            reason: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            from_id: r.get::<_, Option<String>>(3)?.and_then(|m| EmployeeId::parse(&m).ok()),
            to_id: r.get::<_, Option<String>>(4)?.and_then(|m| EmployeeId::parse(&m).ok()),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The task's conversation and the employee it belongs to.
pub fn task_conversation(conn: &Connection, task: TaskId) -> Result<Option<(aurelius_domain::ConversationId, Option<EmployeeId>)>, StorageError> {
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT c.id, c.employee_id FROM tasks t JOIN conversations c ON c.id = t.conversation_id WHERE t.id = ?1",
            [task.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(c, m)| Some((aurelius_domain::ConversationId::parse(&c).ok()?, m.and_then(|m| EmployeeId::parse(&m).ok())))))
}

/// The conversation this task was given from (outside a task), and the employee
/// who gave it; none when the assistant of a project conversation gave it.
pub fn set_task_origin(tx: &Transaction<'_>, task: TaskId, conversation: aurelius_domain::ConversationId, employee: Option<EmployeeId>) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE tasks SET origin_conversation_id = ?2, requested_by = ?3 WHERE id = ?1",
        params![task.to_string(), conversation.to_string(), employee.map(|m| m.to_string())],
    )?;
    Ok(())
}

/// The conversation that asked for this task (see `set_task_origin`), if it is still open.
pub fn task_origin(conn: &Connection, task: TaskId) -> Result<Option<aurelius_domain::ConversationId>, StorageError> {
    let row: Option<String> = conn
        .query_row(
            "SELECT c.id FROM tasks t JOIN conversations c ON c.id = t.origin_conversation_id WHERE t.id = ?1 AND c.archived_at_ms IS NULL",
            [task.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(row.and_then(|c| aurelius_domain::ConversationId::parse(&c).ok()))
}

pub fn set_task_conversation(tx: &Transaction<'_>, task: TaskId, conversation: aurelius_domain::ConversationId) -> Result<(), StorageError> {
    tx.execute("UPDATE tasks SET conversation_id = ?2 WHERE id = ?1", params![task.to_string(), conversation.to_string()])?;
    Ok(())
}

/// The task a conversation is about (the latest, if several).
pub fn task_for_conversation(conn: &Connection, conversation: aurelius_domain::ConversationId) -> Result<Option<TaskId>, StorageError> {
    let id: Option<String> = conn
        .query_row("SELECT id FROM tasks WHERE conversation_id = ?1 ORDER BY created_at_ms DESC LIMIT 1", [conversation.to_string()], |r| r.get(0))
        .optional()?;
    Ok(id.and_then(|i| TaskId::parse(&i).ok()))
}

/// The mode, connection, model and effort the task's conversation runs with.
pub fn task_run(conn: &Connection, task: TaskId) -> Result<aurelius_domain::task::RunSettings, StorageError> {
    let json: Option<String> = conn.query_row("SELECT run_json FROM tasks WHERE id = ?1", [task.to_string()], |r| r.get(0)).optional()?.flatten();
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
}

pub fn set_task_run(tx: &Transaction<'_>, task: TaskId, run: &aurelius_domain::task::RunSettings) -> Result<(), StorageError> {
    let json = (!run.is_empty()).then(|| serde_json::to_string(run).unwrap_or_default());
    tx.execute("UPDATE tasks SET run_json = ?2 WHERE id = ?1", params![task.to_string(), json])?;
    Ok(())
}

/// Files attached to a task or schedule: copies in `folder` (absolute), each
/// with its path relative to it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TaskAttachments {
    pub folder: String,
    pub files: Vec<aurelius_domain::views::MessageFileView>,
}

impl TaskAttachments {
    /// The prompt text that tells the model where the files are (absolute paths).
    pub fn prompt(&self) -> Option<String> {
        if self.files.is_empty() {
            return None;
        }
        let root = std::path::Path::new(&self.folder);
        let list: Vec<String> = self.files.iter().map(|f| format!("- {} ({} bytes)", root.join(&f.path).display(), f.byte_length)).collect();
        Some(format!("The user attached these files to the task (read them with your tools):
{}", list.join("
")))
    }
}

pub fn attachments_of(conn: &Connection, task: TaskId) -> Result<Option<TaskAttachments>, StorageError> {
    let json: Option<String> = conn.query_row("SELECT attachments_json FROM tasks WHERE id = ?1", [task.to_string()], |r| r.get(0)).optional()?.flatten();
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

pub fn set_attachments(tx: &Transaction<'_>, task: TaskId, attachments: Option<&TaskAttachments>) -> Result<(), StorageError> {
    let json = attachments.filter(|a| !a.files.is_empty()).map(|a| serde_json::to_string(a)).transpose()?;
    tx.execute("UPDATE tasks SET attachments_json = ?2 WHERE id = ?1", params![task.to_string(), json])?;
    Ok(())
}

/// The recurring task this task was made from.
pub fn schedule_of(conn: &Connection, task: TaskId) -> Result<Option<aurelius_domain::TaskScheduleId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT schedule_id FROM tasks WHERE id = ?1", [task.to_string()], |r| r.get(0)).optional()?.flatten();
    Ok(id.and_then(|i| aurelius_domain::TaskScheduleId::parse(&i).ok()))
}

pub fn set_schedule(tx: &Transaction<'_>, task: TaskId, schedule: aurelius_domain::TaskScheduleId) -> Result<(), StorageError> {
    tx.execute("UPDATE tasks SET schedule_id = ?2 WHERE id = ?1", params![task.to_string(), schedule.to_string()])?;
    Ok(())
}

/// Where uploads for a project's (or one of its employees') tasks go: the
/// project's folder, then the employee's; the same order as a conversation's.
pub fn work_folders(conn: &Connection, project: ProjectId, employee: Option<EmployeeId>) -> Result<Vec<String>, StorageError> {
    let project_folder: Option<String> = conn.query_row("SELECT folder FROM projects WHERE id = ?1", [project.to_string()], |r| r.get(0)).optional()?.flatten();
    let employee_folder: Option<String> = match employee {
        Some(m) => conn.query_row("SELECT folder FROM employees WHERE id = ?1", [m.to_string()], |r| r.get(0)).optional()?.flatten(),
        None => None,
    };
    Ok([project_folder, employee_folder].into_iter().flatten().collect())
}
