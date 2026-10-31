//! The vault (feature `vault`): items of the company or of a project, of the
//! type text, url or secret. Every value is stored encrypted; a secret's value
//! is never in a view, an event, a receipt or a log. Only `vault.reveal` gives
//! it, to a board member with a session, and that is written to the audit log.

use crate::{EmployeeId, ProjectId, RunId, TaskId, TicketId, UserId, VaultItemId};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A secret shorter than this is refused: redaction could not find it reliably.
pub const MIN_SECRET_CHARS: usize = 8;

/// What an item holds. Text and url may change into each other; a secret stays
/// a secret, and nothing becomes one (`vault_kind_locked`, B7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VaultKind {
    Text,
    /// `https://` or `http://`.
    Url,
    /// Masked everywhere; reaches runs only as an environment variable.
    Secret,
}

impl VaultKind {
    pub fn as_str(self) -> &'static str {
        match self {
            VaultKind::Text => "text",
            VaultKind::Url => "url",
            VaultKind::Secret => "secret",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "text" => Some(VaultKind::Text),
            "url" => Some(VaultKind::Url),
            "secret" => Some(VaultKind::Secret),
            _ => None,
        }
    }

    /// Whether an item of this kind may become `to` (B7).
    pub fn may_become(self, to: VaultKind) -> bool {
        self == to || (self != VaultKind::Secret && to != VaultKind::Secret)
    }
}

/// A board member as an item or an audit row names them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserRef {
    pub id: UserId,
    pub username: String,
}

/// One item as every client sees it.
#[derive(Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VaultItemView {
    pub id: VaultItemId,
    /// None: the company's.
    pub project_id: Option<ProjectId>,
    pub kind: VaultKind,
    pub name: String,
    /// How a run gets it (UPPER_SNAKE, unique in the scope): the environment
    /// variable, and the name in the vault tools.
    pub env_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The value of a text or url item; never a secret's (`vault.reveal`), and
    /// none when it cannot be decrypted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The server's vault key is missing or does not fit this value (a restore
    /// on another machine without the key, MU4-04 AC3): the client shows
    /// "Cannot be decrypted" instead of a value. The rest still works.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub undecryptable: bool,
    /// A project item whose env name a company item has too: in runs of the
    /// project this one wins (pill "overrides Company", B4).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub overrides_company: bool,
    /// Goes up with every change (the envelope's `expected_version`).
    pub version: u64,
    pub created_by: UserRef,
    pub updated_by: UserRef,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Prints whether there is a value, never the value.
impl fmt::Debug for VaultItemView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultItemView")
            .field("id", &self.id)
            .field("project_id", &self.project_id)
            .field("kind", &self.kind)
            .field("name", &self.name)
            .field("env_name", &self.env_name)
            .field("value", &self.value.as_ref().map(|_| "<redacted>"))
            .field("undecryptable", &self.undecryptable)
            .field("overrides_company", &self.overrides_company)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// How many items a scope has (the scope list of the vault screen).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VaultScopeCount {
    /// None: the company.
    pub project_id: Option<ProjectId>,
    pub count: u32,
}

/// `vault.list`: the items of the scope asked for (by name), and for a project
/// the company's apart (a run of the project gets those too, MU4-02 AC1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VaultList {
    /// The scope asked for; none: the company.
    pub project_id: Option<ProjectId>,
    pub items: Vec<VaultItemView>,
    /// For a project: the company's items (empty for the company itself).
    #[serde(default)]
    pub company_items: Vec<VaultItemView>,
    /// Every scope that has items, the company first (zero items included for it).
    #[serde(default)]
    pub counts: Vec<VaultScopeCount>,
}

/// Why `vault.reveal` is asked: written to the audit log as `reveal` or `copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RevealPurpose {
    Show,
    Copy,
}

/// `vault.reveal`: one item's value. Never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VaultValue {
    pub item_id: VaultItemId,
    pub value: String,
}

impl fmt::Debug for VaultValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultValue").field("item_id", &self.item_id).field("value", &"<redacted>").finish()
    }
}

/// `vault.key_export`: the server's vault key, to keep apart (a restore on
/// another machine needs it, B8). Never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VaultKeyExport {
    /// The key's fingerprint: what `aurelius-daemon vault-key import` checks
    /// against the stored values.
    pub key_id: String,
    /// The key as one line: `aurelius-vault-key:v1:<env>:<key_id>:<base64url>`
    /// (`<env>` `dev` or `prod`, the 32 bytes in base64url without padding).
    /// What `aurelius-daemon vault-key import` reads.
    pub key: String,
}

impl fmt::Debug for VaultKeyExport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultKeyExport").field("key_id", &self.key_id).field("key", &"<redacted>").finish()
    }
}

/// What an audit row records. Never a value, also not an old or a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VaultAuditAction {
    Create,
    /// Name, env name, description or kind (`changed_fields`).
    Edit,
    /// A new value (for a secret: "Replaced").
    Replace,
    Archive,
    /// A board member showed the value (`vault.reveal` show).
    Reveal,
    /// A board member copied it (`vault.reveal` copy).
    Copy,
    /// A run got it as an environment variable (one row per run and item).
    Deliver,
    /// A run read a text or url with `vault_read`.
    ToolRead,
    /// A board member exported the vault key (no item).
    KeyExport,
}

/// Who did it: a board member (on a device), or an employee in a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VaultActor {
    BoardMember { user: UserRef },
    Employee {
        employee_id: EmployeeId,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<RunId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<TaskId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ticket_id: Option<TicketId>,
    },
}

/// One row of the audit log (append-only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VaultAuditRow {
    /// Newest is highest; `vault.audit { before }` pages on it.
    pub id: i64,
    /// None: a key export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<VaultItemId>,
    /// The item's name at the time (also for an archived item).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_name: Option<String>,
    /// The item's scope; none: the company (or a key export).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    pub action: VaultAuditAction,
    pub actor: VaultActor,
    /// `edit`: which fields changed (`name`, `env_name`, `description`, `kind`); never their values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_fields: Vec<String>,
    /// The board member's device (the session's name, "Desktop · dev-laptop").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    pub at_ms: i64,
}

/// `vault.audit`: newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VaultAuditPage {
    pub rows: Vec<VaultAuditRow>,
    /// Older rows exist: ask again with `before` = the last row's id.
    pub more: bool,
}

/// The `metadata` of a `vault.item_*` event. Never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VaultEventMeta {
    pub id: VaultItemId,
    pub name: String,
    pub kind: VaultKind,
    pub project_id: Option<ProjectId>,
    pub version: u64,
    pub by: UserId,
    /// `vault.item_updated`: which fields changed (`value` for a new value).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_fields: Vec<String>,
}

/// The longest name and env name.
pub const MAX_NAME_CHARS: usize = 80;
/// The longest value (64 KiB): a vault keeps keys, urls and notes, not files.
pub const MAX_VALUE_BYTES: usize = 64 * 1024;

/// Environment names a vault item may not take: what every run's process needs
/// or gets from the runtime, and the switches of the model providers (an item
/// `ANTHROPIC_API_KEY` would overrule the board member's Claude login).
const RESERVED_ENV: &[&str] = &[
    // The runtime's allow-list and what it sets itself.
    "PATH", "PATHEXT", "SYSTEMROOT", "WINDIR", "COMSPEC", "TEMP", "TMP", "USERPROFILE", "HOMEDRIVE", "HOMEPATH",
    "APPDATA", "LOCALAPPDATA", "PROGRAMDATA", "USERNAME", "USERDOMAIN", "HOME", "USER", "LOGNAME", "SHELL", "TMPDIR",
    "LANG", "LC_ALL", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR", "CARGO_BUILD_JOBS",
    "PWD", "OLDPWD", "TERM", "IFS", "PS1", "PS4", "ENV", "BASH_ENV", "ZDOTDIR", "PROMPT_COMMAND", "SHELLOPTS", "BASHOPTS",
    "NODE_OPTIONS", "PYTHONPATH", "PYTHONSTARTUP", "REQUESTS_CA_BUNDLE", "CURL_CA_BUNDLE", "TEMP", "LANG",
];
/// Code in the claude process (Node) or in every shell, its traffic and its
/// telemetry (Sentinel SEC0 E7). The cloud's own credentials (`AWS_*`,
/// `GOOGLE_*`, `AZURE_*`) are allowed: Claude Code only uses them behind its
/// provider switches (`CLAUDE_CODE_USE_BEDROCK`/`_VERTEX`/`_FOUNDRY`), which
/// stay refused with `CLAUDE_*`, `ANTHROPIC_*`, `VERTEX_*` and `BEDROCK_*`
/// (Titan, Sentinel SEC1 point 7).
const RESERVED_ENV_PREFIXES: &[&str] =
    &["LD_", "DYLD_", "LC_", "XDG_", "ANTHROPIC_", "CLAUDE_", "AURELIUS_", "VERTEX_", "BEDROCK_", "OTEL_", "MCP_", "NODE_", "SSL_CERT_"];
/// `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`, …
const RESERVED_ENV_SUFFIXES: &[&str] = &["_PROXY"];

/// Whether the runtime keeps `name` for itself (case does not count: names on
/// Windows do not have it).
pub fn env_name_reserved(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    RESERVED_ENV.contains(&upper.as_str())
        || RESERVED_ENV_PREFIXES.iter().any(|p| upper.starts_with(p))
        || RESERVED_ENV_SUFFIXES.iter().any(|p| upper.ends_with(p))
}

/// The longest env name (`^[A-Z][A-Z0-9_]{0,63}$`, E7).
pub const MAX_ENV_NAME_CHARS: usize = 64;

/// Why a field of an item is refused (a `validation` error's message).
pub fn check_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the item a name.".into());
    }
    if name.chars().count() > MAX_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(format!("A name has at most {MAX_NAME_CHARS} characters, on one line."));
    }
    Ok(())
}

/// UPPER_SNAKE: A–Z first, then A–Z, 0–9 and `_`; not one the runtime keeps.
pub fn check_env_name(env_name: &str) -> Result<(), String> {
    let mut chars = env_name.chars();
    let shape = matches!(chars.next(), Some('A'..='Z')) && chars.all(|c| matches!(c, 'A'..='Z' | '0'..='9' | '_')) && env_name.len() <= MAX_ENV_NAME_CHARS;
    if !shape {
        return Err("A key is A–Z, 0–9 and _, starting with a letter (like GITHUB_TOKEN).".into());
    }
    if env_name_reserved(env_name) {
        return Err(format!("{env_name} is kept by Aurelius or Claude Code for every run. Choose another key."));
    }
    Ok(())
}

/// A value for `kind`: a url is `https://` or `http://` with a host, a secret
/// has at least `MIN_SECRET_CHARS` characters (shorter ones cannot be redacted
/// reliably) and no line break.
pub fn check_value(kind: VaultKind, value: &str) -> Result<(), String> {
    if value.len() > MAX_VALUE_BYTES {
        return Err("A value has at most 64 KB.".into());
    }
    match kind {
        VaultKind::Text if value.is_empty() => Err("Give the item a value.".into()),
        VaultKind::Text => Ok(()),
        VaultKind::Url => {
            let rest = value.strip_prefix("https://").or_else(|| value.strip_prefix("http://"));
            match rest {
                Some(rest) if !rest.is_empty() && !rest.starts_with('/') && !value.chars().any(char::is_whitespace) => Ok(()),
                _ => Err("Enter a full URL, starting with https://".into()),
            }
        }
        VaultKind::Secret if value.chars().count() < MIN_SECRET_CHARS => Err(format!("A secret has at least {MIN_SECRET_CHARS} characters.")),
        VaultKind::Secret if value.contains(['\n', '\r']) => Err("A secret is one line.".into()),
        VaultKind::Secret => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_url_trade_places_but_a_secret_stays_one() {
        use VaultKind::*;
        assert!(Text.may_become(Url) && Url.may_become(Text) && Secret.may_become(Secret));
        for (from, to) in [(Secret, Text), (Secret, Url), (Text, Secret), (Url, Secret)] {
            assert!(!from.may_become(to), "{from:?} -> {to:?}");
        }
        for kind in [Text, Url, Secret] {
            assert_eq!(VaultKind::parse(kind.as_str()), Some(kind));
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.as_str());
        }
    }

    #[test]
    fn env_names_are_upper_snake_and_never_the_runtimes() {
        // The cloud's credentials go to the tools a run calls (SEC1 point 7).
        for ok in ["GITHUB_TOKEN", "STAGING_URL", "A", "X9_Y", "AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "GOOGLE_APPLICATION_CREDENTIALS", "AZURE_CLIENT_ID"] {
            assert_eq!(check_env_name(ok), Ok(()), "{ok}");
        }
        for shape in ["", "github_token", "1TOKEN", "_X", "MY-KEY", "MY KEY", "TÖKEN"] {
            assert!(check_env_name(shape).is_err(), "{shape}");
        }
        for reserved in ["PATH", "HOME", "USER", "SHELL", "LD_PRELOAD", "DYLD_INSERT_LIBRARIES", "ANTHROPIC_API_KEY", "ANTHROPIC_BASE_URL", "CLAUDE_CONFIG_DIR", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX", "CLAUDE_CODE_USE_FOUNDRY", "ANTHROPIC_BEDROCK_BASE_URL", "ANTHROPIC_VERTEX_PROJECT_ID", "VERTEX_REGION_CLAUDE_3_7_SONNET", "BEDROCK_MODEL", "LANG", "TMPDIR", "XDG_CONFIG_HOME", "CARGO_BUILD_JOBS", "AURELIUS_DATA_DIR"] {
            let err = check_env_name(reserved).unwrap_err();
            assert!(err.contains(reserved), "{reserved}: {err}");
            assert!(env_name_reserved(&reserved.to_ascii_lowercase()));
        }
    }

    #[test]
    fn sec0_e7_names_that_steer_the_claude_process_its_shells_or_its_traffic_are_refused() {
        let refused = [
            "NODE_OPTIONS", "NODE_EXTRA_CA_CERTS", "NODE_TLS_REJECT_UNAUTHORIZED", "HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY", "NO_PROXY",
            "SSL_CERT_FILE", "SSL_CERT_DIR", "REQUESTS_CA_BUNDLE", "CURL_CA_BUNDLE", "BASH_ENV", "ENV", "ZDOTDIR", "PROMPT_COMMAND", "IFS",
            "PS4", "SHELLOPTS", "BASHOPTS", "OTEL_EXPORTER_OTLP_ENDPOINT", "MCP_TIMEOUT", "AURELIUS_SECRETS", "CARGO_BUILD_JOBS", "TMPDIR",
            "TEMP", "TMP", "LANG", "LC_CTYPE", "XDG_RUNTIME_DIR", "ANTHROPIC_API_KEY", "PATH",
        ];
        for name in refused {
            assert!(check_env_name(name).is_err(), "{name}");
            assert!(env_name_reserved(&name.to_ascii_lowercase()), "{name} without case");
        }
        // The shape: ^[A-Z][A-Z0-9_]{0,63}$; `Path` (Windows) has the wrong case anyway.
        assert!(check_env_name(&format!("A{}", "B".repeat(63))).is_ok());
        assert!(check_env_name(&format!("A{}", "B".repeat(64))).is_err());
        for bad in ["Path", "path", "1ABC", "_ABC", "A-B", "A B", ""] {
            assert!(check_env_name(bad).is_err(), "{bad}");
        }
        for fine in ["GITHUB_TOKEN", "STRIPE_KEY", "DEPLOY_PROXY_URL", "CANARY_TOKEN"] {
            assert!(check_env_name(fine).is_ok(), "{fine}");
        }
    }

    #[test]
    fn values_follow_their_kind() {
        assert!(check_value(VaultKind::Url, "https://stg.example.com/x").is_ok() && check_value(VaultKind::Url, "http://localhost:8080").is_ok());
        for bad in ["stg.example.com", "ftp://x", "https://", "https:///x", "https://a b", "javascript:alert(1)"] {
            assert!(check_value(VaultKind::Url, bad).is_err(), "{bad}");
        }
        assert!(check_value(VaultKind::Secret, "1234567").is_err(), "under 8 characters");
        assert!(check_value(VaultKind::Secret, "12345678").is_ok());
        assert!(check_value(VaultKind::Secret, "abcd\nefgh").is_err());
        assert!(check_value(VaultKind::Text, "").is_err() && check_value(VaultKind::Text, "Run make deploy\nthen check").is_ok());
        assert!(check_value(VaultKind::Text, &"x".repeat(MAX_VALUE_BYTES + 1)).is_err());
        assert!(check_name("GitHub token").is_ok() && check_name("  ").is_err() && check_name("a\nb").is_err());
    }

    #[test]
    fn values_and_the_key_never_show_in_debug_output() {
        let by = UserRef { id: UserId::new(), username: "alex".into() };
        let item = VaultItemView {
            id: VaultItemId::new(),
            project_id: None,
            kind: VaultKind::Text,
            name: "Notes".into(),
            env_name: "NOTES".into(),
            description: None,
            value: Some("sk-test-FAKE-0000".into()),
            undecryptable: false,
            overrides_company: false,
            version: 1,
            created_by: by.clone(),
            updated_by: by,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let value = VaultValue { item_id: item.id, value: "sk-test-FAKE-0000".into() };
        let key = VaultKeyExport { key_id: "k1".into(), key: "c2stdGVzdC1GQUtFLTAwMDA=".into() };
        for printed in [format!("{item:?}"), format!("{value:?}"), format!("{key:?}")] {
            assert!(!printed.contains("FAKE") && !printed.contains("c2st"), "{printed}");
        }
        // The flags are left out when false, so an item reads compactly.
        let json = serde_json::to_value(&item).unwrap();
        assert!(json.get("undecryptable").is_none() && json.get("overrides_company").is_none());
    }
}
