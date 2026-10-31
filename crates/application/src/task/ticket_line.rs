//! The engine's side of a ticket (impact analysis §3.3–3.4): its main task and
//! the card that follows it. The task's status is the truth for the engine;
//! the column is what the board sees. The card moves in one place, `on_status`,
//! called from `task_line::set_status` for every status change, and only for
//! the main task of a ticket (no parent, `tickets.root_task_id`).
//!
//! | Column      | Main task                                  |
//! |-------------|--------------------------------------------|
//! | Backlog     | none, or cancelled                         |
//! | Todo        | Draft (no owner yet) or Queued             |
//! | Doing       | Running, Waiting (not for a plan), Failed  |
//! | Plan review | Waiting(`plan_review`)                     |
//! | Review      | Review                                     |
//! | Done        | Succeeded (only through `ticket.accept`)   |
//!
//! A failed main task keeps the card in Doing (B6): Start makes a new main task,
//! the old one stays linked through its `ticket_id`. Delegations below the main
//! task (they have a parent) never move the card and stay as they are (§3.5).
//!
//! The board member's writes (`ticket.rs`) move the card first and then call
//! the engine here, so the hook finds the card where it belongs already.

use crate::run::events::{Ev, emit};
use crate::task::task_threads::{self as thread, Speaker};
use crate::task::tasks::task_event;
use aurelius_domain::{
    ActorKind, EmployeeId, EventKind, EventNotice, ProjectId, TaskId, TicketId, UserId,
    task::{TaskStatus, WAITING_PLAN_REVIEW},
    ticket::{Actor, ColumnKind},
};
use aurelius_storage::{StorageError, Store, Tx, blobs, company as company_store, conversations as conv, tasks as task_store, tickets as store};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};

/// The ticket whose main task `task` is (with its project and column), if it is one.
pub(crate) fn root_of(conn: &rusqlite::Connection, task: TaskId) -> Result<Option<(TicketId, ProjectId, ColumnKind)>, StorageError> {
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT k.id, k.project_id, c.kind FROM tickets k JOIN tasks t ON t.id = k.root_task_id JOIN board_columns c ON c.id = k.column_id
             WHERE t.id = ?1 AND t.parent_task_id IS NULL AND k.archived_at_ms IS NULL",
            [task.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    Ok(row.and_then(|(k, p, c)| Some((TicketId::parse(&k).ok()?, ProjectId::parse(&p).ok()?, ColumnKind::parse(&c)?))))
}

pub(crate) fn is_root(conn: &rusqlite::Connection, task: TaskId) -> Result<bool, StorageError> {
    Ok(root_of(conn, task)?.is_some())
}

/// Where the card of a main task in `status` belongs; none: where it is.
fn column_for(status: TaskStatus, waiting_reason: Option<&str>) -> Option<ColumnKind> {
    match status {
        // Waiting for the dispatcher, or for the owner's turn or job (Todo); a
        // resumption passes through Queued too, so it moves nothing.
        TaskStatus::Draft | TaskStatus::Queued => None,
        TaskStatus::Waiting if waiting_reason == Some(WAITING_PLAN_REVIEW) => Some(ColumnKind::PlanReview),
        TaskStatus::Running | TaskStatus::Waiting | TaskStatus::Failed => Some(ColumnKind::Doing),
        TaskStatus::Review => Some(ColumnKind::Review),
        TaskStatus::Succeeded => Some(ColumnKind::Done),
        TaskStatus::Cancelled => Some(ColumnKind::Backlog),
    }
}

/// An event of the ticket by the engine, with the next version of the ticket.
pub(crate) fn ticket_event(tx: &Tx<'_>, ticket: TicketId, project: ProjectId, kind: EventKind, by: Option<Actor>, extra: Value) -> Result<EventNotice, StorageError> {
    let mut metadata = json!({ "by": by });
    if let (Some(m), Value::Object(extra)) = (metadata.as_object_mut(), extra) {
        m.extend(extra);
    }
    let (actor, actor_id) = match by {
        Some(Actor::Employee { employee_id }) => (ActorKind::Employee, Some(employee_id.to_string())),
        Some(Actor::BoardMember { user_id }) => (ActorKind::User, Some(user_id.to_string())),
        None => (ActorKind::System, None),
    };
    let (_, mut notice) = emit(tx, Ev {
        aggregate_type: "ticket",
        aggregate_id: ticket.to_string(),
        event_type: kind.as_str(),
        actor,
        actor_id,
        correlation: ticket.to_string(),
        payload: None,
        metadata,
        dedupe: format!("engine:{ticket}:{}:{}", kind.as_str(), aurelius_domain::EventId::new()),
    })?;
    notice.project_id = Some(project);
    let version = conv::next_version(tx, "ticket", &ticket.to_string())? - 1;
    store::touch(tx, ticket, version)?;
    Ok(notice)
}

/// The engine's own work on a main task is by its employee; a change nobody
/// of the company made (a cancel, an accept) has no `by` here.
fn worked_by(tx: &Tx<'_>, task: TaskId) -> Result<Option<Actor>, StorageError> {
    Ok(task_store::status_and_employee(tx, task)?.and_then(|(_, e)| e).map(|employee_id| Actor::Employee { employee_id }))
}

/// The hook of `task_line::set_status`: the card of a main task follows its status.
pub(crate) fn on_status(tx: &Tx<'_>, task: TaskId, status: TaskStatus, waiting_reason: Option<&str>) -> Result<Vec<EventNotice>, StorageError> {
    let Some((ticket, project, column)) = root_of(tx, task)? else { return Ok(vec![]) };
    let Some(to) = column_for(status, waiting_reason) else { return Ok(vec![]) };
    // Done is closed; only `ticket.accept` puts a card there (it moves it itself first).
    if to == column || column == ColumnKind::Done || to == ColumnKind::Done {
        return Ok(vec![]);
    }
    let by = match status {
        TaskStatus::Cancelled | TaskStatus::Succeeded => None,
        _ => worked_by(tx, task)?,
    };
    let target = company_store::column_of_kind(tx, to)?;
    store::move_to(tx, ticket, target.id, None)?;
    Ok(vec![ticket_event(tx, ticket, project, EventKind::TicketMoved, by, json!({ "from": column, "to": to, "reason": status.as_str() }))?])
}

/// The hook of `task_line::set_employee`: a hand-over of the main task makes the
/// new employee the ticket's owner (§3.5); the column stays.
pub(crate) fn on_employee(tx: &Tx<'_>, task: TaskId, employee: Option<EmployeeId>) -> Result<Vec<EventNotice>, StorageError> {
    let Some((ticket, project, _)) = root_of(tx, task)? else { return Ok(vec![]) };
    let Some(state) = store::state(tx, ticket)? else { return Ok(vec![]) };
    if employee.is_none() || state.owner == employee {
        return Ok(vec![]);
    }
    store::set_owner(tx, ticket, employee)?;
    let by = employee.map(|employee_id| Actor::Employee { employee_id });
    Ok(vec![ticket_event(tx, ticket, project, EventKind::TicketUpdated, by, json!({ "changed": ["owner"], "reason": "handed_over" }))?])
}

/// The main task reported (it is in Review now, the card too): the board member
/// accepts it or sends it back. Nothing else follows until then (§3.3).
pub(crate) fn reported(tx: &Tx<'_>, task: TaskId) -> Result<Vec<EventNotice>, StorageError> {
    let Some((ticket, project, _)) = root_of(tx, task)? else { return Ok(vec![]) };
    let by = worked_by(tx, task)?;
    Ok(vec![ticket_event(tx, ticket, project, EventKind::TicketReview, by, json!({ "task_id": task }))?])
}

/// The ticket was started (it is in Todo): a new main task in Draft, with the
/// ticket's title, description, files, mode and model. With an owner it goes to
/// that employee at once; without one the dispatcher chooses (`triage`).
pub(crate) fn started(tx: &Tx<'_>, st: &Store, ticket: TicketId, user: UserId) -> Result<Vec<EventNotice>, StorageError> {
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let Some(view) = store::view(tx, ticket, &read)? else { return Ok(vec![]) };
    if view.column != ColumnKind::Todo {
        return Ok(vec![]);
    }
    // A main task that still works stays the main task (a second Start is refused before this).
    if view.root_task_status.is_some_and(|s| !s.is_terminal()) {
        return Ok(vec![]);
    }
    let goal = format!(
        "{}\n\n(Ticket #{} on the board, started by {}.)",
        view.description.as_deref().map(str::trim).filter(|d| !d.is_empty()).unwrap_or(&view.title),
        view.number,
        view.created_by_username
    );
    let goal_blob = st.blobs().put(goal.as_bytes(), "text/plain; charset=utf-8")?;
    let criteria_blob = st.blobs().put_json(&Vec::<String>::new())?;
    blobs::register(tx, &goal_blob)?;
    blobs::register(tx, &criteria_blob)?;
    let task = TaskId::new();
    task_store::insert_ticket_task(tx, task, view.project_id, None, &view.title, &goal_blob.id.to_string(), &criteria_blob.id.to_string(), TaskStatus::Draft, None, Some(ticket))?;
    task_store::set_task_run(tx, task, &view.run)?;
    task_store::set_attachments(tx, task, store::attachments_of(tx, ticket)?.as_ref())?;
    store::set_root_task(tx, ticket, Some(task))?;
    let (created_id, mut created) = emit(tx, Ev {
        aggregate_type: "task",
        aggregate_id: task.to_string(),
        event_type: "task.created",
        actor: ActorKind::User,
        actor_id: Some(user.to_string()),
        correlation: ticket.to_string(),
        payload: Some(goal_blob.id.to_string()),
        metadata: json!({ "title": view.title, "employee_id": Value::Null, "ticket_id": ticket, "main_task": true }),
        dedupe: format!("ticket-root:{ticket}:{task}"),
    })?;
    created.project_id = Some(view.project_id);
    let mut notices = vec![created];
    match view.owner_employee_id {
        Some(owner) => notices.extend(dispatched(tx, ticket, owner)?),
        None => notices.extend(crate::task::triage::request(tx, ticket, created_id)?),
    }
    Ok(notices)
}

/// An owner was chosen (`ticket.dispatched`: by a board member or the
/// dispatcher): in Todo with the main task still a draft, it goes to that
/// employee and starts through its task job (→ Doing). In Backlog the choice
/// waits for the start.
pub(crate) fn dispatched(tx: &Tx<'_>, ticket: TicketId, employee: EmployeeId) -> Result<Vec<EventNotice>, StorageError> {
    let Some(state) = store::state(tx, ticket)? else { return Ok(vec![]) };
    let Some(root) = state.root_task_id else { return Ok(vec![]) };
    if state.column != ColumnKind::Todo || state.root_status != Some(TaskStatus::Draft) {
        return Ok(vec![]);
    }
    let mut notices = crate::task::task_line::set_employee(tx, root, Some(employee))?.into_vec();
    notices.extend(crate::task::task_line::set_status(tx, root, TaskStatus::Queued, None)?);
    notices.push(task_event(tx, root, state.project_id, "task.queued", json!({ "reason": "dispatched", "employee_id": employee }))?);
    crate::task::task_runs::wake_task_job(tx, root, true)?;
    Ok(notices)
}

/// `ticket.accept`: the main task in Review succeeds, accepted by the board member.
pub(crate) fn accept(tx: &Tx<'_>, st: &Store, ticket: TicketId, user: UserId) -> Result<Vec<EventNotice>, StorageError> {
    let Some(root) = store::state(tx, ticket)?.and_then(|s| s.root_task_id) else { return Ok(vec![]) };
    if task_store::load_task(tx, root)?.map(|r| r.status) != Some(TaskStatus::Review) {
        return Ok(vec![]);
    }
    let report = report_text(tx, st, root)?;
    crate::task::task_runs::accept(tx, st, root, &report, Some(user))
}

fn report_text(tx: &Tx<'_>, st: &Store, task: TaskId) -> Result<String, StorageError> {
    let key: Option<String> = tx
        .query_row("SELECT b.storage_key FROM tasks t JOIN blobs b ON b.id = t.result_blob_id WHERE t.id = ?1", [task.to_string()], |r| r.get(0))
        .optional()?;
    Ok(key.and_then(|k| st.blobs().read(&k).ok()).and_then(|b| String::from_utf8(b).ok()).unwrap_or_default())
}

/// The board member's name as the speaker in the main task's conversation.
fn username(tx: &Tx<'_>, user: UserId) -> Result<String, StorageError> {
    Ok(aurelius_storage::users::get(tx, &user.to_string())?.map(|u| u.username).unwrap_or_else(|| "A board member".into()))
}

/// The main task goes on in its conversation with `text` from the board member
/// (send back, plan decided): Waiting/Review → Queued → Running and a run of the
/// same session answers it, as `continue_when_done` does.
pub(crate) fn resume_with(tx: &Tx<'_>, st: &Store, task: TaskId, user: UserId, text: &str, reason: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = task_store::load_task(tx, task)? else { return Ok(vec![]) };
    let Some((conversation, _)) = task_store::task_conversation(tx, task)? else { return Ok(vec![]) };
    let from = username(tx, user)?;
    let (message, posted) = thread::post_returning(tx, st, conversation, task, Speaker::Update { kind: "message", from: &from }, text)?;
    let mut notices = vec![posted];
    notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?);
    notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?);
    notices.push(task_event(tx, task, row.project_id, "task.resumed", json!({ "reason": reason }))?);
    notices.extend(crate::task::task_runs::run_for(tx, task, conversation, message)?);
    Ok(notices)
}

/// `ticket.send_back` (B2): the feedback goes to the main task's conversation and
/// its employee works on (the card is in Doing already).
pub(crate) fn send_back(tx: &Tx<'_>, st: &Store, ticket: TicketId, user: UserId, feedback: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(root) = store::state(tx, ticket)?.and_then(|s| s.root_task_id) else { return Ok(vec![]) };
    if task_store::load_task(tx, root)?.map(|r| r.status) != Some(TaskStatus::Review) {
        return Ok(vec![]);
    }
    let text = format!("Sent back by the board: {}\n\nWork on it and report again.", feedback.trim());
    resume_with(tx, st, root, user, &text, "sent_back")
}

/// `ticket.cancel` (and a drag to Backlog): every task of the ticket that has not
/// ended ends, the main task and its delegations, also those of a main task that
/// failed or of an earlier one (B6: Start again); the ended ones stay as they are.
pub(crate) fn cancel(tx: &Tx<'_>, st: &Store, ticket: TicketId, correlation: &str) -> Result<Vec<EventNotice>, StorageError> {
    let mut notices = Vec::new();
    // Oldest first: a task cancels its sub-tasks with it, those are then ended already.
    for task in store::tasks_of_ticket(tx, ticket)? {
        if task_store::load_task(tx, task)?.is_none_or(|r| r.status.is_terminal()) {
            continue;
        }
        notices.extend(crate::task::tasks::cancel_in(tx, st, task, correlation, json!({ "reason": "ticket_cancelled" }))?);
    }
    Ok(notices)
}
