//! Claude Design links (feature `design_links`): a workspace's styleguide and a ticket's design,
//! saved over the protocol, and what a run gets because of them.

mod common;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::{Client, ClientError, envelope};
use aurelius_domain::company::UpdateProject;
use aurelius_domain::connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults};
use aurelius_domain::employee::{CreateEmployee, Setting};
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{ColumnKind, StartTicket, UpdateTicket};
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, ErrorCode, ProjectId, RunId, TicketId};
use aurelius_protocol::{EmployeeTaskOp, feature};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, ToolProfile, fake};
use common::*;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const GUIDE: &str = "https://claude.ai/artifact/9sfwWLbDbQerBZ3jmBSkXS";
const DESIGN: &str = "https://claude.ai/code/artifact/8f14e45f-ceea-467f-a22a-2b7a2c55e3b1";

fn code<T: std::fmt::Debug>(result: Result<T, ClientError>) -> ErrorCode {
    match result {
        Err(ClientError::Protocol(err)) => err.code,
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

fn set_design(ticket_id: TicketId, design_url: &str) -> Command {
    Command::UpdateTicket(UpdateTicket { design_url: Some(design_url.into()), confirm_secret: false, ticket_id, title: None, description: None, priority: None, labels: None, deadline_ms: None, clear_deadline: false, run: None, add_files: None })
}

fn set_guide(project_id: ProjectId, styleguide_url: &str) -> Command {
    Command::UpdateProject(UpdateProject { ticket_worktrees: None, styleguide_url: Some(styleguide_url.into()), project_id, dispatcher: None })
}

async fn project(client: &Client, name: &str) -> ProjectId {
    let made = client.execute(envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = made.result else { panic!("{:?}", made.result) };
    project_id
}

async fn styleguide(client: &Client, id: ProjectId) -> Option<String> {
    client.projects().await.unwrap().projects.into_iter().find(|p| p.id == id).unwrap().styleguide_url
}

#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_and_a_ticket_keep_their_links_and_refuse_anything_else() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect_as(dir.path(), "grace").await;
    assert!(client.supports(feature::DESIGN_LINKS));
    let atlas = project(&client, "Atlas").await;
    let ticket = backlog_ticket(&client, atlas, "Dashboard").await;
    assert_eq!(styleguide(&client, atlas).await, None, "none yet");
    assert_eq!(client.ticket(ticket).await.unwrap().ticket.design_url, None);

    // Both saved and read back, over the real protocol.
    client.ticket_command(set_guide(atlas, GUIDE), None).await.unwrap();
    assert_eq!(styleguide(&client, atlas).await.as_deref(), Some(GUIDE));
    client.ticket_command(set_design(ticket, DESIGN), None).await.unwrap();
    assert_eq!(client.ticket(ticket).await.unwrap().ticket.design_url.as_deref(), Some(DESIGN));

    // Refused, and what was saved stays.
    for bad in ["https://example.com/artifact/abc", "https://claude.ai/artifact/abc?x=1", "not a link"] {
        assert_eq!(code(client.ticket_command(set_guide(atlas, bad), None).await), ErrorCode::Validation, "{bad}");
        assert_eq!(code(client.ticket_command(set_design(ticket, bad), None).await), ErrorCode::Validation, "{bad}");
    }
    assert_eq!(code(client.ticket_command(set_guide(atlas, &format!("{GUIDE}{}", "x".repeat(300))), None).await), ErrorCode::Validation);
    assert_eq!(styleguide(&client, atlas).await.as_deref(), Some(GUIDE));
    assert_eq!(client.ticket(ticket).await.unwrap().ticket.design_url.as_deref(), Some(DESIGN));

    // An empty value clears.
    client.ticket_command(set_guide(atlas, ""), None).await.unwrap();
    assert_eq!(styleguide(&client, atlas).await, None);
    client.ticket_command(set_design(ticket, ""), None).await.unwrap();
    assert_eq!(client.ticket(ticket).await.unwrap().ticket.design_url, None);
    stop(dir.path(), daemon).await;
}

fn write(root: &Path, sql: &str, params: impl rusqlite::Params) {
    let conn = rusqlite::Connection::open(root.join("data/aurelius.sqlite")).unwrap();
    conn.busy_timeout(Duration::from_secs(10)).unwrap();
    conn.execute(sql, params).unwrap();
}

/// Records what a run gets and answers like the fake.
#[derive(Clone, Default)]
struct Recording {
    seen: Arc<Mutex<Vec<(Option<bool>, String)>>>,
}

#[async_trait::async_trait]
impl AgentRuntime for Recording {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }

    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        if let ToolProfile::Work { artifact, .. } = &request.profile {
            self.seen.lock().unwrap().push((Some(*artifact), request.system.clone().unwrap_or_default()));
        }
        fake::FakeRuntime::default().start(request).await
    }
}

async fn claude_code(client: &Client, root: &Path) {
    let add = AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude Code".into(), runtime_path: Some(root.join("no-claude").display().to_string()), default_model: None };
    let CommandResult::ProviderAdded { connection_id } = client.execute(envelope(Command::AddProvider(add), None)).await.unwrap().result else { panic!("not added") };
    let settings = RunDefaults { connection: Setting::Value(connection_id), model: Setting::Value("sonnet".into()), persona: Setting::Inherit, effort: Setting::Inherit };
    client.execute(envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose: Purpose::Employee, settings }), None)).await.unwrap();
}

async fn hire(client: &Client, name: &str) -> (EmployeeId, ConversationId) {
    let hire = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client.execute(envelope(Command::CreateEmployee(hire), None)).await.unwrap().result else { panic!("not hired") };
    (employee_id, conversation_id)
}

/// Start `ticket` for `employee` and wait for the run's request.
async fn first_run(client: &Client, runtime: &Recording, ticket: TicketId, employee: EmployeeId, count: usize) -> (Option<bool>, String) {
    client.ticket_command(Command::StartTicket(StartTicket { ticket_id: ticket, employee_id: Some(employee) }), None).await.unwrap();
    for _ in 0..400 {
        if let Some(found) = runtime.seen.lock().unwrap().get(count).cloned() {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no run for {ticket}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_with_a_link_gets_the_tool_a_read_only_one_a_sentence_and_one_without_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Recording::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect_as(dir.path(), "grace").await;
    claude_code(&client, dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    let (with_guide, _) = hire(&client, "Forge").await;

    let linked = backlog_ticket(&client, atlas, "With both").await;
    client.ticket_command(set_guide(atlas, GUIDE), None).await.unwrap();
    client.ticket_command(set_design(linked, DESIGN), None).await.unwrap();
    let (artifact, system) = first_run(&client, &runtime, linked, with_guide, 0).await;
    assert_eq!(artifact, Some(true), "{system}");
    assert!(system.contains(GUIDE) && system.contains("project/README.md") && system.contains(DESIGN) && system.contains("This ticket's design"), "{system}");
    assert!(!system.contains("cannot be opened"), "{system}");

    // A read-only employee on the same project: the link is known, the tool is not offered.
    let (read_only, _) = hire(&client, "Reader").await;
    write(dir.path(), "UPDATE employees SET access_level = 'read_only' WHERE id = ?1", [read_only.to_string()]);
    let plain = backlog_ticket(&client, atlas, "Read only").await;
    let (artifact, system) = first_run(&client, &runtime, plain, read_only, 1).await;
    assert_eq!(artifact, Some(false), "{system}");
    assert!(system.contains("cannot be opened in this run") && !system.contains(GUIDE), "{system}");

    // No links: nothing new.
    let other = project(&client, "Orbit").await;
    let nothing = backlog_ticket(&client, other, "No links").await;
    let (artifact, system) = first_run(&client, &runtime, nothing, with_guide, 2).await;
    assert_eq!(artifact, Some(false), "{system}");
    assert!(!system.contains("Claude Design") && !system.contains("project/README.md"), "{system}");
    stop(dir.path(), daemon).await;
}

/// `update_ticket` on the bridge: an employee links the design it made, with the same check.
#[tokio::test(flavor = "multi_thread")]
async fn an_employee_links_its_design_with_update_ticket() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 100).await;
    let client = connect_as(dir.path(), "grace").await;
    let atlas = project(&client, "Atlas").await;
    let (forge, chat) = hire(&client, "Forge").await;
    let ticket = backlog_ticket(&client, atlas, "Dashboard").await;
    let run = start_employee_run(&client, dir.path(), forge, chat).await;
    client.ticket_command(Command::StartTicket(StartTicket { ticket_id: ticket, employee_id: Some(forge) }), None).await.unwrap();
    assert_ne!(client.ticket(ticket).await.unwrap().ticket.column, ColumnKind::Backlog);

    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let update = |design_url: &str| EmployeeTaskOp::UpdateTicket { design_url: Some(design_url.into()), ticket: "#1".into(), title: None, description: None, priority: None, labels: None };
    let (ok, text) = bridge.employee_act(run, update("https://example.com/x")).await.unwrap();
    assert!(!ok && text.contains("Claude artifact link"), "{text}");
    assert_eq!(client.ticket(ticket).await.unwrap().ticket.design_url, None);
    let (ok, text) = bridge.employee_act(run, update(DESIGN)).await.unwrap();
    assert!(ok, "{text}");
    assert_eq!(client.ticket(ticket).await.unwrap().ticket.design_url.as_deref(), Some(DESIGN));
    let (ok, text) = bridge.employee_act(run, update("")).await.unwrap();
    assert!(ok, "{text}");
    assert_eq!(client.ticket(ticket).await.unwrap().ticket.design_url, None, "empty clears");
    stop(dir.path(), daemon).await;
}

/// A run of `employee` that is live (slow fake runtime), to call the bridge from.
async fn start_employee_run(client: &Client, root: &Path, employee: EmployeeId, conversation_id: ConversationId) -> RunId {
    let send = aurelius_domain::employee::SendEmployeeMessage { confirm_secret: false, employee_id: employee, text: "houd me wakker".into(), conversation_id, images: Vec::new(), files: Vec::new(), interrupt: false };
    let CommandResult::EmployeeMessageSent { run_id, .. } = client.execute(envelope(Command::SendEmployeeMessage(send), None)).await.unwrap().result else { panic!("not sent") };
    for _ in 0..200 {
        let status = common::db(root).query_row("SELECT status FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get::<_, String>(0)).ok();
        if matches!(status.as_deref(), Some("starting" | "running" | "waiting")) {
            return run_id;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the run never started");
}
