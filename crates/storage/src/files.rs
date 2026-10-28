//! Attachments, fetched sources and folder grants.

use crate::{StorageError, now_ms};
use aurelius_domain::{AttachmentId, ConversationId, EventId, GrantId, RunId};
use aurelius_domain::views::{AttachmentView, SourceView};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

fn parse<T, E: std::error::Error + Send + Sync + 'static>(r: Result<T, E>) -> rusqlite::Result<T> {
    r.map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

#[allow(clippy::too_many_arguments)]
pub fn insert_attachment(
    tx: &Transaction<'_>,
    id: AttachmentId,
    conversation: ConversationId,
    name: &str,
    kind: &str,
    original_blob: &str,
    text_blob: &str,
    byte_length: u64,
    truncated: bool,
    source_event: EventId,
) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO attachments(id, conversation_id, name, media_kind, original_blob_id, text_blob_id, byte_length, truncated, source_event_id, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![id.to_string(), conversation.to_string(), name, kind, original_blob, text_blob, byte_length as i64, truncated, source_event.to_string(), now_ms()],
    )?;
    Ok(())
}

pub fn attachment_conversation(tx: &Transaction<'_>, id: AttachmentId) -> Result<Option<ConversationId>, StorageError> {
    Ok(tx
        .query_row("SELECT conversation_id FROM attachments WHERE id = ?1 AND removed_at_ms IS NULL", [id.to_string()], |r| r.get::<_, String>(0))
        .optional()?
        .and_then(|c| ConversationId::parse(&c).ok()))
}

pub fn remove_attachment(tx: &Transaction<'_>, id: AttachmentId) -> Result<(), StorageError> {
    tx.execute("UPDATE attachments SET removed_at_ms = ?2 WHERE id = ?1", params![id.to_string(), now_ms()])?;
    Ok(())
}

pub fn list_attachments(conn: &Connection, conversation: ConversationId) -> Result<Vec<AttachmentView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, media_kind, byte_length, truncated FROM attachments
         WHERE conversation_id = ?1 AND removed_at_ms IS NULL ORDER BY created_at_ms",
    )?;
    let rows = stmt.query_map([conversation.to_string()], |r| {
        Ok(AttachmentView {
            id: parse(AttachmentId::parse(&r.get::<_, String>(0)?))?,
            name: r.get(1)?,
            kind: r.get(2)?,
            byte_length: r.get::<_, i64>(3)? as u64,
            truncated: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// (name, storage key of extracted text) for context building.
pub fn attachment_texts(conn: &Connection, conversation: ConversationId) -> Result<Vec<(String, String)>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT a.name, b.storage_key FROM attachments a JOIN blobs b ON b.id = a.text_blob_id
         WHERE a.conversation_id = ?1 AND a.removed_at_ms IS NULL ORDER BY a.created_at_ms",
    )?;
    let rows = stmt.query_map([conversation.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[allow(clippy::too_many_arguments)]
pub fn insert_source(
    tx: &Transaction<'_>,
    run: RunId,
    ordinal: u32,
    url: &str,
    title: Option<&str>,
    sha256: &str,
    text_blob: Option<&str>,
    error: Option<&str>,
) -> Result<(), StorageError> {
    tx.execute(
        "INSERT OR REPLACE INTO message_sources(run_id, ordinal, url, title, sha256, text_blob_id, error, fetched_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![run.to_string(), ordinal, url, title, sha256, text_blob, error, now_ms()],
    )?;
    Ok(())
}

pub fn sources_for_run(conn: &Connection, run: RunId) -> Result<Vec<SourceView>, StorageError> {
    let mut stmt = conn.prepare("SELECT ordinal, url, title, error FROM message_sources WHERE run_id = ?1 ORDER BY ordinal")?;
    let rows = stmt.query_map([run.to_string()], |r| Ok(SourceView { n: r.get::<_, i64>(0)? as u32, url: r.get(1)?, title: r.get(2)?, error: r.get(3)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ── Folder grants (global scope in v0.1) ────────────────────────────────

pub fn insert_grant(tx: &Transaction<'_>, id: GrantId, scope_id: &str, canonical_path: &str, access: &str, source_event: EventId) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO file_grants(id, scope_id, canonical_path, platform_identity_json, access, recursive, source_event_id)
         VALUES (?1, ?2, ?3, '{}', ?4, 1, ?5)",
        params![id.to_string(), scope_id, canonical_path, access, source_event.to_string()],
    )?;
    Ok(())
}

pub fn employee_folder(conn: &Connection, employee: aurelius_domain::EmployeeId) -> Result<Option<String>, StorageError> {
    Ok(conn.query_row("SELECT folder FROM employees WHERE id = ?1", [employee.to_string()], |r| r.get::<_, Option<String>>(0)).optional()?.flatten())
}

/// Set (or clear) an employee's own workspace and its folder grant.
pub fn set_employee_folder(tx: &Transaction<'_>, employee: aurelius_domain::EmployeeId, folder: Option<&str>, grant: GrantId, source_event: EventId) -> Result<(), StorageError> {
    let scope: String = tx.query_row("SELECT scope_id FROM employees WHERE id = ?1", [employee.to_string()], |r| r.get(0))?;
    tx.execute("UPDATE employees SET folder = ?2 WHERE id = ?1", params![employee.to_string(), folder])?;
    tx.execute("UPDATE file_grants SET revoked_at_ms = ?2 WHERE scope_id = ?1 AND revoked_at_ms IS NULL", params![scope, now_ms()])?;
    if let Some(folder) = folder {
        insert_grant(tx, grant, &scope, folder, "stage_write", source_event)?;
    }
    Ok(())
}

pub fn project_folder(conn: &Connection, project: aurelius_domain::ProjectId) -> Result<Option<String>, StorageError> {
    Ok(conn.query_row("SELECT folder FROM projects WHERE id = ?1", [project.to_string()], |r| r.get::<_, Option<String>>(0)).optional()?.flatten())
}

/// Store a project's folder and make it the project's (only) folder grant.
pub fn set_project_folder(tx: &Transaction<'_>, project: aurelius_domain::ProjectId, folder: &str, grant: GrantId, source_event: EventId) -> Result<(), StorageError> {
    let scope: String = tx.query_row("SELECT scope_id FROM projects WHERE id = ?1", [project.to_string()], |r| r.get(0))?;
    tx.execute("UPDATE projects SET folder = ?2 WHERE id = ?1", params![project.to_string(), folder])?;
    tx.execute("UPDATE file_grants SET revoked_at_ms = ?2 WHERE scope_id = ?1 AND revoked_at_ms IS NULL", params![scope, now_ms()])?;
    insert_grant(tx, grant, &scope, folder, "stage_write", source_event)
}

pub fn projects_without_folder(conn: &Connection) -> Result<Vec<(aurelius_domain::ProjectId, String)>, StorageError> {
    let mut stmt = conn.prepare("SELECT id, name FROM projects WHERE folder IS NULL AND status <> 'archived'")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.filter_map(Result::ok).filter_map(|(id, name)| Some((aurelius_domain::ProjectId::parse(&id).ok()?, name))).collect())
}

pub fn folder_taken(conn: &Connection, folder: &str) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM projects WHERE folder = ?1)", [folder], |r| r.get(0))?)
}
