//! Runtime contracts. An `AgentRuntime` executes one turn and
//! reports normalized events. Real adapters (API loop, Codex, Claude Code)
//! implement the same trait as the fake, so the daemon path is identical.

pub mod claude_code;
#[cfg(feature = "testing")]
pub mod fake;
pub mod http_api;
pub mod redact;

use aurelius_domain::RunId;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub role: TurnRole,
    pub text: String,
    /// Images the user sent with this turn, in order; the text names them.
    #[serde(default)]
    pub images: Vec<ImagePart>,
}

/// An image the model sees (already checked and scaled down by Aurelius).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImagePart {
    pub name: String,
    /// `image/jpeg`, `image/png`, `image/gif` or `image/webp`.
    pub media_type: String,
    pub data: Vec<u8>,
}

impl ImagePart {
    /// The bytes as standard base64, as model APIs take them.
    pub fn base64(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&self.data)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRequest {
    pub run_id: RunId,
    pub model_id: String,
    /// Trusted instructions (employee identity); never user or tool content.
    pub system: Option<String>,
    /// Complete prior turns, oldest first, ending with the user turn to answer.
    pub turns: Vec<Turn>,
    /// Reasoning effort, if chosen and supported by the runtime.
    pub effort: Option<String>,
    /// What the runtime's own tools may do. `TextOnly` unless set.
    #[serde(default)]
    pub profile: ToolProfile,
    /// Model for the runtime's own subagents, where it has them: none = the
    /// runtime's default, empty = the runtime decides.
    #[serde(default)]
    pub subagent_model: Option<String>,
    /// What happens to the runtime's own session (its transcript), where it has one.
    #[serde(default)]
    pub session: SessionUse,
    /// The session's id: the conversation's session; none = named after the run.
    #[serde(default)]
    pub session_id: Option<String>,
    /// The conversation goes on in this session: it is kept after the run
    /// completes, not only for a usage limit. It still goes when the run is
    /// stopped or fails, as it may hold half a turn.
    #[serde(default)]
    pub keep_session: bool,
}

/// The runtime's own session of a run, named `session_id` (else the run id). A
/// Claude Code browser run keeps its employee's session id instead and is always `None`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionUse {
    /// Nothing is kept: the turns are the whole context.
    #[default]
    None,
    /// A new session: the turns are the whole context. Kept while the run lasts,
    /// and past its end when it stops for a usage limit (or, with `keep_session`, completes).
    Keep,
    /// Go on in the conversation's session, which already holds the conversation:
    /// the turns are only what is new. The caller checked `has_session` first.
    Continue,
    /// The run waited for a usage limit: carry on in the session the earlier
    /// attempt left behind, with its full context, instead of the turns. When the
    /// runtime no longer has it, this is `Keep` and the turns start over.
    Resume,
}

/// Tools a runtime may use for one run (Claude Code's built-in tools).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum ToolProfile {
    /// No tools at all: the model can only write text.
    #[default]
    TextOnly,
    /// An employee at work: files, commands and the web, confined to `folders`
    /// (the first is the working folder). Whatever the mode doesn't allow by
    /// itself is asked through `approval_bridge`, which asks the user in Aurelius.
    /// `browser` adds Claude in Chrome (the user's own browser, with their
    /// logins); off unless the user allows it. Its tools always ask first.
    /// `browser_session` is the Claude Code session id the run takes for it: the
    /// extension keys its 'Claude' tab group on the session id, so the same id
    /// (one per employee) keeps one group and tab across runs (CLI 2.1.289).
    /// `playwright`, when set, is the browser instead: the stdio config of the
    /// Playwright MCP server (attached to Aurelius' own browser window), used
    /// with `browser` in place of Claude in Chrome.
    /// `mcp_servers` are the user's own MCP servers this employee may use (name
    /// to full config, read from their Claude Code setup at run time).
    /// `plugins` are install folders of the Claude Code plugins this employee (or,
    /// for the project assistant, this project) has enabled (loaded with
    /// `--plugin-dir`; they add the `Skill` tool).
    /// `artifacts` adds Claude Code's artifact tools (pages on claude.ai); off
    /// unless the user turned them on server-wide. A publish always asks first
    /// except in Auto.
    /// `access` is the employee's level (C3): which built-in tools the run gets.
    /// Below `Inherit` it has only that list, see [`ToolProfile::confined`].
    /// `tool_policy` is the employee's own list on top of that (C2): it can only
    /// narrow the level, and says which tools always ask first.
    Work { folders: Vec<std::path::PathBuf>, permission_mode: String, approval_bridge: ApprovalBridge, #[serde(default)] browser: bool, #[serde(default)] browser_session: Option<String>, #[serde(default)] playwright: Option<serde_json::Value>, #[serde(default)] mcp_servers: std::collections::BTreeMap<String, serde_json::Value>, #[serde(default)] plugins: Vec<std::path::PathBuf>, #[serde(default)] artifacts: bool, #[serde(default)] access: aurelius_domain::employee::AccessLevel, #[serde(default)] tool_policy: aurelius_domain::employee_tools::ToolPolicy },
    /// A short decision with only the given tools of the bridge (`tools`: their
    /// full names, allowed without asking): no files, commands, web or other MCP
    /// servers, at most `max_turns` turns (the dispatcher of a ticket).
    Triage { approval_bridge: ApprovalBridge, tools: Vec<String>, max_turns: u32 },
}

impl ToolProfile {
    /// A level below `Inherit` is an explicit list: besides its built-in tools and
    /// the bridge the run has nothing the level does not name, so no MCP servers of
    /// the user, plugins (they bring hooks and servers) or artifacts. The browser
    /// is a permission of its own, which the user switches per employee.
    pub fn confined(mut self) -> Self {
        if let ToolProfile::Work { access, mcp_servers, plugins, artifacts, .. } = &mut self {
            if !access.is_inherit() {
                mcp_servers.clear();
                plugins.clear();
                *artifacts = false;
            }
        }
        self
    }
}

/// Program (the agent bridge, an MCP stdio server) that Claude Code asks for permission; it
/// forwards each question to the daemon as an approval for this run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalBridge {
    pub program: std::path::PathBuf,
    pub args: Vec<String>,
}

/// One use of a runtime-side tool (a command, an edit, a web search…), as
/// reported by the runtime. `Started` comes first; `Finished` may never come
/// (e.g. denied or cancelled).
#[derive(Debug, Clone, PartialEq)]
pub enum Activity {
    /// `parent`: the call that started the subagent doing this, if any.
    Started { call_id: String, tool: String, input: serde_json::Value, parent: Option<String> },
    Finished { call_id: String, ok: bool, output: String },
}

/// Normalized runtime events. Only `Completed` is a terminal success.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeEvent {
    /// What the runtime sent to start the run (before `Started`); runtimes that
    /// cannot say leave it out.
    Sent(Box<SentRequest>),
    Started,
    /// The model the provider/runtime reports it is actually using.
    ModelReported(String),
    TextDelta(String),
    /// The runtime used one of its own tools (see `ToolProfile`).
    Activity(Activity),
    /// Tokens (and cost, when the runtime reports it) for this run; may come in parts.
    Usage(Usage),
    /// Output tokens written so far in this run (thinking, text and tool input),
    /// for the "2m 10s · 3.9k tokens" line while it runs. Display only: never
    /// recorded as usage (that is `Usage`). Exact per finished message, estimated
    /// (characters / 4) while a message streams.
    Progress { output_tokens: u64 },
    /// The run published (or republished) a Claude Code artifact; `version` is
    /// Claude Code's version number of that publish, when it gave one; `path`
    /// is the published file (as the run named it, maybe relative to its folder).
    ArtifactPublished { url: String, title: String, version: Option<u32>, path: Option<String> },
    /// The runtime compacted the session's context (asked with `/compact`, or on its
    /// own when it ran full): about this many tokens before and after.
    Compacted { pre_tokens: u64, post_tokens: u64 },
    Completed,
    Failed { code: String, message: String },
    /// The usage/session limit is reached (terminal, not a failure): the run waits
    /// for the reset (epoch ms) when the provider said when, and goes again.
    Quota { reset_at_ms: Option<i64> },
    Interrupted,
}

/// Usage as reported by the provider. `cost_micro_usd` is only set when the
/// provider reports a cost; `estimate` marks values that are not a bill
/// (e.g. Claude Code via a subscription reports the API-equivalent value).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Input not read from the prompt cache: fresh input plus `cache_creation_tokens`.
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Input read from the prompt cache.
    pub cached_tokens: u64,
    /// The part of `input_tokens` written to the prompt cache (dearer than plain input).
    #[serde(default)]
    pub cache_creation_tokens: u64,
    pub cost_micro_usd: Option<i64>,
    pub estimate: bool,
    /// Each model call of the run (Claude Code: every assistant message, its
    /// subagents' too), for the trace. Empty when the runtime does not report them.
    #[serde(default)]
    pub steps: Vec<UsageStep>,
    /// The tools the runtime loaded for the run (their definitions are input of every call).
    #[serde(default)]
    pub tools: Vec<String>,
}

/// One model call within a run, as the provider counted it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageStep {
    /// Fresh input: neither read from nor written to the prompt cache.
    pub input_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub output_tokens: u64,
    /// The tools this call asked for (empty: it answered in text).
    pub tools: Vec<String>,
    /// The call was a subagent's.
    pub subagent: bool,
}

/// What a runtime actually sent to start a run, for the trace.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentRequest {
    /// The command line, with secrets left out (the MCP configuration is named by its servers).
    pub args: Vec<String>,
    /// none | new | continue | resume: what happened to the runtime's own session.
    pub session: String,
    /// The prompt text as it went in (images as their names only).
    pub prompt: String,
    pub images: Vec<SentImage>,
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentImage {
    pub name: String,
    pub media_type: String,
    pub bytes: u64,
}

/// Capability values distinguish "unknown" from "unsupported".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeCapabilities {
    pub runtime_kind: String,
    pub version: String,
    pub streaming: Capability,
    pub cancel: Capability,
    pub resume: Capability,
    pub tools: Capability,
    pub native_sandbox: Capability,
}

/// Adapter failures, classified so the scheduler can decide.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RuntimeError {
    /// Temporary: network, 5xx, process not reachable. Retried with backoff.
    #[error("runtime unavailable: {0}")]
    Unavailable(String),
    /// Login missing or expired. Waits for the user.
    #[error("sign-in required: {0}")]
    Auth(String),
    /// Quota exhausted; reset time if the provider reported one.
    #[error("quota exhausted")]
    Quota { reset_at_ms: Option<i64> },
    /// Invalid request or unsupported model; retrying cannot help.
    #[error("request rejected: {0}")]
    Permanent(String),
    /// The board member's own Claude login is missing, expired or theirs is
    /// disabled: the run waits for it ('login'), never on another login.
    #[error("Claude login required: {0}")]
    Login(String),
}

/// Input sent back into a running turn.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeInput {
    /// The user switched the working mode while the turn runs (Claude Code's permission mode).
    SetPermissionMode { mode: String },
}

/// Handle to a running turn: a bounded event stream, an input channel and a cancel switch.
pub struct RunHandle {
    pub run_id: RunId,
    pub events: mpsc::Receiver<RuntimeEvent>,
    pub inputs: mpsc::Sender<RuntimeInput>,
    cancel: watch::Sender<bool>,
}

impl RunHandle {
    pub fn new(
        run_id: RunId,
        events: mpsc::Receiver<RuntimeEvent>,
        inputs: mpsc::Sender<RuntimeInput>,
        cancel: watch::Sender<bool>,
    ) -> Self {
        Self { run_id, events: redacted(events), inputs, cancel }
    }

    pub fn canceller(&self) -> Canceller {
        Canceller(self.cancel.clone())
    }
}

/// The events of a run as they leave its adapter, every value of the vault
/// redacted (SEC0 E14 a): text deltas with a tail buffer, so a value split over
/// two deltas is still found; tool input and output, the result, errors and
/// stderr whole. Whatever reads a run's events (messages, traces, activity,
/// approvals, memory) sees only this.
fn redacted(mut raw: mpsc::Receiver<RuntimeEvent>) -> mpsc::Receiver<RuntimeEvent> {
    let (tx, rx) = mpsc::channel(EVENT_BUFFER);
    tokio::spawn(async move {
        let mut text = redact::Stream::default();
        while let Some(event) = raw.recv().await {
            let redactor = redact::current();
            let out = match event {
                RuntimeEvent::TextDelta(delta) => {
                    let ready = text.push(&redactor, &delta);
                    if ready.is_empty() {
                        continue;
                    }
                    RuntimeEvent::TextDelta(ready)
                }
                other => {
                    let rest = text.flush(&redactor);
                    if !rest.is_empty() && tx.send(RuntimeEvent::TextDelta(rest)).await.is_err() {
                        return;
                    }
                    redact_event(&redactor, other)
                }
            };
            if tx.send(out).await.is_err() {
                return;
            }
        }
        let rest = text.flush(&redact::current());
        if !rest.is_empty() {
            let _ = tx.send(RuntimeEvent::TextDelta(rest)).await;
        }
    });
    rx
}

fn redact_event(r: &redact::Redactor, event: RuntimeEvent) -> RuntimeEvent {
    if r.is_empty() {
        return event;
    }
    let s = |text: String| match r.redact(&text) {
        std::borrow::Cow::Borrowed(_) => text,
        std::borrow::Cow::Owned(clean) => clean,
    };
    match event {
        RuntimeEvent::Sent(mut sent) => {
            sent.args = sent.args.into_iter().map(s).collect();
            sent.prompt = s(std::mem::take(&mut sent.prompt));
            RuntimeEvent::Sent(sent)
        }
        RuntimeEvent::ModelReported(model) => RuntimeEvent::ModelReported(s(model)),
        RuntimeEvent::TextDelta(delta) => RuntimeEvent::TextDelta(s(delta)),
        RuntimeEvent::Activity(Activity::Started { call_id, tool, mut input, parent }) => {
            r.redact_json(&mut input);
            RuntimeEvent::Activity(Activity::Started { call_id, tool: s(tool), input, parent })
        }
        RuntimeEvent::Activity(Activity::Finished { call_id, ok, output }) => RuntimeEvent::Activity(Activity::Finished { call_id, ok, output: s(output) }),
        RuntimeEvent::ArtifactPublished { url, title, version, path } => RuntimeEvent::ArtifactPublished { url: s(url), title: s(title), version, path: path.map(s) },
        RuntimeEvent::Failed { code, message } => RuntimeEvent::Failed { code, message: s(message) },
        other => other,
    }
}

/// Cloneable cancel trigger for exactly one run.
#[derive(Clone)]
pub struct Canceller(watch::Sender<bool>);

impl Canceller {
    pub fn cancel(&self) {
        let _ = self.0.send(true);
    }
}

#[async_trait::async_trait]
pub trait AgentRuntime: Send + Sync {
    async fn probe(&self) -> RuntimeCapabilities;
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError>;
    /// Whether a conversation can go on in a session of this runtime (`SessionUse::Continue`).
    fn keeps_sessions(&self) -> bool {
        false
    }
    /// Whether session `id` is still there to go on in.
    fn has_session(&self, _id: &str) -> bool {
        false
    }
    /// Remove what the runtime keeps of session `id`. Best effort.
    fn remove_session(&self, _id: &str) {}
    /// This runtime with the vault's values for one run (a work run's
    /// environment, SEC0 E15); `None`: it cannot take them, the run goes
    /// without.
    fn with_vault(&self, _vault: claude_code::VaultEnv) -> Option<std::sync::Arc<dyn AgentRuntime>> {
        None
    }
}

/// Bounded channel size for runtime events.
pub const EVENT_BUFFER: usize = 256;

#[cfg(test)]
#[path = "redact_events_tests.rs"]
mod redact_events_tests;
