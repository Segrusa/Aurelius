//! App keys (feature `app_keys`): what an outside app may do through
//! `aurelius-mcp`. The user makes, lists and revokes them; a session proves
//! one with its secret and from then on acts for that app. Each request of such
//! a session looks the key up again (`app_key_for_request`), so revoking works
//! at once, also for sessions that are open. Limits and the Auto rule are
//! checked where tasks are made (`tasks::app_may_create`).

use crate::{AppActor, CommandService, domain_error, storage_error};
use aurelius_domain::app_key::{self, AppKeyCreated, AppKeyView, AppSession, CreateAppKey, KEY_PREFIX};
use aurelius_domain::{AppError, AppKeyId, DomainError, ErrorCode};
use aurelius_storage::app_keys::{self as store, AppKeyRow};

/// How often `last_used` is written for a busy key.
const TOUCH_EVERY_MS: i64 = 60_000;

fn new_secret() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS random source");
    format!("{KEY_PREFIX}{}", hex::encode(bytes))
}

/// What the database keeps of an app key's secret (the same hash as API keys).
pub fn key_verifier(secret: &str) -> String {
    aurelius_storage::agent_functions::secret_verifier(secret)
}

fn revoked() -> AppError {
    AppError::new(ErrorCode::Forbidden, "This app key was revoked or does not exist. Ask the user for a new one.")
}

/// Who a session authenticated with this key speaks for (in its hello).
pub fn session_of(key: &AppKeyRow) -> AppSession {
    AppSession { key_id: key.id, name: key.name.clone(), scopes: key.scopes.clone(), audience: key.audience }
}

/// The app as the author of the commands of its session.
pub fn actor_of(key: &AppKeyRow) -> AppActor {
    AppActor { key_id: key.id, name: key.name.clone() }
}

impl CommandService {
    pub fn app_keys(&self) -> Result<Vec<AppKeyView>, AppError> {
        self.store.read(store::list).map_err(storage_error)
    }

    /// Makes a key; the secret is in the answer only, the database keeps its hash.
    pub fn create_app_key(&self, cmd: &CreateAppKey) -> Result<AppKeyCreated, AppError> {
        let key = app_key::validate(cmd).map_err(domain_error)?;
        let secret = new_secret();
        let (verifier, id) = (key_verifier(&secret), AppKeyId::new());
        // A webhook is signed with its own secret, shown once like the key's and kept to sign with.
        let webhook_secret = key.webhook_url.as_ref().map(|_| {
            let mut bytes = [0u8; 32];
            getrandom::fill(&mut bytes).expect("OS random source");
            aurelius_domain::webhook::new_secret(bytes)
        });
        let kept_secret = webhook_secret.clone();
        let view = self
            .store
            .write(move |tx| {
                store::insert(tx, id, &key, &verifier, kept_secret.as_deref())?;
                let row = store::load(tx, id)?.ok_or(aurelius_storage::StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows))?;
                store::view(tx, row, aurelius_storage::now_ms())
            })
            .map_err(storage_error)?;
        Ok(AppKeyCreated { key: view, secret, webhook_secret })
    }

    /// Revoked at once: the next request of any session with this key is refused.
    pub fn revoke_app_key(&self, id: AppKeyId) -> Result<AppKeyView, AppError> {
        self.store
            .write(move |tx| {
                let Some(_) = store::load(tx, id)? else { return Ok(Err(domain_error(DomainError::AppKeyNotFound(id)))) };
                store::revoke(tx, id)?;
                let row = store::load(tx, id)?.ok_or(aurelius_storage::StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows))?;
                Ok(Ok(store::view(tx, row, aurelius_storage::now_ms())?))
            })
            .map_err(storage_error)?
    }

    /// The working key behind a secret a session presents (or the gateway got).
    pub fn authenticate_app(&self, secret: &str) -> Result<AppKeyRow, AppError> {
        if !secret.starts_with(KEY_PREFIX) {
            return Err(revoked());
        }
        self.authenticate_app_verifier(&key_verifier(secret))
    }

    /// As `authenticate_app`, from the hash of the secret.
    pub fn authenticate_app_verifier(&self, verifier: &str) -> Result<AppKeyRow, AppError> {
        let key = self.store.read(|c| store::by_verifier(c, verifier)).map_err(storage_error)?.filter(|k| !k.revoked()).ok_or_else(revoked)?;
        self.touch_app_key(&key);
        Ok(key)
    }

    /// The key of an app session as it is now, for one request: refused once revoked.
    pub fn app_key_for_request(&self, id: AppKeyId) -> Result<AppKeyRow, AppError> {
        let key = self.store.read(|c| store::load(c, id)).map_err(storage_error)?.filter(|k| !k.revoked()).ok_or_else(revoked)?;
        self.touch_app_key(&key);
        Ok(key)
    }

    /// `last_used` at most once a minute (best effort; a busy writer skips it).
    fn touch_app_key(&self, key: &AppKeyRow) {
        let now = aurelius_storage::now_ms();
        if key.last_used_ms.is_some_and(|t| now - t < TOUCH_EVERY_MS) {
            return;
        }
        let id = key.id;
        if let Err(err) = self.store.write(move |tx| store::touch(tx, id, now)) {
            tracing::debug!(error = %err, "app key last-used not saved");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{AppActor, CommandContext, CommandService, Origin};
    use aurelius_domain::{
        ClientId, Command, CommandEnvelope, CommandId, CommandResult, ErrorCode, PrincipalId,
        app_key::{AppScope, CreateAppKey},
        autonomy::{AgentMode, SetAgentMode},
        employee::CreateEmployee,
        project::CreateProject,
        task::{CreateTask, RunSettings},
    };
    use aurelius_storage::Store;
    use std::sync::Arc;

    struct Setup {
        _dir: tempfile::TempDir,
        service: CommandService,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("work")).unwrap();
        let store = Arc::new(Store::open(&dir.path().join("data")).unwrap());
        Setup { service: CommandService::new(store), _dir: dir }
    }

    fn ctx(app: Option<AppActor>) -> CommandContext {
        CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app, user_id: None }
    }

    fn run(s: &Setup, app: Option<AppActor>, command: Command) -> Result<CommandResult, aurelius_domain::AppError> {
        s.service.execute(&ctx(app), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }).map(|c| c.receipt.result)
    }

    fn project_and_employee(s: &Setup) -> (aurelius_domain::ProjectId, aurelius_domain::EmployeeId, aurelius_domain::ConversationId) {
        let folder = Some(s._dir.path().join("work").display().to_string());
        let CommandResult::ProjectCreated { project_id, .. } = run(s, None, Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder })).unwrap() else { panic!() };
        let employee = CreateEmployee { name: "Simon".into(), role: "Server".into(), instructions: None, folder: None, title: None, reports_to: None };
        let CommandResult::EmployeeCreated { employee_id, conversation_id } = run(s, None, Command::CreateEmployee(employee)).unwrap() else { panic!() };
        (project_id, employee_id, conversation_id)
    }

    fn task(project_id: aurelius_domain::ProjectId, employee_id: Option<aurelius_domain::EmployeeId>, mode: Option<AgentMode>) -> Command {
        Command::CreateTask(CreateTask {
            project_id,
            employee_id,
            title: "From the app".into(),
            goal: "Do it".into(),
            acceptance_criteria: vec![],
            depends_on: vec![],
            run: RunSettings { mode, ..Default::default() },
            files: vec![],
        })
    }

    fn key(s: &Setup, scopes: Vec<AppScope>, per_hour: Option<u32>, at_once: Option<u32>) -> (AppActor, String) {
        let created = s.service.create_app_key(&CreateAppKey { name: "Assistant".into(), scopes, limit_per_hour: per_hour, limit_concurrent: at_once, audience: Default::default(), webhook_url: None }).unwrap();
        (AppActor { key_id: created.key.id, name: created.key.name.clone() }, created.secret)
    }

    fn task_view(s: &Setup, id: aurelius_domain::TaskId) -> aurelius_domain::views::TaskView {
        s.service.store().read(|c| aurelius_storage::tasks::list_tasks(c, None, &|_| None)).unwrap().into_iter().find(|t| t.id == id).unwrap()
    }

    #[test]
    fn the_secret_is_shown_once_and_only_its_hash_is_kept() {
        let s = setup();
        let (actor, secret) = key(&s, vec![AppScope::Read], None, None);
        assert!(secret.starts_with("amk_") && secret.len() > 60);
        let stored: String = s.service.store().read(|c| Ok(c.query_row("SELECT key_verifier FROM app_keys", [], |r| r.get(0))?)).unwrap();
        assert_ne!(stored, secret);
        assert_eq!(s.service.authenticate_app(&secret).unwrap().id, actor.key_id);
        assert!(s.service.authenticate_app("amk_wrong").is_err());
        assert!(s.service.authenticate_app(&secret.replace("amk_", "ak_")).is_err(), "only app keys");
        let listed = s.service.app_keys().unwrap();
        assert_eq!((listed.len(), listed[0].scopes.clone()), (1, vec![AppScope::Read]));
        assert!(listed[0].last_used_ms.is_some(), "authenticating counts as use");
    }

    #[test]
    fn revoking_works_at_once() {
        let s = setup();
        let (project_id, _, _) = project_and_employee(&s);
        let (actor, secret) = key(&s, vec![AppScope::Draft], None, None);
        assert!(s.service.app_key_for_request(actor.key_id).is_ok());
        let view = s.service.revoke_app_key(actor.key_id).unwrap();
        assert!(view.revoked_at_ms.is_some());
        assert_eq!(s.service.app_key_for_request(actor.key_id).unwrap_err().code, ErrorCode::Forbidden);
        assert!(s.service.authenticate_app(&secret).is_err());
        // A command that passed the session check just before is refused where it writes.
        assert_eq!(run(&s, Some(actor), task(project_id, None, None)).unwrap_err().code, ErrorCode::Forbidden);
        assert_eq!(s.service.revoke_app_key(aurelius_domain::AppKeyId::new()).unwrap_err().code, ErrorCode::NotFound);
    }

    #[test]
    fn tasks_keep_the_app_that_made_them() {
        let s = setup();
        let (project_id, employee_id, _) = project_and_employee(&s);
        let (actor, _) = key(&s, vec![AppScope::Run], None, None);
        let CommandResult::TaskCreated { task_id } = run(&s, Some(actor.clone()), task(project_id, Some(employee_id), None)).unwrap() else { panic!() };
        let view = task_view(&s, task_id);
        assert_eq!(view.created_by_app.as_deref(), Some("Assistant"));
        assert_eq!(view.created_by_app_key_id, Some(actor.key_id));
        let CommandResult::TaskCreated { task_id: own } = run(&s, None, task(project_id, None, None)).unwrap() else { panic!() };
        assert_eq!(task_view(&s, own).created_by_app, None, "the user's own tasks name no app");
        let (hour, open) = { let k = &s.service.app_keys().unwrap()[0]; (k.tasks_last_hour, k.tasks_open) };
        assert_eq!((hour, open), (1, 1));
    }

    #[test]
    fn limits_per_hour_and_at_once_are_enforced() {
        let s = setup();
        let (project_id, employee_id, _) = project_and_employee(&s);
        let (actor, _) = key(&s, vec![AppScope::Run], Some(3), Some(1));
        run(&s, Some(actor.clone()), task(project_id, Some(employee_id), None)).unwrap();
        let err = run(&s, Some(actor.clone()), task(project_id, Some(employee_id), None)).unwrap_err();
        assert!(err.message.contains("at once"), "{err:?}");
        // A draft is not open work: it only counts per hour.
        run(&s, Some(actor.clone()), task(project_id, None, None)).unwrap();
        run(&s, Some(actor.clone()), task(project_id, None, None)).unwrap();
        let err = run(&s, Some(actor), task(project_id, None, None)).unwrap_err();
        assert!(err.message.contains("per hour"), "{err:?}");
        // The user's own tasks never count against an app.
        run(&s, None, task(project_id, Some(employee_id), None)).unwrap();
    }

    #[test]
    fn auto_needs_approve_and_the_default_is_the_employees_mode() {
        let s = setup();
        let (project_id, employee_id, main) = project_and_employee(&s);
        let (runner, _) = key(&s, vec![AppScope::Run], None, None);
        let (approver, _) = key(&s, vec![AppScope::Run, AppScope::Approve], None, None);
        assert_eq!(run(&s, Some(runner.clone()), task(project_id, Some(employee_id), Some(AgentMode::Auto))).unwrap_err().code, ErrorCode::Forbidden);
        let CommandResult::TaskCreated { task_id } = run(&s, Some(runner.clone()), task(project_id, Some(employee_id), None)).unwrap() else { panic!() };
        assert_eq!(task_view(&s, task_id).run.mode, None, "the employee's own mode (Ask) is kept as is");
        run(&s, None, Command::SetAgentMode(SetAgentMode { conversation_id: main, mode: AgentMode::Auto })).unwrap();
        let CommandResult::TaskCreated { task_id } = run(&s, Some(runner), task(project_id, Some(employee_id), None)).unwrap() else { panic!() };
        assert_eq!(task_view(&s, task_id).run.mode, Some(AgentMode::Ask), "no Auto from the employee without approve");
        let CommandResult::TaskCreated { task_id } = run(&s, Some(approver.clone()), task(project_id, Some(employee_id), None)).unwrap() else { panic!() };
        assert_eq!(task_view(&s, task_id).run.mode, None, "with approve the employee's Auto applies");
        let CommandResult::TaskCreated { task_id } = run(&s, Some(approver), task(project_id, Some(employee_id), Some(AgentMode::Auto))).unwrap() else { panic!() };
        assert_eq!(task_view(&s, task_id).run.mode, Some(AgentMode::Auto));
    }
}
