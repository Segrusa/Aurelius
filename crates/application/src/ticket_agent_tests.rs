//! The board tools of employees (AUR-BOARD-TOOL-001, `ticket::agent`): who may read, make,
//! change, start and post, what a refusal leaves behind (nothing) and who the events name.

use super::*;
use crate::task::task_runs::EmployeeAction;
use aurelius_domain::company::UpdateCompany;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{self, ColumnKind, SetDispatcher};
use aurelius_domain::{ConversationId, EmployeeId, RunId, TicketId, UserId};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    project: ProjectId,
    user: UserId,
    /// The head of the org chart.
    atlas: EmployeeId,
    /// Reports to Atlas; the company's dispatcher.
    orion: EmployeeId,
    ann: EmployeeId,
    bo: EmployeeId,
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: user }
}

fn exec(s: &Setup, command: Command) -> CommandResult {
    s.service.execute(&ctx(Some(s.user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }).unwrap().receipt.result
}

fn hire(service: &CommandService, user: UserId, name: &str, boss: Option<EmployeeId>) -> EmployeeId {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Dev".into(), title: None, reports_to: boss, instructions: None, folder: None };
    let made = service.execute(&ctx(Some(user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::CreateEmployee(create) }).unwrap().receipt.result;
    let CommandResult::EmployeeCreated { employee_id, .. } = made else { panic!("{made:?}") };
    employee_id
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    let user = UserId::new();
    service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &user.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC))
        .unwrap();
    let made = service
        .execute(&ctx(Some(user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::CreateProject(CreateProject { name: "Orbit".into(), description: None, folder: None }) })
        .unwrap()
        .receipt
        .result;
    let CommandResult::ProjectCreated { project_id, .. } = made else { panic!("{made:?}") };
    let atlas = hire(&service, user, "Atlas", None);
    let orion = hire(&service, user, "Orion", Some(atlas));
    let ann = hire(&service, user, "Ann", Some(atlas));
    let bo = hire(&service, user, "Bo", Some(atlas));
    let s = Setup { _dir: dir, service, project: project_id, user, atlas, orion, ann, bo };
    exec(&s, Command::UpdateCompany(UpdateCompany { name: None, dispatcher: Some(SetDispatcher { employee_id: Some(orion) }), tester: None }));
    s
}

/// A run of `employee` on the login of the setup's board member (none: a run without a login).
fn run_of(s: &Setup, employee: EmployeeId, login: bool) -> RunId {
    let run = RunId::new();
    let login = login.then(|| s.user.to_string());
    s.service
        .store()
        .write(move |tx| {
            let chat = aurelius_storage::employees::main_chat(tx, employee)?.expect("an employee has a main chat");
            tx.execute(
                "INSERT INTO runs(id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status, started_at_ms, login_user) VALUES (?1, ?2, 1, 'fake', '', '{}', 'running', 1, ?3)",
                rusqlite::params![run.to_string(), chat.to_string(), login],
            )?;
            Ok(())
        })
        .unwrap();
    run
}

fn act(s: &Setup, run: RunId, action: EmployeeAction) -> (bool, String) {
    let (ok, text, _) = s.service.employee_action(run, action).unwrap();
    (ok, text)
}

fn create_action(title: &str) -> EmployeeAction {
    EmployeeAction::CreateTicket { project: Some("Orbit".into()), title: title.into(), description: Some("Why and what.".into()), priority: None, labels: vec![] }
}

fn update_action(ticket: &str, title: &str) -> EmployeeAction {
    EmployeeAction::UpdateTicket { design_url: None, ticket: ticket.into(), title: Some(title.into()), description: None, priority: None, labels: None }
}

fn move_action(ticket: &str, column: &str) -> EmployeeAction {
    EmployeeAction::MoveTicket { ticket: ticket.into(), column: column.into() }
}

/// A ticket made by the board member, in Backlog.
fn new_ticket(s: &Setup, title: &str, owner: Option<EmployeeId>) -> TicketId {
    let create = ticket::CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id: s.project,
        title: title.into(),
        description: None,
        priority: Default::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: owner,
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: None,
        from_idea: None,
        kind: None,
    };
    let CommandResult::TicketCreated { ticket_id, .. } = exec(s, Command::CreateTicket(create)) else { panic!() };
    ticket_id
}

fn view(s: &Setup, id: TicketId) -> aurelius_domain::ticket::TicketView {
    let read = |_: &str| None;
    s.service.store().read(|c| aurelius_storage::tickets::view(c, id, &read)).unwrap().unwrap()
}

fn engine_puts(s: &Setup, id: TicketId, kind: ColumnKind) {
    let target = s.service.store().read(|c| aurelius_storage::company::column_of_kind(c, kind)).unwrap().id;
    s.service.store().write(move |tx| aurelius_storage::tickets::move_to(tx, id, target, None)).unwrap();
}

/// What a refusal must leave alone: events, tickets, tasks, runs, jobs, messages and blobs.
fn counts(s: &Setup) -> [i64; 7] {
    s.service
        .store()
        .read(|c| {
            let n = |table: &str| c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0));
            Ok([n("events")?, n("tickets")?, n("tasks")?, n("runs")?, n("jobs")?, n("messages")?, n("blobs")?])
        })
        .unwrap()
}

/// The events of one type about a ticket: (actor_kind, actor_id, metadata).
fn events_of(s: &Setup, id: TicketId, kind: &str) -> Vec<(String, Option<String>, serde_json::Value)> {
    let (id, kind) = (id.to_string(), kind.to_owned());
    s.service
        .store()
        .read(move |c| {
            let mut stmt = c.prepare("SELECT actor_kind, actor_id, metadata_json FROM events WHERE aggregate_type = 'ticket' AND aggregate_id = ?1 AND type = ?2 ORDER BY seq")?;
            let rows = stmt.query_map([id, kind], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, serde_json::from_str::<serde_json::Value>(&r.get::<_, String>(2)?).unwrap())))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap()
}

fn the_ticket_made(text: &str) -> String {
    let start = text.find('#').expect("a number") + 1;
    text[start..].chars().take_while(char::is_ascii_digit).collect()
}

/// Everything said in the ticket's discussion, system lines included, one per line.
fn discussion_text(s: &Setup, id: TicketId) -> String {
    let st = s.service.store().clone();
    s.service
        .store()
        .read(move |c| {
            let d = crate::conversation::discussion::of_ticket(c, id)?.unwrap();
            let messages = aurelius_storage::conversations::list_messages(c, d.id, &|k| st.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok()))?;
            Ok(messages.into_iter().map(|m| m.text).collect::<Vec<_>>().join("\n"))
        })
        .unwrap()
}

/// Give `employee` an open task in the ticket's task tree.
fn give_task(s: &Setup, ticket: TicketId, employee: EmployeeId, status: aurelius_domain::task::TaskStatus) {
    let (task, project, st) = (aurelius_domain::TaskId::new(), s.project, s.service.store().clone());
    s.service
        .store()
        .write(move |tx| {
            let blob = st.blobs().put(b"part", "text/plain; charset=utf-8")?;
            aurelius_storage::blobs::register(tx, &blob)?;
            let id = blob.id.to_string();
            aurelius_storage::tasks::insert_ticket_task(tx, task, project, Some(employee), "Part", &id, &id, status, None, Some(ticket))
        })
        .unwrap();
}

#[test]
fn the_head_of_the_chart_creates_a_ticket_in_backlog_named_by_the_employee() {
    let s = setup();
    let run = run_of(&s, s.atlas, true);
    let (ok, text) = act(&s, run, create_action("Export to CSV"));
    assert!(ok, "{text}");
    assert!(text.contains("in Backlog") && text.contains("does not start by itself"), "{text}");
    let id = s.service.store().read(|c| Ok(aurelius_storage::tickets::list(c, &Default::default(), &|_| None)?.into_iter().find(|t| t.title == "Export to CSV").unwrap().id)).unwrap();
    let t = view(&s, id);
    assert_eq!((t.column, t.owner_employee_id, t.created_by_user, t.responsible_user_id), (ColumnKind::Backlog, None, s.user, Some(s.user)));
    assert_eq!(t.description.as_deref(), None, "the test reader returns no blobs");
    let created = events_of(&s, id, "ticket.created");
    assert_eq!(created.len(), 1);
    let (kind, actor, meta) = &created[0];
    assert_eq!((kind.as_str(), actor.as_deref()), ("employee", Some(s.atlas.to_string().as_str())));
    assert_eq!(meta["by"], serde_json::json!({ "employee_id": s.atlas, "kind": "employee" }), "{meta}");
    assert_eq!(meta["run_id"], run.to_string());
    assert!(discussion_text(&s, id).contains("Atlas created this ticket"));
}

/// A running run of `employee` on a task of the ticket, in the employee's main chat.
fn run_on_task(s: &Setup, employee: EmployeeId, ticket: TicketId) -> RunId {
    let (task, project, st, user) = (aurelius_domain::TaskId::new(), s.project, s.service.store().clone(), s.user);
    let run = RunId::new();
    s.service
        .store()
        .write(move |tx| {
            let blob = st.blobs().put(b"part", "text/plain; charset=utf-8")?;
            aurelius_storage::blobs::register(tx, &blob)?;
            let id = blob.id.to_string();
            aurelius_storage::tasks::insert_ticket_task(tx, task, project, Some(employee), "Part", &id, &id, aurelius_domain::task::TaskStatus::Running, None, Some(ticket))?;
            let chat = aurelius_storage::employees::main_chat(tx, employee)?.expect("an employee has a main chat");
            tx.execute(
                "INSERT INTO runs(id, conversation_id, employee_id, task_id, attempt, runtime_kind, model_id, resolved_config_json, status, started_at_ms, login_user) VALUES (?1, ?2, ?3, ?4, 1, 'fake', '', '{}', 'running', 1, ?5)",
                rusqlite::params![run.to_string(), chat.to_string(), employee.to_string(), task.to_string(), user.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    run
}

#[test]
fn any_employee_creates_a_ticket_from_a_chat_a_tickets_discussion_or_a_task() {
    let s = setup();
    // Ann has no reports and is not the dispatcher: no management.
    let (ok, text) = act(&s, run_of(&s, s.ann, true), create_action("From a chat"));
    assert!(ok, "{text}");
    let ticket = new_ticket(&s, "Discussed", None);
    let discussion = view(&s, ticket).conversation_id.expect("a ticket has a discussion");
    // Inside a ticket the project comes from the place, so it need not be named.
    let unnamed = |title: &str| EmployeeAction::CreateTicket { project: None, title: title.into(), description: None, priority: None, labels: vec![] };
    let (ok, text) = act(&s, run_in(&s, s.ann, discussion, Some(s.user)), unnamed("From a discussion"));
    assert!(ok, "{text}");
    let (ok, text) = act(&s, run_on_task(&s, s.bo, ticket), unnamed("From a task"));
    assert!(ok, "{text}");
    let made: Vec<(String, ColumnKind, Option<EmployeeId>)> =
        s.service.store().read(|c| Ok(aurelius_storage::tickets::list(c, &Default::default(), &|_| None)?.into_iter().map(|t| (t.title, t.column, t.owner_employee_id)).collect())).unwrap();
    for title in ["From a chat", "From a discussion", "From a task"] {
        assert!(made.contains(&(title.to_owned(), ColumnKind::Backlog, None)), "{title} in Backlog without owner: {made:?}");
    }
}

#[test]
fn a_plain_employee_creates_but_does_not_start_a_ticket() {
    let s = setup();
    let run = run_of(&s, s.ann, true);
    let (ok, text) = act(&s, run, create_action("Ann's idea"));
    assert!(ok, "{text}");
    let (ok, text) = act(&s, run, move_action(&format!("#{}", the_ticket_made(&text)), "todo"));
    assert!(!ok && text.contains("Only management moves"), "{text}");
}

#[test]
fn create_ticket_starts_no_run_task_or_job_and_leaves_the_ticket_in_backlog() {
    let s = setup();
    let run = run_of(&s, s.atlas, true);
    let before = counts(&s);
    let (ok, text) = act(&s, run, create_action("Quiet ticket"));
    assert!(ok, "{text}");
    let after = counts(&s);
    // events, tickets and blobs grow; tasks, runs and jobs do not.
    assert_eq!((before[2], before[3], before[4]), (after[2], after[3], after[4]), "no task, run or job");
    assert_eq!(after[1], before[1] + 1);
    let number: u32 = the_ticket_made(&text).parse().unwrap();
    let t = s.service.store().read(|c| Ok(aurelius_storage::tickets::list(c, &Default::default(), &|_| None)?.into_iter().find(|t| t.number == number).unwrap())).unwrap();
    assert_eq!((t.column, t.owner_employee_id, t.root_task_id, t.started_at_ms), (ColumnKind::Backlog, None, None, None));
    assert!(events_of(&s, t.id, "ticket.started").is_empty() && events_of(&s, t.id, "ticket.dispatched").is_empty());
}

#[test]
fn creating_the_same_title_twice_in_one_run_makes_one_ticket() {
    let s = setup();
    let run = run_of(&s, s.atlas, true);
    assert!(act(&s, run, create_action("Same title")).0);
    let before = counts(&s);
    let (ok, text) = act(&s, run, create_action("same TITLE"));
    assert!(ok && text.contains("already made in this run"), "{text}");
    assert_eq!(before, counts(&s));
    // Another run makes its own.
    assert!(act(&s, run_of(&s, s.atlas, true), create_action("Same title")).0);
}

#[test]
fn the_same_title_in_two_projects_makes_two_tickets() {
    let s = setup();
    let made = exec(&s, Command::CreateProject(CreateProject { name: "Zeta".into(), description: None, folder: None }));
    assert!(matches!(made, CommandResult::ProjectCreated { .. }), "{made:?}");
    let run = run_of(&s, s.atlas, true);
    assert!(act(&s, run, create_action("Write docs")).0);
    let before = counts(&s);
    let zeta = EmployeeAction::CreateTicket { project: Some("Zeta".into()), title: "Write docs".into(), description: None, priority: None, labels: vec![] };
    let (ok, text) = act(&s, run, zeta);
    assert!(ok && text.contains("Created ticket") && !text.contains("already made"), "{text}");
    assert_eq!(counts(&s)[1], before[1] + 1);
}

#[test]
fn a_run_without_a_board_member_behind_it_cannot_make_or_start_a_ticket() {
    let s = setup();
    let run = run_of(&s, s.atlas, false);
    let t = new_ticket(&s, "Waiting", None);
    let before = counts(&s);
    let (ok, text) = act(&s, run, create_action("No login"));
    assert!(!ok && text.contains("No board member stands behind this run"), "{text}");
    let (ok, text) = act(&s, run, move_action(&t.to_string(), "todo"));
    assert!(!ok && text.contains("No board member"), "{text}");
    assert_eq!(before, counts(&s));
}

#[test]
fn update_changes_only_the_given_fields_and_names_the_employee() {
    let s = setup();
    let t = new_ticket(&s, "Old title", Some(s.ann));
    let run = run_of(&s, s.ann, true);
    let (ok, text) = act(&s, run, EmployeeAction::UpdateTicket { design_url: None, ticket: format!("#{}", view(&s, t).number), title: Some("New title".into()), description: Some("More words.".into()), priority: Some("high".into()), labels: Some(vec!["api".into()]) });
    assert!(ok, "the owner works on it: {text}");
    let v = view(&s, t);
    assert_eq!((v.title.as_str(), v.priority.as_str(), v.labels.clone()), ("New title", "high", vec!["api".to_string()]));
    assert_eq!((v.column, v.owner_employee_id), (ColumnKind::Backlog, Some(s.ann)), "nothing else moved");
    let updated = events_of(&s, t, "ticket.updated");
    let (kind, actor, meta) = updated.last().unwrap();
    assert_eq!((kind.as_str(), actor.as_deref()), ("employee", Some(s.ann.to_string().as_str())));
    assert_eq!(meta["changed"], serde_json::json!(["title", "description", "priority", "labels"]));
    assert_eq!(meta["by"]["employee_id"], s.ann.to_string());
    assert!(discussion_text(&s, t).contains("Ann changed the title, description, priority, labels"));
}

#[test]
fn an_agent_with_an_open_task_in_the_ticket_may_update_it_and_one_without_may_not() {
    let s = setup();
    let t = new_ticket(&s, "Shared work", Some(s.ann));
    let before = counts(&s);
    let (ok, text) = act(&s, run_of(&s, s.bo, true), update_action(&t.to_string(), "Hijacked"));
    assert!(!ok && text.contains("only change a ticket you work on"), "{text}");
    assert_eq!(view(&s, t).title, "Shared work");
    assert_eq!(counts(&s)[..3], [before[0], before[1], before[2]], "no event, ticket or task");
    // Bo gets an open task in the ticket's task tree: now he works on it.
    give_task(&s, t, s.bo, aurelius_domain::task::TaskStatus::Running);
    let (ok, text) = act(&s, run_of(&s, s.bo, true), update_action(&t.to_string(), "Part title"));
    assert!(ok, "{text}");
    assert_eq!(view(&s, t).title, "Part title");
    // An ended task is no longer work on it.
    let cy = hire(&s.service, s.user, "Cy", Some(s.atlas));
    give_task(&s, t, cy, aurelius_domain::task::TaskStatus::Succeeded);
    assert!(!act(&s, run_of(&s, cy, true), update_action(&t.to_string(), "Late")).0);
    assert_eq!(view(&s, t).title, "Part title");
}

#[test]
fn a_manager_updates_any_open_ticket_but_never_a_done_or_archived_one() {
    let s = setup();
    let t = new_ticket(&s, "Anyone's", Some(s.bo));
    let atlas = run_of(&s, s.atlas, true);
    assert!(act(&s, atlas, update_action(&t.to_string(), "Managed")).0);
    assert_eq!(view(&s, t).title, "Managed");
    engine_puts(&s, t, ColumnKind::Done);
    let before = counts(&s);
    let (ok, text) = act(&s, atlas, update_action(&t.to_string(), "Reopened by edit"));
    assert!(!ok && text.contains("closed"), "{text}");
    assert_eq!((view(&s, t).title.as_str(), counts(&s)), ("Managed", before));
    let archived = new_ticket(&s, "Gone", None);
    s.service.store().write(move |tx| aurelius_storage::tickets::mark_archived(tx, archived)).unwrap();
    let (ok, text) = act(&s, atlas, update_action(&archived.to_string(), "x"));
    assert!(!ok && text.contains("No ticket"), "{text}");
}

#[test]
fn update_ticket_links_a_design_and_refuses_what_is_no_artifact() {
    let s = setup();
    let t = new_ticket(&s, "Dashboard", Some(s.ann));
    let run = run_of(&s, s.ann, true);
    let link = "https://claude.ai/artifact/abc123";
    let design = |url: &str| EmployeeAction::UpdateTicket { design_url: Some(url.into()), ticket: t.to_string(), title: None, description: None, priority: None, labels: None };
    let (ok, text) = act(&s, run, design("https://example.com/not-claude"));
    assert!(!ok && text.contains("Claude artifact link"), "{text}");
    assert_eq!(view(&s, t).design_url, None, "nothing written");
    let (ok, text) = act(&s, run, design(link));
    assert!(ok, "{text}");
    assert_eq!(view(&s, t).design_url.as_deref(), Some(link));
    assert_eq!(events_of(&s, t, "ticket.updated").last().unwrap().2["changed"], serde_json::json!(["design"]));
    assert!(discussion_text(&s, t).contains("Ann changed the design link"));
    let (ok, _) = act(&s, run, design(""));
    assert!(ok, "empty clears");
    assert_eq!(view(&s, t).design_url, None);
}

#[test]
fn an_update_with_nothing_to_change_or_a_bad_value_writes_nothing() {
    let s = setup();
    let t = new_ticket(&s, "Plain", None);
    let atlas = run_of(&s, s.atlas, true);
    let before = counts(&s);
    let nothing = EmployeeAction::UpdateTicket { design_url: None, ticket: t.to_string(), title: None, description: None, priority: None, labels: None };
    assert!(act(&s, atlas, nothing).1.contains("Give a field to change"));
    let bad = EmployeeAction::UpdateTicket { design_url: None, ticket: t.to_string(), title: None, description: None, priority: Some("whenever".into()), labels: None };
    assert!(act(&s, atlas, bad).1.contains("No priority called whenever"));
    assert!(act(&s, atlas, update_action(&t.to_string(), "   ")).1.len() > 5);
    assert_eq!(before, counts(&s));
}

#[test]
fn a_manager_starts_a_backlog_ticket_and_the_events_name_the_employee() {
    let s = setup();
    let t = new_ticket(&s, "Start me", None);
    let (ok, text) = act(&s, run_of(&s, s.orion, true), move_action(&format!("#{}", view(&s, t).number), "Todo"));
    assert!(ok, "{text}");
    let v = view(&s, t);
    assert_eq!(v.column, ColumnKind::Todo);
    assert!(v.root_task_id.is_some() && v.started_at_ms.is_some(), "the engine made the main task");
    for kind in ["ticket.moved", "ticket.started"] {
        let events = events_of(&s, t, kind);
        let (actor_kind, actor, meta) = events.last().unwrap();
        assert_eq!((actor_kind.as_str(), actor.as_deref()), ("employee", Some(s.orion.to_string().as_str())), "{kind}");
        assert_eq!(meta["by"]["employee_id"], s.orion.to_string(), "{kind}");
    }
    assert!(discussion_text(&s, t).contains("Orion started the ticket"));
    // The dispatcher is asked to choose the owner: a triage job is queued, as after a board member's Start.
    let jobs: i64 = s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM jobs WHERE kind = 'ticket.triage'", [], |r| r.get(0))?)).unwrap();
    assert_eq!(jobs, 1);
    // A second start is a no-op in the same column.
    let before = counts(&s);
    let (ok, text) = act(&s, run_of(&s, s.orion, true), move_action(&t.to_string(), "todo"));
    assert!(ok && text.contains("nothing moved"), "{text}");
    assert_eq!(before[..3], counts(&s)[..3]);
}

#[test]
fn an_agent_that_is_not_management_cannot_move_a_ticket() {
    let s = setup();
    let t = new_ticket(&s, "Mine", Some(s.ann));
    let before = counts(&s);
    let (ok, text) = act(&s, run_of(&s, s.ann, true), move_action(&t.to_string(), "todo"));
    assert!(!ok && text.contains("Only management moves a ticket"), "{text}");
    assert_eq!(view(&s, t).column, ColumnKind::Backlog);
    assert_eq!(before[..3], counts(&s)[..3]);
}

#[test]
fn a_manager_cannot_take_the_decisions_of_a_board_member_or_of_the_work() {
    let s = setup();
    let atlas = run_of(&s, s.atlas, true);
    let backlog = new_ticket(&s, "Fresh", None);
    for (column, why) in [
        ("done", "Only a board member puts a ticket on Done"),
        ("plan_review", "Plan review"),
        ("Plan review", "Plan review"),
        ("testing", "Testing moves on by itself"),
        ("review", "reports"),
        ("doing", "by itself"),
    ] {
        let before = counts(&s);
        let (ok, text) = act(&s, atlas, move_action(&backlog.to_string(), column));
        assert!(!ok && text.contains(why), "{column}: {text}");
        assert_eq!((view(&s, backlog).column, counts(&s)), (ColumnKind::Backlog, before), "{column} changed something");
    }
    // A started ticket does not go back to Backlog (that cancels its work), nor on to Doing or Done.
    let started = new_ticket(&s, "Started", None);
    assert!(act(&s, atlas, move_action(&started.to_string(), "todo")).0);
    for column in ["backlog", "doing", "done", "review", "testing", "plan_review"] {
        let before = counts(&s);
        let (ok, text) = act(&s, atlas, move_action(&started.to_string(), column));
        assert!(!ok, "{column}: {text}");
        assert_eq!((view(&s, started).column, counts(&s)), (ColumnKind::Todo, before), "{column} changed something");
    }
    assert!(act(&s, atlas, move_action(&started.to_string(), "backlog")).1.contains("cancels its work"));
    assert!(events_of(&s, started, "ticket.cancelled").is_empty());
    // A card on Done is closed.
    engine_puts(&s, backlog, ColumnKind::Done);
    assert!(act(&s, atlas, move_action(&backlog.to_string(), "todo")).1.contains("closed"));
    assert!(act(&s, atlas, move_action(&backlog.to_string(), "sideways")).1.contains("No column called sideways"));
}

#[test]
fn reading_lists_the_board_and_a_ticket_with_its_discussion_for_any_employee() {
    let s = setup();
    let t = new_ticket(&s, "Readable", Some(s.bo));
    let other = new_ticket(&s, "Other thing", None);
    engine_puts(&s, other, ColumnKind::Todo);
    let ann = run_of(&s, s.ann, true);
    let (ok, list) = act(&s, ann, EmployeeAction::ListTickets { project: None, column: None, query: None, limit: None });
    assert!(ok && list.contains("Readable") && list.contains("Other thing") && list.contains("owner Bo") && list.contains("[backlog]"), "{list}");
    let (_, todo) = act(&s, ann, EmployeeAction::ListTickets { project: Some("Orbit".into()), column: Some("To do".into()), query: None, limit: None });
    assert!(todo.contains("Other thing") && !todo.contains("Readable"), "{todo}");
    let (_, found) = act(&s, ann, EmployeeAction::ListTickets { project: None, column: None, query: Some("readable".into()), limit: Some(1) });
    assert!(found.contains("Readable") && !found.contains("Other"), "{found}");
    let (ok, text) = act(&s, ann, EmployeeAction::ListTickets { project: Some("Nowhere".into()), column: None, query: None, limit: None });
    assert!(!ok && text.contains("No project called Nowhere"), "{text}");
    assert!(!act(&s, ann, EmployeeAction::ListTickets { project: None, column: Some("sideways".into()), query: None, limit: None }).0);
    let number = view(&s, t).number;
    let (ok, read) = act(&s, ann, EmployeeAction::ReadTicket { ticket: format!("#{number}"), last: None });
    assert!(ok && read.contains("Readable") && read.contains("owner Bo") && read.contains("Latest messages"), "{read}");
    assert!(act(&s, ann, EmployeeAction::ReadTicket { ticket: t.to_string(), last: Some(3) }).0, "by id too");
}

#[test]
fn a_ticket_that_is_not_shown_reads_like_one_that_does_not_exist() {
    let s = setup();
    let gone = new_ticket(&s, "Archived away", None);
    s.service.store().write(move |tx| aurelius_storage::tickets::mark_archived(tx, gone)).unwrap();
    let ann = run_of(&s, s.ann, true);
    let (_, hidden) = act(&s, ann, EmployeeAction::ReadTicket { ticket: gone.to_string(), last: None });
    let (ok, missing) = act(&s, ann, EmployeeAction::ReadTicket { ticket: "#9999".into(), last: None });
    assert!(!ok && hidden.starts_with("No ticket") && missing.starts_with("No ticket"), "{hidden} / {missing}");
    assert_eq!(hidden.replace(&gone.to_string(), "X"), missing.replace("#9999", "X"));
    let (_, list) = act(&s, ann, EmployeeAction::ListTickets { project: None, column: None, query: None, limit: None });
    assert!(!list.contains("Archived away"), "{list}");
    let (ok, text) = act(&s, ann, EmployeeAction::PostToTicket { ticket: gone.to_string(), message: "hi".into() });
    assert!(!ok && text.starts_with("No ticket"), "{text}");
}

#[test]
fn the_project_assistant_without_an_employee_has_no_ticket_tools() {
    let s = setup();
    // A run of a project's task that no employee works on is not an employee's run.
    let (run, project) = (RunId::new(), s.project);
    s.service
        .store()
        .write(move |tx| {
            let channel = aurelius_domain::ConversationId::new();
            aurelius_storage::conversations::insert_project_conversation(tx, channel, project, "Plans")?;
            tx.execute("INSERT INTO runs(id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status, started_at_ms) VALUES (?1, ?2, 1, 'fake', '', '{}', 'running', 1)", rusqlite::params![run.to_string(), channel.to_string()])?;
            Ok(())
        })
        .unwrap();
    for action in [EmployeeAction::ListTickets { project: None, column: None, query: None, limit: None }, create_action("x"), move_action("#1", "todo")] {
        let (ok, text) = act(&s, run, action);
        assert!(!ok && text.contains("Only an employee"), "{text}");
    }
}

#[test]
fn an_employee_posts_in_a_tickets_discussion_but_not_on_done() {
    let s = setup();
    let t = new_ticket(&s, "Chatty", None);
    let ann = run_of(&s, s.ann, true);
    let before = counts(&s);
    let (ok, text) = act(&s, ann, EmployeeAction::PostToTicket { ticket: t.to_string(), message: "A thought for the board.".into() });
    assert!(ok && text.contains("Posted in"), "{text}");
    assert_eq!(counts(&s)[5], before[5] + 1, "one message");
    let said = discussion_text(&s, t);
    assert!(said.contains("A thought for the board."), "{said}");
    engine_puts(&s, t, ColumnKind::Done);
    let before = counts(&s);
    let (ok, text) = act(&s, ann, EmployeeAction::PostToTicket { ticket: t.to_string(), message: "Too late.".into() });
    assert!(!ok && text.contains("can only be read"), "{text}");
    assert_eq!(before, counts(&s));
    let (ok, text) = act(&s, ann, EmployeeAction::PostToTicket { ticket: "#4242".into(), message: "x".into() });
    assert!(!ok && text.contains("No ticket"), "{text}");
}

#[test]
fn a_post_that_names_a_colleague_wakes_nobody() {
    let s = setup();
    let t = new_ticket(&s, "Wake", None);
    let ann = run_of(&s, s.ann, true);
    let in_chats = |s: &Setup| -> i64 { s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0))?)).unwrap() };
    let before = in_chats(&s);
    let (ok, text) = act(&s, ann, EmployeeAction::PostToTicket { ticket: t.to_string(), message: "@Bo can you look?".into() });
    assert!(ok && text.contains("wakes nobody") && text.contains("hand_off"), "{text}");
    assert_eq!(in_chats(&s), before, "a name only refers to someone");
}

/// A run of `employee` on the login of another board member than the setup's.
fn run_on_login_of_other(s: &Setup, employee: EmployeeId) -> RunId {
    let other = UserId::new();
    s.service
        .store()
        .write(move |tx| {
            aurelius_storage::users::insert(
                tx,
                &aurelius_storage::users::NewUser {
                    id: &other.to_string(),
                    username: "ada",
                    created_by_user: None,
                    now_ms: 2,
                },
                PHC,
            )
        })
        .unwrap();
    let run = run_of(s, employee, false);
    s.service
        .store()
        .write(move |tx| {
            Ok(tx.execute(
                "UPDATE runs SET login_user = ?2 WHERE id = ?1",
                rusqlite::params![run.to_string(), other.to_string()],
            )?)
        })
        .unwrap();
    run
}

/// Put the employee's chat (and so its runs) in `mode`.
fn set_mode(s: &Setup, employee: EmployeeId, mode: aurelius_domain::autonomy::AgentMode) {
    s.service
        .store()
        .write(move |tx| {
            let chat = aurelius_storage::employees::main_chat(tx, employee)?
                .expect("an employee has a main chat");
            aurelius_storage::conversations::set_agent_mode(tx, chat, mode)?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn a_run_on_another_board_members_login_cannot_start_a_ticket_and_nothing_is_written() {
    let s = setup();
    let t = new_ticket(&s, "Grace's ticket", None);
    let other = run_on_login_of_other(&s, s.orion);
    let before = counts(&s);
    let (ok, text) = act(&s, other, move_action(&t.to_string(), "todo"));
    assert!(
        !ok && text.contains("another board member's Claude login"),
        "{text}"
    );
    assert_eq!(
        (view(&s, t).column, counts(&s)),
        (ColumnKind::Backlog, before),
        "a refused start moved or wrote something"
    );
    // The same ticket starts from a run on its own login.
    let (ok, text) = act(
        &s,
        run_of(&s, s.orion, true),
        move_action(&t.to_string(), "todo"),
    );
    assert!(ok, "{text}");
    assert_eq!(view(&s, t).column, ColumnKind::Todo);
}

#[test]
fn a_run_in_plan_mode_reads_but_cannot_create_update_move_or_post() {
    use aurelius_domain::autonomy::AgentMode;
    let s = setup();
    let t = new_ticket(&s, "Plan me", Some(s.orion));
    set_mode(&s, s.orion, AgentMode::Plan);
    let run = run_of(&s, s.orion, true);
    let before = counts(&s);
    let post = EmployeeAction::PostToTicket {
        ticket: t.to_string(),
        message: "hello".into(),
    };
    for (what, action) in [
        ("create", create_action("Nope")),
        ("update", update_action(&t.to_string(), "New")),
        ("move", move_action(&t.to_string(), "todo")),
        ("post", post),
    ] {
        let (ok, text) = act(&s, run, action);
        assert!(!ok && text.contains("Plan mode"), "{what}: {text}");
    }
    assert_eq!(
        (view(&s, t).column, view(&s, t).title.as_str(), counts(&s)),
        (ColumnKind::Backlog, "Plan me", before),
        "Plan mode wrote something"
    );
    let (ok, text) = act(
        &s,
        run,
        EmployeeAction::ListTickets {
            project: None,
            column: None,
            query: None,
            limit: None,
        },
    );
    assert!(ok && text.contains("Plan me"), "{text}");
    let (ok, _) = act(
        &s,
        run,
        EmployeeAction::ReadTicket {
            ticket: t.to_string(),
            last: None,
        },
    );
    assert!(ok);
    // Out of Plan mode the same update goes through.
    set_mode(&s, s.orion, AgentMode::Auto);
    assert!(
        act(
            &s,
            run_of(&s, s.orion, true),
            update_action(&t.to_string(), "New")
        )
        .0
    );
}

#[test]
fn an_employee_cannot_change_the_text_of_a_ticket_under_review_but_may_change_its_priority() {
    let s = setup();
    let atlas = run_of(&s, s.atlas, true);
    for kind in [
        ColumnKind::PlanReview,
        ColumnKind::Testing,
        ColumnKind::Review,
    ] {
        let t = new_ticket(&s, &format!("Under {}", kind.as_str()), None);
        engine_puts(&s, t, kind);
        let before = counts(&s);
        let (ok, text) = act(&s, atlas, update_action(&t.to_string(), "Rewritten"));
        assert!(
            !ok && text.contains("under review"),
            "{}: {text}",
            kind.as_str()
        );
        let describe = EmployeeAction::UpdateTicket { design_url: None,
            ticket: t.to_string(),
            title: None,
            description: Some("new".into()),
            priority: None,
            labels: None,
        };
        assert!(!act(&s, atlas, describe).0, "{}", kind.as_str());
        assert_eq!(
            (view(&s, t).title.clone(), counts(&s)),
            (format!("Under {}", kind.as_str()), before),
            "{} changed something",
            kind.as_str()
        );
        let priority = EmployeeAction::UpdateTicket { design_url: None,
            ticket: t.to_string(),
            title: None,
            description: None,
            priority: Some("high".into()),
            labels: None,
        };
        assert!(
            act(&s, atlas, priority).0,
            "priority stays open in {}",
            kind.as_str()
        );
    }
    // In Todo the text still changes.
    let t = new_ticket(&s, "Open", None);
    assert!(act(&s, atlas, update_action(&t.to_string(), "Open, renamed")).0);
}

// ── delegate_task lands on a ticket ─────────────────────────────────────

fn delegate_action(ticket: Option<&str>) -> EmployeeAction {
    EmployeeAction::Delegate { employee: "Ann".into(), title: "A part".into(), goal: "Do the part".into(), criteria: vec![], ticket: ticket.map(str::to_owned) }
}

/// A running run of `employee` in `conversation` on `login`.
fn run_in(s: &Setup, employee: EmployeeId, conversation: ConversationId, login: Option<UserId>) -> RunId {
    let run = RunId::new();
    s.service
        .store()
        .write(move |tx| {
            tx.execute(
                "INSERT INTO runs(id, conversation_id, employee_id, attempt, runtime_kind, model_id, resolved_config_json, status, started_at_ms, login_user) VALUES (?1, ?2, ?3, 1, 'fake', '', '{}', 'running', 1, ?4)",
                rusqlite::params![run.to_string(), conversation.to_string(), employee.to_string(), login.map(|l| l.to_string())],
            )?;
            Ok(())
        })
        .unwrap();
    run
}

/// The tasks of the company: (ticket, parent, project, requested_by), newest first.
fn given_tasks(s: &Setup) -> Vec<(Option<String>, Option<String>, String, Option<String>)> {
    s.service
        .store()
        .read(|c| {
            let mut q = c.prepare("SELECT ticket_id, parent_task_id, project_id, requested_by FROM tasks WHERE title = 'A part' ORDER BY rowid DESC")?;
            let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

#[test]
fn a_task_from_a_chat_without_a_ticket_is_refused_and_nothing_is_written() {
    let s = setup();
    let run = run_of(&s, s.atlas, true);
    assert_eq!(act(&s, run, delegate_action(None)), (false, crate::ticket::agent::DELEGATE_NEEDS_TICKET.to_owned()));
    let (ok, text) = act(&s, run, delegate_action(Some("#42")));
    assert!(!ok && text.contains("No ticket #42"), "{text}");
    assert!(given_tasks(&s).is_empty());
}

#[test]
fn a_task_from_a_chat_lands_on_the_ticket_it_names_and_is_not_its_main_task() {
    let s = setup();
    let ticket = new_ticket(&s, "Logo", None);
    let number = view(&s, ticket).number;
    let (ok, text) = act(&s, run_of(&s, s.atlas, true), delegate_action(Some(&format!("#{number}"))));
    assert!(ok, "{text}");
    let given = given_tasks(&s);
    assert_eq!(given.len(), 1);
    assert_eq!(given[0], (Some(ticket.to_string()), None, s.project.to_string(), Some(s.atlas.to_string())));
    assert_eq!(view(&s, ticket).root_task_id, None, "it is not the ticket's main task");
}

#[test]
fn a_ticket_on_done_takes_no_new_task() {
    let s = setup();
    let ticket = new_ticket(&s, "Finished", None);
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE tickets SET column_id = (SELECT id FROM board_columns WHERE kind = 'done') WHERE id = ?1", [ticket.to_string()])?;
            Ok(())
        })
        .unwrap();
    let number = view(&s, ticket).number;
    assert_eq!(act(&s, run_of(&s, s.atlas, true), delegate_action(Some(&format!("#{number}")))), (false, crate::ticket::agent::DELEGATE_TICKET_DONE.to_owned()));
    assert!(given_tasks(&s).is_empty());
}

#[test]
fn a_run_on_another_login_cannot_add_work_to_a_ticket_it_only_names() {
    let s = setup();
    let ticket = new_ticket(&s, "Grace's", None);
    let number = view(&s, ticket).number;
    let other = UserId::new();
    s.service.store().write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &other.to_string(), username: "linus", created_by_user: None, now_ms: 1 }, PHC)).unwrap();
    let chat = s.service.store().read(|c| aurelius_storage::employees::main_chat(c, s.atlas)).unwrap().unwrap();
    assert_eq!(act(&s, run_in(&s, s.atlas, chat, Some(other)), delegate_action(Some(&format!("#{number}")))), (false, crate::ticket::agent::DELEGATE_OTHER_LOGIN.to_owned()));
    assert!(given_tasks(&s).is_empty());
}

#[test]
fn a_run_in_a_tickets_discussion_gives_work_on_that_ticket_without_naming_it() {
    let s = setup();
    let ticket = new_ticket(&s, "Discussed", None);
    let other = new_ticket(&s, "Elsewhere", None);
    let discussion = view(&s, ticket).conversation_id.expect("a ticket has a discussion");
    // Woken there by another board member: the ticket's work stays on the ticket's login.
    let writer = UserId::new();
    s.service.store().write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &writer.to_string(), username: "linus", created_by_user: None, now_ms: 1 }, PHC)).unwrap();
    let elsewhere = format!("#{}", view(&s, other).number);
    let (ok, text) = act(&s, run_in(&s, s.atlas, discussion, Some(writer)), delegate_action(Some(&elsewhere)));
    assert!(ok, "{text}");
    assert_eq!(given_tasks(&s)[0].0, Some(ticket.to_string()), "its own ticket wins over a name");
}
