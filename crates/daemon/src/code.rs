//! The Code tab (ticket #77, feature `code_view`): `code.get` reads a project's git state
//! and the `code.*` host writes change it on a board member's request. Never a push.
//! Git runs through `quick`'s helpers (no prompt, no inherited `GIT_DIR`), and every
//! write takes `quick`'s git lock, so it never overlaps a quick task's merge.
//!
//! Git's output is read by small pure parsers (`parse_*`); the view takes what the
//! database knows of each branch (`aurelius_storage::code`) and redacts what it shows.

use crate::Shared;
use crate::api::{app_task, internal, storage_task};
use crate::quick::{self, ok};
use aurelius_application::ticket::quick::MergeOutcome;
use aurelius_domain::ProjectId;
use aurelius_domain::code::{CODE_BRANCH_COMMITS_MAX, CODE_BRANCHES_MAX, CODE_COMMITS_MAX, CODE_FILES_MAX, CODE_SUBJECT_MAX_CHARS, CodeBranch, CodeCommit, CodeCommitLine, CodeView, CodeWorktree};
use aurelius_protocol::{ErrorCode, HostWrite, ProtocolError};
use aurelius_storage::code::{BranchLink, SessionWorktree};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// Where local branches live; refs are named in full so a tag or file of the same name never answers.
const HEADS: &str = "refs/heads/";
/// Separates the records of `git log` (ASCII record separator); fields are split by NUL.
const RECORD: char = '\u{1e}';
/// The fields of a branch line of `git for-each-ref` (see `parse_refs`).
const REFS_FORMAT: &str = "--format=%(refname)%00%(objectname)%00%(upstream:short)%00%(upstream:track,nobracket)%00%(committerdate:unix)%00%(authorname)%00%(subject)";
/// The fields of a commit of the graph (see `parse_log`).
const LOG_FORMAT: &str = "--format=%H%x00%P%x00%ct%x00%an%x00%s%x1e";
/// The fields of a branch's own commit (see `parse_lines`).
const LINES_FORMAT: &str = "--format=%H%x00%s%x1e";
/// How long `code.fetch` may take before its git is stopped: a remote that accepts the
/// connection and never answers would otherwise keep it waiting for good.
pub const CODE_FETCH_TIMEOUT_MS: u64 = 60_000;
/// How often a fetch is checked against its deadline.
const FETCH_POLL_MS: u64 = 50;
/// How long a finished fetch's error text is waited for.
const FETCH_STDERR_WAIT_MS: u64 = 1_000;
/// git's own limit on a slow http transfer: under 1000 bytes/s for 30 s ends it.
const FETCH_HTTP_LIMITS: [&str; 4] = ["-c", "http.lowSpeedLimit=1000", "-c", "http.lowSpeedTime=30"];
/// The ssh a fetch uses when neither the environment nor git's config names one: never a
/// prompt, a host that does not answer is given up after 15 s, and one that stops answering
/// after the handshake after a minute. A configured ssh (`core.sshCommand`) is always kept.
const FETCH_SSH_COMMAND: &str = "core.sshCommand=ssh -o BatchMode=yes -o ConnectTimeout=15 -o ServerAliveInterval=15 -o ServerAliveCountMax=4";

/// `code.get`: the project's git state. A project without a usable repository answers a
/// view with `problem` set; only an unknown project is an error.
pub async fn view(shared: &Arc<Shared>, project: ProjectId) -> Result<CodeView, ProtocolError> {
    let folder = folder_of(shared, project).await?;
    let Some(folder) = folder else { return Ok(problem(project, String::new(), "This project has no folder, so it has no code to show. Choose its folder in the project's settings.".into())) };
    let repo = PathBuf::from(&folder);
    if !repo.is_dir() {
        return Ok(problem(project, folder.clone(), format!("The project's folder {folder} does not exist (anymore).")));
    }
    let read = app_task(move || Ok(read_view(project, &repo))).await?;
    let mut view = match read {
        Ok(view) => view,
        Err(why) => return Ok(problem(project, folder, why)),
    };
    let branches: Vec<String> = view.branches.iter().map(|b| b.name.clone()).collect();
    let links = links_of(shared, project, branches).await?;
    for branch in &mut view.branches {
        let Some(link) = links.get(&branch.name) else { continue };
        branch.ticket = link.ticket.clone().map(|mut t| {
            t.title = aurelius_runtime::redact::string(t.title);
            t
        });
        branch.owner = link.owner.clone();
        branch.working = link.working;
        branch.open = link.open;
    }
    // A worktree works when its branch does, or (without a branch) when a session runs in it.
    let store = shared.store().clone();
    let sessions = storage_task(move || store.read(|c| aurelius_storage::code::session_worktrees(c, project))).await?;
    for worktree in &mut view.worktrees {
        worktree.working = match &worktree.branch {
            Some(branch) => links.get(branch).is_some_and(|l| l.working),
            None => working_at(&sessions, &worktree.path),
        };
    }
    Ok(view)
}

/// A `code.*` host write, then the view as it is after it.
pub async fn write(shared: &Arc<Shared>, write: HostWrite) -> Result<CodeView, ProtocolError> {
    let project = match &write {
        HostWrite::CodeFetch { project_id }
        | HostWrite::CodeMerge { project_id, .. }
        | HostWrite::CodeRemoveWorktree { project_id, .. }
        | HostWrite::CodeDeleteBranch { project_id, .. }
        | HostWrite::CodePruneMerged { project_id } => *project_id,
        _ => return Err(internal("not a code write")),
    };
    let folder = folder_of(shared, project).await?.ok_or_else(|| invalid("This project has no folder, so there is no repository to change."))?;
    let repo = PathBuf::from(folder);
    let state = {
        let repo = repo.clone();
        app_task(move || repo_state(&repo).map_err(invalid)).await?
    };
    // What the database says of each branch and branchless worktree, read before git is locked:
    // a run that starts in between is no different from one that starts right after.
    let branches: Vec<String> = state.refs.iter().map(|r| r.name.clone()).collect();
    let links = links_of(shared, project, branches).await?;
    let store = shared.store().clone();
    let sessions = storage_task(move || store.read(|c| aurelius_storage::code::session_worktrees(c, project))).await?;
    let loose_working: Vec<String> = state.worktrees.iter().filter(|w| w.branch.is_none() && working_at(&sessions, &w.path)).map(|w| w.path.clone()).collect();
    let busy = Busy { links, loose_working };
    app_task(move || apply(&repo, write, &busy, Duration::from_millis(CODE_FETCH_TIMEOUT_MS))).await?;
    view(shared, project).await
}

/// One write (blocking). A fetch runs without `quick::lock()`: it only writes remote refs and
/// `FETCH_HEAD`, which no quick op touches, and it may wait on the network up to
/// `fetch_deadline`. Every other write holds the lock.
fn apply(repo: &Path, write: HostWrite, busy: &Busy, fetch_deadline: Duration) -> Result<(), ProtocolError> {
    if let HostWrite::CodeFetch { .. } = write {
        return fetch(repo, fetch_deadline);
    }
    let _git = quick::lock();
    perform(repo, write, busy)
}

/// What the database says of who works where, for the refusals of a write.
struct Busy {
    links: HashMap<String, BranchLink>,
    /// Branchless worktrees a work session works in now.
    loose_working: Vec<String>,
}

impl Busy {
    fn branch(&self, branch: &str) -> bool {
        self.links.get(branch).is_some_and(|l| l.working)
    }

    /// Its ticket or work session is still open: Clean up merged leaves it alone.
    fn open(&self, branch: &str) -> bool {
        self.links.get(branch).is_some_and(|l| l.open)
    }

    /// Why `branch` stays because it is open: its ticket's, else its work session's reason.
    fn open_why(&self, branch: &str) -> Option<&'static str> {
        let link = self.links.get(branch).filter(|l| l.open)?;
        Some(if link.ticket.is_some() { STILL_OPEN } else { SESSION_OPEN })
    }

    fn worktree(&self, worktree: &WorktreeEntry) -> bool {
        match &worktree.branch {
            Some(branch) => self.branch(branch),
            None => self.loose_working.iter().any(|p| same_path(p, &worktree.path)),
        }
    }
}

/// Whether the newest session whose worktree is the folder at `path` works now. The paths are
/// compared as folders (`same_path`): git and the session may write the same one differently.
fn working_at(sessions: &[SessionWorktree], path: &str) -> bool {
    sessions.iter().find(|s| same_path(&s.path, path)).is_some_and(|s| s.working)
}

/// A refusal for the client. Its text often carries a line of git (a remote's URL, a path),
/// so it passes the vault's redactor first.
fn invalid(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::Validation, aurelius_runtime::redact::string(message.into()))
}

/// The project's folder; an unknown project is `NotFound`.
async fn folder_of(shared: &Arc<Shared>, project: ProjectId) -> Result<Option<String>, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || store.read(|c| aurelius_storage::code::project_folder(c, project))).await?.ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, "This project does not exist (anymore)."))
}

/// What the database knows of each branch.
async fn links_of(shared: &Arc<Shared>, project: ProjectId, branches: Vec<String>) -> Result<HashMap<String, BranchLink>, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|c| {
            let mut links = HashMap::new();
            for branch in branches {
                let link = aurelius_storage::code::branch_link(c, project, &branch)?;
                links.insert(branch, link);
            }
            Ok(links)
        })
    })
    .await
}

/// A view that only says why there is nothing to show; `why` may be git's own line, so it is redacted.
fn problem(project: ProjectId, repo_path: String, why: String) -> CodeView {
    let why = aurelius_runtime::redact::string(why);
    CodeView { project_id: project, repo_path, main_branch: None, problem: Some(why), has_remote: false, fetched_at_ms: None, worktrees: vec![], branches: vec![], commits: vec![] }
}

/// The repository's main branch, its worktrees and local branches, as one write needs them.
struct RepoState {
    main: String,
    worktrees: Vec<WorktreeEntry>,
    refs: Vec<RefEntry>,
}

/// Read what every write checks against. `Err`: why there is no repository to change.
fn repo_state(repo: &Path) -> Result<RepoState, String> {
    let main = quick::main_branch(repo).ok_or_else(|| quick::no_main_reason(repo))?.to_owned();
    let worktrees = parse_worktrees(&read(repo, &["worktree", "list", "--porcelain"])?);
    let refs = parse_refs(&read(repo, &["for-each-ref", "--sort=-committerdate", REFS_FORMAT, HEADS])?);
    Ok(RepoState { main, worktrees, refs })
}

/// The git half of `code.get` (blocking). `Err`: the reason there is nothing to show.
fn read_view(project: ProjectId, repo: &Path) -> Result<CodeView, String> {
    let RepoState { main, worktrees, refs } = repo_state(repo)?;
    let repo_path = repo.to_string_lossy().into_owned();

    let mut listed: Vec<CodeWorktree> = worktrees
        .iter()
        .map(|w| {
            let missing = w.prunable || !Path::new(&w.path).is_dir();
            let (changed_files, changed_count) = if missing { (vec![], 0) } else { changes(Path::new(&w.path)) };
            CodeWorktree { path: w.path.clone(), branch: w.branch.clone(), is_main: same_path(&w.path, &repo_path), head: w.head.clone(), changed_files, changed_count, missing, working: false }
        })
        .collect();
    // The project's folder first; git lists the main worktree first, which is the same unless
    // the folder is itself a linked worktree.
    listed.sort_by_key(|w| !w.is_main);

    let checked_out: HashMap<&str, &str> = worktrees.iter().filter_map(|w| Some((w.branch.as_deref()?, w.path.as_str()))).collect();
    let mut ordered: Vec<&RefEntry> = refs.iter().filter(|r| r.name == main).collect();
    ordered.extend(refs.iter().filter(|r| r.name != main));
    ordered.truncate(CODE_BRANCHES_MAX);
    let branches: Vec<CodeBranch> = ordered.iter().map(|r| branch_of(repo, &main, r, checked_out.get(r.name.as_str()).copied())).collect();

    let mut tips: HashMap<&str, Vec<String>> = HashMap::new();
    for b in &branches {
        tips.entry(b.head.as_str()).or_default().push(b.name.clone());
    }
    let max = format!("--max-count={CODE_COMMITS_MAX}");
    let mut args: Vec<String> = vec!["log".into(), "--date-order".into(), max, LOG_FORMAT.into()];
    args.extend(branches.iter().map(|b| format!("{HEADS}{}", b.name)));
    args.push("--".into());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let commits = parse_log(&read(repo, &args)?)
        .into_iter()
        .map(|c| {
            let branches = tips.get(c.hash.as_str()).cloned().unwrap_or_default();
            CodeCommit { hash: c.hash, parents: c.parents, subject: shown(&c.subject), author: c.author, at_ms: c.at_ms, branches }
        })
        .collect();

    let has_remote = !read(repo, &["remote"]).unwrap_or_default().is_empty();
    let fetched_at_ms = read(repo, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .ok()
        .and_then(|dir| std::fs::metadata(Path::new(&dir).join("FETCH_HEAD")).ok()?.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);

    Ok(CodeView { project_id: project, repo_path, main_branch: Some(main), problem: None, has_remote, fetched_at_ms, worktrees: listed, branches, commits })
}

/// One branch of the view, without what the database knows of it.
fn branch_of(repo: &Path, main: &str, r: &RefEntry, worktree_path: Option<&str>) -> CodeBranch {
    let is_main = r.name == main;
    let (behind, ahead) = if is_main {
        (0, 0)
    } else {
        read(repo, &["rev-list", "--left-right", "--count", &format!("{HEADS}{main}...{HEADS}{}", r.name), "--"]).ok().and_then(|t| parse_left_right(&t)).unwrap_or((0, 0))
    };
    let (mut files_changed, mut insertions, mut deletions, mut commits) = (0, 0, 0, vec![]);
    if ahead > 0 {
        (files_changed, insertions, deletions) = parse_shortstat(&read(repo, &["diff", "--shortstat", &format!("{HEADS}{main}...{HEADS}{}", r.name), "--"]).unwrap_or_default());
        let max = format!("--max-count={CODE_BRANCH_COMMITS_MAX}");
        let range = format!("{HEADS}{main}..{HEADS}{}", r.name);
        commits = parse_lines(&read(repo, &["log", &max, LINES_FORMAT, &range, "--"]).unwrap_or_default());
    }
    CodeBranch {
        name: r.name.clone(),
        head: r.head.clone(),
        is_main,
        ahead,
        behind,
        merged: !is_main && ahead == 0,
        upstream: r.upstream.clone(),
        upstream_ahead: r.upstream_ahead,
        upstream_behind: r.upstream_behind,
        subject: shown(&r.subject),
        author: r.author.clone(),
        at_ms: r.at_ms,
        worktree_path: worktree_path.map(str::to_owned),
        files_changed,
        insertions,
        deletions,
        commits,
        ticket: None,
        owner: None,
        working: false,
        open: false,
    }
}

/// git for a read of the Code tab: it never takes an optional lock (`git status` would
/// otherwise refresh the index under `index.lock`), so a poll never makes an employee's
/// `git add` or `git commit` in the same worktree fail.
fn reader(dir: &Path, args: &[&str]) -> std::process::Command {
    let mut command = quick::command(dir, args);
    command.env("GIT_OPTIONAL_LOCKS", "0");
    command
}

/// `quick::ok` for a read: git's output, or why it failed.
fn read(dir: &Path, args: &[&str]) -> Result<String, String> {
    match reader(dir, args).output() {
        Ok(out) if out.status.success() => Ok(quick::text(&out)),
        Ok(out) => Err(quick::why(&out)),
        Err(e) => Err(format!("git could not run: {e}")),
    }
}

/// A worktree's uncommitted changes: at most `CODE_FILES_MAX` named, and how many there are.
fn changes(worktree: &Path) -> (Vec<String>, u32) {
    let Ok(out) = reader(worktree, &["status", "--porcelain=v1", "-z"]).output() else { return (vec![], 0) };
    if !out.status.success() {
        return (vec![], 0);
    }
    let mut files = parse_status(&String::from_utf8_lossy(&out.stdout));
    let count = files.len() as u32;
    files.truncate(CODE_FILES_MAX);
    (files, count)
}

/// A subject as the view shows it: redacted, cut at `CODE_SUBJECT_MAX_CHARS`.
fn shown(subject: &str) -> String {
    cut(&aurelius_runtime::redact::text(subject))
}

/// At most `CODE_SUBJECT_MAX_CHARS` characters, the last one an ellipsis when it was cut.
fn cut(text: &str) -> String {
    if text.chars().count() <= CODE_SUBJECT_MAX_CHARS {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(CODE_SUBJECT_MAX_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// The same folder, also when one is written differently (a trailing slash, a symlink).
fn same_path(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => Path::new(a.trim_end_matches(['/', '\\'])) == Path::new(b.trim_end_matches(['/', '\\'])),
    }
}

// ---- The writes (blocking, under `quick::lock()`) ----

fn perform(repo: &Path, write: HostWrite, busy: &Busy) -> Result<(), ProtocolError> {
    // Read again under the lock: another write may have changed it since.
    let state = repo_state(repo).map_err(invalid)?;
    match write {
        HostWrite::CodeFetch { .. } => Err(internal("a fetch runs outside the git lock (see apply)")),
        HostWrite::CodeMerge { branch, .. } => merge(repo, &state, &branch, busy),
        HostWrite::CodeRemoveWorktree { path, .. } => remove_worktree(repo, &state, &path, busy),
        HostWrite::CodeDeleteBranch { branch, force, .. } => delete_branch(repo, &state, &branch, force, busy),
        HostWrite::CodePruneMerged { .. } => prune_merged(repo, &state, busy),
        _ => Err(internal("not a code write")),
    }
}

/// `code.fetch`: `git fetch --prune` of every remote in turn, all within `deadline`. One
/// process per remote (not `--all`, whose per-remote children a kill would not reach). A
/// repository without a remote has nothing to fetch.
fn fetch(repo: &Path, deadline: Duration) -> Result<(), ProtocolError> {
    let remotes = read(repo, &["remote"]).map_err(invalid)?;
    let remotes: Vec<&str> = remotes.lines().map(str::trim).filter(|r| !r.is_empty()).collect();
    if remotes.is_empty() {
        return Err(invalid("This repository has no remote to fetch from."));
    }
    // Our ssh options only when nobody chose an ssh: git ranks the environment above
    // `core.sshCommand`, so setting `GIT_SSH_COMMAND` would override the repository's own.
    let own_ssh = std::env::var_os("GIT_SSH_COMMAND").is_some()
        || std::env::var_os("GIT_SSH").is_some()
        || !read(repo, &["config", "--get", "core.sshCommand"]).unwrap_or_default().is_empty();
    let started = Instant::now();
    for remote in remotes {
        let left = deadline.saturating_sub(started.elapsed());
        fetch_remote(repo, remote, own_ssh, left, deadline)?;
    }
    Ok(())
}

/// One remote's `git fetch --prune`, killed when `left` runs out (`deadline` is the whole
/// fetch's, for the message).
fn fetch_remote(repo: &Path, remote: &str, own_ssh: bool, left: Duration, deadline: Duration) -> Result<(), ProtocolError> {
    let mut args = FETCH_HTTP_LIMITS.to_vec();
    if !own_ssh {
        args.extend(["-c", FETCH_SSH_COMMAND]);
    }
    args.extend(["fetch", "--prune", "--end-of-options", remote]);
    let mut command = quick::command(repo, &args);
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| invalid(format!("git could not run: {e}")))?;
    // Read on a thread of its own, so a full pipe never stalls git. After a kill a helper of
    // git may still hold the pipe, so the text is never waited for without a limit.
    let stderr = child.stderr.take();
    let (said, heard) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut text);
        }
        let _ = said.send(text);
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= left => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(invalid(format!("The fetch did not finish within {} s.", deadline.as_secs())));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(FETCH_POLL_MS)),
            Err(e) => return Err(invalid(format!("git could not run: {e}"))),
        }
    };
    if status.success() {
        return Ok(());
    }
    let text = heard.recv_timeout(Duration::from_millis(FETCH_STDERR_WAIT_MS)).unwrap_or_default();
    Err(invalid(format!("The fetch failed: {}", quick::reason(&text))))
}

fn known_branch<'a>(state: &'a RepoState, branch: &str) -> Result<&'a RefEntry, ProtocolError> {
    state.refs.iter().find(|r| r.name == branch).ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, format!("The branch {branch} does not exist (anymore).")))
}

/// `code.merge`: `quick::merge` of the branch into main, with the ticket's number when it has one.
fn merge(repo: &Path, state: &RepoState, branch: &str, busy: &Busy) -> Result<(), ProtocolError> {
    known_branch(state, branch)?;
    if branch == state.main {
        return Err(invalid(format!("{branch} is the main branch; there is nothing to merge into it from itself.")));
    }
    // The client hides Merge for a working branch, but its view can be seconds old.
    if busy.branch(branch) {
        return Err(invalid(format!("An employee works on {branch} now; wait until they are done.")));
    }
    let message = match busy.links.get(branch).and_then(|l| l.ticket.as_ref()) {
        Some(ticket) => format!("Merge {branch} (ticket #{})", ticket.number),
        None => format!("Merge {branch}"),
    };
    match quick::merge_locked(repo, branch, &message) {
        MergeOutcome::Merged(_) => Ok(()),
        MergeOutcome::NoCommits => Err(invalid(format!("Nothing to merge: {branch} has no commits that {} lacks.", state.main))),
        MergeOutcome::Failed { reason, files } => Err(invalid(merge_failure(branch, &reason, &files))),
    }
}

/// Why a merge did not happen, with the files concerned.
fn merge_failure(branch: &str, reason: &str, files: &[String]) -> String {
    let mut message = format!("{branch} was not merged: {reason}.");
    if !files.is_empty() {
        message.push_str(&format!(" Files: {}.", files.join(", ")));
    }
    message
}

/// Why a branch of an open ticket keeps its worktree and stays: work on it may still come,
/// also when it has no commit of its own yet (as Clean up merged leaves it).
const STILL_OPEN: &str = "Its ticket is still open; it is cleaned up after the ticket is done.";
/// The same for a branch of a work session that is not deleted (it has no ticket).
const SESSION_OPEN: &str = "Its work session is still open; it is cleaned up after the session is deleted.";

/// `code.remove_worktree`: never the project's folder, one with changes, one an employee
/// works in or one of a branch whose ticket or session is still open.
fn remove_worktree(repo: &Path, state: &RepoState, path: &str, busy: &Busy) -> Result<(), ProtocolError> {
    let worktree = state.worktrees.iter().find(|w| same_path(&w.path, path)).ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, format!("Git does not know a worktree at {path}.")))?;
    if same_path(&worktree.path, &repo.to_string_lossy()) || state.worktrees.first().is_some_and(|w| std::ptr::eq(w, worktree)) {
        return Err(invalid("This is the project's own folder; it is never removed."));
    }
    if busy.worktree(worktree) {
        return Err(invalid("An employee works in this worktree now; wait until they are done."));
    }
    if let Some(why) = worktree.branch.as_deref().and_then(|b| busy.open_why(b)) {
        return Err(invalid(why));
    }
    let missing = worktree.prunable || !Path::new(&worktree.path).is_dir();
    if !missing {
        let (_, changed) = changes(Path::new(&worktree.path));
        if changed > 0 {
            return Err(invalid(format!("This worktree has uncommitted changes in {changed} file(s); commit or discard them first.")));
        }
        ok(repo, &["worktree", "remove", &worktree.path]).map_err(|why| invalid(format!("Git could not remove the worktree: {why}")))?;
    }
    ok(repo, &["worktree", "prune"]).map(drop).map_err(invalid)
}

/// `code.delete_branch`: never main, a checked-out branch, one an employee works on or one
/// whose ticket or session is still open; work main lacks only with `force`.
fn delete_branch(repo: &Path, state: &RepoState, branch: &str, force: bool, busy: &Busy) -> Result<(), ProtocolError> {
    known_branch(state, branch)?;
    if branch == state.main {
        return Err(invalid(format!("{branch} is the main branch; it is never deleted.")));
    }
    if let Some(w) = state.worktrees.iter().find(|w| w.branch.as_deref() == Some(branch)) {
        return Err(invalid(format!("{branch} is checked out in the worktree {}; remove that worktree first.", w.path)));
    }
    if busy.branch(branch) {
        return Err(invalid(format!("An employee works on {branch} now; wait until they are done.")));
    }
    if let Some(why) = busy.open_why(branch) {
        return Err(invalid(why));
    }
    let ahead = ahead_of_main(repo, &state.main, branch)?;
    if ahead > 0 && !force {
        return Err(invalid(format!("{branch} has {ahead} commit(s) that are not in {}; deleting it loses that work.", state.main)));
    }
    let flag = if force { "-D" } else { "-d" };
    ok(repo, &["branch", flag, "--end-of-options", branch]).map(drop).map_err(|why| invalid(format!("Git could not delete {branch}: {why}")))
}

/// How many commits `branch` has that main lacks.
fn ahead_of_main(repo: &Path, main: &str, branch: &str) -> Result<u32, ProtocolError> {
    let count = read(repo, &["rev-list", "--count", &format!("{HEADS}{main}..{HEADS}{branch}"), "--"]).map_err(invalid)?;
    count.trim().parse().map_err(|_| internal(format!("git counted {count:?} commits")))
}

/// `code.prune_merged`: every merged branch (not main) nobody works on loses its clean
/// worktrees, then the branch itself when no worktree has it any more. A branch that
/// cannot go entirely is left as it is. Nothing is forced.
fn prune_merged(repo: &Path, state: &RepoState, busy: &Busy) -> Result<(), ProtocolError> {
    let _ = ok(repo, &["worktree", "prune"]);
    let merged = read(repo, &["for-each-ref", "--format=%(refname)", &format!("--merged={HEADS}{}", state.main), HEADS]).map_err(invalid)?;
    let project = repo.to_string_lossy();
    for branch in merged.lines().filter_map(|l| l.strip_prefix(HEADS)) {
        // A branch whose ticket or session is still open may just not have a commit yet.
        if branch == state.main || busy.branch(branch) || busy.open(branch) {
            continue;
        }
        let mut kept = false;
        for w in state.worktrees.iter().filter(|w| w.branch.as_deref() == Some(branch)) {
            if same_path(&w.path, &project) || state.worktrees.first().is_some_and(|first| std::ptr::eq(first, w)) {
                kept = true;
                continue;
            }
            if w.prunable || !Path::new(&w.path).is_dir() {
                continue;
            }
            if changes(Path::new(&w.path)).1 > 0 {
                kept = true;
                continue;
            }
            if let Err(why) = ok(repo, &["worktree", "remove", &w.path]) {
                tracing::warn!(worktree = %w.path, error = %why, "code.prune_merged could not remove a worktree");
                kept = true;
            }
        }
        if !kept && let Err(why) = ok(repo, &["branch", "-d", "--end-of-options", branch]) {
            tracing::warn!(%branch, error = %why, "code.prune_merged could not delete a branch");
        }
    }
    ok(repo, &["worktree", "prune"]).map(drop).map_err(invalid)
}

// ---- Git's output, parsed ----

/// One worktree of `git worktree list --porcelain` (a bare entry is left out).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct WorktreeEntry {
    path: String,
    head: String,
    /// None: a detached HEAD.
    branch: Option<String>,
    /// Git says its folder is gone (`prunable`).
    prunable: bool,
}

fn parse_worktrees(text: &str) -> Vec<WorktreeEntry> {
    let mut list = Vec::new();
    for block in text.split("\n\n") {
        let mut entry = WorktreeEntry::default();
        let mut bare = false;
        for line in block.lines() {
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "worktree" => entry.path = value.to_owned(),
                "HEAD" => entry.head = value.to_owned(),
                "branch" => entry.branch = Some(value.strip_prefix(HEADS).unwrap_or(value).to_owned()),
                "bare" => bare = true,
                "prunable" => entry.prunable = true,
                _ => {}
            }
        }
        if !entry.path.is_empty() && !bare {
            list.push(entry);
        }
    }
    list
}

/// The files of `git status --porcelain=v1 -z`: each changed or untracked path once (a
/// rename by its new name).
fn parse_status(text: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut entries = text.split('\0');
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let (status, path) = entry.split_at(3);
        files.push(path.to_owned());
        // A rename or copy is followed by the path it came from.
        if status.starts_with(['R', 'C']) || status[1..].starts_with(['R', 'C']) {
            entries.next();
        }
    }
    files
}

/// One local branch of `git for-each-ref` with `REFS_FORMAT`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RefEntry {
    name: String,
    head: String,
    upstream: Option<String>,
    upstream_ahead: u32,
    upstream_behind: u32,
    at_ms: i64,
    author: String,
    subject: String,
}

fn parse_refs(text: &str) -> Vec<RefEntry> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.splitn(7, '\0');
            let name = f.next()?.strip_prefix(HEADS)?.to_owned();
            let head = f.next()?.to_owned();
            let upstream = Some(f.next()?).filter(|u| !u.is_empty()).map(str::to_owned);
            let (upstream_ahead, upstream_behind) = parse_track(f.next()?);
            let at_ms = f.next()?.parse::<i64>().unwrap_or(0) * 1000;
            let author = f.next()?.to_owned();
            let subject = f.next().unwrap_or("").to_owned();
            Some(RefEntry { name, head, upstream, upstream_ahead, upstream_behind, at_ms, author, subject })
        })
        .collect()
}

/// `%(upstream:track,nobracket)`: "ahead 2, behind 1", "ahead 2", "behind 1", "gone" or empty.
fn parse_track(text: &str) -> (u32, u32) {
    let (mut ahead, mut behind) = (0, 0);
    for part in text.split(',') {
        match part.trim().split_once(' ') {
            Some(("ahead", n)) => ahead = n.parse().unwrap_or(0),
            Some(("behind", n)) => behind = n.parse().unwrap_or(0),
            _ => {}
        }
    }
    (ahead, behind)
}

/// One commit of `git log` with `LOG_FORMAT`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LogRecord {
    hash: String,
    parents: Vec<String>,
    at_ms: i64,
    author: String,
    subject: String,
}

fn parse_log(text: &str) -> Vec<LogRecord> {
    text.split(RECORD)
        .filter_map(|record| {
            let mut f = record.trim_start_matches(['\n', '\r']).splitn(5, '\0');
            let hash = f.next().filter(|h| !h.is_empty())?.to_owned();
            let parents = f.next()?.split_whitespace().map(str::to_owned).collect();
            let at_ms = f.next()?.parse::<i64>().unwrap_or(0) * 1000;
            let author = f.next()?.to_owned();
            let subject = f.next().unwrap_or("").to_owned();
            Some(LogRecord { hash, parents, at_ms, author, subject })
        })
        .collect()
}

/// A branch's own commits of `git log` with `LINES_FORMAT`, subjects as shown.
fn parse_lines(text: &str) -> Vec<CodeCommitLine> {
    text.split(RECORD)
        .filter_map(|record| {
            let (hash, subject) = record.trim_start_matches(['\n', '\r']).split_once('\0')?;
            (!hash.is_empty()).then(|| CodeCommitLine { hash: hash.to_owned(), subject: shown(subject) })
        })
        .collect()
}

/// `git rev-list --left-right --count main...branch`: (behind, ahead) of the branch.
fn parse_left_right(text: &str) -> Option<(u32, u32)> {
    let mut n = text.split_whitespace().map(str::parse::<u32>);
    Some((n.next()?.ok()?, n.next()?.ok()?))
}

/// `git diff --shortstat`: (files changed, insertions, deletions); empty when nothing differs.
fn parse_shortstat(text: &str) -> (u32, u32, u32) {
    let (mut files, mut insertions, mut deletions) = (0, 0, 0);
    for part in text.split(',') {
        let mut words = part.split_whitespace();
        let (Some(n), Some(what)) = (words.next().and_then(|n| n.parse().ok()), words.next()) else { continue };
        if what.starts_with("file") {
            files = n;
        } else if what.starts_with("insertion") {
            insertions = n;
        } else if what.starts_with("deletion") {
            deletions = n;
        }
    }
    (files, insertions, deletions)
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
