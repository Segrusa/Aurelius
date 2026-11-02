//! Following a ticket in storage (AUR-FA-S1): idempotent, the first reason stays,
//! unfollowing removes the row, and a disabled board member stays listed but is
//! no one to notify.

use crate::followers::{self, FollowReason};
use crate::tickets::{self, NewTicket};
use crate::{Store, company, events, users};
use aurelius_domain::task::RunSettings;
use aurelius_domain::ticket::{ColumnKind, TicketPriority};
use aurelius_domain::{ProjectId, TicketId, UserId};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Fixture {
    _dir: tempfile::TempDir,
    store: Store,
    ticket: TicketId,
    alex: UserId,
    bea: UserId,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (ticket, alex, bea, project) = (TicketId::new(), UserId::new(), UserId::new(), ProjectId::new());
    store
        .write(move |tx| {
            users::insert(tx, &users::NewUser { id: &alex.to_string(), username: "alex", created_by_user: None, now_ms: 1 }, PHC)?;
            users::insert(tx, &users::NewUser { id: &bea.to_string(), username: "bea", created_by_user: None, now_ms: 1 }, PHC)?;
            events::insert_project_projection(tx, project, "Atlas", None)?;
            let backlog = company::column_of_kind(tx, ColumnKind::Backlog)?;
            tickets::insert(
                tx,
                &NewTicket { id: ticket, project_id: project, title: "Fix", description_blob: None, column_id: backlog.id, priority: TicketPriority::default(), created_by_user: alex, owner: None, run: &RunSettings::default(), labels: &[], deadline_ms: None },
            )?;
            Ok(())
        })
        .unwrap();
    Fixture { _dir: dir, store, ticket, alex, bea }
}

fn follow(f: &Fixture, user: UserId, reason: FollowReason, at: i64) -> bool {
    let ticket = f.ticket;
    f.store.write(move |tx| followers::follow(tx, ticket, user, reason, at)).unwrap()
}

fn unfollow(f: &Fixture, user: UserId) -> bool {
    let ticket = f.ticket;
    f.store.write(move |tx| followers::unfollow(tx, ticket, user)).unwrap()
}

fn disable(f: &Fixture, user: UserId) {
    f.store.write(move |tx| Ok(tx.execute("UPDATE users SET disabled_at_ms = 5 WHERE id = ?1", [user.to_string()])?)).unwrap();
}

#[test]
fn nobody_follows_a_new_ticket_in_storage_the_application_decides() {
    let f = fixture();
    assert!(f.store.read(|c| followers::list(c, f.ticket)).unwrap().is_empty());
}

#[test]
fn following_twice_keeps_the_first_reason_and_time() {
    let f = fixture();
    assert!(follow(&f, f.bea, FollowReason::Mentioned, 10));
    assert!(!follow(&f, f.bea, FollowReason::Manual, 20));
    let list = f.store.read(|c| followers::list(c, f.ticket)).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].reason.as_str(), list[0].followed_at_ms), ("mentioned", 10));
}

#[test]
fn every_reason_passes_the_check_of_the_table() {
    let f = fixture();
    for reason in [FollowReason::Created, FollowReason::Responsible, FollowReason::Mentioned, FollowReason::Manual] {
        assert!(follow(&f, f.bea, reason, 10), "{reason:?}");
        assert_eq!(f.store.read(|c| followers::list(c, f.ticket)).unwrap()[0].reason, reason.as_str());
        assert!(unfollow(&f, f.bea));
    }
}

#[test]
fn unfollowing_removes_the_row_and_says_whether_there_was_one() {
    let f = fixture();
    follow(&f, f.bea, FollowReason::Manual, 10);
    assert!(f.store.read(|c| followers::is_following(c, f.ticket, f.bea)).unwrap());
    assert!(unfollow(&f, f.bea));
    assert!(!f.store.read(|c| followers::is_following(c, f.ticket, f.bea)).unwrap());
    assert!(!unfollow(&f, f.bea), "not any more");
    assert!(follow(&f, f.bea, FollowReason::Created, 30), "nothing remembers they left");
    assert_eq!(f.store.read(|c| followers::list(c, f.ticket)).unwrap()[0].reason, "created");
}

#[test]
fn the_list_gives_name_and_disabled_in_order_of_following() {
    let f = fixture();
    follow(&f, f.bea, FollowReason::Manual, 10);
    follow(&f, f.alex, FollowReason::Created, 20);
    disable(&f, f.bea);
    let list = f.store.read(|c| followers::list(c, f.ticket)).unwrap();
    assert_eq!(list.iter().map(|v| (v.user_id, v.username.as_str(), v.disabled)).collect::<Vec<_>>(), [(f.bea, "bea", true), (f.alex, "alex", false)]);
}

#[test]
fn a_disabled_follower_is_listed_but_no_one_to_notify() {
    let f = fixture();
    follow(&f, f.bea, FollowReason::Manual, 10);
    follow(&f, f.alex, FollowReason::Created, 20);
    assert_eq!(f.store.read(|c| followers::active_followers(c, f.ticket)).unwrap(), [f.bea, f.alex]);
    disable(&f, f.bea);
    assert_eq!(f.store.read(|c| followers::active_followers(c, f.ticket)).unwrap(), [f.alex]);
}

#[test]
fn following_a_ticket_or_as_someone_that_does_not_exist_is_refused() {
    let f = fixture();
    let (ticket, user) = (f.ticket, f.bea);
    assert!(f.store.write(move |tx| followers::follow(tx, TicketId::new(), user, FollowReason::Manual, 1)).is_err());
    assert!(f.store.write(move |tx| followers::follow(tx, ticket, UserId::new(), FollowReason::Manual, 1)).is_err());
}

#[test]
fn following_gives_the_tickets_of_that_board_member_only() {
    let f = fixture();
    follow(&f, f.bea, FollowReason::Manual, 10);
    assert_eq!(f.store.read(|c| followers::following(c, f.bea)).unwrap().into_iter().collect::<Vec<_>>(), [f.ticket]);
    assert!(f.store.read(|c| followers::following(c, f.alex)).unwrap().is_empty());
}
