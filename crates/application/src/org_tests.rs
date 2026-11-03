//! The company's org chart: reports-to rules, the text an employee gets,
//! delegation and hand-over limits, escalation to the boss and colleague proposals.

use crate::task::task_runs::EmployeeAction;
use crate::*;
use aurelius_domain::{
    ConversationId, EventKind, EmployeeId, MessageId, RunId, TaskId,
    autonomy::{AgentMode, SetAgentMode},
    employee::{CreateEmployee, LifecycleStatus, SetFollowOrgChart, SetEmployeeReportsTo, SetEmployeeStatus, UpdateEmployee},
    project::CreateProject,
    project_conversation::SendProjectMessage,
    proposal::{DecideProposal, ProposalFields, ProposalKind, ProposalStatus},
    task::{CreateTask, TaskStatus},
    views::ProposalView,
};
use aurelius_storage::{Store, employees, tasks as store};

// ── Helpers ─────────────────────────────────────────────────────────────

fn ctx() -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app: None, user_id: None }
}

fn setup() -> (tempfile::TempDir, CommandService) {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    (dir, service)
}

fn run(s: &CommandService, command: Command) -> Result<Committed, AppError> {
    s.execute(&ctx(), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command })
}

fn project_named(s: &CommandService, name: &str) -> ProjectId {
    match run(s, Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None })).unwrap().receipt.result {
        CommandResult::ProjectCreated { project_id, .. } => project_id,
        other => panic!("unexpected {other:?}"),
    }
}

fn project(s: &CommandService) -> ProjectId {
    project_named(s, "Atlas")
}

/// Employees are company-wide; the project only says where the test works.
fn try_hire(s: &CommandService, _project: ProjectId, name: &str, title: Option<&str>, boss: Option<EmployeeId>) -> Result<(EmployeeId, ConversationId), AppError> {
    let create = CreateEmployee {
        name: name.into(),
        role: "Dev".into(),
        title: title.map(Into::into),
        reports_to: boss,
        instructions: Some(format!("Focus of {name}.")),
        folder: None,
    };
    match run(s, Command::CreateEmployee(create))?.receipt.result {
        CommandResult::EmployeeCreated { employee_id, conversation_id } => Ok((employee_id, conversation_id)),
        other => panic!("unexpected {other:?}"),
    }
}

fn hire(s: &CommandService, project: ProjectId, name: &str, title: Option<&str>, boss: Option<EmployeeId>) -> (EmployeeId, ConversationId) {
    try_hire(s, project, name, title, boss).unwrap()
}

fn retitle(s: &CommandService, employee: EmployeeId, name: &str, title: &str) {
    let update = UpdateEmployee { employee_id: employee, name: name.into(), role: "Dev".into(), title: Some(title.into()), instructions: Some(format!("Focus of {name}.")), sequential_tasks: None };
    run(s, Command::UpdateEmployee(update)).unwrap();
}

fn set_boss(s: &CommandService, employee: EmployeeId, boss: Option<EmployeeId>) -> Result<Committed, AppError> {
    run(s, Command::SetEmployeeReportsTo(SetEmployeeReportsTo { employee_id: employee, reports_to: boss }))
}

fn archive(s: &CommandService, employee: EmployeeId) -> Committed {
    run(s, Command::SetEmployeeStatus(SetEmployeeStatus { employee_id: employee, status: LifecycleStatus::Archived })).unwrap()
}

/// "Follow the org chart" is one setting of the company (on by default).
fn follow(s: &CommandService, _project: ProjectId, on: bool) -> Committed {
    run(s, Command::SetFollowOrgChart(SetFollowOrgChart { follow: on })).unwrap()
}

fn boss_of(s: &CommandService, employee: EmployeeId) -> Option<EmployeeId> {
    let text: Option<String> = s.store().read(|c| Ok(c.query_row("SELECT reports_to FROM employees WHERE id = ?1", [employee.to_string()], |r| r.get(0))?)).unwrap();
    text.map(|t| EmployeeId::parse(&t).unwrap())
}

fn count(s: &CommandService, sql: &'static str) -> i64 {
    s.store().read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?)).unwrap()
}

/// A queued run in an employee's chat, or another conversation (what a bridge call belongs to).
fn chat_run(s: &CommandService, conversation: ConversationId) -> RunId {
    s.store().write(move |tx| Ok(crate::run::queue::queue_run(tx, &serde_json::json!({}), "test", conversation, MessageId::new())?.0)).unwrap()
}

/// A task of `employee` that is running, with a run in its conversation.
fn task_run(s: &CommandService, project: ProjectId, employee: EmployeeId, title: &str) -> (TaskId, RunId) {
    let create = CreateTask { confirm_secret: false, project_id: project, employee_id: Some(employee), title: title.into(), goal: "Do it".into(), acceptance_criteria: vec![], depends_on: vec![], run: Default::default(), files: vec![] };
    let CommandResult::TaskCreated { task_id } = run(s, Command::CreateTask(create)).unwrap().receipt.result else { panic!() };
    (task_id, start_task(s, task_id, employee))
}

fn start_task(s: &CommandService, task: TaskId, employee: EmployeeId) -> RunId {
    let st = s.store().clone();
    s.store()
        .write(move |tx| {
            let (conversation, brief, _) = crate::task::task_threads::open_returning(tx, &st, task, employee, "brief")?;
            let _ = crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?;
            Ok(crate::run::queue::queue_run(tx, &serde_json::json!({}), &task.to_string(), conversation.unwrap(), brief.unwrap())?.0)
        })
        .unwrap()
}

/// From a chat the work lands on ticket #1 (of Atlas); in a task without a ticket too.
fn delegate(s: &CommandService, run: RunId, to: &str) -> (bool, String) {
    let action = EmployeeAction::Delegate { employee: to.into(), title: "A part".into(), goal: "Do the part".into(), criteria: vec![], ticket: Some("#1".into()) };
    let (ok, text, _) = s.employee_action(run, action).unwrap();
    (ok, text)
}

fn hand_over(s: &CommandService, run: RunId, to: &str) -> (bool, String) {
    let (ok, text, _) = s.employee_action(run, EmployeeAction::HandOver { employee: to.into(), reason: "better suited".into() }).unwrap();
    (ok, text)
}

/// The system text an employee's answer would get.
fn system_of(s: &CommandService, run: RunId) -> String {
    let payload: String = s.store().read(|c| Ok(c.query_row("SELECT payload_json FROM jobs WHERE id = ?1", [run.to_string()], |r| r.get(0))?)).unwrap();
    s.prepare_chat_run(&serde_json::from_str(&payload).unwrap()).unwrap().expect("a plan").system.unwrap_or_default()
}

/// `(title, employee_id)` of every task, oldest first.
fn tasks(s: &CommandService) -> Vec<(String, Option<String>)> {
    s.store()
        .read(|c| {
            let mut stmt = c.prepare("SELECT title, employee_id FROM tasks ORDER BY created_at_ms, rowid")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap()
}

fn end_run(s: &CommandService, run: RunId, completed: bool, failed: Option<&'static str>) {
    let st = s.store().clone();
    s.store().write(move |tx| crate::task::task_runs::on_run_finished(tx, &st, run, completed, failed, "partial work")).unwrap();
}

fn texts(s: &CommandService, conversation: ConversationId) -> Vec<String> {
    let st = s.store().clone();
    s.store()
        .read(|c| {
            let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            aurelius_storage::conversations::list_messages(c, conversation, &read)
        })
        .unwrap()
        .into_iter()
        .map(|m| m.text)
        .collect()
}

/// jean → simon → (dean, max); returns their ids and main conversations.
struct Team {
    project: ProjectId,
    jean: (EmployeeId, ConversationId),
    simon: (EmployeeId, ConversationId),
    dean: (EmployeeId, ConversationId),
    max: (EmployeeId, ConversationId),
}

/// Ticket #1 of the project, in Backlog: work given with `delegate` lands on it.
fn shared_ticket(s: &CommandService, project_id: ProjectId) -> aurelius_domain::TicketId {
    let user = aurelius_domain::UserId::new();
    s.store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &user.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA"))
        .unwrap();
    let create = aurelius_domain::ticket::CreateTicket {
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
    let by = CommandContext { user_id: Some(user), ..ctx() };
    match s.execute(&by, CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::CreateTicket(create) }).unwrap().receipt.result {
        CommandResult::TicketCreated { ticket_id, .. } => ticket_id,
        other => panic!("unexpected {other:?}"),
    }
}

/// The id of ticket #1.
fn shared_ticket_of(s: &CommandService) -> String {
    s.store().read(|c| Ok(c.query_row("SELECT id FROM tickets WHERE number = 1", [], |r| r.get(0))?)).unwrap()
}

fn team(s: &CommandService) -> Team {
    let project = project(s);
    let jean = hire(s, project, "Jean", Some("Head"), None);
    let simon = hire(s, project, "Simon", Some("Lead"), Some(jean.0));
    let dean = hire(s, project, "Dean", None, Some(simon.0));
    let max = hire(s, project, "Max", None, Some(simon.0));
    shared_ticket(s, project);
    Team { project, jean, simon, dean, max }
}

// ── Reports to ──────────────────────────────────────────────────────────

#[test]
fn reports_to_refuses_loops_strangers_and_archived_bosses() {
    let (_d, s) = setup();
    let p = project(&s);
    let (a, _) = hire(&s, p, "Ann", None, None);
    let (b, _) = hire(&s, p, "Bob", None, Some(a));

    let looped = set_boss(&s, a, Some(b)).err().unwrap();
    assert_eq!(looped.code, ErrorCode::Validation);
    assert!(looped.message.contains("circle"), "{}", looped.message);
    assert!(set_boss(&s, a, Some(a)).is_err(), "nobody is their own boss");

    // Employees are company-wide: a boss who works in another project is fine.
    let other = project_named(&s, "Other");
    let (x, _) = hire(&s, other, "Xia", None, None);
    set_boss(&s, b, Some(x)).unwrap();
    set_boss(&s, b, Some(a)).unwrap();
    // Names are unique in the company (any case), also when renaming.
    let taken = try_hire(&s, other, " ann ", None, None).err().unwrap();
    assert!(taken.message.contains("already exists"), "{}", taken.message);
    let rename = UpdateEmployee { employee_id: x, name: "Bob".into(), role: "Dev".into(), title: None, instructions: None, sequential_tasks: None };
    assert!(run(&s, Command::UpdateEmployee(rename)).is_err());

    let (c, _) = hire(&s, p, "Cy", None, None);
    archive(&s, c);
    assert!(set_boss(&s, b, Some(c)).is_err(), "an archived boss");
    assert_eq!(set_boss(&s, c, Some(a)).err().unwrap().code, ErrorCode::Validation, "an archived employee cannot be moved");

    // Free Bob, then Ann may report to him.
    set_boss(&s, b, None).unwrap();
    assert_eq!(boss_of(&s, b), None);
    let moved = set_boss(&s, a, Some(b)).unwrap();
    assert_eq!(moved.notices.len(), 1);
    assert_eq!(moved.notices[0].kind, EventKind::EmployeeReportsToSet);
    assert_eq!(boss_of(&s, a), Some(b));
    // The same boss again is a receipt and no event.
    assert!(set_boss(&s, a, Some(b)).unwrap().notices.is_empty());
}

#[test]
fn the_chart_is_at_most_six_levels_deep() {
    let (_d, s) = setup();
    let p = project(&s);
    let mut chain: Vec<EmployeeId> = Vec::new();
    for i in 1..=6 {
        let (m, _) = hire(&s, p, &format!("M{i}"), None, chain.last().copied());
        chain.push(m);
    }
    let deeper = try_hire(&s, p, "M7", None, Some(chain[5])).err().unwrap();
    assert!(deeper.message.contains("6 levels"), "{}", deeper.message);
    let (loose, _) = hire(&s, p, "Loose", None, None);
    assert!(set_boss(&s, loose, Some(chain[5])).is_err(), "moving someone under the bottom too");
    assert!(set_boss(&s, loose, Some(chain[4])).is_ok());
}

#[test]
fn archiving_moves_reports_up_and_withdraws_proposals() {
    let (_d, s) = setup();
    let t = team(&s);
    let run_id = chat_run(&s, t.simon.1);
    assert!(propose(&s, run_id, "Eva", None).0);

    let archived = archive(&s, t.simon.0);
    let moved = archived.notices.iter().filter(|n| n.kind == EventKind::EmployeeReportsToSet).count();
    assert_eq!(moved, 2, "one event per moved report");
    assert_eq!(boss_of(&s, t.dean.0), Some(t.jean.0));
    assert_eq!(boss_of(&s, t.max.0), Some(t.jean.0));
    assert_eq!(s.proposals().unwrap().proposals[0].status, ProposalStatus::Withdrawn);
    let chart = s.store().read(employees::org_entries).unwrap();
    assert!(chart.iter().all(|(m, _)| *m != t.simon.0), "archived employees leave the chart");
}

#[test]
fn the_follow_setting_is_on_until_it_is_turned_off() {
    let (_d, s) = setup();
    let p = project(&s);
    let on = |s: &CommandService| s.store().read(employees::follow_org_chart).unwrap();
    assert!(on(&s), "the company follows its chart by default");
    assert!(follow(&s, p, true).notices.is_empty(), "no change, no event");
    let off = follow(&s, p, false);
    assert_eq!((off.notices.len(), off.notices[0].kind, off.notices[0].project_id), (1, EventKind::SettingsFollowOrgChart, None));
    assert!(!on(&s));
    follow(&s, p, true);
    assert!(on(&s));
}

// ── What an employee is told ──────────────────────────────────────────────

#[test]
fn a_flat_project_gets_the_same_text_as_before_and_a_chart_adds_lines() {
    let (_d, s) = setup();
    let p = project(&s);
    hire(&s, p, "Ann", None, None);
    let (bob, bob_conv) = hire(&s, p, "Bob", None, None);
    let before = system_of(&s, chat_run(&s, bob_conv));
    assert!(before.contains("Ann (Dev)") && !before.contains("Org chart"), "{before}");

    retitle(&s, bob, "Bob", "Lead");
    let with_title = system_of(&s, chat_run(&s, bob_conv));
    assert!(with_title.contains("Org chart of the company"), "{with_title}");

    // Back to a title-less, boss-less project: the text is what it was.
    retitle(&s, bob, "Bob", "");
    assert_eq!(system_of(&s, chat_run(&s, bob_conv)), before);
}

#[test]
fn the_chart_text_names_the_boss_the_reports_and_the_others() {
    let (_d, s) = setup();
    let t = team(&s);
    let simon = system_of(&s, chat_run(&s, t.simon.1));
    assert!(simon.contains("You report to Jean (Head)."), "{simon}");
    assert!(simon.contains("Reporting to you: Dean (Dev), Max (Dev)."), "{simon}");
    assert!(simon.contains("Your position: Lead."), "{simon}");
    let dean = system_of(&s, chat_run(&s, t.dean.1));
    assert!(dean.contains("You report to Simon (Lead).") && dean.contains("Peers (same boss): Max (Dev).") && dean.contains("Not directly reachable, go via your boss or your report: Jean (Head)."), "{dean}");
    assert!(simon.contains("The company follows its org chart"), "{simon}");
    // Its chat is outside projects: it is told to name one when it gives work.
    assert!(simon.contains("outside any project") && simon.contains("Jean (Dev, management team)"), "{simon}");
    follow(&s, t.project, false);
    assert!(!system_of(&s, chat_run(&s, t.simon.1)).contains("follows its org chart"), "only while the setting is on");
}

// ── Delegation and hand-over ────────────────────────────────────────────

#[test]
fn delegation_goes_to_direct_reports_only_while_the_chart_is_followed() {
    let (_d, s) = setup();
    let t = team(&s);
    let dean_run = chat_run(&s, t.dean.1);

    // Off: anyone may give work to anyone.
    follow(&s, t.project, false);
    assert!(delegate(&s, dean_run, "Max").0);
    // From a chat the task needs a ticket.
    let action = EmployeeAction::Delegate { employee: "Max".into(), title: "A part".into(), goal: "Do it".into(), criteria: vec![], ticket: None };
    let (ok, text, _) = s.employee_action(dean_run, action).unwrap();
    assert!(!ok && text == crate::ticket::agent::DELEGATE_NEEDS_TICKET, "{text}");
    let action = EmployeeAction::Delegate { employee: "Max".into(), title: "A part".into(), goal: "Do it".into(), criteria: vec![], ticket: Some("#99".into()) };
    assert!(!s.employee_action(dean_run, action).unwrap().0, "an unknown ticket");
    follow(&s, t.project, true);
    let before = tasks(&s).len();
    let (ok, text) = delegate(&s, dean_run, "Max");
    assert!(!ok && text.contains("Delegation follows the org chart"), "{text}");
    assert!(text.contains("nobody reports to you"), "{text}");
    assert_eq!(tasks(&s).len(), before, "nothing was created");

    // Only one level down works; a deeper employee is refused with the route.
    assert!(delegate(&s, chat_run(&s, t.jean.1), "Simon").0);
    let (ok, text) = delegate(&s, chat_run(&s, t.jean.1), "Dean");
    assert!(!ok && text.contains("Dean reports to Simon; delegate to Simon instead"), "{text}");
    assert!(delegate(&s, chat_run(&s, t.simon.1), "Dean").0);
    // The system text offers only the direct lines as targets, the rest marked.
    let jean = system_of(&s, chat_run(&s, t.jean.1));
    assert!(jean.contains("delegate_task only works for Simon"), "{jean}");
    assert!(jean.contains("Not directly reachable (go via your boss or your report):") && jean.contains("- Dean"), "{jean}");
    let (ok, text) = delegate(&s, chat_run(&s, t.simon.1), "Jean");
    assert!(!ok && text.contains("Dean, Max"), "{text}");

    // The project assistant is not on the chart.
    let sent = run(&s, Command::SendProjectMessage(SendProjectMessage { confirm_secret: false, project_id: t.project, text: "hi".into(), conversation_id: None, images: vec![], files: vec![], interrupt: false, start: Default::default() })).unwrap();
    let CommandResult::ProjectMessageSent { run_id, .. } = sent.receipt.result else { panic!() };
    assert!(delegate(&s, run_id, "Max").0);
}

#[test]
fn a_task_is_handed_over_to_the_boss_or_a_peer_only() {
    let (_d, s) = setup();
    let t = team(&s);
    follow(&s, t.project, true);
    let (_task, run_id) = task_run(&s, t.project, t.dean.0, "Fix it");
    let (ok, text) = hand_over(&s, run_id, "Jean");
    assert!(!ok && text.contains("Handing over follows the org chart") && text.contains("Simon, Max"), "{text}");
    assert!(hand_over(&s, run_id, "Max").0, "a peer");

    let (_, other_run) = task_run(&s, t.project, t.dean.0, "Fix that");
    assert!(hand_over(&s, other_run, "Simon").0, "the boss");
    assert_eq!(tasks(&s).iter().filter(|(_, m)| *m == Some(t.simon.0.to_string())).count(), 1);

    // Off: the task goes to whoever suits it.
    follow(&s, t.project, false);
    let (_, free_run) = task_run(&s, t.project, t.dean.0, "And this");
    assert!(hand_over(&s, free_run, "Jean").0);
}

#[test]
fn work_goes_down_a_six_level_chart_to_the_bottom() {
    let (_d, s) = setup();
    let p = project(&s);
    shared_ticket(&s, p);
    let mut chain: Vec<EmployeeId> = Vec::new();
    for i in 1..=6 {
        let (m, _) = hire(&s, p, &format!("M{i}"), None, chain.last().copied());
        chain.push(m);
    }
    follow(&s, p, true);
    // The chart's depth (six) is the limit: every level may give work one level down.
    let (mut task, mut run_id) = task_run(&s, p, chain[0], "Top");
    for level in 0..5 {
        let (ok, text) = delegate(&s, run_id, &format!("M{}", level + 2));
        assert!(ok, "level {level}: {text}");
        let child: String = s.store().read(|c| Ok(c.query_row("SELECT id FROM tasks WHERE parent_task_id = ?1", [task.to_string()], |r| r.get(0))?)).unwrap();
        task = TaskId::parse(&child).unwrap();
        run_id = start_task(&s, task, chain[level + 1]);
    }
    // The bottom has nobody below it.
    let (ok, text) = delegate(&s, run_id, "M1");
    assert!(!ok && text.contains("nobody reports to you"), "{text}");
}

// ── Escalation ──────────────────────────────────────────────────────────

#[test]
fn an_escalation_stays_on_the_ticket_of_the_task_it_comes_from() {
    let (_d, s) = setup();
    let t = team(&s);
    follow(&s, t.project, true);
    let ticket = shared_ticket_of(&s);
    let (task, run_id) = task_run(&s, t.project, t.simon.0, "Fix it");
    s.store().write(move |tx| Ok(tx.execute("UPDATE tasks SET ticket_id = ?2 WHERE id = ?1", [task.to_string(), ticket.clone()]).map(drop)?)).unwrap();
    end_run(&s, run_id, false, Some("provider_error"));
    let on: Option<String> = s.store().read(|c| Ok(c.query_row("SELECT ticket_id FROM tasks WHERE title LIKE 'Escalated:%'", [], |r| r.get(0))?)).unwrap();
    assert_eq!(on, Some(shared_ticket_of(&s)));
}

#[test]
fn a_failed_task_goes_to_the_boss_first_with_the_chart_on() {
    let (_d, s) = setup();
    let t = team(&s);
    follow(&s, t.project, true);
    let (task, run_id) = task_run(&s, t.project, t.simon.0, "Fix it");
    end_run(&s, run_id, false, Some("provider_error"));

    let all = tasks(&s);
    assert_eq!(all.len(), 2);
    assert_eq!(all[1], ("Escalated: Fix it".to_string(), Some(t.jean.0.to_string())));
    let (requested_by, origin): (String, String) = s
        .store()
        .read(|c| Ok(c.query_row("SELECT requested_by, origin_conversation_id FROM tasks WHERE title LIKE 'Escalated:%'", [], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .unwrap();
    let own_conversation: String = s.store().read(|c| Ok(c.query_row("SELECT conversation_id FROM tasks WHERE id = ?1", [task.to_string()], |r| r.get(0))?)).unwrap();
    assert_eq!((requested_by, origin), (t.simon.0.to_string(), own_conversation));
    let failed: String = s.store().read(|c| Ok(c.query_row("SELECT metadata_json FROM events WHERE type = 'task.failed'", [], |r| r.get(0))?)).unwrap();
    assert!(failed.contains("\"escalated_to\":\"Jean\""), "{failed}");
    assert_eq!(count(&s, "SELECT COUNT(*) FROM notifications"), 0, "the user is not asked: the boss has it");

    // The boss's answer comes back into the task's conversation.
    let escalation = s.store().read(|c| Ok(c.query_row("SELECT id FROM tasks WHERE title LIKE 'Escalated:%'", [], |r| r.get::<_, String>(0))?)).unwrap();
    let escalation = TaskId::parse(&escalation).unwrap();
    let st = s.store().clone();
    s.store().write(move |tx| crate::task::task_runs::complete(tx, &st, escalation, "Restart it")).unwrap();
    let from_jean = count(&s, "SELECT COUNT(*) FROM messages WHERE json_extract(meta_json, '$.from') = 'Jean' AND content_blob_id IS NOT NULL");
    assert_eq!(from_jean, 1);
}

#[test]
fn a_task_that_stops_waits_for_the_boss_not_for_the_user() {
    let (_d, s) = setup();
    let t = team(&s);
    follow(&s, t.project, true);
    let (task, run_id) = task_run(&s, t.project, t.dean.0, "Stuck");
    end_run(&s, run_id, false, None);
    assert_eq!(tasks(&s)[1].1, Some(t.simon.0.to_string()));
    let cause = s.store().read(|c| store::waiting_cause(c, task)).unwrap();
    assert_eq!(cause.as_deref(), Some("escalated_to_boss"));
}

#[test]
fn nothing_is_escalated_without_the_setting_for_a_root_or_a_sub_task() {
    let (_d, s) = setup();
    let t = team(&s);
    // Off: the user is told, as before.
    follow(&s, t.project, false);
    let (_, run_id) = task_run(&s, t.project, t.simon.0, "Fix it");
    end_run(&s, run_id, false, Some("provider_error"));
    assert_eq!(tasks(&s).len(), 1);
    assert_eq!(count(&s, "SELECT COUNT(*) FROM notifications WHERE severity = 'warning'"), 1);

    // On, but a root has nobody above it.
    follow(&s, t.project, true);
    let (_, root_run) = task_run(&s, t.project, t.jean.0, "Top");
    end_run(&s, root_run, false, Some("provider_error"));
    assert_eq!(tasks(&s).len(), 2);

    // On, and a delegated part reports to the employee who delegated it.
    let (top, top_run) = task_run(&s, t.project, t.jean.0, "Parent");
    assert!(delegate(&s, top_run, "Simon").0);
    let child: String = s.store().read(|c| Ok(c.query_row("SELECT id FROM tasks WHERE parent_task_id = ?1", [top.to_string()], |r| r.get(0))?)).unwrap();
    let child_run = start_task(&s, TaskId::parse(&child).unwrap(), t.simon.0);
    let before = tasks(&s).len();
    end_run(&s, child_run, false, Some("provider_error"));
    assert_eq!(tasks(&s).len(), before);
}

// ── Colleague proposals ─────────────────────────────────────────────────

fn propose(s: &CommandService, run: RunId, name: &str, boss: Option<&str>) -> (bool, String) {
    let action = EmployeeAction::ProposeColleague {
        name: name.into(),
        role: "Designer".into(),
        title: Some("UI lead".into()),
        instructions: "Design the screens.".into(),
        reports_to: boss.map(Into::into),
        reason: "We need design.".into(),
    };
    let (ok, text, _) = s.employee_action(run, action).unwrap();
    (ok, text)
}

/// Proposals are company-wide.
fn proposals(s: &CommandService, _project: ProjectId) -> Vec<ProposalView> {
    s.proposals().unwrap().proposals
}

fn decide(s: &CommandService, proposal: &ProposalView, approve: bool, edits: Option<ProposalFields>, note: Option<&str>) -> Result<Committed, AppError> {
    let cmd = DecideProposal { proposal_id: proposal.id, approve, edits, note: note.map(Into::into), instructions: None };
    run(s, Command::DecideProposal(cmd))
}

fn employee_row(s: &CommandService, name: &'static str) -> Option<(String, Option<String>, Option<String>)> {
    s.store()
        .read(|c| {
            let mut stmt = c.prepare("SELECT id, title, reports_to FROM employees WHERE name = ?1")?;
            let rows = stmt.query_map([name], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?.into_iter().next())
        })
        .unwrap()
}

#[test]
fn a_proposal_waits_for_the_user_even_in_auto_mode() {
    let (_d, s) = setup();
    let t = team(&s);
    run(&s, Command::SetAgentMode(SetAgentMode { conversation_id: t.simon.1, mode: AgentMode::Auto })).unwrap();
    let employees_before = count(&s, "SELECT COUNT(*) FROM employees");
    let (ok, text) = propose(&s, chat_run(&s, t.simon.1), "Eva", None);
    assert!(ok && text.contains("approves") && text.contains("this conversation"), "{text}");

    let pending = proposals(&s, t.project);
    assert_eq!((pending.len(), pending[0].status), (1, ProposalStatus::Pending));
    assert_eq!(pending[0].reports_to, Some(t.simon.0), "the proposer unless said otherwise");
    assert_eq!(count(&s, "SELECT COUNT(*) FROM employees"), employees_before, "no employee without the user");
    assert_eq!(employee_row(&s, "Eva"), None);
    // It shows up for the user.
    let notes = s.store().read(|c| aurelius_storage::timeline::list_notifications(c, 50)).unwrap();
    assert_eq!(notes.iter().filter(|n| n.target_type == "employee.proposed").count(), 1);
}

#[test]
fn approving_creates_the_employee_under_the_proposer_and_tells_it() {
    let (_d, s) = setup();
    let t = team(&s);
    let proposer_run = chat_run(&s, t.simon.1);
    assert!(propose(&s, proposer_run, "Eva", None).0);
    // The proposer's own answer is over; the decision starts a new one.
    s.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'succeeded' WHERE id = ?1", [proposer_run.to_string()])?)).unwrap();
    let runs_before = count(&s, "SELECT COUNT(*) FROM runs");

    let done = decide(&s, &proposals(&s, t.project)[0], true, None, None).unwrap();
    let CommandResult::ProposalDecided { status, employee_id: Some(created), .. } = done.receipt.result else { panic!("{:?}", done.receipt.result) };
    assert_eq!(status, ProposalStatus::Approved);
    let (id, title, boss) = employee_row(&s, "Eva").expect("the employee exists");
    assert_eq!((id, title, boss), (created.to_string(), Some("UI lead".to_string()), Some(t.simon.0.to_string())));
    assert!(texts(&s, t.simon.1).iter().any(|m| m.contains("Eva joined as Designer, reporting to Simon.")));
    assert_eq!(count(&s, "SELECT COUNT(*) FROM runs"), runs_before + 1, "the proposer sees the decision");
    let notes = s.store().read(|c| aurelius_storage::timeline::list_notifications(c, 50)).unwrap();
    assert!(notes.iter().all(|n| n.target_type != "employee.proposed"), "a decided proposal leaves the inbox");
    assert_eq!(decide(&s, &proposals(&s, t.project)[0], true, None, None).err().unwrap().code, ErrorCode::Validation, "decided once");
}

#[test]
fn the_user_can_adjust_a_proposal_while_approving_it() {
    let (_d, s) = setup();
    let t = team(&s);
    assert!(propose(&s, chat_run(&s, t.simon.1), "Eva", None).0);
    let edits = ProposalFields { name: "Eve".into(), role: "Lead designer".into(), title: Some("Head of design".into()), instructions: "New text".into(), reports_to: Some(t.jean.0) };
    decide(&s, &proposals(&s, t.project)[0], true, Some(edits), None).unwrap();
    let (_, title, boss) = employee_row(&s, "Eve").expect("the edited employee exists");
    assert_eq!((title, boss), (Some("Head of design".to_string()), Some(t.jean.0.to_string())));
    assert_eq!(employee_row(&s, "Eva"), None);
}

#[test]
fn a_rejection_goes_back_with_the_note_and_creates_nothing() {
    let (_d, s) = setup();
    let t = team(&s);
    assert!(propose(&s, chat_run(&s, t.simon.1), "Eva", None).0);
    let employees_before = count(&s, "SELECT COUNT(*) FROM employees");
    let runs_before = count(&s, "SELECT COUNT(*) FROM runs");
    let done = decide(&s, &proposals(&s, t.project)[0], false, None, Some("Not now")).unwrap();
    assert!(matches!(done.receipt.result, CommandResult::ProposalDecided { status: ProposalStatus::Rejected, employee_id: None, .. }));
    assert!(texts(&s, t.simon.1).iter().any(|m| m == "Rejected: Not now"));
    assert_eq!(count(&s, "SELECT COUNT(*) FROM employees"), employees_before);
    assert_eq!(proposals(&s, t.project)[0].decision_note.as_deref(), Some("Not now"));
    assert_eq!(count(&s, "SELECT COUNT(*) FROM runs"), runs_before, "a run of that conversation is still live: no second one");
}

#[test]
fn an_employee_has_at_most_two_open_proposals() {
    let (_d, s) = setup();
    let t = team(&s);
    let run_id = chat_run(&s, t.simon.1);
    assert!(!propose(&s, run_id, "Hal", Some("Nobody")).0, "an unknown boss is refused");
    assert!(propose(&s, run_id, "Eva", None).0);
    assert!(propose(&s, run_id, "Fay", Some("Jean")).0, "a colleague's name picks the boss");
    assert!(!propose(&s, run_id, "dean", None).0, "a name the company already has");
    let (ok, text) = propose(&s, run_id, "Gil", None);
    assert!(!ok && text.contains("2 proposals"), "{text}");

    decide(&s, &proposals(&s, t.project)[0], false, None, None).unwrap();
    assert!(propose(&s, run_id, "Gil", None).0, "deciding one frees a slot");
}

#[test]
fn the_project_assistant_cannot_propose_a_colleague() {
    let (_d, s) = setup();
    let p = project(&s);
    hire(&s, p, "Ann", None, None);
    let sent = run(&s, Command::SendProjectMessage(SendProjectMessage { confirm_secret: false, project_id: p, text: "hi".into(), conversation_id: None, images: vec![], files: vec![], interrupt: false, start: Default::default() })).unwrap();
    let CommandResult::ProjectMessageSent { run_id, .. } = sent.receipt.result else { panic!() };
    let (ok, text) = propose(&s, run_id, "Eva", None);
    assert!(!ok);
    assert_eq!(text, "Only an employee can propose a colleague.");
    assert!(proposals(&s, p).is_empty());
}

// ── Instruction proposals and versions (Harness² B) ─────────────────────

fn propose_instructions(s: &CommandService, run: RunId, employee: Option<&str>, text: &str, evidence: &[&str]) -> (bool, String) {
    let action = EmployeeAction::ProposeInstructions {
        employee: employee.map(Into::into),
        instructions: text.into(),
        reason: "Two tasks went wrong the same way.".into(),
        evidence: evidence.iter().map(|e| e.to_string()).collect(),
    };
    let (ok, text, _) = s.employee_action(run, action).unwrap();
    (ok, text)
}

fn decide_instructions(s: &CommandService, proposal: &ProposalView, approve: bool, instructions: Option<&str>) -> Result<Committed, AppError> {
    let cmd = DecideProposal { proposal_id: proposal.id, approve, edits: None, note: None, instructions: instructions.map(Into::into) };
    run(s, Command::DecideProposal(cmd))
}

fn instructions_of(s: &CommandService, employee: EmployeeId) -> String {
    let st = s.store().clone();
    s.store()
        .read(move |c| {
            let key: Option<String> = c.query_row("SELECT b.storage_key FROM employees m LEFT JOIN blobs b ON b.id = m.instructions_blob_id WHERE m.id = ?1", [employee.to_string()], |r| r.get(0))?;
            Ok(key.and_then(|k| st.blobs().read(&k).ok()).and_then(|b| String::from_utf8(b).ok()).unwrap_or_default())
        })
        .unwrap()
}

fn restore(s: &CommandService, employee: EmployeeId, version: u32) -> Result<Committed, AppError> {
    run(s, Command::RestoreEmployeeInstructions(aurelius_domain::proposal::RestoreEmployeeInstructions { employee_id: employee, version }))
}

#[test]
fn an_employee_proposes_instructions_for_itself_or_a_direct_report_only() {
    let (_d, s) = setup();
    let t = team(&s);
    task_run(&s, t.project, t.simon.0, "Migration");
    let run_id = chat_run(&s, t.simon.1);
    assert!(crate::run::plan::AUTONOMY_GUIDANCE.contains("propose_instructions only for a lesson your memory shows was confirmed at least twice"));

    let (ok, text) = propose_instructions(&s, run_id, Some("Jean"), "Jean decides faster.", &["Migration"]);
    assert!(!ok && text.contains("does not report to you"), "{text}");
    let (ok, text) = propose_instructions(&s, run_id, None, "Simon works test-first.", &[]);
    assert!(!ok && text.contains("at least one task"), "{text}");
    let (ok, text) = propose_instructions(&s, run_id, None, "Simon works test-first.", &["Nothing like this"]);
    assert!(!ok && text.contains("No task or memory matches"), "{text}");
    let (ok, text) = propose_instructions(&s, run_id, Some("Dean"), "Focus of Dean.", &["Migration"]);
    assert!(!ok && text.contains("already in use"), "{text}");

    let (ok, text) = propose_instructions(&s, run_id, Some("Dean"), "Dean works test-first.", &["Migration"]);
    assert!(ok && text.contains("approves"), "{text}");
    let (ok, text) = propose_instructions(&s, run_id, Some("Dean"), "Dean works test-first, really.", &["Migration"]);
    assert!(!ok && text.contains("already waits"), "one open proposal per employee's instructions: {text}");
    assert!(propose_instructions(&s, run_id, None, "Simon works test-first.", &["Migration"]).0, "another employee's instructions are another slot");

    let pending = proposals(&s, t.project);
    let dean = pending.iter().find(|p| p.target_employee_id == Some(t.dean.0)).expect("Dean's proposal");
    assert_eq!((dean.kind, dean.status, dean.base_version, dean.current_version), (ProposalKind::Instructions, ProposalStatus::Pending, Some(1), Some(1)));
    assert_eq!((dean.name.as_str(), dean.instructions.as_str(), dean.current_instructions.as_deref()), ("Dean", "Dean works test-first.", Some("Focus of Dean.")));
    assert_eq!((dean.evidence[0].kind.as_str(), dean.evidence[0].label.as_str()), ("task", "Migration"));
    assert_eq!(instructions_of(&s, t.dean.0), "Focus of Dean.", "nothing changes without the user");
    let notes = s.store().read(|c| aurelius_storage::timeline::list_notifications(c, 50)).unwrap();
    assert_eq!(notes.iter().filter(|n| n.target_type == "employee.instructions_proposed").count(), 2, "they wait in Needs you");
    // Colleague proposals keep their own two slots.
    assert!(propose(&s, run_id, "Eva", None).0);
}

#[test]
fn approving_with_edits_makes_a_new_version_and_a_new_session() {
    let (_d, s) = setup();
    let t = team(&s);
    task_run(&s, t.project, t.simon.0, "Migration");
    let proposer_run = chat_run(&s, t.simon.1);
    assert!(propose_instructions(&s, proposer_run, Some("Dean"), "Dean works test-first.", &["Migration"]).0);
    s.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'succeeded' WHERE id = ?1", [proposer_run.to_string()])?)).unwrap();
    // Dean's conversation goes on in a session that keeps its system text.
    let dean_conversation = t.dean.1;
    s.store()
        .write(move |tx| {
            Ok(tx.execute(
                "INSERT INTO conversation_sessions(conversation_id, session_id, updated_at_ms, system_text, system_at_ms) VALUES (?1, 's1', ?2, 'You are Dean. Focus of Dean.', ?2)",
                rusqlite::params![dean_conversation.to_string(), aurelius_storage::now_ms()],
            )?)
        })
        .unwrap();
    let runs_before = count(&s, "SELECT COUNT(*) FROM runs");

    let proposal = proposals(&s, t.project).into_iter().find(|p| p.kind == ProposalKind::Instructions).unwrap();
    let done = decide_instructions(&s, &proposal, true, Some("Dean works test-first, in small steps.")).unwrap();
    assert!(matches!(done.receipt.result, CommandResult::ProposalDecided { status: ProposalStatus::Approved, employee_id: None, .. }));
    assert_eq!(instructions_of(&s, t.dean.0), "Dean works test-first, in small steps.", "the user's edit is what applies");
    let versions = s.employee_instruction_versions(t.dean.0).unwrap();
    assert_eq!(versions.current_version, 2);
    assert_eq!((versions.versions[0].source.as_str(), versions.versions[0].proposal_id), ("proposal", Some(proposal.id)));
    assert_eq!((versions.versions[1].version, versions.versions[1].instructions.as_str()), (1, "Focus of Dean."));
    let at: i64 = s.store().read(move |c| Ok(c.query_row("SELECT system_at_ms FROM conversation_sessions WHERE conversation_id = ?1", [dean_conversation.to_string()], |r| r.get(0))?)).unwrap();
    assert_eq!(at, 1, "the session's system text is out of date: the next run starts a new session");
    assert!(texts(&s, t.simon.1).iter().any(|m| m.contains("Approved with the user's changes") && m.contains("version 2")), "{:?}", texts(&s, t.simon.1));
    assert_eq!(count(&s, "SELECT COUNT(*) FROM runs"), runs_before + 1, "the proposer sees the decision");
    let notes = s.store().read(|c| aurelius_storage::timeline::list_notifications(c, 50)).unwrap();
    assert!(notes.iter().all(|n| n.target_type != "employee.instructions_proposed"), "a decided proposal leaves Needs you");
    assert_eq!(decide_instructions(&s, &proposal, true, None).err().unwrap().code, ErrorCode::Validation, "decided once");
}

#[test]
fn a_stale_proposal_conflicts_and_an_old_version_can_be_restored() {
    let (_d, s) = setup();
    let t = team(&s);
    task_run(&s, t.project, t.simon.0, "Migration");
    let run_id = chat_run(&s, t.simon.1);
    assert!(propose_instructions(&s, run_id, None, "Simon works test-first.", &["Migration"]).0);
    // Meanwhile the user changes Simon's instructions themselves.
    let update = UpdateEmployee { employee_id: t.simon.0, name: "Simon".into(), role: "Dev".into(), title: None, instructions: Some("Simon writes docs first.".into()), sequential_tasks: None };
    run(&s, Command::UpdateEmployee(update.clone())).unwrap();
    run(&s, Command::UpdateEmployee(update)).unwrap();
    assert_eq!(s.employee_instruction_versions(t.simon.0).unwrap().current_version, 2, "the same text again is no new version");

    let proposal = proposals(&s, t.project).into_iter().find(|p| p.kind == ProposalKind::Instructions).unwrap();
    assert_eq!((proposal.base_version, proposal.current_version), (Some(1), Some(2)));
    let conflict = decide_instructions(&s, &proposal, true, None).err().unwrap();
    assert_eq!(conflict.code, ErrorCode::VersionConflict);
    assert_eq!(instructions_of(&s, t.simon.0), "Simon writes docs first.");
    assert_eq!(proposals(&s, t.project)[0].status, ProposalStatus::Pending, "the user can still turn it down");
    decide_instructions(&s, &proposal, false, None).unwrap();

    // Version 1 back: a new version, history kept.
    let done = restore(&s, t.simon.0, 1).unwrap();
    assert!(matches!(done.receipt.result, CommandResult::EmployeeInstructionsRestored { version: 3, .. }), "{:?}", done.receipt.result);
    assert_eq!(instructions_of(&s, t.simon.0), "Focus of Simon.");
    let versions = s.employee_instruction_versions(t.simon.0).unwrap();
    assert_eq!(versions.versions.iter().map(|v| (v.version, v.source.as_str())).collect::<Vec<_>>(), vec![(3, "restored"), (2, "edited"), (1, "created")]);
    assert_eq!(versions.versions[0].restored_from, Some(1));
    assert_eq!(restore(&s, t.simon.0, 3).err().unwrap().code, ErrorCode::Validation, "the version in use");
    assert_eq!(restore(&s, t.simon.0, 9).err().unwrap().code, ErrorCode::Validation, "no such version");
}

// ── Management chooses its own mode (feature management_chooses_mode) ──

/// Ann is the CEO, Bob reports to her (management too), Dan reports to Bob (not management).
fn mode_company(s: &CommandService) -> (ProjectId, EmployeeId, EmployeeId, EmployeeId) {
    let p = project(s);
    let (ann, _) = hire(s, p, "Ann", None, None);
    let (bob, _) = hire(s, p, "Bob", None, Some(ann));
    let (dan, _) = hire(s, p, "Dan", None, Some(bob));
    (p, ann, bob, dan)
}

fn set_mode(s: &CommandService, run: RunId, mode: &str) -> (bool, String) {
    let (ok, text, _) = s.employee_action(run, EmployeeAction::SetMode { mode: mode.into(), reason: "Small and reversible.".into() }).unwrap();
    (ok, text)
}

fn mode_of_run(s: &CommandService, run: RunId) -> AgentMode {
    s.store().read(move |c| aurelius_storage::conversations::mode_for_run(c, run)).unwrap()
}

fn task_with_mode(s: &CommandService, project: ProjectId, employee: EmployeeId, mode: Option<AgentMode>) -> RunId {
    let create = CreateTask { confirm_secret: false, project_id: project, employee_id: Some(employee), title: "T".into(), goal: "Do it".into(), acceptance_criteria: vec![], depends_on: vec![], run: aurelius_domain::task::RunSettings { mode, ..Default::default() }, files: vec![] };
    let CommandResult::TaskCreated { task_id } = run(s, Command::CreateTask(create)).unwrap().receipt.result else { panic!() };
    let run_id = start_task(s, task_id, employee);
    s.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'running' WHERE id = ?1", [run_id.to_string()])?)).unwrap();
    run_id
}

fn turn_setting(s: &CommandService, on: bool) {
    s.store().write(move |tx| aurelius_storage::management_mode::set_enabled(tx, on, 1)).unwrap();
}

/// The user switches the conversation of a run to `mode`.
fn user_sets_mode(s: &CommandService, run_id: RunId, mode: AgentMode) {
    let conversation: String = s.store().read(move |c| Ok(c.query_row("SELECT conversation_id FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0))?)).unwrap();
    run(s, Command::SetAgentMode(SetAgentMode { conversation_id: ConversationId::parse(&conversation).unwrap(), mode })).unwrap();
}

#[test]
fn a_management_task_without_a_mode_starts_in_plan_and_an_explicit_mode_stays() {
    let (_d, s) = setup();
    let (p, ann, bob, dan) = mode_company(&s);
    assert_eq!(mode_of_run(&s, task_with_mode(&s, p, bob, None)), AgentMode::Plan);
    assert_eq!(mode_of_run(&s, task_with_mode(&s, p, ann, None)), AgentMode::Plan);
    assert_eq!(mode_of_run(&s, task_with_mode(&s, p, bob, Some(AgentMode::Auto))), AgentMode::Auto);
    assert_eq!(mode_of_run(&s, task_with_mode(&s, p, bob, Some(AgentMode::Ask))), AgentMode::Ask);
    // Not management: the old default.
    assert_eq!(mode_of_run(&s, task_with_mode(&s, p, dan, None)), AgentMode::Ask);
    // Setting off: the old default for management too.
    turn_setting(&s, false);
    assert_eq!(mode_of_run(&s, task_with_mode(&s, p, bob, None)), AgentMode::Ask);
}

#[test]
fn management_chooses_auto_or_plan_with_a_reason_that_shows_in_the_thread_and_the_audit_log() {
    let (_d, s) = setup();
    let (p, _ann, bob, _dan) = mode_company(&s);
    let run = task_with_mode(&s, p, bob, None);
    let (ok, text) = set_mode(&s, run, "auto");
    assert!(ok, "{text}");
    assert_eq!(mode_of_run(&s, run), AgentMode::Auto);
    let (ok, _) = set_mode(&s, run, "Plan");
    assert!(ok);
    assert_eq!(mode_of_run(&s, run), AgentMode::Plan);
    assert_eq!(count(&s, "SELECT COUNT(*) FROM events WHERE type = 'task.mode_chosen'"), 2);
    assert_eq!(count(&s, "SELECT COUNT(*) FROM messages WHERE json_extract(meta_json, '$.kind') = 'mode_chosen'"), 2);
}

#[test]
fn set_mode_is_refused_outside_its_rules() {
    let (_d, s) = setup();
    let (p, ann, bob, dan) = mode_company(&s);
    // Only management.
    let (ok, why) = set_mode(&s, task_with_mode(&s, p, dan, None), "auto");
    assert!(!ok && why.contains("management"), "{why}");
    // Only Plan or Auto.
    let run = task_with_mode(&s, p, bob, None);
    for mode in ["ask", "accept_edits", "", "yolo"] {
        let (ok, why) = set_mode(&s, run, mode);
        assert!(!ok && why.contains("auto or plan"), "{mode}: {why}");
    }
    assert_eq!(mode_of_run(&s, run), AgentMode::Plan);
    // Not in a chat, only in the task's own conversation.
    let chat: String = s.store().read(move |c| Ok(c.query_row("SELECT id FROM conversations WHERE employee_id = ?1 AND kind = 'employee'", [ann.to_string()], |r| r.get(0))?)).unwrap();
    let (ok, why) = set_mode(&s, chat_run(&s, ConversationId::parse(&chat).unwrap()), "auto");
    assert!(!ok && why.contains("task"), "{why}");
    // A mode the user chose for the task, or switched to, stays.
    let explicit = task_with_mode(&s, p, bob, Some(AgentMode::Ask));
    let (ok, why) = set_mode(&s, explicit, "auto");
    assert!(!ok && why.contains("user"), "{why}");
    let switched = task_with_mode(&s, p, ann, None);
    user_sets_mode(&s, switched, AgentMode::AcceptEdits);
    let (ok, why) = set_mode(&s, switched, "auto");
    assert!(!ok && why.contains("user"), "{why}");
    // The setting off.
    turn_setting(&s, false);
    let (ok, why) = set_mode(&s, run, "auto");
    assert!(!ok && why.contains("turned off"), "{why}");
    assert_eq!(count(&s, "SELECT COUNT(*) FROM events WHERE type = 'task.mode_chosen'"), 0);
}

#[test]
fn set_mode_is_refused_while_a_plan_waits_for_the_user() {
    let (_d, s) = setup();
    let (p, _ann, bob, _dan) = mode_company(&s);
    let run = task_with_mode(&s, p, bob, None);
    let input = serde_json::json!({ "plan": "# Plan\n\n1. Do it" });
    match s.request_runtime_permission(run, Some("plan-1".into()), "ExitPlanMode", &input).unwrap() {
        crate::run::approvals::RuntimePermission::Pending { .. } => {}
        crate::run::approvals::RuntimePermission::Denied(why) => panic!("{why}"),
    }
    let (ok, why) = set_mode(&s, run, "auto");
    assert!(!ok && why.contains("approval"), "{why}");
    assert_eq!(mode_of_run(&s, run), AgentMode::Plan);
}

#[test]
fn delegation_to_management_inherits_no_mode_but_to_others_it_does() {
    let (_d, s) = setup();
    let (p, ann, bob, _dan) = mode_company(&s);
    shared_ticket(&s, p);
    let child_mode = |s: &CommandService| -> Option<AgentMode> {
        s.store()
            .read(|c| {
                let id: String = c.query_row("SELECT id FROM tasks ORDER BY created_at_ms DESC, rowid DESC LIMIT 1", [], |r| r.get(0))?;
                Ok(aurelius_storage::tasks::task_run(c, TaskId::parse(&id).unwrap())?.mode)
            })
            .unwrap()
    };
    // The CEO works in Auto and gives a part to the CTO: nothing is passed on.
    let ceo_run = task_with_mode(&s, p, ann, None);
    user_sets_mode(&s, ceo_run, AgentMode::Auto);
    let (ok, text) = delegate(&s, ceo_run, "Bob");
    assert!(ok, "{text}");
    assert_eq!(child_mode(&s), None);
    // Bob chooses Auto himself and gives a part to Dan, who is no management: Auto goes along.
    let bob_run = task_with_mode(&s, p, bob, None);
    assert!(set_mode(&s, bob_run, "auto").0);
    let (ok, text) = delegate(&s, bob_run, "Dan");
    assert!(ok, "{text}");
    assert_eq!(child_mode(&s), Some(AgentMode::Auto));
    // With the setting off the old rule holds for management as well.
    turn_setting(&s, false);
    let (ok, text) = delegate(&s, ceo_run, "Bob");
    assert!(ok, "{text}");
    assert_eq!(child_mode(&s), Some(AgentMode::Auto));
}

#[test]
fn the_choice_rule_is_in_the_text_management_gets() {
    let rule = crate::run::plan::MODE_CHOICE_GUIDANCE;
    for needle in ["set_mode", "Plan mode", "Auto", "reason", "ExitPlanMode", "budget", "irreversible", "external"] {
        assert!(rule.contains(needle), "{needle}");
    }
}
