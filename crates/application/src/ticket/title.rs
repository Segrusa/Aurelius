//! A ticket's title made from its description (feature `ticket_made_title`). A
//! `ticket.create` with an empty title and a description (as the New session view
//! sends) starts with the first line of the description as its title, and the job
//! `ticket.title` asks a model for a short title from the description
//! (`aurelius_daemon::memory::run_ticket_title`). `record_ticket_title` puts it on
//! the ticket only while the title is still that first line: a title a board member
//! typed in the meantime wins. One try per ticket: when it fails, the first line stays.

use crate::{CommandService, storage_error};
use aurelius_domain::work_session::{SESSION_TITLE_INPUT_MAX_CHARS, session_title};
use aurelius_domain::{AppError, EventId, EventKind, EventNotice, TicketId, ticket::ColumnKind};
use aurelius_storage::{StorageError, Tx, company as company_store, events, queries, tickets as store};
use rusqlite::OptionalExtension;

/// The job that makes a ticket's title.
pub const TICKET_TITLE_JOB_KIND: &str = "ticket.title";

/// Everything one ticket title needs.
#[derive(Debug, Clone)]
pub struct TicketTitleInput {
    pub ticket_id: TicketId,
    pub project_name: String,
    /// The start of the description, cut at `SESSION_TITLE_INPUT_MAX_CHARS`.
    pub description: String,
    pub config: crate::run::connections::ResolvedRunConfig,
}

/// Queue the title job of a ticket just created (in the create's transaction).
/// `placeholder` is the first-line title it starts with.
pub(crate) fn schedule_ticket_title(tx: &Tx<'_>, ticket: TicketId, source: EventId, placeholder: &str) -> Result<(), StorageError> {
    let job = format!("ticket-title-{}", aurelius_domain::JobId::new());
    let payload = serde_json::json!({ "ticket_id": ticket, "placeholder": placeholder });
    events::insert_job(tx, &job, TICKET_TITLE_JOB_KIND, source, &job, &payload)
}

impl CommandService {
    /// The input for a ticket's title; `None` when the ticket is gone, has no
    /// description, or has nobody to run it (no owner and no dispatcher). It runs
    /// as its owner, else as the project's dispatcher, on the ticket's login.
    pub fn ticket_title_input(&self, ticket: TicketId) -> Result<Option<TicketTitleInput>, AppError> {
        let st = self.store.clone();
        let found = self
            .store
            .read(|c| {
                let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let Some(view) = store::view(c, ticket, &read)? else { return Ok(None) };
                let runner = match view.owner_employee_id {
                    Some(owner) => Some(owner),
                    None => company_store::dispatcher_for(c, view.project_id)?,
                };
                let description: String = view.description.as_deref().unwrap_or_default().trim().chars().take(SESSION_TITLE_INPUT_MAX_CHARS).collect();
                Ok(Some((view.project_name, description, runner, store::login_user(c, ticket)?)))
            })
            .map_err(storage_error)?;
        let Some((project_name, description, Some(runner), login)) = found else { return Ok(None) };
        if description.is_empty() {
            return Ok(None);
        }
        let mut config = self.resolve_for_employee(runner, aurelius_domain::connection::Purpose::Memory)?;
        config.login_user = login;
        Ok(Some(TicketTitleInput { ticket_id: ticket, project_name, description, config }))
    }

    /// Put a ticket's made title on it and close its job, fenced by the job's lease.
    /// The answer goes through the vault redactor and `session_title`. Nothing changes
    /// when the answer is unusable, the ticket is on Done, or its title is no longer
    /// `placeholder` (someone renamed it). Open boards hear `ticket.updated { changed: ["title"] }`.
    pub fn record_ticket_title(&self, job_id: &str, epoch: i64, ticket: TicketId, placeholder: &str, answer: &str) -> Result<Vec<EventNotice>, AppError> {
        let title = session_title(&aurelius_runtime::redact::string(answer.to_owned()))
            .and_then(|t| aurelius_domain::ticket::valid_title(&t).ok())
            .filter(|t| t != placeholder);
        let (job, placeholder) = (job_id.to_owned(), placeholder.to_owned());
        self.store
            .write(move |tx| {
                if !queries::holds_lease(tx, &job, epoch)? {
                    return Ok(Vec::new());
                }
                queries::complete_job(tx, &job, epoch)?;
                let Some(title) = title else { return Ok(Vec::new()) };
                let Some(state) = store::state(tx, ticket)? else { return Ok(Vec::new()) };
                let current: Option<String> = tx.query_row("SELECT title FROM tickets WHERE id = ?1", [ticket.to_string()], |r| r.get(0)).optional()?;
                if state.column == ColumnKind::Done || current.as_deref() != Some(placeholder.as_str()) {
                    return Ok(Vec::new());
                }
                store::update_fields(tx, ticket, &store::TicketFields { title: Some(&title), ..Default::default() })?;
                let notice = crate::task::ticket_line::ticket_event(tx, ticket, state.project_id, EventKind::TicketUpdated, None, serde_json::json!({ "changed": ["title"] }))?;
                Ok(vec![notice])
            })
            .map_err(storage_error)
    }

    /// The title could not be made (quota, a failed run, a deleted ticket): the job
    /// ends and the first-line title stays. `why` only goes to the server's log.
    pub fn ticket_title_failed(&self, job_id: &str, epoch: i64) -> Result<(), AppError> {
        let job = job_id.to_owned();
        self.store.write(move |tx| queries::kill_job(tx, &job, epoch, "title_failed").map(|_| ())).map_err(storage_error)
    }
}

#[cfg(test)]
#[path = "title_tests.rs"]
mod tests;
