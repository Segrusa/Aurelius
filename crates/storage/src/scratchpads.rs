//! One note per employee (`migrations/0038`, C7): always in its prompt, written with
//! the bridge's `scratchpad` tool.

use crate::{StorageError, now_ms};
use aurelius_domain::{EmployeeId, RunId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// The text and when it last changed; `None` when it was never written or was cleared.
pub fn get(conn: &Connection, employee: EmployeeId) -> Result<Option<(String, i64)>, StorageError> {
    Ok(conn.query_row("SELECT text, updated_at_ms FROM employee_scratchpads WHERE employee_id = ?1", [employee.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
}

/// Replace the note. Empty clears it.
pub fn set(tx: &Transaction<'_>, employee: EmployeeId, text: &str, run: RunId) -> Result<(), StorageError> {
    if text.is_empty() {
        tx.execute("DELETE FROM employee_scratchpads WHERE employee_id = ?1", [employee.to_string()])?;
        return Ok(());
    }
    tx.execute(
        "INSERT INTO employee_scratchpads(employee_id, text, updated_at_ms, updated_by_run) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(employee_id) DO UPDATE SET text = ?2, updated_at_ms = ?3, updated_by_run = ?4",
        params![employee.to_string(), text, now_ms(), run.to_string()],
    )?;
    Ok(())
}
