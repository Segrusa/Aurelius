//! Department projections: a named group of employees with a head and its own
//! memory scope (`scopes.kind = 'department'`, below the global one).

use crate::{StorageError, now_ms};
use aurelius_domain::views::{DepartmentStatus, DepartmentSummary};
use aurelius_domain::{DepartmentId, EmployeeId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub struct DepartmentRow {
    pub name: String,
    pub description: Option<String>,
    pub head_id: Option<EmployeeId>,
    pub status: DepartmentStatus,
    pub version: u64,
    pub scope_id: String,
}

pub fn insert(tx: &Transaction<'_>, id: DepartmentId, name: &str, description: Option<&str>, head: Option<EmployeeId>) -> Result<(), StorageError> {
    let global = crate::events::ensure_global_scope(tx)?;
    let scope = aurelius_domain::ScopeId::new().to_string();
    let now = now_ms();
    tx.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'department', ?2, ?3)", params![scope, global, id.to_string()])?;
    tx.execute(
        "INSERT INTO departments(id, scope_id, name, description, head_id, status, version, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, 'active', 1, ?6, ?6)",
        params![id.to_string(), scope, name, description, head.map(|h| h.to_string()), now],
    )?;
    Ok(())
}

pub fn load(conn: &Connection, id: DepartmentId) -> Result<Option<DepartmentRow>, StorageError> {
    Ok(conn
        .query_row("SELECT name, description, head_id, status, version, scope_id FROM departments WHERE id = ?1", [id.to_string()], |r| {
            Ok(DepartmentRow {
                name: r.get(0)?,
                description: r.get(1)?,
                head_id: r.get::<_, Option<String>>(2)?.and_then(|h| EmployeeId::parse(&h).ok()),
                status: DepartmentStatus::parse(&r.get::<_, String>(3)?).unwrap_or(DepartmentStatus::Archived),
                version: r.get::<_, i64>(4)? as u64,
                scope_id: r.get(5)?,
            })
        })
        .optional()?)
}

/// Another active department (not `except`) already has this name (trimmed, case-insensitive).
pub fn name_taken(conn: &Connection, name: &str, except: Option<DepartmentId>) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM departments WHERE status = 'active' AND lower(trim(name)) = lower(trim(?1)) AND (?2 IS NULL OR id <> ?2))",
        params![name, except.map(|d| d.to_string())],
        |r| r.get(0),
    )?)
}

/// The active department with this name (trimmed, case-insensitive).
pub fn by_name(conn: &Connection, name: &str) -> Result<Option<DepartmentId>, StorageError> {
    let id: Option<String> = conn
        .query_row("SELECT id FROM departments WHERE status = 'active' AND lower(trim(name)) = lower(trim(?1)) LIMIT 1", [name], |r| r.get(0))
        .optional()?;
    Ok(id.and_then(|i| DepartmentId::parse(&i).ok()))
}

/// Name, description and head as given; bumps the version.
pub fn update(tx: &Transaction<'_>, id: DepartmentId, name: &str, description: Option<&str>, head: Option<EmployeeId>) -> Result<u64, StorageError> {
    tx.execute(
        "UPDATE departments SET name = ?2, description = ?3, head_id = ?4, version = version + 1, updated_at_ms = ?5 WHERE id = ?1",
        params![id.to_string(), name, description, head.map(|h| h.to_string()), now_ms()],
    )?;
    Ok(tx.query_row("SELECT version FROM departments WHERE id = ?1", [id.to_string()], |r| r.get::<_, i64>(0))? as u64)
}

/// Archive: its employees leave it (they keep working), the head is cleared.
pub fn archive(tx: &Transaction<'_>, id: DepartmentId) -> Result<Vec<EmployeeId>, StorageError> {
    let members: Vec<EmployeeId> = {
        let mut stmt = tx.prepare("SELECT id FROM employees WHERE department_id = ?1")?;
        let rows = stmt.query_map([id.to_string()], |r| r.get::<_, String>(0))?;
        rows.filter_map(|r| r.ok()).filter_map(|m| EmployeeId::parse(&m).ok()).collect()
    };
    let now = now_ms();
    tx.execute("UPDATE employees SET department_id = NULL, version = version + 1, updated_at_ms = ?2 WHERE department_id = ?1", params![id.to_string(), now])?;
    tx.execute(
        "UPDATE departments SET status = 'archived', head_id = NULL, version = version + 1, updated_at_ms = ?2 WHERE id = ?1",
        params![id.to_string(), now],
    )?;
    Ok(members)
}

/// The departments `employee` heads; their head is cleared (it was archived).
pub fn clear_head(tx: &Transaction<'_>, employee: EmployeeId) -> Result<Vec<DepartmentId>, StorageError> {
    let headed: Vec<DepartmentId> = {
        let mut stmt = tx.prepare("SELECT id FROM departments WHERE head_id = ?1")?;
        let rows = stmt.query_map([employee.to_string()], |r| r.get::<_, String>(0))?;
        rows.filter_map(|r| r.ok()).filter_map(|d| DepartmentId::parse(&d).ok()).collect()
    };
    tx.execute("UPDATE departments SET head_id = NULL, version = version + 1, updated_at_ms = ?2 WHERE head_id = ?1", params![employee.to_string(), now_ms()])?;
    Ok(headed)
}

/// Departments by name; archived ones only with `archived`.
pub fn list(conn: &Connection, archived: bool) -> Result<Vec<DepartmentSummary>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT d.id, d.name, d.description, d.head_id, d.status, d.version, d.created_at_ms,
                (SELECT COUNT(*) FROM employees e WHERE e.department_id = d.id AND e.status <> 'archived'),
                (SELECT COUNT(*) FROM tasks t JOIN employees e ON e.id = t.employee_id
                  WHERE e.department_id = d.id AND t.status NOT IN ('succeeded','failed','cancelled'))
         FROM departments d WHERE ?1 OR d.status = 'active'
         ORDER BY lower(d.name), d.id",
    )?;
    let rows = stmt.query_map([archived], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
            r.get::<_, i64>(8)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, description, head, status, version, created, members, open) = row?;
        let Ok(id) = DepartmentId::parse(&id) else { continue };
        out.push(DepartmentSummary {
            id,
            name,
            description,
            head_id: head.and_then(|h| EmployeeId::parse(&h).ok()),
            status: DepartmentStatus::parse(&status).unwrap_or(DepartmentStatus::Archived),
            version: version as u64,
            created_at_ms: created,
            member_count: members as u32,
            open_tasks: open as u32,
        });
    }
    Ok(out)
}

/// The conversion report, when this server converted 2.x data.
pub fn conversion_report(conn: &Connection) -> Result<Option<aurelius_domain::views::ConversionReport>, StorageError> {
    let metadata: Option<String> = conn
        .query_row("SELECT metadata_json FROM events WHERE type = 'company.converted' ORDER BY seq DESC LIMIT 1", [], |r| r.get(0))
        .optional()?;
    Ok(metadata
        .and_then(|m| serde_json::from_str::<serde_json::Value>(&m).ok())
        .and_then(|m| serde_json::from_value(m["report"].clone()).ok()))
}
