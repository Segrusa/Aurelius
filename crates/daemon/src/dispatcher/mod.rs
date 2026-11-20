//! The durable job dispatcher: claims due jobs under the load limits and runs
//! them (chat answers, task starts, memory jobs). A chat job plans a run, drives
//! the runtime and commits the outcome; every write is fenced by the lease epoch.

use crate::{Shared, runtimes};
use aurelius_application::run::{ChatRunPlan, RunOutcome};
use aurelius_application::CHAT_RUN_KIND;
use aurelius_protocol::StreamDelta;
use aurelius_runtime::{Activity, ApprovalBridge, RunRequest, RuntimeEvent, SessionUse, ToolProfile};
use aurelius_storage::queries;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

mod chat;
mod session;
mod tooling;

pub(crate) use tooling::{approval_bridge, permission_mode, started_in, with_flag};

const POLL: Duration = Duration::from_millis(200);
const LEASE_MS: i64 = 45_000;
const RENEW_EVERY: Duration = Duration::from_secs(10);

const EXPIRY_SWEEP: Duration = Duration::from_secs(5);
/// How often runs held back by the machine's load are told why.
const NOTE_HELD_EVERY: Duration = Duration::from_secs(2);

pub async fn run(shared: Arc<Shared>) {
    let owner = format!("daemon-{}", std::process::id());
    let mut last_sweep = tokio::time::Instant::now();
    let mut gate = crate::load::Gate::default();
    let mut last_note: Option<tokio::time::Instant> = None;
    loop {
        if last_sweep.elapsed() >= EXPIRY_SWEEP {
            last_sweep = tokio::time::Instant::now();
            let s = shared.clone();
            if let Ok(Ok(notices)) = tokio::task::spawn_blocking(move || s.service.expire_approvals()).await {
                shared.publish(notices);
            }
            // A chat run nobody drives any more (its lease ran out) must not hold its
            // conversation: it ends with a note, and what waits there goes next.
            let driven = shared.drivers.lock().map(|d| d.clone()).ok();
            if let Some(driven) = driven {
                let s = shared.clone();
                if let Ok(Ok(notices)) = tokio::task::spawn_blocking(move || s.service.end_abandoned_chat_runs(aurelius_storage::now_ms(), &driven)).await {
                    shared.publish(notices);
                }
            }
        }
        // After a restore nothing starts until the user resumes.
        if shared.paused.load(std::sync::atomic::Ordering::Relaxed) {
            tokio::time::sleep(POLL).await;
            continue;
        }
        // New runs only start while the machine has room; running ones go on.
        let running = shared.store().read(queries::running_runs).unwrap_or(0);
        // The limits as the user set them just now.
        let limits = shared.admission.limits();
        let load = if shared.admission.checks_active && limits.measured { shared.load.current() } else { None };
        let (verdict, next) = crate::load::admit(load, &limits, running, gate);
        gate = next;
        let held = match verdict {
            Ok(()) => 0,
            Err(_) => shared.store().read(|c| queries::held_runs(c, aurelius_storage::now_ms())).unwrap_or(0),
        };
        crate::load::observe(&shared, verdict, running, held);
        let limit = match verdict {
            Ok(()) => limits.max_runs,
            Err(hold) => {
                if last_note.is_none_or(|t| t.elapsed() >= NOTE_HELD_EVERY) {
                    last_note = Some(tokio::time::Instant::now());
                    let s = shared.clone();
                    if let Ok(Ok(notices)) = tokio::task::spawn_blocking(move || s.service.note_held_runs(hold.code())).await {
                        if !notices.is_empty() {
                            tracing::info!(reason = hold.code(), running, ?load, held = notices.len(), "new runs wait for the machine");
                        }
                        shared.publish(notices);
                    }
                }
                0
            }
        };
        // Handoffs held for a sender that has caught up go out first (R5).
        let s = shared.clone();
        if let Ok(Ok(notices)) = tokio::task::spawn_blocking(move || s.service.release_handoffs()).await {
            shared.publish(notices);
        }
        // Scheduled messages that came due go out as items of their mailboxes (R7).
        let s = shared.clone();
        if let Ok(Ok(notices)) = tokio::task::spawn_blocking(move || s.service.deliver_scheduled_messages()).await {
            shared.publish(notices);
        }
        let s = shared.clone();
        let owner2 = owner.clone();
        // A worker's item first (it becomes a run with a leased job), else a due job.
        let claimed = tokio::task::spawn_blocking(move || s.service.claim_next_work(&owner2, LEASE_MS, limit)).await.map(|r| {
            r.map(|work| {
                work.map(|(job, notices)| {
                    shared.publish(notices);
                    job
                })
            })
        });
        match claimed {
            Ok(Ok(Some(job))) if job.kind == CHAT_RUN_KIND => {
                tokio::spawn(chat::run_chat(shared.clone(), job));
            }
            Ok(Ok(Some(job))) if job.kind == aurelius_application::task::tasks::TASK_JOB_KIND => {
                tokio::spawn(crate::tasks::run_task(shared.clone(), job));
            }
            Ok(Ok(Some(job))) if job.kind == aurelius_application::task::triage::TRIAGE_JOB_KIND => {
                tokio::spawn(crate::triage::run_triage(shared.clone(), job));
            }
            Ok(Ok(Some(job))) if job.kind == aurelius_application::conversation::session_dispatch::SESSION_DISPATCH_JOB_KIND => {
                tokio::spawn(crate::session_dispatch::run_session_dispatch(shared.clone(), job));
            }
            Ok(Ok(Some(job))) if job.kind == aurelius_application::ticket::quick::QUICK_MERGE_JOB_KIND => {
                tokio::spawn(crate::quick::run_merge(shared.clone(), job));
            }
            Ok(Ok(Some(job))) if job.kind == aurelius_application::ticket::quick::QUICK_CLEANUP_JOB_KIND => {
                tokio::spawn(crate::quick::run_cleanup(shared.clone(), job));
            }
            Ok(Ok(Some(job))) if job.kind == aurelius_application::memory::EXTRACT_JOB_KIND => {
                tokio::spawn(crate::memory::run_extraction(shared.clone(), job));
            }
            Ok(Ok(Some(job))) if job.kind == aurelius_application::memory::SUMMARY_JOB_KIND => {
                tokio::spawn(crate::memory::run_summary(shared.clone(), job));
            }
            Ok(Ok(Some(job))) if job.kind == aurelius_application::conversation::session_history::SESSION_SUMMARY_JOB_KIND => {
                tokio::spawn(crate::memory::run_session_summary(shared.clone(), job));
            }
            Ok(Ok(Some(job))) if job.kind == aurelius_application::conversation::session_title::SESSION_TITLE_JOB_KIND => {
                tokio::spawn(crate::memory::run_session_title(shared.clone(), job));
            }
            Ok(Ok(Some(job))) if job.kind == aurelius_application::ticket::title::TICKET_TITLE_JOB_KIND => {
                tokio::spawn(crate::memory::run_ticket_title(shared.clone(), job));
            }
            Ok(Ok(Some(job))) if job.kind == aurelius_application::memory::CONSOLIDATE_JOB_KIND => {
                tokio::spawn(crate::memory::run_consolidation(shared.clone(), job));
            }
            Ok(Ok(Some(job))) => tracing::warn!(kind = %job.kind, "no handler for job kind"),
            Ok(Ok(None)) => {
                // New load limits let waiting runs try at once.
                tokio::select! {
                    _ = tokio::time::sleep(POLL) => {}
                    _ = shared.admission.wake.notified() => {}
                }
            }
            Ok(Err(err)) => {
                tracing::error!(error = %err.message, "claim failed; pausing dispatch");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(_) => return,
        }
    }
}

/// While a chat job is worked on here, it is known as driven (see
/// `end_abandoned_chat_runs`); however its driver ends, the run's live state
/// ("Thinking…") and its switches go with it. A driver is known by its job and the
/// lease epoch it holds: an old driver of a job that was deferred and claimed again
/// may end after the new one started, and must neither make the job look undriven
/// nor take the new driver's live state and switches with it.
struct Driver {
    shared: Arc<Shared>,
    key: (String, i64),
    run: Option<aurelius_domain::RunId>,
}

impl Driver {
    fn new(shared: &Arc<Shared>, job: &str, epoch: i64) -> Self {
        let key = (job.to_owned(), epoch);
        if let Ok(mut driven) = shared.drivers.lock() {
            driven.insert(key.clone());
        }
        Driver { shared: shared.clone(), key, run: None }
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        let last = self.shared.drivers.lock().map(|mut driven| undrive(&mut driven, &self.key)).unwrap_or(true);
        if !last {
            return;
        }
        if let Some(run) = self.run {
            chat::forget_run(&self.shared, run);
            self.shared.live.run_ended(run);
        }
        self.shared.live.job_ended(&self.key.0);
    }
}

/// Forget this driver; true when no other driver of its job is left.
fn undrive(driven: &mut HashSet<(String, i64)>, key: &(String, i64)) -> bool {
    driven.remove(key);
    !driven.iter().any(|(job, _)| *job == key.0)
}

#[cfg(test)]
mod driver_tests {
    #[test]
    fn an_old_driver_ending_after_the_new_one_started_leaves_it_driving() {
        let mut driven = std::collections::HashSet::new();
        // Claimed under epoch 1, deferred, claimed again under epoch 2 before the old driver dropped.
        driven.insert(("job".to_string(), 1));
        driven.insert(("job".to_string(), 2));
        assert!(!super::undrive(&mut driven, &("job".into(), 1)), "the new driver still drives it");
        assert!(driven.contains(&("job".to_string(), 2)));
        assert!(super::undrive(&mut driven, &("job".into(), 2)));
        assert!(driven.is_empty());
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()
}

fn classify(err: &aurelius_runtime::RuntimeError) -> aurelius_domain::scheduler::Failure {
    use aurelius_runtime::RuntimeError as E;
    use aurelius_domain::scheduler::Failure as F;
    match err {
        E::Unavailable(m) => F::Transient(m.clone()),
        E::Auth(m) => F::Auth(m.clone()),
        E::Login(m) => F::Login(m.clone()),
        // Always a time: a limit without a known reset is tried again after a default delay.
        E::Quota { reset_at_ms } => F::Quota { reset_at_ms: Some(aurelius_domain::scheduler::quota_wake_ms(*reset_at_ms, aurelius_storage::now_ms())) },
        E::Permanent(m) => F::Permanent(m.clone()),
    }
}

/// A chat job that could not be planned: retry with backoff, then fail the run
/// (and its message). A job without a valid run id is given up at once.
async fn release_unplanned(shared: &Arc<Shared>, job: &queries::JobRow, code: &str) {
    let epoch = job.lease_epoch;
    let Ok(run_id) = aurelius_domain::RunId::parse(&job.id) else {
        let (store, id, code) = (shared.store().clone(), job.id.clone(), code.to_string());
        let _ = blocking(move || store.write(move |tx| queries::kill_job(tx, &id, epoch, &code))).await;
        return;
    };
    let store = shared.store().clone();
    let id = job.id.clone();
    let (attempt, max) = blocking(move || store.write(move |tx| queries::job_attempts(tx, &id))).await.and_then(Result::ok).unwrap_or((1, 3));
    let failure = aurelius_domain::scheduler::Failure::Transient(code.to_string());
    let disposition = aurelius_domain::scheduler::dispose(&failure, attempt, max, aurelius_storage::now_ms(), shared.backoff_base_ms, &job.id);
    let s = shared.clone();
    match blocking(move || s.service.defer_run(run_id, epoch, disposition)).await {
        Some(Ok(notices)) => shared.publish(notices),
        other => {
            tracing::error!(job = %job.id, ?other, "could not release unplanned chat job; failing it");
            let (store, id, code) = (shared.store().clone(), job.id.clone(), code.to_string());
            let _ = blocking(move || store.write(move |tx| queries::kill_job(tx, &id, epoch, &code))).await;
        }
    }
}
