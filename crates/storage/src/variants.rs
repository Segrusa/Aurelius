//! Task variants (`migrations/0012`, Harness² C): the server setting, one set
//! per task and a row per variant with its outcome.

use crate::{StorageError, now_ms};
use aurelius_domain::variants::{MAX_VARIANTS, VariantPhase, VariantStatus};
use aurelius_domain::views::{TaskVariantView, TaskVariantsView, VariantsSetting};
use aurelius_domain::{ConversationId, RunId, TaskId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

const SETTING_KEY: &str = "task_variants";

/// Tasks may be tried in several ways first (off unless the user turned it on).
pub fn enabled(conn: &Connection) -> Result<bool, StorageError> {
    Ok(crate::settings::get(conn, SETTING_KEY)?.and_then(|j| serde_json::from_str::<bool>(&j).ok()).unwrap_or(false))
}

pub fn setting(conn: &Connection) -> Result<VariantsSetting, StorageError> {
    Ok(VariantsSetting { enabled: enabled(conn)?, max_variants: MAX_VARIANTS })
}

pub fn set_enabled(tx: &Transaction<'_>, enabled: bool, now: i64) -> Result<VariantsSetting, StorageError> {
    crate::settings::put(tx, SETTING_KEY, if enabled { "true" } else { "false" }, now)?;
    Ok(VariantsSetting { enabled, max_variants: MAX_VARIANTS })
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetRow {
    pub task_id: TaskId,
    pub phase: VariantPhase,
    pub count: u8,
    pub repo: Option<String>,
    pub base_commit: Option<String>,
    pub judge_pick: Option<u8>,
    pub compose: Option<String>,
    pub chosen: Option<u8>,
    pub chosen_by: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariantRow {
    pub n: u8,
    pub hint: String,
    pub status: VariantStatus,
    pub conversation_id: Option<ConversationId>,
    pub run_id: Option<RunId>,
    pub worktree: Option<String>,
    pub collected: bool,
    pub files: u32,
    pub insertions: u32,
    pub deletions: u32,
    pub diff_key: Option<String>,
    pub diff_truncated: bool,
    pub report_key: Option<String>,
    pub errors: u32,
    pub tests: Option<String>,
    pub rank: Option<u8>,
    pub judge_note: Option<String>,
}

/// A task about to try `count` variants in `repo`.
pub fn create_set(tx: &Transaction<'_>, task: TaskId, count: u8, repo: &str) -> Result<bool, StorageError> {
    let now = now_ms();
    Ok(tx.execute(
        "INSERT OR IGNORE INTO task_variant_sets(task_id, phase, count, repo, created_at_ms, updated_at_ms) VALUES (?1, 'proposing', ?2, ?3, ?4, ?4)",
        params![task.to_string(), count, repo, now],
    )? == 1)
}

pub fn load_set(conn: &Connection, task: TaskId) -> Result<Option<SetRow>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT phase, count, repo, base_commit, judge_pick, compose, chosen, chosen_by, error FROM task_variant_sets WHERE task_id = ?1",
            [task.to_string()],
            |r| {
                Ok(SetRow {
                    task_id: task,
                    phase: VariantPhase::parse(&r.get::<_, String>(0)?).unwrap_or(VariantPhase::Failed),
                    count: r.get::<_, i64>(1)? as u8,
                    repo: r.get(2)?,
                    base_commit: r.get(3)?,
                    judge_pick: r.get::<_, Option<i64>>(4)?.map(|v| v as u8),
                    compose: r.get(5)?,
                    chosen: r.get::<_, Option<i64>>(6)?.map(|v| v as u8),
                    chosen_by: r.get(7)?,
                    error: r.get(8)?,
                })
            },
        )
        .optional()?)
}

/// The phase of a task's variants, if it has some.
pub fn phase(conn: &Connection, task: TaskId) -> Result<Option<VariantPhase>, StorageError> {
    let phase: Option<String> = conn.query_row("SELECT phase FROM task_variant_sets WHERE task_id = ?1", [task.to_string()], |r| r.get(0)).optional()?;
    Ok(phase.and_then(|p| VariantPhase::parse(&p)))
}

pub fn variants(conn: &Connection, task: TaskId) -> Result<Vec<VariantRow>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT v.n, v.hint, v.status, v.conversation_id, v.run_id, v.worktree, v.collected, v.files, v.insertions, v.deletions,
                db.storage_key, v.diff_truncated, rb.storage_key, v.errors, v.tests, v.rank, v.judge_note
         FROM task_variants v LEFT JOIN blobs db ON db.id = v.diff_blob_id LEFT JOIN blobs rb ON rb.id = v.report_blob_id
         WHERE v.task_id = ?1 ORDER BY v.n",
    )?;
    let rows = stmt.query_map([task.to_string()], |r| {
        Ok(VariantRow {
            n: r.get::<_, i64>(0)? as u8,
            hint: r.get(1)?,
            status: VariantStatus::parse(&r.get::<_, String>(2)?).unwrap_or(VariantStatus::Failed),
            conversation_id: r.get::<_, Option<String>>(3)?.and_then(|c| ConversationId::parse(&c).ok()),
            run_id: r.get::<_, Option<String>>(4)?.and_then(|c| RunId::parse(&c).ok()),
            worktree: r.get(5)?,
            collected: r.get::<_, i64>(6)? == 1,
            files: r.get::<_, i64>(7)?.max(0) as u32,
            insertions: r.get::<_, i64>(8)?.max(0) as u32,
            deletions: r.get::<_, i64>(9)?.max(0) as u32,
            diff_key: r.get(10)?,
            diff_truncated: r.get::<_, i64>(11)? == 1,
            report_key: r.get(12)?,
            errors: r.get::<_, i64>(13)?.max(0) as u32,
            tests: r.get(14)?,
            rank: r.get::<_, Option<i64>>(15)?.map(|v| v as u8),
            judge_note: r.get(16)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn set_phase(tx: &Transaction<'_>, task: TaskId, phase: VariantPhase, error: Option<&str>) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE task_variant_sets SET phase = ?2, error = COALESCE(?3, error), updated_at_ms = ?4 WHERE task_id = ?1",
        params![task.to_string(), phase.as_str(), error, now_ms()],
    )?;
    Ok(())
}

/// The approaches are known: one waiting variant each, from `base_commit`.
pub fn set_hints(tx: &Transaction<'_>, task: TaskId, base_commit: &str, hints: &[String]) -> Result<(), StorageError> {
    let now = now_ms();
    tx.execute("UPDATE task_variant_sets SET base_commit = ?2, phase = 'probing', updated_at_ms = ?3 WHERE task_id = ?1", params![task.to_string(), base_commit, now])?;
    for (i, hint) in hints.iter().enumerate() {
        tx.execute(
            "INSERT OR IGNORE INTO task_variants(task_id, n, hint, status, created_at_ms) VALUES (?1, ?2, ?3, 'waiting', ?4)",
            params![task.to_string(), i as i64 + 1, hint, now],
        )?;
    }
    Ok(())
}

pub fn start_variant(tx: &Transaction<'_>, task: TaskId, n: u8, conversation: ConversationId, worktree: &str) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE task_variants SET status = 'running', conversation_id = ?3, worktree = ?4 WHERE task_id = ?1 AND n = ?2",
        params![task.to_string(), n, conversation.to_string(), worktree],
    )?;
    Ok(())
}

/// The run of a variant that started.
pub fn set_variant_run(tx: &Transaction<'_>, task: TaskId, n: u8, run: RunId) -> Result<(), StorageError> {
    tx.execute("UPDATE task_variants SET run_id = ?3 WHERE task_id = ?1 AND n = ?2", params![task.to_string(), n, run.to_string()])?;
    Ok(())
}

/// A variant's run ended; returns false when it was not running.
pub fn end_variant(tx: &Transaction<'_>, task: TaskId, n: u8, status: VariantStatus) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE task_variants SET status = ?3, ended_at_ms = ?4 WHERE task_id = ?1 AND n = ?2 AND status IN ('waiting','running')",
        params![task.to_string(), n, status.as_str(), now_ms()],
    )? == 1)
}

/// What a variant's run left: its diff and size, closing answer, errors and tests.
#[allow(clippy::too_many_arguments)]
pub fn record_outcome(
    tx: &Transaction<'_>,
    task: TaskId,
    n: u8,
    stat: (u32, u32, u32),
    diff_blob: Option<&str>,
    truncated: bool,
    report_blob: Option<&str>,
    errors: u32,
    tests: Option<&str>,
) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE task_variants SET collected = 1, files = ?3, insertions = ?4, deletions = ?5, diff_blob_id = ?6, diff_truncated = ?7, report_blob_id = COALESCE(?8, report_blob_id), errors = ?9, tests = ?10
         WHERE task_id = ?1 AND n = ?2",
        params![task.to_string(), n, stat.0, stat.1, stat.2, diff_blob, truncated as i64, report_blob, errors, tests],
    )?;
    Ok(())
}

/// The judge's ranking: places, notes and the pick; the set waits for the user.
pub fn set_judgement(tx: &Transaction<'_>, task: TaskId, order: &[u8], notes: &[(u8, String)], compose: Option<&str>) -> Result<(), StorageError> {
    for (place, n) in order.iter().enumerate() {
        let note = notes.iter().find(|(v, _)| v == n).map(|(_, w)| w.as_str()).filter(|w| !w.is_empty());
        tx.execute("UPDATE task_variants SET rank = ?3, judge_note = ?4 WHERE task_id = ?1 AND n = ?2", params![task.to_string(), n, place as i64 + 1, note])?;
    }
    tx.execute(
        "UPDATE task_variant_sets SET judge_pick = ?2, compose = ?3, phase = 'ready', updated_at_ms = ?4 WHERE task_id = ?1",
        params![task.to_string(), order.first().copied(), compose, now_ms()],
    )?;
    Ok(())
}

/// Chosen (by `user` or `judge`): its diff goes into the work folder next.
pub fn choose(tx: &Transaction<'_>, task: TaskId, n: u8, by: &str) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE task_variant_sets SET chosen = ?2, chosen_by = ?3, phase = 'applying', updated_at_ms = ?4 WHERE task_id = ?1 AND phase = 'ready'",
        params![task.to_string(), n, by, now_ms()],
    )? == 1)
}

/// The variant's worktree is gone.
pub fn clear_worktree(tx: &Transaction<'_>, task: TaskId, n: u8) -> Result<(), StorageError> {
    tx.execute("UPDATE task_variants SET worktree = NULL WHERE task_id = ?1 AND n = ?2", params![task.to_string(), n])?;
    Ok(())
}

/// A probe: the variant a conversation belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub task_id: TaskId,
    pub n: u8,
    pub hint: String,
    pub worktree: Option<String>,
}

pub fn probe_of_conversation(conn: &Connection, conversation: ConversationId) -> Result<Option<Probe>, StorageError> {
    Ok(conn
        .query_row("SELECT task_id, n, hint, worktree FROM task_variants WHERE conversation_id = ?1", [conversation.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?))
        })
        .optional()?
        .and_then(|(t, n, hint, worktree)| Some(Probe { task_id: TaskId::parse(&t).ok()?, n: n as u8, hint, worktree })))
}

pub fn probe_of_run(conn: &Connection, run: RunId) -> Result<Option<Probe>, StorageError> {
    let conversation: Option<String> = conn.query_row("SELECT conversation_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).optional()?.flatten();
    match conversation.and_then(|c| ConversationId::parse(&c).ok()) {
        Some(c) => probe_of_conversation(conn, c),
        None => Ok(None),
    }
}

/// Runs of a task's variants that still go on.
pub fn live_probe_runs(conn: &Connection, task: TaskId) -> Result<Vec<RunId>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT r.id FROM runs r JOIN task_variants v ON v.conversation_id = r.conversation_id
         WHERE v.task_id = ?1 AND r.status IN ('created','starting','running','waiting','cancelling')",
    )?;
    let rows = stmt.query_map([task.to_string()], |r| r.get::<_, String>(0))?;
    Ok(rows.filter_map(Result::ok).filter_map(|r| RunId::parse(&r).ok()).collect())
}

/// Sets whose worktrees must still be removed (their phase is over, or another variant was chosen).
pub fn worktrees_to_remove(conn: &Connection, task: TaskId) -> Result<Vec<(u8, String)>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT v.n, v.worktree FROM task_variants v JOIN task_variant_sets s ON s.task_id = v.task_id
         WHERE v.task_id = ?1 AND v.worktree IS NOT NULL AND v.status NOT IN ('waiting','running')
           AND (s.phase IN ('done','stopped','failed') OR (s.phase = 'applying' AND v.n <> s.chosen))",
    )?;
    let rows = stmt.query_map([task.to_string()], |r| Ok((r.get::<_, i64>(0)? as u8, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The view of a task's variants, with diffs and answers read from their blobs.
pub fn view(conn: &Connection, task: TaskId, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Option<TaskVariantsView>, StorageError> {
    let Some(set) = load_set(conn, task)? else { return Ok(None) };
    let mut out = Vec::new();
    for v in variants(conn, task)? {
        let usage = match v.conversation_id {
            Some(c) => crate::usage::conversation_totals(conn, c)?.own,
            None => Default::default(),
        };
        out.push(TaskVariantView {
            n: v.n,
            hint: v.hint,
            status: v.status,
            conversation_id: v.conversation_id,
            run_id: v.run_id,
            files: v.files,
            insertions: v.insertions,
            deletions: v.deletions,
            diff: v.diff_key.as_deref().and_then(read_blob).unwrap_or_default(),
            diff_truncated: v.diff_truncated,
            report: v.report_key.as_deref().and_then(read_blob).unwrap_or_default(),
            errors: v.errors,
            tests: v.tests,
            usage,
            rank: v.rank,
            judge_note: v.judge_note,
        });
    }
    Ok(Some(TaskVariantsView {
        task_id: task,
        phase: set.phase,
        count: set.count,
        base_commit: set.base_commit,
        judge_pick: set.judge_pick,
        compose: set.compose,
        chosen: set.chosen,
        chosen_by: set.chosen_by,
        error: set.error,
        variants: out,
    }))
}
