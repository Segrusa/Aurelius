//! Who may edit or delete a message, and what an edit left behind (AUR-FA-F3).
//! The rule is `EditFacts::refusal` in the domain; this collects its facts, one
//! way for a whole conversation (the read) and for one message (the commands).

use crate::{StorageError, conversations, threads};
use aurelius_domain::group_chat::EditFacts;
use aurelius_domain::{ConversationId, MessageId, UserId};
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashSet;

/// The facts of every message of `conversation` (or only of `only`), told by the
/// rows themselves. A store before schema 29 has no deletions, so none are deleted.
fn facts_in(conn: &Connection, conversation: ConversationId, only: Option<MessageId>) -> Result<Vec<(MessageId, EditFacts)>, StorageError> {
    let in_discussion = threads::discussion_closed(conn, conversation)?;
    let read_only = in_discussion.unwrap_or(false);
    let deleted = if conversations::has_edit_columns(conn)? { "m.deleted_at_ms IS NOT NULL" } else { "0" };
    let mut stmt = conn.prepare(&format!(
        "SELECT m.id, m.author_user_id, json_extract(m.meta_json, '$.system') IS NOT NULL, {deleted}
         FROM messages m WHERE m.conversation_id = ?1 AND (?2 IS NULL OR m.id = ?2)"
    ))?;
    let rows = stmt.query_map(rusqlite::params![conversation.to_string(), only.map(|m| m.to_string())], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, bool>(2)?, r.get::<_, bool>(3)?))
    })?;
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    let queued = conversations::queued_messages(conn, conversation)?;
    let feedback = conversations::decision_feedback_in(conn, conversation)?;
    let mut out = Vec::new();
    for (id, author, system, deleted) in rows {
        let Ok(id) = MessageId::parse(&id) else { continue };
        out.push((
            id,
            EditFacts {
                author: author.and_then(|a| UserId::parse(&a).ok()),
                in_discussion: in_discussion.is_some(),
                system,
                queued: queued.contains(&id),
                deleted,
                decision_feedback: feedback.contains(&id),
                read_only,
            },
        ));
    }
    Ok(out)
}

/// The facts of one message and its conversation; none for an unknown message.
pub fn facts(conn: &Connection, message: MessageId) -> Result<Option<(ConversationId, EditFacts)>, StorageError> {
    let Some(place) = threads::place(conn, message)? else { return Ok(None) };
    Ok(facts_in(conn, place.conversation_id, Some(message))?.into_iter().next().map(|(_, f)| (place.conversation_id, f)))
}

/// The messages of `conversation` that `reader` may edit and delete: one query
/// set for the whole read, with the same rule as the commands. None for no reader.
pub fn editable_in(conn: &Connection, conversation: ConversationId, reader: Option<UserId>) -> Result<HashSet<MessageId>, StorageError> {
    if reader.is_none() {
        return Ok(HashSet::new());
    }
    Ok(facts_in(conn, conversation, None)?.into_iter().filter(|(_, f)| f.refusal(reader).is_none()).map(|(id, _)| id).collect())
}

/// Set `editable` on `messages` of `conversation` as `reader` may change them.
pub fn mark_editable(conn: &Connection, conversation: ConversationId, reader: Option<UserId>, messages: &mut [aurelius_domain::views::MessageView]) -> Result<(), StorageError> {
    let editable = editable_in(conn, conversation, reader)?;
    for message in messages.iter_mut() {
        message.editable = editable.contains(&message.id);
    }
    Ok(())
}

/// The earlier texts of `message`, newest first, as `(storage key, when an edit replaced it)`.
/// Read from the `message.edited` events, whose payload is the blob that was replaced.
pub fn earlier_texts(conn: &Connection, message: MessageId) -> Result<Vec<(String, i64)>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT b.storage_key, e.occurred_at_ms FROM events e JOIN blobs b ON b.id = e.payload_blob_id
         WHERE e.type = 'message.edited' AND json_extract(e.metadata_json, '$.message_id') = ?1 ORDER BY e.seq DESC",
    )?;
    let rows = stmt.query_map([message.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Whether `message` was deleted, and whether it is a message of a channel or a
/// ticket's discussion at all (none: it is not).
pub fn deleted(conn: &Connection, message: MessageId) -> Result<Option<bool>, StorageError> {
    let Some(place) = threads::place(conn, message)? else { return Ok(None) };
    if threads::discussion_closed(conn, place.conversation_id)?.is_none() {
        return Ok(None);
    }
    if !conversations::has_edit_columns(conn)? {
        return Ok(Some(false));
    }
    Ok(conn.query_row("SELECT deleted_at_ms IS NOT NULL FROM messages WHERE id = ?1", [message.to_string()], |r| r.get::<_, bool>(0)).optional()?)
}
