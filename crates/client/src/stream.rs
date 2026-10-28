//! Remote connections receive streamed text as appends (`StreamAppend`,
//! offsets in UTF-16 units); a window expects the full text so far
//! (`StreamDelta`), as over local IPC. This rebuilds it, mirroring the
//! gateway's `Appends` (same clearing on message/run events).
//!
//! Both transports may carry only the tool calls that changed
//! (`activity_changes`, asked for in the hello); `Calls` merges them back into
//! the full list a window expects.

use aurelius_domain::MessageId;
use aurelius_protocol::remote::StreamAppend;
use aurelius_protocol::{ActivityView, Event, StreamDelta};
use std::collections::HashMap;

#[derive(Default)]
pub struct Rebuild {
    texts: HashMap<MessageId, (String, Vec<ActivityView>)>,
}

impl Rebuild {
    /// The full text so far, or `None` when the append doesn't continue what
    /// arrived before (a missed frame); the next full text (offset 0) fixes it.
    pub fn apply(&mut self, append: StreamAppend) -> Option<StreamDelta> {
        let entry = self.texts.entry(append.message_id).or_default();
        if append.offset == 0 {
            entry.0 = append.text;
        } else {
            let at = byte_index(&entry.0, append.offset)?;
            entry.0.truncate(at);
            entry.0.push_str(&append.text);
        }
        let activity = if append.activity_changes {
            // Changes only: `Calls` merges them.
            append.activity.unwrap_or_default()
        } else {
            if let Some(activity) = append.activity {
                entry.1 = activity;
            }
            entry.1.clone()
        };
        Some(StreamDelta {
            run_id: append.run_id,
            conversation_id: append.conversation_id,
            message_id: append.message_id,
            text: entry.0.clone(),
            activity,
            progress: append.progress,
            activity_changes: append.activity_changes,
        })
    }

    /// The gateway forgets its texts on these events and resends full text after.
    pub fn on_event(&mut self, event: &Event) {
        let kind = event.kind.as_str();
        if kind.starts_with("message.") || kind.starts_with("run.") {
            self.texts.clear();
        }
    }
}

/// The tool calls of each streaming message, so updates that carry only the
/// changed calls become full lists again. Forgotten only on `message.*` /
/// `run.*` events, at the same point in the stream where the server forgets
/// what it sent (it then sends full lists again); never by size, or a list
/// could go while the server still sends its changes.
#[derive(Default)]
pub struct Calls {
    messages: HashMap<MessageId, Vec<ActivityView>>,
}

impl Calls {
    pub fn apply(&mut self, mut delta: StreamDelta) -> StreamDelta {
        if !delta.activity_changes && !self.messages.contains_key(&delta.message_id) && delta.activity.is_empty() {
            // Plain text of a message without tools (and an older server): nothing to keep.
            return delta;
        }
        let calls = self.messages.entry(delta.message_id).or_default();
        if delta.activity_changes {
            for call in std::mem::take(&mut delta.activity) {
                match calls.iter_mut().find(|c| c.call_id == call.call_id) {
                    Some(known) => *known = call,
                    None => calls.push(call),
                }
            }
        } else {
            *calls = std::mem::take(&mut delta.activity);
        }
        StreamDelta { activity: calls.clone(), activity_changes: false, ..delta }
    }

    pub fn on_event(&mut self, event: &Event) {
        let kind = event.kind.as_str();
        if kind.starts_with("message.") || kind.starts_with("run.") {
            self.messages.clear();
        }
    }
}

/// Byte index of UTF-16 offset `units` in `text`; `None` past the end or
/// inside a surrogate pair.
fn byte_index(text: &str, units: u64) -> Option<usize> {
    let mut seen = 0u64;
    for (index, ch) in text.char_indices() {
        if seen == units {
            return Some(index);
        }
        seen += ch.len_utf16() as u64;
        if seen > units {
            return None;
        }
    }
    (seen == units).then_some(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_domain::{ConversationId, RunId};

    fn append(id: MessageId, offset: u64, text: &str) -> StreamAppend {
        StreamAppend { run_id: RunId::new(), conversation_id: ConversationId::new(), message_id: id, offset, text: text.into(), activity: None, progress: None, activity_changes: false }
    }

    #[test]
    fn appends_become_the_full_text_again() {
        let mut r = Rebuild::default();
        let id = MessageId::new();
        assert_eq!(r.apply(append(id, 0, "Hi 👋")).unwrap().text, "Hi 👋");
        // "Hi 👋" is 5 UTF-16 units: the emoji is a surrogate pair.
        assert_eq!(r.apply(append(id, 5, " there")).unwrap().text, "Hi 👋 there");
        assert_eq!(r.apply(append(id, 0, "Hello")).unwrap().text, "Hello", "a rewrite resends everything");
        assert_eq!(r.apply(append(id, 2, "y")).unwrap().text, "Hey", "an earlier offset cuts first");
    }

    #[test]
    fn a_gap_waits_for_the_next_full_text() {
        let mut r = Rebuild::default();
        let id = MessageId::new();
        assert!(r.apply(append(id, 7, "lost")).is_none());
        r.apply(append(id, 0, "Hi 👋")).unwrap();
        assert!(r.apply(append(id, 4, "x")).is_none(), "inside the surrogate pair");
    }

    #[test]
    fn activity_is_kept_until_it_changes() {
        let mut r = Rebuild::default();
        let id = MessageId::new();
        let tool = ActivityView { call_id: "1".into(), tool: "Bash".into(), input: serde_json::json!({}), output: None, ok: None, offset: 0, truncated: false, output_bytes: None };
        let mut first = append(id, 0, "a");
        first.activity = Some(vec![tool.clone()]);
        r.apply(first);
        assert_eq!(r.apply(append(id, 1, "b")).unwrap().activity, vec![tool]);
    }

    fn call(id: &str, ok: Option<bool>) -> ActivityView {
        ActivityView { call_id: id.into(), tool: "Bash".into(), input: serde_json::json!({}), output: None, ok, offset: 0, truncated: false, output_bytes: None }
    }

    fn update(id: MessageId, activity: Vec<ActivityView>, activity_changes: bool) -> StreamDelta {
        StreamDelta { run_id: RunId::new(), conversation_id: ConversationId::new(), message_id: id, text: "t".into(), activity, progress: None, activity_changes }
    }

    #[test]
    fn changed_calls_are_merged_into_the_full_list() {
        let mut calls = Calls::default();
        let id = MessageId::new();
        let full = calls.apply(update(id, vec![call("a", Some(true)), call("b", None)], false));
        assert_eq!(full.activity, vec![call("a", Some(true)), call("b", None)]);
        // Text only: the list stays.
        let same = calls.apply(update(id, vec![], true));
        assert!(!same.activity_changes);
        assert_eq!(same.activity, vec![call("a", Some(true)), call("b", None)]);
        // "b" finished, "c" started.
        let merged = calls.apply(update(id, vec![call("b", Some(false)), call("c", None)], true));
        assert_eq!(merged.activity, vec![call("a", Some(true)), call("b", Some(false)), call("c", None)]);
        // A full list replaces.
        assert_eq!(calls.apply(update(id, vec![call("x", None)], false)).activity, vec![call("x", None)]);
    }

    #[test]
    fn appends_with_changes_pass_them_on() {
        let mut r = Rebuild::default();
        let id = MessageId::new();
        let mut first = append(id, 0, "a");
        first.activity = Some(vec![call("1", None)]);
        first.activity_changes = true;
        let delta = r.apply(first).unwrap();
        assert!(delta.activity_changes);
        assert_eq!(delta.activity, vec![call("1", None)]);
        let mut next = append(id, 1, "b");
        next.activity_changes = true;
        assert!(r.apply(next).unwrap().activity.is_empty());
    }
}
