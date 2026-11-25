//! Work sessions in the History (feature `session_history`): a short AI summary
//! of what was done in a session, kept on its row (`migrations/0046`).
//!
//! A session has no Done. After every answer its summary job is (re)set to run
//! once the session has been quiet for `SESSION_SUMMARY_QUIET_MS`, so a stretch
//! of work costs one model call. The daemon runs the job
//! (`aurelius_daemon::memory::run_session_summary`) with what `session_summary_input`
//! gives and stores the answer with `record_session_summary`. A board member asks
//! for one at once with `work_session.retry_summary`.

use crate::run::events::{Ev, emit};
use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::history::{SESSION_SUMMARY_INPUT_MAX_CHARS, SESSION_SUMMARY_MESSAGE_MAX_CHARS, SESSION_SUMMARY_QUIET_MS, history_summary};
use aurelius_domain::work_session::RetrySessionSummary;
use aurelius_domain::{ActorKind, AppError, CommandEnvelope, CommandResult, ConversationId, DomainError, EventId, EventNotice, RunId, UserId};
use aurelius_storage::{StorageError, Tx, events, queries, work_sessions};
use rusqlite::OptionalExtension;

/// The job that makes a session's summary.
pub const SESSION_SUMMARY_JOB_KIND: &str = "session.summarize";

/// Refused: the summary is being made already.
pub const SUMMARY_UNDER_WAY: &str = "The summary is being made; wait for it.";

/// Everything one session summary needs.
#[derive(Debug, Clone)]
pub struct SessionSummaryInput {
    pub conversation_id: ConversationId,
    pub title: String,
    pub project_name: String,
    pub employee_name: String,
    /// The summary so far, which the new one replaces.
    pub previous: Option<String>,
    /// (who, text) of the messages after the previous summary, oldest first.
    pub turns: Vec<(String, String)>,
    /// The newest message it covers.
    pub through_ms: i64,
    pub config: crate::run::connections::ResolvedRunConfig,
}

/// After an answer in a session (in the run's transaction): its summary job runs
/// once the session has been quiet for a while. A job that nobody took yet moves
/// on; else a new one is queued. Nothing for a conversation that is no session.
pub(crate) fn schedule_session_summary(tx: &Tx<'_>, conversation: ConversationId, source: EventId, run: RunId) -> Result<(), StorageError> {
    if work_sessions::of_conversation(tx, conversation)?.is_none() {
        return Ok(());
    }
    queue(tx, conversation, source, Some(run), aurelius_storage::now_ms() + SESSION_SUMMARY_QUIET_MS)
}

/// Queue (or move) the session's summary job to `not_before_ms` and mark it asked for.
fn queue(tx: &Tx<'_>, conversation: ConversationId, source: EventId, run: Option<RunId>, not_before_ms: i64) -> Result<(), StorageError> {
    let id = conversation.to_string();
    if !events::postpone_ready_job(tx, SESSION_SUMMARY_JOB_KIND, "conversation_id", &id, not_before_ms)? {
        let job = format!("session-summary-{}", aurelius_domain::JobId::new());
        let payload = match run {
            Some(run) => serde_json::json!({ "conversation_id": conversation, "run_id": run }),
            None => serde_json::json!({ "conversation_id": conversation }),
        };
        events::insert_job_at(tx, &job, SESSION_SUMMARY_JOB_KIND, source, &job, &payload, not_before_ms)?;
    }
    work_sessions::mark_summary_pending(tx, conversation)
}

/// Whether the session's summary job is queued or running.
fn summary_under_way(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE kind = ?1 AND state = 'leased' AND json_extract(payload_json, '$.conversation_id') = ?2)",
        rusqlite::params![SESSION_SUMMARY_JOB_KIND, conversation.to_string()],
        |r| r.get(0),
    )?)
}

/// What an open History hears when a session's summary changed: the
/// conversation's settings event (no new event kind), with `changed: ["summary"]`.
fn summary_changed(tx: &Tx<'_>, conversation: ConversationId, correlation: String) -> Result<EventNotice, StorageError> {
    let (_, notice) = emit(tx, Ev {
        aggregate_type: "conversation",
        aggregate_id: conversation.to_string(),
        event_type: "conversation.settings_changed",
        actor: ActorKind::System,
        actor_id: None,
        correlation: correlation.clone(),
        payload: None,
        metadata: serde_json::json!({ "changed": ["summary"] }),
        dedupe: format!("session-summary:{conversation}:{correlation}"),
    })?;
    Ok(notice)
}

/// The messages a summary reads: the newest first until `max` characters, each
/// cut at `SESSION_SUMMARY_MESSAGE_MAX_CHARS`, then oldest first again.
pub fn fit_turns(turns: Vec<(String, String)>, max: usize) -> Vec<(String, String)> {
    let mut kept = Vec::new();
    let mut used = 0;
    for (who, text) in turns.into_iter().rev() {
        let text: String = text.trim().chars().take(SESSION_SUMMARY_MESSAGE_MAX_CHARS).collect();
        if text.is_empty() {
            continue;
        }
        let size = text.chars().count();
        if used + size > max && !kept.is_empty() {
            break;
        }
        used += size;
        kept.push((who, text));
    }
    kept.reverse();
    kept
}

impl CommandService {
    /// The input for a session's summary; `None` when it is no session any more
    /// (deleted) or has nothing new since its summary. `run`: the run that
    /// scheduled it; the summary runs on that run's login, else on the login of
    /// the board member who started the session (B1-1).
    pub fn session_summary_input(&self, conversation: ConversationId, run: Option<RunId>) -> Result<Option<SessionSummaryInput>, AppError> {
        type Head = (String, String, String, String, String);
        let head: Option<Head> = self
            .store
            .read(|c| {
                Ok(c.query_row(
                    "SELECT c.title, p.name, e.id, e.name, s.started_by FROM work_sessions s JOIN conversations c ON c.id = s.conversation_id
                     JOIN projects p ON p.id = c.project_id JOIN employees e ON e.id = c.employee_id
                     WHERE s.conversation_id = ?1 AND c.kind = 'session' AND c.archived_at_ms IS NULL",
                    [conversation.to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?)
            })
            .map_err(storage_error)?;
        let Some((title, project_name, employee, employee_name, started_by)) = head else { return Ok(None) };
        let Ok(employee_id) = aurelius_domain::EmployeeId::parse(&employee) else { return Ok(None) };
        let by_run = match run {
            Some(run) => self.store.read(|c| crate::run::login::of_run(c, run)).map_err(storage_error)?,
            None => None,
        };
        let config = self.resolve_for_employee(employee_id, aurelius_domain::connection::Purpose::Memory, by_run.or_else(|| UserId::parse(&started_by).ok()))?;
        let store = self.store.clone();
        self.store
            .read(|conn| {
                let read = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let (previous, through) = work_sessions::summary_so_far(conn, conversation)?;
                let mut stmt = conn.prepare(
                    "SELECT m.role, b.storage_key, m.created_at_ms FROM messages m JOIN blobs b ON b.id = m.content_blob_id
                     WHERE m.conversation_id = ?1 AND m.status = 'complete' AND m.role IN ('user','assistant') AND m.deleted_at_ms IS NULL
                       AND m.created_at_ms > ?2
                     ORDER BY m.ordinal",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![conversation.to_string(), through.unwrap_or(-1)], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                let Some(through_ms) = rows.iter().map(|r| r.2).max() else { return Ok(None) };
                let turns = rows
                    .into_iter()
                    .filter_map(|(role, key, _)| {
                        let who = if role == "user" { "Board member".to_owned() } else { employee_name.clone() };
                        Some((who, read(&key)?))
                    })
                    .collect();
                let turns = fit_turns(turns, SESSION_SUMMARY_INPUT_MAX_CHARS);
                if turns.is_empty() {
                    return Ok(None);
                }
                Ok(Some(SessionSummaryInput { conversation_id: conversation, title, project_name, employee_name: employee_name.clone(), previous, turns, through_ms, config }))
            })
            .map_err(storage_error)
    }

    /// Store a session's summary and close its job, fenced by the job's lease.
    /// The text goes through the vault redactor and is flattened and cut as a
    /// ticket's; an empty answer counts as a failure.
    pub fn record_session_summary(&self, job_id: &str, epoch: i64, input: &SessionSummaryInput, text: &str) -> Result<Vec<EventNotice>, AppError> {
        let summary = history_summary(&aurelius_runtime::redact::string(text.to_owned()));
        let (conversation, through, job) = (input.conversation_id, input.through_ms, job_id.to_owned());
        self.store
            .write(move |tx| {
                if !queries::holds_lease(tx, &job, epoch)? {
                    return Ok(Vec::new());
                }
                let written = match &summary {
                    Some(text) => work_sessions::set_summary(tx, conversation, text, through)?,
                    None => work_sessions::set_summary_failed(tx, conversation)?,
                };
                queries::complete_job(tx, &job, epoch)?;
                Ok(if written { vec![summary_changed(tx, conversation, job.clone())?] } else { Vec::new() })
            })
            .map_err(storage_error)
    }

    /// The session's summary could not be made (quota, a failed run): the job
    /// ends, and while the session has no summary the History says so with Try
    /// again. `why` only goes to the server's log (the daemon logs it).
    pub fn session_summary_failed(&self, job_id: &str, epoch: i64, conversation: ConversationId) -> Result<Vec<EventNotice>, AppError> {
        let job = job_id.to_owned();
        self.store
            .write(move |tx| {
                if !queries::kill_job(tx, &job, epoch, "summary_failed")? {
                    return Ok(Vec::new());
                }
                Ok(if work_sessions::set_summary_failed(tx, conversation)? { vec![summary_changed(tx, conversation, job.clone())?] } else { Vec::new() })
            })
            .map_err(storage_error)
    }

    /// Nothing new to summarize (or the session was deleted): the job ends; a
    /// session that asked for its first summary shows "No summary yet" again.
    pub fn session_summary_skipped(&self, job_id: &str, epoch: i64, conversation: ConversationId) -> Result<(), AppError> {
        let job = job_id.to_owned();
        self.store
            .write(move |tx| {
                if queries::complete_job(tx, &job, epoch)? {
                    tx.execute("UPDATE work_sessions SET summary_state = NULL WHERE conversation_id = ?1 AND summary_text IS NULL", [conversation.to_string()])?;
                }
                Ok(())
            })
            .map_err(storage_error)
    }

    /// `work_session.retry_summary`: a board member asks for a session's summary
    /// now (one that failed, an older session without one, or to bring it up to
    /// date). Any board member may: the History shows every board member's sessions.
    /// Refused while it is being made.
    pub(crate) fn retry_session_summary(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &RetrySessionSummary) -> Result<Committed, AppError> {
        if ctx.user_id.is_none() {
            return Err(domain_error(DomainError::BoardMemberRequired));
        }
        let conversation = cmd.conversation_id;
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            let not_found = || Ok(Err(domain_error(DomainError::ConversationNotFound(conversation))));
            if work_sessions::of_conversation(tx, conversation)?.is_none() {
                return not_found();
            }
            let deleted: bool = tx.query_row("SELECT archived_at_ms IS NOT NULL FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0))?;
            if deleted {
                return not_found();
            }
            if summary_under_way(tx, conversation)? {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "conversation_id".into(), detail: SUMMARY_UNDER_WAY.into() })));
            }
            let (source, notice) = {
                let (id, notice) = emit(tx, Ev {
                    aggregate_type: "conversation",
                    aggregate_id: conversation.to_string(),
                    event_type: "conversation.settings_changed",
                    actor: ActorKind::User,
                    actor_id: None,
                    correlation: command_id.to_string(),
                    payload: None,
                    metadata: serde_json::json!({ "changed": ["summary"] }),
                    dedupe: format!("cmd:{command_id}:0"),
                })?;
                (id, notice)
            };
            queue(tx, conversation, source, None, aurelius_storage::now_ms())?;
            Ok(Ok((CommandResult::SessionSummaryAsked { conversation_id: conversation }, vec![notice])))
        })
    }
}

#[cfg(test)]
#[path = "session_history_tests.rs"]
mod tests;
