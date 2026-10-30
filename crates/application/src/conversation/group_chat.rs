//! The company's group chat ("Team chat"): the user and every employee in one
//! conversation (kind `group`).
//!
//! * Who answers: the employees the message names with `@Name`; without a name,
//!   the top of the org chart (the CEO). A post of an employee wakes only the
//!   colleagues it names, never itself, and at most `MAX_HOPS` hops after one
//!   message of the user (RULE 08).
//! * How: the message, with the latest group messages, goes to each of them as an
//!   update (`[Group chat · from …]`) in their own main chat, where a normal run
//!   answers it. Identity, session, mode, memory and the org chart stay as they are;
//!   several employees answer in parallel (one lease per conversation).
//! * The answer: copied to the group as the employee's message when the run ends
//!   (`mirror_answer`), streamed there live under that message's id meanwhile.

use crate::run::events::{Ev, emit};
use crate::run::queue::queue_run_marked;
use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::group_chat::{self as group, CONTEXT_CHARS, CONTEXT_MESSAGES, GROUP_KIND, MAX_HOPS, OWNER, POST_MAX};
use aurelius_domain::conversation::{MessageRole, MessageStatus};
use aurelius_domain::views::MessageFileView;
use aurelius_domain::{ActorKind, AppError, CommandEnvelope, CommandResult, ConversationId, EmployeeId, ErrorCode, EventNotice, MessageId, RunId};
use aurelius_storage::conversations::{self as conv, GroupRun, NewMessage};
use aurelius_storage::{StorageError, Store, Tx, blobs, employees};

/// The employees of the company that are not archived: (id, name, active).
fn team(conn: &rusqlite::Connection) -> Result<Vec<(EmployeeId, String, bool)>, StorageError> {
    let mut stmt = conn.prepare("SELECT id, name, status FROM employees WHERE status <> 'archived' ORDER BY created_at_ms, id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, status) = row?;
        if let Ok(id) = EmployeeId::parse(&id) {
            out.push((id, name, status == "active"));
        }
    }
    Ok(out)
}

/// Who answers `text`: the active employees it names (not `author`); without a
/// name, from the user (`author` none), the CEO. Paused employees are skipped.
fn addressees(conn: &rusqlite::Connection, text: &str, author: Option<EmployeeId>) -> Result<Vec<(EmployeeId, String)>, StorageError> {
    let team = team(conn)?;
    let names: Vec<(EmployeeId, String)> = team.iter().map(|(id, name, _)| (*id, name.clone())).collect();
    let mut chosen = group::mentions(text, &names);
    if chosen.is_empty() && author.is_none() {
        chosen.extend(aurelius_storage::app_keys::management(conn)?.into_iter().find(|m| m.is_ceo).map(|m| m.id));
    }
    Ok(chosen
        .into_iter()
        .filter(|id| Some(*id) != author)
        .filter_map(|id| team.iter().find(|(e, _, active)| *e == id && *active).map(|(e, name, _)| (*e, name.clone())))
        .collect())
}

/// The latest group messages before `before` (oldest first), as lines "Name: text".
fn context_text(conn: &rusqlite::Connection, store: &Store, group_id: ConversationId, before: MessageId) -> Result<String, StorageError> {
    let read = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let messages = conv::list_messages(conn, group_id, &read)?;
    let earlier: Vec<String> = messages
        .iter()
        .filter(|m| m.id != before && m.status == MessageStatus::Complete && !m.text.trim().is_empty())
        .map(|m| format!("{}: {}", author_of(m), group::shortened(&m.text, CONTEXT_CHARS)))
        .collect();
    let start = earlier.len().saturating_sub(CONTEXT_MESSAGES);
    Ok(earlier[start..].join("\n"))
}

/// Who wrote a group message, as the employees read it.
fn author_of(m: &aurelius_domain::views::MessageView) -> String {
    match (&m.update, &m.author) {
        (Some(_), Some(name)) => name.clone(),
        _ if m.role == MessageRole::User => OWNER.to_owned(),
        _ => "Aurelius".to_owned(),
    }
}

/// A message of an employee in the group chat: an update with its name as author,
/// under `message` (the id it was streamed under, for an answer).
fn post_in_group(tx: &Tx<'_>, store: &Store, group_id: ConversationId, message: MessageId, from: &str, text: &str, extra: serde_json::Value) -> Result<EventNotice, StorageError> {
    let blob = store.blobs().put(text.as_bytes(), "text/markdown; charset=utf-8")?;
    blobs::register(tx, &blob)?;
    let (event, notice) = emit(tx, Ev {
        aggregate_type: "conversation",
        aggregate_id: group_id.to_string(),
        event_type: "task.posted",
        actor: ActorKind::Employee,
        actor_id: None,
        correlation: group_id.to_string(),
        payload: Some(blob.id.to_string()),
        metadata: serde_json::json!({ "task_id": null, "kind": GROUP_KIND, "from": from, "message_id": message }),
        dedupe: format!("group-post:{message}"),
    })?;
    let mut meta = serde_json::json!({ "kind": GROUP_KIND, "from": from });
    if let (Some(m), Some(e)) = (meta.as_object_mut(), extra.as_object()) {
        m.extend(e.clone());
    }
    conv::insert_update(tx, message, group_id, &blob.id.to_string(), event, &meta)?;
    Ok(notice)
}

/// What a group message brings along for the employees: its images (shown to
/// them) and the files the user shared (named by their path on the server).
#[derive(Default)]
struct Shared<'a> {
    images: &'a [aurelius_domain::views::FileRef],
    files: Vec<String>,
}

/// Wake `employee` for the group message `message` (`text`, from `from`): the
/// message and the latest group messages go to its main chat as an update, and a
/// run there answers it. Its answer comes back to the group (`mirror_answer`).
#[allow(clippy::too_many_arguments)]
fn wake(
    tx: &Tx<'_>,
    store: &Store,
    meta: &serde_json::Value,
    correlation: &str,
    group_id: ConversationId,
    message: MessageId,
    (employee, name): (EmployeeId, &str),
    from: &str,
    text: &str,
    shared: &Shared<'_>,
    hop: u32,
) -> Result<Option<(RunId, Vec<EventNotice>)>, StorageError> {
    let Some(chat) = employees::main_chat(tx, employee)? else { return Ok(None) };
    let context = context_text(tx, store, group_id, message)?;
    let mut body = String::new();
    if !shared.files.is_empty() {
        body.push_str(&format!("{from} shared these files in the group chat: {}\n\n", shared.files.join(", ")));
    }
    body.push_str(text.trim());
    if !context.is_empty() {
        body.push_str(&format!("\n\n---\nEarlier in the group chat (oldest first):\n{context}"));
    }
    body.push_str(&format!("\n\n---\n{name}: your answer is posted in the group chat under your name. Keep it short."));
    let (update, posted) = crate::task::task_threads::post_note(tx, store, chat, crate::task::task_threads::Speaker::Update { kind: GROUP_KIND, from }, &body)?;
    // The images of the message come along, so the employee sees them (see `run::plan`).
    if !shared.images.is_empty() {
        conv::set_message_files(tx, update, shared.images)?;
    }
    let marking = GroupRun { conversation_id: group_id, message_id: MessageId::new(), hop };
    let (run, queued) = queue_run_marked(tx, meta, correlation, chat, update, Some(marking))?;
    Ok(Some((run, vec![posted, queued])))
}

/// The answer of a run the group chat woke, copied to the group as the
/// employee's message (`GroupRun::message_id`, the id it streamed under).
/// Nothing for another run, or for an answer that is empty and did not fail.
pub(crate) fn mirror_answer(tx: &Tx<'_>, store: &Store, run: RunId, conversation: ConversationId, text: &str, failed: Option<&str>, files: &[aurelius_domain::views::FileRef]) -> Result<Vec<EventNotice>, StorageError> {
    let Some(marking) = conv::group_run(tx, run)? else { return Ok(Vec::new()) };
    // Already copied (the run's job finished twice: never).
    if conv::message_ordinal(tx, marking.message_id)?.is_some() {
        return Ok(Vec::new());
    }
    let name = employees::employee_for_conversation(tx, conversation)?.map(|(name, _, _)| name).unwrap_or_else(|| "Aurelius".into());
    let text = match (text.trim(), failed) {
        ("", None) => return Ok(Vec::new()),
        ("", Some(code)) => format!("(Could not answer: {code}.)"),
        (text, _) => group::shortened(text, crate::task::task_threads::MAX_POST),
    };
    let notice = post_in_group(tx, store, marking.conversation_id, marking.message_id, &name, &text, serde_json::json!({ "run_id": run, "hop": marking.hop }))?;
    if !files.is_empty() {
        conv::set_message_files(tx, marking.message_id, files)?;
    }
    Ok(vec![notice])
}

impl CommandService {
    /// The user writes in the group chat: the message is stored, and the employees
    /// it names (or the CEO) each answer from their own main chat.
    pub(crate) fn send_group_message(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &group::SendGroupMessage) -> Result<Committed, AppError> {
        let text = cmd.validate().map_err(domain_error)?;
        let sources = crate::conversation::message_uploads::check(&cmd.files)?;
        let images = crate::conversation::message_images::prepare(&self.store, &cmd.images)?;
        let group_id = self.store.read(conv::group_conversation).map_err(storage_error)?.ok_or_else(|| AppError::new(ErrorCode::NotFound, "The group chat does not exist."))?;
        // Files go into the group chat's own folder before the commit; a failed or
        // replayed command removes them again.
        let folder = self.store.root().join("workspaces").join(group_id.to_string());
        let uploads = if sources.is_empty() {
            Vec::new()
        } else {
            std::fs::create_dir_all(&folder).map_err(|e| AppError::new(ErrorCode::Internal, format!("Could not make the group chat's folder: {e}")))?;
            crate::conversation::message_uploads::copy(&folder, &sources)?
        };
        let committed = self.commit_group_message(ctx, envelope, hash, group_id, text, images, &folder, uploads.clone());
        if !uploads.is_empty() && committed.as_ref().map_or(true, |c| c.receipt.replayed) {
            crate::conversation::message_uploads::remove(&folder, &uploads);
        }
        committed
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_group_message(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        group_id: ConversationId,
        text: String,
        images: Vec<crate::conversation::message_images::PreparedImage>,
        folder: &std::path::Path,
        uploads: Vec<MessageFileView>,
    ) -> Result<Committed, AppError> {
        let text_blob = self.store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let message_id = MessageId::new();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        let store = self.store.clone();
        let files: Vec<String> = uploads.iter().map(|u| format!("{} ({})", folder.join(&u.path).display(), crate::conversation::message_uploads::size_label(u.byte_length))).collect();
        self.commit(command_id, hash, move |tx| {
            let correlation = command_id.to_string();
            blobs::register(tx, &text_blob)?;
            let (event_id, n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: group_id.to_string(),
                event_type: "message.user_committed",
                actor: ActorKind::User,
                actor_id: Some(actor),
                correlation: correlation.clone(),
                payload: Some(text_blob.id.to_string()),
                metadata: meta.clone(),
                dedupe: format!("cmd:{command_id}:message"),
            })?;
            conv::insert_message(tx, NewMessage {
                id: message_id,
                conversation_id: group_id,
                role: MessageRole::User,
                content_blob_id: Some(text_blob.id.to_string()),
                source_event_id: event_id,
                status: MessageStatus::Complete,
                run_id: None,
                reply_to: None,
            })?;
            crate::conversation::message_images::keep(tx, message_id, &images)?;
            conv::set_message_uploads(tx, message_id, &uploads)?;
            let kept: Vec<aurelius_domain::views::FileRef> = images.iter().map(|i| i.file.clone()).collect();
            let shared = Shared { images: &kept, files: files.clone() };
            let mut notices = vec![n];
            let (mut to, mut run_ids) = (Vec::new(), Vec::new());
            for (employee, name) in addressees(tx, &text, None)? {
                if let Some((run, n)) = wake(tx, &store, &meta, &correlation, group_id, message_id, (employee, &name), OWNER, &text, &shared, 0)? {
                    to.push(employee);
                    run_ids.push(run);
                    notices.extend(n);
                }
            }
            Ok(Ok((CommandResult::GroupMessageSent { conversation_id: group_id, message_id, to, run_ids }, notices)))
        })
    }
}

/// `post_to_group_chat`: an employee writes in the group chat during a run. It
/// wakes only the colleagues it names (never itself), and only up to `MAX_HOPS`
/// hops after the user's message that started the exchange.
pub(crate) fn post_from_run(tx: &Tx<'_>, store: &Store, run: RunId, message: &str) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let Some(me) = crate::task::task_runs::run_owner(tx, run)?.and_then(|o| o.employee) else {
        return Ok((false, "Only an employee can post in the group chat.".into(), vec![]));
    };
    let text = message.trim();
    if text.is_empty() {
        return Ok((false, "Write a message to post.".into(), vec![]));
    }
    if text.chars().count() > POST_MAX {
        return Ok((false, format!("Too long for the group chat (at most {POST_MAX} characters); post a short summary."), vec![]));
    }
    let Some(group_id) = conv::group_conversation(tx)? else { return Ok((false, "The group chat does not exist.".into(), vec![])) };
    let Some(name) = employees::load_employee(tx, me)?.map(|e| e.name) else { return Ok((false, "Only an employee can post in the group chat.".into(), vec![])) };
    // The hop it would wake colleagues at: one more than its own run's, or 1 from elsewhere (a task).
    let hop = conv::group_run(tx, run)?.map_or(1, |g| g.hop + 1);
    let posted = MessageId::new();
    let mut notices = vec![post_in_group(tx, store, group_id, posted, &name, text, serde_json::json!({ "run_id": run, "hop": hop }))?];
    let named = addressees(tx, text, Some(me))?;
    if named.is_empty() {
        return Ok((true, "Posted in the group chat.".into(), notices));
    }
    let names: Vec<&str> = named.iter().map(|(_, n)| n.as_str()).collect();
    if hop > MAX_HOPS {
        return Ok((true, format!("Posted in the group chat. {} will not be woken: colleagues already answered each other {MAX_HOPS} times after the user's message; wait for the user.", names.join(", ")), notices));
    }
    let meta = serde_json::json!({ "run_id": run, "group_post": posted });
    let mut woke = Vec::new();
    for (employee, colleague) in &named {
        if let Some((_, n)) = wake(tx, store, &meta, &run.to_string(), group_id, posted, (*employee, colleague), &name, text, &Shared::default(), hop)? {
            woke.push(colleague.as_str());
            notices.extend(n);
        }
    }
    Ok((true, format!("Posted in the group chat. {} will answer there.", woke.join(", ")), notices))
}

/// `read_group_chat`: the latest `last` messages of the group chat (default and most 50), oldest first.
pub(crate) fn read_text(conn: &rusqlite::Connection, store: &Store, last: Option<u32>) -> Result<String, StorageError> {
    let Some(group_id) = conv::group_conversation(conn)? else { return Ok("There is no group chat.".into()) };
    let read = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let messages: Vec<aurelius_domain::views::MessageView> = conv::list_messages(conn, group_id, &read)?.into_iter().filter(|m| !m.text.trim().is_empty()).collect();
    if messages.is_empty() {
        return Ok("The group chat is empty.".into());
    }
    let last = last.unwrap_or(50).clamp(1, 50) as usize;
    let start = messages.len().saturating_sub(last);
    let lines: Vec<String> = messages[start..].iter().map(|m| format!("{}: {}", author_of(m), group::shortened(&m.text, 2_000))).collect();
    Ok(format!("The group chat, latest {} of {} messages (oldest first):\n\n{}", lines.len(), messages.len(), lines.join("\n\n")))
}
