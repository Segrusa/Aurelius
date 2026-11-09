//! Messages an employee scheduled (`migrations/0038`, R7). What delivers them is in
//! `aurelius_application::conversation::scheduled`.

use crate::{StorageError, now_ms};
use aurelius_domain::scheduler::scheduled_message::{CANCELLED, SCHEDULED};
use aurelius_domain::views::ScheduledMessageView;
use aurelius_domain::{ConversationId, EmployeeId, MessageId, RunId, ScheduledMessageId, TicketId, UserId};
use rusqlite::{Connection, Transaction, params};

/// A message to schedule. `discussion` a ticket's or a channel, else `chat` an employee's own.
pub struct NewScheduled<'a> {
    pub from: EmployeeId,
    pub to: EmployeeId,
    pub content: &'a str,
    pub discussion: Option<ConversationId>,
    pub chat: Option<ConversationId>,
    pub thread_key: &'a str,
    pub ticket: Option<TicketId>,
    pub thread_parent: Option<MessageId>,
    pub place: &'a str,
    pub hop: u32,
    pub login_user: Option<UserId>,
    pub planned_by_run: RunId,
    pub due_at_ms: i64,
}

/// A message the dispatcher delivers.
#[derive(Debug, Clone)]
pub struct Due {
    pub id: ScheduledMessageId,
    pub from: EmployeeId,
    pub to: EmployeeId,
    pub content: String,
    pub discussion: Option<ConversationId>,
    pub chat: Option<ConversationId>,
    pub thread_parent: Option<MessageId>,
    pub hop: u32,
    pub login_user: Option<UserId>,
}

pub fn insert(tx: &Transaction<'_>, new: NewScheduled<'_>) -> Result<ScheduledMessageId, StorageError> {
    let id = ScheduledMessageId::new();
    tx.execute(
        "INSERT INTO scheduled_messages(id, from_employee_id, to_employee_id, content, discussion_id, chat_id, thread_key, ticket_id,
                                        thread_parent, place, hop, login_user, planned_by_run, state, due_at_ms, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            id.to_string(),
            new.from.to_string(),
            new.to.to_string(),
            new.content,
            new.discussion.map(|c| c.to_string()),
            new.chat.map(|c| c.to_string()),
            new.thread_key,
            new.ticket.map(|t| t.to_string()),
            new.thread_parent.map(|m| m.to_string()),
            new.place,
            new.hop,
            new.login_user.map(|u| u.to_string()),
            new.planned_by_run.to_string(),
            SCHEDULED,
            new.due_at_ms,
            now_ms(),
        ],
    )?;
    crate::activity_log::scheduled_step(tx, id, "scheduled", None);
    Ok(id)
}

/// What came due at `now`, the longest overdue first.
pub fn due(conn: &Connection, now: i64) -> Result<Vec<Due>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT id, from_employee_id, to_employee_id, content, discussion_id, chat_id, thread_parent, hop, login_user
         FROM scheduled_messages WHERE state = 'scheduled' AND due_at_ms <= ?1 ORDER BY due_at_ms, rowid",
    )?;
    let rows = stmt.query_map([now], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, u32>(7)?,
            r.get::<_, Option<String>>(8)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, from, to, content, discussion, chat, thread_parent, hop, login_user) = row?;
        // A row this build cannot read waits; it never stops the others.
        let (Ok(parsed), Ok(from), Ok(to)) = (ScheduledMessageId::parse(&id), EmployeeId::parse(&from), EmployeeId::parse(&to)) else {
            tracing::warn!(message = %id, "unreadable scheduled message");
            continue;
        };
        out.push(Due {
            id: parsed,
            from,
            to,
            content,
            discussion: discussion.and_then(|c| ConversationId::parse(&c).ok()),
            chat: chat.and_then(|c| ConversationId::parse(&c).ok()),
            thread_parent: thread_parent.and_then(|m| MessageId::parse(&m).ok()),
            hop,
            login_user: login_user.and_then(|u| UserId::parse(&u).ok()),
        });
    }
    Ok(out)
}

/// It went out, or it could not: `state` with the run it started or why not.
pub fn settle(tx: &Transaction<'_>, id: ScheduledMessageId, state: &str, run: Option<RunId>, outcome: Option<&str>) -> Result<(), StorageError> {
    let settled = tx.execute(
        "UPDATE scheduled_messages SET state = ?2, run_id = ?3, outcome = ?4, settled_at_ms = ?5 WHERE id = ?1 AND state = 'scheduled'",
        params![id.to_string(), state, run.map(|r| r.to_string()), outcome, now_ms()],
    )?;
    if settled == 1 {
        crate::activity_log::scheduled_step(tx, id, &format!("scheduled_{state}"), None);
    }
    Ok(())
}

/// A board member takes it back. False when it is no longer waiting, or `reader` may not see it.
pub fn cancel(tx: &Transaction<'_>, id: ScheduledMessageId, reader: Option<UserId>) -> Result<bool, StorageError> {
    let cancelled = tx.execute(
        &format!(
            "UPDATE scheduled_messages SET state = '{CANCELLED}', settled_at_ms = ?2, outcome = NULL WHERE id = ?1 AND state = 'scheduled'
               AND NOT EXISTS(SELECT 1 FROM conversations c WHERE c.id = chat_id AND c.board_member_id IS NOT NULL AND c.board_member_id IS NOT ?3)"
        ),
        params![id.to_string(), now_ms(), reader.map(|u| u.to_string())],
    )? == 1;
    if cancelled {
        crate::activity_log::scheduled_step(tx, id, "scheduled_cancelled", None);
    }
    Ok(cancelled)
}

/// What `employee` has waiting.
pub fn pending_of(conn: &Connection, employee: EmployeeId) -> Result<u32, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM scheduled_messages WHERE from_employee_id = ?1 AND state = 'scheduled'", [employee.to_string()], |r| r.get::<_, i64>(0))? as u32)
}

/// How many messages `run` has scheduled.
pub fn of_run(conn: &Connection, run: RunId) -> Result<u32, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM scheduled_messages WHERE planned_by_run = ?1", [run.to_string()], |r| r.get::<_, i64>(0))? as u32)
}

const VIEW: &str = "SELECT s.id, s.from_employee_id, a.name, s.to_employee_id, b.name, s.content, s.created_at_ms, s.due_at_ms, s.state, s.place, s.ticket_id,
        CASE WHEN s.thread_key LIKE 'channel:%' THEN s.discussion_id END, u.username, s.outcome, s.run_id
    FROM scheduled_messages s
    JOIN employees a ON a.id = s.from_employee_id
    JOIN employees b ON b.id = s.to_employee_id
    LEFT JOIN users u ON u.id = s.login_user
    LEFT JOIN conversations c ON c.id = s.chat_id";

/// Someone else's private chat stays out of what `reader` sees (`employee_chats::hidden_from`).
const VISIBLE: &str = "(c.board_member_id IS NULL OR c.board_member_id = ?1)";

fn view(r: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduledMessageView> {
    fn id<T, E>(text: Option<String>, parse: impl Fn(&str) -> Result<T, E>) -> Option<T> {
        text.and_then(|t| parse(&t).ok())
    }
    let bad = |i: usize, e: aurelius_domain::uuid_error::Error| rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e));
    Ok(ScheduledMessageView {
        id: ScheduledMessageId::parse(&r.get::<_, String>(0)?).map_err(|e| bad(0, e))?,
        from_employee: EmployeeId::parse(&r.get::<_, String>(1)?).map_err(|e| bad(1, e))?,
        from_name: r.get(2)?,
        to_employee: EmployeeId::parse(&r.get::<_, String>(3)?).map_err(|e| bad(3, e))?,
        to_name: r.get(4)?,
        text: r.get(5)?,
        created_at_ms: r.get(6)?,
        due_at_ms: r.get(7)?,
        state: r.get(8)?,
        place: r.get(9)?,
        ticket_id: id(r.get(10)?, TicketId::parse),
        channel_id: id(r.get(11)?, ConversationId::parse),
        login_name: r.get(12)?,
        outcome: r.get(13)?,
        run_id: id(r.get(14)?, RunId::parse),
    })
}

/// What `reader` may see: every message waiting, soonest first, then the latest
/// `settled` that went out or did not.
pub fn list(conn: &Connection, reader: Option<UserId>, settled: u32) -> Result<Vec<ScheduledMessageView>, StorageError> {
    let reader = reader.map(|u| u.to_string());
    let mut out = Vec::new();
    let mut waiting = conn.prepare(&format!("{VIEW} WHERE {VISIBLE} AND s.state = 'scheduled' ORDER BY s.due_at_ms, s.rowid"))?;
    for row in waiting.query_map(params![reader], view)? {
        out.push(row?);
    }
    let mut done = conn.prepare(&format!("{VIEW} WHERE {VISIBLE} AND s.state <> 'scheduled' ORDER BY s.settled_at_ms DESC, s.rowid DESC LIMIT ?2"))?;
    for row in done.query_map(params![reader, settled], view)? {
        out.push(row?);
    }
    Ok(out)
}

/// What waits to go out in one thread, soonest first.
pub fn of_thread(conn: &Connection, thread: &str, reader: Option<UserId>) -> Result<Vec<ScheduledMessageView>, StorageError> {
    let mut stmt = conn.prepare(&format!("{VIEW} WHERE {VISIBLE} AND s.state = 'scheduled' AND s.thread_key = ?2 ORDER BY s.due_at_ms, s.rowid"))?;
    let rows = stmt.query_map(params![reader.map(|u| u.to_string()), thread], view)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
