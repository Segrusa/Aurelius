//! Human decisions on proposed actions.

use crate::ApprovalId;
use serde::{Deserialize, Serialize};

/// The user's answers to a question a runtime asked (Claude Code's
/// AskUserQuestion), keyed by the question text: the chosen option label,
/// several labels joined by ", ", or the user's own words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AnswerQuestion {
    pub approval_id: ApprovalId,
    pub answers: std::collections::BTreeMap<String, String>,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// What the user decided about a plan Claude Code proposed (ExitPlanMode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanDecision {
    /// Go ahead; the work continues in auto mode.
    Approve,
    /// Go ahead; edits go through, everything else is asked (accept-edits mode).
    ApproveEdits,
    /// Not yet: the agent revises the plan with the user's feedback (required).
    Revise,
    /// No: the agent stops (feedback optional).
    Reject,
}

/// The user's decision on a plan waiting in the inbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DecidePlan {
    pub approval_id: ApprovalId,
    pub decision: PlanDecision,
    #[serde(default)]
    pub feedback: Option<String>,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// The user's decision on exactly one action. `action_digest` must match the
/// digest the user was shown; a changed action is a new request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DecideApproval {
    pub approval_id: ApprovalId,
    pub action_digest: String,
    pub approve: bool,
    /// With `approve`: also allow this MCP tool from now on, in every run (see
    /// `always_allow_eligible`), and approve its other pending asks.
    #[serde(default)]
    pub always_allow: bool,
}

/// Whether 'Always allow' may be offered for a tool: an MCP tool
/// `mcp__<server>__<tool>` of the user's own servers or Claude in Chrome, never
/// Aurelius' own `mcp__aurelius__*` tools. Takes the bare tool name or the
/// approval's (`claude.` prefixed).
pub fn always_allow_eligible(tool: &str) -> bool {
    let tool = tool.strip_prefix("claude.").unwrap_or(tool);
    match tool.strip_prefix("mcp__").and_then(|rest| rest.split_once("__")) {
        Some((server, name)) => !server.is_empty() && !name.is_empty() && server != "aurelius",
        None => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Invalidated,
    Consumed,
}

impl ApprovalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Rejected => "rejected",
            ApprovalStatus::Expired => "expired",
            ApprovalStatus::Invalidated => "invalidated",
            ApprovalStatus::Consumed => "consumed",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "expired" => Some(Self::Expired),
            "invalidated" => Some(Self::Invalidated),
            "consumed" => Some(Self::Consumed),
            _ => None,
        }
    }
}

/// What an action does, for the approval and its audit trail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Read,
    LocalWrite,
    ExternalWrite,
    Execute,
}

impl Effect {
    pub fn as_str(self) -> &'static str {
        match self {
            Effect::Read => "read",
            Effect::LocalWrite => "local_write",
            Effect::ExternalWrite => "external_write",
            Effect::Execute => "execute",
        }
    }
}

/// A policy layer's verdict on an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

/// Layers are intersected: Deny wins over Ask, Ask wins over Allow. An agent
/// can only propose; nothing it produces loosens a layer.
pub fn intersect(layers: impl IntoIterator<Item = Decision>) -> Decision {
    layers.into_iter().max().unwrap_or(Decision::Allow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_beats_ask_beats_allow() {
        assert_eq!(intersect([Decision::Allow, Decision::Ask]), Decision::Ask);
        assert_eq!(intersect([Decision::Ask, Decision::Deny, Decision::Allow]), Decision::Deny);
        assert_eq!(intersect([]), Decision::Allow);
    }

    #[test]
    fn only_mcp_tools_outside_aurelius_can_be_always_allowed() {
        assert!(always_allow_eligible("mcp__github__create_issue") && always_allow_eligible("claude.mcp__claude-in-chrome__navigate"));
        for tool in ["mcp__aurelius__screenshot", "claude.mcp__aurelius__list_windows", "Bash", "claude.Edit", "mcp__x", "mcp____y", "mcp__x__"] {
            assert!(!always_allow_eligible(tool), "{tool}");
        }
    }
}
