//! The company's group chat ("Team chat"): one conversation (kind `group`) of
//! the user with every employee. The user's messages are plain user messages;
//! an employee's are updates (`MessageView::update` "group") with its name as
//! author. Who answers: the employees named with `@Name`, else the top of the
//! org chart. An employee answers from its own main chat; the answer is copied here.

use crate::{EmployeeId, MessageId, conversation::validate_employee_message, DomainError};
use serde::{Deserialize, Serialize};

/// The `MessageView::update` kind of an employee's message in the group chat,
/// and of the group messages forwarded to an employee's main chat.
pub const GROUP_KIND: &str = "group";
/// The author of the user's messages, as employees read them.
pub const OWNER: &str = "Owner";
/// How many of the latest group messages an employee gets with a new one.
pub const CONTEXT_MESSAGES: usize = 20;
/// How long one earlier group message is in that context (characters).
pub const CONTEXT_CHARS: usize = 600;
/// Most hops of employees waking each other with `@Name` after one message of
/// the user (RULE 08: no endless ping-pong). The user's message wakes hop 0; a
/// post of an employee woken at hop `h` wakes at `h + 1`, refused past this.
pub const MAX_HOPS: u32 = 3;
/// Longest post of an employee in the group chat (characters).
pub const POST_MAX: usize = 4_000;
/// Most posts of one run that wake colleagues (RULE 08); later posts are shown but wake nobody.
pub const WAKING_POSTS_PER_RUN: u32 = 2;
/// Most posts of one employee that wake colleagues within `WAKE_WINDOW_MS`, over
/// all its runs. The hop count restarts in a run the group chat did not wake (a
/// task's, a heartbeat's), so this bounds a loop through `delegate_task` or
/// `message_task` too (RULE 08).
pub const WAKING_POSTS_PER_WINDOW: u32 = 4;
pub const WAKE_WINDOW_MS: i64 = 60 * 60 * 1000;

/// A message of the user in the group chat (`group_chat.send`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SendGroupMessage {
    pub text: String,
    /// Images that belong to this message (see `SendChatMessage::images`).
    #[serde(default)]
    pub images: Vec<String>,
    /// Files of any type for the employees (host paths, or this device's upload inbox).
    #[serde(default)]
    pub files: Vec<String>,
    /// The channel (feature `team_channels`): a project's, or none for the general one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<crate::ConversationId>,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
    /// A reply in the thread of this message of the channel (feature
    /// `chat_threads`): a message of the same channel that is not itself a reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_parent: Option<MessageId>,
}

/// The longest emoji of a reaction (bytes of UTF-8): one emoji with its modifiers.
pub const REACTION_EMOJI_MAX: usize = 64;

/// A board member adds (`on`) or takes back their reaction with `emoji` on a
/// message of a channel or a ticket's discussion that is not closed
/// (`message.react`, feature `chat_reactions`). Idempotent: on twice is on once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReactToMessage {
    pub message_id: MessageId,
    pub emoji: String,
    pub on: bool,
}

/// A board member changes the text of their own message in a channel or a
/// ticket's discussion (`message.edit`, feature `message_edits`). It wakes nobody.
/// The earlier text stays in the audit log: editing does not erase a secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EditMessage {
    pub message_id: MessageId,
    pub text: String,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// A board member deletes their own message in a channel or a ticket's discussion
/// (`message.delete`, feature `message_edits`): a line stays that says so. For a
/// message still waiting for its answer see `conversation::DeleteQueuedMessage`.
/// The text stays in the audit log: deleting does not erase a secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteMessage {
    pub message_id: MessageId,
}

/// The text a deleted message shows instead of its own (`message.delete`).
pub const MESSAGE_DELETED_TEXT: &str = "This message was deleted.";

/// What `message.edit` and `message.delete` need to know about a message to decide
/// whether a reader may change it. Storage collects it; `refusal` is the one rule,
/// used by the read (`MessageView::editable`) and by both commands alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditFacts {
    /// The board member who wrote it; none for an employee's, a system line, an app's.
    pub author: Option<crate::UserId>,
    /// It is in a channel or a ticket's discussion (not a chat, a task or a run).
    pub in_discussion: bool,
    /// It is a system line (a grey line that says what happened).
    pub system: bool,
    /// It still waits in the queue for its answer (`DeleteQueuedMessage` is for that).
    pub queued: bool,
    pub deleted: bool,
    /// It is the feedback a board member gave with a decision on a ticket.
    pub decision_feedback: bool,
    /// The discussion is closed: an archived project's channel, a ticket on Done or archived.
    pub read_only: bool,
}

/// Why a reader may not edit or delete a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditRefusal {
    /// Not the reader's own message, or the reader is no board member.
    NotYours,
    /// A message that is not in a channel or a ticket's discussion, or is a system line.
    NotHere,
    Queued,
    Deleted,
    DecisionFeedback,
    ReadOnly,
}

impl EditRefusal {
    /// The words a client may show.
    pub fn why(self) -> &'static str {
        match self {
            EditRefusal::NotYours => "You can only change your own messages.",
            EditRefusal::NotHere => "Only a message in a channel or a ticket's discussion can be changed.",
            EditRefusal::Queued => "This message is still waiting for its answer: delete it from the queue instead.",
            EditRefusal::Deleted => "This message was deleted.",
            EditRefusal::DecisionFeedback => "This message is the feedback of a decision: it cannot be changed.",
            EditRefusal::ReadOnly => "This discussion is closed: its messages can only be read.",
        }
    }
}

impl EditFacts {
    /// Why `reader` (the board member of the session; none for an app) may not edit
    /// or delete the message, or none when they may. This is the only rule.
    pub fn refusal(&self, reader: Option<crate::UserId>) -> Option<EditRefusal> {
        if !self.in_discussion || self.system {
            return Some(EditRefusal::NotHere);
        }
        if self.author.is_none() || self.author != reader {
            return Some(EditRefusal::NotYours);
        }
        if self.deleted {
            return Some(EditRefusal::Deleted);
        }
        if self.queued {
            return Some(EditRefusal::Queued);
        }
        if self.decision_feedback {
            return Some(EditRefusal::DecisionFeedback);
        }
        if self.read_only {
            return Some(EditRefusal::ReadOnly);
        }
        None
    }
}

impl ReactToMessage {
    /// The emoji as stored: trimmed, not empty, at most `REACTION_EMOJI_MAX` bytes.
    pub fn validate(&self) -> Result<String, DomainError> {
        let emoji = self.emoji.trim();
        if emoji.is_empty() {
            return Err(DomainError::Required { field: "emoji".into() });
        }
        if emoji.len() > REACTION_EMOJI_MAX {
            return Err(DomainError::TooLong { field: "emoji".into(), max: REACTION_EMOJI_MAX });
        }
        Ok(emoji.to_owned())
    }
}

impl SendGroupMessage {
    /// The text as stored (empty only with files).
    pub fn validate(&self) -> Result<String, DomainError> {
        validate_employee_message(&self.text, &self.images, &self.files)
    }
}

/// The employees `text` names with `@Name` (case-insensitive, the whole name;
/// the longest name wins when one name starts another), in the order named,
/// each once. A name must end at the end of the text or at a character that is
/// not a letter, digit, `_` or `-`.
pub fn mentions(text: &str, employees: &[(EmployeeId, String)]) -> Vec<EmployeeId> {
    let lower = text.to_lowercase();
    let mut by_length: Vec<(EmployeeId, String)> = employees.iter().map(|(id, name)| (*id, name.trim().to_lowercase())).filter(|(_, n)| !n.is_empty()).collect();
    by_length.sort_by_key(|(_, n)| std::cmp::Reverse(n.chars().count()));
    let mut out = Vec::new();
    for (at, _) in lower.match_indices('@') {
        // `@` inside a word (an email address) is no mention.
        if lower[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '+')) {
            continue;
        }
        let rest = &lower[at + 1..];
        let found = by_length.iter().find(|(_, name)| {
            rest.starts_with(name.as_str()) && rest[name.len()..].chars().next().is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-'))
        });
        if let Some((id, _)) = found {
            if !out.contains(id) {
                out.push(*id);
            }
        }
    }
    out
}

/// A text at most `max` characters long, cut with "…".
pub fn shortened(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_find_whole_names_in_any_case_once() {
        let (ann, anna, bo, jean_paul) = (EmployeeId::new(), EmployeeId::new(), EmployeeId::new(), EmployeeId::new());
        let team = vec![(ann, "Ann".to_owned()), (anna, "Anna".to_owned()), (bo, "Bo".to_owned()), (jean_paul, "Jean Paul".to_owned())];
        assert_eq!(mentions("@anna and @BO, then @Ann again @anna", &team), vec![anna, bo, ann]);
        assert_eq!(mentions("Hi @Jean Paul!", &team), vec![jean_paul]);
        assert!(mentions("no one here, mail bo@bo.com, x-@Bo, x_@Bo, x+@Bo or @Bob", &team).is_empty());
        assert!(mentions("@ Ann", &team).is_empty());
        assert_eq!(mentions("(@Bo)", &team), vec![bo]);
    }

    #[test]
    fn a_message_needs_text_or_files() {
        assert!(SendGroupMessage { confirm_secret: false, text: "  ".into(), images: vec![], files: vec![], channel: None, thread_parent: None }.validate().is_err());
        assert_eq!(SendGroupMessage { confirm_secret: false, text: " Hi ".into(), images: vec![], files: vec![], channel: None, thread_parent: None }.validate().unwrap(), "Hi");
        assert_eq!(shortened("abcdef", 3), "abc…");
        assert_eq!(shortened(" ab ", 3), "ab");
    }
}
