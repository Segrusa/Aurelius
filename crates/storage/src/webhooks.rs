//! The outbox of webhooks (feature `webhooks`): events waiting to be posted to
//! an app key's webhook (`aurelius_domain::webhook`). A row is written when the
//! event happens and kept until it is delivered or given up, so a restart loses
//! nothing. The row id is the delivery id the receiver sees on every attempt.

use crate::{StorageError, now_ms};
use aurelius_domain::AppKeyId;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// Settings key: the last event (`events.seq`) the dispatcher has handled.
const CURSOR: &str = "webhook_cursor";
/// Finished rows are kept this long (for the audit and for debugging a receiver).
pub const KEEP_MS: i64 = 7 * 24 * 3_600_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxRow {
    pub id: String,
    pub key_id: AppKeyId,
    pub event: String,
    pub body_json: String,
    /// Attempts made so far.
    pub attempts: u32,
    pub created_at_ms: i64,
}

/// Queue one delivery. A second call with the same `id` changes nothing, so
/// handling an event twice (a crash before the cursor moved) sends it once.
pub fn enqueue(tx: &Transaction<'_>, id: &str, key: AppKeyId, event: &str, body_json: &str) -> Result<bool, StorageError> {
    let now = now_ms();
    Ok(tx.execute(
        "INSERT OR IGNORE INTO webhook_outbox(id, key_id, event, body_json, created_at_ms, next_attempt_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, key.to_string(), event, body_json, now],
    )? == 1)
}

/// Deliveries due at `now`, oldest first.
pub fn due(conn: &Connection, now: i64, limit: u32) -> Result<Vec<OutboxRow>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT id, key_id, event, body_json, attempts, created_at_ms FROM webhook_outbox
         WHERE delivered_at_ms IS NULL AND gave_up_at_ms IS NULL AND next_attempt_ms <= ?1
         ORDER BY created_at_ms, id LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![now, limit], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?)))?
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, key, event, body_json, attempts, created_at_ms)| Some(OutboxRow { id, key_id: AppKeyId::parse(&key).ok()?, event, body_json, attempts: attempts as u32, created_at_ms }))
        .collect())
}

/// When the next waiting delivery is due (none: nothing waits).
pub fn next_due_ms(conn: &Connection) -> Result<Option<i64>, StorageError> {
    Ok(conn
        .query_row("SELECT MIN(next_attempt_ms) FROM webhook_outbox WHERE delivered_at_ms IS NULL AND gave_up_at_ms IS NULL", [], |r| r.get::<_, Option<i64>>(0))
        .optional()?
        .flatten())
}

pub fn mark_delivered(tx: &Transaction<'_>, id: &str, status: u16) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE webhook_outbox SET attempts = attempts + 1, delivered_at_ms = ?2, last_status = ?3, last_error = NULL WHERE id = ?1",
        params![id, now_ms(), status],
    )?;
    Ok(())
}

/// An attempt failed. `retry_at_ms`: when to try again; none: this was the last attempt.
pub fn mark_failed(tx: &Transaction<'_>, id: &str, retry_at_ms: Option<i64>, status: Option<u16>, error: &str) -> Result<(), StorageError> {
    let now = now_ms();
    match retry_at_ms {
        Some(at) => tx.execute(
            "UPDATE webhook_outbox SET attempts = attempts + 1, next_attempt_ms = ?2, last_status = ?3, last_error = ?4 WHERE id = ?1",
            params![id, at, status, error],
        )?,
        None => tx.execute(
            "UPDATE webhook_outbox SET attempts = attempts + 1, gave_up_at_ms = ?2, last_status = ?3, last_error = ?4 WHERE id = ?1",
            params![id, now, status, error],
        )?,
    };
    Ok(())
}

/// Stop trying (the key was revoked or lost its webhook).
pub fn give_up(tx: &Transaction<'_>, id: &str, why: &str) -> Result<(), StorageError> {
    tx.execute("UPDATE webhook_outbox SET gave_up_at_ms = ?2, last_error = ?3 WHERE id = ?1", params![id, now_ms(), why])?;
    Ok(())
}

/// Forget finished deliveries older than `KEEP_MS`.
pub fn purge(tx: &Transaction<'_>) -> Result<usize, StorageError> {
    let before = now_ms() - KEEP_MS;
    Ok(tx.execute(
        "DELETE FROM webhook_outbox WHERE (delivered_at_ms IS NOT NULL AND delivered_at_ms < ?1) OR (gave_up_at_ms IS NOT NULL AND gave_up_at_ms < ?1)",
        [before],
    )?)
}

/// The last event handled; none until the dispatcher first ran.
pub fn cursor(conn: &Connection) -> Result<Option<u64>, StorageError> {
    Ok(crate::settings::get(conn, CURSOR)?.and_then(|v| v.trim().parse().ok()))
}

pub fn set_cursor(tx: &Transaction<'_>, seq: u64) -> Result<(), StorageError> {
    crate::settings::put(tx, CURSOR, &seq.to_string(), now_ms())
}

/// What the event log says about `seq`: the aggregate and the metadata.
pub fn event_detail(conn: &Connection, seq: u64) -> Result<Option<(String, String, serde_json::Value)>, StorageError> {
    let row: Option<(String, String, String)> =
        conn.query_row("SELECT aggregate_type, aggregate_id, metadata_json FROM events WHERE seq = ?1", [seq as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    Ok(row.map(|(kind, id, meta)| (kind, id, serde_json::from_str(&meta).unwrap_or(serde_json::Value::Null))))
}

/// Deliveries of a key that are still waiting, and how many were given up.
pub fn counts(conn: &Connection, key: AppKeyId) -> Result<(u32, u32), StorageError> {
    let waiting: i64 = conn.query_row("SELECT COUNT(*) FROM webhook_outbox WHERE key_id = ?1 AND delivered_at_ms IS NULL AND gave_up_at_ms IS NULL", [key.to_string()], |r| r.get(0))?;
    let gave_up: i64 = conn.query_row("SELECT COUNT(*) FROM webhook_outbox WHERE key_id = ?1 AND gave_up_at_ms IS NOT NULL", [key.to_string()], |r| r.get(0))?;
    Ok((waiting as u32, gave_up as u32))
}
