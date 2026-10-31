//! The sessions of the board members (migration 21). A login makes one; the
//! client keeps the token, storage keeps only its verifier (the caller hashes
//! the token, like `remote::create_session`). A session is valid while it is
//! not revoked, younger than 90 days, used within the last 30 days, its board
//! member is not disabled and, for a remote one, its device is not revoked.
//! `client_id` none: the local IPC (the desktop on the server itself).

use crate::StorageError;
use aurelius_domain::ClientId;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// A session expires this long after it was last used.
pub const IDLE_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// And this long after it was made, however much it is used.
pub const LIFETIME_MS: i64 = 90 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    pub id: String,
    pub user_id: String,
    pub client_id: Option<ClientId>,
    pub device_name: String,
    pub created_at_ms: i64,
    pub last_seen_ms: i64,
    pub expires_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
}

pub struct NewSession<'a> {
    pub id: &'a str,
    pub user_id: &'a str,
    pub client_id: Option<ClientId>,
    pub device_name: &'a str,
    pub now_ms: i64,
}

const COLUMNS: &str = "s.id, s.user_id, s.client_id, s.device_name, s.created_at_ms, s.last_seen_ms, s.expires_at_ms, s.revoked_at_ms";

/// The conditions of a valid session `s` at `:now`, as SQL on `user_sessions s`.
const VALID: &str = "s.revoked_at_ms IS NULL AND s.expires_at_ms > :now AND s.last_seen_ms > :now - :idle
     AND EXISTS (SELECT 1 FROM users u WHERE u.id = s.user_id AND u.disabled_at_ms IS NULL)
     AND (s.client_id IS NULL OR EXISTS (SELECT 1 FROM remote_clients c WHERE c.id = s.client_id AND c.revoked_at_ms IS NULL))";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: r.get(0)?,
        user_id: r.get(1)?,
        client_id: r
            .get::<_, Option<String>>(2)?
            .map(|c| ClientId::parse(&c).map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))))
            .transpose()?,
        device_name: r.get(3)?,
        created_at_ms: r.get(4)?,
        last_seen_ms: r.get(5)?,
        expires_at_ms: r.get(6)?,
        revoked_at_ms: r.get(7)?,
    })
}

/// Store a new session with the verifier of its token; it expires
/// `LIFETIME_MS` from now. A remote device remembers the board member as the
/// last who used it (`remote_clients.user_id`).
pub fn insert(tx: &Transaction<'_>, session: &NewSession<'_>, token_verifier: &str) -> Result<SessionRow, StorageError> {
    let client = session.client_id.map(|c| c.to_string());
    tx.execute(
        "INSERT INTO user_sessions(id, user_id, client_id, device_name, token_verifier, created_at_ms, last_seen_ms, expires_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)",
        params![session.id, session.user_id, client, session.device_name, token_verifier, session.now_ms, session.now_ms + LIFETIME_MS],
    )?;
    if let Some(client) = &client {
        tx.execute("UPDATE remote_clients SET user_id = ?2 WHERE id = ?1", params![client, session.user_id])?;
    }
    Ok(SessionRow {
        id: session.id.to_string(),
        user_id: session.user_id.to_string(),
        client_id: session.client_id,
        device_name: session.device_name.to_string(),
        created_at_ms: session.now_ms,
        last_seen_ms: session.now_ms,
        expires_at_ms: session.now_ms + LIFETIME_MS,
        revoked_at_ms: None,
    })
}

/// The valid session with this verifier at `now_ms`; none when there is no
/// such session or it is revoked, expired (idle or absolute), its board member
/// is disabled or its device revoked.
pub fn by_verifier(conn: &Connection, token_verifier: &str, now_ms: i64) -> Result<Option<SessionRow>, StorageError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM user_sessions s WHERE s.token_verifier = :verifier AND {VALID}"),
            rusqlite::named_params! { ":verifier": token_verifier, ":now": now_ms, ":idle": IDLE_MS },
            row,
        )
        .optional()?)
}

/// The session with this id if it is still valid at `now_ms` (as `by_verifier`):
/// what an open connection checks its session against, so an end that did not
/// reach it (another process, a race with the hub) still signs it out.
pub fn valid(conn: &Connection, id: &str, now_ms: i64) -> Result<Option<SessionRow>, StorageError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM user_sessions s WHERE s.id = :id AND {VALID}"),
            rusqlite::named_params! { ":id": id, ":now": now_ms, ":idle": IDLE_MS },
            row,
        )
        .optional()?)
}

/// Any session by id, valid or not.
pub fn get(conn: &Connection, id: &str) -> Result<Option<SessionRow>, StorageError> {
    Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM user_sessions s WHERE s.id = ?1"), [id], row).optional()?)
}

/// The session was used: its idle time starts again (its absolute expiry
/// stays). Not for a revoked one.
pub fn touch(tx: &Transaction<'_>, id: &str, now_ms: i64) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE user_sessions SET last_seen_ms = MAX(last_seen_ms, ?2) WHERE id = ?1 AND revoked_at_ms IS NULL",
        params![id, now_ms],
    )?;
    Ok(())
}

/// The valid sessions of a board member, most recently used first.
pub fn list_for_user(conn: &Connection, user_id: &str, now_ms: i64) -> Result<Vec<SessionRow>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM user_sessions s WHERE s.user_id = :user AND {VALID} ORDER BY s.last_seen_ms DESC, s.id"
    ))?;
    let rows = stmt
        .query_map(rusqlite::named_params! { ":user": user_id, ":now": now_ms, ":idle": IDLE_MS }, row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Revoke one session of this board member; false when it is not theirs or
/// already revoked.
pub fn revoke(tx: &Transaction<'_>, user_id: &str, id: &str, now_ms: i64) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE user_sessions SET revoked_at_ms = ?3 WHERE id = ?1 AND user_id = ?2 AND revoked_at_ms IS NULL",
        params![id, user_id, now_ms],
    )? == 1)
}

/// Revoke every session of a board member; returns how many.
pub fn revoke_for_user(tx: &Transaction<'_>, user_id: &str, now_ms: i64) -> Result<usize, StorageError> {
    Ok(tx.execute(
        "UPDATE user_sessions SET revoked_at_ms = ?2 WHERE user_id = ?1 AND revoked_at_ms IS NULL",
        params![user_id, now_ms],
    )?)
}

/// Revoke every session of a board member except `keep` (the one in use).
pub fn revoke_others(tx: &Transaction<'_>, user_id: &str, keep: &str, now_ms: i64) -> Result<usize, StorageError> {
    Ok(tx.execute(
        "UPDATE user_sessions SET revoked_at_ms = ?3 WHERE user_id = ?1 AND id <> ?2 AND revoked_at_ms IS NULL",
        params![user_id, keep, now_ms],
    )?)
}

/// Revoke every session made on a device (also done by `remote::revoke_client`).
pub fn revoke_for_client(tx: &Transaction<'_>, client_id: ClientId, now_ms: i64) -> Result<usize, StorageError> {
    Ok(tx.execute(
        "UPDATE user_sessions SET revoked_at_ms = ?2 WHERE client_id = ?1 AND revoked_at_ms IS NULL",
        params![client_id.to_string(), now_ms],
    )?)
}
