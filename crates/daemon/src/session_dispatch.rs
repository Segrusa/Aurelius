//! The session dispatcher's job (`session.dispatch`, see
//! `aurelius_application::conversation::session_dispatch`): one short run of the
//! project's dispatcher with only the bridge's `assign_session` and `recall`, at
//! most `MAX_TURNS` turns, as the board's dispatcher (`triage`). A run that chose
//! nobody gets one more; then the board member chooses. So does a dispatcher that
//! cannot work, also after one more try.

use crate::Shared;
use crate::triage::{Ended, OneRun, blocking, close, drive, renew};
use aurelius_application::conversation::session_dispatch::{ATTEMPTS, TOOLS};
use aurelius_domain::ConversationId;
use aurelius_storage::queries;
use std::sync::Arc;
use std::time::Duration;

/// The pause before preparing the dispatcher's run again (as `triage`).
const PREPARE_AGAIN_AFTER: Duration = Duration::from_millis(1_500);

pub async fn run_session_dispatch(shared: Arc<Shared>, job: queries::JobRow) {
    let epoch = job.lease_epoch;
    let Some(conversation) = job.payload["conversation_id"].as_str().and_then(|c| ConversationId::parse(c).ok()) else {
        close(&shared, &job.id, epoch).await;
        return;
    };
    // A run of an earlier claim that never ended (the daemon stopped) is over.
    let s = shared.clone();
    if let Some(Err(e)) = blocking(move || s.service.session_dispatch_claimed(conversation)).await {
        tracing::warn!(%conversation, error = %e.message, "could not end the session dispatcher's earlier run");
    }
    let mut not_prepared = 0;
    loop {
        if !renew(&shared, &job.id, epoch).await {
            return;
        }
        let s = shared.clone();
        let (run, notices) = match blocking(move || s.service.session_dispatch_run(conversation)).await {
            Some(Ok(prepared)) => prepared,
            Some(Err(e)) => {
                tracing::warn!(%conversation, error = %e.message, "the session dispatcher's run could not be prepared");
                not_prepared += 1;
                if not_prepared < ATTEMPTS {
                    tokio::time::sleep(PREPARE_AGAIN_AFTER).await;
                    continue;
                }
                gave_up(&shared, conversation).await;
                break;
            }
            None => return,
        };
        shared.publish(notices);
        let Some(run) = run else { break };
        let (run_id, login) = (run.run_id, run.config.login_user);
        let one = OneRun { run_id, config: run.config, system: run.system, prompt: run.prompt, flag: "--session-dispatch", tools: &TOOLS };
        let Some(ended) = drive(&shared, one, &job.id, epoch).await else { return };
        let ended = match ended {
            // The login is missing or expired, or hit its limit: the job waits (no try used).
            Ended::Login | Ended::Quota(_) => {
                let until = match ended {
                    Ended::Quota(reset_at_ms) => Some(aurelius_domain::scheduler::quota_wake_ms(reset_at_ms, aurelius_storage::now_ms())),
                    _ => None,
                };
                let (s, id) = (shared.clone(), job.id.clone());
                if let Some(Ok(notices)) = blocking(move || s.service.session_dispatch_waits(conversation, run_id, id, epoch, until)).await {
                    shared.publish(notices);
                }
                if let (Some(at), Some(login)) = (until, login) {
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
        match blocking(move || s.service.session_dispatch_ended(conversation, run_id, ended.err())).await {
            // It chose nobody: one more run, told to choose.
            Some(Ok((true, notices))) => shared.publish(notices),
            Some(Ok((false, notices))) => {
                shared.publish(notices);
                break;
            }
            Some(Err(e)) => {
                tracing::warn!(%conversation, error = %e.message, "the session dispatcher's run could not be closed");
                gave_up(&shared, conversation).await;
                break;
            }
            None => break,
        }
    }
    close(&shared, &job.id, epoch).await;
}

/// The dispatcher could not work: the session asks the board member to choose.
async fn gave_up(shared: &Arc<Shared>, conversation: ConversationId) {
    let s = shared.clone();
    match blocking(move || s.service.session_dispatch_gave_up(conversation, "dispatcher_failed")).await {
        Some(Ok(notices)) => shared.publish(notices),
        Some(Err(e)) => tracing::warn!(%conversation, error = %e.message, "could not tell the session that the dispatcher failed"),
        None => {}
    }
}
