//! Discussions (migration 19): the channels of the Team chat and the tickets'
//! discussions. A channel is a conversation of kind `group`: the general one
//! (the group chat of migration 17, no project) and one per project, made with
//! the project. A ticket's discussion is a conversation of kind `ticket` with
//! the ticket's project, made with the ticket (`tickets.conversation_id`).
//!
//! The loop limits of RULE 08 count over all of them together, per employee
//! (`waking_posts`): a loop through three channels gets no more than one would.

use crate::{StorageError, now_ms};
use aurelius_domain::{ConversationId, EmployeeId, MessageId, ProjectId, RunId, TicketId, UserId, conversation::ConversationKind};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::collections::HashMap;

/// The schema version whose migration makes the channels and discussions.
pub const DISCUSSIONS_SCHEMA: i64 = 19;

fn parse<T, E: std::error::Error + Send + Sync + 'static>(r: Result<T, E>) -> rusqlite::Result<T> {
    r.map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

fn conversation_id(id: Option<String>) -> Option<ConversationId> {
    id.and_then(|c| ConversationId::parse(&c).ok())
}

/// What a conversation is for; none for an unknown conversation.
pub fn kind_of(conn: &Connection, conversation: ConversationId) -> Result<Option<ConversationKind>, StorageError> {
    let kind: Option<String> = conn.query_row("SELECT kind FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0)).optional()?;
    Ok(kind.as_deref().and_then(ConversationKind::parse))
}

/// `run` said its last word in `discussion` itself: it posted there and used no
/// tool after that other than another post, so what it still wrote is a sign-off.
pub fn run_said_last_in(conn: &Connection, discussion: ConversationId, run: RunId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM messages m WHERE m.conversation_id = ?1 AND json_extract(m.meta_json, '$.run_id') = ?2
           AND NOT EXISTS(SELECT 1 FROM run_activity a WHERE a.run_id = ?2 AND a.started_at_ms > m.created_at_ms
                          AND a.tool NOT LIKE '%post_to_discussion' AND a.tool NOT LIKE '%post_to_ticket'))",
        [discussion.to_string(), run.to_string()],
        |r| r.get(0),
    )?)
}

// ---- channels ----

/// The channel of a project (`Some`), or the general channel (`None`).
pub fn channel(conn: &Connection, project: Option<ProjectId>) -> Result<Option<ConversationId>, StorageError> {
    let id: Option<String> = match project {
        None => conn
            .query_row("SELECT id FROM conversations WHERE kind = 'group' AND project_id IS NULL", [], |r| r.get(0))
            .optional()?,
        Some(project) => conn
            .query_row("SELECT id FROM conversations WHERE kind = 'group' AND project_id = ?1", [project.to_string()], |r| r.get(0))
            .optional()?,
    };
    Ok(conversation_id(id))
}

/// Whether the database has the channels and discussions (schema 19 or later).
/// The v0.6 import and some fixtures write projects into an older schema;
/// migration 19 then makes their channels.
pub fn has_discussions(conn: &Connection) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'one_project_channel')", [], |r| r.get(0))?)
}

/// Make a project's channel, titled with its name (`insert_project_projection`
/// does, in the project's transaction). A project has at most one. None: the
/// schema is older than 19 (see `has_discussions`).
pub fn insert_project_channel(tx: &Transaction<'_>, project: ProjectId, name: &str) -> Result<Option<ConversationId>, StorageError> {
    if !has_discussions(tx)? {
        return Ok(None);
    }
    let id = ConversationId::new();
    tx.execute(
        "INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES (?1, 'group', ?2, ?3, 1, ?4)",
        params![id.to_string(), project.to_string(), name, now_ms()],
    )?;
    Ok(Some(id))
}

/// A project's channel follows its name.
pub fn rename_project_channel(tx: &Transaction<'_>, project: ProjectId, name: &str) -> Result<(), StorageError> {
    tx.execute("UPDATE conversations SET title = ?2 WHERE kind = 'group' AND project_id = ?1", params![project.to_string(), name])?;
    Ok(())
}

/// One channel of the Team chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub conversation_id: ConversationId,
    /// None: the general channel.
    pub project_id: Option<ProjectId>,
    /// "Team chat" for the general channel, the project's name for the others.
    pub name: String,
    /// The project is archived: its channel is read-only.
    pub read_only: bool,
}

/// The channels: the general one first, then the projects' by name.
pub fn channels(conn: &Connection) -> Result<Vec<Channel>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.project_id, COALESCE(p.name, c.title), COALESCE(p.status = 'archived', 0)
         FROM conversations c LEFT JOIN projects p ON p.id = c.project_id
         WHERE c.kind = 'group'
         ORDER BY c.project_id IS NOT NULL, p.name COLLATE NOCASE, c.created_at_ms, c.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Channel {
            conversation_id: parse(ConversationId::parse(&r.get::<_, String>(0)?))?,
            project_id: r.get::<_, Option<String>>(1)?.map(|p| parse(ProjectId::parse(&p))).transpose()?,
            name: r.get(2)?,
            read_only: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ---- tickets ----

/// Make a ticket's discussion (`tickets::insert` does, before the ticket).
pub(crate) fn insert_ticket_discussion(tx: &Transaction<'_>, project: ProjectId, title: &str) -> Result<ConversationId, StorageError> {
    let id = ConversationId::new();
    tx.execute(
        "INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES (?1, 'ticket', ?2, ?3, 1, ?4)",
        params![id.to_string(), project.to_string(), title, now_ms()],
    )?;
    Ok(id)
}

/// The title of a ticket's discussion.
pub(crate) fn ticket_discussion_title(number: u32, title: &str) -> String {
    format!("#{number} {title}")
}

/// A ticket's discussion; none for an unknown ticket.
pub fn ticket_conversation(conn: &Connection, ticket: TicketId) -> Result<Option<ConversationId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT conversation_id FROM tickets WHERE id = ?1", [ticket.to_string()], |r| r.get(0)).optional()?;
    Ok(conversation_id(id))
}

/// The ticket a discussion belongs to; none when it is not a ticket's.
pub fn ticket_of_conversation(conn: &Connection, conversation: ConversationId) -> Result<Option<TicketId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT id FROM tickets WHERE conversation_id = ?1", [conversation.to_string()], |r| r.get(0)).optional()?;
    Ok(id.and_then(|t| TicketId::parse(&t).ok()))
}

// ---- ticket channels (migration 26) ----

/// The schema version whose migration makes channel members, threads and reactions.
pub const TICKET_CHANNELS_SCHEMA: i64 = 26;

/// Whether the database has channel members (schema 26 or later).
pub fn has_members(conn: &Connection) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'channel_members')", [], |r| r.get(0))?)
}

/// Who is in a channel: a board member or an employee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Member {
    /// A board member: the ticket's creator, the Owner, or who joined later.
    User(UserId),
    /// An employee: the ticket's owner or its main task's employee.
    Employee(EmployeeId),
}

/// `member` joins the channel `conversation` (a ticket's), unless already in it.
/// True when they joined now. Nobody is ever removed. Nothing before schema 26.
pub fn join(tx: &Transaction<'_>, conversation: ConversationId, member: Member, at_ms: i64) -> Result<bool, StorageError> {
    if !has_members(tx)? {
        return Ok(false);
    }
    let (user, employee) = match member {
        Member::User(u) => (Some(u.to_string()), None),
        Member::Employee(e) => (None, Some(e.to_string())),
    };
    let added = tx.execute(
        "INSERT OR IGNORE INTO channel_members(conversation_id, user_id, employee_id, joined_at_ms) VALUES (?1, ?2, ?3, ?4)",
        params![conversation.to_string(), user, employee, at_ms],
    )?;
    Ok(added > 0)
}

/// The members of `conversation`, oldest first.
pub fn members(conn: &Connection, conversation: ConversationId) -> Result<Vec<Member>, StorageError> {
    if !has_members(conn)? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare("SELECT user_id, employee_id FROM channel_members WHERE conversation_id = ?1 ORDER BY joined_at_ms, rowid")?;
    let rows = stmt.query_map([conversation.to_string()], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        match row? {
            (Some(u), _) => out.push(Member::User(parse(UserId::parse(&u))?)),
            (_, Some(e)) => out.push(Member::Employee(parse(EmployeeId::parse(&e))?)),
            _ => {}
        }
    }
    Ok(out)
}

/// The Owner: the board members made in the onboarding (by nobody) who are not
/// disabled. Board members have no roles; the one who set the server up owns it.
pub fn owners(conn: &Connection) -> Result<Vec<UserId>, StorageError> {
    let mut stmt = conn.prepare("SELECT id FROM users WHERE created_by_user IS NULL AND disabled_at_ms IS NULL ORDER BY created_at_ms, id")?;
    let rows = stmt.query_map([], |r| parse(UserId::parse(&r.get::<_, String>(0)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// When the channel `conversation` was archived (a closed ticket's); none: open.
pub fn archived_at(conn: &Connection, conversation: ConversationId) -> Result<Option<i64>, StorageError> {
    Ok(conn.query_row("SELECT archived_at_ms FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0)).optional()?.flatten())
}

/// Archive (`Some`) or reopen (`None`) a ticket's channel. Only a conversation
/// of kind `ticket`: archiving another kind means deleting it.
pub fn set_ticket_channel_archived(tx: &Transaction<'_>, conversation: ConversationId, at_ms: Option<i64>) -> Result<(), StorageError> {
    tx.execute("UPDATE conversations SET archived_at_ms = ?2 WHERE id = ?1 AND kind = 'ticket'", params![conversation.to_string(), at_ms])?;
    Ok(())
}

/// A ticket's channel, as `channel.list` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketChannel {
    /// The ticket's discussion, which is its channel.
    pub conversation_id: ConversationId,
    /// The project the ticket belongs to.
    pub project_id: ProjectId,
    /// The ticket whose channel this is.
    pub ticket_id: TicketId,
    /// The ticket's number (`#N`), for the channel's name and search.
    pub number: u32,
    /// The ticket's whole title: the name's slug is cut, search reads all of it.
    pub title: String,
    /// The kind of the column the ticket is in.
    pub column: aurelius_domain::ticket::ColumnKind,
    /// Closed (the ticket is on Done or archived): read-only.
    pub archived: bool,
    /// How many board members and employees are members (0 before schema 26).
    pub member_count: u32,
}

/// Every ticket's channel, newest ticket first.
pub fn ticket_channels(conn: &Connection) -> Result<Vec<TicketChannel>, StorageError> {
    let members = if has_members(conn)? { "(SELECT COUNT(*) FROM channel_members m WHERE m.conversation_id = c.id)" } else { "0" };
    let mut stmt = conn.prepare(&format!(
        "SELECT c.id, t.project_id, t.id, t.number, t.title, b.kind, c.archived_at_ms IS NOT NULL OR t.archived_at_ms IS NOT NULL OR b.kind = 'done', {members}
         FROM tickets t JOIN conversations c ON c.id = t.conversation_id JOIN board_columns b ON b.id = t.column_id
         WHERE c.kind = 'ticket'
         ORDER BY t.number DESC"
    ))?;
    let rows = stmt.query_map([], |r| {
        Ok(TicketChannel {
            conversation_id: parse(ConversationId::parse(&r.get::<_, String>(0)?))?,
            project_id: parse(ProjectId::parse(&r.get::<_, String>(1)?))?,
            ticket_id: parse(TicketId::parse(&r.get::<_, String>(2)?))?,
            number: r.get::<_, i64>(3)? as u32,
            title: r.get(4)?,
            column: aurelius_domain::ticket::ColumnKind::parse(&r.get::<_, String>(5)?).unwrap_or(aurelius_domain::ticket::ColumnKind::Backlog),
            archived: r.get(6)?,
            member_count: r.get::<_, i64>(7)? as u32,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The highest ordinal `user` had read in `conversation`; none: never read.
pub fn read_through(conn: &Connection, user: UserId, conversation: ConversationId) -> Result<Option<u32>, StorageError> {
    if !has_reads(conn)? {
        return Ok(None);
    }
    let through: Option<i64> = conn
        .query_row("SELECT read_through FROM discussion_reads WHERE user_id = ?1 AND conversation_id = ?2", [user.to_string(), conversation.to_string()], |r| r.get(0))
        .optional()?;
    Ok(through.map(|t| t as u32))
}

// ---- the loop limits ----

/// How many posts woke colleagues (`woke` in their meta), over every channel and
/// ticket discussion together: of run `run`, and of employee `employee` since
/// `since_ms` (the limits of `aurelius_domain::group_chat`).
pub fn waking_posts(conn: &Connection, run: RunId, employee: EmployeeId, since_ms: i64) -> Result<(u32, u32), StorageError> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(json_extract(m.meta_json, '$.run_id') = ?1), 0),
                COALESCE(SUM(json_extract(m.meta_json, '$.employee_id') = ?2 AND m.created_at_ms >= ?3), 0)
         FROM messages m JOIN conversations c ON c.id = m.conversation_id
         WHERE m.role = 'tool' AND json_extract(m.meta_json, '$.woke') > 0 AND c.kind IN ('group','ticket')",
        params![run.to_string(), employee.to_string(), since_ms],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

// ---- read markers (migration 20) ----

/// The schema version whose migration keeps how far each board member read.
pub const READS_SCHEMA: i64 = 20;

/// The messages that count for unread: the discussion's own (a board member's,
/// an employee's post or answer), not Aurelius's notes and system lines.
pub(crate) const COUNTED: &str = "m.status = 'complete' AND (m.role = 'user' OR json_extract(m.meta_json, '$.kind') = 'group')";

/// `user` read the discussion `conversation` (a channel or a ticket's
/// discussion) up to its latest message. It never goes back; nothing for
/// another conversation, an unknown board member, or a store before migration 20.
pub fn mark_read(tx: &Transaction<'_>, user: UserId, conversation: ConversationId) -> Result<(), StorageError> {
    if !has_reads(tx)? {
        return Ok(());
    }
    tx.execute(
        "INSERT INTO discussion_reads(user_id, conversation_id, read_through, read_at_ms)
         SELECT ?1, c.id, COALESCE((SELECT MAX(ordinal) FROM messages WHERE conversation_id = c.id), 0), ?3
         FROM conversations c WHERE c.id = ?2 AND c.kind IN ('group', 'ticket') AND EXISTS (SELECT 1 FROM users WHERE id = ?1)
         ON CONFLICT(user_id, conversation_id) DO UPDATE SET read_through = MAX(read_through, excluded.read_through), read_at_ms = excluded.read_at_ms",
        params![user.to_string(), conversation.to_string(), now_ms()],
    )?;
    Ok(())
}

fn has_reads(conn: &Connection) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'discussion_reads'", [], |r| r.get::<_, i64>(0))? > 0)
}

/// The newest unread messages whose text is checked for a mention.
const MENTION_SCAN: u32 = 500;

/// What `user` (`username`) has not read yet in `conversation`: the messages of
/// others since the last one read, and how many of those name them with
/// `@username` (case does not matter). `read_text` reads a message's text by its storage key.
///
/// The replies in its threads (schema 27) are not among the unread: a thread
/// has its own (`MessageView::thread_unread`). But a reply of another that names
/// them and that they have not read in its thread counts among the mentions.
pub fn unread(conn: &Connection, user: UserId, username: &str, conversation: ConversationId, read_text: &dyn Fn(&str) -> Option<String>) -> Result<(u32, u32), StorageError> {
    if !has_reads(conn)? {
        return Ok((0, 0));
    }
    let through: i64 = conn
        .query_row("SELECT read_through FROM discussion_reads WHERE user_id = ?1 AND conversation_id = ?2", [user.to_string(), conversation.to_string()], |r| r.get(0))
        .optional()?
        .unwrap_or(0);
    let threads = crate::threads::has_threads(conn)?;
    let own = if threads { " AND m.thread_parent_id IS NULL" } else { "" };
    let filter = format!("m.conversation_id = ?1 AND m.ordinal > ?2 AND (m.author_user_id IS NULL OR m.author_user_id <> ?3) AND {COUNTED}{own}");
    let unread: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM messages m WHERE {filter}"), params![conversation.to_string(), through, user.to_string()], |r| r.get(0))?;
    let mut keys = Vec::new();
    if unread > 0 {
        let mut stmt = conn.prepare(&format!("SELECT b.storage_key FROM messages m JOIN blobs b ON b.id = m.content_blob_id WHERE {filter} ORDER BY m.ordinal DESC LIMIT ?4"))?;
        keys = stmt.query_map(params![conversation.to_string(), through, user.to_string(), MENTION_SCAN], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    }
    if threads {
        keys.extend(crate::threads::unread_reply_keys(conn, conversation, user, MENTION_SCAN)?);
    }
    let mentions = keys.iter().filter_map(|k| read_text(k)).filter(|text| mentions(text, username)).count();
    Ok((unread as u32, mentions as u32))
}

/// `text` names `username` with `@username`, not as the start of a longer name.
pub fn mentions(text: &str, username: &str) -> bool {
    let (text, name) = (text.to_lowercase(), format!("@{}", username.to_lowercase()));
    if name.len() < 2 {
        return false;
    }
    text.match_indices(&name).any(|(at, _)| !text[at + name.len()..].chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.' && text[at + name.len() + 1..].chars().next().is_some_and(char::is_alphanumeric)))
}

// ---- authors ----

/// The board member who wrote a message; none for an employee's, Aurelius's, or
/// an unknown message.
pub fn message_author_user(conn: &Connection, message: MessageId) -> Result<Option<UserId>, StorageError> {
    let id: Option<Option<String>> = conn.query_row("SELECT author_user_id FROM messages WHERE id = ?1", [message.to_string()], |r| r.get(0)).optional()?;
    Ok(id.flatten().and_then(|u| UserId::parse(&u).ok()))
}

/// The board members who wrote in a conversation, per message (only the
/// messages that have one).
pub fn message_author_users(conn: &Connection, conversation: ConversationId) -> Result<HashMap<MessageId, UserId>, StorageError> {
    let mut stmt = conn.prepare("SELECT id, author_user_id FROM messages WHERE conversation_id = ?1 AND author_user_id IS NOT NULL")?;
    let rows = stmt.query_map([conversation.to_string()], |r| {
        Ok((parse(MessageId::parse(&r.get::<_, String>(0)?))?, parse(UserId::parse(&r.get::<_, String>(1)?))?))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// One message of a discussion as employees read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscussionLine {
    pub id: MessageId,
    /// The employee who wrote it, or "Aurelius"; none for a board member's.
    pub author: Option<String>,
    /// The board member who wrote it and their username (none for an older
    /// message of the user, from before phase 2).
    pub author_user: Option<(UserId, String)>,
    /// The storage key of its text.
    pub storage_key: String,
}

/// The latest `limit` messages employees read in a channel or ticket discussion
/// (the same as `conversations::latest_group_messages`, with the board member),
/// before `before` when given, newest first. Not the replies in its threads.
pub fn latest_messages(conn: &Connection, conversation: ConversationId, before: Option<MessageId>, limit: u32) -> Result<Vec<DiscussionLine>, StorageError> {
    latest_in(conn, conversation, None, before, limit)
}

/// `latest_messages` of the channel itself (`thread` none), or of the thread of
/// `thread`: its root and its replies.
pub fn latest_in(conn: &Connection, conversation: ConversationId, thread: Option<MessageId>, before: Option<MessageId>, limit: u32) -> Result<Vec<DiscussionLine>, StorageError> {
    // Each filter names ?4 (the root, or NULL), so the statement always takes every parameter.
    let only = match (crate::conversations::has_thread_column(conn)?, thread) {
        (true, None) => "?4 IS NULL AND m.thread_parent_id IS NULL",
        (true, Some(_)) => "(m.id = ?4 OR m.thread_parent_id = ?4)",
        (false, None) => "?4 IS NULL",
        (false, Some(_)) => "m.id = ?4",
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT m.id, CASE WHEN m.role = 'user' THEN NULL ELSE COALESCE(json_extract(m.meta_json, '$.from'), 'Aurelius') END,
                u.id, u.username, b.storage_key
         FROM messages m JOIN blobs b ON b.id = m.content_blob_id LEFT JOIN users u ON u.id = m.author_user_id
         WHERE m.conversation_id = ?1 AND m.status = 'complete' AND (m.role = 'user' OR json_extract(m.meta_json, '$.kind') = 'group') AND {only}
           AND (?2 IS NULL OR m.ordinal < (SELECT ordinal FROM messages WHERE id = ?2)) ORDER BY m.ordinal DESC LIMIT ?3"
    ))?;
    let rows = stmt.query_map(params![conversation.to_string(), before.map(|m| m.to_string()), limit, thread.map(|t| t.to_string())], |r| {
        let user = match (r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?) {
            (Some(id), Some(name)) => Some((parse(UserId::parse(&id))?, name)),
            _ => None,
        };
        Ok(DiscussionLine { id: parse(MessageId::parse(&r.get::<_, String>(0)?))?, author: r.get(1)?, author_user: user, storage_key: r.get(4)? })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod mention_tests {
    use super::mentions;

    #[test]
    fn a_mention_is_the_whole_username_in_any_case() {
        assert!(mentions("Hi @Robin, look", "robin"));
        assert!(mentions("@robin.", "robin"));
        assert!(mentions("cc @ROBIN", "robin"));
        assert!(!mentions("Hi @robins", "robin"));
        assert!(!mentions("Hi @robin_2", "robin"));
        assert!(!mentions("Hi @robin.lee", "robin"));
        assert!(!mentions("robin without at", "robin"));
    }
}
