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
