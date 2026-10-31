//! Agent functions, API keys and the calls made with them (see
//! `aurelius_domain::agent_function`). Keys keep only the SHA-256 of their
//! secret (the gateway computes it); the secret itself is never stored.

use crate::{StorageError, now_ms, settings};
use aurelius_domain::{
    AgentFunctionId, ApiKeyId, FunctionCallId, EmployeeId, ProjectId, TaskId,
    agent_function::{CallStatus, DAY_MS, FunctionFields, FunctionParam, WEEK_MS},
    autonomy::AgentMode,
    task::TaskStatus,
};
use aurelius_domain::views::{ApiKeyView, FunctionCallView, FunctionUsageView};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// Settings key of the kill switch (absent: functions are on).
const ALL_ENABLED: &str = "agent_functions/all_enabled";

/// What is kept of an API key's secret: the hex SHA-256 (the same as the
/// gateway's `auth::verifier`, which it applies to the key a caller presents).
pub fn secret_verifier(secret: &str) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(secret.as_bytes()))
}

fn parse<T, E: std::error::Error + Send + Sync + 'static>(r: Result<T, E>) -> rusqlite::Result<T> {
    r.map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

#[derive(Debug, Clone)]
pub struct FunctionRow {
    pub id: AgentFunctionId,
    pub project_id: ProjectId,
    pub fields: FunctionFields,
    pub enabled: bool,
    pub created_at_ms: i64,
}

const COLUMNS: &str = "id, project_id, employee_id, name, slug, description, instruction_template, params_json, mode, limit_per_day, limit_per_week, enabled, created_at_ms";

fn row_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<FunctionRow> {
    let params: String = r.get(7)?;
    let mode: Option<String> = r.get(8)?;
    Ok(FunctionRow {
        id: parse(AgentFunctionId::parse(&r.get::<_, String>(0)?))?,
        project_id: parse(ProjectId::parse(&r.get::<_, String>(1)?))?,
        fields: FunctionFields {
            employee_id: parse(EmployeeId::parse(&r.get::<_, String>(2)?))?,
            name: r.get(3)?,
            slug: r.get(4)?,
            description: r.get(5)?,
            instruction_template: r.get(6)?,
            params: serde_json::from_str::<Vec<FunctionParam>>(&params).unwrap_or_default(),
            mode: mode.as_deref().map(AgentMode::parse),
            limit_per_day: r.get::<_, Option<i64>>(9)?.map(|v| v as u32),
            limit_per_week: r.get::<_, Option<i64>>(10)?.map(|v| v as u32),
        },
        enabled: r.get::<_, i64>(11)? != 0,
        created_at_ms: r.get(12)?,
    })
}

pub fn insert_function(tx: &Transaction<'_>, id: AgentFunctionId, project: ProjectId, fields: &FunctionFields, enabled: bool) -> Result<(), StorageError> {
    let now = now_ms();
    tx.execute(
        "INSERT INTO agent_functions(id, project_id, employee_id, name, slug, description, instruction_template, params_json, mode, limit_per_day, limit_per_week,
                                     enabled, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
        params![
            id.to_string(),
            project.to_string(),
            fields.employee_id.to_string(),
            fields.name,
            fields.slug,
            fields.description,
            fields.instruction_template,
            serde_json::to_string(&fields.params)?,
            fields.mode.map(|m| m.as_str()),
            fields.limit_per_day,
            fields.limit_per_week,
            enabled as i64,
            now,
        ],
    )?;
    Ok(())
}

pub fn update_function(tx: &Transaction<'_>, id: AgentFunctionId, fields: &FunctionFields) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE agent_functions SET employee_id = ?2, name = ?3, slug = ?4, description = ?5, instruction_template = ?6, params_json = ?7, mode = ?8,
                limit_per_day = ?9, limit_per_week = ?10, updated_at_ms = ?11 WHERE id = ?1",
        params![
            id.to_string(),
            fields.employee_id.to_string(),
            fields.name,
            fields.slug,
            fields.description,
            fields.instruction_template,
            serde_json::to_string(&fields.params)?,
            fields.mode.map(|m| m.as_str()),
            fields.limit_per_day,
            fields.limit_per_week,
            now_ms(),
        ],
    )?;
    Ok(())
}

pub fn set_enabled(tx: &Transaction<'_>, id: AgentFunctionId, enabled: bool) -> Result<(), StorageError> {
    tx.execute("UPDATE agent_functions SET enabled = ?2, updated_at_ms = ?3 WHERE id = ?1", params![id.to_string(), enabled as i64, now_ms()])?;
    Ok(())
}

/// The function, its key links and its call records go; the tasks the calls made stay.
pub fn delete_function(tx: &Transaction<'_>, id: AgentFunctionId) -> Result<(), StorageError> {
    let id = id.to_string();
    tx.execute("DELETE FROM function_calls WHERE function_id = ?1", [&id])?;
    tx.execute("DELETE FROM api_key_functions WHERE function_id = ?1", [&id])?;
    tx.execute("DELETE FROM agent_functions WHERE id = ?1", [&id])?;
    Ok(())
}

pub fn load_function(conn: &Connection, id: AgentFunctionId) -> Result<Option<FunctionRow>, StorageError> {
    Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM agent_functions WHERE id = ?1"), [id.to_string()], row_of).optional()?)
}

pub fn function_by_slug(conn: &Connection, slug: &str) -> Result<Option<FunctionRow>, StorageError> {
    Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM agent_functions WHERE slug = ?1"), [slug], row_of).optional()?)
}

/// Another function (not `except`) already uses this slug.
pub fn slug_taken(conn: &Connection, slug: &str, except: Option<AgentFunctionId>) -> Result<bool, StorageError> {
    let other: Option<String> = conn
        .query_row("SELECT id FROM agent_functions WHERE slug = ?1 AND (?2 IS NULL OR id <> ?2)", params![slug, except.map(|e| e.to_string())], |r| r.get(0))
        .optional()?;
    Ok(other.is_some())
}

/// Functions with their employee's name, oldest first.
pub fn list_functions(conn: &Connection, project: Option<ProjectId>) -> Result<Vec<(FunctionRow, String)>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.project_id, f.employee_id, f.name, f.slug, f.description, f.instruction_template, f.params_json, f.mode, f.limit_per_day,
                f.limit_per_week, f.enabled, f.created_at_ms, COALESCE(m.name, '')
         FROM agent_functions f LEFT JOIN employees m ON m.id = f.employee_id
         WHERE (?1 IS NULL OR f.project_id = ?1) ORDER BY f.created_at_ms, f.id",
    )?;
    let rows = stmt.query_map([project.map(|p| p.to_string())], |r| Ok((row_of(r)?, r.get::<_, String>(13)?)))?;
    Ok(rows.collect::<Result<Vec<_>, rusqlite::Error>>()?)
}

/// Whether the kill switch leaves functions on (the default).
pub fn all_enabled(conn: &Connection) -> Result<bool, StorageError> {
    Ok(settings::get(conn, ALL_ENABLED)?.map(|v| v.trim() != "false").unwrap_or(true))
}

pub fn set_all_enabled(tx: &Transaction<'_>, enabled: bool) -> Result<(), StorageError> {
    settings::put(tx, ALL_ENABLED, if enabled { "true" } else { "false" }, now_ms())
}

// ----- keys -----

#[derive(Debug, Clone)]
pub struct KeyRow {
    pub id: ApiKeyId,
    pub name: String,
    pub created_at_ms: i64,
    pub last_used_ms: Option<i64>,
    pub revoked_at_ms: Option<i64>,
}

impl KeyRow {
    pub fn revoked(&self) -> bool {
        self.revoked_at_ms.is_some()
    }
}

fn key_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<KeyRow> {
    Ok(KeyRow {
        id: parse(ApiKeyId::parse(&r.get::<_, String>(0)?))?,
        name: r.get(1)?,
        created_at_ms: r.get(2)?,
        last_used_ms: r.get(3)?,
        revoked_at_ms: r.get(4)?,
    })
}

const KEY_COLUMNS: &str = "id, name, created_at_ms, last_used_ms, revoked_at_ms";

pub fn insert_key(tx: &Transaction<'_>, id: ApiKeyId, name: &str, verifier: &str) -> Result<(), StorageError> {
    tx.execute("INSERT INTO api_keys(id, name, key_verifier, created_at_ms) VALUES (?1, ?2, ?3, ?4)", params![id.to_string(), name, verifier, now_ms()])?;
    Ok(())
}

/// The key whose secret hashes to `verifier` (revoked ones too: the caller decides).
pub fn key_by_verifier(conn: &Connection, verifier: &str) -> Result<Option<KeyRow>, StorageError> {
    Ok(conn.query_row(&format!("SELECT {KEY_COLUMNS} FROM api_keys WHERE key_verifier = ?1"), [verifier], key_of).optional()?)
}

pub fn load_key(conn: &Connection, id: ApiKeyId) -> Result<Option<KeyRow>, StorageError> {
    Ok(conn.query_row(&format!("SELECT {KEY_COLUMNS} FROM api_keys WHERE id = ?1"), [id.to_string()], key_of).optional()?)
}

/// The board member on whose Claude login the key's calls run (B5); none: they wait with 'login'.
pub fn set_key_login_user(tx: &Transaction<'_>, id: ApiKeyId, login_user: Option<&str>) -> Result<(), StorageError> {
    tx.execute("UPDATE api_keys SET login_user = ?2 WHERE id = ?1", params![id.to_string(), login_user])?;
    Ok(())
}

pub fn revoke_key(tx: &Transaction<'_>, id: ApiKeyId) -> Result<(), StorageError> {
    tx.execute("UPDATE api_keys SET revoked_at_ms = COALESCE(revoked_at_ms, ?2) WHERE id = ?1", params![id.to_string(), now_ms()])?;
    Ok(())
}

pub fn touch_key(tx: &Transaction<'_>, id: ApiKeyId, now: i64) -> Result<(), StorageError> {
    tx.execute("UPDATE api_keys SET last_used_ms = ?2 WHERE id = ?1", params![id.to_string(), now])?;
    Ok(())
}

pub fn set_key_functions(tx: &Transaction<'_>, key: ApiKeyId, functions: &[AgentFunctionId]) -> Result<(), StorageError> {
    tx.execute("DELETE FROM api_key_functions WHERE api_key_id = ?1", [key.to_string()])?;
    for f in functions {
        tx.execute("INSERT OR IGNORE INTO api_key_functions(api_key_id, function_id) VALUES (?1, ?2)", params![key.to_string(), f.to_string()])?;
    }
    Ok(())
}

pub fn key_allows(conn: &Connection, key: ApiKeyId, function: AgentFunctionId) -> Result<bool, StorageError> {
    let found: Option<i64> =
        conn.query_row("SELECT 1 FROM api_key_functions WHERE api_key_id = ?1 AND function_id = ?2", params![key.to_string(), function.to_string()], |r| r.get(0)).optional()?;
    Ok(found.is_some())
}

pub fn list_keys(conn: &Connection) -> Result<Vec<ApiKeyView>, StorageError> {
    let mut stmt = conn.prepare(&format!("SELECT {KEY_COLUMNS} FROM api_keys ORDER BY created_at_ms DESC, id"))?;
    let keys = stmt.query_map([], key_of)?.collect::<Result<Vec<KeyRow>, rusqlite::Error>>()?;
    let mut links = conn.prepare("SELECT function_id FROM api_key_functions WHERE api_key_id = ?1 ORDER BY function_id")?;
    let mut out = Vec::new();
    for k in keys {
        let function_ids = links
            .query_map([k.id.to_string()], |r| r.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .filter_map(|f| AgentFunctionId::parse(&f).ok())
            .collect();
        out.push(ApiKeyView { id: k.id, name: k.name, function_ids, created_at_ms: k.created_at_ms, last_used_ms: k.last_used_ms, revoked_at_ms: k.revoked_at_ms });
    }
    Ok(out)
}

// ----- calls -----

#[derive(Debug, Clone)]
pub struct CallRow {
    pub id: FunctionCallId,
    pub function_id: AgentFunctionId,
    pub api_key_id: ApiKeyId,
    pub task_id: TaskId,
    pub created_at_ms: i64,
}

pub fn insert_call(tx: &Transaction<'_>, call: &CallRow) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO function_calls(id, function_id, api_key_id, task_id, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![call.id.to_string(), call.function_id.to_string(), call.api_key_id.to_string(), call.task_id.to_string(), call.created_at_ms],
    )?;
    Ok(())
}

pub fn load_call(conn: &Connection, id: FunctionCallId) -> Result<Option<CallRow>, StorageError> {
    Ok(conn
        .query_row("SELECT id, function_id, api_key_id, task_id, created_at_ms FROM function_calls WHERE id = ?1", [id.to_string()], |r| {
            Ok(CallRow {
                id: parse(FunctionCallId::parse(&r.get::<_, String>(0)?))?,
                function_id: parse(AgentFunctionId::parse(&r.get::<_, String>(1)?))?,
                api_key_id: parse(ApiKeyId::parse(&r.get::<_, String>(2)?))?,
                task_id: parse(TaskId::parse(&r.get::<_, String>(3)?))?,
                created_at_ms: r.get(4)?,
            })
        })
        .optional()?)
}

/// Accepted calls of a function since `since_ms` (exclusive), as their times.
pub fn call_times(conn: &Connection, function: AgentFunctionId, since_ms: i64) -> Result<Vec<i64>, StorageError> {
    let mut stmt = conn.prepare("SELECT created_at_ms FROM function_calls WHERE function_id = ?1 AND created_at_ms > ?2 ORDER BY created_at_ms")?;
    let times = stmt.query_map(params![function.to_string(), since_ms], |r| r.get(0))?.collect::<Result<Vec<_>, rusqlite::Error>>()?;
    Ok(times)
}

/// A call's task: its status, when it last changed and its report key.
pub fn task_state(conn: &Connection, task: TaskId) -> Result<Option<(TaskStatus, i64, Option<String>)>, StorageError> {
    let row: Option<(String, i64, Option<String>)> = conn
        .query_row(
            "SELECT t.status, t.updated_at_ms, rb.storage_key FROM tasks t LEFT JOIN blobs rb ON rb.id = t.result_blob_id WHERE t.id = ?1",
            [task.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    Ok(row.map(|(status, updated, key)| (TaskStatus::parse(&status).unwrap_or(TaskStatus::Failed), updated, key)))
}

/// The function whose call made this task.
pub fn function_of_task(conn: &Connection, task: TaskId) -> Result<Option<AgentFunctionId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT function_id FROM function_calls WHERE task_id = ?1", [task.to_string()], |r| r.get(0)).optional()?;
    Ok(id.and_then(|i| AgentFunctionId::parse(&i).ok()))
}

/// Recent calls, newest first (`function`: only that function's).
pub fn recent_calls(conn: &Connection, function: Option<AgentFunctionId>, limit: u32) -> Result<Vec<FunctionCallView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.function_id, f.name, c.api_key_id, k.name, c.task_id, c.created_at_ms, t.status, t.updated_at_ms
         FROM function_calls c JOIN agent_functions f ON f.id = c.function_id JOIN api_keys k ON k.id = c.api_key_id JOIN tasks t ON t.id = c.task_id
         WHERE (?1 IS NULL OR c.function_id = ?1) ORDER BY c.created_at_ms DESC, c.id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![function.map(|f| f.to_string()), limit], |r| {
        let status = CallStatus::from_task(TaskStatus::parse(&r.get::<_, String>(7)?).unwrap_or(TaskStatus::Failed));
        let updated: i64 = r.get(8)?;
        Ok(FunctionCallView {
            id: parse(FunctionCallId::parse(&r.get::<_, String>(0)?))?,
            function_id: parse(AgentFunctionId::parse(&r.get::<_, String>(1)?))?,
            function_name: r.get(2)?,
            api_key_id: parse(ApiKeyId::parse(&r.get::<_, String>(3)?))?,
            api_key_name: r.get(4)?,
            task_id: parse(TaskId::parse(&r.get::<_, String>(5)?))?,
            status,
            created_at_ms: r.get(6)?,
            finished_at_ms: status.is_done().then_some(updated),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, rusqlite::Error>>()?)
}

/// Accepted calls per function in the rolling 24 h and 7 days.
pub fn usage_of(conn: &Connection, now: i64) -> Result<Vec<FunctionUsageView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.limit_per_day, f.limit_per_week,
                (SELECT COUNT(*) FROM function_calls c WHERE c.function_id = f.id AND c.created_at_ms > ?1),
                (SELECT COUNT(*) FROM function_calls c WHERE c.function_id = f.id AND c.created_at_ms > ?2)
         FROM agent_functions f ORDER BY f.created_at_ms, f.id",
    )?;
    let rows = stmt.query_map(params![now - DAY_MS, now - WEEK_MS], |r| {
        Ok(FunctionUsageView {
            function_id: parse(AgentFunctionId::parse(&r.get::<_, String>(0)?))?,
            limit_per_day: r.get::<_, Option<i64>>(1)?.map(|v| v as u32),
            limit_per_week: r.get::<_, Option<i64>>(2)?.map(|v| v as u32),
            used_day: r.get::<_, i64>(3)? as u32,
            used_week: r.get::<_, i64>(4)? as u32,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, rusqlite::Error>>()?)
}
