//! The dispatcher's job (`ticket.triage`, see `aurelius_application::task::triage`):
//! one short run of the ticket's dispatcher with only the bridge's
//! `assign_ticket` and `recall`, at most `MAX_TURNS` turns. A run that chose
//! nobody gets one more; then the ticket waits for a board member. So does a
//! dispatcher that cannot work (`dispatcher_failed`), also after one more try.
//!
//! The job keeps its lease while the dispatcher chooses, as a chat run does
//! (`dispatcher::chat`); lost, it stops without writing anything more.

use crate::{Shared, runtimes};
use aurelius_application::task::triage::{ATTEMPTS, MAX_TURNS, TOOLS};
use aurelius_domain::{RunId, TaskId, TicketId};
use aurelius_runtime::{RunRequest, RuntimeEvent, ToolProfile, Turn, TurnRole};
use aurelius_storage::queries;
use std::sync::Arc;
use std::time::Duration;

/// The job's lease and how often it is renewed (those of the dispatcher's jobs, `dispatcher::LEASE_MS`).
const LEASE_MS: i64 = 45_000;
const RENEW_EVERY: Duration = Duration::from_secs(10);
/// The pause before preparing the dispatcher's run again, so a passing cause
/// (a busy database) has gone by at the second try.
const PREPARE_AGAIN_AFTER: Duration = Duration::from_millis(1_500);

pub(crate) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()
}

pub async fn run_triage(shared: Arc<Shared>, job: queries::JobRow) {
    let epoch = job.lease_epoch;
    let ids = (job.payload["ticket_id"].as_str().and_then(|t| TicketId::parse(t).ok()), job.payload["task_id"].as_str().and_then(|t| TaskId::parse(t).ok()));
    let (Some(ticket), Some(root)) = ids else {
        close(&shared, &job.id, epoch).await;
        return;
    };
    // A run of an earlier claim that never ended (the daemon stopped) is over.
    let s = shared.clone();
    if let Some(Err(e)) = blocking(move || s.service.triage_claimed(root)).await {
        tracing::warn!(%ticket, error = %e.message, "could not end the dispatcher's earlier run");
    }
    let mut not_prepared = 0;
    loop {
        if !renew(&shared, &job.id, epoch).await {
            return;
        }
        let s = shared.clone();
        let (run, notices) = match blocking(move || s.service.triage_run(ticket, root)).await {
            Some(Ok(prepared)) => prepared,
            Some(Err(e)) => {
                // No run row was made: it counts here, one more try and then the board decides.
                tracing::warn!(%ticket, error = %e.message, "the dispatcher's run could not be prepared");
                not_prepared += 1;
                if not_prepared < ATTEMPTS {
                    tokio::time::sleep(PREPARE_AGAIN_AFTER).await;
                    continue;
                }
                gave_up(&shared, ticket, root).await;
                break;
            }
            None => return,
        };
        shared.publish(notices);
        let Some(run) = run else { break };
        let (run_id, login) = (run.run_id, run.config.login_user);
        let one = OneRun { run_id, config: run.config, system: run.system, prompt: run.prompt, flag: "--triage", tools: &TOOLS };
        let Some(ended) = drive(&shared, one, &job.id, epoch).await else { return };
        // The ticket's Claude login is missing or expired: the job waits for it (no try used).
        let ended = match ended {
            Ended::Login => {
                let (s, id) = (shared.clone(), job.id.clone());
                if let Some(Ok(notices)) = blocking(move || s.service.triage_waits_for_login(ticket, run_id, id, epoch)).await {
                    shared.publish(notices);
                }
                shared.live.job_ended(&job.id);
                return;
            }
            // The ticket's login hit its limit: the job waits for the reset (no try used), as its runs do.
            Ended::Quota(reset_at_ms) => {
                let at = aurelius_domain::scheduler::quota_wake_ms(reset_at_ms, aurelius_storage::now_ms());
                let (s, id) = (shared.clone(), job.id.clone());
                if let Some(Ok(notices)) = blocking(move || s.service.triage_waits_for_quota(ticket, run_id, id, epoch, at)).await {
                    shared.publish(notices);
                }
                if let Some(login) = login {
                    let s = shared.clone();
                    if let Some(Ok(notices)) = blocking(move || s.service.park_runs_for_quota_of(login, at)).await {
                        shared.publish(notices);
                    }
                }
                shared.live.job_ended(&job.id);
                return;
            }
            Ended::Done(result) => result,
        };
        let s = shared.clone();
        match blocking(move || s.service.triage_ended(ticket, root, run_id, ended.err())).await {
            // It chose nobody: one more run, told to choose.
            Some(Ok((true, notices))) => shared.publish(notices),
            Some(Ok((false, notices))) => {
                shared.publish(notices);
                break;
            }
            Some(Err(e)) => {
                tracing::warn!(%ticket, error = %e.message, "the dispatcher's run could not be closed");
                gave_up(&shared, ticket, root).await;
                break;
            }
            None => break,
        }
    }
    close(&shared, &job.id, epoch).await;
}

/// The dispatcher could not work: the ticket says "choose an owner" (B5).
async fn gave_up(shared: &Arc<Shared>, ticket: TicketId, root: TaskId) {
    let s = shared.clone();
    match blocking(move || s.service.triage_gave_up(ticket, root, "dispatcher_failed")).await {
        Some(Ok(notices)) => shared.publish(notices),
        Some(Err(e)) => tracing::warn!(%ticket, error = %e.message, "could not tell the board that the dispatcher failed"),
        None => {}
    }
}

/// Renew the job's lease; false when it is no longer ours.
pub(crate) async fn renew(shared: &Arc<Shared>, job: &str, epoch: i64) -> bool {
    let (store, id) = (shared.store().clone(), job.to_owned());
    let renewed = blocking(move || store.write(move |tx| queries::renew_lease(tx, &id, epoch, LEASE_MS))).await;
    if !matches!(renewed, Some(Ok(true))) {
        tracing::warn!(job, "the dispatcher's job lost its lease; stopping without further writes");
        return false;
    }
    true
}

/// How the dispatcher's run ended: done (the error when it failed), or it
/// waits for the ticket's Claude login.
pub(crate) enum Ended {
    Done(Result<(), String>),
    Login,
    /// Its login's usage limit, with the reset when Claude named one.
    Quota(Option<i64>),
}

/// One run of a dispatcher: for a ticket (`--triage`) or a work session (`--session-dispatch`),
/// with the bridge's flag and the tools it may use without asking.
pub(crate) struct OneRun {
    pub run_id: RunId,
    pub config: aurelius_application::run::connections::ResolvedRunConfig,
    pub system: String,
    pub prompt: String,
    pub flag: &'static str,
    pub tools: &'static [&'static str],
}

/// Run the dispatcher until its answer is complete; none when the job lost its
/// lease (the run is stopped).
pub(crate) async fn drive(shared: &Arc<Shared>, run: OneRun, job: &str, epoch: i64) -> Option<Ended> {
    let claude = run.config.provider == Some(aurelius_domain::connection::ProviderKind::ClaudeCode);
    let login = run.config.login_user;
    let runtime = match runtimes::runtime_for_config(shared, &run.config) {
        Ok(runtime) => runtime,
        Err(aurelius_runtime::RuntimeError::Login(_)) => return Some(Ended::Login),
        Err(e) => return Some(Ended::Done(Err(e.to_string()))),
    };
    let bridge = crate::dispatcher::with_flag(crate::dispatcher::approval_bridge(shared, run.run_id), run.flag);
    let request = RunRequest {
        run_id: run.run_id,
        model_id: run.config.model.clone().unwrap_or_default(),
        system: Some(run.system),
        turns: vec![Turn { role: TurnRole::User, text: run.prompt, images: Vec::new() }],
        effort: run.config.effort.clone(),
        profile: ToolProfile::Triage { approval_bridge: bridge, tools: run.tools.iter().map(|t| t.to_string()).collect(), max_turns: MAX_TURNS },
        subagent_model: None,
        session: Default::default(),
        session_id: None,
        keep_session: false,
    };
    let mut handle = match runtime.start(request).await {
        Ok(handle) => handle,
        Err(aurelius_runtime::RuntimeError::Login(_)) => return Some(Ended::Login),
        Err(e) => return Some(Ended::Done(Err(e.to_string()))),
    };
    remember_canceller(shared, run.run_id, Some(handle.canceller()));
    let mut renewal = tokio::time::interval_at(tokio::time::Instant::now() + RENEW_EVERY, RENEW_EVERY);
    let result = loop {
        let event = tokio::select! {
            event = handle.events.recv() => event,
            _ = renewal.tick() => {
                if !renew(shared, job, epoch).await {
                    handle.canceller().cancel();
                    remember_canceller(shared, run.run_id, None);
                    return None;
                }
                continue;
            }
        };
        let Some(event) = event else { break Err("stream ended without a result".to_string()) };
        match event {
            RuntimeEvent::Usage(usage) => {
                let (s, id) = (shared.clone(), run.run_id);
                if let Some(Ok(notices)) = blocking(move || s.service.record_usage(id, &usage)).await {
                    shared.publish(notices);
                }
            }
            RuntimeEvent::Completed => break Ok(()),
            RuntimeEvent::Interrupted => break Err("interrupted".to_string()),
            RuntimeEvent::Quota { reset_at_ms } => {
                remember_canceller(shared, run.run_id, None);
                return Some(Ended::Quota(reset_at_ms));
            }
            RuntimeEvent::Failed { code, .. } if claude && code == aurelius_runtime::claude_code::parse::AUTH_FAILED => {
                // Claude refused the ticket's login (a 401): expired until reconnected.
                if let Some(user) = login {
                    let s = shared.clone();
                    if let Some(Ok(notices)) = blocking(move || s.service.claude_login_expired(user)).await {
                        shared.publish(notices);
                    }
                }
                remember_canceller(shared, run.run_id, None);
                return Some(Ended::Login);
            }
            RuntimeEvent::Failed { code, message } => break Err(format!("{code}: {message}")),
            _ => {}
        }
    };
    remember_canceller(shared, run.run_id, None);
    Some(Ended::Done(result))
}

/// A cancelled ticket stops its dispatcher's run too (`api::writes` stops the runs of its tasks).
pub(crate) fn remember_canceller(shared: &Shared, run: RunId, canceller: Option<aurelius_runtime::Canceller>) {
    if let Ok(mut map) = shared.cancellers.lock() {
        match canceller {
            Some(c) => {
                map.insert(run, c);
            }
            None => {
                map.remove(&run);
            }
        }
    }
}

pub(crate) async fn close(shared: &Arc<Shared>, job: &str, epoch: i64) {
    shared.live.job_ended(job);
    let (store, id) = (shared.store().clone(), job.to_owned());
    let _ = blocking(move || store.write(move |tx| queries::complete_job(tx, &id, epoch).map(|_| ()))).await;
}
