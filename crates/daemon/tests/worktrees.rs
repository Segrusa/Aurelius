//! A git worktree and a branch per ticket (feature `ticket_worktrees`), against a real
//! repository: a ticket's first run makes them, every run of the ticket works there and
//! is told so, the branch is what the ticket changed, and a closed ticket's worktree
//! goes only when nothing in it is uncommitted. Runs use a fake runtime.
#![cfg(unix)]

mod common;

use aurelius_client::Client;
use aurelius_domain::company::UpdateProject;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::TaskStatus;
use aurelius_domain::ticket::*;
use aurelius_domain::{Command, CommandResult, EmployeeId, ProjectId, TicketId};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, fake};
use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The fake runtime, keeping the system text of every run.
#[derive(Default)]
struct Recording {
    systems: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl AgentRuntime for Recording {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        self.systems.lock().unwrap().push(request.system.clone().unwrap_or_default());
        fake::FakeRuntime { delay: Duration::from_millis(1), behavior: fake::FakeBehavior::Normal }.start(request).await
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let done = Process::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Forge", "-c", "user.email=forge@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(done.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&done.stderr));
    String::from_utf8_lossy(&done.stdout).trim().to_owned()
}

fn repository(at: &Path) {
    std::fs::create_dir_all(at.join("src")).unwrap();
    git(at, &["init", "-q", "-b", "main"]);
    std::fs::write(at.join("src/main.rs"), "fn main() {}\n").unwrap();
    git(at, &["add", "."]);
    git(at, &["commit", "-q", "-m", "Start"]);
}

async fn project(client: &Client, name: &str, folder: &Path) -> ProjectId {
    let create = CreateProject { name: name.into(), description: None, folder: Some(folder.display().to_string()) };
    let CommandResult::ProjectCreated { project_id, .. } = client.execute(aurelius_client::envelope(Command::CreateProject(create), None)).await.unwrap().result else { panic!() };
    project_id
}

async fn employee(client: &Client, name: &str) -> EmployeeId {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    employee_id
}

/// A ticket started with an owner; back once its main task reported.
async fn done_ticket(client: &Client, project_id: ProjectId, title: &str, owner: EmployeeId) -> TicketDetail {
    let made = client
        .create_ticket(CreateTicket {
            responsible_user_id: None,
            confirm_secret: false,
            project_id,
            title: title.into(),
            description: Some("Do it.".into()),
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
    for _ in 0..800 {
        let detail = client.ticket(ticket_id).await.unwrap();
        if detail.root_task.as_ref().map(|t| t.status) == Some(TaskStatus::Review) {
            return detail;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{title} never reported");
}

fn worktree_of(root: &Path, ticket: TicketId) -> Option<PathBuf> {
    let path: Option<String> = common::db(root).query_row("SELECT worktree FROM tickets WHERE id = ?1", [ticket.to_string()], |r| r.get(0)).unwrap();
    path.map(PathBuf::from)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_works_on_its_own_branch_in_its_own_worktree_and_leaves_no_work_behind() {
    let dir = tempfile::tempdir().unwrap();
    let recording = Arc::new(Recording::default());
    let mut daemon = common::start_with(dir.path(), recording.clone()).await;
    let client = common::connect(dir.path()).await;
    assert!(client.supports(aurelius_protocol::feature::TICKET_WORKTREES));
    let repo = dir.path().join("repo");
    repository(&repo);
    let atlas = project(&client, "Atlas", &repo).await;
    assert!(client.projects().await.unwrap().projects.iter().any(|p| p.id == atlas && p.ticket_worktrees), "on by default");
    let forge = employee(&client, "Forge").await;

    // The first run of a ticket gives it a branch from where the folder is, checked out in a worktree of its own.
    let login = done_ticket(&client, atlas, "Fix the login!", forge).await;
    assert_eq!((login.ticket.branch.as_deref(), login.ticket.base_branch.as_deref()), (Some("ticket/1-fix-the-login"), Some("main")));
    let work = worktree_of(dir.path(), login.ticket.id).expect("a worktree");
    assert!(work.join("src/main.rs").is_file(), "the repository's files are there");
    assert!(!work.starts_with(&repo), "never inside the project's folder");
    assert_eq!(git(&work, &["rev-parse", "--abbrev-ref", "HEAD"]), "ticket/1-fix-the-login");
    assert_eq!(git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]), "main", "the project's folder stays where it was");
    assert!(git(&repo, &["status", "--porcelain"]).is_empty(), "and clean");

    // Its runs work there, and are told so.
    let conversation = login.root_task.as_ref().unwrap().conversation_id.unwrap();
    let folders = aurelius_storage::conversations::work_folders(&common::db(dir.path()), conversation).unwrap();
    assert_eq!(folders.first().map(PathBuf::from), Some(work.clone()));
    let told = recording.systems.lock().unwrap().iter().any(|s| s.contains("on the branch ticket/1-fix-the-login, made from main") && s.contains(&work.display().to_string()));
    assert!(told, "the run knows its worktree");

    // A second ticket has its own: the two never write through each other.
    let board = done_ticket(&client, atlas, "Board drag", forge).await;
    let other = worktree_of(dir.path(), board.ticket.id).expect("a worktree");
    assert_ne!(other, work);
    assert_eq!(board.ticket.branch.as_deref(), Some("ticket/2-board-drag"));

    // What the ticket committed is its branch: read as what it has that its base has not.
    std::fs::write(work.join("src/login.rs"), "pub fn login() {}\n").unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "Add the login"]);
    let log = client.git_branch_log(atlas, "ticket/1-fix-the-login", "main").await.unwrap();
    assert_eq!(log.commits.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["Add the login"]);
    let diff = client.git_branch_diff(atlas, "ticket/1-fix-the-login", "main").await.unwrap();
    assert!(diff.text.contains("+pub fn login() {}") && !diff.text.contains("fn main()"), "{}", diff.text);
    assert!(client.git_branch_log(atlas, "ticket/2-board-drag", "main").await.unwrap().commits.is_empty(), "the other ticket changed nothing");

    // Closed: the first has everything committed, the second has a file nobody committed.
    std::fs::write(other.join("draft.txt"), "not committed\n").unwrap();
    for id in [login.ticket.id, board.ticket.id] {
        let version = client.ticket(id).await.unwrap().ticket.version;
        client.ticket_command(Command::CancelTicket(CancelTicket { ticket_id: id }), Some(version)).await.unwrap();
    }
    common::stop(dir.path(), daemon).await;
    daemon = common::start_with(dir.path(), recording.clone()).await;
    let client = common::connect(dir.path()).await;
    for _ in 0..400 {
        if worktree_of(dir.path(), login.ticket.id).is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(worktree_of(dir.path(), login.ticket.id), None, "a closed ticket's clean worktree goes");
    assert!(!work.exists());
    assert_eq!(git(&repo, &["rev-parse", "--verify", "ticket/1-fix-the-login"]).len(), 40, "its branch stays");
    assert_eq!(client.ticket(login.ticket.id).await.unwrap().ticket.branch.as_deref(), Some("ticket/1-fix-the-login"));
    assert_eq!(worktree_of(dir.path(), board.ticket.id), Some(other.clone()), "one with uncommitted work stays");
    assert_eq!(std::fs::read_to_string(other.join("draft.txt")).unwrap(), "not committed\n");

    // Switched off for a project: its tickets work in the folder, as before.
    let plain = dir.path().join("plain");
    repository(&plain);
    let hermes = project(&client, "Hermes", &plain).await;
    let off = UpdateProject { project_id: hermes, dispatcher: None, styleguide_url: None, ticket_worktrees: Some(false) };
    client.execute(aurelius_client::envelope(Command::UpdateProject(off), None)).await.unwrap();
    let old_way = done_ticket(&client, hermes, "As before", forge).await;
    assert_eq!((old_way.ticket.branch, worktree_of(dir.path(), old_way.ticket.id)), (None, None));
    let conversation = old_way.root_task.as_ref().unwrap().conversation_id.unwrap();
    let folders = aurelius_storage::conversations::work_folders(&common::db(dir.path()), conversation).unwrap();
    assert_eq!(folders.first().map(|f| std::fs::canonicalize(f).unwrap()), Some(std::fs::canonicalize(&plain).unwrap()));
    common::stop(dir.path(), daemon).await;
}
