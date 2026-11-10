//! A git worktree and a branch per ticket (migration 42, feature `ticket_worktrees`).
//!
//! Before a run of a ticket is planned, the ticket gets a branch of its own
//! (`ticket/12-fix-the-login`, from where the project's folder is then) checked out in
//! a worktree below the server's data. `conversations::work_folders` gives that folder
//! to every run of the ticket in place of the project's, so two tickets never write
//! through each other and what a ticket changed is its branch.
//!
//! This is the only place that writes to a project's repository, and it writes two
//! things: `git worktree add` and `git worktree remove`. It never commits, merges,
//! pushes, deletes a branch or touches the project's own folder. A worktree is removed
//! only when its ticket is closed and nothing in it is uncommitted; otherwise it stays,
//! so no work is ever lost here. Nothing in this file can fail a run: when a worktree
//! cannot be made the run works in the project's folder, as before.

use crate::Shared;
use aurelius_domain::{RunId, TicketId};
use aurelius_storage::tickets::{self, WorktreeNeed};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(60);

/// One ticket at a time: two runs of a ticket that start together make one worktree.
static MAKING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = tokio::process::Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(["-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    match tokio::time::timeout(TIMEOUT, command.output()).await {
        Err(_) => Err("git took too long".into()),
        Ok(Err(e)) => Err(format!("git could not be started: {e}")),
        Ok(Ok(out)) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned()),
        Ok(Ok(out)) => Err(String::from_utf8_lossy(&out.stderr).trim().chars().take(300).collect()),
    }
}

/// The ticket's branch: `ticket/<number>-<words of its title>`.
pub(crate) fn branch_name(number: u32, title: &str) -> String {
    let name = aurelius_domain::views::ticket_channel_name(number, title);
    format!("ticket/{}", name.trim_start_matches('#'))
}

async fn read<T: Send + 'static>(shared: &Arc<Shared>, f: impl FnOnce(&rusqlite::Connection) -> Result<T, aurelius_storage::StorageError> + Send + 'static) -> Option<T> {
    let store = shared.store().clone();
    tokio::task::spawn_blocking(move || store.read(f)).await.ok()?.ok()
}

async fn write(shared: &Arc<Shared>, f: impl FnOnce(&aurelius_storage::Tx<'_>) -> Result<(), aurelius_storage::StorageError> + Send + 'static) -> bool {
    let store = shared.store().clone();
    matches!(tokio::task::spawn_blocking(move || store.write(f)).await, Ok(Ok(())))
}

/// Before a run is planned: its ticket has its worktree, when it should have one.
pub async fn ensure_for_run(shared: &Arc<Shared>, run: &str) {
    let Ok(run) = RunId::parse(run) else { return };
    let Some(Some(need)) = read(shared, move |c| match tickets::of_run(c, run)? {
        Some(ticket) => tickets::worktree_need(c, ticket),
        None => Ok(None),
    })
    .await
    else {
        return;
    };
    let ticket = need.ticket;
    if let Err(why) = ensure(shared, need).await {
        tracing::warn!(%ticket, %why, "the ticket got no worktree of its own; its run works in the project's folder");
    }
    sweep(shared).await;
}

/// Where the ticket's worktree is (or will be): below the server's data, never in the project's folder.
fn worktree_root(shared: &Shared, need: &WorktreeNeed) -> PathBuf {
    shared.root.join("worktrees").join(need.project.to_string()).join(format!("ticket-{}", need.number))
}

async fn ensure(shared: &Arc<Shared>, need: WorktreeNeed) -> Result<(), String> {
    if !need.wanted || !need.open {
        return Ok(());
    }
    // Checked out already and still there: nothing to do.
    if need.has.as_ref().and_then(|w| w.path.as_deref()).is_some_and(|p| Path::new(p).is_dir()) {
        return Ok(());
    }
    let Some(folder) = need.folder.as_deref().and_then(|f| std::fs::canonicalize(f).ok()).filter(|f| f.is_dir()) else { return Ok(()) };
    let _one = MAKING.lock().await;
    // Not a repository, or one without a commit: the ticket works in the folder as before.
    let Ok(top) = git(&folder, &["rev-parse", "--show-toplevel"]).await else { return Ok(()) };
    let top = PathBuf::from(top);
    let Ok(head) = git(&top, &["rev-parse", "--verify", "-q", "HEAD"]).await else { return Ok(()) };
    let root = worktree_root(shared, &need);
    // The project's folder may be a folder inside its repository: the run works in the same one of the worktree.
    let inside = folder.strip_prefix(&top).map(Path::to_path_buf).unwrap_or_default();
    let (branch, base) = match &need.has {
        // The branch exists from an earlier start of this ticket: check it out again.
        Some(has) => (has.branch.clone(), None),
        None => {
            let on = git(&top, &["symbolic-ref", "-q", "--short", "HEAD"]).await.unwrap_or_else(|_| head.chars().take(12).collect());
            (branch_name(need.number, &need.title), Some((on, head.clone())))
        }
    };
    // Left over from a worktree that was deleted by hand.
    let _ = git(&top, &["worktree", "prune"]).await;
    if !root.is_dir() {
        if let Some(parent) = root.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let path = root.display().to_string();
        let exists = git(&top, &["rev-parse", "--verify", "-q", &format!("refs/heads/{branch}")]).await.is_ok();
        if exists {
            git(&top, &["worktree", "add", "--", &path, &branch]).await?;
        } else {
            git(&top, &["worktree", "add", "-b", &branch, "--", &path, &head]).await?;
        }
    }
    let work = root.join(inside);
    if !work.is_dir() {
        return Err(format!("{} is not in the ticket's branch", work.display()));
    }
    let (ticket, work) = (need.ticket, work.display().to_string());
    let stored = write(shared, move |tx| tickets::set_worktree(tx, ticket, &branch, base.as_ref().map(|(b, c)| (b.as_str(), c.as_str())), &work)).await;
    if stored { Ok(()) } else { Err("the worktree could not be recorded".into()) }
}

/// Worktrees of tickets that are closed go, when nothing in them is uncommitted. The branch stays.
pub async fn sweep(shared: &Arc<Shared>) {
    let Some(closed) = read(shared, tickets::closed_with_worktree).await else { return };
    for (ticket, path, _) in closed {
        if remove(Path::new(&path)).await {
            forget(shared, ticket).await;
        }
    }
}

async fn forget(shared: &Arc<Shared>, ticket: TicketId) {
    write(shared, move |tx| tickets::clear_worktree(tx, ticket)).await;
}

/// Whether the worktree at `path` is gone now. One with uncommitted work stays.
async fn remove(path: &Path) -> bool {
    if !path.is_dir() {
        return true;
    }
    let Ok(root) = git(path, &["rev-parse", "--show-toplevel"]).await else { return false };
    // `git worktree remove` refuses a worktree with changes or untracked files, and is never forced here.
    match git(Path::new(&root), &["worktree", "remove", "--", &root]).await {
        Ok(_) => true,
        Err(why) => {
            tracing::info!(path = %path.display(), %why, "a closed ticket's worktree stays: it has work that is not committed");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_is_named_after_the_tickets_number_and_title() {
        assert_eq!(branch_name(12, "Fix the login!"), "ticket/12-fix-the-login");
        assert!(branch_name(7, "…").starts_with("ticket/7"));
        // Nothing git would refuse in a branch name, whatever the title holds.
        let odd = branch_name(3, "a..b ~^: [x] \\ @{y} .lock");
        assert!(odd.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '/'), "{odd}");
    }
}
