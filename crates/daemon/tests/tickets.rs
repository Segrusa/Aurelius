//! The board over the protocol (Fase 1, S3): reads, writes, live events and the
//! company/project settings, as a board member's client sees them. The engine
//! (main task, review, plan review, dispatcher) has its own files: board.rs,
//! plan_review.rs and triage.rs.

mod common;

use aurelius_client::{Client, ClientError};
use aurelius_domain::company::{UpdateCompany, UpdateProject};
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::*;
use aurelius_domain::{Command, CommandResult, EmployeeId, ErrorCode, EventKind, ProjectId, TicketId};
use std::time::Duration;

fn code<T: std::fmt::Debug>(result: Result<T, ClientError>) -> ErrorCode {
    match result {
        Err(ClientError::Protocol(err)) => err.code,
        other => panic!("expected a protocol error, got {other:?}"),
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

async fn create(client: &Client, project_id: ProjectId, title: &str, priority: TicketPriority) -> (TicketId, u32) {
    let made = client
        .create_ticket(CreateTicket {
            project_id,
            title: title.into(),
            description: Some("As a board member I want …".into()),
            priority,
            labels: vec!["mvp".into()],
            deadline_ms: Some(1_900_000_000_000),
            owner_employee_id: None,
            run: Default::default(),
            files: vec![],
            start: false,
            from_message: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { ticket_id, number } = made.result else { panic!("{:?}", made.result) };
    (ticket_id, number)
}

async fn next_ticket_event(events: &mut tokio::sync::mpsc::UnboundedReceiver<aurelius_protocol::Event>, kind: EventKind) -> aurelius_protocol::Event {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.expect("events");
            if event.kind == kind {
                return event;
            }
        }
    })
    .await
    .expect("the event arrives live")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_board_member_fills_and_works_the_board() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let (client, mut events) = common::connect_events(dir.path()).await;
    client.subscribe(0).await.unwrap();
    let me = match client.user_create("grace", "a-long-enough-password").await.unwrap().result {
        CommandResult::UserCreated { user_id } => user_id,
        other => panic!("{other:?}"),
    };
    let atlas = project(&client, "Atlas").await;
    let hermes = project(&client, "Hermes").await;

    // MU1-19 AC4: an empty board with the six columns.
    let board = client.board(None).await.unwrap();
    assert_eq!(board.columns.iter().map(|c| c.column.kind).collect::<Vec<_>>(), ColumnKind::ALL);
    assert!(board.columns.iter().all(|c| c.tickets.is_empty()));

    let (login, one) = create(&client, atlas, "Login", TicketPriority::High).await;
    let (_, two) = create(&client, hermes, "Search", TicketPriority::default()).await;
    assert_eq!((one, two), (1, 2));
    let created = next_ticket_event(&mut events, EventKind::TicketCreated).await;
    assert_eq!((created.aggregate_id, created.project_id), (login.to_string(), Some(atlas)));

    // B4: one board, filtered per project.
    let atlas_board = client.board(Some(atlas)).await.unwrap();
    assert_eq!(atlas_board.project_id, Some(atlas));
    assert_eq!(atlas_board.columns[0].tickets.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Login"]);
    let card = &atlas_board.columns[0].tickets[0];
    assert_eq!((card.priority, card.created_by_user, card.created_by_username.as_str()), (TicketPriority::High, me, "grace"));
    assert_eq!(card.labels, ["mvp"]);
    assert_eq!(client.tickets(None, Some(ColumnKind::Backlog), Some("sear".into())).await.unwrap().tickets.len(), 1);

    // Drag to Todo: started (MU1-09 AC2), seen live (MU1-05 AC3).
    let todo = board.columns[1].column.id;
    let moved = client.ticket_command(Command::MoveTicket(MoveTicket { ticket_id: login, column_id: todo, before_ticket_id: None }), None).await.unwrap();
    assert!(matches!(moved.result, CommandResult::TicketChanged { column: ColumnKind::Todo, .. }));
    next_ticket_event(&mut events, EventKind::TicketMoved).await;
    next_ticket_event(&mut events, EventKind::TicketStarted).await;
    let detail = client.ticket(login).await.unwrap();
    assert_eq!(detail.ticket.column, ColumnKind::Todo);
    // The engine made the main task; without an owner it is a draft waiting for one (no dispatcher here, B5).
    assert_eq!(detail.root_task.as_ref().map(|t| t.status), Some(aurelius_domain::task::TaskStatus::Draft));
    assert_eq!(detail.tasks.len(), 1);
    // Never to Done by hand (MU1-15 AC2 / MU1-16).
    let done = board.columns[5].column.id;
    assert_eq!(code(client.ticket_command(Command::MoveTicket(MoveTicket { ticket_id: login, column_id: done, before_ticket_id: None }), None).await), ErrorCode::Validation);
    assert_eq!(code(client.ticket_command(Command::AcceptTicket(AcceptTicket { ticket_id: login, note: None }), None).await), ErrorCode::Validation);
    // Cancel: back to Backlog.
    client.ticket_command(Command::CancelTicket(CancelTicket { ticket_id: login }), None).await.unwrap();
    let cancelled = client.ticket(login).await.unwrap();
    assert_eq!(cancelled.ticket.column, ColumnKind::Backlog);
    assert_eq!(cancelled.root_task.map(|t| t.status), Some(aurelius_domain::task::TaskStatus::Cancelled), "the main task ends with it");
    // Archive: off the board, still readable.
    client.ticket_command(Command::ArchiveTicket(ArchiveTicket { ticket_id: login }), None).await.unwrap();
    assert!(client.board(Some(atlas)).await.unwrap().columns.iter().all(|c| c.tickets.is_empty()));
    assert!(client.ticket(login).await.unwrap().ticket.archived_at_ms.is_some());
    assert_eq!(code(client.ticket(TicketId::new()).await), ErrorCode::NotFound);

    // B3: rename and reorder, seen by every client.
    let mut columns: Vec<BoardColumnEdit> = board.columns.iter().map(|c| BoardColumnEdit { column_id: c.column.id, name: c.column.name.clone() }).collect();
    columns[0].name = "Inbox".into();
    client.ticket_command(Command::UpdateBoardColumns(UpdateBoardColumns { columns: columns.clone() }), None).await.unwrap();
    next_ticket_event(&mut events, EventKind::BoardColumnsUpdated).await;
    assert_eq!(client.board(None).await.unwrap().columns[0].column.name, "Inbox");
    columns.truncate(5);
    assert_eq!(code(client.ticket_command(Command::UpdateBoardColumns(UpdateBoardColumns { columns }), None).await), ErrorCode::Validation);

    drop(client);
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_dispatcher_is_set_for_the_company_and_per_project() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let client = common::connect(dir.path()).await;
    let orion = employee(&client, "Orion").await;
    let forge = employee(&client, "Forge").await;
    let atlas = project(&client, "Atlas").await;

    // company.update is a board member's write; project.update is not a ticket write.
    let set_company = Command::UpdateCompany(UpdateCompany { name: Some("Segrusa".into()), dispatcher: Some(SetDispatcher { employee_id: Some(orion) }) });
    assert_eq!(code(client.ticket_command(set_company.clone(), None).await), ErrorCode::BoardMemberRequired);
    client.user_create("grace", "a-long-enough-password").await.unwrap();
    client.ticket_command(set_company, None).await.unwrap();
    let company = client.company().await.unwrap().company;
    assert_eq!((company.name.as_str(), company.dispatcher_employee_id), ("Segrusa", Some(orion)));

    let set_project = |employee_id| Command::UpdateProject(UpdateProject { project_id: atlas, dispatcher: Some(SetDispatcher { employee_id }) });
    client.ticket_command(set_project(Some(forge)), None).await.unwrap();
    let projects = client.projects().await.unwrap().projects;
    assert_eq!(projects.iter().find(|p| p.id == atlas).unwrap().dispatcher_employee_id, Some(forge));
    client.ticket_command(set_project(None), None).await.unwrap();
    let projects = client.projects().await.unwrap().projects;
    assert_eq!(projects.iter().find(|p| p.id == atlas).unwrap().dispatcher_employee_id, None, "MU1-11 AC2: the company's again");
    drop(client);
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn start_with_an_owner_then_update_the_fields() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let client = common::connect(dir.path()).await;
    client.user_create("grace", "a-long-enough-password").await.unwrap();
    let forge = employee(&client, "Forge").await;
    let atlas = project(&client, "Atlas").await;
    let (id, _) = create(&client, atlas, "Owned", TicketPriority::Urgent).await;
    client.ticket_command(Command::StartTicket(StartTicket { ticket_id: id, employee_id: Some(forge) }), None).await.unwrap();
    let ticket = client.ticket(id).await.unwrap().ticket;
    // Started for Forge: the work begins at once (the engine moves it on from Todo by itself).
    assert_ne!(ticket.column, ColumnKind::Backlog);
    assert_eq!((ticket.owner_employee_id, ticket.owner_name.as_deref()), (Some(forge), Some("Forge")));
    // Update: the fields change while the engine works (its moves give the ticket new versions, so none is expected here).
    let rename = Command::UpdateTicket(UpdateTicket { ticket_id: id, title: Some("Owned, renamed".into()), description: None, priority: Some(TicketPriority::Low), labels: None, deadline_ms: None, clear_deadline: true, run: None });
    client.ticket_command(rename, None).await.unwrap();
    let ticket = client.ticket(id).await.unwrap().ticket;
    assert_eq!((ticket.title.as_str(), ticket.priority, ticket.deadline_ms), ("Owned, renamed", TicketPriority::Low, None));
    drop(client);
    common::stop(dir.path(), daemon).await;
}
