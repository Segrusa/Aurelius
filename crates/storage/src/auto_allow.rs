//! Auto-allow of the safe read-only tools: the server setting (on unless the user turned it off).

use crate::StorageError;
use rusqlite::{Connection, Transaction};

pub const SETTING_KEY: &str = "auto_allow_safe_tools";

pub fn enabled(conn: &Connection) -> Result<bool, StorageError> {
    Ok(crate::settings::get(conn, SETTING_KEY)?.and_then(|j| serde_json::from_str::<bool>(&j).ok()).unwrap_or(true))
}

pub fn set_enabled(tx: &Transaction<'_>, enabled: bool, now: i64) -> Result<(), StorageError> {
    crate::settings::put(tx, SETTING_KEY, if enabled { "true" } else { "false" }, now)
}
