//! The dispatcher's job (`ticket.triage`, see `aurelius_application::task::triage`):
//! one short run of the ticket's dispatcher with only the bridge's
//! `assign_ticket` and `recall`, at most `MAX_TURNS` turns. A run that chose
//! nobody gets one more; then the ticket waits for a board member. So does a
//! dispatcher that cannot work (`dispatcher_failed`), also after one more try.
//!
//! The job keeps its lease while the dispatcher chooses, as a chat run does
//! (`dispatcher::chat`); lost, it stops without writing anything more.

use crate::{Shared, runtimes};
use aurelius_application::task::triage::{ATTEMPTS, MAX_TURNS, TOOLS, TriageRun};
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

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
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
        let run_id = run.run_id;
        let Some(ended) = drive(&shared, run, &job.id, epoch).await else { return };
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
async fn renew(shared: &Arc<Shared>, job: &str, epoch: i64) -> bool {
    let (store, id) = (shared.store().clone(), job.to_owned());
    let renewed = blocking(move || store.write(move |tx| queries::renew_lease(tx, &id, epoch, LEASE_MS))).await;
    if !matches!(renewed, Some(Ok(true))) {
        tracing::warn!(job, "the dispatcher's job lost its lease; stopping without further writes");
        return false;
    }
    true
}

/// Run the dispatcher until its answer is complete; the error when it failed,
/// none when the job lost its lease (the run is stopped).
async fn drive(shared: &Arc<Shared>, run: TriageRun, job: &str, epoch: i64) -> Option<Result<(), String>> {
    let runtime = match runtimes::runtime_for_config(shared, &run.config) {
        Ok(runtime) => runtime,
        Err(e) => return Some(Err(e.to_string())),
    };
    let bridge = crate::dispatcher::with_flag(crate::dispatcher::approval_bridge(shared, run.run_id), "--triage");
    let request = RunRequest {
        run_id: run.run_id,
        model_id: run.config.model.clone().unwrap_or_default(),
        system: Some(run.system),
        turns: vec![Turn { role: TurnRole::User, text: run.prompt, images: Vec::new() }],
        effort: run.config.effort.clone(),
        profile: ToolProfile::Triage { approval_bridge: bridge, tools: TOOLS.iter().map(|t| t.to_string()).collect(), max_turns: MAX_TURNS },
        subagent_model: None,
        session: Default::default(),
        session_id: None,
        keep_session: false,
    };
    let mut handle = match runtime.start(request).await {
        Ok(handle) => handle,
        Err(e) => return Some(Err(e.to_string())),
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
            RuntimeEvent::Quota { .. } => break Err("rate_limit: the usage limit is reached".to_string()),
            RuntimeEvent::Failed { code, message } => break Err(format!("{code}: {message}")),
            _ => {}
        }
    };
    remember_canceller(shared, run.run_id, None);
    Some(result)
}

/// A cancelled ticket stops its dispatcher's run too (`api::writes` stops the runs of its tasks).
fn remember_canceller(shared: &Shared, run: RunId, canceller: Option<aurelius_runtime::Canceller>) {
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

async fn close(shared: &Arc<Shared>, job: &str, epoch: i64) {
    shared.live.job_ended(job);
    let (store, id) = (shared.store().clone(), job.to_owned());
    let _ = blocking(move || store.write(move |tx| queries::complete_job(tx, &id, epoch).map(|_| ()))).await;
}
