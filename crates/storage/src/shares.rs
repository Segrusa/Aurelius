//! Shares: one snapshot, one destination (a scope or a chat), revocable.

use crate::{StorageError, now_ms};
use aurelius_domain::{ConversationId, EventId, MemoryId, ProjectId, ShareId, share::ShareMode};
use aurelius_domain::views::ShareView;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub struct NewShare<'a> {
    pub id: ShareId,
    pub source_event: EventId,
    pub grant_event: EventId,
    pub scope_id: Option<&'a str>,
    pub conversation: Option<ConversationId>,
    pub blob_id: &'a str,
    pub mode: ShareMode,
    pub title: &'a str,
    pub source_label: &'a str,
    pub memory_id: Option<MemoryId>,
}

pub fn insert(tx: &Transaction<'_>, s: NewShare<'_>) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO shares(id, source_event_id, destination_scope_id, destination_conversation_id, snapshot_blob_id, grant_event_id,
             mode, title, source_label, memory_id, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            s.id.to_string(),
            s.source_event.to_string(),
            s.scope_id,
            s.conversation.map(|c| c.to_string()),
            s.blob_id,
            s.grant_event.to_string(),
            s.mode.as_str(),
            s.title,
            s.source_label,
            s.memory_id.map(|m| m.to_string()),
            now_ms(),
        ],
    )?;
    Ok(())
}

/// Revoke; returns the memory made from it (if any) so it can be forgotten.
pub fn revoke(tx: &Transaction<'_>, id: ShareId) -> Result<Option<Option<MemoryId>>, StorageError> {
    let row: Option<Option<String>> = tx
        .query_row("SELECT memory_id FROM shares WHERE id = ?1 AND revoked_at_ms IS NULL", [id.to_string()], |r| r.get(0))
        .optional()?;
    let Some(memory) = row else { return Ok(None) };
    tx.execute("UPDATE shares SET revoked_at_ms = ?2 WHERE id = ?1", params![id.to_string(), now_ms()])?;
    Ok(Some(memory.and_then(|m| MemoryId::parse(&m).ok())))
}

/// Active shares into these scopes or this chat: (id, title, source, blob key).
pub fn active(conn: &Connection, scopes: &[String], conversation: Option<ConversationId>) -> Result<Vec<(ShareId, String, String, String)>, StorageError> {
    let list = scopes.iter().map(|s| format!("'{}'", s.replace('\'', ""))).collect::<Vec<_>>().join(",");
    let scope_clause = if list.is_empty() { "0".to_string() } else { format!("s.destination_scope_id IN ({list})") };
    let sql = format!(
        "SELECT s.id, s.title, s.source_label, b.storage_key FROM shares s JOIN blobs b ON b.id = s.snapshot_blob_id
         WHERE s.revoked_at_ms IS NULL AND ({scope_clause} OR s.destination_conversation_id = ?1)
         ORDER BY s.created_at_ms"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([conversation.map(|c| c.to_string())], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
    })?;
    Ok(rows.filter_map(Result::ok).filter_map(|(id, t, s, k)| Some((ShareId::parse(&id).ok()?, t, s, k))).collect())
}

fn view(conn: &Connection, where_clause: &str, arg: &str, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<ShareView>, StorageError> {
    let sql = format!(
        "SELECT s.id, s.title, s.source_label, s.mode, s.created_at_ms, s.revoked_at_ms, b.storage_key,
                CASE WHEN s.destination_conversation_id IS NOT NULL THEN 'conversation'
                     WHEN sc.kind = 'project' THEN 'project · ' || (SELECT name FROM projects WHERE scope_id = sc.id)
                     WHEN sc.kind = 'department' THEN 'department · ' || (SELECT name FROM departments WHERE scope_id = sc.id)
                     ELSE 'employee · ' || (SELECT name FROM employees WHERE scope_id = sc.id) END
         FROM shares s JOIN blobs b ON b.id = s.snapshot_blob_id LEFT JOIN scopes sc ON sc.id = s.destination_scope_id
         WHERE {where_clause} ORDER BY s.created_at_ms DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([arg], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, String>(7)?,
        ))
    })?;
    Ok(rows
        .filter_map(Result::ok)
        .filter_map(|(id, title, source, mode, created, revoked, key, destination)| {
            Some(ShareView {
                id: ShareId::parse(&id).ok()?,
                title,
                source,
                destination,
                mode: ShareMode::parse(&mode),
                excerpt: read_blob(&key).unwrap_or_default().chars().take(280).collect(),
                created_at_ms: created,
                revoked: revoked.is_some(),
            })
        })
        .collect())
}

pub fn for_project(conn: &Connection, project: ProjectId, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<ShareView>, StorageError> {
    view(
        conn,
        "s.destination_scope_id IN (SELECT scope_id FROM projects WHERE id = ?1)",
        &project.to_string(),
        read_blob,
    )
}

pub fn for_conversation(conn: &Connection, conversation: ConversationId, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<ShareView>, StorageError> {
    view(conn, "s.destination_conversation_id = ?1 AND s.revoked_at_ms IS NULL", &conversation.to_string(), read_blob)
}
