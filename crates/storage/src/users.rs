//! Board members (`users`, migration 18). Fase 1 identifies, it does not
//! authenticate: a user has a name and the argon2id PHC string of its password.
//!
//! The password never reaches storage; the caller hashes it first and passes the
//! hash as its own argument, so it is in no command, event or log. A read
//! (`UserRow`) never carries the hash: only `password_hash` returns it, for the
//! one check when a board member is chosen.

use crate::StorageError;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// The unique index on `username COLLATE NOCASE` refused the name.
pub const USERNAME_TAKEN: &str = "username_taken";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRow {
    pub id: String,
    pub company_id: String,
    pub username: String,
    /// NULL: made in the onboarding, without an active board member.
    pub created_by_user: Option<String>,
    pub version: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

pub struct NewUser<'a> {
    pub id: &'a str,
    pub username: &'a str,
    pub created_by_user: Option<&'a str>,
    pub now_ms: i64,
}

const COLUMNS: &str = "id, company_id, username, created_by_user, version, created_at_ms, updated_at_ms";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<UserRow> {
    Ok(UserRow {
        id: r.get(0)?,
        company_id: r.get(1)?,
        username: r.get(2)?,
        created_by_user: r.get(3)?,
        version: r.get(4)?,
        created_at_ms: r.get(5)?,
        updated_at_ms: r.get(6)?,
    })
}

/// Add a board member to the (only) company. A name that differs only in case
/// from an existing one is refused with `Rejected(USERNAME_TAKEN)`.
pub fn insert(tx: &Transaction<'_>, user: &NewUser<'_>, password_hash: &str) -> Result<(), StorageError> {
    let company: String = tx
        .query_row("SELECT id FROM company", [], |r| r.get(0))
        .optional()?
        .ok_or_else(|| StorageError::Rejected("there is no company".into()))?;
    let inserted = tx.execute(
        "INSERT INTO users(id, company_id, username, password_hash, created_by_user, version, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)",
        params![user.id, company, user.username, password_hash, user.created_by_user, user.now_ms],
    );
    match inserted {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE && by_username(tx, user.username)?.is_some() =>
        {
            Err(StorageError::Rejected(USERNAME_TAKEN.into()))
        }
        Err(e) => Err(e.into()),
    }
}

/// Case-insensitive, like the unique index.
pub fn by_username(conn: &Connection, username: &str) -> Result<Option<UserRow>, StorageError> {
    Ok(conn
        .query_row(&format!("SELECT {COLUMNS} FROM users WHERE username = ?1 COLLATE NOCASE"), [username], row)
        .optional()?)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<UserRow>, StorageError> {
    Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM users WHERE id = ?1"), [id], row).optional()?)
}

/// Oldest first.
pub fn list(conn: &Connection) -> Result<Vec<UserRow>, StorageError> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM users ORDER BY created_at_ms, id"))?;
    let rows = stmt.query_map([], row)?.collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn count(conn: &Connection) -> Result<u64, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get::<_, i64>(0))? as u64)
}

/// The stored PHC string, only to verify a password.
pub fn password_hash(conn: &Connection, id: &str) -> Result<Option<String>, StorageError> {
    Ok(conn.query_row("SELECT password_hash FROM users WHERE id = ?1", [id], |r| r.get(0)).optional()?)
}
