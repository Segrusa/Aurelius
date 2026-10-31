//! Bridge between the GPUI thread and the server. A tokio thread owns the
//! connection; the UI only holds read models and sends requests. On
//! disconnect it reconnects, refetches snapshots and re-sends unconfirmed
//! commands with the same command ID (idempotent).
//!
//! The server is the service on this computer (named pipe) or one on another
//! computer over Tailscale; `Target` says which. Paths from this
//! computer never reach a remote server: files are uploaded first, folders
//! are picked on the server, Claude Code is looked up there.

use aurelius_client::{Client, ClientError, Target, envelope, retryable};
use aurelius_domain::{
    ApprovalId, Command, CommandEnvelope, ProviderId, ConversationId, EmployeeId, MessageId, ProjectId, RunId, approval,
    connection::{self, ConfigScope, ProviderKind, Purpose, RunDefaults},
    conversation,
    employee::{self, LifecycleStatus},
    project,
};
use aurelius_protocol::{Method, ResponseBody};
use aurelius_protocol::{
    ApprovalView, CommandResult, ProvidersSnapshot, Event, JobSummary, ProposalView, EmployeeSummary, MessagesSnapshot,
    ProjectSummary, StreamDelta, TaskView,
};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq)]
pub enum Connection {
    Connecting,
    Online { daemon_version: String, server: ServerInfo },
    Offline(String),
    /// Removed on the server, paired without full access, or another server
    /// at the address: reconnecting won't help (Settings → Servers).
    Unpaired(String),
    /// No service on this computer and no server chosen yet.
    NoServer,
    /// The server speaks another protocol major: reconnecting won't help, an update will.
    UpdateNeeded(String),
}

/// The handshake failed on the protocol major: an update helps, reconnecting does not.
pub fn update_needed(err: &aurelius_client::ClientError) -> bool {
    matches!(err, aurelius_client::ClientError::UpdateNeeded(_))
}

/// The server this window talks to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServerInfo {
    pub host_name: String,
    /// Over the network (another computer); folders and files are on the server.
    pub remote: bool,
    /// This connection may change load limits (the Approve right; always on the local server).
    pub approve: bool,
    /// This connection has the owner right (always on the local server); agent functions and API keys need it.
    pub owner: bool,
    /// This computer's pairing, when remote.
    pub client_id: Option<aurelius_domain::ClientId>,
    /// Uploaded files can be fetched by id (`upload.get`); older servers can't.
    pub uploads: bool,
    /// The server has plugins per project (for the project assistant).
    pub project_plugins: bool,
    /// A project's plugins apply to its managers too (feature `project_plugins_managers`).
    pub project_plugins_managers: bool,
    /// A project's description can be edited (feature `project_description`).
    pub project_description: bool,
    /// Keys for outside apps that use `aurelius-mcp` (feature `app_keys`).
    pub app_keys: bool,
    /// App keys have an audience (feature `management_channel`).
    pub management_channel: bool,
    /// App keys can have a webhook (feature `webhooks`).
    pub webhooks: bool,
    /// Ideas have files and can be deleted (feature `idea_attachments`).
    pub idea_attachments: bool,
    /// Tokens per answer and `usage.get_totals` (feature `run_usage`).
    pub run_usage: bool,
    /// Trace mode and `run.get_trace` (feature `run_trace`).
    pub run_trace: bool,
    /// Managers propose instructions; versions and restore (feature `instruction_proposals`).
    pub instruction_proposals: bool,
    /// The agent can end an answer with quick replies (feature `quick_replies`).
    pub quick_replies: bool,
    /// Management chooses its own task mode (feature `management_chooses_mode`).
    pub management_chooses_mode: bool,
    /// Safe read-only tools pass without asking, a server setting (feature `auto_allow`).
    pub auto_allow: bool,
    /// Tickets have their discussion and a ticket can be made from a message
    /// (feature `ticket_discussion`, `CreateTicket::from_message`).
    pub ticket_discussion: bool,
}

/// Thumbnails are asked at this size: sharp at 2x in a 120×80 tile.
pub const UPLOAD_THUMB_SIDE: u32 = 240;

/// The server the app should use at start: the chosen remote server, else the
/// service on this computer when this install has one, else none.
pub fn initial_target(root: &std::path::Path) -> Result<Option<Target>, String> {
    let servers = aurelius_client::servers::Servers::load(root);
    if let Some(host_id) = servers.active {
        return servers
            .credentials(host_id)
            .map(|server| Some(Target::Remote(server)))
            .map_err(|e| format!("The chosen server can't be used: {e}. Pick or pair one in Settings → Servers."));
    }
    Ok(local_service_available().then(|| Target::Local { root: root.to_path_buf(), spawn: true }))
}

/// This install can run the background service itself: not a client-only
/// install (`role.txt`), a service program next to the app, and not macOS
/// (the Mac is a client of a server elsewhere for now).
pub fn local_service_available() -> bool {
    if cfg!(target_os = "macos") {
        return false;
    }
    let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) else { return false };
    let role = std::fs::read_to_string(dir.join("role.txt")).unwrap_or_default();
    if role.trim() == "client" {
        return false;
    }
    std::env::var_os("AURELIUS_DAEMON_PATH").is_some() || dir.join(format!("aurelius-daemon{}", std::env::consts::EXE_SUFFIX)).is_file()
}

/// Where pairing with a server stands (Settings → Servers → Add).
#[derive(Debug)]
pub enum PairEvent {
    Waiting(String),
    Done(Result<aurelius_client::RemoteServer, String>),
}

/// Pair this computer with the server in `text`, on a thread of its own
/// (works while the window is connected elsewhere or not at all).
pub fn pair_server(text: String, device_name: String) -> mpsc::UnboundedReceiver<PairEvent> {
    let (tx, rx) = mpsc::unbounded_channel();
    std::thread::Builder::new()
        .name("aurelius-pairing".into())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                let _ = tx.send(PairEvent::Done(Err("could not start pairing".into())));
                return;
            };
            let progress = tx.clone();
            let outcome = runtime.block_on(aurelius_client::remote::pair(&text, &device_name, move |p| {
                let aurelius_client::remote::PairProgress::Waiting { host_name } = p;
                let _ = progress.send(PairEvent::Waiting(host_name));
            }));
            let _ = tx.send(PairEvent::Done(outcome.map_err(|e| e.to_string())));
        })
        .expect("pairing thread");
    rx
}

/// Why a Markdown file or list could not be shown.
#[derive(Debug, Clone, PartialEq)]
pub enum MarkdownFailure {
    /// Outside the work folder, or the client may not read.
    Forbidden(String),
    NotFound(String),
    /// Over the size limit.
    TooLarge(String),
    /// Not Markdown, not UTF-8, a folder, …
    Invalid(String),
    Other(String),
}

impl MarkdownFailure {
    pub fn of(error: &aurelius_client::ClientError) -> Self {
        use aurelius_protocol::ErrorCode;
        match error {
            aurelius_client::ClientError::Protocol(p) => match p.code {
                ErrorCode::Forbidden => Self::Forbidden(p.message.clone()),
                ErrorCode::NotFound => Self::NotFound(p.message.clone()),
                ErrorCode::Validation if p.message.contains("too large") => Self::TooLarge(p.message.clone()),
                ErrorCode::Validation => Self::Invalid(p.message.clone()),
                _ => Self::Other(p.message.clone()),
            },
            other => Self::Other(other.to_string()),
        }
    }

    pub fn headline(&self) -> &'static str {
        match self {
            Self::Forbidden(_) => "Not allowed",
            Self::NotFound(_) => "Not found",
            Self::TooLarge(_) => "Too large",
            Self::Invalid(_) => "Can't show this file",
            Self::Other(_) => "Couldn't load",
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Self::Forbidden(m) | Self::NotFound(m) | Self::TooLarge(m) | Self::Invalid(m) | Self::Other(m) => m.clone(),
        }
    }
}

#[derive(Debug)]
pub enum Request {
    CreateProject { name: String, description: Option<String>, folder: Option<String> },
    RenameProject { id: ProjectId, name: String, expected_version: u64 },
    /// Answered with `Update::DescriptionSaved`; empty clears it.
    SetProjectDescription { id: ProjectId, description: String, expected_version: u64 },
    Retry { conversation_id: ConversationId, user_message_id: MessageId },
    Stop { run_id: RunId },
    /// Remove a user message still waiting behind a running answer (1.9).
    DeleteQueued { conversation_id: ConversationId, message_id: MessageId },
    /// Stop the running answers and answer this waiting message now (1.9).
    SendQueuedNow { conversation_id: ConversationId, message_id: MessageId },
    /// Show this conversation; its messages are kept up to date.
    OpenConversation(Option<ConversationId>),
    /// Show the group chat (Team chat), kept up to date like `OpenConversation`:
    /// a channel from `channel.list` (feature `team_channels`), or none for the general one.
    OpenGroupChat(Option<ConversationId>),
    /// The Team chat's channels (`channel.list`) once; answered with `Update::Channels`.
    LoadChannels,
    /// The Team chat shows (`true`: read the channels now and again on every chat
    /// event, for their counts) or closed (`false`: stop that).
    FollowChannels(bool),
    CreateEmployee { name: String, role: String, title: Option<String>, reports_to: Option<EmployeeId>, instructions: Option<String>, folder: Option<String> },
    /// `title`: `None` leaves it, an empty text clears it.
    UpdateEmployee { employee_id: EmployeeId, name: String, role: String, title: Option<String>, instructions: Option<String>, sequential_tasks: Option<bool>, expected_version: u64 },
    /// Who a manager reports to (`None`: a root of the org chart); a cycle is refused by the server.
    SetReportsTo { employee_id: EmployeeId, reports_to: Option<EmployeeId> },
    /// The user's decision on a colleague or instructions a manager proposed;
    /// `instructions` approves an instruction proposal with the user's edits.
    DecideProposal { proposal_id: aurelius_domain::ProposalId, approve: bool, edits: Option<aurelius_domain::proposal::ProposalFields>, note: Option<String>, instructions: Option<String> },
    /// The versions of a manager's instructions (`manager.instruction_versions`).
    LoadInstructionVersions(EmployeeId),
    /// Put an earlier version of a manager's instructions back (`manager.restore_instructions`).
    RestoreInstructions { employee_id: EmployeeId, version: u32 },
    /// Settings → Company: whether management chooses its own task mode.
    LoadManagementMode,
    SetManagementMode(bool),
    /// Whether safe read-only tools pass without asking, and which (`auto_allow.get_setting`).
    LoadAutoAllow,
    /// Turn auto-allow on or off (`auto_allow.set_enabled`, needs Approve).
    SetAutoAllow(bool),
    SetEmployeeStatus { employee_id: EmployeeId, status: LifecycleStatus },
    SetProjectStatus { project_id: ProjectId, status: LifecycleStatus },
    SendManager { employee_id: EmployeeId, text: String, conversation_id: ConversationId, images: Vec<String>, files: Vec<String>, interrupt: bool },
    /// A message in the group chat (`group_chat.send`), in a channel (none: the general one).
    SendGroup { text: String, images: Vec<String>, files: Vec<String>, channel: Option<ConversationId> },
    /// `always_allow`: approve and allow this MCP tool from now on (only for `always_allow_eligible` approvals).
    DecideApproval { approval_id: ApprovalId, action_digest: String, approve: bool, always_allow: bool },
    AnswerQuestion { approval_id: ApprovalId, answers: std::collections::BTreeMap<String, String> },
    AddConnection { provider: ProviderKind, name: String, runtime_path: Option<String> },
    RemoveConnection(ProviderId),
    VerifyConnection(ProviderId),
    /// Global default for everything that runs: managers, task
    /// workers, reviews and memory extraction.
    UseConnection { connection_id: ProviderId, model: Option<String> },
    SetDefaults { scope: ConfigScope, purpose: Purpose, settings: RunDefaults },
    /// `files`: files on this computer, attached to the task (protocol 1.14).
    CreateTask { project_id: ProjectId, employee_id: Option<EmployeeId>, title: String, goal: String, criteria: Vec<String>, run: aurelius_domain::task::RunSettings, files: Vec<String> },
    /// Recurring tasks (protocol 1.14); files on this computer are uploaded first.
    /// Keys of outside apps (feature `app_keys`, owner right).
    AppKeys(AppKeyCall),
    /// Manager plugins (protocol 1.22).
    Plugins(PluginsCall),
    CreateSchedule(aurelius_domain::schedule::CreateTaskSchedule),
    UpdateSchedule(aurelius_domain::schedule::UpdateTaskSchedule),
    DeleteSchedule(aurelius_domain::TaskScheduleId),
    PauseSchedule { schedule_id: aurelius_domain::TaskScheduleId, paused: bool },
    RunScheduleNow(aurelius_domain::TaskScheduleId),
    CancelTask(aurelius_domain::TaskId),
    AssignTask { task_id: aurelius_domain::TaskId, employee_id: Option<EmployeeId> },
    /// Goes to the OS credential store via `secrets/set`; never a command or event.
    SetSecret { connection_id: ProviderId, secret: String },
    MarkNotificationsRead,
    /// Clear the results (inbox notifications) with these ids; approvals are untouched.
    ClearResults(Vec<String>),
    /// Show this project's memory (kept up to date).
    LoadMemory(ProjectId),
    /// What of a project's memory goes into a manager's context (`None`: no manager).
    LoadMemoryContext { project: ProjectId, manager: Option<aurelius_domain::EmployeeId> },
    /// A project's ideas (kept fresh while open).
    /// One project's ideas, or every project's (`None`).
    LoadIdeas(Option<ProjectId>),
    AddIdea { project_id: ProjectId, title: String, note: Option<String>, kind: aurelius_domain::idea::IdeaKind, files: Vec<String> },
    UpdateIdea { idea_id: aurelius_domain::IdeaId, title: String, note: Option<String>, kind: Option<aurelius_domain::idea::IdeaKind>, add_files: Vec<String>, remove_files: Vec<String> },
    DeleteIdea(aurelius_domain::IdeaId),
    SetIdeaStatus { idea_id: aurelius_domain::IdeaId, status: aurelius_domain::idea::IdeaStatus },
    /// `mode`: `None` follows the manager (and is all a server before 1.8 gets).
    StartIdea { idea_id: aurelius_domain::IdeaId, employee_id: EmployeeId, mode: Option<aurelius_domain::autonomy::AgentMode>, model: Option<(aurelius_domain::ProviderId, String)> },
    /// Context manifest of one answer (context inspector).
    LoadContext(RunId),
    /// A screenshot the agent took (side panel).
    LoadScreenshot(String),
    /// One shortened tool call whole: (message, call id).
    LoadFullActivity(aurelius_domain::MessageId, String),
    /// A file kept with a message (to show or open).
    LoadFile(String),
    /// A file the user uploaded (`MessageFileView::id`), whole; it arrives as `Update::File`.
    LoadUpload(String),
    /// An uploaded image's thumbnail.
    LoadUploadThumb(String),
    /// The subagents running in a conversation (protocol 1.6).
    LoadSubagents(ConversationId),
    /// Messages of the conversation in the preview pane (not the open one).
    LoadPreviewMessages(ConversationId),
    /// One Markdown file of a work folder, relative or absolute (protocol 1.16).
    LoadMarkdownFile(aurelius_protocol::FileScope, String),
    /// The kept file of an artifact version (`None`: the newest), for the side panel.
    LoadArtifactContent { url: String, version: Option<u32> },
    Remember { project_id: ProjectId, kind: aurelius_domain::memory::MemoryKind, content: String },
    DecideMemory { memory_id: aurelius_domain::MemoryId, accept: bool },
    ReviseMemory { memory_id: aurelius_domain::MemoryId, content: String },
    ForgetMemory(aurelius_domain::MemoryId),
    PinMemory { memory_id: aurelius_domain::MemoryId, pinned: bool },
    /// Merge one memory (a project's or a manager's) into short points now.
    ConsolidateMemory(aurelius_domain::memory::MemoryScope),
    SetAgentMode { conversation_id: ConversationId, mode: aurelius_domain::autonomy::AgentMode },
    /// Model and effort of one conversation (unset: the manager's).
    SetConversationRun { conversation_id: ConversationId, model: Option<String>, effort: Option<String> },
    /// Let the agent use the user's own browser (Claude in Chrome), or not.
    SetBrowserAllowed { conversation_id: ConversationId, allowed: bool },
    /// Let the agent take screenshots without asking each time, or not.
    SetScreenAllowed { conversation_id: ConversationId, allowed: bool },
    /// Approve, ask changes on, or reject a plan Claude handed in.
    DecidePlan { approval_id: ApprovalId, decision: aurelius_domain::approval::PlanDecision, feedback: Option<String> },
    LoadUsage,
    /// Usage per hour of the day starting at this time (a `by_day` value).
    LoadUsageHours(i64),
    /// Claude subscription limits; `true` bypasses the one-minute cache.
    LoadSubscription(bool),
    SetProjectFolder { project_id: ProjectId, path: String },
    SetEmployeeFolder { employee_id: EmployeeId, path: Option<String> },
    DeleteConversation(ConversationId),
    SetHeartbeat { employee_id: EmployeeId, every_minutes: u32 },
    /// Update status; `true` asks GitHub now.
    CheckUpdate(bool),
    /// Download, verify and apply the latest server release; the server restarts.
    InstallUpdate,
    /// The same for this desktop app (released separately); the app restarts.
    InstallAppUpdate,
    /// Restart the server without updating it (protocol 1.7); the app reconnects.
    RestartServer,
    /// Shut down the server's PC, or cancel the pending shutdown (protocol 1.12).
    Power(PowerCall),
    /// The server's load and limits (protocol 1.10; kept fresh by `load.changed`).
    LoadServerLoad,
    /// Change the load limits (`None` leaves one as it is).
    SetLoadLimits(aurelius_protocol::LoadLimitsUpdate),
    /// Which browser Claude in Chrome uses (protocol 1.19).
    LoadBrowser,
    SetBrowser(aurelius_protocol::BrowserChoice),
    /// Claude in Chrome or Playwright (`browser.set_backend`).
    SetBrowserBackend(aurelius_protocol::BrowserBackend),
    /// Whether runs may publish Claude Code artifacts (`artifact.get_setting`).
    LoadArtifactSetting,
    /// Turn Claude Code artifacts on or off (`artifact.set_enabled`, needs Approve).
    SetArtifactsEnabled(bool),
    /// What a conversation or task used (`usage.get_totals`; a task wins).
    LoadUsageTotals { conversation_id: ConversationId, task_id: Option<aurelius_domain::TaskId> },
    /// Whether runs keep a trace (`trace.get_setting`).
    LoadTraceSetting,
    /// Trace mode on or off (`trace.set_enabled`, needs Approve; off deletes the traces).
    SetTraceEnabled(bool),
    /// What one run sent (`run.get_trace`).
    LoadTrace(aurelius_domain::RunId),
    /// The server-wide MCP servers (`mcp/list`).
    LoadMcp,
    /// Turn exactly these servers on (`mcp/set`, needs Approve).
    SetMcp(Vec<String>),
    /// Add an MCP server (`mcp/add`, owner right, protocol 1.23). Its `Debug` hides secret values.
    McpAdd(aurelius_protocol::McpAddParams),
    /// Remove a user-scope MCP server (`mcp/remove`, owner right).
    McpRemove(String),
    /// OAuth login and logout of a remote MCP server (Approve right).
    McpAuth(McpAuthCall),
    /// The MCP tools always allowed (`mcp_always_allow.list`).
    LoadMcpAlwaysAllow,
    /// Take a tool off that list (`mcp_always_allow.remove`, needs Approve).
    McpAlwaysAllowRemove(String),
    Backups(BackupsCall),
    ExportProject(ProjectId),
    /// Remote access (phone companion); never a command.
    Remote(RemoteCall),
    /// Folders on the server (`None`: the starting points); `typed`: the
    /// path was typed in the browser, so an error shows under that field.
    BrowseDir { path: Option<String>, typed: bool },
    /// A new folder `name` in `parent` (protocol 1.10); the browser opens into it.
    CreateDir { parent: String, name: String },
    /// Add the Claude Code installed on the server as a connection.
    AddClaudeCode,
    /// Board members (protocol 4): list, create, choose who works here.
    BoardMembers(UserCall),
    /// The board, a ticket and their writes (protocol 4; see [`BoardCall`]).
    Board(BoardCall),
}

/// The board's reads and writes. Writes are not kept for a reconnect: the
/// board disables its actions while offline, and a write the connection lost
/// says so (the board reloads and shows what the server has).
#[derive(Debug, Clone)]
pub enum BoardCall {
    /// `board.get` for one project or every project (B4).
    Load(Option<ProjectId>),
    /// `ticket.get`.
    Ticket(aurelius_domain::TicketId),
    /// A ticket's discussion (`conversation.get` on `TicketView::conversation_id`),
    /// kept up to date while it shows (AUR-MU-02); none: it no longer shows.
    Discussion(Option<ConversationId>),
    /// The latest plan in the main task's conversation (Plan review).
    Plan(ConversationId),
    /// What happened to a ticket, newest first (`timeline.list` on its id, Activity).
    Activity(aurelius_domain::TicketId),
    /// Whether the dispatcher gave up on a Todo ticket at this version (B5, from its timeline).
    GaveUp { ticket: aurelius_domain::TicketId, version: u64 },
    /// A ticket write, `board.columns_update` (with the board's version) or
    /// `project.update`. `done`: what the board says when it worked.
    Write { command: Command, expected_version: Option<u64>, done: Option<String> },
}

impl BoardCall {
    pub fn is_write(&self) -> bool {
        matches!(self, BoardCall::Write { .. })
    }

    /// A write that could not go, answered as its own kind: the discussion's writes
    /// to the discussion (its draft stays and its buttons wait no longer), the others to the board.
    pub fn refused(&self, message: String) -> BoardUpdate {
        match self {
            BoardCall::Write { command: Command::PostTicketMessage(p), .. } => BoardUpdate::PostFailed { ticket: p.ticket_id, message },
            BoardCall::Write { command: Command::RetryTicketSummary(r), .. } => BoardUpdate::SummaryRetried { ticket: r.ticket_id, failure: Some(message) },
            _ => BoardUpdate::Failed { ticket: self.ticket(), message },
        }
    }

    /// The ticket a write is about (its card springs back when it fails).
    pub fn ticket(&self) -> Option<aurelius_domain::TicketId> {
        match self {
            BoardCall::Write { command, .. } => ticket_of(command),
            BoardCall::Ticket(id) => Some(*id),
            _ => None,
        }
    }
}

/// The ticket of a ticket command.
pub fn ticket_of(command: &Command) -> Option<aurelius_domain::TicketId> {
    Some(match command {
        Command::UpdateTicket(c) => c.ticket_id,
        Command::MoveTicket(c) => c.ticket_id,
        Command::StartTicket(c) => c.ticket_id,
        Command::AssignTicket(c) => c.ticket_id,
        Command::ApprovePlan(c) => c.ticket_id,
        Command::RevisePlan(c) => c.ticket_id,
        Command::AcceptTicket(c) => c.ticket_id,
        Command::SendBackTicket(c) => c.ticket_id,
        Command::CancelTicket(c) => c.ticket_id,
        Command::ArchiveTicket(c) => c.ticket_id,
        _ => return None,
    })
}

/// What the board hears back.
#[derive(Debug, Clone)]
pub enum BoardUpdate {
    Loaded(Option<ProjectId>, Result<aurelius_domain::ticket::BoardView, String>),
    Ticket(aurelius_domain::TicketId, Result<aurelius_domain::ticket::TicketDetail, String>),
    /// The text of the latest plan in that conversation (none: no plan there).
    /// A conversation's newest plan and its agent mode (none when it did not load).
    Plan(ConversationId, Option<String>, Option<aurelius_domain::autonomy::AgentMode>),
    /// A ticket's activity, newest first.
    Activity(aurelius_domain::TicketId, Result<Vec<aurelius_protocol::TimelineCard>, String>),
    /// A ticket's discussion, or why it did not load.
    Discussion(ConversationId, Result<Vec<aurelius_protocol::MessageView>, String>),
    /// `ticket.post` went through: its `to` (the owner first when the post went to
    /// them in the main task, then those woken in the discussion; `discussion::reached`).
    Posted { ticket: aurelius_domain::TicketId, to: Vec<EmployeeId> },
    /// `ticket.post` did not go: why (the draft stays; `unknown_method` from an older server).
    PostFailed { ticket: aurelius_domain::TicketId, message: String },
    /// `ticket.retry_summary` answered: none when it went, else why not (MU2-12 AC4).
    SummaryRetried { ticket: aurelius_domain::TicketId, failure: Option<String> },
    /// `gave_up`: none when the timeline did not load.
    GaveUp { ticket: aurelius_domain::TicketId, version: u64, gave_up: Option<bool> },
    /// A write worked; `text` is what to say.
    Done { ticket: Option<aurelius_domain::TicketId>, text: Option<String> },
    /// `ticket.create` worked.
    Created { number: u32, text: Option<String> },
    /// A write failed: the server's reason, or a lost connection.
    Failed { ticket: Option<aurelius_domain::TicketId>, message: String },
    /// An event changed the board, a ticket, the company, a project or a task:
    /// read again what shows. `tickets`: a ticket or the columns changed (the
    /// sidebar's "Needs you" counts them even while the board does not show);
    /// `settings`: the company or a project (the dispatcher); `tasks`: the ids of
    /// changed tasks, which matter only when they work for a ticket.
    Changed { tickets: bool, settings: bool, tasks: Vec<String> },
}

/// How much of a ticket's activity one read brings (the newest).
pub const ACTIVITY_LIMIT: u32 = 60;

/// What a lost connection during a board write tells the user.
pub const BOARD_CONNECTION_DROPPED: &str = "The connection dropped before the server answered. The board reloads with what the server has.";

/// Why the board did not load, for its page (ui-ontwerp §3.3).
pub fn board_failure(e: &ClientError) -> String {
    match e {
        ClientError::Protocol(p) if p.code == aurelius_domain::error::ErrorCode::UnknownMethod => "This server does not have the Board yet. Update the server.".into(),
        ClientError::TimedOut(_) => "The server did not answer in time.".into(),
        e => e.to_string(),
    }
}

/// A write refused for a stale version (§7.2): the window shows the latest.
pub const BOARD_CONFLICT: &str = "Changed on another device. Showing the latest.";

/// A refused board write in the user's words: a version conflict means another
/// device changed it first (§7.2).
pub fn board_write_failure(e: &ClientError) -> String {
    match e {
        e if e.is_disconnect() || e.is_final() => BOARD_CONNECTION_DROPPED.into(),
        ClientError::Protocol(p) if p.code == aurelius_domain::error::ErrorCode::VersionConflict => BOARD_CONFLICT.into(),
        ClientError::Protocol(p) => p.message.clone(),
        e => e.to_string(),
    }
}

/// A password on its way to the server: never printed, never kept after the call.
pub struct Password(String);

impl Password {
    pub fn new(text: String) -> Self {
        Self(text)
    }
}

impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug)]
pub enum UserCall {
    List,
    /// A new board member; the connection becomes it when it had none (onboarding).
    Create { username: String, password: Password },
    /// The one password check of this device (B8): the board member works here from now on.
    Use { user_id: aurelius_domain::UserId, password: Password },
}

/// Which field of the board member forms a refusal belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberField {
    Username,
    Password,
}

/// Why `user.create` or `user.use` was refused, for the form that asked.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberFailure {
    pub field: Option<MemberField>,
    pub message: String,
}

/// The server's refusal as the forms show it: by the field it is about, in the
/// words of the design (§11.1). Never repeats what was typed.
pub fn member_failure(err: &ClientError) -> MemberFailure {
    use aurelius_domain::error::ErrorCode;
    let (field, message) = match err {
        ClientError::Protocol(p) => match p.code {
            ErrorCode::UsernameTaken => (Some(MemberField::Username), "This username is taken.".to_string()),
            ErrorCode::InvalidUsername => (Some(MemberField::Username), p.message.clone()),
            ErrorCode::WeakPassword => (Some(MemberField::Password), p.message.clone()),
            ErrorCode::WrongPassword => (Some(MemberField::Password), "That password is wrong.".to_string()),
            ErrorCode::TooManyAttempts => (Some(MemberField::Password), "Too many tries. Wait a minute and try again.".to_string()),
            _ => (None, p.message.clone()),
        },
        other => (None, other.to_string()),
    };
    MemberFailure { field, message }
}

/// What a lost connection during `user.create` / `user.use` tells the form.
pub const MEMBER_CONNECTION_DROPPED: &str = "The connection dropped. Check the list of board members and try again.";

/// A failed `user.create` / `user.use` as its form shows it: a refusal by its
/// field, a lost connection as such (the server may have made it anyway).
pub fn member_call_failure(err: &ClientError) -> MemberFailure {
    if err.is_disconnect() || err.is_final() {
        return MemberFailure { field: None, message: MEMBER_CONNECTION_DROPPED.into() };
    }
    member_failure(err)
}

/// The gate's notice when a write needs a board member and this window has none.
pub const BOARD_MEMBER_REQUIRED: &str = "Choose who you are first: this needs an active board member.";

/// A write refused because this connection has no active board member.
pub fn needs_board_member(err: &ClientError) -> bool {
    matches!(err, ClientError::Protocol(p) if p.code == aurelius_domain::error::ErrorCode::BoardMemberRequired)
}

/// Which folder browser request failed, so its error shows in the right place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirAsk {
    /// Opening a folder from the list, a crumb or quick access.
    Browse,
    /// A path typed with Ctrl+L.
    Typed,
    /// New folder.
    Create,
}

#[derive(Debug, Clone)]
pub enum RemoteCall {
    Status,
    Enable,
    Disable,
    PairingStart,
    Decide { pairing_id: String, approve: bool, owner: bool },
    Revoke(aurelius_domain::ClientId),
    PushConfigure(aurelius_protocol::remote::PushConfig),
    PushTest(aurelius_domain::ClientId),
}

#[derive(Debug, Clone)]
pub enum BackupsCall {
    List,
    Create,
    Restore(String),
    Resume,
}

/// What to do with app keys; every call answers with the list again.
#[derive(Debug, Clone)]
pub enum AppKeyCall {
    List,
    Create(aurelius_domain::app_key::CreateAppKey),
    Revoke(aurelius_domain::AppKeyId),
}

#[derive(Debug)]
pub enum AppKeyUpdate {
    Keys(Vec<aurelius_domain::app_key::AppKeyView>),
    /// A new key with its secret (only here).
    Created(aurelius_domain::app_key::AppKeyCreated),
    Failed(String),
}

/// What to ask about manager plugins (protocol 1.22; changes need Approve).
#[derive(Debug, Clone)]
pub enum PluginsCall {
    List,
    Available,
    /// Add the marketplace behind a link and list its plugins.
    Preview(String),
    Install { ids: Vec<String>, managers: Vec<aurelius_domain::EmployeeId>, projects: Vec<ProjectId> },
    Update(String),
    Remove(String),
    ManagerGet(aurelius_domain::EmployeeId),
    ManagerSet(aurelius_domain::EmployeeId, Vec<String>),
    ProjectGet(ProjectId),
    ProjectSet(ProjectId, Vec<String>),
}

/// What the server answered about plugins.
#[derive(Debug)]
pub enum PluginsUpdate {
    Installed(Result<aurelius_protocol::PluginsSnapshot, String>),
    Catalog(Result<aurelius_protocol::PluginCatalog, String>),
    Preview(Result<aurelius_protocol::PluginPreview, String>),
    /// The install call ended: the new list, or what went wrong.
    InstallDone(Result<aurelius_protocol::PluginsSnapshot, String>),
    Manager(aurelius_protocol::EmployeePlugins),
    Project(aurelius_protocol::ProjectPlugins),
    /// A change (update, remove, a manager's toggles) failed.
    Failed(String),
}

/// An OAuth login or logout of an MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpAuthCall {
    Start(String),
    /// The address the browser ended on (another computer than the server's).
    Complete { login_id: String, redirect_url: String },
    Logout(String),
}

/// What to ask the server about its PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerCall {
    Shutdown(aurelius_protocol::ShutdownWhen),
    Cancel,
}

#[derive(Debug)]
pub enum Update {
    Tasks(Vec<TaskView>),
    AppKeys(AppKeyUpdate),
    Plugins(PluginsUpdate),
    /// The recurring tasks of every project (servers from 1.14).
    Schedules(Vec<aurelius_protocol::TaskScheduleView>),
    Notifications(aurelius_protocol::NotificationsSnapshot),
    Memory(ProjectId, Vec<aurelius_protocol::MemoryView>),
    /// A memory or share changed; reload the list only if it is on screen.
    MemoryChanged,
    /// Answer to `LoadMemoryContext`; the error is shown in place of the view.
    MemoryContext { project: ProjectId, manager: Option<aurelius_domain::EmployeeId>, result: Result<aurelius_protocol::MemoryContextView, String> },
    Context(aurelius_protocol::ContextView),
    /// A screenshot's JPEG, or why it could not be loaded.
    Screenshot(String, Result<Vec<u8>, String>),
    /// A tool call whole, by call id, or why it could not be loaded.
    FullActivity(String, Result<aurelius_protocol::ActivityView, String>),
    Ideas(Option<ProjectId>, Vec<aurelius_protocol::IdeaView>),
    /// A file kept with a message: id → (name, media type, bytes).
    File(String, Result<(String, String, Vec<u8>), String>),
    /// An uploaded image's thumbnail (JPEG), by upload id.
    UploadThumb(String, Result<Vec<u8>, String>),
    Usage(aurelius_protocol::UsageSnapshot),
    /// A day's usage per hour (keyed by the requested day), or why there is none.
    UsageHours(i64, Result<aurelius_protocol::UsageHours, String>),
    Subscription(aurelius_protocol::SubscriptionUsage),
    /// The server's load (fetched or pushed), or why there is none.
    Load(Result<aurelius_protocol::LoadSnapshot, String>),
    /// New load limits are in place (the new snapshot), or why not.
    LoadSaved(Result<aurelius_protocol::LoadSnapshot, String>),
    /// The browser setting for Claude in Chrome (fetched or just saved), or why there is none.
    Browser(Result<aurelius_protocol::BrowserSetting, String>),
    /// Claude Code artifacts on or off; `None` when the server has no such setting (older server).
    ArtifactSetting(Option<bool>),
    /// What the conversation used, keyed by the conversation asked for.
    UsageTotals(ConversationId, aurelius_protocol::UsageTotals),
    /// Trace mode; `None` when the server could not say.
    TraceSetting(Option<aurelius_protocol::TraceSetting>),
    /// One run's trace, or why there is none.
    Trace(aurelius_domain::RunId, Result<aurelius_protocol::RunTrace, String>),
    /// The versions of a manager's instructions, or why they could not be loaded.
    InstructionVersions(EmployeeId, Result<aurelius_protocol::EmployeeInstructionVersionsSnapshot, String>),
    /// `management_mode.get_setting`: on or off (`None`: not available).
    ManagementMode(Option<bool>),
    /// `auto_allow.get_setting`: the setting with its tool list (`None`: not available).
    AutoAllow(Option<aurelius_protocol::AutoAllowSetting>),
    /// Every recorded artifact, newest publish first (only sent when the server has `artifact.list`).
    Artifacts(Vec<aurelius_protocol::ArtifactView>),
    /// The MCP server list (fetched or just saved), or why it could not be loaded.
    Mcp(Result<aurelius_protocol::McpServers, String>),
    /// Saving the MCP servers failed; the list stays as it was.
    McpSaveFailed(String),
    /// `mcp/add` went through (the list arrives as `Mcp`).
    McpAdded,
    /// `mcp/add` failed; the text is the server's, with any secret value blanked.
    McpAddFailed(String),
    /// `mcp/remove` failed.
    McpRemoveFailed(String),
    /// Where an MCP login stands (started, completed, or finished on the server).
    McpLogin(aurelius_protocol::McpLogin),
    /// An MCP login or logout call failed.
    McpAuthFailed(String),
    /// The always-allowed MCP tools; `None` when the server has no such list.
    McpAlwaysAllow(Option<Vec<String>>),
    /// `mcp_always_allow.remove` failed.
    McpAlwaysAllowRemoveFailed(String),
    /// The server's pending PC shutdown (fetched, answered or pushed by `power.changed`).
    Power(aurelius_protocol::PowerState),
    UpdateStatus(aurelius_protocol::UpdateStatus),
    /// Only the desktop app's part of the update status.
    AppUpdateStatus(aurelius_protocol::UpdateStatus),
    Backups(aurelius_protocol::BackupsSnapshot),
    Remote(aurelius_protocol::remote::RemoteStatus),
    /// A fresh pairing code to show as a QR code.
    Pairing(aurelius_protocol::remote::PairingCode),
    /// The new version is in place: start it and close this window.
    RestartForUpdate,
    /// The subagents running in a conversation.
    Subagents(ConversationId, Vec<aurelius_protocol::AgentView>),
    PreviewMessages(MessagesSnapshot),
    /// One Markdown file, as asked (the path may be absolute), or why it can't be shown.
    MarkdownFile(aurelius_protocol::FileScope, String, Result<aurelius_protocol::MarkdownFileContent, MarkdownFailure>),
    /// An artifact's content as asked (url, version), or why not.
    ArtifactContent(String, Option<u32>, Result<aurelius_protocol::ArtifactContent, String>),
    /// Folders on the server, for the folder browser.
    DirListing(aurelius_protocol::DirListing),
    /// The folder browser's request failed; shown inside the browser.
    DirFailed(DirAsk, String),
    Connections(ProvidersSnapshot),
    Approvals(Vec<ApprovalView>),
    Managers(Vec<EmployeeSummary>),
    /// The company (`company.get`): employees, roots and the org chart setting.
    Company(aurelius_protocol::CompanySnapshot),
    /// Colleagues managers proposed: pending first, then the latest decisions.
    Proposals(Vec<ProposalView>),
    Messages(MessagesSnapshot),
    /// The group chat's conversation (its messages follow as `Messages`).
    GroupChat(ConversationId),
    /// The group chat could not be loaded: what to tell the user (see [`group_chat_failure`]).
    GroupChatFailed(String),
    /// The Team chat's channels; none when the server has no channels (before `team_channels`).
    Channels(Option<Vec<aurelius_domain::views::ChannelView>>),
    Stream(StreamDelta),
    EmployeeCreated(EmployeeId),
    Info(String),
    /// The server gave this task to the employee chosen in Give to….
    TaskAssigned(aurelius_domain::TaskId),
    Connection(Connection),
    Projects(Vec<ProjectSummary>),
    Jobs(Vec<JobSummary>),
    Created(ProjectId),
    /// A task was created (the Dashboard opens the one it gave in the right panel).
    TaskCreated(aurelius_domain::TaskId),
    Error(String),
    /// Deleting this waiting message was refused: show it again.
    QueuedDeleteFailed(MessageId),
    /// `project.set_description` answered: `Err` holds the reason, shown under the field.
    DescriptionSaved(ProjectId, Result<(), String>),
    /// Whether the server has board members, and who works in this window (after
    /// each hello, `user.use` and `user.create`).
    Who(aurelius_domain::user::WhoAmI),
    /// The server's board members, oldest first.
    BoardMembers(Vec<aurelius_domain::user::UserView>),
    /// Another board member was made by this window (its name).
    BoardMemberCreated(String),
    BoardMemberFailed(MemberFailure),
    /// A write was refused for want of an active board member.
    BoardMemberRequired,
    Board(BoardUpdate),
}

pub struct Handle {
    pub requests: mpsc::UnboundedSender<Request>,
    pub updates: mpsc::UnboundedReceiver<Update>,
}

/// Connect to `target` (`None`: no server yet), as the board member this
/// window remembers for it. Dropping the handle's request sender ends the
/// thread, also while offline.
pub fn start(target: Option<Target>, remembered: Option<aurelius_domain::UserId>) -> Handle {
    let (req_tx, req_rx) = mpsc::unbounded_channel();
    let (upd_tx, upd_rx) = mpsc::unbounded_channel();
    std::thread::Builder::new()
        .name("aurelius-backend".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
            runtime.block_on(run(target, remembered, req_rx, upd_tx));
        })
        .expect("backend thread");
    Handle { requests: req_tx, updates: upd_rx }
}

/// Nothing to connect to: tell the window once, then wait until it closes or
/// picks a server (which starts a new backend).
async fn idle(requests: &mut mpsc::UnboundedReceiver<Request>, updates: &mpsc::UnboundedSender<Update>, connection: Connection) {
    let _ = updates.send(Update::Connection(connection));
    while let Some(request) = requests.recv().await {
        let text = "Not connected to a server. Choose one in Settings → Servers (Ctrl Alt S).";
        // A form that waits for its answer gets it, so it does not stay busy (Trace M3a m1).
        let _ = updates.send(match request {
            Request::BoardMembers(UserCall::Create { .. } | UserCall::Use { .. }) => Update::BoardMemberFailed(MemberFailure { field: None, message: text.into() }),
            Request::Board(call) if call.is_write() => Update::Board(call.refused(text.into())),
            _ => Update::Error(text.into()),
        });
    }
}

async fn run(target: Option<Target>, remembered: Option<aurelius_domain::UserId>, mut requests: mpsc::UnboundedReceiver<Request>, updates: mpsc::UnboundedSender<Update>) {
    let Some(target) = target else {
        idle(&mut requests, &updates, Connection::NoServer).await;
        return;
    };
    let remote_target = matches!(target, Target::Remote(_));
    // Commands not yet confirmed by a receipt; retried with the same ID after reconnect.
    let mut outbox: VecDeque<CommandEnvelope> = VecDeque::new();
    let mut open_conversation: Option<ConversationId> = None;
    let mut open_memory: Option<ProjectId> = None;
    let mut open_ideas: Option<Option<ProjectId>> = None;
    // A ticket's discussion in the panel, and whether the Team chat shows its channels (AUR-MU-02).
    let mut open_discussion: Option<ConversationId> = None;
    let mut channels_shown = false;
    let mut backoff = Duration::from_millis(250);
    // Who works in this window: said again in every hello, so a reconnect keeps it (Trace m9).
    let mut me = remembered;

    loop {
        let _ = updates.send(Update::Connection(Connection::Connecting));
        let (events_tx, mut events) = mpsc::unbounded_channel();
        let (streams_tx, mut streams) = mpsc::unbounded_channel();
        let client = match Client::open(&target, "aurelius-desktop", events_tx, Some(streams_tx), &[aurelius_protocol::client_feature::TEAM_CHANNELS]).await {
            Ok(client) => client,
            // Before `is_final`, which an update-needed error also is.
            Err(err) if update_needed(&err) => {
                idle(&mut requests, &updates, Connection::UpdateNeeded(err.to_string())).await;
                return;
            }
            Err(err) if err.is_final() => {
                idle(&mut requests, &updates, Connection::Unpaired(err.to_string())).await;
                return;
            }
            Err(err) => {
                let _ = updates.send(Update::Connection(Connection::Offline(err.to_string())));
                // Keep accepting requests while offline; they wait in the outbox.
                let deadline = tokio::time::sleep(backoff);
                tokio::pin!(deadline);
                loop {
                    tokio::select! {
                        _ = &mut deadline => break,
                        req = requests.recv() => match req {
                            // The window closed or switched servers.
                            None => return,
                            Some(Request::OpenConversation(id)) => open_conversation = id,
                            // Its id is known once connected; the window asks again then.
                            Some(Request::OpenGroupChat(_) | Request::LoadChannels) => {}
                            Some(Request::FollowChannels(on)) => channels_shown = on,
                            Some(Request::LoadMemory(project_id)) => open_memory = Some(project_id),
                            Some(Request::LoadIdeas(project_id)) => open_ideas = Some(project_id),
                            Some(Request::SendManager { ref images, .. } | Request::SendGroup { ref images, .. }) if remote_target && !images.is_empty() => {
                                let _ = updates.send(Update::Error("Not connected to the server; send the message with its images again when it is back.".into()));
                            }
                            // Whether the server takes files is only known once connected.
                            Some(Request::SendManager { ref files, .. } | Request::SendGroup { ref files, .. }) if !files.is_empty() => {
                                let _ = updates.send(Update::Error("Not connected to the server; send the message with its files again when it is back.".into()));
                            }
                            Some(Request::CreateTask { ref files, .. }) if !files.is_empty() => {
                                let _ = updates.send(Update::Error("Not connected to the server; create the task with its files again when it is back.".into()));
                            }
                            Some(Request::CreateSchedule(ref s)) if !s.files.is_empty() => {
                                let _ = updates.send(Update::Error("Not connected to the server; save the recurring task with its files again when it is back.".into()));
                            }
                            Some(Request::UpdateSchedule(ref s)) if !s.add_files.is_empty() => {
                                let _ = updates.send(Update::Error("Not connected to the server; save the recurring task with its files again when it is back.".into()));
                            }
                            Some(Request::AddIdea { ref files, .. } | Request::UpdateIdea { add_files: ref files, .. }) if !files.is_empty() => {
                                let _ = updates.send(Update::Error("Not connected to the server; save the idea with its files again when it is back.".into()));
                            }
                            // A password is not kept for later: ask again once connected.
                            Some(Request::BoardMembers(UserCall::Create { .. } | UserCall::Use { .. })) => {
                                let _ = updates.send(Update::BoardMemberFailed(MemberFailure { field: None, message: "Not connected to the server. Try again when it is back.".into() }));
                            }
                            // The board's writes are not kept for later; its reads come again once online.
                            Some(Request::Board(call)) => if call.is_write() {
                                let _ = updates.send(Update::Board(call.refused("Not connected to the server. Try again when it is back.".into())));
                            },
                            Some(other) => if let Some(env) = to_envelope(other) { outbox.push_back(env) },
                        },
                    }
                }
                backoff = (backoff * 2).min(Duration::from_secs(5));
                continue;
            }
        };
        backoff = Duration::from_millis(250);
        let who = match identify(&client, me).await {
            Ok(who) => who,
            Err(_) => continue, // lost again: reconnect
        };
        me = who.user.as_ref().map(|u| u.id);
        let _ = updates.send(Update::Who(who));
        // The server and this app are released apart: their version numbers
        // differ and that is fine. The handshake checked the protocol major.
        let has = |c: aurelius_domain::remote::Capability| client.hello.capabilities.contains(&c);
        let _ = updates.send(Update::Connection(Connection::Online {
            daemon_version: client.hello.daemon_version.clone(),
            server: ServerInfo {
                host_name: client.hello.host_name.clone(),
                remote: client.is_remote(),
                approve: !client.is_remote() || has(aurelius_domain::remote::Capability::Approve) || has(aurelius_domain::remote::Capability::Owner),
                owner: !client.is_remote() || has(aurelius_domain::remote::Capability::Owner),
                client_id: client.hello.client_id,
                uploads: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::UPLOAD_FETCH),
                project_plugins: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::PROJECT_PLUGINS),
                project_plugins_managers: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::PROJECT_PLUGINS_EMPLOYEES),
                project_description: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::PROJECT_DESCRIPTION),
                app_keys: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::APP_KEYS),
                management_channel: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::MANAGEMENT_CHANNEL),
                webhooks: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::WEBHOOKS),
                idea_attachments: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::IDEA_ATTACHMENTS),
                run_usage: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::RUN_USAGE),
                run_trace: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::RUN_TRACE),
                instruction_proposals: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::INSTRUCTION_PROPOSALS),
                quick_replies: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::QUICK_REPLIES),
                management_chooses_mode: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::MANAGEMENT_CHOOSES_MODE),
                auto_allow: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::AUTO_ALLOW),
                ticket_discussion: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::TICKET_DISCUSSION),
            },
        }));

        match session(&client, &mut requests, &mut events, &mut streams, &updates, &mut outbox, &mut open_conversation, &mut open_memory, &mut open_ideas, &mut me, &mut open_discussion, &mut channels_shown).await {
            Ok(()) => return, // UI dropped the request channel: window closed
            Err(Disconnected(Some(reason), _)) => {
                idle(&mut requests, &updates, Connection::Unpaired(reason)).await;
                return;
            }
            Err(Disconnected(None, notice)) => {
                if let Some(notice) = notice {
                    let _ = updates.send(Update::Error(notice));
                }
                continue; // reconnect
            }
        }
    }
}

fn to_envelope(request: Request) -> Option<CommandEnvelope> {
    Some(match request {
        Request::CreateProject { name, description, folder } => {
            envelope(Command::CreateProject(project::CreateProject { name, description, folder }), None)
        }
        Request::RenameProject { id, name, expected_version } => {
            envelope(Command::RenameProject(project::RenameProject { project_id: id, name }), Some(expected_version))
        }
        Request::SetProjectDescription { id, description, expected_version } => {
            envelope(Command::SetProjectDescription(project::SetProjectDescription { project_id: id, description }), Some(expected_version))
        }
        Request::Retry { conversation_id, user_message_id } => {
            envelope(Command::RetryReply(conversation::RetryReply { conversation_id, user_message_id }), None)
        }
        Request::Stop { run_id } => envelope(Command::StopRun(conversation::StopRun { run_id }), None),
        Request::DeleteQueued { conversation_id, message_id } => {
            envelope(Command::DeleteQueuedMessage(conversation::DeleteQueuedMessage { conversation_id, message_id }), None)
        }
        Request::SendQueuedNow { conversation_id, message_id } => {
            envelope(Command::SendQueuedNow(conversation::SendQueuedNow { conversation_id, message_id }), None)
        }
        Request::OpenConversation(_) | Request::OpenGroupChat(_) | Request::LoadChannels | Request::FollowChannels(_) => return None,
        Request::CreateEmployee { name, role, title, reports_to, instructions, folder } => {
            envelope(Command::CreateEmployee(employee::CreateEmployee { name, role, title, reports_to, instructions, folder }), None)
        }
        Request::UpdateEmployee { employee_id, name, role, title, instructions, sequential_tasks, expected_version } => envelope(
            Command::UpdateEmployee(employee::UpdateEmployee { employee_id, name, role, title, instructions, sequential_tasks }),
            Some(expected_version),
        ),
        Request::SetReportsTo { employee_id, reports_to } => envelope(Command::SetEmployeeReportsTo(employee::SetEmployeeReportsTo { employee_id, reports_to }), None),
        Request::DecideProposal { proposal_id, approve, edits, note, instructions } => {
            envelope(Command::DecideProposal(aurelius_domain::proposal::DecideProposal { proposal_id, approve, edits, note, instructions }), None)
        }
        Request::RestoreInstructions { employee_id, version } => {
            envelope(Command::RestoreEmployeeInstructions(aurelius_domain::proposal::RestoreEmployeeInstructions { employee_id, version }), None)
        }
        Request::SetEmployeeStatus { employee_id, status } => {
            envelope(Command::SetEmployeeStatus(employee::SetEmployeeStatus { employee_id, status }), None)
        }
        Request::SetProjectStatus { project_id, status } => {
            envelope(Command::SetProjectStatus(employee::SetProjectStatus { project_id, status }), None)
        }
        Request::SendManager { employee_id, text, conversation_id, images, files, interrupt } => {
            envelope(Command::SendEmployeeMessage(employee::SendEmployeeMessage { employee_id, text, conversation_id, images, files, interrupt }), None)
        }
        Request::SendGroup { text, images, files, channel } => envelope(Command::SendGroupMessage(aurelius_domain::group_chat::SendGroupMessage { text, images, files, channel }), None),
        Request::DecideApproval { approval_id, action_digest, approve, always_allow } => {
            envelope(Command::DecideApproval(approval::DecideApproval { approval_id, action_digest, approve, always_allow }), None)
        }
        Request::AnswerQuestion { approval_id, answers } => envelope(Command::AnswerQuestion(approval::AnswerQuestion { approval_id, answers }), None),
        Request::AddConnection { provider, name, runtime_path } => envelope(
            Command::AddProvider(connection::AddProvider { provider, display_name: name, runtime_path, default_model: None }),
            None,
        ),
        Request::CreateTask { project_id, employee_id, title, goal, criteria, run, files } => envelope(
            Command::CreateTask(aurelius_domain::task::CreateTask { files, project_id, employee_id, title, goal, acceptance_criteria: criteria, depends_on: vec![], run }),
            None,
        ),
        Request::CreateSchedule(schedule) => envelope(Command::CreateTaskSchedule(schedule), None),
        Request::UpdateSchedule(schedule) => envelope(Command::UpdateTaskSchedule(schedule), None),
        Request::DeleteSchedule(schedule_id) => envelope(Command::DeleteTaskSchedule(aurelius_domain::schedule::DeleteTaskSchedule { schedule_id }), None),
        Request::PauseSchedule { schedule_id, paused } => envelope(Command::SetTaskSchedulePaused(aurelius_domain::schedule::SetTaskSchedulePaused { schedule_id, paused }), None),
        Request::RunScheduleNow(schedule_id) => envelope(Command::RunTaskScheduleNow(aurelius_domain::schedule::RunTaskScheduleNow { schedule_id }), None),
        Request::CancelTask(task_id) => envelope(Command::CancelTask(aurelius_domain::task::CancelTask { task_id }), None),
        Request::AssignTask { task_id, employee_id } => envelope(Command::AssignTask(aurelius_domain::task::AssignTask { task_id, employee_id }), None),
        Request::SetDefaults { scope, purpose, settings } => {
            envelope(Command::SetRunDefaults(connection::SetRunDefaults { scope, purpose, settings }), None)
        }
        Request::RemoveConnection(connection_id) => envelope(Command::RemoveProvider(connection::RemoveProvider { connection_id }), None),
        Request::VerifyConnection(connection_id) => envelope(Command::VerifyProvider(connection::VerifyProvider { connection_id }), None),
        // Expanded into two commands by `session`.
        Request::UseConnection { .. } | Request::SetSecret { .. } => return None,
        // Queries, not commands.
        Request::MarkNotificationsRead | Request::ClearResults(_) | Request::LoadMemory(_) | Request::LoadMemoryContext { .. } | Request::LoadContext(_) | Request::LoadScreenshot(_) | Request::LoadFullActivity(..) | Request::LoadFile(_) | Request::LoadUpload(_) | Request::LoadUploadThumb(_) | Request::LoadIdeas(_) | Request::LoadSubagents(_) | Request::LoadMarkdownFile(..) | Request::LoadArtifactContent { .. } | Request::LoadPreviewMessages(_) => return None,
        Request::AddIdea { project_id, title, note, kind, files } => envelope(Command::AddIdea(aurelius_domain::idea::AddIdea { project_id, title, note, kind, files }), None),
        Request::UpdateIdea { idea_id, title, note, kind, add_files, remove_files } => envelope(Command::UpdateIdea(aurelius_domain::idea::UpdateIdea { idea_id, title, note, kind, add_files, remove_files }), None),
        Request::DeleteIdea(idea_id) => envelope(Command::DeleteIdea(aurelius_domain::idea::DeleteIdea { idea_id }), None),
        Request::SetIdeaStatus { idea_id, status } => envelope(Command::SetIdeaStatus(aurelius_domain::idea::SetIdeaStatus { idea_id, status }), None),
        Request::StartIdea { idea_id, employee_id, mode, model } => {
            let (connection_id, model) = model.unzip();
            let run = aurelius_domain::task::RunSettings { mode, connection_id, model, ..Default::default() };
            envelope(Command::StartIdea(aurelius_domain::idea::StartIdea { idea_id, employee_id, run }), None)
        }
        Request::Remember { project_id, kind, content } => envelope(
            Command::Remember(aurelius_domain::memory::Remember { scope: aurelius_domain::memory::MemoryScope::Project(project_id), kind, content, pinned: false }),
            None,
        ),
        Request::DecideMemory { memory_id, accept } => envelope(Command::DecideMemory(aurelius_domain::memory::DecideMemory { memory_id, accept }), None),
        Request::ReviseMemory { memory_id, content } => envelope(Command::ReviseMemory(aurelius_domain::memory::ReviseMemory { memory_id, content }), None),
        Request::ForgetMemory(memory_id) => envelope(Command::ForgetMemory(aurelius_domain::memory::ForgetMemory { memory_id }), None),
        Request::PinMemory { memory_id, pinned } => envelope(Command::PinMemory(aurelius_domain::memory::PinMemory { memory_id, pinned }), None),
        Request::ConsolidateMemory(scope) => envelope(Command::ConsolidateMemory(aurelius_domain::memory::ConsolidateMemory { scope }), None),
        Request::SetAgentMode { conversation_id, mode } => envelope(Command::SetAgentMode(aurelius_domain::autonomy::SetAgentMode { conversation_id, mode }), None),
        Request::SetConversationRun { conversation_id, model, effort } => {
            envelope(Command::SetConversationRun(aurelius_domain::autonomy::SetConversationRun { conversation_id, model, effort }), None)
        }
        Request::SetBrowserAllowed { conversation_id, allowed } => envelope(Command::SetBrowserAllowed(aurelius_domain::autonomy::SetBrowserAllowed { conversation_id, allowed }), None),
        Request::SetScreenAllowed { conversation_id, allowed } => envelope(Command::SetScreenAllowed(aurelius_domain::autonomy::SetScreenAllowed { conversation_id, allowed }), None),
        Request::DecidePlan { approval_id, decision, feedback } => envelope(Command::DecidePlan(aurelius_domain::approval::DecidePlan { approval_id, decision, feedback }), None),
        Request::SetHeartbeat { employee_id, every_minutes } => envelope(
            Command::SetHeartbeat(employee::SetHeartbeat { employee_id, every_minutes, quiet_start_min: None, quiet_end_min: None, daily_token_budget: None }),
            None,
        ),
        Request::DeleteConversation(conversation_id) => envelope(Command::DeleteConversation(conversation::DeleteConversation { conversation_id }), None),
        Request::SetEmployeeFolder { employee_id, path } => envelope(Command::SetEmployeeFolder(employee::SetEmployeeFolder { employee_id, path }), None),
        Request::SetProjectFolder { project_id, path } => envelope(Command::SetProjectFolder(project::SetProjectFolder { project_id, path }), None),
        Request::LoadUsage | Request::LoadUsageHours(_) | Request::LoadSubscription(_) | Request::CheckUpdate(_) | Request::InstallUpdate | Request::InstallAppUpdate | Request::RestartServer | Request::Power(_) | Request::AppKeys(_) | Request::Plugins(_) | Request::LoadServerLoad | Request::SetLoadLimits(_) | Request::LoadBrowser | Request::SetBrowser(_) | Request::SetBrowserBackend(_) | Request::LoadArtifactSetting | Request::SetArtifactsEnabled(_) | Request::LoadUsageTotals { .. } | Request::LoadTraceSetting | Request::SetTraceEnabled(_) | Request::LoadTrace(_) | Request::LoadInstructionVersions(_) | Request::LoadAutoAllow | Request::SetAutoAllow(_) | Request::LoadManagementMode | Request::SetManagementMode(_) | Request::LoadMcp | Request::SetMcp(_) | Request::McpAdd(_) | Request::McpRemove(_) | Request::McpAuth(_) | Request::LoadMcpAlwaysAllow | Request::McpAlwaysAllowRemove(_) | Request::Backups(_) | Request::ExportProject(_) | Request::Remote(_) => return None,
        Request::BrowseDir { .. } | Request::CreateDir { .. } | Request::AddClaudeCode | Request::BoardMembers(_) | Request::Board(_) => return None,
    })
}

/// The connection ended; `Some(reason)` when reconnecting can't help
/// (this computer was removed on the server), and what to show before
/// reconnecting when it was more than a dropped connection (a timeout).
struct Disconnected(Option<String>, Option<String>);

impl From<ClientError> for Disconnected {
    fn from(err: ClientError) -> Self {
        let shown = !err.is_final() && !matches!(err, ClientError::Disconnected);
        Disconnected(err.is_final().then(|| err.to_string()), shown.then(|| err.to_string()))
    }
}

/// A view's answer, or `None` after showing why it failed. Only a lost
/// connection ends the session: reconnecting would load the same view and
/// fail again (a reply too large to send), so the app would never settle.
fn shown<T>(result: Result<T, ClientError>, updates: &mpsc::UnboundedSender<Update>) -> Result<Option<T>, Disconnected> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.is_disconnect() || e.is_final() => Err(e.into()),
        Err(e) => {
            let _ = updates.send(Update::Error(e.to_string()));
            Ok(None)
        }
    }
}

/// Why the group chat did not load, for its page. A server from before the group
/// chat does not know `group_chat.get` (`unknown_method`): it still runs the old
/// version after an update and must be restarted.
pub fn group_chat_failure(e: &ClientError) -> String {
    match e {
        ClientError::Protocol(p) if p.code == aurelius_domain::error::ErrorCode::UnknownMethod => {
            "The server is older than this app and has no Team chat yet. Restart the server (or update it), then try again.".into()
        }
        ClientError::TimedOut(_) => "The server did not answer in time. Try again; if it keeps happening, restart the server.".into(),
        e => format!("The Team chat could not be loaded: {e}"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn session(
    client: &Client,
    requests: &mut mpsc::UnboundedReceiver<Request>,
    events: &mut mpsc::UnboundedReceiver<Event>,
    streams: &mut mpsc::UnboundedReceiver<StreamDelta>,
    updates: &mpsc::UnboundedSender<Update>,
    outbox: &mut VecDeque<CommandEnvelope>,
    open_conversation: &mut Option<ConversationId>,
    open_memory: &mut Option<ProjectId>,
    open_ideas: &mut Option<Option<ProjectId>>,
    me: &mut Option<aurelius_domain::UserId>,
    open_discussion: &mut Option<ConversationId>,
    channels_shown: &mut bool,
) -> Result<(), Disconnected> {
    // Snapshot first, then subscribe from its cursor.
    let (as_of, mut jobs_running) = refresh(client, updates).await?;
    refresh_board_members(client, updates).await?;
    refresh_messages(client, updates, *open_conversation).await?;
    refresh_memory(client, updates, *open_memory).await?;
    refresh_ideas(client, updates, *open_ideas).await?;
    refresh_schedules(client, updates).await?;
    refresh_artifacts(client, updates).await?;
    // Before subscribing, so no load.changed is missed.
    let mut loads = client.load_changes();
    refresh_load(client, updates).await?;
    // The same for power.changed.
    let mut powers = client.power_changes();
    match client.query_power().await {
        Ok(state) => { let _ = updates.send(Update::Power(state)); }
        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
        Err(_) => {}
    }
    client.subscribe(as_of).await.map_err(Disconnected::from)?;

    while let Some(envelope) = outbox.front().cloned() {
        send(client, envelope, updates).await?;
        outbox.pop_front();
    }

    let mut poll = tokio::time::interval(Duration::from_millis(500));
    // A step of a run (or of one of its subagents) writes no event; its stream
    // delta shows it. Reload the managers then, at most once a second.
    let mut steps = tokio::time::interval(Duration::from_secs(1));
    steps.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut marks: std::collections::HashMap<aurelius_domain::RunId, ActivityMark> = std::collections::HashMap::new();
    let mut stepped = false;
    // The dozen snapshots of `refresh` (the full task list among them) at most
    // twice a second while runs write events.
    let mut stale = Coalesce::new(Duration::from_millis(500));
    loop {
        tokio::select! {
            request = requests.recv() => {
                let Some(request) = request else { return Ok(()) };
                if let Request::OpenConversation(id) = request {
                    *open_conversation = id;
                    refresh_messages(client, updates, id).await?;
                    continue;
                }
                if let Request::OpenGroupChat(channel) = request {
                    let asked = match channel {
                        None => client.group_chat().await,
                        Some(c) => read(client, Method::GroupChatGet { channel: Some(c) }, |b| match b { ResponseBody::Messages(m) => Some(m), _ => None }).await,
                    };
                    match asked {
                        Ok(messages) => {
                            *open_conversation = Some(messages.conversation_id);
                            let _ = updates.send(Update::GroupChat(messages.conversation_id));
                            let _ = updates.send(Update::Messages(messages));
                        }
                        Err(e) if e.is_final() => return Err(e.into()),
                        // The page stops loading and says why; a lost connection reconnects as usual.
                        Err(e) => {
                            let _ = updates.send(Update::GroupChatFailed(group_chat_failure(&e)));
                            if e.is_disconnect() {
                                return Err(e.into());
                            }
                        }
                    }
                    continue;
                }
                if let Request::LoadChannels = request {
                    send_channels(client, updates).await?;
                    continue;
                }
                if let Request::FollowChannels(on) = request {
                    *channels_shown = on;
                    if on {
                        send_channels(client, updates).await?;
                    }
                    continue;
                }
                if let Request::LoadMemory(project_id) = request {
                    *open_memory = Some(project_id);
                    refresh_memory(client, updates, Some(project_id)).await?;
                    continue;
                }
                if let Request::LoadMemoryContext { project, manager } = request {
                    let method = Method::MemoryGetContext { project_id: Some(project), employee_id: manager };
                    match read(client, method, |b| match b { ResponseBody::MemoryContext(v) => Some(v), _ => None }).await {
                        Ok(view) => { let _ = updates.send(Update::MemoryContext { project, manager, result: Ok(view) }); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::MemoryContext { project, manager, result: Err(e.to_string()) }); }
                    }
                    continue;
                }
                if let Request::LoadIdeas(project_id) = request {
                    *open_ideas = Some(project_id);
                    refresh_ideas(client, updates, Some(project_id)).await?;
                    continue;
                }
                if let Request::Backups(call) = &request {
                    let restoring = matches!(call, BackupsCall::Restore(_));
                    let answer = match call.clone() {
                        BackupsCall::List => client.backups().await,
                        BackupsCall::Create => {
                            let _ = updates.send(Update::Info("Backing up…".into()));
                            client.create_backup().await
                        }
                        BackupsCall::Restore(id) => client.restore_backup(id).await,
                        BackupsCall::Resume => client.resume_backups().await,
                    };
                    match answer {
                        Ok(snap) => {
                            let restart = restoring && snap.error.is_none() && snap.restore_pending.is_some();
                            let _ = updates.send(Update::Backups(snap));
                            if restart && client.is_remote() {
                                // The server restarts itself and restores; this window reconnects.
                                let _ = updates.send(Update::Info(format!("{} is restoring the backup and restarts; reconnecting…", client.hello.host_name)));
                            } else if restart {
                                // The service restores the backup when it starts again.
                                let _ = updates.send(Update::RestartForUpdate);
                            }
                        }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => {
                            let _ = updates.send(Update::Error(e.to_string()));
                        }
                    }
                    continue;
                }
                if let Request::Remote(call) = &request {
                    use aurelius_protocol::HostWrite;
                    let outcome = match call.clone() {
                        RemoteCall::PairingStart => client.pairing_start().await.map(Update::Pairing),
                        RemoteCall::PushTest(client_id) => client.push_test(client_id).await.map(|()| Update::Info("Test notification sent.".into())),
                        RemoteCall::Status => client.remote_status().await.map(Update::Remote),
                        other => {
                            let write = match other {
                                RemoteCall::Enable => HostWrite::RemoteEnable,
                                RemoteCall::Disable => HostWrite::RemoteDisable,
                                RemoteCall::Decide { pairing_id, approve, owner } => HostWrite::RemoteDecidePairing { pairing_id, approve, owner },
                                RemoteCall::Revoke(client_id) => HostWrite::RemoteRevoke { client_id },
                                RemoteCall::PushConfigure(config) => HostWrite::PushConfigure(config),
                                RemoteCall::Status | RemoteCall::PairingStart | RemoteCall::PushTest(_) => unreachable!(),
                            };
                            client.remote(write).await.map(Update::Remote)
                        }
                    };
                    match outcome {
                        Ok(update) => {
                            let _ = updates.send(update);
                        }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => {
                            let _ = updates.send(Update::Error(e.to_string()));
                        }
                    }
                    continue;
                }
                if let Request::ExportProject(project_id) = request {
                    match client.export_project(project_id).await {
                        Ok((markdown, _)) => {
                            let folder = std::path::Path::new(&markdown).parent().map(|p| p.display().to_string()).unwrap_or_default();
                            let on = if client.is_remote() { format!(" on {}", client.hello.host_name) } else { String::new() };
                            let _ = updates.send(Update::Info(format!("Exported (Markdown + JSON) to {folder}{on}")));
                        }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => {
                            let _ = updates.send(Update::Error(e.to_string()));
                        }
                    }
                    continue;
                }
                if let Request::BrowseDir { path, typed } = request {
                    let ask = if typed { DirAsk::Typed } else { DirAsk::Browse };
                    match client.list_dir(path).await {
                        Ok(listing) => { let _ = updates.send(Update::DirListing(listing)); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::DirFailed(ask, e.to_string())); }
                    }
                    continue;
                }
                if let Request::CreateDir { parent, name } = request {
                    match client.create_dir(parent, name).await {
                        Ok(listing) => { let _ = updates.send(Update::DirListing(listing)); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::DirFailed(DirAsk::Create, e.to_string())); }
                    }
                    continue;
                }
                if let Request::AddClaudeCode = request {
                    let path = client.detect_runtime().await?;
                    let Some(runtime_path) = path else {
                        let place = if client.is_remote() { client.hello.host_name.clone() } else { "this computer".into() };
                        let _ = updates.send(Update::Error(format!("Claude Code was not found on {place}. Install it there (claude.com/code) and try again.")));
                        continue;
                    };
                    let env = envelope(
                        Command::AddProvider(connection::AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude Code".into(), runtime_path: Some(runtime_path), default_model: None }),
                        None,
                    );
                    outbox.push_back(env.clone());
                    send(client, env, updates).await?;
                    outbox.pop_front();
                    let _ = updates.send(Update::Info("Connection added; verifying…".into()));
                    continue;
                }
                if let Request::CheckUpdate(check) = request {
                    let s = client.update_status(check).await.map_err(Disconnected::from)?;
                    let _ = updates.send(Update::UpdateStatus(s));
                    continue;
                }
                if let Request::InstallUpdate = request {
                    let _ = updates.send(Update::Info("Downloading and verifying the server update…".into()));
                    let prepared = client.prepare_update().await.map_err(Disconnected::from)?;
                    let ready = prepared.staged.is_some() && prepared.error.is_none();
                    let _ = updates.send(Update::UpdateStatus(prepared.clone()));
                    if !ready {
                        let _ = updates.send(Update::Error(prepared.error.unwrap_or_else(|| "No newer version available.".into())));
                        continue;
                    }
                    match client.apply_update().await {
                        // The server updated itself and restarts; this app stays and reconnects.
                        Ok(_) | Err(ClientError::Disconnected) => {
                            let _ = updates.send(Update::Info(format!("{} is updated and restarts; reconnecting…", client.hello.host_name)));
                        }
                        Err(e) if e.is_disconnect() => {
                            let _ = updates.send(Update::Info(format!("{} is updated and restarts; reconnecting…", client.hello.host_name)));
                        }
                        Err(e) => {
                            let _ = updates.send(Update::Error(e.to_string()));
                        }
                    }
                    continue;
                }
                if let Request::InstallAppUpdate = request {
                    // The local service installs the app; a remote server's computer has its own.
                    if client.is_remote() {
                        let _ = updates.send(Update::Error("Update this app with scripts\\install-production.ps1 -Role client.".into()));
                        continue;
                    }
                    let _ = updates.send(Update::Info("Downloading and verifying the app update…".into()));
                    let prepared = client.prepare_app_update().await.map_err(Disconnected::from)?;
                    let ready = prepared.staged.is_some() && prepared.error.is_none();
                    let _ = updates.send(Update::AppUpdateStatus(prepared.clone()));
                    if !ready {
                        let _ = updates.send(Update::Error(prepared.error.unwrap_or_else(|| "No newer app version available.".into())));
                        continue;
                    }
                    match client.apply_app_update().await {
                        // The service keeps running; only this window restarts.
                        Ok(_) => {
                            let _ = updates.send(Update::RestartForUpdate);
                        }
                        Err(e) => {
                            let _ = updates.send(Update::Error(e.to_string()));
                        }
                    }
                    continue;
                }
                if let Request::RestartServer = request {
                    match client.restart_server().await {
                        // The server answers, then goes down; the reconnect loop takes over.
                        Ok(_) | Err(ClientError::Disconnected) => {
                            let _ = updates.send(Update::Info(format!("{} restarts; reconnecting…", client.hello.host_name)));
                        }
                        Err(e) if e.is_disconnect() => {
                            let _ = updates.send(Update::Info(format!("{} restarts; reconnecting…", client.hello.host_name)));
                        }
                        Err(e) if e.is_final() => return Err(e.into()),
                        Err(e) => {
                            let _ = updates.send(Update::Error(e.to_string()));
                        }
                    }
                    continue;
                }
                if let Request::AppKeys(call) = request {
                    handle_app_keys(client, call, updates).await?;
                    continue;
                }
                if let Request::Plugins(call) = request {
                    handle_plugins(client, call, updates).await?;
                    continue;
                }
                if let Request::Power(call) = request {
                    let result = match call {
                        PowerCall::Shutdown(when) => client.power_shutdown(when).await,
                        PowerCall::Cancel => client.power_cancel().await,
                    };
                    match result {
                        Ok(state) => { let _ = updates.send(Update::Power(state)); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::Error(e.to_string())); }
                    }
                    continue;
                }
                if let Request::McpAdd(params) = request {
                    match client.mcp_add(params.clone()).await {
                        Ok(list) => {
                            let _ = updates.send(Update::Mcp(Ok(list)));
                            let _ = updates.send(Update::McpAdded);
                        }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::McpAddFailed(crate::screens::mcp_add::scrub(&mcp_manage_error(e), &params))); }
                    }
                    continue;
                }
                if let Request::McpRemove(name) = request {
                    match client.mcp_remove(&name).await {
                        Ok(list) => { let _ = updates.send(Update::Mcp(Ok(list))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::McpRemoveFailed(mcp_manage_error(e))); }
                    }
                    continue;
                }
                if let Request::McpAuth(call) = request {
                    let result = match call {
                        McpAuthCall::Start(name) => client.mcp_login_start(&name).await.map(Update::McpLogin),
                        McpAuthCall::Complete { login_id, redirect_url } => client.mcp_login_complete(&login_id, &redirect_url).await.map(Update::McpLogin),
                        McpAuthCall::Logout(name) => client.mcp_logout(&name).await.map(|list| Update::Mcp(Ok(list))),
                    };
                    match result {
                        Ok(update) => { let _ = updates.send(update); }
                        Err(e) if e.is_final() => return Err(e.into()),
                        // An older server drops the connection on a method it does not know.
                        Err(e) if e.is_disconnect() => {
                            let _ = updates.send(Update::McpAuthFailed(crate::screens::mcp_add::TOO_OLD.into()));
                            return Err(e.into());
                        }
                        Err(ClientError::Protocol(p)) if p.code == aurelius_protocol::ErrorCode::Forbidden => {
                            let _ = updates.send(Update::McpAuthFailed(crate::screens::mcp_add::NO_APPROVE.into()));
                        }
                        Err(e) => { let _ = updates.send(Update::McpAuthFailed(e.to_string())); }
                    }
                    continue;
                }
                if let Request::McpAlwaysAllowRemove(tool) = request {
                    match client.mcp_always_allow_remove(&tool).await {
                        Ok(list) => { let _ = updates.send(Update::McpAlwaysAllow(Some(list.tools))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::McpAlwaysAllowRemoveFailed(always_allow_error(e))); }
                    }
                    continue;
                }
                if matches!(request, Request::LoadMcpAlwaysAllow) {
                    load_always_allow(client, updates).await?;
                    continue;
                }
                if matches!(request, Request::LoadMcp | Request::SetMcp(_)) {
                    let saving = matches!(request, Request::SetMcp(_));
                    let result = match request {
                        Request::SetMcp(names) => client.set_mcp(names).await,
                        _ => client.mcp_list().await,
                    };
                    match result {
                        Ok(list) => { let _ = updates.send(Update::Mcp(Ok(list))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) if saving => { let _ = updates.send(Update::McpSaveFailed(mcp_error(e, true))); }
                        Err(e) => { let _ = updates.send(Update::Mcp(Err(mcp_error(e, false)))); }
                    }
                    continue;
                }
                if matches!(request, Request::LoadBrowser | Request::SetBrowser(_) | Request::SetBrowserBackend(_)) {
                    let result = match request {
                        Request::SetBrowser(choice) => client.set_browser(choice).await,
                        Request::SetBrowserBackend(backend) => client.set_browser_backend(backend).await,
                        _ => client.browser_setting().await,
                    };
                    match result {
                        Ok(setting) => { let _ = updates.send(Update::Browser(Ok(setting))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::Browser(Err(browser_error(e)))); }
                    }
                    continue;
                }
                if matches!(request, Request::LoadArtifactSetting | Request::SetArtifactsEnabled(_)) {
                    let saving = matches!(request, Request::SetArtifactsEnabled(_));
                    let result = match request {
                        Request::SetArtifactsEnabled(on) => client.set_artifacts_enabled(on).await,
                        _ => client.artifact_setting().await,
                    };
                    match result {
                        Ok(setting) => { let _ = updates.send(Update::ArtifactSetting(Some(setting.enabled))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(ClientError::Protocol(p)) if saving && p.code == aurelius_protocol::ErrorCode::Forbidden => { let _ = updates.send(Update::Error("You need the Approve right on this server to turn artifacts on or off.".into())); }
                        Err(e) if saving => { let _ = updates.send(Update::Error(e.to_string())); }
                        Err(_) => { let _ = updates.send(Update::ArtifactSetting(None)); }
                    }
                    continue;
                }
                if let Request::LoadUsageTotals { conversation_id, task_id } = request {
                    let conversation = task_id.is_none().then_some(conversation_id);
                    match client.usage_totals(conversation, task_id).await {
                        Ok(totals) => { let _ = updates.send(Update::UsageTotals(conversation_id, totals)); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        // Shown nowhere: the header just has no total.
                        Err(_) => {}
                    }
                    continue;
                }
                if matches!(request, Request::LoadTraceSetting | Request::SetTraceEnabled(_)) {
                    let saving = matches!(request, Request::SetTraceEnabled(_));
                    let result = match request {
                        Request::SetTraceEnabled(on) => client.set_trace_enabled(on).await,
                        _ => client.trace_setting().await,
                    };
                    match result {
                        Ok(setting) => { let _ = updates.send(Update::TraceSetting(Some(setting))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(ClientError::Protocol(p)) if saving && p.code == aurelius_protocol::ErrorCode::Forbidden => { let _ = updates.send(Update::Error("You need the Approve right on this server to turn trace mode on or off.".into())); }
                        Err(e) if saving => { let _ = updates.send(Update::Error(e.to_string())); }
                        Err(_) => { let _ = updates.send(Update::TraceSetting(None)); }
                    }
                    continue;
                }
                if let Request::LoadTrace(run_id) = request {
                    match client.run_trace(run_id).await {
                        Ok(trace) => { let _ = updates.send(Update::Trace(run_id, Ok(trace))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(ClientError::Protocol(p)) if p.code == aurelius_protocol::ErrorCode::NotFound => {
                            let _ = updates.send(Update::Trace(run_id, Err("No trace for this answer: trace mode was off when it ran, or it is older than the latest 200 runs / 7 days.".into())));
                        }
                        Err(e) => { let _ = updates.send(Update::Trace(run_id, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadInstructionVersions(employee_id) = request {
                    match client.employee_instruction_versions(employee_id).await {
                        Ok(list) => { let _ = updates.send(Update::InstructionVersions(employee_id, Ok(list))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::InstructionVersions(employee_id, Err(e.to_string()))); }
                    }
                    continue;
                }
                if matches!(request, Request::LoadAutoAllow | Request::SetAutoAllow(_)) {
                    let saving = matches!(request, Request::SetAutoAllow(_));
                    let result = match request {
                        Request::SetAutoAllow(on) => client.set_auto_allow_enabled(on).await,
                        _ => client.auto_allow_setting().await,
                    };
                    match result {
                        Ok(setting) => { let _ = updates.send(Update::AutoAllow(Some(setting))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(ClientError::Protocol(p)) if saving && p.code == aurelius_protocol::ErrorCode::Forbidden => { let _ = updates.send(Update::Error("You need the Approve right on this server to change auto-allow.".into())); }
                        Err(e) if saving => { let _ = updates.send(Update::Error(e.to_string())); }
                        Err(_) => { let _ = updates.send(Update::AutoAllow(None)); }
                    }
                    continue;
                }
                if matches!(request, Request::LoadManagementMode | Request::SetManagementMode(_)) {
                    let saving = matches!(request, Request::SetManagementMode(_));
                    let result = match request {
                        Request::SetManagementMode(on) => client.set_management_mode_enabled(on).await,
                        _ => client.management_mode_setting().await,
                    };
                    match result {
                        Ok(setting) => { let _ = updates.send(Update::ManagementMode(Some(setting.enabled))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(ClientError::Protocol(p)) if saving && p.code == aurelius_protocol::ErrorCode::Forbidden => { let _ = updates.send(Update::Error("You need the Approve right on this server to change who chooses the mode.".into())); }
                        Err(e) if saving => { let _ = updates.send(Update::Error(e.to_string())); }
                        Err(_) => { let _ = updates.send(Update::ManagementMode(None)); }
                    }
                    continue;
                }
                if let Request::LoadServerLoad = request {
                    refresh_load(client, updates).await?;
                    continue;
                }
                if let Request::SetLoadLimits(update) = request {
                    match client.set_load_limits(update).await {
                        Ok(snapshot) => { let _ = updates.send(Update::LoadSaved(Ok(snapshot))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::LoadSaved(Err(load_error(e)))); }
                    }
                    continue;
                }
                if let Request::LoadSubscription(refresh) = request {
                    let s = client.subscription(refresh).await.map_err(Disconnected::from)?;
                    let _ = updates.send(Update::Subscription(s));
                    continue;
                }
                if let Request::LoadUsage = request {
                    let usage = client.usage().await.map_err(Disconnected::from)?;
                    let _ = updates.send(Update::Usage(usage));
                    continue;
                }
                if let Request::LoadUsageHours(day) = request {
                    match client.query_usage_hours(day).await {
                        Ok(hours) => { let _ = updates.send(Update::UsageHours(day, Ok(hours))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::UsageHours(day, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadPreviewMessages(conversation) = request {
                    match client.messages(conversation).await {
                        Ok(messages) => { let _ = updates.send(Update::PreviewMessages(messages)); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(_) => {}
                    }
                    continue;
                }
                if let Request::LoadSubagents(conversation) = request {
                    match client.subagents(conversation).await {
                        Ok(agents) => { let _ = updates.send(Update::Subagents(conversation, agents)); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(_) => {}
                    }
                    continue;
                }
                if let Request::LoadMarkdownFile(scope, path) = request {
                    match client.markdown_file(scope.clone(), &path).await {
                        Ok(file) => { let _ = updates.send(Update::MarkdownFile(scope, path, Ok(file))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::MarkdownFile(scope, path, Err(MarkdownFailure::of(&e)))); }
                    }
                    continue;
                }
                if let Request::LoadArtifactContent { url, version } = request {
                    match client.artifact_content(url.clone(), version).await {
                        Ok(content) => { let _ = updates.send(Update::ArtifactContent(url, version, Ok(content))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(ClientError::Protocol(p)) if p.code == aurelius_protocol::ErrorCode::NotFound => {
                            let _ = updates.send(Update::ArtifactContent(url, version, Err("This artifact is no longer on the server (its conversation may have been deleted).".into())));
                        }
                        Err(e) => { let _ = updates.send(Update::ArtifactContent(url, version, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadFile(id) = request {
                    match client.file(&id).await {
                        Ok(file) => { let _ = updates.send(Update::File(id, Ok(file))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::File(id, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadUpload(id) = request {
                    match client.upload(&id).await {
                        Ok(file) => { let _ = updates.send(Update::File(id, Ok(file))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::File(id, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadUploadThumb(id) = request {
                    match client.upload_thumbnail(&id, UPLOAD_THUMB_SIDE).await {
                        Ok(bytes) => { let _ = updates.send(Update::UploadThumb(id, Ok(bytes))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::UploadThumb(id, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadScreenshot(id) = request {
                    match client.screenshot(&id).await {
                        Ok(bytes) => { let _ = updates.send(Update::Screenshot(id, Ok(bytes))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::Screenshot(id, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadFullActivity(message_id, call_id) = request {
                    match client.activity(message_id, &call_id).await {
                        Ok(view) => { let _ = updates.send(Update::FullActivity(call_id, Ok(view))); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::FullActivity(call_id, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadContext(run_id) = request {
                    match client.context(run_id).await {
                        Ok(view) => { let _ = updates.send(Update::Context(view)); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::Error(e.to_string())); }
                    }
                    continue;
                }
                if let Request::MarkNotificationsRead = request {
                    client.mark_notifications_read().await.map_err(Disconnected::from)?;
                    let notifications = client.notifications().await.map_err(Disconnected::from)?;
                    let _ = updates.send(Update::Notifications(notifications));
                    continue;
                }
                if let Request::ClearResults(_) = request {
                    let outcome = client.dismiss_all_notifications().await.map(|_| ());
                    match outcome {
                        Ok(()) => {}
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::Error(e.to_string())); }
                    }
                    // Show what the server really has (also puts back what failed).
                    let notifications = client.notifications().await.map_err(Disconnected::from)?;
                    let _ = updates.send(Update::Notifications(notifications));
                    continue;
                }
                if let Request::BoardMembers(call) = request {
                    board_members(client, call, me, updates).await?;
                    continue;
                }
                if let Request::Board(call) = request {
                    if let BoardCall::Discussion(conversation) = call {
                        *open_discussion = conversation;
                    }
                    board(client, call, updates).await?;
                    continue;
                }
                if let Request::SetSecret { connection_id, secret } = request {
                    match client.set_secret(connection_id, secret).await {
                        Ok(_) => { let _ = updates.send(Update::Info("API key saved in the OS credential store. Checking the connection…".into())); }
                        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::Error(e.to_string())); }
                    }
                    continue;
                }
                if let Request::UseConnection { connection_id, model } = request {
                    let model = model.map(aurelius_domain::employee::Setting::Value).unwrap_or(aurelius_domain::employee::Setting::Inherit);
                    // All purposes: otherwise some keep running on whatever they had (the test runtime after a fresh install).
                    for purpose in [Purpose::Employee, Purpose::Memory] {
                        let settings = RunDefaults { connection: aurelius_domain::employee::Setting::Value(connection_id), model: model.clone(), persona: aurelius_domain::employee::Setting::Inherit, effort: aurelius_domain::employee::Setting::Inherit };
                        let env = envelope(Command::SetRunDefaults(connection::SetRunDefaults { scope: ConfigScope::Global, purpose, settings }), None);
                        outbox.push_back(env.clone());
                        send(client, env, updates).await?;
                        outbox.pop_front();
                    }
                    let _ = updates.send(Update::Info("Employees and memory now use this connection.".into()));
                    continue;
                }
                let mut request = request;
                if let Request::SendManager { images, .. } | Request::SendGroup { images, .. } = &mut request {
                    // Images on this computer: as they are for the local service, uploaded for a remote one.
                    let mut staged = Vec::new();
                    for path in images.iter() {
                        match client.stage_file(std::path::Path::new(path)).await {
                            Ok(path) => staged.push(path),
                            Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                            Err(e) => {
                                let _ = updates.send(Update::Error(format!("Could not send the image {path}: {e}")));
                                break;
                            }
                        }
                    }
                    if staged.len() < images.len() {
                        continue;
                    }
                    *images = staged;
                }
                if let Request::SendManager { files, .. } | Request::SendGroup { files, .. } = &mut request {
                    // Files on this computer: as they are for the local service, uploaded for a remote one.
                    let mut staged = Vec::new();
                    for path in files.iter() {
                        match client.stage_file(std::path::Path::new(path)).await {
                            Ok(path) => staged.push(path),
                            Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                            Err(e) => {
                                let _ = updates.send(Update::Error(format!("Could not send the file {path}: {e}")));
                                break;
                            }
                        }
                    }
                    if staged.len() < files.len() {
                        continue;
                    }
                    *files = staged;
                }
                // Task files: as they are for the local service, uploaded for a remote one.
                let files = match &mut request {
                    Request::CreateTask { files, .. } if !files.is_empty() => Some(files),
                    Request::CreateSchedule(s) if !s.files.is_empty() => Some(&mut s.files),
                    Request::UpdateSchedule(s) if !s.add_files.is_empty() => Some(&mut s.add_files),
                    Request::AddIdea { files, .. } if !files.is_empty() => Some(files),
                    Request::UpdateIdea { add_files, .. } if !add_files.is_empty() => Some(add_files),
                    _ => None,
                };
                if let Some(files) = files {
                    let mut staged = Vec::new();
                    for path in files.iter() {
                        match client.stage_file(std::path::Path::new(path)).await {
                            Ok(path) => staged.push(path),
                            Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
                            Err(e) => {
                                let _ = updates.send(Update::Error(format!("Could not send the file {path}: {e}")));
                                break;
                            }
                        }
                    }
                    if staged.len() < files.len() {
                        continue;
                    }
                    *files = staged;
                }
                let Some(envelope) = to_envelope(request) else { continue };
                outbox.push_back(envelope.clone());
                send(client, envelope, updates).await?;
                outbox.pop_front();
            }
            delta = streams.recv() => {
                if let Some(delta) = delta {
                    let mark = activity_mark(&delta.activity);
                    if marks.get(&delta.run_id) != Some(&mark) {
                        if marks.len() > 64 {
                            marks.clear();
                        }
                        marks.insert(delta.run_id, mark);
                        stepped = true;
                    }
                    let _ = updates.send(Update::Stream(delta));
                }
            }
            snapshot = loads.recv() => {
                // The sender lives in `client`, so this only ends with the session.
                let Some(snapshot) = snapshot else { return Err(client.end_reason().into()) };
                let _ = updates.send(Update::Load(Ok(snapshot)));
            }
            state = powers.recv() => {
                let Some(state) = state else { return Err(client.end_reason().into()) };
                let _ = updates.send(Update::Power(state));
            }
            _ = stale.due(), if stale.marked => {
                jobs_running = refresh(client, updates).await?.1;
            }
            _ = steps.tick(), if stepped => {
                stepped = false;
                send_company(updates, company(client).await.map_err(Disconnected::from)?);
            }
            event = events.recv() => {
                let Some(first) = event else { return Err(client.end_reason().into()) };
                // Drain bursts, then refresh the affected read models once.
                let is_chat = |e: &Event| matches!(e.kind.area(), "conversation" | "message" | "run");
                let is_artifact = |e: &Event| e.kind == aurelius_protocol::EventKind::ArtifactPublished;
                let (mut chat, mut other, mut mcp, mut approval, mut artifact, mut logins) = (false, false, false, false, false, Vec::new());
                // These areas change only their own list, not the dozen of `refresh`;
                // ideas and recurring tasks also show their task's status.
                let (mut memory, mut ideas, mut schedules, mut task, mut users, mut settings, mut tickets) = (false, false, false, false, false, false, false);
                let mut board_tasks: Vec<String> = Vec::new();
                for e in std::iter::once(first).chain(std::iter::from_fn(|| events.try_recv().ok())) {
                    // 'Always allow' on an approval adds its tool to the always-allowed list.
                    approval |= e.kind.area() == "approval";
                    // A posted update (a question, a plan, a worker's report) is a message in the open conversation too.
                    chat |= e.kind == aurelius_domain::EventKind::TaskPosted;
                    // A deleted conversation's artifacts leave the list (the chat lists refresh as usual).
                    artifact |= e.kind == aurelius_protocol::EventKind::ConversationDeleted;
                    // So do the tasks: a task keeps no deleted conversation (its employee's page lists it by them).
                    other |= e.kind == aurelius_protocol::EventKind::ConversationDeleted;
                    if e.kind == aurelius_domain::EventKind::McpLoginFinished {
                        logins.push(e.aggregate_id.clone());
                        mcp = true;
                    } else if e.kind.area() == "mcp" {
                        mcp = true;
                    } else if matches!(e.kind.area(), "memory" | "share") {
                        memory = true;
                    } else if e.kind.area() == "idea" {
                        ideas = true;
                    } else if e.kind.area() == "recurring_task" {
                        schedules = true;
                    } else if is_artifact(&e) { artifact = true } else if is_chat(&e) { chat = true } else { other = true }
                    task |= e.kind.area() == "task";
                    users |= e.kind.area() == "user";
                    // A card follows its main task (the app keeps those of tickets); the dispatcher follows the company and its projects.
                    settings |= matches!(e.kind.area(), "company" | "project");
                    tickets |= matches!(e.kind.area(), "ticket" | "board");
                    if e.kind.area() == "task" && !board_tasks.contains(&e.aggregate_id) {
                        board_tasks.push(e.aggregate_id.clone());
                    }
                }
                if tickets || settings || !board_tasks.is_empty() {
                    let _ = updates.send(Update::Board(BoardUpdate::Changed { tickets, settings, tasks: board_tasks }));
                }
                if users {
                    // Another window made a board member: the onboarding may be over (MU1-20 AC4).
                    send_who(client, updates).await?;
                    refresh_board_members(client, updates).await?;
                }
                if artifact {
                    refresh_artifacts(client, updates).await?;
                }
                if approval {
                    load_always_allow(client, updates).await?;
                }
                if mcp {
                    let list = client.mcp_list().await.map_err(Disconnected::from)?;
                    let _ = updates.send(Update::Mcp(Ok(list)));
                    for id in logins {
                        if let Ok(login) = client.mcp_login_get(&id).await {
                            let _ = updates.send(Update::McpLogin(login));
                        }
                    }
                }
                if other {
                    stale.mark();
                }
                if memory {
                    // The window reloads it only while the Memory tab shows.
                    let _ = updates.send(Update::MemoryChanged);
                }
                if ideas || task {
                    refresh_ideas(client, updates, *open_ideas).await?;
                }
                if schedules || task {
                    refresh_schedules(client, updates).await?;
                }
                if chat {
                    refresh_messages(client, updates, *open_conversation).await?;
                    // A ticket's discussion and the channels' counts follow their messages (AUR-MU-02).
                    if let Some(conversation) = *open_discussion {
                        board(client, BoardCall::Discussion(Some(conversation)), updates).await?;
                    }
                    if *channels_shown {
                        send_channels(client, updates).await?;
                    }
                    // Presence ("working") of employees derives from runs.
                    send_company(updates, company(client).await.map_err(Disconnected::from)?);
                    let usage = client.usage().await.map_err(Disconnected::from)?;
                    let _ = updates.send(Update::Usage(usage));
                }
            }
            _ = poll.tick(), if jobs_running => {
                // Job progress is in-memory in the daemon; poll only while a job runs (idle stays idle).
                let jobs = client.jobs().await.map_err(Disconnected::from)?.jobs;
                jobs_running = any_running(&jobs);
                let _ = updates.send(Update::Jobs(jobs));
            }
        }
    }
}

/// Steps started and steps finished: changes with each step a run (or one of
/// its subagents) takes, not with its text.
pub type ActivityMark = (usize, usize);

pub fn activity_mark(activity: &[aurelius_protocol::ActivityView]) -> ActivityMark {
    (activity.len(), activity.iter().filter(|a| a.ok.is_some()).count())
}

/// Marks within one period end in one refresh: the first at once, the next
/// one period later, so the last mark is always followed by a refresh.
struct Coalesce {
    marked: bool,
    tick: tokio::time::Interval,
}

impl Coalesce {
    fn new(period: Duration) -> Self {
        let mut tick = tokio::time::interval(period);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Self { marked: false, tick }
    }

    fn mark(&mut self) {
        self.marked = true;
    }

    /// Ready when a marked refresh may run; poll only while `marked`.
    async fn due(&mut self) {
        self.tick.tick().await;
        self.marked = false;
    }
}

/// The hello's answer for this window: with the board member it remembers,
/// active again when the server still has it (none: "Who's working here?").
async fn identify(client: &Client, me: Option<aurelius_domain::UserId>) -> Result<aurelius_domain::user::WhoAmI, ClientError> {
    let hello = match me {
        Some(user_id) => client.identify(user_id).await?,
        None => client.hello.clone(),
    };
    Ok(aurelius_domain::user::WhoAmI { board_members_exist: hello.board_members_exist, user: hello.user })
}

async fn send_who(client: &Client, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    if let Some(who) = shown(client.user_whoami().await, updates)? {
        let _ = updates.send(Update::Who(who));
    }
    Ok(())
}

async fn refresh_board_members(client: &Client, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    if let Some(list) = shown(client.user_list().await, updates)? {
        let _ = updates.send(Update::BoardMembers(list.users));
    }
    Ok(())
}

/// `user.*` for the board member screens. The password goes to the server and
/// is dropped with the call; a refusal goes back to the form that asked.
async fn board_members(client: &Client, call: UserCall, me: &mut Option<aurelius_domain::UserId>, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    // The form always hears back, also when the connection drops (Trace M3a m1):
    // else it stays on "Creating…" / "Checking…". No retry: after the reconnect the
    // list shows a board member the server made anyway.
    let refused = |err: ClientError| -> Result<(), Disconnected> {
        let _ = updates.send(Update::BoardMemberFailed(member_call_failure(&err)));
        if err.is_disconnect() || err.is_final() {
            return Err(err.into());
        }
        Ok(())
    };
    match call {
        UserCall::List => refresh_board_members(client, updates).await,
        UserCall::Create { username, password } => {
            let created = match client.user_create(&username, &password.0).await {
                Ok(receipt) => receipt,
                Err(err) => return refused(err),
            };
            drop(password);
            let who = client.user_whoami().await.map_err(Disconnected::from)?;
            let new_id = match created.result {
                CommandResult::UserCreated { user_id } => Some(user_id),
                _ => None,
            };
            if new_id.is_some() && who.user.as_ref().map(|u| u.id) == new_id {
                // The onboarding: this window works as the new board member.
                *me = new_id;
            } else {
                let _ = updates.send(Update::BoardMemberCreated(username.trim().to_string()));
            }
            let _ = updates.send(Update::Who(who));
            refresh_board_members(client, updates).await
        }
        UserCall::Use { user_id, password } => {
            let result = client.user_use(user_id, &password.0).await;
            drop(password);
            match result {
                Ok(who) => {
                    *me = who.user.as_ref().map(|u| u.id);
                    let _ = updates.send(Update::Who(who));
                    Ok(())
                }
                Err(err) => refused(err),
            }
        }
    }
}

/// One board call. A read that fails goes to the page; a write that fails
/// goes back to the board, also when the connection drops on the way (the
/// session then reconnects as usual). Writes are never sent twice.
async fn board(client: &Client, call: BoardCall, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    let lost = |e: &ClientError| e.is_disconnect() || e.is_final();
    match call {
        BoardCall::Load(project_id) => {
            let result = read(client, Method::BoardGet { project_id }, |b| match b { ResponseBody::Board(v) => Some(v), _ => None }).await;
            if let Err(e) = &result && lost(e) {
                return Err(result.unwrap_err().into());
            }
            let _ = updates.send(Update::Board(BoardUpdate::Loaded(project_id, result.map_err(|e| board_failure(&e)))));
        }
        BoardCall::Ticket(ticket_id) => {
            let result = read(client, Method::TicketGet { ticket_id }, |b| match b { ResponseBody::Ticket(v) => Some(v), _ => None }).await;
            if let Err(e) = &result && lost(e) {
                return Err(result.unwrap_err().into());
            }
            let _ = updates.send(Update::Board(BoardUpdate::Ticket(ticket_id, result.map_err(|e| board_failure(&e)))));
        }
        BoardCall::Plan(conversation) => match client.messages(conversation).await {
            Ok(messages) => {
                let _ = updates.send(Update::Board(BoardUpdate::Plan(conversation, latest_plan(&messages.messages), Some(messages.agent_mode))));
            }
            Err(e) if lost(&e) => return Err(e.into()),
            Err(_) => {
                let _ = updates.send(Update::Board(BoardUpdate::Plan(conversation, None, None)));
            }
        },
        BoardCall::Discussion(None) => {}
        BoardCall::Discussion(Some(conversation)) => match client.messages(conversation).await {
            Ok(snapshot) => {
                let _ = updates.send(Update::Board(BoardUpdate::Discussion(conversation, Ok(snapshot.messages))));
            }
            Err(e) if lost(&e) => return Err(e.into()),
            Err(e) => {
                let _ = updates.send(Update::Board(BoardUpdate::Discussion(conversation, Err(crate::screens::discussion::load_failure(&e)))));
            }
        },
        // The discussion's writes answer the discussion: the draft stays when they fail.
        BoardCall::Write { command: Command::PostTicketMessage(post), .. } => {
            let ticket = post.ticket_id;
            match client.execute(envelope(Command::PostTicketMessage(post), None)).await {
                Ok(receipt) => {
                    // Whether the owner took it in the main task, the window reads from `to`
                    // and the ticket (`discussion::reached`): `run_ids` cannot tell, the
                    // owner may have a run there or not.
                    let to = match receipt.result {
                        CommandResult::TicketMessagePosted { to, .. } => to,
                        _ => Vec::new(),
                    };
                    let _ = updates.send(Update::Board(BoardUpdate::Posted { ticket, to }));
                }
                Err(e) if needs_board_member(&e) => {
                    let _ = updates.send(Update::Board(BoardUpdate::PostFailed { ticket, message: BOARD_MEMBER_REQUIRED.into() }));
                    let _ = updates.send(Update::BoardMemberRequired);
                }
                Err(e) => {
                    let _ = updates.send(Update::Board(BoardUpdate::PostFailed { ticket, message: crate::screens::discussion::post_failure(&e) }));
                    if lost(&e) {
                        return Err(e.into());
                    }
                }
            }
        }
        BoardCall::Write { command: Command::RetryTicketSummary(retry), .. } => {
            let ticket = retry.ticket_id;
            match client.execute(envelope(Command::RetryTicketSummary(retry), None)).await {
                Ok(_) => {
                    let _ = updates.send(Update::Board(BoardUpdate::SummaryRetried { ticket, failure: None }));
                }
                Err(e) if needs_board_member(&e) => {
                    let _ = updates.send(Update::Board(BoardUpdate::SummaryRetried { ticket, failure: Some(BOARD_MEMBER_REQUIRED.into()) }));
                    let _ = updates.send(Update::BoardMemberRequired);
                }
                Err(e) => {
                    let _ = updates.send(Update::Board(BoardUpdate::SummaryRetried { ticket, failure: Some(crate::screens::discussion::retry_failure(&e)) }));
                    if lost(&e) {
                        return Err(e.into());
                    }
                }
            }
        }
        BoardCall::Activity(ticket_id) => {
            let query = aurelius_protocol::TimelineQuery { correlation_id: Some(ticket_id.to_string()), limit: ACTIVITY_LIMIT, activity: true, ..Default::default() };
            let result = client.timeline(query).await;
            if let Err(e) = &result && lost(e) {
                return Err(result.unwrap_err().into());
            }
            let _ = updates.send(Update::Board(BoardUpdate::Activity(ticket_id, result.map(|p| p.cards).map_err(|e| board_failure(&e)))));
        }
        BoardCall::GaveUp { ticket, version } => {
            let query = aurelius_protocol::TimelineQuery { correlation_id: Some(ticket.to_string()), limit: crate::screens::board::GAVE_UP_LIMIT, activity: true, ..Default::default() };
            let result = client.timeline(query).await;
            if let Err(e) = &result && lost(e) {
                return Err(result.unwrap_err().into());
            }
            let gave_up = result.ok().map(|p| crate::screens::board::dispatcher_gave_up(&p.cards));
            let _ = updates.send(Update::Board(BoardUpdate::GaveUp { ticket, version, gave_up }));
        }
        BoardCall::Write { mut command, expected_version, done } => {
            let ticket = ticket_of(&command);
            // A new ticket's files: as they are for the local service, uploaded for a remote one.
            if let Command::CreateTicket(create) = &mut command {
                let mut staged = Vec::new();
                for path in &create.files {
                    match client.stage_file(std::path::Path::new(path)).await {
                        Ok(path) => staged.push(path),
                        Err(e) => {
                            let name = std::path::Path::new(path).file_name().map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
                            let message = if lost(&e) { BOARD_CONNECTION_DROPPED.to_string() } else { format!("Could not send the file {name}: {e}") };
                            let _ = updates.send(Update::Board(BoardUpdate::Failed { ticket: None, message }));
                            return if lost(&e) { Err(e.into()) } else { Ok(()) };
                        }
                    }
                }
                create.files = staged;
            }
            match client.execute(envelope(command, expected_version)).await {
                Ok(receipt) => {
                    let _ = updates.send(Update::Board(match receipt.result {
                        CommandResult::TicketCreated { number, .. } => BoardUpdate::Created { number, text: done },
                        _ => BoardUpdate::Done { ticket, text: done },
                    }));
                }
                Err(e) if needs_board_member(&e) => {
                    let _ = updates.send(Update::Board(BoardUpdate::Failed { ticket, message: BOARD_MEMBER_REQUIRED.into() }));
                    let _ = updates.send(Update::BoardMemberRequired);
                }
                Err(e) => {
                    let _ = updates.send(Update::Board(BoardUpdate::Failed { ticket, message: board_write_failure(&e) }));
                    if lost(&e) {
                        return Err(e.into());
                    }
                }
            }
        }
    }
    Ok(())
}

/// The Team chat's channels (`channel.list`): none from a server without channels
/// (`unknown_method`, or any other refusal: the Team chat then is the one general chat).
async fn send_channels(client: &Client, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    let result = read(client, Method::ChannelList, |b| match b { ResponseBody::Channels(c) => Some(c), _ => None }).await;
    match result {
        Ok(snapshot) => {
            let _ = updates.send(Update::Channels(Some(snapshot.channels)));
        }
        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
        Err(_) => {
            let _ = updates.send(Update::Channels(None));
        }
    }
    Ok(())
}

/// The newest plan posted in a conversation (a ticket's plan review posts it there).
pub fn latest_plan(messages: &[aurelius_protocol::MessageView]) -> Option<String> {
    messages.iter().rev().find(|m| m.update.as_deref() == Some("plan")).map(|m| m.text.clone())
}

fn any_running(jobs: &[JobSummary]) -> bool {
    use aurelius_domain::job::JobState;
    jobs.iter().any(|j| matches!(j.state, JobState::Ready | JobState::Leased))
}

/// A read the client has no helper for (yet), with the expected body picked out.
async fn read<T>(client: &Client, method: Method, pick: impl FnOnce(ResponseBody) -> Option<T>) -> Result<T, ClientError> {
    pick(client.call(method).await?).ok_or(ClientError::UnexpectedResponse)
}

/// The company in one read: employees, roots and the org chart setting.
async fn company(client: &Client) -> Result<aurelius_protocol::CompanySnapshot, ClientError> {
    read(client, Method::CompanyGet, |b| match b { ResponseBody::Company(c) => Some(c), _ => None }).await
}

/// The employees go where the window kept its managers; the rest of the company beside them.
fn send_company(updates: &mpsc::UnboundedSender<Update>, company: aurelius_protocol::CompanySnapshot) {
    let _ = updates.send(Update::Managers(company.employees.clone()));
    let _ = updates.send(Update::Company(company));
}

async fn refresh(client: &Client, updates: &mpsc::UnboundedSender<Update>) -> Result<(u64, bool), Disconnected> {
    // All at once: over the tailnet each answer is a round trip. The projects
    // snapshot fixes the cursor; the others are only as new or newer.
    let (projects, company, approvals, connections, tasks, notifications, usage, update, backups, subscription, jobs, proposals) = tokio::join!(
        client.projects(),
        company(client),
        client.approvals(),
        client.connections(),
        read(client, Method::TaskList { project_id: None, employee_id: None, state: None }, |b| match b { ResponseBody::Tasks(t) => Some(t), _ => None }),
        client.notifications(),
        client.usage(),
        client.update_status(false),
        client.backups(),
        // Subscription limits: answered from the daemon's cache (refreshed there in the background).
        client.subscription(false),
        client.jobs(),
        read(client, Method::ProposalList, |b| match b { ResponseBody::Proposals(p) => Some(p), _ => None }),
    );
    let projects = projects?;
    send_company(updates, company?);
    let _ = updates.send(Update::Approvals(approvals?.approvals));
    if let Ok(c) = connections {
        let _ = updates.send(Update::Connections(c));
    }
    let _ = updates.send(Update::Tasks(tasks?.tasks));
    if let Ok(proposals) = proposals {
        let _ = updates.send(Update::Proposals(proposals.proposals));
    }
    let _ = updates.send(Update::Notifications(notifications?));
    let _ = updates.send(Update::Usage(usage?));
    let _ = updates.send(Update::UpdateStatus(update?));
    if let Ok(backups) = backups {
        let _ = updates.send(Update::Backups(backups));
    }
    let _ = updates.send(Update::Subscription(subscription?));
    let jobs = jobs?.jobs;
    let running = any_running(&jobs);
    let _ = updates.send(Update::Projects(projects.projects));
    let _ = updates.send(Update::Jobs(jobs));
    Ok((projects.as_of_seq, running))
}

/// The load now, or why there is none.
async fn refresh_load(client: &Client, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    match client.load_snapshot().await {
        Ok(snapshot) => { let _ = updates.send(Update::Load(Ok(snapshot))); }
        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
        Err(e) => { let _ = updates.send(Update::Load(Err(load_error(e)))); }
    }
    Ok(())
}

/// What the user reads when the browser setting can't be shown or changed.
fn browser_error(err: ClientError) -> String {
    match err {
        ClientError::Protocol(p) if p.code == aurelius_protocol::ErrorCode::Forbidden => "You need the Approve right on this server to change the browser.".into(),
        other => other.to_string(),
    }
}

fn mcp_manage_error(err: ClientError) -> String {
    match err {
        ClientError::Protocol(p) if p.code == aurelius_protocol::ErrorCode::Forbidden => crate::screens::mcp_add::NO_OWNER.into(),
        other => other.to_string(),
    }
}

fn always_allow_error(err: ClientError) -> String {
    match err {
        ClientError::Protocol(p) if p.code == aurelius_protocol::ErrorCode::Forbidden => "You need the Approve right on this server to remove always allowed tools.".into(),
        other => other.to_string(),
    }
}

/// The always-allowed MCP tools; a server without the list answers an error and the section stays hidden.
async fn load_always_allow(client: &Client, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    match client.mcp_always_allow().await {
        Ok(list) => { let _ = updates.send(Update::McpAlwaysAllow(Some(list.tools))); }
        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
        Err(_) => { let _ = updates.send(Update::McpAlwaysAllow(None)); }
    }
    Ok(())
}

fn mcp_error(err: ClientError, saving: bool) -> String {
    match err {
        ClientError::Protocol(p) if saving && p.code == aurelius_protocol::ErrorCode::Forbidden => "You need the Approve right on this server to change MCP servers.".into(),
        other => other.to_string(),
    }
}

fn load_error(err: ClientError) -> String {
    match err {
        ClientError::Protocol(p) if p.code == aurelius_protocol::ErrorCode::Forbidden => crate::screens::load::NO_RIGHT.into(),
        other => other.to_string(),
    }
}

async fn refresh_ideas(client: &Client, updates: &mpsc::UnboundedSender<Update>, open: Option<Option<ProjectId>>) -> Result<(), Disconnected> {
    let Some(scope) = open else { return Ok(()) };
    let ideas = match scope {
        Some(project_id) => shown(client.ideas(project_id).await, updates)?.map(|s| s.ideas),
        None => shown(client.all_ideas().await, updates)?.map(|s| s.ideas),
    };
    let Some(ideas) = ideas else { return Ok(()) };
    let _ = updates.send(Update::Ideas(scope, ideas));
    Ok(())
}

/// One plugins call. A failure is shown
/// in the place that asked, only a lost connection ends the session.
async fn handle_plugins(client: &Client, call: PluginsCall, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    fn readable(e: &ClientError) -> String {
        match e {
            ClientError::Protocol(p) if p.code == aurelius_protocol::ErrorCode::Forbidden => "You need the Approve right on this server to change plugins.".into(),
            other => other.to_string(),
        }
    }
    fn check<T>(r: Result<T, ClientError>) -> Result<Result<T, String>, Disconnected> {
        match r {
            Ok(v) => Ok(Ok(v)),
            Err(e) if e.is_disconnect() || e.is_final() => Err(e.into()),
            Err(e) => Ok(Err(readable(&e))),
        }
    }
    use PluginsUpdate as U;
    let send = |u: U| {
        let _ = updates.send(Update::Plugins(u));
    };
    match call {
        PluginsCall::List => send(U::Installed(check(client.plugins_list().await)?)),
        PluginsCall::Available => send(U::Catalog(check(client.plugins_available().await)?)),
        PluginsCall::Preview(source) => send(U::Preview(check(client.plugins_preview(source).await)?)),
        PluginsCall::Install { ids, managers, projects } => send(U::InstallDone(check(client.plugins_install(ids, managers, projects).await)?)),
        PluginsCall::Update(id) => match check(client.plugin_update(id).await)? {
            Ok(s) => send(U::Installed(Ok(s))),
            Err(why) => send(U::Failed(why)),
        },
        PluginsCall::Remove(id) => match check(client.plugin_remove(id).await)? {
            Ok(s) => send(U::Installed(Ok(s))),
            Err(why) => send(U::Failed(why)),
        },
        PluginsCall::ManagerGet(id) => match check(client.employee_plugins(id).await)? {
            Ok(m) => send(U::Manager(m)),
            Err(why) => send(U::Failed(why)),
        },
        PluginsCall::ManagerSet(id, enabled) => match check(client.set_employee_plugins(id, enabled).await)? {
            Ok(m) => send(U::Manager(m)),
            Err(why) => send(U::Failed(why)),
        },
        PluginsCall::ProjectGet(id) => match check(client.project_plugins(id).await)? {
            Ok(p) => send(U::Project(p)),
            Err(why) => send(U::Failed(why)),
        },
        PluginsCall::ProjectSet(id, enabled) => match check(client.set_project_plugins(id, enabled).await)? {
            Ok(p) => send(U::Project(p)),
            Err(why) => send(U::Failed(why)),
        },
    }
    Ok(())
}

async fn handle_app_keys(client: &Client, call: AppKeyCall, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    let send = |u: AppKeyUpdate| {
        let _ = updates.send(Update::AppKeys(u));
    };
    let done = match call {
        AppKeyCall::List => Ok(()),
        AppKeyCall::Create(cmd) => client.create_app_key(cmd).await.map(|created| send(AppKeyUpdate::Created(created))),
        AppKeyCall::Revoke(id) => client.revoke_app_key(id).await.map(|_| ()),
    };
    match done.and(client.app_keys().await) {
        Ok(keys) => send(AppKeyUpdate::Keys(keys)),
        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
        Err(e) => send(AppKeyUpdate::Failed(e.to_string())),
    }
    Ok(())
}

/// Every recorded artifact; nothing on servers without `artifact.list`, so the lists stay hidden.
// shortcut: the server lists at most 500 (all scopes at once, filtered per conversation/task here), fine until someone publishes hundreds.
async fn refresh_artifacts(client: &Client, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    match client.artifacts(None, None, None).await {
        Ok(artifacts) => { let _ = updates.send(Update::Artifacts(artifacts)); }
        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
        Err(_) => {}
    }
    Ok(())
}

/// The recurring tasks.
async fn refresh_schedules(client: &Client, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    match client.task_schedules(None).await {
        Ok(snapshot) => { let _ = updates.send(Update::Schedules(snapshot.schedules)); }
        Err(e) if e.is_disconnect() || e.is_final() => return Err(e.into()),
        Err(_) => {}
    }
    Ok(())
}

async fn refresh_memory(client: &Client, updates: &mpsc::UnboundedSender<Update>, open: Option<ProjectId>) -> Result<(), Disconnected> {
    let Some(project_id) = open else { return Ok(()) };
    let method = Method::MemoryList { project_id: Some(project_id), employee_id: None };
    let Some(snapshot) = shown(read(client, method, |b| match b { ResponseBody::Memory(m) => Some(m), _ => None }).await, updates)? else { return Ok(()) };
    let _ = updates.send(Update::Memory(project_id, snapshot.items));
    Ok(())
}

async fn refresh_messages(client: &Client, updates: &mpsc::UnboundedSender<Update>, open: Option<ConversationId>) -> Result<(), Disconnected> {
    if let Some(id) = open {
        if let Some(messages) = shown(client.messages(id).await, updates)? {
            let _ = updates.send(Update::Messages(messages));
        }
    }
    Ok(())
}

async fn send(client: &Client, envelope: CommandEnvelope, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    let deleting = match &envelope.command {
        Command::DeleteQueuedMessage(c) => Some(c.message_id),
        _ => None,
    };
    let describing = match &envelope.command {
        Command::SetProjectDescription(c) => Some(c.project_id),
        _ => None,
    };
    let deciding = matches!(&envelope.command, Command::DecideProposal(_));
    match client.execute(envelope).await {
        Ok(receipt) => {
            if let Some(project_id) = describing {
                let _ = updates.send(Update::DescriptionSaved(project_id, Ok(())));
            }
            match receipt.result {
                CommandResult::ProjectCreated { project_id, .. } => {
                    let _ = updates.send(Update::Created(project_id));
                }
                CommandResult::EmployeeCreated { employee_id, .. } => {
                    let _ = updates.send(Update::EmployeeCreated(employee_id));
                }
                CommandResult::BrowserAllowedSet { allowed, .. } => {
                    let _ = updates.send(Update::Info(if allowed {
                        "Browser allowed. Every browser action still asks you first.".into()
                    } else {
                        "Browser off.".into()
                    }));
                }
                CommandResult::ScreenAllowedSet { allowed, .. } => {
                    let _ = updates.send(Update::Info(if allowed {
                        "Screen allowed: this agent takes screenshots without asking.".into()
                    } else {
                        "Screen off: the agent asks you before each screenshot.".into()
                    }));
                }
                CommandResult::AgentModeSet { mode, .. } => {
                    let _ = updates.send(Update::Info(format!("Mode: {} — {}", mode.label(), mode.description())));
                }
                CommandResult::TaskCancelled { .. } => {
                    let _ = updates.send(Update::Info("Task cancelled.".into()));
                }
                CommandResult::TaskAssigned { task_id } => {
                    let _ = updates.send(Update::TaskAssigned(task_id));
                }
                CommandResult::ConversationDeleted { .. } => {
                    let _ = updates.send(Update::Info("Conversation deleted; its messages and attachments are erased.".into()));
                }
                CommandResult::EmployeeFolderSet { folder, .. } => {
                    let _ = updates.send(Update::Info(match folder {
                        Some(f) => format!("Employee workspace: {f}"),
                        None => "This employee now works in the project folder.".into(),
                    }));
                }
                CommandResult::ProjectFolderSet { folder, .. } => {
                    let _ = updates.send(Update::Info(format!("Project folder: {folder}")));
                }
                CommandResult::MemoryStored { .. } => {
                    let _ = updates.send(Update::Info("Saved to project memory, with you as the source.".into()));
                }
                CommandResult::MemoryConsolidationQueued { queued: false } => {
                    let _ = updates.send(Update::Info("A consolidation of this memory is already running.".into()));
                }
                CommandResult::MemoryForgotten { .. } => {
                    let _ = updates.send(Update::Info("Forgotten: no longer in context or the search index; content erased.".into()));
                }
                CommandResult::TaskCreated { task_id } => {
                    let _ = updates.send(Update::Info("Task created.".into()));
                    let _ = updates.send(Update::TaskCreated(task_id));
                }
                CommandResult::EmployeeReportsToSet { reports_to, .. } => {
                    let _ = updates.send(Update::Info(if reports_to.is_some() { "Reporting line updated." } else { "This employee is now a root of the org chart." }.into()));
                }
                CommandResult::FollowOrgChartSet { follow } => {
                    let _ = updates.send(Update::Info(if follow {
                        "Following the org chart: delegation, hand-overs and escalation follow who reports to whom."
                    } else {
                        "Not following the org chart: employees work with anyone in the company."
                    }.into()));
                }
                CommandResult::ProposalDecided { status, employee_id, .. } => {
                    use aurelius_domain::proposal::ProposalStatus;
                    // Only an approved colleague creates a manager; approved instructions don't.
                    let _ = updates.send(Update::Info(match (status, employee_id) {
                        (ProposalStatus::Approved, Some(_)) => "Colleague approved: the employee is added to the project.",
                        (ProposalStatus::Approved, None) => "Instructions approved: the employee's next answers use them.",
                        (ProposalStatus::Rejected, _) => "Proposal declined. The proposer is told.",
                        _ => "Proposal updated.",
                    }.into()));
                }
                CommandResult::EmployeeInstructionsRestored { version, .. } => {
                    let _ = updates.send(Update::Info(format!("Instructions restored as version {version}: the employee's next answers use them.")));
                }
                CommandResult::ApprovalDecided { output, .. } => {
                    let _ = updates.send(Update::Info(output));
                }
                _ => {}
            }
            Ok(())
        }
        Err(err) if retryable(&err) || err.is_final() => Err(err.into()),
        Err(err) if needs_board_member(&err) => {
            let _ = updates.send(Update::BoardMemberRequired);
            Ok(())
        }
        Err(err) => {
            // Rejected (validation, conflict): tell the user; do not retry.
            if let Some(message_id) = deleting {
                let _ = updates.send(Update::QueuedDeleteFailed(message_id));
            }
            if let Some(project_id) = describing {
                let _ = updates.send(Update::DescriptionSaved(project_id, Err(err.to_string())));
                return Ok(());
            }
            if deciding && matches!(&err, ClientError::Protocol(p) if p.code == aurelius_protocol::ErrorCode::VersionConflict) {
                let _ = updates.send(Update::Error("The instructions changed since this was proposed, so it can't be approved as it is. Decline it, or edit the instructions in Edit employee.".into()));
                return Ok(());
            }
            let _ = updates.send(Update::Error(err.to_string()));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_team_chat_that_does_not_load_says_why() {
        use aurelius_domain::error::{AppError, ErrorCode};
        // What a server from before the group chat answers to `group_chat.get` (`reject_unparsed`).
        let old = AppError::new(ErrorCode::UnknownMethod, "This server does not know the method group_chat.get.");
        assert!(group_chat_failure(&ClientError::Protocol(old)).contains("older than this app"));
        assert!(group_chat_failure(&ClientError::TimedOut(60)).contains("did not answer in time"));
        let missing = ClientError::Protocol(AppError::new(ErrorCode::NotFound, "the group chat does not exist"));
        assert_eq!(group_chat_failure(&missing), "The Team chat could not be loaded: the group chat does not exist");
    }

    /// Refreshes the throttle lets through while `marks` (ms after start) arrive, until `end_ms`.
    async fn refreshes(marks: &[u64], end_ms: u64) -> Vec<u64> {
        let start = tokio::time::Instant::now();
        let mut stale = Coalesce::new(Duration::from_millis(500));
        let mut marks = marks.iter().peekable();
        let mut out = Vec::new();
        loop {
            let next = marks.peek().map(|&&ms| start + Duration::from_millis(ms));
            tokio::select! {
                _ = tokio::time::sleep_until(next.unwrap_or(start + Duration::from_millis(end_ms))) => {
                    if next.is_none() { return out }
                    marks.next();
                    stale.mark();
                }
                _ = stale.due(), if stale.marked => out.push(start.elapsed().as_millis() as u64),
            }
        }
    }

    #[test]
    fn a_password_is_never_printed() {
        let call = Request::BoardMembers(UserCall::Use { user_id: aurelius_domain::UserId::new(), password: Password::new("Tr0ub4dor&3-zebra".into()) });
        let create = Request::BoardMembers(UserCall::Create { username: "ada".into(), password: Password::new("Tr0ub4dor&3-zebra".into()) });
        for printed in [format!("{call:?}"), format!("{create:?}")] {
            assert!(!printed.contains("Tr0ub4dor"), "{printed}");
            assert!(printed.contains("<redacted>"), "{printed}");
        }
    }

    #[test]
    fn a_refusal_goes_to_its_field() {
        use aurelius_domain::error::{AppError, ErrorCode};
        let refusal = |code, text: &str| member_failure(&ClientError::Protocol(AppError::new(code, text)));
        assert_eq!(refusal(ErrorCode::UsernameTaken, "another board member already has this name"), MemberFailure { field: Some(MemberField::Username), message: "This username is taken.".into() });
        assert_eq!(refusal(ErrorCode::WeakPassword, "A password has at least 12 characters.").field, Some(MemberField::Password));
        assert_eq!(refusal(ErrorCode::InvalidUsername, "Use only letters, digits, '.', '_' and '-'.").field, Some(MemberField::Username));
        assert_eq!(refusal(ErrorCode::TooManyAttempts, "Too many tries.").message, "Too many tries. Wait a minute and try again.");
        assert_eq!(refusal(ErrorCode::Internal, "storage is busy").field, None);
        assert!(needs_board_member(&ClientError::Protocol(AppError::new(ErrorCode::BoardMemberRequired, "choose who you are first"))));
        assert!(!needs_board_member(&ClientError::Disconnected));
    }

    mod against_a_daemon {
        use super::super::*;
        use aurelius_domain::UserId;
        use aurelius_domain::user::WhoAmI;

        const ADA: &str = "Tr0ub4dor&3-zebra-ada";
        const MARIE: &str = "correct-horse-battery-marie";

        async fn daemon(root: &std::path::Path) -> tokio::task::JoinHandle<anyhow::Result<()>> {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let handle = tokio::spawn(aurelius_daemon::run(root.to_path_buf(), Some(tx)));
            tokio::time::timeout(Duration::from_secs(20), rx).await.expect("daemon ready").expect("ready signal");
            handle
        }

        async fn stop(root: &std::path::Path, handle: tokio::task::JoinHandle<anyhow::Result<()>>) {
            aurelius_client::bridge::shutdown_daemon(root).await.expect("shutdown");
            tokio::time::timeout(Duration::from_secs(20), handle).await.expect("daemon stops").expect("join").expect("clean stop");
        }

        /// The first update `pick` takes; the others pass by.
        async fn next<T>(handle: &mut Handle, mut pick: impl FnMut(Update) -> Option<T>) -> T {
            tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let update = handle.updates.recv().await.expect("the backend runs");
                    if let Some(found) = pick(update) {
                        return found;
                    }
                }
            })
            .await
            .expect("the update came")
        }

        async fn who(handle: &mut Handle) -> WhoAmI {
            next(handle, |u| match u {
                Update::Who(who) => Some(who),
                _ => None,
            })
            .await
        }

        async fn failure(handle: &mut Handle) -> MemberFailure {
            next(handle, |u| match u {
                Update::BoardMemberFailed(f) => Some(f),
                _ => None,
            })
            .await
        }

        fn ask(handle: &Handle, call: UserCall) {
            handle.requests.send(Request::BoardMembers(call)).unwrap();
        }

        fn create(username: &str, password: &str) -> UserCall {
            UserCall::Create { username: username.into(), password: Password::new(password.into()) }
        }

        fn use_(user_id: UserId, password: &str) -> UserCall {
            UserCall::Use { user_id, password: Password::new(password.into()) }
        }

        /// MU1-20, MU1-21, MU1-22 and B8 through this app's backend: the onboarding,
        /// a second board member, the choice with its one password check, the
        /// remembered one in every hello (also after a reconnect, Trace m9), an
        /// unknown one forgotten, switching, and `board_member_required` on a write.
        #[tokio::test(flavor = "multi_thread")]
        async fn board_members_from_onboarding_to_switching() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let mut server = daemon(&root).await;
            let target = || Some(Target::Local { root: root.clone(), spawn: false });

            // A server without board members: only the onboarding (MU1-20 AC1).
            let mut first = start(target(), None);
            let empty = who(&mut first).await;
            assert!(!empty.board_members_exist && empty.user.is_none(), "{empty:?}");

            // Refused, nothing made (AC3): a short password, by its field.
            ask(&first, create("ada", "short"));
            assert_eq!(failure(&mut first).await.field, Some(MemberField::Password));

            // Made, and this window works as it (AC2, MU1-21 AC1).
            ask(&first, create("ada", ADA));
            let onboarded = who(&mut first).await;
            let ada = onboarded.user.clone().expect("the new board member works here");
            assert!(onboarded.board_members_exist);
            assert_eq!(ada.username, "ada");

            // A second board member (MU1-22 AC1); this window stays ada.
            ask(&first, create("ada", MARIE));
            assert_eq!(failure(&mut first).await, MemberFailure { field: Some(MemberField::Username), message: "This username is taken.".into() }, "MU1-22 AC2");
            ask(&first, create("marie", MARIE));
            let made = next(&mut first, |u| match u {
                Update::BoardMemberCreated(name) => Some(name),
                _ => None,
            })
            .await;
            assert_eq!(made, "marie");
            assert_eq!(who(&mut first).await.user.map(|u| u.id), Some(ada.id), "still ada");
            let list = next(&mut first, |u| match u {
                Update::BoardMembers(list) => Some(list),
                _ => None,
            })
            .await;
            assert_eq!(list.iter().map(|u| u.username.as_str()).collect::<Vec<_>>(), ["ada", "marie"]);
            let marie = list[1].id;

            // A new device: board members exist, none works here (MU1-20 AC5, MU1-21 AC2).
            let mut device = start(target(), None);
            let fresh = who(&mut device).await;
            assert!(fresh.board_members_exist && fresh.user.is_none(), "{fresh:?}");

            // Writes wait for the choice: the S5 gate as the window hears it.
            let (events, _events) = mpsc::unbounded_channel();
            let bare = Client::open(target().as_ref().unwrap(), "test", events, None, &[]).await.unwrap();
            let (gate_tx, mut gate_rx) = mpsc::unbounded_channel();
            let ticket = aurelius_domain::ticket::CreateTicket {
                project_id: ProjectId::new(),
                title: "Login per board member".into(),
                description: None,
                priority: Default::default(),
                labels: vec![],
                deadline_ms: None,
                owner_employee_id: None,
                run: Default::default(),
                files: vec![],
                start: false,
                from_message: None,
            };
            assert!(send(&bare, envelope(Command::CreateTicket(ticket), None), &gate_tx).await.is_ok(), "the session goes on");
            assert!(matches!(gate_rx.try_recv(), Ok(Update::BoardMemberRequired)));

            // The one password check of this device (B8).
            ask(&device, use_(marie, "not her password"));
            assert_eq!(failure(&mut device).await, MemberFailure { field: Some(MemberField::Password), message: "That password is wrong.".into() });
            ask(&device, use_(marie, MARIE));
            assert_eq!(who(&mut device).await.user.map(|u| u.id), Some(marie));

            // The server restarts: the device says marie again in its hello, no password (Trace m9).
            stop(&root, server).await;
            server = daemon(&root).await;
            let back = who(&mut device).await;
            assert_eq!(back.user.map(|u| u.id), Some(marie), "not forgotten on a reconnect");

            // A window that remembers ada starts as that board member; an unknown one is not taken.
            let mut remembered = start(target(), Some(ada.id));
            assert_eq!(who(&mut remembered).await.user.map(|u| u.id), Some(ada.id));
            let mut unknown = start(target(), Some(UserId::new()));
            let gone = who(&mut unknown).await;
            assert!(gone.board_members_exist && gone.user.is_none(), "the choice shows again: {gone:?}");

            // Switch board member.
            ask(&device, use_(ada.id, ADA));
            assert_eq!(who(&mut device).await.user.map(|u| u.id), Some(ada.id));

            drop((first, device, remembered, unknown));
            stop(&root, server).await;
        }
    }

    mod the_board_against_a_daemon {
        use super::super::*;
        use aurelius_domain::ticket::{self, BoardView, ColumnKind};

        async fn daemon(root: &std::path::Path) -> tokio::task::JoinHandle<anyhow::Result<()>> {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let handle = tokio::spawn(aurelius_daemon::run(root.to_path_buf(), Some(tx)));
            tokio::time::timeout(Duration::from_secs(20), rx).await.expect("daemon ready").expect("ready signal");
            handle
        }

        async fn next<T>(handle: &mut Handle, mut pick: impl FnMut(Update) -> Option<T>) -> T {
            tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    if let Some(found) = pick(handle.updates.recv().await.expect("the backend runs")) {
                        return found;
                    }
                }
            })
            .await
            .expect("the update came")
        }

        async fn load(handle: &mut Handle, project: Option<ProjectId>) -> BoardView {
            handle.requests.send(Request::Board(BoardCall::Load(project))).unwrap();
            next(handle, |u| match u {
                Update::Board(BoardUpdate::Loaded(p, result)) if p == project => Some(result.expect("the board loads")),
                _ => None,
            })
            .await
        }

        /// A write's answer, and whether its event already said the board changed.
        async fn write(handle: &mut Handle, command: Command, expected_version: Option<u64>) -> (BoardUpdate, bool) {
            handle.requests.send(Request::Board(BoardCall::Write { command, expected_version, done: Some("ok".into()) })).unwrap();
            let mut changed = false;
            let answer = next(handle, |u| match u {
                Update::Board(BoardUpdate::Changed { .. }) => {
                    changed = true;
                    None
                }
                Update::Board(b @ (BoardUpdate::Done { .. } | BoardUpdate::Created { .. } | BoardUpdate::Failed { .. })) => Some(b),
                _ => None,
            })
            .await;
            (answer, changed)
        }

        async fn done(handle: &mut Handle, command: Command, expected_version: Option<u64>) {
            match write(handle, command, expected_version).await.0 {
                BoardUpdate::Done { .. } | BoardUpdate::Created { .. } => {}
                other => panic!("refused: {other:?}"),
            }
        }

        async fn refused(handle: &mut Handle, command: Command, expected_version: Option<u64>) -> String {
            match write(handle, command, expected_version).await.0 {
                BoardUpdate::Failed { message, .. } => message,
                other => panic!("not refused: {other:?}"),
            }
        }

        fn tickets(view: &BoardView, kind: ColumnKind) -> Vec<ticket::TicketView> {
            view.columns.iter().find(|c| c.column.kind == kind).map(|c| c.tickets.clone()).unwrap_or_default()
        }

        fn new_ticket(project_id: ProjectId, title: &str) -> Command {
            Command::CreateTicket(ticket::CreateTicket {
                project_id,
                title: title.into(),
                description: Some("What and why.".into()),
                priority: ticket::TicketPriority::High,
                labels: vec!["ui".into()],
                deadline_ms: None,
                owner_employee_id: None,
                run: Default::default(),
                files: vec![],
                start: false,
                from_message: None,
            })
        }

        /// MU1-05..MU1-12 and B3/B4/B5 through this app's backend: the empty board,
        /// new tickets in Backlog, reordering with `before_ticket_id`, a refused move
        /// with its reason, the owner and back to the dispatcher, the columns with the
        /// board's version (and a stale one refused), the project's dispatcher, Start
        /// with nobody to choose, Back to Backlog, Delete, live changes, and a write
        /// while offline that says so instead of waiting.
        #[tokio::test(flavor = "multi_thread")]
        async fn the_board_from_a_new_ticket_to_its_columns() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let server = daemon(&root).await;
            let mut app = start(Some(Target::Local { root: root.clone(), spawn: false }), None);
            app.requests.send(Request::BoardMembers(UserCall::Create { username: "ada".into(), password: Password::new("Tr0ub4dor&3-zebra-ada".into()) })).unwrap();
            next(&mut app, |u| match u {
                Update::Who(w) if w.user.is_some() => Some(()),
                _ => None,
            })
            .await;
            app.requests.send(Request::CreateProject { name: "Atlas".into(), description: None, folder: None }).unwrap();
            let project = next(&mut app, |u| match u {
                Update::Created(p) => Some(p),
                _ => None,
            })
            .await;

            // The empty board: the six columns in order (MU1-05 AC1, MU1-19 AC4).
            let empty = load(&mut app, None).await;
            assert_eq!(empty.columns.iter().map(|c| c.column.kind).collect::<Vec<_>>(), ColumnKind::ALL);
            assert!(empty.columns.iter().all(|c| c.tickets.is_empty()));

            // New tickets land in Backlog, made by the active board member (MU1-07 AC1).
            let (created, _) = write(&mut app, new_ticket(project, "Columns"), None).await;
            assert!(matches!(created, BoardUpdate::Created { number: 1, .. }), "{created:?}");
            done(&mut app, new_ticket(project, "Filters"), None).await;
            let view = load(&mut app, Some(project)).await;
            let backlog = tickets(&view, ColumnKind::Backlog);
            assert_eq!(backlog.iter().map(|t| t.number).collect::<Vec<_>>(), [1, 2]);
            assert_eq!(backlog[0].created_by_username, "ada");
            assert_eq!(backlog[0].priority, ticket::TicketPriority::High, "B9");
            let (one, two) = (backlog[0].id, backlog[1].id);
            let column = |view: &BoardView, kind: ColumnKind| view.columns.iter().find(|c| c.column.kind == kind).unwrap().column.clone();

            // #2 right above #1 (ticket.move with before_ticket_id); the event says the board changed.
            let (moved, changed) = write(&mut app, Command::MoveTicket(ticket::MoveTicket { ticket_id: two, column_id: column(&view, ColumnKind::Backlog).id, before_ticket_id: Some(one) }), None).await;
            assert!(matches!(moved, BoardUpdate::Done { ticket: Some(t), .. } if t == two), "{moved:?}");
            if !changed {
                next(&mut app, |u| matches!(u, Update::Board(BoardUpdate::Changed { tickets: true, .. })).then_some(())).await;
            }
            assert_eq!(tickets(&load(&mut app, Some(project)).await, ColumnKind::Backlog).iter().map(|t| t.number).collect::<Vec<_>>(), [2, 1], "MU1-05 AC3");

            // A move the rules refuse comes back with its reason (§6: Backlog → Doing).
            assert!(!refused(&mut app, Command::MoveTicket(ticket::MoveTicket { ticket_id: one, column_id: column(&view, ColumnKind::Doing).id, before_ticket_id: None }), None).await.is_empty());

            // The owner, then back to the dispatcher in Backlog (ticket.assign).
            app.requests.send(Request::CreateEmployee { name: "Builder".into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None }).unwrap();
            let builder = next(&mut app, |u| match u {
                Update::EmployeeCreated(e) => Some(e),
                _ => None,
            })
            .await;
            done(&mut app, Command::AssignTicket(ticket::AssignTicket { ticket_id: one, employee_id: Some(builder), reason: None }), None).await;
            let owned = tickets(&load(&mut app, Some(project)).await, ColumnKind::Backlog);
            assert_eq!(owned.iter().find(|t| t.id == one).unwrap().owner_employee_id, Some(builder));
            done(&mut app, Command::AssignTicket(ticket::AssignTicket { ticket_id: one, employee_id: None, reason: None }), None).await;
            assert_eq!(tickets(&load(&mut app, Some(project)).await, ColumnKind::Backlog).iter().find(|t| t.id == one).unwrap().owner_employee_id, None, "the dispatcher chooses again");

            // A ticket write carries the version the window knows (D-m1): a stale one is refused with §7.2's words.
            let card = tickets(&load(&mut app, Some(project)).await, ColumnKind::Backlog).into_iter().find(|t| t.id == one).unwrap();
            let rename = |title: &str| Command::UpdateTicket(ticket::UpdateTicket { title: Some(title.into()), ..crate::screens::board::empty_update(one) });
            let version = crate::screens::board::expected_version_for(&rename("x"), Some(card.version));
            assert_eq!(version, Some(card.version));
            done(&mut app, rename("Renamed here"), version).await;
            assert_eq!(refused(&mut app, rename("Renamed elsewhere"), version).await, BOARD_CONFLICT);
            assert_eq!(tickets(&load(&mut app, Some(project)).await, ColumnKind::Backlog).iter().find(|t| t.id == one).unwrap().title, "Renamed here", "the first write stays");

            // The columns: renamed and reordered with the board's version; a stale version is refused (B3, MU1-06).
            let all = load(&mut app, None).await;
            let mut columns: Vec<ticket::BoardColumnView> = all.columns.iter().map(|c| c.column.clone()).collect();
            let version = ticket::board_version(&columns);
            columns.iter_mut().find(|c| c.kind == ColumnKind::Doing).unwrap().name = "Building".into();
            let edit = crate::screens::board::columns_edit(&columns);
            done(&mut app, Command::UpdateBoardColumns(edit.clone()), Some(version)).await;
            let renamed = load(&mut app, None).await;
            assert_eq!(renamed.columns.iter().find(|c| c.column.kind == ColumnKind::Doing).unwrap().column.name, "Building");
            assert_eq!(refused(&mut app, Command::UpdateBoardColumns(edit), Some(version)).await, BOARD_CONFLICT);

            // A project's own dispatcher, then the company's again (MU1-11 AC1/AC2).
            let set = |employee_id| Command::UpdateProject(aurelius_domain::company::UpdateProject { project_id: project, dispatcher: Some(ticket::SetDispatcher { employee_id }) });
            done(&mut app, set(Some(builder)), None).await;
            done(&mut app, set(None), None).await;

            // Start with nobody to choose: Todo, no owner, no triage job: "choose an owner" (B5).
            done(&mut app, Command::StartTicket(ticket::StartTicket { ticket_id: two, employee_id: None }), None).await;
            let todo = tickets(&load(&mut app, Some(project)).await, ColumnKind::Todo);
            let started = todo.iter().find(|t| t.id == two).expect("#2 is in Todo (MU1-09 AC1)");
            assert_eq!(started.owner_employee_id, None);
            assert!(!crate::screens::board::choosing(started, false, false), "no dispatcher");
            assert_eq!(crate::screens::board::needs_you(started, false, 0), Some(crate::screens::board::Needs::ChooseOwner));

            // Its Activity (timeline.list on the ticket): nobody chose, said by the server (MU1-15 AC4, B5).
            let activity = tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    app.requests.send(Request::Board(BoardCall::Activity(two))).unwrap();
                    let cards = next(&mut app, |u| match u {
                        Update::Board(BoardUpdate::Activity(t, r)) if t == two => Some(r.expect("the activity loads")),
                        _ => None,
                    })
                    .await;
                    if cards.iter().any(|c| c.title == "ticket.updated") {
                        return cards;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
            .await
            .expect("the dispatcher's answer shows");
            let lines: Vec<String> = activity.iter().map(crate::screens::board::activity_text).collect();
            assert_eq!(lines.first().map(String::as_str), Some("Nobody chose an owner; a board member must"), "newest first: {lines:?}");
            assert!(activity.windows(2).all(|w| w[0].at_ms >= w[1].at_ms), "newest first");
            // The same read the board does per card: the dispatcher gave up, as mobile shows it.
            app.requests.send(Request::Board(BoardCall::GaveUp { ticket: two, version: started.version })).unwrap();
            let gave_up = next(&mut app, |u| match u {
                Update::Board(BoardUpdate::GaveUp { ticket, gave_up, .. }) if ticket == two => Some(gave_up),
                _ => None,
            })
            .await;
            assert_eq!(gave_up, Some(true), "B5");

            // Back to Backlog stops it (ticket.cancel); Delete takes it off the board.
            done(&mut app, Command::CancelTicket(ticket::CancelTicket { ticket_id: two }), None).await;
            assert!(tickets(&load(&mut app, Some(project)).await, ColumnKind::Backlog).iter().any(|t| t.id == two));
            done(&mut app, Command::ArchiveTicket(ticket::ArchiveTicket { ticket_id: one }), None).await;
            assert!(!tickets(&load(&mut app, Some(project)).await, ColumnKind::Backlog).iter().any(|t| t.id == one));

            // A ticket with a file: copied into the project's work folder, shown on the ticket (MU1-07).
            let folder = root.join("work");
            std::fs::create_dir_all(&folder).unwrap();
            let spec = root.join("spec.md");
            std::fs::write(&spec, b"# What and why").unwrap();
            app.requests.send(Request::CreateProject { name: "Hermes".into(), description: None, folder: Some(folder.display().to_string()) }).unwrap();
            let with_folder = next(&mut app, |u| match u {
                Update::Created(p) => Some(p),
                _ => None,
            })
            .await;
            let Command::CreateTicket(mut create) = new_ticket(with_folder, "With a file") else { unreachable!() };
            create.files = vec![spec.display().to_string()];
            done(&mut app, Command::CreateTicket(create), None).await;
            let attached = tickets(&load(&mut app, Some(with_folder)).await, ColumnKind::Backlog);
            assert_eq!(attached[0].attachments.iter().map(|f| (f.name.as_str(), f.byte_length)).collect::<Vec<_>>(), [("spec.md", 14)]);

            // Offline a write says so at once and is not kept for later.
            aurelius_client::bridge::shutdown_daemon(&root).await.expect("shutdown");
            tokio::time::timeout(Duration::from_secs(20), server).await.expect("daemon stops").expect("join").expect("clean stop");
            next(&mut app, |u| matches!(u, Update::Connection(Connection::Offline(_))).then_some(())).await;
            let message = refused(&mut app, Command::StartTicket(ticket::StartTicket { ticket_id: two, employee_id: None }), None).await;
            assert!(message.starts_with("Not connected"), "{message}");
        }

        /// AUR-MU-02 end to end against the discussion engine, in two windows (ada and
        /// marie, no employees, so nobody is woken and nothing runs): a post with an
        /// `@mention` in a ticket's discussion, by its author; `channel.list` with
        /// #general and one channel per project; a mention in a project channel that
        /// marie sees as unread with a mention, reading it marks it read, and a next
        /// message comes in live (`team_channels`, no reload); a ticket from that
        /// message; Try again on the summary of a ticket that is not Done is refused
        /// as the discussion's own answer.
        #[tokio::test(flavor = "multi_thread")]
        async fn the_discussion_and_channels_end_to_end() {
            const ADA: &str = "Tr0ub4dor&3-zebra-ada";
            const MARIE: &str = "correct-horse-battery-marie";
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let server = daemon(&root).await;
            let target = || Some(Target::Local { root: root.clone(), spawn: false });
            let mut app = start(target(), None);
            app.requests.send(Request::BoardMembers(UserCall::Create { username: "ada".into(), password: Password::new(ADA.into()) })).unwrap();
            next(&mut app, |u| match u {
                Update::Who(w) if w.user.is_some() => Some(()),
                _ => None,
            })
            .await;
            app.requests.send(Request::BoardMembers(UserCall::Create { username: "marie".into(), password: Password::new(MARIE.into()) })).unwrap();
            next(&mut app, |u| matches!(u, Update::BoardMemberCreated(_)).then_some(())).await;
            let marie = next(&mut app, |u| match u {
                Update::BoardMembers(list) => list.into_iter().find(|m| m.username == "marie").map(|m| m.id),
                _ => None,
            })
            .await;
            let mut projects = Vec::new();
            for name in ["Demo", "Docs"] {
                app.requests.send(Request::CreateProject { name: name.into(), description: None, folder: None }).unwrap();
                projects.push(next(&mut app, |u| match u {
                    Update::Created(p) => Some(p),
                    _ => None,
                })
                .await);
            }
            let project = projects[0];
            done(&mut app, new_ticket(project, "Discuss me"), None).await;
            let ticket = tickets(&load(&mut app, Some(project)).await, ColumnKind::Backlog)[0].clone();
            let discussion = ticket.conversation_id.expect("every ticket has its discussion");

            // MU2-02 / MU2-04: a post with a mention; in Backlog with no owner it wakes nobody.
            let post = Command::PostTicketMessage(ticket::PostTicketMessage { ticket_id: ticket.id, text: "@marie can you look?".into(), images: vec![], files: vec![] });
            app.requests.send(Request::Board(BoardCall::Write { command: post, expected_version: None, done: None })).unwrap();
            let to = next(&mut app, |u| match u {
                Update::Board(BoardUpdate::Posted { ticket: t, to }) if t == ticket.id => Some(to),
                Update::Board(other @ (BoardUpdate::PostFailed { .. } | BoardUpdate::Failed { .. })) => panic!("refused: {other:?}"),
                _ => None,
            })
            .await;
            assert!(to.is_empty(), "nobody is woken: {to:?}");
            app.requests.send(Request::Board(BoardCall::Discussion(Some(discussion)))).unwrap();
            let messages = next(&mut app, |u| match u {
                Update::Board(BoardUpdate::Discussion(c, result)) if c == discussion => Some(result.expect("the discussion reads")),
                _ => None,
            })
            .await;
            let mine = messages.iter().find(|m| m.system.is_none() && m.text == "@marie can you look?").expect("the post is in the discussion");
            assert_eq!(mine.author_user.as_ref().map(|u| u.username.as_str()), Some("ada"), "a board member's post carries their name (A-01)");
            // System lines carry the server's own sentence, which the app shows as is.
            for line in messages.iter().filter(|m| m.system.is_some()) {
                assert!(!line.text.trim().is_empty(), "{line:?}");
                assert_eq!(crate::screens::discussion::system_text(line.system.as_ref().unwrap(), &line.text), line.text.trim());
            }
            app.requests.send(Request::Board(BoardCall::Discussion(None))).unwrap();

            // MU2-08: #general first, then one channel per project by name.
            app.requests.send(Request::LoadChannels).unwrap();
            let channels = next(&mut app, |u| match u {
                Update::Channels(c) => Some(c),
                _ => None,
            })
            .await
            .expect("this server has channel.list");
            assert_eq!(channels.iter().map(|c| (c.project_id, c.name.as_str())).collect::<Vec<_>>(), [(None, "General"), (Some(projects[0]), "Demo"), (Some(projects[1]), "Docs")]);
            let general = channels[0].conversation_id;
            let demo = channels[1].conversation_id;

            // ada writes in #Demo and names marie.
            app.requests.send(Request::OpenGroupChat(Some(demo))).unwrap();
            assert_eq!(next(&mut app, |u| match u {
                Update::GroupChat(c) => Some(c),
                Update::GroupChatFailed(why) => panic!("{why}"),
                _ => None,
            })
            .await, demo);
            app.requests.send(Request::SendGroup { text: "@marie the spec is ready".into(), images: vec![], files: vec![], channel: Some(demo) }).unwrap();
            let source = next(&mut app, |u| match u {
                Update::Messages(m) if m.conversation_id == demo => m.messages.iter().find(|m| m.text == "@marie the spec is ready").map(|m| m.id),
                _ => None,
            })
            .await;

            // marie's window: #Demo is unread with a mention (B8), #general is not.
            let mut other = start(target(), None);
            other.requests.send(Request::BoardMembers(UserCall::Use { user_id: marie, password: Password::new(MARIE.into()) })).unwrap();
            next(&mut other, |u| match u {
                Update::Who(w) if w.user.as_ref().is_some_and(|u| u.id == marie) => Some(()),
                _ => None,
            })
            .await;
            let counts = |list: &[aurelius_domain::views::ChannelView], id| list.iter().find(|c| c.conversation_id == id).map(|c| (c.unread, c.mentions));
            // Her Team chat shows: its channels follow every chat event.
            other.requests.send(Request::FollowChannels(true)).unwrap();
            let unread = next(&mut other, |u| match u {
                Update::Channels(Some(c)) => Some(c),
                _ => None,
            })
            .await;
            assert_eq!(counts(&unread, demo), Some((1, 1)), "{unread:?}");
            assert_eq!(counts(&unread, general), Some((0, 0)));

            // Opening it reads it, and marks it read on the server.
            other.requests.send(Request::OpenGroupChat(Some(demo))).unwrap();
            next(&mut other, |u| match u {
                Update::Messages(m) if m.conversation_id == demo => Some(()),
                _ => None,
            })
            .await;
            other.requests.send(Request::LoadChannels).unwrap();
            let read = next(&mut other, |u| match u {
                Update::Channels(Some(c)) => Some(c),
                _ => None,
            })
            .await;
            assert_eq!(counts(&read, demo), Some((0, 0)), "{read:?}");

            // Live (team_channels): ada's next message in #Demo reaches marie's open channel
            // and her counts without any reload from her side.
            app.requests.send(Request::SendGroup { text: "Second note".into(), images: vec![], files: vec![], channel: Some(demo) }).unwrap();
            next(&mut other, |u| match u {
                Update::Messages(m) if m.conversation_id == demo && m.messages.iter().any(|m| m.text == "Second note") => Some(()),
                _ => None,
            })
            .await;
            let live = next(&mut other, |u| match u {
                Update::Channels(Some(c)) => Some(c),
                _ => None,
            })
            .await;
            assert_eq!(counts(&live, demo), Some((0, 0)), "the open channel is read as it comes: {live:?}");

            // Trace c2: her Team chat closed, a chat event no longer reads channel.list.
            other.requests.send(Request::FollowChannels(false)).unwrap();
            // Requests go in order: once the discussion answers, the window stopped following
            // (a channel.list of the earlier burst may still come before it).
            other.requests.send(Request::Board(BoardCall::Discussion(Some(discussion)))).unwrap();
            next(&mut other, |u| matches!(u, Update::Board(BoardUpdate::Discussion(c, _)) if c == discussion).then_some(())).await;
            other.requests.send(Request::Board(BoardCall::Discussion(None))).unwrap();
            app.requests.send(Request::SendGroup { text: "Third note".into(), images: vec![], files: vec![], channel: Some(demo) }).unwrap();
            let mut seen = false;
            next(&mut other, |u| match u {
                Update::Channels(_) => panic!("channel.list after the Team chat closed"),
                Update::Messages(m) if m.conversation_id == demo && m.messages.iter().any(|m| m.text == "Third note") => {
                    seen = true;
                    None
                }
                // The chat event's refresh ends with the company; channels would have come before it.
                Update::Company(_) if seen => Some(()),
                _ => None,
            })
            .await;

            // MU2-10: a ticket from the channel's message lands in Backlog.
            let Command::CreateTicket(mut create) = new_ticket(project, "From a message") else { unreachable!() };
            create.from_message = Some(source);
            match write(&mut app, Command::CreateTicket(create), None).await.0 {
                BoardUpdate::Created { .. } => {}
                other => panic!("not made: {other:?}"),
            }
            assert!(tickets(&load(&mut app, Some(project)).await, ColumnKind::Backlog).iter().any(|t| t.title == "From a message"));

            // MU2-12 AC4: Try again on a ticket that is not Done is the discussion's own failure.
            app.requests.send(Request::Board(BoardCall::Write { command: Command::RetryTicketSummary(ticket::RetryTicketSummary { ticket_id: ticket.id }), expected_version: None, done: None })).unwrap();
            let retried = next(&mut app, |u| match u {
                Update::Board(BoardUpdate::SummaryRetried { ticket: t, failure }) if t == ticket.id => Some(failure),
                Update::Board(other @ BoardUpdate::Failed { .. }) => panic!("not the discussion's answer: {other:?}"),
                _ => None,
            })
            .await;
            assert!(retried.is_some(), "nothing to retry before Done");

            drop(other);
            drop(app);
            stop_daemon(&root, server).await;
        }

        async fn stop_daemon(root: &std::path::Path, server: tokio::task::JoinHandle<anyhow::Result<()>>) {
            aurelius_client::bridge::shutdown_daemon(root).await.expect("shutdown");
            tokio::time::timeout(Duration::from_secs(20), server).await.expect("daemon stops").expect("join").expect("clean stop");
        }
    }

    #[test]
    fn a_discussion_write_that_cannot_go_answers_the_discussion() {
        let ticket = aurelius_domain::TicketId::new();
        let post = BoardCall::Write { command: Command::PostTicketMessage(aurelius_domain::ticket::PostTicketMessage { ticket_id: ticket, text: "Hi".into(), images: vec![], files: vec![] }), expected_version: None, done: None };
        assert!(matches!(post.refused("offline".into()), BoardUpdate::PostFailed { ticket: t, .. } if t == ticket), "the Sending… state ends, the draft stays");
        let retry = BoardCall::Write { command: Command::RetryTicketSummary(aurelius_domain::ticket::RetryTicketSummary { ticket_id: ticket }), expected_version: None, done: None };
        assert!(matches!(retry.refused("offline".into()), BoardUpdate::SummaryRetried { failure: Some(_), .. }));
        let cancel = BoardCall::Write { command: Command::CancelTicket(aurelius_domain::ticket::CancelTicket { ticket_id: ticket }), expected_version: None, done: None };
        assert!(matches!(cancel.refused("offline".into()), BoardUpdate::Failed { ticket: Some(t), .. } if t == ticket));
    }

    #[tokio::test(start_paused = true)]
    async fn a_burst_refreshes_at_once_then_at_most_twice_a_second_and_after_its_last_event() {
        // A burst: one refresh at once, one per period while it lasts, one after the last event.
        assert_eq!(refreshes(&[0, 10, 20, 300, 700, 1200], 5000).await, vec![0, 500, 1000, 1500]);
        // No events, no refreshes.
        assert_eq!(refreshes(&[], 5000).await, Vec::<u64>::new());
        // A lone event after a quiet spell refreshes at once.
        assert_eq!(refreshes(&[3000], 5000).await, vec![3000]);
    }
}
