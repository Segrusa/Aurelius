//! Per-agent working mode (like Claude Code's permission modes). Applies to
//! a conversation: a chat or an employee (whose tasks follow the employee).

use crate::ConversationId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    /// Only read and plan; no changes. Tasks stop after planning until you approve.
    Plan,
    /// Ask before every change (the default).
    #[default]
    Ask,
    /// File and artifact edits in granted folders go ahead; external actions still ask.
    AcceptEdits,
    /// Everything goes ahead without asking, except what is always forbidden.
    Auto,
}

impl AgentMode {
    pub const ALL: [AgentMode; 4] = [AgentMode::Ask, AgentMode::AcceptEdits, AgentMode::Auto, AgentMode::Plan];

    pub fn as_str(self) -> &'static str {
        match self {
            AgentMode::Plan => "plan",
            AgentMode::Ask => "ask",
            AgentMode::AcceptEdits => "accept_edits",
            AgentMode::Auto => "auto",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "plan" => AgentMode::Plan,
            "accept_edits" => AgentMode::AcceptEdits,
            "auto" => AgentMode::Auto,
            _ => AgentMode::Ask,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AgentMode::Plan => "Plan",
            AgentMode::Ask => "Ask",
            AgentMode::AcceptEdits => "Accept edits",
            AgentMode::Auto => "Auto",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            AgentMode::Plan => "Read and plan only; nothing is changed. Tasks wait for your approval after planning.",
            AgentMode::Ask => "Asks permission for every change and external action.",
            AgentMode::AcceptEdits => "File and artifact edits in granted folders go ahead; external actions still ask.",
            AgentMode::Auto => "Runs everything without asking, except what is always forbidden.",
        }
    }

    /// Shift+Tab order, as in Claude Code.
    pub fn next(self) -> Self {
        match self {
            AgentMode::Ask => AgentMode::AcceptEdits,
            AgentMode::AcceptEdits => AgentMode::Auto,
            AgentMode::Auto => AgentMode::Plan,
            AgentMode::Plan => AgentMode::Ask,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetAgentMode {
    pub conversation_id: ConversationId,
    pub mode: AgentMode,
}

/// The model and effort of one conversation (a thread with an employee); each
/// left unset follows the employee's own settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetConversationRun {
    pub conversation_id: ConversationId,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
}

/// Allow or forbid the agent to use the user's own browser (Claude in Chrome).
/// The browser shares the user's logins, so this is off until the user turns it on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetBrowserAllowed {
    pub conversation_id: ConversationId,
    pub allowed: bool,
}

/// Allow the agent to look at the user's screen (screenshots of the desktop or
/// one window) without asking each time. Off by default: the agent may still
/// ask for a screenshot, and the user decides in the inbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetScreenAllowed {
    pub conversation_id: ConversationId,
    pub allowed: bool,
}
