//! Whose Claude login a run uses (MU3-10, impact analysis §3 "Resolutie"). It is
//! decided when the run is queued, kept in its snapshot (`ResolvedRunConfig::
//! login_user`) and in `runs.login_user`, and never changes after: a retry or a
//! run that waited for a limit goes on on the same login (A-02).
//!
//! - A task's run (a ticket's main task, its triage, a delegation at any depth,
//!   the summary after Done): the ticket's `login_user` (its creator unless a
//!   board member took it over, `ticket.use_my_login`). A task without a ticket:
//!   the login it was made with (`tasks.login_user`: the board member who made
//!   it, the run that delegated it or a schedule's), else its parent's.
//! - A run the Team chat or a discussion woke: the login of the board member of
//!   hop 0, carried in the marking (`GroupRun::login_user`), also at hop 2–3.
//! - A chat run (an employee, the project assistant): the board member who wrote
//!   the message; else the one whose command queued it (a decision); a report of
//!   a delegated task: that task's login (B5).
//! - Background jobs without a source (memory consolidation): the company's login
//!   (`company.background_login_user`, B6).
//!
//! There is no fallback to anyone else's login or to the daemon's: without one
//! the run waits with 'login'.

use aurelius_domain::{ConversationId, MessageId, TaskId, UserId};
use aurelius_storage::StorageError;
use rusqlite::{Connection, OptionalExtension};

fn user(text: Option<String>) -> Option<UserId> {
    text.and_then(|t| UserId::parse(&t).ok())
}

/// The login a task's runs use: its ticket's, else its own, else its parent's,
/// else its schedule's.
pub(crate) fn of_task(conn: &Connection, task: TaskId) -> Result<Option<UserId>, StorageError> {
    let mut at = Some(task.to_string());
    // A chain of delegations is short; the bound only guards against a cycle.
    for _ in 0..64 {
        let Some(id) = at.take() else { return Ok(None) };
        let row: Option<(Option<String>, Option<String>, Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT (SELECT k.login_user FROM tickets k WHERE k.id = t.ticket_id), t.login_user, t.parent_task_id,
                        (SELECT s.login_user FROM recurring_tasks s WHERE s.id = t.schedule_id)
                 FROM tasks t WHERE t.id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let Some((ticket, own, parent, schedule)) = row else { return Ok(None) };
        if let Some(login) = user(ticket).or(user(own)) {
            return Ok(Some(login));
        }
        if parent.is_none() {
            return Ok(user(schedule));
        }
        at = parent;
    }
    Ok(None)
}

/// The task whose conversation `conversation` is, if it is one.
fn task_of(conn: &Connection, conversation: ConversationId) -> Result<Option<TaskId>, StorageError> {
    let id: Option<String> = conn.query_row("SELECT id FROM tasks WHERE conversation_id = ?1", [conversation.to_string()], |r| r.get(0)).optional()?;
    Ok(id.and_then(|t| TaskId::parse(&t).ok()))
}

/// The login of the run `run` (its snapshot's), if it had one.
pub(crate) fn of_run(conn: &Connection, run: aurelius_domain::RunId) -> Result<Option<UserId>, StorageError> {
    Ok(user(aurelius_storage::conversations::run_login_user(conn, run)?))
}

/// The company's login for background jobs without a source (B6); none: they do not run.
pub(crate) fn of_company(conn: &Connection) -> Result<Option<UserId>, StorageError> {
    aurelius_storage::company::background_login_user(conn)
}

/// The login of a run queued in `conversation` for `reply_to`. `group`: the
/// login of hop 0 when the Team chat or a discussion woke it. `meta`: the queuing
/// command's metadata (`login_user` set by the caller, the board member
/// `user_id`, or the `task_id` it reports for).
pub(crate) fn for_run(conn: &Connection, conversation: ConversationId, reply_to: MessageId, group: Option<UserId>, meta: &serde_json::Value) -> Result<Option<UserId>, StorageError> {
    if let Some(login) = group {
        return Ok(Some(login));
    }
    // Set by the caller (a scheduled message: the login it was planned with, null when none).
    if let Some(login) = meta.get("login_user") {
        return Ok(login.as_str().and_then(|u| UserId::parse(u).ok()));
    }
    // A task's conversation runs on the task's login, whoever writes in it.
    if let Some(task) = task_of(conn, conversation)? {
        return of_task(conn, task);
    }
    let author: Option<Option<String>> = conn.query_row("SELECT author_user_id FROM messages WHERE id = ?1", [reply_to.to_string()], |r| r.get(0)).optional()?;
    if let Some(login) = user(author.flatten()) {
        return Ok(Some(login));
    }
    if let Some(login) = meta["user_id"].as_str().and_then(|u| UserId::parse(u).ok()) {
        return Ok(Some(login));
    }
    // A delegated task reporting back: the login it ran on. Nothing else: never
    // the login some earlier run of the conversation had (B1-1).
    match meta["task_id"].as_str().and_then(|t| TaskId::parse(t).ok()) {
        Some(task) => of_task(conn, task),
        None => Ok(None),
    }
}
