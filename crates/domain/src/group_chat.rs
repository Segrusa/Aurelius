//! The company's group chat ("Team chat"): one conversation (kind `group`) of
//! the user with every employee. The user's messages are plain user messages;
//! an employee's are updates (`MessageView::update` "group") with its name as
//! author. Who answers: the employees named with `@Name`, else the top of the
//! org chart. An employee answers from its own main chat; the answer is copied here.

use crate::{EmployeeId, conversation::validate_employee_message, DomainError};
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
        assert!(SendGroupMessage { confirm_secret: false, text: "  ".into(), images: vec![], files: vec![], channel: None }.validate().is_err());
        assert_eq!(SendGroupMessage { confirm_secret: false, text: " Hi ".into(), images: vec![], files: vec![], channel: None }.validate().unwrap(), "Hi");
        assert_eq!(shortened("abcdef", 3), "abc…");
        assert_eq!(shortened(" ab ", 3), "ab");
    }
}
