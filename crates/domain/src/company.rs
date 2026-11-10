//! The company (one per server, protocol 4): its name, its default
//! dispatcher and its tester (protocol 5, feature `board_testing`). A project
//! may name its own dispatcher (B5).

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
    /// Runs the full test suite once while a card is in Testing, before Review
    /// (feature `board_testing`). None: no Testing step, cards go from Doing to
    /// Review as before. Left out by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tester_employee_id: Option<EmployeeId>,
    pub version: u64,
}

/// Change the company's name, its dispatcher or its tester; left out: unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateCompany {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub dispatcher: Option<SetDispatcher>,
    /// Feature `board_testing`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tester: Option<SetTester>,
}

/// Who runs the full test suite once for a card in Testing (feature
/// `board_testing`). `employee_id` none: no tester, so no Testing step: cards
/// go from Doing to Review as before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetTester {
    #[serde(default)]
    pub employee_id: Option<EmployeeId>,
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
    /// The workspace's styleguide: a Claude Design artifact link
    /// (`ticket::valid_artifact_url`, feature `design_links`); an empty text clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub styleguide_url: Option<String>,
    /// Whether each ticket of this project works in a git worktree and on a branch of
    /// its own (feature `ticket_worktrees`); off: in the project's folder, as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_worktrees: Option<bool>,
    /// How many wakes of employees a ticket of this project may have since a board
    /// member last acted on it (feature `ticket_budget`): 1 to `TICKET_BUDGET_MAX`, or
    /// 0 for no limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_budget: Option<u32>,
}

impl UpdateCompany {
    pub fn valid_name(&self) -> Result<Option<String>, DomainError> {
        self.name.as_deref().map(|n| require_text("name", n, NAME_MAX)).transpose()
    }
}
