//! Management chooses its own task mode (feature `management_chooses_mode`): the
//! server setting, and who counts as management.

use crate::StorageError;
use aurelius_domain::EmployeeId;
use rusqlite::{Connection, Transaction};

pub const SETTING_KEY: &str = "management_chooses_mode";

/// Whether management chooses its own mode; on unless the user turned it off.
pub fn enabled(conn: &Connection) -> Result<bool, StorageError> {
    Ok(crate::settings::get(conn, SETTING_KEY)?.and_then(|j| serde_json::from_str::<bool>(&j).ok()).unwrap_or(true))
}

pub fn set_enabled(tx: &Transaction<'_>, enabled: bool, now: i64) -> Result<(), StorageError> {
    crate::settings::put(tx, SETTING_KEY, if enabled { "true" } else { "false" }, now)
}

/// Whether `employee` is part of the management now: the CEO or a direct report of the CEO.
pub fn is_management(conn: &Connection, employee: EmployeeId) -> Result<bool, StorageError> {
    Ok(management(conn)?.iter().any(|m| m.id == employee))
}

/// Management that chooses its own mode: the employee is management and the setting is on.
pub fn chooses_own_mode(conn: &Connection, employee: EmployeeId) -> Result<bool, StorageError> {
    Ok(enabled(conn)? && is_management(conn, employee)?)
}

/// One member of the management: the CEO or a direct report of the CEO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagementMember {
    pub id: aurelius_domain::EmployeeId,
    pub name: String,
    pub role: String,
    pub title: Option<String>,
    pub is_ceo: bool,
}

/// The management of the company now: the CEO (the root of the chart whose role or title
/// names the CEO, else the oldest root) first, then the CEO's direct reports. Archived
/// employees are not on the chart. Empty when the company has no employees.
pub fn management(conn: &Connection) -> Result<Vec<ManagementMember>, StorageError> {
    use aurelius_domain::EmployeeId;
    let org = crate::employees::org(conn)?;
    let mut stmt = conn.prepare("SELECT id, name, role, title FROM employees WHERE status <> 'archived' ORDER BY created_at_ms, id")?;
    let all = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?)))?
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;
    let all: Vec<(EmployeeId, String, String, Option<String>)> = all.into_iter().filter_map(|(id, name, role, title)| Some((EmployeeId::parse(&id).ok()?, name, role, title))).collect();
    let roots: Vec<(EmployeeId, String, Option<String>)> = all.iter().filter(|(id, ..)| org.boss(*id).is_none()).map(|(id, _, role, title)| (*id, role.clone(), title.clone())).collect();
    let Some(ceo) = aurelius_domain::org::pick_ceo(&roots) else { return Ok(Vec::new()) };
    Ok(org
        .management(ceo)
        .into_iter()
        .filter_map(|id| all.iter().find(|(e, ..)| *e == id).map(|(e, name, role, title)| ManagementMember { id: *e, name: name.clone(), role: role.clone(), title: title.clone(), is_ceo: *e == ceo }))
        .collect())
}
