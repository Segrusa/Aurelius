//! Only worktrees (ticket #86): a ticket's run works in a worktree of its own that the
//! daemon makes before the run, never in the project's folder, which stays on main.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::connection::{
    AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults,
};
use aurelius_domain::employee::{CreateEmployee, Setting};
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::CreateTicket;
use aurelius_domain::{Command, CommandResult, EmployeeId, ProjectId};
use aurelius_runtime::{
    AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent,
    ToolProfile, fake,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// Answers at once and keeps the folders each run was given.
#[derive(Default)]
struct Recorder {
    folders: Mutex<Vec<Vec<PathBuf>>>,
}

#[async_trait::async_trait]
impl AgentRuntime for Recorder {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        if let ToolProfile::Work { folders, .. } = &request.profile {
            self.folders.lock().unwrap().push(folders.clone());
        }
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            let _ = tx.send(RuntimeEvent::TextDelta("Done.".into())).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

fn git(folder: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Runs go through Claude Code (the recorder stands in for it), so they get a work folder.
async fn claude_code(client: &Client, root: &Path) {
    let add = AddProvider {
        provider: ProviderKind::ClaudeCode,
        display_name: "Claude Code".into(),
        runtime_path: Some(root.join("no-claude").display().to_string()),
        default_model: None,
    };
    let CommandResult::ProviderAdded { connection_id } = client
        .execute(envelope(Command::AddProvider(add), None))
        .await
        .unwrap()
        .result
    else {
        panic!("not added")
    };
    let settings = RunDefaults {
        connection: Setting::Value(connection_id),
        model: Setting::Value("sonnet".into()),
        persona: Setting::Inherit,
        effort: Setting::Inherit,
    };
    client
        .execute(envelope(
            Command::SetRunDefaults(SetRunDefaults {
                scope: ConfigScope::Global,
                purpose: Purpose::Employee,
                settings,
            }),
            None,
        ))
        .await
        .unwrap();
}

async fn project(client: &Client, folder: &Path) -> ProjectId {
    let create = CreateProject {
        name: "Tidewater".into(),
        description: None,
        folder: Some(folder.display().to_string()),
    };
    let CommandResult::ProjectCreated { project_id, .. } = client
        .execute(envelope(Command::CreateProject(create), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    project_id
}

async fn employee(client: &Client) -> EmployeeId {
    let create = CreateEmployee {
        access_level: None,
        tool_policy: None,
        start_profile: None,
        description: None,
        name: "Forge".into(),
        role: "Backend Developer".into(),
        title: None,
        reports_to: None,
        instructions: None,
        folder: None,
    };
    let CommandResult::EmployeeCreated { employee_id, .. } = client
        .execute(envelope(Command::CreateEmployee(create), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    employee_id
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tickets_run_works_in_its_own_worktree_and_the_project_folder_stays_on_main() {
    let dir = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    common::git_repo(folder.path());
    let recorder = Arc::new(Recorder::default());
    let daemon = common::start_with(dir.path(), recorder.clone()).await;
    let client = common::connect_as(dir.path(), "grace").await;
    claude_code(&client, dir.path()).await;
    let project_id = project(&client, folder.path()).await;
    let forge = employee(&client).await;
    let made = client
        .create_ticket(CreateTicket {
            responsible_user_id: None,
            confirm_secret: false,
            project_id,
            title: "Export faster".into(),
            description: Some("Make the export faster.".into()),
            priority: Default::default(),
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: Some(forge),
            run: Default::default(),
            files: vec![],
            start: true,
            from_message: None,
            from_idea: None,
            kind: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { number, .. } = made.result else {
        panic!("{:?}", made.result)
    };

    let mut seen = Vec::new();
    for _ in 0..400 {
        seen = recorder.folders.lock().unwrap().clone();
        if !seen.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let first = seen.first().expect("the ticket's main task ran").clone();
    let worktree = dir
        .path()
        .join("worktrees")
        .join(format!("ticket-{number}"));
    let worktree = std::fs::canonicalize(&worktree).unwrap_or(worktree);
    let given: Vec<PathBuf> = first
        .iter()
        .map(|f| std::fs::canonicalize(f).unwrap_or_else(|_| f.clone()))
        .collect();
    assert_eq!(given.first(), Some(&worktree), "{given:?}");
    let project_folder = std::fs::canonicalize(folder.path()).unwrap();
    assert!(
        !given.contains(&project_folder),
        "never the project's folder: {given:?}"
    );
    assert_eq!(
        git(&worktree, &["symbolic-ref", "--short", "HEAD"]),
        format!("ticket/{number}")
    );
    assert_eq!(
        git(folder.path(), &["symbolic-ref", "--short", "HEAD"]),
        "main",
        "the project's folder stays on main"
    );
    common::stop(dir.path(), daemon).await;
}
