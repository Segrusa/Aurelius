//! Conversation ↔ Company sharing. A share is an explicit, immutable
//! snapshot with a source; it grants exactly that content to one destination
//! and nothing else. Revoking stops all future use, including memories made from it.

use crate::{ConversationId, EmployeeId, MessageId, ProjectId, ShareId, TaskId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShareMode {
    /// Used as context by the destination's agents.
    Context,
    /// Also proposed as a memory (the user still accepts it).
    Memory,
}

impl ShareMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ShareMode::Context => "context",
            ShareMode::Memory => "memory",
        }
    }

    pub fn parse(text: &str) -> Self {
        if text == "memory" { ShareMode::Memory } else { ShareMode::Context }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ShareTarget {
    Project(ProjectId),
    Employee(EmployeeId),
}

/// Share one chat message with a project or employee.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ShareMessage {
    pub conversation_id: ConversationId,
    pub message_id: MessageId,
    pub target: ShareTarget,
    pub mode: ShareMode,
}

/// Discuss a task with its lead employee: the task's result becomes context
/// of the employee's chat (`CommandResult::TaskDiscussed` names it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussTask {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RevokeShare {
    pub share_id: ShareId,
}

pub const MAX_SNAPSHOT_CHARS: usize = 20_000;
