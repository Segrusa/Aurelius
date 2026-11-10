//! Threads and reactions in channels (features `chat_threads` and
//! `chat_reactions`, AUR-CHAT-002), through the daemon's API: a reply is counted
//! on its root and read with `thread.read`, an employee's answer lands in the
//! thread, a reaction is once per board member and emoji, and a connection
//! without the client flags reads its channels as before and never gets
//! `message.reacted` or a new error code.

mod common;

use aurelius_client::Client;
use aurelius_domain::employee::{CreateEmployee, LifecycleStatus, SetProjectStatus};
use aurelius_domain::group_chat::{ReactToMessage, SendGroupMessage};
use aurelius_domain::project::CreateProject;
use aurelius_domain::views::{MessageView, ThreadSnapshot};
use aurelius_domain::{AppError, Command, CommandId, CommandResult, ConversationId, ErrorCode, EventKind, MessageId, ProjectId};
use aurelius_host::ipc::{self, BoxStream, read_frame, write_frame};
use aurelius_protocol::{HelloParams, Method, Outcome, Receipt, Request, ResponseBody, ServerMessage, WriteMethod, WriteResult, client_feature};
use common::*;
use std::time::Duration;

/// A raw connection signed in as `name`, whose hello accepts `accepts`.
async fn raw(root: &std::path::Path, name: &str, accepts: &[&str]) -> BoxStream {
    let mut stream = ipc::connect(&aurelius_host::endpoint(root)).await.unwrap();
    let token = sign_in(root, &connect_anonymous(root).await, name).await.session_token;
    let accepts = accepts.iter().map(|a| (*a).to_owned()).collect();
    let hello = Request::read(1, Method::SessionHello(HelloParams { client_name: "raw".into(), client_version: "0".into(), accepts, session_token: Some(token) }));
    write_frame(&mut stream, &hello).await.unwrap();
    loop {
        if let ServerMessage::Response { request_id: 1, .. } = read_frame::<ServerMessage, _>(&mut stream).await.unwrap().unwrap() {
            return stream;
        }
    }
}

/// A client of today: it shows channels, ticket channels, threads and reactions.
const NEW: &[&str] = &[client_feature::TEAM_CHANNELS, client_feature::TICKET_CHANNELS, client_feature::CHAT_THREADS, client_feature::CHAT_REACTIONS];
/// A client of before phase 2: channels and ticket channels only.
const OLD: &[&str] = &[client_feature::TEAM_CHANNELS, client_feature::TICKET_CHANNELS];

async fn call(stream: &mut BoxStream, request: Request) -> Outcome {
    let id = request.request_id;
    write_frame(stream, &request).await.unwrap();
    loop {
        if let ServerMessage::Response { request_id, outcome } = read_frame::<ServerMessage, _>(stream).await.unwrap().unwrap()
            && request_id == id
        {
            return outcome;
        }
    }
}

async fn write(stream: &mut BoxStream, command: Command) -> Result<CommandResult, AppError> {
    match call(stream, Request::write(60, CommandId::new(), WriteMethod::Domain(command))).await {
        Outcome::Ok(ResponseBody::Receipt(Receipt { result: WriteResult::Command(result), .. })) => Ok(result),
        Outcome::Error(err) => Err(err),
        other => panic!("{other:?}"),
    }
}

async fn channel_messages(stream: &mut BoxStream, channel: ConversationId) -> Vec<MessageView> {
    match call(stream, Request::read(70, Method::GroupChatGet { channel: Some(channel), peek: false })).await {
        Outcome::Ok(ResponseBody::Messages(snapshot)) => snapshot.messages,
        other => panic!("{other:?}"),
    }
}

async fn read_thread(stream: &mut BoxStream, message_id: MessageId) -> Result<ThreadSnapshot, AppError> {
    match call(stream, Request::read(80, Method::ThreadRead { message_id })).await {
        Outcome::Ok(ResponseBody::Thread(snapshot)) => Ok(snapshot),
        Outcome::Error(err) => Err(err),
        other => panic!("{other:?}"),
    }
}

/// (unread, mentions) of `channel` in `channel.list` for this connection's board member.
async fn badges(stream: &mut BoxStream, channel: ConversationId) -> (u32, u32) {
    match call(stream, Request::read(90, Method::ChannelList)).await {
        Outcome::Ok(ResponseBody::Channels(snapshot)) => snapshot.channels.into_iter().find(|c| c.conversation_id == channel).map(|c| (c.unread, c.mentions)).expect("listed"),
        other => panic!("{other:?}"),
    }
}

fn send(channel: ConversationId, text: &str, thread_parent: Option<MessageId>) -> Command {
    Command::SendGroupMessage(SendGroupMessage { confirm_secret: false, text: text.into(), images: vec![], files: vec![], channel: Some(channel), thread_parent })
}

async fn sent(stream: &mut BoxStream, command: Command) -> MessageId {
    match write(stream, command).await.unwrap() {
        CommandResult::GroupMessageSent { message_id, .. } => message_id,
        other => panic!("{other:?}"),
    }
}

/// A project and its channel.
async fn project_channel(client: &Client, name: &str) -> (ProjectId, ConversationId) {
    let created = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = created.result else { panic!("{:?}", created.result) };
    let channel = client.channels().await.unwrap().channels.into_iter().find(|c| c.project_id == Some(project_id)).expect("its channel").conversation_id;
    (project_id, channel)
}

/// The kinds of every event this connection gets for a while after subscribing from the start.
async fn event_kinds(stream: &mut BoxStream) -> Vec<EventKind> {
    write_frame(stream, &Request::read(2, Method::EventSubscribe { after_seq: 0 })).await.unwrap();
    let mut kinds = Vec::new();
    while let Ok(Ok(Some(message))) = tokio::time::timeout(Duration::from_millis(400), read_frame::<ServerMessage, _>(stream)).await {
        if let ServerMessage::Event(event) = message {
            kinds.push(event.kind);
        }
    }
    kinds
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reply_is_counted_on_its_root_kept_out_of_the_channel_and_read_with_thread_read() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (_, channel) = project_channel(&client, "Atlas").await;
    let mut alex = raw(dir.path(), BOARD_MEMBER, NEW).await;
    let mut grace = raw(dir.path(), "grace", NEW).await;
    let root = sent(&mut alex, send(channel, "Release on Friday?", None)).await;
    channel_messages(&mut alex, channel).await;
    let reply = sent(&mut grace, send(channel, "@alex only if the tests pass", Some(root))).await;

    // Alex: no unread in the channel itself, but the mention in the thread counts in @.
    assert_eq!(badges(&mut alex, channel).await, (0, 1));
    let list = channel_messages(&mut alex, channel).await;
    assert!(list.iter().all(|m| m.id != reply), "a reply is not in the channel's list");
    let on_root = list.iter().find(|m| m.id == root).unwrap();
    assert_eq!((on_root.thread_reply_count, on_root.thread_unread), (Some(1), Some(1)));
    assert!(on_root.thread_last_reply_ms.is_some());
    // Reading the channel did not read the thread.
    assert_eq!(badges(&mut alex, channel).await, (0, 1));

    let thread = read_thread(&mut alex, root).await.unwrap();
    assert_eq!((thread.conversation_id, thread.root.id, thread.read_only), (channel, root, false));
    let replies: Vec<_> = thread.replies.iter().filter(|m| m.system.is_none()).map(|m| (m.id, m.thread_parent)).collect();
    assert_eq!(replies, [(reply, Some(root))]);
    assert_eq!(thread.root.thread_unread, Some(1), "as it was before this read");
    // Now it is read: no unread on the root, no mention in the channel.
    let on_root = channel_messages(&mut alex, channel).await.into_iter().find(|m| m.id == root).unwrap();
    assert_eq!((on_root.thread_reply_count, on_root.thread_unread), (Some(1), None));
    assert_eq!(badges(&mut alex, channel).await, (0, 0));
    // Grace's own reply is never unread for her.
    let for_grace = channel_messages(&mut grace, channel).await.into_iter().find(|m| m.id == root).unwrap();
    assert_eq!(for_grace.thread_unread, None);
    // A reply is no thread of its own.
    assert_eq!(read_thread(&mut alex, reply).await.unwrap_err().code, ErrorCode::Validation);
    assert_eq!(read_thread(&mut alex, MessageId::new()).await.unwrap_err().code, ErrorCode::NotFound);

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_nested_reply_and_a_reply_in_an_archived_channel_are_refused_with_codes_each_client_knows() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (project, channel) = project_channel(&client, "Atlas").await;
    let mut new = raw(dir.path(), BOARD_MEMBER, NEW).await;
    let mut old = raw(dir.path(), BOARD_MEMBER, &[]).await;
    let root = sent(&mut new, send(channel, "Root", None)).await;
    let reply = sent(&mut new, send(channel, "Reply", Some(root))).await;
    let nested = write(&mut new, send(channel, "Nested", Some(reply))).await.unwrap_err();
    assert_eq!(nested.code, ErrorCode::Validation);
    client.execute(aurelius_client::envelope(Command::SetProjectStatus(SetProjectStatus { project_id: project, status: LifecycleStatus::Archived }), None)).await.unwrap();
    assert_eq!(write(&mut new, send(channel, "Late", Some(root))).await.unwrap_err().code, ErrorCode::ChannelArchived);
    assert_eq!(write(&mut old, send(channel, "Late", Some(root))).await.unwrap_err().code, ErrorCode::Validation, "an older client never reads channel_archived");
    let react = Command::ReactToMessage(ReactToMessage { message_id: root, emoji: "👍".into(), on: true });
    assert_eq!(write(&mut new, react.clone()).await.unwrap_err().code, ErrorCode::ChannelArchived);
    assert_eq!(write(&mut old, react).await.unwrap_err().code, ErrorCode::Validation);
    // The thread can still be read, and says it is closed.
    assert!(read_thread(&mut new, root).await.unwrap().read_only);

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_employee_named_in_a_reply_answers_in_that_thread() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (_, channel) = project_channel(&client, "Atlas").await;
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Ann".into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: None };
    client.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap();
    let mut alex = raw(dir.path(), BOARD_MEMBER, NEW).await;
    let root = sent(&mut alex, send(channel, "The build is red.", None)).await;
    sent(&mut alex, send(channel, "@Ann can you look?", Some(root))).await;
    let mut answer = None;
    for _ in 0..200 {
        let thread = read_thread(&mut alex, root).await.unwrap();
        if let Some(found) = thread.replies.into_iter().find(|m| m.author.as_deref() == Some("Ann")) {
            answer = Some(found);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let answer = answer.expect("Ann answered in the thread");
    assert_eq!(answer.thread_parent, Some(root));
    let list = channel_messages(&mut alex, channel).await;
    assert!(list.iter().all(|m| m.id != answer.id), "the thread's answer is not in the channel's own list");
    assert!(list.iter().all(|m| m.thread_parent.is_none()), "no reply is in the channel's own list");
    // The root names nobody, so B3 wakes Ann (the only employee) for it too: that answer is the
    // channel's, and the only one of hers there.
    assert!(list.iter().filter(|m| m.author.as_deref() == Some("Ann")).count() <= 1, "only her answer to the root");
    assert_eq!(list.iter().find(|m| m.id == root).unwrap().thread_reply_count, Some(2));

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_is_idempotent_names_who_and_reaches_only_clients_that_show_reactions() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (_, channel) = project_channel(&client, "Atlas").await;
    let mut alex = raw(dir.path(), BOARD_MEMBER, NEW).await;
    let mut grace = raw(dir.path(), "grace", NEW).await;
    let root = sent(&mut alex, send(channel, "Ship it?", None)).await;
    let react = |emoji: &str, on: bool| Command::ReactToMessage(ReactToMessage { message_id: root, emoji: emoji.into(), on });
    let changed = |result: CommandResult| match result {
        CommandResult::MessageReacted { changed, message_id, conversation_id, .. } => {
            assert_eq!((message_id, conversation_id), (root, channel));
            changed
        }
        other => panic!("{other:?}"),
    };
    assert!(changed(write(&mut grace, react("🚀", true)).await.unwrap()));
    assert!(changed(write(&mut alex, react("🚀", true)).await.unwrap()));
    assert!(!changed(write(&mut alex, react("🚀", true)).await.unwrap()), "on twice is once");
    assert!(changed(write(&mut alex, react("👀", true)).await.unwrap()));
    let reactions = |list: Vec<MessageView>| list.into_iter().find(|m| m.id == root).unwrap().reactions;
    let for_alex = reactions(channel_messages(&mut alex, channel).await);
    assert_eq!(for_alex.iter().map(|r| (r.emoji.as_str(), r.count, r.mine)).collect::<Vec<_>>(), [("🚀", 2, true), ("👀", 1, true)]);
    assert_eq!(for_alex[0].by, ["grace", BOARD_MEMBER]);
    let for_grace = reactions(channel_messages(&mut grace, channel).await);
    assert_eq!(for_grace.iter().map(|r| r.mine).collect::<Vec<_>>(), [true, false]);
    assert!(changed(write(&mut alex, react("🚀", false)).await.unwrap()));
    assert!(!changed(write(&mut alex, react("🚀", false)).await.unwrap()), "off twice is off");
    let after = reactions(channel_messages(&mut alex, channel).await);
    assert_eq!(after.iter().map(|r| (r.emoji.as_str(), r.count, r.mine)).collect::<Vec<_>>(), [("🚀", 1, false), ("👀", 1, true)]);

    // Replayed: only a client that shows reactions gets `message.reacted`.
    let mut shows = raw(dir.path(), BOARD_MEMBER, NEW).await;
    let mut older = raw(dir.path(), BOARD_MEMBER, OLD).await;
    let kinds = event_kinds(&mut shows).await;
    assert_eq!(kinds.iter().filter(|k| **k == EventKind::MessageReacted).count(), 4, "{kinds:?}");
    let kinds = event_kinds(&mut older).await;
    assert!(!kinds.contains(&EventKind::MessageReacted), "{kinds:?}");
    assert!(kinds.contains(&EventKind::MessageUserCommitted));

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_older_client_reads_the_channel_as_before_without_replies_or_new_event_kinds() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (_, channel) = project_channel(&client, "Atlas").await;
    let mut new = raw(dir.path(), BOARD_MEMBER, NEW).await;
    let root = sent(&mut new, send(channel, "Root", None)).await;
    let mut old = raw(dir.path(), BOARD_MEMBER, &[client_feature::TEAM_CHANNELS]).await;
    let before = channel_messages(&mut old, channel).await;
    sent(&mut new, send(channel, "A reply", Some(root))).await;
    write(&mut new, Command::ReactToMessage(ReactToMessage { message_id: root, emoji: "✓".into(), on: true })).await.unwrap();
    let after = channel_messages(&mut old, channel).await;
    // The same messages as before: the reply is not among them.
    assert_eq!(after.iter().map(|m| m.id).collect::<Vec<_>>(), before.iter().map(|m| m.id).collect::<Vec<_>>());
    assert!(after.iter().all(|m| m.text != "A reply"));
    // Every event it gets is a kind an older client knows: no `message.reacted`.
    let kinds = event_kinds(&mut old).await;
    assert!(!kinds.contains(&EventKind::MessageReacted), "{kinds:?}");
    assert!(!kinds.contains(&EventKind::ServerRenamed));

    drop(client);
    stop(dir.path(), daemon).await;
}
