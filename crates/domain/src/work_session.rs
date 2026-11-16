//! Work sessions (feature `work_sessions`): as in Claude Desktop, a board member
//! starts work by typing. One employee works in one project's folder in a
//! conversation of its own (kind `session`), without a ticket, triage or review.
//! It works in the project's folder itself, or, when asked, in a worktree of its
//! own on the branch `session/<last 8 of its id>` that the daemon makes before the first
//! run. Later messages go in with `chat.send`; `work_session.list` lists them.

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
}
