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
    pub tool_policy: Setting<String>,
    pub autonomy_policy: Setting<String>,
    pub task_policy: TaskPolicy,
    pub runtime_selection: Setting<String>,
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

/// Turn an employee's heartbeat on (`every_minutes` > 0) or off (0). Quiet hours
/// are local minutes since midnight; `None` keeps the defaults (22:00–07:00,
/// 20k tokens a day).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetHeartbeat {
    pub employee_id: EmployeeId,
    pub every_minutes: u32,
    #[serde(default)]
    pub quiet_start_min: Option<u32>,
    #[serde(default)]
    pub quiet_end_min: Option<u32>,
    #[serde(default)]
    pub daily_token_budget: Option<u64>,
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
