//! The company row (migration 18: exactly one), its dispatcher, the board's six
//! columns and a project's own dispatcher.

use crate::{StorageError, now_ms};
use aurelius_domain::{
    BoardColumnId, EmployeeId, ProjectId, UserId,
    company::CompanyView,
    ticket::{BoardColumnView, ColumnKind},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

fn employee(text: Option<String>) -> Option<EmployeeId> {
    text.and_then(|t| EmployeeId::parse(&t).ok())
}

/// The company. Every database at schema 18 has exactly one.
pub fn get(conn: &Connection) -> Result<CompanyView, StorageError> {
    conn.query_row("SELECT id, name, dispatcher_employee_id, version FROM company", [], |r| {
        Ok(CompanyView { id: r.get(0)?, name: r.get(1)?, dispatcher_employee_id: employee(r.get(2)?), tester_employee_id: None, version: r.get::<_, i64>(3)? as u64 })
    })
    .optional()?
    .ok_or_else(|| StorageError::Rejected("there is no company".into()))
}

/// Change the name and/or the dispatcher (`Some(None)` clears it); returns the new version.
pub fn update(tx: &Transaction<'_>, name: Option<&str>, dispatcher: Option<Option<EmployeeId>>) -> Result<u64, StorageError> {
    let current = get(tx)?;
    let version = current.version + 1;
    let dispatcher = match dispatcher {
        Some(choice) => choice,
        None => current.dispatcher_employee_id,
    };
    tx.execute(
        "UPDATE company SET name = ?2, dispatcher_employee_id = ?3, version = ?4, updated_at_ms = ?5 WHERE id = ?1",
        params![current.id, name.unwrap_or(&current.name), dispatcher.map(|d| d.to_string()), version as i64, now_ms()],
    )?;
    Ok(version)
}

/// The "company login": the board member whose Claude login runs the
/// background jobs without a source (memory consolidation); none: they do not run.
pub fn background_login_user(conn: &Connection) -> Result<Option<UserId>, StorageError> {
    let user: Option<String> = conn.query_row("SELECT background_login_user FROM company", [], |r| r.get(0)).optional()?.flatten();
    Ok(user.and_then(|u| UserId::parse(&u).ok()))
}

/// Choose (or clear) the company login; a board member chooses their own.
/// Returns the company's new version.
pub fn set_background_login_user(tx: &Transaction<'_>, user: Option<UserId>) -> Result<u64, StorageError> {
    let current = get(tx)?;
    let version = current.version + 1;
    tx.execute(
        "UPDATE company SET background_login_user = ?2, version = ?3, updated_at_ms = ?4 WHERE id = ?1",
        params![current.id, user.map(|u| u.to_string()), version as i64, now_ms()],
    )?;
    Ok(version)
}

fn column_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<BoardColumnView> {
    let id: String = r.get(0)?;
    let kind: String = r.get(1)?;
    Ok(BoardColumnView {
        id: BoardColumnId::parse(&id).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?,
        kind: ColumnKind::parse(&kind).unwrap_or(ColumnKind::Backlog),
        name: r.get(2)?,
        position: r.get::<_, i64>(3)? as u32,
        version: r.get::<_, i64>(4)? as u64,
    })
}

const COLUMN_COLUMNS: &str = "id, kind, name, position, version";

/// The board's columns, left to right.
pub fn columns(conn: &Connection) -> Result<Vec<BoardColumnView>, StorageError> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMN_COLUMNS} FROM board_columns ORDER BY position, kind"))?;
    let rows = stmt.query_map([], column_row)?.collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn column(conn: &Connection, id: BoardColumnId) -> Result<Option<BoardColumnView>, StorageError> {
    Ok(conn.query_row(&format!("SELECT {COLUMN_COLUMNS} FROM board_columns WHERE id = ?1"), [id.to_string()], column_row).optional()?)
}

pub fn column_of_kind(conn: &Connection, kind: ColumnKind) -> Result<BoardColumnView, StorageError> {
    conn.query_row(&format!("SELECT {COLUMN_COLUMNS} FROM board_columns WHERE kind = ?1"), [kind.as_str()], column_row)
        .optional()?
        .ok_or_else(|| StorageError::Rejected(format!("the board has no {} column", kind.as_str())))
}

/// New names, and the list order as the new order (positions 0..n).
pub fn update_columns(tx: &Transaction<'_>, columns: &[(BoardColumnId, String)]) -> Result<(), StorageError> {
    for (position, (id, name)) in columns.iter().enumerate() {
        tx.execute(
            "UPDATE board_columns SET name = ?2, position = ?3, version = version + 1 WHERE id = ?1",
            params![id.to_string(), name, position as i64],
        )?;
    }
    Ok(())
}

/// A project's own dispatcher (`None`: the company's); returns the project's new version.
pub fn set_project_dispatcher(tx: &Transaction<'_>, project: ProjectId, dispatcher: Option<EmployeeId>) -> Result<u64, StorageError> {
    tx.execute(
        "UPDATE projects SET dispatcher_employee_id = ?2, version = version + 1, updated_at_ms = ?3 WHERE id = ?1",
        params![project.to_string(), dispatcher.map(|d| d.to_string()), now_ms()],
    )?;
    Ok(tx.query_row("SELECT version FROM projects WHERE id = ?1", [project.to_string()], |r| r.get::<_, i64>(0))? as u64)
}

pub fn project_dispatcher(conn: &Connection, project: ProjectId) -> Result<Option<EmployeeId>, StorageError> {
    Ok(employee(conn.query_row("SELECT dispatcher_employee_id FROM projects WHERE id = ?1", [project.to_string()], |r| r.get(0)).optional()?.flatten()))
}

/// An employee's status (`active`, `paused`, `archived`), none when it does not exist.
pub fn employee_status(conn: &Connection, id: EmployeeId) -> Result<Option<String>, StorageError> {
    Ok(conn.query_row("SELECT status FROM employees WHERE id = ?1", [id.to_string()], |r| r.get(0)).optional()?)
}

/// Who dispatches a ticket of `project`: the project's own dispatcher, else the
/// company's (impact analysis §3.7.1, `project ?? company`). That one only while
/// it is an active employee: a paused or archived dispatcher is none (B5), also
/// when the project has its own and the company's would be active.
pub fn dispatcher_for(conn: &Connection, project: ProjectId) -> Result<Option<EmployeeId>, StorageError> {
    let Some(choice) = project_dispatcher(conn, project)?.or(get(conn)?.dispatcher_employee_id) else { return Ok(None) };
    let active = conn
        .query_row("SELECT status = 'active' FROM employees WHERE id = ?1", [choice.to_string()], |r| r.get::<_, bool>(0))
        .optional()?
        .unwrap_or(false);
    Ok(active.then_some(choice))
}
