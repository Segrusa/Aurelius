//! Attachments.

use crate::{AttachmentId, ConversationId};
use serde::{Deserialize, Serialize};

/// Snapshot a user-picked file into a conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AttachFile {
    pub conversation_id: ConversationId,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RemoveAttachment {
    pub attachment_id: AttachmentId,
}

