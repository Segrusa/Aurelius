//! Project and employee use-cases. Employees are free-form and
//! company-wide: there is no default role, no mandatory employee per project,
//! and any active employee may work in any project. Names are unique in the
//! company (case-insensitive, among those not archived).

use crate::run::events::{Ev, emit};
use crate::run::queue::queue_run;
use crate::{CommandContext, CommandService, Committed, TxResult, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, ConversationId, DomainError, EventKind, EventNotice, EmployeeId, MessageId,
    conversation::{MessageRole, MessageStatus},
    employee::{self, LifecycleStatus, EmployeeConfig},
};
use aurelius_domain::{AppError, CommandEnvelope, CommandResult};
use aurelius_storage::{
    PreparedBlob, Store, Tx, blobs,
    conversations::{self as conv, NewMessage},
    employees,
};

/// An employee about to be created, prepared outside the transaction (its blobs are
/// on disk). Shared by `employee.create` and an approved colleague proposal, so
/// both make an employee exactly the same way.
pub(crate) struct NewEmployee {
    pub employee_id: EmployeeId,
    /// Its persistent chat.
    pub conversation_id: ConversationId,
    pub name: String,
    pub role: String,
    pub title: Option<String>,
    pub reports_to: Option<EmployeeId>,
    pub instructions_blob: Option<PreparedBlob>,
    pub payload: PreparedBlob,
    pub config: EmployeeConfig,
    pub workspace: Option<String>,
}

impl NewEmployee {
    /// `name`, `role`, `title` and `instructions` are already validated.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        store: &Store,
        name: &str,
        role: &str,
        title: Option<String>,
        reports_to: Option<EmployeeId>,
        instructions: Option<&str>,
        workspace: Option<String>,
    ) -> Result<Self, AppError> {
        let instructions_blob = match instructions {
            Some(text) => Some(store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?),
            None => None,
        };
        let employee_id = EmployeeId::new();
        let config = EmployeeConfig { identity: format!("{name}, {role}"), ..Default::default() };
        let payload = store
            .blobs()
            .put_json(&serde_json::json!({ "employee_id": employee_id, "name": name, "role": role, "title": title, "reports_to": reports_to, "config": config }))
            .map_err(storage_error)?;
        Ok(Self {
            employee_id,
            conversation_id: ConversationId::new(),
            name: name.to_owned(),
            role: role.to_owned(),
            title,
            reports_to,
            instructions_blob,
            payload,
            config,
            workspace,
        })
    }
}

/// Create the employee inside the caller's transaction and record `employee.created`.
/// Every check runs before the first write, so a refusal leaves nothing behind.
pub(crate) fn insert_new_employee(
    tx: &Tx<'_>,
    m: &NewEmployee,
    actor: ActorKind,
    actor_id: Option<String>,
    correlation: &str,
    meta: serde_json::Value,
    dedupe: String,
) -> TxResult<EventNotice> {
    if employees::name_taken(tx, &m.name, None)? {
        return Ok(Err(name_taken(&m.name)));
    }
    // The boss is a colleague who still works here, and the chart stays at most MAX_CHART_DEPTH deep.
    if let Some(boss) = m.reports_to {
        match employees::load_employee(tx, boss)? {
            Some(row) if row.status != LifecycleStatus::Archived => {}
            Some(_) => return Ok(Err(domain_error(DomainError::Org("An employee reports to a colleague who is not archived.".into())))),
            None => return Ok(Err(domain_error(DomainError::EmployeeNotFound(boss)))),
        }
        let mut entries: Vec<(EmployeeId, Option<EmployeeId>)> = employees::org_entries(tx)?;
        entries.push((m.employee_id, Some(boss)));
        if aurelius_domain::org::Org::new(entries).height() > aurelius_domain::org::MAX_CHART_DEPTH {
            return Ok(Err(domain_error(DomainError::Org(format!("The org chart may be at most {} levels deep.", aurelius_domain::org::MAX_CHART_DEPTH)))));
        }
    }
    blobs::register(tx, &m.payload)?;
    if let Some(b) = &m.instructions_blob {
        blobs::register(tx, b)?;
    }
    employees::insert_employee(
        tx,
        m.employee_id,
        &m.name,
        &m.role,
        m.title.as_deref(),
        m.reports_to,
        m.instructions_blob.as_ref().map(|b| b.id.to_string()).as_deref(),
        &serde_json::to_value(&m.config)?,
        m.conversation_id,
    )?;
    let (event_id, notice) = crate::org::append_event(
        tx,
        crate::org::OrgEvent {
            aggregate_type: "employee",
            aggregate_id: m.employee_id.to_string(),
            kind: EventKind::EmployeeCreated,
            project: None,
            actor,
            actor_id,
            correlation: correlation.to_owned(),
            payload: Some(m.payload.id.to_string()),
            metadata: meta,
            dedupe,
        },
    )?;
    if let Some(folder) = &m.workspace {
        aurelius_storage::files::set_employee_folder(tx, m.employee_id, Some(folder), aurelius_domain::GrantId::new(), event_id)?;
    }
    // Its instructions start at version 1.
    aurelius_storage::instruction_versions::record(tx, m.employee_id, aurelius_storage::instruction_versions::Source::Created)?;
    Ok(Ok(notice))
}

/// The refusal for a name another employee already has.
pub(crate) fn name_taken(name: &str) -> AppError {
    domain_error(DomainError::OutOfRange { field: "name".into(), detail: format!("An employee named {name} already exists; names are unique in the company") })
}

impl CommandService {
    pub(crate) fn create_employee(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &employee::CreateEmployee,
    ) -> Result<Committed, AppError> {
        let (name, role, instructions) = employee::validate_fields(&cmd.name, &cmd.role, cmd.instructions.as_deref()).map_err(domain_error)?;
        let title = employee::validate_title(cmd.title.as_deref()).map_err(domain_error)?;
        let description = employee::validate_description(cmd.description.as_deref()).map_err(domain_error)?;
        let workspace = match cmd.folder.as_deref().filter(|p| !p.trim().is_empty()) {
            Some(p) => Some(self.chosen_folder(p)?),
            None => None,
        };
        let new = NewEmployee::prepare(&self.store, &name, &role, title, cmd.reports_to, instructions.as_deref(), workspace)?;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        self.commit(command_id, hash, move |tx| {
            let notice = match insert_new_employee(tx, &new, ActorKind::User, Some(actor), &command_id.to_string(), meta, format!("cmd:{command_id}:0"))? {
                Ok(notice) => notice,
                Err(refused) => return Ok(Err(refused)),
            };
            employees::set_description(tx, new.employee_id, description.as_deref())?;
            Ok(Ok((CommandResult::EmployeeCreated { employee_id: new.employee_id, conversation_id: new.conversation_id }, vec![notice])))
        })
    }

    pub(crate) fn update_employee(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &employee::UpdateEmployee,
    ) -> Result<Committed, AppError> {
        let (name, role, instructions) = employee::validate_fields(&cmd.name, &cmd.role, cmd.instructions.as_deref()).map_err(domain_error)?;
        let blobs_store = self.store.blobs();
        let instructions_blob = match &instructions {
            Some(text) => Some(blobs_store.put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?),
            None => None,
        };
        // `None` leaves the title as it is; an empty text clears it.
        let new_title = cmd.title.as_deref().map(|t| employee::validate_title(Some(t))).transpose().map_err(domain_error)?;
        // `None` leaves the description as it is; an empty text clears it.
        let new_description = cmd.description.as_deref().map(|d| employee::validate_description(Some(d))).transpose().map_err(domain_error)?;
        let payload = blobs_store
            .put_json(&serde_json::json!({ "employee_id": cmd.employee_id, "name": name, "role": role, "title": new_title, "sequential_tasks": cmd.sequential_tasks, "description": new_description }))
            .map_err(storage_error)?;
        let employee_id = cmd.employee_id;
        let sequential = cmd.sequential_tasks;
        let expected = envelope.expected_version;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            let Some(current) = employees::load_employee(tx, employee_id)? else {
                return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee_id))));
            };
            if current.status == LifecycleStatus::Archived {
                return Ok(Err(domain_error(DomainError::EmployeeArchived)));
            }
            if let Some(expected) = expected.filter(|e| *e != current.version) {
                return Ok(Err(domain_error(DomainError::VersionConflict { expected, actual: current.version })));
            }
            if employees::name_taken(tx, &name, Some(employee_id))? {
                return Ok(Err(name_taken(&name)));
            }
            blobs::register(tx, &payload)?;
            if let Some(b) = &instructions_blob {
                blobs::register(tx, b)?;
            }
            let version = current.version + 1;
            let title = new_title.clone().unwrap_or(current.title.clone());
            employees::update_employee(tx, employee_id, &name, &role, title.as_deref(), instructions_blob.as_ref().map(|b| b.id.to_string()).as_deref(), version)?;
            if let Some(description) = &new_description {
                employees::set_description(tx, employee_id, description.as_deref())?;
            }
            // Changed instructions are a new version, and the employee's sessions start over with them.
            let (_, changed) = aurelius_storage::instruction_versions::record(tx, employee_id, aurelius_storage::instruction_versions::Source::Edited)?;
            if changed {
                aurelius_storage::conversation_sessions::expire_for_employee(tx, employee_id)?;
            }
            if let Some(on) = sequential {
                let was = aurelius_storage::tasks::sequential(tx, employee_id)?;
                employees::set_sequential_tasks(tx, employee_id, on)?;
                // Off again: the tasks waiting for their turn start; on: running ones go on.
                if was && !on {
                    crate::task::task_line::release_line(tx, employee_id)?;
                }
            }
            // New runs use the new config; running runs keep their snapshot.
            let (_, mut n) = emit(tx, Ev {
                aggregate_type: "employee",
                aggregate_id: employee_id.to_string(),
                event_type: "employee.config_changed",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: Some(payload.id.to_string()),
                metadata: meta,
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            n.project_id = None;
            Ok(Ok((CommandResult::EmployeeUpdated { employee_id, version }, vec![n])))
        })
    }

    pub(crate) fn set_employee_status(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &employee::SetEmployeeStatus,
    ) -> Result<Committed, AppError> {
        let (employee_id, status) = (cmd.employee_id, cmd.status);
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            let Some(current) = employees::load_employee(tx, employee_id)? else {
                return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee_id))));
            };
            if current.status == LifecycleStatus::Archived {
                return Ok(Err(domain_error(DomainError::EmployeeArchived)));
            }
            if let Err(e) = employee::check_transition(current.status, status) {
                return Ok(Err(domain_error(e)));
            }
            let event_type = match status {
                LifecycleStatus::Active => "employee.resumed",
                LifecycleStatus::Paused => "employee.paused",
                LifecycleStatus::Archived => "employee.archived",
            };
            // Archived: its reports move one level up and its open proposals are withdrawn,
            // while it is still on the chart.
            let mut notices = if status == LifecycleStatus::Archived {
                crate::org::on_employee_archived(tx, employee_id, command_id, &meta)?
            } else {
                Vec::new()
            };
            employees::set_employee_status(tx, employee_id, status, current.version + 1)?;
            if status == LifecycleStatus::Active {
                // Tasks parked because the employee was paused continue now.
                tx.execute(
                    "UPDATE jobs SET not_before_ms = ?2, last_error_code = NULL WHERE state = 'ready' AND last_error_code = 'paused'
                     AND id IN (SELECT id FROM tasks WHERE employee_id = ?1)",
                    (employee_id.to_string(), aurelius_storage::now_ms()),
                )?;
            }
            let (_, mut n) = emit(tx, Ev {
                aggregate_type: "employee",
                aggregate_id: employee_id.to_string(),
                event_type,
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: meta,
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            n.project_id = None;
            notices.push(n);
            Ok(Ok((CommandResult::EmployeeStatusChanged { employee_id, status }, notices)))
        })
    }

    pub(crate) fn set_project_status(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &employee::SetProjectStatus,
    ) -> Result<Committed, AppError> {
        let (project_id, status) = (cmd.project_id, cmd.status);
        let expected = envelope.expected_version;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            let Some(current) = crate::queries_load_project(tx, project_id)? else {
                return Ok(Err(domain_error(DomainError::ProjectNotFound(project_id))));
            };
            if let Some(expected) = expected.filter(|e| *e != current.version) {
                return Ok(Err(domain_error(DomainError::VersionConflict { expected, actual: current.version })));
            }
            let from = LifecycleStatus::parse(current.status.as_str()).unwrap_or(LifecycleStatus::Active);
            if let Err(e) = employee::check_transition(from, status) {
                return Ok(Err(domain_error(e)));
            }
            let event_type = match status {
                LifecycleStatus::Active => "project.updated",
                LifecycleStatus::Paused => "project.paused",
                LifecycleStatus::Archived => "project.archived",
            };
            employees::set_project_status(tx, project_id, status)?;
            let (_, mut n) = emit(tx, Ev {
                aggregate_type: "project",
                aggregate_id: project_id.to_string(),
                event_type,
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({ "status": status.as_str(), "ctx": meta }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            n.project_id = Some(project_id);
            Ok(Ok((CommandResult::ProjectStatusChanged { project_id, status }, vec![n])))
        })
    }

    /// A user message to an employee starts an employee run, unless the employee is paused.
    pub(crate) fn send_employee_message(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &employee::SendEmployeeMessage,
    ) -> Result<Committed, AppError> {
        let text = aurelius_domain::conversation::validate_employee_message(&cmd.text, &cmd.images, &cmd.files).map_err(domain_error)?;
        let sources = crate::conversation::message_uploads::check(&cmd.files)?;
        let images = crate::conversation::message_images::prepare(&self.store, &cmd.images)?;
        let thread = cmd.conversation_id;
        // Files go into the conversation's work folder before the commit; a
        // failed or replayed command removes them again.
        let uploaded = if sources.is_empty() {
            None
        } else {
            let folders = self.store.read(|c| conv::work_folders(c, thread)).map_err(storage_error)?;
            let folder = crate::conversation::message_uploads::work_folder(&folders)?;
            let uploads = crate::conversation::message_uploads::copy(&folder, &sources)?;
            Some((folder, uploads))
        };
        let uploads = uploaded.as_ref().map(|(_, u)| u.clone()).unwrap_or_default();
        let committed = self.commit_employee_message(ctx, envelope, hash, cmd, text, images, uploads);
        if let Some((folder, uploads)) = &uploaded {
            if committed.as_ref().map_or(true, |c| c.receipt.replayed) {
                crate::conversation::message_uploads::remove(folder, uploads);
            }
        }
        committed
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_employee_message(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &employee::SendEmployeeMessage,
        text: String,
        images: Vec<crate::conversation::message_images::PreparedImage>,
        uploads: Vec<aurelius_domain::views::MessageFileView>,
    ) -> Result<Committed, AppError> {
        let text_blob = self.store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let employee_id = cmd.employee_id;
        let conversation_id = cmd.conversation_id;
        let interrupt = cmd.interrupt;
        let message_id = MessageId::new();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        // An app writing to the employee's chat gets the replies to it (management channel).
        let app_key = ctx.app.as_ref().map(|a| a.key_id);
        let store = self.store.clone();
        self.commit(command_id, hash, move |tx| {
            let Some(current) = employees::load_employee(tx, employee_id)? else {
                return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee_id))));
            };
            match current.status {
                LifecycleStatus::Archived => return Ok(Err(domain_error(DomainError::EmployeeArchived))),
                LifecycleStatus::Paused => return Ok(Err(domain_error(DomainError::EmployeePaused))),
                LifecycleStatus::Active => {}
            }
            // The message goes to the conversation of one of its tasks, or to its persistent
            // chat; its older chats (from before protocol 3) are history.
            let task_conversation: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks t JOIN conversations c ON c.id = t.conversation_id
                   WHERE c.id = ?1 AND c.employee_id = ?2 AND c.kind = 'task' AND c.archived_at_ms IS NULL)",
                rusqlite::params![conversation_id.to_string(), employee_id.to_string()],
                |r| r.get(0),
            )?;
            let chat = current.conversation_id == conversation_id;
            if !task_conversation && !chat {
                if conv::is_older_employee_chat(tx, conversation_id)? {
                    return Ok(Err(domain_error(DomainError::OutOfRange {
                        field: "conversation_id".into(),
                        detail: "This is an older conversation; write in the employee's chat".into(),
                    })));
                }
                return Ok(Err(domain_error(DomainError::ConversationNotFound(conversation_id))));
            }
            let project = conv::owner_of(tx, conversation_id)?.1;
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
            n.project_id = project;
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
            if chat {
                if let Some(key) = app_key {
                    aurelius_storage::app_keys::note_thread(tx, key, conversation_id, employee_id)?;
                }
            }
            let mut notices = vec![n];
            if interrupt {
                notices.extend(crate::run::queue::interrupt_conversation(tx, &store, &meta, &command_id.to_string(), conversation_id)?);
            }
            let (run_id, run_notice) = queue_run(tx, &meta, &command_id.to_string(), conversation_id, message_id)?;
            notices.extend(run_notice);
            // A finished task whose conversation gets a run: its live run changed.
            notices.extend(crate::task::task_runs::live_run_notice(tx, conversation_id)?);
            Ok(Ok((CommandResult::EmployeeMessageSent { conversation_id, message_id, run_id }, notices)))
        })
    }
}
