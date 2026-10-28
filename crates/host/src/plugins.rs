//! Claude Code plugins for employees and projects.
//!
//! Claude Code's own plugin install is the source of truth: this module drives
//! `claude plugin marketplace add / install / update / uninstall / list` on the
//! server's computer (argument lists only, never a shell, with timeouts) and
//! keeps which plugins each employee has enabled in the table `employee_plugins`,
//! and each project (for its project assistant and every employee run in its tasks) in
//! `project_plugins` (`aurelius_storage::employees`). An employee's run in a task loads the
//! task project's list plus its own (in its chat only its own), the project assistant's the project's; every enabled plugin that still exists is loaded with `--plugin-dir <installPath>` (see `paths_for_run`). A plugin's
//! hooks run as commands on this computer outside the approval bridge, so every
//! method that adds or assigns a plugin needs the Approve right, and a plugin's
//! own MCP servers are not loaded (the run keeps `--strict-mcp-config`).

use crate::Service;
use aurelius_domain::{ConversationId, EmployeeId, ProjectId};
use aurelius_protocol::{
    ErrorCode, EmployeePlugins, PluginCatalog, PluginComponents, PluginInfo, PluginKind, PluginOffer, PluginPreview, PluginsSnapshot, ProjectPlugins, ProtocolError,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;

const LIST_TIMEOUT: Duration = Duration::from_secs(30);
const ADD_TIMEOUT: Duration = Duration::from_secs(180);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(300);
const REMOVE_TIMEOUT: Duration = Duration::from_secs(60);
const CLONE_TIMEOUT: Duration = Duration::from_secs(120);

/// The local marketplace that holds standalone skills wrapped as plugins.
const SKILLS_MARKETPLACE: &str = "aurelius-skills";

/// One plugin change at a time: the CLI edits shared files.
static CHANGE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn internal(e: impl ToString) -> ProtocolError {
    ProtocolError::new(ErrorCode::Internal, e.to_string())
}

fn invalid(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::InvalidRequest, message)
}

fn storage(e: impl ToString) -> ProtocolError {
    ProtocolError::new(ErrorCode::StorageUnavailable, e.to_string())
}

// ---- the Claude Code CLI --------------------------------------------------

/// Run `claude <args>` and return its stdout. A failure comes back as the CLI's
/// own words (stderr, else stdout), shortened.
pub async fn claude(args: &[&str], timeout: Duration) -> Result<String, ProtocolError> {
    let exe = aurelius_runtime::claude_code::locate().ok_or_else(|| internal("Claude Code is not installed on the server's computer."))?;
    let mut command = Command::new(exe);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let label = args.iter().take(3).copied().collect::<Vec<_>>().join(" ");
    let output = match tokio::time::timeout(timeout, command.output()).await {
        Err(_) => return Err(internal(format!("'claude {label}' took longer than {} seconds and was stopped.", timeout.as_secs()))),
        Ok(Err(e)) => return Err(internal(format!("could not run Claude Code: {e}"))),
        Ok(Ok(output)) => output,
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stderr.trim().is_empty() { stdout.as_str() } else { &stderr };
    Err(internal(format!("'claude {label}' failed: {}", readable(text))))
}

/// The tail of a CLI message without control characters (colour codes, spinners).
fn readable(text: &str) -> String {
    let clean: String = text.chars().filter(|c| !c.is_control() || *c == '\n').collect();
    let clean = clean.trim();
    let tail: String = clean.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
    if tail.is_empty() { "no message".into() } else { tail }
}

fn json_of(text: &str) -> Result<Value, ProtocolError> {
    serde_json::from_str(text).map_err(|e| internal(format!("Claude Code answered something unexpected: {e}")))
}

// ---- input checks ---------------------------------------------------------

/// A pasted link split up: `source` is the argument for `claude plugin marketplace add`
/// (`owner/repo` for GitHub, else the git URL itself); `git_ref` and `subpath` come from
/// `/tree/<ref>/<path>` or `/blob/<ref>/<path>/SKILL.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub source: String,
    pub git_ref: Option<String>,
    pub subpath: Option<String>,
}

impl Link {
    /// What `git clone` takes.
    /// The link as a user would paste it; `parse_link` reads it back.
    fn text(&self) -> String {
        if self.source.contains(':') {
            return self.source.clone();
        }
        let mut out = format!("https://github.com/{}", self.source);
        if let Some(r) = &self.git_ref {
            out.push_str(&format!("/tree/{r}"));
            if let Some(p) = &self.subpath {
                out.push_str(&format!("/{p}"));
            }
        }
        out
    }

    fn clone_url(&self) -> String {
        if self.source.contains(':') { self.source.clone() } else { format!("https://github.com/{}.git", self.source) }
    }
}

/// Parses what the user pasted: `owner/repo`, `github.com/owner/repo`, a GitHub URL (also
/// `/tree/...`, `/blob/...` and `.git`), or an https/ssh git URL. Local paths are refused.
/// shortcut: a ref with a slash in its name is read as ref = first segment, rest = path.
pub fn parse_link(input: &str) -> Result<Link, ProtocolError> {
    let text = input.trim();
    let bad = || invalid("Paste a GitHub link (https://github.com/owner/repo), 'owner/repo' or a git URL.");
    if text.is_empty() || text.len() > 500 || text.starts_with('-') || text.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad());
    }
    let github = |rest: &str| -> Option<Link> {
        let mut parts = rest.trim_matches('/').split('/');
        let owner = parts.next()?;
        let repo = parts.next()?.trim_end_matches(".git");
        let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && !s.starts_with('.');
        if !(ok(owner) && ok(repo)) {
            return None;
        }
        let source = format!("{owner}/{repo}");
        let (mut git_ref, mut subpath) = (None, None);
        if let Some(kind) = parts.next() {
            if !matches!(kind, "tree" | "blob") {
                return Some(Link { source, git_ref, subpath });
            }
            let git_ref_part = parts.next().filter(|r| ok(r) && !r.starts_with('-'))?;
            let mut path: Vec<&str> = parts.collect();
            if kind == "blob" && path.last().is_some_and(|f| f.eq_ignore_ascii_case("SKILL.md")) {
                path.pop();
            }
            if path.iter().any(|p| !ok(p) || *p == "..") {
                return None;
            }
            git_ref = Some(git_ref_part.to_string());
            subpath = (!path.is_empty()).then(|| path.join("/"));
        }
        Some(Link { source, git_ref, subpath })
    };
    for prefix in ["https://github.com/", "http://github.com/", "https://www.github.com/", "github.com/", "git@github.com:"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return github(rest.split(['?', '#']).next().unwrap_or("")).ok_or_else(bad);
        }
    }
    if text.starts_with("https://") || text.starts_with("http://") || text.starts_with("ssh://") || text.starts_with("git@") {
        return Ok(Link { source: text.to_string(), git_ref: None, subpath: None });
    }
    if !text.contains(':') && !text.contains('\\') && text.matches('/').count() == 1 {
        return github(text).ok_or_else(bad);
    }
    Err(bad())
}

/// The marketplace source of a link (see [`parse_link`]).
pub fn normalize_source(input: &str) -> Result<String, ProtocolError> {
    parse_link(input).map(|l| l.source)
}

/// `name@marketplace` with safe characters only.
fn check_id(id: &str) -> Result<String, ProtocolError> {
    let id = id.trim();
    let safe = |s: &str| !s.is_empty() && !s.starts_with(['-', '.']) && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    match id.split_once('@') {
        Some((name, marketplace)) if id.len() <= 200 && safe(name) && safe(marketplace) => Ok(id.to_string()),
        _ => Err(invalid(format!("'{id}' is not a plugin id (name@marketplace)."))),
    }
}

fn split_id(id: &str) -> (String, String) {
    match id.rsplit_once('@') {
        Some((name, marketplace)) => (name.to_string(), marketplace.to_string()),
        None => (id.to_string(), String::new()),
    }
}

// ---- what is installed ----------------------------------------------------

/// Where Claude Code keeps its plugin files (`CLAUDE_CONFIG_DIR` or `~/.claude`).
fn claude_home() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".claude"))
}

/// Install folders by plugin id, from the file Claude Code keeps (cheap enough
/// for every run start). The first entry of an id wins.
fn install_paths_from(value: &Value) -> BTreeMap<String, PathBuf> {
    let mut out = BTreeMap::new();
    if let Some(map) = value["plugins"].as_object() {
        for (id, entries) in map {
            let path = entries.as_array().and_then(|a| a.iter().find_map(|e| e["installPath"].as_str())).map(PathBuf::from);
            if let Some(path) = path {
                out.insert(id.clone(), path);
            }
        }
    }
    out
}

fn installed_paths() -> BTreeMap<String, PathBuf> {
    let file = claude_home().map(|h| h.join("plugins").join("installed_plugins.json"));
    file.and_then(|f| std::fs::read(f).ok()).and_then(|b| serde_json::from_slice::<Value>(&b).ok()).map(|v| install_paths_from(&v)).unwrap_or_default()
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn names_in(dir: &Path, want_dirs: bool) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir) else { return vec![] };
    let mut names: Vec<String> = read
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if want_dirs {
                path.join("SKILL.md").is_file().then(|| e.file_name().to_string_lossy().into_owned())
            } else {
                (path.extension().is_some_and(|x| x == "md")).then(|| path.file_stem().map(|s| s.to_string_lossy().into_owned())).flatten()
            }
        })
        .collect();
    names.sort();
    names
}

/// What the plugin in `dir` brings, read from its files (conventional folders;
/// the manifest only matters for hooks and MCP servers).
pub fn components_of(dir: &Path) -> PluginComponents {
    let manifest = read_json(&dir.join(".claude-plugin").join("plugin.json")).or_else(|| read_json(&dir.join("plugin.json"))).unwrap_or(Value::Null);
    let mut mcp: Vec<String> = Vec::new();
    for source in [read_json(&dir.join(".mcp.json")), manifest.get("mcpServers").filter(|v| v.is_object()).cloned()].into_iter().flatten() {
        let servers = source.get("mcpServers").filter(|v| v.is_object()).unwrap_or(&source);
        mcp.extend(servers.as_object().into_iter().flat_map(|m| m.keys().cloned()));
    }
    mcp.sort();
    mcp.dedup();
    PluginComponents {
        skills: names_in(&dir.join("skills"), true),
        commands: names_in(&dir.join("commands"), false),
        agents: names_in(&dir.join("agents"), false),
        hooks: !manifest["hooks"].is_null() || dir.join("hooks").join("hooks.json").is_file(),
        mcp_servers: mcp,
    }
}

fn manifest_description(dir: &Path) -> Option<String> {
    let manifest = read_json(&dir.join(".claude-plugin").join("plugin.json")).or_else(|| read_json(&dir.join("plugin.json")))?;
    manifest["description"].as_str().map(str::to_owned)
}

// ---- per-employee lists ----------------------------------------------------

/// Every employee's enabled ids.
fn all_enabled(conn: &aurelius_storage::Conn) -> Result<Vec<(EmployeeId, Vec<String>)>, aurelius_storage::StorageError> {
    let mut lists: Vec<(EmployeeId, Vec<String>)> = Vec::new();
    for (plugin, employee) in aurelius_storage::employees::plugin_users(conn)? {
        match lists.iter_mut().find(|(m, _)| *m == employee) {
            Some((_, ids)) => ids.push(plugin),
            None => lists.push((employee, vec![plugin])),
        }
    }
    Ok(lists)
}

fn enabled_of(conn: &aurelius_storage::Conn, employee: EmployeeId) -> Result<Vec<String>, aurelius_storage::StorageError> {
    aurelius_storage::employees::plugins_of(conn, employee)
}

fn put_lists(tx: &aurelius_storage::Tx<'_>, lists: &[(EmployeeId, Vec<String>)]) -> Result<(), aurelius_storage::StorageError> {
    for (employee, ids) in lists {
        aurelius_storage::employees::set_plugins(tx, *employee, ids)?;
    }
    Ok(())
}

/// Every project's enabled ids.
fn all_project_enabled(conn: &aurelius_storage::Conn) -> Result<Vec<(ProjectId, Vec<String>)>, aurelius_storage::StorageError> {
    let mut lists: Vec<(ProjectId, Vec<String>)> = Vec::new();
    for (plugin, project) in aurelius_storage::employees::project_plugin_users(conn)? {
        match lists.iter_mut().find(|(p, _)| *p == project) {
            Some((_, ids)) => ids.push(plugin),
            None => lists.push((project, vec![plugin])),
        }
    }
    Ok(lists)
}

fn put_project_lists(tx: &aurelius_storage::Tx<'_>, lists: &[(ProjectId, Vec<String>)]) -> Result<(), aurelius_storage::StorageError> {
    for (project, ids) in lists {
        aurelius_storage::employees::set_project_plugins(tx, *project, ids)?;
    }
    Ok(())
}

/// Checked ids without doubles, in the given order.
fn checked_ids(ids: Vec<String>) -> Result<Vec<String>, ProtocolError> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        let id = check_id(&id)?;
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

/// `current` with the ids of `add` it lacks appended.
fn merged(mut current: Vec<String>, add: &[String]) -> Vec<String> {
    let fresh: Vec<String> = add.iter().filter(|id| !current.contains(id)).cloned().collect();
    current.extend(fresh);
    current
}

async fn blocking<T: Send + 'static>(shared: &Arc<impl Service>, f: impl FnOnce(&aurelius_storage::Conn) -> Result<T, aurelius_storage::StorageError> + Send + 'static) -> Result<T, ProtocolError> {
    let store = shared.store().clone();
    tokio::task::spawn_blocking(move || store.read(f)).await.map_err(internal)?.map_err(storage)
}

/// The enabled ids that are no longer installed.
fn missing_of(enabled: &[String]) -> Vec<String> {
    let installed = installed_paths();
    enabled.iter().filter(|id| !installed.get(*id).is_some_and(|p| p.is_dir())).cloned().collect()
}

fn employee_listing(employee_id: EmployeeId, enabled: Vec<String>) -> EmployeePlugins {
    EmployeePlugins { employee_id, missing: missing_of(&enabled), enabled }
}

fn project_listing(project_id: ProjectId, enabled: Vec<String>) -> ProjectPlugins {
    ProjectPlugins { project_id, missing: missing_of(&enabled), enabled }
}

/// `plugin.get_for_employee`.
pub async fn employee_get(shared: &Arc<impl Service>, employee_id: EmployeeId) -> Result<EmployeePlugins, ProtocolError> {
    let enabled = blocking(shared, move |c| enabled_of(c, employee_id)).await?;
    Ok(employee_listing(employee_id, enabled))
}

/// `plugin.set_for_employee`: replaces the employee's own list (in a task its
/// runs add the task project's); applies from its next run.
pub async fn employee_set(shared: &Arc<impl Service>, employee_id: EmployeeId, enabled: Vec<String>) -> Result<EmployeePlugins, ProtocolError> {
    let ids = checked_ids(enabled)?;
    let saved = ids.clone();
    shared.store().write(move |tx| put_lists(tx, &[(employee_id, saved)])).map_err(storage)?;
    Ok(employee_listing(employee_id, ids))
}

/// `plugin.get_for_project`.
pub async fn project_get(shared: &Arc<impl Service>, project_id: ProjectId) -> Result<ProjectPlugins, ProtocolError> {
    let enabled = blocking(shared, move |c| aurelius_storage::employees::project_plugins_of(c, project_id)).await?;
    Ok(project_listing(project_id, enabled))
}

/// `plugin.set_for_project`: replaces the project's list; applies from the next run of
/// its assistant and of each of its employees.
pub async fn project_set(shared: &Arc<impl Service>, project_id: ProjectId, enabled: Vec<String>) -> Result<ProjectPlugins, ProtocolError> {
    let ids = checked_ids(enabled)?;
    let saved = ids.clone();
    shared.store().write(move |tx| put_project_lists(tx, &[(project_id, saved)])).map_err(storage)?;
    Ok(project_listing(project_id, ids))
}

/// The plugins a conversation's run loads: in a task conversation the task
/// project's list followed by the employee's own, without doubles; in an
/// employee's chat its own; the project's in a project conversation (the project
/// assistant); else none.
pub fn enabled_for_run(c: &aurelius_storage::Conn, conversation: ConversationId) -> Result<Vec<String>, aurelius_storage::StorageError> {
    if let Some((employee, project)) = aurelius_storage::employees::employee_project(c, conversation)? {
        let project_list = match project {
            Some(project) => aurelius_storage::employees::project_plugins_of(c, project)?,
            None => vec![],
        };
        return Ok(merged(project_list, &enabled_of(c, employee)?));
    }
    match aurelius_storage::conversations::project_of_conversation(c, conversation)? {
        Some(project) => aurelius_storage::employees::project_plugins_of(c, project),
        None => Ok(vec![]),
    }
}

/// The install folders of the plugins the conversation's run loads (see
/// `enabled_for_run`) that still exist.
pub fn paths_for_run(store: &aurelius_storage::Store, conversation: ConversationId) -> Vec<PathBuf> {
    let enabled = store.read(|c| enabled_for_run(c, conversation)).unwrap_or_default();
    if enabled.is_empty() {
        return vec![];
    }
    let installed = installed_paths();
    enabled.iter().filter_map(|id| installed.get(id)).filter(|p| p.is_dir()).cloned().collect()
}

// ---- installed list -------------------------------------------------------

async fn installed_json() -> Result<Vec<Value>, ProtocolError> {
    let list = json_of(&claude(&["plugin", "list", "--json"], LIST_TIMEOUT).await?)?;
    Ok(list.as_array().cloned().unwrap_or_default())
}

async fn snapshot(shared: &Arc<impl Service>) -> Result<PluginsSnapshot, ProtocolError> {
    let entries = installed_json().await?;
    let enabled = blocking(shared, all_enabled).await?;
    let project_enabled = blocking(shared, all_project_enabled).await?;
    let sources = skill_sources();
    let mut plugins: Vec<PluginInfo> = Vec::new();
    for entry in entries {
        let Some(id) = entry["id"].as_str().map(str::to_owned) else { continue };
        if plugins.iter().any(|p| p.id == id) {
            continue;
        }
        let (name, marketplace) = split_id(&id);
        let dir = entry["installPath"].as_str().map(PathBuf::from);
        let present = dir.as_deref().is_some_and(Path::is_dir);
        let (components, description) = match (&dir, present) {
            (Some(dir), true) => (components_of(dir), manifest_description(dir).unwrap_or_default()),
            _ => (PluginComponents::default(), String::new()),
        };
        let employees = enabled.iter().filter(|(_, ids)| ids.contains(&id)).map(|(m, _)| *m).collect();
        let projects = project_enabled.iter().filter(|(_, ids)| ids.contains(&id)).map(|(p, _)| *p).collect();
        let (kind, source) = kind_of(&marketplace, &name, &sources);
        plugins.push(PluginInfo {
            id,
            name,
            marketplace,
            version: entry["version"].as_str().unwrap_or("").to_string(),
            description,
            components,
            employees,
            projects,
            missing: !present,
            kind,
            source,
        });
    }
    plugins.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(PluginsSnapshot { plugins })
}

/// `plugin.list`.
pub async fn list(shared: &Arc<impl Service>) -> Result<PluginsSnapshot, ProtocolError> {
    snapshot(shared).await
}

// ---- marketplaces and what they offer -------------------------------------

/// Offers of the marketplaces (all, or one), from `claude plugin list --available`.
async fn offers(only: Option<&str>) -> Result<Vec<PluginOffer>, ProtocolError> {
    let available = json_of(&claude(&["plugin", "list", "--available", "--json"], LIST_TIMEOUT).await?)?;
    let installed: Vec<String> = installed_json().await?.iter().filter_map(|e| e["id"].as_str().map(str::to_owned)).collect();
    let marketplaces = marketplace_dirs().await;
    let sources = skill_sources();
    let mut out = Vec::new();
    for item in available["available"].as_array().into_iter().flatten() {
        let (Some(id), Some(name), Some(marketplace)) = (item["pluginId"].as_str(), item["name"].as_str(), item["marketplaceName"].as_str()) else { continue };
        if only.is_some_and(|m| m != marketplace) {
            continue;
        }
        // A plugin that lives inside the marketplace's own files can be inspected before install.
        let components = item["source"].as_str().filter(|s| s.starts_with("./")).and_then(|rel| {
            let dir = marketplaces.get(marketplace)?.join(rel.trim_start_matches("./"));
            dir.is_dir().then(|| components_of(&dir))
        });
        let (kind, source) = kind_of(marketplace, name, &sources);
        out.push(PluginOffer {
            id: id.to_string(),
            name: name.to_string(),
            marketplace: marketplace.to_string(),
            description: item["description"].as_str().unwrap_or("").to_string(),
            components,
            installed: installed.iter().any(|i| i == id),
            kind,
            source,
        });
    }
    Ok(out)
}

/// Marketplace name to its local folder.
async fn marketplace_dirs() -> BTreeMap<String, PathBuf> {
    let Ok(text) = claude(&["plugin", "marketplace", "list", "--json"], LIST_TIMEOUT).await else { return BTreeMap::new() };
    let list = serde_json::from_str::<Value>(&text).ok();
    list.as_ref()
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| Some((m["name"].as_str()?.to_string(), PathBuf::from(m["installLocation"].as_str()?))))
        .collect()
}

/// `plugin.list_available`.
pub async fn available() -> Result<PluginCatalog, ProtocolError> {
    Ok(PluginCatalog { offers: offers(None).await? })
}

/// The marketplace's name from `claude plugin marketplace add` output
/// ("✔ Successfully added marketplace: ponytail (declared in user settings)").
fn marketplace_name(output: &str) -> Option<String> {
    let rest = output.lines().find_map(|l| l.split_once("marketplace:").map(|(_, r)| r))?;
    let name = rest.trim().split([' ', '(']).next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// `plugin.preview`: adds the marketplace (a no-op when it is there) and lists its plugins.
/// A link to a subfolder, or a repo without a marketplace, is read as standalone skills
/// (SKILL.md folders) that get wrapped as plugins in the local `aurelius-skills` marketplace.
pub async fn preview(source: String) -> Result<PluginPreview, ProtocolError> {
    let link = parse_link(&source)?;
    let _guard = CHANGE.lock().await;
    if link.subpath.is_none() {
        if let Ok(output) = claude(&["plugin", "marketplace", "add", &link.source], ADD_TIMEOUT).await {
            if let Some(marketplace) = marketplace_name(&output) {
                return Ok(PluginPreview { offers: offers(Some(&marketplace)).await?, marketplace });
            }
        }
    }
    let names = add_skills(&link, None).await?;
    let offers = offers(Some(SKILLS_MARKETPLACE)).await?.into_iter().filter(|o| names.contains(&o.name)).collect();
    Ok(PluginPreview { offers, marketplace: SKILLS_MARKETPLACE.into() })
}

// ---- standalone skills ----------------------------------------------------

/// A folder with a SKILL.md whose frontmatter has a valid name and a description.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Skill {
    name: String,
    description: String,
    dir: PathBuf,
}

/// Value of a top-level `key:` in a SKILL.md frontmatter, also `>`/`|` blocks.
/// shortcut: flat YAML only (no nesting, anchors), enough for skill headers.
fn frontmatter_value(text: &str, key: &str) -> Option<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    let head: Vec<&str> = lines.take_while(|l| l.trim_end() != "---").collect();
    let at = head.iter().position(|l| l.strip_prefix(key).is_some_and(|r| r.starts_with(':')))?;
    let value = head[at][key.len() + 1..].trim();
    let value = if matches!(value, ">" | "|" | ">-" | "|-" | ">+" | "|+") {
        head[at + 1..].iter().take_while(|l| l.starts_with([' ', '\t']) || l.trim().is_empty()).map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
    } else {
        value.trim_matches(['"', '\'']).to_string()
    };
    (!value.is_empty()).then_some(value)
}

/// Reads `dir/SKILL.md`; `Err` says why the skill is unusable.
fn read_skill(dir: &Path) -> Result<Skill, String> {
    let text = std::fs::read_to_string(dir.join("SKILL.md")).map_err(|e| format!("cannot read SKILL.md: {e}"))?;
    let name = frontmatter_value(&text, "name").ok_or("SKILL.md has no 'name' in its frontmatter")?;
    let description = frontmatter_value(&text, "description").ok_or("SKILL.md has no 'description' in its frontmatter")?;
    let safe = name.len() <= 64 && !name.starts_with(['-', '.']) && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !safe {
        return Err(format!("'{name}' is not a valid skill name (letters, digits, - _ . only)"));
    }
    Ok(Skill { name, description, dir: dir.to_path_buf() })
}

/// The SKILL.md folders under `base`: `base` itself, else `skills/*` and `*` one level deep.
fn find_skills(base: &Path) -> Result<Vec<Skill>, String> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if base.join("SKILL.md").is_file() {
        dirs.push(base.to_path_buf());
    } else {
        for parent in [base.join("skills"), base.to_path_buf()] {
            let Ok(read) = std::fs::read_dir(&parent) else { continue };
            let mut found: Vec<PathBuf> = read
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p.join("SKILL.md").is_file() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
                .collect();
            found.sort();
            dirs.extend(found);
        }
    }
    if dirs.is_empty() {
        return Err("no SKILL.md found (looked in the folder itself, skills/* and one level down)".into());
    }
    let (mut skills, mut problems): (Vec<Skill>, Vec<String>) = (vec![], vec![]);
    for dir in dirs {
        let label = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match read_skill(&dir) {
            Ok(skill) if skills.iter().any(|s| s.name == skill.name) => problems.push(format!("{label}: duplicate name '{}'", skill.name)),
            Ok(skill) => skills.push(skill),
            Err(e) => problems.push(format!("{label}: {e}")),
        }
    }
    if skills.is_empty() {
        return Err(format!("no usable skill: {}", problems.join("; ")));
    }
    Ok(skills)
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (name, kind) = (entry.file_name(), entry.file_type()?);
        if name == ".git" || kind.is_symlink() {
            continue; // a link could point out of the clone
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &to.join(&name))?;
        } else {
            std::fs::copy(entry.path(), to.join(&name))?;
        }
    }
    Ok(())
}

fn skills_market() -> Result<PathBuf, ProtocolError> {
    Ok(crate::data_dir().map_err(internal)?.join(SKILLS_MARKETPLACE))
}

/// Skill name to the link it was added from (`sources.json` in the local marketplace folder).
fn sources_in(market: &Path) -> BTreeMap<String, String> {
    read_json(&market.join("sources.json")).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn skill_sources() -> BTreeMap<String, String> {
    skills_market().map(|m| sources_in(&m)).unwrap_or_default()
}

fn kind_of(marketplace: &str, name: &str, sources: &BTreeMap<String, String>) -> (PluginKind, Option<String>) {
    if marketplace == SKILLS_MARKETPLACE { (PluginKind::Skill, sources.get(name).cloned()) } else { (PluginKind::Plugin, None) }
}

/// Wraps `skill` as the plugin `plugins/<name>` of the marketplace folder `market`, records
/// its link and rewrites the marketplace files. Replaces an earlier copy.
fn wrap_skill(market: &Path, skill: &Skill, link: &str, version: &str) -> std::io::Result<()> {
    let plugin = market.join("plugins").join(&skill.name);
    if plugin.exists() {
        std::fs::remove_dir_all(&plugin)?;
    }
    copy_tree(&skill.dir, &plugin.join("skills").join(&skill.name))?;
    std::fs::create_dir_all(plugin.join(".claude-plugin"))?;
    let manifest = serde_json::json!({ "name": skill.name, "description": skill.description, "version": version });
    std::fs::write(plugin.join(".claude-plugin").join("plugin.json"), serde_json::to_vec_pretty(&manifest)?)?;
    let mut sources = sources_in(market);
    sources.insert(skill.name.clone(), link.to_string());
    save_market(market, &sources, &BTreeMap::from([(skill.name.clone(), skill.description.clone())]))
}

/// Writes `sources.json` and a `marketplace.json` listing every plugin in `sources`;
/// `described` gives fresh descriptions, the others are kept from the old file.
fn save_market(market: &Path, sources: &BTreeMap<String, String>, described: &BTreeMap<String, String>) -> std::io::Result<()> {
    let old = read_json(&market.join(".claude-plugin").join("marketplace.json")).unwrap_or(Value::Null);
    let entries: Vec<Value> = sources
        .keys()
        .map(|name| {
            let kept = old["plugins"].as_array().and_then(|a| a.iter().find(|p| p["name"] == *name)).and_then(|p| p["description"].as_str());
            let description = described.get(name).map(String::as_str).or(kept).unwrap_or("");
            serde_json::json!({ "name": name, "source": format!("./plugins/{name}"), "description": description })
        })
        .collect();
    std::fs::create_dir_all(market.join(".claude-plugin"))?;
    let file = serde_json::json!({ "name": SKILLS_MARKETPLACE, "owner": { "name": "Aurelius" }, "plugins": entries });
    std::fs::write(market.join(".claude-plugin").join("marketplace.json"), serde_json::to_vec_pretty(&file)?)?;
    std::fs::write(market.join("sources.json"), serde_json::to_vec_pretty(sources)?)
}

/// Drops one wrapped skill from the local marketplace folder.
fn unwrap_skill(market: &Path, name: &str) -> std::io::Result<()> {
    let plugin = market.join("plugins").join(name);
    if plugin.exists() {
        std::fs::remove_dir_all(plugin)?;
    }
    let mut sources = sources_in(market);
    sources.remove(name);
    save_market(market, &sources, &BTreeMap::new())
}

async fn git(args: &[&str], dir: Option<&Path>) -> Result<String, ProtocolError> {
    let mut command = Command::new("git");
    command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).env("GIT_TERMINAL_PROMPT", "0");
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let output = match tokio::time::timeout(CLONE_TIMEOUT, command.output()).await {
        Err(_) => return Err(internal(format!("git took longer than {} seconds and was stopped.", CLONE_TIMEOUT.as_secs()))),
        Ok(Err(e)) => return Err(internal(format!("could not run git (is it installed?): {e}"))),
        Ok(Ok(output)) => output,
    };
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    Err(internal(format!("git could not fetch the link: {}", readable(&String::from_utf8_lossy(&output.stderr)))))
}

/// Shallow-clones the link fresh into the cache; returns the folder and the commit.
async fn fetch(link: &Link) -> Result<(PathBuf, String), ProtocolError> {
    use sha2::{Digest, Sha256};
    let url = link.clone_url();
    let key: String = hex::encode(Sha256::digest(format!("{url}#{}", link.git_ref.as_deref().unwrap_or("")))).chars().take(16).collect();
    let dir = crate::data_dir().map_err(internal)?.join("skill-cache").join(key);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(internal)?;
    }
    std::fs::create_dir_all(dir.parent().expect("has parent")).map_err(internal)?;
    let target = dir.to_string_lossy().into_owned();
    let mut args = vec!["clone", "--depth", "1", "--quiet"];
    if let Some(r) = &link.git_ref {
        args.extend(["--branch", r]);
    }
    args.extend(["--", &url, &target]);
    git(&args, None).await?;
    let commit = git(&["rev-parse", "--short", "HEAD"], Some(&dir)).await.unwrap_or_else(|_| "0".into());
    Ok((dir, commit))
}

/// Clones the link, finds its skills (only `only`, if given), wraps them and refreshes the
/// local marketplace (registering it the first time). Returns the wrapped names.
/// Call with `CHANGE` held.
async fn add_skills(link: &Link, only: Option<&str>) -> Result<Vec<String>, ProtocolError> {
    let (root, commit) = fetch(link).await?;
    let base = match &link.subpath {
        Some(sub) => root.join(sub),
        None => root.clone(),
    };
    if !base.is_dir() {
        return Err(invalid(format!("'{}' is not a folder in that repository.", link.subpath.as_deref().unwrap_or(""))));
    }
    let found = find_skills(&base).map_err(|e| invalid(format!("That link has no plugin marketplace and {e}.")))?;
    let market = skills_market()?;
    let mut names = Vec::new();
    for skill in found.iter().filter(|s| only.is_none_or(|n| n == s.name)) {
        wrap_skill(&market, skill, &link.text(), &commit).map_err(internal)?;
        names.push(skill.name.clone());
    }
    if names.is_empty() {
        return Err(invalid(format!("The skill '{}' is no longer at its source.", only.unwrap_or(""))));
    }
    refresh_market(&market).await?;
    Ok(names)
}

/// Registers the local marketplace with Claude Code, or tells it to re-read the files.
async fn refresh_market(market: &Path) -> Result<(), ProtocolError> {
    if marketplace_dirs().await.contains_key(SKILLS_MARKETPLACE) {
        claude(&["plugin", "marketplace", "update", SKILLS_MARKETPLACE], ADD_TIMEOUT).await?;
    } else {
        claude(&["plugin", "marketplace", "add", &market.to_string_lossy()], ADD_TIMEOUT).await?;
    }
    Ok(())
}

// ---- install, update, remove ----------------------------------------------

/// `plugin.install`: installs each plugin, then enables the ones that worked for the employees and projects.
pub async fn install(shared: &Arc<impl Service>, ids: Vec<String>, employees: Vec<EmployeeId>, projects: Vec<ProjectId>) -> Result<PluginsSnapshot, ProtocolError> {
    let checked = checked_ids(ids)?;
    if checked.is_empty() {
        return Err(invalid("Pick at least one plugin."));
    }
    let _guard = CHANGE.lock().await;
    let mut done: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for id in checked {
        match claude(&["plugin", "install", &id], INSTALL_TIMEOUT).await {
            Ok(_) => done.push(id),
            Err(e) => failures.push(format!("{id}: {}", e.message)),
        }
    }
    if !done.is_empty() && !employees.is_empty() {
        let (add, targets) = (done.clone(), employees.clone());
        let current = blocking(shared, move |c| targets.iter().map(|m| Ok((*m, enabled_of(c, *m)?))).collect::<Result<Vec<_>, _>>()).await?;
        let lists: Vec<(EmployeeId, Vec<String>)> = current.into_iter().map(|(m, ids)| (m, merged(ids, &add))).collect();
        shared.store().write(move |tx| put_lists(tx, &lists)).map_err(storage)?;
    }
    if !done.is_empty() && !projects.is_empty() {
        let (add, targets) = (done.clone(), projects.clone());
        let current = blocking(shared, move |c| targets.iter().map(|p| Ok((*p, aurelius_storage::employees::project_plugins_of(c, *p)?))).collect::<Result<Vec<_>, _>>()).await?;
        let lists: Vec<(ProjectId, Vec<String>)> = current.into_iter().map(|(p, ids)| (p, merged(ids, &add))).collect();
        shared.store().write(move |tx| put_project_lists(tx, &lists)).map_err(storage)?;
    }
    if !failures.is_empty() {
        let installed = if done.is_empty() { String::new() } else { format!(" Installed: {}.", done.join(", ")) };
        return Err(internal(format!("Could not install {}.{installed}", failures.join("; "))));
    }
    snapshot(shared).await
}

/// `plugin.update`.
pub async fn update(shared: &Arc<impl Service>, id: String) -> Result<PluginsSnapshot, ProtocolError> {
    let id = check_id(&id)?;
    {
        let _guard = CHANGE.lock().await;
        let (name, marketplace) = split_id(&id);
        if marketplace == SKILLS_MARKETPLACE {
            // Re-pull the source first; Claude Code then updates its copy from the local marketplace.
            let source = skill_sources().remove(&name).ok_or_else(|| invalid(format!("'{name}' has no known source.")))?;
            add_skills(&parse_link(&source)?, Some(&name)).await?;
        }
        claude(&["plugin", "update", &id], INSTALL_TIMEOUT).await?;
    }
    snapshot(shared).await
}

/// `plugin.remove`: uninstalls and takes the plugin off every employee and project.
pub async fn remove(shared: &Arc<impl Service>, id: String) -> Result<PluginsSnapshot, ProtocolError> {
    let id = check_id(&id)?;
    {
        let _guard = CHANGE.lock().await;
        claude(&["plugin", "uninstall", &id], REMOVE_TIMEOUT).await?;
        let (name, marketplace) = split_id(&id);
        if marketplace == SKILLS_MARKETPLACE {
            let market = skills_market()?;
            unwrap_skill(&market, &name).map_err(internal)?;
            refresh_market(&market).await?;
        }
    }
    let drop_id = id.clone();
    let lists = blocking(shared, all_enabled).await?;
    let changed: Vec<(EmployeeId, Vec<String>)> = lists.into_iter().filter(|(_, ids)| ids.contains(&drop_id)).map(|(m, ids)| (m, ids.into_iter().filter(|i| *i != drop_id).collect())).collect();
    if !changed.is_empty() {
        shared.store().write(move |tx| put_lists(tx, &changed)).map_err(storage)?;
    }
    let drop_id = id.clone();
    let lists = blocking(shared, all_project_enabled).await?;
    let changed: Vec<(ProjectId, Vec<String>)> = lists.into_iter().filter(|(_, ids)| ids.contains(&drop_id)).map(|(p, ids)| (p, ids.into_iter().filter(|i| *i != drop_id).collect())).collect();
    if !changed.is_empty() {
        shared.store().write(move |tx| put_project_lists(tx, &changed)).map_err(storage)?;
    }
    snapshot(shared).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_become_marketplace_sources() {
        let ok = |s: &str| normalize_source(s).unwrap();
        assert_eq!(ok("https://github.com/DietrichGebert/ponytail"), "DietrichGebert/ponytail");
        assert_eq!(ok(" https://github.com/DietrichGebert/ponytail.git/ "), "DietrichGebert/ponytail");
        assert_eq!(ok("https://github.com/a/b/tree/main/plugins"), "a/b");
        assert_eq!(ok("github.com/a/b"), "a/b");
        assert_eq!(ok("git@github.com:a/b.git"), "a/b");
        assert_eq!(ok("DietrichGebert/ponytail"), "DietrichGebert/ponytail");
        assert_eq!(ok("https://gitlab.com/a/b.git"), "https://gitlab.com/a/b.git");
        for bad in ["", "   ", "--help", "-x/y", "C:\\plugins\\x", "/etc/passwd", "../x", "a b/c", "https://github.com/onlyowner", "one", "a/b/c"] {
            assert!(normalize_source(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn links_keep_ref_and_subpath() {
        let l = |s: &str| parse_link(s).unwrap();
        let tree = l("https://github.com/anthropics/skills/tree/main/skills/pdf");
        assert_eq!((tree.source.as_str(), tree.git_ref.as_deref(), tree.subpath.as_deref()), ("anthropics/skills", Some("main"), Some("skills/pdf")));
        assert_eq!(tree.text(), "https://github.com/anthropics/skills/tree/main/skills/pdf");
        assert_eq!(l(&tree.text()), tree);
        let blob = l("https://github.com/a/b/blob/v1.2/skills/x/SKILL.md?plain=1");
        assert_eq!((blob.git_ref.as_deref(), blob.subpath.as_deref()), (Some("v1.2"), Some("skills/x")));
        let root = l("https://github.com/a/b/tree/dev");
        assert_eq!((root.git_ref.as_deref(), root.subpath.as_deref()), (Some("dev"), None));
        let plain = l("a/b");
        assert_eq!((plain.git_ref, plain.subpath), (None, None));
        let url = l("https://gitlab.com/a/b.git");
        assert_eq!((url.clone_url().as_str(), url.text().as_str()), ("https://gitlab.com/a/b.git", "https://gitlab.com/a/b.git"));
        assert_eq!(l("a/b").clone_url(), "https://github.com/a/b.git");
        for bad in ["https://github.com/a/b/tree/main/../x", "https://github.com/a/b/tree/-x/y", "https://github.com/a/b/tree"] {
            assert!(parse_link(bad).is_err(), "{bad:?} must be refused");
        }
    }

    fn skill_md(dir: &Path, name: &str, header: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), format!("---\n{header}\n---\nbody {name}\n")).unwrap();
        std::fs::write(dir.join("extra.txt"), name).unwrap();
    }

    #[test]
    fn frontmatter_is_validated() {
        assert_eq!(frontmatter_value("---\nname: pdf\ndescription: \"Do PDFs\"\n---\nx", "description").as_deref(), Some("Do PDFs"));
        assert_eq!(frontmatter_value("---\nname: a\ndescription: >\n  one\n  two\nlicense: x\n---\n", "description").as_deref(), Some("one two"));
        assert_eq!(frontmatter_value("no header", "name"), None);
        let dir = tempfile::tempdir().unwrap();
        skill_md(&dir.path().join("ok"), "ok", "name: ok\ndescription: fine");
        skill_md(&dir.path().join("nodesc"), "nodesc", "name: nodesc");
        skill_md(&dir.path().join("badname"), "x", "name: ../evil\ndescription: d");
        assert!(read_skill(&dir.path().join("ok")).is_ok());
        assert!(read_skill(&dir.path().join("nodesc")).unwrap_err().contains("description"));
        assert!(read_skill(&dir.path().join("badname")).unwrap_err().contains("valid skill name"));
    }

    #[test]
    fn skills_are_found_in_the_usual_places() {
        let names = |base: &Path| find_skills(base).map(|v| v.into_iter().map(|s| s.name).collect::<Vec<_>>());
        let t = tempfile::tempdir().unwrap();
        let (root, nested, flat, none) = (t.path().join("root"), t.path().join("nested"), t.path().join("flat"), t.path().join("none"));
        skill_md(&root, "solo", "name: solo\ndescription: d");
        skill_md(&root.join("skills/ignored"), "i", "name: ignored\ndescription: d");
        skill_md(&nested.join("skills/b"), "b", "name: b\ndescription: d");
        skill_md(&nested.join("skills/a"), "a", "name: a\ndescription: d");
        skill_md(&nested.join("c"), "c", "name: c\ndescription: d");
        skill_md(&nested.join("skills/broken"), "x", "name: broken");
        skill_md(&flat.join("only"), "only", "name: only\ndescription: d");
        std::fs::create_dir_all(&none).unwrap();
        assert_eq!(names(&root).unwrap(), ["solo"]); // SKILL.md in the folder wins
        assert_eq!(names(&nested).unwrap(), ["a", "b", "c"]); // invalid one skipped
        assert_eq!(names(&flat).unwrap(), ["only"]);
        assert!(names(&none).unwrap_err().contains("no SKILL.md"));
    }

    #[test]
    fn a_skill_is_wrapped_as_a_plugin_and_unwrapped() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("src/pdf");
        skill_md(&src, "pdf", "name: pdf\ndescription: Work with PDFs");
        std::fs::create_dir_all(src.join("scripts")).unwrap();
        std::fs::write(src.join("scripts/run.py"), "print()").unwrap();
        std::fs::create_dir_all(src.join(".git")).unwrap();
        std::fs::write(src.join(".git/config"), "x").unwrap();
        let skill = read_skill(&src).unwrap();
        let market = t.path().join("aurelius-skills");
        wrap_skill(&market, &skill, "https://github.com/a/b", "abc123").unwrap();
        wrap_skill(&market, &skill, "https://github.com/a/b", "abc124").unwrap(); // again: replaced, not doubled
        let plugin = market.join("plugins/pdf");
        assert!(plugin.join("skills/pdf/SKILL.md").is_file() && plugin.join("skills/pdf/scripts/run.py").is_file());
        assert!(!plugin.join("skills/pdf/.git").exists());
        let manifest = read_json(&plugin.join(".claude-plugin/plugin.json")).unwrap();
        assert_eq!((manifest["name"].as_str(), manifest["version"].as_str()), (Some("pdf"), Some("abc124")));
        assert_eq!(components_of(&plugin).skills, ["pdf"]);
        let listing = read_json(&market.join(".claude-plugin/marketplace.json")).unwrap();
        assert_eq!(listing["name"], SKILLS_MARKETPLACE);
        assert_eq!(listing["plugins"].as_array().unwrap().len(), 1);
        assert_eq!((listing["plugins"][0]["source"].as_str(), listing["plugins"][0]["description"].as_str()), (Some("./plugins/pdf"), Some("Work with PDFs")));
        assert_eq!(sources_in(&market)["pdf"], "https://github.com/a/b");
        assert_eq!(kind_of(SKILLS_MARKETPLACE, "pdf", &sources_in(&market)), (PluginKind::Skill, Some("https://github.com/a/b".into())));
        assert_eq!(kind_of("ponytail", "pdf", &sources_in(&market)), (PluginKind::Plugin, None));

        unwrap_skill(&market, "pdf").unwrap();
        assert!(!plugin.exists());
        assert!(read_json(&market.join(".claude-plugin/marketplace.json")).unwrap()["plugins"].as_array().unwrap().is_empty());
    }

    #[test]
    fn employee_runs_load_the_task_projects_plugins_and_their_own() {
        use aurelius_domain::conversation::ConversationKind;
        use aurelius_storage::{conversations, events, employees};
        let dir = tempfile::tempdir().unwrap();
        let store = aurelius_storage::Store::open(dir.path()).unwrap();
        let (project, other) = (ProjectId::new(), ProjectId::new());
        let (lead, plain) = (EmployeeId::new(), EmployeeId::new());
        let (lead_chat, plain_chat, task, other_task, assistant) =
            (ConversationId::new(), ConversationId::new(), ConversationId::new(), ConversationId::new(), ConversationId::new());
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        store
            .write(move |tx| {
                events::insert_project_projection(tx, project, "Atlas", None)?;
                events::insert_project_projection(tx, other, "Other", None)?;
                employees::insert_employee(tx, lead, "Lead", "Dev", None, None, None, None, &serde_json::json!({}), lead_chat)?;
                employees::insert_employee(tx, plain, "Plain", "Dev", None, None, None, None, &serde_json::json!({}), plain_chat)?;
                employees::insert_thread(tx, lead, task, ConversationKind::Task, Some(project))?;
                employees::insert_thread(tx, lead, other_task, ConversationKind::Task, Some(other))?;
                conversations::insert_project_conversation(tx, assistant, project, "Assistant")?;
                employees::set_project_plugins(tx, project, &ids(&["p@m", "shared@m"]))?;
                employees::set_project_plugins(tx, other, &ids(&["o@m"]))?;
                employees::set_plugins(tx, lead, &ids(&["shared@m", "own@m"]))
            })
            .unwrap();
        let run = |c: ConversationId| store.read(|conn| enabled_for_run(conn, c)).unwrap();
        // In a task: the task project's list first, then the employee's own, once each.
        assert_eq!(run(task), ids(&["p@m", "shared@m", "own@m"]));
        assert_eq!(run(other_task), ids(&["o@m", "shared@m", "own@m"]));
        // In its chat (no project): its own only.
        assert_eq!(run(lead_chat), ids(&["shared@m", "own@m"]));
        assert!(run(plain_chat).is_empty());
        // The assistant keeps the project's list; an unknown conversation has none.
        assert_eq!(run(assistant), ids(&["p@m", "shared@m"]));
        assert!(run(ConversationId::new()).is_empty());
        assert_eq!(store.read(|c| enabled_of(c, lead)).unwrap(), ids(&["shared@m", "own@m"]));
    }

    #[test]
    fn employee_plugins_from_older_servers_still_parse() {
        let old = serde_json::json!({ "employee_id": EmployeeId::new(), "enabled": ["a@m"], "missing": [], "inherited": ["p@m"] });
        let parsed: EmployeePlugins = serde_json::from_value(old).unwrap();
        assert_eq!(parsed.enabled, vec!["a@m".to_string()]);
    }

    #[test]
    fn ids_are_name_at_marketplace() {
        assert_eq!(check_id(" ponytail@ponytail ").unwrap(), "ponytail@ponytail");
        for bad in ["", "ponytail", "@x", "x@", "-x@y", "a b@c", "x@y;rm", "a@b@c"] {
            assert!(check_id(bad).is_err(), "{bad:?} must be refused");
        }
        assert_eq!(split_id("ponytail@ponytail"), ("ponytail".into(), "ponytail".into()));
    }

    #[test]
    fn marketplace_name_is_read_from_the_cli_message() {
        assert_eq!(marketplace_name("Cloning...\n✔ Successfully added marketplace: ponytail (declared in user settings)\n").as_deref(), Some("ponytail"));
        assert_eq!(marketplace_name("✔ Marketplace 'x' is already added"), None);
    }

    #[test]
    fn install_paths_come_from_claudes_file() {
        let file = serde_json::json!({ "version": 2, "plugins": { "a@m": [{ "scope": "user", "installPath": "C:/x/a" }], "b@m": [] } });
        let paths = install_paths_from(&file);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths["a@m"], PathBuf::from("C:/x/a"));
    }

    #[test]
    fn components_are_read_from_the_plugin_folder() {
        let dir = std::env::temp_dir().join(format!("aurelius-plugin-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("skills/one")).unwrap();
        std::fs::create_dir_all(dir.join("skills/not-a-skill")).unwrap();
        std::fs::create_dir_all(dir.join("commands")).unwrap();
        std::fs::create_dir_all(dir.join(".claude-plugin")).unwrap();
        std::fs::write(dir.join("skills/one/SKILL.md"), "x").unwrap();
        std::fs::write(dir.join("commands/go.md"), "x").unwrap();
        std::fs::write(dir.join("commands/readme.txt"), "x").unwrap();
        std::fs::write(dir.join(".mcp.json"), r#"{"mcpServers":{"srv":{}}}"#).unwrap();
        std::fs::write(dir.join(".claude-plugin/plugin.json"), r#"{"name":"p","description":"d","hooks":"./hooks/h.json"}"#).unwrap();
        let c = components_of(&dir);
        assert_eq!((c.skills, c.commands, c.agents, c.hooks, c.mcp_servers), (vec!["one".to_string()], vec!["go".to_string()], vec![], true, vec!["srv".to_string()]));
        assert_eq!(manifest_description(&dir).as_deref(), Some("d"));
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(components_of(&dir), PluginComponents::default());
    }

    /// Needs network, git and Claude Code: `cargo test -p aurelius-host live_pdf -- --ignored --nocapture`.
    /// Uses a temporary data dir and plugin config, so the real ones stay untouched.
    #[tokio::test]
    #[ignore = "network and Claude Code"]
    async fn live_pdf_skill_is_previewed_installed_and_listed_in_a_run() {
        let t = tempfile::tempdir().unwrap();
        let real_config = claude_home();
        // SAFETY: the only test that sets these; it is run alone with --ignored.
        unsafe {
            std::env::set_var(crate::DATA_DIR_ENV, t.path().join("data"));
            std::env::set_var("CLAUDE_CONFIG_DIR", t.path().join("claude"));
        }
        let preview = preview("https://github.com/anthropics/skills/tree/main/skills/pdf".into()).await.unwrap();
        assert_eq!(preview.marketplace, SKILLS_MARKETPLACE);
        let offer = preview.offers.iter().find(|o| o.name == "pdf").expect("pdf offered");
        assert_eq!(offer.kind, PluginKind::Skill);
        assert_eq!(offer.components.as_ref().map(|c| c.skills.clone()), Some(vec!["pdf".to_string()]));
        claude(&["plugin", "install", &offer.id], INSTALL_TIMEOUT).await.unwrap();
        let path = installed_paths()[&offer.id].clone();
        // A run as an employee's run starts it: --plugin-dir <installPath>, with the user's own login.
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", real_config.unwrap()) };
        let out = claude(&["-p", "hi", "--max-turns", "1", "--output-format", "stream-json", "--verbose", "--plugin-dir", path.to_str().unwrap(), "--tools", "Skill"], Duration::from_secs(120)).await.unwrap();
        let init = out.lines().find(|l| l.contains("\"subtype\":\"init\"")).expect("init line");
        assert!(init.contains("pdf:pdf"), "skill missing in run: {init}");
    }
}
