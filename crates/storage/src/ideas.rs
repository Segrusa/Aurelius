//! Ideas per project (see `aurelius_domain::idea`).

use crate::{StorageError, now_ms, tasks::TaskAttachments};
use aurelius_domain::{IdeaId, EmployeeId, ProjectId, TaskId, TicketId, idea::{IdeaKind, IdeaStatus}, task::TaskStatus};
use aurelius_domain::views::IdeaView;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub struct IdeaRow {
    pub project_id: ProjectId,
    pub title: String,
    pub note: Option<String>,
    pub status: IdeaStatus,
    pub kind: IdeaKind,
    pub task_id: Option<TaskId>,
    pub ticket_id: Option<TicketId>,
    pub attachments: Option<TaskAttachments>,
}

pub fn insert(tx: &Transaction<'_>, id: IdeaId, project: ProjectId, title: &str, note: Option<&str>, kind: IdeaKind, employee: Option<EmployeeId>) -> Result<(), StorageError> {
    let now = now_ms();
    tx.execute(
        "INSERT INTO ideas(id, project_id, title, note, status, kind, employee_id, version, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, 'open', ?7, ?5, 1, ?6, ?6)",
        params![id.to_string(), project.to_string(), title, note, employee.map(|m| m.to_string()), now, kind.as_str()],
    )?;
    Ok(())
}

pub fn load(conn: &Connection, id: IdeaId) -> Result<Option<IdeaRow>, StorageError> {
    Ok(conn
        .query_row("SELECT project_id, title, note, status, task_id, kind, attachments_json, ticket_id FROM ideas WHERE id = ?1", [id.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        })
        .optional()?
        .and_then(|(p, title, note, status, task, kind, attachments, ticket)| {
            Some(IdeaRow {
                project_id: ProjectId::parse(&p).ok()?,
                title,
                note,
                status: IdeaStatus::parse(&status)?,
                kind: IdeaKind::parse(&kind).unwrap_or_default(),
                task_id: task.and_then(|t| TaskId::parse(&t).ok()),
                ticket_id: ticket.and_then(|t| TicketId::parse(&t).ok()),
                attachments: attachments.and_then(|a| serde_json::from_str(&a).ok()),
            })
        }))
}

/// An idea's files (see `tasks::TaskAttachments`); none when it has none.
pub fn set_attachments(tx: &Transaction<'_>, id: IdeaId, attachments: Option<&TaskAttachments>) -> Result<(), StorageError> {
    let json = attachments.filter(|a| !a.files.is_empty()).map(serde_json::to_string).transpose()?;
    tx.execute("UPDATE ideas SET attachments_json = ?2, version = version + 1, updated_at_ms = ?3 WHERE id = ?1", params![id.to_string(), json, now_ms()])?;
    Ok(())
}

/// Delete an idea for good (its events stay in the timeline).
pub fn delete(tx: &Transaction<'_>, id: IdeaId) -> Result<(), StorageError> {
    tx.execute("DELETE FROM ideas WHERE id = ?1", [id.to_string()])?;
    Ok(())
}

pub fn update(tx: &Transaction<'_>, id: IdeaId, title: &str, note: Option<&str>, kind: IdeaKind) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE ideas SET title = ?2, note = ?3, kind = ?5, version = version + 1, updated_at_ms = ?4 WHERE id = ?1",
        params![id.to_string(), title, note, now_ms(), kind.as_str()],
    )?;
    Ok(())
}

pub fn set_status(tx: &Transaction<'_>, id: IdeaId, status: IdeaStatus, task: Option<TaskId>) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE ideas SET status = ?2, task_id = COALESCE(?3, task_id), version = version + 1, updated_at_ms = ?4 WHERE id = ?1",
        params![id.to_string(), status.as_str(), task.map(|t| t.to_string()), now_ms()],
    )?;
    Ok(())
}

/// The idea became this ticket: it is done and points at the ticket.
pub fn set_ticket(tx: &Transaction<'_>, id: IdeaId, ticket: TicketId) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE ideas SET status = 'done', ticket_id = ?2, version = version + 1, updated_at_ms = ?3 WHERE id = ?1",
        params![id.to_string(), ticket.to_string(), now_ms()],
    )?;
    Ok(())
}

/// A project's ideas: open ones first, then the rest, newest first.
pub fn list(conn: &Connection, project: ProjectId) -> Result<Vec<IdeaView>, StorageError> {
    query(conn, Some(project))
}

/// The ideas of every project, in the same order. Each idea still carries its own project.
pub fn list_all(conn: &Connection) -> Result<Vec<IdeaView>, StorageError> {
    query(conn, None)
}

fn query(conn: &Connection, project: Option<ProjectId>) -> Result<Vec<IdeaView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT i.id, i.title, i.note, i.status, m.name, i.task_id, t.status, i.created_at_ms, i.updated_at_ms, i.employee_id, i.kind, i.attachments_json, i.project_id, i.ticket_id, k.number
         FROM ideas i LEFT JOIN employees m ON m.id = i.employee_id LEFT JOIN tasks t ON t.id = i.task_id LEFT JOIN tickets k ON k.id = i.ticket_id
         WHERE (?1 IS NULL OR i.project_id = ?1) ORDER BY i.status = 'open' DESC, i.created_at_ms DESC LIMIT 1000",
    )?;
    let rows = stmt.query_map([project.map(|p| p.to_string())], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, i64>(7)?,
            r.get::<_, i64>(8)?,
            r.get::<_, Option<String>>(9)?,
            r.get::<_, String>(10)?,
            r.get::<_, Option<String>>(11)?,
            r.get::<_, String>(12)?,
            r.get::<_, Option<String>>(13)?,
            r.get::<_, Option<u32>>(14)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, title, note, status, employee, task, task_status, created, updated, employee_id, kind, attachments, project_id, ticket, ticket_number) = row?;
        let (Ok(id), Ok(project_id)) = (IdeaId::parse(&id), ProjectId::parse(&project_id)) else { continue };
        let mut attachments: Vec<aurelius_domain::views::MessageFileView> =
            attachments.and_then(|a| serde_json::from_str::<TaskAttachments>(&a).ok()).map(|a| a.files).unwrap_or_default();
        aurelius_domain::conversation::with_upload_ids(aurelius_domain::conversation::UploadOwner::Idea(id), &mut attachments);
        let employee_id = employee_id.and_then(|m| EmployeeId::parse(&m).ok());
        // An employee that no longer exists still did not make it the user's idea.
        let author = match (employee, employee_id) {
            (Some(name), _) => name,
            (None, Some(_)) => "A removed employee".into(),
            (None, None) => "You".into(),
        };
        out.push(IdeaView {
            id,
            project_id,
            title,
            note,
            status: IdeaStatus::parse(&status).unwrap_or(IdeaStatus::Open),
            kind: IdeaKind::parse(&kind).unwrap_or_default(),
            author,
            employee_id,
            task_id: task.and_then(|t| TaskId::parse(&t).ok()),
            task_status: task_status.and_then(|s| TaskStatus::parse(&s)),
            ticket_id: ticket.and_then(|t| TicketId::parse(&t).ok()),
            ticket_number,
            created_at_ms: created,
            updated_at_ms: updated,
            attachments,
        });
    }
    Ok(out)
}
