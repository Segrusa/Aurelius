//! Trace mode (off by default): per run what was sent to the runtime and what
//! each model call cost. Only the latest runs keep one, so the database stays small.

use crate::{StorageError, now_ms};
use aurelius_domain::views::{RunTrace, TraceSetting};
use aurelius_domain::{ConversationId, RunId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// `settings` key: `true` makes every run keep a trace.
pub const SETTING_KEY: &str = "trace_mode";
/// Traces kept at most: the newest.
pub const KEEP_RUNS: u32 = 200;
/// Traces older than this go.
pub const KEEP_DAYS: u32 = 7;

pub fn enabled(conn: &Connection) -> Result<bool, StorageError> {
    Ok(crate::settings::get(conn, SETTING_KEY)?.and_then(|j| serde_json::from_str::<bool>(&j).ok()).unwrap_or(false))
}

pub fn setting(conn: &Connection) -> Result<TraceSetting, StorageError> {
    Ok(TraceSetting { enabled: enabled(conn)?, keep_runs: KEEP_RUNS, keep_days: KEEP_DAYS })
}

/// Turning it off also forgets the traces kept so far.
pub fn set_enabled(tx: &Transaction<'_>, enabled: bool, now: i64) -> Result<TraceSetting, StorageError> {
    crate::settings::put(tx, SETTING_KEY, if enabled { "true" } else { "false" }, now)?;
    if !enabled {
        tx.execute("DELETE FROM run_traces", [])?;
    }
    Ok(TraceSetting { enabled, keep_runs: KEEP_RUNS, keep_days: KEEP_DAYS })
}

/// Keep (or replace) a run's trace, then drop what is over the limits.
pub fn put(tx: &Transaction<'_>, trace: &RunTrace) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO run_traces(run_id, conversation_id, created_at_ms, trace_json) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(run_id) DO UPDATE SET trace_json = excluded.trace_json",
        params![trace.run_id.to_string(), trace.conversation_id.map(|c| c.to_string()), trace.created_at_ms, serde_json::to_string(trace)?],
    )?;
    prune(tx, now_ms())
}

/// Drop traces older than `KEEP_DAYS` and all but the newest `KEEP_RUNS`.
pub fn prune(tx: &Transaction<'_>, now: i64) -> Result<(), StorageError> {
    tx.execute("DELETE FROM run_traces WHERE created_at_ms < ?1", [now - KEEP_DAYS as i64 * 86_400_000])?;
    tx.execute(
        "DELETE FROM run_traces WHERE run_id NOT IN (SELECT run_id FROM run_traces ORDER BY created_at_ms DESC, run_id DESC LIMIT ?1)",
        [KEEP_RUNS as i64],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, run: RunId) -> Result<Option<RunTrace>, StorageError> {
    let json: Option<String> = conn.query_row("SELECT trace_json FROM run_traces WHERE run_id = ?1", [run.to_string()], |r| r.get(0)).optional()?;
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

/// A deleted conversation keeps no traces (they hold its messages).
pub fn delete_for_conversation(tx: &Transaction<'_>, conversation: ConversationId) -> Result<(), StorageError> {
    tx.execute("DELETE FROM run_traces WHERE conversation_id = ?1", [conversation.to_string()])?;
    Ok(())
}
