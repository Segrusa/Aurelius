//! Project conversations (protocol 1.18): the user talks with an assistant of
//! the project, not with an employee. The assistant works in the project folder,
//! sees the employees, tasks, ideas and memories of the project and can give
//! work to an employee.

use crate::ProjectId;
use crate::conversation::ChatStart;
use serde::{Deserialize, Serialize};

/// A new, empty project conversation. It is named after its first message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StartProjectConversation {
    pub project_id: ProjectId,
    /// The mode, browser and screen it starts with (default: the standard mode).
    #[serde(default, flatten)]
    pub start: ChatStart,
}

/// A user message in a project conversation; without `conversation_id` a new
/// conversation is made. Messages in the read-only conversations (those of
/// employees from before 1.18) are refused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SendProjectMessage {
    pub project_id: ProjectId,
    pub text: String,
    #[serde(default)]
    pub conversation_id: Option<crate::ConversationId>,
    /// Images that belong to this message (see `SendChatMessage::images`).
    #[serde(default)]
    pub images: Vec<String>,
    /// Files of any type to copy into the project folder (under
    /// `.aurelius/uploads/`): host paths, or this device's upload inbox.
    #[serde(default)]
    pub files: Vec<String>,
    /// "Send now" (see `SendChatMessage::interrupt`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interrupt: bool,
    /// For a new conversation: the mode, browser and screen it starts with.
    /// Ignored for an existing one.
    #[serde(default, flatten)]
    pub start: ChatStart,
}
