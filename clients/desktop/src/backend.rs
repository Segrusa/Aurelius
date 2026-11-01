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
    ApprovalId, Command, CommandEnvelope, ProviderId, ConversationId, EmployeeId, GuidelineId, GuidelineProposalId, MessageId, ProjectId, RunId, approval,
    connection::{self, ConfigScope, ProviderKind, Purpose, RunDefaults},
    conversation,
    employee::{self, LifecycleStatus},
    guideline::GuidelineTarget,
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
    /// The vault of the company and its projects (feature `vault`).
    pub vault: bool,
    /// The guideline library, its links and proposals (feature `guidelines`).
    pub guidelines: bool,
    /// An existing ticket takes more files (feature `ticket_add_files`, `UpdateTicket::add_files`).
    pub ticket_add_files: bool,
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
    /// Board members and this device's session (protocol 5): sign in and out,
    /// sessions, passwords, disabling.
    BoardMembers(UserCall),
    /// The own Claude login (protocol 5).
    Claude(ClaudeCall),
    /// The board, a ticket and their writes (protocol 4; see [`BoardCall`]).
    Board(BoardCall),
    /// The vault (feature `vault`; see [`VaultCall`]).
    Vault(VaultCall),
    /// The guidelines (feature `guidelines`; see [`GuidelineCall`]).
    Guidelines(GuidelineCall),
    /// A message `looks_like_secret` held back, sent again after "Send anyway" (B10):
    /// its `confirm_secret` is set, so the server stores it with the value redacted.
    SendConfirmed(Command),
}

/// A vault value from `vault.reveal` or the vault key from `vault.key_export`, on its
/// way to the screen or the clipboard: never printed, wiped when dropped (E19, E20).
pub struct Revealed(String);

impl Revealed {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for Revealed {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

impl std::fmt::Debug for Revealed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Overwrite a string's bytes before its memory goes back (E20, without a new dependency).
pub fn wipe(text: &mut String) {
    let mut bytes = std::mem::take(text).into_bytes();
    bytes.fill(0);
    std::hint::black_box(&bytes);
}

/// Which audit log a read asks for: a scope's (none: the company's), every scope's, or one item's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditAsk {
    pub project_id: Option<ProjectId>,
    pub all: bool,
    pub item_id: Option<aurelius_domain::VaultItemId>,
}

/// The vault's reads and writes (feature `vault`). None is kept for a reconnect:
/// a value or a password does not wait in an outbox, and the screen reloads.
#[derive(Debug)]
pub enum VaultCall {
    /// `vault.list` of the company (none) or a project.
    List(Option<ProjectId>),
    /// `vault.reveal`: show or copy one value (an audit row on the server).
    Reveal { item_id: aurelius_domain::VaultItemId, purpose: aurelius_domain::vault::RevealPurpose },
    /// `vault.audit`, newest first; `before`: the page after that row.
    Audit { ask: AuditAsk, before: Option<i64> },
    /// `vault.key_export` with the own password again (E9).
    KeyExport { password: Password },
    Create(aurelius_protocol::VaultCreateParams),
    Update(aurelius_protocol::VaultUpdateParams),
    Archive(aurelius_domain::VaultItemId),
}

impl VaultCall {
    /// A read the gate keeps for after the sign-in (the vault page loads then).
    pub fn is_list(&self) -> bool {
        matches!(self, VaultCall::List(_) | VaultCall::Audit { .. })
    }

    /// What a call that could not go answers, so nothing keeps waiting.
    pub fn refused(&self, message: String) -> VaultUpdate {
        match self {
            VaultCall::List(scope) => VaultUpdate::Listed(*scope, Err(message)),
            VaultCall::Reveal { item_id, purpose } => VaultUpdate::Value { item_id: *item_id, purpose: *purpose, result: Err(message) },
            VaultCall::Audit { ask, before } => VaultUpdate::Audit { ask: *ask, before: *before, result: Err(message) },
            VaultCall::KeyExport { .. } => VaultUpdate::Key(Err(message)),
            VaultCall::Create(_) | VaultCall::Update(_) => VaultUpdate::Saved(Err(message)),
            VaultCall::Archive(item_id) => VaultUpdate::Archived(*item_id, Err(message)),
        }
    }
}

/// The vault key as `vault.key_export` gave it, to save or show once (B8, E9).
#[derive(Debug)]
pub struct KeyFile {
    pub key_id: String,
    pub key: Revealed,
}

/// What the vault hears back. A value only ever travels as [`Revealed`].
#[derive(Debug)]
pub enum VaultUpdate {
    Listed(Option<ProjectId>, Result<aurelius_domain::vault::VaultList, String>),
    Value { item_id: aurelius_domain::VaultItemId, purpose: aurelius_domain::vault::RevealPurpose, result: Result<Revealed, String> },
    Audit { ask: AuditAsk, before: Option<i64>, result: Result<aurelius_domain::vault::VaultAuditPage, String> },
    Key(Result<KeyFile, String>),
    /// `vault.create` or `vault.update` answered.
    Saved(Result<aurelius_domain::vault::VaultItemView, String>),
    Archived(aurelius_domain::VaultItemId, Result<aurelius_domain::vault::VaultItemView, String>),
    /// A `vault.item_*` event (also of another device): read again what shows (MU4-01 AC2).
    Changed,
}

/// The guidelines' reads and writes (feature `guidelines`, docs/product/guidelines).
#[derive(Debug, Clone, PartialEq)]
pub enum GuidelineCall {
    /// `guideline.list`: the library, or (`archived`) only the archived ones.
    List { archived: bool },
    /// `guideline.get`: the current version, or `version`.
    Get { guideline_id: GuidelineId, version: Option<u32> },
    Versions(GuidelineId),
    /// `guideline.effective`: what a run of this employee in this project gets now.
    Effective(EffectiveKey),
    Proposals,
    Affected(GuidelineId),
    StarterSet,
    /// `run.guidelines`: the versions a run had (B6).
    Run(aurelius_domain::RunId),
    Write(GuidelineWrite),
}

/// The employee and the project of a `guideline.effective` (none: without).
pub type EffectiveKey = (Option<EmployeeId>, Option<ProjectId>);

/// A guideline write (a domain command; only a board member).
#[derive(Debug, Clone, PartialEq)]
pub enum GuidelineWrite {
    Create { title: String, text: String, links: Vec<GuidelineTarget> },
    /// `expected_version`: the version the editor started from (P-64: no silent overwrite).
    Update { guideline_id: GuidelineId, expected_version: u32, title: Option<String>, text: Option<String> },
    Archive(GuidelineId),
    Restore(GuidelineId),
    RestoreVersion { guideline_id: GuidelineId, version: u32 },
    /// Links and unlinks, one after the other, in this order; stops at the first refusal.
    Links(Vec<LinkChange>),
    /// `expected_version`: the guideline's version the proposal rests on (a change).
    Decide { proposal_id: GuidelineProposalId, approve: bool, title: Option<String>, text: Option<String>, links: Option<Vec<GuidelineTarget>>, note: Option<String>, expected_version: Option<u32> },
    AddStarter(Vec<String>),
}

/// One link or unlink of the Links dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkChange {
    pub guideline_id: GuidelineId,
    pub target: GuidelineTarget,
    pub link: bool,
}

impl GuidelineCall {
    /// A read: kept by the gate for after the sign-in.
    pub fn is_read(&self) -> bool {
        !matches!(self, GuidelineCall::Write(_))
    }

    /// What the window hears when the call cannot go.
    pub fn refused(&self, failure: GuidelineFailure) -> GuidelineUpdate {
        let why = failure.message.clone();
        match self {
            GuidelineCall::List { archived } => GuidelineUpdate::Listed { archived: *archived, result: Err(why) },
            GuidelineCall::Get { guideline_id, version } => GuidelineUpdate::Detail { guideline_id: *guideline_id, version: *version, result: Err(why) },
            GuidelineCall::Versions(id) => GuidelineUpdate::Versions(*id, Err(why)),
            GuidelineCall::Effective(key) => GuidelineUpdate::Effective(*key, Err(why)),
            GuidelineCall::Proposals => GuidelineUpdate::Proposals(Err(why)),
            GuidelineCall::Affected(id) => GuidelineUpdate::Affected(*id, Err(why)),
            GuidelineCall::StarterSet => GuidelineUpdate::Starter(Err(why)),
            GuidelineCall::Run(run_id) => GuidelineUpdate::Run(*run_id, Err(why)),
            GuidelineCall::Write(write) => GuidelineUpdate::Written { write: write.clone(), done: 0, result: Err(failure) },
        }
    }
}

/// Why a guideline call was refused, with what the window needs to say it well.
#[derive(Debug, Clone, PartialEq)]
pub struct GuidelineFailure {
    pub code: Option<aurelius_domain::error::ErrorCode>,
    pub message: String,
    /// `guideline_too_large`: the combination that overflows (B4).
    pub too_large: Option<aurelius_domain::guideline::GuidelineTooLarge>,
    /// `looks_like_secret`: the vault items' names (never a value).
    pub matched_items: Vec<String>,
}

impl GuidelineFailure {
    /// A refusal this app says itself (not connected, signed out).
    pub fn said(message: &str) -> Self {
        Self { code: None, message: message.into(), too_large: None, matched_items: Vec::new() }
    }

    pub fn of(err: &ClientError) -> Self {
        use aurelius_domain::error::ErrorCode;
        match err {
            ClientError::Protocol(p) if p.code == ErrorCode::UnknownMethod || (p.code == ErrorCode::Validation && p.message.starts_with(GUIDELINES_PLACEHOLDER)) => {
                Self { code: Some(ErrorCode::UnknownMethod), ..Self::said(GUIDELINES_NOT_BUILT) }
            }
            e if not_built(e) => Self { code: Some(ErrorCode::UnknownMethod), ..Self::said(GUIDELINES_NOT_BUILT) },
            ClientError::Protocol(p) if p.code == ErrorCode::BoardMemberRequired => Self { code: Some(p.code), ..Self::said(BOARD_MEMBER_REQUIRED) },
            ClientError::Protocol(p) => Self { code: Some(p.code), message: p.message.clone(), too_large: p.too_large.clone(), matched_items: p.matched_items.clone() },
            e if ends(e) => Self::said("The connection dropped before the server answered. Guidelines reload with what the server has."),
            ClientError::TimedOut(_) => Self::said("The server did not answer in time."),
            e => Self::said(&e.to_string()),
        }
    }
}

/// The text of the daemon's placeholder until AUR-GL-01-F2 (a Validation error).
pub const GUIDELINES_PLACEHOLDER: &str = "Guidelines are not available on this server yet";
pub const GUIDELINES_NOT_BUILT: &str = "This server does not have guidelines yet. Update the server.";

/// What the guidelines hear back.
#[derive(Debug, Clone, PartialEq)]
pub enum GuidelineUpdate {
    Listed { archived: bool, result: Result<Vec<aurelius_domain::guideline::GuidelineView>, String> },
    Detail { guideline_id: GuidelineId, version: Option<u32>, result: Result<aurelius_domain::guideline::GuidelineDetail, String> },
    Versions(GuidelineId, Result<Vec<aurelius_domain::guideline::GuidelineVersionView>, String>),
    Effective(EffectiveKey, Result<aurelius_domain::guideline::EffectiveGuidelines, String>),
    Proposals(Result<Vec<aurelius_domain::guideline::GuidelineProposalView>, String>),
    Affected(GuidelineId, Result<aurelius_domain::guideline::GuidelineAffected, String>),
    Starter(Result<Vec<aurelius_domain::guideline::StarterTemplate>, String>),
    Run(aurelius_domain::RunId, Result<Vec<aurelius_domain::guideline::RunGuideline>, String>),
    /// A write answered; `done`: the link changes that went through before a refusal.
    Written { write: GuidelineWrite, done: usize, result: Result<Option<aurelius_domain::CommandResult>, GuidelineFailure> },
    /// A `guideline.*` event (also of another device): read again what shows.
    Changed,
}

/// A message the server held back because it holds a vault secret's value (B10):
/// the items' names and the message again, with `confirm_secret` set. Its text is
/// the user's own and is never printed.
pub struct SecretHold {
    pub names: Vec<String>,
    pub command: Command,
    /// A board write goes again as one, with the version and the words it had (none: not a board write).
    pub board: Option<(Option<u64>, Option<String>)>,
}

impl std::fmt::Debug for SecretHold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretHold").field("names", &self.names).finish_non_exhaustive()
    }
}

impl SecretHold {
    pub fn new(names: Vec<String>, command: Command) -> Self {
        Self { names, command, board: None }
    }

    /// A ticket write held back: it goes again with the version it checked and what the board says when it works.
    pub fn on_board(mut self, expected_version: Option<u64>, done: Option<String>) -> Self {
        self.board = Some((expected_version, done));
        self
    }

    /// How it goes again after "Send anyway": a ticket write as the board's write, the others as a message.
    pub fn resend(self) -> Request {
        match (self.command, self.board) {
            (command @ Command::PostTicketMessage(_), None) => Request::Board(BoardCall::Write { command, expected_version: None, done: None }),
            (command, Some((expected_version, done))) => Request::Board(BoardCall::Write { command, expected_version, done }),
            (command, None) => Request::SendConfirmed(command),
        }
    }

    /// The text that was held back, to put back where it was written on Cancel.
    pub fn text(&self) -> &str {
        match &self.command {
            Command::SendEmployeeMessage(m) => &m.text,
            Command::SendGroupMessage(m) => &m.text,
            Command::PostTicketMessage(m) => &m.text,
            Command::RevisePlan(c) => &c.feedback,
            Command::SendBackTicket(c) => &c.feedback,
            Command::DecidePlan(c) => c.feedback.as_deref().unwrap_or_default(),
            _ => "",
        }
    }
}

/// The `confirm_secret` of a command B10 can hold back and "Send anyway" can store
/// redacted (AUR-MU-04-F5); none for the rest: names, instructions, memories and paths.
fn confirm_flag(command: &mut Command) -> Option<&mut bool> {
    Some(match command {
        Command::SendEmployeeMessage(m) => &mut m.confirm_secret,
        Command::SendGroupMessage(m) => &mut m.confirm_secret,
        Command::PostTicketMessage(m) => &mut m.confirm_secret,
        Command::SendChatMessage(m) => &mut m.confirm_secret,
        Command::SendProjectMessage(m) => &mut m.confirm_secret,
        Command::CreateTask(c) => &mut c.confirm_secret,
        Command::CreateTaskSchedule(c) => &mut c.confirm_secret,
        Command::UpdateTaskSchedule(c) => &mut c.confirm_secret,
        Command::AddIdea(c) => &mut c.confirm_secret,
        Command::UpdateIdea(c) => &mut c.confirm_secret,
        Command::CreateTicket(c) => &mut c.confirm_secret,
        Command::UpdateTicket(c) => &mut c.confirm_secret,
        Command::RevisePlan(c) => &mut c.confirm_secret,
        Command::SendBackTicket(c) => &mut c.confirm_secret,
        Command::AcceptTicket(c) => &mut c.confirm_secret,
        Command::DecidePlan(c) => &mut c.confirm_secret,
        Command::AnswerQuestion(c) => &mut c.confirm_secret,
        _ => return None,
    })
}

/// A command B10 can hold back, with `confirm_secret` set for "Send anyway"; none for the
/// rest, and none for one that went with it already: refused again, asking again would never end (D-m5).
pub fn confirmed(command: &Command) -> Option<Command> {
    let mut command = command.clone();
    let flag = confirm_flag(&mut command)?;
    if *flag {
        return None;
    }
    *flag = true;
    Some(command)
}

/// Refused although it went with `confirm_secret`: the server redacts the text, never
/// an attached file, so the value is in one of those.
pub fn secret_in_file(command: &Command) -> bool {
    let mut command = command.clone();
    confirm_flag(&mut command).is_some_and(|f| *f) && has_files(&command)
}

/// Whether a command carries files of its own (the server reads them, it cannot redact them).
fn has_files(command: &Command) -> bool {
    match command {
        Command::SendEmployeeMessage(m) => !m.files.is_empty(),
        Command::SendGroupMessage(m) => !m.files.is_empty(),
        Command::PostTicketMessage(m) => !m.files.is_empty(),
        Command::CreateTask(c) => !c.files.is_empty(),
        Command::CreateTaskSchedule(c) => !c.files.is_empty(),
        Command::UpdateTaskSchedule(c) => !c.add_files.is_empty(),
        Command::AddIdea(c) => !c.files.is_empty(),
        Command::UpdateIdea(c) => !c.add_files.is_empty(),
        Command::CreateTicket(c) => !c.files.is_empty(),
        Command::UpdateTicket(c) => c.add_files.as_ref().is_some_and(|f| !f.is_empty()),
        _ => false,
    }
}

/// What the server's refusal tells the user: none when "Send anyway?" can ask; else that the
/// value has to come out, from the attached file when "Send anyway" went already (no second ask).
pub fn secret_refused(command: &Command, names: &[String]) -> Option<String> {
    let mut sent = command.clone();
    match confirm_flag(&mut sent).map(|f| *f) {
        Some(false) => None,
        Some(true) if has_files(command) => Some(secret_in_file_refusal(names)),
        Some(true) => Some(secret_still_refusal(names)),
        None => Some(secret_refusal(names)),
    }
}

/// What "Send anyway" says when the server refuses it again.
pub fn secret_in_file_refusal(names: &[String]) -> String {
    match names {
        [] => "Not sent: a vault secret's value is in an attached file, which is never redacted. Take it out of the attached file and send again.".into(),
        names => format!("Not sent: the value of {} is in an attached file, which is never redacted. Take it out of the attached file and send again.", names.join(", ")),
    }
}

/// "Send anyway" refused again without a file: the server could not store it redacted.
pub fn secret_still_refusal(names: &[String]) -> String {
    match names {
        [] => "Not sent: the server still refuses this as a vault secret. Take the value out and try again.".into(),
        names => format!("Not sent: the server still refuses the value of {}. Take the value out and try again.", names.join(", ")),
    }
}

/// `looks_like_secret`: the names of the items the text holds a value of (never a value).
pub fn secret_names(err: &ClientError) -> Option<Vec<String>> {
    match err {
        ClientError::Protocol(p) if p.code == aurelius_domain::error::ErrorCode::LooksLikeSecret => Some(p.matched_items.clone()),
        _ => None,
    }
}

/// A refused text that is no message (a ticket, a task): the value must come out first.
pub fn secret_refusal(names: &[String]) -> String {
    format!("This looks like a vault secret: {}. Take the value out and try again.", names.join(", "))
}

/// What a refused vault call tells the user (never a value, never the params).
pub fn vault_failure(err: &ClientError) -> String {
    use aurelius_domain::error::ErrorCode;
    match err {
        e if not_built(e) => VAULT_NOT_BUILT.into(),
        ClientError::Protocol(p) if p.code == ErrorCode::UnknownMethod => VAULT_NOT_BUILT.into(),
        ClientError::Protocol(p) if p.code == ErrorCode::VaultKindLocked => VAULT_KIND_LOCKED.into(),
        ClientError::Protocol(p) if p.code == ErrorCode::VaultUndecryptable => VAULT_UNDECRYPTABLE.into(),
        ClientError::Protocol(p) if p.code == ErrorCode::BoardMemberRequired => BOARD_MEMBER_REQUIRED.into(),
        ClientError::Protocol(p) if p.code == ErrorCode::VersionConflict => BOARD_CONFLICT.into(),
        ClientError::Protocol(p) if p.code == ErrorCode::WrongPassword => "That password is wrong.".into(),
        ClientError::Protocol(p) => p.message.clone(),
        e if ends(e) => VAULT_CONNECTION_DROPPED.into(),
        ClientError::TimedOut(_) => "The server did not answer in time.".into(),
        e => e.to_string(),
    }
}

pub const VAULT_NOT_BUILT: &str = "This server does not have the vault yet. Update the server.";
pub const VAULT_KIND_LOCKED: &str = "A secret stays a secret, and nothing becomes one. Make a new item instead.";
pub const VAULT_UNDECRYPTABLE: &str = "Cannot be decrypted: the server's vault key is missing or does not fit this value.";
pub const VAULT_CONNECTION_DROPPED: &str = "The connection dropped before the server answered. The vault reloads with what the server has.";

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
        Command::UseMyLogin(c) => c.ticket_id,
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
        e if ends(e) => BOARD_CONNECTION_DROPPED.into(),
        e if not_built(e) => NOT_BUILT.into(),
        ClientError::Protocol(p) if p.code == aurelius_domain::error::ErrorCode::VersionConflict => BOARD_CONFLICT.into(),
        ClientError::Protocol(p) if p.code == aurelius_domain::error::ErrorCode::LooksLikeSecret => secret_refusal(&p.matched_items),
        ClientError::Protocol(p) => p.message.clone(),
        e => e.to_string(),
    }
}

/// A ticket post held back by B10, under its draft while the window asks.
pub const NOT_SENT_SECRET: &str = "Not sent: this looks like a vault secret.";

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

/// A one-time code Claude showed after the sign-in at Anthropic, on its way to
/// `claude_login.input`: never printed, logged or kept after the call (Sentinel C5–C8).
pub struct LoginCode(String);

impl LoginCode {
    pub fn new(text: String) -> Self {
        Self(text)
    }
}

impl std::fmt::Debug for LoginCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Board members and this device's session (protocol 5).
#[derive(Debug)]
pub enum UserCall {
    List,
    /// A new board member. Without a session only the first (the onboarding),
    /// and this device then signs in as them at once.
    Create { username: String, password: Password },
    /// Sign in on this device (MU3-02); its session token goes to the OS credential store.
    Login { username: String, password: Password },
    /// Sign out on this device (MU3-04 AC3): its session ends and its token is forgotten.
    Logout,
    /// The signed-in devices of a board member (none: the own, MU3-04 AC1/AC4).
    Sessions(Option<aurelius_domain::UserId>),
    /// End one session (any board member's); `of` is whose list to read again.
    Revoke { session_id: aurelius_domain::SessionId, of: Option<aurelius_domain::UserId> },
    /// End every own session but this device's ("Sign out all other devices").
    RevokeOthers,
    /// The own password (with the current one; the other sessions end), or a
    /// temporary one for another board member (B10: all their sessions end).
    SetPassword { user_id: Option<aurelius_domain::UserId>, old: Option<Password>, new: Password },
    /// Disable a board member (MU3-05 AC3/AC4).
    Disable(aurelius_domain::UserId),
}

impl UserCall {
    /// A call whose form waits for its answer (it holds a password).
    pub fn waits(&self) -> bool {
        matches!(self, UserCall::Create { .. } | UserCall::Login { .. } | UserCall::SetPassword { .. })
    }
}

/// The own Claude login (protocol 5, MU3-09).
#[derive(Debug)]
pub enum ClaudeCall {
    Status,
    /// Connect: the server starts `claude auth login`; its prompts come as `ClaudeUpdate::Prompt`.
    Link,
    /// The code Claude showed, for the running link.
    Input { link_id: String, code: LoginCode },
    Cancel(String),
    /// Disconnect (`claude auth logout`, the folder removed).
    Unlink,
}

/// What the server says about the own Claude login.
#[derive(Debug, Clone, PartialEq)]
pub enum ClaudeUpdate {
    Status(aurelius_domain::user::ClaudeLoginView),
    /// A link runs (its id): the prompt follows.
    Linking(String),
    Prompt(LoginPrompt),
    /// A call failed: what to say (never the code).
    Failed(String),
}

/// `ServerMessage::ClaudeLoginPrompt` as the window keeps it; its sign-in link
/// (with its `state`) is not printed either (Sentinel C6).
#[derive(Clone, PartialEq)]
pub struct LoginPrompt {
    pub link_id: String,
    pub url: Option<String>,
    pub asks_code: bool,
    pub error: Option<String>,
}

impl From<aurelius_protocol::ClaudeLoginPrompt> for LoginPrompt {
    fn from(p: aurelius_protocol::ClaudeLoginPrompt) -> Self {
        Self { link_id: p.link_id, url: p.url, asks_code: p.asks_code, error: p.error }
    }
}

impl std::fmt::Debug for LoginPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginPrompt").field("link_id", &self.link_id).field("url", &self.url.as_ref().map(|_| "<hidden>")).field("asks_code", &self.asks_code).field("error", &self.error).finish()
    }
}

/// What the account's actions answered (Settings → Account, Board members).
#[derive(Debug, Clone, PartialEq)]
pub enum AccountUpdate {
    /// The own password changed (`None`), or a temporary one was set for this board member.
    PasswordSaved(Option<String>),
    /// The password form's refusal, by its field.
    PasswordFailed(MemberFailure),
    /// A session ended (and how many, for "Sign out all other devices").
    Revoked(usize),
    Disabled(aurelius_domain::user::UserView),
    Failed(String),
}

/// Which field of the board member forms a refusal belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberField {
    Username,
    Password,
    /// The current (or temporary) password of a password change.
    Current,
}

/// Why a board member form was refused, for the form that asked.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberFailure {
    pub field: Option<MemberField>,
    pub message: String,
    /// `too_many_attempts`: when the next try is taken (the sign-in counts down).
    pub retry_after_ms: Option<u64>,
}

impl MemberFailure {
    pub fn general(message: impl Into<String>) -> Self {
        Self { field: None, message: message.into(), retry_after_ms: None }
    }
}

/// The server's refusal as the forms show it: by the field it is about, in the
/// words of the design (§11.1). Never repeats what was typed.
pub fn member_failure(err: &ClientError) -> MemberFailure {
    use aurelius_domain::error::ErrorCode;
    if not_built(err) {
        return MemberFailure::general(NOT_BUILT);
    }
    let (field, message) = match err {
        ClientError::Protocol(p) => match p.code {
            ErrorCode::UsernameTaken => (Some(MemberField::Username), "This username is taken.".to_string()),
            ErrorCode::InvalidUsername => (Some(MemberField::Username), p.message.clone()),
            ErrorCode::WeakPassword => (Some(MemberField::Password), p.message.clone()),
            ErrorCode::WrongPassword => (Some(MemberField::Current), "That password is wrong.".to_string()),
            ErrorCode::TooManyAttempts => (None, p.message.clone()),
            _ => (None, p.message.clone()),
        },
        other => (None, other.to_string()),
    };
    let retry_after_ms = match err {
        ClientError::Protocol(p) => p.retry_after_ms,
        _ => None,
    };
    MemberFailure { field, message, retry_after_ms }
}

/// The one refusal of the sign-in (ui-ontwerp §1): never which of the two was wrong.
pub const SIGN_IN_WRONG: &str = "Username or password is wrong.";

/// A refused `user.login`: one generic text for a wrong name, a wrong password or a
/// disabled board member; too many tries with the wait ("Try again in 0:30").
pub fn login_failure(err: &ClientError) -> MemberFailure {
    use aurelius_domain::error::ErrorCode;
    if ends(err) && !signed_out(err) {
        return MemberFailure::general(MEMBER_CONNECTION_DROPPED);
    }
    match err {
        ClientError::Protocol(p) if p.code == ErrorCode::TooManyAttempts => {
            let wait = p.retry_after_ms.unwrap_or(60_000);
            MemberFailure { field: None, message: aurelius_domain::AppError::too_many_attempts(wait).message, retry_after_ms: Some(wait) }
        }
        ClientError::Protocol(p) if matches!(p.code, ErrorCode::WrongPassword | ErrorCode::Unauthenticated | ErrorCode::NotFound) => MemberFailure::general(SIGN_IN_WRONG),
        other => member_failure(other),
    }
}

/// What a lost connection during a board member form tells it.
pub const MEMBER_CONNECTION_DROPPED: &str = "The connection dropped. Check the list of board members and try again.";

/// A failed board member form as it shows it: a refusal by its field, a lost
/// connection as such (the server may have done it anyway).
pub fn member_call_failure(err: &ClientError) -> MemberFailure {
    if ends(err) && !signed_out(err) {
        return MemberFailure::general(MEMBER_CONNECTION_DROPPED);
    }
    member_failure(err)
}

/// The account's other refusals in the user's words.
pub fn account_failure(err: &ClientError) -> String {
    use aurelius_domain::error::ErrorCode;
    match err {
        e if not_built(e) => NOT_BUILT.into(),
        ClientError::Protocol(p) if p.code == ErrorCode::LastBoardMember => LAST_BOARD_MEMBER.into(),
        ClientError::Protocol(p) => p.message.clone(),
        e if ends(e) => "The connection dropped before the server answered. Check again once it is back.".into(),
        e => e.to_string(),
    }
}

/// `last_board_member` (MU3-05 AC4).
pub const LAST_BOARD_MEMBER: &str = "This is the last active board member. Someone must be able to sign in, so they can't be disabled.";

/// How this device shows in `session.list` ("Desktop · dev-laptop (Linux)").
pub fn device_name() -> String {
    let os = match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    };
    format!("Desktop · {} ({os})", aurelius_host::host_name())
}

/// Where this device keeps its session token for a server (S8): the OS credential
/// store, never ui.json, a log or a notice.
pub trait TokenStore: Send + Sync {
    fn get(&self, server: &str) -> Option<String>;
    fn set(&self, server: &str, token: &str);
    fn delete(&self, server: &str);
}

/// `aurelius_host::secrets` (Windows Credential Manager, the macOS Keychain, a 0600 file elsewhere).
pub struct OsTokens;

fn token_ref(server: &str) -> String {
    format!("desktop-session-{server}")
}

impl TokenStore for OsTokens {
    fn get(&self, server: &str) -> Option<String> {
        aurelius_host::secrets::get(&token_ref(server)).ok().flatten().filter(|t| !t.is_empty())
    }

    fn set(&self, server: &str, token: &str) {
        if let Err(e) = aurelius_host::secrets::set(&token_ref(server), token) {
            // The error is the store's, never the token: this device signs in again next time.
            tracing::warn!("could not keep this device's session: {e}");
        }
    }

    fn delete(&self, server: &str) {
        let _ = aurelius_host::secrets::delete(&token_ref(server));
    }
}

/// The session token of one server on this device.
#[derive(Clone)]
pub struct Tokens {
    server: String,
    store: std::sync::Arc<dyn TokenStore>,
}

impl Tokens {
    pub fn new(server: String, store: std::sync::Arc<dyn TokenStore>) -> Self {
        Self { server, store }
    }

    /// In the OS credential store, under the server's key (`AppView::server_key`).
    pub fn os(server: String) -> Self {
        Self::new(server, std::sync::Arc::new(OsTokens))
    }

    fn get(&self) -> Option<String> {
        self.store.get(&self.server)
    }

    /// Keep a token the server gave; an empty one (a server without sessions yet) is not kept.
    fn keep(&self, token: &str) {
        if !token.is_empty() {
            self.store.set(&self.server, token);
        }
    }

    pub fn forget(&self) {
        self.store.delete(&self.server);
    }
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens").field("server", &self.server).finish_non_exhaustive()
    }
}

/// The gate's notice when a write needs a board member and this window has none.
pub const BOARD_MEMBER_REQUIRED: &str = "Sign in first: this needs a board member.";

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
    /// Whether the server has board members, and who is signed in in this window
    /// (after each hello, `user.login`, `user.logout` and the first `user.create`).
    Who(aurelius_domain::user::WhoAmI),
    /// This device's session ended on the server (revoked, a new password, disabled,
    /// expired): the sign-in screen says so (MU3-04 AC2, MU3-06 AC3).
    SignedOut(aurelius_domain::user::SignOutReason),
    /// A board member's signed-in devices (none: the own), or why they did not load.
    Sessions(Option<aurelius_domain::UserId>, Result<aurelius_domain::user::SessionsSnapshot, String>),
    Account(AccountUpdate),
    Claude(ClaudeUpdate),
    /// The server's board members, oldest first.
    BoardMembers(Vec<aurelius_domain::user::UserView>),
    /// Another board member was made by this window (its name).
    BoardMemberCreated(String),
    BoardMemberFailed(MemberFailure),
    /// A write was refused for want of an active board member.
    BoardMemberRequired,
    Board(BoardUpdate),
    Vault(VaultUpdate),
    Guidelines(GuidelineUpdate),
    /// A message held back by `looks_like_secret` (B10): ask "Send anyway?".
    LooksLikeSecret(SecretHold),
}

pub struct Handle {
    pub requests: mpsc::UnboundedSender<Request>,
    pub updates: mpsc::UnboundedReceiver<Update>,
}

/// Connect to `target` (`None`: no server yet) with the session this device
/// keeps for it in `tokens`. Dropping the handle's request sender ends the
/// thread, also while offline.
pub fn start(target: Option<Target>, tokens: Tokens) -> Handle {
    let (req_tx, req_rx) = mpsc::unbounded_channel();
    let (upd_tx, upd_rx) = mpsc::unbounded_channel();
    std::thread::Builder::new()
        .name("aurelius-backend".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
            runtime.block_on(run(target, tokens, req_rx, upd_tx));
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
            Request::BoardMembers(call) if call.waits() => Update::BoardMemberFailed(MemberFailure::general(text)),
            Request::Board(call) if call.is_write() => Update::Board(call.refused(text.into())),
            Request::Vault(call) => Update::Vault(call.refused(text.into())),
            Request::Guidelines(call) => Update::Guidelines(call.refused(GuidelineFailure::said(text))),
            _ => Update::Error(text.into()),
        });
    }
}

async fn run(target: Option<Target>, tokens: Tokens, mut requests: mpsc::UnboundedReceiver<Request>, updates: mpsc::UnboundedSender<Update>) {
    let Some(target) = target else {
        idle(&mut requests, &updates, Connection::NoServer).await;
        return;
    };
    let remote_target = matches!(target, Target::Remote(_));
    // Commands not yet confirmed by a receipt; retried with the same ID after reconnect,
    // each with the board member who made it: only theirs go (R3 M2, MU3-08).
    let mut outbox: VecDeque<(aurelius_domain::UserId, CommandEnvelope)> = VecDeque::new();
    // Who was signed in last on this window: an offline write is theirs.
    let mut me: Option<aurelius_domain::UserId> = None;
    let mut open_conversation: Option<ConversationId> = None;
    let mut open_memory: Option<ProjectId> = None;
    let mut open_ideas: Option<Option<ProjectId>> = None;
    // A ticket's discussion in the panel, and whether the Team chat shows its channels (AUR-MU-02).
    let mut open_discussion: Option<ConversationId> = None;
    let mut channels_shown = false;
    let mut backoff = Duration::from_millis(250);

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
                            Some(Request::BoardMembers(call)) if call.waits() => {
                                let _ = updates.send(Update::BoardMemberFailed(MemberFailure::general("Not connected to the server. Try again when it is back.")));
                            }
                            // Neither is a Claude login's code: the link it was for ends with the connection.
                            Some(Request::Claude(_)) => {
                                let _ = updates.send(Update::Claude(ClaudeUpdate::Failed("Not connected to the server. Try again when it is back.".into())));
                            }
                            // The board's writes are not kept for later; its reads come again once online.
                            Some(Request::Board(call)) => if call.is_write() {
                                let _ = updates.send(Update::Board(call.refused("Not connected to the server. Try again when it is back.".into())));
                            },
                            // Nor the vault's: no value or password waits for later; the page reloads once online.
                            Some(Request::Vault(call)) => {
                                let _ = updates.send(Update::Vault(call.refused("Not connected to the server. Try again when it is back.".into())));
                            }
                            // Nor the guidelines': a write is not kept for later; the page reads again once online.
                            Some(Request::Guidelines(call)) => {
                                let _ = updates.send(Update::Guidelines(call.refused(GuidelineFailure::said("Not connected to the server. Try again when it is back."))));
                            }
                            Some(other) => match me {
                                Some(by) => if let Some(env) = to_envelope(other) { outbox.push_back((by, env)) },
                                // Signed out: nobody's write to keep (the gate refuses it too).
                                None => refuse_at_gate(other, &updates),
                            },
                        },
                    }
                }
                backoff = (backoff * 2).min(Duration::from_secs(5));
                continue;
            }
        };
        backoff = Duration::from_millis(250);
        // One receiver for the connection: a sign-out pushed just before the server
        // closes is still read after the close (MU3-04 AC2).
        let mut pushes = client.session_pushes();
        let mut who = match resume(&client, &tokens).await {
            Ok((who, ended)) => {
                if let Some(reason) = ended {
                    let _ = updates.send(Update::SignedOut(reason));
                }
                who
            }
            Err(_) => continue, // lost again: reconnect
        };
        me = who.user.as_ref().map(|u| u.id);
        let _ = updates.send(Update::Who(who.clone()));
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
                vault: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::VAULT),
                guidelines: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::GUIDELINES),
                ticket_add_files: client.hello.features.iter().any(|f| f == aurelius_protocol::feature::TICKET_ADD_FILES),
            },
        }));

        // Signed in, or back at the gate after signing out on this connection.
        let ended = loop {
            let mut held = VecDeque::new();
            if gated(&who) {
                // Nothing loads without a session (the gate, MU3-06): only the sign-in,
                // the onboarding or the temporary password's change.
                // The gate tells the window each change itself (a temporary password's sign-in, QA3 D2).
                match gate(&client, &mut requests, &updates, &tokens, &mut pushes, who.clone(), &mut held).await {
                    Ok(Some(now)) => who = now,
                    Ok(None) => return,
                    Err(d) => break signed_out_before(d, &mut pushes, &tokens, &updates),
                }
            }
            let Some(user) = who.user.as_ref().map(|u| u.id) else { continue };
            me = Some(user);
            match session(&client, &mut requests, &mut events, &mut streams, &updates, &mut outbox, &mut open_conversation, &mut open_memory, &mut open_ideas, &tokens, &mut open_discussion, &mut channels_shown, &mut pushes, held, user).await {
                Ok(Ended::Closed) => return, // UI dropped the request channel: window closed
                Ok(Ended::SignedOut) => {
                    who = aurelius_domain::user::WhoAmI { board_members_exist: true, user: None, must_change_password: false };
                    me = None;
                }
                Err(d) => break signed_out_before(d, &mut pushes, &tokens, &updates),
            }
        };
        // Signed out before or by the drop: a write made offline now is nobody's.
        if ended.2 || tokens.get().is_none() {
            me = None;
        }
        match ended {
            Disconnected(Some(reason), _, _) => {
                idle(&mut requests, &updates, Connection::Unpaired(reason)).await;
                return;
            }
            Disconnected(None, notice, signed_out) => {
                if signed_out {
                    // The server no longer knows this session: sign in again after the reconnect.
                    tokens.forget();
                    let _ = updates.send(Update::SignedOut(aurelius_domain::user::SignOutReason::Expired));
                }
                if let Some(notice) = notice {
                    let _ = updates.send(Update::Error(notice));
                }
                continue; // reconnect
            }
        }
    }
}

/// The connection dropped: when the server said why just before (its `signed_out`
/// push, read only now), the window gets that reason, the token goes, and the
/// reconnect does not report it again as expired.
fn signed_out_before(d: Disconnected, pushes: &mut mpsc::UnboundedReceiver<aurelius_client::SessionPush>, tokens: &Tokens, updates: &mpsc::UnboundedSender<Update>) -> Disconnected {
    while let Ok(push) = pushes.try_recv() {
        if let aurelius_client::SessionPush::SignedOut(reason) = push {
            tokens.forget();
            let _ = updates.send(Update::SignedOut(reason));
            let _ = updates.send(Update::Who(aurelius_domain::user::WhoAmI { board_members_exist: true, user: None, must_change_password: false }));
            return Disconnected(d.0, d.1, false);
        }
    }
    d
}

/// How a signed-in session ended without losing the connection.
enum Ended {
    /// The window closed (its request sender is gone).
    Closed,
    /// Signed out on this device, or the server ended its session: back to the gate.
    SignedOut,
}

/// Only the gate's screens until there is a session without a temporary password.
pub fn gated(who: &aurelius_domain::user::WhoAmI) -> bool {
    who.user.is_none() || who.must_change_password
}

/// The hello with the session this device kept for the server (MU3-03 AC2): its
/// board member is back, or the token is forgotten and, when the server knew it
/// no longer, the sign-in screen says this device was signed out.
async fn resume(client: &Client, tokens: &Tokens) -> Result<(aurelius_domain::user::WhoAmI, Option<aurelius_domain::user::SignOutReason>), ClientError> {
    let who_of = |h: &aurelius_protocol::HelloResult| aurelius_domain::user::WhoAmI { board_members_exist: h.board_members_exist, user: h.user.clone(), must_change_password: h.must_change_password };
    let Some(token) = tokens.get() else { return Ok((who_of(&client.hello), None)) };
    match client.resume(&token).await {
        Ok(hello) if hello.user.is_some() => Ok((who_of(&hello), None)),
        Ok(hello) => {
            tokens.forget();
            Ok((who_of(&hello), Some(aurelius_domain::user::SignOutReason::Expired)))
        }
        Err(e) if e.is_disconnect() => Err(e),
        // Refused (a damaged token): as unknown, sign in again.
        Err(_) => {
            tokens.forget();
            Ok((who_of(&client.hello), Some(aurelius_domain::user::SignOutReason::Expired)))
        }
    }
}

/// Before a session (protocol 5): the sign-in, the onboarding's first board member
/// and a temporary password's change; nothing else reaches the server. Reads the
/// window asks meanwhile wait in `held` and go once signed in; writes are refused.
/// Each change of who is signed in goes to the window as `Update::Who`.
/// Answers who is signed in now, or none when the window closed.
async fn gate(
    client: &Client,
    requests: &mut mpsc::UnboundedReceiver<Request>,
    updates: &mpsc::UnboundedSender<Update>,
    tokens: &Tokens,
    pushes: &mut mpsc::UnboundedReceiver<aurelius_client::SessionPush>,
    mut who: aurelius_domain::user::WhoAmI,
    held: &mut VecDeque<Request>,
) -> Result<Option<aurelius_domain::user::WhoAmI>, Disconnected> {
    const HELD_MAX: usize = 400;
    // Whether the connection still lives: nothing else asks the server meanwhile.
    let mut alive = tokio::time::interval(Duration::from_secs(5));
    alive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    alive.tick().await;
    while gated(&who) {
        tokio::select! {
            request = requests.recv() => {
                let Some(request) = request else { return Ok(None) };
                match request {
                    Request::BoardMembers(call) => {
                        if let Some(now) = gate_call(client, call, updates, tokens, &who).await? {
                            who = now;
                            // At once, also while still at the gate: a sign-in with a temporary
                            // password goes on to "Choose a new password" (QA3 D2).
                            let _ = updates.send(Update::Who(who.clone()));
                        }
                    }
                    // The window shows only the gate, so what it asks are the reads of its
                    // page; they wait. A write (one made just before a sign-out) is refused:
                    // after the sign-in it would go as whoever signs in (R3 M2).
                    other if held_read(&other) => {
                        if held.len() >= HELD_MAX {
                            held.pop_front();
                        }
                        held.push_back(other);
                    }
                    other => refuse_at_gate(other, updates),
                }
            }
            push = pushes.recv() => {
                // The server ends this connection right after a sign-out.
                if let Some(aurelius_client::SessionPush::SignedOut(reason)) = push {
                    tokens.forget();
                    let _ = updates.send(Update::SignedOut(reason));
                }
            }
            _ = alive.tick() => {
                client.user_whoami().await.map_err(Disconnected::from)?;
            }
        }
    }
    Ok(Some(who))
}

/// One board member call at the gate. Answers who is signed in when it changed.
async fn gate_call(client: &Client, call: UserCall, updates: &mpsc::UnboundedSender<Update>, tokens: &Tokens, who: &aurelius_domain::user::WhoAmI) -> Result<Option<aurelius_domain::user::WhoAmI>, Disconnected> {
    let refused = |err: ClientError, failure: MemberFailure| -> Result<Option<aurelius_domain::user::WhoAmI>, Disconnected> {
        let _ = updates.send(Update::BoardMemberFailed(failure));
        if ends(&err) && !signed_out(&err) {
            return Err(err.into());
        }
        Ok(None)
    };
    match call {
        UserCall::List => {
            refresh_board_members(client, updates).await?;
            Ok(None)
        }
        UserCall::Login { username, password } => match client.user_login(username.trim(), &password.0, &device_name()).await {
            Ok(login) => {
                drop(password);
                tokens.keep(&login.session_token);
                Ok(Some(aurelius_domain::user::WhoAmI { board_members_exist: true, user: Some(login.user), must_change_password: login.must_change_password }))
            }
            Err(err) => {
                let failure = login_failure(&err);
                refused(err, failure)
            }
        },
        // The onboarding (MU3-02 AC5, MU1-20): the first board member, then signed in at once
        // with the same password, so this device keeps a session like after a sign-in.
        UserCall::Create { username, password } => {
            if let Err(err) = client.user_create(&username, &password.0).await {
                let failure = member_call_failure(&err);
                return refused(err, failure);
            }
            let login = client.user_login(username.trim(), &password.0, &device_name()).await;
            drop(password);
            match login {
                Ok(login) => {
                    tokens.keep(&login.session_token);
                    let _ = refresh_board_members(client, updates).await;
                    Ok(Some(aurelius_domain::user::WhoAmI { board_members_exist: true, user: Some(login.user), must_change_password: login.must_change_password }))
                }
                // Made, but no session: whatever the server says now (the sign-in screen).
                Err(err) if !ends(&err) || signed_out(&err) => Ok(Some(client.user_whoami().await.map_err(Disconnected::from)?)),
                Err(err) => Err(err.into()),
            }
        }
        // A temporary password's change (B10, MU3-05): with the temporary one.
        UserCall::SetPassword { user_id: None, old, new } => {
            let result = client.set_password(None, old.as_ref().map(|p| p.0.as_str()), &new.0).await;
            drop((old, new));
            match result {
                Ok(()) => {
                    let _ = updates.send(Update::Account(AccountUpdate::PasswordSaved(None)));
                    Ok(Some(aurelius_domain::user::WhoAmI { must_change_password: false, ..who.clone() }))
                }
                Err(err) => {
                    let failure = member_call_failure(&err);
                    let _ = updates.send(Update::Account(AccountUpdate::PasswordFailed(failure.clone())));
                    refused(err, failure)
                }
            }
        }
        UserCall::Logout => {
            tokens.forget();
            let now = match client.user_logout().await {
                Ok(now) => now,
                Err(err) if ends(&err) && !signed_out(&err) => return Err(err.into()),
                Err(_) => aurelius_domain::user::WhoAmI { board_members_exist: true, user: None, must_change_password: false },
            };
            Ok(Some(now))
        }
        // The rest needs a session.
        other => {
            if other.waits() {
                let _ = updates.send(Update::BoardMemberFailed(MemberFailure::general(BOARD_MEMBER_REQUIRED)));
            }
            Ok(None)
        }
    }
}

/// A read the gate keeps for after the sign-in (its page loads then); everything
/// else changes something and must not go as the next board member (R3 M2).
fn held_read(request: &Request) -> bool {
    match request {
        Request::Board(call) => !call.is_write(),
        Request::Vault(call) => call.is_list(),
        Request::Guidelines(call) => call.is_read(),
        Request::AppKeys(call) => matches!(call, AppKeyCall::List),
        Request::Plugins(call) => matches!(call, PluginsCall::List | PluginsCall::Available | PluginsCall::ManagerGet(_) | PluginsCall::ProjectGet(_)),
        Request::Backups(call) => matches!(call, BackupsCall::List),
        Request::Remote(call) => matches!(call, RemoteCall::Status),
        Request::OpenConversation(_) | Request::OpenGroupChat(_) | Request::LoadChannels | Request::FollowChannels(_) => true,
        Request::LoadMemory(_) | Request::LoadMemoryContext { .. } | Request::LoadIdeas(_) | Request::LoadContext(_) | Request::LoadScreenshot(_) | Request::LoadFullActivity(..) => true,
        Request::LoadFile(_) | Request::LoadUpload(_) | Request::LoadUploadThumb(_) | Request::LoadSubagents(_) | Request::LoadPreviewMessages(_) | Request::LoadMarkdownFile(..) | Request::LoadArtifactContent { .. } => true,
        Request::LoadUsage | Request::LoadUsageHours(_) | Request::LoadSubscription(_) | Request::CheckUpdate(_) | Request::LoadServerLoad | Request::LoadBrowser | Request::LoadArtifactSetting => true,
        Request::LoadUsageTotals { .. } | Request::LoadTraceSetting | Request::LoadTrace(_) | Request::LoadInstructionVersions(_) | Request::LoadAutoAllow | Request::LoadManagementMode => true,
        Request::LoadMcp | Request::LoadMcpAlwaysAllow | Request::BrowseDir { .. } => true,
        _ => false,
    }
}

/// A write without a session, answered as its own kind so no form keeps waiting.
fn refuse_at_gate(request: Request, updates: &mpsc::UnboundedSender<Update>) {
    let _ = updates.send(match request {
        Request::BoardMembers(call) if call.waits() => Update::BoardMemberFailed(MemberFailure::general(BOARD_MEMBER_REQUIRED)),
        Request::Claude(_) => Update::Claude(ClaudeUpdate::Failed(BOARD_MEMBER_REQUIRED.into())),
        Request::Board(call) => Update::Board(call.refused(BOARD_MEMBER_REQUIRED.into())),
        Request::Vault(call) => Update::Vault(call.refused(BOARD_MEMBER_REQUIRED.into())),
        Request::Guidelines(call) => Update::Guidelines(call.refused(GuidelineFailure::said(BOARD_MEMBER_REQUIRED))),
        Request::AppKeys(_) => Update::AppKeys(AppKeyUpdate::Failed(BOARD_MEMBER_REQUIRED.into())),
        _ => Update::Error(BOARD_MEMBER_REQUIRED.into()),
    });
}

/// Why writes from before a sign-out did not go once someone else signed in (R3 M2).
pub const NOT_SENT_SIGNED_OUT: &str = "Not sent: you were signed out.";

/// Drops the writes another board member made on this window (before a sign-out,
/// an expired or revoked session, or a forgotten token): they never go as `me`.
fn keep_own(outbox: &mut VecDeque<(aurelius_domain::UserId, CommandEnvelope)>, me: aurelius_domain::UserId, updates: &mpsc::UnboundedSender<Update>) {
    let before = outbox.len();
    outbox.retain(|(by, _)| *by == me);
    if outbox.len() < before {
        let _ = updates.send(Update::Error(NOT_SENT_SIGNED_OUT.into()));
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
            envelope(Command::SendEmployeeMessage(employee::SendEmployeeMessage { confirm_secret: false, employee_id, text, conversation_id, images, files, interrupt }), None)
        }
        Request::SendGroup { text, images, files, channel } => envelope(Command::SendGroupMessage(aurelius_domain::group_chat::SendGroupMessage { confirm_secret: false, text, images, files, channel }), None),
        Request::DecideApproval { approval_id, action_digest, approve, always_allow } => {
            envelope(Command::DecideApproval(approval::DecideApproval { approval_id, action_digest, approve, always_allow }), None)
        }
        Request::AnswerQuestion { approval_id, answers } => envelope(Command::AnswerQuestion(approval::AnswerQuestion { confirm_secret: false, approval_id, answers }), None),
        Request::AddConnection { provider, name, runtime_path } => envelope(
            Command::AddProvider(connection::AddProvider { provider, display_name: name, runtime_path, default_model: None }),
            None,
        ),
        Request::CreateTask { project_id, employee_id, title, goal, criteria, run, files } => envelope(
            Command::CreateTask(aurelius_domain::task::CreateTask { confirm_secret: false, files, project_id, employee_id, title, goal, acceptance_criteria: criteria, depends_on: vec![], run }),
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
        Request::AddIdea { project_id, title, note, kind, files } => envelope(Command::AddIdea(aurelius_domain::idea::AddIdea { confirm_secret: false, project_id, title, note, kind, files }), None),
        Request::UpdateIdea { idea_id, title, note, kind, add_files, remove_files } => envelope(Command::UpdateIdea(aurelius_domain::idea::UpdateIdea { confirm_secret: false, idea_id, title, note, kind, add_files, remove_files }), None),
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
        Request::DecidePlan { approval_id, decision, feedback } => envelope(Command::DecidePlan(aurelius_domain::approval::DecidePlan { confirm_secret: false, approval_id, decision, feedback }), None),
        Request::SetHeartbeat { employee_id, every_minutes } => envelope(
            Command::SetHeartbeat(employee::SetHeartbeat { employee_id, every_minutes, quiet_start_min: None, quiet_end_min: None, daily_token_budget: None }),
            None,
        ),
        Request::DeleteConversation(conversation_id) => envelope(Command::DeleteConversation(conversation::DeleteConversation { conversation_id }), None),
        Request::SetEmployeeFolder { employee_id, path } => envelope(Command::SetEmployeeFolder(employee::SetEmployeeFolder { employee_id, path }), None),
        Request::SetProjectFolder { project_id, path } => envelope(Command::SetProjectFolder(project::SetProjectFolder { project_id, path }), None),
        Request::LoadUsage | Request::LoadUsageHours(_) | Request::LoadSubscription(_) | Request::CheckUpdate(_) | Request::InstallUpdate | Request::InstallAppUpdate | Request::RestartServer | Request::Power(_) | Request::AppKeys(_) | Request::Plugins(_) | Request::LoadServerLoad | Request::SetLoadLimits(_) | Request::LoadBrowser | Request::SetBrowser(_) | Request::SetBrowserBackend(_) | Request::LoadArtifactSetting | Request::SetArtifactsEnabled(_) | Request::LoadUsageTotals { .. } | Request::LoadTraceSetting | Request::SetTraceEnabled(_) | Request::LoadTrace(_) | Request::LoadInstructionVersions(_) | Request::LoadAutoAllow | Request::SetAutoAllow(_) | Request::LoadManagementMode | Request::SetManagementMode(_) | Request::LoadMcp | Request::SetMcp(_) | Request::McpAdd(_) | Request::McpRemove(_) | Request::McpAuth(_) | Request::LoadMcpAlwaysAllow | Request::McpAlwaysAllowRemove(_) | Request::Backups(_) | Request::ExportProject(_) | Request::Remote(_) => return None,
        Request::BrowseDir { .. } | Request::CreateDir { .. } | Request::AddClaudeCode | Request::BoardMembers(_) | Request::Claude(_) | Request::Board(_) | Request::Vault(_) | Request::Guidelines(_) => return None,
        Request::SendConfirmed(command) => envelope(command, None),
    })
}

/// The connection ended; `Some(reason)` when reconnecting can't help
/// (this computer was removed on the server), and what to show before
/// reconnecting when it was more than a dropped connection (a timeout). The
/// third field: the server said this device's session is gone (protocol 5),
/// so its token is forgotten and the sign-in screen shows after the reconnect.
struct Disconnected(Option<String>, Option<String>, bool);

impl From<ClientError> for Disconnected {
    fn from(err: ClientError) -> Self {
        if signed_out(&err) {
            return Disconnected(None, None, true);
        }
        let shown = !err.is_final() && !matches!(err, ClientError::Disconnected);
        Disconnected(err.is_final().then(|| err.to_string()), shown.then(|| err.to_string()), false)
    }
}

/// The session goes on no more: the connection dropped, reconnecting can't help,
/// or the server no longer knows this device's session.
fn ends(e: &ClientError) -> bool {
    e.is_disconnect() || e.is_final() || signed_out(e)
}

/// The gate's refusals (protocol 5): no (valid) session, or a temporary password to change first.
pub fn signed_out(e: &ClientError) -> bool {
    use aurelius_domain::error::ErrorCode;
    matches!(e, ClientError::Protocol(p) if matches!(p.code, ErrorCode::Unauthenticated | ErrorCode::PasswordChangeRequired))
}

/// What a server that does not have a protocol 5 method yet tells the window.
pub const NOT_BUILT: &str = "This server can't do this yet. Update the server, then try again.";

/// An older server (`unknown_method`), or one with the protocol 5 surface whose
/// method is not built yet ("This server cannot … yet.").
pub fn not_built(e: &ClientError) -> bool {
    use aurelius_domain::error::ErrorCode;
    matches!(e, ClientError::Protocol(p) if p.code == ErrorCode::UnknownMethod || (p.code == ErrorCode::Validation && p.message.starts_with("This server cannot")))
}

/// A view's answer, or `None` after showing why it failed. Only a lost
/// connection ends the session: reconnecting would load the same view and
/// fail again (a reply too large to send), so the app would never settle.
fn shown<T>(result: Result<T, ClientError>, updates: &mpsc::UnboundedSender<Update>) -> Result<Option<T>, Disconnected> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e) if ends(&e) => Err(e.into()),
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
    outbox: &mut VecDeque<(aurelius_domain::UserId, CommandEnvelope)>,
    open_conversation: &mut Option<ConversationId>,
    open_memory: &mut Option<ProjectId>,
    open_ideas: &mut Option<Option<ProjectId>>,
    tokens: &Tokens,
    open_discussion: &mut Option<ConversationId>,
    channels_shown: &mut bool,
    pushes: &mut mpsc::UnboundedReceiver<aurelius_client::SessionPush>,
    mut held: VecDeque<Request>,
    me: aurelius_domain::UserId,
) -> Result<Ended, Disconnected> {
    // Snapshot first, then subscribe from its cursor.
    let (as_of, mut jobs_running) = refresh(client, updates).await?;
    refresh_board_members(client, updates).await?;
    claude_login(client, ClaudeCall::Status, updates).await?;
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
        Err(e) if ends(&e) => return Err(e.into()),
        Err(_) => {}
    }
    client.subscribe(as_of).await.map_err(Disconnected::from)?;

    keep_own(outbox, me, updates);
    while let Some((_, envelope)) = outbox.front().cloned() {
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
            push = pushes.recv() => match push {
                Some(aurelius_client::SessionPush::ClaudeLoginPrompt(prompt)) => {
                    let _ = updates.send(Update::Claude(ClaudeUpdate::Prompt(prompt.into())));
                }
                // Revoked, a new password, disabled, expired (MU3-04 AC2, MU3-06 AC3): the
                // server closes the connection next; the window shows the sign-in at once.
                Some(aurelius_client::SessionPush::SignedOut(reason)) => {
                    tokens.forget();
                    let _ = updates.send(Update::SignedOut(reason));
                    let _ = updates.send(Update::Who(aurelius_domain::user::WhoAmI { board_members_exist: true, user: None, must_change_password: false }));
                    return Ok(Ended::SignedOut);
                }
                None => return Err(client.end_reason().into()),
            },
            request = next_request(&mut held, requests) => {
                let Some(request) = request else { return Ok(Ended::Closed) };
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::DirFailed(ask, e.to_string())); }
                    }
                    continue;
                }
                if let Request::CreateDir { parent, name } = request {
                    match client.create_dir(parent, name).await {
                        Ok(listing) => { let _ = updates.send(Update::DirListing(listing)); }
                        Err(e) if ends(&e) => return Err(e.into()),
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
                    outbox.push_back((me, env.clone()));
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::McpAddFailed(crate::screens::mcp_add::scrub(&mcp_manage_error(e), &params))); }
                    }
                    continue;
                }
                if let Request::McpRemove(name) = request {
                    match client.mcp_remove(&name).await {
                        Ok(list) => { let _ = updates.send(Update::Mcp(Ok(list))); }
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(ClientError::Protocol(p)) if saving && p.code == aurelius_protocol::ErrorCode::Forbidden => { let _ = updates.send(Update::Error("You need the Approve right on this server to turn trace mode on or off.".into())); }
                        Err(e) if saving => { let _ = updates.send(Update::Error(e.to_string())); }
                        Err(_) => { let _ = updates.send(Update::TraceSetting(None)); }
                    }
                    continue;
                }
                if let Request::LoadTrace(run_id) = request {
                    match client.run_trace(run_id).await {
                        Ok(trace) => { let _ = updates.send(Update::Trace(run_id, Ok(trace))); }
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::UsageHours(day, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadPreviewMessages(conversation) = request {
                    match client.messages(conversation).await {
                        Ok(messages) => { let _ = updates.send(Update::PreviewMessages(messages)); }
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(_) => {}
                    }
                    continue;
                }
                if let Request::LoadSubagents(conversation) = request {
                    match client.subagents(conversation).await {
                        Ok(agents) => { let _ = updates.send(Update::Subagents(conversation, agents)); }
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(_) => {}
                    }
                    continue;
                }
                if let Request::LoadMarkdownFile(scope, path) = request {
                    match client.markdown_file(scope.clone(), &path).await {
                        Ok(file) => { let _ = updates.send(Update::MarkdownFile(scope, path, Ok(file))); }
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::MarkdownFile(scope, path, Err(MarkdownFailure::of(&e)))); }
                    }
                    continue;
                }
                if let Request::LoadArtifactContent { url, version } = request {
                    match client.artifact_content(url.clone(), version).await {
                        Ok(content) => { let _ = updates.send(Update::ArtifactContent(url, version, Ok(content))); }
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::File(id, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadUpload(id) = request {
                    match client.upload(&id).await {
                        Ok(file) => { let _ = updates.send(Update::File(id, Ok(file))); }
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::File(id, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadUploadThumb(id) = request {
                    match client.upload_thumbnail(&id, UPLOAD_THUMB_SIDE).await {
                        Ok(bytes) => { let _ = updates.send(Update::UploadThumb(id, Ok(bytes))); }
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::UploadThumb(id, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadScreenshot(id) = request {
                    match client.screenshot(&id).await {
                        Ok(bytes) => { let _ = updates.send(Update::Screenshot(id, Ok(bytes))); }
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::Screenshot(id, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadFullActivity(message_id, call_id) = request {
                    match client.activity(message_id, &call_id).await {
                        Ok(view) => { let _ = updates.send(Update::FullActivity(call_id, Ok(view))); }
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::FullActivity(call_id, Err(e.to_string()))); }
                    }
                    continue;
                }
                if let Request::LoadContext(run_id) = request {
                    match client.context(run_id).await {
                        Ok(view) => { let _ = updates.send(Update::Context(view)); }
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        Err(e) if ends(&e) => return Err(e.into()),
                        Err(e) => { let _ = updates.send(Update::Error(e.to_string())); }
                    }
                    // Show what the server really has (also puts back what failed).
                    let notifications = client.notifications().await.map_err(Disconnected::from)?;
                    let _ = updates.send(Update::Notifications(notifications));
                    continue;
                }
                if let Request::BoardMembers(call) = request {
                    if board_members(client, call, tokens, updates).await? {
                        return Ok(Ended::SignedOut);
                    }
                    continue;
                }
                if let Request::Claude(call) = request {
                    claude_login(client, call, updates).await?;
                    continue;
                }
                if let Request::Vault(call) = request {
                    vault(client, call, updates).await?;
                    continue;
                }
                if let Request::Guidelines(call) = request {
                    guidelines(client, call, updates).await?;
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
                        Err(e) if ends(&e) => return Err(e.into()),
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
                        outbox.push_back((me, env.clone()));
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
                            Err(e) if ends(&e) => return Err(e.into()),
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
                            Err(e) if ends(&e) => return Err(e.into()),
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
                            Err(e) if ends(&e) => return Err(e.into()),
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
                outbox.push_back((me, envelope.clone()));
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
                let mut claude = false;
                let mut vault = false;
                let mut guidelines = false;
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
                    } else if e.kind.area() == "vault" {
                        vault = true;
                    } else if e.kind.area() == "guideline" {
                        guidelines = true;
                    } else if is_artifact(&e) { artifact = true } else if is_chat(&e) { chat = true } else { other = true }
                    task |= e.kind.area() == "task";
                    users |= e.kind.area() == "user";
                    claude |= e.kind == aurelius_domain::EventKind::ClaudeLoginUpdated;
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
                if vault {
                    // The window reads again only what shows (MU4-01 AC2).
                    let _ = updates.send(Update::Vault(VaultUpdate::Changed));
                }
                if guidelines {
                    // The library, a link or a proposal changed (also on another device): the window reads again what shows.
                    let _ = updates.send(Update::Guidelines(GuidelineUpdate::Changed));
                }
                if users {
                    // Another window made or disabled a board member.
                    send_who(client, updates).await?;
                    refresh_board_members(client, updates).await?;
                }
                if claude {
                    // Connected, expired or disconnected (also on another device), or the company's background login.
                    claude_login(client, ClaudeCall::Status, updates).await?;
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

/// The next request: the reads held at the gate first, then the window's.
async fn next_request(held: &mut VecDeque<Request>, requests: &mut mpsc::UnboundedReceiver<Request>) -> Option<Request> {
    if let Some(request) = held.pop_front() {
        return Some(request);
    }
    requests.recv().await
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

async fn send_sessions(client: &Client, of: Option<aurelius_domain::UserId>, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    match client.sessions(of).await {
        Ok(list) => {
            let _ = updates.send(Update::Sessions(of, Ok(list)));
        }
        Err(e) if ends(&e) => return Err(e.into()),
        Err(e) => {
            let _ = updates.send(Update::Sessions(of, Err(account_failure(&e))));
        }
    }
    Ok(())
}

/// `user.*` and `session.*` while signed in. A password goes to the server and is
/// dropped with the call; a refusal goes back to the form that asked. True when
/// this device signed out (back to the gate).
async fn board_members(client: &Client, call: UserCall, tokens: &Tokens, updates: &mpsc::UnboundedSender<Update>) -> Result<bool, Disconnected> {
    // The form always hears back, also when the connection drops (Trace M3a m1):
    // else it stays on "Creating…" / "Saving…". No retry: after the reconnect the
    // list shows what the server did anyway.
    let refused = |err: ClientError| -> Result<bool, Disconnected> {
        let _ = updates.send(Update::BoardMemberFailed(member_call_failure(&err)));
        if ends(&err) {
            return Err(err.into());
        }
        Ok(false)
    };
    let failed = |err: ClientError| -> Result<bool, Disconnected> {
        let _ = updates.send(Update::Account(AccountUpdate::Failed(account_failure(&err))));
        if ends(&err) {
            return Err(err.into());
        }
        Ok(false)
    };
    match call {
        UserCall::List => refresh_board_members(client, updates).await.map(|()| false),
        // Another board member (MU1-22): this device stays signed in as it is.
        UserCall::Create { username, password } => {
            let created = client.user_create(&username, &password.0).await;
            drop(password);
            if let Err(err) = created {
                return refused(err);
            }
            let _ = updates.send(Update::BoardMemberCreated(username.trim().to_string()));
            refresh_board_members(client, updates).await.map(|()| false)
        }
        // Signed in already: a sign-in as someone else goes through Sign out first.
        UserCall::Login { password, .. } => {
            drop(password);
            let _ = updates.send(Update::BoardMemberFailed(MemberFailure::general("Sign out first, then sign in as someone else.")));
            Ok(false)
        }
        UserCall::Logout => {
            tokens.forget();
            match client.user_logout().await {
                Ok(_) => {}
                Err(err) if ends(&err) && !signed_out(&err) => return Err(err.into()),
                Err(_) => {}
            }
            let _ = updates.send(Update::Who(aurelius_domain::user::WhoAmI { board_members_exist: true, user: None, must_change_password: false }));
            Ok(true)
        }
        UserCall::Sessions(of) => send_sessions(client, of, updates).await.map(|()| false),
        UserCall::Revoke { session_id, of } => {
            match client.revoke_session(session_id).await {
                Ok(()) => {
                    let _ = updates.send(Update::Account(AccountUpdate::Revoked(1)));
                }
                Err(err) => {
                    failed(err)?;
                }
            }
            send_sessions(client, of, updates).await.map(|()| false)
        }
        UserCall::RevokeOthers => {
            let list = match client.sessions(None).await {
                Ok(list) => list,
                Err(err) => return failed(err),
            };
            let mut ended = 0;
            for s in list.sessions.iter().filter(|s| !s.current) {
                match client.revoke_session(s.id).await {
                    Ok(()) => ended += 1,
                    Err(err) => {
                        failed(err)?;
                        break;
                    }
                }
            }
            if ended > 0 {
                let _ = updates.send(Update::Account(AccountUpdate::Revoked(ended)));
            }
            send_sessions(client, None, updates).await.map(|()| false)
        }
        UserCall::SetPassword { user_id, old, new } => {
            let result = client.set_password(user_id, old.as_ref().map(|p| p.0.as_str()), &new.0).await;
            drop((old, new));
            match result {
                Ok(()) => {
                    let name = match user_id {
                        Some(id) => client.user_list().await.ok().and_then(|l| l.users.into_iter().find(|u| u.id == id)).map(|u| u.username),
                        None => None,
                    };
                    let temporary = user_id.map(|_| name.unwrap_or_else(|| "this board member".into()));
                    let _ = updates.send(Update::Account(AccountUpdate::PasswordSaved(temporary)));
                    // The own change ended the other devices' sessions: the list shows that (QA3 D-m2).
                    if user_id.is_none() {
                        return send_sessions(client, None, updates).await.map(|()| false);
                    }
                    Ok(false)
                }
                Err(err) => {
                    let failure = member_call_failure(&err);
                    let _ = updates.send(Update::Account(AccountUpdate::PasswordFailed(failure)));
                    if ends(&err) {
                        return Err(err.into());
                    }
                    Ok(false)
                }
            }
        }
        UserCall::Disable(user_id) => {
            match client.disable_user(user_id).await {
                Ok(user) => {
                    let _ = updates.send(Update::Account(AccountUpdate::Disabled(user)));
                }
                Err(err) => {
                    failed(err)?;
                }
            }
            refresh_board_members(client, updates).await.map(|()| false)
        }
    }
}

/// A code the server refused before it reached Claude (Sentinel C7).
pub const CODE_REFUSED: &str = "That is not a code Claude shows. Copy the whole code and paste it again.";

/// The own Claude login (MU3-09). The code goes to the server once and is
/// dropped with the call; no text of an answer repeats it.
async fn claude_login(client: &Client, call: ClaudeCall, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    let input = matches!(call, ClaudeCall::Input { .. });
    let failure = |err: &ClientError| -> String {
        use aurelius_domain::error::ErrorCode;
        match err {
            e if not_built(e) => NOT_BUILT.into(),
            // C5: a link of another connection, a used one or an ended one: no difference.
            ClientError::Protocol(p) if p.code == ErrorCode::NotFound => "This sign-in ended. Start again with Connect.".into(),
            // C7: refused before it reached Claude; the server never repeats the text.
            ClientError::Protocol(p) if input && p.code == ErrorCode::Validation => CODE_REFUSED.into(),
            ClientError::Protocol(p) => p.message.clone(),
            e => e.to_string(),
        }
    };
    let result = match call {
        ClaudeCall::Status => client.claude_login().await.map(ClaudeUpdate::Status),
        ClaudeCall::Link => client.claude_login_link().await.map(ClaudeUpdate::Linking),
        ClaudeCall::Input { link_id, code } => {
            let answer = client.claude_login_input(&link_id, &code.0).await;
            drop(code);
            answer.map(ClaudeUpdate::Status)
        }
        ClaudeCall::Cancel(link_id) => client.claude_login_cancel(&link_id).await.map(ClaudeUpdate::Status),
        ClaudeCall::Unlink => client.claude_login_unlink().await.map(ClaudeUpdate::Status),
    };
    match result {
        Ok(update) => {
            let _ = updates.send(Update::Claude(update));
        }
        Err(e) if ends(&e) => return Err(e.into()),
        Err(e) => {
            let _ = updates.send(Update::Claude(ClaudeUpdate::Failed(failure(&e))));
        }
    }
    Ok(())
}

/// One board call. A read that fails goes to the page; a write that fails
/// goes back to the board, also when the connection drops on the way (the
/// session then reconnects as usual). Writes are never sent twice.
async fn board(client: &Client, call: BoardCall, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    let lost = |e: &ClientError| ends(&e);
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
            let command = Command::PostTicketMessage(post);
            let held = confirmed(&command);
            let sent = held.is_none().then(|| command.clone());
            match client.execute(envelope(command, None)).await {
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
                // B10: the draft stays, and the window asks "Send anyway?".
                Err(e) if secret_names(&e).is_some() => {
                    let names = secret_names(&e).unwrap_or_default();
                    let message = sent.as_ref().and_then(|c| secret_refused(c, &names)).unwrap_or_else(|| NOT_SENT_SECRET.into());
                    let _ = updates.send(Update::Board(BoardUpdate::PostFailed { ticket, message }));
                    if let Some(command) = held {
                        let _ = updates.send(Update::LooksLikeSecret(SecretHold::new(names, command)));
                    }
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
            // A ticket's new files: as they are for the local service, uploaded for a remote one.
            // Sent anyway (B10), they went up already with the first try.
            let files = match &mut command {
                Command::CreateTicket(create) if !create.confirm_secret => Some(&mut create.files),
                Command::UpdateTicket(update) if !update.confirm_secret => update.add_files.as_mut(),
                _ => None,
            };
            if let Some(files) = files {
                let mut staged = Vec::new();
                for path in files.iter() {
                    match client.stage_file(std::path::Path::new(path)).await {
                        Ok(path) => staged.push(path),
                        Err(e) => {
                            let name = std::path::Path::new(path).file_name().map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
                            let message = if lost(&e) { BOARD_CONNECTION_DROPPED.to_string() } else { format!("Could not send the file {name}: {e}") };
                            let _ = updates.send(Update::Board(BoardUpdate::Failed { ticket, message }));
                            return if lost(&e) { Err(e.into()) } else { Ok(()) };
                        }
                    }
                }
                *files = staged;
            }
            // B10: a ticket's text the server may hold back, ready to go again with `confirm_secret`.
            let held = confirmed(&command);
            let sent = held.is_none().then(|| command.clone());
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
                // The form or the card waits, and the window asks "Send anyway?"; refused again, it says why (D-m5).
                Err(e) if secret_names(&e).is_some() => {
                    let names = secret_names(&e).unwrap_or_default();
                    let message = sent.as_ref().and_then(|c| secret_refused(c, &names)).unwrap_or_else(|| NOT_SENT_SECRET.into());
                    let _ = updates.send(Update::Board(BoardUpdate::Failed { ticket, message }));
                    if let Some(command) = held {
                        let _ = updates.send(Update::LooksLikeSecret(SecretHold::new(names, command).on_board(expected_version, done)));
                    }
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

/// A vault call's answer: the refusal in the user's words; a lost connection is kept in `lost`.
fn vault_said<T>(result: Result<T, ClientError>, lost: &mut Option<ClientError>) -> Result<T, String> {
    result.map_err(|e| {
        let text = vault_failure(&e);
        if ends(&e) {
            *lost = Some(e);
        }
        text
    })
}

/// The vault's calls (feature `vault`). Reveal and key export answers go to the
/// window as [`Revealed`] and are never logged or kept here (E19).
async fn vault(client: &Client, call: VaultCall, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    let mut lost = None;
    let answer = match call {
        VaultCall::List(scope) => VaultUpdate::Listed(scope, vault_said(client.vault_list(scope).await, &mut lost)),
        VaultCall::Reveal { item_id, purpose } => {
            let result = client.vault_reveal(item_id, purpose).await.map(|v| Revealed::new(v.value));
            VaultUpdate::Value { item_id, purpose, result: vault_said(result, &mut lost) }
        }
        VaultCall::Audit { ask, before } => {
            let result = client.vault_audit(ask.project_id, ask.all, ask.item_id, before, Some(crate::screens::vault::AUDIT_PAGE)).await;
            VaultUpdate::Audit { ask, before, result: vault_said(result, &mut lost) }
        }
        VaultCall::KeyExport { password } => {
            // E9: the own password again, moved into the protocol's params (redacted Debug); no copy stays here.
            let Password(password) = password;
            let result = client.vault_key_export(password).await.map(|k| KeyFile { key_id: k.key_id, key: Revealed::new(k.key) });
            VaultUpdate::Key(vault_said(result, &mut lost))
        }
        VaultCall::Create(params) => VaultUpdate::Saved(vault_said(client.vault_create(params).await, &mut lost)),
        VaultCall::Update(params) => VaultUpdate::Saved(vault_said(client.vault_update(params).await, &mut lost)),
        VaultCall::Archive(item_id) => VaultUpdate::Archived(item_id, vault_said(client.vault_archive(item_id).await, &mut lost)),
    };
    let _ = updates.send(Update::Vault(answer));
    match lost {
        Some(e) => Err(e.into()),
        None => Ok(()),
    }
}

/// A guideline read's answer, picked out of its body; a lost connection is kept in `lost`.
async fn guideline_read<T>(client: &Client, method: Method, pick: impl FnOnce(ResponseBody) -> Option<T>, lost: &mut Option<ClientError>) -> Result<T, String> {
    let result = match client.call(method).await {
        Ok(body) => pick(body).ok_or(ClientError::UnexpectedResponse),
        Err(e) => Err(e),
    };
    result.map_err(|e| {
        let text = GuidelineFailure::of(&e).message;
        if ends(&e) {
            *lost = Some(e);
        }
        text
    })
}

/// The domain command of a guideline write, with the version it expects; `Links` has one per change.
pub fn guideline_commands(write: &GuidelineWrite) -> Vec<(Command, Option<u64>)> {
    use aurelius_domain::guideline as g;
    match write.clone() {
        GuidelineWrite::Create { title, text, links } => vec![(Command::CreateGuideline(g::CreateGuideline { title, text, links }), None)],
        GuidelineWrite::Update { guideline_id, expected_version, title, text } => {
            vec![(Command::UpdateGuideline(g::UpdateGuideline { guideline_id, title, text }), Some(u64::from(expected_version)))]
        }
        GuidelineWrite::Archive(guideline_id) => vec![(Command::ArchiveGuideline(g::ArchiveGuideline { guideline_id }), None)],
        GuidelineWrite::Restore(guideline_id) => vec![(Command::RestoreGuideline(g::RestoreGuideline { guideline_id }), None)],
        GuidelineWrite::RestoreVersion { guideline_id, version } => vec![(Command::RestoreGuidelineVersion(g::RestoreGuidelineVersion { guideline_id, version }), None)],
        GuidelineWrite::Links(changes) => changes
            .into_iter()
            .map(|c| match c.link {
                true => (Command::LinkGuideline(g::LinkGuideline { guideline_id: c.guideline_id, target: c.target }), None),
                false => (Command::UnlinkGuideline(g::UnlinkGuideline { guideline_id: c.guideline_id, target: c.target }), None),
            })
            .collect(),
        GuidelineWrite::Decide { proposal_id, approve, title, text, links, note, expected_version } => {
            vec![(Command::DecideGuidelineProposal(g::DecideGuidelineProposal { proposal_id, approve, title, text, links, note }), expected_version.map(u64::from))]
        }
        GuidelineWrite::AddStarter(keys) => vec![(Command::AddStarterGuidelines(g::AddStarterGuidelines { keys }), None)],
    }
}

/// The guidelines' calls (feature `guidelines`).
async fn guidelines(client: &Client, call: GuidelineCall, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    let mut lost = None;
    let answer = match call {
        GuidelineCall::List { archived } => {
            let result = guideline_read(client, Method::GuidelineList { archived }, |b| match b { ResponseBody::Guidelines(s) => Some(s.guidelines), _ => None }, &mut lost).await;
            GuidelineUpdate::Listed { archived, result }
        }
        GuidelineCall::Get { guideline_id, version } => {
            let result = guideline_read(client, Method::GuidelineGet { guideline_id, version }, |b| match b { ResponseBody::Guideline(d) => Some(d), _ => None }, &mut lost).await;
            GuidelineUpdate::Detail { guideline_id, version, result }
        }
        GuidelineCall::Versions(guideline_id) => {
            let result = guideline_read(client, Method::GuidelineVersions { guideline_id }, |b| match b { ResponseBody::GuidelineVersions(v) => Some(v.versions), _ => None }, &mut lost).await;
            GuidelineUpdate::Versions(guideline_id, result)
        }
        GuidelineCall::Effective(key) => {
            let (employee_id, project_id) = key;
            let result = guideline_read(client, Method::GuidelineEffective { employee_id, project_id }, |b| match b { ResponseBody::EffectiveGuidelines(e) => Some(e), _ => None }, &mut lost).await;
            GuidelineUpdate::Effective(key, result)
        }
        GuidelineCall::Proposals => {
            let result = guideline_read(client, Method::GuidelineProposals { status: None }, |b| match b { ResponseBody::GuidelineProposals(p) => Some(p.proposals), _ => None }, &mut lost).await;
            GuidelineUpdate::Proposals(result)
        }
        GuidelineCall::Affected(guideline_id) => {
            let result = guideline_read(client, Method::GuidelineAffected { guideline_id }, |b| match b { ResponseBody::GuidelineAffected(a) => Some(a), _ => None }, &mut lost).await;
            GuidelineUpdate::Affected(guideline_id, result)
        }
        GuidelineCall::StarterSet => {
            let result = guideline_read(client, Method::GuidelineStarterSet, |b| match b { ResponseBody::StarterSet(s) => Some(s.templates), _ => None }, &mut lost).await;
            GuidelineUpdate::Starter(result)
        }
        GuidelineCall::Run(run_id) => {
            let result = guideline_read(client, Method::RunGuidelines { run_id }, |b| match b { ResponseBody::RunGuidelines(r) => Some(r.guidelines), _ => None }, &mut lost).await;
            GuidelineUpdate::Run(run_id, result)
        }
        GuidelineCall::Write(write) => {
            let (mut done, mut result) = (0, Ok(None));
            for (command, expected_version) in guideline_commands(&write) {
                match client.execute(envelope(command, expected_version)).await {
                    Ok(receipt) => {
                        done += 1;
                        result = Ok(Some(receipt.result));
                    }
                    Err(e) => {
                        result = Err(GuidelineFailure::of(&e));
                        if ends(&e) {
                            lost = Some(e);
                        }
                        break;
                    }
                }
            }
            GuidelineUpdate::Written { write, done, result }
        }
    };
    let _ = updates.send(Update::Guidelines(answer));
    match lost {
        Some(e) => Err(e.into()),
        None => Ok(()),
    }
}

/// The Team chat's channels (`channel.list`): none from a server without channels
/// (`unknown_method`, or any other refusal: the Team chat then is the one general chat).
async fn send_channels(client: &Client, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    let result = read(client, Method::ChannelList, |b| match b { ResponseBody::Channels(c) => Some(c), _ => None }).await;
    match result {
        Ok(snapshot) => {
            let _ = updates.send(Update::Channels(Some(snapshot.channels)));
        }
        Err(e) if ends(&e) => return Err(e.into()),
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
        Err(e) if ends(&e) => return Err(e.into()),
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
        Err(e) if ends(&e) => return Err(e.into()),
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
            Err(e) if ends(&e) => Err(e.into()),
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
        Err(e) if ends(&e) => return Err(e.into()),
        Err(e) => send(AppKeyUpdate::Failed(e.to_string())),
    }
    Ok(())
}

/// Every recorded artifact; nothing on servers without `artifact.list`, so the lists stay hidden.
// shortcut: the server lists at most 500 (all scopes at once, filtered per conversation/task here), fine until someone publishes hundreds.
async fn refresh_artifacts(client: &Client, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    match client.artifacts(None, None, None).await {
        Ok(artifacts) => { let _ = updates.send(Update::Artifacts(artifacts)); }
        Err(e) if ends(&e) => return Err(e.into()),
        Err(_) => {}
    }
    Ok(())
}

/// The recurring tasks.
async fn refresh_schedules(client: &Client, updates: &mpsc::UnboundedSender<Update>) -> Result<(), Disconnected> {
    match client.task_schedules(None).await {
        Ok(snapshot) => { let _ = updates.send(Update::Schedules(snapshot.schedules)); }
        Err(e) if ends(&e) => return Err(e.into()),
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
    // B10: a text the server may hold back, ready to go again with `confirm_secret`.
    let held = confirmed(&envelope.command);
    let sent = held.is_none().then(|| envelope.command.clone());
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
        Err(err) if secret_names(&err).is_some() => {
            let names = secret_names(&err).unwrap_or_default();
            // A description is no message: refused, its Save comes back.
            if let Some(project_id) = describing {
                let _ = updates.send(Update::DescriptionSaved(project_id, Err(secret_refusal(&names))));
                return Ok(());
            }
            let _ = updates.send(match held {
                Some(command) => Update::LooksLikeSecret(SecretHold::new(names, command)),
                None => Update::Error(sent.as_ref().and_then(|c| secret_refused(c, &names)).unwrap_or_else(|| secret_refusal(&names))),
            });
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

    /// This device's session tokens in memory, for the tests (the OS store is the user's).
    #[derive(Default)]
    pub(super) struct MemoryTokens(std::sync::Mutex<std::collections::HashMap<String, String>>);

    impl TokenStore for MemoryTokens {
        fn get(&self, server: &str) -> Option<String> {
            self.0.lock().unwrap().get(server).cloned()
        }
        fn set(&self, server: &str, token: &str) {
            self.0.lock().unwrap().insert(server.into(), token.into());
        }
        fn delete(&self, server: &str) {
            self.0.lock().unwrap().remove(server);
        }
    }

    pub(super) fn memory_tokens() -> (Tokens, std::sync::Arc<MemoryTokens>) {
        let store = std::sync::Arc::new(MemoryTokens::default());
        (Tokens::new("local".into(), store.clone()), store)
    }

    /// Sentinel S4/C6/C8: no password, code or sign-in link is ever printed by this window.
    #[test]
    fn a_password_a_code_or_a_link_is_never_printed() {
        let login = Request::BoardMembers(UserCall::Login { username: "alex".into(), password: Password::new("Tr0ub4dor&3-zebra".into()) });
        let create = Request::BoardMembers(UserCall::Create { username: "alex".into(), password: Password::new("Tr0ub4dor&3-zebra".into()) });
        let change = Request::BoardMembers(UserCall::SetPassword { user_id: None, old: Some(Password::new("Tr0ub4dor&3-zebra".into())), new: Password::new("Tr0ub4dor&3-zebra-2".into()) });
        let code = Request::Claude(ClaudeCall::Input { link_id: "l1".into(), code: LoginCode::new("CODE-SECRET#STATE".into()) });
        let prompt = Update::Claude(ClaudeUpdate::Prompt(LoginPrompt { link_id: "l1".into(), url: Some("https://claude.com/cai/oauth/authorize?state=STATE-SECRET".into()), asks_code: true, error: None }));
        for printed in [format!("{login:?}"), format!("{create:?}"), format!("{change:?}"), format!("{code:?}"), format!("{prompt:?}")] {
            assert!(!printed.contains("Tr0ub4dor") && !printed.contains("SECRET"), "{printed}");
        }
        let (tokens, _) = memory_tokens();
        assert!(!format!("{tokens:?}").contains("aus_"), "the store prints only its server");
    }

    /// An empty token (a server without sessions yet) is never kept.
    #[test]
    fn only_a_real_token_is_kept() {
        let (tokens, store) = memory_tokens();
        tokens.keep("");
        assert_eq!(store.get("local"), None);
        tokens.keep("aus_token");
        assert_eq!(tokens.get().as_deref(), Some("aus_token"));
        tokens.forget();
        assert_eq!(tokens.get(), None);
    }

    #[test]
    fn a_refusal_goes_to_its_field() {
        use aurelius_domain::error::{AppError, ErrorCode};
        let refusal = |code, text: &str| member_failure(&ClientError::Protocol(AppError::new(code, text)));
        assert_eq!(refusal(ErrorCode::UsernameTaken, "another board member already has this name"), MemberFailure { field: Some(MemberField::Username), message: "This username is taken.".into(), retry_after_ms: None });
        assert_eq!(refusal(ErrorCode::WeakPassword, "A password has at least 12 characters.").field, Some(MemberField::Password));
        assert_eq!(refusal(ErrorCode::InvalidUsername, "Use only letters, digits, '.', '_' and '-'.").field, Some(MemberField::Username));
        assert_eq!(refusal(ErrorCode::WrongPassword, "wrong").field, Some(MemberField::Current), "MU3-05 AC1: the current password");
        assert_eq!(refusal(ErrorCode::Internal, "storage is busy").field, None);
        assert!(needs_board_member(&ClientError::Protocol(AppError::new(ErrorCode::BoardMemberRequired, "sign in first"))));
        assert!(!needs_board_member(&ClientError::Disconnected));
    }

    /// MU3-02 AC3/AC4: one generic refusal; too many tries with the server's wait.
    #[test]
    fn a_refused_sign_in_never_says_which_was_wrong() {
        use aurelius_domain::error::{AppError, ErrorCode};
        let login = |e: AppError| login_failure(&ClientError::Protocol(e));
        assert_eq!(login(AppError::new(ErrorCode::WrongPassword, "Username or password is wrong.")), MemberFailure::general(SIGN_IN_WRONG));
        assert_eq!(login(AppError::new(ErrorCode::Unauthenticated, "no")).message, SIGN_IN_WRONG);
        let wait = login(AppError::too_many_attempts(29_400));
        assert_eq!((wait.message.as_str(), wait.retry_after_ms), ("Too many attempts. Try again in 0:30.", Some(29_400)));
        assert_eq!(login_failure(&ClientError::Disconnected).message, MEMBER_CONNECTION_DROPPED);
    }

    /// The gate's refusals end the session (sign in again); a missing method says "update the server".
    #[test]
    fn unauthenticated_and_not_built_are_told_apart() {
        use aurelius_domain::error::{AppError, ErrorCode};
        let e = |code, text: &str| ClientError::Protocol(AppError::new(code, text));
        assert!(signed_out(&e(ErrorCode::Unauthenticated, "Sign in.")) && ends(&e(ErrorCode::Unauthenticated, "Sign in.")));
        assert!(signed_out(&e(ErrorCode::PasswordChangeRequired, "Change it.")));
        let d = Disconnected::from(e(ErrorCode::Unauthenticated, "Sign in."));
        assert!(d.2 && d.0.is_none() && d.1.is_none(), "no notice, no unpairing: the sign-in screen");
        assert!(not_built(&e(ErrorCode::UnknownMethod, "This server does not know the method session.list.")));
        assert!(not_built(&e(ErrorCode::Validation, "This server cannot do this yet.")));
        assert!(not_built(&e(ErrorCode::Validation, "This server cannot run tickets on board members' own Claude logins yet.")));
        assert!(!not_built(&e(ErrorCode::Validation, "The title is empty.")));
        assert_eq!(account_failure(&e(ErrorCode::LastBoardMember, "x")), LAST_BOARD_MEMBER);
        assert_eq!(board_write_failure(&e(ErrorCode::UnknownMethod, "x")), NOT_BUILT);
    }

    mod against_a_daemon {
        use super::super::*;
        use aurelius_domain::user::WhoAmI;

        const ALEX: &str = "Tr0ub4dor&3-zebra-alex";
        const ROBIN: &str = "correct-horse-battery-robin";

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

        fn login(username: &str, password: &str) -> UserCall {
            UserCall::Login { username: username.into(), password: Password::new(password.into()) }
        }

        /// MU3-02, MU3-03, MU3-04, MU3-06 and MU3-09 through this app's backend on protocol 5:
        /// the onboarding signs in at once, a second board member, a new device that shows
        /// only the sign-in (reads wait, writes are refused), the one generic refusal, the
        /// sign-in with its held reads, the own sessions and the Claude login (or "update the
        /// server" while Forge builds them), Run on my login, Sign out, and a kept session
        /// that the server no longer knows (signed out, token forgotten).
        #[tokio::test(flavor = "multi_thread")]
        async fn signing_in_from_the_onboarding_to_signing_out() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let mut server = daemon(&root).await;
            let target = || Some(Target::Local { root: root.clone(), spawn: false });

            // A server without board members: only the onboarding (MU1-20 AC1, MU3-02 AC5).
            let (tokens, first_store) = super::memory_tokens();
            let mut first = start(target(), tokens);
            let empty = who(&mut first).await;
            assert!(!empty.board_members_exist && empty.user.is_none(), "{empty:?}");

            // Refused, nothing made (MU1-20 AC3): a short password, by its field.
            ask(&first, create("alex", "short"));
            assert_eq!(failure(&mut first).await.field, Some(MemberField::Password));

            // Made, and this device is signed in as alex at once.
            ask(&first, create("alex", ALEX));
            let onboarded = next(&mut first, |u| match u {
                Update::Who(w) if w.user.is_some() => Some(w),
                _ => None,
            })
            .await;
            let alex = onboarded.user.clone().unwrap();
            assert_eq!(alex.username, "alex");
            let kept = first_store.get("local").expect("the session token is kept (MU3-03 AC1)");
            assert!(!kept.contains(ALEX), "a token, never a password");

            // Signed in, the session loads: the own Claude login among it (MU3-09 AC2).
            let claude = next(&mut first, |u| match u {
                Update::Claude(ClaudeUpdate::Status(v)) => Some(Ok(v)),
                Update::Claude(ClaudeUpdate::Failed(why)) => Some(Err(why)),
                _ => None,
            })
            .await;
            let claude = claude.expect("the status is a session read");
            assert_eq!(claude.user_id, alex.id, "the own login only (B12)");
            assert_eq!(claude.status, aurelius_domain::user::ClaudeLoginStatus::Missing);

            // A second board member (MU1-22 AC1); this device stays alex.
            ask(&first, create("robin", ROBIN));
            let made = next(&mut first, |u| match u {
                Update::BoardMemberCreated(name) => Some(name),
                _ => None,
            })
            .await;
            assert_eq!(made, "robin");

            // Alex on a second device: the own sessions list both (MU3-04 AC1); revoking the other
            // sends that device back to the sign-in at once, its token forgotten (AC2).
            let (tokens, other_store) = super::memory_tokens();
            let mut other = start(target(), tokens);
            assert!(who(&mut other).await.user.is_none());
            ask(&other, login("alex", ALEX));
            next(&mut other, |u| match u {
                Update::Who(w) if w.user.is_some() => Some(()),
                _ => None,
            })
            .await;
            assert!(other_store.get("local").is_some());
            ask(&first, UserCall::Sessions(None));
            let list = next(&mut first, |u| match u {
                Update::Sessions(None, r) => Some(r),
                _ => None,
            })
            .await
            .expect("the own sessions");
            assert_eq!(list.sessions.len(), 2, "{list:?}");
            assert!(list.sessions.iter().all(|s| s.device_name.starts_with("Desktop · ")), "{list:?}");
            let theirs = list.sessions.iter().find(|s| !s.current).expect("the other device").id;
            ask(&first, UserCall::Revoke { session_id: theirs, of: None });
            assert_eq!(next(&mut first, |u| match u {
                Update::Account(AccountUpdate::Revoked(n)) => Some(n),
                _ => None,
            })
            .await, 1);
            let reason = next(&mut other, |u| match u {
                Update::SignedOut(reason) => Some(reason),
                _ => None,
            })
            .await;
            assert_eq!(reason, aurelius_domain::user::SignOutReason::Revoked);
            assert_eq!(other_store.get("local"), None);
            let gone = next(&mut other, |u| match u {
                Update::Who(w) => Some(w),
                _ => None,
            })
            .await;
            assert!(gone.user.is_none() && gated(&gone), "{gone:?}");
            let left = next(&mut first, |u| match u {
                Update::Sessions(None, r) => Some(r),
                _ => None,
            })
            .await
            .expect("the own sessions");
            assert_eq!(left.sessions.len(), 1, "{left:?}");

            // A new device: board members exist, nobody is signed in: the sign-in only (MU3-02 AC1).
            let (tokens, device_store) = super::memory_tokens();
            let mut device = start(target(), tokens);
            let fresh = who(&mut device).await;
            assert!(fresh.board_members_exist && fresh.user.is_none(), "{fresh:?}");
            assert!(gated(&fresh));
            // Before a session a read waits for the sign-in, a write is refused (MU3-06).
            device.requests.send(Request::Board(BoardCall::Load(None))).unwrap();
            let start_ticket = Command::StartTicket(aurelius_domain::ticket::StartTicket { ticket_id: aurelius_domain::TicketId::new(), employee_id: None });
            device.requests.send(Request::Board(BoardCall::Write { command: start_ticket, expected_version: None, done: None })).unwrap();
            let refused = next(&mut device, |u| match u {
                Update::Board(BoardUpdate::Loaded(..)) => panic!("a read before the sign-in"),
                Update::Board(BoardUpdate::Failed { message, .. }) => Some(message),
                _ => None,
            })
            .await;
            assert_eq!(refused, BOARD_MEMBER_REQUIRED);

            // One generic refusal (MU3-02 AC3), then signed in (AC2); the held read goes.
            ask(&device, login("robin", "not her password"));
            assert_eq!(failure(&mut device).await, MemberFailure::general(SIGN_IN_WRONG));
            ask(&device, login("nobody", ROBIN));
            assert_eq!(failure(&mut device).await.message, SIGN_IN_WRONG, "an unknown name says the same");
            ask(&device, login("robin", ROBIN));
            let robin = next(&mut device, |u| match u {
                Update::Who(w) if w.user.is_some() => w.user,
                _ => None,
            })
            .await;
            assert_eq!(robin.username, "robin");
            next(&mut device, |u| matches!(u, Update::Board(BoardUpdate::Loaded(None, Ok(_)))).then_some(())).await;

            // Run on my login (B4, MU3-12, F2): an unknown ticket is refused with the server's reason.
            device.requests.send(Request::Board(BoardCall::Write { command: Command::UseMyLogin(aurelius_domain::ticket::UseMyLogin { ticket_id: aurelius_domain::TicketId::new() }), expected_version: None, done: None })).unwrap();
            let taken = next(&mut device, |u| match u {
                Update::Board(BoardUpdate::Failed { message, .. }) => Some(message),
                Update::Board(BoardUpdate::Done { .. }) => Some(String::new()),
                _ => None,
            })
            .await;
            assert!(taken != NOT_BUILT && !taken.is_empty(), "an unknown ticket is refused: {taken}");

            // Connect (MU3-09, F2): without a Claude Code connection, the daemon's own words.
            device.requests.send(Request::Claude(ClaudeCall::Link)).unwrap();
            let link = next(&mut device, |u| match u {
                Update::Claude(ClaudeUpdate::Linking(id)) => Some(Ok(id)),
                Update::Claude(ClaudeUpdate::Failed(why)) => Some(Err(why)),
                _ => None,
            })
            .await;
            assert_eq!(link, Err("There is no Claude Code connection on this server. Add one in Settings → Connections first.".to_string()));

            // Sign out on this device (MU3-04 AC3): the sign-in again, the token forgotten.
            ask(&device, UserCall::Logout);
            let out = next(&mut device, |u| match u {
                Update::Who(w) if w.user.is_none() => Some(w),
                _ => None,
            })
            .await;
            assert!(out.board_members_exist && gated(&out));
            assert_eq!(device_store.get("local"), None);

            // A kept session the server does not know: signed out, the token forgotten (MU3-03 AC3).
            stop(&root, server).await;
            server = daemon(&root).await;
            let (tokens, stale_store) = super::memory_tokens();
            stale_store.set("local", "aus_not-a-session");
            let mut stale = start(target(), tokens);
            let reason = next(&mut stale, |u| match u {
                Update::SignedOut(reason) => Some(reason),
                _ => None,
            })
            .await;
            assert_eq!(reason, aurelius_domain::user::SignOutReason::Expired);
            let back = who(&mut stale).await;
            assert!(back.user.is_none(), "{back:?}");
            assert_eq!(stale_store.get("local"), None);

            // A device that kept its session is back after the restart without a password (MU3-03 AC2).
            next(&mut first, |u| matches!(u, Update::Connection(Connection::Connecting)).then_some(())).await;
            let back = who(&mut first).await;
            assert_eq!(back.user.map(|u| u.id), Some(alex.id));
            assert_eq!(first_store.get("local"), Some(kept));

            drop((first, other, device, stale));
            stop(&root, server).await;
        }

        /// Alex onboards and adds Robin; the window is signed in as Alex once it loaded.
        async fn alex_and_robin(root: &std::path::Path) -> (Handle, std::sync::Arc<super::MemoryTokens>, aurelius_domain::UserId) {
            let (tokens, store) = super::memory_tokens();
            let mut first = start(Some(Target::Local { root: root.to_path_buf(), spawn: false }), tokens);
            assert!(who(&mut first).await.user.is_none());
            ask(&first, create("alex", ALEX));
            next(&mut first, |u| matches!(u, Update::Claude(ClaudeUpdate::Status(_))).then_some(())).await;
            ask(&first, create("robin", ROBIN));
            let robin = next(&mut first, |u| match u {
                Update::BoardMembers(list) => list.into_iter().find(|m| m.username == "robin").map(|m| m.id),
                _ => None,
            })
            .await;
            (first, store, robin)
        }

        fn new_project(name: &str) -> Request {
            Request::CreateProject { name: name.into(), description: None, folder: None }
        }

        /// R3 M2 (MU3-08): a write made offline by Alex, whose session then is no longer
        /// known (a token the server refuses), never goes as Robin who signs in next on
        /// the same window; a write at the gate is refused, not kept for after the sign-in.
        #[tokio::test(flavor = "multi_thread")]
        async fn writes_from_before_a_sign_out_never_go_as_the_next_board_member() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let server = daemon(&root).await;
            let (mut first, store, _) = alex_and_robin(&root).await;

            // Offline: Alex's write waits in the outbox.
            stop(&root, server).await;
            next(&mut first, |u| matches!(u, Update::Connection(Connection::Offline(_))).then_some(())).await;
            first.requests.send(new_project("From Alex")).unwrap();
            // Meanwhile the session ended (here: a token the server never knew).
            store.set("local", "aus_not-a-session");
            let server = daemon(&root).await;
            let reason = next(&mut first, |u| match u {
                Update::SignedOut(reason) => Some(reason),
                Update::Created(_) => panic!("Alex's write went without a session"),
                _ => None,
            })
            .await;
            assert_eq!(reason, aurelius_domain::user::SignOutReason::Expired);
            assert!(who(&mut first).await.user.is_none());

            // At the gate a write is refused at once (only reads wait for the sign-in).
            first.requests.send(new_project("Also from Alex")).unwrap();
            let refused = next(&mut first, |u| match u {
                Update::Error(why) => Some(why),
                Update::Created(_) => panic!("a write at the gate went"),
                _ => None,
            })
            .await;
            assert_eq!(refused, BOARD_MEMBER_REQUIRED);

            // Robin signs in on the same window: Alex's write is dropped, and said so.
            ask(&first, login("robin", ROBIN));
            let dropped = next(&mut first, |u| match u {
                Update::Error(why) if why == NOT_SENT_SIGNED_OUT => Some(why),
                Update::Created(_) => panic!("Alex's write went as Robin"),
                _ => None,
            })
            .await;
            assert_eq!(dropped, NOT_SENT_SIGNED_OUT);

            // Robin's own write goes; the server has only that project.
            first.requests.send(new_project("From Robin")).unwrap();
            let made = next(&mut first, |u| match u {
                Update::Created(id) => Some(id),
                _ => None,
            })
            .await;
            let projects = next(&mut first, |u| match u {
                Update::Projects(list) if list.iter().any(|p| p.id == made) => Some(list),
                _ => None,
            })
            .await;
            let names: Vec<_> = projects.iter().map(|p| p.name.as_str()).collect();
            assert!(!names.iter().any(|n| n.contains("Alex")), "{names:?}");

            drop(first);
            stop(&root, server).await;
        }

        /// QA3 D2 (MU3-05, B10): signing in with a temporary password goes on to "Choose a
        /// new password" without a restart, then into the session. QA3 D-m2: after the own
        /// password's change the sessions list is read again (the other devices are gone).
        #[tokio::test(flavor = "multi_thread")]
        async fn a_temporary_password_signs_in_to_its_change_and_the_sessions_follow() {
            const TEMPORARY: &str = "temporary-horse-staple-41";
            const ROBIN_NEW: &str = "robin-chose-this-one-42";
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let server = daemon(&root).await;
            let target = || Some(Target::Local { root: root.clone(), spawn: false });
            let (mut first, _, robin) = alex_and_robin(&root).await;
            ask(&first, UserCall::SetPassword { user_id: Some(robin), old: None, new: Password::new(TEMPORARY.into()) });
            next(&mut first, |u| matches!(u, Update::Account(AccountUpdate::PasswordSaved(Some(_)))).then_some(())).await;

            let (tokens, _) = super::memory_tokens();
            let mut device = start(target(), tokens);
            assert!(who(&mut device).await.user.is_none());
            ask(&device, login("robin", TEMPORARY));
            let must = next(&mut device, |u| match u {
                Update::Who(w) if w.user.is_some() => Some(w),
                Update::BoardMemberFailed(f) => panic!("refused: {f:?}"),
                _ => None,
            })
            .await;
            assert!(must.must_change_password && gated(&must), "{must:?}");
            ask(&device, UserCall::SetPassword { user_id: None, old: Some(Password::new(TEMPORARY.into())), new: Password::new(ROBIN_NEW.into()) });
            let changed = who(&mut device).await;
            assert!(!changed.must_change_password && !gated(&changed), "{changed:?}");
            next(&mut device, |u| matches!(u, Update::Claude(ClaudeUpdate::Status(_))).then_some(())).await;

            // Robin on a second device, then a password change on the first: one session left in the list.
            let (tokens, _) = super::memory_tokens();
            let mut second = start(target(), tokens);
            ask(&second, login("robin", ROBIN_NEW));
            next(&mut second, |u| matches!(u, Update::Who(w) if w.user.is_some()).then_some(())).await;
            ask(&device, UserCall::SetPassword { user_id: None, old: Some(Password::new(ROBIN_NEW.into())), new: Password::new(ROBIN.into()) });
            next(&mut device, |u| matches!(u, Update::Account(AccountUpdate::PasswordSaved(None))).then_some(())).await;
            let list = next(&mut device, |u| match u {
                Update::Sessions(None, r) => Some(r),
                _ => None,
            })
            .await
            .expect("the own sessions");
            assert_eq!(list.sessions.len(), 1, "{list:?}");
            assert!(list.sessions[0].current);

            drop((first, device, second));
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
                confirm_secret: false,
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
            let mut app = start(Some(Target::Local { root: root.clone(), spawn: false }), super::memory_tokens().0);
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

            // A screenshot pasted while editing it: added beside the file it had (ticket #3).
            let shot = root.join("screenshot.png");
            std::fs::write(&shot, b"\x89PNG").unwrap();
            let update = ticket::UpdateTicket { add_files: Some(vec![shot.display().to_string()]), ..crate::screens::board::empty_update(attached[0].id) };
            done(&mut app, Command::UpdateTicket(update), Some(attached[0].version)).await;
            let attached = tickets(&load(&mut app, Some(with_folder)).await, ColumnKind::Backlog);
            assert_eq!(attached[0].attachments.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["spec.md", "screenshot.png"]);

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
            let mut app = start(target(), super::memory_tokens().0);
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
            let post = Command::PostTicketMessage(ticket::PostTicketMessage { confirm_secret: false, ticket_id: ticket.id, text: "@marie can you look?".into(), images: vec![], files: vec![] });
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
            let mut other = start(target(), super::memory_tokens().0);
            other.requests.send(Request::BoardMembers(UserCall::Login { username: "marie".into(), password: Password::new(MARIE.into()) })).unwrap();
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

        /// The stand-in `claude` of the daemon's tests (F2 3ba4f28), in a folder of its own.
        #[cfg(unix)]
        fn fake_claude(root: &std::path::Path) -> std::path::PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let bin = root.join("fake-claude-bin");
            std::fs::create_dir_all(&bin).unwrap();
            let path = bin.join("claude");
            std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/daemon/tests/fixtures/fake-claude"), &path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        /// MU3-09, MU3-12 and ui-ontwerp §4–§5 through this app's backend against F2:
        /// a started ticket waits for its creator's missing Claude login (`needs: login`,
        /// "Waiting for your Claude login" for the creator, "alex's" for nobody else here),
        /// a code Claude refuses comes back in the daemon's own words, a good one connects
        /// (e-mail and plan), the ticket goes on, and `/usage` is the own login's only.
        #[cfg(unix)]
        #[tokio::test(flavor = "multi_thread")]
        async fn the_own_claude_login_from_a_waiting_ticket_to_its_usage() {
            use aurelius_domain::connection::{ConfigScope, Purpose, RunDefaults};
            use aurelius_domain::employee::Setting;
            use aurelius_domain::user::ClaudeLoginStatus;
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let server = daemon(&root).await;
            let mut app = start(Some(Target::Local { root: root.clone(), spawn: false }), super::memory_tokens().0);
            app.requests.send(Request::BoardMembers(UserCall::Create { username: "alex".into(), password: Password::new("Tr0ub4dor&3-zebra-alex".into()) })).unwrap();
            let alex = next(&mut app, |u| match u {
                Update::Who(w) => w.user,
                _ => None,
            })
            .await;
            let missing = next(&mut app, |u| match u {
                Update::Claude(ClaudeUpdate::Status(v)) => Some(v),
                _ => None,
            })
            .await;
            assert_eq!((missing.user_id, missing.status), (alex.id, ClaudeLoginStatus::Missing));
            assert!(crate::screens::account::needs_connect(Some(&missing), false), "the Board's banner");

            // A Claude Code connection on the stand-in, the default for employee runs.
            let program = fake_claude(&root);
            app.requests.send(Request::AddConnection { provider: aurelius_domain::connection::ProviderKind::ClaudeCode, name: "Claude Code".into(), runtime_path: Some(program.display().to_string()) }).unwrap();
            let connection = next(&mut app, |u| match u {
                Update::Connections(c) => c.connections.iter().find(|c| c.provider == "claude_code" && c.status == "ready").map(|c| c.id),
                _ => None,
            })
            .await;
            let settings = RunDefaults { connection: Setting::Value(connection), model: Setting::Value("sonnet".into()), persona: Setting::Inherit, effort: Setting::Inherit };
            app.requests.send(Request::SetDefaults { scope: ConfigScope::Global, purpose: Purpose::Employee, settings }).unwrap();

            // Started without a login (P-36): it waits, Needs you for its creator only (§5).
            app.requests.send(Request::CreateProject { name: "Atlas".into(), description: None, folder: None }).unwrap();
            let project = next(&mut app, |u| match u {
                Update::Created(p) => Some(p),
                _ => None,
            })
            .await;
            app.requests.send(Request::CreateEmployee { name: "Builder".into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None }).unwrap();
            let builder = next(&mut app, |u| match u {
                Update::EmployeeCreated(e) => Some(e),
                _ => None,
            })
            .await;
            let Command::CreateTicket(mut create) = new_ticket(project, "Login per board member") else { unreachable!() };
            create.owner_employee_id = Some(builder);
            done(&mut app, Command::CreateTicket(create), None).await;
            let id = tickets(&load(&mut app, Some(project)).await, ColumnKind::Backlog)[0].id;
            done(&mut app, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: None }), None).await;
            let mut waiting = None;
            for _ in 0..100 {
                let view = load(&mut app, Some(project)).await;
                waiting = view.columns.iter().flat_map(|c| c.tickets.iter()).find(|t| t.id == id && t.needs == Some(ticket::TicketNeed::Login)).cloned();
                if waiting.is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let waiting = waiting.expect("the ticket waits for alex's login (MU3-12 AC1)");
            assert_eq!(waiting.login_user, alex.id);
            assert_eq!(crate::screens::board::login_wait(&waiting, Some(alex.id), false).as_deref(), Some("◆ Waiting for your Claude login"));
            assert_eq!(crate::screens::board::login_wait(&waiting, Some(aurelius_domain::UserId::new()), false).as_deref(), Some("Waiting for alex's Claude login"));
            assert!(crate::screens::board::take_over_offered(&waiting, Some(aurelius_domain::UserId::new())), "another board member may run it on theirs");
            assert!(!crate::screens::board::take_over_offered(&waiting, Some(alex.id)));

            // Connect: Claude's link, a refused code in the daemon's words, then a good one.
            let prompt = |app: &mut Handle| {
                app.requests.send(Request::Claude(ClaudeCall::Link)).unwrap();
            };
            prompt(&mut app);
            let first = next(&mut app, |u| match u {
                Update::Claude(ClaudeUpdate::Prompt(p)) if p.asks_code => Some(p),
                Update::Claude(ClaudeUpdate::Failed(why)) => panic!("no link: {why}"),
                _ => None,
            })
            .await;
            assert!(first.url.as_deref().is_some_and(|u| u.starts_with("https://claude.com/cai/oauth/authorize?")), "{first:?}");
            app.requests.send(Request::Claude(ClaudeCall::Input { link_id: first.link_id.clone(), code: LoginCode::new("wrong#FAKESTATE".into()) })).unwrap();
            let refused = next(&mut app, |u| match u {
                Update::Claude(ClaudeUpdate::Prompt(p)) if p.error.is_some() => p.error,
                Update::Claude(ClaudeUpdate::Failed(why)) => Some(why),
                _ => None,
            })
            .await;
            assert_eq!(refused, "The code was not accepted. Start again.", "the daemon's own words");
            let mut link = crate::screens::account::Link { id: Some(first.link_id), failure: Some(refused), started_ms: 1, ..Default::default() };
            assert!(matches!(crate::screens::account::connect_step(&link), crate::screens::account::ConnectStep::Failed(_)), "Try again");
            assert!(!crate::screens::account::command_fallback(&link, i64::MAX));
            link.failure = None;
            prompt(&mut app);
            let second = next(&mut app, |u| match u {
                Update::Claude(ClaudeUpdate::Prompt(p)) if p.asks_code => Some(p),
                _ => None,
            })
            .await;
            app.requests.send(Request::Claude(ClaudeCall::Input { link_id: second.link_id, code: LoginCode::new("good-code#FAKESTATE".into()) })).unwrap();
            let linked = next(&mut app, |u| match u {
                Update::Claude(ClaudeUpdate::Status(v)) if v.status == ClaudeLoginStatus::Linked => Some(v),
                _ => None,
            })
            .await;
            assert_eq!((linked.email.as_deref(), linked.plan.as_deref()), (Some("test@example.com"), Some("max")));
            assert!(!crate::screens::account::needs_connect(Some(&linked), false), "the banner goes");

            // The ticket goes on by itself (B11): no longer waiting for a login.
            let mut resumed = false;
            for _ in 0..100 {
                let view = load(&mut app, Some(project)).await;
                if view.columns.iter().flat_map(|c| c.tickets.iter()).any(|t| t.id == id && t.needs.is_none()) {
                    resumed = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            assert!(resumed, "the ticket resumes once alex connects");

            // /usage: the own login's, with its limits (B12, P-37).
            // (The connection is verified again after the defaults changed: until then it is not "ready".)
            let mut usage = Default::default();
            for _ in 0..50 {
                app.requests.send(Request::LoadSubscription(true)).unwrap();
                usage = next(&mut app, |u| match u {
                    Update::Subscription(s) if s.fetched_at_ms > 0 => Some(s),
                    _ => None,
                })
                .await;
                if usage.error.is_none() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            assert_eq!(usage.login_user, Some(alex.id));
            let shown = crate::screens::account::usage_of(Some(&usage), alex.id);
            assert!(matches!(&shown, crate::screens::account::Usage::Limits(l) if !l.is_empty()), "{shown:?} from {:?}", usage.raw);
            assert_eq!(crate::screens::account::usage_of(Some(&usage), aurelius_domain::UserId::new()), crate::screens::account::Usage::Unknown, "never another's");

            drop(app);
            stop_daemon(&root, server).await;
        }

        /// The smoke test of AUR-MU-03-D2 against a running test daemon of the real binary
        /// (`crates/daemon/tests/fixtures/test-daemon.sh <folder> <port>`): set
        /// `AURELIUS_D2_SMOKE=<folder>` (a fresh one) to run it; without, it does nothing.
        /// Alex and Robin on their own devices, each on their own login: Robin's ticket
        /// waits for Robin's login (Needs you for Robin only), Alex runs it on Alex's own
        /// (Run on my login), each sees only their own /usage, and Alex's limit pauses
        /// Alex's ticket with "Paused: …" in its discussion.
        #[cfg(unix)]
        #[tokio::test(flavor = "multi_thread")]
        async fn smoke_against_a_test_daemon() {
            use aurelius_domain::connection::{ConfigScope, Purpose, RunDefaults};
            use aurelius_domain::employee::Setting;
            use aurelius_domain::user::ClaudeLoginStatus;
            use crate::screens::{account, board, discussion};
            let Some(folder) = std::env::var_os("AURELIUS_D2_SMOKE").map(std::path::PathBuf::from) else { return };
            let root = folder.join("data");
            let program = folder.join("bin").join("claude");
            let target = || Some(Target::Local { root: root.clone(), spawn: false });
            let say = |step: &str| eprintln!("smoke: {step}");

            // Onboarding: Alex, then Robin; each on a device of their own.
            let mut alex_app = start(target(), super::memory_tokens().0);
            alex_app.requests.send(Request::BoardMembers(UserCall::Create { username: "alex".into(), password: Password::new("Tr0ub4dor&3-zebra-alex".into()) })).unwrap();
            let alex = next(&mut alex_app, |u| match u {
                Update::Who(w) => w.user,
                _ => None,
            })
            .await;
            alex_app.requests.send(Request::BoardMembers(UserCall::Create { username: "robin".into(), password: Password::new("correct-horse-battery-robin".into()) })).unwrap();
            next(&mut alex_app, |u| matches!(u, Update::BoardMemberCreated(_)).then_some(())).await;
            let mut robin_app = start(target(), super::memory_tokens().0);
            let gate = next(&mut robin_app, |u| match u {
                Update::Who(w) => Some(w),
                _ => None,
            })
            .await;
            assert!(gate.board_members_exist && gate.user.is_none(), "the sign-in from hello/whoami, not user.list: {gate:?}");
            robin_app.requests.send(Request::BoardMembers(UserCall::Login { username: "robin".into(), password: Password::new("correct-horse-battery-robin".into()) })).unwrap();
            let robin = next(&mut robin_app, |u| match u {
                Update::Who(w) => w.user,
                _ => None,
            })
            .await;
            say("alex and robin signed in on their own devices");

            // The stand-in claude as the Claude Code connection.
            alex_app.requests.send(Request::AddConnection { provider: aurelius_domain::connection::ProviderKind::ClaudeCode, name: "Claude Code".into(), runtime_path: Some(program.display().to_string()) }).unwrap();
            let connection = next(&mut alex_app, |u| match u {
                Update::Connections(c) => c.connections.iter().find(|c| c.provider == "claude_code" && c.status == "ready").map(|c| c.id),
                _ => None,
            })
            .await;
            let settings = RunDefaults { connection: Setting::Value(connection), model: Setting::Value("sonnet".into()), persona: Setting::Inherit, effort: Setting::Inherit };
            alex_app.requests.send(Request::SetDefaults { scope: ConfigScope::Global, purpose: Purpose::Employee, settings }).unwrap();

            // Alex connects (the code starts with "good").
            alex_app.requests.send(Request::Claude(ClaudeCall::Link)).unwrap();
            let link = next(&mut alex_app, |u| match u {
                Update::Claude(ClaudeUpdate::Prompt(p)) if p.asks_code => Some(p),
                Update::Claude(ClaudeUpdate::Failed(why)) => panic!("no link: {why}"),
                _ => None,
            })
            .await;
            alex_app.requests.send(Request::Claude(ClaudeCall::Input { link_id: link.link_id, code: LoginCode::new("good-smoke#FAKESTATE".into()) })).unwrap();
            let linked = next(&mut alex_app, |u| match u {
                Update::Claude(ClaudeUpdate::Status(v)) if v.status == ClaudeLoginStatus::Linked => Some(v),
                _ => None,
            })
            .await;
            assert_eq!(account::status_line(&linked, &account::Usage::Unknown).text, "Connected  test@example.com · Max");
            say("alex connected: Connected  test@example.com · Max");

            // A project, an employee; Robin's ticket waits for Robin's login.
            alex_app.requests.send(Request::CreateProject { name: "Atlas".into(), description: None, folder: None }).unwrap();
            let project = next(&mut alex_app, |u| match u {
                Update::Created(p) => Some(p),
                _ => None,
            })
            .await;
            alex_app.requests.send(Request::CreateEmployee { name: "Builder".into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None }).unwrap();
            let builder = next(&mut alex_app, |u| match u {
                Update::EmployeeCreated(e) => Some(e),
                _ => None,
            })
            .await;
            let owned = |title: &str| {
                let Command::CreateTicket(mut create) = new_ticket(project, title) else { unreachable!() };
                create.owner_employee_id = Some(builder);
                Command::CreateTicket(create)
            };
            done(&mut robin_app, owned("Robin's ticket"), None).await;
            let robins = tickets(&load(&mut robin_app, Some(project)).await, ColumnKind::Backlog).into_iter().find(|t| t.title == "Robin's ticket").unwrap().id;
            done(&mut robin_app, Command::StartTicket(ticket::StartTicket { ticket_id: robins, employee_id: None }), None).await;
            let waiting = until(&mut robin_app, project, |t| t.id == robins && t.needs == Some(ticket::TicketNeed::Login)).await;
            assert_eq!(board::login_wait(&waiting, Some(robin.id), false).as_deref(), Some("◆ Waiting for your Claude login"));
            assert_eq!(board::login_wait(&waiting, Some(alex.id), false).as_deref(), Some("Waiting for robin's Claude login"));
            assert!(board::take_over_offered(&waiting, Some(alex.id)) && !board::take_over_offered(&waiting, Some(robin.id)));
            say("robin's ticket: '◆ Waiting for your Claude login' for robin, 'Waiting for robin's Claude login' + Run on my login for alex");

            // Alex: Run on my login (after the confirmation in the window); it goes on, on Alex's.
            done(&mut alex_app, Command::UseMyLogin(ticket::UseMyLogin { ticket_id: robins }), None).await;
            let taken = until(&mut alex_app, project, |t| t.id == robins && t.needs.is_none() && t.login_user == alex.id).await;
            assert_eq!((taken.created_by_user, taken.login_username.as_str()), (robin.id, "alex"), "the creator stays");
            say("run on my login: the ticket runs on alex's login, robin stays its creator");

            // /usage: each their own only.
            let usage_of = |app: &mut Handle| {
                app.requests.send(Request::LoadSubscription(true)).unwrap();
            };
            let mut mine = Default::default();
            for _ in 0..50 {
                usage_of(&mut alex_app);
                mine = next(&mut alex_app, |u| match u {
                    Update::Subscription(s) if s.fetched_at_ms > 0 => Some(s),
                    _ => None,
                })
                .await;
                if mine.error.is_none() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            assert!(matches!(account::usage_of(Some(&mine), alex.id), account::Usage::Limits(l) if !l.is_empty()), "{mine:?}");
            usage_of(&mut robin_app);
            let theirs = next(&mut robin_app, |u| match u {
                Update::Subscription(s) if s.fetched_at_ms > 0 => Some(s),
                _ => None,
            })
            .await;
            assert_eq!(account::usage_of(Some(&theirs), robin.id), account::Usage::Unavailable("Connect your Claude login to see your usage.".into()));
            assert_eq!(account::usage_of(Some(&mine), robin.id), account::Usage::Unknown, "never another's");
            say("usage: alex sees alex's limits; robin sees 'Connect your Claude login to see your usage.'");

            // Alex's limit: Alex's next ticket pauses, its discussion says so.
            std::fs::write(folder.join("bin").join("quota-users"), format!("{}\n", alex.id)).unwrap();
            done(&mut alex_app, owned("Alex's ticket"), None).await;
            let alexs = tickets(&load(&mut alex_app, Some(project)).await, ColumnKind::Backlog).into_iter().find(|t| t.title == "Alex's ticket").unwrap();
            done(&mut alex_app, Command::StartTicket(ticket::StartTicket { ticket_id: alexs.id, employee_id: None }), None).await;
            let conversation = alexs.conversation_id.expect("its discussion");
            let mut paused = None;
            for _ in 0..150 {
                alex_app.requests.send(Request::Board(BoardCall::Discussion(Some(conversation)))).unwrap();
                let messages = next(&mut alex_app, |u| match u {
                    Update::Board(BoardUpdate::Discussion(c, result)) if c == conversation => Some(result.expect("the discussion reads")),
                    _ => None,
                })
                .await;
                paused = messages.iter().find_map(|m| m.system.as_ref().filter(|s| discussion::login_line(s, &m.text) == Some(discussion::LoginLine::Paused)).map(|_| m.text.clone()));
                if paused.is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            let paused = paused.expect("a Paused line in the discussion");
            assert!(paused.starts_with("Paused: alex's Claude limit is reached until "), "{paused}");
            alex_app.requests.send(Request::Board(BoardCall::Discussion(None))).unwrap();
            say(&format!("alex's limit: '{paused}'"));
            std::fs::remove_file(folder.join("bin").join("quota-users")).unwrap();
            drop((alex_app, robin_app));
        }

        /// The ticket that `pick` finds once the board shows it (polled; at most 20 s).
        async fn until(app: &mut Handle, project: ProjectId, pick: impl Fn(&ticket::TicketView) -> bool) -> ticket::TicketView {
            for _ in 0..200 {
                let view = load(app, Some(project)).await;
                if let Some(t) = view.columns.iter().flat_map(|c| c.tickets.iter()).find(|t| pick(t)) {
                    return t.clone();
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            panic!("the board never showed it");
        }

        async fn stop_daemon(root: &std::path::Path, server: tokio::task::JoinHandle<anyhow::Result<()>>) {
            aurelius_client::bridge::shutdown_daemon(root).await.expect("shutdown");
            tokio::time::timeout(Duration::from_secs(20), server).await.expect("daemon stops").expect("join").expect("clean stop");
        }
    }

    #[test]
    fn a_discussion_write_that_cannot_go_answers_the_discussion() {
        let ticket = aurelius_domain::TicketId::new();
        let post = BoardCall::Write { command: Command::PostTicketMessage(aurelius_domain::ticket::PostTicketMessage { confirm_secret: false, ticket_id: ticket, text: "Hi".into(), images: vec![], files: vec![] }), expected_version: None, done: None };
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

    /// E19: no vault value, key or held message is ever printed by this window.
    #[test]
    fn a_vault_value_or_key_is_never_printed() {
        const FAKE: &str = "sk-test-FAKE-0000";
        let item_id = aurelius_domain::VaultItemId::new();
        let create = Request::Vault(VaultCall::Create(aurelius_protocol::VaultCreateParams { project_id: None, kind: aurelius_domain::vault::VaultKind::Secret, name: "GitHub token".into(), env_name: "GITHUB_TOKEN".into(), description: None, value: FAKE.into() }));
        let update = Request::Vault(VaultCall::Update(aurelius_protocol::VaultUpdateParams { item_id, name: None, env_name: None, description: None, value: Some(FAKE.into()), kind: None }));
        let export = Request::Vault(VaultCall::KeyExport { password: Password::new("Tr0ub4dor&3-zebra-alex".into()) });
        let shown = Update::Vault(VaultUpdate::Value { item_id, purpose: aurelius_domain::vault::RevealPurpose::Show, result: Ok(Revealed::new(FAKE.into())) });
        let key = Update::Vault(VaultUpdate::Key(Ok(KeyFile { key_id: "k1".into(), key: Revealed::new("aurelius-vault-key:v1:dev:k1:c2stdGVzdC1GQUtFLTAwMDA".into()) })));
        let post = Command::SendGroupMessage(aurelius_domain::group_chat::SendGroupMessage { confirm_secret: false, text: format!("the token is {FAKE}"), images: vec![], files: vec![], channel: None });
        let held = Update::LooksLikeSecret(SecretHold::new(vec!["GitHub token".into()], confirmed(&post).unwrap()));
        for printed in [format!("{create:?}"), format!("{update:?}"), format!("{export:?}"), format!("{shown:?}"), format!("{key:?}"), format!("{held:?}")] {
            assert!(!printed.contains("FAKE") && !printed.contains("c2st") && !printed.contains("Tr0ub4dor"), "{printed}");
        }
        assert!(format!("{held:?}").contains("GitHub token"), "the names may show");
    }

    /// B10: the three messages a board member writes are held back and go again with
    /// `confirm_secret`; a ticket post as the board's write, a chat message as a message.
    #[test]
    fn a_held_message_goes_again_confirmed() {
        use aurelius_domain::error::{AppError, ErrorCode};
        let group = Command::SendGroupMessage(aurelius_domain::group_chat::SendGroupMessage { confirm_secret: false, text: "Hi Robin".into(), images: vec![], files: vec![], channel: None });
        let chat = Command::SendEmployeeMessage(employee::SendEmployeeMessage { confirm_secret: false, employee_id: EmployeeId::new(), text: "Hi Forge".into(), conversation_id: ConversationId::new(), images: vec![], files: vec![], interrupt: false });
        let post = Command::PostTicketMessage(aurelius_domain::ticket::PostTicketMessage { confirm_secret: false, ticket_id: aurelius_domain::TicketId::new(), text: "Hi Alex".into(), images: vec![], files: vec![] });
        for command in [&group, &chat, &post] {
            let flag = match confirmed(command).expect("a message B10 checks") {
                Command::SendGroupMessage(m) => m.confirm_secret,
                Command::SendEmployeeMessage(m) => m.confirm_secret,
                Command::PostTicketMessage(m) => m.confirm_secret,
                _ => false,
            };
            assert!(flag);
        }
        assert!(confirmed(&Command::CreateProject(project::CreateProject { name: "x".into(), description: None, folder: None })).is_none(), "anything else is refused, not held");
        let hold = SecretHold::new(vec!["GitHub token".into()], confirmed(&post).unwrap());
        assert_eq!(hold.text(), "Hi Alex");
        assert!(matches!(hold.resend(), Request::Board(BoardCall::Write { command: Command::PostTicketMessage(p), .. }) if p.confirm_secret));
        let hold = SecretHold::new(vec![], confirmed(&group).unwrap());
        assert_eq!(hold.text(), "Hi Robin");
        assert!(matches!(hold.resend(), Request::SendConfirmed(Command::SendGroupMessage(m)) if m.confirm_secret));
        // The server's refusal: the names only; for a text that is no message, what to do.
        let err = ClientError::Protocol(AppError::looks_like_secret(vec!["GitHub token".into()]));
        assert_eq!(secret_names(&err), Some(vec!["GitHub token".to_string()]));
        assert_eq!(board_write_failure(&err), "This looks like a vault secret: GitHub token. Take the value out and try again.");
        assert_eq!(secret_names(&ClientError::Protocol(AppError::new(ErrorCode::Validation, "x"))), None);
        // R2 D-m5: refused although confirmed (a value in an attached file): no new hold, but what to do.
        let sent_anyway = confirmed(&chat).unwrap();
        assert!(!secret_in_file(&sent_anyway) && !secret_in_file(&chat), "no file: the server still refused the text");
        assert_eq!(secret_refused(&sent_anyway, &[]), Some(secret_still_refusal(&[])));
        assert_eq!(secret_refused(&chat, &[]), None, "the first refusal asks");
        let mut with_file = sent_anyway.clone();
        if let Command::SendEmployeeMessage(m) = &mut with_file {
            m.files.push("/tmp/notes.txt".into());
        }
        assert!(secret_in_file(&with_file));
        assert_eq!(secret_refused(&with_file, &["GitHub token".into()]), Some(secret_in_file_refusal(&["GitHub token".into()])));
        assert!(confirmed(&sent_anyway).is_none(), "\"Send anyway?\" is not asked again");
        assert_eq!(secret_in_file_refusal(&["GitHub token".into()]), "Not sent: the value of GitHub token is in an attached file, which is never redacted. Take it out of the attached file and send again.");
        // Sent again, it is a plain envelope with a fresh command id.
        let a = to_envelope(Request::SendConfirmed(confirmed(&group).unwrap())).unwrap();
        let b = to_envelope(Request::SendConfirmed(confirmed(&group).unwrap())).unwrap();
        assert_ne!(a.command_id, b.command_id);
    }

    /// AUR-MU-04-D3: every text command of F5 the desktop sends asks "Send anyway?" once; a ticket
    /// write goes again as the board's write with its version and words; refused again, it says why.
    #[test]
    fn every_f5_command_is_held_once() {
        use aurelius_domain::schedule::{CreateTaskSchedule, Recurrence, UpdateTaskSchedule};
        use aurelius_domain::task::RunSettings;
        let (project_id, employee_id, ticket_id, approval_id) = (ProjectId::new(), EmployeeId::new(), aurelius_domain::TicketId::new(), aurelius_domain::ApprovalId::new());
        let feedback = || "x".to_string();
        let commands = [
            to_envelope(Request::CreateTask { project_id, employee_id: None, title: "x".into(), goal: "x".into(), criteria: vec![], run: RunSettings::default(), files: vec![] }).unwrap().command,
            Command::CreateTaskSchedule(CreateTaskSchedule { confirm_secret: false, project_id, employee_id, title: "x".into(), goal: "x".into(), acceptance_criteria: vec![], files: vec![], run: RunSettings::default(), recurrence: Recurrence::Daily { at_minute: 1 }, paused: true }),
            Command::UpdateTaskSchedule(UpdateTaskSchedule { confirm_secret: false, schedule_id: aurelius_domain::TaskScheduleId::new(), employee_id, title: "x".into(), goal: "x".into(), acceptance_criteria: vec![], add_files: vec![], remove_files: vec![], run: RunSettings::default(), recurrence: Recurrence::Daily { at_minute: 1 } }),
            to_envelope(Request::AddIdea { project_id, title: "x".into(), note: None, kind: Default::default(), files: vec![] }).unwrap().command,
            to_envelope(Request::UpdateIdea { idea_id: aurelius_domain::IdeaId::new(), title: "x".into(), note: None, kind: None, add_files: vec![], remove_files: vec![] }).unwrap().command,
            Command::UpdateTicket(crate::screens::board::empty_update(ticket_id)),
            Command::RevisePlan(aurelius_domain::ticket::RevisePlan { confirm_secret: false, ticket_id, feedback: feedback() }),
            Command::SendBackTicket(aurelius_domain::ticket::SendBackTicket { confirm_secret: false, ticket_id, feedback: feedback() }),
            Command::AcceptTicket(aurelius_domain::ticket::AcceptTicket { confirm_secret: false, ticket_id, note: None }),
            to_envelope(Request::DecidePlan { approval_id, decision: aurelius_domain::approval::PlanDecision::Revise, feedback: Some(feedback()) }).unwrap().command,
            to_envelope(Request::AnswerQuestion { approval_id, answers: Default::default() }).unwrap().command,
        ];
        for command in &commands {
            assert_eq!(secret_refused(command, &[]), None, "the first refusal asks: {command:?}");
            let again = confirmed(command).unwrap_or_else(|| panic!("held: {command:?}"));
            assert!(confirmed(&again).is_none(), "asked once: {command:?}");
            assert_eq!(secret_refused(&again, &["D3 token".into()]), Some(secret_still_refusal(&["D3 token".into()])), "{command:?}");
        }
        let hold = SecretHold::new(vec![], confirmed(&commands[6]).unwrap()).on_board(Some(7), Some("done".into()));
        assert!(matches!(hold.resend(), Request::Board(BoardCall::Write { command: Command::RevisePlan(c), expected_version: Some(7), done: Some(d) }) if c.confirm_secret && d == "done"));
        let hold = SecretHold::new(vec![], confirmed(&commands[9]).unwrap());
        assert!(matches!(hold.resend(), Request::SendConfirmed(Command::DecidePlan(c)) if c.confirm_secret));
        // Names, instructions, memories and paths: "take the value out", never "Send anyway?".
        let memory = to_envelope(Request::Remember { project_id, kind: aurelius_domain::memory::MemoryKind::Decision, content: "x".into() }).unwrap().command;
        assert!(confirmed(&memory).is_none());
        assert_eq!(secret_refused(&memory, &["D3 token".into()]), Some(secret_refusal(&["D3 token".into()])));
    }

    /// The vault's refusals in the user's words; nothing of it waits in the outbox, and
    /// without a session only its lists wait for the sign-in.
    #[test]
    fn the_vault_says_why_and_keeps_nothing_for_later() {
        use aurelius_domain::error::{AppError, ErrorCode};
        let say = |code, message: &str| vault_failure(&ClientError::Protocol(AppError::new(code, message)));
        assert_eq!(say(ErrorCode::VaultUndecryptable, "x"), VAULT_UNDECRYPTABLE);
        assert_eq!(say(ErrorCode::VaultKindLocked, "x"), VAULT_KIND_LOCKED);
        assert_eq!(say(ErrorCode::UnknownMethod, "x"), VAULT_NOT_BUILT);
        assert_eq!(say(ErrorCode::BoardMemberRequired, "x"), BOARD_MEMBER_REQUIRED);
        assert_eq!(say(ErrorCode::Validation, "GITHUB_TOKEN is used in this scope."), "GITHUB_TOKEN is used in this scope.");
        assert_eq!(vault_failure(&ClientError::Disconnected), VAULT_CONNECTION_DROPPED);
        let item = aurelius_domain::VaultItemId::new();
        assert!(held_read(&Request::Vault(VaultCall::List(None))));
        assert!(held_read(&Request::Vault(VaultCall::Audit { ask: AuditAsk { project_id: None, all: true, item_id: None }, before: None })));
        assert!(!held_read(&Request::Vault(VaultCall::Reveal { item_id: item, purpose: aurelius_domain::vault::RevealPurpose::Show })));
        assert!(!held_read(&Request::Vault(VaultCall::KeyExport { password: Password::new("pw".into()) })));
        assert!(to_envelope(Request::Vault(VaultCall::Archive(item))).is_none(), "never in the outbox");
        assert!(matches!(VaultCall::Archive(item).refused("offline".into()), VaultUpdate::Archived(i, Err(_)) if i == item));
        assert!(matches!(VaultCall::KeyExport { password: Password::new("pw".into()) }.refused("offline".into()), VaultUpdate::Key(Err(_))));
    }

    /// The vault through this app's backend against a daemon of this branch (protocol 5,
    /// feature `vault`): the feature is announced, a board member's calls reach the vault and
    /// come back in its words, a list is a read that waits for the sign-in, and a reveal
    /// without a session is refused.
    mod vault_against_a_daemon {
        use super::super::*;

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

        async fn saved(app: &mut Handle) -> aurelius_domain::vault::VaultItemView {
            next(app, |u| match u {
                Update::Vault(VaultUpdate::Saved(r)) => Some(r.expect("saved")),
                _ => None,
            })
            .await
        }

        async fn listed(app: &mut Handle, scope: Option<ProjectId>) -> Result<aurelius_domain::vault::VaultList, String> {
            app.requests.send(Request::Vault(VaultCall::List(scope))).unwrap();
            next(app, |u| match u {
                Update::Vault(VaultUpdate::Listed(s, result)) if s == scope => Some(result),
                _ => None,
            })
            .await
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn the_vault_calls_reach_the_server() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let server = daemon(&root).await;
            let target = || Some(Target::Local { root: root.clone(), spawn: false });

            // Alex is the first board member: the vault is announced and its calls are answered.
            let mut alex = start(target(), super::memory_tokens().0);
            let online = next(&mut alex, |u| match u {
                Update::Connection(Connection::Online { server, .. }) => Some(server),
                _ => None,
            })
            .await;
            assert!(online.vault, "this daemon has the feature `vault`");
            alex.requests.send(Request::BoardMembers(UserCall::Create { username: "alex".into(), password: Password::new("Tr0ub4dor&3-zebra-alex".into()) })).unwrap();
            next(&mut alex, |u| matches!(u, Update::Who(w) if w.user.is_some()).then_some(())).await;
            let empty = listed(&mut alex, None).await.expect("a board member lists the vault");
            assert!(empty.items.is_empty() && empty.company_items.is_empty());
            assert_eq!(empty.counts.iter().map(|c| (c.project_id, c.count)).collect::<Vec<_>>(), vec![(None, 0)], "the company, empty");
            alex.requests.send(Request::Vault(VaultCall::Reveal { item_id: aurelius_domain::VaultItemId::new(), purpose: aurelius_domain::vault::RevealPurpose::Show })).unwrap();
            let reveal = next(&mut alex, |u| match u {
                Update::Vault(VaultUpdate::Value { result, .. }) => Some(result),
                _ => None,
            })
            .await;
            assert!(reveal.is_err());
            alex.requests.send(Request::Vault(VaultCall::Create(aurelius_protocol::VaultCreateParams { project_id: None, kind: aurelius_domain::vault::VaultKind::Secret, name: "GitHub token".into(), env_name: "GITHUB_TOKEN".into(), description: None, value: "sk-test-FAKE-0000".into() }))).unwrap();
            let saved = next(&mut alex, |u| match u {
                Update::Vault(VaultUpdate::Saved(result)) => Some(result),
                _ => None,
            })
            .await;
            let saved = saved.expect("created");
            assert!(saved.value.is_none() && !format!("{saved:?}").contains("FAKE"), "a secret's value never comes back");
            // The next read has it, still without its value.
            let list = listed(&mut alex, None).await.expect("listed");
            assert!(list.items.iter().any(|i| i.id == saved.id && i.value.is_none()));

            // Robin's device without a session: a reveal is refused at the gate; a list waits for the sign-in.
            let mut robin = start(target(), super::memory_tokens().0);
            next(&mut robin, |u| matches!(u, Update::Who(w) if w.user.is_none()).then_some(())).await;
            robin.requests.send(Request::Vault(VaultCall::Reveal { item_id: aurelius_domain::VaultItemId::new(), purpose: aurelius_domain::vault::RevealPurpose::Copy })).unwrap();
            let refused = next(&mut robin, |u| match u {
                Update::Vault(VaultUpdate::Value { result: Err(why), .. }) => Some(why),
                _ => None,
            })
            .await;
            assert_eq!(refused, BOARD_MEMBER_REQUIRED);
            drop(robin);
            drop(alex);
            stop(&root, server).await;
        }

        /// The whole vault: company and project items, "overrides Company", reveal, kind locked,
        /// url → text without a new value (B7), the audit log, archive, the event to another
        /// device, and B10 in the Team chat with the items' names.
        #[tokio::test(flavor = "multi_thread")]
        async fn the_vault_end_to_end() {
            use aurelius_domain::vault::{RevealPurpose, VaultKind};
            const FAKE: &str = "sk-test-FAKE-0000";
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let server = daemon(&root).await;
            let target = || Some(Target::Local { root: root.clone(), spawn: false });
            let mut alex = start(target(), super::memory_tokens().0);
            alex.requests.send(Request::BoardMembers(UserCall::Create { username: "alex".into(), password: Password::new("Tr0ub4dor&3-zebra-alex".into()) })).unwrap();
            next(&mut alex, |u| matches!(u, Update::Who(w) if w.user.is_some()).then_some(())).await;
            alex.requests.send(Request::CreateProject { name: "aurelius".into(), description: None, folder: None }).unwrap();
            let project = next(&mut alex, |u| match u {
                Update::Created(id) => Some(id),
                _ => None,
            })
            .await;
            let create = |project_id: Option<ProjectId>, kind: VaultKind, name: &str, env: &str, value: &str| {
                Request::Vault(VaultCall::Create(aurelius_protocol::VaultCreateParams { project_id, kind, name: name.into(), env_name: env.into(), description: None, value: value.into() }))
            };
            // MU4-01: company items of each type.
            alex.requests.send(create(None, VaultKind::Secret, "GitHub token", "GITHUB_TOKEN", FAKE)).unwrap();
            let token = saved(&mut alex).await;
            alex.requests.send(create(None, VaultKind::Url, "Staging URL", "STAGING_URL", "https://stg.example.test")).unwrap();
            let url = saved(&mut alex).await;
            assert!(token.value.is_none() && url.value.as_deref() == Some("https://stg.example.test"));
            // MU4-02 + B4: a project item with the company's key overrides it.
            alex.requests.send(create(Some(project), VaultKind::Secret, "GitHub token (aurelius)", "GITHUB_TOKEN", "sk-test-FAKE-0001")).unwrap();
            let own = saved(&mut alex).await;
            let list = listed(&mut alex, Some(project)).await.unwrap();
            assert!(list.items.iter().any(|i| i.id == own.id && i.overrides_company));
            assert!(list.company_items.iter().any(|i| i.id == token.id));
            // B6: reveal shows the value, only in the reveal's answer.
            alex.requests.send(Request::Vault(VaultCall::Reveal { item_id: token.id, purpose: RevealPurpose::Show })).unwrap();
            let value = next(&mut alex, |u| match u {
                Update::Vault(VaultUpdate::Value { result, .. }) => Some(result.unwrap()),
                _ => None,
            })
            .await;
            assert_eq!(value.expose(), FAKE);
            drop(value);
            // B7: a secret stays a secret; a url becomes text.
            alex.requests.send(Request::Vault(VaultCall::Update(aurelius_protocol::VaultUpdateParams { item_id: token.id, name: None, env_name: None, description: None, value: None, kind: Some(VaultKind::Text) }))).unwrap();
            let locked = next(&mut alex, |u| match u {
                Update::Vault(VaultUpdate::Saved(r)) => Some(r),
                _ => None,
            })
            .await;
            assert_eq!(locked.unwrap_err(), VAULT_KIND_LOCKED);
            alex.requests.send(Request::Vault(VaultCall::Update(aurelius_protocol::VaultUpdateParams { item_id: url.id, name: None, env_name: None, description: None, value: None, kind: Some(VaultKind::Text) }))).unwrap();
            assert_eq!(saved(&mut alex).await.kind, VaultKind::Text);
            // MU4-09: the audit log names the reveal, never a value.
            alex.requests.send(Request::Vault(VaultCall::Audit { ask: AuditAsk { project_id: None, all: true, item_id: None }, before: None })).unwrap();
            let page = next(&mut alex, |u| match u {
                Update::Vault(VaultUpdate::Audit { result, .. }) => Some(result.unwrap()),
                _ => None,
            })
            .await;
            assert!(page.rows.iter().any(|r| r.action == aurelius_domain::vault::VaultAuditAction::Reveal && r.item_id == Some(token.id)));
            assert!(!serde_json::to_string(&page.rows).unwrap().contains("FAKE"));
            // B10: the Team chat holds back a message with the secret's value, then takes it confirmed.
            alex.requests.send(Request::SendGroup { text: format!("token: {FAKE}"), images: vec![], files: vec![], channel: None }).unwrap();
            let hold = next(&mut alex, |u| match u {
                Update::LooksLikeSecret(h) => Some(h),
                _ => None,
            })
            .await;
            assert_eq!(hold.names, vec!["GitHub token".to_string()]);
            alex.requests.send(hold.resend()).unwrap();
            // B8 / E9: the key export asks the own password again; a wrong one gives nothing.
            let export = |password: &str| Request::Vault(VaultCall::KeyExport { password: Password::new(password.into()) });
            let key = |u: Update| match u {
                Update::Vault(VaultUpdate::Key(result)) => Some(result),
                _ => None,
            };
            alex.requests.send(export("not-alex-password")).unwrap();
            assert_eq!(next(&mut alex, key).await.unwrap_err(), "That password is wrong.");
            alex.requests.send(export("Tr0ub4dor&3-zebra-alex")).unwrap();
            let file = next(&mut alex, key).await.unwrap();
            let env = crate::screens::vault::key_env(file.key.expose()).expect("a v1 key line");
            assert!(file.key.expose().starts_with(&format!("aurelius-vault-key:v1:{env}:{}:", file.key_id)));
            // MU4-01 AC2: archived; the event reaches the window.
            alex.requests.send(Request::Vault(VaultCall::Archive(url.id))).unwrap();
            next(&mut alex, |u| matches!(u, Update::Vault(VaultUpdate::Archived(_, Ok(_)))).then_some(())).await;
            next(&mut alex, |u| matches!(u, Update::Vault(VaultUpdate::Changed)).then_some(())).await;
            drop(alex);
            stop(&root, server).await;
        }

        /// The vault item of the B10 tests below: its value is a fake, no other test's prefix.
        const D3: &str = "sk-test-FAKE-d3-0000";

        /// A text the server refused for a vault secret's value: held back by the item's name, never told as an error.
        async fn held(app: &mut Handle, request: Request) -> SecretHold {
            app.requests.send(request).unwrap();
            let hold = next(app, |u| match u {
                Update::LooksLikeSecret(h) => Some(h),
                Update::Error(e) if e.contains("vault secret") => panic!("not held: {e}"),
                _ => None,
            })
            .await;
            assert_eq!(hold.names, vec!["D3 token".to_string()]);
            hold
        }

        /// What "Send anyway" brought, up to `pick`: never a second "Send anyway?" (D-m5), never a secret's refusal.
        async fn sent_anyway<T>(app: &mut Handle, hold: SecretHold, mut pick: impl FnMut(Update) -> Option<T>) -> T {
            assert!(confirmed(&hold.command).is_none(), "it goes with confirm_secret");
            app.requests.send(hold.resend()).unwrap();
            next(app, |u| match u {
                Update::LooksLikeSecret(_) => panic!("\"Send anyway?\" asked again"),
                Update::Error(e) if e.contains("vault secret") => panic!("refused again: {e}"),
                Update::Board(BoardUpdate::Failed { message, .. }) if message.contains("vault secret") => panic!("refused again: {message}"),
                u => pick(u),
            })
            .await
        }

        /// "Send anyway" refused again (the value is in an attached file): what it says, no second ask.
        async fn refused_again(app: &mut Handle, hold: SecretHold) -> String {
            app.requests.send(hold.resend()).unwrap();
            next(app, |u| match u {
                Update::LooksLikeSecret(_) => panic!("\"Send anyway?\" asked again"),
                Update::Error(e) => Some(e),
                Update::Board(BoardUpdate::Failed { message, .. }) => Some(message),
                _ => None,
            })
            .await
        }

        /// B10 for every command of AUR-MU-04-F5 the desktop sends from a text field: refused, it asks
        /// "Send anyway?" with the item's name; it goes again with `confirm_secret` and gets past B10
        /// (stored redacted, or the server's own answer for a ticket that is not in that column);
        /// refused again for a value in an attached file, it says so once. A text that cannot be
        /// confirmed (a memory) says "take the value out" and does not ask.
        #[tokio::test(flavor = "multi_thread")]
        async fn every_text_command_asks_send_anyway_once() {
            use aurelius_domain::schedule::{CreateTaskSchedule, Recurrence, UpdateTaskSchedule};
            use aurelius_domain::task::RunSettings;
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let server = daemon(&root).await;
            let files = tempfile::tempdir().unwrap();
            let notes = files.path().join("notes.txt");
            std::fs::write(&notes, format!("token {D3}\n")).unwrap();
            let notes = notes.display().to_string();
            let mut alex = start(Some(Target::Local { root: root.clone(), spawn: false }), super::memory_tokens().0);
            alex.requests.send(Request::BoardMembers(UserCall::Create { username: "alex".into(), password: Password::new("Tr0ub4dor&3-zebra-alex".into()) })).unwrap();
            next(&mut alex, |u| matches!(u, Update::Who(w) if w.user.is_some()).then_some(())).await;
            alex.requests.send(Request::CreateProject { name: "aurelius".into(), description: None, folder: None }).unwrap();
            let project = next(&mut alex, |u| match u {
                Update::Created(id) => Some(id),
                _ => None,
            })
            .await;
            alex.requests.send(Request::CreateEmployee { name: "Builder".into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None }).unwrap();
            let builder = next(&mut alex, |u| match u {
                Update::EmployeeCreated(e) => Some(e),
                _ => None,
            })
            .await;
            alex.requests.send(Request::Vault(VaultCall::Create(aurelius_protocol::VaultCreateParams { project_id: None, kind: aurelius_domain::vault::VaultKind::Secret, name: "D3 token".into(), env_name: "D3_TOKEN".into(), description: None, value: D3.into() }))).unwrap();
            saved(&mut alex).await;
            let ideas = |app: &mut Handle| app.requests.send(Request::LoadIdeas(Some(project))).unwrap();
            let run = RunSettings::default;

            // CreateTask: a draft task, created once sent anyway.
            let task = |files: Vec<String>| Request::CreateTask { project_id: project, employee_id: None, title: "Rotate".into(), goal: format!("Rotate {D3}"), criteria: vec![], run: run(), files };
            let hold = held(&mut alex, task(vec![])).await;
            sent_anyway(&mut alex, hold, |u| matches!(u, Update::TaskCreated(_)).then_some(())).await;
            let hold = held(&mut alex, task(vec![notes.clone()])).await;
            assert_eq!(refused_again(&mut alex, hold).await, secret_in_file_refusal(&["D3 token".into()]));

            // CreateTaskSchedule and UpdateTaskSchedule: stored without the value.
            let schedule = |goal: String, files: Vec<String>| CreateTaskSchedule { confirm_secret: false, project_id: project, employee_id: builder, title: "Nightly".into(), goal, acceptance_criteria: vec![], files, run: run(), recurrence: Recurrence::Daily { at_minute: 120 }, paused: true };
            let hold = held(&mut alex, Request::CreateSchedule(schedule(format!("Check {D3}"), vec![]))).await;
            let made = sent_anyway(&mut alex, hold, |u| match u {
                Update::Schedules(list) => list.into_iter().find(|s| s.title == "Nightly"),
                _ => None,
            })
            .await;
            assert!(!made.goal.contains(D3) && made.goal.contains("D3_TOKEN"), "{}", made.goal);
            let hold = held(&mut alex, Request::CreateSchedule(schedule("Check the notes".into(), vec![notes.clone()]))).await;
            assert_eq!(refused_again(&mut alex, hold).await, secret_in_file_refusal(&["D3 token".into()]));
            let edit = |goal: String, add_files: Vec<String>| UpdateTaskSchedule { confirm_secret: false, schedule_id: made.id, employee_id: builder, title: "Nightly".into(), goal, acceptance_criteria: vec![], add_files, remove_files: vec![], run: run(), recurrence: Recurrence::Daily { at_minute: 180 } };
            let hold = held(&mut alex, Request::UpdateSchedule(edit(format!("Check again {D3}"), vec![]))).await;
            sent_anyway(&mut alex, hold, |u| match u {
                Update::Schedules(list) => list.into_iter().find(|s| s.id == made.id && s.goal.starts_with("Check again")).map(|s| assert!(!s.goal.contains(D3))),
                _ => None,
            })
            .await;
            let hold = held(&mut alex, Request::UpdateSchedule(edit("Check the notes".into(), vec![notes.clone()]))).await;
            assert_eq!(refused_again(&mut alex, hold).await, secret_in_file_refusal(&["D3 token".into()]));

            // AddIdea and UpdateIdea.
            let add = |note: String, files: Vec<String>| Request::AddIdea { project_id: project, title: "Key rotation".into(), note: Some(note), kind: Default::default(), files };
            // No answer of its own: the list read after it shows it.
            let hold = held(&mut alex, add(format!("now {D3}"), vec![])).await;
            alex.requests.send(hold.resend()).unwrap();
            ideas(&mut alex);
            let idea = next(&mut alex, |u| match u {
                Update::Ideas(_, list) => list.into_iter().find(|i| i.title == "Key rotation"),
                Update::LooksLikeSecret(_) => panic!("\"Send anyway?\" asked again"),
                Update::Error(e) if e.contains("vault secret") => panic!("refused again: {e}"),
                _ => None,
            })
            .await;
            assert!(idea.note.as_deref().is_some_and(|n| !n.contains(D3) && n.contains("D3_TOKEN")), "{:?}", idea.note);
            let hold = held(&mut alex, add("see the notes".into(), vec![notes.clone()])).await;
            assert_eq!(refused_again(&mut alex, hold).await, secret_in_file_refusal(&["D3 token".into()]));
            let update = |note: String, add_files: Vec<String>| Request::UpdateIdea { idea_id: idea.id, title: "Key rotation".into(), note: Some(note), kind: None, add_files, remove_files: vec![] };
            let hold = held(&mut alex, update(format!("later {D3}"), vec![])).await;
            alex.requests.send(hold.resend()).unwrap();
            ideas(&mut alex);
            next(&mut alex, |u| match u {
                Update::Ideas(_, list) => list.into_iter().find(|i| i.id == idea.id && i.note.as_deref().is_some_and(|n| n.starts_with("later"))).map(|i| assert!(!i.note.unwrap().contains(D3))),
                Update::LooksLikeSecret(_) => panic!("asked again"),
                Update::Error(e) if e.contains("vault secret") => panic!("refused again: {e}"),
                _ => None,
            })
            .await;
            let hold = held(&mut alex, update("see the notes".into(), vec![notes.clone()])).await;
            assert_eq!(refused_again(&mut alex, hold).await, secret_in_file_refusal(&["D3 token".into()]));

            // CreateTicket (the board's write; the form waits under "Not sent") and UpdateTicket.
            let new_ticket = |description: String, files: Vec<String>| {
                Command::CreateTicket(aurelius_domain::ticket::CreateTicket { confirm_secret: false, project_id: project, title: "Deploy".into(), description: Some(description), priority: Default::default(), labels: vec![], deadline_ms: None, owner_employee_id: None, run: run(), files, start: false, from_message: None })
            };
            let board = |command: Command, done: &str| Request::Board(BoardCall::Write { command, expected_version: None, done: Some(done.into()) });
            alex.requests.send(board(new_ticket(format!("Deploy with {D3}"), vec![]), "created")).unwrap();
            next(&mut alex, |u| match u {
                Update::Board(BoardUpdate::Failed { ticket: None, message }) => Some(assert_eq!(message, NOT_SENT_SECRET)),
                _ => None,
            })
            .await;
            let hold = next(&mut alex, |u| match u {
                Update::LooksLikeSecret(h) => Some(h),
                _ => None,
            })
            .await;
            assert_eq!(hold.names, vec!["D3 token".to_string()]);
            let number = sent_anyway(&mut alex, hold, |u| match u {
                Update::Board(BoardUpdate::Created { number, text }) => Some((number, text)),
                _ => None,
            })
            .await;
            assert_eq!(number.1.as_deref(), Some("created"), "it went again with what the board says");
            let hold = held(&mut alex, board(new_ticket("see the notes".into(), vec![notes.clone()]), "created")).await;
            assert_eq!(refused_again(&mut alex, hold).await, secret_in_file_refusal(&["D3 token".into()]));
            alex.requests.send(Request::Board(BoardCall::Load(Some(project)))).unwrap();
            let ticket = next(&mut alex, |u| match u {
                Update::Board(BoardUpdate::Loaded(_, Ok(view))) => view.columns.into_iter().flat_map(|c| c.tickets).find(|t| t.number == number.0),
                _ => None,
            })
            .await;
            assert!(ticket.description.as_deref().is_some_and(|d| !d.contains(D3) && d.contains("D3_TOKEN")), "{:?}", ticket.description);
            let mut edit = crate::screens::board::empty_update(ticket.id);
            edit.description = Some(format!("Deploy later with {D3}"));
            let hold = held(&mut alex, board(Command::UpdateTicket(edit), "saved")).await;
            sent_anyway(&mut alex, hold, |u| match u {
                Update::Board(BoardUpdate::Done { ticket: Some(t), text }) if t == ticket.id => Some(assert_eq!(text.as_deref(), Some("saved"))),
                _ => None,
            })
            .await;

            // RevisePlan and SendBackTicket on a Backlog ticket: past B10, the server says it is not in that column.
            let revise = Command::RevisePlan(aurelius_domain::ticket::RevisePlan { confirm_secret: false, ticket_id: ticket.id, feedback: format!("Sign with {D3}") });
            let send_back = Command::SendBackTicket(aurelius_domain::ticket::SendBackTicket { confirm_secret: false, ticket_id: ticket.id, feedback: format!("Leave out {D3}") });
            for command in [revise, send_back] {
                let hold = held(&mut alex, board(command, "decided")).await;
                assert!(hold.text().contains(D3), "Cancel puts the feedback back");
                let why = sent_anyway(&mut alex, hold, |u| match u {
                    Update::Board(BoardUpdate::Failed { ticket: Some(t), message }) if t == ticket.id => Some(message),
                    _ => None,
                })
                .await;
                assert!(!why.contains("vault secret"), "{why}");
            }

            // DecidePlan and AnswerQuestion of a plan or a question no longer open: past B10, the server says so.
            let approval_id = aurelius_domain::ApprovalId::new();
            let decide = Request::DecidePlan { approval_id, decision: aurelius_domain::approval::PlanDecision::Revise, feedback: Some(format!("Deploy with {D3}")) };
            let answer = Request::AnswerQuestion { approval_id, answers: [("Which token?".to_owned(), format!("This one: {D3}"))].into_iter().collect() };
            for request in [decide, answer] {
                let hold = held(&mut alex, request).await;
                assert!(hold.board.is_none(), "a message, not the board's write");
                let why = sent_anyway(&mut alex, hold, |u| match u {
                    Update::Error(e) => Some(e),
                    _ => None,
                })
                .await;
                assert!(!why.contains("vault secret"), "{why}");
            }

            // A memory cannot be confirmed: "take the value out", no "Send anyway?".
            alex.requests.send(Request::Remember { project_id: project, kind: aurelius_domain::memory::MemoryKind::ProjectKnowledge, content: format!("The token is {D3}") }).unwrap();
            let why = next(&mut alex, |u| match u {
                Update::LooksLikeSecret(_) => panic!("a memory is not held"),
                Update::Error(e) => Some(e),
                _ => None,
            })
            .await;
            assert_eq!(why, secret_refusal(&["D3 token".into()]));
            drop(alex);
            stop(&root, server).await;
        }
    }

    /// The guideline calls against a real daemon (AUR-GL-01-D1): every read and write goes
    /// over the wire as the frozen protocol says and comes back as an answer, or (until the
    /// server implements it, F2) as "not on this server yet"; never a parse error.
    mod guidelines_against_a_daemon {
        use super::super::*;

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

        fn answered<T>(result: &Result<T, String>) {
            if let Err(why) = result {
                assert_eq!(why, GUIDELINES_NOT_BUILT, "an answer, or the server says it has no guidelines yet");
            }
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn the_guideline_calls_reach_the_server() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let (tx, rx) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(aurelius_daemon::run(root.clone(), Some(tx)));
            tokio::time::timeout(Duration::from_secs(20), rx).await.expect("daemon ready").expect("ready signal");
            let mut alex = start(Some(Target::Local { root: root.clone(), spawn: false }), super::memory_tokens().0);
            let online = next(&mut alex, |u| match u {
                Update::Connection(Connection::Online { server, .. }) => Some(server),
                _ => None,
            })
            .await;
            assert!(online.guidelines, "this daemon announces the feature `guidelines`");
            alex.requests.send(Request::BoardMembers(UserCall::Create { username: "alex".into(), password: Password::new("Tr0ub4dor&3-zebra-alex".into()) })).unwrap();
            next(&mut alex, |u| matches!(u, Update::Who(w) if w.user.is_some()).then_some(())).await;

            alex.requests.send(Request::Guidelines(GuidelineCall::List { archived: false })).unwrap();
            let list = next(&mut alex, |u| match u {
                Update::Guidelines(GuidelineUpdate::Listed { archived: false, result }) => Some(result),
                _ => None,
            })
            .await;
            answered(&list);
            alex.requests.send(Request::Guidelines(GuidelineCall::Proposals)).unwrap();
            answered(&next(&mut alex, |u| match u {
                Update::Guidelines(GuidelineUpdate::Proposals(r)) => Some(r),
                _ => None,
            })
            .await);
            alex.requests.send(Request::Guidelines(GuidelineCall::StarterSet)).unwrap();
            answered(&next(&mut alex, |u| match u {
                Update::Guidelines(GuidelineUpdate::Starter(r)) => Some(r),
                _ => None,
            })
            .await);
            alex.requests.send(Request::Guidelines(GuidelineCall::Effective((None, None)))).unwrap();
            answered(&next(&mut alex, |u| match u {
                Update::Guidelines(GuidelineUpdate::Effective(_, r)) => Some(r),
                _ => None,
            })
            .await);
            let write = GuidelineWrite::Create { title: "Rust".into(), text: "## Errors\n- No `unwrap()` outside tests.".into(), links: vec![] };
            alex.requests.send(Request::Guidelines(GuidelineCall::Write(write.clone()))).unwrap();
            let (back, result) = next(&mut alex, |u| match u {
                Update::Guidelines(GuidelineUpdate::Written { write, result, .. }) => Some((write, result)),
                _ => None,
            })
            .await;
            assert_eq!(back, write, "the window hears which write was answered");
            match result {
                Ok(result) => assert!(matches!(result, Some(aurelius_domain::CommandResult::GuidelineChanged { version: 1, .. })), "{result:?}"),
                Err(f) => assert_eq!(f.message, GUIDELINES_NOT_BUILT),
            }
            drop(alex);
            aurelius_client::bridge::shutdown_daemon(&root).await.expect("shutdown");
            tokio::time::timeout(Duration::from_secs(20), server).await.expect("daemon stops").expect("join").expect("clean stop");
        }
    }
}
