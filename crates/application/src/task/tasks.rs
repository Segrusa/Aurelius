//! Tasks. Commands create and cancel tasks; the
//! daemon's executor drives the steps below. Every step is a short,
//! lease-fenced transaction with events.

use crate::run::events::{Ev, emit};
use crate::task::task_threads::{self as thread, Speaker};
use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, AppError, CommandEnvelope, CommandResult, DomainError, EventNotice, EmployeeId, TaskId,
    employee::{LifecycleStatus, TaskPolicy},
    task::{self, TaskStatus},
};
use aurelius_storage::{StorageError, Store, Tx, blobs, employees, queries, tasks as store};

pub const TASK_JOB_KIND: &str = "task.run";

pub(crate) fn ev<'a>(aggregate_type: &'a str, id: String, event_type: &'a str, actor: ActorKind, correlation: String, payload: Option<String>, metadata: serde_json::Value) -> Ev<'a> {
    let dedupe = format!("{aggregate_type}:{id}:{event_type}:{}", aurelius_domain::EventId::new());
    Ev { aggregate_type, aggregate_id: id, event_type, actor, actor_id: None, correlation, payload, metadata, dedupe }
}

pub(crate) fn task_event(tx: &Tx<'_>, task: TaskId, project: aurelius_domain::ProjectId, event_type: &'static str, metadata: serde_json::Value) -> Result<EventNotice, StorageError> {
    let mut metadata = metadata;
    // A sub-task is followed up by the employee who delegated it, not by the user.
    if store::load_task(tx, task)?.is_some_and(|t| t.parent.is_some()) {
        if let Some(m) = metadata.as_object_mut() {
            m.insert("sub_task".into(), serde_json::Value::Bool(true));
        }
    }
    let mut n = emit(tx, ev("task", task.to_string(), event_type, ActorKind::Employee, task.to_string(), None, metadata))?.1;
    n.project_id = Some(project);
    Ok(n)
}

/// Everything the executor needs for the next step of a task.
#[derive(Debug, Clone)]
pub struct TaskContext {
    pub task_id: TaskId,
    pub project_id: aurelius_domain::ProjectId,
    pub title: String,
    pub goal: String,
    pub criteria: Vec<String>,
    pub status: TaskStatus,
    pub employee_id: Option<EmployeeId>,
    pub employee_name: String,
    pub employee_status: Option<LifecycleStatus>,
    /// Employee identity and instructions (trusted system text).
    pub employee_system: String,
    pub policy: TaskPolicy,
    /// The task this one was delegated from, and the employee who follows it up.
    pub parent: Option<ParentInfo>,
    /// Sub-tasks this task's employee delegated, oldest first.
    pub children: Vec<ChildState>,
    /// The last hand-over to this employee: (from, reason).
    pub handed_over_by: Option<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct ParentInfo {
    pub task_id: TaskId,
    pub title: String,
    pub employee_name: String,
}

#[derive(Debug, Clone)]
pub struct ChildState {
    pub id: TaskId,
    pub title: String,
    pub employee_name: String,
    pub status: TaskStatus,
    pub report: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Colleague {
    pub id: EmployeeId,
    pub name: String,
    pub role: String,
    /// What the colleague is for (P7): its description, else the first line of its instructions.
    pub focus: String,
    /// The position on the org chart, if one is set.
    pub title: Option<String>,
    pub reports_to: Option<EmployeeId>,
}

impl TaskContext {
    /// Any sub-task still going.
    pub fn children_open(&self) -> bool {
        self.children.iter().any(|c| !c.status.is_terminal())
    }
}

/// The task as the opening of its conversation: what, from whom, and the criteria.
pub(crate) fn brief_text(ctx: &TaskContext) -> String {
    let origin = match (&ctx.parent, &ctx.handed_over_by) {
        (Some(p), _) => format!("{} delegated this to you as part of “{}”.", p.employee_name, p.title),
        (None, Some((from, reason))) if reason.is_empty() => format!("{from} handed this task over to you."),
        (None, Some((from, reason))) => format!("{from} handed this task over to you: {reason}"),
        (None, None) => "The user gave you this task.".into(),
    };
    let mut out = format!("**{}**\n\n{origin}\n\n**Goal**\n\n{}\n", ctx.title, ctx.goal.trim());
    if !ctx.criteria.is_empty() {
        out.push_str("\n**Acceptance criteria**\n\n");
        for c in &ctx.criteria {
            out.push_str(&format!("- {c}\n"));
        }
    }
    out
}

pub(crate) type Identity = (String, LifecycleStatus, String, TaskPolicy);

/// An employee's name, status, identity text and policy.
pub(crate) fn employee_identity(conn: &rusqlite::Connection, m: EmployeeId, read: &dyn Fn(&str) -> Option<String>) -> Result<Option<Identity>, StorageError> {
    use rusqlite::OptionalExtension;
    let row = conn
        .query_row(
            "SELECT m.name, m.role, m.status, m.config_json, b.storage_key FROM employees m
             LEFT JOIN blobs b ON b.id = m.instructions_blob_id WHERE m.id = ?1",
            [m.to_string()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?)),
        )
        .optional()?;
    let Some((name, role, status, config, instr)) = row else { return Ok(None) };
    let policy = serde_json::from_str::<aurelius_domain::employee::EmployeeConfig>(&config).map(|c| c.task_policy).unwrap_or_default();
    let instructions = instr.as_deref().and_then(read).unwrap_or_default();
    Ok(Some((
        name.clone(),
        LifecycleStatus::parse(&status).unwrap_or(LifecycleStatus::Active),
        format!("You are {name}, {role}. {instructions}").trim().to_string(),
        policy,
    )))
}

/// The company's active employees, for handing over and delegating (any of them
/// may work in any project), oldest first.
pub(crate) fn colleagues(conn: &rusqlite::Connection, read: &dyn Fn(&str) -> Option<String>) -> Result<Vec<Colleague>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.name, m.role, b.storage_key, m.title, m.reports_to, m.description FROM employees m LEFT JOIN blobs b ON b.id = m.instructions_blob_id
         WHERE m.status = 'active' ORDER BY m.created_at_ms, m.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, role, key, title, reports_to, description) = row?;
        let Ok(id) = EmployeeId::parse(&id) else { continue };
        let first_line = || key.as_deref().and_then(read).and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.chars().take(aurelius_domain::employee::DESCRIPTION_MAX).collect()));
        let focus = if description.is_empty() { first_line().unwrap_or_default() } else { description };
        out.push(Colleague { id, name, role, focus, title, reports_to: reports_to.and_then(|m| EmployeeId::parse(&m).ok()) });
    }
    Ok(out)
}

/// The colleagues as lines for a system text, in the order given, each with the
/// `@Name` that reaches them and when to hand them work (P7); the management team
/// (no boss) is marked once there is a chart (someone has a boss): in a flat
/// company everyone would be.
pub(crate) fn colleague_lines(colleagues: &[&Colleague]) -> String {
    let charted = colleagues.iter().any(|c| c.reports_to.is_some());
    let line = |c: &Colleague| {
        let team = if charted && c.reports_to.is_none() { ", management team" } else { "" };
        if c.focus.is_empty() { format!("- @{} ({}{team})", c.name, c.role) } else { format!("- @{} ({}{team}): {}", c.name, c.role, c.focus) }
    };
    colleagues.iter().map(|c| line(c)).collect::<Vec<_>>().join("\n")
}

/// The parent's job continues when a sub-task ended (it waits for its sub-tasks).
pub(crate) fn wake_parent(tx: &Tx<'_>, task: TaskId) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE jobs SET not_before_ms = ?2, last_error_code = NULL WHERE state = 'ready'
         AND id = (SELECT parent_task_id FROM tasks WHERE id = ?1)",
        (task.to_string(), aurelius_storage::now_ms()),
    )?;
    Ok(())
}

/// Why `task.assign` and `task.cancel` refuse the main task of a ticket.
const USE_THE_BOARD: &str = "This is the main task of a ticket: use the board (assign or cancel the ticket).";

/// Cancel the sub-tasks of a cancelled task (and theirs).
pub(crate) fn cancel_children(tx: &Tx<'_>, st: &Store, task: TaskId, notices: &mut Vec<EventNotice>) -> Result<(), StorageError> {
    for child in store::children_of(tx, task)? {
        if child.status.is_terminal() {
            continue;
        }
        let Some(row) = store::load_task(tx, child.id)? else { continue };
        notices.extend(crate::task::task_line::set_status(tx, child.id, TaskStatus::Cancelled, None)?);
        tx.execute("UPDATE jobs SET state = 'cancelled' WHERE id = ?1 AND state = 'ready'", [child.id.to_string()])?;
        notices.push(task_event(tx, child.id, row.project_id, "task.cancelled", serde_json::json!({ "reason": "parent_cancelled" }))?);
        notices.extend(crate::run::queue::stop_parked_runs_of_task(tx, st, child.id, &child.id.to_string())?);
        cancel_children(tx, st, child.id, notices)?;
    }
    Ok(())
}

/// Cancel a task that has not ended, with everything that hangs on it (its
/// parked runs, sub-tasks), and let whoever waits on it go on.
pub(crate) fn cancel_in(tx: &Tx<'_>, st: &Store, task_id: TaskId, correlation: &str, meta: serde_json::Value) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = store::load_task(tx, task_id)? else { return Ok(vec![]) };
    let mut notices = crate::task::task_line::set_status(tx, task_id, TaskStatus::Cancelled, None)?.into_vec();
    tx.execute("UPDATE jobs SET state = 'cancelled' WHERE id = ?1 AND state = 'ready'", [task_id.to_string()])?;
    notices.push(task_event(tx, task_id, row.project_id, "task.cancelled", meta)?);
    // Its runs parked for a usage limit end now; they must not start again at the reset.
    notices.extend(crate::run::queue::stop_parked_runs_of_task(tx, st, task_id, correlation)?);
    cancel_children(tx, st, task_id, &mut notices)?;
    wake_parent(tx, task_id)?;
    notices.extend(crate::task::task_runs::origin_released(tx, task_id)?);
    Ok(notices)
}

impl CommandService {
    pub(crate) fn create_task(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &task::CreateTask) -> Result<Committed, AppError> {
        let valid = task::validate(cmd).map_err(domain_error)?;
        let run = cmd.run.validated().map_err(domain_error)?;
        let blobs_store = self.store.blobs();
        let goal_blob = blobs_store.put(valid.goal.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let criteria_blob = blobs_store.put_json(&valid.criteria).map_err(storage_error)?;
        let attachments = self.attach_files(cmd.project_id, cmd.employee_id, &cmd.files, None)?;
        let cmd = cmd.clone();
        let task_id = TaskId::new();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let user = ctx.user_id;
        let stored = attachments.clone();
        let committed = self.commit(command_id, hash, move |tx| {
            let Some((_, project_status)) = employees::project_scope(tx, cmd.project_id)? else {
                return Ok(Err(domain_error(DomainError::ProjectNotFound(cmd.project_id))));
            };
            if project_status == LifecycleStatus::Archived {
                return Ok(Err(domain_error(DomainError::ProjectArchived)));
            }
            // Any employee may work in any project; an archived one no longer works.
            if let Some(m) = cmd.employee_id {
                match employees::load_employee(tx, m)? {
                    Some(row) if row.status != LifecycleStatus::Archived => {}
                    Some(_) => return Ok(Err(domain_error(DomainError::EmployeeArchived))),
                    None => return Ok(Err(domain_error(DomainError::EmployeeNotFound(m)))),
                }
            }
            if let Some(id) = run.connection_id {
                if aurelius_storage::connections::load(tx, id)?.is_none() {
                    return Ok(Err(domain_error(DomainError::ProviderNotFound(id))));
                }
            }
            blobs::register(tx, &goal_blob)?;
            blobs::register(tx, &criteria_blob)?;
            let status = if cmd.employee_id.is_some() { TaskStatus::Queued } else { TaskStatus::Draft };
            store::insert_task(tx, task_id, cmd.project_id, cmd.employee_id, &valid.title, &goal_blob.id.to_string(), &criteria_blob.id.to_string(), status, None)?;
            store::set_task_run(tx, task_id, &run)?;
            // It runs on the Claude login of the board member who made it (B5).
            store::set_login_user(tx, task_id, user.map(|u| u.to_string()).as_deref())?;
            if stored.is_some() {
                store::set_attachments(tx, task_id, stored.as_ref())?;
            }
            let mut created = emit(tx, Ev {
                aggregate_type: "task",
                aggregate_id: task_id.to_string(),
                event_type: "task.created",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: Some(goal_blob.id.to_string()),
                metadata: serde_json::json!({ "title": valid.title, "employee_id": cmd.employee_id, "ctx": meta }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            created.1.project_id = Some(cmd.project_id);
            let mut notices = vec![created.1];
            if cmd.employee_id.is_some() {
                notices.push(task_event(tx, task_id, cmd.project_id, "task.queued", serde_json::json!({}))?);
                aurelius_storage::events::insert_job(tx, &task_id.to_string(), TASK_JOB_KIND, created.0, &format!("task:{task_id}"), &serde_json::json!({ "task_id": task_id }))?;
            }
            Ok(Ok((CommandResult::TaskCreated { task_id }, notices)))
        });
        // Copies of a command that failed or was only replayed are not kept.
        self.discard_unused(&attachments, committed.as_ref().map_or(true, |c| c.receipt.replayed));
        committed
    }

    /// Check and copy a task's (or schedule's) files into the work folder's
    /// git-ignored uploads area (`.aurelius/uploads/`), the same way messages do.
    pub(crate) fn attach_files(&self, project: aurelius_domain::ProjectId, employee: Option<EmployeeId>, files: &[String], into: Option<&std::path::Path>) -> Result<Option<store::TaskAttachments>, AppError> {
        if files.is_empty() {
            return Ok(None);
        }
        if files.len() > aurelius_domain::conversation::UPLOADS_MAX {
            return Err(domain_error(DomainError::File(format!("Too many files: attach at most {}.", aurelius_domain::conversation::UPLOADS_MAX))));
        }
        if files.iter().any(|p| p.trim().is_empty()) {
            return Err(domain_error(DomainError::Required { field: "files".into() }));
        }
        let sources = crate::conversation::message_uploads::check(files)?;
        let folder = match into {
            Some(folder) => folder.to_path_buf(),
            None => {
                let folders = self.store.read(|c| store::work_folders(c, project, employee)).map_err(storage_error)?;
                crate::conversation::message_uploads::work_folder(&folders)?
            }
        };
        let uploads = crate::conversation::message_uploads::copy(&folder, &sources)?;
        Ok(Some(store::TaskAttachments { folder: folder.display().to_string(), files: uploads }))
    }

    /// Remove freshly copied files again when the command that made them did not commit.
    pub(crate) fn discard_unused(&self, attachments: &Option<store::TaskAttachments>, unused: bool) {
        if let (Some(a), true) = (attachments, unused) {
            crate::conversation::message_uploads::remove(std::path::Path::new(&a.folder), &a.files);
        }
    }

    /// The main task of a ticket is the board's: its owner and its end go through
    /// the ticket (`ticket.assign`, `ticket.cancel`), so card and task stay one.
    pub(crate) fn cancel_task(&self, envelope: &CommandEnvelope, hash: String, cmd: &task::CancelTask) -> Result<Committed, AppError> {
        let task_id = cmd.task_id;
        let command_id = envelope.command_id;
        let st = self.store.clone();
        self.commit(command_id, hash, move |tx| {
            let Some(row) = store::load_task(tx, task_id)? else {
                return Ok(Err(domain_error(DomainError::TaskNotFound(task_id))));
            };
            if crate::task::ticket_line::is_root(tx, task_id)? {
                return Ok(Err(domain_error(DomainError::Ticket(USE_THE_BOARD.into()))));
            }
            if row.status.is_terminal() {
                return Ok(Ok((CommandResult::TaskCancelled { task_id }, vec![])));
            }
            let notices = cancel_in(tx, &st, task_id, &command_id.to_string(), serde_json::json!({}))?;
            Ok(Ok((CommandResult::TaskCancelled { task_id }, notices)))
        })
    }

    /// The user gives a task to an employee (or makes it a draft again). Only
    /// while nothing runs: a draft, a queued task or a waiting one.
    pub(crate) fn assign_task(&self, envelope: &CommandEnvelope, hash: String, cmd: &task::AssignTask) -> Result<Committed, AppError> {
        let cmd = cmd.clone();
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            let task_id = cmd.task_id;
            let Some(row) = store::load_task(tx, task_id)? else { return Ok(Err(domain_error(DomainError::TaskNotFound(task_id)))) };
            if crate::task::ticket_line::is_root(tx, task_id)? {
                return Ok(Err(domain_error(DomainError::Ticket(USE_THE_BOARD.into()))));
            }
            if !matches!(row.status, TaskStatus::Draft | TaskStatus::Queued | TaskStatus::Waiting) {
                return Ok(Err(domain_error(DomainError::Memory("A task can only be given to another employee while it is not running.".into()))));
            }
            if row.employee_id == cmd.employee_id {
                return Ok(Ok((CommandResult::TaskAssigned { task_id }, vec![])));
            }
            let name = match cmd.employee_id {
                Some(m) => match employees::load_employee(tx, m)? {
                    Some(r) if r.status != LifecycleStatus::Archived => Some(r.name),
                    Some(_) => return Ok(Err(domain_error(DomainError::EmployeeArchived))),
                    None => return Ok(Err(domain_error(DomainError::EmployeeNotFound(m)))),
                },
                None => None,
            };
            let owner = crate::task::task_line::set_employee(tx, task_id, cmd.employee_id)?;
            let mut created = emit(tx, Ev {
                aggregate_type: "task",
                aggregate_id: task_id.to_string(),
                event_type: "task.assigned",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({ "employee_id": cmd.employee_id, "subject": name, "from": row.employee_id }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            created.1.project_id = Some(row.project_id);
            let mut notices = vec![created.1];
            notices.extend(owner);
            match cmd.employee_id {
                Some(_) => {
                    if row.status != TaskStatus::Queued {
                        notices.extend(crate::task::task_line::set_status(tx, task_id, TaskStatus::Queued, None)?);
                        notices.push(task_event(tx, task_id, row.project_id, "task.queued", serde_json::json!({ "reason": "assigned" }))?);
                    }
                    let woken = tx.execute(
                        "UPDATE jobs SET state = 'ready', not_before_ms = ?2, lease_owner = NULL, lease_until_ms = NULL, last_error_code = NULL WHERE id = ?1 AND state IN ('done','ready','cancelled')",
                        rusqlite::params![task_id.to_string(), aurelius_storage::now_ms()],
                    )?;
                    if woken == 0 {
                        aurelius_storage::events::insert_job(tx, &task_id.to_string(), TASK_JOB_KIND, created.0, &format!("task:{task_id}:{command_id}"), &serde_json::json!({ "task_id": task_id }))?;
                    }
                }
                None => {
                    // Back to a draft: nothing runs until it has an employee again.
                    notices.extend(crate::task::task_line::set_status(tx, task_id, TaskStatus::Draft, None)?);
                    tx.execute("UPDATE jobs SET state = 'cancelled' WHERE id = ?1 AND state = 'ready'", [task_id.to_string()])?;
                }
            }
            Ok(Ok((CommandResult::TaskAssigned { task_id }, notices)))
        })
    }

    // ── Executor steps ──────────────────────────────────────────────────

    pub fn task_context(&self, task_id: TaskId) -> Result<Option<TaskContext>, AppError> {
        let store_ref = self.store.clone();
        self.store
            .read(|conn| {
                let Some(row) = store::load_task(conn, task_id)? else { return Ok(None) };
                let read = |key: &str| store_ref.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let goal = read(&row.goal_key).unwrap_or_default();
                // Attached files: tell the employee where they are.
                let goal = match store::attachments_of(conn, task_id)?.and_then(|a| a.prompt()) {
                    Some(files) => format!("{goal}

{files}"),
                    None => goal,
                };
                let criteria: Vec<String> = row.criteria_key.as_deref().and_then(read).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
                let (employee_name, employee_status, employee_system, policy) = match row.employee_id {
                    Some(m) => {
                        let (name, status, system, policy) = employee_identity(conn, m, &read)?.ok_or_else(|| StorageError::Rejected(format!("employee {m} not found")))?;
                        (name, Some(status), system, policy)
                    }
                    None => (String::new(), None, String::new(), TaskPolicy::default()),
                };
                let parent = match row.parent {
                    Some(p) => match store::load_task(conn, p)? {
                        Some(prow) => match prow.employee_id {
                            Some(pm) => employee_identity(conn, pm, &read)?.map(|(name, ..)| ParentInfo { task_id: p, title: prow.title.clone(), employee_name: name }),
                            None => None,
                        },
                        None => None,
                    },
                    None => None,
                };
                let children = store::children_of(conn, task_id)?
                    .into_iter()
                    .map(|c| ChildState {
                        id: c.id,
                        title: c.title,
                        employee_name: c.employee_name.unwrap_or_default(),
                        status: c.status,
                        report: c.result_key.as_deref().and_then(read),
                    })
                    .collect::<Vec<_>>();
                let handoff_list = store::handoffs(conn, task_id)?;
                let handed_over_by = handoff_list.last().filter(|h| h.to_id == row.employee_id).map(|h| (h.from.clone(), h.reason.clone()));
                Ok(Some(TaskContext {
                    task_id,
                    project_id: row.project_id,
                    title: row.title,
                    goal,
                    criteria,
                    status: row.status,
                    employee_id: row.employee_id,
                    employee_name,
                    employee_status,
                    employee_system,
                    policy,
                    parent,
                    children,
                    handed_over_by,
                }))
            })
            .map_err(storage_error)
    }

    fn fenced<R: Send + 'static>(&self, task: TaskId, epoch: i64, f: impl FnOnce(&Tx<'_>) -> Result<R, StorageError> + Send + 'static) -> Result<Option<R>, AppError> {
        self.store
            .write(move |tx| {
                if !queries::holds_lease(tx, &task.to_string(), epoch)? {
                    return Ok(None);
                }
                f(tx).map(Some)
            })
            .map_err(storage_error)
    }

    /// Move the task to a status (validated transition) with an event.
    pub fn task_transition(&self, ctx: &TaskContext, epoch: i64, to: TaskStatus, reason: Option<&'static str>, event_type: &'static str) -> Result<Vec<EventNotice>, AppError> {
        let (task, project) = (ctx.task_id, ctx.project_id);
        let (st, parent_task, colleague, title) = (self.store.clone(), ctx.parent.as_ref().map(|p| p.task_id), ctx.employee_name.clone(), ctx.title.clone());
        Ok(self
            .fenced(task, epoch, move |tx| {
                let Some(row) = store::load_task(tx, task)? else { return Ok(vec![]) };
                if row.status == to || !task::can_transition(row.status, to) {
                    return Ok(vec![]);
                }
                // The card of a main task moves with it (Queued → Failed or Waiting: Todo → Doing).
                let mut notices = crate::task::task_line::set_status(tx, task, to, reason)?.into_vec();
                notices.push(task_event(tx, task, project, event_type, serde_json::json!({ "reason": reason }))?);
                if to == TaskStatus::Failed {
                    let why = reason.map(|r| format!(" ({r})")).unwrap_or_default();
                    notices.extend(thread::post(tx, &st, task, Speaker::Update { kind: "status", from: "Aurelius" }, &format!("The task failed{why}."))?);
                    let speaker = Speaker::Update { kind: "status", from: &colleague };
                    match parent_task {
                        Some(p) => notices.extend(thread::post(tx, &st, p, speaker, &format!("“{title}” failed{why}."))?),
                        None => notices.extend(crate::task::task_runs::report_to_origin(tx, &st, task, speaker, &format!("“{title}” failed{why}."))?),
                    }
                }
                if to.is_terminal() {
                    wake_parent(tx, task)?;
                }
                Ok(notices)
            })?
            .unwrap_or_default())
    }

    /// Close the task's job after the executor finished (or parked) it.
    pub fn close_task_job(&self, task: TaskId, epoch: i64) -> Result<(), AppError> {
        self.store.write(move |tx| queries::complete_job(tx, &task.to_string(), epoch).map(|_| ())).map_err(storage_error)
    }
}
