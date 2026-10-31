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
    /// A ticket, board or company write needs an active board member on the connection
    /// (`user.use`, `user.create` or `HelloParams::user_id`).
    BoardMemberRequired,
    /// `user.create`: another board member has this name (case does not count).
    UsernameTaken,
    /// `user.create`: 3 to 32 letters, digits, `.`, `_` or `-`.
    InvalidUsername,
    /// `user.create`: the password is shorter than 12 characters.
    WeakPassword,
    /// `user.use`: the board member or the password is wrong (one answer for both).
    WrongPassword,
    /// `user.use`: too many tries on this connection; wait a minute.
    TooManyAttempts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    /// User-facing explanation.
    pub message: String,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}
