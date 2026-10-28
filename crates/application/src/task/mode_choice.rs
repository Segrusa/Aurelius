//! Management chooses the mode of its own task (feature `management_chooses_mode`): the
//! `set_mode` tool of the bridge. Only the CEO and their direct reports, only in the task's own
//! conversation, only Plan or Auto, never while a plan waits for the user, and only when the
//! user picked no mode for the task and the server setting is on.

use crate::task::task_runs::run_owner;
use crate::task::task_threads::{self as thread, Speaker};
use crate::task::tasks::task_event;
use aurelius_domain::autonomy::AgentMode;
use aurelius_domain::{EventNotice, RunId, TaskId};
use aurelius_storage::{StorageError, Store, Tx, conversations as conv, employees, management_mode, tasks as store};

type Outcome = Result<(bool, String, Vec<EventNotice>), StorageError>;

/// `set_mode {mode, reason}` from a run of a task conversation.
pub(crate) fn set_mode(tx: &Tx<'_>, st: &Store, run: RunId, mode: &str, reason: &str) -> Outcome {
    let refuse = |why: &str| Ok((false, why.to_string(), vec![]));
    let new_mode = match mode.trim().to_ascii_lowercase().as_str() {
        "auto" => AgentMode::Auto,
        "plan" => AgentMode::Plan,
        _ => return refuse("The mode must be auto or plan."),
    };
    let reason = reason.trim();
    if reason.is_empty() {
        return refuse("Give a short reason for the mode you choose.");
    }
    let Some(owner) = run_owner(tx, run)? else { return refuse("Only an employee can choose a mode.") };
    let Some(me) = owner.employee else { return refuse("Only an employee can choose a mode.") };
    if !management_mode::is_management(tx, me)? {
        return refuse("Only management (the CEO and those who report to them) chooses its own mode.");
    }
    if !management_mode::enabled(tx)? {
        return refuse("The user turned off \"Management chooses its own mode\": the mode is theirs to set.");
    }
    // The task of this very run, and its own conversation: not a chat, not another task.
    let task: Option<String> = tx.query_row("SELECT task_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).ok().flatten();
    let Some(task) = task.and_then(|t| TaskId::parse(&t).ok()) else { return refuse("set_mode only works in the conversation of your own task.") };
    let Some(row) = store::load_task(tx, task)? else { return refuse("The task no longer exists.") };
    if row.status.is_terminal() {
        return refuse("The task has already ended.");
    }
    let Some((conversation, Some(owner_of_thread))) = store::task_conversation(tx, task)? else { return refuse("set_mode only works in the conversation of your own task.") };
    if owner.conversation != Some(conversation) || owner_of_thread != me || row.employee_id != Some(me) {
        return refuse("set_mode only works in the conversation of your own task.");
    }
    if store::task_run(tx, task)?.mode.is_some() {
        return refuse("The user chose the mode of this task; it stays as they set it.");
    }
    // The user's own switch (Ask, Accept edits) is theirs to undo.
    let current = conv::mode_for_run(tx, run)?;
    if !matches!(current, AgentMode::Plan | AgentMode::Auto) {
        return refuse("The user set the mode of this conversation; it stays as they set it.");
    }
    // Nobody approves their own plan.
    let waiting: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM approvals a JOIN runs r ON r.id = a.run_id
           WHERE r.conversation_id = ?1 AND a.tool = 'claude.ExitPlanMode' AND a.status = 'pending')",
        [conversation.to_string()],
        |r| r.get(0),
    )?;
    if waiting {
        return refuse("Your plan waits for the user's approval; the user decides, set_mode is refused until then.");
    }
    if conv::set_mode_for_run(tx, run, new_mode)?.is_none() {
        return refuse("This run has no conversation to set the mode of.");
    }
    let name = employees::load_employee(tx, me)?.map(|m| m.name).unwrap_or_default();
    let label = if new_mode == AgentMode::Auto { "Auto" } else { "Plan" };
    let mut notices = Vec::new();
    // The line in the thread: "<name> chose Auto: <reason>".
    let line = format!("{name} chose {label}: {reason}");
    notices.push(thread::post_to(tx, st, conversation, task, Speaker::Update { kind: "mode_chosen", from: &name }, &line)?);
    // The audit log: the event is the entry (task.mode_chosen).
    notices.push(task_event(
        tx,
        task,
        row.project_id,
        "task.mode_chosen",
        serde_json::json!({ "employee_id": me, "mode": new_mode.as_str(), "reason": reason, "subject": name, "conversation_id": conversation, "run_id": run }),
    )?);
    let text = if new_mode == AgentMode::Auto {
        format!("Mode is now Auto: work without asking, as the reason says: {reason}.")
    } else {
        format!("Mode is now Plan: read and plan only, then present the plan with ExitPlanMode. Reason: {reason}.")
    };
    Ok((true, text, notices))
}
