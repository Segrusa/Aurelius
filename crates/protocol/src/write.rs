//! Writes: every method that changes something. They all pass the same
//! pipeline: the idempotency key from the envelope, a receipt, an audit event
//! and the right the method requires. Domain commands run in `application`;
//! host writes (MCP, load, backups, …) run in the daemon, which
//! records their receipt and audit event through the same application call.

use crate::remote::{PushConfig, RemoteStatus, PairingCode};
use crate::views::*;
use aurelius_domain::{
    ClientId, Command, CommandId, CommandResult, ProviderId, ProjectId,
    COMMAND_METHODS,
};
use schemars::JsonSchema;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A write: a domain command, or something only the host can do.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteMethod {
    Domain(Command),
    Host(HostWrite),
}

impl WriteMethod {
    /// The `<area>.<verb>` name of this write.
    pub fn name(&self) -> String {
        match self {
            WriteMethod::Domain(command) => command.method().to_owned(),
            WriteMethod::Host(host) => host.name(),
        }
    }

    /// Whether `method` names a write (domain command or host write).
    pub fn knows(method: &str) -> bool {
        COMMAND_METHODS.iter().any(|(_, m)| *m == method) || HostWrite::NAMES.contains(&method)
    }
}

impl Serialize for WriteMethod {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            WriteMethod::Host(host) => host.serialize(serializer),
            WriteMethod::Domain(command) => {
                let mut value = serde_json::to_value(command).map_err(serde::ser::Error::custom)?;
                let object = value.as_object_mut().ok_or_else(|| serde::ser::Error::custom("a command is a JSON object"))?;
                object.remove("type");
                serde_json::json!({ "method": command.method(), "params": value }).serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for WriteMethod {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = serde_json::Value::deserialize(deserializer)?;
        let name = value.get("method").and_then(|m| m.as_str()).unwrap_or("").to_owned();
        if let Some((kind, _)) = COMMAND_METHODS.iter().find(|(_, m)| *m == name) {
            let mut params = value.get_mut("params").map(serde_json::Value::take).filter(|p| !p.is_null()).unwrap_or_else(|| serde_json::json!({}));
            let object = params.as_object_mut().ok_or_else(|| D::Error::custom(format!("{name}: params must be an object")))?;
            object.insert("type".into(), (*kind).into());
            return serde_json::from_value(params).map(WriteMethod::Domain).map_err(|e| D::Error::custom(format!("{name}: {e}")));
        }
        crate::rpc::from_method_value(value).map(WriteMethod::Host).map_err(D::Error::custom)
    }
}

impl JsonSchema for WriteMethod {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "WriteMethod".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Every domain command as `{ method, params }`, plus the host writes.
        let commands = generator.subschema_for::<Command>();
        let hosts = generator.subschema_for::<HostWrite>();
        schemars::json_schema!({ "anyOf": [commands, hosts] })
    }
}

/// Writes only the host can do. Secrets in parameters are never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", content = "params")]
pub enum HostWrite {
    #[serde(rename = "secret.set")]
    SecretSet { connection_id: ProviderId, secret: String },
    #[serde(rename = "mcp.set")]
    McpSet { enabled: Vec<String> },
    #[serde(rename = "mcp.add")]
    McpAdd(McpAddParams),
    #[serde(rename = "mcp.remove")]
    McpRemove { name: String },
    /// Starts an OAuth login for an http/sse server; answers `McpLogin` with the URL to open.
    #[serde(rename = "mcp.login_start")]
    McpLoginStart { name: String },
    /// Finishes a login with the address the browser ended on (`…/callback?code=…&state=…`),
    /// for a browser that is not on the server's computer.
    #[serde(rename = "mcp.login_complete")]
    McpLoginComplete { login_id: String, redirect_url: String },
    /// Forgets the server's tokens (keeps its client registration).
    #[serde(rename = "mcp.logout")]
    McpLogout { name: String },
    /// Take a tool off the always-allow list: its next call asks again.
    #[serde(rename = "mcp_always_allow.remove")]
    McpAlwaysAllowRemove { tool: String },
    #[serde(rename = "load.set_limits")]
    LoadSetLimits(LoadLimitsUpdate),
    #[serde(rename = "browser.set")]
    BrowserSet { choice: BrowserChoice },
    /// Claude in Chrome or Playwright, for every run that starts afterwards.
    #[serde(rename = "browser.set_backend")]
    BrowserSetBackend { backend: BrowserBackend },
    /// Management chooses its own task mode (feature `management_chooses_mode`), for every run
    /// that starts afterwards. On by default.
    #[serde(rename = "management_mode.set_enabled")]
    ManagementModeSetEnabled { enabled: bool },
    /// Above how many tokens of context a session is compacted before its next turn
    /// (feature `compaction_setting`), from the next run on; 0: never.
    #[serde(rename = "compaction.set_threshold")]
    CompactionSetThreshold { threshold_tokens: u64 },
    /// Auto-allow of the safe read-only tools on or off, from the next tool call. On by default.
    #[serde(rename = "auto_allow.set_enabled")]
    AutoAllowSetEnabled { enabled: bool },
    /// Trace mode on or off for every run that starts afterwards (feature `run_trace`).
    /// Off also forgets the traces kept so far.
    #[serde(rename = "trace.set_enabled")]
    TraceSetEnabled { enabled: bool },
    #[serde(rename = "notification.mark_all_read")]
    NotificationMarkAllRead,
    /// One row read, for this board member only (feature `personal_inbox`); answers
    /// `Count`, 0 when there was nothing to read, so it can be repeated.
    #[serde(rename = "notification.mark_read")]
    NotificationMarkRead { id: String },
    #[serde(rename = "notification.dismiss")]
    NotificationDismiss { id: String },
    #[serde(rename = "notification.dismiss_all")]
    NotificationDismissAll,
    #[serde(rename = "backup.create")]
    BackupCreate,
    /// Restore at the next start (the service stops now); work stays paused until resumed.
    #[serde(rename = "backup.restore")]
    BackupRestore { id: String },
    #[serde(rename = "backup.resume")]
    BackupResume,
    /// Write a readable Markdown + JSON export of a project into its folder.
    #[serde(rename = "project.export")]
    ProjectExport { project_id: ProjectId },
    #[serde(rename = "power.shutdown")]
    PowerShutdown { when: ShutdownWhen },
    #[serde(rename = "power.cancel")]
    PowerCancel,
    #[serde(rename = "server.restart")]
    ServerRestart,
    #[serde(rename = "update.prepare")]
    UpdatePrepare,
    #[serde(rename = "update.apply")]
    UpdateApply,
    #[serde(rename = "update.prepare_app")]
    UpdatePrepareApp,
    #[serde(rename = "update.apply_app")]
    UpdateApplyApp,
    #[serde(rename = "remote.enable")]
    RemoteEnable,
    #[serde(rename = "remote.disable")]
    RemoteDisable,
    /// A new one-time code (5 minutes) plus the QR payload; the receipt keeps neither.
    #[serde(rename = "remote.start_pairing")]
    RemoteStartPairing,
    /// Allow or refuse a device that entered the code. A computer that asks for
    /// full access is only allowed with `owner` (the user confirmed it).
    #[serde(rename = "remote.decide_pairing")]
    RemoteDecidePairing { pairing_id: String, approve: bool, owner: bool },
    #[serde(rename = "remote.revoke")]
    RemoteRevoke { client_id: ClientId },
    #[serde(rename = "push.configure")]
    PushConfigure(PushConfig),
    #[serde(rename = "push.test")]
    PushTest { client_id: ClientId },
    /// A new folder `name` in the folder `parent`.
    #[serde(rename = "fs.create_dir")]
    FsCreateDir { parent: String, name: String },
    /// A new board member (OV-12: `invalid_username`, `weak_password` (also a
    /// password of only spaces), `username_taken`; `too_many_attempts` after five
    /// a minute on one connection or twenty an hour on the server); answers
    /// `CommandResult::UserCreated` as a command receipt. Without a session only
    /// the first board member (the onboarding): that connection then gets a
    /// session, whose token `user.login` would give (see `HelloResult::user`;
    /// the client signs in once more to get the token). After that it needs a
    /// session (any board member, one role). The server hashes the password
    /// before anything is recorded: the receipt, its request hash and the event
    /// `user.created` hold the name only. Not for apps.
    #[serde(rename = "user.create")]
    UserCreate(UserCreateParams),
    /// End one session (any board member's: one role). Its device's open
    /// connection closes at once (`ServerMessage::SignedOut`); the timeline says
    /// who revoked it. Answers `Done`. Not for apps.
    #[serde(rename = "session.revoke")]
    SessionRevoke { session_id: aurelius_domain::SessionId },
    /// Change a password. Own (no `user_id`, or the session's): with the old
    /// password; the other sessions end, this one stays. Another board member's
    /// (B10): a temporary password, changed at their next sign-in
    /// (`must_change_password`); all their sessions end. Like `user.create`, the
    /// passwords are in no receipt, request hash, event or log. Answers `Done`.
    #[serde(rename = "user.set_password")]
    UserSetPassword(SetPasswordParams),
    /// Disable a board member: their sessions end at once and they cannot sign
    /// in; their Claude login is disconnected; tickets keep their creator. The
    /// last active board member cannot be disabled (`last_board_member`).
    /// Answers `User`. Not for apps.
    #[serde(rename = "user.disable")]
    UserDisable { user_id: aurelius_domain::UserId },
    /// Connect the session's own Claude login: the server starts `claude auth
    /// login` for `<data>/logins/<user_id>/` and answers `ClaudeLoginLink`. What
    /// Claude asks (the sign-in link, the code) comes to this connection only as
    /// `ServerMessage::ClaudeLoginPrompt`; the code goes back with
    /// `claude_login.input`. One link per board member at a time, 10 minutes.
    #[serde(rename = "claude_login.link")]
    ClaudeLoginLink,
    /// Stop a running link of the session's own login. Answers `ClaudeLogin`.
    #[serde(rename = "claude_login.cancel")]
    ClaudeLoginCancel { link_id: String },
    /// Disconnect the session's own Claude login (`claude auth logout`, its folder
    /// removed). Answers `ClaudeLogin`.
    #[serde(rename = "claude_login.unlink")]
    ClaudeLoginUnlink,
    /// A new vault item (feature `vault`), in the company (no `project_id`) or a
    /// project. Answers `VaultItem` (`value` only for text/url). The server
    /// encrypts the value before anything is recorded: the receipt, its request
    /// hash and the event `vault.item_created` hold no value (MU4-05 AC2).
    /// `validation` for a name or env name used in the scope, an env name that
    /// is not UPPER_SNAKE or that the runtime reserves (`PATH`, `ANTHROPIC_*`,
    /// …), a url without `https://`/`http://`, a secret under 8 characters.
    /// Board members with a session only (employees and agent tools never); not for apps.
    #[serde(rename = "vault.create")]
    VaultCreate(VaultCreateParams),
    /// Change an item; what is left out stays. `value`: a new value (a secret's
    /// is never sent back to the editor, so "leave empty to keep"). `kind` only
    /// between text and url: to or from secret is `vault_kind_locked` (B7).
    /// With `expected_version` in the envelope a stale edit is `version_conflict`.
    /// Answers `VaultItem`. As `vault.create`: no value in receipt, hash or event.
    #[serde(rename = "vault.update")]
    VaultUpdate(VaultUpdateParams),
    /// Archive ("Delete") an item: runs and employees no longer get it; its audit
    /// history stays. Answers `VaultItem`. Not for apps.
    #[serde(rename = "vault.archive")]
    VaultArchive { item_id: aurelius_domain::VaultItemId },
}

/// `vault.create`. The value is never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VaultCreateParams {
    /// None: the company's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    pub kind: aurelius_domain::vault::VaultKind,
    pub name: String,
    pub env_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub value: String,
}

impl fmt::Debug for VaultCreateParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultCreateParams")
            .field("project_id", &self.project_id)
            .field("kind", &self.kind)
            .field("name", &self.name)
            .field("env_name", &self.env_name)
            .field("description", &self.description)
            .field("value", &"<redacted>")
            .finish()
    }
}

/// `vault.update`. The value is never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VaultUpdateParams {
    pub item_id: aurelius_domain::VaultItemId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_name: Option<String>,
    /// `Some("")` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<aurelius_domain::vault::VaultKind>,
}

impl fmt::Debug for VaultUpdateParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultUpdateParams")
            .field("item_id", &self.item_id)
            .field("name", &self.name)
            .field("env_name", &self.env_name)
            .field("description", &self.description)
            .field("value", &self.value.as_ref().map(|_| "<redacted>"))
            .field("kind", &self.kind)
            .finish()
    }
}

/// `user.create`. The password is never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UserCreateParams {
    pub username: String,
    pub password: String,
}

impl fmt::Debug for UserCreateParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserCreateParams").field("username", &self.username).field("password", &"<redacted>").finish()
    }
}

/// `user.set_password`. No password is ever printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetPasswordParams {
    /// Another board member (a temporary password); none: the session's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<aurelius_domain::UserId>,
    /// The current password; required for the own one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_password: Option<String>,
    pub new_password: String,
}

impl fmt::Debug for SetPasswordParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetPasswordParams").field("user_id", &self.user_id).field("old_password", &self.old_password.as_ref().map(|_| "<redacted>")).field("new_password", &"<redacted>").finish()
    }
}

impl HostWrite {
    /// Every host write name, in declaration order (checked by a test against the schema).
    pub const NAMES: &'static [&'static str] = &[
        "secret.set",
        "mcp.set",
        "mcp.add",
        "mcp.remove",
        "mcp.login_start",
        "mcp.login_complete",
        "mcp.logout",
        "mcp_always_allow.remove",
        "load.set_limits",
        "browser.set",
        "browser.set_backend",
        "management_mode.set_enabled",
        "compaction.set_threshold",
        "auto_allow.set_enabled",
        "trace.set_enabled",
        "notification.mark_all_read",
        "notification.mark_read",
        "notification.dismiss",
        "notification.dismiss_all",
        "backup.create",
        "backup.restore",
        "backup.resume",
        "project.export",
        "power.shutdown",
        "power.cancel",
        "server.restart",
        "update.prepare",
        "update.apply",
        "update.prepare_app",
        "update.apply_app",
        "remote.enable",
        "remote.disable",
        "remote.start_pairing",
        "remote.decide_pairing",
        "remote.revoke",
        "push.configure",
        "push.test",
        "fs.create_dir",
        "user.create",
        "session.revoke",
        "user.set_password",
        "user.disable",
        "claude_login.link",
        "claude_login.cancel",
        "claude_login.unlink",
        "vault.create",
        "vault.update",
        "vault.archive",
    ];

    pub fn name(&self) -> String {
        match serde_json::to_value(self) {
            Ok(value) => value["method"].as_str().unwrap_or("").to_owned(),
            Err(_) => String::new(),
        }
    }
}

impl fmt::Debug for HostWrite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Parameters can hold secrets (a connection key, MCP env values, the APNs key): print the name only.
        write!(f, "HostWrite({})", self.name())
    }
}

/// What a host write answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostResult {
    Done,
    ShuttingDown,
    /// Notifications changed.
    Count { count: u64 },
    McpServers(McpServers),
    McpLogin(McpLogin),
    McpAlwaysAllow(McpAlwaysAllow),
    Load(LoadSnapshot),
    Browser(BrowserSetting),
    ManagementModeSetting(ManagementModeSetting),
    AutoAllowSetting(AutoAllowSetting),
    TraceSetting(TraceSetting),
    CompactionSetting(CompactionSetting),
    Backups(BackupsSnapshot),
    Exported { markdown_path: String, json_path: String },
    Power(PowerState),
    Update(UpdateStatus),
    Remote(RemoteStatus),
    /// A replayed receipt has empty codes.
    PairingCode(PairingCode),
    DirListing(DirListing),
    /// `user.disable`: the board member as it is now.
    User(aurelius_domain::user::UserView),
    /// `claude_login.link`: the link that runs; its prompts follow as
    /// `ServerMessage::ClaudeLoginPrompt` with this id.
    ClaudeLoginLink { link_id: String },
    /// `claude_login.cancel` / `claude_login.unlink`: the login as it is now.
    ClaudeLogin(aurelius_domain::user::ClaudeLoginView),
    /// `vault.create` / `vault.update` / `vault.archive`: the item as it is now.
    /// A replayed receipt has no value: read it with `vault.list` or `vault.reveal`.
    VaultItem(aurelius_domain::vault::VaultItemView),
}

impl HostResult {
    /// What a receipt keeps: secrets blanked, so a replay never shows them again.
    pub fn redacted(&self) -> HostResult {
        match self {
            HostResult::PairingCode(code) => HostResult::PairingCode(PairingCode { code: String::new(), qr: String::new(), expires_at_ms: code.expires_at_ms, pairing_text: String::new() }),
            HostResult::VaultItem(item) => HostResult::VaultItem(aurelius_domain::vault::VaultItemView { value: None, ..item.clone() }),
            other => other.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum WriteResult {
    Command(CommandResult),
    Host(HostResult),
}

/// The answer to every write.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Receipt {
    pub idempotency_key: CommandId,
    pub committed_seq: u64,
    /// True when this is the receipt of an earlier identical write.
    pub replayed: bool,
    pub result: WriteResult,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn host_write_names_match_the_enum() {
        let schema = serde_json::to_value(schemars::schema_for!(HostWrite)).unwrap();
        let variants = schema["oneOf"].as_array().expect("tagged enum");
        let names: BTreeSet<&str> = variants.iter().filter_map(|v| v["properties"]["method"]["const"].as_str()).collect();
        assert_eq!(names, HostWrite::NAMES.iter().copied().collect::<BTreeSet<_>>());
        assert_eq!(names.len(), variants.len());
    }

    #[test]
    fn domain_commands_travel_as_method_and_params() {
        let command = Command::CreateProject(aurelius_domain::project::CreateProject { name: "Atlas".into(), description: None, folder: None });
        let write = WriteMethod::Domain(command);
        let json = serde_json::to_value(&write).unwrap();
        assert_eq!(json["method"], "project.create");
        assert_eq!(json["params"]["name"], "Atlas");
        assert!(json["params"].get("type").is_none(), "the method name replaces the type tag");
        assert_eq!(serde_json::from_value::<WriteMethod>(json).unwrap(), write);
    }

    #[test]
    fn host_writes_travel_as_method_and_params_and_hide_secrets_in_debug() {
        let write = WriteMethod::Host(HostWrite::SecretSet { connection_id: ProviderId::new(), secret: "sk-SECRET".into() });
        let json = serde_json::to_value(&write).unwrap();
        assert_eq!(json["method"], "secret.set");
        assert_eq!(serde_json::from_value::<WriteMethod>(json).unwrap(), write);
        assert!(!format!("{write:?}").contains("SECRET"));
        assert!(WriteMethod::knows("project.create") && WriteMethod::knows("mcp.add") && !WriteMethod::knows("project.list"));
    }

    #[test]
    fn receipts_keep_no_secrets() {
        let pairing = HostResult::PairingCode(PairingCode { code: "ABCD2345".into(), qr: "{}".into(), expires_at_ms: 9, pairing_text: "aurelius-pair:x".into() });
        let kept = serde_json::to_string(&pairing.redacted()).unwrap();
        assert!(!kept.contains("ABCD2345") && !kept.contains("aurelius-pair"));
    }
}
