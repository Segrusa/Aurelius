//! Root view: routing, keyboard actions, overlays and the app shell
//! (sidebar with the company's pages, content, right panel, footer). Plan v0.9 phase 2.

use crate::backend::{self, Connection, Update};
use crate::keymap::*;
use crate::theme::{self, Fonts, c};
use crate::ui::{self, h, v};
use crate::preview;
use aurelius_domain::{
    ApprovalId, ConversationId, EmployeeId, MessageId, ProjectId,
    conversation::{MessageRole, MessageStatus},
    employee::LifecycleStatus,
};
use aurelius_protocol::{ApprovalView, ProvidersSnapshot, TaskView, JobSummary, EmployeeSummary, MessagesSnapshot, NotificationView, NotificationsSnapshot, ProjectSummary};
use std::collections::HashMap;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectRef {
    Real(ProjectId),
    Sample(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Overview,
    /// The board with only this project's tickets: the Board page, its project filter fixed.
    Board,
    Tasks,
    Ideas,
    /// The project's memory list: not a tab of the bar, opened from the project menu
    /// (and from Memory in the app menu, with a project picker).
    Memory,
    /// The project's vault: the vault page on this project's scope (ui-ontwerp Fase 4 §1).
    Vault,
}

impl Tab {
    /// The tabs of the project bar.
    pub const ALL: [Tab; 5] = [Tab::Overview, Tab::Board, Tab::Tasks, Tab::Ideas, Tab::Vault];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Board => "Board",
            Tab::Tasks => "Tasks",
            Tab::Ideas => "Ideas",
            Tab::Memory => "Memory",
            Tab::Vault => "Vault",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ManagerRef {
    /// A design sample: (project, employee) indices in `preview::PROJECTS`.
    Sample(usize, usize),
    Real(EmployeeId),
}

impl ManagerRef {
    /// A sample employee's sample project; real employees belong to no project.
    pub fn sample_project(self) -> Option<ProjectRef> {
        match self {
            ManagerRef::Sample(p, _) => Some(ProjectRef::Sample(p)),
            ManagerRef::Real(_) => None,
        }
    }
}

/// What the main area shows; picked in the sidebar.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Dashboard,
    /// The board: tickets in six columns (protocol 4).
    Board,
    /// The group chat ("Team chat"): the user and every employee.
    GroupChat,
    /// Employees, with the tabs of `EmployeesTab` (list, org chart, guidelines).
    Employees,
    Employee(ManagerRef),
    /// Projects, with the tabs of `ProjectsTab` (projects, ideas of every project).
    Projects,
    Project(ProjectRef, Tab),
    /// Every task of every project, with filters.
    Tasks,
    /// Approvals, questions, plans, proposals and results across all projects.
    Inbox,
    Settings,
    /// The vault of the company and its projects (feature `vault`, Fase 4).
    Vault,
}

/// The tabs of the Employees page.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EmployeesTab {
    #[default]
    List,
    OrgChart,
    /// The guideline library and its editor (feature `guidelines`, P-51).
    Guidelines,
}

impl EmployeesTab {
    pub const ALL: [EmployeesTab; 3] = [EmployeesTab::List, EmployeesTab::OrgChart, EmployeesTab::Guidelines];

    pub fn label(self) -> &'static str {
        match self {
            EmployeesTab::List => "List",
            EmployeesTab::OrgChart => "Org chart",
            EmployeesTab::Guidelines => "Guidelines",
        }
    }
}

/// The tabs of the Projects page.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ProjectsTab {
    #[default]
    Projects,
    /// The ideas of every project, each with its project; filter by project.
    Ideas,
}

impl ProjectsTab {
    pub const ALL: [ProjectsTab; 2] = [ProjectsTab::Projects, ProjectsTab::Ideas];

    pub fn label(self) -> &'static str {
        match self {
            ProjectsTab::Projects => "Projects",
            ProjectsTab::Ideas => "Ideas",
        }
    }
}

/// The right panel beside the page: a conversation or a task opened from the page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    None,
    Chat(ConversationId),
    Task(aurelius_domain::TaskId),
    /// A ticket of the board (`screens/ticket.rs`).
    Ticket(aurelius_domain::TicketId),
}

/// One entry of the sidebar, top to bottom (Settings has no row: Ctrl+, , Ctrl+9, the app menu and the server picker open it).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavItem {
    Dashboard,
    /// The board, right under Dashboard (D-02).
    Board,
    /// The group chat, right under Dashboard.
    Chat,
    Inbox,
    Tasks,
    Employees,
    Projects,
    /// The vault, under Projects (P-41).
    Vault,
    Settings,
}

impl NavItem {
    /// The name in ui.json's `sidebar_folded` (only Employees and Projects fold).
    pub fn fold_key(self) -> &'static str {
        if self == NavItem::Employees { "employees" } else { "projects" }
    }

    pub const ALL: [NavItem; 9] = [NavItem::Dashboard, NavItem::Board, NavItem::Chat, NavItem::Inbox, NavItem::Tasks, NavItem::Employees, NavItem::Projects, NavItem::Vault, NavItem::Settings];

    pub fn label(self) -> &'static str {
        match self {
            NavItem::Dashboard => "Dashboard",
            NavItem::Board => "Board",
            NavItem::Chat => "Team chat",
            NavItem::Inbox => "Inbox",
            NavItem::Tasks => "Tasks",
            NavItem::Employees => "Employees",
            NavItem::Projects => "Projects",
            NavItem::Vault => "Vault",
            NavItem::Settings => "Settings",
        }
    }

    /// Its shortcut. The numbers keep their meaning from before the sidebar was cut down:
    /// Ctrl+4 (org chart) and Ctrl+0 (ideas) open a tab of Employees / Projects (see
    /// `keymap.rs`); Team chat took the free Ctrl+8, the Board the freed Ctrl+5 (D-02).
    pub fn keys(self) -> &'static str {
        match self {
            NavItem::Dashboard => "secondary-1",
            NavItem::Board => "secondary-5",
            NavItem::Chat => "secondary-8",
            NavItem::Inbox => "secondary-2",
            NavItem::Employees => "secondary-3",
            NavItem::Projects => "secondary-6",
            NavItem::Tasks => "secondary-7",
            // P-41 asks Ctrl+9, which Settings keeps: the vault takes Ctrl+Shift+V.
            NavItem::Vault => "secondary-shift-v",
            NavItem::Settings => "secondary-9",
        }
    }

    /// The sidebar entry a page belongs to.
    pub fn of(page: Page) -> NavItem {
        match page {
            Page::Dashboard => NavItem::Dashboard,
            Page::Board => NavItem::Board,
            Page::GroupChat => NavItem::Chat,
            Page::Employees | Page::Employee(..) => NavItem::Employees,
            Page::Projects | Page::Project(..) => NavItem::Projects,
            Page::Tasks => NavItem::Tasks,
            Page::Inbox => NavItem::Inbox,
            Page::Settings => NavItem::Settings,
            Page::Vault => NavItem::Vault,
        }
    }
}

/// The inbox's filter tabs.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum InboxTab {
    #[default]
    NeedsYou,
    Results,
    Failed,
    All,
}

impl InboxTab {
    pub const ALL: [InboxTab; 4] = [InboxTab::NeedsYou, InboxTab::Results, InboxTab::Failed, InboxTab::All];

    pub fn label(self) -> &'static str {
        match self {
            InboxTab::NeedsYou => "Needs you",
            InboxTab::Results => "Results",
            InboxTab::Failed => "Failed",
            InboxTab::All => "All",
        }
    }

    /// Whether the tab lists the pending approvals and proposals.
    pub fn shows_head(self) -> bool {
        matches!(self, InboxTab::NeedsYou | InboxTab::All)
    }

    pub fn shows(self, kind: NoteKind) -> bool {
        match kind {
            NoteKind::Hidden => false,
            NoteKind::NeedsYou => self.shows_head(),
            NoteKind::Failed => self != InboxTab::NeedsYou,
            NoteKind::Result => matches!(self, InboxTab::Results | InboxTab::All),
        }
    }
}

/// What an inbox notification is to the user.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NoteKind {
    /// An alert about an approval or proposal, which the inbox lists itself.
    Hidden,
    /// Waits for the user (an escalated task).
    NeedsYou,
    Failed,
    Result,
}

pub fn note_kind(n: &NotificationView) -> NoteKind {
    match (n.severity.as_str(), n.target_type.as_str()) {
        // The inbox lists these itself, a guideline's proposal too (B5): one row each, gone once decided.
        ("action", "approval.requested" | "employee.proposed" | "employee.instructions_proposed" | "guideline.proposed") => NoteKind::Hidden,
        ("action", _) => NoteKind::NeedsYou,
        (_, "task.failed") => NoteKind::Failed,
        _ => NoteKind::Result,
    }
}

/// The notifications a tab lists: those that need you first, then the rest newest first.
pub fn tab_notes<'a>(tab: InboxTab, notes: &[&'a NotificationView]) -> Vec<&'a NotificationView> {
    let mut out: Vec<&NotificationView> = notes.iter().copied().filter(|n| tab.shows(note_kind(n))).collect();
    out.sort_by_key(|n| note_kind(n) != NoteKind::NeedsYou);
    out
}

/// Items per tab, in `InboxTab::ALL` order; `head` is the pending approvals and proposals.
/// The first (Needs you) is the sidebar badge.
pub fn tab_counts(head: usize, notes: &[&NotificationView]) -> [usize; 4] {
    InboxTab::ALL.map(|t| if t.shows_head() { head } else { 0 } + tab_notes(t, notes).len())
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DialogKind {
    NewProject,
    /// Add an employee.
    NewManager,
    NewTask { project: ProjectId },
    /// A recurring task: new (`None`) or the one being edited (protocol 1.14).
    TaskSchedule { project: ProjectId, schedule: Option<aurelius_domain::TaskScheduleId> },
    EditManager { id: EmployeeId, version: u64 },
    NewMemory { project: ProjectId },
    NewIdea { project: ProjectId },
    EditIdea { id: aurelius_domain::IdeaId },
    ReviseMemory { id: aurelius_domain::MemoryId },
    /// The user's own answer to question `index` of a question in the inbox.
    /// `in_inbox`: opened from the inbox (else from the question's card in the conversation).
    QuestionAnswer { approval: ApprovalId, index: usize, in_inbox: bool },
    /// Approve an instruction proposal with the user's edits (feature `instruction_proposals`).
    EditProposal { proposal: aurelius_domain::ProposalId },
}

/// Claude Code's question tool, as an approval's tool name.
pub const QUESTION_TOOL: &str = "claude.AskUserQuestion";

/// Claude Code's plan tool (ExitPlanMode), as an approval's tool name.
pub const PLAN_TOOL: &str = "claude.ExitPlanMode";

/// The mode picker's default for a management lead: it picks Auto or Plan itself.
pub const MANAGER_CHOOSES: &str = "Manager chooses";
/// The group chat composer's placeholder.
pub const GROUP_PLACEHOLDER: &str = "Message the team… (@Name asks someone)";

/// One inline comment on a plan: the selected fragment and what the user says about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanComment {
    pub quote: String,
    pub text: String,
}

/// The feedback for the agent: per comment a blockquote of the fragment followed by the
/// comment, then the free text. Empty when there is nothing.
pub fn compose_plan_feedback(comments: &[PlanComment], free: &str) -> String {
    let mut parts: Vec<String> = comments
        .iter()
        .map(|c| {
            let quote = c.quote.trim().lines().map(|l| if l.trim().is_empty() { ">".to_string() } else { format!("> {}", l.trim_end()) }).collect::<Vec<_>>().join("\n");
            format!("{quote}\n\n{}", c.text.trim())
        })
        .collect();
    if !free.trim().is_empty() {
        parts.push(free.trim().to_string());
    }
    parts.join("\n\n")
}

/// What the plan of an approval is as text (markdown); empty when there is none.
pub fn plan_text_of(a: &ApprovalView) -> String {
    a.arguments["plan"].as_str().unwrap_or("").to_string()
}

/// The plan's first heading (or first line), for lists.
pub fn plan_headline_of(a: &ApprovalView) -> String {
    plan_text_of(a)
        .lines()
        .map(|l| l.trim().trim_start_matches('#').trim().to_string())
        .find(|l| !l.is_empty())
        .unwrap_or_else(|| "A plan".to_string())
}

/// What a conversation waits for from the user, shown next to its name in the lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeedsYou {
    Answer,
    Plan,
    Approve,
    Decide,
}

impl NeedsYou {
    pub fn label(self) -> &'static str {
        match self {
            NeedsYou::Answer => "Answer",
            NeedsYou::Plan => "Review plan",
            NeedsYou::Approve => "Approve",
            NeedsYou::Decide => "Decide",
        }
    }

    pub fn color(self) -> u32 {
        theme::ACCENT_TEXT
    }
}

/// What conversation `id` waits for: a pending approval of its runs (a worker's
/// approval counts through its task's conversation), else its task waiting for the user.
/// The task that owns a conversation (task conversations are not listed under their employee).
pub fn task_of_conversation(tasks: &[TaskView], conversation: ConversationId) -> Option<aurelius_domain::TaskId> {
    tasks.iter().find(|t| t.conversation_id == Some(conversation)).map(|t| t.id)
}

pub fn needs_you_of(approvals: &[ApprovalView], tasks: &[TaskView], id: ConversationId) -> Option<NeedsYou> {
    let task = tasks.iter().find(|t| t.conversation_id == Some(id));
    let pending: Vec<&ApprovalView> = approvals
        .iter()
        .filter(|a| a.status == aurelius_domain::approval::ApprovalStatus::Pending)
        .filter(|a| a.conversation_id == Some(id) || a.task_id.is_some_and(|t| task.is_some_and(|task| task.id == t)))
        .collect();
    let waiting = task.filter(|t| t.status == aurelius_domain::task::TaskStatus::Waiting).and_then(|t| t.waiting_reason.as_deref());
    if pending.iter().any(|a| a.tool == QUESTION_TOOL) {
        Some(NeedsYou::Answer)
    } else if pending.iter().any(|a| a.tool == PLAN_TOOL) {
        Some(NeedsYou::Plan)
    } else if !pending.is_empty() {
        Some(NeedsYou::Approve)
    } else {
        match waiting {
            Some("input") => Some(NeedsYou::Decide),
            _ => None,
        }
    }
}

/// How a plan in the drawer stands: open, being sent, decided, or no longer ours to decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanStatus {
    Open,
    Sending(aurelius_domain::approval::PlanDecision),
    Decided(aurelius_domain::approval::PlanDecision),
    /// Decided on another device (or gone) while the drawer was open.
    HandledElsewhere,
    Expired,
}

/// One of Claude's questions (AskUserQuestion), as shown in the inbox.
#[derive(Clone, Debug)]
pub struct Question {
    pub text: String,
    pub header: String,
    pub multi: bool,
    /// (label, description)
    pub options: Vec<(String, String)>,
}

pub fn questions_of(a: &ApprovalView) -> Vec<Question> {
    let s = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
    a.arguments["questions"]
        .as_array()
        .map(|qs| {
            qs.iter()
                .map(|q| Question {
                    text: s(&q["question"]),
                    header: s(&q["header"]),
                    multi: q["multiSelect"].as_bool().unwrap_or(false),
                    options: q["options"].as_array().map(|o| o.iter().map(|o| (s(&o["label"]), s(&o["description"]))).collect()).unwrap_or_default(),
                })
                .filter(|q| !q.text.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// What the Ideas list is filtered by: scope, project, status tab, author, kind.
pub type IdeasKey = (Option<ProjectId>, Option<ProjectId>, usize, IdeaAuthor, Option<aurelius_domain::idea::IdeaKind>);

/// Whose ideas the Ideas tab shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IdeaAuthor {
    Anyone,
    You,
    Managers,
}

impl IdeaAuthor {
    pub const ALL: [IdeaAuthor; 3] = [IdeaAuthor::Anyone, IdeaAuthor::You, IdeaAuthor::Managers];

    pub fn label(self) -> &'static str {
        match self {
            IdeaAuthor::Anyone => "Everyone",
            IdeaAuthor::You => "Yours",
            IdeaAuthor::Managers => "Employees'",
        }
    }

    pub fn next(self) -> Self {
        match self {
            IdeaAuthor::Anyone => IdeaAuthor::You,
            IdeaAuthor::You => IdeaAuthor::Managers,
            IdeaAuthor::Managers => IdeaAuthor::Anyone,
        }
    }

    pub fn admits(self, idea: &aurelius_protocol::IdeaView) -> bool {
        match self {
            IdeaAuthor::Anyone => true,
            IdeaAuthor::You => !idea.by_employee(),
            IdeaAuthor::Managers => idea.by_employee(),
        }
    }
}

/// Ideas split by who noted them, each keeping its order: yours first, then the managers'.
pub fn split_ideas<I>(ideas: I) -> (Vec<I::Item>, Vec<I::Item>)
where
    I: IntoIterator,
    I::Item: std::ops::Deref<Target = aurelius_protocol::IdeaView>,
{
    ideas.into_iter().partition(|i| !i.by_employee())
}

/// What a working manager is doing, for the home page: the title of its task or
/// conversation, and when it started (ms, 0 when unknown). Task work comes first,
/// then its own conversation, then colleagues it waits on.
pub fn work_of(agents: &[aurelius_protocol::AgentView]) -> Option<(String, i64)> {
    let rank = |kind: &str| match kind {
        "planning" | "follow_up" => 0,
        "conversation" => 1,
        "delegated" => 2,
        _ => 3,
    };
    let agent = agents.iter().filter(|a| rank(&a.kind) < 3).min_by_key(|a| (rank(&a.kind), a.since_ms))?;
    let title = match agent.kind.as_str() {
        "conversation" => agent.name.clone(),
        "delegated" => format!("Waiting on {}: {}", agent.name, agent.detail),
        _ if agent.detail.trim().is_empty() => agent.name.clone(),
        _ => agent.detail.clone(),
    };
    Some((title, agent.since_ms))
}

/// A manager that is working right now, listed on the home page.
pub struct WorkingNow {
    pub manager: EmployeeId,
    /// The project of its task at work (none: its chat).
    pub project: Option<ProjectId>,
    pub name: String,
    pub project_name: String,
    pub title: String,
    pub since_ms: i64,
}

/// An employee's answer streaming into the open group chat that is no message there yet
/// (it is copied in when it ends).
#[derive(Clone, Debug)]
pub struct GroupLive {
    pub message: MessageId,
    pub run: aurelius_domain::RunId,
    /// When the last delta came.
    pub at: std::time::Instant,
    /// Who writes it, kept from while the run worked: once it ends, the run no longer
    /// names its employee, but the message may not be in yet.
    pub author: Option<String>,
    /// The mode its run started in (shown with the answer).
    pub mode: Option<aurelius_domain::autonomy::AgentMode>,
}

/// What waits for the user in one project.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Waiting {
    pub approvals: u32,
    pub decisions: u32,
}

impl Waiting {
    pub fn total(self) -> u32 {
        self.approvals + self.decisions
    }
}

impl AppView {
    /// Managers working right now across all projects, longest running first.
    pub fn working_now(&self) -> Vec<WorkingNow> {
        let mut out: Vec<WorkingNow> = self
            .managers
            .iter()
            .filter(|m| m.working && m.status != LifecycleStatus::Archived)
            .map(|m| {
                // Where it works: the project of its task at work, else its chat (no project).
                let task = m.agents.iter().find_map(|a| a.task_id).and_then(|id| self.tasks.iter().find(|t| t.id == id));
                let project = task.and_then(|t| self.projects.iter().find(|p| p.id == t.project_id));
                let (title, since_ms) = work_of(&m.agents).unwrap_or_else(|| ("Working on an answer".into(), 0));
                WorkingNow { manager: m.id, project: project.map(|p| p.id), name: m.name.clone(), project_name: project.map_or_else(|| "Chat".to_string(), |p| p.name.clone()), title, since_ms }
            })
            .collect();
        out.sort_by_key(|w| if w.since_ms > 0 { w.since_ms } else { i64::MAX });
        out
    }

    /// Approvals and decisions waiting for the user in a real project.
    pub fn waiting_in(&self, project: ProjectId) -> Waiting {
        use aurelius_domain::task::TaskStatus;
        let task_project = |id| self.tasks.iter().find(|t| t.id == id).map(|t| t.project_id);
        // An employee's chat belongs to no project; a task conversation to its task's.
        let conversation_project = |id: ConversationId| self.tasks.iter().find(|t| t.conversation_id == Some(id)).map(|t| t.project_id);
        let approvals = self
            .pending_approvals()
            .filter(|a| a.task_id.and_then(task_project).or_else(|| a.conversation_id.and_then(conversation_project)) == Some(project))
            .count() as u32;
        let decisions = self
            .tasks
            .iter()
            .filter(|t| t.project_id == project && t.status == TaskStatus::Waiting && t.pending_approvals == 0)
            .filter(|t| t.waiting_reason.as_deref() == Some("input"))
            .count() as u32;
        Waiting { approvals, decisions }
    }

    /// Tasks of a project that are not finished.
    pub fn active_task_count(&self, project: ProjectId) -> usize {
        self.tasks.iter().filter(|t| t.project_id == project && crate::ui::is_active(t)).count()
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ConfirmKind {
    ArchiveManager(EmployeeId),
    ArchiveProject(ProjectId),
    ForgetMemory(aurelius_domain::MemoryId),
    /// Delete a recurring task; tasks it already made stay.
    DeleteSchedule(aurelius_domain::TaskScheduleId),
    /// Delete an idea with its files; a task it became stays.
    DeleteIdea(aurelius_domain::IdeaId),
    DeleteConversation(ConversationId),
    /// Index into the backups list.
    RestoreBackup(usize),
    /// Let the agent of this conversation use the user's own browser.
    AllowBrowser(ConversationId),
    /// Let the agent of this conversation take screenshots without asking.
    AllowScreen(ConversationId),
    /// Restart the connected server; running answers are interrupted.
    RestartServer,
    /// Shut down the PC that runs the connected server, after a short countdown.
    ShutdownNow,
    /// Clear every result (inbox notification); approvals stay.
    ClearResults,
    /// End a session (MU3-04): its device, and whose list it is in (none: the own).
    RevokeSession(aurelius_domain::SessionId, Option<aurelius_domain::UserId>),
    /// End every own session but this device's.
    RevokeOtherSessions,
    /// Disable a board member (MU3-05 AC3).
    DisableMember(aurelius_domain::UserId),
    /// Disconnect the own Claude login (MU3-09 AC4).
    DisconnectClaude,
    /// Run a ticket that waits for another's login on the own Claude login (B4).
    UseMyLogin(aurelius_domain::TicketId),
    /// Delete ("archive") a vault item (MU4-11 AC2).
    ArchiveVaultItem(aurelius_domain::VaultItemId),
    /// A message held back by `looks_like_secret` (B10, `vault.held`): "Send anyway?".
    LooksLikeSecret,
}

/// One manager card, from the daemon or from the design samples.
#[derive(Clone)]
pub struct ManagerCard {
    pub key: ManagerRef,
    pub name: SharedString,
    pub role: SharedString,
    /// The position on the org chart, if it has one.
    pub title: Option<SharedString>,
    pub summary: SharedString,
    pub working: bool,
    pub status: LifecycleStatus,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Overlay {
    None,
    Palette,
    Dialog(DialogKind),
    Review,
    Confirm(ConfirmKind),
    Context,
    Usage,
    /// A screenshot the agent took, in the side panel.
    Screenshot,
    /// The Markdown viewer (documents of a work folder), in the side panel.
    Markdown,
    /// What one answer's run sent and what each model call cost, in the side panel.
    Trace,
    /// A Claude Code artifact's kept content, in the side panel.
    Artifact,
    /// A plan Claude handed in (ExitPlanMode), decided in the right-hand drawer.
    PlanApproval(ApprovalId),
    /// A small popup menu (⋯ or right-click) with the item's actions.
    Menu,
    /// The folder browser (`fs/list_dir`), for this computer and remote servers.
    ServerFolder,
    /// Connect your Claude login (ui-ontwerp §4): the link and the code, with the
    /// same-OS-user notice (Sentinel B1-3).
    ConnectClaude,
    /// "You were signed out on this device." (ui-ontwerp §1): the server ended this
    /// device's session; OK shows the sign-in, which already waits beneath it.
    SignedOut,
    /// A guideline dialog: Links, Add starter set, Reject with a note, a confirmation.
    Guidelines,
}

/// What a menu entry does: dispatch an action (same as its shortcut) or call
/// the view directly when the action's meaning depends on the screen.
#[derive(Clone)]
pub enum MenuDo {
    Action(fn() -> Box<dyn Action>),
    Call(fn(&mut AppView, &mut Window, &mut Context<AppView>)),
    /// Use this model (global default for chat or managers).
    Model(String),
    /// Use this effort level; `None` = the runtime's default.
    Effort(Option<String>),
    /// Start this idea as a task for this manager.
    StartIdea(aurelius_domain::IdeaId, EmployeeId),
    /// Take this project out of the sidebar.
    Unpin(ProjectId),
    /// Pin this employee under Employees in the sidebar, or take it out again.
    TogglePinEmployee(EmployeeId),
    /// Open this project's memory list.
    OpenMemory(ProjectId),
    /// Make a recurring task in this project.
    NewSchedule(ProjectId),
    /// The mode the idea's task starts in (`None`: the manager's default); the menu stays open.
    IdeaMode(Option<aurelius_domain::autonomy::AgentMode>),
    /// The model the idea's task runs on (`None`: the employee's default); keeps the menu open.
    IdeaModel(Option<(aurelius_domain::ProviderId, String)>),
    /// Mark this idea open, done or dropped.
    IdeaStatus(aurelius_domain::IdeaId, aurelius_domain::idea::IdeaStatus),
    /// Give this task to this manager (`None`: a draft again).
    Assign(aurelius_domain::TaskId, Option<EmployeeId>),
    /// A choice in one of the new task dialog's dropdowns.
    Task(TaskChoice),
    /// A board or ticket action (`screens/board.rs`).
    Board(crate::screens::board::BoardDo),
}

/// New task dialog: a value picked from one of its dropdowns (`None`: as the manager / off).
#[derive(Clone)]
pub enum TaskChoice {
    Manager(Option<EmployeeId>),
    /// Dashboard "Give work": who gets it, and in which project.
    GiveManager(EmployeeId),
    GiveProject(ProjectId),
    /// Tasks page filters (`None`: any).
    FilterEmployee(Option<EmployeeId>),
    FilterProject(Option<ProjectId>),
    /// Ideas page filter (`None`: every project).
    IdeasProject(Option<ProjectId>),
    /// New idea dialog: the project the idea is added to.
    IdeaDialogProject(ProjectId),
    Mode(Option<aurelius_domain::autonomy::AgentMode>),
    Model(Option<(aurelius_domain::ProviderId, String)>),
    Effort(Option<String>),
    /// Edit manager: the manager's default model (None: inherit).
    ManagerModel(Option<(aurelius_domain::ProviderId, String)>),
    /// Add / Edit manager: who it reports to (None: nobody, a root of the org chart).
    ReportsTo(Option<EmployeeId>),
    /// Edit manager: the manager's default effort (None: inherit).
    ManagerEffort(Option<String>),
    /// Subagents: a model (None: inherit). In the Edit manager dialog for that manager,
    /// in Settings the global default.
    DelegationModel(DelegationRow, Option<(aurelius_domain::ProviderId, String)>),
}

/// The run defaults for the Claude subagents a run starts (`Purpose::Subagent`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DelegationRow {
    Subagents,
}

impl DelegationRow {
    pub const ALL: [DelegationRow; 1] = [DelegationRow::Subagents];

    pub fn purpose(self) -> aurelius_domain::connection::Purpose {
        aurelius_domain::connection::Purpose::Subagent
    }

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone)]
pub struct MenuEntry {
    pub label: SharedString,
    pub keys: Option<&'static str>,
    pub run: MenuDo,
    pub danger: bool,
    pub checked: bool,
    pub detail: Option<SharedString>,
}

/// `None` is a separator.
pub type MenuItems = Vec<Option<MenuEntry>>;

pub fn entry(label: impl Into<SharedString>, keys: &'static str, action: fn() -> Box<dyn Action>) -> Option<MenuEntry> {
    Some(MenuEntry { label: label.into(), keys: (!keys.is_empty()).then_some(keys), run: MenuDo::Action(action), danger: false, checked: false, detail: None })
}

pub fn call(label: impl Into<SharedString>, keys: &'static str, f: fn(&mut AppView, &mut Window, &mut Context<AppView>)) -> Option<MenuEntry> {
    Some(MenuEntry { label: label.into(), keys: (!keys.is_empty()).then_some(keys), run: MenuDo::Call(f), danger: false, checked: false, detail: None })
}

/// The main task of a ticket: the Board starts, gives and stops it, not the Tasks tab
/// (`task.assign` / `task.cancel` refuse it). A delegation below it carries the ticket too.
pub fn is_ticket_main_task(t: &aurelius_protocol::TaskView) -> bool {
    t.ticket_id.is_some() && t.parent_task_id.is_none()
}

/// Why Cancel task and Give to… do not apply to this task, if they do not.
pub fn use_the_board(t: &aurelius_protocol::TaskView) -> Option<String> {
    is_ticket_main_task(t).then(|| match t.ticket_number {
        Some(n) => format!("This is the main task of ticket #{n}: assign or cancel it on the Board."),
        None => "This is the main task of a ticket: assign or cancel it on the Board.".into(),
    })
}

pub fn danger(e: Option<MenuEntry>) -> Option<MenuEntry> {
    e.map(|e| MenuEntry { danger: true, ..e })
}

pub struct Menu {
    pub anchor: Point<Pixels>,
    pub entries: MenuItems,
    pub selected: usize,
    /// A dropdown is as wide as its field; other menus size to their entries.
    pub width: Option<f32>,
    /// The dropdown field this menu belongs to.
    pub field: Option<&'static str>,
    /// Top edge of the trigger: the menu opens upward from here when there is no room below.
    pub trigger_top: Option<f32>,
    /// The dialog the menu opened over (and what had focus there): it stays open beneath.
    pub parent: Option<(Overlay, Option<FocusHandle>)>,
}

pub struct Notice {
    pub text: SharedString,
    pub error: bool,
}

pub struct AppView {
    pub focus: FocusHandle,
    pub screen: Page,
    /// The open tab of the Employees page.
    pub employees_tab: EmployeesTab,
    /// The open tab of the Projects page.
    pub projects_tab: ProjectsTab,
    /// What the right panel shows beside the page.
    pub panel: Panel,
    pub back_stack: Vec<Page>,
    /// Browser-like history of places (screen or chat) for ← and →.
    history_back: Vec<Place>,
    history_forward: Vec<Place>,
    last_place: Option<Place>,
    /// The app sidebar is hidden (Ctrl+B; ui.json `sidebar_hidden`).
    pub sidebar_collapsed: bool,
    /// Hidden, it shows over the page while the mouse is on its title strip button or in it.
    pub sidebar_peek: bool,
    /// Projects pinned under Projects in the sidebar, in pin order, at most `MAX_PINS` (per server, in ui.json).
    pub pins: Vec<ProjectId>,
    /// Employees pinned under Employees in the sidebar, same rules as `pins`.
    pub employee_pins: Vec<EmployeeId>,
    pin_server: String,
    /// Sidebar items (Employees, Projects) whose pinned rows are folded away (ui.json `sidebar_folded`).
    pub folded: Vec<NavItem>,
    /// Side panel widths the user dragged to (by panel), kept in ui.json.
    pub panel_widths: HashMap<String, f32>,
    pub panel_save: Option<Task<()>>,
    pub selected: usize,
    pub task_filter: usize,
    pub history_open: bool,
    pub approval_done: bool,
    pub show_preview: bool,
    pub overlay: Overlay,

    pub palette_input: Entity<InputState>,
    pub palette_selected: usize,
    pub name_input: Entity<InputState>,
    pub description_input: Entity<InputState>,
    pub role_input: Entity<InputState>,
    pub instructions_input: Entity<InputState>,
    /// Add / Edit manager: the position on the org chart ("Head of engineering").
    pub title_input: Entity<InputState>,
    /// Add / Edit manager: who the manager reports to (`None`: a root of the org chart).
    pub pending_reports_to: Option<EmployeeId>,
    /// The company: its employees and the management team.
    pub company: Option<aurelius_protocol::CompanySnapshot>,
    pub composer: Entity<TextareaState>,
    /// Images pasted or picked for the next message (files on this computer).
    pub pending_images: Vec<std::path::PathBuf>,
    /// Files picked or dropped for the next message to a manager, with their
    /// size; the server copies them into the conversation's work folder.
    pub pending_files: Vec<(std::path::PathBuf, u64)>,
    /// Feedback on the plan in the drawer.
    pub plan_input: Entity<InputState>,
    /// The plan in the drawer as it was when opened (it may leave the inbox while open).
    pub plan_snapshot: Option<ApprovalView>,
    /// Inline comments on the plan in review: quoted fragment + comment. Cleared after a decision.
    pub plan_comments: Vec<PlanComment>,
    /// The fragment a comment is being written on (the 'Comment' button was pressed).
    pub plan_comment_draft: Option<String>,
    /// The text selected in the plan right now (set on mouse-up), offered for a comment.
    pub plan_selection: Option<String>,
    /// The comment text field.
    pub plan_comment_input: Entity<InputState>,
    /// A decision on its way for a plan: the approval and what was chosen.
    pub plan_sending: Option<(ApprovalId, aurelius_domain::approval::PlanDecision)>,
    pub managers: Vec<EmployeeSummary>,
    /// Colleagues managers proposed: pending first, then the latest decisions.
    pub proposals: Vec<aurelius_protocol::ProposalView>,
    pub approvals: Vec<ApprovalView>,
    pub tasks: Vec<TaskView>,
    pub connections: Option<ProvidersSnapshot>,
    pub settings_selected: usize,
    /// Model chosen in settings per connection (cycled with M).
    pub settings_model: HashMap<aurelius_domain::ProviderId, usize>,
    /// Some(provider) while choosing a provider for a new connection.
    pub adding_connection: bool,
    pub key_input: Entity<InputState>,
    pub entering_key: bool,
    /// Settings → Remote access (tab 4).
    pub remote: Option<aurelius_protocol::remote::RemoteStatus>,
    pub pairing: Option<aurelius_protocol::remote::PairingCode>,
    /// Bumped when the remote tab opens; an older poll loop then stops.
    pub remote_poll: u64,
    pub pending_push: Option<crate::screens::remote::PendingPushKey>,
    /// This app's own data folder (UI prefs, paired servers).
    pub data_root: Option<std::path::PathBuf>,
    /// Settings → Servers: the servers this computer is paired with.
    pub servers: aurelius_client::servers::Servers,
    /// The server in use when it is another computer: (host id, name).
    pub remote_host: Option<(aurelius_domain::HostId, String)>,
    /// Board members: who is signed in in this window, the onboarding and the sign-in.
    pub members: crate::screens::board_members::Members,
    /// The vault page: its lists, a value shown for 30 s, the editor, the key export (Fase 4).
    pub vault: crate::screens::vault::Vault,
    /// Guidelines: the library, the editor, the sets per employee, the proposals, the dialogs.
    pub guidelines: crate::screens::guidelines::Guidelines,
    /// The signed-in board member's own Claude login (Settings → Account).
    pub claude: crate::screens::account::Claude,
    /// The board, the open ticket and their dialogs.
    pub board: crate::screens::board::Board,
    pub adding_server: bool,
    pub server_input: Entity<InputState>,
    /// Settings → Server & PC: the server's name and its field (feature `server_name`).
    pub server_name: crate::screens::server_name::ServerName,
    pub server_name_input: Entity<InputState>,
    /// The folder browser's typed path (Ctrl+L) and new folder name.
    pub folder_path_input: Entity<InputState>,
    pub folder_name_input: Entity<InputState>,
    /// Pairing with a server: `Some(host)` while waiting for "Allow as owner".
    pub pairing_wait: Option<String>,
    pub folder: Option<crate::screens::servers::FolderBrowser>,
    /// Bumped per backend; updates of an earlier backend are ignored.
    backend_epoch: u64,
    /// Open Settings → Servers on the next render (needs the window).
    pending_servers: bool,
    pending_notice: Option<String>,
    pub team_input: Entity<InputState>,
    /// The server's load and limits, live (Settings → Load limits, "Server busy").
    pub load: Option<aurelius_protocol::LoadSnapshot>,
    /// The server's pending PC shutdown (banner in the status bar).
    pub power: Option<aurelius_protocol::PendingShutdown>,
    /// Why there is no load (a server before 1.10).
    pub load_error: Option<String>,
    /// Settings → Load limits: the edited limits and whether they show the server's.
    pub load_form: crate::screens::load::LoadForm,
    /// Agent functions, their calls and API keys.
    /// 0 = connections, 1 = folders
    pub settings_tab: usize,
    pub inbox_selected: usize,
    pub inbox_tab: InboxTab,
    /// Answers being composed in the inbox, per (question approval, question):
    /// chosen option indices, or the user's own text.
    pub question_choices: HashMap<(ApprovalId, usize), Vec<usize>>,
    pub question_own: HashMap<(ApprovalId, usize), String>,
    /// The question that 1–4 answer.
    pub question_focus: usize,
    pub notifications: Option<NotificationsSnapshot>,
    /// Notifications that were unread when the inbox opened (shown as new).
    pub inbox_new: Vec<String>,
    /// Memory of the open project (B17).
    pub memory: Option<(ProjectId, Vec<aurelius_protocol::MemoryView>)>,
    memory_requested: Option<ProjectId>,
    /// The ideas of one project (`Some`) or of every project (`None`), and which of them show: 0 open, 1 done, 2 dropped.
    pub ideas: Option<(Option<ProjectId>, Vec<aurelius_protocol::IdeaView>)>,
    ideas_requested: Option<Option<ProjectId>>,
    pub ideas_filter: usize,
    /// Whose ideas show: everyone's, only yours, or only the managers' (key 4 cycles).
    pub ideas_author: IdeaAuthor,
    /// Ideas list: how many rows show, for the filters they were loaded under.
    pub ideas_shown: std::cell::Cell<(IdeasKey, usize)>,
    /// A project's Tasks tab: how many rows show, for the project and filter they were loaded under.
    pub project_tasks_shown: std::cell::Cell<((Option<ProjectId>, usize), usize)>,
    /// Which kind of idea shows (`None`: all); only offered by servers from 1.24.
    pub ideas_kind: Option<aurelius_domain::idea::IdeaKind>,
    /// The kind picked in the New/Edit idea dialog.
    pub pending_idea_kind: aurelius_domain::idea::IdeaKind,
    /// Remember dialog: a decision (else knowledge).
    pub pending_memory_decision: bool,
    /// New task: criteria, files and run settings shown (behind "More").
    pub task_more_open: bool,
    /// Give work on the Dashboard: the same, kept apart so a dialog doesn't fold it.
    pub give_more_open: bool,
    /// Edit employee: the Advanced section shown.
    pub employee_advanced_open: bool,
    /// Mode picked in the idea's "Start as a task" menu (`None`: the manager's default).
    idea_start_mode: Option<aurelius_domain::autonomy::AgentMode>,
    /// 0 = current (active + proposals), 1 = proposals, 2 = history.
    pub memory_filter: usize,
    /// "In context" view: the manager picked (`None` outside the Option: not picked yet,
    /// the first manager is used), the last answer, and what was asked for.
    pub memory_ctx_manager: Option<Option<EmployeeId>>,
    pub memory_ctx: Option<(ProjectId, Option<EmployeeId>, Result<aurelius_protocol::MemoryContextView, String>)>,
    memory_ctx_requested: Option<(ProjectId, Option<EmployeeId>)>,
    /// Folded sections of the "In context" view that the user unfolded.
    pub memory_over_open: bool,
    pub memory_dups_open: bool,
    pub context_view: Option<aurelius_protocol::ContextView>,
    pub usage: Option<aurelius_protocol::UsageSnapshot>,
    /// The day picked in the Usage dialog's 14-day chart; None: today.
    pub usage_day: Option<i64>,
    /// That day's usage per hour (keyed by day), or why there is none.
    pub usage_hours: Option<(i64, Result<aurelius_protocol::UsageHours, String>)>,
    usage_hours_requested: Option<i64>,
    pub subscription: Option<aurelius_protocol::SubscriptionUsage>,
    pub update: Option<aurelius_protocol::UpdateStatus>,
    pub backups: Option<aurelius_protocol::BackupsSnapshot>,
    pub menu: Option<Menu>,
    /// The open task conversation of the open employee (`None`: its most recent one).
    pub manager_thread: Option<ConversationId>,
    /// The group chat's conversation, once loaded (`group_chat.get`).
    pub group_conversation: Option<ConversationId>,
    /// Why the group chat did not load (its page shows it with Try again instead of loading).
    pub group_error: Option<String>,
    /// Answers streaming into the open group chat that are no message there yet.
    pub group_live: Vec<GroupLive>,
    /// The Team chat's channels (AUR-MU-02); none: the server has none (one general chat).
    pub channels: Option<Vec<aurelius_domain::views::ChannelView>>,
    /// The channel the Team chat shows (none: the general one).
    pub channel: Option<ConversationId>,
    /// A channel just opened with this many unread messages and the reader's last
    /// read ordinal, both as the list had them before opening (opening marks it read):
    /// where "New messages" goes is fixed when its messages arrive.
    pub channel_divider: Option<(ConversationId, usize, Option<u32>)>,
    /// The first message after the "New" divider in the open channel (it stays there while more come).
    pub channel_new_at: Option<MessageId>,
    /// The archived projects' channels are unfolded.
    pub channels_archived_open: bool,
    /// "Show more" was chosen under Tickets / under Archived (AUR-CHAT-001, O-5).
    pub channels_tickets_all: bool,
    pub channels_archived_all: bool,
    /// The ticket channels' order while the pointer is over the list: a row never jumps under the click.
    pub channel_order: Option<Vec<ConversationId>>,
    /// The search field above the channel list (`channel.search`).
    pub channel_search: Entity<InputState>,
    /// What `channel.search` found for this query (none: no search, the whole list shows).
    pub channel_results: Option<(String, Vec<aurelius_domain::views::ChannelView>)>,
    /// The open ticket channel's ticket (`ticket.get`): its title and assignees for the header.
    pub channel_ticket: Option<aurelius_domain::ticket::TicketDetail>,
    /// A post here was refused with `channel_archived`: the channel shows its read-only
    /// bar until the list says it can be written in again (a reopened ticket).
    pub channel_refused: Option<ConversationId>,
    /// The keyboard of the Team chat composer's `@` suggestions.
    pub group_mentions: crate::screens::discussion::MentionPicker,
    /// The backend reads the channels on every chat event while the Team chat shows
    /// (`Request::FollowChannels`); this window last asked for that.
    pub channels_followed: bool,
    /// The subagent chip's panel is open; which subagent shows its full prompt.
    pub subagents_open: bool,
    /// Subagents running in a conversation, from `query/subagents` (protocol 1.6).
    pub subagents: Option<(ConversationId, Vec<aurelius_protocol::AgentView>)>,
    /// The chip's conversation took a step since its subagents were loaded.
    subagents_stepped: bool,
    pub subagent_expanded: Option<String>,
    pub subagents_closed_at: Option<std::time::Instant>,
    /// Employee view, right panel: the folded "Earlier" group of tasks that ended before today is open.
    pub threads_tasks_earlier_open: bool,
    /// Background service registered to start at sign-in (read when Settings opens).
    pub login_start: bool,
    /// Which browser Claude in Chrome uses on the server (fetched when Settings opens), or why not.
    pub browser: Option<Result<aurelius_protocol::BrowserSetting, String>>,
    /// Settings → Claude artifacts on or off; `None` while loading or on servers without the setting (hidden then).
    pub artifacts_enabled: Option<bool>,
    /// What the open conversation (or its task) used, with its subtasks; `None` until loaded (feature `run_usage`).
    pub usage_totals: Option<(ConversationId, aurelius_protocol::UsageTotals)>,
    /// Trace mode on the server; `None` while loading or on servers without `run_trace`.
    pub trace_setting: Option<aurelius_protocol::TraceSetting>,
    /// Settings → MCP & tools asks before turning trace mode off (that deletes the traces).
    pub trace_confirm_off: bool,
    /// The run whose trace the side panel shows, and the trace once loaded.
    pub trace: Option<(aurelius_domain::RunId, Option<Result<aurelius_protocol::RunTrace, String>>)>,
    /// The trace panel shows the whole system text / prompt.
    pub trace_open: [bool; 2],
    /// Edit manager: the versions of the edited manager's instructions (feature `instruction_proposals`).
    pub instruction_versions: Option<(EmployeeId, Result<aurelius_protocol::EmployeeInstructionVersionsSnapshot, String>)>,
    pub instruction_versions_requested: Option<EmployeeId>,
    /// Management chooses its own task mode; `None` while loading or on servers without `management_chooses_mode`.
    pub management_mode: Option<bool>,
    /// Settings → MCP & tools: safe read-only tools pass without asking; `None` while loading or on servers without `auto_allow`.
    pub auto_allow: Option<aurelius_protocol::AutoAllowSetting>,
    /// Starting an idea: the task's model (`None`: the employee's default).
    pub idea_start_model: Option<(aurelius_domain::ProviderId, String)>,
    /// Every recorded Claude Code artifact, newest first; `None` on servers without `artifact.list` (lists hidden).
    pub artifacts: Option<Vec<aurelius_protocol::ArtifactView>>,
    /// Settings → MCP & tools: the server-wide MCP list (fetched when Settings opens), or why not.
    pub mcp: Option<Result<aurelius_protocol::McpServers, String>>,
    /// The last failed save of the MCP servers.
    pub mcp_error: Option<String>,
    /// Settings → MCP & tools: the Add dialog and Remove confirmation (protocol 1.23).
    pub mcp_manage: crate::screens::mcp_add::McpManage,
    /// Settings → MCP & tools → Always allowed tools (MCP tools approved with 'Always allow').
    pub always_allow: crate::screens::always_allow::AlwaysAllowState,
    /// Settings → MCP & tools → Apps (MCP): keys of outside apps (feature `app_keys`).
    pub app_keys: crate::screens::app_keys::AppKeysState,
    /// Manager plugins: the installed list, the catalog, Edit manager's toggles and the Add plugin dialog.
    pub plugins: crate::screens::plugins::PluginsState,
    /// The open Project settings card (project menu, palette).
    pub project_settings: Option<crate::screens::project_settings::ProjectSettings>,
    /// Manager to assign in the new task dialog.
    pub pending_task_manager: Option<EmployeeId>,
    /// New task: mode, connection, model and effort of its conversation.
    pub pending_task_run: aurelius_domain::task::RunSettings,
    /// New task and recurring task dialogs: the goal (several lines, kept as typed).
    pub goal_input: Entity<TextareaState>,
    /// Dashboard "Give work": title, goal, the project (required) and who gets it
    /// (`None`: the default, the single root employee). Mode, model and effort are `pending_task_run`.
    pub give_goal: Entity<TextareaState>,
    pub give_project: Option<ProjectId>,
    /// Employees page: the search field; Tasks page: the filters (`None`: any).
    pub employee_search: Entity<InputState>,
    pub tasks_filter: crate::screens::tasks::TaskFilter,
    /// Tasks page: how many rows show, for the filter they were loaded under.
    pub tasks_shown: (crate::screens::tasks::TaskFilter, usize),
    /// Tasks page: folds the user set per task with subtasks (`true`: open). Without one, a tree is
    /// open while its task is not finished. Kept for the session only.
    pub tasks_folds: std::collections::HashMap<aurelius_domain::TaskId, bool>,
    pub give_manager: Option<EmployeeId>,
    /// The task just given opens in the right panel once the server confirms it.
    give_open_for: Option<EmployeeId>,
    /// Whom the user last gave work to (ui.json `last_given_to`): the default with several roots.
    give_last: Option<EmployeeId>,
    /// Recurring task dialog: the hours of "every N hours" and the "HH:MM" of daily and weekly.
    pub recur_hours_input: Entity<InputState>,
    pub recur_time_input: Entity<InputState>,
    /// Files on this computer to attach (path, size); at most `UPLOADS_MAX` with the kept ones.
    pub pending_task_files: Vec<(std::path::PathBuf, u64)>,
    /// Recurring task dialog: 0 every N hours, 1 daily, 2 weekly; weekdays Monday = 0.
    pub pending_recurrence: usize,
    pub pending_weekdays: [bool; 7],
    pub pending_schedule_paused: bool,
    /// Editing a recurring task: the files it has (dropped ones are in `pending_schedule_remove`, by path).
    pub pending_schedule_files: Vec<aurelius_protocol::MessageFileView>,
    pub pending_schedule_remove: Vec<String>,
    /// Recurring tasks of every project (servers from 1.14; the Tasks tab's "Recurring" list).
    pub schedules: Vec<aurelius_protocol::TaskScheduleView>,
    /// Edit manager dialog: the manager's own default model and effort (its layer
    /// of the run defaults), saved with the dialog.
    pub pending_manager_run: aurelius_domain::connection::RunDefaults,
    /// Edit manager dialog: run its tasks one at a time, saved with the dialog.
    pub pending_manager_sequential: bool,
    /// Edit manager dialog: the manager's layer for tasks given to it and for its
    /// subagents (indexed by `DelegationRow`), saved with the dialog.
    pub pending_manager_delegation: [aurelius_domain::connection::RunDefaults; 1],
    /// Where each dropdown field was drawn last (its menu opens right below it).
    pub dropdown_bounds: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<&'static str, Bounds<Pixels>>>>,
    /// Folder chosen in the New project / Add manager dialog.
    pub dialog_folder: Option<String>,
    /// What to say once the server gave a task to the chosen employee (Give to…).
    pub assign_said: HashMap<aurelius_domain::TaskId, String>,
    pub overlay_focus: FocusHandle,
    /// Scroll position of the open popup menu (keyboard selection scrolls into view).
    pub menu_scroll: ScrollHandle,
    /// Scroll position of the main content (chat, manager, project pages).
    pub scroll: ScrollHandle,
    /// Scroll position of list pages (dashboard, tasks), separate from the conversation so the two never drive each other.
    pub page_scroll: ScrollHandle,
    /// Conversation whose messages are loaded (chat or manager).
    pub open_conversation: Option<ConversationId>,
    open_new_manager: bool,
    /// A created manager to open as soon as it appears in the manager list.
    pending_manager: Option<EmployeeId>,
    /// Composer placeholder to apply on the next render (needs the window).
    pending_placeholder: Option<String>,
    pub manager_extra: Vec<SharedString>,
    pub messages: Option<MessagesSnapshot>,
    /// Streamed text per assistant message while a run is active.
    pub live: HashMap<MessageId, String>,
    /// Tool use of answers still streaming.
    pub live_activity: HashMap<MessageId, Vec<aurelius_protocol::ActivityView>>,
    /// Since when answers still streaming run, and their tokens so far.
    pub live_progress: HashMap<MessageId, aurelius_protocol::RunProgress>,
    /// A once-a-second re-render runs (elapsed times stay current).
    ticking: bool,
    /// The screenshot in the side panel.
    pub shot: Option<crate::screens::side_panels::ShotView>,
    /// Files being fetched to open with this computer's own program.
    pub opening_files: std::collections::HashSet<String>,
    /// Thumbnails of uploaded images fetched from the server, by upload id
    /// (`None`: loading, or the server has none).
    pub upload_thumbs: HashMap<String, Option<std::sync::Arc<Image>>>,
    /// The Markdown viewer in the side panel.
    pub markdown: Option<crate::screens::markdown::MarkdownPanel>,
    /// The artifact in the side panel.
    pub artifact: Option<crate::screens::artifact::ArtifactPanel>,
    /// The claude.ai webview of that panel (persistent profile), kept once made.
    pub claude_web: Option<std::rc::Rc<crate::screens::artifact::ClaudeView>>,
    /// Latest stream of every conversation with a running answer (for the run panel).
    pub streams: HashMap<ConversationId, aurelius_protocol::StreamDelta>,
    /// Home: the selected "At work now" row (its manager shows in the preview pane).
    pub at_work_selected: Option<usize>,
    /// Esc hid the preview pane; selecting again shows it.
    pub preview_closed: bool,
    /// Messages of the conversation shown in the preview pane, and the one asked for.
    pub preview_messages: Option<MessagesSnapshot>,
    pub preview_requested: Option<ConversationId>,
    /// Tool blocks whose full output is shown.
    pub expanded_activity: std::collections::HashSet<String>,
    /// Shortened tool calls fetched whole (`activity.get`), by call id; `None` while loading.
    pub full_activity: HashMap<String, Option<aurelius_protocol::ActivityView>>,
    /// Long messages shown whole ("Show more"); answers seen streaming start here, so they don't fold while read.
    pub expanded_messages: std::collections::HashSet<MessageId>,

    pub(crate) requests: mpsc::UnboundedSender<backend::Request>,
    pub connection: Connection,
    pub projects: Vec<ProjectSummary>,
    /// The project a new idea from the Ideas page starts in: the last one an idea was added to.
    pub ideas_project: Option<ProjectId>,
    /// The Ideas page shows only this project's ideas (`None`: every project).
    pub ideas_project_filter: Option<ProjectId>,
    /// The project the open idea dialog adds to.
    pub pending_idea_project: Option<ProjectId>,
    pub jobs: Vec<JobSummary>,
    pub notice: Option<Notice>,
    notice_epoch: u64,
    /// Key of the copy button that just copied (shows "Copied" for ~1.5 s) and its epoch.
    pub copied: Option<String>,
    copy_epoch: u64,
    /// URL of the link under the pointer, shown bottom-left like a browser, and its epoch.
    hovered_link: Option<SharedString>,
    hover_epoch: u64,
    /// Bumped to stop an earlier "keep at the bottom" after opening a conversation.
    settle_epoch: u64,
    /// The open conversation's scroll area, cached apart from the composer (see `ConversationPane`).
    pub conversation_pane: Entity<crate::screens::chat::ConversationPane>,
    /// A repaint for streamed text is already scheduled.
    stream_repaint: bool,
    /// That repaint must be the whole view: a stream shown outside the open conversation changed.
    stream_repaint_app: bool,
    /// Set around a notify that only changes streamed text and elapsed times: the open
    /// conversation's finished messages stay cached (see `ConversationPane`).
    pub quiet_notify: bool,
    /// The open conversation's messages as cached views, their Markdown and measured boxes.
    pub message_cache: std::cell::RefCell<crate::screens::chat::MessageCache>,
    /// Plans in the conversation the user folded open or shut (default: open while waiting).
    pub plan_open: HashMap<MessageId, bool>,
    /// Waiting messages the user deleted; hidden until the server confirms or refuses.
    pub deleted_queued: std::collections::HashSet<MessageId>,
    /// Answers whose quick replies the user closed.
    pub dismissed_replies: std::collections::HashSet<MessageId>,
    /// Approvals whose command, content or change the user unfolded in full.
    pub approval_full: std::collections::HashSet<ApprovalId>,
    /// Approvals whose raw details the user unfolded in the inbox.
    pub approval_raw: std::collections::HashSet<ApprovalId>,
    open_created: bool,
    /// This window shows only this project (opened with 'Open in new window'); `None`: the main window.
    pub window_project: Option<ProjectId>,
    window_title: String,
    pub window_save: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl AppView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.set_global(ui::Hints { always: ui_pref("always_show_shortcuts") });
        theme::set_appearance(theme::Appearance::from_key(ui_prefs()["appearance"].as_str().unwrap_or_default()), cx);
        let data_root = aurelius_host::data_dir().ok();
        let (target, pending_notice) = match data_root.as_deref().map(backend::initial_target) {
            Some(Ok(target)) => (target, None),
            Some(Err(e)) => (None, Some(e)),
            None => (None, Some("The data folder is not available.".to_string())),
        };
        let remote_host = match &target {
            Some(aurelius_client::Target::Remote(s)) => Some((s.host_id, s.host_name.clone())),
            _ => None,
        };
        let pin_server = remote_host.as_ref().map_or_else(|| "local".to_string(), |(id, _)| id.to_string());
        let servers = data_root.as_deref().map(aurelius_client::servers::Servers::load).unwrap_or_default();
        let backend = backend::start(target, backend::Tokens::os(pin_server.clone()));
        let server_input = cx.new(|cx| InputState::new(window, cx).placeholder("Paste the pairing text (aurelius-pair:…)"));
        let server_name_input = cx.new(|cx| InputState::new(window, cx).placeholder("The server's name, for example Office"));
        let palette_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search a project, employee or command…"));
        let channel_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search channels"));
        let folder_path_input = cx.new(|cx| InputState::new(window, cx).placeholder("Type or paste a folder path"));
        let folder_name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Name of the new folder"));
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Name, for example Atlas"));
        let description_input = cx.new(|cx| InputState::new(window, cx).placeholder("Short description (optional)"));
        // Enter sends; Shift+Enter adds a line, and the field grows up to 8 rows.
        let composer = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 8).submit_on_enter(true).placeholder("Reply…"));
        let plan_input = cx.new(|cx| InputState::new(window, cx).placeholder("Feedback for the agent (needed to ask for changes)"));
        let plan_comment_input = cx.new(|cx| InputState::new(window, cx).placeholder("Your comment on the selected text"));
        let key_input = cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("Paste your API key (never shown or logged)"));
        let team_input = cx.new(|cx| InputState::new(window, cx).placeholder("Team ID, for example A1B2C3D4E5"));
        let load_form = crate::screens::load::LoadForm::new(window, cx);
        let role_input = cx.new(|cx| InputState::new(window, cx).placeholder("Free-form role, for example Product partner"));
        // The goal of a task has several lines: Enter adds one, Ctrl+Enter confirms the dialog.
        let goal_input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 8).placeholder("Goal: what needs to be achieved?"));
        let employee_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search employees by name, role or title"));
        let give_goal = cx.new(|cx| TextareaState::new(window, cx).auto_grow(2, 6).placeholder("What needs doing? For example 'Compare three hosting options'"));
        let recur_hours_input = cx.new(|cx| InputState::new(window, cx).placeholder("Hours, for example 6"));
        let recur_time_input = cx.new(|cx| InputState::new(window, cx).placeholder("HH:MM, for example 09:00"));
        let instructions_input = cx.new(|cx| InputState::new(window, cx).placeholder("Instructions (optional)"));
        let title_input = cx.new(|cx| InputState::new(window, cx).placeholder("Title on the org chart, for example Head of engineering (optional)"));

        let mut subscriptions = vec![
            cx.subscribe_in(&employee_search, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&plan_input, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            // The channel search asks the server as the query changes (`channel.search`).
            cx.subscribe_in(&channel_search, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.search_channels(cx);
                }
            }),
            cx.subscribe_in(&palette_input, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.palette_selected = 0;
                    cx.notify();
                }
            }),
            cx.subscribe_in(&folder_path_input, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.folder_go_typed(cx);
                }
            }),
            cx.subscribe_in(&folder_name_input, window, |this, _, event: &InputEvent, _, cx| {
                match event {
                    InputEvent::PressEnter { .. } => this.folder_create(cx),
                    InputEvent::Change => this.folder_name_changed(cx),
                    _ => {}
                }
            }),
            cx.subscribe_in(&name_input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.confirm_dialog(window, cx);
                }
            }),
            cx.subscribe_in(&description_input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.confirm_dialog(window, cx);
                }
            }),
            cx.subscribe_in(&recur_hours_input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.confirm_dialog(window, cx);
                }
            }),
            cx.subscribe_in(&recur_time_input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.confirm_dialog(window, cx);
                }
            }),
            cx.subscribe_in(&role_input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.confirm_dialog(window, cx);
                }
            }),
            cx.subscribe_in(&title_input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.confirm_dialog(window, cx);
                }
            }),
            cx.subscribe_in(&instructions_input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.confirm_dialog(window, cx);
                }
            }),
            cx.subscribe_in(&composer, window, |this, _, event: &InputEvent, window, cx| {
                // Shift+Enter also reports PressEnter, after inserting the newline.
                match event {
                    // While an answer runs, Enter interrupts it with the message (stop and send in one go).
                    // The Team chat (ui-ontwerp §4, O-2): Enter sends, Shift+Enter is a new line; Alt+Enter is Send now.
                    InputEvent::PressEnter { shift: false, .. } if this.screen == Page::GroupChat => this.send_message(window, cx),
                    InputEvent::PressEnter { shift: true, .. } if this.screen == Page::GroupChat => {}
                    InputEvent::PressEnter { shift: false, .. } if this.active_run().is_some() => this.send_now(window, cx),
                    InputEvent::PressEnter { shift: false, .. } => this.send_message(window, cx),
                    // Shift+Enter keeps its newline when nothing runs; while an answer runs it queues the message behind it.
                    InputEvent::PressEnter { shift: true, .. } if this.active_run().is_some() => this.send_message(window, cx),
                    _ => {}
                }
                // Typing hides the quick replies (only the first key and a cleared draft re-render).
                if matches!(event, InputEvent::Change) && this.quick_replies_on() && this.composer.read(cx).value().chars().count() <= 1 {
                    cx.notify();
                }
                // The Team chat's `@` suggestions follow the text.
                if matches!(event, InputEvent::Change) && this.screen == Page::GroupChat {
                    let text = this.composer.read(cx).value().to_string();
                    this.group_mentions.changed(&text);
                    cx.notify();
                }
            }),
        ];
        subscriptions.push(cx.observe_window_appearance(window, |_, _, cx| theme::system_changed(cx)));
        let members = crate::screens::board_members::Members::new(window, cx);
        subscriptions.extend(members.subscriptions(window, cx));
        let claude = crate::screens::account::Claude::new(window, cx);
        subscriptions.extend(claude.subscriptions(window, cx));
        let board = crate::screens::board::Board::new(window, cx);
        subscriptions.extend(board.subscriptions(window, cx));
        let vault = crate::screens::vault::Vault::new(window, cx);
        subscriptions.extend(vault.subscriptions(window, cx));
        let guidelines = crate::screens::guidelines::Guidelines::new(window, cx);
        subscriptions.extend(guidelines.subscriptions(window, cx));

        listen(backend.updates, 0, cx);
        poll_subagents(cx);
        tick_clock(cx);
        tick_power(cx);

        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let app = cx.entity();
        let conversation_pane = cx.new(|cx| crate::screens::chat::ConversationPane::new(&app, cx));

        Self {
            focus,
            screen: Page::Dashboard,
            employees_tab: EmployeesTab::default(),
            projects_tab: ProjectsTab::default(),
            panel: Panel::None,
            back_stack: Vec::new(),
            history_back: Vec::new(),
            history_forward: Vec::new(),
            last_place: None,
            // A new key: the old `sidebar_collapsed` hid the employee's conversation list, not the app sidebar.
            sidebar_collapsed: ui_pref("sidebar_hidden"),
            sidebar_peek: false,
            pins: pins_load(&ui_prefs(), &pin_server),
            employee_pins: pins_load(&ui_prefs(), &pin_server),
            pin_server,
            members,
            claude,
            board,
            vault,
            guidelines,
            folded: folded_load(&ui_prefs()),
            panel_widths: serde_json::from_value(ui_prefs()["panel_widths"].clone()).unwrap_or_default(),
            panel_save: None,
            selected: 0,
            task_filter: 0,
            history_open: false,
            approval_done: false,
            // Production starts clean; dev shows the design samples.
            show_preview: aurelius_host::Environment::current() == aurelius_host::Environment::Dev,
            overlay: Overlay::None,
            palette_input,
            palette_selected: 0,
            name_input,
            description_input,
            role_input,
            instructions_input,
            title_input,
            pending_reports_to: None,
            company: None,
            composer,
            pending_images: Vec::new(),
            pending_files: Vec::new(),
            plan_input,
            plan_snapshot: None,
            plan_comments: Vec::new(),
            plan_comment_draft: None,
            plan_selection: None,
            plan_comment_input,
            plan_sending: None,
            managers: Vec::new(),
            proposals: Vec::new(),
            approvals: Vec::new(),
            tasks: Vec::new(),
            connections: None,
            settings_selected: 0,
            settings_model: HashMap::new(),
            adding_connection: false,
            key_input,
            entering_key: false,
            remote: None,
            pairing: None,
            remote_poll: 0,
            pending_push: None,
            data_root,
            servers,
            remote_host,
            adding_server: false,
            server_input,
            server_name: Default::default(),
            server_name_input,
            folder_path_input,
            folder_name_input,
            pairing_wait: None,
            folder: None,
            backend_epoch: 0,
            pending_servers: false,
            pending_notice,
            team_input,
            load: None,
            power: None,
            load_error: None,
            load_form,
            settings_tab: 1,
            inbox_selected: 0,
            inbox_tab: InboxTab::NeedsYou,
            question_choices: HashMap::new(),
            question_own: HashMap::new(),
            question_focus: 0,
            notifications: None,
            inbox_new: Vec::new(),
            memory: None,
            memory_requested: None,
            ideas: None,
            ideas_project: None,
            ideas_project_filter: None,
            pending_idea_project: None,
            ideas_requested: None,
            usage_day: None,
            usage_hours: None,
            usage_hours_requested: None,
            ideas_filter: 0,
            ideas_author: IdeaAuthor::Anyone,
            ideas_shown: std::cell::Cell::new(((None, None, 0, IdeaAuthor::Anyone, None), crate::screens::projects::IDEAS_STEP)),
            project_tasks_shown: std::cell::Cell::new(((None, 0), crate::screens::projects::IDEAS_STEP)),
            ideas_kind: None,
            pending_idea_kind: Default::default(),
            pending_memory_decision: false,
            task_more_open: false,
            give_more_open: false,
            employee_advanced_open: false,
            idea_start_mode: None,
            memory_filter: 0,
            memory_ctx_manager: None,
            memory_ctx: None,
            memory_ctx_requested: None,
            memory_over_open: false,
            memory_dups_open: false,
            context_view: None,
            usage: None,
            subscription: None,
            update: None,
            backups: None,
            menu: None,
            manager_thread: None,
            group_conversation: None,
            group_error: None,
            group_live: Vec::new(),
            channels: None,
            channel: None,
            channel_divider: None,
            channel_new_at: None,
            channels_archived_open: false,
            channels_tickets_all: false,
            channels_archived_all: false,
            channel_order: None,
            channel_search,
            channel_results: None,
            channel_ticket: None,
            channel_refused: None,
            group_mentions: Default::default(),
            channels_followed: false,
            subagents_open: false,
            subagents: None,
            subagents_stepped: false,
            subagent_expanded: None,
            subagents_closed_at: None,
            threads_tasks_earlier_open: false,
            login_start: aurelius_host::startup::enabled(),
            browser: None,
            artifacts_enabled: None,
            usage_totals: None,
            trace_setting: None,
            trace_confirm_off: false,
            trace: None,
            trace_open: [false, false],
            instruction_versions: None,
            instruction_versions_requested: None,
            management_mode: None,
            auto_allow: None,
            idea_start_model: None,
            artifacts: None,
            mcp: None,
            mcp_error: None,
            mcp_manage: Default::default(),
            always_allow: Default::default(),
            app_keys: Default::default(),
            plugins: Default::default(),
            project_settings: None,
            pending_task_manager: None,
            pending_task_run: Default::default(),
            goal_input,
            give_goal,
            give_project: None,
            employee_search,
            tasks_filter: Default::default(),
            tasks_shown: (Default::default(), crate::screens::tasks::ROWS_STEP),
            tasks_folds: Default::default(),
            give_manager: None,
            give_open_for: None,
            give_last: ui_prefs()["last_given_to"].as_str().and_then(|s| EmployeeId::parse(s).ok()),
            recur_hours_input,
            recur_time_input,
            pending_task_files: Vec::new(),
            pending_recurrence: 1,
            pending_weekdays: [true, true, true, true, true, false, false],
            pending_schedule_paused: false,
            pending_schedule_files: Vec::new(),
            pending_schedule_remove: Vec::new(),
            schedules: Vec::new(),
            pending_manager_run: Default::default(),
            pending_manager_sequential: false,
            pending_manager_delegation: Default::default(),
            dropdown_bounds: Default::default(),
            dialog_folder: None,
            assign_said: HashMap::new(),
            overlay_focus: cx.focus_handle(),
            menu_scroll: ScrollHandle::new(),
            scroll: ScrollHandle::new(),
            page_scroll: ScrollHandle::new(),
            open_conversation: None,
            open_new_manager: false,
            pending_manager: None,
            pending_placeholder: None,
            manager_extra: Vec::new(),
            messages: None,
            live: HashMap::new(),
            live_activity: HashMap::new(),
            live_progress: HashMap::new(),
            ticking: false,
            shot: None,
            opening_files: Default::default(),
            upload_thumbs: Default::default(),
            markdown: None,
            artifact: None,
            claude_web: None,
            streams: HashMap::new(),
            at_work_selected: None,
            preview_closed: false,
            preview_messages: None,
            preview_requested: None,
            expanded_activity: Default::default(),
            full_activity: Default::default(),
            expanded_messages: Default::default(),
            requests: backend.requests,
            connection: Connection::Connecting,
            projects: Vec::new(),
            jobs: Vec::new(),
            notice: None,
            notice_epoch: 0,
            copied: None,
            copy_epoch: 0,
            hovered_link: None,
            hover_epoch: 0,
            settle_epoch: 0,
            conversation_pane,
            stream_repaint: false,
            stream_repaint_app: false,
            quiet_notify: false,
            message_cache: Default::default(),
            plan_open: HashMap::new(),
            deleted_queued: Default::default(),
            dismissed_replies: Default::default(),
            approval_full: Default::default(),
            approval_raw: Default::default(),
            open_created: false,
            window_project: None,
            window_title: String::new(),
            window_save: None,
            _subscriptions: subscriptions,
        }
    }

    fn apply(&mut self, update: Update, cx: &mut Context<Self>) {
        match update {
            Update::Connection(connection) => {
                if matches!(connection, Connection::NoServer | Connection::Unpaired(_)) && self.connection != connection {
                    self.pending_servers = true;
                }
                if matches!(connection, Connection::Online { .. }) && !matches!(self.connection, Connection::Online { .. }) {
                    // (Re)connected: ask the chip's subagents again.
                    self.subagents_stepped = true;
                }
                let online = matches!(connection, Connection::Online { .. }) && !matches!(self.connection, Connection::Online { .. });
                self.members_connection(&connection, cx);
                self.board_connection(&connection, cx);
                self.connection = connection;
                if online {
                    // The board reads again what it missed while offline (also for the sidebar's count).
                    self.board_refresh();
                    // The Team chat opened while offline: its id is known only now.
                    if self.screen == Page::GroupChat && self.group_conversation.is_none() {
                        let _ = self.requests.send(backend::Request::OpenGroupChat(self.channel));
                    }
                    // The channels' counts give the Team chat its badge (B8), also off its page.
                    let _ = self.requests.send(backend::Request::LoadChannels);
                    // The Trace buttons follow the server's trace mode.
                    if self.trace_supported() {
                        let _ = self.requests.send(backend::Request::LoadTraceSetting);
                    }
                    // The mode picker's "Manager chooses" option and the Company switch follow this setting.
                    if self.management_mode_supported() {
                        let _ = self.requests.send(backend::Request::LoadManagementMode);
                    }
                    if self.auto_allow_supported() {
                        let _ = self.requests.send(backend::Request::LoadAutoAllow);
                    }
                    // Guideline proposals wait in the Inbox (B5), also off the Guidelines tab; the rest reads again as it shows.
                    self.guidelines_changed();
                    // The hello's name goes to servers.json; Settings reads where it comes from.
                    self.server_name_online(cx);
                }
            }
            Update::DirListing(listing) => self.apply_dir_listing(listing),
            Update::DirFailed(ask, message) => self.apply_dir_failed(ask, message, cx),
            Update::PreviewMessages(snapshot) => {
                if self.preview_requested == Some(snapshot.conversation_id) {
                    self.preview_messages = Some(snapshot);
                }
            }
            Update::Subagents(conversation, agents) => {
                let agents = Some((conversation, agents));
                if self.subagents == agents {
                    return;
                }
                self.subagents = agents;
                if self.subagents_open && self.subagents_here().is_empty() {
                    self.subagents_open = false;
                }
            }
            Update::MarkdownFile(scope, path, result) => self.markdown_loaded(scope, path, result, cx),
            Update::ArtifactContent(url, version, result) => self.artifact_loaded(url, version, result, cx),
            Update::Projects(projects) => self.projects = projects,
            Update::Jobs(jobs) => {
                // Polled every 500 ms while a job runs: repaint only when it changed.
                if self.jobs == jobs {
                    return;
                }
                let running_conversations = |this: &Self| -> std::collections::HashSet<ConversationId> { this.running_jobs().filter_map(|j| j.conversation_id).collect() };
                let before = running_conversations(self);
                self.jobs = jobs;
                // Keep the streams of answers that still run only.
                let running = running_conversations(self);
                self.streams.retain(|c, _| running.contains(c));
                // A run started or ended: the preview's messages may have changed.
                if running != before {
                    self.preview_requested = None;
                }
                if self.running_jobs().next().is_some() {
                    self.ensure_ticking(cx);
                }
            }
            Update::Created(id) => {
                if std::mem::take(&mut self.open_created) {
                    self.navigate(Page::Project(ProjectRef::Real(id), Tab::Overview));
                }
                self.notify_user("Project created and saved locally.", false, cx);
            }
            Update::QueuedDeleteFailed(message_id) => {
                self.deleted_queued.remove(&message_id);
            }
            Update::DescriptionSaved(project, result) => self.project_settings_description_saved(project, result, cx),
            Update::Who(who) => {
                if who.user.as_ref().map(|u| u.id) != self.active_member().map(|u| u.id) {
                    // Another board member (or none) here now: nothing of the vault stays on screen.
                    self.vault_reset();
                    // Nor a guideline being edited or decided by the previous one.
                    self.guidelines_reset();
                }
                self.apply_who(who, cx)
            }
            Update::SignedOut(reason) => self.apply_signed_out(reason, cx),
            Update::Sessions(of, result) => self.apply_sessions(of, result, cx),
            Update::Account(update) => self.apply_account(update, cx),
            Update::Claude(update) => self.apply_claude(update, cx),
            Update::Board(update) => self.apply_board(update, cx),
            Update::Vault(update) => self.apply_vault(update, cx),
            Update::Guidelines(update) => self.apply_guidelines(update, cx),
            Update::ServerName(update) => self.apply_server_name(update, cx),
            Update::LooksLikeSecret(hold) => self.apply_secret_hold(hold, cx),
            Update::BoardMembers(list) => {
                self.members.list = list;
                cx.notify();
            }
            Update::BoardMemberCreated(name) => self.apply_member_created(name, cx),
            Update::BoardMemberFailed(failure) => self.apply_member_failure(failure, cx),
            Update::BoardMemberRequired => self.apply_member_required(cx),
            Update::Error(message) => {
                // A refused plan decision (already handled, expired) leaves the plan open to read.
                self.plan_sending = None;
                // Not connected: the description request was dropped, so allow Save again.
                if let Some(s) = self.project_settings.as_mut() {
                    s.description_saving = false;
                }
                self.notify_user(message, true, cx)
            }
            Update::Company(company) => {
                self.company = Some(company);
                cx.notify();
            }
            Update::Managers(managers) => {
                self.managers = managers;
                self.open_pending_manager();
                // A streamed group answer that never became a message (an empty answer):
                // gone once its run no longer works and no delta came for a moment.
                let managers = &self.managers;
                self.group_live.retain(|l| l.at.elapsed() < Duration::from_secs(3) || managers.iter().any(|m| m.agents.iter().any(|a| a.run_id == Some(l.run))));
                for i in 0..self.group_live.len() {
                    if self.group_live[i].author.is_none() {
                        self.group_live[i].author = self.run_author(self.group_live[i].run);
                    }
                }
                // The last subagent finished: its panel goes away with the chip.
                if self.subagents_open && self.subagents_here().is_empty() {
                    self.subagents_open = false;
                }
            }
            Update::Approvals(approvals) => {
                let before = self.pending_approvals().count();
                self.approvals = approvals;
                let after = self.pending_approvals().count();
                if after > before {
                    self.notify_user("An approval is waiting for you. Ctrl I opens the inbox.", false, cx);
                }
                self.inbox_selected = self.inbox_selected.min(self.inbox_len().saturating_sub(1));
            }
            Update::Info(text) => self.notify_user(text, false, cx),
            Update::TaskAssigned(task_id) => {
                let text = self.assign_said.remove(&task_id).unwrap_or_else(|| "Task given.".into());
                self.notify_user(text, false, cx);
            }
            Update::AppKeys(update) => self.app_keys_update(update, cx),
            Update::Plugins(update) => self.plugins_update(update, cx),
            Update::Tasks(tasks) => {
                self.tasks = tasks;
                self.request_upload_thumbs();
                // The open employee got its first task: show it.
                if matches!(self.screen, Page::Employee(ManagerRef::Real(_))) && self.open_conversation.is_none() && self.current_manager_thread().is_some() {
                    self.open_thread();
                }
                // The task in the right panel got its conversation (or another one): load it.
                if matches!(self.panel, Panel::Task(_)) {
                    let conversation = self.current_manager_thread();
                    if conversation.is_some() && conversation != self.open_conversation {
                        self.open(conversation);
                    }
                }
            }
            Update::TaskCreated(task) => {
                if let Some(employee) = self.give_open_for.take() {
                    self.panel = Panel::Task(task);
                    self.manager_thread = None;
                    let conversation = self.current_manager_thread();
                    self.open(conversation);
                    // The task may not be in `tasks` yet: name its employee from the form.
                    let name = self.managers.iter().find(|m| m.id == employee).map(|m| m.name.clone());
                    self.pending_placeholder = Some(name.map_or_else(|| "Reply…".to_string(), |n| format!("Message {n}…")));
                }
            }
            Update::Proposals(proposals) => {
                let before = self.pending_proposals().len();
                self.proposals = proposals;
                let after = self.pending_proposals().len();
                if after > before {
                    self.notify_user("An employee proposed a colleague. Ctrl I opens the inbox.", false, cx);
                }
                self.inbox_selected = self.inbox_selected.min(self.inbox_len().saturating_sub(1));
            }
            Update::Schedules(schedules) => {
                self.schedules = schedules;
                self.request_upload_thumbs();
                cx.notify();
            }
            Update::Notifications(snapshot) => {
                let before = self.notifications.as_ref().map(|n| n.unread);
                if let (Some(before), Some(newest)) = (before, snapshot.notifications.first()) {
                    if snapshot.unread > before && !newest.read {
                        let subject = if newest.subject.is_empty() { String::new() } else { format!(": {}", newest.subject) };
                        self.notify_user(format!("{}{subject} · Ctrl I opens the inbox", newest.title), false, cx);
                    }
                }
                self.notifications = Some(snapshot);
            }
            Update::Ideas(scope, items) => {
                // Deleted elsewhere (`idea.deleted`): a dialog for it has nothing left to save.
                if let Overlay::Dialog(DialogKind::EditIdea { id }) | Overlay::Confirm(ConfirmKind::DeleteIdea(id)) = self.overlay {
                    if self.ideas.as_ref().is_some_and(|(p, l)| *p == scope && l.iter().any(|i| i.id == id)) && !items.iter().any(|i| i.id == id) {
                        self.overlay = Overlay::None;
                        self.notify_user("This idea was deleted.", false, cx);
                    }
                }
                self.ideas = Some((scope, items));
                self.ideas_requested = None;
                self.request_upload_thumbs();
            }
            Update::MemoryChanged => match self.screen {
                Page::Project(ProjectRef::Real(project), Tab::Memory) => {
                    self.memory_requested = Some(project);
                    let _ = self.requests.send(backend::Request::LoadMemory(project));
                }
                // Off screen: the next visit loads it fresh.
                _ => {
                    self.memory = None;
                    self.memory_requested = None;
                }
            },
            Update::Memory(project, items) => {
                self.memory = Some((project, items));
                self.memory_requested = None;
                // A memory changed: ask again what goes into the context.
                if let Some((p, m, _)) = self.memory_ctx.as_ref().filter(|(p, ..)| *p == project) {
                    let (p, m) = (*p, *m);
                    self.memory_ctx_requested = Some((p, m));
                    let _ = self.requests.send(backend::Request::LoadMemoryContext { project: p, manager: m });
                }
            }
            Update::MemoryContext { project, manager, result } => {
                if self.memory_ctx_requested == Some((project, manager)) {
                    self.memory_ctx_requested = None;
                }
                self.memory_ctx = Some((project, manager, result));
            }
            Update::Usage(snapshot) => {
                self.usage = Some(snapshot);
                // New usage while the dialog is open: the hours may have changed too.
                if self.overlay == Overlay::Usage && self.usage_hours_requested.is_none() {
                    self.load_usage_hours(true);
                }
            }
            Update::UsageHours(day, result) => {
                if self.usage_hours_requested == Some(day) {
                    self.usage_hours_requested = None;
                }
                self.usage_hours = Some((day, result));
            }
            Update::File(id, result) => self.file_loaded(id, result, cx),
            Update::UploadThumb(id, result) => {
                let image = result.ok().and_then(|bytes| crate::screens::side_panels::image_format(&bytes).map(|f| std::sync::Arc::new(Image::from_bytes(f, bytes))));
                self.upload_thumbs.insert(id, image);
                cx.notify();
            }
            Update::FullActivity(call_id, result) => match result {
                Ok(view) => {
                    self.full_activity.insert(call_id, Some(view));
                    cx.notify();
                }
                Err(error) => {
                    self.full_activity.remove(&call_id);
                    self.notify_user(error, true, cx);
                }
            },
            Update::Screenshot(id, result) => {
                if let Some(shot) = self.shot.as_mut().filter(|s| s.id == id) {
                    match result {
                        Ok(bytes) => shot.image = Some(std::sync::Arc::new(Image::from_bytes(ImageFormat::Jpeg, bytes))),
                        Err(error) => shot.error = Some(error),
                    }
                }
            }
            Update::Subscription(s) => {
                // First fetch happens in the background: ask again shortly.
                if s.fetched_at_ms == 0 {
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(Duration::from_secs(6)).await;
                        let _ = this.update(cx, |this, _| {
                            let _ = this.requests.send(backend::Request::LoadSubscription(false));
                        });
                    })
                    .detach();
                }
                self.subscription = Some(s);
            }
            Update::Power(state) => {
                self.power = state.pending;
                cx.notify();
            }
            Update::Load(result) => self.load_loaded(result),
            Update::LoadSaved(result) => self.load_saved(result, cx),
            Update::Mcp(result) => {
                self.mcp = Some(result);
                self.mcp_error = None;
                self.mcp_manage.removing = None;
                self.mcp_manage.logging_out = None;
                cx.notify();
            }
            Update::McpLogin(login) => self.mcp_login_update(login, cx),
            Update::McpAuthFailed(why) => self.mcp_auth_failed(why, cx),
            Update::McpAdded => self.mcp_added(cx),
            Update::McpAddFailed(why) => self.mcp_add_failed(why, cx),
            Update::McpRemoveFailed(why) => self.mcp_remove_failed(why, cx),
            Update::McpAlwaysAllow(tools) => self.always_allow_loaded(tools),
            Update::McpAlwaysAllowRemoveFailed(why) => self.always_allow_remove_failed(why),
            Update::McpSaveFailed(why) => {
                self.mcp_error = Some(why);
                cx.notify();
            }
            Update::Browser(result) => {
                self.browser = Some(result);
                cx.notify();
            }
            Update::ArtifactSetting(enabled) => {
                self.artifacts_enabled = enabled;
                cx.notify();
            }
            Update::UsageTotals(conversation, totals) => {
                if self.open_conversation == Some(conversation) {
                    self.usage_totals = Some((conversation, totals));
                    cx.notify();
                }
            }
            Update::TraceSetting(setting) => {
                self.trace_setting = setting;
                self.trace_confirm_off = false;
                cx.notify();
            }
            Update::Trace(run_id, result) => {
                if let Some((_, slot)) = self.trace.as_mut().filter(|(open, _)| *open == run_id) {
                    *slot = Some(result);
                    cx.notify();
                }
            }
            Update::InstructionVersions(manager, result) => {
                self.instruction_versions = Some((manager, result));
                cx.notify();
            }
            Update::ManagementMode(setting) => {
                self.management_mode = setting;
                cx.notify();
            }
            Update::AutoAllow(setting) => {
                self.auto_allow = setting;
                cx.notify();
            }
            Update::Artifacts(artifacts) => {
                self.artifacts = Some(artifacts);
                self.artifacts_changed(cx);
                cx.notify();
            }
            Update::Remote(status) => {
                if let Some(error) = &status.error {
                    self.notify_user(error.clone(), true, cx);
                }
                let max = status.clients.len().saturating_sub(1);
                self.settings_selected = self.settings_selected.min(max);
                self.remote = Some(status);
            }
            Update::Pairing(code) => self.pairing = Some(code),
            Update::Backups(snap) => {
                if let Some(err) = &snap.error {
                    self.notify_user(err.clone(), true, cx);
                } else if snap.restore_pending.is_some() {
                    self.notify_user("Restoring: Aurelius restarts now…", false, cx);
                } else if self.backups.as_ref().is_some_and(|b| b.backups.first().map(|x| &x.id) != snap.backups.first().map(|x| &x.id)) {
                    self.notify_user("Backup created.", false, cx);
                } else if self.backups.as_ref().is_some_and(|b| b.paused.is_some()) && snap.paused.is_none() {
                    self.notify_user("Resumed. Background work continues.", false, cx);
                }
                self.backups = Some(snap);
            }
            Update::UpdateStatus(s) => {
                let newly = s.available && !self.update.as_ref().is_some_and(|u| u.available);
                let app_was = self.app_update().is_some_and(|a| a.available);
                if newly {
                    self.notify_user(format!("Server {} is available · Ctrl Shift U", s.latest.clone().unwrap_or_default()), false, cx);
                }
                self.update = Some(s);
                if !newly && !app_was {
                    if let Some(latest) = self.app_update().filter(|a| a.available).map(|a| a.latest.clone().unwrap_or_default()) {
                        self.notify_user(format!("App {latest} is available · Ctrl Shift U"), false, cx);
                    }
                }
            }
            Update::AppUpdateStatus(s) => {
                self.update.get_or_insert_with(Default::default).app = Some(Box::new(s));
            }
            Update::RestartForUpdate => {
                // The new programs are in place; start the new version and close this one.
                // By name: after the swap this process runs from the renamed `.old` file.
                if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
                    let exe = dir.join(format!("aurelius{}", std::env::consts::EXE_SUFFIX));
                    let _ = std::process::Command::new(exe).current_dir(&dir).spawn();
                }
                cx.quit();
            }
            Update::Context(view) => {
                self.context_view = Some(view);
                self.overlay = Overlay::Context;
            }
            Update::Connections(snapshot) => {
                self.settings_selected = self.settings_selected.min(snapshot.connections.len().saturating_sub(1));
                self.connections = Some(snapshot);
            }
            Update::EmployeeCreated(id) => {
                if std::mem::take(&mut self.open_new_manager) {
                    // The refreshed manager list may arrive after this receipt.
                    self.pending_manager = Some(id);
                    self.open_pending_manager();
                }
                self.notify_user("Employee added.", false, cx);
            }
            Update::Messages(snapshot) => {
                // A channel just opened: "New" goes before its first unread message, and stays there (fase 2 §3).
                if let Some((_, unread, read_through)) = self.channel_divider.filter(|(c, _, _)| *c == snapshot.conversation_id) {
                    let ordinals: Vec<u32> = snapshot.messages.iter().map(|m| m.ordinal).collect();
                    self.channel_new_at = crate::screens::team_feed::new_messages_at(&ordinals, read_through, unread as u32).map(|i| snapshot.messages[i].id);
                    self.channel_divider = None;
                }
                if self.open_conversation == Some(snapshot.conversation_id) {
                    // Just opened: always all the way down, also once late layout (Markdown, images) settles.
                    if self.messages.is_none() {
                        self.settle_bottom(cx);
                    }
                    let grew = self.messages.as_ref().is_none_or(|m| m.messages.len() != snapshot.messages.len());
                    if grew || self.near_bottom() {
                        self.follow_bottom(cx);
                    }
                    // Durable text replaces streamed text once committed.
                    for m in &snapshot.messages {
                        if m.status != MessageStatus::Partial {
                            self.live.remove(&m.id);
                            self.live_activity.remove(&m.id);
                            self.live_progress.remove(&m.id);
                        } else {
                            self.expanded_messages.insert(m.id);
                        }
                    }
                    self.message_cache.get_mut().retain(&snapshot.messages);
                    // A streamed group answer is a message now.
                    self.group_live.retain(|l| !snapshot.messages.iter().any(|m| m.id == l.message));
                    // No answer running: the totals are final for now (again after each answer ends).
                    let settled = !snapshot.messages.iter().any(|m| m.status == MessageStatus::Partial);
                    let conversation_id = snapshot.conversation_id;
                    self.messages = Some(snapshot);
                    if settled && self.run_usage_on() {
                        let task_id = self.tasks.iter().find(|t| t.conversation_id == Some(conversation_id)).map(|t| t.id);
                        let _ = self.requests.send(backend::Request::LoadUsageTotals { conversation_id, task_id });
                    }
                    self.request_upload_thumbs();
                }
            }
            Update::GroupChat(conversation) => {
                self.group_conversation = Some(conversation);
                self.group_error = None;
                if self.screen == Page::GroupChat && self.open_conversation.is_none() {
                    self.open_conversation = Some(conversation);
                }
            }
            Update::GroupChatFailed(why) => {
                self.group_error = Some(why);
            }
            Update::Channels(channels) => self.channels_loaded(channels),
            Update::ChannelSearch(query, found) => self.channels_found(query, found, cx),
            Update::ChannelArchived(channel) => self.channel_archived(channel),
            Update::Stream(delta) => {
                if stream_stepped(self.streams.get(&delta.conversation_id), &delta) && self.chip_conversation() == Some(delta.conversation_id) {
                    self.subagents_stepped = true;
                }
                let conversation = delta.conversation_id;
                let open = self.open_conversation == Some(conversation);
                // Outside the open conversation only the preview pane and the run panel show a stream;
                // the first delta also makes a manager "work in" it (`render_manager_preview`).
                // The open ticket's discussion shows "Forge is answering…" with the text so far.
                let discussion = self.board.discussion.as_ref().is_some_and(|(c, _)| *c == conversation);
                if discussion && self.discussion_at_bottom() {
                    self.board.discussion_scroll.scroll_to_bottom();
                }
                let shown = self.streams.insert(conversation, delta.clone()).is_none() || self.stream_shown_beside(conversation) || discussion;
                if !open && !shown {
                    return;
                }
                self.stream_repaint_app |= shown;
                if open {
                    // An employee's answer streams into the group chat under the id it gets there once done.
                    if Some(conversation) == self.group_conversation && !self.messages.as_ref().is_some_and(|s| s.messages.iter().any(|m| m.id == delta.message_id)) {
                        let at = std::time::Instant::now();
                        let author = self.run_author(delta.run_id);
                        match self.group_live.iter_mut().find(|l| l.message == delta.message_id) {
                            Some(entry) => {
                                entry.at = at;
                                entry.author = entry.author.take().or(author);
                                entry.mode = entry.mode.or(delta.mode);
                            }
                            None => self.group_live.push(GroupLive { message: delta.message_id, run: delta.run_id, at, author, mode: delta.mode }),
                        }
                    }
                    // Seen streaming: it stays whole when it ends, even if no message list came in meanwhile.
                    self.expanded_messages.insert(delta.message_id);
                    self.live.insert(delta.message_id, delta.text);
                    self.live_activity.insert(delta.message_id, delta.activity);
                    if let Some(progress) = delta.progress {
                        self.live_progress.insert(delta.message_id, progress);
                        self.ensure_ticking(cx);
                    }
                    if self.near_bottom() {
                        self.scroll.scroll_to_bottom();
                    }
                }
                // Deltas come per few tokens: repaint at most every 40 ms, so typing keeps up while it streams.
                // Only the open conversation's: notify just its pane, not AppView's other observers.
                if !self.stream_repaint {
                    self.stream_repaint = true;
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(Duration::from_millis(40)).await;
                        let _ = this.update(cx, |this, cx| {
                            this.stream_repaint = false;
                            if std::mem::take(&mut this.stream_repaint_app) {
                                this.quiet_notify = true;
                                cx.notify();
                            } else {
                                this.conversation_pane.update(cx, |_, cx| cx.notify());
                            }
                        });
                        // Observers ran when that update ended.
                        let _ = this.update(cx, |this, _| this.quiet_notify = false);
                    })
                    .detach();
                }
                return;
            }
        }
        cx.notify();
    }

    fn open_pending_manager(&mut self) {
        let Some(id) = self.pending_manager else { return };
        if let Some(m) = self.managers.iter().find(|m| m.id == id).cloned() {
            self.pending_manager = None;
            self.pending_placeholder = Some(format!("Message {}…", m.name));
            self.navigate(Page::Employee(ManagerRef::Real(id)));
            self.manager_thread = None;
            self.open_thread();
        }
    }

    /// Stable key of what is on screen; animations keyed by it replay on navigation only.
    pub fn page_key(&self) -> usize {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{:?}", self.screen).hash(&mut h);
        h.finish() as usize
    }

    pub fn overlay_key(&self) -> usize {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{:?}", self.overlay).hash(&mut h);
        h.finish() as usize
    }

    pub fn requests_handle(&self) -> &mpsc::UnboundedSender<backend::Request> {
        &self.requests
    }

    /// Talk to another server: a new backend; the old one ends when its
    /// request channel is dropped, and its late updates are ignored.
    pub fn restart_backend(&mut self, target: Option<aurelius_client::Target>, cx: &mut Context<Self>) {
        let handle = backend::start(target, self.server_tokens());
        self.requests = handle.requests;
        self.backend_epoch += 1;
        listen(handle.updates, self.backend_epoch, cx);
        self.reset_server_state();
        self.members_reset(None, cx);
        self.claude_reset();
        self.vault_reset();
        self.guidelines_reset();
        self.server_name_reset();
        cx.notify();
    }

    /// Forget everything shown from the previous server.
    fn reset_server_state(&mut self) {
        self.connection = Connection::Connecting;
        self.projects.clear();
        self.jobs.clear();
        self.managers.clear();
        self.proposals.clear();
        self.approvals.clear();
        self.tasks.clear();
        self.connections = None;
        self.remote = None;
        self.pairing = None;
        self.notifications = None;
        self.memory = None;
        self.memory_requested = None;
        self.memory_ctx = None;
        self.memory_ctx_requested = None;
        self.memory_ctx_manager = None;
        self.context_view = None;
        self.usage = None;
        self.usage_totals = None;
        self.trace_setting = None;
        self.trace = None;
        self.instruction_versions = None;
        self.instruction_versions_requested = None;
        self.management_mode = None;
        self.auto_allow = None;
        self.subscription = None;
        self.update = None;
        self.backups = None;
        self.messages = None;
        self.message_cache.take();
        self.live.clear();
        self.live_activity.clear();
        self.live_progress.clear();
        self.open_conversation = None;
        self.manager_thread = None;
        self.group_conversation = None;
        self.group_live.clear();
        self.channels = None;
        self.channel = None;
        self.channel_divider = None;
        self.channel_ticket = None;
        self.channel_refused = None;
        self.channel_results = None;
        self.screen = Page::Dashboard;
        tracing::info!(panel = ?self.panel, "server state reset (see B6)");
        self.panel = Panel::None;
        self.back_stack.clear();
        self.history_back.clear();
        self.history_forward.clear();
        self.last_place = None;
        self.folder = None;
        self.selected = 0;
    }

    /// The server is another computer: its name.
    pub fn remote_name(&self) -> Option<&str> {
        self.remote_host.as_ref().map(|(_, n)| n.as_str())
    }

    /// This app's own update status. Only the local service installs the
    /// app; a remote server reports its own computer's app, not this one.
    pub fn app_update(&self) -> Option<&aurelius_protocol::UpdateStatus> {
        if self.remote_name().is_some() {
            return None;
        }
        self.update.as_ref().and_then(|u| u.app.as_deref())
    }

    /// Status-bar label when the server or this app has a newer release.
    fn update_pill(&self) -> Option<String> {
        let server = self.update.as_ref().filter(|u| u.enabled && u.available).map(|u| u.latest.clone().unwrap_or_default());
        let app = self.app_update().filter(|u| u.enabled && u.available).map(|u| u.latest.clone().unwrap_or_default());
        match (server, app) {
            (Some(_), Some(_)) => Some("Updates".into()),
            (Some(v), None) => Some(format!("Server {v}")),
            (None, Some(v)) => Some(format!("App {v}")),
            (None, None) => None,
        }
    }

    /// Settings → MCP & tools: turn one server on or off for everything; the rest keep their state.
    pub fn toggle_mcp(&mut self, name: &str, _cx: &mut Context<Self>) {
        let Some(Ok(list)) = &self.mcp else { return };
        let mut enabled: Vec<String> = list.servers.iter().filter(|s| s.enabled).map(|s| s.name.clone()).collect();
        enabled.extend(list.missing.iter().cloned());
        match enabled.iter().position(|n| n == name) {
            Some(i) => { enabled.remove(i); }
            None => enabled.push(name.to_string()),
        }
        self.mcp_error = None;
        let _ = self.requests.send(backend::Request::SetMcp(enabled));
    }

    /// Settings → MCP & tools: forget a server Claude Code no longer has (saves the list without it).
    pub fn remove_missing_mcp(&mut self, name: &str, _cx: &mut Context<Self>) {
        let Some(Ok(list)) = &self.mcp else { return };
        let mut enabled: Vec<String> = list.servers.iter().filter(|s| s.enabled).map(|s| s.name.clone()).collect();
        enabled.extend(list.missing.iter().filter(|n| n.as_str() != name).cloned());
        self.mcp_error = None;
        let _ = self.requests.send(backend::Request::SetMcp(enabled));
    }

    /// Settings → MCP & tools: pick the browser for Claude in Chrome (kept on the server).
    pub fn set_browser(&mut self, choice: aurelius_protocol::BrowserChoice, _cx: &mut Context<Self>) {
        let _ = self.requests.send(backend::Request::SetBrowser(choice));
    }

    /// Settings → MCP & tools: Claude in Chrome or Playwright (kept on the server).
    pub fn set_browser_backend(&mut self, backend: aurelius_protocol::BrowserBackend, _cx: &mut Context<Self>) {
        let _ = self.requests.send(backend::Request::SetBrowserBackend(backend));
    }

    /// The server reports tokens per answer and per conversation (feature `run_usage`).
    pub fn run_usage_on(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.run_usage)
    }

    /// The server has trace mode (feature `run_trace`).
    pub fn trace_supported(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.run_trace)
    }

    /// Managers propose instructions, which have versions (feature `instruction_proposals`).
    pub fn instruction_proposals_on(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.instruction_proposals)
    }

    /// The server lets management choose its own task mode (feature `management_chooses_mode`).
    pub fn management_mode_supported(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.management_chooses_mode)
    }

    /// The Team chat lists a channel per ticket (feature `ticket_channels`).
    pub fn ticket_channels_supported(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.ticket_channels)
    }

    /// Tickets have their discussion and a ticket can be made from a message (feature `ticket_discussion`).
    pub fn ticket_discussion_supported(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.ticket_discussion)
    }

    /// The board has its Testing column and the company a tester (feature `board_testing`).
    pub fn board_testing_supported(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.board_testing)
    }

    /// The server can auto-allow safe read-only tools (feature `auto_allow`).
    pub fn auto_allow_supported(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.auto_allow)
    }

    /// Settings → MCP & tools: safe read-only tools pass without asking (needs Approve).
    pub fn set_auto_allow(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let _ = self.requests.send(backend::Request::SetAutoAllow(enabled));
        cx.notify();
    }

    /// Settings → Company: management chooses Auto or Plan itself (needs Approve).
    pub fn set_management_mode(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let _ = self.requests.send(backend::Request::SetManagementMode(enabled));
        cx.notify();
    }

    /// The new task's lead is on the management team and the server lets it choose its own mode:
    /// "no mode picked" then reads "Manager chooses" instead of "Employee default".
    pub fn manager_chooses_mode(&self) -> bool {
        let (employee, _) = self.task_form_target();
        self.management_mode_supported() && self.management_mode == Some(true) && employee.is_some_and(|e| crate::screens::dashboard::is_management(&self.managers, e))
    }

    /// The versions of a manager's instructions, asked once per opened Edit manager.
    pub fn ensure_instruction_versions(&mut self, manager: EmployeeId) {
        if self.instruction_proposals_on() && self.instruction_versions_requested != Some(manager) {
            self.instruction_versions_requested = Some(manager);
            self.instruction_versions = None;
            let _ = self.requests.send(backend::Request::LoadInstructionVersions(manager));
        }
    }

    /// Answers get a Trace button: the server has trace mode and it is on.
    pub fn trace_on(&self) -> bool {
        self.trace_supported() && self.trace_setting.is_some_and(|s| s.enabled)
    }

    /// Settings → MCP & tools: trace mode on, or off (which deletes the kept traces).
    pub fn set_trace_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.trace_confirm_off = false;
        let _ = self.requests.send(backend::Request::SetTraceEnabled(enabled));
        cx.notify();
    }

    /// Open what this answer's run sent in the side panel.
    pub fn open_trace(&mut self, run_id: aurelius_domain::RunId, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.requests.send(backend::Request::LoadTrace(run_id));
        // The guideline versions it had (B6), also without trace mode.
        self.guideline_run_want(run_id);
        self.trace = Some((run_id, None));
        self.trace_open = [false, false];
        self.overlay = Overlay::Trace;
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// Settings → MCP & tools: let runs publish Claude Code artifacts to claude.ai, or not.
    pub fn set_artifacts_enabled(&mut self, enabled: bool, _cx: &mut Context<Self>) {
        let _ = self.requests.send(backend::Request::SetArtifactsEnabled(enabled));
    }

    /// Settings → App: System / Light / Dark, kept in ui.json.
    pub fn set_appearance(&mut self, appearance: theme::Appearance, cx: &mut Context<Self>) {
        theme::set_appearance(appearance, cx);
        set_ui_value("appearance", serde_json::json!(appearance.key()));
        cx.notify();
    }

    /// Show the hovered link's URL; hiding waits a moment, so the gap between
    /// the lines of a wrapped link doesn't make it flicker.
    pub fn set_hovered_link(&mut self, url: Option<SharedString>, cx: &mut Context<Self>) {
        self.hover_epoch += 1;
        if url.is_some() {
            self.hovered_link = url;
            cx.notify();
            return;
        }
        let epoch = self.hover_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(150)).await;
            let _ = this.update(cx, |this, cx| {
                if this.hover_epoch == epoch {
                    this.hovered_link = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Copy `text` to the clipboard; the button `key` shows "Copied" for 1.5 s (one-shot timer).
    pub fn copy_text(&mut self, key: String, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = Some(key);
        self.copy_epoch += 1;
        let epoch = self.copy_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            let _ = this.update(cx, |this, cx| {
                if this.copy_epoch == epoch {
                    this.copied = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub fn notify_user(&mut self, text: impl Into<SharedString>, error: bool, cx: &mut Context<Self>) {
        self.notice = Some(Notice { text: text.into(), error });
        self.notice_epoch += 1;
        let epoch = self.notice_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(6)).await;
            let _ = this.update(cx, |this, cx| {
                if this.notice_epoch == epoch {
                    this.notice = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    // ── Data helpers ────────────────────────────────────────────────────

    pub fn sample(&self, project: ProjectRef) -> Option<&'static preview::Project> {
        match project {
            ProjectRef::Sample(i) => preview::PROJECTS.get(i),
            ProjectRef::Real(_) => None,
        }
    }

    pub fn real(&self, project: ProjectRef) -> Option<&ProjectSummary> {
        match project {
            ProjectRef::Real(id) => self.projects.iter().find(|p| p.id == id),
            ProjectRef::Sample(_) => None,
        }
    }

    /// Projects on the Company home: real ones first (pinned ones on top, in pin order), then samples.
    pub fn home_projects(&self) -> Vec<ProjectRef> {
        let mut out: Vec<_> = pinned_first(self.projects.iter().map(|p| p.id).collect(), &self.pins, |id| *id).into_iter().map(ProjectRef::Real).collect();
        if self.show_preview {
            out.extend((0..preview::PROJECTS.len()).map(ProjectRef::Sample));
        }
        out
    }

    pub fn project_name(&self, project: ProjectRef) -> SharedString {
        match project {
            ProjectRef::Real(_) => self.real(project).map(|p| p.name.clone()).unwrap_or_else(|| "Project".into()).into(),
            ProjectRef::Sample(i) => preview::PROJECTS[i].name.into(),
        }
    }

    /// Load (and follow) one conversation's messages.
    /// The composer's placeholder from the next render on.
    pub(crate) fn pending_placeholder_set(&mut self, text: String) {
        self.pending_placeholder = Some(text);
    }

    pub(crate) fn open(&mut self, conversation: Option<ConversationId>) {
        if self.open_conversation != conversation {
            self.live.clear();
            self.live_activity.clear();
            self.live_progress.clear();
            self.group_live.clear();
            self.messages = None;
            self.message_cache.take();
            self.subagents_open = false;
        }
        self.open_conversation = conversation;
        let _ = self.requests.send(backend::Request::OpenConversation(conversation));
    }

    /// Managers of a project as cards: daemon managers or design samples.
    pub fn project_managers(&self, project: ProjectRef) -> Vec<ManagerCard> {
        match project {
            ProjectRef::Sample(i) => preview::PROJECTS[i]
                .managers
                .iter()
                .enumerate()
                .map(|(n, m)| ManagerCard {
                    key: ManagerRef::Sample(i, n),
                    name: m.name.into(),
                    role: m.role.into(),
                    title: None,
                    summary: m.summary.into(),
                    working: m.working,
                    status: LifecycleStatus::Active,
                })
                .collect(),
            ProjectRef::Real(id) => self
                .managers
                .iter()
                // Who works here: employees with open tasks in the project.
                .filter(|m| m.projects_active.contains(&id))
                .map(|m| ManagerCard {
                    key: ManagerRef::Real(m.id),
                    name: m.name.clone().into(),
                    role: m.role.clone().into(),
                    title: m.title.clone().map(Into::into),
                    summary: match (m.status, m.working) {
                        (LifecycleStatus::Paused, _) => "Paused. No new work starts until you resume.".into(),
                        (_, true) => "Working on an answer.".into(),
                        _ => m.instructions.clone().unwrap_or_else(|| "No instructions yet.".into()).into(),
                    },
                    working: m.working,
                    status: m.status,
                })
                .collect(),
        }
    }

    pub fn manager_card(&self, manager: ManagerRef) -> Option<ManagerCard> {
        match manager {
            ManagerRef::Sample(p, _) => self.project_managers(ProjectRef::Sample(p)).into_iter().find(|m| m.key == manager),
            ManagerRef::Real(id) => self.managers.iter().find(|m| m.id == id).map(|m| ManagerCard {
                key: manager,
                name: m.name.clone().into(),
                role: m.role.clone().into(),
                title: m.title.clone().map(Into::into),
                summary: match (m.status, m.working) {
                    (LifecycleStatus::Paused, _) => "Paused. No new work starts until you resume.".into(),
                    (_, true) => "Working on an answer.".into(),
                    _ => m.instructions.clone().unwrap_or_else(|| "No instructions yet.".into()).into(),
                },
                working: m.working,
                status: m.status,
            }),
        }
    }

    pub fn real_manager(&self, manager: ManagerRef) -> Option<&EmployeeSummary> {
        match manager {
            ManagerRef::Real(id) => self.managers.iter().find(|m| m.id == id),
            ManagerRef::Sample(..) => None,
        }
    }

    /// The project of the open task conversation (an employee's chat has none).
    pub fn thread_project(&self) -> Option<ProjectId> {
        let id = self.current_manager_thread()?;
        self.tasks.iter().find(|t| t.conversation_id == Some(id)).map(|t| t.project_id)
    }

    /// Remove a waiting message right away; it comes back if the server refuses.
    pub fn delete_queued(&mut self, message_id: MessageId, cx: &mut Context<Self>) {
        let Some(conversation_id) = self.open_conversation else { return };
        self.deleted_queued.insert(message_id);
        let _ = self.requests.send(backend::Request::DeleteQueued { conversation_id, message_id });
        cx.notify();
    }

    /// Stop what runs in the conversation and answer this waiting message now.
    pub fn send_queued_now(&mut self, message_id: MessageId, cx: &mut Context<Self>) {
        let Some(conversation_id) = self.open_conversation else { return };
        let _ = self.requests.send(backend::Request::SendQueuedNow { conversation_id, message_id });
        cx.notify();
    }

    pub fn active_run(&self) -> Option<aurelius_domain::RunId> {
        self.messages
            .as_ref()?
            .messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Assistant && m.status == MessageStatus::Partial)
            .and_then(|m| m.run_id)
    }

    /// True when the view is at (or close to) the bottom, so new text should follow.
    fn near_bottom(&self) -> bool {
        let max = self.scroll.max_offset().y;
        -self.scroll.offset().y >= max - px(120.)
    }

    /// Scroll to the bottom; GPUI applies it at the next layout, when the new height is known.
    fn follow_bottom(&self, cx: &mut Context<Self>) {
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    /// Keep the view at the bottom for a moment after a conversation opens:
    /// its height grows over a few frames while Markdown and files lay out.
    fn settle_bottom(&mut self, cx: &mut Context<Self>) {
        self.settle_epoch += 1;
        let (epoch, conversation) = (self.settle_epoch, self.open_conversation);
        self.follow_bottom(cx);
        cx.spawn(async move |this, cx| {
            for _ in 0..15 {
                cx.background_executor().timer(Duration::from_millis(60)).await;
                let still = this
                    .update(cx, |this, cx| {
                        let current = this.settle_epoch == epoch && this.open_conversation == conversation;
                        if current {
                            this.follow_bottom(cx);
                        }
                        current
                    })
                    .unwrap_or(false);
                if !still {
                    break;
                }
            }
        })
        .detach();
    }

    /// The scroll handle of what the main area shows: the conversation or a list page.
    fn active_scroll(&self) -> ScrollHandle {
        match self.screen {
            Page::Employee(..) | Page::GroupChat => self.scroll.clone(),
            _ => self.page_scroll.clone(),
        }
    }

    fn scroll_by(&mut self, pages: f32, cx: &mut Context<Self>) {
        self.settle_epoch += 1;
        let scroll = self.active_scroll();
        let page = scroll.bounds().size.height * 0.85;
        let max = scroll.max_offset().y;
        let current = scroll.offset();
        let y = (current.y - page * pages).min(px(0.)).max(-max);
        scroll.set_offset(point(current.x, y));
        cx.notify();
    }

    /// Tasks of a real project under the current filter (1 active, 2 waiting, 3 done).
    pub fn real_tasks(&self, project: ProjectId) -> Vec<&TaskView> {
        use aurelius_domain::task::TaskStatus::*;
        self.tasks
            .iter()
            .filter(|t| t.project_id == project)
            .filter(|t| match self.task_filter {
                0 => (matches!(t.status, Draft | Queued | Running | Review) || (t.status.is_terminal() && t.live_run.is_some())) && !crate::ui::is_waiting(t),
                1 => crate::ui::is_waiting(t),
                2 => t.status.is_terminal() && t.live_run.is_none(),
                _ => false,
            })
            .collect()
    }

    /// The recurring tasks of a real project (Tasks tab, filter 4), in the order the server gives.
    pub fn real_schedules(&self, project: ProjectId) -> Vec<&aurelius_protocol::TaskScheduleView> {
        self.schedules.iter().filter(|s| s.project_id == project).collect()
    }

    /// An employee screen or the group chat: where the open conversation lives.
    pub fn on_thread_screen(&self) -> bool {
        matches!(self.screen, Page::Employee(..) | Page::GroupChat) || self.panel_shown()
    }

    /// The Dashboard's Give work box takes the task dropdowns (mode, model, effort, who).
    fn give_work_open(&self) -> bool {
        self.screen == Page::Dashboard && matches!(self.overlay, Overlay::None | Overlay::Menu)
    }

    /// The project of the open new task or recurring task dialog.
    fn task_dialog_project(&self) -> Option<ProjectId> {
        match self.overlay {
            Overlay::Dialog(DialogKind::NewTask { project } | DialogKind::TaskSchedule { project, .. }) => Some(project),
            _ => None,
        }
    }

    /// Ideas have files and can be deleted (feature `idea_attachments`).
    pub fn idea_attachments_on(&self) -> bool {
        matches!(&self.connection, crate::backend::Connection::Online { server, .. } if server.idea_attachments)
    }

    /// The open idea dialog takes files: the server has them and the project a work folder for the copies.
    pub fn idea_files_on(&self) -> bool {
        let project = match self.overlay {
            Overlay::Dialog(DialogKind::NewIdea { project }) => Some(project),
            Overlay::Dialog(DialogKind::EditIdea { id }) => self.ideas.iter().flat_map(|(_, l)| l).find(|i| i.id == id).map(|i| i.project_id),
            _ => None,
        };
        self.idea_attachments_on() && project.is_some_and(|id| self.projects.iter().any(|p| p.id == id && p.folder.is_some()))
    }

    fn on_attach(&mut self, _: &AttachFiles, _: &mut Window, cx: &mut Context<Self>) {
        if self.board.dialog == Some(crate::screens::board::BoardDialog::NewTicket) {
            self.board_pick_files(cx);
            return;
        }
        if self.give_work_open() {
            self.pick_task_files(cx);
            return;
        }
        if !self.takes_manager_files() {
            self.notify_user("Open an employee conversation to add files.", false, cx);
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("Add files for the employee".into()) });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                let _ = this.update(cx, |this, cx| this.add_files(paths, cx));
            }
        })
        .detach();
    }

    /// The open screen is a real employee's task conversation or the group chat,
    /// which take files with a message.
    pub fn takes_manager_files(&self) -> bool {
        matches!(self.thread_screen(), Page::Employee(ManagerRef::Real(_)) | Page::GroupChat) && self.current_manager_thread().is_some()
    }

    /// Files dropped on the conversation wait beside the composer like picked ones.
    pub fn drop_files(&mut self, paths: &[std::path::PathBuf], cx: &mut Context<Self>) {
        let paths: Vec<_> = paths.iter().filter(|p| p.is_file()).cloned().collect();
        if paths.is_empty() {
            self.notify_user("Only files can be dropped here, not folders.", true, cx);
            return;
        }
        if self.give_work_open() {
            self.add_task_files(paths, cx);
        } else if self.takes_manager_files() {
            self.add_files(paths, cx);
        } else {
            self.notify_user("Open an employee conversation to add files.", false, cx);
        }
    }

    /// Files for the next message to a manager: at most `UPLOADS_MAX`, each at
    /// most `UPLOAD_MAX_BYTES` (the server checks again).
    pub fn add_files(&mut self, paths: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        let mut problems = Vec::new();
        for path in paths {
            if self.pending_files.iter().any(|(p, _)| *p == path) {
                continue;
            }
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
            let size = match std::fs::metadata(&path) {
                Ok(m) if m.is_file() => m.len(),
                Ok(_) => {
                    problems.push(format!("{name} is a folder; add files only."));
                    continue;
                }
                Err(e) => {
                    problems.push(format!("Could not read {name}: {e}"));
                    continue;
                }
            };
            if size > aurelius_domain::conversation::UPLOAD_MAX_BYTES {
                problems.push(format!("{name} is {}; files can be at most {} MB.", file_size(size), aurelius_domain::conversation::UPLOAD_MAX_BYTES / 1_048_576));
                continue;
            }
            if self.pending_files.len() >= aurelius_domain::conversation::UPLOADS_MAX {
                problems.push(format!("At most {} files per message.", aurelius_domain::conversation::UPLOADS_MAX));
                break;
            }
            self.pending_files.push((path, size));
        }
        if !problems.is_empty() {
            self.notify_user(problems.join(" "), true, cx);
        }
        cx.notify();
    }

    /// Pick files for the open task or idea dialog (Ctrl+O): any type, at most `UPLOADS_MAX`.
    pub fn pick_task_files(&mut self, cx: &mut Context<Self>) {
        let prompt = if self.idea_files_on() { "Attach to the idea" } else { "Attach to the task" };
        let paths = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some(prompt.into()) });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                let _ = this.update(cx, |this, cx| this.add_task_files(paths, cx));
            }
        })
        .detach();
    }

    /// Files for the task or idea being made: at most `UPLOADS_MAX` (with the files a recurring
    /// task or idea keeps), each at most `UPLOAD_MAX_BYTES`; the server checks again.
    pub fn add_task_files(&mut self, paths: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        if self.task_dialog_project().is_none() && !self.idea_files_on() && !self.give_work_open() {
            return;
        }
        let kept = self.pending_schedule_files.iter().filter(|f| !self.pending_schedule_remove.contains(&f.path)).count();
        let problems = take_files(&mut self.pending_task_files, paths, kept);
        if !problems.is_empty() {
            self.notify_user(problems.join(" "), true, cx);
        }
        cx.notify();
    }

    /// A paste into the goal: a copied image (a screenshot) or copied files are attached to
    /// the task. Returns whether it took the paste; anything else is pasted as text.
    pub fn paste_into_task_goal(&mut self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let mut found = Vec::new();
        let mut took = false;
        for entry in item.entries() {
            match entry {
                ClipboardEntry::Image(image) => {
                    took = true;
                    match save_pasted_image(image) {
                        Ok(path) => found.push(path),
                        Err(e) => self.notify_user(e, true, cx),
                    }
                }
                ClipboardEntry::ExternalPaths(paths) => {
                    let files: Vec<_> = paths.paths().iter().filter(|p| p.is_file()).cloned().collect();
                    took |= !files.is_empty();
                    found.extend(files);
                }
                ClipboardEntry::String(_) => {}
            }
        }
        self.add_task_files(found, cx);
        took
    }

    /// The recurrence typed in the recurring task dialog, or what to correct.
    pub fn read_recurrence(&self, cx: &App) -> Result<aurelius_domain::schedule::Recurrence, String> {
        use aurelius_domain::schedule::{EVERY_HOURS_MAX, Recurrence};
        let time = || parse_clock(self.recur_time_input.read(cx).value().as_ref()).ok_or_else(|| "Enter the time as HH:MM, for example 09:00.".to_string());
        match self.pending_recurrence {
            0 => {
                let hours: u32 = self.recur_hours_input.read(cx).value().trim().parse().map_err(|_| "Enter the number of hours, for example 6.".to_string())?;
                if !(1..=EVERY_HOURS_MAX).contains(&hours) {
                    return Err(format!("Hours must be between 1 and {EVERY_HOURS_MAX}."));
                }
                Ok(Recurrence::EveryHours { hours })
            }
            1 => Ok(Recurrence::Daily { at_minute: time()? }),
            _ => {
                let weekdays: Vec<u8> = (0..7u8).filter(|d| self.pending_weekdays[*d as usize]).collect();
                if weekdays.is_empty() {
                    return Err("Pick at least one weekday.".into());
                }
                Ok(Recurrence::Weekly { weekdays, at_minute: time()? })
            }
        }
    }

    /// A new recurring task in `project` (Settings → Advanced → Recurring schedules).
    pub fn new_schedule_in(&mut self, project: ProjectId, window: &mut Window, cx: &mut Context<Self>) {
        self.open_dialog(DialogKind::TaskSchedule { project, schedule: None }, window, cx);
        self.pending_task_manager = self.task_manager(project);
        self.pending_task_run = Default::default();
    }

    /// The recurring task selected in Settings → Advanced → Recurring schedules.
    pub fn selected_schedule(&self) -> Option<aurelius_protocol::TaskScheduleView> {
        if self.screen != Page::Settings || self.settings_tab != crate::screens::schedules::SCHEDULES_TAB {
            return None;
        }
        self.schedules.get(self.settings_selected).cloned()
    }

    /// Edit the selected recurring task in the same form as a new one.
    pub fn edit_schedule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use aurelius_domain::schedule::Recurrence;
        let Some(s) = self.selected_schedule() else { return };
        self.open_dialog(DialogKind::TaskSchedule { project: s.project_id, schedule: Some(s.id) }, window, cx);
        self.name_input.update(cx, |i, cx| i.set_value(s.title.clone(), window, cx));
        self.goal_input.update(cx, |i, cx| i.set_value(s.goal.clone(), window, cx));
        self.role_input.update(cx, |i, cx| i.set_value(s.criteria.join("; "), window, cx));
        self.pending_task_manager = Some(s.employee_id);
        self.pending_task_run = s.run.clone();
        self.pending_schedule_files = s.attachments.clone();
        match &s.recurrence {
            Recurrence::EveryHours { hours } => {
                self.pending_recurrence = 0;
                self.recur_hours_input.update(cx, |i, cx| i.set_value(hours.to_string(), window, cx));
            }
            Recurrence::Daily { at_minute } => {
                self.pending_recurrence = 1;
                self.recur_time_input.update(cx, |i, cx| i.set_value(format_clock(*at_minute), window, cx));
            }
            Recurrence::Weekly { weekdays, at_minute } => {
                self.pending_recurrence = 2;
                self.pending_weekdays = [false; 7];
                for d in weekdays.iter().filter(|d| **d < 7) {
                    self.pending_weekdays[*d as usize] = true;
                }
                self.recur_time_input.update(cx, |i, cx| i.set_value(format_clock(*at_minute), window, cx));
            }
        }
        cx.notify();
    }

    /// Pause the selected recurring task, or resume it.
    pub fn toggle_schedule_paused(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.selected_schedule() else { return };
        let _ = self.requests.send(backend::Request::PauseSchedule { schedule_id: s.id, paused: !s.paused });
        self.notify_user(if s.paused { "Resumed: the next time is planned from now." } else { "Paused: nothing is made until you resume it." }, false, cx);
    }

    /// Make a task from the selected recurring task now; its schedule stays as it is.
    pub fn run_schedule_now(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.selected_schedule() else { return };
        let _ = self.requests.send(backend::Request::RunScheduleNow(s.id));
        self.notify_user(format!("Started a task from “{}”.", s.title), false, cx);
    }

    /// Ask before deleting the selected recurring task.
    pub fn ask_delete_schedule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.selected_schedule() else { return };
        self.overlay = Overlay::Confirm(ConfirmKind::DeleteSchedule(s.id));
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Show a task in its project's Tasks tab.
    pub fn show_task(&mut self, task: aurelius_domain::TaskId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.tasks.iter().find(|t| t.id == task).cloned() else {
            self.notify_user("This task is no longer available.", false, cx);
            return;
        };
        self.read_task_results(task.id);
        self.task_filter = if !crate::ui::is_active(&task) { 2 } else if task.status == aurelius_domain::task::TaskStatus::Waiting { 1 } else { 0 };
        self.navigate(Page::Project(ProjectRef::Real(task.project_id), Tab::Tasks));
        self.selected = self.real_tasks(task.project_id).iter().position(|t| t.id == task.id).unwrap_or(0);
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Show a recurring task in Settings → Advanced → Recurring schedules ("From schedule" on a task).
    pub fn open_schedule(&mut self, schedule: aurelius_domain::TaskScheduleId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.schedules.iter().position(|s| s.id == schedule) else {
            self.notify_user("This recurring task no longer exists.", false, cx);
            return;
        };
        self.open_settings_page(window, cx);
        self.set_settings_tab(crate::screens::schedules::SCHEDULES_TAB, cx);
        self.settings_selected = at;
        cx.notify();
    }

    pub fn schedule_menu(&self) -> MenuItems {
        let Some(s) = self.selected_schedule() else { return vec![] };
        vec![
            call("Edit…", "", |this, w, cx| this.edit_schedule(w, cx)),
            call(if s.paused { "Resume" } else { "Pause" }, "", |this, _, cx| this.toggle_schedule_paused(cx)),
            call("Run now", "", |this, _, cx| this.run_schedule_now(cx)),
            None,
            danger(call("Delete…", "", |this, w, cx| this.ask_delete_schedule(w, cx))),
        ]
    }

    fn on_new_task(&mut self, _: &NewTask, window: &mut Window, cx: &mut Context<Self>) {
        // In Ideas: the selected idea becomes a task (choose the manager).
        if self.on_ideas_screen() {
            if self.selected_idea().is_some_and(|i| i.task_id.is_none()) {
                let size = window.viewport_size();
                self.open_idea_menu(point(size.width / 2. - px(160.), px(180.)), window, cx);
                return;
            }
        }
        match self.current_project() {
            Some(ProjectRef::Real(project)) => {
                self.open_dialog(DialogKind::NewTask { project }, window, cx);
                // The open manager by default; Alt+M chooses another (or none: a draft).
                self.pending_task_manager = self.task_manager(project);
                self.pending_task_run = Default::default();
            }
            Some(ProjectRef::Sample(_)) => self.notify_user("Sample projects are read-only.", true, cx),
            // Elsewhere a task starts in the Dashboard's Give work box (for the open employee, if any).
            None => {
                if let Page::Employee(ManagerRef::Real(id)) = self.screen {
                    self.give_manager = Some(id);
                }
                self.go_nav(NavItem::Dashboard, window, cx);
                self.give_goal.update(cx, |i, cx| i.focus(window, cx));
            }
        }
    }

    /// Tasks: the selected task's conversation with its manager (plan, findings, results).
    fn on_open_task_conversation(&mut self, _: &OpenTaskConversation, window: &mut Window, cx: &mut Context<Self>) {
        let Page::Project(ProjectRef::Real(project), Tab::Tasks) = self.screen else {
            self.notify_user("Select a task in a project's Tasks tab first.", false, cx);
            return;
        };
        let Some(task) = self.real_tasks(project).get(self.selected).map(|t| (*t).clone()) else { return };
        let Some(conversation) = task.conversation_id else {
            self.notify_user("This task has no conversation yet: it starts when an employee takes the task on.", false, cx);
            return;
        };
        self.open_task_thread(project, conversation, window, cx);
    }

    /// Open a task's conversation in the right panel. Task conversations are not in an employee's
    /// `conversations`, so they are found through the task that owns them.
    pub fn open_task_thread(&mut self, _project: ProjectId, conversation: ConversationId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(task) = task_of_conversation(&self.tasks, conversation) {
            self.open_panel(Panel::Task(task), window, cx);
            return;
        }
        let Some(manager) = self.managers.iter().find(|m| m.conversations.iter().any(|c| c.id == conversation) || m.conversation_id == conversation).map(|m| m.id) else {
            self.notify_user("The employee of this conversation is no longer available.", true, cx);
            return;
        };
        self.open_manager(ManagerRef::Real(manager), window, cx);
        self.select_manager_thread(conversation, cx);
    }

    /// One option of a new task dropdown.
    fn task_option(label: impl Into<SharedString>, detail: Option<SharedString>, choice: TaskChoice, checked: bool) -> Option<MenuEntry> {
        Some(MenuEntry { label: label.into(), keys: None, run: MenuDo::Task(choice), danger: false, checked, detail })
    }

    /// Who a task can be given to: any active employee (protocol 3), the management team first.
    pub fn assignable_managers(&self, _project: ProjectId) -> Vec<(EmployeeId, String, String)> {
        crate::screens::dashboard::assignee_order(&self.managers).into_iter().map(|m| (m.id, m.name.clone(), m.role.clone())).collect()
    }

    /// Alt+P in the Add / Edit manager dialog: who the manager reports to (nobody: the top of the org chart).
    /// Those that already report to it, directly or not, are not offered: that would make a circle.
    fn on_cycle_reports_to(&mut self, _: &CycleReportsTo, window: &mut Window, cx: &mut Context<Self>) {
        let manager = match self.overlay {
            Overlay::Dialog(DialogKind::NewManager) => None,
            Overlay::Dialog(DialogKind::EditManager { id, .. }) => Some(id),
            _ => return,
        };
        let current = self.pending_reports_to;
        let mut items: MenuItems = crate::screens::org::reports_candidates(&self.managers, manager)
            .into_iter()
            .map(|m| Self::task_option(m.name.clone(), Some(crate::screens::org::position(m).into()), TaskChoice::ReportsTo(Some(m.id)), current == Some(m.id)))
            .collect();
        if !items.is_empty() {
            items.push(None);
        }
        items.push(Self::task_option("Nobody (top of the org chart)", Some("Reports to you".into()), TaskChoice::ReportsTo(None), current.is_none()));
        self.open_dropdown("reports-to", items, window, cx);
    }

    /// The switch in Settings → MCP & tools: auto-allow safe tools.
    fn on_toggle_auto_allow(&mut self, _: &ToggleAutoAllow, _: &mut Window, cx: &mut Context<Self>) {
        if let (true, Some(on)) = (self.may_approve(), self.auto_allow.as_ref().map(|s| s.enabled)) {
            self.set_auto_allow(!on, cx);
        }
    }

    /// The switch in Settings → Company: management chooses its own task mode.
    fn on_toggle_management_mode(&mut self, _: &ToggleManagementMode, _: &mut Window, cx: &mut Context<Self>) {
        if let (true, Some(on)) = (self.may_approve(), self.management_mode) {
            self.set_management_mode(!on, cx);
        }
    }

    /// Alt+M in the new task dialog: the manager dropdown (or none: a draft).
    fn on_cycle_task_manager(&mut self, _: &CycleTaskManager, window: &mut Window, cx: &mut Context<Self>) {
        if self.task_dialog_project().is_none() && self.give_work_open() {
            // The management team first, then everyone else as the org chart orders them.
            let current = self.give_assignee();
            let team = crate::screens::dashboard::roots(&self.managers);
            let items: MenuItems = crate::screens::dashboard::assignee_order(&self.managers)
                .into_iter()
                .map(|m| {
                    let detail = if team.contains(&m.id) { "Management team".to_string() } else { m.role.clone() };
                    Self::task_option(m.name.clone(), Some(detail.into()), TaskChoice::GiveManager(m.id), current == Some(m.id))
                })
                .collect();
            if items.is_empty() {
                self.notify_user("Nobody to give work to yet: add an employee first.", false, cx);
                return;
            }
            self.open_dropdown("give-manager", items, window, cx);
            return;
        }
        let Some(project) = self.task_dialog_project() else { return };
        let current = self.pending_task_manager;
        let mut items: MenuItems = self
            .assignable_managers(project)
            .into_iter()
            .map(|(id, name, role)| Self::task_option(name, Some(role.into()), TaskChoice::Manager(Some(id)), current == Some(id)))
            .collect();
        if !items.is_empty() {
            items.push(None);
        }
        items.push(Self::task_option("No employee (draft)", Some("Nothing runs until it has an employee".into()), TaskChoice::Manager(None), current.is_none()));
        self.open_dropdown("task-manager", items, window, cx);
    }

    /// Who the Give work box gives to: the one picked, else the single root employee (with
    /// several: whom the user last gave work to, else the first of the management team).
    pub fn give_assignee(&self) -> Option<EmployeeId> {
        self.give_manager
            .filter(|m| self.managers.iter().any(|x| x.id == *m && x.status == LifecycleStatus::Active))
            .or_else(|| crate::screens::dashboard::default_assignee(&self.managers, self.give_last))
    }

    /// The project the Give work box puts the task in (required, no default).
    pub fn give_project_id(&self) -> Option<ProjectId> {
        self.give_project.filter(|p| self.projects.iter().any(|x| x.id == *p))
    }

    fn on_cycle_give_project(&mut self, _: &CycleGiveProject, window: &mut Window, cx: &mut Context<Self>) {
        if !self.give_work_open() {
            return;
        }
        let current = self.give_project_id();
        let items: MenuItems = self.projects.iter().map(|p| Self::task_option(p.name.clone(), None, TaskChoice::GiveProject(p.id), current == Some(p.id))).collect();
        if items.is_empty() {
            self.notify_user("Create a project first: work always belongs to one.", false, cx);
            return;
        }
        self.open_dropdown("give-project", items, window, cx);
    }

    /// Give work: a task for the chosen employee in the chosen project; it opens in the right panel.
    pub fn give_work(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let goal = self.give_goal.read(cx).value().trim().to_string();
        if goal.is_empty() {
            self.notify_user("Say what needs doing first.", true, cx);
            self.give_goal.update(cx, |i, cx| i.focus(window, cx));
            return;
        }
        // No title field: the title is the first line of the text, shortened.
        let title = aurelius_domain::conversation::derive_title(&goal);
        let Some(project_id) = self.give_project_id() else {
            self.notify_user("Choose the project this work belongs to (Alt+J).", true, cx);
            return;
        };
        let Some(employee_id) = self.give_assignee() else {
            self.notify_user("Choose who gets this work (Alt+M).", true, cx);
            return;
        };
        let run = std::mem::take(&mut self.pending_task_run);
        self.remember_mode(Some(employee_id), run.mode);
        let files: Vec<String> = std::mem::take(&mut self.pending_task_files).into_iter().map(|(p, _)| p.display().to_string()).collect();
        let _ = self.requests.send(backend::Request::CreateTask { project_id, employee_id: Some(employee_id), title, goal, criteria: Vec::new(), run, files });
        self.give_open_for = Some(employee_id);
        self.give_last = Some(employee_id);
        set_ui_value("last_given_to", serde_json::json!(employee_id.to_string()));
        self.give_manager = None;
        self.give_goal.update(cx, |i, cx| i.set_value("", window, cx));
        cx.notify();
    }

    /// New task: the models it can run on, per usable connection (connection, model, label).
    pub fn task_model_options(&self) -> Vec<(aurelius_domain::ProviderId, String, String)> {
        let Some(snapshot) = &self.connections else { return vec![] };
        snapshot
            .connections
            .iter()
            .filter(|c| c.status != "disconnected")
            .flat_map(|c| {
                let models: Vec<String> = if c.models.is_empty() { c.default_model.iter().cloned().collect() } else { c.models.clone() };
                models.into_iter().map(move |m| (c.id, m.clone(), format!("{} · {}", Self::friendly_model(&m), c.display_name)))
            })
            .collect()
    }

    /// The employee and project of the open New task or recurring task dialog, else of the Give work box.
    fn task_form_target(&self) -> (Option<EmployeeId>, Option<ProjectId>) {
        match self.task_dialog_project() {
            Some(project) => (self.pending_task_manager, Some(project)),
            None => (self.give_assignee(), self.give_project_id()),
        }
    }

    /// "Plan", "Opus 5.5 · Claude Code", "High", or how the employee runs ("Employee default (Opus 5.5)").
    pub fn task_run_labels(&self) -> (String, String, String) {
        let run = &self.pending_task_run;
        let (employee, project) = self.task_form_target();
        let default = self.default_run(employee, project);
        let mode = run.mode.map(|m| m.label().to_string()).unwrap_or_else(|| if self.manager_chooses_mode() { MANAGER_CHOOSES.into() } else { "Employee default".into() });
        let model = match (&run.connection_id, &run.model) {
            (Some(id), Some(m)) => self.task_model_options().into_iter().find(|(c, x, _)| c == id && x == m).map(|(_, _, l)| l).unwrap_or_else(|| Self::friendly_model(m)),
            _ => format!("Employee default ({})", Self::run_text(default.as_ref().and_then(|(_, m, _)| m.as_deref()), None)),
        };
        let effort = match run.effort.as_deref() {
            Some(e) => effort_label(e),
            None => format!("Employee default ({})", default.and_then(|(_, _, e)| e).map_or_else(|| "Claude Code decides".into(), |e| effort_label(&e))),
        };
        (mode, model, effort)
    }

    /// The line beside "More ▾" in New task and Give work: how the task runs and what else
    /// is folded away, for example "Employee default (Opus 5.5 · High) · 2 files".
    pub fn task_more_summary(&self, criteria: bool) -> String {
        let run = &self.pending_task_run;
        let mut parts = Vec::new();
        if run.mode.is_none() && run.model.is_none() && run.effort.is_none() {
            let (employee, project) = self.task_form_target();
            parts.push(self.default_run_label(employee, project));
        } else {
            let (mode, model, effort) = self.task_run_labels();
            parts.push(if run.mode.is_some() { mode } else { "Employee's mode".into() });
            parts.extend([model, effort]);
        }
        let files = self.pending_task_files.len() + self.pending_schedule_files.len().saturating_sub(self.pending_schedule_remove.len());
        if files > 0 {
            parts.push(format!("{files} file{}", if files == 1 { "" } else { "s" }));
        }
        if criteria {
            parts.push("criteria".into());
        }
        parts.join(" · ")
    }

    /// Edit manager: the manager's own model and effort ("Inherit" when unset) and
    /// what its next run uses with them, with where that comes from.
    pub fn manager_run_labels(&self, manager: EmployeeId) -> (String, String, String) {
        use aurelius_domain::connection::{ConfigScope, Purpose};
        use aurelius_domain::employee::Setting;
        let run = &self.pending_manager_run;
        let model = match (&run.connection, &run.model) {
            (Setting::Value(id), Setting::Value(m)) => self.task_model_options().into_iter().find(|(c, x, _)| c == id && x == m).map(|(_, _, l)| l).unwrap_or_else(|| Self::friendly_model(m)),
            (_, Setting::Value(m)) => Self::friendly_model(m),
            _ => "Inherit".into(),
        };
        let effort = match &run.effort {
            Setting::Value(e) => effort_label(e),
            _ => "Inherit".into(),
        };
        let Some(snapshot) = &self.connections else { return (model, effort, "Loading connection…".into()) };
        // An employee has no project; its layers are its own and the global default.
        let project: Option<ProjectId> = None;
        let layer = |scope: ConfigScope| snapshot.defaults.iter().find(|d| d.scope == scope && d.purpose == Purpose::Employee).map(|d| d.settings.clone());
        let mut layers = vec![("manager", run.clone())];
        layers.extend(project.and_then(|p| layer(ConfigScope::Project(p))).map(|s| ("project", s)));
        layers.extend(layer(ConfigScope::Global).map(|s| ("global default", s)));
        (model, effort, effective_run(&layers, &snapshot.connections))
    }

    /// The manager layer of the run defaults as stored (empty when there is none).
    fn manager_defaults(&self, manager: EmployeeId) -> aurelius_domain::connection::RunDefaults {
        use aurelius_domain::connection::{ConfigScope, Purpose};
        self.connections
            .as_ref()
            .and_then(|s| s.defaults.iter().find(|d| d.scope == ConfigScope::Employee(manager) && d.purpose == Purpose::Employee))
            .map(|d| d.settings.clone())
            .unwrap_or_default()
    }

    /// Save the Edit manager dialog's model and effort as the manager's own defaults,
    /// if they changed. Inherit clears that field of the manager layer.
    fn save_manager_defaults(&mut self, manager: EmployeeId) {
        use aurelius_domain::connection::{ConfigScope, Purpose};
        let settings = std::mem::take(&mut self.pending_manager_run);
        if settings == self.manager_defaults(manager) {
            return;
        }
        self.store_run_layer(ConfigScope::Employee(manager), Purpose::Employee, settings);
    }

    /// One layer of the run defaults as stored, if there is one.
    fn run_layer(&self, scope: aurelius_domain::connection::ConfigScope, purpose: aurelius_domain::connection::Purpose) -> Option<aurelius_domain::connection::RunDefaults> {
        self.connections.as_ref()?.defaults.iter().find(|d| d.scope == scope && d.purpose == purpose).map(|d| d.settings.clone())
    }

    /// Save one layer of the run defaults.
    fn store_run_layer(&mut self, scope: aurelius_domain::connection::ConfigScope, purpose: aurelius_domain::connection::Purpose, settings: aurelius_domain::connection::RunDefaults) {
        // Optimistic: show it at once; the daemon confirms with a fresh snapshot.
        if let Some(snapshot) = self.connections.as_mut() {
            match snapshot.defaults.iter_mut().find(|d| d.scope == scope && d.purpose == purpose) {
                Some(d) => d.settings = settings.clone(),
                None => snapshot.defaults.push(aurelius_protocol::DefaultsView { scope, purpose, settings: settings.clone() }),
            }
        }
        let _ = self.requests.send(backend::Request::SetDefaults { scope, purpose, settings });
    }

    /// Where the subagent dropdown writes: the manager of the open Edit manager
    /// dialog (`Some(Some)`), or the global default in Company settings (`Some(None)`).
    fn delegation_target(&self) -> Option<Option<EmployeeId>> {
        match self.overlay {
            Overlay::Dialog(DialogKind::EditManager { id, .. }) => Some(Some(id)),
            Overlay::None if self.screen == Page::Settings && self.settings_tab == crate::screens::overlays::COMPANY_TAB && self.connections.is_some() => Some(None),
            _ => None,
        }
    }

    /// The subagent row's own layer: the dialog's pending one for a manager, else the global one.
    fn delegation_settings(&self, row: DelegationRow, manager: Option<EmployeeId>) -> aurelius_domain::connection::RunDefaults {
        match manager {
            Some(_) => self.pending_manager_delegation[row.index()].clone(),
            None => self.run_layer(aurelius_domain::connection::ConfigScope::Global, row.purpose()).unwrap_or_default(),
        }
    }

    /// The subagent row (for this manager, or the global default): its own model and
    /// effort ("Inherit" when unset) and what such a run uses, with where that comes from.
    /// A subagent's model falls back to Claude Code's subagent default, its effort
    /// always follows the manager (Claude Code takes no subagent effort).
    pub fn delegation_labels(&self, row: DelegationRow, manager: Option<EmployeeId>) -> (String, String, String) {
        use aurelius_domain::connection::ConfigScope;
        use aurelius_domain::employee::Setting;
        let own = self.delegation_settings(row, manager);
        let model = match &own.model {
            Setting::Value(m) => Self::friendly_model(m),
            _ => "Inherit".into(),
        };
        let effort = match &own.effort {
            Setting::Value(e) => effort_label(e),
            _ => "Inherit".into(),
        };
        if self.connections.is_none() {
            return (model, effort, "Loading connection…".into());
        }
        let project: Option<ProjectId> = None;
        let mut layers = vec![("set here", own)];
        if manager.is_some() {
            layers.extend(project.and_then(|p| self.run_layer(ConfigScope::Project(p), row.purpose())).map(|s| ("project subagent default", s)));
            layers.extend(self.run_layer(ConfigScope::Global, row.purpose()).map(|s| ("global subagent default", s)));
        }
        (model, effort, effective_subagent_run(&layers))
    }

    /// A subagent value was picked: into the Edit manager dialog, or saved as the global default at once.
    fn choose_delegation(&mut self, row: DelegationRow, cx: &mut Context<Self>, change: impl FnOnce(&mut aurelius_domain::connection::RunDefaults)) {
        match self.delegation_target() {
            Some(Some(_)) => change(&mut self.pending_manager_delegation[row.index()]),
            Some(None) => {
                let mut settings = self.delegation_settings(row, None);
                change(&mut settings);
                self.store_run_layer(aurelius_domain::connection::ConfigScope::Global, row.purpose(), settings);
                self.notify_user(format!("Subagents: {}", self.delegation_labels(row, None).2), false, cx);
            }
            None => {}
        }
    }

    /// Save the Edit manager dialog's subagent row as the manager's own layer, if it changed.
    fn save_manager_delegation(&mut self, manager: EmployeeId) {
        use aurelius_domain::connection::ConfigScope;
        let pending = std::mem::take(&mut self.pending_manager_delegation);
        for row in DelegationRow::ALL {
            let settings = pending[row.index()].clone();
            let scope = ConfigScope::Employee(manager);
            if settings != self.run_layer(scope, row.purpose()).unwrap_or_default() {
                self.store_run_layer(scope, row.purpose(), settings);
            }
        }
    }

    /// Alt+U (Edit manager dialog, Company settings): the model dropdown of
    /// subagents (Inherit, or a Claude Code model).
    fn open_delegation_model(&mut self, row: DelegationRow, window: &mut Window, cx: &mut Context<Self>) {
        use aurelius_domain::employee::Setting;
        let Some(manager) = self.delegation_target() else { return };
        let own = self.delegation_settings(row, manager);
        let current = match &own.model {
            Setting::Value(m) => Some(m.clone()),
            _ => None,
        };
        // Subagents are Claude Code's: its models, once each.
        let claude: Vec<_> = self.connections.iter().flat_map(|s| s.connections.iter()).filter(|c| c.provider == "claude_code").map(|c| c.id).collect();
        let mut seen = std::collections::HashSet::new();
        let options: Vec<(aurelius_domain::ProviderId, String, String)> = self
            .task_model_options()
            .into_iter()
            .filter(|(c, m, _)| claude.contains(c) && seen.insert(m.clone()))
            .map(|(c, m, _)| {
                let label = Self::friendly_model(&m);
                (c, m, label)
            })
            .collect();
        let inherit = if manager.is_some() { "Project or global subagent default" } else { "Claude Code's subagent default (Sonnet)" };
        let mut items: MenuItems = vec![Self::task_option("Inherit", Some(inherit.into()), TaskChoice::DelegationModel(row, None), current.is_none())];
        if !options.is_empty() {
            items.push(None);
        }
        items.extend(options.into_iter().map(|(c, m, label)| {
            let checked = current.as_deref() == Some(m.as_str());
            Self::task_option(label, None, TaskChoice::DelegationModel(row, Some((c, m))), checked)
        }));
        self.open_dropdown("subagent-model", items, window, cx);
    }

    /// The Edit manager dialog's "Run tasks one at a time", if it changed (else `None`,
    /// which leaves it as it is). Optimistic: the manager shows it at once.
    fn take_manager_sequential(&mut self, manager: EmployeeId) -> Option<bool> {
        let on = std::mem::take(&mut self.pending_manager_sequential);
        let m = self.managers.iter_mut().find(|m| m.id == manager)?;
        if m.sequential_tasks == on {
            return None;
        }
        m.sequential_tasks = on;
        Some(on)
    }

    /// Alt+T in the Edit manager dialog: "Run tasks one at a time" on/off.
    fn on_toggle_manager_sequential(&mut self, _: &ToggleManagerSequential, _: &mut Window, cx: &mut Context<Self>) {
        let Overlay::Dialog(DialogKind::EditManager { .. }) = self.overlay else { return };
        self.pending_manager_sequential = !self.pending_manager_sequential;
        cx.notify();
    }

    /// Alt+O in the new task dialog: the mode dropdown (as the manager, Plan, Ask, Accept edits, Auto).
    fn on_cycle_task_mode(&mut self, _: &CycleTaskMode, window: &mut Window, cx: &mut Context<Self>) {
        use aurelius_domain::autonomy::AgentMode;
        if self.task_dialog_project().is_none() && !self.give_work_open() {
            return;
        }
        let current = self.pending_task_run.mode;
        let default_label = if self.manager_chooses_mode() { MANAGER_CHOOSES } else { "Employee default" };
        let mut items: MenuItems = vec![Self::task_option(default_label, None, TaskChoice::Mode(None), current.is_none()), None];
        items.extend([AgentMode::Ask, AgentMode::AcceptEdits, AgentMode::Auto, AgentMode::Plan].into_iter().map(|m| Self::task_option(m.label(), None, TaskChoice::Mode(Some(m)), current == Some(m))));
        self.open_dropdown("task-mode", items, window, cx);
    }

    /// Alt+L in the new task dialog: the model dropdown (as the manager, or a model of a connection).
    fn on_cycle_task_model(&mut self, _: &CycleTaskModel, window: &mut Window, cx: &mut Context<Self>) {
        if let Overlay::Dialog(DialogKind::EditManager { .. }) = self.overlay {
            use aurelius_domain::employee::Setting;
            let current = match (&self.pending_manager_run.connection, &self.pending_manager_run.model) {
                (Setting::Value(c), Setting::Value(m)) => Some((*c, m.clone())),
                _ => None,
            };
            let mut items: MenuItems = vec![Self::task_option("Inherit", Some("Project or global default".into()), TaskChoice::ManagerModel(None), current.is_none())];
            let options = self.task_model_options();
            if !options.is_empty() {
                items.push(None);
            }
            items.extend(options.into_iter().map(|(c, m, label)| {
                let checked = current.as_ref().is_some_and(|(cc, cm)| *cc == c && *cm == m);
                Self::task_option(label, None, TaskChoice::ManagerModel(Some((c, m))), checked)
            }));
            self.open_dropdown("manager-model", items, window, cx);
            return;
        }
        if self.task_dialog_project().is_none() && !self.give_work_open() {
            return;
        }
        let current = self.pending_task_run.connection_id.zip(self.pending_task_run.model.clone());
        let (employee, project) = self.task_form_target();
        let default = self.default_run_label(employee, project);
        let mut items: MenuItems = vec![Self::task_option(default, Some("This task".into()), TaskChoice::Model(None), current.is_none())];
        let options = self.task_model_options();
        if !options.is_empty() {
            items.push(None);
        }
        items.extend(options.into_iter().map(|(c, m, label)| {
            let checked = current.as_ref().is_some_and(|(cc, cm)| *cc == c && *cm == m);
            Self::task_option(label, None, TaskChoice::Model(Some((c, m))), checked)
        }));
        self.open_dropdown("task-model", items, window, cx);
    }

    /// Alt+E in the new task dialog: the effort dropdown (as the manager, low … max).
    fn on_cycle_task_effort(&mut self, _: &CycleTaskEffort, window: &mut Window, cx: &mut Context<Self>) {
        if let Overlay::Dialog(DialogKind::EditManager { .. }) = self.overlay {
            use aurelius_domain::employee::Setting;
            let current = match &self.pending_manager_run.effort {
                Setting::Value(e) => Some(e.clone()),
                _ => None,
            };
            let mut items: MenuItems = vec![Self::task_option("Inherit", Some("Project or global default".into()), TaskChoice::ManagerEffort(None), current.is_none()), None];
            items.extend(EFFORTS.iter().map(|(level, label, detail)| {
                Self::task_option(*label, Some((*detail).into()), TaskChoice::ManagerEffort(Some(level.to_string())), current.as_deref() == Some(*level))
            }));
            self.open_dropdown("manager-effort", items, window, cx);
            return;
        }
        if self.task_dialog_project().is_none() && !self.give_work_open() {
            return;
        }
        let current = self.pending_task_run.effort.clone();
        let (employee, project) = self.task_form_target();
        let default = self.default_run(employee, project).and_then(|(_, _, e)| e).map_or_else(|| "Claude Code decides".into(), |e| effort_label(&e));
        let mut items: MenuItems = vec![Self::task_option(format!("Employee default ({default})"), Some("This task".into()), TaskChoice::Effort(None), current.is_none()), None];
        items.extend(aurelius_domain::connection::EFFORT_LEVELS.iter().map(|e| {
            let mut chars = e.chars();
            let label: String = chars.next().map(|f| f.to_uppercase().chain(chars).collect()).unwrap_or_default();
            Self::task_option(label, None, TaskChoice::Effort(Some(e.to_string())), current.as_deref() == Some(*e))
        }));
        self.open_dropdown("task-effort", items, window, cx);
    }

    /// Tasks: give the selected task to a manager (a menu of the project's managers).
    fn on_assign_task(&mut self, _: &AssignSelectedTask, window: &mut Window, cx: &mut Context<Self>) {
        use aurelius_domain::task::TaskStatus;
        let Page::Project(ProjectRef::Real(project), Tab::Tasks) = self.screen else {
            self.notify_user("Select a task in a project's Tasks tab first.", false, cx);
            return;
        };
        let Some(task) = self.real_tasks(project).get(self.selected).map(|t| (*t).clone()) else { return };
        if let Some(why) = use_the_board(&task) {
            self.notify_user(why, false, cx);
            return;
        }
        if !matches!(task.status, TaskStatus::Draft | TaskStatus::Queued | TaskStatus::Waiting) {
            self.notify_user("A task can only be given to another employee while it is not running.", true, cx);
            return;
        }
        let mut items: MenuItems = self
            .assignable_managers(project)
            .into_iter()
            .map(|(id, name, role)| {
                Some(MenuEntry { label: name.into(), keys: None, run: MenuDo::Assign(task.id, Some(id)), danger: false, checked: task.employee_id == Some(id), detail: Some(role.into()) })
            })
            .collect();
        items.push(None);
        items.push(Some(MenuEntry {
            label: "No employee (draft)".into(),
            keys: None,
            run: MenuDo::Assign(task.id, None),
            danger: false,
            checked: task.employee_id.is_none(),
            detail: Some("Nothing runs until it has an employee".into()),
        }));
        let size = window.viewport_size();
        self.open_menu(point(size.width / 2. - px(160.), px(180.)), items, window, cx);
    }

    fn on_cancel_task(&mut self, _: &CancelSelectedTask, _: &mut Window, cx: &mut Context<Self>) {
        if let Page::Project(ProjectRef::Real(project), Tab::Tasks) = self.screen {
            if let Some(task) = self.real_tasks(project).get(self.selected).map(|t| (*t).clone()) {
                if let Some(why) = use_the_board(&task) {
                    self.notify_user(why, false, cx);
                } else if task.status.is_terminal() {
                    self.notify_user("This task is already finished.", false, cx);
                } else {
                    // "Task cancelled." comes with the server's answer (backend::send).
                    let _ = self.requests.send(backend::Request::CancelTask(task.id));
                }
            }
        }
    }

    /// Default employee for a new task: the open employee, else the single root (the Board's default).
    fn task_manager(&self, _project: ProjectId) -> Option<EmployeeId> {
        match self.screen {
            Page::Employee(ManagerRef::Real(id)) => Some(id),
            // With several roots the dialog leaves it open (a draft), unlike Give work.
            _ => match crate::screens::dashboard::roots(&self.managers).as_slice() {
                [only] => Some(*only),
                _ => None,
            },
        }
    }

    pub fn pending_approvals(&self) -> impl Iterator<Item = &ApprovalView> {
        self.approvals.iter().filter(|a| a.status == aurelius_domain::approval::ApprovalStatus::Pending)
    }

    pub fn needs_you(&self, id: ConversationId) -> Option<NeedsYou> {
        needs_you_of(&self.approvals, &self.tasks, id)
    }

    fn open_inbox(&mut self, _: &OpenInbox, window: &mut Window, cx: &mut Context<Self>) {
        self.close_overlay();
        self.navigate(Page::Inbox);
        // Open on what needs you; with nothing waiting, on the results.
        self.inbox_tab = if self.inbox_counts()[0] > 0 { InboxTab::NeedsYou } else { InboxTab::Results };
        self.inbox_selected = 0;
        self.inbox_new = self.inbox_notifications().iter().filter(|n| !n.read).map(|n| n.id.clone()).collect();
        if !self.inbox_new.is_empty() {
            let _ = self.requests.send(backend::Request::MarkNotificationsRead);
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Settings (Ctrl+, , Ctrl+9 and the app menu): one entry, five sections.
    pub fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_page(window, cx);
    }

    fn open_settings_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_overlay();
        self.navigate(Page::Settings);
        self.adding_connection = false;
        self.entering_key = false;
        let _ = self.requests.send(backend::Request::LoadBrowser);
        let _ = self.requests.send(backend::Request::LoadArtifactSetting);
        let _ = self.requests.send(backend::Request::LoadMcp);
        let _ = self.requests.send(backend::Request::LoadMcpAlwaysAllow);
        // The open page loads what it shows (the same as choosing it in the navigation).
        let tab = self.settings_tab;
        self.set_settings_tab(tab, cx);
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Short, human model name: "claude-opus-5-5" → "Opus 5.5", "sonnet" → "Sonnet".
    pub fn friendly_model(model: &str) -> String {
        let m = model.trim();
        if let Some((_, name, _)) = MODEL_CATALOG.iter().find(|(id, _, _)| id.split('|').any(|a| a == m)) {
            return (*name).to_string();
        }
        if m.starts_with("fake") {
            return "Test runtime".into();
        }
        let core = m.strip_prefix("claude-").unwrap_or(m);
        let mut parts = core.split('-').filter(|p| p.chars().all(|c| c.is_ascii_digit()) || p.chars().all(|c| c.is_alphabetic()));
        let Some(first) = parts.next() else { return m.to_owned() };
        let mut name: String = first.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default() + &first[1..];
        if first == "gpt" {
            name = "GPT".into();
        }
        let digits: Vec<&str> = parts.clone().filter(|p| p.chars().all(|c| c.is_ascii_digit()) && p.len() <= 2).take(2).collect();
        let words: Vec<String> = parts
            .filter(|p| p.chars().all(|c| c.is_alphabetic()))
            .map(|w| w.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default() + &w[1..])
            .collect();
        if !digits.is_empty() {
            name.push_str(if first == "gpt" { "-" } else { " " });
            name.push_str(&digits.join("."));
        }
        for w in words {
            name.push(' ');
            name.push_str(&w);
        }
        name
    }

    pub fn next_runtime(&self) -> NextRuntime {
        let label = self.next_runtime_label();
        let parts: Vec<&str> = label.split(" · ").collect();
        let (persona, rest): (Option<String>, &[&str]) = match parts.first() {
            Some(&"Marcus") => (Some("Marcus".into()), &parts[1..]),
            _ => (None, &parts[..]),
        };
        match rest {
            [connection, model, ..] => NextRuntime {
                persona,
                connection: (*connection).to_owned(),
                model: if *model == "default model" { "Default".into() } else { Self::friendly_model(model) },
            },
            [only] => NextRuntime { persona, connection: (*only).to_owned(), model: "—".into() },
            [] => NextRuntime { persona, connection: "—".into(), model: "—".into() },
        }
    }

    /// The global default of this page's purpose, with its connection (shown only; Settings edits it).
    pub(crate) fn run_choice_target(&self) -> Option<(aurelius_domain::connection::Purpose, aurelius_domain::connection::RunDefaults, aurelius_protocol::ProviderView)> {
        use aurelius_domain::connection::{ConfigScope, Purpose};
        use aurelius_domain::employee::Setting;
        let purpose = Purpose::Employee;
        let snapshot = self.connections.as_ref()?;
        let settings = snapshot.defaults.iter().find(|d| d.scope == ConfigScope::Global && d.purpose == purpose).map(|d| d.settings.clone()).unwrap_or_default();
        let Setting::Value(id) = settings.connection else { return None };
        let conn = snapshot.connections.iter().find(|c| c.id == id)?.clone();
        Some((purpose, settings, conn))
    }

    /// Effort currently chosen for the next run on this screen, if any.
    pub fn current_effort(&self) -> Option<String> {
        use aurelius_domain::employee::Setting;
        if self.thread_run().is_some() {
            return self.thread_effective().and_then(|(_, _, e)| e);
        }
        match self.run_choice_target()?.1.effort {
            Setting::Value(e) => Some(e),
            _ => None,
        }
    }

    /// What a conversation runs on when nothing is chosen for it or its task, resolved like
    /// the daemon (employee → project → global): the connection, model and effort.
    pub fn default_run(&self, employee: Option<EmployeeId>, project: Option<ProjectId>) -> Option<(aurelius_protocol::ProviderView, Option<String>, Option<String>)> {
        use aurelius_domain::connection::{ConfigScope, Purpose};
        use aurelius_domain::employee::Setting;
        let snapshot = self.connections.as_ref()?;
        let layers: Vec<_> = employee
            .map(ConfigScope::Employee)
            .into_iter()
            .chain(project.map(ConfigScope::Project))
            .chain([ConfigScope::Global])
            .filter_map(|s| self.run_layer(s, Purpose::Employee))
            .collect();
        let id = layers.iter().find_map(|s| match s.connection {
            Setting::Inherit => None,
            Setting::Disabled => Some(None),
            Setting::Value(id) => Some(Some(id)),
        })??;
        let conn = snapshot.connections.iter().find(|c| c.id == id)?.clone();
        let value = |s: &Setting<String>| match s {
            Setting::Value(v) => Some(v.clone()),
            _ => None,
        };
        let model = layers.iter().find_map(|s| value(&s.model)).or_else(|| conn.default_model.clone());
        let effort = layers.iter().find_map(|s| value(&s.effort));
        Some((conn, model, effort))
    }

    /// "Opus 5.5 · High" (the effort only when one is set).
    pub fn run_text(model: Option<&str>, effort: Option<&str>) -> String {
        let model = model.map_or_else(|| "Runtime default".into(), Self::friendly_model);
        match effort {
            Some(e) => format!("{model} · {}", effort_label(e)),
            None => model,
        }
    }

    /// "Employee default (Opus 5.5 · High)": what this employee (in this project) runs on.
    pub fn default_run_label(&self, employee: Option<EmployeeId>, project: Option<ProjectId>) -> String {
        match self.default_run(employee, project) {
            Some((_, m, e)) => format!("Employee default ({})", Self::run_text(m.as_deref(), e.as_deref())),
            None => "Employee default (no connection)".into(),
        }
    }

    /// The open conversation's task, if it is a task conversation.
    fn thread_task(&self) -> Option<&TaskView> {
        let (id, _) = self.thread_run()?;
        self.tasks.iter().find(|t| t.conversation_id == Some(id))
    }

    /// The open conversation's default (its employee's) and what it runs on now with the
    /// choice for the conversation, then for its task, on top: (default, model, effort).
    fn thread_effective(&self) -> Option<((aurelius_protocol::ProviderView, Option<String>, Option<String>), Option<String>, Option<String>)> {
        let (_, run) = self.thread_run()?;
        let snapshot = self.messages.as_ref()?;
        let default = self.default_run(snapshot.employee_id, snapshot.project_id)?;
        let task = self.thread_task().map(|t| t.run.clone()).unwrap_or_default();
        let model = run.model.or(task.model).or_else(|| default.1.clone());
        let effort = run.effort.or(task.effort).or_else(|| default.2.clone());
        Some((default, model, effort))
    }

    /// The open chat or task runs on something else than its employee's default: what
    /// ("Sonnet 5.5 · Low"), for the override chip.
    pub fn thread_override(&self) -> Option<String> {
        let ((_, dm, de), model, effort) = self.thread_effective()?;
        ((&model, &effort) != (&dm, &de)).then(|| Self::run_text(model.as_deref(), effort.as_deref()))
    }

    /// "This task" or "This chat": what the model popup changes.
    pub fn thread_scope(&self) -> &'static str {
        if self.thread_task().is_some() { "This task" } else { "This chat" }
    }

    /// The model popup (click on the model, or Ctrl+M) of an employee chat or task: the
    /// employee's default first, then models and effort for this conversation only.
    /// It never changes a default: those are set in Edit employee and Settings.
    pub fn model_menu(&self) -> MenuItems {
        let settings = || entry("Models: Edit employee, or Settings → Connections…", "secondary-,", || Box::new(OpenSettings));
        let Some(((conn, dm, de), model, effort)) = self.thread_effective() else { return vec![settings()] };
        let overridden = self.thread_override().is_some();
        let scope = self.thread_scope();
        let default = format!("Employee default ({})", Self::run_text(dm.as_deref(), de.as_deref()));
        let mut items: MenuItems = vec![
            Some(MenuEntry { label: default.into(), keys: None, run: MenuDo::Call(|this, _, cx| this.reset_thread_run(cx)), danger: false, checked: !overridden, detail: Some(scope.into()) }),
            None,
        ];
        items.extend(conn.models.iter().map(|m| {
            let detail = MODEL_CATALOG.iter().find(|(id, _, _)| id.split('|').any(|a| a == m)).map(|(_, _, d)| SharedString::from(*d)).filter(|d| !d.is_empty());
            let checked = overridden && model.as_deref() == Some(m.as_str());
            Some(MenuEntry { label: Self::friendly_model(m).into(), keys: None, run: MenuDo::Model(m.clone()), danger: false, checked, detail })
        }));
        if conn.provider == "claude_code" {
            items.push(None);
            for (level, label, detail) in EFFORTS {
                items.push(Some(MenuEntry {
                    label: format!("Effort: {label}").into(),
                    keys: None,
                    run: MenuDo::Effort(Some(level.to_string())),
                    danger: false,
                    checked: overridden && effort.as_deref() == Some(level),
                    detail: Some(detail.into()),
                }));
            }
        }
        items.push(None);
        items.push(settings());
        items
    }

    /// The open manager conversation and the model and effort chosen for it:
    /// there, the model popup (Ctrl+M) sets them for this conversation only.
    pub fn thread_run(&self) -> Option<(ConversationId, aurelius_domain::task::RunSettings)> {
        if !matches!(self.thread_screen(), Page::Employee(ManagerRef::Real(_))) {
            return None;
        }
        let snapshot = self.messages.as_ref().filter(|s| Some(s.conversation_id) == self.open_conversation)?;
        Some((snapshot.conversation_id, snapshot.run.clone()))
    }

    fn send_thread_run(&mut self, conversation_id: ConversationId, run: aurelius_domain::task::RunSettings, cx: &mut Context<Self>) {
        // Optimistic: show it at once; the daemon confirms with fresh messages.
        if let Some(s) = self.messages.as_mut().filter(|s| s.conversation_id == conversation_id) {
            s.run = run.clone();
        }
        let _ = self.requests.send(backend::Request::SetConversationRun { conversation_id, model: run.model, effort: run.effort });
        cx.notify();
    }

    /// "Back to employee default" (model popup, override chip): only this conversation changes.
    pub fn reset_thread_run(&mut self, cx: &mut Context<Self>) {
        let Some((conversation_id, _)) = self.thread_run() else { return };
        let task = self.thread_task().map(|t| t.run.clone()).unwrap_or_default();
        // shortcut: a model or effort chosen when the task was made has no client command to clear it, so the
        // conversation pins the employee's current default over it (a connection chosen for the task stays);
        // clear the task's choice instead once the server has a command for it.
        // Without a task choice, clearing the conversation's own choice is enough.
        let run = match self.thread_effective() {
            Some(((_, model, effort), _, _)) if task.model.is_some() || task.effort.is_some() => aurelius_domain::task::RunSettings { model, effort, ..Default::default() },
            _ => Default::default(),
        };
        self.send_thread_run(conversation_id, run, cx);
        // A task choice the default has no value for (often the effort) cannot be pinned over,
        // so it keeps applying and the override chip stays; say so instead of claiming a reset.
        // shortcut: becomes a full reset with the server's 'inherit' (v0.9.1, server-wave2-design.md §3b).
        let scope = self.thread_scope();
        match self.thread_override() {
            None => self.notify_user(format!("{scope} runs on the employee's default again."), false, cx),
            Some(run) => self.notify_user(format!("{scope} keeps {run}: the employee has no default to put over the task's own choice."), false, cx),
        }
    }

    /// A model or effort picked in the model popup, for the open conversation only.
    fn set_run_choice(&mut self, model: Option<String>, effort: Option<Option<String>>, cx: &mut Context<Self>) {
        // Never a default: without an employee chat or task there is nothing to set.
        let Some((conversation_id, mut run)) = self.thread_run() else { return };
        let scope = self.thread_scope();
        if let Some(m) = &model {
            run.model = Some(m.clone());
            self.notify_user(format!("{scope}: {}", Self::friendly_model(m)), false, cx);
        }
        if let Some(e) = &effort {
            run.effort = e.clone();
            self.notify_user(format!("{scope}: effort {}", e.as_deref().map_or_else(|| "of the employee".into(), effort_label)), false, cx);
        }
        self.send_thread_run(conversation_id, run, cx);
    }

    /// What the next message on this screen will use, resolved like the daemon does
    /// (manager → project → global) for display: "Claude Code · sonnet · global default".
    pub fn next_runtime_label(&self) -> String {
        use aurelius_domain::connection::{ConfigScope, Purpose};
        use aurelius_domain::employee::Setting;
        let Some(snapshot) = &self.connections else { return "Loading connection…".into() };
        let (purpose, scopes): (Purpose, Vec<(ConfigScope, String)>) = match self.thread_screen() {
            Page::Employee(ManagerRef::Real(manager)) => (
                Purpose::Employee,
                // The company chat has no project; a task conversation does.
                [(ConfigScope::Employee(manager), "employee".to_string())]
                    .into_iter()
                    .chain(self.thread_project().map(|p| (ConfigScope::Project(p), "project".to_string())))
                    .chain([(ConfigScope::Global, "global default".to_string())])
                    .collect(),
            ),
            Page::Employee(..) => return "Sample employee · no runtime".into(),
            _ => (Purpose::Employee, vec![(ConfigScope::Global, "global default".into())]),
        };
        let layers: Vec<_> = scopes
            .iter()
            .filter_map(|(scope, label)| {
                snapshot.defaults.iter().find(|d| d.scope == *scope && d.purpose == purpose).map(|d| (label.clone(), d.settings.clone()))
            })
            .collect();
        let mut connection = None;
        let mut origin = String::new();
        for (label, settings) in &layers {
            match settings.connection {
                Setting::Inherit => continue,
                Setting::Disabled => return format!("No connection ({label})"),
                Setting::Value(id) => {
                    connection = Some(id);
                    origin = label.clone();
                    break;
                }
            }
        }
        let model = self.thread_effective().and_then(|(_, m, _)| m).or_else(|| layers.iter().find_map(|(_, s)| match &s.model {
            Setting::Value(m) => Some(m.clone()),
            _ => None,
        }));
        match connection.and_then(|id| snapshot.connections.iter().find(|c| c.id == id)) {
            Some(c) => {
                let model = model.or(c.default_model.clone()).unwrap_or_else(|| "default model".into());
                format!("{} · {} · {}", c.display_name, model, origin)
            }
            None => "No connection configured".into(),
        }
    }

    pub fn selected_connection(&self) -> Option<aurelius_protocol::ProviderView> {
        self.connections.as_ref()?.connections.get(self.settings_selected).cloned()
    }

    pub fn chosen_model(&self, c: &aurelius_protocol::ProviderView) -> Option<String> {
        let index = self.settings_model.get(&c.id).copied()?;
        c.models.get(index).cloned()
    }

    fn settings_verify(&mut self, cx: &mut Context<Self>) {
        if let Some(c) = self.selected_connection() {
            let _ = self.requests.send(backend::Request::VerifyConnection(c.id));
            self.notify_user(format!("Verifying {} (no prompt is sent)…", c.display_name), false, cx);
        }
    }

    /// Company settings: make this connection and model the default of every employee.
    pub fn set_employee_default_model(&mut self, connection_id: aurelius_domain::ProviderId, model: String, cx: &mut Context<Self>) {
        let ready = self.connections.as_ref().and_then(|s| s.connections.iter().find(|c| c.id == connection_id)).is_some_and(|c| c.status == "ready");
        if !ready {
            self.notify_user("This connection isn't ready yet. Verify it under App settings → Connections.", true, cx);
            return;
        }
        let _ = self.requests.send(backend::Request::UseConnection { connection_id, model: Some(model) });
        cx.notify();
    }

    fn settings_use(&mut self, cx: &mut Context<Self>) {
        if self.settings_tab == crate::screens::remote::REMOTE_TAB {
            self.remote_decide(true, cx);
            return;
        }
        if let Some(c) = self.selected_connection() {
            if c.status != "ready" {
                self.notify_user("This connection isn't ready yet. Verify it first (V) or add a key (S).", true, cx);
                return;
            }
            let model = self.chosen_model(&c);
            let _ = self.requests.send(backend::Request::UseConnection { connection_id: c.id, model });
        }
    }

    fn settings_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.selected_connection() {
            Some(c) if c.auth_kind == "api_key" => {
                self.entering_key = true;
                self.key_input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                    input.focus(window, cx);
                });
                cx.notify();
            }
            Some(_) => self.notify_user("This connection doesn't use an API key. Sign in to Claude Code in Claude Code itself (`claude auth login`).", false, cx),
            None => {}
        }
    }

    fn save_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_push.is_some() {
            let team = self.team_input.read(cx).value().trim().to_string();
            self.team_input.update(cx, |input, cx| input.set_value("", window, cx));
            self.entering_key = false;
            self.focus.focus(window, cx);
            if team.is_empty() {
                self.pending_push = None;
            } else {
                self.save_push_team(team, cx);
            }
            cx.notify();
            return;
        }
        let secret = self.key_input.read(cx).value().trim().to_string();
        // Clear the field immediately; the value only travels to the daemon.
        self.key_input.update(cx, |input, cx| input.set_value("", window, cx));
        self.entering_key = false;
        self.focus.focus(window, cx);
        if let (Some(c), false) = (self.selected_connection(), secret.is_empty()) {
            let _ = self.requests.send(backend::Request::SetSecret { connection_id: c.id, secret });
        }
        cx.notify();
    }

    fn settings_remove(&mut self, cx: &mut Context<Self>) {
        if self.settings_tab == crate::screens::remote::REMOTE_TAB {
            self.remote_revoke(cx);
            return;
        }
        if let Some(c) = self.selected_connection() {
            if c.provider == "fake" {
                self.notify_user("The local test runtime is always available.", false, cx);
                return;
            }
            let _ = self.requests.send(backend::Request::RemoveConnection(c.id));
            self.notify_user(format!("{} removed. Running runs keep their configuration.", c.display_name), false, cx);
        }
    }

    fn settings_cycle_model(&mut self, cx: &mut Context<Self>) {
        if let Some(c) = self.selected_connection() {
            if c.models.is_empty() {
                self.notify_user("No models known; verify the connection first (V).", false, cx);
                return;
            }
            let next = self.settings_model.get(&c.id).map_or(0, |i| (i + 1) % c.models.len());
            self.settings_model.insert(c.id, next);
            cx.notify();
        }
    }

    fn add_provider(&mut self, provider: aurelius_domain::connection::ProviderKind, cx: &mut Context<Self>) {
        if !self.adding_connection {
            return;
        }
        use aurelius_domain::connection::ProviderKind;
        let (name, runtime_path) = match provider {
            // Looked up where it runs: on the server.
            ProviderKind::ClaudeCode => {
                self.adding_connection = false;
                let _ = self.requests.send(backend::Request::AddClaudeCode);
                cx.notify();
                return;
            }
            ProviderKind::AnthropicApi => ("Anthropic API".to_string(), None),
            ProviderKind::OpenaiApi => ("OpenAI API".to_string(), None),
            ProviderKind::Fake => return,
        };
        self.adding_connection = false;
        let _ = self.requests.send(backend::Request::AddConnection { provider, name, runtime_path });
        self.notify_user("Connection added; verifying…", false, cx);
        cx.notify();
    }

    /// The selected inbox item, when it is one of Claude's questions.
    pub fn selected_question(&self) -> Option<(ApprovalView, Vec<Question>)> {
        let a = self.inbox_approval()?.clone();
        (a.tool == QUESTION_TOOL).then(|| {
            let questions = questions_of(&a);
            (a, questions)
        })
    }

    /// The answers composed so far; `None` while a question is unanswered.
    pub fn question_answers(&self, a: &ApprovalView, questions: &[Question]) -> Option<std::collections::BTreeMap<String, String>> {
        questions
            .iter()
            .enumerate()
            .map(|(i, q)| {
                let key = (a.id, i);
                let answer = match self.question_own.get(&key) {
                    Some(own) => own.clone(),
                    None => {
                        let chosen = self.question_choices.get(&key)?;
                        let labels: Vec<&str> = chosen.iter().filter_map(|n| q.options.get(*n)).map(|(l, _)| l.as_str()).collect();
                        labels.join(", ")
                    }
                };
                (!answer.trim().is_empty()).then(|| (q.text.clone(), answer))
            })
            .collect()
    }

    /// 1–4 in the inbox: choose (or, for multi-select, toggle) an option of the focused question.
    pub fn choose_option(&mut self, n: usize, cx: &mut Context<Self>) {
        let Some((a, questions)) = self.selected_question() else { return };
        let i = self.question_focus.min(questions.len().saturating_sub(1));
        let Some(q) = questions.get(i) else { return };
        if n >= q.options.len() {
            return;
        }
        self.choose_option_for(a.id, i, n, cx);
        if !q.multi && i + 1 < questions.len() {
            self.question_focus = i + 1;
        }
    }

    /// Choose (or, for multi-select, toggle) option `n` of question `index` of a pending
    /// question: from the inbox or from its card in the conversation.
    pub fn choose_option_for(&mut self, approval: ApprovalId, index: usize, n: usize, cx: &mut Context<Self>) {
        let Some(a) = self.pending_approvals().find(|a| a.id == approval) else { return };
        let questions = questions_of(a);
        let Some(q) = questions.get(index) else { return };
        if n >= q.options.len() {
            return;
        }
        let key = (approval, index);
        self.question_own.remove(&key);
        let chosen = self.question_choices.entry(key).or_default();
        if q.multi {
            match chosen.iter().position(|c| *c == n) {
                Some(p) => {
                    chosen.remove(p);
                }
                None => chosen.push(n),
            }
        } else {
            *chosen = vec![n];
        }
        cx.notify();
    }

    /// Send the answers composed for a pending question; `false` while a question is unanswered.
    pub fn send_answers(&mut self, approval: ApprovalId) -> bool {
        let Some(a) = self.pending_approvals().find(|a| a.id == approval).cloned() else { return false };
        let Some(answers) = self.question_answers(&a, &questions_of(&a)) else { return false };
        let _ = self.requests.send(backend::Request::AnswerQuestion { approval_id: approval, answers });
        true
    }

    /// Answer question `index` in your own words, in a dialog over the inbox or the conversation.
    pub fn open_own_answer(&mut self, approval: ApprovalId, index: usize, in_inbox: bool, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.question_own.get(&(approval, index)).cloned().unwrap_or_default();
        self.open_dialog(DialogKind::QuestionAnswer { approval, index, in_inbox }, window, cx);
        self.name_input.update(cx, |input, cx| input.set_value(current, window, cx));
    }

    fn close_own_answer(&mut self, in_inbox: bool, window: &mut Window, cx: &mut Context<Self>) {
        // The inbox is a page now: closing the answer returns to whatever page is open.
        let _ = in_inbox;
        self.overlay = Overlay::None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn focus_question(&mut self, index: usize, cx: &mut Context<Self>) {
        self.question_focus = index;
        cx.notify();
    }

    fn on_question_next(&mut self, _: &QuestionNext, _: &mut Window, cx: &mut Context<Self>) {
        if let Some((_, questions)) = self.selected_question() {
            self.question_focus = (self.question_focus + 1) % questions.len().max(1);
            cx.notify();
        }
    }

    pub fn on_question_own(&mut self, _: &QuestionOwnAnswer, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((a, questions)) = self.selected_question() {
            let index = self.question_focus.min(questions.len().saturating_sub(1));
            self.open_own_answer(a.id, index, true, window, cx);
        }
    }

    fn decide_selected(&mut self, approve: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((a, _)) = self.selected_question().filter(|_| approve) {
            if self.send_answers(a.id) {
                self.question_focus = 0;
                if self.pending_approvals().count() <= 1 {
                    self.overlay = Overlay::None;
                    self.focus.focus(window, cx);
                }
            } else {
                self.notify_user("Answer every question first (1–4, or T for your own answer), or skip with Ctrl+Backspace.", true, cx);
            }
            cx.notify();
            return;
        }
        // A colleague a manager proposed: Approve / Decline.
        if let Some(proposal) = self.inbox_proposal().cloned() {
            self.decide_proposal(proposal.id, approve, cx);
            return;
        }
        // A guideline an employee proposed: Approve when the card allows it; Reject asks for a note.
        if let Some(id) = self.inbox_guideline_proposal().map(|p| p.id) {
            self.guideline_decide_selected(id, approve, window, cx);
            return;
        }
        let plan = self.inbox_approval().filter(|a| a.tool == PLAN_TOOL).cloned();
        if let Some(plan) = plan {
            // A plan is read first; approving, changes and rejecting happen in its drawer.
            self.open_plan_approval(plan, window, cx);
            return;
        }
        let Some(a) = self.inbox_approval().cloned() else {
            if approve {
                self.open_inbox_notification(window, cx);
            } else {
                self.notify_user("No pending approval selected.", false, cx);
            }
            return;
        };
        let _ = self.requests.send(backend::Request::DecideApproval { approval_id: a.id, action_digest: a.action_digest, approve, always_allow: false });
        if self.pending_approvals().count() <= 1 {
            self.overlay = Overlay::None;
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    /// Open a plan Claude handed in, in the drawer on the right.
    pub fn open_plan_approval(&mut self, approval: ApprovalView, window: &mut Window, cx: &mut Context<Self>) {
        self.plan_sending = None;
        self.plan_input.update(cx, |input, cx| input.set_value("", window, cx));
        self.clear_plan_comments(window, cx);
        self.overlay = Overlay::PlanApproval(approval.id);
        self.plan_snapshot = Some(approval);
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// Forget the inline comments and the one being written.
    pub fn clear_plan_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.plan_comments.clear();
        self.plan_comment_draft = None;
        self.plan_selection = None;
        self.plan_comment_input.update(cx, |input, cx| input.set_value("", window, cx));
    }

    /// Save the comment being written on the selected fragment.
    pub fn add_plan_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.plan_comment_input.read(cx).value().trim().to_string();
        let Some(quote) = self.plan_comment_draft.take() else { return };
        if !text.is_empty() {
            self.plan_comments.push(PlanComment { quote, text });
        }
        self.plan_comment_input.update(cx, |input, cx| input.set_value("", window, cx));
        gpui_kit::base::TextSelection::clear(window, cx);
        cx.notify();
    }

    /// Where the plan in the drawer stands (see [`PlanStatus`]).
    pub fn plan_status(&self, id: ApprovalId) -> PlanStatus {
        let pending = self.pending_approvals().any(|a| a.id == id);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
        match self.plan_sending {
            Some((sid, decision)) if sid == id => {
                if pending { PlanStatus::Sending(decision) } else { PlanStatus::Decided(decision) }
            }
            _ if !pending => PlanStatus::HandledElsewhere,
            _ if self.plan_snapshot.as_ref().is_some_and(|a| a.id == id && a.expires_at_ms <= now) => PlanStatus::Expired,
            _ => PlanStatus::Open,
        }
    }

    /// Approve, ask for changes on, or reject the plan in the drawer. The drawer
    /// closes; the plan and the decision stay readable in the conversation.
    pub fn decide_plan(&mut self, decision: aurelius_domain::approval::PlanDecision, window: &mut Window, cx: &mut Context<Self>) {
        let Overlay::PlanApproval(id) = self.overlay else { return };
        self.decide_plan_for(id, decision, window, cx);
    }

    /// The same decision for the plan card in the conversation, where no drawer is open;
    /// the feedback field (`plan_input`) is shared with the drawer.
    pub fn decide_plan_for(&mut self, id: ApprovalId, decision: aurelius_domain::approval::PlanDecision, window: &mut Window, cx: &mut Context<Self>) {
        use aurelius_domain::approval::PlanDecision;
        if self.plan_status(id) != PlanStatus::Open {
            return;
        }
        // Feedback goes with changes and with a rejection; an approval carries it only when there are comments.
        let free = self.plan_input.read(cx).value().trim().to_string();
        let feedback = if matches!(decision, PlanDecision::Approve | PlanDecision::ApproveEdits) && self.plan_comments.is_empty() { String::new() } else { compose_plan_feedback(&self.plan_comments, &free) };
        if decision == PlanDecision::Revise && feedback.is_empty() {
            self.notify_user("Say what should change, then ask for changes.", true, cx);
            return;
        }
        self.plan_sending = Some((id, decision));
        let _ = self.requests.send(backend::Request::DecidePlan { approval_id: id, decision, feedback: (!feedback.is_empty()).then_some(feedback) });
        if matches!(self.overlay, Overlay::PlanApproval(_)) {
            self.overlay = Overlay::None;
        }
        self.plan_input.update(cx, |input, cx| input.set_value("", window, cx));
        self.clear_plan_comments(window, cx);
        self.focus.focus(window, cx);
        self.notify_user(
            match decision {
                PlanDecision::Approve => "Plan approved. The agent goes on in auto mode.",
                PlanDecision::ApproveEdits => "Plan approved. Edits go through; the agent asks for the rest.",
                PlanDecision::Revise => "Changes requested. The agent revises the plan.",
                PlanDecision::Reject => "Plan rejected. The agent stops.",
            },
            false,
            cx,
        );
    }

    /// The employee whose settings (mode, browser, screen) apply here: the open employee,
    /// or the right panel's. They live on its main chat, its hidden workspace.
    pub fn settings_employee(&self) -> Option<&EmployeeSummary> {
        match self.thread_screen() {
            Page::Employee(ManagerRef::Real(id)) => self.managers.iter().find(|m| m.id == id),
            _ => None,
        }
    }

    /// The conversation the browser switch acts on (the employee's main chat), and whether the browser is on.
    pub fn browser_target(&self) -> Option<(ConversationId, bool)> {
        self.settings_employee().map(|m| (m.conversation_id, m.browser_allowed))
    }

    /// Turning the browser on asks first (it is the user's own Chrome, with their logins); off is immediate.
    pub fn toggle_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((conversation_id, allowed)) = self.browser_target() else {
            self.notify_user("Open an employee to allow its browser.", false, cx);
            return;
        };
        if allowed {
            for m in self.managers.iter_mut().filter(|m| m.conversation_id == conversation_id) {
                m.browser_allowed = false;
            }
            let _ = self.requests.send(backend::Request::SetBrowserAllowed { conversation_id, allowed: false });
            cx.notify();
        } else {
            self.overlay = Overlay::Confirm(ConfirmKind::AllowBrowser(conversation_id));
            self.overlay_focus.focus(window, cx);
            cx.notify();
        }
    }

    /// The conversation the screen switch acts on (the employee's main chat), and whether screenshots are allowed without asking.
    pub fn screen_target(&self) -> Option<(ConversationId, bool)> {
        self.settings_employee().map(|m| (m.conversation_id, m.screen_allowed))
    }

    /// Turning the screen on asks first (the agent may then look at anything on screen); off is immediate.
    /// Off, the agent can still ask for a screenshot; the user decides each time.
    pub fn toggle_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((conversation_id, allowed)) = self.screen_target() else {
            self.notify_user("Open an employee to allow it to see your screen.", false, cx);
            return;
        };
        if allowed {
            for m in self.managers.iter_mut().filter(|m| m.conversation_id == conversation_id) {
                m.screen_allowed = false;
            }
            let _ = self.requests.send(backend::Request::SetScreenAllowed { conversation_id, allowed: false });
            cx.notify();
        } else {
            self.overlay = Overlay::Confirm(ConfirmKind::AllowScreen(conversation_id));
            self.overlay_focus.focus(window, cx);
            cx.notify();
        }
    }

    /// Decide one pending approval straight from the chat that triggered it.
    pub fn decide_approval(&mut self, approval_id: ApprovalId, action_digest: String, approve: bool, cx: &mut Context<Self>) {
        let _ = self.requests.send(backend::Request::DecideApproval { approval_id, action_digest, approve, always_allow: false });
        cx.notify();
    }

    /// Open what the selected inbox notification is about (its task).
    fn open_inbox_notification(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.inbox_note().and_then(|n| n.task_id);
        let Some(task) = target.and_then(|id| self.tasks.iter().find(|t| t.id == id)).cloned() else {
            self.notify_user("This notification has no task to open.", false, cx);
            return;
        };
        self.overlay = Overlay::None;
        self.task_filter = if !crate::ui::is_active(&task) { 2 } else if task.status == aurelius_domain::task::TaskStatus::Waiting { 1 } else { 0 };
        self.navigate(Page::Project(ProjectRef::Real(task.project_id), Tab::Tasks));
        self.selected = self.real_tasks(task.project_id).iter().position(|t| t.id == task.id).unwrap_or(0);
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn running_jobs(&self) -> impl Iterator<Item = &JobSummary> {
        use aurelius_domain::job::JobState;
        self.jobs.iter().filter(|j| matches!(j.state, JobState::Ready | JobState::Leased))
    }

    /// The ideas list is on screen: a project's Ideas tab, or the Ideas tab of Projects.
    pub fn on_ideas_screen(&self) -> bool {
        matches!(self.screen, Page::Project(ProjectRef::Real(_), Tab::Ideas)) || (self.screen == Page::Projects && self.projects_tab == ProjectsTab::Ideas)
    }

    /// Number of keyboard-selectable items on the current screen.
    fn item_count(&self) -> usize {
        match self.screen {
            Page::Projects if self.projects_tab == ProjectsTab::Projects => self.home_projects().len(),
            Page::Project(ProjectRef::Real(id), Tab::Tasks) => self.real_tasks(id).len(),
            Page::Project(p, Tab::Tasks) if self.sample(p).is_some() => match self.task_filter {
                0 => preview::ACTIVE_TASKS.iter().filter(|t| !t.waiting).count(),
                1 => preview::ACTIVE_TASKS.iter().filter(|t| t.waiting).count(),
                _ => preview::COMPLETED_TASKS.len(),
            },
            Page::Project(ProjectRef::Real(id), Tab::Memory) if self.memory_filter == 0 => self.memory_ctx_rows(id).len(),
            Page::Project(ProjectRef::Real(id), Tab::Memory) => self.real_memory(id).len(),
            Page::Project(ProjectRef::Real(id), Tab::Ideas) => self.real_ideas(Some(id)).len(),
            Page::Projects if self.projects_tab == ProjectsTab::Ideas => self.real_ideas(None).len(),
            _ => 0,
        }
    }

    /// Memory items of a project under the current filter, in display order
    /// (project and global first, then managers).
    pub fn real_memory(&self, project: ProjectId) -> Vec<&aurelius_protocol::MemoryView> {
        use aurelius_domain::memory::MemoryStatus::*;
        let Some((_, items)) = self.memory.as_ref().filter(|(p, _)| *p == project) else { return Vec::new() };
        if self.memory_filter == 1 {
            // Proposals, newest first.
            let mut list: Vec<_> = items.iter().filter(|m| m.status == Proposed).collect();
            list.sort_by_key(|m| std::cmp::Reverse(m.created_at_ms));
            return list;
        }
        // "More": what was superseded, rejected or forgotten.
        items.iter().filter(|m| matches!(m.status, Superseded | Rejected | Forgotten)).collect()
    }

    /// The manager whose context the "In context" view shows: the picked one,
    /// else the project's first manager, else none (the project conversation).
    pub fn memory_ctx_manager_for(&self, project: ProjectId) -> Option<EmployeeId> {
        let managers = || self.managers.iter().filter(move |m| m.projects_active.contains(&project));
        match self.memory_ctx_manager {
            Some(Some(id)) if managers().any(|m| m.id == id) => Some(id),
            Some(None) => None,
            _ => managers().next().map(|m| m.id),
        }
    }

    /// Ask for the context view of the shown manager unless it is loaded or on its way.
    pub fn ensure_memory_ctx(&mut self, project: ProjectId) {
        let manager = self.memory_ctx_manager_for(project);
        let loaded = self.memory_ctx.as_ref().is_some_and(|(p, m, _)| *p == project && *m == manager);
        if !loaded && self.memory_ctx_requested != Some((project, manager)) {
            self.memory_ctx_requested = Some((project, manager));
            let _ = self.requests.send(backend::Request::LoadMemoryContext { project, manager });
        }
    }

    pub fn pick_memory_ctx_manager(&mut self, manager: Option<EmployeeId>) {
        self.memory_ctx_manager = Some(manager);
        self.memory_over_open = false;
        self.memory_dups_open = false;
        self.selected = 0;
    }

    /// The context view for the shown manager, if its answer is in.
    pub fn memory_ctx_view(&self, project: ProjectId) -> Option<&aurelius_protocol::MemoryContextView> {
        let manager = self.memory_ctx_manager_for(project);
        match self.memory_ctx.as_ref() {
            Some((p, m, Ok(view))) if *p == project && *m == manager => Some(view),
            _ => None,
        }
    }

    /// Items of one placement, in the order the server sent them.
    pub fn memory_ctx_placed(&self, project: ProjectId, placement: &str) -> Vec<&aurelius_protocol::MemoryContextItem> {
        self.memory_ctx_view(project).map(|v| v.items.iter().filter(|i| i.placement == placement).collect()).unwrap_or_default()
    }

    /// The rows the "In context" view shows now, top to bottom (folded sections excluded).
    pub fn memory_ctx_rows(&self, project: ProjectId) -> Vec<&aurelius_protocol::MemoryContextItem> {
        let mut rows = self.memory_ctx_placed(project, "always");
        rows.extend(self.memory_ctx_placed(project, "when_relevant"));
        let over = self.memory_ctx_placed(project, "over_budget");
        if over.len() <= 5 || self.memory_over_open {
            rows.extend(over);
        }
        if self.memory_dups_open {
            rows.extend(self.memory_ctx_placed(project, "duplicate"));
        }
        rows
    }

    pub fn decide_memory(&mut self, memory_id: aurelius_domain::MemoryId, accept: bool, cx: &mut Context<Self>) {
        let _ = self.requests.send(backend::Request::DecideMemory { memory_id, accept });
        self.notify_user(if accept { "Proposal accepted; it is used as context from now on." } else { "Proposal rejected." }, false, cx);
    }

    /// A project's ideas under the current filter (open, done or dropped), in the order the
    /// Ideas tab shows them: yours first, then the managers'.
    pub fn real_ideas(&self, scope: Option<ProjectId>) -> Vec<&aurelius_protocol::IdeaView> {
        use aurelius_domain::idea::IdeaStatus;
        let Some((_, items)) = self.ideas.as_ref().filter(|(p, _)| *p == scope) else { return Vec::new() };
        // The Ideas page can narrow the company-wide list to one project.
        let only = if scope.is_none() { self.ideas_project_filter } else { None };
        let wanted = match self.ideas_filter {
            1 => IdeaStatus::Done,
            2 => IdeaStatus::Dropped,
            _ => IdeaStatus::Open,
        };
        let (mut yours, managers) = split_ideas(items.iter().filter(|i| only.is_none_or(|p| i.project_id == p) && i.status == wanted && self.ideas_author.admits(i) && self.ideas_kind.is_none_or(|k| i.kind == k)));
        yours.extend(managers);
        yours
    }

    pub fn ensure_ideas(&mut self, scope: Option<ProjectId>) {
        let loaded = self.ideas.as_ref().is_some_and(|(p, _)| *p == scope);
        if !loaded && self.ideas_requested != Some(scope) {
            self.ideas_requested = Some(scope);
            let _ = self.requests.send(backend::Request::LoadIdeas(scope));
        }
    }

    fn selected_idea(&self) -> Option<aurelius_protocol::IdeaView> {
        match self.screen {
            Page::Project(ProjectRef::Real(p), Tab::Ideas) => self.real_ideas(Some(p)).get(self.selected).map(|i| (*i).clone()),
            Page::Projects if self.projects_tab == ProjectsTab::Ideas => self.real_ideas(None).get(self.selected).map(|i| (*i).clone()),
            _ => None,
        }
    }

    /// Opens the selected idea's menu on its first "Start as a task", the mode back on the manager's default.
    pub fn open_idea_menu(&mut self, anchor: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.idea_start_mode = None;
        self.idea_start_model = None;
        let items = self.idea_menu();
        self.open_menu(anchor, items, window, cx);
        if let Some(menu) = self.menu.as_mut() {
            if let Some(i) = menu.entries.iter().position(|e| e.as_ref().is_some_and(|e| matches!(e.run, MenuDo::StartIdea(..)))) {
                menu.selected = i;
            }
        }
    }

    /// The selected idea's actions (⋯, right-click): the task's mode (servers from 1.8),
    /// start as a task for a manager, edit, done, drop.
    pub fn idea_menu(&self) -> MenuItems {
        use aurelius_domain::{autonomy::AgentMode, idea::IdeaStatus};
        let Some(idea) = self.selected_idea() else { return vec![] };
        let mut items: MenuItems = Vec::new();
        if idea.task_id.is_none() {
            let managers = self.assignable_managers(idea.project_id);
            if !managers.is_empty() {
                let current = self.idea_start_mode;
                items.push(Some(MenuEntry { label: "Employee's default".into(), keys: None, run: MenuDo::IdeaMode(None), danger: false, checked: current.is_none(), detail: Some("Mode".into()) }));
                for m in [AgentMode::Ask, AgentMode::AcceptEdits, AgentMode::Auto, AgentMode::Plan] {
                    let label = format!("{} {}", crate::screens::chat::mode_icon(m), m.label());
                    items.push(Some(MenuEntry { label: label.into(), keys: None, run: MenuDo::IdeaMode(Some(m)), danger: false, checked: current == Some(m), detail: None }));
                }
                items.push(None);
                // Model: each employee's own default (named beside them below), or one for this task.
                let model = self.idea_start_model.clone();
                items.push(Some(MenuEntry { label: "Model: employee default".into(), keys: None, run: MenuDo::IdeaModel(None), danger: false, checked: model.is_none(), detail: Some("This task".into()) }));
                for (c, m, label) in self.task_model_options() {
                    let checked = model.as_ref().is_some_and(|(cc, cm)| *cc == c && *cm == m);
                    items.push(Some(MenuEntry { label: format!("Model: {label}").into(), keys: None, run: MenuDo::IdeaModel(Some((c, m))), danger: false, checked, detail: None }));
                }
                items.push(None);
            }
            let chosen = self.idea_start_model.as_ref().map(|(_, m)| Self::friendly_model(m));
            for (id, name, role) in managers {
                let run = chosen.clone().unwrap_or_else(|| self.default_run_label(Some(id), Some(idea.project_id)));
                items.push(Some(MenuEntry { label: format!("Start as a task for {name}").into(), keys: None, run: MenuDo::StartIdea(idea.id, id), danger: false, checked: false, detail: Some(format!("{role} · {run}").into()) }));
            }
            items.push(None);
        }
        items.push(entry("Edit…", "f2", || Box::new(EditItem)));
        if idea.status != IdeaStatus::Open {
            items.push(Some(MenuEntry { label: "Reopen".into(), keys: None, run: MenuDo::IdeaStatus(idea.id, IdeaStatus::Open), danger: false, checked: false, detail: None }));
        }
        if idea.status != IdeaStatus::Done {
            items.push(Some(MenuEntry { label: "Mark done".into(), keys: None, run: MenuDo::IdeaStatus(idea.id, IdeaStatus::Done), danger: false, checked: false, detail: None }));
        }
        if idea.status != IdeaStatus::Dropped {
            items.push(danger(Some(MenuEntry { label: "Drop".into(), keys: Some("delete"), run: MenuDo::IdeaStatus(idea.id, IdeaStatus::Dropped), danger: true, checked: false, detail: None })));
        }
        if self.idea_attachments_on() {
            let keys = (idea.status == IdeaStatus::Dropped).then_some("delete");
            items.push(danger(Some(MenuEntry { label: "Delete…".into(), keys, run: MenuDo::Call(|this, w, cx| this.ask_delete_idea(w, cx)), danger: true, checked: false, detail: None })));
        }
        items
    }

    /// Ask before deleting the selected idea for good.
    pub fn ask_delete_idea(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.selected_idea() else { return };
        self.overlay = Overlay::Confirm(ConfirmKind::DeleteIdea(i.id));
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn ensure_memory(&mut self, project: ProjectId) {
        let loaded = self.memory.as_ref().is_some_and(|(p, _)| *p == project);
        if !loaded && self.memory_requested != Some(project) {
            self.memory_requested = Some(project);
            let _ = self.requests.send(backend::Request::LoadMemory(project));
        }
    }

    fn selected_memory(&self) -> Option<aurelius_protocol::MemoryView> {
        match self.screen {
            Page::Project(ProjectRef::Real(p), Tab::Memory) if self.memory_filter == 0 => {
                let id = self.memory_ctx_rows(p).get(self.selected)?.memory_id;
                self.memory.as_ref()?.1.iter().find(|m| m.id == id).cloned()
            }
            Page::Project(ProjectRef::Real(p), Tab::Memory) => self.real_memory(p).get(self.selected).map(|m| (*m).clone()),
            _ => None,
        }
    }

    fn on_accept_memory(&mut self, _: &AcceptMemory, _: &mut Window, cx: &mut Context<Self>) {
        match self.selected_memory() {
            Some(m) if m.status == aurelius_domain::memory::MemoryStatus::Proposed => {
                let _ = self.requests.send(backend::Request::DecideMemory { memory_id: m.id, accept: true });
                self.notify_user("Proposal accepted; it is used as context from now on.", false, cx);
            }
            Some(_) => self.notify_user("Select a proposal to accept (filter 2 shows only proposals).", false, cx),
            None => self.notify_user("Open a project's Memory tab (Alt 5).", false, cx),
        }
    }

    pub fn resume_work(&mut self, cx: &mut Context<Self>) {
        if self.backups.as_ref().is_some_and(|b| b.paused.is_some()) {
            let _ = self.requests.send(backend::Request::Backups(backend::BackupsCall::Resume));
        } else if self.screen == Page::Settings && self.settings_tab == 2 {
            self.notify_user("Nothing is paused.", false, cx);
        }
    }

    fn place(&self) -> Place {
        Place { screen: self.screen }
    }

    /// Remember every change of place (called on each render), so ← and →
    /// work like a browser whatever caused the change.
    fn track_place(&mut self) {
        let now = self.place();
        if let Some(last) = self.last_place.filter(|l| *l != now) {
            self.history_back.push(last);
            if self.history_back.len() > 200 {
                self.history_back.remove(0);
            }
            self.history_forward.clear();
        }
        self.last_place = Some(now);
    }

    fn go_to_place(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Overlay::None;
        self.screen = place.screen;
        self.selected = 0;
        self.open_thread();
        // A jump through history is not a new step.
        self.last_place = Some(self.place());
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn on_nav_back(&mut self, _: &NavBack, window: &mut Window, cx: &mut Context<Self>) {
        self.track_place();
        if let Some(previous) = self.history_back.pop() {
            self.history_forward.push(self.place());
            self.go_to_place(previous, window, cx);
        }
    }

    fn on_nav_forward(&mut self, _: &NavForward, window: &mut Window, cx: &mut Context<Self>) {
        self.track_place();
        if let Some(next) = self.history_forward.pop() {
            self.history_back.push(self.place());
            self.go_to_place(next, window, cx);
        }
    }

    pub(crate) fn on_toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        self.sidebar_peek = false;
        set_ui_pref("sidebar_hidden", self.sidebar_collapsed);
        cx.notify();
    }

    /// Open a popup menu at `anchor` (window coordinates).
    pub fn set_settings_tab(&mut self, tab: usize, cx: &mut Context<Self>) {
        self.settings_tab = tab;
        self.settings_selected = 0;
        self.adding_connection = false;
        self.server_name.editing = false;
        match tab {
            1 => {
                self.login_start = aurelius_host::startup::enabled();
                let _ = self.requests.send(backend::Request::CheckUpdate(false));
            }
            crate::screens::overlays::SERVER_PC_TAB => {
                let _ = self.requests.send(backend::Request::CheckUpdate(false));
                // Its name may have changed while the event could not arrive (only servers that have it).
                if matches!(&self.connection, Connection::Online { server, .. } if server.server_name) {
                    let _ = self.requests.send(backend::Request::ServerName(backend::ServerNameCall::Load));
                }
            }
            crate::screens::trace::TRACE_TAB => self.enter_trace_tab(),
            crate::screens::schedules::SCHEDULES_TAB => {}
            crate::screens::mcp::MCP_TAB => self.enter_mcp_tab(),
            2 => {
                let _ = self.requests.send(backend::Request::Backups(backend::BackupsCall::List));
            }
            crate::screens::remote::REMOTE_TAB => self.enter_remote_tab(cx),
            crate::screens::load::LOAD_TAB => self.enter_load_tab(),
            crate::screens::app_keys::APP_KEYS_TAB => self.app_keys_load(),
            crate::screens::plugins::PLUGINS_TAB => self.enter_plugins_tab(),
            crate::screens::account::ACCOUNT_TAB => self.enter_account_tab(),
            crate::screens::servers::SERVERS_TAB => {
                self.reload_servers();
                self.adding_server = false;
                // Start on the server in use.
                let active = self.active_server();
                self.settings_selected = self.server_rows().iter().position(|r| r.host_id == active).unwrap_or(0);
            }
            _ => {}
        }
        cx.notify();
    }

    // ── Menus (⋯ and right-click): every action reachable without shortcuts ──

    pub fn app_menu(&self) -> MenuItems {
        vec![
            call("Give work", "", |this, w, cx| {
                this.go_nav(NavItem::Dashboard, w, cx);
                this.give_goal.update(cx, |i, cx| i.focus(w, cx));
            }),
            entry("New project…", "secondary-shift-n", || Box::new(NewProject)),
            entry("Add employee…", "secondary-shift-m", || Box::new(AddEmployee)),
            entry("Search and commands…", "secondary-k", || Box::new(OpenPalette)),
            None,
            entry("Inbox", "secondary-i", || Box::new(OpenInbox)),
            entry("Memory…", "", || Box::new(OpenMemory)),
            entry("Usage", "secondary-u", || Box::new(OpenUsage)),
            None,
            entry("Settings", "secondary-,", || Box::new(OpenSettings)),
            entry("Updates", "secondary-shift-u", || Box::new(OpenUpdates)),
            entry(if self.show_preview { "Hide sample data" } else { "Show sample data" }, "secondary-shift-d", || Box::new(TogglePreview)),
            None,
            entry("Close window (work continues)", "secondary-q", || Box::new(Quit)),
        ]
    }

    /// The project's ⋯ menu: new window, edit (name, description, folder), folder, export,
    /// memory, pause and archive.
    pub fn project_menu(&self) -> MenuItems {
        let paused = self.current_project().and_then(|p| self.real(p)).is_some_and(|p| p.status == aurelius_domain::ProjectStatus::Paused);
        let mut items = vec![
            entry("Edit…", "secondary-shift-,", || Box::new(OpenProjectSettings)),
            entry("Open folder", "secondary-shift-o", || Box::new(OpenProjectFolder)),
            entry("Export (Markdown + JSON)", "secondary-shift-a", || Box::new(ExportProject)),
            call("Memory", "", |this, w, cx| {
                if let Some(ProjectRef::Real(p)) = this.current_project() {
                    this.open_project_memory(p, w, cx);
                }
            }),
            None,
            call(if paused { "Resume project" } else { "Pause project" }, "", |this, w, cx| {
                if let Some(p) = match this.screen { Page::Employee(m) => m.sample_project(), _ => None } {
                    this.navigate(Page::Project(p, Tab::Overview));
                }
                this.on_toggle_pause(&TogglePause, w, cx);
            }),
            danger(call("Archive project…", "", |this, w, cx| {
                if let Some(p) = match this.screen { Page::Employee(m) => m.sample_project(), _ => None } {
                    this.navigate(Page::Project(p, Tab::Overview));
                }
                this.on_archive(&ArchiveItem, w, cx);
            })),
        ];
        if self.window_project.is_none() {
            items.insert(0, call("Open in new window", "", |this, _, cx| {
                if let Some(ProjectRef::Real(p)) = this.current_project() {
                    this.open_project_window(p, cx);
                }
            }));
        }
        items
    }

    /// The memory list of a project (project menu; Memory in the app menu and the palette).
    pub fn open_project_memory(&mut self, project: ProjectId, window: &mut Window, cx: &mut Context<Self>) {
        self.close_overlay();
        self.navigate(Page::Project(ProjectRef::Real(project), Tab::Memory));
        self.memory_filter = 0;
        self.selected = 0;
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Memory… (app menu, palette): the open project's list, else a choice of project.
    fn on_open_memory(&mut self, _: &OpenMemory, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ProjectRef::Real(project)) = self.current_project() {
            return self.open_project_memory(project, window, cx);
        }
        let mut projects: Vec<(ProjectId, String)> = self.projects.iter().map(|p| (p.id, p.name.clone())).collect();
        match projects.len() {
            0 => self.notify_user("Make a project first: memory belongs to a project.", false, cx),
            1 => self.open_project_memory(projects.remove(0).0, window, cx),
            _ => {
                let items: MenuItems = projects
                    .into_iter()
                    .map(|(id, name)| Some(MenuEntry { label: name.into(), keys: None, run: MenuDo::OpenMemory(id), danger: false, checked: false, detail: Some("Memory of".into()) }))
                    .collect();
                let size = window.viewport_size();
                self.open_menu(point(size.width / 2. - px(140.), px(160.)), items, window, cx);
            }
        }
    }

    /// New recurring task (Settings → Advanced → Recurring schedules): choose its project first.
    pub fn pick_schedule_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items: MenuItems = self
            .projects
            .iter()
            .map(|p| Some(MenuEntry { label: p.name.clone().into(), keys: None, run: MenuDo::NewSchedule(p.id), danger: false, checked: false, detail: Some("New recurring task in".into()) }))
            .collect();
        if items.is_empty() {
            self.notify_user("Make a project first: a recurring task belongs to a project.", false, cx);
            return;
        }
        let size = window.viewport_size();
        self.open_menu(point(size.width / 2. - px(140.), px(160.)), items, window, cx);
    }

    pub fn manager_menu(&self) -> MenuItems {
        let paused = match self.screen {
            Page::Employee(m) => self.real_manager(m).is_some_and(|m| m.status == LifecycleStatus::Paused),
            _ => false,
        };
        let pin_entry = match self.screen {
            Page::Employee(ManagerRef::Real(id)) => Some(MenuEntry {
                label: if self.employee_pins.contains(&id) { "Unpin from the sidebar" } else { "Pin to the sidebar" }.into(),
                keys: None,
                run: MenuDo::TogglePinEmployee(id),
                danger: false,
                checked: false,
                detail: None,
            }),
            _ => None,
        };
        vec![
            entry("Edit employee…", "f2", || Box::new(EditItem)),
            pin_entry,
            entry("Switch mode", "shift-tab", || Box::new(CycleAgentMode)),
            entry("Heartbeat: off / 30 min / 1 h / 4 h", "alt-h", || Box::new(CycleHeartbeat)),
            entry("Browser on / off", "alt-b", || Box::new(ToggleBrowser)),
            entry("Screen on / off", "alt-s", || Box::new(ToggleScreen)),
            None,
            entry("Choose workspace folder…", "secondary-shift-g", || Box::new(ChangeProjectFolder)),
            entry("Open workspace folder", "secondary-shift-o", || Box::new(OpenProjectFolder)),
            entry("Show / hide task history", "secondary-shift-h", || Box::new(ToggleHistory)),
            None,
            entry(if paused { "Resume employee" } else { "Pause employee" }, "secondary-shift-p", || Box::new(TogglePause)),
            danger(entry("Archive employee…", "secondary-shift-backspace", || Box::new(ArchiveItem))),
        ]
    }

    pub fn task_menu(&self) -> MenuItems {
        let task = match self.screen {
            Page::Project(ProjectRef::Real(project), Tab::Tasks) => self.real_tasks(project).get(self.selected).map(|t| (*t).clone()),
            _ => None,
        };
        let Some(t) = task else { return vec![] };
        let mut items = Vec::new();
        // A ticket's main task is stopped and given on the Board (the server refuses it here).
        if let Some(ticket) = t.ticket_id.filter(|_| is_ticket_main_task(&t)) {
            items.push(crate::screens::board::menu_entry("Open ticket", crate::screens::board::BoardDo::Open(ticket), false, None));
        } else if !t.status.is_terminal() {
            items.push(danger(entry("Cancel task", "secondary-shift-x", || Box::new(CancelSelectedTask))));
        }
        items
    }

    pub fn thread_menu(&self) -> MenuItems {
        let mut items: MenuItems = Vec::new();
        items.push(entry("Show context of last answer", "secondary-shift-e", || Box::new(ShowContext)));
        items.push(entry("Retry last answer", "secondary-r", || Box::new(RetryAnswer)));
        items.push(None);
        items.push(danger(call("Delete conversation…", "", |this, w, cx| this.ask_delete_thread(w, cx))));
        items
    }

    pub fn memory_menu(&self) -> MenuItems {
        use aurelius_domain::memory::MemoryStatus;
        let status = self.selected_memory().map(|m| m.status);
        let mut items = Vec::new();
        if status == Some(MemoryStatus::Proposed) {
            items.push(entry("Accept proposal", "secondary-shift-y", || Box::new(AcceptMemory)));
        }
        if status == Some(MemoryStatus::Active) {
            items.push(entry("Revise…", "f2", || Box::new(EditItem)));
        }
        items.push(entry("Pin / unpin", "secondary-shift-b", || Box::new(TogglePin)));
        items.push(None);
        items.push(danger(entry(if status == Some(MemoryStatus::Proposed) { "Reject" } else { "Forget…" }, "delete", || Box::new(RemoveMemory))));
        items
    }

    pub fn open_menu(&mut self, anchor: Point<Pixels>, entries: MenuItems, window: &mut Window, cx: &mut Context<Self>) {
        // Start on the current choice, if the menu marks one.
        let selected = entries.iter().position(|e| e.as_ref().is_some_and(|e| e.checked)).or_else(|| entries.iter().position(Option::is_some)).unwrap_or(0);
        let parent = matches!(self.overlay, Overlay::Dialog(_)).then(|| (self.overlay, window.focused(cx)));
        self.menu = Some(Menu { anchor, entries, selected, width: None, field: None, trigger_top: None, parent });
        self.overlay = Overlay::Menu;
        self.menu_scroll.scroll_to_item(selected);
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// A dropdown: the menu right below its field (drawn with `dropdown_bounds`), as wide as it.
    pub fn open_dropdown(&mut self, field: &'static str, entries: MenuItems, window: &mut Window, cx: &mut Context<Self>) {
        // A shortcut for a field folded under "More" / "Advanced" unfolds it; its old bounds are stale.
        let folded = match field {
            // Only the form in use: the task dialog over a page, else the dashboard's Give work.
            "task-mode" | "task-model" | "task-effort" if matches!(self.overlay, Overlay::Dialog(_)) => !std::mem::replace(&mut self.task_more_open, true),
            "task-mode" | "task-model" | "task-effort" => !std::mem::replace(&mut self.give_more_open, true),
            "subagent-model" => !std::mem::replace(&mut self.employee_advanced_open, true),
            _ => false,
        };
        if folded {
            self.dropdown_bounds.borrow_mut().remove(field);
        }
        let bounds = self.dropdown_bounds.borrow().get(field).copied();
        let anchor = match bounds {
            Some(b) => point(b.origin.x, b.origin.y + b.size.height + px(4.)),
            None => point(window.viewport_size().width / 2. - px(160.), px(180.)),
        };
        self.open_menu(anchor, entries, window, cx);
        if let Some(menu) = self.menu.as_mut() {
            menu.width = bounds.map(|b| f32::from(b.size.width));
            menu.field = Some(field);
            menu.trigger_top = bounds.map(|b| f32::from(b.origin.y));
        }
    }

    /// Closes the menu: back to the dialog it opened over, else to the screen.
    fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.menu.take().and_then(|m| m.parent) {
            Some((overlay, focus)) => {
                self.overlay = overlay;
                match focus {
                    Some(f) => f.focus(window, cx),
                    None => self.name_input.update(cx, |i, cx| i.focus(window, cx)),
                }
            }
            None => {
                self.overlay = Overlay::None;
                self.focus.focus(window, cx);
            }
        }
        cx.notify();
    }

    pub fn run_menu_entry(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.menu.as_ref().and_then(|m| m.entries.get(index).cloned().flatten()) else { return };
        if let MenuDo::IdeaMode(_) | MenuDo::IdeaModel(_) = entry.run {
            // Pick the mode (model), then the manager: the menu stays open, its marks follow.
            match entry.run {
                MenuDo::IdeaMode(mode) => self.idea_start_mode = mode,
                MenuDo::IdeaModel(m) => self.idea_start_model = m,
                _ => {}
            }
            let entries = self.idea_menu();
            if let Some(menu) = self.menu.as_mut() {
                menu.entries = entries;
                menu.selected = index;
            }
            cx.notify();
            return;
        }
        self.close_menu(window, cx);
        match entry.run {
            MenuDo::Action(make) => window.dispatch_action(make(), cx),
            MenuDo::Call(f) => f(self, window, cx),
            MenuDo::Model(model) => self.set_run_choice(Some(model), None, cx),
            MenuDo::Effort(effort) => self.set_run_choice(None, Some(effort), cx),
            MenuDo::StartIdea(idea_id, employee_id) => {
                let mode = std::mem::take(&mut self.idea_start_mode);
                let model = self.idea_start_model.take();
                let _ = self.requests.send(backend::Request::StartIdea { idea_id, employee_id, mode, model });
                let who = self.managers.iter().find(|x| x.id == employee_id).map(|x| x.name.clone()).unwrap_or_default();
                self.notify_user(match mode {
                    Some(m) => format!("Started as a task for {who}, in {}.", m.label()),
                    None => format!("Started as a task for {who}."),
                }, false, cx);
            }
            MenuDo::Unpin(project) => self.toggle_pin(project, cx),
            MenuDo::TogglePinEmployee(employee) => self.toggle_employee_pin(employee, cx),
            MenuDo::OpenMemory(project) => self.open_project_memory(project, window, cx),
            MenuDo::NewSchedule(project) => self.new_schedule_in(project, window, cx),
            MenuDo::IdeaMode(_) | MenuDo::IdeaModel(_) => {}
            MenuDo::IdeaStatus(idea_id, status) => {
                let _ = self.requests.send(backend::Request::SetIdeaStatus { idea_id, status });
            }
            MenuDo::Assign(task_id, employee_id) => {
                let _ = self.requests.send(backend::Request::AssignTask { task_id, employee_id });
                // Said when the server answers (Update::TaskAssigned); a refusal shows its error.
                let who = employee_id.and_then(|m| self.managers.iter().find(|x| x.id == m).map(|x| x.name.clone()));
                self.assign_said.insert(task_id, match who {
                    Some(name) => format!("Task given to {name}."),
                    None => "The task is a draft again.".into(),
                });
            }
            MenuDo::Board(run) => self.board_do(run, window, cx),
            MenuDo::Task(choice) => match choice {
                TaskChoice::Manager(m) => self.pending_task_manager = m,
                TaskChoice::GiveManager(m) => self.give_manager = Some(m),
                TaskChoice::GiveProject(p) => self.give_project = Some(p),
                TaskChoice::FilterEmployee(m) => self.tasks_filter.employee = m,
                TaskChoice::FilterProject(p) => self.tasks_filter.project = p,
                TaskChoice::IdeasProject(p) => self.ideas_project_filter = p,
                TaskChoice::IdeaDialogProject(p) => self.pending_idea_project = Some(p),
                TaskChoice::Mode(m) => self.pending_task_run.mode = m,
                TaskChoice::Model(m) => {
                    self.pending_task_run.connection_id = m.as_ref().map(|(c, _)| *c);
                    self.pending_task_run.model = m.map(|(_, m)| m);
                }
                TaskChoice::Effort(e) => self.pending_task_run.effort = e,
                TaskChoice::ManagerModel(m) => {
                    use aurelius_domain::employee::Setting;
                    let run = &mut self.pending_manager_run;
                    (run.connection, run.model) = match m {
                        Some((c, m)) => (Setting::Value(c), Setting::Value(m)),
                        None => (Setting::Inherit, Setting::Inherit),
                    };
                }
                TaskChoice::ReportsTo(r) => self.pending_reports_to = r,
                TaskChoice::ManagerEffort(e) => self.pending_manager_run.effort = e.map_or(aurelius_domain::employee::Setting::Inherit, aurelius_domain::employee::Setting::Value),
                TaskChoice::DelegationModel(row, m) => self.choose_delegation(row, cx, |run| {
                    use aurelius_domain::employee::Setting;
                    (run.connection, run.model) = match (row, m) {
                        // Subagents run on their parent's connection: only the model.
                        (DelegationRow::Subagents, Some((_, m))) => (Setting::Inherit, Setting::Value(m)),
                        (_, None) => (Setting::Inherit, Setting::Inherit),
                    };
                }),
            },
        }
        cx.notify();
    }

    fn move_menu(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(menu) = self.menu.as_mut() {
            let n = menu.entries.len();
            let mut i = menu.selected;
            for _ in 0..n {
                i = if down { (i + 1) % n } else { (i + n - 1) % n };
                if menu.entries[i].is_some() {
                    break;
                }
            }
            menu.selected = i;
            self.menu_scroll.scroll_to_item(i);
            cx.notify();
        }
    }

    fn on_remove_memory(&mut self, _: &RemoveMemory, window: &mut Window, cx: &mut Context<Self>) {
        use aurelius_domain::memory::MemoryStatus::*;
        if self.on_ideas_screen() {
            match self.selected_idea() {
                Some(i) if i.status != aurelius_domain::idea::IdeaStatus::Dropped => {
                    let _ = self.requests.send(backend::Request::SetIdeaStatus { idea_id: i.id, status: aurelius_domain::idea::IdeaStatus::Dropped });
                    self.notify_user(format!("Dropped “{}” (filter 3 shows dropped ideas).", i.title), false, cx);
                }
                // A dropped idea is deleted for good, after asking.
                Some(_) if self.idea_attachments_on() => self.ask_delete_idea(window, cx),
                _ => {}
            }
            return;
        }
        match self.selected_memory() {
            Some(m) if m.status == Proposed => {
                let _ = self.requests.send(backend::Request::DecideMemory { memory_id: m.id, accept: false });
                self.notify_user("Proposal rejected.", false, cx);
            }
            Some(m) if m.status == Active => {
                self.overlay = Overlay::Confirm(ConfirmKind::ForgetMemory(m.id));
                self.focus.focus(window, cx);
                cx.notify();
            }
            Some(_) => self.notify_user("This item is no longer active.", false, cx),
            None => {}
        }
    }

    /// Merge the project's memory, and that of each of its managers, into
    /// short points now (the server also does this by itself once a memory grows).
    pub fn consolidate_memory(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        for scope in consolidation_scopes(project, self.memory.as_ref().filter(|(p, _)| *p == project).map(|(_, l)| l.as_slice()).unwrap_or_default()) {
            let _ = self.requests.send(backend::Request::ConsolidateMemory(scope));
        }
        self.notify_user("Consolidating memory in the background. Merged memories move to History.", false, cx);
    }

    fn on_toggle_pin(&mut self, _: &TogglePin, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(m) = self.selected_memory() {
            let _ = self.requests.send(backend::Request::PinMemory { memory_id: m.id, pinned: !m.pinned });
            self.notify_user(if m.pinned { "Unpinned." } else { "Pinned: always included as context." }, false, cx);
        }
    }

    /// Conversation whose working mode the mode chip and Shift+Tab act on:
    /// the open manager, or the selected manager card.
    pub fn mode_target(&self) -> Option<(ConversationId, aurelius_domain::autonomy::AgentMode, String)> {
        match self.thread_screen() {
            // Managers with tasks only: the open task's conversation.
            Page::Employee(ManagerRef::Real(m)) => {
                let manager = self.managers.iter().find(|x| x.id == m)?;
                let open = self.messages.as_ref().filter(|s| Some(s.conversation_id) == self.open_conversation)?;
                Some((open.conversation_id, open.agent_mode, manager.name.clone()))
            }
            _ => None,
        }
    }

    fn current_real_project(&self) -> Option<ProjectSummary> {
        match self.current_project() {
            Some(ProjectRef::Real(id)) => self.projects.iter().find(|p| p.id == id).cloned(),
            _ => None,
        }
    }

    fn on_choose_dialog_folder(&mut self, _: &ChooseDialogFolder, window: &mut Window, cx: &mut Context<Self>) {
        if self.task_dialog_project().is_some() || self.idea_files_on() {
            self.pick_task_files(cx);
            return;
        }
        if !matches!(self.overlay, Overlay::Dialog(DialogKind::NewProject | DialogKind::NewManager)) {
            return;
        }
        self.pick_folder(crate::screens::servers::FolderPurpose::Dialog, "Choose folder", window, cx);
    }

    fn on_open_project_folder(&mut self, _: &OpenProjectFolder, _: &mut Window, cx: &mut Context<Self>) {
        let mut folder = None;
        if let Page::Employee(ManagerRef::Real(manager)) = self.screen {
            folder = self.managers.iter().find(|m| m.id == manager).and_then(|m| m.folder.clone());
        }
        let Some(folder) = folder.or_else(|| self.current_real_project().and_then(|p| p.folder)) else {
            self.notify_user("Open one of your own projects first.", false, cx);
            return;
        };
        match self.remote_name().map(str::to_owned) {
            // The folder is on the server, not on this computer.
            Some(host) => {
                cx.write_to_clipboard(ClipboardItem::new_string(folder.clone()));
                self.notify_user(format!("This folder is on {host}: {folder} (path copied)."), false, cx);
            }
            None => cx.open_with_system(std::path::Path::new(&folder)),
        }
    }

    fn on_change_project_folder(&mut self, _: &ChangeProjectFolder, window: &mut Window, cx: &mut Context<Self>) {
        use crate::screens::servers::FolderPurpose;
        if let Page::Employee(ManagerRef::Real(manager)) = self.screen {
            self.pick_folder(FolderPurpose::ManagerFolder(manager), "Use as this employee's workspace", window, cx);
            return;
        }
        let Some(project) = self.current_real_project() else {
            self.notify_user("Open one of your own projects first.", false, cx);
            return;
        };
        self.pick_folder(FolderPurpose::ProjectFolder(project.id), "Use as project folder", window, cx);
    }

    /// Re-render once a second while something runs, so elapsed times stay current.
    fn ensure_ticking(&mut self, cx: &mut Context<Self>) {
        if self.ticking {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let alive = this.update(cx, |this, cx| {
                let busy = this.running_jobs().next().is_some() || !this.live_progress.is_empty();
                if busy {
                    this.quiet_notify = true;
                    cx.notify();
                } else {
                    this.ticking = false;
                }
                busy
            });
            // Observers ran when that update ended.
            let _ = this.update(cx, |this, _| this.quiet_notify = false);
            if !matches!(alive, Ok(true)) {
                break;
            }
        })
        .detach();
    }

    fn on_open_usage(&mut self, _: &OpenUsage, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Overlay::Usage;
        self.usage_day = None;
        let _ = self.requests.send(backend::Request::LoadUsage);
        self.load_usage_hours(true);
        let _ = self.requests.send(backend::Request::LoadSubscription(true));
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// The day the Usage dialog shows per hour: the picked one while it is
    /// still among the 14 days, else today (the last one).
    pub fn usage_selected_day(&self) -> Option<i64> {
        let days = &self.usage.as_ref()?.by_day;
        self.usage_day.filter(|d| days.iter().any(|x| x.day_start_ms == *d)).or_else(|| days.last().map(|d| d.day_start_ms))
    }

    pub fn select_usage_day(&mut self, day: i64, cx: &mut Context<Self>) {
        self.usage_day = Some(day);
        self.load_usage_hours(false);
        cx.notify();
    }

    /// Ask for the selected day's hours; `force` also when they are already here.
    fn load_usage_hours(&mut self, force: bool) {
        let Some(day) = self.usage_selected_day() else { return };
        let have = self.usage_hours.as_ref().is_some_and(|(d, _)| *d == day);
        if (force || !have) && self.usage_hours_requested != Some(day) {
            self.usage_hours_requested = Some(day);
            let _ = self.requests.send(backend::Request::LoadUsageHours(day));
        }
    }

    fn on_cycle_agent_mode(&mut self, _: &CycleAgentMode, _: &mut Window, cx: &mut Context<Self>) {
        match self.mode_target() {
            Some((conversation_id, mode, _)) => {
                let next = mode.next();
                // Optimistic: show the new mode at once; the daemon confirms.
                if let Some(m) = self.messages.as_mut().filter(|m| m.conversation_id == conversation_id) {
                    m.agent_mode = next;
                }
                if let Some(m) = self.managers.iter_mut().find(|m| m.conversation_id == conversation_id) {
                    m.agent_mode = next;
                }
                let _ = self.requests.send(backend::Request::SetAgentMode { conversation_id, mode: next });
                cx.notify();
            }
            // No task open on the employee's page: its own mode (the default for its next work).
            None => match self.settings_employee().map(|m| m.id) {
                Some(id) => self.cycle_employee_mode(id, cx),
                None => self.notify_user("Modes belong to employees: open an employee (or select its card) to switch its mode.", false, cx),
            },
        }
    }

    /// The employee's own mode, one step on (the mode chip on its page).
    pub fn cycle_employee_mode(&mut self, employee: EmployeeId, cx: &mut Context<Self>) {
        let next = self.managers.iter().find(|m| m.id == employee).map(|m| m.agent_mode.next());
        self.remember_mode(Some(employee), next);
        cx.notify();
    }

    /// The role of the employee (not archived) by this name, from the company.
    pub fn employee_role(&self, name: &str) -> Option<&str> {
        self.managers.iter().find(|m| m.status != LifecycleStatus::Archived && m.name == name).map(|m| m.role.as_str())
    }

    /// Who wrote an answer: in the group chat an employee's name with their role ("Orion — Project Manager").
    pub fn author_label(&self, m: &aurelius_protocol::MessageView, fallback: &SharedString) -> SharedString {
        let name = crate::screens::chat::message_author(m, fallback, self.active_member());
        if self.screen != Page::GroupChat || m.role == aurelius_domain::conversation::MessageRole::User {
            return name;
        }
        crate::screens::chat::with_role(&name, self.employee_role(&name)).into()
    }

    /// Who answers a group chat message without an `@name`: the top of the org chart.
    pub fn group_answerer(&self) -> Option<&str> {
        let top = crate::screens::dashboard::roots(&self.managers);
        top.first().and_then(|id| self.managers.iter().find(|m| m.id == *id)).map(|m| m.name.as_str())
    }

    /// The employee a run works for (its answer streams into the group chat under its name).
    pub fn run_author(&self, run: aurelius_domain::RunId) -> Option<String> {
        self.managers.iter().find(|m| m.agents.iter().any(|a| a.run_id == Some(run))).map(|m| m.name.clone())
    }

    /// The latest answer streaming into the open group chat (what Stop stops there).
    pub fn group_run(&self) -> Option<aurelius_domain::RunId> {
        if self.open_conversation.is_none() || self.open_conversation != self.group_conversation {
            return None;
        }
        self.group_live.last().map(|l| l.run)
    }

    /// Whether `conversation` is an employee's main chat: no longer shown on its own; its
    /// runs answer in the group chat, so what they ask is decided there.
    pub fn is_main_chat(&self, conversation: ConversationId) -> bool {
        self.managers.iter().any(|m| m.conversation_id == conversation)
    }

    /// The mode chosen when giving work becomes the employee's mode: the default for its next
    /// tasks and conversations, and the one the Give work chip shows. Nothing picked: no change.
    pub fn remember_mode(&mut self, employee: Option<EmployeeId>, mode: Option<aurelius_domain::autonomy::AgentMode>) {
        let (Some(employee), Some(mode)) = (employee, mode) else { return };
        let Some(m) = self.managers.iter_mut().find(|m| m.id == employee) else { return };
        if m.agent_mode == mode {
            return;
        }
        m.agent_mode = mode;
        let _ = self.requests.send(backend::Request::SetAgentMode { conversation_id: m.conversation_id, mode });
    }

    /// Heartbeat of the open (or selected) manager: off → 30 min → 1 h → 4 h → off.
    fn on_cycle_heartbeat(&mut self, _: &CycleHeartbeat, _: &mut Window, cx: &mut Context<Self>) {
        let target = match self.screen {
            Page::Employee(ManagerRef::Real(m)) => Some(m),
            _ => None,
        };
        let Some(manager) = target.and_then(|id| self.managers.iter_mut().find(|m| m.id == id)) else {
            self.notify_user("Open one of your employees to set a heartbeat.", false, cx);
            return;
        };
        const CHOICES: [u32; 4] = [0, 30, 60, 240];
        let current = manager.heartbeat.as_ref().map_or(0, |h| h.every_minutes);
        let next = CHOICES[(CHOICES.iter().position(|c| *c == current).unwrap_or(0) + 1) % CHOICES.len()];
        manager.heartbeat = (next > 0).then(|| aurelius_protocol::HeartbeatView {
            every_minutes: next,
            quiet_start_min: 22 * 60,
            quiet_end_min: 7 * 60,
            daily_token_budget: 20_000,
            tokens_today: manager.heartbeat.as_ref().map_or(0, |h| h.tokens_today),
            next_due_ms: None,
            waiting: None,
        });
        let (id, name) = (manager.id, manager.name.clone());
        let _ = self.requests.send(backend::Request::SetHeartbeat { employee_id: id, every_minutes: next });
        let message = if next == 0 {
            format!("Heartbeat for {name} is off.")
        } else {
            format!("{name} checks in {}. Only when something changed; quiet 22:00–07:00; at most 20k tokens a day. {}", heartbeat_label(next), crate::screens::account::OWN_SUBSCRIPTION)
        };
        self.notify_user(message, false, cx);
    }

    fn on_show_context(&mut self, _: &ShowContext, _: &mut Window, cx: &mut Context<Self>) {
        let run = self.messages.as_ref().and_then(|m| {
            m.messages.iter().rev().find(|m| m.role == MessageRole::Assistant && m.run_id.is_some()).and_then(|m| m.run_id)
        });
        match run {
            Some(run_id) => {
                let _ = self.requests.send(backend::Request::LoadContext(run_id));
            }
            None => self.notify_user("Open a conversation with an answer to see the context it used.", false, cx),
        }
    }

    /// Inbox entries: pending approvals first, then notifications.
    pub fn inbox_notifications(&self) -> Vec<&NotificationView> {
        self.notifications.as_ref().map(|n| n.notifications.iter().collect()).unwrap_or_default()
    }

    /// Items per inbox tab, in `InboxTab::ALL` order.
    pub fn inbox_counts(&self) -> [usize; 4] {
        tab_counts(self.pending_approvals().count() + self.pending_proposals().len() + self.pending_guideline_proposals().len(), &self.inbox_notifications())
    }

    /// The notifications the open tab lists, after its approvals and proposals.
    pub fn inbox_notes(&self) -> Vec<&NotificationView> {
        tab_notes(self.inbox_tab, &self.inbox_notifications())
    }

    pub fn inbox_len(&self) -> usize {
        self.inbox_head() + self.inbox_notes().len()
    }

    /// The selected inbox item, when it is a pending approval.
    pub fn inbox_approval(&self) -> Option<&ApprovalView> {
        self.inbox_tab.shows_head().then(|| self.pending_approvals().nth(self.inbox_selected)).flatten()
    }

    pub fn inbox_proposal(&self) -> Option<&aurelius_protocol::ProposalView> {
        let i = self.inbox_selected.checked_sub(self.pending_approvals().count()).filter(|_| self.inbox_tab.shows_head())?;
        self.pending_proposals().get(i).copied()
    }

    pub fn inbox_note(&self) -> Option<&NotificationView> {
        self.inbox_notes().get(self.inbox_selected.checked_sub(self.inbox_head())?).copied()
    }

    // ── Navigation ──────────────────────────────────────────────────────

    pub fn navigate(&mut self, screen: Page) {
        // Going elsewhere leaves Switch / New board member (the gate itself stays).
        self.members.stack.clear();
        if self.screen != screen {
            // A new page starts at the top.
            self.scroll.set_offset(point(px(0.), px(0.)));
            self.page_scroll.set_offset(point(px(0.), px(0.)));
            self.back_stack.push(self.screen);
            self.screen = screen;
            self.selected = 0;
            self.at_work_selected = None;
            self.preview_closed = false;
        }
    }

    /// Pin the project under Projects in the sidebar (at most `MAX_PINS`), or take it out again.
    pub fn toggle_pin(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let known: Vec<ProjectId> = self.projects.iter().map(|p| p.id).collect();
        self.toggle_pin_in(|this| &mut this.pins, &known, project, "projects", cx);
    }

    /// Pin the employee under Employees in the sidebar (at most `MAX_PINS`), or take it out again.
    pub fn toggle_employee_pin(&mut self, employee: EmployeeId, cx: &mut Context<Self>) {
        let known: Vec<EmployeeId> = self.managers.iter().map(|m| m.id).collect();
        self.toggle_pin_in(|this| &mut this.employee_pins, &known, employee, "employees", cx);
    }

    /// Shared by both pin lists: prune what is gone, refuse a sixth, toggle and save per server.
    fn toggle_pin_in<T: Pinnable>(&mut self, list: fn(&mut Self) -> &mut Vec<T>, known: &[T], id: T, noun: &str, cx: &mut Context<Self>) {
        // Items that are gone fall out; before the list has loaded, nothing is pruned.
        if !known.is_empty() {
            list(self).retain(|p| known.contains(p));
        }
        if !can_pin(list(self), id) {
            self.notify_user(format!("Only {MAX_PINS} {noun} can be pinned. Unpin one first."), true, cx);
            return;
        }
        pins_toggle(list(self), id);
        let pins = list(self).clone();
        let mut prefs = ui_prefs();
        pins_save(&mut prefs, &self.pin_server, &pins);
        set_ui_value(T::KEY, prefs[T::KEY].clone());
        cx.notify();
    }

    /// Fold or unfold the pinned rows under Employees or Projects; remembered in ui.json.
    pub fn toggle_folded(&mut self, nav: NavItem, cx: &mut Context<Self>) {
        match self.folded.iter().position(|n| *n == nav) {
            Some(i) => {
                self.folded.remove(i);
            }
            None => self.folded.push(nav),
        }
        set_ui_value("sidebar_folded", serde_json::json!(self.folded.iter().map(|n| n.fold_key()).collect::<Vec<_>>()));
        cx.notify();
    }

    /// The pinned employees that still exist, in pin order.
    pub(crate) fn pinned_employees(&self) -> Vec<&EmployeeSummary> {
        self.employee_pins.iter().filter_map(|id| self.managers.iter().find(|m| m.id == *id)).collect()
    }

    /// The pinned projects that still exist, in pin order.
    /// The task's mark, "Context comprimeren…" while its run compacts the context.
    pub(crate) fn task_mark(&self, t: &aurelius_protocol::TaskView) -> crate::ui::TaskMark {
        let compacting = t.conversation_id.and_then(|c| self.streams.get(&c)).and_then(|s| s.progress).is_some_and(|p| p.compacting);
        crate::ui::TaskMark::of_live(t, compacting)
    }

    pub(crate) fn pinned_projects(&self) -> Vec<&ProjectSummary> {
        self.pins.iter().filter_map(|id| self.projects.iter().find(|p| p.id == *id)).collect()
    }

    /// Live state of a project for the top bar: needs you > working > quiet.
    pub(crate) fn project_state(&self, project: ProjectId) -> (&'static str, u32) {
        let waiting = self.tasks.iter().any(|t| t.project_id == project && (t.pending_approvals > 0 || t.status == aurelius_domain::task::TaskStatus::Waiting));
        if waiting {
            return ("Needs you", theme::ACCENT_TEXT);
        }
        if self.tasks.iter().any(|t| t.project_id == project && (t.status == aurelius_domain::task::TaskStatus::Running || (t.status.is_terminal() && t.live_run == Some(aurelius_domain::views::LiveRun::Running)))) {
            return ("Working", theme::ACCENT);
        }
        ("Quiet", theme::MUTED)
    }

    /// Makes this the window of one project: it opens there and stays there.
    pub fn hold_to_project(&mut self, project: ProjectId, window: &mut Window, cx: &mut Context<Self>) {
        self.window_project = Some(project);
        self.screen = Page::Project(ProjectRef::Real(project), Tab::Overview);
        self._subscriptions.push(cx.observe_window_bounds(window, move |this, w, cx| crate::project_window::save_bounds(this, project, w, cx)));
    }

    /// Opens the project in a window of its own (or brings that window to the front).
    pub fn open_project_window(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let name = self.projects.iter().find(|p| p.id == project).map_or_else(|| "Project".to_string(), |p| p.name.clone());
        // Deferred so `open` can read the main window's display (it is borrowed during this update).
        cx.defer(move |cx| crate::project_window::open(project, &name, cx));
    }

    pub fn open_project(&mut self, project: ProjectRef, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Page::Project(project, Tab::Overview));
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn open_manager(&mut self, manager: ManagerRef, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Page::Employee(manager));
        // The employee page shows its own conversation.
        self.panel = Panel::None;
        let name = self.manager_card(manager).map_or_else(|| SharedString::from("manager"), |m| m.name);
        self.manager_thread = None;
        self.open_thread();
        self.composer.update(cx, |input, cx| input.set_placeholder(format!("Message {name}…"), window, cx));
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn set_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        let project = match self.screen {
            Page::Project(p, _) => p,
            Page::Employee(m) => match m.sample_project() {
                Some(p) => p,
                None => return,
            },
            Page::Projects => match self.home_projects().get(self.selected) {
                Some(p) => *p,
                None => return,
            },
            _ => return,
        };
        if let Page::Project(_, current) = self.screen {
            if current == tab {
                return;
            }
            self.screen = Page::Project(project, tab);
        } else {
            self.navigate(Page::Project(project, tab));
        }
        self.selected = 0;
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn step_tab(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let tab = match self.screen {
            Page::Project(_, tab) => Some(tab),
            _ => None,
        };
        if let Some(tab) = tab {
            let next = (tab.index() as isize + delta).rem_euclid(Tab::ALL.len() as isize) as usize;
            self.set_tab(Tab::ALL[next], window, cx);
        } else if self.screen == Page::Employees {
            let at = EmployeesTab::ALL.iter().position(|t| *t == self.employees_tab).unwrap_or(0) as isize;
            self.set_employees_tab(EmployeesTab::ALL[(at + delta).rem_euclid(EmployeesTab::ALL.len() as isize) as usize], cx);
        } else if self.screen == Page::Projects {
            let at = ProjectsTab::ALL.iter().position(|t| *t == self.projects_tab).unwrap_or(0) as isize;
            self.set_projects_tab(ProjectsTab::ALL[(at + delta).rem_euclid(ProjectsTab::ALL.len() as isize) as usize], cx);
        } else if self.screen == Page::Inbox {
            let at = InboxTab::ALL.iter().position(|t| *t == self.inbox_tab).unwrap_or(0) as isize;
            self.set_inbox_tab(InboxTab::ALL[(at + delta).rem_euclid(InboxTab::ALL.len() as isize) as usize], cx);
        }
    }

    /// A tab of the Employees page (list, org chart).
    pub fn set_employees_tab(&mut self, tab: EmployeesTab, cx: &mut Context<Self>) {
        self.employees_tab = tab;
        self.selected = 0;
        cx.notify();
    }

    /// A tab of the Projects page (projects, ideas of every project).
    pub fn set_projects_tab(&mut self, tab: ProjectsTab, cx: &mut Context<Self>) {
        self.projects_tab = tab;
        self.selected = 0;
        if tab == ProjectsTab::Ideas {
            // Company-wide: the ideas of every project (each still belongs to its own project).
            self.ideas_project_filter = None;
        }
        cx.notify();
    }

    pub fn set_inbox_tab(&mut self, tab: InboxTab, cx: &mut Context<Self>) {
        self.inbox_tab = tab;
        self.inbox_selected = 0;
        self.question_focus = 0;
        cx.notify();
    }

    // ── Action handlers ─────────────────────────────────────────────────

    fn on_open_palette(&mut self, _: &OpenPalette, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Overlay::Palette;
        self.palette_selected = 0;
        self.palette_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// A sidebar entry (or its Ctrl+digit): its page, through normal navigation (Back works).
    /// Inbox and Settings open over the page. Employees and Projects keep the tab they were on.
    pub fn go_nav(&mut self, item: NavItem, window: &mut Window, cx: &mut Context<Self>) {
        let page = match item {
            NavItem::Inbox => return self.open_inbox(&OpenInbox, window, cx),
            NavItem::Settings => return self.open_settings(&OpenSettings, window, cx),
            NavItem::Dashboard => Page::Dashboard,
            NavItem::Board => Page::Board,
            NavItem::Chat => Page::GroupChat,
            NavItem::Employees => Page::Employees,
            NavItem::Projects => Page::Projects,
            NavItem::Tasks => Page::Tasks,
            NavItem::Vault => Page::Vault,
        };
        self.close_overlay();
        self.navigate(page);
        self.selected = 0;
        self.open_thread();
        if page == Page::GroupChat {
            // The group chat is for writing: straight into its composer.
            self.composer.update(cx, |input, cx| input.focus(window, cx));
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    /// Ask for the group chat again after it failed to load.
    pub fn retry_group_chat(&mut self, cx: &mut Context<Self>) {
        self.group_error = None;
        let _ = self.requests.send(backend::Request::OpenGroupChat(self.channel));
        cx.notify();
    }

    /// Load the conversation the page shows: the open employee's task, the right panel's,
    /// or the group chat (asked for by name until its id is known).
    fn open_thread(&mut self) {
        if self.screen == Page::GroupChat {
            self.pending_placeholder = Some(self.channel_placeholder());
            self.channels_followed = true;
            let _ = self.requests.send(backend::Request::FollowChannels(true));
            if self.group_conversation.is_none() {
                self.open(None);
                let _ = self.requests.send(backend::Request::OpenGroupChat(self.channel));
                return;
            }
        }
        if self.on_thread_screen() {
            let conversation = self.current_manager_thread();
            // The task it opens on stays open while other tasks change.
            if matches!(self.screen, Page::Employee(_)) && !self.panel_shown() {
                self.manager_thread = conversation;
            }
            self.open(conversation);
        }
    }

    /// Go Org chart: the Employees page on that tab.
    pub fn go_employees_tab(&mut self, tab: EmployeesTab, window: &mut Window, cx: &mut Context<Self>) {
        self.employees_tab = tab;
        self.go_nav(NavItem::Employees, window, cx);
    }

    /// Go Ideas: the Projects page on its Ideas tab.
    pub fn go_projects_tab(&mut self, tab: ProjectsTab, window: &mut Window, cx: &mut Context<Self>) {
        self.set_projects_tab(tab, cx);
        self.go_nav(NavItem::Projects, window, cx);
    }

    /// Opening a task reads its result notification: a result is not news that waits for you. Only
    /// "Needs you" and "Failed" stay unread until you handle them, and give the badge.
    pub fn read_task_results(&mut self, task: aurelius_domain::TaskId) {
        let (changed, others_unread) = {
            let Some(snapshot) = self.notifications.as_mut() else { return };
            let mut changed = false;
            for n in snapshot.notifications.iter_mut().filter(|n| !n.read && n.task_id == Some(task) && note_kind(n) == NoteKind::Result) {
                n.read = true;
                changed = true;
            }
            snapshot.unread = snapshot.notifications.iter().filter(|n| !n.read).count() as _;
            (changed, snapshot.notifications.iter().any(|n| !n.read && note_kind(n) != NoteKind::Result))
        };
        // The server can only read everything at once: do that only when nothing else is unread.
        if changed && !others_unread {
            let _ = self.requests.send(backend::Request::MarkNotificationsRead);
        }
    }

    /// Open a task or conversation in the right panel; the page stays.
    pub fn open_panel(&mut self, panel: Panel, window: &mut Window, cx: &mut Context<Self>) {
        // "‹ #42" only for a task opened from its ticket (`open_ticket_task` sets it again).
        self.board.back_to = None;
        if let Panel::Task(task) = panel {
            self.read_task_results(task);
        }
        self.panel = panel;
        self.manager_thread = None;
        let conversation = self.current_manager_thread();
        self.open(conversation);
        let name = self.panel_manager().map(|m| m.name.clone());
        let placeholder = name.map_or_else(|| "Reply…".to_string(), |n| format!("Message {n}…"));
        self.composer.update(cx, |input, cx| input.set_placeholder(placeholder, window, cx));
        cx.notify();
    }

    pub fn close_panel(&mut self, cx: &mut Context<Self>) {
        if self.panel == Panel::None {
            return;
        }
        // shortcut: live-test B6 (panel closing by itself) has no known cause yet; RUST_LOG=info shows who closed it. Remove once found.
        tracing::info!(backtrace = %std::backtrace::Backtrace::force_capture(), "right panel closed");
        self.panel = Panel::None;
        let conversation = self.current_manager_thread();
        self.open(conversation);
        cx.notify();
    }

    /// The task in the right panel, when it shows one.
    pub fn panel_task(&self) -> Option<&TaskView> {
        let Panel::Task(id) = self.panel else { return None };
        self.tasks.iter().find(|t| t.id == id)
    }

    /// The employee whose conversation the right panel shows.
    pub fn panel_manager(&self) -> Option<&EmployeeSummary> {
        let id = match self.panel {
            Panel::Task(_) => self.panel_task()?.employee_id?,
            Panel::Chat(c) => self.managers.iter().find(|m| m.conversation_id == c || m.conversations.iter().any(|t| t.id == c))?.id,
            Panel::None | Panel::Ticket(_) => return None,
        };
        self.managers.iter().find(|m| m.id == id)
    }

    /// The page as the open conversation sees it: the right panel's employee when the panel
    /// shows a task or chat, else the page itself (composer, files, modes, runtime line).
    pub fn thread_screen(&self) -> Page {
        match self.panel_manager().filter(|_| self.panel_shown()) {
            Some(m) => Page::Employee(ManagerRef::Real(m.id)),
            None => self.screen,
        }
    }

    /// The right panel shows beside this page: not beside a page with a conversation of its
    /// own (an employee, the group chat).
    pub fn panel_shown(&self) -> bool {
        self.panel != Panel::None && !matches!(self.screen, Page::Employee(..) | Page::GroupChat)
    }

    /// The conversation shown: the group chat on its page, else the right panel's, else the
    /// open employee's task conversation (the chosen one, else its latest; none without tasks).
    /// An employee's own main chat is its hidden workspace: the user talks with it in the group chat.
    pub fn current_manager_thread(&self) -> Option<ConversationId> {
        if self.screen == Page::GroupChat {
            return self.group_conversation;
        }
        // The right panel owns the open conversation while it shows one.
        match self.panel {
            Panel::Chat(id) if self.panel_shown() => return Some(id),
            Panel::Task(_) if self.panel_shown() => return self.panel_task().and_then(|t| t.conversation_id),
            _ => {}
        }
        let Page::Employee(manager) = self.screen else { return None };
        let m = self.real_manager(manager)?;
        let own = || self.tasks.iter().filter(|t| t.employee_id == Some(m.id)).filter_map(|t| t.conversation_id.map(|c| (c, t.updated_at_ms)));
        self.manager_thread
            .filter(|chosen| own().any(|(c, _)| c == *chosen))
            .or_else(|| own().max_by_key(|(_, at)| *at).map(|(c, _)| c))
    }

    /// The conversation the subagent chip belongs to: the open manager conversation.
    pub fn chip_conversation(&self) -> Option<ConversationId> {
        self.current_manager_thread()
    }

    /// Subagents running in the open conversation's own answers, oldest first.
    pub fn subagents_here(&self) -> Vec<aurelius_protocol::AgentView> {
        let Some(current) = self.chip_conversation() else { return Vec::new() };
        subagents_of(current, self.subagents.as_ref())
    }

    /// The subagent chip opens or closes its panel; while open, the times tick.
    pub fn toggle_subagents(&mut self, cx: &mut Context<Self>) {
        // A click on the chip first closes the open panel (a click outside it): keep it closed.
        if self.subagents_closed_at.is_some_and(|at| at.elapsed() < Duration::from_millis(300)) {
            return;
        }
        self.subagents_open = !self.subagents_open;
        self.subagent_expanded = None;
        if self.subagents_open {
            cx.spawn(async move |this, cx| loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let open = this.update(cx, |this, cx| {
                    cx.notify();
                    this.subagents_open
                });
                if !matches!(open, Ok(true)) {
                    break;
                }
            })
            .detach();
        }
        cx.notify();
    }

    /// A click outside the subagent panel closes it.
    pub fn close_subagents(&mut self, cx: &mut Context<Self>) {
        if self.subagents_open {
            self.subagents_open = false;
            self.subagents_closed_at = Some(std::time::Instant::now());
            cx.notify();
        }
    }

    pub fn select_manager_thread(&mut self, conversation: ConversationId, cx: &mut Context<Self>) {
        self.manager_thread = Some(conversation);
        self.open(Some(conversation));
        cx.notify();
    }

    /// Delete the open task conversation of an employee.
    pub fn ask_delete_thread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.screen, Page::Employee(_)) {
            return;
        }
        match self.current_manager_thread() {
            Some(t) => {
                self.overlay = Overlay::Confirm(ConfirmKind::DeleteConversation(t));
                self.focus.focus(window, cx);
                cx.notify();
            }
            None => self.notify_user("Open one of the employee's tasks first.", false, cx),
        }
    }

    fn on_new_item(&mut self, _: &NewItem, window: &mut Window, cx: &mut Context<Self>) {
        match self.screen {
            // Work for the open employee: the Dashboard's Give work box, the employee chosen.
            Page::Employee(ManagerRef::Real(_)) => self.on_new_task(&NewTask, window, cx),
            Page::Project(ProjectRef::Real(_), Tab::Tasks) => self.on_new_task(&NewTask, window, cx),
            Page::Project(ProjectRef::Real(_), Tab::Board) => self.board_new_ticket(window, cx),
            Page::Project(ProjectRef::Real(project), Tab::Memory) => self.open_dialog(DialogKind::NewMemory { project }, window, cx),
            Page::Project(ProjectRef::Real(project), Tab::Ideas) => self.open_dialog(DialogKind::NewIdea { project }, window, cx),
            Page::Projects if self.projects_tab == ProjectsTab::Ideas => {
                // Ideas belong to a project: the filtered one, else the last one used, else the first.
                let known = |p: &ProjectId| self.projects.iter().any(|x| x.id == *p);
                let project = self.ideas_project_filter.filter(known).or(self.ideas_project.filter(known)).or_else(|| self.projects.first().map(|p| p.id));
                match project {
                    Some(project) => self.open_dialog(DialogKind::NewIdea { project }, window, cx),
                    None => self.notify_user("Make a project first: ideas belong to a project.", false, cx),
                }
            }
            Page::Projects => self.open_dialog(DialogKind::NewProject, window, cx),
            Page::Employees => self.on_add_employee(&AddEmployee, window, cx),
            // On the board Ctrl+N is New ticket (§2).
            Page::Board => self.board_new_ticket(window, cx),
            // Anywhere else Ctrl+N gives work: the Dashboard's box, in the current project when there is one.
            _ => {
                if let Some(ProjectRef::Real(p)) = self.current_project() {
                    self.give_project = Some(p);
                }
                self.navigate(Page::Dashboard);
                self.give_goal.update(cx, |i, cx| i.focus(window, cx));
                cx.notify();
            }
        }
    }

    fn on_new_project(&mut self, _: &NewProject, window: &mut Window, cx: &mut Context<Self>) {
        self.open_dialog(DialogKind::NewProject, window, cx);
    }

    fn on_edit(&mut self, _: &EditItem, window: &mut Window, cx: &mut Context<Self>) {
        if self.on_ideas_screen() {
            match self.selected_idea() {
                Some(i) => {
                    self.open_dialog(DialogKind::EditIdea { id: i.id }, window, cx);
                    self.name_input.update(cx, |input, cx| input.set_value(i.title.clone(), window, cx));
                    self.goal_input.update(cx, |input, cx| input.set_value(i.note.clone().unwrap_or_default(), window, cx));
                    self.pending_schedule_files = i.attachments.clone();
                }
                None => self.notify_user("Select an idea to edit.", false, cx),
            }
            return;
        }
        if let Page::Project(ProjectRef::Real(_), Tab::Memory) = self.screen {
            match self.selected_memory() {
                Some(m) if m.status == aurelius_domain::memory::MemoryStatus::Active => {
                    self.open_dialog(DialogKind::ReviseMemory { id: m.id }, window, cx);
                    self.name_input.update(cx, |input, cx| input.set_value(m.content.clone(), window, cx));
                }
                _ => self.notify_user("Select an active memory to revise.", false, cx),
            }
            return;
        }
        if let Page::Employee(manager) = self.screen {
            match self.real_manager(manager).cloned() {
                Some(m) => {
                    self.open_dialog(DialogKind::EditManager { id: m.id, version: m.version }, window, cx);
                    // Its instruction versions are asked again each time it opens.
                    self.instruction_versions_requested = None;
                    self.pending_manager_run = self.manager_defaults(m.id);
                    self.pending_manager_delegation = DelegationRow::ALL.map(|row| self.run_layer(aurelius_domain::connection::ConfigScope::Employee(m.id), row.purpose()).unwrap_or_default());
                    self.pending_manager_sequential = m.sequential_tasks;
                    self.pending_reports_to = m.reports_to;
                    let title = m.title.clone().unwrap_or_default();
                    self.title_input.update(cx, |input, cx| input.set_value(title, window, cx));
                    self.plugins_for_manager(m.id);
                    self.name_input.update(cx, |input, cx| input.set_value(m.name.clone(), window, cx));
                    self.role_input.update(cx, |input, cx| input.set_value(m.role.clone(), window, cx));
                    let instructions = m.instructions.clone().unwrap_or_default();
                    self.instructions_input.update(cx, |input, cx| input.set_value(instructions, window, cx));
                }
                None => self.notify_user("Sample employees can't be edited.", true, cx),
            }
            return;
        }
        self.on_rename(window, cx);
    }

    /// Instruction proposal: edit the proposed text, then approve that.
    pub fn edit_proposal(&mut self, proposal: aurelius_domain::ProposalId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.proposals.iter().find(|p| p.id == proposal).map(|p| p.instructions.clone()) else { return };
        self.open_dialog(DialogKind::EditProposal { proposal }, window, cx);
        self.goal_input.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.focus(window, cx);
        });
    }

    fn current_project(&self) -> Option<ProjectRef> {
        match self.screen {
            Page::Project(p, _) => Some(p),
            Page::Employee(m) => m.sample_project(),
            Page::Projects => self.home_projects().get(self.selected).copied(),
            _ => None,
        }
    }

    fn on_add_employee(&mut self, _: &AddEmployee, window: &mut Window, cx: &mut Context<Self>) {
        self.open_dialog(DialogKind::NewManager, window, cx);
    }

    fn on_toggle_pause(&mut self, _: &TogglePause, _: &mut Window, cx: &mut Context<Self>) {
        if let Page::Employee(manager) = self.screen {
            if let Some(m) = self.real_manager(manager) {
                let status = if m.status == LifecycleStatus::Paused { LifecycleStatus::Active } else { LifecycleStatus::Paused };
                let _ = self.requests.send(backend::Request::SetEmployeeStatus { employee_id: m.id, status });
                let text = if status == LifecycleStatus::Paused { "Employee paused: no new work starts." } else { "Employee resumed." };
                self.notify_user(text, false, cx);
                return;
            }
        }
        match self.current_project().and_then(|p| self.real(p)).map(|p| (p.id, p.status)) {
            Some((project_id, status)) => {
                let next = if status == aurelius_domain::ProjectStatus::Paused { LifecycleStatus::Active } else { LifecycleStatus::Paused };
                let _ = self.requests.send(backend::Request::SetProjectStatus { project_id, status: next });
                self.notify_user(if next == LifecycleStatus::Paused { "Project paused." } else { "Project resumed." }, false, cx);
            }
            None => self.notify_user("Select one of your own projects or employees to pause.", true, cx),
        }
    }

    fn on_archive(&mut self, _: &ArchiveItem, window: &mut Window, cx: &mut Context<Self>) {
        let target = match self.screen {
            Page::Employee(manager) => self.real_manager(manager).map(|m| ConfirmKind::ArchiveManager(m.id)),
            _ => self.current_project().and_then(|p| self.real(p)).map(|p| ConfirmKind::ArchiveProject(p.id)),
        };
        match target {
            Some(kind) => {
                self.overlay = Overlay::Confirm(kind);
                self.focus.focus(window, cx);
                cx.notify();
            }
            None => self.notify_user("Nothing to archive; sample data is read-only.", true, cx),
        }
    }

    fn on_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = match self.screen {
            Page::Project(p, _) => Some(p),
            Page::Employee(m) => m.sample_project(),
            Page::Projects => self.home_projects().get(self.selected).copied(),
            _ => None,
        };
        // The name is edited in Project settings; there is no separate Rename dialog.
        match target.and_then(|p| self.real(p)).map(|p| p.id) {
            Some(id) => self.open_project_settings(id, window, cx),
            None => self.notify_user("Select one of your own projects first; sample projects can't be renamed.", true, cx),
        }
    }

    pub(crate) fn open_dialog(&mut self, kind: DialogKind, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Overlay::Dialog(kind);
        self.dialog_folder = None;
        let (name, second, third) = match kind {
            DialogKind::NewTask { .. } | DialogKind::TaskSchedule { .. } => ("Title, for example 'Compare three options'", "", "Acceptance criteria, separated by ;"),
            DialogKind::NewMemory { .. } | DialogKind::ReviseMemory { .. } => ("For example: publishing requires a prior review", "", ""),
            DialogKind::NewIdea { .. } | DialogKind::EditIdea { .. } => ("The idea, for example 'Dark mode'", "", ""),
            DialogKind::QuestionAnswer { .. } => ("Your answer, in your own words", "", ""),
            DialogKind::NewManager | DialogKind::EditManager { .. } => ("Name, for example Alex", "Short description (optional)", "Free-form role, for example Product partner"),
            _ => ("Name, for example Atlas", "Short description (optional)", "Free-form role, for example Product partner"),
        };
        self.name_input.update(cx, |i, cx| i.set_placeholder(name, window, cx));
        self.description_input.update(cx, |i, cx| i.set_placeholder(second, window, cx));
        self.role_input.update(cx, |i, cx| i.set_placeholder(third, window, cx));
        self.name_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.description_input.update(cx, |input, cx| input.set_value("", window, cx));
        self.role_input.update(cx, |input, cx| input.set_value("", window, cx));
        let goal = if matches!(kind, DialogKind::NewIdea { .. } | DialogKind::EditIdea { .. }) { "Why, what it would take (optional)" } else { "Goal: what needs to be achieved?" };
        self.goal_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder(goal, window, cx);
        });
        self.recur_hours_input.update(cx, |input, cx| input.set_value("6", window, cx));
        self.recur_time_input.update(cx, |input, cx| input.set_value("09:00", window, cx));
        self.pending_task_files.clear();
        self.pending_recurrence = 1;
        self.pending_weekdays = [true, true, true, true, true, false, false];
        self.pending_schedule_paused = false;
        self.pending_memory_decision = false;
        self.task_more_open = false;
        self.employee_advanced_open = false;
        self.pending_idea_project = match kind {
            DialogKind::NewIdea { project } => Some(project),
            _ => None,
        };
        self.pending_idea_kind = match kind {
            DialogKind::EditIdea { id } => self.ideas.iter().flat_map(|(_, l)| l).find(|i| i.id == id).map_or(Default::default(), |i| i.kind),
            _ => Default::default(),
        };
        self.pending_schedule_files.clear();
        self.pending_schedule_remove.clear();
        self.pending_reports_to = None;
        self.title_input.update(cx, |input, cx| input.set_value("", window, cx));
        self.instructions_input.update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    pub fn confirm_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Overlay::Dialog(kind) = self.overlay else { return };
        let name = self.name_input.read(cx).value().trim().to_string();
        if let DialogKind::QuestionAnswer { approval, index, in_inbox } = kind {
            // Your own words replace the options; back to the inbox or the conversation to send.
            if name.is_empty() {
                self.question_own.remove(&(approval, index));
            } else {
                self.question_choices.remove(&(approval, index));
                self.question_own.insert((approval, index), name);
            }
            self.close_own_answer(in_inbox, window, cx);
            return;
        }
        if let DialogKind::EditProposal { proposal } = kind {
            let text = self.goal_input.read(cx).value().trim().to_string();
            if text.is_empty() {
                self.notify_user("Write the instructions to approve, or decline the proposal instead.", true, cx);
                return;
            }
            self.approve_edited_instructions(proposal, text, cx);
            self.overlay = Overlay::None;
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        if name.is_empty() {
            self.notify_user("Enter a name.", true, cx);
            return;
        }
        let role = self.role_input.read(cx).value().trim().to_string();
        let instructions = self.instructions_input.read(cx).value().trim().to_string();
        let instructions = (!instructions.is_empty()).then_some(instructions);
        if matches!(kind, DialogKind::NewManager | DialogKind::EditManager { .. }) && role.is_empty() {
            self.notify_user("Give the employee a role. It can be anything.", true, cx);
            return;
        }
        if kind == DialogKind::NewProject && self.dialog_folder.is_none() {
            self.notify_user("Choose a folder for the project first (Ctrl+O).", true, cx);
            return;
        }
        let mut files: Vec<String> = Vec::new();
        if (matches!(kind, DialogKind::NewTask { .. } | DialogKind::TaskSchedule { .. }) || self.idea_files_on()) && !self.pending_task_files.is_empty() {
            files = self.pending_task_files.iter().map(|(p, _)| p.display().to_string()).collect();
        }
        let mut follow_up: Vec<backend::Request> = Vec::new();
        let request = match kind {
            DialogKind::NewProject => {
                let description = self.description_input.read(cx).value().trim().to_string();
                self.open_created = true;
                backend::Request::CreateProject { name, description: (!description.is_empty()).then_some(description), folder: self.dialog_folder.clone() }
            }
            DialogKind::NewMemory { project } => {
                let kind = if self.pending_memory_decision {
                    aurelius_domain::memory::MemoryKind::Decision
                } else {
                    aurelius_domain::memory::MemoryKind::ProjectKnowledge
                };
                self.memory = None;
                backend::Request::Remember { project_id: project, kind, content: name }
            }
            DialogKind::ReviseMemory { id } => backend::Request::ReviseMemory { memory_id: id, content: name },
            // The note keeps its lines as typed; the server trims blank lines around it.
            DialogKind::NewIdea { project } => {
                let project = self.pending_idea_project.unwrap_or(project);
                self.ideas_project = Some(project);
                let note = self.goal_input.read(cx).value().to_string();
                backend::Request::AddIdea { project_id: project, title: name, note: (!note.trim().is_empty()).then_some(note), kind: self.pending_idea_kind, files }
            }
            DialogKind::EditIdea { id } => {
                let note = self.goal_input.read(cx).value().to_string();
                let remove_files = std::mem::take(&mut self.pending_schedule_remove);
                backend::Request::UpdateIdea { idea_id: id, title: name, note: (!note.trim().is_empty()).then_some(note), kind: Some(self.pending_idea_kind), add_files: files, remove_files }
            }
            // Handled above: an answer stays in the inbox until it is sent; edited instructions are a decision.
            DialogKind::QuestionAnswer { .. } | DialogKind::EditProposal { .. } => return,
            DialogKind::TaskSchedule { project, schedule } => {
                let Some(employee_id) = self.pending_task_manager else {
                    self.notify_user("A recurring task needs an employee: choose one (Alt+M).", true, cx);
                    return;
                };
                let recurrence = match self.read_recurrence(cx) {
                    Ok(r) => r,
                    Err(e) => {
                        self.notify_user(e, true, cx);
                        return;
                    }
                };
                let goal = self.goal_input.read(cx).value().trim().to_string();
                let criteria: Vec<String> = role.split(';').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
                self.pending_task_manager = None;
                let run = std::mem::take(&mut self.pending_task_run);
                match schedule {
                    None => backend::Request::CreateSchedule(aurelius_domain::schedule::CreateTaskSchedule {
                        confirm_secret: false,
                        project_id: project,
                        employee_id,
                        title: name,
                        goal,
                        acceptance_criteria: criteria,
                        files,
                        run,
                        recurrence,
                        paused: self.pending_schedule_paused,
                    }),
                    Some(schedule_id) => backend::Request::UpdateSchedule(aurelius_domain::schedule::UpdateTaskSchedule {
                        confirm_secret: false,
                        schedule_id,
                        employee_id,
                        title: name,
                        goal,
                        acceptance_criteria: criteria,
                        add_files: files,
                        remove_files: std::mem::take(&mut self.pending_schedule_remove),
                        run,
                        recurrence,
                    }),
                }
            }
            DialogKind::NewTask { project } => {
                let goal = self.goal_input.read(cx).value().trim().to_string();
                let criteria: Vec<String> = role.split(';').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
                let employee_id = self.pending_task_manager.take();
                if employee_id.is_none() {
                    self.notify_user("No active employee: the task is saved as a draft.", false, cx);
                }
                let run = std::mem::take(&mut self.pending_task_run);
                self.remember_mode(employee_id, run.mode);
                backend::Request::CreateTask { project_id: project, employee_id, title: name, goal, criteria, run, files }
            }
            DialogKind::NewManager => {
                self.open_new_manager = true;
                let title = self.title_input.read(cx).value().trim().to_string();
                backend::Request::CreateEmployee {
                    name,
                    role,
                    title: (!title.is_empty()).then_some(title),
                    reports_to: self.pending_reports_to.take(),
                    instructions,
                    folder: self.dialog_folder.clone(),
                }
            }
            DialogKind::EditManager { id, version } => {
                self.save_manager_defaults(id);
                self.save_manager_delegation(id);
                let sequential_tasks = self.take_manager_sequential(id);
                // An empty title clears it.
                let title = self.title_input.read(cx).value().trim().to_string();
                // The reporting line is its own command: sent after the update, which checks the manager's version.
                let before = self.managers.iter().find(|m| m.id == id).and_then(|m| m.reports_to);
                if self.pending_reports_to != before {
                    follow_up.push(backend::Request::SetReportsTo { employee_id: id, reports_to: self.pending_reports_to });
                }
                backend::Request::UpdateEmployee { employee_id: id, name, role, title: Some(title), instructions, sequential_tasks, expected_version: version }
            }
        };
        if self.requests.send(request).is_err() {
            self.notify_user("The connection to the background service is closed.", true, cx);
        }
        for request in follow_up {
            let _ = self.requests.send(request);
        }
        self.overlay = Overlay::None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn on_dialog_next_field(&mut self, _: &DialogNextField, window: &mut Window, cx: &mut Context<Self>) {
        let fields: Vec<Entity<InputState>> = match self.overlay {
            Overlay::Dialog(DialogKind::NewProject) => vec![self.name_input.clone(), self.description_input.clone()],
            Overlay::Dialog(DialogKind::NewIdea { .. } | DialogKind::EditIdea { .. }) => {
                if self.name_input.read(cx).focus_handle(cx).is_focused(window) {
                    self.goal_input.update(cx, |i, cx| i.focus(window, cx));
                } else {
                    self.name_input.update(cx, |i, cx| i.focus(window, cx));
                }
                return;
            }
            Overlay::Dialog(DialogKind::NewTask { .. } | DialogKind::TaskSchedule { .. }) => {
                let mut handles = vec![self.name_input.read(cx).focus_handle(cx), self.goal_input.read(cx).focus_handle(cx)];
                let mut focus: Vec<Box<dyn Fn(&mut Window, &mut App)>> = vec![
                    Box::new({ let i = self.name_input.clone(); move |w, cx| i.update(cx, |i, cx| i.focus(w, cx)) }),
                    Box::new({ let i = self.goal_input.clone(); move |w, cx| i.update(cx, |i, cx| i.focus(w, cx)) }),
                ];
                // The criteria are folded under "More" until it is opened.
                if self.task_more_open {
                    handles.push(self.role_input.read(cx).focus_handle(cx));
                    focus.push(Box::new({ let i = self.role_input.clone(); move |w, cx| i.update(cx, |i, cx| i.focus(w, cx)) }));
                }
                // A recurring task also has the hours or the time of its recurrence.
                if matches!(self.overlay, Overlay::Dialog(DialogKind::TaskSchedule { .. })) {
                    let input = if self.pending_recurrence == 0 { self.recur_hours_input.clone() } else { self.recur_time_input.clone() };
                    handles.push(input.read(cx).focus_handle(cx));
                    focus.push(Box::new(move |w, cx| input.update(cx, |i, cx| i.focus(w, cx))));
                }
                let current = handles.iter().position(|h| h.is_focused(window)).unwrap_or(0);
                focus[(current + 1) % focus.len()](window, cx);
                return;
            }
            Overlay::Dialog(DialogKind::NewManager | DialogKind::EditManager { .. }) => {
                vec![self.name_input.clone(), self.role_input.clone(), self.title_input.clone(), self.instructions_input.clone()]
            }
            _ => return,
        };
        let current = fields.iter().position(|f| f.read(cx).focus_handle(cx).is_focused(window)).unwrap_or(0);
        let next = fields[(current + 1) % fields.len()].clone();
        next.update(cx, |input, cx| input.focus(window, cx));
    }

    fn on_confirm_primary(&mut self, _: &ConfirmPrimary, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay == Overlay::None && !matches!(self.screen, Page::Inbox | Page::Settings) && self.send_first_quick_reply(window, cx) {
            return;
        }
        match self.overlay {
            Overlay::Dialog(_) => self.confirm_dialog(window, cx),
            Overlay::Guidelines => self.guideline_dialog_primary(window, cx),
            Overlay::None if self.screen == Page::Inbox => self.decide_selected(true, window, cx),
            Overlay::PlanApproval(_) => self.decide_plan(aurelius_domain::approval::PlanDecision::Approve, window, cx),
            Overlay::None if self.screen == Page::Settings && self.adding_server => self.servers_submit(window, cx),
            Overlay::None if self.screen == Page::Settings && self.server_name.editing => self.server_name_save(cx),
            Overlay::None if self.screen == Page::Settings && self.entering_key => self.save_key(window, cx),
            Overlay::Confirm(kind) => {
                let request = match kind {
                    ConfirmKind::ArchiveManager(employee_id) => backend::Request::SetEmployeeStatus { employee_id, status: LifecycleStatus::Archived },
                    ConfirmKind::ArchiveProject(project_id) => backend::Request::SetProjectStatus { project_id, status: LifecycleStatus::Archived },
                    ConfirmKind::DeleteConversation(conversation_id) => {
                        let _ = self.requests.send(backend::Request::DeleteConversation(conversation_id));
                        self.overlay = Overlay::None;
                        if self.manager_thread == Some(conversation_id) {
                            self.manager_thread = None;
                            if let Page::Employee(ManagerRef::Real(id)) = self.screen {
                                // Its latest other task, if any.
                                let next = self.tasks.iter().filter(|t| t.employee_id == Some(id)).filter_map(|t| t.conversation_id.map(|c| (c, t.updated_at_ms))).filter(|(c, _)| *c != conversation_id).max_by_key(|(_, at)| *at).map(|(c, _)| c);
                                self.manager_thread = next;
                                self.open(next);
                            }
                        }
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::AllowBrowser(conversation_id) => {
                        for m in self.managers.iter_mut().filter(|m| m.conversation_id == conversation_id) {
                            m.browser_allowed = true;
                        }
                        let _ = self.requests.send(backend::Request::SetBrowserAllowed { conversation_id, allowed: true });
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::AllowScreen(conversation_id) => {
                        for m in self.managers.iter_mut().filter(|m| m.conversation_id == conversation_id) {
                            m.screen_allowed = true;
                        }
                        let _ = self.requests.send(backend::Request::SetScreenAllowed { conversation_id, allowed: true });
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::RestoreBackup(index) => {
                        if let Some(b) = self.backups.as_ref().and_then(|s| s.backups.get(index)) {
                            let _ = self.requests.send(backend::Request::Backups(backend::BackupsCall::Restore(b.id.clone())));
                        }
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::RestartServer => {
                        let _ = self.requests.send(backend::Request::RestartServer);
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::ClearResults => {
                        // Optimistic: empty the list now; the backend then reloads the real one.
                        // Only results: what needs you, and the hidden alerts of approvals, stay.
                        let result = |n: &NotificationView| matches!(note_kind(n), NoteKind::Result | NoteKind::Failed);
                        let ids: Vec<String> = self.inbox_notifications().into_iter().filter(|n| result(n)).map(|n| n.id.clone()).collect();
                        if let Some(n) = self.notifications.as_mut() {
                            n.notifications.retain(|n| !result(n));
                            n.unread = n.notifications.iter().filter(|n| !n.read).count() as _;
                        }
                        self.inbox_new.clear();
                        self.inbox_selected = 0;
                        let _ = self.requests.send(backend::Request::ClearResults(ids));
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::RevokeSession(session_id, of) => {
                        let _ = self.requests.send(backend::Request::BoardMembers(backend::UserCall::Revoke { session_id, of }));
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::RevokeOtherSessions => {
                        let _ = self.requests.send(backend::Request::BoardMembers(backend::UserCall::RevokeOthers));
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::DisableMember(user) => {
                        self.members_disable(user, cx);
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::UseMyLogin(ticket) => {
                        self.ticket_use_my_login(ticket, cx);
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::ArchiveVaultItem(item) => {
                        self.vault_archive(item, cx);
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::LooksLikeSecret => {
                        self.vault_send_anyway(cx);
                        self.overlay = Overlay::None;
                        self.vault_hold_next(cx);
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::DisconnectClaude => {
                        self.claude_unlink(cx);
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::ShutdownNow => {
                        let _ = self.requests.send(backend::Request::Power(backend::PowerCall::Shutdown(aurelius_protocol::ShutdownWhen::Now)));
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::DeleteSchedule(schedule_id) => {
                        let _ = self.requests.send(backend::Request::DeleteSchedule(schedule_id));
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::DeleteIdea(idea_id) => {
                        let _ = self.requests.send(backend::Request::DeleteIdea(idea_id));
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                    ConfirmKind::ForgetMemory(memory_id) => {
                        let _ = self.requests.send(backend::Request::ForgetMemory(memory_id));
                        self.overlay = Overlay::None;
                        self.focus.focus(window, cx);
                        cx.notify();
                        return;
                    }
                };
                let _ = self.requests.send(request);
                self.overlay = Overlay::None;
                self.back_stack.clear();
                self.screen = match (kind, self.screen) {
                    (ConfirmKind::ArchiveManager(_), _) => Page::Employees,
                    _ => Page::Projects,
                };
                self.focus.focus(window, cx);
                self.notify_user("Archived. The history is kept.", false, cx);
            }
            Overlay::Review => {
                self.approval_done = true;
                self.overlay = Overlay::None;
                self.focus.focus(window, cx);
                self.notify_user(
                    "Sample: approved. Real approvals are bound to the exact action.",
                    false,
                    cx,
                );
            }
            _ => {}
        }
    }

    fn close_overlay(&mut self) {
        self.overlay = Overlay::None;
    }

    pub fn close_overlay_now(&mut self) {
        self.close_overlay();
    }

    fn on_back(&mut self, _: &Back, window: &mut Window, cx: &mut Context<Self>) {
        let gated = self.member_gate() != crate::screens::board_members::Gate::Open;
        if self.overlay == Overlay::None && self.member_step().is_some() && !(gated && self.screen == Page::Settings) && self.members_back(window, cx) {
            return;
        }
        if self.mcp_manage.add.is_some() {
            self.close_add_mcp(cx);
            return;
        }
        if self.plugins.add.is_some() {
            self.close_add_plugin(cx);
            return;
        }
        if self.project_settings.is_some() {
            self.close_project_settings(cx);
            return;
        }
        if self.subagents_open && self.overlay == Overlay::None {
            self.subagents_open = false;
            cx.notify();
            return;
        }
        if self.overlay == Overlay::Menu {
            self.close_menu(window, cx);
            return;
        }
        if self.board.dialog.is_some() {
            self.board_close_dialog(window, cx);
            return;
        }
        if self.board_cancel_hold(cx) {
            return;
        }
        if self.board.editing.is_some() && matches!(self.panel, Panel::Ticket(_)) {
            self.ticket_cancel_edit(cx);
            return;
        }
        if self.overlay == Overlay::None && self.preview_shown()
            && (self.at_work_selected.is_some() || f32::from(window.viewport_size().width) >= crate::screens::activity::PREVIEW_MIN_WINDOW)
        {
            self.at_work_selected = None;
            self.preview_closed = true;
            cx.notify();
            return;
        }
        if self.overlay == Overlay::ServerFolder {
            self.folder_back(window, cx);
            return;
        }
        if self.overlay == Overlay::Confirm(ConfirmKind::LooksLikeSecret) {
            // Not sent: the message goes back to its composer (B10).
            self.vault_hold_cancel(window, cx);
            self.overlay = Overlay::None;
            self.vault_hold_next(cx);
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        if self.vault_back(window, cx) {
            return;
        }
        if self.guidelines_back(window, cx) {
            return;
        }
        if self.overlay == Overlay::ConnectClaude {
            // Closing the dialog ends Claude's sign-in on the server too.
            self.claude_cancel(cx);
            self.focus.focus(window, cx);
            return;
        }
        if self.overlay == Overlay::None && self.screen == Page::Settings && self.server_name_cancel(window, cx) {
            return;
        }
        if self.overlay == Overlay::None && self.screen == Page::Settings && self.adding_server {
            self.adding_server = false;
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        if matches!(self.overlay, Overlay::Confirm(ConfirmKind::RestartServer | ConfirmKind::ShutdownNow | ConfirmKind::ClearResults)) {
            self.overlay = Overlay::None;
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        if let Overlay::Dialog(DialogKind::QuestionAnswer { in_inbox, .. }) = self.overlay {
            self.close_own_answer(in_inbox, window, cx);
        } else if self.overlay != Overlay::None {
            self.overlay = Overlay::None;
        } else if !self.focus.is_focused(window) {
            // Leaving a text field returns to keyboard navigation.
        } else if self.history_open && matches!(self.screen, Page::Employee(..)) {
            self.history_open = false;
        } else if self.panel_shown() {
            self.close_panel(cx);
        } else if let Some(previous) = self.back_stack.pop() {
            self.screen = previous;
            self.selected = 0;
            self.open_thread();
        } else if self.screen != Page::Dashboard {
            self.screen = Page::Dashboard;
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn on_focus_composer(&mut self, _: &FocusComposer, window: &mut Window, cx: &mut Context<Self>) {
        let has_composer = match self.thread_screen() {
            // Employees have a composer only inside a task.
            Page::Employee(ManagerRef::Real(_)) => self.current_manager_thread().is_some(),
            Page::Employee(..) | Page::GroupChat => true,
            _ => false,
        };
        if has_composer {
            self.composer.update(cx, |input, cx| input.focus(window, cx));
        } else {
            self.on_open_palette(&OpenPalette, window, cx);
        }
    }

    fn on_toggle_preview(&mut self, _: &TogglePreview, window: &mut Window, cx: &mut Context<Self>) {
        self.show_preview = !self.show_preview;
        if !self.show_preview && matches!(self.screen, Page::Project(ProjectRef::Sample(_), _) | Page::Employee(..)) {
            self.screen = Page::Projects;
            self.back_stack.clear();
        }
        self.selected = 0;
        let text = if self.show_preview { "Sample data visible." } else { "Sample data hidden; only your own data." };
        self.notify_user(text, false, cx);
        self.focus.focus(window, cx);
    }


    fn on_quit(&mut self, _: &Quit, window: &mut Window, _: &mut Context<Self>) {
        // Closing the window ends the UI only; the daemon keeps running.
        window.remove_window();
    }

    fn on_select(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.preview_closed = false;
        // Dashboard: Up and Down move through "At work now" (its employee shows in the preview pane).
        if self.screen == Page::Dashboard {
            let at_work = self.working_now().len();
            if at_work > 0 {
                let next = self.at_work_selected.map_or(if delta > 0 { 0 } else { at_work as isize - 1 }, |n| n as isize + delta);
                self.at_work_selected = Some(next.clamp(0, at_work as isize - 1) as usize);
                cx.notify();
            }
            return;
        }
        let count = self.item_count();
        if count == 0 {
            return;
        }
        self.selected = (self.selected as isize + delta).clamp(0, count as isize - 1) as usize;
        cx.notify();
    }

    fn on_open_selected(&mut self, _: &OpenSelected, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay == Overlay::None && self.screen == Page::Inbox {
            if self.inbox_note().is_some() {
                self.open_inbox_notification(window, cx);
            }
            return;
        }
        match self.screen {
            Page::Dashboard if self.at_work_selected.is_some() => {
                if let Some(w) = self.at_work_selected.and_then(|n| self.working_now().into_iter().nth(n)) {
                    self.open_manager(ManagerRef::Real(w.manager), window, cx);
                }
            }
            Page::Project(ProjectRef::Real(_), Tab::Tasks) => {
                self.on_open_task_conversation(&OpenTaskConversation, window, cx);
            }
            Page::Projects if self.projects_tab == ProjectsTab::Projects => {
                if let Some(project) = self.home_projects().get(self.selected).copied() {
                    self.open_project(project, window, cx);
                }
            }
            _ => {}
        }
    }

    fn on_filter(&mut self, filter: usize, cx: &mut Context<Self>) {
        match self.screen {
            Page::Project(_, Tab::Tasks) => self.task_filter = filter,
            Page::Project(ProjectRef::Real(_), Tab::Memory) => self.memory_filter = filter.min(2),
            _ if self.on_ideas_screen() && filter == 3 => self.ideas_author = self.ideas_author.next(),
            _ if self.on_ideas_screen() => self.ideas_filter = filter.min(2),
            _ => return,
        }
        self.selected = 0;
        cx.notify();
    }

    fn on_review(&mut self, _: &ReviewApproval, window: &mut Window, cx: &mut Context<Self>) {
        let has_approval = matches!(self.screen, Page::Project(p, Tab::Overview) if self.sample(p).is_some_and(|s| s.approvals > 0))
            && !self.approval_done;
        if has_approval {
            self.overlay = Overlay::Review;
            self.focus.focus(window, cx);
            cx.notify();
        } else {
            self.notify_user("No approval is waiting for you here.", false, cx);
        }
    }

    /// Send what is in the composer. It waits behind a running answer (queued) unless it is sent with `send_now`.
    pub fn send_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.send_composer(false, window, cx);
    }

    /// Send what is in the composer; `interrupt` stops the answers still
    /// running in the conversation first ("Send now").
    fn send_composer(&mut self, interrupt: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().trim().to_string();
        if text.is_empty() && self.pending_images.is_empty() && self.pending_files.is_empty() {
            return;
        }
        // On an employee's page a message goes into one of its tasks; talk with it in the Team chat.
        if matches!(self.thread_screen(), Page::Employee(ManagerRef::Real(_))) && self.current_manager_thread().is_none() {
            self.notify_user("Open one of this employee's tasks to message it, or talk with it in the Team chat (Ctrl+8).", true, cx);
            return;
        }
        if self.thread_screen() == Page::GroupChat && self.group_conversation.is_none() {
            let why = self.group_error.clone().unwrap_or_else(|| "The Team chat is still loading.".into());
            self.notify_user(why, true, cx);
            return;
        }
        // MU2-08 AC2: an archived project's channel is read-only, and so is a closed ticket's.
        if self.thread_screen() == Page::GroupChat && self.channel_read_only() {
            let note = self.current_channel().map_or_else(String::new, crate::screens::discussion::read_only_bar);
            self.notify_user(note, true, cx);
            return;
        }
        if !self.pending_files.is_empty() && !self.takes_manager_files() {
            self.notify_user("Files go to an employee conversation; remove them or open one.", true, cx);
            return;
        }
        let images: Vec<String> = std::mem::take(&mut self.pending_images).iter().map(|p| p.display().to_string()).collect();
        let files: Vec<String> = std::mem::take(&mut self.pending_files).iter().map(|(p, _)| p.display().to_string()).collect();
        match self.thread_screen() {
            Page::Employee(ManagerRef::Real(employee_id)) => {
                let Some(conversation_id) = self.current_manager_thread() else { return };
                let _ = self.requests.send(backend::Request::SendManager { employee_id, text, conversation_id, images, files, interrupt });
            }
            // Who answers follows from the @names in it (else the top of the org chart); nothing to interrupt.
            Page::GroupChat => {
                let _ = self.requests.send(backend::Request::SendGroup { text, images, files, channel: self.channel });
            }
            _ => {
                self.manager_extra.push(text.into());
                self.notify_user("Sample employee: this message only exists in this window.", false, cx);
            }
        }
        self.composer.update(cx, |input, cx| input.set_value("", window, cx));
    }

    /// A paste into the composer: a copied image (a screenshot) or copied
    /// image files wait beside it to go with the message. Returns whether it
    /// took the paste; anything else is pasted as text.
    pub fn paste_into_composer(&mut self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let mut found = Vec::new();
        let mut took = false;
        for entry in item.entries() {
            match entry {
                ClipboardEntry::Image(image) => {
                    took = true;
                    match save_pasted_image(image) {
                        Ok(path) => found.push(path),
                        Err(e) => self.notify_user(e, true, cx),
                    }
                }
                ClipboardEntry::ExternalPaths(paths) => {
                    let images: Vec<_> = paths.paths().iter().filter(|p| is_image_file(p)).cloned().collect();
                    took |= !images.is_empty();
                    found.extend(images);
                }
                ClipboardEntry::String(_) => {}
            }
        }
        self.add_images(found, cx);
        took
    }

    /// Pick image files for the next message.
    pub fn pick_images(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("Add images".into()) });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                let _ = this.update(cx, |this, cx| {
                    let (images, other): (Vec<_>, Vec<_>) = paths.into_iter().partition(|p| is_image_file(p));
                    if !other.is_empty() {
                        this.notify_user("Only images (PNG, JPEG, GIF, WebP, BMP) go with a message; attach other files with Ctrl+O.", true, cx);
                    }
                    this.add_images(images, cx);
                });
            }
        })
        .detach();
    }

    fn add_images(&mut self, images: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        for image in images {
            if self.pending_images.len() >= aurelius_domain::conversation::IMAGES_MAX {
                self.notify_user(format!("At most {} images per message.", aurelius_domain::conversation::IMAGES_MAX), true, cx);
                break;
            }
            if !self.pending_images.contains(&image) {
                self.pending_images.push(image);
            }
        }
        cx.notify();
    }

    /// Interrupt the running answer and send the message right away, instead
    /// of after the answer (Enter waits). The new answer starts once the old
    /// one has stopped and sees what it had written so far.
    pub fn send_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composer.read(cx).value().trim().is_empty() && self.pending_images.is_empty() && self.pending_files.is_empty() {
            return;
        }
        // The server stops whatever still runs in the conversation together with
        // the message, so a stale view here can't turn this into a plain send.
        // The separate Stop is for servers from before that (they ignore `interrupt`).
        if let Some(run_id) = self.active_run() {
            let _ = self.requests.send(backend::Request::Stop { run_id });
        }
        self.send_composer(true, window, cx);
    }

    // ── Palette ─────────────────────────────────────────────────────────

    pub fn palette_entries(&self, cx: &App) -> Vec<PaletteEntry> {
        let query = self.palette_input.read(cx).value().to_lowercase();
        let mut entries = Vec::new();
        for project in self.home_projects() {
            let name = self.project_name(project);
            entries.push(PaletteEntry {
                label: format!("Open project {name}").into(),
                hint: if matches!(project, ProjectRef::Sample(_)) { "Sample".into() } else { "Project".into() },
                keys: None,
                target: PaletteTarget::Project(project),
            });
            if let (ProjectRef::Real(id), None) = (project, self.window_project) {
                entries.push(PaletteEntry {
                    label: format!("Open project {name} in new window").into(),
                    hint: "Window".into(),
                    keys: None,
                    target: PaletteTarget::ProjectWindow(id),
                });
            }
            // Sample employees under their sample project; real ones are the company's (below).
            for manager in self.project_managers(project).into_iter().filter(|_| matches!(project, ProjectRef::Sample(_))) {
                entries.push(PaletteEntry {
                    label: format!("{} · {} ({})", manager.name, manager.role, name).into(),
                    hint: "Employee".into(),
                    keys: None,
                    target: PaletteTarget::Manager(project, manager.key),
                });
            }
        }
        for m in self.managers.iter().filter(|m| m.status != LifecycleStatus::Archived) {
            entries.push(PaletteEntry {
                label: format!("{} · {}", m.name, m.role).into(),
                hint: "Employee".into(),
                keys: None,
                target: PaletteTarget::Employee(m.id),
            });
        }
        // Ctrl+K "Go to #…" (fase 2 §3): the Team chat's channels, in the list's order.
        if self.window_project.is_none() {
            let s = crate::screens::discussion::sections(self.channels.as_deref().unwrap_or_default());
            for ch in s.general.into_iter().chain(s.projects).chain(s.tickets).chain(s.archived) {
                entries.push(PaletteEntry {
                    label: crate::screens::discussion::go_to_channel_label(ch).into(),
                    hint: "Channel".into(),
                    keys: None,
                    target: PaletteTarget::Channel(ch.conversation_id),
                });
            }
        }
        for shortcut in SHORTCUTS.iter().filter(|s| s.in_palette) {
            entries.push(PaletteEntry {
                label: shortcut.label.into(),
                hint: shortcut.section.title().into(),
                keys: shortcut.keys.first().copied(),
                target: PaletteTarget::Action(shortcut.action),
            });
        }
        if !query.is_empty() {
            entries.retain(|e| e.label.to_lowercase().contains(&query) || e.hint.to_lowercase().contains(&query));
        }
        entries
    }

    fn on_palette_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        let len = self.palette_entries(cx).len();
        if len > 0 {
            self.palette_selected = (self.palette_selected as isize + delta).rem_euclid(len as isize) as usize;
            cx.notify();
        }
    }

    pub fn run_palette_entry(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.palette_entries(cx).into_iter().nth(index) else { return };
        self.overlay = Overlay::None;
        self.focus.focus(window, cx);
        match entry.target {
            PaletteTarget::Project(p) => self.open_project(p, window, cx),
            PaletteTarget::ProjectWindow(p) => self.open_project_window(p, cx),
            PaletteTarget::Manager(_, m) => self.open_manager(m, window, cx),
            PaletteTarget::Employee(id) => self.open_manager(ManagerRef::Real(id), window, cx),
            PaletteTarget::Channel(id) => self.go_to_channel(id, window, cx),
            PaletteTarget::Action(make) => window.dispatch_action(make(), cx),
        }
        cx.notify();
    }
}

pub struct PaletteEntry {
    pub label: SharedString,
    pub hint: SharedString,
    pub keys: Option<&'static str>,
    pub target: PaletteTarget,
}

#[derive(Clone, Copy)]
pub enum PaletteTarget {
    Project(ProjectRef),
    ProjectWindow(ProjectId),
    Manager(ProjectRef, ManagerRef),
    Employee(EmployeeId),
    /// A channel of the Team chat.
    Channel(ConversationId),
    Action(fn() -> Box<dyn Action>),
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Whatever left the Team chat (a page, Back, a jump), its channels stop being read on every chat event.
        let team_chat = self.screen == Page::GroupChat;
        if team_chat != self.channels_followed {
            self.channels_followed = team_chat;
            let _ = self.requests.send(backend::Request::FollowChannels(team_chat));
        }
        if let Some(project) = self.window_project {
            // Whatever led elsewhere (Back, Chat, an archive), a project window stays on its project.
            // Employees work company-wide: their pages may open in a project window too.
            let here = matches!(self.screen, Page::Project(ProjectRef::Real(p), _) if p == project) || matches!(self.screen, Page::Employee(_));
            if !here {
                self.screen = Page::Project(ProjectRef::Real(project), Tab::Overview);
                self.back_stack.clear();
            }
            if let Some(p) = self.projects.iter().find(|p| p.id == project) {
                if self.window_title != p.name {
                    self.window_title = p.name.clone();
                    window.set_window_title(&crate::project_window::title(&p.name));
                }
            }
        }
        self.track_place();
        // The webviews are native windows over the app: hidden as soon as their panel is.
        self.sync_artifact_webview();
        if let Some(text) = self.pending_placeholder.take() {
            self.composer.update(cx, |input, cx| input.set_placeholder(text, window, cx));
        }
        if let Some(text) = self.pending_notice.take() {
            self.notify_user(text, true, cx);
        }
        if std::mem::take(&mut self.pending_servers) && self.overlay == Overlay::None {
            self.on_open_servers(&OpenServers, window, cx);
        }
        let fonts = Fonts::get(cx).clone();
        // The board member gate (MU1-20/21): only Settings → Servers stays reachable.
        self.members_hold();
        self.members_show_drafts(window, cx);
        self.claude_clear_code(window, cx);
        let gated = self.member_gate() != crate::screens::board_members::Gate::Open;
        let member_step = crate::screens::board_members::shown_step(self.member_gate(), &self.members.stack, self.screen);
        let page: AnyElement = match member_step {
            Some(step) => self.render_members(step, window, cx),
            None => self.render_company(window, cx),
        };
        // The right panel: a task or conversation opened from the page (not over an employee
        // page, which shows its own conversation).
        let panel = (member_step.is_none() && self.panel_shown()).then(|| {
            let panel = self.render_panel(window, cx);
            self.resizable("right_panel", 460., crate::resize::Edge::Left, panel, cx)
        });
        let body: AnyElement = h()
            .flex_1()
            .min_h_0()
            .items_stretch()
            .relative()
            .when(self.window_project.is_none() && !self.sidebar_collapsed && !gated, |el| el.child(self.render_sidebar(false, window, cx)))
            .child(
                v().flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.render_title_strip(window, cx))
                    // Beside the panel the page keeps room for its forms; the panel gives way down to the 900 px window.
                    .child(h().flex_1().min_h_0().items_stretch().child(v().flex_1().min_w_0().when(panel.is_some(), |el| el.min_w(px(360.))).h_full().child(page)).children(panel)),
            )
            .when(self.window_project.is_none() && self.sidebar_collapsed && self.sidebar_peek && !gated, |el| {
                el.child(div().absolute().left_0().top_0().bottom_0().child(self.render_sidebar(true, window, cx)))
            })
            .into_any_element();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(c(theme::BG))
            .text_color(c(theme::TEXT))
            .font_family(fonts.ui)
            .text_size(px(13.))
            .key_context(APP)
            .on_action(cx.listener(Self::on_nav_back))
            .on_action(cx.listener(Self::on_nav_forward))
            .on_action(cx.listener(Self::on_toggle_sidebar))
            // Mouse back / forward buttons.
            .on_mouse_down(MouseButton::Navigate(NavigationDirection::Back), cx.listener(|this, _, w, cx| this.on_nav_back(&NavBack, w, cx)))
            .on_mouse_down(MouseButton::Navigate(NavigationDirection::Forward), cx.listener(|this, _, w, cx| this.on_nav_forward(&NavForward, w, cx)))
            .on_action(cx.listener(|this, _: &MenuNext, _, cx| this.move_menu(true, cx)))
            .on_action(cx.listener(|this, _: &MenuPrev, _, cx| this.move_menu(false, cx)))
            .on_action(cx.listener(|this, _: &MenuRun, w, cx| {
                let i = this.menu.as_ref().map_or(0, |m| m.selected);
                this.run_menu_entry(i, w, cx);
            }))
            .on_action(cx.listener(|this, _: &SettingsAppearance, _, cx| {
                if this.settings_tab == 1 {
                    this.set_appearance(theme::Appearance::current(cx).next(), cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsTrace, _, cx| {
                // Off deletes every kept trace: ask first.
                match this.trace_setting {
                    Some(s) if s.enabled => {
                        this.trace_confirm_off = true;
                        cx.notify();
                    }
                    Some(_) => this.set_trace_enabled(true, cx),
                    None => {}
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsArtifacts, _, cx| {
                if let Some(on) = this.artifacts_enabled {
                    this.set_artifacts_enabled(!on, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsShowHints, _, cx| {
                if this.settings_tab == 1 {
                    let always = !cx.try_global::<ui::Hints>().is_some_and(|h| h.always);
                    cx.set_global(ui::Hints { always });
                    set_ui_pref("always_show_shortcuts", always);
                    this.notify_user(if always { "Shortcut hints are shown." } else { "Shortcut hints are hidden." }, false, cx);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(Self::on_open_palette))
            .on_action(cx.listener(Self::on_open_memory))
            .on_action(cx.listener(|this, _: &GoDashboard, w, cx| this.go_nav(NavItem::Dashboard, w, cx)))
            .on_action(cx.listener(|this, _: &GoTeamChat, w, cx| this.go_nav(NavItem::Chat, w, cx)))
            .on_action(cx.listener(|this, _: &GoBoard, w, cx| this.go_nav(NavItem::Board, w, cx)))
            .on_action(cx.listener(|this, _: &GoVault, w, cx| this.go_nav(NavItem::Vault, w, cx)))
            .on_action(cx.listener(|this, _: &GoEmployees, w, cx| this.go_nav(NavItem::Employees, w, cx)))
            .on_action(cx.listener(|this, _: &GoOrgChart, w, cx| this.go_employees_tab(EmployeesTab::OrgChart, w, cx)))
            .on_action(cx.listener(|this, _: &GoGuidelines, w, cx| this.guidelines_go(crate::screens::guidelines::GuidelinesView::Library, w, cx)))
            .on_action(cx.listener(|this, _: &FindGuideline, w, cx| this.guidelines_find(w, cx)))
            .on_action(cx.listener(|this, _: &NewGuideline, w, cx| {
                if this.guidelines_can_edit() {
                    this.guideline_open(None, w, cx);
                } else {
                    this.notify_user(crate::screens::guidelines::READ_ONLY, true, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &GoProjects, w, cx| this.go_nav(NavItem::Projects, w, cx)))
            .on_action(cx.listener(|this, _: &GoTasks, w, cx| this.go_nav(NavItem::Tasks, w, cx)))
            .on_action(cx.listener(|this, _: &GoIdeas, w, cx| this.go_projects_tab(ProjectsTab::Ideas, w, cx)))
            .on_action(cx.listener(Self::on_new_item))
            .on_action(cx.listener(Self::on_new_project))
            .on_action(cx.listener(Self::on_edit))
            .on_action(cx.listener(Self::on_add_employee))
            .on_action(cx.listener(Self::on_new_task))
            .on_action(cx.listener(Self::on_assign_task))
            .on_action(cx.listener(Self::on_open_task_conversation))
            .on_action(cx.listener(Self::on_cycle_task_manager))
            .on_action(cx.listener(Self::on_cycle_give_project))
            .on_action(cx.listener(|this, _: &SubmitGiveWork, w, cx| this.give_work(w, cx)))
            .on_action(cx.listener(Self::on_cycle_task_mode))
            .on_action(cx.listener(Self::on_cycle_task_model))
            .on_action(cx.listener(Self::on_cycle_task_effort))
            .on_action(cx.listener(Self::on_toggle_manager_sequential))
            .on_action(cx.listener(|this, _: &CycleSubagentModel, w, cx| this.open_delegation_model(DelegationRow::Subagents, w, cx)))
            .on_action(cx.listener(Self::on_attach))
            .on_action(cx.listener(|this, _: &crate::keymap::BoardLeft, _, cx| this.board_step(crate::screens::board::Step::Left, cx)))
            .on_action(cx.listener(|this, _: &crate::keymap::BoardRight, _, cx| this.board_step(crate::screens::board::Step::Right, cx)))
            .on_action(cx.listener(|this, _: &crate::keymap::BoardUp, _, cx| this.board_step(crate::screens::board::Step::Up, cx)))
            .on_action(cx.listener(|this, _: &crate::keymap::BoardDown, _, cx| this.board_step(crate::screens::board::Step::Down, cx)))
            .on_action(cx.listener(|this, _: &crate::keymap::BoardOpen, w, cx| this.board_key_open(w, cx)))
            .on_action(cx.listener(|this, _: &crate::keymap::BoardStart, _, cx| this.board_key_start(cx)))
            .on_action(cx.listener(|this, _: &crate::keymap::BoardMoveTo, w, cx| this.board_key_move(w, cx)))
            .on_action(cx.listener(|this, _: &crate::keymap::BoardPickUp, _, cx| this.board_key_pick(cx)))
            .on_action(cx.listener(|this, _: &OpenUpdates, w, cx| {
                this.open_settings(&OpenSettings, w, cx);
                // Straight to the server's page when only the server has an update.
                let server_only = this.update.as_ref().is_some_and(|u| u.enabled && u.available) && !this.app_update().is_some_and(|u| u.enabled && u.available);
                this.settings_tab = if server_only { crate::screens::overlays::SERVER_PC_TAB } else { 1 };
                let _ = this.requests.send(backend::Request::CheckUpdate(true));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SettingsCheckUpdate, _, cx| {
                if this.settings_tab == 1 || this.settings_tab == crate::screens::overlays::SERVER_PC_TAB {
                    let _ = this.requests.send(backend::Request::CheckUpdate(true));
                    this.notify_user("Checking for updates…", false, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsInstallUpdate, _, cx| {
                if this.settings_tab == crate::screens::overlays::SERVER_PC_TAB {
                    match this.update.clone() {
                        Some(u) if u.enabled && u.available => {
                            let _ = this.requests.send(backend::Request::InstallUpdate);
                        }
                        Some(u) if !u.enabled => this.notify_user(u.reason.unwrap_or_default(), false, cx),
                        _ => this.notify_user("No newer server version available.", false, cx),
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsRenameServer, w, cx| {
                if this.settings_tab == crate::screens::overlays::SERVER_PC_TAB {
                    this.server_name_edit(w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsRestartServer, w, cx| {
                if this.settings_tab == crate::screens::overlays::SERVER_PC_TAB {
                    this.overlay = Overlay::Confirm(ConfirmKind::RestartServer);
                    this.overlay_focus.focus(w, cx);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsShutdownNow, w, cx| {
                if this.settings_tab == crate::screens::overlays::SERVER_PC_TAB {
                    this.overlay = Overlay::Confirm(ConfirmKind::ShutdownNow);
                    this.overlay_focus.focus(w, cx);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsShutdownIdle, _, _| {
                if this.settings_tab == crate::screens::overlays::SERVER_PC_TAB {
                    let _ = this.requests.send(backend::Request::Power(backend::PowerCall::Shutdown(aurelius_protocol::ShutdownWhen::WhenIdle)));
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsInstallAppUpdate, _, cx| {
                if this.settings_tab == 1 {
                    match this.app_update().cloned() {
                        Some(u) if u.enabled && u.available => {
                            let _ = this.requests.send(backend::Request::InstallAppUpdate);
                        }
                        Some(u) if !u.enabled => this.notify_user(u.reason.unwrap_or_default(), false, cx),
                        Some(_) => this.notify_user("No newer app version available.", false, cx),
                        None => this.notify_user("Update this app with scripts\\install-production.ps1 -Role client.", false, cx),
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &OpenBackups, w, cx| {
                this.open_settings(&OpenSettings, w, cx);
                this.settings_tab = 2;
                this.settings_selected = 0;
                let _ = this.requests.send(backend::Request::Backups(backend::BackupsCall::List));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SettingsLoginStart, _, cx| {
                if this.settings_tab != 1 {
                    return;
                }
                if let Some(host) = this.remote_name().map(str::to_owned) {
                    this.notify_user(format!("Start at sign-in belongs to the server: on {host}, run aurelius-ctl autostart on."), false, cx);
                    return;
                }
                let on = !this.login_start;
                match aurelius_host::startup::set_enabled(on) {
                    Ok(()) => {
                        this.login_start = aurelius_host::startup::enabled();
                        this.notify_user(if on { "The background service now starts when you sign in to Windows." } else { "No longer starts at sign-in." }, false, cx);
                    }
                    Err(e) => this.notify_user(e, false, cx),
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SettingsBackupNow, _, _| {
                if this.settings_tab == 2 {
                    let _ = this.requests.send(backend::Request::Backups(backend::BackupsCall::Create));
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsResume, _, cx| this.resume_work(cx)))
            .on_action(cx.listener(|this, _: &SettingsRestore, w, cx| {
                if this.settings_tab == 2 {
                    let count = this.backups.as_ref().map_or(0, |b| b.backups.len());
                    if count == 0 {
                        this.notify_user("No backups yet. Press B to make one.", false, cx);
                    } else {
                        this.overlay = Overlay::Confirm(ConfirmKind::RestoreBackup(this.settings_selected.min(count - 1)));
                        this.focus.focus(w, cx);
                        cx.notify();
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &ExportProject, _, cx| match this.current_real_project() {
                Some(p) => {
                    let _ = this.requests.send(backend::Request::ExportProject(p.id));
                    this.notify_user(format!("Exporting {}…", p.name), false, cx);
                }
                None => this.notify_user("Open one of your own projects first.", false, cx),
            }))
            .on_action(cx.listener(Self::on_open_remote))
            .on_action(cx.listener(|this, _: &SettingsRemoteToggle, _, cx| this.remote_toggle(cx)))
            .on_action(cx.listener(|this, _: &SettingsRefusePairing, _, cx| {
                if this.settings_tab == crate::screens::remote::REMOTE_TAB {
                    this.remote_decide(false, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsPushKey, w, cx| this.remote_push_key(w, cx)))
            .on_action(cx.listener(|this, _: &SettingsPushTest, _, cx| this.remote_push_test(cx)))
            .on_action(cx.listener(|this, _: &SettingsCopyPairing, _, cx| this.remote_copy_pairing(cx)))
            .on_action(cx.listener(|this, _: &SettingsNextTab, _, cx| {
                let next = crate::screens::overlays::next_settings_tab(this.settings_tab);
                this.set_settings_tab(next, cx);
            }))
            .on_action(cx.listener(|this, _: &LoadNextField, w, cx| this.load_move_field(true, w, cx)))
            .on_action(cx.listener(|this, _: &LoadPrevField, w, cx| this.load_move_field(false, w, cx)))
            .on_action(cx.listener(|this, _: &LoadLeaveField, w, cx| this.load_leave_field(w, cx)))
            .on_action(cx.listener(|this, _: &LoadSave, _, cx| this.save_load_limits(cx)))
            .on_action(cx.listener(|this, _: &LoadToggleEnabled, _, cx| this.toggle_load_enabled(cx)))
            .on_action(cx.listener(Self::on_open_servers))
            .on_action(cx.listener(|this, _: &FolderNext, _, cx| this.folder_move(true, cx)))
            .on_action(cx.listener(|this, _: &FolderPrev, _, cx| this.folder_move(false, cx)))
            .on_action(cx.listener(|this, _: &FolderOpen, _, cx| this.folder_open(cx)))
            .on_action(cx.listener(|this, _: &FolderParent, _, cx| this.folder_parent(cx)))
            .on_action(cx.listener(|this, _: &FolderUse, w, cx| this.folder_use(w, cx)))
            .on_action(cx.listener(|this, _: &FolderNewFolder, w, cx| this.folder_new(w, cx)))
            .on_action(cx.listener(|this, _: &FolderErase, _, cx| this.folder_erase(cx)))
            .on_action(cx.listener(|this, _: &FolderTypePath, w, cx| this.folder_type_path(w, cx)))
            .on_action(cx.listener(|this, _: &FolderSystemDialog, w, cx| this.folder_system_dialog(w, cx)))
            .on_action(cx.listener(Self::on_cancel_task))
            .on_action(cx.listener(Self::open_inbox))
            .on_action(cx.listener(Self::on_accept_memory))
            .on_action(cx.listener(Self::on_cycle_agent_mode))
            .on_action(cx.listener(Self::on_cycle_heartbeat))
            .on_action(cx.listener(|this, _: &PanelConfirm, _, cx| {
                if this.overlay == Overlay::Markdown {
                    this.reload_markdown(cx);
                } else if this.overlay == Overlay::Artifact {
                    this.reload_artifact(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleBrowser, w, cx| this.toggle_browser(w, cx)))
            .on_action(cx.listener(|this, _: &ToggleScreen, w, cx| this.toggle_screen(w, cx)))
            .on_action(cx.listener(Self::on_open_usage))
            .on_action(cx.listener(|this, _: &Filter4, _, cx| this.on_filter(3, cx)))
            .on_action(cx.listener(Self::on_open_project_folder))
            .on_action(cx.listener(Self::on_change_project_folder))
            .on_action(cx.listener(Self::on_choose_dialog_folder))
            .on_action(cx.listener(Self::on_remove_memory))
            .on_action(cx.listener(Self::on_toggle_pin))
            .on_action(cx.listener(Self::on_show_context))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(|this, _: &SettingsVerify, _, cx| this.settings_verify(cx)))
            .on_action(cx.listener(|this, _: &SettingsUse, _, cx| {
                if this.settings_tab == crate::screens::remote::REMOTE_TAB {
                    this.remote_decide(true, cx);
                } else if this.settings_tab == crate::screens::servers::SERVERS_TAB {
                    this.servers_connect(cx);
                } else {
                    this.settings_use(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsKey, w, cx| this.settings_key(w, cx)))
            .on_action(cx.listener(|this, _: &SettingsNew, w, cx| {
                if this.settings_tab == crate::screens::remote::REMOTE_TAB {
                    this.remote_pair(cx);
                    return;
                }
                if this.settings_tab == crate::screens::servers::SERVERS_TAB {
                    this.servers_add(w, cx);
                    return;
                }
                this.adding_connection = true;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SettingsRemove, _, cx| {
                if this.settings_tab == crate::screens::remote::REMOTE_TAB {
                    this.remote_revoke(cx);
                } else if this.settings_tab == crate::screens::servers::SERVERS_TAB {
                    this.servers_forget(cx);
                } else {
                    this.settings_remove(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SettingsModel, _, cx| this.settings_cycle_model(cx)))
            .on_action(cx.listener(|this, _: &CycleModel, w, cx| {
                // Outside an employee chat or task, Ctrl+M changes nothing: the default is set in Company settings.
                if this.thread_run().is_none() {
                    this.set_settings_tab(crate::screens::overlays::COMPANY_TAB, cx);
                    this.open_settings(&OpenSettings, w, cx);
                    return;
                }
                let size = w.viewport_size();
                let items = this.model_menu();
                this.open_menu(point(size.width - px(320.), size.height - px(70.)), items, w, cx);
            }))
            .on_action(cx.listener(|this, _: &TogglePlanFold, _, cx| this.toggle_latest_plan(cx)))
            .on_action(cx.listener(|this, _: &PrevChannel, w, cx| if this.screen == Page::GroupChat { this.step_channel(false, w, cx) }))
            .on_action(cx.listener(|this, _: &NextChannel, w, cx| if this.screen == Page::GroupChat { this.step_channel(true, w, cx) }))
            .on_action(cx.listener(|this, _: &ScrollPageUp, _, cx| this.scroll_by(-1., cx)))
            .on_action(cx.listener(|this, _: &ScrollPageDown, _, cx| this.scroll_by(1., cx)))
            .on_action(cx.listener(|this, _: &ScrollTop, _, cx| {
                this.settle_epoch += 1;
                this.active_scroll().set_offset(point(px(0.), px(0.)));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ScrollBottom, _, cx| {
                this.active_scroll().scroll_to_bottom();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Provider1, _, cx| this.add_provider(aurelius_domain::connection::ProviderKind::ClaudeCode, cx)))
            .on_action(cx.listener(|this, _: &Provider2, _, cx| this.add_provider(aurelius_domain::connection::ProviderKind::AnthropicApi, cx)))
            .on_action(cx.listener(|this, _: &Provider3, _, cx| this.add_provider(aurelius_domain::connection::ProviderKind::OpenaiApi, cx)))
            .on_action(cx.listener(|this, _: &RejectApproval, w, cx| this.decide_selected(false, w, cx)))
            .on_action(cx.listener(|this, _: &QuestionOption1, _, cx| this.choose_option(0, cx)))
            .on_action(cx.listener(|this, _: &QuestionOption2, _, cx| this.choose_option(1, cx)))
            .on_action(cx.listener(|this, _: &QuestionOption3, _, cx| this.choose_option(2, cx)))
            .on_action(cx.listener(|this, _: &QuestionOption4, _, cx| this.choose_option(3, cx)))
            .on_action(cx.listener(Self::on_question_next))
            .on_action(cx.listener(Self::on_question_own))
            .on_action(cx.listener(|this, _: &InboxUp, _, cx| {
                if this.screen == Page::Settings {
                    this.settings_selected = this.settings_selected.saturating_sub(1);
                } else {
                    this.inbox_selected = this.inbox_selected.saturating_sub(1);
                    this.question_focus = 0;
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &InboxDown, _, cx| {
                if this.screen == Page::Settings {
                    let max = if this.settings_tab == crate::screens::remote::REMOTE_TAB {
                        this.remote.as_ref().map_or(0, |r| r.clients.len().saturating_sub(1))
                    } else if this.settings_tab == crate::screens::servers::SERVERS_TAB {
                        this.server_rows().len().saturating_sub(1)
                    } else if this.settings_tab == 2 {
                        this.backups.as_ref().map_or(0, |b| b.backups.len().saturating_sub(1))
                    } else {
                        this.connections.as_ref().map_or(0, |c| c.connections.len().saturating_sub(1))
                    };
                    this.settings_selected = (this.settings_selected + 1).min(max);
                } else {
                    let max = this.inbox_len().saturating_sub(1);
                    this.inbox_selected = (this.inbox_selected + 1).min(max);
                    this.question_focus = 0;
                }
                cx.notify();
            }))
            .on_action(cx.listener(Self::on_toggle_management_mode))
            .on_action(cx.listener(Self::on_toggle_auto_allow))
            .on_action(cx.listener(|this, _: &OpenProjectSettings, w, cx| match this.current_project() {
                Some(ProjectRef::Real(p)) => this.open_project_settings(p, w, cx),
                Some(ProjectRef::Sample(_)) => this.notify_user("Sample projects are read-only.", true, cx),
                None => this.notify_user("Open a project first.", true, cx),
            }))
            .on_action(cx.listener(Self::on_cycle_reports_to))
            .on_action(cx.listener(Self::on_toggle_pause))
            .on_action(cx.listener(Self::on_archive))
            .on_action(cx.listener(Self::on_back))
            .on_action(cx.listener(Self::on_focus_composer))
            .on_action(cx.listener(Self::on_toggle_preview))
            .on_action(cx.listener(Self::on_quit))
            .on_action(cx.listener(Self::on_open_selected))
            .on_action(cx.listener(Self::on_review))
            .on_action(cx.listener(Self::on_confirm_primary))
            .on_action(cx.listener(Self::on_dialog_next_field))
            .on_action(cx.listener(|this, _: &StopGeneration, _, cx| {
                match this.active_run().or_else(|| this.group_run()) {
                    Some(run_id) => {
                        let _ = this.requests.send(backend::Request::Stop { run_id });
                    }
                    None => this.notify_user("No answer is running here.", false, cx),
                }
            }))
            .on_action(cx.listener(|this, _: &SendNow, w, cx| this.send_now(w, cx)))
            .on_action(cx.listener(|this, _: &RetryAnswer, _, cx| {
                let target = match (this.current_manager_thread(), this.messages.as_ref()) {
                    (Some(id), Some(snapshot)) if snapshot.conversation_id == id => snapshot
                        .messages
                        .iter()
                        .rev()
                        .find(|m| m.role == MessageRole::User)
                        .map(|m| (id, m.id)),
                    _ => None,
                };
                match target {
                    Some((conversation_id, user_message_id)) => {
                        let _ = this.requests.send(backend::Request::Retry { conversation_id, user_message_id });
                    }
                    None => this.notify_user("Open a chat with a question first.", false, cx),
                }
            }))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.on_select(1, cx)))
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.on_select(-1, cx)))
            .on_action(cx.listener(|this, _: &Filter1, _, cx| this.on_filter(0, cx)))
            .on_action(cx.listener(|this, _: &Filter2, _, cx| this.on_filter(1, cx)))
            .on_action(cx.listener(|this, _: &Filter3, _, cx| this.on_filter(2, cx)))
            .on_action(cx.listener(|this, _: &NextTab, w, cx| this.step_tab(1, w, cx)))
            .on_action(cx.listener(|this, _: &PrevTab, w, cx| this.step_tab(-1, w, cx)))
            .on_action(cx.listener(|this, _: &ToggleHistory, _, cx| {
                this.history_open = !this.history_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PaletteUp, _, cx| this.on_palette_move(-1, cx)))
            .on_action(cx.listener(|this, _: &PaletteDown, _, cx| this.on_palette_move(1, cx)))
            .on_action(cx.listener(|this, _: &PaletteConfirm, w, cx| this.run_palette_entry(this.palette_selected, w, cx)))
            .on_action(cx.listener(|this, _: &PaletteClose, w, cx| this.on_back(&Back, w, cx)))
            .on_action(cx.listener(|this, _: &DialogCancel, w, cx| this.on_back(&Back, w, cx)))
            .child(body)
            .child(self.render_status_bar(cx))
            .when(self.overlay != Overlay::None || self.project_settings.is_some() || self.board.dialog.is_some(), |el| el.child(self.render_overlay(window, cx)))
            .when_some(self.hovered_link.clone(), |el, url| {
                el.child(
                    div().absolute().left_0().bottom_0().max_w(relative(0.6)).h(px(26.)).flex().items_center().px(px(10.))
                        .rounded_tr(px(6.)).border_t_1().border_r_1().border_color(c(theme::BORDER_STRONG)).bg(c(theme::SURFACE_2))
                        .text_size(px(11.5)).text_color(c(theme::TEXT_2))
                        .child(div().truncate().child(url)),
                )
            })
    }
}

impl AppView {
    /// The strip above the page: back and forward, what runs, updates, usage and the
    /// app menu; it drags the window (the OS title bar is hidden) and carries its controls.
    fn render_title_strip(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let working = self.running_jobs().count();
        let in_window = self.window_project.is_some();
        let bar = div()
            .id("title-drag")
            .h_full()
            .flex_1()
            // Without the sidebar (a project window, or hidden) macOS draws its window buttons here.
            .pl(px(if (in_window || self.sidebar_collapsed) && cfg!(target_os = "macos") && !window.is_fullscreen() { 84. } else { 12. }))
            .pr(px(12.))
            .flex()
            .items_center()
            .window_control_area(WindowControlArea::Drag)
            .child({
                let can_back = !self.history_back.is_empty();
                let can_forward = !self.history_forward.is_empty();
                let has_sidebar = !in_window;
                let arrow = |id: &'static str, forward: bool, label: &'static str, enabled: bool| {
                    ui::icon_button(id).aria_label(label).tooltip(ui::tip(label, Some(if forward { "alt-right" } else { "alt-left" }))).occlude().size(px(32.)).rounded(px(7.)).when(!enabled, |b| b.opacity(0.35)).child(ui::nav_arrow(forward, 20., theme::TEXT))
                };
                h().gap(px(2.))
                    .when(has_sidebar, |el| {
                        el.child(
                            ui::icon_button("sidebar-toggle")
                                .aria_label("Expand or collapse the sidebar")
                                .tooltip(ui::tip(if self.sidebar_collapsed { "Expand sidebar" } else { "Collapse sidebar" }, Some("secondary-b")))
                                .occlude()
                                // Hidden, hovering the button shows the sidebar over the page.
                                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                    if *hovered && this.sidebar_collapsed && !this.sidebar_peek {
                                        this.sidebar_peek = true;
                                        cx.notify();
                                    }
                                }))
                                .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.on_toggle_sidebar(&ToggleSidebar, w, cx)))
                                .child(ui::sidebar_glyph(!self.sidebar_collapsed)),
                        )
                    })
                    .child(arrow("nav-back", false, "Back", can_back).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.on_nav_back(&NavBack, w, cx))))
                    .child(arrow("nav-forward", true, "Forward", can_forward).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.on_nav_forward(&NavForward, w, cx))))
            })
            .when(in_window, |el| el.child(div().ml(px(10.)).font_family(ui::serif(cx)).text_size(px(15.)).child(self.window_title.clone())))
            .when_some(self.remote_name().map(str::to_owned).filter(|_| in_window), |el, host| {
                el.child(div().ml(px(10.)).text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(format!("on {host}")))
            })
            .when(self.backups.as_ref().is_some_and(|b| b.paused.is_some()), |el| {
                el.child(
                    h().id("paused-pill").ml(px(14.)).gap(px(6.)).px(px(8.)).py(px(2.)).rounded(px(10.)).cursor_pointer().occlude()
                        .bg(c(theme::ATTENTION_BG)).text_color(c(theme::ACCENT_TEXT)).text_size(px(11.5))
                        .child(ui::pulse_dot("paused-dot", 6., theme::ACCENT))
                        .child("Paused after restore · click to resume")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.resume_work(cx))),
                )
            })
            .child(
                h().flex_1()
                    .h_full()
                    .items_center()
                    .justify_end()
                    .gap(px(10.))
                    .when(working > 0, |el| {
                        el.child(
                            h().id("working-button")
                                .gap(px(7.))
                                .px(px(8.))
                                .h(px(28.))
                                .rounded(px(6.))
                                .cursor_pointer()
                                .text_size(px(12.))
                                .text_color(c(theme::TEXT_2))
                                .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                                .tooltip(ui::tip("Open the dashboard: what is at work now", Some("secondary-1")))
                                .occlude()
                                .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.go_nav(NavItem::Dashboard, w, cx)))
                                .child(ui::pulse_dot("working-dot", 6., theme::ACCENT))
                                .child(format!("{working} working")),
                        )
                    })
                    .when_some(self.update_pill(), |el, label| {
                        el.child(
                            h().id("update-pill").gap(px(6.)).px(px(9.)).h(px(24.)).rounded(px(12.)).cursor_pointer()
                                .bg(c(theme::ACCENT)).text_color(c(theme::BG)).text_size(px(11.5)).font_weight(FontWeight::MEDIUM)
                                .tooltip(ui::tip("Updates", Some("secondary-shift-u")))
                                .occlude()
                                .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(OpenUpdates), cx)))
                                .child(label),
                        )
                    })
                    .child({
                        let today = self.usage.as_ref().and_then(|u| u.windows.first()).map_or(0, |w| w.input_tokens + w.output_tokens);
                        // Prefer the subscription's session limit; fall back to today's tokens.
                        let me = self.active_member().map(|u| u.id);
                        let session = crate::screens::account::own_subscription(self.subscription.as_ref(), me).and_then(|s| s.limits.first().cloned());
                        let label = match &session {
                            Some(l) => format!("Session {}%", l.percent),
                            None => format!("{} today", ui::fmt_tokens(today)),
                        };
                        let hot = session.as_ref().is_some_and(|l| l.percent >= 80);
                        h().id("usage-button")
                            .gap(px(6.))
                            .px(px(8.))
                            .h(px(28.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .text_size(px(12.))
                            .text_color(c(theme::TEXT_2))
                            .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                            .tooltip(ui::tip("Usage", Some("secondary-u")))
                            .occlude()
                            .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.on_open_usage(&OpenUsage, w, cx)))
                            .when(hot, |el| el.text_color(rgb(0xd99a3a)))
                            .child(ui::usage_glyph(if hot { 0xd99a3a } else { theme::TEXT_2 }))
                            .child(label)
                    })
                    .child(
                        ui::more_button("app-menu")
                            .aria_label("Menu")
                            .occlude()
                            .on_click(cx.listener(|this, e: &ClickEvent, w, cx| {
                                let items = this.app_menu();
                                this.open_menu(e.position(), items, w, cx);
                            })),
                    ),
            );
        div()
            .h(px(46.))
            .flex_none()
            .border_b_1()
            .border_color(c(theme::BORDER))
            .flex()
            .items_center()
            .child(bar)
            .when(cfg!(target_os = "windows"), |el| el.child(window_controls(window.is_maximized())))
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (dot, text): (u32, SharedString) = match &self.connection {
            Connection::Connecting => match self.remote_name() {
                Some(host) => (theme::MUTED, format!("Connecting to {host}…").into()),
                None => (theme::MUTED, "Connecting to the background service…".into()),
            },
            // Online: just the green dot; server and version live in Settings › Updates.
            Connection::Online { .. } => (theme::ACCENT, SharedString::default()),
            Connection::Offline(err) => (theme::NEGATIVE, format!("Offline: {err}. Reconnecting…").into()),
            Connection::Unpaired(reason) => (theme::NEGATIVE, format!("{reason} · Ctrl Alt S: servers").into()),
            Connection::NoServer => (theme::MUTED, "Not connected to a server · Ctrl Alt S: choose or pair one".into()),
            Connection::UpdateNeeded(reason) => (theme::NEGATIVE, format!("Update needed · {reason}").into()),
        };
        let job = self.running_jobs().next().map(|j| {
            format!("{} · {:.0}%", j.label, j.progress_permille as f32 / 10.)
        });
        h().h(px(26.))
            .flex_none()
            .px(px(14.))
            .gap(px(14.))
            .border_t_1()
            .border_color(c(theme::BORDER))
            .bg(c(theme::SIDEBAR))
            .text_size(px(11.5))
            .text_color(c(theme::MUTED))
            .child(h().gap(px(7.)).child(ui::dot(6., dot)).when(!text.is_empty(), |el| el.child(text)))
            .when_some(job, |el, job| el.child(h().gap(px(7.)).text_color(c(theme::TEXT_2)).child(ui::ring(7., theme::ACCENT)).child(job)))
            .when_some(self.power.clone(), |el, p| {
                let text = match p.deadline_ms {
                    Some(deadline) => {
                        let left = ((deadline - ui::now_ms()).max(0) + 999) / 1000;
                        format!("PC shuts down in {}:{:02}", left / 60, left % 60)
                    }
                    None => "PC shuts down when all work is done".to_string(),
                };
                el.child(
                    h().gap(px(8.)).text_color(c(theme::NEGATIVE))
                        .child(ui::dot(6., theme::NEGATIVE))
                        .child(text)
                        .child(
                            div().id("power-cancel").role(Role::Button).aria_label("Cancel the PC shutdown").cursor_pointer().text_color(c(theme::TEXT))
                                .underline().child("Cancel")
                                .on_click(cx.listener(|this, _: &ClickEvent, _, _| {
                                    let _ = this.requests.send(backend::Request::Power(backend::PowerCall::Cancel));
                                })),
                        ),
                )
            })
            .child(div().flex_1())
            .when_some(self.notice.as_ref(), |el, notice| {
                el.child(
                    ui::rise(
                        div()
                            .text_color(c(if notice.error { theme::NEGATIVE } else { theme::TEXT_2 }))
                            .child(notice.text.clone()),
                        ("notice", self.notice_epoch as usize),
                        0,
                    ),
                )
            })
            .when(self.inbox_count() > 0, |el| {
                el.child(h().gap(px(6.)).text_color(c(theme::ACCENT_TEXT)).child(ui::diamond(8., theme::ACCENT_TEXT, false)).child(ui::hint("status-inbox", "secondary-i", format!("{} waiting for you", self.inbox_count()), || Box::new(OpenInbox), cx).text_color(c(theme::ACCENT_TEXT))))
            })
            .child(div().flex_none().child(crate::screens::projects::clock(ui::now_ms())))
    }
}

/// The loaded subagents, when they belong to `conversation`.
pub fn subagents_of(conversation: ConversationId, loaded: Option<&(ConversationId, Vec<aurelius_protocol::AgentView>)>) -> Vec<aurelius_protocol::AgentView> {
    loaded.filter(|(c, _)| *c == conversation).map(|(_, agents)| agents.clone()).unwrap_or_default()
}

/// A stream delta shows a new step (of the answer or one of its subagents):
/// another run, or another activity mark than the delta before. Text alone is no step.
pub fn stream_stepped(before: Option<&aurelius_protocol::StreamDelta>, delta: &aurelius_protocol::StreamDelta) -> bool {
    before.is_none_or(|before| before.run_id != delta.run_id || backend::activity_mark(&before.activity) != backend::activity_mark(&delta.activity))
}

/// The subagents among what runs, that work in `conversation`.
pub fn subagents_in<'a>(agents: impl IntoIterator<Item = &'a aurelius_protocol::AgentView>, conversation: ConversationId) -> Vec<aurelius_protocol::AgentView> {
    agents.into_iter().filter(|a| a.kind == "subagent" && a.conversation_id == Some(conversation)).cloned().collect()
}

/// The memories "Consolidate" merges: the project's, and that of each
/// manager that has memories in it.
pub fn consolidation_scopes(project: ProjectId, items: &[aurelius_protocol::MemoryView]) -> Vec<aurelius_domain::memory::MemoryScope> {
    use aurelius_domain::memory::MemoryScope;
    let mut scopes = vec![MemoryScope::Project(project)];
    for manager in items.iter().filter(|m| m.scope == "employee").filter_map(|m| m.employee_id) {
        if !scopes.contains(&MemoryScope::Employee(manager)) {
            scopes.push(MemoryScope::Employee(manager));
        }
    }
    scopes
}

/// Re-render on each minute change so the status bar clock stays current,
/// sleeping until the next minute instead of waking every second.
fn tick_clock(cx: &mut Context<AppView>) {
    cx.spawn(async move |this, cx| loop {
        cx.background_executor().timer(until_next_minute(ui::now_ms())).await;
        if this.update(cx, |_, cx| cx.notify()).is_err() {
            break;
        }
    })
    .detach();
}

/// Re-render each second while a PC shutdown countdown runs.
fn tick_power(cx: &mut Context<AppView>) {
    cx.spawn(async move |this, cx| loop {
        cx.background_executor().timer(Duration::from_secs(1)).await;
        if this.update(cx, |this, cx| {
            if this.power.as_ref().is_some_and(|p| p.deadline_ms.is_some()) {
                cx.notify();
            }
        }).is_err() {
            break;
        }
    })
    .detach();
}

/// Time from `now_ms` to just past the next minute boundary.
fn until_next_minute(now_ms: i64) -> Duration {
    Duration::from_millis((60_000 - now_ms.rem_euclid(60_000)) as u64 + 50)
}

/// Keeps the subagent chip current: at once when another conversation opens,
/// after a step of its answer (its stream shows the steps; at most once a
/// second), once more when that answer ends.
fn poll_subagents(cx: &mut Context<AppView>) {
    cx.spawn(async move |this, cx| {
        let mut last: Option<(ConversationId, bool)> = None;
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let alive = this.update(cx, |this, _| {
                let Some(current) = this.chip_conversation() else {
                    last = None;
                    return;
                };
                let running = this.active_run().is_some() || !this.subagents_here().is_empty();
                let stepped = std::mem::take(&mut this.subagents_stepped);
                let due = match last {
                    None => true,
                    Some((c, was_running)) => c != current || stepped || (!running && was_running),
                };
                last = Some((current, running));
                if due {
                    let _ = this.requests.send(backend::Request::LoadSubagents(current));
                }
            });
            if alive.is_err() {
                break;
            }
        }
    })
    .detach();
}

/// Parts of the "next message uses" line under the composer.
pub struct NextRuntime {
    pub persona: Option<String>,
    pub connection: String,
    pub model: String,
}

#[cfg(test)]
mod tests {
    #[test]
    fn plan_feedback_quotes_fragments_then_free_text() {
        use super::{PlanComment, compose_plan_feedback};
        let c = vec![PlanComment { quote: "step one\nstep two".into(), text: "Split this".into() }];
        assert_eq!(compose_plan_feedback(&c, " also faster "), "> step one\n> step two\n\nSplit this\n\nalso faster");
        assert_eq!(compose_plan_feedback(&[], "  "), "");
        assert_eq!(compose_plan_feedback(&[], "x"), "x");
    }

    use super::{AppView, NeedsYou, work_of, until_next_minute, effective_run, is_image_file, needs_you_of, plan_headline_of, plan_text_of, save_pasted_image};
    use aurelius_domain::{ApprovalId, ConversationId, RunId, TaskId, approval::ApprovalStatus};
    use aurelius_protocol::{ApprovalView, NotificationView};
    use std::time::Duration;

    #[test]
    fn the_sidebar_lists_six_pages_and_settings_keeps_its_shortcut() {
        use super::{EmployeesTab, NavItem, Page, ProjectRef, ProjectsTab, Tab};
        let labels: Vec<&str> = NavItem::ALL.iter().map(|n| n.label()).collect();
        assert_eq!(labels, ["Dashboard", "Board", "Team chat", "Inbox", "Tasks", "Employees", "Projects", "Vault", "Settings"]);
        // Every entry's shortcut is bound in the shortcut table; Settings keeps Ctrl+9.
        for nav in NavItem::ALL {
            assert!(crate::keymap::SHORTCUTS.iter().any(|s| s.keys.contains(&nav.keys())), "{} has its shortcut", nav.label());
        }
        assert_eq!(NavItem::Settings.keys(), "secondary-9");
        assert_eq!(NavItem::Chat.keys(), "secondary-8", "Team chat took the free Ctrl+8");
        assert_eq!(NavItem::of(Page::GroupChat), NavItem::Chat);
        // Go Org chart / Ideas keep working: each opens a tab of Employees / Projects.
        for key in ["secondary-4", "secondary-0"] {
            assert!(crate::keymap::SHORTCUTS.iter().any(|s| s.keys.contains(&key)), "{key} is bound");
        }
        assert_eq!(NavItem::Board.keys(), "secondary-5", "D-02: the board took the freed Ctrl+5");
        assert_eq!(crate::keymap::SHORTCUTS.iter().filter(|s| s.keys.contains(&"secondary-5")).count(), 1, "only the board");
        assert_eq!(NavItem::of(Page::Board), NavItem::Board);
        assert_eq!(NavItem::of(Page::Project(ProjectRef::Sample(0), Tab::Tasks)), NavItem::Projects);
        assert_eq!(NavItem::of(Page::Inbox), NavItem::Inbox, "the inbox is a page, not a dialog");
        assert_eq!(NavItem::of(Page::Settings), NavItem::Settings);
        assert_eq!(EmployeesTab::ALL.map(|t| t.label()), ["List", "Org chart", "Guidelines"]);
        assert_eq!(ProjectsTab::ALL.map(|t| t.label()), ["Projects", "Ideas"]);
        assert_eq!(Tab::ALL.map(|t| t.title()), ["Overview", "Board", "Tasks", "Ideas", "Vault"], "memory is not a tab of the bar");
        assert_eq!(NavItem::of(Page::Vault), NavItem::Vault);
    }

    #[test]
    fn inbox_tabs_filter_and_count_what_they_show() {
        use super::{InboxTab, NoteKind, note_kind, tab_counts, tab_notes};
        let note = |id: &str, severity: &str, target: &str| NotificationView {
            id: id.into(),
            severity: severity.into(),
            title: String::new(),
            subject: String::new(),
            created_at_ms: 0,
            read: false,
            task_id: None,
            target_type: target.into(),
        };
        let notes = [
            note("ok", "info", "task.succeeded"),
            note("approval", "action", "approval.requested"),
            note("failed", "warning", "task.failed"),
            note("escalated", "action", "task.waiting"),
            note("proposal", "action", "employee.proposed"),
            note("sign-in", "warning", "connection.changed"),
            note("guideline", "action", "guideline.proposed"),
        ];
        let refs: Vec<&NotificationView> = notes.iter().collect();
        assert_eq!(note_kind(&notes[1]), NoteKind::Hidden, "the approval itself is listed, not its alert");
        assert_eq!(note_kind(&notes[6]), NoteKind::Hidden, "a guideline's proposal is its own row, not also its alert");
        let ids = |tab| tab_notes(tab, &refs).iter().map(|n| n.id.as_str()).collect::<Vec<_>>();
        assert_eq!(ids(InboxTab::NeedsYou), ["escalated"]);
        assert_eq!(ids(InboxTab::Results), ["ok", "failed", "sign-in"]);
        assert_eq!(ids(InboxTab::Failed), ["failed"]);
        assert_eq!(ids(InboxTab::All), ["escalated", "ok", "failed", "sign-in"], "what needs you comes first");
        // Two approvals or proposals waiting; hidden alerts count nowhere.
        assert_eq!(tab_counts(2, &refs), [3, 3, 1, 6]);
        assert_eq!(tab_counts(0, &[]), [0, 0, 0, 0]);
    }

    #[test]
    fn relative_age_reads_like_paperclip() {
        use crate::ui::{day_group, relative_age};
        let now = 1_800_000_000_000;
        assert_eq!(relative_age(now - 59_000, now), "just now");
        assert_eq!(relative_age(now + 5_000, now), "just now", "a clock slightly ahead is not in the future");
        assert_eq!(relative_age(now - 5 * 60_000, now), "5m ago");
        assert_eq!(relative_age(now - 2 * 3_600_000 - 1, now), "2h ago");
        assert_eq!(relative_age(now - 3 * 86_400_000, now), "3d ago");
        let real_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        assert_eq!(day_group(real_now), "TODAY");
        assert_eq!(day_group(real_now - 3 * 86_400_000), "EARLIER");
    }

    #[test]
    fn a_protocol_mismatch_means_update_needed_not_offline() {
        use aurelius_client::ClientError;
        assert!(crate::backend::update_needed(&ClientError::UpdateNeeded("Update needed: the server speaks protocol 2, this app 3. Update the server.".into())));
        assert!(!crate::backend::update_needed(&ClientError::Unreachable("connection refused".into())));
        assert!(!crate::backend::update_needed(&ClientError::Disconnected));
    }

    #[test]
    fn pins_keep_their_order_per_server_and_survive_a_reload() {
        use super::{pins_load, pins_save, pins_toggle};
        use aurelius_domain::ProjectId;
        let (a, b, c) = (ProjectId::new(), ProjectId::new(), ProjectId::new());
        let mut pins = Vec::new();
        assert!(pins_toggle(&mut pins, b));
        assert!(pins_toggle(&mut pins, a));
        assert!(pins_toggle(&mut pins, c));
        assert!(!pins_toggle(&mut pins, a));
        assert_eq!(pins, vec![b, c]);
        let mut prefs = serde_json::json!({ "sidebar_collapsed": true });
        pins_save(&mut prefs, "local", &pins);
        pins_save(&mut prefs, "host-2", &[a]);
        // A restart reads the same file back: each server has its own pins, other prefs stay.
        let prefs: serde_json::Value = serde_json::from_str(&prefs.to_string()).unwrap();
        assert_eq!(pins_load::<ProjectId>(&prefs, "local"), vec![b, c]);
        assert_eq!(pins_load::<ProjectId>(&prefs, "host-2"), vec![a]);
        assert!(pins_load::<ProjectId>(&prefs, "other").is_empty());
        assert_eq!(prefs["sidebar_collapsed"], true);
        assert!(pins_load::<ProjectId>(&serde_json::json!({}), "local").is_empty());
    }

    #[test]
    fn pinned_items_come_first_in_pin_order_then_the_rest_as_before() {
        use super::pinned_first;
        let items = vec!["a", "b", "c", "d", "e"];
        // c was pinned before a; b is not pinned; "gone" is no longer in the list.
        assert_eq!(pinned_first(items.clone(), &["c", "gone", "a"], |s| *s), vec!["c", "a", "b", "d", "e"]);
        assert_eq!(pinned_first(items.clone(), &[], |s| *s), items, "nothing pinned: unchanged");
    }

    #[test]
    fn employee_pins_have_their_own_key_and_the_same_limit() {
        use super::{pins_load, pins_save, pins_toggle};
        use aurelius_domain::{EmployeeId, ProjectId};
        let (e, p) = (EmployeeId::new(), ProjectId::new());
        let mut prefs = serde_json::json!({});
        pins_save(&mut prefs, "local", &[e]);
        pins_save(&mut prefs, "local", &[p]);
        assert_eq!(pins_load::<EmployeeId>(&prefs, "local"), vec![e]);
        assert_eq!(pins_load::<ProjectId>(&prefs, "local"), vec![p]);
        let mut pins = vec![e];
        assert!(!pins_toggle(&mut pins, e));
    }

    #[test]
    fn folded_sidebar_items_survive_a_reload() {
        use super::{folded_load, NavItem};
        assert_eq!(folded_load(&serde_json::json!({ "sidebar_folded": ["projects"] })), vec![NavItem::Projects]);
        assert!(folded_load(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn at_most_five_projects_can_be_pinned() {
        use super::{can_pin, pins_toggle, MAX_PINS};
        use aurelius_domain::ProjectId;
        let mut pins: Vec<ProjectId> = Vec::new();
        for _ in 0..MAX_PINS {
            let p = ProjectId::new();
            assert!(can_pin(&pins, p));
            pins_toggle(&mut pins, p);
        }
        assert!(!can_pin(&pins, ProjectId::new()), "the sixth is refused");
        assert!(can_pin(&pins, pins[0]), "a pinned one can always be unpinned");
    }

    #[test]
    fn work_of_prefers_task_work_then_conversation_then_colleagues() {
        use aurelius_protocol::AgentView;
        let a = |kind: &str, name: &str, detail: &str, since| AgentView { kind: kind.into(), name: name.into(), detail: detail.into(), since_ms: since, ..Default::default() };
        assert_eq!(work_of(&[]), None);
        assert_eq!(work_of(&[a("subagent", "Explore", "", 1)]), None);
        let agents = [a("delegated", "Bram", "Pricing · running", 1), a("conversation", "Roadmap chat", "", 2), a("planning", "Planning", "Landing page", 3)];
        assert_eq!(work_of(&agents), Some(("Landing page".into(), 3)));
        assert_eq!(work_of(&agents[..2]), Some(("Roadmap chat".into(), 2)));
        assert_eq!(work_of(&agents[..1]), Some(("Waiting on Bram: Pricing · running".into(), 1)));
    }

    #[test]
    fn the_clock_wakes_just_after_the_next_minute() {
        assert_eq!(until_next_minute(0), Duration::from_millis(60_050));
        assert_eq!(until_next_minute(59_990), Duration::from_millis(60));
        assert_eq!(until_next_minute(120_000 + 1_000), Duration::from_millis(59_050));
    }

    #[test]
    fn consolidate_covers_the_project_and_each_manager_once() {
        use aurelius_domain::memory::{MemoryKind, MemoryScope, MemoryStatus};
        use aurelius_domain::{EmployeeId, MemoryId, ProjectId};
        let (project, alex) = (ProjectId::new(), EmployeeId::new());
        let item = |scope: &str, manager: Option<EmployeeId>| aurelius_protocol::MemoryView {
            id: MemoryId::new(), scope: scope.into(), employee_id: manager, employee_name: None, kind: MemoryKind::Decision, status: MemoryStatus::Active,
            content: "x".into(), pinned: false, importance: 0.5, confidence: 0.5, created_at_ms: 0, supersedes_id: None, conflicts_with_id: None, decided_by: None, sources: vec![], lesson: None,
        };
        let items = [item("project", None), item("employee", Some(alex)), item("employee", Some(alex)), item("global", None)];
        assert_eq!(super::consolidation_scopes(project, &items), vec![MemoryScope::Project(project), MemoryScope::Employee(alex)]);
        assert_eq!(super::consolidation_scopes(project, &[]), vec![MemoryScope::Project(project)]);
    }

    #[test]
    fn the_subagent_chip_counts_only_subagents_of_the_open_conversation() {
        use aurelius_protocol::AgentView;
        let (here, other) = (ConversationId::new(), ConversationId::new());
        let agent = |kind: &str, name: &str, conversation| AgentView { kind: kind.into(), name: name.into(), conversation_id: Some(conversation), ..Default::default() };
        let agents = [agent("conversation", "Answering", here), agent("subagent", "Find the chip", here), agent("subagent", "Elsewhere", other), agent("worker", "Kai", here), agent("subagent", "Read docs", here)];
        let names: Vec<_> = super::subagents_in(&agents, here).into_iter().map(|a| a.name).collect();
        assert_eq!(names, ["Find the chip", "Read docs"]);
    }

    #[test]
    fn the_ideas_filter_tells_your_ideas_from_the_managers() {
        use super::IdeaAuthor;
        use aurelius_domain::{IdeaId, EmployeeId, ProjectId, idea::IdeaStatus};
        let idea = |author: &str, employee_id| aurelius_protocol::IdeaView {
            id: IdeaId::new(), project_id: ProjectId::new(), title: "t".into(), note: None, status: IdeaStatus::Open, kind: Default::default(),
            author: author.into(), employee_id, task_id: None, task_status: None, created_at_ms: 0, updated_at_ms: 0,
            attachments: Vec::new(),
        };
        let (yours, iris, from_old_server) = (idea("You", None), idea("Iris", Some(EmployeeId::new())), idea("Iris", None));
        let shown = |who: IdeaAuthor| [&yours, &iris, &from_old_server].into_iter().filter(|i| who.admits(i)).count();
        assert_eq!((shown(IdeaAuthor::Anyone), shown(IdeaAuthor::You), shown(IdeaAuthor::Managers)), (3, 1, 2));
        assert_eq!(IdeaAuthor::Managers.next(), IdeaAuthor::Anyone, "key 4 cycles back to everyone");
    }

    #[test]
    fn the_kind_filter_keeps_only_that_kind() {
        use aurelius_domain::idea::IdeaKind;
        assert_eq!(serde_json::from_str::<IdeaKind>("\"bug\"").unwrap(), IdeaKind::Bug);
        let wanted = Some(IdeaKind::Bug);
        assert!(wanted.is_none_or(|k| IdeaKind::Bug == k) && !wanted.is_none_or(|k| IdeaKind::Feature == k));
    }

    #[test]
    fn ideas_group_into_yours_first_then_the_managers() {
        use super::{IdeaAuthor, split_ideas};
        use aurelius_domain::{IdeaId, EmployeeId, ProjectId, idea::IdeaStatus};
        let idea = |title: &str, author: &str, employee_id| aurelius_protocol::IdeaView {
            id: IdeaId::new(), project_id: ProjectId::new(), title: title.into(), note: None, status: IdeaStatus::Open, kind: Default::default(),
            author: author.into(), employee_id, task_id: None, task_status: None, created_at_ms: 0, updated_at_ms: 0,
            attachments: Vec::new(),
        };
        let list = [idea("a", "Iris", Some(EmployeeId::new())), idea("b", "You", None), idea("c", "Kai", None), idea("d", "You", None)];
        let titles = |v: Vec<&aurelius_protocol::IdeaView>| v.into_iter().map(|i| i.title.clone()).collect::<Vec<_>>();
        let (yours, managers) = split_ideas(list.iter());
        assert_eq!((titles(yours), titles(managers)), (vec!["b".to_string(), "d".into()], vec!["a".to_string(), "c".into()]));
        // A filter leaves one section; the other stays empty and is hidden.
        let (yours, managers) = split_ideas(list.iter().filter(|i| IdeaAuthor::Managers.admits(i)));
        assert!(yours.is_empty());
        assert_eq!(managers.len(), 2);
        let (yours, managers) = split_ideas(list.iter().filter(|i| IdeaAuthor::You.admits(i)));
        assert_eq!((yours.len(), managers.len()), (2, 0));
    }

    #[test]
    fn the_subagent_chip_shows_only_the_open_conversations_subagents() {
        use super::subagents_of;
        use aurelius_protocol::AgentView;
        let (chat, thread, other) = (ConversationId::new(), ConversationId::new(), ConversationId::new());
        let agent = AgentView { kind: "subagent".into(), name: "Find".into(), conversation_id: Some(thread), ..Default::default() };
        let loaded = (thread, vec![agent]);
        assert_eq!(subagents_of(thread, Some(&loaded)).len(), 1);
        assert!(subagents_of(chat, Some(&loaded)).is_empty());
        assert!(subagents_of(other, None).is_empty());
    }

    #[test]
    fn a_step_in_the_stream_reloads_the_chip_but_text_alone_does_not() {
        use super::stream_stepped;
        use aurelius_domain::MessageId;
        use aurelius_protocol::{ActivityView, StreamDelta};
        let step = |call: &str, ok: Option<bool>| ActivityView { call_id: call.into(), tool: "Read".into(), input: serde_json::Value::Null, output: None, ok, offset: 0, truncated: false, output_bytes: None };
        let first = StreamDelta { run_id: RunId::new(), conversation_id: ConversationId::new(), message_id: MessageId::new(), text: "Look".into(), activity: vec![step("agent", None)], progress: None, activity_changes: false, mode: None };
        assert!(stream_stepped(None, &first), "the first delta of a conversation counts");
        let more_text = StreamDelta { text: "Looking at it".into(), ..first.clone() };
        assert!(!stream_stepped(Some(&first), &more_text), "text alone is no step");
        let subagent_step = StreamDelta { activity: vec![step("agent", None), step("sub-read", None)], ..more_text.clone() };
        assert!(stream_stepped(Some(&more_text), &subagent_step), "a subagent started a step");
        let finished = StreamDelta { activity: vec![step("agent", None), step("sub-read", Some(true))], ..subagent_step.clone() };
        assert!(stream_stepped(Some(&subagent_step), &finished), "a step finished");
        let next_run = StreamDelta { run_id: RunId::new(), ..finished.clone() };
        assert!(stream_stepped(Some(&finished), &next_run), "another answer");
    }

    #[test]
    fn pasted_images_become_files_and_only_images_are_taken() {
        assert!(is_image_file(std::path::Path::new(r"C:\shots\Scherm.PNG")));
        assert!(!is_image_file(std::path::Path::new("notes.pdf")));
        let pasted = gpui_kit::Image { format: gpui_kit::ImageFormat::Png, bytes: vec![1, 2, 3], id: 7 };
        let path = save_pasted_image(&pasted).unwrap();
        assert_eq!(path.file_name().unwrap(), "Pasted image.png");
        assert_eq!(std::fs::read(&path).unwrap(), vec![1, 2, 3]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        let svg = gpui_kit::Image { format: gpui_kit::ImageFormat::Svg, bytes: vec![], id: 8 };
        assert!(save_pasted_image(&svg).is_err(), "the server can't read SVG");
    }

    fn plan_approval(arguments: serde_json::Value) -> ApprovalView {
        ApprovalView {
            id: ApprovalId::new(),
            status: ApprovalStatus::Pending,
            tool: super::PLAN_TOOL.into(),
            effect: "read".into(),
            arguments,
            resources: vec![],
            account: String::new(),
            reasons: vec![],
            action_digest: String::new(),
            requested_at_ms: 0,
            expires_at_ms: 0,
            run_id: RunId::new(),
            conversation_id: None,
            requester: "Alex".into(),
            task_id: None,
            always_allow_eligible: false,
        }
    }

    #[test]
    fn a_plan_approval_carries_its_markdown_and_a_headline() {
        let a = plan_approval(serde_json::json!({ "plan": "
## Create hello.txt

1. Write the file", "planFilePath": "C:/p.md" }));
        assert_eq!(plan_text_of(&a), "
## Create hello.txt

1. Write the file");
        assert_eq!(plan_headline_of(&a), "Create hello.txt");
        let empty = plan_approval(serde_json::json!({}));
        assert_eq!(plan_text_of(&empty), "");
        assert_eq!(plan_headline_of(&empty), "A plan");
    }

    fn task(conversation: ConversationId, status: &str, waiting_reason: Option<&str>) -> aurelius_protocol::TaskView {
        serde_json::from_value(serde_json::json!({
            "id": TaskId::new(), "project_id": aurelius_domain::ProjectId::new(), "employee_id": null, "employee_name": null,
            "title": "T", "goal": "", "criteria": [], "status": status, "waiting_reason": waiting_reason,
            "created_at_ms": 0, "updated_at_ms": 0, "result": null, "depends_on": [], "pending_approvals": 0, "parent_task_id": null,
            "delegated_by": null, "children": [], "handoffs": [], "run": {}, "attachments": [], "schedule_id": null, "function_id": null,
            "conversation_id": conversation,
        }))
        .unwrap()
    }

    #[test]
    fn a_tickets_main_task_is_given_and_cancelled_on_the_board() {
        use super::{is_ticket_main_task, use_the_board};
        let mut t = task(ConversationId::new(), "queued", None);
        assert!(!is_ticket_main_task(&t) && use_the_board(&t).is_none(), "a plain task: Cancel task and Give to… stay");
        t.ticket_id = Some(aurelius_domain::TicketId::new());
        t.ticket_number = Some(7);
        assert!(is_ticket_main_task(&t), "F-m1: the server refuses task.assign/task.cancel here");
        assert_eq!(use_the_board(&t).as_deref(), Some("This is the main task of ticket #7: assign or cancel it on the Board."));
        t.parent_task_id = Some(TaskId::new());
        assert!(!is_ticket_main_task(&t) && use_the_board(&t).is_none(), "a delegation below it carries the ticket but is a task like any other");
    }

    #[test]
    fn a_finished_task_with_a_live_run_is_working_or_queued_and_active() {
        use crate::ui::{is_active, TaskMark};
        use aurelius_domain::views::LiveRun;
        let mut t = task(ConversationId::new(), "succeeded", None);
        assert_eq!((TaskMark::of(&t), is_active(&t)), (TaskMark::Done, false));
        t.live_run = Some(LiveRun::Running);
        assert_eq!((TaskMark::of(&t), is_active(&t)), (TaskMark::Working, true));
        assert_eq!(TaskMark::of_live(&t, true), TaskMark::Compacting);
        t.live_run = Some(LiveRun::Queued);
        assert_eq!((TaskMark::of(&t), is_active(&t)), (TaskMark::Queued, true));
        let mut failed = task(ConversationId::new(), "failed", None);
        failed.live_run = Some(LiveRun::Running);
        assert_eq!(TaskMark::of(&failed), TaskMark::Working);
        failed.live_run = None;
        assert_eq!((TaskMark::of(&failed), is_active(&failed)), (TaskMark::Failed, false));
        assert!(is_active(&task(ConversationId::new(), "running", None)));
    }

    #[test]
    fn a_working_task_that_compacts_says_so() {
        use crate::ui::TaskMark;
        let running = task(ConversationId::new(), "running", None);
        assert_eq!(TaskMark::of_live(&running, true), TaskMark::Compacting);
        assert_eq!(TaskMark::of_live(&running, false), TaskMark::Working);
        assert_eq!(TaskMark::of_live(&task(ConversationId::new(), "failed", None), true), TaskMark::Failed);
        let old_server: aurelius_protocol::RunProgress = serde_json::from_value(serde_json::json!({ "started_at_ms": 1, "output_tokens": 2 })).unwrap();
        assert!(!old_server.compacting, "older servers leave the field out");
        assert!(crate::ui::fmt_run(&aurelius_protocol::RunProgress { compacting: true, ..old_server }).starts_with("Context comprimeren…"));
    }

    #[test]
    fn a_quota_wait_is_a_calm_wait() {
        let chat = ConversationId::new();
        let quota = task(chat, "waiting", Some("quota"));
        assert_eq!(crate::ui::TaskMark::of(&quota), crate::ui::TaskMark::LimitReset(None));
        assert_eq!(crate::ui::TaskMark::LimitReset(None).label(), "Waiting for limit reset");
        assert!(!crate::ui::TaskMark::LimitReset(None).is_finished());
        let soon = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        assert!(crate::ui::limit_wait(Some(soon)).starts_with("Waiting for limit reset until "));
        assert!(crate::ui::limit_wait(Some(soon + 3 * 86_400_000)).len() > crate::ui::limit_wait(Some(soon)).len(), "another day adds the day");
        assert_eq!(needs_you_of(&[], &[quota], chat), None);
        assert_eq!(crate::ui::TaskMark::of(&task(chat, "waiting", Some("dependency"))), crate::ui::TaskMark::Waiting);
    }

    #[test]
    fn a_conversation_shows_what_it_waits_for() {
        let chat = ConversationId::new();
        let tool = |name: &str| ApprovalView { tool: name.into(), conversation_id: Some(chat), ..plan_approval(serde_json::json!({})) };
        assert_eq!(needs_you_of(&[], &[], chat), None);
        assert_eq!(needs_you_of(&[tool("fs.write")], &[], chat), Some(NeedsYou::Approve));
        assert_eq!(needs_you_of(&[tool("fs.write"), tool(super::PLAN_TOOL)], &[], chat), Some(NeedsYou::Plan));
        assert_eq!(needs_you_of(&[tool(super::PLAN_TOOL), tool(super::QUESTION_TOOL)], &[], chat), Some(NeedsYou::Answer));
        let decided = ApprovalView { status: ApprovalStatus::Approved, ..tool("fs.write") };
        assert_eq!(needs_you_of(&[decided], &[], chat), None);
        assert_eq!(needs_you_of(&[tool("fs.write")], &[], ConversationId::new()), None, "another conversation's approval");

        // A worker's approval has no conversation of its own: it counts for its task's.
        let t = task(chat, "running", None);
        let worker = ApprovalView { conversation_id: None, task_id: Some(t.id), ..tool("fs.write") };
        assert_eq!(needs_you_of(&[worker], &[t], chat), Some(NeedsYou::Approve));

        assert_eq!(needs_you_of(&[], &[task(chat, "waiting", Some("input"))], chat), Some(NeedsYou::Decide));
        assert_eq!(needs_you_of(&[], &[task(chat, "waiting", Some("dependency"))], chat), None);
        assert_eq!(needs_you_of(&[], &[task(chat, "review", None)], chat), None, "the manager's review, not the user's");
    }

    #[test]
    fn queued_tasks_that_do_not_start_now_are_waiting() {
        use crate::ui::{is_waiting, queued_wait, TaskMark};
        let chat = ConversationId::new();
        let now = task(chat, "queued", None);
        assert!(!is_waiting(&now));
        assert_eq!((queued_wait(&now), TaskMark::of(&now)), (None, TaskMark::Queued), "about to start");
        let line = task(chat, "queued", Some("in_line"));
        assert!(is_waiting(&line));
        assert_eq!((queued_wait(&line).as_deref(), TaskMark::of(&line)), (Some("In line"), TaskMark::InLine));
        let mut later = task(chat, "queued", Some("scheduled"));
        assert_eq!((queued_wait(&later).as_deref(), TaskMark::of(&later)), (Some("Scheduled"), TaskMark::Scheduled));
        later.starts_at_ms = Some(0);
        assert!(queued_wait(&later).unwrap().starts_with("Scheduled · starts "));
        assert!(is_waiting(&task(chat, "waiting", Some("dependency"))));
        assert_eq!(queued_wait(&task(chat, "waiting", Some("input"))), None);
    }

    #[test]
    fn friendly_model_names() {
        assert_eq!(AppView::friendly_model("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(AppView::friendly_model("claude-sonnet-5-5"), "Sonnet 5.5");
        assert_eq!(AppView::friendly_model("claude-haiku-4-5-20251001"), "Haiku 4.5");
        assert_eq!(AppView::friendly_model("opus"), "Opus 5.5");
        assert_eq!(AppView::friendly_model("gpt-6-astra"), "GPT-6 Astra");
        assert_eq!(AppView::friendly_model("fake-deterministic-1"), "Test runtime");
    }

    #[test]
    fn manager_defaults_resolve_with_their_origin() {
        use aurelius_domain::connection::RunDefaults;
        use aurelius_domain::employee::Setting;
        let id = aurelius_domain::ProviderId::new();
        let conn = aurelius_protocol::ProviderView {
            id,
            provider: "claude_code".into(),
            display_name: "Claude Code".into(),
            auth_kind: "subscription".into(),
            billing: "subscription".into(),
            status: "ready".into(),
            has_secret: false,
            runtime_path: None,
            default_model: Some("claude-opus-5-5".into()),
            models: vec!["claude-opus-5-5".into(), "claude-sonnet-5-5".into()],
            detail: None,
            last_verified_ms: None,
        };
        let global = RunDefaults { connection: Setting::Value(id), effort: Setting::Value("high".into()), ..Default::default() };
        let inherit = RunDefaults::default();
        assert_eq!(effective_run(&[("manager", inherit), ("global default", global.clone())], std::slice::from_ref(&conn)), "Opus 5.5 · High · global default");
        let own = RunDefaults { connection: Setting::Value(id), model: Setting::Value("claude-sonnet-5-5".into()), effort: Setting::Value("medium".into()), ..Default::default() };
        assert_eq!(effective_run(&[("manager", own), ("global default", global.clone())], std::slice::from_ref(&conn)), "Sonnet 5.5 · Medium · manager");
        let model_only = RunDefaults { connection: Setting::Value(id), model: Setting::Value("claude-sonnet-5-5".into()), ..Default::default() };
        assert_eq!(effective_run(&[("manager", model_only), ("global default", global.clone())],std::slice::from_ref(&conn)), "Sonnet 5.5 (manager) · High (global default)");
        assert_eq!(effective_run(&[], &[conn.clone()]), "No connection configured");
        // Delegated tasks: unset fields follow the manager's chain behind the task layers.
        let tasks_effort = RunDefaults { effort: Setting::Value("low".into()), ..Default::default() };
        assert_eq!(
            effective_run(&[("set here", RunDefaults::default()), ("global tasks default", tasks_effort), ("global default", global)], std::slice::from_ref(&conn)),
            "Opus 5.5 (global default) · Low (global tasks default)"
        );
    }

    #[test]
    fn subagents_fall_back_to_claude_codes_default() {
        use super::effective_subagent_run;
        use aurelius_domain::connection::RunDefaults;
        use aurelius_domain::employee::Setting;
        assert_eq!(effective_subagent_run(&[("set here", RunDefaults::default())]), "Sonnet · Claude Code default");
        let global = RunDefaults { model: Setting::Value("claude-haiku-4-5-20251001".into()), ..Default::default() };
        // A stored effort is not shown: Claude Code runs subagents at the manager's.
        let own = RunDefaults { effort: Setting::Value("medium".into()), ..Default::default() };
        assert_eq!(effective_subagent_run(&[("set here", own), ("global subagent default", global)]), format!("{} · global subagent default", super::AppView::friendly_model("claude-haiku-4-5-20251001")));
    }
}

/// Minimize / maximize-restore / close in the app's own style. On Windows the
/// OS handles the clicks through the control areas (snap layouts keep working).
fn window_controls(maximized: bool) -> impl IntoElement {
    let button = |id: &'static str, area: WindowControlArea, close: bool| {
        div()
            .id(id)
            .w(px(46.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(c(theme::TEXT))
            .hover(move |s| if close { s.bg(rgb(0xc42b1c)).text_color(rgb(0xffffff)) } else { s.bg(c(theme::SURFACE_2)) })
            .window_control_area(area)
    };
    h().h_full()
        .flex_none()
        .child(button("win-min", WindowControlArea::Min, false).child(div().w(px(10.)).h(px(1.)).bg(c(theme::TEXT))))
        .child(button("win-max", WindowControlArea::Max, false).child(if maximized {
            div().relative().size(px(10.))
                .child(div().absolute().top(px(0.)).left(px(2.)).size(px(8.)).border_1().border_color(c(theme::TEXT)))
                .child(div().absolute().top(px(2.)).left(px(0.)).size(px(8.)).border_1().border_color(c(theme::TEXT)).bg(c(theme::BG)))
        } else {
            div().size(px(10.)).border_1().border_color(c(theme::TEXT))
        }))
        .child(button("win-close", WindowControlArea::Close, true).text_size(px(13.)).child("✕"))
}

/// "every 30 min", "hourly", "every 4 h".
pub fn heartbeat_label(minutes: u32) -> String {
    match minutes {
        60 => "hourly".into(),
        m if m % 60 == 0 => format!("every {} h", m / 60),
        m => format!("every {m} min"),
    }
}


/// Small per-user UI preferences (not app data): `<data dir>/ui.json`.
fn ui_prefs_path() -> Option<std::path::PathBuf> {
    aurelius_host::data_dir().ok().map(|d| d.join("ui.json"))
}

pub(crate) fn ui_prefs() -> serde_json::Value {
    ui_prefs_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}))
}

fn ui_pref(key: &str) -> bool {
    ui_prefs()[key].as_bool().unwrap_or(false)
}

fn set_ui_pref(key: &str, value: bool) {
    set_ui_value(key, serde_json::json!(value));
}

pub(crate) fn set_ui_value(key: &str, value: serde_json::Value) {
    if let Some(path) = ui_prefs_path() {
        let mut prefs = ui_prefs();
        prefs[key] = value;
        let _ = std::fs::write(path, prefs.to_string());
    }
}

/// What can be pinned in the sidebar: it has its own key in ui.json (pins are kept per server under it).
pub trait Pinnable: Copy + PartialEq + std::fmt::Display {
    const KEY: &'static str;
    fn parse_id(text: &str) -> Option<Self>;
}

impl Pinnable for ProjectId {
    const KEY: &'static str = "pinned_projects";
    fn parse_id(text: &str) -> Option<Self> {
        ProjectId::parse(text).ok()
    }
}

impl Pinnable for EmployeeId {
    const KEY: &'static str = "pinned_employees";
    fn parse_id(text: &str) -> Option<Self> {
        EmployeeId::parse(text).ok()
    }
}

/// The pinned items of one server, in pin order, from ui.json (`pinned_projects`, `pinned_employees`).
fn pins_load<T: Pinnable>(prefs: &serde_json::Value, server: &str) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for id in prefs[T::KEY][server].as_array().into_iter().flatten().filter_map(|v| v.as_str()).filter_map(T::parse_id) {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

fn pins_save<T: Pinnable>(prefs: &mut serde_json::Value, server: &str, pins: &[T]) {
    if !prefs[T::KEY].is_object() {
        prefs[T::KEY] = serde_json::json!({});
    }
    prefs[T::KEY][server] = serde_json::json!(pins.iter().map(ToString::to_string).collect::<Vec<_>>());
}

/// The sidebar shows at most this many pinned projects, and as many pinned employees.
pub const MAX_PINS: usize = 5;

/// Whether `id` may be toggled: unpinning always, pinning while there is room.
fn can_pin<T: PartialEq>(pins: &[T], id: T) -> bool {
    pins.contains(&id) || pins.len() < MAX_PINS
}

/// Pin at the end, or unpin. Returns whether the item is pinned now.
fn pins_toggle<T: PartialEq>(pins: &mut Vec<T>, id: T) -> bool {
    match pins.iter().position(|p| *p == id) {
        Some(i) => {
            pins.remove(i);
            false
        }
        None => {
            pins.push(id);
            true
        }
    }
}

/// `items` with the pinned ones first in pin order, then the rest in their own order.
pub(crate) fn pinned_first<T, I: PartialEq>(items: Vec<T>, pins: &[I], id: impl Fn(&T) -> I) -> Vec<T> {
    let (mut pinned, rest): (Vec<T>, Vec<T>) = items.into_iter().partition(|t| pins.contains(&id(t)));
    pinned.sort_by_key(|t| pins.iter().position(|p| *p == id(t)));
    pinned.extend(rest);
    pinned
}

/// The folded sidebar items from ui.json's `sidebar_folded`.
fn folded_load(prefs: &serde_json::Value) -> Vec<NavItem> {
    [NavItem::Employees, NavItem::Projects]
        .into_iter()
        .filter(|n| prefs["sidebar_folded"].as_array().into_iter().flatten().any(|v| v.as_str() == Some(n.fold_key())))
        .collect()
}

/// Where the user is.
#[derive(Clone, Copy, PartialEq)]
struct Place {
    screen: Page,
}

/// Model ids (and Claude Code aliases) with their current version and a
/// one-line description, as in Claude Desktop's model picker.
pub const MODEL_CATALOG: &[(&str, &str, &str)] = &[
    ("opus|claude-opus-5-5", "Opus 5.5", "Most capable, for complex work"),
    ("sonnet|claude-sonnet-5-5", "Sonnet 5.5", "Smart and fast, for everyday work"),
    ("fable|claude-fable-5-1", "Fable 5.1", ""),
    ("haiku|claude-haiku-4-5|claude-haiku-4-5-20251001", "Haiku 4.5", "Fastest, for quick answers"),
];

/// Claude Code effort levels: (value, label, description).
/// "09:30" (or "9:30") as minutes since midnight.
pub fn parse_clock(text: &str) -> Option<u32> {
    let (h, m) = text.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Minutes since midnight as "09:30".
pub fn format_clock(minutes: u32) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

const WEEKDAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// A recurrence in words: "Every 6 hours", "Every day at 09:00", "Every weekday at 09:00",
/// "Every Mon, Wed at 08:30" (server local time).
pub fn recurrence_label(recurrence: &aurelius_domain::schedule::Recurrence) -> String {
    use aurelius_domain::schedule::Recurrence;
    match recurrence {
        Recurrence::EveryHours { hours: 1 } => "Every hour".into(),
        Recurrence::EveryHours { hours } => format!("Every {hours} hours"),
        Recurrence::Daily { at_minute } => format!("Every day at {}", format_clock(*at_minute)),
        Recurrence::Weekly { weekdays, at_minute } => {
            let mut days: Vec<u8> = weekdays.iter().copied().filter(|d| *d < 7).collect();
            days.sort_unstable();
            days.dedup();
            let at = format_clock(*at_minute);
            match days.as_slice() {
                [0, 1, 2, 3, 4] => format!("Every weekday at {at}"),
                [5, 6] => format!("Every weekend at {at}"),
                [0, 1, 2, 3, 4, 5, 6] => format!("Every day at {at}"),
                _ => format!("Every {} at {at}", days.iter().map(|d| WEEKDAY_NAMES[*d as usize]).collect::<Vec<_>>().join(", ")),
            }
        }
    }
}

/// A moment for the recurring list: "today 09:00", "tomorrow 09:00", "Mon 12 Oct 09:00"
/// (this computer's time zone; `now_ms` is the reference).
pub fn when_label(ms: i64, now_ms: i64) -> String {
    let offset = aurelius_host::utc_offset_minutes_at(ms) * 60_000;
    let day = (ms + offset).div_euclid(86_400_000);
    let today = (now_ms + aurelius_host::utc_offset_minutes_at(now_ms) * 60_000).div_euclid(86_400_000);
    let minutes = ((ms + offset).rem_euclid(86_400_000) / 60_000) as u32;
    let clock = format_clock(minutes);
    match day - today {
        0 => format!("today {clock}"),
        1 => format!("tomorrow {clock}"),
        -1 => format!("yesterday {clock}"),
        _ => {
            // Civil date from days since 1970-01-01 (proleptic Gregorian).
            let z = day + 719_468;
            let era = z.div_euclid(146_097);
            let doe = z.rem_euclid(146_097);
            let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let date = doy - (153 * mp + 2) / 5 + 1;
            let month = if mp < 10 { mp + 3 } else { mp - 9 };
            let _ = era;
            const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            let weekday = WEEKDAY_NAMES[(day + 3).rem_euclid(7) as usize];
            format!("{weekday} {date} {} {clock}", MONTHS[(month - 1) as usize])
        }
    }
}

pub const EFFORTS: [(&str, &str, &str); 5] = [
    ("low", "Low", "Fastest answers"),
    ("medium", "Medium", "Balanced"),
    ("high", "High", "Thinks longer"),
    ("xhigh", "Extra high", "For hard problems"),
    ("max", "Max", "Deepest reasoning, slowest"),
];

/// "xhigh" → "Extra high"; an unknown level as it is.
fn effort_label(level: &str) -> String {
    EFFORTS.iter().find(|(l, _, _)| *l == level).map_or_else(|| level.to_string(), |(_, label, _)| (*label).to_string())
}

/// What a run uses given its layers of run defaults (most specific first), resolved
/// like the daemon does: "Sonnet 5.5 · Medium · manager", or with the origin per part
/// when model and effort come from different layers.
fn effective_run(layers: &[(&str, aurelius_domain::connection::RunDefaults)], connections: &[aurelius_protocol::ProviderView]) -> String {
    use aurelius_domain::employee::Setting;
    let mut connection = None;
    for (origin, settings) in layers {
        match settings.connection {
            Setting::Inherit => continue,
            Setting::Disabled => return format!("No connection ({origin})"),
            Setting::Value(id) => {
                connection = Some((id, *origin));
                break;
            }
        }
    }
    let Some((conn, conn_origin)) = connection.and_then(|(id, o)| connections.iter().find(|c| c.id == id).map(|c| (c, o))) else {
        return "No connection configured".into();
    };
    let (model, model_origin) = layers
        .iter()
        .find_map(|(o, s)| match &s.model {
            Setting::Value(m) => Some((AppView::friendly_model(m), *o)),
            _ => None,
        })
        .unwrap_or_else(|| (conn.default_model.as_deref().map_or_else(|| "Default model".into(), AppView::friendly_model), conn_origin));
    run_line(&model, model_origin, layer_effort(layers))
}

/// What Claude subagents use given their layers (most specific first): unset, the
/// model is Claude Code's subagent default. Their effort is the manager's, so not shown.
fn effective_subagent_run(layers: &[(&str, aurelius_domain::connection::RunDefaults)]) -> String {
    use aurelius_domain::employee::Setting;
    let (model, model_origin) = layers
        .iter()
        .find_map(|(o, s)| match &s.model {
            Setting::Value(m) => Some((AppView::friendly_model(m), *o)),
            _ => None,
        })
        .unwrap_or_else(|| ("Sonnet".into(), "Claude Code default"));
    run_line(&model, model_origin, None)
}

/// The first effort set in these layers, with its origin.
fn layer_effort<'a>(layers: &[(&'a str, aurelius_domain::connection::RunDefaults)]) -> Option<(String, &'a str)> {
    use aurelius_domain::employee::Setting;
    layers.iter().find_map(|(o, s)| match &s.effort {
        Setting::Value(e) => Some((effort_label(e), *o)),
        _ => None,
    })
}

/// "Sonnet 5.5 · Medium · manager", or with the origin per part when they differ.
fn run_line(model: &str, model_origin: &str, effort: Option<(String, &str)>) -> String {
    match effort {
        None => format!("{model} · {model_origin}"),
        Some((effort, o)) if o == model_origin => format!("{model} · {effort} · {o}"),
        Some((effort, o)) => format!("{model} ({model_origin}) · {effort} ({o})"),
    }
}

/// Apply one backend's updates while it is the current one.
fn listen(mut updates: mpsc::UnboundedReceiver<Update>, epoch: u64, cx: &mut Context<AppView>) {
    cx.spawn(async move |this, cx| {
        while let Some(update) = updates.recv().await {
            let applied = this.update(cx, |this, cx| {
                if this.backend_epoch == epoch {
                    this.apply(update, cx);
                }
            });
            if applied.is_err() {
                break;
            }
        }
    })
    .detach();
}

/// Images the server reads (it scales them down and re-encodes them).
const IMAGE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];

fn is_image_file(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Add files on this computer to `list` (path, size) for a task, idea or ticket: no
/// duplicates, no folders, each at most `UPLOAD_MAX_BYTES`, at most `UPLOADS_MAX`
/// with the `kept` ones. Returns what went wrong, in words; the server checks again.
pub fn take_files(list: &mut Vec<(std::path::PathBuf, u64)>, paths: Vec<std::path::PathBuf>, kept: usize) -> Vec<String> {
    let mut problems = Vec::new();
    for path in paths {
        if list.iter().any(|(p, _)| *p == path) {
            continue;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
        let size = match std::fs::metadata(&path) {
            Ok(m) if m.is_file() => m.len(),
            Ok(_) => {
                problems.push(format!("{name} is a folder; add files only."));
                continue;
            }
            Err(e) => {
                problems.push(format!("Could not read {name}: {e}"));
                continue;
            }
        };
        if size > aurelius_domain::conversation::UPLOAD_MAX_BYTES {
            problems.push(format!("{name} is {}; files can be at most {} MB.", file_size(size), aurelius_domain::conversation::UPLOAD_MAX_BYTES / 1_048_576));
            continue;
        }
        if list.len() + kept >= aurelius_domain::conversation::UPLOADS_MAX {
            problems.push(format!("At most {} files.", aurelius_domain::conversation::UPLOADS_MAX));
            break;
        }
        list.push((path, size));
    }
    problems
}

/// "12 KB", "3.4 MB".
pub fn file_size(bytes: u64) -> String {
    if bytes >= 1_048_576 { format!("{:.1} MB", bytes as f64 / 1_048_576.0) } else { format!("{} KB", bytes.div_ceil(1024).max(1)) }
}

/// A pasted image as a file, so it travels like a picked one. Each paste gets
/// its own folder (the name stays readable); folders older than a day go.
fn save_pasted_image(image: &Image) -> Result<std::path::PathBuf, String> {
    if !matches!(image.format, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::Webp | ImageFormat::Bmp) {
        return Err(format!("This kind of image ({}) can't be sent; use PNG, JPEG, GIF, WebP or BMP.", image.format.extension()));
    }
    let root = std::env::temp_dir().join("aurelius-paste");
    if let Ok(old) = std::fs::read_dir(&root) {
        let day = Duration::from_secs(24 * 3600);
        for entry in old.flatten() {
            let stale = entry.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > day);
            if stale {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let dir = root.join(format!("{millis}-{}", image.id));
    let path = dir.join(format!("Pasted image.{}", image.format.extension()));
    std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, &image.bytes)).map_err(|e| format!("Could not keep the pasted image: {e}"))?;
    Ok(path)
}

#[cfg(test)]
mod file_tests {
    use super::take_files;

    /// The limits a task's files have, also for a ticket's (MU1-07): no folders,
    /// no duplicates, at most `UPLOADS_MAX` with the kept ones.
    #[test]
    fn files_are_taken_once_without_folders_and_within_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = |name: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, b"notes").unwrap();
            path
        };
        let mut list = Vec::new();
        let problems = take_files(&mut list, vec![file("spec.md"), file("spec.md"), dir.path().to_path_buf(), dir.path().join("gone.md")], 0);
        assert_eq!(list.iter().map(|(p, size)| (p.file_name().unwrap().to_string_lossy().into_owned(), *size)).collect::<Vec<_>>(), [("spec.md".to_string(), 5)]);
        assert_eq!(problems.len(), 2, "a folder and a missing file: {problems:?}");
        assert!(problems[0].ends_with("is a folder; add files only."));
        let max = aurelius_domain::conversation::UPLOADS_MAX;
        let more: Vec<_> = (0..max).map(|i| file(&format!("f{i}.txt"))).collect();
        let problems = take_files(&mut list, more, 1);
        assert_eq!(list.len() + 1, max, "with the kept one");
        assert_eq!(problems, [format!("At most {max} files.")]);
    }
}

#[cfg(test)]
mod schedule_tests {
    use super::{format_clock, parse_clock, recurrence_label};
    use aurelius_domain::schedule::Recurrence;

    #[test]
    fn recurrences_read_in_words() {
        assert_eq!(recurrence_label(&Recurrence::EveryHours { hours: 6 }), "Every 6 hours");
        assert_eq!(recurrence_label(&Recurrence::EveryHours { hours: 1 }), "Every hour");
        assert_eq!(recurrence_label(&Recurrence::Daily { at_minute: 9 * 60 }), "Every day at 09:00");
        assert_eq!(recurrence_label(&Recurrence::Weekly { weekdays: vec![0, 1, 2, 3, 4], at_minute: 540 }), "Every weekday at 09:00");
        assert_eq!(recurrence_label(&Recurrence::Weekly { weekdays: vec![6, 5], at_minute: 600 }), "Every weekend at 10:00");
        assert_eq!(recurrence_label(&Recurrence::Weekly { weekdays: vec![2, 0], at_minute: 510 }), "Every Mon, Wed at 08:30");
    }

    #[test]
    fn times_are_typed_as_hours_and_minutes() {
        assert_eq!(parse_clock("09:30"), Some(570));
        assert_eq!(parse_clock(" 9:05 "), Some(545));
        assert_eq!(parse_clock("24:00"), None);
        assert_eq!(parse_clock("9"), None);
        assert_eq!(format_clock(570), "09:30");
    }
}
