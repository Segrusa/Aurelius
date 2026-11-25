//! Quick tasks (protocol 5, feature `quick_task`, ticket #53): a kind of ticket for a
//! small piece of work. It starts at once in Doing with the employee the board member
//! chose, in Auto mode; it never gets triage, Plan review or Testing, and its main task
//! cannot be delegated. The employee works on its own branch `quick/ticket-<N>` in a
//! worktree of the project's folder (`worktree_path`), made by the daemon before the
//! first run. It always ends in Review: Done ("Approve and merge") merges the branch into
//! the project's local main, as the daemon's job `QUICK_MERGE_JOB_KIND` after the commit;
//! only a merge that succeeded (or a branch without commits) puts the card on Done. A
//! cancel removes the worktree and the branch (`QUICK_CLEANUP_JOB_KIND`). Nothing is pushed.
//!
//! The git work itself is the daemon's (IO at the edge); this module is its database side.

use crate::{CommandService, domain_error, storage_error};
use aurelius_domain::{
    AppError, DomainError, ErrorCode, EventId, EventKind, EventNotice, TicketId, UserId,
    autonomy::AgentMode,
    task::TaskStatus,
    ticket::{self, Actor, ColumnKind, CreateTicket, TicketKind},
    views::{SystemLine, SystemLineKind, system_action},
};
use aurelius_storage::{StorageError, Store, Tx, company as company_store, tickets as store};
use serde_json::json;
use std::path::{Path, PathBuf};

/// The daemon's job that merges an accepted quick task into its project's main.
pub const QUICK_MERGE_JOB_KIND: &str = "ticket.quick_merge";
/// The daemon's job that removes a cancelled quick task's worktree and branch (BR4), or a
/// cancelled or done ticket's worktrees (ticket #86).
pub const QUICK_CLEANUP_JOB_KIND: &str = "ticket.quick_cleanup";

/// ST-02 AC2: what `delegate_task` answers in a quick task's main task.
pub const DELEGATE_REFUSED: &str = "Snelle taak: doe dit zelf of vraag de board member om er een gewoon ticket van te maken";
/// BR3: a quick task needs a project whose folder is a git repository.
pub const NO_GIT_FOLDER: &str = "Dit project heeft geen git-map";
/// ST-01 AC2: a quick task needs the employee who does it.
pub const OWNER_REQUIRED: &str = "A quick task needs the employee who does it.";
/// ST-01 AC2: a quick task needs a description (its title comes from it when empty).
pub const DESCRIPTION_REQUIRED: &str = "A quick task needs a description.";
/// A quick task always runs in Auto mode (ST-01 AC4).
pub const PLAN_REFUSED: &str = "A quick task always runs in Auto mode; it never gets Plan mode.";
/// Only one merge of a quick task at a time.
pub const MERGE_PENDING: &str = "This quick task is being merged already; wait for the result.";

/// The folder of a quick task's worktree: in the data folder, so the project's own folder
/// stays clean (a main with untracked files would refuse the merge) and a backup skips it.
pub fn worktree_path(data_root: &Path, number: u32) -> PathBuf {
    data_root.join("worktrees").join(format!("ticket-{number}"))
}

/// Whether `folder` is the top of a git repository (its `.git` is a folder or, in a
/// worktree or submodule, a file). The daemon's git commands check the rest.
pub fn is_git_folder(folder: &Path) -> bool {
    folder.is_dir() && folder.join(".git").exists()
}

/// The title of a quick task without one: the first line of its description, cut to `TITLE_MAX`.
pub fn title_from(description: &str) -> String {
    let line = description.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    let line = line.trim_start_matches('#').trim();
    if line.chars().count() <= ticket::TITLE_MAX {
        return line.to_owned();
    }
    let cut: String = line.chars().take(ticket::TITLE_MAX - 1).collect();
    format!("{}…", cut.trim_end())
}

/// A quick task's create, before the usual checks: its employee and description are
/// required, the title comes from the description when empty, its creator is responsible
/// unless someone else is chosen, it runs in Auto, and its project has a git folder (BR3).
pub(crate) fn prepare(st: &Store, cmd: &mut CreateTicket, user: UserId) -> Result<(), AppError> {
    if cmd.owner_employee_id.is_none() {
        return Err(AppError::new(ErrorCode::Validation, OWNER_REQUIRED));
    }
    let description = cmd.description.as_deref().map(str::trim).unwrap_or_default();
    if description.is_empty() {
        return Err(AppError::new(ErrorCode::Validation, DESCRIPTION_REQUIRED));
    }
    if cmd.title.trim().is_empty() {
        cmd.title = title_from(description);
    }
    if cmd.run.mode.is_some_and(|m| m != AgentMode::Auto) {
        return Err(AppError::new(ErrorCode::Validation, PLAN_REFUSED));
    }
    cmd.run.mode = Some(AgentMode::Auto);
    cmd.start = true;
    // ST-01 AC3: the creator is responsible, unless the board member chose someone.
    cmd.responsible_user_id = cmd.responsible_user_id.or(Some(user));
    let project = cmd.project_id;
    let folder = st.read(|c| aurelius_storage::files::project_folder(c, project)).map_err(storage_error)?;
    if !folder.as_deref().is_some_and(|f| is_git_folder(Path::new(f))) {
        return Err(AppError::new(ErrorCode::Validation, NO_GIT_FOLDER));
    }
    Ok(())
}

/// Whether the task `task` is the main task of a quick task (its delegations are refused).
pub(crate) fn is_quick_root(conn: &rusqlite::Connection, task: aurelius_domain::TaskId) -> Result<bool, StorageError> {
    let Some((ticket, _, _)) = crate::task::ticket_line::root_of(conn, task)? else { return Ok(false) };
    Ok(store::state(conn, ticket)?.is_some_and(|s| s.kind == TicketKind::Quick))
}

/// Whether a merge job of this ticket is waiting or running.
fn merge_pending(conn: &rusqlite::Connection, ticket: TicketId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE kind = ?1 AND state IN ('ready','leased') AND json_extract(payload_json, '$.ticket_id') = ?2)",
        rusqlite::params![QUICK_MERGE_JOB_KIND, ticket.to_string()],
        |r| r.get(0),
    )?)
}

/// `ticket.accept` of a quick task, in its transaction: the card stays in Review, the
/// discussion says the merge started, and the daemon's merge job is queued (`source`: an
/// event of this write). `Err(AppError)`: a merge is under way already.
pub(crate) fn request_merge(tx: &Tx<'_>, state: &store::TicketState, by: Actor, user: UserId, source: EventId, command: &str) -> Result<Result<Vec<EventNotice>, AppError>, StorageError> {
    if merge_pending(tx, state.id)? {
        return Ok(Err(domain_error(DomainError::Ticket(MERGE_PENDING.into()))));
    }
    let job = format!("quick-merge:{}:{command}", state.id);
    aurelius_storage::events::insert_job(tx, &job, QUICK_MERGE_JOB_KIND, source, &job, &json!({ "ticket_id": state.id, "user_id": user }))?;
    let name = crate::conversation::discussion::actor_name(tx, Some(by))?;
    let branch = ticket::quick_branch(state.number);
    let mut line = SystemLine::new(SystemLineKind::Moved).with_action(system_action::APPROVED);
    line.by = Some(name.clone());
    Ok(Ok(crate::conversation::discussion::ticket_line(tx, state.id, line, &format!("{name} approved the quick task: merging {branch} into main…"))?))
}

/// A cancel of a quick task (BR4), in its transaction: the daemon removes its worktree and
/// branch. For a normal ticket's cancel or Done (ticket #86): the daemon removes the
/// worktrees of its tasks and keeps their branches that have work of their own.
pub(crate) fn request_cleanup(tx: &Tx<'_>, state: &store::TicketState, source: EventId, command: &str) -> Result<(), StorageError> {
    let job = format!("quick-cleanup:{}:{command}", state.id);
    aurelius_storage::events::insert_job(tx, &job, QUICK_CLEANUP_JOB_KIND, source, &job, &json!({ "ticket_id": state.id }))
}

/// What the daemon needs to merge or clean up a quick task.
#[derive(Debug, Clone, PartialEq)]
pub struct QuickRepo {
    pub ticket: TicketId,
    pub number: u32,
    /// The project's folder: the repository whose main the branch goes into.
    pub folder: PathBuf,
    pub branch: String,
    pub worktree: PathBuf,
    /// The title, for the merge commit's message.
    pub title: String,
    /// Its employee works on it (Todo or Doing): the worktree may be made.
    pub working: bool,
}

/// How the daemon's merge ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Merged with `--no-ff`: the merge commit.
    Merged(String),
    /// The branch has no commits of its own: nothing to merge (ST-05 AC5).
    NoCommits,
    /// Nothing changed in main: why (main not clean, branch missing, a conflict) and the files concerned.
    Failed { reason: String, files: Vec<String> },
}

impl CommandService {
    /// The repository, branch and worktree of a quick task; none for a normal ticket or one without a folder.
    pub fn quick_repo(&self, ticket: TicketId) -> Result<Option<QuickRepo>, AppError> {
        let root = self.store().root().to_path_buf();
        self.store()
            .read(|c| {
                let Some(state) = store::state(c, ticket)? else { return Ok(None) };
                if state.kind != TicketKind::Quick {
                    return Ok(None);
                }
                let Some(folder) = store::project_folder(c, ticket)? else { return Ok(None) };
                let title: String = c.query_row("SELECT title FROM tickets WHERE id = ?1", [ticket.to_string()], |r| r.get(0))?;
                Ok(Some(QuickRepo {
                    ticket,
                    number: state.number,
                    folder: PathBuf::from(folder),
                    branch: ticket::quick_branch(state.number),
                    worktree: worktree_path(&root, state.number),
                    title,
                    working: !state.archived && matches!(state.column, ColumnKind::Todo | ColumnKind::Doing),
                }))
            })
            .map_err(storage_error)
    }

    /// The quick task a conversation works for (its main task's, or its discussion), if any.
    pub fn quick_repo_of_conversation(&self, conversation: aurelius_domain::ConversationId) -> Result<Option<QuickRepo>, AppError> {
        match self.store().read(|c| store::quick_of_conversation(c, conversation)).map_err(storage_error)? {
            Some((ticket, _)) => self.quick_repo(ticket),
            None => Ok(None),
        }
    }

    /// The daemon's merge ended: on a merge (or nothing to merge) the card goes to Done as
    /// `ticket.accept` puts it there, with the commit; on a failure it stays in Review with a
    /// line that says why. Either way the result is in the discussion and the activity.
    /// Returns whether the worktree and branch may go now (the card is on Done).
    pub fn quick_merge_finished(&self, ticket: TicketId, user: UserId, outcome: MergeOutcome) -> Result<(bool, Vec<EventNotice>), AppError> {
        let st = self.store().clone();
        self.store().write(move |tx| merge_finished(tx, &st, ticket, user, &outcome)).map_err(storage_error)
    }

    /// The worktrees of a normal ticket's tasks, to remove once it is no longer worked on
    /// (cancelled or Done); none while it is in Todo or Doing again, or for a quick task.
    pub fn ticket_worktrees(&self, ticket: TicketId) -> Result<Vec<aurelius_storage::work_places::WorkPlace>, AppError> {
        let worktrees = aurelius_domain::work_place::worktrees_dir(self.store().root());
        self.store()
            .read(|c| {
                let Some(state) = store::state(c, ticket)? else { return Ok(vec![]) };
                if state.kind == TicketKind::Quick || (!state.archived && matches!(state.column, ColumnKind::Todo | ColumnKind::Doing)) {
                    return Ok(vec![]);
                }
                aurelius_storage::work_places::of_ticket_tasks(c, &worktrees, ticket)
            })
            .map_err(storage_error)
    }

    /// The daemon could not remove a cancelled quick task's worktree or branch: the discussion says so.
    pub fn quick_cleanup_failed(&self, ticket: TicketId, reason: String) -> Result<Vec<EventNotice>, AppError> {
        self.store()
            .write(move |tx| {
                let line = SystemLine::new(SystemLineKind::Moved);
                crate::conversation::discussion::ticket_line(tx, ticket, line, &format!("Aurelius could not remove the quick task's worktree and branch: {reason}"))
            })
            .map_err(storage_error)
    }
}

/// The line of a merge in the discussion and what goes in the activity's event.
fn merge_text(branch: &str, outcome: &MergeOutcome) -> String {
    match outcome {
        MergeOutcome::Merged(commit) => format!("Merged {branch} into main as {}. Nothing was pushed.", short(commit)),
        MergeOutcome::NoCommits => format!("{branch} had no commits of its own: nothing was merged."),
        MergeOutcome::Failed { reason, files } if files.is_empty() => format!("The merge of {branch} into main failed: {reason}. Main is unchanged; the quick task stays in Review."),
        MergeOutcome::Failed { reason, files } => {
            format!("The merge of {branch} into main failed: {reason}. Files: {}. Main is unchanged; the quick task stays in Review.", files.join(", "))
        }
    }
}

fn short(commit: &str) -> &str {
    commit.get(..10).unwrap_or(commit)
}

fn merge_finished(tx: &Tx<'_>, st: &Store, ticket: TicketId, user: UserId, outcome: &MergeOutcome) -> Result<(bool, Vec<EventNotice>), StorageError> {
    let Some(state) = store::state(tx, ticket)? else { return Ok((false, vec![])) };
    let branch = ticket::quick_branch(state.number);
    let by = Actor::BoardMember { user_id: user };
    let text = merge_text(&branch, outcome);
    let mut line = SystemLine::new(SystemLineKind::Moved);
    line.by = Some("Aurelius".into());
    let ready = !state.archived && state.column == ColumnKind::Review && state.root_status == Some(TaskStatus::Review);
    let commit = match outcome {
        MergeOutcome::Merged(commit) => Some(commit.as_str()),
        MergeOutcome::NoCommits => None,
        MergeOutcome::Failed { reason, files } => {
            let mut notices = vec![crate::task::ticket_line::ticket_event(
                tx,
                ticket,
                state.project_id,
                EventKind::TicketUpdated,
                Some(by),
                json!({ "changed": ["merge"], "merge_error": reason, "files": files }),
            )?];
            notices.extend(crate::conversation::discussion::ticket_line(tx, ticket, line, &text)?);
            return Ok((false, notices));
        }
    };
    if let Some(commit) = commit {
        store::set_merge_commit(tx, ticket, commit)?;
    }
    let mut notices = crate::conversation::discussion::ticket_line(tx, ticket, line, &text)?;
    if !ready {
        // Sent back or moved while the merge ran: the board member decides again.
        return Ok((false, notices));
    }
    let done = company_store::column_of_kind(tx, ColumnKind::Done)?;
    store::move_to(tx, ticket, done.id, None)?;
    store::mark_done(tx, ticket)?;
    notices.push(crate::task::ticket_line::ticket_event(tx, ticket, state.project_id, EventKind::TicketMoved, Some(by), json!({ "from": state.column, "to": ColumnKind::Done }))?);
    notices.push(crate::task::ticket_line::ticket_event(tx, ticket, state.project_id, EventKind::TicketAccepted, Some(by), json!({ "note": serde_json::Value::Null, "merge_commit": commit }))?);
    let mut done_line = SystemLine::new(SystemLineKind::Done).with_action(system_action::APPROVED);
    done_line.by = Some(crate::conversation::discussion::actor_name(tx, Some(by))?);
    notices.extend(crate::conversation::discussion::ticket_line(tx, ticket, done_line, "")?);
    notices.extend(crate::task::ticket_line::accept(tx, st, ticket, user)?);
    notices.extend(crate::conversation::ticket_channel::sync(tx, ticket, Some(by))?);
    Ok((true, notices))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_comes_from_the_first_line_of_the_description() {
        assert_eq!(title_from("\n  # Fix the typo  \nin the footer"), "Fix the typo");
        let long = "x".repeat(ticket::TITLE_MAX + 5);
        let title = title_from(&long);
        assert_eq!(title.chars().count(), ticket::TITLE_MAX);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn a_failed_merge_names_its_reason_and_files() {
        let text = merge_text("quick/ticket-7", &MergeOutcome::Failed { reason: "a conflict".into(), files: vec!["a.rs".into(), "b.rs".into()] });
        assert!(text.contains("a conflict") && text.contains("a.rs, b.rs") && text.contains("Main is unchanged"), "{text}");
        assert!(merge_text("quick/ticket-7", &MergeOutcome::Merged("0123456789abcdef".into())).contains("0123456789"));
    }
}

#[cfg(test)]
#[path = "quick_tests.rs"]
mod quick_tests;
