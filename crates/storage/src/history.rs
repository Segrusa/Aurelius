//! The History of finished tickets (`migrations/0042`, ticket #57): the summary of
//! a ticket's latest Done on the ticket itself, and the read `ticket.history`.
//!
//! The state and the text are written in the transaction that moves the ticket
//! (`mark_pending` at Done) or that records its extraction (`set_summary`,
//! `set_failed`); the read derives "summarizing" from the extraction job.

use crate::{BlobStore, StorageError};
use aurelius_domain::history::{self, HistoryEntry, HistorySummaryState, stored};
use aurelius_domain::{EmployeeId, ProjectId, TicketId};
use rusqlite::types::ToSql;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// The ticket went to Done: its summary is to come (every Done starts again, B2).
pub fn mark_pending(tx: &Transaction<'_>, ticket: TicketId) -> Result<(), StorageError> {
    tx.execute("UPDATE tickets SET summary_state = ?2, summary_text = NULL WHERE id = ?1", params![ticket.to_string(), stored::PENDING])?;
    Ok(())
}

/// The summary of the ticket's Done, already through the vault redactor and
/// `history::history_summary`. Only while the ticket is on Done and has none:
/// a job that runs again does not overwrite it. Whether it was written.
pub fn set_summary(tx: &Transaction<'_>, ticket: TicketId, text: &str) -> Result<bool, StorageError> {
    let changed = tx.execute(
        "UPDATE tickets SET summary_state = ?2, summary_text = ?3
         WHERE id = ?1 AND done_at_ms IS NOT NULL AND summary_text IS NULL
           AND column_id IN (SELECT id FROM board_columns WHERE kind = 'done')",
        params![ticket.to_string(), stored::READY, text],
    )?;
    Ok(changed > 0)
}

/// The extraction of the ticket's Done failed or gave no summary. Whether it changed.
pub fn set_failed(tx: &Transaction<'_>, ticket: TicketId) -> Result<bool, StorageError> {
    let changed = tx.execute(
        "UPDATE tickets SET summary_state = ?2
         WHERE id = ?1 AND summary_text IS NULL AND (summary_state IS NULL OR summary_state = ?3)
           AND column_id IN (SELECT id FROM board_columns WHERE kind = 'done')",
        params![ticket.to_string(), stored::FAILED, stored::PENDING],
    )?;
    Ok(changed > 0)
}

/// Whether the ticket has its summary on it (`summary_state` 'ready').
pub fn has_summary(conn: &Connection, ticket: TicketId) -> Result<bool, StorageError> {
    Ok(conn
        .query_row("SELECT summary_text IS NOT NULL FROM tickets WHERE id = ?1", [ticket.to_string()], |r| r.get::<_, bool>(0))
        .optional()?
        .unwrap_or(false))
}

/// `tickets.summary_state` of the ticket (none: not on Done since the History).
pub fn stored_state(conn: &Connection, ticket: TicketId) -> Result<Option<String>, StorageError> {
    Ok(conn.query_row("SELECT summary_state FROM tickets WHERE id = ?1", [ticket.to_string()], |r| r.get(0)).optional()?.flatten())
}

/// What `ticket.history` narrows on; every given one applies.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub project: Option<ProjectId>,
    /// The ticket's owner.
    pub employee: Option<EmployeeId>,
    /// Done at or after this moment.
    pub from_ms: Option<i64>,
    /// Done before this moment.
    pub to_ms: Option<i64>,
    /// In the title or the summary, case-insensitive.
    pub query: Option<String>,
    /// Keyset: only rows after this one (`done_at_ms`, `number`) in the order of the read.
    pub before: Option<(i64, u32)>,
}

/// Tickets on Done, newest Done first (then the higher number), at most `limit`,
/// and whether there are more. A ticket off Done is not in it (B1, B2); an
/// archived ticket or project stays. `extract_kind`: the extraction job's kind,
/// whose ready or leased job for a ticket makes its row "summarizing". The text
/// comes as stored; the daemon redacts it on the way out.
pub fn list(conn: &Connection, filter: &Filter, limit: u32, extract_kind: &str) -> Result<(Vec<HistoryEntry>, bool), StorageError> {
    let mut clauses = vec!["c.kind = 'done'".to_owned(), "k.done_at_ms IS NOT NULL".to_owned()];
    let mut values: Vec<Box<dyn ToSql>> = vec![Box::new(extract_kind.to_owned())];
    let mut add = |clause: &dyn Fn(usize) -> String, value: Box<dyn ToSql>| {
        values.push(value);
        clauses.push(clause(values.len()));
    };
    if let Some(project) = filter.project {
        add(&|n| format!("k.project_id = ?{n}"), Box::new(project.to_string()));
    }
    if let Some(employee) = filter.employee {
        add(&|n| format!("k.owner_employee_id = ?{n}"), Box::new(employee.to_string()));
    }
    if let Some(from) = filter.from_ms {
        add(&|n| format!("k.done_at_ms >= ?{n}"), Box::new(from));
    }
    if let Some(to) = filter.to_ms {
        add(&|n| format!("k.done_at_ms < ?{n}"), Box::new(to));
    }
    let needle = filter.query.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(|q| q.chars().take(history::HISTORY_QUERY_MAX_CHARS).collect::<String>().to_lowercase());
    if let Some(needle) = needle {
        // instr, not LIKE: a % or _ in what was typed is just a character.
        add(&|n| format!("(instr(lower(k.title), ?{n}) > 0 OR instr(lower(COALESCE(k.summary_text, '')), ?{n}) > 0)"), Box::new(needle));
    }
    if let Some((done, number)) = filter.before {
        values.push(Box::new(done));
        let d = values.len();
        values.push(Box::new(number));
        let m = values.len();
        clauses.push(format!("(k.done_at_ms < ?{d} OR (k.done_at_ms = ?{d} AND k.number < ?{m}))"));
    }
    let sql = format!(
        "SELECT k.id, k.number, k.title, k.project_id, p.name, k.owner_employee_id, e.name, k.done_at_ms, k.summary_text, k.summary_state,
                EXISTS(SELECT 1 FROM jobs j WHERE j.kind = ?1 AND j.state IN ('ready','leased') AND json_extract(j.payload_json, '$.ticket_id') = k.id)
         FROM tickets k JOIN board_columns c ON c.id = k.column_id JOIN projects p ON p.id = k.project_id
         LEFT JOIN employees e ON e.id = k.owner_employee_id
         WHERE {} ORDER BY k.done_at_ms DESC, k.number DESC LIMIT {}",
        clauses.join(" AND "),
        limit + 1
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values.iter().map(|v| v.as_ref())), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, u32>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, i64>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, Option<String>>(9)?,
            r.get::<_, bool>(10)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, number, title, project, project_name, owner, owner_name, done_at_ms, summary, state, pending) = row?;
        let (Ok(ticket_id), Ok(project_id)) = (TicketId::parse(&id), ProjectId::parse(&project)) else { continue };
        out.push(HistoryEntry {
            ticket_id,
            number,
            title,
            project_id,
            project_name,
            owner_employee_id: owner.and_then(|o| EmployeeId::parse(&o).ok()),
            owner_name,
            done_at_ms,
            summary_state: HistorySummaryState::of(summary.is_some(), pending, state.as_deref()),
            summary,
        });
    }
    let more = out.len() > limit as usize;
    out.truncate(limit as usize);
    Ok((out, more))
}

/// Migration 42's step (A3): a ticket already on Done gets the summary its project's
/// memory has of it (`locator.ticket_id`, written only by the ticket's extraction),
/// without a model call. Without a blob store (a bare database) or without such a
/// memory, the ticket stays as it is and shows "finished before History".
pub fn backfill(tx: &Transaction<'_>, blobs: Option<&BlobStore>) -> Result<(), StorageError> {
    let Some(blobs) = blobs else { return Ok(()) };
    let found: Vec<(String, u32, String)> = {
        let mut stmt = tx.prepare(
            "SELECT k.id, k.number,
                    (SELECT b.storage_key FROM memory_items m JOIN blobs b ON b.id = m.content_blob_id
                     WHERE json_extract(m.locator_json, '$.ticket_id') = k.id AND m.status = 'active' AND m.kind = 'project_knowledge'
                     ORDER BY m.created_at_ms DESC LIMIT 1)
             FROM tickets k JOIN board_columns c ON c.id = k.column_id
             WHERE c.kind = 'done' AND k.done_at_ms IS NOT NULL",
        )?;
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?, r.get::<_, Option<String>>(2)?)))?
            .filter_map(|row| match row {
                Ok((id, number, Some(key))) => Some(Ok((id, number, key))),
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<_, _>>()?
    };
    for (id, number, key) in found {
        // A blob that cannot be read leaves the ticket "finished before History".
        let Some(text) = blobs.read(&key).ok().and_then(|b| String::from_utf8(b).ok()) else { continue };
        if let Some(summary) = history::summary_from_memory(number, &text) {
            tx.execute("UPDATE tickets SET summary_state = ?2, summary_text = ?3 WHERE id = ?1", params![id, stored::READY, summary])?;
        }
    }
    Ok(())
}
