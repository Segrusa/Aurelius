//! Quick tasks' git work (ticket #53, see `aurelius_application::ticket::quick`): the
//! worktree and branch `quick/ticket-<N>` made before the first run, the merge into the
//! project's local main after a board member approved (`ticket.quick_merge`), and the
//! cleanup after a merge or a cancel (`ticket.quick_cleanup`). Never a push.
//!
//! The merge changes main only when every check passed and git merged without a conflict:
//! main is on its branch and clean, the branch exists and has commits of its own. A
//! conflict is aborted (`git merge --abort`), so main stays as it was.

use crate::Shared;
use aurelius_application::ticket::quick::{MergeOutcome, QuickRepo};
use aurelius_domain::{TicketId, UserId};
use aurelius_storage::queries;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

/// One git operation of quick tasks at a time: two merges into the same main never overlap.
static GIT: Mutex<()> = Mutex::new(());

/// The branches a project's main may be called, in the order they are tried.
pub const MAIN_BRANCHES: [&str; 2] = ["main", "master"];

/// How many conflicting files a failed merge names at most.
pub const FILES_MAX: usize = 20;

pub(crate) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> std::io::Result<Output> {
    command(dir, args).output()
}

/// git in `dir` as every caller here runs it, for one that adds to it before it runs.
pub(crate) fn command(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(dir)
        // Never a prompt for credentials, and never the repository of the daemon's environment.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    command
}

pub(crate) fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// The first line git wrote to stderr (or stdout), as the reason of a failure.
pub(crate) fn why(out: &Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr);
    let all = if err.trim().is_empty() { String::from_utf8_lossy(&out.stdout).into_owned() } else { err.into_owned() };
    reason(&all)
}

/// The first line of what git wrote, without its "fatal: " or "error: ".
pub(crate) fn reason(text: &str) -> String {
    text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("git failed").trim_start_matches("fatal: ").trim_start_matches("error: ").to_owned()
}

pub(crate) fn ok(dir: &Path, args: &[&str]) -> Result<String, String> {
    match git(dir, args) {
        Ok(out) if out.status.success() => Ok(text(&out)),
        Ok(out) => Err(why(&out)),
        Err(e) => Err(format!("git could not run: {e}")),
    }
}

pub(crate) fn branch_exists(repo: &Path, branch: &str) -> bool {
    git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok_and(|o| o.status.success())
}

/// Hold the git lock of quick tasks and the Code tab's writes: one git write at a time.
pub(crate) fn lock() -> std::sync::MutexGuard<'static, ()> {
    GIT.lock().unwrap_or_else(|e| e.into_inner())
}

/// The project's main branch: the first of `MAIN_BRANCHES` that exists.
pub fn main_branch(repo: &Path) -> Option<&'static str> {
    MAIN_BRANCHES.into_iter().find(|b| branch_exists(repo, b))
}

/// Why `repo` has no main branch to start from, in words a board member can act on:
/// git missing, not a repository git will use (also "dubious ownership"), no commit
/// yet, or the branches it has instead of main and master.
pub fn no_main_reason(repo: &Path) -> String {
    match git(repo, &["rev-parse", "--is-inside-work-tree"]) {
        Err(e) => format!("git could not run ({e}); is Git installed and on the PATH of Aurelius?"),
        Ok(out) if !out.status.success() => format!("{} is not a git repository git will use: {}", repo.display(), why(&out)),
        Ok(_) => {
            let heads = git(repo, &["for-each-ref", "--format=%(refname:short)", "refs/heads"]).map(|o| text(&o)).unwrap_or_default();
            let branches: Vec<&str> = heads.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            if branches.is_empty() {
                format!("the repository in {} has no commit yet; make a first commit on main", repo.display())
            } else {
                format!("the repository in {} has no main or master branch (it has: {})", repo.display(), branches.join(", "))
            }
        }
    }
}

/// Make the worktree of a quick task, on its branch: a new branch from the project's
/// current main, or the branch as it is when it exists already (after a restart or a
/// cancel and a new start). Nothing happens when the worktree is there.
pub fn ensure_worktree(repo: &Path, branch: &str, worktree: &Path) -> Result<(), String> {
    let _one = GIT.lock().unwrap_or_else(|e| e.into_inner());
    if worktree.join(".git").exists() {
        return Ok(());
    }
    let main = main_branch(repo).ok_or_else(|| no_main_reason(repo))?;
    if let Some(parent) = worktree.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("could not make {}: {e}", parent.display()))?;
    }
    // A folder of an earlier worktree that git no longer knows would block `worktree add`.
    let _ = ok(repo, &["worktree", "prune"]);
    let path = worktree.to_string_lossy();
    if branch_exists(repo, branch) {
        ok(repo, &["worktree", "add", &path, branch]).map(drop)
    } else {
        ok(repo, &["worktree", "add", "-b", branch, &path, main]).map(drop)
    }
}

/// Merge a quick task's branch into the project's main (`--no-ff`, the ticket's number in
/// the message). Every failure leaves main as it was.
pub fn merge(repo: &Path, branch: &str, message: &str) -> MergeOutcome {
    let _one = lock();
    merge_locked(repo, branch, message)
}

/// `merge` for a caller that holds `lock()` already (the Code tab's `code.merge`).
pub(crate) fn merge_locked(repo: &Path, branch: &str, message: &str) -> MergeOutcome {
    let failed = |reason: String, files: Vec<String>| MergeOutcome::Failed { reason, files };
    let Some(main) = main_branch(repo) else { return failed(no_main_reason(repo), vec![]) };
    match ok(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        Ok(head) if head == main => {}
        Ok(head) => return failed(format!("the project's folder is on the branch {head}, not on {main}"), vec![]),
        Err(_) => return failed(format!("the project's folder is not on {main} (a detached HEAD)"), vec![]),
    }
    if repo.join(".git").join("MERGE_HEAD").exists() {
        return failed(format!("{main} is in the middle of another merge"), vec![]);
    }
    match ok(repo, &["status", "--porcelain", "--untracked-files=no"]) {
        Ok(changes) if changes.is_empty() => {}
        Ok(_) => {
            let files = ok(repo, &["diff", "--name-only", "HEAD"]).unwrap_or_default().lines().map(str::to_owned).take(FILES_MAX).collect();
            return failed(format!("{main} is not clean (it has uncommitted changes)"), files);
        }
        Err(e) => return failed(e, vec![]),
    }
    // The branch's commit, resolved once by its full ref: a branch named like an option
    // (`-Xours`) or shadowed by a tag of the same name still means this branch, and git
    // merges exactly the commit the checks saw.
    let Ok(commit) = ok(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}^{{commit}}")]) else {
        return failed(format!("the branch {branch} does not exist"), vec![]);
    };
    match ok(repo, &["rev-list", "--count", &format!("refs/heads/{main}..{commit}"), "--"]) {
        Ok(n) if n == "0" => return MergeOutcome::NoCommits,
        Ok(_) => {}
        Err(e) => return failed(e, vec![]),
    }
    let before = ok(repo, &["rev-parse", "HEAD"]).unwrap_or_default();
    let mut args: Vec<String> = Vec::new();
    // A machine without a git identity still merges, as Aurelius.
    if ok(repo, &["config", "user.email"]).unwrap_or_default().is_empty() {
        args.extend(["-c".into(), "user.name=Aurelius".into(), "-c".into(), "user.email=aurelius@localhost".into()]);
    }
    args.extend(["merge".into(), "--no-ff".into(), "--no-edit".into(), "-m".into(), message.into(), "--end-of-options".into(), commit]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match git(repo, &args) {
        Ok(out) if out.status.success() => match ok(repo, &["rev-parse", "HEAD"]) {
            Ok(commit) => MergeOutcome::Merged(commit),
            Err(e) => failed(e, vec![]),
        },
        result => {
            let reason = match &result {
                Ok(out) => why(out),
                Err(e) => format!("git could not run: {e}"),
            };
            let files: Vec<String> = ok(repo, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default().lines().map(str::to_owned).take(FILES_MAX).collect();
            if repo.join(".git").join("MERGE_HEAD").exists() {
                let _ = ok(repo, &["merge", "--abort"]);
            }
            // Main stays as it was, also when git stopped before it merged.
            if ok(repo, &["rev-parse", "HEAD"]).unwrap_or_default() != before {
                let _ = ok(repo, &["reset", "--hard", &before]);
            }
            let reason = if files.is_empty() { reason } else { "a merge conflict".to_owned() };
            failed(reason, files)
        }
    }
}

/// Remove a quick task's worktree and branch (after its merge, or a cancel the board
/// member confirmed: work that was not merged is gone then, BR4).
pub fn cleanup(repo: &Path, branch: &str, worktree: &Path) -> Result<(), String> {
    let _one = GIT.lock().unwrap_or_else(|e| e.into_inner());
    if worktree.exists() {
        ok(repo, &["worktree", "remove", "--force", &worktree.to_string_lossy()])?;
    }
    let _ = ok(repo, &["worktree", "prune"]);
    if branch_exists(repo, branch) {
        ok(repo, &["branch", "-D", branch])?;
    }
    Ok(())
}

/// The merge commit's message.
pub fn merge_message(repo: &QuickRepo) -> String {
    format!("Merge {}: {} (ticket #{})", repo.branch, repo.title, repo.number)
}

/// Before a run: the worktree of a quick task (when its employee works on it) or of a
/// work session that asked for one. `Err` says why it could not be made; the run then
/// fails with that reason and never starts without its folder (and so without tools).
pub(crate) async fn prepare_run(shared: &Arc<Shared>, conversation: Option<aurelius_domain::ConversationId>) -> Result<(), String> {
    let Some(conversation) = conversation else { return Ok(()) };
    let store = shared.store().clone();
    let session = blocking(move || {
        store.read(|c| {
            let Some(session) = aurelius_storage::work_sessions::of_conversation(c, conversation)? else { return Ok(None) };
            let folder = aurelius_storage::files::project_folder(c, session.project)?;
            Ok(Some((session, folder)))
        })
    })
    .await;
    match session {
        Some(Ok(Some((session, folder)))) => {
            let Some((worktree, branch)) = session.worktree else { return Ok(()) };
            let worktree = std::path::PathBuf::from(worktree);
            if worktree.join(".git").exists() {
                return Ok(());
            }
            let Some(folder) = folder else { return Err("Aurelius could not make this session's worktree: the project has no folder".into()) };
            let made = blocking(move || ensure_worktree(Path::new(&folder), &branch, &worktree)).await.unwrap_or_else(|| Err("the git work stopped".into()));
            return made.map_err(|e| {
                tracing::warn!(%conversation, error = %e, "could not make the work session's worktree");
                format!("Aurelius could not make this session's worktree: {e}")
            });
        }
        Some(Err(e)) => return Err(format!("Aurelius could not read this session: {e}")),
        _ => {}
    }
    let s = shared.clone();
    let Some(Ok(Some(repo))) = blocking(move || s.service.quick_repo_of_conversation(conversation)).await else { return Ok(()) };
    if !repo.working || repo.worktree.join(".git").exists() {
        return Ok(());
    }
    let (folder, branch, worktree) = (repo.folder.clone(), repo.branch.clone(), repo.worktree.clone());
    match blocking(move || ensure_worktree(&folder, &branch, &worktree)).await {
        Some(Ok(())) => Ok(()),
        Some(Err(e)) => {
            tracing::warn!(ticket = %repo.ticket, error = %e, "could not make the quick task's worktree");
            Err(format!("Aurelius could not make the quick task's worktree: {e}"))
        }
        None => Err("Aurelius could not make the quick task's worktree: the git work stopped".into()),
    }
}

async fn close(shared: &Arc<Shared>, job: &str, epoch: i64) {
    shared.live.job_ended(job);
    let (store, id) = (shared.store().clone(), job.to_owned());
    let _ = blocking(move || store.write(move |tx| queries::complete_job(tx, &id, epoch).map(|_| ()))).await;
}

/// The job `ticket.quick_merge`: merge, then Done (and the cleanup) or Review with the reason.
pub async fn run_merge(shared: Arc<Shared>, job: queries::JobRow) {
    let epoch = job.lease_epoch;
    let ticket = job.payload["ticket_id"].as_str().and_then(|t| TicketId::parse(t).ok());
    let user = job.payload["user_id"].as_str().and_then(|u| UserId::parse(u).ok());
    let (Some(ticket), Some(user)) = (ticket, user) else {
        close(&shared, &job.id, epoch).await;
        return;
    };
    let s = shared.clone();
    let repo = blocking(move || s.service.quick_repo(ticket)).await.and_then(Result::ok).flatten();
    let outcome = match repo.clone() {
        Some(repo) => {
            let message = merge_message(&repo);
            blocking(move || merge(&repo.folder, &repo.branch, &message))
                .await
                .unwrap_or_else(|| MergeOutcome::Failed { reason: "the merge stopped unexpectedly".into(), files: vec![] })
        }
        None => MergeOutcome::Failed { reason: "the project has no folder".into(), files: vec![] },
    };
    tracing::info!(%ticket, ?outcome, "quick task merge");
    let s = shared.clone();
    let done = match blocking(move || s.service.quick_merge_finished(ticket, user, outcome)).await {
        Some(Ok((done, notices))) => {
            shared.publish(notices);
            done
        }
        Some(Err(e)) => {
            tracing::warn!(%ticket, error = %e.message, "could not record the quick task's merge");
            false
        }
        None => false,
    };
    if let (true, Some(repo)) = (done, repo) {
        clean(&shared, repo).await;
    }
    close(&shared, &job.id, epoch).await;
}

/// The job `ticket.quick_cleanup`: a cancelled quick task's worktree and branch go.
pub async fn run_cleanup(shared: Arc<Shared>, job: queries::JobRow) {
    let epoch = job.lease_epoch;
    let ticket = job.payload["ticket_id"].as_str().and_then(|t| TicketId::parse(t).ok());
    let s = shared.clone();
    let repo = match ticket {
        Some(ticket) => blocking(move || s.service.quick_repo(ticket)).await.and_then(Result::ok).flatten(),
        None => None,
    };
    // A quick task started again before this job ran keeps its worktree and branch.
    if let Some(repo) = repo.filter(|r| !r.working) {
        clean(&shared, repo).await;
    }
    close(&shared, &job.id, epoch).await;
}

async fn clean(shared: &Arc<Shared>, repo: QuickRepo) {
    let ticket = repo.ticket;
    let (folder, branch, worktree) = (repo.folder.clone(), repo.branch.clone(), repo.worktree.clone());
    if let Some(Err(reason)) = blocking(move || cleanup(&folder, &branch, &worktree)).await {
        tracing::warn!(%ticket, error = %reason, "could not remove the quick task's worktree and branch");
        let s = shared.clone();
        if let Some(Ok(notices)) = blocking(move || s.service.quick_cleanup_failed(ticket, reason)).await {
            shared.publish(notices);
        }
    }
}

#[cfg(test)]
#[path = "quick_tests.rs"]
mod tests;
