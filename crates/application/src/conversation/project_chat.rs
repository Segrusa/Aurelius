//! Project conversations. The user talks with an assistant of the project
//! instead of with an employee: it works in the project folder, sees the
//! employees, tasks, ideas and memories of the project and gives work to a
//! employee with `delegate_task`; the result comes back in the conversation.
//! Employees have tasks only.

use crate::run::events::{Ev, emit};
use crate::run::queue::queue_run;
use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, ConversationId, DomainError, MessageId, ProjectId,
    conversation::{MessageRole, MessageStatus},
    employee::LifecycleStatus,
    project_conversation::{SendProjectMessage, StartProjectConversation},
};
use aurelius_domain::{AppError, CommandEnvelope, CommandResult};
use aurelius_storage::{
    blobs,
    conversations::{self as conv, NewMessage},
    employees,
};
use rusqlite::OptionalExtension;

/// How the assistant of a project conversation introduces itself and its tools.
pub const ASSISTANT_GUIDANCE: &str = "You talk with the user about the project as a whole. The company's employees do the work, each as tasks of their own in this project: when work should be done, give it to the employee who suits it best with delegate_task (a title, a goal and optionally criteria); the employee's result comes back in this conversation as a [Task …] message and you tell the user. Do small things (reading, answering, looking something up) yourself. You cannot hand over a task. Something worth doing later that is not part of the current work: note it with add_idea. Decisions, assumptions and project knowledge worth keeping: remember (it goes to the project's memory), and look up earlier decisions with recall before you decide something.";

impl CommandService {
    pub(crate) fn start_project_conversation(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &StartProjectConversation,
    ) -> Result<Committed, AppError> {
        let (project_id, start) = (cmd.project_id, aurelius_domain::task::RunSettings::from(cmd.start.clone()));
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            if let Err(err) = open_project(tx, project_id)? {
                return Ok(Err(err));
            }
            let conversation_id = ConversationId::new();
            let notices = create(tx, &meta, command_id, project_id, conversation_id, "", &start)?;
            Ok(Ok((CommandResult::ProjectConversationStarted { project_id, conversation_id }, notices)))
        })
    }

    pub(crate) fn send_project_message(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &SendProjectMessage,
    ) -> Result<Committed, AppError> {
        let text = aurelius_domain::conversation::validate_employee_message(&cmd.text, &cmd.images, &cmd.files).map_err(domain_error)?;
        let sources = crate::conversation::message_uploads::check(&cmd.files)?;
        let images = crate::conversation::message_images::prepare(&self.store, &cmd.images)?;
        // Files go into the project folder before the commit; a failed or
        // replayed command removes them again.
        let uploaded = if sources.is_empty() {
            None
        } else {
            let project = cmd.project_id;
            let folders = self
                .store
                .read(|c| {
                    let folder: Option<Option<String>> = c.query_row("SELECT folder FROM projects WHERE id = ?1", [project.to_string()], |r| r.get(0)).optional()?;
                    Ok(folder.flatten().into_iter().collect::<Vec<String>>())
                })
                .map_err(storage_error)?;
            let folder = crate::conversation::message_uploads::work_folder(&folders)?;
            let uploads = crate::conversation::message_uploads::copy(&folder, &sources)?;
            Some((folder, uploads))
        };
        let uploads = uploaded.as_ref().map(|(_, u)| u.clone()).unwrap_or_default();
        let committed = self.commit_project_message(ctx, envelope, hash, cmd, text, images, uploads);
        if let Some((folder, uploads)) = &uploaded {
            if committed.as_ref().map_or(true, |c| c.receipt.replayed) {
                crate::conversation::message_uploads::remove(folder, uploads);
            }
        }
        committed
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_project_message(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &SendProjectMessage,
        text: String,
        images: Vec<crate::conversation::message_images::PreparedImage>,
        uploads: Vec<aurelius_domain::views::MessageFileView>,
    ) -> Result<Committed, AppError> {
        let text_blob = self.store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let (project_id, thread, interrupt, start) = (cmd.project_id, cmd.conversation_id, cmd.interrupt, aurelius_domain::task::RunSettings::from(cmd.start.clone()));
        // A conversation that starts with files only is named after the first.
        let title_source = match (text.trim().is_empty(), uploads.first()) {
            (true, Some(first)) => first.name.clone(),
            _ => text.clone(),
        };
        let message_id = MessageId::new();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        let store = self.store.clone();
        self.commit(command_id, hash, move |tx| {
            if let Err(err) = open_project(tx, project_id)? {
                return Ok(Err(err));
            }
            let title = aurelius_domain::conversation::derive_title(&title_source);
            let mut notices = Vec::new();
            let conversation_id = match thread {
                None => {
                    let id = ConversationId::new();
                    notices.extend(create(tx, &meta, command_id, project_id, id, &title, &start)?);
                    id
                }
                Some(id) => {
                    if conv::project_of_conversation(tx, id)? != Some(project_id) {
                        return Ok(Err(domain_error(DomainError::ConversationNotFound(id))));
                    }
                    // A conversation started empty is named after its first message.
                    tx.execute("UPDATE conversations SET title = ?2 WHERE id = ?1 AND title = ''", rusqlite::params![id.to_string(), title])?;
                    id
                }
            };
            blobs::register(tx, &text_blob)?;
            let (event_id, mut n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: conversation_id.to_string(),
                event_type: "message.user_committed",
                actor: ActorKind::User,
                actor_id: Some(actor),
                correlation: command_id.to_string(),
                payload: Some(text_blob.id.to_string()),
                metadata: meta.clone(),
                dedupe: format!("cmd:{command_id}:message"),
            })?;
            n.project_id = Some(project_id);
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
            conv::set_message_uploads(tx, message_id, &uploads)?;
            if interrupt && thread.is_some() {
                notices.extend(crate::run::queue::interrupt_conversation(tx, &store, &meta, &command_id.to_string(), conversation_id)?);
            }
            let (run_id, run_notice) = queue_run(tx, &meta, &command_id.to_string(), conversation_id, message_id)?;
            notices.push(run_notice);
            Ok(Ok((CommandResult::ProjectMessageSent { project_id, conversation_id, message_id, run_id }, notices)))
        })
    }
}

/// The project exists and is not archived.
fn open_project(tx: &aurelius_storage::Tx<'_>, project: ProjectId) -> Result<Result<(), AppError>, aurelius_storage::StorageError> {
    Ok(match employees::project_scope(tx, project)? {
        None => Err(domain_error(DomainError::ProjectNotFound(project))),
        Some((_, LifecycleStatus::Archived)) => Err(domain_error(DomainError::ProjectArchived)),
        Some(_) => Ok(()),
    })
}

/// Make a project conversation with its starting settings; the notices.
fn create(
    tx: &aurelius_storage::Tx<'_>,
    meta: &serde_json::Value,
    command_id: aurelius_domain::CommandId,
    project: ProjectId,
    conversation: ConversationId,
    title: &str,
    start: &aurelius_domain::task::RunSettings,
) -> Result<Vec<aurelius_domain::EventNotice>, aurelius_storage::StorageError> {
    conv::insert_project_conversation(tx, conversation, project, title)?;
    if let Some(mode) = start.mode {
        conv::set_agent_mode(tx, conversation, mode)?;
    }
    if start.browser_allowed {
        conv::set_browser_allowed(tx, conversation, true)?;
    }
    if start.screen_allowed {
        conv::set_screen_allowed(tx, conversation, true)?;
    }
    let (_, mut n) = emit(tx, Ev {
        aggregate_type: "conversation",
        aggregate_id: conversation.to_string(),
        event_type: "conversation.started",
        actor: ActorKind::User,
        actor_id: None,
        correlation: command_id.to_string(),
        payload: None,
        metadata: serde_json::json!({ "project_id": project, "project_conversation": true, "ctx": meta }),
        dedupe: format!("cmd:{command_id}:conversation"),
    })?;
    n.project_id = Some(project);
    let mut notices = vec![n];
    if !start.is_empty() {
        let mut settings = serde_json::json!({ "ctx": meta });
        if let Some(mode) = start.mode {
            settings["agent_mode"] = mode.as_str().into();
        }
        if start.browser_allowed {
            settings["browser_allowed"] = true.into();
        }
        if start.screen_allowed {
            settings["screen_allowed"] = true.into();
        }
        let (_, mut n) = emit(tx, Ev {
            aggregate_type: "conversation",
            aggregate_id: conversation.to_string(),
            event_type: "conversation.settings_changed",
            actor: ActorKind::User,
            actor_id: None,
            correlation: command_id.to_string(),
            payload: None,
            metadata: settings,
            dedupe: format!("cmd:{command_id}:settings"),
        })?;
        n.project_id = Some(project);
        notices.push(n);
    }
    Ok(notices)
}

/// The system text of a project conversation: who the assistant is and the
/// employees it can give work to. The run adds the project's tasks at the end.
pub(crate) fn assistant_system(
    conn: &rusqlite::Connection,
    project: ProjectId,
    read_blob: &dyn Fn(&str) -> Option<String>,
    tools: bool,
) -> Result<String, aurelius_storage::StorageError> {
    let name: Option<String> = conn.query_row("SELECT name FROM projects WHERE id = ?1", [project.to_string()], |r| r.get(0)).optional()?;
    let mut text = format!("You are the assistant of the project “{}”.", name.unwrap_or_default());
    text.push_str(&format!("\n\n{}", ASSISTANT_GUIDANCE));
    text.push_str(&format!("\n\n{}", crate::run::plan::QUICK_REPLY_GUIDANCE));
    // Every employee of the company, by department; the management team (no boss) marked.
    let colleagues = crate::task::tasks::colleagues(conn, read_blob)?;
    let employees: Vec<&crate::task::tasks::Colleague> = colleagues.iter().collect();
    if employees.is_empty() {
        text.push_str("\n\nThe company has no employees yet, so there is nobody to delegate to; tell the user if work needs one.");
    } else {
        text.push_str(&format!("\n\nThe company's employees (give work to them with delegate_task):\n{}", crate::task::tasks::colleague_lines(&employees)));
    }
    if tools {
        text.push_str(&format!("\n\n{}", crate::conversation::overview::TOOLS_GUIDANCE));
    }
    Ok(text)
}
