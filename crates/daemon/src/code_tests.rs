//! The Code tab's parsers of git output (ticket #77).

use super::*;

#[test]
fn the_porcelain_worktree_list_gives_paths_heads_branches_and_gone_folders() {
    let text = "worktree /repo\nHEAD 1111\nbranch refs/heads/main\n\n\
                worktree /data/worktrees/ticket-7\nHEAD 2222\nbranch refs/heads/quick/ticket-7\n\n\
                worktree /tmp/detached\nHEAD 3333\ndetached\n\n\
                worktree /tmp/gone\nHEAD 4444\nbranch refs/heads/old\nprunable gitdir file points to non-existent location\n\n";
    let list = parse_worktrees(text);
    assert_eq!(
        list,
        vec![
            WorktreeEntry { path: "/repo".into(), head: "1111".into(), branch: Some("main".into()), prunable: false },
            WorktreeEntry { path: "/data/worktrees/ticket-7".into(), head: "2222".into(), branch: Some("quick/ticket-7".into()), prunable: false },
            WorktreeEntry { path: "/tmp/detached".into(), head: "3333".into(), branch: None, prunable: false },
            WorktreeEntry { path: "/tmp/gone".into(), head: "4444".into(), branch: Some("old".into()), prunable: true },
        ]
    );
}

#[test]
fn a_bare_entry_and_a_path_with_spaces_are_read_right() {
    let text = "worktree /srv/bare.git\nbare\n\nworktree /home/a b/wt\nHEAD 5555\nbranch refs/heads/x\n";
    assert_eq!(parse_worktrees(text), vec![WorktreeEntry { path: "/home/a b/wt".into(), head: "5555".into(), branch: Some("x".into()), prunable: false }]);
}

#[test]
fn the_status_names_each_file_once_and_a_rename_by_its_new_name() {
    let text = " M src/lib.rs\0?? new file.txt\0R  renamed.rs\0old.rs\0A  added.rs\0 D gone.rs\0";
    assert_eq!(parse_status(text), vec!["src/lib.rs", "new file.txt", "renamed.rs", "added.rs", "gone.rs"]);
    assert!(parse_status("").is_empty());
}

#[test]
fn for_each_ref_lines_give_the_branch_its_upstream_and_last_commit() {
    let text = "refs/heads/main\0aaaa\0origin/main\0behind 3\x001700000000\0Ann\0Merge feat/x (ticket #4)\n\
                refs/heads/feat/ticket-77-code\0bbbb\0\0\x001700000100\0Bob\0code: a subject, with commas\n\
                refs/heads/pushed\0cccc\0origin/pushed\0ahead 2, behind 1\x001700000200\0Cy\0\n\
                refs/heads/stale\0dddd\0origin/stale\0gone\x001700000300\0Di\0x\n";
    let refs = parse_refs(text);
    assert_eq!(refs.len(), 4);
    assert_eq!(
        refs[0],
        RefEntry {
            name: "main".into(),
            head: "aaaa".into(),
            upstream: Some("origin/main".into()),
            upstream_ahead: 0,
            upstream_behind: 3,
            at_ms: 1_700_000_000_000,
            author: "Ann".into(),
            subject: "Merge feat/x (ticket #4)".into()
        }
    );
    assert_eq!((refs[1].name.as_str(), refs[1].upstream.as_deref(), refs[1].subject.as_str()), ("feat/ticket-77-code", None, "code: a subject, with commas"));
    assert_eq!((refs[2].upstream_ahead, refs[2].upstream_behind, refs[2].subject.as_str()), (2, 1, ""));
    assert_eq!((refs[3].upstream_ahead, refs[3].upstream_behind), (0, 0), "a gone upstream counts nothing");
}

#[test]
fn log_records_give_hash_parents_time_author_and_subject() {
    let text = "h3\0h2 h1\x001700000002\0Ann\0Merge x\u{1e}\nh2\0h0\x001700000001\0Bob\0with \u{1f} odd\u{1e}\nh0\0\x001700000000\0Ann\0first\u{1e}\n";
    let log = parse_log(text);
    assert_eq!(log.len(), 3);
    assert_eq!(log[0], LogRecord { hash: "h3".into(), parents: vec!["h2".into(), "h1".into()], at_ms: 1_700_000_002_000, author: "Ann".into(), subject: "Merge x".into() });
    assert_eq!(log[1].subject, "with \u{1f} odd");
    assert!(log[2].parents.is_empty(), "the root commit has no parent");
    assert!(parse_log("").is_empty());
}

#[test]
fn a_branchs_own_commits_are_read_and_cut() {
    let long = "x".repeat(CODE_SUBJECT_MAX_CHARS + 10);
    let text = format!("h2\0second\u{1e}\nh1\0{long}\u{1e}\n");
    let lines = parse_lines(&text);
    assert_eq!(lines[0], CodeCommitLine { hash: "h2".into(), subject: "second".into() });
    assert_eq!(lines[1].subject.chars().count(), CODE_SUBJECT_MAX_CHARS);
    assert!(lines[1].subject.ends_with('…'));
}

#[test]
fn left_right_counts_are_behind_then_ahead() {
    assert_eq!(parse_left_right("3\t5\n"), Some((3, 5)));
    assert_eq!(parse_left_right(""), None);
    assert_eq!(parse_left_right("x\t1"), None);
}

#[test]
fn a_shortstat_gives_files_insertions_and_deletions() {
    assert_eq!(parse_shortstat(" 3 files changed, 10 insertions(+), 2 deletions(-)\n"), (3, 10, 2));
    assert_eq!(parse_shortstat(" 1 file changed, 1 insertion(+)"), (1, 1, 0));
    assert_eq!(parse_shortstat(" 1 file changed, 4 deletions(-)"), (1, 0, 4));
    assert_eq!(parse_shortstat(""), (0, 0, 0));
}

#[test]
fn upstream_tracking_reads_ahead_and_behind() {
    assert_eq!(parse_track("ahead 2, behind 1"), (2, 1));
    assert_eq!(parse_track("behind 7"), (0, 7));
    assert_eq!(parse_track("gone"), (0, 0));
    assert_eq!(parse_track(""), (0, 0));
}

#[test]
fn a_subject_is_cut_at_its_limit_by_characters_not_bytes() {
    let short = "é".repeat(CODE_SUBJECT_MAX_CHARS);
    assert_eq!(cut(&short), short, "exactly the limit stays whole");
    let long = "é".repeat(CODE_SUBJECT_MAX_CHARS + 1);
    let cut = cut(&long);
    assert_eq!(cut.chars().count(), CODE_SUBJECT_MAX_CHARS);
    assert!(cut.ends_with('…'));
}

// ---- The writes against real repositories in a temp folder ----

fn run(dir: &Path, args: &[&str]) -> String {
    let out = quick::git(dir, args).unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    quick::text(&out)
}

/// A repository on main with one commit ("base").
fn repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("project");
    std::fs::create_dir_all(&repo).unwrap();
    run(&repo, &["init", "--quiet", "--initial-branch=main"]);
    run(&repo, &["config", "user.name", "Test"]);
    run(&repo, &["config", "user.email", "test@example.com"]);
    run(&repo, &["config", "core.autocrlf", "false"]);
    commit(&repo, "readme.md", "base");
    (dir, repo)
}

fn commit(dir: &Path, file: &str, message: &str) -> String {
    std::fs::write(dir.join(file), format!("{message}\n")).unwrap();
    run(dir, &["add", "."]);
    run(dir, &["commit", "--quiet", "-m", message]);
    run(dir, &["rev-parse", "HEAD"])
}

/// A commit on a branch that is gone afterwards: reachable only from what the test points at it.
fn loose_commit(repo: &Path, file: &str, message: &str) -> String {
    run(repo, &["switch", "--quiet", "-c", "scratch"]);
    let sha = commit(repo, file, message);
    run(repo, &["switch", "--quiet", "main"]);
    run(repo, &["branch", "--quiet", "-D", "scratch"]);
    sha
}

fn nobody() -> Busy {
    Busy { links: HashMap::new(), loose_working: vec![] }
}

fn merge_of(branch: &str) -> HostWrite {
    HostWrite::CodeMerge { project_id: ProjectId::new(), branch: branch.into() }
}

fn is_ancestor(repo: &Path, commit: &str, of: &str) -> bool {
    quick::git(repo, &["merge-base", "--is-ancestor", commit, of]).unwrap().status.success()
}

#[test]
fn a_branch_named_like_an_option_merges_exactly_its_commit_and_never_mains_upstream() {
    let (_dir, repo) = repo();
    // main tracks origin/main, which has a commit main lacks: a bare `git merge -Xours` would merge it.
    let upstream = loose_commit(&repo, "upstream.txt", "upstream-only");
    run(&repo, &["update-ref", "refs/remotes/origin/main", &upstream]);
    run(&repo, &["config", "remote.origin.url", "/nowhere"]);
    run(&repo, &["config", "remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"]);
    run(&repo, &["config", "branch.main.remote", "origin"]);
    run(&repo, &["config", "branch.main.merge", "refs/heads/main"]);
    let work = loose_commit(&repo, "dash.txt", "the branch's work");
    run(&repo, &["update-ref", "refs/heads/-Xours", &work]);

    perform(&repo, merge_of("-Xours"), &nobody()).unwrap();
    assert_eq!(run(&repo, &["rev-parse", "main^2"]), work, "the branch's commit is merged");
    assert!(!is_ancestor(&repo, &upstream, "main"), "origin/main never reaches main");
    assert_eq!(run(&repo, &["log", "-1", "--format=%s", "main"]), "Merge -Xours", "the message keeps the short name");
}

#[test]
fn a_tag_named_like_the_branch_does_not_shadow_it_in_a_merge() {
    let (_dir, repo) = repo();
    let other = loose_commit(&repo, "other.txt", "NOT the branch");
    run(&repo, &["tag", "feat/x", &other]);
    run(&repo, &["switch", "--quiet", "-c", "feat/x"]);
    let work = commit(&repo, "x.txt", "the branch");
    run(&repo, &["switch", "--quiet", "main"]);

    perform(&repo, merge_of("feat/x"), &nobody()).unwrap();
    assert_eq!(run(&repo, &["rev-parse", "main^2"]), work);
    assert!(!is_ancestor(&repo, &other, "main"), "the tag's commit stays out");
}

#[test]
fn a_branch_named_like_an_option_can_be_deleted_with_force() {
    let (_dir, repo) = repo();
    let work = loose_commit(&repo, "x.txt", "unmerged");
    run(&repo, &["update-ref", "refs/heads/-x", &work]);
    let delete = HostWrite::CodeDeleteBranch { project_id: ProjectId::new(), branch: "-x".into(), force: true };
    perform(&repo, delete, &nobody()).unwrap();
    assert!(!quick::git(&repo, &["rev-parse", "--verify", "--quiet", "refs/heads/-x"]).unwrap().status.success(), "the branch is gone");
}

#[test]
fn a_fetch_from_a_remote_that_never_answers_ends_at_its_deadline_and_leaves_the_git_lock_free() {
    let (_dir, repo) = repo();
    // The kernel accepts the connection; nobody ever answers it.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    run(&repo, &["remote", "add", "origin", &format!("git://127.0.0.1:{}/repo.git", silent.local_addr().unwrap().port())]);
    let deadline = Duration::from_secs(2);
    let fetching = {
        let repo = repo.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            let result = apply(&repo, HostWrite::CodeFetch { project_id: ProjectId::new() }, &nobody(), deadline);
            (result, started.elapsed())
        })
    };
    std::thread::sleep(Duration::from_millis(300));
    let (got, lock_taken) = mpsc::channel();
    std::thread::spawn(move || {
        let _git = quick::lock();
        let _ = got.send(());
    });
    assert!(lock_taken.recv_timeout(Duration::from_secs(1)).is_ok(), "a quick task's git work goes on while the fetch waits");
    let (result, took) = fetching.join().unwrap();
    let e = result.expect_err("the fetch is stopped");
    assert_eq!(e.code, ErrorCode::Validation);
    assert_eq!(e.message, "The fetch did not finish within 2 s.");
    assert!(took < deadline + Duration::from_secs(2), "stopped at its deadline, took {took:?}");
    drop(silent);
}

#[test]
fn reading_the_view_never_writes_the_index_of_a_worktree_that_needs_a_refresh() {
    let (dir, repo) = repo();
    let worktree = dir.path().join("wt");
    run(&repo, &["worktree", "add", "--quiet", "-b", "feat/x", &worktree.to_string_lossy(), "main"]);
    let index = PathBuf::from(run(&worktree, &["rev-parse", "--path-format=absolute", "--git-path", "index"]));
    // Same content, another time: git sees a file that may have changed and wants to refresh its index.
    let file = std::fs::File::options().write(true).open(worktree.join("readme.md")).unwrap();
    file.set_modified(std::time::SystemTime::now() - Duration::from_secs(3600)).unwrap();
    let before = std::fs::read(&index).unwrap();

    let view = read_view(ProjectId::new(), &repo).unwrap();
    let listed = view.worktrees.iter().find(|w| w.branch.as_deref() == Some("feat/x")).unwrap();
    assert_eq!(listed.changed_count, 0, "an unchanged file is no change");
    assert!(std::fs::read(&index).unwrap() == before, "the index is never written, so index.lock is never taken");
    assert!(!index.with_extension("lock").exists());
}

#[test]
fn a_sessions_worktree_is_found_also_when_its_path_is_written_differently() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("wt");
    std::fs::create_dir_all(&real).unwrap();
    let real = real.to_string_lossy().into_owned();
    let sessions = vec![SessionWorktree { path: format!("{real}/"), working: true }];
    assert!(working_at(&sessions, &real), "a trailing slash");
    assert!(!working_at(&sessions, &dir.path().join("other").to_string_lossy()));
    #[cfg(unix)]
    {
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let sessions = vec![SessionWorktree { path: link.to_string_lossy().into_owned(), working: true }];
        assert!(working_at(&sessions, &real), "a symlinked data root");
    }
    let idle = vec![SessionWorktree { path: real.clone(), working: false }, SessionWorktree { path: real.clone(), working: true }];
    assert!(!working_at(&idle, &real), "the newest session of the folder decides");
}

#[test]
fn a_git_line_that_reaches_the_client_is_redacted() {
    let token = "t77f2-vault-value-in-a-remote-url";
    aurelius_runtime::redact::add_to_set("code-tests", aurelius_runtime::redact::Secret { name: "remote".into(), label: "Remote".into(), value: token.to_owned().into() });
    let (dir, repo) = repo();
    let remote = dir.path().join(token);
    run(&repo, &["remote", "add", "origin", &remote.to_string_lossy()]);
    let e = apply(&repo, HostWrite::CodeFetch { project_id: ProjectId::new() }, &nobody(), Duration::from_secs(30)).expect_err("no repository there");
    assert!(e.message.starts_with("The fetch failed"), "{}", e.message);
    assert!(!e.message.contains(token), "{}", e.message);
    let shown = problem(ProjectId::new(), String::new(), format!("fatal: {token}"));
    assert!(!shown.problem.unwrap_or_default().contains(token));
}

#[cfg(unix)]
#[test]
fn a_fetch_uses_the_repositorys_own_ssh_command() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = repo();
    let marker = dir.path().join("own-ssh-ran");
    let script = dir.path().join("own-ssh.sh");
    std::fs::write(&script, format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    run(&repo, &["remote", "add", "origin", "ssh://git@example.invalid/x.git"]);
    run(&repo, &["config", "core.sshCommand", &script.to_string_lossy()]);
    let result = apply(&repo, HostWrite::CodeFetch { project_id: ProjectId::new() }, &nobody(), Duration::from_secs(20));
    assert_eq!(result.expect_err("the script fails the fetch").code, ErrorCode::Validation);
    assert!(marker.exists(), "the repository's core.sshCommand ran, not ours");
}

/// The `git` processes whose working folder is `repo` (Linux: read from /proc).
#[cfg(target_os = "linux")]
fn gits_in(repo: &Path) -> Vec<String> {
    let repo = std::fs::canonicalize(repo).unwrap();
    std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| std::fs::read_link(e.path().join("cwd")).is_ok_and(|cwd| cwd == repo))
        .filter_map(|e| std::fs::read(e.path().join("cmdline")).ok())
        .map(|c| String::from_utf8_lossy(&c).replace('\0', " "))
        .filter(|c| c.contains("fetch"))
        .collect()
}

#[cfg(target_os = "linux")]
#[test]
fn a_stopped_fetch_of_two_silent_remotes_leaves_no_git_running() {
    let (_dir, repo) = repo();
    let (a, b) = (std::net::TcpListener::bind("127.0.0.1:0").unwrap(), std::net::TcpListener::bind("127.0.0.1:0").unwrap());
    run(&repo, &["remote", "add", "a", &format!("git://127.0.0.1:{}/repo.git", a.local_addr().unwrap().port())]);
    run(&repo, &["remote", "add", "b", &format!("git://127.0.0.1:{}/repo.git", b.local_addr().unwrap().port())]);
    let started = Instant::now();
    let e = apply(&repo, HostWrite::CodeFetch { project_id: ProjectId::new() }, &nobody(), Duration::from_secs(2)).expect_err("stopped");
    assert_eq!(e.message, "The fetch did not finish within 2 s.");
    assert!(started.elapsed() < Duration::from_secs(4), "one deadline for every remote, took {:?}", started.elapsed());
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(gits_in(&repo), Vec::<String>::new(), "no fetch left running after the stop");
    drop((a, b));
}

#[test]
fn an_open_branch_says_whether_its_ticket_or_its_work_session_keeps_it() {
    let ticket = aurelius_domain::code::CodeTicket { ticket_id: aurelius_domain::TicketId::new(), number: 7, title: "T".into() };
    let mut links = HashMap::new();
    links.insert("feat/ticket-7-x".to_owned(), BranchLink { ticket: Some(ticket), open: true, ..BranchLink::default() });
    links.insert("session/1a2b3c4d".to_owned(), BranchLink { open: true, ..BranchLink::default() });
    links.insert("feat/closed".to_owned(), BranchLink::default());
    let busy = Busy { links, loose_working: vec![] };
    assert_eq!(busy.open_why("feat/ticket-7-x"), Some(STILL_OPEN));
    assert_eq!(busy.open_why("session/1a2b3c4d"), Some(SESSION_OPEN));
    assert_eq!(busy.open_why("feat/closed"), None);
    assert_eq!(busy.open_why("unknown"), None);
}

#[test]
fn a_failing_remote_does_not_stop_the_next_one_and_a_skipped_one_is_left_out() {
    let (_good_dir, good) = repo();
    let (_dir, repo) = repo();
    run(&repo, &["remote", "add", "a", "/nonexistent/dead"]);
    run(&repo, &["remote", "add", "b", &good.to_string_lossy()]);
    run(&repo, &["remote", "add", "c", &good.to_string_lossy()]);
    run(&repo, &["config", "remote.c.skipFetchAll", "true"]);
    // A key without a value and a nonzero number are true for git too.
    run(&repo, &["remote", "add", "d", &good.to_string_lossy()]);
    run(&repo, &["remote", "add", "e", &good.to_string_lossy()]);
    std::fs::OpenOptions::new().append(true).open(repo.join(".git/config")).and_then(|mut f| std::io::Write::write_all(&mut f, b"[remote \"d\"]\n\tskipFetchAll\n[remote \"e\"]\n\tskipFetchAll = 2\n")).unwrap();
    let e = apply(&repo, HostWrite::CodeFetch { project_id: ProjectId::new() }, &nobody(), Duration::from_secs(20)).expect_err("a failed");
    assert!(e.message.starts_with("The fetch failed for a: "), "{}", e.message);
    assert!(!e.message.contains("b:"), "b was fetched: {}", e.message);
    assert!(!run(&repo, &["for-each-ref", "refs/remotes/b"]).is_empty(), "b fetched after a failed");
    assert!(run(&repo, &["for-each-ref", "refs/remotes/c"]).is_empty(), "c skips fetch --all");
    assert!(run(&repo, &["for-each-ref", "refs/remotes/d", "refs/remotes/e"]).is_empty(), "so do a bare key and a 2");
}

#[test]
fn skip_fetch_all_is_read_from_gits_config_lines() {
    let config = "remote.c.skipfetchall true\nremote.d.skipfetchall false\nremote.up-stream.skipfetchall yes\n";
    assert_eq!(skipped_remotes(config), vec!["c", "up-stream"]);
    assert!(skipped_remotes("").is_empty());
}

// ---- Reviewing a branch (ticket #84) ----

#[test]
fn a_unified_diff_gives_each_file_its_status_hunks_and_counts() {
    let text = "diff --git a/src/a.rs b/src/a.rs\nindex 1..2 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n\
                @@ -3,4 +3,5 @@ fn main() {\n keep\n-old\n+new\n+more\n keep\n\\ No newline at end of file\n\
                diff --git a/new.txt b/new.txt\nnew file mode 100644\nindex 0..3\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+hello\n\
                diff --git a/gone.txt b/gone.txt\ndeleted file mode 100644\n--- a/gone.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-bye\n\
                diff --git a/old name.md b/new name.md\nsimilarity index 90%\nrename from old name.md\nrename to new name.md\n\
                diff --git a/logo.png b/logo.png\nBinary files a/logo.png and b/logo.png differ\n";
    let files = parse_diff(text);
    let summary: Vec<_> = files.iter().map(|f| (f.path.as_str(), f.status, f.additions, f.deletions, f.hunks.len(), f.binary)).collect();
    assert_eq!(
        summary,
        vec![
            ("src/a.rs", CodeFileStatus::Modified, 2, 1, 1, false),
            ("new.txt", CodeFileStatus::Added, 1, 0, 1, false),
            ("gone.txt", CodeFileStatus::Deleted, 0, 1, 1, false),
            ("new name.md", CodeFileStatus::Renamed, 0, 0, 0, false),
            ("logo.png", CodeFileStatus::Modified, 0, 0, 0, true),
        ]
    );
    let hunk = &files[0].hunks[0];
    assert_eq!((hunk.old_start, hunk.new_start), (3, 3));
    assert_eq!(hunk.lines, vec![" keep", "-old", "+new", "+more", " keep"], "no-newline marker is not a line");
    assert_eq!(files[3].old_path.as_deref(), Some("old name.md"));
}

#[test]
fn a_line_inside_a_hunk_that_looks_like_a_header_stays_a_line() {
    let text = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n---- a/y\n+++++ b/y\n";
    let files = parse_diff(text);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "x");
    assert_eq!(files[0].hunks[0].lines, vec!["---- a/y", "+++++ b/y"]);
}

#[test]
fn a_hunk_header_without_counts_starts_at_its_line() {
    assert_eq!(parse_hunk_header("@@ -0,0 +1 @@"), (0, 1));
    assert_eq!(parse_hunk_header("@@ -12,7 +14,9 @@ fn x()"), (12, 14));
}

#[test]
fn a_long_diff_is_cut_per_file_and_in_total() {
    let file = |n: usize| CodeDiffFile {
        path: format!("f{n}"),
        old_path: None,
        status: CodeFileStatus::Added,
        additions: 0,
        deletions: 0,
        binary: false,
        uncommitted: false,
        hunks: vec![CodeHunk { header: "@@".into(), old_start: 0, new_start: 1, lines: vec!["+x".into(); CODE_DIFF_FILE_LINES_MAX + 5] }],
        truncated: false,
    };
    let mut files: Vec<_> = (0..CODE_DIFF_FILES_MAX + 2).map(file).collect();
    assert!(limit_diff(&mut files));
    assert_eq!(files.len(), CODE_DIFF_FILES_MAX);
    assert_eq!(files[0].hunks[0].lines.len(), CODE_DIFF_FILE_LINES_MAX);
    let total: usize = files.iter().flat_map(|f| &f.hunks).map(|h| h.lines.len()).sum();
    assert_eq!(total, CODE_DIFF_LINES_MAX);
    assert!(files.last().is_some_and(|f| f.truncated && f.hunks.is_empty()));

    let mut small = vec![file(0)];
    small[0].hunks[0].lines.truncate(3);
    assert!(!limit_diff(&mut small), "a small diff is whole");
}

#[test]
fn merge_tree_names_each_conflicting_file_once_after_its_tree() {
    assert_eq!(parse_conflicts("36a3690\nsrc/a.rs\nsrc/a.rs\nREADME.md\n"), vec!["src/a.rs", "README.md"]);
    assert!(parse_conflicts("36a3690\n").is_empty());
}

#[test]
fn a_very_long_diff_line_is_cut_with_an_ellipsis() {
    let line = format!("+{}", "x".repeat(CODE_DIFF_LINE_MAX_CHARS * 2));
    let cut = cut_line(&line);
    assert_eq!(cut.chars().count(), CODE_DIFF_LINE_MAX_CHARS);
    assert!(cut.ends_with('…'));
}

#[test]
fn a_branchs_diff_holds_what_it_committed_and_what_its_worktree_did_not() {
    let (dir, repo) = repo();
    run(&repo, &["switch", "--quiet", "-c", "feat/x"]);
    commit(&repo, "readme.md", "changed on the branch");
    commit(&repo, "added.txt", "new file");
    run(&repo, &["switch", "--quiet", "main"]);
    let wt = dir.path().join("wt");
    run(&repo, &["worktree", "add", "--quiet", &wt.to_string_lossy(), "feat/x"]);
    std::fs::write(wt.join("added.txt"), "new file\nnot committed\n").unwrap();

    let diff = read_diff(ProjectId::new(), &repo, "feat/x").unwrap();
    let files: Vec<_> = diff.files.iter().map(|f| (f.path.as_str(), f.status, f.uncommitted)).collect();
    assert_eq!(files, vec![("added.txt", CodeFileStatus::Added, false), ("readme.md", CodeFileStatus::Modified, false), ("added.txt", CodeFileStatus::Modified, true)]);
    assert_eq!(diff.head, run(&repo, &["rev-parse", "refs/heads/feat/x"]));
    assert_eq!(diff.base, run(&repo, &["rev-parse", "refs/heads/main"]));
    assert_eq!(diff.files[1].hunks[0].lines, vec!["-base", "+changed on the branch"]);
    assert!(!diff.truncated);

    let unknown = read_diff(ProjectId::new(), &repo, "nope").expect_err("unknown branch");
    assert_eq!(unknown.code, ErrorCode::NotFound);
}

#[test]
fn the_view_says_which_files_a_merge_into_main_would_conflict_on() {
    let (_dir, repo) = repo();
    run(&repo, &["switch", "--quiet", "-c", "clash"]);
    commit(&repo, "readme.md", "the branch's words");
    run(&repo, &["switch", "--quiet", "-c", "clean", "main"]);
    commit(&repo, "other.txt", "elsewhere");
    run(&repo, &["switch", "--quiet", "-c", "fresh", "main"]);
    commit(&repo, "fresh.txt", "ahead only");
    run(&repo, &["switch", "--quiet", "main"]);
    let before = run(&repo, &["rev-parse", "HEAD"]);
    let fresh = read_view(ProjectId::new(), &repo).unwrap();
    assert_eq!(fresh.branches.iter().find(|b| b.name == "fresh").unwrap().conflicts, Some(vec![]), "up to date with main: nothing to check");
    commit(&repo, "readme.md", "main's words");

    let view = read_view(ProjectId::new(), &repo).unwrap();
    let of = |name: &str| view.branches.iter().find(|b| b.name == name).unwrap().conflicts.clone();
    assert_eq!(of("clash"), Some(vec!["readme.md".to_owned()]));
    assert_eq!(of("clean"), Some(vec![]));
    assert_eq!(of("main"), None, "main is not merged into itself");
    // merge-tree touches neither main nor the worktree.
    assert_ne!(run(&repo, &["rev-parse", "HEAD"]), before);
    assert_eq!(run(&repo, &["status", "--porcelain"]), "");
}
