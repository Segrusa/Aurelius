//! Timeline projection: human-readable cards derived
//! deterministically from event types. Routine runtime/tool noise is only
//! included on request ("Activity"). Keyset pagination on `seq`.

use crate::{StorageError, now_ms};
use aurelius_domain::{ConversationId, EventId, MessageId, ProjectId, TaskId, TicketId, UserId};
use aurelius_domain::views::{NotificationView, TimelineCard};
use rusqlite::{Connection, OptionalExtension, params};

const MEANINGFUL: &[&str] = &[
    "project.created",
    "project.updated",
    "project.paused",
    "project.archived",
    "employee.created",
    "employee.config_changed",
    "employee.paused",
    "employee.resumed",
    "employee.archived",
    "employee.proposed",
    "employee.proposal_decided",
    "employee.instructions_proposed",
    "guideline.proposed",
    "settings.follow_org_chart",
    "company.converted",
    "task.created",
    "task.succeeded",
    "task.failed",
    "task.cancelled",
    "task.waiting",
    "task.assigned",
    "task.handed_off",
    "task.delegated",
    "task.mode_chosen",
    "memory.remembered",
    "idea.added",
    "idea.started",
    "idea.ticket_created",
    "approval.requested",
    "approval.approved",
    "approval.answered",
    "approval.rejected",
    "approval.expired",
    "approval.invalidated",
    "attachment.added",
];

fn is_meaningful(event_type: &str) -> bool {
    MEANINGFUL.contains(&event_type) || event_type == "user.disabled" || event_type == "server.renamed"
}

/// Account writes recorded as `host.write` that belong in the timeline (MU3-04 AC4).
const ACCOUNT_WRITES: &[&str] = &["session.revoke", "user.set_password"];

fn meaningful(row: &Row) -> bool {
    is_meaningful(&row.event_type)
        || (row.event_type == "host.write" && row.metadata["method"].as_str().is_some_and(|m| ACCOUNT_WRITES.contains(&m)))
        || (row.event_type == "ticket.updated" && row.metadata["files_added"].is_array())
        || (row.event_type == "ticket.updated" && changes_responsible(&row.metadata))
}

/// A `ticket.updated` about who is responsible (`ticket.set_responsible`).
fn changes_responsible(metadata: &serde_json::Value) -> bool {
    metadata["changed"].as_array().is_some_and(|changed| changed.iter().any(|c| c == "responsible"))
}

/// "added 1 file" / "added 3 files".
fn added_files(count: usize) -> String {
    if count == 1 { "added 1 file".into() } else { format!("added {count} files") }
}

struct Row {
    seq: i64,
    event_type: String,
    aggregate_type: String,
    aggregate_id: String,
    correlation: String,
    actor_kind: String,
    metadata: serde_json::Value,
    at: i64,
}

fn lookup(conn: &Connection, sql: &str, id: &str) -> Option<String> {
    conn.query_row(sql, [id], |r| r.get::<_, String>(0)).ok()
}

/// A board member's name by the id in event metadata ("A board member" when gone).
fn board_member(conn: &Connection, id: &serde_json::Value) -> String {
    id.as_str().and_then(|id| lookup(conn, "SELECT username FROM users WHERE id = ?1", id)).unwrap_or_else(|| "A board member".into())
}

fn card(conn: &Connection, row: &Row) -> TimelineCard {
    let m = &row.metadata;
    // A task's own events may carry the creating command as correlation.
    let chain = if row.aggregate_type == "task" { row.aggregate_id.clone() } else { row.correlation.clone() };
    let task_title = || lookup(conn, "SELECT title FROM tasks WHERE id = ?1", &chain).unwrap_or_default();
    let task_employee = || lookup(conn, "SELECT m.name FROM tasks t JOIN employees m ON m.id = t.employee_id WHERE t.id = ?1", &chain).unwrap_or_else(|| "Employee".into());
    let s = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
    let (kind, label, title, detail, actor): (&str, &str, String, String, String) = match row.event_type.as_str() {
        "project.created" => ("start", "PROJECT", "Project created".into(), String::new(), "You".into()),
        "project.updated" => ("minor", "PROJECT", "Project updated".into(), String::new(), "You".into()),
        "project.paused" => ("decision", "DECISION", "Project paused".into(), String::new(), "You".into()),
        "project.archived" => ("decision", "DECISION", "Project archived".into(), String::new(), "You".into()),
        "employee.created" => {
            let name = lookup(conn, "SELECT name FROM employees WHERE id = ?1", &row.aggregate_id).unwrap_or_default();
            ("start", "EMPLOYEE", format!("{name} added"), lookup(conn, "SELECT role FROM employees WHERE id = ?1", &row.aggregate_id).unwrap_or_default(), "You".into())
        }
        "employee.config_changed" => ("minor", "EMPLOYEE", "Employee configuration changed".into(), "Applies to new runs".into(), "You".into()),
        "employee.paused" => ("decision", "DECISION", "Employee paused".into(), String::new(), "You".into()),
        "employee.resumed" => ("decision", "DECISION", "Employee resumed".into(), String::new(), "You".into()),
        "employee.archived" => ("decision", "DECISION", "Employee archived".into(), String::new(), "You".into()),
        "settings.follow_org_chart" => (
            "decision",
            "DECISION",
            if m["follow"].as_bool() == Some(true) { "The company follows its org chart".into() } else { "The company no longer follows its org chart".into() },
            String::new(),
            "You".into(),
        ),
        "company.converted" => ("start", "COMPANY", "Your projects' managers became the company's employees".into(), s(&m["subject"]), "Aurelius".into()),
        "task.created" if m["sub_task"].as_bool() == Some(true) => {
            let by = lookup(conn, "SELECT m.name FROM tasks p JOIN employees m ON m.id = p.employee_id WHERE p.id = ?1", &s(&m["parent_task_id"])).unwrap_or_else(|| "Employee".into());
            ("start", "DELEGATED", format!("{by} → {}", s(&m["title"])), format!("for {}", task_employee()), by)
        }
        "task.created" => ("start", "WORK STARTED", format!("You → {}", s(&m["title"])), String::new(), "You".into()),
        "task.assigned" => ("decision", "ASSIGNED", format!("You gave {} to {}", task_title(), if m["subject"].is_string() { s(&m["subject"]) } else { "nobody (draft)".into() }), String::new(), "You".into()),
        "task.handed_off" => (
            "decision",
            "HANDED OVER",
            format!("{} handed {} over to {}", s(&m["from_name"]), task_title(), s(&m["to_name"])),
            s(&m["reason"]),
            s(&m["from_name"]),
        ),
        "task.delegated" => ("minor", "DELEGATED", format!("{} delegated {} to {}", task_employee(), s(&m["title"]), s(&m["subject"])), task_title(), task_employee()),
        "task.mode_chosen" => ("minor", "MODE", format!("{} chose {}", s(&m["subject"]), if s(&m["mode"]) == "auto" { "Auto" } else { "Plan" }), s(&m["reason"]), s(&m["subject"])),
        "idea.added" => {
            let who = lookup(conn, "SELECT m.name FROM ideas i JOIN employees m ON m.id = i.employee_id WHERE i.id = ?1", &row.aggregate_id).unwrap_or_else(|| "You".into());
            ("minor", "IDEA", format!("{who} noted an idea: {}", s(&m["title"])), String::new(), who)
        }
        "idea.started" => ("start", "IDEA → TASK", format!("You started the idea “{}”", s(&m["title"])), String::new(), "You".into()),
        "idea.ticket_created" => ("start", "IDEA → TICKET", format!("You made a ticket of the idea “{}”", s(&m["title"])), String::new(), "You".into()),
        "memory.remembered" => {
            let who = lookup(conn, "SELECT name FROM employees WHERE id = ?1", &s(&m["employee_id"])).unwrap_or_else(|| "You".into());
            ("minor", "MEMORY", format!("{who} noted a {}", s(&m["kind"]).replace('_', " ")), String::new(), who)
        }
        "task.succeeded" => ("outcome", "COMPLETED", task_title(), format!("{} · checked against the criteria", task_employee()), task_employee()),
        "task.failed" => ("outcome", "FAILED", task_title(), if m["escalated_to"].is_string() { format!("Escalated to {}", s(&m["escalated_to"])) } else { "Task failed".into() }, task_employee()),
        "task.cancelled" => ("decision", "DECISION", format!("{} cancelled", task_title()), String::new(), "You".into()),
        "task.waiting" => ("decision", "WAITING", format!("{} is waiting", task_title()), match s(&m["reason"]).as_str() {
            "escalated" | "input" => "Your decision is needed".into(),
            "escalated_to_boss" => format!("Escalated to {}", s(&m["boss"])),
            "dependency" => "Waiting for other tasks".into(),
            other => other.to_string(),
        }, task_employee()),
        "approval.requested" => ("decision", "APPROVAL", format!("Approval requested for {}", s(&m["tool"])), String::new(), "Aurelius".into()),
        "approval.approved" => ("decision", "DECISION", "You approved an action".into(), String::new(), "You".into()),
        "approval.rejected" => ("decision", "DECISION", "You rejected an action".into(), String::new(), "You".into()),
        "approval.answered" => ("decision", "DECISION", "You answered a question".into(), String::new(), "You".into()),
        "approval.expired" => ("minor", "APPROVAL", "Approval expired".into(), "Expiry never counts as permission".into(), "Aurelius".into()),
        "approval.invalidated" => ("minor", "APPROVAL", "Approval invalidated".into(), "The action changed".into(), "Aurelius".into()),
        "employee.proposed" => {
            let by = lookup(conn, "SELECT name FROM employees WHERE id = ?1", &s(&m["proposed_by"])).unwrap_or_else(|| "An employee".into());
            ("decision", "PROPOSAL", format!("{by} proposes a colleague: {}", s(&m["subject"])), s(&m["reason"]), by)
        }
        "employee.instructions_proposed" => {
            let by = lookup(conn, "SELECT name FROM employees WHERE id = ?1", &s(&m["proposed_by"])).unwrap_or_else(|| "An employee".into());
            ("decision", "PROPOSAL", format!("{by} proposes new instructions for {}", s(&m["subject"])), s(&m["reason"]), by)
        }
        // B5 (SEC0 E15): the title and who proposed it, never the text or the reason.
        "guideline.proposed" => {
            let by = lookup(conn, "SELECT name FROM employees WHERE id = ?1", &s(&m["by_employee"])).unwrap_or_else(|| "An employee".into());
            ("decision", "PROPOSAL", format!("A guideline is proposed: {}", s(&m["subject"])), String::new(), by)
        }
        "employee.proposal_decided" if m["kind"].as_str() == Some("instructions") => (
            "decision",
            "DECISION",
            format!("You {} new instructions for {}", if m["approved"].as_bool() == Some(true) { "approved" } else { "turned down" }, s(&m["subject"])),
            s(&m["note"]),
            "You".into(),
        ),
        "employee.proposal_decided" => ("decision", "DECISION", format!("You {} the colleague {}", if m["approved"].as_bool() == Some(true) { "approved" } else { "turned down" }, s(&m["subject"])), s(&m["note"]), "You".into()),
        // Ticket #5: the clients word the line from `detail` ("passed"/"failed").
        "ticket.tested" => {
            let passed = m["passed"].as_bool() == Some(true);
            let tester = lookup(conn, "SELECT m.name FROM tasks t JOIN employees m ON m.id = t.employee_id WHERE t.id = ?1", &s(&m["task_id"])).unwrap_or_else(|| "Tester".into());
            let (title, detail) = if passed { ("Tests passed", "passed") } else { ("Tests failed", "failed") };
            ("outcome", "TESTING", title.into(), detail.into(), tester)
        }
        // Files added to a ticket (feature `ticket_add_files`): their names, never where they are kept.
        "ticket.updated" if m["files_added"].is_array() => {
            let names: Vec<String> = m["files_added"].as_array().into_iter().flatten().map(s).collect();
            let by = board_member(conn, &m["by"]["user_id"]);
            let number = lookup(conn, "SELECT CAST(number AS TEXT) FROM tickets WHERE id = ?1", &row.aggregate_id).map(|n| format!(" to #{n}")).unwrap_or_default();
            ("minor", "ATTACHMENT", format!("{by} {}{number}", added_files(names.len())), names.join(", "), by)
        }
        // US-A1: who (the time is the card's). The clients look for "responsible" in `detail`.
        "ticket.updated" if changes_responsible(m) => {
            let by = board_member(conn, &m["by"]["user_id"]);
            let number = lookup(conn, "SELECT CAST(number AS TEXT) FROM tickets WHERE id = ?1", &row.aggregate_id).map(|n| format!(" for #{n}")).unwrap_or_default();
            let before = (!m["before"].is_null()).then(|| board_member(conn, &m["before"]));
            let (title, detail) = match (m["responsible_user_id"].is_null(), before) {
                (true, Some(before)) => (format!("{by} removed {before} as responsible{number}"), "Nobody is responsible now".to_owned()),
                (true, None) => (format!("{by} changed who is responsible{number}"), "Nobody is responsible now".to_owned()),
                (false, Some(before)) => (format!("{by} made {} responsible{number}", board_member(conn, &m["responsible_user_id"])), format!("{before} was responsible before")),
                (false, None) => (format!("{by} made {} responsible{number}", board_member(conn, &m["responsible_user_id"])), "Nobody was responsible before".to_owned()),
            };
            ("decision", "RESPONSIBLE", title, detail, by)
        }
        "attachment.added" => ("minor", "ATTACHMENT", format!("Attachment {}", s(&m["name"])), String::new(), "You".into()),
        // Accounts: always with the name of the board member who did it.
        "host.write" if m["method"].as_str() == Some("session.revoke") => {
            let by = board_member(conn, &m["by"]);
            let whose = if m["by"] == m["subject_user_id"] { "their own".to_string() } else { format!("{}'s", s(&m["subject"])) };
            ("decision", "SESSION", format!("{by} signed out {whose} session on {}", s(&m["device_name"])), String::new(), by)
        }
        "host.write" if m["method"].as_str() == Some("user.set_password") => {
            let by = board_member(conn, &m["by"]);
            let title = if m["temporary"].as_bool() == Some(true) { format!("{by} set a temporary password for {}", s(&m["subject"])) } else { format!("{by} changed their password") };
            ("decision", "ACCOUNT", title, "Other sessions were signed out".into(), by)
        }
        "user.disabled" => {
            let by = board_member(conn, &m["by"]);
            ("decision", "ACCOUNT", format!("{by} disabled {}", s(&m["subject"])), "Their sessions were signed out".into(), by)
        }
        // SN-01 AC7: who, the old and the new name (the time is the card's).
        "server.renamed" => {
            let by = board_member(conn, &m["by"]);
            ("decision", "SERVER", format!("{by} renamed the server to “{}”", s(&m["after"])), format!("It was “{}”", s(&m["before"])), by)
        }
        other => ("activity", "ACTIVITY", other.to_string(), row.aggregate_type.clone(), row.actor_kind.clone()),
    };
    TimelineCard {
        seq: row.seq as u64,
        at_ms: row.at,
        kind: kind.into(),
        label: label.into(),
        title,
        detail,
        actor,
        correlation_id: chain.clone(),
        task_id: TaskId::parse(&chain).ok().filter(|_| lookup(conn, "SELECT id FROM tasks WHERE id = ?1", &chain).is_some()),
    }
}

/// Cards for a project (or everything), newest first, before `before_seq`.
pub fn cards(
    conn: &Connection,
    project: Option<ProjectId>,
    correlation: Option<&str>,
    before_seq: Option<u64>,
    limit: u32,
    activity: bool,
) -> Result<(Vec<TimelineCard>, Option<u64>), StorageError> {
    let project_text = project.map(|p| p.to_string());
    let mut stmt = conn.prepare(
        "SELECT e.seq, e.type, e.aggregate_type, e.aggregate_id, e.correlation_id, e.actor_kind, e.metadata_json, e.occurred_at_ms
         FROM events e
         WHERE e.seq < ?1
           AND (?3 IS NULL OR e.correlation_id = ?3 OR (e.aggregate_type IN ('task','ticket') AND e.aggregate_id = ?3))
           AND (?2 IS NULL
                OR e.project_id = ?2
                OR (e.aggregate_type = 'project' AND e.aggregate_id = ?2)
                OR e.correlation_id IN (SELECT id FROM tasks WHERE project_id = ?2)
                OR (e.aggregate_type = 'task' AND e.aggregate_id IN (SELECT id FROM tasks WHERE project_id = ?2))
                OR (e.aggregate_type IN ('employee') AND e.aggregate_id IN (SELECT employee_id FROM tasks WHERE project_id = ?2))
                OR (e.aggregate_type = 'idea' AND e.aggregate_id IN (SELECT id FROM ideas WHERE project_id = ?2))
                OR e.correlation_id IN (SELECT r.id FROM runs r JOIN conversations c ON c.id = r.conversation_id WHERE c.project_id = ?2))
         ORDER BY e.seq DESC LIMIT ?4",
    )?;
    let fetch = (limit as i64) * 4 + 20;
    let rows = stmt.query_map(
        params![before_seq.map_or(i64::MAX, |s| s as i64), project_text, correlation, fetch],
        |r| {
            let metadata: String = r.get(6)?;
            Ok(Row {
                seq: r.get(0)?,
                event_type: r.get(1)?,
                aggregate_type: r.get(2)?,
                aggregate_id: r.get(3)?,
                correlation: r.get(4)?,
                actor_kind: r.get(5)?,
                metadata: serde_json::from_str(&metadata).unwrap_or_default(),
                at: r.get(7)?,
            })
        },
    )?;
    let mut out = Vec::new();
    let mut last_seq = None;
    let mut exhausted = true;
    for row in rows {
        let row = row?;
        last_seq = Some(row.seq as u64);
        if !activity && !meaningful(&row) {
            continue;
        }
        out.push(card(conn, &row));
        if out.len() >= limit as usize {
            exhausted = false;
            break;
        }
    }
    let next = if exhausted && out.len() < limit as usize { None } else { out.last().map(|c| c.seq).or(last_seq) };
    Ok((out, next))
}

// ── Notifications (Inbox) ───────────────────────────────────────────────

/// Title of the notification for a plan that waits for a decision (push shows Approve / View).
pub const PLAN_TITLE: &str = "Plan to review";
/// `NotificationView::reason`: this board member was made responsible.
pub const REASON_RESPONSIBLE: &str = "responsible";
/// `NotificationView::reason`: a message or a ticket's description names them.
pub const REASON_MENTION: &str = "mention";
/// `NotificationView::reason`: the ticket waits in Plan review.
pub const REASON_PLAN_REVIEW: &str = "plan_review";
/// `NotificationView::reason`: the ticket waits in Review.
pub const REASON_REVIEW: &str = "review";
/// `NotificationView::reason`: the ticket was sent back.
pub const REASON_SENT_BACK: &str = "sent_back";
/// `NotificationView::reason`: the main task of the ticket failed.
pub const REASON_FAILED: &str = "failed";
/// `NotificationView::reason`: an answer waits for this board member's Claude login.
pub const REASON_LOGIN: &str = "login";
/// The sentence of a row about a main task that failed.
const FAILED_TITLE: &str = "The main task failed";
/// How a row names who did it when that person or employee is gone.
const ACTOR_UNKNOWN: &str = "Someone";
/// How long a notification is kept (A0); its event stays. A request that still
/// waits for someone is kept until it is handled.
pub const NOTIFICATION_KEEP_MS: i64 = 90 * 24 * 60 * 60 * 1000;

/// Events that deserve a notification.
pub fn notification_for(event_type: &str, metadata: &serde_json::Value) -> Option<(&'static str, &'static str)> {
    // A board member's own Claude login is needed: for that board member only (`for_user`).
    if metadata["needs"].as_str() == Some("login") && metadata["for_user"].is_string() {
        return Some(match metadata["login"].as_str() {
            Some("expired") => ("action", "Reconnect your Claude login"),
            _ => ("action", "Connect your Claude login"),
        });
    }
    match event_type {
        "approval.requested" if metadata["tool"].as_str() == Some("claude.AskUserQuestion") => Some(("action", "Claude asks you something")),
        "approval.requested" if metadata["tool"].as_str() == Some("claude.ExitPlanMode") => Some(("action", PLAN_TITLE)),
        "approval.requested" => Some(("action", "Approval needed")),
        // A sub-task's outcome goes to the employee who delegated it, not to the user.
        "task.succeeded" | "task.failed" if metadata["sub_task"].as_bool() == Some(true) => None,
        "task.succeeded" => Some(("info", "Result ready")),
        // A failure the boss took over (org chart on) is not the user's to handle.
        "task.failed" if metadata["escalated_to"].is_string() => None,
        "task.failed" => Some(("warning", "Task failed")),
        "task.waiting" if metadata["reason"].as_str() == Some("escalated") => Some(("action", "Your decision needed")),
        // A colleague an employee proposed always waits for the user.
        "employee.proposed" => Some(("action", "A colleague is proposed")),
        // So do new instructions an employee proposes (Harness² B).
        "employee.instructions_proposed" => Some(("action", "New instructions are proposed")),
        // And so does a guideline (B5): only a board member decides; the subject is its title and proposer.
        "guideline.proposed" => Some(("action", "A guideline is proposed")),
        "connection.changed" if matches!(metadata["status"].as_str(), Some("needs_auth" | "needs_login")) => Some(("warning", "Connection needs sign-in")),
        _ => None,
    }
}

/// "Connect/Reconnect your Claude login" (`n`) is resolved once that board
/// member linked their login after it was made (QA2 m2).
macro_rules! login_linked_since {
    () => {
        "EXISTS (SELECT 1 FROM claude_logins l WHERE l.user_id = n.user_id AND l.status = 'linked' AND l.linked_at_ms >= n.created_at_ms)"
    };
}

/// Whether `events::append` has to call `notify` for this event. An event of a
/// message is not one of them: its row points at the message, which is written
/// after the event, so the discussion calls `notify_message` itself.
pub fn notifies(event_type: &str, metadata: &serde_json::Value) -> bool {
    notification_for(event_type, metadata).is_some()
        || matches!(event_type, "ticket.plan_review" | "ticket.review" | "ticket.sent_back")
        || (event_type == "ticket.created" && metadata["mentioned_users"].is_array())
        || (event_type == "ticket.updated" && (metadata["mentioned_users"].is_array() || changed_responsible(metadata)))
}

fn changed_responsible(metadata: &serde_json::Value) -> bool {
    metadata["changed"].as_array().is_some_and(|c| c.iter().any(|f| f.as_str() == Some("responsible")))
}

/// Create what an event deserves. Called from `events::append`, so it happens
/// exactly once per event (never on replay): a row per board member for what
/// happens to a ticket (§4 of the phase A freeze), or one for everyone.
pub fn notify(tx: &rusqlite::Transaction<'_>, seq: u64) -> Result<bool, StorageError> {
    let Some(event) = event_at(tx, seq)? else { return Ok(false) };
    if event.metadata["needs"].as_str() == Some("login") && event.metadata["for_user"].is_string() {
        return notify_login(tx, seq, &event);
    }
    match event.event_type.as_str() {
        "ticket.plan_review" => return notify_ticket(tx, seq, &event, REASON_PLAN_REVIEW),
        "ticket.review" => return notify_ticket(tx, seq, &event, REASON_REVIEW),
        "ticket.sent_back" => return notify_ticket(tx, seq, &event, REASON_SENT_BACK),
        "ticket.updated" if changed_responsible(&event.metadata) => return notify_responsible(tx, seq, &event),
        "ticket.created" | "ticket.updated" => return notify_mentions(tx, seq, &event, None),
        // The main task of a ticket says this on the ticket (B2): "Result ready" is
        // a run's update there, and a failure goes to who follows it. Not a sub-task.
        "task.succeeded" | "task.failed" if event.metadata["sub_task"].as_bool() != Some(true) => {
            if let Some(ticket) = ticket_of_main_task(tx, &event.aggregate_id)? {
                return if event.event_type == "task.failed" { notify_failed(tx, seq, &event, ticket) } else { Ok(false) };
            }
        }
        _ => {}
    }
    let Some((severity, _)) = notification_for(&event.event_type, &event.metadata) else { return Ok(false) };
    let target = serde_json::json!({ "type": event.event_type, "aggregate_type": event.aggregate_type, "aggregate_id": event.aggregate_id, "seq": seq });
    let inserted = tx.execute(
        "INSERT INTO notifications(id, event_id, dedupe_key, severity, target_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(dedupe_key) DO NOTHING",
        params![EventId::new().to_string(), event.id, format!("event:{}", event.id), severity, target.to_string(), now_ms()],
    )?;
    Ok(inserted == 1)
}

/// `notify` for the event of a message in a discussion, once `message` is written:
/// who it names gets a row (`metadata.mentioned_users`, also outside a ticket).
pub fn notify_message(tx: &rusqlite::Transaction<'_>, seq: u64, message: MessageId) -> Result<bool, StorageError> {
    let Some(event) = event_at(tx, seq)? else { return Ok(false) };
    notify_mentions(tx, seq, &event, Some(message))
}

/// What `notify` needs of an event.
struct EventRow {
    id: String,
    event_type: String,
    aggregate_type: String,
    aggregate_id: String,
    metadata: serde_json::Value,
}

fn event_at(conn: &Connection, seq: u64) -> Result<Option<EventRow>, StorageError> {
    Ok(conn
        .query_row("SELECT id, type, aggregate_type, aggregate_id, metadata_json FROM events WHERE seq = ?1", [seq as i64], |r| {
            Ok(EventRow { id: r.get(0)?, event_type: r.get(1)?, aggregate_type: r.get(2)?, aggregate_id: r.get(3)?, metadata: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default() })
        })
        .optional()?)
}

/// The ticket whose current main task `task` is.
fn ticket_of_main_task(conn: &Connection, task: &str) -> Result<Option<TicketId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT id FROM tickets WHERE root_task_id = ?1", [task], |r| r.get(0)).optional()?;
    Ok(id.and_then(|t| TicketId::parse(&t).ok()))
}

/// The board member who did what an event says; none for an employee or Aurelius.
fn actor_user(metadata: &serde_json::Value) -> Option<&str> {
    match metadata["by"]["kind"].as_str() {
        Some("board_member") => metadata["by"]["user_id"].as_str(),
        Some(_) => None,
        None => metadata["user_id"].as_str(),
    }
}

/// How a row names who did it: a board member's username or an employee's name.
fn actor_name(conn: &Connection, metadata: &serde_json::Value) -> Result<String, StorageError> {
    let found = if let Some(user) = actor_user(metadata) {
        conn.query_row("SELECT username FROM users WHERE id = ?1", [user], |r| r.get(0)).optional()?
    } else if let Some(employee) = metadata["by"]["employee_id"].as_str() {
        conn.query_row("SELECT name FROM employees WHERE id = ?1", [employee], |r| r.get(0)).optional()?
    } else {
        metadata["from"].as_str().map(str::to_owned)
    };
    Ok(found.unwrap_or_else(|| ACTOR_UNKNOWN.to_owned()))
}

/// Whether a ticket may still get a row, on the state after the change that
/// caused it: never on Done or archived, and a reason only while it is true.
/// The name of its column when it may; none when not, or when it does not exist.
fn ticket_gate(conn: &Connection, ticket: TicketId, reason: &str) -> Result<Option<String>, StorageError> {
    let row: Option<(String, String, Option<i64>, Option<String>)> = conn
        .query_row(
            "SELECT c.kind, c.name, k.archived_at_ms, t.status FROM tickets k JOIN board_columns c ON c.id = k.column_id
             LEFT JOIN tasks t ON t.id = k.root_task_id WHERE k.id = ?1",
            [ticket.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((kind, name, archived, root)) = row else { return Ok(None) };
    let open = kind != "done" && archived.is_none();
    let still = match reason {
        REASON_PLAN_REVIEW => kind == "plan_review",
        REASON_REVIEW => kind == "review",
        REASON_FAILED => root.as_deref() == Some("failed"),
        _ => true,
    };
    Ok((open && still).then_some(name))
}

/// Who hears about a ticket: its active followers and active Responsible, without
/// who did it.
fn ticket_audience(conn: &Connection, ticket: TicketId, actor: Option<&str>) -> Result<Vec<UserId>, StorageError> {
    let mut users = crate::followers::active_followers(conn, ticket)?;
    if let Some(responsible) = crate::tickets::responsible(conn, ticket)?.filter(|r| !r.disabled)
        && !users.contains(&responsible.user_id)
    {
        users.push(responsible.user_id);
    }
    users.retain(|u| actor != Some(u.to_string().as_str()));
    Ok(users)
}

fn deliver_all(tx: &rusqlite::Transaction<'_>, seq: u64, event: &EventRow, users: &[UserId], head: &Head<'_>) -> Result<bool, StorageError> {
    let mut any = false;
    for user in users {
        any |= deliver(tx, seq, event, *user, head)?;
    }
    Ok(any)
}

/// `ticket.plan_review`, `ticket.review`, `ticket.sent_back`: for who follows it.
fn notify_ticket(tx: &rusqlite::Transaction<'_>, seq: u64, event: &EventRow, reason: &'static str) -> Result<bool, StorageError> {
    let Ok(ticket) = TicketId::parse(&event.aggregate_id) else { return Ok(false) };
    let Some(column) = ticket_gate(tx, ticket, reason)? else { return Ok(false) };
    let title = match reason {
        REASON_SENT_BACK => format!("{} sent it back to {column}", actor_name(tx, &event.metadata)?),
        _ => review_title(reason, false).to_owned(),
    };
    let users = ticket_audience(tx, ticket, actor_user(&event.metadata))?;
    deliver_all(tx, seq, event, &users, &Head { reason, severity: "info", title: &title, ticket: Some(ticket), conversation: None, message: None })
}

/// The main task of a ticket failed: for who follows the ticket.
fn notify_failed(tx: &rusqlite::Transaction<'_>, seq: u64, event: &EventRow, ticket: TicketId) -> Result<bool, StorageError> {
    if ticket_gate(tx, ticket, REASON_FAILED)?.is_none() {
        return Ok(false);
    }
    let users = ticket_audience(tx, ticket, actor_user(&event.metadata))?;
    deliver_all(tx, seq, event, &users, &Head { reason: REASON_FAILED, severity: "warning", title: FAILED_TITLE, ticket: Some(ticket), conversation: None, message: None })
}

/// `ticket.updated` with a new Responsible: for that board member, unless they did it.
fn notify_responsible(tx: &rusqlite::Transaction<'_>, seq: u64, event: &EventRow) -> Result<bool, StorageError> {
    let Ok(ticket) = TicketId::parse(&event.aggregate_id) else { return Ok(false) };
    let Some(user) = event.metadata["responsible_user_id"].as_str().and_then(|u| UserId::parse(u).ok()) else { return Ok(false) };
    if ticket_gate(tx, ticket, REASON_RESPONSIBLE)?.is_none() || actor_user(&event.metadata) == Some(user.to_string().as_str()) || !active_member(tx, user)? {
        return Ok(false);
    }
    let title = format!("{} made you responsible", actor_name(tx, &event.metadata)?);
    deliver(tx, seq, event, user, &Head { reason: REASON_RESPONSIBLE, severity: "info", title: &title, ticket: Some(ticket), conversation: None, message: None })
}

/// Who `metadata.mentioned_users` names, in a ticket's discussion or description
/// or in a channel. Never the text: the event only carries who is named.
fn notify_mentions(tx: &rusqlite::Transaction<'_>, seq: u64, event: &EventRow, message: Option<MessageId>) -> Result<bool, StorageError> {
    let Some(named) = event.metadata["mentioned_users"].as_array() else { return Ok(false) };
    let (ticket, conversation) = match event.aggregate_type.as_str() {
        "ticket" => (TicketId::parse(&event.aggregate_id).ok(), None),
        _ => {
            let conversation = ConversationId::parse(&event.aggregate_id).ok();
            (event.metadata["ticket_id"].as_str().and_then(|t| TicketId::parse(t).ok()), conversation)
        }
    };
    if let Some(ticket) = ticket
        && ticket_gate(tx, ticket, REASON_MENTION)?.is_none()
    {
        return Ok(false);
    }
    let title = format!("{} mentioned you", actor_name(tx, &event.metadata)?);
    let head = Head { reason: REASON_MENTION, severity: "info", title: &title, ticket, conversation, message };
    let actor = actor_user(&event.metadata);
    let mut any = false;
    for user in named.iter().filter_map(|u| u.as_str()).filter(|u| actor != Some(*u)).filter_map(|u| UserId::parse(u).ok()) {
        if active_member(tx, user)? {
            any |= deliver(tx, seq, event, user, &head)?;
        }
    }
    Ok(any)
}

/// A board member's Claude login is needed: only for them, one unread at a time.
fn notify_login(tx: &rusqlite::Transaction<'_>, seq: u64, event: &EventRow) -> Result<bool, StorageError> {
    let Some(user) = event.metadata["for_user"].as_str().and_then(|u| UserId::parse(u).ok()) else { return Ok(false) };
    if !active_member(tx, user)? {
        return Ok(false);
    }
    let unread: bool = tx.query_row(
        concat!(
            "SELECT EXISTS(SELECT 1 FROM notifications n JOIN events e ON e.id = n.event_id
                       WHERE n.user_id = ?1 AND n.read_at_ms IS NULL AND json_extract(e.metadata_json, '$.needs') = 'login' AND NOT ",
            login_linked_since!(),
            ")"
        ),
        [user.to_string()],
        |r| r.get(0),
    )?;
    if unread {
        return Ok(false);
    }
    let Some((severity, title)) = notification_for(&event.event_type, &event.metadata) else { return Ok(false) };
    let ticket = (event.aggregate_type == "ticket").then(|| TicketId::parse(&event.aggregate_id).ok()).flatten();
    if let Some(ticket) = ticket
        && ticket_gate(tx, ticket, REASON_LOGIN)?.is_none()
    {
        return Ok(false);
    }
    deliver(tx, seq, event, user, &Head { reason: REASON_LOGIN, severity, title, ticket, conversation: None, message: None })
}

fn active_member(conn: &Connection, user: UserId) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM users WHERE id = ?1 AND disabled_at_ms IS NULL)", [user.to_string()], |r| r.get(0))?)
}

/// What one event says to one board member.
struct Head<'a> {
    reason: &'static str,
    severity: &'a str,
    title: &'a str,
    ticket: Option<TicketId>,
    conversation: Option<ConversationId>,
    message: Option<MessageId>,
}

/// Which reason a merged row shows: the most important, not the latest (§4).
fn reason_rank(reason: &str) -> u8 {
    match reason {
        REASON_LOGIN => 5,
        REASON_FAILED => 4,
        REASON_REVIEW | REASON_PLAN_REVIEW => 3,
        REASON_MENTION => 2,
        REASON_SENT_BACK => 1,
        _ => 0,
    }
}

/// One unread row per (user, ticket), or (user, channel) outside a ticket: a new
/// event merges into it, and the head goes to the most important reason. A row the
/// reader cannot see any more (`STILL_RELEVANT`) is not merged into: it would hide
/// this event with it, so this event is a first row again.
///
/// Push: for a first row, and always for `responsible`, `mention` and `login`. What
/// to push stays on the row under `push`, so the gateway finds it by the event's `seq`.
fn deliver(tx: &rusqlite::Transaction<'_>, seq: u64, event: &EventRow, user: UserId, head: &Head<'_>) -> Result<bool, StorageError> {
    let Ok(event_id) = EventId::parse(&event.id) else { return Ok(false) };
    let existing: Option<(String, Option<String>, bool)> = match (head.ticket, head.conversation) {
        (None, None) => None,
        (ticket, conversation) => tx
            .query_row(
                &format!(
                    "SELECT n.id, n.reason, {STILL_RELEVANT} FROM notifications n JOIN events e ON e.id = n.event_id
                     WHERE n.user_id = ?1 AND n.read_at_ms IS NULL AND (n.ticket_id = ?2 OR (?2 IS NULL AND n.ticket_id IS NULL AND n.conversation_id = ?3))
                     ORDER BY n.created_at_ms DESC LIMIT 1"
                ),
                params![user.to_string(), ticket.map(|t| t.to_string()), conversation.map(|c| c.to_string())],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?,
    };
    let (id, first) = match existing {
        Some((id, reason, true)) => {
            let takes_over = reason_rank(head.reason) >= reason_rank(reason.as_deref().unwrap_or(""));
            if takes_over {
                tx.execute(
                    "UPDATE notifications SET event_id = ?2, severity = ?3, reason = ?4, conversation_id = COALESCE(?5, conversation_id), message_id = ?6,
                         target_json = json_object('type', ?7, 'aggregate_type', ?8, 'aggregate_id', ?9, 'seq', ?10, 'title', ?11),
                         created_at_ms = ?12, merged_count = merged_count + 1 WHERE id = ?1",
                    params![
                        id,
                        event.id,
                        head.severity,
                        head.reason,
                        head.conversation.map(|c| c.to_string()),
                        head.message.map(|m| m.to_string()),
                        event.event_type,
                        event.aggregate_type,
                        event.aggregate_id,
                        seq as i64,
                        head.title,
                        now_ms(),
                    ],
                )?;
            } else {
                tx.execute("UPDATE notifications SET created_at_ms = ?2, merged_count = merged_count + 1 WHERE id = ?1", params![id, now_ms()])?;
            }
            (id, false)
        }
        stale => {
            if let Some((id, _, _)) = stale {
                tx.execute("DELETE FROM notifications WHERE id = ?1", [id])?;
            }
            let row = NewNotification { event_id, user, severity: head.severity, title: head.title, reason: head.reason, ticket: head.ticket, conversation: head.conversation, message: head.message };
            let Some(id) = insert_notification(tx, &row)? else { return Ok(false) };
            (id, true)
        }
    };
    if first || matches!(head.reason, REASON_RESPONSIBLE | REASON_MENTION | REASON_LOGIN) {
        let push = serde_json::json!({ "seq": seq, "title": head.title, "reason": head.reason, "message_id": head.message });
        tx.execute("UPDATE notifications SET target_json = json_set(target_json, '$.push', json(?2)) WHERE id = ?1", params![id, push.to_string()])?;
    }
    Ok(true)
}

/// A row's sentence for a review: "your" only for who it waits for, worked out when
/// it is read (it follows a handover).
fn review_title(reason: &str, needs_you: bool) -> &'static str {
    match (reason, needs_you) {
        (REASON_PLAN_REVIEW, true) => "Plan ready for your review",
        (REASON_PLAN_REVIEW, false) => "Plan ready for review",
        (_, true) => "Ready for your review",
        (_, false) => "Ready for review",
    }
}

/// A notification row for one board member (feature `personal_inbox`).
#[derive(Debug, Clone)]
pub struct NewNotification<'a> {
    /// The event it comes from; it gives the row its target.
    pub event_id: EventId,
    pub user: UserId,
    /// `warning` or `info`.
    pub severity: &'a str,
    /// The whole sentence an older client shows too ("Marie made you responsible").
    /// Kept in `target_json`: the table has no column for it.
    pub title: &'a str,
    /// `NotificationView::reason`, at most 32 characters.
    pub reason: &'a str,
    pub ticket: Option<TicketId>,
    pub conversation: Option<ConversationId>,
    pub message: Option<MessageId>,
}

/// Write one row, at most once per event and board member. Its id; none when it
/// is there already or the event is unknown. Who gets a row, and whether it
/// merges into an unread one, is decided before this.
pub fn insert_notification(tx: &rusqlite::Transaction<'_>, row: &NewNotification<'_>) -> Result<Option<String>, StorageError> {
    let id = EventId::new().to_string();
    let inserted = tx.execute(
        "INSERT INTO notifications(id, event_id, dedupe_key, severity, target_json, created_at_ms, user_id, ticket_id, conversation_id, message_id, reason)
         SELECT ?1, e.id, 'event:' || e.id || ':' || ?3, ?4,
                json_object('type', e.type, 'aggregate_type', e.aggregate_type, 'aggregate_id', e.aggregate_id, 'seq', e.seq, 'title', ?5),
                ?6, ?3, ?7, ?8, ?9, ?10
         FROM events e WHERE e.id = ?2
         ON CONFLICT(dedupe_key) DO NOTHING",
        params![
            id,
            row.event_id.to_string(),
            row.user.to_string(),
            row.severity,
            row.title,
            now_ms(),
            row.ticket.map(|t| t.to_string()),
            row.conversation.map(|c| c.to_string()),
            row.message.map(|m| m.to_string()),
            row.reason,
        ],
    )?;
    Ok((inserted == 1).then_some(id))
}

/// Every notification, also those for one board member (push routing, tests).
pub fn list_notifications(conn: &Connection, limit: u32) -> Result<Vec<NotificationView>, StorageError> {
    list(conn, limit, None)
}

/// What `user`'s Inbox shows: the notifications for everyone and their own
/// (none: only those for everyone).
pub fn list_notifications_for(conn: &Connection, limit: u32, user: Option<&str>) -> Result<Vec<NotificationView>, StorageError> {
    list(conn, limit, Some(viewer(user)))
}

/// Whose notifications a query sees: the board member bound at `?{at}` (`viewer`)
/// sees their own and those for everyone they did not dismiss; NULL: only those
/// for everyone.
fn visible_to(at: u8) -> String {
    format!("(n.user_id = ?{at} OR (n.user_id IS NULL AND NOT EXISTS (SELECT 1 FROM notification_dismissals d WHERE d.notification_id = n.id AND d.user_id = ?{at})))")
}

/// Whether the board member bound at `?{at}` has read a notification. A
/// `read_at_ms` on a row for everyone is from before schema 28 and counts for all.
/// A row for everyone from before they joined counts as read for them, without a
/// write, unless it asks for something: a request that is hidden once handled
/// (`STILL_RELEVANT`) is one they should see.
fn read_by(at: u8) -> String {
    format!(
        "(n.read_at_ms IS NOT NULL OR EXISTS (SELECT 1 FROM notification_reads r WHERE r.notification_id = n.id AND r.user_id = ?{at})
          OR (n.user_id IS NULL AND n.severity <> 'action' AND COALESCE(n.created_at_ms < (SELECT u.created_at_ms FROM users u WHERE u.id = ?{at}), 0)))"
    )
}

/// The value for `visible_to`: `user` when it is a board member's id, else NULL.
fn viewer(user: Option<&str>) -> Option<String> {
    user.filter(|u| aurelius_domain::UserId::parse(u).is_ok()).map(str::to_owned)
}

/// `viewer`: whose Inbox (`Some(None)`: only everyone's); `None`: every notification.
fn list(conn: &Connection, limit: u32, viewer: Option<Option<String>>) -> Result<Vec<NotificationView>, StorageError> {
    let visible = if viewer.is_some() { visible_to(2) } else { "?2 IS NULL".to_string() };
    read_rows(conn, &format!("{STILL_RELEVANT} AND {visible} ORDER BY n.created_at_ms DESC LIMIT ?1"), params![limit, viewer.flatten()])
}

/// The rows `filter` selects, with what the reader sees of them; `?2` in `filter`
/// is who reads (`read_by`). `needs_you` and "your" are worked out here from the
/// current state, so they follow a handover.
fn read_rows(conn: &Connection, filter: &str, values: &[&dyn rusqlite::ToSql]) -> Result<Vec<NotificationView>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT n.id, n.severity, n.target_json, n.created_at_ms, {read}, e.type, e.metadata_json, e.correlation_id,
                n.ticket_id, k.number, n.conversation_id, n.message_id, n.reason, n.merged_count,
                k.title, k.root_task_id, n.user_id, k.responsible_user_id,
                EXISTS(SELECT 1 FROM users u WHERE u.id = k.responsible_user_id AND u.disabled_at_ms IS NULL)
         FROM notifications n JOIN events e ON e.id = n.event_id LEFT JOIN tickets k ON k.id = n.ticket_id WHERE {filter}",
        read = read_by(2),
    ))?;
    let rows = stmt.query_map(values, |r| {
        Ok(ReadRow {
            id: r.get(0)?,
            severity: r.get(1)?,
            target: r.get(2)?,
            created: r.get(3)?,
            read: r.get(4)?,
            event_type: r.get(5)?,
            metadata: r.get(6)?,
            correlation: r.get(7)?,
            ticket: r.get(8)?,
            number: r.get(9)?,
            conversation: r.get(10)?,
            message: r.get(11)?,
            reason: r.get(12)?,
            merged: r.get(13)?,
            ticket_title: r.get(14)?,
            root_task: r.get(15)?,
            user: r.get(16)?,
            responsible: r.get(17)?,
            responsible_active: r.get(18)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        let row = row?;
        let metadata: serde_json::Value = serde_json::from_str(&row.metadata).unwrap_or_default();
        let target: serde_json::Value = serde_json::from_str(&row.target).unwrap_or_default();
        let needs_you = row.needs_you();
        // A row written by `insert_notification` carries its own sentence.
        let title = match row.reason.as_deref() {
            Some(reason @ (REASON_PLAN_REVIEW | REASON_REVIEW)) => review_title(reason, needs_you).to_owned(),
            _ => target["title"].as_str().or(notification_for(&row.event_type, &metadata).map(|(_, t)| t)).unwrap_or("Notification").to_string(),
        };
        // Discreet by default: no project content beyond the title of the ticket or task.
        let subject = match (row.number, &row.ticket_title) {
            (Some(number), Some(title)) => format!("#{number} {title}"),
            _ => lookup(conn, "SELECT title FROM tasks WHERE id = ?1", &row.correlation)
                .filter(|s| !s.is_empty())
                .or_else(|| metadata["subject"].as_str().map(str::to_owned))
                .unwrap_or_default(),
        };
        // The main task, for a client that can only open a task (an older one).
        let task_id = match &row.ticket {
            Some(_) => row.root_task.as_deref().and_then(|t| TaskId::parse(t).ok()),
            None => TaskId::parse(&row.correlation).ok().filter(|_| !lookup(conn, "SELECT id FROM tasks WHERE id = ?1", &row.correlation).unwrap_or_default().is_empty()),
        };
        out.push(NotificationView {
            id: row.id,
            severity: row.severity,
            title,
            subject,
            created_at_ms: row.created,
            read: row.read,
            task_id,
            target_type: target["type"].as_str().unwrap_or("").to_owned(),
            ticket_id: row.ticket.and_then(|t| TicketId::parse(&t).ok()),
            ticket_number: row.number.map(|n| n as u32),
            conversation_id: row.conversation.and_then(|c| ConversationId::parse(&c).ok()),
            message_id: row.message.and_then(|m| MessageId::parse(&m).ok()),
            reason: row.reason,
            merged_count: row.merged as u32,
            needs_you,
        });
    }
    Ok(out)
}

/// One row of `read_rows`.
struct ReadRow {
    id: String,
    severity: String,
    target: String,
    created: i64,
    read: bool,
    event_type: String,
    metadata: String,
    correlation: String,
    ticket: Option<String>,
    number: Option<i64>,
    conversation: Option<String>,
    message: Option<String>,
    reason: Option<String>,
    merged: i64,
    ticket_title: Option<String>,
    root_task: Option<String>,
    user: Option<String>,
    responsible: Option<String>,
    responsible_active: bool,
}

impl ReadRow {
    /// Whether this row waits for its board member, by the state of now: the active
    /// Responsible, or everyone who has a row when there is none (B6).
    fn needs_you(&self) -> bool {
        match self.reason.as_deref() {
            Some(REASON_LOGIN) => true,
            Some(REASON_REVIEW | REASON_PLAN_REVIEW | REASON_FAILED) => !self.responsible_active || self.responsible == self.user,
            _ => false,
        }
    }
}

pub fn unread_count(conn: &Connection) -> Result<u64, StorageError> {
    unread_count_for(conn, None)
}

pub fn unread_count_for(conn: &Connection, user: Option<&str>) -> Result<u64, StorageError> {
    Ok(conn.query_row(
        &format!("SELECT COUNT(*) FROM notifications n JOIN events e ON e.id = n.event_id WHERE NOT {} AND {STILL_RELEVANT} AND {}", read_by(1), visible_to(1)),
        [viewer(user)],
        |r| r.get::<_, i64>(0),
    )? as u64)
}

/// A request for the user that was handled (an approval or a colleague
/// proposal decided, answered or expired; a task no longer waiting for them) is no longer shown.
/// Nor is a row of a closed ticket, a review the ticket has left, or a failure of a
/// main task that no longer failed (B12).
const STILL_RELEVANT: &str = concat!("NOT (e.type = 'approval.requested' AND EXISTS (SELECT 1 FROM approvals a WHERE a.id = e.aggregate_id AND a.status <> 'pending'))
     AND NOT (e.type IN ('employee.proposed','employee.instructions_proposed') AND EXISTS (SELECT 1 FROM proposals p WHERE p.id = e.aggregate_id AND p.status <> 'pending'))
     AND NOT (e.type = 'guideline.proposed' AND EXISTS (SELECT 1 FROM guideline_proposals g WHERE g.id = e.aggregate_id AND g.status <> 'pending'))
     AND NOT (e.type = 'task.waiting' AND NOT EXISTS (SELECT 1 FROM tasks t WHERE t.id = e.aggregate_id AND t.status = 'waiting'))
     AND NOT (n.user_id IS NOT NULL AND json_extract(e.metadata_json, '$.needs') IS 'login' AND ", login_linked_since!(), ")
     AND NOT (n.ticket_id IS NOT NULL AND EXISTS (SELECT 1 FROM tickets k JOIN board_columns c ON c.id = k.column_id WHERE k.id = n.ticket_id AND (c.kind = 'done' OR k.archived_at_ms IS NOT NULL)))
     AND NOT (n.reason IS 'review' AND NOT EXISTS (SELECT 1 FROM tickets k JOIN board_columns c ON c.id = k.column_id WHERE k.id = n.ticket_id AND c.kind = 'review'))
     AND NOT (n.reason IS 'plan_review' AND NOT EXISTS (SELECT 1 FROM tickets k JOIN board_columns c ON c.id = k.column_id WHERE k.id = n.ticket_id AND c.kind = 'plan_review'))
     AND NOT (n.reason IS 'failed' AND NOT EXISTS (SELECT 1 FROM tickets k JOIN tasks t ON t.id = k.root_task_id WHERE k.id = n.ticket_id AND t.status = 'failed'))");



/// Remove one notification from `user`'s Inbox (the event it was about stays):
/// their own goes, one for everyone stays for the others. Answers how many went;
/// without a board member none.
pub fn dismiss_for(tx: &rusqlite::Transaction<'_>, id: &str, user: Option<&str>) -> Result<usize, StorageError> {
    let Some(user) = viewer(user) else { return Ok(0) };
    let own = tx.execute("DELETE FROM notifications WHERE id = ?1 AND user_id = ?2", params![id, user])?;
    let shared = tx.execute(
        &format!("INSERT INTO notification_dismissals(notification_id, user_id, dismissed_at_ms) SELECT n.id, ?2, ?3 FROM notifications n WHERE n.id = ?1 AND n.user_id IS NULL AND {}", visible_to(2)),
        params![id, user, now_ms()],
    )?;
    Ok(own + shared)
}

/// `dismiss_for`, of everything in `user`'s Inbox.
pub fn dismiss_all_for(tx: &rusqlite::Transaction<'_>, user: Option<&str>) -> Result<usize, StorageError> {
    let Some(user) = viewer(user) else { return Ok(0) };
    let own = tx.execute("DELETE FROM notifications WHERE user_id = ?1", [&user])?;
    let shared = tx.execute(
        &format!("INSERT INTO notification_dismissals(notification_id, user_id, dismissed_at_ms) SELECT n.id, ?1, ?2 FROM notifications n WHERE n.user_id IS NULL AND {}", visible_to(1)),
        params![user, now_ms()],
    )?;
    Ok(own + shared)
}

/// `mark_all_read_for`, of one row. Answers 1 when it was unread and `user` can see
/// it; 0 for an unknown id, someone else's row, a row that is no longer shown, one
/// already read or without a board member, so a client may repeat it.
pub fn mark_read_for(tx: &rusqlite::Transaction<'_>, id: &str, user: Option<&str>) -> Result<usize, StorageError> {
    let Some(user) = viewer(user) else { return Ok(0) };
    let now = now_ms();
    let unread = format!("FROM notifications n JOIN events e ON e.id = n.event_id WHERE n.id = ?1 AND NOT {} AND {STILL_RELEVANT}", read_by(2));
    let own = tx.execute(&format!("UPDATE notifications SET read_at_ms = ?3 WHERE id IN (SELECT n.id {unread} AND n.user_id = ?2)"), params![id, user, now])?;
    let shared = tx.execute(
        &format!("INSERT INTO notification_reads(notification_id, user_id, read_at_ms) SELECT n.id, ?2, ?3 {unread} AND n.user_id IS NULL AND {}", visible_to(2)),
        params![id, user, now],
    )?;
    Ok(own + shared)
}

/// Mark what `user` has not read as read, for them only. Answers how many; without
/// a board member none.
pub fn mark_all_read_for(tx: &rusqlite::Transaction<'_>, user: Option<&str>) -> Result<usize, StorageError> {
    let Some(user) = viewer(user) else { return Ok(0) };
    let now = now_ms();
    let own = tx.execute("UPDATE notifications SET read_at_ms = ?1 WHERE user_id = ?2 AND read_at_ms IS NULL", params![now, user])?;
    let shared = tx.execute(
        &format!(
            "INSERT INTO notification_reads(notification_id, user_id, read_at_ms) SELECT n.id, ?1, ?2 FROM notifications n WHERE n.user_id IS NULL AND {} AND NOT {}",
            visible_to(1),
            read_by(1)
        ),
        params![user, now],
    )?;
    Ok(own + shared)
}

/// A row an event made or changed, with what it points at (for push routing).
#[derive(Debug, Clone)]
pub struct FoundNotification {
    pub view: NotificationView,
    pub aggregate_type: String,
    pub aggregate_id: String,
    /// The board member it is for (`notifications.user_id`); none: for everyone.
    pub user_id: Option<String>,
}

/// The rows the event at `seq` has to push: one for everyone, or one per board
/// member. In each, the title, reason and message are this event's, also when a
/// more important reason stays the row's head. Empty for any other event.
pub fn notifications_at_seq(conn: &Connection, seq: u64) -> Result<Vec<FoundNotification>, StorageError> {
    let Some(event) = event_at(conn, seq)? else { return Ok(Vec::new()) };
    if !notifies(&event.event_type, &event.metadata) && !event.metadata["mentioned_users"].is_array() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT n.id, n.user_id, e.aggregate_type, e.aggregate_id, n.target_json FROM notifications n JOIN events e ON e.id = n.event_id
         WHERE (n.user_id IS NULL AND e.seq = ?1) OR (n.user_id IS NOT NULL AND n.read_at_ms IS NULL AND json_extract(n.target_json, '$.push.seq') = ?1)",
    )?;
    let rows = stmt
        .query_map([seq as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::new();
    for (id, user_id, aggregate_type, aggregate_id, target) in rows {
        // A row that is no longer relevant by now is not worth waking a phone for.
        let Some(mut view) = read_rows(conn, &format!("n.id = ?1 AND ?2 IS NULL AND {STILL_RELEVANT}"), params![id, None::<String>])?.pop() else { continue };
        let push = &serde_json::from_str::<serde_json::Value>(&target).unwrap_or_default()["push"];
        // A lesser reason that merged into the row: the push says what happened now.
        if let (Some(title), Some(reason)) = (push["title"].as_str(), push["reason"].as_str())
            && view.reason.as_deref() != Some(reason)
        {
            view.title = title.to_owned();
            view.reason = Some(reason.to_owned());
            view.message_id = push["message_id"].as_str().and_then(|m| MessageId::parse(m).ok());
        }
        out.push(FoundNotification { view, aggregate_type, aggregate_id, user_id });
    }
    Ok(out)
}

/// Remove what is older than `NOTIFICATION_KEEP_MS`, unless it is a request that
/// still waits for someone. Who read or dismissed it goes with it (`ON DELETE
/// CASCADE`); the event stays, so nothing of the audit goes. Answers how many went.
pub fn prune_notifications(tx: &rusqlite::Transaction<'_>, now: i64) -> Result<usize, StorageError> {
    Ok(tx.execute(
        &format!(
            "DELETE FROM notifications WHERE id IN (
               SELECT n.id FROM notifications n JOIN events e ON e.id = n.event_id
               WHERE n.created_at_ms < ?1 AND NOT (n.severity = 'action' AND {STILL_RELEVANT}))"
        ),
        [now - NOTIFICATION_KEEP_MS],
    )?)
}
