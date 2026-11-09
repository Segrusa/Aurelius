//! The Team chat's channels (feature `team_channels`): `channel.list`,
//! `group_chat.get { channel }`, and live events of a project's channel only for
//! the connections that say they show channels (`accepts: team_channels`).

mod common;

use aurelius_client::envelope;
use aurelius_domain::{Command, ConversationId, group_chat::SendGroupMessage, project::CreateProject};
use aurelius_host::ipc::{self, BoxStream, read_frame, write_frame};
use aurelius_protocol::{CommandResult, HelloParams, Method, Outcome, Request, ResponseBody, ServerMessage, client_feature};
use common::*;
use std::time::Duration;

async fn raw(root: &std::path::Path, accepts: Vec<String>) -> BoxStream {
    let mut stream = ipc::connect(&aurelius_host::endpoint(root)).await.unwrap();
    // Signed in: a board member's session (the helper's), its token in the hello.
    let token = sign_in(root, &connect_anonymous(root).await, BOARD_MEMBER).await.session_token;
    let hello = Request::read(1, Method::SessionHello(HelloParams { client_name: "raw".into(), client_version: "0".into(), accepts, session_token: Some(token) }));
    write_frame(&mut stream, &hello).await.unwrap();
    loop {
        if let ServerMessage::Response { request_id: 1, .. } = read_frame::<ServerMessage, _>(&mut stream).await.unwrap().unwrap() {
            return stream;
        }
    }
}

/// Subscribe from the start and collect the aggregates of every event for a while.
async fn aggregates(stream: &mut BoxStream, then: impl std::future::Future<Output = ()>) -> Vec<String> {
    write_frame(stream, &Request::read(2, Method::EventSubscribe { after_seq: 0 })).await.unwrap();
    then.await;
    let mut seen = Vec::new();
    while let Ok(Ok(Some(message))) = tokio::time::timeout(Duration::from_millis(400), read_frame::<ServerMessage, _>(stream)).await {
        if let ServerMessage::Event(event) = message {
            seen.push(event.aggregate_id);
        }
    }
    seen
}

async fn send(client: &aurelius_client::Client, channel: Option<ConversationId>, text: &str) {
    let send = SendGroupMessage { confirm_secret: false, text: text.into(), images: vec![], files: vec![], channel, thread_parent: None };
    let result = client.execute(envelope(Command::SendGroupMessage(send), None)).await.unwrap().result;
    assert!(matches!(result, CommandResult::GroupMessageSent { .. }), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_projects_channel_reaches_only_clients_that_show_channels() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    client.execute(envelope(Command::CreateProject(CreateProject { name: "Orbit".into(), description: None, folder: None }), None)).await.unwrap();

    // channel.list: General first, then the project's.
    let mut new = raw(dir.path(), vec![client_feature::TEAM_CHANNELS.into()]).await;
    write_frame(&mut new, &Request::read(3, Method::ChannelList)).await.unwrap();
    let channels = loop {
        if let ServerMessage::Response { request_id: 3, outcome } = read_frame::<ServerMessage, _>(&mut new).await.unwrap().unwrap() {
            let Outcome::Ok(ResponseBody::Channels(snapshot)) = outcome else { panic!("{outcome:?}") };
            break snapshot.channels;
        }
    };
    let names: Vec<_> = channels.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["General", "Orbit"]);
    let (general, orbit) = (channels[0].conversation_id, channels[1].conversation_id);
    assert!(channels[0].project_id.is_none() && channels[1].project_id.is_some());

    send(&client, None, "In general").await;
    send(&client, Some(orbit), "In Orbit").await;
    // group_chat.get { channel }: that channel; without: the general one.
    assert!(client.group_chat().await.unwrap().messages.iter().any(|m| m.text == "In general"));

    // Replayed and live: the old client gets the general channel, never Orbit's.
    let mut old = raw(dir.path(), vec![]).await;
    let old_seen = aggregates(&mut old, send(&client, Some(orbit), "Live in Orbit")).await;
    assert!(old_seen.contains(&general.to_string()), "{old_seen:?}");
    assert!(!old_seen.contains(&orbit.to_string()), "{old_seen:?}");
    let new_seen = aggregates(&mut new, async {}).await;
    assert!(new_seen.contains(&orbit.to_string()) && new_seen.contains(&general.to_string()), "{new_seen:?}");

    stop(dir.path(), daemon).await;
}

/// The events a `Client` got for a while, by aggregate.
async fn received(events: &mut tokio::sync::mpsc::UnboundedReceiver<aurelius_protocol::Event>) -> Vec<String> {
    let mut seen = Vec::new();
    while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(400), events.recv()).await {
        seen.push(event.aggregate_id);
    }
    seen
}

#[tokio::test(flavor = "multi_thread")]
async fn client_open_names_team_channels_in_its_hello_and_again_when_it_identifies() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    client.execute(envelope(Command::CreateProject(CreateProject { name: "Orbit".into(), description: None, folder: None }), None)).await.unwrap();
    let target = aurelius_client::Target::Local { root: dir.path().to_path_buf(), spawn: false };
    let (tx, mut shows) = tokio::sync::mpsc::unbounded_channel();
    let channels = aurelius_client::Client::open(&target, "test", tx, None, &[client_feature::TEAM_CHANNELS]).await.unwrap();
    let (tx, mut plain) = tokio::sync::mpsc::unbounded_channel();
    let older = aurelius_client::Client::open(&target, "test", tx, None, &[]).await.unwrap();
    let token = sign_in(dir.path(), &channels, BOARD_MEMBER).await.session_token;
    let older_token = sign_in(dir.path(), &older, BOARD_MEMBER).await.session_token;
    let orbit = channels.channels().await.unwrap().channels.into_iter().find(|c| c.project_id.is_some()).unwrap().conversation_id;
    // A second hello (the session this device kept) keeps the client's features.
    channels.resume(&token).await.unwrap();
    older.resume(&older_token).await.unwrap();
    channels.subscribe(0).await.unwrap();
    older.subscribe(0).await.unwrap();
    send(&client, Some(orbit), "Live in Orbit").await;
    assert!(received(&mut shows).await.contains(&orbit.to_string()));
    assert!(!received(&mut plain).await.contains(&orbit.to_string()));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reading_a_channel_marks_it_read_for_that_board_member_only() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect_as(dir.path(), "alex").await;
    let robin = connect_as(dir.path(), "robin").await;
    alex.execute(envelope(Command::CreateProject(CreateProject { name: "Orbit".into(), description: None, folder: None }), None)).await.unwrap();
    let orbit = |c: aurelius_domain::views::ChannelsSnapshot| c.channels.into_iter().find(|c| c.project_id.is_some()).unwrap();
    let id = orbit(alex.channels().await.unwrap()).conversation_id;

    send(&alex, Some(id), "Hello @Robin, a question").await;
    send(&alex, Some(id), "And @robins is someone else").await;
    let seen = orbit(robin.channels().await.unwrap());
    // Alex's two messages, plus whatever Aurelius or an employee wrote back; one names Robin.
    assert!(seen.unread >= 2 && seen.mentions == 1, "{seen:?}");
    assert_eq!((orbit(alex.channels().await.unwrap()).unread, orbit(alex.channels().await.unwrap()).mentions), (0, 0), "your own messages are read");
    robin.channel_messages(id).await.unwrap();
    let after = orbit(robin.channels().await.unwrap());
    assert_eq!((after.unread, after.mentions), (0, 0), "reading it marks it read");
    send(&alex, Some(id), "One more").await;
    assert!(orbit(robin.channels().await.unwrap()).unread >= 1, "new messages are unread again");
    // Without a board member nothing is counted, and reading marks nothing.
    let nobody = connect(dir.path()).await;
    assert_eq!(orbit(nobody.channels().await.unwrap()).unread, 0);
    stop(dir.path(), daemon).await;
}
