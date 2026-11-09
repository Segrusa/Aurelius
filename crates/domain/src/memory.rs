//! Scoped memory: types, statuses, user commands and the
//! pure validation of extracted candidates.

use crate::{DomainError, EmployeeId, MemoryId, ProjectId, require_text};
use serde::{Deserialize, Serialize};

/// An employee's memory tool call during a run (the bridge's `remember` and `recall`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EmployeeMemoryOp {
    /// Note something: kind decision | project_knowledge | episodic, scope employee | project.
    Remember { kind: String, scope: String, content: String },
    /// Search the memories this employee may read.
    Recall { query: String },
}

pub const MAX_MEMORY_CHARS: usize = 2_000;
/// What belongs in memory (C5): the words of the extraction prompt and of the
/// `remember` tool, so an employee and the extraction draw the same line.
pub const MEMORY_SCOPE_RULE: &str = "Memory is for what stays true after this work: how you do your job, conventions, and decisions and knowledge that also matter for later work. A fact that only holds for one ticket or task (its status, its branch or PR number, who is doing what, a value used once) belongs in that ticket's discussion, not in memory.";
/// 2: lessons from what went wrong in a task (signals S1..S8), see `validate_lessons`.
/// 3: the scope rule (C5), see `MEMORY_SCOPE_RULE`.
pub const EXTRACTION_VERSION: &str = "aurelius-extract-3";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Identity,
    ProjectKnowledge,
    Decision,
    Episodic,
    ConversationSummary,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryKind::Identity => "identity",
            MemoryKind::ProjectKnowledge => "project_knowledge",
            MemoryKind::Decision => "decision",
            MemoryKind::Episodic => "episodic",
            MemoryKind::ConversationSummary => "conversation_summary",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "identity" => MemoryKind::Identity,
            "project_knowledge" => MemoryKind::ProjectKnowledge,
            "decision" => MemoryKind::Decision,
            "episodic" => MemoryKind::Episodic,
            "conversation_summary" => MemoryKind::ConversationSummary,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            MemoryKind::Identity => "Identity",
            MemoryKind::ProjectKnowledge => "Knowledge",
            MemoryKind::Decision => "Decision",
            MemoryKind::Episodic => "Episode",
            MemoryKind::ConversationSummary => "Summary",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Proposed,
    Active,
    Superseded,
    Rejected,
    Forgotten,
}

impl MemoryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryStatus::Proposed => "proposed",
            MemoryStatus::Active => "active",
            MemoryStatus::Superseded => "superseded",
            MemoryStatus::Rejected => "rejected",
            MemoryStatus::Forgotten => "forgotten",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "proposed" => MemoryStatus::Proposed,
            "active" => MemoryStatus::Active,
            "superseded" => MemoryStatus::Superseded,
            "rejected" => MemoryStatus::Rejected,
            "forgotten" => MemoryStatus::Forgotten,
            _ => return None,
        })
    }
}

/// Where a memory lives. Global only holds what was explicitly made global.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum MemoryScope {
    Global,
    Project(ProjectId),
    Employee(EmployeeId),
}

/// The user writes a memory directly: active at once, the command is its source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Remember {
    pub scope: MemoryScope,
    pub kind: MemoryKind,
    pub content: String,
    #[serde(default)]
    pub pinned: bool,
}

/// Accept or reject a proposed memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DecideMemory {
    pub memory_id: MemoryId,
    pub accept: bool,
}

/// Replace the content with a new version; the old one is superseded, not erased.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReviseMemory {
    pub memory_id: MemoryId,
    pub content: String,
}

/// Remove from all future context and the search index; content is erased.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ForgetMemory {
    pub memory_id: MemoryId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PinMemory {
    pub memory_id: MemoryId,
    pub pinned: bool,
}

/// Merge a scope's memories into short points now, instead of waiting for
/// the background job (which starts by itself once the scope grows).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConsolidateMemory {
    pub scope: MemoryScope,
}

pub fn validate_content(content: &str) -> Result<String, DomainError> {
    require_text("content", content, MAX_MEMORY_CHARS)
}

/// User-authored kinds per scope: global holds explicit preferences/knowledge
/// only; identity belongs to an employee; summaries are system-made.
pub fn validate_remember(cmd: &Remember) -> Result<String, DomainError> {
    let content = validate_content(&cmd.content)?;
    let ok = match (cmd.scope, cmd.kind) {
        (_, MemoryKind::ConversationSummary) => false,
        (MemoryScope::Employee(_), MemoryKind::Identity) => true,
        (_, MemoryKind::Identity) => false,
        _ => true,
    };
    if !ok {
        return Err(DomainError::OutOfRange { field: "kind".into(), detail: format!("{} is not allowed in this scope", cmd.kind.label()) });
    }
    Ok(content)
}

/// Normalized text hash input: lowercase, alphanumerics, single spaces.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            out.push(ch);
            space = false;
        } else if !space && !out.is_empty() {
            out.push(' ');
            space = true;
        }
    }
    out.trim_end().to_owned()
}

/// Word-set similarity (Jaccard) on normalized words of 3+ characters.
pub fn similarity(a: &str, b: &str) -> f64 {
    let words = |t: &str| normalize(t).split(' ').filter(|w| w.chars().count() >= 3).map(str::to_owned).collect::<std::collections::BTreeSet<_>>();
    let (a, b) = (words(a), words(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(&b).count() as f64 / a.union(&b).count() as f64
}

/// Best-effort secret detector.
pub fn looks_secret(text: &str) -> bool {
    let lower = text.to_lowercase();
    if ["password", "wachtwoord", "api key", "api-key", "apikey", "secret", "token:", "bearer "].iter().any(|k| lower.contains(k)) {
        return true;
    }
    text.split(|c: char| c.is_whitespace() || c == '"' || c == '\'').any(|word| {
        let w = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
        let prefixed = ["sk-", "sk_", "ghp_", "xoxb-", "AKIA", "AIza"].iter().any(|p| w.starts_with(p)) && w.len() >= 16;
        let long_mixed = w.len() >= 32 && w.chars().any(|c| c.is_ascii_digit()) && w.chars().any(|c| c.is_ascii_alphabetic()) && !w.contains("://");
        prefixed || long_mixed
    })
}

/// One candidate as the extraction model returned it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Candidate {
    pub kind: String,
    pub content: String,
    /// Label of a source given in the prompt (e.g. "E2").
    pub source: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default = "half")]
    pub importance: f64,
    #[serde(default = "half")]
    pub confidence: f64,
}

fn half() -> f64 {
    0.5
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Extraction {
    #[serde(default)]
    pub items: Vec<Candidate>,
    /// Lessons from the signals S1..Sn; only asked for when there are signals.
    #[serde(default)]
    pub lessons: Vec<LessonCandidate>,
    /// What a ticket delivered; only asked for after the ticket's Done (MU1-18).
    #[serde(default)]
    pub ticket_summary: Option<String>,
}

/// The ticket summary is cut here (the "#n “title”" line included).
pub const TICKET_SUMMARY_MAX_CHARS: usize = 800;

/// The one project memory of a ticket that went to Done (MU1-18): it names the
/// ticket by number and title. Without a usable summary from the model, what
/// the extraction kept for the project stands in; without that, only the close.
pub fn ticket_summary(number: u32, title: &str, summary: Option<&str>, kept: &[&str]) -> String {
    let flat = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let head = format!("Ticket #{number} “{}” is done", flat(title));
    let body = summary.map(flat).filter(|s| !s.is_empty()).or_else(|| Some(kept.iter().map(|k| flat(k)).filter(|k| !k.is_empty()).collect::<Vec<_>>().join(" ")).filter(|s| !s.is_empty()));
    let text = match body {
        Some(body) => format!("{head}: {body}"),
        None => format!("{head}."),
    };
    if text.chars().count() <= TICKET_SUMMARY_MAX_CHARS {
        return text;
    }
    let mut cut: String = text.chars().take(TICKET_SUMMARY_MAX_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// At most this many signals go to the extraction, labelled S1..S8.
pub const MAX_SIGNALS: usize = 8;
/// One signal is cut here.
pub const SIGNAL_MAX_CHARS: usize = 400;
/// At most this many lessons per task.
pub const MAX_LESSONS: usize = 2;
/// A lesson (its `applies_when` and `lesson` together) is at most this long.
pub const LESSON_MAX_CHARS: usize = 400;
/// From this similarity on a new lesson reinforces an existing one.
pub const LESSON_SIMILAR: f64 = 0.5;
/// An employee proposes an instruction change only for a lesson reinforced this often.
pub const LESSON_PROPOSE_AT: u32 = 2;

/// The gate before a lesson: what went wrong or was corrected in a task,
/// found in SQL without a model (each with its source). Signals are trimmed,
/// cut at `SIGNAL_MAX_CHARS`, without doubles, at most `MAX_SIGNALS`, and
/// labelled S1..Sn in their order. No signals: no lesson section at all.
pub fn gate_signals<T>(raw: Vec<(T, String)>) -> Vec<(String, T, String)> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (source, text) in raw {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() || looks_secret(&text) {
            continue;
        }
        let text = if text.chars().count() > SIGNAL_MAX_CHARS {
            format!("{}…", text.chars().take(SIGNAL_MAX_CHARS - 1).collect::<String>().trim_end())
        } else {
            text
        };
        let key = normalize(&text);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push((format!("S{}", out.len() + 1), source, text));
        if out.len() == MAX_SIGNALS {
            break;
        }
    }
    out
}

/// One lesson as the extraction model returned it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LessonCandidate {
    /// When the lesson applies, e.g. "editing the Flutter protocol models".
    #[serde(default)]
    pub applies_when: String,
    pub lesson: String,
    /// Labels of the signals it rests on, e.g. ["S1", "S3"].
    #[serde(default)]
    pub evidence: Vec<String>,
    /// employee (default) | project
    #[serde(default)]
    pub scope: Option<String>,
}

/// A lesson that passed `validate_lessons`.
#[derive(Debug, Clone, PartialEq)]
pub struct Lesson {
    pub applies_when: String,
    pub lesson: String,
    /// Signal labels, only ones that exist.
    pub evidence: Vec<String>,
    /// true = employee scope (active at once), false = project scope (proposed).
    pub employee_scope: bool,
    /// The memory text: "When <applies_when>: <lesson>".
    pub content: String,
}

/// A lesson already in memory (`locator_json.lesson` set).
#[derive(Debug, Clone)]
pub struct ExistingLesson {
    pub id: MemoryId,
    pub content: String,
    pub employee_scope: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LessonVerdict {
    /// Stored as a new lesson.
    New(Lesson),
    /// Like a lesson already in memory: that one is reinforced, nothing new is stored.
    Reinforce { existing: MemoryId, lesson: Lesson },
    Drop { lesson: String, reason: &'static str },
}

/// Validate lessons (pure, like `validate_candidates`): a lesson needs
/// `applies_when` and at least one existing signal as evidence, must not look
/// like a secret, and at most `MAX_LESSONS` are kept. One like an existing
/// lesson of the same scope reinforces that one instead of making a double.
pub fn validate_lessons(candidates: &[LessonCandidate], signals: &[&str], existing: &[ExistingLesson]) -> Vec<LessonVerdict> {
    let mut out = Vec::new();
    let mut kept = 0;
    let mut seen: Vec<String> = Vec::new();
    for c in candidates.iter().take(6) {
        let drop = |reason| LessonVerdict::Drop { lesson: c.lesson.clone(), reason };
        if signals.is_empty() {
            out.push(drop("no signals"));
            continue;
        }
        if kept >= MAX_LESSONS {
            out.push(drop("more than two lessons"));
            continue;
        }
        let when = c.applies_when.trim();
        let when = when.strip_prefix("When ").or_else(|| when.strip_prefix("when ")).unwrap_or(when).trim().trim_end_matches([':', '.', ',']).trim();
        if when.is_empty() {
            out.push(drop("no applies_when"));
            continue;
        }
        let lesson = c.lesson.trim();
        if lesson.is_empty() {
            out.push(drop("empty lesson"));
            continue;
        }
        let content = format!("When {when}: {lesson}");
        if content.chars().count() > LESSON_MAX_CHARS {
            out.push(drop("too long"));
            continue;
        }
        let mut evidence: Vec<String> = Vec::new();
        for label in c.evidence.iter().map(|l| l.trim()) {
            if signals.contains(&label) && !evidence.iter().any(|e| e == label) {
                evidence.push(label.to_owned());
            }
        }
        if evidence.is_empty() {
            out.push(drop("no evidence"));
            continue;
        }
        if looks_secret(&content) {
            out.push(drop("possible secret"));
            continue;
        }
        let employee_scope = match c.scope.as_deref().map(str::trim) {
            None | Some("") | Some("employee") => true,
            Some("project") => false,
            Some(_) => {
                out.push(drop("scope not allowed (project or employee only)"));
                continue;
            }
        };
        let key = normalize(&content);
        if seen.contains(&key) {
            out.push(drop("duplicate"));
            continue;
        }
        seen.push(key);
        kept += 1;
        let lesson = Lesson { applies_when: when.to_owned(), lesson: lesson.to_owned(), evidence, employee_scope, content };
        let like = existing
            .iter()
            .filter(|e| e.employee_scope == employee_scope)
            .map(|e| (e, similarity(&e.content, &lesson.content)))
            .filter(|(_, s)| *s >= LESSON_SIMILAR)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        out.push(match like {
            Some((e, _)) => LessonVerdict::Reinforce { existing: e.id, lesson },
            None => LessonVerdict::New(lesson),
        });
    }
    out
}

/// A lesson's place in `locator_json.lesson`: when it applies, its evidence
/// (label and signal text), how often later tasks confirmed it and from which tasks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LessonLocator {
    pub applies_when: String,
    #[serde(default)]
    pub evidence: Vec<LessonEvidence>,
    /// How often a later task found the same lesson again.
    #[serde(default)]
    pub reinforced: u32,
    #[serde(default)]
    pub tasks: Vec<crate::TaskId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LessonEvidence {
    /// S1..S8 as the extraction saw it.
    pub label: String,
    pub text: String,
}

/// Where an accepted candidate goes and with which status.
#[derive(Debug, Clone, PartialEq)]
pub struct Accepted {
    pub kind: MemoryKind,
    pub content: String,
    pub source_label: String,
    /// true = employee scope, false = project scope.
    pub employee_scope: bool,
    pub importance: f64,
    pub confidence: f64,
    pub status: MemoryStatus,
    /// Existing memory this one conflicts with (shown, never overwritten).
    pub conflicts_with: Option<MemoryId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Keep(Accepted),
    Drop { content: String, reason: &'static str },
}

/// An existing memory the validator compares against.
#[derive(Debug, Clone)]
pub struct Existing {
    pub id: MemoryId,
    pub kind: MemoryKind,
    pub content: String,
}

/// Validate extracted candidates. Pure, so fixed eval
/// fixtures can test it without a model.
pub fn validate_candidates(extraction: &Extraction, sources: &[&str], existing: &[Existing]) -> Vec<Verdict> {
    let mut seen: Vec<String> = existing.iter().map(|e| normalize(&e.content)).collect();
    let mut out = Vec::new();
    for c in extraction.items.iter().take(12) {
        let drop = |reason| Verdict::Drop { content: c.content.clone(), reason };
        let Some(kind) = MemoryKind::parse(c.kind.trim()) else {
            out.push(drop("unknown kind"));
            continue;
        };
        if matches!(kind, MemoryKind::Identity | MemoryKind::ConversationSummary) {
            out.push(drop("kind not allowed for extraction"));
            continue;
        }
        let employee_scope = match c.scope.as_deref().map(str::trim) {
            None | Some("") | Some("employee") => true,
            Some("project") => false,
            Some(_) => {
                out.push(drop("scope not allowed (project or employee only)"));
                continue;
            }
        };
        if !sources.contains(&c.source.trim()) {
            out.push(drop("source does not exist"));
            continue;
        }
        let Ok(content) = validate_content(&c.content) else {
            out.push(drop("empty or too long"));
            continue;
        };
        if looks_secret(&content) {
            out.push(drop("possible secret"));
            continue;
        }
        let normalized = normalize(&content);
        if seen.contains(&normalized) {
            out.push(drop("duplicate"));
            continue;
        }
        seen.push(normalized);
        let conflicts_with = existing
            .iter()
            .filter(|e| e.kind == kind && matches!(kind, MemoryKind::Decision | MemoryKind::ProjectKnowledge))
            .find(|e| similarity(&e.content, &content) >= 0.5)
            .map(|e| e.id);
        // Employee episodes are stored automatically in their own scope;
        // knowledge, decisions and anything conflicting need the user.
        let status = if kind == MemoryKind::Episodic && employee_scope && conflicts_with.is_none() { MemoryStatus::Active } else { MemoryStatus::Proposed };
        out.push(Verdict::Keep(Accepted {
            kind,
            content,
            source_label: c.source.trim().to_owned(),
            employee_scope,
            importance: c.importance.clamp(0.0, 1.0),
            confidence: c.confidence.clamp(0.0, 1.0),
            status,
            conflicts_with,
        }));
    }
    out
}

/// A memory an employee compares its own note against.
#[derive(Debug, Clone)]
pub struct ExistingNote {
    pub id: MemoryId,
    pub kind: MemoryKind,
    pub content: String,
    /// The user wrote or decided it: an employee never overrules that.
    pub by_user: bool,
}

/// Where an employee's own note (the `remember` tool) lands.
#[derive(Debug, Clone, PartialEq)]
pub enum NoteOutcome {
    /// Already known: nothing is stored.
    Duplicate,
    /// Active at once.
    New,
    /// Active, and supersedes an earlier memory an employee or the policy decided.
    Replaces(MemoryId),
    /// Contradicts what the user decided: stored as a proposal for the user.
    Proposal(MemoryId),
}

/// Kinds an employee may note itself.
pub fn employee_note_kind(kind: &str) -> Option<MemoryKind> {
    match MemoryKind::parse(kind.trim())? {
        k @ (MemoryKind::Decision | MemoryKind::ProjectKnowledge | MemoryKind::Episodic) => Some(k),
        _ => None,
    }
}

/// Place an employee's note among the memories of its scope (pure, testable).
pub fn place_employee_note(kind: MemoryKind, content: &str, existing: &[ExistingNote]) -> NoteOutcome {
    let normalized = normalize(content);
    if existing.iter().any(|e| normalize(&e.content) == normalized) {
        return NoteOutcome::Duplicate;
    }
    let conflict = existing
        .iter()
        .filter(|e| e.kind == kind && matches!(kind, MemoryKind::Decision | MemoryKind::ProjectKnowledge))
        .max_by(|a, b| similarity(&a.content, content).total_cmp(&similarity(&b.content, content)))
        .filter(|e| similarity(&e.content, content) >= 0.5);
    match conflict {
        Some(e) if e.by_user => NoteOutcome::Proposal(e.id),
        Some(e) => NoteOutcome::Replaces(e.id),
        None => NoteOutcome::New,
    }
}

pub const CONSOLIDATION_VERSION: &str = "aurelius-consolidate-1";
/// A scope is consolidated by itself once its mergeable memories reach this
/// many items or this many tokens (and at most once per `CONSOLIDATE_EVERY_MS`).
pub const CONSOLIDATE_AT_ITEMS: u32 = 30;
pub const CONSOLIDATE_AT_TOKENS: u32 = 3_000;
pub const CONSOLIDATE_EVERY_MS: i64 = 24 * 60 * 60 * 1000;
/// One consolidated point: a short statement, not a report.
pub const POINT_MAX_CHARS: usize = 400;

pub fn needs_consolidation(items: u32, tokens: u32) -> bool {
    items >= CONSOLIDATE_AT_ITEMS || tokens >= CONSOLIDATE_AT_TOKENS
}

/// A memory the consolidation may merge: active, not pinned, not decided by
/// the user, and not identity or a conversation summary.
#[derive(Debug, Clone)]
pub struct Mergeable {
    /// Label given to the model, e.g. "M3".
    pub label: String,
    pub id: MemoryId,
    pub kind: MemoryKind,
    pub content: String,
}

/// What the consolidation model returned.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConsolidationOutput {
    #[serde(default)]
    pub points: Vec<PointCandidate>,
    /// Labels of memories that are outdated or replaced by another one.
    #[serde(default)]
    pub dropped: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PointCandidate {
    pub kind: String,
    pub content: String,
    /// Labels of the memories this point replaces.
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default = "half")]
    pub importance: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub kind: MemoryKind,
    pub content: String,
    pub sources: Vec<MemoryId>,
    pub importance: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConsolidationPlan {
    pub points: Vec<Point>,
    /// Outdated memories that no point replaces.
    pub dropped: Vec<MemoryId>,
    /// The cut looks too drastic to apply on its own: the points become
    /// proposals for the user and nothing is superseded until they accept.
    pub needs_review: bool,
}

impl ConsolidationPlan {
    /// Every memory this plan would supersede.
    pub fn covered(&self) -> Vec<MemoryId> {
        let mut out: Vec<MemoryId> = Vec::new();
        for id in self.points.iter().flat_map(|p| &p.sources).chain(&self.dropped) {
            if !out.contains(id) {
                out.push(*id);
            }
        }
        out
    }
}

/// Validate a consolidation (pure, like `validate_candidates`): a point must
/// name existing memories as its sources, so the model cannot invent one;
/// what no valid point or drop names stays as it is.
pub fn validate_consolidation(output: &ConsolidationOutput, inputs: &[Mergeable]) -> ConsolidationPlan {
    let find = |label: &str| inputs.iter().find(|m| m.label == label.trim());
    let mut plan = ConsolidationPlan::default();
    let mut seen: Vec<String> = Vec::new();
    for c in &output.points {
        let Some(kind) = MemoryKind::parse(c.kind.trim()).filter(|k| matches!(k, MemoryKind::Decision | MemoryKind::ProjectKnowledge | MemoryKind::Episodic)) else { continue };
        let Ok(content) = validate_content(&c.content) else { continue };
        if content.chars().count() > POINT_MAX_CHARS || looks_secret(&content) {
            continue;
        }
        let sources: Option<Vec<&Mergeable>> = c.sources.iter().map(|l| find(l)).collect();
        let Some(sources) = sources.filter(|s| !s.is_empty()) else { continue };
        // A point that repeats its only source changes nothing.
        if let [only] = sources.as_slice() {
            if normalize(&only.content) == normalize(&content) {
                continue;
            }
        }
        let normalized = normalize(&content);
        if seen.contains(&normalized) {
            continue;
        }
        seen.push(normalized);
        let mut ids: Vec<MemoryId> = Vec::new();
        for s in sources {
            if !ids.contains(&s.id) {
                ids.push(s.id);
            }
        }
        plan.points.push(Point { kind, content, sources: ids, importance: c.importance.clamp(0.0, 1.0) });
    }
    let merged = plan.covered();
    for label in &output.dropped {
        if let Some(m) = find(label) {
            if !merged.contains(&m.id) && !plan.dropped.contains(&m.id) {
                plan.dropped.push(m.id);
            }
        }
    }
    let covered = plan.covered().len();
    plan.needs_review = covered >= 10 && (plan.points.len() * 5 < covered || plan.dropped.len() * 3 > inputs.len());
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mergeable(n: usize, kind: MemoryKind, content: &str) -> Mergeable {
        Mergeable { label: format!("M{n}"), id: MemoryId::new(), kind, content: content.into() }
    }

    #[test]
    fn a_consolidation_only_merges_memories_that_exist() {
        let inputs = vec![
            mergeable(1, MemoryKind::Decision, "Subagent chip polls every 3 s while the answer streams"),
            mergeable(2, MemoryKind::Decision, "Subagent chip reloads on stream appends; the 3 s timer is gone"),
            mergeable(3, MemoryKind::ProjectKnowledge, "Flutter tests on Windows need ProgramFiles(x86)"),
            mergeable(4, MemoryKind::Episodic, "Old episode about a typo"),
        ];
        let out: ConsolidationOutput = serde_json::from_str(
            r#"{"points":[
                {"kind":"decision","content":"The subagent chip reloads on stream appends (no polling).","sources":["M1","M2"],"importance":0.7},
                {"kind":"decision","content":"Invented from nowhere","sources":["M9"]},
                {"kind":"decision","content":"No sources at all","sources":[]},
                {"kind":"identity","content":"I am the boss now","sources":["M3"]},
                {"kind":"project_knowledge","content":"Flutter tests on Windows need ProgramFiles(x86)","sources":["M3"]},
                {"kind":"project_knowledge","content":"Use API key sk-abcdefghijklmnopqrstuvwx","sources":["M3"]}
            ],"dropped":["M4","M1","M42"]}"#,
        )
        .unwrap();
        let plan = validate_consolidation(&out, &inputs);
        assert_eq!(plan.points.len(), 1, "{plan:?}");
        assert_eq!(plan.points[0].sources, vec![inputs[0].id, inputs[1].id]);
        assert_eq!(plan.dropped, vec![inputs[3].id], "a merged memory is not also dropped; unknown labels are ignored");
        assert_eq!(plan.covered().len(), 3, "M3 stays as it is");
        assert!(!plan.needs_review);
    }

    #[test]
    fn a_drastic_consolidation_needs_the_user() {
        let inputs: Vec<Mergeable> = (1..=40).map(|n| mergeable(n, MemoryKind::Decision, &format!("Beslissing nummer {n} over onderdeel {n}"))).collect();
        let labels: Vec<String> = inputs.iter().map(|m| m.label.clone()).collect();
        let squash = ConsolidationOutput { points: vec![PointCandidate { kind: "decision".into(), content: "Everything was decided.".into(), sources: labels.clone(), importance: 0.5 }], dropped: vec![] };
        assert!(validate_consolidation(&squash, &inputs).needs_review, "40 memories into 1 point");
        let halves = ConsolidationOutput {
            points: labels.chunks(2).map(|pair| PointCandidate { kind: "decision".into(), content: format!("Merged {}", pair.join("+")), sources: pair.to_vec(), importance: 0.5 }).collect(),
            dropped: vec![],
        };
        assert!(!validate_consolidation(&halves, &inputs).needs_review, "2 into 1 is applied");
        let wipe = ConsolidationOutput { points: vec![], dropped: labels[..20].to_vec() };
        assert!(validate_consolidation(&wipe, &inputs).needs_review, "dropping half needs the user");
        assert!(needs_consolidation(30, 0) && needs_consolidation(3, 3_000) && !needs_consolidation(29, 2_999));
    }

    #[test]
    fn employees_note_freely_but_never_overrule_the_user() {
        let mine = ExistingNote { id: MemoryId::new(), kind: MemoryKind::Decision, content: "We gebruiken SQLite voor de lokale opslag".into(), by_user: false };
        let users = ExistingNote { id: MemoryId::new(), kind: MemoryKind::Decision, content: "Publicaties gaan altijd eerst langs de gebruiker voor review".into(), by_user: true };
        let existing = [mine.clone(), users.clone()];
        assert_eq!(place_employee_note(MemoryKind::Decision, "We gebruiken SQLite voor de lokale opslag.", &existing), NoteOutcome::Duplicate);
        assert_eq!(place_employee_note(MemoryKind::Decision, "We gebruiken SQLite met WAL voor de lokale opslag", &existing), NoteOutcome::Replaces(mine.id));
        assert_eq!(place_employee_note(MemoryKind::Decision, "Publicaties gaan niet meer langs de gebruiker voor review", &existing), NoteOutcome::Proposal(users.id));
        assert_eq!(place_employee_note(MemoryKind::ProjectKnowledge, "De build draait op Windows en macOS", &existing), NoteOutcome::New);
        assert_eq!(employee_note_kind("identity"), None);
        assert_eq!(employee_note_kind("decision"), Some(MemoryKind::Decision));
    }

    #[test]
    fn a_ticket_summary_names_the_ticket_and_falls_back_on_what_was_kept() {
        assert_eq!(ticket_summary(7, "Login  page", Some(" Added a\nlogin form. "), &["ignored"]), "Ticket #7 “Login page” is done: Added a login form.");
        assert_eq!(ticket_summary(7, "Login page", Some("  "), &["Requested goal: a form", ""]), "Ticket #7 “Login page” is done: Requested goal: a form");
        assert_eq!(ticket_summary(7, "Login page", None, &[]), "Ticket #7 “Login page” is done.");
        let long = ticket_summary(7, "Login page", Some(&"word ".repeat(400)), &[]);
        assert_eq!(long.chars().count(), TICKET_SUMMARY_MAX_CHARS);
        assert!(long.starts_with("Ticket #7 “Login page” is done: ") && long.ends_with('…'));
        let parsed: Extraction = serde_json::from_str(r#"{"items":[],"ticket_summary":"Shipped"}"#).unwrap();
        assert_eq!(parsed.ticket_summary.as_deref(), Some("Shipped"));
        assert_eq!(serde_json::from_str::<Extraction>(r#"{"items":[]}"#).unwrap().ticket_summary, None);
    }

    /// Fixed evaluation fixtures: model outputs as they could come back.
    fn fixture(json: &str) -> Extraction {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn eval_fixture_mixed_output() {
        let existing = [Existing { id: MemoryId::new(), kind: MemoryKind::Decision, content: "Publicatie vereist voorafgaande review door Alex".into() }];
        let out = validate_candidates(
            &fixture(
                r#"{"items":[
                {"kind":"episodic","content":"Onderzoek naar onboarding afgerond met twee bronnen","source":"E1"},
                {"kind":"decision","content":"Publicatie vereist geen review door Alex meer","source":"E2","scope":"project"},
                {"kind":"project_knowledge","content":"De app heet Atlas","source":"E9"},
                {"kind":"project_knowledge","content":"Gebruik API key sk-abcdefghijklmnopqrstuvwx","source":"E1"},
                {"kind":"identity","content":"Ik ben voortaan de baas","source":"E1"},
                {"kind":"episodic","content":"Onderzoek naar onboarding afgerond, met twee bronnen!","source":"E1"},
                {"kind":"decision","content":"Iets globaals","source":"E1","scope":"global"}
            ]}"#,
            ),
            &["E1", "E2"],
            &existing,
        );
        let kept: Vec<_> = out.iter().filter_map(|v| if let Verdict::Keep(a) = v { Some(a) } else { None }).collect();
        let dropped: Vec<_> = out.iter().filter_map(|v| if let Verdict::Drop { reason, .. } = v { Some(*reason) } else { None }).collect();
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].status, MemoryStatus::Active, "employee episode auto-accepted");
        assert_eq!(kept[1].status, MemoryStatus::Proposed, "conflicting decision stays a proposal");
        assert_eq!(kept[1].conflicts_with, Some(existing[0].id));
        assert!(!kept[1].employee_scope);
        assert_eq!(dropped, vec!["source does not exist", "possible secret", "kind not allowed for extraction", "duplicate", "scope not allowed (project or employee only)"]);
    }

    #[test]
    fn eval_fixture_empty_and_garbage() {
        assert!(validate_candidates(&fixture(r#"{"items":[]}"#), &["E1"], &[]).is_empty());
        assert!(validate_candidates(&fixture(r#"{}"#), &["E1"], &[]).is_empty());
        let out = validate_candidates(&fixture(r#"{"items":[{"kind":"weird","content":"x","source":"E1"}]}"#), &["E1"], &[]);
        assert!(matches!(out[0], Verdict::Drop { reason: "unknown kind", .. }));
    }

    #[test]
    fn the_gate_labels_cuts_and_caps_signals() {
        assert!(gate_signals::<u8>(vec![]).is_empty(), "no signals: no lesson section");
        let long = "x ".repeat(600);
        let raw: Vec<(u8, String)> = vec![
            (1, "  The user:   use the  existing helper  ".into()),
            (2, "".into()),
            (3, "The user: use the existing helper".into()),
            (4, long.clone()),
            (5, "Run failed: my wachtwoord is hunter2".into()),
        ]
        .into_iter()
        .chain((10..20).map(|n| (n, format!("Run {n} was cancelled"))))
        .collect();
        let gated = gate_signals(raw);
        assert_eq!(gated.len(), MAX_SIGNALS);
        assert_eq!((gated[0].0.as_str(), gated[0].1, gated[0].2.as_str()), ("S1", 1, "The user: use the existing helper"));
        assert_eq!(gated[1].1, 4, "empty, double and secret signals are left out");
        assert_eq!(gated[1].2.chars().count(), SIGNAL_MAX_CHARS);
        assert!(gated[1].2.ends_with('…'));
        assert_eq!(gated.last().unwrap().0, "S8");
    }

    #[test]
    fn lessons_need_evidence_and_reinforce_instead_of_doubling() {
        let old = ExistingLesson { id: MemoryId::new(), content: "When editing protocol views: add the field to the Dart models in the same change".into(), employee_scope: true };
        let lesson = |when: &str, text: &str, evidence: &[&str], scope: Option<&str>| LessonCandidate {
            applies_when: when.into(),
            lesson: text.into(),
            evidence: evidence.iter().map(|e| e.to_string()).collect(),
            scope: scope.map(str::to_owned),
        };
        let candidates = vec![
            lesson("", "Always run the tests", &["S1"], None),
            lesson("fixing a flaky test", "Look at the clock first", &["S9"], None),
            lesson("deploying", "Use token: abcdef", &["S1"], None),
            lesson("When editing protocol views:", "add the field to the Dart models in the same change too", &["S2", "S2"], None),
            lesson("a plan is rejected", "Ask what the user wants before a second plan", &["S1"], Some("project")),
            lesson("anything", "A third lesson", &["S1"], None),
        ];
        let out = validate_lessons(&candidates, &["S1", "S2"], std::slice::from_ref(&old));
        let reasons: Vec<&str> = out.iter().filter_map(|v| if let LessonVerdict::Drop { reason, .. } = v { Some(*reason) } else { None }).collect();
        assert_eq!(reasons, vec!["no applies_when", "no evidence", "possible secret", "more than two lessons"]);
        let LessonVerdict::Reinforce { existing, lesson } = &out[3] else { panic!("{:?}", out[3]) };
        assert_eq!(*existing, old.id);
        assert_eq!(lesson.evidence, vec!["S2".to_string()]);
        assert_eq!(lesson.content, "When editing protocol views: add the field to the Dart models in the same change too");
        let LessonVerdict::New(new) = &out[4] else { panic!("{:?}", out[4]) };
        assert!(!new.employee_scope, "a project lesson");
        // Without signals nothing passes, whatever the model says.
        assert!(validate_lessons(&candidates, &[], &[]).iter().all(|v| matches!(v, LessonVerdict::Drop { reason: "no signals", .. })));
        let parsed: Extraction = serde_json::from_str(r#"{"items":[],"lessons":[{"applies_when":"x","lesson":"y","evidence":["S1"]}]}"#).unwrap();
        assert_eq!(parsed.lessons.len(), 1);
        assert!(serde_json::from_str::<Extraction>(r#"{"items":[]}"#).unwrap().lessons.is_empty(), "older output has no lessons");
    }

    #[test]
    fn user_kinds_per_scope() {
        let r = |scope, kind| validate_remember(&Remember { scope, kind, content: "x".into(), pinned: false });
        assert!(r(MemoryScope::Employee(EmployeeId::new()), MemoryKind::Identity).is_ok());
        assert!(r(MemoryScope::Project(ProjectId::new()), MemoryKind::Identity).is_err());
        assert!(r(MemoryScope::Global, MemoryKind::ConversationSummary).is_err());
        assert!(r(MemoryScope::Global, MemoryKind::ProjectKnowledge).is_ok());
    }

    #[test]
    fn normalize_and_similarity() {
        assert_eq!(normalize("  Hallo, Wereld!! "), "hallo wereld");
        assert!(similarity("Publicatie vereist review", "publicatie vereist geen review") > 0.5);
        assert!(similarity("appels", "peren") < 0.1);
        assert!(looks_secret("mijn wachtwoord is hunter2"));
        assert!(!looks_secret("De klant wil een rapport op vrijdag"));
    }
}
