//! Server settings the user changes in the app: one JSON value per key.

use crate::StorageError;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// The stored JSON for `key`, if the user ever set it.
pub fn get(conn: &Connection, key: &str) -> Result<Option<String>, StorageError> {
    Ok(conn.query_row("SELECT value_json FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

pub fn put(tx: &Transaction<'_>, key: &str, value_json: &str, now: i64) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO settings (key, value_json, updated_at_ms) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at_ms = excluded.updated_at_ms",
        params![key, value_json, now],
    )?;
    Ok(())
}

/// Every `(key, value_json)` whose key starts with `prefix`, in key order.
pub fn with_prefix(conn: &Connection, prefix: &str) -> Result<Vec<(String, String)>, StorageError> {
    let mut stmt = conn.prepare("SELECT key, value_json FROM settings WHERE substr(key, 1, length(?1)) = ?1 ORDER BY key")?;
    let rows = stmt.query_map([prefix], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The MCP tools the user always allows (`mcp__<server>__<tool>`), server-wide.
pub const MCP_ALWAYS_ALLOW: &str = "mcp_always_allow";

pub fn mcp_always_allow(conn: &Connection) -> Result<Vec<String>, StorageError> {
    Ok(get(conn, MCP_ALWAYS_ALLOW)?.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
}

pub fn set_mcp_always_allow(tx: &Transaction<'_>, tools: &[String], now: i64) -> Result<(), StorageError> {
    let json = serde_json::to_string(tools).map_err(|e| StorageError::Rejected(e.to_string()))?;
    put(tx, MCP_ALWAYS_ALLOW, &json, now)
}

/// How runs drive the browser: `"claude_in_chrome"` (also when unset) or `"playwright"`.
pub const BROWSER_BACKEND: &str = "browser_backend";

/// Runs use Playwright MCP instead of Claude in Chrome.
pub fn browser_uses_playwright(conn: &Connection) -> Result<bool, StorageError> {
    Ok(get(conn, BROWSER_BACKEND)?.is_some_and(|j| j.trim() == "\"playwright\""))
}
