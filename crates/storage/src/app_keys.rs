//! App keys (see `aurelius_domain::app_key`) and what they count: the tasks
//! each app created. Keys keep only the SHA-256 of their secret
//! (`agent_functions::secret_verifier`); the secret itself is never stored.

use crate::{StorageError, now_ms};
use aurelius_domain::AppKeyId;
use aurelius_domain::app_key::{AppAudience, AppKeyView, AppScope, CreateAppKey, HOUR_MS};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// Task statuses that count as open for an app's limit (drafts and ended tasks do not).
const OPEN: &str = "('queued','running','waiting','review')";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppKeyRow {
    pub id: AppKeyId,
    pub name: String,
    pub scopes: Vec<AppScope>,
    pub limit_per_hour: Option<u32>,
    pub limit_concurrent: Option<u32>,
    pub created_at_ms: i64,
    pub last_used_ms: Option<i64>,
    pub revoked_at_ms: Option<i64>,
    pub audience: AppAudience,
    pub webhook_url: Option<String>,
    /// Needed to sign deliveries; never part of a view.
    pub webhook_secret: Option<String>,
}

impl AppKeyRow {
    pub fn revoked(&self) -> bool {
        self.revoked_at_ms.is_some()
    }

    pub fn grants(&self, scope: AppScope) -> bool {
        aurelius_domain::app_key::grants(&self.scopes, scope)
    }
}

const COLUMNS: &str = "id, name, scopes_json, limit_per_hour, limit_concurrent, created_at_ms, last_used_ms, revoked_at_ms, audience, webhook_url, webhook_secret";

fn row_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<AppKeyRow> {
    let id: String = r.get(0)?;
    let scopes: String = r.get(2)?;
    Ok(AppKeyRow {
        id: AppKeyId::parse(&id).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?,
        name: r.get(1)?,
        // A scope this build does not know grants nothing.
        scopes: serde_json::from_str::<Vec<String>>(&scopes).unwrap_or_default().iter().filter_map(|s| AppScope::parse(s)).collect(),
        limit_per_hour: r.get::<_, Option<i64>>(3)?.map(|v| v as u32),
        limit_concurrent: r.get::<_, Option<i64>>(4)?.map(|v| v as u32),
        created_at_ms: r.get(5)?,
        last_used_ms: r.get(6)?,
        revoked_at_ms: r.get(7)?,
        audience: AppAudience::parse(&r.get::<_, String>(8)?).unwrap_or_default(),
        webhook_url: r.get(9)?,
        webhook_secret: r.get(10)?,
    })
}

/// `key` must be validated (`app_key::validate`). `webhook_secret` goes with its `webhook_url`.
pub fn insert(tx: &Transaction<'_>, id: AppKeyId, key: &CreateAppKey, verifier: &str, webhook_secret: Option<&str>) -> Result<(), StorageError> {
    let scopes: Vec<&str> = key.scopes.iter().map(|s| s.as_str()).collect();
    tx.execute(
        "INSERT INTO app_keys(id, name, key_verifier, scopes_json, limit_per_hour, limit_concurrent, created_at_ms, audience, webhook_url, webhook_secret) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![id.to_string(), key.name, verifier, serde_json::to_string(&scopes)?, key.limit_per_hour, key.limit_concurrent, now_ms(), key.audience.as_str(), key.webhook_url, webhook_secret],
    )?;
    Ok(())
}

pub fn load(conn: &Connection, id: AppKeyId) -> Result<Option<AppKeyRow>, StorageError> {
    Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM app_keys WHERE id = ?1"), [id.to_string()], row_of).optional()?)
}

/// The key whose secret hashes to `verifier` (revoked ones too: the caller decides).
pub fn by_verifier(conn: &Connection, verifier: &str) -> Result<Option<AppKeyRow>, StorageError> {
    Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM app_keys WHERE key_verifier = ?1"), [verifier], row_of).optional()?)
}

pub fn revoke(tx: &Transaction<'_>, id: AppKeyId) -> Result<(), StorageError> {
    tx.execute("UPDATE app_keys SET revoked_at_ms = COALESCE(revoked_at_ms, ?2) WHERE id = ?1", params![id.to_string(), now_ms()])?;
    Ok(())
}

pub fn touch(tx: &Transaction<'_>, id: AppKeyId, now: i64) -> Result<(), StorageError> {
    tx.execute("UPDATE app_keys SET last_used_ms = ?2 WHERE id = ?1", params![id.to_string(), now])?;
    Ok(())
}

/// Tasks the app created after `since_ms`, and its open tasks now.
pub fn task_counts(conn: &Connection, id: AppKeyId, since_ms: i64) -> Result<(u32, u32), StorageError> {
    let id = id.to_string();
    let recent: i64 = conn.query_row("SELECT COUNT(*) FROM tasks WHERE created_by_app_key_id = ?1 AND created_at_ms > ?2", params![id, since_ms], |r| r.get(0))?;
    let open: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM tasks WHERE created_by_app_key_id = ?1 AND status IN {OPEN}"), [&id], |r| r.get(0))?;
    Ok((recent as u32, open as u32))
}

/// Record which app created a task.
pub fn set_task_app(tx: &Transaction<'_>, task: aurelius_domain::TaskId, id: AppKeyId) -> Result<(), StorageError> {
    tx.execute("UPDATE tasks SET created_by_app_key_id = ?2 WHERE id = ?1", params![task.to_string(), id.to_string()])?;
    Ok(())
}

pub fn view(conn: &Connection, row: AppKeyRow, now: i64) -> Result<AppKeyView, StorageError> {
    let (tasks_last_hour, tasks_open) = task_counts(conn, row.id, now - HOUR_MS)?;
    Ok(AppKeyView {
        id: row.id,
        name: row.name,
        scopes: row.scopes,
        limit_per_hour: row.limit_per_hour,
        limit_concurrent: row.limit_concurrent,
        created_at_ms: row.created_at_ms,
        last_used_ms: row.last_used_ms,
        revoked_at_ms: row.revoked_at_ms,
        tasks_last_hour,
        tasks_open,
        audience: row.audience,
        webhook_url: row.webhook_url,
    })
}

/// Every key, newest first.
pub fn list(conn: &Connection) -> Result<Vec<AppKeyView>, StorageError> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM app_keys ORDER BY created_at_ms DESC, id"))?;
    let rows = stmt.query_map([], row_of)?.collect::<Result<Vec<_>, rusqlite::Error>>()?;
    let now = now_ms();
    rows.into_iter().map(|row| view(conn, row, now)).collect()
}

/// An employee's own mode (that of its main conversation), which its new task conversations start in.
pub fn employee_mode(conn: &Connection, employee: aurelius_domain::EmployeeId) -> Result<Option<aurelius_domain::autonomy::AgentMode>, StorageError> {
    let mode: Option<String> = conn
        .query_row(
            "SELECT agent_mode FROM conversations WHERE employee_id = ?1 AND kind = 'employee' ORDER BY created_at_ms, id LIMIT 1",
            [employee.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(mode.map(|m| aurelius_domain::autonomy::AgentMode::parse(&m)))
}

/// Whether a conversation belongs to a project (a task's or the project
/// assistant's), not an employee's chat: apps reach only those.
pub fn project_conversation(conn: &Connection, conversation: aurelius_domain::ConversationId) -> Result<bool, StorageError> {
    let found: Option<i64> = conn
        .query_row("SELECT 1 FROM conversations WHERE id = ?1 AND project_id IS NOT NULL", [conversation.to_string()], |r| r.get(0))
        .optional()?;
    Ok(found.is_some())
}

// ---- the management channel (feature `management_channel`) ----

/// One member of the management: the CEO or a direct report of the CEO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagementMember {
    pub id: aurelius_domain::EmployeeId,
    pub name: String,
    pub role: String,
    pub title: Option<String>,
    pub is_ceo: bool,
}

/// The management of the company now: the CEO (the root of the chart whose role or title
/// names the CEO, else the oldest root) first, then the CEO's direct reports. Archived
/// employees are not on the chart. Empty when the company has no employees.
pub fn management(conn: &Connection) -> Result<Vec<ManagementMember>, StorageError> {
    use aurelius_domain::EmployeeId;
    let org = crate::employees::org(conn)?;
    let mut stmt = conn.prepare("SELECT id, name, role, title FROM employees WHERE status <> 'archived' ORDER BY created_at_ms, id")?;
    let all = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?)))?
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;
    let all: Vec<(EmployeeId, String, String, Option<String>)> = all.into_iter().filter_map(|(id, name, role, title)| Some((EmployeeId::parse(&id).ok()?, name, role, title))).collect();
    let roots: Vec<(EmployeeId, String, Option<String>)> = all.iter().filter(|(id, ..)| org.boss(*id).is_none()).map(|(id, _, role, title)| (*id, role.clone(), title.clone())).collect();
    let Some(ceo) = aurelius_domain::org::pick_ceo(&roots) else { return Ok(Vec::new()) };
    Ok(org
        .management(ceo)
        .into_iter()
        .filter_map(|id| all.iter().find(|(e, ..)| *e == id).map(|(e, name, role, title)| ManagementMember { id: *e, name: name.clone(), role: role.clone(), title: title.clone(), is_ceo: *e == ceo }))
        .collect())
}

/// The employee a task is for (none: a draft).
pub fn task_employee(conn: &Connection, task: aurelius_domain::TaskId) -> Result<Option<aurelius_domain::EmployeeId>, StorageError> {
    let id: Option<Option<String>> = conn.query_row("SELECT employee_id FROM tasks WHERE id = ?1", [task.to_string()], |r| r.get(0)).optional()?;
    Ok(id.flatten().and_then(|e| aurelius_domain::EmployeeId::parse(&e).ok()))
}

/// What a conversation is, for the audience check: the employee it belongs to
/// (none for a project conversation, the project assistant's) and whether it is
/// that employee's persistent chat (not one of its tasks').
pub fn conversation_owner(conn: &Connection, conversation: aurelius_domain::ConversationId) -> Result<Option<(Option<aurelius_domain::EmployeeId>, bool)>, StorageError> {
    let row: Option<(Option<String>, String)> =
        conn.query_row("SELECT employee_id, kind FROM conversations WHERE id = ?1", [conversation.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    Ok(row.map(|(employee, kind)| (employee.and_then(|e| aurelius_domain::EmployeeId::parse(&e).ok()), kind == "employee")))
}

/// Remember that `key` wrote into `conversation` (the persistent chat of `employee`):
/// it may read it and gets the replies. `awaiting` until a reply was delivered.
pub fn note_thread(tx: &Transaction<'_>, key: AppKeyId, conversation: aurelius_domain::ConversationId, employee: aurelius_domain::EmployeeId) -> Result<(), StorageError> {
    let now = now_ms();
    tx.execute(
        "INSERT INTO app_key_threads(key_id, conversation_id, employee_id, awaiting, created_at_ms, last_sent_ms) VALUES (?1, ?2, ?3, 1, ?4, ?4)
         ON CONFLICT(key_id, conversation_id) DO UPDATE SET awaiting = 1, last_sent_ms = ?4",
        params![key.to_string(), conversation.to_string(), employee.to_string(), now],
    )?;
    Ok(())
}

/// Whether `key` wrote into `conversation`.
pub fn has_thread(conn: &Connection, key: AppKeyId, conversation: aurelius_domain::ConversationId) -> Result<bool, StorageError> {
    let found: Option<i64> = conn.query_row("SELECT 1 FROM app_key_threads WHERE key_id = ?1 AND conversation_id = ?2", params![key.to_string(), conversation.to_string()], |r| r.get(0)).optional()?;
    Ok(found.is_some())
}

/// The keys waiting for a reply in `conversation`.
pub fn awaiting_keys(conn: &Connection, conversation: &str) -> Result<Vec<AppKeyId>, StorageError> {
    let mut stmt = conn.prepare("SELECT key_id FROM app_key_threads WHERE conversation_id = ?1 AND awaiting = 1 ORDER BY last_sent_ms")?;
    let rows = stmt.query_map([conversation], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, rusqlite::Error>>()?;
    Ok(rows.into_iter().filter_map(|k| AppKeyId::parse(&k).ok()).collect())
}

/// A reply was queued for `key`: it waits for the next message again.
pub fn clear_awaiting(tx: &Transaction<'_>, key: AppKeyId, conversation: &str) -> Result<(), StorageError> {
    tx.execute("UPDATE app_key_threads SET awaiting = 0 WHERE key_id = ?1 AND conversation_id = ?2", params![key.to_string(), conversation])?;
    Ok(())
}

/// Keys that post events: not revoked, with a webhook.
pub fn webhook_keys(conn: &Connection) -> Result<Vec<AppKeyRow>, StorageError> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM app_keys WHERE webhook_url IS NOT NULL AND webhook_secret IS NOT NULL AND revoked_at_ms IS NULL ORDER BY created_at_ms, id"))?;
    Ok(stmt.query_map([], row_of)?.collect::<Result<Vec<_>, rusqlite::Error>>()?)
}
