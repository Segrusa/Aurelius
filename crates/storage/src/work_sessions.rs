//! Work sessions (`migrations/0043`): a conversation of kind `session` and the row
//! that says who started it and where it works. What is done with them is in
//! `aurelius_application::conversation::work_session`.

use crate::{StorageError, now_ms};
use aurelius_domain::work_session::WorkSessionView;
use aurelius_domain::{ConversationId, EmployeeId, ProjectId, UserId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// Where a session works besides the project: its own worktree and branch, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub project: ProjectId,
    pub started_by: UserId,
    /// `(worktree folder, branch)`; none: the project's folder itself.
    pub worktree: Option<(String, String)>,
}

/// Make the session's conversation and its row.
pub fn insert(
    tx: &Transaction<'_>,
    id: ConversationId,
    project: ProjectId,
    employee: EmployeeId,
    title: &str,
    started_by: UserId,
    worktree: Option<(&str, &str)>,
) -> Result<(), StorageError> {
    let now = now_ms();
    tx.execute(
        "INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES (?1, 'session', ?2, ?3, ?4, 1, ?5)",
        params![id.to_string(), project.to_string(), employee.to_string(), title, now],
    )?;
    tx.execute(
        "INSERT INTO work_sessions(conversation_id, started_by, worktree, branch, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id.to_string(), started_by.to_string(), worktree.map(|w| w.0), worktree.map(|w| w.1), now],
    )?;
    Ok(())
}

/// The session a conversation is, if it is one.
pub fn of_conversation(conn: &Connection, conversation: ConversationId) -> Result<Option<Session>, StorageError> {
    let row: Option<(String, String, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT c.project_id, s.started_by, s.worktree, s.branch FROM work_sessions s JOIN conversations c ON c.id = s.conversation_id
             WHERE s.conversation_id = ?1 AND c.kind = 'session'",
            [conversation.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((project, started_by, worktree, branch)) = row else { return Ok(None) };
    Ok(Some(Session {
        project: ProjectId::parse(&project).map_err(bad)?,
        started_by: UserId::parse(&started_by).map_err(bad)?,
        worktree: worktree.zip(branch),
    }))
}

/// The sessions `user` started, the latest activity first; `project`: only those of it.
pub fn list(conn: &Connection, user: UserId, project: Option<ProjectId>) -> Result<Vec<WorkSessionView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, p.id, p.name, e.id, e.name, c.title, s.worktree, s.branch, s.created_at_ms,
                COALESCE((SELECT MAX(m.created_at_ms) FROM messages m WHERE m.conversation_id = c.id), s.created_at_ms) AS last,
                EXISTS(SELECT 1 FROM runs r WHERE r.conversation_id = c.id AND r.status IN ('created','starting','running','waiting','cancelling'))
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
        ))
    })?;
    rows.map(|row| {
        let (conversation, project, project_name, employee, employee_name, title, worktree, branch, created_at_ms, last_activity_ms, running) = row?;
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
            running,
        })
    })
    .collect()
}

fn bad(e: impl std::fmt::Display) -> StorageError {
    StorageError::Rejected(e.to_string())
}
