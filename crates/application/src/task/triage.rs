//! The dispatcher of the board (impact analysis §3.7; "triage", since
//! `daemon/src/dispatcher` is the run and job dispatcher). A ticket that is
//! started without an owner has a main task in Draft; its dispatcher (the
//! project's, else the company's: by default the employee named Orion) chooses
//! the owner in one short run, `TRIAGE_JOB_KIND`, with only the bridge's
//! `assign_ticket` and `recall` (no files, commands or web; `MAX_TURNS`).
//!
//! `assign_ticket` is `ticket.dispatched` by the dispatcher, the same way a
//! board member's choice goes (`ticket_line::dispatched`): the draft goes to
//! that employee and starts. The org chart does not apply: the dispatcher
//! chooses for the board (after that, the owner's delegations follow it again).
//!
//! No dispatcher (none set, archived or paused): the ticket stays in Todo and
//! says "choose an owner" (B5). A run that ends without a choice gets one more
//! (`ATTEMPTS`), told to choose with `assign_ticket`; then the same (RULE 08:
//! no loop). The runs count for the main task (`runs.task_id`, no conversation).
//! A dispatcher that cannot work (its run fails, or cannot even be prepared)
//! ends the same way after one more try, with `dispatcher_failed`.

use crate::task::ticket_line;
use crate::{CommandService, storage_error};
use aurelius_domain::{
    AppError, EmployeeId, EventId, EventKind, EventNotice, RunId, TaskId, TicketId,
    task::TaskStatus,
    ticket::{Actor, ColumnKind},
};
use aurelius_storage::{StorageError, Store, Tx, company as company_store, tickets as store};
use serde_json::json;

pub const TRIAGE_JOB_KIND: &str = "ticket.triage";
/// The dispatcher's budget per run.
pub const MAX_TURNS: u32 = 3;
/// The first run and one more when it chose nobody.
pub const ATTEMPTS: u32 = 2;
/// The bridge's tools of a dispatcher's run (full names, allowed without asking).
pub const TOOLS: [&str; 2] = ["mcp__aurelius__assign_ticket", "mcp__aurelius__recall"];

/// What the dispatcher is told.
const GUIDANCE: &str = "You are the dispatcher of the board: a board member started the ticket below without an owner. Choose the one colleague who should own it and call assign_ticket with their name and a one-sentence reason; their main task starts at once. Choose by role, focus and load; recall helps with earlier decisions (who did this before). Do not do the work yourself and do not plan it. After assign_ticket, answer in one short sentence.";
const RETRY: &str = "You ended without choosing. Choose with assign_ticket now: one colleague from the list, with a reason.";

/// The ticket waits in Todo for a board member to choose (B5): no dispatcher, or it chose nobody.
fn needs_owner(tx: &Tx<'_>, ticket: TicketId, why: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(state) = store::state(tx, ticket)? else { return Ok(vec![]) };
    Ok(vec![ticket_line::ticket_event(tx, ticket, state.project_id, EventKind::TicketUpdated, None, json!({ "changed": [], "needs": "owner", "reason": why }))?])
}

/// The ticket is in Todo with its main task a draft: the dispatcher's job, or "choose an owner" (B5).
pub(crate) fn request(tx: &Tx<'_>, ticket: TicketId, source: EventId) -> Result<Vec<EventNotice>, StorageError> {
    let Some(state) = store::state(tx, ticket)? else { return Ok(vec![]) };
    let Some(root) = state.root_task_id else { return Ok(vec![]) };
    if company_store::dispatcher_for(tx, state.project_id)?.is_none() {
        return needs_owner(tx, ticket, "no_dispatcher");
    }
    let job = format!("triage:{root}");
    aurelius_storage::events::insert_job(tx, &job, TRIAGE_JOB_KIND, source, &job, &json!({ "ticket_id": ticket, "task_id": root }))?;
    Ok(vec![])
}

/// One run of the dispatcher, as the daemon starts it.
#[derive(Debug, Clone)]
pub struct TriageRun {
    pub run_id: RunId,
    pub dispatcher: EmployeeId,
    pub config: crate::run::connections::ResolvedRunConfig,
    pub system: String,
    pub prompt: String,
}

/// The main task still waits for the dispatcher: Todo, a draft, no owner.
fn still_waiting(tx: &Tx<'_>, ticket: TicketId, root: TaskId) -> Result<Option<store::TicketState>, StorageError> {
    let Some(state) = store::state(tx, ticket)? else { return Ok(None) };
    let waits = !state.archived && state.column == ColumnKind::Todo && state.root_task_id == Some(root) && state.root_status == Some(TaskStatus::Draft) && state.owner.is_none();
    Ok(waits.then_some(state))
}

/// Earlier runs of the dispatcher for this main task.
fn runs_so_far(conn: &rusqlite::Connection, root: TaskId) -> Result<u32, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM runs WHERE task_id = ?1 AND conversation_id IS NULL", [root.to_string()], |r| r.get::<_, i64>(0))? as u32)
}

/// Each active colleague with its role, focus and the number of open tasks.
fn candidates(tx: &Tx<'_>, st: &Store, dispatcher: EmployeeId) -> Result<String, StorageError> {
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let colleagues = crate::task::tasks::colleagues(tx, &read)?;
    let mut lines = Vec::new();
    for c in &colleagues {
        let open: i64 = tx.query_row(
            "SELECT COUNT(*) FROM tasks WHERE employee_id = ?1 AND status IN ('queued','running','waiting','review')",
            [c.id.to_string()],
            |r| r.get(0),
        )?;
        let you = if c.id == dispatcher { " (you)" } else { "" };
        let focus = if c.focus.is_empty() { String::new() } else { format!(": {}", c.focus) };
        lines.push(format!("- {}{you} ({}){focus} [open tasks: {open}]", c.name, c.role));
    }
    Ok(lines.join("\n"))
}

impl CommandService {
    /// The next run of the dispatcher for `root`, made now (its run row counts
    /// for the main task); none when there is nothing to choose any more, no
    /// dispatcher, or its runs are used up (the ticket then says "choose an owner").
    pub fn triage_run(&self, ticket: TicketId, root: TaskId) -> Result<(Option<TriageRun>, Vec<EventNotice>), AppError> {
        let st = self.store.clone();
        let prepared = self
            .store
            .write(move |tx| {
                let Some(state) = still_waiting(tx, ticket, root)? else { return Ok((None, vec![])) };
                let Some(dispatcher) = company_store::dispatcher_for(tx, state.project_id)? else { return Ok((None, needs_owner(tx, ticket, "no_dispatcher")?)) };
                let earlier = runs_so_far(tx, root)?;
                if earlier >= ATTEMPTS {
                    return Ok((None, needs_owner(tx, ticket, "dispatcher_did_not_choose")?));
                }
                let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let Some(view) = store::view(tx, ticket, &read)? else { return Ok((None, vec![])) };
                let identity = crate::task::tasks::employee_identity(tx, dispatcher, &read)?.map(|(_, _, system, _)| system).unwrap_or_default();
                let system = format!("{identity}\n\n{GUIDANCE}");
                let description = view.description.as_deref().map(str::trim).filter(|d| !d.is_empty()).unwrap_or("(no description)");
                let mut prompt = format!(
                    "Ticket #{} in project {}: {}\nPriority: {}\n\n{description}\n\nColleagues:\n{}",
                    view.number,
                    view.project_name,
                    view.title,
                    view.priority.as_str(),
                    candidates(tx, &st, dispatcher)?
                );
                if earlier > 0 {
                    prompt.push_str(&format!("\n\n{RETRY}"));
                }
                Ok((Some((dispatcher, system, prompt)), vec![]))
            })
            .map_err(storage_error)?;
        let (Some((dispatcher, system, prompt)), notices) = prepared else { return Ok((None, prepared.1)) };
        let config = self.resolve_for_employee(dispatcher, aurelius_domain::connection::Purpose::Employee)?;
        let run_id = RunId::new();
        let resolved = serde_json::to_string(&config).map_err(|e| AppError::new(aurelius_domain::ErrorCode::Internal, e.to_string()))?;
        let (kind, model) = (config.runtime_kind().to_owned(), config.model.clone().unwrap_or_default());
        let connection = config.connection_id.map(|c| c.to_string());
        self.store
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO runs(id, task_id, employee_id, attempt, runtime_kind, connection_id, model_id, resolved_config_json, status, started_at_ms)
                     VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, 'running', ?8)",
                    rusqlite::params![run_id.to_string(), root.to_string(), dispatcher.to_string(), kind, connection, model, resolved, aurelius_storage::now_ms()],
                )?;
                Ok(())
            })
            .map_err(storage_error)?;
        Ok((Some(TriageRun { run_id, dispatcher, config, system, prompt }), notices))
    }

    /// A run of the dispatcher ended (`failed`: why). Chosen: done. Not chosen:
    /// one more run, then "choose an owner". Returns whether the job goes again.
    pub fn triage_ended(&self, ticket: TicketId, root: TaskId, run: RunId, failed: Option<String>) -> Result<(bool, Vec<EventNotice>), AppError> {
        self.store
            .write(move |tx| {
                tx.execute(
                    "UPDATE runs SET status = ?2, error_code = ?3, ended_at_ms = ?4 WHERE id = ?1",
                    rusqlite::params![run.to_string(), if failed.is_some() { "failed" } else { "succeeded" }, failed, aurelius_storage::now_ms()],
                )?;
                if still_waiting(tx, ticket, root)?.is_none() {
                    return Ok((false, vec![]));
                }
                if runs_so_far(tx, root)? < ATTEMPTS {
                    return Ok((true, vec![]));
                }
                let why = if failed.is_some() { "dispatcher_failed" } else { "dispatcher_did_not_choose" };
                Ok((false, needs_owner(tx, ticket, why)?))
            })
            .map_err(storage_error)
    }

    /// The dispatcher's job was claimed (again): a run of an earlier claim that
    /// never ended (the daemon stopped during it) ends now, as interrupted; it
    /// still counts as one of its runs.
    pub fn triage_claimed(&self, root: TaskId) -> Result<(), AppError> {
        self.store
            .write(move |tx| {
                tx.execute(
                    "UPDATE runs SET status = 'failed', error_code = 'interrupted', ended_at_ms = ?2
                     WHERE task_id = ?1 AND conversation_id IS NULL AND status IN ('created','starting','running','waiting','cancelling')",
                    rusqlite::params![root.to_string(), aurelius_storage::now_ms()],
                )?;
                Ok(())
            })
            .map_err(storage_error)
    }

    /// The dispatcher's job gives up (`why`, e.g. `dispatcher_failed`): a ticket
    /// that still waits for it says "choose an owner" (B5).
    pub fn triage_gave_up(&self, ticket: TicketId, root: TaskId, why: &'static str) -> Result<Vec<EventNotice>, AppError> {
        self.store
            .write(move |tx| {
                if still_waiting(tx, ticket, root)?.is_none() {
                    return Ok(vec![]);
                }
                needs_owner(tx, ticket, why)
            })
            .map_err(storage_error)
    }
}

/// The bridge's `assign_ticket` from a dispatcher's run: the ticket's owner, and
/// its draft main task goes to that employee (`ticket.dispatched` by the dispatcher).
pub(crate) fn assign(tx: &Tx<'_>, st: &Store, run: RunId, employee: &str, reason: &str) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let refuse = |why: &str| Ok((false, why.to_string(), vec![]));
    let row: Option<(Option<String>, Option<String>, Option<String>)> = tx
        .query_row("SELECT task_id, employee_id, conversation_id FROM runs WHERE id = ?1", [run.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .ok();
    let Some((Some(root), Some(dispatcher), None)) = row else { return refuse("Only the dispatcher of a ticket can assign it.") };
    let (Ok(root), Ok(dispatcher)) = (TaskId::parse(&root), EmployeeId::parse(&dispatcher)) else { return refuse("Only the dispatcher of a ticket can assign it.") };
    let Some((ticket, _, _)) = ticket_line::root_of(tx, root)? else { return refuse("The ticket no longer exists.") };
    let Some(state) = still_waiting(tx, ticket, root)? else { return refuse("This ticket has an owner already (or was moved); nothing to do.") };
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
    store::set_owner(tx, ticket, Some(chosen.id))?;
    let by = Actor::Employee { employee_id: dispatcher };
    let mut notices = vec![ticket_line::ticket_event(
        tx,
        ticket,
        state.project_id,
        EventKind::TicketDispatched,
        Some(by),
        json!({ "employee_id": chosen.id, "reason": if reason.is_empty() { serde_json::Value::Null } else { json!(reason) } }),
    )?];
    notices.extend(ticket_line::dispatched(tx, ticket, chosen.id)?);
    Ok((true, format!("Assigned the ticket to {}; their main task starts now. Answer in one sentence.", chosen.name), notices))
}
