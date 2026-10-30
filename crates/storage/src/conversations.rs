//! Conversation, message and run projections.

use crate::{StorageError, now_ms};
use aurelius_domain::{
    ConversationId, EmployeeId, EventId, MessageId, RunId,
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

/// A project conversation (no employee). The group chat is not one: it takes
/// messages only through `group_chat.send`.
pub fn conversation_exists(tx: &Transaction<'_>, id: ConversationId) -> Result<bool, StorageError> {
    Ok(tx.query_row("SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1 AND employee_id IS NULL AND kind <> 'group')", [id.to_string()], |r| r.get(0))?)
}

/// The company's group chat (migration 17 makes it).
pub fn group_conversation(conn: &Connection) -> Result<Option<ConversationId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT id FROM conversations WHERE kind = 'group' ORDER BY created_at_ms, id LIMIT 1", [], |r| r.get(0)).optional()?;
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

/// How many of those messages the group chat has.
pub fn group_message_count(conn: &Connection, group: ConversationId) -> Result<u32, StorageError> {
    Ok(conn.query_row(&format!("SELECT COUNT(*) {GROUP_MESSAGES}"), [group.to_string()], |r| r.get(0))?)
}

/// How many posts in the group chat woke colleagues (`woke` in their meta): of
/// run `run`, and of employee `employee` since `since_ms`.
pub fn waking_posts(conn: &Connection, group: ConversationId, run: RunId, employee: EmployeeId, since_ms: i64) -> Result<(u32, u32), StorageError> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(json_extract(meta_json, '$.run_id') = ?2), 0),
                COALESCE(SUM(json_extract(meta_json, '$.employee_id') = ?3 AND created_at_ms >= ?4), 0)
         FROM messages WHERE conversation_id = ?1 AND role = 'tool' AND json_extract(meta_json, '$.woke') > 0",
        params![group.to_string(), run.to_string(), employee.to_string(), since_ms],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
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
}

/// The conversation a run answers in.
pub fn run_conversation(conn: &Connection, run: RunId) -> Result<Option<ConversationId>, StorageError> {
    let id: Option<Option<String>> = conn.query_row("SELECT conversation_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).optional()?;
    Ok(id.flatten().and_then(|c| ConversationId::parse(&c).ok()))
}

/// The group chat marking of a run, if the group chat woke it.
pub fn group_run(conn: &Connection, run: RunId) -> Result<Option<GroupRun>, StorageError> {
    let marking: Option<Option<String>> = conn
        .query_row("SELECT json_extract(payload_json, '$.group') FROM jobs WHERE id = ?1", [run.to_string()], |r| r.get(0))
        .optional()?;
    Ok(marking.flatten().and_then(|m| serde_json::from_str(&m).ok()))
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
    let ordinal: i64 = tx.query_row(
        "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM messages WHERE conversation_id = ?1",
        [m.conversation_id.to_string()],
        |r| r.get(0),
    )?;
    let role = match m.role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    };
    tx.execute(
        "INSERT INTO messages(id, conversation_id, ordinal, role, content_blob_id, source_event_id, status,
             input_kind, created_at_ms, run_id, reply_to_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'text', ?8, ?9, ?10)",
        params![
            m.id.to_string(),
            m.conversation_id.to_string(),
            ordinal,
            role,
            m.content_blob_id,
            m.source_event_id.to_string(),
            m.status.as_str(),
            now_ms(),
            m.run_id.map(|r| r.to_string()),
            m.reply_to.map(|r| r.to_string()),
        ],
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
    let ordinal: i64 = tx.query_row("SELECT COALESCE(MAX(ordinal), 0) + 1 FROM messages WHERE conversation_id = ?1", [conversation.to_string()], |r| r.get(0))?;
    tx.execute(
        "INSERT INTO messages(id, conversation_id, ordinal, role, content_blob_id, source_event_id, status, input_kind, created_at_ms, meta_json)
         VALUES (?1, ?2, ?3, 'tool', ?4, ?5, 'complete', 'tool', ?6, ?7)",
        params![id.to_string(), conversation.to_string(), ordinal, content_blob_id, source_event.to_string(), now_ms(), meta.to_string()],
    )?;
    Ok(ordinal as u32)
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
        "INSERT INTO runs(id, conversation_id, attempt, runtime_kind, connection_id, model_id, resolved_config_json, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'created')",
        params![
            run_id.to_string(),
            conversation_id.to_string(),
            attempt,
            resolved.runtime_kind(),
            resolved.connection(),
            resolved.model().unwrap_or_default(),
            serde_json::to_string(resolved)?
        ],
    )?;
    Ok(attempt as u32)
}

pub fn run_status(tx: &rusqlite::Connection, run_id: RunId) -> Result<Option<String>, StorageError> {
    Ok(tx.query_row("SELECT status FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).optional()?)
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

/// An employee's chats (not its task conversations), most recently active first.
pub fn employee_conversations(conn: &Connection, employee: aurelius_domain::EmployeeId) -> Result<Vec<ConversationSummary>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.title, c.created_at_ms,
                COALESCE((SELECT MAX(m.created_at_ms) FROM messages m WHERE m.conversation_id = c.id), c.created_at_ms),
                (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id)
         FROM conversations c WHERE c.kind = 'employee' AND c.employee_id = ?1 AND c.archived_at_ms IS NULL
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
        "SELECT EXISTS(SELECT 1 FROM conversations c WHERE c.id = ?1 AND c.kind = 'employee'
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

/// All messages of a conversation in order. `read_blob` resolves content.
pub fn list_messages(
    conn: &Connection,
    conversation: ConversationId,
    read_blob: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<MessageView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.ordinal, m.role, m.status, b.storage_key, m.created_at_ms, m.run_id, m.reply_to_id,
                c.display_name, r.model_id, json_extract(r.resolved_config_json, '$.persona_name'),
                json_extract(m.meta_json, '$.kind'), json_extract(m.meta_json, '$.from'), json_extract(m.meta_json, '$.approval_id'), json_extract(m.meta_json, '$.files'), json_extract(m.meta_json, '$.uploads')
         FROM messages m LEFT JOIN blobs b ON b.id = m.content_blob_id
         LEFT JOIN runs r ON r.id = m.run_id LEFT JOIN connections c ON c.id = r.connection_id
         WHERE m.conversation_id = ?1 ORDER BY m.ordinal",
    )?;
    let rows = stmt.query_map([conversation.to_string()], |r| {
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
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, ordinal, role, status, key, created, run, reply, connection, model, persona, update, from, approval, files, uploads) = row?;
        let produced_by = match (connection, model.filter(|m| !m.is_empty())) {
            (Some(c), Some(m)) => Some(format!("{c} · {m}")),
            (Some(c), None) => Some(format!("{c} · default model")),
            (None, _) => None,
        };
        let id = MessageId::parse(&id).map_err(|e| StorageError::Rejected(e.to_string()))?;
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
        });
    }
    let waiting = queued_messages(conn, conversation)?;
    for message in out.iter_mut() {
        message.queued = message.role == MessageRole::User && waiting.contains(&message.id);
        if let Some(run) = message.run_id {
            message.sources = crate::files::sources_for_run(conn, run)?;
            if message.role == MessageRole::Assistant {
                message.usage = crate::usage::for_run(conn, run)?;
            }
        }
        message.activity = activity_for_message(conn, message.id)?;
    }
    Ok(out)
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
    let mut stmt =
        conn.prepare("SELECT id, status FROM runs WHERE conversation_id = ?1 AND status IN ('created','starting','running','waiting','cancelling')")?;
    let rows = stmt.query_map([conversation.to_string()], |r| Ok((RunId::parse(&r.get::<_, String>(0)?).map_err(conv_err)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
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
                         AND (r.status IN ('starting','running','waiting','cancelling','succeeded','failed','unknown') OR j.state = 'leased'))",
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
    Ok(rows.collect::<Result<_, _>>()?)
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

/// Let a queued run go first: ahead of other ready jobs and without waiting for a retry delay.
pub fn hurry_job(tx: &Transaction<'_>, run: RunId) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE jobs SET priority = (SELECT COALESCE(MAX(priority), 0) + 1 FROM jobs WHERE state = 'ready'), not_before_ms = MIN(not_before_ms, ?2)
         WHERE id = ?1 AND state = 'ready'",
        params![run.to_string(), now_ms()],
    )?;
    Ok(())
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

/// The files uploaded with a message, and its conversation; none for an unknown message.
pub fn message_uploads(conn: &Connection, message: MessageId) -> Result<Option<(ConversationId, Vec<aurelius_domain::views::MessageFileView>)>, StorageError> {
    let row: Option<(String, Option<String>)> = conn
        .query_row("SELECT conversation_id, json_extract(meta_json, '$.uploads') FROM messages WHERE id = ?1", [message.to_string()], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    let Some((conversation, uploads)) = row else { return Ok(None) };
    let conversation = ConversationId::parse(&conversation).map_err(|e| StorageError::Rejected(e.to_string()))?;
    Ok(Some((conversation, uploads.and_then(|u| serde_json::from_str(&u).ok()).unwrap_or_default())))
}

/// A file kept with a message: (name, media type, storage key). Only blobs a
/// message refers to as a file can be read this way.
pub fn message_file(conn: &Connection, id: &str) -> Result<Option<(String, String, String)>, StorageError> {
    let found: Option<String> = conn
        .query_row(
            "SELECT json_extract(f.value, '$.name') FROM messages m, json_each(m.meta_json, '$.files') f
             WHERE m.meta_json IS NOT NULL AND json_extract(f.value, '$.id') = ?1 LIMIT 1",
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
