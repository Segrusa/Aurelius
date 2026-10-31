//! A task's conversation with its employee. When an employee takes a task on, a
//! conversation with that employee starts with the task as its context; what
//! happens in the task comes back there: the employee's plan and decisions,
//! findings of its subagents, results and questions of the colleagues it
//! delegated to. The user can read along and talk in it.

use crate::run::events::{Ev, emit};
use aurelius_domain::{ActorKind, ConversationId, EmployeeId, MessageId, TaskId, conversation::{MessageRole, MessageStatus}};
use aurelius_domain::EventNotice;
use aurelius_storage::{StorageError, Store, Tx, blobs, conversations as conv, employees, tasks as store};

/// Longest text posted at once (a colleague's result, a subagent's findings).
pub(crate) const MAX_POST: usize = 12_000;

/// Who says it in the task's conversation.
#[derive(Debug, Clone, Copy)]
pub enum Speaker<'a> {
    /// The task's employee (an answer of its own).
    Employee,
    /// An update: brief | finding | question | status, from someone else.
    Update { kind: &'a str, from: &'a str },
}

/// How long a finding is in the conversation; the full text stays in the task.
const SUMMARY_CHARS: usize = 600;

/// The gist of a result: without its list of actions, cut at a paragraph or
/// sentence end.
pub(crate) fn summary(text: &str, max: usize) -> String {
    let body = text.split("\n**Actions**").next().unwrap_or(text).trim();
    if body.chars().count() <= max {
        return body.to_owned();
    }
    let cut: String = body.chars().take(max).collect();
    let end = cut
        .rfind("\n\n")
        .filter(|&i| i > max / 3)
        .or_else(|| cut.rfind(". ").map(|i| i + 1))
        .or_else(|| cut.rfind('\n'))
        .unwrap_or(cut.len());
    format!("{}\n\n… (the full result is in the task)", cut[..end].trim_end())
}

fn capped(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= MAX_POST {
        return text.to_owned();
    }
    let cut: String = text.chars().take(MAX_POST).collect();
    format!("{cut}\n\n… (shortened; the full text is in the task)")
}

/// Post into a conversation about `task`.
pub(crate) fn post_to(tx: &Tx<'_>, blobs_store: &Store, conversation: ConversationId, task: TaskId, speaker: Speaker<'_>, text: &str) -> Result<EventNotice, StorageError> {
    post_with(tx, blobs_store, conversation, Some(task), speaker, text, text, serde_json::json!({})).map(|(_, n)| n)
}

/// Post, and the message it became (a run can answer it).
pub(crate) fn post_returning(tx: &Tx<'_>, blobs_store: &Store, conversation: ConversationId, task: TaskId, speaker: Speaker<'_>, text: &str) -> Result<(MessageId, EventNotice), StorageError> {
    post_with(tx, blobs_store, conversation, Some(task), speaker, text, text, serde_json::json!({}))
}

/// Post into a conversation an update that belongs to no task (the user's decision
/// on a colleague proposal); a run can answer it.
pub(crate) fn post_note(tx: &Tx<'_>, blobs_store: &Store, conversation: ConversationId, speaker: Speaker<'_>, text: &str) -> Result<(MessageId, EventNotice), StorageError> {
    post_with(tx, blobs_store, conversation, None, speaker, text, text, serde_json::json!({}))
}

/// A colleague's result on task `of`, posted in a conversation about `task`
/// (the delegating task's, or the conversation that asked for it); marked, so
/// a run that started before it knows it has not seen it.
pub(crate) fn post_result(tx: &Tx<'_>, blobs_store: &Store, conversation: ConversationId, task: TaskId, of: TaskId, speaker: Speaker<'_>, text: &str) -> Result<(MessageId, EventNotice), StorageError> {
    post_with(tx, blobs_store, conversation, Some(task), speaker, text, text, serde_json::json!({ "result_of": of }))
}

#[allow(clippy::too_many_arguments)]
fn post_with(tx: &Tx<'_>, blobs_store: &Store, conversation: ConversationId, task: Option<TaskId>, speaker: Speaker<'_>, text: &str, files_from: &str, extra: serde_json::Value) -> Result<(MessageId, EventNotice), StorageError> {
    // Files it mentions come along (from the full text), the text itself stays short.
    let task_row = match task {
        Some(task) => store::load_task(tx, task)?,
        None => None,
    };
    let files = match &task_row {
        Some(row) => crate::conversation::message_files::keep_for_project(tx, blobs_store, files_from, row.project_id)?,
        None => Vec::new(),
    };
    let text = match speaker {
        Speaker::Update { kind: "finding", .. } => summary(text, SUMMARY_CHARS),
        _ => capped(text),
    };
    let blob = blobs_store.blobs().put(text.as_bytes(), "text/markdown; charset=utf-8")?;
    blobs::register(tx, &blob)?;
    let (kind, from) = match speaker {
        Speaker::Employee => ("answer", ""),
        Speaker::Update { kind, from } => (kind, from),
    };
    let message = MessageId::new();
    let (event, mut notice) = emit(tx, Ev {
        aggregate_type: "conversation",
        aggregate_id: conversation.to_string(),
        event_type: "task.posted",
        actor: if matches!(speaker, Speaker::Employee) { ActorKind::Employee } else { ActorKind::System },
        actor_id: None,
        correlation: task.map_or_else(|| conversation.to_string(), |t| t.to_string()),
        payload: Some(blob.id.to_string()),
        metadata: serde_json::json!({ "task_id": task, "kind": kind, "from": from, "message_id": message }),
        dedupe: format!("task-post:{message}"),
    })?;
    notice.project_id = task_row.map(|t| t.project_id);
    match speaker {
        Speaker::Employee => {
            conv::insert_message(tx, conv::NewMessage {
                id: message,
                conversation_id: conversation,
                role: MessageRole::Assistant,
                content_blob_id: Some(blob.id.to_string()),
                source_event_id: event,
                status: MessageStatus::Complete,
                run_id: None,
                reply_to: None,
            })?;
        }
        Speaker::Update { kind, from } => {
            let mut meta = serde_json::json!({ "kind": kind, "from": from, "task_id": task });
            if let (Some(m), Some(e)) = (meta.as_object_mut(), extra.as_object()) {
                m.extend(e.clone());
            }
            conv::insert_update(tx, message, conversation, &blob.id.to_string(), event, &meta)?;
        }
    }
    conv::set_message_files(tx, message, &files)?;
    tx.execute("UPDATE conversations SET updated_at_ms = ?2 WHERE id = ?1", rusqlite::params![conversation.to_string(), aurelius_storage::now_ms()]).ok();
    Ok((message, notice))
}

/// Post into the task's own conversation, if it has one.
pub(crate) fn post(tx: &Tx<'_>, blobs_store: &Store, task: TaskId, speaker: Speaker<'_>, text: &str) -> Result<Option<EventNotice>, StorageError> {
    match store::task_conversation(tx, task)? {
        Some((conversation, _)) => post_to(tx, blobs_store, conversation, task, speaker, text).map(Some),
        None => Ok(None),
    }
}

/// The employee takes the task on: its conversation about the task, made once
/// per employee (a hand-over starts one with the new employee), opened with the
/// task as context. Returns the conversation and the notices.
/// Returns the conversation, the brief's message when it was made now, and the notices.
pub(crate) fn open_returning(tx: &Tx<'_>, blobs_store: &Store, task: TaskId, employee: EmployeeId, brief: &str) -> Result<(Option<ConversationId>, Option<MessageId>, Vec<EventNotice>), StorageError> {
    if let Some((conversation, Some(owner))) = store::task_conversation(tx, task)? {
        if owner == employee {
            return Ok((Some(conversation), None, vec![]));
        }
    }
    let Some(row) = store::load_task(tx, task)? else { return Ok((None, None, vec![])) };
    let conversation = ConversationId::new();
    if !employees::insert_thread(tx, employee, conversation, aurelius_domain::conversation::ConversationKind::Task, Some(row.project_id))? {
        return Ok((None, None, vec![]));
    }
    let title: String = row.title.chars().take(80).collect();
    tx.execute("UPDATE conversations SET title = ?2 WHERE id = ?1", rusqlite::params![conversation.to_string(), title])?;
    store::set_task_conversation(tx, task, conversation)?;
    // The mode chosen for the task, for this conversation only.
    match store::task_run(tx, task)?.mode {
        Some(mode) => {
            tx.execute("UPDATE conversations SET agent_mode = ?2 WHERE id = ?1", rusqlite::params![conversation.to_string(), mode.as_str()])?;
        }
        // Management that chooses its own mode starts in Plan (read only) and picks Auto with set_mode.
        None if aurelius_storage::management_mode::chooses_own_mode(tx, employee)? => {
            tx.execute("UPDATE conversations SET agent_mode = 'plan' WHERE id = ?1", [conversation.to_string()])?;
        }
        None => {}
    }
    let (_, mut started) = emit(tx, Ev {
        aggregate_type: "conversation",
        aggregate_id: conversation.to_string(),
        event_type: "conversation.started",
        actor: ActorKind::System,
        actor_id: None,
        correlation: task.to_string(),
        payload: None,
        metadata: serde_json::json!({ "employee_id": employee, "task_id": task }),
        dedupe: format!("task-thread:{task}:{conversation}"),
    })?;
    started.project_id = Some(row.project_id);
    let from = "Task";
    let (brief_message, brief_notice) = post_returning(tx, blobs_store, conversation, task, Speaker::Update { kind: "brief", from }, brief)?;
    Ok((Some(conversation), Some(brief_message), vec![started, brief_notice]))
}

/// The conversation a run answers in, its task, and who runs it.
fn run_conversation(conn: &rusqlite::Connection, run: aurelius_domain::RunId) -> Result<Option<(ConversationId, Option<TaskId>, String)>, StorageError> {
    use rusqlite::OptionalExtension;
    let row: Option<(Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT r.conversation_id, r.task_id, COALESCE(m.name, cm.name) FROM runs r
             LEFT JOIN employees m ON m.id = r.employee_id
             LEFT JOIN conversations c ON c.id = r.conversation_id
             LEFT JOIN employees cm ON cm.id = c.employee_id
             WHERE r.id = ?1",
            [run.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    Ok(row.and_then(|(conversation, task, who)| {
        let conversation = ConversationId::parse(&conversation?).ok()?;
        Some((conversation, task.and_then(|t| TaskId::parse(&t).ok()), who.unwrap_or_else(|| "Claude".into())))
    }))
}

/// Posts that belong to a run (`of_run`: its plan, question and the user's decision on
/// them) are in its session already: the run asked them and got the decision as a
/// tool result. Its answer moves below them.
///
/// A plan Claude made (ExitPlanMode): in full in the conversation the run answers
/// in, chat or task, so it stays readable there after it is decided.
pub(crate) fn post_plan(tx: &Tx<'_>, blobs_store: &Store, run: aurelius_domain::RunId, approval: aurelius_domain::ApprovalId, plan: &str) -> Result<Option<EventNotice>, StorageError> {
    let Some((conversation, task, who)) = run_conversation(tx, run)? else { return Ok(None) };
    post_with(tx, blobs_store, conversation, task, Speaker::Update { kind: "plan", from: &who }, plan, plan, serde_json::json!({ "approval_id": approval, "of_run": run })).map(|(_, n)| Some(n))
}

/// The plan of a ticket's main task, handed in for the board (no inbox approval:
/// `plan_review`); in full in the task's conversation, where the board reads it.
pub(crate) fn post_ticket_plan(tx: &Tx<'_>, blobs_store: &Store, run: aurelius_domain::RunId, plan: &str) -> Result<Option<EventNotice>, StorageError> {
    let Some((conversation, task, who)) = run_conversation(tx, run)? else { return Ok(None) };
    post_with(tx, blobs_store, conversation, task, Speaker::Update { kind: "plan", from: &who }, plan, plan, serde_json::json!({ "of_run": run, "plan_review": true })).map(|(_, n)| Some(n))
}

/// Claude asks the user (AskUserQuestion): the questions and their options in full
/// in the conversation the run answers in, chat or task, where they can be answered too.
pub(crate) fn post_question(tx: &Tx<'_>, blobs_store: &Store, run: aurelius_domain::RunId, approval: aurelius_domain::ApprovalId, text: &str) -> Result<Option<EventNotice>, StorageError> {
    let Some((conversation, task, who)) = run_conversation(tx, run)? else { return Ok(None) };
    post_with(tx, blobs_store, conversation, task, Speaker::Update { kind: "approval", from: &who }, text, text, serde_json::json!({ "approval_id": approval, "of_run": run })).map(|(_, n)| Some(n))
}

/// The user's decision on a plan or answer to a question, noted under it in the run's conversation.
pub(crate) fn post_run_decision(tx: &Tx<'_>, blobs_store: &Store, run: aurelius_domain::RunId, text: &str) -> Result<Option<EventNotice>, StorageError> {
    let Some((conversation, task, _)) = run_conversation(tx, run)? else { return Ok(None) };
    post_with(tx, blobs_store, conversation, task, Speaker::Update { kind: "status", from: "You" }, text, text, serde_json::json!({ "of_run": run })).map(|(_, n)| Some(n))
}

/// Claude's questions (AskUserQuestion) in full, as markdown: each question with
/// its header and options, so they stay readable after they are answered.
pub(crate) fn describe_questions(input: &serde_json::Value) -> String {
    let s = |v: &serde_json::Value| v.as_str().unwrap_or("").trim().to_owned();
    let questions: Vec<String> = input["questions"]
        .as_array()
        .map(|qs| {
            qs.iter()
                .filter(|q| !s(&q["question"]).is_empty())
                .map(|q| {
                    let header = s(&q["header"]);
                    let mut text = if header.is_empty() { s(&q["question"]) } else { format!("**{header}** · {}", s(&q["question"])) };
                    if q["multiSelect"].as_bool().unwrap_or(false) {
                        text.push_str(" *(one or more)*");
                    }
                    let options: Vec<String> = q["options"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|o| (s(&o["label"]), s(&o["description"])))
                        .filter(|(label, _)| !label.is_empty())
                        .map(|(label, description)| if description.is_empty() { format!("- **{label}**") } else { format!("- **{label}** — {description}") })
                        .collect();
                    if !options.is_empty() {
                        text = format!("{text}\n\n{}", options.join("\n"));
                    }
                    text
                })
                .collect()
        })
        .unwrap_or_default();
    if questions.is_empty() { "Asks you something.".into() } else { questions.join("\n\n") }
}
