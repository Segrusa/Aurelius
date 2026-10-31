//! Plan review of a ticket (impact analysis §3.6): a plan of a ticket's main
//! task is a milestone of the task, not a live question. A board member may
//! review it hours later, after a restart too, so no Claude Code process waits
//! for it (an inbox approval would hold one and expire after 30 minutes).
//!
//! 1. The main task's conversation runs in Plan mode (the ticket's mode).
//! 2. Its `ExitPlanMode` is no inbox approval: the plan is posted in the
//!    conversation and Claude gets a deny that tells it to stop (`hand_in`).
//! 3. The run ends; in Plan mode the main task then waits (`plan_review`) and
//!    the card goes to Plan review (`on_finished`), before any report. An
//!    answer in Plan mode without `ExitPlanMode` is the plan as well.
//! 4. Approve: the conversation goes to Auto or Accept edits and the same
//!    session carries the plan out. Revise: Plan mode stays, the feedback goes
//!    in and a new plan follows. Reject is `ticket.cancel`.
//!
//! Other plans (a chat, a task without a ticket, a delegation: its Plan mode is
//! never passed on) still go to the inbox as before.

use crate::task::task_threads as thread;
use crate::task::tasks::task_event;
use crate::{CommandService, storage_error};
use aurelius_domain::{
    AppError, EventKind, EventNotice, RunId, TaskId, TicketId, UserId,
    autonomy::AgentMode,
    task::{TaskStatus, WAITING_PLAN_REVIEW},
    ticket::{Actor, PlanApproval},
};
use aurelius_storage::{StorageError, Store, Tx, conversations as conv, tasks as task_store, tickets as store};
use serde_json::json;

/// What Claude is told after it handed in the plan of a ticket.
pub const HANDED_IN: &str = "Your plan is handed in for review by the board. Stop now; you will be resumed when it is decided.";

/// The task of a run when it is the main task of a ticket and the run works in Plan mode.
fn planning_root(tx: &Tx<'_>, run: RunId) -> Result<Option<TaskId>, StorageError> {
    let task: Option<String> = tx.query_row("SELECT task_id FROM runs WHERE id = ?1 AND conversation_id IS NOT NULL", [run.to_string()], |r| r.get(0)).ok().flatten();
    let Some(task) = task.and_then(|t| TaskId::parse(&t).ok()) else { return Ok(None) };
    if !crate::task::ticket_line::is_root(tx, task)? || conv::mode_for_run(tx, run)? != AgentMode::Plan {
        return Ok(None);
    }
    Ok(Some(task))
}

/// `ExitPlanMode` of a run: for the main task of a ticket in Plan mode, the
/// plan goes in the conversation and the answer for Claude is the deny text
/// (`HANDED_IN`); none for any other run (the inbox decides, as before).
pub(crate) fn hand_in(tx: &Tx<'_>, st: &Store, run: RunId, input: &serde_json::Value) -> Result<Option<(String, Vec<EventNotice>)>, StorageError> {
    if planning_root(tx, run)?.is_none() {
        return Ok(None);
    }
    let plan = input.get("plan").and_then(|p| p.as_str()).map(str::trim).filter(|p| !p.is_empty()).unwrap_or("(No plan text was given.)");
    let notices = thread::post_ticket_plan(tx, st, run, plan)?.into_iter().collect();
    // The run's last words go under the plan, as after a decision in the inbox.
    conv::move_run_message_to_end(tx, run)?;
    Ok(Some((HANDED_IN.to_string(), notices)))
}

/// A run of a ticket's main task ended in Plan mode (completed, nothing left to
/// wait for): the task waits for a board member and the card goes to Plan
/// review. None when this run is no such run (the report follows as usual).
pub(crate) fn on_finished(tx: &Tx<'_>, run: RunId) -> Result<Option<Vec<EventNotice>>, StorageError> {
    let Some(task) = planning_root(tx, run)? else { return Ok(None) };
    let Some(row) = task_store::load_task(tx, task)? else { return Ok(None) };
    let Some((ticket, project, _)) = crate::task::ticket_line::root_of(tx, task)? else { return Ok(None) };
    let mut notices = crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some(WAITING_PLAN_REVIEW))?.into_vec();
    notices.push(task_event(tx, task, row.project_id, "task.waiting", json!({ "reason": WAITING_PLAN_REVIEW }))?);
    let by = row.employee_id.map(|employee_id| Actor::Employee { employee_id });
    notices.push(crate::task::ticket_line::ticket_event(tx, ticket, project, EventKind::TicketPlanReview, by, json!({ "task_id": task, "run_id": run }))?);
    Ok(Some(notices))
}

/// The main task waits for its plan to be decided.
fn waiting_root(tx: &Tx<'_>, ticket: TicketId) -> Result<Option<TaskId>, StorageError> {
    let Some(root) = store::state(tx, ticket)?.and_then(|s| s.root_task_id) else { return Ok(None) };
    let (status, reason): (String, Option<String>) = tx.query_row("SELECT status, waiting_reason FROM tasks WHERE id = ?1", [root.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let waits = TaskStatus::parse(&status) == Some(TaskStatus::Waiting) && reason.as_deref() == Some(WAITING_PLAN_REVIEW);
    Ok(waits.then_some(root))
}

/// Whether the main task of the ticket waits in Plan review (the buttons need it).
pub(crate) fn waits_for_review(tx: &Tx<'_>, ticket: TicketId) -> Result<bool, StorageError> {
    Ok(waiting_root(tx, ticket)?.is_some())
}

/// `ticket.approve_plan`: the conversation leaves Plan mode and the same session carries the plan out.
pub(crate) fn approve(tx: &Tx<'_>, st: &Store, ticket: TicketId, user: UserId, decision: PlanApproval, feedback: Option<&str>) -> Result<Vec<EventNotice>, StorageError> {
    let Some(root) = waiting_root(tx, ticket)? else { return Ok(vec![]) };
    let mode = match decision {
        PlanApproval::Approve => AgentMode::Auto,
        PlanApproval::ApproveEdits => AgentMode::AcceptEdits,
    };
    let mut notices = Vec::new();
    if let Some((conversation, _)) = task_store::task_conversation(tx, root)? {
        if conv::set_agent_mode(tx, conversation, mode)? {
            let (_, n) = crate::run::events::emit(
                tx,
                crate::task::tasks::ev("conversation", conversation.to_string(), "conversation.settings_changed", aurelius_domain::ActorKind::User, ticket.to_string(), None, json!({ "agent_mode": mode.as_str(), "ticket_id": ticket })),
            )?;
            notices.push(n);
        }
    }
    let how = match decision {
        PlanApproval::Approve => "carry it out",
        PlanApproval::ApproveEdits => "carry it out; edits go through, everything else is asked",
    };
    let text = match feedback.map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => format!("The board approved your plan: {how}.\n\nFeedback: {f}"),
        None => format!("The board approved your plan: {how}."),
    };
    notices.extend(crate::task::ticket_line::resume_with(tx, st, root, user, &text, "plan_approved")?);
    Ok(notices)
}

/// `ticket.revise_plan`: Plan mode stays; the feedback goes in and a new plan follows.
pub(crate) fn revise(tx: &Tx<'_>, st: &Store, ticket: TicketId, user: UserId, feedback: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(root) = waiting_root(tx, ticket)? else { return Ok(vec![]) };
    let text = format!("The board wants changes to your plan: {}\n\nMake a new plan and hand it in with ExitPlanMode. Do not start the work yet.", feedback.trim());
    crate::task::ticket_line::resume_with(tx, st, root, user, &text, "plan_revised")
}

impl CommandService {
    /// The bridge's `ExitPlanMode` of a run (see `hand_in`): `Some(deny text)`
    /// for a ticket's main task in Plan mode, none for the inbox as before.
    pub fn ticket_plan_handed_in(&self, run: RunId, input: &serde_json::Value) -> Result<Option<(String, Vec<EventNotice>)>, AppError> {
        let st = self.store.clone();
        let input = input.clone();
        self.store.write(move |tx| hand_in(tx, &st, run, &input)).map_err(storage_error)
    }
}
