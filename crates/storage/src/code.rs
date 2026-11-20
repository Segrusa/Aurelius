//! What the Code tab (ticket #77, `code.get`) knows about a project's branches from the
//! database: the ticket a branch belongs to, who works on it and whether a run of it is
//! live or queued now. The git side is in the daemon (`aurelius_daemon::code`).

use crate::StorageError;
use aurelius_domain::code::{CodeOwner, CodeTicket, ticket_number_of};
use aurelius_domain::ticket::quick_branch;
use aurelius_domain::{ConversationId, EmployeeId, ProjectId, TicketId};
use rusqlite::{Connection, OptionalExtension, params};

/// The run statuses that count as live or queued (as `work_session.list`'s `running`).
const UNFINISHED_RUNS: &str = "('created','starting','running','waiting','cancelling')";

/// What the database says about one branch of a project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchLink {
    /// The ticket it belongs to. Its title is as stored: the caller redacts it.
    pub ticket: Option<CodeTicket>,
    /// The ticket's owner, else the employee of the work session.
    pub owner: Option<CodeOwner>,
    /// A run of its ticket's task tree or of its work session is live or queued.
    pub working: bool,
    /// Its ticket is not on Done (nor archived), or its work session is not deleted: work
    /// on it may still come, even when it has no commit of its own yet. Clean up merged
    /// leaves such a branch alone (a fresh branch at main counts as merged in git).
    pub open: bool,
}

/// The project's folder: `None` when the project does not exist, `Some(None)` when it has none.
pub fn project_folder(conn: &Connection, project: ProjectId) -> Result<Option<Option<String>>, StorageError> {
    Ok(conn.query_row("SELECT folder FROM projects WHERE id = ?1", [project.to_string()], |r| r.get::<_, Option<String>>(0)).optional()?)
}

/// What the database knows of `branch` in `project`. The ticket is the quick task whose
/// branch it is (`quick/ticket-<N>`), else the ticket of this project whose number the
/// name carries (`ticket_number_of`). A work session whose worktree is on the branch
/// links it to its employee (when the ticket has no owner) and to its runs. A session
/// carries no ticket of its own, so its branch name is still read for one.
pub fn branch_link(conn: &Connection, project: ProjectId, branch: &str) -> Result<BranchLink, StorageError> {
    let session = session_on(conn, project, branch)?;
    let quick = match branch.strip_prefix("quick/ticket-").and_then(|n| n.parse::<u32>().ok()) {
        Some(n) if quick_branch(n) == branch => ticket_of(conn, project, n, true)?,
        _ => None,
    };
    let ticket = match quick {
        Some(found) => Some(found),
        None => match ticket_number_of(branch) {
            Some(n) => ticket_of(conn, project, n, false)?,
            None => None,
        },
    };
    let mut link = BranchLink::default();
    if let Some((ticket, owner)) = ticket {
        link.working = ticket_working(conn, ticket.ticket_id)?;
        link.open = ticket_open(conn, ticket.ticket_id)?;
        link.owner = owner;
        link.ticket = Some(ticket);
    }
    if let Some((conversation, owner)) = session {
        link.working = link.working || session_working(conn, conversation)?;
        link.open = link.open || session_open(conn, conversation)?;
        if link.owner.is_none() {
            link.owner = Some(owner);
        }
    }
    Ok(link)
}

/// Whether a live or queued run of a work session of `project` works in the worktree at `path`
/// (a worktree without a branch, which `branch_link` cannot find).
pub fn worktree_working(conn: &Connection, project: ProjectId, path: &str) -> Result<bool, StorageError> {
    let session: Option<String> = conn
        .query_row(
            "SELECT s.conversation_id FROM work_sessions s JOIN conversations c ON c.id = s.conversation_id
             WHERE c.kind = 'session' AND c.project_id = ?1 AND s.worktree = ?2 AND c.archived_at_ms IS NULL
             ORDER BY s.created_at_ms DESC LIMIT 1",
            params![project.to_string(), path],
            |r| r.get(0),
        )
        .optional()?;
    match session {
        Some(id) => session_working(conn, ConversationId::parse(&id).map_err(bad)?),
        None => Ok(false),
    }
}

/// id, number, title, owner id, owner name.
type TicketRow = (String, i64, String, Option<String>, Option<String>);

/// The ticket of `project` with `number` (a quick task only when `quick`), with its owner.
fn ticket_of(conn: &Connection, project: ProjectId, number: u32, quick: bool) -> Result<Option<(CodeTicket, Option<CodeOwner>)>, StorageError> {
    let row: Option<TicketRow> = conn
        .query_row(
            "SELECT k.id, k.number, k.title, e.id, e.name FROM tickets k LEFT JOIN employees e ON e.id = k.owner_employee_id
             WHERE k.project_id = ?1 AND k.number = ?2 AND (?3 = 0 OR k.kind = 'quick')",
            params![project.to_string(), number, quick],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((id, number, title, owner_id, owner_name)) = row else {
        return Ok(None);
    };
    let owner = match owner_id.zip(owner_name) {
        Some((id, name)) => Some(CodeOwner { employee_id: EmployeeId::parse(&id).map_err(bad)?, name }),
        None => None,
    };
    Ok(Some((CodeTicket { ticket_id: TicketId::parse(&id).map_err(bad)?, number: number as u32, title }, owner)))
}

/// The work session of `project` whose worktree is on `branch` (one not deleted first,
/// then the newest), with its employee.
fn session_on(conn: &Connection, project: ProjectId, branch: &str) -> Result<Option<(ConversationId, CodeOwner)>, StorageError> {
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT s.conversation_id, e.id, e.name FROM work_sessions s JOIN conversations c ON c.id = s.conversation_id
             JOIN employees e ON e.id = c.employee_id
             WHERE c.kind = 'session' AND c.project_id = ?1 AND s.branch = ?2
             ORDER BY c.archived_at_ms IS NOT NULL, s.created_at_ms DESC LIMIT 1",
            params![project.to_string(), branch],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((conversation, employee, name)) = row else {
        return Ok(None);
    };
    Ok(Some((ConversationId::parse(&conversation).map_err(bad)?, CodeOwner { employee_id: EmployeeId::parse(&employee).map_err(bad)?, name })))
}

/// The ticket is neither on Done nor archived.
fn ticket_open(conn: &Connection, ticket: TicketId) -> Result<bool, StorageError> {
    Ok(conn
        .query_row(
            "SELECT k.archived_at_ms IS NULL AND b.kind <> 'done' FROM tickets k JOIN board_columns b ON b.id = k.column_id WHERE k.id = ?1",
            [ticket.to_string()],
            |r| r.get::<_, bool>(0),
        )
        .optional()?
        .unwrap_or(false))
}

/// The work session's conversation is not deleted (archived).
fn session_open(conn: &Connection, conversation: ConversationId) -> Result<bool, StorageError> {
    Ok(conn
        .query_row("SELECT archived_at_ms IS NULL FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get::<_, bool>(0))
        .optional()?
        .unwrap_or(false))
}

/// A task of the ticket's tree is queued or running (the sidebar's "working" of a ticket),
/// or a run in a conversation of one of its tasks is live or queued (a follow-up run of a
/// task in Review, as `TaskView::live_run`).
fn ticket_working(conn: &Connection, ticket: TicketId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM tasks t WHERE t.ticket_id = ?1 AND t.status IN ('queued','running'))
                 OR EXISTS(SELECT 1 FROM runs r JOIN tasks t ON t.conversation_id = r.conversation_id
                           WHERE t.ticket_id = ?1 AND r.status IN {UNFINISHED_RUNS})"
        ),
        [ticket.to_string()],
        |r| r.get(0),
    )?)
}

/// A run of the session is live or queued, or its dispatcher still chooses who takes it
/// (as `work_session.list`'s `running`).
fn session_working(conn: &Connection, conversation: ConversationId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM runs r WHERE r.conversation_id = ?1 AND r.status IN {UNFINISHED_RUNS})
                 OR EXISTS(SELECT 1 FROM work_sessions s WHERE s.conversation_id = ?1 AND s.dispatch = 'choosing')"
        ),
        [conversation.to_string()],
        |r| r.get(0),
    )?)
}

fn bad(e: impl std::fmt::Display) -> StorageError {
    StorageError::Rejected(e.to_string())
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
