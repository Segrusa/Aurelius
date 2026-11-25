//! Where a run works (ticket #86): the worktree of its session or task
//! (`aurelius_storage::work_places`), or, for a colleague woken in a ticket's discussion,
//! the worktree of that ticket's main task. The daemon makes it before the run
//! (`quick::prepare_run`); the plan swaps the project's folder for it.

use crate::{CommandService, storage_error};
use aurelius_domain::{AppError, ConversationId, RunId, work_place::worktrees_dir};
use aurelius_storage::{StorageError, work_places};
use std::path::Path;

pub use aurelius_storage::work_places::{PlaceKind, WorkPlace};

/// The worktree a run of `conversation` works in (`run`: the one that may have been woken
/// in a ticket's discussion).
pub(crate) fn of_run(
    conn: &rusqlite::Connection,
    data_root: &Path,
    conversation: ConversationId,
    run: Option<RunId>,
) -> Result<Option<WorkPlace>, StorageError> {
    let worktrees = worktrees_dir(data_root);
    if let Some(place) = work_places::of_conversation(conn, &worktrees, conversation)? {
        return Ok(Some(place));
    }
    let Some(run) = run else { return Ok(None) };
    match crate::conversation::discussion::run_discussion(conn, run)?
        .and_then(|d| d.ticket().map(|t| t.id))
    {
        Some(ticket) => work_places::of_ticket(conn, &worktrees, ticket),
        None => Ok(None),
    }
}

/// What an employee is told about its worktree in a ticket or a task (a session and a
/// quick task have their own sections).
pub fn guidance(place: &WorkPlace) -> Option<String> {
    let (folder, branch) = (place.worktree.display(), &place.branch);
    let rules = "Commit your changes there yourself. Never work in the project's folder, never switch it to another branch, never merge into main and never push: the board member merges after review.";
    match &place.kind {
        PlaceKind::Ticket { quick: false, .. } => Some(format!(
            "\n\n## Where you work\nAurelius made this ticket's worktree {folder} on the branch {branch}; do the ticket's work only there (do not make another worktree). {rules} A part you delegate gets its own worktree, branched from {branch}: when its result names its branch, merge that branch into {branch} yourself (git merge in your worktree) before you report."
        )),
        PlaceKind::Part { .. } => Some(format!(
            "\n\n## Where you work\nAurelius made your own worktree {folder} on the branch {branch}, branched from the work of the task this was delegated from; work only there (do not make another worktree). {rules} Name the branch {branch} and your commits in your result, so the one who delegated this can merge it."
        )),
        PlaceKind::Task => Some(format!(
            "\n\n## Where you work\nAurelius made your own worktree {folder} on the branch {branch}; work only there (do not make another worktree). {rules} Name the branch in your result."
        )),
        PlaceKind::Ticket { quick: true, .. } | PlaceKind::Session { .. } => None,
    }
}

impl CommandService {
    /// The worktree a run of `conversation` works in, if any (`of_run`).
    pub fn work_place_of_run(
        &self,
        conversation: ConversationId,
        run: Option<RunId>,
    ) -> Result<Option<WorkPlace>, AppError> {
        let root = self.store().root().to_path_buf();
        self.store()
            .read(|c| of_run(c, &root, conversation, run))
            .map_err(storage_error)
    }

    /// A session that worked in the project's folder now has its own worktree (made by the daemon).
    pub fn move_session_to_worktree(
        &self,
        conversation: ConversationId,
        worktree: &Path,
        branch: &str,
    ) -> Result<(), AppError> {
        let (worktree, branch) = (worktree.to_path_buf(), branch.to_owned());
        self.store()
            .write(move |tx| work_places::move_session(tx, conversation, &worktree, &branch))
            .map_err(storage_error)
    }
}
