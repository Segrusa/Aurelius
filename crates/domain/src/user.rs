//! Board members and their sessions (protocol 5). A connection has at most one
//! board member, and only through a session: `user.login`, the first
//! `user.create`, or the session token in the hello. Without one the server
//! answers `unauthenticated` (the gate).
//!
//! Neither a password nor a session token enters the domain: the daemon hashes
//! the password (argon2id) and keeps only the token's SHA-256, so they are in
//! no command, receipt hash, event or log.

use crate::{ClientId, DomainError, ProjectId, SessionId, UserId};
use serde::{Deserialize, Serialize};

pub const USERNAME_MIN: usize = 3;
pub const USERNAME_MAX: usize = 32;
pub const PASSWORD_MIN: usize = 12;
/// Longer input only costs hashing time.
pub const PASSWORD_MAX: usize = 1_024;

/// A board member's name (OV-12): 3 to 32 letters, digits, `.`, `_` or `-`;
/// unique on the server regardless of case (the database decides that).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Username(String);

impl Username {
    pub fn parse(text: &str) -> Result<Self, DomainError> {
        let text = text.trim();
        let count = text.chars().count();
        if !(USERNAME_MIN..=USERNAME_MAX).contains(&count) {
            return Err(DomainError::InvalidUsername(format!("A name has {USERNAME_MIN} to {USERNAME_MAX} characters.")));
        }
        if !text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
            return Err(DomainError::InvalidUsername("Use only letters, digits, '.', '_' and '-'.".into()));
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// At least 12 characters (OV-12), not only spaces. Checked before hashing.
pub fn check_password(password: &str) -> Result<(), DomainError> {
    let count = password.chars().count();
    if count > 0 && password.trim().is_empty() {
        return Err(DomainError::WeakPassword("A password of only spaces is too weak.".into()));
    }
    if count < PASSWORD_MIN {
        return Err(DomainError::WeakPassword(format!("A password has at least {PASSWORD_MIN} characters.")));
    }
    // In bytes, which bound the hashing work (SEC1 m7): never more than in characters.
    if password.len() > PASSWORD_MAX {
        return Err(DomainError::WeakPassword(format!("A password has at most {PASSWORD_MAX} characters.")));
    }
    Ok(())
}

/// A board member as every client sees it; never the password's hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserView {
    pub id: UserId,
    pub username: String,
    pub created_at_ms: i64,
    /// None: made in the onboarding, without an active board member.
    pub created_by_user: Option<UserId>,
    /// Disabled (`user.disable`): cannot sign in; tickets keep their creator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_at_ms: Option<i64>,
}

/// `user.list`, oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsersSnapshot {
    pub users: Vec<UserView>,
}

/// `user.whoami`: what the onboarding gate needs (also in the hello).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WhoAmI {
    pub board_members_exist: bool,
    /// The board member of this connection's session.
    pub user: Option<UserView>,
    /// The session's board member signed in with a temporary password and must
    /// change it (`user.set_password`) before anything else works.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub must_change_password: bool,
}

/// `user.login`: what the client keeps for the server. The token is shown
/// here once; the server keeps only its SHA-256. Never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LoginResult {
    pub user: UserView,
    /// Give it in every hello (`HelloParams::session_token`) on this device.
    pub session_token: String,
    /// When the session ends at the latest (90 days); 30 days without use end it earlier.
    pub expires_at_ms: i64,
    /// A temporary password: change it with `user.set_password` first.
    pub must_change_password: bool,
}

impl std::fmt::Debug for LoginResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginResult")
            .field("user", &self.user)
            .field("session_token", &"<redacted>")
            .field("expires_at_ms", &self.expires_at_ms)
            .field("must_change_password", &self.must_change_password)
            .finish()
    }
}

/// One signed-in device of a board member (`session.list`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionView {
    pub id: SessionId,
    pub user_id: UserId,
    /// What the device called itself at `user.login`.
    pub device_name: String,
    /// The paired device; none: the desktop on the server itself (local IPC).
    pub client_id: Option<ClientId>,
    pub created_at_ms: i64,
    pub last_seen_ms: i64,
    /// The absolute end (90 days); 30 days without use end it earlier.
    pub expires_at_ms: i64,
    /// The session of the connection that asked.
    pub current: bool,
}

/// `session.list`: newest activity first; never a token or its hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionsSnapshot {
    pub user_id: UserId,
    pub sessions: Vec<SessionView>,
}

/// Why the server ended a connection's session (`ServerMessage::SignedOut`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SignOutReason {
    /// `user.logout` on this device.
    LoggedOut,
    /// `session.revoke` (this or another device), or the paired device was revoked.
    Revoked,
    /// The password changed (on another device, or reset by another board member).
    PasswordChanged,
    /// The board member was disabled.
    Disabled,
    /// 30 days without use, or 90 days in all.
    Expired,
}

/// The state of a board member's own Claude login (`<data>/logins/<user_id>/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeLoginStatus {
    /// Never connected, or disconnected: the board member's tickets wait (`needs: login`).
    Missing,
    Linked,
    /// Claude asked to sign in again (a 401 in a run): Reconnect.
    Expired,
}

/// `claude_login.status`, `HelloResult::claude_login`: the session's own login.
/// Never a token or a path; the email and the plan come from `claude auth status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClaudeLoginView {
    pub user_id: UserId,
    pub status: ClaudeLoginStatus,
    pub email: Option<String>,
    /// e.g. "max", "pro"; none when unknown.
    pub plan: Option<String>,
    pub linked_at_ms: Option<i64>,
    pub checked_at_ms: Option<i64>,
    /// A `claude_login.link` of this board member is running (one at a time).
    pub link_id: Option<String>,
    /// This login also runs the company's background jobs (memory
    /// consolidation, B6): `company.set_background_login`.
    pub background: bool,
}

/// The domain side of `user.create`: the name only. The password's hash is
/// passed beside it, straight to storage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateUser {
    pub username: String,
}

/// The most workspaces one board member can pin to their rail. Far above what a
/// rail shows; it only bounds the size of one write.
pub const RAIL_PINS_MAX: usize = 500;

/// `user.set_rail_pins` (feature `rail_pins`): the workspaces the board member of
/// the session pins to their rail, in rail order. Replaces the whole list, so
/// reordering and unpinning are the same write; an empty list is a choice too
/// ("none"). Answers `RailPinsSet`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetRailPins {
    pub project_ids: Vec<ProjectId>,
}

impl SetRailPins {
    /// The list to store: at most `RAIL_PINS_MAX`, a workspace named twice only
    /// at its first place.
    pub fn valid_pins(&self) -> Result<Vec<ProjectId>, DomainError> {
        if self.project_ids.len() > RAIL_PINS_MAX {
            return Err(DomainError::OutOfRange { field: "project_ids".into(), detail: format!("at most {RAIL_PINS_MAX} workspaces can be pinned") });
        }
        let mut pins: Vec<ProjectId> = Vec::with_capacity(self.project_ids.len());
        for id in &self.project_ids {
            if !pins.contains(id) {
                pins.push(*id);
            }
        }
        Ok(pins)
    }
}

/// `user.rail_pins` (feature `rail_pins`): the workspaces the board member of
/// the session pinned to their rail, in rail order. Archived ones stay listed;
/// the client shows only open ones.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RailPinsView {
    /// False until the board member pins or unpins for the first time: the
    /// client then pins every open workspace, so no rail starts empty.
    pub chosen: bool,
    pub project_ids: Vec<ProjectId>,
}

/// The longest model name a board member's default can hold, as for a task's model.
pub const RUN_DEFAULT_MODEL_MAX: usize = 200;

/// A board member's own defaults for the runs that work for them (feature
/// `user_run_defaults`): runs on their login (the tickets they made, the sessions
/// they started) use these unless the conversation or the task chose otherwise.
/// They replace the employee's and the company's model and effort. None: not
/// chosen, so the connection's default model, the model's own effort and Ask.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserRunDefaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<crate::autonomy::AgentMode>,
    /// The model of the subagents a run starts itself (Claude Code's Agent tool).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_model: Option<String>,
}

impl UserRunDefaults {
    /// Trimmed, an empty text as not chosen; a model of at most
    /// `RUN_DEFAULT_MODEL_MAX` characters and a known effort level.
    pub fn validated(&self) -> Result<UserRunDefaults, DomainError> {
        let text = |v: &Option<String>| v.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned);
        let (model, effort, subagent_model) = (text(&self.model), text(&self.effort), text(&self.subagent_model));
        for (field, value) in [("model", &model), ("subagent_model", &subagent_model)] {
            if value.as_ref().is_some_and(|m| m.chars().count() > RUN_DEFAULT_MODEL_MAX) {
                return Err(DomainError::OutOfRange { field: field.into(), detail: format!("at most {RUN_DEFAULT_MODEL_MAX} characters") });
            }
        }
        if effort.as_ref().is_some_and(|e| !crate::connection::EFFORT_LEVELS.contains(&e.as_str())) {
            return Err(DomainError::OutOfRange { field: "effort".into(), detail: crate::connection::EFFORT_LEVELS.join(", ") });
        }
        Ok(UserRunDefaults { model, effort, mode: self.mode, subagent_model })
    }

    /// Whether setting these needs the right to approve from a phone: Auto mode
    /// lets the runs act without asking, as switching a conversation to Auto does.
    pub fn needs_approve(&self) -> bool {
        self.mode == Some(crate::autonomy::AgentMode::Auto)
    }
}

/// `user.set_run_defaults` (feature `user_run_defaults`): the board member of
/// the session sets their own defaults. Replaces them as a whole; an empty one
/// clears them. Answers `UserRunDefaultsSet`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetUserRunDefaults {
    pub defaults: UserRunDefaults,
}

/// `user.run_defaults` (feature `user_run_defaults`): the defaults of the board
/// member of the session, never another's, and what a run of theirs comes to.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserRunDefaultsView {
    /// What they chose.
    pub defaults: UserRunDefaults,
    /// The model a run of theirs uses without a choice of its own: theirs, else
    /// the connection's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The effort levels that model takes, for the effort picker.
    #[serde(default)]
    pub supported_efforts: Vec<String>,
    /// The model's own effort, shown as "Default (…)".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_default_effort: Option<String>,
    /// A chosen effort the model does not take: a run leaves it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsupported_effort: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_defaults_are_trimmed_and_an_empty_text_is_not_chosen() {
        let set = UserRunDefaults { model: Some("  claude-opus-5-5 ".into()), effort: Some(" ".into()), mode: Some(crate::autonomy::AgentMode::Plan), subagent_model: Some(String::new()) };
        let valid = set.validated().unwrap();
        assert_eq!(valid, UserRunDefaults { model: Some("claude-opus-5-5".into()), effort: None, mode: Some(crate::autonomy::AgentMode::Plan), subagent_model: None });
    }

    #[test]
    fn an_unknown_effort_or_a_too_long_model_is_refused() {
        let effort = UserRunDefaults { effort: Some("extreme".into()), ..Default::default() };
        assert!(matches!(effort.validated(), Err(DomainError::OutOfRange { field, .. }) if field == "effort"));
        let long = UserRunDefaults { subagent_model: Some("m".repeat(RUN_DEFAULT_MODEL_MAX + 1)), ..Default::default() };
        assert!(matches!(long.validated(), Err(DomainError::OutOfRange { field, .. }) if field == "subagent_model"));
    }

    #[test]
    fn only_auto_as_default_mode_needs_approve() {
        use crate::autonomy::AgentMode;
        assert!(UserRunDefaults { mode: Some(AgentMode::Auto), ..Default::default() }.needs_approve());
        assert!(!UserRunDefaults { mode: Some(AgentMode::AcceptEdits), model: Some("m".into()), ..Default::default() }.needs_approve());
    }

    #[test]
    fn rail_pins_keep_their_order_and_a_workspace_named_twice_only_at_its_first_place() {
        let (a, b) = (ProjectId::new(), ProjectId::new());
        let pins = SetRailPins { project_ids: vec![b, a, b] }.valid_pins().unwrap();
        assert_eq!(pins, [b, a]);
    }

    #[test]
    fn more_rail_pins_than_the_limit_are_refused() {
        let many = SetRailPins { project_ids: (0..=RAIL_PINS_MAX).map(|_| ProjectId::new()).collect() };
        assert!(matches!(many.valid_pins(), Err(DomainError::OutOfRange { .. })));
    }

    #[test]
    fn a_login_result_never_prints_its_token() {
        let user = UserView { id: UserId::new(), username: "alex".into(), created_at_ms: 1, created_by_user: None, disabled_at_ms: None };
        let login = LoginResult { user, session_token: "TOKEN-SECRET".into(), expires_at_ms: 9, must_change_password: false };
        assert!(!format!("{login:?}").contains("TOKEN-SECRET"));
        assert!(serde_json::to_string(&login).unwrap().contains("TOKEN-SECRET"), "the answer itself carries it once");
    }

    #[test]
    fn usernames_follow_ov_12() {
        assert_eq!(Username::parse("  grace ").unwrap().as_str(), "grace");
        for ok in ["abc", "a.b-c_d", "A1234567890123456789012345678901"] {
            assert!(Username::parse(ok).is_ok(), "{ok}");
        }
        for bad in ["", "ab", "a b c", "élodie", "x@y.z", &"a".repeat(33)] {
            assert!(matches!(Username::parse(bad), Err(DomainError::InvalidUsername(_))), "{bad}");
        }
    }

    #[test]
    fn passwords_have_at_least_twelve_characters() {
        assert!(matches!(check_password("short-pass1"), Err(DomainError::WeakPassword(_))));
        assert!(check_password("twelve-chars").is_ok());
        assert!(check_password(&"x".repeat(PASSWORD_MAX + 1)).is_err());
        // Counted in bytes (SEC1 m7): 1 024 characters of 4 bytes are too long, 256 are not.
        assert!(check_password(&"😀".repeat(PASSWORD_MAX)).is_err());
        assert!(check_password(&"😀".repeat(PASSWORD_MAX / 4)).is_ok());
    }

    #[test]
    fn a_password_of_only_spaces_is_weak() {
        for blank in [" ".repeat(12), "\t".repeat(20), " \n ".repeat(6)] {
            assert!(matches!(check_password(&blank), Err(DomainError::WeakPassword(_))), "{blank:?}");
        }
        assert!(check_password("  twelve chars  ").is_ok());
    }
}
