//! Queueing, stopping and interrupting runs, and the text of a run that never started.

use crate::run::events::{Ev, emit};
use crate::{CHAT_RUN_KIND, CommandService};
use aurelius_domain::{ActorKind, ConversationId, MessageId, RunId, EventNotice};
use aurelius_storage::{PreparedBlob, StorageError, Store, Tx, blobs};
use aurelius_domain::conversation::{MessageRole, MessageStatus};
use aurelius_storage::conversations::{self as conv, NewMessage};
use aurelius_storage::events;

/// What the user reads in place of an answer that could not be started.
pub(crate) fn failed_start_text(code: &str) -> String {
    let reason = code.strip_prefix("retries_exhausted:").unwrap_or(code);
    let reason = match reason {
        "plan_failed" => "the server could not prepare this answer",
        "abandoned" => "the server stopped working on it",
        "start_not_recorded" => "the server could not record its start",
        other => other,
    };
    format!("This answer could not be started: {reason}\n\nSend the message again to retry.")
}

/// Put the reason a run never started in its answer. The answer message only
/// exists once a run starts, so for a run that never did it is made here, from
/// the job's payload; otherwise the user would see no answer at all.
pub(crate) fn explain_failed_start(tx: &Tx<'_>, run_id: RunId, blob: &PreparedBlob) -> Result<Vec<EventNotice>, StorageError> {
    use rusqlite::OptionalExtension;
    let payload: Option<String> = tx.query_row("SELECT payload_json FROM jobs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).optional()?;
    let payload: serde_json::Value = payload.and_then(|p| serde_json::from_str(&p).ok()).unwrap_or_default();
    let field = |key: &str| payload[key].as_str().map(str::to_owned);
    let Some(conversation) = field("conversation_id").and_then(|c| ConversationId::parse(&c).ok()) else { return Ok(Vec::new()) };
    blobs::register(tx, blob)?;
    let (event_id, notice) = emit(tx, Ev {
        aggregate_type: "conversation",
        aggregate_id: conversation.to_string(),
        event_type: "message.interrupted",
        actor: ActorKind::System,
        actor_id: None,
        correlation: run_id.to_string(),
        payload: Some(blob.id.to_string()),
        metadata: serde_json::json!({ "run_id": run_id, "never_started": true }),
        dedupe: format!("run:{run_id}:message"),
    })?;
    match conv::message_for_run(tx, run_id)? {
        Some(message) => conv::update_message(tx, message, Some(&blob.id.to_string()), MessageStatus::Failed)?,
        None => {
            let (Some(message), Some(reply_to)) = (field("assistant_message_id").and_then(|m| MessageId::parse(&m).ok()), field("reply_to").and_then(|m| MessageId::parse(&m).ok())) else {
                return Ok(vec![notice]);
            };
            conv::insert_message(tx, NewMessage {
                id: message,
                conversation_id: conversation,
                role: MessageRole::Assistant,
                content_blob_id: Some(blob.id.to_string()),
                source_event_id: event_id,
                status: MessageStatus::Failed,
                run_id: Some(run_id),
                reply_to: Some(reply_to),
            })?;
        }
    }
    Ok(vec![notice])
}

/// `group`: the group chat woke this run (see `conv::GroupRun`); its answer is copied there.
fn job_payload(run: RunId, conversation: ConversationId, message: MessageId, reply_to: MessageId, group: Option<conv::GroupRun>) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "run_id": run, "conversation_id": conversation,
        "assistant_message_id": message, "reply_to": reply_to,
    });
    if let Some(group) = group {
        payload["group"] = serde_json::json!(group);
    }
    payload
}

/// Queue a run for `reply_to`: runs row + `run.queued` + durable job (job id = run id).
/// The notices: `run.queued`, after those of a task that goes on with it (`link_run`).
pub(crate) fn queue_run(
    tx: &Tx<'_>,
    ctx_meta: &serde_json::Value,
    correlation: &str,
    conversation: ConversationId,
    reply_to: MessageId,
) -> Result<(RunId, Vec<EventNotice>), StorageError> {
    queue_run_marked(tx, ctx_meta, correlation, conversation, reply_to, None)
}

/// `queue_run`, for a run the group chat woke (`group`).
pub(crate) fn queue_run_marked(
    tx: &Tx<'_>,
    ctx_meta: &serde_json::Value,
    correlation: &str,
    conversation: ConversationId,
    reply_to: MessageId,
    group: Option<conv::GroupRun>,
) -> Result<(RunId, Vec<EventNotice>), StorageError> {
    let run_id = RunId::new();
    let message_id = MessageId::new();
    // Every conversation runs like an employee at work: the employee defaults apply.
    let resolved = crate::run::connections::resolve(tx, conversation, aurelius_domain::connection::Purpose::Employee)?;
    conv::insert_run(tx, run_id, conversation, &resolved)?;
    // A run in a live task's conversation counts for the task.
    let mut notices = crate::task::task_runs::link_run(tx, run_id, conversation)?;
    let (event_id, notice) = emit(
        tx,
        Ev {
            aggregate_type: "run",
            aggregate_id: run_id.to_string(),
            event_type: "run.queued",
            actor: ActorKind::User,
            actor_id: None,
            correlation: correlation.into(),
            payload: None,
            metadata: ctx_meta.clone(),
            dedupe: format!("run:{run_id}:queued"),
        },
    )?;
    let payload = job_payload(run_id, conversation, message_id, reply_to, group);
    events::insert_job(tx, &run_id.to_string(), CHAT_RUN_KIND, event_id, &format!("run:{run_id}"), &payload)?;
    notices.push(notice);
    Ok((run_id, notices))
}

/// Stop one run that has status `status`; `None` when it had already finished.
/// A run that was not claimed yet is cancelled outright; otherwise it becomes
/// 'cancelling' and the daemon interrupts its process. A run the group chat
/// woke tells the group it was stopped (`store`: for that message).
pub(crate) fn request_stop(
    tx: &Tx<'_>,
    store: &Store,
    ctx_meta: &serde_json::Value,
    correlation: &str,
    run_id: RunId,
    status: &str,
    dedupe: String,
) -> Result<Option<Vec<EventNotice>>, StorageError> {
    if matches!(status, "succeeded" | "failed" | "cancelled" | "unknown") {
        return Ok(None);
    }
    let mut notices = vec![
        emit(tx, Ev {
            aggregate_type: "run",
            aggregate_id: run_id.to_string(),
            event_type: "run.cancel_requested",
            actor: ActorKind::User,
            actor_id: None,
            correlation: correlation.into(),
            payload: None,
            metadata: ctx_meta.clone(),
            dedupe,
        })?
        .1,
    ];
    // Its open approvals and questions no longer apply; deciding one later must not revive the run.
    notices.extend(CommandService::invalidate_run_approvals(tx, run_id)?);
    // Not claimed yet, or parked (for a usage limit) with its job back in the
    // queue: cancel the job directly; no process runs for it.
    // Claimed: the dispatcher sees 'cancelling' and interrupts.
    let not_claimed = matches!(status, "created" | "waiting")
        && tx.execute("UPDATE jobs SET state = 'cancelled' WHERE id = ?1 AND state = 'ready'", [run_id.to_string()])? == 1;
    if not_claimed {
        if status == "waiting" {
            notices.extend(interrupt_parked_message(tx, run_id, correlation)?);
        }
        conv::set_run_status(tx, run_id, "cancelled", None)?;
        notices.push(
            emit(tx, Ev {
                aggregate_type: "run",
                aggregate_id: run_id.to_string(),
                event_type: "run.cancelled",
                actor: ActorKind::System,
                actor_id: None,
                correlation: correlation.into(),
                payload: None,
                metadata: serde_json::json!({}),
                dedupe: format!("run:{run_id}:cancelled"),
            })?
            .1,
        );
        if status == "waiting" {
            notices.extend(crate::task::task_runs::on_parked_run_stopped(tx, run_id)?);
        }
        // Its process never finishes it: the group hears it here (a claimed run's in `finish_chat_run`).
        if let Some(conversation) = conv::run_conversation(tx, run_id)? {
            notices.extend(crate::conversation::group_chat::mirror_answer(tx, store, run_id, conversation, "", Some("cancelled"), &[], None)?);
        }
    } else {
        conv::set_run_status(tx, run_id, "cancelling", None)?;
    }
    Ok(Some(notices))
}

/// A cancelled task: its runs parked for a usage limit (job back in the queue)
/// end now, so they never start again at the reset. Live runs are interrupted
/// by the daemon.
pub(crate) fn stop_parked_runs_of_task(tx: &Tx<'_>, store: &Store, task: aurelius_domain::TaskId, correlation: &str) -> Result<Vec<EventNotice>, StorageError> {
    let parked: Vec<String> = tx
        .prepare(
            "SELECT r.id FROM runs r JOIN jobs j ON j.id = r.id
             WHERE r.task_id = ?1 AND r.conversation_id IS NOT NULL AND r.status = 'waiting' AND j.state = 'ready'",
        )?
        .query_map([task.to_string()], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut notices = Vec::new();
    for run_id in parked.iter().filter_map(|r| RunId::parse(r).ok()) {
        let meta = serde_json::json!({ "task_id": task });
        if let Some(n) = request_stop(tx, store, &meta, correlation, run_id, "waiting", format!("task:{task}:cancel:{run_id}"))? {
            notices.extend(n);
        }
    }
    Ok(notices)
}

/// The answer of a parked run that is stopped: what it wrote before it was
/// parked stays, marked interrupted. A run parked before it started has none.
fn interrupt_parked_message(tx: &Tx<'_>, run_id: RunId, correlation: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(message) = conv::message_for_run(tx, run_id)? else { return Ok(Vec::new()) };
    conv::update_message(tx, message, None, MessageStatus::Interrupted)?;
    let conversation: String = tx.query_row("SELECT conversation_id FROM messages WHERE id = ?1", [message.to_string()], |r| r.get(0))?;
    let (_, notice) = emit(tx, Ev {
        aggregate_type: "conversation",
        aggregate_id: conversation,
        event_type: "message.interrupted",
        actor: ActorKind::System,
        actor_id: None,
        correlation: correlation.into(),
        payload: None,
        metadata: serde_json::json!({ "run_id": run_id, "message_id": message, "reason": "stopped_while_parked" }),
        dedupe: format!("run:{run_id}:message"),
    })?;
    Ok(vec![notice])
}

/// "Send now": stop every answer still running or queued in the conversation,
/// before the new message's run is queued.
pub(crate) fn interrupt_conversation(
    tx: &Tx<'_>,
    store: &Store,
    ctx_meta: &serde_json::Value,
    correlation: &str,
    conversation: ConversationId,
) -> Result<Vec<EventNotice>, StorageError> {
    let mut notices = Vec::new();
    for (run_id, status) in conv::unfinished_runs_in(tx, conversation)? {
        // Already stopping (a Stop sent just before): the daemon interrupts it again anyway.
        if status == "cancelling" {
            continue;
        }
        if let Some(n) = request_stop(tx, store, ctx_meta, correlation, run_id, &status, format!("cmd:{correlation}:cancel:{run_id}"))? {
            notices.extend(n);
        }
    }
    Ok(notices)
}

