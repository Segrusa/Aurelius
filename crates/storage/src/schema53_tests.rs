//! Migration 53: a branch's review in the Code tab. A schema 51 store (52 is on another
//! branch) gets the tables; a comment stays its writer's until their review is finished;
//! finished reviews are append-only; viewed marks are per board member and head.

use crate::{Store, code_review};
use aurelius_domain::code_review::{CodeLineSide, CodeVerdict, ValidReview, ValidThread, ValidViewed};
use aurelius_domain::{ProjectId, UserId};
use rusqlite::Connection;

const ALPHA: &str = "00000000-0000-7000-8000-000000000531";
const ALEX: &str = "00000000-0000-7000-8000-000000000532";
const BEA: &str = "00000000-0000-7000-8000-000000000533";
const HEAD: &str = "abcdef1234567";

/// A schema 51 store with the project Alpha and the board members Alex and Bea.
fn schema_51_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 51).unwrap());
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    assert!(conn.prepare("SELECT id FROM code_review_threads").is_err(), "new in 53");
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('pa', 'project', '{global}', '{ALPHA}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{ALPHA}', 'pa', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES
           ('{ALEX}', '{company}', 'alex', '$argon2id$x', 1, 1), ('{BEA}', '{company}', 'bea', '$argon2id$x', 1, 1);"
    ))
    .unwrap();
}

fn ids() -> (ProjectId, UserId, UserId) {
    (ProjectId::parse(ALPHA).unwrap(), UserId::parse(ALEX).unwrap(), UserId::parse(BEA).unwrap())
}

fn thread(project: ProjectId, body: &str) -> ValidThread {
    ValidThread {
        project_id: project,
        branch: "feat/ticket-84".into(),
        path: "src/a.rs".into(),
        line: 12,
        side: CodeLineSide::New,
        commit: HEAD.into(),
        snippet: vec!["+let a = 1;".into()],
        body: body.into(),
    }
}

fn review(project: ProjectId, verdict: CodeVerdict) -> ValidReview {
    ValidReview { project_id: project, branch: "feat/ticket-84".into(), head: HEAD.into(), verdict, body: "Looks right.".into() }
}

#[test]
fn a_comment_is_only_its_writers_until_their_review_is_finished() {
    let dir = tempfile::tempdir().unwrap();
    schema_51_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let (alpha, alex, bea) = ids();
    let (thread_id, _) = store.write(move |tx| code_review::start_thread(tx, alex, &thread(alpha, "Rename this."), 10)).unwrap();

    let mine = store.read(move |c| code_review::view(c, alex, alpha, "feat/ticket-84", None)).unwrap();
    assert_eq!(mine.threads.len(), 1);
    assert_eq!(mine.threads[0].snippet, vec!["+let a = 1;"]);
    assert!(mine.threads[0].comments[0].pending && mine.threads[0].comments[0].mine);
    let theirs = store.read(move |c| code_review::view(c, bea, alpha, "feat/ticket-84", None)).unwrap();
    assert!(theirs.threads.is_empty(), "a thread with only someone else's pending comment is not shown");

    assert_eq!(store.read(move |c| code_review::pending_count(c, alex, alpha, "feat/ticket-84")).unwrap(), 1);
    let (_, sent) = store.write(move |tx| code_review::submit(tx, alex, &review(alpha, CodeVerdict::Changes), 20)).unwrap();
    assert_eq!(sent, 1);
    let theirs = store.read(move |c| code_review::view(c, bea, alpha, "feat/ticket-84", None)).unwrap();
    assert_eq!(theirs.threads[0].id, thread_id);
    assert!(!theirs.threads[0].comments[0].pending && !theirs.threads[0].comments[0].mine);
    assert_eq!(theirs.reviews.len(), 1);
    assert_eq!((theirs.reviews[0].verdict, theirs.reviews[0].username.as_str()), (CodeVerdict::Changes, "alex"));

    // Bea's reply is pending for her; Alex sees only what was sent.
    store.write(move |tx| code_review::reply(tx, thread_id, bea, "Done in the next commit.", 30)).unwrap();
    let alexs = store.read(move |c| code_review::view(c, alex, alpha, "feat/ticket-84", None)).unwrap();
    assert_eq!(alexs.threads[0].comments.len(), 1);
}

#[test]
fn a_finished_review_cannot_be_changed_or_removed() {
    let dir = tempfile::tempdir().unwrap();
    schema_51_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let (alpha, alex, _) = ids();
    store.write(move |tx| code_review::submit(tx, alex, &review(alpha, CodeVerdict::Approved), 1)).unwrap();
    let conn = Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    assert!(conn.execute("UPDATE code_reviews SET verdict = 'changes'", []).is_err());
    assert!(conn.execute("DELETE FROM code_reviews", []).is_err());
    assert!(conn.execute("INSERT INTO code_reviews(id, project_id, branch, head, user_id, verdict, body, created_at_ms) VALUES ('x', ?1, 'b', 'abcdef1', ?2, 'merged', '', 1)", [ALPHA, ALEX]).is_err(), "an unknown verdict");
}

#[test]
fn a_resolved_thread_and_a_viewed_file_change_only_once() {
    let dir = tempfile::tempdir().unwrap();
    schema_51_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let (alpha, alex, bea) = ids();
    let (thread_id, _) = store.write(move |tx| code_review::start_thread(tx, alex, &thread(alpha, "Why?"), 1)).unwrap();
    assert!(store.write(move |tx| code_review::set_resolved(tx, thread_id, true)).unwrap());
    assert!(!store.write(move |tx| code_review::set_resolved(tx, thread_id, true)).unwrap());

    let mark = move |viewed: bool, head: &str| ValidViewed { project_id: alpha, branch: "feat/ticket-84".into(), head: head.into(), path: "src/a.rs".into(), viewed };
    let (on, again) = (mark(true, HEAD), mark(true, HEAD));
    assert!(store.write(move |tx| code_review::set_viewed(tx, alex, &on)).unwrap());
    assert!(!store.write(move |tx| code_review::set_viewed(tx, alex, &again)).unwrap());
    let at = |user: UserId, head: &'static str| store.read(move |c| code_review::view(c, user, alpha, "feat/ticket-84", Some(head))).unwrap().viewed;
    assert_eq!(at(alex, HEAD), vec!["src/a.rs"]);
    assert!(at(alex, "1111111").is_empty(), "a new head starts unviewed");
    assert!(at(bea, HEAD).is_empty(), "each board member their own");
    let off = mark(false, HEAD);
    assert!(store.write(move |tx| code_review::set_viewed(tx, alex, &off)).unwrap());
    assert!(at(alex, HEAD).is_empty());
}
