//! The MCP servers the user already has in Claude Code, for every Aurelius run.
//!
//! Discovery reads the user-scope servers of Claude Code's `~/.claude.json`
//! (top-level `mcpServers`). One server-wide list (`mcp/set`) says which are
//! enabled; an enabled server's full config is read again at every run start
//! and goes into the run's `--mcp-config`. Clients only get names, scope,
//! transport and a summary: env values, headers and tokens stay on this machine.

use crate::Shared;
use aurelius_protocol::{ErrorCode, McpAddParams, McpAlwaysAllow, McpScope, McpServerInfo, McpServers, McpTransport, ProtocolError};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A server as found in the user's files, with its full (secret) config.
#[derive(Debug, Clone)]
pub struct Found {
    pub name: String,
    pub scope: McpScope,
    pub config: Value,
}

const SETTINGS_KEY: &str = "mcp_servers";

fn internal(e: impl ToString) -> ProtocolError {
    ProtocolError::new(ErrorCode::Internal, e.to_string())
}

fn home_claude_json() -> Option<PathBuf> {
    let home = std::env::var_os("AURELIUS_CLAUDE_JSON").map(PathBuf::from).or_else(|| {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
        Some(PathBuf::from(home).join(".claude.json"))
    });
    home
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn servers_of(value: &Value, scope: McpScope, out: &mut BTreeMap<String, Found>) {
    let Some(map) = value.as_object() else { return };
    for (name, config) in map {
        if config.is_object() && !aurelius_runtime::claude_code::is_reserved_server(name) {
            out.insert(name.clone(), Found { name: name.clone(), scope, config: config.clone() });
        }
    }
}

/// Every user-scope server Claude Code has.
pub fn discover() -> Vec<Found> {
    let mut found = BTreeMap::new();
    if let Some(claude) = home_claude_json().and_then(|p| read_json(&p)) {
        servers_of(&claude["mcpServers"], McpScope::User, &mut found);
    }
    found.into_values().collect()
}

pub fn transport_of(config: &Value) -> McpTransport {
    match config["type"].as_str() {
        Some("sse") => McpTransport::Sse,
        Some("http") | Some("streamable-http") => McpTransport::Http,
        Some("stdio") => McpTransport::Stdio,
        _ if config["url"].is_string() => McpTransport::Http,
        _ => McpTransport::Stdio,
    }
}

/// What a client may see: the program (no arguments, they may hold tokens) or
/// the URL without credentials, query or fragment.
pub fn summary_of(config: &Value) -> String {
    match transport_of(config) {
        McpTransport::Stdio => config["command"].as_str().unwrap_or("").to_string(),
        _ => {
            let url = config["url"].as_str().unwrap_or("");
            let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
            let rest = rest.split(['?', '#']).next().unwrap_or("");
            let rest = rest.rsplit_once('@').map(|(_, host)| host).unwrap_or(rest);
            if scheme.is_empty() { rest.to_string() } else { format!("{scheme}://{rest}") }
        }
    }
}

fn info(found: &Found, enabled: &[String], auth: aurelius_protocol::McpAuthStatus) -> McpServerInfo {
    McpServerInfo {
        name: found.name.clone(),
        scope: found.scope,
        transport: transport_of(&found.config),
        summary: summary_of(&found.config),
        enabled: enabled.contains(&found.name),
        auth,
    }
}

/// The server-wide list of enabled names.
pub fn enabled_names(conn: &aurelius_storage::Conn) -> Result<Vec<String>, aurelius_storage::StorageError> {
    let json = aurelius_storage::settings::get(conn, SETTINGS_KEY)?;
    Ok(json.and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok()).unwrap_or_default())
}

/// `files`: the board member's own Claude login (their MCP logins); none: none of them.
fn listing(conn: &aurelius_storage::Conn, files: Option<crate::mcp_oauth::ClaudeFiles>) -> Result<McpServers, ProtocolError> {
    let enabled = enabled_names(conn).map_err(|e| ProtocolError::new(ErrorCode::StorageUnavailable, e.to_string()))?;
    let found = discover();
    let missing = enabled.iter().filter(|n| !found.iter().any(|f| &f.name == *n)).cloned().collect();
    // The OAuth status comes from Claude Code's own files of the board member's login (no network here).
    let (credentials, needs_auth) = files.map(|f| (f.credentials(), f.needs_auth())).unwrap_or_default();
    let now = aurelius_storage::now_ms();
    let auth = |f: &Found| crate::mcp_oauth::status_of(&f.name, &f.config, &credentials, &needs_auth, now);
    Ok(McpServers { servers: found.iter().map(|f| info(f, &enabled, auth(f))).collect(), missing })
}

/// `mcp/list`, with the MCP logins of `user`'s own Claude login.
pub async fn list(shared: &Arc<Shared>, user: Option<aurelius_domain::UserId>) -> Result<McpServers, ProtocolError> {
    let store = shared.store().clone();
    let files = crate::mcp_oauth::ClaudeFiles::locate(&shared.root, user);
    tokio::task::spawn_blocking(move || store.read(|c| Ok(listing(c, files))).map_err(internal)?).await.map_err(internal)?
}

fn clean_names(enabled: Vec<String>) -> Result<Vec<String>, ProtocolError> {
    let mut names: Vec<String> = Vec::new();
    for name in enabled {
        let name = name.trim().to_string();
        if name.is_empty() || name.len() > 200 || aurelius_runtime::claude_code::is_reserved_server(&name) {
            return Err(ProtocolError::new(ErrorCode::InvalidRequest, format!("'{name}' is not an MCP server you can enable.")));
        }
        if !names.contains(&name) {
            names.push(name);
        }
    }
    Ok(names)
}

/// `mcp/set`: replaces the server-wide list (names only; the config is read from
/// Claude Code's file at each run). Applies from each run that starts afterwards.
pub async fn set(shared: &Arc<Shared>, enabled: Vec<String>, user: Option<aurelius_domain::UserId>) -> Result<McpServers, ProtocolError> {
    let names = clean_names(enabled)?;
    let files = crate::mcp_oauth::ClaudeFiles::locate(&shared.root, user);
    let json = serde_json::to_string(&names).map_err(internal)?;
    let store = shared.store().clone();
    tokio::task::spawn_blocking(move || {
        store.write(move |tx| aurelius_storage::settings::put(tx, SETTINGS_KEY, &json, aurelius_storage::now_ms())).map_err(internal)?;
        store.read(|c| Ok(listing(c, files))).map_err(internal)?
    })
    .await
    .map_err(internal)?
}

fn invalid(message: impl ToString) -> ProtocolError {
    ProtocolError::new(ErrorCode::InvalidRequest, message.to_string())
}

/// A server name that is safe as a CLI argument and as `mcp__<name>__tool`.
fn check_name(name: &str) -> Result<(), ProtocolError> {
    let ok = !name.is_empty() && !name.starts_with('-') && name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if !ok {
        return Err(invalid("The name may only use letters, digits, '-' and '_' (up to 64 characters)."));
    }
    if aurelius_runtime::claude_code::is_reserved_server(name) {
        return Err(invalid(format!("'{name}' is reserved; choose another name.")));
    }
    Ok(())
}

fn no_control(text: &str) -> bool {
    !text.chars().any(|c| c.is_control())
}

/// The arguments for `claude mcp add --scope user`, after checking the input.
/// The name comes first because `-e` and `-H` take several values.
fn add_args(p: &McpAddParams) -> Result<Vec<String>, ProtocolError> {
    check_name(&p.name)?;
    let mut args: Vec<String> = ["mcp", "add", "--scope", "user", "--transport"].map(String::from).into();
    match p.transport {
        McpTransport::Stdio => {
            let command = p.command.as_deref().map(str::trim).unwrap_or("");
            if command.is_empty() || !no_control(command) || command.len() > 1000 {
                return Err(invalid("A stdio server needs a command."));
            }
            if p.url.is_some() || !p.headers.is_empty() {
                return Err(invalid("A stdio server has a command and env, not a URL or headers."));
            }
            if p.args.iter().any(|a| !no_control(a) || a.len() > 4000) {
                return Err(invalid("An argument has unusable characters."));
            }
            args.extend(["stdio".into(), p.name.clone()]);
            for (key, value) in &p.env {
                let key_ok = key.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !key_ok || !no_control(value) {
                    return Err(invalid(format!("'{key}' is not a usable environment variable.")));
                }
                args.extend(["-e".into(), format!("{key}={value}")]);
            }
            args.extend(["--".into(), command.to_string()]);
            args.extend(p.args.iter().cloned());
        }
        McpTransport::Http | McpTransport::Sse => {
            let url = p.url.as_deref().map(str::trim).unwrap_or("");
            if !(url.starts_with("http://") || url.starts_with("https://")) || !no_control(url) || url.chars().any(char::is_whitespace) || url.len() > 2000 {
                return Err(invalid("Give an http:// or https:// URL."));
            }
            if p.command.is_some() || !p.args.is_empty() || !p.env.is_empty() {
                return Err(invalid("An http or sse server has a URL and headers, not a command or env."));
            }
            args.extend([if p.transport == McpTransport::Http { "http" } else { "sse" }.into(), p.name.clone(), url.to_string()]);
            for (key, value) in &p.headers {
                let key_ok = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
                if !key_ok || !no_control(value) {
                    return Err(invalid(format!("'{key}' is not a usable header name.")));
                }
                args.extend(["-H".into(), format!("{key}: {value}")]);
            }
        }
    }
    Ok(args)
}

/// Every secret the request carried, longest first, so error text can be cleaned.
fn secrets_of(p: &McpAddParams) -> Vec<String> {
    let mut secrets: Vec<String> = p.env.values().chain(p.headers.values()).chain(p.args.iter()).cloned().collect();
    if let Some(url) = &p.url {
        // The URL may hold credentials or a key in its query.
        let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
        if let Some((_, query)) = rest.split_once('?') {
            secrets.push(query.to_string());
        }
        if let Some((cred, _)) = rest.split('/').next().and_then(|h| h.rsplit_once('@')) {
            secrets.push(cred.to_string());
        }
    }
    secrets.retain(|s| s.len() >= 3);
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    secrets.dedup();
    secrets
}

fn redact(text: &str, secrets: &[String]) -> String {
    secrets.iter().fold(text.to_string(), |t, s| t.replace(s.as_str(), "***"))
}

fn write_enabled(shared: &Arc<Shared>, change: impl FnOnce(&mut Vec<String>) + Send + 'static) -> Result<(), ProtocolError> {
    let store = shared.store().clone();
    let mut names = store.read(|c| Ok(enabled_names(c).unwrap_or_default())).map_err(internal)?;
    change(&mut names);
    let json = serde_json::to_string(&names).map_err(internal)?;
    store.write(move |tx| aurelius_storage::settings::put(tx, SETTINGS_KEY, &json, aurelius_storage::now_ms())).map_err(internal)
}

/// `mcp/add`: Claude Code adds the server (we never edit its file); then it is
/// enabled when asked. Errors carry the CLI's text without any secret value.
pub async fn add(shared: &Arc<Shared>, params: McpAddParams, user: Option<aurelius_domain::UserId>) -> Result<McpServers, ProtocolError> {
    let args = add_args(&params)?;
    if discover().iter().any(|f| f.name == params.name) {
        return Err(invalid(format!("A server named '{}' already exists.", params.name)));
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    if let Err(e) = aurelius_host::plugins::claude(&refs, std::time::Duration::from_secs(60)).await {
        return Err(ProtocolError::new(e.code, redact(&e.message, &secrets_of(&params))));
    }
    if !discover().iter().any(|f| f.name == params.name) {
        return Err(internal("Claude Code did not add the server."));
    }
    if params.enable {
        let name = params.name.clone();
        let shared2 = shared.clone();
        tokio::task::spawn_blocking(move || write_enabled(&shared2, move |names| if !names.contains(&name) { names.push(name) }))
            .await
            .map_err(internal)??;
    }
    list(shared, user).await
}

/// `mcp/remove`: `claude mcp remove --scope user`, then off the enabled list.
pub async fn remove(shared: &Arc<Shared>, name: String, user: Option<aurelius_domain::UserId>) -> Result<McpServers, ProtocolError> {
    check_name(&name)?;
    if !discover().iter().any(|f| f.name == name) {
        return Err(invalid(format!("There is no user-scope server named '{name}'.")));
    }
    aurelius_host::plugins::claude(&["mcp", "remove", "--scope", "user", &name], std::time::Duration::from_secs(60)).await?;
    let shared2 = shared.clone();
    tokio::task::spawn_blocking(move || write_enabled(&shared2, move |names| names.retain(|n| *n != name))).await.map_err(internal)??;
    list(shared, user).await
}

/// The full configs of the enabled servers that still exist, read now.
/// Empty when none is enabled (the run is then as before).
pub fn configs_for_run(shared: &Shared) -> BTreeMap<String, Value> {
    let enabled = shared.store().read(|c| Ok(enabled_names(c).unwrap_or_default())).unwrap_or_default();
    if enabled.is_empty() {
        return BTreeMap::new();
    }
    discover().into_iter().filter(|f| enabled.contains(&f.name)).map(|f| (f.name, f.config)).collect()
}

/// Whether `tool` (`mcp__<server>__<tool>`) belongs to an enabled server.
/// Auto lets these through, like the browser; the other modes ask.
pub async fn tool_is_enabled(shared: &Arc<Shared>, tool: &str) -> bool {
    let Some(server) = aurelius_runtime::claude_code::mcp_tool_server(tool).map(str::to_owned) else { return false };
    if aurelius_runtime::claude_code::is_reserved_server(&server) {
        return false;
    }
    let store = shared.store().clone();
    tokio::task::spawn_blocking(move || {
        store
            .read(|c| Ok(enabled_names(c).unwrap_or_default()))
            .map(|names| names.iter().any(|n| aurelius_runtime::claude_code::tool_server_name(n) == server))
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// `mcp_always_allow.list`.
pub async fn always_allow_list(shared: &Arc<Shared>) -> Result<McpAlwaysAllow, ProtocolError> {
    let store = shared.store().clone();
    tokio::task::spawn_blocking(move || store.read(|c| aurelius_storage::settings::mcp_always_allow(c)).map(|tools| McpAlwaysAllow { tools }).map_err(internal))
        .await
        .map_err(internal)?
}

/// `mcp_always_allow.remove`: the tool asks again from its next call. Removing one
/// that is not on the list changes nothing.
pub async fn always_allow_remove(shared: &Arc<Shared>, tool: String) -> Result<McpAlwaysAllow, ProtocolError> {
    let store = shared.store().clone();
    tokio::task::spawn_blocking(move || {
        store
            .write(move |tx| {
                let mut tools = aurelius_storage::settings::mcp_always_allow(tx)?;
                tools.retain(|t| *t != tool);
                aurelius_storage::settings::set_mcp_always_allow(tx, &tools, aurelius_storage::now_ms())?;
                Ok(McpAlwaysAllow { tools })
            })
            .map_err(internal)
    })
    .await
    .map_err(internal)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_never_carry_secrets() {
        let stdio = serde_json::json!({ "type": "stdio", "command": "npx", "args": ["-y", "pkg", "--token", "SECRET"], "env": { "KEY": "SECRET" } });
        assert_eq!((transport_of(&stdio), summary_of(&stdio).as_str()), (McpTransport::Stdio, "npx"));
        let http = serde_json::json!({ "type": "http", "url": "https://user:SECRET@mcp.example.com/v1?key=SECRET#x", "headers": { "Authorization": "Bearer SECRET" } });
        assert_eq!((transport_of(&http), summary_of(&http).as_str()), (McpTransport::Http, "https://mcp.example.com/v1"));
        assert_eq!(transport_of(&serde_json::json!({ "type": "sse", "url": "https://a/sse" })), McpTransport::Sse);
        assert_eq!(transport_of(&serde_json::json!({ "command": "x" })), McpTransport::Stdio);
        assert_eq!(transport_of(&serde_json::json!({ "url": "https://a" })), McpTransport::Http);
    }

    fn stdio() -> McpAddParams {
        McpAddParams {
            name: "everything".into(),
            transport: McpTransport::Stdio,
            command: Some("npx".into()),
            args: vec!["-y".into(), "pkg".into()],
            env: [("API_KEY".to_string(), "SECRETVALUE".to_string())].into(),
            url: None,
            headers: Default::default(),
            enable: true,
        }
    }

    #[test]
    fn add_builds_the_cli_call() {
        let args = add_args(&stdio()).unwrap();
        assert_eq!(args.join(" "), "mcp add --scope user --transport stdio everything -e API_KEY=SECRETVALUE -- npx -y pkg");
        let http = McpAddParams {
            transport: McpTransport::Sse,
            command: None,
            args: vec![],
            env: Default::default(),
            url: Some("https://a.example/sse?k=SECRETKEY".into()),
            headers: [("Authorization".to_string(), "Bearer SECRETTOKEN".to_string())].into(),
            ..stdio()
        };
        assert_eq!(add_args(&http).unwrap().join(" "), "mcp add --scope user --transport sse everything https://a.example/sse?k=SECRETKEY -H Authorization: Bearer SECRETTOKEN");
    }

    #[test]
    fn add_refuses_bad_input() {
        for name in ["", "a b", "-x", "a/b", "aurelius", "claude-in-chrome", &"x".repeat(65)] {
            assert!(add_args(&McpAddParams { name: name.into(), ..stdio() }).is_err(), "{name}");
        }
        assert!(add_args(&McpAddParams { command: None, ..stdio() }).is_err());
        assert!(add_args(&McpAddParams { env: [("A=B".to_string(), "x".to_string())].into(), ..stdio() }).is_err());
        assert!(add_args(&McpAddParams { transport: McpTransport::Http, url: Some("file:///x".into()), command: None, ..stdio() }).is_err());
        assert!(add_args(&McpAddParams { url: Some("https://a".into()), ..stdio() }).is_err());
    }

    #[test]
    fn error_text_hides_values() {
        let p = McpAddParams {
            url: Some("https://user:PASSWORD@a.example/x?k=SECRETKEY".into()),
            headers: [("Authorization".to_string(), "Bearer SECRETTOKEN".to_string())].into(),
            ..stdio()
        };
        let text = "failed: SECRETVALUE Bearer SECRETTOKEN k=SECRETKEY user:PASSWORD";
        let clean = redact(text, &secrets_of(&p));
        for secret in ["SECRETVALUE", "SECRETTOKEN", "SECRETKEY", "PASSWORD"] {
            assert!(!clean.contains(secret), "{clean}");
        }
    }

    #[test]
    fn discovery_reads_user_scope_only() {
        let dir = std::env::temp_dir().join(format!("aurelius-mcp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let claude = serde_json::json!({
            "mcpServers": { "u": { "command": "user" }, "aurelius": { "command": "no" }, "claude-in-chrome": { "command": "no" } },
            "projects": { "x": { "mcpServers": { "l": { "type": "http", "url": "https://x/y" } } } }
        });
        let file = dir.join("claude.json");
        std::fs::write(&file, claude.to_string()).unwrap();
        // SAFETY of the test: a unique variable value, read only by `discover`.
        unsafe { std::env::set_var("AURELIUS_CLAUDE_JSON", &file) };
        let names: Vec<String> = discover().into_iter().map(|f| f.name).collect();
        assert_eq!(names, vec!["u".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
