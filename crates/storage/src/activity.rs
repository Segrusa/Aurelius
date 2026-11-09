//! `activity.overview` (ticket #50): who is busy, what waits and on what, and where
//! each ticket stands. Read-only and computed from the jobs, the runs and the
//! mailboxes, so it cannot disagree with what the dispatcher does.

use crate::inbox::{self, HELD};
use crate::{StorageError, activity_log, queries};
use aurelius_domain::views::{ACTIVITY_PAUSED, ACTIVITY_RUNNING, ACTIVITY_WAITING_OTHER, ACTIVITY_WAITING_SLOT, ActivityBusy, ActivityOverview, ActivityWaiting, ActivityWork, TicketActivity};
use aurelius_domain::{ConversationId, EmployeeId, TaskId, TicketId, UserId};
use rusqlite::{Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};

/// Who a run without an employee is: the project assistant.
const ASSISTANT: &str = "Assistant";

/// An employee as the overview names it.
struct Person {
    name: String,
    paused: bool,
}

/// What a row knows of where it belongs.
struct Where<'a> {
    thread: Option<&'a str>,
    conversation: Option<&'a str>,
    task: Option<&'a str>,
    board_member: Option<&'a str>,
}

/// The work of a row as `reader` may see it. Another board member's chat is
/// answered first and from nothing else: it carries no title and no place.
fn work_of(conn: &Connection, at: &Where<'_>, reader: Option<&str>) -> Result<ActivityWork, StorageError> {
    if at.board_member.is_some() && at.board_member != reader {
        return Ok(ActivityWork { private: true, ..ActivityWork::default() });
    }
    let task = match at.task {
        Some(task) => TaskId::parse(task).ok(),
        None => match at.conversation {
            Some(c) => conn.query_row("SELECT id FROM tasks WHERE conversation_id = ?1", [c], |r| r.get::<_, String>(0)).optional()?.and_then(|t| TaskId::parse(&t).ok()),
            None => None,
        },
    };
    if let Some(thread) = at.thread {
        let place = activity_log::place_of(conn, thread)?;
        return Ok(ActivityWork { label: place.label, private: false, ticket_id: place.ticket_id, task_id: task, channel_id: place.channel_id, chat_id: place.chat_id });
    }
    if let Some(task) = task {
        let row: Option<(String, Option<String>)> = conn.query_row("SELECT title, ticket_id FROM tasks WHERE id = ?1", [task.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        let (label, ticket) = row.unwrap_or_default();
        return Ok(ActivityWork { label, task_id: Some(task), ticket_id: ticket.and_then(|t| TicketId::parse(&t).ok()), ..ActivityWork::default() });
    }
    let title: Option<String> = match at.conversation {
        Some(c) => conn.query_row("SELECT title FROM conversations WHERE id = ?1", [c], |r| r.get(0)).optional()?,
        None => None,
    };
    Ok(ActivityWork { label: title.unwrap_or_else(|| "A chat".to_owned()), chat_id: at.conversation.and_then(|c| ConversationId::parse(c).ok()), ..ActivityWork::default() })
}

fn people(conn: &Connection) -> Result<HashMap<String, Person>, StorageError> {
    let mut stmt = conn.prepare("SELECT id, name, status = 'paused' FROM employees")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, Person { name: r.get(1)?, paused: r.get(2)? })))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn name_of(people: &HashMap<String, Person>, employee: Option<&str>) -> String {
    employee.and_then(|e| people.get(e)).map_or_else(|| ASSISTANT.to_owned(), |p| p.name.clone())
}

fn employee_id(employee: Option<&str>) -> Option<EmployeeId> {
    employee.and_then(|e| EmployeeId::parse(e).ok())
}

fn paused(people: &HashMap<String, Person>, employee: Option<&str>) -> bool {
    employee.and_then(|e| people.get(e)).is_some_and(|p| p.paused)
}

/// An approval is pending in this thread, so its run holds the thread but no slot.
fn approval_in_thread(conn: &Connection, employee: &str, thread: &str) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs o JOIN approvals a ON a.run_id = o.id WHERE o.kind = 'run.chat' AND o.state = 'leased' AND a.status = 'pending'
             AND json_extract(o.payload_json, '$.employee_id') = ?1 AND json_extract(o.payload_json, '$.thread_key') = ?2)",
        [employee, thread],
        |r| r.get(0),
    )?)
}

/// What the items of one employee in one thread wait for.
struct Line<'a> {
    employee: &'a str,
    thread: &'a str,
    state: &'a str,
    resumes: bool,
    not_before: i64,
    quota: bool,
}

fn line_waits_for(conn: &Connection, people: &HashMap<String, Person>, line: &Line<'_>, now: i64) -> Result<&'static str, StorageError> {
    Ok(match () {
        _ if paused(people, Some(line.employee)) => "paused",
        _ if line.state == HELD => "handoff",
        _ if !line.resumes && inbox::is_parked(conn, line.employee, line.thread)? => "question",
        _ if line.not_before > now && line.quota => "limit",
        _ if line.not_before > now => "later",
        _ if !inbox::slot_free(conn, line.employee)? => "slot",
        _ if approval_in_thread(conn, line.employee, line.thread)? => "approval",
        _ => "machine",
    })
}

/// What a chat run back in line waits for.
fn job_waits_for(conn: &Connection, people: &HashMap<String, Person>, employee: Option<&str>, error: Option<&str>, not_before: i64, now: i64) -> Result<&'static str, StorageError> {
    Ok(match (employee, error) {
        _ if paused(people, employee) => "paused",
        (_, Some("login")) => "login",
        (_, Some("quota")) if not_before > now => "limit",
        _ if not_before > now => "later",
        (Some(e), _) if !inbox::slot_free(conn, e)? => "slot",
        _ => "machine",
    })
}

/// The ticket state a row in line gives its ticket.
fn state_of(waits_for: &str) -> &'static str {
    match waits_for {
        "slot" | "machine" => ACTIVITY_WAITING_SLOT,
        "paused" => ACTIVITY_PAUSED,
        _ => ACTIVITY_WAITING_OTHER,
    }
}

/// What goes first when several things happen on one ticket: lower wins.
fn rank(state: &str) -> u8 {
    match state {
        ACTIVITY_RUNNING => 0,
        ACTIVITY_WAITING_SLOT => 1,
        ACTIVITY_PAUSED => 2,
        _ => 3,
    }
}

/// Keeps per ticket what says the most, and of equals the oldest.
#[derive(Default)]
struct Tickets(HashMap<TicketId, TicketActivity>);

impl Tickets {
    fn note(&mut self, next: TicketActivity) {
        match self.0.get(&next.ticket_id) {
            Some(known) if (rank(&known.state), known.since_ms) <= (rank(&next.state), next.since_ms) => {}
            _ => {
                self.0.insert(next.ticket_id, next);
            }
        }
    }
}

/// What a main task's own waiting reason means for its card.
fn root_waits(reason: Option<&str>) -> (&'static str, &'static str) {
    match reason {
        Some("paused") => (ACTIVITY_PAUSED, "paused"),
        Some("dependency") => (ACTIVITY_WAITING_OTHER, "colleague"),
        Some("quota") => (ACTIVITY_WAITING_OTHER, "limit"),
        Some(aurelius_domain::task::WAITING_LOGIN) => (ACTIVITY_WAITING_OTHER, "login"),
        _ => (ACTIVITY_WAITING_OTHER, "board_member"),
    }
}

/// The overview as `reader` may see it.
pub fn overview(conn: &Connection, reader: Option<UserId>, now: i64) -> Result<ActivityOverview, StorageError> {
    let reader = reader.map(|u| u.to_string());
    let reader = reader.as_deref();
    let people = people(conn)?;
    let mut busy = Vec::new();
    let mut waiting = Vec::new();
    let mut tickets = Tickets::default();
    // Per employee and thread what is already listed: a retry and new items become one line.
    let mut listed: HashSet<(String, String)> = HashSet::new();

    type RunRow = (Option<String>, Option<String>, Option<String>, Option<String>, i64, Option<String>, bool);
    let mut stmt = conn.prepare(
        "SELECT COALESCE(json_extract(j.payload_json, '$.employee_id'), c.employee_id), json_extract(j.payload_json, '$.thread_key'), r.conversation_id, r.task_id,
                COALESCE(r.started_at_ms, j.not_before_ms), c.board_member_id,
                EXISTS(SELECT 1 FROM approvals a WHERE a.run_id = j.id AND a.status = 'pending')
         FROM jobs j JOIN runs r ON r.id = j.id LEFT JOIN conversations c ON c.id = r.conversation_id
         WHERE j.kind = 'run.chat' AND j.state = 'leased' AND r.status IN ('created', 'starting', 'running', 'waiting', 'cancelling')
         ORDER BY 5, j.id",
    )?;
    let runs: Vec<RunRow> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?.collect::<rusqlite::Result<_>>()?;
    for (employee, thread, conversation, task, since_ms, board_member, approval) in runs {
        let employee = employee.as_deref();
        let at = Where { thread: thread.as_deref(), conversation: conversation.as_deref(), task: task.as_deref(), board_member: board_member.as_deref() };
        let work = work_of(conn, &at, reader)?;
        let name = name_of(&people, employee);
        if let Some(ticket_id) = work.ticket_id {
            let (state, waits_for) = if approval { (ACTIVITY_WAITING_OTHER, Some("approval".to_owned())) } else { (ACTIVITY_RUNNING, None) };
            tickets.note(TicketActivity { ticket_id, state: state.to_owned(), employee_id: employee_id(employee), employee_name: Some(name.clone()), waits_for, since_ms });
        }
        if approval {
            waiting.push(ActivityWaiting { employee_id: employee_id(employee), employee_name: name, work, waits_for: "approval".to_owned(), since_ms });
        } else {
            busy.push(ActivityBusy { employee_id: employee_id(employee), employee_name: name, employee_paused: paused(&people, employee), work, since_ms });
        }
    }

    type ItemRow = (String, String, String, Option<String>, Option<String>, Option<String>, i64, i64, bool, bool);
    let mut stmt = conn.prepare(
        "SELECT i.employee_id, i.thread_key, i.state, i.conversation_id, i.task_id, c.board_member_id,
                MIN(i.created_at_ms), MIN(i.not_before_ms), MAX(i.kind = 'resume'), COALESCE(MAX(i.last_error_code = 'quota'), 0)
         FROM employee_inbox i LEFT JOIN conversations c ON c.id = i.conversation_id
         WHERE i.state IN ('queued', 'held') GROUP BY i.employee_id, i.thread_key, i.state ORDER BY 7, i.rowid",
    )?;
    let items: Vec<ItemRow> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (employee, thread, state, conversation, task, board_member, since_ms, not_before, resumes, quota) in items {
        let waits_for = line_waits_for(conn, &people, &Line { employee: &employee, thread: &thread, state: &state, resumes, not_before, quota }, now)?;
        let at = Where { thread: Some(&thread), conversation: conversation.as_deref(), task: task.as_deref(), board_member: board_member.as_deref() };
        listed.insert((employee.clone(), thread.clone()));
        push_waiting(conn, &people, &mut waiting, &mut tickets, Some(&employee), &at, reader, waits_for, since_ms)?;
    }

    type JobRow = (Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, i64, Option<String>);
    let mut stmt = conn.prepare(
        "SELECT COALESCE(json_extract(j.payload_json, '$.employee_id'), c.employee_id), json_extract(j.payload_json, '$.thread_key'), r.conversation_id, r.task_id,
                j.last_error_code, j.not_before_ms, c.board_member_id
         FROM jobs j JOIN runs r ON r.id = j.id LEFT JOIN conversations c ON c.id = r.conversation_id
         WHERE j.kind = 'run.chat' AND j.state = 'ready' ORDER BY j.not_before_ms, j.id",
    )?;
    let jobs: Vec<JobRow> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?.collect::<rusqlite::Result<_>>()?;
    for (employee, thread, conversation, task, error, not_before, board_member) in jobs {
        if let (Some(e), Some(t)) = (&employee, &thread)
            && !listed.insert((e.clone(), t.clone()))
        {
            continue;
        }
        let waits_for = job_waits_for(conn, &people, employee.as_deref(), error.as_deref(), not_before, now)?;
        let at = Where { thread: thread.as_deref(), conversation: conversation.as_deref(), task: task.as_deref(), board_member: board_member.as_deref() };
        // A far-away `not_before_ms` is a parking date, not when it began to wait.
        push_waiting(conn, &people, &mut waiting, &mut tickets, employee.as_deref(), &at, reader, waits_for, not_before.min(now))?;
    }

    // Tasks whose job has not started a conversation yet.
    let mut stmt = conn.prepare(
        "SELECT t.id, t.employee_id, t.updated_at_ms, j.not_before_ms FROM jobs j JOIN tasks t ON t.id = j.id
         WHERE j.kind = 'task.run' AND j.state = 'ready' AND t.status IN ('queued', 'waiting')
           AND (t.status = 'queued' OR t.waiting_reason = 'paused') ORDER BY t.updated_at_ms, t.id",
    )?;
    let queued: Vec<(String, Option<String>, i64, i64)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
    for (task, employee, since_ms, not_before) in queued {
        let waits_for = match () {
            _ if paused(&people, employee.as_deref()) => "paused",
            _ if not_before > now => "later",
            _ => "machine",
        };
        let at = Where { thread: None, conversation: None, task: Some(&task), board_member: None };
        push_waiting(conn, &people, &mut waiting, &mut tickets, employee.as_deref(), &at, reader, waits_for, since_ms)?;
    }
    waiting.sort_by_key(|w| w.since_ms);

    // A main task that stands still with nothing in line for it.
    let mut stmt = conn.prepare(
        "SELECT k.id, r.waiting_reason, r.employee_id, r.updated_at_ms FROM tickets k JOIN tasks r ON r.id = k.root_task_id WHERE r.status = 'waiting'",
    )?;
    let roots: Vec<(String, Option<String>, Option<String>, i64)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
    for (ticket, reason, employee, since_ms) in roots {
        let Ok(ticket_id) = TicketId::parse(&ticket) else { continue };
        if tickets.0.contains_key(&ticket_id) {
            continue;
        }
        let (state, waits_for) = root_waits(reason.as_deref());
        tickets.note(TicketActivity {
            ticket_id,
            state: state.to_owned(),
            employee_id: employee_id(employee.as_deref()),
            employee_name: employee.as_deref().and_then(|e| people.get(e)).map(|p| p.name.clone()),
            waits_for: Some(waits_for.to_owned()),
            since_ms,
        });
    }
    let mut tickets: Vec<TicketActivity> = tickets.0.into_values().collect();
    tickets.sort_by_key(|t| (t.since_ms, t.ticket_id.to_string()));
    Ok(ActivityOverview { as_of_seq: queries::last_seq(conn)?, busy, waiting, tickets })
}

#[allow(clippy::too_many_arguments)]
fn push_waiting(
    conn: &Connection,
    people: &HashMap<String, Person>,
    waiting: &mut Vec<ActivityWaiting>,
    tickets: &mut Tickets,
    employee: Option<&str>,
    at: &Where<'_>,
    reader: Option<&str>,
    waits_for: &str,
    since_ms: i64,
) -> Result<(), StorageError> {
    let work = work_of(conn, at, reader)?;
    let name = name_of(people, employee);
    if let Some(ticket_id) = work.ticket_id {
        tickets.note(TicketActivity { ticket_id, state: state_of(waits_for).to_owned(), employee_id: employee_id(employee), employee_name: Some(name.clone()), waits_for: Some(waits_for.to_owned()), since_ms });
    }
    waiting.push(ActivityWaiting { employee_id: employee_id(employee), employee_name: name, work, waits_for: waits_for.to_owned(), since_ms });
    Ok(())
}
