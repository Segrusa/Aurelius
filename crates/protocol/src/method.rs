//! Methods that read or open a session. Every method is `<area>.<verb>`; reads
//! are `list` or `get`. Writes are in `write.rs`; they all go through the same
//! pipeline (idempotency key, receipt, audit event, required right).

use crate::views::{FileScope, TimelineQuery};
use aurelius_domain::{ConversationId, EmployeeId, MessageId, ProjectId, RunId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", content = "params")]
pub enum Method {
    /// The first request of a session: who is calling. The protocol major in
    /// every request must equal the server's, else the answer is a hard error.
    #[serde(rename = "session.hello")]
    SessionHello(HelloParams),
    #[serde(rename = "server.get_health")]
    ServerGetHealth,

    #[serde(rename = "project.list")]
    ProjectList,
    #[serde(rename = "job.list")]
    JobList,
    /// A conversation's messages and settings.
    #[serde(rename = "conversation.get")]
    ConversationGet { conversation_id: ConversationId },
    /// The company's employees (not archived), oldest first.
    #[serde(rename = "employee.list")]
    EmployeeList,
    /// An employee's persistent chat with the user (company scope, no project):
    /// its messages and settings, as `conversation.get` answers them.
    #[serde(rename = "employee.chat")]
    EmployeeChat { employee_id: EmployeeId },
    /// The company's group chat (Team chat): the user and every employee. Its
    /// messages and settings, as `conversation.get` answers them; an employee's
    /// messages carry its name as `author`. Write with `group_chat.send`.
    /// `channel` (feature `team_channels`): a project's channel from
    /// `channel.list`; none: the general channel, the one older servers have.
    #[serde(rename = "group_chat.get")]
    GroupChatGet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<ConversationId>,
    },
    /// The channels of the Team chat (feature `team_channels`): the general
    /// one first, then one per project (archived projects too: read-only).
    #[serde(rename = "channel.list")]
    ChannelList,
    /// The company in one read: employees, the org chart setting and the
    /// management team (roots).
    #[serde(rename = "company.get")]
    CompanyGet,
    /// The board: its six columns left to right, each with its cards (not
    /// archived), for one project or every project.
    #[serde(rename = "board.get")]
    BoardGet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    /// One ticket with its main task and every task linked to it.
    #[serde(rename = "ticket.get")]
    TicketGet { ticket_id: aurelius_domain::TicketId },
    /// Tickets in board order (not archived); every given filter applies. `query`
    /// searches title, description and `#number`.
    #[serde(rename = "ticket.list")]
    TicketList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        column: Option<aurelius_domain::ticket::ColumnKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
    },
    /// The board members, oldest first (never a password's hash), disabled ones
    /// too (`UserView::disabled_at_ms`). Works without a session (the sign-in
    /// screen may offer the names). Apps may read it too.
    #[serde(rename = "user.list")]
    UserList,
    /// Whether the server has board members, and this connection's session's one.
    /// Works without a session.
    #[serde(rename = "user.whoami")]
    UserWhoami,
    /// Sign in (protocol 5): a right username and password give this connection a
    /// session and answer `Login` with its token, shown once (keep it for the
    /// server and give it in every hello). A wrong password, an unknown username
    /// and a disabled board member all answer `wrong_password`; after 5 failed tries
    /// for one username the server waits longer each time, up to 15 minutes
    /// (`too_many_attempts` with `retry_after_ms`). A session method: it changes
    /// only the connection and the sessions, so it has no receipt and no event.
    /// Not for apps.
    #[serde(rename = "user.login")]
    UserLogin(LoginParams),
    /// End this connection's session (the device must sign in again); answers
    /// `WhoAmI` without a board member. A session method.
    #[serde(rename = "user.logout")]
    UserLogout,
    /// The signed-in devices of a board member (none: the session's own),
    /// newest activity first (`SessionView::current` marks this one). Any board
    /// member may list another's (one role). Never a token or its hash.
    #[serde(rename = "session.list")]
    SessionList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_id: Option<aurelius_domain::UserId>,
    },
    /// The session's own Claude login (`ClaudeLoginView`). Feature of protocol 5.
    #[serde(rename = "claude_login.status")]
    ClaudeLoginStatus,
    /// What a running `claude_login.link` asks for: the code Claude showed after
    /// the sign-in at Anthropic, written to `claude auth login` as typed. Only
    /// the connection that started the link. A session method: no receipt, no
    /// request hash, no event, never printed or logged. Answers `ClaudeLogin`.
    #[serde(rename = "claude_login.input")]
    ClaudeLoginInput(ClaudeLoginInputParams),
    /// The vault of a scope (feature `vault`): none, the company's items; a
    /// project, its items plus the company's apart (`VaultList::company_items`),
    /// with `overrides_company` on a project item whose env name the company
    /// has too, and the item count of every scope. Never a secret's value.
    /// Archived items are left out. Board members only; not for apps (B5).
    #[serde(rename = "vault.list")]
    VaultList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    /// One item's value (`VaultValue`, never printed), for a board member with a
    /// session. Writes an audit row `reveal` (show) or `copy` with the session's
    /// device. `vault_undecryptable` when the key does not fit. Not for apps.
    #[serde(rename = "vault.reveal")]
    VaultReveal { item_id: aurelius_domain::VaultItemId, purpose: aurelius_domain::vault::RevealPurpose },
    /// The audit log, newest first (`VaultAuditPage`): the company's (nothing
    /// given), a project's, or every scope's (`all`); one item's with `item_id`.
    /// `before`: rows older than that row id; `limit` at most 200 (default 50).
    /// Never a value. Not for apps.
    #[serde(rename = "vault.audit")]
    VaultAudit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        all: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item_id: Option<aurelius_domain::VaultItemId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },
    /// The server's vault key (`VaultKeyExport`, never printed), for a restore on
    /// another machine (B8): a board member with a session that needs no new
    /// password, who gives their password again (`wrong_password`; it counts
    /// for the sign-in limits, `too_many_attempts`). Writes an audit row
    /// `key_export` and tells every board member. Not found before the first
    /// item. Not for apps.
    #[serde(rename = "vault.key_export")]
    VaultKeyExport(KeyExportParams),
    /// The guideline library (feature `guidelines`): every guideline without its
    /// text, alphabetical; archived ones only with `archived` (then only those).
    #[serde(rename = "guideline.list")]
    GuidelineList {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        archived: bool,
    },
    /// One guideline with the text of its current version, or of `version`.
    #[serde(rename = "guideline.get")]
    GuidelineGet {
        guideline_id: aurelius_domain::GuidelineId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<u32>,
    },
    /// Every version of a guideline, newest first, with its text and source.
    #[serde(rename = "guideline.versions")]
    GuidelineVersions { guideline_id: aurelius_domain::GuidelineId },
    /// The set a run of this employee in this project would get now, in block
    /// order, with `total_chars` against `limit_chars` (B4). Only an employee:
    /// its own chat (employee guidelines); only a project: its guidelines alone.
    #[serde(rename = "guideline.effective")]
    GuidelineEffective {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        employee_id: Option<EmployeeId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    /// Proposals of `propose_guideline` (B5), pending first, then the latest
    /// decisions; `status` filters. Apart from `proposal.list`.
    #[serde(rename = "guideline.proposals")]
    GuidelineProposals {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<aurelius_domain::proposal::ProposalStatus>,
    },
    /// Who a change of this guideline reaches from their next run (B6): employees,
    /// projects and conversations that start a new session.
    #[serde(rename = "guideline.affected")]
    GuidelineAffected { guideline_id: aurelius_domain::GuidelineId },
    /// The templates of the starter set (B7), for "Add starter set…".
    #[serde(rename = "guideline.starter_set")]
    GuidelineStarterSet,
    /// The guideline versions a run had in its system text (B6), kept for every
    /// run (the trace's "Guidelines: Rust v3, …"); also without trace mode.
    #[serde(rename = "run.guidelines")]
    RunGuidelines { run_id: RunId },
    /// The conversations of a project, most recently active first.
    #[serde(rename = "project_conversation.list")]
    ProjectConversationList { project_id: ProjectId },
    #[serde(rename = "approval.list")]
    ApprovalList,
    #[serde(rename = "connection.list")]
    ProviderList,
    /// Tasks, newest first, company-wide; every given filter applies.
    #[serde(rename = "task.list")]
    TaskList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        employee_id: Option<EmployeeId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<TaskListState>,
    },
    /// Human-readable timeline cards, newest first (keyset on `seq`).
    #[serde(rename = "timeline.list")]
    TimelineList(TimelineQuery),
    /// Memories, newest first: global always, plus the scopes asked for (a
    /// project, an employee). Nothing asked: global and every employee.
    #[serde(rename = "memory.list")]
    MemoryList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        employee_id: Option<EmployeeId>,
    },
    /// What goes into a context: an employee's (own, the project if given,
    /// global) or a project conversation's (project, global; no employee). At
    /// least one of the two.
    #[serde(rename = "memory.get_context")]
    MemoryGetContext {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        employee_id: Option<EmployeeId>,
    },
    #[serde(rename = "recurring_task.list")]
    RecurringTaskList { project_id: Option<ProjectId> },
    #[serde(rename = "agent_function.list")]
    AgentFunctionList { project_id: Option<ProjectId> },
    #[serde(rename = "function_call.list")]
    FunctionCallList { function_id: Option<aurelius_domain::AgentFunctionId>, limit: Option<u32> },
    #[serde(rename = "api_key.list")]
    ApiKeyList,
    /// The keys of outside apps that use `aurelius-mcp`, newest first (feature `app_keys`).
    #[serde(rename = "app_key.list")]
    AppKeyList,
    /// The context manifest of one run (context inspector).
    #[serde(rename = "run.get_context")]
    RunGetContext { run_id: RunId },
    /// A screenshot the agent took (the id is in its screenshot tool's output).
    #[serde(rename = "screenshot.get")]
    ScreenshotGet { id: String },
    /// A file kept with a message (see `MessageView::files`).
    #[serde(rename = "file.get")]
    FileGet { id: String },
    /// A file the user uploaded with a message, task, recurring task or idea (see
    /// `MessageFileView::id`), in parts of at most `UPLOAD_CHUNK_BYTES` from
    /// `offset` (none: 0). With `max_side` an image comes back whole as a JPEG
    /// that fits that many pixels (clamped to `UPLOAD_THUMB_MAX_SIDE`); other files are refused.
    #[serde(rename = "upload.get")]
    UploadGet {
        id: String,
        #[serde(default)]
        offset: Option<u64>,
        #[serde(default)]
        max_side: Option<u32>,
    },
    #[serde(rename = "idea.list")]
    IdeaList { project_id: ProjectId },
    /// The ideas of every project (each keeps its own project).
    #[serde(rename = "idea.list_all")]
    IdeaListAll {},
    /// A project's notes (summaries), pinned first, then the last edited; `query` searches the text.
    #[serde(rename = "note.list")]
    NoteList { project_id: ProjectId, #[serde(default)] query: Option<String> },
    /// One note with its Markdown body.
    #[serde(rename = "note.get")]
    NoteGet { note_id: aurelius_domain::NoteId },
    #[serde(rename = "share.list")]
    ShareList { project_id: ProjectId },
    /// Proposals of the whole company (colleagues and instructions): pending
    /// first, then the latest decisions.
    #[serde(rename = "proposal.list")]
    ProposalList,
    /// The versions of an employee's instructions, newest first (feature `instruction_proposals`).
    #[serde(rename = "employee.instruction_versions")]
    EmployeeInstructionVersions { employee_id: aurelius_domain::EmployeeId },
    #[serde(rename = "usage.get")]
    UsageGet,
    /// Usage per hour of one server-local day (any moment in it).
    #[serde(rename = "usage.get_hours")]
    UsageGetHours { day_start_ms: i64 },
    /// What a conversation or a task used, with the tasks given away from it apart
    /// (feature `run_usage`). Give one of the two; a task wins.
    #[serde(rename = "usage.get_totals")]
    UsageGetTotals {
        #[serde(default)]
        conversation_id: Option<ConversationId>,
        #[serde(default)]
        task_id: Option<aurelius_domain::TaskId>,
    },
    /// What one run sent to the runtime and what each model call cost; not found
    /// unless trace mode was on for it and it is among the latest (feature `run_trace`).
    #[serde(rename = "run.get_trace")]
    RunGetTrace { run_id: RunId },
    /// Whether runs keep a trace (feature `run_trace`).
    #[serde(rename = "trace.get_setting")]
    TraceGetSetting,
    /// Whether tasks may be tried in several ways first (feature `task_variants`).
    #[serde(rename = "variants.get_setting")]
    VariantsGetSetting,
    /// A task's variants and the judge's comparison (feature `task_variants`).
    #[serde(rename = "task.variants")]
    TaskVariants { task_id: aurelius_domain::TaskId },
    /// Update status; `check` asks GitHub for the latest release first.
    #[serde(rename = "update.get")]
    UpdateGet { check: bool },
    #[serde(rename = "power.get")]
    PowerGet,
    #[serde(rename = "load.get")]
    LoadGet,
    #[serde(rename = "browser.get")]
    BrowserGet,
    /// Whether runs may publish Claude Code artifacts (pages on claude.ai).
    #[serde(rename = "artifact.get_setting")]
    ArtifactGetSetting,
    /// Whether management chooses its own task mode (feature `management_chooses_mode`).
    #[serde(rename = "management_mode.get_setting")]
    ManagementModeGetSetting,
    /// Whether the safe read-only tools pass without a question (the auto-allow list).
    #[serde(rename = "auto_allow.get_setting")]
    AutoAllowGetSetting,
    /// Artifacts runs published, newest first: every given filter applies, none lists all.
    #[serde(rename = "artifact.list")]
    ArtifactList { conversation_id: Option<ConversationId>, task_id: Option<aurelius_domain::TaskId>, project_id: Option<ProjectId> },
    /// The file the server kept of one publish (`version`, else the newest kept),
    /// so a client can show the page without a claude.ai login. Not found for an
    /// unknown URL; `snapshot` is none for pages published before servers kept them.
    #[serde(rename = "artifact.get_content")]
    ArtifactGetContent { url: String, version: Option<u32> },
    /// The user-scope MCP servers of Claude Code and which of them every run uses.
    #[serde(rename = "mcp.list")]
    McpList,
    /// A login started with `mcp.login_start`; kept for a while after it ends.
    #[serde(rename = "mcp.login_get")]
    McpLoginGet { login_id: String },
    /// The MCP tools the user always allows (answered an approval with 'Always allow').
    #[serde(rename = "mcp_always_allow.list")]
    McpAlwaysAllowList,
    /// Claude Code plugins installed on the server's computer, with the employees that use each.
    #[serde(rename = "plugin.list")]
    PluginList,
    /// Plugins offered by the marketplaces already added.
    #[serde(rename = "plugin.list_available")]
    PluginListAvailable,
    /// The plugins one employee has enabled.
    #[serde(rename = "plugin.get_for_employee")]
    PluginGetForEmployee { employee_id: EmployeeId },
    /// The plugins a project has enabled (its project assistant loads them).
    #[serde(rename = "plugin.get_for_project")]
    PluginGetForProject { project_id: ProjectId },
    #[serde(rename = "subscription.get")]
    SubscriptionGet { refresh: bool },
    #[serde(rename = "backup.list")]
    BackupList,
    #[serde(rename = "notification.list")]
    NotificationList,
    /// Branch and uncommitted change of the folder a conversation works in; none outside a Git repository.
    #[serde(rename = "git.get_status")]
    GitGetStatus { conversation_id: ConversationId },
    #[serde(rename = "git.get_diff")]
    GitGetDiff { conversation_id: ConversationId },
    /// The Markdown files in a work folder, for the in-app viewer.
    #[serde(rename = "markdown.list")]
    MarkdownList { scope: FileScope },
    /// One Markdown file as UTF-8 text. `path` is relative to the work folder.
    #[serde(rename = "markdown.get")]
    MarkdownGet { scope: FileScope, path: String },
    /// The subagents running in the answers of one conversation.
    #[serde(rename = "subagent.list")]
    SubagentList { conversation_id: ConversationId },
    /// One tool call of an answer whole: the input and output that
    /// `conversation.get` and live updates shorten (feature `activity_preview`).
    #[serde(rename = "activity.get")]
    ActivityGet { message_id: MessageId, call_id: String },
    #[serde(rename = "remote.get_status")]
    RemoteGetStatus,
    /// Folders on the server: none lists the starting points.
    #[serde(rename = "fs.list_dir")]
    FsListDir { path: Option<String> },
    /// Where Claude Code is installed on the server.
    #[serde(rename = "connection.detect_runtime")]
    ProviderDetectRuntime,

    /// Replay events after `after_seq`, then stream live events (and load/power pushes).
    #[serde(rename = "event.subscribe")]
    EventSubscribe { after_seq: u64 },
    /// A shell in the folder a conversation works in. Attaches to the running terminal
    /// unless `fresh`; this session then gets its output as `ServerMessage::TerminalOutput`.
    /// With `terminal_id` it attaches to exactly that terminal (its scrollback follows);
    /// with `additional` it starts another terminal beside the running ones
    /// (feature `terminal.multi`; at most 4 per conversation, 8 per server).
    #[serde(rename = "terminal.open")]
    TerminalOpen {
        conversation_id: ConversationId,
        cols: u16,
        rows: u16,
        fresh: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal_id: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        additional: bool,
    },
    /// This session stops getting a terminal's output; the shell keeps running and
    /// keeps its scrollback (feature `terminal.multi`).
    #[serde(rename = "terminal.detach")]
    TerminalDetach { terminal_id: String },
    /// The terminals of a conversation, oldest first (feature `terminal.multi`).
    #[serde(rename = "terminal.list")]
    TerminalList { conversation_id: ConversationId },
    #[serde(rename = "terminal.input")]
    TerminalInput { terminal_id: String, data_base64: String },
    #[serde(rename = "terminal.resize")]
    TerminalResize { terminal_id: String, cols: u16, rows: u16 },
    #[serde(rename = "terminal.close")]
    TerminalClose { terminal_id: String },
}

/// Which tasks `task.list` gives by state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskListState {
    /// Not ended: draft, queued, running, waiting, review.
    Open,
    /// Waiting for the user (input, an approval) or in review.
    NeedsYou,
    /// Succeeded, failed or cancelled.
    Ended,
}

impl TaskListState {
    pub fn matches(self, status: aurelius_domain::task::TaskStatus, waiting_reason: Option<&str>) -> bool {
        use aurelius_domain::task::TaskStatus::*;
        match self {
            TaskListState::Open => !matches!(status, Succeeded | Failed | Cancelled),
            TaskListState::NeedsYou => status == Review || (status == Waiting && matches!(waiting_reason, Some("input") | Some("approval"))),
            TaskListState::Ended => matches!(status, Succeeded | Failed | Cancelled),
        }
    }
}

impl Method {
    /// Whether `name` is a read or session method of this server (taken from the schema once).
    pub fn knows(name: &str) -> bool {
        static NAMES: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
            let schema = serde_json::to_value(schemars::schema_for!(Method)).unwrap_or_default();
            schema["oneOf"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v["properties"]["method"]["const"].as_str().map(str::to_owned))
                .collect()
        });
        NAMES.iter().any(|n| n == name)
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HelloParams {
    pub client_name: String,
    pub client_version: String,
    /// An app key's secret (`amk_...`, feature `app_keys`): the session then acts
    /// for that app, with its scopes only (`HelloResult::app`). A wrong or
    /// revoked key is refused. A server without the feature ignores it, so a
    /// client must check `HelloResult::app` before it goes on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_key: Option<String>,
    /// What this client can take that older clients cannot (`client_feature`).
    /// Servers without such a feature ignore the name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepts: Vec<String>,
    /// The session this device got from `user.login` (or the first
    /// `user.create`) for this server: valid, it makes its board member this
    /// connection's (`HelloResult::user`); unknown, revoked or expired, the
    /// hello still works without a board member and the client shows its
    /// sign-in screen. Ignored for apps. Never printed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
}

/// `user.login`. The password is never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LoginParams {
    pub username: String,
    pub password: String,
    /// How the device shows in `session.list` ("Desktop · dev-laptop").
    pub device_name: String,
}

impl std::fmt::Debug for LoginParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginParams").field("username", &self.username).field("password", &"<redacted>").field("device_name", &self.device_name).finish()
    }
}

/// `vault.key_export`. The password is never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KeyExportParams {
    /// The signed-in board member's own password, asked again.
    pub password: String,
}

impl std::fmt::Debug for KeyExportParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyExportParams").field("password", &"<redacted>").finish()
    }
}

/// `claude_login.input`. The text (a one-time code) is never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClaudeLoginInputParams {
    pub link_id: String,
    pub text: String,
}

impl std::fmt::Debug for ClaudeLoginInputParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeLoginInputParams").field("link_id", &self.link_id).field("text", &"<redacted>").finish()
    }
}

/// What `HelloParams::accepts` names.
pub mod client_feature {
    /// Live updates may carry only the tool calls that are new or changed
    /// (`StreamDelta::activity_changes`); the client merges them by `call_id`.
    pub const ACTIVITY_CHANGES: &str = "activity_changes";
    /// The client shows the Team chat's channels (feature `team_channels`):
    /// only such a connection gets the live events of project channels, so an
    /// older client never shows them in its one Team chat.
    pub const TEAM_CHANNELS: &str = "team_channels";
}

impl std::fmt::Debug for HelloParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelloParams")
            .field("client_name", &self.client_name)
            .field("client_version", &self.client_version)
            .field("app_key", &self.app_key.as_ref().map(|_| "<hidden>"))
            .field("accepts", &self.accepts)
            .field("session_token", &self.session_token.as_ref().map(|_| "<hidden>"))
            .finish()
    }
}
