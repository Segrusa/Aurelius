//! Tickets (migration 18): the cards of the board. A ticket is numbered per
//! company, sits in one column at a position (smallest on top), and links to
//! its tasks through `tasks.ticket_id`; `root_task_id` is the current main task.
//!
//! Versions: the application gives every event of a ticket the next version and
//! stores the last with `touch`; the other writes here leave `version` alone.

use crate::tasks::TaskAttachments;
use crate::{StorageError, now_ms};
use aurelius_domain::{
    BoardColumnId, EmployeeId, ProjectId, TaskId, TicketId, UserId,
    task::{RunSettings, TaskStatus},
    ticket::{BoardColumnCards, ColumnKind, MY_TICKETS_MAX, MyTicketsView, TicketKind, TicketPriority, TicketView},
};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

/// `move_to`: the card to put it above is not in that column.
pub const NOT_IN_COLUMN: &str = "not_in_column";

fn parse<T, E: std::error::Error + Send + Sync + 'static>(r: Result<T, E>) -> rusqlite::Result<T> {
    r.map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

pub struct NewTicket<'a> {
    pub id: TicketId,
    pub project_id: ProjectId,
    pub title: &'a str,
    pub description_blob: Option<&'a str>,
    pub column_id: BoardColumnId,
    pub priority: TicketPriority,
    pub created_by_user: UserId,
    pub owner: Option<EmployeeId>,
    pub run: &'a RunSettings,
    pub labels: &'a [String],
    pub deadline_ms: Option<i64>,
    pub kind: TicketKind,
}

fn json_or_null<T: serde::Serialize>(value: &T, empty: bool) -> Result<Option<String>, StorageError> {
    if empty { Ok(None) } else { Ok(Some(serde_json::to_string(value)?)) }
}

/// Insert a ticket at the bottom of its column with the next number of the
/// company, with its discussion (`discussions::ticket_conversation`); returns
/// the number. Version 1. Its runs use its creator's Claude login (`login_user`).
pub fn insert(tx: &Transaction<'_>, ticket: &NewTicket<'_>) -> Result<u32, StorageError> {
    let number: i64 = tx.query_row("SELECT COALESCE(MAX(number), 0) + 1 FROM tickets", [], |r| r.get(0))?;
    let position = next_position(tx, ticket.column_id)?;
    let now = now_ms();
    let discussion = crate::discussions::insert_ticket_discussion(tx, ticket.project_id, &crate::discussions::ticket_discussion_title(number as u32, ticket.title))?;
    tx.execute(
        "INSERT INTO tickets(id, company_id, project_id, number, title, description_blob_id, column_id, position, priority, created_by_user,
                             owner_employee_id, run_json, labels_json, deadline_ms, version, created_at_ms, updated_at_ms, conversation_id, login_user, kind)
         VALUES (?1, (SELECT id FROM company), ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 1, ?14, ?14, ?15, ?9, ?16)",
        params![
            ticket.id.to_string(),
            ticket.project_id.to_string(),
            number,
            ticket.title,
            ticket.description_blob,
            ticket.column_id.to_string(),
            position,
            ticket.priority.as_str(),
            ticket.created_by_user.to_string(),
            ticket.owner.map(|o| o.to_string()),
            json_or_null(ticket.run, ticket.run.is_empty())?,
            json_or_null(&ticket.labels, ticket.labels.is_empty())?,
            ticket.deadline_ms,
            now,
            discussion.to_string(),
            ticket.kind.as_str(),
        ],
    )?;
    Ok(number as u32)
}

/// Whose Claude login the ticket's runs use (at first its creator).
/// The ticket's design link, if it has one.
pub fn design_url(conn: &Connection, id: TicketId) -> Result<Option<String>, StorageError> {
    Ok(conn.query_row("SELECT design_url FROM tickets WHERE id = ?1", [id.to_string()], |r| r.get(0)).optional()?.flatten())
}

pub fn login_user(conn: &Connection, id: TicketId) -> Result<Option<UserId>, StorageError> {
    let user: Option<String> = conn.query_row("SELECT login_user FROM tickets WHERE id = ?1", [id.to_string()], |r| r.get(0)).optional()?;
    Ok(user.and_then(|u| UserId::parse(&u).ok()))
}

/// "Run on my login": the ticket's runs use the login of `me`, the board
/// member who asks for it, never someone else (D3-3). The only way to change
/// `login_user`; `created_by_user` stays. False when the ticket is unknown or
/// already on that login. The version is left alone (see `touch`).
pub fn use_my_login(tx: &Transaction<'_>, id: TicketId, me: UserId) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE tickets SET login_user = ?2 WHERE id = ?1 AND login_user <> ?2",
        params![id.to_string(), me.to_string()],
    )? == 1)
}

/// The board member responsible for a ticket, as a write or a notification needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TicketResponsible {
    pub user_id: UserId,
    /// Disabled: the ticket counts as having no active responsible.
    pub disabled: bool,
}

/// Make `responsible` the board member responsible for a ticket (none: nobody).
/// False when the ticket is unknown or it already is. The version is left alone
/// (see `touch`).
pub fn set_responsible(tx: &Transaction<'_>, id: TicketId, responsible: Option<UserId>) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE tickets SET responsible_user_id = ?2 WHERE id = ?1 AND responsible_user_id IS NOT ?2",
        params![id.to_string(), responsible.map(|u| u.to_string())],
    )? == 1)
}

/// Who is responsible for a ticket; none when nobody is or the ticket is unknown.
pub fn responsible(conn: &Connection, id: TicketId) -> Result<Option<TicketResponsible>, StorageError> {
    let row: Option<(String, bool)> = conn
        .query_row(
            "SELECT k.responsible_user_id, u.disabled_at_ms IS NOT NULL FROM tickets k JOIN users u ON u.id = k.responsible_user_id WHERE k.id = ?1",
            [id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(user, disabled)| Some(TicketResponsible { user_id: UserId::parse(&user).ok()?, disabled })))
}

/// Whether a task of the ticket (or its dispatcher's job) waits for a Claude login (`needs: login`).
pub fn waits_for_login(conn: &Connection, id: TicketId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM tasks w WHERE w.ticket_id = ?1 AND w.status = 'waiting' AND w.waiting_reason = 'login')
             OR EXISTS (SELECT 1 FROM jobs j WHERE j.kind = 'ticket.triage' AND j.state = 'ready' AND j.last_error_code = 'login'
                        AND json_extract(j.payload_json, '$.ticket_id') = ?1)",
        [id.to_string()],
        |r| r.get(0),
    )?)
}

fn next_position(conn: &Connection, column: BoardColumnId) -> Result<f64, StorageError> {
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM tickets WHERE column_id = ?1 AND archived_at_ms IS NULL",
        [column.to_string()],
        |r| r.get(0),
    )?)
}

/// The commit a quick task was merged into its project's main with (feature `quick_task`).
pub fn set_merge_commit(tx: &Transaction<'_>, id: TicketId, commit: &str) -> Result<(), StorageError> {
    tx.execute("UPDATE tickets SET merge_commit = ?2 WHERE id = ?1", params![id.to_string(), commit])?;
    Ok(())
}

/// The project folder of a ticket's project, if it has one.
pub fn project_folder(conn: &Connection, id: TicketId) -> Result<Option<String>, StorageError> {
    Ok(conn
        .query_row("SELECT p.folder FROM tickets k JOIN projects p ON p.id = k.project_id WHERE k.id = ?1", [id.to_string()], |r| r.get(0))
        .optional()?
        .flatten())
}

/// The quick task a conversation works for: the conversation of its main task (or of
/// a task below it) or its discussion, with the ticket's number. None for every other conversation.
pub fn quick_of_conversation(conn: &Connection, conversation: aurelius_domain::ConversationId) -> Result<Option<(TicketId, u32)>, StorageError> {
    let row: Option<(String, i64)> = conn
        .query_row(
            "SELECT k.id, k.number FROM tickets k
             WHERE k.kind = 'quick' AND (k.conversation_id = ?1
                OR k.id IN (SELECT t.ticket_id FROM tasks t WHERE t.conversation_id = ?1))",
            [conversation.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(id, n)| Some((TicketId::parse(&id).ok()?, n as u32))))
}

/// The ticket whose discussion `conversation` is; none for every other conversation.
pub fn of_discussion(conn: &Connection, conversation: aurelius_domain::ConversationId) -> Result<Option<TicketId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT id FROM tickets WHERE conversation_id = ?1", [conversation.to_string()], |r| r.get(0)).optional()?;
    Ok(id.and_then(|id| TicketId::parse(&id).ok()))
}

/// The conversations the tasks of a ticket work in, with each task's title, oldest task first.
pub fn task_conversations(conn: &Connection, ticket: TicketId) -> Result<Vec<(aurelius_domain::ConversationId, String)>, StorageError> {
    let mut stmt = conn.prepare("SELECT conversation_id, title FROM tasks WHERE ticket_id = ?1 AND conversation_id IS NOT NULL ORDER BY created_at_ms, id")?;
    let rows = stmt.query_map([ticket.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().filter_map(|(c, title)| Some((aurelius_domain::ConversationId::parse(&c).ok()?, title))).collect())
}

/// What a write needs to decide about a ticket.
#[derive(Debug, Clone, PartialEq)]
pub struct TicketState {
    pub id: TicketId,
    pub project_id: ProjectId,
    pub column_id: BoardColumnId,
    pub column: ColumnKind,
    pub version: u64,
    pub owner: Option<EmployeeId>,
    pub root_task_id: Option<TaskId>,
    pub root_status: Option<TaskStatus>,
    pub archived: bool,
    pub kind: TicketKind,
    pub number: u32,
}

pub fn state(conn: &Connection, id: TicketId) -> Result<Option<TicketState>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT k.project_id, k.column_id, c.kind, k.version, k.owner_employee_id, k.root_task_id, t.status, k.archived_at_ms IS NOT NULL,
                    k.kind, k.number
             FROM tickets k JOIN board_columns c ON c.id = k.column_id LEFT JOIN tasks t ON t.id = k.root_task_id
             WHERE k.id = ?1",
            [id.to_string()],
            |r| {
                Ok(TicketState {
                    id,
                    project_id: parse(ProjectId::parse(&r.get::<_, String>(0)?))?,
                    column_id: parse(BoardColumnId::parse(&r.get::<_, String>(1)?))?,
                    column: ColumnKind::parse(&r.get::<_, String>(2)?).unwrap_or(ColumnKind::Backlog),
                    version: r.get::<_, i64>(3)? as u64,
                    owner: r.get::<_, Option<String>>(4)?.and_then(|o| EmployeeId::parse(&o).ok()),
                    root_task_id: r.get::<_, Option<String>>(5)?.and_then(|t| TaskId::parse(&t).ok()),
                    root_status: r.get::<_, Option<String>>(6)?.and_then(|s| TaskStatus::parse(&s)),
                    archived: r.get(7)?,
                    kind: TicketKind::parse(&r.get::<_, String>(8)?).unwrap_or_default(),
                    number: r.get::<_, i64>(9)? as u32,
                })
            },
        )
        .optional()?)
}

/// Put a ticket in `column` right above `before` (none: at the bottom). The
/// cards of that column are numbered again 1..n, so the order stays exact.
/// `Rejected(NOT_IN_COLUMN)` when `before` is not a card of that column.
pub fn move_to(tx: &Transaction<'_>, id: TicketId, column: BoardColumnId, before: Option<TicketId>) -> Result<(), StorageError> {
    let mut stmt = tx.prepare(
        "SELECT id FROM tickets WHERE column_id = ?1 AND id <> ?2 AND archived_at_ms IS NULL ORDER BY position, number",
    )?;
    let mut ids: Vec<String> = stmt.query_map(params![column.to_string(), id.to_string()], |r| r.get(0))?.collect::<Result<_, _>>()?;
    drop(stmt);
    let at = match before {
        None => ids.len(),
        Some(before) => ids.iter().position(|t| *t == before.to_string()).ok_or_else(|| StorageError::Rejected(NOT_IN_COLUMN.into()))?,
    };
    ids.insert(at, id.to_string());
    tx.execute("UPDATE tickets SET column_id = ?2 WHERE id = ?1", params![id.to_string(), column.to_string()])?;
    for (i, ticket) in ids.iter().enumerate() {
        tx.execute("UPDATE tickets SET position = ?2 WHERE id = ?1", params![ticket, (i + 1) as f64])?;
    }
    Ok(())
}

/// Store the ticket's version after its events, and when it changed.
pub fn touch(tx: &Transaction<'_>, id: TicketId, version: u64) -> Result<(), StorageError> {
    tx.execute("UPDATE tickets SET version = ?2, updated_at_ms = ?3 WHERE id = ?1", params![id.to_string(), version as i64, now_ms()])?;
    Ok(())
}

/// The fields of `ticket.update` that are given.
#[derive(Debug, Default)]
pub struct TicketFields<'a> {
    pub title: Option<&'a str>,
    /// `Some(None)`: cleared.
    pub description_blob: Option<Option<&'a str>>,
    pub priority: Option<TicketPriority>,
    pub labels: Option<&'a [String]>,
    pub deadline_ms: Option<Option<i64>>,
    pub owner: Option<Option<EmployeeId>>,
    pub run: Option<&'a RunSettings>,
    /// `Some(None)`: cleared.
    pub design_url: Option<Option<&'a str>>,
}

pub fn update_fields(tx: &Transaction<'_>, id: TicketId, fields: &TicketFields<'_>) -> Result<(), StorageError> {
    let id = id.to_string();
    if let Some(title) = fields.title {
        tx.execute("UPDATE tickets SET title = ?2 WHERE id = ?1", params![id, title])?;
        // Its discussion follows the title.
        tx.execute(
            "UPDATE conversations SET title = '#' || (SELECT number FROM tickets WHERE id = ?1) || ' ' || ?2
             WHERE id = (SELECT conversation_id FROM tickets WHERE id = ?1)",
            params![id, title],
        )?;
    }
    if let Some(blob) = fields.description_blob {
        tx.execute("UPDATE tickets SET description_blob_id = ?2 WHERE id = ?1", params![id, blob])?;
    }
    if let Some(priority) = fields.priority {
        tx.execute("UPDATE tickets SET priority = ?2 WHERE id = ?1", params![id, priority.as_str()])?;
    }
    if let Some(labels) = fields.labels {
        tx.execute("UPDATE tickets SET labels_json = ?2 WHERE id = ?1", params![id, json_or_null(&labels, labels.is_empty())?])?;
    }
    if let Some(deadline) = fields.deadline_ms {
        tx.execute("UPDATE tickets SET deadline_ms = ?2 WHERE id = ?1", params![id, deadline])?;
    }
    if let Some(design) = fields.design_url {
        tx.execute("UPDATE tickets SET design_url = ?2 WHERE id = ?1", params![id, design])?;
    }
    if let Some(owner) = fields.owner {
        tx.execute("UPDATE tickets SET owner_employee_id = ?2 WHERE id = ?1", params![id, owner.map(|o| o.to_string())])?;
    }
    if let Some(run) = fields.run {
        tx.execute("UPDATE tickets SET run_json = ?2 WHERE id = ?1", params![id, json_or_null(run, run.is_empty())?])?;
    }
    Ok(())
}

pub fn set_owner(tx: &Transaction<'_>, id: TicketId, owner: Option<EmployeeId>) -> Result<(), StorageError> {
    update_fields(tx, id, &TicketFields { owner: Some(owner), ..Default::default() })
}

pub fn set_feedback(tx: &Transaction<'_>, id: TicketId, feedback: Option<&str>) -> Result<(), StorageError> {
    tx.execute("UPDATE tickets SET review_feedback = ?2 WHERE id = ?1", params![id.to_string(), feedback])?;
    Ok(())
}

/// The first start is kept; a start again does not change it.
pub fn mark_started(tx: &Transaction<'_>, id: TicketId) -> Result<(), StorageError> {
    tx.execute("UPDATE tickets SET started_at_ms = COALESCE(started_at_ms, ?2) WHERE id = ?1", params![id.to_string(), now_ms()])?;
    Ok(())
}

/// The ticket went to Done now: a new moment, and a new summary to come for the History (B2).
pub fn mark_done(tx: &Transaction<'_>, id: TicketId) -> Result<(), StorageError> {
    tx.execute("UPDATE tickets SET done_at_ms = ?2 WHERE id = ?1", params![id.to_string(), now_ms()])?;
    crate::history::mark_pending(tx, id)
}

pub fn mark_archived(tx: &Transaction<'_>, id: TicketId) -> Result<(), StorageError> {
    tx.execute("UPDATE tickets SET archived_at_ms = ?2 WHERE id = ?1", params![id.to_string(), now_ms()])?;
    Ok(())
}

/// The current main task (the engine sets it when it makes one).
pub fn set_root_task(tx: &Transaction<'_>, id: TicketId, task: Option<TaskId>) -> Result<(), StorageError> {
    tx.execute("UPDATE tickets SET root_task_id = ?2 WHERE id = ?1", params![id.to_string(), task.map(|t| t.to_string())])?;
    Ok(())
}

/// The ticket's latest test sub-task (the engine sets it when the card goes to Testing).
pub fn set_test_task(tx: &Transaction<'_>, id: TicketId, task: Option<TaskId>) -> Result<(), StorageError> {
    tx.execute("UPDATE tickets SET test_task_id = ?2 WHERE id = ?1", params![id.to_string(), task.map(|t| t.to_string())])?;
    Ok(())
}

/// The ticket's latest test sub-task, if it ever had one.
pub fn test_task(conn: &Connection, id: TicketId) -> Result<Option<TaskId>, StorageError> {
    let task: Option<String> = conn.query_row("SELECT test_task_id FROM tickets WHERE id = ?1", [id.to_string()], |r| r.get(0)).optional()?.flatten();
    Ok(task.and_then(|t| TaskId::parse(&t).ok()))
}

/// The ticket whose latest test sub-task `task` is, if it is one (ended or not).
pub fn tested_by(conn: &Connection, task: TaskId) -> Result<Option<TicketId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT id FROM tickets WHERE test_task_id = ?1", [task.to_string()], |r| r.get(0)).optional()?;
    Ok(id.and_then(|i| TicketId::parse(&i).ok()))
}

pub fn attachments_of(conn: &Connection, id: TicketId) -> Result<Option<TaskAttachments>, StorageError> {
    let json: Option<String> = conn.query_row("SELECT attachments_json FROM tickets WHERE id = ?1", [id.to_string()], |r| r.get(0)).optional()?.flatten();
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

pub fn set_attachments(tx: &Transaction<'_>, id: TicketId, attachments: Option<&TaskAttachments>) -> Result<(), StorageError> {
    let json = attachments.filter(|a| !a.files.is_empty()).map(serde_json::to_string).transpose()?;
    tx.execute("UPDATE tickets SET attachments_json = ?2 WHERE id = ?1", params![id.to_string(), json])?;
    Ok(())
}

/// The ticket a task belongs to (its own, inherited from the main task).
pub fn ticket_of_task(conn: &Connection, task: TaskId) -> Result<Option<TicketId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT ticket_id FROM tasks WHERE id = ?1", [task.to_string()], |r| r.get(0)).optional()?.flatten();
    Ok(id.and_then(|i| TicketId::parse(&i).ok()))
}

/// The main task of a ticket, if it has one.
pub fn root_task_of(conn: &Connection, ticket: TicketId) -> Result<Option<TaskId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT root_task_id FROM tickets WHERE id = ?1", [ticket.to_string()], |r| r.get(0)).optional()?.flatten();
    Ok(id.and_then(|i| TaskId::parse(&i).ok()))
}

/// Every task linked to a ticket, oldest first.
pub fn tasks_of_ticket(conn: &Connection, ticket: TicketId) -> Result<Vec<TaskId>, StorageError> {
    let mut stmt = conn.prepare("SELECT id FROM tasks WHERE ticket_id = ?1 ORDER BY created_at_ms, id")?;
    let ids = stmt.query_map([ticket.to_string()], |r| Ok(parse(TaskId::parse(&r.get::<_, String>(0)?))?))?.collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Linked tasks that still work (not ended and not in review), other than `except`.
pub fn working_tasks(conn: &Connection, ticket: TicketId, except: Option<TaskId>) -> Result<u32, StorageError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE ticket_id = ?1 AND (?2 IS NULL OR id <> ?2)
           AND status IN ('queued','running','waiting')",
        params![ticket.to_string(), except.map(|t| t.to_string())],
        |r| r.get::<_, i64>(0),
    )? as u32)
}

/// Whether `employee` works on a ticket: it owns the ticket, or has a task that has not ended in
/// the ticket's task tree (`tasks.ticket_id`; a delegation below the main task carries it too).
pub fn employee_works_on(conn: &Connection, ticket: TicketId, employee: EmployeeId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM tickets WHERE id = ?1 AND owner_employee_id = ?2)
             OR EXISTS (SELECT 1 FROM tasks WHERE ticket_id = ?1 AND employee_id = ?2 AND status NOT IN ('succeeded','failed','cancelled'))",
        params![ticket.to_string(), employee.to_string()],
        |r| r.get::<_, bool>(0),
    )?)
}

/// The ticket `employee` already made in this `run` and `project` with the same title (any case),
/// as (id, number): a repeated `create_ticket` call finds it instead of making a second one.
pub fn created_in_run(conn: &Connection, run: aurelius_domain::RunId, employee: EmployeeId, project: ProjectId, title: &str) -> Result<Option<(TicketId, u32)>, StorageError> {
    let row: Option<(String, i64)> = conn
        .query_row(
            "SELECT k.id, k.number FROM events v JOIN tickets k ON k.id = v.aggregate_id
             WHERE v.aggregate_type = 'ticket' AND v.type = 'ticket.created' AND v.actor_id = ?2
               AND json_extract(v.metadata_json, '$.run_id') = ?1 AND lower(k.title) = lower(?3) AND k.project_id = ?4
             ORDER BY v.seq LIMIT 1",
            params![run.to_string(), employee.to_string(), title, project.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(id, number)| Some((TicketId::parse(&id).ok()?, number as u32))))
}

/// What `list` filters on; every given filter applies.
#[derive(Debug, Default, Clone)]
pub struct TicketFilter {
    pub project: Option<ProjectId>,
    pub column: Option<ColumnKind>,
    /// Title, description or `#number`, case-insensitive.
    pub query: Option<String>,
    pub ticket: Option<TicketId>,
    pub include_archived: bool,
}

const SELECT: &str = "SELECT k.id, k.number, k.project_id, p.name, k.title, db.storage_key, k.column_id, c.kind, k.position, k.priority,
        k.created_by_user, u.username, k.owner_employee_id, e.name, k.run_json, k.labels_json, k.deadline_ms, k.root_task_id, t.status,
        k.review_feedback, k.version, k.created_at_ms, k.updated_at_ms, k.started_at_ms, k.done_at_ms, k.archived_at_ms, k.attachments_json, k.conversation_id,
        k.login_user, lu.username,
        (EXISTS (SELECT 1 FROM tasks w WHERE w.ticket_id = k.id AND w.status = 'waiting' AND w.waiting_reason = 'login')
         OR EXISTS (SELECT 1 FROM jobs j WHERE j.kind = 'ticket.triage' AND j.state = 'ready' AND j.last_error_code = 'login'
                    AND json_extract(j.payload_json, '$.ticket_id') = k.id)),
        k.responsible_user_id, ru.username, COALESCE(ru.disabled_at_ms IS NOT NULL, 0), k.design_url,
        k.kind, k.merge_commit
     FROM tickets k
     JOIN projects p ON p.id = k.project_id
     JOIN board_columns c ON c.id = k.column_id
     JOIN users u ON u.id = k.created_by_user
     JOIN users lu ON lu.id = k.login_user
     LEFT JOIN users ru ON ru.id = k.responsible_user_id
     LEFT JOIN employees e ON e.id = k.owner_employee_id
     LEFT JOIN tasks t ON t.id = k.root_task_id
     LEFT JOIN blobs db ON db.id = k.description_blob_id
     WHERE (?1 IS NULL OR k.project_id = ?1) AND (?2 IS NULL OR c.kind = ?2) AND (?3 IS NULL OR k.id = ?3)
       AND (?4 OR k.archived_at_ms IS NULL)";

/// A row of `SELECT` as a ticket, without its description, with that description's storage key.
fn ticket_row(r: &Row<'_>) -> rusqlite::Result<(TicketView, Option<String>)> {
    let id = parse(TicketId::parse(&r.get::<_, String>(0)?))?;
    let description: Option<String> = r.get(5)?;
    let run: Option<String> = r.get(14)?;
    let labels: Option<String> = r.get(15)?;
    let attachments: Option<String> = r.get(26)?;
    Ok((
        TicketView {
            id,
            number: r.get::<_, i64>(1)? as u32,
            project_id: parse(ProjectId::parse(&r.get::<_, String>(2)?))?,
            project_name: r.get(3)?,
            title: r.get(4)?,
            description: None,
            column_id: parse(BoardColumnId::parse(&r.get::<_, String>(6)?))?,
            column: ColumnKind::parse(&r.get::<_, String>(7)?).unwrap_or(ColumnKind::Backlog),
            position: r.get(8)?,
            priority: TicketPriority::parse(&r.get::<_, String>(9)?).unwrap_or_default(),
            created_by_user: parse(UserId::parse(&r.get::<_, String>(10)?))?,
            created_by_username: r.get(11)?,
            owner_employee_id: r.get::<_, Option<String>>(12)?.and_then(|o| EmployeeId::parse(&o).ok()),
            owner_name: r.get(13)?,
            run: run.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default(),
            labels: labels.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default(),
            attachments: attachments
                .and_then(|j| serde_json::from_str::<TaskAttachments>(&j).ok())
                .map(|mut a| {
                    aurelius_domain::conversation::with_upload_ids(aurelius_domain::conversation::UploadOwner::Ticket(id), &mut a.files);
                    a.files
                })
                .unwrap_or_default(),
            deadline_ms: r.get(16)?,
            root_task_id: r.get::<_, Option<String>>(17)?.and_then(|t| TaskId::parse(&t).ok()),
            root_task_status: r.get::<_, Option<String>>(18)?.and_then(|s| TaskStatus::parse(&s)),
            review_feedback: r.get(19)?,
            version: r.get::<_, i64>(20)? as u64,
            created_at_ms: r.get(21)?,
            updated_at_ms: r.get(22)?,
            started_at_ms: r.get(23)?,
            done_at_ms: r.get(24)?,
            archived_at_ms: r.get(25)?,
            conversation_id: r.get::<_, Option<String>>(27)?.and_then(|c| aurelius_domain::ConversationId::parse(&c).ok()),
            login_user: parse(UserId::parse(&r.get::<_, String>(28)?))?,
            login_username: r.get(29)?,
            // A task of it (or its dispatcher) waits for `login_user`'s Claude login (MU3-12).
            needs: r.get::<_, bool>(30)?.then_some(aurelius_domain::ticket::TicketNeed::Login),
            responsible_user_id: r.get::<_, Option<String>>(31)?.and_then(|u| UserId::parse(&u).ok()),
            responsible_username: r.get(32)?,
            responsible_disabled: r.get(33)?,
            design_url: r.get(34)?,
            // Left out for a normal ticket, so its view stays as it was (feature `quick_task`).
            kind: Some(r.get::<_, String>(35)?).filter(|k| k.as_str() != TicketKind::Normal.as_str()),
            merge_commit: r.get(36)?,
        },
        description,
    ))
}

/// Tickets in board order (column, then position).
pub fn list(conn: &Connection, filter: &TicketFilter, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<TicketView>, StorageError> {
    let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY c.position, k.position, k.number"))?;
    let rows = stmt.query_map(
        params![filter.project.map(|p| p.to_string()), filter.column.map(|c| c.as_str()), filter.ticket.map(|t| t.to_string()), filter.include_archived],
        ticket_row,
    )?;
    let needle = filter.query.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(str::to_lowercase);
    let mut out = Vec::new();
    for row in rows {
        let (mut ticket, key) = row?;
        ticket.description = key.as_deref().and_then(read_blob).filter(|d| !d.is_empty());
        if let Some(needle) = &needle {
            let number = needle.strip_prefix('#').and_then(|n| n.parse::<u32>().ok());
            let hit = number == Some(ticket.number)
                || ticket.title.to_lowercase().contains(needle.as_str())
                || ticket.description.as_deref().is_some_and(|d| d.to_lowercase().contains(needle.as_str()));
            if !hit {
                continue;
            }
        }
        out.push(ticket);
    }
    Ok(out)
}

/// `ticket.my_tickets`: the tickets of `viewer`, whom the daemon takes from the
/// session. Those they are responsible for, made or follow, and the open ones on
/// their login or without an active responsible. Done and archived tickets only
/// with `include_done`, and then not just for lacking a responsible. Highest
/// priority first, then the latest update, at most `MY_TICKETS_MAX`.
pub fn my_tickets(conn: &Connection, viewer: UserId, project: Option<ProjectId>, include_done: bool, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<MyTicketsView, StorageError> {
    // ?4 of `SELECT` takes the archived tickets along.
    let mut stmt = conn.prepare(&format!(
        "{SELECT}
           AND (?4 OR c.kind <> 'done')
           AND (k.responsible_user_id = ?5 OR k.created_by_user = ?5
                OR EXISTS (SELECT 1 FROM ticket_followers f WHERE f.ticket_id = k.id AND f.user_id = ?5)
                OR (c.kind <> 'done' AND k.archived_at_ms IS NULL
                    AND (k.login_user = ?5 OR k.responsible_user_id IS NULL OR ru.disabled_at_ms IS NOT NULL)))
         ORDER BY CASE k.priority WHEN 'urgent' THEN 0 WHEN 'high' THEN 1 WHEN 'normal' THEN 2 ELSE 3 END, k.updated_at_ms DESC, k.number DESC
         LIMIT ?6"
    ))?;
    let rows = stmt.query_map(
        params![project.map(|p| p.to_string()), None::<String>, None::<String>, include_done, viewer.to_string(), MY_TICKETS_MAX as i64],
        ticket_row,
    )?;
    let mut tickets = Vec::new();
    for row in rows {
        let (mut ticket, key) = row?;
        ticket.description = key.as_deref().and_then(read_blob).filter(|d| !d.is_empty());
        tickets.push(ticket);
    }
    let followed = crate::followers::following(conn, viewer)?;
    let following = tickets.iter().map(|t| t.id).filter(|t| followed.contains(t)).collect();
    Ok(MyTicketsView { tickets, following })
}

pub fn view(conn: &Connection, id: TicketId, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Option<TicketView>, StorageError> {
    Ok(list(conn, &TicketFilter { ticket: Some(id), include_archived: true, ..Default::default() }, read_blob)?.pop())
}

/// The board: every column left to right with its cards (not archived), for
/// one project or all (B4).
pub fn board(conn: &Connection, project: Option<ProjectId>, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<BoardColumnCards>, StorageError> {
    let mut tickets = list(conn, &TicketFilter { project, ..Default::default() }, read_blob)?;
    Ok(crate::company::columns(conn)?
        .into_iter()
        .map(|column| {
            let (mine, rest): (Vec<TicketView>, Vec<TicketView>) = std::mem::take(&mut tickets).into_iter().partition(|t| t.column_id == column.id);
            tickets = rest;
            BoardColumnCards { column, tickets: mine }
        })
        .collect())
}
