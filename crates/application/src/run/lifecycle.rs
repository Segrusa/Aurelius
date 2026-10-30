//! The run lifecycle the daemon drives (start → checkpoints → terminal outcome).
//! A run reaches `succeeded` only on the runtime's terminal success.

use crate::{CommandService, storage_error};
use aurelius_domain::{ActorKind, RunId, EventNotice, AppError};
use aurelius_storage::{PreparedBlob, StorageError, blobs, queries};
use aurelius_domain::conversation::{MessageRole, MessageStatus};
use aurelius_storage::conversations::{self as conv, NewMessage};
use crate::run::events::{Ev, emit};
use crate::run::plan::{ChatRunPlan, RunOutcome};
use crate::run::queue::{explain_failed_start, failed_start_text};

impl CommandService {
    /// Record usage reported for a run (chat or task step). Runs that are not
    /// registered (internal memory jobs) are skipped.
    pub fn record_usage(&self, run_id: RunId, usage: &aurelius_runtime::Usage) -> Result<Vec<EventNotice>, AppError> {
        let reported = usage.clone();
        let usage = aurelius_storage::usage::NewUsage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_tokens: usage.cached_tokens,
            cache_creation_tokens: usage.cache_creation_tokens,
            cost_micro_usd: usage.cost_micro_usd,
            estimate: usage.estimate,
        };
        self.store
            .write(move |tx| {
                if !aurelius_storage::usage::run_exists(tx, run_id)? {
                    return Ok(vec![]);
                }
                // Trace mode: the steps of the run go with its trace.
                if let Some(mut trace) = aurelius_storage::traces::get(tx, run_id)? {
                    crate::run::trace::with_usage(&mut trace, &reported);
                    aurelius_storage::traces::put(tx, &trace)?;
                }
                let n: i64 = tx.query_row("SELECT COUNT(*) FROM usage_records WHERE run_id = ?1", [run_id.to_string()], |r| r.get(0))?;
                let (event_id, notice) = emit(tx, Ev {
                    aggregate_type: "run",
                    aggregate_id: run_id.to_string(),
                    event_type: "run.usage_reported",
                    actor: ActorKind::Runtime,
                    actor_id: None,
                    correlation: run_id.to_string(),
                    payload: None,
                    metadata: serde_json::json!({ "input": usage.input_tokens, "output": usage.output_tokens, "cached": usage.cached_tokens, "cache_creation": usage.cache_creation_tokens, "cost_micro_usd": usage.cost_micro_usd, "estimate": usage.estimate }),
                    dedupe: format!("run:{run_id}:usage:{n}"),
                })?;
                aurelius_storage::usage::insert(tx, run_id, event_id, &usage)?;
                Ok(vec![notice])
            })
            .map_err(storage_error)
    }

    /// The runtime accepted the turn: mark running and create the answer message.
    /// Fenced: returns `None` if this owner no longer holds the lease.
    pub fn start_chat_run(&self, plan: &ChatRunPlan, epoch: i64) -> Result<Option<Vec<EventNotice>>, AppError> {
        let plan = plan.clone();
        let manifest = match &plan.manifest {
            Some(m) => Some(self.store.blobs().put_json(m).map_err(storage_error)?),
            None => None,
        };
        self.store
            .write(move |tx| {
                if !queries::holds_lease(tx, &plan.run_id.to_string(), epoch)? {
                    return Ok(None);
                }
                if let Some(blob) = &manifest {
                    blobs::register(tx, blob)?;
                    tx.execute("UPDATE runs SET context_blob_id = ?2 WHERE id = ?1", [plan.run_id.to_string(), blob.id.to_string()])?;
                }
                // A stop that arrived while the run was being prepared stays in force.
                if conv::run_status(tx, plan.run_id)?.as_deref() != Some("cancelling") {
                    conv::set_run_status(tx, plan.run_id, "running", None)?;
                }
                // A run that waited for the usage limit goes again; its task with it.
                let resumed = if conv::run_error(tx, plan.run_id)?.as_deref() == Some("quota") {
                    tx.execute("UPDATE runs SET error_code = NULL WHERE id = ?1", [plan.run_id.to_string()])?;
                    crate::task::task_runs::on_run_quota_resumed(tx, plan.run_id)?
                } else {
                    vec![]
                };
                let (event_id, started) = emit(tx, Ev {
                    aggregate_type: "run",
                    aggregate_id: plan.run_id.to_string(),
                    event_type: "run.started",
                    actor: ActorKind::Runtime,
                    actor_id: None,
                    correlation: plan.run_id.to_string(),
                    payload: None,
                    metadata: serde_json::json!({ "runtime": plan.config.runtime_kind(), "model": plan.config.model, "lease_epoch": epoch }),
                    dedupe: format!("run:{}:started:{epoch}", plan.run_id),
                })?;
                if conv::message_for_run(tx, plan.run_id)?.is_none() {
                    conv::insert_message(tx, NewMessage {
                        id: plan.message_id,
                        conversation_id: plan.conversation_id,
                        role: MessageRole::Assistant,
                        content_blob_id: None,
                        source_event_id: event_id,
                        status: MessageStatus::Partial,
                        run_id: Some(plan.run_id),
                        reply_to: Some(plan.reply_to),
                    })?;
                } else {
                    conv::update_message(tx, plan.message_id, None, MessageStatus::Partial)?;
                }
                let mut notices = vec![started];
                notices.extend(resumed);
                // A run of a finished task goes from queued to running.
                notices.extend(crate::task::task_runs::live_run_notice(tx, plan.conversation_id)?);
                Ok(Some(notices))
            })
            .map_err(storage_error)
    }

    /// A stop was requested for this run (checked right after start).
    pub fn run_cancelling(&self, run_id: RunId) -> bool {
        self.store.read(|c| conv::run_status(c, run_id)).ok().flatten().as_deref() == Some("cancelling")
    }

    /// The runtime could not take the turn: retry later, park, or fail.
    pub fn defer_chat_run(&self, plan: &ChatRunPlan, epoch: i64, disposition: aurelius_domain::scheduler::Disposition) -> Result<Vec<EventNotice>, AppError> {
        self.defer_run(plan.run_id, epoch, disposition)
    }

    /// Same as `defer_chat_run`, for a run that could not even be planned.
    pub fn defer_run(&self, run_id: RunId, epoch: i64, disposition: aurelius_domain::scheduler::Disposition) -> Result<Vec<EventNotice>, AppError> {
        use aurelius_domain::scheduler::Disposition;
        // A run that never started leaves its reason in the answer, not an empty message.
        let explanation: Option<PreparedBlob> = match &disposition {
            Disposition::Fail { code } => Some(self.store.blobs().put(failed_start_text(code).as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?),
            _ => None,
        };
        let st = self.store.clone();
        self.store
            .write(move |tx| {
                let job = run_id.to_string();
                let mut until: Option<i64> = None;
                let (event_type, reason, run_status) = match &disposition {
                    Disposition::RetryAt(at) => {
                        if !queries::reschedule_job(tx, &job, epoch, *at, "transient", true)? {
                            return Ok(vec![]);
                        }
                        ("run.waiting", "retry".to_string(), "created")
                    }
                    Disposition::WaitForUser { reason } => {
                        if !queries::reschedule_job(tx, &job, epoch, i64::MAX / 2, reason, false)? {
                            return Ok(vec![]);
                        }
                        ("run.waiting", reason.to_string(), "waiting")
                    }
                    Disposition::WaitUntil { at_ms, reason } => {
                        if !queries::reschedule_job(tx, &job, epoch, *at_ms, reason, false)? {
                            return Ok(vec![]);
                        }
                        until = Some(*at_ms);
                        ("run.waiting", reason.to_string(), "waiting")
                    }
                    Disposition::Fail { code } => {
                        if !queries::kill_job(tx, &job, epoch, code)? {
                            return Ok(vec![]);
                        }
                        ("run.failed", code.clone(), "failed")
                    }
                };
                let error = if run_status == "created" { None } else { Some(reason.as_str()) };
                conv::set_run_status(tx, run_id, run_status, error)?;
                let version = conv::next_version(tx, "run", &job)?;
                let (_, n) = emit(tx, Ev {
                    aggregate_type: "run",
                    aggregate_id: job.clone(),
                    event_type,
                    actor: ActorKind::System,
                    actor_id: None,
                    correlation: job.clone(),
                    payload: None,
                    metadata: serde_json::json!({ "reason": reason, "until_ms": until }),
                    dedupe: format!("run:{job}:{event_type}:{version}"),
                })?;
                let mut notices = vec![n];
                if let (Some(at), "quota") = (until, reason.as_str()) {
                    notices.extend(crate::task::task_runs::on_run_quota_wait(tx, run_id, at)?);
                }
                if let Some(blob) = &explanation {
                    notices.extend(explain_failed_start(tx, run_id, blob)?);
                    // Woken by the group chat: the group hears it could not answer.
                    if let Some(conversation) = conv::run_conversation(tx, run_id)? {
                        notices.extend(crate::conversation::group_chat::mirror_answer(tx, &st, run_id, conversation, "", Some("not started"), &[])?);
                    }
                    // A task's run that never started: the task must not stay "running".
                    notices.extend(crate::task::task_runs::on_run_not_started(tx, &st, run_id)?);
                }
                Ok(notices)
            })
            .map_err(storage_error)
    }

    /// Runs that stay queued because the machine is busy (or the run limit is
    /// reached) say why in their conversation: one `run.waiting` per run and
    /// reason (`load:cpu`, `load:memory`, `load:max_runs`). The job is not touched.
    pub fn note_held_runs(&self, reason: &str) -> Result<Vec<EventNotice>, AppError> {
        let reason = reason.to_owned();
        self.store
            .write(move |tx| {
                let mut notices = Vec::new();
                for job in queries::mark_held_runs(tx, &reason, aurelius_storage::now_ms())? {
                    let conversation: Option<String> = tx
                        .query_row("SELECT json_extract(payload_json, '$.conversation_id') FROM jobs WHERE id = ?1", [&job], |r| r.get(0))?;
                    let version = conv::next_version(tx, "run", &job)?;
                    let (_, n) = emit(tx, Ev {
                        aggregate_type: "run",
                        aggregate_id: job.clone(),
                        event_type: "run.waiting",
                        actor: ActorKind::System,
                        actor_id: None,
                        correlation: job.clone(),
                        payload: None,
                        metadata: serde_json::json!({ "reason": reason, "conversation_id": conversation }),
                        dedupe: format!("run:{job}:run.waiting:{version}"),
                    })?;
                    notices.push(n);
                }
                Ok(notices)
            })
            .map_err(storage_error)
    }

    /// The Claude usage limit is reached until `until_ms`: every run of the provider
    /// that has not started waits for the reset too ("quota"), instead of failing on it.
    pub fn park_runs_for_quota(&self, until_ms: i64) -> Result<Vec<EventNotice>, AppError> {
        self.store
            .write(move |tx| {
                let mut notices = Vec::new();
                for job in queries::park_ready_runs_for_quota(tx, "claude_code", until_ms)? {
                    let run_id = RunId::parse(&job).map_err(|e| StorageError::Rejected(e.to_string()))?;
                    conv::set_run_status(tx, run_id, "waiting", Some("quota"))?;
                    let version = conv::next_version(tx, "run", &job)?;
                    let (_, n) = emit(tx, Ev {
                        aggregate_type: "run",
                        aggregate_id: job.clone(),
                        event_type: "run.waiting",
                        actor: ActorKind::System,
                        actor_id: None,
                        correlation: job.clone(),
                        payload: None,
                        metadata: serde_json::json!({ "reason": "quota", "until_ms": until_ms }),
                        dedupe: format!("run:{job}:run.waiting:{version}"),
                    })?;
                    notices.push(n);
                    notices.extend(crate::task::task_runs::on_run_quota_wait(tx, run_id, until_ms)?);
                }
                Ok(notices)
            })
            .map_err(storage_error)
    }

    /// The run waited for the usage limit and now goes again: it continues the
    /// work of its earlier attempt (the dispatcher tells the model so).
    pub fn run_was_quota_parked(&self, run_id: RunId) -> bool {
        self.store.read(|c| conv::run_error(c, run_id)).ok().flatten().as_deref() == Some("quota")
    }

    /// Make runs parked for `reason` (e.g. "auth") due again.
    pub fn wake_parked_runs(&self, reason: &str) -> Result<usize, AppError> {
        let reason = reason.to_owned();
        self.store.write(move |tx| queries::wake_parked(tx, &reason, aurelius_storage::now_ms())).map_err(storage_error)
    }

    /// Record the model the runtime reports (shown with the answer). Fenced by lease.
    pub fn record_run_model(&self, run_id: RunId, epoch: i64, model: &str) -> Result<(), AppError> {
        let model = model.to_owned();
        self.store
            .write(move |tx| {
                if queries::holds_lease(tx, &run_id.to_string(), epoch)? {
                    tx.execute("UPDATE runs SET model_id = ?2 WHERE id = ?1", [run_id.to_string(), model])?;
                }
                Ok(())
            })
            .map_err(storage_error)
    }

    /// Durable partial text, so a crash keeps what was already streamed.
    pub fn checkpoint_chat_run(&self, plan: &ChatRunPlan, text: &str, epoch: i64) -> Result<Vec<EventNotice>, AppError> {
        let blob = self.store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let (run_id, message_id) = (plan.run_id, plan.message_id);
        self.store
            .write(move |tx| {
                if !queries::holds_lease(tx, &run_id.to_string(), epoch)? {
                    return Err(StorageError::Rejected("lease lost; checkpoint refused".into()));
                }
                blobs::register(tx, &blob)?;
                let version = conv::next_version(tx, "run", &run_id.to_string())?;
                let (_, n) = emit(tx, Ev {
                    aggregate_type: "run",
                    aggregate_id: run_id.to_string(),
                    event_type: "run.checkpointed",
                    actor: ActorKind::Runtime,
                    actor_id: Some("fake".into()),
                    correlation: run_id.to_string(),
                    payload: Some(blob.id.to_string()),
                    metadata: serde_json::json!({}),
                    dedupe: format!("run:{run_id}:checkpoint:{version}"),
                })?;
                conv::update_message(tx, message_id, Some(&blob.id.to_string()), MessageStatus::Partial)?;
                Ok(vec![n])
            })
            .map_err(storage_error)
    }

    /// Commit the terminal outcome and complete the job if `epoch` still holds the lease.
    pub fn finish_chat_run(
        &self,
        plan: &ChatRunPlan,
        outcome: RunOutcome,
        text: &str,
        epoch: i64,
    ) -> Result<Vec<EventNotice>, AppError> {
        let blob: Option<PreparedBlob> = if text.is_empty() {
            None
        } else {
            Some(self.store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?)
        };
        let (run_id, message_id, conversation_id) = (plan.run_id, plan.message_id, plan.conversation_id);
        let reply = text.to_string();
        let files_store = self.store.clone();
        let session = plan.session.clone();
        self.store
            .write(move |tx| {
                if !queries::complete_job(tx, &run_id.to_string(), epoch)? {
                    return Err(StorageError::Rejected("lease lost; a newer owner is responsible".into()));
                }
                if let Some(blob) = &blob {
                    blobs::register(tx, blob)?;
                }
                let (message_event, message_status, run_event, run_status, error) = match &outcome {
                    RunOutcome::Completed => ("message.assistant_committed", MessageStatus::Complete, "run.completed", "succeeded", None),
                    RunOutcome::Interrupted => ("message.interrupted", MessageStatus::Interrupted, "run.cancelled", "cancelled", None),
                    RunOutcome::Failed(code) => ("message.interrupted", MessageStatus::Failed, "run.failed", "failed", Some(code.clone())),
                };
                let blob_id = blob.as_ref().map(|b| b.id.to_string());
                let mut notices = Vec::new();
                let (message_event_id, message_notice) = emit(tx, Ev {
                    aggregate_type: "conversation",
                    aggregate_id: conversation_id.to_string(),
                    event_type: message_event,
                    actor: ActorKind::Runtime,
                    actor_id: Some("fake".into()),
                    correlation: run_id.to_string(),
                    payload: blob_id.clone(),
                    metadata: serde_json::json!({ "run_id": run_id, "message_id": message_id }),
                    dedupe: format!("run:{run_id}:message"),
                })?;
                notices.push(message_notice);
                conv::update_message(tx, message_id, blob_id.as_deref(), message_status)?;
                // The conversation goes on in the run's session only when it completed there.
                crate::run::session::on_run_finished(tx, conversation_id, run_id, message_id, &session, outcome == RunOutcome::Completed)?;
                // Files the answer names (in the project's folders) are kept with it, to open in the app.
                let mut kept_files = Vec::new();
                if outcome == RunOutcome::Completed {
                    let project: Option<String> = tx.query_row("SELECT project_id FROM conversations WHERE id = ?1", [conversation_id.to_string()], |r| r.get(0)).ok().flatten();
                    let files = match project.and_then(|p| aurelius_domain::ProjectId::parse(&p).ok()) {
                        Some(project) => crate::conversation::message_files::keep_for_project(tx, &files_store, &reply, project)?,
                        // A chat outside projects (also an employee's): its own work folder,
                        // and an employee's own workspace.
                        None => {
                            let own = files_store.root().join("workspaces").join(conversation_id.to_string());
                            let roots: Vec<std::path::PathBuf> = conv::work_folders(tx, conversation_id)?
                                .into_iter()
                                .map(std::path::PathBuf::from)
                                .chain(std::iter::once(own))
                                .filter_map(|r| r.canonicalize().ok())
                                .collect();
                            crate::conversation::message_files::keep(tx, &files_store, &reply, &roots)?
                        }
                    };
                    conv::set_message_files(tx, message_id, &files)?;
                    kept_files = files;
                }
                if outcome == RunOutcome::Completed {
                    crate::memory::maybe_schedule_summary(tx, conversation_id, message_event_id)?;
                }
                // Woken by the group chat: the answer goes there too, under the employee's name.
                let group_failure = match &outcome {
                    RunOutcome::Failed(code) => Some(code.as_str()),
                    RunOutcome::Interrupted => Some("interrupted"),
                    RunOutcome::Completed => None,
                };
                notices.extend(crate::conversation::group_chat::mirror_answer(tx, &files_store, run_id, conversation_id, &reply, group_failure, &kept_files)?);
                // The run of a task's conversation: its answer is the task's report.
                let failed = match &outcome {
                    RunOutcome::Failed(code) => Some(code.as_str()),
                    _ => None,
                };
                notices.extend(crate::task::task_runs::on_run_finished(tx, &files_store, run_id, outcome == RunOutcome::Completed, failed, &reply)?);
                // Results of tasks given from here that came in during this run.
                notices.extend(crate::task::task_runs::answer_results(tx, run_id, outcome == RunOutcome::Completed)?);
                notices.push(
                    emit(tx, Ev {
                        aggregate_type: "run",
                        aggregate_id: run_id.to_string(),
                        event_type: run_event,
                        actor: ActorKind::Runtime,
                        actor_id: Some("fake".into()),
                        correlation: run_id.to_string(),
                        payload: None,
                        metadata: serde_json::json!({ "error_code": error }),
                        dedupe: format!("run:{run_id}:terminal"),
                    })?
                    .1,
                );
                notices.extend(CommandService::invalidate_run_approvals(tx, run_id)?);
                conv::set_run_status(tx, run_id, run_status, error.as_deref())?;
                if let Some(n) = crate::run::heartbeat::on_run_finished(tx, run_id, outcome == RunOutcome::Completed, &reply)? {
                    notices.push(n);
                }
                Ok(notices)
            })
            .map_err(storage_error)
    }

    /// The runtime started using one of its tools during this answer.
    pub fn record_activity_started(&self, plan: &ChatRunPlan, call_id: &str, tool: &str, input: &serde_json::Value, parent: Option<&str>, offset: u32) -> Result<(), AppError> {
        let (run, message, call, tool, input, parent) = (plan.run_id, plan.message_id, call_id.to_owned(), tool.to_owned(), input.clone(), parent.map(str::to_owned));
        self.store.write(move |tx| conv::activity_started(tx, run, message, &call, &tool, &input, parent.as_deref(), offset)).map_err(storage_error)
    }

    pub fn record_activity_finished(&self, plan: &ChatRunPlan, call_id: &str, ok: bool, output: &str) -> Result<(), AppError> {
        let (run, call, output) = (plan.run_id, call_id.to_owned(), output.to_owned());
        self.store.write(move |tx| conv::activity_finished(tx, run, &call, ok, &output)).map_err(storage_error)?;
        Ok(())
    }

    /// Startup reconciliation: runs that were active when the daemon died end
    /// as failed with their last checkpoint kept; their jobs are not re-run blindly.
    pub fn reconcile_chat_runs(&self) -> Result<usize, AppError> {
        self.store
            .write(|tx| {
                let runs = conv::unfinished_runs(tx)?;
                for (run_id, conversation_id) in &runs {
                    let _ = CommandService::invalidate_run_approvals(tx, *run_id)?;
                    if let Some(message_id) = conv::message_for_run(tx, *run_id)? {
                        conv::update_message(tx, message_id, None, MessageStatus::Interrupted)?;
                    }
                    emit(tx, Ev {
                        aggregate_type: "conversation",
                        aggregate_id: conversation_id.to_string(),
                        event_type: "message.interrupted",
                        actor: ActorKind::System,
                        actor_id: None,
                        correlation: run_id.to_string(),
                        payload: None,
                        metadata: serde_json::json!({ "run_id": run_id, "reason": "daemon_restart" }),
                        dedupe: format!("run:{run_id}:message"),
                    })?;
                    emit(tx, Ev {
                        aggregate_type: "run",
                        aggregate_id: run_id.to_string(),
                        event_type: "run.failed",
                        actor: ActorKind::System,
                        actor_id: None,
                        correlation: run_id.to_string(),
                        payload: None,
                        metadata: serde_json::json!({ "error_code": "daemon_restart" }),
                        dedupe: format!("run:{run_id}:terminal"),
                    })?;
                    conv::set_run_status(tx, *run_id, "failed", Some("daemon_restart"))?;
                    tx.execute("UPDATE jobs SET state = 'dead', last_error_code = 'daemon_restart' WHERE id = ?1 AND state = 'leased'", [run_id.to_string()])?;
                }
                // Claimed but never started: safe to run again.
                tx.execute(
                    "UPDATE jobs SET state = 'ready', lease_owner = NULL, lease_until_ms = NULL
                     WHERE kind = 'run.chat' AND state = 'leased' AND id IN (SELECT id FROM runs WHERE status = 'created')",
                    [],
                )?;
                Ok(runs.len())
            })
            .map_err(storage_error)
    }
}
