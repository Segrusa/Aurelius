//! The protocol freeze of phase A (AUR-FA-N1): `message.locate` works, the methods
//! whose server part is still to come refuse cleanly and leave nothing, and a
//! connection that does not accept the new events never gets them, in the replay either.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::{Client, ClientError};
use aurelius_domain::group_chat::{DeleteMessage, EditMessage, SendGroupMessage};
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{CreateTicket, FollowTicket, PostTicketMessage, SetTicketResponsible, TicketPriority};
use aurelius_domain::{ActorKind, Command, CommandEnvelope, CommandId, CommandResult, ConversationId, ErrorCode, EventKind, MessageId, ProjectId, TicketId, UserId};
use aurelius_host::ipc::{self, BoxStream, read_frame, write_frame};
use aurelius_protocol::{HelloParams, Method, Outcome, Request, ResponseBody, ServerMessage, client_feature, feature};
use aurelius_storage::Store;
use aurelius_storage::events::{NewEvent, append};
use std::path::Path;
use std::time::Duration;

async fn project_channel(client: &Client, name: &str) -> (ProjectId, ConversationId) {
    let created = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = created.result else { panic!("{:?}", created.result) };
    let channel = client.channels().await.unwrap().channels.into_iter().find(|c| c.project_id == Some(project_id)).expect("its channel").conversation_id;
    (project_id, channel)
}

async fn post(client: &Client, channel: ConversationId, text: &str, thread_parent: Option<MessageId>) -> MessageId {
    let send = SendGroupMessage { confirm_secret: false, text: text.into(), images: vec![], files: vec![], channel: Some(channel), thread_parent };
    match client.execute(aurelius_client::envelope(Command::SendGroupMessage(send), None)).await.unwrap().result {
        CommandResult::GroupMessageSent { message_id, .. } => message_id,
        other => panic!("{other:?}"),
    }
}

fn new_ticket(project_id: ProjectId, responsible_user_id: Option<UserId>) -> CreateTicket {
    CreateTicket {
        confirm_secret: false,
        project_id,
        title: "Login".into(),
        description: None,
        priority: TicketPriority::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: None,
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: None,
        responsible_user_id,
    }
}

async fn ticket(client: &Client, project_id: ProjectId) -> (TicketId, u32) {
    let CommandResult::TicketCreated { ticket_id, number } = client.create_ticket(new_ticket(project_id, None)).await.unwrap().result else { panic!("not created") };
    (ticket_id, number)
}

async fn post_on_ticket(client: &Client, ticket_id: TicketId, text: &str, thread_parent: Option<MessageId>) -> (ConversationId, MessageId) {
    let post = PostTicketMessage { ticket_id, text: text.into(), images: vec![], files: vec![], confirm_secret: false, thread_parent };
    match client.execute(aurelius_client::envelope(Command::PostTicketMessage(post), None)).await.unwrap().result {
        CommandResult::TicketMessagePosted { conversation_id, message_id, .. } => (conversation_id, message_id),
        other => panic!("{other:?}"),
    }
}

fn code(err: ClientError) -> ErrorCode {
    match err {
        ClientError::Protocol(err) => err.code,
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn message_locate_finds_a_message_of_a_channel_with_its_thread() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    assert!(client.supports(feature::MESSAGE_LOCATE), "the feature is advertised");
    let (_, channel) = project_channel(&client, "Atlas").await;
    let root = post(&client, channel, "Release on Friday?", None).await;
    let reply = post(&client, channel, "Yes", Some(root)).await;

    let found = client.message_locate(root).await.unwrap();
    assert_eq!((found.message_id, found.conversation_id, found.ticket_id, found.ticket_number, found.thread_root), (root, channel, None, None, None));
    let found = client.message_locate(reply).await.unwrap();
    assert_eq!((found.message_id, found.conversation_id, found.ticket_id, found.thread_root), (reply, channel, None, Some(root)));

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn message_locate_names_the_ticket_of_a_discussion_and_its_thread() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (project, _) = project_channel(&client, "Atlas").await;
    let (ticket_id, number) = ticket(&client, project).await;
    let (discussion, root) = post_on_ticket(&client, ticket_id, "Which login?", None).await;
    let (_, reply) = post_on_ticket(&client, ticket_id, "The new one", Some(root)).await;

    let found = client.message_locate(root).await.unwrap();
    assert_eq!((found.conversation_id, found.ticket_id, found.ticket_number, found.thread_root), (discussion, Some(ticket_id), Some(number), None));
    let found = client.message_locate(reply).await.unwrap();
    assert_eq!((found.conversation_id, found.ticket_id, found.ticket_number, found.thread_root), (discussion, Some(ticket_id), Some(number), Some(root)));

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn message_locate_answers_not_found_outside_a_channel_or_a_discussion() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    // A project conversation: it exists, and answers just like an unknown message.
    let (private, message, _) = send_new(&client, "Only for the assistant").await;
    assert_eq!(code(client.message_locate(message).await.unwrap_err()), ErrorCode::NotFound, "in {private}");
    assert_eq!(code(client.message_locate(MessageId::new()).await.unwrap_err()), ErrorCode::NotFound);

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stub_write_refuses_with_validation_and_leaves_no_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (project, channel) = project_channel(&client, "Atlas").await;
    let (ticket_id, _) = ticket(&client, project).await;
    let message = post(&client, channel, "Hello", None).await;
    let events = || db(dir.path()).query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0)).unwrap();
    let before = events();

    for command in [
        Command::SetTicketResponsible(SetTicketResponsible { ticket_id, responsible_user_id: None }),
        Command::FollowTicket(FollowTicket { ticket_id, on: true }),
        Command::EditMessage(EditMessage { message_id: message, text: "Better".into(), confirm_secret: false }),
        Command::DeleteMessage(DeleteMessage { message_id: message }),
        Command::CreateTicket(new_ticket(project, Some(UserId::new()))),
    ] {
        let key = CommandId::new();
        let refused = client.execute(CommandEnvelope { command_id: key, expected_version: None, command: command.clone() }).await.unwrap_err();
        assert_eq!(code(refused), ErrorCode::Validation, "{}", command.method());
        let receipts: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM command_receipts WHERE command_id = ?1", [key.to_string()], |r| r.get(0)).unwrap();
        assert_eq!(receipts, 0, "{}", command.method());
    }
    assert_eq!(events(), before, "a refusal writes no event");

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_write_without_a_session_is_refused_before_it_reaches_the_stub() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let anonymous = connect_anonymous(dir.path()).await;
    let command = Command::FollowTicket(FollowTicket { ticket_id: TicketId::new(), on: true });
    assert_eq!(code(anonymous.execute(aurelius_client::envelope(command, None)).await.unwrap_err()), ErrorCode::Unauthenticated);

    drop(client);
    stop(dir.path(), daemon).await;
}

/// A raw connection signed in as `BOARD_MEMBER`, whose hello accepts `accepts`.
async fn raw(root: &Path, accepts: &[&str]) -> BoxStream {
    let mut stream = ipc::connect(&aurelius_host::endpoint(root)).await.unwrap();
    let token = sign_in(root, &connect_anonymous(root).await, BOARD_MEMBER).await.session_token;
    let accepts = accepts.iter().map(|a| (*a).to_owned()).collect();
    let hello = Request::read(1, Method::SessionHello(HelloParams { client_name: "raw".into(), client_version: "0".into(), app_key: None, accepts, session_token: Some(token) }));
    write_frame(&mut stream, &hello).await.unwrap();
    loop {
        if let ServerMessage::Response { request_id: 1, .. } = read_frame::<ServerMessage, _>(&mut stream).await.unwrap().unwrap() {
            return stream;
        }
    }
}

async fn read_outcome(stream: &mut BoxStream, method: Method) -> Outcome {
    write_frame(stream, &Request::read(40, method)).await.unwrap();
    loop {
        if let ServerMessage::Response { request_id: 40, outcome } = read_frame::<ServerMessage, _>(stream).await.unwrap().unwrap() {
            return outcome;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stub_read_refuses_with_validation() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let mut stream = raw(dir.path(), &[]).await;
    for method in [Method::TicketFollowers { ticket_id: TicketId::new() }, Method::TicketMyTickets { project_id: None, include_done: false }, Method::MessageHistory { message_id: MessageId::new() }] {
        match read_outcome(&mut stream, method.clone()).await {
            Outcome::Error(err) => assert_eq!(err.code, ErrorCode::Validation, "{method:?}"),
            Outcome::Ok(body) => panic!("{method:?}: {body:?}"),
        }
    }
    assert!(matches!(read_outcome(&mut stream, Method::UserWhoami).await, Outcome::Ok(ResponseBody::WhoAmI(_))), "the connection stays usable");

    drop(client);
    stop(dir.path(), daemon).await;
}

/// The kinds of every event this connection gets after subscribing from the start.
async fn replayed_kinds(stream: &mut BoxStream) -> Vec<EventKind> {
    write_frame(stream, &Request::read(2, Method::EventSubscribe { after_seq: 0 })).await.unwrap();
    let mut kinds = Vec::new();
    while let Ok(Ok(Some(message))) = tokio::time::timeout(Duration::from_millis(400), read_frame::<ServerMessage, _>(stream)).await {
        if let ServerMessage::Event(event) = message {
            kinds.push(event.kind);
        }
    }
    kinds
}

/// Put an event in the log as the server part will: nothing writes these kinds yet.
fn log_event(root: &Path, kind: EventKind, aggregate_type: &'static str, aggregate_id: String, metadata: serde_json::Value) {
    let store = Store::open(root).unwrap();
    store
        .write(move |tx| {
            let version: u64 = tx.query_row("SELECT COALESCE(MAX(aggregate_version), 0) + 1 FROM events WHERE aggregate_type = ?1 AND aggregate_id = ?2", [aggregate_type, aggregate_id.as_str()], |r| r.get::<_, i64>(0))? as u64;
            append(
                tx,
                NewEvent {
                    aggregate_type,
                    aggregate_id: aggregate_id.clone(),
                    aggregate_version: version,
                    event_type: kind.as_str(),
                    project_id: None,
                    actor_kind: ActorKind::User,
                    actor_id: None,
                    correlation_id: CommandId::new().to_string(),
                    causation_id: None,
                    payload_blob_id: None,
                    metadata,
                    dedupe_key: format!("test:{aggregate_id}:{}", kind.as_str()),
                },
            )?;
            Ok(())
        })
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_without_accepts_gets_neither_new_event_in_the_replay() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (project, channel) = project_channel(&client, "Atlas").await;
    let (ticket_id, _) = ticket(&client, project).await;
    let message = post(&client, channel, "Hello", None).await;
    log_event(dir.path(), EventKind::TicketFollowChanged, "ticket", ticket_id.to_string(), serde_json::json!({ "on": true }));
    log_event(dir.path(), EventKind::MessageEdited, "conversation", channel.to_string(), serde_json::json!({ "message_id": message }));

    let count = |kinds: &[EventKind], kind| kinds.iter().filter(|k| **k == kind).count();
    let kinds = replayed_kinds(&mut raw(dir.path(), &[client_feature::TEAM_CHANNELS]).await).await;
    assert_eq!((count(&kinds, EventKind::TicketFollowChanged), count(&kinds, EventKind::MessageEdited)), (0, 0), "{kinds:?}");
    assert!(kinds.contains(&EventKind::MessageUserCommitted), "the rest of the log comes: {kinds:?}");

    let kinds = replayed_kinds(&mut raw(dir.path(), &[client_feature::TEAM_CHANNELS, client_feature::TICKET_FOLLOW]).await).await;
    assert_eq!((count(&kinds, EventKind::TicketFollowChanged), count(&kinds, EventKind::MessageEdited)), (1, 0), "{kinds:?}");

    let kinds = replayed_kinds(&mut raw(dir.path(), &[client_feature::TEAM_CHANNELS, client_feature::MESSAGE_EDITS]).await).await;
    assert_eq!((count(&kinds, EventKind::TicketFollowChanged), count(&kinds, EventKind::MessageEdited)), (0, 1), "{kinds:?}");

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_deleted_that_was_not_queued_reaches_an_older_client_and_breaks_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (_, channel) = project_channel(&client, "Atlas").await;
    let message = post(&client, channel, "Hello", None).await;
    log_event(dir.path(), EventKind::MessageDeleted, "conversation", channel.to_string(), serde_json::json!({ "message_id": message, "by": UserId::new(), "queued": false }));

    let mut older = raw(dir.path(), &[client_feature::TEAM_CHANNELS]).await;
    let kinds = replayed_kinds(&mut older).await;
    assert_eq!(kinds.iter().filter(|k| **k == EventKind::MessageDeleted).count(), 1, "it is a kind an older client knows: {kinds:?}");
    // What its handler does next: read the conversation again. The message is still there.
    match read_outcome(&mut older, Method::GroupChatGet { channel: Some(channel) }).await {
        Outcome::Ok(ResponseBody::Messages(snapshot)) => assert!(snapshot.messages.iter().any(|m| m.id == message)),
        other => panic!("{other:?}"),
    }

    drop(client);
    stop(dir.path(), daemon).await;
}
