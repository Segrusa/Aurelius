//! Employees: user-defined and company-wide; any employee may work in
//! any project. Names are unique in the company (case-insensitive, among those
//! not archived). Roles are free text, never an enum. Removing means archiving;
//! history keeps the actor ID.

use crate::{DomainError, EmployeeId, ProjectId, optional_text, require_text};
use serde::{Deserialize, Serialize};

pub const NAME_MAX: usize = 80;
pub const ROLE_MAX: usize = 120;
pub const TITLE_MAX: usize = 80;
pub const INSTRUCTIONS_MAX: usize = 20_000;
/// The longest description (characters): a line in every colleague's system text,
/// no longer than the first instruction line it replaces there (P7).
pub const DESCRIPTION_MAX: usize = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStatus {
    Active,
    Paused,
    Archived,
}

impl LifecycleStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            LifecycleStatus::Active => "active",
            LifecycleStatus::Paused => "paused",
            LifecycleStatus::Archived => "archived",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

/// What an employee's runs may do with files and commands (C3), next to its mode:
/// the level says which tools a run has, the mode which of those are asked first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccessLevel {
    /// Every work tool, as a run has without a level of its own.
    #[default]
    Inherit,
    /// Reads and searches the folder; no edits, commands or web.
    ReadOnly,
    /// Also changes files; no commands or web.
    ReadEdit,
    /// A level of a newer server this client does not know. A run treats it as read-only.
    #[serde(other)]
    Unknown,
}

impl AccessLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            AccessLevel::Inherit => "inherit",
            AccessLevel::ReadOnly => "read_only",
            AccessLevel::ReadEdit => "read_edit",
            AccessLevel::Unknown => "unknown",
        }
    }

    /// As stored; anything else is `Unknown`, which is the narrowest (a store of a newer version).
    pub fn parse(text: &str) -> Self {
        match text {
            "inherit" => Self::Inherit,
            "read_only" => Self::ReadOnly,
            "read_edit" => Self::ReadEdit,
            _ => Self::Unknown,
        }
    }

    /// Whether this is the default, which the JSON leaves out.
    pub fn is_inherit(&self) -> bool {
        *self == Self::Inherit
    }

    /// A level a command may set.
    pub fn validate(self) -> Result<Self, DomainError> {
        match self {
            Self::Unknown => Err(DomainError::OutOfRange { field: "access_level".into(), detail: "unknown level".into() }),
            level => Ok(level),
        }
    }
}

/// How far an employee may let others work for it, and how it runs its tasks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskPolicy {
    pub delegation_allowed: bool,
    /// How many delegations deep a task may go (a user's task is depth 0).
    pub max_depth: u32,
    /// The employee works on one task at a time; the others wait their turn.
    #[serde(default)]
    pub sequential_tasks: bool,
}

impl Default for TaskPolicy {
    fn default() -> Self {
        Self { delegation_allowed: true, max_depth: 2, sequential_tasks: false }
    }
}

/// What an employee is doing now (U6): the dot in the list. Derived, never stored; a
/// paused employee is in `status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    /// Nothing for it to do.
    #[default]
    Idle,
    /// A run of it is going.
    Working,
    /// It waits for a board member: an approval, or an answer to a question.
    Waiting,
    /// There is work, but nothing runs yet (its turn, a usage limit, the machine).
    Queued,
    /// A state of a newer server this client does not know.
    #[serde(other)]
    Unknown,
}

impl ActivityState {
    /// As computed in storage; anything else is `Unknown`.
    pub fn parse(text: &str) -> Self {
        match text {
            "idle" => Self::Idle,
            "working" => Self::Working,
            "waiting" => Self::Waiting,
            "queued" => Self::Queued,
            _ => Self::Unknown,
        }
    }

    /// Whether this is the default, which the JSON leaves out.
    pub fn is_idle(&self) -> bool {
        *self == Self::Idle
    }
}

/// A field that can inherit, hold a value, or be explicitly disabled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(tag = "mode", content = "value", rename_all = "snake_case")]
pub enum Setting<T> {
    #[default]
    Inherit,
    Value(T),
    Disabled,
}

/// Full employee configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct EmployeeConfig {
    pub identity: String,
    pub memory_policy: Setting<String>,
    /// Which tools and MCP servers it has and which ask first (C2).
    #[serde(default, deserialize_with = "crate::employee_tools::lenient_tool_policy")]
    pub tool_policy: Setting<crate::employee_tools::ToolPolicy>,
    pub autonomy_policy: Setting<String>,
    pub task_policy: TaskPolicy,
    pub runtime_selection: Setting<String>,
    /// The catalogue entry it was hired from (C6); a copy, the catalogue never changes it afterwards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateEmployee {
    pub name: String,
    pub role: String,
    /// The position on the org chart ("Head of engineering"); free text, at most 80 characters.
    #[serde(default)]
    pub title: Option<String>,
    /// Who this employee reports to (not archived); none: a root (the management team).
    #[serde(default)]
    pub reports_to: Option<EmployeeId>,
    #[serde(default)]
    pub instructions: Option<String>,
    /// Optional own workspace folder: the work folder of its chat.
    #[serde(default)]
    pub folder: Option<String>,
    /// What other employees read to know when to hand work to this one (P7).
    /// None or empty: their prompt shows the first line of the instructions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What its runs may do with files and commands (C3); none: `Inherit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_level: Option<AccessLevel>,
    /// Which tools and MCP servers it has and which ask first (C2); none: all of its level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_policy: Option<crate::employee_tools::ToolPolicy>,
    /// A key of `employee.start_profiles` (C6). What this command leaves out comes from there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_profile: Option<String>,
}

/// The longest an employee's scratchpad may be (C7): it is in every prompt.
pub const SCRATCHPAD_MAX: usize = 4_000;

/// `scheduled_message.cancel`: a message an employee scheduled does not go out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CancelScheduledMessage {
    pub message_id: crate::ScheduledMessageId,
}

/// `employee.open_chat`: once the acting board member's lasting chat with this employee.
/// Chatting with an employee is gone: the server refuses it (the command stays on the wire).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OpenEmployeeChat {
    pub employee_id: EmployeeId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateEmployee {
    pub employee_id: EmployeeId,
    pub name: String,
    pub role: String,
    /// `None` leaves the title as it is; an empty text clears it.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub instructions: Option<String>,
    /// Run its tasks one at a time; `None` leaves it as it is.
    #[serde(default)]
    pub sequential_tasks: Option<bool>,
    /// What other employees read about this one (P7). `None` leaves it as it is;
    /// an empty text clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What its runs may do with files and commands (C3). `None` leaves it as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_level: Option<AccessLevel>,
    /// Which tools and MCP servers it has and which ask first (C2). `None` leaves it
    /// as it is; an empty policy is all of its level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_policy: Option<crate::employee_tools::ToolPolicy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetEmployeeStatus {
    pub employee_id: EmployeeId,
    pub status: LifecycleStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetProjectStatus {
    pub project_id: ProjectId,
    pub status: LifecycleStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SendEmployeeMessage {
    pub employee_id: EmployeeId,
    pub text: String,
    /// The conversation the message goes to: one of its tasks' conversations,
    /// or its persistent chat (`EmployeeSummary::conversation_id`).
    pub conversation_id: crate::ConversationId,
    /// Images that belong to this message (see `SendChatMessage::images`).
    #[serde(default)]
    pub images: Vec<String>,
    /// Files of any type to copy into the conversation's work folder (under
    /// `.aurelius/uploads/`): host paths, or this device's upload inbox.
    #[serde(default)]
    pub files: Vec<String>,
    /// "Send now" (see `SendChatMessage::interrupt`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interrupt: bool,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// A title as stored: trimmed, empty is none.
pub fn validate_title(title: Option<&str>) -> Result<Option<String>, DomainError> {
    optional_text("title", title, TITLE_MAX)
}

/// A description as stored: trimmed, on one line, empty is none.
pub fn validate_description(description: Option<&str>) -> Result<Option<String>, DomainError> {
    let flat = description.map(|d| d.split_whitespace().collect::<Vec<_>>().join(" "));
    optional_text("description", flat.as_deref(), DESCRIPTION_MAX)
}

/// Validated employee fields: (name, role, instructions).
pub fn validate_fields(name: &str, role: &str, instructions: Option<&str>) -> Result<(String, String, Option<String>), DomainError> {
    Ok((
        require_text("name", name, NAME_MAX)?,
        require_text("role", role, ROLE_MAX)?,
        optional_text("instructions", instructions, INSTRUCTIONS_MAX)?,
    ))
}

/// Allowed lifecycle transitions. Archived is final in v1 (history is kept).
pub fn check_transition(from: LifecycleStatus, to: LifecycleStatus) -> Result<(), DomainError> {
    match (from, to) {
        (LifecycleStatus::Archived, _) => Err(DomainError::ProjectArchived),
        (a, b) if a == b => Err(DomainError::OutOfRange { field: "status".into(), detail: format!("already {}", a.as_str()) }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_is_free_text_but_required() {
        assert!(validate_fields("Alex", "Productpartner", None).is_ok());
        assert!(validate_fields("Alex", "  ", None).is_err());
        let (_, role, _) = validate_fields("Iris", "Anything, freely chosen", None).unwrap();
        assert_eq!(role, "Anything, freely chosen");
    }

    #[test]
    fn archived_is_final() {
        assert!(check_transition(LifecycleStatus::Active, LifecycleStatus::Paused).is_ok());
        assert!(check_transition(LifecycleStatus::Paused, LifecycleStatus::Active).is_ok());
        assert!(check_transition(LifecycleStatus::Archived, LifecycleStatus::Active).is_err());
        assert!(check_transition(LifecycleStatus::Active, LifecycleStatus::Active).is_err());
    }

    #[test]
    fn task_policy_defaults() {
        let p = TaskPolicy::default();
        assert_eq!((p.delegation_allowed, p.max_depth, p.sequential_tasks), (true, 2, false));
    }

    #[test]
    fn titles_are_short_free_text() {
        assert_eq!(validate_title(Some("  Head of engineering ")).unwrap().as_deref(), Some("Head of engineering"));
        assert_eq!(validate_title(Some("  ")).unwrap(), None);
        assert!(validate_title(Some(&"x".repeat(TITLE_MAX + 1))).is_err());
    }
}

/// Give an employee its own workspace folder (its chat works there), or `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetEmployeeFolder {
    pub employee_id: EmployeeId,
    pub path: Option<String>,
}

/// Say who an employee reports to, or make it a root (`None`). Anyone in the
/// company who is not archived, never a cycle, at most `org::MAX_CHART_DEPTH` levels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetEmployeeReportsTo {
    pub employee_id: EmployeeId,
    #[serde(default)]
    pub reports_to: Option<EmployeeId>,
}

/// The server setting "Follow the org chart" (on by default): delegation stays
/// in the employee's own subtree, hand-overs go to the boss or to peers, and
/// "Needs you" goes to the boss first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetFollowOrgChart {
    pub follow: bool,
}
