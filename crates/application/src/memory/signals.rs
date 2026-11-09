//! The gate before a lesson (Harness² A): what went wrong or was corrected in
//! a task, read from SQL without a model. Each signal keeps the event it comes
//! from (the lesson's provenance); `memory::gate_signals` labels them S1..S8.
//! A task without signals gets no lesson section in its extraction at all.

use aurelius_domain::{EventId, TaskId};
use aurelius_storage::StorageError;
use rusqlite::Connection;

/// The task's signals in the order they happened: (event, text), not yet cut or labelled.
///  - the user's own messages in the task's conversations (corrections), and a
///    message an employee sent the task;
///  - runs of the task that failed or were stopped, and runs that went again
///    after a transient error (requeues);
///  - actions and plans the user rejected or sent back for changes;
///  - parts delegated from it that failed, were cancelled or came back to the boss;
///  - the task's own failure.
pub fn task_signals(conn: &Connection, task: TaskId, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<(EventId, String)>, StorageError> {
    let task = task.to_string();
    let mut found: Vec<(i64, String, String)> = Vec::new();

    // Corrections: the user's messages (and an employee's message_task) in the task's conversations.
    {
        let mut stmt = conn.prepare(
            "SELECT e.seq, e.id, b.storage_key, m.role, json_extract(m.meta_json, '$.from')
             FROM messages m JOIN blobs b ON b.id = m.content_blob_id JOIN events e ON e.id = m.source_event_id
             WHERE m.conversation_id IN (SELECT conversation_id FROM runs WHERE task_id = ?1 AND conversation_id IS NOT NULL
                                         UNION SELECT conversation_id FROM tasks WHERE id = ?1 AND conversation_id IS NOT NULL)
               AND m.status = 'complete'
               AND (m.role = 'user' OR (m.role = 'tool' AND json_extract(m.meta_json, '$.kind') = 'message'))
             ORDER BY e.seq",
        )?;
        let rows = stmt.query_map([&task], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?)))?;
        for (seq, event, key, role, from) in rows.filter_map(Result::ok) {
            let Some(text) = read_blob(&key) else { continue };
            let who = if role == "user" { "The user wrote".to_string() } else { format!("{} wrote", from.unwrap_or_else(|| "A colleague".into())) };
            found.push((seq, event, format!("{who} during the task: {}", text.trim())));
        }
    }

    // Runs that failed, were stopped, or went again after a transient error.
    {
        let mut stmt = conn.prepare(
            "SELECT e.seq, e.id, e.type, COALESCE(json_extract(e.metadata_json, '$.reason'), r.error_code, '')
             FROM runs r JOIN events e ON e.aggregate_type = 'run' AND e.aggregate_id = r.id
             WHERE r.task_id = ?1 AND (e.type IN ('run.failed','run.cancelled') OR (e.type = 'run.waiting' AND json_extract(e.metadata_json, '$.reason') = 'retry'))
             ORDER BY e.seq",
        )?;
        let rows = stmt.query_map([&task], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))?;
        for (seq, event, kind, reason) in rows.filter_map(Result::ok) {
            let text = match kind.as_str() {
                "run.failed" if reason.is_empty() => "A run of the task failed.".to_string(),
                "run.failed" => format!("A run of the task failed ({reason})."),
                "run.cancelled" => "A run of the task was stopped before it finished.".to_string(),
                _ => "A run of the task had to be started again after an error.".to_string(),
            };
            found.push((seq, event, text));
        }
    }

    // Actions and plans the user rejected.
    {
        let mut stmt = conn.prepare(
            "SELECT e.seq, e.id, e.type, a.tool, a.answer_json, b.storage_key
             FROM approvals a JOIN runs r ON r.id = a.run_id JOIN events e ON e.id = a.decision_event_id
             LEFT JOIN blobs b ON b.id = a.action_blob_id
             WHERE r.task_id = ?1 AND a.status = 'rejected'
             ORDER BY e.seq",
        )?;
        let rows = stmt.query_map([&task], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?)))?;
        for (seq, event, kind, tool, answer, key) in rows.filter_map(Result::ok) {
            let feedback = answer.and_then(|a| serde_json::from_str::<serde_json::Value>(&a).ok()).and_then(|a| a["feedback"].as_str().map(str::to_owned)).filter(|f| !f.trim().is_empty());
            let text = match (kind.as_str(), feedback) {
                ("approval.plan_revision_requested", Some(f)) => format!("The user sent the plan back for changes: {f}"),
                ("approval.plan_revision_requested", None) => "The user sent the plan back for changes.".to_string(),
                ("approval.plan_rejected", Some(f)) => format!("The user rejected the plan: {f}"),
                ("approval.plan_rejected", None) => "The user rejected the plan.".to_string(),
                _ => {
                    let action = key.and_then(|k| read_blob(&k)).and_then(|a| serde_json::from_str::<serde_json::Value>(&a).ok()).map(|a| a["arguments"].to_string()).unwrap_or_default();
                    let tool = tool.strip_prefix(crate::run::approvals::RUNTIME_TOOL_PREFIX).unwrap_or(&tool).to_owned();
                    if action.is_empty() || action == "null" { format!("The user rejected {tool}.") } else { format!("The user rejected {tool}: {action}") }
                }
            };
            found.push((seq, event, text));
        }
    }

    // Parts delegated from the task that failed, were cancelled or came back to the boss; and the task's own failure.
    {
        let mut stmt = conn.prepare(
            "SELECT e.seq, e.id, e.type, t.id = ?1, t.title, COALESCE(json_extract(e.metadata_json, '$.reason'), ''), COALESCE(m.name, '')
             FROM tasks t JOIN events e ON e.aggregate_type = 'task' AND e.aggregate_id = t.id
             LEFT JOIN employees m ON m.id = t.employee_id
             WHERE (t.parent_task_id = ?1 AND (e.type IN ('task.failed','task.cancelled')
                    OR (e.type = 'task.waiting' AND json_extract(e.metadata_json, '$.reason') = 'escalated_to_boss')))
                OR (t.id = ?1 AND e.type = 'task.failed')
             ORDER BY e.seq",
        )?;
        let rows = stmt.query_map([&task], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, bool>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?)))?;
        for (seq, event, kind, own, title, reason, who) in rows.filter_map(Result::ok) {
            let why = if reason.is_empty() { String::new() } else { format!(" ({reason})") };
            let text = match (own, kind.as_str()) {
                (true, _) => format!("The task failed{why}."),
                (false, "task.failed") => format!("The part “{title}” given to {who} failed{why}."),
                (false, "task.cancelled") => format!("The part “{title}” given to {who} was cancelled{why}."),
                _ => format!("The part “{title}” given to {who} came back: it stopped and waits."),
            };
            found.push((seq, event, text));
        }
    }

    found.sort_by_key(|(seq, _, _)| *seq);
    Ok(found.into_iter().filter_map(|(_, event, text)| Some((EventId::parse(&event).ok()?, text))).collect())
}
