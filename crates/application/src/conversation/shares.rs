//! Conversation ↔ Company sharing: explicit snapshots with a source,
//! one destination each, revocable. Only the selected content moves.

use crate::run::events::{Ev, emit};
use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, ConversationId, DomainError, EmployeeId, EventId, MemoryId, ShareId,
    conversation::{MessageRole, MessageStatus},
    employee::LifecycleStatus,
    memory::{MemoryKind, MemoryScope, MemoryStatus},
    share::{DiscussTask, MAX_SNAPSHOT_CHARS, RevokeShare, ShareMessage, ShareMode, ShareTarget},
};
use aurelius_domain::{CommandEnvelope, CommandResult, AppError};
use aurelius_storage::{blobs, conversations as conv, memory as mem, shares as store};

fn share_error(message: impl Into<String>) -> AppError {
    domain_error(DomainError::Memory(message.into()))
}

fn title_of(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Shared text");
    let line = line.trim_start_matches('#').trim();
    let mut title: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        title.push('…');
    }
    title
}

impl CommandService {
    pub(crate) fn share_message(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &ShareMessage) -> Result<Committed, AppError> {
        let store_ref = self.store.clone();
        let (message, conversation_title, source_event) = self
            .store
            .read(|c| {
                let employee: Option<Option<String>> = c.query_row("SELECT employee_id FROM conversations WHERE id = ?1", [cmd.conversation_id.to_string()], |r| r.get(0)).ok();
                if employee != Some(None) {
                    return Ok(None);
                }
                let read = |key: &str| store_ref.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let message = conv::list_messages(c, cmd.conversation_id, &read)?.into_iter().find(|m| m.id == cmd.message_id);
                let title: String = c.query_row("SELECT title FROM conversations WHERE id = ?1", [cmd.conversation_id.to_string()], |r| r.get(0))?;
                let source: Option<String> = c.query_row("SELECT source_event_id FROM messages WHERE id = ?1", [cmd.message_id.to_string()], |r| r.get(0)).ok();
                Ok(message.map(|m| (m, title, source)))
            })
            .map_err(storage_error)?
            .ok_or_else(|| share_error("Only messages from a project conversation can be shared."))
            .map(|(m, t, s)| (m, t, s))?;
        if message.status != MessageStatus::Complete || message.text.trim().is_empty() {
            return Err(share_error("Only finished messages can be shared."));
        }
        let source_event = source_event.and_then(|s| EventId::parse(&s).ok()).ok_or_else(|| share_error("This message has no source."))?;
        let snapshot: String = message.text.chars().take(MAX_SNAPSHOT_CHARS).collect();
        let blob = self.store.blobs().put(snapshot.as_bytes(), "text/markdown; charset=utf-8").map_err(storage_error)?;
        // A memory proposal is limited in size; the share keeps the full snapshot.
        let memory_text: String = snapshot.chars().take(aurelius_domain::memory::MAX_MEMORY_CHARS - 1).collect();
        let memory_blob = match cmd.mode {
            ShareMode::Memory => Some(self.store.blobs().put(memory_text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?),
            ShareMode::Context => None,
        };
        let title = title_of(&snapshot);
        let source_label = format!("{} · {}", if message.role == MessageRole::User { "Your message" } else { "Answer" }, conversation_title);
        let cmd = cmd.clone();
        let id = ShareId::new();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            let scope = match cmd.target {
                ShareTarget::Project(p) => mem::scope_id(tx, MemoryScope::Project(p))?,
                ShareTarget::Employee(m) => mem::scope_id(tx, MemoryScope::Employee(m))?,
            };
            let Some(scope) = scope else { return Ok(Err(share_error("This destination no longer exists."))) };
            blobs::register(tx, &blob)?;
            let (grant, n) = emit(tx, Ev {
                aggregate_type: "share",
                aggregate_id: id.to_string(),
                event_type: "share.granted",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: Some(blob.id.to_string()),
                metadata: serde_json::json!({ "target": cmd.target, "mode": cmd.mode.as_str(), "message_id": cmd.message_id, "title": title, "ctx": meta }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            let memory_id = match &memory_blob {
                Some(mblob) => {
                    blobs::register(tx, mblob)?;
                    let memory = MemoryId::new();
                    mem::insert(tx, mem::NewMemory {
                        id: memory,
                        scope_id: &scope,
                        kind: MemoryKind::ProjectKnowledge,
                        blob_id: &mblob.id.to_string(),
                        text: &memory_text,
                        status: MemoryStatus::Proposed,
                        confidence: 1.0,
                        importance: 0.6,
                        pinned: false,
                        supersedes: None,
                        conflicts_with: None,
                        decided_by: None,
                        locator: serde_json::json!({ "share_id": id }),
                        sources: vec![(grant, serde_json::json!({ "share": id }))],
                        extraction_version: "share",
                    })?;
                    Some(memory)
                }
                None => None,
            };
            store::insert(tx, store::NewShare {
                id,
                source_event,
                grant_event: grant,
                scope_id: Some(&scope),
                conversation: None,
                blob_id: &blob.id.to_string(),
                mode: cmd.mode,
                title: &title,
                source_label: &source_label,
                memory_id,
            })?;
            Ok(Ok((CommandResult::Shared { share_id: id, memory_id }, vec![n])))
        })
    }

    /// "Discuss": the task's result goes as context into the chat of the task's
    /// lead employee (its persistent conversation), which the client then opens.
    pub(crate) fn discuss_task(&self, envelope: &CommandEnvelope, hash: String, cmd: &DiscussTask) -> Result<Committed, AppError> {
        let store_ref = self.store.clone();
        let (title, text, employee) = self
            .store
            .read(|c| {
                let row: Option<(String, Option<String>, String, Option<String>)> = c
                    .query_row(
                        "SELECT t.title, rb.storage_key, gb.storage_key, t.employee_id FROM tasks t JOIN blobs gb ON gb.id = t.brief_blob_id
                         LEFT JOIN blobs rb ON rb.id = t.result_blob_id WHERE t.id = ?1",
                        [cmd.task_id.to_string()],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                    )
                    .ok();
                let read = |key: &str| store_ref.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                Ok(row.map(|(title, result, goal, employee)| {
                    let body = result.as_deref().and_then(read).unwrap_or_else(|| format!("Goal: {}", read(&goal).unwrap_or_default()));
                    (title, body, employee.and_then(|e| EmployeeId::parse(&e).ok()))
                }))
            })
            .map_err(storage_error)?
            .ok_or_else(|| domain_error(DomainError::TaskNotFound(cmd.task_id)))?;
        let employee = employee.ok_or_else(|| share_error("This task has no lead employee to discuss it with."))?;
        let snapshot: String = text.chars().take(MAX_SNAPSHOT_CHARS).collect();
        let blob = self.store.blobs().put(snapshot.as_bytes(), "text/markdown; charset=utf-8").map_err(storage_error)?;
        let task = cmd.task_id;
        let id = ShareId::new();
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            let Some(row) = aurelius_storage::employees::load_employee(tx, employee)? else {
                return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee))));
            };
            if row.status == LifecycleStatus::Archived {
                return Ok(Err(share_error("The task's lead employee is archived.")));
            }
            let conversation = row.conversation_id;
            blobs::register(tx, &blob)?;
            let (grant, n) = emit(tx, Ev {
                aggregate_type: "share",
                aggregate_id: id.to_string(),
                event_type: "share.granted",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: Some(blob.id.to_string()),
                metadata: serde_json::json!({ "task_id": task, "conversation_id": conversation, "employee_id": employee, "title": title }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            store::insert(tx, store::NewShare {
                id,
                source_event: grant,
                grant_event: grant,
                scope_id: None,
                conversation: Some(conversation),
                blob_id: &blob.id.to_string(),
                mode: ShareMode::Context,
                title: &title,
                source_label: &format!("Task · {title}"),
                memory_id: None,
            })?;
            Ok(Ok((CommandResult::TaskDiscussed { conversation_id: conversation, employee_id: employee }, vec![n])))
        })
    }

    pub(crate) fn revoke_share(&self, envelope: &CommandEnvelope, hash: String, cmd: &RevokeShare) -> Result<Committed, AppError> {
        let id = cmd.share_id;
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            let Some(memory) = store::revoke(tx, id)? else { return Ok(Err(share_error("This share no longer exists or was already revoked."))) };
            // Derived memory stops too: a proposal is rejected, an accepted one forgotten.
            if let Some(memory) = memory {
                if let Some(row) = mem::load(tx, memory)? {
                    match row.status {
                        MemoryStatus::Proposed => mem::set_status(tx, memory, MemoryStatus::Rejected, Some("share revoked"))?,
                        MemoryStatus::Active => {
                            mem::set_status(tx, memory, MemoryStatus::Forgotten, Some("share revoked"))?;
                            mem::deindex(tx, memory)?;
                        }
                        _ => {}
                    }
                }
            }
            let (_, n) = emit(tx, Ev {
                aggregate_type: "share",
                aggregate_id: id.to_string(),
                event_type: "share.revoked",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({}),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            Ok(Ok((CommandResult::ShareRevoked { share_id: id }, vec![n])))
        })
    }
}

/// Active shares as pinned context candidates (labelled data).
pub(crate) fn share_candidates(
    conn: &rusqlite::Connection,
    scopes: &[String],
    conversation: Option<ConversationId>,
    read_blob: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<crate::memory::context::Candidate>, aurelius_storage::StorageError> {
    Ok(store::active(conn, scopes, conversation)?
        .into_iter()
        .filter_map(|(id, title, source, key)| {
            Some(crate::memory::context::Candidate {
                id: id.to_string(),
                kind: MemoryKind::ProjectKnowledge,
                text: read_blob(&key)?,
                lexical: 0.0,
                importance: 0.8,
                created_at_ms: aurelius_storage::now_ms(),
                pinned: true,
                source: format!("share:{id}"),
                label: format!("Shared · {title} ({source})"),
                lesson: false,
            })
        })
        .collect())
}
