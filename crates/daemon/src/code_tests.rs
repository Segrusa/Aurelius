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
