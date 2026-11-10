//! Board members (`users`, migration 18). A user has a name and the argon2id
//! PHC string of its password. Since migration 21 a user can be disabled (never
//! deleted) and its password reset to a temporary one (`must_change_password`).
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
    pub disabled_at_ms: Option<i64>,
    pub password_changed_at_ms: Option<i64>,
    /// The password is a temporary one: it must be changed at the next login.
    pub must_change_password: bool,
    /// An administrator (migration 41): resets another's password, ends their
    /// sessions, disables them and names other administrators.
    pub admin: bool,
}

impl UserRow {
    pub fn disabled(&self) -> bool {
        self.disabled_at_ms.is_some()
    }
}

pub struct NewUser<'a> {
    pub id: &'a str,
    pub username: &'a str,
    pub created_by_user: Option<&'a str>,
    pub now_ms: i64,
}

const COLUMNS: &str =
    "id, company_id, username, created_by_user, version, created_at_ms, updated_at_ms, disabled_at_ms, password_changed_at_ms, must_change_password, admin";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<UserRow> {
    Ok(UserRow {
        id: r.get(0)?,
        company_id: r.get(1)?,
        username: r.get(2)?,
        created_by_user: r.get(3)?,
        version: r.get(4)?,
        created_at_ms: r.get(5)?,
        updated_at_ms: r.get(6)?,
        disabled_at_ms: r.get(7)?,
        password_changed_at_ms: r.get(8)?,
        must_change_password: r.get(9)?,
        admin: r.get(10)?,
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
        // The first board member of a server is its administrator.
        "INSERT INTO users(id, company_id, username, password_hash, created_by_user, version, created_at_ms, updated_at_ms, admin)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6, NOT EXISTS(SELECT 1 FROM users))",
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

/// The board members that are not disabled (e.g. so the last one cannot be).
pub fn count_active(conn: &Connection) -> Result<u64, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM users WHERE disabled_at_ms IS NULL", [], |r| r.get::<_, i64>(0))? as u64)
}

/// The administrators that are not disabled (so the last one stays one).
pub fn count_admins(conn: &Connection) -> Result<u64, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM users WHERE admin = 1 AND disabled_at_ms IS NULL", [], |r| r.get::<_, i64>(0))? as u64)
}

/// Make a board member an administrator, or an ordinary one again. False when unknown.
pub fn set_admin(tx: &Transaction<'_>, id: &str, admin: bool, now_ms: i64) -> Result<bool, StorageError> {
    Ok(tx.execute("UPDATE users SET admin = ?2, version = version + 1, updated_at_ms = ?3 WHERE id = ?1", params![id, admin, now_ms])? == 1)
}

/// Disable a board member: every session of theirs is revoked, and their
/// login is no longer the company login. False when unknown or already
/// disabled. The caller removes the Claude login itself (`claude_logins`, the folder).
pub fn disable(tx: &Transaction<'_>, id: &str, now_ms: i64) -> Result<bool, StorageError> {
    let changed = tx.execute(
        "UPDATE users SET disabled_at_ms = ?2, version = version + 1, updated_at_ms = ?2 WHERE id = ?1 AND disabled_at_ms IS NULL",
        params![id, now_ms],
    )?;
    if changed == 0 {
        return Ok(false);
    }
    crate::user_sessions::revoke_for_user(tx, id, now_ms)?;
    tx.execute("UPDATE company SET background_login_user = NULL WHERE background_login_user = ?1", [id])?;
    Ok(true)
}

/// The same password hashed again with stronger parameters (at a login): no
/// session ends and the password counts as unchanged.
pub fn replace_hash(tx: &Transaction<'_>, id: &str, password_hash: &str) -> Result<bool, StorageError> {
    Ok(tx.execute("UPDATE users SET password_hash = ?2 WHERE id = ?1", params![id, password_hash])? == 1)
}

/// A new password (its PHC string). `must_change`: it is a temporary one set by
/// someone else, to be changed at the next login. Every session of the board
/// member is revoked except `keep` (the session that changed its own
/// password; none when another board member reset it). False when unknown.
pub fn set_password(
    tx: &Transaction<'_>,
    id: &str,
    password_hash: &str,
    must_change: bool,
    keep: Option<&str>,
    now_ms: i64,
) -> Result<bool, StorageError> {
    let changed = tx.execute(
        "UPDATE users SET password_hash = ?2, password_changed_at_ms = ?3, must_change_password = ?4, version = version + 1, updated_at_ms = ?3
         WHERE id = ?1",
        params![id, password_hash, now_ms, must_change],
    )?;
    if changed == 0 {
        return Ok(false);
    }
    match keep {
        Some(keep) => crate::user_sessions::revoke_others(tx, id, keep, now_ms)?,
        None => crate::user_sessions::revoke_for_user(tx, id, now_ms)?,
    };
    Ok(true)
}

/// The stored PHC string, only to verify a password.
pub fn password_hash(conn: &Connection, id: &str) -> Result<Option<String>, StorageError> {
    Ok(conn.query_row("SELECT password_hash FROM users WHERE id = ?1", [id], |r| r.get(0)).optional()?)
}

impl UserRow {
    /// As every client sees it (never the password's hash); none for a damaged id.
    pub fn view(&self) -> Option<aurelius_domain::user::UserView> {
        Some(aurelius_domain::user::UserView {
            id: aurelius_domain::UserId::parse(&self.id).ok()?,
            username: self.username.clone(),
            created_at_ms: self.created_at_ms,
            created_by_user: self.created_by_user.as_deref().and_then(|u| aurelius_domain::UserId::parse(u).ok()),
            disabled_at_ms: self.disabled_at_ms,
            admin: self.admin,
        })
    }
}
