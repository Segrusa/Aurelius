//! Recurring tasks (see `aurelius_domain::schedule`).

use crate::tasks::TaskAttachments;
use crate::{StorageError, now_ms};
use aurelius_domain::{EmployeeId, ProjectId, TaskId, TaskScheduleId, schedule::Recurrence, task::RunSettings};
use aurelius_domain::views::TaskScheduleView;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

#[derive(Debug, Clone)]
pub struct ScheduleRow {
    pub id: TaskScheduleId,
    pub project_id: ProjectId,
    pub employee_id: EmployeeId,
    pub title: String,
    pub goal: String,
    pub criteria: Vec<String>,
    pub attachments: Option<TaskAttachments>,
    pub run: RunSettings,
    pub recurrence: Recurrence,
    pub paused: bool,
    pub anchor_ms: i64,
    pub next_run_ms: Option<i64>,
    pub last_run_ms: Option<i64>,
    pub last_task_id: Option<TaskId>,
    pub last_skipped_ms: Option<i64>,
    pub created_at_ms: i64,
}

const COLUMNS: &str = "id, project_id, employee_id, title, goal, criteria_json, attachments_json, run_json, recurrence_json, paused, anchor_ms,
    next_run_ms, last_run_ms, last_task_id, last_skipped_ms, created_at_ms";

fn row_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<Option<ScheduleRow>> {
    let (id, project, employee): (String, String, String) = (r.get(0)?, r.get(1)?, r.get(2)?);
    let criteria: String = r.get(5)?;
    let attachments: Option<String> = r.get(6)?;
    let run: Option<String> = r.get(7)?;
    let recurrence: String = r.get(8)?;
    let last_task: Option<String> = r.get(13)?;
    let (Ok(id), Ok(project_id), Ok(employee_id), Ok(recurrence)) =
        (TaskScheduleId::parse(&id), ProjectId::parse(&project), EmployeeId::parse(&employee), serde_json::from_str::<Recurrence>(&recurrence))
    else {
        return Ok(None);
    };
    Ok(Some(ScheduleRow {
        id,
        project_id,
        employee_id,
        title: r.get(3)?,
        goal: r.get(4)?,
        criteria: serde_json::from_str(&criteria).unwrap_or_default(),
        attachments: attachments.and_then(|a| serde_json::from_str(&a).ok()),
        run: run.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default(),
        recurrence,
        paused: r.get::<_, i64>(9)? != 0,
        anchor_ms: r.get(10)?,
        next_run_ms: r.get(11)?,
        last_run_ms: r.get(12)?,
        last_task_id: last_task.and_then(|t| TaskId::parse(&t).ok()),
        last_skipped_ms: r.get(14)?,
        created_at_ms: r.get(15)?,
    }))
}

fn json_of<T: serde::Serialize>(value: &T) -> Result<String, StorageError> {
    Ok(serde_json::to_string(value)?)
}

pub fn insert(tx: &Transaction<'_>, row: &ScheduleRow) -> Result<(), StorageError> {
    let now = now_ms();
    tx.execute(
        "INSERT INTO recurring_tasks(id, project_id, employee_id, title, goal, criteria_json, attachments_json, run_json, recurrence_json, paused, anchor_ms,
                                    next_run_ms, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
        params![
            row.id.to_string(),
            row.project_id.to_string(),
            row.employee_id.to_string(),
            row.title,
            row.goal,
            json_of(&row.criteria)?,
            row.attachments.as_ref().filter(|a| !a.files.is_empty()).map(json_of).transpose()?,
            (!row.run.is_empty()).then(|| json_of(&row.run)).transpose()?,
            json_of(&row.recurrence)?,
            row.paused as i64,
            row.anchor_ms,
            row.next_run_ms,
            now,
        ],
    )?;
    Ok(())
}

/// Replace the template and the recurrence; `next_run_ms` is the new due time.
pub fn update(tx: &Transaction<'_>, row: &ScheduleRow) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE recurring_tasks SET employee_id = ?2, title = ?3, goal = ?4, criteria_json = ?5, attachments_json = ?6, run_json = ?7, recurrence_json = ?8,
                anchor_ms = ?9, next_run_ms = ?10, updated_at_ms = ?11 WHERE id = ?1",
        params![
            row.id.to_string(),
            row.employee_id.to_string(),
            row.title,
            row.goal,
            json_of(&row.criteria)?,
            row.attachments.as_ref().filter(|a| !a.files.is_empty()).map(json_of).transpose()?,
            (!row.run.is_empty()).then(|| json_of(&row.run)).transpose()?,
            json_of(&row.recurrence)?,
            row.anchor_ms,
            row.next_run_ms,
            now_ms(),
        ],
    )?;
    Ok(())
}

pub fn load(conn: &Connection, id: TaskScheduleId) -> Result<Option<ScheduleRow>, StorageError> {
    Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM recurring_tasks WHERE id = ?1"), [id.to_string()], row_of).optional()?.flatten())
}

/// Active schedules whose due time has come.
pub fn due(conn: &Connection, now: i64) -> Result<Vec<TaskScheduleId>, StorageError> {
    let mut stmt = conn.prepare("SELECT id FROM recurring_tasks WHERE paused = 0 AND next_run_ms IS NOT NULL AND next_run_ms <= ?1 ORDER BY next_run_ms, id")?;
    let ids = stmt.query_map([now], |r| r.get::<_, String>(0))?.filter_map(|r| r.ok()).filter_map(|id| TaskScheduleId::parse(&id).ok()).collect();
    Ok(ids)
}

/// The newest task of the schedule that has not ended.
pub fn open_task(conn: &Connection, id: TaskScheduleId) -> Result<Option<TaskId>, StorageError> {
    let task: Option<String> = conn
        .query_row(
            "SELECT id FROM tasks WHERE schedule_id = ?1 AND status NOT IN ('succeeded','failed','cancelled') ORDER BY created_at_ms DESC LIMIT 1",
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(task.and_then(|t| TaskId::parse(&t).ok()))
}

/// A task was made at `now`; the next one is due at `next`.
pub fn mark_fired(tx: &Transaction<'_>, id: TaskScheduleId, now: i64, next: Option<i64>, task: TaskId) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE recurring_tasks SET last_run_ms = ?2, last_task_id = ?3, next_run_ms = COALESCE(?4, next_run_ms), updated_at_ms = ?2 WHERE id = ?1",
        params![id.to_string(), now, task.to_string(), next],
    )?;
    Ok(())
}

/// A due run was skipped (the previous task is still open, or the employee cannot work).
pub fn mark_skipped(tx: &Transaction<'_>, id: TaskScheduleId, now: i64, next: i64) -> Result<(), StorageError> {
    tx.execute("UPDATE recurring_tasks SET last_skipped_ms = ?2, next_run_ms = ?3, updated_at_ms = ?2 WHERE id = ?1", params![id.to_string(), now, next])?;
    Ok(())
}

pub fn set_paused(tx: &Transaction<'_>, id: TaskScheduleId, paused: bool, next: Option<i64>) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE recurring_tasks SET paused = ?2, next_run_ms = ?3, updated_at_ms = ?4 WHERE id = ?1",
        params![id.to_string(), paused as i64, next, now_ms()],
    )?;
    Ok(())
}

/// Delete the schedule; tasks it made stay, without the link.
pub fn delete(tx: &Transaction<'_>, id: TaskScheduleId) -> Result<(), StorageError> {
    tx.execute("UPDATE tasks SET schedule_id = NULL WHERE schedule_id = ?1", [id.to_string()])?;
    tx.execute("DELETE FROM recurring_tasks WHERE id = ?1", [id.to_string()])?;
    Ok(())
}

/// Schedules of a project (or all), oldest first, as clients see them.
pub fn views(conn: &Connection, project: Option<ProjectId>) -> Result<Vec<TaskScheduleView>, StorageError> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM recurring_tasks WHERE (?1 IS NULL OR project_id = ?1) ORDER BY created_at_ms, id"))?;
    let rows: Vec<ScheduleRow> = stmt.query_map([project.map(|p| p.to_string())], row_of)?.filter_map(|r| r.ok().flatten()).collect();
    let mut out = Vec::new();
    for row in rows {
        let employee_name: Option<String> = conn.query_row("SELECT name FROM employees WHERE id = ?1", [row.employee_id.to_string()], |r| r.get(0)).optional()?;
        out.push(TaskScheduleView {
            id: row.id,
            project_id: row.project_id,
            employee_id: row.employee_id,
            employee_name: employee_name.unwrap_or_default(),
            title: row.title,
            goal: row.goal,
            criteria: row.criteria,
            attachments: row
                .attachments
                .map(|mut a| {
                    aurelius_domain::conversation::with_upload_ids(aurelius_domain::conversation::UploadOwner::Schedule(row.id), &mut a.files);
                    a.files
                })
                .unwrap_or_default(),
            run: row.run,
            recurrence: row.recurrence,
            paused: row.paused,
            next_run_ms: row.next_run_ms.filter(|_| !row.paused),
            last_run_ms: row.last_run_ms,
            last_task_id: row.last_task_id,
            last_skipped_ms: row.last_skipped_ms,
            open_task_id: open_task(conn, row.id)?,
            created_at_ms: row.created_at_ms,
        });
    }
    Ok(out)
}

/// Whose Claude login the tasks of a recurring task run on (migration 21;
/// none: they wait with 'login').
pub fn set_login_user(tx: &Transaction<'_>, id: TaskScheduleId, login_user: Option<&str>) -> Result<(), StorageError> {
    tx.execute("UPDATE recurring_tasks SET login_user = ?2 WHERE id = ?1", params![id.to_string(), login_user])?;
    Ok(())
}

pub fn login_user(conn: &Connection, id: TaskScheduleId) -> Result<Option<String>, StorageError> {
    Ok(conn.query_row("SELECT login_user FROM recurring_tasks WHERE id = ?1", [id.to_string()], |r| r.get(0)).optional()?.flatten())
}
