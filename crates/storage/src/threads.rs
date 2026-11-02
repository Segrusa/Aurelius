//! Threads and reactions in the discussions (migrations 26 and 27; features
//! `chat_threads` and `chat_reactions`). A reply is a message of the channel's
//! own conversation with `thread_parent_id` set to the message that starts the
//! thread (its root, never itself a reply). Replies are never in the channel's
//! own list (`conversations::list_messages`) nor in its unread count; the root
//! carries how many there are, the latest, and how many the reader has not read
//! (`thread_reads`, moved forward only by reading the thread).
//!
//! A reaction is one emoji of one board member (or employee) on one message, once.

use crate::discussions::COUNTED;
use crate::{StorageError, now_ms};
use aurelius_domain::group_chat::OWNER;
use aurelius_domain::views::{MessageLocation, MessageView, ReactionView, THREAD_REPLIERS_MAX};
use aurelius_domain::{ConversationId, MessageId, TicketId, UserId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::collections::HashMap;

/// The schema version whose migration keeps how far each board member read each thread.
pub const THREADS_SCHEMA: i64 = 27;

/// Whether the store has the threads' read markers (schema 27).
pub fn has_threads(conn: &Connection) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'thread_reads'", [], |r| r.get::<_, i64>(0))? > 0)
}

/// Where a message is, as a thread or a reaction needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessagePlace {
    pub conversation_id: ConversationId,
    /// The root of the thread it replies in; none for a message of the channel itself.
    pub thread_parent: Option<MessageId>,
}

/// The conversation of `message` and the thread it is a reply in; none for an unknown message.
pub fn place(conn: &Connection, message: MessageId) -> Result<Option<MessagePlace>, StorageError> {
    let row: Option<(String, Option<String>)> =
        conn.query_row("SELECT conversation_id, thread_parent_id FROM messages WHERE id = ?1", [message.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    let Some((conversation, parent)) = row else { return Ok(None) };
    let Ok(conversation_id) = ConversationId::parse(&conversation) else { return Ok(None) };
    Ok(Some(MessagePlace { conversation_id, thread_parent: parent.and_then(|p| MessageId::parse(&p).ok()) }))
}

/// Where `message` is for a link to it (`message.locate`): its conversation, the
/// ticket whose discussion that is, and the thread it replies in. None for an
/// unknown message and for one outside a channel or a ticket's discussion.
pub fn locate(conn: &Connection, message: MessageId) -> Result<Option<MessageLocation>, StorageError> {
    type Row = (String, Option<String>, Option<String>, Option<i64>);
    let row: Option<Row> = conn
        .query_row(
            "SELECT m.conversation_id, m.thread_parent_id, t.id, t.number
             FROM messages m JOIN conversations c ON c.id = m.conversation_id LEFT JOIN tickets t ON t.conversation_id = c.id
             WHERE m.id = ?1 AND c.kind IN ('group', 'ticket')",
            [message.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((conversation, parent, ticket, number)) = row else { return Ok(None) };
    let Ok(conversation_id) = ConversationId::parse(&conversation) else { return Ok(None) };
    Ok(Some(MessageLocation {
        message_id: message,
        conversation_id,
        ticket_id: ticket.and_then(|t| TicketId::parse(&t).ok()),
        ticket_number: number.map(|n| n as u32),
        thread_root: parent.and_then(|p| MessageId::parse(&p).ok()),
    }))
}

/// Make `message` (just inserted) a reply in the thread of `root`.
pub fn set_thread_parent(tx: &Transaction<'_>, message: MessageId, root: MessageId) -> Result<(), StorageError> {
    tx.execute("UPDATE messages SET thread_parent_id = ?2 WHERE id = ?1", [message.to_string(), root.to_string()])?;
    Ok(())
}

/// `user` read the thread of `root` up to its latest reply (the root itself
/// when it has none). It never goes back; nothing for an unknown message or
/// board member, or a store before schema 27.
pub fn mark_read(tx: &Transaction<'_>, user: UserId, root: MessageId) -> Result<(), StorageError> {
    if !has_threads(tx)? {
        return Ok(());
    }
    tx.execute(
        "INSERT INTO thread_reads(user_id, root_message_id, read_through, read_at_ms)
         SELECT ?1, m.id, COALESCE((SELECT MAX(ordinal) FROM messages WHERE thread_parent_id = m.id), m.ordinal), ?3
         FROM messages m WHERE m.id = ?2 AND EXISTS (SELECT 1 FROM users WHERE id = ?1)
         ON CONFLICT(user_id, root_message_id) DO UPDATE SET read_through = MAX(read_through, excluded.read_through), read_at_ms = excluded.read_at_ms",
        params![user.to_string(), root.to_string(), now_ms()],
    )?;
    Ok(())
}

/// The highest reply ordinal `user` read in the thread of `root`; none: never read.
pub fn read_through(conn: &Connection, user: UserId, root: MessageId) -> Result<Option<u32>, StorageError> {
    if !has_threads(conn)? {
        return Ok(None);
    }
    let through: Option<i64> =
        conn.query_row("SELECT read_through FROM thread_reads WHERE user_id = ?1 AND root_message_id = ?2", [user.to_string(), root.to_string()], |r| r.get(0)).optional()?;
    Ok(through.map(|t| t as u32))
}

/// Per root in `conversation`: (replies, latest reply's time). Only the replies
/// that count (a board member's, an employee's post or answer; not system lines).
fn reply_counts(conn: &Connection, conversation: ConversationId) -> Result<HashMap<String, (u32, i64)>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT m.thread_parent_id, COUNT(*), MAX(m.created_at_ms) FROM messages m
         WHERE m.conversation_id = ?1 AND m.thread_parent_id IS NOT NULL AND {COUNTED} GROUP BY m.thread_parent_id"
    ))?;
    let rows = stmt.query_map([conversation.to_string()], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)? as u32, r.get::<_, i64>(2)?))))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Per root in `conversation`: who replied (the replies that count), each name
/// once, most recent first, at most `THREAD_REPLIERS_MAX`. One query for the
/// whole conversation: each (root, name) once with its latest reply.
fn repliers(conn: &Connection, conversation: ConversationId) -> Result<HashMap<String, Vec<String>>, StorageError> {
    // A board member by username (the owner before phase 2 has none), an employee by the name it posted under.
    let mut stmt = conn.prepare(&format!(
        "SELECT m.thread_parent_id,
                CASE WHEN m.role = 'user' THEN COALESCE(u.username, ?2) ELSE COALESCE(json_extract(m.meta_json, '$.from'), 'Aurelius') END AS name,
                MAX(m.ordinal) AS latest
         FROM messages m LEFT JOIN users u ON u.id = m.author_user_id
         WHERE m.conversation_id = ?1 AND m.thread_parent_id IS NOT NULL AND {COUNTED}
         GROUP BY m.thread_parent_id, name"
    ))?;
    let rows = stmt.query_map(params![conversation.to_string(), OWNER], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
    Ok(most_recent_repliers(rows.collect::<Result<Vec<_>, _>>()?))
}

/// Per root, the names of (root, name, latest reply ordinal) rows, latest first,
/// at most `THREAD_REPLIERS_MAX`. A name is once per root in the rows already.
fn most_recent_repliers(mut rows: Vec<(String, String, i64)>) -> HashMap<String, Vec<String>> {
    rows.sort_by_key(|row| std::cmp::Reverse(row.2));
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for (root, name, _) in rows {
        let names = out.entry(root).or_default();
        if names.len() < THREAD_REPLIERS_MAX && !names.contains(&name) {
            names.push(name);
        }
    }
    out
}

/// Per root in `conversation`: the replies of others `user` has not read yet.
fn unread_counts(conn: &Connection, conversation: ConversationId, user: UserId) -> Result<HashMap<String, u32>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT m.thread_parent_id, COUNT(*) FROM messages m
         LEFT JOIN thread_reads t ON t.user_id = ?2 AND t.root_message_id = m.thread_parent_id
         WHERE m.conversation_id = ?1 AND m.thread_parent_id IS NOT NULL AND {COUNTED}
           AND m.ordinal > COALESCE(t.read_through, 0) AND (m.author_user_id IS NULL OR m.author_user_id <> ?2)
         GROUP BY m.thread_parent_id"
    ))?;
    let rows = stmt.query_map(params![conversation.to_string(), user.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The storage keys of the replies of others in `conversation` that `user` has
/// not read yet (newest first, at most `limit`), to look for a mention of them.
pub fn unread_reply_keys(conn: &Connection, conversation: ConversationId, user: UserId, limit: u32) -> Result<Vec<String>, StorageError> {
    if !has_threads(conn)? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(&format!(
        "SELECT b.storage_key FROM messages m JOIN blobs b ON b.id = m.content_blob_id
         LEFT JOIN thread_reads t ON t.user_id = ?2 AND t.root_message_id = m.thread_parent_id
         WHERE m.conversation_id = ?1 AND m.thread_parent_id IS NOT NULL AND {COUNTED}
           AND m.ordinal > COALESCE(t.read_through, 0) AND (m.author_user_id IS NULL OR m.author_user_id <> ?2)
         ORDER BY m.ordinal DESC LIMIT ?3"
    ))?;
    let rows = stmt.query_map(params![conversation.to_string(), user.to_string(), limit], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Who reacted how on the messages of `conversation`: per message, per emoji
/// (in the order each emoji was first used), the reactors oldest first as
/// (board member, name). An employee's reaction has no board member.
type Reactions = HashMap<String, Vec<(String, Vec<(Option<String>, String)>)>>;

fn reactions_in(conn: &Connection, conversation: ConversationId) -> Result<Reactions, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT r.message_id, r.emoji, r.user_id, COALESCE(u.username, e.name, '?') FROM message_reactions r
         JOIN messages m ON m.id = r.message_id LEFT JOIN users u ON u.id = r.user_id LEFT JOIN employees e ON e.id = r.employee_id
         WHERE m.conversation_id = ?1 ORDER BY r.created_at_ms, r.rowid",
    )?;
    let rows = stmt.query_map([conversation.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?)))?;
    let mut out: Reactions = HashMap::new();
    for row in rows {
        let (message, emoji, user, name) = row?;
        let emojis = out.entry(message).or_default();
        match emojis.iter_mut().find(|(e, _)| *e == emoji) {
            Some((_, who)) => who.push((user, name)),
            None => emojis.push((emoji, vec![(user, name)])),
        }
    }
    Ok(out)
}

/// The threads' counters and the reactions of `messages` (of `conversation`),
/// as `reader` sees them: `thread_reply_count`, `thread_last_reply_ms`,
/// `thread_repliers`, `thread_unread` (none without a reader) and `reactions`
/// (`mine` for the reader).
pub fn decorate(conn: &Connection, conversation: ConversationId, reader: Option<UserId>, messages: &mut [MessageView]) -> Result<(), StorageError> {
    if !has_threads(conn)? {
        return Ok(());
    }
    let counts = reply_counts(conn, conversation)?;
    let mut repliers = if counts.is_empty() { HashMap::new() } else { repliers(conn, conversation)? };
    let unread = match reader {
        Some(user) if !counts.is_empty() => unread_counts(conn, conversation, user)?,
        _ => HashMap::new(),
    };
    let reactions = reactions_in(conn, conversation)?;
    let me = reader.map(|u| u.to_string());
    for message in messages.iter_mut() {
        let id = message.id.to_string();
        if let Some((count, last)) = counts.get(&id) {
            message.thread_reply_count = Some(*count);
            message.thread_last_reply_ms = Some(*last);
        }
        message.thread_repliers = repliers.remove(&id).unwrap_or_default();
        message.thread_unread = unread.get(&id).copied().filter(|n| *n > 0);
        message.reactions = reactions
            .get(&id)
            .map(|emojis| {
                emojis
                    .iter()
                    .map(|(emoji, who)| ReactionView {
                        emoji: emoji.clone(),
                        count: who.len() as u32,
                        mine: me.as_ref().is_some_and(|me| who.iter().any(|(u, _)| u.as_ref() == Some(me))),
                        by: who.iter().map(|(_, name)| name.clone()).collect(),
                    })
                    .collect()
            })
            .unwrap_or_default();
    }
    Ok(())
}

/// A board member's reaction `emoji` on `message`: added (`on`; nothing when it
/// is there already) or taken back (nothing when it is not there). Whether anything changed.
pub fn react(tx: &Transaction<'_>, message: MessageId, user: UserId, emoji: &str, on: bool) -> Result<bool, StorageError> {
    let changed = if on {
        tx.execute(
            "INSERT OR IGNORE INTO message_reactions(message_id, user_id, emoji, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
            params![message.to_string(), user.to_string(), emoji, now_ms()],
        )?
    } else {
        tx.execute("DELETE FROM message_reactions WHERE message_id = ?1 AND user_id = ?2 AND emoji = ?3", params![message.to_string(), user.to_string(), emoji])?
    };
    Ok(changed > 0)
}

/// Whether the discussion `conversation` (a channel or a ticket's) is closed:
/// an archived project's channel, a ticket's on Done or archived. None for a
/// conversation that is no discussion.
pub fn discussion_closed(conn: &Connection, conversation: ConversationId) -> Result<Option<bool>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT CASE c.kind
                 WHEN 'group' THEN COALESCE((SELECT p.status = 'archived' FROM projects p WHERE p.id = c.project_id), 0)
                 ELSE c.archived_at_ms IS NOT NULL OR EXISTS (SELECT 1 FROM tickets t JOIN board_columns b ON b.id = t.column_id
                        WHERE t.conversation_id = c.id AND (t.archived_at_ms IS NOT NULL OR b.kind = 'done')) END
             FROM conversations c WHERE c.id = ?1 AND c.kind IN ('group', 'ticket')",
            [conversation.to_string()],
            |r| r.get::<_, bool>(0),
        )
        .optional()?)
}
