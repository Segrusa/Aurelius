//! Ideas per project: noted by the user or by an employee while working; an
//! idea becomes a task for an employee, is marked done, or dropped.

use crate::run::events::{Ev, emit};
use crate::{CommandService, Committed, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, DomainError, IdeaId, EmployeeId, ProjectId, TaskId,
    idea::{self, AddIdea, DeleteIdea, IdeaKind, IdeaStatus, SetIdeaStatus, StartIdea, UpdateIdea},
    employee::LifecycleStatus,
    task::TaskStatus,
};
use aurelius_domain::{CommandEnvelope, CommandResult, EventNotice, AppError};
use aurelius_storage::{StorageError, Tx, blobs, ideas as store, employees, tasks, tasks::TaskAttachments};
use std::path::{Path, PathBuf};

fn idea_event(tx: &Tx<'_>, id: IdeaId, project: ProjectId, event_type: &'static str, actor: ActorKind, actor_id: Option<String>, correlation: String, metadata: serde_json::Value) -> Result<EventNotice, StorageError> {
    let (_, mut n) = emit(tx, Ev {
        aggregate_type: "idea",
        aggregate_id: id.to_string(),
        event_type,
        actor,
        actor_id,
        correlation: correlation.clone(),
        payload: None,
        metadata,
        dedupe: format!("idea:{id}:{event_type}:{correlation}"),
    })?;
    n.project_id = Some(project);
    Ok(n)
}

/// Note an idea in a project (by the user, or an employee: `employee`).
pub(crate) fn add(tx: &Tx<'_>, project: ProjectId, title: &str, note: Option<&str>, kind: IdeaKind, employee: Option<EmployeeId>, correlation: String) -> Result<(IdeaId, EventNotice), StorageError> {
    let id = IdeaId::new();
    store::insert(tx, id, project, title, note, kind, employee)?;
    let actor = if employee.is_some() { ActorKind::Employee } else { ActorKind::User };
    let n = idea_event(tx, id, project, "idea.added", actor, employee.map(|m| m.to_string()), correlation, serde_json::json!({ "title": title }))?;
    Ok((id, n))
}

impl CommandService {
    pub(crate) fn add_idea(&self, envelope: &CommandEnvelope, hash: String, cmd: &AddIdea) -> Result<Committed, AppError> {
        let (title, note) = idea::validate(&cmd.title, cmd.note.as_deref()).map_err(domain_error)?;
        idea::check_file_count(cmd.files.len()).map_err(domain_error)?;
        let (project, kind) = (cmd.project_id, cmd.kind);
        let attachments = self.attach_idea_files(project, &cmd.files, None)?;
        let stored = attachments.clone();
        let command_id = envelope.command_id;
        let committed = self.commit(command_id, hash, move |tx| {
            let Some((_, status)) = employees::project_scope(tx, project)? else { return Ok(Err(domain_error(DomainError::ProjectNotFound(project)))) };
            if status == LifecycleStatus::Archived {
                return Ok(Err(domain_error(DomainError::ProjectArchived)));
            }
            let (idea_id, n) = add(tx, project, &title, note.as_deref(), kind, None, command_id.to_string())?;
            if stored.is_some() {
                store::set_attachments(tx, idea_id, stored.as_ref())?;
            }
            Ok(Ok((CommandResult::IdeaAdded { idea_id }, vec![n])))
        });
        // Copies of a command that failed or was only replayed are not kept.
        self.discard_unused(&attachments, committed.as_ref().map_or(true, |c| c.receipt.replayed));
        committed
    }

    pub(crate) fn update_idea(&self, envelope: &CommandEnvelope, hash: String, cmd: &UpdateIdea) -> Result<Committed, AppError> {
        let (title, note) = idea::validate(&cmd.title, cmd.note.as_deref()).map_err(domain_error)?;
        let (id, kind) = (cmd.idea_id, cmd.kind);
        let existing = self.store.read(|c| store::load(c, id)).map_err(storage_error)?.ok_or_else(|| domain_error(DomainError::IdeaNotFound(id)))?;
        let (kept, removed): (Vec<_>, Vec<_>) =
            existing.attachments.as_ref().map(|a| a.files.clone()).unwrap_or_default().into_iter().partition(|f| !cmd.remove_files.contains(&f.path));
        idea::check_file_count(kept.len() + cmd.add_files.len()).map_err(domain_error)?;
        // New files go next to the ones already attached, so an idea keeps one folder.
        let folder = existing.attachments.as_ref().map(|a| PathBuf::from(&a.folder));
        let added = self.attach_idea_files(existing.project_id, &cmd.add_files, folder.as_deref())?;
        let changed = added.is_some() || !removed.is_empty();
        let attachments = match (&folder, added.clone()) {
            (_, Some(mut new)) => {
                new.files.splice(0..0, kept);
                Some(new)
            }
            (Some(folder), None) => Some(TaskAttachments { folder: folder.display().to_string(), files: kept }),
            (None, None) => None,
        };
        let before = existing.attachments;
        let command_id = envelope.command_id;
        let committed = self.commit(command_id, hash, move |tx| {
            let Some(row) = store::load(tx, id)? else { return Ok(Err(domain_error(DomainError::IdeaNotFound(id)))) };
            if changed && row.attachments != before {
                return Ok(Err(domain_error(DomainError::File("The idea's files changed meanwhile; reload it and try again.".into()))));
            }
            store::update(tx, id, &title, note.as_deref(), kind.unwrap_or(row.kind))?;
            if changed {
                store::set_attachments(tx, id, attachments.as_ref())?;
            }
            let n = idea_event(tx, id, row.project_id, "idea.updated", ActorKind::User, None, command_id.to_string(), serde_json::json!({ "title": title }))?;
            Ok(Ok((CommandResult::IdeaUpdated { idea_id: id }, vec![n])))
        });
        let unused = committed.as_ref().map_or(true, |c| c.receipt.replayed);
        self.discard_unused(&added, unused);
        // The idea owns its copies (a task it became has its own), so dropped files go.
        if let (Some(folder), false) = (&folder, unused) {
            crate::conversation::message_uploads::remove(folder, &removed);
        }
        committed
    }

    /// Delete an idea and its files. A task it became keeps its own copies.
    pub(crate) fn delete_idea(&self, envelope: &CommandEnvelope, hash: String, cmd: &DeleteIdea) -> Result<Committed, AppError> {
        let id = cmd.idea_id;
        let files = self.store.read(|c| store::load(c, id)).map_err(storage_error)?.and_then(|r| r.attachments);
        let command_id = envelope.command_id;
        let committed = self.commit(command_id, hash, move |tx| {
            let Some(row) = store::load(tx, id)? else { return Ok(Err(domain_error(DomainError::IdeaNotFound(id)))) };
            store::delete(tx, id)?;
            let n = idea_event(tx, id, row.project_id, "idea.deleted", ActorKind::User, None, command_id.to_string(), serde_json::json!({ "title": row.title }))?;
            Ok(Ok((CommandResult::IdeaDeleted { idea_id: id }, vec![n])))
        });
        self.discard_unused(&files, committed.as_ref().is_ok_and(|c| !c.receipt.replayed));
        committed
    }

    /// Check and copy an idea's files into the project's uploads area (or into
    /// `into`, the folder the idea's files are already in).
    fn attach_idea_files(&self, project: ProjectId, files: &[String], into: Option<&Path>) -> Result<Option<TaskAttachments>, AppError> {
        if files.is_empty() {
            return Ok(None);
        }
        let folder = match into {
            Some(folder) => folder.to_path_buf(),
            None => {
                let folders = self.store.read(|c| tasks::work_folders(c, project, None)).map_err(storage_error)?;
                folders.iter().map(PathBuf::from).find(|d| d.is_dir()).ok_or_else(|| {
                    domain_error(DomainError::File("This project has no work folder on the server to keep files in; give the project a folder first.".into()))
                })?
            }
        };
        self.attach_files(project, None, files, Some(&folder))
    }

    pub(crate) fn set_idea_status(&self, envelope: &CommandEnvelope, hash: String, cmd: &SetIdeaStatus) -> Result<Committed, AppError> {
        let (id, status) = (cmd.idea_id, cmd.status);
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            let Some(row) = store::load(tx, id)? else { return Ok(Err(domain_error(DomainError::IdeaNotFound(id)))) };
            if row.status == status {
                return Ok(Ok((CommandResult::IdeaUpdated { idea_id: id }, vec![])));
            }
            store::set_status(tx, id, status, None)?;
            let n = idea_event(tx, id, row.project_id, "idea.status_changed", ActorKind::User, None, command_id.to_string(), serde_json::json!({ "status": status, "title": row.title }))?;
            Ok(Ok((CommandResult::IdeaUpdated { idea_id: id }, vec![n])))
        })
    }

    /// The idea becomes a task for the employee: its title, its note as the goal.
    pub(crate) fn start_idea(&self, ctx: &crate::CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &StartIdea) -> Result<Committed, AppError> {
        let login = ctx.login_user();
        let (id, employee) = (cmd.idea_id, cmd.employee_id);
        let run = cmd.run.validated().map_err(domain_error)?;
        let row = self.store.read(|c| store::load(c, id)).map_err(storage_error)?.ok_or_else(|| domain_error(DomainError::IdeaNotFound(id)))?;
        if row.task_id.is_some() {
            return Err(domain_error(DomainError::Memory("This idea already became a task.".into())));
        }
        let goal = row.note.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| row.title.clone());
        let goal_blob = self.store.blobs().put(goal.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let criteria_blob = self.store.blobs().put_json(&Vec::<String>::new()).map_err(storage_error)?;
        // The task gets its own copies of the idea's files, in its work folder,
        // as if the user had attached them to the task (files gone meanwhile are skipped).
        let sources: Vec<String> = row
            .attachments
            .as_ref()
            .map(|a| a.files.iter().map(|f| Path::new(&a.folder).join(&f.path)).filter(|p| p.is_file()).map(|p| p.display().to_string()).collect())
            .unwrap_or_default();
        let attachments = self.attach_files(row.project_id, Some(employee), &sources, None)?;
        let stored = attachments.clone();
        let command_id = envelope.command_id;
        let committed = self.commit(command_id, hash, move |tx| {
            match employees::load_employee(tx, employee)? {
                Some(m) if m.status != LifecycleStatus::Archived => {}
                Some(_) => return Ok(Err(domain_error(DomainError::EmployeeArchived))),
                None => return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee)))),
            }
            if let Some(c) = run.connection_id {
                if aurelius_storage::connections::load(tx, c)?.is_none() {
                    return Ok(Err(domain_error(DomainError::ProviderNotFound(c))));
                }
            }
            if let Err(refused) = crate::task::variants::check_new(tx, &run, row.project_id, Some(employee))? {
                return Ok(Err(refused));
            }
            blobs::register(tx, &goal_blob)?;
            blobs::register(tx, &criteria_blob)?;
            let task_id = TaskId::new();
            tasks::insert_task(tx, task_id, row.project_id, Some(employee), &row.title, &goal_blob.id.to_string(), &criteria_blob.id.to_string(), TaskStatus::Queued, None)?;
            tasks::set_task_run(tx, task_id, &run)?;
            tasks::set_login_user(tx, task_id, login.as_deref())?;
            if stored.is_some() {
                tasks::set_attachments(tx, task_id, stored.as_ref())?;
            }
            let (event_id, mut created) = emit(tx, Ev {
                aggregate_type: "task",
                aggregate_id: task_id.to_string(),
                event_type: "task.created",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: Some(goal_blob.id.to_string()),
                metadata: serde_json::json!({ "title": row.title, "employee_id": employee, "idea_id": id }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            created.project_id = Some(row.project_id);
            let mut notices = vec![created];
            notices.push(crate::task::tasks::task_event(tx, task_id, row.project_id, "task.queued", serde_json::json!({}))?);
            aurelius_storage::events::insert_job(tx, &task_id.to_string(), crate::task::tasks::TASK_JOB_KIND, event_id, &format!("task:{task_id}"), &serde_json::json!({ "task_id": task_id }))?;
            store::set_status(tx, id, IdeaStatus::Done, Some(task_id))?;
            notices.push(idea_event(tx, id, row.project_id, "idea.started", ActorKind::User, None, command_id.to_string(), serde_json::json!({ "task_id": task_id, "title": row.title }))?);
            Ok(Ok((CommandResult::IdeaStarted { idea_id: id, task_id }, notices)))
        });
        self.discard_unused(&attachments, committed.as_ref().map_or(true, |c| c.receipt.replayed));
        committed
    }

    /// The ideas of every project.
    pub fn all_ideas(&self) -> Result<aurelius_domain::views::AllIdeasSnapshot, AppError> {
        self.store.read(|c| Ok(aurelius_domain::views::AllIdeasSnapshot { ideas: store::list_all(c)? })).map_err(storage_error)
    }

    /// A project's ideas.
    pub fn ideas(&self, project: ProjectId) -> Result<aurelius_domain::views::IdeasSnapshot, AppError> {
        self.store.read(|c| Ok(aurelius_domain::views::IdeasSnapshot { project_id: project, ideas: store::list(c, project)? })).map_err(storage_error)
    }
}
