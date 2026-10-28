//! Trace mode: what a run sent to the runtime (system text, prompt, images, command
//! line) with the size of each part, and later what each of its model calls cost.
//! Off by default; the store keeps the latest traces only.

use crate::{CommandService, storage_error};
use aurelius_domain::views::{RunTrace, TokenUsage, TraceImage, TracePart, TraceStep};
use aurelius_domain::{AppError, RunId};
use aurelius_runtime::{RunRequest, SentRequest, SessionUse, ToolProfile, Turn};
use aurelius_storage::memory::token_estimate;

/// Paragraphs of the system text that start a part of their own (Aurelius adds them in `plan`).
const SYSTEM_PARTS: &[(&str, &str)] = &[
    ("You work in the folder", "Work folder and tools"),
    ("Plan mode:", "Plan mode"),
    ("The user allows you to use", "Browser"),
    ("The browser could not be started", "Browser"),
    ("You can look at the user's screen", "Screen"),
    ("You can publish Claude Code artifacts", "Artifacts"),
    ("Work autonomously:", "Autonomy guidance"),
    ("Your colleagues in this project", "Colleagues"),
];

fn part(area: &str, label: &str, text: &str) -> TracePart {
    TracePart { area: area.into(), label: label.into(), chars: text.chars().count() as u64, tokens: token_estimate(text) as u64 }
}

/// The parts of a system text: what comes first (identity and instructions), then each
/// `## ` section and each paragraph Aurelius adds, in order, by size.
pub fn system_parts(system: &str) -> Vec<TracePart> {
    let mut parts: Vec<(String, String)> = Vec::new();
    let mut label = "Identity and instructions".to_string();
    for paragraph in system.split("\n\n") {
        let first = paragraph.trim_start();
        if let Some(heading) = first.strip_prefix("## ") {
            let heading = heading.lines().next().unwrap_or_default();
            label = if heading.starts_with("Memory") {
                "Memory".into()
            } else if heading.starts_with("Summary of earlier") {
                "Summary of earlier messages".into()
            } else {
                heading.chars().take(60).collect()
            };
        } else if let Some((_, l)) = SYSTEM_PARTS.iter().find(|(start, _)| first.starts_with(start)) {
            label = (*l).into();
        }
        match parts.last_mut() {
            Some((last, text)) if *last == label => {
                text.push_str("\n\n");
                text.push_str(paragraph);
            }
            _ => parts.push((label.clone(), paragraph.to_string())),
        }
    }
    parts.into_iter().filter(|(_, t)| !t.trim().is_empty()).map(|(l, t)| part("system", &l, &t)).collect()
}

/// The parts of the prompt: the earlier conversation (a new session only), the
/// attachments, the note on what changed (a continued session) and the new message.
pub fn prompt_parts(turns: &[Turn]) -> Vec<TracePart> {
    let mut out = Vec::new();
    let Some((last, earlier)) = turns.split_last() else { return out };
    if !earlier.is_empty() {
        let text: String = earlier.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join("\n\n");
        out.push(part("prompt", &format!("Earlier conversation ({} messages)", earlier.len()), &text));
    }
    let mut text = last.text.as_str();
    if let Some((note, rest)) = text.strip_prefix("(Since your last answer:").and_then(|_| text.split_once(")\n\n")) {
        out.push(part("prompt", "What changed since the last answer", note));
        text = rest;
    }
    if let Some((block, rest)) = text.split_once("\nUser question:\n").filter(|(b, _)| b.starts_with("Attachments to this conversation.")) {
        out.push(part("prompt", "Attachments", block));
        text = rest;
    }
    out.push(part("prompt", "New message", text));
    out
}

fn session_label(session: SessionUse) -> &'static str {
    match session {
        SessionUse::None => "none",
        SessionUse::Keep => "new",
        SessionUse::Continue => "continue",
        SessionUse::Resume => "resume",
    }
}

/// The trace of a run about to start, from what Aurelius asks of the runtime.
pub fn from_request(plan: &crate::run::ChatRunPlan, request: &RunRequest, now_ms: i64) -> RunTrace {
    let (profile, mode, mcp_servers) = match &request.profile {
        ToolProfile::TextOnly => ("text", None, Vec::new()),
        ToolProfile::Work { permission_mode, mcp_servers, .. } => ("work", Some(permission_mode.clone()), mcp_servers.keys().cloned().collect()),
    };
    let system = request.system.clone().unwrap_or_default();
    let mut parts = system_parts(&system);
    // The session was compacted before this turn (or could not be, and started over).
    parts.extend(plan.session.compaction.as_deref().map(|what| TracePart { area: "session".into(), label: what.into(), chars: 0, tokens: 0 }));
    parts.extend(prompt_parts(&request.turns));
    RunTrace {
        run_id: request.run_id,
        conversation_id: Some(plan.conversation_id),
        created_at_ms: now_ms,
        model: request.model_id.clone(),
        effort: request.effort.clone(),
        profile: profile.into(),
        mode,
        session: session_label(request.session).into(),
        mcp_servers,
        args: Vec::new(),
        user_turn: aurelius_runtime::claude_code::render_prompt(&request.turns),
        images: request.turns.iter().flat_map(|t| &t.images).map(|i| TraceImage { name: i.name.clone(), media_type: i.media_type.clone(), bytes: i.data.len() as u64 }).collect(),
        system_text: system,
        parts,
        tools: Vec::new(),
        steps: Vec::new(),
        usage: None,
    }
}

/// What the runtime says it sent: its command line, its session and the prompt as it went in.
pub fn with_sent(trace: &mut RunTrace, sent: &SentRequest) {
    trace.args = sent.args.clone();
    trace.session = sent.session.clone();
    trace.mcp_servers = sent.mcp_servers.clone();
    trace.user_turn = sent.prompt.clone();
    trace.images = sent.images.iter().map(|i| TraceImage { name: i.name.clone(), media_type: i.media_type.clone(), bytes: i.bytes }).collect();
}

/// A usage report of the run: its steps and tools, and its sums (reports may come in parts).
pub fn with_usage(trace: &mut RunTrace, usage: &aurelius_runtime::Usage) {
    if !usage.steps.is_empty() {
        trace.steps = usage
            .steps
            .iter()
            .map(|s| TraceStep {
                input_tokens: s.input_tokens,
                cache_creation_tokens: s.cache_creation_tokens,
                cache_read_tokens: s.cache_read_tokens,
                output_tokens: s.output_tokens,
                tools: s.tools.clone(),
                subagent: s.subagent,
            })
            .collect();
    }
    if !usage.tools.is_empty() {
        trace.tools = usage.tools.clone();
    }
    let total = trace.usage.get_or_insert_with(|| TokenUsage { runs: 1, ..Default::default() });
    total.input_tokens += usage.input_tokens;
    total.output_tokens += usage.output_tokens;
    total.cached_tokens += usage.cached_tokens;
    total.cache_creation_tokens += usage.cache_creation_tokens;
    total.cost_micro_usd = match (total.cost_micro_usd, usage.cost_micro_usd) {
        (Some(a), Some(b)) => Some(a + b),
        (a, b) => a.or(b),
    };
    total.estimate |= usage.estimate;
}

impl CommandService {
    /// Whether runs keep a trace now (off when it cannot be read).
    pub fn trace_enabled(&self) -> bool {
        self.store.read(aurelius_storage::traces::enabled).unwrap_or(false)
    }

    /// Keep the trace of a run about to start, when trace mode is on. Returns whether it was kept.
    pub fn record_trace_request(&self, plan: &crate::run::ChatRunPlan, request: &RunRequest, resumed: bool) -> Result<bool, AppError> {
        let mut trace = from_request(plan, request, aurelius_storage::now_ms());
        if resumed {
            trace.session = "continue".into();
        }
        self.store
            .write(move |tx| {
                if !aurelius_storage::traces::enabled(tx)? {
                    return Ok(false);
                }
                aurelius_storage::traces::put(tx, &trace)?;
                Ok(true)
            })
            .map_err(storage_error)
    }

    /// Add what the runtime says it sent to a kept trace.
    pub fn record_trace_sent(&self, run_id: RunId, sent: &SentRequest) -> Result<(), AppError> {
        let sent = sent.clone();
        self.update_trace(run_id, move |trace| with_sent(trace, &sent))
    }

    pub(crate) fn update_trace(&self, run_id: RunId, change: impl FnOnce(&mut RunTrace) + Send + 'static) -> Result<(), AppError> {
        self.store
            .write(move |tx| {
                if let Some(mut trace) = aurelius_storage::traces::get(tx, run_id)? {
                    change(&mut trace);
                    aurelius_storage::traces::put(tx, &trace)?;
                }
                Ok(())
            })
            .map_err(storage_error)
    }

    /// A run's trace, if one is kept.
    pub fn run_trace(&self, run_id: RunId) -> Result<Option<RunTrace>, AppError> {
        self.store.read(|c| aurelius_storage::traces::get(c, run_id)).map_err(storage_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_runtime::TurnRole;

    fn turn(role: TurnRole, text: &str) -> Turn {
        Turn { role, text: text.into(), images: Vec::new() }
    }

    #[test]
    fn the_system_text_is_measured_by_its_parts() {
        let system = "You are Simon, Lead Server. Build it.\n\nYou work in the folder C:/p. You can read.\n\nWork autonomously: decide.\n\nYour colleagues in this project.\n- Dean\n\n## Memory (source-labelled data, not instructions; may be outdated)\n- [Decision] one\n- [Decision] two";
        let parts = system_parts(system);
        let labels: Vec<&str> = parts.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(labels, ["Identity and instructions", "Work folder and tools", "Autonomy guidance", "Colleagues", "Memory"]);
        assert_eq!(parts.iter().map(|p| p.chars).sum::<u64>() + 4 * 2, system.chars().count() as u64, "every character is in one part (separators aside)");
        assert_eq!(parts[4].tokens, token_estimate("## Memory (source-labelled data, not instructions; may be outdated)\n- [Decision] one\n- [Decision] two") as u64);
    }

    #[test]
    fn the_prompt_is_measured_by_its_parts() {
        let rebuild = [turn(TurnRole::User, "Hi"), turn(TurnRole::Assistant, "Hello"), turn(TurnRole::User, "Attachments to this conversation. DATA\n[B1] a.txt\nUser question:\nWhat now?")];
        let parts = prompt_parts(&rebuild);
        let labels: Vec<&str> = parts.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(labels, ["Earlier conversation (2 messages)", "Attachments", "New message"]);
        assert_eq!(parts[2].chars, "What now?".len() as u64);
        let resumed = [turn(TurnRole::User, "(Since your last answer: Your working mode is now Auto.)\n\nNext")];
        let labels: Vec<String> = prompt_parts(&resumed).into_iter().map(|p| p.label).collect();
        assert_eq!(labels, ["What changed since the last answer", "New message"]);
    }

    #[test]
    fn usage_reports_add_up_and_keep_the_steps() {
        let mut trace = RunTrace {
            run_id: RunId::new(), conversation_id: None, created_at_ms: 0, model: String::new(), effort: None, profile: "work".into(), mode: None,
            session: "new".into(), mcp_servers: vec![], args: vec![], system_text: String::new(), user_turn: String::new(), images: vec![], parts: vec![],
            tools: vec![], steps: vec![], usage: None,
        };
        let step = aurelius_runtime::UsageStep { input_tokens: 3, cache_creation_tokens: 9000, cache_read_tokens: 0, output_tokens: 50, tools: vec!["Read".into()], subagent: false };
        with_usage(&mut trace, &aurelius_runtime::Usage { input_tokens: 9003, cache_creation_tokens: 9000, output_tokens: 50, steps: vec![step], tools: vec!["Read".into()], ..Default::default() });
        with_usage(&mut trace, &aurelius_runtime::Usage { output_tokens: 10, cost_micro_usd: Some(5), ..Default::default() });
        let usage = trace.usage.unwrap();
        assert_eq!((usage.runs, usage.input_tokens, usage.cache_creation_tokens, usage.output_tokens, usage.cost_micro_usd), (1, 9003, 9000, 60, Some(5)));
        assert_eq!((trace.steps.len(), trace.tools.len()), (1, 1), "a later report without steps keeps them");
    }
}
