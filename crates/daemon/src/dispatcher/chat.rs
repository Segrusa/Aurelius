//! One chat run: plan, start the runtime, stream, checkpoint and finish.

use super::*;

const CHECKPOINT_EVERY: Duration = Duration::from_millis(1500);
/// While a tool call is open the run may be silent this many times the idle limit
/// (a long build or test run says nothing until it ends).
const TOOL_IDLE_FACTOR: u32 = 3;
/// How long a stopped runtime gets to end its stream before the run ends anyway.
const STOP_GRACE: Duration = Duration::from_secs(10);

/// Execute one chat run: plan, ask the runtime to start (retry, park or fail
/// on error), then stream deltas with durable checkpoints and commit the
/// terminal outcome. Every write is fenced by the lease epoch.
pub(super) async fn run_chat(shared: Arc<Shared>, job: queries::JobRow) {
    let epoch = job.lease_epoch;
    let mut driver = super::Driver::new(&shared, &job.id, epoch);
    let s = shared.clone();
    let payload = job.payload.clone();
    let mut plan: ChatRunPlan = match blocking(move || s.service.prepare_chat_run(&payload)).await {
        Some(Ok(Some(plan))) => plan,
        Some(Ok(None)) => {
            // Cancelled before start; close the job under our lease.
            let store = shared.store().clone();
            let id = job.id.clone();
            let _ = blocking(move || store.write(move |tx| queries::complete_job(tx, &id, epoch))).await;
            return;
        }
        failed => {
            // Never keep the lease: it would block every later message of this conversation.
            let reason = match failed {
                Some(Err(err)) => err.to_string(),
                _ => "planning panicked".to_string(),
            };
            tracing::error!(job = %job.id, error = %reason, "could not plan chat run");
            release_unplanned(&shared, &job, "plan_failed").await;
            return;
        }
    };

    driver.run = Some(plan.run_id);
    let idle = idle_limit(&shared).await;

    if let Some(explanation) = plan.context_error.clone() {
        // Mandatory context does not fit: fail explicitly, never truncate silently.
        let (s, p) = (shared.clone(), plan.clone());
        if let Some(Ok(Some(notices))) = blocking(move || s.service.start_chat_run(&p, epoch)).await {
            shared.publish(notices);
        }
        let (s, p) = (shared.clone(), plan.clone());
        if let Some(Ok(notices)) = blocking(move || s.service.finish_chat_run(&p, RunOutcome::Failed("context_too_large".into()), &explanation, epoch)).await {
            shared.publish(notices);
        }
        return;
    }
    let s = shared.clone();
    let run_id = plan.run_id;
    let parked = blocking(move || s.service.run_was_quota_parked(run_id)).await == Some(true);
    let mut request = RunRequest {
        run_id: plan.run_id,
        model_id: plan.config.model.clone().unwrap_or_default(),
        // Also when the run goes on in the conversation's session (Claude Code does not
        // keep it with the session): then the session's own text, as it started with it.
        system: plan.system.clone(),
        profile: tooling::tool_profile(&shared, &plan),
        turns: Vec::new(),
        effort: plan.config.effort.clone(),
        subagent_model: plan.config.subagent_model.clone(),
        session: SessionUse::Keep,
        session_id: None,
        keep_session: false,
    };
    let runtime = match runtimes::runtime_for(&shared, &plan) {
        Ok(runtime) => runtime,
        Err(err) => {
            let disposition = aurelius_domain::scheduler::dispose(&classify(&err), 1, 1, aurelius_storage::now_ms(), shared.backoff_base_ms, &job.id);
            tracing::warn!(run = %plan.run_id, error = %err, "no usable runtime for this run");
            let (s, p) = (shared.clone(), plan.clone());
            if let Some(Ok(notices)) = blocking(move || s.service.defer_chat_run(&p, epoch, disposition)).await {
                shared.publish(notices);
            }
            return;
        }
    };
    // Said with this answer's message, not in the system text a session keeps.
    let mut browser_note = None;
    if plan.browser && plan.work.is_some() && plan.playwright {
        // Playwright MCP needs its package and Aurelius' browser window (with a DevTools port).
        // Without them the run goes on without a browser and says why, never with Chrome instead.
        match aurelius_host::playwright::prepare_run(shared.store().root()).await {
            Ok(config) => {
                if let ToolProfile::Work { playwright, .. } = &mut request.profile {
                    *playwright = Some(config);
                }
            }
            Err(why) => {
                tracing::warn!(run = %plan.run_id, %why, "browser allowed but Playwright is not available; running without a browser");
                if let ToolProfile::Work { browser, .. } = &mut request.profile {
                    *browser = false;
                }
                browser_note = Some(format!("(The browser could not be started for this answer ({why}). Work without it and tell the user why.)"));
            }
        }
    } else if plan.browser && plan.work.is_some() {
        // Claude in Chrome needs a running Chrome; start it windowless if it isn't.
        match aurelius_host::chrome::ensure_running().await {
            aurelius_host::chrome::ChromeState::Running => {}
            aurelius_host::chrome::ChromeState::Started => tracing::info!(run = %plan.run_id, "started Chrome in the background for this run"),
            aurelius_host::chrome::ChromeState::Unavailable(why) => tracing::warn!(run = %plan.run_id, %why, "browser allowed but Chrome is not available; its tools will fail"),
        }
    }
    // The conversation's session: go on in it with only the new messages, or start
    // it (again) from the whole conversation.
    let mut choice = session::choose(&shared, &plan, &runtime, &request.profile, parked, request.system.as_deref()).await;
    // Its context grew too big: compacted first (or started over when that fails).
    if choice.compact {
        // While it compacts the answer already shows as running ("Context comprimeren…"):
        // the answer message and the job's clock exist before the turn starts (starting
        // again below finds both). A lost lease falls through to compact_first's own check.
        let (s, p) = (shared.clone(), plan.clone());
        if let Some(Ok(Some(notices))) = blocking(move || s.service.start_chat_run(&p, epoch)).await {
            shared.live.run_started(&job.id, plan.run_id);
            shared.live.set_compacting(plan.run_id, true);
            shared.publish(notices);
            send_progress(&shared, &plan);
        }
        let compacted = session::compact_first(&shared, &plan, &runtime, &request, choice, &job.id, epoch).await;
        shared.live.set_compacting(plan.run_id, false);
        match compacted {
            session::Compacted::Go(next, line) => {
                choice = next;
                plan.session.compaction = Some(line);
                send_progress(&shared, &plan);
            }
            // Its driver forgets the run as it ends (unless a newer one drives it).
            session::Compacted::LeaseLost => return,
        }
    }
    if let Some(system) = choice.system.clone() {
        request.system = Some(system);
    }
    let mut turns = choice.turns;
    if let (Some(note), Some(last)) = (&browser_note, turns.last_mut()) {
        last.text = format!("{}\n\n{note}", last.text);
    }
    // A run that waited for the usage limit carries on where the earlier attempt
    // stopped: in its session when the runtime still has it, otherwise from the
    // turns with a note.
    if parked {
        if let Some(last) = turns.last_mut() {
            last.text = format!("{}\n\n{QUOTA_RESUME_NOTE}", last.text);
        }
    }
    request.turns = turns;
    request.session = choice.session;
    request.session_id = choice.session_id;
    request.keep_session = choice.keep;
    plan.session.claimed = choice.claimed;
    if choice.resumed {
        // The runtime's session holds the conversation; only the new messages were sent.
        if let Some(manifest) = plan.manifest.as_mut() {
            manifest.runtime_managed = true;
        }
        tracing::debug!(run = %plan.run_id, "going on in the conversation's session");
    }
    // Trace mode: keep what goes to the runtime (the runtime adds what it sent exactly).
    let s = shared.clone();
    let mut traced = blocking(move || s.service.trace_enabled()).await == Some(true);
    if traced {
        let (s, p, r, resumed) = (shared.clone(), plan.clone(), request.clone(), choice.resumed);
        traced = matches!(blocking(move || s.service.record_trace_request(&p, &r, resumed)).await, Some(Ok(true)));
    }
    // A runtime that does not even start counts as unavailable (tried again later).
    let started = tokio::time::timeout(idle, runtime.start(request)).await.unwrap_or_else(|_| Err(aurelius_runtime::RuntimeError::Unavailable(format!("the runtime did not start within {} s", idle.as_secs()))));
    let mut handle = match started {
        Ok(handle) => handle,
        Err(err) => {
            let store = shared.store().clone();
            let id = job.id.clone();
            let (attempt, max) = blocking(move || store.write(move |tx| queries::job_attempts(tx, &id))).await.and_then(Result::ok).unwrap_or((1, 3));
            let disposition = aurelius_domain::scheduler::dispose(&classify(&err), attempt, max, aurelius_storage::now_ms(), shared.backoff_base_ms, &job.id);
            tracing::warn!(run = %plan.run_id, error = %err, ?disposition, "runtime could not start the turn");
            let (s, p) = (shared.clone(), plan.clone());
            if let Some(Ok(notices)) = blocking(move || s.service.defer_chat_run(&p, epoch, disposition)).await {
                shared.publish(notices);
            }
            return;
        }
    };
    let s = shared.clone();
    let p = plan.clone();
    // Register before the run becomes visible as started, so a Stop never slips through.
    if let Ok(mut map) = shared.cancellers.lock() {
        map.insert(plan.run_id, handle.canceller());
    }
    match blocking(move || s.service.start_chat_run(&p, epoch)).await {
        Some(Ok(Some(notices))) => shared.publish(notices),
        Some(Ok(None)) => {
            // Lease lost before we started: abandon this turn without writing.
            handle.canceller().cancel();
            return;
        }
        other => {
            // The start could not be recorded: never leave the run "running" without a
            // driver (its conversation would wait for it forever). The turn stops and the
            // run is tried again later, or fails with the reason in its answer.
            tracing::error!(run = %plan.run_id, ?other, "could not record the run's start; stopping its turn");
            stop(&mut handle, STOP_GRACE).await;
            let store = shared.store().clone();
            let id = job.id.clone();
            let (attempt, max) = blocking(move || store.write(move |tx| queries::job_attempts(tx, &id))).await.and_then(Result::ok).unwrap_or((1, 3));
            let failure = aurelius_domain::scheduler::Failure::Transient("start_not_recorded".into());
            let disposition = aurelius_domain::scheduler::dispose(&failure, attempt, max, aurelius_storage::now_ms(), shared.backoff_base_ms, &job.id);
            let (s, p) = (shared.clone(), plan.clone());
            match blocking(move || s.service.defer_chat_run(&p, epoch, disposition)).await {
                Some(Ok(notices)) => shared.publish(notices),
                other => tracing::error!(run = %plan.run_id, ?other, "could not release the run after its start failed"),
            }
            return;
        }
    }
    shared.live.run_started(&job.id, plan.run_id);
    let (s, run_id) = (shared.clone(), plan.run_id);
    if blocking(move || s.service.run_cancelling(run_id)).await == Some(true) {
        handle.canceller().cancel();
    }
    if let Ok(mut map) = shared.run_inputs.lock() {
        map.insert(plan.run_id, handle.inputs.clone());
    }

    let mut text = String::new();
    let mut activity: Vec<aurelius_protocol::ActivityView> = Vec::new();
    let mut last_checkpoint = tokio::time::Instant::now();
    let mut last_renew = tokio::time::Instant::now();
    let mut quota: Option<Option<i64>> = None;
    // The watchdog: a run that shows no progress at all (no output, no tool use)
    // for the idle limit is stopped with a note, so its conversation goes on.
    let mut open_tools: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut last_progress = tokio::time::Instant::now();
    let outcome = loop {
        let limit = if open_tools.is_empty() { idle } else { idle * TOOL_IDLE_FACTOR };
        let next = tokio::select! {
            event = handle.events.recv() => Some(event),
            _ = tokio::time::sleep_until(last_progress + limit) => None,
        };
        let Some(next) = next else {
            // Stopped by the user, but its runtime does not end: it ends as stopped, not stalled.
            let (s, run_id) = (shared.clone(), plan.run_id);
            if blocking(move || s.service.run_cancelling(run_id)).await == Some(true) {
                tracing::warn!(run = %plan.run_id, "a stopped run's runtime did not end; ending it as stopped");
                stop(&mut handle, idle).await;
                break RunOutcome::Interrupted;
            }
            // Waiting on the user's approval is not hanging; in doubt, neither.
            let (s, run_id) = (shared.clone(), plan.run_id);
            match blocking(move || s.service.run_waits_on_others(run_id)).await {
                Some(false) => {}
                waits => {
                    if waits.is_none() {
                        tracing::warn!(run = %plan.run_id, "could not tell whether the run waits on others; not stopping it");
                    }
                    last_progress = tokio::time::Instant::now();
                    continue;
                }
            }
            tracing::warn!(run = %plan.run_id, idle_s = limit.as_secs(), open_tools = open_tools.len(), "no progress from the runtime; stopping the run");
            stop(&mut handle, idle).await;
            text = aurelius_application::run::lifecycle::with_note(&text, &aurelius_application::run::lifecycle::stalled_note(limit.as_secs().div_ceil(60) as i64));
            break RunOutcome::Failed("stalled".into());
        };
        last_progress = tokio::time::Instant::now();
        let Some(event) = next else {
            // Stream ended without a terminal event: never success.
            break RunOutcome::Failed("stream_incomplete".into());
        };
        if last_renew.elapsed() >= RENEW_EVERY {
            last_renew = tokio::time::Instant::now();
            let store = shared.store().clone();
            let id = job.id.clone();
            let renewed = blocking(move || store.write(move |tx| queries::renew_lease(tx, &id, epoch, LEASE_MS))).await;
            if !matches!(renewed, Some(Ok(true))) {
                tracing::warn!(run = %plan.run_id, "lease lost; stopping without further writes");
                handle.canceller().cancel();
                return;
            }
        }
        match event {
            RuntimeEvent::Started => {}
            RuntimeEvent::Sent(sent) => {
                if traced {
                    let (s, run_id) = (shared.clone(), plan.run_id);
                    let _ = blocking(move || s.service.record_trace_sent(run_id, &sent)).await;
                }
            }
            RuntimeEvent::Usage(usage) => {
                // How big the session's context is now: the next run compacts it first when too big.
                if let Some(size) = aurelius_application::run::session::context_size(&usage) {
                    plan.session.context_tokens = Some(size);
                }
                let (s, run_id) = (shared.clone(), plan.run_id);
                if let Some(Ok(notices)) = blocking(move || s.service.record_usage(run_id, &usage)).await {
                    shared.publish(notices);
                }
            }
            RuntimeEvent::ModelReported(model) => {
                let (s, run_id) = (shared.clone(), plan.run_id);
                let _ = blocking(move || s.service.record_run_model(run_id, epoch, &model)).await;
            }
            RuntimeEvent::Progress { output_tokens } => {
                shared.live.tokens(plan.run_id, output_tokens);
                stream(&shared, &plan, StreamDelta {
                    run_id: plan.run_id,
                    conversation_id: plan.conversation_id,
                    message_id: plan.message_id,
                    text: text.clone(),
                    activity: activity.clone(),
                    progress: progress(&shared, plan.run_id),
                    activity_changes: false,
                    mode: None,
                });
            }
            RuntimeEvent::TextDelta(delta) => {
                text.push_str(&delta);
                // Runtimes without a token count: estimate from the text.
                shared.live.tokens(plan.run_id, (text.len() / 4) as u64);
                if let Ok(mut live) = shared.live_text.lock() {
                    live.insert(plan.run_id, text.clone());
                }
                stream(&shared, &plan, StreamDelta {
                    run_id: plan.run_id,
                    conversation_id: plan.conversation_id,
                    message_id: plan.message_id,
                    text: text.clone(),
                    activity: activity.clone(),
                    progress: progress(&shared, plan.run_id),
                    activity_changes: false,
                    mode: None,
                });
                if last_checkpoint.elapsed() >= CHECKPOINT_EVERY {
                    last_checkpoint = tokio::time::Instant::now();
                    let (s, p, t) = (shared.clone(), plan.clone(), text.clone());
                    if let Some(Ok(notices)) = blocking(move || s.service.checkpoint_chat_run(&p, &t, epoch)).await {
                        shared.publish(notices);
                    }
                }
            }
            RuntimeEvent::Activity(event) => {
                // The runtime used one of its own tools: keep it with the answer.
                let (s, p) = (shared.clone(), plan.clone());
                match event {
                    Activity::Started { call_id, tool, input, parent } => {
                        open_tools.insert(call_id.clone());
                        let offset = text.len() as u32;
                        // Live updates carry tool use shortened; `activity.get` has it whole.
                        activity.push(aurelius_domain::activity::preview(aurelius_protocol::ActivityView {
                            call_id: call_id.clone(),
                            tool: tool.clone(),
                            input: input.clone(),
                            output: None,
                            ok: None,
                            offset,
                            truncated: false,
                            output_bytes: None,
                        }));
                        let _ = blocking(move || s.service.record_activity_started(&p, &call_id, &tool, &input, parent.as_deref(), offset)).await;
                    }
                    Activity::Finished { call_id, ok, output } => {
                        open_tools.remove(&call_id);
                        if let Some(a) = activity.iter_mut().find(|a| a.call_id == call_id) {
                            a.ok = Some(ok);
                            a.output = Some(output.clone());
                            *a = aurelius_domain::activity::preview(a.clone());
                        }
                        let _ = blocking(move || s.service.record_activity_finished(&p, &call_id, ok, &output)).await;
                    }
                }
                stream(&shared, &plan, StreamDelta {
                    run_id: plan.run_id,
                    conversation_id: plan.conversation_id,
                    message_id: plan.message_id,
                    text: text.clone(),
                    activity: activity.clone(),
                    progress: progress(&shared, plan.run_id),
                    activity_changes: false,
                    mode: None,
                });
            }
            RuntimeEvent::ArtifactPublished { url, title, version, path } => {
                // Kept by URL, so later runs republish to the same page; the
                // published file is kept per version, so clients can show it.
                let (s, conversation) = (shared.clone(), plan.conversation_id);
                let folders = plan.work.as_ref().map(|(f, _)| f.clone()).unwrap_or_default();
                match blocking(move || {
                    let file = path.and_then(|p| aurelius_application::run::artifacts::read_published_file(&p, &folders));
                    let published = aurelius_storage::artifacts::Published { url, title, version, file };
                    s.service.record_artifact(conversation, &published)
                })
                .await
                {
                    Some(Ok(notices)) => shared.publish(notices),
                    other => tracing::warn!(run = %plan.run_id, ?other, "could not keep a published artifact"),
                }
            }
            // Claude Code compacted on its own while it answered: the next run measures again.
            RuntimeEvent::Compacted { pre_tokens, post_tokens } => {
                tracing::info!(run = %plan.run_id, pre_tokens, post_tokens, "the runtime compacted the session while it answered");
            }
            RuntimeEvent::Completed => break RunOutcome::Completed,
            RuntimeEvent::Interrupted => break RunOutcome::Interrupted,
            RuntimeEvent::Quota { reset_at_ms } => {
                quota = Some(reset_at_ms);
                break RunOutcome::Failed("rate_limit".into());
            }
            RuntimeEvent::Failed { code, message } => {
                if text.is_empty() {
                    // Keep the reason visible in the conversation.
                    text = message;
                }
                break RunOutcome::Failed(code);
            }
        }
    };
    if let Some(reset_at_ms) = quota {
        park_for_quota(&shared, &plan, epoch, reset_at_ms).await;
    } else {
        finish(&shared, &plan, outcome, text, epoch).await;
        // A conversation deleted during this answer keeps no session.
        if plan.session.claimed.is_some() {
            let (s, conversation) = (shared.clone(), plan.conversation_id);
            if let Some(Ok(Some(id))) = blocking(move || s.service.close_session_if_over(conversation)).await
                && shared.owns_session(&id)
            {
                runtime.remove_session(&id);
            }
        }
    }
    // A chat job is one answer: its clock stops as its driver ends.
    drop(driver);
}

/// How long a run may go without progress: `AURELIUS_RUN_IDLE_MS`, else the setting.
async fn idle_limit(shared: &Arc<Shared>) -> Duration {
    let ms = match shared.run_idle_ms {
        Some(ms) => ms,
        None => {
            let s = shared.clone();
            blocking(move || s.service.run_idle_limit_ms()).await.unwrap_or(aurelius_application::run::lifecycle::DEFAULT_RUN_IDLE_MS)
        }
    };
    Duration::from_millis(ms.max(1) as u64)
}

/// Stop the runtime's turn and give it a moment to end its stream (a runtime that
/// hangs does not get to keep the run).
async fn stop(handle: &mut aurelius_runtime::RunHandle, grace: Duration) {
    handle.canceller().cancel();
    let _ = tokio::time::timeout(grace.min(STOP_GRACE), async {
        while let Some(event) = handle.events.recv().await {
            if matches!(event, RuntimeEvent::Completed | RuntimeEvent::Interrupted | RuntimeEvent::Failed { .. } | RuntimeEvent::Quota { .. }) {
                break;
            }
        }
    })
    .await;
}

/// Added to the prompt of a run that waited for the usage limit.
const QUOTA_RESUME_NOTE: &str = "(Your earlier attempt at this was interrupted by a Claude usage limit. Whatever it already did is still in place: check the work folder and carry on where it stopped instead of starting over.)";

/// The usage limit is reached: the run is not failed but parked until the reset (or
/// a default delay), its task waits with it, and the provider's other queued runs
/// wait too. Never uses up an attempt; a restart finds the parked job in the database.
async fn park_for_quota(shared: &Arc<Shared>, plan: &ChatRunPlan, epoch: i64, reset_at_ms: Option<i64>) {
    let now = aurelius_storage::now_ms();
    let at = aurelius_domain::scheduler::quota_wake_ms(reset_at_ms, now);
    let disposition = aurelius_domain::scheduler::dispose(&aurelius_domain::scheduler::Failure::Quota { reset_at_ms: Some(at) }, 1, 1, now, shared.backoff_base_ms, &plan.run_id.to_string());
    tracing::warn!(run = %plan.run_id, wake_at_ms = at, known_reset = reset_at_ms.is_some(), "usage limit reached; the run waits");
    let (s, p) = (shared.clone(), plan.clone());
    match blocking(move || s.service.defer_chat_run(&p, epoch, disposition)).await {
        Some(Ok(notices)) => shared.publish(notices),
        other => tracing::error!(run = %plan.run_id, ?other, "could not park the run for the usage limit"),
    }
    let s = shared.clone();
    if let Some(Ok(notices)) = blocking(move || s.service.park_runs_for_quota(at)).await {
        shared.publish(notices);
    }
}

/// Since when the run's job runs and its tokens so far.
/// A stream update without text, so clients see the run's phase change at once.
fn send_progress(shared: &Shared, plan: &ChatRunPlan) {
    stream(shared, plan, StreamDelta {
        run_id: plan.run_id,
        conversation_id: plan.conversation_id,
        message_id: plan.message_id,
        text: String::new(),
        activity: Vec::new(),
        progress: progress(shared, plan.run_id),
        activity_changes: false,
        mode: None,
    });
}

/// Send a stream update; a run the group chat woke streams there too, under the
/// id its answer gets in the group (`GroupRun::message_id`).
fn stream(shared: &Shared, plan: &ChatRunPlan, delta: StreamDelta) {
    if let Some(group) = plan.group {
        let _ = shared.streams.send(StreamDelta { conversation_id: group.conversation_id, message_id: group.message_id, mode: group.mode, ..delta.clone() });
    }
    let _ = shared.streams.send(delta);
}

fn progress(shared: &Shared, run_id: aurelius_domain::RunId) -> Option<aurelius_protocol::RunProgress> {
    shared.live.for_run(run_id).map(|(started_at_ms, output_tokens)| aurelius_protocol::RunProgress { started_at_ms, output_tokens, compacting: shared.live.run_compacting(run_id) })
}

pub(super) fn forget_run(shared: &Shared, run_id: aurelius_domain::RunId) {
    if let Ok(mut map) = shared.cancellers.lock() {
        map.remove(&run_id);
    }
    if let Ok(mut map) = shared.run_inputs.lock() {
        map.remove(&run_id);
    }
    if let Ok(mut map) = shared.run_modes.lock() {
        map.remove(&run_id);
    }
    if let Ok(mut map) = shared.run_switches.lock() {
        map.remove(&run_id);
    }
    if let Ok(mut live) = shared.live_text.lock() {
        live.remove(&run_id);
    }
}

async fn finish(shared: &Arc<Shared>, plan: &ChatRunPlan, outcome: RunOutcome, text: String, epoch: i64) {
    if let Ok(mut map) = shared.cancellers.lock() {
        map.remove(&plan.run_id);
    }
    if let Ok(mut map) = shared.run_inputs.lock() {
        map.remove(&plan.run_id);
    }
    if let Ok(mut map) = shared.run_modes.lock() {
        map.remove(&plan.run_id);
    }
    if let Ok(mut map) = shared.run_switches.lock() {
        map.remove(&plan.run_id);
    }
    let (s, p) = (shared.clone(), plan.clone());
    match blocking(move || s.service.finish_chat_run(&p, outcome, &text, epoch)).await {
        Some(Ok(notices)) => shared.publish(notices),
        Some(Err(err)) => tracing::error!(run = %plan.run_id, error = %err, "could not commit run outcome"),
        None => {}
    }
    if let Ok(mut live) = shared.live_text.lock() {
        live.remove(&plan.run_id);
    }
}
