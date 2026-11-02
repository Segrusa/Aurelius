//! Timeline projection: human-readable cards derived
//! deterministically from event types. Routine runtime/tool noise is only
//! included on request ("Activity"). Keyset pagination on `seq`.

use crate::{StorageError, now_ms};
use aurelius_domain::{ConversationId, EventId, MessageId, ProjectId, TaskId, TicketId, UserId};
use aurelius_domain::views::{NotificationView, TimelineCard};
use rusqlite::{Connection, params};

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
    "task.variants_ready",
    "task.variant_chosen",
    "memory.remembered",
    "idea.added",
    "idea.started",
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
        "memory.remembered" => {
            let who = lookup(conn, "SELECT name FROM employees WHERE id = ?1", &s(&m["employee_id"])).unwrap_or_else(|| "You".into());
            ("minor", "MEMORY", format!("{who} noted a {}", s(&m["kind"]).replace('_', " ")), String::new(), who)
        }
        "task.succeeded" => ("outcome", "COMPLETED", task_title(), format!("{} · checked against the criteria", task_employee()), task_employee()),
        "task.failed" => ("outcome", "FAILED", task_title(), if m["escalated_to"].is_string() { format!("Escalated to {}", s(&m["escalated_to"])) } else { "Task failed".into() }, task_employee()),
        "task.cancelled" => ("decision", "DECISION", format!("{} cancelled", task_title()), String::new(), "You".into()),
        "task.variants_ready" => (
            "decision",
            "VARIANTS",
            format!("{} was tried in {} ways", task_title(), m["count"].as_u64().unwrap_or(0)),
            if m["auto"].as_bool() == Some(true) { "The judge chose".into() } else { "Choose one to go on with".into() },
            "Aurelius".into(),
        ),
        "task.variant_chosen" => (
            "decision",
            "DECISION",
            format!("Trial {} of {} chosen", m["n"].as_u64().unwrap_or(0), task_title()),
            s(&m["hint"]),
            if m["by"].as_str() == Some("judge") { "Judge".into() } else { "You".into() },
        ),
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
        "heartbeat.reported" => Some(("info", "Heartbeat update")),
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
        // The variants of a task wait for the user's choice (Harness² C).
        "task.variants_ready" if metadata["auto"].as_bool() != Some(true) => Some(("action", "Variants ready to compare")),
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

/// Create a notification for an event, deduplicated by the event. Called from
/// `events::append`, so it happens exactly once per event (never on replay).
pub fn notify(tx: &rusqlite::Transaction<'_>, seq: u64) -> Result<bool, StorageError> {
    let row: Option<(String, String, String, String, String)> = tx
        .query_row(
            "SELECT id, type, aggregate_type, aggregate_id, metadata_json FROM events WHERE seq = ?1",
            [seq as i64],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .ok();
    let Some((event_id, event_type, aggregate_type, aggregate_id, metadata)) = row else { return Ok(false) };
    let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap_or_default();
    let Some((severity, _)) = notification_for(&event_type, &metadata) else { return Ok(false) };
    // For one board member only; one unread "connect your login" at a time is enough.
    let user = metadata["for_user"].as_str();
    if let (Some(user), Some("login")) = (user, metadata["needs"].as_str()) {
        let unread: bool = tx.query_row(
            concat!(
                "SELECT EXISTS(SELECT 1 FROM notifications n JOIN events e ON e.id = n.event_id
                           WHERE n.user_id = ?1 AND n.read_at_ms IS NULL AND json_extract(e.metadata_json, '$.needs') = 'login' AND NOT ",
                login_linked_since!(),
                ")"
            ),
            [user],
            |r| r.get(0),
        )?;
        if unread {
            return Ok(false);
        }
    }
    let target = serde_json::json!({ "type": event_type, "aggregate_type": aggregate_type, "aggregate_id": aggregate_id, "seq": seq });
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO notifications(id, event_id, dedupe_key, severity, target_json, created_at_ms, user_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![aurelius_domain::EventId::new().to_string(), event_id, format!("event:{event_id}"), severity, target.to_string(), now_ms(), user],
    )?;
    Ok(inserted == 1)
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
fn read_by(at: u8) -> String {
    format!("(n.read_at_ms IS NOT NULL OR EXISTS (SELECT 1 FROM notification_reads r WHERE r.notification_id = n.id AND r.user_id = ?{at}))")
}

/// The value for `visible_to`: `user` when it is a board member's id, else NULL.
fn viewer(user: Option<&str>) -> Option<String> {
    user.filter(|u| aurelius_domain::UserId::parse(u).is_ok()).map(str::to_owned)
}

/// `viewer`: whose Inbox (`Some(None)`: only everyone's); `None`: every notification.
fn list(conn: &Connection, limit: u32, viewer: Option<Option<String>>) -> Result<Vec<NotificationView>, StorageError> {
    let visible = if viewer.is_some() { visible_to(2) } else { "?2 IS NULL".to_string() };
    let mut stmt = conn.prepare(
        &format!(
            "SELECT n.id, n.severity, n.target_json, n.created_at_ms, {read}, e.type, e.metadata_json, e.correlation_id,
                    n.ticket_id, k.number, n.conversation_id, n.message_id, n.reason, n.merged_count
             FROM notifications n JOIN events e ON e.id = n.event_id LEFT JOIN tickets k ON k.id = n.ticket_id WHERE {STILL_RELEVANT} AND {visible} ORDER BY n.created_at_ms DESC LIMIT ?1",
            read = read_by(2),
        ),
    )?;
    let rows = stmt.query_map(params![limit, viewer.flatten()], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, bool>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, Option<i64>>(9)?,
            r.get::<_, Option<String>>(10)?,
            r.get::<_, Option<String>>(11)?,
            r.get::<_, Option<String>>(12)?,
            r.get::<_, i64>(13)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, severity, target, created, read, event_type, metadata, correlation, ticket, number, conversation, message, reason, merged) = row?;
        let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap_or_default();
        let target: serde_json::Value = serde_json::from_str(&target).unwrap_or_default();
        // A row written by `insert_notification` carries its own sentence.
        let title = target["title"].as_str().or(notification_for(&event_type, &metadata).map(|(_, t)| t)).unwrap_or("Notification").to_string();
        // Discreet by default: no project content beyond the task title.
        let subject = lookup(conn, "SELECT title FROM tasks WHERE id = ?1", &correlation)
            .filter(|s| !s.is_empty())
            .or_else(|| metadata["subject"].as_str().map(str::to_owned))
            .unwrap_or_default();
        out.push(NotificationView {
            id,
            severity,
            title,
            subject,
            created_at_ms: created,
            read,
            task_id: TaskId::parse(&correlation).ok().filter(|_| !lookup(conn, "SELECT id FROM tasks WHERE id = ?1", &correlation).unwrap_or_default().is_empty()),
            target_type: target["type"].as_str().unwrap_or("").to_owned(),
            ticket_id: ticket.and_then(|t| TicketId::parse(&t).ok()),
            ticket_number: number.map(|n| n as u32),
            conversation_id: conversation.and_then(|c| ConversationId::parse(&c).ok()),
            message_id: message.and_then(|m| MessageId::parse(&m).ok()),
            reason,
            merged_count: merged as u32,
            needs_you: false,
        });
    }
    Ok(out)
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
const STILL_RELEVANT: &str = concat!("NOT (e.type = 'approval.requested' AND EXISTS (SELECT 1 FROM approvals a WHERE a.id = e.aggregate_id AND a.status <> 'pending'))
     AND NOT (e.type IN ('employee.proposed','employee.instructions_proposed') AND EXISTS (SELECT 1 FROM proposals p WHERE p.id = e.aggregate_id AND p.status <> 'pending'))
     AND NOT (e.type = 'guideline.proposed' AND EXISTS (SELECT 1 FROM guideline_proposals g WHERE g.id = e.aggregate_id AND g.status <> 'pending'))
     AND NOT (e.type = 'task.variants_ready' AND NOT EXISTS (SELECT 1 FROM task_variant_sets s WHERE s.task_id = e.aggregate_id AND s.phase = 'ready'))
     AND NOT (e.type = 'task.waiting' AND NOT EXISTS (SELECT 1 FROM tasks t WHERE t.id = e.aggregate_id AND t.status = 'waiting'))
     AND NOT (n.user_id IS NOT NULL AND json_extract(e.metadata_json, '$.needs') = 'login' AND ", login_linked_since!(), ")");



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

/// The notification created for the event at `seq`, with what it points at
/// (for push routing). `None` when that event made no notification.
#[derive(Debug, Clone)]
pub struct FoundNotification {
    pub view: NotificationView,
    pub aggregate_type: String,
    pub aggregate_id: String,
    /// The board member it is for (`notifications.user_id`); none: for everyone.
    pub user_id: Option<String>,
}

pub fn notification_at_seq(conn: &Connection, seq: u64) -> Result<Option<FoundNotification>, StorageError> {
    let found: Option<(String, String, String, Option<String>)> = conn
        .query_row(
            "SELECT n.id, e.aggregate_type, e.aggregate_id, n.user_id FROM notifications n JOIN events e ON e.id = n.event_id WHERE e.seq = ?1",
            [seq as i64],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok();
    let Some((id, aggregate_type, aggregate_id, user_id)) = found else { return Ok(None) };
    Ok(list_notifications(conn, 50)?
        .into_iter()
        .find(|n| n.id == id)
        .map(|view| FoundNotification { view, aggregate_type, aggregate_id, user_id }))
}
