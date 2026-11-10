//! The dispatcher of the board (Fase 1, S4, impact analysis §3.7): a ticket
//! started without an owner waits in Todo while its dispatcher (the project's,
//! else the company's) chooses with `assign_ticket` in one short run that has
//! only that tool and `recall`. No dispatcher, or one that chooses nobody (after
//! one more run), leaves the ticket in Todo for a board member (B5).
//!
//! The runtime here is the fake one, scripted for the dispatcher's runs: it
//! calls `assign_ticket` through the real bridge channel, as Claude Code would.

mod common;

use aurelius_client::{Client, bridge::BridgeClient};
use aurelius_domain::company::{UpdateCompany, UpdateProject};
use aurelius_domain::employee::{CreateEmployee, LifecycleStatus, SetEmployeeStatus};
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::TaskStatus;
use aurelius_domain::ticket::*;
use aurelius_domain::{Command, CommandResult, EmployeeId, ProjectId, TicketId};
use aurelius_protocol::EmployeeTaskOp;
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, ToolProfile, fake};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the scripted dispatcher does in its next run: choose this name, or nobody.
struct Dispatching {
    root: PathBuf,
    choices: Mutex<Vec<Option<&'static str>>>,
    /// Each dispatcher run's prompt, tools and turns, and what assign_ticket answered.
    seen: Mutex<Vec<(String, Vec<String>, u32, Option<(bool, String)>)>>,
    /// The dispatcher has no connection: its runs cannot start (as `runtimes::runtime_for_config` says then).
    no_connection: std::sync::atomic::AtomicBool,
    /// How long a dispatcher's run takes after its choice.
    takes: Mutex<Duration>,
}

#[async_trait::async_trait]
impl AgentRuntime for Dispatching {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let ToolProfile::Triage { tools, max_turns, .. } = &request.profile else {
            return fake::FakeRuntime { delay: Duration::from_millis(1), ..Default::default() }.start(request).await;
        };
        let prompt = request.turns.last().map(|t| t.text.clone()).unwrap_or_default();
        if self.no_connection.load(std::sync::atomic::Ordering::SeqCst) {
            self.seen.lock().unwrap().push((prompt, tools.clone(), *max_turns, None));
            return Err(RuntimeError::Permanent("No connection is configured for this conversation. Choose one in Settings (Ctrl ,).".into()));
        }
        let choice = { let mut c = self.choices.lock().unwrap(); if c.is_empty() { None } else { c.remove(0) } };
        let answer = match choice {
            Some(name) => {
                let bridge = BridgeClient::connect(&self.root).await.expect("bridge");
                Some(bridge.employee_act(request.run_id, EmployeeTaskOp::AssignTicket { employee: name.into(), reason: format!("{name} knows this part") }).await.expect("assign_ticket"))
            }
            None => None,
        };
        self.seen.lock().unwrap().push((prompt, tools.clone(), *max_turns, answer));
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let (input_tx, _input_rx) = tokio::sync::mpsc::channel(8);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        let takes = *self.takes.lock().unwrap();
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            tokio::time::sleep(takes).await;
            let _ = tx.send(RuntimeEvent::TextDelta("Done.".into())).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

struct Board {
    _dir: tempfile::TempDir,
    root: PathBuf,
    daemon: common::Daemon,
    client: Client,
    runtime: Arc<Dispatching>,
    project: ProjectId,
}

async fn board(choices: Vec<Option<&'static str>>) -> Board {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let runtime = Arc::new(Dispatching { root: root.clone(), choices: Mutex::new(choices), seen: Mutex::new(vec![]), no_connection: Default::default(), takes: Mutex::new(Duration::ZERO) });
    let daemon = common::start_with(&root, runtime.clone()).await;
    let client = common::connect_as(&root, "grace").await;
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    Board { _dir: dir, root, daemon, client, runtime, project: project_id }
}

async fn employee(client: &Client, name: &str, role: &str) -> EmployeeId {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: role.into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    employee_id
}

async fn company_dispatcher(client: &Client, employee_id: Option<EmployeeId>) {
    client.ticket_command(Command::UpdateCompany(UpdateCompany { name: None, dispatcher: Some(SetDispatcher { employee_id }), tester: None }), None).await.unwrap();
}

async fn started(client: &Client, project_id: ProjectId, title: &str) -> TicketId {
    let made = client
        .create_ticket(CreateTicket {
            responsible_user_id: None,
            confirm_secret: false,
            project_id,
            title: title.into(),
            description: Some("Build the export to CSV.".into()),
            priority: TicketPriority::High,
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: None,
            run: Default::default(),
            files: vec![],
            start: true,
            from_message: None,
            from_idea: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!() };
    ticket_id
}

async fn until(client: &Client, id: TicketId, done: impl Fn(&TicketDetail) -> bool) -> TicketDetail {
    for _ in 0..400 {
        let detail = client.ticket(id).await.unwrap();
        if done(&detail) {
            return detail;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("never: {:?}", client.ticket(id).await.unwrap().ticket);
}

/// The ticket's events of `kind` with their metadata.
fn events(root: &std::path::Path, id: TicketId, kind: &str) -> Vec<serde_json::Value> {
    let db = common::db(root);
    let mut stmt = db.prepare("SELECT metadata_json FROM events WHERE aggregate_type = 'ticket' AND aggregate_id = ?1 AND type = ?2 ORDER BY aggregate_version").unwrap();
    stmt.query_map([id.to_string(), kind.to_string()], |r| r.get::<_, String>(0)).unwrap().map(|m| serde_json::from_str(&m.unwrap()).unwrap()).collect()
}

fn dispatcher_runs(root: &std::path::Path, task: aurelius_domain::TaskId) -> Vec<(String, Option<String>)> {
    let db = common::db(root);
    let mut stmt = db.prepare("SELECT status, employee_id FROM runs WHERE task_id = ?1 AND conversation_id IS NULL").unwrap();
    stmt.query_map([task.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap()
}

/// The system lines of a ticket's discussion, in words (MU2-03).
async fn discussion_lines(client: &Client, id: TicketId) -> Vec<(aurelius_domain::views::SystemLineKind, Option<aurelius_domain::TaskId>, String)> {
    let conversation = client.ticket(id).await.unwrap().ticket.conversation_id.expect("a discussion");
    client.messages(conversation).await.unwrap().messages.into_iter().filter_map(|m| m.system.map(|s| (s.kind, s.task_id, m.text))).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_dispatcher_chooses_an_owner_and_the_work_starts() {
    let b = board(vec![Some("Builder")]).await;
    let orion = employee(&b.client, "Orion", "Project Manager").await;
    let builder = employee(&b.client, "Builder", "Backend Developer").await;
    company_dispatcher(&b.client, Some(orion)).await;

    let id = started(&b.client, b.project, "CSV export").await;
    // MU1-10 AC2: the owner is filled, the main task belongs to them and the card goes on (Doing, then Review).
    let detail = until(&b.client, id, |d| d.ticket.owner_employee_id == Some(builder) && d.root_task.as_ref().is_some_and(|t| t.status == TaskStatus::Review)).await;
    assert_eq!(detail.root_task.as_ref().unwrap().employee_id, Some(builder));
    assert_eq!(detail.ticket.column, ColumnKind::Review);
    let dispatched = events(&b.root, id, "ticket.dispatched");
    assert_eq!(dispatched.len(), 1);
    assert_eq!(dispatched[0]["by"], serde_json::json!({ "kind": "employee", "employee_id": orion }));
    assert_eq!((dispatched[0]["employee_id"].clone(), dispatched[0]["reason"].clone()), (serde_json::json!(builder), serde_json::json!("Builder knows this part")));

    // One short run with only assign_ticket and recall, and the colleagues with their load.
    let seen = b.runtime.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    let (prompt, tools, turns, answer) = &seen[0];
    assert_eq!(tools, &["mcp__aurelius__assign_ticket", "mcp__aurelius__recall"]);
    assert_eq!(*turns, 3);
    assert!(prompt.contains("CSV export") && prompt.contains("- Builder (Backend Developer)") && prompt.contains("Orion (you)") && prompt.contains("[open tasks: 0]"), "{prompt}");
    assert!(answer.as_ref().unwrap().0);
    let root = detail.root_task.unwrap().id;
    let runs = dispatcher_runs(&b.root, root);
    assert_eq!(runs, [("succeeded".to_string(), Some(orion.to_string()))]);
    // MU2-03: the dispatcher's choice with its reason, linked to the main task; once.
    let lines = discussion_lines(&b.client, id).await;
    let chose: Vec<_> = lines.iter().filter(|(_, _, t)| t.starts_with("Orion made")).collect();
    assert_eq!(chose.len(), 1, "{lines:#?}");
    assert_eq!((chose[0].0, chose[0].1, chose[0].2.as_str()), (aurelius_domain::views::SystemLineKind::Delegated, Some(root), "Orion made Builder the owner of this ticket. Reason: Builder knows this part"));

    drop(b.client);
    common::stop(&b.root, b.daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_projects_own_dispatcher_goes_first() {
    let b = board(vec![Some("Builder")]).await;
    let orion = employee(&b.client, "Orion", "Project Manager").await;
    let lead = employee(&b.client, "Lead", "Tech Lead").await;
    employee(&b.client, "Builder", "Backend Developer").await;
    company_dispatcher(&b.client, Some(orion)).await;
    b.client.ticket_command(Command::UpdateProject(UpdateProject { ticket_worktrees: None, styleguide_url: None, project_id: b.project, dispatcher: Some(SetDispatcher { employee_id: Some(lead) }) }), None).await.unwrap();
    let id = started(&b.client, b.project, "Search").await;
    until(&b.client, id, |d| d.ticket.owner_employee_id.is_some()).await;
    assert_eq!(events(&b.root, id, "ticket.dispatched")[0]["by"]["employee_id"], serde_json::json!(lead));
    drop(b.client);
    common::stop(&b.root, b.daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_dispatcher_the_ticket_waits_for_a_board_member() {
    let b = board(vec![]).await;
    let orion = employee(&b.client, "Orion", "Project Manager").await;
    let builder = employee(&b.client, "Builder", "Backend Developer").await;

    // None set (B5): Todo, "choose an owner", no run.
    let id = started(&b.client, b.project, "Nobody chooses").await;
    let waiting = until(&b.client, id, |d| !events(&b.root, d.ticket.id, "ticket.updated").is_empty()).await;
    assert_eq!((waiting.ticket.column, waiting.root_task.as_ref().map(|t| t.status)), (ColumnKind::Todo, Some(TaskStatus::Draft)));
    let needs = events(&b.root, id, "ticket.updated");
    assert_eq!((needs[0]["needs"].as_str(), needs[0]["reason"].as_str()), (Some("owner"), Some("no_dispatcher")));
    assert!(b.runtime.seen.lock().unwrap().is_empty(), "no dispatcher run");
    assert!(discussion_lines(&b.client, id).await.iter().any(|(_, _, t)| t == "Nobody chose an owner: there is no active dispatcher. Choose one on the card."));

    // An archived dispatcher counts as none (B5).
    company_dispatcher(&b.client, Some(orion)).await;
    b.client.execute(aurelius_client::envelope(Command::SetEmployeeStatus(SetEmployeeStatus { employee_id: orion, status: LifecycleStatus::Archived }), None)).await.unwrap();
    let other = started(&b.client, b.project, "Archived dispatcher").await;
    until(&b.client, other, |d| !events(&b.root, d.ticket.id, "ticket.updated").is_empty()).await;
    assert_eq!(events(&b.root, other, "ticket.updated")[0]["reason"], "no_dispatcher");
    assert!(b.runtime.seen.lock().unwrap().is_empty());

    // The board member chooses (MU1-10 AC4): the work starts.
    b.client.ticket_command(Command::AssignTicket(AssignTicket { ticket_id: id, employee_id: Some(builder), reason: Some("knows the export".into()) }), None).await.unwrap();
    assert!(discussion_lines(&b.client, id).await.iter().any(|(_, _, t)| t == "grace made Builder the owner of this ticket. Reason: knows the export"), "a board member's choice");
    // (Builder is the only active employee now, so management: its task starts in Plan and stops in Plan review.)
    let worked = until(&b.client, id, |d| d.root_task.as_ref().is_some_and(|t| t.employee_id == Some(builder) && t.status != TaskStatus::Draft && t.status != TaskStatus::Queued)).await;
    assert_ne!(worked.ticket.column, ColumnKind::Todo);

    drop(b.client);
    common::stop(&b.root, b.daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dispatcher_that_chooses_nobody_gets_one_more_run_then_the_board_decides() {
    // First run: nothing; second: itself while others exist (refused). No third run (RULE 08).
    let b = board(vec![None, Some("Orion"), Some("Builder")]).await;
    let orion = employee(&b.client, "Orion", "Project Manager").await;
    employee(&b.client, "Builder", "Backend Developer").await;
    company_dispatcher(&b.client, Some(orion)).await;
    let id = started(&b.client, b.project, "Hard to place").await;
    let waiting = until(&b.client, id, |d| !events(&b.root, d.ticket.id, "ticket.updated").is_empty()).await;
    assert_eq!(events(&b.root, id, "ticket.updated")[0]["reason"], "dispatcher_did_not_choose");
    assert_eq!((waiting.ticket.column, waiting.ticket.owner_employee_id), (ColumnKind::Todo, None));
    let seen = b.runtime.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2, "two runs, no more");
    assert!(!seen[0].0.contains("Choose with assign_ticket now") && seen[1].0.contains("Choose with assign_ticket now"), "the second run is told to choose");
    let (ok, text) = seen[1].3.clone().unwrap();
    assert!(!ok && text.contains("not yourself"), "{text}");
    assert_eq!(dispatcher_runs(&b.root, waiting.root_task.unwrap().id).len(), 2);
    drop(b.client);
    common::stop(&b.root, b.daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_paused_or_archived_project_dispatcher_leaves_the_choice_to_the_board() {
    // B5 as §3.7.1 says: the dispatcher is the project's ?? the company's; is that one
    // paused or archived, the ticket waits in Todo, the company's does not take over.
    let b = board(vec![Some("Builder"), Some("Builder")]).await;
    let orion = employee(&b.client, "Orion", "Project Manager").await;
    let lead = employee(&b.client, "Lead", "Tech Lead").await;
    employee(&b.client, "Builder", "Backend Developer").await;
    company_dispatcher(&b.client, Some(orion)).await;
    b.client.ticket_command(Command::UpdateProject(UpdateProject { ticket_worktrees: None, styleguide_url: None, project_id: b.project, dispatcher: Some(SetDispatcher { employee_id: Some(lead) }) }), None).await.unwrap();

    for (status, title) in [(LifecycleStatus::Paused, "Paused dispatcher"), (LifecycleStatus::Archived, "Archived dispatcher")] {
        b.client.execute(aurelius_client::envelope(Command::SetEmployeeStatus(SetEmployeeStatus { employee_id: lead, status }), None)).await.unwrap();
        let id = started(&b.client, b.project, title).await;
        let waiting = until(&b.client, id, |d| !events(&b.root, d.ticket.id, "ticket.updated").is_empty()).await;
        let needs = events(&b.root, id, "ticket.updated");
        assert_eq!((needs[0]["needs"].as_str(), needs[0]["reason"].as_str()), (Some("owner"), Some("no_dispatcher")), "{title}");
        assert_eq!((waiting.ticket.column, waiting.ticket.owner_employee_id), (ColumnKind::Todo, None), "{title}");
        assert!(events(&b.root, id, "ticket.dispatched").is_empty(), "{title}: no fall back to the company's dispatcher");
    }
    assert!(b.runtime.seen.lock().unwrap().is_empty(), "no dispatcher run");
    drop(b.client);
    common::stop(&b.root, b.daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dispatcher_without_a_connection_leaves_the_choice_to_the_board() {
    // Its run cannot start: one more try, then "choose an owner" with why (B5).
    let b = board(vec![]).await;
    b.runtime.no_connection.store(true, std::sync::atomic::Ordering::SeqCst);
    let orion = employee(&b.client, "Orion", "Project Manager").await;
    employee(&b.client, "Builder", "Backend Developer").await;
    company_dispatcher(&b.client, Some(orion)).await;
    let id = started(&b.client, b.project, "Unreachable dispatcher").await;
    let waiting = until(&b.client, id, |d| !events(&b.root, d.ticket.id, "ticket.updated").is_empty()).await;
    let needs = events(&b.root, id, "ticket.updated");
    assert_eq!(needs.len(), 1);
    assert_eq!((needs[0]["needs"].as_str(), needs[0]["reason"].as_str()), (Some("owner"), Some("dispatcher_failed")));
    assert_eq!((waiting.ticket.column, waiting.root_task.as_ref().map(|t| t.status)), (ColumnKind::Todo, Some(TaskStatus::Draft)));
    assert_eq!(b.runtime.seen.lock().unwrap().len(), 2, "the run and one more");
    let runs = dispatcher_runs(&b.root, waiting.root_task.unwrap().id);
    assert_eq!(runs.iter().map(|(status, _)| status.as_str()).collect::<Vec<_>>(), ["failed", "failed"]);
    drop(b.client);
    common::stop(&b.root, b.daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dispatcher_whose_run_cannot_be_prepared_leaves_the_choice_to_the_board() {
    // Its settings cannot be read (here: its own conversation is gone), so no run is made at all.
    let b = board(vec![Some("Builder")]).await;
    let orion = employee(&b.client, "Orion", "Project Manager").await;
    let builder = employee(&b.client, "Builder", "Backend Developer").await;
    company_dispatcher(&b.client, Some(orion)).await;
    let db = rusqlite::Connection::open(b.root.join("data/aurelius.sqlite")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    db.execute("UPDATE conversations SET employee_id = ?2 WHERE employee_id = ?1 AND kind = 'employee'", [orion.to_string(), builder.to_string()]).unwrap();
    let id = started(&b.client, b.project, "Unprepared dispatcher").await;
    let waiting = until(&b.client, id, |d| !events(&b.root, d.ticket.id, "ticket.updated").is_empty()).await;
    assert_eq!(events(&b.root, id, "ticket.updated")[0]["reason"], "dispatcher_failed");
    assert_eq!((waiting.ticket.column, waiting.ticket.owner_employee_id), (ColumnKind::Todo, None));
    assert!(b.runtime.seen.lock().unwrap().is_empty());
    assert!(dispatcher_runs(&b.root, waiting.root_task.unwrap().id).is_empty());
    drop(b.client);
    common::stop(&b.root, b.daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dispatcher_whose_first_preparation_fails_gets_a_second_try_after_a_pause() {
    // The cause passes (here: the dispatcher's conversation comes back) during the pause before the second try.
    let b = board(vec![Some("Builder")]).await;
    let orion = employee(&b.client, "Orion", "Project Manager").await;
    let builder = employee(&b.client, "Builder", "Backend Developer").await;
    company_dispatcher(&b.client, Some(orion)).await;
    let db = rusqlite::Connection::open(b.root.join("data/aurelius.sqlite")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    let own: String = db.query_row("SELECT id FROM conversations WHERE employee_id = ?1 AND kind = 'employee'", [orion.to_string()], |r| r.get(0)).unwrap();
    db.execute("UPDATE conversations SET employee_id = ?2 WHERE id = ?1", [own.clone(), builder.to_string()]).unwrap();
    let started_at = std::time::Instant::now();
    let id = started(&b.client, b.project, "Second try").await;
    // The job is claimed: its first preparation fails at once; then it waits before the second.
    for _ in 0..400 {
        let leased: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE kind = 'ticket.triage' AND state = 'leased')", [], |r| r.get(0)).unwrap();
        if leased {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    db.execute("UPDATE conversations SET employee_id = ?2 WHERE id = ?1", [own, orion.to_string()]).unwrap();
    let gave_up = |id| events(&b.root, id, "ticket.updated").iter().any(|e| e["reason"] == "dispatcher_failed");
    let chosen = until(&b.client, id, |d| d.ticket.owner_employee_id.is_some() || gave_up(d.ticket.id)).await;
    assert_eq!(chosen.ticket.owner_employee_id, Some(builder), "{:?}", events(&b.root, id, "ticket.updated"));
    assert!(started_at.elapsed() >= Duration::from_millis(1_400), "the second try came after the pause");
    assert_eq!(b.runtime.seen.lock().unwrap().len(), 1, "one run, made at the second try");
    drop(b.client);
    common::stop(&b.root, b.daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_dispatchers_job_keeps_its_lease_during_a_long_run() {
    let b = board(vec![Some("Builder")]).await;
    *b.runtime.takes.lock().unwrap() = Duration::from_secs(13);
    let orion = employee(&b.client, "Orion", "Project Manager").await;
    employee(&b.client, "Builder", "Backend Developer").await;
    company_dispatcher(&b.client, Some(orion)).await;
    let id = started(&b.client, b.project, "Slow dispatcher").await;
    let lease = |root: &std::path::Path| -> Option<(i64, i64)> {
        common::db(root).query_row("SELECT lease_until_ms, lease_epoch FROM jobs WHERE kind = 'ticket.triage' AND state = 'leased'", [], |r| Ok((r.get(0)?, r.get(1)?))).ok()
    };
    let mut first = None;
    for _ in 0..400 {
        first = lease(&b.root);
        if first.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let (until_ms, epoch) = first.expect("the job is leased");
    tokio::time::sleep(Duration::from_secs(11)).await;
    let (later, same_epoch) = lease(&b.root).expect("still leased while the run goes");
    assert!(later > until_ms, "renewed: {later} > {until_ms}");
    assert_eq!(same_epoch, epoch, "the same claim, not a second one");
    until(&b.client, id, |d| d.root_task.as_ref().is_some_and(|t| t.status == TaskStatus::Review)).await;
    assert_eq!(b.runtime.seen.lock().unwrap().len(), 1, "one run");
    drop(b.client);
    common::stop(&b.root, b.daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_restart_the_dispatchers_job_goes_on_from_todo() {
    // The daemon stops during the dispatcher's run: its lease stays behind and its run stays open.
    let b = board(vec![None]).await;
    *b.runtime.takes.lock().unwrap() = Duration::from_secs(3600);
    let orion = employee(&b.client, "Orion", "Project Manager").await;
    let builder = employee(&b.client, "Builder", "Backend Developer").await;
    company_dispatcher(&b.client, Some(orion)).await;
    let id = started(&b.client, b.project, "Across a restart").await;
    let root = until(&b.client, id, |d| d.root_task.is_some()).await.root_task.unwrap().id;
    for _ in 0..400 {
        if dispatcher_runs(&b.root, root).iter().any(|(status, _)| status == "running") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    drop(b.client);
    common::stop(&b.root, b.daemon).await;
    let leased: String = common::db(&b.root).query_row("SELECT state FROM jobs WHERE kind = 'ticket.triage'", [], |r| r.get(0)).unwrap();
    assert_eq!(leased, "leased", "left behind by the stopped daemon");
    assert_eq!(dispatcher_runs(&b.root, root), [("running".to_string(), Some(orion.to_string()))]);

    // Started again: the job is claimed again, the open run ends as interrupted, and the dispatcher chooses.
    let runtime = Arc::new(Dispatching { root: b.root.clone(), choices: Mutex::new(vec![Some("Builder")]), seen: Mutex::new(vec![]), no_connection: Default::default(), takes: Mutex::new(Duration::ZERO) });
    let daemon = common::start_with(&b.root, runtime.clone()).await;
    let client = common::connect(&b.root).await;
    let detail = until(&client, id, |d| d.ticket.owner_employee_id == Some(builder) && d.ticket.column != ColumnKind::Todo).await;
    assert_eq!(detail.root_task.unwrap().id, root, "the same main task goes on");
    let error: Option<String> = common::db(&b.root).query_row("SELECT error_code FROM runs WHERE task_id = ?1 AND conversation_id IS NULL AND status = 'failed'", [root.to_string()], |r| r.get(0)).unwrap();
    assert_eq!(error.as_deref(), Some("interrupted"));
    let seen = runtime.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].0.contains("Choose with assign_ticket now"), "the interrupted run counted as the first");
    drop(client);
    common::stop(&b.root, daemon).await;
}
