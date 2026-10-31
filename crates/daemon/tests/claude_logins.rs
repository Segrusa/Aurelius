//! A board member's own Claude login (MU3-09, MU3-12, Sentinel SEC0 C1–C16)
//! against a daemon whose Claude Code connection is the stand-in `claude`
//! (`tests/fixtures/fake-claude`): connecting it through the PTY, what reaches
//! whom, ending a sign-in, unlinking, and the runs on each board member's own
//! login folder (no fallback, nothing of the daemon's environment).
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::{Client, ClientError, SessionPush, envelope};
use aurelius_domain::connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults};
use aurelius_domain::employee::Setting;
use aurelius_domain::user::ClaudeLoginStatus;
use aurelius_domain::{Command, CommandResult, ErrorCode};
use aurelius_protocol::ClaudeLoginPrompt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

/// The stand-in `claude` in a folder of its own (it writes its records next to itself).
pub fn fake_claude(root: &Path) -> PathBuf {
    let bin = root.join("fake-claude-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let path = bin.join("claude");
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-claude"), &path).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A Claude Code connection on `program`, the default for every employee run.
pub async fn claude_code(client: &Client, program: &Path) {
    let add = AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude Code".into(), runtime_path: Some(program.display().to_string()), default_model: None };
    let CommandResult::ProviderAdded { connection_id } = client.execute(envelope(Command::AddProvider(add), None)).await.unwrap().result else { panic!("not added") };
    for purpose in [Purpose::Employee, Purpose::Memory] {
        let settings = RunDefaults { connection: Setting::Value(connection_id), model: Setting::Value("sonnet".into()), persona: Setting::Inherit, effort: Setting::Inherit };
        client.execute(envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose, settings }), None)).await.unwrap();
    }
    // Verified (the program runs): ready.
    for _ in 0..200 {
        if client.connections().await.unwrap().connections.iter().any(|c| c.id == connection_id && c.status == "ready") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the Claude Code connection never became ready");
}

async fn prompt(pushes: &mut UnboundedReceiver<SessionPush>) -> ClaudeLoginPrompt {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(SessionPush::ClaudeLoginPrompt(p)) = pushes.recv().await {
                return p;
            }
        }
    })
    .await
    .expect("a prompt from the sign-in")
}

async fn nothing(pushes: &mut UnboundedReceiver<SessionPush>) {
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(pushes.try_recv().is_err(), "nothing reached this connection");
}

fn code<T: std::fmt::Debug>(result: Result<T, ClientError>) -> ErrorCode {
    match result {
        Err(ClientError::Protocol(e)) => e.code,
        other => panic!("expected an error, got {other:?}"),
    }
}

async fn status_until(client: &Client, status: ClaudeLoginStatus) -> aurelius_domain::user::ClaudeLoginView {
    for _ in 0..200 {
        let view = client.claude_login().await.unwrap();
        if view.status == status && view.link_id.is_none() {
            return view;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the login never became {status:?}");
}

fn last_pid(bin: &Path) -> u32 {
    std::fs::read_to_string(bin.join("login-pids")).unwrap().lines().last().unwrap().trim().parse().unwrap()
}

async fn gone(pid: u32, what: &str) {
    for _ in 0..100 {
        if !Path::new(&format!("/proc/{pid}")).exists() || std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| s.contains(") Z ")) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{what}: the sign-in process {pid} is still there");
}

/// Every text in every table of the database: what must never hold a secret.
pub fn whole_database(root: &Path) -> String {
    let conn = db(root);
    let tables: Vec<String> = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    let mut all = String::new();
    for table in tables {
        let Ok(mut stmt) = conn.prepare(&format!("SELECT * FROM \"{table}\"")) else { continue };
        let columns = stmt.column_count();
        let rows = stmt.query_map([], |r| Ok((0..columns).map(|i| r.get::<_, rusqlite::types::Value>(i).ok()).collect::<Vec<_>>())).unwrap();
        for row in rows.flatten() {
            for value in row.into_iter().flatten() {
                match value {
                    rusqlite::types::Value::Text(t) => all.push_str(&t),
                    rusqlite::types::Value::Blob(b) => all.push_str(&String::from_utf8_lossy(&b)),
                    _ => {}
                }
                all.push('\n');
            }
        }
    }
    all
}

#[tokio::test(flavor = "multi_thread")]
async fn a_board_member_connects_their_own_claude_login_through_the_pty() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let alex = connect_as(dir.path(), "alex").await;
    let mut alex_pushes = alex.session_pushes();
    let robin = connect_as(dir.path(), "robin").await;
    let mut robin_pushes = robin.session_pushes();
    let alex_elsewhere = connect_as(dir.path(), "alex").await;
    let mut elsewhere_pushes = alex_elsewhere.session_pushes();

    // Without a Claude Code connection there is nothing to sign in with.
    assert_eq!(code(alex.claude_login_link().await), ErrorCode::Validation);
    let program = fake_claude(dir.path());
    claude_code(&alex, &program).await;
    assert_eq!(alex.claude_login().await.unwrap().status, ClaudeLoginStatus::Missing);

    // C4: the link and the question go only to the connection that started it.
    let link = alex.claude_login_link().await.unwrap();
    assert_eq!(link.len(), 32, "128 random bits");
    let asked = prompt(&mut alex_pushes).await;
    assert_eq!((asked.link_id.as_str(), asked.asks_code, asked.error.as_deref()), (link.as_str(), true, None));
    assert!(asked.url.as_deref().unwrap().starts_with("https://claude.com/cai/oauth/authorize?"));
    assert!(!asked.url.as_deref().unwrap().contains('\u{1b}'), "without its terminal codes");
    nothing(&mut robin_pushes).await;
    nothing(&mut elsewhere_pushes).await;
    assert_eq!(alex.claude_login().await.unwrap().link_id.as_deref(), Some(link.as_str()));
    assert_eq!(robin.claude_login().await.unwrap().link_id, None, "Robin sees only Robin's own");

    // C5: another connection (of Alex too) or another board member: not found, alike.
    assert_eq!(code(alex_elsewhere.claude_login_input(&link, "good-code#FAKESTATE").await), ErrorCode::NotFound);
    assert_eq!(code(robin.claude_login_input(&link, "good-code#FAKESTATE").await), ErrorCode::NotFound);
    // C7: only a code goes in, never another line.
    assert_eq!(code(alex.claude_login_input(&link, "good\rcode").await), ErrorCode::Validation);
    // C14: a wrong code: Claude refuses it and ends; Aurelius says so in its own words.
    alex.claude_login_input(&link, "wrong-code-0000#FAKESTATE").await.unwrap();
    let refused = prompt(&mut alex_pushes).await;
    assert_eq!((refused.asks_code, refused.error.as_deref()), (false, Some("The code was not accepted. Start again.")));
    assert_eq!(status_until(&alex, ClaudeLoginStatus::Missing).await.email, None);
    assert_eq!(code(alex.claude_login_input(&link, "good-code#FAKESTATE").await), ErrorCode::NotFound, "once only");

    // A good code: linked, with what `claude auth status` says (MU3-09 AC1/AC2).
    let link = alex.claude_login_link().await.unwrap();
    prompt(&mut alex_pushes).await;
    alex.claude_login_input(&link, "good-code-1234#FAKESTATE").await.unwrap();
    let linked = status_until(&alex, ClaudeLoginStatus::Linked).await;
    assert_eq!((linked.email.as_deref(), linked.plan.as_deref()), (Some("test@example.com"), Some("max")));
    assert_eq!(robin.claude_login().await.unwrap().status, ClaudeLoginStatus::Missing, "B12: Robin's is Robin's");
    // D1: the login folder is the user's UUID, 0700, and holds what `claude` wrote.
    let me = alex.user_whoami().await.unwrap().user.unwrap().id;
    let folder = dir.path().join("logins").join(me.to_string());
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&folder).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(std::fs::metadata(dir.path().join("logins")).unwrap().permissions().mode() & 0o777, 0o700);
    assert!(folder.join(".fake-login").exists());

    // C4/C8/MU3-09 AC3: the link, its state and the codes are nowhere in the database.
    let all = whole_database(dir.path());
    for secret in ["FAKESTATE", "FAKECHALLENGE", "oauth/authorize", "good-code-1234", "wrong-code-0000"] {
        assert!(!all.contains(secret), "{secret} is in the database");
    }

    // MU3-09 AC4: unlink signs out with `claude auth logout` and removes the folder.
    let unlinked = alex.claude_login_unlink().await.unwrap();
    assert_eq!(unlinked.status, ClaudeLoginStatus::Missing);
    assert!(!folder.exists(), "the folder is gone");
    let calls = std::fs::read_to_string(program.parent().unwrap().join("runs.log")).unwrap();
    assert!(calls.lines().any(|l| l.contains(&format!("logins/{me}")) && l.ends_with("call=auth logout")), "{calls}");
    // C11: nothing of the daemon's environment reached any of those calls.
    assert!(calls.lines().all(|l| l.contains("leaked=[]")), "{calls}");

    // MU3-05 AC3 / C15: a disabled board member's login is signed out and its folder removed.
    let link = robin.claude_login_link().await.unwrap();
    prompt(&mut robin_pushes).await;
    robin.claude_login_input(&link, "good-robin#FAKESTATE").await.unwrap();
    status_until(&robin, ClaudeLoginStatus::Linked).await;
    let robin_id = robin.user_whoami().await.unwrap().user.unwrap().id;
    let robin_folder = dir.path().join("logins").join(robin_id.to_string());
    assert!(robin_folder.exists());
    alex.disable_user(robin_id).await.unwrap();
    assert!(!robin_folder.exists(), "gone with the board member");
    let calls = std::fs::read_to_string(program.parent().unwrap().join("runs.log")).unwrap();
    assert!(calls.lines().any(|l| l.contains(&format!("logins/{robin_id}")) && l.ends_with("call=auth logout")), "{calls}");
    assert_eq!(db(dir.path()).query_row("SELECT COUNT(*) FROM claude_logins WHERE user_id = ?1", [robin_id.to_string()], |r| r.get::<_, i64>(0)).unwrap(), 0);

    drop((alex, robin, alex_elsewhere));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sign_in_ends_with_cancel_with_its_connection_and_on_a_second_one() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let alex = connect_as(dir.path(), "alex").await;
    let mut pushes = alex.session_pushes();
    let program = fake_claude(dir.path());
    let bin = program.parent().unwrap().to_path_buf();
    claude_code(&alex, &program).await;

    // C13: cancel ends the process (and its group), and the board member hears it.
    let link = alex.claude_login_link().await.unwrap();
    prompt(&mut pushes).await;
    let pid = last_pid(&bin);
    assert_eq!(alex.claude_login_cancel(&link).await.unwrap().status, ClaudeLoginStatus::Missing);
    assert_eq!(prompt(&mut pushes).await.error.as_deref(), Some("The sign-in was cancelled."));
    gone(pid, "cancel").await;

    // C2: a second link ends the first one, explicitly.
    let first = alex.claude_login_link().await.unwrap();
    prompt(&mut pushes).await;
    let first_pid = last_pid(&bin);
    let second = alex.claude_login_link().await.unwrap();
    assert_ne!(first, second);
    let mut seen = Vec::new();
    for _ in 0..2 {
        seen.push(prompt(&mut pushes).await);
    }
    assert!(seen.iter().any(|p| p.link_id == first && p.error.is_some()), "the first one heard it was replaced");
    assert!(seen.iter().any(|p| p.link_id == second && p.asks_code));
    gone(first_pid, "replaced").await;

    // C13: the connection goes: its sign-in goes with it (a bare connection, closed whole).
    let token = sign_in(dir.path(), &alex, "alex").await.session_token;
    drop(alex);
    let pid = {
        use aurelius_protocol::{Call, HelloParams, HostWrite, Method, PROTOCOL_MAJOR, Request, ServerMessage, WriteMethod};
        let mut stream = aurelius_host::ipc::connect(&aurelius_host::endpoint(dir.path())).await.unwrap();
        let hello = HelloParams { client_name: "test".into(), client_version: "0".into(), app_key: None, accepts: vec![], session_token: Some(token) };
        let requests = [
            Request { protocol: PROTOCOL_MAJOR, request_id: 1, idempotency_key: None, expected_version: None, call: Call::Read(Method::SessionHello(hello)) },
            Request { protocol: PROTOCOL_MAJOR, request_id: 2, idempotency_key: Some(aurelius_domain::CommandId::new()), expected_version: None, call: Call::Write(WriteMethod::Host(HostWrite::ClaudeLoginLink)) },
        ];
        for request in &requests {
            aurelius_host::ipc::write_frame(&mut stream, request).await.unwrap();
        }
        loop {
            let message: ServerMessage = aurelius_host::ipc::read_frame(&mut stream).await.unwrap().expect("the daemon answers");
            if matches!(message, ServerMessage::ClaudeLoginPrompt(ref p) if p.asks_code) {
                break;
            }
        }
        last_pid(&bin)
    };
    gone(pid, "disconnected").await;

    // C16: at most five in ten minutes (four were started above).
    let alex = connect_as(dir.path(), "alex").await;
    let _pushes = alex.session_pushes();
    alex.claude_login_link().await.unwrap();
    let refused = alex.claude_login_link().await;
    match refused {
        Err(ClientError::Protocol(e)) => assert!(e.code == ErrorCode::TooManyAttempts && e.retry_after_ms.is_some(), "{e:?}"),
        other => panic!("{other:?}"),
    }
    drop(alex);
    stop(dir.path(), daemon).await;
}

/// R4 m-2: another board member signing in on the same connection is a switch,
/// as a logout: the earlier one's sign-in to Claude and terminals end there.
#[tokio::test(flavor = "multi_thread")]
async fn signing_in_as_another_board_member_ends_the_earlier_ones_sign_in_and_terminals() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    drop(connect_as(dir.path(), "robin").await);
    let alex = connect_as(dir.path(), "alex").await;
    let mut pushes = alex.session_pushes();
    let program = fake_claude(dir.path());
    let bin = program.parent().unwrap().to_path_buf();
    claude_code(&alex, &program).await;
    let folder = dir.path().join("shell-folder");
    std::fs::create_dir_all(&folder).unwrap();
    let create = aurelius_domain::employee::CreateEmployee { name: "Shell".into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: Some(folder.display().to_string()) };
    let CommandResult::EmployeeCreated { conversation_id: conversation, .. } = alex.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!("no employee") };
    let (view, _output) = alex.open_terminal(conversation, 80, 24, false).await.unwrap();
    let attached = |list: Vec<aurelius_protocol::TerminalSummary>| list.into_iter().find(|t| t.terminal_id == view.terminal_id).map(|t| t.attached);
    assert_eq!(attached(alex.list_terminals(conversation).await.unwrap()), Some(1));
    let link = alex.claude_login_link().await.unwrap();
    prompt(&mut pushes).await;
    let pid = last_pid(&bin);

    let robin = alex.user_login("robin", BOARD_PASSWORD, "test").await.unwrap();
    assert_eq!(robin.user.username, "robin");
    let ended = prompt(&mut pushes).await;
    assert!(ended.link_id == link && ended.error.is_some(), "{ended:?}");
    gone(pid, "another board member").await;
    assert_eq!(attached(alex.list_terminals(conversation).await.unwrap()), Some(0));

    // Signing in again as the same board member is no switch: a sign-in of it stays.
    let again = alex.claude_login_link().await.unwrap();
    prompt(&mut pushes).await;
    alex.user_login("robin", BOARD_PASSWORD, "test").await.unwrap();
    nothing(&mut pushes).await;
    alex.claude_login_cancel(&again).await.unwrap();
    drop(alex);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_code_that_is_not_whole_ends_the_sign_in_with_the_fixed_text() {
    // QA2 M1: CLI 2.1.312 answers an incomplete code with "Invalid code. …" and waits on.
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let alex = connect_as(dir.path(), "alex").await;
    let mut pushes = alex.session_pushes();
    let program = fake_claude(dir.path());
    let bin = program.parent().unwrap().to_path_buf();
    claude_code(&alex, &program).await;
    const REFUSED: Option<&str> = Some("The code was not accepted. Start again.");

    // Without its "#state" it is refused before it is written, with the same text, and the sign-in ends.
    let link = alex.claude_login_link().await.unwrap();
    prompt(&mut pushes).await;
    let pid = last_pid(&bin);
    alex.claude_login_input(&link, "abc").await.unwrap();
    let ended = prompt(&mut pushes).await;
    assert_eq!((ended.link_id.as_str(), ended.asks_code, ended.error.as_deref()), (link.as_str(), false, REFUSED));
    gone(pid, "refused before it was written").await;
    assert_eq!(status_until(&alex, ClaudeLoginStatus::Missing).await.link_id, None);

    // One Claude itself finds not whole: "Invalid code" is a refusal, the process is ended.
    let link = alex.claude_login_link().await.unwrap();
    prompt(&mut pushes).await;
    let pid = last_pid(&bin);
    alex.claude_login_input(&link, "incomplete#FAKESTATE").await.unwrap();
    let ended = prompt(&mut pushes).await;
    assert_eq!((ended.asks_code, ended.error.as_deref()), (false, REFUSED));
    gone(pid, "Invalid code").await;
    assert_eq!(status_until(&alex, ClaudeLoginStatus::Missing).await.email, None);

    drop(alex);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn what_claude_leaves_behind_ends_with_its_group_also_when_it_exits_by_itself() {
    // R2 m2 / SEC2 m5: after its own exit, the rest of the group is killed too.
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let alex = connect_as(dir.path(), "alex").await;
    let mut pushes = alex.session_pushes();
    let program = fake_claude(dir.path());
    let bin = program.parent().unwrap().to_path_buf();
    std::fs::write(bin.join("login-mode"), "child").unwrap();
    claude_code(&alex, &program).await;
    let link = alex.claude_login_link().await.unwrap();
    prompt(&mut pushes).await;
    let child: u32 = std::fs::read_to_string(bin.join("login-children")).unwrap().lines().last().unwrap().trim().parse().unwrap();
    // It signs in and exits 0 by itself.
    alex.claude_login_input(&link, "good-code#FAKESTATE").await.unwrap();
    status_until(&alex, ClaudeLoginStatus::Linked).await;
    gone(child, "a child that ignores the hang-up").await;
    drop(alex);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_api_keys_calls_run_on_the_board_member_who_made_or_last_set_it() {
    // F3 point 9 (B5): the session's board member, never an app; a key without one waits with 'login'.
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let alex = connect_as(dir.path(), "alex").await;
    let robin = connect_as(dir.path(), "robin").await;
    let login_of = |key: aurelius_domain::ApiKeyId| -> Option<String> {
        db(dir.path()).query_row("SELECT login_user FROM api_keys WHERE id = ?1", [key.to_string()], |r| r.get(0)).unwrap()
    };
    let created = alex.create_api_key("ci".into(), vec![]).await.unwrap();
    let alex_id = alex.user_whoami().await.unwrap().user.unwrap().id;
    assert_eq!(login_of(created.key.id), Some(alex_id.to_string()));
    // Another board member who sets its functions takes it on (as a key from before, without one, does then).
    robin.set_api_key_functions(created.key.id, vec![]).await.unwrap();
    let robin_id = robin.user_whoami().await.unwrap().user.unwrap().id;
    assert_eq!(login_of(created.key.id), Some(robin_id.to_string()));
    drop((alex, robin));
    stop(dir.path(), daemon).await;
}

/// Connect `client`'s own login with a code the stand-in accepts.
async fn link_login(client: &Client, pushes: &mut UnboundedReceiver<SessionPush>) {
    let link = client.claude_login_link().await.unwrap();
    prompt(pushes).await;
    client.claude_login_input(&link, "good-code#FAKESTATE").await.unwrap();
    status_until(client, ClaudeLoginStatus::Linked).await;
}

async fn employee(client: &Client, name: &str) -> (aurelius_domain::EmployeeId, aurelius_domain::ConversationId) {
    use aurelius_domain::employee::CreateEmployee;
    let create = CreateEmployee { name: name.into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: None };
    match client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result {
        CommandResult::EmployeeCreated { employee_id, conversation_id } => (employee_id, conversation_id),
        other => panic!("{other:?}"),
    }
}

async fn say(client: &Client, to: (aurelius_domain::EmployeeId, aurelius_domain::ConversationId), text: &str) -> aurelius_domain::RunId {
    use aurelius_domain::employee::SendEmployeeMessage;
    let send = SendEmployeeMessage { confirm_secret: false, employee_id: to.0, text: text.into(), conversation_id: to.1, images: vec![], files: vec![], interrupt: false };
    match client.execute(envelope(Command::SendEmployeeMessage(send), None)).await.unwrap().result {
        CommandResult::EmployeeMessageSent { run_id, .. } => run_id,
        other => panic!("{other:?}"),
    }
}

/// (status, error code, login) of a run once it is `status`.
async fn run_is(root: &Path, run: aurelius_domain::RunId, status: &str) -> (Option<String>, Option<String>) {
    for _ in 0..400 {
        let row: Option<(String, Option<String>, Option<String>)> =
            db(root).query_row("SELECT status, error_code, login_user FROM runs WHERE id = ?1", [run.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).ok();
        if let Some((now, error, login)) = row {
            if now == status {
                return (error, login);
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let now: String = db(root).query_row("SELECT status || ' ' || COALESCE(error_code, '') FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).unwrap();
    panic!("run {run} never became {status} (it is {now})");
}

/// §7a with the stand-in `claude`: two board members, each on their own login folder.
#[tokio::test(flavor = "multi_thread")]
async fn each_board_members_runs_use_their_own_login_folder_and_wait_without_one() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let alex = connect_as(dir.path(), "alex").await;
    let mut alex_pushes = alex.session_pushes();
    let robin = connect_as(dir.path(), "robin").await;
    let mut robin_pushes = robin.session_pushes();
    let program = fake_claude(dir.path());
    let bin = program.parent().unwrap().to_path_buf();
    claude_code(&alex, &program).await;
    let alex_id = alex.user_whoami().await.unwrap().user.unwrap().id;
    let robin_id = robin.user_whoami().await.unwrap().user.unwrap().id;
    let ada = employee(&alex, "Ada").await;

    // MU3-09 AC5: /usage per login; without one: "connect", never 0%; only one's own (B12).
    let usage = robin.subscription(true).await.unwrap();
    assert!(usage.limits.is_empty() && usage.error.as_deref().is_some_and(|e| e.contains("Connect your Claude login")), "{usage:?}");
    link_login(&alex, &mut alex_pushes).await;
    let usage = alex.subscription(true).await.unwrap();
    assert_eq!(usage.login_user, Some(alex_id));
    assert_eq!(usage.limits.iter().map(|l| l.percent).collect::<Vec<_>>(), [12, 34]);
    assert_eq!(usage.plan.as_deref(), Some("max"));
    assert!(robin.subscription(false).await.unwrap().limits.is_empty(), "Robin never sees Alex's");

    // MU3-12 AC1/AC5: Robin has no login: the run waits, nothing ran, never on another folder.
    let waiting = say(&robin, ada, "Hello from Robin").await;
    let (error, login) = run_is(dir.path(), waiting, "waiting").await;
    assert_eq!((error.as_deref(), login), (Some("login"), Some(robin_id.to_string())));
    let calls = || std::fs::read_to_string(bin.join("runs.log")).unwrap_or_default();
    assert!(!calls().contains(&format!("logins/{robin_id} ")), "no run started for Robin: {}", calls());
    let notes = robin.notifications().await.unwrap().notifications;
    assert_eq!(notes.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), ["Connect your Claude login"]);
    assert!(alex.notifications().await.unwrap().notifications.iter().all(|n| n.title != "Connect your Claude login"), "Robin's own Inbox only");

    // Alex's run: Alex's folder, nothing of the daemon's environment.
    let alexs = say(&alex, ada, "Hello from Alex").await;
    run_is(dir.path(), alexs, "succeeded").await;
    let line = calls().lines().rev().find(|l| l.contains("call=-p")).unwrap().to_string();
    assert!(line.starts_with(&format!("config={}", dir.path().join("logins").join(alex_id.to_string()).display())), "{line}");
    assert!(line.contains("leaked=[]"), "{line}");
    assert!(calls().lines().all(|l| !l.contains("/.claude")), "never the default folder: {}", calls());

    // B11: Robin connects: the waiting run goes by itself, on Robin's folder.
    link_login(&robin, &mut robin_pushes).await;
    run_is(dir.path(), waiting, "succeeded").await;
    assert!(calls().lines().any(|l| l.starts_with(&format!("config={}", dir.path().join("logins").join(robin_id.to_string()).display())) && l.contains("call=-p")));
    // An API login: no limits, no 0%.
    std::fs::write(dir.path().join("logins").join(robin_id.to_string()).join(".fake-api-only"), "").unwrap();
    let usage = robin.subscription(true).await.unwrap();
    assert!(usage.limits.is_empty() && usage.error.is_none() && usage.raw.contains("API key"), "{usage:?}");

    // MU3-11: Robin's limit: Robin's run waits; Alex's goes on.
    std::fs::write(bin.join("quota-users"), format!("{robin_id}\n")).unwrap();
    let limited = say(&robin, ada, "One more from Robin").await;
    assert_eq!(run_is(dir.path(), limited, "waiting").await.0.as_deref(), Some("quota"));
    let goes_on = say(&alex, ada, "And Alex goes on").await;
    run_is(dir.path(), goes_on, "succeeded").await;

    // A 401 in Alex's run: expired, and the run waits for the login.
    std::fs::write(bin.join("expired-users"), format!("{alex_id}\n")).unwrap();
    let refused = say(&alex, ada, "After the token expired").await;
    assert_eq!(run_is(dir.path(), refused, "waiting").await.0.as_deref(), Some("login"));
    assert_eq!(alex.claude_login().await.unwrap().status, ClaudeLoginStatus::Expired);
    assert_eq!(alex.notifications().await.unwrap().notifications.iter().filter(|n| n.title == "Reconnect your Claude login").count(), 1);
    // QA2 m2: Robin linked meanwhile: Robin's "Connect your Claude login" is resolved.
    assert!(robin.notifications().await.unwrap().notifications.iter().all(|n| n.title != "Connect your Claude login"));

    // R2 m5: after unlink, /usage no longer answers from what the old login said.
    assert_eq!(alex.subscription(false).await.unwrap().limits.len(), 2, "cached from before");
    alex.claude_login_unlink().await.unwrap();
    let usage = alex.subscription(false).await.unwrap();
    assert!(usage.limits.is_empty() && usage.plan.is_none(), "{usage:?}");

    drop((alex, robin));
    stop(dir.path(), daemon).await;
}
