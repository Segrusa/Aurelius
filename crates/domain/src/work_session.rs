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
    /// A design session (feature `design_sessions`): the employee makes HTML pages,
    /// and the app opens the session with its preview.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub design: bool,
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
    /// See `StartWorkSession::design`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub design: bool,
}

/// The board member who started a session chooses who takes it, while the
/// dispatcher still chooses or after it chose nobody (feature `session_dispatch`).
/// The chosen employee answers the first message at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AssignWorkSession {
    pub conversation_id: ConversationId,
    pub employee_id: EmployeeId,
}

/// A board member asks for a session's summary in the History now (feature
/// `session_history`): after it failed, for a session without one, or to bring
/// it up to date. Any board member may, as the History shows every session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RetrySessionSummary {
    pub conversation_id: ConversationId,
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
    /// Named after its first message, until a title made from its content
    /// replaces it after the first answer.
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
    /// A run of it ended after the reader last opened it (with `conversation.get`), so
    /// there is a finished answer they have not seen. Never while it runs; older
    /// servers leave it out, and a client then shows no mark.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unseen: bool,
    /// A design session (feature `design_sessions`); older servers leave it out.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub design: bool,
}

/// `work_session.list`: the reader's sessions, the latest activity first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkSessionsSnapshot {
    pub as_of_seq: u64,
    pub sessions: Vec<WorkSessionView>,
}

/// A made session title is cut at this many characters (as `conversation::derive_title`).
pub const SESSION_TITLE_MAX_CHARS: usize = 60;
/// How much of a session's opening the title job reads (the oldest messages win).
pub const SESSION_TITLE_INPUT_MAX_CHARS: usize = 6_000;
/// One message in the title job's input is cut here.
pub const SESSION_TITLE_MESSAGE_MAX_CHARS: usize = 2_000;

/// The title a model's answer gives a session, or none when it gave nothing usable.
/// A model may wrap the title in quotes, Markdown or a "Title:" label, or add a
/// second line; only the bare first line is kept, cut at `SESSION_TITLE_MAX_CHARS`.
pub fn session_title(answer: &str) -> Option<String> {
    let line = answer.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line.trim_start_matches('#').trim_start();
    let line = match line.split_once(':') {
        Some((label, rest)) if ["title", "titel"].contains(&label.trim().trim_matches('*').to_lowercase().as_str()) => rest,
        _ => line,
    };
    // Not « »: the vault redactor marks a secret with them.
    const WRAP: &[char] = &['"', '\'', '“', '”', '‘', '’', '*', '_', '`'];
    let unwrap = |s: &str| s.trim_matches(|c: char| c.is_whitespace() || WRAP.contains(&c)).to_owned();
    let line = unwrap(unwrap(line).trim_end_matches('.'));
    let line: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return None;
    }
    let mut title: String = line.chars().take(SESSION_TITLE_MAX_CHARS).collect();
    if line.chars().count() > SESSION_TITLE_MAX_CHARS {
        title = title.trim_end().to_owned();
        title.push('…');
    }
    Some(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_title_is_kept_as_it_is() {
        assert_eq!(session_title("Sessietitels laten genereren").as_deref(), Some("Sessietitels laten genereren"));
    }

    #[test]
    fn quotes_markdown_a_label_and_a_second_line_are_taken_off() {
        assert_eq!(session_title("\n\n\"Fix the login flow.\"\nThis title says what was done.").as_deref(), Some("Fix the login flow"));
        assert_eq!(session_title("**Title:** “Faster CSV export”").as_deref(), Some("Faster CSV export"));
        assert_eq!(session_title("# Titel: Sneller exporteren").as_deref(), Some("Sneller exporteren"));
        assert_eq!(session_title("`Release   0.9.3   prep`").as_deref(), Some("Release 0.9.3 prep"));
        assert_eq!(session_title("\"Token «secret:GH_TOKEN».\"").as_deref(), Some("Token «secret:GH_TOKEN»"), "a redacted secret keeps its marks");
    }

    #[test]
    fn a_colon_that_is_no_label_stays() {
        assert_eq!(session_title("Mobile: shorter tab bar").as_deref(), Some("Mobile: shorter tab bar"));
    }

    #[test]
    fn an_empty_answer_gives_no_title() {
        assert_eq!(session_title(""), None);
        assert_eq!(session_title("  \n \"\" \n"), None);
    }

    #[test]
    fn a_long_title_is_cut_with_an_ellipsis() {
        let title = session_title(&"word ".repeat(40)).unwrap();
        assert!(title.ends_with('…'));
        assert_eq!(title.chars().count(), SESSION_TITLE_MAX_CHARS);
    }

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
