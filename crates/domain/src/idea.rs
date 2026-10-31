//! Ideas: a list per project of things that may be worth doing later. The
//! user notes them (and employees too, while they work); an idea becomes a task
//! for an employee when its time has come, or is dropped.

use crate::{DomainError, IdeaId, EmployeeId, ProjectId, optional_text, require_text, task::RunSettings};
use serde::{Deserialize, Serialize};

pub const TITLE_MAX: usize = 160;
pub const NOTE_MAX: usize = 8_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdeaStatus {
    /// Still an idea.
    Open,
    /// Done: it became a task, or the user marked it done.
    Done,
    /// Not doing it.
    Dropped,
}

impl IdeaStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            IdeaStatus::Open => "open",
            IdeaStatus::Done => "done",
            IdeaStatus::Dropped => "dropped",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "open" => Self::Open,
            "done" => Self::Done,
            "dropped" => Self::Dropped,
            _ => return None,
        })
    }
}

/// What an idea is about: something new or better, or something broken.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum IdeaKind {
    #[default]
    Feature,
    Bug,
}

impl IdeaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            IdeaKind::Feature => "feature",
            IdeaKind::Bug => "bug",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "feature" => Self::Feature,
            "bug" => Self::Bug,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddIdea {
    pub project_id: ProjectId,
    pub title: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub kind: IdeaKind,
    /// Files on the server to attach (feature `idea_attachments`): copied into
    /// the project's uploads area, as with a task's `files`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateIdea {
    pub idea_id: IdeaId,
    pub title: String,
    #[serde(default)]
    pub note: Option<String>,
    /// None keeps the idea's kind (older clients).
    #[serde(default)]
    pub kind: Option<IdeaKind>,
    /// Files on the server to attach next to the ones the idea has.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_files: Vec<String>,
    /// Attachments to drop, by their `path` in `IdeaView::attachments`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove_files: Vec<String>,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// Delete an idea for good, with its attachments (feature `idea_attachments`).
/// A task it became stays, with its own copies of the files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteIdea {
    pub idea_id: IdeaId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetIdeaStatus {
    pub idea_id: IdeaId,
    pub status: IdeaStatus,
}

/// Turn an idea into a task for this employee (its title and note become the
/// task's title and goal); the idea is then done and points at the task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StartIdea {
    pub idea_id: IdeaId,
    pub employee_id: EmployeeId,
    /// Mode, model and effort of the task; what is left out follows the employee.
    #[serde(default, skip_serializing_if = "RunSettings::is_empty")]
    pub run: RunSettings,
}

/// A valid title and note. The note keeps its lines and the indentation of its
/// first line: only blank lines around it and trailing spaces go.
pub fn validate(title: &str, note: Option<&str>) -> Result<(String, Option<String>), DomainError> {
    let note = note.map(|n| {
        let n = n.trim_end();
        let blank = n.len() - n.trim_start().len();
        n[..blank].rfind('\n').map_or(n, |i| &n[i + 1..])
    });
    let note = optional_text("note", note, NOTE_MAX)?.and(note.map(str::to_owned));
    Ok((require_text("title", title, TITLE_MAX)?, note))
}

/// At most this many files per idea (as per message or task).
pub fn check_file_count(count: usize) -> Result<(), DomainError> {
    if count > crate::conversation::UPLOADS_MAX {
        return Err(DomainError::File(format!("Too many files: an idea has at most {} attachments.", crate::conversation::UPLOADS_MAX)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idea_needs_a_title_and_a_note_is_optional() {
        assert!(validate("  ", None).is_err());
        assert_eq!(validate(" Dark mode ", Some("  ")).unwrap(), ("Dark mode".to_string(), None));
        assert!(validate("x", Some(&"y".repeat(NOTE_MAX + 1))).is_err());
        assert_eq!(IdeaStatus::parse("dropped"), Some(IdeaStatus::Dropped));
        let add: AddIdea = serde_json::from_value(serde_json::json!({ "project_id": ProjectId::new(), "title": "x" })).unwrap();
        assert_eq!(add.kind, IdeaKind::Feature);
        assert_eq!(serde_json::to_value(IdeaKind::Bug).unwrap(), serde_json::json!("bug"));
        assert_eq!(IdeaKind::parse("bug"), Some(IdeaKind::Bug));
        assert!(add.files.is_empty());
    }

    #[test]
    fn a_note_keeps_its_lines() {
        let note = "\n  \n  - first\n\n  - second  \n\n";
        assert_eq!(validate("x", Some(note)).unwrap().1.as_deref(), Some("  - first\n\n  - second"));
        assert_eq!(validate("x", Some("a\r\nb")).unwrap().1.as_deref(), Some("a\r\nb"));
        assert_eq!(validate("x", Some(" \n\t\n ")).unwrap().1, None);
        assert!(validate("x", Some(&format!("\n\n{}", "y".repeat(NOTE_MAX + 1)))).is_err());
        assert!(check_file_count(crate::conversation::UPLOADS_MAX).is_ok());
        assert!(check_file_count(crate::conversation::UPLOADS_MAX + 1).is_err());
    }

    #[test]
    fn older_clients_update_an_idea_without_files() {
        let id = IdeaId::new();
        let old: UpdateIdea = serde_json::from_value(serde_json::json!({ "idea_id": id, "title": "x" })).unwrap();
        assert!(old.add_files.is_empty() && old.remove_files.is_empty());
        let wire = serde_json::to_value(&old).unwrap();
        assert!(wire.get("add_files").is_none() && wire.get("remove_files").is_none());
    }

    #[test]
    fn an_older_client_starts_an_idea_without_a_run() {
        let (idea_id, employee_id) = (IdeaId::new(), EmployeeId::new());
        let old: StartIdea = serde_json::from_value(serde_json::json!({ "idea_id": idea_id, "employee_id": employee_id })).unwrap();
        assert_eq!(old, StartIdea { idea_id, employee_id, run: RunSettings::default() });
        assert!(serde_json::to_value(&old).unwrap().get("run").is_none());
        let auto = StartIdea { run: RunSettings { mode: Some(crate::autonomy::AgentMode::Auto), ..RunSettings::default() }, ..old };
        assert_eq!(serde_json::to_value(&auto).unwrap()["run"], serde_json::json!({ "mode": "auto" }));
    }
}
