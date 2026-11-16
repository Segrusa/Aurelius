//! Quick tasks' git work against real repositories in a temp folder (ticket #53, ST-05).

use super::*;
use std::fs;
use std::path::PathBuf;

const BRANCH: &str = "quick/ticket-7";

fn run(dir: &Path, args: &[&str]) -> String {
    let out = git(dir, args).unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    text(&out)
}

/// A repository on main with one commit, and the place of its quick task's worktree.
fn repo() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("project");
    fs::create_dir_all(&repo).unwrap();
    run(&repo, &["init", "--quiet", "--initial-branch=main"]);
    run(&repo, &["config", "user.name", "Test"]);
    run(&repo, &["config", "user.email", "test@example.com"]);
    // Windows runners set core.autocrlf=true globally: files git writes back
    // (e.g. after `merge --abort`) would get CRLF and differ from what we wrote.
    run(&repo, &["config", "core.autocrlf", "false"]);
    fs::write(repo.join("readme.md"), "hello\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "--quiet", "-m", "first"]);
    let worktree = dir.path().join("data/worktrees/ticket-7");
    (dir, repo, worktree)
}

/// The employee's commit on the quick task's branch, in its worktree.
fn commit_in(worktree: &Path, file: &str, body: &str) {
    fs::write(worktree.join(file), body).unwrap();
    run(worktree, &["add", "."]);
    run(worktree, &["-c", "user.name=Emp", "-c", "user.email=emp@example.com", "commit", "--quiet", "-m", "work"]);
}

fn head(repo: &Path) -> String {
    run(repo, &["rev-parse", "HEAD"])
}

#[test]
fn a_worktree_is_made_on_a_new_branch_from_main_and_again_after_it_is_gone() {
    let (_dir, repo, worktree) = repo();
    ensure_worktree(&repo, BRANCH, &worktree).unwrap();
    assert_eq!(run(&worktree, &["symbolic-ref", "--short", "HEAD"]), BRANCH);
    assert_eq!(head(&worktree), head(&repo), "from main as it is");
    ensure_worktree(&repo, BRANCH, &worktree).unwrap();
    commit_in(&worktree, "a.txt", "a\n");
    let work = head(&worktree);
    cleanup_worktree_only(&repo, &worktree);
    ensure_worktree(&repo, BRANCH, &worktree).unwrap();
    assert_eq!(head(&worktree), work, "an existing branch keeps its commits");
}

fn cleanup_worktree_only(repo: &Path, worktree: &Path) {
    run(repo, &["worktree", "remove", "--force", &worktree.to_string_lossy()]);
}

#[test]
fn an_approved_branch_is_merged_with_no_ff_and_the_ticket_number() {
    let (_dir, repo, worktree) = repo();
    ensure_worktree(&repo, BRANCH, &worktree).unwrap();
    commit_in(&worktree, "a.txt", "a\n");
    let MergeOutcome::Merged(commit) = merge(&repo, BRANCH, "Merge quick/ticket-7: Fix (ticket #7)") else { panic!("merged") };
    assert_eq!(commit, head(&repo));
    assert_eq!(run(&repo, &["rev-list", "--count", "--merges", "HEAD"]), "1", "a merge commit (--no-ff)");
    assert!(run(&repo, &["log", "-1", "--format=%s"]).contains("ticket #7"));
    assert!(repo.join("a.txt").exists());
    cleanup(&repo, BRANCH, &worktree).unwrap();
    assert!(!worktree.exists() && !branch_exists(&repo, BRANCH), "worktree and branch are gone");
}

#[test]
fn a_conflict_is_aborted_and_main_stays_as_it_was() {
    let (_dir, repo, worktree) = repo();
    ensure_worktree(&repo, BRANCH, &worktree).unwrap();
    commit_in(&worktree, "readme.md", "from the branch\n");
    fs::write(repo.join("readme.md"), "from main\n").unwrap();
    run(&repo, &["commit", "--quiet", "-am", "main moved on"]);
    let before = head(&repo);
    let outcome = merge(&repo, BRANCH, "m");
    assert_eq!(outcome, MergeOutcome::Failed { reason: "a merge conflict".into(), files: vec!["readme.md".into()] });
    assert_eq!(head(&repo), before);
    assert!(!repo.join(".git/MERGE_HEAD").exists(), "the merge was aborted");
    assert_eq!(run(&repo, &["status", "--porcelain"]), "", "main is clean");
    assert_eq!(fs::read_to_string(repo.join("readme.md")).unwrap(), "from main\n");
}

#[test]
fn a_main_with_uncommitted_changes_is_not_merged_into() {
    let (_dir, repo, worktree) = repo();
    ensure_worktree(&repo, BRANCH, &worktree).unwrap();
    commit_in(&worktree, "a.txt", "a\n");
    fs::write(repo.join("readme.md"), "edited\n").unwrap();
    let before = head(&repo);
    let MergeOutcome::Failed { reason, files } = merge(&repo, BRANCH, "m") else { panic!("refused") };
    assert!(reason.contains("not clean"), "{reason}");
    assert_eq!(files, vec!["readme.md".to_owned()]);
    assert_eq!(head(&repo), before);
    assert_eq!(fs::read_to_string(repo.join("readme.md")).unwrap(), "edited\n", "the board member's change stays");
}

#[test]
fn a_branch_without_commits_is_a_plain_done() {
    let (_dir, repo, worktree) = repo();
    ensure_worktree(&repo, BRANCH, &worktree).unwrap();
    let before = head(&repo);
    assert_eq!(merge(&repo, BRANCH, "m"), MergeOutcome::NoCommits);
    assert_eq!(head(&repo), before);
}

#[test]
fn a_missing_branch_is_not_merged() {
    let (_dir, repo, _worktree) = repo();
    let before = head(&repo);
    let MergeOutcome::Failed { reason, .. } = merge(&repo, BRANCH, "m") else { panic!("refused") };
    assert!(reason.contains("does not exist"), "{reason}");
    assert_eq!(head(&repo), before);
}

#[test]
fn a_project_folder_on_another_branch_is_not_merged_into() {
    let (_dir, repo, worktree) = repo();
    ensure_worktree(&repo, BRANCH, &worktree).unwrap();
    commit_in(&worktree, "a.txt", "a\n");
    run(&repo, &["checkout", "--quiet", "-b", "feature"]);
    let MergeOutcome::Failed { reason, .. } = merge(&repo, BRANCH, "m") else { panic!("refused") };
    assert!(reason.contains("feature"), "{reason}");
    assert_eq!(run(&repo, &["rev-parse", "main"]), run(&repo, &["rev-parse", "feature"]), "main unchanged");
}

#[test]
fn a_cancel_removes_the_worktree_and_the_branch_with_its_unmerged_work() {
    let (_dir, repo, worktree) = repo();
    ensure_worktree(&repo, BRANCH, &worktree).unwrap();
    commit_in(&worktree, "a.txt", "a\n");
    fs::write(worktree.join("b.txt"), "not committed\n").unwrap();
    let before = head(&repo);
    cleanup(&repo, BRANCH, &worktree).unwrap();
    assert!(!worktree.exists() && !branch_exists(&repo, BRANCH));
    assert_eq!(head(&repo), before, "main is untouched");
    cleanup(&repo, BRANCH, &worktree).unwrap();
}

#[test]
fn a_missing_main_says_why_in_words_a_board_member_can_act_on() {
    // Not a repository at all (seen on Windows: "no main branch" said nothing).
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain");
    fs::create_dir_all(&plain).unwrap();
    let worktree = dir.path().join("data/worktrees/session-1");
    let why = ensure_worktree(&plain, "session/1", &worktree).unwrap_err();
    assert!(why.contains("is not a git repository"), "{why}");
    // A repository without a commit yet.
    let empty = dir.path().join("empty");
    fs::create_dir_all(&empty).unwrap();
    run(&empty, &["init", "--quiet", "--initial-branch=main"]);
    let why = ensure_worktree(&empty, "session/1", &worktree).unwrap_err();
    assert!(why.contains("no commit yet"), "{why}");
    // A repository whose branch has another name: it is named.
    let (_dir, repo, worktree) = repo();
    run(&repo, &["branch", "-m", "main", "develop"]);
    let why = ensure_worktree(&repo, BRANCH, &worktree).unwrap_err();
    assert!(why.contains("no main or master branch") && why.contains("develop"), "{why}");
    assert!(matches!(merge(&repo, BRANCH, "m"), MergeOutcome::Failed { reason, .. } if reason.contains("develop")));
}

#[test]
fn a_sessions_worktree_is_made_on_its_own_branch() {
    let (_dir, repo, _) = repo();
    let worktree = repo.parent().unwrap().join("data/worktrees/session-1a2b3c4d");
    ensure_worktree(&repo, "session/1a2b3c4d", &worktree).unwrap();
    assert_eq!(run(&worktree, &["symbolic-ref", "--short", "HEAD"]), "session/1a2b3c4d");
}
