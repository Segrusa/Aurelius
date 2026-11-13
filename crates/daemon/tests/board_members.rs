//! Board members over the protocol: the onboarding, `user.*`, the hello fields,
//! the gate (protocol 5: nothing without a session), and that a password (or
//! its hash) never ends up in the database, a receipt, an event or the log.

mod common;

use aurelius_client::{Client, ClientError};
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{CreateTicket, TicketPriority};
use aurelius_domain::{Command, CommandResult, ErrorCode, ProjectId, UserId};
use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const PASSWORD: &str = "Tr0ub4dor&3-zebra-UNIQUE";
const CHANGED: &str = "a-CHANGED-password-UNIQUE";

fn code<T: std::fmt::Debug>(result: Result<T, ClientError>) -> ErrorCode {
    match result {
        Err(ClientError::Protocol(err)) => err.code,
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

/// Everything every test of this binary logs, at every level.
fn log() -> Arc<Mutex<Vec<u8>>> {
    static LOG: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    LOG.get_or_init(|| {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let sink = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || Sink(sink.clone()))
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect("one subscriber per test binary");
        buffer
    })
    .clone()
}

struct Sink(Arc<Mutex<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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
        kind: None,
    })
}

fn created(receipt: aurelius_domain::CommandReceipt) -> UserId {
    match receipt.result {
        CommandResult::UserCreated { user_id } => user_id,
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_board_member_is_made_in_the_onboarding_and_becomes_active() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let first = common::connect_anonymous(dir.path()).await;
    // MU1-20 AC1: a new server has no board members; the client shows the onboarding.
    let hello = first.user_whoami().await.unwrap();
    assert_eq!((hello.board_members_exist, hello.user), (false, None));
    // Protocol 5: nothing else before the first board member.
    let early = first.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await;
    assert_eq!(code(early), ErrorCode::Unauthenticated);

    // AC3: the reasons, nothing made.
    assert_eq!(code(first.user_create("grace", "short").await), ErrorCode::WeakPassword);
    assert_eq!(code(first.user_create("se", PASSWORD).await), ErrorCode::InvalidUsername);
    assert_eq!(code(first.user_create("ada lovelace", PASSWORD).await), ErrorCode::InvalidUsername);
    assert!(!first.user_whoami().await.unwrap().board_members_exist);

    // AC2: made, and this connection is now that board member.
    let grace = created(first.user_create("grace", PASSWORD).await.unwrap());
    let me = first.user_whoami().await.unwrap();
    assert!(me.board_members_exist);
    assert_eq!(me.user.as_ref().map(|u| (u.id, u.username.as_str(), u.created_by_user)), Some((grace, "grace", None)));

    // MU1-21 AC1: tickets are by this board member.
    let project = project(&first).await;
    let made = first.ticket_command(ticket(project, "Mine"), None).await.unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!("{:?}", made.result) };
    assert_eq!(first.ticket(ticket_id).await.unwrap().ticket.created_by_user, grace);

    // MU1-22: an active board member makes another and stays itself; the name is unique regardless of case.
    assert_eq!(code(first.user_create("GRACE", PASSWORD).await), ErrorCode::UsernameTaken);
    let ada = created(first.user_create("ada", "another-long-password").await.unwrap());
    assert_eq!(first.user_whoami().await.unwrap().user.unwrap().id, grace);
    let users = first.user_list().await.unwrap().users;
    assert_eq!(users.iter().map(|u| (u.username.as_str(), u.created_by_user)).collect::<Vec<_>>(), [("grace", None), ("ada", Some(grace))]);

    // AC5: a new client sees board members but has no active one yet.
    let second = common::connect_anonymous(dir.path()).await;
    let who = second.user_whoami().await.unwrap();
    assert_eq!((who.board_members_exist, who.user), (true, None));
    // Titan (SEC1 m9): without a session nobody is named; `board_members_exist` is all the sign-in needs.
    assert!(second.user_list().await.unwrap().users.is_empty());
    // The event says who made whom, never the password.
    let db = common::db(dir.path());
    let meta: String = db.query_row("SELECT metadata_json FROM events WHERE type = 'user.created' AND aggregate_id = ?1", [ada.to_string()], |r| r.get(0)).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta).unwrap();
    // As in every event, `user_id` is who did it; the new one is `created_user_id` (m2).
    assert_eq!(
        (meta["user_id"].clone(), meta["created_user_id"].clone(), meta["username"].clone(), meta["by"].clone()),
        (serde_json::json!(grace), serde_json::json!(ada), serde_json::json!("ada"), serde_json::json!(grace))
    );
    let first_meta: String = db.query_row("SELECT metadata_json FROM events WHERE type = 'user.created' AND aggregate_id = ?1", [grace.to_string()], |r| r.get(0)).unwrap();
    let first_meta: serde_json::Value = serde_json::from_str(&first_meta).unwrap();
    assert!(first_meta.get("user_id").is_none(), "the onboarding has no one who made it: {first_meta}");
    assert_eq!(first_meta["created_user_id"], serde_json::json!(grace));
    drop((first, second));
    common::stop(dir.path(), daemon).await;
}

/// R8 (R1 B-1, SEC1 M1): the onboarding is decided in the write transaction, so
/// of two first board members at the same time exactly one is made; the other
/// is `unauthenticated`. After it, names clash regardless of case (MU1-20 AC4).
#[tokio::test(flavor = "multi_thread")]
async fn of_two_first_board_members_at_once_exactly_one_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let (a, b) = (common::connect_anonymous(dir.path()).await, common::connect_anonymous(dir.path()).await);
    let (one, two) = tokio::join!(a.user_create("first", PASSWORD), b.user_create("second", PASSWORD));
    let made: Vec<&str> = [("first", &one), ("second", &two)].into_iter().filter(|(_, r)| r.is_ok()).map(|(n, _)| n).collect();
    assert_eq!(made.len(), 1, "exactly one first board member: {one:?} / {two:?}");
    for result in [one, two].into_iter().filter(|r| r.is_err()) {
        assert_eq!(code(result), ErrorCode::Unauthenticated);
    }
    let name = made[0];
    let winner = if name == "first" { &a } else { &b };
    assert_eq!(winner.user_list().await.unwrap().users.iter().map(|u| u.username.as_str()).collect::<Vec<_>>(), [name]);
    let loser = if name == "first" { &b } else { &a };
    assert!(loser.user_whoami().await.unwrap().user.is_none(), "the other connection has no board member");
    let db = common::db(dir.path());
    let users: i64 = db.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)).unwrap();
    let events: i64 = db.query_row("SELECT COUNT(*) FROM events WHERE type = 'user.created'", [], |r| r.get(0)).unwrap();
    assert_eq!((users, events), (1, 1));

    let (c, d) = (common::connect_anonymous(dir.path()).await, common::connect_anonymous(dir.path()).await);
    assert_eq!(code(c.user_create("third", PASSWORD).await), ErrorCode::Unauthenticated, "after the onboarding only with a session");
    c.user_login(name, PASSWORD, "test").await.unwrap();
    d.user_login(name, PASSWORD, "test").await.unwrap();
    let (three, four) = tokio::join!(c.user_create("same", PASSWORD), d.user_create("SAME", PASSWORD));
    let codes: Vec<Option<ErrorCode>> = [three, four].into_iter().map(|r| r.err().map(|e| match e { ClientError::Protocol(p) => p.code, other => panic!("{other:?}") })).collect();
    assert_eq!(codes.iter().filter(|c| c.is_none()).count(), 1, "{codes:?}");
    assert!(codes.contains(&Some(ErrorCode::UsernameTaken)));
    assert_eq!(c.user_list().await.unwrap().users.len(), 2);
    drop((a, b, c, d));
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn user_login_checks_the_password_with_one_answer_a_delay_and_a_limit() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let maker = common::connect_anonymous(dir.path()).await;
    let grace = created(maker.user_create("grace", PASSWORD).await.unwrap());

    let device = common::connect_anonymous(dir.path()).await;
    let started = Instant::now();
    assert_eq!(code(device.user_login("grace", "wrong password!!", "test").await), ErrorCode::WrongPassword);
    assert!(started.elapsed() >= Duration::from_millis(400), "a wrong answer takes a while");
    // An unknown board member gets the very same answer.
    assert_eq!(code(device.user_login("nobody", PASSWORD, "test").await), ErrorCode::WrongPassword);
    assert!(device.user_whoami().await.unwrap().user.is_none());
    let me = device.user_login("grace", PASSWORD, "test").await.unwrap();
    assert_eq!(me.user.id, grace);
    // Five tries a minute on one connection, right or wrong.
    assert_eq!(code(device.user_login("grace", "wrong password!!", "test").await), ErrorCode::WrongPassword);
    assert_eq!(code(device.user_login("grace", "wrong password!!", "test").await), ErrorCode::WrongPassword);
    assert_eq!(code(device.user_login("grace", PASSWORD, "test").await), ErrorCode::TooManyAttempts);
    assert_eq!(device.user_whoami().await.unwrap().user.map(|u| u.id), Some(grace), "a refused try changes nothing");
    // Another connection has its own tries.
    let other = common::connect_anonymous(dir.path()).await;
    assert!(other.user_login("grace", PASSWORD, "test").await.is_ok());
    drop((maker, device, other));
    common::stop(dir.path(), daemon).await;
}

/// B9 / MU3-02 AC4: after five failed tries for one username, from any
/// connections, the next waits ("Try again in 1:00"), even with the right
/// password; other usernames are not affected and nothing is blocked for good.
#[tokio::test(flavor = "multi_thread")]
async fn five_failed_sign_ins_for_a_username_make_the_next_one_wait() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let maker = common::connect_anonymous(dir.path()).await;
    created(maker.user_create("alex", PASSWORD).await.unwrap());
    created(maker.user_create("robin", PASSWORD).await.unwrap());
    for _ in 0..5 {
        // A fresh connection each time: the per-connection limit plays no part.
        let device = common::connect_anonymous(dir.path()).await;
        assert_eq!(code(device.user_login("Alex", "not the password", "test").await), ErrorCode::WrongPassword);
    }
    let device = common::connect_anonymous(dir.path()).await;
    match device.user_login("alex", PASSWORD, "test").await {
        Err(ClientError::Protocol(err)) => {
            assert_eq!(err.code, ErrorCode::TooManyAttempts);
            let wait = err.retry_after_ms.expect("when to try again");
            assert!((55_000..=60_000).contains(&wait), "a minute (SEC1 m2): {wait}");
            assert!(err.message.starts_with("Too many attempts. Try again in 0:") || err.message == "Too many attempts. Try again in 1:00.", "{}", err.message);
        }
        other => panic!("{other:?}"),
    }
    assert!(device.user_whoami().await.unwrap().user.is_none());
    // Robin signs in as usual; an unknown name counts the same way, so it tells nothing.
    assert_eq!(device.user_login("robin", PASSWORD, "test").await.unwrap().user.username, "robin");
    drop((maker, device));
    common::stop(dir.path(), daemon).await;
}

/// R1 M-3 / SEC1 M2: tries at the same time on one username are reserved
/// before their argon2 work, so twenty at once from twenty connections check
/// at most five passwords; the rest wait.
#[tokio::test(flavor = "multi_thread")]
async fn twenty_wrong_sign_ins_at_once_check_at_most_five_passwords() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let maker = common::connect_anonymous(dir.path()).await;
    created(maker.user_create("alex", PASSWORD).await.unwrap());
    let mut devices = Vec::new();
    for _ in 0..20 {
        devices.push(common::connect_anonymous(dir.path()).await);
    }
    let tries = devices.iter().map(|d| d.user_login("alex", "not the password", "test"));
    let codes: Vec<ErrorCode> = futures_util::future::join_all(tries).await.into_iter().map(code).collect();
    let checked = codes.iter().filter(|c| **c == ErrorCode::WrongPassword).count();
    assert!((1..=5).contains(&checked), "at most five checked: {codes:?}");
    assert!(codes.iter().all(|c| matches!(c, ErrorCode::WrongPassword | ErrorCode::TooManyAttempts)), "{codes:?}");
    assert_eq!(codes.iter().filter(|c| **c == ErrorCode::TooManyAttempts).count(), 20 - checked);
    drop((maker, devices));
    common::stop(dir.path(), daemon).await;
}

/// SEC1 M3: a name no board member can have answers `wrong_password` after the
/// same wait, but is not counted (nor kept); a password longer than any is wrong
/// too (SEC1 m7, R1 m-5), and too long for a new one.
#[tokio::test(flavor = "multi_thread")]
async fn a_name_that_cannot_exist_is_wrong_but_not_counted() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let maker = common::connect_anonymous(dir.path()).await;
    created(maker.user_create("alex", PASSWORD).await.unwrap());
    let huge = "n".repeat(100_000);
    for name in ["no spaces here", huge.as_str(), "x", "x", "x", "x", "x", "x"] {
        // A fresh connection each time: only the limits of the name could stop it.
        let device = common::connect_anonymous(dir.path()).await;
        let started = Instant::now();
        assert_eq!(code(device.user_login(name, PASSWORD, "test").await), ErrorCode::WrongPassword, "never too_many_attempts");
        assert!(started.elapsed() >= Duration::from_millis(400), "the same wait as a wrong password");
    }
    let device = common::connect_anonymous(dir.path()).await;
    let started = Instant::now();
    assert_eq!(code(device.user_login("alex", &"p".repeat(2_000), "test").await), ErrorCode::WrongPassword);
    assert!(started.elapsed() >= Duration::from_millis(400));
    // 1 024 characters of 4 bytes each are over the bound in bytes.
    assert_eq!(code(maker.user_create("robin", &"😀".repeat(1_024)).await), ErrorCode::WeakPassword);
    assert_eq!(code(maker.set_password(None, Some(PASSWORD), &"😀".repeat(1_024)).await), ErrorCode::WeakPassword);
    assert_eq!(device.user_login("alex", PASSWORD, "test").await.unwrap().user.username, "alex");
    drop((maker, device));
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_makes_at_most_five_board_members_a_minute() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let device = common::connect_anonymous(dir.path()).await;
    // Refused names and passwords cost nothing and do not count (S5-3).
    assert_eq!(code(device.user_create("x", PASSWORD).await), ErrorCode::InvalidUsername);
    assert_eq!(code(device.user_create("grace", &" ".repeat(12)).await), ErrorCode::WeakPassword, "QA-m7: only spaces");
    created(device.user_create("grace", PASSWORD).await.unwrap());
    // A taken name was hashed, so it counts.
    assert_eq!(code(device.user_create("GRACE", PASSWORD).await), ErrorCode::UsernameTaken);
    for name in ["ada", "alan", "edsger"] {
        created(device.user_create(name, PASSWORD).await.unwrap());
    }
    assert_eq!(code(device.user_create("barbara", PASSWORD).await), ErrorCode::TooManyAttempts);
    assert_eq!(device.user_list().await.unwrap().users.len(), 4);
    // Another connection has its own count (with a session: after the onboarding).
    let other = common::connect_anonymous(dir.path()).await;
    other.user_login("grace", PASSWORD, "test").await.unwrap();
    created(other.user_create("barbara", PASSWORD).await.unwrap());
    drop((device, other));
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_signed_in_device_makes_tickets_as_its_board_member() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let maker = common::connect_anonymous(dir.path()).await;
    let grace = created(maker.user_create("grace", PASSWORD).await.unwrap());
    let project = project(&maker).await;

    let device = common::connect_anonymous(dir.path()).await;
    assert_eq!(code(device.ticket_command(ticket(project, "Nobody"), None).await), ErrorCode::Unauthenticated);
    let login = device.user_login("grace", PASSWORD, "test").await.unwrap();
    assert_eq!((login.user.id, login.user.username.as_str()), (grace, "grace"));
    let made = device.ticket_command(ticket(project, "Signed in"), None).await.unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!() };
    assert_eq!(device.ticket(ticket_id).await.unwrap().ticket.created_by_user, grace);

    // Signed out: no board member on this connection any more.
    assert!(device.user_logout().await.unwrap().user.is_none());
    assert_eq!(code(device.ticket_command(ticket(project, "Nobody"), None).await), ErrorCode::Unauthenticated);
    drop((maker, device));
    common::stop(dir.path(), daemon).await;
}

/// Every text value in every table of the database.
fn dump(db: &rusqlite::Connection) -> String {
    let tables: Vec<String> = {
        let mut stmt = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    };
    let mut out = String::new();
    for table in tables {
        let mut stmt = db.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
        let columns = stmt.column_count();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            for i in 0..columns {
                match row.get_ref(i).unwrap() {
                    rusqlite::types::ValueRef::Text(t) | rusqlite::types::ValueRef::Blob(t) => {
                        out.push_str(&String::from_utf8_lossy(t));
                        out.push('\n');
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

/// Every file under `root` (blobs, uploads, the database and its WAL), as text.
fn files(root: &std::path::Path) -> String {
    let mut out = String::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                todo.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.push_str(&String::from_utf8_lossy(&bytes));
            }
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn a_password_is_nowhere_in_the_database_the_files_or_the_log() {
    let log = log();
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let client = common::connect_anonymous(dir.path()).await;
    let user = created(client.user_create("grace", PASSWORD).await.unwrap());
    let other = common::connect_anonymous(dir.path()).await;
    assert!(other.user_login("grace", "a-wrong-PASSWORD-guess", "test").await.is_err());
    let token = other.user_login("grace", PASSWORD, "test").await.unwrap().session_token;
    // A new password, and a hello with the token: neither may show up anywhere either.
    other.set_password(None, Some(PASSWORD), CHANGED).await.unwrap();
    other.resume(&token).await.unwrap();
    let project = project(&other).await;
    other.ticket_command(ticket(project, "After the onboarding"), None).await.unwrap();

    let db = common::db(dir.path());
    let hash: String = db.query_row("SELECT password_hash FROM users WHERE id = ?1", [user.to_string()], |r| r.get(0)).unwrap();
    assert!(hash.starts_with("$argon2id$v=19$m=65536,t=3,p=1$"), "{hash}");
    let receipts: String = db.query_row("SELECT group_concat(request_hash || result_json, ' ') FROM command_receipts", [], |r| r.get(0)).unwrap();
    let events: String = db.query_row("SELECT group_concat(type || metadata_json, ' ') FROM events", [], |r| r.get(0)).unwrap();
    assert!(events.contains("user.created"));
    for (place, text) in [("command_receipts", &receipts), ("events", &events)] {
        assert!(!text.contains(PASSWORD) && !text.contains(&hash), "the password or its hash is in {place}");
    }
    let everything = dump(&db);
    assert!(!everything.contains(PASSWORD) && !everything.contains("a-wrong-PASSWORD-guess") && !everything.contains(CHANGED), "the password is in the database");
    assert!(!everything.contains(&token), "the session token is in the database");
    assert_eq!(everything.matches(&hash).count(), 1, "the hash is only in users.password_hash");
    drop(db);
    drop((client, other));
    common::stop(dir.path(), daemon).await;
    let on_disk = files(dir.path());
    assert!(!on_disk.contains(PASSWORD) && !on_disk.contains("a-wrong-PASSWORD-guess") && !on_disk.contains(CHANGED) && !on_disk.contains(&token), "a secret is in a file of the data folder");
    let logged = String::from_utf8_lossy(&log.lock().unwrap()).into_owned();
    assert!(!logged.is_empty(), "the log was captured");
    assert!(!logged.contains(PASSWORD) && !logged.contains("a-wrong-PASSWORD-guess") && !logged.contains(&hash), "the password is in the log");
    assert!(!logged.contains(CHANGED) && !logged.contains(&token), "the new password or the session token is in the log");
    // L3/R4 (SEC1 m3): the failed sign-in is logged, by the name as counted, with the connection.
    let failed = logged.lines().find(|l| l.contains("sign-in failed") && l.contains("username=grace")).expect("the failed sign-in is logged");
    assert!(failed.contains("conn=") && failed.contains("failures=1"), "{failed}");
}

#[test]
fn the_password_parameters_print_redacted() {
    let create = aurelius_protocol::UserCreateParams { username: "grace".into(), password: PASSWORD.into() };
    let login = aurelius_protocol::LoginParams { username: "grace".into(), password: PASSWORD.into(), device_name: "test".into() };
    for printed in [format!("{create:?}"), format!("{login:?}"), format!("{:?}", aurelius_protocol::Method::UserLogin(login.clone()))] {
        assert!(!printed.contains(PASSWORD) && printed.contains("<redacted>"), "{printed}");
    }
    let write = aurelius_protocol::WriteMethod::Host(aurelius_protocol::HostWrite::UserCreate(create));
    assert!(!format!("{write:?}").contains(PASSWORD));
}
