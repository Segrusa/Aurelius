//! A board member's lasting chat with an employee (`migrations/0037`) and the tickets
//! made from it. What is done with them is in `aurelius_application::conversation::
//! employee_chat`.

use crate::{StorageError, now_ms};
use aurelius_domain::conversation::TICKET_NOTE_KIND;
use aurelius_domain::views::EmployeeChatView;
use aurelius_domain::{ConversationId, EmployeeId, TicketId, UserId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// Who a chat is between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    pub employee: EmployeeId,
    pub user: UserId,
    pub username: String,
}

/// A ticket made from a chat that reached Review and the chat has not heard of yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingUpdate {
    pub conversation: ConversationId,
    pub ticket: TicketId,
    pub number: u32,
    pub title: String,
}

fn bad(e: impl std::fmt::Display) -> StorageError {
    StorageError::Rejected(e.to_string())
}

/// The chat of `user` with `employee`, made when there is none yet; none without the
/// employee. It starts with the mode of the employee's own workspace.
pub fn ensure(tx: &Transaction<'_>, employee: EmployeeId, user: UserId) -> Result<Option<ConversationId>, StorageError> {
    let main = crate::employees::main_chat_of("m.id");
    tx.execute(
        &format!(
            "INSERT OR IGNORE INTO conversations(id, kind, employee_id, title, version, created_at_ms, agent_mode, board_member_id)
             SELECT ?1, 'employee', m.id, m.name, 1, ?2, c.agent_mode, ?3
             FROM employees m JOIN conversations c ON c.id = {main} WHERE m.id = ?4"
        ),
        params![ConversationId::new().to_string(), now_ms(), user.to_string(), employee.to_string()],
    )?;
    let id: Option<String> = tx
        .query_row("SELECT id FROM conversations WHERE employee_id = ?1 AND board_member_id = ?2", [employee.to_string(), user.to_string()], |r| r.get(0))
        .optional()?;
    id.map(|c| ConversationId::parse(&c).map_err(bad)).transpose()
}

/// The chat `conversation` is, if it is one.
pub fn of_conversation(conn: &Connection, conversation: ConversationId) -> Result<Option<Chat>, StorageError> {
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT c.employee_id, c.board_member_id, u.username FROM conversations c JOIN users u ON u.id = c.board_member_id WHERE c.id = ?1 AND c.kind = 'employee'",
            [conversation.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    row.map(|(employee, user, username)| Ok(Chat { employee: EmployeeId::parse(&employee).map_err(bad)?, user: UserId::parse(&user).map_err(bad)?, username })).transpose()
}

/// What only `user` reads: another board member's chat with an employee.
pub fn hidden_from(conn: &Connection, conversation: ConversationId, user: Option<UserId>) -> Result<bool, StorageError> {
    let owner: Option<Option<String>> = conn.query_row("SELECT board_member_id FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0)).optional()?;
    Ok(owner.flatten().is_some_and(|owner| user.is_none_or(|u| u.to_string() != owner)))
}

/// The same for anything that belongs to such a chat and is named by its own id: the
/// conversation, a run in it, a message in it or an approval one of its runs asked.
/// The live events, the streamed text and the reads keyed by a run or a message all
/// ask this, so another board member's private chat stays out of each of them.
pub fn hides(conn: &Connection, id: &str, user: Option<UserId>) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM conversations c
          WHERE c.board_member_id IS NOT NULL AND (?2 IS NULL OR c.board_member_id <> ?2)
            AND c.id IN (
              SELECT ?1
              UNION ALL SELECT conversation_id FROM runs WHERE id = ?1
              UNION ALL SELECT conversation_id FROM messages WHERE id = ?1
              UNION ALL SELECT r.conversation_id FROM approvals a JOIN runs r ON r.id = a.run_id WHERE a.id = ?1))",
        params![id, user.map(|u| u.to_string())],
        |r| r.get(0),
    )?)
}

/// How much of the latest message a list of chats shows.
const PREVIEW_CHARS: usize = 120;

/// The chats of `user` that have something in them, the latest first; `read` gives the text of a blob.
pub fn list(conn: &Connection, user: UserId, read: impl Fn(&str) -> Option<String>) -> Result<Vec<EmployeeChatView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, e.id, e.name,
                (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id AND m.status = 'complete' AND m.ordinal > COALESCE(r.read_through, 0)
                   AND (m.role = 'assistant' OR json_extract(m.meta_json, '$.kind') = ?2)),
                (SELECT MAX(m.created_at_ms) FROM messages m WHERE m.conversation_id = c.id AND (m.role <> 'tool' OR json_extract(m.meta_json, '$.kind') = ?2)) AS last,
                (SELECT b.storage_key FROM messages m JOIN blobs b ON b.id = m.content_blob_id
                  WHERE m.conversation_id = c.id AND m.deleted_at_ms IS NULL AND (m.role <> 'tool' OR json_extract(m.meta_json, '$.kind') = ?2)
                  ORDER BY m.ordinal DESC LIMIT 1)
         FROM conversations c JOIN employees e ON e.id = c.employee_id
         LEFT JOIN discussion_reads r ON r.conversation_id = c.id AND r.user_id = ?1
         WHERE c.board_member_id = ?1 AND e.status <> 'archived' AND last IS NOT NULL
         ORDER BY last DESC, c.id",
    )?;
    let rows = stmt.query_map([user.to_string(), TICKET_NOTE_KIND.to_owned()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?, r.get::<_, Option<String>>(5)?)))?;
    rows.map(|row| {
        let (conversation, employee, employee_name, unread, last_activity_ms, key) = row?;
        let preview = key.and_then(|k| read(&k)).map(|text| aurelius_domain::group_chat::shortened(&text.split_whitespace().collect::<Vec<_>>().join(" "), PREVIEW_CHARS)).filter(|t| !t.is_empty());
        Ok(EmployeeChatView { conversation_id: ConversationId::parse(&conversation).map_err(bad)?, employee_id: EmployeeId::parse(&employee).map_err(bad)?, employee_name, unread: unread as u32, last_activity_ms, preview })
    })
    .collect()
}

/// `ticket` was made from `conversation`.
pub fn link_ticket(tx: &Transaction<'_>, ticket: TicketId, conversation: ConversationId) -> Result<(), StorageError> {
    tx.execute("INSERT OR IGNORE INTO chat_tickets(ticket_id, conversation_id, created_at_ms) VALUES (?1, ?2, ?3)", params![ticket.to_string(), conversation.to_string(), now_ms()])?;
    Ok(())
}

/// `ticket` reached Review (once per arrival): if it came from a chat, that chat is owed an update.
pub fn reached_review(tx: &Transaction<'_>, ticket: TicketId) -> Result<(), StorageError> {
    tx.execute("UPDATE chat_tickets SET review_arrivals = review_arrivals + 1 WHERE ticket_id = ?1", [ticket.to_string()])?;
    Ok(())
}

/// Whether any chat is owed an update: read at every pass of the dispatcher.
pub fn updates_waiting(conn: &Connection) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM chat_tickets WHERE review_arrivals > reviews_told)", [], |r| r.get(0))?)
}

/// What chats are owed, per conversation. A paused employee's wait until it is back.
pub fn waiting_updates(conn: &Connection) -> Result<Vec<WaitingUpdate>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, k.id, k.number, k.title
         FROM chat_tickets l JOIN conversations c ON c.id = l.conversation_id JOIN employees e ON e.id = c.employee_id JOIN tickets k ON k.id = l.ticket_id
         WHERE l.review_arrivals > l.reviews_told AND e.status = 'active'
         ORDER BY c.id, k.number",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, u32>(2)?, r.get::<_, String>(3)?)))?;
    rows.map(|row| {
        let (conversation, ticket, number, title) = row?;
        Ok(WaitingUpdate { conversation: ConversationId::parse(&conversation).map_err(bad)?, ticket: TicketId::parse(&ticket).map_err(bad)?, number, title })
    })
    .collect()
}

/// The chat heard of every arrival of `ticket` so far: none of them is delivered again.
pub fn told(tx: &Transaction<'_>, ticket: TicketId) -> Result<(), StorageError> {
    tx.execute("UPDATE chat_tickets SET reviews_told = review_arrivals WHERE ticket_id = ?1", [ticket.to_string()])?;
    Ok(())
}
