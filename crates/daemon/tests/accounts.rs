//! Sign-in, sessions, passwords and disabled board members (protocol 5):
//! MU3-02 .. MU3-06 and MU3-08 over the protocol.

mod common;

use aurelius_client::{Client, ClientError, SessionPush};
use aurelius_domain::group_chat::SendGroupMessage;
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{CreateTicket, TicketPriority};
use aurelius_domain::user::SignOutReason;
use aurelius_domain::{Command, CommandResult, ErrorCode, ProjectId, UserId};
use common::{BOARD_MEMBER, BOARD_PASSWORD};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

const NEW_PASSWORD: &str = "a-NEW-password-only-alex-knows";
const TEMPORARY: &str = "a-TEMPORARY-password-from-robin";

fn code<T: std::fmt::Debug>(result: Result<T, ClientError>) -> ErrorCode {
    match result {
        Err(ClientError::Protocol(err)) => err.code,
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

/// A connection signed in as `name` on a device called `device`, with its token.
async fn device(root: &Path, name: &str, password: &str, device: &str) -> (Client, String) {
    let client = common::connect_anonymous(root).await;
    let login = client.user_login(name, password, device).await.expect("sign in");
    (client, login.session_token)
}

/// The sign-out this connection is told, then the connection is gone.
async fn signed_out(client: &Client, pushes: &mut UnboundedReceiver<SessionPush>) -> SignOutReason {
    let reason = loop {
        match tokio::time::timeout(Duration::from_secs(5), pushes.recv()).await.expect("signed out at once").expect("pushes") {
            SessionPush::SignedOut(reason) => break reason,
            SessionPush::ClaudeLoginPrompt(_) => {}
        }
    };
    // The connection is closed: the next call fails at once.
    let next = tokio::time::timeout(Duration::from_secs(5), client.user_whoami()).await.expect("an answer in time");
    assert!(matches!(&next, Err(err) if err.is_disconnect()), "the connection was closed: {next:?}");
    reason
}

async fn project(client: &Client) -> ProjectId {
    let created = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = created.result else { panic!("{:?}", created.result) };
    project_id
}

fn ticket(project_id: ProjectId, title: &str) -> Command {
    Command::CreateTicket(CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id,
        title: title.into(),
        description: None,
        priority: TicketPriority::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: None,
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: None,
        from_idea: None,
    })
}

fn sha256(text: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn timeline_titles(root: &Path) -> Vec<String> {
    let db = common::db(root);
    aurelius_storage::timeline::cards(&db, None, None, None, 50, false).unwrap().0.into_iter().map(|c| c.title).collect()
}

/// MU3-02 AC2/AC3, MU3-03 AC1: a session for this device, only its hash kept;
/// one answer for a wrong password and an unknown name.
#[tokio::test(flavor = "multi_thread")]
async fn a_sign_in_gives_this_device_a_session_and_one_answer_for_every_mistake() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let alex = common::connect(dir.path()).await;
    let alex_id = alex.user_whoami().await.unwrap().user.unwrap().id;

    let anonymous = common::connect_anonymous(dir.path()).await;
    let wrong = anonymous.user_login(BOARD_MEMBER, "not the password at all", "Laptop").await.unwrap_err();
    let unknown = anonymous.user_login("nobody-here", BOARD_PASSWORD, "Laptop").await.unwrap_err();
    let (ClientError::Protocol(wrong), ClientError::Protocol(unknown)) = (wrong, unknown) else { panic!() };
    assert_eq!((wrong.code, &wrong.message), (ErrorCode::WrongPassword, &"Username or password is wrong.".to_string()));
    assert_eq!((unknown.code, &unknown.message), (wrong.code, &wrong.message), "nothing tells which one was wrong");

    let login = anonymous.user_login(BOARD_MEMBER, BOARD_PASSWORD, "Laptop").await.unwrap();
    assert_eq!(login.user.id, alex_id);
    assert!(!login.must_change_password);
    let now = aurelius_storage::now_ms();
    let ninety_days = 90 * 24 * 3_600_000;
    assert!((now + ninety_days - 60_000..=now + ninety_days).contains(&login.expires_at_ms), "90 days at most (B7)");
    // In the app: a write works, by this board member.
    let project = project(&anonymous).await;
    let made = anonymous.ticket_command(ticket(project, "Signed in"), None).await.unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!() };
    assert_eq!(anonymous.ticket(ticket_id).await.unwrap().ticket.created_by_user, alex_id);

    // The database keeps the SHA-256 of the token, the device and no client (local IPC, B8).
    let db = common::db(dir.path());
    let (client_id, device, verifier): (Option<String>, String, String) = db
        .query_row("SELECT client_id, device_name, token_verifier FROM user_sessions WHERE token_verifier = ?1", [sha256(&login.session_token)], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("the session, by the hash of its token");
    assert_eq!((client_id, device.as_str()), (None, "Laptop"));
    assert_ne!(verifier, login.session_token);
    let receipts: String = db.query_row("SELECT coalesce(group_concat(request_hash || result_json, ' '), '') FROM command_receipts", [], |r| r.get(0)).unwrap();
    let events: String = db.query_row("SELECT group_concat(metadata_json, ' ') FROM events", [], |r| r.get(0)).unwrap();
    let sessions: String = db.query_row("SELECT group_concat(id || device_name || token_verifier, ' ') FROM user_sessions", [], |r| r.get(0)).unwrap();
    for (place, text) in [("receipts", &receipts), ("events", &events), ("sessions", &sessions)] {
        assert!(!text.contains(&login.session_token), "the token is in {place}");
    }
    drop((alex, anonymous));
    common::stop(dir.path(), daemon).await;
}

/// MU3-03 AC2/AC3: the session survives a reconnect and a restart of the
/// server, not its end (30 days unused, 90 in all); a token of another device does not count.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_lasts_over_restarts_until_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    drop(common::connect(dir.path()).await);
    let (first, token) = device(dir.path(), BOARD_MEMBER, BOARD_PASSWORD, "Laptop").await;
    drop(first);
    let again = common::connect_anonymous(dir.path()).await;
    assert_eq!(again.resume(&token).await.unwrap().user.map(|u| u.username), Some(BOARD_MEMBER.to_string()));
    assert!(again.projects().await.is_ok());
    drop(again);
    common::stop(dir.path(), daemon).await;

    let daemon = common::start(dir.path()).await;
    let back = common::connect_anonymous(dir.path()).await;
    let hello = back.resume(&token).await.unwrap();
    assert_eq!(hello.user.map(|u| u.username), Some(BOARD_MEMBER.to_string()), "still signed in after a restart");
    assert_eq!(hello.claude_login.map(|l| l.status), Some(aurelius_domain::user::ClaudeLoginStatus::Missing));
    assert!(back.resume("not-a-token-of-this-server").await.unwrap().user.is_none(), "an unknown token: sign in again");
    assert_eq!(code(back.projects().await), ErrorCode::Unauthenticated);

    // 30 days without use, or 90 days in all: the sign-in screen, nothing else lost.
    let (_idle, idle_token) = device(dir.path(), BOARD_MEMBER, BOARD_PASSWORD, "Old phone").await;
    let (_aged, aged_token) = device(dir.path(), BOARD_MEMBER, BOARD_PASSWORD, "Old laptop").await;
    let rw = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    let now = aurelius_storage::now_ms();
    rw.execute("UPDATE user_sessions SET last_seen_ms = ?1 WHERE token_verifier = ?2", rusqlite::params![now - 31 * 24 * 3_600_000, sha256(&idle_token)]).unwrap();
    rw.execute("UPDATE user_sessions SET expires_at_ms = ?1 WHERE token_verifier = ?2", rusqlite::params![now - 1, sha256(&aged_token)]).unwrap();
    drop(rw);
    for token in [idle_token, aged_token] {
        let late = common::connect_anonymous(dir.path()).await;
        assert!(late.resume(&token).await.unwrap().user.is_none());
    }
    assert!(common::connect_anonymous(dir.path()).await.resume(&token).await.unwrap().user.is_some(), "the others stay");
    drop(back);
    common::stop(dir.path(), daemon).await;
}

/// MU3-04 AC1-AC4, MU3-06 AC3: the sessions of a board member, revoked from
/// another device or by another board member (named in the timeline), and the
/// device's connection is closed at once; signing out ends the session.
#[tokio::test(flavor = "multi_thread")]
async fn sessions_are_listed_and_revoking_one_closes_its_device_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let alex = common::connect(dir.path()).await;
    let robin = common::connect_as(dir.path(), "robin").await;
    let (phone, _) = device(dir.path(), BOARD_MEMBER, BOARD_PASSWORD, "Phone").await;
    let (laptop, laptop_token) = device(dir.path(), BOARD_MEMBER, BOARD_PASSWORD, "Laptop").await;

    // AC1: per session the device, when it began and was last active; this one marked.
    let mine = alex.sessions(None).await.unwrap();
    assert_eq!(mine.user_id, alex.user_whoami().await.unwrap().user.unwrap().id);
    let phone_session = mine.sessions.iter().find(|s| s.device_name == "Phone").expect("the phone").clone();
    assert!(!phone_session.current && phone_session.created_at_ms > 0 && phone_session.last_seen_ms >= phone_session.created_at_ms && phone_session.client_id.is_none());
    assert_eq!(mine.sessions.iter().filter(|s| s.current).count(), 1);
    assert_eq!(code(alex.revoke_session(mine.sessions.iter().find(|s| s.current).unwrap().id).await), ErrorCode::Validation, "this device signs out instead");

    // AC2: revoked from another device: the phone is signed out at once and must sign in again.
    let mut pushes = phone.session_pushes();
    alex.revoke_session(phone_session.id).await.unwrap();
    assert_eq!(signed_out(&phone, &mut pushes).await, SignOutReason::Revoked);
    assert!(!alex.sessions(None).await.unwrap().sessions.iter().any(|s| s.id == phone_session.id));

    // AC4: another board member revokes one of Alex's; the timeline names who.
    let laptop_session = alex.sessions(None).await.unwrap().sessions.into_iter().find(|s| s.device_name == "Laptop").unwrap();
    let mut pushes = laptop.session_pushes();
    let robin_sees = robin.sessions(Some(laptop_session.user_id)).await.unwrap();
    assert!(robin_sees.sessions.iter().all(|s| !s.current), "none of them is Robin's");
    robin.revoke_session(laptop_session.id).await.unwrap();
    assert_eq!(signed_out(&laptop, &mut pushes).await, SignOutReason::Revoked);
    assert!(common::connect_anonymous(dir.path()).await.resume(&laptop_token).await.unwrap().user.is_none());
    let titles = timeline_titles(dir.path());
    assert!(titles.contains(&"robin signed out alex's session on Laptop".to_string()), "{titles:?}");
    assert!(titles.contains(&"alex signed out their own session on Phone".to_string()), "{titles:?}");

    // AC3: signing out ends the session of this device; its other connections close.
    let (desk, desk_token) = device(dir.path(), BOARD_MEMBER, BOARD_PASSWORD, "Desk").await;
    let twin = common::connect_anonymous(dir.path()).await;
    assert!(twin.resume(&desk_token).await.unwrap().user.is_some());
    let mut twin_pushes = twin.session_pushes();
    assert!(desk.user_logout().await.unwrap().user.is_none());
    assert_eq!(code(desk.projects().await), ErrorCode::Unauthenticated, "the sign-in screen");
    assert_eq!(signed_out(&twin, &mut twin_pushes).await, SignOutReason::LoggedOut);
    assert!(common::connect_anonymous(dir.path()).await.resume(&desk_token).await.unwrap().user.is_none());
    drop((alex, robin, desk));
    common::stop(dir.path(), daemon).await;
}

/// MU3-05 AC1/AC2, B10: the own password (with the old one) keeps this session
/// and ends the others; another's is a temporary one that must be changed first.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_password_ends_the_other_sessions_and_a_temporary_one_must_be_changed() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let robin = common::connect_as(dir.path(), "robin").await;
    let alex = common::connect(dir.path()).await;
    let alex_id = alex.user_whoami().await.unwrap().user.unwrap().id;
    let (other, _) = device(dir.path(), BOARD_MEMBER, BOARD_PASSWORD, "Phone").await;

    assert_eq!(code(alex.set_password(None, Some("not my password at all"), NEW_PASSWORD).await), ErrorCode::WrongPassword);
    assert_eq!(code(alex.set_password(None, Some(BOARD_PASSWORD), "short").await), ErrorCode::WeakPassword);
    let mut pushes = other.session_pushes();
    alex.set_password(None, Some(BOARD_PASSWORD), NEW_PASSWORD).await.unwrap();
    assert_eq!(signed_out(&other, &mut pushes).await, SignOutReason::PasswordChanged);
    assert!(alex.projects().await.is_ok(), "this session stays");
    let anonymous = common::connect_anonymous(dir.path()).await;
    assert_eq!(code(anonymous.user_login(BOARD_MEMBER, BOARD_PASSWORD, "x").await), ErrorCode::WrongPassword);
    anonymous.user_login(BOARD_MEMBER, NEW_PASSWORD, "x").await.unwrap();

    // Robin resets Alex's password: every session of Alex ends, this one too.
    let mut pushes = alex.session_pushes();
    let mut anonymous_pushes = anonymous.session_pushes();
    robin.set_password(Some(alex_id), None, TEMPORARY).await.unwrap();
    assert_eq!(signed_out(&alex, &mut pushes).await, SignOutReason::PasswordChanged);
    assert_eq!(signed_out(&anonymous, &mut anonymous_pushes).await, SignOutReason::PasswordChanged);
    let back = common::connect_anonymous(dir.path()).await;
    let login = back.user_login(BOARD_MEMBER, TEMPORARY, "Laptop").await.unwrap();
    assert!(login.must_change_password);
    assert!(back.user_whoami().await.unwrap().must_change_password);
    assert_eq!(code(back.projects().await), ErrorCode::PasswordChangeRequired);
    // SEC1 M4: not a new board member either, nor the list of them.
    assert_eq!(code(back.user_create("casey", NEW_PASSWORD).await), ErrorCode::PasswordChangeRequired);
    assert_eq!(code(back.user_list().await), ErrorCode::PasswordChangeRequired);
    assert_eq!(code(back.set_password(Some(robin.user_whoami().await.unwrap().user.unwrap().id), None, NEW_PASSWORD).await), ErrorCode::PasswordChangeRequired);
    back.set_password(None, Some(TEMPORARY), NEW_PASSWORD).await.unwrap();
    assert!(back.projects().await.is_ok() && !back.user_whoami().await.unwrap().must_change_password);
    assert!(!common::connect_anonymous(dir.path()).await.user_login(BOARD_MEMBER, NEW_PASSWORD, "y").await.unwrap().must_change_password);

    let titles = timeline_titles(dir.path());
    assert!(titles.contains(&"robin set a temporary password for alex".to_string()) && titles.contains(&"alex changed their password".to_string()), "{titles:?}");
    // No password anywhere in the database: receipts (and their request hashes), events, sessions.
    let db = common::db(dir.path());
    let everything: String = db
        .query_row(
            "SELECT (SELECT coalesce(group_concat(request_hash || result_json, ' '), '') FROM command_receipts) || (SELECT group_concat(metadata_json, ' ') FROM events)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    for secret in [NEW_PASSWORD, TEMPORARY, BOARD_PASSWORD] {
        assert!(!everything.contains(secret), "a password is in the receipts or events");
    }
    drop((robin, back));
    common::stop(dir.path(), daemon).await;
}

/// Only an administrator resets another board member's password, ends their
/// sessions, sees their devices or disables them; the first board member is one,
/// names others, and the last one stays one.
#[tokio::test(flavor = "multi_thread")]
async fn only_an_administrator_manages_another_board_members_account() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let alex = common::connect(dir.path()).await;
    let robin = common::connect_as(dir.path(), "robin").await;
    let alex_id = alex.user_whoami().await.unwrap().user.unwrap().id;
    let robin_id = robin.user_whoami().await.unwrap().user.unwrap().id;
    let admins = |users: Vec<aurelius_domain::user::UserView>| users.into_iter().filter(|u| u.admin).map(|u| u.id).collect::<Vec<_>>();
    assert_eq!(admins(alex.user_list().await.unwrap().users), vec![alex_id], "the first board member");

    // Robin is an ordinary board member: Alex's account is not theirs to touch.
    assert_eq!(code(robin.set_password(Some(alex_id), None, TEMPORARY).await), ErrorCode::Forbidden);
    assert_eq!(code(robin.sessions(Some(alex_id)).await), ErrorCode::Forbidden);
    let alex_session = alex.sessions(None).await.unwrap().sessions[0].id;
    assert_eq!(code(robin.revoke_session(alex_session).await), ErrorCode::Forbidden);
    assert_eq!(code(robin.disable_user(alex_id).await), ErrorCode::Forbidden);
    assert_eq!(code(robin.set_admin(robin_id, true).await), ErrorCode::Forbidden);
    assert!(alex.user_whoami().await.unwrap().user.is_some(), "Alex is still signed in");
    // Their own they still manage.
    assert!(robin.sessions(None).await.is_ok());

    // The last administrator stays one, and cannot be disabled by a second one made meanwhile.
    assert_eq!(code(alex.set_admin(alex_id, false).await), ErrorCode::Validation);
    assert!(alex.set_admin(robin_id, true).await.unwrap().admin);
    assert!(robin.sessions(Some(alex_id)).await.is_ok(), "an administrator now");
    assert!(!robin.set_admin(alex_id, false).await.unwrap().admin);
    assert_eq!(admins(robin.user_list().await.unwrap().users), vec![robin_id]);
    assert_eq!(code(alex.disable_user(robin_id).await), ErrorCode::Forbidden);
    assert_eq!(code(robin.set_admin(robin_id, false).await), ErrorCode::Validation);
    assert!(timeline_titles(dir.path()).contains(&"alex made robin an administrator".to_string()));
    drop((alex, robin));
    common::stop(dir.path(), daemon).await;
}

/// MU3-05 AC3/AC4: a disabled board member is signed out at once and cannot
/// sign in; their tickets keep their creator; the last active one stays.
#[tokio::test(flavor = "multi_thread")]
async fn a_disabled_board_member_is_signed_out_and_the_last_one_stays() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let alex = common::connect(dir.path()).await;
    let robin = common::connect_as(dir.path(), "robin").await;
    let robin_id = robin.user_whoami().await.unwrap().user.unwrap().id;
    let project = project(&robin).await;
    let made = robin.ticket_command(ticket(project, "Robin's"), None).await.unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!() };

    let mut pushes = robin.session_pushes();
    let disabled = alex.disable_user(robin_id).await.unwrap();
    assert!(disabled.disabled_at_ms.is_some());
    assert_eq!(signed_out(&robin, &mut pushes).await, SignOutReason::Disabled);
    let anonymous = common::connect_anonymous(dir.path()).await;
    assert_eq!(code(anonymous.user_login("robin", BOARD_PASSWORD, "x").await), ErrorCode::WrongPassword, "the same answer as a wrong password");
    assert!(alex.user_list().await.unwrap().users.iter().any(|u| u.id == robin_id && u.disabled_at_ms.is_some()), "never deleted");
    assert_eq!(alex.ticket(ticket_id).await.unwrap().ticket.created_by_user, robin_id, "tickets keep their creator");
    assert_eq!(code(alex.disable_user(robin_id).await), ErrorCode::Validation);

    // AC4: Alex is the last active board member.
    let alex_id = alex.user_whoami().await.unwrap().user.unwrap().id;
    assert_eq!(code(alex.disable_user(alex_id).await), ErrorCode::LastBoardMember);
    let db = common::db(dir.path());
    let meta: String = db.query_row("SELECT metadata_json FROM events WHERE type = 'user.disabled'", [], |r| r.get(0)).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta).unwrap();
    assert_eq!((meta["disabled_user_id"].clone(), meta["by"].clone()), (serde_json::json!(robin_id), serde_json::json!(alex_id)));
    assert!(timeline_titles(dir.path()).contains(&"alex disabled robin".to_string()));
    drop((alex, anonymous));
    common::stop(dir.path(), daemon).await;
}

/// MU3-08 AC1/AC2: the author of a message and of a write is the session's
/// board member, never what a client says.
#[tokio::test(flavor = "multi_thread")]
async fn the_author_of_every_message_and_write_is_the_sessions_board_member() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let alex = common::connect(dir.path()).await;
    let robin = common::connect_as(dir.path(), "robin").await;
    let alex_id: UserId = alex.user_whoami().await.unwrap().user.unwrap().id;
    let robin_id: UserId = robin.user_whoami().await.unwrap().user.unwrap().id;
    for (client, text) in [(&alex, "From Alex"), (&robin, "From Robin")] {
        let send = SendGroupMessage { confirm_secret: false, text: text.into(), images: vec![], files: vec![], channel: None, thread_parent: None };
        client.execute(aurelius_client::envelope(Command::SendGroupMessage(send), None)).await.unwrap();
    }
    let messages = alex.group_chat().await.unwrap().messages;
    let author = |text: &str| messages.iter().find(|m| m.text == text).and_then(|m| m.author_user.as_ref().map(|u| u.id));
    assert_eq!((author("From Alex"), author("From Robin")), (Some(alex_id), Some(robin_id)));

    let project = project(&robin).await;
    let made = robin.ticket_command(ticket(project, "By Robin"), None).await.unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!() };
    let db = common::db(dir.path());
    let meta: String = db.query_row("SELECT metadata_json FROM events WHERE type = 'ticket.created' AND aggregate_id = ?1", [ticket_id.to_string()], |r| r.get(0)).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta).unwrap();
    assert_eq!(meta["user_id"], serde_json::json!(robin_id));
    drop((alex, robin));
    common::stop(dir.path(), daemon).await;
}

/// How many `project.created` events reach `events` before none came for `wait`.
async fn project_events(events: &mut UnboundedReceiver<aurelius_protocol::Event>, wait: Duration) -> usize {
    let mut seen = 0;
    while let Ok(Some(event)) = tokio::time::timeout(wait, events.recv()).await {
        if event.kind.as_str() == "project.created" {
            seen += 1;
        }
    }
    seen
}

/// R1 M-1 / SEC1 M5 (MU3-06 AC2): once a connection loses its session (signed
/// out, or a hello with a token that no longer works) it gets no more events,
/// though it subscribed before.
#[tokio::test(flavor = "multi_thread")]
async fn a_connection_that_lost_its_session_gets_no_more_events() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    drop(common::connect(dir.path()).await);
    let robin = common::connect_as(dir.path(), "robin").await;
    for lose in ["logout", "a bad hello"] {
        let (tx, mut events) = tokio::sync::mpsc::unbounded_channel();
        let device = Client::connect(&aurelius_host::endpoint(dir.path()), "test", tx).await.unwrap();
        device.user_login(BOARD_MEMBER, BOARD_PASSWORD, "Laptop").await.unwrap();
        device.subscribe(0).await.unwrap();
        project(&robin).await;
        assert!(project_events(&mut events, Duration::from_millis(500)).await >= 1, "{lose}: subscribed, it gets them");
        match lose {
            "logout" => assert!(device.user_logout().await.unwrap().user.is_none()),
            _ => assert!(device.resume("aus_not-a-session-of-this-server").await.unwrap().user.is_none()),
        }
        // What was on its way before the answer is drained; nothing comes after it.
        while events.try_recv().is_ok() {}
        project(&robin).await;
        assert_eq!(project_events(&mut events, Duration::from_millis(800)).await, 0, "{lose}: an event after the session ended");
        assert_eq!(code(device.subscribe(0).await), ErrorCode::Unauthenticated);
    }
    drop(robin);
    common::stop(dir.path(), daemon).await;
}

/// B9 R7 (SEC1 m10 / R1 m-2): twenty new board members an hour on the server,
/// over any connections; refused ones without a session do not count (SEC1 m1, QA1 m1).
#[tokio::test(flavor = "multi_thread")]
async fn the_twenty_first_board_member_in_an_hour_waits_over_connections() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    // The onboarding of `BOARD_MEMBER` is the first.
    drop(common::connect(dir.path()).await);
    let anonymous = common::connect_anonymous(dir.path()).await;
    for i in 0..5 {
        assert_eq!(code(anonymous.user_create(&format!("anon{i}"), BOARD_PASSWORD).await), ErrorCode::Unauthenticated);
    }
    let mut makers = Vec::new();
    for _ in 0..4 {
        makers.push(common::connect(dir.path()).await);
    }
    for i in 0..19 {
        makers[i % 4].user_create(&format!("member{i}"), BOARD_PASSWORD).await.unwrap_or_else(|e| panic!("member{i}: {e:?}"));
    }
    let late = common::connect(dir.path()).await;
    match late.user_create("member19", BOARD_PASSWORD).await {
        Err(ClientError::Protocol(err)) => {
            assert_eq!(err.code, ErrorCode::TooManyAttempts);
            assert!(err.retry_after_ms.is_some_and(|ms| ms > 50 * 60_000), "{err:?}");
        }
        other => panic!("{other:?}"),
    }
    drop((anonymous, makers, late));
    common::stop(dir.path(), daemon).await;
}

/// B9 R7 (SEC1 m10 / R1 m-2): five failed sign-ins on one name over two
/// connections make the sixth wait, on either connection, even when it is right.
#[tokio::test(flavor = "multi_thread")]
async fn the_sixth_sign_in_over_two_connections_waits() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    drop(common::connect(dir.path()).await);
    let (one, two) = (common::connect_anonymous(dir.path()).await, common::connect_anonymous(dir.path()).await);
    for client in [&one, &two, &one, &two, &one] {
        assert_eq!(code(client.user_login(BOARD_MEMBER, "not the password at all", "x").await), ErrorCode::WrongPassword);
    }
    assert_eq!(code(two.user_login(BOARD_MEMBER, BOARD_PASSWORD, "x").await), ErrorCode::TooManyAttempts);
    drop((one, two));
    common::stop(dir.path(), daemon).await;
}
