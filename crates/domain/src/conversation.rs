//! Conversations and messages. Every conversation belongs to
//! a project, a task or an employee (3.1: standalone chats are gone). Messages are durable; streamed deltas are not.

use crate::{ConversationId, DomainError, MessageId, RunId, optional_text, require_text};
use serde::{Deserialize, Serialize};

pub const MESSAGE_MAX: usize = 100_000;
pub const TITLE_MAX: usize = 120;
/// Most images with one message.
pub const IMAGES_MAX: usize = 8;
/// Most files uploaded with one message to an employee.
pub const UPLOADS_MAX: usize = 8;
/// Largest uploaded file (the same as the remote upload limit).
pub const UPLOAD_MAX_BYTES: u64 = 25 * 1024 * 1024;

/// What a conversation is for. Stored as `conversations.kind`; never on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationKind {
    /// A conversation of a project's assistant (no employee).
    Project,
    /// An employee's conversation about one task (project and employee).
    Task,
    /// An employee's chat with the user, outside projects (company scope).
    Employee,
}

impl ConversationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ConversationKind::Project => "project",
            ConversationKind::Task => "task",
            ConversationKind::Employee => "employee",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "project" => Some(Self::Project),
            "task" => Some(Self::Task),
            "employee" => Some(Self::Employee),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Partial,
    Complete,
    Interrupted,
    Failed,
}

impl MessageStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageStatus::Partial => "partial",
            MessageStatus::Complete => "complete",
            MessageStatus::Interrupted => "interrupted",
            MessageStatus::Failed => "failed",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "partial" => Some(Self::Partial),
            "complete" => Some(Self::Complete),
            "interrupted" => Some(Self::Interrupted),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Send a user message in an existing project conversation (protocol 3.1:
/// there are no standalone chats any more, so the conversation is required).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SendChatMessage {
    pub conversation_id: ConversationId,
    pub text: String,
    /// Images that belong to this message: files on the server (a remote
    /// device's own uploads), kept with the message.
    #[serde(default)]
    pub images: Vec<String>,
    /// "Send now": stop the conversation's running (and queued) answers in
    /// the same step, so this message is answered right away.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interrupt: bool,
}

/// The wire form of what a new project conversation starts with (see
/// `StartProjectConversation::start`); the server works with `RunSettings`. Each is
/// off/default when left out, so older clients and the command hash are unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChatStart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_mode: Option<crate::autonomy::AgentMode>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub browser_allowed: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub screen_allowed: bool,
}

impl From<ChatStart> for crate::task::RunSettings {
    fn from(start: ChatStart) -> Self {
        Self { mode: start.agent_mode, browser_allowed: start.browser_allowed, screen_allowed: start.screen_allowed, ..Self::default() }
    }
}

impl ChatStart {
    pub fn needs_approve(&self) -> bool {
        crate::task::RunSettings::from(self.clone()).needs_approve()
    }
}

/// Ask for a new answer version; the previous answer is kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RetryReply {
    pub conversation_id: ConversationId,
    pub user_message_id: MessageId,
}

/// Delete a conversation: it disappears and its content (messages, attachments,
/// shared context) is erased. Employee conversations are archived with the employee.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteConversation {
    pub conversation_id: ConversationId,
}

/// Stop one run. Never affects other runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StopRun {
    pub run_id: RunId,
}

/// Delete a user message that still waits for its answer (1.9): its queued
/// run is cancelled and the message is removed. Refused once that run started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteQueuedMessage {
    pub conversation_id: ConversationId,
    pub message_id: MessageId,
}

/// "Send now" for a user message that still waits (1.9): the answers running
/// or queued ahead of it stop and its own answer starts at once; the messages
/// queued after it keep their place. Refused once that run started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SendQueuedNow {
    pub conversation_id: ConversationId,
    pub message_id: MessageId,
}

impl SendChatMessage {
    pub fn validate(&self) -> Result<String, DomainError> {
        validate_message(&self.text, &self.images)
    }
}

/// A message's text; it may be empty when the message carries images.
pub fn validate_message(text: &str, images: &[String]) -> Result<String, DomainError> {
    if images.len() > IMAGES_MAX {
        return Err(DomainError::OutOfRange { field: "images".into(), detail: format!("at most {IMAGES_MAX}") });
    }
    if images.iter().any(|p| p.trim().is_empty()) {
        return Err(DomainError::Required { field: "images".into() });
    }
    if images.is_empty() {
        return require_text("text", text, MESSAGE_MAX);
    }
    Ok(optional_text("text", Some(text), MESSAGE_MAX)?.unwrap_or_default())
}

/// An employee message's text; it may be empty when the message carries images or files.
pub fn validate_employee_message(text: &str, images: &[String], files: &[String]) -> Result<String, DomainError> {
    if files.len() > UPLOADS_MAX {
        return Err(DomainError::File(format!("Too many files: send at most {UPLOADS_MAX} with one message.")));
    }
    if files.iter().any(|p| p.trim().is_empty()) {
        return Err(DomainError::Required { field: "files".into() });
    }
    match validate_message(text, images) {
        Err(DomainError::Required { field }) if field == "text" && !files.is_empty() => Ok(String::new()),
        other => other,
    }
}

/// Title for a new conversation: first line of the first message, shortened.
pub fn derive_title(text: &str) -> String {
    let first = text.lines().next().unwrap_or("").trim();
    let mut title: String = first.chars().take(60).collect();
    if first.chars().count() > 60 {
        title.push('…');
    }
    optional_text("title", Some(&title), TITLE_MAX).ok().flatten().unwrap_or_else(|| "New chat".into())
}

/// What an uploaded file belongs to: its list of files holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadOwner {
    Message(MessageId),
    Task(crate::TaskId),
    Schedule(crate::TaskScheduleId),
    Idea(crate::IdeaId),
}

/// The id of the `index`-th file of `owner` (`MessageFileView::id`): derived,
/// not stored, so files from before ids existed have one too.
pub fn upload_id(owner: UploadOwner, index: usize) -> String {
    match owner {
        UploadOwner::Message(id) => format!("m:{id}:{index}"),
        UploadOwner::Task(id) => format!("t:{id}:{index}"),
        UploadOwner::Schedule(id) => format!("s:{id}:{index}"),
        UploadOwner::Idea(id) => format!("i:{id}:{index}"),
    }
}

pub fn parse_upload_id(id: &str) -> Option<(UploadOwner, usize)> {
    let mut parts = id.splitn(3, ':');
    let (kind, owner, index) = (parts.next()?, parts.next()?, parts.next()?.parse().ok()?);
    let owner = match kind {
        "m" => UploadOwner::Message(MessageId::parse(owner).ok()?),
        "t" => UploadOwner::Task(crate::TaskId::parse(owner).ok()?),
        "s" => UploadOwner::Schedule(crate::TaskScheduleId::parse(owner).ok()?),
        "i" => UploadOwner::Idea(crate::IdeaId::parse(owner).ok()?),
        _ => return None,
    };
    Some((owner, index))
}

/// Give each file its id (see `upload_id`).
pub fn with_upload_ids(owner: UploadOwner, files: &mut [crate::views::MessageFileView]) {
    for (index, file) in files.iter_mut().enumerate() {
        file.id = Some(upload_id(owner, index));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_ids_round_trip_and_reject_junk() {
        let task = crate::TaskId::new();
        let id = upload_id(UploadOwner::Task(task), 2);
        assert_eq!(parse_upload_id(&id), Some((UploadOwner::Task(task), 2)));
        let message = MessageId::new();
        assert_eq!(parse_upload_id(&upload_id(UploadOwner::Message(message), 0)), Some((UploadOwner::Message(message), 0)));
        let idea = crate::IdeaId::new();
        assert_eq!(upload_id(UploadOwner::Idea(idea), 1), format!("i:{idea}:1"));
        assert_eq!(parse_upload_id(&format!("i:{idea}:1")), Some((UploadOwner::Idea(idea), 1)));
        for junk in ["", "t:", "x:1:0", &format!("t:{task}"), &format!("t:{task}:-1"), &format!("m:{task}:a"), "t:../../etc:0"] {
            assert_eq!(parse_upload_id(junk), None, "{junk}");
        }
    }

    #[test]
    fn title_is_first_line_shortened() {
        assert_eq!(derive_title("Hello\nworld"), "Hello");
        let long = "x".repeat(80);
        assert_eq!(derive_title(&long).chars().count(), 61);
        assert_eq!(derive_title("   "), "New chat");
    }

    #[test]
    fn a_employee_message_may_be_only_files_but_not_too_many() {
        let file = vec!["C:/a.pdf".to_string()];
        assert_eq!(validate_employee_message("  ", &[], &file).unwrap(), "");
        assert!(matches!(validate_employee_message("", &[], &[]), Err(DomainError::Required { .. })));
        let many = vec!["C:/a.pdf".to_string(); UPLOADS_MAX + 1];
        assert!(matches!(validate_employee_message("hi", &[], &many), Err(DomainError::File(m)) if m.starts_with("Too many files")));
    }

    #[test]
    fn empty_message_is_rejected() {
        let cmd = SendChatMessage { conversation_id: ConversationId::new(), text: "  ".into(), images: Vec::new(), interrupt: false };
        assert!(cmd.validate().is_err());
    }

    #[test]
    fn an_image_may_stand_without_text_but_not_too_many() {
        let image = vec!["C:\\inbox\\shot.png".to_string()];
        assert_eq!(validate_message("  ", &image).unwrap(), "");
        assert_eq!(validate_message(" kijk ", &image).unwrap(), "kijk");
        assert!(validate_message("x", &["  ".to_string()]).is_err());
        assert!(validate_message("x", &vec!["a.png".to_string(); IMAGES_MAX + 1]).is_err());
    }

    #[test]
    fn a_message_needs_its_conversation() {
        let id = ConversationId::new();
        let sent: SendChatMessage = serde_json::from_value(serde_json::json!({ "conversation_id": id, "text": "hoi" })).unwrap();
        assert_eq!(sent, SendChatMessage { conversation_id: id, text: "hoi".into(), images: Vec::new(), interrupt: false });
        assert!(serde_json::from_value::<SendChatMessage>(serde_json::json!({ "text": "hoi" })).is_err(), "no standalone chats");
    }

}
