//! Chat use-cases: send, retry as new version, stop, delete and
//! the per-conversation settings. The runs they queue are in `crate::run`.

use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::{ActorKind, ConversationId, DomainError, MessageId, RunId, CommandEnvelope, CommandResult, AppError};
use aurelius_storage::{StorageError, Tx, blobs};
use aurelius_domain::conversation::{self, MessageRole, MessageStatus};
use aurelius_storage::conversations::{self as conv, NewMessage};
use crate::run::events::{Ev, emit};
use crate::run::queue::{interrupt_conversation, queue_run, request_stop};

/// The queued run of a user message that still waits for its answer, or why
/// that message can no longer be changed. Project, employee and task conversations alike.
fn waiting_run(tx: &Tx<'_>, conversation: ConversationId, message: MessageId) -> Result<Result<RunId, AppError>, StorageError> {
    let open: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1 AND archived_at_ms IS NULL)", [conversation.to_string()], |r| r.get(0))?;
    if !open {
        return Ok(Err(domain_error(DomainError::ConversationNotFound(conversation))));
    }
    Ok(match conv::reply_state(tx, conversation, message)? {
        None => Err(domain_error(DomainError::MessageNotFound(message))),
        Some(conv::ReplyState::Queued(run)) => Ok(run),
        Some(conv::ReplyState::Started) => Err(domain_error(DomainError::MessageAlreadyStarted)),
        Some(conv::ReplyState::NotQueued) => Err(domain_error(DomainError::MessageNotQueued)),
    })
}

/// The project a conversation belongs to (none for an employee chat).
fn conversation_project(tx: &Tx<'_>, conversation: ConversationId) -> Result<Option<aurelius_domain::ProjectId>, StorageError> {
    use rusqlite::OptionalExtension;
    let project: Option<String> = tx.query_row("SELECT project_id FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0)).optional()?.flatten();
    Ok(project.and_then(|p| aurelius_domain::ProjectId::parse(&p).ok()))
}

impl CommandService {
    pub(crate) fn send_chat_message(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &conversation::SendChatMessage,
    ) -> Result<Committed, AppError> {
        let text = cmd.validate().map_err(domain_error)?;
        let images = crate::conversation::message_images::prepare(&self.store, &cmd.images)?;
        let blobs = self.store.blobs();
        let text_blob = blobs.put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let conversation_id = cmd.conversation_id;
        let message_id = MessageId::new();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        let interrupt = cmd.interrupt;
        let store = self.store.clone();
        self.commit(command_id, hash, move |tx| {
            let mut notices = Vec::new();
            let correlation = command_id.to_string();
            if !conv::conversation_exists(tx, conversation_id)? {
                return Ok(Err(domain_error(DomainError::ConversationNotFound(conversation_id))));
            }
            blobs::register(tx, &text_blob)?;
            let (event_id, n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: conversation_id.to_string(),
                event_type: "message.user_committed",
                actor: ActorKind::User,
                actor_id: Some(actor),
                correlation: correlation.clone(),
                payload: Some(text_blob.id.to_string()),
                metadata: meta.clone(),
                dedupe: format!("cmd:{command_id}:message"),
            })?;
            notices.push(n);
            conv::insert_message(tx, NewMessage {
                id: message_id,
                conversation_id,
                role: MessageRole::User,
                content_blob_id: Some(text_blob.id.to_string()),
                source_event_id: event_id,
                status: MessageStatus::Complete,
                run_id: None,
                reply_to: None,
            })?;
            crate::conversation::message_images::keep(tx, message_id, &images)?;
            if interrupt {
                notices.extend(interrupt_conversation(tx, &store, &meta, &correlation, conversation_id)?);
            }
            let (run_id, n) = queue_run(tx, &meta, &correlation, conversation_id, message_id)?;
            notices.extend(n);
            // A finished task whose conversation gets a run: its live run changed.
            notices.extend(crate::task::task_runs::live_run_notice(tx, conversation_id)?);
            Ok(Ok((CommandResult::ChatMessageSent { conversation_id, message_id, run_id }, notices)))
        })
    }

    pub(crate) fn retry_reply(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &conversation::RetryReply,
    ) -> Result<Committed, AppError> {
        let cmd = cmd.clone();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            if !conv::conversation_exists(tx, cmd.conversation_id)? {
                return Ok(Err(domain_error(DomainError::ConversationNotFound(cmd.conversation_id))));
            }
            if !conv::user_message_in(tx, cmd.conversation_id, cmd.user_message_id)? {
                return Ok(Err(domain_error(DomainError::MessageNotFound(cmd.user_message_id))));
            }
            let (run_id, mut notices) = queue_run(tx, &meta, &command_id.to_string(), cmd.conversation_id, cmd.user_message_id)?;
            notices.extend(crate::task::task_runs::live_run_notice(tx, cmd.conversation_id)?);
            Ok(Ok((CommandResult::ReplyRetried { run_id }, notices)))
        })
    }

    pub(crate) fn stop_run(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &conversation::StopRun,
    ) -> Result<Committed, AppError> {
        let run_id = cmd.run_id;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let store = self.store.clone();
        self.commit(command_id, hash, move |tx| {
            let Some(status) = conv::run_status(tx, run_id)? else {
                return Ok(Err(domain_error(DomainError::RunNotFound(run_id))));
            };
            match request_stop(tx, &store, &meta, &command_id.to_string(), run_id, &status, format!("cmd:{command_id}:cancel"))? {
                None => Ok(Ok((CommandResult::StopRequested { run_id, already_finished: true }, vec![]))),
                Some(notices) => Ok(Ok((CommandResult::StopRequested { run_id, already_finished: false }, notices))),
            }
        })
    }

    /// Remove a user message that still waits for its answer, with its queued run.
    pub(crate) fn delete_queued_message(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &conversation::DeleteQueuedMessage) -> Result<Committed, AppError> {
        let (conversation_id, message_id) = (cmd.conversation_id, cmd.message_id);
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let store = self.store.clone();
        self.commit(command_id, hash, move |tx| {
            let correlation = command_id.to_string();
            let run_id = match waiting_run(tx, conversation_id, message_id)? {
                Ok(run) => run,
                Err(err) => return Ok(Err(err)),
            };
            let mut notices = request_stop(tx, &store, &meta, &correlation, run_id, "created", format!("cmd:{command_id}:cancel"))?.unwrap_or_default();
            conv::delete_message(tx, message_id)?;
            let (_, mut n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: conversation_id.to_string(),
                event_type: "message.deleted",
                actor: ActorKind::User,
                actor_id: None,
                correlation,
                payload: None,
                metadata: serde_json::json!({ "message_id": message_id, "run_id": run_id, "ctx": meta }),
                dedupe: format!("cmd:{command_id}:message"),
            })?;
            n.project_id = conversation_project(tx, conversation_id)?;
            notices.push(n);
            // In a task's conversation: a running task with nothing else running
            // for it waits for the user, as after a stop.
            let task_running: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM runs r JOIN tasks t ON t.id = r.task_id WHERE r.id = ?1 AND t.status = 'running')",
                [run_id.to_string()],
                |r| r.get(0),
            )?;
            if task_running {
                notices.extend(crate::task::task_runs::on_run_finished(tx, &store, run_id, false, None, "")?);
            }
            Ok(Ok((CommandResult::QueuedMessageDeleted { conversation_id, message_id, run_id }, notices)))
        })
    }

    /// "Send now" for a waiting message: stop what runs or waits ahead of it and
    /// let its own run go first; the messages after it keep waiting.
    pub(crate) fn send_queued_now(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &conversation::SendQueuedNow) -> Result<Committed, AppError> {
        let (conversation_id, message_id) = (cmd.conversation_id, cmd.message_id);
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let store = self.store.clone();
        self.commit(command_id, hash, move |tx| {
            let correlation = command_id.to_string();
            let run_id = match waiting_run(tx, conversation_id, message_id)? {
                Ok(run) => run,
                Err(err) => return Ok(Err(err)),
            };
            let ordinal = conv::message_ordinal(tx, message_id)?.unwrap_or(0);
            let mut notices = Vec::new();
            for (ahead, status) in conv::runs_ahead_of(tx, conversation_id, run_id, ordinal)? {
                // Already stopping: the daemon interrupts it again anyway.
                if status == "cancelling" {
                    continue;
                }
                if let Some(n) = request_stop(tx, &store, &meta, &correlation, ahead, &status, format!("cmd:{command_id}:cancel:{ahead}"))? {
                    notices.extend(n);
                }
            }
            conv::hurry_job(tx, run_id)?;
            let (_, mut n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: conversation_id.to_string(),
                event_type: "message.sent_now",
                actor: ActorKind::User,
                actor_id: None,
                correlation,
                payload: None,
                metadata: serde_json::json!({ "message_id": message_id, "run_id": run_id, "ctx": meta }),
                dedupe: format!("cmd:{command_id}:message"),
            })?;
            n.project_id = conversation_project(tx, conversation_id)?;
            notices.push(n);
            Ok(Ok((CommandResult::QueuedMessageSentNow { conversation_id, message_id, run_id }, notices)))
        })
    }

    pub(crate) fn set_agent_mode(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &aurelius_domain::autonomy::SetAgentMode) -> Result<Committed, AppError> {
        let cmd = cmd.clone();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let conversation_id = cmd.conversation_id;
        let mut committed = self.commit(command_id, hash, move |tx| {
            if !conv::set_agent_mode(tx, cmd.conversation_id, cmd.mode)? {
                return Ok(Err(domain_error(DomainError::ConversationNotFound(cmd.conversation_id))));
            }
            let (_, n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: cmd.conversation_id.to_string(),
                event_type: "conversation.settings_changed",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({ "agent_mode": cmd.mode.as_str(), "ctx": meta }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            Ok(Ok((CommandResult::AgentModeSet { conversation_id: cmd.conversation_id, mode: cmd.mode }, vec![n])))
        })?;
        if !committed.receipt.replayed {
            committed.followups.push(crate::Followup::AgentModeChanged(conversation_id));
        }
        Ok(committed)
    }

    /// Model and effort for one conversation; its next answers run with them.
    pub(crate) fn set_conversation_run(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &aurelius_domain::autonomy::SetConversationRun) -> Result<Committed, AppError> {
        let run = aurelius_domain::task::RunSettings { model: cmd.model.clone(), effort: cmd.effort.clone(), ..Default::default() }.validated().map_err(domain_error)?;
        let conversation_id = cmd.conversation_id;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            if !conv::set_conversation_run(tx, conversation_id, &run)? {
                return Ok(Err(domain_error(DomainError::ConversationNotFound(conversation_id))));
            }
            let (_, n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: conversation_id.to_string(),
                event_type: "conversation.settings_changed",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({ "model": run.model, "effort": run.effort, "ctx": meta }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            Ok(Ok((CommandResult::ConversationRunSet { conversation_id }, vec![n])))
        })
    }

    pub(crate) fn set_browser_allowed(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &aurelius_domain::autonomy::SetBrowserAllowed) -> Result<Committed, AppError> {
        let cmd = cmd.clone();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            if !conv::set_browser_allowed(tx, cmd.conversation_id, cmd.allowed)? {
                return Ok(Err(domain_error(DomainError::ConversationNotFound(cmd.conversation_id))));
            }
            let (_, n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: cmd.conversation_id.to_string(),
                event_type: "conversation.settings_changed",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({ "browser_allowed": cmd.allowed, "ctx": meta }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            Ok(Ok((CommandResult::BrowserAllowedSet { conversation_id: cmd.conversation_id, allowed: cmd.allowed }, vec![n])))
        })
    }

    pub(crate) fn set_screen_allowed(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &aurelius_domain::autonomy::SetScreenAllowed) -> Result<Committed, AppError> {
        let cmd = cmd.clone();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            if !conv::set_screen_allowed(tx, cmd.conversation_id, cmd.allowed)? {
                return Ok(Err(domain_error(DomainError::ConversationNotFound(cmd.conversation_id))));
            }
            let (_, n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: cmd.conversation_id.to_string(),
                event_type: "conversation.settings_changed",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({ "screen_allowed": cmd.allowed, "ctx": meta }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            Ok(Ok((CommandResult::ScreenAllowedSet { conversation_id: cmd.conversation_id, allowed: cmd.allowed }, vec![n])))
        })
    }

    pub(crate) fn delete_conversation(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &conversation::DeleteConversation) -> Result<Committed, AppError> {
        let id = cmd.conversation_id;
        let keys = self.store.read(|c| conv::content_blob_keys(c, id)).map_err(storage_error)?;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let store = self.store.clone();
        let committed = self.commit(command_id, hash, move |tx| {
            let kind: Option<String> = tx.query_row("SELECT kind FROM conversations WHERE id = ?1 AND archived_at_ms IS NULL", [id.to_string()], |r| r.get(0)).ok();
            match kind.as_deref() {
                None => return Ok(Err(domain_error(DomainError::ConversationNotFound(id)))),
                Some("employee") if conv::is_main_employee_conversation(tx, id)? => {
                    return Ok(Err(domain_error(DomainError::Memory("This is the employee's main conversation; archive the employee instead.".into()))));
                }
                Some("group") => return Ok(Err(domain_error(DomainError::Memory("The company's group chat cannot be deleted.".into())))),
                _ => {}
            }
            // What still runs or waits in it is stopped; a run stuck in 'cancelling' no longer blocks the delete.
            // shortcut: text an interrupted answer flushes after this commit isn't erased; erase it at run end if that matters.
            let mut notices = interrupt_conversation(tx, &store, &meta, &command_id.to_string(), id)?;
            conv::delete_conversation(tx, id)?;
            let (_, n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: id.to_string(),
                event_type: "conversation.deleted",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({ "ctx": meta }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            notices.push(n);
            Ok(Ok((CommandResult::ConversationDeleted { conversation_id: id }, notices)))
        })?;
        // After the commit: erase the content itself.
        if !committed.receipt.replayed {
            for key in &keys {
                if let Err(err) = self.store.blobs().erase(key) {
                    tracing::warn!(error = %err, "could not erase chat content");
                }
            }
            if let Err(err) = aurelius_storage::backup::scrub(self.store.root(), &keys, &[], &[id.to_string()]) {
                tracing::warn!(error = %err, "could not scrub the deleted chat from backups");
            }
        }
        Ok(committed)
    }
}
