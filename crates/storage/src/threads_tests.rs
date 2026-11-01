//! Threads and reactions in storage (AUR-CHAT-002): replies stay out of the
//! channel's list and unread, the root counts them, a thread is read on its own,
//! and a reaction is once per board member and emoji.

use crate::{Store, conversations, discussions, threads};
use aurelius_domain::{ConversationId, MessageId, UserId};

const P: &str = "00000000-0000-7000-8000-000000000001";
const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const BEA: &str = "00000000-0000-7000-8000-000000000005";
const EVENT: &str = "00000000-0000-7000-8000-000000000031";
const CHANNEL: &str = "00000000-0000-7000-8000-0000000000c1";
const ROOT: &str = "00000000-0000-7000-8000-0000000000a1";
const REPLY_BEA: &str = "00000000-0000-7000-8000-0000000000a2";
const REPLY_PULSE: &str = "00000000-0000-7000-8000-0000000000a3";
const OWN: &str = "00000000-0000-7000-8000-0000000000a4";
const LINE: &str = "00000000-0000-7000-8000-0000000000a5";

fn conv_id(id: &str) -> ConversationId {
    ConversationId::parse(id).unwrap()
}

fn msg_id(id: &str) -> MessageId {
    MessageId::parse(id).unwrap()
}

fn user_id(id: &str) -> UserId {
    UserId::parse(id).unwrap()
}

/// A project channel: Alex's root (1), Bea's reply naming @alex (2), Pulse's
/// reply (3), Bea's own message in the channel (4) and a system line in the thread (5).
/// A message's storage key is its text.
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
                 INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{ALEX}', '{company}', 'alex', '$argon2id$x', 1, 1), ('{BEA}', '{company}', 'bea', '$argon2id$x', 1, 1);
                 INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES ('{CHANNEL}', 'group', '{P}', 'Alpha', 1, 10);
                 INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id,
                                    metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms)
                   VALUES ('{EVENT}', 'conversation', '{CHANNEL}', 1, 'message.user_committed', 1, 'user', 'c', '{{}}', 'k', 12, 12);
                 INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES
                   ('b1', 'Root', 's', 0, 'text/plain', 1), ('b2', 'Look @alex', 's', 0, 'text/plain', 1), ('b3', 'Done', 's', 0, 'text/plain', 1),
                   ('b4', 'Hi', 's', 0, 'text/plain', 1), ('b5', 'Limit reached', 's', 0, 'text/plain', 1);
                 INSERT INTO messages(id, conversation_id, ordinal, role, content_blob_id, source_event_id, status, input_kind, created_at_ms, author_user_id, thread_parent_id, meta_json) VALUES
                   ('{ROOT}', '{CHANNEL}', 1, 'user', 'b1', '{EVENT}', 'complete', 'text', 100, '{ALEX}', NULL, NULL),
                   ('{REPLY_BEA}', '{CHANNEL}', 2, 'user', 'b2', '{EVENT}', 'complete', 'text', 200, '{BEA}', '{ROOT}', NULL),
                   ('{REPLY_PULSE}', '{CHANNEL}', 3, 'tool', 'b3', '{EVENT}', 'complete', 'tool', 300, NULL, '{ROOT}', '{{\"kind\":\"group\",\"from\":\"Pulse\"}}'),
                   ('{OWN}', '{CHANNEL}', 4, 'user', 'b4', '{EVENT}', 'complete', 'text', 400, '{BEA}', NULL, NULL),
                   ('{LINE}', '{CHANNEL}', 5, 'tool', 'b5', '{EVENT}', 'complete', 'tool', 500, NULL, '{ROOT}', '{{\"kind\":\"status\",\"from\":\"Aurelius\"}}');"
            ))?;
            Ok(())
        })
        .unwrap();
    (dir, store)
}

fn text(key: &str) -> Option<String> {
    Some(key.to_owned())
}

fn ids(messages: &[aurelius_domain::views::MessageView]) -> Vec<String> {
    messages.iter().map(|m| m.id.to_string()).collect()
}

fn decorated(store: &Store, reader: Option<UserId>) -> Vec<aurelius_domain::views::MessageView> {
    store
        .read(|c| {
            let mut messages = conversations::list_messages(c, conv_id(CHANNEL), &text)?;
            threads::decorate(c, conv_id(CHANNEL), reader, &mut messages)?;
            Ok(messages)
        })
        .unwrap()
}

#[test]
fn the_channels_list_leaves_out_replies_and_a_thread_is_its_root_then_its_replies() {
    let (_dir, store) = channel();
    let channel: ConversationId = conv_id(CHANNEL);
    let list = store.read(|c| conversations::list_messages(c, channel, &text)).unwrap();
    assert_eq!(ids(&list), [ROOT, OWN]);
    assert!(list.iter().all(|m| m.thread_parent.is_none()));
    let thread = store.read(|c| conversations::list_thread(c, channel, msg_id(ROOT), &text)).unwrap();
    assert_eq!(ids(&thread), [ROOT, REPLY_BEA, REPLY_PULSE, LINE]);
    assert_eq!(thread[0].thread_parent, None);
    assert!(thread[1..].iter().all(|m| m.thread_parent == Some(msg_id(ROOT))));
    let place = store.read(|c| threads::place(c, msg_id(REPLY_BEA))).unwrap().unwrap();
    assert_eq!(place, threads::MessagePlace { conversation_id: channel, thread_parent: Some(msg_id(ROOT)) });
}

#[test]
fn a_root_counts_its_replies_and_the_readers_unread_until_they_read_the_thread() {
    let (_dir, store) = channel();
    let (alex, bea): (UserId, UserId) = (user_id(ALEX), user_id(BEA));
    let root = |reader| decorated(&store, reader).into_iter().find(|m| m.id == msg_id(ROOT)).unwrap();
    // The system line is in the thread but not counted.
    let seen = root(Some(alex));
    assert_eq!((seen.thread_reply_count, seen.thread_last_reply_ms, seen.thread_unread), (Some(2), Some(300), Some(2)));
    // Her own reply is never unread for Bea.
    assert_eq!(root(Some(bea)).thread_unread, Some(1));
    assert_eq!(root(None).thread_unread, None, "no reader, no unread");
    store.write(move |tx| threads::mark_read(tx, alex, msg_id(ROOT))).unwrap();
    let read = root(Some(alex));
    assert_eq!((read.thread_reply_count, read.thread_unread), (Some(2), None));
    assert_eq!(store.read(|c| threads::read_through(c, alex, msg_id(ROOT))).unwrap(), Some(5));
    // A message without replies has no counters.
    let own = decorated(&store, Some(alex)).into_iter().find(|m| m.id == msg_id(OWN)).unwrap();
    assert_eq!((own.thread_reply_count, own.thread_last_reply_ms, own.thread_unread), (None, None, None));
}

#[test]
fn a_root_names_at_most_three_distinct_repliers_most_recent_first() {
    let (_dir, store) = channel();
    let repliers = |id: &str| decorated(&store, None).into_iter().find(|m| m.id == msg_id(id)).unwrap().thread_repliers;
    // Pulse (3) after Bea (2); the system line (5) is no replier.
    assert_eq!(repliers(ROOT), ["Pulse", "bea"]);
    assert!(repliers(OWN).is_empty(), "no replies, no repliers");
    // Alex (6), Atlas (7), then Bea again (8): Bea once, as the latest, and Pulse drops out.
    store
        .write(|tx| {
            tx.execute_batch(&format!(
                "INSERT INTO messages(id, conversation_id, ordinal, role, content_blob_id, source_event_id, status, input_kind, created_at_ms, author_user_id, thread_parent_id, meta_json) VALUES
                   ('00000000-0000-7000-8000-0000000000a6', '{CHANNEL}', 6, 'user', 'b3', '{EVENT}', 'complete', 'text', 600, '{ALEX}', '{ROOT}', NULL),
                   ('00000000-0000-7000-8000-0000000000a7', '{CHANNEL}', 7, 'tool', 'b3', '{EVENT}', 'complete', 'tool', 700, NULL, '{ROOT}', '{{\"kind\":\"group\",\"from\":\"Atlas\"}}'),
                   ('00000000-0000-7000-8000-0000000000a8', '{CHANNEL}', 8, 'user', 'b3', '{EVENT}', 'complete', 'text', 800, '{BEA}', '{ROOT}', NULL);"
            ))?;
            Ok(())
        })
        .unwrap();
    assert_eq!(repliers(ROOT), ["bea", "Atlas", "alex"]);
    // The thread's own read names them on its root too.
    let thread = store
        .read(|c| {
            let mut messages = conversations::list_thread(c, conv_id(CHANNEL), msg_id(ROOT), &text)?;
            threads::decorate(c, conv_id(CHANNEL), None, &mut messages)?;
            Ok(messages)
        })
        .unwrap();
    assert_eq!(thread[0].thread_repliers, ["bea", "Atlas", "alex"]);
    assert!(thread[1..].iter().all(|m| m.thread_repliers.is_empty()), "a reply names no repliers");
}

#[test]
fn a_thread_read_marker_never_goes_back() {
    let (_dir, store) = channel();
    let alex: UserId = user_id(ALEX);
    store.write(move |tx| threads::mark_read(tx, alex, msg_id(ROOT))).unwrap();
    store.write(|tx| Ok(tx.execute("UPDATE thread_reads SET read_through = 9", [])?)).unwrap();
    store.write(move |tx| threads::mark_read(tx, alex, msg_id(ROOT))).unwrap();
    assert_eq!(store.read(|c| threads::read_through(c, alex, msg_id(ROOT))).unwrap(), Some(9));
}

#[test]
fn replies_do_not_count_in_the_channels_unread_but_an_unread_mention_in_one_does() {
    let (_dir, store) = channel();
    let alex: UserId = user_id(ALEX);
    let unread = || store.read(|c| discussions::unread(c, alex, "alex", conv_id(CHANNEL), &text)).unwrap();
    // Bea's message in the channel; Bea's reply names @alex.
    assert_eq!(unread(), (1, 1));
    // Reading the channel does not read the thread.
    store.write(move |tx| discussions::mark_read(tx, alex, conv_id(CHANNEL))).unwrap();
    assert_eq!(unread(), (0, 1));
    store.write(move |tx| threads::mark_read(tx, alex, msg_id(ROOT))).unwrap();
    assert_eq!(unread(), (0, 0));
}

#[test]
fn a_reaction_is_once_per_board_member_and_emoji_and_names_who_oldest_first() {
    let (_dir, store) = channel();
    let (alex, bea): (UserId, UserId) = (user_id(ALEX), user_id(BEA));
    let react = |user: UserId, emoji: &'static str, on: bool| store.write(move |tx| threads::react(tx, msg_id(ROOT), user, emoji, on)).unwrap();
    assert!(react(bea, "👍", true));
    assert!(!react(bea, "👍", true), "on twice is once");
    assert!(react(alex, "🎉", true));
    assert!(react(alex, "👍", true));
    let root = |reader| decorated(&store, Some(reader)).into_iter().find(|m| m.id == msg_id(ROOT)).unwrap();
    let seen = root(alex).reactions;
    assert_eq!(seen.iter().map(|r| (r.emoji.as_str(), r.count, r.mine)).collect::<Vec<_>>(), [("👍", 2, true), ("🎉", 1, true)]);
    assert_eq!(seen[0].by, ["bea", "alex"]);
    assert_eq!(root(bea).reactions.iter().map(|r| r.mine).collect::<Vec<_>>(), [true, false]);
    assert!(react(alex, "👍", false));
    assert!(!react(alex, "👍", false), "off twice is off");
    let after = root(alex).reactions;
    assert_eq!(after.iter().map(|r| (r.emoji.as_str(), r.count, r.mine)).collect::<Vec<_>>(), [("👍", 1, false), ("🎉", 1, true)]);
    assert_eq!(after[0].by, ["bea"]);
    // Off for everyone: the emoji is gone.
    assert!(react(bea, "👍", false));
    assert_eq!(root(alex).reactions.len(), 1);
}

#[test]
fn only_a_channel_or_a_tickets_discussion_is_a_discussion_and_an_archived_projects_is_closed() {
    let (_dir, store) = channel();
    assert_eq!(store.read(|c| threads::discussion_closed(c, conv_id(CHANNEL))).unwrap(), Some(false));
    assert_eq!(store.read(|c| threads::discussion_closed(c, ConversationId::new())).unwrap(), None);
    store.write(|tx| Ok(tx.execute("UPDATE projects SET status = 'archived'", [])?)).unwrap();
    assert_eq!(store.read(|c| threads::discussion_closed(c, conv_id(CHANNEL))).unwrap(), Some(true));
    let unknown = store.read(|c| threads::place(c, MessageId::new())).unwrap();
    assert_eq!(unknown, None);
}
