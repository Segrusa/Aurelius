//! Task executor. A task is worked on in a conversation: the employee that
//! takes it on gets a conversation with the task as its context, works there
//! itself (with its tools, subagents where that helps) and ends with a
//! report. This job only starts that conversation's run once the task may
//! go (dependencies done, employee not paused); the run's end decides the
//! task's outcome (`aurelius_application::task::task_runs`). Resumable: each pass
//! derives what to do from durable state.

use crate::{Shared, runtimes};
use aurelius_application::run::connections::ResolvedRunConfig;
use aurelius_application::task::tasks::TaskContext;
use aurelius_domain::{RunId, TaskId, task::TaskStatus};
use aurelius_runtime::{RunRequest, RuntimeError, RuntimeEvent, ToolProfile, Turn, TurnRole};
use aurelius_storage::queries;
use std::sync::Arc;

const PARK_POLL_MS: i64 = 60_000;

pub(crate) enum StepError {
    Runtime(RuntimeError),
    Failed(String),
    /// The board member's Claude login is missing, expired or refused (a 401): wait for it.
    Login,
}

impl std::fmt::Display for StepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepError::Runtime(e) => write!(f, "{e}"),
            StepError::Failed(m) => f.write_str(m),
            StepError::Login => f.write_str("waiting for the board member's Claude login"),
        }
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()
}

/// Run one text-only step to completion and return (text, reported model)
/// (memory extraction and summaries).
pub(crate) async fn complete_text(shared: &Arc<Shared>, config: &ResolvedRunConfig, run_id: RunId, system: String, prompt: String) -> Result<(String, Option<String>), StepError> {
    let runtime = runtimes::runtime_for_config(shared, config).map_err(step_error)?;
    let request = RunRequest {
        run_id,
        model_id: config.model.clone().unwrap_or_default(),
        system: Some(system),
        turns: vec![Turn { role: TurnRole::User, text: prompt, images: Vec::new() }],
        effort: config.effort.clone(),
        profile: ToolProfile::TextOnly,
        subagent_model: None,
        session: Default::default(),
        session_id: None,
        keep_session: false,
    };
    let mut handle = runtime.start(request).await.map_err(step_error)?;
    let claude = config.provider == Some(aurelius_domain::connection::ProviderKind::ClaudeCode);
    if let Ok(mut map) = shared.cancellers.lock() {
        map.insert(run_id, handle.canceller());
    }
    let (mut text, mut model) = (String::new(), None);
    let result = loop {
        let Some(event) = handle.events.recv().await else {
            break Err(StepError::Failed("stream ended without a result".into()));
        };
        match event {
            RuntimeEvent::Usage(usage) => {
                let (s, run) = (shared.clone(), run_id);
                if let Some(Ok(notices)) = blocking(move || s.service.record_usage(run, &usage)).await {
                    shared.publish(notices);
                }
            }
            RuntimeEvent::ModelReported(m) => model = Some(m),
            RuntimeEvent::TextDelta(d) => text.push_str(&d),
            RuntimeEvent::Completed => break Ok(()),
            RuntimeEvent::Interrupted => break Err(StepError::Failed("onderbroken".into())),
            RuntimeEvent::Quota { .. } => break Err(StepError::Failed("rate_limit: the usage limit is reached".into())),
            RuntimeEvent::Failed { code, .. } if claude && code == aurelius_runtime::claude_code::parse::AUTH_FAILED => {
                // Claude refused the login (a 401): expired until reconnected.
                if let Some(user) = config.login_user {
                    let s = shared.clone();
                    if let Some(Ok(notices)) = blocking(move || s.service.claude_login_expired(user)).await {
                        shared.publish(notices);
                    }
                }
                break Err(StepError::Login);
            }
            RuntimeEvent::Failed { code, message } => break Err(StepError::Failed(format!("{code}: {message}"))),
            _ => {}
        }
    };
    if let Ok(mut map) = shared.cancellers.lock() {
        map.remove(&run_id);
    }
    result.map(|_| (text, model))
}

fn step_error(err: RuntimeError) -> StepError {
    match err {
        RuntimeError::Login(_) => StepError::Login,
        other => StepError::Runtime(other),
    }
}

pub async fn run_task(shared: Arc<Shared>, job: queries::JobRow) {
    let epoch = job.lease_epoch;
    let Ok(task_id) = TaskId::parse(&job.id) else { return };
    let s = shared.clone();
    let Some(Ok(Some(ctx))) = blocking(move || s.service.task_context(task_id)).await else {
        close(&shared, task_id, epoch).await;
        return;
    };
    if ctx.status.is_terminal() {
        close(&shared, task_id, epoch).await;
        return;
    }
    if ctx.status == TaskStatus::Draft || (ctx.status == TaskStatus::Waiting && waiting_for_user(&shared, task_id)) {
        close(&shared, task_id, epoch).await;
        return;
    }
    if ctx.employee_status == Some(aurelius_domain::employee::LifecycleStatus::Paused) {
        transition(&shared, &ctx, epoch, TaskStatus::Waiting, Some("paused"), "task.waiting").await;
        park(&shared, task_id, epoch, aurelius_storage::now_ms() + PARK_POLL_MS, "paused").await;
        return;
    }
    // The employee takes it on: its conversation does the work from here.
    let (s, c) = (shared.clone(), ctx.clone());
    match blocking(move || s.service.start_task_conversation(&c, epoch)).await {
        Some(Ok(n)) => shared.publish(n),
        Some(Err(e)) => {
            tracing::warn!(task = %task_id, error = %e.message, "could not start the task's conversation");
            transition(&shared, &ctx, epoch, TaskStatus::Failed, None, "task.failed").await;
        }
        None => {}
    }
    close(&shared, task_id, epoch).await;
}

async fn transition(shared: &Arc<Shared>, ctx: &TaskContext, epoch: i64, to: TaskStatus, reason: Option<&'static str>, event: &'static str) {
    let (s, c) = (shared.clone(), ctx.clone());
    if let Some(Ok(n)) = blocking(move || s.service.task_transition(&c, epoch, to, reason, event)).await {
        shared.publish(n);
    }
}

async fn close(shared: &Arc<Shared>, task: TaskId, epoch: i64) {
    shared.live.job_ended(&task.to_string());
    let s = shared.clone();
    let _ = blocking(move || s.service.close_task_job(task, epoch)).await;
}

async fn park(shared: &Arc<Shared>, task: TaskId, epoch: i64, until: i64, reason: &str) {
    let store = shared.store().clone();
    let reason = reason.to_owned();
    let _ = blocking(move || store.write(move |tx| queries::reschedule_job(tx, &task.to_string(), epoch, until, &reason, false))).await;
}

/// Waiting for the user (a decision, or they stopped it): never resumed by this job.
fn waiting_for_user(shared: &Arc<Shared>, task: TaskId) -> bool {
    shared
        .store()
        .read(|c| Ok(c.query_row("SELECT COALESCE(waiting_reason = 'input', 0) FROM tasks WHERE id = ?1", [task.to_string()], |r| r.get::<_, bool>(0))?))
        .unwrap_or(false)
}
