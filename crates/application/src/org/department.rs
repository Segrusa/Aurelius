//! Departments: create, change (name, description, head) and archive. A head
//! joins its department; archiving keeps its employees working without one.

use crate::org::{OrgEvent, append_event};
use crate::{CommandContext, CommandService, Committed, domain_error};
use aurelius_domain::department::{self, ArchiveDepartment, CreateDepartment, UpdateDepartment};
use aurelius_domain::views::DepartmentStatus;
use aurelius_domain::{ActorKind, AppError, CommandEnvelope, CommandId, CommandResult, DepartmentId, DomainError, EmployeeId, EventKind, EventNotice, employee::LifecycleStatus};
use aurelius_storage::{StorageError, Tx, departments, employees};

impl CommandService {
    pub(crate) fn create_department(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &CreateDepartment) -> Result<Committed, AppError> {
        let (name, description) = department::validate_fields(&cmd.name, cmd.description.as_deref()).map_err(domain_error)?;
        let head = cmd.head_id;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            if departments::name_taken(tx, &name, None)? {
                return Ok(Err(name_taken(&name)));
            }
            if let Some(head) = head {
                if let Err(refused) = working_employee(tx, head)? {
                    return Ok(Err(refused));
                }
            }
            let id = DepartmentId::new();
            departments::insert(tx, id, &name, description.as_deref(), head)?;
            let mut notices = vec![event(tx, id, EventKind::DepartmentCreated, command_id, "0", serde_json::json!({ "name": name, "head_id": head, "ctx": meta }))?];
            if let Some(head) = head {
                notices.extend(head_joins(tx, head, id, command_id, &meta)?);
            }
            Ok(Ok((CommandResult::DepartmentCreated { department_id: id }, notices)))
        })
    }

    pub(crate) fn update_department(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &UpdateDepartment) -> Result<Committed, AppError> {
        let (name, description) = department::validate_fields(&cmd.name, cmd.description.as_deref()).map_err(domain_error)?;
        let (id, head) = (cmd.department_id, cmd.head_id);
        let expected = envelope.expected_version;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            let Some(current) = departments::load(tx, id)? else {
                return Ok(Err(domain_error(DomainError::DepartmentNotFound(id))));
            };
            if current.status == DepartmentStatus::Archived {
                return Ok(Err(domain_error(DomainError::Org(format!("The department {} is archived.", current.name)))));
            }
            if let Some(expected) = expected.filter(|e| *e != current.version) {
                return Ok(Err(domain_error(DomainError::VersionConflict { expected, actual: current.version })));
            }
            if departments::name_taken(tx, &name, Some(id))? {
                return Ok(Err(name_taken(&name)));
            }
            if let Some(head) = head {
                if let Err(refused) = working_employee(tx, head)? {
                    return Ok(Err(refused));
                }
            }
            let version = departments::update(tx, id, &name, description.as_deref(), head)?;
            let mut notices = vec![event(tx, id, EventKind::DepartmentUpdated, command_id, "0", serde_json::json!({ "name": name, "head_id": head, "ctx": meta }))?];
            if let Some(head) = head.filter(|h| Some(*h) != current.head_id) {
                notices.extend(head_joins(tx, head, id, command_id, &meta)?);
            }
            Ok(Ok((CommandResult::DepartmentUpdated { department_id: id, version }, notices)))
        })
    }

    pub(crate) fn archive_department(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &ArchiveDepartment) -> Result<Committed, AppError> {
        let id = cmd.department_id;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            let Some(current) = departments::load(tx, id)? else {
                return Ok(Err(domain_error(DomainError::DepartmentNotFound(id))));
            };
            let result = CommandResult::DepartmentArchived { department_id: id };
            if current.status == DepartmentStatus::Archived {
                return Ok(Ok((result, vec![])));
            }
            let members = departments::archive(tx, id)?;
            let notices = vec![event(tx, id, EventKind::DepartmentArchived, command_id, "0", serde_json::json!({ "name": current.name, "members": members, "ctx": meta }))?];
            Ok(Ok((result, notices)))
        })
    }
}

fn name_taken(name: &str) -> AppError {
    domain_error(DomainError::OutOfRange { field: "name".into(), detail: format!("A department named {name} already exists") })
}

/// The employee exists and is not archived.
fn working_employee(tx: &Tx<'_>, employee: EmployeeId) -> Result<Result<(), AppError>, StorageError> {
    Ok(match employees::load_employee(tx, employee)? {
        None => Err(domain_error(DomainError::EmployeeNotFound(employee))),
        Some(row) if row.status == LifecycleStatus::Archived => Err(domain_error(DomainError::EmployeeArchived)),
        Some(_) => Ok(()),
    })
}

/// A new head joins the department it leads (if it was elsewhere).
fn head_joins(tx: &Tx<'_>, head: EmployeeId, department: DepartmentId, command_id: CommandId, meta: &serde_json::Value) -> Result<Vec<EventNotice>, StorageError> {
    let from = employees::department_of(tx, head)?;
    if from == Some(department) {
        return Ok(vec![]);
    }
    crate::org::employee::set_department(tx, head, from, Some(department), command_id, meta)
}

fn event(tx: &Tx<'_>, id: DepartmentId, kind: EventKind, command_id: CommandId, part: &str, metadata: serde_json::Value) -> Result<EventNotice, StorageError> {
    let (_, n) = append_event(
        tx,
        OrgEvent {
            aggregate_type: "department",
            aggregate_id: id.to_string(),
            kind,
            project: None,
            actor: ActorKind::User,
            actor_id: None,
            correlation: command_id.to_string(),
            payload: None,
            metadata,
            dedupe: format!("cmd:{command_id}:{part}"),
        },
    )?;
    Ok(n)
}
