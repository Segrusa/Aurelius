//! The envelope of protocol 3.0: one JSON object per message. Frames on local IPC
//! are a 4-byte big-endian length plus this JSON; the gateway sends it as one
//! WebSocket text frame.

use crate::events::Event;
use crate::method::Method;
use crate::remote::RemoteStatus;
use crate::views::*;
use crate::write::{Receipt, WriteMethod};
use aurelius_domain::views::*;
use aurelius_domain::{AppError, ClientId, CommandId, ErrorCode, HostId};
use schemars::JsonSchema;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The protocol major. The handshake checks only this; a mismatch is a hard
/// "update the app or the server" (`incompatible_protocol`, "Update needed").
/// There are no minors: additions within a major are announced in
/// `HelloResult::features` (see `FEATURES`). 3: the company (employees, one
/// org chart). 4: departments are gone (methods, fields and commands).
/// 5: sign-in and sessions (`user.login`, `HelloParams::session_token`; `user.use`
/// and the remembered `user_id` are gone) and the gate: nothing without a session
/// but the hello, `user.whoami`, `user.login`, `user.list` and the first `user.create`.
pub const PROTOCOL_MAJOR: u16 = 5;

/// The message of `incompatible_protocol`: what the user must do.
pub fn incompatible_message(client: u16) -> String {
    if client < PROTOCOL_MAJOR {
        format!("Update needed: this app speaks protocol {client}, the server {PROTOCOL_MAJOR}. Update the app.")
    } else {
        format!("Update needed: this app speaks protocol {client}, the server {PROTOCOL_MAJOR}. Update the server.")
    }
}

/// `feature::TERMINAL_MULTI` under its earlier name.
pub const FEATURE_TERMINAL_MULTI: &str = feature::TERMINAL_MULTI;

/// Client → daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// The client's protocol major; must equal the server's.
    pub protocol: u16,
    pub request_id: u64,
    /// Required for writes: the same key with the same request is answered from
    /// the receipt; the same key with another request is a conflict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<CommandId>,
    /// Optimistic concurrency for writes that change a versioned entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u64>,
    #[serde(flatten)]
    pub call: Call,
}

impl Request {
    pub fn read(request_id: u64, method: Method) -> Self {
        Self { protocol: PROTOCOL_MAJOR, request_id, idempotency_key: None, expected_version: None, call: Call::Read(method) }
    }

    pub fn write(request_id: u64, key: CommandId, write: WriteMethod) -> Self {
        Self { protocol: PROTOCOL_MAJOR, request_id, idempotency_key: Some(key), expected_version: None, call: Call::Write(write) }
    }
}

/// A read or session method, or a write. On the wire both are `{ method, params }`;
/// the method name says which.
#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    Read(Method),
    Write(WriteMethod),
}

impl Serialize for Call {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Call::Read(method) => method.serialize(serializer),
            Call::Write(write) => write.serialize(serializer),
        }
    }
}

/// The answer to a message that is not a valid `Request`. The request id is read
/// leniently so the client can match the error; a method this server does not
/// know is `unknown_method` (how newer clients detect an older server), anything
/// else `invalid_request`. The connection stays open either way.
pub fn reject_unparsed(value: Option<&serde_json::Value>, err: &dyn std::fmt::Display) -> ServerMessage {
    let request_id = value.and_then(|v| v.get("request_id")).and_then(|id| id.as_u64()).unwrap_or(0);
    let method = value.and_then(|v| v.get("method")).and_then(|m| m.as_str());
    let error = match method {
        Some(name) if !Method::knows(name) && !WriteMethod::knows(name) => {
            AppError::new(ErrorCode::UnknownMethod, format!("This server does not know the method {name}."))
        }
        // serde quotes values it could not take: never for a method with a secret in it (SEC0 E5).
        Some(name) if quotes_no_params(name) => AppError::new(ErrorCode::InvalidRequest, format!("The request for {name} is not valid.")),
        _ => AppError::new(ErrorCode::InvalidRequest, err.to_string()),
    };
    ServerMessage::Response { request_id, outcome: Outcome::Error(error) }
}

/// Methods whose params may hold a value, a password or a code: an error never
/// repeats what serde said about them.
pub fn quotes_no_params(name: &str) -> bool {
    name.starts_with("vault.") || matches!(name, "user.login" | "user.create" | "user.set_password" | "claude_login.input" | "session.hello")
}

impl<'de> Deserialize<'de> for Call {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let name = value.get("method").and_then(|m| m.as_str()).unwrap_or("").to_owned();
        if WriteMethod::knows(&name) {
            from_method_value(value).map(Call::Write).map_err(D::Error::custom)
        } else {
            from_method_value(value).map(Call::Read).map_err(|e| D::Error::custom(format!("{name}: {e}")))
        }
    }
}

/// A `{ method, params }` object into a method enum. Params whose fields are
/// all optional may be left out or `null` (section 4 of the protocol): a method
/// that needs an object there is tried again with `{}`. The other way round, a
/// method without params also takes `"params": {}`.
pub(crate) fn from_method_value<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, serde_json::Error> {
    let fallback = match value.get("params") {
        None | Some(serde_json::Value::Null) => Some(serde_json::json!({})),
        Some(serde_json::Value::Object(o)) if o.is_empty() => None,
        Some(_) => return serde_json::from_value(value),
    };
    serde_json::from_value(value.clone()).or_else(|err| {
        let mut retry = value;
        let Some(object) = retry.as_object_mut() else { return Err(err) };
        match fallback {
            Some(params) => {
                object.insert("params".into(), params);
                serde_json::from_value(retry)
            }
            // A method with params says best what is missing in them.
            None => {
                object.remove("params");
                serde_json::from_value(retry).map_err(|_| err)
            }
        }
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HelloResult {
    pub host_id: HostId,
    pub daemon_version: String,
    pub protocol: u16,
    /// `dev` or `production`; a client never talks to the other environment.
    pub environment: String,
    /// The server's computer name.
    pub host_name: String,
    /// The server's OS: `windows`, `macos` or `linux`.
    pub os: String,
    /// This connection came in over the network.
    pub remote: bool,
    /// The paired device behind a remote connection.
    pub client_id: Option<ClientId>,
    pub capabilities: Vec<aurelius_domain::remote::Capability>,
    /// Additions to protocol 3.0 this server has (see `FEATURES`); a client hides
    /// what is missing, since an unknown method is not a clean error. Older servers leave it out.
    #[serde(default)]
    pub features: Vec<String>,
    /// The app this session acts for when it authenticated with an app key
    /// (`HelloParams::app_key`, or the key on the gateway); feature `app_keys`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<aurelius_domain::app_key::AppSession>,
    /// The server has at least one board member; false: the onboarding
    /// ("Create the first board member") is all a client shows.
    #[serde(default)]
    pub board_members_exist: bool,
    /// The board member of this connection's session (`HelloParams::session_token`
    /// when it is valid); none: the client shows its sign-in screen (or the
    /// onboarding without board members).
    #[serde(default)]
    pub user: Option<aurelius_domain::user::UserView>,
    /// The session's board member must change a temporary password first.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub must_change_password: bool,
    /// The session's own Claude login; none without a session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_login: Option<aurelius_domain::user::ClaudeLoginView>,
}

/// What `HelloResult::features` names.
pub mod feature {
    /// Uploaded files carry an id (`MessageFileView::id`) and `upload.get` fetches
    /// them, whole or as a thumbnail.
    pub const UPLOAD_FETCH: &str = "upload_fetch";
    /// `note.*` methods and `note.created/updated/deleted` events.
    pub const NOTES: &str = "notes";
    /// Plugins per project (`plugin.get_for_project`, `plugin.set_for_project`,
    /// `PluginInfo::projects`), loaded by the project assistant.
    pub const PROJECT_PLUGINS: &str = "project_plugins";
    /// A project's plugins apply to every employee run in the project's tasks as
    /// well, on top of the employee's own list.
    pub const PROJECT_PLUGINS_EMPLOYEES: &str = "project_plugins_employees";
    /// `terminal.detach`, `terminal.list` and `terminal.open` with `terminal_id` /
    /// `additional` (several terminals per conversation).
    pub const TERMINAL_MULTI: &str = "terminal.multi";
    /// `project.set_description` (`set_project_description`): change or clear a
    /// project's description after creation.
    pub const PROJECT_DESCRIPTION: &str = "project_description";
    /// Keys for outside apps (`aurelius-mcp`): `app_key.list`, `app_key.create`,
    /// `app_key.revoke`, `HelloParams::app_key` / `HelloResult::app`, the scopes
    /// of `crate::app`, the gateway's `/v1/mcp`, and `TaskView::created_by_app`.
    pub const APP_KEYS: &str = "app_keys";
    /// Ideas have files: `IdeaView::attachments` (fetched with `upload.get`),
    /// `AddIdea::files`, `UpdateIdea::add_files` / `remove_files`, and
    /// `idea.delete`. Starting an idea gives the task copies of them.
    pub const IDEA_ATTACHMENTS: &str = "idea_attachments";
    /// Tokens per answer (`MessageView::usage`), `usage.get_totals` per conversation
    /// or task, and cache writes apart (`cache_creation_tokens` in the usage views).
    pub const RUN_USAGE: &str = "run_usage";
    /// Trace mode: `trace.get_setting`, `trace.set_enabled` and `run.get_trace`.
    pub const RUN_TRACE: &str = "run_trace";
    /// Lessons learned from tasks (Harness² A): `MemoryView::lesson` (when it
    /// applies, evidence S1..S8, times confirmed, tasks) on episodic memories,
    /// event `memory.reinforced`, and failed tasks are extracted too.
    pub const TASK_LESSONS: &str = "task_lessons";
    /// Task variants (Harness² C): `TaskRun::variants`, `TaskView::variant_phase`,
    /// `task.variants`, `task.choose_variant`, `variants.get_setting` /
    /// `variants.set_enabled` and the `task.variant*` events.
    pub const TASK_VARIANTS: &str = "task_variants";
    /// Instruction proposals (Harness² B): `ProposalView::kind` / `target_employee_id` /
    /// `base_version` / `current_instructions` / `current_version` / `evidence`,
    /// `DecideProposal::instructions`, `employee.restore_instructions`,
    /// `employee.instruction_versions` and event `employee.instructions_proposed`.
    pub const INSTRUCTION_PROPOSALS: &str = "instruction_proposals";
    /// A conversation's runtime session whose context grew past the server's
    /// threshold is compacted before its next turn: a `compacted` update ("Context
    /// compacted (≈X → Y tokens)") in the conversation, and a `session` part in the run's trace.
    pub const SESSION_COMPACTION: &str = "session_compaction";
    /// Short tool use: `conversation.get` and live updates shorten each tool
    /// call (`ActivityView::truncated` / `output_bytes`), `activity.get` returns
    /// one whole, and a client that sends `HelloParams::accepts`
    /// `activity_changes` gets only new or changed calls in live updates.
    pub const ACTIVITY_PREVIEW: &str = "activity_preview";
    /// The management channel for outside apps: `AppAudience` (`CreateAppKey::audience`,
    /// `AppKeyView::audience`, `AppSession::audience`) and the MCP tools `ask_management`,
    /// `list_open_questions` and `answer_question`. The server enforces the audience.
    pub const MANAGEMENT_CHANNEL: &str = "management_channel";
    /// Webhooks per app key: `CreateAppKey::webhook_url`, `AppKeyView::webhook_url`,
    /// `AppKeyCreated::webhook_secret` (shown once) and the signed events
    /// `manager.replied`, `question.for_you`, `task.finished`, `task.waiting`.
    pub const WEBHOOKS: &str = "webhooks";
    /// Quick replies: an answer can end with a `suggest_replies` tool use
    /// (`ActivityView::tool` `mcp__aurelius__suggest_replies`, input `question` and
    /// `options`); clients show the options as buttons that send one as the user's
    /// message. No new methods; older clients show it as a plain tool step.
    pub const QUICK_REPLIES: &str = "quick_replies";
    /// While a run compacts the conversation's context before its turn, its `RunProgress`
    /// (stream updates and `JobSummary::live`) has `compacting: true`; older servers
    /// never set it and clients show plain work.
    pub const COMPACT_STATUS: &str = "compact_status";
    /// Management (the CEO and those who report to them) chooses its own task mode, Auto or
    /// Plan, when the user picked none: `management_mode.get_setting` /
    /// `management_mode.set_enabled` (`ManagementModeSetting`), the `mode_chosen` task
    /// update ("<name> chose Auto: <reason>") and the event `task.mode_chosen`.
    pub const MANAGEMENT_CHOOSES_MODE: &str = "management_chooses_mode";
    /// Auto-allow of the safe read-only tools (Read, Grep, Glob, WebSearch, list_windows) in
    /// every mode but Plan: `auto_allow.get_setting` / `auto_allow.set_enabled`
    /// (`AutoAllowSetting`, on by default, Approve right).
    pub const AUTO_ALLOW: &str = "auto_allow";
    /// One discussion per ticket (protocol 4, additive): `TicketView::conversation_id`
    /// (read with `conversation.get`), the write `ticket.post`, `MessageView::author_user`
    /// and `MessageView::system` (system lines, never a new `MessageRole`),
    /// `ticket.create { from_message }` and `ticket.retry_summary`. A discussion
    /// of a ticket on Done is read-only.
    pub const TICKET_DISCUSSION: &str = "ticket_discussion";
    /// Channels in the Team chat: `channel.list`, `group_chat.get { channel }`,
    /// `group_chat.send { channel }`. Live events of project channels go only to
    /// connections whose `HelloParams::accepts` names `team_channels`.
    pub const TEAM_CHANNELS: &str = "team_channels";
    /// The Testing column (protocol 5, additive): `ColumnKind::Testing` between
    /// Plan review and Review, the company tester (`company.update { tester }`,
    /// `CompanyView::tester_employee_id`) and the event `ticket.tested`. A client
    /// shows Testing and the tester picker only when the server has this.
    pub const BOARD_TESTING: &str = "board_testing";
    /// The vault (protocol 5, additive): `vault.list`, `vault.reveal`,
    /// `vault.audit`, `vault.key_export`, the writes `vault.create`,
    /// `vault.update`, `vault.archive`, the events `vault.item_created` /
    /// `_updated` / `_archived` (never a value), the error codes
    /// `vault_kind_locked`, `vault_undecryptable` and `looks_like_secret`
    /// (`AppError::matched_items`), and `confirm_secret` on `employee.send_message`,
    /// `ticket.post` and `group_chat.send` (B10). Board members only; never apps.
    pub const VAULT: &str = "vault";
    /// Guidelines (protocol 5, additive; docs/product/guidelines): the reads
    /// `guideline.list`, `.get`, `.versions`, `.effective`, `.proposals`,
    /// `.affected`, `.starter_set` and `run.guidelines`; the writes
    /// `guideline.create`, `.update`, `.archive`, `.restore`, `.restore_version`,
    /// `.link`, `.unlink`, `.decide_proposal`, `.add_starter_set` (board members
    /// only; never apps); the `guideline.*` events; the error codes
    /// `guideline_too_large` (`AppError::too_large`) and `guideline_title_taken`;
    /// context items with section `guideline`.
    pub const GUIDELINES: &str = "guidelines";
    /// The server's name (protocol 5, additive; docs/product/servernaam): the
    /// read `server.get_name`, the write `server.set_name` (board members only;
    /// never apps; remote: owner) and the event `server.renamed`, only to
    /// connections whose `HelloParams::accepts` names `server_name`. The hello's
    /// `host_name`, the pairing text and the pairing poll give the chosen name.
    pub const SERVER_NAME: &str = "server_name";
    /// A channel per ticket (protocol 5, additive; AUR-CHAT-001): the ticket's
    /// discussion is its channel, `#<number>-<slug>` (`ticket_channel_name`).
    /// `channel.list` names ticket channels only to connections whose
    /// `HelloParams::accepts` names `ticket_channels` (any other sees the list of
    /// before), with `ChannelView::kind`, `ticket_id`, `ticket_number`,
    /// `ticket_column`, `member`, `member_count` and `read_through`; a closed
    /// ticket's channel is `archived`. The read `channel.search { query }`; the
    /// writes `group_chat.send { channel }` to a ticket's channel; the error code
    /// `channel_archived` (only to such connections); `SystemLine::action` and
    /// `SystemLine::channel_id`, with one line in General when a ticket opens and
    /// one when it closes.
    pub const TICKET_CHANNELS: &str = "ticket_channels";
    /// Threads in channels (phase 2, defined but not advertised yet):
    /// `thread_parent` on `group_chat.send` / `ticket.post`, and
    /// `MessageView::thread_parent`, `thread_reply_count`, `thread_last_reply_ms`.
    pub const CHAT_THREADS: &str = "chat_threads";
    /// Reactions on messages (phase 2, defined but not advertised yet): the
    /// write `message.react` and `MessageView::reactions`.
    pub const CHAT_REACTIONS: &str = "chat_reactions";
    /// `ticket.update { add_files }` (protocol 5, additive): files added to an
    /// existing ticket, as `files` on `ticket.create`. A main task that still
    /// works gets them too, as an update in its conversation that names them
    /// (with a run if it is Running), or through its brief if it has not opened
    /// its conversation yet. It gives `ticket.updated`, and the timeline says who
    /// added how many files, with their names only.
    pub const TICKET_ADD_FILES: &str = "ticket_add_files";
}

/// Every feature this build has.
pub const FEATURES: &[&str] = &[
    feature::UPLOAD_FETCH,
    feature::NOTES,
    feature::PROJECT_PLUGINS,
    feature::PROJECT_PLUGINS_EMPLOYEES,
    feature::TERMINAL_MULTI,
    feature::PROJECT_DESCRIPTION,
    feature::APP_KEYS,
    feature::IDEA_ATTACHMENTS,
    feature::RUN_USAGE,
    feature::RUN_TRACE,
    feature::TASK_LESSONS,
    feature::INSTRUCTION_PROPOSALS,
    feature::TASK_VARIANTS,
    feature::SESSION_COMPACTION,
    feature::ACTIVITY_PREVIEW,
    feature::MANAGEMENT_CHANNEL,
    feature::WEBHOOKS,
    feature::QUICK_REPLIES,
    feature::COMPACT_STATUS,
    feature::MANAGEMENT_CHOOSES_MODE,
    feature::AUTO_ALLOW,
    feature::TICKET_DISCUSSION,
    feature::TEAM_CHANNELS,
    feature::TICKET_CHANNELS,
    feature::VAULT,
    feature::GUIDELINES,
    feature::BOARD_TESTING,
    feature::SERVER_NAME,
    feature::TICKET_ADD_FILES,
];

/// Daemon → client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Response {
        request_id: u64,
        #[serde(flatten)]
        outcome: Outcome,
    },
    /// A committed event (durable); replayed from the log after `event.subscribe`.
    Event(Event),
    /// Ephemeral streamed text; after a reconnect the client refetches instead.
    Stream(StreamDelta),
    /// Remote clients only: streamed text as an append.
    StreamAppend(crate::remote::StreamAppend),
    /// Built-in terminal: only to sessions that opened it.
    TerminalOutput(TerminalOutput),
    TerminalExit(TerminalExit),
    /// The machine's load or the admission changed. While runs wait, at most every 2 s.
    LoadChanged(LoadSnapshot),
    /// The pending shutdown changed.
    PowerChanged(PowerState),
    /// What `claude auth login` asks, only to the connection that started the
    /// link (`claude_login.link`); never an event, never logged.
    ClaudeLoginPrompt(ClaudeLoginPrompt),
    /// The session of this connection ended (`user.logout`, `session.revoke`, a
    /// new password, `user.disable`, expiry); the server closes the connection
    /// right after. The client shows "You were signed out on this device." and
    /// its sign-in screen (a logout of its own needs no message).
    SignedOut { reason: aurelius_domain::user::SignOutReason },
}

/// `ServerMessage::ClaudeLoginPrompt`: the sign-in link to open and/or the
/// question for the code, as `claude` printed them.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClaudeLoginPrompt {
    pub link_id: String,
    /// The Anthropic sign-in page to open (`https://claude.com/…/oauth/authorize?…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Claude waits for the code: send it with `claude_login.input`.
    pub asks_code: bool,
    /// The link ended without a login: a fixed text of Aurelius (never Claude's
    /// output), shown with "Try again". A link that worked ends with the event
    /// `claude_login.updated` instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Hides the link (it carries the sign-in's state): never in a log (C6).
impl std::fmt::Debug for ClaudeLoginPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeLoginPrompt")
            .field("link_id", &self.link_id)
            .field("url", &self.url.as_ref().map(|_| "<redacted>"))
            .field("asks_code", &self.asks_code)
            .field("error", &self.error)
            .finish()
    }
}

#[cfg(test)]
mod claude_login_prompt_tests {
    #[test]
    fn the_sign_in_link_and_the_code_never_show_in_debug_output() {
        let prompt = super::ClaudeLoginPrompt { link_id: "l".into(), url: Some("https://claude.com/cai/oauth/authorize?state=SECRET-STATE".into()), asks_code: true, error: None };
        assert!(!format!("{prompt:?}").contains("SECRET-STATE"));
        let input = crate::ClaudeLoginInputParams { link_id: "l".into(), text: "SECRET-CODE".into() };
        assert!(!format!("{input:?}").contains("SECRET-CODE"));
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok(ResponseBody),
    Error(AppError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ResponseBody {
    Hello(HelloResult),
    Health(HealthResult),
    /// The answer to every write.
    Receipt(Receipt),
    Projects(ProjectsSnapshot),
    Jobs(JobsSnapshot),
    Messages(MessagesSnapshot),
    Employees(EmployeesSnapshot),
    Company(CompanySnapshot),
    Board(aurelius_domain::ticket::BoardView),
    Ticket(aurelius_domain::ticket::TicketDetail),
    Tickets(aurelius_domain::ticket::TicketsSnapshot),
    Users(aurelius_domain::user::UsersSnapshot),
    Channels(aurelius_domain::views::ChannelsSnapshot),
    WhoAmI(aurelius_domain::user::WhoAmI),
    /// `user.login`: the session token, once.
    Login(aurelius_domain::user::LoginResult),
    Sessions(aurelius_domain::user::SessionsSnapshot),
    ClaudeLogin(aurelius_domain::user::ClaudeLoginView),
    /// `vault.list`.
    Vault(aurelius_domain::vault::VaultList),
    /// `vault.reveal`: never printed.
    VaultValue(aurelius_domain::vault::VaultValue),
    /// `vault.audit`.
    VaultAudit(aurelius_domain::vault::VaultAuditPage),
    /// `vault.key_export`: never printed.
    VaultKey(aurelius_domain::vault::VaultKeyExport),
    /// `guideline.list`.
    Guidelines(aurelius_domain::guideline::GuidelinesSnapshot),
    /// `guideline.get`.
    Guideline(aurelius_domain::guideline::GuidelineDetail),
    /// `guideline.versions`.
    GuidelineVersions(aurelius_domain::guideline::GuidelineVersions),
    /// `guideline.effective`.
    EffectiveGuidelines(aurelius_domain::guideline::EffectiveGuidelines),
    /// `guideline.proposals`.
    GuidelineProposals(aurelius_domain::guideline::GuidelineProposals),
    /// `guideline.affected`.
    GuidelineAffected(aurelius_domain::guideline::GuidelineAffected),
    /// `guideline.starter_set`.
    StarterSet(aurelius_domain::guideline::StarterSet),
    /// `run.guidelines`.
    RunGuidelines(aurelius_domain::guideline::RunGuidelines),
    /// `server.get_name`.
    ServerName(aurelius_domain::server::ServerNameView),
    ProjectConversations(ProjectConversationsSnapshot),
    Approvals(ApprovalsSnapshot),
    Connections(ProvidersSnapshot),
    Tasks(TasksSnapshot),
    Timeline(TimelinePage),
    Memory(MemorySnapshot),
    MemoryContext(MemoryContextView),
    MarkdownFiles(MarkdownFileList),
    MarkdownFile(MarkdownFileContent),
    Usage(UsageSnapshot),
    UsageHours(UsageHours),
    UsageTotals(UsageTotals),
    RunTrace(RunTrace),
    TraceSetting(TraceSetting),
    VariantsSetting(VariantsSetting),
    TaskVariants(TaskVariantsView),
    Load(LoadSnapshot),
    Browser(BrowserSetting),
    ArtifactSetting(ArtifactSetting),
    ManagementModeSetting(ManagementModeSetting),
    AutoAllowSetting(AutoAllowSetting),
    Artifacts(ArtifactsSnapshot),
    ArtifactContent(ArtifactContent),
    McpServers(McpServers),
    McpLogin(McpLogin),
    McpAlwaysAllow(McpAlwaysAllow),
    Plugins(PluginsSnapshot),
    PluginCatalog(PluginCatalog),
    EmployeePlugins(EmployeePlugins),
    ProjectPlugins(ProjectPlugins),
    Power(PowerState),
    Subscription(SubscriptionUsage),
    Update(UpdateStatus),
    Backups(BackupsSnapshot),
    Shares(Vec<ShareView>),
    Context(ContextView),
    /// A screenshot the agent took, as JPEG.
    Screenshot { id: String, jpeg_base64: String },
    /// A file kept with a message.
    File { id: String, name: String, media_type: String, data_base64: String },
    /// A file the user uploaded (a part of it, or its thumbnail).
    Upload(UploadChunk),
    Ideas(IdeasSnapshot),
    AllIdeas(aurelius_domain::views::AllIdeasSnapshot),
    Notes(aurelius_domain::views::NotesSnapshot),
    Note(aurelius_domain::views::NoteView),
    Proposals(ProposalsSnapshot),
    EmployeeInstructionVersions(EmployeeInstructionVersionsSnapshot),
    TaskSchedules(TaskSchedulesSnapshot),
    AgentFunctions(AgentFunctionsSnapshot),
    FunctionCalls(FunctionCallsSnapshot),
    ApiKeys(Vec<ApiKeyView>),
    AppKeys(Vec<aurelius_domain::app_key::AppKeyView>),
    Notifications(NotificationsSnapshot),
    Subscribed { replayed_through_seq: u64 },
    Remote(RemoteStatus),
    DirListing(DirListing),
    /// `None`: Claude Code was not found on the server.
    RuntimeDetected { claude_code: Option<String> },
    GitStatus(Option<GitStatusView>),
    GitDiff(Option<GitDiffView>),
    Subagents(Vec<AgentView>),
    /// `activity.get`: one tool call whole.
    Activity(ActivityView),
    Terminal(TerminalView),
    Terminals(Vec<TerminalSummary>),
    /// Input, resize, detach or close of a terminal was handled.
    TerminalDone,
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_domain::project::CreateProject;

    fn write_request(key: CommandId) -> Request {
        Request::write(7, key, WriteMethod::Domain(aurelius_domain::Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None })))
    }

    #[test]
    fn a_write_request_is_one_flat_object() {
        let key = CommandId::new();
        let request = write_request(key);
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!((json["protocol"].as_u64(), json["request_id"].as_u64()), (Some(5), Some(7)));
        assert_eq!(json["idempotency_key"], serde_json::json!(key));
        assert_eq!(json["method"], "project.create");
        assert_eq!(json["params"]["name"], "Atlas");
        assert!(json.get("expected_version").is_none());
        assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request);
    }

    #[test]
    fn reads_and_unit_methods_roundtrip() {
        for method in [Method::ServerGetHealth, Method::ProjectList, Method::EmployeeList, Method::CompanyGet, Method::LoadGet] {
            let request = Request::read(1, method);
            let text = serde_json::to_string(&request).unwrap();
            assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request, "{text}");
        }
        let json = serde_json::to_value(Request::read(1, Method::TaskList { project_id: None, employee_id: None, state: None })).unwrap();
        assert_eq!(json["method"], "task.list");
        assert_eq!(json["params"], serde_json::json!({}));
        // Every filter may be left out.
        let parsed: Request = serde_json::from_value(serde_json::json!({ "protocol": 3, "request_id": 2, "method": "task.list", "params": { "state": "needs_you" } })).unwrap();
        assert_eq!(parsed.call, Call::Read(Method::TaskList { project_id: None, employee_id: None, state: Some(crate::method::TaskListState::NeedsYou) }));
        // A unit method also takes an empty params object (or null).
        for name in ["employee.list", "proposal.list", "company.get"] {
            for params in [serde_json::json!({}), serde_json::Value::Null] {
                let parsed: Request = serde_json::from_value(serde_json::json!({ "protocol": 4, "request_id": 4, "method": name, "params": params })).unwrap();
                assert_eq!(serde_json::to_value(&parsed).unwrap()["method"], name);
            }
        }
        let err = serde_json::from_value::<Request>(serde_json::json!({ "protocol": 4, "request_id": 5, "method": "employee.list", "params": { "x": 1 } }));
        assert!(err.is_err(), "a unit method takes no params");
        let chat = serde_json::to_value(Request::read(3, Method::EmployeeChat { employee_id: aurelius_domain::EmployeeId::new() })).unwrap();
        assert_eq!(chat["method"], "employee.chat");
    }

    #[test]
    fn the_variants_methods_roundtrip_under_their_names() {
        use crate::write::HostWrite;
        use aurelius_domain::{TaskId, variants::ChooseTaskVariant};
        let task_id = TaskId::new();
        let cases = [
            (Request::read(1, Method::TaskVariants { task_id }), "task.variants"),
            (Request::read(2, Method::VariantsGetSetting), "variants.get_setting"),
            (Request::write(3, CommandId::new(), WriteMethod::Host(HostWrite::VariantsSetEnabled { enabled: true })), "variants.set_enabled"),
            (Request::write(4, CommandId::new(), WriteMethod::Domain(aurelius_domain::Command::ChooseTaskVariant(ChooseTaskVariant { task_id, variant: Some(2) }))), "task.choose_variant"),
        ];
        for (request, name) in cases {
            let json = serde_json::to_value(&request).unwrap();
            assert_eq!(json["method"], name);
            assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request, "{name}");
        }
        // A task's run settings carry the count; older clients leave it out.
        let run: aurelius_domain::task::RunSettings = serde_json::from_value(serde_json::json!({ "variants": 3 })).unwrap();
        assert_eq!(run.variants, Some(3));
        assert!(serde_json::to_value(aurelius_domain::task::RunSettings::default()).unwrap().get("variants").is_none());
        assert!(FEATURES.contains(&feature::TASK_VARIANTS) && FEATURES.contains(&feature::TASK_LESSONS));
    }

    #[test]
    fn the_instruction_methods_roundtrip_under_their_names() {
        use aurelius_domain::{EmployeeId, ProposalId, proposal::{DecideProposal, RestoreEmployeeInstructions}};
        let employee_id = EmployeeId::new();
        let decide = DecideProposal { proposal_id: ProposalId::new(), approve: true, edits: None, note: None, instructions: Some("Work test-first.".into()) };
        let cases = [
            (Request::read(1, Method::EmployeeInstructionVersions { employee_id }), "employee.instruction_versions"),
            (Request::write(2, CommandId::new(), WriteMethod::Domain(aurelius_domain::Command::RestoreEmployeeInstructions(RestoreEmployeeInstructions { employee_id, version: 1 }))), "employee.restore_instructions"),
            (Request::write(3, CommandId::new(), WriteMethod::Domain(aurelius_domain::Command::DecideProposal(decide))), "proposal.decide"),
        ];
        for (request, name) in cases {
            let json = serde_json::to_value(&request).unwrap();
            assert_eq!(json["method"], name);
            assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request, "{name}");
        }
        assert!(FEATURES.contains(&feature::INSTRUCTION_PROPOSALS) && FEATURES.contains(&feature::QUICK_REPLIES));
    }

    #[test]
    fn optional_params_may_be_left_out_or_null() {
        let parse = |method: &str, params: Option<serde_json::Value>| {
            let mut json = serde_json::json!({ "protocol": 3, "request_id": 2, "method": method });
            if let Some(params) = params {
                json["params"] = params;
            }
            serde_json::from_value::<Request>(json).map(|r| r.call)
        };
        let all_optional = [
            Method::EmployeeList,
            Method::TaskList { project_id: None, employee_id: None, state: None },
            Method::MemoryList { project_id: None, employee_id: None },
            Method::CompanyGet,
            Method::BackupList,
        ];
        for method in all_optional {
            let name = serde_json::to_value(&method).unwrap()["method"].as_str().unwrap().to_owned();
            for params in [None, Some(serde_json::Value::Null)] {
                assert_eq!(parse(&name, params.clone()).unwrap(), Call::Read(method.clone()), "{name} {params:?}");
            }
        }
        // A required field is still required, and named.
        let err = parse("employee.chat", None).unwrap_err().to_string();
        assert!(err.contains("employee_id"), "{err}");
    }

    #[test]
    fn an_unknown_method_is_a_clear_error() {
        let bad = serde_json::json!({ "protocol": 3, "request_id": 1, "method": "task.reviews", "params": {} });
        let err = serde_json::from_value::<Request>(bad).unwrap_err().to_string();
        assert!(err.contains("task.reviews"), "{err}");
    }

    #[test]
    fn an_unparsed_request_is_answered_for_its_own_id() {
        let error_of = |value: serde_json::Value| {
            let err = serde_json::from_value::<Request>(value.clone()).unwrap_err();
            match reject_unparsed(Some(&value), &err) {
                ServerMessage::Response { request_id, outcome: Outcome::Error(e) } => (request_id, e.code),
                other => panic!("{other:?}"),
            }
        };
        let unknown = serde_json::json!({ "protocol": 3, "request_id": 42, "method": "mcp.login", "params": {} });
        assert_eq!(error_of(unknown), (42, ErrorCode::UnknownMethod));
        // A known method with bad params is still invalid_request.
        let bad_params = serde_json::json!({ "protocol": 3, "request_id": 43, "method": "task.list", "params": { "project_id": 5 } });
        assert_eq!(error_of(bad_params), (43, ErrorCode::InvalidRequest));
        let bad_write = serde_json::json!({ "protocol": 3, "request_id": 44, "method": "project.create", "params": {} });
        assert_eq!(error_of(bad_write), (44, ErrorCode::InvalidRequest));
        let no_id = serde_json::json!({ "method": "task.list" });
        assert_eq!(error_of(no_id).0, 0);
        assert!(Method::knows("session.hello") && Method::knows("terminal.close") && !Method::knows("project.create"));
    }

    #[test]
    fn the_server_messages_use_snake_case_kinds() {
        let json = serde_json::to_value(ServerMessage::PowerChanged(PowerState { pending: None })).unwrap();
        assert_eq!(json["type"], "power_changed");
        let json = serde_json::to_value(ServerMessage::LoadChanged(LoadSnapshot {
            cpu_pct: 1.0,
            mem_free_mb: 1,
            mem_total_mb: 2,
            running: 0,
            held: 0,
            gate: LoadGate::Open,
            hold: None,
            limits: LoadLimits { max_runs: 1, cpu_high: 80, cpu_low: 70, mem_min_mb: 1, enabled: true },
            checks_active: true,
            sampled_at_ms: 1,
        }))
        .unwrap();
        assert_eq!(json["type"], "load_changed");
        assert_eq!(json["gate"], "open");
    }
}
