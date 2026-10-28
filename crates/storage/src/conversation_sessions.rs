//! The runtime session a conversation goes on in (`migrations/0008`): one per
//! conversation, claimed by the run that uses it, and given back with what the
//! session has seen when that run completes.

use crate::{StorageError, now_ms};
use aurelius_domain::{ConversationId, RunId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// Run statuses of a run that still goes on (or waits to).
const UNFINISHED: &str = "('created','starting','running','waiting','cancelling')";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    pub session_id: String,
    /// The run going on in it, or waiting for a usage limit to go on.
    pub busy_run: Option<String>,
    /// How many of the conversation's messages the session has, in order, before its last answer.
    pub seen_count: u32,
    /// Digest of those messages; empty while the session's first run goes on.
    pub seen_digest: String,
    /// The session's last answer (follows the seen messages).
    pub answer_id: Option<String>,
    pub model: String,
    pub folder: String,
    pub profile: String,
    /// What the session last knew of its context (for the change note).
    pub context_json: String,
    /// The system text it was started with, given again as it is on every run in it
    /// (empty for a session from before it was kept).
    pub system_text: String,
    /// When that text was made (0: unknown).
    pub system_at_ms: i64,
    /// How big its context was at its last answer (input, cache reads and cache
    /// writes of its last model call; 0: unknown).
    pub context_tokens: u64,
}

/// The server setting: a session whose context grew past this many tokens is
/// compacted before its next turn (0: never).
pub const COMPACT_THRESHOLD: &str = "session_compact_threshold";
pub const DEFAULT_COMPACT_THRESHOLD: u64 = 120_000;

pub fn compact_threshold(conn: &Connection) -> Result<u64, StorageError> {
    Ok(crate::settings::get(conn, COMPACT_THRESHOLD)?.and_then(|j| serde_json::from_str::<u64>(j.trim()).ok()).unwrap_or(DEFAULT_COMPACT_THRESHOLD))
}

pub fn set_compact_threshold(tx: &Transaction<'_>, tokens: u64) -> Result<(), StorageError> {
    crate::settings::put(tx, COMPACT_THRESHOLD, &tokens.to_string(), now_ms())
}

/// What it was started with: a change starts a new session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionKey {
    pub model: String,
    pub folder: String,
    pub profile: String,
}

/// What a run gets when it asks for the conversation's session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    /// It goes on in the session it asked for.
    Continue,
    /// It starts the conversation's new session, named after the run; `replaced`
    /// is the earlier session, which the runtime can drop.
    Start { replaced: Option<String> },
    /// Another run goes on in the conversation's session: this one keeps a session of its own.
    Busy,
}

pub fn get(conn: &Connection, conversation: ConversationId) -> Result<Option<SessionRow>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT session_id, busy_run_id, seen_count, seen_digest, answer_id, model, folder, profile, context_json, system_text, system_at_ms, context_tokens
             FROM conversation_sessions WHERE conversation_id = ?1",
            [conversation.to_string()],
            |r| {
                Ok(SessionRow {
                    session_id: r.get(0)?,
                    busy_run: r.get(1)?,
                    seen_count: r.get::<_, i64>(2)?.max(0) as u32,
                    seen_digest: r.get(3)?,
                    answer_id: r.get(4)?,
                    model: r.get(5)?,
                    folder: r.get(6)?,
                    profile: r.get(7)?,
                    context_json: r.get(8)?,
                    system_text: r.get(9)?,
                    system_at_ms: r.get(10)?,
                    context_tokens: r.get::<_, i64>(11)?.max(0) as u64,
                })
            },
        )
        .optional()?)
}

fn unfinished(conn: &Connection, run: &str) -> Result<bool, StorageError> {
    Ok(conn
        .query_row(&format!("SELECT 1 FROM runs WHERE id = ?1 AND status IN {UNFINISHED}"), [run], |_| Ok(()))
        .optional()?
        .is_some())
}

/// Run `run` asks for the conversation's session: to go on in `resume` (the
/// stored session it checked), or for a new one. A session another live run
/// holds is never touched; one held by a run that is over (it crashed, was
/// stopped while it waited) may hold half a turn, so it is replaced. A new
/// session keeps `system`, the system text it starts with; one from before that
/// has none keeps it from now on.
pub fn claim(tx: &Transaction<'_>, conversation: ConversationId, run: RunId, resume: Option<&str>, key: &SessionKey, system: &str) -> Result<Claim, StorageError> {
    let run = run.to_string();
    let row = get(tx, conversation)?;
    if let Some(row) = &row {
        match row.busy_run.as_deref() {
            Some(busy) if busy != run && unfinished(tx, busy)? => return Ok(Claim::Busy),
            busy => {
                let free = busy.is_none() || busy == Some(run.as_str());
                if free && resume == Some(row.session_id.as_str()) && !row.seen_digest.is_empty() {
                    tx.execute(
                        "UPDATE conversation_sessions SET busy_run_id = ?2, updated_at_ms = ?3,
                           system_at_ms = CASE WHEN system_text = '' THEN ?3 ELSE system_at_ms END,
                           system_text = CASE WHEN system_text = '' THEN ?4 ELSE system_text END
                         WHERE conversation_id = ?1",
                        params![conversation.to_string(), run, now_ms(), system],
                    )?;
                    return Ok(Claim::Continue);
                }
            }
        }
    }
    tx.execute(
        "INSERT INTO conversation_sessions(conversation_id, session_id, busy_run_id, seen_count, seen_digest, answer_id, model, folder, profile, context_json, updated_at_ms, system_text, system_at_ms)
         VALUES (?1, ?2, ?2, 0, '', NULL, ?3, ?4, ?5, '{}', ?6, ?7, ?6)
         ON CONFLICT(conversation_id) DO UPDATE SET session_id = ?2, busy_run_id = ?2, seen_count = 0, seen_digest = '', answer_id = NULL,
           model = ?3, folder = ?4, profile = ?5, context_json = '{}', updated_at_ms = ?6, system_text = ?7, system_at_ms = ?6, context_tokens = 0",
        params![conversation.to_string(), run, key.model, key.folder, key.profile, now_ms(), system],
    )?;
    Ok(Claim::Start { replaced: row.map(|r| r.session_id).filter(|id| *id != run) })
}

/// What run `run` leaves the session with when it completed: the messages it
/// has now, its last answer and how big its context is now (kept as it was when
/// the runtime did not say). Only the run holding the session can.
#[allow(clippy::too_many_arguments)]
pub fn complete(
    tx: &Transaction<'_>,
    conversation: ConversationId,
    run: RunId,
    seen_count: u32,
    seen_digest: &str,
    answer: aurelius_domain::MessageId,
    context_json: &str,
    context_tokens: Option<u64>,
) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE conversation_sessions SET busy_run_id = NULL, seen_count = ?3, seen_digest = ?4, answer_id = ?5, context_json = ?6, updated_at_ms = ?7,
           context_tokens = COALESCE(?8, context_tokens)
         WHERE conversation_id = ?1 AND busy_run_id = ?2",
        params![conversation.to_string(), run.to_string(), seen_count, seen_digest, answer.to_string(), context_json, now_ms(), context_tokens.map(|n| n as i64)],
    )? == 1)
}

/// Run `run` did not complete (stopped, failed): its session no longer matches the
/// conversation and goes. Returns its id, for the runtime to remove.
pub fn release(tx: &Transaction<'_>, conversation: ConversationId, run: RunId) -> Result<Option<String>, StorageError> {
    let row = get(tx, conversation)?.filter(|r| r.busy_run.as_deref() == Some(run.to_string().as_str()));
    if row.is_some() {
        tx.execute("DELETE FROM conversation_sessions WHERE conversation_id = ?1", [conversation.to_string()])?;
    }
    Ok(row.map(|r| r.session_id))
}

/// Sessions whose conversation is over (deleted), or held by a run that is over
/// without giving it back. Not ones a live run holds. A task's conversation keeps
/// its session after the task ended: the user goes on talking in it. They are
/// forgotten here; returns their ids.
pub fn take_over(tx: &Transaction<'_>, only: Option<ConversationId>, live: &[String]) -> Result<Vec<String>, StorageError> {
    let mut stmt = tx.prepare(&format!(
        "SELECT s.conversation_id, s.session_id, s.busy_run_id FROM conversation_sessions s
         LEFT JOIN conversations c ON c.id = s.conversation_id
         WHERE (?1 IS NULL OR s.conversation_id = ?1)
           AND (c.id IS NULL OR c.archived_at_ms IS NOT NULL
                OR (s.busy_run_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM runs r WHERE r.id = s.busy_run_id AND r.status IN {UNFINISHED})))"
    ))?;
    let rows: Vec<(String, String, Option<String>)> =
        stmt.query_map([only.map(|c| c.to_string())], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
    drop(stmt);
    let mut out = Vec::new();
    for (conversation, session, busy) in rows {
        // A run still streaming in it (its status may lag behind) keeps it.
        if busy.as_ref().is_some_and(|b| live.contains(b)) {
            continue;
        }
        tx.execute("DELETE FROM conversation_sessions WHERE conversation_id = ?1", [&conversation])?;
        out.push(session);
    }
    Ok(out)
}

/// An employee's instructions changed: the system text its conversations' sessions
/// keep is out of date, so each of them starts a new session on its next run (as
/// when the text is a day old). A run going on in one now finishes in it. Returns
/// how many sessions this concerns.
pub fn expire_for_employee(tx: &Transaction<'_>, employee: aurelius_domain::EmployeeId) -> Result<usize, StorageError> {
    Ok(tx.execute(
        "UPDATE conversation_sessions SET system_at_ms = 1
         WHERE system_text <> '' AND conversation_id IN (SELECT id FROM conversations WHERE employee_id = ?1)",
        [employee.to_string()],
    )?)
}

/// Every session id a conversation keeps (never leftovers to sweep).
pub fn session_ids(conn: &Connection) -> Result<std::collections::HashSet<String>, StorageError> {
    let mut stmt = conn.prepare("SELECT session_id FROM conversation_sessions")?;
    let ids = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<_, _>>()?;
    Ok(ids)
}

/// The data root this database last ran in (a JSON string).
pub const SESSIONS_ROOT: &str = "sessions_root";
/// Sessions started before this time (ms) belong to another data root.
pub const SESSIONS_OWNED_SINCE: &str = "sessions_owned_since_ms";

/// Which Claude Code sessions this data root may remove: those started from
/// the returned time on (`None`: all of them). Claude Code keeps its sessions
/// per user (`~/.claude`, `%TEMP%\claude`), so a database copied to another
/// root, or a backup restored from one, holds the ids of sessions another
/// daemon still uses. The first start in such a root marks everything before
/// it as not ours. An older database seen for the first time is trusted only
/// in this host's default root.
pub fn claim_root(tx: &Transaction<'_>, root: &str, default_root: bool, now: i64) -> Result<Option<i64>, StorageError> {
    let stored: Option<String> = crate::settings::get(tx, SESSIONS_ROOT)?.and_then(|j| serde_json::from_str(&j).ok());
    let root_json = serde_json::to_string(root).map_err(|e| StorageError::Rejected(e.to_string()))?;
    match stored {
        Some(stored) if stored == root => {}
        None if default_root => crate::settings::put(tx, SESSIONS_ROOT, &root_json, now)?,
        _ => {
            crate::settings::put(tx, SESSIONS_ROOT, &root_json, now)?;
            crate::settings::put(tx, SESSIONS_OWNED_SINCE, &now.to_string(), now)?;
        }
    }
    Ok(crate::settings::get(tx, SESSIONS_OWNED_SINCE)?.and_then(|j| j.trim().parse().ok()))
}

/// Whether session `id` (a run id, UUIDv7) was started from `since` on; ids
/// that carry no time are never ours once there is a `since`.
pub fn started_since(id: &str, since: Option<i64>) -> bool {
    let Some(since) = since else { return true };
    let hex: String = id.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 || hex.as_bytes()[12] != b'7' {
        return false;
    }
    i64::from_str_radix(&hex[..12], 16).is_ok_and(|ms| ms >= since)
}
