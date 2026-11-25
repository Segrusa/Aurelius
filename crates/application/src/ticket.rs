//! Ticket, board and company writes (protocol 4). Every one needs an active
//! board member on the connection (`require_board_member`): it is the author of
//! a new ticket (`created_by_user`) and the `by` of every column change.
//!
//! These writes change the ticket and its column, then hand over to the
//! engine (`task::ticket_line`) in the same transaction: a start makes the main
//! task, a chosen owner gets it, accept and send back act on it, a cancel ends
//! it. The engine finds the card already where the board member put it.

use crate::{CommandContext, Committed, CommandService, TxResult, domain_error, notice, storage_error};
use aurelius_domain::{
    ActorKind, AppError, Command, CommandEnvelope, CommandResult, DomainError, EmployeeId, EventKind, EventNotice, ProjectId, TicketId, UserId,
    company::{UpdateCompany, UpdateProject},
    employee::LifecycleStatus,
    project::ProjectStatus,
    ticket::{self, Actor, ColumnKind, MoveEffect, TicketKind},
};
use aurelius_storage::{
    Tx as Transaction, StorageError, blobs, company as company_store,
    events::{self, NewEvent},
    followers::FollowReason,
    queries, tasks as task_store, tickets as store, users,
};
use crate::task::{plan_review, ticket_line};
use crate::ticket_follow;
use aurelius_domain::views::system_action;
use serde_json::{Value, json};

pub(crate) mod agent;
pub mod quick;
pub mod title;

/// Why a paused or archived employee cannot be the tester: nobody would run the tests.
pub(crate) const TESTER_NOT_ACTIVE: &str = "The tester must be an active employee.";

/// Why nothing about a ticket on Done changes.
const TICKET_CLOSED: &str = "A ticket in Done is closed; it can no longer change.";

/// Why a disabled board member cannot become the Responsible (US-A1).
pub(crate) const RESPONSIBLE_DISABLED: &str = "A disabled board member cannot be made responsible.";

/// Why an unknown board member cannot become the Responsible.
pub(crate) const RESPONSIBLE_UNKNOWN: &str = "This board member does not exist.";

/// The board member of the connection, or `board_member_required`.
pub(crate) fn require_board_member(ctx: &CommandContext) -> Result<UserId, AppError> {
    ctx.user_id.ok_or_else(|| domain_error(DomainError::BoardMemberRequired))
}

/// Who and what wrote, owned so it can go into the transaction.
#[derive(Clone)]
struct Base {
    command_id: String,
    meta: Value,
    actor_kind: ActorKind,
    actor: String,
    /// The board member of the write, as a ticket's lines name them.
    by: Option<Actor>,
}

impl Base {
    fn of(ctx: &CommandContext, envelope: &CommandEnvelope) -> Self {
        let by = ctx.user_id.map(|user_id| Actor::BoardMember { user_id });
        Self { command_id: envelope.command_id.to_string(), meta: ctx.metadata(), actor_kind: ActorKind::User, actor: ctx.principal_id.to_string(), by }
    }

    /// An employee's board action during `run`: one fresh correlation per call, the employee as actor and as `by`.
    fn of_employee(employee: EmployeeId, run: aurelius_domain::RunId) -> Self {
        Self {
            command_id: format!("board_tool:{}", aurelius_domain::EventId::new()),
            meta: json!({ "run_id": run }),
            actor_kind: ActorKind::Employee,
            actor: employee.to_string(),
            by: Some(Actor::Employee { employee_id: employee }),
        }
    }
}

/// The events of one write about one aggregate: each gets the next version.
struct Events {
    aggregate_type: &'static str,
    aggregate_id: String,
    project: Option<ProjectId>,
    version: u64,
    command_id: String,
    meta: Value,
    actor_kind: ActorKind,
    actor: String,
    by: Option<Actor>,
    notices: Vec<EventNotice>,
}

impl Events {
    fn new(base: &Base, aggregate_type: &'static str, aggregate_id: String, project: Option<ProjectId>, version: u64) -> Self {
        Self {
            aggregate_type,
            aggregate_id,
            project,
            version,
            command_id: base.command_id.clone(),
            meta: base.meta.clone(),
            actor_kind: base.actor_kind,
            actor: base.actor.clone(),
            by: base.by,
            notices: Vec::new(),
        }
    }

    fn push(&mut self, tx: &Transaction<'_>, kind: EventKind, extra: Value) -> Result<(), StorageError> {
        self.push_id(tx, kind, extra).map(drop)
    }

    /// `push`, with the id of the event (the source of a job the write queues).
    fn push_id(&mut self, tx: &Transaction<'_>, kind: EventKind, extra: Value) -> Result<aurelius_domain::EventId, StorageError> {
        self.version += 1;
        let mut metadata = self.meta.clone();
        if let (Some(meta), Value::Object(extra)) = (metadata.as_object_mut(), extra) {
            meta.extend(extra);
        }
        let appended = events::append(
            tx,
            NewEvent {
                aggregate_type: self.aggregate_type,
                aggregate_id: self.aggregate_id.clone(),
                aggregate_version: self.version,
                event_type: kind.as_str(),
                project_id: self.project,
                actor_kind: self.actor_kind,
                actor_id: Some(self.actor.clone()),
                correlation_id: self.command_id.clone(),
                causation_id: None,
                payload_blob_id: None,
                metadata,
                dedupe_key: format!("cmd:{}:{}", self.command_id, self.notices.len()),
            },
        )?;
        self.notices.push(notice(appended.seq, kind.as_str(), self.aggregate_id.clone(), self.project));
        Ok(appended.id)
    }

    /// The system lines of this write in the ticket's discussion (MU2-03), with its events.
    fn line(&mut self, notices: Vec<EventNotice>) {
        self.notices.extend(notices);
    }

    /// `user` follows the ticket now, in this write's transaction.
    fn follow(&mut self, tx: &Transaction<'_>, ticket: TicketId, project: ProjectId, user: UserId, reason: FollowReason) -> Result<(), StorageError> {
        let cause = ticket_follow::Cause { actor: self.actor_kind, actor_id: Some(self.actor.clone()), correlation: &self.command_id, meta: self.meta.clone() };
        self.notices.extend(ticket_follow::follow(tx, ticket, project, user, reason, &cause)?);
        Ok(())
    }

    /// Who is responsible changed: `ticket.updated`, the new Responsible follows, and
    /// the line in the discussion. `by` is the board member of the write.
    fn responsible(&mut self, tx: &Transaction<'_>, ticket: TicketId, project: ProjectId, before: Option<UserId>, after: Option<UserId>, by: Actor) -> Result<(), StorageError> {
        self.push(tx, EventKind::TicketUpdated, json!({ "changed": ["responsible"], "by": by, "responsible_user_id": after, "before": before }))?;
        if let Some(user) = after {
            self.follow(tx, ticket, project, user, FollowReason::Responsible)?;
        }
        self.line(ticket_line::responsible_line(tx, ticket, by, before, after)?);
        Ok(())
    }

    /// `ticket.moved` from → to by `by`.
    fn moved(&mut self, tx: &Transaction<'_>, from: ColumnKind, to: ColumnKind, by: Actor) -> Result<(), StorageError> {
        self.push(tx, EventKind::TicketMoved, json!({ "from": from, "to": to, "by": by }))
    }
}

fn rejected<T>(err: DomainError) -> TxResult<T> {
    Ok(Err(domain_error(err)))
}

/// An employee that can own or dispatch tickets: exists and is not archived.
fn check_employee(tx: &Transaction<'_>, id: EmployeeId) -> Result<Result<(), AppError>, StorageError> {
    Ok(match company_store::employee_status(tx, id)?.as_deref().and_then(LifecycleStatus::parse) {
        None => Err(domain_error(DomainError::EmployeeNotFound(id))),
        Some(LifecycleStatus::Archived) => Err(domain_error(DomainError::EmployeeArchived)),
        Some(_) => Ok(()),
    })
}

/// A board member who can become the Responsible: exists and is not disabled.
fn check_responsible(tx: &Transaction<'_>, user: UserId) -> Result<Result<(), AppError>, StorageError> {
    Ok(match users::get(tx, &user.to_string())? {
        None => Err(AppError::new(aurelius_domain::ErrorCode::NotFound, RESPONSIBLE_UNKNOWN)),
        Some(row) if row.disabled() => Err(AppError::new(aurelius_domain::ErrorCode::Validation, RESPONSIBLE_DISABLED)),
        Some(_) => Ok(()),
    })
}

/// The ticket as it is now, checked against the expected version; archived tickets do not change.
fn load(tx: &Transaction<'_>, id: TicketId, expected: Option<u64>) -> Result<Result<store::TicketState, AppError>, StorageError> {
    let Some(state) = store::state(tx, id)? else { return Ok(Err(domain_error(DomainError::TicketNotFound(id)))) };
    if let Some(expected) = expected {
        if expected != state.version {
            return Ok(Err(domain_error(DomainError::VersionConflict { expected, actual: state.version })));
        }
    }
    if state.archived {
        return Ok(Err(domain_error(DomainError::Ticket("This ticket is archived; it can no longer change.".into()))));
    }
    Ok(Ok(state))
}

macro_rules! check {
    ($e:expr) => {
        match $e {
            Ok(value) => value,
            Err(err) => return Ok(Err(err)),
        }
    };
}

macro_rules! rule {
    ($e:expr) => {
        match $e {
            Ok(value) => value,
            Err(err) => return rejected(err),
        }
    };
}

impl CommandService {
    /// Every ticket, board and company write.
    pub(crate) fn ticket_command(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String) -> Result<Committed, AppError> {
        let user = require_board_member(ctx)?;
        let by = Actor::BoardMember { user_id: user };
        let expected = envelope.expected_version;
        let command_id = envelope.command_id;
        let base = Base::of(ctx, envelope);
        let st = self.store().clone();
        match envelope.command.clone() {
            Command::CreateTicket(cmd) => self.create_ticket(base, envelope, hash, cmd, user),
            Command::UpdateTicket(cmd) => self.update_ticket(base, envelope, hash, cmd),
            Command::MoveTicket(cmd) => {
                self.commit(command_id, hash, move |tx| {
                    let state = check!(load(tx, cmd.ticket_id, expected)?);
                    let Some(target) = company_store::column(tx, cmd.column_id)? else { return rejected(DomainError::BoardColumnNotFound(cmd.column_id)) };
                    let effect = rule!(ticket::decide_move(state.column, target.kind));
                    if cmd.before_ticket_id == Some(state.id) {
                        // Dropped on its own place: nothing moves.
                        if target.id != state.column_id {
                            return rejected(DomainError::Ticket("A card cannot be put above itself in another column.".into()));
                        }
                        return Ok(Ok((CommandResult::TicketChanged { ticket_id: state.id, column: state.column, version: state.version }, Vec::new())));
                    }
                    let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
                    match store::move_to(tx, state.id, target.id, cmd.before_ticket_id) {
                        Err(StorageError::Rejected(why)) if why == store::NOT_IN_COLUMN => {
                            return rejected(DomainError::Ticket("The card to put it above is not in that column; refresh the board.".into()));
                        }
                        other => other?,
                    }
                    e.moved(tx, state.column, target.kind, by)?;
                    match effect {
                        MoveEffect::Reorder => {}
                        MoveEffect::Start => {
                            store::mark_started(tx, state.id)?;
                            e.push(tx, EventKind::TicketStarted, json!({ "by": by }))?;
                        }
                        MoveEffect::Cancel => {
                            let source = e.push_id(tx, EventKind::TicketCancelled, json!({ "by": by }))?;
                            quick::request_cleanup(tx, &state, source, &command_id.to_string())?;
                        }
                    }
                    if target.kind != state.column {
                        let what = match effect {
                            MoveEffect::Start => Some("started the ticket"),
                            MoveEffect::Cancel => Some("cancelled the ticket"),
                            MoveEffect::Reorder => None,
                        };
                        e.line(ticket_line::column_line(tx, state.id, Some(by), state.column, target.kind, what)?);
                    }
                    finish_then(tx, state.id, e, |tx| match effect {
                        MoveEffect::Reorder => Ok(vec![]),
                        MoveEffect::Start => ticket_line::started(tx, &st, state.id, user),
                        MoveEffect::Cancel => ticket_line::cancel(tx, &st, state.id, &command_id.to_string()),
                    })
                })
            }
            Command::StartTicket(cmd) => self.commit(command_id, hash, move |tx| {
                let state = check!(load(tx, cmd.ticket_id, expected)?);
                rule!(ticket::check_start(state.column, state.root_status));
                if let Some(employee) = cmd.employee_id {
                    check!(check_employee(tx, employee)?);
                }
                let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
                start_card(tx, &mut e, &state, by, cmd.employee_id)?;
                // A new main task (also after a failed one, B6), for the owner if there is one.
                finish_then(tx, state.id, e, |tx| ticket_line::started(tx, &st, state.id, user))
            }),
            Command::AssignTicket(cmd) => {
                let reason = ticket::valid_note("reason", cmd.reason.as_deref(), ticket::REASON_MAX).map_err(domain_error)?;
                self.commit(command_id, hash, move |tx| {
                    let state = check!(load(tx, cmd.ticket_id, expected)?);
                    rule!(ticket::check_assign(state.column, state.root_status));
                    let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
                    match cmd.employee_id {
                        Some(employee) => {
                            check!(check_employee(tx, employee)?);
                            store::set_owner(tx, state.id, Some(employee))?;
                            e.push(tx, EventKind::TicketDispatched, json!({ "employee_id": employee, "by": by, "reason": reason }))?;
                            e.line(ticket_line::owner_line(tx, state.id, Some(by), employee, reason.as_deref())?);
                            // In Todo the draft main task goes to that employee now.
                            return finish_then(tx, state.id, e, |tx| ticket_line::dispatched(tx, state.id, employee));
                        }
                        // Back to "the dispatcher chooses": nobody was chosen, so no `ticket.dispatched`.
                        None => {
                            rule!(ticket::check_unassign(state.column));
                            store::set_owner(tx, state.id, None)?;
                            e.push(tx, EventKind::TicketUpdated, json!({ "changed": ["owner"], "by": by, "reason": reason }))?;
                        }
                    }
                    finish(tx, state.id, state.column, e)
                })
            }
            // No version check: a handover is always possible, also while someone edits.
            Command::SetTicketResponsible(cmd) => self.commit(command_id, hash, move |tx| {
                let state = check!(load(tx, cmd.ticket_id, None)?);
                if state.column == ColumnKind::Done {
                    return rejected(DomainError::Ticket(TICKET_CLOSED.into()));
                }
                let before = store::responsible(tx, state.id)?.map(|r| r.user_id);
                let after = cmd.responsible_user_id;
                // Before the check below: a disabled Responsible stays while nobody changes it.
                if before == after {
                    return Ok(Ok((CommandResult::TicketChanged { ticket_id: state.id, column: state.column, version: state.version }, Vec::new())));
                }
                if let Some(user) = after {
                    check!(check_responsible(tx, user)?);
                }
                store::set_responsible(tx, state.id, after)?;
                let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
                e.responsible(tx, state.id, state.project_id, before, after, by)?;
                finish(tx, state.id, state.column, e)
            }),
            Command::ApprovePlan(cmd) => {
                let feedback = ticket::valid_note("feedback", cmd.feedback.as_deref(), ticket::FEEDBACK_MAX).map_err(domain_error)?;
                self.commit(command_id, hash, move |tx| {
                    let state = check!(load(tx, cmd.ticket_id, expected)?);
                    rule!(ticket::check_in(state.column, ColumnKind::PlanReview, "Approving a plan"));
                    if !plan_review::waits_for_review(tx, state.id)? {
                        return rejected(DomainError::Ticket("The plan is not waiting for a decision (any more).".into()));
                    }
                    let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
                    to_column(tx, state.id, ColumnKind::Doing)?;
                    store::set_feedback(tx, state.id, feedback.as_deref())?;
                    e.moved(tx, state.column, ColumnKind::Doing, by)?;
                    e.push(tx, EventKind::TicketPlanApproved, json!({ "decision": cmd.decision, "feedback": feedback.is_some(), "by": by }))?;
                    e.line(ticket_line::column_line_with(tx, state.id, Some(by), state.column, ColumnKind::Doing, Some("approved the plan"), Some(system_action::APPROVED))?);
                    finish_then(tx, state.id, e, |tx| {
                        let mut notices = match &feedback {
                            Some(text) => feedback_note(tx, &st, state.id, user, &base, text)?,
                            None => Vec::new(),
                        };
                        notices.extend(plan_review::approve(tx, &st, state.id, user, cmd.decision, feedback.as_deref())?);
                        Ok(notices)
                    })
                })
            }
            Command::RevisePlan(cmd) => {
                let feedback = ticket::valid_feedback(&cmd.feedback).map_err(domain_error)?;
                self.commit(command_id, hash, move |tx| {
                    let state = check!(load(tx, cmd.ticket_id, expected)?);
                    rule!(ticket::check_in(state.column, ColumnKind::PlanReview, "Asking for a new plan"));
                    if !plan_review::waits_for_review(tx, state.id)? {
                        return rejected(DomainError::Ticket("The plan is not waiting for a decision (any more).".into()));
                    }
                    let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
                    to_column(tx, state.id, ColumnKind::Doing)?;
                    store::set_feedback(tx, state.id, Some(&feedback))?;
                    e.moved(tx, state.column, ColumnKind::Doing, by)?;
                    e.push(tx, EventKind::TicketPlanRevised, json!({ "by": by }))?;
                    e.line(ticket_line::column_line_with(tx, state.id, Some(by), state.column, ColumnKind::Doing, Some("sent the plan back"), Some(system_action::ADJUSTED))?);
                    // B2: the feedback goes to the main task's conversation; Plan mode stays.
                    finish_then(tx, state.id, e, |tx| {
                        let mut notices = feedback_note(tx, &st, state.id, user, &base, &feedback)?;
                        notices.extend(plan_review::revise(tx, &st, state.id, user, &feedback)?);
                        Ok(notices)
                    })
                })
            }
            Command::AcceptTicket(cmd) => {
                let note = ticket::valid_note("note", cmd.note.as_deref(), ticket::FEEDBACK_MAX).map_err(domain_error)?;
                self.commit(command_id, hash, move |tx| {
                    let state = check!(load(tx, cmd.ticket_id, expected)?);
                    rule!(ticket::check_in(state.column, ColumnKind::Review, "Accept"));
                    if state.root_status != Some(aurelius_domain::task::TaskStatus::Review) {
                        return rejected(DomainError::Ticket("The main task is working again; accept once it reported.".into()));
                    }
                    // B6: Done only when nothing linked to the ticket still works.
                    let working = store::working_tasks(tx, state.id, state.root_task_id)?;
                    if working > 0 {
                        return rejected(DomainError::Ticket(format!("{working} linked task(s) still work; wait for them or cancel them first.")));
                    }
                    // Nor while a run in the main task's conversation goes (a question in Review, §3.3).
                    if let Some(root) = state.root_task_id {
                        if crate::task::task_runs::live_run(tx, root, None)? {
                            return rejected(DomainError::Ticket("The main task's conversation is still running; accept once it answered.".into()));
                        }
                    }
                    let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
                    // A quick task (ST-05): the daemon merges its branch first; only a merge that
                    // worked puts the card on Done (`quick::quick_merge_finished`).
                    if state.kind == TicketKind::Quick {
                        let source = e.push_id(tx, EventKind::TicketUpdated, json!({ "changed": ["merge"], "by": by, "merge": "requested", "note": note }))?;
                        e.line(check!(quick::request_merge(tx, &state, by, user, source, &command_id.to_string())?));
                        return finish(tx, state.id, state.column, e);
                    }
                    to_column(tx, state.id, ColumnKind::Done)?;
                    store::mark_done(tx, state.id)?;
                    e.moved(tx, state.column, ColumnKind::Done, by)?;
                    e.push(tx, EventKind::TicketAccepted, json!({ "note": note, "by": by }))?;
                    e.line(done_line(tx, state.id, by)?);
                    finish_then(tx, state.id, e, |tx| ticket_line::accept(tx, &st, state.id, user))
                })
            }
            Command::SendBackTicket(cmd) => {
                let feedback = ticket::valid_feedback(&cmd.feedback).map_err(domain_error)?;
                self.commit(command_id, hash, move |tx| {
                    let state = check!(load(tx, cmd.ticket_id, expected)?);
                    rule!(ticket::check_in(state.column, ColumnKind::Review, "Sending back"));
                    if state.root_status != Some(aurelius_domain::task::TaskStatus::Review) {
                        return rejected(DomainError::Ticket("The main task is working again; send it back once it reported.".into()));
                    }
                    let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
                    to_column(tx, state.id, ColumnKind::Doing)?;
                    store::set_feedback(tx, state.id, Some(&feedback))?;
                    e.moved(tx, state.column, ColumnKind::Doing, by)?;
                    e.push(tx, EventKind::TicketSentBack, json!({ "by": by }))?;
                    e.line(ticket_line::column_line_with(tx, state.id, Some(by), state.column, ColumnKind::Doing, Some("sent the ticket back for more work"), Some(system_action::SENT_BACK))?);
                    // B2: the feedback goes to the main task's conversation.
                    finish_then(tx, state.id, e, |tx| {
                        let mut notices = feedback_note(tx, &st, state.id, user, &base, &feedback)?;
                        notices.extend(ticket_line::send_back(tx, &st, state.id, user, &feedback)?);
                        Ok(notices)
                    })
                })
            }
            Command::CancelTicket(cmd) => self.commit(command_id, hash, move |tx| {
                let state = check!(load(tx, cmd.ticket_id, expected)?);
                rule!(ticket::check_cancel(state.column));
                let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
                to_column(tx, state.id, ColumnKind::Backlog)?;
                e.moved(tx, state.column, ColumnKind::Backlog, by)?;
                let source = e.push_id(tx, EventKind::TicketCancelled, json!({ "by": by }))?;
                // BR4: a quick task's worktree and branch go too (the client confirmed it).
                quick::request_cleanup(tx, &state, source, &command_id.to_string())?;
                e.line(ticket_line::column_line(tx, state.id, Some(by), state.column, ColumnKind::Backlog, Some("cancelled the ticket"))?);
                finish_then(tx, state.id, e, |tx| ticket_line::cancel(tx, &st, state.id, &command_id.to_string()))
            }),
            Command::ArchiveTicket(cmd) => self.commit(command_id, hash, move |tx| {
                let state = check!(load(tx, cmd.ticket_id, expected)?);
                rule!(ticket::check_archive(state.column));
                let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
                store::mark_archived(tx, state.id)?;
                e.push(tx, EventKind::TicketArchived, json!({ "by": by }))?;
                finish(tx, state.id, state.column, e)
            }),
            Command::UpdateBoardColumns(cmd) => self.commit(command_id, hash, move |tx| {
                let current = company_store::columns(tx)?;
                let actual = ticket::board_version(&current);
                if let Some(expected) = expected {
                    if expected != actual {
                        return rejected(DomainError::VersionConflict { expected, actual });
                    }
                }
                let columns = rule!(cmd.decide(&current));
                let company = company_store::get(tx)?;
                let version = aurelius_storage::conversations::next_version(tx, "board", &company.id)? - 1;
                let mut e = Events::new(&base, "board", company.id, None, version);
                company_store::update_columns(tx, &columns)?;
                let after = company_store::columns(tx)?;
                // Names and order left to right, so the audit shows what changed.
                let listed: Vec<Value> = after.iter().map(|c| json!({ "column_id": c.id, "kind": c.kind, "name": c.name, "position": c.position })).collect();
                e.push(tx, EventKind::BoardColumnsUpdated, json!({ "columns": listed, "by": by }))?;
                Ok(Ok((CommandResult::BoardColumnsUpdated { version: ticket::board_version(&after) }, e.notices)))
            }),
            Command::UpdateCompany(cmd) => self.update_company(base, envelope, hash, cmd, by),
            // "Run on my login" (B4, D3-3): only the board member's own login, never another's.
            Command::UseMyLogin(cmd) => self.commit(command_id, hash, move |tx| {
                let state = check!(load(tx, cmd.ticket_id, expected)?);
                let before = store::login_user(tx, state.id)?;
                if before == Some(user) {
                    return Ok(Ok((CommandResult::TicketChanged { ticket_id: state.id, column: state.column, version: state.version }, Vec::new())));
                }
                store::use_my_login(tx, state.id, user)?;
                let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
                e.push(tx, EventKind::TicketUpdated, json!({ "changed": ["login_user"], "by": by, "login_user": user, "before": before }))?;
                let (me, was) = (crate::conversation::discussion::actor_name(tx, Some(by))?, crate::run::login_wait::name_of(tx, before)?);
                let mut line = aurelius_domain::views::SystemLine::new(aurelius_domain::views::SystemLineKind::Delegated);
                (line.from, line.to, line.by) = (Some(was.clone()), Some(me.clone()), Some(me.clone()));
                e.line(crate::conversation::discussion::ticket_line(tx, state.id, line, &format!("{me} runs this ticket on their own Claude login now (it ran on {was}'s)."))?);
                finish_then(tx, state.id, e, |tx| crate::run::login_wait::take_over(tx, state.id, user))
            }),
            // The company's background jobs on the board member's own login, or on none (B6).
            Command::SetBackgroundLogin(cmd) => self.commit(command_id, hash, move |tx| {
                let current = company_store::background_login_user(tx)?;
                let choice = match (cmd.enabled, current) {
                    (true, _) => Some(user),
                    (false, Some(other)) if other != user => {
                        return rejected(DomainError::Ticket("Only the board member whose Claude login runs the background jobs can turn that off.".into()));
                    }
                    (false, _) => None,
                };
                let company = company_store::get(tx)?;
                if current == choice {
                    return Ok(Ok((CommandResult::CompanyUpdated { version: company.version }, Vec::new())));
                }
                let version = company_store::set_background_login_user(tx, choice)?;
                if let Some(old) = current {
                    crate::run::login_wait::background_login_left(tx, old)?;
                }
                let previous = aurelius_storage::conversations::next_version(tx, "company", &company.id)? - 1;
                let mut e = Events::new(&base, "company", company.id, None, previous);
                e.push(tx, EventKind::CompanyUpdated, json!({ "by": by, "changed": ["background_login"], "background_login": choice }))?;
                Ok(Ok((CommandResult::CompanyUpdated { version }, e.notices)))
            }),
            _ => Err(AppError::new(aurelius_domain::ErrorCode::Internal, "not a ticket command")),
        }
    }

    fn create_ticket(&self, base: Base, envelope: &CommandEnvelope, hash: String, mut cmd: ticket::CreateTicket, user: UserId) -> Result<Committed, AppError> {
        let kind = cmd.kind.unwrap_or_default();
        // Feature `ticket_made_title`: no title but a description, so the title is
        // made from it; until then the description's first line is the title.
        let made_title = cmd.title.trim().is_empty() && cmd.description.as_deref().is_some_and(|d| !d.trim().is_empty());
        if kind == TicketKind::Quick {
            crate::ticket::quick::prepare(self.store(), &mut cmd, user)?;
        }
        if made_title && cmd.title.trim().is_empty() {
            cmd.title = quick::title_from(cmd.description.as_deref().unwrap_or_default());
        }
        let title = ticket::valid_title(&cmd.title).map_err(domain_error)?;
        let description = ticket::valid_description(cmd.description.as_deref()).map_err(domain_error)?;
        let labels = ticket::valid_labels(&cmd.labels).map_err(domain_error)?;
        ticket::valid_deadline(cmd.deadline_ms).map_err(domain_error)?;
        let blobs = self.store().blobs();
        let description_blob = match &description {
            Some(text) => Some(blobs.put(text.as_bytes(), "text/markdown; charset=utf-8").map_err(storage_error)?),
            None => None,
        };
        // Made from an idea: the ticket gets its own copies of the idea's files too.
        let mut files = cmd.files.clone();
        if let Some(idea) = cmd.from_idea {
            let row = self.store().read(|c| aurelius_storage::ideas::load(c, idea)).map_err(storage_error)?.ok_or_else(|| domain_error(DomainError::IdeaNotFound(idea)))?;
            files.extend(crate::task::ideas::files_of(&row));
        }
        let attachments = self.attach_files(cmd.project_id, None, &files, None)?;
        let stored = attachments.clone();
        let command_id = envelope.command_id;
        let by = Actor::BoardMember { user_id: user };
        let st = self.store().clone();
        let committed = self.commit(envelope.command_id, hash, move |tx| {
            match queries::load_project(tx, cmd.project_id)? {
                None => return rejected(DomainError::ProjectNotFound(cmd.project_id)),
                Some(project) if project.status == ProjectStatus::Archived => return rejected(DomainError::ProjectArchived),
                Some(_) => {}
            }
            if let Some(owner) = cmd.owner_employee_id {
                check!(check_employee(tx, owner)?);
            }
            if let Some(responsible) = cmd.responsible_user_id {
                check!(check_responsible(tx, responsible)?);
            }
            // "Make ticket" from a chat message (MU2-10 AC1/AC3): it must exist, in a channel, a
            // ticket's discussion, a chat with an employee, the project assistant, a task or a
            // work session;
            // one that belongs to a project (its channel, a ticket's discussion, a task, its
            // assistant) only for a ticket of that same project.
            let source = match cmd.from_message {
                Some(message) => match crate::conversation::discussion::message_place(tx, message)? {
                    Some((_, _, Some(project))) if project != cmd.project_id => {
                        return rejected(DomainError::Ticket("The message belongs to another project: make the ticket in that project.".into()));
                    }
                    Some((conversation, label, _)) => Some((message, (conversation, label))),
                    None => return rejected(DomainError::Ticket("The message to make the ticket from does not exist (any more).".into())),
                },
                None => None,
            };
            if let Some(idea) = cmd.from_idea {
                check!(crate::task::ideas::check_for_ticket(tx, idea, cmd.project_id)?);
            }
            if let Some(blob) = &description_blob {
                blobs::register(tx, blob)?;
            }
            let mut e = Events::new(&base, "ticket", String::new(), Some(cmd.project_id), 0);
            let (id, number, created) = insert_ticket(
                tx,
                &mut e,
                &NewCard {
                    project: cmd.project_id,
                    title: &title,
                    description: description.as_deref(),
                    description_blob: description_blob.as_ref().map(|b| b.id.to_string()),
                    priority: cmd.priority,
                    user,
                    responsible: cmd.responsible_user_id,
                    owner: cmd.owner_employee_id,
                    run: &cmd.run,
                    labels: &labels,
                    deadline_ms: cmd.deadline_ms,
                    attachments: stored.as_ref(),
                    kind,
                },
            )?;
            if made_title {
                title::schedule_ticket_title(tx, id, created, &title)?;
            }
            // The discussion starts with a link to the message, and its chat gets "→ #N" (MU2-10 AC3).
            if let Some((message, (conversation, label))) = &source {
                e.line(crate::conversation::discussion::created_from(tx, id, number, user, *conversation, *message, label)?);
            }
            if kind == TicketKind::Quick {
                // ST-01 AC3: at once in Doing with its chosen employee; no Todo, no dispatcher.
                let doing = company_store::column_of_kind(tx, ColumnKind::Doing)?;
                store::move_to(tx, id, doing.id, None)?;
                store::mark_started(tx, id)?;
                e.moved(tx, ColumnKind::Backlog, ColumnKind::Doing, by)?;
                e.push(tx, EventKind::TicketStarted, json!({ "by": by, "kind": kind }))?;
                let owner = cmd.owner_employee_id.map(|o| crate::conversation::discussion::employee_name(tx, o)).transpose()?.unwrap_or_default();
                let what = format!("started the quick task with {owner} as owner, on the branch {}", ticket::quick_branch(number));
                e.line(ticket_line::column_line(tx, id, Some(by), ColumnKind::Backlog, ColumnKind::Doing, Some(&what))?);
            } else if cmd.start {
                let todo = company_store::column_of_kind(tx, ColumnKind::Todo)?;
                store::move_to(tx, id, todo.id, None)?;
                store::mark_started(tx, id)?;
                e.moved(tx, ColumnKind::Backlog, ColumnKind::Todo, by)?;
                e.push(tx, EventKind::TicketStarted, json!({ "by": by }))?;
                let what = match cmd.owner_employee_id {
                    Some(employee) => format!("started the ticket with {} as owner", crate::conversation::discussion::employee_name(tx, employee)?),
                    None => "started the ticket".to_owned(),
                };
                e.line(ticket_line::column_line(tx, id, Some(by), ColumnKind::Backlog, ColumnKind::Todo, Some(&what))?);
            }
            // Every choice of an owner is `ticket.dispatched`, as with Start and Assign.
            if let Some(employee) = cmd.owner_employee_id {
                e.push(tx, EventKind::TicketDispatched, json!({ "employee_id": employee, "by": by, "reason": Value::Null }))?;
            }
            store::touch(tx, id, e.version)?;
            let mut notices = e.notices;
            if let Some(idea) = cmd.from_idea {
                notices.push(crate::task::ideas::became_ticket(tx, idea, cmd.project_id, id, &title, command_id.to_string())?);
            }
            if cmd.start || kind == TicketKind::Quick {
                notices.extend(ticket_line::started(tx, &st, id, user)?);
            }
            // Its channel opens.
            notices.extend(crate::conversation::ticket_channel::opened(tx, id, user)?);
            Ok(Ok((CommandResult::TicketCreated { ticket_id: id, number }, notices)))
        });
        self.discard_unused(&attachments, committed.as_ref().map_or(true, |c| c.receipt.replayed));
        committed
    }

    fn update_ticket(&self, base: Base, envelope: &CommandEnvelope, hash: String, cmd: ticket::UpdateTicket) -> Result<Committed, AppError> {
        let title = cmd.title.as_deref().map(ticket::valid_title).transpose().map_err(domain_error)?;
        let description = match cmd.description.as_deref() {
            None => None,
            Some(text) => Some(ticket::valid_description(Some(text)).map_err(domain_error)?),
        };
        let labels = cmd.labels.as_deref().map(ticket::valid_labels).transpose().map_err(domain_error)?;
        let deadline = if cmd.clear_deadline { Some(None) } else { ticket::valid_deadline(cmd.deadline_ms).map_err(domain_error)?.map(Some) };
        let design_url = cmd.design_url.as_deref().map(|u| ticket::valid_artifact_url("design_url", u)).transpose().map_err(domain_error)?;
        if title.is_none() && description.is_none() && cmd.priority.is_none() && labels.is_none() && deadline.is_none() && cmd.run.is_none() && design_url.is_none() && cmd.add_files.as_deref().unwrap_or_default().is_empty() {
            return Err(domain_error(DomainError::Required { field: "a field to change".into() }));
        }
        let description_blob = match description.as_ref().and_then(|d| d.as_deref()) {
            Some(text) => Some(self.store().blobs().put(text.as_bytes(), "text/markdown; charset=utf-8").map_err(storage_error)?),
            None => None,
        };
        let (before, added) = self.add_ticket_files(&cmd)?;
        let new_files = added.clone();
        let expected = envelope.expected_version;
        let st = self.store().clone();
        let committed = self.commit(envelope.command_id, hash, move |tx| {
            let state = check!(load(tx, cmd.ticket_id, expected)?);
            if state.column == ColumnKind::Done {
                return rejected(DomainError::Ticket(TICKET_CLOSED.into()));
            }
            if cmd.run.is_some() && state.root_status == Some(aurelius_domain::task::TaskStatus::Running) {
                return rejected(DomainError::Ticket("Mode and model change only while the main task does not run.".into()));
            }
            // A quick task stays in Auto (ST-01 AC4).
            if state.kind == TicketKind::Quick && cmd.run.as_ref().is_some_and(|r| r.mode != Some(aurelius_domain::autonomy::AgentMode::Auto)) {
                return rejected(DomainError::Ticket(quick::PLAN_REFUSED.into()));
            }
            let mut files_added: Vec<String> = Vec::new();
            let mut told: Vec<EventNotice> = Vec::new();
            if let Some(new) = &new_files {
                // Counted and copied against what the ticket had before the transaction.
                if store::attachments_of(tx, state.id)? != before {
                    return rejected(DomainError::File("The ticket's files changed meanwhile; reload it and try again.".into()));
                }
                let mut all = before.clone().unwrap_or_else(|| task_store::TaskAttachments { folder: new.folder.clone(), files: Vec::new() });
                all.files.extend(new.files.iter().cloned());
                store::set_attachments(tx, state.id, Some(&all))?;
                // A main task that still works gets them too, and is told in its
                // conversation (`ticket_line::files_added`); a finished one keeps its own.
                if let (Some(root), Some(status)) = (state.root_task_id, state.root_status)
                    && !status.is_terminal()
                {
                    let mut task_files = task_store::attachments_of(tx, root)?;
                    match task_files.as_mut() {
                        Some(t) if t.folder == new.folder => t.files.extend(new.files.iter().cloned()),
                        // The main task copies the ticket's folder when it starts, so this does not
                        // happen; if it does, its row keeps its own files (their paths are relative to
                        // its folder), and the update in its conversation still names the new ones.
                        Some(_) => {}
                        None => task_files = Some(new.clone()),
                    }
                    task_store::set_attachments(tx, root, task_files.as_ref())?;
                    told = ticket_line::files_added(tx, &st, root, new)?;
                }
                files_added = new.files.iter().map(|f| f.name.clone()).collect();
            }
            if let Some(blob) = &description_blob {
                blobs::register(tx, blob)?;
            }
            let blob_id = description_blob.as_ref().map(|b| b.id.to_string());
            let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
            let change = FieldChange {
                title: title.as_deref(),
                description: description.as_ref().map(|d| d.as_deref()),
                description_blob: blob_id,
                priority: cmd.priority,
                labels: labels.as_deref(),
                deadline,
                run: cmd.run.as_ref(),
                design_url: design_url.as_ref().map(|d| d.as_deref()),
                files_added,
            };
            write_fields(tx, &st, &mut e, &state, &change)?;
            finish_then(tx, state.id, e, |_| Ok(told))
        });
        // Copies of a command that failed or was only replayed are not kept.
        self.discard_unused(&added, committed.as_ref().map_or(true, |c| c.receipt.replayed));
        committed
    }

    /// Check and copy `add_files` of a ticket update before its transaction: the
    /// count against what the ticket has, the copies next to its files (so a
    /// ticket keeps one folder). Gives the attachments it had and the new copies.
    fn add_ticket_files(&self, cmd: &ticket::UpdateTicket) -> Result<(Option<task_store::TaskAttachments>, Option<task_store::TaskAttachments>), AppError> {
        let files = cmd.add_files.as_deref().unwrap_or_default();
        if files.is_empty() {
            return Ok((None, None));
        }
        let id = cmd.ticket_id;
        let (state, before) = self
            .store()
            .read(|c| Ok((store::state(c, id)?, store::attachments_of(c, id)?)))
            .map_err(storage_error)?;
        let state = state.ok_or_else(|| domain_error(DomainError::TicketNotFound(id)))?;
        let kept = before.as_ref().map_or(0, |a| a.files.len());
        ticket::check_file_count(kept + files.len()).map_err(domain_error)?;
        let folder = before.as_ref().map(|a| std::path::PathBuf::from(&a.folder));
        let added = self.attach_files(state.project_id, None, files, folder.as_deref())?;
        Ok((before, added))
    }

    fn update_company(&self, base: Base, envelope: &CommandEnvelope, hash: String, cmd: UpdateCompany, by: Actor) -> Result<Committed, AppError> {
        let name = cmd.valid_name().map_err(domain_error)?;
        let dispatcher = cmd.dispatcher.map(|d| d.employee_id);
        let tester = cmd.tester.map(|t| t.employee_id);
        if name.is_none() && dispatcher.is_none() && tester.is_none() {
            return Err(domain_error(DomainError::Required { field: "a field to change".into() }));
        }
        let expected = envelope.expected_version;
        self.commit(envelope.command_id, hash, move |tx| {
            if let Some(Some(employee)) = dispatcher {
                check!(check_employee(tx, employee)?);
            }
            // The tester runs the suite as soon as a card reports: it must be at work now.
            if let Some(Some(employee)) = tester {
                check!(check_employee(tx, employee)?);
                if company_store::employee_status(tx, employee)?.as_deref() != Some(LifecycleStatus::Active.as_str()) {
                    return Ok(Err(AppError::new(aurelius_domain::ErrorCode::Validation, TESTER_NOT_ACTIVE)));
                }
            }
            let company = company_store::get(tx)?;
            if let Some(expected) = expected {
                if expected != company.version {
                    return rejected(DomainError::VersionConflict { expected, actual: company.version });
                }
            }
            let version = company_store::update(tx, name.as_deref(), dispatcher, tester)?;
            let previous = aurelius_storage::conversations::next_version(tx, "company", &company.id)? - 1;
            let mut e = Events::new(&base, "company", company.id, None, previous);
            let mut extra = json!({ "by": by });
            if let Some(name) = &name {
                extra["name"] = json!(name);
            }
            if let Some(choice) = dispatcher {
                extra["dispatcher_employee_id"] = json!(choice);
            }
            if let Some(choice) = tester {
                extra["tester_employee_id"] = json!(choice);
            }
            e.push(tx, EventKind::CompanyUpdated, extra)?;
            Ok(Ok((CommandResult::CompanyUpdated { version }, e.notices)))
        })
    }

    /// `ticket.retry_summary` (MU2-12 AC4): a board member asks for the summary of a
    /// ticket on Done again, after it failed or when it is missing. Refused while it
    /// is there already (never a second item) or still being made; else the ticket's
    /// extraction is queued once more and its summary state is pending again; the
    /// ticket's column and version do not change.
    pub(crate) fn retry_ticket_summary(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &ticket::RetryTicketSummary) -> Result<Committed, AppError> {
        require_board_member(ctx)?;
        let id = cmd.ticket_id;
        self.commit(envelope.command_id, hash, move |tx| {
            let Some(state) = store::state(tx, id)? else { return rejected(DomainError::TicketNotFound(id)) };
            if state.column != ColumnKind::Done {
                return rejected(DomainError::Ticket("Only a ticket on Done has a summary to try again.".into()));
            }
            // On the ticket already (ticket #57); for a ticket on Done before the History,
            // its summary in memory counts (never a second item).
            if aurelius_storage::history::has_summary(tx, id)? || (aurelius_storage::history::stored_state(tx, id)?.is_none() && crate::memory::has_ticket_summary(tx, id)?) {
                return rejected(DomainError::Ticket("The ticket has its summary already.".into()));
            }
            if crate::memory::ticket_summary_pending(tx, id)? {
                return rejected(DomainError::Ticket("The summary is being made; wait for it.".into()));
            }
            let Some(root) = state.root_task_id else { return rejected(DomainError::Ticket("This ticket has no main task to summarize.".into())) };
            let accepted: Option<String> = tx
                .query_row("SELECT id FROM events WHERE aggregate_type = 'ticket' AND aggregate_id = ?1 AND type = 'ticket.accepted' ORDER BY seq DESC LIMIT 1", [id.to_string()], |r| r.get(0))
                .ok();
            let Some(source) = accepted.and_then(|e| aurelius_domain::EventId::parse(&e).ok()) else {
                return rejected(DomainError::Ticket("This ticket was not accepted on the board; there is no summary to make.".into()));
            };
            crate::memory::schedule_ticket_summary(tx, root, id, source)?;
            // Pending again, so the end of this extraction, also a second failure,
            // changes the state and tells an open History (ticket #57).
            aurelius_storage::history::mark_pending(tx, id)?;
            // An open History shows "Summarizing…" now (ticket #57).
            let notices: Vec<EventNotice> = crate::memory::summary_changed(tx, id)?.into_iter().collect();
            let version = store::state(tx, id)?.map_or(state.version, |s| s.version);
            Ok(Ok((CommandResult::TicketChanged { ticket_id: id, column: state.column, version }, notices)))
        })
    }

    /// `project.update`: a project's own dispatcher (B5). A board member, as for the company's.
    pub(crate) fn update_project(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &UpdateProject) -> Result<Committed, AppError> {
        let by = Actor::BoardMember { user_id: require_board_member(ctx)? };
        let cmd = cmd.clone();
        let dispatcher = cmd.dispatcher.map(|d| d.employee_id);
        let styleguide = cmd.styleguide_url.as_deref().map(|u| ticket::valid_artifact_url("styleguide_url", u)).transpose().map_err(domain_error)?;
        if dispatcher.is_none() && styleguide.is_none() {
            return Err(domain_error(DomainError::Required { field: "a field to change".into() }));
        }
        let expected = envelope.expected_version;
        let base = Base::of(ctx, envelope);
        self.commit(envelope.command_id, hash, move |tx| {
            let Some(project) = queries::load_project(tx, cmd.project_id)? else { return rejected(DomainError::ProjectNotFound(cmd.project_id)) };
            if let Some(expected) = expected {
                if expected != project.version {
                    return rejected(DomainError::VersionConflict { expected, actual: project.version });
                }
            }
            if project.status == ProjectStatus::Archived {
                return rejected(DomainError::ProjectArchived);
            }
            if let Some(employee) = dispatcher.flatten() {
                check!(check_employee(tx, employee)?);
            }
            let version = company_store::update_project(tx, cmd.project_id, dispatcher, styleguide.as_ref().map(|s| s.as_deref()))?;
            let mut e = Events::new(&base, "project", cmd.project_id.to_string(), Some(cmd.project_id), version - 1);
            let mut payload = json!({ "by": by });
            if let Some(dispatcher) = dispatcher {
                payload["dispatcher_employee_id"] = json!(dispatcher);
            }
            if styleguide.is_some() {
                payload["changed"] = json!(["styleguide"]);
            }
            e.push(tx, EventKind::ProjectUpdated, payload)?;
            Ok(Ok((CommandResult::ProjectUpdated { project_id: cmd.project_id, version }, e.notices)))
        })
    }
}

/// What a new ticket is made of (the shared core of `ticket.create` and an employee's `create_ticket`).
struct NewCard<'a> {
    project: ProjectId,
    title: &'a str,
    description: Option<&'a str>,
    description_blob: Option<String>,
    priority: ticket::TicketPriority,
    /// The board member who owns it (`created_by_user`, its first follower).
    user: UserId,
    responsible: Option<UserId>,
    owner: Option<EmployeeId>,
    run: &'a aurelius_domain::task::RunSettings,
    labels: &'a [String],
    deadline_ms: Option<i64>,
    attachments: Option<&'a task_store::TaskAttachments>,
    kind: TicketKind,
}

/// Insert a ticket at the bottom of Backlog and write its `ticket.created` event, the creator's
/// follow, a Responsible other than the creator and the board members its description names.
/// `e` takes the ticket's identity here; the caller goes on with the rest of its write.
/// Gives the id, the number and the `ticket.created` event (the source of a job it queues).
fn insert_ticket(tx: &Transaction<'_>, e: &mut Events, card: &NewCard<'_>) -> Result<(TicketId, u32, aurelius_domain::EventId), StorageError> {
    let id = TicketId::new();
    let backlog = company_store::column_of_kind(tx, ColumnKind::Backlog)?;
    let number = store::insert(
        tx,
        &store::NewTicket {
            id,
            project_id: card.project,
            title: card.title,
            description_blob: card.description_blob.as_deref(),
            column_id: backlog.id,
            priority: card.priority,
            created_by_user: card.user,
            owner: card.owner,
            run: card.run,
            labels: card.labels,
            deadline_ms: card.deadline_ms,
            kind: card.kind,
        },
    )?;
    store::set_attachments(tx, id, card.attachments)?;
    e.aggregate_id = id.to_string();
    store::set_responsible(tx, id, card.responsible)?;
    let named = match card.description {
        Some(text) => ticket_follow::mentioned_board_members(tx, text)?,
        None => Vec::new(),
    };
    let by = e.by.unwrap_or(Actor::BoardMember { user_id: card.user });
    let mut created = json!({ "number": number, "created_by_user": card.user, "by": by, "responsible_user_id": card.responsible });
    if !named.is_empty() {
        created["mentioned_users"] = json!(named);
    }
    if card.kind == TicketKind::Quick {
        created["kind"] = json!(card.kind);
    }
    let created = e.push_id(tx, EventKind::TicketCreated, created)?;
    e.follow(tx, id, card.project, card.user, FollowReason::Created)?;
    // Choosing someone else is news for them; "You" (the default, B9) says nothing new.
    if let Some(responsible) = card.responsible.filter(|r| *r != card.user) {
        e.responsible(tx, id, card.project, None, Some(responsible), by)?;
    }
    for named in named {
        e.follow(tx, id, card.project, named, FollowReason::Mentioned)?;
    }
    Ok((id, number, created))
}

/// The fields of a ticket update (`ticket.update` and an employee's `update_ticket`): what is
/// given is written, with the board members the new description names following.
struct FieldChange<'a> {
    title: Option<&'a str>,
    /// `Some(None)` clears the description.
    description: Option<Option<&'a str>>,
    description_blob: Option<String>,
    priority: Option<ticket::TicketPriority>,
    labels: Option<&'a [String]>,
    /// `Some(None)` clears the deadline.
    deadline: Option<Option<i64>>,
    run: Option<&'a aurelius_domain::task::RunSettings>,
    /// `Some(None)` clears the design link.
    design_url: Option<Option<&'a str>>,
    files_added: Vec<String>,
}

/// Write the changed fields of `state`'s ticket and its `ticket.updated` event; the callers check
/// the rights and the ticket's state first.
fn write_fields(tx: &Transaction<'_>, st: &aurelius_storage::Store, e: &mut Events, state: &store::TicketState, change: &FieldChange<'_>) -> Result<(), StorageError> {
    // Read before it is replaced: only names that are new here make someone follow.
    let named = match change.description.flatten() {
        Some(text) => {
            let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            let old = store::view(tx, state.id, &read)?.and_then(|t| t.description);
            ticket_follow::newly_mentioned(tx, old.as_deref(), text)?
        }
        None => Vec::new(),
    };
    store::update_fields(
        tx,
        state.id,
        &store::TicketFields {
            title: change.title,
            description_blob: change.description.map(|_| change.description_blob.as_deref()),
            priority: change.priority,
            labels: change.labels,
            deadline_ms: change.deadline,
            owner: None,
            run: change.run,
            design_url: change.design_url,
        },
    )?;
    let changed: Vec<&str> = [
        ("title", change.title.is_some()),
        ("description", change.description.is_some()),
        ("priority", change.priority.is_some()),
        ("labels", change.labels.is_some()),
        ("deadline", change.deadline.is_some()),
        ("run", change.run.is_some()),
        ("design", change.design_url.is_some()),
        ("files", !change.files_added.is_empty()),
    ]
    .into_iter()
    .filter_map(|(name, given)| given.then_some(name))
    .collect();
    let by = e.by;
    let mut extra = json!({ "changed": changed, "by": by });
    // Only the names of the new files, never where they are kept.
    if !change.files_added.is_empty() {
        extra["files_added"] = json!(change.files_added);
    }
    if !named.is_empty() {
        extra["mentioned_users"] = json!(named);
    }
    e.push(tx, EventKind::TicketUpdated, extra)?;
    for user in named {
        e.follow(tx, state.id, state.project_id, user, FollowReason::Mentioned)?;
    }
    Ok(())
}

/// Put a Backlog ticket in Todo as started by `by`, with `owner` if one is chosen: the column
/// change, `ticket.started`, the line in the discussion and `ticket.dispatched`. The caller
/// hands over to the engine (`ticket_line::started`) in the same transaction.
fn start_card(tx: &Transaction<'_>, e: &mut Events, state: &store::TicketState, by: Actor, owner: Option<EmployeeId>) -> Result<(), StorageError> {
    let todo = company_store::column_of_kind(tx, ColumnKind::Todo)?;
    store::move_to(tx, state.id, todo.id, None)?;
    store::mark_started(tx, state.id)?;
    e.moved(tx, state.column, ColumnKind::Todo, by)?;
    e.push(tx, EventKind::TicketStarted, json!({ "by": by }))?;
    let what = match owner {
        Some(employee) => format!("started the ticket with {} as owner", crate::conversation::discussion::employee_name(tx, employee)?),
        None => "started the ticket".to_owned(),
    };
    e.line(ticket_line::column_line(tx, state.id, Some(by), state.column, ColumnKind::Todo, Some(&what))?);
    if let Some(employee) = owner {
        store::set_owner(tx, state.id, Some(employee))?;
        e.push(tx, EventKind::TicketDispatched, json!({ "employee_id": employee, "by": by, "reason": Value::Null }))?;
    }
    Ok(())
}

/// "‹name› put it on Done" (MU2-06 AC2).
fn done_line(tx: &Transaction<'_>, ticket: TicketId, by: Actor) -> Result<Vec<EventNotice>, StorageError> {
    let mut line = aurelius_domain::views::SystemLine::new(aurelius_domain::views::SystemLineKind::Done).with_action(system_action::APPROVED);
    line.by = Some(crate::conversation::discussion::actor_name(tx, Some(by))?);
    crate::conversation::discussion::ticket_line(tx, ticket, line, "")
}

/// Put a ticket last in the column of `kind`.
fn to_column(tx: &Transaction<'_>, id: TicketId, kind: ColumnKind) -> Result<(), StorageError> {
    let column = company_store::column_of_kind(tx, kind)?;
    store::move_to(tx, id, column.id, None)
}

/// As `finish`, then the engine's part, which may add events (each the next
/// version) and move the card; the answer is where the card is after it.
/// The feedback of a decision, as the board member's message in the ticket's discussion (MU2-05 AC3).
fn feedback_note(tx: &Transaction<'_>, st: &aurelius_storage::Store, ticket: TicketId, user: UserId, base: &Base, text: &str) -> Result<Vec<EventNotice>, StorageError> {
    crate::conversation::discussion::decision_feedback(tx, st, ticket, user, &base.meta, &base.command_id, text)
}

fn finish_then(
    tx: &Transaction<'_>,
    id: TicketId,
    e: Events,
    engine: impl FnOnce(&Transaction<'_>) -> Result<Vec<EventNotice>, StorageError>,
) -> TxResult<(CommandResult, Vec<EventNotice>)> {
    store::touch(tx, id, e.version)?;
    let mut notices = e.notices;
    notices.extend(engine(tx)?);
    // The ticket's channel follows: who joined, closed on Done, reopened off it.
    notices.extend(crate::conversation::ticket_channel::sync(tx, id, e.by)?);
    let Some(state) = store::state(tx, id)? else { return rejected(DomainError::TicketNotFound(id)) };
    Ok(Ok((CommandResult::TicketChanged { ticket_id: id, column: state.column, version: state.version }, notices)))
}

/// Store the ticket's version after its events and answer where it is now.
fn finish(tx: &Transaction<'_>, id: TicketId, column: ColumnKind, e: Events) -> TxResult<(CommandResult, Vec<EventNotice>)> {
    store::touch(tx, id, e.version)?;
    let mut notices = e.notices;
    notices.extend(crate::conversation::ticket_channel::sync(tx, id, e.by)?);
    Ok(Ok((CommandResult::TicketChanged { ticket_id: id, column, version: e.version }, notices)))
}
