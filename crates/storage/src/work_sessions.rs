//! Work sessions (`migrations/0043`): a conversation of kind `session` and the row
//! that says who started it and where it works. What is done with them is in
//! `aurelius_application::conversation::work_session`.

use crate::{StorageError, now_ms};
use aurelius_domain::work_session::{SessionDispatch, WorkSessionView};
use aurelius_domain::{ConversationId, EmployeeId, ProjectId, RunId, UserId};
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

fn bad(e: impl std::fmt::Display) -> StorageError {
    StorageError::Rejected(e.to_string())
}
