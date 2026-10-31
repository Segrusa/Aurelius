//! The one write pipeline. Every write, a domain command or a host write, needs
//! an idempotency key; the right it requires was checked by the session before.
//!
//! * Domain commands run in `application` (receipt and audit event in their own
//!   transaction); this adds what happens after the commit: publishing, follow-ups,
//!   stopping runs.
//! * Host writes run here: the same key with the same request answers from the
//!   stored receipt without doing the action again; a new one is done, then
//!   recorded through `record_host_write` (audit event plus receipt, secrets
//!   redacted). A failed action leaves no receipt, so a retry runs again.

use super::{Peer, app_task, host, internal};
use crate::Shared;
use aurelius_application::{CommandContext, request_hash_of};
use aurelius_domain::{Command, CommandEnvelope, CommandId, CommandResult};
use aurelius_protocol::{ErrorCode, HostResult, HostWrite, ProtocolError, Receipt, ResponseBody, WriteMethod, WriteResult};
use std::sync::Arc;

pub(super) async fn execute(
    shared: &Arc<Shared>,
    peer: &mut Peer,
    request_id: u64,
    key: Option<CommandId>,
    expected_version: Option<u64>,
    write: WriteMethod,
) -> Result<ResponseBody, ProtocolError> {
    let Some(key) = key else {
        return Err(ProtocolError::new(ErrorCode::InvalidRequest, "writes need an idempotency_key"));
    };
    let ctx = context(shared, peer, request_id);
    let receipt = match write {
        // Answered by `users::create` before this (the host pipeline would hash and keep the password).
        WriteMethod::Host(HostWrite::UserCreate(_)) => return Err(internal("user.create reached the write pipeline")),
        WriteMethod::Domain(command) => domain(shared, ctx, key, expected_version, command).await?,
        WriteMethod::Host(write) => host_write(shared, ctx, key, expected_version, write).await?,
    };
    Ok(ResponseBody::Receipt(receipt))
}

/// Who makes a write: the board member always from the session, never from the request (MU3-08).
pub(super) fn context(shared: &Arc<Shared>, peer: &Peer, request_id: u64) -> CommandContext {
    let app = peer.app.as_ref().map(|a| aurelius_application::AppActor { key_id: a.key_id, name: a.name.clone() });
    // Never a board member for an app, even if one stuck to the session (S5-1).
    let user_id = peer.user.filter(|_| app.is_none());
    CommandContext { principal_id: shared.principal_id, client_id: peer.client_id, origin: peer.origin, request_id, app, user_id }
}

async fn domain(shared: &Arc<Shared>, ctx: CommandContext, key: CommandId, expected_version: Option<u64>, command: Command) -> Result<Receipt, ProtocolError> {
    let interrupt = match &command {
        Command::SendChatMessage(c) => c.interrupt,
        Command::SendEmployeeMessage(c) => c.interrupt,
        Command::SendProjectMessage(c) => c.interrupt,
        _ => false,
    };
    let envelope = CommandEnvelope { command_id: key, expected_version, command };
    let s = shared.clone();
    let committed = app_task(move || s.service.execute(&ctx, envelope)).await?;
    let receipt = committed.receipt;
    if !receipt.replayed {
        stop_runs(shared, &receipt.result, interrupt).await;
    }
    shared.publish(committed.notices);
    shared.follow_up(committed.followups);
    Ok(Receipt { idempotency_key: receipt.command_id, committed_seq: receipt.committed_seq, replayed: receipt.replayed, result: WriteResult::Command(receipt.result) })
}

/// The runs a committed command asked to stop are cancelled at once.
async fn stop_runs(shared: &Arc<Shared>, result: &CommandResult, interrupt: bool) {
    let mut runs = Vec::new();
    if let CommandResult::StopRequested { run_id, already_finished: false } = result {
        runs.push(*run_id);
    }
    // "Send now": the answers it stopped are interrupted at once.
    let conversation = match result {
        CommandResult::ChatMessageSent { conversation_id, .. }
        | CommandResult::EmployeeMessageSent { conversation_id, .. }
        | CommandResult::ProjectMessageSent { conversation_id, .. }
            if interrupt =>
        {
            Some(*conversation_id)
        }
        CommandResult::QueuedMessageSentNow { conversation_id, .. } | CommandResult::ConversationDeleted { conversation_id } => Some(*conversation_id),
        _ => None,
    };
    if let Some(conversation) = conversation {
        let store = shared.store().clone();
        runs.extend(
            tokio::task::spawn_blocking(move || store.read(|c| aurelius_storage::conversations::cancelling_runs_in(c, conversation)))
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default(),
        );
    }
    // A cancelled task stops the steps it still runs.
    if let CommandResult::TaskCancelled { task_id } = result {
        let (store, task) = (shared.store().clone(), *task_id);
        runs.extend(
            tokio::task::spawn_blocking(move || store.read(|c| aurelius_storage::tasks::active_runs(c, task)))
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default(),
        );
    }
    // A ticket put back in Backlog (cancelled): its main task and delegations ended, so do their runs.
    let ticket_cancelled = matches!(result, CommandResult::TicketChanged { column: aurelius_domain::ticket::ColumnKind::Backlog, .. });
    if let CommandResult::TicketChanged { ticket_id, column: aurelius_domain::ticket::ColumnKind::Backlog, .. } = result {
        let (store, ticket) = (shared.store().clone(), *ticket_id);
        let active = move |c: &rusqlite::Connection| -> Result<Vec<aurelius_domain::RunId>, aurelius_storage::StorageError> {
            let mut runs = Vec::new();
            for task in aurelius_storage::tickets::tasks_of_ticket(c, ticket)? {
                runs.extend(aurelius_storage::tasks::active_runs(c, task)?);
            }
            Ok(runs)
        };
        runs.extend(tokio::task::spawn_blocking(move || store.read(active)).await.ok().and_then(Result::ok).unwrap_or_default());
    }
    for run in runs {
        if let Some(canceller) = shared.cancellers.lock().ok().and_then(|m| m.get(&run).cloned()) {
            canceller.cancel();
        }
    }
    // A cancelled task's trials stop with it: their worktrees go once their runs ended.
    if let CommandResult::TaskCancelled { task_id } = result {
        let (s, task) = (shared.clone(), *task_id);
        tokio::spawn(async move {
            for _ in 0..20 {
                let store = s.store().clone();
                let live = tokio::task::spawn_blocking(move || store.read(|c| aurelius_storage::variants::live_probe_runs(c, task))).await.ok().and_then(Result::ok).unwrap_or_default();
                if live.is_empty() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            crate::variants::remove_unneeded(&s, task).await;
        });
    }
    // A run stopped while it waited for a usage limit never resumes: its kept session goes.
    let stopped = conversation.is_some()
        || ticket_cancelled
        || matches!(result, CommandResult::StopRequested { already_finished: false, .. } | CommandResult::TaskCancelled { .. } | CommandResult::ConversationDeleted { .. });
    if stopped {
        crate::sessions::sweep(shared);
    }
}

async fn host_write(shared: &Arc<Shared>, ctx: CommandContext, key: CommandId, expected_version: Option<u64>, write: HostWrite) -> Result<Receipt, ProtocolError> {
    let method = write.name();
    // Note writes announce themselves with `note.*` events; their audit record is
    // not broadcast, so an autosave every second does not make clients refresh everything.
    let quiet = method.starts_with("note.");
    let hash = request_hash_of(&(expected_version, &write));

    let (s, h) = (shared.clone(), hash.clone());
    if let Some((committed_seq, stored)) = app_task(move || s.service.host_receipt(key, &h)).await? {
        let result: HostResult = serde_json::from_value(stored).map_err(|e| internal(format!("corrupt receipt: {e}")))?;
        return Ok(Receipt { idempotency_key: key, committed_seq, replayed: true, result: WriteResult::Host(result) });
    }

    let user = ctx.user_id.filter(|_| ctx.app.is_none());
    let reconfigured = match &write {
        aurelius_protocol::HostWrite::ApiKeySetFunctions { key_id, .. } => Some(*key_id),
        _ => None,
    };
    let result = host::perform(shared, write, user).await?;
    // An app key's tasks run on its maker's Claude login (B5).
    if let (HostResult::AppKeyCreated(created), Some(user)) = (&result, user) {
        let (s, id) = (shared.clone(), created.key.id);
        app_task(move || s.service.set_app_key_login(id, user)).await?;
    }
    // So do an API key's calls: its maker's, or the board member who last set its functions (B5).
    let api_key = match &result {
        HostResult::ApiKeyCreated(created) => Some(created.key.id),
        _ => reconfigured,
    };
    if let (Some(id), Some(user)) = (api_key, user) {
        let s = shared.clone();
        app_task(move || s.service.set_api_key_login(id, user)).await?;
    }
    let stored = serde_json::to_value(result.redacted()).map_err(internal)?;
    let s = shared.clone();
    let (committed_seq, notice) = app_task(move || s.service.record_host_write(&ctx, key, hash, &method, &stored)).await?;
    if !quiet {
        shared.publish([notice]);
    }
    // The answer shows what the receipt hides (a new API key's secret, the pairing code).
    Ok(Receipt { idempotency_key: key, committed_seq, replayed: false, result: WriteResult::Host(result) })
}
