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
    ActorKind, AppError, ConversationId, EventNotice, EmployeeId, MessageId, RunId, TaskId, TicketId,
    task::{self, TaskStatus},
};
use aurelius_storage::{StorageError, Store, Tx, blobs, employees, tasks as store};
use rusqlite::OptionalExtension;

/// How an employee works on a task in its conversation (system text).
pub const TASK_GUIDANCE: &str = "This conversation is a task you took on. Start with one or two sentences on how you will approach it. Do the work yourself with your own tools; use subagents (the Task tool) only for a broad search across many files, not by default. You can give a part to a colleague with delegate_task (you get their result here) or hand the whole task over with hand_over_task. Message a part you gave away with message_task (it interrupts them by default). Everything you do is visible in this conversation, so keep your closing answer short: say whether it worked, in one or two sentences, and what the user must still do or decide, if anything. No report, no headings, and do not repeat what is already above it (steps, tool calls, files, screenshots, ids). That closing answer is the task's result.";

/// What an employee is told when the colleagues' results are in.
const COLLEAGUES_DONE: &str = "Your colleagues finished their parts (their results are above). Bring everything together and finish with your report.";

/// What the owner is told when the colleagues it handed work to are done.
const HANDOFFS_DONE: &str = "The colleagues you handed work to are done; read what they wrote in the ticket's discussion. If a review was rejected or anything is still open, deal with it first. Only when everything is done, finish with your report for the board member.";
/// The same, when a colleague's run failed or was stopped before it had answered.
const HANDOFF_BROKEN: &str = "A colleague you handed work to did not finish: their run failed or was stopped, or their message was never delivered. Read the ticket's discussion, take the work back or hand it to someone else, and finish with your report for the board member.";

/// `closing_text` does not stop at a closing shorter than this.
const CLOSING_MIN_CHARS: usize = 80;

/// What the discussion says when an answer about a report was stopped.
const ANSWER_STOPPED: &str = "My answer was stopped before it was finished. Send your message again if you still need it.";
/// What the discussion says when the run of an answer about a report could not start.
const ANSWER_NOT_STARTED: &str = "I could not start on your message. Send it again.";

/// Why a main task waits after a run that ended on a question for the board member.
const WAITING_QUESTION: &str = "question";

/// A run of the task's conversation still going (other than `except`).
pub(crate) fn live_run(conn: &rusqlite::Connection, task: TaskId, except: Option<RunId>) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM runs WHERE task_id = ?1 AND conversation_id IS NOT NULL AND id <> COALESCE(?2, '')
           AND status IN ('created','starting','running','waiting','cancelling'))
         OR EXISTS(SELECT 1 FROM employee_inbox WHERE task_id = ?1 AND state = 'queued' AND run_id <> COALESCE(?2, ''))",
        rusqlite::params![task.to_string(), except.map(|r| r.to_string())],
        |r| r.get(0),
    )?)
}

/// Queue a run of the task's conversation that answers `reply_to`, counted for the task.
pub(crate) fn run_for(tx: &Tx<'_>, task: TaskId, conversation: ConversationId, reply_to: MessageId) -> Result<Vec<EventNotice>, StorageError> {
    run_for_at(tx, task, conversation, reply_to, aurelius_storage::inbox::task_hop(tx, task)?)
}

/// `run_for`, at `hop` (R4): a colleague's `message_task` counts one on from its own run.
fn run_for_at(tx: &Tx<'_>, task: TaskId, conversation: ConversationId, reply_to: MessageId, hop: u32) -> Result<Vec<EventNotice>, StorageError> {
    let mut meta = serde_json::json!({ "task_id": task });
    if hop > 0 {
        meta["hop"] = hop.into();
    }
    let (run, notices) = queue_run(tx, &meta, &task.to_string(), conversation, reply_to)?;
    crate::run::inbox::link_task(tx, run, task)?;
    Ok(notices)
}

/// The queuing metadata of a run that answers a result coming back: a result counts
/// no hop, so the run goes on at the hop of the run that delegated (R4).
fn reported_back(tx: &Tx<'_>, task: TaskId) -> Result<serde_json::Value, StorageError> {
    let hop = aurelius_storage::inbox::task_hop(tx, task)?.saturating_sub(1);
    Ok(serde_json::json!({ "origin": "delegated", "task_id": task, "hop": hop }))
}

/// The hop of a delegation from `run`, or why it is refused (R4; the ticket's limit,
/// because work always belongs to a ticket).
fn delegation_hop(tx: &Tx<'_>, run: RunId) -> Result<Result<u32, String>, StorageError> {
    let hop = aurelius_storage::inbox::run_hop(tx, run)? + 1;
    Ok(if hop > aurelius_domain::group_chat::TICKET_MAX_HOPS {
        Err(format!("Delegating further is not allowed now: colleagues already passed this on {} times after a board member's message. Do this part yourself or wait for a board member.", aurelius_domain::group_chat::TICKET_MAX_HOPS))
    } else {
        Ok(hop)
    })
}

/// Called when a run is queued in a conversation: a run in a live task's
/// conversation (of the task's own employee) counts for the task; a task that
/// waited for the user goes on (its notices go with the run's).
pub(crate) fn link_run(tx: &Tx<'_>, run: RunId, conversation: ConversationId) -> Result<Vec<EventNotice>, StorageError> {
    let Some(task) = store::task_for_conversation(tx, conversation)? else { return Ok(vec![]) };
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    let owner = store::task_conversation(tx, task)?.and_then(|(_, m)| m);
    if row.status.is_terminal() || owner.is_none() || owner != row.employee_id {
        return Ok(vec![]);
    }
    crate::run::inbox::link_task(tx, run, task)?;
    let mut notices = Vec::new();
    if row.status == TaskStatus::Waiting && matches!(store::waiting_cause(tx, task)?.as_deref(), Some("stopped" | "not_started" | "escalated_to_boss" | WAITING_QUESTION)) {
        notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?);
        notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?);
        notices.push(task_event(tx, task, row.project_id, "task.resumed", serde_json::json!({ "reason": "conversation" }))?);
    }
    Ok(notices)
}

/// A run was queued, started or ended in the conversation of a finished task
/// (its employee's own) or one in Review: the task keeps its status, but
/// `TaskView::live_run` may have changed, so clients are told to read the task again.
pub(crate) fn live_run_notice(tx: &Tx<'_>, conversation: ConversationId) -> Result<Vec<EventNotice>, StorageError> {
    let Some(task) = store::task_for_conversation(tx, conversation)? else { return Ok(vec![]) };
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    let owner = store::task_conversation(tx, task)?.and_then(|(_, m)| m);
    if !(row.status.is_terminal() || row.status == TaskStatus::Review) || owner.is_none() || owner != row.employee_id {
        return Ok(vec![]);
    }
    Ok(vec![task_event(tx, task, row.project_id, "task.run_changed", serde_json::json!({ "conversation_id": conversation }))?])
}

/// A run ended: what it means for the task it belongs to.
pub(crate) fn on_run_finished(tx: &Tx<'_>, st: &Store, run: RunId, completed: bool, failed: Option<&str>, answer: &str) -> Result<Vec<EventNotice>, StorageError> {
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
    // Ended or handed over meanwhile: nothing to do.
    if row.status.is_terminal() || handed_over {
        return Ok(vec![]);
    }
    let another_live = live_run(tx, task, Some(run))?;
    // An answer about a report that fails, stops or has a run behind it: the discussion
    // hears of it here, and the report stays what the board member decides on.
    if row.status == TaskStatus::Review && (another_live || failed.is_some() || !completed) && crate::task::ticket_line::is_root(tx, task)? {
        let text = match (failed, completed) {
            (Some(why), _) => format!("I could not finish my answer ({why}). Send your message again."),
            (None, false) => ANSWER_STOPPED.to_owned(),
            (None, true) => closing_text(answer, &aurelius_storage::conversations::tool_offsets_of_run(tx, run)?).to_owned(),
        };
        if let Some(answered) = crate::task::ticket_line::answered_in_review(tx, st, task, run, &text)? {
            return Ok(answered);
        }
    }
    // Another run of it is still going: that one's end decides.
    if another_live {
        return Ok(vec![]);
    }
    let mut notices = Vec::new();
    if let Some(why) = failed {
        notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Failed, None)?);
        // With the org chart on, the employee's boss gets it first; "Needs you" then shows it as handled.
        let up = crate::org::escalate(tx, st, task, &row, &format!("it failed ({why})"), answer)?;
        let mut failed_meta = serde_json::json!({ "reason": why });
        if let Some((boss, _)) = &up {
            failed_meta["escalated_to"] = serde_json::json!(boss);
        }
        let failed_notice = task_event(tx, task, row.project_id, "task.failed", failed_meta)?;
        // A failed task teaches too: the cheap extraction looks at what went wrong (lessons).
        // Not below a ticket: the ticket's one extraction after Done covers its tasks (MU2-12 AC3).
        let source: String = tx.query_row("SELECT id FROM events WHERE seq = ?1", [failed_notice.seq as i64], |r| r.get(0))?;
        if let (Ok(source), None) = (aurelius_domain::EventId::parse(&source), crate::task::ticket_line::ticket_of(tx, task)?) {
            crate::memory::schedule_extraction(tx, task, None, source)?;
        }
        notices.push(failed_notice);
        notices.extend(up.map(|(_, n)| n).unwrap_or_default());
        notices.extend(after_ended(tx, st, task, &format!("“{}” failed ({why}).", row.title), false)?);
        return Ok(notices);
    }
    if !completed {
        // Stopped by the user: the task waits until they continue in its conversation.
        notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("input"))?);
        let up = crate::org::escalate(tx, st, task, &row, "it was stopped and waits for input", answer)?;
        let waiting = match &up {
            Some((boss, _)) => serde_json::json!({ "reason": "escalated_to_boss", "boss": boss }),
            None => serde_json::json!({ "reason": "stopped" }),
        };
        notices.push(task_event(tx, task, row.project_id, "task.waiting", waiting)?);
        notices.extend(up.map(|(_, n)| n).unwrap_or_default());
        return Ok(notices);
    }
    if open_parts(tx, task)? {
        // Colleagues still work on parts: it continues when they are done.
        notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("dependency"))?);
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
    // A ticket's plan (Plan mode) waits for the board before anything is reported.
    if let Some(review) = crate::task::plan_review::on_finished(tx, run)? {
        notices.extend(review);
        return Ok(notices);
    }
    // A ticket's board member reads the closing text; the working notes stay in the task's conversation.
    if crate::task::ticket_line::is_root(tx, task)? {
        let closing = closing_text(answer, &aurelius_storage::conversations::tool_offsets_of_run(tx, run)?);
        if row.status == TaskStatus::Review
            && let Some(answered) = crate::task::ticket_line::answered_in_review(tx, st, task, run, closing)?
        {
            notices.extend(answered);
            return Ok(notices);
        }
        // Only the owner's `report_ready` brings the ticket to Review; what the run says decides nothing.
        let report = reported_in_run(tx, st, run)?;
        let closing = report.as_deref().unwrap_or(closing);
        // The work may lie with a colleague (a handoff, a run in the discussion): the ticket is not
        // ready for the board member then. It stays in Doing until the last of them has ended.
        if ticket_busy(tx, task, Some(run))? {
            notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("dependency"))?);
            notices.push(task_event(tx, task, row.project_id, "task.waiting", serde_json::json!({ "reason": "handed_off" }))?);
            return Ok(notices);
        }
        // A run that ends without it has not finished, with a question mark or without: the ticket
        // stays in Doing, the board member is told, and their answer wakes the owner.
        if row.status != TaskStatus::Review && report.is_none() {
            notices.extend(wait_for_answer(tx, st, task, &row, run, closing)?);
            return Ok(notices);
        }
        notices.extend(complete(tx, st, task, closing)?);
        return Ok(notices);
    }
    notices.extend(complete(tx, st, task, answer)?);
    Ok(notices)
}

/// Longest report `report_ready` takes, in characters.
pub const REPORT_READY_MAX: usize = 20_000;
/// The event that records a `report_ready` call: the run and the blob of its report.
const REPORT_READY_EVENT: &str = "task.report_ready";
/// What the discussion says when the owner's run ended without `report_ready` and without a word of its own.
const NO_REPORT_NOTE: &str = "I stopped without reporting the work as ready, so the ticket stays in Doing. I am waiting for your input.";

/// `report_ready` of a run: the owner says the work is ready for Review. Only a live run of a
/// ticket's main task whose card has not been reported yet may (a tool call is never trusted).
/// Nothing moves now: the report is kept with the run, and its end decides (so an open handoff,
/// delegation or run still holds the ticket in Doing). Returns (ok, text for the model, notices).
pub(crate) fn report_ready(tx: &Tx<'_>, st: &Store, run: RunId, report: &str) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let refuse = |why: &str| Ok((false, why.to_owned(), vec![]));
    let Some((task, row)) = live_task_of(tx, run)? else { return refuse("report_ready is only for the owner's run of a ticket's main task.") };
    let Some((_, _, column)) = crate::task::ticket_line::root_of(tx, task)? else { return refuse("report_ready is only for the main task of a ticket.") };
    if matches!(column, aurelius_domain::ticket::ColumnKind::Review | aurelius_domain::ticket::ColumnKind::Testing | aurelius_domain::ticket::ColumnKind::Done) {
        return refuse("This ticket is already reported. Answer in the discussion instead.");
    }
    let report = report.trim();
    if report.is_empty() {
        return refuse("Give the report: what you did, what you checked and what is still open.");
    }
    if report.chars().count() > REPORT_READY_MAX {
        return refuse(&format!("The report is too long: at most {REPORT_READY_MAX} characters."));
    }
    let blob = st.blobs().put(report.as_bytes(), "text/markdown; charset=utf-8")?;
    blobs::register(tx, &blob)?;
    let notice = task_event(tx, task, row.project_id, REPORT_READY_EVENT, serde_json::json!({ "run_id": run, "blob_id": blob.id.to_string() }))?;
    Ok((true, "Report recorded. Finish your answer now without asking anything; the ticket goes to Review when your run ends and nothing else is open on it.".to_owned(), vec![notice]))
}

/// The report the run gave with `report_ready` (the last one), if it did.
fn reported_in_run(tx: &Tx<'_>, st: &Store, run: RunId) -> Result<Option<String>, StorageError> {
    let key: Option<String> = tx
        .query_row(
            "SELECT b.storage_key FROM events v JOIN blobs b ON b.id = json_extract(v.metadata_json, '$.blob_id')
             WHERE v.aggregate_type = 'task' AND v.type = ?1 AND json_extract(v.metadata_json, '$.run_id') = ?2
             ORDER BY v.seq DESC LIMIT 1",
            rusqlite::params![REPORT_READY_EVENT, run.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(key.and_then(|k| st.blobs().read(&k).ok()).and_then(|b| String::from_utf8(b).ok()))
}

/// The run ended without `report_ready`: what it said (a question, or that it needs input) goes to the
/// discussion (unless the run posted there itself) and the main task waits for the answer; the card stays in Doing. The answer resumes it.
fn wait_for_answer(tx: &Tx<'_>, st: &Store, task: TaskId, row: &store::TaskRow, run: RunId, closing: &str) -> Result<Vec<EventNotice>, StorageError> {
    let mut notices = Vec::new();
    if let Some(ticket) = crate::task::ticket_line::ticket_of(tx, task)? {
        let said = if closing.trim().is_empty() { NO_REPORT_NOTE } else { closing };
        notices.extend(crate::conversation::discussion::owner_answer(tx, st, ticket, run, said)?);
    }
    notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("input"))?);
    notices.push(task_event(tx, task, row.project_id, "task.waiting", serde_json::json!({ "reason": WAITING_QUESTION }))?);
    // What comes in for this thread now waits until the answer has been dealt with (R3).
    crate::run::inbox::park_on_question(tx, run, task)?;
    Ok(notices)
}

/// The end of `answer` that stands on its own: from the last tool use on
/// (`tool_offsets`, byte offsets in order), going back further while that says
/// too little, such as "Done." after a last tool call that follows the real text.
fn closing_text<'a>(answer: &'a str, tool_offsets: &[u32]) -> &'a str {
    for offset in tool_offsets.iter().rev().map(|o| *o as usize) {
        // The final redaction may have moved the text a little: an offset that no longer fits is skipped.
        if let Some(tail) = answer.get(offset..).map(str::trim)
            && tail.chars().count() >= CLOSING_MIN_CHARS
        {
            return tail;
        }
    }
    answer.trim()
}

/// A run of the task's conversation could not be started (its retries are
/// used up): nothing of it runs any more, so the task waits for the user, who
/// sends the message again in its conversation (as after a stop).
pub(crate) fn on_run_not_started(tx: &Tx<'_>, st: &Store, run: RunId) -> Result<Vec<EventNotice>, StorageError> {
    let mut notices = task_not_started(tx, st, run)?;
    // A colleague's handoff that never ran: the owner must not keep waiting for it.
    notices.extend(run_settled(tx, st, run, true)?);
    Ok(notices)
}

fn task_not_started(tx: &Tx<'_>, st: &Store, run: RunId) -> Result<Vec<EventNotice>, StorageError> {
    let task: Option<String> = tx.query_row("SELECT task_id FROM runs WHERE id = ?1 AND conversation_id IS NOT NULL", [run.to_string()], |r| r.get(0)).ok().flatten();
    let Some(task) = task.and_then(|t| TaskId::parse(&t).ok()) else { return Ok(vec![]) };
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    let employee: Option<String> = tx.query_row("SELECT c.employee_id FROM runs r JOIN conversations c ON c.id = r.conversation_id WHERE r.id = ?1", [run.to_string()], |r| r.get(0)).ok().flatten();
    let handed_over = employee.and_then(|m| EmployeeId::parse(&m).ok()) != row.employee_id;
    // A message about a report that gets no answer: the discussion says so, the ticket stays in Review.
    if row.status == TaskStatus::Review && !handed_over && crate::task::ticket_line::is_root(tx, task)?
        && let Some(answered) = crate::task::ticket_line::answered_in_review(tx, st, task, run, ANSWER_NOT_STARTED)?
    {
        return Ok(answered);
    }
    if row.status != TaskStatus::Running || handed_over || live_run(tx, task, Some(run))? {
        return Ok(vec![]);
    }
    let mut notices = crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("input"))?.into_vec();
    let up = crate::org::escalate(tx, st, task, &row, "its run could not be started", "")?;
    let waiting = match &up {
        Some((boss, _)) => serde_json::json!({ "reason": "escalated_to_boss", "boss": boss }),
        None => serde_json::json!({ "reason": "not_started" }),
    };
    notices.push(task_event(tx, task, row.project_id, "task.waiting", waiting)?);
    notices.extend(up.map(|(_, n)| n).unwrap_or_default());
    Ok(notices)
}

/// The task of `run` (still going, not handed over), if there is one.
pub(crate) fn live_task_of(tx: &Tx<'_>, run: RunId) -> Result<Option<(TaskId, store::TaskRow)>, StorageError> {
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
    // The ticket's discussion says so once per wait, with whose limit (MU3-11, ui-ontwerp §5).
    let mut notices = match crate::task::ticket_line::ticket_of(tx, task)? {
        Some(ticket) => crate::run::login_wait::quota_paused(tx, ticket, run, until_ms)?,
        None => Vec::new(),
    };
    notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("quota"))?.into_vec());
    notices.push(task_event(tx, task, row.project_id, "task.waiting", serde_json::json!({ "reason": "quota", "until_ms": until_ms, "login_user": crate::run::login::of_run(tx, run)? }))?);
    Ok(notices)
}

/// A run that waited for the limit was stopped before it went again: its task
/// no longer waits for the reset but, as after any stop, for the user.
pub(crate) fn on_parked_run_stopped(tx: &Tx<'_>, run: RunId) -> Result<Vec<EventNotice>, StorageError> {
    let Some((task, row)) = live_task_of(tx, run)? else { return Ok(vec![]) };
    if row.status != TaskStatus::Waiting || store::waiting_cause(tx, task)?.as_deref() != Some("quota") || live_run(tx, task, Some(run))? {
        return Ok(vec![]);
    }
    let mut notices = crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("input"))?.into_vec();
    notices.push(task_event(tx, task, row.project_id, "task.waiting", serde_json::json!({ "reason": "stopped" }))?);
    Ok(notices)
}

/// A run that waited for the limit starts again: so does its task.
pub(crate) fn on_run_quota_resumed(tx: &Tx<'_>, run: RunId) -> Result<Vec<EventNotice>, StorageError> {
    let Some((task, row)) = live_task_of(tx, run)? else { return Ok(vec![]) };
    if row.status != TaskStatus::Waiting || store::waiting_cause(tx, task)?.as_deref() != Some("quota") {
        return Ok(vec![]);
    }
    let mut notices = crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?.into_vec();
    notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?);
    notices.push(task_event(tx, task, row.project_id, "task.resumed", serde_json::json!({ "reason": "quota" }))?);
    if let Some(ticket) = crate::task::ticket_line::ticket_of(tx, task)? {
        notices.extend(crate::run::login_wait::quota_resumed(tx, ticket)?);
    }
    Ok(notices)
}

/// The task reported (the impact analysis' `report()`): the report is kept and
/// the task goes to Review. The main task of a ticket stops there, for a board
/// member to accept or send back (`ticket_line::reported`); any other task (a
/// delegation, a task without a ticket) is accepted at once, as before.
///
/// A message about a main task in Review gives a run that answers it; its answer
/// is no new report (`ticket_line::answered_in_review`). Only while the card is in
/// Testing does its end report again, which tests it again.
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
            notices.extend(crate::task::task_line::set_status(tx, task, to, None)?);
        }
    }
    if crate::task::ticket_line::is_root(tx, task)? {
        notices.extend(crate::task::ticket_line::reported(tx, st, task)?);
        return Ok(notices);
    }
    notices.extend(accept(tx, st, task, &report, None)?);
    Ok(notices)
}

/// A task in Review succeeds: automatically after its report (`by` none), or
/// accepted by a board member (`ticket.accept`). Then what follows a success:
/// the memory extraction (for a ticket: its summary after Done), the tasks
/// waiting on it go, and its result goes to whoever gave it.
pub(crate) fn accept(tx: &Tx<'_>, st: &Store, task: TaskId, report: &str, by: Option<aurelius_domain::UserId>) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    let mut notices = Vec::new();
    notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Succeeded, None)?);
    let meta = match by {
        None => serde_json::json!({ "reason": "reported" }),
        Some(user) => serde_json::json!({ "reason": "accepted", "by": aurelius_domain::ticket::Actor::BoardMember { user_id: user } }),
    };
    let succeeded = task_event(tx, task, row.project_id, "task.succeeded", meta)?;
    let source: String = tx.query_row("SELECT id FROM events WHERE seq = ?1", [succeeded.seq as i64], |r| r.get(0))?;
    // Accepted on the board (Done): the main task's extraction is the ticket's one
    // extraction and writes its summary (MU2-12). The other tasks of a ticket get
    // none of their own (AC3); a task without a ticket does, as before.
    let ticket = match by {
        Some(_) => crate::task::ticket_line::root_of(tx, task)?.map(|(ticket, _, _)| ticket),
        None => None,
    };
    let extract = ticket.is_some() || crate::task::ticket_line::ticket_of(tx, task)?.is_none();
    if let (Ok(source), true) = (aurelius_domain::EventId::parse(&source), extract) {
        crate::memory::schedule_extraction(tx, task, ticket, source)?;
    }
    notices.push(succeeded);
    notices.extend(after_ended(tx, st, task, &report, true)?);
    Ok(notices)
}

/// A sub-task ended: its result goes to the employee who delegated it; when
/// nothing else is open, that employee continues (and writes its report).
/// A task an employee gave from one of its conversations: the result goes to
/// that conversation, and the employee answers on it.
fn after_ended(tx: &Tx<'_>, st: &Store, task: TaskId, text: &str, succeeded: bool) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    // A test task's end moves the card itself (`testing`); the owner gets no result of it.
    if crate::task::testing::is_test_task(tx, task)? {
        return Ok(vec![]);
    }
    let colleague = row.employee_id.and_then(|m| employees::load_employee(tx, m).ok().flatten()).map(|m| m.name).unwrap_or_else(|| "A colleague".into());
    let finding = if succeeded { format!("Finished “{}”:\n\n{}", row.title, text.trim()) } else { text.to_owned() };
    let speaker = Speaker::Update { kind: if succeeded { "finding" } else { "status" }, from: &colleague };
    if let Some(parent) = row.parent {
        // Below a ticket: a short line in its discussion too (MU2-03).
        let mut notices = crate::task::ticket_line::sub_task_line(tx, task, &colleague, &row.title, text, succeeded)?;
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
        let (_, queued) = queue_run(tx, &reported_back(tx, task)?, &task.to_string(), conversation, message)?;
        notices.extend(queued);
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
    let (_, queued) = queue_run(tx, &reported_back(tx, task)?, &task.to_string(), conversation, latest)?;
    Ok(queued)
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
    let (_, queued) = queue_run(tx, &reported_back(tx, task)?, &task.to_string(), conversation, message)?;
    Ok(queued)
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
           AND status IN ('created','starting','running','waiting','cancelling'))
         OR EXISTS(SELECT 1 FROM employee_inbox WHERE conversation_id = ?1 AND state = 'queued' AND run_id <> COALESCE(?2, ''))",
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

/// Parts of the task colleagues still work on. A ticket's test task is no part
/// of its main task: the main task keeps status Review while the tests run.
fn open_parts(tx: &Tx<'_>, task: TaskId) -> Result<bool, StorageError> {
    for child in store::children_of(tx, task)? {
        if !child.status.is_terminal() && !crate::task::testing::is_test_task(tx, child.id)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Work on the task's ticket (a handoff, a run in its discussion) is open besides `except`.
fn ticket_busy(tx: &Tx<'_>, task: TaskId, except: Option<RunId>) -> Result<bool, StorageError> {
    match crate::task::ticket_line::ticket_of(tx, task)? {
        Some(ticket) => aurelius_storage::inbox::open_on_ticket(tx, ticket, except),
        None => Ok(false),
    }
}

/// A task waiting for its colleagues goes on once none of them is still at work.
pub(crate) fn continue_when_done(tx: &Tx<'_>, st: &Store, task: TaskId) -> Result<Vec<EventNotice>, StorageError> {
    continue_with(tx, st, task, COLLEAGUES_DONE, "colleagues_done")
}

/// A run, handoff or message of `ticket` ended or was cancelled (`except`: that run): when the
/// main task waits for it and nothing else is open, the owner goes on. `broken`: it did not
/// finish its work (failed, stopped, never started), which the owner is told, so a ticket
/// never stays in Doing without somebody on it.
pub(crate) fn ticket_settled(tx: &Tx<'_>, st: &Store, ticket: TicketId, except: Option<RunId>, broken: bool) -> Result<Vec<EventNotice>, StorageError> {
    let Some(root) = aurelius_storage::tickets::root_task_of(tx, ticket)? else { return Ok(vec![]) };
    if aurelius_storage::inbox::open_on_ticket(tx, ticket, except)? {
        return Ok(vec![]);
    }
    // Only a task that waits for its colleagues goes on; one that waits for an answer, a login or the quota does not.
    if !matches!(store::waiting_cause(tx, root)?.as_deref(), Some("handed_off" | "delegated")) {
        return Ok(vec![]);
    }
    let (text, reason) = if broken { (HANDOFF_BROKEN, "handoff_broken") } else { (HANDOFFS_DONE, "handoff_done") };
    continue_with(tx, st, root, text, reason)
}

/// `ticket_settled` for the run that ended: whatever ticket its mailbox item was for.
pub(crate) fn run_settled(tx: &Tx<'_>, st: &Store, run: RunId, broken: bool) -> Result<Vec<EventNotice>, StorageError> {
    match aurelius_storage::inbox::ticket_of_run(tx, run)? {
        Some(ticket) => ticket_settled(tx, st, ticket, Some(run), broken),
        None => Ok(vec![]),
    }
}

/// The owner of a waiting task goes on, told `text`, when nothing of it is still open.
fn continue_with(tx: &Tx<'_>, st: &Store, task: TaskId, text: &str, reason: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    if row.status != TaskStatus::Waiting || open_parts(tx, task)? || live_run(tx, task, None)? || ticket_busy(tx, task, None)? {
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
    let (message, posted) = thread::post_returning(tx, st, conversation, task, Speaker::Update { kind: "status", from: "Aurelius" }, text)?;
    let mut notices = vec![posted];
    notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?);
    notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?);
    notices.push(task_event(tx, task, row.project_id, "task.resumed", serde_json::json!({ "reason": reason }))?);
    notices.extend(run_for(tx, task, conversation, message)?);
    Ok(notices)
}

/// The task waits for its turn with an employee that works on one task at a
/// time: queued (said once), and the oldest one in line goes when nothing runs.
fn in_line(tx: &Tx<'_>, task: TaskId) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
    let mut notices = Vec::new();
    if row.status == TaskStatus::Waiting {
        notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?);
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
    Delegate { employee: String, title: String, goal: String, criteria: Vec<String>, ticket: Option<String> },
    HandOver { employee: String, reason: String },
    MessageTask { task: String, message: String, interrupt: bool },
    AddIdea { title: String, note: String, kind: aurelius_domain::idea::IdeaKind, project: Option<String> },
    ListTasks { status: String, employee: String, query: String, project: Option<String> },
    ReadTask { task: String },
    ListConversations { query: String },
    ReadConversation { conversation: String },
    /// Suggest a new colleague; only the user can approve it.
    ProposeColleague { name: String, role: String, title: Option<String>, instructions: String, reports_to: Option<String>, reason: String },
    /// Suggest new instructions for oneself or a direct report; only the user can approve them.
    ProposeInstructions { employee: Option<String>, instructions: String, reason: String, evidence: Vec<String> },
    /// Management chooses the mode (`auto` or `plan`) of its own task, with a reason.
    SetMode { mode: String, reason: String },
    /// The latest messages of a channel of the Team chat (nothing changes); `channel`
    /// none: the run's own (see `discussion::run_channel`).
    ReadGroupChat { last: Option<u32>, channel: Option<String> },
    /// A message in a channel under the employee's name; it wakes nobody.
    PostToGroupChat { message: String, channel: Option<String> },
    /// The latest messages of the discussion of the run's ticket (nothing changes).
    ReadDiscussion { last: Option<u32> },
    /// A message in the discussion of the run's ticket; it wakes nobody.
    PostToDiscussion { message: String },
    /// Hand the work in the run's thread to a colleague (`discussion::hand_off`).
    HandOff { to: String, message: String },
    /// Send the responsible board member of the run's ticket a message in their chat (`employee_chat`).
    TellResponsible { message: String },
    /// A message to itself or a colleague later in this thread (`conversation::scheduled`).
    ScheduleMessage { delay: String, content: String, to: Option<String> },
    /// Replace the employee's own note, which is in its prompt at every run.
    Scratchpad { content: String },
    /// The dispatcher of a ticket chooses its owner (`triage`; only in a dispatcher's run).
    AssignTicket { employee: String, reason: String },
    /// The tester's verdict on a ticket in Testing (`testing::verdict`; only in
    /// the run of its open test task).
    ReportTestResult { passed: bool, summary: String },
    /// The owner of a ticket reports the work ready for Review (`report_ready`; only in a live
    /// run of the ticket's main task). The ticket moves when the run ends.
    ReportReady { report: String },
    /// The tickets of the board, filtered (`ticket::agent`); `project` none: the run's project, else all.
    ListTickets { project: Option<String>, column: Option<String>, query: Option<String>, limit: Option<u32> },
    /// One ticket in full with the latest messages of its discussion (`ticket` is `#12`, a number or an id).
    ReadTicket { ticket: String, last: Option<u32> },
    /// A new ticket in Backlog; management only, and it starts nothing.
    CreateTicket { project: Option<String>, title: String, description: Option<String>, priority: Option<String>, labels: Vec<String> },
    /// Title, description, priority or labels of a ticket the employee manages or works on.
    UpdateTicket { ticket: String, title: Option<String>, description: Option<String>, priority: Option<String>, labels: Option<Vec<String>>, design_url: Option<String> },
    /// Management starts a ticket in Backlog (`column` todo); no other move is for an employee.
    MoveTicket { ticket: String, column: String },
    /// A message in the discussion of a ticket.
    PostToTicket { ticket: String, message: String },
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
                            notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?);
                        }
                        notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?);
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
        if let EmployeeAction::ReadGroupChat { last, channel } = &action {
            let (last, channel) = (*last, channel.clone());
            return self
                .store
                .read(|conn| {
                    if run_owner(conn, run)?.and_then(|o| o.employee).is_none() {
                        return Ok((false, "Only an employee can read the group chat.".to_string(), vec![]));
                    }
                    match crate::conversation::discussion::run_channel(conn, run, channel.as_deref())? {
                        Ok(d) => Ok((true, crate::conversation::discussion::read_text(conn, &st, &d, last)?, vec![])),
                        Err(why) => Ok((false, why, vec![])),
                    }
                })
                .map_err(storage_error);
        }
        if let EmployeeAction::PostToGroupChat { message, channel } = &action {
            let (message, channel) = (message.clone(), channel.clone());
            return self
                .store
                .write(move |tx| match crate::conversation::discussion::run_channel(tx, run, channel.as_deref())? {
                    Ok(d) => crate::conversation::discussion::post_from_run(tx, &st, run, &d, &message),
                    Err(why) => Ok((false, why, vec![])),
                })
                .map_err(storage_error);
        }
        const NO_DISCUSSION: &str = "This run has no ticket discussion: only a run under a ticket, or one woken from its discussion, can use it.";
        if let EmployeeAction::ReadDiscussion { last } = &action {
            let last = *last;
            return self
                .store
                .read(|conn| match crate::conversation::discussion::run_discussion(conn, run)? {
                    Some(d) => Ok((true, crate::conversation::discussion::read_text(conn, &st, &d, last)?, vec![])),
                    None => Ok((false, NO_DISCUSSION.to_string(), vec![])),
                })
                .map_err(storage_error);
        }
        if let EmployeeAction::PostToDiscussion { message } = &action {
            let message = message.clone();
            return self
                .store
                .write(move |tx| match crate::conversation::discussion::run_discussion(tx, run)? {
                    Some(d) => crate::conversation::discussion::post_from_run(tx, &st, run, &d, &message),
                    None => Ok((false, NO_DISCUSSION.to_string(), vec![])),
                })
                .map_err(storage_error);
        }
        if let EmployeeAction::HandOff { to, message } = &action {
            let (to, message) = (to.clone(), message.clone());
            return self.store.write(move |tx| crate::conversation::discussion::hand_off(tx, &st, run, &to, &message)).map_err(storage_error);
        }
        if let EmployeeAction::TellResponsible { message } = &action {
            let message = message.clone();
            return self.store.write(move |tx| crate::conversation::employee_chat::tell_responsible(tx, &st, run, &message)).map_err(storage_error);
        }
        if let EmployeeAction::ScheduleMessage { delay, content, to } = &action {
            let (delay, content, to) = (delay.clone(), content.clone(), to.clone());
            return self.store.write(move |tx| crate::conversation::scheduled::schedule_message(tx, run, &delay, &content, to.as_deref())).map_err(storage_error);
        }
        if let EmployeeAction::Scratchpad { content } = &action {
            return self.write_scratchpad(run, content);
        }
        if let EmployeeAction::ListTickets { project, column, query, limit } = &action {
            return self
                .store
                .read(|conn| crate::ticket::agent::list(conn, &st, run, project.as_deref(), column.as_deref(), query.as_deref(), *limit))
                .map_err(storage_error);
        }
        if let EmployeeAction::ReadTicket { ticket, last } = &action {
            return self.store.read(|conn| crate::ticket::agent::read(conn, &st, run, ticket, *last)).map_err(storage_error);
        }
        if let EmployeeAction::CreateTicket { project, title, description, priority, labels } = &action {
            let (project, title, description, priority, labels) = (project.clone(), title.clone(), description.clone(), priority.clone(), labels.clone());
            return self
                .store
                .write(move |tx| {
                    let input = crate::ticket::agent::CreateInput { project: project.as_deref(), title: &title, description: description.as_deref(), priority: priority.as_deref(), labels: &labels };
                    crate::ticket::agent::create(tx, &st, run, input)
                })
                .map_err(storage_error);
        }
        if let EmployeeAction::UpdateTicket { ticket, title, description, priority, labels, design_url } = &action {
            let (ticket, title, description, priority, labels, design_url) = (ticket.clone(), title.clone(), description.clone(), priority.clone(), labels.clone(), design_url.clone());
            return self
                .store
                .write(move |tx| {
                    let input = crate::ticket::agent::UpdateInput { ticket: &ticket, title: title.as_deref(), description: description.as_deref(), priority: priority.as_deref(), labels: labels.as_deref(), design_url: design_url.as_deref() };
                    crate::ticket::agent::update(tx, &st, run, input)
                })
                .map_err(storage_error);
        }
        if let EmployeeAction::MoveTicket { ticket, column } = &action {
            let (ticket, column) = (ticket.clone(), column.clone());
            return self.store.write(move |tx| crate::ticket::agent::move_to(tx, &st, run, &ticket, &column)).map_err(storage_error);
        }
        if let EmployeeAction::PostToTicket { ticket, message } = &action {
            let (ticket, message) = (ticket.clone(), message.clone());
            return self.store.write(move |tx| crate::ticket::agent::post(tx, &st, run, &ticket, &message)).map_err(storage_error);
        }
        if let EmployeeAction::ProposeColleague { name, role, title, instructions, reports_to, reason } = &action {
            let input = crate::org::ProposalInput {
                name: name.clone(),
                role: role.clone(),
                title: title.clone(),
                instructions: instructions.clone(),
                reports_to: reports_to.clone(),
                reason: reason.clone(),
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
        if let EmployeeAction::AssignTicket { employee, reason } = &action {
            let (employee, reason) = (employee.clone(), reason.clone());
            return self.store.write(move |tx| crate::task::triage::assign(tx, &st, run, &employee, &reason)).map_err(storage_error);
        }
        if let EmployeeAction::ReportTestResult { passed, summary } = &action {
            let (passed, summary) = (*passed, summary.clone());
            return self.store.write(move |tx| crate::task::testing::verdict(tx, &st, run, passed, &summary)).map_err(storage_error);
        }
        if let EmployeeAction::ReportReady { report } = &action {
            let report = report.clone();
            return self.store.write(move |tx| report_ready(tx, &st, run, &report)).map_err(storage_error);
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
                    EmployeeAction::Delegate { employee, title, goal, criteria, ticket } => {
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
                        let hop = match delegation_hop(tx, run)? {
                            Ok(hop) => hop,
                            Err(why) => return Ok((false, why, vec![])),
                        };
                        // A part inherits the task's ticket; a task without one (recurring, from an app) names one.
                        let named = match crate::task::ticket_line::ticket_of(tx, task)? {
                            Some(_) => None,
                            None => match crate::ticket::agent::for_delegation(tx, &st, ticket.as_deref(), Some(row.project_id), crate::run::login::of_run(tx, run)?)? {
                                Ok(t) => Some(t.id),
                                Err(why) => return Ok((false, why, vec![])),
                            },
                        };
                        let me_name = employees::load_employee(tx, me)?.map(|m| m.name).unwrap_or_default();
                        let note = format!("(Delegated by {me_name} as part of the task \"{}\".)", row.title);
                        let Some((child, title, mut notices)) = insert_part(tx, &st, row.project_id, Some(me), &to, &title, &goal, &criteria, &note, Some(task), None, None, named, hop)? else {
                            return Ok((false, "Give the part a title.".to_string(), vec![]));
                        };
                        inherit_mode(tx, run, child, to.id)?;
                        notices.insert(1, task_event(tx, task, row.project_id, "task.delegated", serde_json::json!({ "child": child, "to": to.id, "subject": to.name, "title": title }))?);
                        notices.extend(crate::task::ticket_line::delegated_line(tx, child, &me_name, &to.name, &title)?);
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
                    | EmployeeAction::PostToGroupChat { .. }
                    | EmployeeAction::ReadDiscussion { .. }
                    | EmployeeAction::PostToDiscussion { .. }
                    | EmployeeAction::HandOff { .. }
                    | EmployeeAction::TellResponsible { .. }
                    | EmployeeAction::ScheduleMessage { .. }
                    | EmployeeAction::Scratchpad { .. }
                    | EmployeeAction::AssignTicket { .. }
                    | EmployeeAction::ReportTestResult { .. }
                    | EmployeeAction::ReportReady { .. }
                    | EmployeeAction::ListTickets { .. }
                    | EmployeeAction::ReadTicket { .. }
                    | EmployeeAction::CreateTicket { .. }
                    | EmployeeAction::UpdateTicket { .. }
                    | EmployeeAction::MoveTicket { .. }
                    | EmployeeAction::PostToTicket { .. } => unreachable!("handled above"),
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
                        let owner = crate::task::task_line::set_employee(tx, task, Some(to.id))?;
                        inherit_mode(tx, run, task, to.id)?;
                        let mut notices = vec![task_event(
                            tx,
                            task,
                            row.project_id,
                            "task.handed_off",
                            serde_json::json!({ "from": me, "to": to.id, "from_name": me_name, "to_name": to.name, "reason": reason, "subject": to.name }),
                        )?];
                        notices.extend(owner);
                        notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?);
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
        notices.extend(crate::run::queue::interrupt_conversation(tx, st, &serde_json::json!({ "task_id": task, "from_employee": me }), &task.to_string(), conversation)?);
    }
    notices.extend(run_for_at(tx, task, conversation, posted_id, aurelius_storage::inbox::run_hop(tx, run)? + 1)?);
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
    login: Option<aurelius_domain::UserId>,
    ticket: Option<TicketId>,
    hop: u32,
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
    store::insert_ticket_task(tx, child, project, Some(to.id), &title, &goal_blob.id.to_string(), &criteria_blob.id.to_string(), TaskStatus::Queued, parent, ticket)?;
    if let Some(conversation) = origin {
        store::set_task_origin(tx, child, conversation, me)?;
        // Asked from a conversation (not a part of a task): it runs on the login
        // the caller names (the delegating run's, an escalated task's), never on
        // whatever run of the conversation came last (B1-1).
        if parent.is_none() {
            store::set_login_user(tx, child, login.map(|u| u.to_string()).as_deref())?;
        }
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
    aurelius_storage::events::insert_job(tx, &child.to_string(), crate::task::tasks::TASK_JOB_KIND, event_id, &format!("task:{child}"), &serde_json::json!({ "task_id": child, "hop": hop }))?;
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
/// (the assistant gives an employee work), or an employee's chat with the user (it
/// names the ticket). The colleague gets a task of its own on
/// the ticket, whose result comes back in that conversation.
fn delegate_from_conversation(tx: &Tx<'_>, st: &Store, run: RunId, action: EmployeeAction) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let EmployeeAction::Delegate { employee, title, goal, criteria, ticket } = action else {
        return Ok((false, "Handing over only works for a task you work on. Outside a task, give work to a colleague with delegate_task.".to_string(), vec![]));
    };
    let Some(RunOwner { employee: me, project, conversation: Some(conversation) }) = run_owner(tx, run)? else {
        return Ok((false, "Only an employee or the project assistant can give work to an employee.".to_string(), vec![]));
    };
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
    let hop = match delegation_hop(tx, run)? {
        Ok(hop) => hop,
        Err(why) => return Ok((false, why, vec![])),
    };
    // On the login of the run that delegates, not of another run queued in the same conversation.
    let login = crate::run::login::of_run(tx, run)?;
    // A ticket's discussion works for that ticket; any other conversation names one.
    let (named, outside) = match aurelius_storage::discussions::ticket_of_conversation(tx, conversation)? {
        Some(own) => (Some(own.to_string()), None),
        None => (ticket, login),
    };
    let ticket = match crate::ticket::agent::for_delegation(tx, st, named.as_deref(), project, outside)? {
        Ok(t) => t,
        Err(why) => return Ok((false, why, vec![])),
    };
    let project = ticket.project_id;
    if !matches!(employees::project_scope(tx, project)?, Some((_, aurelius_domain::employee::LifecycleStatus::Active))) {
        return Ok((false, "That project is paused or archived; it takes no new work.".to_string(), vec![]));
    }
    let note = format!("(Asked by {asked_by} from a conversation with the user.)");
    let Some((child, title, mut notices)) = insert_part(tx, st, project, me, &to, &title, &goal, &criteria, &note, None, Some(conversation), login, Some(ticket.id), hop)? else {
        return Ok((false, "Give the task a title.".to_string(), vec![]));
    };
    inherit_mode(tx, run, child, to.id)?;
    notices.extend(crate::task::ticket_line::delegated_line(tx, child, &asked_by, &to.name, &title)?);
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

#[cfg(test)]
mod closing_tests {
    use super::{CLOSING_MIN_CHARS, closing_text};

    #[test]
    fn the_closing_text_starts_after_the_last_tool_use() {
        let closing = "x".repeat(CLOSING_MIN_CHARS);
        let answer = format!("First a note.\n\nAnother note.\n\n{closing}");
        assert_eq!(closing_text(&answer, &[13, answer.len() as u32 - CLOSING_MIN_CHARS as u32]), closing);
    }

    #[test]
    fn a_closing_too_short_to_stand_alone_takes_the_text_before_it() {
        let report = "r".repeat(CLOSING_MIN_CHARS);
        let answer = format!("A note.{report}\n\nDone.");
        assert_eq!(closing_text(&answer, &[7, answer.len() as u32 - 5]), format!("{report}\n\nDone."));
    }

    #[test]
    fn a_tool_use_after_the_last_text_keeps_that_text() {
        let report = "r".repeat(CLOSING_MIN_CHARS);
        assert_eq!(closing_text(&report, &[0, report.len() as u32]), report);
    }

    #[test]
    fn an_offset_inside_a_character_or_past_the_end_is_skipped() {
        assert_eq!(closing_text("é short", &[1, 500]), "é short");
    }
}
