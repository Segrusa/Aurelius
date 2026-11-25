//! Quick tasks (ticket #53): the database side. The git work is the daemon's
//! (`daemon/src/quick_tests.rs` tests it against real repositories).

use super::*;
use crate::*;
use aurelius_domain::company::{SetTester, UpdateCompany};
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{AcceptTicket, CancelTicket, TicketView};
use aurelius_domain::{EmployeeId, RunId, TaskId};
use aurelius_storage::Store;
use std::sync::Arc;

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    /// A project whose folder is a git repository.
    project: aurelius_domain::ProjectId,
    /// A project whose folder is not.
    plain: aurelius_domain::ProjectId,
    user: UserId,
    orion: EmployeeId,
    ann: EmployeeId,
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: aurelius_domain::PrincipalId::new(), client_id: aurelius_domain::ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: user }
}

fn exec(s: &Setup, command: Command) -> Result<CommandResult, AppError> {
    s.service.execute(&ctx(Some(s.user)), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command }).map(|c| c.receipt.result)
}

fn make_project(service: &CommandService, user: UserId, name: &str, folder: &std::path::Path) -> aurelius_domain::ProjectId {
    std::fs::create_dir_all(folder).unwrap();
    let command = Command::CreateProject(CreateProject { name: name.into(), description: None, folder: Some(folder.to_string_lossy().into_owned()) });
    let made = service.execute(&ctx(Some(user)), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command }).unwrap().receipt.result;
    let CommandResult::ProjectCreated { project_id, .. } = made else { panic!("{made:?}") };
    project_id
}

fn hire(service: &CommandService, user: UserId, name: &str, boss: Option<EmployeeId>) -> EmployeeId {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Dev".into(), title: None, reports_to: boss, instructions: None, folder: None };
    let command = Command::CreateEmployee(create);
    let made = service.execute(&ctx(Some(user)), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command }).unwrap().receipt.result;
    let CommandResult::EmployeeCreated { employee_id, .. } = made else { panic!("{made:?}") };
    employee_id
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(&dir.path().join("data")).unwrap()));
    let user = UserId::new();
    service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &user.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC))
        .unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let project = make_project(&service, user, "Atlas", &repo);
    let plain = make_project(&service, user, "Plain", &dir.path().join("plain"));
    let atlas = hire(&service, user, "Atlas", None);
    let orion = hire(&service, user, "Orion", Some(atlas));
    let ann = hire(&service, user, "Ann", Some(orion));
    Setup { _dir: dir, service, project, plain, user, orion, ann }
}

fn create(project: aurelius_domain::ProjectId, kind: Option<TicketKind>, owner: Option<EmployeeId>, title: &str, description: Option<&str>) -> Command {
    Command::CreateTicket(CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id: project,
        title: title.into(),
        description: description.map(str::to_owned),
        priority: Default::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: owner,
        run: Default::default(),
        files: vec![],
        start: kind.is_none(),
        from_message: None,
        from_idea: None,
        kind,
    })
}

fn quick(s: &Setup) -> TicketId {
    let made = exec(s, create(s.project, Some(TicketKind::Quick), Some(s.orion), "", Some("Fix the footer\n\nThe year is wrong."))).unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made else { panic!("{made:?}") };
    ticket_id
}

fn normal(s: &Setup) -> TicketId {
    let made = exec(s, create(s.project, None, Some(s.orion), "Normal work", Some("Why"))).unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made else { panic!("{made:?}") };
    ticket_id
}

fn view(s: &Setup, id: TicketId) -> TicketView {
    let store = s.service.store().clone();
    store.read(|c| store::view(c, id, &|k| store.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok()))).unwrap().unwrap()
}

fn root(s: &Setup, id: TicketId) -> TaskId {
    view(s, id).root_task_id.expect("a main task")
}

fn jobs(s: &Setup, kind: &'static str) -> u32 {
    s.service.store().read(move |c| Ok(c.query_row("SELECT COUNT(*) FROM jobs WHERE kind = ?1", [kind], |r| r.get(0))?)).unwrap()
}

fn lines(s: &Setup, id: TicketId) -> Vec<String> {
    let conversation = view(s, id).conversation_id.unwrap();
    let store = s.service.store().clone();
    store
        .read(|c| aurelius_storage::conversations::list_messages(c, conversation, &|k| store.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok())))
        .unwrap()
        .into_iter()
        .map(|m| m.text)
        .collect()
}

/// The main task as the engine moves it: through Running to `status` (the card follows).
fn root_goes(s: &Setup, id: TicketId, status: TaskStatus) {
    let task = root(s, id);
    s.service
        .store()
        .write(move |tx| {
            if aurelius_storage::tasks::load_task(tx, task)?.map(|r| r.status) != Some(TaskStatus::Running) {
                let _ = crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?;
            }
            let _ = crate::task::task_line::set_status(tx, task, status, None)?;
            Ok(())
        })
        .unwrap();
}

fn accept(s: &Setup, id: TicketId) -> Result<CommandResult, AppError> {
    let accept: AcceptTicket = serde_json::from_value(json!({ "ticket_id": id })).unwrap();
    exec(s, Command::AcceptTicket(accept))
}

/// A run of `employee` in the main task of `ticket` (as the bridge's tool calls come in).
fn task_run(s: &Setup, ticket: TicketId, employee: EmployeeId) -> RunId {
    let (run, task) = (RunId::new(), root(s, ticket));
    s.service
        .store()
        .write(move |tx| {
            let chat = aurelius_storage::employees::main_chat(tx, employee)?.expect("an employee has a main chat");
            tx.execute(
                "INSERT INTO runs(id, conversation_id, task_id, attempt, runtime_kind, model_id, resolved_config_json, status, started_at_ms) VALUES (?1, ?2, ?3, 1, 'fake', '', '{}', 'running', 1)",
                rusqlite::params![run.to_string(), chat.to_string(), task.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    run
}

fn delegate(s: &Setup, run: RunId) -> (bool, String) {
    let action = crate::task::task_runs::EmployeeAction::Delegate { employee: "Ann".into(), title: "Part".into(), goal: "Do a part".into(), criteria: vec![], ticket: None };
    let (ok, text, _) = s.service.employee_action(run, action).unwrap();
    (ok, text)
}

#[test]
fn a_quick_task_starts_in_doing_with_its_employee_in_auto_and_without_the_dispatcher() {
    let s = setup();
    let id = quick(&s);
    let v = view(&s, id);
    assert!(v.is_quick());
    assert_eq!(v.title, "Fix the footer", "the title is the description's first line");
    assert_eq!((v.column, v.owner_employee_id, v.responsible_user_id), (ColumnKind::Doing, Some(s.orion), Some(s.user)));
    assert_eq!(v.run.mode, Some(AgentMode::Auto));
    assert!(v.started_at_ms.is_some());
    assert_eq!(jobs(&s, crate::task::triage::TRIAGE_JOB_KIND), 0, "no dispatcher");
    let task = root(&s, id);
    let (status, employee) = s.service.store().read(move |c| aurelius_storage::tasks::status_and_employee(c, task)).unwrap().unwrap();
    assert_eq!((status, employee), (TaskStatus::Queued, Some(s.orion)), "it goes to its employee at once");
    assert_eq!(s.service.store().read(move |c| aurelius_storage::tasks::task_run(c, task)).unwrap().mode, Some(AgentMode::Auto), "never Plan");
    assert!(lines(&s, id).iter().any(|l| l.contains("quick/ticket-") && l.contains("Orion")), "{:?}", lines(&s, id));
}

#[test]
fn a_quick_task_needs_an_employee_a_description_auto_and_a_git_folder() {
    let s = setup();
    let no_owner = exec(&s, create(s.project, Some(TicketKind::Quick), None, "x", Some("Fix"))).unwrap_err();
    assert_eq!(no_owner.message, OWNER_REQUIRED);
    let no_text = exec(&s, create(s.project, Some(TicketKind::Quick), Some(s.orion), "x", None)).unwrap_err();
    assert_eq!(no_text.message, DESCRIPTION_REQUIRED);
    let no_git = exec(&s, create(s.plain, Some(TicketKind::Quick), Some(s.orion), "x", Some("Fix"))).unwrap_err();
    assert_eq!(no_git.message, NO_GIT_FOLDER);
    let Command::CreateTicket(mut plan) = create(s.project, Some(TicketKind::Quick), Some(s.orion), "x", Some("Fix")) else { unreachable!("a create") };
    plan.run.mode = Some(AgentMode::Plan);
    assert_eq!(exec(&s, Command::CreateTicket(plan)).unwrap_err().message, PLAN_REFUSED);
    // A normal ticket in a project without git is as before.
    assert!(exec(&s, create(s.plain, None, None, "Normal", None)).is_ok());
}

#[test]
fn a_quick_tasks_employee_cannot_delegate_but_a_normal_tickets_can() {
    let s = setup();
    let quick = quick(&s);
    let (ok, text) = delegate(&s, task_run(&s, quick, s.orion));
    assert_eq!((ok, text.as_str()), (false, DELEGATE_REFUSED));
    let normal = normal(&s);
    let (ok, text) = delegate(&s, task_run(&s, normal, s.orion));
    assert!(ok, "a normal ticket delegates as before: {text}");
}

#[test]
fn a_quick_task_reports_to_review_also_with_a_tester_and_a_normal_ticket_to_testing() {
    let s = setup();
    exec(&s, Command::UpdateCompany(UpdateCompany { name: None, dispatcher: None, tester: Some(SetTester { employee_id: Some(s.ann) }) })).unwrap();
    let quick = quick(&s);
    root_goes(&s, quick, TaskStatus::Review);
    assert_eq!(view(&s, quick).column, ColumnKind::Review);
    let normal = normal(&s);
    root_goes(&s, normal, TaskStatus::Review);
    assert_eq!(view(&s, normal).column, ColumnKind::Testing, "a normal ticket is tested as before");
}

#[test]
fn approving_a_quick_task_queues_its_merge_and_keeps_it_in_review_until_the_merge_worked() {
    let s = setup();
    let id = quick(&s);
    root_goes(&s, id, TaskStatus::Review);
    let CommandResult::TicketChanged { column, .. } = accept(&s, id).unwrap() else { panic!("changed") };
    assert_eq!(column, ColumnKind::Review, "not Done before the merge");
    assert_eq!(jobs(&s, QUICK_MERGE_JOB_KIND), 1);
    assert!(lines(&s, id).iter().any(|l| l.contains("merging quick/ticket-")));
    assert_eq!(accept(&s, id).unwrap_err().message, MERGE_PENDING, "one merge at a time");

    let (done, _) = s.service.quick_merge_finished(id, s.user, MergeOutcome::Merged("0123456789abcdef".into())).unwrap();
    assert!(done, "the worktree and branch may go");
    let v = view(&s, id);
    assert_eq!((v.column, v.merge_commit.as_deref()), (ColumnKind::Done, Some("0123456789abcdef")));
    assert_eq!(v.root_task_status, Some(TaskStatus::Succeeded));
    assert!(lines(&s, id).iter().any(|l| l.contains("Merged quick/ticket-") && l.contains("0123456789")));
}

#[test]
fn a_failed_merge_keeps_the_quick_task_in_review_and_names_the_reason_and_files() {
    let s = setup();
    let id = quick(&s);
    root_goes(&s, id, TaskStatus::Review);
    accept(&s, id).unwrap();
    let failed = MergeOutcome::Failed { reason: "a merge conflict".into(), files: vec!["src/a.rs".into()] };
    let (done, _) = s.service.quick_merge_finished(id, s.user, failed).unwrap();
    assert!(!done);
    let v = view(&s, id);
    assert_eq!((v.column, v.root_task_status, v.merge_commit), (ColumnKind::Review, Some(TaskStatus::Review), None));
    assert!(lines(&s, id).iter().any(|l| l.contains("a merge conflict") && l.contains("src/a.rs")));
    // The board member may approve again once it is solved.
    s.service.store().write(|tx| Ok(tx.execute("UPDATE jobs SET state = 'done' WHERE kind = ?1", [QUICK_MERGE_JOB_KIND])?)).unwrap();
    assert!(accept(&s, id).is_ok());
}

#[test]
fn a_quick_task_without_commits_is_a_plain_done() {
    let s = setup();
    let id = quick(&s);
    root_goes(&s, id, TaskStatus::Review);
    accept(&s, id).unwrap();
    let (done, _) = s.service.quick_merge_finished(id, s.user, MergeOutcome::NoCommits).unwrap();
    assert!(done);
    let v = view(&s, id);
    assert_eq!((v.column, v.merge_commit), (ColumnKind::Done, None));
    assert!(lines(&s, id).iter().any(|l| l.contains("no commits")));
}

#[test]
fn a_normal_ticket_is_accepted_straight_to_done_without_a_merge() {
    let s = setup();
    exec(&s, Command::UpdateCompany(UpdateCompany { name: None, dispatcher: None, tester: Some(SetTester { employee_id: None }) })).unwrap();
    let id = normal(&s);
    root_goes(&s, id, TaskStatus::Review);
    let CommandResult::TicketChanged { column, .. } = accept(&s, id).unwrap() else { panic!("changed") };
    assert_eq!(column, ColumnKind::Done);
    assert_eq!(jobs(&s, QUICK_MERGE_JOB_KIND), 0);
    assert_eq!(jobs(&s, QUICK_CLEANUP_JOB_KIND), 1, "its worktrees go (ticket #86)");
    assert!(view(&s, id).kind.is_none(), "its view is as before");
}

#[test]
fn cancelling_a_quick_task_or_a_normal_ticket_queues_the_cleanup_of_its_worktrees() {
    let s = setup();
    let normal = normal(&s);
    exec(&s, Command::CancelTicket(CancelTicket { ticket_id: normal })).unwrap();
    assert_eq!(jobs(&s, QUICK_CLEANUP_JOB_KIND), 1, "a normal ticket's worktrees go too (ticket #86)");
    assert!(s.service.quick_repo(normal).unwrap().is_none(), "but it is no quick task: its branches are not deleted with them");
    let id = quick(&s);
    exec(&s, Command::CancelTicket(CancelTicket { ticket_id: id })).unwrap();
    assert_eq!(view(&s, id).column, ColumnKind::Backlog);
    assert_eq!(jobs(&s, QUICK_CLEANUP_JOB_KIND), 2);
    let repo = s.service.quick_repo(id).unwrap().unwrap();
    assert!(!repo.working, "a cancelled quick task gets no new worktree");
    assert_eq!(repo.branch, aurelius_domain::ticket::quick_branch(view(&s, id).number));
}

#[test]
fn a_quick_task_stays_in_auto_when_its_mode_changes() {
    let s = setup();
    let id = quick(&s);
    let update: aurelius_domain::ticket::UpdateTicket = serde_json::from_value(json!({ "ticket_id": id, "run": { "mode": "plan" } })).unwrap();
    assert_eq!(exec(&s, Command::UpdateTicket(update)).unwrap_err().message, PLAN_REFUSED);
}

#[test]
fn the_quick_section_names_the_worktree_and_branch_and_forbids_main_and_push() {
    let dir = tempfile::tempdir().unwrap();
    let text = crate::run::plan::quick_task_guidance(7, dir.path());
    for part in ["quick/ticket-7", &dir.path().display().to_string(), "never push", "delegate_task is refused", "only when the description or the board member asks", "Your report names the branch"] {
        assert!(text.contains(part), "{part}: {text}");
    }
    assert!(crate::run::plan::quick_task_guidance(7, &dir.path().join("missing")).contains("could not be made yet"));
}

/// The History rows of the store, as (number, state).
fn history(s: &Setup) -> Vec<(u32, aurelius_domain::history::HistorySummaryState)> {
    let (rows, _) = s.service.store().read(|c| aurelius_storage::history::list(c, &aurelius_storage::history::Filter::default(), 50, crate::memory::EXTRACT_JOB_KIND)).unwrap();
    rows.into_iter().map(|r| (r.number, r.summary_state)).collect()
}

#[test]
fn a_quick_task_gets_its_summary_from_the_same_extraction_once_its_merge_put_it_on_done() {
    let s = setup();
    let id = quick(&s);
    root_goes(&s, id, TaskStatus::Review);
    accept(&s, id).unwrap();
    // Approved but not merged: not Done, so not in the History and no extraction yet.
    assert!(history(&s).is_empty());
    assert_eq!(jobs(&s, crate::memory::EXTRACT_JOB_KIND), 0);

    s.service.quick_merge_finished(id, s.user, MergeOutcome::Merged("0123456789abcdef".into())).unwrap();
    // H-01 AC1: the main task's one extraction, with the ticket, as for a normal Done.
    let ticket_jobs: u32 = s
        .service
        .store()
        .read(move |c| Ok(c.query_row("SELECT COUNT(*) FROM jobs WHERE kind = ?1 AND json_extract(payload_json, '$.ticket_id') = ?2", [crate::memory::EXTRACT_JOB_KIND, &id.to_string()], |r| r.get(0))?))
        .unwrap();
    assert_eq!(ticket_jobs, 1);
    let number = view(&s, id).number;
    assert_eq!(history(&s), vec![(number, aurelius_domain::history::HistorySummaryState::Summarizing)], "on Done, its summary on the way");
}

#[test]
fn a_tickets_main_task_and_a_delegated_part_each_get_a_worktree_and_the_part_starts_from_its_parents_branch() {
    let s = setup();
    let id = normal(&s);
    let number = view(&s, id).number;
    let main = root(&s, id);
    let worktrees = aurelius_domain::work_place::worktrees_dir(s.service.store().root());
    let (project, ann) = (s.project, s.ann);
    let part = TaskId::new();
    s.service
        .store()
        .write(move |tx| {
            let brief: String = tx.query_row("SELECT brief_blob_id FROM tasks WHERE id = ?1", [main.to_string()], |r| r.get(0))?;
            aurelius_storage::tasks::insert_task(tx, part, project, Some(ann), "A part", &brief, &brief, aurelius_domain::task::TaskStatus::Queued, Some(main))
        })
        .unwrap();
    let places = s.service.store().read(|c| aurelius_storage::work_places::of_ticket_tasks(c, &worktrees, id)).unwrap();
    assert_eq!(places.len(), 2, "{places:?}");
    let (main_place, part_place) = (&places[0], &places[1]);
    assert_eq!((main_place.branch.as_str(), main_place.worktree.clone(), main_place.base.clone()), (format!("ticket/{number}").as_str(), worktrees.join(format!("ticket-{number}")), None));
    assert!(part_place.branch.starts_with(&format!("ticket/{number}-ann-")), "{}", part_place.branch);
    assert_eq!(part_place.base.as_deref(), Some(format!("ticket/{number}").as_str()), "from the main task's work, not from main");
    assert_ne!(part_place.worktree, main_place.worktree);
    assert!(places.iter().all(|p| p.repo != p.worktree && p.make), "never the project's folder");
    // A ticket worked on keeps its worktrees; cancelled, they may go.
    assert!(s.service.ticket_worktrees(id).unwrap().is_empty());
    exec(&s, Command::CancelTicket(CancelTicket { ticket_id: id })).unwrap();
    assert_eq!(s.service.ticket_worktrees(id).unwrap().len(), 2);
}

#[test]
fn a_task_in_a_project_without_a_git_folder_still_works_in_that_folder() {
    let s = setup();
    let made = exec(&s, create(s.plain, None, Some(s.orion), "Plain work", Some("Why"))).unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made else { panic!("{made:?}") };
    let worktrees = aurelius_domain::work_place::worktrees_dir(s.service.store().root());
    let main = root(&s, ticket_id);
    assert!(s.service.store().read(|c| aurelius_storage::work_places::of_task(c, &worktrees, main)).unwrap().is_none());
}
