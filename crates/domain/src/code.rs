//! The Code tab of a workspace (ticket #77, feature `code_view`): the git state of a
//! project's folder as a board member sees it. Its worktrees, its local branches with
//! how far they are from main and who works on them, and the recent commits of all of
//! them for a graph. Read with `code.get`; changed only by the board member's own
//! `code.*` host writes (fetch, merge, remove a worktree, delete a branch, clean up
//! merged), never by a run. Nothing here is pushed.

use crate::{EmployeeId, ProjectId, TicketId};
use serde::{Deserialize, Serialize};

/// Most commits `code.get` lists for the graph (newest first, all branches).
pub const CODE_COMMITS_MAX: usize = 200;
/// Most local branches `code.get` lists (main first, then the newest commit first).
pub const CODE_BRANCHES_MAX: usize = 200;
/// Most commits a branch lists of its own (`main..branch`, newest first).
pub const CODE_BRANCH_COMMITS_MAX: usize = 20;
/// Most changed files a worktree names.
pub const CODE_FILES_MAX: usize = 50;
/// Longest a commit subject travels; a longer one is cut.
pub const CODE_SUBJECT_MAX_CHARS: usize = 200;

/// The git state of one project's folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeView {
    pub project_id: ProjectId,
    /// The project's folder (the main checkout).
    pub repo_path: String,
    /// The repository's main branch (`main`, else `master`); none: see `problem`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_branch: Option<String>,
    /// Why there is nothing to show, in words a board member can act on (no folder,
    /// not a git repository, git missing, no main branch). The lists are empty then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// Whether the repository has a remote at all (no remote: no Fetch).
    #[serde(default)]
    pub has_remote: bool,
    /// When the last `git fetch` ended (the time of `FETCH_HEAD`); none: never.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at_ms: Option<i64>,
    /// Every worktree git knows, the main checkout first.
    #[serde(default)]
    pub worktrees: Vec<CodeWorktree>,
    /// The local branches, main first, then the newest last commit first.
    #[serde(default)]
    pub branches: Vec<CodeBranch>,
    /// The newest commits of main and every listed branch, newest first (date order),
    /// at most `CODE_COMMITS_MAX`. A parent outside the list is cut off in the graph.
    #[serde(default)]
    pub commits: Vec<CodeCommit>,
}

/// One worktree of the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeWorktree {
    pub path: String,
    /// Its branch; none: a detached HEAD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The project's folder itself; never removed from here.
    #[serde(default)]
    pub is_main: bool,
    /// The commit it is on.
    pub head: String,
    /// Uncommitted changes (tracked and untracked files), at most `CODE_FILES_MAX` named.
    #[serde(default)]
    pub changed_files: Vec<String>,
    /// How many files have uncommitted changes (may be more than `changed_files` names).
    #[serde(default)]
    pub changed_count: u32,
    /// Git no longer finds its folder (`git worktree prune` removes it).
    #[serde(default)]
    pub missing: bool,
}

/// One local branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeBranch {
    pub name: String,
    /// Its newest commit.
    pub head: String,
    /// The repository's main branch.
    #[serde(default)]
    pub is_main: bool,
    /// Commits on this branch that main lacks (`main..branch`); 0 for main.
    pub ahead: u32,
    /// Commits on main this branch lacks (`branch..main`); 0 for main.
    pub behind: u32,
    /// Every commit of it is in main (it can be deleted without losing work).
    #[serde(default)]
    pub merged: bool,
    /// Its upstream (`origin/…`); for main: how far it is from it after a fetch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    #[serde(default)]
    pub upstream_ahead: u32,
    #[serde(default)]
    pub upstream_behind: u32,
    /// Its newest commit: subject, author name and time.
    pub subject: String,
    pub author: String,
    pub at_ms: i64,
    /// The worktree it is checked out in; none: no worktree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    /// The size of `main...branch` (what a merge would bring): files, lines added, removed.
    #[serde(default)]
    pub files_changed: u32,
    #[serde(default)]
    pub insertions: u32,
    #[serde(default)]
    pub deletions: u32,
    /// Its own commits (`main..branch`), newest first, at most `CODE_BRANCH_COMMITS_MAX`.
    #[serde(default)]
    pub commits: Vec<CodeCommitLine>,
    /// The ticket it belongs to: a quick task's branch, a work session's worktree, or a
    /// ticket number in its name (`feat/ticket-77-…`, `fix/77-…`) of this project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<CodeTicket>,
    /// Who works on it: the ticket's owner, else the employee of the work session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<CodeOwner>,
    /// An employee works on it now: a run of its ticket or work session is live or queued.
    #[serde(default)]
    pub working: bool,
}

/// A commit of a branch's own list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeCommitLine {
    pub hash: String,
    pub subject: String,
}

/// The ticket a branch belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeTicket {
    pub ticket_id: TicketId,
    pub number: u32,
    pub title: String,
}

/// Who works on a branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeOwner {
    pub employee_id: EmployeeId,
    pub name: String,
}

/// One commit of the graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeCommit {
    /// The full hash; the client shows the first seven characters.
    pub hash: String,
    /// Its parents' full hashes, the first parent first (two or more: a merge).
    #[serde(default)]
    pub parents: Vec<String>,
    pub subject: String,
    pub author: String,
    pub at_ms: i64,
    /// The listed local branches whose newest commit this is.
    #[serde(default)]
    pub branches: Vec<String>,
}

/// The ticket number a branch name carries, when it carries one: `ticket-<N>` anywhere
/// in it (`feat/ticket-77-code`, `quick/ticket-61`), else a number right after the
/// first slash followed by `-` or the end (`fix/73-rail-hit-area`, `fix/73`).
pub fn ticket_number_of(branch: &str) -> Option<u32> {
    let digits = |s: &str| -> Option<u32> {
        let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        let rest = &s[end..];
        if end == 0 || !(rest.is_empty() || rest.starts_with('-') || rest.starts_with('/')) {
            return None;
        }
        s[..end].parse().ok().filter(|n| *n > 0)
    };
    let lower = branch.to_ascii_lowercase();
    if let Some(n) = lower.match_indices("ticket-").find_map(|(i, m)| digits(&lower[i + m.len()..])) {
        return Some(n);
    }
    let (_, after) = lower.split_once('/')?;
    digits(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_name_carries_its_ticket_number() {
        assert_eq!(ticket_number_of("feat/ticket-77-code"), Some(77));
        assert_eq!(ticket_number_of("quick/ticket-61"), Some(61));
        assert_eq!(ticket_number_of("fix/73-rail-hit-area"), Some(73));
        assert_eq!(ticket_number_of("fix/73"), Some(73));
        assert_eq!(ticket_number_of("Feat/Ticket-12-x"), Some(12));
    }

    #[test]
    fn a_branch_name_without_a_ticket_number_has_none() {
        assert_eq!(ticket_number_of("main"), None);
        assert_eq!(ticket_number_of("spike/mobile-nav"), None);
        assert_eq!(ticket_number_of("feat/aur-mu-01-s3"), None);
        assert_eq!(ticket_number_of("chore/2fa"), None);
        assert_eq!(ticket_number_of("session/1a2b3c4d"), None);
        assert_eq!(ticket_number_of("fix/0-zero"), None);
    }
}
