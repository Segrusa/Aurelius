//! Which worktree a conversation works in (ticket #86, names in
//! `aurelius_domain::work_place`): a work session, a ticket's main task, a delegated part
//! of a ticket or a task without a ticket, in a project whose folder is a git repository.
//! None for every other conversation, and for a project without a git folder: those
//! still work in the project's folder (there is no branch to keep apart).

use crate::StorageError;
use aurelius_domain::{ConversationId, TaskId, TicketId, ticket::ColumnKind, work_place as names};
use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// The worktree of a conversation and what it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkPlace {
    /// The project's folder: the repository the worktree belongs to.
    pub repo: PathBuf,
    pub worktree: PathBuf,
    pub branch: String,
    /// The branch a new `branch` starts from; none: the project's main.
    pub base: Option<String>,
    /// The daemon may make the worktree now (a quick task only while its employee works on it).
    pub make: bool,
    pub kind: PlaceKind,
}

/// What a worktree belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaceKind {
    /// A work session. `moves`: its row still says the project's folder (an older session
    /// on its own branch, or in the folder itself); it moves to this worktree at its next run.
    Session { moves: bool },
    /// A ticket's main task (`quick`: a quick task, merged by Aurelius on approval).
    Ticket {
        ticket: TicketId,
        number: u32,
        quick: bool,
    },
    /// A part of a ticket delegated from another task of it.
    Part { ticket: TicketId, number: u32 },
    /// A task without a ticket.
    Task,
}

/// The project's folder when it is the top of a git repository.
fn git_repo(folder: Option<String>) -> Option<PathBuf> {
    folder
        .map(PathBuf::from)
        .filter(|f| f.is_dir() && f.join(".git").exists())
}

/// The worktree `conversation` works in: of its session, else of its task.
pub fn of_conversation(
    conn: &Connection,
    worktrees: &Path,
    conversation: ConversationId,
) -> Result<Option<WorkPlace>, StorageError> {
    if let Some(place) = of_session(conn, worktrees, conversation)? {
        return Ok(Some(place));
    }
    match crate::tasks::task_for_conversation(conn, conversation)? {
        Some(task) => of_task(conn, worktrees, task),
        None => Ok(None),
    }
}

/// The worktree of a work session: the one it has, or where it goes when it has none of
/// its own yet (it worked in the project's folder).
pub fn of_session(
    conn: &Connection,
    worktrees: &Path,
    conversation: ConversationId,
) -> Result<Option<WorkPlace>, StorageError> {
    let Some(session) = crate::work_sessions::of_conversation(conn, conversation)? else {
        return Ok(None);
    };
    let folder: Option<String> = conn
        .query_row(
            "SELECT folder FROM projects WHERE id = ?1",
            [session.project.to_string()],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let Some(repo) = git_repo(folder) else {
        return Ok(None);
    };
    let (worktree, branch, moves) = match session.worktree {
        Some((worktree, branch)) if !session.own_branch && Path::new(&worktree) != repo => {
            (PathBuf::from(worktree), branch, false)
        }
        stored => {
            let branch = stored
                .map(|(_, b)| b)
                .unwrap_or_else(|| aurelius_domain::work_session::branch_of(conversation));
            (worktrees.join(branch.replace('/', "-")), branch, true)
        }
    };
    Ok(Some(WorkPlace {
        repo,
        worktree,
        branch,
        base: None,
        make: true,
        kind: PlaceKind::Session { moves },
    }))
}

/// parent, ticket id, number, kind, column kind, archived, employee name, project folder.
type TaskSql = (Option<String>, Option<String>, Option<i64>, Option<String>, Option<String>, Option<i64>, Option<String>, Option<String>);

/// number, kind, project folder, column kind, archived.
type TicketSql = (i64, String, Option<String>, String, Option<i64>);

/// One task and what decides its place.
struct TaskRow {
    parent: Option<TaskId>,
    ticket: Option<(TicketId, u32, bool, bool)>,
    employee: Option<String>,
    folder: Option<String>,
}

fn task_row(conn: &Connection, task: TaskId) -> Result<Option<TaskRow>, StorageError> {
    let row: Option<TaskSql> = conn
        .query_row(
            "SELECT t.parent_task_id, k.id, k.number, k.kind, c.kind, k.archived_at_ms, e.name, p.folder
             FROM tasks t JOIN projects p ON p.id = t.project_id
             LEFT JOIN tickets k ON k.id = t.ticket_id LEFT JOIN board_columns c ON c.id = k.column_id
             LEFT JOIN employees e ON e.id = t.employee_id
             WHERE t.id = ?1",
            [task.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )
        .optional()?;
    Ok(row.map(
        |(parent, ticket, number, kind, column, archived, employee, folder)| {
            let ticket = match (ticket.and_then(|t| TicketId::parse(&t).ok()), number) {
                (Some(id), Some(n)) => {
                    let quick = kind.as_deref() == Some("quick");
                    // A quick task's worktree is made only while its employee works on it.
                    let working = archived.is_none()
                        && matches!(
                            column.as_deref().and_then(ColumnKind::parse),
                            Some(ColumnKind::Todo | ColumnKind::Doing)
                        );
                    Some((id, n as u32, quick, working))
                }
                _ => None,
            };
            TaskRow {
                parent: parent.and_then(|p| TaskId::parse(&p).ok()),
                ticket,
                employee,
                folder,
            }
        },
    ))
}

/// The worktree of a task: its ticket's (main task), its own branched from the task it
/// was delegated from (a part), or its own from main (no ticket).
pub fn of_task(
    conn: &Connection,
    worktrees: &Path,
    task: TaskId,
) -> Result<Option<WorkPlace>, StorageError> {
    let Some(row) = task_row(conn, task)? else {
        return Ok(None);
    };
    let Some(repo) = git_repo(row.folder.clone()) else {
        return Ok(None);
    };
    let place = match (row.ticket, row.parent) {
        (None, _) => WorkPlace {
            repo,
            worktree: names::task_worktree(worktrees, task),
            branch: names::task_branch(task),
            base: None,
            make: true,
            kind: PlaceKind::Task,
        },
        (Some((ticket, number, quick, working)), None) => {
            root_place(repo, worktrees, ticket, number, quick, working)
        }
        (Some((ticket, number, _, _)), Some(parent)) => WorkPlace {
            worktree: names::part_worktree(worktrees, number, task),
            branch: names::part_branch(number, row.employee.as_deref().unwrap_or_default(), task),
            base: parent_branch(conn, parent)?,
            repo,
            make: true,
            kind: PlaceKind::Part { ticket, number },
        },
    };
    Ok(Some(place))
}

fn root_place(
    repo: PathBuf,
    worktrees: &Path,
    ticket: TicketId,
    number: u32,
    quick: bool,
    working: bool,
) -> WorkPlace {
    WorkPlace {
        repo,
        worktree: names::ticket_worktree(worktrees, number),
        branch: root_branch(number, quick),
        base: None,
        make: !quick || working,
        kind: PlaceKind::Ticket {
            ticket,
            number,
            quick,
        },
    }
}

/// The branch of the task a part was delegated from, so the part starts from its work.
fn parent_branch(conn: &Connection, parent: TaskId) -> Result<Option<String>, StorageError> {
    let Some(row) = task_row(conn, parent)? else {
        return Ok(None);
    };
    Ok(Some(match (row.ticket, row.parent) {
        (Some((_, number, quick, _)), None) => root_branch(number, quick),
        (Some((_, number, _, _)), Some(_)) => {
            names::part_branch(number, row.employee.as_deref().unwrap_or_default(), parent)
        }
        (None, _) => names::task_branch(parent),
    }))
}

fn root_branch(number: u32, quick: bool) -> String {
    if quick {
        aurelius_domain::ticket::quick_branch(number)
    } else {
        names::ticket_branch(number)
    }
}

/// The worktree of a ticket's main task (also for a colleague woken in its discussion).
pub fn of_ticket(
    conn: &Connection,
    worktrees: &Path,
    ticket: TicketId,
) -> Result<Option<WorkPlace>, StorageError> {
    let row: Option<TicketSql> = conn
        .query_row(
            "SELECT k.number, k.kind, p.folder, c.kind, k.archived_at_ms FROM tickets k JOIN projects p ON p.id = k.project_id
             JOIN board_columns c ON c.id = k.column_id WHERE k.id = ?1",
            [ticket.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((number, kind, folder, column, archived)) = row else {
        return Ok(None);
    };
    let Some(repo) = git_repo(folder) else {
        return Ok(None);
    };
    let working = archived.is_none()
        && matches!(
            ColumnKind::parse(&column),
            Some(ColumnKind::Todo | ColumnKind::Doing)
        );
    Ok(Some(root_place(
        repo,
        worktrees,
        ticket,
        number as u32,
        kind == "quick",
        working,
    )))
}

/// The worktrees of a ticket's tasks: its main task's first, then its delegated parts.
/// What is removed when the ticket ends.
pub fn of_ticket_tasks(
    conn: &Connection,
    worktrees: &Path,
    ticket: TicketId,
) -> Result<Vec<WorkPlace>, StorageError> {
    let mut stmt = conn.prepare("SELECT id FROM tasks WHERE ticket_id = ?1 ORDER BY parent_task_id IS NOT NULL, created_at_ms, id")?;
    let ids = stmt
        .query_map([ticket.to_string()], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut places: Vec<WorkPlace> = Vec::new();
    for id in ids.iter().filter_map(|i| TaskId::parse(i).ok()) {
        if let Some(place) = of_task(conn, worktrees, id)?
            && !places.iter().any(|p| p.worktree == place.worktree)
        {
            places.push(place);
        }
    }
    Ok(places)
}

/// A session leaves the project's folder for its own worktree: its row says so from now
/// on (no longer on its own branch in the folder).
pub fn move_session(
    tx: &rusqlite::Transaction<'_>,
    conversation: ConversationId,
    worktree: &Path,
    branch: &str,
) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE work_sessions SET worktree = ?2, branch = ?3, own_branch = 0 WHERE conversation_id = ?1",
        rusqlite::params![conversation.to_string(), worktree.to_string_lossy(), branch],
    )?;
    Ok(())
}
