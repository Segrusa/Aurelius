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
    Ok(crate::app_keys::management(conn)?.iter().any(|m| m.id == employee))
}

/// Management that chooses its own mode: the employee is management and the setting is on.
pub fn chooses_own_mode(conn: &Connection, employee: EmployeeId) -> Result<bool, StorageError> {
    Ok(enabled(conn)? && is_management(conn, employee)?)
}
