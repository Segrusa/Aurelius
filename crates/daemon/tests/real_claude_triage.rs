//! End to end with the real Claude Code CLI (it uses the user's own Claude
//! login and a few tokens, so this test is ignored by default): the dispatcher
//! of a ticket chooses its owner with the bridge's `assign_ticket` in a run that
//! has no other tools and at most three turns.
//!
//! Run with: cargo test -p aurelius-daemon --test real_claude_triage -- --ignored --nocapture
//! (build `aurelius-bridge` first: `cargo build -p aurelius-agent-bridge`, same target directory.)

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, EmployeeId, ProjectId,
    company::UpdateCompany,
    connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults},
    employee::{CreateEmployee, Setting},
    project::CreateProject,
    ticket::{CreateTicket, SetDispatcher, TicketPriority},
};
use aurelius_protocol::{CommandResult, Method, ResponseBody};
use common::*;
use std::path::PathBuf;
use std::time::Duration;

/// A project folder, an employee, and Claude Code as the connection for everything.
async fn setup(client: &Client) -> (EmployeeId, ProjectId, PathBuf) {
    let claude = aurelius_runtime::claude_code::locate().expect("Claude Code is installed");
    let CommandResult::ProviderAdded { connection_id } = client
        .execute(envelope(
            Command::AddProvider(AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude Code".into(), runtime_path: Some(claude.display().to_string()), default_model: None }),
            None,
        ))
        .await
        .unwrap()
        .result
    else {
        panic!("connection not added")
    };
    // Wait for the probe: ready means installed and signed in.
    for _ in 0..200 {
        let ResponseBody::Connections(snapshot) = client.call(Method::ProviderList).await.unwrap() else { panic!() };
        let connection = snapshot.connections.iter().find(|c| c.id == connection_id).expect("the connection exists");
        // "needs_auth" is also the status before the probe has run: judge it only once verified.
        match (connection.status.as_str(), connection.last_verified_ms) {
            ("ready", _) => break,
            ("needs_auth" | "unavailable", Some(_)) => panic!("Claude Code is not usable: {connection:?}"),
            _ => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    for purpose in [Purpose::Employee, Purpose::Memory] {
        let settings = RunDefaults { connection: Setting::Value(connection_id), model: Setting::Value("haiku".into()), persona: Setting::Inherit, effort: Setting::Inherit };
        client.execute(envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose, settings }), None)).await.unwrap();
    }
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Plans".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let folder = PathBuf::from(client.projects().await.unwrap().projects.into_iter().find(|p| p.id == project_id).unwrap().folder.expect("project folder"));
    let CommandResult::EmployeeCreated { employee_id, .. } = client
        .execute(envelope(
            Command::CreateEmployee(CreateEmployee { name: "Alex".into(), role: "Builder".into(), title: None, reports_to: None, instructions: None, folder: None }),
            None,
        ))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    (employee_id, project_id, folder)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the real Claude Code CLI"]
async fn a_real_dispatcher_assigns_the_ticket() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let client = connect_as(dir.path(), "grace").await;
    let (dispatcher, project, _) = setup(&client).await;
    let CommandResult::EmployeeCreated { employee_id: designer, .. } = client
        .execute(envelope(Command::CreateEmployee(CreateEmployee { name: "Dana".into(), role: "UI/UX Designer".into(), title: None, reports_to: None, instructions: None, folder: None }), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    let CommandResult::EmployeeCreated { .. } = client
        .execute(envelope(Command::CreateEmployee(CreateEmployee { name: "Sam".into(), role: "Database Engineer".into(), title: None, reports_to: None, instructions: None, folder: None }), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    client.ticket_command(Command::UpdateCompany(UpdateCompany { name: None, dispatcher: Some(SetDispatcher { employee_id: Some(dispatcher) }), tester: None }), None).await.unwrap();
    let made = client
        .create_ticket(CreateTicket {
            responsible_user_id: None,
            confirm_secret: false,
            project_id: project,
            title: "New colour palette and icons for the settings screen".into(),
            description: Some("Design a new colour palette and icon set for the settings screen (visual design only).".into()),
            priority: TicketPriority::default(),
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: None,
            run: Default::default(),
            files: vec![],
            start: true,
            from_message: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!() };
    let mut owner: Option<EmployeeId> = None;
    for _ in 0..1200 {
        owner = client.ticket(ticket_id).await.unwrap().ticket.owner_employee_id;
        if owner.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let db = db(dir.path());
    let reason: Option<String> = db
        .query_row("SELECT json_extract(metadata_json, '$.reason') FROM events WHERE aggregate_id = ?1 AND type = 'ticket.dispatched'", [ticket_id.to_string()], |r| r.get(0))
        .ok()
        .flatten();
    let runs: Vec<(String, Option<String>)> = {
        let mut stmt = db.prepare("SELECT status, error_code FROM runs WHERE conversation_id IS NULL").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap()
    };
    println!("OWNER: {owner:?} (designer {designer}); REASON: {reason:?}; DISPATCHER RUNS: {runs:?}");
    assert_eq!(owner, Some(designer), "the designer owns a design ticket");
    assert_eq!(runs.len(), 1, "one dispatcher run");
    stop(dir.path(), daemon).await;
}
