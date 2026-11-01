//! Ticket, board and company writes: the board member's side of the workflow.

use super::*;
use aurelius_domain::company::{UpdateCompany, UpdateProject};
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{self, ColumnKind, SetDispatcher, TicketPriority};
use aurelius_domain::{EmployeeId, TicketId, UserId, task::TaskStatus};
use aurelius_storage::{company as company_store, tickets as ticket_store};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    project: ProjectId,
    user: UserId,
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app: None, user_id: user }
}

fn run(s: &Setup, command: Command) -> Result<CommandResult, AppError> {
    run_as(s, Some(s.user), command, None)
}

fn run_as(s: &Setup, user: Option<UserId>, command: Command, expected_version: Option<u64>) -> Result<CommandResult, AppError> {
    s.service.execute(&ctx(user), CommandEnvelope { command_id: CommandId::new(), expected_version, command }).map(|c| c.receipt.result)
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    let created = service
        .execute(&ctx(None), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }) })
        .unwrap()
        .receipt
        .result;
    let CommandResult::ProjectCreated { project_id, .. } = created else { panic!("{created:?}") };
    let user = UserId::new();
    service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &user.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC))
        .unwrap();
    Setup { _dir: dir, service, project: project_id, user }
}

fn employee(s: &Setup, name: &str, status: &str) -> EmployeeId {
    let (id, name, status) = (EmployeeId::new(), name.to_owned(), status.to_owned());
    s.service
        .store()
        .write(move |tx| {
            tx.execute(
                "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'employee', (SELECT id FROM scopes WHERE kind = 'global'), ?2)",
                [aurelius_domain::ScopeId::new().to_string(), id.to_string()],
            )?;
            tx.execute(
                "INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms)
                 VALUES (?1, (SELECT id FROM scopes WHERE owner_id = ?1), ?2, 'r', '{}', ?3, 1, 1, 1)",
                [id.to_string(), name, status],
            )?;
            Ok(())
        })
        .unwrap();
    id
}

fn new_ticket(s: &Setup, title: &str) -> TicketId {
    let created = run(
        s,
        Command::CreateTicket(ticket::CreateTicket {
            confirm_secret: false,
            project_id: s.project,
            title: title.into(),
            description: Some("**Why**: because".into()),
            priority: TicketPriority::default(),
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: None,
            run: Default::default(),
            files: vec![],
            start: false,
            from_message: None,
        }),
    )
    .unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = created else { panic!("{created:?}") };
    ticket_id
}

fn view(s: &Setup, id: TicketId) -> aurelius_domain::ticket::TicketView {
    let store = s.service.store().clone();
    store.read(|c| ticket_store::view(c, id, &|k| store.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok()))).unwrap().unwrap()
}

fn column(s: &Setup, kind: ColumnKind) -> aurelius_domain::BoardColumnId {
    s.service.store().read(|c| company_store::column_of_kind(c, kind)).unwrap().id
}

/// Put a ticket where the engine would (Review, Plan review): not a hand's move.
fn engine_puts(s: &Setup, id: TicketId, kind: ColumnKind) {
    let target = column(s, kind);
    s.service.store().write(move |tx| ticket_store::move_to(tx, id, target, None)).unwrap();
}

/// Where the work of the main task is (as the engine would have put it), without a run.
fn root_is(s: &Setup, id: TicketId, status: TaskStatus, reason: Option<&'static str>) {
    let root = view(s, id).root_task_id.expect("a main task");
    s.service.store().write(move |tx| aurelius_storage::tasks::set_task_status(tx, root, status, reason)).unwrap();
}

fn task_owner(s: &Setup, task: aurelius_domain::TaskId) -> Option<EmployeeId> {
    s.service.store().read(|c| aurelius_storage::tasks::status_and_employee(c, task)).unwrap().and_then(|(_, e)| e)
}

fn kinds(s: &Setup, id: TicketId) -> Vec<String> {
    s.service
        .store()
        .read(|c| {
            let mut stmt = c.prepare("SELECT type FROM events WHERE aggregate_type = 'ticket' AND aggregate_id = ?1 ORDER BY aggregate_version")?;
            let rows = stmt.query_map([id.to_string()], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

#[test]
fn every_ticket_board_and_company_write_needs_a_board_member() {
    let s = setup();
    let ticket = new_ticket(&s, "Mine");
    let commands = [
        Command::CreateTicket(ticket::CreateTicket { confirm_secret: false, project_id: s.project, title: "x".into(), description: None, priority: Default::default(), labels: vec![], deadline_ms: None, owner_employee_id: None, run: Default::default(), files: vec![], start: false, from_message: None }),
        Command::UpdateTicket(ticket::UpdateTicket { confirm_secret: false, ticket_id: ticket, title: Some("y".into()), description: None, priority: None, labels: None, deadline_ms: None, clear_deadline: false, run: None }),
        Command::MoveTicket(ticket::MoveTicket { ticket_id: ticket, column_id: column(&s, ColumnKind::Todo), before_ticket_id: None }),
        Command::StartTicket(ticket::StartTicket { ticket_id: ticket, employee_id: None }),
        Command::AssignTicket(ticket::AssignTicket { ticket_id: ticket, employee_id: Some(EmployeeId::new()), reason: None }),
        Command::ApprovePlan(ticket::ApprovePlan { ticket_id: ticket, decision: ticket::PlanApproval::Approve, feedback: None }),
        Command::RevisePlan(ticket::RevisePlan { confirm_secret: false, ticket_id: ticket, feedback: "again".into() }),
        Command::AcceptTicket(ticket::AcceptTicket { confirm_secret: false, ticket_id: ticket, note: None }),
        Command::SendBackTicket(ticket::SendBackTicket { confirm_secret: false, ticket_id: ticket, feedback: "no".into() }),
        Command::CancelTicket(ticket::CancelTicket { ticket_id: ticket }),
        Command::ArchiveTicket(ticket::ArchiveTicket { ticket_id: ticket }),
        Command::UpdateBoardColumns(ticket::UpdateBoardColumns { columns: vec![] }),
        Command::UpdateCompany(UpdateCompany { name: Some("Acme".into()), dispatcher: None, tester: None }),
        Command::UpdateProject(UpdateProject { project_id: s.project, dispatcher: Some(SetDispatcher { employee_id: None }) }),
    ];
    for command in commands {
        let method = command.method();
        let err = run_as(&s, None, command, None).unwrap_err();
        assert_eq!(err.code, ErrorCode::BoardMemberRequired, "{method}");
    }
    assert_eq!(kinds(&s, ticket), ["ticket.created"], "nothing changed");
}

#[test]
fn a_new_ticket_is_in_backlog_by_its_author_with_normal_priority() {
    let s = setup();
    let id = new_ticket(&s, "  Login page  ");
    let t = view(&s, id);
    assert_eq!((t.title.as_str(), t.column, t.priority, t.number), ("Login page", ColumnKind::Backlog, TicketPriority::Normal, 1));
    assert_eq!((t.created_by_user, t.description.as_deref()), (s.user, Some("**Why**: because")));
    assert!(t.root_task_id.is_none() && t.started_at_ms.is_none(), "MU1-07 AC4: no task, no run");
    // The author and the executor are in the event's metadata.
    let meta: serde_json::Value = s
        .service
        .store()
        .read(|c| Ok(c.query_row("SELECT metadata_json FROM events WHERE aggregate_id = ?1", [id.to_string()], |r| r.get::<_, String>(0))?))
        .map(|m| serde_json::from_str(&m).unwrap())
        .unwrap();
    assert_eq!(meta["user_id"], serde_json::json!(s.user));
    assert_eq!(meta["created_by_user"], serde_json::json!(s.user));
    assert_eq!(meta["by"], serde_json::json!({ "kind": "board_member", "user_id": s.user }));

    let err = run(&s, Command::CreateTicket(ticket::CreateTicket { confirm_secret: false, project_id: s.project, title: " ".into(), description: None, priority: Default::default(), labels: vec![], deadline_ms: None, owner_employee_id: None, run: Default::default(), files: vec![], start: false, from_message: None })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation, "MU1-07 AC2: a title is needed");
    let err = run(&s, Command::CreateTicket(ticket::CreateTicket { confirm_secret: false, project_id: ProjectId::new(), title: "x".into(), description: None, priority: Default::default(), labels: vec![], deadline_ms: None, owner_employee_id: None, run: Default::default(), files: vec![], start: false, from_message: None })).unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound, "and an existing project");

    // `start`: straight to Todo.
    let started = run(&s, Command::CreateTicket(ticket::CreateTicket { confirm_secret: false, project_id: s.project, title: "Now".into(), description: None, priority: TicketPriority::Urgent, labels: vec![], deadline_ms: None, owner_employee_id: None, run: Default::default(), files: vec![], start: true, from_message: None })).unwrap();
    let CommandResult::TicketCreated { ticket_id, number } = started else { panic!() };
    assert_eq!(number, 2);
    let t = view(&s, ticket_id);
    // No dispatcher in this company: the engine's draft main task waits for a board member (B5).
    assert_eq!((t.column, t.priority, t.version), (ColumnKind::Todo, TicketPriority::Urgent, 4));
    assert_eq!(kinds(&s, ticket_id), ["ticket.created", "ticket.moved", "ticket.started", "ticket.updated"]);
    assert_eq!(t.root_task_status, Some(TaskStatus::Draft));
}

#[test]
fn a_hand_starts_reorders_or_cancels_but_never_closes() {
    let s = setup();
    let a = new_ticket(&s, "A");
    let b = new_ticket(&s, "B");
    // Within Backlog: only the order.
    run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: b, column_id: column(&s, ColumnKind::Backlog), before_ticket_id: Some(a) })).unwrap();
    assert!(view(&s, b).position < view(&s, a).position);
    // To Todo is Start (MU1-09 AC2).
    let moved = run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a, column_id: column(&s, ColumnKind::Todo), before_ticket_id: None })).unwrap();
    assert!(matches!(moved, CommandResult::TicketChanged { column: ColumnKind::Todo, .. }));
    assert!(view(&s, a).started_at_ms.is_some());
    assert_eq!(kinds(&s, a), ["ticket.created", "ticket.moved", "ticket.started", "ticket.updated"], "B5: no dispatcher, choose an owner");
    // Never to Done, never Review → Doing by hand.
    for kind in [ColumnKind::Done, ColumnKind::Doing, ColumnKind::Review, ColumnKind::PlanReview] {
        let err = run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a, column_id: column(&s, kind), before_ticket_id: None })).unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation, "{kind:?}");
    }
    engine_puts(&s, a, ColumnKind::Review);
    let err = run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a, column_id: column(&s, ColumnKind::Doing), before_ticket_id: None })).unwrap_err();
    assert!(err.message.contains("Send it back"), "{}", err.message);
    let err = run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a, column_id: column(&s, ColumnKind::Done), before_ticket_id: None })).unwrap_err();
    assert!(err.message.contains("Accept"), "{}", err.message);
    // To Backlog is Cancel.
    run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a, column_id: column(&s, ColumnKind::Backlog), before_ticket_id: None })).unwrap();
    assert_eq!(view(&s, a).column, ColumnKind::Backlog);
    assert_eq!(kinds(&s, a).last().map(String::as_str), Some("ticket.cancelled"));
    let err = run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a, column_id: aurelius_domain::BoardColumnId::new(), before_ticket_id: None })).unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn start_and_assign_choose_the_owner_before_the_work_begins() {
    let s = setup();
    let forge = employee(&s, "Forge", "active");
    let gone = employee(&s, "Gone", "archived");
    let id = new_ticket(&s, "Owned");
    let err = run(&s, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: Some(gone) })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation, "an archived employee owns nothing");
    run(&s, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: Some(forge) })).unwrap();
    let t = view(&s, id);
    assert_eq!((t.column, t.owner_employee_id, t.owner_name.as_deref()), (ColumnKind::Todo, Some(forge), Some("Forge")));
    assert_eq!(kinds(&s, id), ["ticket.created", "ticket.moved", "ticket.started", "ticket.dispatched"]);
    let err = run(&s, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: None })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation, "started once");
    // With an owner the main task goes to that employee at once (MU1-12 AC1).
    let t = view(&s, id);
    let root = t.root_task_id.expect("the engine made the main task");
    assert_eq!(t.root_task_status, Some(TaskStatus::Queued));
    assert_eq!(task_owner(&s, root), Some(forge));
    // In Todo while it waits for the dispatcher: a board member chooses (MU1-12 AC2).
    let waits = new_ticket(&s, "Waits");
    run(&s, Command::StartTicket(ticket::StartTicket { ticket_id: waits, employee_id: None })).unwrap();
    assert_eq!(view(&s, waits).root_task_status, Some(TaskStatus::Draft), "the draft waits for an owner");
    let orion = employee(&s, "Orion", "active");
    run(&s, Command::AssignTicket(ticket::AssignTicket { ticket_id: waits, employee_id: Some(orion), reason: Some("lead".into()) })).unwrap();
    let t = view(&s, waits);
    assert_eq!((t.owner_employee_id, t.root_task_status), (Some(orion), Some(TaskStatus::Queued)));
    assert_eq!(task_owner(&s, t.root_task_id.unwrap()), Some(orion));
    let err = run(&s, Command::AssignTicket(ticket::AssignTicket { ticket_id: waits, employee_id: Some(forge), reason: None })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation, "not once the work began");
}

#[test]
fn plan_review_and_review_are_decided_by_buttons_with_feedback() {
    let s = setup();
    let forge = employee(&s, "Forge", "active");
    let id = new_ticket(&s, "Planned");
    let err = run(&s, Command::ApprovePlan(ticket::ApprovePlan { ticket_id: id, decision: ticket::PlanApproval::Approve, feedback: None })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation, "only in Plan review");
    run(&s, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: Some(forge) })).unwrap();
    engine_puts(&s, id, ColumnKind::PlanReview);
    root_is(&s, id, TaskStatus::Waiting, Some("plan_review"));
    let err = run(&s, Command::RevisePlan(ticket::RevisePlan { confirm_secret: false, ticket_id: id, feedback: "  ".into() })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation, "a new plan needs feedback");
    run(&s, Command::RevisePlan(ticket::RevisePlan { confirm_secret: false, ticket_id: id, feedback: "Smaller steps".into() })).unwrap();
    assert_eq!((view(&s, id).column, view(&s, id).review_feedback.as_deref()), (ColumnKind::Doing, Some("Smaller steps")));
    engine_puts(&s, id, ColumnKind::PlanReview);
    root_is(&s, id, TaskStatus::Waiting, Some("plan_review"));
    run(&s, Command::ApprovePlan(ticket::ApprovePlan { ticket_id: id, decision: ticket::PlanApproval::ApproveEdits, feedback: None })).unwrap();
    assert_eq!(view(&s, id).column, ColumnKind::Doing);

    // Review: send back needs feedback (MU1-16 AC3), accept closes.
    engine_puts(&s, id, ColumnKind::Review);
    root_is(&s, id, TaskStatus::Review, None);
    let err = run(&s, Command::SendBackTicket(ticket::SendBackTicket { confirm_secret: false, ticket_id: id, feedback: "".into() })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation);
    run(&s, Command::SendBackTicket(ticket::SendBackTicket { confirm_secret: false, ticket_id: id, feedback: "Add a test".into() })).unwrap();
    let t = view(&s, id);
    assert_eq!((t.column, t.review_feedback.as_deref()), (ColumnKind::Doing, Some("Add a test")));
    let err = run(&s, Command::AcceptTicket(ticket::AcceptTicket { confirm_secret: false, ticket_id: id, note: None })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation, "only from Review");
    engine_puts(&s, id, ColumnKind::Review);
    root_is(&s, id, TaskStatus::Review, None);
    run(&s, Command::AcceptTicket(ticket::AcceptTicket { confirm_secret: false, ticket_id: id, note: Some("Fine".into()) })).unwrap();
    let t = view(&s, id);
    assert_eq!(t.column, ColumnKind::Done);
    assert!(t.done_at_ms.is_some());
    let k = kinds(&s, id);
    for kind in ["ticket.plan_revised", "ticket.plan_approved", "ticket.sent_back", "ticket.accepted"] {
        assert!(k.iter().any(|e| e == kind), "{kind} in {k:?}");
    }
    // The feedback of each decision is the board member's own message in the discussion (MU2-05 AC3).
    let discussion = t.conversation_id.expect("a discussion");
    let st = s.service.store().clone();
    let said: Vec<(String, Option<UserId>)> = s
        .service
        .store()
        .read(move |c| aurelius_storage::conversations::list_messages(c, discussion, &|k| st.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok())))
        .unwrap()
        .into_iter()
        .filter(|m| m.role == aurelius_domain::conversation::MessageRole::User)
        .map(|m| (m.text, m.author_user.map(|u| u.id)))
        .collect();
    assert_eq!(said, [("Smaller steps".to_string(), Some(s.user)), ("Add a test".to_string(), Some(s.user))], "no message for an approval without feedback");
    // Done is read-only: no update, no move, no cancel.
    let err = run(&s, Command::UpdateTicket(ticket::UpdateTicket { confirm_secret: false, ticket_id: id, title: Some("New".into()), description: None, priority: None, labels: None, deadline_ms: None, clear_deadline: false, run: None })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation);
    assert!(run(&s, Command::CancelTicket(ticket::CancelTicket { ticket_id: id })).is_err());
    // But it can leave the board.
    run(&s, Command::ArchiveTicket(ticket::ArchiveTicket { ticket_id: id })).unwrap();
    assert!(view(&s, id).archived_at_ms.is_some());
    assert!(run(&s, Command::ArchiveTicket(ticket::ArchiveTicket { ticket_id: id })).is_err(), "archived is final");
}

#[test]
fn update_changes_fields_but_never_the_author_and_checks_the_version() {
    let s = setup();
    let id = new_ticket(&s, "Draft");
    let update = |title: Option<&str>, priority: Option<TicketPriority>| ticket::UpdateTicket {
        confirm_secret: false,
        ticket_id: id,
        title: title.map(str::to_owned),
        description: Some("".into()),
        priority,
        labels: Some(vec!["api".into(), "API".into()]),
        deadline_ms: Some(42),
        clear_deadline: false,
        run: None,
    };
    let err = run_as(&s, Some(s.user), Command::UpdateTicket(update(Some("Final"), None)), Some(7)).unwrap_err();
    assert_eq!(err.code, ErrorCode::VersionConflict);
    let changed = run_as(&s, Some(s.user), Command::UpdateTicket(update(Some("Final"), Some(TicketPriority::Low))), Some(1)).unwrap();
    assert!(matches!(changed, CommandResult::TicketChanged { version: 2, .. }));
    let t = view(&s, id);
    assert_eq!((t.title.as_str(), t.priority, t.description, t.labels, t.deadline_ms), ("Final", TicketPriority::Low, None, vec!["api".to_owned()], Some(42)));
    assert_eq!(t.created_by_user, s.user);
    // Another board member edits; the author stays.
    let other = UserId::new();
    s.service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &other.to_string(), username: "ada", created_by_user: None, now_ms: 2 }, PHC))
        .unwrap();
    run_as(&s, Some(other), Command::UpdateTicket(update(None, Some(TicketPriority::High))), None).unwrap();
    assert_eq!(view(&s, id).created_by_user, s.user);
    let nothing = ticket::UpdateTicket { confirm_secret: false, ticket_id: id, title: None, description: None, priority: None, labels: None, deadline_ms: None, clear_deadline: false, run: None };
    assert_eq!(run(&s, Command::UpdateTicket(nothing)).unwrap_err().code, ErrorCode::Validation);
}

#[test]
fn columns_can_only_be_renamed_and_reordered() {
    let s = setup();
    let columns = s.service.store().read(company_store::columns).unwrap();
    let mut edits: Vec<ticket::BoardColumnEdit> = columns.iter().map(|c| ticket::BoardColumnEdit { column_id: c.id, name: c.name.clone() }).collect();
    edits.swap(4, 5);
    edits[0].name = "Ideas".into();
    run(&s, Command::UpdateBoardColumns(ticket::UpdateBoardColumns { columns: edits.clone() })).unwrap();
    let after = s.service.store().read(company_store::columns).unwrap();
    assert_eq!(after.iter().map(|c| c.kind).collect::<Vec<_>>(), [ColumnKind::Backlog, ColumnKind::Todo, ColumnKind::Doing, ColumnKind::PlanReview, ColumnKind::Done, ColumnKind::Review]);
    assert_eq!(after[0].name, "Ideas");
    edits.pop();
    assert_eq!(run(&s, Command::UpdateBoardColumns(ticket::UpdateBoardColumns { columns: edits })).unwrap_err().code, ErrorCode::Validation, "B3: none dropped");
    // A renamed column keeps its function (MU1-06 AC4): Start still lands in the Todo kind.
    let id = new_ticket(&s, "Still works");
    run(&s, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: None })).unwrap();
    assert_eq!(view(&s, id).column, ColumnKind::Todo);
}

#[test]
fn the_dispatcher_is_set_on_the_company_and_per_project() {
    let s = setup();
    let orion = employee(&s, "Orion", "active");
    let forge = employee(&s, "Forge", "active");
    let gone = employee(&s, "Gone", "archived");
    let set = |name: Option<&str>, who: Option<Option<EmployeeId>>| Command::UpdateCompany(UpdateCompany { name: name.map(str::to_owned), dispatcher: who.map(|employee_id| SetDispatcher { employee_id }), tester: None });
    assert_eq!(run(&s, set(None, Some(Some(gone)))).unwrap_err().code, ErrorCode::Validation);
    assert!(matches!(run(&s, set(Some("Acme"), Some(Some(orion)))).unwrap(), CommandResult::CompanyUpdated { version: 2 }));
    let company = s.service.store().read(company_store::get).unwrap();
    assert_eq!((company.name.as_str(), company.dispatcher_employee_id), ("Acme", Some(orion)));
    assert_eq!(run(&s, set(None, None)).unwrap_err().code, ErrorCode::Validation, "nothing to change");

    // project.update sets a dispatcher too, so it needs a board member as well (m3).
    let project = s.project;
    let update = |who: Option<EmployeeId>| Command::UpdateProject(UpdateProject { project_id: project, dispatcher: Some(SetDispatcher { employee_id: who }) });
    assert_eq!(run_as(&s, None, update(Some(forge)), None).unwrap_err().code, ErrorCode::BoardMemberRequired);
    let done = run(&s, update(Some(forge))).unwrap();
    assert!(matches!(done, CommandResult::ProjectUpdated { version: 2, .. }), "{done:?}");
    assert_eq!(s.service.store().read(|c| company_store::dispatcher_for(c, project)).unwrap(), Some(forge));
    run(&s, update(None)).unwrap();
    assert_eq!(s.service.store().read(|c| company_store::dispatcher_for(c, project)).unwrap(), Some(orion), "MU1-11 AC2: back to the company's");
    let kinds: Vec<String> = s
        .service
        .store()
        .read(|c| {
            let mut stmt = c.prepare("SELECT type FROM events WHERE aggregate_type IN ('company','project') ORDER BY seq")?;
            let rows = stmt.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    assert_eq!(kinds, ["project.created", "company.updated", "project.updated", "project.updated"]);
    let by: String = s
        .service
        .store()
        .read(|c| Ok(c.query_row("SELECT metadata_json FROM events WHERE type = 'project.updated' ORDER BY seq LIMIT 1", [], |r| r.get(0))?))
        .unwrap();
    let by: serde_json::Value = serde_json::from_str(&by).unwrap();
    assert_eq!(by["by"], serde_json::json!({ "kind": "board_member", "user_id": s.user }));
}

#[test]
fn a_tester_is_refused_until_the_testing_step_is_built_and_changes_nothing() {
    let s = setup();
    let pulse = employee(&s, "Pulse", "active");
    let set = |name: Option<&str>| Command::UpdateCompany(UpdateCompany { name: name.map(str::to_owned), dispatcher: None, tester: Some(aurelius_domain::company::SetTester { employee_id: Some(pulse) }) });
    for command in [set(None), set(Some("Acme"))] {
        let refused = run(&s, command).unwrap_err();
        assert_eq!((refused.code, refused.message.as_str()), (ErrorCode::Validation, crate::ticket::TESTER_NOT_YET));
    }
    let company = s.service.store().read(company_store::get).unwrap();
    assert_eq!((company.version, company.tester_employee_id), (1, None));
    assert_ne!(company.name, "Acme");
}

#[test]
fn accept_waits_for_linked_tasks_that_still_work() {
    let s = setup();
    let id = new_ticket(&s, "Busy");
    let project = s.project;
    s.service
        .store()
        .write(move |tx| {
            let blob = aurelius_domain::BlobId::new().to_string();
            tx.execute("INSERT INTO blobs(id, sha256, media_type, byte_length, storage_key, created_at_ms) VALUES (?1, ?1, 'text/plain', 1, ?1, 1)", [&blob])?;
            let (root, part) = (aurelius_domain::TaskId::new(), aurelius_domain::TaskId::new());
            aurelius_storage::tasks::insert_ticket_task(tx, root, project, None, "Main", &blob, &blob, aurelius_domain::task::TaskStatus::Review, None, Some(id))?;
            aurelius_storage::tasks::insert_task(tx, part, project, None, "Part", &blob, &blob, aurelius_domain::task::TaskStatus::Running, Some(root))?;
            ticket_store::set_root_task(tx, id, Some(root))
        })
        .unwrap();
    engine_puts(&s, id, ColumnKind::Review);
    let err = run(&s, Command::AcceptTicket(ticket::AcceptTicket { confirm_secret: false, ticket_id: id, note: None })).unwrap_err();
    assert!(err.message.contains("still work"), "B6: {}", err.message);
}

fn ticket_in(s: &Setup, project: ProjectId, title: &str, owner: Option<EmployeeId>, start: bool) -> TicketId {
    let created = run(
        s,
        Command::CreateTicket(ticket::CreateTicket { confirm_secret: false, project_id: project, title: title.into(), description: None, priority: Default::default(), labels: vec![], deadline_ms: None, owner_employee_id: owner, run: Default::default(), files: vec![], start, from_message: None }),
    )
    .unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = created else { panic!("{created:?}") };
    ticket_id
}

/// The titles of one column in board order, as `board.get` shows them for `project` (none: all).
fn column_titles(s: &Setup, kind: ColumnKind, project: Option<ProjectId>) -> Vec<String> {
    let store = s.service.store().clone();
    let filter = ticket_store::TicketFilter { project, column: Some(kind), ..Default::default() };
    store.read(|c| ticket_store::list(c, &filter, &|_| None)).unwrap().into_iter().map(|t| t.title).collect()
}

#[test]
fn a_move_lands_above_its_neighbour_also_on_a_board_filtered_by_project() {
    let s = setup();
    let created = run(&s, Command::CreateProject(CreateProject { name: "Hermes".into(), description: None, folder: None })).unwrap();
    let CommandResult::ProjectCreated { project_id: hermes, .. } = created else { panic!() };
    // Backlog, two projects mixed: A1 H1 A2 H2 A3.
    let a1 = ticket_in(&s, s.project, "A1", None, false);
    ticket_in(&s, hermes, "H1", None, false);
    let a2 = ticket_in(&s, s.project, "A2", None, false);
    ticket_in(&s, hermes, "H2", None, false);
    let a3 = ticket_in(&s, s.project, "A3", None, false);
    let backlog = column(&s, ColumnKind::Backlog);
    assert_eq!(column_titles(&s, ColumnKind::Backlog, Some(s.project)), ["A1", "A2", "A3"]);

    // Filtered on Atlas: drag A3 between A1 and A2, so right above A2.
    run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a3, column_id: backlog, before_ticket_id: Some(a2) })).unwrap();
    assert_eq!(column_titles(&s, ColumnKind::Backlog, Some(s.project)), ["A1", "A3", "A2"]);
    assert_eq!(column_titles(&s, ColumnKind::Backlog, None), ["A1", "H1", "A3", "A2", "H2"], "the other project keeps its places");
    // To the top of the filtered view: above A1.
    run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a2, column_id: backlog, before_ticket_id: Some(a1) })).unwrap();
    assert_eq!(column_titles(&s, ColumnKind::Backlog, Some(s.project)), ["A2", "A1", "A3"]);
    // None: at the bottom.
    run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a2, column_id: backlog, before_ticket_id: None })).unwrap();
    assert_eq!(column_titles(&s, ColumnKind::Backlog, None), ["A1", "H1", "A3", "H2", "A2"]);
    // Above a card of another column: refused, nothing moves.
    let todo = column(&s, ColumnKind::Todo);
    let err = run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a1, column_id: todo, before_ticket_id: Some(a3) })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation);
    assert_eq!(view(&s, a1).column, ColumnKind::Backlog);
    // Into Todo above a card there.
    run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a1, column_id: todo, before_ticket_id: None })).unwrap();
    run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a3, column_id: todo, before_ticket_id: Some(a1) })).unwrap();
    assert_eq!(column_titles(&s, ColumnKind::Todo, Some(s.project)), ["A3", "A1"]);
}

#[test]
fn every_choice_of_an_owner_is_dispatched_and_update_has_no_owner() {
    let s = setup();
    let forge = employee(&s, "Forge", "active");
    let kept = ticket_in(&s, s.project, "Kept", Some(forge), false);
    assert_eq!(kinds(&s, kept), ["ticket.created", "ticket.dispatched"]);
    let now = ticket_in(&s, s.project, "Now", Some(forge), true);
    assert_eq!(kinds(&s, now), ["ticket.created", "ticket.moved", "ticket.started", "ticket.dispatched"]);
    assert_eq!(view(&s, now).owner_employee_id, Some(forge));
    // The owner is not a field of ticket.update any more (only ticket.assign): refused, not ignored (QA-m3).
    let old = serde_json::json!({ "ticket_id": kept, "owner_employee_id": forge });
    assert!(serde_json::from_value::<ticket::UpdateTicket>(old).unwrap_err().to_string().contains("unknown field `owner_employee_id`"));
}

#[test]
fn assign_none_leaves_the_owner_to_the_dispatcher_again_but_only_in_backlog() {
    let s = setup();
    let forge = employee(&s, "Forge", "active");
    let id = ticket_in(&s, s.project, "Mine", Some(forge), false);
    let before = view(&s, id).version;
    let back = run(&s, Command::AssignTicket(ticket::AssignTicket { ticket_id: id, employee_id: None, reason: Some("let Orion choose".into()) })).unwrap();
    let t = view(&s, id);
    assert_eq!((t.owner_employee_id, t.owner_name), (None, None));
    assert_eq!(back, CommandResult::TicketChanged { ticket_id: id, column: ColumnKind::Backlog, version: before + 1 });
    assert_eq!(kinds(&s, id), ["ticket.created", "ticket.dispatched", "ticket.updated"], "no ticket.dispatched: nobody was chosen");
    let meta: serde_json::Value = s
        .service
        .store()
        .read(move |c| Ok(c.query_row("SELECT metadata_json FROM events WHERE type = 'ticket.updated' AND aggregate_id = ?1", [id.to_string()], |r| r.get::<_, String>(0))?))
        .map(|m| serde_json::from_str(&m).unwrap())
        .unwrap();
    assert_eq!((meta["changed"].clone(), meta["reason"].clone()), (serde_json::json!(["owner"]), serde_json::json!("let Orion choose")));
    // Once started (Todo), the dispatcher may already be choosing: too late.
    let started = ticket_in(&s, s.project, "Started", Some(forge), true);
    let err = run(&s, Command::AssignTicket(ticket::AssignTicket { ticket_id: started, employee_id: None, reason: None })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation);
    assert_eq!(view(&s, started).owner_employee_id, Some(forge));
}

#[test]
fn a_card_put_above_itself_stays_where_it_is() {
    let s = setup();
    let a = ticket_in(&s, s.project, "A", None, false);
    let b = ticket_in(&s, s.project, "B", None, false);
    let (backlog, todo) = (column(&s, ColumnKind::Backlog), column(&s, ColumnKind::Todo));
    let version = view(&s, b).version;
    let same = run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: b, column_id: backlog, before_ticket_id: Some(b) })).unwrap();
    assert_eq!(same, CommandResult::TicketChanged { ticket_id: b, column: ColumnKind::Backlog, version });
    assert_eq!(column_titles(&s, ColumnKind::Backlog, None), ["A", "B"]);
    assert_eq!(kinds(&s, b), ["ticket.created"], "no event");
    // Above itself in another column: a message about that, not "refresh".
    let err = run(&s, Command::MoveTicket(ticket::MoveTicket { ticket_id: a, column_id: todo, before_ticket_id: Some(a) })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation);
    assert!(err.message.contains("above itself"), "{}", err.message);
    assert_eq!(view(&s, a).column, ColumnKind::Backlog);
}

#[test]
fn a_deadline_must_be_a_real_moment() {
    let s = setup();
    let mut create = ticket::CreateTicket {
        confirm_secret: false,
        project_id: s.project,
        title: "Late".into(),
        description: None,
        priority: Default::default(),
        labels: vec![],
        deadline_ms: Some(-5),
        owner_employee_id: None,
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: None,
    };
    assert_eq!(run(&s, Command::CreateTicket(create.clone())).unwrap_err().code, ErrorCode::Validation);
    create.deadline_ms = Some(1_800_000_000_000);
    let CommandResult::TicketCreated { ticket_id: id, .. } = run(&s, Command::CreateTicket(create)).unwrap() else { panic!() };
    let update = |deadline_ms| ticket::UpdateTicket { confirm_secret: false, ticket_id: id, title: None, description: None, priority: None, labels: None, deadline_ms, clear_deadline: false, run: None };
    for bad in [0, -1, ticket::DEADLINE_MAX_MS + 1] {
        assert_eq!(run(&s, Command::UpdateTicket(update(Some(bad)))).unwrap_err().code, ErrorCode::Validation, "{bad}");
    }
    assert_eq!(view(&s, id).deadline_ms, Some(1_800_000_000_000));
}

#[test]
fn columns_update_checks_the_board_version_and_records_names_and_order() {
    let s = setup();
    let columns = s.service.store().read(company_store::columns).unwrap();
    let version = ticket::board_version(&columns);
    let mut edits: Vec<ticket::BoardColumnEdit> = columns.iter().map(|c| ticket::BoardColumnEdit { column_id: c.id, name: c.name.clone() }).collect();
    edits[0].name = "Ideas".into();
    let update = |edits: &Vec<ticket::BoardColumnEdit>, expected| run_as(&s, Some(s.user), Command::UpdateBoardColumns(ticket::UpdateBoardColumns { columns: edits.clone() }), expected);
    let done = update(&edits, Some(version)).unwrap();
    assert_eq!(done, CommandResult::BoardColumnsUpdated { version: version + 1 });
    // Another board member with the old version: refused, nothing overwritten (QA-m1).
    edits[0].name = "Inbox".into();
    let err = update(&edits, Some(version)).unwrap_err();
    assert_eq!(err.code, ErrorCode::VersionConflict);
    assert_eq!(s.service.store().read(company_store::columns).unwrap()[0].name, "Ideas");
    edits.swap(4, 5);
    assert_eq!(update(&edits, None).unwrap(), CommandResult::BoardColumnsUpdated { version: version + 2 }, "no version: last one wins");
    let meta: Vec<serde_json::Value> = s
        .service
        .store()
        .read(|c| {
            let mut stmt = c.prepare("SELECT metadata_json FROM events WHERE type = 'board.columns_updated' ORDER BY seq")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
        .into_iter()
        .map(|m| serde_json::from_str(&m).unwrap())
        .collect();
    let last = &meta[1]["columns"];
    let names: Vec<&str> = last.as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Inbox", "Todo", "Doing", "Plan review", "Done", "Review"]);
    assert_eq!((last[4]["kind"].clone(), last[4]["position"].clone()), (serde_json::json!("done"), serde_json::json!(4)));
}

#[test]
fn after_a_cancel_the_owner_can_be_chosen_again_in_backlog() {
    let s = setup();
    let forge = employee(&s, "Forge", "active");
    let orion = employee(&s, "Orion", "active");
    let id = ticket_in(&s, s.project, "Again", Some(forge), true);
    let project = s.project;
    // The engine made a main task; the cancel put it to Cancelled and the card in Backlog.
    s.service
        .store()
        .write(move |tx| {
            let blob = aurelius_domain::BlobId::new().to_string();
            tx.execute("INSERT INTO blobs(id, sha256, media_type, byte_length, storage_key, created_at_ms) VALUES (?1, ?1, 'text/plain', 1, ?1, 1)", [&blob])?;
            let root = aurelius_domain::TaskId::new();
            aurelius_storage::tasks::insert_ticket_task(tx, root, project, None, "Main", &blob, &blob, aurelius_domain::task::TaskStatus::Cancelled, None, Some(id))?;
            ticket_store::set_root_task(tx, id, Some(root))
        })
        .unwrap();
    run(&s, Command::CancelTicket(ticket::CancelTicket { ticket_id: id })).unwrap();
    assert_eq!(view(&s, id).column, ColumnKind::Backlog);
    run(&s, Command::AssignTicket(ticket::AssignTicket { ticket_id: id, employee_id: Some(orion), reason: None })).unwrap();
    assert_eq!(view(&s, id).owner_employee_id, Some(orion));
    assert_eq!(kinds(&s, id).last().map(String::as_str), Some("ticket.dispatched"));
}

#[test]
fn the_owner_follows_a_hand_over_of_the_main_task_and_the_column_stays() {
    let s = setup();
    let forge = employee(&s, "Forge", "active");
    let orion = employee(&s, "Orion", "active");
    let id = new_ticket(&s, "Handed");
    run(&s, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: Some(forge) })).unwrap();
    let root = view(&s, id).root_task_id.unwrap();
    let before = view(&s, id);
    let notices = s.service.store().write(move |tx| crate::task::task_line::set_employee(tx, root, Some(orion))).unwrap().into_vec();
    assert_eq!(notices.len(), 2, "one ticket.updated and its system line (MU2-03)");
    let after = view(&s, id);
    assert_eq!((after.owner_employee_id, after.column), (Some(orion), before.column));
    assert_eq!(after.version, before.version + 1);
    assert_eq!(kinds(&s, id).last().map(String::as_str), Some("ticket.updated"));
    // A delegation (a task with a parent) never moves or changes the card.
    let child = aurelius_domain::TaskId::new();
    let project = s.project;
    s.service
        .store()
        .write(move |tx| {
            let blob: String = tx.query_row("SELECT brief_blob_id FROM tasks WHERE id = ?1", [root.to_string()], |r| r.get(0))?;
            aurelius_storage::tasks::insert_task(tx, child, project, Some(forge), "Part", &blob, &blob, TaskStatus::Queued, Some(root))?;
            let mut moved = crate::task::task_line::set_status(tx, child, TaskStatus::Running, None)?.into_vec();
            moved.extend(crate::task::task_line::set_employee(tx, child, Some(orion))?);
            Ok(moved)
        })
        .unwrap();
    let card = view(&s, id);
    assert_eq!((card.column, card.version, card.owner_employee_id), (after.column, after.version, Some(orion)));
}

/// The clients' B5 rule (mobile `dispatcherGaveUp`, desktop C1c): in the
/// ticket's Activity, newest first, a system `ticket.updated` (needs owner)
/// counts until a later start, choice of an owner or cancel.
fn dispatcher_gave_up(s: &Setup, id: TicketId) -> bool {
    let (cards, _) = s.service.store().read(|c| aurelius_storage::timeline::cards(c, None, Some(&id.to_string()), None, 100, true)).unwrap();
    for card in cards {
        match card.title.as_str() {
            "ticket.started" | "ticket.dispatched" | "ticket.cancelled" => return false,
            "ticket.updated" if card.actor == "system" => return true,
            _ => {}
        }
    }
    false
}

#[test]
fn b5_sees_a_new_start_after_a_cancel_in_the_tickets_activity() {
    let s = setup();
    let id = new_ticket(&s, "Again");
    run(&s, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: None })).unwrap();
    assert!(dispatcher_gave_up(&s, id), "no dispatcher: choose an owner");
    run(&s, Command::CancelTicket(ticket::CancelTicket { ticket_id: id })).unwrap();
    assert!(!dispatcher_gave_up(&s, id), "back in Backlog");
    let orion = employee(&s, "Orion", "active");
    run(&s, Command::UpdateCompany(UpdateCompany { name: None, dispatcher: Some(SetDispatcher { employee_id: Some(orion) }), tester: None })).unwrap();
    run(&s, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: None })).unwrap();
    assert_eq!(view(&s, id).column, ColumnKind::Todo);
    assert!(!dispatcher_gave_up(&s, id), "the new start is newer than the old needs-owner");
    let (cards, _) = s.service.store().read(|c| aurelius_storage::timeline::cards(c, None, Some(&id.to_string()), None, 100, true)).unwrap();
    let board: Vec<&str> = cards.iter().rev().map(|c| c.title.as_str()).filter(|t| t.starts_with("ticket.")).collect();
    assert_eq!(board, ["ticket.created", "ticket.moved", "ticket.started", "ticket.updated", "ticket.moved", "ticket.cancelled", "ticket.moved", "ticket.started"], "every board write, in order");
}

#[test]
fn a_dispatchers_run_left_open_ends_when_its_job_is_claimed_again() {
    // Trace M3 m2: the daemon stopped during the dispatcher's run; claimed again, that run is over.
    let s = setup();
    let orion = employee(&s, "Orion", "active");
    run(&s, Command::UpdateCompany(UpdateCompany { name: None, dispatcher: Some(SetDispatcher { employee_id: Some(orion) }), tester: None })).unwrap();
    let id = new_ticket(&s, "Interrupted");
    run(&s, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: None })).unwrap();
    let root = view(&s, id).root_task_id.unwrap();
    let (open, done) = (aurelius_domain::RunId::new(), aurelius_domain::RunId::new());
    s.service
        .store()
        .write(move |tx| {
            for (run, status) in [(open, "running"), (done, "succeeded")] {
                tx.execute(
                    "INSERT INTO runs(id, task_id, employee_id, attempt, runtime_kind, model_id, resolved_config_json, status, started_at_ms)
                     VALUES (?1, ?2, ?3, 1, 'fake', '', '{}', ?4, 1)",
                    rusqlite::params![run.to_string(), root.to_string(), orion.to_string(), status],
                )?;
            }
            Ok(())
        })
        .unwrap();
    s.service.triage_claimed(root).unwrap();
    let state = |run: aurelius_domain::RunId| {
        s.service
            .store()
            .read(move |c| Ok(c.query_row("SELECT status, error_code, ended_at_ms IS NOT NULL FROM runs WHERE id = ?1", [run.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, bool>(2)?)))?))
            .unwrap()
    };
    assert_eq!(state(open), ("failed".to_string(), Some("interrupted".to_string()), true));
    assert_eq!(state(done), ("succeeded".to_string(), None, false), "an ended run stays as it was");
}

#[test]
fn the_task_job_returns_the_cards_move_with_its_own() {
    // Trace M3: a step of the task job (here: its owner is paused, Queued → Waiting)
    // moves the card Todo → Doing; that ticket.moved is among the notices it publishes.
    let s = setup();
    let forge = employee(&s, "Forge", "active");
    let id = new_ticket(&s, "Paused owner");
    run(&s, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: Some(forge) })).unwrap();
    let root = view(&s, id).root_task_id.unwrap();
    assert_eq!(view(&s, id).column, ColumnKind::Todo);
    let job = s
        .service
        .store()
        .write(|tx| aurelius_storage::queries::claim_due_job(tx, "test", aurelius_storage::now_ms() + 60_000, 45_000, 4))
        .unwrap()
        .expect("the task's job");
    assert_eq!(job.id, root.to_string());
    let ctx = s.service.task_context(root).unwrap().unwrap();
    let notices = s.service.task_transition(&ctx, job.lease_epoch, TaskStatus::Waiting, Some("paused"), "task.waiting").unwrap();
    let kinds: Vec<_> = notices.iter().map(|n| (n.kind, n.aggregate_id.clone())).collect();
    assert!(kinds.contains(&(aurelius_domain::EventKind::TicketMoved, id.to_string())), "{kinds:?}");
    assert!(kinds.contains(&(aurelius_domain::EventKind::TaskWaiting, root.to_string())), "{kinds:?}");
    assert_eq!(view(&s, id).column, ColumnKind::Doing);
}
