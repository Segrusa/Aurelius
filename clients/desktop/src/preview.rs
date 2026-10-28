//! Sample content from the "Aurelius Core Screens" design. Everything here is
//! rendered with a visible "Sample" label and is replaced by daemon read
//! models as the backlog lands (managers B13, tasks B14, timeline B16, memory B17).
//! Toggle with Ctrl+Shift+D.

pub struct Manager {
    pub name: &'static str,
    pub role: &'static str,
    pub working: bool,
    pub summary: &'static str,
}

pub struct Project {
    pub name: &'static str,
    pub kind: &'static str,
    pub description: &'static str,
    pub managers: &'static [Manager],
    pub active_tasks: u32,
    pub approvals: u32,
}

pub const PROJECTS: &[Project] = &[
    Project {
        name: "Atlas",
        kind: "Product development",
        description: "Offline-first desktop app",
        managers: &[
            Manager { name: "Alex", role: "Development Manager", working: true, summary: "Fixing the authentication race. 3 workers on implementation and review." },
            Manager { name: "Maya", role: "Social Manager", working: false, summary: "Launch proposal is ready for your read. No active work." },
        ],
        active_tasks: 3,
        approvals: 1,
    },
    Project {
        name: "Phoenix",
        kind: "Market positioning",
        description: "",
        managers: &[
            Manager { name: "Iris", role: "Product", working: false, summary: "No active work." },
            Manager { name: "Theo", role: "Research", working: false, summary: "Competitor analysis finished." },
        ],
        active_tasks: 0,
        approvals: 0,
    },
];

pub struct Approval {
    pub requester: &'static str,
    pub title: &'static str,
    pub code: &'static str,
    pub detail: &'static str,
}

pub const APPROVAL: Approval = Approval {
    requester: "Alex requests approval",
    title: "Database migration: add ",
    code: "sessions.refreshed_at",
    detail: "Additive only. No existing records are modified. Reversible.",
};

// ── Manager conversation ────────────────────────────────────────────────

pub const MANAGER_REQUEST: &str = "Investigate the authentication issue and fix it if you're confident about the cause.";
pub const MANAGER_REPLY: &[&str] = &[
    "I've isolated it. The token refresh logic can race when two requests arrive at the same moment: both see an expired token, both refresh, and the second invalidates the first.",
    "The fix is a single-flight lock around refresh. I've asked a second worker to verify it independently before I apply it.",
];
pub const MANAGER_FOLLOWUP: &str = "Sounds good. Don't change the public API.";
pub const MANAGER_ACK: &str =
    "Understood. The lock stays internal to the session module. I'll remember this for future work on Atlas.";

#[derive(Clone, Copy, PartialEq)]
pub enum Step {
    Done,
    Running,
    Pending,
}

pub const CURRENT_WORK: &[(Step, &str, &str)] = &[
    (Step::Done, "Root cause analysis", "12:40"),
    (Step::Running, "Implementation", "04:18"),
    (Step::Running, "Independent review", "01:02"),
    (Step::Pending, "Final verification", ""),
];

pub struct HistoryItem {
    pub reverted: bool,
    pub title: &'static str,
    pub date: &'static str,
    pub commit: &'static str,
    pub diff: &'static str,
}

pub const HISTORY_IN_PROGRESS: &[(&str, &str, bool)] = &[
    ("Fix authentication race", "alex/auth-refresh-lock · 3 commits", true),
    ("Sync conflict resolution UI", "alex/sync-conflicts · 1 commit", false),
];

pub const HISTORY_DONE: &[HistoryItem] = &[
    HistoryItem { reverted: false, title: "Sync conflict detection", date: "Sep 29", commit: "c9e02b1", diff: "+312 −40" },
    HistoryItem { reverted: false, title: "WAL mode for local database", date: "Sep 30", commit: "0b81e4d", diff: "+18 −2" },
    HistoryItem { reverted: false, title: "Settings window keyboard nav", date: "Sep 30", commit: "c47a2f9", diff: "+96 −31" },
    HistoryItem { reverted: true, title: "Virtualised message list", date: "Sep 24", commit: "4e17d0a", diff: "" },
    HistoryItem { reverted: false, title: "Session module extraction", date: "Sep 22", commit: "7d0e912", diff: "+204 −188" },
];

// ── Tasks ───────────────────────────────────────────────────────────────

pub struct Task {
    pub title: &'static str,
    pub manager: &'static str,
    pub branch: Option<&'static str>,
    pub waiting: bool,
    pub steps: &'static [(Step, &'static str)],
}

pub const ACTIVE_TASKS: &[Task] = &[
    Task {
        title: "Fix authentication race",
        manager: "Alex",
        branch: Some("alex/auth-refresh-lock"),
        waiting: false,
        steps: &[(Step::Done, "Investigation"), (Step::Running, "Implementation"), (Step::Running, "Review")],
    },
    Task {
        title: "Prepare October launch",
        manager: "Maya",
        branch: None,
        waiting: false,
        steps: &[(Step::Done, "Competitor research"), (Step::Running, "Positioning"), (Step::Pending, "Copy")],
    },
    Task { title: "Sync conflict resolution UI", manager: "Alex", branch: Some("alex/sync-conflicts"), waiting: false, steps: &[] },
    Task { title: "Database migration", manager: "Alex", branch: None, waiting: true, steps: &[] },
];

pub const COMPLETED_TASKS: &[(&str, &str, &str, bool, &str)] = &[
    ("TODAY", "Fix authentication race", "a3f9c21", true, "18:42"),
    ("TODAY", "Competitor analysis", "Document", false, "17:51"),
    ("YESTERDAY", "WAL mode for local database", "0b81e4d", true, "Sep 30"),
    ("YESTERDAY", "Settings window keyboard nav", "c47a2f9", true, "Sep 30"),
    ("YESTERDAY", "Launch announcement draft", "Document", false, "Sep 30"),
];

pub const COMMITS: &[(&str, &str, &str)] = &[
    ("e41b07c", "Add regression test for parallel refresh", "Worker B · 18:39 · +22"),
    ("9c2d4aa", "Single-flight lock around token refresh", "Worker A · 18:33 · +17 −9"),
    ("51fa3e0", "Log refresh timing for reproduction", "Worker B · 18:16 · +2"),
];

// ── Timeline ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum Marker {
    Outcome,
    Decision,
    Start,
    Minor,
    Git,
}

pub struct TimelineEvent {
    pub time: &'static str,
    pub marker: Marker,
    pub label: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    pub quote: &'static str,
    pub git_only: bool,
}

pub const TIMELINE: &[TimelineEvent] = &[
    TimelineEvent { time: "18:42", marker: Marker::Outcome, label: "MERGED · COMPLETED", title: "Authentication race condition fixed", detail: "a3f9c21 → main · Alex · 4 workers · 38 min", quote: "", git_only: false },
    TimelineEvent { time: "18:41", marker: Marker::Decision, label: "DECISION", title: "You approved the merge into main", detail: "", quote: "", git_only: false },
    TimelineEvent { time: "18:33", marker: Marker::Git, label: "", title: "3 commits on alex/auth-refresh-lock", detail: "e41b07c  regression test for parallel refresh · B\n9c2d4aa  single-flight lock around refresh · A\n51fa3e0  log refresh timing · B", quote: "", git_only: true },
    TimelineEvent { time: "18:31", marker: Marker::Decision, label: "DECISION", title: "You approved the database migration", detail: "", quote: "Don't modify existing customer records.", git_only: false },
    TimelineEvent { time: "18:05", marker: Marker::Git, label: "", title: "Branch alex/auth-refresh-lock created from main @ 7d0e912", detail: "", quote: "", git_only: true },
    TimelineEvent { time: "18:04", marker: Marker::Start, label: "WORK STARTED", title: "You asked Alex to investigate the authentication issue", detail: "", quote: "Investigate the authentication issue and fix it if you're confident about the cause.", git_only: false },
    TimelineEvent { time: "17:51", marker: Marker::Minor, label: "COMPLETED", title: "Maya finished the competitor analysis", detail: "", quote: "", git_only: false },
];

pub const CAUSALITY: &[(&str, &str, &str, bool)] = &[
    ("├─ ", "Your request", "18:04", true),
    ("├─ ", "Branch  from 7d0e912", "", false),
    ("├─ ", "Investigation", "", false),
    ("│  ├─ ", "Worker A · 51fa3e0", "", false),
    ("│  └─ ", "Worker B · repro", "", false),
    ("├─ ", "Decision", "18:31", true),
    ("├─ ", "Implementation · 9c2d4aa", "", false),
    ("├─ ", "Review · e41b07c", "", false),
    ("├─ ", "Merge approval", "18:41", true),
    ("└─ ", "a3f9c21 on main", "18:42", true),
];

// ── Memory ──────────────────────────────────────────────────────────────

pub struct MemoryItem {
    pub category: &'static str,
    pub text: &'static str,
    pub provenance: &'static str,
    pub explicit: bool,
}

pub const PROJECT_MEMORY: &[MemoryItem] = &[
    MemoryItem { category: "Architecture", text: "Atlas uses SQLite for local persistence.", provenance: "Learned · Sep 18", explicit: false },
    MemoryItem { category: "Product", text: "Offline-first is a core product requirement.", provenance: "Explicit decision · Sep 21", explicit: true },
    MemoryItem { category: "Customers", text: "Existing customer records are never modified by migrations.", provenance: "Explicit decision · Today", explicit: true },
];

pub const MANAGER_MEMORY: &[MemoryItem] = &[
    MemoryItem { category: "Process", text: "Database migrations always require approval.", provenance: "Explicit instruction · Oct 1", explicit: true },
    MemoryItem { category: "Style", text: "Prefer targeted fixes over broad refactoring.", provenance: "Learned from 4 interactions", explicit: false },
    MemoryItem { category: "Constraints", text: "Don't change the public API without asking.", provenance: "Explicit instruction · Today", explicit: true },
];
