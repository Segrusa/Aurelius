//! OAuth login for the user's remote (http/sse) MCP servers, so runs get their
//! tools without `claude /mcp`.
//!
//! Claude Code keeps MCP tokens in its credential file (`.credentials.json` of the
//! board member's own login folder, `<data>/logins/<user_id>/`,
//! `mcpOAuth["<name>|<hash of type, url, headers>"]`) and refreshes them itself.
//! Aurelius changes only that `mcpOAuth` entry: the Claude login (`claudeAiOauth`)
//! in the same file is never logged, sent or changed (Sentinel D6), and the file
//! is written with mode 0600.
//! `claude mcp login` needs a terminal, so the daemon runs the MCP authorization
//! flow (protected-resource and authorization-server metadata, dynamic client
//! registration, PKCE) and writes the result into that same entry. The browser
//! comes back to a listener on `http://localhost:<port>/callback`; a browser on
//! another device ends on a page that does not load, and the client sends that
//! address (`mcp.login_complete`). Tokens, the code verifier and client secrets
//! never leave this module: clients only get `McpLogin` and `McpAuthStatus`.

use crate::Shared;
use aurelius_domain::EventKind;
use aurelius_protocol::{ErrorCode, McpAuthStatus, McpLogin, McpLoginState, McpServers, McpTransport, ProtocolError};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// How long a started login waits for the browser.
const LOGIN_TTL_MS: i64 = 10 * 60_000;
/// How long `mcp.login_get` still knows a finished login.
const KEEP_FINISHED_MS: i64 = 30 * 60_000;
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

fn invalid(message: impl ToString) -> ProtocolError {
    ProtocolError::new(ErrorCode::InvalidRequest, message.to_string())
}

fn internal(message: impl ToString) -> ProtocolError {
    ProtocolError::new(ErrorCode::Internal, message.to_string())
}

// ---------------------------------------------------------------------------
// Claude Code's files
// ---------------------------------------------------------------------------

/// Claude Code's credential file and its cache of servers that asked for a login.
#[derive(Debug, Clone)]
pub struct ClaudeFiles {
    pub credentials: PathBuf,
    pub needs_auth_cache: PathBuf,
}

impl ClaudeFiles {
    /// The files of the Claude login in `dir`.
    pub fn in_dir(dir: &std::path::Path) -> Self {
        Self { credentials: dir.join(".credentials.json"), needs_auth_cache: dir.join("mcp-needs-auth-cache.json") }
    }

    /// `AURELIUS_CLAUDE_CREDENTIALS` (tests: only in a debug build, so a released
    /// daemon never lets every board member share one file, SEC2 m7), else the
    /// files of `user`'s own Claude login (`<root>/logins/<user_id>/`); none
    /// without a board member: never the daemon user's `~/.claude`.
    pub fn locate(root: &std::path::Path, user: Option<aurelius_domain::UserId>) -> Option<Self> {
        if let Some(file) = std::env::var_os("AURELIUS_CLAUDE_CREDENTIALS").filter(|_| cfg!(debug_assertions)).map(PathBuf::from) {
            let dir = file.parent().map(PathBuf::from).unwrap_or_default();
            return Some(Self { credentials: file, needs_auth_cache: dir.join("mcp-needs-auth-cache.json") });
        }
        let login = aurelius_runtime::claude_code::ClaudeLogin::of(root, user?).ok()?;
        Some(Self::in_dir(login.config_dir()))
    }

    fn read(path: &std::path::Path) -> Value {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| json!({}))
    }

    pub fn credentials(&self) -> Value {
        Self::read(&self.credentials)
    }

    pub fn needs_auth(&self) -> Value {
        Self::read(&self.needs_auth_cache)
    }

    /// Change the file in place (written next to it, then moved over it). A file
    /// that exists but does not parse is never overwritten.
    fn update(path: &std::path::Path, change: impl FnOnce(&mut Map<String, Value>)) -> Result<(), String> {
        let mut root = match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                Ok(Value::Object(map)) => map,
                _ => return Err(format!("{} is not readable JSON; it was left as it is.", path.display())),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
            Err(e) => return Err(format!("Could not read {}: {e}", path.display())),
        };
        change(&mut root);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.aurelius-tmp");
        let text = serde_json::to_vec_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
        // Only its owner may read it (D6): made 0600 (never the umask's 0644), and
        // never wider than the original was.
        let _ = std::fs::remove_file(&tmp);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let original = std::fs::metadata(path).ok().map(|m| m.permissions().mode() & 0o600).filter(|m| m & 0o400 != 0);
            options.mode(original.unwrap_or(0o600));
        }
        let written = options.open(&tmp).and_then(|mut file| std::io::Write::write_all(&mut file, &text).and_then(|_| file.sync_all()));
        written.map_err(|e| format!("Could not write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("Could not replace {}: {e}", path.display()))
    }

    fn update_entry(&self, key: &str, change: impl FnOnce(&mut Map<String, Value>)) -> Result<(), String> {
        Self::update(&self.credentials, |root| {
            let all = root.entry("mcpOAuth").or_insert_with(|| json!({}));
            if !all.is_object() {
                *all = json!({});
            }
            let entry = all.as_object_mut().expect("object").entry(key).or_insert_with(|| json!({}));
            if !entry.is_object() {
                *entry = json!({});
            }
            change(entry.as_object_mut().expect("object"));
        })
    }

    /// Claude Code skips a server it saw asking for a login for a while; after a login it must try again.
    fn forget_needs_auth(&self, name: &str) {
        if self.needs_auth().get(name).is_some() {
            let _ = Self::update(&self.needs_auth_cache, |root| {
                root.remove(name);
            });
        }
    }
}

/// Claude Code's key of a server's tokens: `<name>|` and the first 16 hex digits of
/// the SHA-256 of `JSON.stringify({type, url, headers: headers || {}})`.
/// The fields are written in that order by hand (a JSON map may sort its keys);
/// the headers keep the file's order (`preserve_order`).
pub fn store_key(name: &str, config: &Value) -> String {
    let mut fields: Vec<String> = ["type", "url"]
        .into_iter()
        .filter_map(|field| config.get(field).map(|value| format!("{}:{value}", Value::from(field))))
        .collect();
    let headers = config.get("headers").filter(|h| h.is_object()).cloned().unwrap_or_else(|| json!({}));
    fields.push(format!("\"headers\":{headers}"));
    let digest = hex::encode(Sha256::digest(format!("{{{}}}", fields.join(",")).as_bytes()));
    format!("{name}|{}", &digest[..16])
}

/// A fixed `Authorization` header (or a header helper) instead of OAuth.
fn fixed_auth(config: &Value) -> bool {
    let header = config["headers"].as_object().is_some_and(|h| h.keys().any(|k| k.eq_ignore_ascii_case("authorization")));
    header || config.get("headersHelper").is_some_and(|h| !h.is_null())
}

fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.is_empty())
}

/// The status of one server from Claude Code's files, without network.
pub fn status_of(name: &str, config: &Value, credentials: &Value, needs_auth: &Value, now_ms: i64) -> McpAuthStatus {
    if crate::mcp_servers::transport_of(config) == McpTransport::Stdio || fixed_auth(config) {
        return McpAuthStatus::None;
    }
    let entry = &credentials["mcpOAuth"][store_key(name, config)];
    if text(&entry["accessToken"]).is_some() {
        let expires = entry["expiresAt"].as_f64().map(|e| e as i64);
        let usable = text(&entry["refreshToken"]).is_some() || expires.is_none_or(|e| e > now_ms);
        return if usable { McpAuthStatus::Authorized } else { McpAuthStatus::NeedsLogin };
    }
    let oauth_known = entry["discoveryState"]["oauthMetadataFound"] == Value::Bool(true) || text(&entry["clientId"]).is_some();
    if oauth_known || needs_auth.get(name).is_some() {
        return McpAuthStatus::NeedsLogin;
    }
    McpAuthStatus::Unknown
}

// ---------------------------------------------------------------------------
// The flow's parts
// ---------------------------------------------------------------------------

fn random_text(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).expect("OS random source");
    URL_SAFE_NO_PAD.encode(buffer)
}

/// PKCE S256 (RFC 7636).
pub fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// A parameter of a `WWW-Authenticate: Bearer k="v", …` header.
pub fn auth_param(header: &str, key: &str) -> Option<String> {
    let mut rest = header;
    while let Some(at) = rest.find(key) {
        let before = rest[..at].chars().last();
        let after = &rest[at + key.len()..];
        if before.is_none_or(|c| c == ' ' || c == ',') {
            if let Some(value) = after.trim_start().strip_prefix('=') {
                let value = value.trim_start();
                if let Some(quoted) = value.strip_prefix('"') {
                    return quoted.split('"').next().map(str::to_owned);
                }
                return value.split([',', ' ']).next().filter(|v| !v.is_empty()).map(str::to_owned);
            }
        }
        rest = after;
    }
    None
}

fn origin_and_path(url: &url::Url) -> (String, String) {
    (url.origin().ascii_serialization(), url.path().trim_end_matches('/').to_string())
}

/// Where the protected-resource metadata may be (RFC 9728), path-specific first.
pub fn resource_metadata_urls(server: &url::Url) -> Vec<String> {
    let (origin, path) = origin_and_path(server);
    let mut urls = Vec::new();
    if !path.is_empty() {
        urls.push(format!("{origin}/.well-known/oauth-protected-resource{path}"));
    }
    urls.push(format!("{origin}/.well-known/oauth-protected-resource"));
    urls
}

/// Where the authorization server's metadata may be (RFC 8414 and OpenID).
pub fn server_metadata_urls(issuer: &url::Url) -> Vec<String> {
    let (origin, path) = origin_and_path(issuer);
    if path.is_empty() {
        return vec![format!("{origin}/.well-known/oauth-authorization-server"), format!("{origin}/.well-known/openid-configuration")];
    }
    vec![
        format!("{origin}/.well-known/oauth-authorization-server{path}"),
        format!("{origin}/.well-known/openid-configuration{path}"),
        format!("{origin}{path}/.well-known/openid-configuration"),
        format!("{origin}{path}/.well-known/oauth-authorization-server"),
    ]
}

/// The query of what the browser ended on: a full URL, `/callback?…`, `?…` or the bare query.
pub fn redirect_params(redirect: &str) -> HashMap<String, String> {
    let redirect = redirect.trim();
    let query = match redirect.split_once('?') {
        Some((_, q)) => q,
        None if redirect.contains('=') => redirect,
        None => "",
    };
    let query = query.split('#').next().unwrap_or("");
    url::form_urlencoded::parse(query.as_bytes()).into_owned().collect()
}

/// What discovery found out about a server.
struct Discovered {
    resource_metadata_url: Option<String>,
    resource: Option<String>,
    scope: Option<String>,
    issuer: String,
    metadata_found: bool,
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: Option<String>,
    auth_methods: Vec<String>,
}

async fn get_json(http: &reqwest::Client, url: &str) -> Option<Value> {
    let response = http.get(url).header(reqwest::header::ACCEPT, "application/json").send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<Value>().await.ok().filter(Value::is_object)
}

fn with_headers(mut request: reqwest::RequestBuilder, config: &Value) -> reqwest::RequestBuilder {
    if let Some(headers) = config["headers"].as_object() {
        for (k, v) in headers {
            if let Some(v) = v.as_str() {
                request = request.header(k.as_str(), v);
            }
        }
    }
    request
}

async fn discover(http: &reqwest::Client, config: &Value, hint: Option<String>) -> Result<Discovered, String> {
    let server_url = config["url"].as_str().unwrap_or("");
    let server = url::Url::parse(server_url).map_err(|_| "The server's URL is not usable.".to_string())?;

    // Ask without a token: a server that needs one answers 401 and says where its metadata is.
    let probe = if crate::mcp_servers::transport_of(config) == McpTransport::Sse {
        http.get(server.clone()).header(reqwest::header::ACCEPT, "text/event-stream")
    } else {
        let initialize = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "aurelius", "version": env!("CARGO_PKG_VERSION") } } });
        http.post(server.clone()).header(reqwest::header::ACCEPT, "application/json, text/event-stream").json(&initialize)
    };
    let (challenge_url, challenge_scope, asks) = match with_headers(probe, config).send().await {
        Ok(response) => {
            let header = response.headers().get(reqwest::header::WWW_AUTHENTICATE).and_then(|h| h.to_str().ok()).unwrap_or("").to_string();
            (auth_param(&header, "resource_metadata"), auth_param(&header, "scope"), response.status() == reqwest::StatusCode::UNAUTHORIZED)
        }
        Err(e) => return Err(format!("The server could not be reached: {}", without_url(&e))),
    };

    let mut candidates: Vec<String> = challenge_url.into_iter().chain(hint).collect();
    candidates.extend(resource_metadata_urls(&server));
    let mut resource_metadata = None;
    for candidate in candidates {
        if let Some(document) = get_json(http, &candidate).await {
            if document["authorization_servers"].as_array().is_some_and(|a| !a.is_empty()) {
                resource_metadata = Some((candidate, document));
                break;
            }
        }
    }
    if resource_metadata.is_none() && !asks {
        return Err("This server does not ask for a login.".into());
    }

    let issuer = resource_metadata
        .as_ref()
        .and_then(|(_, d)| d["authorization_servers"][0].as_str().map(str::to_owned))
        .unwrap_or_else(|| server.origin().ascii_serialization());
    let issuer_url = url::Url::parse(&issuer).map_err(|_| "The server names an authorization server that is not a URL.".to_string())?;
    let mut metadata = None;
    for candidate in server_metadata_urls(&issuer_url) {
        if let Some(document) = get_json(http, &candidate).await {
            if text(&document["authorization_endpoint"]).is_some() && text(&document["token_endpoint"]).is_some() {
                metadata = Some(document);
                break;
            }
        }
    }
    let origin = issuer_url.origin().ascii_serialization();
    let endpoint = |field: &str, fallback: &str| {
        metadata.as_ref().and_then(|m| text(&m[field]).map(str::to_owned)).unwrap_or_else(|| format!("{origin}{fallback}"))
    };
    let scope = challenge_scope.or_else(|| {
        let scopes: Vec<&str> = resource_metadata.as_ref()?.1["scopes_supported"].as_array()?.iter().filter_map(Value::as_str).collect();
        (!scopes.is_empty()).then(|| scopes.join(" "))
    });
    Ok(Discovered {
        resource: resource_metadata.as_ref().map(|(_, d)| text(&d["resource"]).map(str::to_owned).unwrap_or_else(|| server_url.to_string())),
        resource_metadata_url: resource_metadata.as_ref().map(|(u, _)| u.clone()),
        scope,
        issuer,
        metadata_found: metadata.is_some(),
        authorization_endpoint: endpoint("authorization_endpoint", "/authorize"),
        token_endpoint: endpoint("token_endpoint", "/token"),
        registration_endpoint: match &metadata {
            Some(m) => text(&m["registration_endpoint"]).map(str::to_owned),
            None => Some(format!("{origin}/register")),
        },
        auth_methods: metadata
            .as_ref()
            .and_then(|m| m["token_endpoint_auth_methods_supported"].as_array())
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default(),
    })
}

/// Error text of a request without its URL (it may carry a query).
fn without_url(e: &reqwest::Error) -> String {
    let e = e.to_string();
    e.split(" for url").next().unwrap_or(&e).to_string()
}

/// The `error_description` or `error` of an OAuth error answer, else the status.
async fn oauth_error(response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.json::<Value>().await.unwrap_or_default();
    let detail = text(&body["error_description"]).or_else(|| text(&body["error"])).map(|d| d.chars().take(300).collect::<String>());
    match detail {
        Some(detail) => format!("{detail} (HTTP {})", status.as_u16()),
        None => format!("HTTP {}", status.as_u16()),
    }
}

/// Dynamic client registration (RFC 7591) for this login's redirect URI, or the
/// client id from the server's config when the server has no registration.
async fn register(http: &reqwest::Client, name: &str, config: &Value, found: &Discovered, redirect_uri: &str) -> Result<(String, Option<String>), String> {
    if let Some(client_id) = text(&config["oauth"]["clientId"]) {
        return Ok((client_id.to_string(), None));
    }
    let Some(endpoint) = &found.registration_endpoint else {
        return Err("The server does not let new apps register, and no client id is configured for it.".into());
    };
    let mut metadata = json!({
        "client_name": format!("Claude Code ({name}) via Aurelius"),
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
    });
    if let Some(scope) = &found.scope {
        metadata["scope"] = json!(scope);
    }
    let response = http.post(endpoint).json(&metadata).send().await.map_err(|e| format!("Registering with the server failed: {}", without_url(&e)))?;
    if !response.status().is_success() {
        return Err(format!("The server refused the registration: {}", oauth_error(response).await));
    }
    let client: Value = response.json().await.map_err(|_| "The server's registration answer is not JSON.".to_string())?;
    let id = text(&client["client_id"]).ok_or("The server's registration answer has no client id.")?;
    Ok((id.to_string(), text(&client["client_secret"]).map(str::to_owned)))
}

/// What a pending login keeps for its last step. Never sent anywhere but the token endpoint.
struct Flow {
    /// The files of the board member's own login it saves into.
    files: ClaudeFiles,
    name: String,
    key: String,
    server_url: String,
    verifier: String,
    state: String,
    client_id: String,
    client_secret: Option<String>,
    redirect_uri: String,
    token_endpoint: String,
    basic_auth: bool,
    resource: Option<String>,
    discovery_state: Value,
    http: reqwest::Client,
}

struct Tokens {
    access: String,
    refresh: Option<String>,
    expires_in: Option<f64>,
    scope: Option<String>,
}

async fn exchange(flow: &Flow, code: &str) -> Result<Tokens, String> {
    // Built before any await: the serializer is not `Send`.
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "authorization_code")
            .append_pair("code", code)
            .append_pair("redirect_uri", &flow.redirect_uri)
            .append_pair("client_id", &flow.client_id)
            .append_pair("code_verifier", &flow.verifier);
        if let Some(resource) = &flow.resource {
            form.append_pair("resource", resource);
        }
        if let (Some(secret), false) = (&flow.client_secret, flow.basic_auth) {
            form.append_pair("client_secret", secret);
        }
        form.finish()
    };
    let mut request = flow.http.post(&flow.token_endpoint).header(reqwest::header::ACCEPT, "application/json");
    if let (Some(secret), true) = (&flow.client_secret, flow.basic_auth) {
        request = request.basic_auth(&flow.client_id, Some(secret));
    }
    let response = request
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("The token request failed: {}", without_url(&e)))?;
    if !response.status().is_success() {
        return Err(format!("The server refused the login: {}", oauth_error(response).await));
    }
    let body: Value = response.json().await.map_err(|_| "The server's token answer is not JSON.".to_string())?;
    let access = text(&body["access_token"]).ok_or("The server's token answer has no access token.")?.to_string();
    let expires_in = body["expires_in"].as_f64().or_else(|| body["expires_in"].as_str().and_then(|s| s.parse().ok()));
    Ok(Tokens { access, refresh: text(&body["refresh_token"]).map(str::to_owned), expires_in, scope: text(&body["scope"]).map(str::to_owned) })
}

/// Write the tokens where Claude Code reads them, keeping what else the entry has.
fn save(files: &ClaudeFiles, flow: &Flow, tokens: Tokens) -> Result<(), String> {
    let now = aurelius_storage::now_ms();
    files.update_entry(&flow.key, |entry| {
        let mut set = |k: &str, v: Option<Value>| match v {
            Some(v) => {
                entry.insert(k.into(), v);
            }
            None => {
                entry.remove(k);
            }
        };
        set("serverName", Some(json!(flow.name)));
        set("serverUrl", Some(json!(flow.server_url)));
        set("accessToken", Some(json!(tokens.access)));
        set("refreshToken", tokens.refresh.map(Value::from));
        set("expiresAt", tokens.expires_in.map(|s| json!(now + (s * 1000.0) as i64)));
        set("scope", tokens.scope.map(Value::from));
        set("clientId", Some(json!(flow.client_id)));
        set("clientSecret", flow.client_secret.clone().map(Value::from));
        set("redirectUri", Some(json!(flow.redirect_uri)));
        set("discoveryState", Some(flow.discovery_state.clone()));
    })?;
    files.forget_needs_auth(&flow.name);
    Ok(())
}

// ---------------------------------------------------------------------------
// Logins in progress
// ---------------------------------------------------------------------------

struct Login {
    view: McpLogin,
    /// The board member who started it: only they may see or complete it (SEC2 m6).
    user: Option<aurelius_domain::UserId>,
    /// Taken by whoever completes it (the callback or `mcp.login_complete`).
    flow: Option<Flow>,
    listener: Option<tokio::task::AbortHandle>,
    finished_at_ms: Option<i64>,
}

static LOGINS: LazyLock<Mutex<HashMap<String, Login>>> = LazyLock::new(Default::default);

fn logins() -> std::sync::MutexGuard<'static, HashMap<String, Login>> {
    LOGINS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Mark a login ended; answers its view when it was still pending.
fn end(login_id: &str, state: McpLoginState, error: Option<String>) -> Option<McpLogin> {
    let mut all = logins();
    let login = all.get_mut(login_id)?;
    if login.view.state != McpLoginState::Pending {
        return None;
    }
    login.view.state = state;
    login.view.error = error;
    login.flow = None;
    login.finished_at_ms = Some(aurelius_storage::now_ms());
    Some(login.view.clone())
}

async fn announce(shared: &Arc<Shared>, kind: EventKind, aggregate_id: String, metadata: Value) {
    let s = shared.clone();
    match tokio::task::spawn_blocking(move || s.service.record_host_event(kind, aggregate_id, metadata)).await {
        Ok(Ok(notice)) => shared.publish([notice]),
        Ok(Err(e)) => tracing::warn!("mcp login event not recorded: {e}"),
        Err(e) => tracing::warn!("mcp login event not recorded: {e}"),
    }
}

async fn announce_finished(shared: &Arc<Shared>, view: &McpLogin) {
    let state = serde_json::to_value(view.state).unwrap_or_default();
    announce(shared, EventKind::McpLoginFinished, view.login_id.clone(), json!({ "server": view.name, "state": state })).await;
    if view.state == McpLoginState::Succeeded {
        announce(shared, EventKind::McpAuthChanged, view.name.clone(), json!({ "server": view.name, "auth": "authorized" })).await;
    }
}

fn find_server(name: &str) -> Result<crate::mcp_servers::Found, ProtocolError> {
    crate::mcp_servers::discover().into_iter().find(|f| f.name == name).ok_or_else(|| invalid(format!("There is no user-scope server named '{name}'.")))
}

fn oauth_server(shared: &Shared, name: &str, user: Option<aurelius_domain::UserId>) -> Result<(crate::mcp_servers::Found, ClaudeFiles), ProtocolError> {
    let found = find_server(name)?;
    if crate::mcp_servers::transport_of(&found.config) == McpTransport::Stdio {
        return Err(invalid("Only http and sse servers log in with OAuth."));
    }
    if fixed_auth(&found.config) {
        return Err(invalid("This server uses a fixed Authorization header; it has no login."));
    }
    let files = ClaudeFiles::locate(&shared.root, user).ok_or_else(|| invalid("Only a board member logs in to an MCP server, on their own Claude login."))?;
    Ok((found, files))
}

/// `mcp.login_start`: discover, register, and wait for the browser.
/// `user`: the board member whose own Claude login keeps the tokens.
pub async fn start(shared: &Arc<Shared>, name: String, user: Option<aurelius_domain::UserId>) -> Result<McpLogin, ProtocolError> {
    if cfg!(target_os = "macos") {
        return Err(invalid("On macOS Claude Code keeps MCP logins in the Keychain; log in with `claude /mcp` for now."));
    }
    let (found, files) = oauth_server(shared, &name, user)?;
    let key = store_key(&name, &found.config);
    let previous = files.credentials()["mcpOAuth"][&key]["discoveryState"].clone();

    // The listener first: its port is part of the redirect URI the client registers.
    let v4 = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| internal(format!("No local port for the login: {e}")))?;
    let port = v4.local_addr().map_err(internal)?.port();
    let v6 = std::net::TcpListener::bind(("::1", port)).ok();
    let redirect_uri = format!("http://localhost:{port}/callback");

    let http = reqwest::Client::builder().timeout(HTTP_TIMEOUT).build().map_err(internal)?;
    let hint = text(&previous["resourceMetadataUrl"]).map(str::to_owned);
    let found_auth = discover(&http, &found.config, hint).await.map_err(invalid)?;
    let (client_id, client_secret) = register(&http, &name, &found.config, &found_auth, &redirect_uri).await.map_err(invalid)?;

    let (verifier, state) = (random_text(48), random_text(24));
    let mut url = url::Url::parse(&found_auth.authorization_endpoint).map_err(|_| invalid("The server's authorization address is not a URL."))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", &client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("code_challenge", &code_challenge(&verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state);
        if let Some(scope) = &found_auth.scope {
            q.append_pair("scope", scope);
        }
        if let Some(resource) = &found_auth.resource {
            q.append_pair("resource", resource);
        }
    }

    let now = aurelius_storage::now_ms();
    let login_id = random_text(16);
    let view = McpLogin {
        login_id: login_id.clone(),
        name: name.clone(),
        state: McpLoginState::Pending,
        authorization_url: url.to_string(),
        redirect_uri: redirect_uri.clone(),
        expires_at_ms: now + LOGIN_TTL_MS,
        error: None,
    };
    let mut discovery_state = json!({
        "authorizationServerUrl": found_auth.issuer,
        "oauthMetadataFound": found_auth.metadata_found,
        "authServerMetadataUrl": null,
    });
    if let Some(u) = &found_auth.resource_metadata_url {
        discovery_state["resourceMetadataUrl"] = json!(u);
    }
    let basic_auth = found_auth.auth_methods.iter().any(|m| m == "client_secret_basic") && !found_auth.auth_methods.iter().any(|m| m == "client_secret_post");
    let flow = Flow {
        files: files.clone(),
        name: name.clone(),
        key,
        server_url: found.config["url"].as_str().unwrap_or("").to_string(),
        verifier,
        state,
        client_id,
        client_secret,
        redirect_uri,
        token_endpoint: found_auth.token_endpoint,
        basic_auth,
        resource: found_auth.resource,
        discovery_state,
        http,
    };

    let listener = tokio::spawn(serve(Arc::downgrade(shared), login_id.clone(), v4, v6, view.expires_at_ms));
    {
        let mut all = logins();
        all.retain(|_, l| l.finished_at_ms.is_none_or(|at| now - at < KEEP_FINISHED_MS));
        // One login per server and board member: a new one replaces their one still waiting.
        for old in all.values_mut().filter(|l| l.view.name == name && l.user == user && l.view.state == McpLoginState::Pending) {
            old.view.state = McpLoginState::Expired;
            old.view.error = Some("A newer login replaced this one.".into());
            old.flow = None;
            old.finished_at_ms = Some(now);
            if let Some(l) = old.listener.take() {
                l.abort();
            }
        }
        all.insert(login_id, Login { view: view.clone(), user, flow: Some(flow), listener: Some(listener.abort_handle()), finished_at_ms: None });
    }
    Ok(view)
}

/// The last step, from the callback or from `mcp.login_complete`. A wrong or
/// foreign address leaves the login waiting; a refusal or failed exchange ends it.
async fn complete(shared: &Arc<Shared>, login_id: &str, redirect: &str) -> Result<McpLogin, ProtocolError> {
    let params = redirect_params(redirect);
    let flow = {
        let mut all = logins();
        let login = all.get_mut(login_id).ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, "This login is unknown or too old; start a new one."))?;
        if login.view.state != McpLoginState::Pending {
            return Err(invalid(format!("This login has already ended ({}).", serde_json::to_value(login.view.state).unwrap_or_default().as_str().unwrap_or(""))));
        }
        let Some(flow) = login.flow.as_ref() else { return Err(invalid("This login is being completed already.")) };
        if params.get("state").map(String::as_str) != Some(flow.state.as_str()) {
            return Err(invalid("This address does not belong to this login (its state does not match). Copy the full address the browser ended on."));
        }
        if params.get("error").is_none() && params.get("code").is_none_or(|c| c.is_empty()) {
            return Err(invalid("The address has no code in it. Copy the full address the browser ended on."));
        }
        login.flow.take().expect("checked")
    };
    if let Some(error) = params.get("error") {
        let reason = params.get("error_description").unwrap_or(error).chars().take(300).collect::<String>();
        let message = format!("The server refused the login: {reason}");
        if let Some(view) = end(login_id, McpLoginState::Failed, Some(message.clone())) {
            announce_finished(shared, &view).await;
        }
        return Err(invalid(message));
    }
    let code = params.get("code").cloned().unwrap_or_default();
    let outcome = match exchange(&flow, &code).await {
        Ok(tokens) => {
            tokio::task::spawn_blocking(move || save(&flow.files.clone(), &flow, tokens)).await.unwrap_or_else(|e| Err(e.to_string()))
        }
        Err(e) => Err(e),
    };
    let (state, error) = match &outcome {
        Ok(()) => (McpLoginState::Succeeded, None),
        Err(e) => (McpLoginState::Failed, Some(e.clone())),
    };
    let view = end(login_id, state, error);
    if let Some(view) = &view {
        announce_finished(shared, view).await;
    }
    match (outcome, view) {
        (Ok(()), Some(view)) => Ok(view),
        (Ok(()), None) => Err(invalid("This login has already ended.")),
        (Err(e), _) => Err(invalid(e)),
    }
}

/// `mcp.login_complete`: the address a browser elsewhere ended on, from the
/// board member who started the login (anyone else: `NotFound`, SEC2 m6).
pub async fn complete_pasted(shared: &Arc<Shared>, login_id: String, redirect_url: String, user: Option<aurelius_domain::UserId>) -> Result<McpLogin, ProtocolError> {
    get(&login_id, user)?;
    let result = complete(shared, &login_id, &redirect_url).await;
    let listener = logins().get_mut(&login_id).filter(|l| l.view.state != McpLoginState::Pending).and_then(|l| l.listener.take());
    if let Some(listener) = listener {
        listener.abort();
    }
    result
}

/// `mcp.login_get`: only the login `user` started; another's reads as unknown.
pub fn get(login_id: &str, user: Option<aurelius_domain::UserId>) -> Result<McpLogin, ProtocolError> {
    logins().get(login_id).filter(|l| l.user == user).map(|l| l.view.clone()).ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, "This login is unknown or too old."))
}

/// `mcp.logout`: Claude Code's tokens of the server are removed; its client
/// registration stays so a next login is quicker. Runs ask for a login again.
pub async fn logout(shared: &Arc<Shared>, name: String, user: Option<aurelius_domain::UserId>) -> Result<McpServers, ProtocolError> {
    let (found, files) = oauth_server(shared, &name, user)?;
    let key = store_key(&name, &found.config);
    let had = files.credentials()["mcpOAuth"].get(&key).is_some();
    if had {
        let files2 = files.clone();
        tokio::task::spawn_blocking(move || {
            files2.update_entry(&key, |entry| {
                for field in ["refreshToken", "expiresAt", "scope"] {
                    entry.remove(field);
                }
                entry.insert("accessToken".into(), json!(""));
                let discovery = entry.entry("discoveryState").or_insert_with(|| json!({}));
                if let Some(d) = discovery.as_object_mut() {
                    d.insert("oauthMetadataFound".into(), json!(true));
                }
            })
        })
        .await
        .map_err(internal)?
        .map_err(internal)?;
        announce(shared, EventKind::McpAuthChanged, name.clone(), json!({ "server": name, "auth": "needs_login" })).await;
    }
    crate::mcp_servers::list(shared, user).await
}

// ---------------------------------------------------------------------------
// The browser's way back
// ---------------------------------------------------------------------------

async fn accept(v4: &TcpListener, v6: Option<&TcpListener>) -> std::io::Result<TcpStream> {
    match v6 {
        Some(v6) => tokio::select! {
            r = v4.accept() => r.map(|(s, _)| s),
            r = v6.accept() => r.map(|(s, _)| s),
        },
        None => v4.accept().await.map(|(s, _)| s),
    }
}

/// The request target of a `GET <target> HTTP/1.1` request.
async fn request_target(stream: &mut TcpStream) -> Option<String> {
    let mut buffer = Vec::with_capacity(2048);
    let mut chunk = [0u8; 2048];
    let read = async {
        while !buffer.windows(4).any(|w| w == b"\r\n\r\n") && buffer.len() < 16 * 1024 {
            let n = stream.read(&mut chunk).await.ok()?;
            if n == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..n]);
        }
        Some(())
    };
    tokio::time::timeout(Duration::from_secs(5), read).await.ok()??;
    let head = String::from_utf8_lossy(&buffer);
    let mut parts = head.lines().next()?.split_whitespace();
    (parts.next()? == "GET").then(|| parts.next().map(str::to_owned))?
}

async fn respond(mut stream: TcpStream, status: &str, title: &str, body: &str) {
    let page = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title></head>\
         <body style=\"font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto\"><h2>{title}</h2><p>{body}</p></body></html>"
    );
    let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Waits for the browser on the login's port until it completes or expires.
async fn serve(shared: Weak<Shared>, login_id: String, v4: std::net::TcpListener, v6: Option<std::net::TcpListener>, expires_at_ms: i64) {
    let listen = |l: std::net::TcpListener| l.set_nonblocking(true).ok().and_then(|_| TcpListener::from_std(l).ok());
    let wait = Duration::from_millis((expires_at_ms - aurelius_storage::now_ms()).max(0) as u64);
    let deadline = tokio::time::Instant::now() + wait;
    if let Some(v4) = listen(v4) {
        let v6 = v6.and_then(listen);
        while let Ok(Ok(mut stream)) = tokio::time::timeout_at(deadline, accept(&v4, v6.as_ref())).await {
            let Some(target) = request_target(&mut stream).await else { continue };
            if !target.starts_with("/callback") {
                respond(stream, "404 Not Found", "Not found", "").await;
                continue;
            }
            let Some(shared) = shared.upgrade() else { return };
            match complete(&shared, &login_id, &target).await {
                Ok(view) => {
                    let body = format!("Aurelius is logged in to <b>{}</b>. You can close this window.", html_escape(&view.name));
                    respond(stream, "200 OK", "Logged in", &body).await;
                    return;
                }
                Err(e) => {
                    let still_waiting = logins().get(&login_id).is_some_and(|l| l.view.state == McpLoginState::Pending);
                    respond(stream, "400 Bad Request", "Login not completed", &html_escape(&e.message)).await;
                    if !still_waiting {
                        return;
                    }
                }
            }
        }
    } else {
        tokio::time::sleep_until(deadline).await;
    }
    if let Some(view) = end(&login_id, McpLoginState::Expired, Some("The login was not finished in time.".into())) {
        if let Some(shared) = shared.upgrade() {
            announce_finished(&shared, &view).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sentinel F1/D6: an MCP login changes only its `mcpOAuth` entry, in the
    /// board member's own login folder, and leaves the file readable by its owner only.
    #[cfg(unix)]
    #[test]
    fn an_mcp_login_writes_only_its_entry_with_mode_0600_in_the_board_members_folder() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let (alex, robin) = (aurelius_domain::UserId::new(), aurelius_domain::UserId::new());
        let login = aurelius_runtime::claude_code::ClaudeLogin::of(root.path(), alex).unwrap();
        let files = ClaudeFiles::in_dir(login.config_dir());
        // What `claude` wrote at its own sign-in: the Claude login, 0600.
        let claude_login = r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-CANARY","refreshToken":"sk-ant-ort01-CANARY","expiresAt":1,"scopes":["user:inference"]}}"#;
        std::fs::write(&files.credentials, claude_login).unwrap();
        std::fs::set_permissions(&files.credentials, std::fs::Permissions::from_mode(0o600)).unwrap();
        files.update_entry("notes|0123456789abcdef", |entry| {
            entry.insert("accessToken".into(), json!("mcp-token"));
        })
        .unwrap();
        assert_eq!(std::fs::metadata(&files.credentials).unwrap().permissions().mode() & 0o777, 0o600);
        let after = files.credentials();
        assert_eq!(after["claudeAiOauth"], serde_json::from_str::<Value>(claude_login).unwrap()["claudeAiOauth"], "the Claude login is untouched");
        assert_eq!(after["mcpOAuth"]["notes|0123456789abcdef"]["accessToken"], "mcp-token");
        assert!(!files.credentials.with_extension("json.aurelius-tmp").exists());

        // A file made new by an MCP login (no sign-in yet) is 0600 too, whatever the umask.
        let other = ClaudeFiles::in_dir(aurelius_runtime::claude_code::ClaudeLogin::of(root.path(), robin).unwrap().config_dir());
        other.update_entry("notes|0123456789abcdef", |_| {}).unwrap();
        assert_eq!(std::fs::metadata(&other.credentials).unwrap().permissions().mode() & 0o777, 0o600);

        // Without the test override, a board member's files are those of their own login
        // folder, and without a board member there are none (never ~/.claude).
        if std::env::var_os("AURELIUS_CLAUDE_CREDENTIALS").is_none() {
            assert_eq!(ClaudeFiles::locate(root.path(), Some(alex)).unwrap().credentials, files.credentials);
            assert!(ClaudeFiles::locate(root.path(), None).is_none());
        }
    }

    #[test]
    fn a_login_is_seen_and_completed_only_by_who_started_it() {
        // SEC2 m6: another board member who knows the id finds nothing.
        let (alex, robin) = (aurelius_domain::UserId::new(), aurelius_domain::UserId::new());
        let id = random_text(16);
        let view = McpLogin {
            login_id: id.clone(),
            name: "recipes".into(),
            state: McpLoginState::Pending,
            authorization_url: "https://mcp.example.com/authorize".into(),
            redirect_uri: "http://localhost:1/callback".into(),
            expires_at_ms: aurelius_storage::now_ms() + LOGIN_TTL_MS,
            error: None,
        };
        logins().insert(id.clone(), Login { view, user: Some(alex), flow: None, listener: None, finished_at_ms: None });
        assert_eq!(get(&id, Some(alex)).unwrap().login_id, id);
        for other in [Some(robin), None] {
            assert_eq!(get(&id, other).err().map(|e| e.code), Some(ErrorCode::NotFound));
        }
        logins().remove(&id);
    }

    #[test]
    fn the_key_is_claude_codes() {
        // Vectors from `JSON.stringify` + sha256 in Node, as Claude Code computes them.
        let http = json!({ "type": "http", "url": "https://mcp.example.com/mcp" });
        assert_eq!(store_key("recipes", &http), "recipes|4282f2a9112820d2");
        let sse = json!({ "type": "sse", "url": "https://a.example/sse", "headers": { "X-Team": "t1", "Accept": "x" } });
        assert_eq!(store_key("s", &sse), "s|b4ce53628023e939");
        let local = json!({ "type": "http", "url": "http://127.0.0.1:63070/mcp" });
        assert_eq!(store_key("fake-recipes", &local), "fake-recipes|fcef45929a9ab2de", "seen in Claude Code's own file");
    }

    #[test]
    fn pkce_challenge_is_s256() {
        assert_eq!(code_challenge("dBjftJeZ4CVP-mJ92K9qkPrAbCoq4FrEi3wGMGd5ybA"), "iWv4ivj60XZXnnXBHRmC2VQBbnLF2tYaARFUjpPVqRA");
        let v = random_text(48);
        assert!(v.len() >= 43 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn status_from_claude_codes_files() {
        let http = json!({ "type": "http", "url": "https://mcp.example.com/mcp" });
        let key = store_key("r", &http);
        let none = json!({});
        let now = 1_000_000;
        assert_eq!(status_of("s", &json!({ "command": "npx" }), &none, &none, now), McpAuthStatus::None);
        let fixed = json!({ "type": "http", "url": "https://x", "headers": { "authorization": "Bearer x" } });
        assert_eq!(status_of("f", &fixed, &none, &none, now), McpAuthStatus::None);
        assert_eq!(status_of("r", &http, &none, &none, now), McpAuthStatus::Unknown);
        assert_eq!(status_of("r", &http, &none, &json!({ "r": { "timestamp": 1 } }), now), McpAuthStatus::NeedsLogin);
        let stub = json!({ "mcpOAuth": { key.clone(): { "accessToken": "", "discoveryState": { "oauthMetadataFound": true } } } });
        assert_eq!(status_of("r", &http, &stub, &none, now), McpAuthStatus::NeedsLogin);
        let fresh = json!({ "mcpOAuth": { key.clone(): { "accessToken": "a", "expiresAt": now + 10 } } });
        assert_eq!(status_of("r", &http, &fresh, &none, now), McpAuthStatus::Authorized);
        let expired = json!({ "mcpOAuth": { key.clone(): { "accessToken": "a", "expiresAt": now - 10 } } });
        assert_eq!(status_of("r", &http, &expired, &none, now), McpAuthStatus::NeedsLogin);
        let refreshable = json!({ "mcpOAuth": { key: { "accessToken": "a", "refreshToken": "r", "expiresAt": now - 10 } } });
        assert_eq!(status_of("r", &http, &refreshable, &none, now), McpAuthStatus::Authorized);
    }

    #[test]
    fn www_authenticate_params() {
        let h = r#"Bearer error="invalid_token", resource_metadata="https://a.example/.well-known/oauth-protected-resource/mcp", scope="read write""#;
        assert_eq!(auth_param(h, "resource_metadata").as_deref(), Some("https://a.example/.well-known/oauth-protected-resource/mcp"));
        assert_eq!(auth_param(h, "scope").as_deref(), Some("read write"));
        assert_eq!(auth_param("Bearer realm=x, scope=one", "scope").as_deref(), Some("one"));
        assert_eq!(auth_param("Bearer xresource_metadata=\"no\"", "resource_metadata"), None);
    }

    #[test]
    fn metadata_locations() {
        let server = url::Url::parse("https://p.example.co/functions/v1/mcp").unwrap();
        assert_eq!(
            resource_metadata_urls(&server),
            vec!["https://p.example.co/.well-known/oauth-protected-resource/functions/v1/mcp", "https://p.example.co/.well-known/oauth-protected-resource"]
        );
        let issuer = url::Url::parse("https://p.example.co/auth/v1").unwrap();
        assert_eq!(server_metadata_urls(&issuer)[0], "https://p.example.co/.well-known/oauth-authorization-server/auth/v1");
        assert_eq!(server_metadata_urls(&issuer)[2], "https://p.example.co/auth/v1/.well-known/openid-configuration");
        assert_eq!(server_metadata_urls(&url::Url::parse("https://a.example").unwrap()).len(), 2);
    }

    #[test]
    fn pasted_addresses() {
        for pasted in ["http://localhost:4321/callback?code=abc&state=s%2B1", "/callback?code=abc&state=s%2B1", "  code=abc&state=s%2B1#x "] {
            let p = redirect_params(pasted);
            assert_eq!((p.get("code").map(String::as_str), p.get("state").map(String::as_str)), (Some("abc"), Some("s+1")), "{pasted}");
        }
        assert!(redirect_params("http://localhost:4321/callback").is_empty());
    }

    #[test]
    fn files_are_changed_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let files = ClaudeFiles { credentials: dir.path().join(".credentials.json"), needs_auth_cache: dir.path().join("mcp-needs-auth-cache.json") };
        std::fs::write(&files.credentials, r#"{"claudeAiOauth":{"accessToken":"keep"},"mcpOAuth":{"x|1":{"clientId":"c"}}}"#).unwrap();
        std::fs::write(&files.needs_auth_cache, r#"{"r":{"timestamp":1},"o":{"timestamp":2}}"#).unwrap();
        files.update_entry("r|2", |e| {
            e.insert("accessToken".into(), json!("t"));
        }).unwrap();
        files.forget_needs_auth("r");
        let creds = files.credentials();
        assert_eq!((creds["claudeAiOauth"]["accessToken"].as_str(), creds["mcpOAuth"]["x|1"]["clientId"].as_str()), (Some("keep"), Some("c")));
        assert_eq!(creds["mcpOAuth"]["r|2"]["accessToken"], "t");
        assert_eq!(files.needs_auth(), json!({ "o": { "timestamp": 2 } }));
        // A broken file is left alone.
        std::fs::write(&files.credentials, "{not json").unwrap();
        assert!(files.update_entry("r|2", |_| {}).is_err());
        assert_eq!(std::fs::read_to_string(&files.credentials).unwrap(), "{not json");
    }
}
