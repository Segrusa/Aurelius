//! The company (one per server, protocol 4): its name and its default
//! dispatcher. A project may name its own dispatcher (B5).

use crate::ticket::SetDispatcher;
use crate::{DomainError, EmployeeId, ProjectId, require_text};
use serde::{Deserialize, Serialize};

pub const NAME_MAX: usize = 120;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CompanyView {
    pub id: String,
    pub name: String,
    /// Chooses the owner of a started ticket without one, unless its project
    /// names its own. None: such tickets wait in Todo for a board member.
    pub dispatcher_employee_id: Option<EmployeeId>,
    pub version: u64,
}

/// Change the company's name or its dispatcher; left out: unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateCompany {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub dispatcher: Option<SetDispatcher>,
}

/// The company's background jobs (memory consolidation, B6) run on the Claude
/// login of the board member who asks (`enabled`), or on none (`false`, only
/// when it is theirs): without one they do not run. Only the own login can be
/// chosen. Answers `CompanyUpdated`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetBackgroundLogin {
    pub enabled: bool,
}

/// Change a project's settings; left out: unchanged. `dispatcher` with no
/// employee: the company's dispatcher again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateProject {
    pub project_id: ProjectId,
    #[serde(default)]
    pub dispatcher: Option<SetDispatcher>,
}

impl UpdateCompany {
    pub fn valid_name(&self) -> Result<Option<String>, DomainError> {
        self.name.as_deref().map(|n| require_text("name", n, NAME_MAX)).transpose()
    }
}
