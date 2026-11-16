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
//! an allowlisted environment and exactly one Claude login: the board member's
//! own config folder (`login`), never the daemon user's `~/.claude`.

mod job;
pub mod login;
pub mod parse;
pub mod session;

pub use login::ClaudeLogin;

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
    /// Whose Claude login the child uses: a run always has one (it never starts
    /// without, `start` refuses). Without one (`--version`) the child gets an
    /// empty config folder of Aurelius' own, never `~/.claude`.
    pub login: Option<ClaudeLogin>,
    /// The vault's values this run gets as environment variables (E7, E15):
    /// only a work run, after the allow-list and before Aurelius' own.
    pub vault: VaultEnv,
}

/// One environment variable from the vault. Never printed.
#[derive(Clone)]
pub struct VaultVar {
    pub name: String,
    pub value: zeroize::Zeroizing<String>,
    /// A secret: delivered only when the redactor knows its value.
    pub secret: bool,
}

/// Environment variables from the vault, never printed. Set by the daemon on
/// a runtime for one run; nothing of it is in `RunRequest` (Debug, traces).
#[derive(Clone, Default)]
pub struct VaultEnv(pub std::sync::Arc<Vec<VaultVar>>);

impl VaultEnv {
    pub fn new(vars: Vec<VaultVar>) -> Self {
        Self(std::sync::Arc::new(vars))
    }

    pub fn names(&self) -> Vec<&str> {
        self.0.iter().map(|v| v.name.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for VaultEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("VaultEnv").field(&self.names()).finish()
    }
}

/// Environment variables passed to the child. Everything else (including any
/// API keys, tokens or a `CLAUDE_CONFIG_DIR` in the daemon's environment) is
/// left out; the child gets its login only through `ClaudeCodeRuntime::login`.
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
/// The built-in tools of a read-only employee (C3): it reads and searches, and it
/// can still ask the user and hand in a plan. No `Task`: a subagent is one more way
/// to act, and a level is an explicit list.
pub const READ_ONLY_TOOLS: &[&str] = &["Read", "Glob", "Grep", "AskUserQuestion", "ExitPlanMode"];
/// The built-in tools of an employee that may also change files.
pub const READ_EDIT_TOOLS: &[&str] = &["Read", "Glob", "Grep", "Edit", "Write", "AskUserQuestion", "ExitPlanMode"];

/// The built-in tools of a level; a level this build does not know gets the narrowest.
pub fn tools_for(access: aurelius_domain::employee::AccessLevel) -> &'static [&'static str] {
    use aurelius_domain::employee::AccessLevel;
    match access {
        AccessLevel::Inherit => WORK_TOOLS,
        AccessLevel::ReadEdit => READ_EDIT_TOOLS,
        AccessLevel::ReadOnly | AccessLevel::Unknown => READ_ONLY_TOOLS,
    }
}

/// The built-in tools a run has: its level's, without what the employee's own list leaves out (C2).
pub fn work_tools(access: aurelius_domain::employee::AccessLevel, policy: &aurelius_domain::employee_tools::ToolPolicy) -> Vec<&'static str> {
    tools_for(access).iter().copied().filter(|t| policy.allows(t)).collect()
}

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
/// The bridge's tools that are allowed by rule and not asked (the name is older than
/// the list): the employee's own memory and scratchpad, reading its work and its
/// actions in the company. An employee's tool policy (C2) can switch off the ones
/// in `SWITCHABLE_TOOLS` or make them ask first; they then leave `--allowedTools`
/// and `permissions::ask` puts them in the inbox like any other tool.
pub const MEMORY_TOOLS: &[&str] = &["mcp__aurelius__remember", "mcp__aurelius__recall", "mcp__aurelius__delegate_task", "mcp__aurelius__message_task", "mcp__aurelius__hand_over_task", "mcp__aurelius__list_tasks", "mcp__aurelius__read_task", "mcp__aurelius__list_conversations", "mcp__aurelius__read_conversation", "mcp__aurelius__propose_colleague", "mcp__aurelius__propose_instructions", "mcp__aurelius__propose_guideline", "mcp__aurelius__list_guidelines", "mcp__aurelius__read_guideline", "mcp__aurelius__suggest_replies", "mcp__aurelius__set_mode", "mcp__aurelius__read_group_chat", "mcp__aurelius__post_to_group_chat", "mcp__aurelius__read_discussion", "mcp__aurelius__post_to_discussion", "mcp__aurelius__vault_list", "mcp__aurelius__vault_read", "mcp__aurelius__list_tickets", "mcp__aurelius__read_ticket", "mcp__aurelius__create_ticket", "mcp__aurelius__update_ticket", "mcp__aurelius__move_ticket", "mcp__aurelius__post_to_ticket", "mcp__aurelius__hand_off", "mcp__aurelius__tell_responsible", "mcp__aurelius__schedule_message", "mcp__aurelius__scratchpad", "mcp__aurelius__report_ready", "mcp__aurelius__report_test_result"];
/// Claude Code's subagents (`--tools Agent` lists as `Task`, CLI 2.1.287):
/// they inherit the run's tools and ask the same way.
pub const SUBAGENT_TOOL: &str = "Task";
/// Claude Code's MCP tool search: with it in `--tools` the MCP tools (the bridge,
/// the user's servers, the browser) go in as names only and the model loads a
/// definition when it needs one; without it every definition is sent upfront on
/// every request (CLI 2.1.301). It only reads, so it is allowed by rule.
pub const TOOL_SEARCH_TOOL: &str = "ToolSearch";
/// Claude Code's own todo list: the run keeps its plan with it, and a ticket's chat
/// shows that list beside the talk. CLI 2.1.318 keeps it with `TaskCreate` and
/// `TaskUpdate` (one item a call) and drops `TodoWrite` from `--tools` without a word;
/// older versions have only `TodoWrite`. Each offers what it knows. They only touch
/// the run's own list, so every level has them and they are allowed by rule.
pub const TODO_TOOLS: &[&str] = &["TaskCreate", "TaskUpdate", "TaskList", "TaskGet", "TodoWrite"];
/// Claude Code's tool for claude.ai artifacts (the Design System and Design types of
/// Claude Design). Works on the run's Claude login; only with a design link, see `ToolProfile::Work`.
pub const ARTIFACT_TOOL: &str = "Artifact";
/// What `--tools Artifact` brings along (CLI 2.1.318): the comment threads and the database
/// of an artifact, which write. Off (`--disallowedTools`), so the run has only the tool it
/// is offered and the init line lists nothing outside the profile.
pub const ARTIFACT_SIBLINGS_OFF: &str = "ArtifactComments,ArtifactData";

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
        ToolProfile::Work { folders, permission_mode, approval_bridge, browser, browser_session, playwright, mcp_servers, access, tool_policy, artifact } => {
            let listed = work_tools(*access, tool_policy);
            let mut tools = listed.join(",");
            tools.push(',');
            tools.push_str(TOOL_SEARCH_TOOL);
            for todo in TODO_TOOLS {
                tools.push(',');
                tools.push_str(todo);
            }
            if *artifact {
                tools.push(',');
                tools.push_str(ARTIFACT_TOOL);
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
            // What the employee's list has off or asks first is no rule of ours: off it is not
            // there, and asking first needs Claude Code to come to the bridge.
            let own = MEMORY_TOOLS.iter().copied().filter(|t| tool_policy.allows(t) && !tool_policy.needs_approval(t));
            let subagent = (listed.contains(&SUBAGENT_TOOL) && !tool_policy.needs_approval(SUBAGENT_TOOL)).then_some(SUBAGENT_TOOL);
            let artifact_rule = artifact.then_some(ARTIFACT_TOOL);
            let allowed: Vec<&str> = SCREEN_TOOLS.iter().copied().chain(own).chain(subagent).chain([TOOL_SEARCH_TOOL]).chain(TODO_TOOLS.iter().copied()).chain(artifact_rule).collect();
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
            if *artifact {
                push(&mut args, &["--disallowedTools", ARTIFACT_SIBLINGS_OFF]);
            }
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

/// The file tools Sentinel D5 denies on what is not the run's own.
const DENIED_TOOLS: &[&str] = &["Read", "Edit", "Write", "Glob", "Grep"];
/// The tools that search a folder: Claude Code checks the folder their `path`
/// resolves to against the `Read` deny rules (SEC2 M1). `Grep(…)`/`Glob(…)` rules
/// are kept with them: they do no harm if Claude Code only reads `Read(…)` ones.
const SEARCH_TOOLS: &[&str] = &["Read", "Glob", "Grep"];

/// A path as Claude Code's permission rules take an absolute one (`//abs/path`).
fn rule_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    format!("/{}", if text.starts_with('/') { text } else { format!("/{text}") })
}

/// The permission rules a run on `login` gets (Sentinel D5, F2, SEC2 M1).
///
/// - Every file tool is denied what is in another board member's login folder
///   (`<other>/**`), Aurelius' database and the daemon user's own Claude folders.
/// - A search is denied the folder `logins/` itself and each other login folder
///   itself (`Read`/`Grep`/`Glob` on the exact folder): Claude Code checks the
///   folder a Grep or Glob searches against these, so `path = logins` or
///   `path = <other>` is refused for certain, not best-effort.
/// - The run's own login folder stays open (Claude keeps its plans there).
///
/// What this cannot close (SEC2 M1, documented in f4-rapport.md): a search from a
/// folder above `logins/` (`<data>`, the home folder) is checked on that folder
/// only, and whether Claude Code then skips the login folders inside is
/// best-effort. `logins/.ignore` (`ensure_dir`) makes ripgrep skip them, as one
/// more layer that is not relied on. No run works in such a folder unless a
/// project's folder holds the data folder (`holds_logins` warns then). Bash
/// (`grep -r`, a script) is the accepted rest risk of phase 3 (until runs are
/// sandboxed); whether Claude Code enforces all this is proven only by an E2E
/// run on a real login (an open point).
pub fn deny_rules(login: &ClaudeLogin) -> Vec<String> {
    let own = login.config_dir();
    let mut paths: Vec<String> = Vec::new();
    let mut searched: Vec<String> = Vec::new();
    if let Some(logins) = own.parent() {
        searched.push(rule_path(logins));
        for other in std::fs::read_dir(logins).into_iter().flatten().filter_map(Result::ok).filter(|e| e.path().is_dir()).map(|e| e.path()) {
            if other != own {
                paths.push(format!("{}/**", rule_path(&other)));
                searched.push(rule_path(&other));
            }
        }
        if let Some(data) = logins.parent() {
            paths.push(format!("{}*", rule_path(&data.join("data").join("aurelius.sqlite"))));
            // The vault's key on Linux (and every other file secret), E8.
            paths.push(format!("{}/**", rule_path(&data.join("secrets"))));
            searched.push(rule_path(&data.join("secrets")));
        }
    }
    paths.extend(["~/.claude/**", "~/.claude*", "~/.config/claude/**"].map(str::to_owned));
    DENIED_TOOLS
        .iter()
        .flat_map(|tool| paths.iter().map(move |p| format!("{tool}({p})")))
        .chain(SEARCH_TOOLS.iter().flat_map(|tool| searched.iter().map(move |p| format!("{tool}({p})"))))
        .collect()
}

/// Whether a run's work folders hold `logins/` (a project's folder above the
/// data folder): then a search of its own tree reaches the login folders, and
/// only the best-effort part of `deny_rules` keeps them out.
pub fn holds_logins(folders: &[std::path::PathBuf], login: &ClaudeLogin) -> bool {
    let Some(logins) = login.config_dir().parent().and_then(|l| l.canonicalize().ok()) else { return false };
    folders.iter().filter_map(|f| f.canonicalize().ok()).any(|f| logins.starts_with(f))
}

/// Bash commands that print the whole environment (SEC0 E17), denied in every
/// mode as `--settings` rules once a run has vault values. Defence in depth,
/// not a boundary: `/usr/bin/env`, `python -c` or `sh -c` get past them, and
/// the values are redacted from what Aurelius keeps anyway. `curl -H "$TOKEN"`
/// keeps working.
pub const ENV_DUMP_RULES: &[&str] = &[
    "Bash(env)", "Bash(env:*)", "Bash(printenv)", "Bash(printenv:*)", "Bash(set)", "Bash(export -p)", "Bash(export -p:*)",
    "Bash(declare -x)", "Bash(declare -x:*)", "Bash(declare -p)", "Bash(declare -p:*)", "Bash(typeset -x)", "Bash(compgen -v)",
    "Bash(cat /proc/*/environ)", "Bash(cat /proc/*/environ:*)", "Bash(strings /proc/*/environ)", "Bash(strings /proc/*/environ:*)",
    "Bash(xargs -0 < /proc/*/environ)", "Bash(tr:*/proc/*/environ*)",
];

/// `--settings` of a run: its deny rules (D5, E8), and with vault values the
/// rules against printing the environment (E17).
#[cfg(test)]
fn deny_settings(login: &ClaudeLogin, vault: bool) -> String {
    deny_settings_with_plans(login, vault, None, &[])
}

/// The `ask` rules of a run: the built-in tools its employee wants asked first (C2).
/// Without a rule Claude Code decides some of them itself (reads in the folder,
/// edits in Accept edits) and never reaches the bridge.
fn ask_rules(profile: &ToolProfile) -> Vec<String> {
    match profile {
        ToolProfile::Work { tool_policy, .. } => WORK_TOOLS.iter().filter(|t| tool_policy.needs_approval(t)).map(|t| t.to_string()).collect(),
        _ => Vec::new(),
    }
}

/// Makes the plan folder in `workdir`, kept out of git like the uploads next to it.
async fn prepare_plans_dir(workdir: &Path) -> bool {
    let dir = workdir.join(PLANS_DIR);
    let made = match tokio::fs::create_dir_all(&dir).await {
        Ok(()) => tokio::fs::write(dir.join(".gitignore"), "*\n").await,
        Err(e) => Err(e),
    };
    if let Err(e) = &made {
        tracing::warn!(error = %e, "could not prepare the plan folder of this run");
    }
    made.is_ok()
}

/// Where plan mode keeps its plan files, relative to the run's work folder.
/// Claude Code ignores a `plansDirectory` outside the folder it runs in.
pub const PLANS_DIR: &str = ".aurelius/plans";

/// As `deny_settings`, with `plansDirectory`: plan mode writes its plan file
/// there instead of in the login's `plans` folder, which the deny rules keep
/// shut. Without a saved file `ExitPlanMode` carries no plan at all. The path
/// is relative because Claude Code only accepts one inside its work folder;
/// the deny rules on `~/.claude` stay as they are (AUR-PLAN-FILE-001).
fn deny_settings_with_plans(login: &ClaudeLogin, vault: bool, plans: Option<&str>, ask: &[String]) -> String {
    let mut deny = deny_rules(login);
    if vault {
        deny.extend(ENV_DUMP_RULES.iter().map(|r| r.to_string()));
    }
    let mut settings = serde_json::json!({ "permissions": { "deny": deny } });
    if !ask.is_empty() {
        settings["permissions"]["ask"] = serde_json::json!(ask);
    }
    if let Some(plans) = plans {
        settings["plansDirectory"] = serde_json::Value::String(plans.to_owned());
    }
    settings.to_string()
}

/// The vault values a run gets: only names the vault may give (checked again
/// at delivery, E7) and only values the redactor knows (E14: never a value
/// that could reach a transcript, an event or a log unredacted).
fn deliverable(vault: &VaultEnv) -> Vec<(String, zeroize::Zeroizing<String>)> {
    let redactor = crate::redact::current();
    vault
        .0
        .iter()
        .filter(|var| {
            let ok = aurelius_domain::vault::check_env_name(&var.name).is_ok() && (!var.secret || !redactor.names_in(&var.value).is_empty());
            if !ok {
                tracing::warn!(name = %var.name, "a vault value was not delivered: its name is not allowed or the redactor does not know it");
            }
            ok
        })
        .map(|var| (var.name.clone(), var.value.clone()))
        .collect()
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
    // The deny rules name the login folders (D4): only how many there are.
    if let Some(i) = shown.iter().position(|a| a == "--settings") {
        if let Some(settings) = shown.get_mut(i + 1) {
            let value: serde_json::Value = serde_json::from_str(settings).unwrap_or_default();
            *settings = format!("<{} deny rule(s)>", value["permissions"]["deny"].as_array().map_or(0, Vec::len));
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

/// What `claude auth status --json` says of a login (no prompt, no cost).
/// Only these fields are read; nothing of the credentials themselves.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthStatus {
    pub logged_in: bool,
    /// e.g. "claude.ai", "none".
    pub auth_method: Option<String>,
    pub email: Option<String>,
    /// e.g. "max", "pro".
    pub plan: Option<String>,
}

impl AuthStatus {
    /// Tolerant: the JSON is not a documented format (CLI 2.1.312 has `loggedIn`,
    /// `authMethod` and, once signed in, `subscriptionType` and the account's e-mail).
    pub fn parse(json: &[u8]) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_slice(json).ok()?;
        let text = |v: &serde_json::Value| v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
        let email = ["email", "emailAddress"].iter().find_map(|k| text(&value[*k])).or_else(|| text(&value["account"]["email"])).or_else(|| text(&value["account"]["emailAddress"]));
        Some(Self {
            logged_in: value["loggedIn"].as_bool().unwrap_or(false),
            auth_method: text(&value["authMethod"]),
            email: email.filter(|e| e.contains('@') && e.len() <= 320),
            plan: text(&value["subscriptionType"]).filter(|p| p.len() <= 64),
        })
    }
}

impl ClaudeCodeRuntime {
    fn command(&self, args: &[String]) -> Command {
        self.command_from(args, |key| std::env::var_os(key), &[])
    }

    /// The folder the child keeps its config, login and sessions in: the run's
    /// login, else an empty one of Aurelius' own (so never `~/.claude`).
    pub fn config_dir(&self) -> PathBuf {
        match &self.login {
            Some(login) => login.config_dir().to_path_buf(),
            None => self.run_root.join("no-login"),
        }
    }

    /// `command` with the daemon's environment given, so a test can show what
    /// of it reaches the child.
    /// Never print the result (`Debug` of a `Command` shows its environment, E20).
    fn command_from(&self, args: &[String], parent: impl Fn(&str) -> Option<std::ffi::OsString>, vault: &[(String, zeroize::Zeroizing<String>)]) -> Command {
        let mut command = Command::new(&self.executable);
        command.args(args).env_clear().kill_on_drop(true);
        for key in ENV_ALLOWLIST {
            if let Some(value) = parent(key) {
                command.env(key, value);
            }
        }
        // The vault after the allow-list and before Aurelius' own variables, so
        // these always win, even over a name the checks let through (E7).
        for (name, value) in vault {
            command.env(name, value.as_str());
        }
        // The run's own login, after the allowlist: the only one the child gets.
        command.env(login::CONFIG_DIR_ENV, self.config_dir());
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

    /// `claude auth status --json` of this login: signed in or not, the e-mail
    /// and the plan. No prompt, no cost; never opens `.credentials.json` itself.
    pub async fn auth_status(&self) -> Result<AuthStatus, RuntimeError> {
        let output = self
            .command(&["auth".into(), "status".into(), "--json".into()])
            .current_dir(self.login.as_ref().map_or(self.run_root.as_path(), |l| l.config_dir()))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
        // Signed out it exits 1 with `loggedIn: false` (CLI 2.1.312).
        AuthStatus::parse(&output.stdout).ok_or_else(|| RuntimeError::Unavailable("Claude Code gave no sign-in status.".into()))
    }

    /// `claude auth logout` of this login (unlink, a disabled board member).
    pub async fn logout(&self) -> Result<(), RuntimeError> {
        let Some(login) = &self.login else { return Ok(()) };
        self.command(&["auth".into(), "logout".into()])
            .current_dir(login.config_dir())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
        Ok(())
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
        // No fallback: a run without a board member's login never starts (MU3-12).
        if self.login.is_none() {
            return Err(RuntimeError::Login("This run has no board member's Claude login.".into()));
        }
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
        // Its file tools stay out of the other logins, the database and ~/.claude (D5).
        if let Some(login) = &self.login {
            args.push("--settings".into());
            // A folder that cannot be made (read-only project) must not stop the run: the plan then comes as its answer.
            let plans = prepare_plans_dir(&workdir).await.then_some(PLANS_DIR);
            args.push(deny_settings_with_plans(login, matches!(request.profile, ToolProfile::Work { .. }) && !self.vault.is_empty(), plans, &ask_rules(&request.profile)));
            if let ToolProfile::Work { folders, .. } = &request.profile {
                if holds_logins(folders, login) {
                    tracing::warn!(run = %request.run_id, "a work folder of this run holds the Claude login folders; only Claude Code's best effort keeps its searches out of them");
                }
            }
        }
        // The conversation's session, or the run's own (named after the run).
        // Resumed only if Claude Code still has it; otherwise a fresh one, and the
        // turns carry the context. A browser run keeps its employee's session id and
        // stores nothing, so after a usage limit it starts over from the turns.
        let session_id = request.session_id.clone().filter(|id| is_session_id(id)).unwrap_or_else(|| request.run_id.to_string());
        let claude_dir = Some(self.config_dir());
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
        let vault = if work { deliverable(&self.vault) } else { Vec::new() };
        let mut command = self.command_from(&args, |key| std::env::var_os(key), &vault);
        drop(vault);
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
                // Or that its login is gone: it waits for the login, not a failure.
                None if parse::auth_in(&stderr_text) => {
                    let _ = tx.send(RuntimeEvent::Failed { code: parse::AUTH_FAILED.into(), message: parse::describe(parse::AUTH_FAILED) }).await;
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

    fn with_vault(&self, vault: VaultEnv) -> Option<std::sync::Arc<dyn AgentRuntime>> {
        Some(std::sync::Arc::new(Self { vault, ..self.clone() }))
    }

    /// Only in this login's folder: a session of another login is never found (R4).
    fn has_session(&self, id: &str) -> bool {
        is_session_id(id) && self.login.is_some() && session::transcript(&self.config_dir(), id).is_some()
    }

    fn remove_session(&self, id: &str) {
        if is_session_id(id) && self.login.is_some() {
            session::remove(&self.config_dir(), id);
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
mod vault_env_tests {
    use super::*;

    /// A vault item never takes a name the runtime passes or sets (MU4-06 AC4).
    #[test]
    fn every_name_the_runtime_passes_or_sets_is_reserved_for_the_vault() {
        let set = [login::CONFIG_DIR_ENV, DISABLE_BACKGROUND_TASKS, SUBAGENT_MODEL, SUBAGENT_MODEL_SETTING, CARGO_BUILD_JOBS];
        for name in ENV_ALLOWLIST.iter().chain(set.iter()) {
            assert!(aurelius_domain::vault::env_name_reserved(name), "{name}");
        }
    }
}

#[cfg(test)]
mod vault_delivery_tests {
    use super::*;
    use crate::redact::{self, Redactor, Secret};
    use zeroize::Zeroizing;

    const FAKE: &str = "sk-test-FAKE-0000-delivery";

    fn vault(pairs: &[(&str, &str)]) -> VaultEnv {
        VaultEnv::new(pairs.iter().map(|(k, v)| VaultVar { name: k.to_string(), value: Zeroizing::new(v.to_string()), secret: true }).collect())
    }

    /// E7: the allow-list, then the vault, then Aurelius' own variables, which win.
    #[test]
    fn aurelius_own_variables_win_over_the_vault_and_the_vault_over_the_allow_list() {
        let runtime = ClaudeCodeRuntime { executable: "claude".into(), run_root: "/srv/a/runs".into(), model: None, login: None, vault: VaultEnv::default() };
        let pairs = vec![("HOME".to_string(), Zeroizing::new("/vault-home".to_string())), ("CLAUDE_CONFIG_DIR".to_string(), Zeroizing::new("/vault-config".to_string())), ("CARGO_BUILD_JOBS".to_string(), Zeroizing::new("999".to_string())), ("GITHUB_TOKEN".to_string(), Zeroizing::new(FAKE.to_string()))];
        let command = runtime.command_from(&["-p".into()], |k| (k == "HOME").then(|| "/home/daemon".into()), &pairs);
        let env: std::collections::BTreeMap<String, String> = command.as_std().get_envs().filter_map(|(k, v)| Some((k.to_string_lossy().into_owned(), v?.to_string_lossy().into_owned()))).collect();
        assert_eq!(env["HOME"], "/vault-home", "the vault comes after the allow-list (and HOME is refused at create and delivery)");
        assert_eq!(Path::new(&env["CLAUDE_CONFIG_DIR"]), Path::new("/srv/a/runs/no-login"), "Aurelius' own come last");
        assert_ne!(env["CARGO_BUILD_JOBS"], "999");
        assert_eq!(env["GITHUB_TOKEN"], FAKE);
        assert!(!format!("{:?}", runtime.vault).contains("FAKE"));
    }

    /// E7 at delivery, E14: a name refused since, or a value the redactor does
    /// not know, is not delivered.
    #[test]
    fn only_allowed_names_with_values_the_redactor_knows_are_delivered() {
        let _global = redact::TEST_GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut env = vault(&[("GITHUB_TOKEN", FAKE), ("NODE_OPTIONS", FAKE), ("OTHER_TOKEN", "sk-test-FAKE-unknown-to-it")]);
        redact::install(Redactor::default());
        assert!(deliverable(&env).is_empty(), "no redactor, no delivery");
        let mut vars = (*env.0).clone();
        vars.push(VaultVar { name: "DOCS_URL".into(), value: Zeroizing::new("https://docs.example".into()), secret: false });
        env = VaultEnv::new(vars);
        redact::install(Redactor::new(&[Secret { name: "GITHUB_TOKEN".into(), label: String::new(), value: Zeroizing::new(FAKE.into()) }]));
        let names: Vec<String> = deliverable(&env).into_iter().map(|(k, _)| k).collect();
        redact::install(Redactor::default());
        assert_eq!(names, vec!["GITHUB_TOKEN".to_string(), "DOCS_URL".to_string()], "a text or url needs no redaction");
    }

    /// E8: the secrets folder is closed to every file tool and to searches; E17
    /// the environment dumps are denied as settings once a run has values.
    /// Claude Code ignores an absolute folder outside its work folder (ticket #47).
    #[test]
    fn the_plan_folder_is_given_relative_to_the_work_folder() {
        let data = std::env::temp_dir().join(format!("aurelius-plans-{}", aurelius_domain::UserId::new()));
        std::fs::create_dir_all(&data).unwrap();
        let own = ClaudeLogin::of(&data, aurelius_domain::UserId::new()).unwrap();
        let settings: serde_json::Value = serde_json::from_str(&deny_settings_with_plans(&own, false, Some(PLANS_DIR), &[])).unwrap();
        assert_eq!(settings["plansDirectory"], ".aurelius/plans");
        std::fs::remove_dir_all(&data).unwrap();
    }

    #[tokio::test]
    async fn the_plan_folder_is_made_and_kept_out_of_git() {
        let dir = std::env::temp_dir().join(format!("aurelius-plans-{}", aurelius_domain::UserId::new()));
        assert!(prepare_plans_dir(&dir).await);
        assert_eq!(std::fs::read_to_string(dir.join(PLANS_DIR).join(".gitignore")).unwrap(), "*\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_secrets_folder_and_the_environment_dumps_are_denied() {
        let data = std::env::temp_dir().join(format!("aurelius-deny-secrets-{}", aurelius_domain::UserId::new()));
        std::fs::create_dir_all(&data).unwrap();
        let own = ClaudeLogin::of(&data, aurelius_domain::UserId::new()).unwrap();
        let rules = deny_rules(&own);
        let secrets = format!("//{}/secrets", data.display().to_string().replace('\\', "/").trim_start_matches('/'));
        for tool in ["Read", "Edit", "Write", "Glob", "Grep"] {
            assert!(rules.contains(&format!("{tool}({secrets}/**)")), "{tool}: {rules:?}");
        }
        for tool in ["Read", "Glob", "Grep"] {
            assert!(rules.contains(&format!("{tool}({secrets})")), "{tool}: {rules:?}");
        }
        let asks: serde_json::Value = serde_json::from_str(&deny_settings_with_plans(&own, false, None, &["Read".to_owned()])).unwrap();
        assert_eq!(asks["permissions"]["ask"], serde_json::json!(["Read"]), "reads in the folder only ask with a rule");
        let with: serde_json::Value = serde_json::from_str(&deny_settings(&own, true)).unwrap();
        let deny: Vec<&str> = with["permissions"]["deny"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        for rule in ["Bash(env)", "Bash(env:*)", "Bash(printenv:*)", "Bash(set)", "Bash(export -p)", "Bash(declare -x)", "Bash(cat /proc/*/environ)"] {
            assert!(deny.contains(&rule), "{rule}");
        }
        assert!(!deny.iter().any(|r| r.starts_with("Bash(curl")), "curl -H \"$TOKEN\" keeps working");
        let without: serde_json::Value = serde_json::from_str(&deny_settings(&own, false)).unwrap();
        assert!(!without.to_string().contains("printenv"));
        let _ = std::fs::remove_dir_all(data);
    }

    /// E20: a `Command` prints its environment in `Debug`: never in this adapter.
    #[test]
    fn the_adapter_never_prints_a_command() {
        // The adapter, without this test (which names the patterns).
        let source = include_str!("mod.rs").split(concat!("mod vault_", "delivery_tests")).next().unwrap();
        let code: String = source.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        for printed in ["{command:?}", "{cmd:?}", "?command", "?cmd", "{:?}\", command", "dbg!(command", "dbg!(&command"] {
            assert!(!code.contains(printed), "{printed}");
        }
    }

    /// E15: only a work run gets the vault in its environment.
    #[cfg(unix)]
    #[tokio::test]
    async fn only_a_work_run_gets_the_vault() {
        use std::os::unix::fs::PermissionsExt;
        let _global = redact::TEST_GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
        redact::install(Redactor::new(&[Secret { name: "GITHUB_TOKEN".into(), label: String::new(), value: Zeroizing::new(FAKE.into()) }]));
        let dir = std::env::temp_dir().join(format!("aurelius-vault-env-{}", aurelius_domain::UserId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("env.txt");
        let script = dir.join("fake-claude");
        std::fs::write(&script, format!("#!/bin/sh\nprintf '%s\\n' \"GITHUB_TOKEN=${{GITHUB_TOKEN:-none}}\" >> {}\n", out.display())).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let login = ClaudeLogin::of(&dir, aurelius_domain::UserId::new()).unwrap();
        let runtime = ClaudeCodeRuntime { executable: script, run_root: dir.join("runs"), model: None, login: Some(login), vault: vault(&[("GITHUB_TOKEN", FAKE)]) };
        let bridge = ApprovalBridge { program: "/bin/true".into(), args: vec![] };
        let work = ToolProfile::Work { artifact: false, access: Default::default(), tool_policy: Default::default(), folders: vec![dir.clone()], permission_mode: "default".into(), approval_bridge: bridge.clone(), browser: false, browser_session: None, playwright: None, mcp_servers: Default::default() };
        let triage = ToolProfile::Triage { approval_bridge: bridge, tools: vec![], max_turns: 1 };
        for profile in [ToolProfile::TextOnly, triage, work] {
            let request = RunRequest {
                run_id: aurelius_domain::RunId::new(), model_id: "sonnet".into(), system: None,
                turns: vec![Turn { role: TurnRole::User, text: "hi".into(), images: Vec::new() }],
                effort: None, profile, subagent_model: None, session: SessionUse::None, session_id: None, keep_session: false,
            };
            let mut handle = runtime.start(request).await.unwrap();
            while handle.events.recv().await.is_some() {}
        }
        redact::install(Redactor::default());
        let seen = std::fs::read_to_string(&out).unwrap();
        assert_eq!(seen, format!("GITHUB_TOKEN=none\nGITHUB_TOKEN=none\nGITHUB_TOKEN={FAKE}\n"));
        let _ = std::fs::remove_dir_all(dir);
    }
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

    fn work_at(access: aurelius_domain::employee::AccessLevel) -> ToolProfile {
        let bridge = crate::ApprovalBridge { program: "ctl".into(), args: vec![] };
        let mut servers = std::collections::BTreeMap::new();
        servers.insert("notes".to_string(), serde_json::json!({ "type": "stdio", "command": "node" }));
        ToolProfile::Work { artifact: false, access, tool_policy: Default::default(), folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge, browser: false, browser_session: None, playwright: None, mcp_servers: servers }
    }

    fn listed_tools(args: &[String]) -> String {
        args[args.iter().position(|a| a == "--tools").unwrap() + 1].clone()
    }

    fn allowed_rules(args: &[String]) -> String {
        args[args.iter().position(|a| a == "--allowedTools").unwrap() + 1].clone()
    }

    fn with_artifact(mut profile: ToolProfile) -> ToolProfile {
        if let ToolProfile::Work { artifact, .. } = &mut profile {
            *artifact = true;
        }
        profile
    }

    #[test]
    fn a_design_link_offers_the_artifact_tool_and_nothing_else_changes() {
        use aurelius_domain::employee::AccessLevel::*;
        for level in [Inherit, ReadEdit] {
            let without = profile_arguments(&work_at(level), None, None, None);
            assert!(!listed_tools(&without).contains("Artifact") && !allowed_rules(&without).contains("Artifact"), "no link, nothing new");
            let with = profile_arguments(&with_artifact(work_at(level)), None, None, None);
            assert_eq!(listed_tools(&with), format!("{},Artifact", listed_tools(&without)));
            assert_eq!(allowed_rules(&with), format!("{},Artifact", allowed_rules(&without)));
            assert_eq!(with[with.iter().position(|a| a == "--disallowedTools").unwrap() + 1], "ArtifactComments,ArtifactData", "only the tool it is offered");
            assert!(!without.iter().any(|a| a == "--disallowedTools"));
        }
        // An employee's own list does not block it: it comes with the link.
        let listed = ToolProfile::Work { artifact: true, access: Inherit, tool_policy: aurelius_domain::employee_tools::ToolPolicy { tools: Some(vec!["Read".into()]), ..Default::default() }, folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: crate::ApprovalBridge { program: "ctl".into(), args: vec![] }, browser: false, browser_session: None, playwright: None, mcp_servers: Default::default() };
        let args = profile_arguments(&listed, None, None, None);
        assert!(listed_tools(&args).ends_with(",Artifact") && allowed_rules(&args).ends_with(",Artifact"));
        assert_eq!(ask_rules(&listed), Vec::<String>::new());
    }

    #[test]
    fn the_artifact_tool_passes_the_init_line_only_when_offered() {
        use crate::claude_code::parse::StreamState;
        let init = r#"{"type":"system","subtype":"init","tools":["Artifact"],"mcp_servers":[{"name":"aurelius","status":"connected"}],"plugins":[]}"#;
        assert!(StreamState::for_profile(&work_at(aurelius_domain::employee::AccessLevel::Inherit)).feed(init).is_err());
        assert!(StreamState::for_profile(&with_artifact(work_at(aurelius_domain::employee::AccessLevel::Inherit))).feed(init).is_ok());
    }

    #[test]
    fn a_level_is_an_explicit_tool_list_and_inherit_changes_nothing() {
        use aurelius_domain::employee::AccessLevel::*;
        let args = |level| profile_arguments(&work_at(level).confined(), None, None, None);
        let inherit = args(Inherit);
        assert_eq!(listed_tools(&inherit), format!("{},ToolSearch,TaskCreate,TaskUpdate,TaskList,TaskGet,TodoWrite", WORK_TOOLS.join(",")));
        assert!(allowed_rules(&inherit).contains("Task") && inherit.join(" ").contains("notes"));

        let read_only = args(ReadOnly);
        assert_eq!(listed_tools(&read_only), "Read,Glob,Grep,AskUserQuestion,ExitPlanMode,ToolSearch,TaskCreate,TaskUpdate,TaskList,TaskGet,TodoWrite");
        let read_edit = args(ReadEdit);
        assert_eq!(listed_tools(&read_edit), "Read,Glob,Grep,Edit,Write,AskUserQuestion,ExitPlanMode,ToolSearch,TaskCreate,TaskUpdate,TaskList,TaskGet,TodoWrite");
        for restricted in [&read_only, &read_edit, &args(Unknown)] {
            let joined = restricted.join(" ");
            assert!(joined.contains("--restricted") && joined.contains("mcp__aurelius__approve"), "still the bridge: {joined}");
            assert!(!joined.contains("notes"), "nothing the level does not name: {joined}");
            assert!(!listed_tools(restricted).contains("Bash"));
            assert!(!allowed_rules(restricted).split(',').any(|t| t == "Task"), "no rule for a tool it does not have");
            assert!(allowed_rules(restricted).contains("mcp__aurelius__post_to_discussion"), "it can still hand work over");
        }
        assert_eq!(listed_tools(&args(Unknown)), listed_tools(&read_only), "a level of a newer version is the narrowest");
    }

    #[test]
    fn the_trace_of_what_was_sent_leaves_the_mcp_configuration_out() {
        let bridge = crate::ApprovalBridge { program: "ctl".into(), args: vec!["mcp-permissions".into()] };
        let mut servers = std::collections::BTreeMap::new();
        servers.insert("notes".to_string(), serde_json::json!({ "type": "http", "url": "https://x", "headers": { "Authorization": "Bearer SECRET" } }));
        let p = ToolProfile::Work { artifact: false, access: Default::default(), tool_policy: Default::default(), folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge, browser: false, browser_session: None, playwright: None, mcp_servers: servers };
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
        let work = ToolProfile::Work { artifact: false, access: Default::default(), tool_policy: Default::default(), folders: vec!["C:/p".into()], permission_mode: "plan".into(), approval_bridge: bridge, browser: false, browser_session: None, playwright: None, mcp_servers: Default::default() };
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
        let work = |browser, playwright: Option<serde_json::Value>| ToolProfile::Work { artifact: false,
            access: Default::default(), tool_policy: Default::default(), folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: ApprovalBridge { program: "x".into(), args: vec![] },
            browser, browser_session: Some(employee.into()), playwright, mcp_servers: Default::default(),
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
        let work = |browser, browser_session: Option<&str>| ToolProfile::Work { artifact: false,
            access: Default::default(), tool_policy: Default::default(), folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: ApprovalBridge { program: "x".into(), args: vec![] },
            browser, browser_session: browser_session.map(Into::into), playwright: None, mcp_servers: Default::default(),
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
        let runtime = ClaudeCodeRuntime { executable: "claude".into(), run_root: "runs".into(), model: None, login: None, vault: VaultEnv::default() };
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
        let runtime = ClaudeCodeRuntime { executable: "claude".into(), run_root: "runs".into(), model: None, login: None, vault: VaultEnv::default() };
        let command = runtime.command(&["--version".into()]);
        let value = command.as_std().get_envs().find(|(k, _)| *k == SUBAGENT_MODEL).and_then(|(_, v)| v);
        let expected = subagent_model(std::env::var(SUBAGENT_MODEL_SETTING).ok().as_deref(), None);
        assert_eq!(value.and_then(|v| v.to_str()).map(str::to_owned), expected);
    }

    #[test]
    fn every_child_builds_with_half_the_cores() {
        let runtime = ClaudeCodeRuntime { executable: "claude".into(), run_root: "runs".into(), model: None, login: None, vault: VaultEnv::default() };
        let command = runtime.command(&["--version".into()]);
        let value = command.as_std().get_envs().find(|(k, _)| *k == CARGO_BUILD_JOBS).and_then(|(_, v)| v);
        let jobs: usize = value.and_then(|v| v.to_str()).and_then(|v| v.parse().ok()).unwrap();
        let cores = std::thread::available_parallelism().unwrap().get();
        assert_eq!(jobs, (cores / 2).max(1));
    }
}

#[cfg(test)]
mod login_tests {
    use super::*;
    use std::ffi::OsString;

    /// The daemon's environment in these tests, poisoned with its own login and keys.
    fn parent(key: &str) -> Option<OsString> {
        match key {
            "PATH" => Some("/usr/bin".into()),
            "HOME" => Some("/home/daemon".into()),
            "ANTHROPIC_API_KEY" => Some("sk-ant-api03-PARENT".into()),
            "ANTHROPIC_AUTH_TOKEN" => Some("PARENT-BEARER".into()),
            "ANTHROPIC_BASE_URL" => Some("https://parent.example.com".into()),
            "ANTHROPIC_PROFILE" => Some("parent".into()),
            "CLAUDE_CODE_OAUTH_TOKEN" => Some("sk-ant-oat01-PARENT".into()),
            "CLAUDE_CONFIG_DIR" => Some("/home/daemon/.claude-parent".into()),
            "CLAUDE_CODE_USE_BEDROCK" => Some("1".into()),
            "DISPLAY" => Some(":0".into()),
            "BROWSER" => Some("firefox".into()),
            _ => None,
        }
    }

    fn runtime(login: Option<ClaudeLogin>) -> ClaudeCodeRuntime {
        ClaudeCodeRuntime { executable: "claude".into(), run_root: "/srv/aurelius/runtimes/c/runs".into(), model: None, login, vault: VaultEnv::default() }
    }

    fn env_of(login: Option<ClaudeLogin>) -> Vec<(String, Option<String>)> {
        let command = runtime(login).command_from(&["-p".into()], parent, &[]);
        let mut env: Vec<_> = command.as_std().get_envs().map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned()))).collect();
        env.sort();
        env
    }

    fn get<'a>(env: &'a [(String, Option<String>)], key: &str) -> Option<&'a str> {
        env.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_deref())
    }

    fn alex() -> ClaudeLogin {
        ClaudeLogin::at(Path::new("/srv/aurelius"), aurelius_domain::UserId::parse("0199c3a2-0000-7000-8000-0000000000a1").unwrap())
    }

    /// MU3-12 AC5: nothing of the daemon's login or keys reaches a run, with or without a login.
    #[test]
    fn nothing_of_the_daemons_login_reaches_the_child() {
        for login in [None, Some(alex())] {
            let env = env_of(login);
            let leaked: Vec<_> = env.iter().filter(|(k, _)| k.starts_with("ANTHROPIC_") || k == "CLAUDE_CODE_OAUTH_TOKEN" || k == "CLAUDE_CODE_USE_BEDROCK" || k == "DISPLAY" || k == "BROWSER").collect();
            assert!(leaked.is_empty(), "leaked: {leaked:?}");
            let shown = format!("{env:?}");
            assert!(!shown.contains("PARENT") && !shown.contains("claude-parent") && !shown.contains("parent.example"), "{shown}");
            assert_eq!((get(&env, "PATH"), get(&env, "HOME")), (Some("/usr/bin"), Some("/home/daemon")), "the allowlist still passes");
        }
    }

    #[test]
    fn a_run_gets_its_own_login_and_never_the_default_folder() {
        let env = env_of(Some(alex()));
        assert_eq!(get(&env, login::CONFIG_DIR_ENV).map(Path::new), Some(Path::new("/srv/aurelius/logins/0199c3a2-0000-7000-8000-0000000000a1")));
        // Without a login: an empty folder of Aurelius', never ~/.claude nor the daemon's CLAUDE_CONFIG_DIR.
        let none = env_of(None);
        let dir = get(&none, login::CONFIG_DIR_ENV).expect("always set, so the CLI never falls back to ~/.claude");
        assert_eq!(Path::new(dir), Path::new("/srv/aurelius/runtimes/c/runs/no-login"));
        assert!(!dir.contains(".claude"));
    }

    #[tokio::test]
    async fn a_run_without_a_login_never_starts() {
        // A program that would record that it ran: it must never be started.
        let request = RunRequest {
            run_id: aurelius_domain::RunId::new(), model_id: "sonnet".into(), system: None,
            turns: vec![Turn { role: TurnRole::User, text: "hi".into(), images: Vec::new() }],
            effort: None, profile: ToolProfile::TextOnly, subagent_model: None, session: SessionUse::Keep, session_id: None, keep_session: false,
        };
        let runtime = ClaudeCodeRuntime { executable: "/nonexistent/claude-that-must-not-run".into(), run_root: std::env::temp_dir().join("aurelius-no-login"), model: None, login: None, vault: VaultEnv::default() };
        assert!(matches!(runtime.start(request).await, Err(RuntimeError::Login(_))));
        assert!(!runtime.has_session("0199c3a2-0000-7000-8000-000000000001"), "no login, no sessions (not even ~/.claude's)");
    }

    #[test]
    fn sessions_are_looked_up_in_the_logins_own_folder_only() {
        let data = std::env::temp_dir().join(format!("aurelius-login-sessions-{}", std::process::id()));
        std::fs::create_dir_all(&data).unwrap();
        let (a, r) = (aurelius_domain::UserId::new(), aurelius_domain::UserId::new());
        let (alex, robin) = (ClaudeLogin::of(&data, a).unwrap(), ClaudeLogin::of(&data, r).unwrap());
        let id = "0199c3a2-0000-7000-8000-000000000001";
        std::fs::create_dir_all(alex.config_dir().join("projects").join("-work")).unwrap();
        std::fs::write(alex.config_dir().join("projects").join("-work").join(format!("{id}.jsonl")), "{}\n").unwrap();
        let on = |login: &ClaudeLogin| ClaudeCodeRuntime { executable: "claude".into(), run_root: data.join("runs"), model: None, login: Some(login.clone()), vault: VaultEnv::default() };
        assert!(on(&alex).has_session(id));
        assert!(!on(&robin).has_session(id), "Alex's transcript is never resumed on Robin's login (R4)");
        on(&robin).remove_session(id);
        assert!(on(&alex).has_session(id), "removing on Robin's login leaves Alex's");
        let _ = std::fs::remove_dir_all(data);
    }

    /// Sentinel D5 (F2): a run's file tools are denied the other board members'
    /// login folders, the database and ~/.claude*, never its own folder.
    #[test]
    fn a_runs_file_tools_are_denied_what_is_not_its_own() {
        let data = std::env::temp_dir().join(format!("aurelius-deny-{}", aurelius_domain::UserId::new()));
        std::fs::create_dir_all(&data).unwrap();
        let (alex, robin) = (aurelius_domain::UserId::new(), aurelius_domain::UserId::new());
        let own = ClaudeLogin::of(&data, alex).unwrap();
        ClaudeLogin::of(&data, robin).unwrap();
        let rules = deny_rules(&own);
        let robins = format!("//{}/logins/{robin}/**", data.display().to_string().replace('\\', "/").trim_start_matches('/'));
        let db = format!("//{}/data/aurelius.sqlite*", data.display().to_string().replace('\\', "/").trim_start_matches('/'));
        for tool in ["Read", "Edit", "Write", "Glob", "Grep"] {
            for path in [robins.as_str(), db.as_str(), "~/.claude/**", "~/.claude*"] {
                assert!(rules.contains(&format!("{tool}({path})")), "{tool}({path}) missing: {rules:?}");
            }
        }
        assert!(!rules.iter().any(|r| r.contains(&alex.to_string())), "the run's own folder stays open: {rules:?}");
        // SEC2 M1: a Grep or Glob of logins/ itself or of Robin's folder itself is refused for certain.
        let logins = format!("//{}/logins", data.display().to_string().replace('\\', "/").trim_start_matches('/'));
        let robin_dir = format!("{logins}/{robin}");
        for tool in ["Read", "Glob", "Grep"] {
            for path in [logins.as_str(), robin_dir.as_str()] {
                assert!(rules.contains(&format!("{tool}({path})")), "{tool}({path}) missing: {rules:?}");
            }
        }
        assert!(!rules.contains(&format!("Write({logins})")) && !rules.contains(&format!("Edit({logins})")), "only searching the folder itself is denied");
        assert!(!rules.iter().any(|r| r.contains(".ignore")), "only folders: {rules:?}");
        // One more layer: ripgrep skips logins/ when it searches above it.
        assert_eq!(std::fs::read_to_string(data.join("logins").join(".ignore")).unwrap(), "*\n");
        // A work folder above the data folder holds the logins; the run's own workspace does not.
        let workspace = data.join("workspaces").join("c");
        std::fs::create_dir_all(&workspace).unwrap();
        assert!(holds_logins(&[workspace.clone(), data.clone()], &own));
        assert!(!holds_logins(&[workspace], &own));
        // They go in with the run, as --settings.
        let settings: serde_json::Value = serde_json::from_str(&deny_settings(&own, false)).unwrap();
        assert_eq!(settings["permissions"]["deny"].as_array().unwrap().len(), rules.len());
        // The trace of what was sent names no folder.
        let args = vec!["-p".to_string(), "--settings".into(), deny_settings(&own, false)];
        let shown = sent_request(&args, SessionUse::None, &[]).args.join(" ");
        assert!(!shown.contains("logins") && shown.contains("deny rule(s)"), "{shown}");
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn auth_status_reads_only_the_sign_in_fields() {
        let out = br#"{"loggedIn": false, "authMethod": "none", "apiProvider": "firstParty", "configDirectory": "/tmp/x"}"#;
        assert_eq!(AuthStatus::parse(out), Some(AuthStatus { logged_in: false, auth_method: Some("none".into()), email: None, plan: None }));
        let out = br#"{"loggedIn": true, "authMethod": "claude.ai", "email": "test@example.com", "subscriptionType": "max", "accessToken": "sk-ant-oat01-NEVER"}"#;
        let status = AuthStatus::parse(out).unwrap();
        assert_eq!((status.logged_in, status.email.as_deref(), status.plan.as_deref()), (true, Some("test@example.com"), Some("max")));
        assert!(!format!("{status:?}").contains("NEVER"));
        assert_eq!(AuthStatus::parse(b"not json"), None);
    }
}
