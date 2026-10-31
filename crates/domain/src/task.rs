//! Tasks. A task is worked on in a conversation of its employee;
//! the last answer of that conversation is the report.

use crate::{DomainError, EmployeeId, ProjectId, TaskId, optional_text, require_text};
use serde::{Deserialize, Serialize};

pub const TITLE_MAX: usize = 160;
pub const GOAL_MAX: usize = 20_000;
pub const MAX_CRITERIA: usize = 12;
/// `waiting_reason` of a ticket's main task whose plan waits for a board member.
pub const WAITING_PLAN_REVIEW: &str = "plan_review";
/// `waiting_reason` of a run whose board member has no working Claude login
/// (none, expired, or disabled): it waits, never on another login (protocol 5).
pub const WAITING_LOGIN: &str = "login";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Draft,
    Queued,
    Running,
    Waiting,
    Review,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Draft => "draft",
            TaskStatus::Queued => "queued",
            TaskStatus::Running => "running",
            TaskStatus::Waiting => "waiting",
            TaskStatus::Review => "review",
            TaskStatus::Succeeded => "succeeded",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "draft" => Self::Draft,
            "queued" => Self::Queued,
            "running" => Self::Running,
            "waiting" => Self::Waiting,
            "review" => Self::Review,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// Allowed task transitions. Terminal tasks never change;
/// reopening creates a new linked task. Why it waits (`waiting_reason`, such
/// as `plan_review`) is the engine's business, not the matrix's.
pub fn can_transition(from: TaskStatus, to: TaskStatus) -> bool {
    use TaskStatus::*;
    if from.is_terminal() {
        return false;
    }
    matches!(
        (from, to),
        (Draft, Queued)
            | (Queued, Running)
            | (Running, Review)
            // The report after waiting for colleagues.
            | (Waiting, Review)
            | (Review, Succeeded)
            | (Review, Queued)
            // Handed over during its run: queued for the new employee.
            | (Running, Queued)
            | (Queued | Running | Review, Waiting)
            | (Waiting, Queued)
            // No employee any more (a draft again): nothing runs.
            | (Queued | Waiting, Draft)
            | (_, Failed)
            | (_, Cancelled)
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateTask {
    pub project_id: ProjectId,
    /// Without an employee the task stays a draft.
    #[serde(default)]
    pub employee_id: Option<EmployeeId>,
    pub title: String,
    pub goal: String,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    /// Tasks that must succeed first.
    #[serde(default)]
    pub depends_on: Vec<TaskId>,
    /// How the task's conversation runs; unset fields follow the employee.
    #[serde(default, skip_serializing_if = "RunSettings::is_empty")]
    pub run: RunSettings,
    /// Files of any type (images too) to copy into the work folder's
    /// git-ignored uploads area: host paths, or this device's upload inbox
    /// (protocol 1.14; at most 8, 25 MB each).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

// One struct for what a run starts with: the browser and screen of a new chat
// come with `ChatStart`. On the wire a task's `run` is the schema `TaskRun`.
/// The mode, connection, model and effort a task's conversation runs with.
/// Each field left out follows the employee's own settings.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(rename = "TaskRun")]
pub struct RunSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<crate::autonomy::AgentMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<crate::ProviderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Try the task in this many ways first (1 to 3; feature `task_variants`, when
    /// the server's setting is on and the work folder is a git repository).
    /// Left out or 1: no variants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variants: Option<u8>,
    // Never sent with a task; only a new chat's start fills them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[schemars(skip)]
    pub browser_allowed: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[schemars(skip)]
    pub screen_allowed: bool,
}

impl RunSettings {
    pub fn is_empty(&self) -> bool {
        *self == RunSettings::default()
    }

    /// Turns on something that needs the Approve right on its own command
    /// (Auto mode, the browser, the screen).
    pub fn needs_approve(&self) -> bool {
        self.mode == Some(crate::autonomy::AgentMode::Auto) || self.browser_allowed || self.screen_allowed
    }

    /// Trimmed, with an empty model or effort as unset.
    pub fn validated(&self) -> Result<RunSettings, DomainError> {
        let model = self.model.as_deref().map(str::trim).filter(|m| !m.is_empty()).map(str::to_owned);
        if model.as_ref().is_some_and(|m| m.len() > 200) {
            return Err(DomainError::OutOfRange { field: "model".into(), detail: "at most 200 characters".into() });
        }
        let effort = self.effort.as_deref().map(str::trim).filter(|e| !e.is_empty()).map(str::to_owned);
        if effort.as_ref().is_some_and(|e| !crate::connection::EFFORT_LEVELS.contains(&e.as_str())) {
            return Err(DomainError::OutOfRange { field: "effort".into(), detail: crate::connection::EFFORT_LEVELS.join(", ") });
        }
        let variants = self.variants.map(crate::variants::validate_count).transpose()?.filter(|n| *n > 1);
        Ok(RunSettings { mode: self.mode, connection_id: self.connection_id, model, effort, variants, browser_allowed: self.browser_allowed, screen_allowed: self.screen_allowed })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CancelTask {
    pub task_id: TaskId,
}

/// The user gives a task to an employee (or takes it back: a draft). Allowed
/// while nothing runs: draft, queued or waiting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AssignTask {
    pub task_id: TaskId,
    #[serde(default)]
    pub employee_id: Option<EmployeeId>,
}

/// Validated task fields.
pub struct ValidTask {
    pub title: String,
    pub goal: String,
    pub criteria: Vec<String>,
}

pub fn validate(cmd: &CreateTask) -> Result<ValidTask, DomainError> {
    cmd.run.validated()?;
    let title = require_text("title", &cmd.title, TITLE_MAX)?;
    let goal = optional_text("goal", Some(&cmd.goal), GOAL_MAX)?.unwrap_or_else(|| title.clone());
    let criteria: Vec<String> = cmd.acceptance_criteria.iter().map(|c| c.trim().to_owned()).filter(|c| !c.is_empty()).collect();
    if criteria.len() > MAX_CRITERIA {
        return Err(DomainError::OutOfRange { field: "acceptance_criteria".into(), detail: format!("at most {MAX_CRITERIA}") });
    }
    Ok(ValidTask { title, goal, criteria })
}

/// True if adding `task → requires` would create a cycle, given existing edges.
pub fn creates_cycle(edges: &[(TaskId, TaskId)], task: TaskId, requires: TaskId) -> bool {
    if task == requires {
        return true;
    }
    // Is `task` reachable from `requires`?
    let mut stack = vec![requires];
    let mut seen = std::collections::HashSet::new();
    while let Some(node) = stack.pop() {
        if node == task {
            return true;
        }
        if seen.insert(node) {
            stack.extend(edges.iter().filter(|(from, _)| *from == node).map(|(_, to)| *to));
        }
    }
    false
}

/// Most hand-overs of one task (no ping-pong between employees).
pub const MAX_HANDOFFS: u32 = 2;

/// Extract the first JSON object from model text (models may wrap JSON in prose or fences).
pub fn extract_json<T: serde::de::DeserializeOwned>(text: &str) -> Option<T> {
    let bytes: Vec<char> = text.chars().collect();
    let mut start = None;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in bytes.iter().enumerate() {
        if in_string {
            match (escaped, ch) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' if start.is_some() => in_string = true,
            '{' => {
                if start.is_none() {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' if start.is_some() => {
                depth -= 1;
                if depth == 0 {
                    let candidate: String = bytes[start.unwrap()..=i].iter().collect();
                    if let Ok(value) = serde_json::from_str(&candidate) {
                        return Some(value);
                    }
                    start = None;
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_tasks_never_change_and_review_is_required() {
        use TaskStatus::*;
        assert!(can_transition(Running, Review));
        assert!(!can_transition(Running, Succeeded), "success only via review");
        assert!(can_transition(Review, Queued), "revision goes back to queued");
        assert!(!can_transition(Succeeded, Queued));
        assert!(!can_transition(Failed, Cancelled));
    }

    #[test]
    fn the_matrix_covers_the_engine_and_nothing_more() {
        use TaskStatus::*;
        let all = [Draft, Queued, Running, Waiting, Review, Succeeded, Failed, Cancelled];
        let allowed = [
            (Draft, Queued),
            (Queued, Running),
            (Running, Review),
            (Waiting, Review),
            (Review, Succeeded),
            (Review, Queued),
            (Running, Queued),
            (Queued, Waiting),
            (Running, Waiting),
            (Review, Waiting),
            (Waiting, Queued),
            (Queued, Draft),
            (Waiting, Draft),
        ];
        for from in all {
            for to in all {
                let expected = !from.is_terminal() && (matches!(to, Failed | Cancelled) || allowed.contains(&(from, to)));
                assert_eq!(can_transition(from, to), expected, "{from:?} → {to:?}");
            }
        }
        // The review gate: success only through Review, a draft never runs or reports directly.
        assert!(!can_transition(Waiting, Succeeded) && !can_transition(Draft, Running) && !can_transition(Draft, Review));
        assert!(!can_transition(Running, Draft) && !can_transition(Review, Draft), "a working or reported task is not a draft again");
    }

    #[test]
    fn dependency_cycles_are_detected() {
        let (a, b, c) = (TaskId::new(), TaskId::new(), TaskId::new());
        let edges = vec![(a, b), (b, c)];
        assert!(creates_cycle(&edges, c, a));
        assert!(!creates_cycle(&edges, a, c));
        assert!(creates_cycle(&edges, a, a));
    }

    #[test]
    fn json_is_extracted_from_prose_and_fences() {
        let text = "Here it is:
```json
{\"a\":[{\"name\":\"A\",\"brief\":\"zoek {iets}\"}]}
```
Done.";
        let value: serde_json::Value = extract_json(text).unwrap();
        assert_eq!(value["a"][0]["brief"], "zoek {iets}");
        assert!(extract_json::<serde_json::Value>("no json").is_none());
    }
}
