//! The employees' mailboxes (`migrations/0036`, R1-R3): the items that wake an
//! employee, per employee and thread, and the state of each thread. What the worker
//! does with them is in `aurelius_application::run::inbox`.

use crate::{StorageError, activity_log, now_ms};
use aurelius_domain::{ConversationId, EmployeeId, EventId, RunId, TaskId, TicketId, UserId};
use aurelius_domain::views::{EmployeeQueueItem, ThreadWaiter};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::collections::HashSet;

/// A queued item: nothing has been made for it yet.
pub const QUEUED: &str = "queued";
/// A run took it.
pub const CLAIMED: &str = "claimed";
/// A handoff that waits for its sender to catch up (R5): nothing makes it a run;
/// `discussion::release_handoffs` delivers it. `payload_json` has what it needs.
pub const HELD: &str = "held";

/// A new item (`kind`, `source`: see the migration).
pub struct NewItem<'a> {
    pub employee: EmployeeId,
    pub thread_key: &'a str,
    pub ticket: Option<TicketId>,
    pub conversation: ConversationId,
    pub kind: &'a str,
    pub source: &'a str,
    pub hop: u32,
    pub payload: &'a serde_json::Value,
    pub login_user: Option<&'a str>,
    pub task: Option<TaskId>,
    pub source_event: EventId,
    pub run: RunId,
}

/// An item as the worker reads it.
#[derive(Debug, Clone)]
pub struct Item {
    pub id: String,
    pub employee: String,
    pub thread_key: String,
    pub conversation: ConversationId,
    pub kind: String,
    pub hop: u32,
    pub payload: serde_json::Value,
    pub login_user: Option<String>,
    pub task: Option<TaskId>,
    pub priority: i64,
    pub run: RunId,
    pub source_event: EventId,
}

pub fn insert(tx: &Transaction<'_>, item: NewItem<'_>) -> Result<(), StorageError> {
    insert_in(tx, item, QUEUED)
}

/// Keep a handoff until its sender has caught up (R5), for `item.employee`.
pub fn hold(tx: &Transaction<'_>, item: NewItem<'_>) -> Result<(), StorageError> {
    insert_in(tx, item, HELD)
}

fn insert_in(tx: &Transaction<'_>, item: NewItem<'_>, state: &str) -> Result<(), StorageError> {
    let id = uuid::Uuid::now_v7().to_string();
    let detail = format!("{} from {}", item.kind, item.source);
    tx.execute(
        "INSERT INTO employee_inbox(id, employee_id, thread_key, ticket_id, conversation_id, kind, source, hop, payload_json,
                                    login_user, task_id, source_event_id, not_before_ms, run_id, created_at_ms, state)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?13, ?15)",
        params![
            id,
            item.employee.to_string(),
            item.thread_key,
            item.ticket.map(|t| t.to_string()),
            item.conversation.to_string(),
            item.kind,
            item.source,
            item.hop,
            serde_json::to_string(item.payload)?,
            item.login_user,
            item.task.map(|t| t.to_string()),
            item.source_event.to_string(),
            now_ms(),
            item.run.to_string(),
            state,
        ],
    )?;
    activity_log::item_step(tx, &id, activity_log::DELIVERY, if state == HELD { "held" } else { "delivered" }, Some(&detail), None);
    Ok(())
}

type ItemRow = (String, String, String, String, String, u32, String, Option<String>, Option<String>, i64, String, String);

fn item_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ItemRow> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?))
}

/// The items that can start now, those to go first in front (then oldest first).
pub fn due(conn: &Connection, now: i64) -> Result<Vec<Item>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT id, employee_id, thread_key, conversation_id, kind, hop, payload_json, login_user, task_id, priority, run_id, source_event_id
         FROM employee_inbox WHERE state = 'queued' AND not_before_ms <= ?1
         ORDER BY priority DESC, created_at_ms, rowid",
    )?;
    let rows = stmt.query_map([now], item_from_row)?;
    collect_items(rows)
}

/// The held handoffs whose sender has caught up: nothing queued for it in the thread
/// and no run of it there still going or waiting to go on.
pub fn releasable_handoffs(conn: &Connection) -> Result<Vec<Item>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT h.id, h.employee_id, h.thread_key, h.conversation_id, h.kind, h.hop, h.payload_json, h.login_user, h.task_id, h.priority, h.run_id, h.source_event_id
         FROM employee_inbox h WHERE h.state = 'held'
           AND NOT EXISTS(SELECT 1 FROM employee_inbox q WHERE q.state = 'queued' AND q.thread_key = h.thread_key AND q.employee_id = json_extract(h.payload_json, '$.held_by'))
           AND NOT EXISTS(SELECT 1 FROM jobs o WHERE o.kind = 'run.chat' AND o.state IN ('leased', 'ready')
                          AND json_extract(o.payload_json, '$.employee_id') = json_extract(h.payload_json, '$.held_by')
                          AND json_extract(o.payload_json, '$.thread_key') = h.thread_key)
         ORDER BY h.created_at_ms, h.rowid",
    )?;
    let rows = stmt.query_map([], item_from_row)?;
    collect_items(rows)
}

/// A held handoff is settled: `done` when delivered, else `cancelled`.
pub fn settle_handoff(tx: &Transaction<'_>, item: &str, state: &str) -> Result<(), StorageError> {
    if tx.execute("UPDATE employee_inbox SET state = ?2 WHERE id = ?1 AND state = 'held'", [item, state])? == 1 {
        activity_log::item_step(tx, item, activity_log::DELIVERY, if state == "done" { "released" } else { "not_delivered" }, None, None);
    }
    Ok(())
}

/// What `employee` has not read in `thread`: items queued for it.
pub fn unread(conn: &Connection, employee: EmployeeId, thread: &str) -> Result<u32, StorageError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM employee_inbox WHERE state = 'queued' AND employee_id = ?1 AND thread_key = ?2",
        [employee.to_string(), thread.to_owned()],
        |r| r.get::<_, i64>(0),
    )? as u32)
}

/// How many handoffs of `run` are held: they count for its limit as delivered ones do.
pub fn held_of_run(conn: &Connection, run: RunId) -> Result<u32, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM employee_inbox WHERE state = 'held' AND json_extract(payload_json, '$.run') = ?1", [run.to_string()], |r| r.get::<_, i64>(0))? as u32)
}

/// The hop of a run (R4): what woke it, 0 for a run nobody woke with a hop.
pub fn run_hop(conn: &Connection, run: RunId) -> Result<u32, StorageError> {
    let hop: Option<i64> = conn
        .query_row("SELECT COALESCE(json_extract(payload_json, '$.hop'), json_extract(payload_json, '$.group.hop'), 0) FROM jobs WHERE id = ?1", [run.to_string()], |r| r.get(0))
        .optional()?;
    Ok(hop.unwrap_or(0) as u32)
}

/// The hop of a task (a delegation): 0 for a task a board member started.
pub fn task_hop(conn: &Connection, task: TaskId) -> Result<u32, StorageError> {
    let hop: Option<Option<i64>> = conn.query_row("SELECT json_extract(payload_json, '$.hop') FROM jobs WHERE id = ?1", [task.to_string()], |r| r.get(0)).optional()?;
    Ok(hop.flatten().unwrap_or(0) as u32)
}

/// When the newest item that woke `run` was written.
pub fn woken_at(conn: &Connection, run: RunId) -> Result<Option<i64>, StorageError> {
    Ok(conn.query_row("SELECT MAX(i.created_at_ms) FROM run_triggers t JOIN employee_inbox i ON i.id = t.item_id WHERE t.run_id = ?1", [run.to_string()], |r| r.get(0))?)
}

fn collect_items(rows: impl Iterator<Item = rusqlite::Result<ItemRow>>) -> Result<Vec<Item>, StorageError> {
    let mut out = Vec::new();
    for row in rows {
        let (id, employee, thread_key, conversation, kind, hop, payload, login_user, task, priority, run, source_event) = row?;
        // A row this build cannot read waits; it never stops the others.
        let (Ok(conversation), Ok(run), Ok(source_event), Ok(payload)) = (ConversationId::parse(&conversation), RunId::parse(&run), EventId::parse(&source_event), serde_json::from_str(&payload)) else {
            tracing::warn!(item = %id, "unreadable inbox item");
            continue;
        };
        out.push(Item { id, employee, thread_key, conversation, kind, hop, payload, login_user, task: task.and_then(|t| TaskId::parse(&t).ok()), priority, run, source_event });
    }
    Ok(out)
}

/// The one-run-per-employee rule (R2): a leased run of the employee keeps its slot,
/// unless it waits for an approval (assumption 3: its thread stays held, the slot does not).
pub fn slot_free(conn: &Connection, employee: &str) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT NOT EXISTS(SELECT 1 FROM jobs o WHERE o.kind = 'run.chat' AND o.state = 'leased' AND json_extract(o.payload_json, '$.employee_id') = ?1
             AND NOT EXISTS (SELECT 1 FROM approvals a WHERE a.run_id = o.id AND a.status = 'pending'))",
        [employee],
        |r| r.get(0),
    )?)
}

/// A run of this thread is unfinished for an item on `login`: going, waiting for an
/// approval, or back in line on the same login. One board member's limit does not
/// stop another's work in the same chat (MU3-11).
pub fn thread_held(conn: &Connection, employee: &str, thread: &str, login: Option<&str>) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs o JOIN runs r ON r.id = o.id WHERE o.kind = 'run.chat'
             AND (o.state = 'leased' OR (o.state = 'ready' AND r.login_user IS ?3))
             AND json_extract(o.payload_json, '$.employee_id') = ?1 AND json_extract(o.payload_json, '$.thread_key') = ?2)",
        params![employee, thread, login],
        |r| r.get(0),
    )?)
}

/// The thread is parked on a question (R3) that still stands: the task that asked it
/// waits for its board member. A task that was cancelled or went on frees the thread.
pub fn is_parked(conn: &Connection, employee: &str, thread: &str) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM employee_threads h JOIN tasks t ON t.id = h.parked_task_id
           WHERE h.employee_id = ?1 AND h.thread_key = ?2 AND h.state = 'parked' AND t.status = 'waiting' AND t.waiting_reason = 'input')",
        [employee, thread],
        |r| r.get(0),
    )?)
}

/// The conversation of the `resume` item that waits in the thread, if one does.
pub fn resume_conversation(conn: &Connection, employee: &str, thread: &str) -> Result<Option<String>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT conversation_id FROM employee_inbox WHERE employee_id = ?1 AND thread_key = ?2 AND state = 'queued' AND kind = 'resume' ORDER BY created_at_ms, rowid LIMIT 1",
            [employee, thread],
            |r| r.get(0),
        )
        .optional()?)
}

/// Park the thread on the question of `run`, which ended `task`'s run (R3).
pub fn park(tx: &Transaction<'_>, employee: &str, thread: &str, task: TaskId, run: RunId) -> Result<(), StorageError> {
    let at = now_ms();
    tx.execute(
        "INSERT INTO employee_threads(employee_id, thread_key, state, parked_task_id, parked_run_id, parked_at_ms, created_at_ms)
         VALUES (?1, ?2, 'parked', ?3, ?4, ?5, ?5)
         ON CONFLICT(employee_id, thread_key) DO UPDATE SET state = 'parked', parked_task_id = ?3, parked_run_id = ?4, parked_at_ms = ?5",
        params![employee, thread, task.to_string(), run.to_string(), at],
    )?;
    activity_log::thread_step(tx, "parked", employee, thread, Some(run));
    Ok(())
}

/// The thread is open again: a `resume` item is being processed.
pub fn unpark(tx: &Transaction<'_>, employee: &str, thread: &str) -> Result<(), StorageError> {
    let opened = tx.execute(
        "UPDATE employee_threads SET state = 'open', parked_task_id = NULL, parked_run_id = NULL, parked_at_ms = NULL WHERE employee_id = ?1 AND thread_key = ?2 AND state = 'parked'",
        [employee, thread],
    )?;
    if opened == 1 {
        activity_log::thread_step(tx, "unparked", employee, thread, None);
    }
    Ok(())
}

/// The employee and thread of a claimed run, from its job.
pub fn job_thread(conn: &Connection, run: RunId) -> Result<Option<(String, String)>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT json_extract(payload_json, '$.employee_id'), json_extract(payload_json, '$.thread_key') FROM jobs WHERE id = ?1",
            [run.to_string()],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .optional()?
        .and_then(|(e, t)| Some((e?, t?))))
}

/// The employee's own conversation for a thread (R6), hidden from every list and
/// inheriting the settings of its main chat. `None` for an employee without a chat.
pub fn thread_conversation(tx: &Transaction<'_>, employee: EmployeeId, thread: &str, title: &str) -> Result<Option<ConversationId>, StorageError> {
    let found: Option<String> = tx
        .query_row(
            "SELECT conversation_id FROM employee_threads WHERE employee_id = ?1 AND thread_key = ?2 AND conversation_id IS NOT NULL",
            [employee.to_string(), thread.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    let id = match found.and_then(|c| ConversationId::parse(&c).ok()) {
        Some(id) => id,
        None => {
            let id = ConversationId::new();
            let made = tx.execute(
                "INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms, quick_answer, thread_key)
                 SELECT ?1, 'employee', NULL, e.id, ?3, 1, ?4, 0, ?5 FROM employees e WHERE e.id = ?2
                   AND EXISTS (SELECT 1 FROM conversations c WHERE c.employee_id = e.id AND c.kind = 'employee' AND c.quick_answer = 0 AND c.thread_key IS NULL)",
                params![id.to_string(), employee.to_string(), title, now_ms(), thread],
            )?;
            if made != 1 {
                return Ok(None);
            }
            tx.execute(
                "INSERT INTO employee_threads(employee_id, thread_key, conversation_id, created_at_ms) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(employee_id, thread_key) DO UPDATE SET conversation_id = ?3",
                params![employee.to_string(), thread, id.to_string(), now_ms()],
            )?;
            id
        }
    };
    follow_main_chat(tx, id, employee)?;
    Ok(Some(id))
}

/// The switches of the employee's main chat (mode, browser, screen, model and effort)
/// count for `conversation` too: its threads and its chats with board members.
pub fn follow_main_chat(tx: &Transaction<'_>, conversation: ConversationId, employee: EmployeeId) -> Result<(), StorageError> {
    let main = crate::employees::main_chat_of("?2");
    tx.execute(
        &format!(
            "UPDATE conversations SET
                agent_mode = COALESCE((SELECT agent_mode FROM conversations WHERE id = {main}), agent_mode),
                browser_allowed = COALESCE((SELECT browser_allowed FROM conversations WHERE id = {main}), browser_allowed),
                screen_allowed = COALESCE((SELECT screen_allowed FROM conversations WHERE id = {main}), screen_allowed),
                run_json = (SELECT run_json FROM conversations WHERE id = {main})
             WHERE id = ?1"
        ),
        [conversation.to_string(), employee.to_string()],
    )?;
    Ok(())
}

/// The thread key of a conversation that is a thread's own, if it is one.
pub fn conversation_thread(conn: &Connection, conversation: ConversationId) -> Result<Option<String>, StorageError> {
    Ok(conn.query_row("SELECT thread_key FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0)).optional()?.flatten())
}

/// These items were taken by `run`: each points at it, and `run_triggers` keeps which it answers.
pub fn claim(tx: &Transaction<'_>, items: &[String], run: RunId) -> Result<(), StorageError> {
    let at = now_ms();
    for item in items {
        if tx.execute("UPDATE employee_inbox SET state = 'claimed', run_id = ?2 WHERE id = ?1 AND state = 'queued'", params![item, run.to_string()])? == 1 {
            activity_log::item_step(tx, item, activity_log::QUEUE, "claimed", None, Some(run));
        }
        tx.execute("INSERT OR IGNORE INTO run_triggers(run_id, item_id, created_at_ms) VALUES (?1, ?2, ?3)", params![run.to_string(), item, at])?;
    }
    Ok(())
}

/// The run ended: what it answered is done.
pub fn finish_run(tx: &Transaction<'_>, run: RunId) -> Result<(), StorageError> {
    tx.execute("UPDATE employee_inbox SET state = 'done' WHERE run_id = ?1 AND state = 'claimed'", [run.to_string()])?;
    Ok(())
}

/// Take the queued item that would become `run` out of the queue; its payload when there was one.
pub fn cancel(tx: &Transaction<'_>, run: RunId) -> Result<Option<Item>, StorageError> {
    let mut stmt = tx.prepare(
        "SELECT id, employee_id, thread_key, conversation_id, kind, hop, payload_json, login_user, task_id, priority, run_id, source_event_id
         FROM employee_inbox WHERE run_id = ?1 AND state = 'queued'",
    )?;
    let row = stmt.query_row([run.to_string()], item_from_row).optional()?;
    let Some((id, employee, thread_key, conversation, kind, hop, payload, login_user, task, priority, run_text, source_event)) = row else { return Ok(None) };
    drop(stmt);
    tx.execute("UPDATE employee_inbox SET state = 'cancelled' WHERE id = ?1", [&id])?;
    activity_log::item_step(tx, &id, activity_log::QUEUE, "cancelled", None, None);
    let conversation = ConversationId::parse(&conversation).map_err(|e| StorageError::Rejected(e.to_string()))?;
    let run = RunId::parse(&run_text).map_err(|e| StorageError::Rejected(e.to_string()))?;
    let source_event = EventId::parse(&source_event).map_err(|e| StorageError::Rejected(e.to_string()))?;
    Ok(Some(Item { id, employee, thread_key, conversation, kind, hop, payload: serde_json::from_str(&payload)?, login_user, task: task.and_then(|t| TaskId::parse(&t).ok()), priority, run, source_event }))
}

/// Let a queued item go first: ahead of what else waits and without a delay.
pub fn hurry(tx: &Transaction<'_>, run: RunId) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE employee_inbox SET priority = (SELECT COALESCE(MAX(priority), 0) + 1 FROM employee_inbox WHERE state = 'queued'), not_before_ms = MIN(not_before_ms, ?2)
         WHERE run_id = ?1 AND state = 'queued'",
        params![run.to_string(), now_ms()],
    )?;
    activity_log::queued_step(tx, &run.to_string(), "moved_up", None);
    Ok(())
}

/// The task a queued item counts for (a run's own link is on the run).
pub fn link_task(tx: &Transaction<'_>, run: RunId, task: TaskId) -> Result<(), StorageError> {
    tx.execute("UPDATE employee_inbox SET task_id = ?2 WHERE run_id = ?1 AND state = 'queued'", [run.to_string(), task.to_string()])?;
    Ok(())
}

/// The conversation of a queued item that will become `run`.
pub fn queued_conversation(conn: &Connection, run: RunId) -> Result<Option<ConversationId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT conversation_id FROM employee_inbox WHERE run_id = ?1 AND state = 'queued'", [run.to_string()], |r| r.get(0)).optional()?;
    Ok(id.and_then(|c| ConversationId::parse(&c).ok()))
}

/// Whether a queued item will become `run`.
pub fn is_queued(conn: &Connection, run: RunId) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM employee_inbox WHERE run_id = ?1 AND state = 'queued')", [run.to_string()], |r| r.get(0))?)
}

/// The marking of a group-chat wake, from an item (cancelled or queued) of `run`.
pub fn group_marking(conn: &Connection, run: RunId) -> Result<Option<String>, StorageError> {
    Ok(conn.query_row("SELECT json_extract(payload_json, '$.group') FROM employee_inbox WHERE run_id = ?1 ORDER BY rowid DESC LIMIT 1", [run.to_string()], |r| r.get(0)).optional()?.flatten())
}

/// Queued items of a conversation, the id its run will have with the message it answers.
pub fn queued_in(conn: &Connection, conversation: ConversationId) -> Result<Vec<(RunId, Option<String>)>, StorageError> {
    // A store before schema 36 has no inbox.
    if !conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'employee_inbox')", [], |r| r.get::<_, bool>(0))? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT run_id, json_extract(payload_json, '$.reply_to') FROM employee_inbox WHERE conversation_id = ?1 AND state = 'queued' ORDER BY created_at_ms, rowid",
    )?;
    let rows = stmt.query_map([conversation.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (run, message) = row?;
        if let Ok(run) = RunId::parse(&run) {
            out.push((run, message));
        }
    }
    Ok(out)
}

/// The thread key of a ticket, or of a conversation (`channel:`, `chat:`, a ticket's
/// discussion), the way `thread_of` in the worker makes them. `None`: no such place.
pub fn thread_of_place(conn: &Connection, ticket: Option<TicketId>, conversation: Option<ConversationId>) -> Result<Option<String>, StorageError> {
    if let Some(ticket) = ticket {
        return Ok(Some(format!("ticket:{ticket}")));
    }
    let Some(conversation) = conversation else { return Ok(None) };
    if let Some(ticket) = crate::discussions::ticket_of_conversation(conn, conversation)? {
        return Ok(Some(format!("ticket:{ticket}")));
    }
    let row: Option<(String, Option<String>)> = conn.query_row("SELECT kind, thread_key FROM conversations WHERE id = ?1", [conversation.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    Ok(row.map(|(kind, key)| key.unwrap_or_else(|| if kind == "group" { format!("channel:{conversation}") } else { format!("chat:{conversation}") })))
}

/// Who waits to pick up `thread` (U3): an employee with queued items there, how many
/// threads of its go first, and what else holds it back.
pub fn waiters(conn: &Connection, thread: &str, now: i64) -> Result<Vec<ThreadWaiter>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT i.employee_id, e.name, MIN(i.not_before_ms), MAX(i.last_error_code = 'quota')
         FROM employee_inbox i JOIN employees e ON e.id = i.employee_id
         WHERE i.state = 'queued' AND i.thread_key = ?1 GROUP BY i.employee_id ORDER BY MIN(i.created_at_ms)",
    )?;
    let rows: Vec<(String, String, i64, Option<i64>)> = stmt.query_map([thread], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let queue = due(conn, now)?;
    let mut out = Vec::new();
    for (employee, employee_name, not_before, quota) in rows {
        let Ok(employee_id) = EmployeeId::parse(&employee) else { continue };
        // Items of one other thread become one run: they count once.
        let ahead: HashSet<&str> = queue.iter().filter(|i| i.employee == employee).take_while(|i| i.thread_key != thread).map(|i| i.thread_key.as_str()).collect();
        let busy = u32::from(!slot_free(conn, &employee)?);
        let reason = match () {
            _ if is_parked(conn, &employee, thread)? => Some("question"),
            _ if not_before > now && quota == Some(1) => Some("limit"),
            _ => None,
        };
        out.push(ThreadWaiter { employee_id, employee_name, behind: ahead.len() as u32 + busy, reason: reason.map(str::to_owned) });
    }
    Ok(out)
}

/// A due item whose employee has its slot free; what only the machine can hold back.
const FREE_TO_START: &str = "i.state = 'queued' AND i.not_before_ms <= ?1
    AND NOT EXISTS(SELECT 1 FROM jobs o WHERE o.kind = 'run.chat' AND o.state = 'leased' AND json_extract(o.payload_json, '$.employee_id') = i.employee_id
        AND NOT EXISTS (SELECT 1 FROM approvals a WHERE a.run_id = o.id AND a.status = 'pending'))";

/// Items waiting for the machine (due, free to start): the count `load` reports.
pub fn held(conn: &Connection, now: i64) -> Result<u32, StorageError> {
    Ok(conn.query_row(&format!("SELECT COUNT(*) FROM employee_inbox i WHERE {FREE_TO_START}"), [now], |r| r.get::<_, i64>(0))? as u32)
}

/// Mark due items that are held back with `reason`, once per change of reason.
/// Their run ids (the id their `run.waiting` is about) and conversations.
pub fn mark_held(tx: &Transaction<'_>, reason: &str, now: i64) -> Result<Vec<(String, String)>, StorageError> {
    let mut stmt = tx.prepare(&format!("SELECT i.run_id, i.conversation_id FROM employee_inbox i WHERE {FREE_TO_START} AND i.last_error_code IS NOT ?2"))?;
    let rows: Vec<(String, String)> = stmt.query_map(params![now, reason], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    for (run, _) in &rows {
        tx.execute("UPDATE employee_inbox SET last_error_code = ?2 WHERE run_id = ?1 AND state = 'queued'", params![run, reason])?;
        activity_log::queued_step(tx, run, "held_back", Some(reason));
    }
    Ok(rows)
}

/// Hold an item until `until_ms` for `reason` (its login is at a usage limit).
pub fn defer(tx: &Transaction<'_>, item: &str, until_ms: i64, reason: &str) -> Result<(), StorageError> {
    // A deferred item is looked at again each pass until it is due: once is enough.
    let changed = tx.execute(
        "UPDATE employee_inbox SET not_before_ms = ?2, last_error_code = ?3 WHERE id = ?1 AND state = 'queued' AND (not_before_ms IS NOT ?2 OR last_error_code IS NOT ?3)",
        params![item, until_ms, reason],
    )?;
    if changed == 1 {
        activity_log::item_step(tx, item, activity_log::QUEUE, "deferred", Some(reason), None);
    }
    Ok(())
}

/// Items held for `reason` on `login` (none: every login) are due now.
pub fn wake(tx: &Transaction<'_>, reason: &str, login: Option<&str>, now: i64) -> Result<usize, StorageError> {
    let mut held = tx.prepare("SELECT id FROM employee_inbox WHERE state = 'queued' AND last_error_code = ?1 AND (?2 IS NULL OR login_user = ?2)")?;
    let items: Vec<String> = held.query_map(params![reason, login], |r| r.get(0))?.collect::<Result<_, _>>()?;
    drop(held);
    for item in &items {
        activity_log::item_step(tx, item, activity_log::QUEUE, "due_again", Some(reason), None);
    }
    Ok(tx.execute(
        "UPDATE employee_inbox SET not_before_ms = ?3, last_error_code = NULL WHERE state = 'queued' AND last_error_code = ?1 AND (?2 IS NULL OR login_user = ?2)",
        params![reason, login, now],
    )?)
}

/// The furthest a limit on `login` holds a parked run of `provider`, if it does.
pub fn quota_until(conn: &Connection, provider: &str, login: &str, now: i64) -> Result<Option<i64>, StorageError> {
    Ok(conn.query_row(
        "SELECT MAX(j.not_before_ms) FROM jobs j JOIN runs r ON r.id = j.id
         WHERE j.kind = 'run.chat' AND j.state = 'ready' AND j.last_error_code = 'quota' AND j.not_before_ms > ?1 AND r.runtime_kind = ?2 AND r.login_user = ?3",
        params![now, provider, login],
        |r| r.get(0),
    )?)
}

/// `ticket.use_my_login`: what is queued for the ticket runs on `user`'s login now.
pub fn move_ticket_login(tx: &Transaction<'_>, ticket: TicketId, user: &str, now: i64) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE employee_inbox SET login_user = ?2, not_before_ms = CASE WHEN last_error_code = 'quota' THEN ?3 ELSE not_before_ms END,
             last_error_code = CASE WHEN last_error_code = 'quota' THEN NULL ELSE last_error_code END
         WHERE state = 'queued' AND (ticket_id = ?1 OR task_id IN (SELECT id FROM tasks WHERE ticket_id = ?1))",
        params![ticket.to_string(), user, now],
    )?;
    Ok(())
}

/// What is in `employee`'s mailbox now (U6): what it works on, what waits in line, and
/// held handoffs. Someone else's private chat does not exist for `reader`.
pub fn queue_of(conn: &Connection, employee: EmployeeId, reader: Option<UserId>, now: i64) -> Result<Vec<EmployeeQueueItem>, StorageError> {
    type Row = (String, String, String, String, String, u32, i64, i64, Option<String>, String);
    let mut stmt = conn.prepare(
        "SELECT i.id, i.kind, i.source, i.state, i.thread_key, i.hop, i.created_at_ms, i.not_before_ms, i.last_error_code, i.run_id
         FROM employee_inbox i LEFT JOIN conversations c ON c.id = i.conversation_id
         WHERE i.employee_id = ?1 AND (c.board_member_id IS NULL OR c.board_member_id = ?2)
           AND (i.state IN ('queued', 'held')
                OR (i.state = 'claimed' AND EXISTS(SELECT 1 FROM runs r WHERE r.id = i.run_id AND r.status IN ('created','starting','running','waiting','cancelling'))))
         ORDER BY CASE i.state WHEN 'claimed' THEN 0 WHEN 'queued' THEN 1 ELSE 2 END, i.priority DESC, i.created_at_ms, i.rowid",
    )?;
    let rows: Vec<Row> = stmt
        .query_map(params![employee.to_string(), reader.map(|u| u.to_string())], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::new();
    for (id, kind, source, state, thread_key, hop, created_at_ms, not_before_ms, error, run) in rows {
        let waits_for = match state.as_str() {
            HELD => Some("handoff"),
            CLAIMED => None,
            _ if kind != "resume" && is_parked(conn, &employee.to_string(), &thread_key)? => Some("question"),
            _ if not_before_ms > now && error.as_deref() == Some("quota") => Some("limit"),
            _ if not_before_ms > now => Some("later"),
            _ if error.is_some() => Some("machine"),
            _ => None,
        };
        let run_id = if state == CLAIMED { RunId::parse(&run).ok() } else { None };
        let place = activity_log::place_of(conn, &thread_key)?;
        out.push(EmployeeQueueItem { id, kind, source, state, thread_key, place, run_id, hop, created_at_ms, not_before_ms, waits_for: waits_for.map(str::to_owned) });
    }
    Ok(out)
}
