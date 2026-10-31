//! Board members over the protocol (Fase 1, S5): the onboarding, `user.*`, the
//! hello fields, `board_member_required` on ticket writes, and that a password
//! (or its hash) never ends up in the database, a receipt, an event or the log.

mod common;

use aurelius_client::{Client, ClientError};
use aurelius_domain::app_key::{AppScope, CreateAppKey};
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{CreateTicket, TicketPriority};
use aurelius_domain::{Command, CommandResult, ErrorCode, ProjectId, UserId};
use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const PASSWORD: &str = "Tr0ub4dor&3-zebra-UNIQUE";

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
    let first = common::connect(dir.path()).await;
    // MU1-20 AC1: a new server has no board members; the client shows the onboarding.
    let hello = first.identify(UserId::new()).await.unwrap();
    assert_eq!((hello.board_members_exist, hello.user), (false, None));
    let project = project(&first).await;
    assert_eq!(code(first.ticket_command(ticket(project, "Too early"), None).await), ErrorCode::BoardMemberRequired);

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
    let second = common::connect(dir.path()).await;
    let who = second.user_whoami().await.unwrap();
    assert_eq!((who.board_members_exist, who.user), (true, None));
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

#[tokio::test(flavor = "multi_thread")]
async fn two_first_board_members_at_once_only_clash_on_the_same_name() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let (a, b) = (common::connect(dir.path()).await, common::connect(dir.path()).await);
    let (one, two) = tokio::join!(a.user_create("first", PASSWORD), b.user_create("second", PASSWORD));
    assert!(one.is_ok() && two.is_ok(), "MU1-20 AC4: different names both work");
    let (c, d) = (common::connect(dir.path()).await, common::connect(dir.path()).await);
    let (three, four) = tokio::join!(c.user_create("same", PASSWORD), d.user_create("SAME", PASSWORD));
    let codes: Vec<Option<ErrorCode>> = [three, four].into_iter().map(|r| r.err().map(|e| match e { ClientError::Protocol(p) => p.code, other => panic!("{other:?}") })).collect();
    assert_eq!(codes.iter().filter(|c| c.is_none()).count(), 1, "{codes:?}");
    assert!(codes.contains(&Some(ErrorCode::UsernameTaken)));
    assert_eq!(a.user_list().await.unwrap().users.len(), 3);
    drop((a, b, c, d));
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn user_use_checks_the_password_once_with_a_delay_and_a_limit() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let maker = common::connect(dir.path()).await;
    let grace = created(maker.user_create("grace", PASSWORD).await.unwrap());

    let device = common::connect(dir.path()).await;
    let started = Instant::now();
    assert_eq!(code(device.user_use(grace, "wrong password!!").await), ErrorCode::WrongPassword);
    assert!(started.elapsed() >= Duration::from_millis(400), "a wrong answer takes a while");
    // An unknown board member gets the very same answer.
    assert_eq!(code(device.user_use(UserId::new(), PASSWORD).await), ErrorCode::WrongPassword);
    assert!(device.user_whoami().await.unwrap().user.is_none());
    let me = device.user_use(grace, PASSWORD).await.unwrap();
    assert_eq!(me.user.map(|u| u.id), Some(grace));
    // Five tries a minute on one connection, right or wrong.
    assert_eq!(code(device.user_use(grace, "wrong password!!").await), ErrorCode::WrongPassword);
    assert_eq!(code(device.user_use(grace, "wrong password!!").await), ErrorCode::WrongPassword);
    assert_eq!(code(device.user_use(grace, PASSWORD).await), ErrorCode::TooManyAttempts);
    assert_eq!(device.user_whoami().await.unwrap().user.map(|u| u.id), Some(grace), "a refused try changes nothing");
    // Another connection has its own tries.
    let other = common::connect(dir.path()).await;
    assert!(other.user_use(grace, PASSWORD).await.is_ok());
    drop((maker, device, other));
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_makes_at_most_five_board_members_a_minute() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let device = common::connect(dir.path()).await;
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
    // Another connection has its own count.
    let other = common::connect(dir.path()).await;
    created(other.user_create("barbara", PASSWORD).await.unwrap());
    drop((device, other));
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_hello_brings_back_a_remembered_board_member() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let maker = common::connect(dir.path()).await;
    let grace = created(maker.user_create("grace", PASSWORD).await.unwrap());

    let device = common::connect(dir.path()).await;
    let hello = device.identify(grace).await.unwrap();
    assert!(hello.board_members_exist);
    assert_eq!(hello.user.map(|u| (u.id, u.username)), Some((grace, "grace".to_owned())));
    let project = project(&device).await;
    let made = device.ticket_command(ticket(project, "Remembered"), None).await.unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!() };
    assert_eq!(device.ticket(ticket_id).await.unwrap().ticket.created_by_user, grace);

    // A remembered board member that does not exist (a restore): back to the choice.
    let hello = device.identify(UserId::new()).await.unwrap();
    assert_eq!((hello.board_members_exist, hello.user), (true, None));
    assert_eq!(code(device.ticket_command(ticket(project, "Nobody"), None).await), ErrorCode::BoardMemberRequired);
    drop((maker, device));
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn apps_may_list_board_members_but_never_create_or_become_one() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let owner = common::connect(dir.path()).await;
    let grace = created(owner.user_create("grace", PASSWORD).await.unwrap());
    let project = project(&owner).await;
    let key = owner
        .create_app_key(CreateAppKey { name: "assistant".into(), scopes: AppScope::ALL.to_vec(), limit_per_hour: None, limit_concurrent: None, audience: Default::default(), webhook_url: None })
        .await
        .unwrap();
    let app = Client::connect_app(&aurelius_host::endpoint(dir.path()), &key.secret, "test app").await.unwrap();
    assert_eq!(app.user_list().await.unwrap().users.len(), 1);
    assert_eq!(code(app.user_create("robot", PASSWORD).await), ErrorCode::Forbidden);
    assert_eq!(code(app.user_use(grace, PASSWORD).await), ErrorCode::Forbidden);
    // A remembered board member in an app's hello is ignored: apps have none.
    assert!(app.identify(grace).await.unwrap().user.is_none());
    // So tickets are out of reach (board_member_required), and the review gate is refused outright.
    assert_eq!(code(app.ticket_command(ticket(project, "From an app"), None).await), ErrorCode::BoardMemberRequired);
    let accept = Command::AcceptTicket(aurelius_domain::ticket::AcceptTicket { ticket_id: aurelius_domain::TicketId::new(), note: None });
    assert_eq!(code(app.ticket_command(accept, None).await), ErrorCode::Forbidden);
    drop((owner, app));
    common::stop(dir.path(), daemon).await;
}

/// S5-1: a connection that names a board member and then says hello with an app
/// key acts for the app only; the board member is gone, so tickets are refused.
#[tokio::test(flavor = "multi_thread")]
async fn an_app_key_after_a_board_member_drops_the_board_member() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let owner = common::connect(dir.path()).await;
    let grace = created(owner.user_create("grace", PASSWORD).await.unwrap());
    let project = project(&owner).await;
    let key = owner
        .create_app_key(CreateAppKey { name: "assistant".into(), scopes: AppScope::ALL.to_vec(), limit_per_hour: None, limit_concurrent: None, audience: Default::default(), webhook_url: None })
        .await
        .unwrap();
    let mixed = common::connect(dir.path()).await;
    assert_eq!(mixed.identify(grace).await.unwrap().user.map(|u| u.id), Some(grace));
    let hello = aurelius_protocol::HelloParams {
        client_name: "test app".into(),
        client_version: "0".into(),
        app_key: Some(key.secret.clone()),
        accepts: vec![],
        user_id: None,
    };
    let answer = mixed.call(aurelius_protocol::Method::SessionHello(hello)).await.unwrap();
    let aurelius_protocol::ResponseBody::Hello(answer) = answer else { panic!("{answer:?}") };
    assert!(answer.user.is_none(), "the app key drops the board member");
    assert!(mixed.user_whoami().await.unwrap().user.is_none());
    assert_eq!(code(mixed.ticket_command(ticket(project, "From an app"), None).await), ErrorCode::BoardMemberRequired);
    // And naming the board member again does not bring it back for the app.
    assert!(mixed.identify(grace).await.unwrap().user.is_none());
    assert_eq!(code(mixed.ticket_command(ticket(project, "From an app"), None).await), ErrorCode::BoardMemberRequired);
    let db = common::db(dir.path());
    let tickets: i64 = db.query_row("SELECT COUNT(*) FROM tickets", [], |r| r.get(0)).unwrap();
    assert_eq!(tickets, 0);
    drop((owner, mixed));
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
    let client = common::connect(dir.path()).await;
    let user = created(client.user_create("grace", PASSWORD).await.unwrap());
    let other = common::connect(dir.path()).await;
    assert!(other.user_use(user, "a-wrong-PASSWORD-guess").await.is_err());
    other.user_use(user, PASSWORD).await.unwrap();
    let project = project(&other).await;
    other.ticket_command(ticket(project, "After the onboarding"), None).await.unwrap();

    let db = common::db(dir.path());
    let hash: String = db.query_row("SELECT password_hash FROM users WHERE id = ?1", [user.to_string()], |r| r.get(0)).unwrap();
    assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"), "{hash}");
    let receipts: String = db.query_row("SELECT group_concat(request_hash || result_json, ' ') FROM command_receipts", [], |r| r.get(0)).unwrap();
    let events: String = db.query_row("SELECT group_concat(type || metadata_json, ' ') FROM events", [], |r| r.get(0)).unwrap();
    assert!(events.contains("user.created"));
    for (place, text) in [("command_receipts", &receipts), ("events", &events)] {
        assert!(!text.contains(PASSWORD) && !text.contains(&hash), "the password or its hash is in {place}");
    }
    let everything = dump(&db);
    assert!(!everything.contains(PASSWORD) && !everything.contains("a-wrong-PASSWORD-guess"), "the password is in the database");
    assert_eq!(everything.matches(&hash).count(), 1, "the hash is only in users.password_hash");
    drop(db);
    drop((client, other));
    common::stop(dir.path(), daemon).await;
    let on_disk = files(dir.path());
    assert!(!on_disk.contains(PASSWORD) && !on_disk.contains("a-wrong-PASSWORD-guess"), "the password is in a file of the data folder");
    let logged = String::from_utf8_lossy(&log.lock().unwrap()).into_owned();
    assert!(!logged.is_empty(), "the log was captured");
    assert!(!logged.contains(PASSWORD) && !logged.contains("a-wrong-PASSWORD-guess") && !logged.contains(&hash), "the password is in the log");
}

#[test]
fn the_password_parameters_print_redacted() {
    let create = aurelius_protocol::UserCreateParams { username: "grace".into(), password: PASSWORD.into() };
    let used = aurelius_protocol::UserUseParams { user_id: UserId::new(), password: PASSWORD.into() };
    for printed in [format!("{create:?}"), format!("{used:?}"), format!("{:?}", aurelius_protocol::Method::UserUse(used.clone()))] {
        assert!(!printed.contains(PASSWORD) && printed.contains("<redacted>"), "{printed}");
    }
    let write = aurelius_protocol::WriteMethod::Host(aurelius_protocol::HostWrite::UserCreate(create));
    assert!(!format!("{write:?}").contains(PASSWORD));
}
