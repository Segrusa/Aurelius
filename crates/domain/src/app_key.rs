//! App keys (feature `app_keys`): what an outside app or agent may do when it
//! talks to Aurelius through `aurelius-mcp` (MCP over stdio or over the remote
//! gateway). Each app has its own key; the server keeps only the SHA-256 of its
//! secret. This module is the pure part: scopes, validation, the limits.
//!
//! * `read`: projects, tasks, employees, the inbox and memory.
//! * `draft`: tasks as drafts (no employee), ideas, memories.
//! * `run`: tasks that start at once, messages to tasks, stopping tasks,
//!   talking to the project assistant. It includes `draft`.
//! * `approve`: Auto mode and answering approvals. Without it a task never
//!   runs in Auto, not even when its employee's own mode is Auto.
//!
//! Besides scopes a key has an audience (feature `management_channel`): `all`
//! (the default; today's behaviour) or `management`, which reaches only the CEO
//! and the CEO's direct reports (`Org::management`); the server enforces it.
//! A key may also have a webhook (feature `webhooks`, `crate::webhook`).

use crate::{AppKeyId, DomainError, require_text};
use serde::{Deserialize, Serialize};

pub const NAME_MAX: usize = 80;
/// An app key's secret starts with this (agent-function keys use `ak_`).
pub const KEY_PREFIX: &str = "amk_";
pub const HOUR_MS: i64 = 3_600_000;
/// Highest limit a key may have (unset: unlimited).
pub const LIMIT_MAX: u32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AppScope {
    Read,
    Draft,
    Run,
    Approve,
}

impl AppScope {
    pub const ALL: [AppScope; 4] = [AppScope::Read, AppScope::Draft, AppScope::Run, AppScope::Approve];

    pub fn as_str(self) -> &'static str {
        match self {
            AppScope::Read => "read",
            AppScope::Draft => "draft",
            AppScope::Run => "run",
            AppScope::Approve => "approve",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        AppScope::ALL.into_iter().find(|s| s.as_str() == text)
    }
}

/// Whom an app key may reach (feature `management_channel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AppAudience {
    /// Everything its scopes allow: today's behaviour, and the default.
    #[default]
    All,
    /// Only the management: the CEO and the CEO's direct reports. Employees below
    /// them cannot be listed, messaged or given tasks, and the project assistant is out of reach.
    Management,
}

impl AppAudience {
    pub fn as_str(self) -> &'static str {
        match self {
            AppAudience::All => "all",
            AppAudience::Management => "management",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        [AppAudience::All, AppAudience::Management].into_iter().find(|a| a.as_str() == text)
    }
}

/// Why a management app is refused a colleague below the management.
pub const NOT_MANAGEMENT: &str = "This app key reaches the management only (the CEO and the CEO's direct reports). Ask one of them to pass the work on.";
/// Why a management app is refused the project assistant.
pub const NO_PROJECT_ASSISTANT: &str = "This app key reaches the management only; it cannot talk to a project's assistant. Use ask_management.";

/// Whether `held` grants `needed`. `run` includes `draft`: an app that may
/// start tasks may also leave one as a draft.
pub fn grants(held: &[AppScope], needed: AppScope) -> bool {
    held.contains(&needed) || (needed == AppScope::Draft && held.contains(&AppScope::Run))
}

/// `app_key.create`. The secret is in the answer once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateAppKey {
    /// The app's name; tasks it creates show it (`TaskView::created_by_app`).
    pub name: String,
    pub scopes: Vec<AppScope>,
    /// Tasks it may create in any rolling hour; unset: unlimited.
    #[serde(default)]
    pub limit_per_hour: Option<u32>,
    /// Tasks of this app that may be open at once (queued, running, waiting or
    /// in review; drafts do not count); unset: unlimited.
    #[serde(default)]
    pub limit_concurrent: Option<u32>,
    /// Whom the app may reach; default `all` (feature `management_channel`).
    #[serde(default)]
    pub audience: AppAudience,
    /// Where Aurelius posts this app's events (`http://` or `https://`); unset: no
    /// webhook. A secret for the signature is made with the key and shown once
    /// (`AppKeyCreated::webhook_secret`). Feature `webhooks`.
    #[serde(default)]
    pub webhook_url: Option<String>,
}

/// Longest webhook URL.
pub const WEBHOOK_URL_MAX: usize = 2_000;

/// A webhook URL as stored: trimmed, `http` or `https` only, with a host, an
/// optional port and path, no credentials and no fragment. Plain `http` is
/// allowed (a server on your LAN or tailnet).
pub fn validate_webhook_url(url: &str) -> Result<String, DomainError> {
    let bad = |detail: &str| DomainError::OutOfRange { field: "webhook url".into(), detail: detail.into() };
    let url = url.trim();
    if url.len() > WEBHOOK_URL_MAX || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad("no spaces, at most 2000 characters"));
    }
    let lower = url.to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &url["https://".len()..]
    } else if lower.starts_with("http://") {
        &url["http://".len()..]
    } else {
        return Err(bad("must start with http:// or https://"));
    };
    if rest.contains('#') {
        return Err(bad("no #fragment"));
    }
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    if authority.contains('@') {
        return Err(bad("no user name or password in the URL"));
    }
    let (host, port) = match authority.rfind(':') {
        Some(i) if !authority.ends_with(']') => (&authority[..i], Some(&authority[i + 1..])),
        _ => (authority, None),
    };
    if host.is_empty() || host == "[]" {
        return Err(bad("needs a host name"));
    }
    if !host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '[' | ']' | ':')) {
        return Err(bad("the host has characters a host name cannot have"));
    }
    if let Some(port) = port {
        if port.parse::<u16>().ok().filter(|p| *p > 0).is_none() {
            return Err(bad("the port must be 1 to 65535"));
        }
    }
    Ok(url.to_owned())
}

/// `cmd` after validation: name trimmed, scopes sorted without doubles.
pub fn validate(cmd: &CreateAppKey) -> Result<CreateAppKey, DomainError> {
    let name = require_text("name", &cmd.name, NAME_MAX)?;
    let mut scopes = cmd.scopes.clone();
    scopes.sort();
    scopes.dedup();
    if scopes.is_empty() {
        return Err(DomainError::OutOfRange { field: "scopes".into(), detail: "at least one of read, draft, run, approve".into() });
    }
    for (field, limit) in [("limit per hour", cmd.limit_per_hour), ("limit at once", cmd.limit_concurrent)] {
        if limit.is_some_and(|l| l == 0 || l > LIMIT_MAX) {
            return Err(DomainError::OutOfRange { field: field.into(), detail: format!("1 to {LIMIT_MAX} (empty: unlimited; to stop the app, revoke its key)") });
        }
    }
    if cmd.audience == AppAudience::Management && scopes.contains(&AppScope::Approve) {
        return Err(DomainError::OutOfRange { field: "scopes".into(), detail: "a management app cannot have approve; the user answers approvals in Aurelius".into() });
    }
    let webhook_url = match cmd.webhook_url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        Some(url) => Some(validate_webhook_url(url)?),
        None => None,
    };
    Ok(CreateAppKey { name, scopes, limit_per_hour: cmd.limit_per_hour, limit_concurrent: cmd.limit_concurrent, audience: cmd.audience, webhook_url })
}

/// An app key; the secret is never part of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AppKeyView {
    pub id: AppKeyId,
    pub name: String,
    pub scopes: Vec<AppScope>,
    pub limit_per_hour: Option<u32>,
    pub limit_concurrent: Option<u32>,
    pub created_at_ms: i64,
    pub last_used_ms: Option<i64>,
    pub revoked_at_ms: Option<i64>,
    /// Tasks it created in the last hour (rolling).
    pub tasks_last_hour: u32,
    /// Its tasks that are open now (queued, running, waiting, in review).
    pub tasks_open: u32,
    /// Whom it may reach (feature `management_channel`); `all` on older keys.
    #[serde(default)]
    pub audience: AppAudience,
    /// Where its events are posted (feature `webhooks`); never the secret.
    #[serde(default)]
    pub webhook_url: Option<String>,
}

/// The answer to `app_key.create`: the secret (`amk_...`) is shown once and not kept
/// (only its hash is). The webhook secret (`whsec_...`) is shown once as well, but
/// Aurelius has to keep it to sign events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AppKeyCreated {
    pub key: AppKeyView,
    pub secret: String,
    /// Only when the key has a webhook (feature `webhooks`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook_secret: Option<String>,
}

/// Who a session speaks for after it authenticated with an app key (in `HelloResult::app`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AppSession {
    pub key_id: AppKeyId,
    pub name: String,
    pub scopes: Vec<AppScope>,
    /// Whom this session may reach (feature `management_channel`).
    #[serde(default)]
    pub audience: AppAudience,
}

/// Why a new task of an app is refused for its limits, if it is. `last_hour`
/// counts the app's tasks made in the last hour, `open` its open ones.
pub fn over_limit(limit_per_hour: Option<u32>, limit_concurrent: Option<u32>, last_hour: u32, open: u32, starts: bool) -> Option<String> {
    if let Some(limit) = limit_per_hour.filter(|l| last_hour >= *l) {
        return Some(format!("This app may create {limit} tasks per hour and has reached that. Try again later."));
    }
    if let Some(limit) = limit_concurrent.filter(|l| starts && open >= *l) {
        return Some(format!("This app may have {limit} open tasks at once. Wait until one ends, or stop one."));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(scopes: Vec<AppScope>) -> CreateAppKey {
        CreateAppKey { name: "  Assistant ".into(), scopes, limit_per_hour: Some(10), limit_concurrent: None, audience: AppAudience::All, webhook_url: None }
    }

    #[test]
    fn audience_defaults_to_all_and_management_cannot_approve() {
        let old: CreateAppKey = serde_json::from_value(serde_json::json!({ "name": "A", "scopes": ["read"] })).unwrap();
        assert_eq!((old.audience, old.webhook_url.clone()), (AppAudience::All, None));
        assert_eq!(serde_json::to_value(AppAudience::Management).unwrap(), "management");
        let management = CreateAppKey { audience: AppAudience::Management, ..key(vec![AppScope::Read, AppScope::Run]) };
        assert_eq!(validate(&management).unwrap().audience, AppAudience::Management);
        assert!(validate(&CreateAppKey { scopes: vec![AppScope::Approve], ..management }).is_err());
        assert!(validate(&key(vec![AppScope::Approve])).is_ok(), "approve stays fine for the other audience");
    }

    #[test]
    fn webhook_urls_are_http_or_https_only() {
        for ok in ["https://marcus.example.com/hooks", "http://home-server:8787", " http://127.0.0.1:9/x?y=1 ", "HTTPS://Host", "http://[::1]:8080/hook"] {
            assert!(validate_webhook_url(ok).is_ok(), "{ok}");
        }
        for bad in ["ftp://host/x", "javascript:alert(1)", "file:///etc/passwd", "http://", "http://user:pw@host/x", "http://host:0/x", "http://host:99999/x", "https://host/x#frag", "http://ho st/", "//host/x", "host:80/x", ""] {
            assert!(validate_webhook_url(bad).is_err(), "{bad}");
        }
        let empty = validate(&CreateAppKey { webhook_url: Some("  ".into()), ..key(vec![AppScope::Read]) }).unwrap();
        assert_eq!(empty.webhook_url, None, "an empty URL means no webhook");
        let kept = validate(&CreateAppKey { webhook_url: Some(" http://home-server:8787/h ".into()), ..key(vec![AppScope::Read]) }).unwrap();
        assert_eq!(kept.webhook_url.as_deref(), Some("http://home-server:8787/h"));
    }

    #[test]
    fn run_includes_draft_and_nothing_else() {
        assert!(grants(&[AppScope::Run], AppScope::Draft));
        assert!(!grants(&[AppScope::Run], AppScope::Approve));
        assert!(!grants(&[AppScope::Draft], AppScope::Run));
        assert!(!grants(&[AppScope::Approve], AppScope::Read));
        assert!(grants(&[AppScope::Read, AppScope::Approve], AppScope::Approve));
    }

    #[test]
    fn keys_are_validated() {
        let valid = validate(&key(vec![AppScope::Run, AppScope::Read, AppScope::Run])).unwrap();
        assert_eq!(valid.name, "Assistant");
        assert_eq!(valid.scopes, vec![AppScope::Read, AppScope::Run]);
        assert!(validate(&key(vec![])).is_err(), "a key without scopes can do nothing");
        assert!(validate(&CreateAppKey { limit_per_hour: Some(0), ..key(vec![AppScope::Read]) }).is_err());
        assert!(validate(&CreateAppKey { limit_concurrent: Some(LIMIT_MAX + 1), ..key(vec![AppScope::Read]) }).is_err());
        assert!(validate(&CreateAppKey { name: " ".into(), ..key(vec![AppScope::Read]) }).is_err());
        assert_eq!(serde_json::to_value(AppScope::Approve).unwrap(), "approve");
        assert_eq!(AppScope::parse("draft"), Some(AppScope::Draft));
    }

    #[test]
    fn limits_count_per_hour_and_open_tasks() {
        assert!(over_limit(Some(2), None, 1, 9, true).is_none());
        assert!(over_limit(Some(2), None, 2, 0, true).unwrap().contains("per hour"));
        assert!(over_limit(None, Some(1), 0, 1, true).unwrap().contains("at once"));
        assert!(over_limit(None, Some(1), 0, 1, false).is_none(), "a draft does not count against open tasks");
        assert!(over_limit(None, None, 1000, 1000, true).is_none());
    }
}
