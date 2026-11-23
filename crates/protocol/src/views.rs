//! Views that only the wire knows: host state (load, power, MCP, updates, backups),
//! files. The read models shared with storage live in `aurelius_domain::views`.

use aurelius_domain::{ConversationId, HostId, ProjectId, RunId, MessageId, job::JobState};
pub use aurelius_domain::views::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};


/// `fs.list_dir`: the folders in `path` (never files).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DirListing {
    /// The listed folder; `None` when only the starting points are listed.
    pub path: Option<String>,
    pub parent: Option<String>,
    pub entries: Vec<DirEntryView>,
    /// Starting points: the projects folder, home, drives.
    pub roots: Vec<DirEntryView>,
    /// More folders than were sent.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DirEntryView {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HealthResult {
    pub host_id: HostId,
    pub daemon_version: String,
    pub pid: u32,
    pub started_at_ms: i64,
    pub last_seq: u64,
    pub active_jobs: u32,
}

/// Largest Markdown file `markdown.get` returns.
pub const MARKDOWN_MAX_BYTES: u64 = 2 * 1024 * 1024;
/// Most files `markdown.list` lists.
pub const MARKDOWN_MAX_FILES: usize = 500;

/// Whose work folder a file query looks in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileScope {
    /// The project's folder.
    Project { project_id: ProjectId },
    /// Where a conversation works: a task's (its workspace, else the project
    /// folder), a project conversation's (the project folder), an employee
    /// chat's (the employee's own folder).
    Conversation { conversation_id: ConversationId },
    /// A task's own workspace, else where its conversation works.
    Task { task_id: aurelius_domain::TaskId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MarkdownFileEntry {
    /// Relative to the work folder, with `/`.
    pub path: String,
    pub byte_length: u64,
    pub modified_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct MarkdownFileList {
    /// The work folder that was listed (empty when there is none on the server).
    pub root: String,
    /// Sorted by path.
    pub files: Vec<MarkdownFileEntry>,
    /// More files than `MARKDOWN_MAX_FILES` exist; the rest is left out.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MarkdownFileContent {
    /// Relative to the work folder, with `/` (also when asked by absolute path).
    pub path: String,
    pub text: String,
    pub byte_length: u64,
    pub modified_ms: i64,
}

/// Largest part of an uploaded file `upload.get` sends at once (well under the frame limit).
pub const UPLOAD_CHUNK_BYTES: u64 = 2 * 1024 * 1024;
/// Largest side of a thumbnail `upload.get` makes.
pub const UPLOAD_THUMB_MAX_SIDE: u32 = 1024;

/// `upload.get`: a part of an uploaded file from `offset`, or its thumbnail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UploadChunk {
    pub id: String,
    pub name: String,
    /// Of the file (a thumbnail is always `image/jpeg`), guessed from its name.
    pub media_type: String,
    /// The whole file's size (the thumbnail's for a thumbnail).
    pub byte_length: u64,
    pub offset: u64,
    pub data_base64: String,
    /// No more parts after this one: `offset` + this part's size is `byte_length`.
    pub done: bool,
    /// `data_base64` is a JPEG thumbnail, not the file.
    pub thumbnail: bool,
}

/// Ephemeral streamed text. Not domain truth: after reconnect the client
/// refetches messages instead of relying on missed deltas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StreamDelta {
    pub run_id: RunId,
    pub conversation_id: ConversationId,
    pub message_id: MessageId,
    /// Full text so far (idempotent; a dropped delta loses nothing).
    pub text: String,
    /// All tool use so far (idempotent too), each call shortened (feature
    /// `activity_preview`). With `activity_changes`: only the calls new or
    /// changed since this session's previous update for the message.
    pub activity: Vec<ActivityView>,
    /// How long it runs and how many tokens it wrote so far.
    pub progress: Option<RunProgress>,
    /// `activity` holds changes only, to merge by `call_id` (new calls go at
    /// the end); `false` means the full list, as always. Only a session whose
    /// hello `accepts` `activity_changes` gets `true`, and never on the first
    /// update of a message in that session (that one carries every call).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub activity_changes: bool,
    /// A group chat answer streaming: the mode its run started in. Left out
    /// elsewhere and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<aurelius_domain::autonomy::AgentMode>,
    /// An employee's answer streaming into a thread (feature `chat_threads`):
    /// the thread's root message in `conversation_id`. Only connections that
    /// accept `chat_threads` get such an update. Left out elsewhere and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_parent: Option<MessageId>,
    /// A design the answer is writing now (feature `artifact_drafts`): only on the
    /// update that brings it, and only to a connection that accepts
    /// `artifact_drafts`. Left out elsewhere and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<ArtifactDraft>,
}

/// Largest design content a live update carries; a bigger file shows once its call is whole.
pub const ARTIFACT_DRAFT_MAX_BYTES: usize = 1024 * 1024;

/// An HTML file an employee is writing, before its `Write` call is complete: the
/// app shows it as it grows, as Claude Desktop does with an artifact. Display
/// only, never stored; the call itself arrives in `activity` once it is whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactDraft {
    /// The `Write` call's id (`ActivityView::call_id` once it is whole).
    pub call_id: String,
    /// The file's path as the employee wrote it.
    pub path: String,
    /// The content written so far (at most `ARTIFACT_DRAFT_MAX_BYTES`).
    pub content: String,
}

/// Where the approval bridge keeps the agent's screenshots, under the data folder.
pub const SCREENSHOT_DIR: &str = "screenshots";

/// The screenshot id in a screenshot tool's output ("… Saved as screenshot <id>.").
pub fn screenshot_id(output: &str) -> Option<&str> {
    let rest = &output[output.find("Saved as screenshot ")? + "Saved as screenshot ".len()..];
    let id = rest.split(|c: char| !(c.is_ascii_hexdigit() || c == '-')).next()?;
    valid_screenshot_id(id).then_some(id)
}

/// A screenshot id is a UUID (never a path).
pub fn valid_screenshot_id(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Something running now: since when, and the output tokens so far (thinking,
/// text and tool input; estimated while a message streams).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RunProgress {
    pub started_at_ms: i64,
    pub output_tokens: u64,
    /// The run is compacting the conversation's context before its turn (feature
    /// `compact_status`); absent on older servers, which show it as plain work.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub compacting: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JobSummary {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub state: JobState,
    pub progress_permille: u16,
    pub created_at_ms: i64,
    /// While it runs: since when and the tokens so far.
    pub live: Option<RunProgress>,
    /// What it works on: the conversation of an answer, or the task.
    pub conversation_id: Option<ConversationId>,
    pub task_id: Option<aurelius_domain::TaskId>,
    /// A ready job that waits: until when (none when it waits for the user
    /// without a time, e.g. "auth"), and why ("quota", "auth", "transient", ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_before_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JobsSnapshot {
    pub as_of_seq: u64,
    pub jobs: Vec<JobSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownWhen {
    /// After the grace countdown (60 s).
    Now,
    /// Once no run is running or queued and no task is running or queued.
    WhenIdle,
}

/// A requested shutdown of the server's PC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PendingShutdown {
    pub when: ShutdownWhen,
    pub requested_at_ms: i64,
    /// When the PC powers off; set while the countdown runs, `None` while
    /// `when_idle` still waits for work to finish.
    pub deadline_ms: Option<i64>,
}

/// `power.get`, `power.shutdown`, `power.cancel` and `power_changed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PowerState {
    pub pending: Option<PendingShutdown>,
}

/// The machine's load and the run admission (`load.get`, `load_changed`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LoadSnapshot {
    /// All cores together, 0–100, averaged over ~10 s.
    pub cpu_pct: f32,
    pub mem_free_mb: u64,
    pub mem_total_mb: u64,
    /// Runs going now (chat answers and task starts).
    pub running: u32,
    /// Runs that could start but wait for the machine.
    pub held: u32,
    pub gate: LoadGate,
    /// Why new runs wait (`gate` is `held`).
    pub hold: Option<LoadHold>,
    pub limits: LoadLimits,
    /// Whether CPU and memory are measured at all (only for the standard data
    /// folder; elsewhere only `max_runs` counts).
    pub checks_active: bool,
    pub sampled_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoadGate {
    Open,
    Held,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LoadHold {
    pub kind: LoadHoldKind,
    pub cpu_pct: Option<f32>,
    pub mem_free_mb: Option<u64>,
    pub running: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoadHoldKind {
    Cpu,
    Memory,
    MaxRuns,
    /// `max_runs` is 0: every new run waits.
    DisabledAll,
}

/// When a new run may start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LoadLimits {
    /// Runs at the same time; 0 holds every new run.
    pub max_runs: u32,
    /// Hold new runs from this CPU use (%)…
    pub cpu_high: u8,
    /// …until it is below this again.
    pub cpu_low: u8,
    pub mem_min_mb: u64,
    /// Whether CPU and memory count (the run limit always does).
    pub enabled: bool,
}

/// `load/set_limits`: `None` leaves a limit as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LoadLimitsUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_runs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_high: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_low: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mem_min_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// The browser that carries the Claude in Chrome extension.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserChoice {
    /// Whichever is running, else the one that has the extension.
    #[default]
    Automatic,
    Chrome,
    Edge,
}

/// `browser.get` and `browser.set`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BrowserSetting {
    /// What the user chose.
    pub choice: BrowserChoice,
    /// `AURELIUS_BROWSER` is set on the server and wins over `choice`.
    pub env_override: Option<BrowserChoice>,
    /// How runs drive the browser. Always sent by servers that know the choice;
    /// absent from older servers, which only have Claude in Chrome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<BrowserBackend>,
}

/// How a run drives the browser when the user allowed it for an employee.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserBackend {
    /// The Claude in Chrome extension in the user's own browser profile.
    #[default]
    ClaudeInChrome,
    /// Playwright MCP attached to a browser window of its own (an Aurelius
    /// profile in Chrome or Edge, kept between runs, so logins stay). Its tools
    /// follow the mode like other MCP tools and parallel runs each get a tab.
    Playwright,
}

/// `management_mode.get_setting` and `management_mode.set_enabled` (feature
/// `management_chooses_mode`): whether management (the CEO and their direct reports) chooses
/// its own task mode, Auto or Plan, when the user picked none. On by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManagementModeSetting {
    pub enabled: bool,
}

impl Default for ManagementModeSetting {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// `auto_allow.get_setting` and `auto_allow.set_enabled`: whether the safe read-only tools
/// (`tools`) pass without a question in every mode but Plan. On by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AutoAllowSetting {
    pub enabled: bool,
    /// The tools the setting covers, for display; fixed by the server.
    #[serde(default)]
    pub tools: Vec<String>,
}

/// The tools that pass without a question when auto-allow is on: read-only files, web search
/// and the window list. WebFetch (a fetched URL can leak data after prompt injection) and
/// screenshots are not on it.
pub const AUTO_ALLOW_TOOLS: &[&str] = &["Read", "Grep", "Glob", "WebSearch", "list_windows"];

/// Whether a tool as Claude Code names it (`list_windows` is `mcp__aurelius__list_windows`) is on the list.
pub fn auto_allow_tool(tool: &str) -> bool {
    AUTO_ALLOW_TOOLS.contains(&tool.strip_prefix("mcp__aurelius__").unwrap_or(tool))
}

impl AutoAllowSetting {
    pub fn new(enabled: bool) -> Self {
        Self { enabled, tools: AUTO_ALLOW_TOOLS.iter().map(|t| t.to_string()).collect() }
    }
}

/// Where Claude Code keeps an MCP server the user configured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpScope {
    /// `~/.claude.json`, for all projects.
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    Stdio,
    Http,
    Sse,
}

/// One MCP server of the user. Never holds env values, headers or tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpServerInfo {
    /// The name the user gave it; it is the key for `mcp.set`.
    pub name: String,
    pub scope: McpScope,
    pub transport: McpTransport,
    /// The command (without arguments) or the URL (without credentials or query).
    pub summary: String,
    /// Every run uses it.
    pub enabled: bool,
    /// Whether it needs an OAuth login (http/sse); older servers leave it out (`unknown`).
    #[serde(default)]
    pub auth: McpAuthStatus,
}

/// The OAuth state of an MCP server, read from Claude Code's own credential store.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpAuthStatus {
    /// No OAuth: stdio, or a fixed `Authorization` header.
    None,
    /// Claude Code has a usable token (it refreshes it itself).
    Authorized,
    /// The server asks for a login (`mcp.login_start`).
    NeedsLogin,
    /// Not known yet (never connected); a login can still be tried.
    #[default]
    Unknown,
}

/// Where a login started with `mcp.login_start` stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpLoginState {
    /// Waiting for the browser (the server's own callback) or for `mcp.login_complete`.
    Pending,
    Succeeded,
    Failed,
    /// Not finished in time; start again.
    Expired,
}

/// `mcp.login_start`, `mcp.login_complete` and `mcp.login_get`. Never holds a token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpLogin {
    pub login_id: String,
    /// The MCP server.
    pub name: String,
    pub state: McpLoginState,
    /// Open this in a browser. When the browser runs on the server's computer the
    /// login finishes by itself; elsewhere, the browser ends on a page that does
    /// not load: send that page's address to `mcp.login_complete`.
    pub authorization_url: String,
    /// The `http://localhost:<port>/callback` address the browser is sent back to.
    pub redirect_uri: String,
    pub expires_at_ms: i64,
    /// Why it failed.
    #[serde(default)]
    pub error: Option<String>,
}

/// What `mcp/add` sends. Env and header values are secrets: `Debug`
/// never prints them and the server never returns them.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpAddParams {
    pub name: String,
    pub transport: McpTransport,
    /// Stdio: the program to run.
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    /// Http and sse: the endpoint.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    /// Also switch it on for every run.
    #[serde(default)]
    pub enable: bool,
}

impl std::fmt::Debug for McpAddParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpAddParams")
            .field("name", &self.name)
            .field("transport", &self.transport)
            .field("command", &self.command)
            .field("args", &format_args!("<{} hidden>", self.args.len()))
            .field("env", &self.env.keys().collect::<Vec<_>>())
            .field("url", &self.url.as_ref().map(|_| "<hidden>"))
            .field("headers", &self.headers.keys().collect::<Vec<_>>())
            .field("enable", &self.enable)
            .finish()
    }
}

/// `mcp.list` and the MCP writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpServers {
    pub servers: Vec<McpServerInfo>,
    /// Enabled names that Claude Code no longer has; they are skipped in runs.
    pub missing: Vec<String>,
}

/// `mcp_always_allow.list` and `.remove`: the MCP tools (`mcp__<server>__<tool>`)
/// the user answered with 'Always allow'. They pass without asking in every run, in
/// Ask, Accept edits and Auto; Plan mode still asks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpAlwaysAllow {
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateStatus {
    pub current: String,
    /// Only the installed production app updates itself.
    pub enabled: bool,
    pub reason: Option<String>,
    pub latest: Option<String>,
    pub available: bool,
    pub notes: Option<String>,
    /// A downloaded and verified version, ready to apply.
    pub staged: Option<String>,
    pub checked_at_ms: i64,
    pub error: Option<String>,
    /// The fields above are the server's (service and aurelius-ctl). The
    /// desktop app is released separately: its own status, when this
    /// computer has the app installed. Absent from older servers.
    pub app: Option<Box<UpdateStatus>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BackupView {
    pub id: String,
    /// "auto" or "manual"
    pub kind: String,
    pub created_at_ms: i64,
    pub app_version: String,
    pub bytes: u64,
    pub files: u64,
    /// Forgotten content was removed from this backup afterwards.
    pub scrubbed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BackupsSnapshot {
    pub backups: Vec<BackupView>,
    pub folder: String,
    /// Set after a restore: background work waits until resumed.
    pub paused: Option<String>,
    pub restore_pending: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanLimitView {
    pub label: String,
    pub percent: u8,
    pub resets: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SubscriptionUsage {
    /// Whose Claude login this is (protocol 5: `subscription.get` answers the
    /// session's own; nobody sees another's usage, B12). None: the server's
    /// own login, until runs move to the board members' logins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_user: Option<aurelius_domain::UserId>,
    /// e.g. "max"; None when unknown.
    pub plan: Option<String>,
    pub limits: Vec<PlanLimitView>,
    /// The full `/usage` text (shown when parsing found nothing).
    pub raw: String,
    pub fetched_at_ms: i64,
    /// Why there is no data (no Claude Code connection, not signed in, …).
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimelineQuery {
    pub project_id: Option<ProjectId>,
    /// Only the chain with this correlation id ("Waarom?" / "Wat volgde?").
    pub correlation_id: Option<String>,
    /// Keyset cursor: cards strictly older than this sequence number.
    pub before_seq: Option<u64>,
    pub limit: u32,
    /// Include routine runtime/tool events ("Activity").
    pub activity: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimelinePage {
    pub cards: Vec<TimelineCard>,
    /// Pass as `before_seq` for the next page; `None` when exhausted.
    pub next_before_seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NotificationsSnapshot {
    pub notifications: Vec<NotificationView>,
    pub unread: u64,
}
