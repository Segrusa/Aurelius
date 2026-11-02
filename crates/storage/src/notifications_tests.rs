//! The Inbox per board member (AUR-FA-A0): reading or dismissing a notification
//! for everyone counts only for who did it; a board member's own works as before.

use crate::{Store, timeline};

const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const BEA: &str = "00000000-0000-7000-8000-000000000005";
const FOR_ALL: &str = "n-all";
const FOR_ALEX: &str = "n-alex";
const FOR_BEA: &str = "n-bea";

/// Alex and Bea, a notification for everyone and one of each of their own.
fn inbox() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|tx| {
            let company: String = tx.query_row("SELECT id FROM company", [], |r| r.get(0))?;
            tx.execute_batch(&format!(
                "INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{ALEX}', '{company}', 'alex', '$argon2id$x', 1, 1), ('{BEA}', '{company}', 'bea', '$argon2id$x', 1, 1);"
            ))?;
            add(tx, FOR_ALL, None)?;
            add(tx, FOR_ALEX, Some(ALEX))?;
            add(tx, FOR_BEA, Some(BEA))
        })
        .unwrap();
    (dir, store)
}

/// A notification `id` with its own event, as `timeline::notify` leaves it.
fn add(tx: &rusqlite::Transaction<'_>, id: &str, user: Option<&str>) -> Result<(), crate::StorageError> {
    tx.execute(
        "INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms)
         VALUES (?1, 'task', ?1, 1, 'task.succeeded', 1, 'system', ?1, '{}', ?1, 10, 10)",
        [id],
    )?;
    tx.execute(
        "INSERT INTO notifications(id, event_id, dedupe_key, severity, target_json, created_at_ms, user_id) VALUES (?1, ?1, ?1, 'info', '{}', 10, ?2)",
        rusqlite::params![id, user],
    )?;
    Ok(())
}

/// (id, read) of what `user`'s Inbox shows, sorted by id.
fn shown(store: &Store, user: &str) -> Vec<(String, bool)> {
    let mut rows: Vec<_> = store.read(|c| timeline::list_notifications_for(c, 50, Some(user))).unwrap().into_iter().map(|n| (n.id, n.read)).collect();
    rows.sort();
    rows
}

fn row(id: &str, read: bool) -> (String, bool) {
    (id.to_owned(), read)
}

fn unread(store: &Store, user: &str) -> u64 {
    store.read(|c| timeline::unread_count_for(c, Some(user))).unwrap()
}

fn mark_all_read(store: &Store, user: &'static str) -> usize {
    store.write(|tx| timeline::mark_all_read_for(tx, Some(user))).unwrap()
}

fn dismiss(store: &Store, id: &'static str, user: &'static str) -> usize {
    store.write(|tx| timeline::dismiss_for(tx, id, Some(user))).unwrap()
}

fn dismiss_all(store: &Store, user: &'static str) -> usize {
    store.write(|tx| timeline::dismiss_all_for(tx, Some(user))).unwrap()
}

#[test]
fn marking_all_read_leaves_a_notification_for_everyone_unread_for_another_board_member() {
    let (_dir, store) = inbox();
    assert_eq!(mark_all_read(&store, ALEX), 2, "the one for everyone and Alex's own");
    assert_eq!(shown(&store, ALEX), [row(FOR_ALEX, true), row(FOR_ALL, true)]);
    assert_eq!(unread(&store, ALEX), 0);
    assert_eq!(shown(&store, BEA), [row(FOR_ALL, false), row(FOR_BEA, false)]);
    assert_eq!(unread(&store, BEA), 2);
    assert_eq!(mark_all_read(&store, ALEX), 0, "nothing new to read");
    assert_eq!(mark_all_read(&store, BEA), 2, "Bea reads it for herself too");
    assert_eq!(unread(&store, BEA), 0);
}

#[test]
fn dismissing_a_notification_for_everyone_removes_it_only_for_that_board_member() {
    let (_dir, store) = inbox();
    assert_eq!(dismiss(&store, FOR_ALL, ALEX), 1);
    assert_eq!(shown(&store, ALEX), [row(FOR_ALEX, false)]);
    assert_eq!(unread(&store, ALEX), 1);
    assert_eq!(shown(&store, BEA), [row(FOR_ALL, false), row(FOR_BEA, false)]);
    assert_eq!(unread(&store, BEA), 2);
    assert_eq!(dismiss(&store, FOR_ALL, ALEX), 0, "already gone for Alex");
}

#[test]
fn dismissing_all_removes_a_notification_for_everyone_only_for_that_board_member() {
    let (_dir, store) = inbox();
    assert_eq!(dismiss_all(&store, ALEX), 2);
    assert!(shown(&store, ALEX).is_empty());
    assert_eq!(unread(&store, ALEX), 0);
    assert_eq!(shown(&store, BEA), [row(FOR_ALL, false), row(FOR_BEA, false)]);
    assert_eq!(unread(&store, BEA), 2);
    assert_eq!(dismiss_all(&store, ALEX), 0, "nothing left for Alex");
}

#[test]
fn a_dismissed_notification_for_everyone_is_not_marked_read_afterwards() {
    let (_dir, store) = inbox();
    dismiss(&store, FOR_ALL, ALEX);
    assert_eq!(mark_all_read(&store, ALEX), 1, "only Alex's own");
    assert_eq!(shown(&store, ALEX), [row(FOR_ALEX, true)]);
    assert_eq!(unread(&store, BEA), 2);
}

#[test]
fn a_read_notification_for_everyone_can_still_be_dismissed_by_that_board_member() {
    let (_dir, store) = inbox();
    mark_all_read(&store, ALEX);
    assert_eq!(dismiss(&store, FOR_ALL, ALEX), 1);
    assert_eq!(shown(&store, ALEX), [row(FOR_ALEX, true)]);
    assert_eq!(shown(&store, BEA), [row(FOR_ALL, false), row(FOR_BEA, false)]);
}

#[test]
fn a_board_members_own_notification_is_read_and_removed_on_its_row_and_only_by_them() {
    let (_dir, store) = inbox();
    assert_eq!(dismiss(&store, FOR_BEA, ALEX), 0, "not Alex's");
    assert_eq!(unread(&store, BEA), 2);
    mark_all_read(&store, BEA);
    let read: Option<i64> = store.read(|c| Ok(c.query_row("SELECT read_at_ms FROM notifications WHERE id = ?1", [FOR_BEA], |r| r.get(0))?)).unwrap();
    assert!(read.is_some(), "as before: on its row");
    assert_eq!(dismiss(&store, FOR_BEA, BEA), 1);
    let left: i64 = store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM notifications WHERE id = ?1", [FOR_BEA], |r| r.get(0))?)).unwrap();
    assert_eq!(left, 0, "as before: the row goes");
    assert_eq!(shown(&store, ALEX), [row(FOR_ALEX, false), row(FOR_ALL, false)], "Alex's Inbox did not change");
}

#[test]
fn a_notification_for_everyone_read_before_the_upgrade_stays_read_for_everyone() {
    let (_dir, store) = inbox();
    store.write(|tx| Ok(tx.execute("UPDATE notifications SET read_at_ms = 5 WHERE id = ?1", [FOR_ALL])?)).unwrap();
    assert_eq!(shown(&store, BEA), [row(FOR_ALL, true), row(FOR_BEA, false)]);
    assert_eq!(unread(&store, BEA), 1);
    assert_eq!(mark_all_read(&store, BEA), 1, "only Bea's own was still unread");
    assert_eq!(dismiss(&store, FOR_ALL, BEA), 1, "and it can still be dismissed");
    assert_eq!(shown(&store, ALEX), [row(FOR_ALEX, false), row(FOR_ALL, true)]);
}

#[test]
fn without_a_board_member_nothing_is_read_or_removed() {
    let (_dir, store) = inbox();
    assert_eq!(store.write(|tx| timeline::mark_all_read_for(tx, None)).unwrap(), 0);
    assert_eq!(store.write(|tx| timeline::dismiss_for(tx, FOR_ALL, None)).unwrap(), 0);
    assert_eq!(store.write(|tx| timeline::dismiss_all_for(tx, None)).unwrap(), 0);
    assert_eq!(shown(&store, BEA), [row(FOR_ALL, false), row(FOR_BEA, false)]);
}

#[test]
fn a_notification_for_everyone_dismissed_by_one_keeps_its_row_for_push_routing() {
    let (_dir, store) = inbox();
    dismiss_all(&store, ALEX);
    let all = store.read(|c| timeline::list_notifications(c, 50)).unwrap();
    assert!(all.iter().any(|n| n.id == FOR_ALL && !n.read));
}

// ---- One row read (notification.mark_read) ------------------------------------

fn mark_read(store: &Store, id: &'static str, user: &'static str) -> usize {
    store.write(|tx| timeline::mark_read_for(tx, id, Some(user))).unwrap()
}

#[test]
fn a_row_for_everyone_read_by_one_stays_unread_for_the_other() {
    let (_dir, store) = inbox();
    assert_eq!(mark_read(&store, FOR_ALL, ALEX), 1);
    assert_eq!(shown(&store, ALEX), [row(FOR_ALEX, false), row(FOR_ALL, true)]);
    assert_eq!(shown(&store, BEA), [row(FOR_ALL, false), row(FOR_BEA, false)]);
    assert_eq!(unread(&store, BEA), 2);
}

#[test]
fn only_the_row_named_is_read_and_a_second_time_counts_nothing() {
    let (_dir, store) = inbox();
    assert_eq!(mark_read(&store, FOR_ALEX, ALEX), 1);
    assert_eq!(shown(&store, ALEX), [row(FOR_ALEX, true), row(FOR_ALL, false)]);
    assert_eq!(mark_read(&store, FOR_ALEX, ALEX), 0);
    assert_eq!(mark_read(&store, FOR_ALL, ALEX), 1);
    assert_eq!(mark_read(&store, FOR_ALL, ALEX), 0);
}

#[test]
fn no_one_reads_a_row_of_someone_else_or_an_unknown_one() {
    let (_dir, store) = inbox();
    assert_eq!(mark_read(&store, FOR_BEA, ALEX), 0);
    assert_eq!(shown(&store, BEA), [row(FOR_ALL, false), row(FOR_BEA, false)]);
    assert_eq!(mark_read(&store, "no-such-row", ALEX), 0);
    assert_eq!(store.write(|tx| timeline::mark_read_for(tx, FOR_ALL, None)).unwrap(), 0);
}

#[test]
fn a_dismissed_row_cannot_be_read() {
    let (_dir, store) = inbox();
    dismiss(&store, FOR_ALL, ALEX);
    assert_eq!(mark_read(&store, FOR_ALL, ALEX), 0);
}

// ---- The rows of phase A (AUR-FA-S1) ---------------------------------------

const T1: &str = "00000000-0000-7000-8000-000000000011";
const C1: &str = "00000000-0000-7000-8000-0000000000c1";
const M1: &str = "00000000-0000-7000-8000-0000000000a1";
const EV: &str = "00000000-0000-7000-8000-000000000031";

/// `inbox` with ticket #7, its discussion, a message in it and the event of a row.
fn inbox_with_ticket() -> (tempfile::TempDir, Store) {
    let (dir, store) = inbox();
    store
        .write(|tx| {
            let company: String = tx.query_row("SELECT id FROM company", [], |r| r.get(0))?;
            let global: String = tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0))?;
            let column: String = tx.query_row("SELECT id FROM board_columns WHERE kind = 'review'", [], |r| r.get(0))?;
            tx.execute_batch(&format!(
                "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', 'p');
                 INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('p', 'ps', 'Alpha', 'active', 1, 1, 1);
                 INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES ('{C1}', 'ticket', 'p', '#7 Fix', 1, 10);
                 INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, priority, created_by_user, version, created_at_ms, updated_at_ms, conversation_id, login_user)
                   VALUES ('{T1}', '{company}', 'p', 7, 'Fix', '{column}', 1.0, 'normal', '{ALEX}', 1, 10, 10, '{C1}', '{ALEX}');
                 INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms)
                   VALUES ('{EV}', 'ticket', '{T1}', 1, 'ticket.updated', 1, 'user', '{T1}', '{{}}', 'ev', 11, 11);
                 INSERT INTO messages(id, conversation_id, ordinal, role, source_event_id, status, input_kind, created_at_ms) VALUES ('{M1}', '{C1}', 1, 'user', '{EV}', 'complete', 'text', 11);"
            ))?;
            Ok(())
        })
        .unwrap();
    (dir, store)
}

fn new_row(user: &str) -> timeline::NewNotification<'static> {
    timeline::NewNotification {
        event_id: aurelius_domain::EventId::parse(EV).unwrap(),
        user: aurelius_domain::UserId::parse(user).unwrap(),
        severity: "info",
        title: "Marie made you responsible",
        reason: "responsible",
        ticket: Some(aurelius_domain::TicketId::parse(T1).unwrap()),
        conversation: Some(aurelius_domain::ConversationId::parse(C1).unwrap()),
        message: Some(aurelius_domain::MessageId::parse(M1).unwrap()),
    }
}

fn read_row(store: &Store, user: &str, id: &str) -> aurelius_domain::views::NotificationView {
    store.read(|c| timeline::list_notifications_for(c, 50, Some(user))).unwrap().into_iter().find(|n| n.id == id).unwrap()
}

#[test]
fn a_written_row_reads_back_with_all_it_is_about_and_is_only_for_that_board_member() {
    let (_dir, store) = inbox_with_ticket();
    let id = store.write(|tx| timeline::insert_notification(tx, &new_row(BEA))).unwrap().unwrap();

    let row = read_row(&store, BEA, &id);
    assert_eq!(row.ticket_id.unwrap().to_string(), T1);
    assert_eq!(row.ticket_number, Some(7));
    assert_eq!(row.conversation_id.unwrap().to_string(), C1);
    assert_eq!(row.message_id.unwrap().to_string(), M1);
    assert_eq!(row.reason.as_deref(), Some("responsible"));
    assert_eq!((row.severity.as_str(), row.title.as_str(), row.target_type.as_str()), ("info", "Marie made you responsible", "ticket.updated"));
    assert_eq!((row.merged_count, row.needs_you, row.read), (0, false, false), "needs_you is worked out by the engine, later");

    assert!(!store.read(|c| timeline::list_notifications_for(c, 50, Some(ALEX))).unwrap().iter().any(|n| n.id == id));
}

#[test]
fn one_event_writes_one_row_per_board_member_and_never_two_for_the_same() {
    let (_dir, store) = inbox_with_ticket();
    assert!(store.write(|tx| timeline::insert_notification(tx, &new_row(BEA))).unwrap().is_some());
    assert!(store.write(|tx| timeline::insert_notification(tx, &new_row(ALEX))).unwrap().is_some());
    assert_eq!(store.write(|tx| timeline::insert_notification(tx, &new_row(BEA))).unwrap(), None);
}

#[test]
fn a_row_for_an_event_that_does_not_exist_is_not_written() {
    let (_dir, store) = inbox_with_ticket();
    let row = timeline::NewNotification { event_id: aurelius_domain::EventId::new(), ..new_row(BEA) };
    assert_eq!(store.write(move |tx| timeline::insert_notification(tx, &row)).unwrap(), None);
}

#[test]
fn a_reason_longer_than_the_check_is_refused() {
    let (_dir, store) = inbox_with_ticket();
    let refused = store.write(|tx| {
        let long = "x".repeat(33);
        timeline::insert_notification(tx, &timeline::NewNotification { reason: &long, ..new_row(BEA) })
    });
    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn the_merged_count_of_the_column_reaches_the_view() {
    let (_dir, store) = inbox_with_ticket();
    let id = store.write(|tx| timeline::insert_notification(tx, &new_row(BEA))).unwrap().unwrap();
    let update = id.clone();
    store.write(move |tx| Ok(tx.execute("UPDATE notifications SET merged_count = 3 WHERE id = ?1", [&update])?)).unwrap();
    assert_eq!(read_row(&store, BEA, &id).merged_count, 3);
}

#[test]
fn a_row_of_before_phase_a_keeps_the_title_of_its_event() {
    let (_dir, store) = inbox();
    assert_eq!(read_row(&store, ALEX, FOR_ALEX).title, "Result ready");
}
