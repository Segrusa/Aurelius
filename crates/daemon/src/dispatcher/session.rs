//! Which runtime session a chat run uses: the conversation's own, going on in it
//! with only the new messages, or a new one with the whole conversation.

use super::*;
use aurelius_application::run::session::{COMPACT_COMMAND, Claimed, Compaction, compacted_line, mcp_change, with_note};
use aurelius_runtime::{AgentRuntime, Turn};
use aurelius_storage::conversation_sessions::{Claim, SessionKey, SessionRow};

/// What the run asks of the runtime.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Choice {
    pub session: SessionUse,
    /// The conversation's session; none = the run's own.
    pub session_id: Option<String>,
    /// Kept after the run completes: the conversation goes on in it.
    pub keep: bool,
    pub turns: Vec<Turn>,
    /// The conversation's session this run holds (stored when it completes).
    pub claimed: Option<Claimed>,
    /// Only the new messages go in.
    pub resumed: bool,
    /// The session's own system text, given instead of the run's (it goes on in it).
    pub system: Option<String>,
    /// Goes on in it only after compacting it (its context grew past the threshold).
    pub compact: bool,
}

/// A session's system text is kept this long; then the next run starts a new session
/// with the current one (the change notes would only grow).
pub(super) const SYSTEM_TEXT_MAX_AGE_MS: i64 = 24 * 3_600_000;

/// The session's own system text, if it keeps one.
fn frozen(s: &SessionRow) -> Option<String> {
    (!s.system_text.is_empty()).then(|| s.system_text.clone())
}

impl Choice {
    /// A session of the run's own, as before conversations had one: the turns are the
    /// whole conversation, and it is kept only past a usage limit.
    fn own(plan: &ChatRunPlan, parked: bool) -> Self {
        let session = if parked { SessionUse::Resume } else { SessionUse::Keep };
        Choice { session, session_id: None, keep: false, turns: plan.turns.clone(), claimed: None, resumed: false, system: None, compact: false }
    }
}

/// What a session is started with; a change starts a new one. The mode and the
/// MCP list may change (the new message says so), the run's bridge always does.
/// The version of the platform rules is part of it: a resumed session keeps its
/// system text for up to a day, and never gets other rules (guidelines B6).
pub(super) fn session_key(plan: &ChatRunPlan, profile: &ToolProfile) -> SessionKey {
    let model = plan.config.model.clone().unwrap_or_default();
    match profile {
        ToolProfile::Work { folders, browser, playwright, plugins, artifacts, .. } => SessionKey {
            model,
            folder: folders.first().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(),
            profile: serde_json::json!({ "folders": folders, "browser": browser, "playwright": playwright.is_some(), "plugins": plugins, "artifacts": artifacts, "platform_rules": aurelius_application::run::plan::PLATFORM_RULES_VERSION }).to_string(),
        },
        ToolProfile::TextOnly => SessionKey { model, folder: String::new(), profile: "text".into() },
        ToolProfile::Triage { .. } => SessionKey { model, folder: String::new(), profile: "triage".into() },
    }
}

fn mcp_names(profile: &ToolProfile) -> Vec<String> {
    match profile {
        ToolProfile::Work { mcp_servers, .. } => mcp_servers.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// Decide the run's session. Only a runtime that keeps sessions, at work in a
/// folder (Claude Code finds a session by its folder), without a pinned browser
/// session, goes on in the conversation's. It does when the stored session is
/// still there, the history only grew since its last answer and the model, folder
/// and profile are the same and its system text is less than a day old; otherwise
/// this run starts the conversation's new session from the whole conversation,
/// which keeps `system` (the run's system text). While another run holds it, this
/// run keeps a session of its own.
pub(super) async fn choose(shared: &Arc<Shared>, plan: &ChatRunPlan, runtime: &Arc<dyn AgentRuntime>, profile: &ToolProfile, parked: bool, system: Option<&str>) -> Choice {
    let work = matches!(profile, ToolProfile::Work { .. });
    if !runtime.keeps_sessions() || !work || aurelius_runtime::claude_code::pins_session(profile) {
        return Choice::own(plan, parked);
    }
    let run = plan.run_id.to_string();
    let mcp = mcp_names(profile);
    let stored = plan.session.stored.as_ref();
    if parked {
        // The earlier attempt went on in the conversation's session: carry on there
        // (or start it over from the turns when the runtime no longer has it).
        return match stored.filter(|s| s.busy_run.as_deref() == Some(run.as_str())) {
            Some(s) => Choice {
                session: SessionUse::Resume,
                session_id: Some(s.session_id.clone()),
                keep: true,
                turns: plan.turns.clone(),
                claimed: Some(Claimed { session_id: s.session_id.clone(), mcp, resumed: true }),
                resumed: false,
                system: frozen(s),
                compact: false,
            },
            None => Choice::own(plan, parked),
        };
    }
    let key = session_key(plan, profile);
    let resume = match (stored, &plan.session.resume) {
        (Some(s), Some(r))
            if s.model == key.model
                && s.folder == key.folder
                && s.profile == key.profile
                && (s.system_text.is_empty() || aurelius_storage::now_ms() - s.system_at_ms < SYSTEM_TEXT_MAX_AGE_MS)
                && runtime.has_session(&s.session_id) =>
        {
            let before: aurelius_application::run::session::SessionContext = serde_json::from_str(&s.context_json).unwrap_or_default();
            let mut changes = r.changes.clone();
            changes.extend(mcp_change(&before.mcp, &mcp));
            // A command starts the prompt: with something to say first, start over.
            (!r.command || changes.is_empty()).then(|| (r.session_id.clone(), with_note(r.turn.clone(), &changes), frozen(s)))
        }
        _ => None,
    };
    let (s, conversation, run_id, want) = (shared.clone(), plan.conversation_id, plan.run_id, resume.as_ref().map(|(id, _, _)| id.clone()));
    let system = system.unwrap_or_default().to_string();
    match blocking(move || s.service.claim_session(conversation, run_id, want, key, system)).await {
        Some(Ok(Claim::Continue)) => {
            let (session_id, turn, system) = resume.expect("continue is only granted for the session asked for");
            Choice { session: SessionUse::Continue, session_id: Some(session_id.clone()), keep: true, turns: vec![turn], claimed: Some(Claimed { session_id, mcp, resumed: true }), resumed: true, system, compact: plan.session.compact }
        }
        Some(Ok(Claim::Start { replaced })) => started(plan, runtime, replaced.filter(|id| shared.owns_session(id)), mcp),
        Some(Ok(Claim::Busy)) => Choice::own(plan, false),
        other => {
            tracing::warn!(run = %plan.run_id, ?other, "could not claim the conversation's session; this run keeps its own");
            Choice::own(plan, false)
        }
    }
}

/// The run starts the conversation's new session (named after it) from the whole
/// conversation; the one it replaces goes.
fn started(plan: &ChatRunPlan, runtime: &Arc<dyn AgentRuntime>, replaced: Option<String>, mcp: Vec<String>) -> Choice {
    if let Some(old) = replaced {
        runtime.remove_session(&old);
    }
    let run = plan.run_id.to_string();
    Choice {
        session: SessionUse::Keep,
        session_id: Some(run.clone()),
        keep: true,
        turns: plan.turns.clone(),
        claimed: Some(Claimed { session_id: run, mcp, resumed: false }),
        resumed: false,
        system: None,
        compact: false,
    }
}

/// How long compacting may take before the run gives up on it.
const COMPACT_TIMEOUT: Duration = Duration::from_secs(300);

/// What a compaction before the run's turn came to.
pub(super) enum Compacted {
    /// The run goes on with this choice; the line says what happened (for the trace).
    Go(Choice, String),
    /// The run's lease was lost meanwhile: it stops without writing.
    LeaseLost,
}

/// The conversation's session grew past the threshold: compact it with `/compact`
/// in the same session (with the same system text, tools and model, so the prompt
/// cache holds), then the run's turn goes on in it with the earlier context
/// summarised; a line in the conversation says so. When it did not compact, the
/// run starts the conversation's session over from the whole conversation instead
/// (as when it cannot go on in it). `request` is the run's own (its system text).
pub(super) async fn compact_first(shared: &Arc<Shared>, plan: &ChatRunPlan, runtime: &Arc<dyn AgentRuntime>, request: &RunRequest, choice: Choice, job: &str, epoch: i64) -> Compacted {
    let Some(session_id) = choice.session_id.clone().filter(|_| choice.compact) else { return Compacted::Go(choice, String::new()) };
    let compact = RunRequest {
        system: choice.system.clone().or_else(|| request.system.clone()),
        turns: vec![Turn { role: aurelius_runtime::TurnRole::User, text: COMPACT_COMMAND.into(), images: Vec::new() }],
        session: SessionUse::Continue,
        session_id: Some(session_id),
        keep_session: true,
        ..request.clone()
    };
    let begun = tokio::time::timeout(COMPACT_TIMEOUT, runtime.start(compact)).await;
    let outcome = match begun.unwrap_or_else(|_| Err(aurelius_runtime::RuntimeError::Unavailable(format!("it did not start within {} s", COMPACT_TIMEOUT.as_secs())))) {
        Ok(mut handle) => {
            // A stop meanwhile ends it (the run then stops when it starts).
            if let Ok(mut map) = shared.cancellers.lock() {
                map.insert(plan.run_id, handle.canceller());
            }
            let mut compaction = Compaction::default();
            let mut over = false;
            let mut deadline = tokio::time::Instant::now() + COMPACT_TIMEOUT;
            let mut timed_out = false;
            let mut last_renew = tokio::time::Instant::now();
            // Until the stream closes: the runtime is done with the session (and the
            // run's system file) before the turn uses them.
            loop {
                let event = match tokio::time::timeout_at(deadline, handle.events.recv()).await {
                    Ok(Some(event)) => event,
                    Ok(None) => break,
                    Err(_) if !timed_out => {
                        timed_out = true;
                        handle.canceller().cancel();
                        deadline = tokio::time::Instant::now() + Duration::from_secs(30);
                        continue;
                    }
                    Err(_) => break,
                };
                if last_renew.elapsed() >= RENEW_EVERY {
                    last_renew = tokio::time::Instant::now();
                    let (store, id) = (shared.store().clone(), job.to_string());
                    if !matches!(blocking(move || store.write(move |tx| queries::renew_lease(tx, &id, epoch, LEASE_MS))).await, Some(Ok(true))) {
                        tracing::warn!(run = %plan.run_id, "lease lost while compacting; stopping without further writes");
                        handle.canceller().cancel();
                        return Compacted::LeaseLost;
                    }
                }
                // What compacting cost counts for the run.
                if let RuntimeEvent::Usage(usage) = &event {
                    let (s, run_id, usage) = (shared.clone(), plan.run_id, usage.clone());
                    if let Some(Ok(notices)) = blocking(move || s.service.record_usage(run_id, &usage)).await {
                        shared.publish(notices);
                    }
                }
                if !over {
                    over = compaction.feed(&event);
                }
            }
            if timed_out && !over {
                Err(format!("it took longer than {} s", COMPACT_TIMEOUT.as_secs()))
            } else {
                compaction.outcome()
            }
        }
        Err(err) => Err(err.to_string()),
    };
    match outcome {
        Ok((pre, post)) => {
            tracing::info!(run = %plan.run_id, pre, post, "compacted the conversation's session before the turn");
            let (s, conversation) = (shared.clone(), plan.conversation_id);
            match blocking(move || s.service.record_compaction(conversation, pre, post)).await {
                Some(Ok(notices)) => shared.publish(notices),
                other => tracing::warn!(run = %plan.run_id, ?other, "could not show that the session was compacted"),
            }
            Compacted::Go(Choice { compact: false, ..choice }, compacted_line(pre, post))
        }
        Err(why) => {
            // Start over from the whole conversation: the run holds the session, so it gets the new one.
            tracing::warn!(run = %plan.run_id, %why, "could not compact the conversation's session; starting it over");
            let mcp = choice.claimed.as_ref().map(|c| c.mcp.clone()).unwrap_or_default();
            let key = session_key(plan, &request.profile);
            let (s, conversation, run_id, system) = (shared.clone(), plan.conversation_id, plan.run_id, request.system.clone().unwrap_or_default());
            let line = format!("Compacting the context failed ({why}); started a new session from the conversation");
            match blocking(move || s.service.claim_session(conversation, run_id, None, key, system)).await {
                Some(Ok(Claim::Start { replaced })) => Compacted::Go(started(plan, runtime, replaced.filter(|id| shared.owns_session(id)), mcp), line),
                other => {
                    tracing::warn!(run = %plan.run_id, ?other, "could not start the conversation's session over; this run keeps its own");
                    Compacted::Go(Choice::own(plan, false), line)
                }
            }
        }
    }
}
