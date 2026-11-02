//! Who may edit a message and what the history of an edit holds (AUR-FA-F3):
//! the facts storage collects feed the one rule of the domain.

use crate::message_edits_tests::{ALEX, CHANNEL, ELSEWHERE, FEEDBACK, OTHER, PLAIN, REPLY, channel, conv, msg};
use crate::{conversations, message_edits};
use aurelius_domain::group_chat::EditRefusal;
use aurelius_domain::{MessageId, UserId};

fn alex() -> UserId {
    UserId::parse(ALEX).unwrap()
}

fn refusal(store: &crate::Store, id: &str, reader: Option<UserId>) -> Option<EditRefusal> {
    let (_, facts) = store.read(|c| message_edits::facts(c, msg(id))).unwrap().unwrap();
    facts.refusal(reader)
}

#[test]
fn the_author_may_change_a_plain_message_and_a_reply_in_a_thread() {
    let (_dir, store) = channel();
    assert_eq!(refusal(&store, PLAIN, Some(alex())), None);
    assert_eq!(refusal(&store, REPLY, Some(alex())), None);
    assert_eq!(refusal(&store, ELSEWHERE, Some(alex())), Some(EditRefusal::DecisionFeedback), "a ticket's discussion counts too: only its rule about feedback refuses");
}

#[test]
fn somebody_else_and_an_app_may_not_change_it() {
    let (_dir, store) = channel();
    assert_eq!(refusal(&store, PLAIN, Some(UserId::new())), Some(EditRefusal::NotYours));
    assert_eq!(refusal(&store, PLAIN, None), Some(EditRefusal::NotYours));
}

#[test]
fn the_feedback_of_a_decision_may_not_be_changed() {
    let (_dir, store) = channel();
    assert_eq!(refusal(&store, FEEDBACK, Some(alex())), Some(EditRefusal::DecisionFeedback));
}

#[test]
fn a_deleted_message_and_a_system_line_may_not_be_changed() {
    let (_dir, store) = channel();
    store.write(|tx| conversations::mark_message_deleted(tx, msg(PLAIN), "gone", 500)).unwrap();
    assert_eq!(refusal(&store, PLAIN, Some(alex())), Some(EditRefusal::Deleted));
    store.write(|tx| Ok(tx.execute("UPDATE messages SET meta_json = '{\"system\":{\"kind\":\"moved\"}}' WHERE id = ?1", [REPLY])?)).unwrap();
    assert_eq!(refusal(&store, REPLY, Some(alex())), Some(EditRefusal::NotHere));
}

#[test]
fn a_message_in_a_closed_discussion_may_not_be_changed() {
    let (_dir, store) = channel();
    store.write(|tx| Ok(tx.execute("UPDATE projects SET status = 'archived'", [])?)).unwrap();
    assert_eq!(refusal(&store, PLAIN, Some(alex())), Some(EditRefusal::ReadOnly));
}

#[test]
fn a_message_outside_a_discussion_may_not_be_changed() {
    let (_dir, store) = channel();
    store.write(|tx| Ok(tx.execute("UPDATE conversations SET kind = 'project' WHERE id = ?1", [OTHER])?)).unwrap();
    assert_eq!(refusal(&store, ELSEWHERE, Some(alex())), Some(EditRefusal::NotHere));
}

#[test]
fn the_read_and_the_command_agree_on_every_message() {
    let (_dir, store) = channel();
    let in_read = store.read(|c| message_edits::editable_in(c, conv(CHANNEL), Some(alex()))).unwrap();
    for id in [PLAIN, REPLY, FEEDBACK] {
        assert_eq!(in_read.contains(&msg(id)), refusal(&store, id, Some(alex())).is_none(), "{id}");
    }
    assert!(store.read(|c| message_edits::editable_in(c, conv(CHANNEL), None)).unwrap().is_empty());
}

#[test]
fn a_deleted_message_lists_no_files_or_uploads() {
    let (_dir, store) = channel();
    store
        .write(|tx| {
            tx.execute("UPDATE messages SET meta_json = '{\"files\":[{\"id\":\"f\",\"name\":\"a.txt\",\"media_type\":\"text/plain\",\"size\":1}],\"uploads\":[{\"name\":\"b.txt\",\"path\":\"b.txt\",\"byte_length\":1}]}' WHERE id = ?1", [PLAIN])?;
            Ok(())
        })
        .unwrap();
    let read = |s: &crate::Store| {
        s.read(|c| conversations::list_messages(c, conv(CHANNEL), &|k: &str| Some(k.to_owned()))).unwrap().into_iter().find(|m| m.id == msg(PLAIN)).unwrap()
    };
    let before = read(&store);
    assert!(!before.files.is_empty() || !before.uploads.is_empty(), "the fixture must have attachments");
    store.write(|tx| conversations::mark_message_deleted(tx, msg(PLAIN), "gone", 500)).unwrap();
    let after = read(&store);
    assert!(after.files.is_empty() && after.uploads.is_empty());
}

#[test]
fn the_earlier_texts_come_from_the_edit_events_newest_first() {
    let (_dir, store) = channel();
    store
        .write(|tx| {
            tx.execute_batch(&format!(
                "INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, payload_blob_id, metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms) VALUES
                   ('ed1', 'conversation', '{CHANNEL}', 10, 'message.edited', 1, 'user', 'c', 'b1', '{{\"message_id\":\"{PLAIN}\"}}', 'cmd:e1:edited', 200, 200),
                   ('ed2', 'conversation', '{CHANNEL}', 11, 'message.edited', 1, 'user', 'c', 'new', '{{\"message_id\":\"{PLAIN}\"}}', 'cmd:e2:edited', 300, 300),
                   ('ed3', 'conversation', '{CHANNEL}', 12, 'message.edited', 1, 'user', 'c', 'b2', '{{\"message_id\":\"{REPLY}\"}}', 'cmd:e3:edited', 400, 400);"
            ))?;
            Ok(())
        })
        .unwrap();
    let found = store.read(|c| message_edits::earlier_texts(c, msg(PLAIN))).unwrap();
    assert_eq!(found, [("Second".to_owned(), 300), ("First".to_owned(), 200)]);
    assert!(store.read(|c| message_edits::earlier_texts(c, MessageId::new())).unwrap().is_empty());
}
