//! The protocol freeze of phase A (AUR-FA-N1, F3): `message.locate`, `message.edit`,
//! `message.delete`, `message.history` and `ticket.my_tickets` work through the API,
//! and a connection that does not accept the new events never gets them, in the replay either.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::{Client, ClientError};
use aurelius_domain::group_chat::{DeleteMessage, EditMessage, SendGroupMessage};
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{CreateTicket, FollowTicket, PostTicketMessage, TicketPriority};
use aurelius_domain::{ActorKind, Command, CommandId, CommandResult, ConversationId, ErrorCode, EventKind, MessageId, ProjectId, TicketId, UserId};
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
        from_idea: None,
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

async fn edit(client: &Client, message: MessageId, text: &str) -> Result<CommandResult, ClientError> {
    client.execute(aurelius_client::envelope(Command::EditMessage(EditMessage { message_id: message, text: text.into(), confirm_secret: false }), None)).await.map(|c| c.result)
}

async fn delete(client: &Client, message: MessageId) -> Result<CommandResult, ClientError> {
    client.execute(aurelius_client::envelope(Command::DeleteMessage(DeleteMessage { message_id: message }), None)).await.map(|c| c.result)
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_and_a_delete_show_in_the_read_and_history_and_wake_nobody() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (_, channel) = project_channel(&client, "Atlas").await;
    let message = post(&client, channel, "Hello", None).await;
    let mut stream = raw(dir.path(), &[client_feature::MESSAGE_EDITS]).await;
    let runs = || db(dir.path()).query_row("SELECT COUNT(*) FROM runs", [], |r| r.get::<_, i64>(0)).unwrap();
    let rows = || db(dir.path()).query_row("SELECT (SELECT COUNT(*) FROM notifications) + (SELECT COUNT(*) FROM messages)", [], |r| r.get::<_, i64>(0)).unwrap();
    let (runs_before, rows_before) = (runs(), rows());
    let as_read = |snapshot: aurelius_protocol::MessagesSnapshot| snapshot.messages.into_iter().find(|m| m.id == message).unwrap();

    assert!(as_read(client.channel_messages(channel).await.unwrap()).editable, "the author may");
    assert!(matches!(edit(&client, message, "Hello @alex").await.unwrap(), CommandResult::MessageEdited { .. }));
    let read = as_read(client.channel_messages(channel).await.unwrap());
    assert_eq!((read.text.as_str(), read.edited_at_ms.is_some(), read.editable), ("Hello @alex", true, true));
    match read_outcome(&mut stream, Method::MessageHistory { message_id: message }).await {
        Outcome::Ok(ResponseBody::MessageHistory(history)) => assert_eq!((history.deleted, history.versions.iter().map(|v| v.text.as_str()).collect::<Vec<_>>()), (false, vec!["Hello"])),
        other => panic!("{other:?}"),
    }

    assert!(matches!(delete(&client, message).await.unwrap(), CommandResult::MessageDeleted { .. }));
    let read = as_read(client.channel_messages(channel).await.unwrap());
    assert_eq!((read.text.as_str(), read.deleted_at_ms.is_some(), read.editable), ("This message was deleted.", true, false));
    match read_outcome(&mut stream, Method::MessageHistory { message_id: message }).await {
        Outcome::Ok(ResponseBody::MessageHistory(history)) => assert!(history.deleted && history.versions.is_empty(), "{history:?}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(code(edit(&client, message, "Back").await.unwrap_err()), ErrorCode::NotFound);
    assert_eq!((runs(), rows()), (runs_before, rows_before), "no run, no notification, no message");
    assert_eq!(code(client.message_locate(MessageId::new()).await.unwrap_err()), ErrorCode::NotFound);
    match read_outcome(&mut stream, Method::MessageHistory { message_id: MessageId::new() }).await {
        Outcome::Error(err) => assert_eq!(err.code, ErrorCode::NotFound),
        other => panic!("{other:?}"),
    }

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_history_goes_through_the_vault_redactor_when_it_goes_out() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (_, channel) = project_channel(&client, "Atlas").await;
    let value = format!("ghp_{}", CommandId::new().0.simple());
    let message = post(&client, channel, &format!("The token is {value}"), None).await;
    edit(&client, message, "No token any more").await.unwrap();
    let mut stream = raw(dir.path(), &[client_feature::MESSAGE_EDITS]).await;
    let owner = format!("history-test-{}", CommandId::new());
    let secret = aurelius_runtime::redact::Secret { name: "GH_TOKEN".into(), label: "GitHub token".into(), value: zeroize::Zeroizing::new(value.clone()) };
    aurelius_runtime::redact::install_set(&owner, vec![secret]);
    let history = read_outcome(&mut stream, Method::MessageHistory { message_id: message }).await;
    aurelius_runtime::redact::install_set(&owner, Vec::new());
    let Outcome::Ok(ResponseBody::MessageHistory(history)) = history else { panic!("{history:?}") };
    let text = &history.versions[0].text;
    assert!(!text.contains(&value) && text.contains("«secret:GH_TOKEN»"), "{text}");

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_real_edit_reaches_only_a_connection_that_accepts_message_edits() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (_, channel) = project_channel(&client, "Atlas").await;
    let message = post(&client, channel, "Hello", None).await;
    edit(&client, message, "Hello again").await.unwrap();
    delete(&client, message).await.unwrap();
    let count = |kinds: &[EventKind], kind| kinds.iter().filter(|k| **k == kind).count();
    let older = replayed_kinds(&mut raw(dir.path(), &[client_feature::TEAM_CHANNELS]).await).await;
    assert_eq!((count(&older, EventKind::MessageEdited), count(&older, EventKind::MessageDeleted)), (0, 1), "{older:?}");
    let newer = replayed_kinds(&mut raw(dir.path(), &[client_feature::TEAM_CHANNELS, client_feature::MESSAGE_EDITS]).await).await;
    assert_eq!((count(&newer, EventKind::MessageEdited), count(&newer, EventKind::MessageDeleted)), (1, 1), "{newer:?}");

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_write_without_a_session_is_refused() {
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
async fn my_tickets_are_those_of_the_reader_only() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (project, _) = project_channel(&client, "Atlas").await;
    let (mine, _) = ticket(&client, project).await;
    let other = connect_as(dir.path(), "dana").await;
    let (theirs, _) = ticket(&other, project).await;
    let tickets = |outcome: Outcome| match outcome {
        Outcome::Ok(ResponseBody::MyTickets(view)) => (view.tickets.iter().map(|t| t.id).collect::<Vec<_>>(), view.following),
        other => panic!("{other:?}"),
    };
    let mut stream = raw(dir.path(), &[]).await;
    let (listed, following) = tickets(read_outcome(&mut stream, Method::TicketMyTickets { project_id: None, include_done: false }).await);
    assert!(listed.contains(&mine) && following.contains(&mine));
    assert!(listed.contains(&theirs), "a ticket without a responsible is for everyone");
    assert!(!following.contains(&theirs), "following is the reader's own");

    // Someone made responsible for one is no longer everyone's.
    let dana = other.user_whoami().await.unwrap().user.unwrap().id;
    other.execute(aurelius_client::envelope(Command::SetTicketResponsible(aurelius_domain::ticket::SetTicketResponsible { ticket_id: theirs, responsible_user_id: Some(dana) }), None)).await.unwrap();
    let (listed, _) = tickets(read_outcome(&mut stream, Method::TicketMyTickets { project_id: None, include_done: false }).await);
    assert!(!listed.contains(&theirs));

    drop((client, other));
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

/// Put an event in the log as the server part will: nothing writes `message.edited` yet.
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
    // Its creator's follow is the real `ticket.follow_changed` (A2).
    ticket(&client, project).await;
    let message = post(&client, channel, "Hello", None).await;
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
