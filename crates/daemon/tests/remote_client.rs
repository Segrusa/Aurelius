//! A desktop on another computer: pairs with the pairing text as
//! owner, then uses the same typed `Client` over the gateway's WebSocket as
//! a local window uses over the pipe.

mod common;

use aurelius_client::{Client, ClientError, RemoteServer, envelope};
use aurelius_domain::{Command, CommandId, CommandEnvelope, EventKind, HostId, conversation::SendChatMessage, files::AttachFile, project::CreateProject};
use aurelius_protocol::{Event, HostWrite, StreamDelta};
use common::*;
use std::time::Duration;
use tokio::sync::mpsc;

async fn start_remote(root: &std::path::Path) -> Daemon {
    // SAFETY (test): every test in this binary sets the same values.
    unsafe {
        std::env::set_var("AURELIUS_REMOTE_PORT", "0");
        std::env::set_var("AURELIUS_ENV", "dev");
    }
    start_with_delay(root, 5).await
}

/// Pair "another computer" through the pairing text; the server's window allows it as owner.
async fn pair_computer(server: &Client) -> RemoteServer {
    server.remote(HostWrite::RemoteEnable).await.unwrap();
    let code = server.pairing_start().await.unwrap();
    let pairing = tokio::spawn(async move { aurelius_client::remote::pair(&code.pairing_text, "test-mac", |_| {}).await });
    let pending = loop {
        let status = server.remote_status().await.unwrap();
        if let Some(p) = status.pending.into_iter().next() {
            break p;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(pending.owner, "a computer asks for full access");
    server.remote(HostWrite::RemoteDecidePairing { pairing_id: pending.pairing_id, approve: true, owner: true }).await.unwrap();
    pairing.await.unwrap().unwrap()
}

type Channels = (mpsc::UnboundedReceiver<Event>, mpsc::UnboundedReceiver<StreamDelta>);

async fn connect_remote(server: &RemoteServer) -> Result<(Client, Channels), ClientError> {
    let (events_tx, events) = mpsc::unbounded_channel();
    let (streams_tx, streams) = mpsc::unbounded_channel();
    let client = Client::connect_remote(server, "remote-window", events_tx, Some(streams_tx)).await?;
    // The board member `connect` made on the server signs in on this device too.
    client.user_login(common::BOARD_MEMBER, common::BOARD_PASSWORD, "remote window").await?;
    Ok((client, (events, streams)))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_remote_window_works_like_a_local_one() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let server_window = connect(dir.path()).await;
    let paired = pair_computer(&server_window).await;
    assert_eq!(paired.host_id, server_window.hello.host_id);
    assert!(paired.url.starts_with("http://127.0.0.1:"));

    let (client, (mut events, mut streams)) = connect_remote(&paired).await.unwrap();
    assert!(client.is_remote());
    assert!(client.hello.remote);
    assert_eq!(client.hello.client_id, Some(paired.client_id));

    // Live chat: appends on the wire, full text (and activity) for the window.
    let after = client.health().await.unwrap().last_seq;
    client.subscribe(after).await.unwrap();
    let talk = common::conversation(&client).await;
    let send = Command::SendChatMessage(SendChatMessage { conversation_id: talk, text: "Tell me a long story".into(), images: Vec::new(), interrupt: false });
    client.execute(envelope(send, None)).await.unwrap();
    let mut last = None::<StreamDelta>;
    loop {
        tokio::select! {
            Some(delta) = streams.recv() => {
                if let Some(previous) = last.as_ref().filter(|p| p.message_id == delta.message_id) {
                    assert!(delta.text.starts_with(&previous.text), "text only grows while streaming");
                }
                last = Some(delta);
            }
            Some(event) = events.recv() => if event.kind == EventKind::MessageAssistantCommitted { break },
            _ = tokio::time::sleep(Duration::from_secs(20)) => panic!("no answer"),
        }
    }
    let last = last.expect("streamed text arrived");
    let committed = client.messages(last.conversation_id).await.unwrap();
    let answer = committed.messages.iter().find(|m| m.id == last.message_id).expect("the streamed message");
    assert!(answer.text.starts_with(&last.text), "the rebuilt text is the message so far");

    // A file from this computer: uploaded, then attached by its server path.
    let local_file = dir.path().join("from-the-mac.txt");
    std::fs::write(&local_file, "hello from the other computer").unwrap();
    let staged = client.stage_file(&local_file).await.unwrap();
    assert_ne!(staged, local_file.display().to_string());
    let attach = Command::AttachFile(AttachFile { conversation_id: talk, path: staged });
    client.execute(envelope(attach, None)).await.unwrap();

    // Folders and Claude Code on the server, not on this computer.
    assert!(!client.list_dir(None).await.unwrap().roots.is_empty());
    client.detect_runtime().await.unwrap();

    // R02 over the network: the same command id after a reconnect runs once.
    let create = CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: None,
        command: Command::CreateProject(CreateProject { name: "Twice".into(), description: None, folder: None }),
    };
    assert!(!client.execute(create.clone()).await.unwrap().replayed);
    drop(client);
    let (again, _channels) = connect_remote(&paired).await.unwrap();
    assert!(again.execute(create).await.unwrap().replayed);
    assert_eq!(server_window.projects().await.unwrap().projects.iter().filter(|p| p.name == "Twice").count(), 1);

    // Another server at the paired address is refused.
    let mut elsewhere = paired.clone();
    elsewhere.host_id = HostId::new();
    assert!(matches!(connect_remote(&elsewhere).await.err(), Some(ClientError::WrongServer(_))));

    // Removed on the server: the open connection ends as "unauthorized", and so does a new one.
    server_window.remote(HostWrite::RemoteRevoke { client_id: paired.client_id }).await.unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match again.health().await {
                Err(err) if err.is_final() => break err,
                _ => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
    })
    .await
    .expect("the revoked connection ends");
    assert!(matches!(ended, ClientError::Unauthorized(_)), "{ended:?}");
    assert!(matches!(connect_remote(&paired).await.err(), Some(ClientError::Unauthorized(_))));

    stop(dir.path(), daemon).await;
}
