//! Shared helpers of the daemon's integration tests: start a daemon on a data
//! folder, connect a client (signed in: protocol 5 lets nothing else through),
//! open its database read-only.
//!
//! Use with `mod common;` at the top of a test file. Items a given test does not
//! use would warn, hence the allow.
#![allow(dead_code)]

pub mod fake_mcp;

use aurelius_client::Client;
use aurelius_protocol::Event;
use aurelius_runtime::{AgentRuntime, fake::FakeRuntime};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub type Daemon = JoinHandle<anyhow::Result<()>>;

/// Start a daemon with the built-in fake runtime (answers come in 1 ms steps).
pub async fn start(root: &Path) -> Daemon {
    start_with_delay(root, 1).await
}

/// As `start`, with the fake runtime's streaming delay (milliseconds) of your choice.
/// The delay goes in with the runtime, not through the environment: tests of one binary run in parallel.
pub async fn start_with_delay(root: &Path, delay_ms: u64) -> Daemon {
    start_with(root, Arc::new(FakeRuntime { delay: Duration::from_millis(delay_ms), ..Default::default() })).await
}

/// Start a daemon whose every run uses `runtime`.
///
/// The runtime reports like an owner would (see `Reporting`): a ticket's work ends in Review,
/// as these tests were written for. Use `start_without_reports` for an owner that never reports.
pub async fn start_with(root: &Path, runtime: Arc<dyn AgentRuntime>) -> Daemon {
    let runtime: Arc<dyn AgentRuntime> = Arc::new(Reporting { inner: runtime, root: root.to_path_buf() });
    start_without_reports(root, runtime).await
}

/// `start_with`, whose runtime never calls `report_ready`: a ticket's main task waits for input at the end of its run.
pub async fn start_without_reports(root: &Path, runtime: Arc<dyn AgentRuntime>) -> Daemon {
    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(aurelius_daemon::run_with_runtime(root.to_path_buf(), Some(tx), runtime));
    tokio::time::timeout(Duration::from_secs(10), rx).await.expect("daemon ready").expect("ready signal");
    handle
}

/// Start a daemon without a runtime override: runs use the connection they resolve to, so the
/// real CLI in the `#[ignore]`d live tests, and no runtime at all when there is no connection.
pub async fn start_real(root: &Path) -> Daemon {
    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(aurelius_daemon::run(root.to_path_buf(), Some(tx)));
    tokio::time::timeout(Duration::from_secs(10), rx).await.expect("daemon ready").expect("ready signal");
    handle
}

/// The board member `connect` signs in as; made at the onboarding of a new server.
pub const BOARD_MEMBER: &str = "alex";
/// The password of every board member these helpers make.
pub const BOARD_PASSWORD: &str = "a-test-password-for-board-members";

/// A client on the daemon's local endpoint, signed in as `BOARD_MEMBER`; events are dropped.
pub async fn connect(root: &Path) -> Client {
    connect_as(root, BOARD_MEMBER).await
}

/// A client signed in as `name` (made with `BOARD_PASSWORD` when it does not exist yet).
pub async fn connect_as(root: &Path, name: &str) -> Client {
    let (client, rx) = connect_events_as(root, name).await;
    std::mem::forget(rx);
    client
}

/// A client plus the receiver of the typed events it subscribed to, signed in as `BOARD_MEMBER`.
pub async fn connect_events(root: &Path) -> (Client, mpsc::UnboundedReceiver<Event>) {
    connect_events_as(root, BOARD_MEMBER).await
}

pub async fn connect_events_as(root: &Path, name: &str) -> (Client, mpsc::UnboundedReceiver<Event>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let client = Client::connect(&aurelius_host::endpoint(root), "test", tx).await.expect("connect");
    sign_in(root, &client, name).await;
    (client, rx)
}

/// The data folder of each daemon these helpers signed in to, by its host id (one per folder):
/// `conversation` writes its empty conversation there, as no method makes one any more (OPR-3).
static ROOTS: std::sync::Mutex<std::collections::BTreeMap<String, std::path::PathBuf>> = std::sync::Mutex::new(std::collections::BTreeMap::new());

async fn remember_root(root: &Path, client: &Client) {
    let host = client.health().await.expect("health").host_id.to_string();
    ROOTS.lock().unwrap().insert(host, root.to_path_buf());
}

/// A client without a session: only the hello, the sign-in and the onboarding work.
pub async fn connect_anonymous(root: &Path) -> Client {
    let (tx, rx) = mpsc::unbounded_channel();
    let client = Client::connect(&aurelius_host::endpoint(root), "test", tx).await.expect("connect");
    std::mem::forget(rx);
    client
}

/// Sign `client` in as `name`: the first board member is made at the onboarding,
/// a later one by `BOARD_MEMBER` (or another board member these helpers made:
/// they share the password). Looks first whether `name` exists, so a new name
/// costs no failed sign-in (R1 m-11). Answers the session.
pub fn sign_in<'a>(root: &'a Path, client: &'a Client, name: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = aurelius_domain::user::LoginResult> + Send + 'a>> {
    Box::pin(async move {
        let session = sign_in_once(root, client, name).await;
        remember_root(root, client).await;
        session
    })
}

/// `sign_in` without remembering the data folder.
async fn sign_in_once(root: &Path, client: &Client, name: &str) -> aurelius_domain::user::LoginResult {
    if !client.user_whoami().await.expect("whoami").board_members_exist {
        // The onboarding; another connection may make the first one at the same time.
        if client.user_create(name, BOARD_PASSWORD).await.is_ok() {
            return client.user_login(name, BOARD_PASSWORD, "test").await.expect("sign in");
        }
    }
    // A maker signed in with the known password: `BOARD_MEMBER` first, else one of the others.
    let maker = connect_anonymous(root).await;
    let mut signed_in = maker.user_login(BOARD_MEMBER, BOARD_PASSWORD, "test").await.is_ok();
    if !signed_in {
        for username in usernames(root) {
            if maker.user_login(&username, BOARD_PASSWORD, "test").await.is_ok() {
                signed_in = true;
                break;
            }
        }
    }
    assert!(signed_in, "no board member to make {name}: the test made its own with other passwords");
    let exists = maker.user_list().await.expect("board members").users.iter().any(|u| u.username.eq_ignore_ascii_case(name));
    if !exists {
        // Another connection may make the same one at the same time.
        let _ = maker.user_create(name, BOARD_PASSWORD).await;
    }
    client.user_login(name, BOARD_PASSWORD, "test").await.expect("sign in")
}

/// The active board members, oldest first, from the database (without a
/// session `user.list` names nobody).
fn usernames(root: &Path) -> Vec<String> {
    let db = db(root);
    let mut stmt = db.prepare("SELECT username FROM users WHERE disabled_at_ms IS NULL ORDER BY created_at_ms").unwrap();
    stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

/// Stop the daemon over its internal channel and wait for it to end.
pub async fn stop(root: &Path, handle: Daemon) {
    aurelius_client::bridge::shutdown_daemon(root).await.expect("shutdown");
    tokio::time::timeout(Duration::from_secs(10), handle).await.expect("daemon stops").expect("join").expect("clean stop");
}

/// Make `folder` a git repository with one commit on main: a work session works on a
/// branch of its own there (or in a worktree of its own), never on what it has checked out.
pub fn git_repo(folder: &Path) {
    std::fs::create_dir_all(folder).unwrap();
    for args in [
        &["init", "--quiet", "--initial-branch=main"][..],
        &["config", "user.name", "Test"],
        &["config", "user.email", "test@example.com"],
        &["commit", "--quiet", "--allow-empty", "-m", "first"],
    ] {
        let out = std::process::Command::new("git").arg("-C").arg(folder).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
}

/// The daemon's database, read-only.
pub fn db(root: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(root.join("data/aurelius.sqlite"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}

/// Let anyone give work to anyone: "Follow the org chart" off (the company
/// follows its chart by default, where peers may not delegate to each other).
pub async fn free_chart(client: &Client) {
    use aurelius_domain::{Command, employee::SetFollowOrgChart};
    client.execute(aurelius_client::envelope(Command::SetFollowOrgChart(SetFollowOrgChart { follow: false }), None)).await.expect("follow the org chart off");
}

/// A conversation to talk in: an empty project conversation of a new project without
/// a folder (answers run text-only, as plain chats did). Since protocol 3.0
/// there are no standalone chats, so `chat.send` needs one; `project_conversation.start`
/// answers gone (OPR-3), so it is written straight into the daemon's database.
pub async fn conversation(client: &Client) -> aurelius_domain::ConversationId {
    use aurelius_domain::{Command, CommandResult, ConversationId, project::CreateProject};
    let name = format!("Talk {}", ConversationId::new());
    let created = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name, description: None, folder: None }), None)).await.expect("project");
    let CommandResult::ProjectCreated { project_id, .. } = created.result else { panic!("{:?}", created.result) };
    let host = client.health().await.expect("health").host_id.to_string();
    let found = ROOTS.lock().unwrap().get(&host).cloned();
    let root = found.expect("a client signed in with `sign_in` (as connect and connect_as do)");
    let store = aurelius_storage::Store::open(&root).expect("the daemon's database");
    aurelius_application::conversation::project_chat::empty_for_tests(&store, project_id).expect("an empty project conversation")
}

/// Send `text` in a new conversation (see `conversation`): its id, the message and the run.
pub async fn send_new(client: &Client, text: &str) -> (aurelius_domain::ConversationId, aurelius_domain::MessageId, aurelius_domain::RunId) {
    use aurelius_domain::{Command, CommandResult, conversation::SendChatMessage};
    let conversation_id = conversation(client).await;
    let send = SendChatMessage { confirm_secret: false, conversation_id, text: text.into(), images: Vec::new(), files: vec![], interrupt: false };
    match client.execute(aurelius_client::envelope(Command::SendChatMessage(send), None)).await.expect("send").result {
        CommandResult::ChatMessageSent { conversation_id, message_id, run_id } => (conversation_id, message_id, run_id),
        other => panic!("{other:?}"),
    }
}

/// A ticket in Backlog of `project_id`: nothing starts, but `delegate_task` can name it (work always belongs to a ticket).
pub async fn backlog_ticket(client: &Client, project_id: aurelius_domain::ProjectId, title: &str) -> aurelius_domain::TicketId {
    let made = client
        .create_ticket(aurelius_domain::ticket::CreateTicket {
            responsible_user_id: None,
            confirm_secret: false,
            project_id,
            title: title.into(),
            description: None,
            priority: Default::default(),
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
        .await
        .unwrap();
    let aurelius_domain::CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!("{:?}", made.result) };
    ticket_id
}

/// The discussion of a ticket made for the purpose: the place where a test writes in a channel (a project's
/// channel of the Team chat was that once).
pub async fn talk_channel(client: &Client, project_id: aurelius_domain::ProjectId) -> aurelius_domain::ConversationId {
    let ticket = backlog_ticket(client, project_id, "Talk").await;
    client.ticket(ticket).await.unwrap().ticket.conversation_id.expect("a discussion")
}

/// A runtime that, in a run offered `report_ready` (the owner's run of a ticket's main task),
/// reports the work ready through the daemon's bridge before the run starts, as an owner
/// that finished would. Every other run is the inner runtime's, unchanged.
pub struct Reporting {
    pub inner: Arc<dyn AgentRuntime>,
    pub root: std::path::PathBuf,
}

#[async_trait::async_trait]
impl AgentRuntime for Reporting {
    async fn probe(&self) -> aurelius_runtime::RuntimeCapabilities {
        self.inner.probe().await
    }
    async fn start(&self, request: aurelius_runtime::RunRequest) -> Result<aurelius_runtime::RunHandle, aurelius_runtime::RuntimeError> {
        let offered = request.system.as_deref().is_some_and(|s| s.contains("report_ready")) || request.turns.iter().any(|t| t.text.contains("call report_ready"));
        if offered && let Ok(bridge) = aurelius_client::bridge::BridgeClient::connect(&self.root).await {
            let _ = bridge.employee_act(request.run_id, aurelius_protocol::EmployeeTaskOp::ReportReady { report: "Report: the work is done.".into() }).await;
        }
        self.inner.start(request).await
    }
    fn keeps_sessions(&self) -> bool {
        self.inner.keeps_sessions()
    }
    fn has_session(&self, id: &str) -> bool {
        self.inner.has_session(id)
    }
    fn remove_session(&self, id: &str) {
        self.inner.remove_session(id)
    }
    fn with_vault(&self, vault: aurelius_runtime::claude_code::VaultEnv) -> Option<Arc<dyn AgentRuntime>> {
        let inner = self.inner.with_vault(vault)?;
        Some(Arc::new(Reporting { inner, root: self.root.clone() }))
    }
}
