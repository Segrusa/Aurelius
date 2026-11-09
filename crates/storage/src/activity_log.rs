//! The activity log (`migrations/0039`, R8): every delivery, queue step and run step of
//! an employee as a row, findable per thread, employee and run. A row has no foreign
//! keys and holds no message text or tool input, only what happened.
//!
//! Written from where the thing happens, in its own transaction: `inbox` (deliveries and
//! queue steps), `scheduled`, `events::append` (a run's and an approval's steps) and
//! `conversations::activity_finished` (tool calls). Writing is best effort: the log
//! never stops the work it describes.

use crate::{StorageError, now_ms};
use aurelius_domain::views::{ActivityLogEntry, ThreadPlace};
use aurelius_domain::{ConversationId, EmployeeId, RunId, ScheduledMessageId, TicketId, UserId};
use rusqlite::types::ToSql;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::collections::HashMap;

/// An item reached a mailbox, or could not.
pub const DELIVERY: &str = "delivery";
/// What happened to an item while it waited.
pub const QUEUE: &str = "queue";
/// What a run did.
pub const RUN: &str = "run";

/// How long a row stays.
pub const KEEP_MS: i64 = 90 * 24 * 60 * 60 * 1000;

/// The board member whose private chat `thread` is, as a column expression.
fn private_to(thread: &str) -> String {
    format!("(SELECT board_member_id FROM conversations WHERE id = substr({thread}, 6) AND {thread} LIKE 'chat:%')")
}

fn noted(step: &str, written: rusqlite::Result<usize>) {
    if let Err(err) = written {
        tracing::warn!(step, error = %err, "activity log: not written");
    }
}

/// What the item `item` went through. `run`: the run it is now part of, if any.
pub(crate) fn item_step(tx: &Transaction<'_>, item: &str, category: &str, step: &str, detail: Option<&str>, run: Option<RunId>) {
    let sql = format!(
        "INSERT INTO activity_log(at_ms, category, step, detail, employee_id, thread_key, item_id, run_id, hop, private_to)
         SELECT ?1, ?2, ?3, ?4, i.employee_id, i.thread_key, i.id, ?5, i.hop, {} FROM employee_inbox i WHERE i.id = ?6",
        private_to("i.thread_key")
    );
    noted(step, tx.execute(&sql, params![now_ms(), category, step, detail, run.map(|r| r.to_string()), item]));
}

/// A queue step of the item that waits to become `run`.
pub(crate) fn queued_step(tx: &Transaction<'_>, run: &str, step: &str, detail: Option<&str>) {
    let sql = format!(
        "INSERT INTO activity_log(at_ms, category, step, detail, employee_id, thread_key, item_id, hop, private_to)
         SELECT ?1, ?2, ?3, ?4, i.employee_id, i.thread_key, i.id, i.hop, {} FROM employee_inbox i WHERE i.run_id = ?5 AND i.state = 'queued'",
        private_to("i.thread_key")
    );
    noted(step, tx.execute(&sql, params![now_ms(), QUEUE, step, detail, run]));
}

/// A step of a thread as a whole (parked, open again).
pub(crate) fn thread_step(tx: &Transaction<'_>, step: &str, employee: &str, thread: &str, run: Option<RunId>) {
    let sql = format!(
        "INSERT INTO activity_log(at_ms, category, step, employee_id, thread_key, run_id, private_to) VALUES (?1, ?2, ?3, ?4, ?5, ?6, {})",
        private_to("?5")
    );
    noted(step, tx.execute(&sql, params![now_ms(), QUEUE, step, employee, thread, run.map(|r| r.to_string())]));
}

/// A step of `run`, when it is an employee's (not the project assistant's, for
/// instance). The run must exist: what still waits in a mailbox has its item's steps.
pub(crate) fn run_step(tx: &Transaction<'_>, run: &str, step: &str, detail: Option<&str>) {
    noted(
        step,
        tx.execute(
            "INSERT INTO activity_log(at_ms, category, step, detail, employee_id, thread_key, run_id, hop, private_to)
             SELECT ?1, ?2, ?3, ?4, COALESCE(r.employee_id, c.employee_id), json_extract(j.payload_json, '$.thread_key'), r.id, json_extract(j.payload_json, '$.hop'), c.board_member_id
             FROM runs r JOIN conversations c ON c.id = r.conversation_id LEFT JOIN jobs j ON j.id = r.id
             WHERE r.id = ?5 AND COALESCE(r.employee_id, c.employee_id) IS NOT NULL",
            params![now_ms(), RUN, step, detail, run],
        ),
    );
}

/// What a scheduled message went through.
pub(crate) fn scheduled_step(tx: &Transaction<'_>, id: ScheduledMessageId, step: &str, detail: Option<&str>) {
    let sql = format!(
        "INSERT INTO activity_log(at_ms, category, step, detail, employee_id, thread_key, run_id, hop, private_to)
         SELECT ?1, ?2, ?3, ?4, s.to_employee_id, s.thread_key, s.run_id, s.hop, {} FROM scheduled_messages s WHERE s.id = ?5",
        private_to("s.thread_key")
    );
    noted(step, tx.execute(&sql, params![now_ms(), DELIVERY, step, detail, id.to_string()]));
}

/// The steps of a run and of an approval, from their events.
pub(crate) fn from_event(tx: &Transaction<'_>, aggregate_type: &str, aggregate_id: &str, event_type: &str, correlation: &str, metadata: &serde_json::Value) {
    let reason = || metadata["reason"].as_str().or_else(|| metadata["error_code"].as_str());
    match (aggregate_type, event_type) {
        ("run", "run.started") => run_step(tx, aggregate_id, "started", None),
        ("run", "run.completed") => run_step(tx, aggregate_id, "completed", None),
        ("run", "run.cancelled") => run_step(tx, aggregate_id, "cancelled", None),
        ("run", "run.failed") => run_step(tx, aggregate_id, "failed", reason()),
        ("run", "run.waiting") => run_step(tx, aggregate_id, "waiting", reason()),
        ("approval", _) => {
            // `approval.requested` comes first; its correlation is the run.
            let run: Option<String> = tx.query_row("SELECT run_id FROM approvals WHERE id = ?1", [aggregate_id], |r| r.get(0)).optional().ok().flatten();
            let step = event_type.replace('.', "_");
            run_step(tx, run.as_deref().unwrap_or(correlation), &step, metadata["tool"].as_str());
        }
        _ => {}
    }
}

/// Remove what is older than `KEEP_MS`. Answers how many went. `seq` follows the clock,
/// so this reads only the rows it removes, plus one.
pub fn prune(tx: &Transaction<'_>, now: i64) -> Result<usize, StorageError> {
    Ok(tx.execute(
        "DELETE FROM activity_log WHERE seq < COALESCE((SELECT seq FROM activity_log WHERE at_ms >= ?1 ORDER BY seq LIMIT 1), 9223372036854775807)",
        [now - KEEP_MS],
    )?)
}

/// What to look for; what is `None` does not narrow it.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub employee: Option<EmployeeId>,
    pub thread: Option<String>,
    pub ticket_number: Option<u32>,
    pub run: Option<RunId>,
    pub category: Option<String>,
    pub before_seq: Option<u64>,
}

/// The newest rows that match, at most `limit`, and whether there are older ones.
/// Someone else's private chat does not exist for `reader`.
pub fn list(conn: &Connection, reader: Option<UserId>, filter: &Filter, limit: u32) -> Result<(Vec<ActivityLogEntry>, bool), StorageError> {
    // Only what is asked for goes in the query: `?1 IS NULL OR ...` would keep it from its index.
    let mut clauses = vec!["(l.private_to IS NULL OR l.private_to = ?1)".to_owned()];
    let mut values: Vec<Box<dyn ToSql>> = vec![Box::new(reader.map(|u| u.to_string()))];
    let mut add = |clause: &dyn Fn(usize) -> String, value: Box<dyn ToSql>| {
        values.push(value);
        clauses.push(clause(values.len()));
    };
    if let Some(employee) = filter.employee {
        add(&|n| format!("l.employee_id = ?{n}"), Box::new(employee.to_string()));
    }
    if let Some(thread) = &filter.thread {
        add(&|n| format!("l.thread_key = ?{n}"), Box::new(thread.clone()));
    }
    if let Some(number) = filter.ticket_number {
        add(&|n| format!("l.thread_key IN (SELECT 'ticket:' || id FROM tickets WHERE number = ?{n})"), Box::new(number));
    }
    if let Some(run) = filter.run {
        // Also what the items it took went through before it existed.
        add(&|n| format!("(l.run_id = ?{n} OR l.item_id IN (SELECT item_id FROM activity_log WHERE run_id = ?{n} AND item_id IS NOT NULL))"), Box::new(run.to_string()));
    }
    if let Some(category) = &filter.category {
        add(&|n| format!("l.category = ?{n}"), Box::new(category.clone()));
    }
    if let Some(before) = filter.before_seq {
        add(&|n| format!("l.seq < ?{n}"), Box::new(before as i64));
    }
    let sql = format!(
        "SELECT l.seq, l.at_ms, l.category, l.step, l.detail, l.employee_id, e.name, l.thread_key, l.run_id, l.hop
         FROM activity_log l LEFT JOIN employees e ON e.id = l.employee_id
         WHERE {} ORDER BY l.seq DESC LIMIT {}",
        clauses.join(" AND "),
        limit + 1
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values.iter().map(|v| v.as_ref())), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, Option<i64>>(9)?,
        ))
    })?;
    let mut places: HashMap<String, ThreadPlace> = HashMap::new();
    let mut out = Vec::new();
    for row in rows {
        let (seq, at_ms, category, step, detail, employee, employee_name, thread_key, run, hop) = row?;
        let place = match &thread_key {
            Some(key) => Some(match places.get(key) {
                Some(place) => place.clone(),
                None => {
                    let place = place_of(conn, key)?;
                    places.insert(key.clone(), place.clone());
                    place
                }
            }),
            None => None,
        };
        out.push(ActivityLogEntry {
            seq: seq as u64,
            at_ms,
            category,
            step,
            detail,
            employee_id: employee.and_then(|e| EmployeeId::parse(&e).ok()),
            employee_name,
            thread_key,
            place,
            run_id: run.and_then(|r| RunId::parse(&r).ok()),
            hop: hop.map(|h| h as u32),
        });
    }
    let more = out.len() > limit as usize;
    out.truncate(limit as usize);
    Ok((out, more))
}

/// Where a thread is, for reading and for a link. Its ticket or chat may be gone.
pub fn place_of(conn: &Connection, thread: &str) -> Result<ThreadPlace, StorageError> {
    let nowhere = |label: &str| ThreadPlace { label: label.to_owned(), ticket_id: None, channel_id: None, chat_id: None };
    if let Some(id) = thread.strip_prefix("ticket:") {
        let row: Option<(u32, String)> = conn.query_row("SELECT number, title FROM tickets WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        return Ok(match row {
            Some((number, title)) => ThreadPlace { label: format!("#{number} {title}"), ticket_id: TicketId::parse(id).ok(), channel_id: None, chat_id: None },
            None => nowhere("A removed ticket"),
        });
    }
    if let Some(id) = thread.strip_prefix("channel:") {
        let title: Option<String> = conn.query_row("SELECT title FROM conversations WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        return Ok(match title {
            Some(title) => ThreadPlace { label: title, ticket_id: None, channel_id: ConversationId::parse(id).ok(), chat_id: None },
            None => nowhere("A removed channel"),
        });
    }
    let Some(id) = thread.strip_prefix("chat:") else { return Ok(nowhere(thread)) };
    let row: Option<(Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT u.username, e.name, (SELECT t.title FROM tasks t WHERE t.conversation_id = c.id)
             FROM conversations c LEFT JOIN employees e ON e.id = c.employee_id LEFT JOIN users u ON u.id = c.board_member_id WHERE c.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    Ok(match row {
        Some((Some(user), _, _)) => ThreadPlace { label: format!("Chat with {user}"), ticket_id: None, channel_id: None, chat_id: ConversationId::parse(id).ok() },
        Some((None, _, Some(task))) => nowhere(&task),
        Some((None, Some(name), None)) => nowhere(&format!("{name}'s own chat")),
        _ => nowhere("A removed chat"),
    })
}
