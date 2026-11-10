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

fn code<T: std::fmt::Debug>(result: Result<T, aurelius_client::ClientError>) -> (aurelius_domain::ErrorCode, String) {
    match result {
        Err(aurelius_client::ClientError::Protocol(err)) => (err.code, err.message),
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

/// `ticket.land`: a board member merges a ticket's branch into its base or pushes it for a
/// pull request; whatever could lose or mix up work is refused, and a conflict is undone.
#[tokio::test(flavor = "multi_thread")]
async fn a_board_member_merges_or_pushes_a_tickets_branch_and_nothing_is_left_half_done() {
    use aurelius_domain::ErrorCode;
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start_with(dir.path(), Arc::new(Recording::default())).await;
    let client = common::connect(dir.path()).await;
    assert!(client.supports(aurelius_protocol::feature::TICKET_LANDING));
    // The project's repository follows a remote (a bare one next to it).
    let remote = dir.path().join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    git(&remote, &["init", "-q", "--bare", "-b", "main"]);
    let repo = dir.path().join("repo");
    repository(&repo);
    git(&repo, &["remote", "add", "origin", &remote.display().to_string()]);
    git(&repo, &["push", "-q", "-u", "origin", "main"]);
    let atlas = project(&client, "Atlas", &repo).await;
    let forge = employee(&client, "Forge").await;
    let login = done_ticket(&client, atlas, "Fix the login", forge).await;
    let id = login.ticket.id;
    let work = worktree_of(dir.path(), id).unwrap();

    // Nothing to merge yet, and never while something in the worktree is not committed.
    let (why, said) = code(client.land_ticket(id, "merge").await);
    assert!(why == ErrorCode::Validation && said.contains("nothing to merge"), "{said}");
    std::fs::write(work.join("src/login.rs"), "pub fn login() {}\n").unwrap();
    let (_, said) = code(client.land_ticket(id, "merge").await);
    assert!(said.contains("not committed yet"), "{said}");
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "Add the login"]);
    assert_eq!(code(client.land_ticket(id, "rebase").await).0, ErrorCode::InvalidRequest);

    // Not while the project's folder is elsewhere or has changes of its own.
    git(&repo, &["checkout", "-q", "-b", "other"]);
    let (_, said) = code(client.land_ticket(id, "merge").await);
    assert!(said.contains("is on other, not on main"), "{said}");
    git(&repo, &["checkout", "-q", "main"]);
    std::fs::write(repo.join("src/main.rs"), "fn main() { /* edited */ }\n").unwrap();
    let (_, said) = code(client.land_ticket(id, "merge").await);
    assert!(said.contains("changes that are not committed"), "{said}");
    git(&repo, &["checkout", "-q", "--", "src/main.rs"]);

    // Pushed for a pull request: the remote has the branch, the ticket says so; it can still be merged.
    let pushed = client.land_ticket(id, "push").await.unwrap();
    assert_eq!((pushed.how.as_str(), pushed.branch.as_str(), pushed.base.as_str(), pushed.url), ("pushed", "ticket/1-fix-the-login", "main", None));
    assert_eq!(git(&remote, &["rev-parse", "refs/heads/ticket/1-fix-the-login"]), pushed.commit);
    assert_eq!(client.ticket(id).await.unwrap().ticket.landed.as_deref(), Some("pushed"));

    // Merged: one merge commit by the board member in the project's folder, said in the discussion.
    let merged = client.land_ticket(id, "merge").await.unwrap();
    assert_eq!(merged.how, "merged");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), merged.commit);
    assert_eq!(git(&repo, &["log", "-1", "--format=%s|%an|%P"]).split('|').take(2).collect::<Vec<_>>(), ["Merge ticket #1: Fix the login", common::BOARD_MEMBER]);
    assert!(repo.join("src/login.rs").is_file(), "the ticket's work is in main now");
    let after = client.ticket(id).await.unwrap();
    assert_eq!(after.ticket.landed.as_deref(), Some("merged"));
    let lines: Vec<String> = client.messages(after.ticket.conversation_id.unwrap()).await.unwrap().messages.into_iter().filter(|m| m.system.is_some()).map(|m| m.text).collect();
    assert!(lines.iter().any(|l| l.contains("merged ticket/1-fix-the-login into main")) && lines.iter().any(|l| l.contains("pushed ticket/1-fix-the-login to origin")), "{lines:?}");
    let (_, said) = code(client.land_ticket(id, "merge").await);
    assert!(said.contains("merged into main already"), "{said}");

    // A branch that conflicts with its base: refused with the file, and the folder is as it was.
    let clash = done_ticket(&client, atlas, "Rewrite main", forge).await;
    let clash_work = worktree_of(dir.path(), clash.ticket.id).unwrap();
    std::fs::write(clash_work.join("src/main.rs"), "fn main() { ticket(); }\n").unwrap();
    git(&clash_work, &["commit", "-q", "-am", "The ticket's main"]);
    std::fs::write(repo.join("src/main.rs"), "fn main() { someone_else(); }\n").unwrap();
    git(&repo, &["commit", "-q", "-am", "Someone else's main"]);
    let before = git(&repo, &["rev-parse", "HEAD"]);
    let (why, said) = code(client.land_ticket(clash.ticket.id, "merge").await);
    assert!(why == ErrorCode::Validation && said.contains("conflicts with main in src/main.rs") && said.contains("Nothing was merged"), "{said}");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), before);
    assert!(git(&repo, &["status", "--porcelain", "--untracked-files=no"]).is_empty(), "no merge left half done");
    assert_eq!(client.ticket(clash.ticket.id).await.unwrap().ticket.landed, None);
    common::stop(dir.path(), daemon).await;
}
