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
/// Most hops in a channel of the Team chat after one message of a board member
/// (RULE 08: no endless ping-pong). A board member's message is hop 0; what an
/// employee writes in a run woken at hop `h` is hop `h + 1`, a handoff past this is refused.
pub const MAX_HOPS: u32 = 3;
/// The same limit in a ticket's thread (assumption 1): a chain of handoffs on a
/// ticket gets further than a question in a channel.
pub const TICKET_MAX_HOPS: u32 = 8;
/// Longest post of an employee in the group chat (characters).
pub const POST_MAX: usize = 4_000;
/// Most handoffs of one run (RULE 08).
pub const WAKING_POSTS_PER_RUN: u32 = 2;
/// Most handoffs of one employee within `WAKE_WINDOW_MS`, over all its runs: a
/// second bound next to the hop count (RULE 08).
pub const WAKING_POSTS_PER_WINDOW: u32 = 4;
pub const WAKE_WINDOW_MS: i64 = 60 * 60 * 1000;

/// What the quick answers of all employees together may cost per hour, in micro-units
/// of the usage records (1 000 000 = 1 USD). A quick answer is an extra run next to
/// the one an employee is busy with; at the cap none starts and the busy line stays.
pub const QUICK_ANSWER_COST_PER_HOUR_MICRO: i64 = 1_000_000;

/// The hop limit of a thread (`MAX_HOPS`, or `TICKET_MAX_HOPS` on a ticket).
pub fn hop_limit(on_ticket: bool) -> u32 {
    if on_ticket { TICKET_MAX_HOPS } else { MAX_HOPS }
}

/// The text ends on a question: its last non-empty line, without markdown marks, ends with "?".
/// The one rule for a run that ended on a question (`MessageView::awaiting_user`).
pub fn asks_question(text: &str) -> bool {
    let last = text.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("");
    last.trim_end_matches(|c: char| matches!(c, '*' | '_' | '`' | '"' | '\'' | ')' | '\u{201d}' | '\u{bb}' | ' ')).ends_with('?')
}

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

/// The word after `@` that addresses everyone in a discussion (`@Everyone`).
pub const EVERYONE_MENTION: &str = "everyone";

/// Whether `text` addresses everyone with `@Everyone` (case-insensitive, the
/// whole word, with the same boundaries as `mentions`: `@` inside a word is an
/// email address, and the name must end at a character that is not a letter,
/// digit, `_` or `-`). Whoever may use it is a rule of the discussion engine.
pub fn names_everyone(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.match_indices('@').any(|(at, _)| {
        if lower[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '+')) {
            return false;
        }
        let rest = &lower[at + 1..];
        rest.starts_with(EVERYONE_MENTION) && rest[EVERYONE_MENTION.len()..].chars().next().is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-'))
    })
}

/// One message of a discussion, as the scoped view looks at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewLine<'a> {
    /// The employee who wrote it (a name); none: a board member.
    pub employee: Option<&'a str>,
    /// The employee a handoff in this message was for (`hand_off`).
    pub to: Option<EmployeeId>,
    pub text: &'a str,
}

/// The scoped view (P3): which of `lines` the prompt of `me` shows, or `None` for
/// all of them: for the thread's lead and for the only employee in the thread (nobody
/// else wrote or was named). A non-lead sees what names it, what was handed over to it
/// and what it wrote;
/// `owner`: it owns the ticket, so a board member's message that names nobody is
/// addressed to it too (B3). A board member's `@Everyone` is addressed to everyone,
/// though it woke only one: this is how the others are in the know. Only the prompt: the clients always show the whole thread.
pub fn scoped_view(lines: &[ViewLine<'_>], me: (EmployeeId, &str), team: &[(EmployeeId, String)], lead: bool, owner: bool) -> Option<Vec<usize>> {
    if lead {
        return None;
    }
    let mine = |line: &ViewLine<'_>| line.employee.is_some_and(|name| name.to_lowercase() == me.1.to_lowercase());
    let named: Vec<Vec<EmployeeId>> = lines.iter().map(|line| mentions(line.text, team)).collect();
    let alone = lines.iter().zip(&named).all(|(line, named)| (line.employee.is_none() || mine(line)) && line.to.is_none_or(|to| to == me.0) && named.iter().all(|e| *e == me.0));
    if alone {
        return None;
    }
    Some(
        lines
            .iter()
            .zip(&named)
            .enumerate()
            .filter(|(_, (line, named))| mine(line) || line.to == Some(me.0) || named.contains(&me.0) || (line.employee.is_none() && (names_everyone(line.text) || (owner && named.is_empty()))))
            .map(|(index, _)| index)
            .collect(),
    )
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
    use super::names_everyone;

    #[test]
    fn everyone_is_a_whole_word_after_an_at_that_is_not_inside_a_word() {
        for yes in ["@Everyone", "hi @EVERYONE!", "(@everyone)", "a\n@everyone, b", "@everyone."] {
            assert!(names_everyone(yes), "{yes}");
        }
        for no in ["everyone", "mail me@everyone.com", "a.@everyone", "@everyone2", "@everyone_x", "@everyone-else", "@every one", "@"] {
            assert!(!names_everyone(no), "{no}");
        }
    }

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

    fn line<'a>(employee: Option<&'a str>, text: &'a str) -> ViewLine<'a> {
        ViewLine { employee, to: None, text }
    }

    #[test]
    fn the_scoped_view_keeps_what_names_me_and_what_i_wrote() {
        let (ann, bo) = (EmployeeId::new(), EmployeeId::new());
        let team = vec![(ann, "Ann".to_owned()), (bo, "Bo".to_owned())];
        let lines = [line(None, "@Bo can you look?"), line(Some("Bo"), "@Ann over to you"), line(Some("ann"), "On it."), line(None, "Thanks all"), line(Some("Bo"), "Looking.")];
        assert_eq!(scoped_view(&lines, (ann, "Ann"), &team, false, false), Some(vec![1, 2]));
        // The owner also has what a board member wrote to nobody in particular (B3).
        assert_eq!(scoped_view(&lines, (ann, "Ann"), &team, false, true), Some(vec![1, 2, 3]));
    }

    #[test]
    fn a_board_members_everyone_is_in_everyones_scoped_view_an_employees_is_not() {
        let (ann, bo) = (EmployeeId::new(), EmployeeId::new());
        let team = vec![(ann, "Ann".to_owned()), (bo, "Bo".to_owned())];
        let lines = [line(None, "@Bo look"), line(None, "@Everyone standup"), line(Some("Bo"), "@everyone done")];
        assert_eq!(scoped_view(&lines, (ann, "Ann"), &team, false, false), Some(vec![1]));
    }

    #[test]
    fn the_lead_and_the_only_employee_see_everything() {
        let (ann, bo) = (EmployeeId::new(), EmployeeId::new());
        let team = vec![(ann, "Ann".to_owned()), (bo, "Bo".to_owned())];
        let lines = [line(None, "@Bo look"), line(Some("Bo"), "Done")];
        assert_eq!(scoped_view(&lines, (ann, "Ann"), &team, true, false), None, "the lead");
        assert_eq!(scoped_view(&[line(None, "Hello"), line(Some("Ann"), "Hi @Ann")], (ann, "Ann"), &team, false, false), None, "alone");
        assert_eq!(scoped_view(&[line(None, "ask @Bo")], (ann, "Ann"), &team, false, false), Some(vec![]), "someone else was named");
    }

    #[test]
    fn what_was_handed_over_to_me_is_in_my_scoped_view() {
        let (ann, bo, cy) = (EmployeeId::new(), EmployeeId::new(), EmployeeId::new());
        let team = vec![(ann, "Ann".to_owned()), (bo, "Bo".to_owned()), (cy, "Cy".to_owned())];
        let handed = ViewLine { employee: Some("Bo"), to: Some(ann), text: "Bo handed over to Ann" };
        let lines = [handed, ViewLine { employee: Some("Bo"), to: Some(cy), text: "Bo handed over to Cy" }, line(Some("Cy"), "Done.")];
        assert_eq!(scoped_view(&lines, (ann, "Ann"), &team, false, false), Some(vec![0]));
        // Someone else's handoff means Ann is not alone in the thread.
        assert_eq!(scoped_view(&[line(None, "Hi"), ViewLine { employee: None, to: Some(cy), text: "x" }], (ann, "Ann"), &team, false, false), Some(vec![]));
    }

    #[test]
    fn a_ticket_thread_gets_further_than_a_channel() {
        assert_eq!((hop_limit(false), hop_limit(true)), (3, 8));
    }

    #[test]
    fn a_message_needs_text_or_files() {
        assert!(SendGroupMessage { confirm_secret: false, text: "  ".into(), images: vec![], files: vec![], channel: None, thread_parent: None }.validate().is_err());
        assert_eq!(SendGroupMessage { confirm_secret: false, text: " Hi ".into(), images: vec![], files: vec![], channel: None, thread_parent: None }.validate().unwrap(), "Hi");
        assert_eq!(shortened("abcdef", 3), "abc…");
        assert_eq!(shortened(" ab ", 3), "ab");
    }
}
