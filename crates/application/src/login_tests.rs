//! Whose Claude login a run uses (MU3-10, `run::login`): a ticket's runs and
//! its delegations at any depth on the ticket's login, a chat on its writer's,
//! woken runs on hop 0's, B5/B6, and the login kept in the run's snapshot.

use super::*;
use crate::task::task_runs::EmployeeAction;
use aurelius_domain::employee::{CreateEmployee, SendEmployeeMessage};
use aurelius_domain::group_chat::SendGroupMessage;
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::{CreateTask, TaskStatus};
use aurelius_domain::ticket::{self, StartTicket};
use aurelius_domain::{ConversationId, EmployeeId, MessageId, RunId, TaskId, TicketId, UserId};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    project: ProjectId,
    alex: UserId,
    robin: UserId,
    ada: (EmployeeId, ConversationId),
    iris: (EmployeeId, ConversationId),
    otto: (EmployeeId, ConversationId),
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app: None, user_id: user }
}

fn exec(s: &Setup, user: UserId, command: Command) -> CommandResult {
    s.service.execute(&ctx(Some(user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }).unwrap().receipt.result
}

fn user(service: &CommandService, name: &str) -> UserId {
    let (id, name) = (UserId::new(), name.to_owned());
    service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &id.to_string(), username: &name, created_by_user: None, now_ms: 1 }, PHC))
        .unwrap();
    id
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    let (alex, robin) = (user(&service, "alex"), user(&service, "robin"));
    let run = |command| service.execute(&ctx(Some(alex)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }).unwrap().receipt.result;
    run(Command::SetFollowOrgChart(aurelius_domain::employee::SetFollowOrgChart { follow: false }));
    let CommandResult::ProjectCreated { project_id, .. } = run(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None })) else { panic!() };
    let hire = |name: &str| {
        let create = CreateEmployee { name: name.into(), role: "Dev".into(), title: None, reports_to: None, instructions: None, folder: None };
        let CommandResult::EmployeeCreated { employee_id, conversation_id } = run(Command::CreateEmployee(create)) else { panic!() };
        (employee_id, conversation_id)
    };
    let (ada, iris, otto) = (hire("Ada"), hire("Iris"), hire("Otto"));
    let shared = aurelius_domain::ticket::CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id,
        title: "Shared work".into(),
        description: None,
        priority: Default::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: None,
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: None,
        from_idea: None,
    };
    run(Command::CreateTicket(shared));
    Setup { _dir: dir, service, project: project_id, alex, robin, ada, iris, otto }
}

/// The login of `run`: in `runs.login_user` and in its snapshot, which must agree.
fn login_of(s: &Setup, run: RunId) -> Option<UserId> {
    let (column, snapshot): (Option<String>, String) = s
        .service
        .store()
        .read(|c| Ok(c.query_row("SELECT login_user, resolved_config_json FROM runs WHERE id = ?1", [run.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .unwrap();
    let snapshot: crate::run::connections::ResolvedRunConfig = serde_json::from_str(&snapshot).unwrap();
    let column = column.map(|c| UserId::parse(&c).unwrap());
    assert_eq!(snapshot.login_user, column, "the snapshot and runs.login_user agree");
    column
}

/// The task's conversation opens with a run, as the task job does.
fn start_task(s: &Setup, task: TaskId, employee: EmployeeId) -> RunId {
    let st = s.service.store().clone();
    s.service
        .store()
        .write(move |tx| {
            let (conversation, brief, _) = crate::task::task_threads::open_returning(tx, &st, task, employee, "brief")?;
            let _ = crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?;
            Ok(crate::run::queue::queue_run(tx, &serde_json::json!({}), &task.to_string(), conversation.unwrap(), brief.unwrap())?.0)
        })
        .unwrap()
}

/// `run` delegates a part to `to`, on ticket #1 when it has no ticket itself; the new task and its first run.
fn delegate(s: &Setup, run: RunId, to: &str, employee: EmployeeId) -> (TaskId, RunId) {
    delegate_on(s, run, to, employee, "#1")
}

fn delegate_on(s: &Setup, run: RunId, to: &str, employee: EmployeeId, on: &str) -> (TaskId, RunId) {
    let action = EmployeeAction::Delegate { employee: to.into(), title: format!("Part for {to}"), goal: "Do the part".into(), criteria: vec![], ticket: Some(on.into()) };
    let (ok, text, _) = s.service.employee_action(run, action).unwrap();
    assert!(ok, "{text}");
    let task: String = s
        .service
        .store()
        .read(move |c| Ok(c.query_row("SELECT id FROM tasks WHERE employee_id = ?1 ORDER BY created_at_ms DESC, rowid DESC LIMIT 1", [employee.to_string()], |r| r.get(0))?))
        .unwrap();
    let task = TaskId::parse(&task).unwrap();
    (task, start_task(s, task, employee))
}

fn ticket_of(s: &Setup, by: UserId, owner: EmployeeId) -> (TicketId, TaskId) {
    let create = ticket::CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id: s.project,
        title: "Login".into(),
        description: None,
        priority: Default::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: Some(owner),
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: None,
        from_idea: None,
    };
    let CommandResult::TicketCreated { ticket_id, .. } = exec(s, by, Command::CreateTicket(create)) else { panic!() };
    exec(s, by, Command::StartTicket(StartTicket { ticket_id, employee_id: None }));
    let root = s.service.store().read(|c| aurelius_storage::tickets::state(c, ticket_id)).unwrap().unwrap().root_task_id.expect("a main task");
    (ticket_id, root)
}

#[test]
fn a_ticket_and_its_delegations_at_every_depth_run_on_the_tickets_login() {
    let s = setup();
    // Robin's ticket, started by Alex: Robin's login (the creator's), not the starter's.
    let (ticket, root) = ticket_of(&s, s.robin, s.ada.0);
    let main = start_task(&s, root, s.ada.0);
    assert_eq!(login_of(&s, main), Some(s.robin));
    // Delegations, three deep.
    let (_, one) = delegate(&s, main, "Iris", s.iris.0);
    let (part, two) = delegate(&s, one, "Otto", s.otto.0);
    // A third level (deeper than the default policy lets an employee go; an org chart can).
    let deep = TaskId::new();
    let otto = s.otto.0;
    let project = s.project;
    s.service
        .store()
        .write(move |tx| {
            let (brief, criteria): (String, String) = tx.query_row("SELECT brief_blob_id, acceptance_blob_id FROM tasks WHERE id = ?1", [part.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?;
            aurelius_storage::tasks::insert_task(tx, deep, project, Some(otto), "Deep part", &brief, &criteria, TaskStatus::Queued, Some(part))
        })
        .unwrap();
    let three = start_task(&s, deep, s.otto.0);
    assert_eq!([one, two, three].map(|r| login_of(&s, r)), [Some(s.robin); 3]);
    // A board member's message in the main task's conversation still runs on the ticket's login.
    let conversation = s.service.store().read(|c| aurelius_storage::tasks::task_conversation(c, root)).unwrap().unwrap().0;
    let send = SendEmployeeMessage { confirm_secret: false, employee_id: s.ada.0, text: "Also this".into(), conversation_id: conversation, images: vec![], files: vec![], interrupt: false };
    let CommandResult::EmployeeMessageSent { run_id, .. } = exec(&s, s.alex, Command::SendEmployeeMessage(send)) else { panic!() };
    assert_eq!(login_of(&s, run_id), Some(s.robin));

    // Taken over on Alex's login (B4): the next runs use it; those already queued keep Robin's (A-02).
    s.service.store().write(move |tx| aurelius_storage::tickets::use_my_login(tx, ticket, s.alex).map(drop)).unwrap();
    let (_, later) = delegate(&s, main, "Iris", s.iris.0);
    assert_eq!(login_of(&s, later), Some(s.alex));
    assert_eq!(login_of(&s, main), Some(s.robin), "a run's login never changes");
}

#[test]
fn a_task_without_a_ticket_runs_on_who_made_it_and_its_parts_too() {
    let s = setup();
    let create = CreateTask { confirm_secret: false, project_id: s.project, employee_id: Some(s.ada.0), title: "Report".into(), goal: "Write it".into(), acceptance_criteria: vec![], depends_on: vec![], run: Default::default(), files: vec![] };
    let CommandResult::TaskCreated { task_id } = exec(&s, s.robin, Command::CreateTask(create)) else { panic!() };
    let run = start_task(&s, task_id, s.ada.0);
    assert_eq!(login_of(&s, run), Some(s.robin));
    // A task without a ticket has none to give its part, so the part names one.
    let part = |ticket: Option<&str>| EmployeeAction::Delegate { employee: "Iris".into(), title: "Part".into(), goal: "Do the part".into(), criteria: vec![], ticket: ticket.map(str::to_owned) };
    let (ok, text, _) = s.service.employee_action(run, part(None)).unwrap();
    assert!(!ok && text == crate::ticket::agent::DELEGATE_NEEDS_TICKET, "{text}");
    // Ticket #1 is Alex's: its work runs on Alex's login, so Robin's run cannot add to it.
    let (ok, text, _) = s.service.employee_action(run, part(Some("#1"))).unwrap();
    assert!(!ok && text == crate::ticket::agent::DELEGATE_OTHER_LOGIN, "{text}");
    let robins = ticket::CreateTicket { responsible_user_id: None, confirm_secret: false, project_id: s.project, title: "Robin's".into(), description: None, priority: Default::default(), labels: vec![], deadline_ms: None, owner_employee_id: None, run: Default::default(), files: vec![], start: false, from_message: None, from_idea: None };
    let CommandResult::TicketCreated { number, .. } = exec(&s, s.robin, Command::CreateTicket(robins)) else { panic!() };
    let (_, part) = delegate_on(&s, run, "Iris", s.iris.0, &format!("#{number}"));
    assert_eq!(login_of(&s, part), Some(s.robin));
}

#[test]
fn a_chat_runs_on_the_login_of_who_wrote_the_message() {
    let s = setup();
    let say = |by: UserId, text: &str| {
        let send = SendEmployeeMessage { confirm_secret: false, employee_id: s.ada.0, text: text.into(), conversation_id: s.ada.1, images: vec![], files: vec![], interrupt: false };
        let CommandResult::EmployeeMessageSent { run_id, .. } = exec(&s, by, Command::SendEmployeeMessage(send)) else { panic!() };
        run_id
    };
    let robins = say(s.robin, "Hi Ada");
    assert_eq!(login_of(&s, robins), Some(s.robin));
    let alexs = say(s.alex, "Hi from Alex");
    assert_eq!(login_of(&s, alexs), Some(s.alex), "the same chat, another writer: the writer's login");
    // A delegation asked from that chat runs on the login of the run that asked.
    let (_, part) = delegate(&s, alexs, "Iris", s.iris.0);
    assert_eq!(login_of(&s, part), Some(s.alex));
}

#[test]
fn in_a_shared_chat_work_given_away_runs_on_the_login_of_the_run_that_gave_it() {
    // R2 M1: Alex's run is running in Ada's chat; Robin writes meanwhile, so a newer run of Robin's waits (created).
    let s = setup();
    let say = |by: UserId, text: &str| {
        let send = SendEmployeeMessage { confirm_secret: false, employee_id: s.ada.0, text: text.into(), conversation_id: s.ada.1, images: vec![], files: vec![], interrupt: false };
        let CommandResult::EmployeeMessageSent { run_id, .. } = exec(&s, by, Command::SendEmployeeMessage(send)) else { panic!() };
        run_id
    };
    let alexs = say(s.alex, "Plan the release");
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'running' WHERE id = ?1", [alexs.to_string()]).map(drop)?)).unwrap();
    let robins = say(s.robin, "And the notes");
    let status: String = s.service.store().read(move |c| Ok(c.query_row("SELECT status FROM runs WHERE id = ?1", [robins.to_string()], |r| r.get(0))?)).unwrap();
    assert_eq!((status.as_str(), login_of(&s, robins)), ("created", Some(s.robin)));
    // Alex's run delegates: the task is on Alex's login, not on the conversation's latest run's.
    let (task, part) = delegate(&s, alexs, "Iris", s.iris.0);
    let own: Option<String> = s.service.store().read(move |c| Ok(c.query_row("SELECT login_user FROM tasks WHERE id = ?1", [task.to_string()], |r| r.get(0))?)).unwrap();
    assert_eq!(own, Some(s.alex.to_string()));
    assert_eq!(login_of(&s, part), Some(s.alex));
    // A summary of that chat runs on the login of the run that scheduled it; without one it does not run.
    let summary = |source: Option<RunId>| s.service.summary_input(s.ada.1, source).unwrap().unwrap().config.login_user;
    assert_eq!(summary(Some(alexs)), Some(s.alex));
    assert_eq!(summary(None), None);
}

#[test]
fn an_escalation_runs_on_the_escalated_tasks_login() {
    // R2 M1: Robin's ticket ran on Robin's login, then Alex took it over (B4): its
    // conversation's latest run is still Robin's, the task's login is Alex's.
    let s = setup();
    exec(&s, s.alex, Command::SetFollowOrgChart(aurelius_domain::employee::SetFollowOrgChart { follow: true }));
    exec(&s, s.alex, Command::SetEmployeeReportsTo(aurelius_domain::employee::SetEmployeeReportsTo { employee_id: s.iris.0, reports_to: Some(s.ada.0) }));
    let (ticket, root) = ticket_of(&s, s.robin, s.iris.0);
    let main = start_task(&s, root, s.iris.0);
    assert_eq!(login_of(&s, main), Some(s.robin));
    s.service.store().write(move |tx| aurelius_storage::tickets::use_my_login(tx, ticket, s.alex).map(drop)).unwrap();
    let st = s.service.store().clone();
    let up = s
        .service
        .store()
        .write(move |tx| {
            let row = aurelius_storage::tasks::load_task(tx, root)?.unwrap();
            crate::org::escalate(tx, &st, root, &row, "it failed", "")
        })
        .unwrap();
    assert!(up.is_some(), "Ada is Iris's boss");
    let login: Option<String> = s.service.store().read(|c| Ok(c.query_row("SELECT login_user FROM tasks WHERE title LIKE 'Escalated:%'", [], |r| r.get(0))?)).unwrap();
    assert_eq!(login, Some(s.alex.to_string()));
}

#[test]
fn runs_the_team_chat_wakes_use_the_login_of_hop_0_at_every_hop() {
    let s = setup();
    let CommandResult::GroupMessageSent { run_ids, .. } = exec(&s, s.robin, Command::SendGroupMessage(SendGroupMessage { confirm_secret: false, text: "@Ada what do you think?".into(), images: vec![], files: vec![], channel: None, thread_parent: None })) else { panic!() };
    let hop0 = run_ids[0];
    assert_eq!(login_of(&s, hop0), Some(s.robin));
    let woken = |employee: EmployeeId| -> RunId {
        s.service
            .store()
            .read(move |c| {
                let chat = aurelius_storage::employees::main_chat(c, employee)?.unwrap();
                let id: String = c.query_row("SELECT id FROM runs WHERE conversation_id = ?1 ORDER BY rowid DESC LIMIT 1", [chat.to_string()], |r| r.get(0))?;
                Ok(RunId::parse(&id).unwrap())
            })
            .unwrap()
    };
    // Ada brings in Iris (hop 1), Iris brings in Otto (hop 2): still Robin's login, never the employee's chat's last one.
    let post = |run: RunId, text: &str| {
        let (ok, said, _) = s.service.employee_action(run, EmployeeAction::PostToGroupChat { message: text.into(), channel: None }).unwrap();
        assert!(ok, "{said}");
    };
    post(hop0, "@Iris please check");
    let hop1 = woken(s.iris.0);
    post(hop1, "@Otto your turn");
    let hop2 = woken(s.otto.0);
    for (run, hop) in [(hop1, 1), (hop2, 2)] {
        let group = s.service.store().read(|c| aurelius_storage::conversations::group_run(c, run)).unwrap().unwrap();
        assert_eq!((group.hop, group.login_user), (hop, Some(s.robin)));
        assert_eq!(login_of(&s, run), Some(s.robin));
    }
}

#[test]
fn schedules_heartbeats_and_background_jobs_use_their_own_login_or_none() {
    let s = setup();
    // B5: a heartbeat turned on by Alex runs on Alex's login.
    exec(&s, s.alex, Command::SetHeartbeat(aurelius_domain::employee::SetHeartbeat { employee_id: s.ada.0, every_minutes: 30, quiet_start_min: None, quiet_end_min: None, daily_token_budget: None }));
    let login = s.service.store().read(|c| aurelius_storage::heartbeats::login_user(c, s.ada.0)).unwrap();
    assert_eq!(login, Some(s.alex.to_string()));
    // A run queued with an explicit login (a heartbeat's): that one; with none, none (never another's).
    let queue = |meta: serde_json::Value| s.service.store().write(move |tx| Ok(crate::run::queue::queue_run(tx, &meta, "t", s.ada.1, MessageId::new())?.0)).unwrap();
    assert_eq!(login_of(&s, queue(serde_json::json!({ "login_user": s.alex }))), Some(s.alex));
    assert_eq!(login_of(&s, queue(serde_json::json!({ "login_user": null, "user_id": s.robin }))), None);
    // Nothing to go by (no writer, no board member, no task): no login at all, not the chat's last.
    assert_eq!(login_of(&s, queue(serde_json::json!({}))), None);

    // B6: memory consolidation only on the company's login.
    let scope: String = s.service.store().read(|c| Ok(c.query_row("SELECT scope_id FROM employees WHERE id = ?1", [s.ada.0.to_string()], |r| r.get(0))?)).unwrap();
    for n in 0..2 {
        let remember = aurelius_domain::memory::Remember { scope: aurelius_domain::memory::MemoryScope::Employee(s.ada.0), kind: aurelius_domain::memory::MemoryKind::Decision, content: format!("Point {n} of the plan."), pinned: false };
        exec(&s, s.alex, Command::Remember(remember));
    }
    // As if an employee had remembered them (what a board member keeps is never merged).
    s.service.store().write(|tx| Ok(tx.execute("UPDATE memory_items SET decided_by = NULL", [])?)).unwrap();
    let input = s.service.consolidation_input(&scope).unwrap().expect("two memories to merge");
    assert_eq!(input.config.login_user, None, "no company login: it does not run");
    let robin = s.robin;
    s.service.store().write(move |tx| aurelius_storage::company::set_background_login_user(tx, Some(robin)).map(drop)).unwrap();
    assert_eq!(s.service.consolidation_input(&scope).unwrap().unwrap().config.login_user, Some(s.robin));
}

#[test]
fn the_dispatchers_triage_and_the_summary_after_done_use_the_tickets_login() {
    let s = setup();
    let dispatcher = aurelius_domain::company::UpdateCompany { name: None, dispatcher: Some(aurelius_domain::ticket::SetDispatcher { employee_id: Some(s.otto.0) }), tester: None };
    exec(&s, s.alex, Command::UpdateCompany(dispatcher));
    let create = ticket::CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id: s.project,
        title: "Search".into(),
        description: None,
        priority: Default::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: None,
        run: Default::default(),
        files: vec![],
        start: true,
        from_message: None,
        from_idea: None,
    };
    let CommandResult::TicketCreated { ticket_id, .. } = exec(&s, s.robin, Command::CreateTicket(create)) else { panic!() };
    let root = s.service.store().read(|c| aurelius_storage::tickets::state(c, ticket_id)).unwrap().unwrap().root_task_id.unwrap();
    let (run, _) = s.service.triage_run(ticket_id, root).unwrap();
    let run = run.expect("the dispatcher runs");
    assert_eq!(run.config.login_user, Some(s.robin));
    assert_eq!(login_of(&s, run.run_id), Some(s.robin));

    // The summary after Done (the extraction of the main task) runs on the ticket's login too.
    let (_, owned) = ticket_of(&s, s.robin, s.ada.0);
    start_task(&s, owned, s.ada.0);
    let input = s.service.extraction_input(owned, None).unwrap().expect("the main task has an employee");
    assert_eq!(input.config.login_user, Some(s.robin));
}

// ── No login: wait, say so, resume (MU3-12) ─────────────────────────────

/// Lease `job` as the dispatcher would (epoch back).
fn lease(s: &Setup, job: String) -> i64 {
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE jobs SET state = 'leased', lease_owner = 't', lease_until_ms = ?2, lease_epoch = lease_epoch + 1 WHERE id = ?1", rusqlite::params![job, i64::MAX / 4])?;
            Ok(tx.query_row("SELECT lease_epoch FROM jobs WHERE id = ?1", [&job], |r| r.get(0))?)
        })
        .unwrap()
}

/// The run finds no login when it starts: it waits ('login').
fn waits_for_login(s: &Setup, run: RunId) {
    let epoch = lease(s, run.to_string());
    s.service.defer_run(run, epoch, aurelius_domain::scheduler::Disposition::WaitForUser { reason: aurelius_domain::task::WAITING_LOGIN }).unwrap();
}

fn ticket_view(s: &Setup, id: TicketId) -> aurelius_domain::ticket::TicketView {
    let read = |_: &str| None;
    s.service.store().read(|c| aurelius_storage::tickets::view(c, id, &read)).unwrap().unwrap()
}

fn lines(s: &Setup, ticket: TicketId) -> Vec<String> {
    let conversation = ticket_view(s, ticket).conversation_id.unwrap();
    let st = s.service.store().clone();
    s.service
        .store()
        .read(move |c| aurelius_storage::conversations::list_messages(c, conversation, &|k| st.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok())))
        .unwrap()
        .into_iter()
        .filter(|m| m.system.is_some())
        .map(|m| m.text)
        .collect()
}

fn inbox(s: &Setup, user: UserId) -> Vec<String> {
    s.service.store().read(move |c| aurelius_storage::timeline::list_notifications_for(c, 50, Some(&user.to_string()))).unwrap().into_iter().map(|n| n.title).collect()
}

fn task_wait(s: &Setup, task: TaskId) -> (TaskStatus, Option<String>) {
    let (status, reason): (String, Option<String>) = s
        .service
        .store()
        .read(move |c| Ok(c.query_row("SELECT status, waiting_reason FROM tasks WHERE id = ?1", [task.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .unwrap();
    (TaskStatus::parse(&status).unwrap(), reason)
}

fn job_due(s: &Setup, job: String) -> bool {
    s.service
        .store()
        .read(move |c| Ok(c.query_row("SELECT state = 'ready' AND last_error_code IS NULL AND not_before_ms <= ?2 FROM jobs WHERE id = ?1", rusqlite::params![job, aurelius_storage::now_ms()], |r| r.get(0))?))
        .unwrap()
}

#[test]
fn a_ticket_without_its_creators_login_waits_tells_them_and_resumes_once_linked() {
    let s = setup();
    let (ticket, root) = ticket_of(&s, s.robin, s.ada.0);
    let main = start_task(&s, root, s.ada.0);
    let (part_task, part) = delegate(&s, main, "Iris", s.iris.0);
    waits_for_login(&s, main);
    waits_for_login(&s, part);

    // AC1: never on another login; it waits with 'login', the card stays where it is.
    assert_eq!(task_wait(&s, root), (TaskStatus::Waiting, Some("login".into())));
    assert_eq!(task_wait(&s, part_task), (TaskStatus::Waiting, Some("login".into())));
    let view = ticket_view(&s, ticket);
    assert_eq!(view.needs, Some(aurelius_domain::ticket::TicketNeed::Login));
    assert_eq!((view.login_user, view.login_username.as_str()), (s.robin, "robin"));
    assert_eq!(view.column, aurelius_domain::ticket::ColumnKind::Doing);
    // AC2: one line in the discussion and one note in Robin's own Inbox, none in Alex's.
    let waiting = "Waiting: robin's Claude login is missing. It goes on once robin connects it.";
    assert_eq!(lines(&s, ticket).iter().filter(|l| *l == waiting).count(), 1, "{:?}", lines(&s, ticket));
    assert_eq!(inbox(&s, s.robin), ["Connect your Claude login"]);
    assert!(inbox(&s, s.alex).is_empty(), "another board member's Inbox stays empty");
    assert!(s.service.store().read(|c| aurelius_storage::timeline::list_notifications_for(c, 50, None)).unwrap().is_empty(), "not for everyone");
    // R2 M2: its push goes to Robin's phone only, never to Alex's.
    let (robins_phone, alexs_phone) = (ClientId::new(), ClientId::new());
    let (robin, alex) = (s.robin, s.alex);
    let seq: i64 = s
        .service
        .store()
        .write(move |tx| {
            use aurelius_storage::remote;
            for (phone, user) in [(robins_phone, robin), (alexs_phone, alex)] {
                remote::create_client(tx, phone, PrincipalId::new(), "phone", "ios", "v", &aurelius_domain::remote::Capability::DEVICE_DEFAULT)?;
                remote::set_push(tx, phone, "apns", &format!("token-{phone}"), "sandbox")?;
                tx.execute("UPDATE remote_clients SET user_id = ?2 WHERE id = ?1", rusqlite::params![phone.to_string(), user.to_string()])?;
            }
            Ok(tx.query_row("SELECT e.seq FROM notifications n JOIN events e ON e.id = n.event_id WHERE n.user_id = ?1", [robin.to_string()], |r| r.get(0))?)
        })
        .unwrap();
    let (found, targets) = s.service.store().read(move |c| aurelius_storage::remote::pushes_for_seq(c, seq as u64)).unwrap().pop().unwrap();
    assert_eq!(found.view.title, "Connect your Claude login");
    assert_eq!(targets.iter().map(|t| t.client_id).collect::<Vec<_>>(), [robins_phone], "no deliver to another board member's device");

    // AC3/B11: Robin links the login: everything goes on by itself.
    s.service.claude_login_linked(s.robin, Some("test@example.com".into()), Some("max".into())).unwrap();
    // QA2 m2: the note is resolved once the login is linked.
    assert!(inbox(&s, s.robin).is_empty(), "{:?}", inbox(&s, s.robin));
    assert!(job_due(&s, main.to_string()) && job_due(&s, part.to_string()), "both runs are due again");
    assert_eq!(task_wait(&s, root), (TaskStatus::Running, None));
    assert_eq!(task_wait(&s, part_task), (TaskStatus::Running, None));
    assert_eq!(ticket_view(&s, ticket).needs, None);
    assert_eq!(lines(&s, ticket).last().map(String::as_str), Some(crate::run::login_wait::RESUMED));
}

#[test]
fn a_refused_login_is_expired_and_a_disabled_creators_ticket_says_so() {
    let s = setup();
    s.service.claude_login_linked(s.robin, Some("test@example.com".into()), None).unwrap();
    // A 401 in a run: expired, and what then waits says "expired".
    s.service.claude_login_expired(s.robin).unwrap();
    let status = s.service.store().read(|c| aurelius_storage::claude_logins::get(c, &s.robin.to_string())).unwrap().unwrap();
    assert_eq!((status.status, status.email.as_deref()), (aurelius_storage::claude_logins::LoginStatus::Expired, Some("test@example.com")));
    let (ticket, root) = ticket_of(&s, s.robin, s.ada.0);
    waits_for_login(&s, start_task(&s, root, s.ada.0));
    assert!(lines(&s, ticket).iter().any(|l| l == "Waiting: robin's Claude login has expired. It goes on once robin reconnects it."), "{:?}", lines(&s, ticket));
    assert_eq!(inbox(&s, s.robin), ["Reconnect your Claude login"]);

    // AC4: a disabled creator: still waiting, and the line says why.
    let robin = s.robin;
    s.service.store().write(move |tx| aurelius_storage::users::disable(tx, &robin.to_string(), 5)).unwrap();
    let (other, root) = ticket_of(&s, s.robin, s.iris.0);
    waits_for_login(&s, start_task(&s, root, s.iris.0));
    assert!(lines(&s, other).iter().any(|l| l.starts_with("Waiting: robin is disabled")), "{:?}", lines(&s, other));
    assert_eq!(ticket_view(&s, other).needs, Some(aurelius_domain::ticket::TicketNeed::Login));
}

#[test]
fn run_on_my_login_takes_the_ticket_over_for_its_waiting_runs_only_for_oneself() {
    let s = setup();
    let (ticket, root) = ticket_of(&s, s.robin, s.ada.0);
    let main = start_task(&s, root, s.ada.0);
    waits_for_login(&s, main);
    s.service.claude_login_linked(s.alex, Some("test@example.com".into()), None).unwrap();

    // An app (no board member) cannot; Alex can, for Alex's own login only.
    let app = CommandContext { app: Some(AppActor { key_id: aurelius_domain::AppKeyId::new(), name: "x".into() }), ..ctx(None) };
    let refused = s.service.execute(&app, CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::UseMyLogin(ticket::UseMyLogin { ticket_id: ticket }) });
    assert_eq!(refused.err().map(|e| e.code), Some(ErrorCode::BoardMemberRequired));
    exec(&s, s.alex, Command::UseMyLogin(ticket::UseMyLogin { ticket_id: ticket }));
    let view = ticket_view(&s, ticket);
    assert_eq!((view.login_user, view.created_by_user, view.needs), (s.alex, s.robin, None), "the creator stays");
    assert!(lines(&s, ticket).iter().any(|l| l == "alex runs this ticket on their own Claude login now (it ran on robin's)."), "{:?}", lines(&s, ticket));
    // The parked run moved with it and goes on at once (Alex's login is linked).
    assert_eq!(login_of(&s, main), Some(s.alex));
    assert!(job_due(&s, main.to_string()));
    assert_eq!(task_wait(&s, root), (TaskStatus::Running, None));
}

#[test]
fn the_dispatcher_waits_for_its_logins_limit_without_using_a_try() {
    // R2 m8: like the runs on that login, it waits for the reset instead of failing.
    let s = setup();
    let dispatcher = aurelius_domain::company::UpdateCompany { name: None, dispatcher: Some(aurelius_domain::ticket::SetDispatcher { employee_id: Some(s.otto.0) }), tester: None };
    exec(&s, s.alex, Command::UpdateCompany(dispatcher));
    let create = ticket::CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id: s.project, title: "Search".into(), description: None, priority: Default::default(), labels: vec![], deadline_ms: None,
        owner_employee_id: None, run: Default::default(), files: vec![], start: true, from_message: None, from_idea: None,
    };
    let CommandResult::TicketCreated { ticket_id, .. } = exec(&s, s.robin, Command::CreateTicket(create)) else { panic!() };
    let root = s.service.store().read(|c| aurelius_storage::tickets::state(c, ticket_id)).unwrap().unwrap().root_task_id.unwrap();
    let job: String = s.service.store().read(|c| Ok(c.query_row("SELECT id FROM jobs WHERE kind = 'ticket.triage'", [], |r| r.get(0))?)).unwrap();
    let attempts = |s: &Setup, job: String| -> i64 { s.service.store().read(move |c| Ok(c.query_row("SELECT attempts FROM jobs WHERE id = ?1", [job], |r| r.get(0))?)).unwrap() };
    let epoch = lease(&s, job.clone());
    let before = attempts(&s, job.clone());
    let run = s.service.triage_run(ticket_id, root).unwrap().0.unwrap();
    let until = aurelius_storage::now_ms() + 3_600_000;
    s.service.triage_waits_for_quota(ticket_id, run.run_id, job.clone(), epoch, until).unwrap();
    assert_eq!(not_before(&s, job.clone()), until);
    let (state, error): (String, String) = s.service.store().read({ let job = job.clone(); move |c| Ok(c.query_row("SELECT state, last_error_code FROM jobs WHERE id = ?1", [job], |r| Ok((r.get(0)?, r.get(1)?)))?) }).unwrap();
    assert_eq!((state.as_str(), error.as_str()), ("ready", "quota"));
    assert!(attempts(&s, job) <= before, "no try used");
    assert!(lines(&s, ticket_id).iter().any(|l| l.starts_with("Paused: robin's Claude limit is reached until")), "{:?}", lines(&s, ticket_id));
}

/// When `job` may be claimed again (its `not_before_ms`).
fn not_before(s: &Setup, job: String) -> i64 {
    s.service.store().read(move |c| Ok(c.query_row("SELECT not_before_ms FROM jobs WHERE id = ?1", [job], |r| r.get(0))?)).unwrap()
}

#[test]
fn run_on_my_login_also_wakes_what_waited_for_the_old_logins_limit() {
    // R2 m7: Robin's limit parked the run; on Alex's login it need not wait for Robin's reset.
    let s = setup();
    let (ticket, root) = ticket_of(&s, s.robin, s.ada.0);
    let main = start_task(&s, root, s.ada.0);
    s.service.store().write(|tx| Ok(tx.execute("UPDATE runs SET runtime_kind = 'claude_code'", [])?)).unwrap();
    let far = aurelius_storage::now_ms() + 3_600_000;
    s.service.park_runs_for_quota_of(s.robin, far).unwrap();
    assert_eq!(not_before(&s, main.to_string()), far);
    s.service.claude_login_linked(s.alex, None, None).unwrap();
    exec(&s, s.alex, Command::UseMyLogin(ticket::UseMyLogin { ticket_id: ticket }));
    assert_eq!(login_of(&s, main), Some(s.alex));
    assert!(not_before(&s, main.to_string()) <= aurelius_storage::now_ms(), "due now, on Alex's login");
}

#[test]
fn background_jobs_that_waited_for_the_old_login_wake_when_it_changes_or_goes() {
    // R2 m6: a consolidation parked on Alex's login (B6) goes again once the company's login is Robin's, or none.
    let s = setup();
    let set = |by: UserId, enabled: bool| s.service.execute(&ctx(Some(by)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::SetBackgroundLogin(aurelius_domain::company::SetBackgroundLogin { enabled }) }).unwrap();
    let parked = |s: &Setup, on: UserId| -> String {
        let id = format!("consolidate-{}", aurelius_domain::JobId::new());
        let job = id.clone();
        s.service
            .store()
            .write(move |tx| {
                let source: String = tx.query_row("SELECT id FROM events ORDER BY seq LIMIT 1", [], |r| r.get(0))?;
                aurelius_storage::events::insert_job(tx, &job, crate::memory::CONSOLIDATE_JOB_KIND, aurelius_domain::EventId::parse(&source).unwrap(), &job, &serde_json::json!({ "scope_id": "x", "login_user": on }))?;
                tx.execute("UPDATE jobs SET last_error_code = 'login', not_before_ms = ?2 WHERE id = ?1", rusqlite::params![job, i64::MAX / 2])?;
                Ok(())
            })
            .unwrap();
        id
    };
    set(s.alex, true);
    let first = parked(&s, s.alex);
    set(s.robin, true);
    assert!(job_due(&s, first.clone()), "a switch to Robin wakes it");
    // Robin unlinks: what waited for Robin's login wakes too (and then finds none).
    s.service.claude_login_linked(s.robin, None, None).unwrap();
    let second = parked(&s, s.robin);
    s.service.claude_login_removed(s.robin).unwrap();
    assert!(job_due(&s, second), "unlinking the background login wakes it");
}

#[test]
fn the_background_login_is_only_ones_own() {
    let s = setup();
    let set = |by: UserId, enabled: bool| s.service.execute(&ctx(Some(by)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::SetBackgroundLogin(aurelius_domain::company::SetBackgroundLogin { enabled }) });
    set(s.alex, true).unwrap();
    assert_eq!(s.service.store().read(aurelius_storage::company::background_login_user).unwrap(), Some(s.alex));
    assert!(set(s.robin, false).is_err(), "Robin cannot turn Alex's off");
    set(s.robin, true).unwrap();
    assert_eq!(s.service.store().read(aurelius_storage::company::background_login_user).unwrap(), Some(s.robin), "Robin chooses their own");
    set(s.robin, false).unwrap();
    assert_eq!(s.service.store().read(aurelius_storage::company::background_login_user).unwrap(), None);
}

#[test]
fn the_dispatcher_waits_for_the_tickets_login_without_using_a_try() {
    let s = setup();
    let dispatcher = aurelius_domain::company::UpdateCompany { name: None, dispatcher: Some(aurelius_domain::ticket::SetDispatcher { employee_id: Some(s.otto.0) }), tester: None };
    exec(&s, s.alex, Command::UpdateCompany(dispatcher));
    let create = ticket::CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id: s.project, title: "Search".into(), description: None, priority: Default::default(), labels: vec![], deadline_ms: None,
        owner_employee_id: None, run: Default::default(), files: vec![], start: true, from_message: None, from_idea: None,
    };
    let CommandResult::TicketCreated { ticket_id, .. } = exec(&s, s.robin, Command::CreateTicket(create)) else { panic!() };
    let root = s.service.store().read(|c| aurelius_storage::tickets::state(c, ticket_id)).unwrap().unwrap().root_task_id.unwrap();
    let job: String = s.service.store().read(|c| Ok(c.query_row("SELECT id FROM jobs WHERE kind = 'ticket.triage'", [], |r| r.get(0))?)).unwrap();
    let epoch = lease(&s, job.clone());
    let run = s.service.triage_run(ticket_id, root).unwrap().0.unwrap();
    s.service.triage_waits_for_login(ticket_id, run.run_id, job.clone(), epoch).unwrap();
    let view = ticket_view(&s, ticket_id);
    assert_eq!((view.column, view.needs), (aurelius_domain::ticket::ColumnKind::Todo, Some(aurelius_domain::ticket::TicketNeed::Login)), "stays in Todo");
    assert_eq!(inbox(&s, s.robin), ["Connect your Claude login"]);
    s.service.claude_login_linked(s.robin, None, None).unwrap();
    assert!(job_due(&s, job), "the dispatcher goes again once Robin links");
    assert_eq!(ticket_view(&s, ticket_id).needs, None);
    // The run that waited was no try: the next one still has both.
    lease(&s, s.service.store().read(|c| Ok(c.query_row("SELECT id FROM jobs WHERE kind = 'ticket.triage'", [], |r| r.get::<_, String>(0))?)).unwrap());
    assert!(s.service.triage_run(ticket_id, root).unwrap().0.is_some());
}

// ── The usage limit per login (MU3-11) ──────────────────────────────────

fn jobs_parked_for_quota(s: &Setup, runs: &[RunId]) -> Vec<bool> {
    runs.iter()
        .map(|r| {
            let id = r.to_string();
            s.service.store().read(move |c| Ok(c.query_row("SELECT COALESCE(last_error_code, '') = 'quota' FROM jobs WHERE id = ?1", [id], |r| r.get(0))?)).unwrap()
        })
        .collect()
}

#[test]
fn only_the_runs_on_the_limited_login_wait_and_the_ticket_says_whose_until_when() {
    let s = setup();
    // The usage overview of a run reports the reset; nothing is on the clock here.
    let (robins_ticket, robins_root) = ticket_of(&s, s.robin, s.ada.0);
    let (alexs_ticket, alexs_root) = ticket_of(&s, s.alex, s.iris.0);
    let robin_main = start_task(&s, robins_root, s.ada.0);
    let (robin_part_task, robin_part) = delegate(&s, robin_main, "Otto", s.otto.0);
    let alex_main = start_task(&s, alexs_root, s.iris.0);
    // The provider of every run here is the built-in one; as on Claude Code.
    s.service.store().write(|tx| Ok(tx.execute("UPDATE runs SET runtime_kind = 'claude_code'", [])?)).unwrap();

    // Robin's main run hits the limit: it waits, and so does every ready run on Robin's login.
    let until = aurelius_storage::now_ms() + 3_600_000;
    let epoch = lease(&s, robin_main.to_string());
    s.service.defer_run(robin_main, epoch, aurelius_domain::scheduler::Disposition::WaitUntil { at_ms: until, reason: "quota" }).unwrap();
    s.service.park_runs_for_quota_of(s.robin, until).unwrap();
    assert_eq!(jobs_parked_for_quota(&s, &[robin_main, robin_part, alex_main]), [true, true, false], "Alex's run goes on");
    assert_eq!(task_wait(&s, robins_root), (TaskStatus::Waiting, Some("quota".into())));
    assert_eq!(task_wait(&s, robin_part_task), (TaskStatus::Waiting, Some("quota".into())));
    assert_eq!(task_wait(&s, alexs_root), (TaskStatus::Running, None));
    let paused = format!("Paused: robin's Claude limit is reached until {}.", crate::run::login_wait::clock(until, aurelius_storage::now_ms()));
    assert_eq!(lines(&s, robins_ticket).iter().filter(|l| **l == paused).count(), 1, "{:?}", lines(&s, robins_ticket));
    assert!(lines(&s, alexs_ticket).iter().all(|l| !l.starts_with("Paused")));
    // The card says whose limit until when from the main task's wait (B12: never the usage itself).
    let root = s.service.store().read(|c| aurelius_storage::tasks::load_task(c, robins_root)).unwrap().unwrap();
    assert_eq!(root.status, TaskStatus::Waiting);

    // At the reset they go again on the same login (AC3), and the discussion says "Resumed." once.
    for run in [robin_main, robin_part] {
        s.service.store().write(move |tx| crate::task::task_runs::on_run_quota_resumed(tx, run)).unwrap();
    }
    assert_eq!(lines(&s, robins_ticket).iter().filter(|l| *l == crate::run::login_wait::RESUMED).count(), 1);
    assert_eq!(login_of(&s, robin_main), Some(s.robin));
}

#[test]
fn a_time_reads_as_the_servers_clock() {
    let now = aurelius_storage::now_ms();
    let today = crate::run::login_wait::clock(now, now);
    assert_eq!(today.len(), 5, "{today}");
    assert!(today.as_bytes()[2] == b':');
    let later = crate::run::login_wait::clock(now + 3 * 86_400_000, now);
    assert!(["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].iter().any(|m| later.contains(m)), "{later}");
}
