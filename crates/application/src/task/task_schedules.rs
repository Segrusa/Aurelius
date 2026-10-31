//! Recurring tasks. A schedule keeps a task template and a
//! recurrence; the daemon calls `fire_due_task_schedules` on a tick, with the
//! clock as an argument, and each due schedule makes a normal task for its
//! employee. A schedule whose previous task is still open skips that
//! occurrence instead of piling up, and after downtime it makes one task, not
//! one per missed occurrence.

use crate::run::events::{Ev, emit};
use crate::task::tasks::TASK_JOB_KIND;
use crate::{CommandService, Committed, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, DomainError, ProjectId, TaskId, TaskScheduleId,
    employee::LifecycleStatus,
    schedule::{self, CreateTaskSchedule, DeleteTaskSchedule, RunTaskScheduleNow, SetTaskSchedulePaused, UpdateTaskSchedule},
    task::TaskStatus,
};
use aurelius_domain::{AppError, CommandEnvelope, CommandResult, EventNotice};
use aurelius_domain::views::TaskSchedulesSnapshot;
use aurelius_storage::{
    StorageError, Tx, blobs, employees,
    task_schedules::{self as store, ScheduleRow},
    tasks as tasks_store,
};

/// The one event clients follow: `metadata.change` says what happened
/// (`created`, `updated`, `deleted`, `paused`, `resumed`, `fired`, `skipped`).
pub const CHANGED: &str = "recurring_task.changed";

fn changed(tx: &Tx<'_>, id: TaskScheduleId, project: ProjectId, change: &str, actor: ActorKind, correlation: &str, extra: serde_json::Value) -> Result<EventNotice, StorageError> {
    let mut metadata = serde_json::json!({ "schedule_id": id, "change": change });
    if let (Some(m), Some(e)) = (metadata.as_object_mut(), extra.as_object()) {
        m.extend(e.clone());
    }
    let (_, mut n) = emit(tx, Ev {
        aggregate_type: "recurring_task",
        aggregate_id: id.to_string(),
        event_type: CHANGED,
        actor,
        actor_id: None,
        correlation: correlation.to_owned(),
        payload: None,
        metadata,
        dedupe: format!("recurring_task:{id}:{change}:{correlation}"),
    })?;
    n.project_id = Some(project);
    Ok(n)
}

/// The project and the employee can still take work (any active employee may
/// work in any project).
pub(crate) fn can_work(tx: &Tx<'_>, project: ProjectId, employee: aurelius_domain::EmployeeId) -> Result<Result<(), DomainError>, StorageError> {
    match employees::project_scope(tx, project)? {
        None => return Ok(Err(DomainError::ProjectNotFound(project))),
        Some((_, LifecycleStatus::Archived)) => return Ok(Err(DomainError::ProjectArchived)),
        Some(_) => {}
    }
    Ok(match employees::load_employee(tx, employee)? {
        Some(row) if row.status != LifecycleStatus::Archived => Ok(()),
        Some(_) => Err(DomainError::EmployeeArchived),
        None => Err(DomainError::EmployeeNotFound(employee)),
    })
}

/// Make a normal, queued task from the schedule's template.
fn make_task(tx: &Tx<'_>, row: &ScheduleRow, goal: &blobs::PreparedBlob, criteria: &blobs::PreparedBlob, actor: ActorKind, correlation: &str) -> Result<(TaskId, Vec<EventNotice>), StorageError> {
    blobs::register(tx, goal)?;
    blobs::register(tx, criteria)?;
    let task_id = TaskId::new();
    tasks_store::insert_task(tx, task_id, row.project_id, Some(row.employee_id), &row.title, &goal.id.to_string(), &criteria.id.to_string(), TaskStatus::Queued, None)?;
    tasks_store::set_task_run(tx, task_id, &row.run)?;
    tasks_store::set_attachments(tx, task_id, row.attachments.as_ref())?;
    tasks_store::set_schedule(tx, task_id, row.id)?;
    let (event_id, mut created) = emit(tx, Ev {
        aggregate_type: "task",
        aggregate_id: task_id.to_string(),
        event_type: "task.created",
        actor,
        actor_id: None,
        correlation: correlation.to_owned(),
        payload: Some(goal.id.to_string()),
        metadata: serde_json::json!({ "title": row.title, "employee_id": row.employee_id, "schedule_id": row.id }),
        dedupe: format!("schedule:{}:{correlation}:created", row.id),
    })?;
    created.project_id = Some(row.project_id);
    let queued = crate::task::tasks::task_event(tx, task_id, row.project_id, "task.queued", serde_json::json!({ "schedule_id": row.id }))?;
    aurelius_storage::events::insert_job(tx, &task_id.to_string(), TASK_JOB_KIND, event_id, &format!("task:{task_id}"), &serde_json::json!({ "task_id": task_id }))?;
    Ok((task_id, vec![created, queued]))
}

fn not_found(id: TaskScheduleId) -> AppError {
    domain_error(DomainError::TaskScheduleNotFound(id))
}

impl CommandService {
    pub(crate) fn create_task_schedule(&self, ctx: &crate::CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &CreateTaskSchedule) -> Result<Committed, AppError> {
        let login = ctx.login_user();
        let (title, goal, criteria, run) =
            schedule::validate_template(cmd.project_id, cmd.employee_id, &cmd.title, &cmd.goal, &cmd.acceptance_criteria, &cmd.run, cmd.files.len()).map_err(domain_error)?;
        let recurrence = cmd.recurrence.validated().map_err(domain_error)?;
        let attachments = self.attach_files(cmd.project_id, Some(cmd.employee_id), &cmd.files, None)?;
        let (now, offset) = (aurelius_storage::now_ms(), aurelius_host::utc_offset_minutes_at);
        let id = TaskScheduleId::new();
        let row = ScheduleRow {
            id,
            project_id: cmd.project_id,
            employee_id: cmd.employee_id,
            title,
            goal,
            criteria,
            attachments: attachments.clone(),
            run,
            next_run_ms: (!cmd.paused).then(|| recurrence.next_after(now, now, offset)),
            recurrence,
            paused: cmd.paused,
            anchor_ms: now,
            last_run_ms: None,
            last_task_id: None,
            last_skipped_ms: None,
            created_at_ms: now,
        };
        let command_id = envelope.command_id;
        let committed = self.commit(command_id, hash, move |tx| {
            if let Err(e) = can_work(tx, row.project_id, row.employee_id)? {
                return Ok(Err(domain_error(e)));
            }
            if let Some(c) = row.run.connection_id {
                if aurelius_storage::connections::load(tx, c)?.is_none() {
                    return Ok(Err(domain_error(DomainError::ProviderNotFound(c))));
                }
            }
            store::insert(tx, &row)?;
            // Its tasks run on the Claude login of who made it (B5).
            store::set_login_user(tx, id, login.as_deref())?;
            let n = changed(tx, id, row.project_id, "created", ActorKind::User, &command_id.to_string(), serde_json::json!({ "title": row.title }))?;
            Ok(Ok((CommandResult::TaskScheduleCreated { schedule_id: id }, vec![n])))
        });
        self.discard_unused(&attachments, committed.as_ref().map_or(true, |c| c.receipt.replayed));
        committed
    }

    pub(crate) fn update_task_schedule(&self, ctx: &crate::CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &UpdateTaskSchedule) -> Result<Committed, AppError> {
        let login = ctx.login_user();
        let id = cmd.schedule_id;
        let existing = self.store.read(|c| store::load(c, id)).map_err(storage_error)?.ok_or_else(|| not_found(id))?;
        let kept = existing.attachments.as_ref().map_or(0, |a| a.files.iter().filter(|f| !cmd.remove_files.contains(&f.path)).count());
        let (title, goal, criteria, run) =
            schedule::validate_template(existing.project_id, cmd.employee_id, &cmd.title, &cmd.goal, &cmd.acceptance_criteria, &cmd.run, kept + cmd.add_files.len()).map_err(domain_error)?;
        let recurrence = cmd.recurrence.validated().map_err(domain_error)?;
        // New files go next to the ones already attached, so a schedule keeps one folder.
        let folder = existing.attachments.as_ref().map(|a| std::path::PathBuf::from(&a.folder));
        let added = self.attach_files(existing.project_id, Some(cmd.employee_id), &cmd.add_files, folder.as_deref())?;
        let mut attachments = existing.attachments.clone();
        if let Some(a) = attachments.as_mut() {
            a.files.retain(|f| !cmd.remove_files.contains(&f.path));
        }
        if let Some(new) = added.clone() {
            match attachments.as_mut() {
                Some(a) => a.files.extend(new.files),
                None => attachments = Some(new),
            }
        }
        let (now, offset) = (aurelius_storage::now_ms(), aurelius_host::utc_offset_minutes_at);
        let row = ScheduleRow {
            employee_id: cmd.employee_id,
            title,
            goal,
            criteria,
            attachments,
            run,
            anchor_ms: now,
            next_run_ms: (!existing.paused).then(|| recurrence.next_after(now, now, offset)),
            recurrence,
            ..existing
        };
        let command_id = envelope.command_id;
        let committed = self.commit(command_id, hash, move |tx| {
            if store::load(tx, id)?.is_none() {
                return Ok(Err(not_found(id)));
            }
            if let Err(e) = can_work(tx, row.project_id, row.employee_id)? {
                return Ok(Err(domain_error(e)));
            }
            if let Some(c) = row.run.connection_id {
                if aurelius_storage::connections::load(tx, c)?.is_none() {
                    return Ok(Err(domain_error(DomainError::ProviderNotFound(c))));
                }
            }
            store::update(tx, &row)?;
            // Changed by a board member: from now on on their login (B5); an app keeps the one it had.
            if login.is_some() {
                store::set_login_user(tx, id, login.as_deref())?;
            }
            let n = changed(tx, id, row.project_id, "updated", ActorKind::User, &command_id.to_string(), serde_json::json!({ "title": row.title }))?;
            Ok(Ok((CommandResult::TaskScheduleUpdated { schedule_id: id }, vec![n])))
        });
        self.discard_unused(&added, committed.as_ref().map_or(true, |c| c.receipt.replayed));
        committed
    }

    /// Delete a schedule. Tasks it made stay (without the link); copies of
    /// its attachments stay in the work folder, as those tasks may use them.
    pub(crate) fn delete_task_schedule(&self, envelope: &CommandEnvelope, hash: String, cmd: &DeleteTaskSchedule) -> Result<Committed, AppError> {
        let id = cmd.schedule_id;
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            let Some(row) = store::load(tx, id)? else { return Ok(Err(not_found(id))) };
            store::delete(tx, id)?;
            let n = changed(tx, id, row.project_id, "deleted", ActorKind::User, &command_id.to_string(), serde_json::json!({ "title": row.title }))?;
            Ok(Ok((CommandResult::TaskScheduleDeleted { schedule_id: id }, vec![n])))
        })
    }

    pub(crate) fn set_task_schedule_paused(&self, ctx: &crate::CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &SetTaskSchedulePaused) -> Result<Committed, AppError> {
        let login = ctx.login_user();
        let (id, paused) = (cmd.schedule_id, cmd.paused);
        let (now, offset) = (aurelius_storage::now_ms(), aurelius_host::utc_offset_minutes_at);
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            let Some(row) = store::load(tx, id)? else { return Ok(Err(not_found(id))) };
            if row.paused == paused {
                return Ok(Ok((CommandResult::TaskScheduleUpdated { schedule_id: id }, vec![])));
            }
            // Resuming plans from now: what was due while paused is not made up.
            let next = (!paused).then(|| row.recurrence.next_after(row.anchor_ms, now, offset));
            store::set_paused(tx, id, paused, next)?;
            // Resumed by a board member: on their login from now on (B5, OV-07).
            if !paused && login.is_some() {
                store::set_login_user(tx, id, login.as_deref())?;
            }
            let change = if paused { "paused" } else { "resumed" };
            let n = changed(tx, id, row.project_id, change, ActorKind::User, &command_id.to_string(), serde_json::json!({ "title": row.title }))?;
            Ok(Ok((CommandResult::TaskScheduleUpdated { schedule_id: id }, vec![n])))
        })
    }

    pub(crate) fn run_task_schedule_now(&self, envelope: &CommandEnvelope, hash: String, cmd: &RunTaskScheduleNow) -> Result<Committed, AppError> {
        let id = cmd.schedule_id;
        let row = self.store.read(|c| store::load(c, id)).map_err(storage_error)?.ok_or_else(|| not_found(id))?;
        let goal = self.store.blobs().put(row.goal.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let criteria = self.store.blobs().put_json(&row.criteria).map_err(storage_error)?;
        let (now, command_id) = (aurelius_storage::now_ms(), envelope.command_id);
        self.commit(command_id, hash, move |tx| {
            let Some(row) = store::load(tx, id)? else { return Ok(Err(not_found(id))) };
            if let Err(e) = can_work(tx, row.project_id, row.employee_id)? {
                return Ok(Err(domain_error(e)));
            }
            let (task_id, mut notices) = make_task(tx, &row, &goal, &criteria, ActorKind::User, &command_id.to_string())?;
            // The schedule's own timetable is unchanged.
            store::mark_fired(tx, id, now, None, task_id)?;
            notices.push(changed(tx, id, row.project_id, "fired", ActorKind::User, &command_id.to_string(), serde_json::json!({ "task_id": task_id, "now": true }))?);
            Ok(Ok((CommandResult::TaskScheduleRan { schedule_id: id, task_id }, notices)))
        })
    }

    /// Recurring tasks of a project (all projects: `None`).
    pub fn task_schedules(&self, project: Option<ProjectId>) -> Result<TaskSchedulesSnapshot, AppError> {
        self.store.read(|c| Ok(TaskSchedulesSnapshot { schedules: store::views(c, project)? })).map_err(storage_error)
    }

    /// Make a task for every schedule that is due at `now_ms` (`offset_at`:
    /// the server's UTC offset in minutes at a moment). Called on a tick and at start, with any
    /// clock in tests. Nothing piles up: a schedule makes at most one task per
    /// call, however many occurrences were missed, and none while its
    /// previous task is still open.
    pub fn fire_due_task_schedules(&self, now_ms: i64, offset_at: fn(i64) -> i64) -> Result<Vec<EventNotice>, AppError> {
        let due = self.store.read(|c| store::due(c, now_ms)).map_err(storage_error)?;
        let mut notices = Vec::new();
        for id in due {
            match self.fire_one(id, now_ms, offset_at) {
                Ok(n) => notices.extend(n),
                Err(err) => tracing::warn!(error = %err.message, schedule = %id, "recurring task failed"),
            }
        }
        Ok(notices)
    }

    fn fire_one(&self, id: TaskScheduleId, now: i64, offset: fn(i64) -> i64) -> Result<Vec<EventNotice>, AppError> {
        let Some(row) = self.store.read(|c| store::load(c, id)).map_err(storage_error)? else { return Ok(vec![]) };
        let due = move |r: &ScheduleRow| !r.paused && r.next_run_ms.is_some_and(|n| n <= now);
        if !due(&row) {
            return Ok(vec![]);
        }
        // Only a schedule that will really make a task needs its text stored.
        let busy = self.store.read(|c| store::open_task(c, id)).map_err(storage_error)?.is_some();
        let prepared = if busy {
            None
        } else {
            Some((
                self.store.blobs().put(row.goal.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?,
                self.store.blobs().put_json(&row.criteria).map_err(storage_error)?,
            ))
        };
        let occurrence = row.next_run_ms.unwrap_or(now);
        self.store
            .write(move |tx| {
                let Some(row) = store::load(tx, id)? else { return Ok(vec![]) };
                if !due(&row) {
                    return Ok(vec![]);
                }
                let next = row.recurrence.next_after(row.anchor_ms, now, offset);
                let correlation = format!("{id}:{occurrence}");
                let skip = |tx: &Tx<'_>, reason: &str| -> Result<Vec<EventNotice>, StorageError> {
                    store::mark_skipped(tx, id, now, next)?;
                    Ok(vec![changed(tx, id, row.project_id, "skipped", ActorKind::System, &correlation, serde_json::json!({ "reason": reason }))?])
                };
                if store::open_task(tx, id)?.is_some() {
                    return skip(tx, "previous_open");
                }
                let (Some((goal, criteria)), Ok(())) = (&prepared, can_work(tx, row.project_id, row.employee_id)?) else {
                    return skip(tx, "employee_unavailable");
                };
                let (task_id, mut notices) = make_task(tx, &row, goal, criteria, ActorKind::System, &correlation)?;
                store::mark_fired(tx, id, now, Some(next), task_id)?;
                notices.push(changed(tx, id, row.project_id, "fired", ActorKind::System, &correlation, serde_json::json!({ "task_id": task_id }))?);
                Ok(notices)
            })
            .map_err(storage_error)
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    use aurelius_domain::{
        employee::CreateEmployee,
        project::CreateProject,
        schedule::{CreateTaskSchedule, DeleteTaskSchedule, Recurrence, RunTaskScheduleNow, SetTaskSchedulePaused, UpdateTaskSchedule},
        task::{CancelTask, RunSettings},
    };

    const HOUR: i64 = 3_600_000;

    fn ctx() -> CommandContext {
        CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app: None, user_id: None }
    }

    fn envelope(command: Command) -> CommandEnvelope {
        CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }
    }

    fn run(service: &CommandService, command: Command) -> (CommandResult, Vec<EventNotice>) {
        let done = service.execute(&ctx(), envelope(command)).unwrap();
        (done.receipt.result, done.notices)
    }

    struct Setup {
        _dir: tempfile::TempDir,
        service: CommandService,
        project: aurelius_domain::ProjectId,
        employee: aurelius_domain::EmployeeId,
        folder: std::path::PathBuf,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("work");
        std::fs::create_dir_all(&folder).unwrap();
        let service = CommandService::new(std::sync::Arc::new(Store::open(&dir.path().join("data")).unwrap()));
        let (CommandResult::ProjectCreated { project_id, .. }, _) =
            run(&service, Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: Some(folder.display().to_string()) }))
        else {
            panic!()
        };
        let (CommandResult::EmployeeCreated { employee_id, .. }, _) =
            run(&service, Command::CreateEmployee(CreateEmployee { name: "Alex".into(), role: "Writer".into(), title: None, reports_to: None, instructions: None, folder: None }))
        else {
            panic!()
        };
        Setup { _dir: dir, service, project: project_id, employee: employee_id, folder }
    }

    fn schedule(s: &Setup, recurrence: Recurrence) -> CreateTaskSchedule {
        CreateTaskSchedule {
            project_id: s.project,
            employee_id: s.employee,
            title: "Weekly report".into(),
            goal: "Summarise the week.\n\n- sales\n- support\n".into(),
            acceptance_criteria: vec!["Short".into()],
            files: vec![],
            run: RunSettings { model: Some("claude-haiku-4-5".into()), effort: Some("low".into()), ..RunSettings::default() },
            recurrence,
            paused: false,
        }
    }

    fn create(s: &Setup, cmd: CreateTaskSchedule) -> aurelius_domain::TaskScheduleId {
        match run(&s.service, Command::CreateTaskSchedule(cmd)) {
            (CommandResult::TaskScheduleCreated { schedule_id }, notices) => {
                assert_eq!(notices[0].kind, EventKind::RecurringTaskChanged);
                schedule_id
            }
            other => panic!("{other:?}"),
        }
    }

    fn view(s: &Setup, id: aurelius_domain::TaskScheduleId) -> aurelius_domain::views::TaskScheduleView {
        s.service.task_schedules(None).unwrap().schedules.into_iter().find(|v| v.id == id).unwrap()
    }

    /// The tasks made so far, oldest first.
    fn tasks(s: &Setup) -> Vec<aurelius_domain::views::TaskView> {
        let store = s.service.store().clone();
        let mut all = store.read(|c| aurelius_storage::tasks::list_tasks(c, None, &|key| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok()))).unwrap();
        all.sort_by_key(|t| t.id);
        all
    }

    #[test]
    fn a_schedule_makes_a_task_when_due_skips_while_it_is_open_and_catches_up_once() {
        let s = setup();
        let id = create(&s, schedule(&s, Recurrence::EveryHours { hours: 2 }));
        let created = view(&s, id);
        let start = created.created_at_ms;
        let next = created.next_run_ms.expect("planned");
        assert!((next - start - 2 * HOUR).abs() < 60_000, "two hours after it was made");
        assert!(tasks(&s).is_empty());

        // Not due yet.
        s.service.fire_due_task_schedules(next - 1, |_| 0).unwrap();
        assert!(tasks(&s).is_empty());

        // Due: one normal task with the template, the goal exactly as typed.
        let notices = s.service.fire_due_task_schedules(next + 1000, |_| 0).unwrap();
        assert!(notices.iter().any(|n| n.kind == EventKind::RecurringTaskChanged) && notices.iter().any(|n| n.kind == EventKind::TaskCreated));
        let made = tasks(&s);
        assert_eq!(made.len(), 1);
        assert_eq!(made[0].title, "Weekly report");
        assert_eq!(made[0].goal, "Summarise the week.\n\n- sales\n- support", "newlines are kept (only the ends are trimmed)");
        assert_eq!(made[0].criteria, vec!["Short".to_string()]);
        assert_eq!((made[0].run.model.as_deref(), made[0].run.effort.as_deref()), (Some("claude-haiku-4-5"), Some("low")));
        assert_eq!(made[0].status, aurelius_domain::task::TaskStatus::Queued);
        assert_eq!(made[0].schedule_id, Some(id));
        assert_eq!(made[0].employee_id, Some(s.employee));
        let v = view(&s, id);
        assert_eq!((v.last_run_ms, v.last_task_id, v.open_task_id), (Some(next + 1000), Some(made[0].id), Some(made[0].id)));
        assert_eq!(v.next_run_ms, Some(next + 2 * HOUR), "on the grid, not drifting");

        // The task is still open at the next occurrence: skipped, nothing piles up.
        let notices = s.service.fire_due_task_schedules(next + 2 * HOUR + 1000, |_| 0).unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].kind, EventKind::RecurringTaskChanged);
        assert_eq!(tasks(&s).len(), 1);
        let v = view(&s, id);
        assert_eq!(v.last_skipped_ms, Some(next + 2 * HOUR + 1000));
        assert_eq!(v.next_run_ms, Some(next + 4 * HOUR));

        // The task ends; the server was off for a day: one catch-up task, not twelve.
        run(&s.service, Command::CancelTask(CancelTask { task_id: made[0].id }));
        assert_eq!(view(&s, id).open_task_id, None);
        let late = next + 24 * HOUR + 5000;
        s.service.fire_due_task_schedules(late, |_| 0).unwrap();
        assert_eq!(tasks(&s).len(), 2);
        assert!(view(&s, id).next_run_ms.unwrap() > late);
        s.service.fire_due_task_schedules(late + 1000, |_| 0).unwrap();
        assert_eq!(tasks(&s).len(), 2, "nothing more until the next occurrence");
    }

    #[test]
    fn pausing_stops_it_and_resuming_does_not_make_up_what_was_missed() {
        let s = setup();
        let id = create(&s, schedule(&s, Recurrence::EveryHours { hours: 1 }));
        let next = view(&s, id).next_run_ms.unwrap();
        let (result, notices) = run(&s.service, Command::SetTaskSchedulePaused(SetTaskSchedulePaused { schedule_id: id, paused: true }));
        assert_eq!(result, CommandResult::TaskScheduleUpdated { schedule_id: id });
        assert_eq!(notices.len(), 1);
        let v = view(&s, id);
        assert!(v.paused && v.next_run_ms.is_none());
        s.service.fire_due_task_schedules(next + 10 * HOUR, |_| 0).unwrap();
        assert!(tasks(&s).is_empty(), "paused: nothing is made");
        // Pausing again changes nothing and says nothing.
        assert!(run(&s.service, Command::SetTaskSchedulePaused(SetTaskSchedulePaused { schedule_id: id, paused: true })).1.is_empty());

        run(&s.service, Command::SetTaskSchedulePaused(SetTaskSchedulePaused { schedule_id: id, paused: false }));
        let v = view(&s, id);
        assert!(!v.paused);
        let resumed = v.next_run_ms.unwrap();
        assert!(resumed > aurelius_storage::now_ms(), "planned from now");
        s.service.fire_due_task_schedules(resumed - 1, |_| 0).unwrap();
        assert!(tasks(&s).is_empty());
        s.service.fire_due_task_schedules(resumed, |_| 0).unwrap();
        assert_eq!(tasks(&s).len(), 1);
    }

    #[test]
    fn run_now_makes_a_task_even_while_one_is_open_and_keeps_the_timetable() {
        let s = setup();
        let id = create(&s, schedule(&s, Recurrence::Daily { at_minute: 9 * 60 }));
        let next = view(&s, id).next_run_ms.unwrap();
        for expected in 1..=2 {
            let (result, _) = run(&s.service, Command::RunTaskScheduleNow(RunTaskScheduleNow { schedule_id: id }));
            assert!(matches!(result, CommandResult::TaskScheduleRan { schedule_id, .. } if schedule_id == id));
            assert_eq!(tasks(&s).len(), expected);
        }
        let v = view(&s, id);
        assert_eq!(v.next_run_ms, Some(next));
        assert_eq!(v.last_task_id, Some(tasks(&s)[1].id));
        // Due now, but the two open ones make it skip.
        s.service.fire_due_task_schedules(next, |_| 0).unwrap();
        assert_eq!(tasks(&s).len(), 2);
    }

    #[test]
    fn update_and_delete() {
        let s = setup();
        let id = create(&s, schedule(&s, Recurrence::EveryHours { hours: 2 }));
        s.service.fire_due_task_schedules(view(&s, id).next_run_ms.unwrap(), |_| 0).unwrap();
        let update = UpdateTaskSchedule {
            schedule_id: id,
            employee_id: s.employee,
            title: "Daily digest".into(),
            goal: "One\nTwo".into(),
            acceptance_criteria: vec![],
            add_files: vec![],
            remove_files: vec![],
            run: RunSettings { mode: Some(aurelius_domain::autonomy::AgentMode::Plan), ..RunSettings::default() },
            recurrence: Recurrence::Weekly { weekdays: vec![4, 0], at_minute: 600 },
        };
        let (result, notices) = run(&s.service, Command::UpdateTaskSchedule(update.clone()));
        assert_eq!(result, CommandResult::TaskScheduleUpdated { schedule_id: id });
        assert_eq!(notices[0].kind, EventKind::RecurringTaskChanged);
        let v = view(&s, id);
        assert_eq!((v.title.as_str(), v.goal.as_str()), ("Daily digest", "One\nTwo"));
        assert_eq!(v.recurrence, Recurrence::Weekly { weekdays: vec![0, 4], at_minute: 600 });
        assert_eq!(v.run.mode, Some(aurelius_domain::autonomy::AgentMode::Plan));
        assert!(v.last_task_id.is_some(), "its history stays");

        // Bad input is refused and changes nothing.
        let bad = UpdateTaskSchedule { recurrence: Recurrence::Daily { at_minute: 2000 }, ..update };
        assert!(s.service.execute(&ctx(), envelope(Command::UpdateTaskSchedule(bad))).is_err());
        assert_eq!(view(&s, id).title, "Daily digest");

        let task = tasks(&s)[0].id;
        run(&s.service, Command::DeleteTaskSchedule(DeleteTaskSchedule { schedule_id: id }));
        assert!(s.service.task_schedules(None).unwrap().schedules.is_empty());
        assert_eq!(tasks(&s)[0].id, task, "tasks it made stay");
        assert_eq!(tasks(&s)[0].schedule_id, None);
        assert!(s.service.execute(&ctx(), envelope(Command::DeleteTaskSchedule(DeleteTaskSchedule { schedule_id: id }))).is_err());
    }

    #[test]
    fn files_are_copied_once_and_every_task_refers_to_them() {
        let s = setup();
        let source = s.folder.parent().unwrap().join("brief.txt");
        std::fs::write(&source, "hello").unwrap();
        let id = create(&s, CreateTaskSchedule { files: vec![source.display().to_string()], ..schedule(&s, Recurrence::EveryHours { hours: 1 }) });
        let v = view(&s, id);
        assert_eq!(v.attachments.len(), 1);
        assert_eq!(v.attachments[0].path, ".aurelius/uploads/brief.txt");
        assert_eq!(std::fs::read_to_string(s.folder.join(".aurelius/uploads/brief.txt")).unwrap(), "hello");
        assert_eq!(v.attachments[0].id, Some(format!("s:{id}:0")));
        run(&s.service, Command::RunTaskScheduleNow(RunTaskScheduleNow { schedule_id: id }));
        let task = &tasks(&s)[0];
        assert_eq!(task.attachments[0].id, Some(format!("t:{}:0", task.id)), "each owner names its own copy");
        let without_ids = |files: &[aurelius_domain::views::MessageFileView]| files.iter().map(|f| aurelius_domain::views::MessageFileView { id: None, ..f.clone() }).collect::<Vec<_>>();
        assert_eq!(without_ids(&task.attachments), without_ids(&v.attachments));
        // More than eight files, or a missing one, is refused.
        let many = CreateTaskSchedule { files: vec![source.display().to_string(); 9], ..schedule(&s, Recurrence::EveryHours { hours: 1 }) };
        assert!(s.service.execute(&ctx(), envelope(Command::CreateTaskSchedule(many))).is_err());
        let missing = CreateTaskSchedule { files: vec!["/no/such/file.bin".into()], ..schedule(&s, Recurrence::EveryHours { hours: 1 }) };
        assert!(s.service.execute(&ctx(), envelope(Command::CreateTaskSchedule(missing))).is_err());
        assert_eq!(s.service.task_schedules(None).unwrap().schedules.len(), 1);
    }

    #[test]
    fn an_archived_employee_makes_the_schedule_skip() {
        let s = setup();
        let id = create(&s, schedule(&s, Recurrence::EveryHours { hours: 1 }));
        let next = view(&s, id).next_run_ms.unwrap();
        run(&s.service, Command::SetEmployeeStatus(aurelius_domain::employee::SetEmployeeStatus { employee_id: s.employee, status: aurelius_domain::employee::LifecycleStatus::Archived }));
        s.service.fire_due_task_schedules(next, |_| 0).unwrap();
        assert!(tasks(&s).is_empty());
        assert_eq!(view(&s, id).last_skipped_ms, Some(next));
    }
}
