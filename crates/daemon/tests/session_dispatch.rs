//! The dispatcher of a work session (feature `session_dispatch`): a session started
//! without an employee waits while the dispatcher chooses with `assign_session` in one
//! short run that has only that tool and `recall`; the chosen employee then answers
//! the first message. One that chooses nobody (after one more run) leaves the choice
//! to the board member.
//!
//! The runtime here is the fake one, scripted for the dispatcher's runs: it calls
//! `assign_session` through the real bridge channel, as Claude Code would.

mod common;

use aurelius_client::{Client, bridge::BridgeClient};
use aurelius_domain::company::UpdateCompany;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::SetDispatcher;
use aurelius_domain::work_session::DispatchWorkSession;
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, ProjectId};
use aurelius_protocol::EmployeeTaskOp;
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, ToolProfile, fake};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the scripted dispatcher does in its next run: choose this name, or nobody.
struct Dispatching {
    root: PathBuf,
    choices: Mutex<Vec<Option<&'static str>>>,
    /// Each dispatcher run's prompt and tools, and what assign_session answered.
    seen: Mutex<Vec<(String, Vec<String>, Option<(bool, String)>)>>,
}

#[async_trait::async_trait]
impl AgentRuntime for Dispatching {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let ToolProfile::Triage { tools, .. } = &request.profile else {
            return fake::FakeRuntime { delay: Duration::from_millis(1), ..Default::default() }.start(request).await;
        };
        let prompt = request.turns.last().map(|t| t.text.clone()).unwrap_or_default();
        let choice = {
            let mut c = self.choices.lock().unwrap();
            if c.is_empty() { None } else { c.remove(0) }
        };
        let answer = match choice {
            Some(name) => {
                let bridge = BridgeClient::connect(&self.root).await.expect("bridge");
                Some(bridge.employee_act(request.run_id, EmployeeTaskOp::AssignSession { employee: name.into(), reason: format!("{name} knows this part") }).await.expect("assign_session"))
            }
            None => None,
        };
        self.seen.lock().unwrap().push((prompt, tools.clone(), answer));
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let (input_tx, _input_rx) = tokio::sync::mpsc::channel(8);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            let _ = tx.send(RuntimeEvent::TextDelta("Done.".into())).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

struct Company {
    _dir: tempfile::TempDir,
    _daemon: common::Daemon,
    client: Client,
    runtime: Arc<Dispatching>,
    project: ProjectId,
    flux: EmployeeId,
}

async fn employee(client: &Client, name: &str, role: &str) -> EmployeeId {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: role.into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    employee_id
}

/// Grace's company: Orion dispatches, Flux builds the frontend.
async fn company(choices: Vec<Option<&'static str>>) -> Company {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let runtime = Arc::new(Dispatching { root: root.clone(), choices: Mutex::new(choices), seen: Mutex::new(vec![]) });
    let daemon = common::start_with(&root, runtime.clone()).await;
    let client = common::connect_as(&root, "grace").await;
    let folder = root.join("atlas");
    std::fs::create_dir_all(&folder).unwrap();
    let made = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: Some(folder.to_string_lossy().into_owned()) }), None)).await.unwrap().result;
    let CommandResult::ProjectCreated { project_id, .. } = made else { panic!("{made:?}") };
    let orion = employee(&client, "Orion", "Project Manager").await;
    let flux = employee(&client, "Flux", "Frontend Developer").await;
    client.ticket_command(Command::UpdateCompany(UpdateCompany { name: None, dispatcher: Some(SetDispatcher { employee_id: Some(orion) }), tester: None }), None).await.unwrap();
    Company { _dir: dir, _daemon: daemon, client, runtime, project: project_id, flux }
}

async fn dispatch(c: &Company) -> ConversationId {
    let command = Command::DispatchWorkSession(DispatchWorkSession { project_id: c.project, text: "The login page is blank.".into(), images: vec![], files: vec![], worktree: false, run: Default::default(), confirm_secret: false });
    let made = c.client.execute(aurelius_client::envelope(command, None)).await.unwrap().result;
    let CommandResult::WorkSessionDispatching { conversation_id, .. } = made else { panic!("{made:?}") };
    conversation_id
}

/// The session's lines from Aurelius, and whether Flux answered (the fake runtime signs its answers).
async fn until(c: &Company, conversation: ConversationId, done: impl Fn(&[String], bool) -> bool) -> (Vec<String>, bool) {
    for _ in 0..400 {
        let snapshot = c.client.messages(conversation).await.unwrap();
        let lines: Vec<String> = snapshot.messages.iter().filter(|m| m.update.as_deref() == Some("system")).map(|m| m.text.clone()).collect();
        let answered = snapshot.messages.iter().any(|m| m.update.is_none() && m.role == aurelius_domain::conversation::MessageRole::Assistant && m.text.starts_with("[Flux"));
        if done(&lines, answered) {
            return (lines, answered);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("never: {:?}", c.client.messages(conversation).await.unwrap().messages.iter().map(|m| (&m.update, &m.text)).collect::<Vec<_>>());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_dispatcher_chooses_who_takes_the_session_and_they_answer() {
    let c = company(vec![Some("Flux")]).await;
    let conversation = dispatch(&c).await;
    let (lines, _) = until(&c, conversation, |_, answered| answered).await;
    assert_eq!(lines, ["Orion is choosing who takes this…", "Orion chose Flux: Flux knows this part"]);
    let seen = c.runtime.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    let (prompt, tools, answer) = &seen[0];
    assert!(prompt.contains("The login page is blank.") && prompt.contains("Flux"), "{prompt}");
    assert_eq!(tools, &["mcp__aurelius__assign_session", "mcp__aurelius__recall"]);
    assert!(answer.as_ref().is_some_and(|(ok, _)| *ok), "{answer:?}");
    let snapshot = c.client.messages(conversation).await.unwrap();
    assert_eq!(snapshot.employee_id, Some(c.flux), "Flux works in it now");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dispatcher_that_chooses_nobody_twice_leaves_the_session_to_the_board_member() {
    let c = company(vec![None, None]).await;
    let conversation = dispatch(&c).await;
    let (lines, answered) = until(&c, conversation, |lines, _| lines.len() == 2).await;
    assert_eq!(lines[1], "The dispatcher chose nobody. Choose who takes this session.");
    assert!(!answered, "nobody answers");
    let seen = c.runtime.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2, "one more run, then the board member chooses");
    assert!(seen[1].0.contains("You ended without choosing"));
}
