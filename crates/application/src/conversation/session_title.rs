//! A work session's title made from its content. A session starts named after the
//! first line of its first message; after its first answer the job
//! `session.title` asks a model for a short title from the opening of the
//! conversation (`aurelius_daemon::memory::run_session_title`) and
//! `record_session_title` puts it on the conversation. One try per session
//! (`work_sessions.title_asked_at_ms`, `migrations/0047`): when it fails, the
//! first-line title stays.

use crate::run::events::{Ev, emit};
use crate::{CommandService, storage_error};
use aurelius_domain::work_session::{SESSION_TITLE_INPUT_MAX_CHARS, SESSION_TITLE_MESSAGE_MAX_CHARS, session_title};
use aurelius_domain::{ActorKind, AppError, ConversationId, EventId, EventNotice, RunId, UserId};
use aurelius_storage::{StorageError, Tx, events, queries, work_sessions};
use rusqlite::OptionalExtension;

/// The job that makes a session's title.
pub const SESSION_TITLE_JOB_KIND: &str = "session.title";

/// How many of a session's first messages the title reads.
const OPENING_MESSAGES: i64 = 8;

/// Everything one session title needs.
#[derive(Debug, Clone)]
pub struct SessionTitleInput {
    pub conversation_id: ConversationId,
    pub project_name: String,
    pub employee_name: String,
    /// (who, text) of the session's first messages, oldest first.
    pub turns: Vec<(String, String)>,
    pub config: crate::run::connections::ResolvedRunConfig,
}

/// After an answer in a session (in the run's transaction): the first time, its
/// title job is queued to run at once. Nothing for a conversation that is no
/// session, or for a session that asked before.
pub(crate) fn schedule_session_title(tx: &Tx<'_>, conversation: ConversationId, source: EventId, run: RunId) -> Result<(), StorageError> {
    if work_sessions::of_conversation(tx, conversation)?.is_none() || !work_sessions::mark_title_asked(tx, conversation)? {
        return Ok(());
    }
    let job = format!("session-title-{}", aurelius_domain::JobId::new());
    let payload = serde_json::json!({ "conversation_id": conversation, "run_id": run });
    events::insert_job(tx, &job, SESSION_TITLE_JOB_KIND, source, &job, &payload)
}

/// The opening of a session the title reads: oldest first until `max`
/// characters, each message cut at `SESSION_TITLE_MESSAGE_MAX_CHARS`.
pub fn opening_turns(turns: Vec<(String, String)>, max: usize) -> Vec<(String, String)> {
    let mut kept = Vec::new();
    let mut used = 0;
    for (who, text) in turns {
        let text: String = text.trim().chars().take(SESSION_TITLE_MESSAGE_MAX_CHARS).collect();
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
    kept
}

impl CommandService {
    /// The input for a session's title; `None` when it is no session any more
    /// (deleted) or has no text to name it after. It runs on the login of the run
    /// that answered, else on the login of the board member who started it.
    pub fn session_title_input(&self, conversation: ConversationId, run: Option<RunId>) -> Result<Option<SessionTitleInput>, AppError> {
        type Head = (String, String, String, String);
        let head: Option<Head> = self
            .store
            .read(|c| {
                Ok(c.query_row(
                    "SELECT p.name, e.id, e.name, s.started_by FROM work_sessions s JOIN conversations c ON c.id = s.conversation_id
                     JOIN projects p ON p.id = c.project_id JOIN employees e ON e.id = c.employee_id
                     WHERE s.conversation_id = ?1 AND c.kind = 'session' AND c.archived_at_ms IS NULL",
                    [conversation.to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?)
            })
            .map_err(storage_error)?;
        let Some((project_name, employee, employee_name, started_by)) = head else { return Ok(None) };
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
                let mut stmt = conn.prepare(
                    "SELECT m.role, b.storage_key FROM messages m JOIN blobs b ON b.id = m.content_blob_id
                     WHERE m.conversation_id = ?1 AND m.status = 'complete' AND m.role IN ('user','assistant') AND m.deleted_at_ms IS NULL
                     ORDER BY m.ordinal LIMIT ?2",
                )?;
                let rows = stmt.query_map(rusqlite::params![conversation.to_string(), OPENING_MESSAGES], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
                let turns = rows
                    .into_iter()
                    .filter_map(|(role, key)| {
                        let who = if role == "user" { "Board member".to_owned() } else { employee_name.clone() };
                        Some((who, read(&key)?))
                    })
                    .collect();
                let turns = opening_turns(turns, SESSION_TITLE_INPUT_MAX_CHARS);
                if turns.is_empty() {
                    return Ok(None);
                }
                Ok(Some(SessionTitleInput { conversation_id: conversation, project_name, employee_name: employee_name.clone(), turns, config }))
            })
            .map_err(storage_error)
    }

    /// Put a session's made title on it and close its job, fenced by the job's
    /// lease. The answer goes through the vault redactor and `session_title`;
    /// an unusable answer leaves the first-line title. Open lists hear it as the
    /// conversation's settings event with `changed: ["title"]`.
    pub fn record_session_title(&self, job_id: &str, epoch: i64, conversation: ConversationId, answer: &str) -> Result<Vec<EventNotice>, AppError> {
        let title = session_title(&aurelius_runtime::redact::string(answer.to_owned()));
        let job = job_id.to_owned();
        self.store
            .write(move |tx| {
                if !queries::holds_lease(tx, &job, epoch)? {
                    return Ok(Vec::new());
                }
                let changed = match &title {
                    Some(title) => work_sessions::set_title(tx, conversation, title)?,
                    None => false,
                };
                queries::complete_job(tx, &job, epoch)?;
                if !changed {
                    return Ok(Vec::new());
                }
                let (_, notice) = emit(tx, Ev {
                    aggregate_type: "conversation",
                    aggregate_id: conversation.to_string(),
                    event_type: "conversation.settings_changed",
                    actor: ActorKind::System,
                    actor_id: None,
                    correlation: job.clone(),
                    payload: None,
                    metadata: serde_json::json!({ "changed": ["title"] }),
                    dedupe: format!("session-title:{conversation}:{job}"),
                })?;
                Ok(vec![notice])
            })
            .map_err(storage_error)
    }

    /// The title could not be made (quota, a failed run, a deleted session): the
    /// job ends and the first-line title stays. `why` only goes to the server's log.
    pub fn session_title_failed(&self, job_id: &str, epoch: i64) -> Result<(), AppError> {
        let job = job_id.to_owned();
        self.store.write(move |tx| queries::kill_job(tx, &job, epoch, "title_failed").map(|_| ())).map_err(storage_error)
    }
}

#[cfg(test)]
#[path = "session_title_tests.rs"]
mod tests;
