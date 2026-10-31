//! The Claude login of each board member (migration 21): only its status and
//! what `claude auth status` reports (e-mail, plan). Never a token or a path:
//! the folder is always `<data>/logins/<user_id>/`, known to the runtime.

use crate::StorageError;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginStatus {
    Linked,
    Missing,
    Expired,
}

impl LoginStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            LoginStatus::Linked => "linked",
            LoginStatus::Missing => "missing",
            LoginStatus::Expired => "expired",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "linked" => Some(LoginStatus::Linked),
            "missing" => Some(LoginStatus::Missing),
            "expired" => Some(LoginStatus::Expired),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeLoginRow {
    pub user_id: String,
    pub status: LoginStatus,
    pub email: Option<String>,
    pub plan: Option<String>,
    /// When it last became linked; kept while it stays linked.
    pub linked_at_ms: Option<i64>,
    pub checked_at_ms: i64,
}

/// Record what a check found. `linked_at_ms` is set when the login becomes
/// linked (new, or from missing/expired) and kept otherwise; `checked_at_ms` is
/// always `now_ms`. E-mail and plan are replaced by what was found.
pub fn upsert(
    tx: &Transaction<'_>,
    user_id: &str,
    status: LoginStatus,
    email: Option<&str>,
    plan: Option<&str>,
    now_ms: i64,
) -> Result<(), StorageError> {
    let linked_at = (status == LoginStatus::Linked).then_some(now_ms);
    tx.execute(
        "INSERT INTO claude_logins(user_id, status, email, plan, linked_at_ms, checked_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(user_id) DO UPDATE SET
           linked_at_ms = CASE WHEN excluded.status = 'linked' AND claude_logins.status <> 'linked' THEN excluded.linked_at_ms
                               ELSE claude_logins.linked_at_ms END,
           status = excluded.status, email = excluded.email, plan = excluded.plan, checked_at_ms = excluded.checked_at_ms",
        params![user_id, status.as_str(), email, plan, linked_at, now_ms],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, user_id: &str) -> Result<Option<ClaudeLoginRow>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT user_id, status, email, plan, linked_at_ms, checked_at_ms FROM claude_logins WHERE user_id = ?1",
            [user_id],
            |r| {
                Ok(ClaudeLoginRow {
                    user_id: r.get(0)?,
                    status: LoginStatus::parse(&r.get::<_, String>(1)?).unwrap_or(LoginStatus::Missing),
                    email: r.get(2)?,
                    plan: r.get(3)?,
                    linked_at_ms: r.get(4)?,
                    checked_at_ms: r.get(5)?,
                })
            },
        )
        .optional()?)
}

/// Whether the board member has a linked login (and is not disabled): the
/// check before a run starts on it.
pub fn is_linked(conn: &Connection, user_id: &str) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM claude_logins l JOIN users u ON u.id = l.user_id
                       WHERE l.user_id = ?1 AND l.status = 'linked' AND u.disabled_at_ms IS NULL)",
        [user_id],
        |r| r.get(0),
    )?)
}

/// Forget the login (unlinked, or its board member disabled); true when there was one.
pub fn delete(tx: &Transaction<'_>, user_id: &str) -> Result<bool, StorageError> {
    Ok(tx.execute("DELETE FROM claude_logins WHERE user_id = ?1", [user_id])? == 1)
}
