//! The few reads that have no storage function yet; they belong in `aurelius_storage`
//! and are kept together here so they move there in one go.

use aurelius_domain::RunId;
use aurelius_storage::{Conn, StorageError};
use std::collections::HashMap;

/// What a running job works on, by name: the employee or chat of an answer, the task's title.
pub(super) fn job_names(c: &Conn, rows: &[aurelius_storage::queries::JobRow]) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for row in rows.iter().filter(|r| matches!(r.state, aurelius_domain::job::JobState::Ready | aurelius_domain::job::JobState::Leased)) {
        let name: Option<String> = if let Some(conversation) = row.payload["conversation_id"].as_str() {
            c.query_row(
                "SELECT COALESCE(m.name || CASE WHEN c.title <> '' AND c.title <> m.name THEN ' · ' || c.title ELSE '' END, NULLIF(c.title, ''))
                 FROM conversations c LEFT JOIN employees m ON m.id = c.employee_id WHERE c.id = ?1",
                [conversation],
                |r| r.get(0),
            )
            .ok()
            .flatten()
        } else if let Some(task) = row.payload["task_id"].as_str() {
            c.query_row("SELECT title FROM tasks WHERE id = ?1", [task], |r| r.get(0)).ok()
        } else {
            None
        };
        if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
            names.insert(row.id.clone(), name);
        }
    }
    names
}

/// The storage key of the context overview kept with a run.
pub(super) fn context_key(c: &Conn, run: RunId) -> Option<String> {
    c.query_row("SELECT b.storage_key FROM runs r JOIN blobs b ON b.id = r.context_blob_id WHERE r.id = ?1", [run.to_string()], |r| r.get(0)).ok()
}

/// Runs that were asked to stop and have not yet.
pub(super) fn cancelling_runs(c: &Conn) -> Result<Vec<RunId>, StorageError> {
    let mut q = c.prepare("SELECT id FROM runs WHERE status = 'cancelling'")?;
    let ids: Vec<String> = q.query_map([], |r| r.get::<_, String>(0))?.filter_map(Result::ok).collect();
    Ok(ids.iter().filter_map(|id| RunId::parse(id).ok()).collect())
}
