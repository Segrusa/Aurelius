//! Append-only event log, command receipts and projection writes.
//! All functions take the caller's transaction so a command commits atomically.

use crate::{StorageError, now_ms};
use aurelius_domain::{ActorKind, EventId, EventKind, ProjectId};
use rusqlite::{OptionalExtension, Transaction, params};

pub struct NewEvent<'a> {
    pub aggregate_type: &'a str,
    pub aggregate_id: String,
    pub aggregate_version: u64,
    pub event_type: &'a str,
    pub project_id: Option<ProjectId>,
    pub actor_kind: ActorKind,
    pub actor_id: Option<String>,
    pub correlation_id: String,
    pub causation_id: Option<EventId>,
    pub payload_blob_id: Option<String>,
    pub metadata: serde_json::Value,
    pub dedupe_key: String,
}

pub struct Appended {
    pub id: EventId,
    pub seq: u64,
}

/// Append one event. Versions per aggregate must be gap-free.
pub fn append(tx: &Transaction<'_>, event: NewEvent<'_>) -> Result<Appended, StorageError> {
    if EventKind::parse(event.event_type).is_none() {
        return Err(StorageError::Rejected(format!("unknown event type {}", event.event_type)));
    }
    let current: u64 = tx.query_row(
        "SELECT COALESCE(MAX(aggregate_version), 0) FROM events WHERE aggregate_type = ?1 AND aggregate_id = ?2",
        params![event.aggregate_type, event.aggregate_id],
        |row| row.get::<_, i64>(0),
    )? as u64;
    if event.aggregate_version != current + 1 {
        return Err(StorageError::AggregateVersion {
            aggregate_type: event.aggregate_type.into(),
            aggregate_id: event.aggregate_id,
            expected: current + 1,
            actual: current,
        });
    }

    let id = EventId::new();
    let now = now_ms();
    tx.execute(
        "INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version,
             project_id, actor_kind, actor_id, correlation_id, causation_id, payload_blob_id,
             metadata_json, occurred_at_ms, recorded_at_ms, dedupe_key)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13, ?14)",
        params![
            id.to_string(),
            event.aggregate_type,
            event.aggregate_id,
            event.aggregate_version as i64,
            event.event_type,
            event.project_id.map(|p| p.to_string()),
            event.actor_kind.as_str(),
            event.actor_id,
            event.correlation_id,
            event.causation_id.map(|c| c.to_string()),
            event.payload_blob_id,
            serde_json::to_string(&event.metadata)?,
            now,
            event.dedupe_key,
        ],
    )?;
    let seq = tx.last_insert_rowid() as u64;
    crate::activity_log::from_event(tx, event.aggregate_type, &event.aggregate_id, event.event_type, &event.correlation_id, &event.metadata);
    if crate::timeline::notifies(event.event_type, &event.metadata) {
        crate::timeline::notify(tx, seq)?;
    }
    Ok(Appended { id, seq })
}

pub struct StoredReceipt {
    pub request_hash: String,
    pub method: String,
    pub result_json: String,
    pub committed_seq: u64,
}

pub fn find_receipt(conn: &rusqlite::Connection, command_id: &str) -> Result<Option<StoredReceipt>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT request_hash, result_json, committed_seq, method FROM command_receipts WHERE command_id = ?1",
            [command_id],
            |row| {
                Ok(StoredReceipt {
                    request_hash: row.get(0)?,
                    result_json: row.get(1)?,
                    committed_seq: row.get::<_, i64>(2)? as u64,
                    method: row.get(3)?,
                })
            },
        )
        .optional()?)
}

pub fn insert_receipt(
    tx: &Transaction<'_>,
    command_id: &str,
    request_hash: &str,
    method: &str,
    result_json: &str,
    committed_seq: u64,
) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO command_receipts(command_id, request_hash, method, result_json, committed_seq, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![command_id, request_hash, method, result_json, committed_seq as i64, now_ms()],
    )?;
    Ok(())
}

/// The implicit single-user company workspace scope.
pub fn ensure_global_scope(tx: &Transaction<'_>) -> Result<String, StorageError> {
    if let Some(id) = tx
        .query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |row| row.get::<_, String>(0))
        .optional()?
    {
        return Ok(id);
    }
    let id = aurelius_domain::ScopeId::new().to_string();
    tx.execute("INSERT INTO scopes(id, kind) VALUES (?1, 'global')", [&id])?;
    Ok(id)
}

pub fn insert_project_projection(
    tx: &Transaction<'_>,
    project_id: ProjectId,
    name: &str,
    description_blob_id: Option<&str>,
) -> Result<(), StorageError> {
    let global = ensure_global_scope(tx)?;
    let scope_id = aurelius_domain::ScopeId::new().to_string();
    let pid = project_id.to_string();
    tx.execute(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'project', ?2, ?3)",
        params![scope_id, global, pid],
    )?;
    let now = now_ms();
    tx.execute(
        "INSERT INTO projects(id, scope_id, name, description_blob_id, status, version, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, 'active', 1, ?5, ?5)",
        params![pid, scope_id, name, description_blob_id, now],
    )?;
    // Its channel in the Team chat, in the same transaction (migration 19).
    crate::discussions::insert_project_channel(tx, project_id, name)?;
    Ok(())
}

pub fn rename_project_projection(
    tx: &Transaction<'_>,
    project_id: ProjectId,
    name: &str,
    new_version: u64,
) -> Result<(), StorageError> {
    let changed = tx.execute(
        "UPDATE projects SET name = ?2, version = ?3, updated_at_ms = ?4 WHERE id = ?1 AND version = ?3 - 1",
        params![project_id.to_string(), name, new_version as i64, now_ms()],
    )?;
    if changed != 1 {
        return Err(StorageError::Rejected("project projection is out of date".into()));
    }
    crate::discussions::rename_project_channel(tx, project_id, name)
}

/// The description lives in a blob; `None` clears it.
pub fn set_project_description_projection(
    tx: &Transaction<'_>,
    project_id: ProjectId,
    description_blob_id: Option<&str>,
    new_version: u64,
) -> Result<(), StorageError> {
    let changed = tx.execute(
        "UPDATE projects SET description_blob_id = ?2, version = ?3, updated_at_ms = ?4 WHERE id = ?1 AND version = ?3 - 1",
        params![project_id.to_string(), description_blob_id, new_version as i64, now_ms()],
    )?;
    if changed != 1 {
        return Err(StorageError::Rejected("project projection is out of date".into()));
    }
    Ok(())
}

pub fn insert_job(
    tx: &Transaction<'_>,
    job_id: &str,
    kind: &str,
    source_event_id: EventId,
    dedupe_key: &str,
    payload: &serde_json::Value,
) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO jobs(id, kind, subject_id, source_event_id, dedupe_key, payload_json, state, not_before_ms)
         VALUES (?1, ?2, ?1, ?3, ?4, ?5, 'ready', ?6)",
        params![job_id, kind, source_event_id.to_string(), dedupe_key, serde_json::to_string(payload)?, now_ms()],
    )?;
    Ok(())
}
