//! Heartbeat schedules and wakes. The relevance check is local SQL
//! only: new task outcomes and shares since the cursor. No inference.

use crate::{StorageError, now_ms};
use aurelius_domain::{EmployeeId, RunId};
use aurelius_domain::views::HeartbeatView;
use aurelius_domain::scheduler::heartbeat::HeartbeatRule;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

#[derive(Debug, Clone)]
pub struct Schedule {
    pub id: String,
    pub employee_id: EmployeeId,
    pub employee_name: String,
    pub rule: HeartbeatRule,
    pub enabled: bool,
    pub next_due_ms: Option<i64>,
    pub cursor_seq: i64,
    pub version: i64,
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Schedule> {
    let rule: String = r.get(2)?;
    Ok(Schedule {
        id: r.get(0)?,
        employee_id: EmployeeId::parse(&r.get::<_, String>(1)?).map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?,
        rule: serde_json::from_str(&rule).map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?,
        enabled: r.get(3)?,
        next_due_ms: r.get(4)?,
        cursor_seq: r.get(5)?,
        version: r.get(6)?,
        employee_name: r.get(7)?,
    })
}

const SELECT: &str = "SELECT s.id, s.employee_id, s.rule_json, s.enabled, s.next_due_ms, s.cursor_seq, s.version, m.name
     FROM heartbeats s JOIN employees m ON m.id = s.employee_id";

pub fn for_employee(conn: &Connection, employee: EmployeeId) -> Result<Option<Schedule>, StorageError> {
    Ok(conn.query_row(&format!("{SELECT} WHERE s.employee_id = ?1"), [employee.to_string()], row).optional()?)
}

/// Enabled heartbeats of active employees that are due.
pub fn due(conn: &Connection, now: i64) -> Result<Vec<Schedule>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT}
         WHERE s.enabled = 1 AND m.status = 'active' AND (s.next_due_ms IS NULL OR s.next_due_ms <= ?1)"
    ))?;
    let rows = stmt.query_map([now], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Create or change an employee's heartbeat. Enabling starts the cursor now:
/// only what happens from here on counts.
pub fn upsert(tx: &Transaction<'_>, employee: EmployeeId, rule: Option<HeartbeatRule>, next_due_ms: i64) -> Result<(), StorageError> {
    let seq: i64 = tx.query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |r| r.get(0))?;
    let existing = for_employee(tx, employee)?;
    match (existing, rule) {
        (None, None) => {}
        (None, Some(rule)) => {
            tx.execute(
                "INSERT INTO heartbeats(id, employee_id, rule_json, timezone, next_due_ms, enabled, version, cursor_seq)
                 VALUES (?1, ?2, ?3, 'local', ?4, 1, 1, ?5)",
                params![uuid::Uuid::now_v7().to_string(), employee.to_string(), serde_json::to_string(&rule)?, next_due_ms, seq],
            )?;
        }
        (Some(s), Some(rule)) => {
            let cursor = if s.enabled { s.cursor_seq } else { seq };
            tx.execute(
                "UPDATE heartbeats SET rule_json = ?2, enabled = 1, next_due_ms = ?3, cursor_seq = ?4, waiting_reason = NULL, version = version + 1 WHERE id = ?1",
                params![s.id, serde_json::to_string(&rule)?, next_due_ms, cursor],
            )?;
        }
        (Some(s), None) => {
            tx.execute("UPDATE heartbeats SET enabled = 0, next_due_ms = NULL, waiting_reason = NULL, version = version + 1 WHERE id = ?1", [s.id])?;
        }
    }
    Ok(())
}

/// Record an evaluation without a wake (idle, quiet hours, budget, busy).
pub fn advance(tx: &Transaction<'_>, id: &str, next_due_ms: i64, cursor_seq: Option<i64>, waiting: Option<&str>) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE heartbeats SET next_due_ms = ?2, cursor_seq = COALESCE(?3, cursor_seq), waiting_reason = ?4, last_evaluated_ms = ?5 WHERE id = ?1",
        params![id, next_due_ms, cursor_seq, waiting, now_ms()],
    )?;
    Ok(())
}

pub fn record_wake(tx: &Transaction<'_>, schedule: &str, run: RunId, occurrence_key: &str, reasons: &[String], missed: u32) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO heartbeat_wakes(run_id, schedule_id, occurrence_key, reasons_json, missed, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![run.to_string(), schedule, occurrence_key, serde_json::to_string(reasons)?, missed, now_ms()],
    )?;
    tx.execute("UPDATE heartbeats SET last_occurrence_key = ?2 WHERE id = ?1", params![schedule, occurrence_key])?;
    Ok(())
}

pub fn wake_exists(conn: &Connection, occurrence_key: &str) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM heartbeat_wakes WHERE occurrence_key = ?1)", [occurrence_key], |r| r.get(0))?)
}

/// The heartbeat wake behind a run, if any: (schedule id, employee id, employee name).
pub fn wake_for_run(conn: &Connection, run: RunId) -> Result<Option<(String, String, String)>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT h.schedule_id, s.employee_id, m.name FROM heartbeat_wakes h JOIN heartbeats s ON s.id = h.schedule_id
             JOIN employees m ON m.id = s.employee_id WHERE h.run_id = ?1 AND h.outcome IS NULL",
            [run.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?)
}

pub fn set_outcome(tx: &Transaction<'_>, run: RunId, outcome: &str) -> Result<(), StorageError> {
    tx.execute("UPDATE heartbeat_wakes SET outcome = ?2 WHERE run_id = ?1", params![run.to_string(), outcome])?;
    Ok(())
}

/// A previous wake of this schedule is still running (coalesce, don't stack).
pub fn wake_active(conn: &Connection, schedule: &str) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM heartbeat_wakes h JOIN runs r ON r.id = h.run_id
         WHERE h.schedule_id = ?1 AND r.status IN ('created','starting','running','waiting','cancelling'))",
        [schedule],
        |r| r.get(0),
    )?)
}

/// Tokens used by this heartbeat's wakes since `since_ms`.
pub fn tokens_since(conn: &Connection, schedule: &str, since_ms: i64) -> Result<u64, StorageError> {
    let n: i64 = conn.query_row(
        "SELECT COALESCE(SUM(COALESCE(u.input_tokens,0) + COALESCE(u.output_tokens,0)), 0)
         FROM usage_records u JOIN heartbeat_wakes h ON h.run_id = u.run_id
         WHERE h.schedule_id = ?1 AND u.recorded_at_ms >= ?2",
        params![schedule, since_ms],
        |r| r.get(0),
    )?;
    Ok(n.max(0) as u64)
}

/// What changed for this employee since the cursor: its own tasks and those it
/// gave away that finished or got stuck, and context shared with it. Returns (reasons, newest seq seen).
pub fn reasons(conn: &Connection, s: &Schedule) -> Result<(Vec<String>, i64), StorageError> {
    let mut reasons = Vec::new();
    let mut newest = s.cursor_seq;
    let mut stmt = conn.prepare(
        "SELECT e.seq, e.type, t.title FROM events e JOIN tasks t ON t.id = e.aggregate_id
         WHERE e.aggregate_type = 'task' AND e.seq > ?1
           AND (t.employee_id = ?2 OR t.requested_by = ?2 OR t.parent_task_id IN (SELECT id FROM tasks WHERE employee_id = ?2))
           AND e.type IN ('task.succeeded','task.failed','task.waiting')
         ORDER BY e.seq",
    )?;
    let rows = stmt.query_map(params![s.cursor_seq, s.employee_id.to_string()], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
    for row in rows {
        let (seq, kind, title) = row?;
        newest = newest.max(seq);
        let what = match kind.as_str() {
            "task.succeeded" => "finished",
            "task.failed" => "failed",
            _ => "is waiting",
        };
        let line = format!("task “{title}” {what}");
        if !reasons.contains(&line) {
            reasons.push(line);
        }
    }
    let mut stmt = conn.prepare(
        "SELECT e.seq, sh.title FROM events e JOIN shares sh ON sh.id = e.aggregate_id
         WHERE e.aggregate_type = 'share' AND e.type = 'share.granted' AND e.seq > ?1 AND sh.revoked_at_ms IS NULL
           AND sh.destination_scope_id IN (SELECT scope_id FROM employees WHERE id = ?2)
         ORDER BY e.seq",
    )?;
    let rows = stmt.query_map(params![s.cursor_seq, s.employee_id.to_string()], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (seq, title) = row?;
        newest = newest.max(seq);
        reasons.push(format!("new shared context “{title}”"));
    }
    // Everything up to now has been looked at, relevant or not.
    let last: i64 = conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |r| r.get(0))?;
    Ok((reasons, newest.max(last)))
}

pub fn view(conn: &Connection, employee: EmployeeId, day_start_ms: i64) -> Result<Option<HeartbeatView>, StorageError> {
    let Some(s) = for_employee(conn, employee)? else { return Ok(None) };
    if !s.enabled {
        return Ok(None);
    }
    let waiting: Option<String> = conn.query_row("SELECT waiting_reason FROM heartbeats WHERE id = ?1", [&s.id], |r| r.get(0))?;
    Ok(Some(HeartbeatView {
        every_minutes: s.rule.every_minutes,
        quiet_start_min: s.rule.quiet_start_min,
        quiet_end_min: s.rule.quiet_end_min,
        daily_token_budget: s.rule.daily_token_budget,
        tokens_today: tokens_since(conn, &s.id, day_start_ms)?,
        next_due_ms: s.next_due_ms,
        waiting,
    }))
}
