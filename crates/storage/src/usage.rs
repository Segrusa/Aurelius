//! Usage records per run (tokens, and cost when the provider reports it).

use crate::{StorageError, now_ms};
use aurelius_domain::{ConversationId, EventId, RunId, TaskId};
use aurelius_domain::views::{SubtaskUsage, TokenUsage, UsageDay, UsageHour, UsageHours, UsageRow, UsageSnapshot, UsageTotals, UsageWindow};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub struct NewUsage {
    /// Input not read from the cache, cache writes included.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    /// The part of `input_tokens` written to the cache.
    pub cache_creation_tokens: u64,
    pub cost_micro_usd: Option<i64>,
    pub estimate: bool,
}

pub fn run_exists(conn: &Connection, run: RunId) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT 1 FROM runs WHERE id = ?1", [run.to_string()], |_| Ok(())).optional()?.is_some())
}

pub fn insert(tx: &Transaction<'_>, run: RunId, source_event: EventId, usage: &NewUsage) -> Result<(), StorageError> {
    let n: i64 = tx.query_row("SELECT COUNT(*) FROM usage_records WHERE run_id = ?1", [run.to_string()], |r| r.get(0))?;
    tx.execute(
        "INSERT INTO usage_records(id, run_id, source_event_id, input_tokens, output_tokens, cached_tokens, cost_microunits, currency, estimate, provider_record_key, recorded_at_ms, cache_creation_tokens)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            EventId::new().to_string(),
            run.to_string(),
            source_event.to_string(),
            usage.input_tokens as i64,
            usage.output_tokens as i64,
            usage.cached_tokens as i64,
            usage.cost_micro_usd,
            usage.cost_micro_usd.map(|_| "USD"),
            usage.estimate as i64,
            format!("{run}:{n}"),
            now_ms(),
            usage.cache_creation_tokens as i64,
        ],
    )?;
    Ok(())
}

const DAY_MS: i64 = 86_400_000;

/// Start of the local day `days_back` days before the one containing `ms`.
/// `offset_at` gives the local UTC offset in minutes at a moment, so a day
/// across a daylight saving change starts at its own midnight.
fn local_midnight(ms: i64, days_back: i64, offset_at: &impl Fn(i64) -> i64) -> i64 {
    let local = ms + offset_at(ms) * 60_000;
    aurelius_domain::schedule::local_to_utc((local.div_euclid(DAY_MS) - days_back) * DAY_MS, offset_at)
}

/// Totals for today / 7 / 30 days, per connection and model (30 days), and per day (14 days).
pub fn snapshot(conn: &Connection, offset_at: impl Fn(i64) -> i64) -> Result<UsageSnapshot, StorageError> {
    let now = now_ms();
    let today_start = local_midnight(now, 0, &offset_at);
    let window = |label: &str, since: i64| -> Result<UsageWindow, StorageError> {
        Ok(conn.query_row(
            "SELECT COUNT(DISTINCT run_id), COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), COALESCE(SUM(cached_tokens),0),
                    SUM(cost_microunits), COALESCE(MAX(estimate),0), COALESCE(SUM(cache_creation_tokens),0)
             FROM usage_records WHERE recorded_at_ms >= ?1",
            [since],
            |r| {
                Ok(UsageWindow {
                    label: label.into(),
                    runs: r.get::<_, i64>(0)? as u64,
                    input_tokens: r.get::<_, i64>(1)? as u64,
                    output_tokens: r.get::<_, i64>(2)? as u64,
                    cached_tokens: r.get::<_, i64>(3)? as u64,
                    cost_micro_usd: r.get(4)?,
                    estimate: r.get::<_, i64>(5)? == 1,
                    cache_creation_tokens: r.get::<_, i64>(6)? as u64,
                })
            },
        )?)
    };
    let windows = vec![window("Today", today_start)?, window("7 days", local_midnight(now, 6, &offset_at))?, window("30 days", local_midnight(now, 29, &offset_at))?];

    let mut stmt = conn.prepare(
        "SELECT COALESCE(c.display_name, r.runtime_kind), COALESCE(c.provider_kind, r.runtime_kind), r.model_id,
                COUNT(DISTINCT u.run_id), SUM(u.input_tokens), SUM(u.output_tokens), SUM(u.cached_tokens), SUM(u.cost_microunits), MAX(u.estimate),
                SUM(u.cache_creation_tokens)
         FROM usage_records u JOIN runs r ON r.id = u.run_id LEFT JOIN connections c ON c.id = r.connection_id
         WHERE u.recorded_at_ms >= ?1
         GROUP BY 1, 2, 3 ORDER BY SUM(u.input_tokens) + SUM(u.output_tokens) DESC",
    )?;
    let by_source = stmt
        .query_map([local_midnight(now, 29, &offset_at)], |r| {
            Ok(UsageRow {
                connection: r.get(0)?,
                provider: r.get(1)?,
                model: r.get(2)?,
                runs: r.get::<_, i64>(3)? as u64,
                input_tokens: r.get::<_, i64>(4)? as u64,
                output_tokens: r.get::<_, i64>(5)? as u64,
                cached_tokens: r.get::<_, i64>(6)? as u64,
                cost_micro_usd: r.get(7)?,
                estimate: r.get::<_, i64>(8)? == 1,
                cache_creation_tokens: r.get::<_, i64>(9)? as u64,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut by_day = Vec::new();
    for i in (0..14).rev() {
        let (start, end) = (local_midnight(now, i, &offset_at), local_midnight(now, i - 1, &offset_at));
        let tokens: i64 = conn.query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0) FROM usage_records WHERE recorded_at_ms >= ?1 AND recorded_at_ms < ?2",
            [start, end],
            |r| r.get(0),
        )?;
        by_day.push(UsageDay { day_start_ms: start, tokens: tokens as u64 });
    }
    Ok(UsageSnapshot { windows, by_source, by_day })
}

const HOUR_MS: i64 = 3_600_000;

/// Sums of the usage records `filter` selects (an SQL condition on `u`, the
/// records, and `r`, their runs, with `?1` its one parameter).
fn sum(conn: &Connection, filter: &str, param: &str) -> Result<TokenUsage, StorageError> {
    Ok(conn.query_row(
        &format!(
            "SELECT COUNT(DISTINCT u.run_id), COALESCE(SUM(u.input_tokens),0), COALESCE(SUM(u.output_tokens),0), COALESCE(SUM(u.cached_tokens),0),
                    COALESCE(SUM(u.cache_creation_tokens),0), SUM(u.cost_microunits), COALESCE(MAX(u.estimate),0)
             FROM usage_records u JOIN runs r ON r.id = u.run_id WHERE {filter}"
        ),
        [param],
        |r| {
            Ok(TokenUsage {
                runs: r.get::<_, i64>(0)? as u64,
                input_tokens: r.get::<_, i64>(1)? as u64,
                output_tokens: r.get::<_, i64>(2)? as u64,
                cached_tokens: r.get::<_, i64>(3)? as u64,
                cache_creation_tokens: r.get::<_, i64>(4)? as u64,
                cost_micro_usd: r.get(5)?,
                estimate: r.get::<_, i64>(6)? == 1,
            })
        },
    )?)
}

/// What one run used; none before its runtime reported anything.
pub fn for_run(conn: &Connection, run: RunId) -> Result<Option<TokenUsage>, StorageError> {
    let usage = sum(conn, "u.run_id = ?1", &run.to_string())?;
    Ok((usage.runs > 0).then_some(usage))
}

fn add(total: &mut TokenUsage, more: &TokenUsage) {
    total.runs += more.runs;
    total.input_tokens += more.input_tokens;
    total.output_tokens += more.output_tokens;
    total.cached_tokens += more.cached_tokens;
    total.cache_creation_tokens += more.cache_creation_tokens;
    total.cost_micro_usd = match (total.cost_micro_usd, more.cost_micro_usd) {
        (Some(a), Some(b)) => Some(a + b),
        (a, b) => a.or(b),
    };
    total.estimate |= more.estimate;
}

/// Runs of a task: its own and those in its conversation.
const TASK_RUNS: &str = "(r.task_id = ?1 OR r.conversation_id = (SELECT conversation_id FROM tasks WHERE id = ?1))";

/// A task and every task below it (`parent_task_id`), at any depth.
fn task_tree(conn: &Connection, task: TaskId) -> Result<TokenUsage, StorageError> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE tree(id) AS (SELECT ?1 UNION SELECT t.id FROM tasks t JOIN tree ON t.parent_task_id = tree.id) SELECT id FROM tree",
    )?;
    let ids: Vec<String> = stmt.query_map([task.to_string()], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut total = TokenUsage::default();
    for id in ids {
        add(&mut total, &sum(conn, TASK_RUNS, &id)?);
    }
    Ok(total)
}

/// The tasks `sql` lists (id, title, employee name for its one parameter), each with its tree.
fn subtasks(conn: &Connection, sql: &str, owner: &str) -> Result<Vec<SubtaskUsage>, StorageError> {
    let mut stmt = conn.prepare(sql)?;
    let rows: Vec<(String, String, Option<String>)> = stmt.query_map([owner], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for (id, title, employee_name) in rows {
        let Ok(task_id) = TaskId::parse(&id) else { continue };
        out.push(SubtaskUsage { task_id, title, employee_name, usage: task_tree(conn, task_id)? });
    }
    Ok(out)
}

fn totals(own: TokenUsage, subtasks: Vec<SubtaskUsage>) -> UsageTotals {
    let mut total = own.clone();
    for s in &subtasks {
        add(&mut total, &s.usage);
    }
    UsageTotals { own, subtasks, total }
}

/// What a task used: its own runs, and apart each task given away from it (with
/// that task's own sub-tasks included).
pub fn task_totals(conn: &Connection, task: TaskId) -> Result<UsageTotals, StorageError> {
    let id = task.to_string();
    let own = sum(conn, TASK_RUNS, &id)?;
    let subtasks = subtasks(
        conn,
        "SELECT t.id, t.title, m.name FROM tasks t LEFT JOIN employees m ON m.id = t.employee_id WHERE t.parent_task_id = ?1 ORDER BY t.created_at_ms, t.id",
        &id,
    )?;
    Ok(totals(own, subtasks))
}

/// What a conversation used. A task's conversation counts as its task; any other
/// lists the tasks started from it (e.g. delegated by its employee) apart.
pub fn conversation_totals(conn: &Connection, conversation: ConversationId) -> Result<UsageTotals, StorageError> {
    let id = conversation.to_string();
    let task: Option<String> = conn.query_row("SELECT id FROM tasks WHERE conversation_id = ?1", [&id], |r| r.get(0)).optional()?;
    if let Some(task) = task.and_then(|t| TaskId::parse(&t).ok()) {
        return task_totals(conn, task);
    }
    let own = sum(conn, "r.conversation_id = ?1", &id)?;
    let subtasks = subtasks(
        conn,
        "SELECT t.id, t.title, m.name FROM tasks t LEFT JOIN employees m ON m.id = t.employee_id
         WHERE t.origin_conversation_id = ?1 AND t.parent_task_id IS NULL ORDER BY t.created_at_ms, t.id",
        &id,
    )?;
    Ok(totals(own, subtasks))
}

/// Usage per hour of the local day containing `day_ms`: always 24 entries,
/// oldest first (a 25-hour day leaves its last hour out).
pub fn hours(conn: &Connection, day_ms: i64, offset_at: impl Fn(i64) -> i64) -> Result<UsageHours, StorageError> {
    let day_start = local_midnight(day_ms, 0, &offset_at);
    let day_end = local_midnight(day_ms, -1, &offset_at).min(day_start + DAY_MS);
    let mut hours: Vec<UsageHour> =
        (0..24).map(|h| UsageHour { hour_start_ms: day_start + h * HOUR_MS, ..Default::default() }).collect();
    let mut stmt = conn.prepare(
        "SELECT (recorded_at_ms - ?1) / 3600000, COUNT(DISTINCT run_id), COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0),
                COALESCE(SUM(cached_tokens),0), SUM(cost_microunits), COALESCE(MAX(estimate),0), COALESCE(SUM(cache_creation_tokens),0)
         FROM usage_records WHERE recorded_at_ms >= ?1 AND recorded_at_ms < ?2
         GROUP BY 1",
    )?;
    let rows = stmt.query_map([day_start, day_end], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)? as u64,
            r.get::<_, i64>(2)? as u64,
            r.get::<_, i64>(3)? as u64,
            r.get::<_, i64>(4)? as u64,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, i64>(6)? == 1,
            r.get::<_, i64>(7)? as u64,
        ))
    })?;
    for row in rows {
        let (h, runs, input_tokens, output_tokens, cached_tokens, cost_micro_usd, estimate, cache_creation_tokens) = row?;
        if let Some(hour) = hours.get_mut(h as usize) {
            *hour = UsageHour { hour_start_ms: hour.hour_start_ms, runs, input_tokens, output_tokens, cached_tokens, cache_creation_tokens, cost_micro_usd, estimate };
        }
    }
    Ok(UsageHours { day_start_ms: day_start, hours })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_bucket_one_local_day() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE usage_records (run_id TEXT, input_tokens INTEGER, output_tokens INTEGER, cached_tokens INTEGER,
                                         cost_microunits INTEGER, estimate INTEGER, recorded_at_ms INTEGER, cache_creation_tokens INTEGER NOT NULL DEFAULT 0)",
        )
        .unwrap();
        // UTC+2: the local day starts at 22:00 UTC the day before.
        let offset = |_: i64| 120;
        let day = 20_000 * DAY_MS - 2 * HOUR_MS;
        let add = |run: &str, at: i64, input: i64, cost: Option<i64>| {
            conn.execute(
                "INSERT INTO usage_records (run_id, input_tokens, output_tokens, cached_tokens, cost_microunits, estimate, recorded_at_ms) VALUES (?1, ?2, 10, 1, ?3, 0, ?4)",
                params![run, input, cost, at],
            )
            .unwrap();
        };
        add("a", day + 3 * HOUR_MS + 5, 100, Some(7));
        add("b", day + 3 * HOUR_MS + 59 * 60_000, 50, None);
        add("a", day + 17 * HOUR_MS, 30, None);
        add("c", day - 1, 999, Some(1));
        add("c", day + DAY_MS, 999, Some(1));

        // Any moment in the day snaps to its start.
        let h = hours(&conn, day + 12 * HOUR_MS, offset).unwrap();
        assert_eq!(h.day_start_ms, day);
        assert_eq!(h.hours.len(), 24);
        assert!(h.hours.iter().enumerate().all(|(i, x)| x.hour_start_ms == day + i as i64 * HOUR_MS));
        let three = &h.hours[3];
        assert_eq!((three.runs, three.input_tokens, three.output_tokens, three.cached_tokens), (2, 150, 20, 2));
        assert_eq!(three.cost_micro_usd, Some(7));
        let seventeen = &h.hours[17];
        assert_eq!((seventeen.runs, seventeen.input_tokens, seventeen.cost_micro_usd), (1, 30, None));
        let total: u64 = h.hours.iter().map(|x| x.input_tokens).sum();
        assert_eq!(total, 180, "records of other days are excluded");
        assert_eq!(h.hours[0], UsageHour { hour_start_ms: day, ..Default::default() });
    }

    #[test]
    fn a_task_counts_its_conversation_and_lists_the_tasks_it_gave_away() {
        let dir = tempfile::tempdir().unwrap();
        drop(crate::Store::open(dir.path()).unwrap());
        // Rows only, without what they point to.
        let conn = Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        let (parent, child, grandchild, chat) = (TaskId::new(), TaskId::new(), TaskId::new(), ConversationId::new());
        let (parent_conv, child_conv) = (ConversationId::new(), ConversationId::new());
        let task = |id: TaskId, parent: Option<TaskId>, conv: ConversationId, origin: Option<ConversationId>, at: i64| {
            conn.execute(
                "INSERT INTO tasks(id, project_id, parent_task_id, title, brief_blob_id, status, version, created_at_ms, updated_at_ms, conversation_id, origin_conversation_id)
                 VALUES (?1, 'p', ?2, ?3, 'b', 'running', 1, ?4, ?4, ?5, ?6)",
                params![id.to_string(), parent.map(|p| p.to_string()), format!("task {at}"), at, conv.to_string(), origin.map(|o| o.to_string())],
            )
            .unwrap();
        };
        task(parent, None, parent_conv, Some(chat), 1);
        task(child, Some(parent), child_conv, None, 2);
        task(grandchild, Some(child), ConversationId::new(), None, 3);
        let run = |conv: Option<ConversationId>, task: Option<TaskId>, input: i64, creation: i64| {
            let id = RunId::new().to_string();
            conn.execute(
                "INSERT INTO runs(id, task_id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status) VALUES (?1, ?2, ?3, 1, 'fake', '', '{}', 'succeeded')",
                params![id, task.map(|t| t.to_string()), conv.map(|c| c.to_string())],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO usage_records(id, run_id, source_event_id, input_tokens, output_tokens, cached_tokens, estimate, provider_record_key, recorded_at_ms, cache_creation_tokens)
                 VALUES (?1, ?1, 'e', ?2, 1, 10, 1, ?1, 0, ?3)",
                params![id, input, creation],
            )
            .unwrap();
        };
        run(Some(chat), None, 1, 0);
        run(Some(parent_conv), None, 100, 60);
        run(Some(parent_conv), None, 100, 0);
        run(Some(child_conv), None, 20, 5);
        run(None, Some(grandchild), 3, 0);

        let totals = task_totals(&conn, parent).unwrap();
        assert_eq!((totals.own.runs, totals.own.input_tokens, totals.own.cache_creation_tokens), (2, 200, 60));
        assert_eq!(totals.subtasks.len(), 1, "a grandchild counts with its child");
        assert_eq!((totals.subtasks[0].task_id, totals.subtasks[0].usage.runs, totals.subtasks[0].usage.input_tokens), (child, 2, 23));
        assert_eq!((totals.total.runs, totals.total.input_tokens, totals.total.cached_tokens), (4, 223, 40));
        assert_eq!(conversation_totals(&conn, parent_conv).unwrap(), totals, "a task's conversation is its task");
        let from_chat = conversation_totals(&conn, chat).unwrap();
        assert_eq!((from_chat.own.runs, from_chat.subtasks.len(), from_chat.total.runs), (1, 1, 5));
        let one = conn.query_row("SELECT run_id FROM usage_records WHERE input_tokens = 3", [], |r| r.get::<_, String>(0)).unwrap();
        assert_eq!(for_run(&conn, RunId::parse(&one).unwrap()).unwrap().map(|u| u.input_tokens), Some(3));
        assert_eq!(for_run(&conn, RunId::new()).unwrap(), None);
    }

    #[test]
    fn a_day_across_a_dst_change_starts_at_its_own_midnight() {
        // Brussels: UTC+2 until 2026-10-25 01:00 UTC, UTC+1 after.
        let change = 1_792_890_000_000;
        let brussels = |ms: i64| if ms < change { 120 } else { 60 };
        // Monday after the change: midnight is 23:00 UTC; Sunday's is 22:00 UTC (summer time).
        let monday_noon = change + 34 * HOUR_MS;
        assert_eq!(local_midnight(monday_noon, 0, &brussels), change + 22 * HOUR_MS);
        assert_eq!(local_midnight(monday_noon, 1, &brussels), change - 3 * HOUR_MS);
        assert_eq!(local_midnight(monday_noon, -1, &brussels), change + 46 * HOUR_MS);
    }
}
