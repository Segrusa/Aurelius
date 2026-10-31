//! Errors that cross the application boundary. The protocol sends them as they are.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    FrameTooLarge,
    IncompatibleProtocol,
    Validation,
    NotFound,
    VersionConflict,
    IdempotencyConflict,
    StorageUnavailable,
    ShuttingDown,
    Internal,
    /// The caller lacks the right for this method.
    Forbidden,
    /// The server does not know this method (an older server); the connection stays open.
    UnknownMethod,
    /// A ticket, board or company write needs an active board member on the
    /// connection (a session: `user.login`, the first `user.create`, or
    /// `HelloParams::session_token`). The second lock behind `unauthenticated`.
    BoardMemberRequired,
    /// `user.create`: another board member has this name (case does not count).
    UsernameTaken,
    /// `user.create`: 3 to 32 letters, digits, `.`, `_` or `-`.
    InvalidUsername,
    /// `user.create` / `user.set_password`: the password is shorter than 12 characters.
    WeakPassword,
    /// `user.login`: the username or the password is wrong (one answer for both,
    /// also for a disabled board member); `user.set_password`: the old password is wrong.
    WrongPassword,
    /// `user.login` / `user.create`: too many tries; `AppError::retry_after_ms`
    /// says when the next one is taken ("Too many attempts. Try again in 0:30.").
    TooManyAttempts,
    /// Protocol 5: this connection has no (valid) session. Everything but
    /// `session.hello`, `user.whoami`, `user.login`, `user.list` and the first
    /// `user.create` answers this; the client shows its sign-in screen.
    Unauthenticated,
    /// The session's board member signed in with a temporary password (set by
    /// another board member): only `user.set_password` (own), `user.logout`,
    /// `user.whoami` and `session.hello` work until it is changed.
    PasswordChangeRequired,
    /// `user.disable`: the last active board member cannot be disabled.
    LastBoardMember,
    /// `vault.update`: a secret stays a secret and nothing becomes one (B7);
    /// make a new item and archive the old one. Text and url may trade places.
    VaultKindLocked,
    /// `vault.reveal` / `vault.key_export`: the server's vault key is missing or
    /// does not fit this value (a restore without the key, MU4-04 AC3).
    VaultUndecryptable,
    /// A board member's message (an employee's chat, `ticket.post`, the Team
    /// chat) holds the value of a vault secret (B10): nothing was stored.
    /// `AppError::matched_items` names the items (never a value); send again
    /// with `confirm_secret` to store it with the value redacted.
    LooksLikeSecret,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    /// User-facing explanation.
    pub message: String,
    /// `too_many_attempts`: how long until the next try is taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    /// `looks_like_secret`: the names of the vault items the text holds the value of.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_items: Vec<String>,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), retry_after_ms: None, matched_items: Vec::new() }
    }

    /// `looks_like_secret` for the named items ("This looks like a vault secret: GitHub token. Send anyway?").
    pub fn looks_like_secret(matched_items: Vec<String>) -> Self {
        let message = format!("This looks like a vault secret: {}. Send anyway?", matched_items.join(", "));
        Self { code: ErrorCode::LooksLikeSecret, message, retry_after_ms: None, matched_items }
    }

    /// `too_many_attempts` with the wait until the next try.
    pub fn too_many_attempts(retry_after_ms: u64) -> Self {
        let secs = retry_after_ms.div_ceil(1_000);
        Self { code: ErrorCode::TooManyAttempts, message: format!("Too many attempts. Try again in {}:{:02}.", secs / 60, secs % 60), retry_after_ms: Some(retry_after_ms), matched_items: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn too_many_attempts_says_when_to_try_again() {
        let err = AppError::too_many_attempts(29_400);
        assert_eq!(err.message, "Too many attempts. Try again in 0:30.");
        assert_eq!(serde_json::to_value(&err).unwrap(), serde_json::json!({ "code": "too_many_attempts", "message": err.message, "retry_after_ms": 29_400 }));
        assert_eq!(AppError::too_many_attempts(15 * 60_000).message, "Too many attempts. Try again in 15:00.");
        // Other errors leave it out, and an error without it still parses.
        assert!(serde_json::to_value(AppError::new(ErrorCode::Unauthenticated, "Sign in.")).unwrap().get("retry_after_ms").is_none());
        let parsed: AppError = serde_json::from_value(serde_json::json!({ "code": "unauthenticated", "message": "Sign in." })).unwrap();
        assert_eq!(parsed.retry_after_ms, None);
    }

    #[test]
    fn looks_like_secret_names_the_items_only() {
        let err = AppError::looks_like_secret(vec!["GitHub token".into()]);
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "looks_like_secret");
        assert_eq!(json["matched_items"], serde_json::json!(["GitHub token"]));
        assert!(serde_json::to_value(AppError::new(ErrorCode::Validation, "x")).unwrap().get("matched_items").is_none());
        assert_eq!(serde_json::to_value(ErrorCode::VaultKindLocked).unwrap(), "vault_kind_locked");
    }
}
