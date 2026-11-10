//! Approvals: questions and permission requests of a run.
//! The action blob holds the exact action shown to the user, so its digest
//! can be recomputed at decision time.

use crate::{StorageError, now_ms};
use aurelius_domain::{ApprovalId, ConversationId, EventId, RunId, approval::ApprovalStatus, views::ApprovalView};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

fn parse<T, E: std::error::Error + Send + Sync + 'static>(r: Result<T, E>) -> rusqlite::Result<T> {
    r.map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

pub struct NewApproval<'a> {
    pub id: ApprovalId,
    pub run_id: RunId,
    pub call_id: &'a str,
    pub tool: &'a str,
    pub effect: &'a str,
    pub action_blob_id: &'a str,
    pub digest: &'a str,
    pub request_event: EventId,
    pub expires_at_ms: i64,
}

pub fn insert_approval(tx: &Transaction<'_>, a: NewApproval<'_>) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO approvals(id, run_id, call_id, tool, effect, action_blob_id, action_digest, request_event_id, status, expires_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9)",
        params![
            a.id.to_string(),
            a.run_id.to_string(),
            a.call_id,
            a.tool,
            a.effect,
            a.action_blob_id,
            a.digest,
            a.request_event.to_string(),
            a.expires_at_ms
        ],
    )?;
    Ok(())
}

pub struct ApprovalRow {
    pub status: ApprovalStatus,
    pub digest: String,
    pub expires_at_ms: i64,
    pub action_blob_key: String,
    pub run_id: RunId,
    pub call_id: String,
    pub tool: String,
}

pub fn load_approval(tx: &Transaction<'_>, id: ApprovalId) -> Result<Option<ApprovalRow>, StorageError> {
    Ok(tx
        .query_row(
            "SELECT a.status, a.action_digest, a.expires_at_ms, b.storage_key, a.run_id, a.call_id, a.tool
             FROM approvals a JOIN blobs b ON b.id = a.action_blob_id
             WHERE a.id = ?1",
            [id.to_string()],
            |r| {
                Ok(ApprovalRow {
                    status: ApprovalStatus::parse(&r.get::<_, String>(0)?).unwrap_or(ApprovalStatus::Invalidated),
                    digest: r.get(1)?,
                    expires_at_ms: r.get(2)?,
                    action_blob_key: r.get(3)?,
                    run_id: parse(RunId::parse(&r.get::<_, String>(4)?))?,
                    call_id: r.get(5)?,
                    tool: r.get(6)?,
                })
            },
        )
        .optional()?)
}

pub fn set_approval_status(tx: &Transaction<'_>, id: ApprovalId, status: ApprovalStatus, decision_event: Option<EventId>) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE approvals SET status = ?2, decision_event_id = COALESCE(?3, decision_event_id),
             decided_at_ms = COALESCE(decided_at_ms, ?4) WHERE id = ?1",
        params![id.to_string(), status.as_str(), decision_event.map(|e| e.to_string()), now_ms()],
    )?;
    Ok(())
}

pub fn set_approval_answer(tx: &Transaction<'_>, id: ApprovalId, answers: &serde_json::Value) -> Result<(), StorageError> {
    tx.execute("UPDATE approvals SET answer_json = ?2 WHERE id = ?1", params![id.to_string(), answers.to_string()])?;
    Ok(())
}

pub fn approval_answer(conn: &Connection, id: ApprovalId) -> Result<Option<serde_json::Value>, StorageError> {
    let json: Option<String> = conn.query_row("SELECT answer_json FROM approvals WHERE id = ?1", [id.to_string()], |r| r.get(0)).optional()?.flatten();
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

/// Pending approvals (with their run) for one tool, as stored (`claude.<tool>`), still in time.
pub fn pending_for_tool(tx: &Transaction<'_>, tool: &str, now: i64) -> Result<Vec<(ApprovalId, RunId)>, StorageError> {
    let mut stmt = tx.prepare("SELECT id, run_id FROM approvals WHERE tool = ?1 AND status = 'pending' AND expires_at_ms > ?2")?;
    let rows = stmt.query_map(params![tool, now], |r| Ok((parse(ApprovalId::parse(&r.get::<_, String>(0)?))?, parse(RunId::parse(&r.get::<_, String>(1)?))?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Pending approvals past their expiry.
pub fn expired_pending(tx: &Transaction<'_>, now: i64) -> Result<Vec<ApprovalId>, StorageError> {
    let mut stmt = tx.prepare("SELECT id FROM approvals WHERE status = 'pending' AND expires_at_ms <= ?1")?;
    let rows = stmt.query_map([now], |r| parse(ApprovalId::parse(&r.get::<_, String>(0)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Pending approvals of a run (used when a run ends without them).
pub fn pending_for_run(tx: &Transaction<'_>, run: RunId) -> Result<Vec<ApprovalId>, StorageError> {
    let mut stmt = tx.prepare("SELECT id FROM approvals WHERE run_id = ?1 AND status = 'pending'")?;
    let rows = stmt.query_map([run.to_string()], |r| parse(ApprovalId::parse(&r.get::<_, String>(0)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Every run with an open approval or question.
pub fn pending_runs(conn: &Connection) -> Result<std::collections::HashSet<RunId>, StorageError> {
    let mut stmt = conn.prepare("SELECT DISTINCT run_id FROM approvals WHERE status = 'pending'")?;
    let rows = stmt.query_map([], |r| parse(RunId::parse(&r.get::<_, String>(0)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Pending approvals first, then the 20 most recent decisions.
/// `project`: only those of a run in that project, through its task or its conversation
/// (a task's, a project's or a ticket's discussion). An employee's own chat has none.
/// What a run in another board member's private chat asked stays out for `reader`.
pub fn list_approvals(conn: &Connection, project: Option<aurelius_domain::ProjectId>, reader: Option<aurelius_domain::UserId>, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<ApprovalView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.status, a.action_digest, e.recorded_at_ms, a.expires_at_ms, a.run_id, r.conversation_id,
                COALESCE(m.name, rm.name, 'Chat'), b.storage_key, r.task_id,
                (SELECT t.ticket_id FROM tasks t WHERE t.id = r.task_id OR t.conversation_id = r.conversation_id LIMIT 1),
                json_extract(j.payload_json, '$.group.conversation_id')
         FROM approvals a
         JOIN blobs b ON b.id = a.action_blob_id
         JOIN events e ON e.id = a.request_event_id
         JOIN runs r ON r.id = a.run_id
         LEFT JOIN conversations c ON c.id = r.conversation_id
         LEFT JOIN employees m ON m.id = c.employee_id
         LEFT JOIN employees rm ON rm.id = r.employee_id
         LEFT JOIN jobs j ON j.id = a.run_id
         WHERE (a.status = 'pending' OR a.id IN (SELECT id FROM approvals WHERE status <> 'pending' ORDER BY decided_at_ms DESC LIMIT 20))
           AND (?1 IS NULL OR COALESCE(
                 (SELECT t.project_id FROM tasks t WHERE t.id = r.task_id),
                 c.project_id,
                 (SELECT g.project_id FROM conversations g WHERE g.id = json_extract(j.payload_json, '$.group.conversation_id'))
               ) = ?1)
           AND (c.board_member_id IS NULL OR c.board_member_id = ?2)
         ORDER BY a.status = 'pending' DESC, e.recorded_at_ms DESC",
    )?;
    let rows = stmt.query_map([project.map(|p| p.to_string()), reader.map(|u| u.to_string())], |r| {
        Ok((
            parse(ApprovalId::parse(&r.get::<_, String>(0)?))?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            parse(RunId::parse(&r.get::<_, String>(5)?))?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, String>(8)?,
            r.get::<_, Option<String>>(9)?,
            r.get::<_, Option<String>>(10)?,
            r.get::<_, Option<String>>(11)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, status, digest, requested, expires, run_id, conversation, requester, key, task, ticket, discussion) = row?;
        let discussion = discussion.and_then(|c| ConversationId::parse(&c).ok());
        // A run woken by a ticket's discussion has no task of the ticket.
        let ticket = match (ticket.and_then(|t| aurelius_domain::TicketId::parse(&t).ok()), discussion) {
            (Some(ticket), _) => Some(ticket),
            (None, Some(d)) => crate::discussions::ticket_of_conversation(conn, d)?,
            (None, None) => None,
        };
        let action: serde_json::Value = read_blob(&key).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let tool = action["tool"].as_str().unwrap_or("");
        out.push(ApprovalView {
            always_allow_eligible: aurelius_domain::approval::always_allow_eligible(tool),
            id,
            status: ApprovalStatus::parse(&status).unwrap_or(ApprovalStatus::Invalidated),
            tool: tool.into(),
            effect: action["effect"].as_str().unwrap_or("").into(),
            arguments: action["arguments"].clone(),
            resources: serde_json::from_value(action["resources"].clone()).unwrap_or_default(),
            account: action["account"].as_str().unwrap_or("").into(),
            reasons: serde_json::from_value(action["reasons"].clone()).unwrap_or_default(),
            action_digest: digest,
            requested_at_ms: requested,
            expires_at_ms: expires,
            run_id,
            conversation_id: conversation.and_then(|c| ConversationId::parse(&c).ok()),
            requester,
            task_id: task.and_then(|t| aurelius_domain::TaskId::parse(&t).ok()),
            ticket_id: ticket,
            discussion_id: discussion,
        });
    }
    Ok(out)
}
