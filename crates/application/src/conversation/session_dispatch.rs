//! The dispatcher of a work session (feature `session_dispatch`): a board member
//! starts a session without an employee (`work_session.dispatch`) and the
//! project's dispatcher (else the company's) chooses who takes it, as it chooses
//! the owner of a ticket (`task::triage`): one short run, `SESSION_DISPATCH_JOB_KIND`,
//! with only the bridge's `assign_session` and `recall`, at most `MAX_TURNS` turns.
//!
//! Until it has chosen, the session's employee is the dispatcher, the session is
//! `choosing` and nothing more can be written in it. Its runs hang on the session's
//! conversation (`session_dispatch_runs`) but answer no message; the chosen
//! employee answers the first message. A run that chose nobody gets one more
//! (`ATTEMPTS`, RULE 08); then the session says the dispatcher chose nobody, and the
//! board member chooses (`work_session.assign`). They can also choose while it is
//! still choosing; the dispatcher's later choice is then refused.

use crate::run::queue::queue_run;
use crate::task::task_threads::{Speaker, post_note};
use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::{
    AppError, CommandEnvelope, CommandResult, ConversationId, DomainError, EmployeeId, EventId, EventNotice, MessageId, RunId,
    employee::LifecycleStatus,
    work_session::{AssignWorkSession, SessionDispatch},
};
use aurelius_storage::{StorageError, Store, Tx, company as company_store, employees, queries, work_sessions};
use rusqlite::OptionalExtension;
use serde_json::json;

pub use crate::task::triage::{ATTEMPTS, MAX_TURNS};

/// The job that runs a session's dispatcher.
pub const SESSION_DISPATCH_JOB_KIND: &str = "session.dispatch";
/// The bridge's tools of a session dispatcher's run (full names, allowed without asking).
pub const TOOLS: [&str; 2] = ["mcp__aurelius__assign_session", "mcp__aurelius__recall"];
/// The update kind of the session's own lines: the clients show them as system lines.
pub const LINE_KIND: &str = "system";

/// Refused: a session for the dispatcher without one.
pub const NO_DISPATCHER: &str = "This project has no active dispatcher; choose who takes the session.";
/// Refused: a message in a session that nobody works in yet.
pub const NOBODY_YET: &str = "Nobody works in this session yet: choose who takes it first.";
/// Refused: a choice for a session somebody works in already.
pub const TAKEN: &str = "Somebody works in this session already.";

/// What the dispatcher is told.
const GUIDANCE: &str = "You are the dispatcher: a board member started a work session below without choosing who does it. Choose the one colleague who should take it and call assign_session with their name and a one-sentence reason; they answer the board member's message at once and work with them in the session. Choose by role, focus and load; recall helps with earlier decisions (who did this before). Do not do the work yourself and do not plan it. After assign_session, answer in one short sentence.";
const RETRY: &str = "You ended without choosing. Choose with assign_session now: one colleague from the list, with a reason.";

/// A line in the session from Aurelius (no employee wrote it).
fn line(tx: &Tx<'_>, st: &Store, conversation: ConversationId, text: &str) -> Result<EventNotice, StorageError> {
    let (_, mut notice) = post_note(tx, st, conversation, Speaker::Update { kind: LINE_KIND, from: "Aurelius" }, text)?;
    let project: Option<String> = tx.query_row("SELECT project_id FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0)).optional()?.flatten();
    notice.project_id = project.and_then(|p| aurelius_domain::ProjectId::parse(&p).ok());
    Ok(notice)
}

/// The dispatcher's job for a session that was just made, and the line that says it chooses.
pub(crate) fn request(tx: &Tx<'_>, st: &Store, conversation: ConversationId, dispatcher: EmployeeId, source: EventId) -> Result<Vec<EventNotice>, StorageError> {
    let name = crate::conversation::discussion::employee_name(tx, dispatcher)?;
    let notice = line(tx, st, conversation, &format!("{name} is choosing who takes this…"))?;
    let job = format!("session-dispatch:{conversation}");
    aurelius_storage::events::insert_job(tx, &job, SESSION_DISPATCH_JOB_KIND, source, &job, &json!({ "conversation_id": conversation }))?;
    Ok(vec![notice])
}

/// The session still waits for its dispatcher: not deleted, still `choosing`.
fn still_choosing(tx: &Tx<'_>, conversation: ConversationId) -> Result<Option<work_sessions::Session>, StorageError> {
    let live: bool = tx
        .query_row("SELECT archived_at_ms IS NULL FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0))
        .optional()?
        .unwrap_or(false);
    if !live {
        return Ok(None);
    }
    Ok(work_sessions::of_conversation(tx, conversation)?.filter(|s| s.dispatch == Some(SessionDispatch::Choosing)))
}

/// The board member's first message: what the chosen employee answers.
fn first_message(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<Option<(MessageId, Option<String>)>, StorageError> {
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT m.id, b.storage_key FROM messages m LEFT JOIN blobs b ON b.id = m.content_blob_id
             WHERE m.conversation_id = ?1 AND m.role = 'user' ORDER BY m.ordinal LIMIT 1",
            [conversation.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(id, key)| MessageId::parse(&id).ok().map(|id| (id, key))))
}

/// `employee` takes the session (`by`: the line that says who chose); it answers the first message now.
fn hand_to(tx: &Tx<'_>, st: &Store, conversation: ConversationId, session: &work_sessions::Session, employee: EmployeeId, said: &str) -> Result<(RunId, MessageId, Vec<EventNotice>), StorageError> {
    work_sessions::set_employee(tx, conversation, employee)?;
    work_sessions::set_dispatch(tx, conversation, None)?;
    let mut notices = vec![line(tx, st, conversation, said)?];
    let Some((message, _)) = first_message(tx, conversation)? else { return Err(StorageError::Rejected("the session has no first message".into())) };
    let meta = json!({ "user_id": session.started_by });
    let (run, queued) = queue_run(tx, &meta, &conversation.to_string(), conversation, message)?;
    notices.extend(queued);
    Ok((run, message, notices))
}

/// The dispatcher chose nobody (`why`): the board member chooses.
fn nobody(tx: &Tx<'_>, st: &Store, conversation: ConversationId, why: &str) -> Result<Vec<EventNotice>, StorageError> {
    work_sessions::set_dispatch(tx, conversation, Some(SessionDispatch::Nobody))?;
    let text = match why {
        "no_dispatcher" => "There is no active dispatcher. Choose who takes this session.",
        "dispatcher_did_not_choose" => "The dispatcher chose nobody. Choose who takes this session.",
        _ => "The dispatcher could not choose. Choose who takes this session.",
    };
    Ok(vec![line(tx, st, conversation, text)?])
}

/// One run of a session's dispatcher, as the daemon starts it.
#[derive(Debug, Clone)]
pub struct SessionDispatchRun {
    pub run_id: RunId,
    pub dispatcher: EmployeeId,
    pub config: crate::run::connections::ResolvedRunConfig,
    pub system: String,
    pub prompt: String,
}

impl CommandService {
    /// The next run of the session's dispatcher, made now; none when there is nothing
    /// to choose any more, no dispatcher, or its runs are used up (the session then
    /// says the dispatcher chose nobody).
    pub fn session_dispatch_run(&self, conversation: ConversationId) -> Result<(Option<SessionDispatchRun>, Vec<EventNotice>), AppError> {
        let st = self.store.clone();
        let prepared = self
            .store
            .write(move |tx| {
                let Some(session) = still_choosing(tx, conversation)? else { return Ok((None, vec![])) };
                let Some(dispatcher) = company_store::dispatcher_for(tx, session.project)? else { return Ok((None, nobody(tx, &st, conversation, "no_dispatcher")?)) };
                let earlier = work_sessions::dispatch_runs_so_far(tx, conversation)?;
                if earlier >= ATTEMPTS {
                    return Ok((None, nobody(tx, &st, conversation, "dispatcher_did_not_choose")?));
                }
                let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let identity = crate::task::tasks::employee_identity(tx, dispatcher, &read)?.map(|(_, _, system, _)| system).unwrap_or_default();
                let system = format!("{identity}\n\n{GUIDANCE}");
                let project: String = tx.query_row("SELECT name FROM projects WHERE id = ?1", [session.project.to_string()], |r| r.get(0))?;
                let text = first_message(tx, conversation)?.and_then(|(_, key)| key).and_then(|k| read(&k)).unwrap_or_default();
                let place = match &session.worktree {
                    Some((_, branch)) if session.own_branch => format!("in the project's folder on its own branch {branch}"),
                    Some((_, branch)) => format!("in a worktree of its own on {branch}"),
                    None => "in the project's folder".to_owned(),
                };
                let mut prompt = format!(
                    "Work session in project {project}, {place}. The board member's message:\n\n{}\n\nColleagues:\n{}",
                    text.trim(),
                    crate::task::triage::candidates(tx, &st, dispatcher)?
                );
                if earlier > 0 {
                    prompt.push_str(&format!("\n\n{RETRY}"));
                }
                Ok((Some((dispatcher, session.started_by, system, prompt)), vec![]))
            })
            .map_err(storage_error)?;
        let (Some((dispatcher, started_by, system, prompt)), notices) = prepared else { return Ok((None, prepared.1)) };
        // The login of the board member who started the session, as its runs have.
        let config = self.resolve_for_employee(dispatcher, aurelius_domain::connection::Purpose::Employee, Some(started_by))?;
        let run_id = RunId::new();
        let resolved = serde_json::to_string(&config).map_err(|e| AppError::new(aurelius_domain::ErrorCode::Internal, e.to_string()))?;
        let (kind, model) = (config.runtime_kind().to_owned(), config.model.clone().unwrap_or_default());
        let connection = config.connection_id.map(|c| c.to_string());
        self.store
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO runs(id, conversation_id, employee_id, attempt, runtime_kind, connection_id, model_id, resolved_config_json, status, started_at_ms, login_user)
                     VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, 'running', ?8, ?9)",
                    rusqlite::params![run_id.to_string(), conversation.to_string(), dispatcher.to_string(), kind, connection, model, resolved, aurelius_storage::now_ms(), started_by.to_string()],
                )?;
                work_sessions::insert_dispatch_run(tx, run_id, conversation)
            })
            .map_err(storage_error)?;
        Ok((Some(SessionDispatchRun { run_id, dispatcher, config, system, prompt }), notices))
    }

    /// A run of the session's dispatcher ended (`failed`: why). Chosen: done. Not
    /// chosen: one more run, then the board member chooses. Returns whether the job goes again.
    pub fn session_dispatch_ended(&self, conversation: ConversationId, run: RunId, failed: Option<String>) -> Result<(bool, Vec<EventNotice>), AppError> {
        let st = self.store.clone();
        self.store
            .write(move |tx| {
                tx.execute(
                    "UPDATE runs SET status = ?2, error_code = ?3, ended_at_ms = ?4 WHERE id = ?1",
                    rusqlite::params![run.to_string(), if failed.is_some() { "failed" } else { "succeeded" }, failed, aurelius_storage::now_ms()],
                )?;
                if still_choosing(tx, conversation)?.is_none() {
                    return Ok((false, vec![]));
                }
                if work_sessions::dispatch_runs_so_far(tx, conversation)? < ATTEMPTS {
                    return Ok((true, vec![]));
                }
                let why = if failed.is_some() { "dispatcher_failed" } else { "dispatcher_did_not_choose" };
                Ok((false, nobody(tx, &st, conversation, why)?))
            })
            .map_err(storage_error)
    }

    /// The session's dispatcher job was claimed (again): a run of an earlier claim
    /// that never ended ends now, as interrupted; it still counts as a try.
    pub fn session_dispatch_claimed(&self, conversation: ConversationId) -> Result<(), AppError> {
        self.store.write(move |tx| work_sessions::end_unfinished_dispatch_runs(tx, conversation)).map_err(storage_error)
    }

    /// The dispatcher's job gives up (`why`, e.g. `dispatcher_failed`): a session
    /// that still waits for it says the board member chooses.
    pub fn session_dispatch_gave_up(&self, conversation: ConversationId, why: &'static str) -> Result<Vec<EventNotice>, AppError> {
        let st = self.store.clone();
        self.store
            .write(move |tx| {
                if still_choosing(tx, conversation)?.is_none() {
                    return Ok(vec![]);
                }
                nobody(tx, &st, conversation, why)
            })
            .map_err(storage_error)
    }

    /// The dispatcher could not run for want of its login (`until_ms`: none) or its
    /// login's usage limit (`until_ms`: the reset): its run ends without counting as
    /// a try and its job (`job`, leased under `epoch`) waits.
    pub fn session_dispatch_waits(&self, conversation: ConversationId, run: RunId, job: String, epoch: i64, until_ms: Option<i64>) -> Result<Vec<EventNotice>, AppError> {
        let st = self.store.clone();
        self.store
            .write(move |tx| {
                let code = if until_ms.is_some() { "quota" } else { "login" };
                tx.execute(
                    "UPDATE runs SET status = 'cancelled', error_code = ?2, ended_at_ms = ?3 WHERE id = ?1",
                    rusqlite::params![run.to_string(), code, aurelius_storage::now_ms()],
                )?;
                let Some(session) = still_choosing(tx, conversation)? else { return Ok(vec![]) };
                let parked = match until_ms {
                    Some(at) => queries::reschedule_job(tx, &job, epoch, at, "quota", false)?,
                    None => queries::park_job_for_login(tx, &job, epoch, Some(&session.started_by.to_string()))?,
                };
                if !parked {
                    return Ok(vec![]);
                }
                let text = if until_ms.is_some() {
                    "The dispatcher waits until your Claude usage limit resets. You can also choose who takes this session."
                } else {
                    "The dispatcher waits for your Claude login; connect it in Settings. You can also choose who takes this session."
                };
                Ok(vec![line(tx, &st, conversation, text)?])
            })
            .map_err(storage_error)
    }

    /// `work_session.assign`: the board member who started the session chooses who takes it.
    pub(crate) fn assign_work_session(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &AssignWorkSession) -> Result<Committed, AppError> {
        let Some(user) = ctx.user_id else {
            return Err(domain_error(DomainError::BoardMemberRequired));
        };
        let (conversation, employee) = (cmd.conversation_id, cmd.employee_id);
        let st = self.store.clone();
        self.commit(envelope.command_id, hash, move |tx| {
            let not_found = || Ok(Err(domain_error(DomainError::ConversationNotFound(conversation))));
            let Some(session) = work_sessions::of_conversation(tx, conversation)? else { return not_found() };
            if session.started_by != user {
                return not_found();
            }
            if session.dispatch.is_none() {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "conversation_id".into(), detail: TAKEN.into() })));
            }
            let Some(chosen) = employees::load_employee(tx, employee)? else {
                return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee))));
            };
            match chosen.status {
                LifecycleStatus::Archived => return Ok(Err(domain_error(DomainError::EmployeeArchived))),
                LifecycleStatus::Paused => return Ok(Err(domain_error(DomainError::EmployeePaused))),
                LifecycleStatus::Active => {}
            }
            let who = crate::conversation::discussion::actor_name(tx, Some(aurelius_domain::ticket::Actor::BoardMember { user_id: user }))?;
            let (run_id, message_id, notices) = hand_to(tx, &st, conversation, &session, employee, &format!("{who} chose {}.", chosen.name))?;
            Ok(Ok((CommandResult::WorkSessionStarted { project_id: session.project, conversation_id: conversation, message_id, run_id }, notices)))
        })
    }
}

/// The bridge's `assign_session` from a session dispatcher's run: that employee
/// takes the session and answers its first message.
pub(crate) fn assign(tx: &Tx<'_>, st: &Store, run: RunId, employee: &str, reason: &str) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let refuse = |why: &str| Ok((false, why.to_string(), vec![]));
    let Some(conversation) = work_sessions::dispatch_run_session(tx, run)? else { return refuse("Only the dispatcher of a work session can assign it.") };
    let dispatcher: Option<String> = tx.query_row("SELECT employee_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).optional()?.flatten();
    let Some(dispatcher) = dispatcher.and_then(|d| EmployeeId::parse(&d).ok()) else { return refuse("Only the dispatcher of a work session can assign it.") };
    let Some(session) = still_choosing(tx, conversation)? else { return refuse("Somebody takes this session already (or it was deleted); nothing to do.") };
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let colleagues = crate::task::tasks::colleagues(tx, &read)?;
    let Some(chosen) = colleagues.iter().find(|c| c.name.trim().eq_ignore_ascii_case(employee.trim()) || c.id.to_string() == employee.trim()) else {
        let names: Vec<&str> = colleagues.iter().map(|c| c.name.as_str()).collect();
        return Ok((false, format!("No active colleague called {employee}. Choose one of: {}.", names.join(", ")), vec![]));
    };
    // Yourself only when nobody else could take it.
    if chosen.id == dispatcher && colleagues.iter().any(|c| c.id != dispatcher) {
        return refuse("Choose a colleague, not yourself: you dispatch the work.");
    }
    let reason = reason.trim().chars().take(aurelius_domain::ticket::REASON_MAX).collect::<String>();
    let by = crate::conversation::discussion::employee_name(tx, dispatcher)?;
    let said = if reason.is_empty() { format!("{by} chose {}.", chosen.name) } else { format!("{by} chose {}: {reason}", chosen.name) };
    let (_, _, notices) = hand_to(tx, st, conversation, &session, chosen.id, &said)?;
    Ok((true, format!("{} takes the session and answers now. Answer in one sentence.", chosen.name), notices))
}

#[cfg(test)]
#[path = "session_dispatch_tests.rs"]
mod tests;
