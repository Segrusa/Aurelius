//! Every project has its own folder on disk. Agents of that project may write
//! there (staged, by their mode); reports are saved there as files.

use crate::run::events::{Ev, emit};
use crate::{CommandService, Committed, domain_error, storage_error};
use aurelius_domain::{ActorKind, DomainError, GrantId, ProjectId, project::SetProjectFolder};
use aurelius_domain::{CommandEnvelope, CommandResult, EventNotice, AppError};
use aurelius_storage::files as store;
use std::path::{Path, PathBuf};

fn folder_error(message: impl Into<String>) -> AppError {
    domain_error(DomainError::File(message.into()))
}

/// Save a text file into `<project folder>/artifacts/`. Best effort: the
/// text always exists in Aurelius' own store; the copy is for the user.
pub fn write_artifact_file(folder: &str, name: &str, content: &str) {
    let dir = Path::new(folder).join("artifacts");
    let file = dir.join(format!("{}.md", aurelius_domain::project::folder_name(name)));
    if let Err(err) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, content)) {
        tracing::warn!(error = %err, path = %file.display(), "could not save artifact file in project folder");
    }
}

impl CommandService {
    /// Give every project without a folder its default one
    /// (`Documents\Aurelius\<name>`), created on disk and granted to that project.
    pub fn ensure_project_folders(&self) -> Result<Vec<EventNotice>, AppError> {
        let missing = self.store.read(store::projects_without_folder).map_err(storage_error)?;
        let Some(root) = self.projects_root() else { return Ok(Vec::new()) };
        let mut notices = Vec::new();
        for (project, name) in missing {
            let base = aurelius_domain::project::folder_name(&name);
            let mut candidate = root.join(&base);
            let mut n = 2;
            loop {
                let text = candidate.to_string_lossy().into_owned();
                let taken = self.store.read(|c| store::folder_taken(c, &text)).map_err(storage_error)?;
                if !taken {
                    break;
                }
                candidate = root.join(format!("{base} ({n})"));
                n += 1;
            }
            std::fs::create_dir_all(&candidate).map_err(|e| folder_error(format!("Could not create the project folder: {e}")))?;
            let folder = canonical(&candidate)?;
            notices.extend(self.store.write(move |tx| assign(tx, project, &folder, None)).map_err(storage_error)?);
        }
        Ok(notices)
    }

    /// Create the default folder for a new project; `None` without a projects root.
    pub(crate) fn new_project_folder(&self, name: &str) -> Result<Option<String>, AppError> {
        let Some(root) = self.projects_root() else { return Ok(None) };
        let base = aurelius_domain::project::folder_name(name);
        let mut candidate = root.join(&base);
        let mut n = 2;
        while candidate.exists() || self.store.read(|c| store::folder_taken(c, &candidate.to_string_lossy())).map_err(storage_error)? {
            candidate = root.join(format!("{base} ({n})"));
            n += 1;
        }
        std::fs::create_dir_all(&candidate).map_err(|e| folder_error(format!("Could not create the project folder: {e}")))?;
        canonical(&candidate).map(Some)
    }

    /// A folder the user chose: existing, not a forbidden root, canonical form.
    pub(crate) fn chosen_folder(&self, path: &str) -> Result<String, AppError> {
        let path = PathBuf::from(path.trim());
        if !path.is_dir() {
            return Err(folder_error("Choose an existing folder."));
        }
        let folder = canonical(&path)?;
        if aurelius_tools::files::forbidden_root(Path::new(&folder)) {
            return Err(folder_error("This folder can't be used (drive root, home folder or credential folder). Choose a more specific folder."));
        }
        Ok(folder)
    }

    pub(crate) fn set_employee_folder(&self, envelope: &CommandEnvelope, hash: String, cmd: &aurelius_domain::employee::SetEmployeeFolder) -> Result<Committed, AppError> {
        let folder = match cmd.path.as_deref().filter(|p| !p.trim().is_empty()) {
            Some(p) => Some(self.chosen_folder(p)?),
            None => None,
        };
        let employee = cmd.employee_id;
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            if aurelius_storage::employees::load_employee(tx, employee)?.is_none() {
                return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee))));
            }
            let (event_id, n) = emit(tx, Ev {
                aggregate_type: "employee_folder",
                aggregate_id: employee.to_string(),
                event_type: "employee.folder_set",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({ "folder": folder }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            store::set_employee_folder(tx, employee, folder.as_deref(), GrantId::new(), event_id)?;
            Ok(Ok((CommandResult::EmployeeFolderSet { employee_id: employee, folder: folder.clone() }, vec![n])))
        })
    }

    pub(crate) fn set_project_folder(&self, envelope: &CommandEnvelope, hash: String, cmd: &SetProjectFolder) -> Result<Committed, AppError> {
        let folder = self.chosen_folder(&cmd.path)?;
        let project = cmd.project_id;
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            if aurelius_storage::queries::load_project(tx, project)?.is_none() {
                return Ok(Err(domain_error(DomainError::ProjectNotFound(project))));
            }
            let other: Option<String> = tx
                .query_row("SELECT id FROM projects WHERE folder = ?1 AND id <> ?2", [folder.as_str(), &project.to_string()], |r| r.get(0))
                .ok();
            if other.is_some() {
                return Ok(Err(folder_error("Another project already uses this folder.")));
            }
            let notices = assign(tx, project, &folder, Some(command_id.to_string()))?;
            Ok(Ok((CommandResult::ProjectFolderSet { project_id: project, folder: folder.clone() }, notices)))
        })
    }
}

fn canonical(path: &Path) -> Result<String, AppError> {
    let resolved = std::fs::canonicalize(path).map_err(|e| folder_error(format!("Folder not found: {e}")))?;
    let text = resolved.to_string_lossy().into_owned();
    // Windows returns verbatim paths (\\?\C:\...); store the familiar form.
    Ok(text.strip_prefix(r"\\?\").map(str::to_owned).unwrap_or(text))
}

fn assign(tx: &aurelius_storage::Tx<'_>, project: ProjectId, folder: &str, command: Option<String>) -> Result<Vec<EventNotice>, aurelius_storage::StorageError> {
    let correlation = command.clone().unwrap_or_else(|| project.to_string());
    let (event_id, mut notice) = emit(tx, Ev {
        aggregate_type: "project_folder",
        aggregate_id: project.to_string(),
        event_type: "project.folder_set",
        actor: if command.is_some() { ActorKind::User } else { ActorKind::System },
        actor_id: None,
        correlation,
        payload: None,
        metadata: serde_json::json!({ "folder": folder }),
        dedupe: format!("project:{project}:folder:{}", aurelius_domain::EventId::new()),
    })?;
    notice.project_id = Some(project);
    store::set_project_folder(tx, project, folder, GrantId::new(), event_id)?;
    Ok(vec![notice])
}
