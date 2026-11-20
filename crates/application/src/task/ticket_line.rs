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
//! | Testing     | Review, while the tester's test task runs  |
//! | Review      | Review                                     |
//! | Done        | Succeeded (only through `ticket.accept`)   |
//!
//! A main task that reports goes to Testing instead of Review when the company
//! has an active tester (`testing`); the tester's verdict moves the card on.
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
    ActorKind, EmployeeId, EventKind, EventNotice, ProjectId, RunId, TaskId, TicketId, UserId,
    task::{TaskStatus, WAITING_PLAN_REVIEW},
    ticket::{Actor, ColumnKind},
    views::{SystemLine, SystemLineKind, system_action},
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
        // Waiting for the Claude login of its board member: it stays where it is (Todo or Doing, `needs: login`).
        TaskStatus::Waiting if waiting_reason == Some(aurelius_domain::task::WAITING_LOGIN) => None,
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

/// The name of a column on this board ("Plan review", or what the board calls it).
fn column_name(tx: &Tx<'_>, kind: ColumnKind) -> Result<String, StorageError> {
    Ok(company_store::column_of_kind(tx, kind)?.name)
}

/// The system line of a column change in the ticket's discussion (MU2-03): who
/// moved it, from → to, and what it was when it was more than a move
/// (`what`: "started it", "approved the plan", …). One line per change.
pub(crate) fn column_line(tx: &Tx<'_>, ticket: TicketId, by: Option<Actor>, from: ColumnKind, to: ColumnKind, what: Option<&str>) -> Result<Vec<EventNotice>, StorageError> {
    column_line_with(tx, ticket, by, from, to, what, None)
}

/// `column_line` with the action of the decision it was (`system_action`, feature `ticket_channels`).
pub(crate) fn column_line_with(tx: &Tx<'_>, ticket: TicketId, by: Option<Actor>, from: ColumnKind, to: ColumnKind, what: Option<&str>, action: Option<&str>) -> Result<Vec<EventNotice>, StorageError> {
    let name = crate::conversation::discussion::actor_name(tx, by)?;
    let (from_name, to_name) = (column_name(tx, from)?, column_name(tx, to)?);
    let text = match what {
        Some(what) => format!("{name} {what} — the ticket moved from {from_name} to {to_name}"),
        None => format!("{name} moved the ticket from {from_name} to {to_name}"),
    };
    let mut line = SystemLine::new(SystemLineKind::Moved);
    (line.by, line.from, line.to) = (Some(name), Some(from_name), Some(to_name));
    line.action = action.map(str::to_owned);
    crate::conversation::discussion::ticket_line(tx, ticket, line, &text)
}

/// The system line of a change of Responsible (US-A1): on `Moved` with its action,
/// so an older client shows the text and never reads a new `SystemLineKind` (R1).
pub(crate) fn responsible_line(tx: &Tx<'_>, ticket: TicketId, by: Actor, before: Option<UserId>, after: Option<UserId>) -> Result<Vec<EventNotice>, StorageError> {
    let board_member = |user: Option<UserId>| user.map(|user_id| crate::conversation::discussion::actor_name(tx, Some(Actor::BoardMember { user_id }))).transpose();
    let name = crate::conversation::discussion::actor_name(tx, Some(by))?;
    let (from, to) = (board_member(before)?, board_member(after)?);
    let text = match (&from, &to) {
        (Some(from), Some(to)) => format!("{name} changed responsible: {from} → {to}"),
        (None, Some(to)) => format!("{name} made {to} responsible"),
        (Some(from), None) => format!("{name} removed {from} as responsible"),
        (None, None) => format!("{name} changed who is responsible"),
    };
    let mut line = SystemLine::new(SystemLineKind::Moved).with_action(system_action::RESPONSIBLE);
    (line.by, line.from, line.to) = (Some(name), from, to);
    crate::conversation::discussion::ticket_line(tx, ticket, line, &text)
}

/// The system line of an owner chosen (`ticket.dispatched`) by a board member or
/// the dispatcher, with the reason the dispatcher gave (MU2-03): a link to the main task.
pub(crate) fn owner_line(tx: &Tx<'_>, ticket: TicketId, by: Option<Actor>, owner: EmployeeId, reason: Option<&str>) -> Result<Vec<EventNotice>, StorageError> {
    let name = crate::conversation::discussion::actor_name(tx, by)?;
    let owner_name = crate::conversation::discussion::employee_name(tx, owner)?;
    let reason = reason.map(str::trim).filter(|r| !r.is_empty());
    let text = match reason {
        Some(reason) => format!("{name} made {owner_name} the owner of this ticket. Reason: {reason}"),
        None => format!("{name} made {owner_name} the owner of this ticket"),
    };
    let mut line = SystemLine::new(SystemLineKind::Delegated);
    (line.from, line.to, line.by) = (Some(name), Some(owner_name), None);
    line.task_id = store::state(tx, ticket)?.and_then(|s| s.root_task_id);
    crate::conversation::discussion::ticket_line(tx, ticket, line, &text)
}

/// The ticket a task works for (its main task or a delegation below it), if any.
pub(crate) fn ticket_of(conn: &rusqlite::Connection, task: TaskId) -> Result<Option<TicketId>, StorageError> {
    let ticket: Option<Option<String>> = conn.query_row("SELECT ticket_id FROM tasks WHERE id = ?1", [task.to_string()], |r| r.get(0)).optional()?;
    Ok(ticket.flatten().and_then(|t| TicketId::parse(&t).ok()))
}

/// A task given on a ticket (MU2-03), below its main task or from a conversation: who gave what to whom, with a link to the new task.
pub(crate) fn delegated_line(tx: &Tx<'_>, child: TaskId, from: &str, to: &str, title: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(ticket) = ticket_of(tx, child)? else { return Ok(vec![]) };
    let mut line = SystemLine::new(SystemLineKind::Delegated);
    (line.from, line.to, line.task_id) = (Some(from.to_owned()), Some(to.to_owned()), Some(child));
    crate::conversation::discussion::ticket_line(tx, ticket, line, &format!("{from} gave the task “{title}” to {to}"))
}

/// How much of a sub-task's report its line in the discussion shows: a short summary, not the report.
pub const REPORT_LINE_CHARS: usize = 160;

/// A sub-task below a ticket reported or failed (MU2-03): a short summary and a
/// link to the task (its report is there). Nothing for a main task (it has its own lines).
pub(crate) fn sub_task_line(tx: &Tx<'_>, task: TaskId, who: &str, title: &str, text: &str, succeeded: bool) -> Result<Vec<EventNotice>, StorageError> {
    let Some(ticket) = ticket_of(tx, task)? else { return Ok(vec![]) };
    if is_root(tx, task)? || store::state(tx, ticket)?.is_some_and(|s| s.root_task_id == Some(task)) {
        return Ok(vec![]);
    }
    let gist = report_gist(text, title);
    let text = match (succeeded, gist.is_empty()) {
        (true, true) => format!("{who} finished the task “{title}”"),
        (true, false) => format!("{who} finished the task “{title}”. Report: {gist}"),
        (false, true) => format!("{who} could not finish the task “{title}”"),
        (false, false) => format!("{who} could not finish the task “{title}”. Reason: {gist}"),
    };
    let mut line = SystemLine::new(SystemLineKind::Reported);
    (line.from, line.task_id) = (Some(who.to_owned()), Some(task));
    crate::conversation::discussion::ticket_line(tx, ticket, line, &text)
}

/// The first words of a report, on one line: without its title heading, at most `REPORT_LINE_CHARS`.
fn report_gist(text: &str, title: &str) -> String {
    let body: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).filter(|l| l.trim_start_matches('#').trim() != title.trim() || !l.starts_with('#')).collect();
    let flat = body.join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= REPORT_LINE_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(REPORT_LINE_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// The engine's own work on a main task is by its employee; a change nobody
/// of the company made (a cancel, an accept) has no `by` here.
fn worked_by(tx: &Tx<'_>, task: TaskId) -> Result<Option<Actor>, StorageError> {
    Ok(task_store::status_and_employee(tx, task)?.and_then(|(_, e)| e).map(|employee_id| Actor::Employee { employee_id }))
}

/// The hook of `task_line::set_status`: the card of a main task follows its status.
pub(crate) fn on_status(tx: &Tx<'_>, task: TaskId, status: TaskStatus, waiting_reason: Option<&str>) -> Result<Vec<EventNotice>, StorageError> {
    // Not a main task: a ticket's test task that ended may still have to move its card.
    let Some((ticket, project, column)) = root_of(tx, task)? else { return crate::task::testing::on_ended(tx, task, status) };
    let Some(to) = column_for(status, waiting_reason) else { return Ok(vec![]) };
    // A report goes to Testing first when the company has a tester; a quick task's never (ST-03 AC1).
    let to = if to == ColumnKind::Review { crate::task::testing::column_after_report(tx, ticket)? } else { to };
    if status == TaskStatus::Failed && to == column {
        return failed_in_place(tx, task, ticket);
    }
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
    let mut notices = vec![ticket_event(tx, ticket, project, EventKind::TicketMoved, by, json!({ "from": column, "to": to, "reason": status.as_str() }))?];
    // Plan review and Review get the line of the plan or the report instead (`plan_review`, `reported`).
    let what = match (status, column) {
        (TaskStatus::Failed, _) => Some("could not finish the ticket's main task"),
        (TaskStatus::Running, ColumnKind::Todo) => Some("started working on the ticket"),
        (TaskStatus::Running, ColumnKind::Testing) => Some("is working on the ticket again after the failed tests"),
        (TaskStatus::Review, _) => Some("reported"),
        _ => None,
    };
    if !matches!(to, ColumnKind::PlanReview | ColumnKind::Review) {
        notices.extend(column_line(tx, ticket, by, column, to, what)?);
    }
    Ok(notices)
}

/// A failed main task whose card stays where it is (Doing) still says so in
/// the discussion (MU2-03): Start makes a new main task (B6).
fn failed_in_place(tx: &Tx<'_>, task: TaskId, ticket: TicketId) -> Result<Vec<EventNotice>, StorageError> {
    let name = crate::conversation::discussion::actor_name(tx, worked_by(tx, task)?)?;
    let mut line = SystemLine::new(SystemLineKind::Reported);
    (line.from, line.task_id) = (Some(name.clone()), Some(task));
    crate::conversation::discussion::ticket_line(tx, ticket, line, &format!("{name} could not finish the ticket's main task. Press Start to begin a new attempt."))
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
    let mut notices = vec![ticket_event(tx, ticket, project, EventKind::TicketUpdated, by, json!({ "changed": ["owner"], "reason": "handed_over" }))?];
    // A hand-over of the main task (MU2-03): from the owner before to the new one.
    if let (Some(before), Some(after)) = (state.owner, employee) {
        let (from, to) = (crate::conversation::discussion::employee_name(tx, before)?, crate::conversation::discussion::employee_name(tx, after)?);
        let mut line = SystemLine::new(SystemLineKind::Delegated);
        (line.from, line.to, line.task_id) = (Some(from.clone()), Some(to.clone()), Some(task));
        notices.extend(crate::conversation::discussion::ticket_line(tx, ticket, line, &format!("{from} handed the ticket over to {to}, who is now the owner"))?);
    }
    notices.extend(crate::conversation::ticket_channel::sync(tx, ticket, by)?);
    Ok(notices)
}

/// The main task reported (it is in Review now, the card too): the board member
/// accepts it or sends it back. Nothing else follows until then (§3.3). With a
/// tester the card is in Testing instead, and the tester tests it first (`testing`).
pub(crate) fn reported(tx: &Tx<'_>, st: &Store, task: TaskId) -> Result<Vec<EventNotice>, StorageError> {
    let Some((ticket, project, column)) = root_of(tx, task)? else { return Ok(vec![]) };
    if column == ColumnKind::Testing {
        let report = report_text(tx, st, task)?;
        return crate::task::testing::start(tx, st, task, ticket, project, &report);
    }
    review_lines(tx, ticket, project, task)
}

/// A run of the main task in Review ended: it answered a message about the report
/// (from the discussion or the task's own conversation). The answer goes in the
/// discussion, the report stays what the board member decides on, and nothing
/// says again that the work is ready. `answer` is the run's text, or what the board
/// member must know when there is none. `None`: the card is not in Review (Testing).
pub(crate) fn answered_in_review(tx: &Tx<'_>, st: &Store, task: TaskId, run: RunId, answer: &str) -> Result<Option<Vec<EventNotice>>, StorageError> {
    let Some((ticket, _, ColumnKind::Review)) = root_of(tx, task)? else { return Ok(None) };
    let mut notices = crate::conversation::discussion::owner_answer(tx, st, ticket, run, answer)?;
    if let Some(conversation) = aurelius_storage::conversations::run_conversation(tx, run)? {
        notices.extend(crate::task::task_runs::live_run_notice(tx, conversation)?);
    }
    Ok(Some(notices))
}

/// The card of the main task `root` is in Review: `ticket.review` and the
/// Review line (MU2-03), a card with the main task for a board member.
pub(crate) fn review_lines(tx: &Tx<'_>, ticket: TicketId, project: ProjectId, root: TaskId) -> Result<Vec<EventNotice>, StorageError> {
    let by = worked_by(tx, root)?;
    let mut notices = vec![ticket_event(tx, ticket, project, EventKind::TicketReview, by, json!({ "task_id": root }))?];
    let mut line = SystemLine::new(SystemLineKind::Review);
    (line.from, line.task_id) = (Some(crate::conversation::discussion::actor_name(tx, by)?), Some(root));
    notices.extend(crate::conversation::discussion::ticket_line(tx, ticket, line, "")?);
    Ok(notices)
}

/// The ticket was started (it is in Todo): a new main task in Draft, with the
/// ticket's title, description, files, mode and model. With an owner it goes to
/// that employee at once; without one the dispatcher chooses (`triage`).
pub(crate) fn started(tx: &Tx<'_>, st: &Store, ticket: TicketId, user: UserId) -> Result<Vec<EventNotice>, StorageError> {
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let Some(view) = store::view(tx, ticket, &read)? else { return Ok(vec![]) };
    // A quick task starts in Doing (ST-01 AC3).
    let quick = view.is_quick();
    if view.column != ColumnKind::Todo && !(quick && view.column == ColumnKind::Doing) {
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
    // A quick task goes to its employee in Doing (ST-01 AC3).
    let quick_start = state.kind == aurelius_domain::ticket::TicketKind::Quick && state.column == ColumnKind::Doing;
    if (state.column != ColumnKind::Todo && !quick_start) || state.root_status != Some(TaskStatus::Draft) {
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
    let from = username(tx, user)?;
    resume_as(tx, st, task, &from, text, reason)
}

/// `resume_with` with `text` from `from` (a board member's name, or the tester's).
pub(crate) fn resume_as(tx: &Tx<'_>, st: &Store, task: TaskId, from: &str, text: &str, reason: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = task_store::load_task(tx, task)? else { return Ok(vec![]) };
    let Some((conversation, _)) = task_store::task_conversation(tx, task)? else { return Ok(vec![]) };
    let (message, posted) = thread::post_returning(tx, st, conversation, task, Speaker::Update { kind: "message", from }, text)?;
    let mut notices = vec![posted];
    notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?);
    notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?);
    notices.push(task_event(tx, task, row.project_id, "task.resumed", json!({ "reason": reason }))?);
    notices.extend(crate::task::task_runs::run_for(tx, task, conversation, message)?);
    Ok(notices)
}

/// Files added to the ticket (`ticket.update { add_files }`) while its main task
/// still works: the task only hears of its files through the brief, posted once
/// when its conversation opens, so a task that has one gets them as an update
/// that names each file (absolute path and size, as `TaskAttachments::prompt`).
/// A Running task gets a run that answers it; a live run is never interrupted:
/// it reads only up to the message it answers, so the new run comes after it.
/// The same holds for a run parked for the usage limit or a Claude login: it
/// resumes with its old message, so a run after it reads the files. A task that
/// waits otherwise (its turn, a plan's decision, Review) reads it on its next run. A task without a conversation yet gets nothing here: its brief names the files.
pub(crate) fn files_added(tx: &Tx<'_>, st: &Store, task: TaskId, added: &task_store::TaskAttachments) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = task_store::load_task(tx, task)? else { return Ok(vec![]) };
    if row.status.is_terminal() {
        return Ok(vec![]);
    }
    let (Some((conversation, _)), Some(text)) = (task_store::task_conversation(tx, task)?, added.prompt()) else { return Ok(vec![]) };
    let (message, posted) = thread::post_returning(tx, st, conversation, task, Speaker::Update { kind: "files", from: "Aurelius" }, &text)?;
    let mut notices = vec![posted];
    let parked = row.status == TaskStatus::Waiting && matches!(task_store::waiting_cause(tx, task)?.as_deref(), Some("quota" | aurelius_domain::task::WAITING_LOGIN));
    if row.status == TaskStatus::Running || parked {
        notices.extend(crate::task::task_runs::run_for(tx, task, conversation, message)?);
    }
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
