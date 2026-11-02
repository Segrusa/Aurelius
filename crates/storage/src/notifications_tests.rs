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

// ---- merging, push, the gate and the A0 point (AUR-FA-F2) ----

/// An event on the ticket, through the real `events::append`, so `notify` decides.
fn append(tx: &rusqlite::Transaction<'_>, event_type: &'static str, version: u64, metadata: serde_json::Value) -> Result<u64, crate::StorageError> {
    Ok(crate::events::append(tx, crate::events::NewEvent {
        aggregate_type: "ticket",
        aggregate_id: T1.to_owned(),
        aggregate_version: version + 1,
        event_type,
        project_id: None,
        actor_kind: aurelius_domain::ActorKind::System,
        actor_id: None,
        correlation_id: T1.to_owned(),
        causation_id: None,
        payload_blob_id: None,
        metadata,
        dedupe_key: format!("t:{event_type}:{version}"),
    })?
    .seq)
}

/// Bea follows the ticket; alex (who is not the actor below) does too.
fn followed_ticket() -> (tempfile::TempDir, Store) {
    let (dir, store) = inbox_with_ticket();
    store
        .write(|tx| {
            tx.execute_batch(&format!(
                "INSERT INTO ticket_followers(ticket_id, user_id, reason, followed_at_ms) VALUES ('{T1}', '{BEA}', 'manual', 1), ('{T1}', '{ALEX}', 'created', 1);"
            ))?;
            Ok(())
        })
        .unwrap();
    (dir, store)
}

fn employee_by() -> serde_json::Value {
    serde_json::json!({ "by": { "kind": "employee", "employee_id": "00000000-0000-7000-8000-00000000000e" } })
}

fn alex_by() -> serde_json::Value {
    serde_json::json!({ "by": { "kind": "board_member", "user_id": ALEX } })
}

fn mention_of(user: &str) -> serde_json::Value {
    let mut m = alex_by();
    m["mentioned_users"] = serde_json::json!([user]);
    m["changed"] = serde_json::json!(["description"]);
    m
}

fn rows_for(store: &Store, user: &str) -> Vec<aurelius_domain::views::NotificationView> {
    store.read(|c| timeline::list_notifications_for(c, 50, Some(user))).unwrap().into_iter().filter(|n| n.ticket_id.is_some()).collect()
}

fn put_in(store: &Store, kind: &'static str) {
    store.write(move |tx| Ok(tx.execute(&format!("UPDATE tickets SET column_id = (SELECT id FROM board_columns WHERE kind = '{kind}') WHERE id = '{T1}'"), [])?)).unwrap();
}

#[test]
fn a_ticket_event_writes_one_row_per_follower_and_none_for_the_actor() {
    let (_dir, store) = followed_ticket();
    store.write(|tx| append(tx, "ticket.review", 1, alex_by())).unwrap();
    assert_eq!(rows_for(&store, BEA).len(), 1);
    assert!(rows_for(&store, ALEX).is_empty(), "alex did it");
}

#[test]
fn a_disabled_follower_or_responsible_gets_no_row() {
    let (_dir, store) = followed_ticket();
    store.write(|tx| Ok(tx.execute(&format!("UPDATE users SET disabled_at_ms = 5 WHERE id = '{BEA}'"), [])?)).unwrap();
    store.write(|tx| Ok(tx.execute(&format!("UPDATE tickets SET responsible_user_id = '{BEA}' WHERE id = '{T1}'"), [])?)).unwrap();
    store.write(|tx| append(tx, "ticket.review", 1, employee_by())).unwrap();
    assert!(rows_for(&store, BEA).is_empty());
    assert_eq!(rows_for(&store, ALEX).len(), 1);
    store.write(|tx| append(tx, "ticket.updated", 2, mention_of(BEA))).unwrap();
    assert!(rows_for(&store, BEA).is_empty(), "nor for a name in a text");
}

#[test]
fn no_event_gives_a_row_on_a_closed_ticket() {
    let responsible = serde_json::json!({ "by": { "kind": "board_member", "user_id": ALEX }, "changed": ["responsible"], "responsible_user_id": BEA });
    let login = serde_json::json!({ "needs": "login", "for_user": BEA, "changed": [] });
    let cases: [(&'static str, serde_json::Value); 6] = [
        ("ticket.updated", responsible),
        ("ticket.updated", mention_of(BEA)),
        ("ticket.plan_review", employee_by()),
        ("ticket.review", employee_by()),
        ("ticket.sent_back", alex_by()),
        ("ticket.updated", login),
    ];
    for closed in ["done", "archived"] {
        for (event_type, metadata) in &cases {
            let (_dir, store) = followed_ticket();
            match closed {
                "done" => put_in(&store, "done"),
                _ => {
                    store.write(|tx| Ok(tx.execute(&format!("UPDATE tickets SET archived_at_ms = 5 WHERE id = '{T1}'"), [])?)).unwrap();
                }
            }
            let (event_type, metadata) = (*event_type, metadata.clone());
            store.write(move |tx| append(tx, event_type, 1, metadata)).unwrap();
            let rows: i64 = store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM notifications WHERE ticket_id IS NOT NULL", [], |r| r.get(0))?)).unwrap();
            assert_eq!(rows, 0, "{event_type} on {closed}");
        }
    }
}

#[test]
fn a_review_event_gives_no_row_when_the_ticket_is_not_in_that_column() {
    let (_dir, store) = followed_ticket();
    put_in(&store, "doing");
    store.write(|tx| append(tx, "ticket.review", 1, employee_by())).unwrap();
    store.write(|tx| append(tx, "ticket.plan_review", 2, employee_by())).unwrap();
    assert!(rows_for(&store, BEA).is_empty());
}

#[test]
fn a_ticket_row_leaves_with_its_reason_and_the_count_agrees() {
    let (_dir, store) = followed_ticket();
    store.write(|tx| append(tx, "ticket.review", 1, employee_by())).unwrap();
    // Next to the two rows `inbox` gives Bea.
    assert_eq!(unread(&store, BEA), 3);
    put_in(&store, "doing");
    assert!(rows_for(&store, BEA).is_empty(), "left Review");
    assert_eq!(unread(&store, BEA), 2);
    put_in(&store, "review");
    assert_eq!(rows_for(&store, BEA).len(), 1, "it follows the state of now");
    put_in(&store, "done");
    assert!(rows_for(&store, BEA).is_empty(), "closed");
}

#[test]
fn a_mention_on_a_ticket_row_stays_when_the_review_of_another_row_ended() {
    let (_dir, store) = followed_ticket();
    store.write(|tx| append(tx, "ticket.updated", 1, mention_of(BEA))).unwrap();
    put_in(&store, "doing");
    assert_eq!(rows_for(&store, BEA).len(), 1, "a mention does not hang on a column");
}

#[test]
fn the_head_is_the_most_important_reason_and_never_the_latest() {
    let (_dir, store) = followed_ticket();
    let responsible = serde_json::json!({ "by": { "kind": "board_member", "user_id": ALEX }, "changed": ["responsible"], "responsible_user_id": BEA });
    let steps: [(&'static str, serde_json::Value, &str, u32); 5] = [
        ("ticket.sent_back", alex_by(), "sent_back", 0),
        ("ticket.updated", responsible, "sent_back", 1),
        ("ticket.updated", mention_of(BEA), "mention", 2),
        ("ticket.review", employee_by(), "review", 3),
        ("ticket.updated", mention_of(BEA), "review", 4),
    ];
    for (i, (event_type, metadata, head, merged)) in steps.into_iter().enumerate() {
        store.write(move |tx| append(tx, event_type, i as u64 + 1, metadata)).unwrap();
        let rows = rows_for(&store, BEA);
        assert_eq!(rows.len(), 1, "{rows:#?}");
        assert_eq!((rows[0].reason.as_deref(), rows[0].merged_count), (Some(head), merged), "step {i}");
    }
    let login = serde_json::json!({ "needs": "login", "for_user": BEA, "changed": [], "subject": "Fix" });
    store.write(move |tx| append(tx, "ticket.updated", 6, login)).unwrap();
    let rows = rows_for(&store, BEA);
    assert_eq!((rows[0].reason.as_deref(), rows[0].severity.as_str(), rows[0].needs_you, rows[0].merged_count), (Some("login"), "action", true, 5));
    let last = serde_json::json!({ "by": { "kind": "board_member", "user_id": ALEX } });
    store.write(move |tx| append(tx, "ticket.sent_back", 7, last)).unwrap();
    assert_eq!(rows_for(&store, BEA)[0].reason.as_deref(), Some("login"), "nothing is above a login");
}

#[test]
fn a_push_goes_for_a_first_row_and_for_a_mention_or_responsibility_but_not_for_any_other_merge() {
    let (_dir, store) = followed_ticket();
    let pushed = |seq: u64| store.read(move |c| crate::timeline::notifications_at_seq(c, seq)).unwrap();
    let first = store.write(|tx| append(tx, "ticket.sent_back", 1, alex_by())).unwrap();
    assert_eq!(pushed(first).len(), 1, "the first row");
    let again = store.write(|tx| append(tx, "ticket.sent_back", 2, alex_by())).unwrap();
    assert!(pushed(again).is_empty(), "merged silently");
    let review = store.write(|tx| append(tx, "ticket.review", 3, employee_by())).unwrap();
    assert!(pushed(review).iter().all(|f| f.user_id.as_deref() != Some(BEA)), "a merge that takes the head still is a merge");
    let mention = store.write(|tx| append(tx, "ticket.updated", 4, mention_of(BEA))).unwrap();
    let found = pushed(mention);
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].view.title.as_str(), found[0].view.reason.as_deref()), ("alex mentioned you", Some("mention")), "in its own words, under the review's row");
    assert_eq!(rows_for(&store, BEA)[0].reason.as_deref(), Some("review"));
    let responsible = serde_json::json!({ "by": { "kind": "board_member", "user_id": ALEX }, "changed": ["responsible"], "responsible_user_id": BEA });
    let handed = store.write(move |tx| append(tx, "ticket.updated", 5, responsible)).unwrap();
    assert_eq!(pushed(handed)[0].view.title, "alex made you responsible");
}

#[test]
fn one_event_pushes_a_row_per_person_each_to_their_own_devices() {
    let (_dir, store) = followed_ticket();
    let (alex_phone, bea_phone) = (aurelius_domain::ClientId::new(), aurelius_domain::ClientId::new());
    store
        .write(move |tx| {
            for (id, user) in [(alex_phone, ALEX), (bea_phone, BEA)] {
                crate::remote::create_client(tx, id, aurelius_domain::PrincipalId::new(), "phone", "ios", "v", &aurelius_domain::remote::Capability::DEVICE_DEFAULT)?;
                tx.execute("UPDATE remote_clients SET user_id = ?2 WHERE id = ?1", [id.to_string(), user.to_owned()])?;
                crate::remote::set_push(tx, id, "apns", "token", "sandbox")?;
            }
            Ok(())
        })
        .unwrap();
    let seq = store.write(|tx| append(tx, "ticket.review", 1, employee_by())).unwrap();
    let mut pushes = store.read(move |c| crate::remote::pushes_for_seq(c, seq)).unwrap();
    pushes.sort_by_key(|(f, _)| f.user_id.clone());
    let got: Vec<_> = pushes.iter().map(|(f, t)| (f.user_id.clone().unwrap(), t.iter().map(|t| t.client_id).collect::<Vec<_>>())).collect();
    assert_eq!(got, vec![(ALEX.to_owned(), vec![alex_phone]), (BEA.to_owned(), vec![bea_phone])]);
}

#[test]
fn more_than_a_page_of_rows_does_not_lose_a_push() {
    let (_dir, store) = followed_ticket();
    let seq = store.write(|tx| append(tx, "ticket.review", 1, employee_by())).unwrap();
    store
        .write(|tx| {
            for i in 0..60 {
                add(tx, &format!("later-{i}"), Some(ALEX))?;
                tx.execute("UPDATE notifications SET created_at_ms = ?2 WHERE id = ?1", rusqlite::params![format!("later-{i}"), crate::now_ms() + 1_000])?;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(store.read(move |c| crate::timeline::notifications_at_seq(c, seq)).unwrap().len(), 2);
}

#[test]
fn a_personal_row_is_not_hidden_because_its_owner_linked_a_login() {
    let (_dir, store) = followed_ticket();
    store.write(|tx| append(tx, "ticket.updated", 1, mention_of(BEA))).unwrap();
    store
        .write(|tx| {
            tx.execute(
                "INSERT INTO claude_logins(user_id, status, linked_at_ms, checked_at_ms) VALUES (?1, 'linked', ?2, ?2)",
                rusqlite::params![BEA, crate::now_ms() + 1_000],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(rows_for(&store, BEA).len(), 1);
}

// ---- A0: a new board member ----

/// A shared notification from `event_type` at `at_ms`.
fn shared_at(store: &Store, id: &'static str, event_type: &'static str, severity: &'static str, at_ms: i64) {
    store
        .write(move |tx| {
            tx.execute(
                "INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms)
                 VALUES (?1, 'x', ?1, 1, ?2, 1, 'system', ?1, '{}', ?1, 10, 10)",
                [id, event_type],
            )?;
            tx.execute(
                "INSERT INTO notifications(id, event_id, dedupe_key, severity, target_json, created_at_ms) VALUES (?1, ?1, ?1, ?2, '{}', ?3)",
                rusqlite::params![id, severity, at_ms],
            )?;
            Ok(())
        })
        .unwrap();
}

fn joined(store: &Store, user: &'static str, at_ms: i64) {
    store.write(move |tx| Ok(tx.execute("UPDATE users SET created_at_ms = ?2 WHERE id = ?1", rusqlite::params![user, at_ms])?)).unwrap();
}

#[test]
fn a_shared_row_from_before_someone_joined_counts_as_read_for_them_only() {
    let (_dir, store) = inbox();
    joined(&store, BEA, 100);
    shared_at(&store, "before", "task.succeeded", "info", 50);
    shared_at(&store, "after", "task.succeeded", "info", 150);
    let read = |user| shown(&store, user).into_iter().filter(|(id, _)| id == "before" || id == "after").collect::<Vec<_>>();
    assert_eq!(read(BEA), vec![row("after", false), row("before", true)]);
    assert_eq!(read(ALEX), vec![row("after", false), row("before", false)], "alex (joined at 1) was there");
    let writes: i64 = store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM notification_reads", [], |r| r.get(0))?)).unwrap();
    assert_eq!(writes, 0);
}

#[test]
fn a_request_from_before_someone_joined_stays_unread_while_it_still_waits() {
    let (_dir, store) = inbox();
    joined(&store, BEA, 100);
    // No approval row says it was handled: it still waits.
    shared_at(&store, "open-approval", "approval.requested", "action", 50);
    let before = unread(&store, BEA);
    assert!(!shown(&store, BEA).into_iter().find(|(id, _)| id == "open-approval").unwrap().1);
    assert!(before >= 1);
}

#[test]
fn without_a_board_member_a_shared_row_is_still_listed_and_counted() {
    let (_dir, store) = inbox();
    shared_at(&store, "old", "task.succeeded", "info", 50);
    let listed = store.read(|c| timeline::list_notifications_for(c, 50, None)).unwrap();
    assert!(listed.iter().any(|n| n.id == "old" && !n.read));
    assert!(store.read(|c| timeline::unread_count_for(c, None)).unwrap() >= 1);
    assert!(store.read(|c| timeline::list_notifications(c, 50)).unwrap().iter().any(|n| n.id == "old"));
}

// ---- A0: growth ----

const NOW: i64 = 1_000 * timeline::NOTIFICATION_KEEP_MS;

fn left(store: &Store) -> Vec<String> {
    let mut ids: Vec<String> = store.read(|c| Ok(c.prepare("SELECT id FROM notifications")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)).unwrap();
    ids.sort();
    ids
}

#[test]
fn prune_removes_old_rows_with_who_read_them_keeps_requests_that_wait_and_all_events() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let old = NOW - timeline::NOTIFICATION_KEEP_MS - 1;
    shared_at(&store, "old-info", "task.succeeded", "info", old);
    shared_at(&store, "new-info", "task.succeeded", "info", NOW - timeline::NOTIFICATION_KEEP_MS + 1);
    shared_at(&store, "old-open-approval", "approval.requested", "action", old);
    // `task.waiting` of a task that no longer waits: handled, so it may go.
    shared_at(&store, "old-handled-request", "task.waiting", "action", old);
    store
        .write(|tx| {
            let company: String = tx.query_row("SELECT id FROM company", [], |r| r.get(0))?;
            tx.execute_batch(&format!(
                "INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{BEA}', '{company}', 'bea', '$argon2id$x', 1, 1);
                 INSERT INTO notification_reads(notification_id, user_id, read_at_ms) VALUES ('old-info', '{BEA}', 1);
                 INSERT INTO notification_dismissals(notification_id, user_id, dismissed_at_ms) VALUES ('old-info', '{BEA}', 1);"
            ))?;
            Ok(())
        })
        .unwrap();
    let events = |store: &Store| -> i64 { store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?)).unwrap() };
    let before = events(&store);

    let gone = store.write(|tx| timeline::prune_notifications(tx, NOW)).unwrap();
    assert_eq!(gone, 2);
    assert_eq!(left(&store), vec!["new-info".to_owned(), "old-open-approval".to_owned()]);
    assert_eq!(events(&store), before, "events stay");
    let related: i64 = store.read(|c| Ok(c.query_row("SELECT (SELECT COUNT(*) FROM notification_reads) + (SELECT COUNT(*) FROM notification_dismissals)", [], |r| r.get(0))?)).unwrap();
    assert_eq!(related, 0, "reads and dismissals cascade");
    assert_eq!(store.write(|tx| timeline::prune_notifications(tx, NOW)).unwrap(), 0, "nothing more to do");
}
