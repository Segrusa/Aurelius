//! Work sessions (feature `work_sessions`): as in Claude Desktop, a board member
//! starts work by typing. One employee works in one project's folder in a
//! conversation of its own (kind `session`), without a ticket, triage or review.
//! It works in the project's folder itself, or, when asked, in a worktree of its
//! own on the branch `session/<last 8 of its id>` that the daemon makes before the first
//! run. Later messages go in with `chat.send`; `work_session.list` lists them.
//! With `work_session.dispatch` the dispatcher chooses the employee first.

use crate::task::RunSettings;
use crate::{ConversationId, EmployeeId, ProjectId};
use serde::{Deserialize, Serialize};

/// Start a work session with its first message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StartWorkSession {
    pub project_id: ProjectId,
    pub employee_id: EmployeeId,
    pub text: String,
    /// Images that belong to the first message (see `SendChatMessage::images`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// Files that belong to the first message (feature `chat_files`, see
    /// `SendChatMessage::files`). Not with `worktree`: that folder does not exist yet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// Work in a worktree of its own instead of the project's folder. Needs a
    /// project folder that is a git repository with a main branch.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub worktree: bool,
    /// Mode, model and effort it starts with (empty: the employee's defaults).
    #[serde(default)]
    pub run: RunSettings,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// Start a work session and let the dispatcher choose who takes it (feature
/// `session_dispatch`), as it chooses the owner of a ticket: the same as
/// `StartWorkSession` without an employee. Until it has chosen, the session's
/// employee is the dispatcher and nothing more can be written in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DispatchWorkSession {
    pub project_id: ProjectId,
    pub text: String,
    /// See `StartWorkSession::images`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// See `StartWorkSession::files`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// See `StartWorkSession::worktree`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub worktree: bool,
    /// See `StartWorkSession::run`; it applies to the chosen employee's runs.
    #[serde(default)]
    pub run: RunSettings,
    /// See `StartWorkSession::confirm_secret`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// The board member who started a session chooses who takes it, while the
/// dispatcher still chooses or after it chose nobody (feature `session_dispatch`).
/// The chosen employee answers the first message at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AssignWorkSession {
    pub conversation_id: ConversationId,
    pub employee_id: EmployeeId,
}

/// Where the dispatcher is with a session (`WorkSessionView::dispatch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionDispatch {
    /// Its job runs: the session waits for its choice.
    Choosing,
    /// It chose nobody (or could not): the board member chooses.
    Nobody,
}

impl SessionDispatch {
    /// The value in `work_sessions.dispatch`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Choosing => "choosing",
            Self::Nobody => "nobody",
        }
    }

    /// Read back from `work_sessions.dispatch`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "choosing" => Some(Self::Choosing),
            "nobody" => Some(Self::Nobody),
            _ => None,
        }
    }
}

/// The branch of a session's worktree: `session/` and the last eight characters of its id
/// (the first ones of a UUIDv7 are its time, the same for sessions started together).
pub fn branch_of(conversation: ConversationId) -> String {
    let id = conversation.to_string();
    format!("session/{}", &id[id.len().saturating_sub(8)..])
}

/// One work session in `work_session.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkSessionView {
    pub conversation_id: ConversationId,
    pub project_id: ProjectId,
    pub project_name: String,
    pub employee_id: EmployeeId,
    pub employee_name: String,
    /// Named after its first message.
    pub title: String,
    /// Its own worktree and branch; none when it works in the project's folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub created_at_ms: i64,
    pub last_activity_ms: i64,
    /// Whether a run of it is queued or working now.
    pub running: bool,
    /// The dispatcher still chooses who takes it, or chose nobody (feature
    /// `session_dispatch`); none: `employee_id` works in it. While it chooses,
    /// `employee_id` is the dispatcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch: Option<SessionDispatch>,
}

/// `work_session.list`: the reader's sessions, the latest activity first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkSessionsSnapshot {
    pub as_of_seq: u64,
    pub sessions: Vec<WorkSessionView>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_branch_is_short_and_under_session() {
        let id = ConversationId::new();
        let branch = branch_of(id);
        assert!(branch.starts_with("session/"));
        assert_eq!(branch.len(), "session/".len() + 8);
        assert!(id.to_string().ends_with(&branch["session/".len()..]));
    }

    #[test]
    fn a_start_without_options_reads_with_defaults() {
        let project = ProjectId::new();
        let employee = EmployeeId::new();
        let json = serde_json::json!({ "project_id": project, "employee_id": employee, "text": "fix the login" });
        let start: StartWorkSession = serde_json::from_value(json).unwrap();
        assert!(!start.worktree && start.images.is_empty() && start.run.is_empty() && !start.confirm_secret);
    }

    #[test]
    fn a_dispatch_state_reads_back_what_it_stores() {
        for state in [SessionDispatch::Choosing, SessionDispatch::Nobody] {
            assert_eq!(SessionDispatch::parse(state.as_str()), Some(state));
            assert_eq!(serde_json::to_value(state).unwrap(), serde_json::json!(state.as_str()));
        }
        assert_eq!(SessionDispatch::parse("chosen"), None);
    }
}
