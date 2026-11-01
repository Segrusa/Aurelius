//! The server's name (migration 24): the one a board member chose, or none for
//! the computer's name. The daemon adds the computer's name; storage knows only
//! the choice.

use crate::{StorageError, now_ms};
use aurelius_domain::UserId;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// The stored choice: the name (none: the computer's name) and how often it changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredServerName {
    pub chosen: Option<String>,
    /// 0 before the first change.
    pub version: u64,
}

/// The chosen name and its version; on a database before migration 24, none.
pub fn get(conn: &Connection) -> Result<StoredServerName, StorageError> {
    let has_table: bool = conn.query_row("SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'server_name')", [], |r| r.get(0))?;
    if !has_table {
        return Ok(StoredServerName::default());
    }
    let row = conn
        .query_row("SELECT name, version FROM server_name WHERE id = 1", [], |r| Ok(StoredServerName { chosen: r.get(0)?, version: r.get::<_, i64>(1)? as u64 }))
        .optional()?;
    Ok(row.unwrap_or_default())
}

/// Store `name` (none: back to the computer's name) chosen by `by`; returns the
/// new version. The caller has validated the name and skips an unchanged one.
pub fn set(tx: &Transaction<'_>, name: Option<&str>, by: UserId) -> Result<u64, StorageError> {
    let version = get(tx)?.version + 1;
    tx.execute(
        "INSERT INTO server_name(id, name, version, updated_by_user, updated_at_ms) VALUES (1, ?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, version = excluded.version,
           updated_by_user = excluded.updated_by_user, updated_at_ms = excluded.updated_at_ms",
        params![name, version as i64, by.to_string(), now_ms()],
    )?;
    Ok(version)
}
