//! The Code tab (ticket #77): `code.get` on a real repository in a temp folder, and every
//! `code.*` write with its refusals, through the daemon's API.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::code::CodeView;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::ticket::CreateTicket;
use aurelius_domain::{Command, ErrorCode, ProjectId, project::CreateProject};
use aurelius_protocol::{CommandResult, HostResult, HostWrite, Method, ProtocolError, ResponseBody, WriteMethod, WriteResult};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use common::*;
use std::path::Path;
use std::process::Command as Process;
use std::sync::Arc;
use std::time::Duration;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Process::new("git").args(args).current_dir(dir).env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").env_remove("GIT_INDEX_FILE").output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn commit(dir: &Path, file: &str, body: &str, message: &str) {
    std::fs::write(dir.join(file), body).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "--quiet", "-m", message]);
}

async fn project(client: &Client, name: &str, folder: &Path) -> ProjectId {
    let made = client.execute(envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: Some(folder.display().to_string()) }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = made.result else { panic!("{:?}", made.result) };
    project_id
}

async fn get(client: &Client, project_id: ProjectId) -> CodeView {
    match client.call(Method::CodeGet { project_id }).await.unwrap() {
        ResponseBody::Code(view) => view,
        other => panic!("{other:?}"),
    }
}

async fn host(client: &Client, write: HostWrite) -> Result<CodeView, ProtocolError> {
    match client.write(aurelius_domain::CommandId::new(), None, WriteMethod::Host(write)).await {
        Ok(receipt) => match receipt.result {
            WriteResult::Host(HostResult::Code(view)) => Ok(view),
            other => panic!("{other:?}"),
        },
        Err(aurelius_client::ClientError::Protocol(e)) => Err(e),
        Err(e) => panic!("{e}"),
    }
}

/// A refusal with its code and a message that says `words`.
fn refused(result: Result<CodeView, ProtocolError>, code: ErrorCode, words: &str) {
    let e = result.expect_err("refused");
    assert_eq!(e.code, code, "{}", e.message);
    assert!(e.message.contains(words), "{:?} does not say {words:?}", e.message);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_code_tab_reads_a_repository_and_changes_it_only_as_asked() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;

    // A folder that is no repository: a view that says why, not an error.
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let plain_project = project(&client, "Plain", &plain).await;
    let view = get(&client, plain_project).await;
    assert!(view.problem.as_deref().is_some_and(|p| p.contains("not a git repository")), "{:?}", view.problem);
    assert!(view.branches.is_empty() && view.worktrees.is_empty() && view.commits.is_empty());
    refused(host(&client, HostWrite::CodeFetch { project_id: plain_project }).await, ErrorCode::Validation, "not a git repository");

    // main, a merged branch, a branch in a worktree of a ticket, and unmerged work.
    let repo = dir.path().join("atlas");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet", "--initial-branch=main"]);
    git(&repo, &["config", "user.name", "Test"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    commit(&repo, "readme.md", "hello\n", "first");
    git(&repo, &["switch", "--quiet", "-c", "feat/merged"]);
    commit(&repo, "merged.txt", "m\n", "merged work");
    git(&repo, &["switch", "--quiet", "main"]);
    git(&repo, &["merge", "--quiet", "--no-ff", "-m", "Merge feat/merged", "feat/merged"]);
    git(&repo, &["switch", "--quiet", "-c", "spike/unmerged"]);
    commit(&repo, "spike.txt", "s\n", "spike work");
    git(&repo, &["switch", "--quiet", "main"]);

    let atlas = project(&client, "Atlas", &repo).await;
    let ticket = backlog_ticket(&client, atlas, "The feature").await;
    let number = client.ticket(ticket).await.unwrap().ticket.number;
    let feature = format!("feat/ticket-{number}-feature");
    let worktree = dir.path().join("wt-feature");
    git(&repo, &["worktree", "add", "--quiet", "-b", &feature, &worktree.to_string_lossy(), "main"]);
    commit(&worktree, "feature.txt", "one\ntwo\n", "the feature");

    let view = get(&client, atlas).await;
    assert_eq!(view.problem, None);
    assert_eq!(view.main_branch.as_deref(), Some("main"));
    assert!(!view.has_remote);
    assert_eq!(view.fetched_at_ms, None);
    assert_eq!(view.worktrees.len(), 2);
    assert!(view.worktrees[0].is_main && view.worktrees[0].branch.as_deref() == Some("main"));
    let wt = &view.worktrees[1];
    assert_eq!((wt.branch.as_deref(), wt.is_main, wt.missing, wt.changed_count), (Some(feature.as_str()), false, false, 0));

    let names: Vec<&str> = view.branches.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names[0], "main", "main first");
    assert_eq!(names.len(), 4, "{names:?}");
    let branch = |name: &str| view.branches.iter().find(|b| b.name == name).unwrap();
    assert!(branch("main").is_main && !branch("main").merged);
    let merged = branch("feat/merged");
    assert_eq!((merged.ahead, merged.merged, merged.commits.len()), (0, true, 0));
    let spike = branch("spike/unmerged");
    assert_eq!((spike.ahead, spike.behind, spike.merged), (1, 0, false));
    assert_eq!(spike.commits.len(), 1);
    assert_eq!(spike.subject, "spike work");
    let feat = branch(&feature);
    assert_eq!((feat.ahead, feat.files_changed, feat.insertions, feat.deletions), (1, 1, 2, 0));
    assert_eq!(feat.commits[0].subject, "the feature");
    assert!(feat.worktree_path.as_deref().is_some_and(|p| Path::new(p).ends_with("wt-feature")));
    assert_eq!(feat.ticket.as_ref().map(|t| (t.ticket_id, t.number, t.title.as_str())), Some((ticket, number, "The feature")));
    assert!(!feat.working);

    let merge_commit = view.commits.iter().find(|c| c.subject == "Merge feat/merged").expect("main's merge in the graph");
    assert_eq!(merge_commit.parents.len(), 2);
    assert_eq!(merge_commit.branches, vec!["main".to_owned()]);
    assert!(view.commits.iter().any(|c| c.branches.contains(&feature)), "the feature's tip is named");
    assert_eq!(view.commits.len(), 5, "first, merged work, the merge, spike, feature");

    // Fetch without a remote.
    refused(host(&client, HostWrite::CodeFetch { project_id: atlas }).await, ErrorCode::Validation, "no remote");

    // Deleting: never main, never a checked-out branch, unmerged work only with force.
    let delete = |branch: &str, force: bool| HostWrite::CodeDeleteBranch { project_id: atlas, branch: branch.into(), force };
    refused(host(&client, delete("main", true)).await, ErrorCode::Validation, "main branch");
    refused(host(&client, delete(&feature, true)).await, ErrorCode::Validation, "checked out");
    refused(host(&client, delete("spike/unmerged", false)).await, ErrorCode::Validation, "1 commit(s) that are not in main");
    refused(host(&client, delete("no/such", false)).await, ErrorCode::NotFound, "does not exist");

    // Removing a worktree: never the project's folder, one git does not know, or one with changes.
    let remove = |path: &Path| HostWrite::CodeRemoveWorktree { project_id: atlas, path: path.display().to_string() };
    refused(host(&client, remove(&repo)).await, ErrorCode::Validation, "project's own folder");
    refused(host(&client, remove(&dir.path().join("nowhere"))).await, ErrorCode::NotFound, "does not know");
    std::fs::write(worktree.join("scratch.txt"), "x").unwrap();
    refused(host(&client, remove(&worktree)).await, ErrorCode::Validation, "uncommitted changes");
    let view = get(&client, atlas).await;
    assert_eq!(view.worktrees[1].changed_files, vec!["scratch.txt".to_owned()]);
    std::fs::remove_file(worktree.join("scratch.txt")).unwrap();

    // Merging into a main with uncommitted changes is refused and changes nothing.
    let main_before = git(&repo, &["rev-parse", "main"]);
    std::fs::write(repo.join("readme.md"), "changed\n").unwrap();
    let merge = |branch: &str| HostWrite::CodeMerge { project_id: atlas, branch: branch.into() };
    refused(host(&client, merge(&feature)).await, ErrorCode::Validation, "not clean");
    assert_eq!(git(&repo, &["rev-parse", "main"]), main_before);
    git(&repo, &["checkout", "--quiet", "--", "readme.md"]);
    refused(host(&client, merge("feat/merged")).await, ErrorCode::Validation, "Nothing to merge");

    // A clean merge, with the ticket's number in its message.
    let view = host(&client, merge(&feature)).await.unwrap();
    assert_eq!(git(&repo, &["log", "-1", "--format=%s", "main"]), format!("Merge {feature} (ticket #{number})"));
    let merged = view.branches.iter().find(|b| b.name == feature).unwrap();
    assert!(merged.merged && merged.open, "the answer is the view after the merge; its ticket is still open");

    // Clean up merged while the feature's ticket is still open: only the merged branch
    // without a ticket goes. An open ticket's branch may simply have no commit yet.
    let view = host(&client, HostWrite::CodePruneMerged { project_id: atlas }).await.unwrap();
    let names: Vec<&str> = view.branches.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, vec!["main".to_owned(), feature.clone(), "spike/unmerged".to_owned()]);
    assert!(worktree.exists(), "an open ticket's worktree stays");

    // Once the ticket is off the board, the feature's clean worktree and branch go too; the spike and main stay.
    client.ticket_command(Command::ArchiveTicket(aurelius_domain::ticket::ArchiveTicket { ticket_id: ticket }), None).await.unwrap();
    let view = host(&client, HostWrite::CodePruneMerged { project_id: atlas }).await.unwrap();
    let names: Vec<&str> = view.branches.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, vec!["main", "spike/unmerged"]);
    assert_eq!(view.worktrees.len(), 1);
    assert!(!worktree.exists(), "the worktree's folder is gone");

    // The spike goes only with force.
    let view = host(&client, delete("spike/unmerged", true)).await.unwrap();
    assert_eq!(view.branches.len(), 1);
    assert_eq!(git(&repo, &["rev-parse", "main"]), git(&repo, &["rev-parse", "HEAD"]), "main is checked out as it was");

    stop(dir.path(), daemon).await;
}

/// The fake runtime, holding a run live while its request says HOLD-MAIN until the gate opens.
struct Held {
    gate: tokio::sync::watch::Sender<bool>,
}

#[async_trait::async_trait]
impl AgentRuntime for Held {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let mut gate = self.gate.subscribe();
        if !request.turns.iter().any(|t| t.text.contains("HOLD-MAIN")) || *gate.borrow() {
            return fake::FakeRuntime { delay: Duration::from_millis(1), ..Default::default() }.start(request).await;
        }
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let (input_tx, _input_rx) = tokio::sync::mpsc::channel(8);
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            while !*gate.borrow() {
                tokio::select! {
                    changed = gate.changed() => if changed.is_err() { break },
                    _ = cancel_rx.changed() => {
                        let _ = tx.send(RuntimeEvent::Interrupted).await;
                        return;
                    }
                }
            }
            let _ = tx.send(RuntimeEvent::TextDelta("Report: done.".into())).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

/// A repository on main with one commit, owned by `dir`.
fn repository(dir: &Path) -> std::path::PathBuf {
    let repo = dir.join("hermes");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet", "--initial-branch=main"]);
    git(&repo, &["config", "user.name", "Test"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    commit(&repo, "readme.md", "hello\n", "first");
    repo
}

/// Ask `code.get` until `branch` says `working`.
async fn until_working(client: &Client, project_id: ProjectId, branch: &str, working: bool) {
    for _ in 0..400 {
        if get(client, project_id).await.branches.iter().any(|b| b.name == branch && b.working == working) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{branch} never had working = {working}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_branch_an_employee_works_on_now_is_never_merged_deleted_or_removed() {
    let dir = tempfile::tempdir().unwrap();
    let held = Arc::new(Held { gate: tokio::sync::watch::channel(false).0 });
    let daemon = start_with(dir.path(), held.clone()).await;
    let client = connect(dir.path()).await;
    let repo = repository(dir.path());
    let hermes = project(&client, "Hermes", &repo).await;
    let create = CreateEmployee {
        access_level: None,
        tool_policy: None,
        start_profile: None,
        description: None,
        name: "Dev".into(),
        role: "Engineer".into(),
        title: None,
        reports_to: None,
        instructions: None,
        folder: None,
    };
    let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    let made = client
        .create_ticket(CreateTicket {
            responsible_user_id: None,
            confirm_secret: false,
            project_id: hermes,
            title: "Busy work".into(),
            description: Some("HOLD-MAIN".into()),
            priority: Default::default(),
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: Some(employee_id),
            run: Default::default(),
            files: vec![],
            start: true,
            from_message: None,
            from_idea: None,
            kind: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!("{:?}", made.result) };
    let number = client.ticket(ticket_id).await.unwrap().ticket.number;

    // The ticket's branch in its worktree, and a second branch of it that no worktree has.
    let feature = format!("feat/ticket-{number}-busy");
    let worktree = dir.path().join("wt-busy");
    git(&repo, &["worktree", "add", "--quiet", "-b", &feature, &worktree.to_string_lossy(), "main"]);
    commit(&worktree, "busy.txt", "half done\n", "half of the work");
    let extra = format!("fix/{number}-extra");
    git(&repo, &["branch", &extra, &feature]);
    until_working(&client, hermes, &feature, true).await;

    let main_before = git(&repo, &["rev-parse", "main"]);
    refused(host(&client, HostWrite::CodeMerge { project_id: hermes, branch: feature.clone() }).await, ErrorCode::Validation, &format!("An employee works on {feature} now"));
    refused(host(&client, HostWrite::CodeDeleteBranch { project_id: hermes, branch: extra.clone(), force: true }).await, ErrorCode::Validation, &format!("An employee works on {extra} now"));
    refused(host(&client, HostWrite::CodeRemoveWorktree { project_id: hermes, path: worktree.display().to_string() }).await, ErrorCode::Validation, "An employee works in this worktree now");
    assert_eq!(git(&repo, &["rev-parse", "main"]), main_before, "main unchanged");
    assert!(worktree.exists() && !git(&repo, &["branch", "--list", &extra]).is_empty(), "worktree and branch stay");

    held.gate.send_replace(true);
    until_working(&client, hermes, &feature, false).await;
    stop(dir.path(), daemon).await;
}
