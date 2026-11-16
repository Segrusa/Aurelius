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
    let user_id = peer.user;
    CommandContext { principal_id: shared.principal_id, client_id: peer.client_id, origin: peer.origin, request_id, user_id }
}

/// The paths of the files a command attaches (text attachments are checked
/// too): only its attachment fields `files` and `add_files` (a message's, a
/// task's, an idea's), never another field that happens to hold a path
/// (R1 m6).
fn attached(value: &serde_json::Value) -> Vec<String> {
    let mut paths = Vec::new();
    for field in ["files", "add_files"] {
        if let Some(serde_json::Value::Array(items)) = value.get(field) {
            paths.extend(items.iter().filter_map(|i| i.as_str().map(str::to_owned)));
        }
    }
    paths
}

/// B10 (SEC0 E6), before the pipeline: a command whose text holds a vault
/// secret is refused with `looks_like_secret` (the items' names only, never
/// where or what), or with `confirm_secret` kept with the value redacted, so
/// the stored text, the request hash and the receipt never hold it. A text
/// file attached with one is always refused. A refusal leaves nothing.
pub(super) fn b10(command: Command) -> Result<Command, ProtocolError> {
    let redactor = aurelius_runtime::redact::current();
    if redactor.is_empty() {
        return Ok(command);
    }
    let confirmed = match &command {
        Command::SendEmployeeMessage(c) => c.confirm_secret,
        Command::SendGroupMessage(c) => c.confirm_secret,
        Command::PostTicketMessage(c) => c.confirm_secret,
        Command::EditMessage(c) => c.confirm_secret,
        Command::SendChatMessage(c) => c.confirm_secret,
        Command::SendProjectMessage(c) => c.confirm_secret,
        Command::StartWorkSession(c) => c.confirm_secret,
        Command::DispatchWorkSession(c) => c.confirm_secret,
        Command::CreateTask(c) => c.confirm_secret,
        Command::CreateTaskSchedule(c) => c.confirm_secret,
        Command::UpdateTaskSchedule(c) => c.confirm_secret,
        Command::AddIdea(c) => c.confirm_secret,
        Command::UpdateIdea(c) => c.confirm_secret,
        Command::CreateTicket(c) => c.confirm_secret,
        Command::UpdateTicket(c) => c.confirm_secret,
        Command::RevisePlan(c) => c.confirm_secret,
        Command::SendBackTicket(c) => c.confirm_secret,
        Command::AcceptTicket(c) => c.confirm_secret,
        Command::DecidePlan(c) => c.confirm_secret,
        Command::AnswerQuestion(c) => c.confirm_secret,
        // Names, instructions, memories and paths stay refused (AUR-MU-04-F5).
        _ => false,
    };
    // Fail-closed (SEC1 m2): what cannot be checked is not stored.
    let Ok(mut value) = serde_json::to_value(&command) else {
        return Err(ProtocolError::new(ErrorCode::Validation, "This message could not be checked for vault values."));
    };
    let mut names: Vec<String> = Vec::new();
    let add = |found: Vec<String>, names: &mut Vec<String>| {
        for name in found {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    };
    fn strings<'a>(v: &'a serde_json::Value, out: &mut Vec<&'a str>) {
        match v {
            serde_json::Value::String(s) => out.push(s),
            serde_json::Value::Array(items) => items.iter().for_each(|i| strings(i, out)),
            serde_json::Value::Object(map) => map.iter().for_each(|(k, v)| {
                out.push(k);
                strings(v, out)
            }),
            _ => {}
        }
    }
    let mut texts = Vec::new();
    strings(&value, &mut texts);
    for text in texts {
        add(redactor.names_in(text), &mut names);
    }
    let mut in_files: Vec<String> = Vec::new();
    for path in attached(&value) {
        let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= 1024 * 1024);
        if let Some(text) = small.then(|| std::fs::read(&path).ok()).flatten().and_then(|b| String::from_utf8(b).ok()) {
            add(redactor.names_in(&text), &mut in_files);
        }
    }
    if !in_files.is_empty() {
        add(in_files, &mut names);
        return Err(aurelius_domain::AppError::looks_like_secret(names));
    }
    if names.is_empty() {
        return Ok(command);
    }
    if !confirmed {
        return Err(aurelius_domain::AppError::looks_like_secret(names));
    }
    redactor.redact_json(&mut value);
    serde_json::from_value(value).map_err(|_| ProtocolError::new(ErrorCode::Validation, "This message could not be stored without the secret."))
}

async fn domain(shared: &Arc<Shared>, ctx: CommandContext, key: CommandId, expected_version: Option<u64>, command: Command) -> Result<Receipt, ProtocolError> {
    let command = b10(command)?;
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
    let hash = request_hash_of(&(expected_version, &write));

    let (s, h) = (shared.clone(), hash.clone());
    if let Some((committed_seq, stored)) = app_task(move || s.service.host_receipt(key, &h)).await? {
        let result: HostResult = serde_json::from_value(stored).map_err(|e| internal(format!("corrupt receipt: {e}")))?;
        return Ok(Receipt { idempotency_key: key, committed_seq, replayed: true, result: WriteResult::Host(result) });
    }

    let result = host::perform(shared, write, ctx.user_id).await?;
    let stored = serde_json::to_value(result.redacted()).map_err(internal)?;
    let s = shared.clone();
    let (committed_seq, notice) = app_task(move || s.service.record_host_write(&ctx, key, hash, &method, &stored)).await?;
    shared.publish([notice]);
    // The answer shows what the receipt hides (the pairing code).
    Ok(Receipt { idempotency_key: key, committed_seq, replayed: false, result: WriteResult::Host(result) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R1 m6: B10 reads the files a command attaches, not any field named `path`.
    #[test]
    fn b10_reads_attachments_only() {
        let owner = "b10-attachments-test";
        let value = "sk-test-FAKE-b10-attachment-0000";
        aurelius_runtime::redact::add_to_set(owner, aurelius_runtime::redact::Secret { name: "B10_FILE".into(), label: String::new(), value: zeroize::Zeroizing::new(value.into()) });
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, format!("token {value}\n")).unwrap();
        let file = file.display().to_string();
        let command = |v: serde_json::Value| -> Command { serde_json::from_value(v).unwrap() };

        let folder = command(serde_json::json!({ "type": "set_employee_folder", "employee_id": aurelius_domain::EmployeeId::new(), "path": file }));
        let folder_ok = b10(folder).is_ok();
        let post = command(serde_json::json!({ "type": "send_group_message", "text": "see the file", "files": [file] }));
        let post_refused = b10(post).map(drop).unwrap_err();
        aurelius_runtime::redact::install_set(owner, Vec::new());

        assert!(folder_ok, "a folder path is not an attachment");
        assert_eq!(post_refused.code, ErrorCode::LooksLikeSecret);
    }

    #[test]
    fn b10_refuses_an_edit_with_a_vault_value_and_redacts_it_once_confirmed() {
        use aurelius_domain::group_chat::EditMessage;
        let owner = "b10-edit-test";
        let value = "sk-test-FAKE-b10-edit-0000";
        aurelius_runtime::redact::add_to_set(owner, aurelius_runtime::redact::Secret { name: "B10_EDIT".into(), label: String::new(), value: zeroize::Zeroizing::new(value.into()) });
        let edit = |confirm_secret: bool| Command::EditMessage(EditMessage { message_id: aurelius_domain::MessageId::new(), text: format!("token {value}"), confirm_secret });
        let refused = b10(edit(false)).map(drop).unwrap_err();
        let confirmed = b10(edit(true));
        aurelius_runtime::redact::install_set(owner, Vec::new());

        assert_eq!(refused.code, ErrorCode::LooksLikeSecret);
        let Ok(Command::EditMessage(kept)) = confirmed else { panic!("the `match` in b10 must let a confirmed edit through") };
        assert!(!kept.text.contains(value), "{}", kept.text);
    }
}
