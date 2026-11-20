//! End-to-end tests over the real local IPC endpoint: durability across
//! restarts, and the behavior of the one write pipeline (idempotency keys,
//! receipts and their replay), the handshake, the bridge endpoint and events.

mod common;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::envelope;
use aurelius_domain::{
    Command, CommandId, RunId,
    conversation::SendChatMessage,
    project::CreateProject,
};
use aurelius_host::ipc::{self, BoxStream, read_frame, write_frame};
use aurelius_protocol::{
    CommandResult, ErrorCode, EventKind, HelloParams, HostResult, HostWrite, Method, Outcome, Request, ResponseBody, ServerMessage, WriteMethod, WriteResult,
};
use common::*;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::oneshot;

fn create(name: &str) -> Command {
    Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None })
}

fn tempdir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    (dir, root)
}

/// One raw connection to the client endpoint, signed in (protocol 5: the gate).
async fn raw(root: &Path) -> BoxStream {
    let token = common::sign_in(root, &common::connect_anonymous(root).await, common::BOARD_MEMBER).await.session_token;
    let mut stream = ipc::connect(&aurelius_host::endpoint(root)).await.unwrap();
    let hello = Request::read(0, Method::SessionHello(HelloParams { client_name: "raw".into(), client_version: "0".into(), accepts: Vec::new(), session_token: Some(token) }));
    assert!(matches!(ask(&mut stream, &hello).await, Outcome::Ok(ResponseBody::Hello(h)) if h.user.is_some()), "signed in");
    stream
}

/// Send one request and wait for its response (other messages are skipped).
async fn ask(stream: &mut BoxStream, request: &Request) -> Outcome {
    write_frame(stream, request).await.unwrap();
    loop {
        match read_frame::<ServerMessage, _>(stream).await.unwrap().expect("the connection stays open") {
            ServerMessage::Response { request_id, outcome } if request_id == request.request_id => return outcome,
            _ => {}
        }
    }
}

fn error_code(outcome: Outcome) -> ErrorCode {
    match outcome {
        Outcome::Error(err) => err.code,
        other => panic!("expected an error, got {other:?}"),
    }
}

fn hello(request_id: u64, protocol: u16) -> Request {
    let mut request = Request::read(request_id, Method::SessionHello(HelloParams { client_name: "raw".into(), client_version: "0".into(), accepts: Vec::new(), session_token: None }));
    request.protocol = protocol;
    request
}

// ---- durability ----

#[tokio::test(flavor = "multi_thread")]
async fn project_persists_across_daemon_restart() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let client = connect(&root).await;
    let host = client.hello.host_id;
    let receipt = client.execute(envelope(create("Atlas"), None)).await.unwrap();
    stop(&root, daemon).await;

    let daemon = start(&root).await;
    let client = connect(&root).await;
    assert_eq!(client.hello.host_id, host, "host_id is stable across restarts");
    let snapshot = client.projects().await.unwrap();
    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(snapshot.as_of_seq, receipt.committed_seq);
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn second_daemon_is_refused() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let second = aurelius_daemon::run(root.clone(), None).await;
    assert!(second.unwrap_err().to_string().contains("already running"));
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn retry_after_reconnect_returns_same_receipt() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let env = envelope(create("Atlas"), None);
    let first = {
        let client = connect(&root).await;
        client.execute(env.clone()).await.unwrap()
    }; // connection dropped here
    let client = connect(&root).await;
    let second = client.execute(env).await.unwrap();
    assert!(second.replayed);
    assert_eq!(first.result, second.result);
    assert_eq!(client.projects().await.unwrap().projects.len(), 1);
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn command_applied_tells_a_new_connection_whether_a_key_committed() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let key = CommandId::new();
    {
        let mut first = raw(&root).await;
        let outcome = ask(&mut first, &Request::write(1, key, WriteMethod::Domain(create("Atlas")))).await;
        assert!(matches!(outcome, Outcome::Ok(ResponseBody::Receipt(_))), "the write commits: {outcome:?}");
    } // the link drops here, as if its answer was lost

    let mut second = raw(&root).await;
    let applied = |id, idempotency_key| Request::read(id, Method::CommandApplied { idempotency_key });
    let committed = ask(&mut second, &applied(1, key)).await;
    assert!(matches!(&committed, Outcome::Ok(ResponseBody::CommandApplied(a)) if a.applied), "{committed:?}");
    let unknown = ask(&mut second, &applied(2, CommandId::new())).await;
    assert!(matches!(&unknown, Outcome::Ok(ResponseBody::CommandApplied(a)) if !a.applied), "{unknown:?}");
    assert!(aurelius_protocol::FEATURES.contains(&aurelius_protocol::feature::COMMAND_APPLIED));
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_survives_a_client_disconnect() {
    let (_dir, root) = tempdir();
    let daemon = start_with_delay(&root, 30).await;
    let conversation_id = {
        let client = connect(&root).await;
        let send = SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "hallo".into(), images: Vec::new(), files: vec![], interrupt: false };
        match client.execute(envelope(Command::SendChatMessage(send), None)).await.unwrap().result {
            CommandResult::ChatMessageSent { conversation_id, .. } => conversation_id,
            other => panic!("{other:?}"),
        }
    }; // the "UI" goes away mid-answer

    let (client, mut events) = connect_events(&root).await;
    let mut complete = false;
    for _ in 0..400 {
        let messages = client.messages(conversation_id).await.unwrap().messages;
        if messages.iter().any(|m| m.role == aurelius_domain::conversation::MessageRole::Assistant && m.status == aurelius_domain::conversation::MessageStatus::Complete) {
            complete = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(complete, "the answer finished without a client");

    // Reconnect + replay from cursor 0 shows the whole story, in order.
    client.subscribe(0).await.unwrap();
    let mut seqs = vec![];
    while let Ok(Some(e)) = tokio::time::timeout(Duration::from_millis(300), events.recv()).await {
        seqs.push(e.seq);
    }
    assert!(!seqs.is_empty());
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "{seqs:?}");
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn live_events_follow_replay_without_gaps() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let writer = connect(&root).await;
    let base = writer.health().await.unwrap().last_seq; // bootstrap events
    writer.execute(envelope(create("One"), None)).await.unwrap();

    let (reader, mut events) = connect_events(&root).await;
    let through = reader.subscribe(base).await.unwrap();
    assert!(through > base);
    writer.execute(envelope(create("Two"), None)).await.unwrap();
    writer.execute(envelope(create("Three"), None)).await.unwrap();

    let mut got = vec![];
    while let Ok(Some(e)) = tokio::time::timeout(Duration::from_millis(500), events.recv()).await {
        got.push((e.seq, e.kind));
    }
    let seqs: Vec<u64> = got.iter().map(|(s, _)| *s).collect();
    assert_eq!(seqs[0], base + 1);
    assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1), "no gap, no duplicate: {seqs:?}");
    assert_eq!(got.iter().filter(|(_, k)| *k == EventKind::ProjectCreated).count(), 3);
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn server_restart_answers_then_stops_to_start_again() {
    let (_dir, root) = tempdir();
    let (ready_tx, ready_rx) = oneshot::channel();
    let daemon = tokio::spawn(aurelius_daemon::run_stopped(root.clone(), Some(ready_tx)));
    tokio::time::timeout(Duration::from_secs(10), ready_rx).await.unwrap().unwrap();
    let client = connect(&root).await;
    client.restart_server().await.unwrap();
    let stopped = tokio::time::timeout(Duration::from_secs(10), daemon).await.unwrap().unwrap().unwrap();
    assert_eq!(stopped, aurelius_daemon::Stopped::Restart);

    // The instance lock is free again: the next start (main.rs) succeeds.
    let daemon = start(&root).await;
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_and_oversized_frames_are_refused() {
    use tokio::io::AsyncWriteExt;
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;

    let mut stream = raw(&root).await;
    stream.write_all(&5u32.to_be_bytes()).await.unwrap();
    stream.write_all(b"{oops").await.unwrap();
    let reply: ServerMessage = read_frame(&mut stream).await.unwrap().unwrap();
    assert!(matches!(reply, ServerMessage::Response { outcome: Outcome::Error(ref e), .. } if e.code == ErrorCode::InvalidRequest));

    let mut stream = raw(&root).await;
    stream.write_all(&(64u32 * 1024 * 1024).to_be_bytes()).await.unwrap();
    let reply: ServerMessage = read_frame(&mut stream).await.unwrap().unwrap();
    assert!(matches!(reply, ServerMessage::Response { outcome: Outcome::Error(ref e), .. } if e.code == ErrorCode::FrameTooLarge));

    // The daemon is still healthy for well-behaved clients.
    let mut stream = raw(&root).await;
    assert!(matches!(ask(&mut stream, &Request::read(9, Method::ServerGetHealth)).await, Outcome::Ok(ResponseBody::Health(_))));
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_method_gets_its_own_id_and_keeps_the_connection() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let mut stream = raw(&root).await;

    write_frame(&mut stream, &serde_json::json!({ "protocol": 2, "request_id": 31, "method": "future.method", "params": {} })).await.unwrap();
    let reply: ServerMessage = read_frame(&mut stream).await.unwrap().unwrap();
    assert!(
        matches!(reply, ServerMessage::Response { request_id: 31, outcome: Outcome::Error(ref e) } if e.code == ErrorCode::UnknownMethod),
        "{reply:?}"
    );
    // Malformed JSON no longer drops the connection either.
    use tokio::io::AsyncWriteExt;
    stream.write_all(&5u32.to_be_bytes()).await.unwrap();
    stream.write_all(b"{oops").await.unwrap();
    let reply: ServerMessage = read_frame(&mut stream).await.unwrap().unwrap();
    assert!(matches!(reply, ServerMessage::Response { request_id: 0, outcome: Outcome::Error(ref e) } if e.code == ErrorCode::InvalidRequest));

    // Same connection, next request.
    assert!(matches!(ask(&mut stream, &Request::read(32, Method::ServerGetHealth)).await, Outcome::Ok(ResponseBody::Health(_))));
    stop(&root, daemon).await;
}

// ---- the write pipeline ----

#[tokio::test(flavor = "multi_thread")]
async fn a_write_without_an_idempotency_key_is_refused() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let mut stream = raw(&root).await;

    let mut request = Request::write(1, CommandId::new(), WriteMethod::Domain(create("Atlas")));
    request.idempotency_key = None;
    assert_eq!(error_code(ask(&mut stream, &request).await), ErrorCode::InvalidRequest);
    // Host writes need one too.
    let mut request = Request::write(2, CommandId::new(), WriteMethod::Host(HostWrite::PowerCancel));
    request.idempotency_key = None;
    assert_eq!(error_code(ask(&mut stream, &request).await), ErrorCode::InvalidRequest);

    let client = connect(&root).await;
    assert!(client.projects().await.unwrap().projects.is_empty(), "nothing was done");
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_key_replays_and_another_command_with_it_conflicts() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let client = connect(&root).await;
    let key = CommandId::new();

    let first = client.write(key, None, WriteMethod::Domain(create("Atlas"))).await.unwrap();
    assert!(!first.replayed);
    let second = client.write(key, None, WriteMethod::Domain(create("Atlas"))).await.unwrap();
    assert!(second.replayed);
    assert_eq!((first.committed_seq, &first.result, first.idempotency_key), (second.committed_seq, &second.result, second.idempotency_key));
    assert_eq!(client.projects().await.unwrap().projects.len(), 1, "no second project");

    // The same key for another command is a conflict, and does nothing.
    match client.write(key, None, WriteMethod::Domain(create("Phoenix"))).await {
        Err(aurelius_client::ClientError::Protocol(err)) => assert_eq!(err.code, ErrorCode::IdempotencyConflict),
        other => panic!("{other:?}"),
    }
    assert_eq!(client.projects().await.unwrap().projects.len(), 1);
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_write_replays_from_its_receipt() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let client = connect(&root).await;

    let key = CommandId::new();
    let make = || WriteMethod::Host(HostWrite::TraceSetEnabled { enabled: true });
    let first = client.write(key, None, make()).await.unwrap();
    assert!(matches!(first.result, WriteResult::Host(HostResult::TraceSetting(_))), "{first:?}");
    assert!(!first.replayed);

    let second = client.write(key, None, make()).await.unwrap();
    assert!(second.replayed);
    assert_eq!(second.committed_seq, first.committed_seq);
    assert_eq!(second.result, first.result, "the same answer from the receipt");
    stop(&root, daemon).await;
}

// ---- the handshake ----

#[tokio::test(flavor = "multi_thread")]
async fn another_protocol_major_is_refused_with_update_needed_but_protocol_5_works() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let mut stream = raw(&root).await;

    // An old client (3.x, and the Fase 1/2 client of protocol 4) and a newer one are told to update.
    for (id, protocol, update) in [(1, 3, "Update the app"), (2, 4, "Update the app"), (3, 6, "Update the server")] {
        match ask(&mut stream, &hello(id, protocol)).await {
            Outcome::Error(err) => {
                assert_eq!(err.code, ErrorCode::IncompatibleProtocol, "hello with protocol {protocol}");
                assert!(err.message.starts_with("Update needed") && err.message.contains(update), "{}", err.message);
            }
            other => panic!("expected an error, got {other:?}"),
        }
        let mut request = Request::read(id + 10, Method::ProjectList);
        request.protocol = protocol;
        assert_eq!(error_code(ask(&mut stream, &request).await), ErrorCode::IncompatibleProtocol, "project.list with protocol {protocol}");
        let mut request = Request::write(id + 20, CommandId::new(), WriteMethod::Domain(create("Atlas")));
        request.protocol = protocol;
        assert_eq!(error_code(ask(&mut stream, &request).await), ErrorCode::IncompatibleProtocol, "a write with protocol {protocol}");
    }

    // The same connection works with the right major.
    match ask(&mut stream, &hello(9, 5)).await {
        Outcome::Ok(ResponseBody::Hello(h)) => assert_eq!(h.protocol, 5),
        other => panic!("{other:?}"),
    }
    let client = connect(&root).await;
    assert!(client.projects().await.unwrap().projects.is_empty(), "the refused write did nothing");
    stop(&root, daemon).await;
}

// ---- the bridge endpoint ----

#[tokio::test(flavor = "multi_thread")]
async fn bridge_methods_are_not_accepted_on_the_client_endpoint() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let mut stream = raw(&root).await;

    write_frame(&mut stream, &serde_json::json!({ "protocol": 2, "request_id": 1, "method": "daemon.shutdown" })).await.unwrap();
    match read_frame::<ServerMessage, _>(&mut stream).await {
        Ok(Some(ServerMessage::Response { request_id, outcome: Outcome::Error(err) })) => assert_eq!((request_id, err.code), (1, ErrorCode::UnknownMethod)),
        Ok(None) | Err(_) => {} // closed without an answer is as good
        other => panic!("{other:?}"),
    }

    // The daemon did not stop.
    let client = connect(&root).await;
    assert!(client.projects().await.unwrap().projects.is_empty());
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn permission_ask_on_the_bridge_endpoint_denies_a_run_that_does_not_exist() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let bridge = BridgeClient::connect(&root).await.unwrap();
    let answer = tokio::time::timeout(Duration::from_secs(5), bridge.permission_ask(RunId::new(), None, "Bash".into(), serde_json::json!({ "command": "dir" })))
        .await
        .unwrap()
        .unwrap();
    assert!(!answer.allow, "{answer:?}");
    assert!(answer.message.is_some());
    stop(&root, daemon).await;
}

// ---- typed events ----

#[tokio::test(flavor = "multi_thread")]
async fn creating_a_project_delivers_a_typed_event_and_replay_returns_it() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let (subscriber, mut events) = connect_events(&root).await;
    let base = subscriber.health().await.unwrap().last_seq;
    subscriber.subscribe(base).await.unwrap();

    let writer = connect(&root).await;
    let CommandResult::ProjectCreated { project_id, .. } = writer.execute(envelope(create("Atlas"), None)).await.unwrap().result else { panic!() };

    let created = loop {
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv()).await.expect("an event in time").expect("events stay open");
        if event.kind == EventKind::ProjectCreated {
            break event;
        }
    };
    assert_eq!(created.aggregate_id, project_id.to_string());
    assert!(created.seq > base);

    // Replay from before it returns it first; from its own seq it does not return it again.
    let (late, mut replay) = connect_events(&root).await;
    late.subscribe(created.seq - 1).await.unwrap();
    let first = tokio::time::timeout(Duration::from_secs(5), replay.recv()).await.unwrap().unwrap();
    assert_eq!((first.seq, first.kind, first.aggregate_id.as_str()), (created.seq, EventKind::ProjectCreated, created.aggregate_id.as_str()));

    let (after, mut nothing) = connect_events(&root).await;
    let through = after.subscribe(created.seq).await.unwrap();
    assert!(through >= created.seq);
    while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(300), nothing.recv()).await {
        assert!(event.seq > created.seq, "{event:?}");
    }
    stop(&root, daemon).await;
}

/// Subscribe after `after_seq`: the kinds of the replayed events, then of the
/// live ones until the stream is quiet for half a second.
async fn subscribe_kinds(stream: &mut BoxStream, after_seq: u64) -> Vec<EventKind> {
    write_frame(stream, &Request::read(1, Method::EventSubscribe { after_seq })).await.unwrap();
    let mut got = Vec::new();
    loop {
        match read_frame::<ServerMessage, _>(stream).await.expect("a frame this client can read").expect("the connection stays open") {
            ServerMessage::Event(e) => got.push(e.kind),
            ServerMessage::Response { request_id: 1, outcome } => {
                assert!(matches!(outcome, Outcome::Ok(ResponseBody::Subscribed { .. })), "{outcome:?}");
                break;
            }
            _ => {}
        }
    }
    got.extend(kinds(stream).await);
    got
}

/// The kinds of the events on `stream` until it is quiet for half a second.
async fn kinds(stream: &mut BoxStream) -> Vec<EventKind> {
    let mut got = Vec::new();
    while let Ok(frame) = tokio::time::timeout(Duration::from_millis(500), read_frame::<ServerMessage, _>(stream)).await {
        if let ServerMessage::Event(e) = frame.expect("a frame this client can read").expect("the connection stays open") {
            got.push(e.kind);
        }
    }
    got
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_that_does_not_accept_server_name_never_gets_server_renamed() {
    let (_dir, root) = tempdir();
    let daemon = start(&root).await;
    let writer = connect(&root).await;
    let base = writer.health().await.unwrap().last_seq;
    let rename = |name: &str| Command::SetServerName(aurelius_domain::server::SetServerName { name: name.into() });
    writer.execute(envelope(create("Before"), None)).await.unwrap();
    writer.execute(envelope(rename("Kantoor"), None)).await.unwrap();

    // An older client: no `server_name` in its hello's accepts (replay, then live).
    let mut old = raw(&root).await;
    let replayed = subscribe_kinds(&mut old, base).await;
    assert!(replayed.contains(&EventKind::ProjectCreated) && !replayed.contains(&EventKind::ServerRenamed), "{replayed:?}");
    writer.execute(envelope(rename("Thuis"), None)).await.unwrap();
    writer.execute(envelope(create("After"), None)).await.unwrap();
    let seen = kinds(&mut old).await;
    assert!(!seen.contains(&EventKind::ServerRenamed), "{seen:?}");
    assert!(seen.contains(&EventKind::ProjectCreated), "it keeps getting the others: {seen:?}");
    assert!(matches!(ask(&mut old, &Request::read(2, Method::ServerGetHealth)).await, Outcome::Ok(ResponseBody::Health(_))), "and keeps working");

    // A client that accepts it gets both changes.
    let token = common::sign_in(&root, &common::connect_anonymous(&root).await, common::BOARD_MEMBER).await.session_token;
    let mut new = ipc::connect(&aurelius_host::endpoint(&root)).await.unwrap();
    let accepts = vec![aurelius_protocol::client_feature::SERVER_NAME.to_owned()];
    let hello = Request::read(0, Method::SessionHello(HelloParams { client_name: "raw".into(), client_version: "0".into(), accepts, session_token: Some(token) }));
    assert!(matches!(ask(&mut new, &hello).await, Outcome::Ok(ResponseBody::Hello(h)) if h.host_name == "Thuis"));
    let seen = subscribe_kinds(&mut new, base).await;
    assert_eq!(seen.iter().filter(|k| **k == EventKind::ServerRenamed).count(), 2, "{seen:?}");
    stop(&root, daemon).await;
}
