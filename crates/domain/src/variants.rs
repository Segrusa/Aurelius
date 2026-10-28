//! Task variants (Harness² C, opt-in): a task may be tried in up to three
//! ways side by side before the real work. Propose: the cheap model gives k
//! short approach hints. Probe: each variant runs, one after the other, in a
//! throwaway git worktree of the work folder, sandboxed (`probe_verdict`).
//! Compare: a judge ranks them without ground truth. Compose: the user (or, in
//! Auto, the judge) picks one; its diff is applied to the real work folder and
//! the task goes on as usual.

use crate::{DomainError, TaskId};
use serde::{Deserialize, Serialize};

/// At most this many variants of a task.
pub const MAX_VARIANTS: u8 = 3;
/// One hint is at most this long.
pub const HINT_MAX_CHARS: usize = 160;
/// What the refusal of a probe says.
pub const PROBE_REFUSAL: &str = "probe: not allowed";

/// The approaches tried when the model gives none (or too few).
pub const DEFAULT_HINTS: [&str; 3] = [
    "Make a short plan first, then implement it step by step.",
    "Write or extend the tests first, then make them pass.",
    "Make the smallest change that does the job; touch as few files as possible.",
];

/// `variants` of a task's run settings: 1 (none) to `MAX_VARIANTS`.
pub fn validate_count(n: u8) -> Result<u8, DomainError> {
    if (1..=MAX_VARIANTS).contains(&n) {
        Ok(n)
    } else {
        Err(DomainError::OutOfRange { field: "variants".into(), detail: format!("1 to {MAX_VARIANTS}") })
    }
}

/// Where a task's variants are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VariantPhase {
    /// The cheap model is thinking of approaches.
    Proposing,
    /// The variants run, one after the other.
    Probing,
    /// The judge compares them.
    Judging,
    /// Waiting for the user's choice (Needs you).
    Ready,
    /// The chosen diff goes into the work folder.
    Applying,
    /// Chosen and applied: the task went on as usual.
    Done,
    /// Stopped by the user; the work folder is as it was.
    Stopped,
    /// Something failed (see `error`); the work folder is as it was.
    Failed,
}

impl VariantPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            VariantPhase::Proposing => "proposing",
            VariantPhase::Probing => "probing",
            VariantPhase::Judging => "judging",
            VariantPhase::Ready => "ready",
            VariantPhase::Applying => "applying",
            VariantPhase::Done => "done",
            VariantPhase::Stopped => "stopped",
            VariantPhase::Failed => "failed",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "proposing" => Self::Proposing,
            "probing" => Self::Probing,
            "judging" => Self::Judging,
            "ready" => Self::Ready,
            "applying" => Self::Applying,
            "done" => Self::Done,
            "stopped" => Self::Stopped,
            "failed" => Self::Failed,
            _ => return None,
        })
    }

    /// Nothing of the variants runs or waits any more.
    pub fn is_over(self) -> bool {
        matches!(self, VariantPhase::Done | VariantPhase::Stopped | VariantPhase::Failed)
    }
}

/// One variant's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VariantStatus {
    Waiting,
    Running,
    /// Its run completed; the outcome is collected.
    Done,
    /// Its run failed.
    Failed,
    /// Stopped.
    Cancelled,
}

impl VariantStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            VariantStatus::Waiting => "waiting",
            VariantStatus::Running => "running",
            VariantStatus::Done => "done",
            VariantStatus::Failed => "failed",
            VariantStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "waiting" => Self::Waiting,
            "running" => Self::Running,
            "done" => Self::Done,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }
}

/// Pick a variant of a task that waits for the user's choice. `variant: None`
/// lets the judge's pick stand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChooseTaskVariant {
    pub task_id: TaskId,
    #[serde(default)]
    pub variant: Option<u8>,
}

/// What the propose step's model returned.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HintsOutput {
    #[serde(default)]
    pub hints: Vec<String>,
}

/// `k` distinct short hints: the model's first, cut and without doubles, then
/// the defaults for what is missing.
pub fn pick_hints(output: &HintsOutput, k: u8) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let candidates = output.hints.iter().map(|h| h.split_whitespace().collect::<Vec<_>>().join(" ")).chain(DEFAULT_HINTS.iter().map(|h| h.to_string()));
    for hint in candidates {
        if out.len() == k as usize {
            break;
        }
        let hint = if hint.chars().count() > HINT_MAX_CHARS { format!("{}…", hint.chars().take(HINT_MAX_CHARS - 1).collect::<String>().trim_end()) } else { hint };
        if hint.is_empty() || crate::memory::looks_secret(&hint) || out.iter().any(|o| crate::memory::normalize(o) == crate::memory::normalize(&hint)) {
            continue;
        }
        out.push(hint);
    }
    out
}

/// What the judge's model returned.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct JudgeOutput {
    /// Best first.
    #[serde(default)]
    pub ranking: Vec<JudgeRank>,
    /// Which parts of which variants belong together, if combining would be better.
    #[serde(default)]
    pub compose: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct JudgeRank {
    pub variant: u8,
    #[serde(default)]
    pub why: String,
}

/// The judge's verdict, checked: every variant ranked once.
#[derive(Debug, Clone, PartialEq)]
pub struct Judgement {
    /// Variant numbers, best first; all of `candidates`, each once.
    pub order: Vec<u8>,
    /// (variant, why), as the judge said it.
    pub notes: Vec<(u8, String)>,
    pub compose: Option<String>,
}

impl Judgement {
    pub fn pick(&self) -> u8 {
        self.order[0]
    }
}

/// Check the judge's output against the variants that can be chosen (those
/// with an outcome). Unknown or double numbers are ignored; variants the judge
/// left out come last, those with a diff before those without, then by number.
pub fn validate_judgement(output: &JudgeOutput, candidates: &[(u8, bool)]) -> Option<Judgement> {
    if candidates.is_empty() {
        return None;
    }
    let mut order: Vec<u8> = Vec::new();
    let mut notes = Vec::new();
    for r in &output.ranking {
        if candidates.iter().any(|(n, _)| *n == r.variant) && !order.contains(&r.variant) {
            order.push(r.variant);
            notes.push((r.variant, r.why.trim().chars().take(600).collect()));
        }
    }
    let mut rest: Vec<(u8, bool)> = candidates.iter().copied().filter(|(n, _)| !order.contains(n)).collect();
    rest.sort_by_key(|(n, changed)| (!changed, *n));
    order.extend(rest.into_iter().map(|(n, _)| n));
    let compose = output.compose.as_deref().map(str::trim).filter(|c| !c.is_empty()).map(|c| c.chars().take(1_000).collect());
    Some(Judgement { order, notes, compose })
}

/// The sandbox of a probe run: what it may do. Probes run unattended in a
/// throwaway worktree, so nothing leaves it: reading anywhere and changing
/// files inside the worktree is fine; writing elsewhere, MCP tools, artifacts,
/// pushing, delegating or any other outside action is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeVerdict {
    Allow,
    Deny(String),
}

/// Aurelius' own tools a probe may use: only those that read.
const OWN_READ_TOOLS: [&str; 5] = ["mcp__aurelius__recall", "mcp__aurelius__list_tasks", "mcp__aurelius__read_task", "mcp__aurelius__list_conversations", "mcp__aurelius__read_conversation"];

/// Commands that reach outside the worktree (network, remote, publishing).
const OUTSIDE_COMMANDS: [&str; 22] = [
    "git push", "git remote", "git fetch", "git pull", "git clone", "git worktree", "git -c", "gh ", "curl", "wget", "invoke-webrequest", "invoke-restmethod",
    "ssh ", "scp ", "sftp", "rsync", "npm publish", "cargo publish", "docker push", "start-process", "shutdown", "rm -rf /",
];

fn normalized_path(p: &str) -> String {
    let mut s = p.trim().trim_matches('"').trim_matches('\'').replace('\\', "/").to_lowercase();
    while s.contains("//") {
        s = s.replace("//", "/");
    }
    s.trim_end_matches('/').to_owned()
}

/// `path` lies inside `root` (both as given; `..` is never inside).
pub fn inside(root: &str, path: &str) -> bool {
    let (root, path) = (normalized_path(root), normalized_path(path));
    if path.split('/').any(|part| part == "..") {
        return false;
    }
    path == root || path.starts_with(&format!("{root}/"))
}

fn is_absolute(word: &str) -> bool {
    let w = word.trim_matches(|c| c == '"' || c == '\'' || c == '(' || c == ')' || c == ';' || c == ',');
    let b = w.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')) || w.starts_with("\\\\") || (w.starts_with('/') && w.len() > 1 && !w.starts_with("//"))
}

/// May a probe in `worktree` use `tool` with `input` (Claude Code's tool name and input)?
pub fn probe_verdict(tool: &str, input: &serde_json::Value, worktree: &str) -> ProbeVerdict {
    let deny = |why: &str| ProbeVerdict::Deny(format!("{PROBE_REFUSAL}: {why}. This is a trial run in a separate copy of the work folder; stay inside it and leave outside actions for after the user chose."));
    let tool = tool.strip_prefix("claude.").unwrap_or(tool);
    if tool.starts_with("mcp__") {
        if OWN_READ_TOOLS.contains(&tool) {
            return ProbeVerdict::Allow;
        }
        return deny("MCP tools, delegation and other outside actions");
    }
    match tool {
        "Read" | "Glob" | "Grep" | "LS" | "TodoWrite" | "WebSearch" | "WebFetch" | "Task" | "Agent" | "BashOutput" | "KillShell" | "ToolSearch" => ProbeVerdict::Allow,
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            let path = input["file_path"].as_str().or_else(|| input["notebook_path"].as_str()).unwrap_or("");
            if !path.is_empty() && inside(worktree, path) {
                ProbeVerdict::Allow
            } else {
                deny("writing outside this trial's folder")
            }
        }
        "Bash" | "PowerShell" => {
            let command = input["command"].as_str().unwrap_or("");
            let lower = command.to_lowercase();
            if let Some(hit) = OUTSIDE_COMMANDS.iter().find(|c| lower.contains(*c)) {
                return deny(&format!("an outside action ({})", hit.trim()));
            }
            if command.split_whitespace().filter(|w| is_absolute(w)).any(|w| !inside(worktree, w.trim_matches(|c| c == '"' || c == '\'' || c == '(' || c == ')' || c == ';' || c == ','))) {
                return deny("a path outside this trial's folder");
            }
            ProbeVerdict::Allow
        }
        "AskUserQuestion" | "ExitPlanMode" => deny("nobody answers questions or plans during a trial; decide yourself"),
        "Artifact" | "ArtifactData" | "ArtifactComments" => deny("publishing artifacts"),
        _ => deny("this tool"),
    }
}

/// The tool outcome of one probe, as the judge sees it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProbeChecks {
    /// Tool calls that failed or were refused.
    pub errors: u32,
    /// The last test or build output seen (cut), if any.
    #[serde(default)]
    pub tests: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const WT: &str = "C:\\repo\\target\\wt-probe-ab12-1";

    #[test]
    fn a_probe_writes_only_in_its_worktree_and_does_nothing_outside() {
        let allow = |tool: &str, input: serde_json::Value| probe_verdict(tool, &input, WT) == ProbeVerdict::Allow;
        assert!(allow("Read", json!({ "file_path": "C:\\elsewhere\\x.rs" })), "reading is fine");
        assert!(allow("Edit", json!({ "file_path": "C:/repo/target/wt-probe-ab12-1/src/lib.rs" })));
        assert!(allow("claude.Write", json!({ "file_path": "c:\\REPO\\target\\wt-probe-ab12-1\\new.txt" })), "case and slashes do not matter");
        assert!(!allow("Write", json!({ "file_path": "C:\\repo\\src\\lib.rs" })), "the real work folder is outside");
        assert!(!allow("Edit", json!({ "file_path": "C:\\repo\\target\\wt-probe-ab12-1\\..\\..\\src\\lib.rs" })), "no way out with ..");
        assert!(!allow("Write", json!({ "file_path": "C:\\repo\\target\\wt-probe-ab12-10\\x" })), "a sibling with the same prefix");
        assert!(allow("Bash", json!({ "command": "cargo test --manifest-path C:\\repo\\target\\wt-probe-ab12-1\\Cargo.toml" })));
        assert!(allow("Bash", json!({ "command": "git status && git diff" })));
        assert!(!allow("Bash", json!({ "command": "git push origin main" })));
        assert!(!allow("Bash", json!({ "command": "curl https://example.com" })));
        assert!(!allow("Bash", json!({ "command": "echo x > C:\\Windows\\x.txt" })));
        assert!(!allow("mcp__notes-app__create_note", json!({})), "the user's MCP servers");
        assert!(!allow("mcp__booking-app__confirm_booking", json!({})), "bookings");
        assert!(!allow("mcp__aurelius__delegate_task", json!({})), "delegation");
        assert!(!allow("mcp__aurelius__remember", json!({})), "no memory writes from a trial");
        assert!(allow("mcp__aurelius__recall", json!({})));
        assert!(!allow("Artifact", json!({ "action": "publish" })));
        assert!(!allow("AskUserQuestion", json!({})));
        let ProbeVerdict::Deny(why) = probe_verdict("Bash", &json!({ "command": "git push" }), WT) else { panic!() };
        assert!(why.starts_with(PROBE_REFUSAL), "{why}");
    }

    #[test]
    fn hints_are_short_distinct_and_filled_up() {
        let out = HintsOutput { hints: vec!["  Plan   first ".into(), "plan first".into(), "x".repeat(400), "".into()] };
        let hints = pick_hints(&out, 3);
        assert_eq!(hints.len(), 3);
        assert_eq!(hints[0], "Plan first");
        assert_eq!(hints[1].chars().count(), HINT_MAX_CHARS);
        assert_eq!(hints[2], DEFAULT_HINTS[0]);
        assert_eq!(pick_hints(&HintsOutput::default(), 2), DEFAULT_HINTS[..2].to_vec());
        assert!(validate_count(0).is_err() && validate_count(4).is_err() && validate_count(3).is_ok());
    }

    #[test]
    fn the_judgement_ranks_every_candidate_once() {
        let out: JudgeOutput = serde_json::from_value(json!({ "ranking": [{ "variant": 2, "why": "tests pass" }, { "variant": 9 }, { "variant": 2 }], "compose": "take the tests of 1" })).unwrap();
        let j = validate_judgement(&out, &[(1, true), (2, true), (3, false)]).unwrap();
        assert_eq!(j.order, vec![2, 1, 3], "unranked ones after, those with a change first");
        assert_eq!(j.pick(), 2);
        assert_eq!(j.notes, vec![(2, "tests pass".to_string())]);
        assert_eq!(j.compose.as_deref(), Some("take the tests of 1"));
        assert_eq!(validate_judgement(&JudgeOutput::default(), &[(1, false), (2, true)]).unwrap().order, vec![2, 1]);
        assert!(validate_judgement(&out, &[]).is_none());
        for p in [VariantPhase::Proposing, VariantPhase::Ready, VariantPhase::Done, VariantPhase::Stopped] {
            assert_eq!(VariantPhase::parse(p.as_str()), Some(p));
        }
    }
}
