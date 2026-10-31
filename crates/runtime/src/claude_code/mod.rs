//! Claude Code runtime adapter.
//!
//! Runs the user's installed, unmodified `claude` binary in print mode with
//! `--output-format stream-json`. Authentication stays entirely inside Claude
//! Code (the user's own login); Aurelius never reads, copies or proxies tokens.
//!
//! Profiles (`ToolProfile`):
//! - **text-only** (default): built-in tools off (`--tools ""`), empty MCP
//!   config, customizations off (`--safe-mode`), every prompt denied.
//! - **web** (chat with web research): only WebSearch and WebFetch, allowed
//!   without prompts; nothing can touch the computer.
//! - **work** (employees and the project assistant): files, commands and the web in the project folder
//!   (`--restricted` confines file tools to it and refuses bypass). The
//!   employee's mode becomes `--permission-mode`; anything that mode would ask
//!   goes to Aurelius' approval bridge (an MCP tool) and so to the user's inbox.
//! The init line is checked against the profile (fail closed). The child gets
//! an allowlisted environment.

mod job;
pub mod parse;
pub mod session;

use crate::*;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, watch};

#[derive(Clone)]
pub struct ClaudeCodeRuntime {
    pub executable: PathBuf,
    /// Aurelius-managed working directories (`<data>/runtimes/<connection>/runs`).
    pub run_root: PathBuf,
    pub model: Option<String>,
}

/// Environment variables passed to the child. Everything else (including any
/// API keys in the parent environment) is left out; the CLI then uses the
/// login the user made in Claude Code itself.
const ENV_ALLOWLIST: &[&str] = &[
    "PATH", "PATHEXT", "SYSTEMROOT", "SystemRoot", "WINDIR", "COMSPEC", "TEMP", "TMP", "USERPROFILE", "HOMEDRIVE",
    "HOMEPATH", "APPDATA", "LOCALAPPDATA", "PROGRAMDATA", "USERNAME", "USERDOMAIN", "HOME", "USER", "LOGNAME", "SHELL",
    "TMPDIR", "LANG", "LC_ALL", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME",
];

/// Turns off `run_in_background` for subagents and Bash in every child.
const DISABLE_BACKGROUND_TASKS: &str = "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS";

/// The model Claude Code gives its subagents (the Task tool) in every child.
const SUBAGENT_MODEL: &str = "CLAUDE_CODE_SUBAGENT_MODEL";

/// The daemon's own setting for it, above the user's choice in Aurelius:
/// unset means that choice (or `sonnet`), empty means the CLI decides.
const SUBAGENT_MODEL_SETTING: &str = "AURELIUS_SUBAGENT_MODEL";

/// The subagent model for a value of `AURELIUS_SUBAGENT_MODEL` and the run's
/// own choice (empty: the CLI decides).
fn subagent_model(setting: Option<&str>, chosen: Option<&str>) -> Option<String> {
    match setting.or(chosen).map(str::trim) {
        None => Some("sonnet".into()),
        Some("") => None,
        Some(model) => Some(model.to_owned()),
    }
}

/// Caps the jobs of every `cargo build`/`test` a run starts.
const CARGO_BUILD_JOBS: &str = "CARGO_BUILD_JOBS";

/// Half the machine's cores (at least one): a run's builds leave room for
/// the other runs and the user, as runs are admitted on measured load.
fn cargo_build_jobs() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get() / 2).max(1)
}

/// Where Claude Code is installed on this computer: `PATH` first, then the
/// installer's default `~/.local/bin`.
pub fn locate() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "claude.exe" } else { "claude" };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(exe);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let candidate = PathBuf::from(home).join(".local").join("bin").join(exe);
    candidate.is_file().then_some(candidate)
}

/// The argument list (no shell, no string concatenation).
/// Built-in tools of the work profile.
/// `AskUserQuestion` goes through the approval bridge: the user answers in Aurelius.
/// `ExitPlanMode` hands in a plan (plan mode); without it in `--tools` Claude cannot
/// submit one (checked against CLI 2.1.287). The user decides on it in Aurelius.
/// `Task` lets the agent start subagents; they use the same tools and ask the same way.
pub const WORK_TOOLS: &[&str] = &["Read", "Glob", "Grep", "Edit", "Write", "Bash", "WebSearch", "WebFetch", "AskUserQuestion", "ExitPlanMode", "Task"];
/// Tools that always go to the user instead of being allowed by rule.
pub const ALWAYS_ASKED_TOOLS: &[&str] = &["AskUserQuestion", "ExitPlanMode"];
/// The MCP server Claude Code starts for `--chrome`, and the prefix of its tools.
pub const CHROME_SERVER: &str = "claude-in-chrome";
pub const CHROME_TOOL_PREFIX: &str = "mcp__claude-in-chrome__";
/// The Playwright MCP server a run gets as its browser instead of Claude in Chrome,
/// and the prefix of its tools. Its own name, so a user's "playwright" server is untouched.
pub const PLAYWRIGHT_SERVER: &str = "aurelius-browser";
pub const PLAYWRIGHT_TOOL_PREFIX: &str = "mcp__aurelius-browser__";
/// Tools of the browser the user allowed for an employee, whichever backend serves it.
pub fn is_browser_tool(tool: &str) -> bool {
    tool.starts_with(CHROME_TOOL_PREFIX) || tool.starts_with(PLAYWRIGHT_TOOL_PREFIX)
}
/// Server names the user's own MCP servers may not take: Aurelius' bridge and the browser.
pub fn is_reserved_server(name: &str) -> bool {
    name == "aurelius" || name == CHROME_SERVER || name == PLAYWRIGHT_SERVER
}
/// How Claude Code spells a server name inside tool names (`mcp__<name>__<tool>`):
/// everything but letters, digits, `_` and `-` becomes `_`.
pub fn tool_server_name(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}
/// The server a Claude Code tool name belongs to (`mcp__<server>__<tool>`), if it is an MCP tool.
pub fn mcp_tool_server(tool: &str) -> Option<&str> {
    tool.strip_prefix("mcp__")?.split_once("__").map(|(server, _)| server)
}
/// The approval bridge's tool, as Claude Code names it.
pub const APPROVAL_TOOL: &str = "mcp__aurelius__approve";
/// The approval bridge's screen tools. Claude Code never asks for them:
/// the bridge itself checks the employee's Screen permission and otherwise
/// asks the user in the inbox (so it holds in every mode, Auto too).
pub const SCREEN_TOOLS: &[&str] = &["mcp__aurelius__screenshot", "mcp__aurelius__list_windows"];
/// An employee's own memory through the bridge: allowed by rule, never asked.
pub const MEMORY_TOOLS: &[&str] = &["mcp__aurelius__remember", "mcp__aurelius__recall", "mcp__aurelius__delegate_task", "mcp__aurelius__message_task", "mcp__aurelius__hand_over_task", "mcp__aurelius__add_idea", "mcp__aurelius__list_tasks", "mcp__aurelius__read_task", "mcp__aurelius__list_conversations", "mcp__aurelius__read_conversation", "mcp__aurelius__propose_colleague", "mcp__aurelius__propose_instructions", "mcp__aurelius__suggest_replies", "mcp__aurelius__set_mode", "mcp__aurelius__read_group_chat", "mcp__aurelius__post_to_group_chat"];
/// Claude Code's subagents (`--tools Agent` lists as `Task`, CLI 2.1.287):
/// they inherit the run's tools and ask the same way.
pub const SUBAGENT_TOOL: &str = "Task";
/// Claude Code's MCP tool search: with it in `--tools` the MCP tools (the bridge,
/// the user's servers, the browser) go in as names only and the model loads a
/// definition when it needs one; without it every definition is sent upfront on
/// every request (CLI 2.1.301). It only reads, so it is allowed by rule.
pub const TOOL_SEARCH_TOOL: &str = "ToolSearch";
/// Claude Code's skill tool (CLI 2.1.289 lists it as `Skill`); only with plugins.
pub const SKILL_TOOL: &str = "Skill";
/// Claude Code's artifact tools (pages on claude.ai), only when the user turned
/// artifacts on. CLI 2.1.289 registers these three; `ArtifactCheck` is not one.
pub const ARTIFACT_TOOLS: &[&str] = &["Artifact", "ArtifactData", "ArtifactComments"];
/// The artifact tool that publishes pages.
pub const ARTIFACT_TOOL: &str = "Artifact";
/// Turns the artifact tools on in print mode (off there by default).
const ARTIFACT_ENV: &str = "CLAUDE_CODE_ARTIFACT";
/// Keeps Claude Code from opening a browser on the server at each publish.
const ARTIFACT_AUTO_OPEN_ENV: &str = "CLAUDE_CODE_ARTIFACT_AUTO_OPEN";

/// Whether a call of an artifact tool only reads (by its `action`, CLI 2.1.289):
/// listing and reading artifacts, reading an artifact's data or comments.
/// Everything else publishes, deletes, pins, writes data or replies on claude.ai.
/// `None` for tools that are not artifact tools.
pub fn artifact_call_reads(tool: &str, input: &serde_json::Value) -> Option<bool> {
    let action = input["action"].as_str().unwrap_or("");
    Some(match tool {
        // No action means publish.
        "Artifact" => matches!(action, "list" | "read"),
        "ArtifactData" => matches!(action, "get" | "list" | "query" | "profiles"),
        "ArtifactComments" => action == "read",
        _ => return None,
    })
}

/// Whether an `Artifact` call publishes a page (a new one or a new version):
/// no action or `publish`, and not an upload into the artifact's asset store.
pub fn artifact_call_publishes(tool: &str, input: &serde_json::Value) -> bool {
    tool == ARTIFACT_TOOL && matches!(input["action"].as_str(), None | Some("publish")) && input["asset"] != true
}

pub fn arguments(model: Option<&str>, system_file: Option<&Path>, effort: Option<&str>) -> Vec<String> {
    profile_arguments(&ToolProfile::TextOnly, model, system_file, effort)
}

/// `system_file` holds the system text: on the command line it would hit
/// Windows' 32 767-character limit (os error 206) once memories grow.
pub fn profile_arguments(profile: &ToolProfile, model: Option<&str>, system_file: Option<&Path>, effort: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = ["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--input-format", "text"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let push = |args: &mut Vec<String>, items: &[&str]| args.extend(items.iter().map(|s| s.to_string()));
    match profile {
        ToolProfile::TextOnly => push(&mut args, &[
            "--tools", "", "--disallowedTools", "mcp__*", "--strict-mcp-config", "--mcp-config", r#"{"mcpServers":{}}"#,
            "--safe-mode", "--permission-prompts", "none",
        ]),
        ToolProfile::Triage { approval_bridge, tools, max_turns } => {
            // Only the bridge, and of it only `tools`: no built-in tool, nothing is asked.
            // No --safe-mode: it leaves MCP servers out, the bridge too (CLI 2.1.312).
            let mcp = serde_json::json!({ "mcpServers": { "aurelius": {
                "type": "stdio",
                "command": approval_bridge.program.to_string_lossy(),
                "args": approval_bridge.args,
            } } })
            .to_string();
            push(&mut args, &[
                "--tools", "", "--strict-mcp-config", "--mcp-config", &mcp, "--allowedTools", &tools.join(","),
                "--permission-prompts", "none", "--max-turns", &max_turns.to_string(),
            ]);
        }
        ToolProfile::Work { folders, permission_mode, approval_bridge, browser, browser_session, playwright, mcp_servers, plugins, artifacts } => {
            let mut tools = WORK_TOOLS.join(",");
            tools.push(',');
            tools.push_str(TOOL_SEARCH_TOOL);
            if !plugins.is_empty() {
                tools.push(',');
                tools.push_str(SKILL_TOOL);
            }
            // Publishing asks Claude Code's permission (manual and acceptEdits, CLI 2.1.289),
            // so it reaches the approval bridge; no allow rule.
            if *artifacts {
                tools.push(',');
                tools.push_str(&ARTIFACT_TOOLS.join(","));
            }
            // The user's own servers first, so a server called "aurelius" can never replace the bridge.
            let mut servers: serde_json::Map<String, serde_json::Value> = mcp_servers.iter().filter(|(n, _)| !is_reserved_server(n)).map(|(n, c)| (n.clone(), c.clone())).collect();
            servers.insert("aurelius".into(), serde_json::json!({
                "type": "stdio",
                "command": approval_bridge.program.to_string_lossy(),
                "args": approval_bridge.args,
            }));
            if let (true, Some(config)) = (*browser, playwright) {
                servers.insert(PLAYWRIGHT_SERVER.into(), config.clone());
            }
            let mcp = serde_json::json!({ "mcpServers": servers }).to_string();
            // Never bypass: an unknown mode falls back to asking.
            let mode = match permission_mode.as_str() {
                "manual" | "acceptEdits" | "auto" | "plan" => permission_mode.as_str(),
                _ => "manual",
            };
            push(&mut args, &[
                "--restricted", "--tools", &tools, "--strict-mcp-config", "--mcp-config", &mcp,
                "--permission-mode", mode, "--permission-prompts", "host", "--permission-prompt-tool", APPROVAL_TOOL,
            ]);
            // Starting a subagent is never asked: what it does is asked as usual.
            let mut allowed: Vec<&str> = SCREEN_TOOLS.iter().chain(MEMORY_TOOLS).copied().chain([SUBAGENT_TOOL, TOOL_SEARCH_TOOL]).collect();
            // A skill only loads instructions; what it then does is asked as usual.
            if !plugins.is_empty() {
                allowed.push(SKILL_TOOL);
            }
            for dir in plugins {
                args.push("--plugin-dir".into());
                args.push(dir.to_string_lossy().into_owned());
            }
            if *browser && playwright.is_none() {
                args.push("--chrome".into());
                // Claude in Chrome's tab group belongs to the session id: a fixed one
                // per employee keeps its group and tab across runs. Nothing is stored
                // (--no-session-persistence), so runs never share a transcript and
                // each run gets the whole conversation again.
                if let Some(id) = browser_session.as_deref().filter(|id| is_session_id(id)) {
                    args.push("--session-id".into());
                    args.push(id.into());
                }
            }
            // Aurelius' Auto: Claude Code asks as in manual mode and the approval
            // bridge allows the work tools while the user's mode is Auto. No allow
            // rules: Claude Code cannot take them back when the user switches to a
            // stricter mode mid-answer. Its own "auto" still asks for edits (CLI
            // 2.1.287) and could wave the browser through.
            if mode == "auto" {
                if let Some(i) = args.iter().position(|a| a == "--permission-mode") {
                    args[i + 1] = "manual".into();
                }
            }
            push(&mut args, &["--allowedTools", &allowed.join(",")]);
            // Stdin stays open during the run: a mode switch goes in as a control request.
            if let Some(i) = args.iter().position(|a| a == "--input-format") {
                args[i + 1] = "stream-json".into();
            }
            for extra in folders.iter().skip(1) {
                args.push("--add-dir".into());
                args.push(extra.to_string_lossy().into_owned());
            }
        }
    }
    args.push("--no-session-persistence".into());
    if let Some(model) = model.filter(|m| !m.trim().is_empty()) {
        args.push("--model".into());
        args.push(model.into());
    }
    if let Some(effort) = effort.filter(|e| aurelius_domain::connection::EFFORT_LEVELS.contains(e)) {
        args.push("--effort".into());
        args.push(effort.into());
    }
    if let Some(file) = system_file {
        args.push("--append-system-prompt-file".into());
        args.push(file.to_string_lossy().into_owned());
    }
    args
}

/// Whether `--session-id` takes this: a lowercase hyphenated UUID.
fn is_session_id(id: &str) -> bool {
    id.len() == 36
        && id.char_indices().all(|(i, c)| if matches!(i, 8 | 13 | 18 | 23) { c == '-' } else { c.is_ascii_digit() || ('a'..='f').contains(&c) })
}

/// Whether the profile fixes the session id itself: a Claude in Chrome run takes
/// its employee's, so Claude in Chrome keeps one tab group per employee. Such a run
/// keeps no session: it cannot go on in the conversation's.
pub fn pins_session(profile: &ToolProfile) -> bool {
    matches!(profile, ToolProfile::Work { browser: true, browser_session: Some(id), playwright: None, .. } if is_session_id(id))
}

/// The session part of the arguments: nothing kept (as `profile_arguments` has
/// it), a new session named `id`, or carry on in session `id`. A pinned session
/// id stays the only one: Claude Code takes one `--session-id`, and forking a
/// run's session into the employee's id fails once that id exists (CLI 2.1.289).
pub fn apply_session(args: &mut Vec<String>, session: SessionUse, id: &str) {
    let flag = match session {
        SessionUse::None => return,
        SessionUse::Keep => "--session-id",
        SessionUse::Continue | SessionUse::Resume => "--resume",
    };
    if args.iter().any(|a| a == "--session-id") {
        return;
    }
    args.retain(|a| a != "--no-session-persistence");
    args.push(flag.into());
    args.push(id.into());
}

/// The prompt of a run that carries on in its own session after a usage limit:
/// the session already holds the request and the work so far.
pub const RESUME_PROMPT: &str = "(You were interrupted by a Claude usage limit, which has reset now. Whatever you already did is still in place: carry on where you stopped instead of starting over, and finish with your answer as usual.)";

/// A run's system text on disk; removed when the run is over (or never started).
struct SystemFile(PathBuf);

impl SystemFile {
    async fn write(run_root: &Path, run_id: &str, text: &str) -> Result<Self, RuntimeError> {
        let dir = run_root.join("system");
        tokio::fs::create_dir_all(&dir).await.map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
        let path = dir.join(format!("{run_id}.md"));
        tokio::fs::write(&path, text).await.map_err(|e| RuntimeError::Unavailable(format!("could not write the system text: {e}")))?;
        Ok(Self(path))
    }
}

impl Drop for SystemFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Render the conversation as one prompt: the whole conversation for a new
/// session, only what is new for a run that goes on in the conversation's session.
pub fn render_prompt(turns: &[Turn]) -> String {
    match turns {
        [] => String::new(),
        [only] => only.text.clone(),
        many => {
            let mut out = String::from("Earlier in this conversation:\n\n");
            for turn in &many[..many.len() - 1] {
                let who = if turn.role == TurnRole::User { "User" } else { "Assistant" };
                out.push_str(&format!("{who}: {}\n\n", turn.text));
            }
            out.push_str("Now answer this latest message from the user:\n\n");
            out.push_str(&many[many.len() - 1].text);
            out
        }
    }
}

/// The prompt as one stream-json user message, for a conversation with images
/// (text input can't carry them): the images in order, then the rendered text,
/// which names them. `None` without images: the prompt goes in as plain text.
pub fn image_message(turns: &[Turn]) -> Option<String> {
    turns.iter().any(|t| !t.images.is_empty()).then(|| user_message(turns))
}

/// The prompt as one stream-json user message: images (if any) in order, then
/// the rendered text.
pub fn user_message(turns: &[Turn]) -> String {
    let images: Vec<&crate::ImagePart> = turns.iter().flat_map(|t| &t.images).collect();
    let mut content: Vec<serde_json::Value> = images
        .iter()
        .map(|i| serde_json::json!({ "type": "image", "source": { "type": "base64", "media_type": i.media_type, "data": i.base64() } }))
        .collect();
    let text = render_prompt(turns);
    if !text.trim().is_empty() {
        // A slash command only runs when it starts the prompt: its text goes before the images.
        let at = if text.starts_with('/') { 0 } else { content.len() };
        content.insert(at, serde_json::json!({ "type": "text", "text": text }));
    }
    serde_json::json!({ "type": "user", "message": { "role": "user", "content": content } }).to_string()
}

/// What a run sent, for the trace: the command line without the MCP configuration
/// (it can hold the user's keys; only its server names stay), the session, and the
/// prompt with its images by name and size.
pub fn sent_request(args: &[String], session: SessionUse, turns: &[Turn]) -> crate::SentRequest {
    let mut mcp_servers = Vec::new();
    let mut shown = args.to_vec();
    if let Some(i) = shown.iter().position(|a| a == "--mcp-config") {
        if let Some(config) = shown.get_mut(i + 1) {
            let value: serde_json::Value = serde_json::from_str(config).unwrap_or_default();
            mcp_servers = value["mcpServers"].as_object().map(|m| m.keys().cloned().collect()).unwrap_or_default();
            // serde_json keeps insertion order here; sort so the trace reads the same every run.
            mcp_servers.sort();
            *config = format!("<{} MCP server(s): {}>", mcp_servers.len(), mcp_servers.join(", "));
        }
    }
    let session = match session {
        SessionUse::None => "none",
        SessionUse::Keep => "new",
        SessionUse::Continue => "continue",
        SessionUse::Resume => "resume",
    };
    crate::SentRequest {
        args: shown,
        session: session.into(),
        prompt: render_prompt(turns),
        images: turns.iter().flat_map(|t| &t.images).map(|i| crate::SentImage { name: i.name.clone(), media_type: i.media_type.clone(), bytes: i.data.len() as u64 }).collect(),
        mcp_servers,
    }
}

/// A mode switch for the running session (stream-json control request).
/// Claude Code accepts acceptEdits, auto, bypassPermissions, default, dontAsk
/// and plan here (CLI 2.1.287); `manual` is its `default`, bypass never goes in.
pub fn permission_mode_request(request_id: &str, mode: &str) -> Option<String> {
    let mode = match mode {
        "manual" | "default" => "default",
        "acceptEdits" | "auto" | "plan" => mode,
        _ => return None,
    };
    Some(serde_json::json!({ "type": "control_request", "request_id": request_id, "request": { "subtype": "set_permission_mode", "mode": mode } }).to_string())
}

impl ClaudeCodeRuntime {
    fn command(&self, args: &[String]) -> Command {
        let mut command = Command::new(&self.executable);
        command.args(args).env_clear().kill_on_drop(true);
        for key in ENV_ALLOWLIST {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        // A run is one turn: it ends at the first `result` and the job object
        // kills what is left. A background subagent or shell would answer after
        // that, into nothing ("I'll wait for the agent…"). In the foreground the
        // turn waits for it (CLI 2.1.287).
        command.env(DISABLE_BACKGROUND_TASKS, "1");
        // Subagents search and read: a cheaper model does that well enough.
        if let Some(model) = subagent_model(std::env::var(SUBAGENT_MODEL_SETTING).ok().as_deref(), None) {
            command.env(SUBAGENT_MODEL, model);
        }
        command.env(CARGO_BUILD_JOBS, cargo_build_jobs().to_string());
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
    }

    /// `claude --version`: proves the binary runs. Does not send a prompt.
    pub async fn version(&self) -> Result<String, RuntimeError> {
        let output = self
            .command(&["--version".into()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| RuntimeError::Unavailable(format!("{}: {e}", self.executable.display())))?;
        if !output.status.success() {
            return Err(RuntimeError::Unavailable(String::from_utf8_lossy(&output.stderr).trim().to_string()));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Plan limits via the local `/usage` slash command: answered by the CLI
    /// itself (synthetic message, 0 tokens, no cost), with the user's own
    /// login. Same closed profile as runs: no tools, no MCP servers, no session.
    pub async fn subscription_usage(&self) -> Result<String, RuntimeError> {
        tokio::fs::create_dir_all(&self.run_root).await.map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
        let args: Vec<String> = [
            "-p", "/usage", "--output-format", "json", "--tools", "", "--disallowedTools", "mcp__*",
            "--strict-mcp-config", "--mcp-config", r#"{"mcpServers":{}}"#, "--safe-mode", "--no-session-persistence",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let output = self
            .command(&args)
            .current_dir(&self.run_root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
        let value: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|_| RuntimeError::Unavailable(String::from_utf8_lossy(&output.stderr).trim().chars().take(300).collect()))?;
        value["result"].as_str().map(str::to_owned).ok_or_else(|| RuntimeError::Unavailable("Claude Code returned no usage overview.".into()))
    }

    /// `claude auth status --json` → subscription type (e.g. "max"). No prompt, no cost.
    pub async fn subscription_type(&self) -> Option<String> {
        let output = self.command(&["auth".into(), "status".into(), "--json".into()]).stdout(Stdio::piped()).stderr(Stdio::null()).output().await.ok()?;
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
        value["subscriptionType"].as_str().map(str::to_owned)
    }

    /// `claude auth status`: exit 0 when signed in, 1 when not. No prompt, no cost.
    pub async fn signed_in(&self) -> Result<bool, RuntimeError> {
        let status = self
            .command(&["auth".into(), "status".into()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
        Ok(status.success())
    }
}

#[async_trait::async_trait]
impl AgentRuntime for ClaudeCodeRuntime {
    async fn probe(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            runtime_kind: "claude_code".into(),
            version: self.version().await.unwrap_or_else(|_| "unknown".into()),
            streaming: Capability::Supported,
            cancel: Capability::Supported,
            // A conversation goes on in its session; a run that waited for a usage limit carries on in its.
            resume: Capability::Supported,
            // Claude Code's own tools, under a profile per run (Aurelius adds no tools of its own).
            tools: Capability::Supported,
            native_sandbox: Capability::Unknown,
        }
    }

    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        // An employee works in its project folder; everything else in a scratch folder.
        let workdir = match &request.profile {
            ToolProfile::Work { folders, .. } if !folders.is_empty() => folders[0].clone(),
            _ => {
                let dir = self.run_root.join(request.run_id.to_string());
                tokio::fs::create_dir_all(&dir).await.map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
                dir
            }
        };
        let model = self.model.as_deref().or(Some(request.model_id.as_str())).filter(|m| !m.starts_with("fake"));
        let system_file = match request.system.as_deref().filter(|s| !s.trim().is_empty()) {
            Some(text) => Some(SystemFile::write(&self.run_root, &request.run_id.to_string(), text).await?),
            None => None,
        };
        let mut args = profile_arguments(&request.profile, model, system_file.as_ref().map(|f| f.0.as_path()), request.effort.as_deref());
        // The conversation's session, or the run's own (named after the run).
        // Resumed only if Claude Code still has it; otherwise a fresh one, and the
        // turns carry the context. A browser run keeps its employee's session id and
        // stores nothing, so after a usage limit it starts over from the turns.
        let session_id = request.session_id.clone().filter(|id| is_session_id(id)).unwrap_or_else(|| request.run_id.to_string());
        let claude_dir = session::claude_dir();
        let session = match (request.session, claude_dir.as_deref()) {
            _ if pins_session(&request.profile) => SessionUse::None,
            (SessionUse::None, _) | (_, None) => SessionUse::None,
            (SessionUse::Resume, Some(dir)) if session::transcript(dir, &session_id).is_some() => SessionUse::Resume,
            // The caller checked it is there; the turns are only what is new, so
            // without it this run cannot start over: it is retried as a new session.
            (SessionUse::Continue, Some(dir)) if session::transcript(dir, &session_id).is_some() => SessionUse::Continue,
            (SessionUse::Continue, Some(_)) => {
                return Err(RuntimeError::Unavailable("the conversation's Claude Code session is gone; it starts over".into()));
            }
            (requested, Some(dir)) => {
                if requested == SessionUse::Resume {
                    tracing::warn!(run = %request.run_id, "the session of the earlier attempt is gone; starting over from the conversation");
                }
                // A leftover (e.g. the service stopped mid-run) would block `--session-id`.
                session::remove(dir, &session_id);
                SessionUse::Keep
            }
        };
        apply_session(&mut args, session, &session_id);
        let resume_turns = [Turn { role: TurnRole::User, text: RESUME_PROMPT.into(), images: Vec::new() }];
        let turns: &[Turn] = if session == SessionUse::Resume { &resume_turns } else { &request.turns };
        // Work keeps stdin open for mode switches; otherwise only images need stream-json.
        let work = matches!(request.profile, ToolProfile::Work { .. });
        let message = if work { Some(user_message(turns)) } else { image_message(turns) };
        if message.is_some() {
            if let Some(i) = args.iter().position(|a| a == "--input-format") {
                args[i + 1] = "stream-json".into();
            }
        }
        let sent = sent_request(&args, session, turns);
        let mut command = self.command(&args);
        if matches!(request.profile, ToolProfile::Work { artifacts: true, .. }) {
            command.env(ARTIFACT_ENV, "1").env(ARTIFACT_AUTO_OPEN_ENV, "0");
        }
        // The subagent model chosen in Aurelius (employee, project or global).
        match subagent_model(std::env::var(SUBAGENT_MODEL_SETTING).ok().as_deref(), request.subagent_model.as_deref()) {
            Some(model) => command.env(SUBAGENT_MODEL, model),
            None => command.env_remove(SUBAGENT_MODEL),
        };
        let mut child = command
            .current_dir(&workdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RuntimeError::Unavailable(format!("could not start {}: {e}", self.executable.display())))?;
        // Stopping the run must also end Claude Code's own children (aurelius-ctl, shells).
        let tree = job::ProcessTree::adopt(&child);

        let mut stdin = child.stdin.take().expect("piped stdin");
        let prompt = match message {
            Some(line) => line + "\n",
            None => render_prompt(turns),
        };
        stdin.write_all(prompt.as_bytes()).await.map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
        // EOF: the prompt is complete. Work closes it once the result is in.
        let mut stdin = if work {
            stdin.flush().await.map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
            Some(stdin)
        } else {
            None
        };

        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let (tx, rx) = mpsc::channel(EVENT_BUFFER);
        // Claude Code runs its own tools; only mode switches are sent in.
        let (input_tx, mut input_rx) = mpsc::channel::<RuntimeInput>(8);
        let mut control_requests = 0u32;
        let profile = request.profile.clone();
        let (cancel_tx, mut cancel_rx) = watch::channel(false);
        // Removed when the run is over, unless it stopped for a usage limit or
        // completed in a session the conversation goes on in.
        let keep_completed = request.keep_session;
        let kept_session = claude_dir.filter(|_| session != SessionUse::None).map(|dir| (dir, session_id));
        let forget_session = move || {
            if let Some((dir, id)) = &kept_session {
                session::remove(dir, id);
            }
        };

        tokio::spawn(async move {
            // Removed when this task ends: Claude Code read it at start.
            let _system_file = system_file;
            let _ = tx.send(RuntimeEvent::Sent(Box::new(sent))).await;
            let mut lines = BufReader::new(stdout).lines();
            let mut state = parse::StreamState::for_profile(&profile);
            let stderr_task = tokio::spawn(async move {
                let mut buf = String::new();
                let mut reader = BufReader::new(stderr);
                let _ = tokio::io::AsyncReadExt::read_to_string(&mut reader, &mut buf).await;
                buf
            });
            let terminal = loop {
                tokio::select! {
                    line = lines.next_line() => match line {
                        Ok(Some(line)) => {
                            match state.feed(&line) {
                                Ok(events) => {
                                    let mut done = None;
                                    for event in events {
                                        let terminal = matches!(event, RuntimeEvent::Completed | RuntimeEvent::Failed { .. } | RuntimeEvent::Quota { .. });
                                        if tx.send(event.clone()).await.is_err() {
                                            done = Some(None);
                                            break;
                                        }
                                        if terminal {
                                            state.failure_sent = matches!(event, RuntimeEvent::Failed { .. } | RuntimeEvent::Quota { .. });
                                            done = Some(Some(event));
                                        }
                                    }
                                    if let Some(d) = done {
                                        break d;
                                    }
                                }
                                Err(violation) => {
                                    // Profile violation (tools/plugins loaded): fail closed.
                                    let _ = child.start_kill();
                                    break Some(RuntimeEvent::Failed { code: "profile_violation".into(), message: violation });
                                }
                            }
                        }
                        Ok(None) => break None,
                        Err(e) => break Some(RuntimeEvent::Failed { code: "stream_read".into(), message: e.to_string() }),
                    },
                    _ = cancel_rx.changed() => {
                        if let Some(tree) = &tree {
                            tree.kill();
                        }
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                        forget_session();
                        let _ = tx.send(RuntimeEvent::Interrupted).await;
                        return;
                    }
                    input = input_rx.recv(), if stdin.is_some() => match input {
                        Some(RuntimeInput::SetPermissionMode { mode }) => {
                            control_requests += 1;
                            if let (Some(pipe), Some(line)) = (stdin.as_mut(), permission_mode_request(&format!("mode-{control_requests}"), &mode)) {
                                if pipe.write_all(format!("{line}\n").as_bytes()).await.is_err() || pipe.flush().await.is_err() {
                                    stdin = None;
                                }
                            }
                        }
                        // Nobody can send any more: let Claude Code finish on its own.
                        None => stdin = None,
                    },
                }
            };
            drop(stdin); // the result is in (or the stream ended): Claude Code may exit
            let status = child.wait().await;
            // Leftover children could hold stderr open and outlive the run.
            drop(tree);
            let stderr_text = stderr_task.await.unwrap_or_default();
            let limited = match &terminal {
                Some(event) => matches!(event, RuntimeEvent::Quota { .. }),
                None => parse::quota_in(&stderr_text).is_some(),
            };
            let completed = matches!(terminal, Some(RuntimeEvent::Completed));
            if !keeps_session_after(limited, completed, keep_completed) {
                forget_session();
            }
            match terminal {
                Some(event @ RuntimeEvent::Failed { .. }) if !state.failure_sent => {
                    let _ = tx.send(event).await;
                }
                Some(_) => {}
                // The stream broke off but stderr says why: the limit, not a failure.
                None if parse::quota_in(&stderr_text).is_some() => {
                    let _ = tx.send(RuntimeEvent::Quota { reset_at_ms: parse::quota_in(&stderr_text).flatten() }).await;
                }
                None => {
                    // No result line: never success.
                    let detail = if stderr_text.trim().is_empty() {
                        format!("Claude Code stopped without a result ({})", status.map(|s| s.to_string()).unwrap_or_default())
                    } else {
                        stderr_text.trim().chars().take(500).collect()
                    };
                    let _ = tx.send(RuntimeEvent::Failed { code: "stream_incomplete".into(), message: detail }).await;
                }
            }
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }

    fn keeps_sessions(&self) -> bool {
        true
    }

    fn has_session(&self, id: &str) -> bool {
        is_session_id(id) && session::claude_dir().is_some_and(|dir| session::transcript(&dir, id).is_some())
    }

    fn remove_session(&self, id: &str) {
        if let (true, Some(dir)) = (is_session_id(id), session::claude_dir()) {
            session::remove(&dir, id);
        }
    }
}

/// Whether a run's session stays after it ended: for the next attempt after a
/// usage limit, or for the conversation's next run once it completed in it.
fn keeps_session_after(limited: bool, completed: bool, keep_completed: bool) -> bool {
    limited || (completed && keep_completed)
}

/// One limit from the `/usage` text, e.g. "Current session: 7% used · resets Oct 2, 12:09pm (Europe/Brussels)".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanLimit {
    pub label: String,
    pub percent: u8,
    pub resets: Option<String>,
}

/// Tolerant parser: the `/usage` text is not a documented format. Lines that
/// do not match are ignored; callers fall back to showing the raw text.
pub fn parse_plan_limits(text: &str) -> Vec<PlanLimit> {
    text.lines()
        .filter_map(|line| {
            let (label, rest) = line.trim().split_once(':')?;
            let (percent, after) = rest.split_once("% used")?;
            let percent: u8 = percent.trim().parse().ok()?;
            let resets = after.split_once("resets ").map(|(_, r)| r.trim().to_owned());
            let label = match label.trim() {
                "Current session" => "Session (5 hours)".to_owned(),
                "Current week (all models)" => "Week (all models)".to_owned(),
                other => match other.strip_prefix("Current week (").and_then(|s| s.strip_suffix(')')) {
                    Some(model) => format!("Week ({model})"),
                    None => other.to_owned(),
                },
            };
            Some(PlanLimit { label, percent: percent.min(100), resets })
        })
        .collect()
}

#[cfg(test)]
mod usage_tests {
    use super::*;

    #[test]
    fn parses_the_usage_text() {
        let text = "You are currently using your subscription\n\nCurrent session: 7% used · resets Oct 2, 12:09pm (Europe/Brussels)\nCurrent week (all models): 55% used · resets Oct 3, 7:59am (Europe/Brussels)\nCurrent week (Fable): 2% used · resets Oct 3, 7:59am (Europe/Brussels)\n\nLast 24h · 1592 requests";
        let limits = parse_plan_limits(text);
        assert_eq!(limits.len(), 3);
        assert_eq!(limits[0], PlanLimit { label: "Session (5 hours)".into(), percent: 7, resets: Some("Oct 2, 12:09pm (Europe/Brussels)".into()) });
        assert_eq!(limits[1].label, "Week (all models)");
        assert_eq!(limits[2].label, "Week (Fable)");
        assert!(parse_plan_limits("iets heel anders").is_empty());
    }
}

#[cfg(test)]
mod image_tests {
    use super::*;
    use crate::ImagePart;

    #[test]
    fn images_go_in_as_one_stream_json_user_message() {
        let text = |role, t: &str| Turn { role, text: t.into(), images: Vec::new() };
        assert_eq!(image_message(&[text(TurnRole::User, "Hoi")]), None);

        let shot = ImagePart { name: "shot.png".into(), media_type: "image/png".into(), data: vec![1, 2, 3] };
        let turns = vec![
            Turn { role: TurnRole::User, text: "[Image 1: shot.png]\nKijk".into(), images: vec![shot] },
            text(TurnRole::Assistant, "Ik zie een knop."),
            text(TurnRole::User, "En nu?"),
        ];
        let line = image_message(&turns).expect("a message with the image");
        assert!(!line.contains('\n'), "one line on stdin");
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["type"], "user");
        let content = &value["message"]["content"];
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["source"]["data"], "AQID");
        assert_eq!(content[1]["text"], render_prompt(&turns));

        // A slash command must start the prompt: its text goes before the images.
        let command = vec![Turn { role: TurnRole::User, text: "/pdf:pdf make one".into(), images: vec![ImagePart { name: "a.png".into(), media_type: "image/png".into(), data: vec![1] }] }];
        let value: serde_json::Value = serde_json::from_str(&user_message(&command)).unwrap();
        assert_eq!((value["message"]["content"][0]["text"].as_str(), value["message"]["content"][1]["type"].as_str()), (Some("/pdf:pdf make one"), Some("image")));
    }
}

#[cfg(test)]
mod mode_switch_tests {
    use super::*;

    #[test]
    fn the_trace_of_what_was_sent_leaves_the_mcp_configuration_out() {
        let bridge = crate::ApprovalBridge { program: "ctl".into(), args: vec!["mcp-permissions".into()] };
        let mut servers = std::collections::BTreeMap::new();
        servers.insert("notes".to_string(), serde_json::json!({ "type": "http", "url": "https://x", "headers": { "Authorization": "Bearer SECRET" } }));
        let p = ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge, browser: false, browser_session: None, playwright: None, mcp_servers: servers, plugins: vec![], artifacts: false };
        let mut args = profile_arguments(&p, Some("claude-opus-5-5"), None, None);
        apply_session(&mut args, SessionUse::Continue, "0192f1a2-3b4c-7d5e-8f60-718293a4b5c6");
        let image = crate::ImagePart { name: "a.png".into(), media_type: "image/png".into(), data: vec![0; 10] };
        let turns = [Turn { role: TurnRole::User, text: "[Image 1: a.png]
Look".into(), images: vec![image] }];
        let sent = sent_request(&args, SessionUse::Continue, &turns);
        let joined = sent.args.join(" ");
        assert!(!joined.contains("SECRET") && joined.contains("<2 MCP server(s): aurelius, notes>"), "{joined}");
        assert!(joined.contains("--resume 0192f1a2") && joined.contains("--model claude-opus-5-5"));
        assert_eq!((sent.session.as_str(), sent.prompt.as_str()), ("continue", "[Image 1: a.png]
Look"));
        assert_eq!(sent.images, vec![crate::SentImage { name: "a.png".into(), media_type: "image/png".into(), bytes: 10 }]);
        assert_eq!(sent.mcp_servers, ["aurelius", "notes"]);
    }

    #[test]
    fn work_reads_stream_json_so_a_mode_switch_can_go_in() {
        let bridge = ApprovalBridge { program: "x".into(), args: vec![] };
        let work = ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: "plan".into(), approval_bridge: bridge, browser: false, browser_session: None, playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts: false };
        let input = |p: &ToolProfile| {
            let args = profile_arguments(p, None, None, None);
            args[args.iter().position(|a| a == "--input-format").unwrap() + 1].clone()
        };
        assert_eq!(input(&work), "stream-json");
        assert_eq!(input(&ToolProfile::TextOnly), "text");
        // Without images the message carries just the text.
        let line = user_message(&[Turn { role: TurnRole::User, text: "Hoi".into(), images: Vec::new() }]);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&line).unwrap()["message"]["content"], serde_json::json!([{ "type": "text", "text": "Hoi" }]));
    }

    #[test]
    fn a_run_keeps_or_resumes_its_own_session_only_when_asked() {
        let id = "0199c3a2-0000-7000-8000-000000000001";
        let with = |session| {
            let mut args = profile_arguments(&ToolProfile::TextOnly, None, None, None);
            apply_session(&mut args, session, id);
            args
        };
        let none = with(SessionUse::None);
        assert!(none.contains(&"--no-session-persistence".to_string()));
        assert!(!none.iter().any(|a| a == "--session-id" || a == "--resume"));
        let keep = with(SessionUse::Keep);
        assert!(!keep.contains(&"--no-session-persistence".to_string()), "a kept session is written to disk");
        assert_eq!(keep[keep.len() - 2..], ["--session-id".to_string(), id.to_string()]);
        let resume = with(SessionUse::Resume);
        assert!(!resume.contains(&"--no-session-persistence".to_string()));
        assert_eq!(resume[resume.len() - 2..], ["--resume".to_string(), id.to_string()]);
        assert!(!resume.contains(&"--session-id".to_string()));
        // Going on in the conversation's session is a resume of it too.
        let go_on = with(SessionUse::Continue);
        assert!(!go_on.contains(&"--no-session-persistence".to_string()));
        assert_eq!(go_on[go_on.len() - 2..], ["--resume".to_string(), id.to_string()]);
    }

    #[test]
    fn a_session_stays_for_a_usage_limit_or_when_the_conversation_goes_on_in_it() {
        assert!(keeps_session_after(true, false, false), "the next attempt carries on after the limit");
        assert!(keeps_session_after(true, false, true));
        assert!(keeps_session_after(false, true, true), "the conversation's next run goes on in it");
        assert!(!keeps_session_after(false, true, false), "a run's own session goes when it completed");
        assert!(!keeps_session_after(false, false, true), "stopped or failed: it may hold half a turn");
    }

    #[test]
    fn only_a_claude_in_chrome_run_pins_the_session() {
        let employee = "0199c3a2-0000-7000-8000-00000000000a";
        let work = |browser, playwright: Option<serde_json::Value>| ToolProfile::Work {
            folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: ApprovalBridge { program: "x".into(), args: vec![] },
            browser, browser_session: Some(employee.into()), playwright, mcp_servers: Default::default(), plugins: vec![], artifacts: false,
        };
        assert!(pins_session(&work(true, None)));
        // Playwright takes no --session-id: such a run can keep the conversation's session.
        assert!(!pins_session(&work(true, Some(serde_json::json!({ "type": "stdio" })))));
        assert!(!pins_session(&work(false, None)));
    }

    #[test]
    fn no_command_line_has_two_session_ids() {
        let employee = "0199c3a2-0000-7000-8000-00000000000a";
        let run = "0199c3a2-0000-7000-8000-000000000001";
        let work = |browser, browser_session: Option<&str>| ToolProfile::Work {
            folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: ApprovalBridge { program: "x".into(), args: vec![] },
            browser, browser_session: browser_session.map(Into::into), playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts: false,
        };
        let profiles = [ToolProfile::TextOnly, work(false, None), work(false, Some(employee)), work(true, None), work(true, Some(employee)), work(true, Some("not-a-uuid"))];
        for profile in &profiles {
            for session in [SessionUse::None, SessionUse::Keep, SessionUse::Continue, SessionUse::Resume] {
                let mut args = profile_arguments(profile, None, None, None);
                apply_session(&mut args, session, run);
                let ids = args.iter().filter(|a| *a == "--session-id" || *a == "--resume").count();
                assert!(ids <= 1, "{session:?}: {args:?}");
            }
        }
        // A browser run keeps the employee's id and stores nothing, also when asked to keep or resume.
        let pinned = work(true, Some(employee));
        assert!(pins_session(&pinned));
        assert!(!pins_session(&work(true, Some("not-a-uuid"))) && !pins_session(&work(false, Some(employee))));
        for session in [SessionUse::Keep, SessionUse::Continue, SessionUse::Resume] {
            let mut args = profile_arguments(&pinned, None, None, None);
            apply_session(&mut args, session, run);
            assert_eq!(args[args.iter().position(|a| a == "--session-id").unwrap() + 1], employee);
            assert!(args.contains(&"--no-session-persistence".to_string()) && !args.contains(&run.to_string()), "{args:?}");
        }
    }

    #[test]
    fn a_dispatcher_run_has_only_its_bridge_tools_and_a_short_budget() {
        let bridge = crate::ApprovalBridge { program: "/x/aurelius-bridge".into(), args: vec!["--run".into(), "r".into(), "--triage".into()] };
        let profile = ToolProfile::Triage { approval_bridge: bridge, tools: vec!["mcp__aurelius__assign_ticket".into(), "mcp__aurelius__recall".into()], max_turns: 3 };
        let args = profile_arguments(&profile, Some("haiku"), None, None);
        let after = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert_eq!(after("--tools"), "", "no built-in tool");
        assert_eq!(after("--allowedTools"), "mcp__aurelius__assign_ticket,mcp__aurelius__recall");
        assert_eq!(after("--max-turns"), "3");
        assert_eq!(after("--permission-prompts"), "none");
        assert_eq!(after("--input-format"), "text");
        let mcp: serde_json::Value = serde_json::from_str(&after("--mcp-config")).unwrap();
        assert_eq!(mcp["mcpServers"].as_object().unwrap().keys().collect::<Vec<_>>(), ["aurelius"], "only the bridge");
        assert_eq!(mcp["mcpServers"]["aurelius"]["args"][2], "--triage");
        assert!(args.contains(&"--strict-mcp-config".to_string()) && !args.contains(&"--permission-prompt-tool".to_string()));
        assert!(!args.contains(&"--safe-mode".to_string()), "safe mode would leave the bridge out");
    }

    #[test]
    fn a_mode_switch_is_a_control_request_and_never_a_bypass() {
        let line = permission_mode_request("mode-1", "acceptEdits").unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&line).unwrap(),
            serde_json::json!({ "type": "control_request", "request_id": "mode-1", "request": { "subtype": "set_permission_mode", "mode": "acceptEdits" } })
        );
        // Claude Code calls manual "default" on this channel (CLI 2.1.287).
        assert!(permission_mode_request("m", "manual").unwrap().contains(r#""mode":"default""#));
        assert!(permission_mode_request("m", "plan").is_some());
        assert_eq!(permission_mode_request("m", "bypassPermissions"), None);
        assert_eq!(permission_mode_request("m", "dontAsk"), None);
    }

    #[tokio::test]
    async fn the_system_text_lives_in_a_file_for_the_run_only() {
        let root = std::env::temp_dir().join(format!("aurelius-system-{}", std::process::id()));
        let long = "remember this. ".repeat(4_000); // ~60 000 chars: too long for a Windows command line
        let file = SystemFile::write(&root, "run-1", &long).await.unwrap();
        let path = file.0.clone();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), long);
        let args = profile_arguments(&ToolProfile::TextOnly, None, Some(&path), None);
        assert!(args.iter().map(String::len).sum::<usize>() < 2_000, "the command line stays short");
        drop(file);
        assert!(!path.exists(), "removed after the run");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn every_child_runs_without_background_tasks() {
        let runtime = ClaudeCodeRuntime { executable: "claude".into(), run_root: "runs".into(), model: None };
        let command = runtime.command(&["--version".into()]);
        let value = command.as_std().get_envs().find(|(k, _)| *k == DISABLE_BACKGROUND_TASKS).and_then(|(_, v)| v);
        assert_eq!(value, Some(std::ffi::OsStr::new("1")));
    }

    #[test]
    fn subagents_run_on_sonnet_unless_the_daemon_says_otherwise() {
        assert_eq!(subagent_model(None, None).as_deref(), Some("sonnet"));
        assert_eq!(subagent_model(Some("haiku"), None).as_deref(), Some("haiku"));
        assert_eq!(subagent_model(Some(" "), None), None, "empty: the CLI decides");
        // The user's choice in Aurelius, unless the daemon's setting says otherwise.
        assert_eq!(subagent_model(None, Some("haiku")).as_deref(), Some("haiku"));
        assert_eq!(subagent_model(None, Some("")), None, "set to Disabled: the CLI decides");
        assert_eq!(subagent_model(Some("opus"), Some("haiku")).as_deref(), Some("opus"));
        // The child gets it (this test binary does not set AURELIUS_SUBAGENT_MODEL).
        let runtime = ClaudeCodeRuntime { executable: "claude".into(), run_root: "runs".into(), model: None };
        let command = runtime.command(&["--version".into()]);
        let value = command.as_std().get_envs().find(|(k, _)| *k == SUBAGENT_MODEL).and_then(|(_, v)| v);
        let expected = subagent_model(std::env::var(SUBAGENT_MODEL_SETTING).ok().as_deref(), None);
        assert_eq!(value.and_then(|v| v.to_str()).map(str::to_owned), expected);
    }

    #[test]
    fn every_child_builds_with_half_the_cores() {
        let runtime = ClaudeCodeRuntime { executable: "claude".into(), run_root: "runs".into(), model: None };
        let command = runtime.command(&["--version".into()]);
        let value = command.as_std().get_envs().find(|(k, _)| *k == CARGO_BUILD_JOBS).and_then(|(_, v)| v);
        let jobs: usize = value.and_then(|v| v.to_str()).and_then(|v| v.parse().ok()).unwrap();
        let cores = std::thread::available_parallelism().unwrap().get();
        assert_eq!(jobs, (cores / 2).max(1));
    }
}
