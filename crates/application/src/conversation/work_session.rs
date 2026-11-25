//! Work sessions (feature `work_sessions`): a board member starts one employee in
//! one project's folder by typing, without a ticket (see
//! `aurelius_domain::work_session`). This is the database side; the worktree, when
//! the session asked for one, is made by the daemon before the first run, and a
//! branch of its own is checked out by the daemon before each run (`aurelius_daemon::quick`).

use crate::run::events::{Ev, emit};
use crate::run::queue::queue_run;
use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, AppError, CommandEnvelope, CommandResult, ConversationId, DomainError, MessageId, UserId,
    conversation::{MessageRole, MessageStatus},
    employee::LifecycleStatus,
    work_session::{DispatchWorkSession, SessionDispatch, StartWorkSession, branch_of},
};
use aurelius_storage::{
    blobs,
    conversations::{self as conv, NewMessage},
    employees,
};
use aurelius_storage::{StorageError, Tx};
use rusqlite::OptionalExtension;
use std::path::{Path, PathBuf};

/// Refused: a worktree needs a project folder that is a git repository.
pub const WORKTREE_NEEDS_GIT: &str = "A worktree of its own needs a project folder that is a git repository; start the session without one, or set the project's folder.";

/// Refused: a branch of its own needs a project folder that is a git repository.
pub const OWN_BRANCH_NEEDS_GIT: &str = "A branch of its own needs a project folder that is a git repository; start the session without one, or set the project's folder.";

/// Refused: a design writes its pages, so its mode stays Auto.
pub const DESIGN_STAYS_AUTO: &str = "A design session always works in Auto: it writes its pages, which Plan or Ask would stop.";

/// Refused: a session works either in a worktree of its own or on its own branch.
pub const WORKTREE_OR_OWN_BRANCH: &str = "Choose a worktree of its own or a branch of its own, not both.";

/// Refused: files with the first message of a session in a worktree of its own (that
/// folder is made only before the first run).
pub const FILES_NEED_FOLDER: &str = "Files can go with a later message of a session in its own worktree; start it without files, or in the project folder.";

/// Why `user` cannot write in `conversation` now, if so: a work session of another
/// board member (or one whose employee is not at work, or still being chosen), an
/// older employee conversation, or a conversation that does not exist. Other
/// conversations `chat.send` takes are left to their own checks.
pub(crate) fn write_refusal(tx: &Tx<'_>, conversation: ConversationId, user: Option<UserId>) -> Result<Option<AppError>, StorageError> {
    let not_found = || domain_error(DomainError::ConversationNotFound(conversation));
    // A work session: only the board member who started it writes in it, while its employee is at work.
    if let Some(session) = aurelius_storage::work_sessions::of_conversation(tx, conversation)? {
        if user != Some(session.started_by) {
            return Ok(Some(not_found()));
        }
        // The dispatcher still chooses, or chose nobody: the employee who answers comes first.
        if session.dispatch.is_some() {
            return Ok(Some(domain_error(DomainError::OutOfRange { field: "conversation_id".into(), detail: crate::conversation::session_dispatch::NOBODY_YET.into() })));
        }
        let Some(employee) = conv::owner_of(tx, conversation)?.0 else { return Ok(Some(not_found())) };
        let Some(employee) = employees::load_employee(tx, employee)? else { return Ok(Some(not_found())) };
        return Ok(match employee.status {
            LifecycleStatus::Archived => Some(domain_error(DomainError::EmployeeArchived)),
            LifecycleStatus::Paused => Some(domain_error(DomainError::EmployeePaused)),
            LifecycleStatus::Active => None,
        });
    }
    if conv::is_older_employee_chat(tx, conversation)? {
        return Ok(Some(domain_error(DomainError::OutOfRange {
            field: "conversation_id".into(),
            detail: "This is an older conversation; write in the employee's chat".into(),
        })));
    }
    Ok((!conv::conversation_exists(tx, conversation)?).then(not_found))
}

/// The folder of a session's worktree: in the data folder, as a quick task's, so the
/// project's own folder stays clean and a backup skips it.
pub fn worktree_path(data_root: &Path, conversation: ConversationId) -> PathBuf {
    let branch = branch_of(conversation);
    data_root.join("worktrees").join(branch.replace('/', "-"))
}

/// What `work_session.start` and `work_session.dispatch` both take; `employee`:
/// none when the dispatcher chooses.
struct Start<'a> {
    project_id: aurelius_domain::ProjectId,
    employee: Option<aurelius_domain::EmployeeId>,
    text: &'a str,
    images: &'a [String],
    files: &'a [String],
    worktree: bool,
    own_branch: bool,
    run: &'a aurelius_domain::task::RunSettings,
    design: bool,
}

impl CommandService {
    pub(crate) fn start_work_session(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &StartWorkSession) -> Result<Committed, AppError> {
        let start = Start { project_id: cmd.project_id, employee: Some(cmd.employee_id), text: &cmd.text, images: &cmd.images, files: &cmd.files, worktree: cmd.worktree, own_branch: cmd.own_branch, run: &cmd.run, design: cmd.design };
        self.start_session(ctx, envelope, hash, start)
    }

    /// `work_session.dispatch`: as `work_session.start`, and the dispatcher chooses who takes it (`session_dispatch`).
    pub(crate) fn dispatch_work_session(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &DispatchWorkSession) -> Result<Committed, AppError> {
        let start = Start { project_id: cmd.project_id, employee: None, text: &cmd.text, images: &cmd.images, files: &cmd.files, worktree: cmd.worktree, own_branch: cmd.own_branch, run: &cmd.run, design: cmd.design };
        self.start_session(ctx, envelope, hash, start)
    }

    fn start_session(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: Start<'_>) -> Result<Committed, AppError> {
        let Some(user) = ctx.user_id else {
            return Err(domain_error(DomainError::BoardMemberRequired));
        };
        let text = aurelius_domain::conversation::validate_employee_message(cmd.text, cmd.images, cmd.files).map_err(domain_error)?;
        let run = cmd.run.validated().map_err(domain_error)?;
        if cmd.worktree && cmd.own_branch {
            return Err(domain_error(DomainError::OutOfRange { field: "own_branch".into(), detail: WORKTREE_OR_OWN_BRANCH.into() }));
        }
        if cmd.worktree && !cmd.files.is_empty() {
            return Err(domain_error(DomainError::OutOfRange { field: "files".into(), detail: FILES_NEED_FOLDER.into() }));
        }
        let sources = crate::conversation::message_uploads::check(cmd.files)?;
        let images = crate::conversation::message_images::prepare(&self.store, cmd.images)?;
        // Files go into the project's folder before the commit; a failed or replayed
        // command removes them again (as a chat message does).
        let uploaded = if sources.is_empty() {
            None
        } else {
            let folders = self.store.read(|c| aurelius_storage::tasks::work_folders(c, cmd.project_id, cmd.employee)).map_err(storage_error)?;
            let folder = crate::conversation::message_uploads::work_folder(&folders)?;
            let uploads = crate::conversation::message_uploads::copy(&folder, &sources)?;
            Some((folder, uploads))
        };
        let uploads = uploaded.as_ref().map(|(_, u)| u.clone()).unwrap_or_default();
        let text_blob = self.store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let (project_id, asked, worktree, own_branch, design) = (cmd.project_id, cmd.employee, cmd.worktree, cmd.own_branch, cmd.design);
        let conversation_id = ConversationId::new();
        let worktree_folder = worktree.then(|| worktree_path(self.store.root(), conversation_id));
        let message_id = MessageId::new();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        let st = self.store.clone();
        let committed = self.commit(command_id, hash, move |tx| {
            match employees::project_scope(tx, project_id)? {
                None => return Ok(Err(domain_error(DomainError::ProjectNotFound(project_id)))),
                Some((_, LifecycleStatus::Archived)) => return Ok(Err(domain_error(DomainError::ProjectArchived))),
                Some(_) => {}
            }
            // Without an employee, the dispatcher holds the session until it has chosen.
            let employee_id = match asked {
                Some(employee) => employee,
                None => match aurelius_storage::company::dispatcher_for(tx, project_id)? {
                    Some(dispatcher) => dispatcher,
                    None => return Ok(Err(domain_error(DomainError::OutOfRange { field: "employee_id".into(), detail: crate::conversation::session_dispatch::NO_DISPATCHER.into() }))),
                },
            };
            let dispatch = asked.is_none().then_some(SessionDispatch::Choosing);
            let Some(employee) = employees::load_employee(tx, employee_id)? else {
                return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee_id))));
            };
            match employee.status {
                LifecycleStatus::Archived => return Ok(Err(domain_error(DomainError::EmployeeArchived))),
                LifecycleStatus::Paused => return Ok(Err(domain_error(DomainError::EmployeePaused))),
                LifecycleStatus::Active => {}
            }
            let worktree = match &worktree_folder {
                // On its own branch: the "worktree" is the project's folder itself.
                None if own_branch => {
                    let project_folder: Option<String> = tx.query_row("SELECT folder FROM projects WHERE id = ?1", [project_id.to_string()], |r| r.get(0)).optional()?.flatten();
                    match project_folder.filter(|f| crate::ticket::quick::is_git_folder(Path::new(f))) {
                        Some(folder) => Some((folder, branch_of(conversation_id))),
                        None => return Ok(Err(domain_error(DomainError::OutOfRange { field: "own_branch".into(), detail: OWN_BRANCH_NEEDS_GIT.into() }))),
                    }
                }
                None => None,
                Some(folder) => {
                    let project_folder: Option<String> = tx.query_row("SELECT folder FROM projects WHERE id = ?1", [project_id.to_string()], |r| r.get(0)).optional()?.flatten();
                    if !project_folder.is_some_and(|f| crate::ticket::quick::is_git_folder(Path::new(&f))) {
                        return Ok(Err(domain_error(DomainError::OutOfRange { field: "worktree".into(), detail: WORKTREE_NEEDS_GIT.into() })));
                    }
                    Some((folder.to_string_lossy().into_owned(), branch_of(conversation_id)))
                }
            };
            let title = aurelius_domain::conversation::derive_title(&text);
            aurelius_storage::work_sessions::insert(tx, conversation_id, project_id, employee_id, &title, user, worktree.as_ref().map(|(f, b)| (f.as_str(), b.as_str())), dispatch, design, own_branch)?;
            // A design writes its pages: Plan or Ask would stop it, so a design is always Auto.
            let mode = if design { Some(aurelius_domain::autonomy::AgentMode::Auto) } else { run.mode };
            if let Some(mode) = mode {
                conv::set_agent_mode(tx, conversation_id, mode)?;
            }
            if run.browser_allowed {
                conv::set_browser_allowed(tx, conversation_id, true)?;
            }
            if run.screen_allowed {
                conv::set_screen_allowed(tx, conversation_id, true)?;
            }
            if run.model.is_some() || run.effort.is_some() {
                let only = aurelius_domain::task::RunSettings { model: run.model.clone(), effort: run.effort.clone(), ..Default::default() };
                conv::set_conversation_run(tx, conversation_id, &only)?;
            }
            let mut started = serde_json::json!({ "project_id": project_id, "employee_id": employee_id, "work_session": true, "ctx": meta });
            if dispatch.is_some() {
                started["dispatch"] = true.into();
            }
            if let Some((folder, branch)) = &worktree {
                started["worktree"] = folder.clone().into();
                started["branch"] = branch.clone().into();
                if own_branch {
                    started["own_branch"] = true.into();
                }
            }
            let (_, mut n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: conversation_id.to_string(),
                event_type: "conversation.started",
                actor: ActorKind::User,
                actor_id: Some(actor.clone()),
                correlation: command_id.to_string(),
                payload: None,
                metadata: started,
                dedupe: format!("cmd:{command_id}:conversation"),
            })?;
            n.project_id = Some(project_id);
            let mut notices = vec![n];
            blobs::register(tx, &text_blob)?;
            let (event_id, mut n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: conversation_id.to_string(),
                event_type: "message.user_committed",
                actor: ActorKind::User,
                actor_id: Some(actor),
                correlation: command_id.to_string(),
                payload: Some(text_blob.id.to_string()),
                metadata: meta.clone(),
                dedupe: format!("cmd:{command_id}:message"),
            })?;
            n.project_id = Some(project_id);
            notices.push(n);
            conv::insert_message_by(
                tx,
                NewMessage {
                    id: message_id,
                    conversation_id,
                    role: MessageRole::User,
                    content_blob_id: Some(text_blob.id.to_string()),
                    source_event_id: event_id,
                    status: MessageStatus::Complete,
                    run_id: None,
                    reply_to: None,
                },
                Some(user),
            )?;
            crate::conversation::message_images::keep(tx, message_id, &images)?;
            conv::set_message_uploads(tx, message_id, &uploads)?;
            if dispatch.is_some() {
                notices.extend(crate::conversation::session_dispatch::request(tx, &st, conversation_id, employee_id, event_id)?);
                return Ok(Ok((CommandResult::WorkSessionDispatching { project_id, conversation_id, message_id }, notices)));
            }
            let (run_id, run_notice) = queue_run(tx, &meta, &command_id.to_string(), conversation_id, message_id)?;
            notices.extend(run_notice);
            Ok(Ok((CommandResult::WorkSessionStarted { project_id, conversation_id, message_id, run_id }, notices)))
        });
        if let Some((folder, uploads)) = &uploaded
            && committed.as_ref().map_or(true, |c| c.receipt.replayed)
        {
            crate::conversation::message_uploads::remove(folder, uploads);
        }
        committed
    }
}

#[cfg(test)]
#[path = "work_session_tests.rs"]
mod tests;
