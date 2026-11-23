//! What a paired device may do, and how a paired device is shown.

use crate::ClientId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Every query and the event stream.
    Read,
    /// Chats and project conversations: send, retry, stop, attach, share.
    Chat,
    /// Projects, employees, tasks, plans and memory.
    TaskControl,
    /// Decide concrete approvals; switch a conversation to Auto.
    Approve,
    /// A paired computer with full access: connections and keys, folders on
    /// the server, backups, updates and remote access itself.
    Owner,
}

impl Capability {
    pub const ALL: [Capability; 5] = [Capability::Read, Capability::Chat, Capability::TaskControl, Capability::Approve, Capability::Owner];
    /// What a paired phone gets.
    pub const DEVICE_DEFAULT: [Capability; 4] = [Capability::Read, Capability::Chat, Capability::TaskControl, Capability::Approve];
    /// What a computer approved "as owner" gets.
    pub const OWNER: [Capability; 5] = Self::ALL;

    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Read => "read",
            Capability::Chat => "chat",
            Capability::TaskControl => "task_control",
            Capability::Approve => "approve",
            Capability::Owner => "owner",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == text)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RemoteClientView {
    pub id: ClientId,
    pub name: String,
    /// ios | android | windows | macos | linux
    pub platform: String,
    pub capabilities: Vec<Capability>,
    pub created_at_ms: i64,
    pub last_seen_ms: Option<i64>,
    pub connected: bool,
    /// The device registered for push notifications.
    pub push: bool,
    /// The board member who paired it (feature `own_devices`); none: a device paired
    /// before devices had an owner and never signed in on, which only the server's
    /// Remote access settings show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paired_by: Option<crate::UserId>,
}
