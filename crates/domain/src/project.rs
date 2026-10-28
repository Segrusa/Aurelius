//! Project aggregate. A new project is empty: no employees,
//! no repository, no default roles.

use crate::{DomainError, ProjectId, optional_text, require_text};
use serde::{Deserialize, Serialize};

pub const NAME_MAX: usize = 120;
pub const DESCRIPTION_MAX: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    Paused,
    Archived,
}

impl ProjectStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectStatus::Active => "active",
            ProjectStatus::Paused => "paused",
            ProjectStatus::Archived => "archived",
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateProject {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// The project's folder; without one Aurelius creates a default folder.
    #[serde(default)]
    pub folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RenameProject {
    pub project_id: ProjectId,
    pub name: String,
}

/// Change a project's description; an empty (or blank) text clears it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetProjectDescription {
    pub project_id: ProjectId,
    pub description: String,
}

/// Domain events for the project aggregate. The serialized form is the
/// immutable event payload (schema_version 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type")]
pub enum ProjectEvent {
    #[serde(rename = "project.created")]
    Created {
        project_id: ProjectId,
        name: String,
        description: Option<String>,
    },
    #[serde(rename = "project.updated")]
    Renamed { project_id: ProjectId, name: String },
    /// Logged as `project.updated` (clients refresh the project on it).
    #[serde(rename = "project.description_set")]
    DescriptionSet { project_id: ProjectId, description: Option<String> },
}

impl ProjectEvent {
    pub fn type_name(&self) -> &'static str {
        match self {
            ProjectEvent::Created { .. } => "project.created",
            ProjectEvent::Renamed { .. } | ProjectEvent::DescriptionSet { .. } => "project.updated",
        }
    }

    pub fn project_id(&self) -> ProjectId {
        match self {
            ProjectEvent::Created { project_id, .. }
            | ProjectEvent::Renamed { project_id, .. }
            | ProjectEvent::DescriptionSet { project_id, .. } => *project_id,
        }
    }
}

/// Current state folded from events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub description: Option<String>,
    pub status: ProjectStatus,
    pub version: u64,
}

impl CreateProject {
    /// Validate input and decide the single `project.created` event.
    pub fn decide(&self, project_id: ProjectId) -> Result<ProjectEvent, DomainError> {
        Ok(ProjectEvent::Created {
            project_id,
            name: require_text("name", &self.name, NAME_MAX)?,
            description: optional_text("description", self.description.as_deref(), DESCRIPTION_MAX)?,
        })
    }
}

impl RenameProject {
    pub fn decide(&self, current: &Project, expected_version: Option<u64>) -> Result<ProjectEvent, DomainError> {
        if let Some(expected) = expected_version {
            if expected != current.version {
                return Err(DomainError::VersionConflict { expected, actual: current.version });
            }
        }
        if current.status == ProjectStatus::Archived {
            return Err(DomainError::ProjectArchived);
        }
        Ok(ProjectEvent::Renamed {
            project_id: current.id,
            name: require_text("name", &self.name, NAME_MAX)?,
        })
    }
}

impl SetProjectDescription {
    pub fn decide(&self, current: &Project, expected_version: Option<u64>) -> Result<ProjectEvent, DomainError> {
        if let Some(expected) = expected_version {
            if expected != current.version {
                return Err(DomainError::VersionConflict { expected, actual: current.version });
            }
        }
        if current.status == ProjectStatus::Archived {
            return Err(DomainError::ProjectArchived);
        }
        Ok(ProjectEvent::DescriptionSet {
            project_id: current.id,
            description: optional_text("description", Some(&self.description), DESCRIPTION_MAX)?,
        })
    }
}

/// Reducer: fold one event into optional prior state.
pub fn apply(state: Option<Project>, event: &ProjectEvent) -> Option<Project> {
    match (state, event) {
        (None, ProjectEvent::Created { project_id, name, description }) => Some(Project {
            id: *project_id,
            name: name.clone(),
            description: description.clone(),
            status: ProjectStatus::Active,
            version: 1,
        }),
        (Some(mut project), ProjectEvent::Renamed { name, .. }) => {
            project.name = name.clone();
            project.version += 1;
            Some(project)
        }
        (Some(mut project), ProjectEvent::DescriptionSet { description, .. }) => {
            project.description = description.clone();
            project.version += 1;
            Some(project)
        }
        // Invalid sequences are rejected by `decide`; the reducer keeps state unchanged.
        (state, _) => state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_trims_and_requires_name() {
        let id = ProjectId::new();
        let err = CreateProject { name: "   ".into(), description: None, folder: None }.decide(id).unwrap_err();
        assert_eq!(err, DomainError::Required { field: "name".into() });

        let event = CreateProject { name: "  Atlas ".into(), description: Some(" ".into()), folder: None }
            .decide(id)
            .unwrap();
        assert_eq!(event, ProjectEvent::Created { project_id: id, name: "Atlas".into(), description: None });
    }

    #[test]
    fn rename_checks_expected_version_and_archive() {
        let id = ProjectId::new();
        let created = CreateProject { name: "Atlas".into(), description: None, folder: None }.decide(id).unwrap();
        let project = apply(None, &created).unwrap();

        let rename = RenameProject { project_id: id, name: "Atlas 2".into() };
        assert_eq!(
            rename.decide(&project, Some(3)).unwrap_err(),
            DomainError::VersionConflict { expected: 3, actual: 1 }
        );
        let renamed = apply(Some(project.clone()), &rename.decide(&project, Some(1)).unwrap()).unwrap();
        assert_eq!((renamed.name.as_str(), renamed.version), ("Atlas 2", 2));

        let archived = Project { status: ProjectStatus::Archived, ..project };
        assert_eq!(rename.decide(&archived, None).unwrap_err(), DomainError::ProjectArchived);
    }

    #[test]
    fn set_description_trims_clears_limits_and_checks_version() {
        let id = ProjectId::new();
        let created = CreateProject { name: "Atlas".into(), description: Some("Old".into()), folder: None }.decide(id).unwrap();
        let project = apply(None, &created).unwrap();
        let set = |text: &str| SetProjectDescription { project_id: id, description: text.into() };

        assert_eq!(
            set("New").decide(&project, Some(2)).unwrap_err(),
            DomainError::VersionConflict { expected: 2, actual: 1 }
        );
        let changed = apply(Some(project.clone()), &set("  New text 
").decide(&project, Some(1)).unwrap()).unwrap();
        assert_eq!((changed.description.as_deref(), changed.version), (Some("New text"), 2));

        let cleared = apply(Some(changed.clone()), &set("   ").decide(&changed, Some(2)).unwrap()).unwrap();
        assert_eq!((cleared.description, cleared.version), (None, 3));

        assert!(set(&"a".repeat(DESCRIPTION_MAX)).decide(&project, None).is_ok());
        assert_eq!(
            set(&"a".repeat(DESCRIPTION_MAX + 1)).decide(&project, None).unwrap_err(),
            DomainError::TooLong { field: "description".into(), max: DESCRIPTION_MAX }
        );

        let archived = Project { status: ProjectStatus::Archived, ..project };
        assert_eq!(set("x").decide(&archived, None).unwrap_err(), DomainError::ProjectArchived);
    }
}

/// Point a project at another folder on disk (its agents may write there).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetProjectFolder {
    pub project_id: ProjectId,
    pub path: String,
}

/// A safe folder name for a project: no reserved characters, no trailing dots or spaces.
pub fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '-' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']).chars().take(80).collect::<String>();
    if trimmed.is_empty() { "Project".into() } else { trimmed }
}
