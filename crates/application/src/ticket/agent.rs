//! The board as an employee's tools (AUR-BOARD-TOOL-001): `list_tickets`, `read_ticket`,
//! `create_ticket`, `update_ticket`, `move_ticket` and `post_to_ticket`. Each runs like the other
//! employee actions (`triage::assign`): one transaction with its events and system line, the
//! employee as actor and as `by`, and nothing written when the action is refused. They call the
//! cores of the board member's commands (`insert_ticket`, `write_fields`, `start_card`), so a
//! ticket changes the same way whoever changes it.
//!
//! What an employee may do is a decision of the Tech Lead, kept here as consts and checks:
//!
//! - **Reading** (`list_tickets`, `read_ticket`): any employee's run, for the tickets the board
//!   shows (not archived). A ticket that is not shown answers like one that does not exist.
//! - **Managers** are found by structure, never by name: the employee at the top of the org chart
//!   (no boss, with reports of its own) or the dispatcher of the ticket's project (the project's,
//!   else the company's). In a flat company only the dispatcher is a manager.
//! - **Creating**: managers only, in Backlog. It never starts anything: no run, task or job.
//! - **Updating** (title, description, priority, labels): managers on any open ticket, others only
//!   on a ticket they work on (`tickets::employee_works_on`). Done and archived never change.
//! - **Moving**: managers only, and only Backlog → Todo (a start) or staying where it is. Every
//!   other move is a decision of a board member or of the work itself.
//! - **Posting**: any employee's run on any shown ticket (`discussion::post_from_run`).
//!
//! - **Delegating**: `delegate_task` follows the org chart, not these rules, but its task always
//!   lands on a ticket that is shown and not Done (`for_delegation`).
//!
//! There is no tool for accepting, approving or revising a plan, sending back, cancelling,
//! archiving, deleting, choosing the Responsible or assigning an owner.

use super::{Base, Events, FieldChange, NewCard, insert_ticket, start_card, store};
use crate::conversation::{discussion, ticket_channel};
use crate::task::{task_runs, ticket_line};
use aurelius_domain::{
    EmployeeId, EventNotice, ProjectId, RunId, TicketId, UserId,
    autonomy::AgentMode,
    ticket::{self, Actor, ColumnKind, TicketPriority, TicketView},
};
use aurelius_storage::{StorageError, Store, Tx, company as company_store, employees};
use rusqlite::Connection;

/// What `list_tickets` shows when no `limit` is given.
pub const LIST_LIMIT_DEFAULT: u32 = 25;
/// The most tickets one `list_tickets` call shows.
pub const LIST_LIMIT_MAX: u32 = 50;

/// Why only management creates tickets.
pub const CREATE_MANAGERS_ONLY: &str = "Only management creates tickets (the head of the org chart or the dispatcher). Ask a manager to create it.";
/// Why a colleague cannot change a ticket it does not work on.
pub const UPDATE_NOT_YOURS: &str = "You can only change a ticket you work on (you own it or have an open task in it); post in its discussion with post_to_ticket, or ask a manager.";
/// Why only management moves a ticket.
pub const MOVE_MANAGERS_ONLY: &str = "Only management moves a ticket, and only from Backlog to Todo (a start). Post in its discussion with post_to_ticket, or ask a manager.";
/// Why a ticket does not go to Done.
pub const MOVE_DONE: &str = "Only a board member puts a ticket on Done (Accept, from Review).";
/// Why a ticket does not go to Plan review.
pub const MOVE_PLAN_REVIEW: &str = "A ticket reaches Plan review when its owner presents a plan; a board member approves it or asks for a new one.";
/// Why a ticket does not go to Review.
pub const MOVE_REVIEW: &str = "A ticket reaches Review when its owner reports; a board member accepts it or sends it back.";
/// Why a ticket does not go to Doing.
pub const MOVE_DOING: &str = "A ticket goes to Doing by itself when its owner starts the work; the work moves it, not a hand.";
/// Why a started ticket does not go back to Backlog.
pub const MOVE_BACKLOG: &str = "Moving a started ticket back to Backlog cancels its work; that is a board member's decision.";
/// Why only a ticket in Backlog goes to Todo.
pub const MOVE_TODO_FROM_BACKLOG: &str = "Only a ticket in Backlog can be started (moved to Todo); this one is already started.";
/// Why the run cannot act for a board member.
pub const NO_BOARD_MEMBER: &str = "No board member stands behind this run (it has no Claude login), so a ticket cannot be made or started from it.";
/// Why a run in Plan mode cannot change the board.
pub const PLAN_MODE_READ_ONLY: &str = "This run is in Plan mode: it may read tickets (list_tickets, read_ticket) but not create, change, move or post. Present your plan first; the writes follow once a board member approves it.";
/// Why a run cannot start a ticket that runs on another board member's login.
pub const START_OTHER_LOGIN: &str = "This ticket runs on another board member's Claude login than the one behind this run, so it cannot be started from here. Ask that board member to start it, or post in its discussion with post_to_ticket.";
/// Why an employee cannot rewrite a ticket's text while it is under review.
pub const UPDATE_TEXT_LOCKED: &str = "The text of a ticket in Plan review, Testing or Review is under review and cannot be changed by an employee; post in its discussion with post_to_ticket instead.";
/// Why a task is refused without a ticket.
pub const DELEGATE_NEEDS_TICKET: &str = "Work always belongs to a ticket and this conversation has none: name it with ticket (#12; list_tickets shows the tickets). If none fits, management creates one with create_ticket, or ask a board member for one; then delegate again.";
/// Why a finished ticket takes no new task.
pub const DELEGATE_TICKET_DONE: &str = "That ticket is Done and takes no new work; name an open ticket.";
/// Why a run cannot put work on a ticket that runs on another board member's login.
pub const DELEGATE_OTHER_LOGIN: &str = "This ticket runs on another board member's Claude login than the one behind this run, so work cannot be added to it from here. Post in its discussion with post_to_ticket, or name a ticket of this board member.";
/// Why only an employee's run uses the board tools.
pub const EMPLOYEES_ONLY: &str = "Only an employee's run can use the ticket tools.";

/// The refusal for a run in Plan mode, else none; the board changes only outside Plan mode.
fn plan_mode_refusal(conn: &Connection, run: RunId) -> Result<Option<&'static str>, StorageError> {
    Ok((aurelius_storage::conversations::mode_for_run(conn, run)? == AgentMode::Plan).then_some(PLAN_MODE_READ_ONLY))
}

fn refuse<T: Default>(why: impl Into<String>) -> Result<(bool, String, T), StorageError> {
    Ok((false, why.into(), T::default()))
}

/// The employee of a run and the project it works in (none in a chat outside projects).
struct Caller {
    employee: EmployeeId,
    project: Option<ProjectId>,
}

fn caller(conn: &Connection, run: RunId) -> Result<Option<Caller>, StorageError> {
    Ok(task_runs::run_owner(conn, run)?.and_then(|o| Some(Caller { employee: o.employee?, project: o.project })))
}

/// Why a ticket cannot be found: the same words for one that does not exist, is archived or is not shown.
fn not_found(needle: &str) -> String {
    format!("No ticket {} found. Use its number (#12) or id; list_tickets shows the tickets.", needle.trim())
}

/// The ticket a tool names (`#12`, `12` or an id) when the board shows it. Numbers are the company's.
fn find(conn: &Connection, st: &Store, needle: &str) -> Result<Option<TicketView>, StorageError> {
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let bare = needle.trim().trim_start_matches('#');
    let found = if let Ok(id) = TicketId::parse(bare) {
        store::view(conn, id, &read)?
    } else if let Ok(number) = bare.parse::<u32>() {
        store::list(conn, &store::TicketFilter::default(), &read)?.into_iter().find(|t| t.number == number)
    } else {
        None
    };
    Ok(found.filter(|t| t.archived_at_ms.is_none()))
}

/// A column named as a person writes it (`plan review`, `Plan-Review`, `to do`, `todo`).
fn parse_column(text: &str) -> Option<ColumnKind> {
    let plain = |t: &str| t.to_lowercase().replace([' ', '-', '_'], "");
    ColumnKind::ALL.into_iter().find(|k| plain(k.as_str()) == plain(text))
}

fn column_names() -> String {
    ColumnKind::ALL.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(", ")
}

/// Whether `employee` is management for tickets of `project`: the head of the org chart (no boss,
/// with reports of its own) or the project's dispatcher. Only an active employee counts.
pub(crate) fn is_manager(conn: &Connection, employee: EmployeeId, project: ProjectId) -> Result<bool, StorageError> {
    if company_store::dispatcher_for(conn, project)? == Some(employee) {
        return Ok(true);
    }
    if company_store::employee_status(conn, employee)?.as_deref() != Some("active") {
        return Ok(false);
    }
    let org = employees::org(conn)?;
    Ok(org.boss(employee).is_none() && !org.reports(employee).is_empty())
}

/// The ticket a new task of `delegate_task` lands on, or why it cannot: named, shown, not Done, in the
/// project the run works in when it works in one (a task stays in its project). `login` is the run's,
/// when it stands outside the ticket: the task will run on the ticket's login, so it must be the same.
pub(crate) fn for_delegation(conn: &Connection, st: &Store, named: Option<&str>, project: Option<ProjectId>, login: Option<UserId>) -> Result<Result<TicketView, String>, StorageError> {
    let Some(named) = named.map(str::trim).filter(|n| !n.is_empty()) else { return Ok(Err(DELEGATE_NEEDS_TICKET.to_owned())) };
    let Some(found) = find(conn, st, named)? else { return Ok(Err(not_found(named))) };
    Ok(if found.column == ColumnKind::Done {
        Err(DELEGATE_TICKET_DONE.to_owned())
    } else if project.is_some_and(|p| p != found.project_id) {
        Err(format!("Ticket #{} is in another project than the work here; name a ticket of this project.", found.number))
    } else if login.is_some_and(|l| l != found.login_user) {
        Err(DELEGATE_OTHER_LOGIN.to_owned())
    } else {
        Ok(found)
    })
}

/// `list_tickets`: tickets in board order. `project` (a name or id) else the run's project, else every project.
pub(crate) fn list(conn: &Connection, st: &Store, run: RunId, project: Option<&str>, column: Option<&str>, query: Option<&str>, limit: Option<u32>) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let Some(me) = caller(conn, run)? else { return refuse(EMPLOYEES_ONLY) };
    let project = match project.map(str::trim).filter(|p| !p.is_empty()) {
        Some(named) => match task_runs::find_project(conn, named)? {
            Ok(p) => Some(p),
            Err(why) => return refuse(why),
        },
        None => me.project,
    };
    let column = match column.map(str::trim).filter(|c| !c.is_empty()) {
        Some(named) => match parse_column(named) {
            Some(kind) => Some(kind),
            None => return refuse(format!("No column called {named}. The columns are: {}.", column_names())),
        },
        None => None,
    };
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let tickets = store::list(conn, &store::TicketFilter { project, column, query: query.map(str::to_owned), ..Default::default() }, &read)?;
    if tickets.is_empty() {
        return Ok((true, "No tickets match.".into(), vec![]));
    }
    let shown = limit.unwrap_or(LIST_LIMIT_DEFAULT).clamp(1, LIST_LIMIT_MAX) as usize;
    let mut lines: Vec<String> = tickets.iter().take(shown).map(ticket_line_text).collect();
    if tickets.len() > shown {
        lines.push(format!("({} more; narrow it with project, column or query)", tickets.len() - shown));
    }
    Ok((true, lines.join("\n"), vec![]))
}

fn ticket_line_text(t: &TicketView) -> String {
    format!(
        "#{} [{}] {} · project {} · priority {} · owner {} · responsible {}",
        t.number,
        t.column.as_str(),
        t.title,
        t.project_name,
        t.priority.as_str(),
        t.owner_name.as_deref().unwrap_or("none"),
        t.responsible_username.as_deref().unwrap_or("none")
    )
}

/// `read_ticket`: the ticket in full and the latest `last` messages of its discussion.
pub(crate) fn read(conn: &Connection, st: &Store, run: RunId, ticket: &str, last: Option<u32>) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    if caller(conn, run)?.is_none() {
        return refuse(EMPLOYEES_ONLY);
    }
    let Some(t) = find(conn, st, ticket)? else { return refuse(not_found(ticket)) };
    let mut text = format!(
        "#{} “{}” · project {}\nColumn: {} · priority {} · owner {} · responsible {} · made by {}\n",
        t.number,
        t.title,
        t.project_name,
        t.column.as_str(),
        t.priority.as_str(),
        t.owner_name.as_deref().unwrap_or("none"),
        t.responsible_username.as_deref().unwrap_or("none"),
        t.created_by_username
    );
    if !t.labels.is_empty() {
        text.push_str(&format!("Labels: {}\n", t.labels.join(", ")));
    }
    if let Some(status) = t.root_task_status {
        text.push_str(&format!("Main task: {}\n", status.as_str()));
    }
    text.push_str(&format!("Description:\n{}\n", t.description.as_deref().unwrap_or("(none)")));
    if let Some(d) = discussion::of_ticket(conn, t.id)? {
        text.push_str(&format!("\nLatest messages (JSON, oldest first):\n{}", discussion::read_text(conn, st, &d, last)?));
    }
    Ok((true, text, vec![]))
}

/// Store a ticket's version after its events, bring its channel in line, and give the notices.
fn settle(tx: &Tx<'_>, ticket: TicketId, e: Events) -> Result<Vec<EventNotice>, StorageError> {
    store::touch(tx, ticket, e.version)?;
    let mut notices = e.notices;
    notices.extend(ticket_channel::sync(tx, ticket, e.by)?);
    Ok(notices)
}

/// A system line of the employee's change in the ticket's discussion.
fn agent_line(tx: &Tx<'_>, ticket: TicketId, by: Actor, what: &str) -> Result<Vec<EventNotice>, StorageError> {
    let name = discussion::actor_name(tx, Some(by))?;
    let mut line = aurelius_domain::views::SystemLine::new(aurelius_domain::views::SystemLineKind::Moved);
    line.by = Some(name.clone());
    discussion::ticket_line(tx, ticket, line, &format!("{name} {what}"))
}

/// The inputs of `create_ticket`.
pub(crate) struct CreateInput<'a> {
    pub project: Option<&'a str>,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub priority: Option<&'a str>,
    pub labels: &'a [String],
}

fn parse_priority(text: Option<&str>) -> Result<TicketPriority, String> {
    match text.map(str::trim).filter(|p| !p.is_empty()) {
        None => Ok(TicketPriority::default()),
        Some(p) => TicketPriority::parse(&p.to_lowercase()).ok_or_else(|| format!("No priority called {p}. The priorities are: low, normal, high, urgent.")),
    }
}

/// `create_ticket`: a ticket in Backlog, for management. The board member behind the run
/// (`login::of_run`) is its creator and Responsible; the same title made twice in one run is the
/// same ticket. It starts nothing.
pub(crate) fn create(tx: &Tx<'_>, st: &Store, run: RunId, input: CreateInput<'_>) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let Some(me) = caller(tx, run)? else { return refuse(EMPLOYEES_ONLY) };
    if let Some(why) = plan_mode_refusal(tx, run)? {
        return refuse(why);
    }
    let project = match (input.project.map(str::trim).filter(|p| !p.is_empty()), me.project) {
        (Some(named), _) => match task_runs::find_project(tx, named)? {
            Ok(p) => p,
            Err(why) => return refuse(why),
        },
        (None, Some(p)) => p,
        (None, None) => return Ok((false, task_runs::need_project(tx, "a ticket")?, vec![])),
    };
    if !is_manager(tx, me.employee, project)? {
        return refuse(CREATE_MANAGERS_ONLY);
    }
    let (title, description, labels) = match (ticket::valid_title(input.title), ticket::valid_description(input.description), ticket::valid_labels(input.labels)) {
        (Ok(t), Ok(d), Ok(l)) => (t, d, l),
        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => return refuse(e.to_string()),
    };
    let priority = match parse_priority(input.priority) {
        Ok(p) => p,
        Err(why) => return refuse(why),
    };
    let Some(user) = crate::run::login::of_run(tx, run)? else { return refuse(NO_BOARD_MEMBER) };
    if let Some((_, number)) = store::created_in_run(tx, run, me.employee, project, &title)? {
        return Ok((true, format!("Ticket #{number} “{title}” was already made in this run; nothing new was made."), vec![]));
    }
    let blob = match &description {
        Some(text) => {
            let blob = st.blobs().put(text.as_bytes(), "text/markdown; charset=utf-8")?;
            aurelius_storage::blobs::register(tx, &blob)?;
            Some(blob.id.to_string())
        }
        None => None,
    };
    let base = Base::of_employee(me.employee, run);
    let mut e = Events::new(&base, "ticket", String::new(), Some(project), 0);
    let by = Actor::Employee { employee_id: me.employee };
    let (id, number, _) = insert_ticket(
        tx,
        &mut e,
        &NewCard {
            project,
            title: &title,
            description: description.as_deref(),
            description_blob: blob,
            priority,
            user,
            responsible: Some(user),
            owner: None,
            run: &Default::default(),
            labels: &labels,
            deadline_ms: None,
            attachments: None,
            kind: aurelius_domain::ticket::TicketKind::Normal,
        },
    )?;
    let mut notices = settle_created(tx, id, e)?;
    notices.extend(ticket_channel::opened_by(tx, id, user, by)?);
    notices.extend(agent_line(tx, id, by, "created this ticket")?);
    let project_name = crate::queries_project_name(tx, project).unwrap_or_default();
    Ok((true, format!("Created ticket #{number} “{title}” in project {project_name}, in Backlog. It does not start by itself: a manager starts it (move_ticket to todo) or a board member."), notices))
}

fn settle_created(tx: &Tx<'_>, id: TicketId, e: Events) -> Result<Vec<EventNotice>, StorageError> {
    store::touch(tx, id, e.version)?;
    Ok(e.notices)
}

/// The inputs of `update_ticket`; what is not given stays.
pub(crate) struct UpdateInput<'a> {
    pub ticket: &'a str,
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub priority: Option<&'a str>,
    pub labels: Option<&'a [String]>,
    /// A Claude Design artifact link; empty clears it.
    pub design_url: Option<&'a str>,
}

/// `update_ticket`: title, description, priority, labels and design link of an open ticket the
/// employee manages or works on. Owner, mode, files and deadline are not here.
pub(crate) fn update(tx: &Tx<'_>, st: &Store, run: RunId, input: UpdateInput<'_>) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let Some(me) = caller(tx, run)? else { return refuse(EMPLOYEES_ONLY) };
    if let Some(why) = plan_mode_refusal(tx, run)? {
        return refuse(why);
    }
    let Some(view) = find(tx, st, input.ticket)? else { return refuse(not_found(input.ticket)) };
    let Some(state) = store::state(tx, view.id)? else { return refuse(not_found(input.ticket)) };
    if state.column == ColumnKind::Done {
        return refuse("A ticket in Done is closed; it can no longer change.");
    }
    if !is_manager(tx, me.employee, state.project_id)? && !store::employee_works_on(tx, state.id, me.employee)? {
        return refuse(UPDATE_NOT_YOURS);
    }
    if (input.title.is_some() || input.description.is_some()) && matches!(state.column, ColumnKind::PlanReview | ColumnKind::Testing | ColumnKind::Review) {
        return refuse(UPDATE_TEXT_LOCKED);
    }
    let title = match input.title.map(ticket::valid_title).transpose() {
        Ok(t) => t,
        Err(e) => return refuse(e.to_string()),
    };
    let description = match input.description.map(|d| ticket::valid_description(Some(d))).transpose() {
        Ok(d) => d,
        Err(e) => return refuse(e.to_string()),
    };
    let labels = match input.labels.map(ticket::valid_labels).transpose() {
        Ok(l) => l,
        Err(e) => return refuse(e.to_string()),
    };
    let priority = match input.priority.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => match parse_priority(Some(p)) {
            Ok(p) => Some(p),
            Err(why) => return refuse(why),
        },
        None => None,
    };
    let design_url = match input.design_url.map(|u| ticket::valid_artifact_url("design_url", u)).transpose() {
        Ok(d) => d,
        Err(e) => return refuse(e.to_string()),
    };
    if title.is_none() && description.is_none() && priority.is_none() && labels.is_none() && design_url.is_none() {
        return refuse("Give a field to change: title, description, priority, labels or design_url.");
    }
    let blob = match description.as_ref().and_then(|d| d.as_deref()) {
        Some(text) => {
            let blob = st.blobs().put(text.as_bytes(), "text/markdown; charset=utf-8")?;
            aurelius_storage::blobs::register(tx, &blob)?;
            Some(blob.id.to_string())
        }
        None => None,
    };
    let base = Base::of_employee(me.employee, run);
    let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
    let change = FieldChange {
        title: title.as_deref(),
        description: description.as_ref().map(|d| d.as_deref()),
        description_blob: blob,
        priority,
        labels: labels.as_deref(),
        deadline: None,
        run: None,
        design_url: design_url.as_ref().map(|d| d.as_deref()),
        files_added: Vec::new(),
    };
    super::write_fields(tx, st, &mut e, &state, &change)?;
    let by = Actor::Employee { employee_id: me.employee };
    let given: Vec<&str> = [("title", title.is_some()), ("description", description.is_some()), ("priority", priority.is_some()), ("labels", labels.is_some()), ("design link", design_url.is_some())]
        .into_iter()
        .filter_map(|(name, given)| given.then_some(name))
        .collect();
    e.line(agent_line(tx, state.id, by, &format!("changed the {}", given.join(", ")))?);
    let notices = settle(tx, state.id, e)?;
    Ok((true, format!("Updated ticket #{}: {}.", view.number, given.join(", ")), notices))
}

/// Why a move to `target` is not for an employee.
fn move_refusal(from: ColumnKind, target: ColumnKind) -> &'static str {
    match target {
        ColumnKind::Done => MOVE_DONE,
        ColumnKind::PlanReview => MOVE_PLAN_REVIEW,
        ColumnKind::Testing => ticket::TESTING_MOVES_BY_ITSELF,
        ColumnKind::Review => MOVE_REVIEW,
        ColumnKind::Doing => MOVE_DOING,
        ColumnKind::Backlog => MOVE_BACKLOG,
        ColumnKind::Todo if from != ColumnKind::Backlog => MOVE_TODO_FROM_BACKLOG,
        ColumnKind::Todo => "",
    }
}

/// `move_ticket`: management starts a ticket from Backlog (to Todo, as the Start button; the
/// dispatcher then chooses the owner), or leaves it where it is. Nothing else moves by hand.
pub(crate) fn move_to(tx: &Tx<'_>, st: &Store, run: RunId, ticket_needle: &str, column: &str) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let Some(me) = caller(tx, run)? else { return refuse(EMPLOYEES_ONLY) };
    if let Some(why) = plan_mode_refusal(tx, run)? {
        return refuse(why);
    }
    let Some(view) = find(tx, st, ticket_needle)? else { return refuse(not_found(ticket_needle)) };
    let Some(state) = store::state(tx, view.id)? else { return refuse(not_found(ticket_needle)) };
    if !is_manager(tx, me.employee, state.project_id)? {
        return refuse(MOVE_MANAGERS_ONLY);
    }
    let Some(target) = parse_column(column) else { return refuse(format!("No column called {}. The columns are: {}.", column.trim(), column_names())) };
    if state.column == ColumnKind::Done {
        return refuse("A ticket in Done is closed; it cannot move.");
    }
    if target == state.column {
        return Ok((true, format!("Ticket #{} is in {} already; nothing moved.", view.number, target.as_str()), vec![]));
    }
    if (state.column, target) != (ColumnKind::Backlog, ColumnKind::Todo) {
        return refuse(move_refusal(state.column, target));
    }
    if let Err(why) = ticket::check_start(state.column, state.root_status) {
        return refuse(why.to_string());
    }
    let Some(user) = crate::run::login::of_run(tx, run)? else { return refuse(NO_BOARD_MEMBER) };
    if store::login_user(tx, state.id)?.is_some_and(|ticket_login| ticket_login != user) {
        return refuse(START_OTHER_LOGIN);
    }
    let by = Actor::Employee { employee_id: me.employee };
    let base = Base::of_employee(me.employee, run);
    let mut e = Events::new(&base, "ticket", state.id.to_string(), Some(state.project_id), state.version);
    start_card(tx, &mut e, &state, by, None)?;
    store::touch(tx, state.id, e.version)?;
    let mut notices = e.notices;
    notices.extend(ticket_line::started(tx, st, state.id, user)?);
    notices.extend(ticket_channel::sync(tx, state.id, e.by)?);
    Ok((true, format!("Started ticket #{}: it is in Todo and the dispatcher chooses its owner.", view.number), notices))
}

/// `post_to_ticket`: a message in the discussion of any shown ticket, as `post_to_discussion`
/// does in the run's own (same limits, wake budget and read-only rules; a ticket on Done is read-only).
pub(crate) fn post(tx: &Tx<'_>, st: &Store, run: RunId, ticket_needle: &str, message: &str) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    if caller(tx, run)?.is_none() {
        return refuse(EMPLOYEES_ONLY);
    }
    if let Some(why) = plan_mode_refusal(tx, run)? {
        return refuse(why);
    }
    let Some(view) = find(tx, st, ticket_needle)? else { return refuse(not_found(ticket_needle)) };
    match discussion::of_ticket(tx, view.id)? {
        Some(d) => discussion::post_from_run(tx, st, run, &d, message),
        None => refuse(format!("Ticket #{} has no discussion.", view.number)),
    }
}
