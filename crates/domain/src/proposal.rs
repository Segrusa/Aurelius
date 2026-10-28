//! Proposals: an employee suggests a new employee (`propose_colleague`) or better
//! instructions for itself or someone who reports to it (`propose_instructions`,
//! Harness² B); the user decides. A proposal always needs the user, in every
//! mode including Auto. Each employee has at most two colleague proposals open,
//! and each employee's instructions at most one proposal.

use crate::{DomainError, EmployeeId, ProposalId, employee};
use serde::{Deserialize, Serialize};

/// Most open (pending) colleague proposals of one employee.
pub const MAX_OPEN_PER_EMPLOYEE: usize = 2;
/// Most open instruction proposals for one employee's instructions.
pub const MAX_OPEN_PER_TARGET: usize = 1;
pub const REASON_MAX: usize = 2_000;
pub const NOTE_MAX: usize = 2_000;
/// At most this many references as a proposal's evidence.
pub const EVIDENCE_MAX: usize = 8;

/// What a proposal is about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    /// A new colleague (`propose_colleague`).
    #[default]
    Colleague,
    /// New instructions for an existing employee (`propose_instructions`).
    Instructions,
}

impl ProposalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProposalKind::Colleague => "colleague",
            ProposalKind::Instructions => "instructions",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "colleague" => Self::Colleague,
            "instructions" => Self::Instructions,
            _ => return None,
        })
    }
}

/// One reference an instruction proposal rests on: a task or a memory (a lesson).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposalEvidence {
    /// `task` | `memory`
    pub kind: String,
    /// The task's or the memory's id.
    pub id: String,
    /// The task's title or the memory's text (shortened), as the user sees it.
    pub label: String,
}

/// The references an instruction proposal gives: trimmed, without doubles or
/// empty ones, at least one and at most `EVIDENCE_MAX`. Whether each names a
/// real task or memory is checked where the data is.
pub fn validate_evidence(refs: &[String]) -> Result<Vec<String>, DomainError> {
    let mut out: Vec<String> = Vec::new();
    for r in refs.iter().map(|r| r.trim()).filter(|r| !r.is_empty()) {
        if !out.iter().any(|o| o == r) {
            out.push(r.to_owned());
        }
    }
    if out.is_empty() {
        return Err(DomainError::Required { field: "evidence (a task or a memory the proposal rests on)".into() });
    }
    if out.len() > EVIDENCE_MAX {
        return Err(DomainError::OutOfRange { field: "evidence".into(), detail: format!("at most {EVIDENCE_MAX} references") });
    }
    Ok(out)
}

/// Proposed or edited instructions: checked like an employee's own.
pub fn validate_instructions(text: &str) -> Result<String, DomainError> {
    let (_, _, instructions) = employee::validate_fields("x", "x", Some(text))?;
    instructions.ok_or_else(|| DomainError::Required { field: "instructions".into() })
}

/// Put an earlier version of an employee's instructions back. It becomes a new
/// version (history is never rewritten), and the employee's conversations start
/// a new session with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RestoreEmployeeInstructions {
    pub employee_id: EmployeeId,
    /// The version to put back (`EmployeeInstructionVersionView::version`).
    pub version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Pending,
    Approved,
    Rejected,
    /// The proposer was archived before the user decided.
    Withdrawn,
}

impl ProposalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ProposalStatus::Pending => "pending",
            ProposalStatus::Approved => "approved",
            ProposalStatus::Rejected => "rejected",
            ProposalStatus::Withdrawn => "withdrawn",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "pending" => Self::Pending,
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            "withdrawn" => Self::Withdrawn,
            _ => return None,
        })
    }
}

/// The employee that would be created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposalFields {
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub title: Option<String>,
    pub instructions: String,
    /// Who the new employee reports to; the proposer unless said otherwise.
    #[serde(default)]
    pub reports_to: Option<EmployeeId>,
    /// The department it joins; the proposer's unless said otherwise.
    #[serde(default)]
    pub department_id: Option<crate::DepartmentId>,
}

impl ProposalFields {
    /// Trimmed and checked with the rules of an employee.
    pub fn validated(&self) -> Result<ProposalFields, DomainError> {
        let (name, role, instructions) = employee::validate_fields(&self.name, &self.role, Some(&self.instructions))?;
        let instructions = instructions.ok_or_else(|| DomainError::Required { field: "instructions".into() })?;
        Ok(ProposalFields { name, role, title: employee::validate_title(self.title.as_deref())?, instructions, reports_to: self.reports_to, department_id: self.department_id })
    }
}

/// The user's decision. `edits` replaces the proposed fields of a colleague
/// proposal (approve and adjust); `instructions` replaces the proposed text of
/// an instruction proposal. `note` goes back to the proposer, usually with a
/// rejection. Approving an instruction proposal whose base version is no longer
/// the employee's current one is refused (`version_conflict`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DecideProposal {
    pub proposal_id: ProposalId,
    pub approve: bool,
    #[serde(default)]
    pub edits: Option<ProposalFields>,
    #[serde(default)]
    pub note: Option<String>,
    /// Instruction proposals: the text to apply instead of the proposed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

pub fn validate_reason(reason: &str) -> Result<String, DomainError> {
    crate::require_text("reason", reason, REASON_MAX)
}

pub fn validate_note(note: Option<&str>) -> Result<Option<String>, DomainError> {
    crate::optional_text("note", note, NOTE_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> ProposalFields {
        ProposalFields { name: " Eva ".into(), role: "Designer".into(), title: Some(" UI lead ".into()), instructions: "Design the screens.".into(), reports_to: None, department_id: None }
    }

    #[test]
    fn fields_are_trimmed_and_checked_like_an_employee() {
        let ok = fields().validated().unwrap();
        assert_eq!((ok.name.as_str(), ok.title.as_deref()), ("Eva", Some("UI lead")));
        assert!(ProposalFields { name: " ".into(), ..fields() }.validated().is_err());
        assert!(ProposalFields { instructions: " ".into(), ..fields() }.validated().is_err());
        assert!(ProposalFields { title: Some("x".repeat(employee::TITLE_MAX + 1)), ..fields() }.validated().is_err());
    }

    #[test]
    fn evidence_needs_one_reference_and_kinds_round_trip() {
        assert!(validate_evidence(&[]).is_err());
        assert!(validate_evidence(&[" ".into()]).is_err());
        assert_eq!(validate_evidence(&[" a ".into(), "a".into(), "b".into()]).unwrap(), vec!["a".to_string(), "b".to_string()]);
        assert!(validate_evidence(&(0..9).map(|n| n.to_string()).collect::<Vec<_>>()).is_err());
        for k in [ProposalKind::Colleague, ProposalKind::Instructions] {
            assert_eq!(ProposalKind::parse(k.as_str()), Some(k));
        }
        assert!(validate_instructions("  ").is_err());
        let old: DecideProposal = serde_json::from_value(serde_json::json!({ "proposal_id": ProposalId::new(), "approve": true })).unwrap();
        assert_eq!(old.instructions, None, "older clients send no instructions");
    }

    #[test]
    fn status_round_trips() {
        for s in [ProposalStatus::Pending, ProposalStatus::Approved, ProposalStatus::Rejected, ProposalStatus::Withdrawn] {
            assert_eq!(ProposalStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(MAX_OPEN_PER_EMPLOYEE, 2);
    }
}
