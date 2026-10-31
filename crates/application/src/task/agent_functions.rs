//! Agent functions (protocol 1.21): stored task templates that a user's own
//! apps call over the remote gateway with an API key. A call checks the key,
//! the parameters and the daily/weekly limits, and in the same transaction
//! makes a normal queued task (like a recurring task does) and records the
//! call, which is what the limits count. Accepted calls count even when their
//! run fails; refused calls are not recorded.

use crate::run::events::{Ev, emit};
use crate::task::task_schedules::can_work;
use crate::task::tasks::{TASK_JOB_KIND, task_event};
use crate::{CommandService, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, AgentFunctionId, ApiKeyId, DomainError, FunctionCallId, ProjectId, TaskId,
    agent_function::{
        self as func, CallAccepted, CallError, CallState, CallStatus, CreateAgentFunction, KEY_NAME_MAX, KEY_PREFIX, UpdateAgentFunction, WEEK_MS,
    },
    task::{RunSettings, TaskStatus},
};
use aurelius_domain::{ErrorCode, EventNotice, AppError};
use aurelius_domain::views::{AgentFunctionView, AgentFunctionsSnapshot, ApiKeyCreated, ApiKeyView, FunctionCallsSnapshot};
use aurelius_storage::{
    StorageError, blobs,
    agent_functions::{self as store, CallRow, FunctionRow},
    tasks as tasks_store,
};
use rusqlite::Connection;

/// What the database keeps of an API key's secret. The gateway computes the
/// same value for the key a caller presents (`auth::verifier`).
pub fn key_verifier(secret: &str) -> String {
    store::secret_verifier(secret)
}

fn new_secret() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS random source");
    format!("{KEY_PREFIX}{}", hex::encode(bytes))
}

fn refuse(field: &str, detail: &str) -> AppError {
    domain_error(DomainError::OutOfRange { field: field.into(), detail: detail.into() })
}

fn views(conn: &Connection, project: Option<ProjectId>, now: i64) -> Result<Vec<AgentFunctionView>, StorageError> {
    let usage = store::usage_of(conn, now)?;
    Ok(store::list_functions(conn, project)?
        .into_iter()
        .map(|(row, employee_name)| {
            let used = usage.iter().find(|u| u.function_id == row.id);
            view_of(row, employee_name, used.map_or(0, |u| u.used_day), used.map_or(0, |u| u.used_week))
        })
        .collect())
}

fn view_of(row: FunctionRow, employee_name: String, used_day: u32, used_week: u32) -> AgentFunctionView {
    let f = row.fields;
    AgentFunctionView {
        id: row.id,
        project_id: row.project_id,
        employee_id: f.employee_id,
        employee_name,
        name: f.name,
        slug: f.slug,
        description: f.description,
        instruction_template: f.instruction_template,
        params: f.params,
        mode: f.mode,
        limit_per_day: f.limit_per_day,
        limit_per_week: f.limit_per_week,
        enabled: row.enabled,
        used_day,
        used_week,
        created_at_ms: row.created_at_ms,
    }
}

fn function_view(conn: &Connection, id: AgentFunctionId) -> Result<Result<AgentFunctionView, AppError>, StorageError> {
    Ok(views(conn, None, aurelius_storage::now_ms())?
        .into_iter()
        .find(|v| v.id == id)
        .ok_or_else(|| domain_error(DomainError::AgentFunctionNotFound(id))))
}

impl CommandService {
    // ----- for the user (desktop and mobile) -----

    pub fn agent_functions(&self, project: Option<ProjectId>) -> Result<AgentFunctionsSnapshot, AppError> {
        self.store
            .read(|c| Ok(AgentFunctionsSnapshot { all_enabled: store::all_enabled(c)?, functions: views(c, project, aurelius_storage::now_ms())? }))
            .map_err(storage_error)
    }

    pub fn create_agent_function(&self, cmd: &CreateAgentFunction) -> Result<AgentFunctionView, AppError> {
        let fields = func::validate(&cmd.function).map_err(domain_error)?;
        let (project, enabled) = (cmd.project_id, cmd.enabled);
        let id = AgentFunctionId::new();
        self.store
            .write(move |tx| {
                if let Err(e) = can_work(tx, project, fields.employee_id)? {
                    return Ok(Err(domain_error(e)));
                }
                if store::slug_taken(tx, &fields.slug, None)? {
                    return Ok(Err(refuse("slug", "another function already uses it")));
                }
                store::insert_function(tx, id, project, &fields, enabled)?;
                function_view(tx, id)
            })
            .map_err(storage_error)?
    }

    pub fn update_agent_function(&self, cmd: &UpdateAgentFunction) -> Result<AgentFunctionView, AppError> {
        let fields = func::validate(&cmd.function).map_err(domain_error)?;
        let id = cmd.function_id;
        self.store
            .write(move |tx| {
                let Some(existing) = store::load_function(tx, id)? else {
                    return Ok(Err(domain_error(DomainError::AgentFunctionNotFound(id))));
                };
                if let Err(e) = can_work(tx, existing.project_id, fields.employee_id)? {
                    return Ok(Err(domain_error(e)));
                }
                if store::slug_taken(tx, &fields.slug, Some(id))? {
                    return Ok(Err(refuse("slug", "another function already uses it")));
                }
                store::update_function(tx, id, &fields)?;
                function_view(tx, id)
            })
            .map_err(storage_error)?
    }

    pub fn delete_agent_function(&self, id: AgentFunctionId) -> Result<(), AppError> {
        self.store
            .write(move |tx| {
                if store::load_function(tx, id)?.is_none() {
                    return Ok(Err(domain_error(DomainError::AgentFunctionNotFound(id))));
                }
                store::delete_function(tx, id)?;
                Ok(Ok(()))
            })
            .map_err(storage_error)?
    }

    pub fn set_agent_function_enabled(&self, id: AgentFunctionId, enabled: bool) -> Result<(), AppError> {
        self.store
            .write(move |tx| {
                if store::load_function(tx, id)?.is_none() {
                    return Ok(Err(domain_error(DomainError::AgentFunctionNotFound(id))));
                }
                store::set_enabled(tx, id, enabled)?;
                Ok(Ok(()))
            })
            .map_err(storage_error)?
    }

    /// The kill switch.
    pub fn set_all_agent_functions_enabled(&self, enabled: bool) -> Result<(), AppError> {
        self.store.write(move |tx| store::set_all_enabled(tx, enabled)).map_err(storage_error)
    }

    /// Recent calls (newest first) and the usage of every function.
    pub fn function_calls(&self, function: Option<AgentFunctionId>, limit: Option<u32>) -> Result<FunctionCallsSnapshot, AppError> {
        let limit = limit.unwrap_or(50).clamp(1, 500);
        self.store
            .read(|c| Ok(FunctionCallsSnapshot { calls: store::recent_calls(c, function, limit)?, usage: store::usage_of(c, aurelius_storage::now_ms())? }))
            .map_err(storage_error)
    }

    pub fn api_keys(&self) -> Result<Vec<ApiKeyView>, AppError> {
        self.store.read(store::list_keys).map_err(storage_error)
    }

    /// Makes a key for some functions. The secret is in the answer only; the
    /// database keeps its hash.
    pub fn create_api_key(&self, name: &str, functions: &[AgentFunctionId]) -> Result<ApiKeyCreated, AppError> {
        let name = key_text("name", name, KEY_NAME_MAX)?;
        let secret = new_secret();
        let (verifier, id, functions) = (key_verifier(&secret), ApiKeyId::new(), functions.to_vec());
        let key = self
            .store
            .write(move |tx| {
                for f in &functions {
                    if store::load_function(tx, *f)?.is_none() {
                        return Ok(Err(domain_error(DomainError::AgentFunctionNotFound(*f))));
                    }
                }
                store::insert_key(tx, id, &name, &verifier)?;
                store::set_key_functions(tx, id, &functions)?;
                Ok(Ok(store::list_keys(tx)?.into_iter().find(|k| k.id == id)))
            })
            .map_err(storage_error)??;
        let key = key.ok_or_else(|| AppError::new(ErrorCode::Internal, "The new key was not saved."))?;
        Ok(ApiKeyCreated { key, secret })
    }

    pub fn revoke_api_key(&self, id: ApiKeyId) -> Result<(), AppError> {
        self.store
            .write(move |tx| {
                if store::load_key(tx, id)?.is_none() {
                    return Ok(Err(domain_error(DomainError::ApiKeyNotFound(id))));
                }
                store::revoke_key(tx, id)?;
                Ok(Ok(()))
            })
            .map_err(storage_error)?
    }

    pub fn set_api_key_functions(&self, id: ApiKeyId, functions: &[AgentFunctionId]) -> Result<(), AppError> {
        let functions = functions.to_vec();
        self.store
            .write(move |tx| {
                if store::load_key(tx, id)?.is_none() {
                    return Ok(Err(domain_error(DomainError::ApiKeyNotFound(id))));
                }
                for f in &functions {
                    if store::load_function(tx, *f)?.is_none() {
                        return Ok(Err(domain_error(DomainError::AgentFunctionNotFound(*f))));
                    }
                }
                store::set_key_functions(tx, id, &functions)?;
                Ok(Ok(()))
            })
            .map_err(storage_error)?
    }

    // ----- for apps (through the gateway) -----

    /// A call from an app. `verifier` is the hash of the key it presented.
    /// Everything that decides (key, switch, function, parameters, limits)
    /// and everything that changes (task, job, call record) is one write
    /// transaction, so two calls cannot both take the last place.
    pub fn call_agent_function(&self, verifier: &str, slug: &str, given: &serde_json::Map<String, serde_json::Value>) -> Result<(CallAccepted, Vec<EventNotice>), CallError> {
        self.call_agent_function_at(aurelius_storage::now_ms(), verifier, slug, given)
    }

    pub(crate) fn call_agent_function_at(
        &self,
        now: i64,
        verifier: &str,
        slug: &str,
        given: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<(CallAccepted, Vec<EventNotice>), CallError> {
        let (verifier, slug, given) = (verifier.to_owned(), slug.to_owned(), given.clone());
        let blob_store = self.store.clone();
        let done = self.store.write(move |tx| {
            let Some(key) = store::key_by_verifier(tx, &verifier)?.filter(|k| !k.revoked()) else { return Ok(Err(CallError::Unauthorized)) };
            if !store::all_enabled(tx)? {
                return Ok(Err(CallError::SwitchedOff));
            }
            let Some(function) = store::function_by_slug(tx, &slug)? else { return Ok(Err(CallError::NotFound)) };
            if !store::key_allows(tx, key.id, function.id)? {
                return Ok(Err(CallError::Forbidden));
            }
            if !function.enabled {
                return Ok(Err(CallError::NotFound));
            }
            let f = &function.fields;
            let values = match func::validate_call_params(&f.params, &given) {
                Ok(v) => v,
                Err(message) => return Ok(Err(CallError::BadParams(message))),
            };
            if let Err(e) = can_work(tx, function.project_id, f.employee_id)? {
                return Ok(Err(CallError::Unavailable(e.to_string())));
            }
            let times = store::call_times(tx, function.id, now - WEEK_MS)?;
            let remaining = match func::check_quota(now, &times, f.limit_per_day, f.limit_per_week) {
                Ok(remaining) => remaining,
                Err(over) => return Ok(Err(CallError::OverLimit(over))),
            };

            let goal = func::render(&f.instruction_template, &values);
            let goal_blob = blob_store.blobs().put(goal.as_bytes(), "text/plain; charset=utf-8")?;
            let criteria_blob = blob_store.blobs().put_json(&Vec::<String>::new())?;
            blobs::register(tx, &goal_blob)?;
            blobs::register(tx, &criteria_blob)?;
            let (task_id, call_id) = (TaskId::new(), FunctionCallId::new());
            tasks_store::insert_task(tx, task_id, function.project_id, Some(f.employee_id), &f.name, &goal_blob.id.to_string(), &criteria_blob.id.to_string(), TaskStatus::Queued, None)?;
            tasks_store::set_task_run(tx, task_id, &RunSettings { mode: f.mode, ..RunSettings::default() })?;
            store::insert_call(tx, &CallRow { id: call_id, function_id: function.id, api_key_id: key.id, task_id, created_at_ms: now })?;
            store::touch_key(tx, key.id, now)?;
            let (event_id, mut created) = emit(
                tx,
                Ev {
                    aggregate_type: "task",
                    aggregate_id: task_id.to_string(),
                    event_type: "task.created",
                    actor: ActorKind::System,
                    actor_id: None,
                    correlation: call_id.to_string(),
                    payload: Some(goal_blob.id.to_string()),
                    metadata: serde_json::json!({ "title": f.name, "employee_id": f.employee_id, "origin": "function_call", "function_id": function.id, "call_id": call_id }),
                    dedupe: format!("function_call:{call_id}:created"),
                },
            )?;
            created.project_id = Some(function.project_id);
            let queued = task_event(tx, task_id, function.project_id, "task.queued", serde_json::json!({ "function_id": function.id }))?;
            aurelius_storage::events::insert_job(tx, &task_id.to_string(), TASK_JOB_KIND, event_id, &format!("task:{task_id}"), &serde_json::json!({ "task_id": task_id }))?;
            Ok(Ok((CallAccepted { call_id, remaining }, vec![created, queued])))
        });
        match done {
            Ok(result) => result,
            Err(err) => {
                tracing::error!(error = %err, "function call could not be saved");
                Err(CallError::Busy)
            }
        }
    }

    /// The state of a call, for the key that made it (any other key: not found).
    pub fn agent_function_call_state(&self, verifier: &str, call_id: &str) -> Result<CallState, CallError> {
        let busy = |err: StorageError| {
            tracing::error!(error = %err, "function call could not be read");
            CallError::Busy
        };
        let key = self.store.read(|c| store::key_by_verifier(c, verifier)).map_err(busy)?.filter(|k| !k.revoked()).ok_or(CallError::Unauthorized)?;
        let id = FunctionCallId::parse(call_id).map_err(|_| CallError::NotFound)?;
        let call = self.store.read(|c| store::load_call(c, id)).map_err(busy)?.filter(|c| c.api_key_id == key.id).ok_or(CallError::NotFound)?;
        let (status, updated, report_key) = self.store.read(|c| store::task_state(c, call.task_id)).map_err(busy)?.ok_or(CallError::NotFound)?;
        let status = CallStatus::from_task(status);
        let result = match report_key {
            Some(k) if status.is_done() => self.store.blobs().read(&k).ok().and_then(|b| String::from_utf8(b).ok()),
            _ => None,
        };
        Ok(CallState { status, result, created_at_ms: call.created_at_ms, finished_at_ms: status.is_done().then_some(updated) })
    }
}

fn key_text(field: &str, value: &str, max: usize) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(domain_error(DomainError::Required { field: field.into() }));
    }
    if trimmed.chars().count() > max {
        return Err(domain_error(DomainError::TooLong { field: field.into(), max }));
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use aurelius_domain::{agent_function::{DAY_MS, FunctionFields, FunctionParam}, employee::CreateEmployee, project::CreateProject};

    const H: i64 = 3_600_000;

    struct Setup {
        _dir: tempfile::TempDir,
        service: CommandService,
        project: ProjectId,
        employee: aurelius_domain::EmployeeId,
    }

    fn ctx() -> CommandContext {
        CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app: None, user_id: None }
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("work");
        std::fs::create_dir_all(&folder).unwrap();
        let service = CommandService::new(std::sync::Arc::new(Store::open(&dir.path().join("data")).unwrap()));
        let run = |c: Command| service.execute(&ctx(), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: c }).unwrap().receipt.result;
        let CommandResult::ProjectCreated { project_id, .. } = run(Command::CreateProject(CreateProject { name: "Demo".into(), description: None, folder: Some(folder.display().to_string()) })) else { panic!() };
        let CommandResult::EmployeeCreated { employee_id, .. } =
            run(Command::CreateEmployee(CreateEmployee { name: "Alex".into(), role: "Support".into(), title: None, reports_to: None, instructions: None, folder: None }))
        else {
            panic!()
        };
        Setup { _dir: dir, service, project: project_id, employee: employee_id }
    }

    fn fields(s: &Setup, slug: &str, day: Option<u32>, week: Option<u32>) -> FunctionFields {
        FunctionFields {
            employee_id: s.employee,
            name: format!("Tips {slug}"),
            slug: slug.into(),
            description: String::new(),
            instruction_template: "Give tips for {{course}}.".into(),
            params: vec![FunctionParam { name: "course".into(), required: true, max_length: 40 }],
            mode: Some(aurelius_domain::autonomy::AgentMode::Auto),
            limit_per_day: day,
            limit_per_week: week,
        }
    }

    fn function(s: &Setup, slug: &str, day: Option<u32>, week: Option<u32>) -> AgentFunctionId {
        s.service.create_agent_function(&CreateAgentFunction { project_id: s.project, function: fields(s, slug, day, week), enabled: true }).unwrap().id
    }

    fn key(s: &Setup, functions: &[AgentFunctionId]) -> (ApiKeyId, String) {
        let made = s.service.create_api_key("test app", functions).unwrap();
        (made.key.id, made.secret)
    }

    fn params(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        v.as_object().unwrap().clone()
    }

    fn call(s: &Setup, now: i64, secret: &str, slug: &str, p: serde_json::Value) -> Result<(CallAccepted, Vec<EventNotice>), CallError> {
        s.service.call_agent_function_at(now, &key_verifier(secret), slug, &params(p))
    }

    const NOW: i64 = 1_800_000_000_000;

    #[test]
    fn a_secret_is_shown_once_and_only_its_hash_is_kept() {
        let s = setup();
        let f = function(&s, "tips", None, None);
        let (id, secret) = key(&s, &[f]);
        assert!(secret.starts_with("ak_") && secret.len() > 40);
        let stored: String = s.service.store().read(|c| Ok(c.query_row("SELECT key_verifier FROM api_keys WHERE id = ?1", [id.to_string()], |r| r.get(0))?)).unwrap();
        assert_eq!(stored, key_verifier(&secret));
        assert_ne!(stored, secret);
        let listed = serde_json::to_string(&s.service.api_keys().unwrap()).unwrap();
        assert!(!listed.contains(&secret) && !listed.contains(&stored));
        assert_ne!(key(&s, &[f]).1, secret, "every key has its own secret");
    }

    #[test]
    fn a_call_makes_a_queued_task_with_the_rendered_goal_and_counts() {
        let s = setup();
        let f = function(&s, "tips", Some(2), Some(5));
        let (_, secret) = key(&s, &[f]);
        let (accepted, notices) = call(&s, NOW, &secret, "tips", serde_json::json!({ "course": "Lakes" })).unwrap();
        assert_eq!(accepted.remaining, func::Remaining { day: Some(1), week: Some(4) });
        assert_eq!(notices.len(), 2);
        let store = s.service.store().clone();
        let tasks = store.read(|c| aurelius_storage::tasks::list_tasks(c, None, &|k| store.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok()))).unwrap();
        assert_eq!(tasks.len(), 1);
        let task = &tasks[0];
        assert_eq!(task.status, TaskStatus::Queued);
        assert_eq!(task.employee_id, Some(s.employee));
        assert_eq!(task.function_id, Some(f));
        assert_eq!(task.run.mode, Some(aurelius_domain::autonomy::AgentMode::Auto));
        assert!(task.goal.starts_with("Give tips for \"Lakes\"."), "{}", task.goal);
        assert!(task.goal.contains("untrusted input"));
        let state = s.service.agent_function_call_state(&key_verifier(&secret), &accepted.call_id.to_string()).unwrap();
        assert_eq!((state.status, state.result, state.finished_at_ms), (CallStatus::Queued, None, None));
    }

    #[test]
    fn calls_over_the_daily_or_weekly_limit_are_refused_and_not_counted() {
        let s = setup();
        let f = function(&s, "tips", Some(2), Some(3));
        let (_, secret) = key(&s, &[f]);
        let p = || serde_json::json!({ "course": "A" });
        assert!(call(&s, NOW, &secret, "tips", p()).is_ok());
        assert!(call(&s, NOW + H, &secret, "tips", p()).is_ok());
        let Err(CallError::OverLimit(over)) = call(&s, NOW + 2 * H, &secret, "tips", p()) else { panic!("third call in a day") };
        assert_eq!(over.remaining, func::Remaining { day: Some(0), week: Some(1) });
        assert_eq!(over.retry_after_secs, 22 * 3600, "the first call leaves the day after 22 more hours");
        // Refused calls left no task and no record.
        let count: i64 = s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM function_calls", [], |r| r.get(0))?)).unwrap();
        assert_eq!(count, 2);
        // A day later the day window has room; the third call of the week is the last.
        assert!(call(&s, NOW + DAY_MS + 1, &secret, "tips", p()).is_ok());
        let Err(CallError::OverLimit(over)) = call(&s, NOW + DAY_MS + 2, &secret, "tips", p()) else { panic!("fourth call in the week") };
        assert_eq!(over.remaining.week, Some(0));
    }

    #[test]
    fn a_failed_run_still_counts() {
        let s = setup();
        let f = function(&s, "tips", Some(1), None);
        let (_, secret) = key(&s, &[f]);
        let (accepted, _) = call(&s, NOW, &secret, "tips", serde_json::json!({ "course": "A" })).unwrap();
        let task: String = s.service.store().read(|c| Ok(c.query_row("SELECT task_id FROM function_calls", [], |r| r.get(0))?)).unwrap();
        s.service.store().write(move |tx| Ok(tx.execute("UPDATE tasks SET status = 'failed' WHERE id = ?1", [task])?)).unwrap();
        let state = s.service.agent_function_call_state(&key_verifier(&secret), &accepted.call_id.to_string()).unwrap();
        assert_eq!(state.status, CallStatus::Failed);
        assert!(state.finished_at_ms.is_some());
        assert!(matches!(call(&s, NOW + 1, &secret, "tips", serde_json::json!({ "course": "A" })), Err(CallError::OverLimit(_))));
    }

    #[test]
    fn unknown_revoked_or_unallowed_keys_and_functions_are_refused() {
        let s = setup();
        let (a, b) = (function(&s, "tips", None, None), function(&s, "other", None, None));
        let (key_id, secret) = key(&s, &[a]);
        let p = || serde_json::json!({ "course": "A" });
        assert_eq!(call(&s, NOW, "ak_nonsense", "tips", p()).unwrap_err(), CallError::Unauthorized);
        assert_eq!(call(&s, NOW, &secret, "other", p()).unwrap_err(), CallError::Forbidden);
        assert_eq!(call(&s, NOW, &secret, "missing", p()).unwrap_err(), CallError::NotFound);
        assert_eq!(call(&s, NOW, &secret, "tips", serde_json::json!({})).unwrap_err(), CallError::BadParams("Parameter \"course\" is required.".into()));
        // A key can be given more functions.
        s.service.set_api_key_functions(key_id, &[a, b]).unwrap();
        assert!(call(&s, NOW, &secret, "other", p()).is_ok());
        // A disabled function is gone for callers; so is everything with the kill switch.
        s.service.set_agent_function_enabled(a, false).unwrap();
        assert_eq!(call(&s, NOW, &secret, "tips", p()).unwrap_err(), CallError::NotFound);
        s.service.set_agent_function_enabled(a, true).unwrap();
        assert!(call(&s, NOW, &secret, "tips", p()).is_ok());
        s.service.set_all_agent_functions_enabled(false).unwrap();
        assert!(!s.service.agent_functions(None).unwrap().all_enabled);
        assert_eq!(call(&s, NOW, &secret, "tips", p()).unwrap_err(), CallError::SwitchedOff);
        s.service.set_all_agent_functions_enabled(true).unwrap();
        // Revoked: nothing works any more, calls already made cannot be read either.
        let (accepted, _) = call(&s, NOW, &secret, "tips", p()).unwrap();
        s.service.revoke_api_key(key_id).unwrap();
        assert_eq!(call(&s, NOW, &secret, "tips", p()).unwrap_err(), CallError::Unauthorized);
        assert_eq!(s.service.agent_function_call_state(&key_verifier(&secret), &accepted.call_id.to_string()).unwrap_err(), CallError::Unauthorized);
        assert!(s.service.api_keys().unwrap()[0].revoked_at_ms.is_some());
    }

    #[test]
    fn only_the_key_that_made_a_call_can_read_it() {
        let s = setup();
        let f = function(&s, "tips", None, None);
        let (_, mine) = key(&s, &[f]);
        let (_, other) = key(&s, &[f]);
        let (accepted, _) = call(&s, NOW, &mine, "tips", serde_json::json!({ "course": "A" })).unwrap();
        let id = accepted.call_id.to_string();
        assert!(s.service.agent_function_call_state(&key_verifier(&mine), &id).is_ok());
        assert_eq!(s.service.agent_function_call_state(&key_verifier(&other), &id).unwrap_err(), CallError::NotFound);
        assert_eq!(s.service.agent_function_call_state(&key_verifier(&mine), "not-an-id").unwrap_err(), CallError::NotFound);
    }

    #[test]
    fn functions_are_validated_unique_and_listed_with_usage() {
        let s = setup();
        let f = function(&s, "tips", Some(3), None);
        let dup = s.service.create_agent_function(&CreateAgentFunction { project_id: s.project, function: fields(&s, "tips", None, None), enabled: true });
        assert!(dup.is_err());
        let mut bad = fields(&s, "bad", None, None);
        bad.instruction_template = "Needs {{nothing}}".into();
        assert!(s.service.create_agent_function(&CreateAgentFunction { project_id: s.project, function: bad, enabled: true }).is_err());
        let (_, secret) = key(&s, &[f]);
        call(&s, aurelius_storage::now_ms(), &secret, "tips", serde_json::json!({ "course": "A" })).unwrap();
        let snapshot = s.service.agent_functions(Some(s.project)).unwrap();
        assert!(snapshot.all_enabled);
        assert_eq!((snapshot.functions[0].used_day, snapshot.functions[0].used_week, snapshot.functions[0].employee_name.as_str()), (1, 1, "Alex"));
        let calls = s.service.function_calls(None, None).unwrap();
        assert_eq!((calls.calls.len(), calls.calls[0].status), (1, CallStatus::Queued));
        assert_eq!(calls.usage[0].limit_per_day, Some(3));
        // Update keeps the slug free for itself; delete forgets calls and key links.
        let updated = s.service.update_agent_function(&UpdateAgentFunction { function_id: f, function: fields(&s, "tips", Some(9), None) }).unwrap();
        assert_eq!(updated.limit_per_day, Some(9));
        s.service.delete_agent_function(f).unwrap();
        assert!(s.service.agent_functions(None).unwrap().functions.is_empty());
        assert!(s.service.api_keys().unwrap()[0].function_ids.is_empty());
        assert_eq!(s.service.function_calls(None, None).unwrap().calls.len(), 0);
    }
}
