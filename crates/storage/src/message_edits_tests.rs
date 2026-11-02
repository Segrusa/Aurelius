//! Editing and deleting a message in storage (AUR-FA-S1): a new blob, the old one
//! handed back for the event, the marks in every read, and how a decision's
//! feedback is told apart.

use crate::{Store, conversations};
use aurelius_domain::{ConversationId, MessageId};

const P: &str = "00000000-0000-7000-8000-000000000001";
const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const CHANNEL: &str = "00000000-0000-7000-8000-0000000000c1";
const OTHER: &str = "00000000-0000-7000-8000-0000000000c2";
const PLAIN: &str = "00000000-0000-7000-8000-0000000000a1";
const REPLY: &str = "00000000-0000-7000-8000-0000000000a2";
const FEEDBACK: &str = "00000000-0000-7000-8000-0000000000a3";
const ELSEWHERE: &str = "00000000-0000-7000-8000-0000000000a4";

fn msg(id: &str) -> MessageId {
    MessageId::parse(id).unwrap()
}

fn conv(id: &str) -> ConversationId {
    ConversationId::parse(id).unwrap()
}

/// A channel with a message, a reply in its thread and a decision's feedback
/// (its event has the dedupe key `decision_feedback` gives it), and a second
/// conversation with feedback of its own. A blob's storage key is its text.
fn channel() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|tx| {
            let company: String = tx.query_row("SELECT id FROM company", [], |r| r.get(0))?;
            let global: String = tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0))?;
            tx.execute_batch(&format!(
                "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}');
                 INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
                 INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{ALEX}', '{company}', 'alex', '$argon2id$x', 1, 1);
                 INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES ('{CHANNEL}', 'group', '{P}', 'Alpha', 1, 10), ('{OTHER}', 'ticket', '{P}', '#1', 1, 10);
                 INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms) VALUES
                   ('e-plain', 'conversation', '{CHANNEL}', 1, 'message.user_committed', 1, 'user', 'c', '{{}}', 'cmd:abc', 12, 12),
                   ('e-feedback', 'conversation', '{CHANNEL}', 2, 'message.user_committed', 1, 'user', 'c', '{{}}', 'cmd:abc:feedback', 13, 13),
                   ('e-near', 'conversation', '{CHANNEL}', 3, 'message.user_committed', 1, 'user', 'c', '{{}}', 'cmd:abc:feedback:again', 14, 14),
                   ('e-other', 'conversation', '{OTHER}', 1, 'message.user_committed', 1, 'user', 'c', '{{}}', 'cmd:def:feedback', 15, 15);
                 INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES
                   ('b1', 'First', 's', 0, 'text/plain', 1), ('b2', 'A reply', 's', 0, 'text/plain', 1), ('b3', 'Feedback', 's', 0, 'text/plain', 1), ('b4', 'Elsewhere', 's', 0, 'text/plain', 1),
                   ('new', 'Second', 's', 0, 'text/plain', 1), ('gone', 'This message was deleted.', 's', 0, 'text/plain', 1);
                 INSERT INTO messages(id, conversation_id, ordinal, role, content_blob_id, source_event_id, status, input_kind, created_at_ms, author_user_id, thread_parent_id) VALUES
                   ('{PLAIN}', '{CHANNEL}', 1, 'user', 'b1', 'e-plain', 'complete', 'text', 100, '{ALEX}', NULL),
                   ('{REPLY}', '{CHANNEL}', 2, 'user', 'b2', 'e-near', 'complete', 'text', 110, '{ALEX}', '{PLAIN}'),
                   ('{FEEDBACK}', '{CHANNEL}', 3, 'user', 'b3', 'e-feedback', 'complete', 'text', 120, '{ALEX}', NULL),
                   ('{ELSEWHERE}', '{OTHER}', 1, 'user', 'b4', 'e-other', 'complete', 'text', 130, '{ALEX}', NULL);"
            ))?;
            Ok(())
        })
        .unwrap();
    (dir, store)
}

fn text(key: &str) -> Option<String> {
    Some(key.to_owned())
}

/// (text, edited, deleted) of a message of the channel itself.
fn read(store: &Store, id: &str) -> (String, Option<i64>, Option<i64>) {
    let messages = store.read(|c| conversations::list_messages(c, conv(CHANNEL), &text)).unwrap();
    let m = messages.into_iter().find(|m| m.id == msg(id)).unwrap();
    (m.text, m.edited_at_ms, m.deleted_at_ms)
}

#[test]
fn replacing_the_text_shows_the_new_blob_and_hands_back_the_old_one() {
    let (_dir, store) = channel();
    let replaced = store.write(|tx| conversations::replace_message_text(tx, msg(PLAIN), "new", 500)).unwrap().unwrap();
    assert_eq!(replaced, conversations::ReplacedText { conversation_id: conv(CHANNEL), previous_blob: Some("b1".into()) });
    assert_eq!(read(&store, PLAIN), ("Second".to_owned(), Some(500), None));
    assert_eq!(read(&store, FEEDBACK), ("Feedback".to_owned(), None, None), "the others stay");
}

#[test]
fn the_old_blob_stays_after_an_edit() {
    let (_dir, store) = channel();
    store.write(|tx| conversations::replace_message_text(tx, msg(PLAIN), "new", 500)).unwrap();
    let kept: i64 = store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM blobs WHERE id = 'b1' AND deleted_at_ms IS NULL", [], |r| r.get(0))?)).unwrap();
    assert_eq!(kept, 1);
}

#[test]
fn marking_deleted_keeps_the_row_with_the_placeholder_and_hands_back_the_old_blob() {
    let (_dir, store) = channel();
    store.write(|tx| conversations::replace_message_text(tx, msg(PLAIN), "new", 500)).unwrap();
    let marked = store.write(|tx| conversations::mark_message_deleted(tx, msg(PLAIN), "gone", 600)).unwrap().unwrap();
    assert_eq!(marked.previous_blob.as_deref(), Some("new"));
    assert_eq!(read(&store, PLAIN), ("This message was deleted.".to_owned(), Some(500), Some(600)));
}

#[test]
fn a_deleted_message_stays_deleted_and_an_unknown_message_changes_nothing() {
    let (_dir, store) = channel();
    store.write(|tx| conversations::mark_message_deleted(tx, msg(PLAIN), "gone", 600)).unwrap();
    assert_eq!(store.write(|tx| conversations::replace_message_text(tx, msg(PLAIN), "new", 700)).unwrap(), None);
    assert_eq!(store.write(|tx| conversations::mark_message_deleted(tx, msg(PLAIN), "new", 700)).unwrap(), None);
    assert_eq!(read(&store, PLAIN), ("This message was deleted.".to_owned(), None, Some(600)));

    let unknown = MessageId::new();
    assert_eq!(store.write(move |tx| conversations::replace_message_text(tx, unknown, "new", 700)).unwrap(), None);
    assert_eq!(store.write(move |tx| conversations::mark_message_deleted(tx, unknown, "new", 700)).unwrap(), None);
}

#[test]
fn a_blob_that_does_not_exist_is_refused() {
    let (_dir, store) = channel();
    assert!(store.write(|tx| conversations::replace_message_text(tx, msg(PLAIN), "nope", 500)).is_err());
    assert_eq!(read(&store, PLAIN), ("First".to_owned(), None, None));
}

#[test]
fn a_thread_read_carries_the_marks_too() {
    let (_dir, store) = channel();
    store.write(|tx| conversations::replace_message_text(tx, msg(REPLY), "new", 500)).unwrap();
    store.write(|tx| conversations::mark_message_deleted(tx, msg(PLAIN), "gone", 600)).unwrap();
    let thread = store.read(|c| conversations::list_thread(c, conv(CHANNEL), msg(PLAIN), &text)).unwrap();
    assert_eq!(
        thread.iter().map(|m| (m.text.as_str(), m.edited_at_ms, m.deleted_at_ms)).collect::<Vec<_>>(),
        [("This message was deleted.", None, Some(600)), ("Second", Some(500), None)]
    );
}

#[test]
fn a_message_list_on_a_store_before_schema_29_reads_as_never_edited() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_until(dir.path(), 28).unwrap();
    assert_eq!(store.read(|c| conversations::list_messages(c, ConversationId::new(), &text)).unwrap(), []);
}

#[test]
fn only_a_message_from_a_feedback_event_is_a_decisions_feedback() {
    let (_dir, store) = channel();
    let is = |id: &str| store.read(|c| conversations::is_decision_feedback(c, msg(id))).unwrap();
    assert!(is(FEEDBACK));
    assert!(!is(PLAIN), "a plain command's message");
    assert!(!is(REPLY), "the key only looks like it");
    assert!(!store.read(|c| conversations::is_decision_feedback(c, MessageId::new())).unwrap(), "an unknown message");
}

#[test]
fn feedback_stays_feedback_after_an_edit() {
    let (_dir, store) = channel();
    store.write(|tx| conversations::replace_message_text(tx, msg(FEEDBACK), "new", 500)).unwrap();
    assert!(store.read(|c| conversations::is_decision_feedback(c, msg(FEEDBACK))).unwrap());
}

#[test]
fn the_feedback_of_a_conversation_is_its_own_and_agrees_with_the_single_check() {
    let (_dir, store) = channel();
    let found = store.read(|c| conversations::decision_feedback_in(c, conv(CHANNEL))).unwrap();
    assert_eq!(found.into_iter().collect::<Vec<_>>(), [msg(FEEDBACK)]);
    let other = store.read(|c| conversations::decision_feedback_in(c, conv(OTHER))).unwrap();
    assert_eq!(other.into_iter().collect::<Vec<_>>(), [msg(ELSEWHERE)]);
}
