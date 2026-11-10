//! The git state of a project's work folder (feature `project_git`), against a real
//! repository: uncommitted changes, branches, the history, a commit and its diff,
//! and that nothing a client names gets out of the repository or becomes an option.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::envelope;
use aurelius_domain::project::CreateProject;
use aurelius_domain::{Command, CommandResult, ErrorCode};
use std::path::Path;
use std::process::Command as Process;

fn git(dir: &Path, args: &[&str]) {
    let done = Process::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Forge", "-c", "user.email=forge@example.com", "-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .args(args)
        .env("GIT_AUTHOR_DATE", "2026-11-01T10:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-11-01T10:00:00Z")
        .output()
        .expect("git runs");
    assert!(done.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&done.stderr));
}

fn code<T: std::fmt::Debug>(result: Result<T, aurelius_client::ClientError>) -> ErrorCode {
    match result {
        Err(aurelius_client::ClientError::Protocol(err)) => err.code,
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_projects_changes_branches_history_and_diffs_are_read_from_its_repository() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    assert!(client.supports(aurelius_protocol::feature::PROJECT_GIT));

    // A folder that is no repository: an overview that says so, and nothing to list.
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let create = |name: &str, folder: &Path| Command::CreateProject(CreateProject { name: name.into(), description: None, folder: Some(folder.display().to_string()) });
    let CommandResult::ProjectCreated { project_id: loose, .. } = client.execute(envelope(create("Loose", &plain), None)).await.unwrap().result else { panic!() };
    let overview = client.git_overview(loose).await.unwrap();
    assert!(!overview.repository && overview.folder.is_some() && overview.changes.is_empty() && overview.branches.is_empty());
    assert_eq!(code(client.git_log(loose, None, None, 0, 10).await), ErrorCode::NotFound);

    // A repository with two commits on main, a branch with one more, and work in progress.
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    git(&repo, &["init", "-q"]);
    std::fs::write(repo.join("README.md"), "# Atlas\n").unwrap();
    std::fs::write(repo.join("src/main.rs"), "fn main() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "Start the project\n\nThe first files."]);
    std::fs::write(repo.join("src/main.rs"), "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
    git(&repo, &["commit", "-q", "-am", "Say hi"]);
    git(&repo, &["checkout", "-q", "-b", "feat/login"]);
    std::fs::write(repo.join("src/login.rs"), "pub fn login() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "Add the login"]);
    git(&repo, &["checkout", "-q", "main"]);
    std::fs::write(repo.join("README.md"), "# Atlas\n\nMore.\n").unwrap();
    std::fs::write(repo.join("notes.txt"), "a new file\n").unwrap();
    git(&repo, &["mv", "src/main.rs", "src/app.rs"]);
    let CommandResult::ProjectCreated { project_id, .. } = client.execute(envelope(create("Atlas", &repo), None)).await.unwrap().result else { panic!() };

    let overview = client.git_overview(project_id).await.unwrap();
    assert!(overview.repository);
    assert_eq!(overview.branch.as_deref(), Some("main"));
    let changes: Vec<(&str, &str, bool)> = overview.changes.iter().map(|c| (c.path.as_str(), c.status.as_str(), c.staged)).collect();
    assert_eq!(changes, [("src/app.rs", "renamed", true), ("README.md", "modified", false), ("notes.txt", "untracked", false)]);
    assert_eq!(overview.changes[0].from.as_deref(), Some("src/main.rs"));
    assert_eq!((overview.changes[1].added, overview.changes[1].removed), (Some(2), Some(0)));
    let branches: Vec<(&str, bool, &str)> = overview.branches.iter().map(|b| (b.name.as_str(), b.current, b.subject.as_str())).collect();
    assert!(branches.contains(&("main", true, "Say hi")) && branches.contains(&("feat/login", false, "Add the login")), "{branches:?}");

    // The history where the folder is, of a branch, of one file, and in pages.
    let subjects = |log: aurelius_protocol::GitLog| log.commits.into_iter().map(|c| c.subject).collect::<Vec<_>>();
    assert_eq!(subjects(client.git_log(project_id, None, None, 0, 10).await.unwrap()), ["Say hi", "Start the project"]);
    assert_eq!(subjects(client.git_log(project_id, Some("feat/login"), None, 0, 10).await.unwrap()), ["Add the login", "Say hi", "Start the project"]);
    assert_eq!(subjects(client.git_log(project_id, None, Some("README.md"), 0, 10).await.unwrap()), ["Start the project"]);
    let first_page = client.git_log(project_id, Some("feat/login"), None, 0, 2).await.unwrap();
    assert!(first_page.has_more && first_page.commits.len() == 2);
    let rest = client.git_log(project_id, Some("feat/login"), None, 2, 2).await.unwrap();
    assert!(!rest.has_more && subjects(rest) == ["Start the project"]);
    let head = &client.git_log(project_id, None, None, 0, 1).await.unwrap().commits[0];
    assert_eq!((head.author.as_str(), head.email.as_str(), head.parents.len()), ("Forge", "forge@example.com", 1));
    assert!(head.refs.iter().any(|r| r == "main"), "{:?}", head.refs);

    // A commit whole, and its diff; the first commit is compared with nothing.
    let detail = client.git_commit(project_id, &head.id[..8]).await.unwrap();
    assert_eq!(detail.commit.id, head.id);
    assert_eq!(detail.files.iter().map(|f| (f.path.as_str(), f.status.as_str(), f.added, f.removed)).collect::<Vec<_>>(), [("src/main.rs", "modified", Some(3), Some(1))]);
    let diff = client.git_diff(project_id, Some(&head.id), None).await.unwrap();
    assert!(diff.text.contains("+    println!(\"hi\");") && diff.text.contains("-fn main() {}") && !diff.truncated, "{}", diff.text);
    let first = client.git_log(project_id, None, None, 1, 1).await.unwrap().commits.remove(0);
    let detail = client.git_commit(project_id, &first.id).await.unwrap();
    assert_eq!(detail.body, "The first files.");
    assert_eq!(detail.files.iter().map(|f| (f.path.as_str(), f.status.as_str())).collect::<Vec<_>>(), [("README.md", "added"), ("src/main.rs", "added")]);

    // The work folder against its last commit: the edit, the rename and the new file; one file alone.
    let working = client.git_diff(project_id, None, None).await.unwrap();
    assert!(working.text.contains("+More.") && working.text.contains("rename to src/app.rs") && working.text.contains("+a new file"), "{}", working.text);
    let one = client.git_diff(project_id, None, Some("notes.txt")).await.unwrap();
    assert!(one.text.contains("+a new file") && !one.text.contains("More."), "{}", one.text);

    // Nothing a client names is an option, a range or a way out of the repository.
    assert_eq!(code(client.git_log(project_id, Some("--output=/tmp/x"), None, 0, 10).await), ErrorCode::InvalidRequest);
    assert_eq!(code(client.git_log(project_id, Some("main..feat/login"), None, 0, 10).await), ErrorCode::InvalidRequest);
    assert_eq!(code(client.git_diff(project_id, None, Some("../plain/secret")).await), ErrorCode::InvalidRequest);
    assert_eq!(code(client.git_commit(project_id, "HEAD").await), ErrorCode::InvalidRequest);
    assert_eq!(code(client.git_commit(project_id, "deadbeef").await), ErrorCode::NotFound);
    assert_eq!(code(client.git_log(project_id, Some("no-such-branch"), None, 0, 10).await), ErrorCode::NotFound);

    // Reading changed nothing: the same status, and no file of ours in the folder.
    let after = client.git_overview(project_id).await.unwrap();
    assert_eq!(after.changes, overview.changes);
    stop(dir.path(), daemon).await;
}
