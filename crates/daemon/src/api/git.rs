//! `git.get_status` and `git.get_diff`: the change in the folder a conversation works in.

use super::{internal, storage_task};
use crate::Shared;
use aurelius_domain::ConversationId;
use aurelius_protocol::{GitDiffView, GitStatusView, ProtocolError};
use std::sync::Arc;

/// Largest diff `git.get_diff` sends (well under MAX_FRAME_BYTES).
const MAX_DIFF_BYTES: usize = 512 * 1024;

/// Branch and uncommitted change of the folder the conversation works in
/// (its project folder, else the employee's own workspace).
pub(super) async fn status(shared: &Arc<Shared>, conversation: ConversationId) -> Result<Option<GitStatusView>, ProtocolError> {
    in_work_folder(shared, conversation, |dir| status_of(dir)).await
}

/// The uncommitted change itself, for a diff view.
pub(super) async fn diff(shared: &Arc<Shared>, conversation: ConversationId) -> Result<Option<GitDiffView>, ProtocolError> {
    in_work_folder(shared, conversation, |dir| {
        let status = status_of(dir)?;
        let (patch, truncated, untracked) = aurelius_tools::git::working_diff(dir, MAX_DIFF_BYTES).unwrap_or_default();
        Some(GitDiffView { status, patch, truncated, untracked })
    })
    .await
}

/// Runs `f` (blocking) in the first existing folder the conversation works in.
async fn in_work_folder<T: Send + 'static>(
    shared: &Arc<Shared>,
    conversation: ConversationId,
    f: impl FnOnce(&std::path::Path) -> Option<T> + Send + 'static,
) -> Result<Option<T>, ProtocolError> {
    let store = shared.store().clone();
    let folders = storage_task(move || store.read(|c| aurelius_storage::conversations::work_folders(c, conversation))).await?;
    tokio::task::spawn_blocking(move || folders.iter().map(std::path::Path::new).find(|d| d.is_dir()).and_then(f))
        .await
        .map_err(internal)
}

/// `None` when `dir` is not in a Git repository (or has no commit yet).
fn status_of(dir: &std::path::Path) -> Option<GitStatusView> {
    use aurelius_tools::git;
    let head = git::head(dir).ok()?;
    let stat = git::working_stat(dir).unwrap_or_default();
    Some(GitStatusView {
        branch: git::current_branch(dir).ok().flatten(),
        head: Some(head.chars().take(7).collect()),
        files: stat.files,
        insertions: stat.insertions,
        deletions: stat.deletions,
    })
}
