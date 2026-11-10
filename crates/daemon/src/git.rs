//! `git.overview`, `git.log`, `git.commit` and `git.diff` (feature `project_git`):
//! what a project's work folder looks like in git, for a board member who checks
//! what employees did without a terminal on the server.
//!
//! Read-only. Every call is `git` itself, run in the repository of the project's
//! folder with arguments this file builds: no command here writes a ref, the index
//! or the work tree (`--no-optional-locks` keeps even `status` from refreshing the
//! index). What a client names (a revision, a commit id, a path) is checked first
//! and always stands after `--end-of-options` or `--`, so it is never read as an
//! option. External diff drivers, text conversion and the file system monitor of
//! the repository's own configuration are off: a repository cannot make this run
//! a program of its choosing.

use crate::Shared;
use aurelius_domain::ProjectId;
use aurelius_protocol::{
    ErrorCode, GIT_DIFF_MAX_BYTES, GIT_FILES_MAX, GIT_LOG_MAX, GitBranch, GitChange, GitCommit, GitCommitDetail, GitDiff, GitLog, GitOverview, ProtocolError,
};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

/// How long one `git` call may take; a repository that needs longer answers with an error.
const TIMEOUT: Duration = Duration::from_secs(30);
const LOG_DEFAULT: u32 = 50;
/// Untracked files whose content `git.diff` of the work folder shows as new; more are named only.
const UNTRACKED_SHOWN: usize = 50;
/// The fields of a commit, NUL between them, and a record separator after the last.
const COMMIT_FORMAT: &str = "%H%x00%s%x00%an%x00%ae%x00%at%x00%P%x00%D%x1e";

fn refused(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::InvalidRequest, message)
}

fn not_found(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::NotFound, message)
}

/// The project's work folder on this server, when it has one that exists.
async fn folder_of(shared: &Arc<Shared>, project: ProjectId) -> Result<Option<PathBuf>, ProtocolError> {
    let store = shared.store().clone();
    let folders = tokio::task::spawn_blocking(move || store.read(|c| aurelius_storage::tasks::work_folders(c, project, None)))
        .await
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
        .map_err(|e| ProtocolError::new(ErrorCode::StorageUnavailable, e.to_string()))?;
    Ok(folders.first().and_then(|f| std::fs::canonicalize(f).ok()).filter(|d| d.is_dir()))
}

/// One `git` call in `dir`: its output when it ended with one of `ok` (0 always is).
async fn git(dir: &Path, args: &[&str], ok: &[i32]) -> Result<Vec<u8>, ProtocolError> {
    let mut command = tokio::process::Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(["--no-pager", "--no-optional-locks", "-c", "core.quotepath=false", "-c", "core.fsmonitor=false", "-c", "color.ui=false"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = match tokio::time::timeout(TIMEOUT, command.output()).await {
        Err(_) => return Err(ProtocolError::new(ErrorCode::Internal, "git took too long to answer for this repository.")),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => return Err(ProtocolError::new(ErrorCode::Internal, "git is not installed on the server.")),
        Ok(Err(e)) => return Err(ProtocolError::new(ErrorCode::Internal, format!("git could not be started: {e}"))),
        Ok(Ok(output)) => output,
    };
    let code = output.status.code().unwrap_or(-1);
    if code == 0 || ok.contains(&code) {
        return Ok(output.stdout);
    }
    let said: String = String::from_utf8_lossy(&output.stderr).trim().chars().take(300).collect();
    Err(not_found(if said.is_empty() { "git could not read this.".to_owned() } else { said }))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The top folder of the repository the project's folder is in; none: it is no repository.
async fn repository(folder: &Path) -> Option<PathBuf> {
    let top = git(folder, &["rev-parse", "--show-toplevel"], &[]).await.ok()?;
    let top = PathBuf::from(text(&top).trim_end_matches(['\r', '\n']));
    top.is_dir().then_some(top)
}

/// The repository of a project, or why there is none to read.
async fn repository_of(shared: &Arc<Shared>, project: ProjectId) -> Result<PathBuf, ProtocolError> {
    let Some(folder) = folder_of(shared, project).await? else { return Err(not_found("This project has no folder on the server.")) };
    repository(&folder).await.ok_or_else(|| not_found("This project's folder is not a git repository."))
}

/// A revision a client names: a branch, a tag or a commit id, with `~` and `^` to step back.
/// Never an option, a range or anything git would read as more than one commit.
fn checked_rev(rev: &str) -> Result<&str, ProtocolError> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-' | '@' | '~' | '^' | '+' | '#');
    if rev.is_empty() || rev.len() > 250 || rev.starts_with('-') || rev.contains("..") || !rev.chars().all(allowed) {
        return Err(refused("This is not a branch, a tag or a commit."));
    }
    Ok(rev)
}

/// A commit id: 4 to 64 hex digits.
fn checked_id(id: &str) -> Result<&str, ProtocolError> {
    if !(4..=64).contains(&id.len()) || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(refused("This is not a commit id."));
    }
    Ok(id)
}

/// A path inside the repository, relative to its top, as git prints it.
fn checked_path(path: &str) -> Result<&str, ProtocolError> {
    let inside = Path::new(path).components().all(|c| matches!(c, Component::Normal(_)));
    if path.is_empty() || path.len() > 4_000 || path.contains('\0') || !inside {
        return Err(refused("This is not a path inside the repository."));
    }
    Ok(path)
}

/// The pathspec that names exactly `path` from the repository's top, whatever is in it.
fn literal(path: &str) -> String {
    format!(":(top,literal){path}")
}

fn status_word(letter: char) -> &'static str {
    match letter {
        'A' => "added",
        'D' => "deleted",
        'R' => "renamed",
        'C' => "copied",
        'U' => "conflict",
        '?' => "untracked",
        _ => "modified",
    }
}

/// `git status --porcelain=v2 -z`: one entry per path. Staged: only when nothing of it is left unstaged.
fn parse_status(out: &[u8]) -> Vec<GitChange> {
    let mut changes = Vec::new();
    let mut fields = out.split(|b| *b == 0).map(text);
    while let Some(entry) = fields.next() {
        let change = |path: &str, from: Option<String>, status: &str, staged: bool| GitChange { path: path.to_owned(), from, status: status.to_owned(), staged, added: None, removed: None };
        let (kind, rest) = entry.split_once(' ').unwrap_or((entry.as_str(), ""));
        match kind {
            "?" => changes.push(change(rest, None, "untracked", false)),
            "u" => {
                // u XY sub m1 m2 m3 mW h1 h2 h3 path
                if let Some(path) = rest.splitn(10, ' ').nth(9) {
                    changes.push(change(path, None, "conflict", false));
                }
            }
            "1" | "2" => {
                // 1 XY sub mH mI mW hH hI path  |  2 XY sub mH mI mW hH hI Xscore path, then the old path.
                let parts: Vec<&str> = rest.splitn(if kind == "1" { 8 } else { 9 }, ' ').collect();
                let from = if kind == "2" { fields.next() } else { None };
                let (Some(xy), Some(path)) = (parts.first(), parts.last()) else { continue };
                let mut xy = xy.chars();
                let (staged, unstaged) = (xy.next().unwrap_or('.'), xy.next().unwrap_or('.'));
                let letter = if unstaged != '.' { unstaged } else { staged };
                // A rename is one in the index; a later edit of the new file is still a rename to the reader.
                let letter = if matches!(staged, 'R' | 'C') { staged } else { letter };
                changes.push(change(path, from, status_word(letter), staged != '.' && unstaged == '.'));
            }
            _ => {}
        }
    }
    changes.sort_by(|a, b| b.staged.cmp(&a.staged).then_with(|| a.path.cmp(&b.path)));
    changes
}

/// `git diff --numstat -z`: lines added and removed per path (none for a binary file).
fn parse_numstat(out: &[u8]) -> HashMap<String, (Option<u32>, Option<u32>)> {
    let mut counts = HashMap::new();
    let mut fields = out.split(|b| *b == 0).map(text);
    while let Some(entry) = fields.next() {
        let mut parts = entry.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
        // A rename has an empty path here and the old and the new one after it.
        let path = if path.is_empty() {
            let _old = fields.next();
            match fields.next() {
                Some(new) => new,
                None => continue,
            }
        } else {
            path.to_owned()
        };
        counts.insert(path, (added.parse().ok(), removed.parse().ok()));
    }
    counts
}

/// `git diff --name-status -z`: the files a commit changed.
fn parse_name_status(out: &[u8]) -> Vec<GitChange> {
    let mut changes = Vec::new();
    let mut fields = out.split(|b| *b == 0).map(text).filter(|f| !f.is_empty());
    while let Some(status) = fields.next() {
        let letter = status.chars().next().unwrap_or('M');
        let Some(first) = fields.next() else { break };
        let (path, from) = if matches!(letter, 'R' | 'C') {
            match fields.next() {
                Some(new) => (new, Some(first)),
                None => break,
            }
        } else {
            (first, None)
        };
        changes.push(GitChange { path, from, status: status_word(letter).to_owned(), staged: false, added: None, removed: None });
    }
    changes
}

fn with_counts(changes: &mut [GitChange], counts: &HashMap<String, (Option<u32>, Option<u32>)>) {
    for change in changes {
        if let Some((added, removed)) = counts.get(&change.path) {
            (change.added, change.removed) = (*added, *removed);
        }
    }
}

/// The records of `COMMIT_FORMAT`.
fn parse_commits(out: &[u8]) -> Vec<GitCommit> {
    text(out)
        .split('\u{1e}')
        .filter_map(|record| {
            let fields: Vec<&str> = record.trim_start_matches(['\r', '\n']).split('\0').collect();
            let [id, subject, author, email, at, parents, refs] = fields.as_slice() else { return None };
            Some(GitCommit {
                id: (*id).to_owned(),
                subject: (*subject).to_owned(),
                author: (*author).to_owned(),
                email: (*email).to_owned(),
                at_ms: at.parse::<i64>().unwrap_or(0) * 1000,
                parents: parents.split_whitespace().map(str::to_owned).collect(),
                refs: refs.split(", ").map(|r| r.trim_start_matches("HEAD -> ").to_owned()).filter(|r| !r.is_empty() && r != "HEAD").collect(),
            })
        })
        .collect()
}

/// `for-each-ref` with the fields of a `GitBranch`, NUL between them.
fn parse_branches(out: &[u8]) -> Vec<GitBranch> {
    let mut branches: Vec<GitBranch> = text(out)
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\0').collect();
            let [name, head, subject, author, at, upstream, track, worktree, current] = fields.as_slice() else { return None };
            let (name, remote) = match name.strip_prefix("refs/heads/") {
                Some(local) => (local, false),
                None => (name.strip_prefix("refs/remotes/")?, true),
            };
            // `origin/HEAD` only says which branch a remote calls its default.
            if remote && name.ends_with("/HEAD") {
                return None;
            }
            let count = |word: &str| track.split(", ").find_map(|part| part.strip_prefix(word)).and_then(|n| n.trim().parse::<u32>().ok());
            let follows = !upstream.is_empty() && *track != "gone";
            Some(GitBranch {
                name: name.to_owned(),
                remote,
                current: *current == "*",
                head: (*head).to_owned(),
                subject: (*subject).to_owned(),
                author: (*author).to_owned(),
                at_ms: at.parse::<i64>().unwrap_or(0) * 1000,
                upstream: (!upstream.is_empty()).then(|| (*upstream).to_owned()),
                ahead: follows.then(|| count("ahead ").unwrap_or(0)),
                behind: follows.then(|| count("behind ").unwrap_or(0)),
                worktree: (!worktree.is_empty() && *current != "*").then(|| (*worktree).to_owned()),
            })
        })
        .collect();
    // Local branches first; each group keeps git's order (latest commit first).
    branches.sort_by_key(|b| b.remote);
    branches
}

/// `text` cut at the end of a line when it is longer than `GIT_DIFF_MAX_BYTES`.
fn capped(mut bytes: Vec<u8>) -> GitDiff {
    let truncated = bytes.len() > GIT_DIFF_MAX_BYTES;
    if truncated {
        let cut = bytes[..GIT_DIFF_MAX_BYTES].iter().rposition(|b| *b == b'\n').map_or(GIT_DIFF_MAX_BYTES, |at| at + 1);
        bytes.truncate(cut);
    }
    GitDiff { text: text(&bytes), truncated }
}

pub async fn overview(shared: &Arc<Shared>, project: ProjectId) -> Result<GitOverview, ProtocolError> {
    let Some(folder) = folder_of(shared, project).await? else { return Ok(GitOverview::default()) };
    let shown = Some(folder.display().to_string());
    let Some(top) = repository(&folder).await else { return Ok(GitOverview { folder: shown, ..Default::default() }) };
    let head = git(&top, &["rev-parse", "--verify", "-q", "HEAD"], &[1]).await.map(|o| text(&o).trim().to_owned()).ok().filter(|h| !h.is_empty());
    let branch = git(&top, &["symbolic-ref", "-q", "--short", "HEAD"], &[1]).await.map(|o| text(&o).trim().to_owned()).ok().filter(|b| !b.is_empty());
    let mut changes = parse_status(&git(&top, &["status", "--porcelain=v2", "-z", "--untracked-files=all"], &[]).await?);
    if head.is_some() {
        with_counts(&mut changes, &parse_numstat(&git(&top, &["diff", "--numstat", "-z", "-M", "--no-ext-diff", "--no-textconv", "HEAD"], &[]).await?));
    }
    let changes_truncated = changes.len() > GIT_FILES_MAX;
    changes.truncate(GIT_FILES_MAX);
    let format = "--format=%(refname)%00%(objectname)%00%(contents:subject)%00%(authorname)%00%(authordate:unix)%00%(upstream:short)%00%(upstream:track,nobracket)%00%(worktreepath)%00%(HEAD)";
    let branches = parse_branches(&git(&top, &["for-each-ref", "--sort=-committerdate", format, "refs/heads", "refs/remotes"], &[]).await?);
    Ok(GitOverview { folder: shown, repository: true, branch, head, changes, changes_truncated, branches })
}

pub async fn log(shared: &Arc<Shared>, project: ProjectId, rev: Option<String>, base: Option<String>, path: Option<String>, skip: Option<u32>, limit: Option<u32>) -> Result<GitLog, ProtocolError> {
    let top = repository_of(shared, project).await?;
    let limit = limit.unwrap_or(LOG_DEFAULT).clamp(1, GIT_LOG_MAX);
    let (count, skip, format) = (format!("--max-count={}", limit + 1), format!("--skip={}", skip.unwrap_or(0)), format!("--format={COMMIT_FORMAT}"));
    let mut args = vec!["log", count.as_str(), skip.as_str(), format.as_str(), "--end-of-options"];
    let rev = rev.as_deref().map(checked_rev).transpose()?;
    args.push(rev.unwrap_or("HEAD"));
    // Without what `base` has: `^base` is one more revision, checked like the first.
    let not_base = base.as_deref().map(checked_rev).transpose()?.map(|b| format!("^{b}"));
    if let Some(not_base) = &not_base {
        args.push(not_base);
    }
    let path = path.as_deref().map(checked_path).transpose()?.map(literal);
    args.push("--");
    if let Some(path) = &path {
        args.push(path);
    }
    // A repository without a commit has no HEAD to list: that is an empty history, not an error.
    let out = match git(&top, &args, &[]).await {
        Ok(out) => out,
        Err(_) if rev.is_none() => return Ok(GitLog::default()),
        Err(err) => return Err(err),
    };
    let mut commits = parse_commits(&out);
    let has_more = commits.len() > limit as usize;
    commits.truncate(limit as usize);
    Ok(GitLog { commits, has_more })
}

/// What a commit is compared with: its first parent, or the empty tree for a first commit.
async fn base_of(top: &Path, commit: &GitCommit) -> Result<String, ProtocolError> {
    match commit.parents.first() {
        Some(parent) => Ok(parent.clone()),
        None => Ok(text(&git(top, &["hash-object", "-t", "tree", "--stdin"], &[]).await?).trim().to_owned()),
    }
}

async fn commit_of(top: &Path, id: &str) -> Result<(GitCommit, String), ProtocolError> {
    let (format, rev) = (format!("--format={COMMIT_FORMAT}%b"), format!("{}^{{commit}}", checked_id(id)?));
    let out = text(&git(top, &["show", "-s", format.as_str(), "--end-of-options", rev.as_str()], &[]).await?);
    let (record, body) = out.split_once('\u{1e}').unwrap_or((out.as_str(), ""));
    let commit = parse_commits(format!("{record}\u{1e}").as_bytes()).pop().ok_or_else(|| not_found("This commit does not exist."))?;
    Ok((commit, body.trim().to_owned()))
}

pub async fn commit(shared: &Arc<Shared>, project: ProjectId, id: String) -> Result<GitCommitDetail, ProtocolError> {
    let top = repository_of(shared, project).await?;
    let (commit, body) = commit_of(&top, &id).await?;
    let base = base_of(&top, &commit).await?;
    let mut files = parse_name_status(&git(&top, &["diff", "--name-status", "-z", "-M", "--no-ext-diff", "--no-textconv", "--end-of-options", &base, &commit.id], &[]).await?);
    with_counts(&mut files, &parse_numstat(&git(&top, &["diff", "--numstat", "-z", "-M", "--no-ext-diff", "--no-textconv", "--end-of-options", &base, &commit.id], &[]).await?));
    let files_truncated = files.len() > GIT_FILES_MAX;
    files.truncate(GIT_FILES_MAX);
    Ok(GitCommitDetail { commit, body, files, files_truncated })
}

pub async fn diff(shared: &Arc<Shared>, project: ProjectId, id: Option<String>, rev: Option<String>, base: Option<String>, path: Option<String>) -> Result<GitDiff, ProtocolError> {
    let top = repository_of(shared, project).await?;
    let path = path.as_deref().map(checked_path).transpose()?;
    let spec = path.map(literal);
    let common = ["diff", "-M", "--no-ext-diff", "--no-textconv"];
    // Everything a branch changed since it left its base: from where the two parted to its tip.
    if let (Some(rev), Some(base)) = (rev.as_deref(), base.as_deref()) {
        let (rev, base) = (checked_rev(rev)?, checked_rev(base)?);
        let parted = text(&git(&top, &["merge-base", "--end-of-options", base, rev], &[]).await?).trim().to_owned();
        let mut args: Vec<&str> = common.to_vec();
        args.extend(["--end-of-options", &parted, rev, "--"]);
        if let Some(spec) = &spec {
            args.push(spec);
        }
        return Ok(capped(git(&top, &args, &[]).await?));
    }
    if let Some(id) = id {
        let (commit, _) = commit_of(&top, &id).await?;
        let base = base_of(&top, &commit).await?;
        let mut args: Vec<&str> = common.to_vec();
        args.extend(["--end-of-options", &base, &commit.id, "--"]);
        if let Some(spec) = &spec {
            args.push(spec);
        }
        return Ok(capped(git(&top, &args, &[]).await?));
    }
    // The work folder against its last commit: what is staged and what is not, then what is new.
    let has_head = git(&top, &["rev-parse", "--verify", "-q", "HEAD"], &[]).await.is_ok();
    let mut args: Vec<&str> = common.to_vec();
    args.push(if has_head { "HEAD" } else { "--cached" });
    args.push("--");
    if let Some(spec) = &spec {
        args.push(spec);
    }
    let mut out = git(&top, &args, &[]).await?;
    let mut untracked = vec!["ls-files", "--others", "--exclude-standard", "-z", "--"];
    if let Some(spec) = &spec {
        untracked.push(spec);
    }
    let new: Vec<String> = git(&top, &untracked, &[]).await?.split(|b| *b == 0).map(text).filter(|p| !p.is_empty()).collect();
    for (shown, file) in new.iter().enumerate() {
        if out.len() > GIT_DIFF_MAX_BYTES {
            break;
        }
        if shown >= UNTRACKED_SHOWN {
            out.extend_from_slice(format!("\n{} more new files are not shown here; open one to see it.\n", new.len() - shown).as_bytes());
            break;
        }
        // `--no-index` ends with 1 when the two differ, which a new file always does.
        if let Ok(added) = git(&top, &["diff", "--no-ext-diff", "--no-textconv", "--no-index", "--", "/dev/null", file], &[1]).await {
            out.extend_from_slice(&added);
        }
    }
    Ok(capped(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_client_names_is_never_an_option_a_range_or_a_way_out() {
        for rev in ["main", "feat/ticket-40", "v0.9.1", "HEAD~3", "origin/main", "1a19f67"] {
            assert!(checked_rev(rev).is_ok(), "{rev}");
        }
        for rev in ["", "-n", "--output=/etc/passwd", "main..dev", "a b", "main;rm", "$(x)", "main\n"] {
            assert!(checked_rev(rev).is_err(), "{rev:?}");
        }
        assert!(checked_id("1a19f67").is_ok() && checked_id("zz19f67").is_err() && checked_id("abc").is_err() && checked_id("-abcd").is_err());
        for path in ["src/main.rs", "a file.txt", "-odd-name"] {
            assert!(checked_path(path).is_ok(), "{path}");
        }
        for path in ["", "../secret", "a/../../b", "/etc/passwd", "a\0b"] {
            assert!(checked_path(path).is_err(), "{path:?}");
        }
        assert_eq!(literal("-odd-name"), ":(top,literal)-odd-name");
    }

    #[test]
    fn a_status_reads_staged_renamed_conflicting_and_new_files() {
        let out = b"1 M. N... 100644 100644 100644 aaa bbb staged.rs\x001 .M N... 100644 100644 100644 aaa bbb edited.rs\x002 R. N... 100644 100644 100644 aaa bbb R100 new name.rs\x00old name.rs\x00u UU N... 100644 100644 100644 100644 a b c both.rs\x00? fresh.txt\x00";
        let seen: Vec<(String, Option<String>, String, bool)> = parse_status(out).into_iter().map(|c| (c.path, c.from, c.status, c.staged)).collect();
        assert_eq!(
            seen,
            [
                ("new name.rs".to_owned(), Some("old name.rs".to_owned()), "renamed".to_owned(), true),
                ("staged.rs".to_owned(), None, "modified".to_owned(), true),
                ("both.rs".to_owned(), None, "conflict".to_owned(), false),
                ("edited.rs".to_owned(), None, "modified".to_owned(), false),
                ("fresh.txt".to_owned(), None, "untracked".to_owned(), false),
            ]
        );
    }

    #[test]
    fn counts_names_commits_and_branches_are_read_as_git_prints_them() {
        let counts = parse_numstat(b"3\t1\tsrc/a.rs\x00-\t-\tlogo.png\x005\t0\t\x00old.rs\x00new.rs\x00");
        assert_eq!(counts["src/a.rs"], (Some(3), Some(1)));
        assert_eq!(counts["logo.png"], (None, None));
        assert_eq!(counts["new.rs"], (Some(5), Some(0)));

        let names = parse_name_status(b"M\x00src/a.rs\x00R087\x00old.rs\x00new.rs\x00D\x00gone.rs\x00");
        assert_eq!(names.iter().map(|c| (c.path.as_str(), c.from.as_deref(), c.status.as_str())).collect::<Vec<_>>(), [("src/a.rs", None, "modified"), ("new.rs", Some("old.rs"), "renamed"), ("gone.rs", None, "deleted")]);

        let commits = parse_commits("abc\0Fix the board\0Forge\0forge@example.com\x001700000000\0p1 p2\0HEAD -> main, origin/main, tag: v1\u{1e}\ndef\0First\0Alex\0a@example.com\x001600000000\0\0\u{1e}\n".as_bytes());
        assert_eq!(commits.len(), 2);
        assert_eq!((commits[0].subject.as_str(), commits[0].at_ms, commits[0].parents.len()), ("Fix the board", 1_700_000_000_000, 2));
        assert_eq!(commits[0].refs, ["main", "origin/main", "tag: v1"]);
        assert!(commits[1].parents.is_empty() && commits[1].refs.is_empty());

        let branches = parse_branches("refs/remotes/origin/main\0aaa\0Tip\0Alex\x001700000000\0\0\0\0 \nrefs/remotes/origin/HEAD\0aaa\0Tip\0Alex\x001700000000\0\0\0\0 \nrefs/heads/feat/x\0bbb\0Work\0Forge\x001700000100\0origin/feat/x\0ahead 2, behind 1\0/srv/wt-x\0 \nrefs/heads/main\0aaa\0Tip\0Alex\x001700000000\0origin/main\0\0/srv/repo\0*\n".as_bytes());
        assert_eq!(branches.iter().map(|b| (b.name.as_str(), b.remote, b.current)).collect::<Vec<_>>(), [("feat/x", false, false), ("main", false, true), ("origin/main", true, false)]);
        assert_eq!((branches[0].ahead, branches[0].behind, branches[0].worktree.as_deref()), (Some(2), Some(1), Some("/srv/wt-x")));
        assert_eq!((branches[1].ahead, branches[1].behind, branches[1].worktree.as_deref()), (Some(0), Some(0), None));
    }

    #[test]
    fn a_long_diff_is_cut_at_the_end_of_a_line() {
        let short = capped(b"diff --git a/x b/x\n".to_vec());
        assert!(!short.truncated && short.text.ends_with('\n'));
        let line = "+a line of a very long change\n";
        let long = capped(line.repeat(GIT_DIFF_MAX_BYTES / line.len() + 10).into_bytes());
        assert!(long.truncated && long.text.len() <= GIT_DIFF_MAX_BYTES && long.text.ends_with('\n'));
    }
}
