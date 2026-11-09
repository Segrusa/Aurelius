//! Conversation, message and run projections.

use crate::{StorageError, now_ms};
use aurelius_domain::{
    ConversationId, EmployeeId, EventId, MessageId, RunId, TaskId, UserId,
    conversation::{MessageRole, MessageStatus},
};
use aurelius_domain::views::{ConversationSummary, MessageView};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

fn conv_err<E: std::error::Error + Send + Sync + 'static>(e: E) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}

/// Next version for an aggregate (versions are gap-free).
pub fn next_version(tx: &Transaction<'_>, aggregate_type: &str, aggregate_id: &str) -> Result<u64, StorageError> {
    let current: i64 = tx.query_row(
        "SELECT COALESCE(MAX(aggregate_version), 0) FROM events WHERE aggregate_type = ?1 AND aggregate_id = ?2",
        params![aggregate_type, aggregate_id],
        |row| row.get(0),
    )?;
    Ok(current as u64 + 1)
}

/// The project of a project conversation (1.18). `None` for employee and task conversations.
pub fn project_of_conversation(conn: &rusqlite::Connection, id: ConversationId) -> Result<Option<aurelius_domain::ProjectId>, StorageError> {
    let project: Option<String> = conn
        .query_row("SELECT project_id FROM conversations WHERE id = ?1 AND kind = 'project' AND employee_id IS NULL", [id.to_string()], |r| r.get(0))
        .optional()?
        .flatten();
    Ok(project.and_then(|p| aurelius_domain::ProjectId::parse(&p).ok()))
}

pub fn insert_project_conversation(tx: &Transaction<'_>, id: ConversationId, project: aurelius_domain::ProjectId, title: &str) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES (?1, 'project', ?2, ?3, 1, ?4)",
        params![id.to_string(), project.to_string(), title, now_ms()],
    )?;
    Ok(())
}

/// The conversations of a project, most recently active first. Task
/// conversations are not among them: they belong to their task.
pub fn project_conversations(conn: &Connection, project: aurelius_domain::ProjectId) -> Result<Vec<aurelius_domain::views::ProjectConversationSummary>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.title, c.created_at_ms,
                COALESCE((SELECT MAX(m.created_at_ms) FROM messages m WHERE m.conversation_id = c.id), c.created_at_ms),
                (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id),
                EXISTS(SELECT 1 FROM runs r WHERE r.conversation_id = c.id AND r.status IN ('created','starting','running','waiting','cancelling'))
         FROM conversations c
         WHERE c.project_id = ?1 AND c.archived_at_ms IS NULL AND c.kind = 'project' AND c.employee_id IS NULL
         ORDER BY 4 DESC, c.created_at_ms DESC LIMIT 500",
    )?;
    let rows = stmt.query_map([project.to_string()], |r| {
        Ok(aurelius_domain::views::ProjectConversationSummary {
            id: ConversationId::parse(&r.get::<_, String>(0)?).map_err(conv_err)?,
            title: r.get(1)?,
            created_at_ms: r.get(2)?,
            updated_at_ms: r.get(3)?,
            message_count: r.get::<_, i64>(4)? as u32,
            working: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// A project conversation (no employee). The channels and ticket discussions are
/// not: they take messages only through their own writes (`group_chat.send`).
pub fn conversation_exists(tx: &Transaction<'_>, id: ConversationId) -> Result<bool, StorageError> {
    Ok(tx.query_row("SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1 AND employee_id IS NULL AND kind NOT IN ('group','ticket'))", [id.to_string()], |r| r.get(0))?)
}

/// The schema version whose migration makes the group chat.
pub const GROUP_CHAT_SCHEMA: i64 = 17;

/// Make the company's group chat (the general channel) again when its row is
/// missing (a database edited or restored by hand): migration 17 makes it once,
/// and `group_chat.get` only reads. Called when the store opens; true when it
/// had to make it.
pub fn ensure_group_conversation(conn: &Connection) -> Result<bool, StorageError> {
    let made = conn.execute(
        "INSERT INTO conversations(id, kind, title, version, created_at_ms)
         SELECT ?1, 'group', 'Team chat', 1, ?2 WHERE NOT EXISTS (SELECT 1 FROM conversations WHERE kind = 'group' AND project_id IS NULL)",
        params![ConversationId::new().to_string(), now_ms()],
    )?;
    Ok(made == 1)
}

/// The company's group chat, the general channel (migration 17 makes it). The
/// projects' channels: `discussions::channel`.
pub fn group_conversation(conn: &Connection) -> Result<Option<ConversationId>, StorageError> {
    let id: Option<String> =
        conn.query_row("SELECT id FROM conversations WHERE kind = 'group' AND project_id IS NULL ORDER BY created_at_ms, id LIMIT 1", [], |r| r.get(0)).optional()?;
    Ok(id.and_then(|c| ConversationId::parse(&c).ok()))
}

/// The messages of the group chat that employees read: the user's and the
/// employees' (kind `group`), complete. Notes of Aurelius are not among them.
const GROUP_MESSAGES: &str = "FROM messages m JOIN blobs b ON b.id = m.content_blob_id
     WHERE m.conversation_id = ?1 AND m.status = 'complete' AND (m.role = 'user' OR json_extract(m.meta_json, '$.kind') = 'group')";

/// The latest `limit` of those messages, before `before` when given, newest
/// first: (author, none for the user; storage key of the text).
pub fn latest_group_messages(conn: &Connection, group: ConversationId, before: Option<MessageId>, limit: u32) -> Result<Vec<(Option<String>, String)>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT CASE WHEN m.role = 'user' THEN NULL ELSE COALESCE(json_extract(m.meta_json, '$.from'), 'Aurelius') END, b.storage_key {GROUP_MESSAGES}
         AND (?2 IS NULL OR m.ordinal < (SELECT ordinal FROM messages WHERE id = ?2)) ORDER BY m.ordinal DESC LIMIT ?3"
    ))?;
    let rows = stmt.query_map(params![group.to_string(), before.map(|m| m.to_string()), limit], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// How many of those messages the group chat has (not the replies in its threads).
pub fn group_message_count(conn: &Connection, group: ConversationId) -> Result<u32, StorageError> {
    let own = if has_thread_column(conn)? { " AND m.thread_parent_id IS NULL" } else { "" };
    Ok(conn.query_row(&format!("SELECT COUNT(*) {GROUP_MESSAGES}{own}"), [group.to_string()], |r| r.get(0))?)
}

/// How many posts woke colleagues (`woke` in their meta): of run `run`, and of
/// employee `employee` since `since_ms`. Counted over every channel and ticket
/// discussion together (`discussions::waking_posts`); `_group` is no longer
/// used and goes with the discussion engine.
pub fn waking_posts(conn: &Connection, _group: ConversationId, run: RunId, employee: EmployeeId, since_ms: i64) -> Result<(u32, u32), StorageError> {
    crate::discussions::waking_posts(conn, run, employee, since_ms)
}

/// A run woken by the group chat: the `group` of its job's payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GroupRun {
    /// The group chat its answer is copied to.
    pub conversation_id: ConversationId,
    /// The message its answer becomes there (streamed under this id while it runs).
    pub message_id: MessageId,
    /// How many employees woke each other before it (0: woken by the user).
    pub hop: u32,
    /// The mode the run started in, shown with its answer in the group. Set
    /// when the run starts (`run::plan`), never in the job's payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<aurelius_domain::autonomy::AgentMode>,
    /// The board member of hop 0 (who wrote the message that started it): every
    /// run woken from it, at any hop, runs on their Claude login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_user: Option<UserId>,
    /// The thread its answer goes into (its root in `conversation_id`): it was
    /// woken by a reply there (feature `chat_threads`). None: the channel itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_parent: Option<MessageId>,
}

/// The conversation a run answers in.
pub fn run_conversation(conn: &Connection, run: RunId) -> Result<Option<ConversationId>, StorageError> {
    let id: Option<Option<String>> = conn.query_row("SELECT conversation_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).optional()?;
    match id.flatten().and_then(|c| ConversationId::parse(&c).ok()) {
        Some(id) => Ok(Some(id)),
        None => crate::inbox::queued_conversation(conn, run),
    }
}

/// The group chat marking of a run, if the group chat woke it.
pub fn group_run(conn: &Connection, run: RunId) -> Result<Option<GroupRun>, StorageError> {
    let marking: Option<Option<String>> = conn
        .query_row("SELECT json_extract(payload_json, '$.group') FROM jobs WHERE id = ?1", [run.to_string()], |r| r.get(0))
        .optional()?;
    // A queued item stopped before it became a run has no job, only its inbox item.
    let marking = match marking.flatten() {
        Some(m) => Some(m),
        None => crate::inbox::group_marking(conn, run)?,
    };
    Ok(marking.and_then(|m| serde_json::from_str(&m).ok()))
}

pub struct NewMessage {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub role: MessageRole,
    pub content_blob_id: Option<String>,
    pub source_event_id: EventId,
    pub status: MessageStatus,
    pub run_id: Option<RunId>,
    pub reply_to: Option<MessageId>,
}

pub fn insert_message(tx: &Transaction<'_>, m: NewMessage) -> Result<u32, StorageError> {
    insert_message_by(tx, m, None)
}

/// `insert_message` with the board member who wrote it (`author_user_id`,
/// schema 19). Without one the column is left out, so an older schema (the
/// v0.6 import, fixtures) takes the message as before.
pub fn insert_message_by(tx: &Transaction<'_>, m: NewMessage, author: Option<UserId>) -> Result<u32, StorageError> {
    let ordinal: i64 = tx.query_row(
        "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM messages WHERE conversation_id = ?1",
        [m.conversation_id.to_string()],
        |r| r.get(0),
    )?;
    let role = match m.role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    };
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(m.id.to_string()),
        Box::new(m.conversation_id.to_string()),
        Box::new(ordinal),
        Box::new(role),
        Box::new(m.content_blob_id),
        Box::new(m.source_event_id.to_string()),
        Box::new(m.status.as_str()),
        Box::new(now_ms()),
        Box::new(m.run_id.map(|r| r.to_string())),
        Box::new(m.reply_to.map(|r| r.to_string())),
    ];
    let column = with_author(&mut values, author);
    tx.execute(
        &format!(
            "INSERT INTO messages(id, conversation_id, ordinal, role, content_blob_id, source_event_id, status,
                 input_kind, created_at_ms, run_id, reply_to_id{column})
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'text', ?8, ?9, ?10{})",
            if column.is_empty() { "" } else { ", ?11" }
        ),
        rusqlite::params_from_iter(values.iter()),
    )?;
    Ok(ordinal as u32)
}

/// A task update in a conversation (see `MessageView::update`): complete at once.
pub fn insert_update(
    tx: &Transaction<'_>,
    id: MessageId,
    conversation: ConversationId,
    content_blob_id: &str,
    source_event: aurelius_domain::EventId,
    meta: &serde_json::Value,
) -> Result<u32, StorageError> {
    insert_update_by(tx, id, conversation, content_blob_id, source_event, meta, None)
}

/// `insert_update` with the board member it is on behalf of (`author_user_id`,
/// schema 19), e.g. a system line of a discussion for a board member's move.
/// Without one the column is left out, as in `insert_message_by`.
pub fn insert_update_by(
    tx: &Transaction<'_>,
    id: MessageId,
    conversation: ConversationId,
    content_blob_id: &str,
    source_event: aurelius_domain::EventId,
    meta: &serde_json::Value,
    author: Option<UserId>,
) -> Result<u32, StorageError> {
    let ordinal: i64 = tx.query_row("SELECT COALESCE(MAX(ordinal), 0) + 1 FROM messages WHERE conversation_id = ?1", [conversation.to_string()], |r| r.get(0))?;
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(id.to_string()),
        Box::new(conversation.to_string()),
        Box::new(ordinal),
        Box::new(content_blob_id.to_owned()),
        Box::new(source_event.to_string()),
        Box::new(now_ms()),
        Box::new(meta.to_string()),
    ];
    let column = with_author(&mut values, author);
    tx.execute(
        &format!(
            "INSERT INTO messages(id, conversation_id, ordinal, role, content_blob_id, source_event_id, status, input_kind, created_at_ms, meta_json{column})
             VALUES (?1, ?2, ?3, 'tool', ?4, ?5, 'complete', 'tool', ?6, ?7{})",
            if column.is_empty() { "" } else { ", ?8" }
        ),
        rusqlite::params_from_iter(values.iter()),
    )?;
    Ok(ordinal as u32)
}

/// The board member as the last value of a message insert: its column, or
/// nothing without one (an older schema has no `author_user_id`).
fn with_author(values: &mut Vec<Box<dyn rusqlite::ToSql>>, author: Option<UserId>) -> &'static str {
    match author {
        Some(user) => {
            values.push(Box::new(user.to_string()));
            ", author_user_id"
        }
        None => "",
    }
}

pub fn update_message(
    tx: &Transaction<'_>,
    id: MessageId,
    content_blob_id: Option<&str>,
    status: MessageStatus,
) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE messages SET content_blob_id = COALESCE(?2, content_blob_id), status = ?3 WHERE id = ?1",
        params![id.to_string(), content_blob_id, status.as_str()],
    )?;
    Ok(())
}

/// The user message a retry answers again; must belong to the conversation.
pub fn user_message_in(tx: &Transaction<'_>, conversation: ConversationId, message: MessageId) -> Result<bool, StorageError> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM messages WHERE id = ?1 AND conversation_id = ?2 AND role = 'user')",
        params![message.to_string(), conversation.to_string()],
        |r| r.get(0),
    )?)
}

pub fn insert_run<C: serde::Serialize + RunConfigInfo>(
    tx: &Transaction<'_>,
    run_id: RunId,
    conversation_id: ConversationId,
    resolved: &C,
) -> Result<u32, StorageError> {
    let attempt: i64 = tx.query_row(
        "SELECT COUNT(*) + 1 FROM runs WHERE conversation_id = ?1",
        [conversation_id.to_string()],
        |r| r.get(0),
    )?;
    tx.execute(
        "INSERT INTO runs(id, conversation_id, attempt, runtime_kind, connection_id, model_id, resolved_config_json, status, login_user)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'created', ?8)",
        params![
            run_id.to_string(),
            conversation_id.to_string(),
            attempt,
            resolved.runtime_kind(),
            resolved.connection(),
            resolved.model().unwrap_or_default(),
            serde_json::to_string(resolved)?,
            resolved.login_user()
        ],
    )?;
    Ok(attempt as u32)
}

/// Whose Claude login a run uses (for a run not made by `insert_run`).
pub fn set_run_login_user(tx: &Transaction<'_>, run_id: RunId, login_user: Option<&str>) -> Result<(), StorageError> {
    tx.execute("UPDATE runs SET login_user = ?2 WHERE id = ?1", params![run_id.to_string(), login_user])?;
    Ok(())
}

/// Whose Claude login a run uses; none for a run from before phase 3 or one without.
pub fn run_login_user(conn: &rusqlite::Connection, run_id: RunId) -> Result<Option<String>, StorageError> {
    Ok(conn.query_row("SELECT login_user FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).optional()?.flatten())
}

pub fn run_status(tx: &rusqlite::Connection, run_id: RunId) -> Result<Option<String>, StorageError> {
    if let Some(status) = tx.query_row("SELECT status FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).optional()? {
        return Ok(Some(status));
    }
    // Still an item in its employee's inbox: the run has not been made yet.
    Ok(crate::inbox::is_queued(tx, run_id)?.then(|| "created".to_string()))
}

/// The error code a run carries (`quota` while it waits for the usage limit).
pub fn run_error(tx: &rusqlite::Connection, run_id: RunId) -> Result<Option<String>, StorageError> {
    Ok(tx.query_row("SELECT error_code FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).optional()?.flatten())
}

pub fn set_run_status(tx: &Transaction<'_>, run_id: RunId, status: &str, error_code: Option<&str>) -> Result<(), StorageError> {
    let now = now_ms();
    tx.execute(
        "UPDATE runs SET status = ?2, error_code = COALESCE(?3, error_code),
             started_at_ms = CASE WHEN ?2 = 'running' THEN ?4 ELSE started_at_ms END,
             ended_at_ms = CASE WHEN ?2 IN ('succeeded','failed','cancelled','unknown') THEN ?4 ELSE ended_at_ms END
         WHERE id = ?1",
        params![run_id.to_string(), status, error_code, now],
    )?;
    if matches!(status, "succeeded" | "failed" | "cancelled" | "unknown") {
        crate::inbox::finish_run(tx, run_id)?;
    }
    Ok(())
}

/// A decided approval lets a waiting run go on. Only from 'waiting': a run
/// stopped or ended meanwhile keeps its status.
pub fn resume_run(tx: &Transaction<'_>, run_id: RunId) -> Result<(), StorageError> {
    tx.execute("UPDATE runs SET status = 'running' WHERE id = ?1 AND status = 'waiting'", [run_id.to_string()])?;
    Ok(())
}

/// Runs that were active when the daemon stopped.
pub fn unfinished_runs(tx: &Transaction<'_>) -> Result<Vec<(RunId, ConversationId)>, StorageError> {
    let mut stmt = tx.prepare("SELECT id, conversation_id FROM runs WHERE status IN ('starting','running','waiting','cancelling') AND conversation_id IS NOT NULL
           AND (status <> 'waiting' OR EXISTS (SELECT 1 FROM jobs j WHERE j.id = runs.id AND j.state = 'leased'))")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            RunId::parse(&r.get::<_, String>(0)?).map_err(conv_err)?,
            ConversationId::parse(&r.get::<_, String>(1)?).map_err(conv_err)?,
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn message_for_run(tx: &Transaction<'_>, run_id: RunId) -> Result<Option<MessageId>, StorageError> {
    Ok(tx
        .query_row("SELECT id FROM messages WHERE run_id = ?1", [run_id.to_string()], |r| r.get::<_, String>(0))
        .optional()?
        .and_then(|t| MessageId::parse(&t).ok()))
}

/// Where in its answer a run used its tools, as byte offsets, in order: what it
/// said between them is working notes, what follows the last one is its closing text.
pub fn tool_offsets_of_run(conn: &Connection, run: RunId) -> Result<Vec<u32>, StorageError> {
    let mut stmt = conn.prepare("SELECT text_offset FROM run_activity WHERE run_id = ?1 ORDER BY text_offset")?;
    let rows = stmt.query_map([run.to_string()], |r| Ok(r.get::<_, i64>(0)?.max(0) as u32))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The blob of what a board member typed in a task's own conversation, when `run`
/// answers such a message (and not an update, such as a copy from a discussion).
pub fn typed_question_of_run(conn: &Connection, run: RunId) -> Result<Option<String>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT b.storage_key FROM messages a JOIN messages q ON q.id = a.reply_to_id JOIN blobs b ON b.id = q.content_blob_id
             WHERE a.run_id = ?1 AND q.role = 'user'",
            [run.to_string()],
            |r| r.get(0),
        )
        .optional()?)
}

/// An employee's chats (not its task conversations), most recently active first.
pub fn employee_conversations(conn: &Connection, employee: aurelius_domain::EmployeeId) -> Result<Vec<ConversationSummary>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.title, c.created_at_ms,
                COALESCE((SELECT MAX(m.created_at_ms) FROM messages m WHERE m.conversation_id = c.id), c.created_at_ms),
                (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id)
         FROM conversations c WHERE c.kind = 'employee' AND c.employee_id = ?1 AND c.archived_at_ms IS NULL AND c.quick_answer = 0 AND c.thread_key IS NULL AND c.board_member_id IS NULL
         ORDER BY 4 DESC, c.created_at_ms DESC",
    )?;
    let rows = stmt.query_map([employee.to_string()], |r| {
        Ok(ConversationSummary {
            id: ConversationId::parse(&r.get::<_, String>(0)?).map_err(conv_err)?,
            title: r.get(1)?,
            created_at_ms: r.get(2)?,
            updated_at_ms: r.get(3)?,
            message_count: r.get::<_, i64>(4)? as u32,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The persistent chat of an employee (its first chat): carries mode, heartbeat and summaries.
pub fn is_main_employee_conversation(conn: &Connection, conversation: ConversationId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM conversations c WHERE c.id = ?1 AND c.kind = 'employee' AND c.employee_id IS NOT NULL
           AND c.id = (SELECT id FROM conversations WHERE employee_id = c.employee_id AND kind = 'employee' ORDER BY created_at_ms, id LIMIT 1))",
        [conversation.to_string()],
        |r| r.get(0),
    )?)
}

/// An older chat of an employee (converted from protocol 2): history, it takes no messages.
pub fn is_older_employee_chat(conn: &Connection, conversation: ConversationId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM conversations c WHERE c.id = ?1 AND c.kind = 'employee' AND c.board_member_id IS NULL
           AND c.id <> (SELECT id FROM conversations WHERE employee_id = c.employee_id AND kind = 'employee' ORDER BY created_at_ms, id LIMIT 1))",
        [conversation.to_string()],
        |r| r.get(0),
    )?)
}

/// The employee and project of a conversation (each none when it has none).
pub fn owner_of(conn: &Connection, conversation: ConversationId) -> Result<(Option<aurelius_domain::EmployeeId>, Option<aurelius_domain::ProjectId>), StorageError> {
    let row: Option<(Option<String>, Option<String>)> =
        conn.query_row("SELECT employee_id, project_id FROM conversations WHERE id = ?1", [conversation.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    let (employee, project) = row.unwrap_or_default();
    Ok((employee.and_then(|e| aurelius_domain::EmployeeId::parse(&e).ok()), project.and_then(|p| aurelius_domain::ProjectId::parse(&p).ok())))
}

/// All messages of a conversation in order, without the replies in its threads
/// (`thread.read` has those). `read_blob` resolves content.
pub fn list_messages(
    conn: &Connection,
    conversation: ConversationId,
    read_blob: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<MessageView>, StorageError> {
    messages_of(conn, conversation, None, read_blob)
}

/// The thread of `root` in `conversation`: the root first, then its replies in order.
pub fn list_thread(conn: &Connection, conversation: ConversationId, root: MessageId, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<MessageView>, StorageError> {
    messages_of(conn, conversation, Some(root), read_blob)
}

/// The messages of `conversation` in order: those of the conversation itself
/// (`thread` none), or the root `thread` and its replies.
fn messages_of(conn: &Connection, conversation: ConversationId, thread: Option<MessageId>, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<MessageView>, StorageError> {
    // A store before schema 26 has no threads.
    let threads = has_thread_column(conn)?;
    // Each filter names ?2 (the root, or NULL), so the statement always takes both parameters.
    let (parent, only) = match (threads, thread) {
        (true, None) => ("m.thread_parent_id", "?2 IS NULL AND m.thread_parent_id IS NULL"),
        (true, Some(_)) => ("m.thread_parent_id", "(m.id = ?2 OR m.thread_parent_id = ?2)"),
        (false, None) => ("NULL", "?2 IS NULL"),
        (false, Some(_)) => ("NULL", "m.id = ?2"),
    };
    // A store before schema 29 has no edits.
    let edits = if has_edit_columns(conn)? { "m.edited_at_ms, m.deleted_at_ms" } else { "NULL, NULL" };
    let mut stmt = conn.prepare(&format!(
        "SELECT m.id, m.ordinal, m.role, m.status, b.storage_key, m.created_at_ms,
                COALESCE(m.run_id, CASE WHEN json_extract(m.meta_json, '$.kind') = 'group' THEN json_extract(m.meta_json, '$.run_id') END), m.reply_to_id,
                c.display_name, r.model_id, json_extract(r.resolved_config_json, '$.persona_name'),
                json_extract(m.meta_json, '$.kind'), json_extract(m.meta_json, '$.from'), json_extract(m.meta_json, '$.approval_id'), json_extract(m.meta_json, '$.files'), json_extract(m.meta_json, '$.uploads'),
                json_extract(m.meta_json, '$.mode'), json_extract(m.meta_json, '$.system'), {parent}, {edits}
         FROM messages m LEFT JOIN blobs b ON b.id = m.content_blob_id
         LEFT JOIN runs r ON r.id = m.run_id LEFT JOIN connections c ON c.id = r.connection_id
         WHERE m.conversation_id = ?1 AND {only} ORDER BY m.ordinal"
    ))?;
    let rows = stmt.query_map(params![conversation.to_string(), thread.map(|t| t.to_string())], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, Option<String>>(9)?,
            r.get::<_, Option<String>>(10)?,
            r.get::<_, Option<String>>(11)?,
            r.get::<_, Option<String>>(12)?,
            r.get::<_, Option<String>>(13)?,
            r.get::<_, Option<String>>(14)?,
            r.get::<_, Option<String>>(15)?,
            r.get::<_, Option<String>>(16)?,
            r.get::<_, Option<String>>(17)?,
            r.get::<_, Option<String>>(18)?,
            r.get::<_, Option<i64>>(19)?,
            r.get::<_, Option<i64>>(20)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, ordinal, role, status, key, created, run, reply, connection, model, persona, update, from, approval, files, uploads, mode, system, parent, edited, deleted) = row?;
        let produced_by = match (connection, model.filter(|m| !m.is_empty())) {
            (Some(c), Some(m)) => Some(format!("{c} · {m}")),
            (Some(c), None) => Some(format!("{c} · default model")),
            (None, _) => None,
        };
        let id = MessageId::parse(&id).map_err(|e| StorageError::Rejected(e.to_string()))?;
        // A deleted message shows no attachments; the rows and blobs stay.
        let (files, uploads) = if deleted.is_some() { (None, None) } else { (files, uploads) };
        let mut uploads: Vec<aurelius_domain::views::MessageFileView> = uploads.and_then(|f| serde_json::from_str(&f).ok()).unwrap_or_default();
        aurelius_domain::conversation::with_upload_ids(aurelius_domain::conversation::UploadOwner::Message(id), &mut uploads);
        out.push(MessageView {
            id,
            ordinal: ordinal as u32,
            role: if role == "user" { MessageRole::User } else { MessageRole::Assistant },
            status: MessageStatus::parse(&status).unwrap_or(MessageStatus::Failed),
            text: key.as_deref().and_then(read_blob).unwrap_or_default(),
            created_at_ms: created,
            run_id: run.and_then(|t| RunId::parse(&t).ok()),
            reply_to: reply.and_then(|t| MessageId::parse(&t).ok()),
            produced_by,
            author: if role == "tool" { from } else { persona },
            sources: Vec::new(),
            activity: Vec::new(),
            update: if role == "tool" { Some(update.unwrap_or_else(|| "status".into())) } else { None },
            approval_id: approval.and_then(|a| aurelius_domain::ApprovalId::parse(&a).ok()),
            files: files.and_then(|f| serde_json::from_str(&f).ok()).unwrap_or_default(),
            uploads,
            queued: false,
            usage: None,
            mode: mode.and_then(|m| serde_json::from_value(serde_json::Value::String(m)).ok()),
            author_user: None,
            system: system.and_then(|s| serde_json::from_str(&s).ok()),
            thread_parent: parent.and_then(|p| MessageId::parse(&p).ok()),
            thread_reply_count: None,
            thread_last_reply_ms: None,
            thread_unread: None,
            thread_repliers: Vec::new(),
            reactions: Vec::new(),
            edited_at_ms: edited,
            deleted_at_ms: deleted,
            editable: false,
            awaiting_user: false,
            run: None,
        });
    }
    // The board members who wrote in a discussion (schema 19).
    if crate::discussions::has_discussions(conn)? {
        let authors = crate::discussions::message_author_users(conn, conversation)?;
        if !authors.is_empty() {
            let users: std::collections::HashMap<String, aurelius_domain::user::UserView> =
                crate::users::list(conn)?.iter().filter_map(|u| Some((u.id.clone(), u.view()?))).collect();
            for message in out.iter_mut() {
                message.author_user = authors.get(&message.id).and_then(|u| users.get(&u.to_string())).cloned();
            }
        }
    }
    let waiting = queued_messages(conn, conversation)?;
    for message in out.iter_mut() {
        message.queued = message.role == MessageRole::User && waiting.contains(&message.id);
        // A post stands for its run in the run card, which reads the rest apart.
        if let (Some(run), false) = (message.run_id, is_post(message)) {
            message.sources = crate::files::sources_for_run(conn, run)?;
            if message.role == MessageRole::Assistant {
                message.usage = crate::usage::for_run(conn, run)?;
            }
        }
        message.activity = activity_for_message(conn, message.id)?;
    }
    with_run_cards(conn, &mut out)?;
    Ok(out)
}

fn is_post(m: &MessageView) -> bool {
    m.update.as_deref() == Some(aurelius_domain::group_chat::GROUP_KIND) && m.system.is_none()
}

/// A run's outcome in a discussion: its `Reported` line (the run is found through
/// the line's task, as it was when the line was written).
fn reported_task(m: &MessageView) -> Option<TaskId> {
    m.system.as_ref().filter(|s| s.kind == aurelius_domain::views::SystemLineKind::Reported)?.task_id
}

/// The run card of each run in `messages`, on the run's last post or `Reported` line:
/// the rest of the run, its tokens and whether it waits for a board member.
fn with_run_cards(conn: &Connection, messages: &mut [MessageView]) -> Result<(), StorageError> {
    if !messages.iter().any(|m| is_post(m) || reported_task(m).is_some()) {
        return Ok(());
    }
    let pending = crate::approvals::pending_runs(conn)?;
    let mut seen = std::collections::HashSet::new();
    for m in messages.iter_mut().rev() {
        if m.deleted_at_ms.is_some() {
            continue;
        }
        if m.run_id.is_none()
            && let Some(task) = reported_task(m)
        {
            m.run_id = run_of_task_at(conn, task, m.created_at_ms)?;
        }
        if !is_post(m) && reported_task(m).is_none() {
            continue;
        }
        let Some(run) = m.run_id.filter(|r| seen.insert(*r)) else { continue };
        m.run = run_brief(conn, run)?;
        m.usage = crate::usage::for_run(conn, run)?;
        m.awaiting_user = pending.contains(&run) || (aurelius_domain::group_chat::asks_question(&m.text) && run_ended_waiting(conn, run)?);
    }
    Ok(())
}

/// The latest run of `task` that started by `at_ms`.
fn run_of_task_at(conn: &Connection, task: TaskId, at_ms: i64) -> Result<Option<RunId>, StorageError> {
    let run: Option<String> = conn
        .query_row(
            "SELECT r.id FROM runs r JOIN tasks t ON t.id = ?1
             WHERE (r.task_id = t.id OR r.conversation_id = t.conversation_id) AND COALESCE(r.started_at_ms, 0) <= ?2
             ORDER BY r.started_at_ms DESC LIMIT 1",
            params![task.to_string(), at_ms],
            |r| r.get(0),
        )
        .optional()?;
    Ok(run.and_then(|r| RunId::parse(&r).ok()))
}

fn run_brief(conn: &Connection, run: RunId) -> Result<Option<aurelius_domain::views::RunBrief>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT r.status, (SELECT COUNT(*) FROM run_activity a WHERE a.run_id = r.id), (SELECT COUNT(*) FROM run_activity a WHERE a.run_id = r.id AND a.ok = 0)
             FROM runs r WHERE r.id = ?1",
            [run.to_string()],
            |r| Ok(aurelius_domain::views::RunBrief { status: r.get(0)?, tool_calls: r.get::<_, i64>(1)? as u32, failed_calls: r.get::<_, i64>(2)? as u32 }),
        )
        .optional()?)
}

/// Whether `run` is the last one of a task that waits for a board member's answer.
fn run_ended_waiting(conn: &Connection, run: RunId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM runs r JOIN tasks t ON t.id = r.task_id OR t.conversation_id = r.conversation_id
           WHERE r.id = ?1 AND t.status = 'waiting' AND t.waiting_reason = 'input'
             AND NOT EXISTS(SELECT 1 FROM runs n WHERE (n.task_id = t.id OR n.conversation_id = t.conversation_id) AND n.started_at_ms > r.started_at_ms))",
        [run.to_string()],
        |r| r.get(0),
    )?)
}

/// Every tool call of `run` (`run.get_activity`), each cut as `conversation.get` cuts it.
pub fn activity_for_run(conn: &Connection, run: RunId) -> Result<aurelius_domain::views::RunActivity, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT call_id, tool, input_json, output, ok, text_offset, message_id FROM run_activity WHERE run_id = ?1 ORDER BY started_at_ms, rowid",
    )?;
    let rows = stmt.query_map([run.to_string()], |r| {
        Ok((
            aurelius_domain::views::ActivityView {
                call_id: r.get(0)?,
                tool: r.get(1)?,
                input: serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default(),
                output: r.get(3)?,
                ok: r.get::<_, Option<i64>>(4)?.map(|v| v == 1),
                offset: r.get::<_, i64>(5)?.max(0) as u32,
                truncated: false,
                output_bytes: None,
            },
            r.get::<_, String>(6)?,
        ))
    })?;
    let mut calls = Vec::new();
    let mut message_id = None;
    for row in rows {
        let (call, message) = row?;
        message_id = message_id.or_else(|| MessageId::parse(&message).ok());
        calls.push(aurelius_domain::activity::preview(call));
    }
    Ok(aurelius_domain::views::RunActivity { run_id: run, message_id, calls })
}

/// Whether `messages` has `thread_parent_id` (schema 26).
pub(crate) fn has_thread_column(conn: &Connection) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name = 'thread_parent_id'", [], |r| r.get::<_, i64>(0))? > 0)
}

/// Whether `messages` has `edited_at_ms` and `deleted_at_ms` (schema 29).
pub(crate) fn has_edit_columns(conn: &Connection) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name = 'edited_at_ms'", [], |r| r.get::<_, i64>(0))? > 0)
}

/// Tool use during one answer, in order.
pub fn activity_for_message(conn: &Connection, message: MessageId) -> Result<Vec<aurelius_domain::views::ActivityView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT call_id, tool, input_json, output, ok, text_offset FROM run_activity WHERE message_id = ?1 ORDER BY started_at_ms, rowid",
    )?;
    let rows = stmt.query_map([message.to_string()], |r| {
        Ok(aurelius_domain::views::ActivityView {
            call_id: r.get(0)?,
            tool: r.get(1)?,
            input: serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default(),
            output: r.get(3)?,
            ok: r.get::<_, Option<i64>>(4)?.map(|v| v == 1),
            offset: r.get::<_, i64>(5)?.max(0) as u32,
            truncated: false,
            output_bytes: None,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// One tool call of an answer, whole (`activity.get`).
pub fn activity_call(conn: &Connection, message: MessageId, call_id: &str) -> Result<Option<aurelius_domain::views::ActivityView>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT call_id, tool, input_json, output, ok, text_offset FROM run_activity WHERE message_id = ?1 AND call_id = ?2 LIMIT 1",
            params![message.to_string(), call_id],
            |r| {
                Ok(aurelius_domain::views::ActivityView {
                    call_id: r.get(0)?,
                    tool: r.get(1)?,
                    input: serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default(),
                    output: r.get(3)?,
                    ok: r.get::<_, Option<i64>>(4)?.map(|v| v == 1),
                    offset: r.get::<_, i64>(5)?.max(0) as u32,
                    truncated: false,
                    output_bytes: None,
                })
            },
        )
        .optional()?)
}

/// `parent`: the Task/Agent call of the subagent that used the tool, if any.
#[allow(clippy::too_many_arguments)]
pub fn activity_started(tx: &Transaction<'_>, run: RunId, message: MessageId, call_id: &str, tool: &str, input: &serde_json::Value, parent: Option<&str>, offset: u32) -> Result<(), StorageError> {
    tx.execute(
        "INSERT OR IGNORE INTO run_activity(run_id, call_id, message_id, tool, input_json, text_offset, started_at_ms, parent_call_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![run.to_string(), call_id, message.to_string(), tool, input.to_string(), offset, now_ms(), parent],
    )?;
    Ok(())
}

pub fn activity_finished(tx: &Transaction<'_>, run: RunId, call_id: &str, ok: bool, output: &str) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE run_activity SET ok = ?3, output = ?4, ended_at_ms = ?5 WHERE run_id = ?1 AND call_id = ?2",
        params![run.to_string(), call_id, ok as i64, output, now_ms()],
    )?;
    Ok(())
}

/// What `insert_run` needs from a resolved run configuration.
pub trait RunConfigInfo {
    fn runtime_kind(&self) -> &'static str;
    fn connection(&self) -> Option<String>;
    fn model(&self) -> Option<String>;
    /// The board member whose Claude login the run uses (`runs.login_user`).
    fn login_user(&self) -> Option<String> {
        None
    }
}


/// Working mode of a conversation (chat or employee).
pub fn agent_mode(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<aurelius_domain::autonomy::AgentMode, StorageError> {
    let text: Option<String> = conn
        .query_row("SELECT agent_mode FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0))
        .optional()?;
    Ok(aurelius_domain::autonomy::AgentMode::parse(text.as_deref().unwrap_or("ask")))
}

/// Set the mode of one conversation. Each thread with an employee keeps its own;
/// the employee's main conversation also governs its task steps and heartbeat.
pub fn set_agent_mode(tx: &Transaction<'_>, conversation: ConversationId, mode: aurelius_domain::autonomy::AgentMode) -> Result<bool, StorageError> {
    Ok(tx.execute("UPDATE conversations SET agent_mode = ?2 WHERE id = ?1", rusqlite::params![conversation.to_string(), mode.as_str()])? == 1)
}

/// The model and effort chosen for this conversation (unset: the employee's).
pub fn conversation_run(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<aurelius_domain::task::RunSettings, StorageError> {
    let json: Option<String> = conn.query_row("SELECT run_json FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0)).optional()?.flatten();
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
}

pub fn set_conversation_run(tx: &Transaction<'_>, conversation: ConversationId, run: &aurelius_domain::task::RunSettings) -> Result<bool, StorageError> {
    let json = (!run.is_empty()).then(|| serde_json::to_string(run).unwrap_or_default());
    Ok(tx.execute("UPDATE conversations SET run_json = ?2 WHERE id = ?1", rusqlite::params![conversation.to_string(), json])? == 1)
}

/// The conversation's messages that were posted for a run while it answered (its
/// plan, question, and the decision on them: `of_run` in their meta), with that run.
pub fn run_posts(conn: &Connection, conversation: ConversationId) -> Result<std::collections::HashMap<String, String>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT id, json_extract(meta_json, '$.of_run') FROM messages
         WHERE conversation_id = ?1 AND meta_json IS NOT NULL AND json_extract(meta_json, '$.of_run') IS NOT NULL",
    )?;
    let rows = stmt.query_map([conversation.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The run's answer continues below what was posted into its conversation
/// meanwhile (a plan and the decision on it): its message moves to the end.
pub fn move_run_message_to_end(tx: &Transaction<'_>, run_id: RunId) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE messages SET ordinal = (SELECT MAX(o.ordinal) + 1 FROM messages o WHERE o.conversation_id = messages.conversation_id)
         WHERE run_id = ?1 AND status = 'partial'",
        [run_id.to_string()],
    )?;
    Ok(())
}

/// Where an employee's conversation works: a task conversation in its project
/// folder, then the employee's own workspace; an employee chat (no project) in
/// the employee's own folder only (either may be missing). A standalone chat
/// never works in one, so it has no Git status or terminal either.
pub fn work_folders(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<Vec<String>, StorageError> {
    // A task variant's conversation works in its own worktree, and only there.
    if let Some(probe) = crate::variants::probe_of_conversation(conn, conversation)? {
        return Ok(probe.worktree.into_iter().collect());
    }
    let found: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT p.folder, m.folder FROM conversations c LEFT JOIN projects p ON p.id = c.project_id JOIN employees m ON m.id = c.employee_id
             WHERE c.id = ?1 AND c.employee_id IS NOT NULL",
            [conversation.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((p, m)) = found {
        return Ok([p, m].into_iter().flatten().collect());
    }
    // A project conversation works in the project's folder alone.
    let project: Option<Option<String>> = conn
        .query_row(
            "SELECT p.folder FROM conversations c JOIN projects p ON p.id = c.project_id WHERE c.id = ?1 AND c.kind = 'project' AND c.employee_id IS NULL",
            [conversation.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(project.flatten().into_iter().collect())
}

/// Whether the agent of this conversation may use the user's browser (off by default).
pub fn browser_allowed(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<bool, StorageError> {
    let allowed: Option<bool> = conn
        .query_row("SELECT browser_allowed FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0))
        .optional()?;
    Ok(allowed.unwrap_or(false))
}

/// Allow or forbid the browser; for an employee it applies to all of its conversations.
pub fn set_browser_allowed(tx: &Transaction<'_>, conversation: ConversationId, allowed: bool) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE conversations SET browser_allowed = ?2
         WHERE id = ?1 OR (employee_id IS NOT NULL AND employee_id = (SELECT employee_id FROM conversations WHERE id = ?1))",
        rusqlite::params![conversation.to_string(), allowed],
    )? >= 1)
}

/// Whether the agent of this conversation may look at the user's screen (off by default).
pub fn screen_allowed(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<bool, StorageError> {
    let allowed: Option<bool> = conn
        .query_row("SELECT screen_allowed FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0))
        .optional()?;
    Ok(allowed.unwrap_or(false))
}

/// Whether the conversation behind a run (or its employee's first one, for task steps) allows the screen.
pub fn screen_for_run(conn: &rusqlite::Connection, run: RunId) -> Result<bool, StorageError> {
    let allowed: Option<bool> = conn
        .query_row(
            "SELECT COALESCE(c1.screen_allowed, c2.screen_allowed) FROM runs r
             LEFT JOIN conversations c1 ON c1.id = r.conversation_id
             LEFT JOIN conversations c2 ON c2.id = (SELECT id FROM conversations WHERE employee_id = r.employee_id AND kind = 'employee' ORDER BY created_at_ms, id LIMIT 1)
             WHERE r.id = ?1",
            [run.to_string()],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(allowed.unwrap_or(false))
}

/// Allow or forbid the screen; for an employee it applies to all of its conversations.
pub fn set_screen_allowed(tx: &Transaction<'_>, conversation: ConversationId, allowed: bool) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE conversations SET screen_allowed = ?2
         WHERE id = ?1 OR (employee_id IS NOT NULL AND employee_id = (SELECT employee_id FROM conversations WHERE id = ?1))",
        rusqlite::params![conversation.to_string(), allowed],
    )? >= 1)
}

/// Mode that governs a run: its own conversation, or the employee's.
pub fn mode_for_run(conn: &rusqlite::Connection, run: RunId) -> Result<aurelius_domain::autonomy::AgentMode, StorageError> {
    use aurelius_domain::autonomy::AgentMode;
    let text: Option<String> = conn
        .query_row(
            "SELECT COALESCE(c1.agent_mode, c2.agent_mode) FROM runs r
             LEFT JOIN conversations c1 ON c1.id = r.conversation_id
             LEFT JOIN conversations c2 ON c2.id = (SELECT id FROM conversations WHERE employee_id = r.employee_id AND kind = 'employee' ORDER BY created_at_ms, id LIMIT 1)
             WHERE r.id = ?1",
            [run.to_string()],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(AgentMode::parse(text.as_deref().unwrap_or("ask")))
}

/// Set the mode of the conversation a run answers in (e.g. after its plan was
/// approved). Task steps without their own conversation leave the employee's alone.
pub fn set_mode_for_run(tx: &Transaction<'_>, run: RunId, mode: aurelius_domain::autonomy::AgentMode) -> Result<Option<ConversationId>, StorageError> {
    let conversation: Option<String> = tx.query_row("SELECT conversation_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).optional()?.flatten();
    let Some(conversation) = conversation.and_then(|c| ConversationId::parse(&c).ok()) else { return Ok(None) };
    Ok(set_agent_mode(tx, conversation, mode)?.then_some(conversation))
}


/// Blob keys holding a chat's content: messages, attachments, shared snapshots.
pub fn content_blob_keys(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<Vec<String>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT b.storage_key FROM messages m JOIN blobs b ON b.id = m.content_blob_id WHERE m.conversation_id = ?1
         UNION SELECT b.storage_key FROM attachments a JOIN blobs b ON b.id IN (a.original_blob_id, a.text_blob_id) WHERE a.conversation_id = ?1
         UNION SELECT b.storage_key FROM shares s JOIN blobs b ON b.id = s.snapshot_blob_id WHERE s.destination_conversation_id = ?1
         UNION SELECT b.storage_key FROM message_sources ms JOIN runs r ON r.id = ms.run_id JOIN blobs b ON b.id = ms.text_blob_id WHERE r.conversation_id = ?1
         UNION SELECT b.storage_key FROM runs r JOIN blobs b ON b.id = r.context_blob_id WHERE r.conversation_id = ?1
         UNION SELECT b.storage_key FROM events e JOIN blobs b ON b.id = e.payload_blob_id
               WHERE (e.aggregate_type = 'conversation' AND e.aggregate_id = ?1)
                  OR e.correlation_id IN (SELECT id FROM runs WHERE conversation_id = ?1)
                  OR (e.aggregate_type = 'run' AND e.aggregate_id IN (SELECT id FROM runs WHERE conversation_id = ?1))",
    )?;
    let rows = stmt.query_map([conversation.to_string()], |r| r.get::<_, String>(0))?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// Hide a chat for good and stop everything that pointed into it.
pub fn delete_conversation(tx: &Transaction<'_>, conversation: ConversationId) -> Result<(), StorageError> {
    let id = conversation.to_string();
    tx.execute("UPDATE conversations SET archived_at_ms = ?2, title = '' WHERE id = ?1", params![id, now_ms()])?;
    tx.execute("DELETE FROM run_activity WHERE message_id IN (SELECT id FROM messages WHERE conversation_id = ?1)", params![id])?;
    tx.execute("UPDATE shares SET revoked_at_ms = ?2 WHERE destination_conversation_id = ?1 AND revoked_at_ms IS NULL", params![id, now_ms()])?;
    tx.execute("UPDATE attachments SET removed_at_ms = ?2 WHERE conversation_id = ?1 AND removed_at_ms IS NULL", params![id, now_ms()])?;
    crate::traces::delete_for_conversation(tx, conversation)?;
    Ok(())
}

/// Runs of the conversation that are queued or still going, with their status.
pub fn unfinished_runs_in(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<Vec<(RunId, String)>, StorageError> {
    let mut out = started_or_made_runs_in(conn, conversation)?;
    // What waits in its employee's inbox has no run yet; it is the run that will answer.
    for (run, _) in crate::inbox::queued_in(conn, conversation)? {
        if !out.iter().any(|(known, _)| *known == run) {
            out.push((run, "created".to_string()));
        }
    }
    Ok(out)
}

fn started_or_made_runs_in(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<Vec<(RunId, String)>, StorageError> {
    let mut stmt =
        conn.prepare("SELECT id, status FROM runs WHERE conversation_id = ?1 AND status IN ('created','starting','running','waiting','cancelling')")?;
    let rows = stmt.query_map([conversation.to_string()], |r| Ok((RunId::parse(&r.get::<_, String>(0)?).map_err(conv_err)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// When the run that is going in the conversation started (milliseconds), if one
/// is: a queued run does not count, it has not begun. `Some(None)` when it is
/// going but has no start time yet. A message posted there waits for this run.
pub fn going_run_since(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<Option<Option<i64>>, StorageError> {
    let mut stmt = conn.prepare("SELECT started_at_ms FROM runs WHERE conversation_id = ?1 AND status IN ('starting','running','waiting','cancelling') ORDER BY started_at_ms LIMIT 1")?;
    Ok(stmt.query_row([conversation.to_string()], |r| r.get::<_, Option<i64>>(0)).optional()?)
}

/// Since when `employee` is busy in a run in any of its conversations (a quick
/// answer does not count): `Some(None)` when one is going but not timed yet.
pub fn employee_going_since(conn: &rusqlite::Connection, employee: aurelius_domain::EmployeeId) -> Result<Option<Option<i64>>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT r.started_at_ms FROM runs r JOIN conversations c ON c.id = r.conversation_id
         WHERE c.employee_id = ?1 AND c.quick_answer = 0 AND r.status IN ('starting','running','waiting','cancelling') ORDER BY r.started_at_ms LIMIT 1",
    )?;
    Ok(stmt.query_row([employee.to_string()], |r| r.get::<_, Option<i64>>(0)).optional()?)
}

/// The title of the hidden conversation of an employee's quick answers.
pub const QUICK_ANSWER_TITLE: &str = "Quick answers";

/// The hidden conversation an employee's quick answers run in (made on first use,
/// then reused): its own conversation, because the dispatcher leases one run per
/// conversation and the employee's own chat is busy. `None` for an employee
/// without a chat. It is marked `quick_answer`, so no list shows it and its runs
/// have no tools.
pub fn quick_answer_chat(tx: &Transaction<'_>, employee: aurelius_domain::EmployeeId) -> Result<Option<ConversationId>, StorageError> {
    let found: Option<String> = tx
        .query_row("SELECT id FROM conversations WHERE employee_id = ?1 AND quick_answer = 1 ORDER BY created_at_ms, id LIMIT 1", [employee.to_string()], |r| r.get(0))
        .optional()?;
    if let Some(id) = found {
        return Ok(ConversationId::parse(&id).ok());
    }
    let id = ConversationId::new();
    let made = tx.execute(
        "INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms, quick_answer)
         SELECT ?1, 'employee', NULL, e.id, ?3, 1, ?4, 1 FROM employees e WHERE e.id = ?2
           AND EXISTS (SELECT 1 FROM conversations c WHERE c.employee_id = e.id AND c.kind = 'employee' AND c.quick_answer = 0)",
        params![id.to_string(), employee.to_string(), QUICK_ANSWER_TITLE, now_ms()],
    )?;
    Ok((made == 1).then_some(id))
}

/// Whether the conversation is an employee's hidden quick-answer conversation.
pub fn is_quick_answer(conn: &Connection, conversation: ConversationId) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1 AND quick_answer = 1)", [conversation.to_string()], |r| r.get(0))?)
}

/// Whether `employee` (by name) already posted a full answer in `discussion` since the
/// message that started the quick run in `quick_chat`: then the long run has answered
/// and a quick answer would only repeat it, late and out of date.
pub fn answered_since_quick_question(conn: &Connection, discussion: ConversationId, quick_chat: ConversationId, employee: &str) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM messages m WHERE m.conversation_id = ?1 AND json_extract(m.meta_json, '$.from') = ?3 AND json_extract(m.meta_json, '$.quick') IS NULL
           AND m.created_at_ms >= COALESCE((SELECT MAX(q.created_at_ms) FROM messages q WHERE q.conversation_id = ?2 AND q.role != 'assistant'), 0))",
        rusqlite::params![discussion.to_string(), quick_chat.to_string(), employee],
        |r| r.get(0),
    )?)
}

/// What the quick answers of all employees cost since `since_ms`, in micro-units
/// of the usage records (a record without a cost counts for nothing).
pub fn quick_answer_cost_since(conn: &Connection, since_ms: i64) -> Result<i64, StorageError> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(u.cost_microunits), 0) FROM usage_records u JOIN runs r ON r.id = u.run_id
         JOIN conversations c ON c.id = r.conversation_id WHERE c.quick_answer = 1 AND u.recorded_at_ms >= ?1",
        [since_ms],
        |r| r.get(0),
    )?)
}

/// Where the answer to a user message stands, for changing a waiting message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyState {
    /// Its run is queued and not claimed yet: the message still waits.
    Queued(RunId),
    /// Its run was claimed or runs, or an answer to it exists.
    Started,
    /// No run will answer it (none was queued, or it was stopped).
    NotQueued,
}

/// The queued, unclaimed `run.chat` runs of a conversation with the message
/// each answers. A retry of an answered message does not make it wait again.
fn queued_runs(conn: &Connection, conversation: ConversationId) -> Result<Vec<(RunId, MessageId)>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT r.id, json_extract(j.payload_json, '$.reply_to') FROM runs r JOIN jobs j ON j.id = r.id
         WHERE r.conversation_id = ?1 AND r.status = 'created' AND j.kind = 'run.chat' AND j.state = 'ready'
           AND NOT EXISTS (SELECT 1 FROM messages a WHERE a.conversation_id = ?1 AND a.reply_to_id = json_extract(j.payload_json, '$.reply_to'))",
    )?;
    let rows = stmt.query_map([conversation.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (run, message) = row?;
        if let (Ok(run), Some(Ok(message))) = (RunId::parse(&run), message.as_deref().map(MessageId::parse)) {
            out.push((run, message));
        }
    }
    // Waiting in the employee's inbox: the worker has not made its run yet.
    for (run, message) in crate::inbox::queued_in(conn, conversation)? {
        let Some(Ok(message)) = message.as_deref().map(MessageId::parse) else { continue };
        let answered: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM messages a WHERE a.conversation_id = ?1 AND a.reply_to_id = ?2)",
            params![conversation.to_string(), message.to_string()],
            |r| r.get(0),
        )?;
        if !answered {
            out.push((run, message));
        }
    }
    Ok(out)
}

/// User messages of the conversation whose answer has not started yet.
pub fn queued_messages(conn: &Connection, conversation: ConversationId) -> Result<std::collections::HashSet<MessageId>, StorageError> {
    Ok(queued_runs(conn, conversation)?.into_iter().map(|(_, m)| m).collect())
}

/// Where the answer to `message` stands; `None` when it is not a user message of the conversation.
pub fn reply_state(conn: &Connection, conversation: ConversationId, message: MessageId) -> Result<Option<ReplyState>, StorageError> {
    let user: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM messages WHERE id = ?1 AND conversation_id = ?2 AND role = 'user')",
        params![message.to_string(), conversation.to_string()],
        |r| r.get(0),
    )?;
    if !user {
        return Ok(None);
    }
    if let Some((run, _)) = queued_runs(conn, conversation)?.into_iter().find(|(_, m)| *m == message) {
        return Ok(Some(ReplyState::Queued(run)));
    }
    let started: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM messages WHERE conversation_id = ?1 AND reply_to_id = ?2)
             OR EXISTS(SELECT 1 FROM runs r JOIN jobs j ON j.id = r.id
                       WHERE r.conversation_id = ?1 AND j.kind = 'run.chat' AND json_extract(j.payload_json, '$.reply_to') = ?2
                         AND (r.status IN ('starting','running','waiting','cancelling','succeeded','failed','unknown') OR j.state = 'leased'))
             OR EXISTS(SELECT 1 FROM employee_inbox i WHERE i.conversation_id = ?1 AND json_extract(i.payload_json, '$.reply_to') = ?2 AND i.state IN ('claimed', 'done'))",
        params![conversation.to_string(), message.to_string()],
        |r| r.get(0),
    )?;
    Ok(Some(if started { ReplyState::Started } else { ReplyState::NotQueued }))
}

/// Unfinished runs of the conversation ahead of `run` (which answers the
/// message at `ordinal`): everything except `run` itself and the queued,
/// unclaimed runs answering a later message.
pub fn runs_ahead_of(conn: &Connection, conversation: ConversationId, run: RunId, ordinal: u32) -> Result<Vec<(RunId, String)>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.status FROM runs r LEFT JOIN jobs j ON j.id = r.id
         LEFT JOIN messages m ON m.id = json_extract(j.payload_json, '$.reply_to')
         WHERE r.conversation_id = ?1 AND r.id <> ?2 AND r.status IN ('created','starting','running','waiting','cancelling')
           AND NOT (r.status = 'created' AND j.state = 'ready' AND COALESCE(m.ordinal, 0) > ?3)",
    )?;
    let rows = stmt.query_map(params![conversation.to_string(), run.to_string(), ordinal], |r| {
        Ok((RunId::parse(&r.get::<_, String>(0)?).map_err(conv_err)?, r.get::<_, String>(1)?))
    })?;
    let mut ahead: Vec<(RunId, String)> = rows.collect::<Result<_, _>>()?;
    // What waits in the employee's inbox for a message up to this one is ahead too.
    let mut stmt = conn.prepare(
        "SELECT i.run_id FROM employee_inbox i LEFT JOIN messages m ON m.id = json_extract(i.payload_json, '$.reply_to')
         WHERE i.conversation_id = ?1 AND i.run_id <> ?2 AND i.state = 'queued' AND COALESCE(m.ordinal, 0) <= ?3",
    )?;
    let queued = stmt.query_map(params![conversation.to_string(), run.to_string(), ordinal], |r| r.get::<_, String>(0))?;
    for item in queued {
        let item = RunId::parse(&item?).map_err(|e| StorageError::Rejected(e.to_string()))?;
        if !ahead.iter().any(|(known, _)| *known == item) {
            ahead.push((item, "created".to_string()));
        }
    }
    Ok(ahead)
}

/// A message's place in its conversation.
pub fn message_ordinal(conn: &Connection, message: MessageId) -> Result<Option<u32>, StorageError> {
    Ok(conn.query_row("SELECT ordinal FROM messages WHERE id = ?1", [message.to_string()], |r| r.get::<_, i64>(0)).optional()?.map(|o| o as u32))
}

/// Remove a user message that was never answered (its event stays in the log).
pub fn delete_message(tx: &Transaction<'_>, message: MessageId) -> Result<(), StorageError> {
    tx.execute("DELETE FROM messages WHERE id = ?1 AND role = 'user'", [message.to_string()])?;
    Ok(())
}

/// A message whose text was just replaced: where it is, and the blob that held the
/// old text, for the event of the change to carry (the blob itself stays).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacedText {
    pub conversation_id: ConversationId,
    /// None: the message had no text yet.
    pub previous_blob: Option<String>,
}

/// Point `message` at the registered `blob` and set `column` to `at_ms`.
/// A deleted message stays as it is: none, as for an unknown message.
fn replace_blob(tx: &Transaction<'_>, message: MessageId, blob: &str, column: &str, at_ms: i64) -> Result<Option<ReplacedText>, StorageError> {
    let row: Option<(String, Option<String>)> = tx
        .query_row("SELECT conversation_id, content_blob_id FROM messages WHERE id = ?1 AND deleted_at_ms IS NULL", [message.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((conversation, previous_blob)) = row else { return Ok(None) };
    let conversation_id = ConversationId::parse(&conversation).map_err(|e| StorageError::Rejected(e.to_string()))?;
    tx.execute(&format!("UPDATE messages SET content_blob_id = ?2, {column} = ?3 WHERE id = ?1"), params![message.to_string(), blob, at_ms])?;
    Ok(Some(ReplacedText { conversation_id, previous_blob }))
}

/// Replace the text of `message` by the registered `blob` and mark it edited at
/// `at_ms`. Who may do so is the command's to check. Wakes nothing, writes no event.
pub fn replace_message_text(tx: &Transaction<'_>, message: MessageId, blob: &str, at_ms: i64) -> Result<Option<ReplacedText>, StorageError> {
    replace_blob(tx, message, blob, "edited_at_ms", at_ms)
}

/// Mark `message` deleted at `at_ms`: its row stays, and its text becomes the
/// registered `placeholder_blob`. Who may do so is the command's to check. Unlike
/// `delete_message`, which removes a user message that was never answered.
pub fn mark_message_deleted(tx: &Transaction<'_>, message: MessageId, placeholder_blob: &str, at_ms: i64) -> Result<Option<ReplacedText>, StorageError> {
    replace_blob(tx, message, placeholder_blob, "deleted_at_ms", at_ms)
}

/// A copy of a discussion message in a task conversation (`to_owner` posts one
/// for the ticket's owner): the copy and the storage key of its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskCopy {
    pub message: MessageId,
    /// None: the copy has no text.
    pub text_key: Option<String>,
}

/// The live copies in task conversations of the discussion message `source`
/// (those whose meta names it as `from_message`). A deleted copy is not listed.
pub fn task_copies_of(conn: &Connection, source: MessageId) -> Result<Vec<TaskCopy>, StorageError> {
    let deleted = if has_edit_columns(conn)? { "AND m.deleted_at_ms IS NULL" } else { "" };
    let mut stmt = conn.prepare(&format!(
        "SELECT m.id, b.storage_key FROM messages m LEFT JOIN blobs b ON b.id = m.content_blob_id
         WHERE json_extract(m.meta_json, '$.from_message') = ?1 {deleted} ORDER BY m.ordinal"
    ))?;
    let rows = stmt.query_map([source.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (id, text_key) = row?;
        out.push(TaskCopy { message: MessageId::parse(&id).map_err(|e| StorageError::Rejected(e.to_string()))?, text_key });
    }
    Ok(out)
}

/// Drop the files and uploads kept with `message`, as a delete does: a deleted
/// message serves no attachment (the rows and blobs stay).
pub fn clear_message_attachments(tx: &Transaction<'_>, message: MessageId) -> Result<(), StorageError> {
    tx.execute("UPDATE messages SET meta_json = json_remove(meta_json, '$.files', '$.uploads') WHERE id = ?1 AND meta_json IS NOT NULL", [message.to_string()])?;
    Ok(())
}

/// The dedupe key of the event of a decision's feedback, `cmd:<id>:feedback`
/// (`conversation::discussion::decision_feedback`), as a LIKE pattern.
const DECISION_FEEDBACK: &str = "cmd:%:feedback";

/// Whether `message` is the feedback a board member gave with a decision on a
/// ticket. Told by the event it came from, so it also holds for older rows and
/// after an edit (the event stays).
pub fn is_decision_feedback(conn: &Connection, message: MessageId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM messages m JOIN events e ON e.id = m.source_event_id WHERE m.id = ?1 AND e.dedupe_key LIKE ?2)",
        params![message.to_string(), DECISION_FEEDBACK],
        |r| r.get(0),
    )?)
}

/// The messages of `conversation` for which `is_decision_feedback` holds, for a
/// read of the whole conversation.
pub fn decision_feedback_in(conn: &Connection, conversation: ConversationId) -> Result<std::collections::HashSet<MessageId>, StorageError> {
    let mut stmt = conn.prepare("SELECT m.id FROM messages m JOIN events e ON e.id = m.source_event_id WHERE m.conversation_id = ?1 AND e.dedupe_key LIKE ?2")?;
    let rows = stmt.query_map(params![conversation.to_string(), DECISION_FEEDBACK], |r| r.get::<_, String>(0))?;
    let mut out = std::collections::HashSet::new();
    for row in rows {
        if let Ok(message) = MessageId::parse(&row?) {
            out.insert(message);
        }
    }
    Ok(out)
}

/// Let a queued run go first: ahead of other ready jobs and without waiting for a retry delay.
pub fn hurry_job(tx: &Transaction<'_>, run: RunId) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE jobs SET priority = (SELECT COALESCE(MAX(priority), 0) + 1 FROM jobs WHERE state = 'ready'), not_before_ms = MIN(not_before_ms, ?2)
         WHERE id = ?1 AND state = 'ready'",
        params![run.to_string(), now_ms()],
    )?;
    crate::inbox::hurry(tx, run)
}

/// Runs of the conversation waiting for their process to be interrupted.
pub fn cancelling_runs_in(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<Vec<RunId>, StorageError> {
    Ok(unfinished_runs_in(conn, conversation)?.into_iter().filter(|(_, s)| s == "cancelling").map(|(id, _)| id).collect())
}

/// Keep files with a message (merged into its metadata).
pub fn set_message_files(tx: &Transaction<'_>, message: MessageId, files: &[aurelius_domain::views::FileRef]) -> Result<(), StorageError> {
    if files.is_empty() {
        return Ok(());
    }
    tx.execute(
        "UPDATE messages SET meta_json = json_set(COALESCE(meta_json, '{}'), '$.files', json(?2)) WHERE id = ?1",
        params![message.to_string(), serde_json::to_string(files)?],
    )?;
    Ok(())
}

/// The files kept with a message (`set_message_files`).
pub fn message_files(conn: &Connection, message: MessageId) -> Result<Vec<aurelius_domain::views::FileRef>, StorageError> {
    let files: Option<String> = conn.query_row("SELECT json_extract(meta_json, '$.files') FROM messages WHERE id = ?1", [message.to_string()], |r| r.get(0)).optional()?.flatten();
    Ok(files.and_then(|f| serde_json::from_str(&f).ok()).unwrap_or_default())
}

/// Keep the files the user uploaded into the work folder with their message.
pub fn set_message_uploads(tx: &Transaction<'_>, message: MessageId, uploads: &[aurelius_domain::views::MessageFileView]) -> Result<(), StorageError> {
    if uploads.is_empty() {
        return Ok(());
    }
    tx.execute(
        "UPDATE messages SET meta_json = json_set(COALESCE(meta_json, '{}'), '$.uploads', json(?2)) WHERE id = ?1",
        params![message.to_string(), serde_json::to_string(uploads)?],
    )?;
    Ok(())
}

/// The files uploaded with a message, and its conversation; none for an unknown
/// or a deleted message (its rows and blobs stay, but nothing serves them).
pub fn message_uploads(conn: &Connection, message: MessageId) -> Result<Option<(ConversationId, Vec<aurelius_domain::views::MessageFileView>)>, StorageError> {
    let live = if has_edit_columns(conn)? { "AND deleted_at_ms IS NULL" } else { "" };
    let row: Option<(String, Option<String>)> = conn
        .query_row(&format!("SELECT conversation_id, json_extract(meta_json, '$.uploads') FROM messages WHERE id = ?1 {live}"), [message.to_string()], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    let Some((conversation, uploads)) = row else { return Ok(None) };
    let conversation = ConversationId::parse(&conversation).map_err(|e| StorageError::Rejected(e.to_string()))?;
    Ok(Some((conversation, uploads.and_then(|u| serde_json::from_str(&u).ok()).unwrap_or_default())))
}

/// A file kept with a message: (name, media type, storage key). Only blobs a
/// message refers to as a file can be read this way, and not those of a deleted message.
pub fn message_file(conn: &Connection, id: &str) -> Result<Option<(String, String, String)>, StorageError> {
    let live = if has_edit_columns(conn)? { "AND m.deleted_at_ms IS NULL" } else { "" };
    let found: Option<String> = conn
        .query_row(
            &format!(
                "SELECT json_extract(f.value, '$.name') FROM messages m, json_each(m.meta_json, '$.files') f
                 WHERE m.meta_json IS NOT NULL {live} AND json_extract(f.value, '$.id') = ?1 LIMIT 1"
            ),
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(name) = found else { return Ok(None) };
    let blob: Option<(String, String)> = conn
        .query_row("SELECT media_type, storage_key FROM blobs WHERE id = ?1 AND deleted_at_ms IS NULL", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    Ok(blob.map(|(media, key)| (name, media, key)))
}

/// The storage key of the context overview kept with a run, if it has one.
pub fn run_context_key(conn: &Connection, run: RunId) -> Option<String> {
    conn.query_row("SELECT b.storage_key FROM runs r JOIN blobs b ON b.id = r.context_blob_id WHERE r.id = ?1", [run.to_string()], |r| r.get(0)).ok()
}

/// Those of `ids` that are runs of ours that will not run again: finished, asked
/// to stop (a run waiting for a usage limit that is stopped never resumes), or of
/// a task that is over. Ids that are not runs here are left out.
pub fn runs_done_among(conn: &Connection, ids: &[String]) -> Result<Vec<String>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT r.id FROM runs r LEFT JOIN tasks t ON t.id = r.task_id
         WHERE r.id = ?1 AND (r.status IN ('succeeded','failed','cancelled','unknown','cancelling') OR t.status IN ('succeeded','failed','cancelled'))",
    )?;
    let mut done = Vec::new();
    for id in ids {
        if let Some(id) = stmt.query_row([id], |r| r.get::<_, String>(0)).optional()? {
            done.push(id);
        }
    }
    Ok(done)
}

/// Runs that were asked to stop and have not stopped yet.
pub fn cancelling_runs(conn: &Connection) -> Result<Vec<RunId>, StorageError> {
    let mut stmt = conn.prepare("SELECT id FROM runs WHERE status = 'cancelling'")?;
    let ids: Vec<String> = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<_, _>>()?;
    Ok(ids.iter().filter_map(|id| RunId::parse(id).ok()).collect())
}
