//! A task is worked on in a conversation. The employee that takes a task on
//! gets a conversation with the task as its context; there it works itself
//! (with its tools; subagents only for broad searches). It can
//! delegate parts to colleagues (sub-tasks, each in a conversation of their
//! own) or hand the whole task over. Its last answer is the report: the
//! task's result.
//!
//! Every run in a live task's conversation counts for the task; when one
//! completes, the task succeeds with that answer, unless colleagues still
//! work on parts of it: then it waits, and continues when they are done.

use crate::run::queue::queue_run;
use crate::task::task_threads::{self as thread, Speaker};
use crate::task::tasks::{TaskContext, ev, task_event};
use crate::{CommandService, storage_error};
use aurelius_domain::{
    ActorKind, AppError, ConversationId, EventNotice, EmployeeId, MessageId, RunId, TaskId,
    task::{self, TaskStatus},
};
use aurelius_storage::{StorageError, Store, Tx, blobs, employees, tasks as store};

/// How an employee works on a task in its conversation (system text).
pub const TASK_GUIDANCE: &str = "This conversation is a task you took on. Start with one or two sentences on how you will approach it. Do the work yourself with your own tools; use subagents (the Task tool) only for a broad search across many files, not by default. You can give a part to a colleague with delegate_task (you get their result here) or hand the whole task over with hand_over_task. Message a part you gave away with message_task (it interrupts them by default). Everything you do is visible in this conversation, so keep your closing answer short: say whether it worked, in one or two sentences, and what the user must still do or decide, if anything. No report, no headings, and do not repeat what is already above it (steps, tool calls, files, screenshots, ids). That closing answer is the task's result.";

/// What an employee is told when the colleagues' results are in.
const COLLEAGUES_DONE: &str = "Your colleagues finished their parts (their results are above). Bring everything together and finish with your report.";

/// A run of the task's conversation still going (other than `except`).
fn live_run(conn: &rusqlite::Connection, task: TaskId, except: Option<RunId>) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM runs WHERE task_id = ?1 AND conversation_id IS NOT NULL AND id <> COALESCE(?2, '')
           AND status IN ('created','starting','running','waiting','cancelling'))",
        rusqlite::params![task.to_string(), except.map(|r| r.to_string())],
        |r| r.get(0),
    )?)
}

/// Queue a run of the task's conversation that answers `reply_to`, counted for the task.
fn run_for(tx: &Tx<'_>, task: TaskId, conversation: ConversationId, reply_to: MessageId) -> Result<Vec<EventNotice>, StorageError> {
    let (run, notice) = queue_run(tx, &serde_json::json!({ "task_id": task }), &task.to_string(), conversation, reply_to)?;
    tx.execute("UPDATE runs SET task_id = ?2 WHERE id = ?1", [run.to_string(), task.to_string()])?;
    Ok(vec![notice])
}

/// Called when a run is queued in a conversation: a run in a live task's
/// conversation (of the task's own employee) counts for the task; a task that
/// waited for the user goes on.
pub(crate) fn link_run(tx: &Tx<'_>, run: RunId, conversation: ConversationId) -> Result<(), StorageError> {
    let Some(task) = store::task_for_conversation(tx, conversation)? else { return Ok(()) };
    let Some(row) = store::load_task(tx, task)? else { return Ok(()) };
    let owner = store::task_conversation(tx, task)?.and_then(|(_, m)| m);
    if row.status.is_terminal() || owner.is_none() || owner != row.employee_id {
        return Ok(());
    }
    tx.execute("UPDATE runs SET task_id = ?2 WHERE id = ?1", [run.to_string(), task.to_string()])?;
    if row.status == TaskStatus::Waiting && matches!(store::waiting_cause(tx, task)?.as_deref(), Some("stopped" | "not_started" | "escalated_to_boss")) {
        crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?;
        crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?;
        task_event(tx, task, row.project_id, "task.resumed", serde_json::json!({ "reason": "conversation" }))?;
    }
    Ok(())
}

/// A run was queued, started or ended in the conversation of a finished task
/// (its employee's own): the task keeps its status, but `TaskView::live_run`
/// may have changed, so clients are told to read the task again.
pub(crate) fn live_run_notice(tx: &Tx<'_>, conversation: ConversationId) -> Result<Vec<EventNotice>, StorageError> {
    let Some(task) = store::task_for_conversation(tx, conversation)? else { return Ok(vec![]) };
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    let owner = store::task_conversation(tx, task)?.and_then(|(_, m)| m);
    if !row.status.is_terminal() || owner.is_none() || owner != row.employee_id {
        return Ok(vec![]);
    }
    Ok(vec![task_event(tx, task, row.project_id, "task.run_changed", serde_json::json!({ "conversation_id": conversation }))?])
}

/// A run ended: what it means for the task it belongs to.
pub(crate) fn on_run_finished(tx: &Tx<'_>, st: &Store, run: RunId, completed: bool, failed: Option<&str>, answer: &str) -> Result<Vec<EventNotice>, StorageError> {
    // A trial of a task (its variants): the variant ends, the task job goes on with the next step.
    if let Some(notices) = crate::task::variants::on_probe_run_finished(tx, st, run, completed, failed, answer)? {
        return Ok(notices);
    }
    let task: Option<String> = tx.query_row("SELECT task_id FROM runs WHERE id = ?1 AND conversation_id IS NOT NULL", [run.to_string()], |r| r.get(0)).ok().flatten();
    // A run of a finished task is not linked to it: the task keeps its status, only its live run ends.
    let Some(task) = task.and_then(|t| TaskId::parse(&t).ok()) else {
        let conversation: Option<String> = tx.query_row("SELECT conversation_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).ok().flatten();
        return match conversation.and_then(|c| ConversationId::parse(&c).ok()) {
            Some(conversation) => live_run_notice(tx, conversation),
            None => Ok(vec![]),
        };
    };
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    let employee: Option<String> = tx.query_row("SELECT c.employee_id FROM runs r JOIN conversations c ON c.id = r.conversation_id WHERE r.id = ?1", [run.to_string()], |r| r.get(0)).ok().flatten();
    let handed_over = employee.and_then(|m| EmployeeId::parse(&m).ok()) != row.employee_id;
    // Handed over while this run went on: the new employee's start may have found
    // this run still live and done nothing; its job goes again now.
    if handed_over && row.status == TaskStatus::Queued && !live_run(tx, task, Some(run))? {
        wake_task_job(tx, task, false)?;
        return Ok(vec![]);
    }
    // Ended, handed over meanwhile, or another run of it is still going: nothing to do.
    if row.status.is_terminal() || handed_over || live_run(tx, task, Some(run))? {
        return Ok(vec![]);
    }
    let mut notices = Vec::new();
    if let Some(why) = failed {
        crate::task::task_line::set_status(tx, task, TaskStatus::Failed, None)?;
        // With the org chart on, the employee's boss gets it first; "Needs you" then shows it as handled.
        let up = crate::org::escalate(tx, st, task, &row, &format!("it failed ({why})"), answer)?;
        let mut failed_meta = serde_json::json!({ "reason": why });
        if let Some((boss, _)) = &up {
            failed_meta["escalated_to"] = serde_json::json!(boss);
        }
        let failed_notice = task_event(tx, task, row.project_id, "task.failed", failed_meta)?;
        // A failed task teaches too: the cheap extraction looks at what went wrong (lessons).
        let source: String = tx.query_row("SELECT id FROM events WHERE seq = ?1", [failed_notice.seq as i64], |r| r.get(0))?;
        if let Ok(source) = aurelius_domain::EventId::parse(&source) {
            crate::memory::schedule_extraction(tx, task, source)?;
        }
        notices.push(failed_notice);
        notices.extend(up.map(|(_, n)| n).unwrap_or_default());
        notices.extend(after_ended(tx, st, task, &format!("“{}” failed ({why}).", row.title), false)?);
        return Ok(notices);
    }
    if !completed {
        // Stopped by the user: the task waits until they continue in its conversation.
        crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("input"))?;
        let up = crate::org::escalate(tx, st, task, &row, "it was stopped and waits for input", answer)?;
        let waiting = match &up {
            Some((boss, _)) => serde_json::json!({ "reason": "escalated_to_boss", "boss": boss }),
            None => serde_json::json!({ "reason": "stopped" }),
        };
        notices.push(task_event(tx, task, row.project_id, "task.waiting", waiting)?);
        notices.extend(up.map(|(_, n)| n).unwrap_or_default());
        return Ok(notices);
    }
    if store::children_of(tx, task)?.iter().any(|c| !c.status.is_terminal()) {
        // Colleagues still work on parts: it continues when they are done.
        crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("dependency"))?;
        notices.push(task_event(tx, task, row.project_id, "task.waiting", serde_json::json!({ "reason": "delegated" }))?);
        return Ok(notices);
    }
    if unseen_results(tx, task, run)? {
        // A colleague finished while this run went on: its answer did not see that result yet.
        if let Some((conversation, _)) = store::task_conversation(tx, task)? {
            let (message, posted) = thread::post_returning(tx, st, conversation, task, Speaker::Update { kind: "status", from: "Aurelius" }, COLLEAGUES_DONE)?;
            notices.push(posted);
            notices.extend(run_for(tx, task, conversation, message)?);
            return Ok(notices);
        }
    }
    notices.extend(complete(tx, st, task, answer)?);
    Ok(notices)
}

/// A run of the task's conversation could not be started (its retries are
/// used up): nothing of it runs any more, so the task waits for the user, who
/// sends the message again in its conversation (as after a stop).
pub(crate) fn on_run_not_started(tx: &Tx<'_>, st: &Store, run: RunId) -> Result<Vec<EventNotice>, StorageError> {
    let task: Option<String> = tx.query_row("SELECT task_id FROM runs WHERE id = ?1 AND conversation_id IS NOT NULL", [run.to_string()], |r| r.get(0)).ok().flatten();
    let Some(task) = task.and_then(|t| TaskId::parse(&t).ok()) else { return Ok(vec![]) };
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    let employee: Option<String> = tx.query_row("SELECT c.employee_id FROM runs r JOIN conversations c ON c.id = r.conversation_id WHERE r.id = ?1", [run.to_string()], |r| r.get(0)).ok().flatten();
    let handed_over = employee.and_then(|m| EmployeeId::parse(&m).ok()) != row.employee_id;
    if row.status != TaskStatus::Running || handed_over || live_run(tx, task, Some(run))? {
        return Ok(vec![]);
    }
    crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("input"))?;
    let up = crate::org::escalate(tx, st, task, &row, "its run could not be started", "")?;
    let waiting = match &up {
        Some((boss, _)) => serde_json::json!({ "reason": "escalated_to_boss", "boss": boss }),
        None => serde_json::json!({ "reason": "not_started" }),
    };
    let mut notices = vec![task_event(tx, task, row.project_id, "task.waiting", waiting)?];
    notices.extend(up.map(|(_, n)| n).unwrap_or_default());
    Ok(notices)
}

/// The task of `run` (still going, not handed over), if there is one.
fn live_task_of(tx: &Tx<'_>, run: RunId) -> Result<Option<(TaskId, store::TaskRow)>, StorageError> {
    let task: Option<String> = tx.query_row("SELECT task_id FROM runs WHERE id = ?1 AND conversation_id IS NOT NULL", [run.to_string()], |r| r.get(0)).ok().flatten();
    let Some(task) = task.and_then(|t| TaskId::parse(&t).ok()) else { return Ok(None) };
    let Some(row) = store::load_task(tx, task)? else { return Ok(None) };
    let employee: Option<String> = tx.query_row("SELECT c.employee_id FROM runs r JOIN conversations c ON c.id = r.conversation_id WHERE r.id = ?1", [run.to_string()], |r| r.get(0)).ok().flatten();
    let handed_over = employee.and_then(|m| EmployeeId::parse(&m).ok()) != row.employee_id;
    Ok((!row.status.is_terminal() && !handed_over).then_some((task, row)))
}

/// A run waits for the usage limit to reset: its task waits with it ("quota"),
/// it is neither failed nor escalated.
pub(crate) fn on_run_quota_wait(tx: &Tx<'_>, run: RunId, until_ms: i64) -> Result<Vec<EventNotice>, StorageError> {
    let Some((task, row)) = live_task_of(tx, run)? else { return Ok(vec![]) };
    if !matches!(row.status, TaskStatus::Running | TaskStatus::Queued) {
        return Ok(vec![]);
    }
    crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("quota"))?;
    Ok(vec![task_event(tx, task, row.project_id, "task.waiting", serde_json::json!({ "reason": "quota", "until_ms": until_ms }))?])
}

/// A run that waited for the limit was stopped before it went again: its task
/// no longer waits for the reset but, as after any stop, for the user.
pub(crate) fn on_parked_run_stopped(tx: &Tx<'_>, run: RunId) -> Result<Vec<EventNotice>, StorageError> {
    let Some((task, row)) = live_task_of(tx, run)? else { return Ok(vec![]) };
    if row.status != TaskStatus::Waiting || store::waiting_cause(tx, task)?.as_deref() != Some("quota") || live_run(tx, task, Some(run))? {
        return Ok(vec![]);
    }
    crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("input"))?;
    Ok(vec![task_event(tx, task, row.project_id, "task.waiting", serde_json::json!({ "reason": "stopped" }))?])
}

/// A run that waited for the limit starts again: so does its task.
pub(crate) fn on_run_quota_resumed(tx: &Tx<'_>, run: RunId) -> Result<Vec<EventNotice>, StorageError> {
    let Some((task, row)) = live_task_of(tx, run)? else { return Ok(vec![]) };
    if row.status != TaskStatus::Waiting || store::waiting_cause(tx, task)?.as_deref() != Some("quota") {
        return Ok(vec![]);
    }
    crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?;
    crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?;
    Ok(vec![task_event(tx, task, row.project_id, "task.resumed", serde_json::json!({ "reason": "quota" }))?])
}

/// The task succeeded with this report.
pub(crate) fn complete(tx: &Tx<'_>, st: &Store, task: TaskId, report: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    // The report, under the task's title.
    let body = if report.trim().is_empty() { "Done.".to_string() } else { report.trim().to_owned() };
    let report = if body.starts_with("# ") { body } else { format!("# {}\n\n{body}", row.title) };
    // Outside Git the report is also kept as a file in the employee's (or the project's) folder.
    let folder = match row.employee_id.map(|m| aurelius_storage::files::employee_folder(tx, m)).transpose()?.flatten() {
        Some(f) => Some(f),
        None => aurelius_storage::files::project_folder(tx, row.project_id)?,
    };
    if let Some(folder) = folder.filter(|f| !std::path::Path::new(f).join(".git").exists()) {
        crate::org::folders::write_artifact_file(&folder, &format!("{} — report", row.title), &report);
    }
    let blob = st.blobs().put(report.as_bytes(), "text/markdown; charset=utf-8")?;
    blobs::register(tx, &blob)?;
    store::set_task_result(tx, task, &blob.id.to_string())?;
    let mut notices = Vec::new();
    for (from, to) in [(TaskStatus::Queued, TaskStatus::Running), (TaskStatus::Running, TaskStatus::Review), (TaskStatus::Waiting, TaskStatus::Review)] {
        if store::load_task(tx, task)?.is_some_and(|r| r.status == from) {
            crate::task::task_line::set_status(tx, task, to, None)?;
        }
    }
    crate::task::task_line::set_status(tx, task, TaskStatus::Succeeded, None)?;
    let succeeded = task_event(tx, task, row.project_id, "task.succeeded", serde_json::json!({ "reason": "reported" }))?;
    let source: String = tx.query_row("SELECT id FROM events WHERE seq = ?1", [succeeded.seq as i64], |r| r.get(0))?;
    if let Ok(source) = aurelius_domain::EventId::parse(&source) {
        crate::memory::schedule_extraction(tx, task, source)?;
    }
    notices.push(succeeded);
    // Tasks waiting on this one can go.
    tx.execute(
        "UPDATE jobs SET not_before_ms = ?2, last_error_code = NULL WHERE state = 'ready'
         AND id IN (SELECT task_id FROM task_dependencies WHERE requires_task_id = ?1)",
        (task.to_string(), aurelius_storage::now_ms()),
    )?;
    notices.extend(after_ended(tx, st, task, &report, true)?);
    Ok(notices)
}

/// A sub-task ended: its result goes to the employee who delegated it; when
/// nothing else is open, that employee continues (and writes its report).
/// A task an employee gave from one of its conversations: the result goes to
/// that conversation, and the employee answers on it.
fn after_ended(tx: &Tx<'_>, st: &Store, task: TaskId, text: &str, succeeded: bool) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    let colleague = row.employee_id.and_then(|m| employees::load_employee(tx, m).ok().flatten()).map(|m| m.name).unwrap_or_else(|| "A colleague".into());
    let finding = if succeeded { format!("Finished “{}”:\n\n{}", row.title, text.trim()) } else { text.to_owned() };
    let speaker = Speaker::Update { kind: if succeeded { "finding" } else { "status" }, from: &colleague };
    if let Some(parent) = row.parent {
        let mut notices = Vec::new();
        if let Some((conversation, _)) = store::task_conversation(tx, parent)? {
            notices.push(thread::post_result(tx, st, conversation, parent, task, speaker, &finding)?.1);
        }
        notices.extend(continue_when_done(tx, st, parent)?);
        return Ok(notices);
    }
    report_to_origin(tx, st, task, speaker, &finding)
}

/// The result of a task an employee gave from one of its conversations goes to
/// that conversation (nothing for other tasks). The employee answers once all
/// it gave from there are in, in one run: not while another of them is still
/// open, and not while a run of the conversation goes (its end looks again,
/// see `answer_results`).
pub(crate) fn report_to_origin(tx: &Tx<'_>, st: &Store, task: TaskId, speaker: Speaker<'_>, text: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(conversation) = store::task_origin(tx, task)? else { return Ok(vec![]) };
    let (message, posted) = thread::post_result(tx, st, conversation, task, task, speaker, text)?;
    let mut notices = vec![posted];
    if !open_from(tx, conversation, Some(task))? && !live_in(tx, conversation, None)? {
        let (_, queued) = queue_run(tx, &serde_json::json!({ "origin": "delegated", "task_id": task }), &task.to_string(), conversation, message)?;
        notices.push(queued);
    }
    Ok(notices)
}

/// A task given from a conversation ended without a result (cancelled): the
/// results it held back are answered now, if nothing else is open there.
pub(crate) fn origin_released(tx: &Tx<'_>, task: TaskId) -> Result<Vec<EventNotice>, StorageError> {
    let Some(conversation) = store::task_origin(tx, task)? else { return Ok(vec![]) };
    if open_from(tx, conversation, None)? || live_in(tx, conversation, None)? {
        return Ok(vec![]);
    }
    let Some(latest) = latest_message(tx, conversation)? else { return Ok(vec![]) };
    // Only when the latest message is a result nobody answered yet.
    if !is_result(tx, latest)? {
        return Ok(vec![]);
    }
    let (_, queued) = queue_run(tx, &serde_json::json!({ "origin": "delegated", "task_id": task }), &task.to_string(), conversation, latest)?;
    Ok(vec![queued])
}

/// A run of a conversation ended: results of tasks given from there that came
/// in while it went (it did not see them) get one run of their own, once
/// nothing else given from there is open and no other run of it goes.
pub(crate) fn answer_results(tx: &Tx<'_>, run: RunId, completed: bool) -> Result<Vec<EventNotice>, StorageError> {
    if !completed {
        return Ok(vec![]);
    }
    let conversation: Option<String> = tx.query_row("SELECT conversation_id FROM runs WHERE id = ?1 AND task_id IS NULL", [run.to_string()], |r| r.get(0)).ok().flatten();
    let Some(conversation) = conversation.and_then(|c| ConversationId::parse(&c).ok()) else { return Ok(vec![]) };
    let latest: Option<(String, String)> = tx
        .query_row(
            "SELECT m.id, json_extract(m.meta_json, '$.result_of') FROM messages m
             WHERE m.conversation_id = ?1 AND json_extract(m.meta_json, '$.result_of') IS NOT NULL
               AND m.ordinal > COALESCE((SELECT q.ordinal FROM messages a JOIN messages q ON q.id = a.reply_to_id WHERE a.run_id = ?2), 0)
             ORDER BY m.ordinal DESC LIMIT 1",
            [conversation.to_string(), run.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let Some((message, task)) = latest.and_then(|(m, t)| Some((MessageId::parse(&m).ok()?, TaskId::parse(&t).ok()?))) else { return Ok(vec![]) };
    if open_from(tx, conversation, None)? || live_in(tx, conversation, Some(run))? {
        return Ok(vec![]);
    }
    let (_, queued) = queue_run(tx, &serde_json::json!({ "origin": "delegated", "task_id": task }), &task.to_string(), conversation, message)?;
    Ok(vec![queued])
}

/// A task given from `conversation` (other than `except`) is still open.
fn open_from(conn: &rusqlite::Connection, conversation: ConversationId, except: Option<TaskId>) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE origin_conversation_id = ?1 AND id <> COALESCE(?2, '')
           AND status NOT IN ('succeeded','failed','cancelled'))",
        rusqlite::params![conversation.to_string(), except.map(|t| t.to_string())],
        |r| r.get(0),
    )?)
}

/// A run of the conversation is queued or going (other than `except`).
pub(crate) fn live_in(conn: &rusqlite::Connection, conversation: ConversationId, except: Option<RunId>) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM runs WHERE conversation_id = ?1 AND id <> COALESCE(?2, '')
           AND status IN ('created','starting','running','waiting','cancelling'))",
        rusqlite::params![conversation.to_string(), except.map(|r| r.to_string())],
        |r| r.get(0),
    )?)
}

/// The message is a colleague's result.
fn is_result(conn: &rusqlite::Connection, message: MessageId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM messages WHERE id = ?1 AND json_extract(meta_json, '$.result_of') IS NOT NULL)",
        [message.to_string()],
        |r| r.get(0),
    )?)
}

/// Results of colleagues posted in the task's conversation after `run` started
/// (after the message it answers), so that run has not seen them.
fn unseen_results(tx: &Tx<'_>, task: TaskId, run: RunId) -> Result<bool, StorageError> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM messages m JOIN tasks t ON t.conversation_id = m.conversation_id
           WHERE t.id = ?1 AND json_extract(m.meta_json, '$.result_of') IS NOT NULL
             AND m.ordinal > COALESCE((SELECT q.ordinal FROM messages a JOIN messages q ON q.id = a.reply_to_id WHERE a.run_id = ?2), 0))",
        [task.to_string(), run.to_string()],
        |r| r.get(0),
    )?)
}

/// A task waiting for its colleagues goes on once none of them is still at work.
pub(crate) fn continue_when_done(tx: &Tx<'_>, st: &Store, task: TaskId) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    if row.status != TaskStatus::Waiting || store::children_of(tx, task)?.iter().any(|c| !c.status.is_terminal()) || live_run(tx, task, None)? {
        return Ok(vec![]);
    }
    let Some((conversation, _)) = store::task_conversation(tx, task)? else { return Ok(vec![]) };
    // An employee that works on one task at a time goes on with this one when it is its turn
    // (the colleagues' results are then the latest in its conversation).
    if let Some(employee) = row.employee_id {
        if crate::task::task_line::waits_its_turn(tx, employee, task)? {
            return in_line(tx, task);
        }
    }
    let (message, posted) = thread::post_returning(
        tx,
        st,
        conversation,
        task,
        Speaker::Update { kind: "status", from: "Aurelius" },
        COLLEAGUES_DONE,
    )?;
    let mut notices = vec![posted];
    crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?;
    crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?;
    notices.push(task_event(tx, task, row.project_id, "task.resumed", serde_json::json!({ "reason": "colleagues_done" }))?);
    notices.extend(run_for(tx, task, conversation, message)?);
    Ok(notices)
}

/// The task waits for its turn with an employee that works on one task at a
/// time: queued (said once), and the oldest one in line goes when nothing runs.
fn in_line(tx: &Tx<'_>, task: TaskId) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    if row.status == TaskStatus::Waiting {
        crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?;
    }
    let said: Option<String> = tx
        .query_row(
            "SELECT json_extract(metadata_json, '$.reason') FROM events
             WHERE aggregate_type = 'task' AND aggregate_id = ?1 AND type IN ('task.queued','task.started','task.resumed','task.waiting','task.handed_off')
             ORDER BY seq DESC LIMIT 1",
            [task.to_string()],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    let mut notices = Vec::new();
    if said.as_deref() != Some("sequential") {
        notices.push(task_event(tx, task, row.project_id, "task.queued", serde_json::json!({ "reason": "sequential" }))?);
    }
    if let Some(employee) = row.employee_id {
        crate::task::task_line::next_in_line(tx, employee)?;
    }
    Ok(notices)
}

/// The latest message of a conversation.
fn latest_message(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<Option<MessageId>, StorageError> {
    let latest: Option<String> = conn
        .query_row("SELECT id FROM messages WHERE conversation_id = ?1 ORDER BY ordinal DESC LIMIT 1", [conversation.to_string()], |r| r.get(0))
        .ok();
    Ok(latest.and_then(|m| MessageId::parse(&m).ok()))
}

/// An employee's action on its task during a run (the bridge's delegate and hand-over tools).
#[derive(Debug, Clone)]
pub enum EmployeeAction {
    /// `project` (a name or id): needed outside a project (an employee's chat); in a task it is the task's.
    Delegate { employee: String, title: String, goal: String, criteria: Vec<String>, project: Option<String> },
    HandOver { employee: String, reason: String },
    MessageTask { task: String, message: String, interrupt: bool },
    AddIdea { title: String, note: String, kind: aurelius_domain::idea::IdeaKind, project: Option<String> },
    ListTasks { status: String, employee: String, query: String, project: Option<String> },
    ReadTask { task: String },
    ListConversations { query: String },
    ReadConversation { conversation: String },
    /// Suggest a new colleague; only the user can approve it.
    ProposeColleague { name: String, role: String, title: Option<String>, instructions: String, reports_to: Option<String>, reason: String, department: Option<String> },
    /// Suggest new instructions for oneself or a direct report; only the user can approve them.
    ProposeInstructions { employee: Option<String>, instructions: String, reason: String, evidence: Vec<String> },
    /// Management chooses the mode (`auto` or `plan`) of its own task, with a reason.
    SetMode { mode: String, reason: String },
    /// The latest messages of the company's group chat (nothing changes).
    ReadGroupChat { last: Option<u32> },
    /// A message in the group chat under the employee's name; wakes the colleagues it names.
    PostToGroupChat { message: String },
}

impl CommandService {
    /// The employee takes the task on: its conversation starts with the task,
    /// and a run of it does the work. Nothing when a run of it already goes,
    /// or when it waits for colleagues.
    pub fn start_task_conversation(&self, ctx: &TaskContext, epoch: i64) -> Result<Vec<EventNotice>, AppError> {
        let Some(employee) = ctx.employee_id else { return Ok(vec![]) };
        let (task, project, st, brief) = (ctx.task_id, ctx.project_id, self.store.clone(), crate::task::tasks::brief_text(ctx));
        let waiting_for_colleagues = ctx.children_open();
        Ok(self
            .fenced_task(task, epoch, move |tx| {
                if live_run(tx, task, None)? {
                    return Ok(vec![]);
                }
                if waiting_for_colleagues {
                    return Ok(vec![]);
                }
                // An employee that works on one task at a time: this one waits for its turn.
                if store::load_task(tx, task)?.is_some_and(|r| r.status != TaskStatus::Running) && crate::task::task_line::waits_its_turn(tx, employee, task)? {
                    return in_line(tx, task);
                }
                let mut notices = Vec::new();
                let (conversation, brief_message, opened) = thread::open_returning(tx, &st, task, employee, &brief)?;
                notices.extend(opened);
                let Some(conversation) = conversation else { return Ok(notices) };
                let reply_to = match brief_message {
                    Some(m) => m,
                    // The conversation was there already (a restart, or its turn came
                    // after colleagues finished): go on from where it is.
                    None => {
                        let text = if latest_message(tx, conversation)?.map(|m| is_result(tx, m)).transpose()?.unwrap_or(false) { COLLEAGUES_DONE } else { "Continue with the task." };
                        let (m, n) = thread::post_returning(tx, &st, conversation, task, Speaker::Update { kind: "status", from: "Aurelius" }, text)?;
                        notices.push(n);
                        m
                    }
                };
                if let Some(row) = store::load_task(tx, task)? {
                    if row.status != TaskStatus::Running {
                        if row.status == TaskStatus::Waiting {
                            crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?;
                        }
                        crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?;
                        notices.push(task_event(tx, task, project, "task.started", serde_json::json!({ "conversation_id": conversation }))?);
                    }
                }
                notices.extend(run_for(tx, task, conversation, reply_to)?);
                Ok(notices)
            })?
            .unwrap_or_default())
    }

    /// Delegate or hand over, from a run of the task's conversation. Returns
    /// (ok, text for the model, notices).
    pub fn employee_action(&self, run: RunId, action: EmployeeAction) -> Result<(bool, String, Vec<EventNotice>), AppError> {
        let st = self.store.clone();
        if let EmployeeAction::ListConversations { .. } | EmployeeAction::ReadConversation { .. } = &action {
            return self
                .store
                .read(|conn| {
                    // The project's conversations; an employee sees its own former ones, not a colleague's.
                    let Some(owner) = run_owner(conn, run)? else {
                        return Ok((false, "Only an employee or the project assistant can look at conversations.".to_string(), vec![]));
                    };
                    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                    let now = aurelius_storage::now_ms();
                    let text = match &action {
                        EmployeeAction::ListConversations { query } => crate::conversation::overview::conversations_text(conn, owner.project, owner.employee, owner.conversation, query, now)?,
                        EmployeeAction::ReadConversation { conversation } => crate::conversation::overview::read_conversation_text(conn, owner.project, owner.employee, owner.conversation, conversation, &read, now)?,
                        _ => unreachable!("only the conversation actions"),
                    };
                    Ok((true, text, vec![]))
                })
                .map_err(storage_error);
        }
        if let EmployeeAction::ListTasks { .. } | EmployeeAction::ReadTask { .. } = &action {
            return self
                .store
                .read(|conn| {
                    // The tasks of the run's project; outside a project (an employee's chat) those of
                    // every project, or of the one it names.
                    let Some(owner) = run_owner(conn, run)? else {
                        return Ok((false, "Only an employee or the project assistant can look at tasks.".to_string(), vec![]));
                    };
                    let mut project = owner.project;
                    if let (None, EmployeeAction::ListTasks { project: Some(named), .. }) = (project, &action) {
                        match find_project(conn, named)? {
                            Ok(p) => project = Some(p),
                            Err(why) => return Ok((false, why, vec![])),
                        }
                    }
                    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                    let now = aurelius_storage::now_ms();
                    let text = match &action {
                        EmployeeAction::ListTasks { status, employee, query, .. } => crate::conversation::overview::list_text(conn, project, status, employee, query, now)?,
                        EmployeeAction::ReadTask { task } => crate::conversation::overview::read_text(conn, project, task, &read, now)?,
                        _ => unreachable!("only the reading actions"),
                    };
                    Ok((true, text, vec![]))
                })
                .map_err(storage_error);
        }
        if let EmployeeAction::ReadGroupChat { last } = &action {
            let last = *last;
            return self
                .store
                .read(|conn| {
                    if run_owner(conn, run)?.and_then(|o| o.employee).is_none() {
                        return Ok((false, "Only an employee can read the group chat.".to_string(), vec![]));
                    }
                    Ok((true, crate::conversation::group_chat::read_text(conn, &st, last)?, vec![]))
                })
                .map_err(storage_error);
        }
        if let EmployeeAction::PostToGroupChat { message } = &action {
            let message = message.clone();
            return self.store.write(move |tx| crate::conversation::group_chat::post_from_run(tx, &st, run, &message)).map_err(storage_error);
        }
        if let EmployeeAction::ProposeColleague { name, role, title, instructions, reports_to, reason, department } = &action {
            let input = crate::org::ProposalInput {
                name: name.clone(),
                role: role.clone(),
                title: title.clone(),
                instructions: instructions.clone(),
                reports_to: reports_to.clone(),
                reason: reason.clone(),
                department: department.clone(),
            };
            return self.store.write(move |tx| crate::org::propose_colleague(tx, &st, run, input)).map_err(storage_error);
        }
        if let EmployeeAction::ProposeInstructions { employee, instructions, reason, evidence } = &action {
            let input = crate::org::InstructionsInput { employee: employee.clone(), instructions: instructions.clone(), reason: reason.clone(), evidence: evidence.clone() };
            return self.store.write(move |tx| crate::org::propose_instructions(tx, &st, run, input)).map_err(storage_error);
        }
        if let EmployeeAction::SetMode { mode, reason } = &action {
            let (mode, reason) = (mode.clone(), reason.clone());
            return self.store.write(move |tx| crate::task::mode_choice::set_mode(tx, &st, run, &mode, &reason)).map_err(storage_error);
        }
        if let EmployeeAction::MessageTask { task, message, interrupt } = action {
            return self.store.write(move |tx| message_task(tx, &st, run, &task, &message, interrupt)).map_err(storage_error);
        }
        if let EmployeeAction::AddIdea { title, note, kind, project } = &action {
            let kind = *kind;
            let named = project.clone();
            let (title, note) = match aurelius_domain::idea::validate(title, Some(note)) {
                Ok(v) => v,
                Err(e) => return Ok((false, e.to_string(), vec![])),
            };
            return self
                .store
                .write(move |tx| {
                    // The run's project (or, from an employee's chat, the one it names), and its
                    // employee (none for the project assistant).
                    let Some(owner) = run_owner(tx, run)? else {
                        return Ok((false, "Only an employee or the project assistant can note ideas.".to_string(), vec![]));
                    };
                    let project = match (owner.project, named.as_deref()) {
                        (Some(p), _) => p,
                        (None, Some(named)) => match find_project(tx, named)? {
                            Ok(p) => p,
                            Err(why) => return Ok((false, why, vec![])),
                        },
                        (None, None) => return Ok((false, need_project(tx, "an idea")?, vec![])),
                    };
                    let (_, n) = crate::task::ideas::add(tx, project, &title, note.as_deref(), kind, owner.employee, run.to_string())?;
                    Ok((true, format!("Noted the idea “{title}” in the project's ideas; the user decides if and when it becomes a task."), vec![n]))
                })
                .map_err(storage_error);
        }
        self.store
            .write(move |tx| {
                let task: Option<String> = tx.query_row("SELECT task_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).ok().flatten();
                let Some(task) = task.and_then(|t| TaskId::parse(&t).ok()) else {
                    // Outside a task: the employee gives a colleague a task of its own.
                    return delegate_from_conversation(tx, &st, run, action);
                };
                let Some(row) = store::load_task(tx, task)? else { return Ok((false, "The task no longer exists.".to_string(), vec![])) };
                if row.status.is_terminal() {
                    return Ok((false, "The task has already ended.".to_string(), vec![]));
                }
                let Some(me) = row.employee_id else { return Ok((false, "The task has no employee.".to_string(), vec![])) };
                let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let colleagues = crate::task::tasks::colleagues(tx, &read)?;
                let find = |name: &str| find_colleague(&colleagues, Some(me), name);
                match action {
                    // A part stays in the task's project.
                    EmployeeAction::Delegate { employee, title, goal, criteria, .. } => {
                        let Some(to) = find(&employee) else {
                            return Ok((false, format!("No active colleague called {employee} in the company."), vec![]));
                        };
                        let policy = task_policy(tx, me);
                        // With the org chart on, work goes down the chart and the chart's depth is the limit.
                        let chart = crate::org::chart(tx)?;
                        let limit = chart.as_ref().map_or(policy.max_depth, |org| org.depth_limit());
                        if !policy.delegation_allowed || store::depth_of(tx, task)? >= limit {
                            return Ok((false, "Delegating further is not allowed here; do this part yourself.".to_string(), vec![]));
                        }
                        if let Some(org) = &chart {
                            if !org.may_delegate(me, to.id) {
                                return Ok((false, crate::org::refuse_delegation(org, me, to.id, &colleagues), vec![]));
                            }
                        }
                        let me_name = employees::load_employee(tx, me)?.map(|m| m.name).unwrap_or_default();
                        let note = format!("(Delegated by {me_name} as part of the task \"{}\".)", row.title);
                        let Some((child, title, mut notices)) = insert_part(tx, &st, row.project_id, Some(me), &to, &title, &goal, &criteria, &note, Some(task), None)? else {
                            return Ok((false, "Give the part a title.".to_string(), vec![]));
                        };
                        inherit_mode(tx, run, child, to.id)?;
                        notices.insert(1, task_event(tx, task, row.project_id, "task.delegated", serde_json::json!({ "child": child, "to": to.id, "subject": to.name, "title": title }))?);
                        Ok((true, format!("Delegated “{title}” to {}. Their result comes back in this conversation; finish your own part meanwhile.", to.name), notices))
                    }
                    EmployeeAction::AddIdea { .. }
                    | EmployeeAction::MessageTask { .. }
                    | EmployeeAction::ListTasks { .. }
                    | EmployeeAction::ReadTask { .. }
                    | EmployeeAction::ListConversations { .. }
                    | EmployeeAction::ReadConversation { .. }
                    | EmployeeAction::ProposeColleague { .. }
                    | EmployeeAction::ProposeInstructions { .. }
                    | EmployeeAction::SetMode { .. }
                    | EmployeeAction::ReadGroupChat { .. }
                    | EmployeeAction::PostToGroupChat { .. } => unreachable!("handled above"),
                    EmployeeAction::HandOver { employee, reason } => {
                        let Some(to) = find(&employee) else {
                            return Ok((false, format!("No active colleague called {employee} in the company."), vec![]));
                        };
                        if let Some(org) = crate::org::chart(tx)? {
                            if !org.may_hand_over(me, to.id) {
                                return Ok((false, crate::org::refuse_hand_over(&org, me, &colleagues), vec![]));
                            }
                        }
                        let handed: Vec<EmployeeId> = store::handoffs(tx, task)?.into_iter().filter_map(|h| h.from_id).collect();
                        if handed.contains(&to.id) || handed.len() as u32 >= task::MAX_HANDOFFS {
                            return Ok((false, "This task was already handed over; finish it yourself.".to_string(), vec![]));
                        }
                        let me_name = employees::load_employee(tx, me)?.map(|m| m.name).unwrap_or_default();
                        crate::task::task_line::set_employee(tx, task, Some(to.id))?;
                        inherit_mode(tx, run, task, to.id)?;
                        let mut notices = vec![task_event(
                            tx,
                            task,
                            row.project_id,
                            "task.handed_off",
                            serde_json::json!({ "from": me, "to": to.id, "from_name": me_name, "to_name": to.name, "reason": reason, "subject": to.name }),
                        )?];
                        crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?;
                        notices.push(task_event(tx, task, row.project_id, "task.queued", serde_json::json!({ "reason": "handed_off" }))?);
                        wake_task_job(tx, task, true)?;
                        Ok((true, format!("Handed the task over to {}. Tell the user in one sentence; stop working on it.", to.name), notices))
                    }
                }
            })
            .map_err(storage_error)
    }

}

/// `message_task`: an employee writes to a task it delegated, in that task's conversation,
/// labelled with its own name. With `interrupt` ("Send now") every unfinished run of the
/// conversation is stopped first; either way a new run answers the message, and a task
/// that stopped and waits for input goes on (as when the user types there).
fn message_task(tx: &Tx<'_>, st: &Store, run: RunId, needle: &str, message: &str, interrupt: bool) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let refuse = |why: &str| Ok((false, why.to_string(), vec![]));
    let caller: Option<(String, Option<String>, Option<String>)> = tx
        .query_row(
            "SELECT m.id, r.conversation_id, r.task_id FROM runs r LEFT JOIN conversations c ON c.id = r.conversation_id
             JOIN employees m ON m.id = COALESCE(r.employee_id, c.employee_id) WHERE r.id = ?1",
            [run.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    let Some((me, my_conversation, my_task)) = caller.and_then(|(m, c, t)| {
        Some((EmployeeId::parse(&m).ok()?, c.and_then(|c| ConversationId::parse(&c).ok()), t.and_then(|t| TaskId::parse(&t).ok())))
    }) else {
        return refuse("Only an employee can message a task.");
    };
    // In a task: the tasks of its project; in a chat: of any project.
    let project = run_owner(tx, run)?.and_then(|o| o.project);
    let message = message.trim();
    if message.is_empty() {
        return refuse("Write the message to send.");
    }
    let task = match crate::conversation::overview::find(tx, project, needle)? {
        Ok(task) => task,
        Err(why) => return Ok((false, why, vec![])),
    };
    let Some(row) = store::load_task(tx, task)? else { return refuse("The task no longer exists.") };
    // Only a task this employee delegated: as a part of its task, or from its conversation.
    let mut mine = (my_task.is_some() && row.parent == my_task) || (my_conversation.is_some() && store::task_origin(tx, task)? == my_conversation);
    // With the org chart on, the tasks of everyone below you count as yours too.
    let chart = crate::org::chart(tx)?;
    if let (Some(org), Some(owner)) = (&chart, row.employee_id) {
        mine = mine || org.may_delegate(me, owner);
    }
    if !mine {
        return refuse(if chart.is_some() {
            "You can only message a task you delegated yourself or a task of someone who reports to you; use list_tasks to find it."
        } else {
            "You can only message a task you delegated yourself (from your own task or conversation); use list_tasks to find it."
        });
    }
    if row.status.is_terminal() {
        return refuse("That task has already ended; delegate a new task instead.");
    }
    let Some(to) = row.employee_id else { return refuse("The task has no employee to receive the message.") };
    let Some((conversation, Some(owner))) = store::task_conversation(tx, task)? else {
        return refuse("That task has not started yet, so it has no conversation; put the information in a new task or try again shortly.");
    };
    if owner != to || Some(conversation) == my_conversation {
        return refuse("That task is being handed over; try again in a moment.");
    }
    let from = employees::load_employee(tx, me)?.map(|m| m.name).unwrap_or_else(|| "A colleague".into());
    let mut notices = Vec::new();
    let (posted_id, posted) = thread::post_returning(tx, st, conversation, task, Speaker::Update { kind: "message", from: &from }, message)?;
    notices.push(posted);
    if interrupt {
        notices.extend(crate::run::queue::interrupt_conversation(tx, &serde_json::json!({ "task_id": task, "from_employee": me }), &task.to_string(), conversation)?);
    }
    notices.extend(run_for(tx, task, conversation, posted_id)?);
    let how = if interrupt { "interrupted them and they are answering it now" } else { "queued it behind what they are doing" };
    Ok((true, format!("Sent your message to “{}” and {how}. Their answer comes back as the task's result, as before.", row.title), notices))
}

/// A colleague by name (any case) or id, never the employee itself.
fn find_colleague(colleagues: &[crate::task::tasks::Colleague], me: Option<EmployeeId>, name: &str) -> Option<crate::task::tasks::Colleague> {
    colleagues.iter().find(|c| (c.name.trim().eq_ignore_ascii_case(name.trim()) || c.id.to_string() == name.trim()) && Some(c.id) != me).cloned()
}

/// How far an employee may let others work for it.
fn task_policy(tx: &Tx<'_>, employee: EmployeeId) -> aurelius_domain::employee::TaskPolicy {
    let config: Option<String> = tx.query_row("SELECT config_json FROM employees WHERE id = ?1", [employee.to_string()], |r| r.get(0)).ok();
    config.and_then(|c| serde_json::from_str::<aurelius_domain::employee::EmployeeConfig>(&c).ok()).map(|c| c.task_policy).unwrap_or_default()
}

/// Work given away keeps the mode the giver works in now, so a colleague's
/// task conversation starts in it. Ask and Plan are not passed on: the
/// colleague's conversation then starts as any of its new ones does. Nothing
/// goes to management that chooses its own mode: it picks Auto or Plan itself.
fn inherit_mode(tx: &Tx<'_>, run: RunId, task: TaskId, to: EmployeeId) -> Result<(), StorageError> {
    use aurelius_domain::autonomy::AgentMode;
    let mode = match aurelius_storage::conversations::mode_for_run(tx, run)? {
        mode @ (AgentMode::Auto | AgentMode::AcceptEdits) => Some(mode),
        AgentMode::Ask | AgentMode::Plan => None,
    };
    let mode = if aurelius_storage::management_mode::chooses_own_mode(tx, to)? { None } else { mode };
    let mut settings = store::task_run(tx, task)?;
    settings.mode = mode;
    store::set_task_run(tx, task, &settings)
}

/// The task job runs (again) soon; a new job when it has none to wake (`or_new`).
pub(crate) fn wake_task_job(tx: &Tx<'_>, task: TaskId, or_new: bool) -> Result<(), StorageError> {
    let woken = tx.execute(
        "UPDATE jobs SET state = 'ready', not_before_ms = ?2, lease_owner = NULL, lease_until_ms = NULL, last_error_code = NULL WHERE id = ?1 AND state IN ('done','ready')",
        rusqlite::params![task.to_string(), aurelius_storage::now_ms()],
    )?;
    if woken == 0 && or_new {
        let (event_id, _) = crate::run::events::emit(tx, ev("task", task.to_string(), "task.job_requeued", ActorKind::Employee, task.to_string(), None, serde_json::json!({})))?;
        aurelius_storage::events::insert_job(tx, &task.to_string(), crate::task::tasks::TASK_JOB_KIND, event_id, &format!("task:{task}:{}", aurelius_domain::EventId::new()), &serde_json::json!({ "task_id": task }))?;
    }
    Ok(())
}

/// A queued task for colleague `to`, given by `me`: a part of `parent`, or asked
/// from conversation `origin`. Returns the task, its title and the notices
/// (task.created, task.queued); nothing when the title is empty.
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_part(
    tx: &Tx<'_>,
    st: &Store,
    project: aurelius_domain::ProjectId,
    me: Option<EmployeeId>,
    to: &crate::task::tasks::Colleague,
    title: &str,
    goal: &str,
    criteria: &[String],
    note: &str,
    parent: Option<TaskId>,
    origin: Option<ConversationId>,
) -> Result<Option<(TaskId, String, Vec<EventNotice>)>, StorageError> {
    let title = title.trim().chars().take(task::TITLE_MAX).collect::<String>();
    if title.is_empty() {
        return Ok(None);
    }
    let goal = if goal.trim().is_empty() { title.clone() } else { goal.trim().to_owned() };
    let goal = format!("{goal}\n\n{note}");
    let criteria: Vec<String> = criteria.iter().map(|c| c.trim().to_owned()).filter(|c| !c.is_empty()).take(task::MAX_CRITERIA).collect();
    let goal_blob = st.blobs().put(goal.as_bytes(), "text/plain; charset=utf-8")?;
    let criteria_blob = st.blobs().put_json(&criteria)?;
    blobs::register(tx, &goal_blob)?;
    blobs::register(tx, &criteria_blob)?;
    let child = TaskId::new();
    store::insert_task(tx, child, project, Some(to.id), &title, &goal_blob.id.to_string(), &criteria_blob.id.to_string(), TaskStatus::Queued, parent)?;
    if let Some(conversation) = origin {
        store::set_task_origin(tx, child, conversation, me)?;
    }
    let correlation = parent.map_or_else(|| child.to_string(), |p| p.to_string());
    let (event_id, mut created) = crate::run::events::emit(tx, crate::run::events::Ev {
        aggregate_type: "task",
        aggregate_id: child.to_string(),
        event_type: "task.created",
        actor: ActorKind::Employee,
        actor_id: me.map(|m| m.to_string()),
        correlation: correlation.clone(),
        payload: Some(goal_blob.id.to_string()),
        metadata: serde_json::json!({ "title": title, "employee_id": to.id, "parent_task_id": parent, "sub_task": parent.is_some(), "requested_by": me, "origin_conversation_id": origin }),
        dedupe: format!("delegate:{correlation}:{child}"),
    })?;
    created.project_id = Some(project);
    let notices = vec![created, task_event(tx, child, project, "task.queued", serde_json::json!({}))?];
    aurelius_storage::events::insert_job(tx, &child.to_string(), crate::task::tasks::TASK_JOB_KIND, event_id, &format!("task:{child}"), &serde_json::json!({ "task_id": child }))?;
    Ok(Some((child, title, notices)))
}

/// What a bridge run belongs to: the employee (none for the assistant of a
/// project conversation), the project (none in an employee's chat), and the
/// conversation it answers in.
pub(crate) struct RunOwner {
    pub employee: Option<EmployeeId>,
    pub project: Option<aurelius_domain::ProjectId>,
    pub conversation: Option<ConversationId>,
}

pub(crate) fn run_owner(conn: &rusqlite::Connection, run: RunId) -> Result<Option<RunOwner>, StorageError> {
    use rusqlite::OptionalExtension;
    let row: Option<(Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT COALESCE(r.employee_id, c.employee_id), COALESCE(c.project_id, (SELECT t.project_id FROM tasks t WHERE t.id = r.task_id)), r.conversation_id
             FROM runs r LEFT JOIN conversations c ON c.id = r.conversation_id WHERE r.id = ?1",
            [run.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    Ok(row.and_then(|(employee, project, conversation)| {
        let employee = employee.and_then(|m| EmployeeId::parse(&m).ok());
        let project = project.and_then(|p| aurelius_domain::ProjectId::parse(&p).ok());
        // Neither an employee nor a project: not a run of the company (a plain chat).
        if employee.is_none() && project.is_none() {
            return None;
        }
        Some(RunOwner { employee, project, conversation: conversation.and_then(|c| ConversationId::parse(&c).ok()) })
    }))
}

/// A project by its name (any case) or id that is not archived; the refusal
/// text names the projects there are.
pub(crate) fn find_project(conn: &rusqlite::Connection, needle: &str) -> Result<Result<aurelius_domain::ProjectId, String>, StorageError> {
    let needle = needle.trim();
    let mut stmt = conn.prepare("SELECT id, name FROM projects WHERE status <> 'archived' ORDER BY created_at_ms, id")?;
    let projects: Vec<(String, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    let found = projects.iter().find(|(id, name)| id == needle || name.trim().eq_ignore_ascii_case(needle));
    Ok(match found.and_then(|(id, _)| aurelius_domain::ProjectId::parse(id).ok()) {
        Some(p) => Ok(p),
        None => Err(format!("No project called {needle}. Projects: {}.", project_names(&projects))),
    })
}

fn project_names(projects: &[(String, String)]) -> String {
    if projects.is_empty() { "none yet".to_string() } else { projects.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join(", ") }
}

/// The answer when work outside a project names no project.
pub(crate) fn need_project(conn: &rusqlite::Connection, what: &str) -> Result<String, StorageError> {
    let mut stmt = conn.prepare("SELECT id, name FROM projects WHERE status <> 'archived' ORDER BY created_at_ms, id")?;
    let projects: Vec<(String, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    Ok(format!("You are not in a project here: name the project for {what} with project (its name). Projects: {}.", project_names(&projects)))
}

/// Delegate from a conversation that is not a task's: from a project conversation
/// (the assistant gives an employee work), an employee's chat with the user (it
/// names the project) or its heartbeat. The colleague gets a task of its own,
/// whose result comes back in that conversation.
fn delegate_from_conversation(tx: &Tx<'_>, st: &Store, run: RunId, action: EmployeeAction) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let EmployeeAction::Delegate { employee, title, goal, criteria, project: named } = action else {
        return Ok((false, "Handing over only works for a task you work on. Outside a task, give work to a colleague with delegate_task.".to_string(), vec![]));
    };
    let Some(RunOwner { employee: me, project, conversation: Some(conversation) }) = run_owner(tx, run)? else {
        return Ok((false, "Only an employee or the project assistant can give work to an employee.".to_string(), vec![]));
    };
    // A project conversation gives work in its project; an employee's chat says where.
    let project = match (project, named.as_deref().map(str::trim).filter(|n| !n.is_empty())) {
        (Some(p), _) => p,
        (None, Some(named)) => match find_project(tx, named)? {
            Ok(p) => p,
            Err(why) => return Ok((false, why, vec![])),
        },
        (None, None) => return Ok((false, need_project(tx, "the task")?, vec![])),
    };
    if !matches!(employees::project_scope(tx, project)?, Some((_, aurelius_domain::employee::LifecycleStatus::Active))) {
        return Ok((false, "That project is paused or archived; it takes no new work.".to_string(), vec![]));
    }
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let colleagues = crate::task::tasks::colleagues(tx, &read)?;
    let Some(to) = find_colleague(&colleagues, me, &employee) else {
        return Ok((false, format!("No active colleague called {employee} in the company."), vec![]));
    };
    let asked_by = match me {
        Some(me) => {
            let policy = task_policy(tx, me);
            // The project assistant (no employee) is not on the chart and stays unrestricted.
            let chart = crate::org::chart(tx)?;
            let limit = chart.as_ref().map_or(policy.max_depth, |org| org.depth_limit());
            if !policy.delegation_allowed || limit == 0 {
                return Ok((false, "Delegating is not allowed for you; do this yourself.".to_string(), vec![]));
            }
            if let Some(org) = &chart {
                if !org.may_delegate(me, to.id) {
                    return Ok((false, crate::org::refuse_delegation(org, me, to.id, &colleagues), vec![]));
                }
            }
            employees::load_employee(tx, me)?.map(|m| m.name).unwrap_or_default()
        }
        None => "The project assistant".to_string(),
    };
    let note = format!("(Asked by {asked_by} from a conversation with the user.)");
    let Some((child, title, notices)) = insert_part(tx, st, project, me, &to, &title, &goal, &criteria, &note, None, Some(conversation))? else {
        return Ok((false, "Give the task a title.".to_string(), vec![]));
    };
    inherit_mode(tx, run, child, to.id)?;
    Ok((true, format!("Gave “{title}” to {} as a task of their own. Their result comes back in this conversation; tell the user and go on meanwhile.", to.name), notices))
}

impl CommandService {
    /// A step that must hold the task job's lease.
    fn fenced_task<R: Send + 'static>(&self, task: TaskId, epoch: i64, f: impl FnOnce(&Tx<'_>) -> Result<R, StorageError> + Send + 'static) -> Result<Option<R>, AppError> {
        self.store
            .write(move |tx| {
                if !aurelius_storage::queries::holds_lease(tx, &task.to_string(), epoch)? {
                    return Ok(None);
                }
                f(tx).map(Some)
            })
            .map_err(storage_error)
    }
}
