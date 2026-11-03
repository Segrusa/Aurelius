//! The Testing column (ticket #5): with a company tester, a ticket's main task
//! that reports goes to Testing, and the engine gives the tester a test task
//! that runs the full suite once. Its verdict (`report_test_result` through the
//! real bridge path) moves the card: green to Review, red back to Doing with
//! the failures for the owner. A test task that ends without a verdict puts the
//! card in Review with a line. Runs use the fake runtime.

mod common;

use aurelius_application::task::testing::{NOT_THE_TEST_TASK, TARGETED_TESTS_ONLY};
use aurelius_client::{Client, bridge::BridgeClient};
use aurelius_domain::company::{SetTester, UpdateCompany};
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::{CancelTask, TaskStatus};
use aurelius_domain::ticket::*;
use aurelius_domain::views::TaskView;
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, ProjectId, RunId, TicketId};
use aurelius_protocol::EmployeeTaskOp;
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The fake runtime. A test task's run (its brief names `report_test_result`)
/// is held until `test` says how it ends (true: completes, false: fails); a run
/// that says HOLD-MAIN is held until `main` opens. Every system text is kept.
struct Gates {
    main: tokio::sync::watch::Sender<bool>,
    test: tokio::sync::watch::Sender<Option<bool>>,
    systems: Mutex<Vec<(String, String)>>,
}

impl Gates {
    fn new() -> Arc<Self> {
        Arc::new(Self { main: tokio::sync::watch::channel(false).0, test: tokio::sync::watch::channel(None).0, systems: Mutex::new(Vec::new()) })
    }

    /// The system texts of the runs whose turns contain `word`.
    fn systems_of(&self, word: &str) -> Vec<String> {
        self.systems.lock().unwrap().iter().filter(|(turns, _)| turns.contains(word)).map(|(_, s)| s.clone()).collect()
    }
}

/// Hold a run until `gate` gives an end (or the run is stopped).
fn held<T: Clone + Send + Sync + 'static>(request: &RunRequest, mut gate: tokio::sync::watch::Receiver<T>, end: impl Fn(&T) -> Option<bool> + Send + 'static) -> RunHandle {
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let (input_tx, _input_rx) = tokio::sync::mpsc::channel(8);
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        let _ = tx.send(RuntimeEvent::Started).await;
        let completes = loop {
            if let Some(completes) = end(&gate.borrow()) {
                break completes;
            }
            tokio::select! {
                changed = gate.changed() => if changed.is_err() { return },
                _ = cancel_rx.changed() => {
                    let _ = tx.send(RuntimeEvent::Interrupted).await;
                    return;
                }
            }
        };
        let _ = tx.send(RuntimeEvent::TextDelta("Report: done on feat/login.".into())).await;
        let end = if completes { RuntimeEvent::Completed } else { RuntimeEvent::Failed { code: "fake_failure".into(), message: "failed on purpose".into() } };
        let _ = tx.send(end).await;
    });
    RunHandle::new(request.run_id, rx, input_tx, cancel_tx)
}

#[async_trait::async_trait]
impl AgentRuntime for Gates {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let turns: String = request.turns.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join("\n");
        self.systems.lock().unwrap().push((turns.clone(), request.system.clone().unwrap_or_default()));
        if turns.contains("report_test_result") {
            return Ok(held(&request, self.test.subscribe(), |end: &Option<bool>| *end));
        }
        if turns.contains("HOLD-MAIN") && !*self.main.borrow() {
            return Ok(held(&request, self.main.subscribe(), |open: &bool| open.then_some(true)));
        }
        fake::FakeRuntime { delay: Duration::from_millis(1), behavior: fake::FakeBehavior::Normal }.start(request).await
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

async fn set_tester(client: &Client, employee_id: Option<EmployeeId>) {
    let update = UpdateCompany { name: None, dispatcher: None, tester: Some(SetTester { employee_id }) };
    client.ticket_command(Command::UpdateCompany(update), None).await.unwrap();
}

/// Started with an owner, in Auto.
async fn ticket(client: &Client, project_id: ProjectId, title: &str, description: &str, owner: EmployeeId) -> TicketId {
    let made = client
        .create_ticket(CreateTicket {
            responsible_user_id: None,
            confirm_secret: false,
            project_id,
            title: title.into(),
            description: Some(description.into()),
            priority: TicketPriority::default(),
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: Some(owner),
            run: aurelius_domain::task::RunSettings { mode: Some(aurelius_domain::autonomy::AgentMode::Auto), ..Default::default() },
            files: vec![],
            start: true,
            from_message: None,
            from_idea: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!("{:?}", made.result) };
    ticket_id
}

/// Wait until the card is in `column` with its main task in `status`.
async fn until(client: &Client, id: TicketId, column: ColumnKind, status: TaskStatus) -> TicketDetail {
    for _ in 0..800 {
        let detail = client.ticket(id).await.unwrap();
        if detail.ticket.column == column && detail.root_task.as_ref().map(|t| t.status) == Some(status) {
            return detail;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let detail = client.ticket(id).await.unwrap();
    panic!("never {column:?}/{status:?}: {:?}/{:?}", detail.ticket.column, detail.root_task.map(|t| t.status));
}

/// The test tasks of the ticket (children of its main task with the tester), oldest first.
fn test_tasks(detail: &TicketDetail) -> Vec<TaskView> {
    let root = detail.root_task.as_ref().map(|t| t.id);
    detail.tasks.iter().filter(|t| t.parent_task_id.is_some() && t.parent_task_id == root).cloned().collect()
}

/// The open test task's live run, once it has one.
async fn test_run(client: &Client, root: &std::path::Path, id: TicketId) -> (TaskView, RunId) {
    for _ in 0..800 {
        let detail = client.ticket(id).await.unwrap();
        let open = test_tasks(&detail).into_iter().rfind(|t| !t.status.is_terminal());
        if let Some((task, run)) = open.and_then(|t| Some((t.clone(), live_run(root, t.conversation_id?)?))) {
            return (task, run);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no live run of a test task of {id}");
}

fn live_run(root: &std::path::Path, conversation: ConversationId) -> Option<RunId> {
    let found: Option<String> = common::db(root).query_row("SELECT id FROM runs WHERE conversation_id = ?1 AND status = 'running'", [conversation.to_string()], |r| r.get(0)).ok();
    found.map(|r| RunId::parse(&r).unwrap())
}

/// The `to` of each `ticket.moved`, in order.
fn moves(root: &std::path::Path, id: TicketId) -> Vec<String> {
    let db = common::db(root);
    let mut stmt = db.prepare("SELECT json_extract(metadata_json, '$.to') FROM events WHERE aggregate_type = 'ticket' AND aggregate_id = ?1 AND type = 'ticket.moved' ORDER BY aggregate_version").unwrap();
    stmt.query_map([id.to_string()], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

/// `(passed, task_id)` of each `ticket.tested`, in order.
fn verdicts(root: &std::path::Path, id: TicketId) -> Vec<(bool, String)> {
    let db = common::db(root);
    let mut stmt = db
        .prepare("SELECT json_extract(metadata_json, '$.passed'), json_extract(metadata_json, '$.task_id') FROM events WHERE aggregate_type = 'ticket' AND aggregate_id = ?1 AND type = 'ticket.tested' ORDER BY aggregate_version")
        .unwrap();
    stmt.query_map([id.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap()
}

/// The system lines of a ticket's discussion, in words.
async fn lines(client: &Client, id: TicketId) -> Vec<String> {
    let conversation = client.ticket(id).await.unwrap().ticket.conversation_id.expect("a discussion");
    client.messages(conversation).await.unwrap().messages.into_iter().filter(|m| m.system.is_some()).map(|m| m.text).collect()
}

fn verdict(passed: bool, summary: &str) -> EmployeeTaskOp {
    EmployeeTaskOp::ReportTestResult { passed, summary: summary.into() }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_report_goes_to_testing_and_a_green_verdict_of_the_test_task_to_review() {
    let dir = tempfile::tempdir().unwrap();
    let gates = Gates::new();
    let daemon = common::start_with(dir.path(), gates.clone()).await;
    let client = common::connect_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder").await;
    let pulse = employee(&client, "Pulse").await;
    let atlas = project(&client, "Atlas").await;
    set_tester(&client, Some(pulse)).await;
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();

    let id = ticket(&client, atlas, "Login page", "HOLD-MAIN A login page.", builder).await;
    let doing = until(&client, id, ColumnKind::Doing, TaskStatus::Running).await;
    let main = doing.root_task.unwrap();
    let mut main_run = None;
    for _ in 0..800 {
        main_run = live_run(dir.path(), main.conversation_id.expect("the main task's conversation"));
        if main_run.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    // Never trusted: the owner's own run and a made-up run cannot give a verdict.
    for run in [main_run.expect("the main task runs"), RunId::new()] {
        let (ok, text) = bridge.employee_act(run, verdict(true, "all green")).await.unwrap();
        assert_eq!((ok, text.as_str()), (false, NOT_THE_TEST_TASK), "{run}");
    }
    gates.main.send_replace(true);

    // The report: the card goes to Testing, the main task stays in Review, the tester gets a test task.
    let testing = until(&client, id, ColumnKind::Testing, TaskStatus::Review).await;
    let (test, run) = test_run(&client, dir.path(), id).await;
    assert_eq!((test.title.as_str(), test.employee_id, test.parent_task_id), ("Test ticket #1", Some(pulse), Some(main.id)));
    assert_eq!(test_tasks(&testing).len(), 1);
    let report = testing.root_task.unwrap().result.expect("the report is kept");
    assert!(test.goal.contains(report.trim()), "the goal holds the report: {}", test.goal);
    for needed in ["exactly once", "in the background", "suite.log", "own clean review worktree", "do not change the branch", "baseline", "report_test_result"] {
        assert!(test.goal.contains(needed), "{needed}");
    }
    assert!(!gates.systems_of("report_test_result").is_empty(), "the tester's run started");
    let tasks_of_ticket: i64 = common::db(dir.path()).query_row("SELECT COUNT(*) FROM tasks WHERE ticket_id = ?1", [id.to_string()], |r| r.get(0)).unwrap();
    assert_eq!(tasks_of_ticket, 2, "the test task carries the ticket");
    // The main task's work was told to run targeted tests only; the tester was not.
    assert!(gates.systems_of("HOLD-MAIN").iter().all(|s| s.contains(TARGETED_TESTS_ONLY)));
    assert!(gates.systems_of("report_test_result").iter().all(|s| !s.contains(TARGETED_TESTS_ONLY)));

    // A word to the owner while the tests run: the owner answers, the card stays in Testing,
    // the main task in Review (it never waits for its test task), and nothing is tested twice.
    let post = PostTicketMessage { confirm_secret: false, ticket_id: id, text: "Which branch is it on?".into(), images: vec![], files: vec![], thread_parent: None };
    client.ticket_command(Command::PostTicketMessage(post), None).await.unwrap();
    let conversation = main.conversation_id.unwrap().to_string();
    for _ in 0..800 {
        let ended: i64 = common::db(dir.path())
            .query_row("SELECT COUNT(*) FROM runs WHERE conversation_id = ?1 AND status NOT IN ('created','starting','running','waiting','cancelling')", [&conversation], |r| r.get(0))
            .unwrap();
        if ended >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let answered = client.ticket(id).await.unwrap();
    assert_eq!((answered.ticket.column, answered.root_task.as_ref().map(|t| t.status)), (ColumnKind::Testing, Some(TaskStatus::Review)));
    assert_eq!(test_tasks(&answered).len(), 1);

    // The open test task's own run, but the card is not in Testing: refused (and nothing moves).
    let column_of = |kind: &str| -> String { common::db(dir.path()).query_row("SELECT id FROM board_columns WHERE kind = ?1", [kind], |r| r.get(0)).unwrap() };
    let set_column = |column: String| {
        let db = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        db.execute("UPDATE tickets SET column_id = ?1 WHERE id = ?2", [column, id.to_string()]).unwrap();
    };
    set_column(column_of("review"));
    let (ok, text) = bridge.employee_act(run, verdict(true, "all green")).await.unwrap();
    assert_eq!((ok, text.as_str()), (false, NOT_THE_TEST_TASK));
    set_column(column_of("testing"));

    // A verdict without a summary is no verdict.
    let (ok, _) = bridge.employee_act(run, verdict(true, "  ")).await.unwrap();
    assert!(!ok);
    let (ok, text) = bridge.employee_act(run, verdict(true, "1093 passed, 0 failed, 16 ignored; log target/t/suite.log")).await.unwrap();
    assert!(ok, "{text}");
    let review = until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    assert_eq!(test_tasks(&review)[0].status, TaskStatus::Succeeded);
    assert_eq!(moves(dir.path(), id), ["todo", "doing", "testing", "review"]);
    assert_eq!(verdicts(dir.path(), id), [(true, test.id.to_string())]);
    let said = lines(&client, id).await;
    assert!(said.contains(&"Builder reported — the ticket moved from Doing to Testing".to_string()), "{said:#?}");
    assert!(said.contains(&"Pulse: the tests passed · 1093 passed, 0 failed, 16 ignored; log target/t/suite.log".to_string()), "{said:#?}");
    assert_eq!(said.iter().filter(|t| t.ends_with("handed in the report. It is ready for your review")).count(), 1, "{said:#?}");
    // Once given, the verdict is closed: the same run is refused now.
    let (ok, text) = bridge.employee_act(run, verdict(false, "again")).await.unwrap();
    assert_eq!((ok, text.as_str()), (false, NOT_THE_TEST_TASK));

    drop(client);
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_red_verdict_sends_the_failures_to_the_owner_and_the_next_report_is_tested_again() {
    let dir = tempfile::tempdir().unwrap();
    let gates = Gates::new();
    let daemon = common::start_with(dir.path(), gates.clone()).await;
    let client = common::connect_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder").await;
    let pulse = employee(&client, "Pulse").await;
    let atlas = project(&client, "Atlas").await;
    set_tester(&client, Some(pulse)).await;
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();

    let id = ticket(&client, atlas, "Login page", "A login page.", builder).await;
    until(&client, id, ColumnKind::Testing, TaskStatus::Review).await;
    let (first, run) = test_run(&client, dir.path(), id).await;
    let (ok, text) = bridge.employee_act(run, verdict(false, "2 new failures: login::empty_password, login::lockout")).await.unwrap();
    assert!(ok, "{text}");

    // The owner works on with the tester's message, reports again, and that goes to Testing again.
    let (second, run) = test_run(&client, dir.path(), id).await;
    assert_ne!(second.id, first.id);
    let again = until(&client, id, ColumnKind::Testing, TaskStatus::Review).await;
    let tests = test_tasks(&again);
    assert_eq!(tests.iter().map(|t| (t.id, t.status)).collect::<Vec<_>>(), [(first.id, TaskStatus::Succeeded), (second.id, tests[1].status)]);
    assert!(!tests[1].status.is_terminal());
    assert_eq!(moves(dir.path(), id), ["todo", "doing", "testing", "doing", "testing"]);
    let main = again.root_task.unwrap();
    let messages = client.messages(main.conversation_id.unwrap()).await.unwrap().messages;
    let told = messages.iter().find(|m| m.text.starts_with("Testing failed:")).expect("the failures are in the main task's conversation");
    assert_eq!(told.text, "Testing failed: 2 new failures: login::empty_password, login::lockout\n\nFix it and report again.");
    assert_eq!((told.update.as_deref(), told.author.as_deref()), (Some("message"), Some("Pulse")), "said by the tester");
    let said = lines(&client, id).await;
    assert!(said.contains(&"Pulse: the tests failed · 2 new failures: login::empty_password, login::lockout".to_string()), "{said:#?}");
    assert!(said.contains(&"Builder is working on the ticket again after the failed tests — the ticket moved from Testing to Doing".to_string()), "{said:#?}");

    // Green the second time: Review.
    let (ok, text) = bridge.employee_act(run, verdict(true, "1095 passed")).await.unwrap();
    assert!(ok, "{text}");
    until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    assert_eq!(verdicts(dir.path(), id), [(false, first.id.to_string()), (true, second.id.to_string())]);

    drop(client);
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_test_task_that_ends_without_a_verdict_never_leaves_the_card_in_testing() {
    let dir = tempfile::tempdir().unwrap();
    let gates = Gates::new();
    let daemon = common::start_with(dir.path(), gates.clone()).await;
    let client = common::connect_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder").await;
    let pulse = employee(&client, "Pulse").await;
    let atlas = project(&client, "Atlas").await;
    set_tester(&client, Some(pulse)).await;
    let could_not = |said: &[String], why: &str| said.contains(&format!("Testing could not run: {why}. A board member decides."));

    // Its run fails: Review with a line; a board member can accept from there.
    let failed = ticket(&client, atlas, "Failed test", "One.", builder).await;
    until(&client, failed, ColumnKind::Testing, TaskStatus::Review).await;
    test_run(&client, dir.path(), failed).await;
    gates.test.send_replace(Some(false));
    let review = until(&client, failed, ColumnKind::Review, TaskStatus::Review).await;
    assert_eq!(test_tasks(&review)[0].status, TaskStatus::Failed);
    assert!(could_not(&lines(&client, failed).await, "the test task failed"), "{:#?}", lines(&client, failed).await);
    client.ticket_command(Command::AcceptTicket(AcceptTicket { confirm_secret: false, ticket_id: failed, note: None }), None).await.unwrap();
    until(&client, failed, ColumnKind::Done, TaskStatus::Succeeded).await;
    gates.test.send_replace(None);

    // Cancelled by hand: Review with a line.
    let cancelled = ticket(&client, atlas, "Cancelled test", "Two.", builder).await;
    until(&client, cancelled, ColumnKind::Testing, TaskStatus::Review).await;
    let (test, _) = test_run(&client, dir.path(), cancelled).await;
    client.execute(aurelius_client::envelope(Command::CancelTask(CancelTask { task_id: test.id }), None)).await.unwrap();
    until(&client, cancelled, ColumnKind::Review, TaskStatus::Review).await;
    assert!(could_not(&lines(&client, cancelled).await, "the test task was cancelled"));

    // The ticket cancelled from Testing: Backlog, its test task ends with it, no Review.
    let dropped = ticket(&client, atlas, "Dropped", "Three.", builder).await;
    until(&client, dropped, ColumnKind::Testing, TaskStatus::Review).await;
    let (test, _) = test_run(&client, dir.path(), dropped).await;
    client.ticket_command(Command::CancelTicket(CancelTicket { ticket_id: dropped }), None).await.unwrap();
    let detail = client.ticket(dropped).await.unwrap();
    assert_eq!(detail.ticket.column, ColumnKind::Backlog);
    assert_eq!(detail.tasks.iter().find(|t| t.id == test.id).map(|t| t.status), Some(TaskStatus::Cancelled));
    assert!(!lines(&client, dropped).await.iter().any(|t| t.starts_with("Testing could not run")));
    assert!(!moves(dir.path(), dropped).contains(&"review".to_string()));

    // The tester answered without calling the tool: Review with a line.
    let silent = ticket(&client, atlas, "Silent", "Four.", builder).await;
    until(&client, silent, ColumnKind::Testing, TaskStatus::Review).await;
    test_run(&client, dir.path(), silent).await;
    gates.test.send_replace(Some(true));
    until(&client, silent, ColumnKind::Review, TaskStatus::Review).await;
    assert!(could_not(&lines(&client, silent).await, "the tester finished without a verdict"));
    assert!(verdicts(dir.path(), silent).is_empty());

    // Without a tester everything is as before: straight to Review, no test task, no note.
    set_tester(&client, None).await;
    let plain = ticket(&client, atlas, "Plain", "NO-TESTER five.", builder).await;
    let review = until(&client, plain, ColumnKind::Review, TaskStatus::Review).await;
    assert_eq!(review.tasks.len(), 1);
    assert_eq!(moves(dir.path(), plain), ["todo", "doing", "review"]);
    let systems = gates.systems_of("NO-TESTER");
    assert!(!systems.is_empty() && systems.iter().all(|s| !s.contains("Do not run the full test suite")));

    drop(client);
    common::stop(dir.path(), daemon).await;
}
