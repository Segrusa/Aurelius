//! Shared helpers of the daemon's integration tests: start a daemon on a data
//! folder, connect a client, open its database read-only.
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
pub async fn start_with(root: &Path, runtime: Arc<dyn AgentRuntime>) -> Daemon {
    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(aurelius_daemon::run_with_runtime(root.to_path_buf(), Some(tx), runtime));
    tokio::time::timeout(Duration::from_secs(10), rx).await.expect("daemon ready").expect("ready signal");
    handle
}

/// Start a daemon without a runtime override, so runs use the provider's real CLI (`#[ignore]`d live tests).
pub async fn start_real(root: &Path) -> Daemon {
    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(aurelius_daemon::run(root.to_path_buf(), Some(tx)));
    tokio::time::timeout(Duration::from_secs(10), rx).await.expect("daemon ready").expect("ready signal");
    handle
}

/// A client on the daemon's local endpoint; events are dropped.
pub async fn connect(root: &Path) -> Client {
    let (client, rx) = connect_events(root).await;
    std::mem::forget(rx);
    client
}

/// A client plus the receiver of the typed events it subscribed to.
pub async fn connect_events(root: &Path) -> (Client, mpsc::UnboundedReceiver<Event>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let client = Client::connect(&aurelius_host::endpoint(root), "test", tx).await.expect("connect");
    (client, rx)
}

/// Stop the daemon over its internal channel and wait for it to end.
pub async fn stop(root: &Path, handle: Daemon) {
    aurelius_client::bridge::shutdown_daemon(root).await.expect("shutdown");
    tokio::time::timeout(Duration::from_secs(10), handle).await.expect("daemon stops").expect("join").expect("clean stop");
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

/// A conversation to talk in: a project conversation of a new project without
/// a folder (answers run text-only, as plain chats did). Since protocol 3.0
/// there are no standalone chats, so `chat.send` needs one.
pub async fn conversation(client: &Client) -> aurelius_domain::ConversationId {
    use aurelius_domain::{Command, CommandResult, ConversationId, project::CreateProject, project_conversation::StartProjectConversation};
    let name = format!("Talk {}", ConversationId::new());
    let created = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name, description: None, folder: None }), None)).await.expect("project");
    let CommandResult::ProjectCreated { project_id, .. } = created.result else { panic!("{:?}", created.result) };
    let start = StartProjectConversation { project_id, start: Default::default() };
    let started = client.execute(aurelius_client::envelope(Command::StartProjectConversation(start), None)).await.expect("conversation");
    let CommandResult::ProjectConversationStarted { conversation_id, .. } = started.result else { panic!("{:?}", started.result) };
    conversation_id
}

/// Send `text` in a new conversation (see `conversation`): its id, the message and the run.
pub async fn send_new(client: &Client, text: &str) -> (aurelius_domain::ConversationId, aurelius_domain::MessageId, aurelius_domain::RunId) {
    use aurelius_domain::{Command, CommandResult, conversation::SendChatMessage};
    let conversation_id = conversation(client).await;
    let send = SendChatMessage { conversation_id, text: text.into(), images: Vec::new(), interrupt: false };
    match client.execute(aurelius_client::envelope(Command::SendChatMessage(send), None)).await.expect("send").result {
        CommandResult::ChatMessageSent { conversation_id, message_id, run_id } => (conversation_id, message_id, run_id),
        other => panic!("{other:?}"),
    }
}
