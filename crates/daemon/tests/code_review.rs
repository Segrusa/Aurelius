//! Reviewing a branch in the Code tab (ticket #84, feature `code_review`) through the
//! daemon's API: the diff of a real branch, a line comment that stays its writer's until
//! the review is finished, a reply, resolving, viewed files, the event other devices hear,
//! and the conflict check in `code.get`.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::{Client, ClientError, envelope};
use aurelius_domain::code::{CodeDiff, CodeFileStatus, CodeView};
use aurelius_domain::code_review::{CodeLineSide, CodeReviewView, CodeVerdict, ReplyCodeThread, ResolveCodeThread, SetCodeViewed, StartCodeThread, SubmitCodeReview};
use aurelius_domain::project::CreateProject;
use aurelius_domain::{Command, CommandResult, ErrorCode, EventKind, ProjectId};
use aurelius_protocol::{Method, ResponseBody};
use std::path::Path;
use std::process::Command as Process;
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

async fn project(client: &Client, folder: &Path) -> ProjectId {
    let made = client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: Some(folder.display().to_string()) }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = made.result else { panic!("{:?}", made.result) };
    project_id
}

async fn review(client: &Client, project_id: ProjectId, head: &str) -> CodeReviewView {
    match client.call(Method::CodeReview { project_id, branch: "feat/ticket-9".into(), head: Some(head.into()) }).await.unwrap() {
        ResponseBody::CodeReview(view) => view,
        other => panic!("{other:?}"),
    }
}

async fn run(client: &Client, command: Command) -> Result<CommandResult, ClientError> {
    client.execute(envelope(command, None)).await.map(|r| r.result)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_branch_is_reviewed_line_by_line_and_the_review_reaches_the_others_once_finished() {
    let dir = tempfile::tempdir().unwrap();
    let _daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;

    let repo = dir.path().join("atlas");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet", "--initial-branch=main"]);
    git(&repo, &["config", "user.name", "Test"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    commit(&repo, "app.rs", "fn main() {}\n", "base");
    git(&repo, &["switch", "--quiet", "-c", "feat/ticket-9"]);
    commit(&repo, "app.rs", "fn main() {\n    run();\n}\n", "run it");
    git(&repo, &["switch", "--quiet", "main"]);
    commit(&repo, "app.rs", "fn main() { start(); }\n", "main moves on");
    let project_id = project(&alex, &repo).await;

    // The diff and the conflict check.
    let diff: CodeDiff = match alex.call(Method::CodeDiff { project_id, branch: "feat/ticket-9".into() }).await.unwrap() {
        ResponseBody::CodeDiff(d) => d,
        other => panic!("{other:?}"),
    };
    assert_eq!(diff.files.len(), 1);
    assert_eq!((diff.files[0].path.as_str(), diff.files[0].status), ("app.rs", CodeFileStatus::Modified));
    assert!(diff.files[0].hunks[0].lines.contains(&"+    run();".to_owned()), "{:?}", diff.files[0].hunks);
    let view: CodeView = match alex.call(Method::CodeGet { project_id }).await.unwrap() {
        ResponseBody::Code(v) => v,
        other => panic!("{other:?}"),
    };
    let branch = view.branches.iter().find(|b| b.name == "feat/ticket-9").unwrap();
    assert_eq!(branch.conflicts.as_deref(), Some(&["app.rs".to_owned()][..]));
    let unknown = alex.call(Method::CodeDiff { project_id, branch: "nope".into() }).await.expect_err("unknown");
    assert!(matches!(unknown, ClientError::Protocol(ref e) if e.code == ErrorCode::NotFound), "{unknown:?}");

    // Bea's other device hears of every change.
    let (bea, mut events) = connect_events_as(dir.path(), "bea").await;
    let after = bea.health().await.unwrap().last_seq;
    bea.subscribe(after).await.unwrap();

    let head = diff.head.clone();
    let start = StartCodeThread {
        project_id,
        branch: "feat/ticket-9".into(),
        path: "app.rs".into(),
        line: 2,
        side: CodeLineSide::New,
        commit: head.clone(),
        snippet: vec!["+fn main() {".into(), "+    run();".into()],
        body: "Why run and not start?".into(),
    };
    let CommandResult::CodeThreadStarted { thread_id, .. } = run(&alex, Command::StartCodeThread(start)).await.unwrap() else { panic!() };
    assert!(review(&bea, project_id, &head).await.threads.is_empty(), "pending: only Alex sees it");
    let mine = review(&alex, project_id, &head).await;
    assert!(mine.threads[0].comments[0].pending);

    // A comment without words or a summary is refused; with the pending comment it goes.
    let empty = SubmitCodeReview { project_id, branch: "feat/ticket-9".into(), head: head.clone(), verdict: CodeVerdict::Changes, body: None };
    let CommandResult::CodeReviewSubmitted { comments, .. } = run(&alex, Command::SubmitCodeReview(empty)).await.unwrap() else { panic!() };
    assert_eq!(comments, 1);
    let seen = review(&bea, project_id, &head).await;
    assert_eq!(seen.threads.len(), 1);
    assert!(!seen.threads[0].comments[0].pending);
    assert_eq!((seen.reviews[0].verdict, seen.reviews[0].username.as_str()), (CodeVerdict::Changes, "alex"));
    let nothing = SubmitCodeReview { project_id, branch: "feat/ticket-9".into(), head: head.clone(), verdict: CodeVerdict::Comment, body: Some(" ".into()) };
    let refused = run(&alex, Command::SubmitCodeReview(nothing)).await.expect_err("nothing to say");
    assert!(matches!(refused, ClientError::Protocol(ref e) if e.code == ErrorCode::Validation), "{refused:?}");

    // Bea replies and resolves; viewed marks are her own.
    run(&bea, Command::ReplyCodeThread(ReplyCodeThread { thread_id, body: "start() is main's; run() is the new name.".into() })).await.unwrap();
    let resolve = || Command::ResolveCodeThread(ResolveCodeThread { thread_id, resolved: true });
    assert!(matches!(run(&bea, resolve()).await.unwrap(), CommandResult::CodeThreadResolved { changed: true, .. }));
    assert!(matches!(run(&bea, resolve()).await.unwrap(), CommandResult::CodeThreadResolved { changed: false, .. }));
    let viewed = SetCodeViewed { project_id, branch: "feat/ticket-9".into(), head: head.clone(), path: "app.rs".into(), viewed: true };
    run(&bea, Command::SetCodeViewed(viewed)).await.unwrap();
    assert_eq!(review(&bea, project_id, &head).await.viewed, vec!["app.rs"]);
    let alexs = review(&alex, project_id, &head).await;
    assert!(alexs.viewed.is_empty());
    assert!(alexs.threads[0].resolved);
    assert_eq!(alexs.threads[0].comments.len(), 1, "Bea's reply is pending");

    let gone = run(&alex, Command::ReplyCodeThread(ReplyCodeThread { thread_id: aurelius_domain::CodeThreadId::new(), body: "?".into() })).await.expect_err("no thread");
    assert!(matches!(gone, ClientError::Protocol(ref e) if e.code == ErrorCode::NotFound), "{gone:?}");

    // Start, submit, reply, resolve and viewed: five changes, each one event.
    let mut heard = 0;
    while heard < 5 {
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv()).await.expect("an event").expect("open");
        if event.kind == EventKind::CodeReviewChanged {
            assert_eq!(event.aggregate_id, format!("{project_id}:feat/ticket-9"));
            heard += 1;
        }
    }
}
