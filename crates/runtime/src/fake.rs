//! Deterministic fake runtime. Output depends only on the request, so tests
//! and demos are reproducible. No network, no credentials.

use crate::*;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeBehavior {
    Normal,
    /// Emits some text, then fails without terminal success (tests that
    /// a final error after deltas never counts as success).
    FailAfterDeltas,
}

#[derive(Clone)]
pub struct FakeRuntime {
    pub delay: Duration,
    pub behavior: FakeBehavior,
}

impl Default for FakeRuntime {
    fn default() -> Self {
        Self { delay: Duration::from_millis(35), behavior: FakeBehavior::Normal }
    }
}

pub const MODEL_ID: &str = "fake-deterministic-1";

/// Markers the task executor puts in its (trusted) system text, so the fake can
/// produce structured output deterministically.
pub const MEMORY_MARKER: &str = "[[aurelius:memory]]";
pub const SUMMARY_MARKER: &str = "[[aurelius:summary]]";
pub const CONSOLIDATE_MARKER: &str = "[[aurelius:consolidate]]";
pub const VARIANTS_MARKER: &str = "[[aurelius:variants]]";
pub const JUDGE_MARKER: &str = "[[aurelius:judge]]";

/// Deterministic structured output for the background steps (memory extraction,
/// consolidation, summaries), picked by a marker in the trusted system text.
fn scripted(request: &RunRequest) -> Option<String> {
    let system = request.system.as_deref()?;
    let prompt = request.turns.iter().rev().find(|t| t.role == TurnRole::User).map(|t| t.text.as_str()).unwrap_or("");
    if system.contains(MEMORY_MARKER) {
        // One episode about the outcome (employee scope) and one knowledge
        // proposal from the assignment (project scope).
        let mut items = Vec::new();
        // One lesson from the first signal, if there are signals (the gate found some).
        let mut lessons = Vec::new();
        if let Some((label, what)) = prompt.lines().filter_map(|l| l.split_once(": ")).find(|(label, _)| *label == "S1") {
            let gist: String = what.split_once(": ").map_or(what, |(_, rest)| rest).chars().take(120).collect();
            lessons.push(serde_json::json!({ "applies_when": "a task like this one", "lesson": format!("Remember this correction: {gist}"), "evidence": [label], "scope": "employee" }));
        }
        for line in prompt.lines() {
            let Some((label, what)) = line.split_once(": ") else { continue };
            if !label.starts_with('E') {
                continue;
            }
            if let Some(title) = what.strip_prefix("Task completed: ") {
                items.push(serde_json::json!({ "kind": "episodic", "content": format!("Task '{title}' was completed and approved after review."), "source": label, "scope": "employee", "importance": 0.5, "confidence": 0.9 }));
            }
            if let Some(rest) = what.strip_prefix("Assignment: ") {
                let goal = rest.split_once(" — ").map_or(rest, |(_, g)| g);
                // The goal is JSON-quoted (data, not instructions): its first line, as before.
                let decoded = serde_json::from_str::<String>(goal).ok();
                let goal = decoded.as_deref().map_or(goal, |g| g.lines().next().unwrap_or_default());
                items.push(serde_json::json!({ "kind": "project_knowledge", "content": format!("Requested goal: {goal}"), "source": label, "scope": "project", "importance": 0.6, "confidence": 0.8 }));
            }
        }
        let mut answer = serde_json::json!({ "items": items, "lessons": lessons });
        // After a ticket's Done the extraction also asks for the ticket's summary.
        if system.contains("\"ticket_summary\"") {
            let title = prompt.lines().find_map(|l| l.split_once("Task completed: ").map(|(_, t)| t)).unwrap_or("the ticket");
            answer["ticket_summary"] = serde_json::json!(format!("The main task '{title}' was delivered and accepted."));
        }
        return Some(answer.to_string());
    }
    if system.contains(VARIANTS_MARKER) {
        // "Count: k" asks for k approaches.
        let k = prompt.lines().find_map(|l| l.strip_prefix("Count: ")).and_then(|n| n.trim().parse::<usize>().ok()).unwrap_or(2);
        let hints: Vec<String> = (1..=k).map(|n| format!("Approach {n}: try it the test-runtime way number {n}")).collect();
        return Some(serde_json::json!({ "hints": hints }).to_string());
    }
    if system.contains(JUDGE_MARKER) {
        // Prefers the variant with the highest number, so tests can tell its pick from the first.
        let mut numbers: Vec<u8> = prompt.lines().filter_map(|l| l.strip_prefix("## Variant ")).filter_map(|r| r.split_whitespace().next()?.trim_end_matches(':').parse().ok()).collect();
        numbers.sort_unstable_by(|a, b| b.cmp(a));
        let ranking: Vec<serde_json::Value> = numbers.iter().map(|n| serde_json::json!({ "variant": n, "why": format!("variant {n} (test runtime ranking)") })).collect();
        return Some(serde_json::json!({ "ranking": ranking, "compose": null }).to_string());
    }
    if system.contains(CONSOLIDATE_MARKER) {
        // Merge the memories two by two ("M1 [Decision] text" lines).
        let items: Vec<(&str, &str)> = prompt
            .lines()
            .filter(|l| l.starts_with('M'))
            .filter_map(|l| {
                let (label, rest) = l.split_once(' ')?;
                Some((label, rest.split_once("] ").map_or(rest, |(_, t)| t)))
            })
            .collect();
        let points: Vec<serde_json::Value> = items
            .chunks(2)
            .map(|pair| {
                let text: String = pair[0].1.chars().take(200).collect();
                serde_json::json!({ "kind": "decision", "content": format!("{text} (merged)"), "sources": pair.iter().map(|p| p.0).collect::<Vec<_>>(), "importance": 0.6 })
            })
            .collect();
        return Some(serde_json::json!({ "points": points, "dropped": [] }).to_string());
    }
    if system.contains(SUMMARY_MARKER) {
        let lines: Vec<&str> = prompt.lines().filter(|l| l.starts_with('[')).collect();
        let last = lines.last().copied().unwrap_or("");
        return Some(format!("Summary (test runtime): {} messages discussed. Latest: {}", lines.len(), last.chars().take(120).collect::<String>()));
    }
    None
}

/// The full answer the fake gives for a request.
pub fn answer_for(request: &RunRequest) -> String {
    if let Some(text) = scripted(request) {
        return text;
    }
    let question = request.turns.iter().rev().find(|t| t.role == TurnRole::User).map(|t| t.text.trim()).unwrap_or("");
    let shown: String = question.chars().take(400).collect();
    let earlier = request.turns.len().saturating_sub(1);
    let persona = request
        .system
        .as_deref()
        .and_then(|s| s.strip_prefix("You are ").or_else(|| s.split_once("\n\nYou are ").map(|(_, rest)| rest)))
        .and_then(|s| s.split('.').next())
        .map(|who| format!("[{who}] "))
        .unwrap_or_default();
    format!(
        "{persona}This is an answer from the local test runtime, without an AI model. You asked: \u{201c}{shown}\u{201d}. \
         The conversation contains {earlier} earlier messages. Real answers come once a provider is connected \
         (Settings → Connections). Streaming, stop and retry already work for real."
    )
}

#[async_trait::async_trait]
impl AgentRuntime for FakeRuntime {
    async fn probe(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            runtime_kind: "fake".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            streaming: Capability::Supported,
            cancel: Capability::Supported,
            resume: Capability::Unsupported,
            tools: Capability::Unsupported,
            native_sandbox: Capability::Unsupported,
        }
    }

    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let (tx, rx) = mpsc::channel(EVENT_BUFFER);
        let (input_tx, _input_rx) = mpsc::channel(8);
        let (cancel_tx, mut cancel_rx) = watch::channel(false);
        let text = answer_for(&request);
        let input_chars = request.system.as_deref().map_or(0, |s| s.chars().count() as u64) + request.turns.iter().map(|t| t.text.chars().count() as u64).sum::<u64>();
        let delay = self.delay;
        let behavior = self.behavior;
        tokio::spawn(async move {
            if tx.send(RuntimeEvent::Started).await.is_err() {
                return;
            }
            let _ = tx.send(RuntimeEvent::ModelReported(MODEL_ID.into())).await;
            let words: Vec<&str> = text.split_inclusive(' ').collect();
            for (i, word) in words.iter().enumerate() {
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = cancel_rx.changed() => {
                        let _ = tx.send(RuntimeEvent::Interrupted).await;
                        return;
                    }
                }
                if behavior == FakeBehavior::FailAfterDeltas && i == words.len() / 2 {
                    let _ = tx.send(RuntimeEvent::Failed { code: "fake_failure".into(), message: "Injected failure".into() }).await;
                    return;
                }
                if tx.send(RuntimeEvent::TextDelta((*word).to_string())).await.is_err() {
                    return;
                }
            }
            // Local test runtime: token estimate (4 characters ≈ 1 token), no cost.
            let _ = tx.send(RuntimeEvent::Usage(Usage { input_tokens: input_chars.div_ceil(4), output_tokens: (text.chars().count() as u64).div_ceil(4), cached_tokens: 0, cost_micro_usd: Some(0), estimate: true, ..Default::default() })).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(text: &str) -> RunRequest {
        RunRequest { run_id: RunId::new(), model_id: MODEL_ID.into(), system: None, effort: None, profile: Default::default(), subagent_model: None, session: Default::default(), session_id: None, keep_session: false, turns: vec![Turn { role: TurnRole::User, text: text.into(), images: Vec::new() }] }
    }

    async fn collect(mut handle: RunHandle) -> (String, RuntimeEvent) {
        let mut text = String::new();
        while let Some(event) = handle.events.recv().await {
            match event {
                RuntimeEvent::TextDelta(d) => text.push_str(&d),
                RuntimeEvent::Started | RuntimeEvent::ModelReported(_) | RuntimeEvent::Usage(_) => {}
                terminal => return (text, terminal),
            }
        }
        panic!("stream ended without terminal event");
    }

    #[tokio::test]
    async fn deterministic_and_complete() {
        let fake = FakeRuntime { delay: Duration::ZERO, ..Default::default() };
        let (a, end) = collect(fake.start(request("hoi")).await.unwrap()).await;
        assert_eq!(end, RuntimeEvent::Completed);
        assert_eq!(a, answer_for(&request("hoi")));
    }

    #[tokio::test]
    async fn cancel_interrupts_only_its_run() {
        let fake = FakeRuntime { delay: Duration::from_millis(5), ..Default::default() };
        let first = fake.start(request("a")).await.unwrap();
        let second = fake.start(request("b")).await.unwrap();
        first.canceller().cancel();
        let (_, end_first) = collect(first).await;
        let (_, end_second) = collect(second).await;
        assert_eq!(end_first, RuntimeEvent::Interrupted);
        assert_eq!(end_second, RuntimeEvent::Completed);
    }

    #[tokio::test]
    async fn failure_after_deltas_is_not_success() {
        let fake = FakeRuntime { delay: Duration::ZERO, behavior: FakeBehavior::FailAfterDeltas };
        let (text, end) = collect(fake.start(request("x")).await.unwrap()).await;
        assert!(!text.is_empty());
        assert!(matches!(end, RuntimeEvent::Failed { .. }));
    }
}
