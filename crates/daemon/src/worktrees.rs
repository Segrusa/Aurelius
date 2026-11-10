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

fn refused(why: impl Into<String>) -> aurelius_protocol::ProtocolError {
    aurelius_protocol::ProtocolError::new(aurelius_domain::ErrorCode::Validation, why)
}

/// Where a pull request for `branch` into `base` is opened, for a remote on a host we know.
pub(crate) fn pull_request_url(remote_url: &str, base: &str, branch: &str) -> Option<String> {
    let url = remote_url.trim().trim_end_matches(".git");
    // git@host:owner/repo, ssh://git@host/owner/repo and https://host/owner/repo name the same place.
    let (host, path) = if let Some(rest) = url.strip_prefix("git@") {
        rest.split_once(':')?
    } else {
        let rest = url.split_once("://")?.1;
        let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
        rest.split_once('/')?
    };
    let host = host.split(':').next()?;
    let path = path.trim_matches('/');
    if path.is_empty() || path.contains(char::is_whitespace) {
        return None;
    }
    let encode = |name: &str| name.replace('%', "%25").replace(' ', "%20").replace('#', "%23").replace('?', "%3F");
    if host == "github.com" {
        return Some(format!("https://github.com/{path}/compare/{}...{}?expand=1", encode(base), encode(branch)));
    }
    if host.contains("gitlab") {
        return Some(format!("https://{host}/{path}/-/merge_requests/new?merge_request%5Bsource_branch%5D={}&merge_request%5Btarget_branch%5D={}", encode(branch), encode(base)));
    }
    None
}

/// `ticket.land`: a board member merges a ticket's branch into the branch it was made
/// from, or pushes it for a pull request. Everything that could lose or mix up work is
/// refused with the reason, before anything is written; a merge that conflicts is undone.
pub async fn land(shared: &Arc<Shared>, ticket: TicketId, user: Option<aurelius_domain::UserId>, how: &str) -> Result<aurelius_protocol::TicketLanded, aurelius_protocol::ProtocolError> {
    let Some(user) = user else { return Err(aurelius_protocol::ProtocolError::new(aurelius_domain::ErrorCode::BoardMemberRequired, "Sign in as a board member to decide on a ticket's branch.")) };
    if !matches!(how, "merge" | "push") {
        return Err(aurelius_protocol::ProtocolError::new(aurelius_domain::ErrorCode::InvalidRequest, "how is merge or push."));
    }
    let found = read(shared, move |c| Ok((tickets::worktree_need(c, ticket)?, tickets::landed(c, ticket)?, aurelius_storage::users::get(c, &user.to_string())?))).await;
    let Some((Some(need), landed, Some(member))) = found else { return Err(aurelius_protocol::ProtocolError::new(aurelius_domain::ErrorCode::NotFound, "This ticket does not exist.")) };
    let Some(own) = need.has.clone() else { return Err(refused("This ticket has no branch of its own.")) };
    let Some(base) = own.base_branch.clone() else { return Err(refused("It is not known which branch this ticket's branch was made from.")) };
    if landed.as_ref().is_some_and(|(done, _)| done == "merged") {
        return Err(refused(format!("{} is merged into {base} already.", own.branch)));
    }
    let Some(folder) = need.folder.as_deref().and_then(|f| std::fs::canonicalize(f).ok()).filter(|f| f.is_dir()) else { return Err(refused("This project has no folder on the server.")) };
    let _one = MAKING.lock().await;
    let top = PathBuf::from(git(&folder, &["rev-parse", "--show-toplevel"]).await.map_err(|_| refused("This project's folder is not a git repository."))?);
    let branch = own.branch.as_str();
    let tip = git(&top, &["rev-parse", "--verify", "-q", &format!("refs/heads/{branch}")]).await.map_err(|_| refused(format!("The branch {branch} no longer exists.")))?;
    // What is in the ticket's worktree and in no commit would be left behind.
    if let Some(path) = own.path.as_deref().filter(|p| Path::new(p).is_dir()) {
        let open = git(Path::new(path), &["status", "--porcelain"]).await.map_err(refused)?;
        let files = open.lines().count();
        if files > 0 {
            return Err(refused(format!("{files} file(s) in this ticket's worktree are not committed yet. Ask the owner to commit them first.")));
        }
    }
    let ahead: u32 = git(&top, &["rev-list", "--count", &format!("{base}..{branch}")]).await.map_err(|_| refused(format!("The branch {base} no longer exists.")))?.parse().unwrap_or(0);
    if ahead == 0 {
        return Err(refused(format!("{branch} has no commits that {base} lacks: there is nothing to {how}.")));
    }
    let number = need.number;
    let (commit, url, text) = if how == "merge" {
        // Only where the base is checked out and nothing else is going on: the project's folder.
        let on = git(&top, &["symbolic-ref", "-q", "--short", "HEAD"]).await.unwrap_or_default();
        if on != base {
            return Err(refused(format!("The project's folder is on {}, not on {base}. Switch it to {base} to merge there.", if on.is_empty() { "no branch" } else { &on })));
        }
        if !git(&top, &["status", "--porcelain", "--untracked-files=no"]).await.map_err(refused)?.is_empty() {
            return Err(refused("The project's folder has changes that are not committed. Commit or remove them before merging there."));
        }
        // The merge is this board member's: under their name, with the repository's e-mail when it has one.
        let email = git(&top, &["config", "--get", "user.email"]).await.ok().filter(|e| !e.is_empty()).unwrap_or_else(|| format!("{}@aurelius.invalid", member.username));
        let (name, mail, message) = (format!("user.name={}", member.username), format!("user.email={email}"), format!("Merge ticket #{number}: {}", need.title));
        if let Err(why) = git(&top, &["-c", &name, "-c", &mail, "merge", "--no-ff", "--no-edit", "-m", &message, "--", branch]).await {
            let conflicts = git(&top, &["diff", "--name-only", "--diff-filter=U"]).await.unwrap_or_default();
            let _ = git(&top, &["merge", "--abort"]).await;
            let files: Vec<&str> = conflicts.lines().take(8).collect();
            return Err(refused(if files.is_empty() { format!("The merge did not work and was undone: {why}") } else { format!("{branch} conflicts with {base} in {}. Nothing was merged. Ask the owner to bring the branch up to date with {base}.", files.join(", ")) }));
        }
        let commit = git(&top, &["rev-parse", "HEAD"]).await.map_err(refused)?;
        (commit, None, format!("{} merged {branch} into {base}", member.username))
    } else {
        let remote = match git(&top, &["config", "--get", &format!("branch.{base}.remote")]).await.ok().filter(|r| !r.is_empty() && r != ".") {
            Some(remote) => remote,
            None => git(&top, &["remote"]).await.unwrap_or_default().lines().next().map(str::to_owned).ok_or_else(|| refused("This repository has no remote to push to."))?,
        };
        git(&top, &["push", "--set-upstream", "--", &remote, &format!("refs/heads/{branch}:refs/heads/{branch}")]).await.map_err(|why| refused(format!("The push to {remote} did not work: {why}")))?;
        let url = git(&top, &["remote", "get-url", "--", &remote]).await.ok().and_then(|u| pull_request_url(&u, &base, branch));
        (tip, url, format!("{} pushed {branch} to {remote} for a pull request into {base}", member.username))
    };
    let landed = if how == "merge" { "merged" } else { "pushed" };
    let (s, c, u) = (shared.clone(), commit.clone(), url.clone());
    let notices = tokio::task::spawn_blocking(move || s.service.ticket_landed(ticket, user, landed, c, u, text))
        .await
        .map_err(|e| aurelius_protocol::ProtocolError::new(aurelius_domain::ErrorCode::Internal, e.to_string()))?
        .map_err(|e| aurelius_protocol::ProtocolError::new(aurelius_domain::ErrorCode::Internal, e.to_string()))?;
    shared.publish(notices);
    Ok(aurelius_protocol::TicketLanded { how: landed.to_owned(), branch: branch.to_owned(), base, commit, url })
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

    #[test]
    fn a_pull_request_opens_on_the_remotes_own_site() {
        let github = Some("https://github.com/acme/atlas/compare/main...ticket/12-fix-the-login?expand=1".to_owned());
        for remote in ["git@github.com:acme/atlas.git", "https://github.com/acme/atlas", "https://token@github.com/acme/atlas.git", "ssh://git@github.com/acme/atlas.git"] {
            assert_eq!(pull_request_url(remote, "main", "ticket/12-fix-the-login"), github, "{remote}");
        }
        let gitlab = pull_request_url("git@gitlab.example.com:team/atlas.git", "main", "ticket/1-x").unwrap();
        assert!(gitlab.starts_with("https://gitlab.example.com/team/atlas/-/merge_requests/new?") && gitlab.contains("ticket/1-x"), "{gitlab}");
        assert_eq!(pull_request_url("/srv/git/atlas.git", "main", "ticket/1-x"), None);
        assert_eq!(pull_request_url("https://git.example.com/acme/atlas.git", "main", "ticket/1-x"), None);
    }
}
