//! A channel per ticket (feature `ticket_channels`, AUR-CHAT-001): the ticket's
//! discussion is its channel. Only a connection that accepts `ticket_channels`
//! sees ticket channels in `channel.list` and gets the code `channel_archived`;
//! an older one sees the list of before and still reads and posts in a ticket's
//! discussion. General gets one line when a ticket opens and one when it closes.

mod common;

use aurelius_client::Client;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::group_chat::{ReactToMessage, SendGroupMessage};
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::TaskStatus;
use aurelius_domain::ticket::*;
use aurelius_domain::views::{ChannelKind, ChannelView, MessageView, SystemLineKind, system_action};
use aurelius_domain::{AppError, Command, CommandId, CommandResult, ConversationId, EmployeeId, ErrorCode, MessageId, ProjectId, TicketId};
use aurelius_host::ipc::{self, BoxStream, read_frame, write_frame};
use aurelius_protocol::{HelloParams, Method, Outcome, Receipt, Request, ResponseBody, ServerMessage, WriteMethod, WriteResult, client_feature};
use common::*;
use std::time::Duration;

/// A raw connection signed in as `name`, whose hello accepts `accepts`.
async fn raw(root: &std::path::Path, name: &str, accepts: &[&str]) -> BoxStream {
    let mut stream = ipc::connect(&aurelius_host::endpoint(root)).await.unwrap();
    let token = sign_in(root, &connect_anonymous(root).await, name).await.session_token;
    let accepts = accepts.iter().map(|a| (*a).to_owned()).collect();
    let hello = Request::read(1, Method::SessionHello(HelloParams { client_name: "raw".into(), client_version: "0".into(), app_key: None, accepts, session_token: Some(token) }));
    write_frame(&mut stream, &hello).await.unwrap();
    loop {
        if let ServerMessage::Response { request_id: 1, .. } = read_frame::<ServerMessage, _>(&mut stream).await.unwrap().unwrap() {
            return stream;
        }
    }
}

const NEW: &[&str] = &[client_feature::TEAM_CHANNELS, client_feature::TICKET_CHANNELS];
const OLD: &[&str] = &[client_feature::TEAM_CHANNELS];

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

async fn list(stream: &mut BoxStream, method: Method) -> Vec<ChannelView> {
    match call(stream, Request::read(50, method)).await {
        Outcome::Ok(ResponseBody::Channels(snapshot)) => snapshot.channels,
        other => panic!("{other:?}"),
    }
}

async fn write(stream: &mut BoxStream, command: Command) -> Result<CommandResult, AppError> {
    match call(stream, Request::write(60, CommandId::new(), WriteMethod::Domain(command))).await {
        Outcome::Ok(ResponseBody::Receipt(Receipt { result: WriteResult::Command(result), .. })) => Ok(result),
        Outcome::Error(err) => Err(err),
        other => panic!("{other:?}"),
    }
}

async fn project(client: &Client, name: &str) -> ProjectId {
    let created = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = created.result else { panic!("{:?}", created.result) };
    project_id
}

async fn employee(client: &Client, name: &str) -> EmployeeId {
    let create = CreateEmployee { name: name.into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let created = client.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap();
    let CommandResult::EmployeeCreated { employee_id, .. } = created.result else { panic!("{:?}", created.result) };
    employee_id
}

async fn ticket(client: &Client, project_id: ProjectId, title: &str, owner: Option<EmployeeId>, start: bool) -> TicketId {
    let made = client
        .create_ticket(CreateTicket {
            confirm_secret: false,
            project_id,
            title: title.into(),
            description: Some("Do it.".into()),
            priority: TicketPriority::default(),
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: owner,
            run: aurelius_domain::task::RunSettings { mode: Some(aurelius_domain::autonomy::AgentMode::Auto), ..Default::default() },
            files: vec![],
            start,
            from_message: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!("{:?}", made.result) };
    ticket_id
}

async fn channel_of(client: &Client, id: TicketId) -> ConversationId {
    client.ticket(id).await.unwrap().ticket.conversation_id.expect("a discussion")
}

async fn general(client: &Client) -> Vec<MessageView> {
    client.group_chat().await.unwrap().messages
}

/// The lines in General about `ticket`, as (kind, action, channel link, text).
async fn general_lines(client: &Client, ticket: TicketId) -> Vec<(SystemLineKind, Option<String>, Option<ConversationId>, String)> {
    general(client).await.into_iter().filter_map(|m| m.system.filter(|s| s.ticket_id == Some(ticket)).map(|s| (s.kind, s.action, s.channel_id, m.text))).collect()
}

fn members(root: &std::path::Path, channel: ConversationId) -> Vec<String> {
    let db = common::db(root);
    let mut stmt = db
        .prepare("SELECT COALESCE(u.username, e.name) FROM channel_members m LEFT JOIN users u ON u.id = m.user_id LEFT JOIN employees e ON e.id = m.employee_id WHERE m.conversation_id = ?1 ORDER BY 1")
        .unwrap();
    stmt.query_map([channel.to_string()], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

fn post(ticket_id: TicketId, text: &str) -> Command {
    Command::PostTicketMessage(PostTicketMessage { confirm_secret: false, ticket_id, text: text.into(), images: vec![], files: vec![], thread_parent: None })
}

fn send(channel: ConversationId, text: &str) -> Command {
    Command::SendGroupMessage(SendGroupMessage { confirm_secret: false, text: text.into(), images: vec![], files: vec![], channel: Some(channel), thread_parent: None })
}

async fn until(client: &Client, id: TicketId, column: ColumnKind, status: TaskStatus) {
    for _ in 0..400 {
        let detail = client.ticket(id).await.unwrap();
        if detail.ticket.column == column && detail.root_task.as_ref().map(|t| t.status) == Some(status) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("never {column:?}/{status:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_older_client_sees_the_list_of_before_and_still_works_in_a_ticket_discussion() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    project(&client, "Orbit").await;
    let atlas = project(&client, "Atlas").await;
    let mut old = raw(dir.path(), BOARD_MEMBER, OLD).await;
    let before = list(&mut old, Method::ChannelList).await;

    let id = ticket(&client, atlas, "Fix the login page", None, false).await;
    let channel = channel_of(&client, id).await;

    // The older client: General and the project channels, as before, none of the new fields.
    let after = list(&mut old, Method::ChannelList).await;
    let shape = |c: &[ChannelView]| c.iter().map(|c| (c.name.clone(), c.conversation_id, c.project_id.is_some())).collect::<Vec<_>>();
    assert_eq!(shape(&after), shape(&before));
    assert_eq!(after.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["General", "Atlas", "Orbit"]);
    for c in &after {
        assert_eq!((c.kind, c.ticket_id, c.member, c.member_count, c.read_through), (ChannelKind::General, None, false, None, None), "{c:?}");
        let json = serde_json::to_value(c).unwrap();
        assert!(json.get("kind").is_none() && json.get("member").is_none(), "{json}");
    }
    // It still posts and reads in the ticket's discussion (through TicketView::conversation_id).
    assert!(matches!(write(&mut old, post(id, "Which browser?")).await, Ok(CommandResult::TicketMessagePosted { .. })));
    let read = call(&mut old, Request::read(70, Method::ConversationGet { conversation_id: channel })).await;
    let Outcome::Ok(ResponseBody::Messages(snapshot)) = read else { panic!("{read:?}") };
    assert!(snapshot.messages.iter().any(|m| m.text == "Which browser?"));

    // A client that shows ticket channels gets the ticket's, after the team channels.
    let mut new = raw(dir.path(), BOARD_MEMBER, NEW).await;
    let channels = list(&mut new, Method::ChannelList).await;
    assert_eq!(channels.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["General", "Atlas", "Orbit", "#1-fix-the-login-page"]);
    let mine = &channels[3];
    assert_eq!((mine.kind, mine.ticket_id, mine.ticket_number, mine.ticket_column, mine.conversation_id), (ChannelKind::Ticket, Some(id), Some(1), Some(ColumnKind::Backlog), channel));
    assert!(mine.member && !mine.archived, "{mine:?}");
    assert_eq!(mine.member_count, Some(1), "alex made it and is the Owner");
    assert_eq!(channels[0].kind, ChannelKind::General);
    assert_eq!(channels[1].kind, ChannelKind::Project);
    // channel.search finds it by number and by name; an older client never gets ticket channels there.
    assert_eq!(list(&mut new, Method::ChannelSearch { query: "#1".into() }).await.iter().map(|c| c.conversation_id).collect::<Vec<_>>(), [channel]);
    assert_eq!(list(&mut new, Method::ChannelSearch { query: "LOGIN".into() }).await.len(), 1);
    assert!(list(&mut old, Method::ChannelSearch { query: "login".into() }).await.is_empty());
    // And it reads and writes it as a channel too.
    assert!(matches!(write(&mut new, send(channel, "Chrome")).await, Ok(CommandResult::GroupMessageSent { .. })));
    let read = call(&mut new, Request::read(71, Method::GroupChatGet { channel: Some(channel) })).await;
    let Outcome::Ok(ResponseBody::Messages(snapshot)) = read else { panic!("{read:?}") };
    assert!(snapshot.messages.iter().any(|m| m.text == "Chrome"));
    // read_through: what the reader had read before; the read above moved it.
    let after_read = list(&mut new, Method::ChannelSearch { query: "#1".into() }).await;
    assert!(after_read[0].read_through.is_some_and(|r| r >= 2), "{:?}", after_read[0]);

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn general_gets_one_line_when_a_ticket_opens_and_one_when_it_closes_and_its_events_stay_in_its_channel() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder").await;
    let atlas = project(&client, "Atlas").await;
    let id = ticket(&client, atlas, "Login page", Some(builder), true).await;
    let channel = channel_of(&client, id).await;
    until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    client.ticket_command(Command::AcceptTicket(AcceptTicket { confirm_secret: false, ticket_id: id, note: None }), None).await.unwrap();

    let lines = general_lines(&client, id).await;
    assert_eq!(lines.len(), 2, "{lines:#?}");
    assert_eq!((lines[0].0, lines[0].1.as_deref(), lines[0].2), (SystemLineKind::TicketCreated, Some(system_action::OPENED), Some(channel)));
    assert_eq!(lines[0].3, "grace opened #1 “Login page”");
    assert_eq!((lines[1].0, lines[1].1.as_deref(), lines[1].2), (SystemLineKind::Done, Some(system_action::CLOSED), Some(channel)));
    assert_eq!(lines[1].3, "#1 “Login page” closed · grace");
    // Nothing else of the ticket went to General: every other line is in its channel.
    assert_eq!(general(&client).await.iter().filter(|m| m.system.is_some()).count(), 2);
    let own = client.messages(channel).await.unwrap().messages;
    let texts: Vec<&str> = own.iter().filter(|m| m.system.is_some()).map(|m| m.text.as_str()).collect();
    assert!(texts.iter().any(|t| t.ends_with("· Backlog → Todo")) && texts.contains(&"grace put it on Done"), "{texts:#?}");
    let done = own.iter().filter_map(|m| m.system.as_ref()).find(|s| s.kind == SystemLineKind::Done && s.action.as_deref() == Some(system_action::APPROVED));
    assert!(done.is_some(), "Accept is an approval");
    assert!(texts.contains(&"#1 is closed · this channel can only be read now"), "{texts:#?}");
    let archived: Option<i64> = common::db(dir.path()).query_row("SELECT archived_at_ms FROM conversations WHERE id = ?1", [channel.to_string()], |r| r.get(0)).unwrap();
    assert!(archived.is_some(), "Done archives the channel");

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_closed_channel_refuses_writes_with_channel_archived_and_stays_listed_and_searchable() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let id = ticket(&client, project(&client, "Atlas").await, "Old idea", None, false).await;
    let channel = channel_of(&client, id).await;
    client.ticket_command(Command::ArchiveTicket(ArchiveTicket { ticket_id: id }), None).await.unwrap();

    let mut new = raw(dir.path(), BOARD_MEMBER, NEW).await;
    for command in [post(id, "Still there?"), send(channel, "Still there?")] {
        let refused = write(&mut new, command).await.unwrap_err();
        assert_eq!(refused.code, ErrorCode::ChannelArchived, "{refused:?}");
        assert!(refused.message.contains("#1 is archived"), "{}", refused.message);
    }
    // An older client cannot read that code: it gets `validation`, with the same words.
    let mut old = raw(dir.path(), BOARD_MEMBER, OLD).await;
    let refused = write(&mut old, post(id, "Still there?")).await.unwrap_err();
    assert_eq!(refused.code, ErrorCode::Validation);
    assert!(refused.message.contains("#1 is archived"));
    // Listed as archived, and found by search.
    let found = list(&mut new, Method::ChannelSearch { query: "old".into() }).await;
    assert_eq!(found.len(), 1);
    assert!(found[0].archived && found[0].kind == ChannelKind::Ticket, "{:?}", found[0]);
    assert_eq!(general_lines(&client, id).await.iter().map(|l| l.1.clone()).collect::<Vec<_>>(), [Some("opened".to_owned()), Some("closed".to_owned())]);

    drop(client);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_creator_the_owner_and_the_assignee_are_members_and_a_new_assignee_joins() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    // alex makes the server (the Owner); grace and bob come later.
    let alex = connect(dir.path()).await;
    let grace = connect_as(dir.path(), "grace").await;
    connect_as(dir.path(), "bob").await;
    let builder = employee(&alex, "Builder").await;
    let helper = employee(&alex, "Helper").await;
    let id = ticket(&grace, project(&alex, "Atlas").await, "Export", Some(builder), false).await;
    let channel = channel_of(&grace, id).await;
    assert_eq!(members(dir.path(), channel), ["Builder", "alex", "grace"]);

    grace.ticket_command(Command::AssignTicket(AssignTicket { ticket_id: id, employee_id: Some(helper), reason: None }), None).await.unwrap();
    // The new one joins; nobody leaves.
    assert_eq!(members(dir.path(), channel), ["Builder", "Helper", "alex", "grace"]);

    let mut as_grace = raw(dir.path(), "grace", NEW).await;
    let mine = list(&mut as_grace, Method::ChannelSearch { query: "#1".into() }).await;
    assert_eq!((mine[0].member, mine[0].member_count), (true, Some(4)));
    let mut as_bob = raw(dir.path(), "bob", NEW).await;
    let theirs = list(&mut as_bob, Method::ChannelSearch { query: "#1".into() }).await;
    assert_eq!((theirs[0].member, theirs[0].member_count), (false, Some(4)), "bob is not in it, and still sees it");

    drop((alex, grace));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn threads_and_reactions_are_refused_until_phase_2() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let id = ticket(&client, project(&client, "Atlas").await, "Threads", None, false).await;
    let mut new = raw(dir.path(), BOARD_MEMBER, NEW).await;
    let reply = Command::PostTicketMessage(PostTicketMessage { confirm_secret: false, ticket_id: id, text: "A reply".into(), images: vec![], files: vec![], thread_parent: Some(MessageId::new()) });
    let refused = write(&mut new, reply).await.unwrap_err();
    assert_eq!(refused.code, ErrorCode::Validation);
    assert!(refused.message.contains("not supported"), "{}", refused.message);
    let react = Command::ReactToMessage(ReactToMessage { message_id: MessageId::new(), emoji: "👍".into(), on: true });
    let refused = write(&mut new, react).await.unwrap_err();
    assert_eq!(refused.code, ErrorCode::Validation);
    assert!(refused.message.contains("not supported"), "{}", refused.message);

    drop(client);
    stop(dir.path(), daemon).await;
}
