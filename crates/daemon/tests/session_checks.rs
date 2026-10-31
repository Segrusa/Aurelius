//! An open connection checks its session against the database now and then
//! (R1 M-2): an end that did not come through the daemon's hub (another
//! process, a race with it) still signs the connection out. The check runs
//! every `AURELIUS_SESSION_CHECK_MS` (a minute; short here), and a session is
//! marked used at most once an hour (S6, SEC1 m5).

mod common;

use aurelius_client::SessionPush;
use aurelius_domain::user::SignOutReason;
use common::{BOARD_MEMBER, BOARD_PASSWORD};
use std::path::Path;
use std::time::Duration;

const CHECK_MS: u64 = 200;

fn start_env() {
    // Every test of this binary wants the same short check: set before any daemon starts.
    unsafe { std::env::set_var("AURELIUS_SESSION_CHECK_MS", CHECK_MS.to_string()) };
}

fn sha256(text: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn rw(root: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(root.join("data/aurelius.sqlite")).unwrap()
}

fn last_seen(root: &Path, token: &str) -> i64 {
    common::db(root).query_row("SELECT last_seen_ms FROM user_sessions WHERE token_verifier = ?1", [sha256(token)], |r| r.get(0)).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_revoked_in_the_database_alone_signs_its_connection_out() {
    start_env();
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    drop(common::connect(dir.path()).await);
    let device = common::connect_anonymous(dir.path()).await;
    let token = device.user_login(BOARD_MEMBER, BOARD_PASSWORD, "Laptop").await.unwrap().session_token;
    let mut pushes = device.session_pushes();
    assert!(device.projects().await.is_ok());

    // Revoked behind the daemon's back: the hub knows nothing of it.
    rw(dir.path()).execute("UPDATE user_sessions SET revoked_at_ms = ?1 WHERE token_verifier = ?2", rusqlite::params![aurelius_storage::now_ms(), sha256(&token)]).unwrap();
    tokio::time::sleep(Duration::from_millis(CHECK_MS * 2)).await;
    let next = device.projects().await;
    assert!(next.is_err(), "not answered as signed in: {next:?}");
    let reason = loop {
        match tokio::time::timeout(Duration::from_secs(5), pushes.recv()).await.expect("signed out").expect("pushes") {
            SessionPush::SignedOut(reason) => break reason,
            SessionPush::ClaudeLoginPrompt(_) => {}
        }
    };
    assert_eq!(reason, SignOutReason::Revoked);
    let after = tokio::time::timeout(Duration::from_secs(5), device.user_whoami()).await.expect("an answer in time");
    assert!(matches!(&after, Err(err) if err.is_disconnect()), "the connection was closed: {after:?}");
    drop(device);
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_is_marked_used_at_most_once_an_hour() {
    start_env();
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    drop(common::connect(dir.path()).await);
    let device = common::connect_anonymous(dir.path()).await;
    let token = device.user_login(BOARD_MEMBER, BOARD_PASSWORD, "Laptop").await.unwrap().session_token;
    let now = aurelius_storage::now_ms();

    // Used ten minutes ago: a hello and requests past the check leave it.
    let recent = now - 10 * 60_000;
    rw(dir.path()).execute("UPDATE user_sessions SET last_seen_ms = ?1 WHERE token_verifier = ?2", rusqlite::params![recent, sha256(&token)]).unwrap();
    let again = common::connect_anonymous(dir.path()).await;
    assert!(again.resume(&token).await.unwrap().user.is_some());
    tokio::time::sleep(Duration::from_millis(CHECK_MS * 2)).await;
    assert!(again.projects().await.is_ok());
    assert_eq!(last_seen(dir.path(), &token), recent, "not marked used within the hour");

    // Two hours ago: the next hello marks it used.
    let old = now - 2 * 3_600_000;
    rw(dir.path()).execute("UPDATE user_sessions SET last_seen_ms = ?1 WHERE token_verifier = ?2", rusqlite::params![old, sha256(&token)]).unwrap();
    let third = common::connect_anonymous(dir.path()).await;
    assert!(third.resume(&token).await.unwrap().user.is_some());
    assert!(last_seen(dir.path(), &token) >= now, "marked used again");
    drop((device, again, third));
    common::stop(dir.path(), daemon).await;
}
