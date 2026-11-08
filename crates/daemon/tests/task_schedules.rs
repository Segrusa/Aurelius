//! Files and a multi-line goal with a new task, a task's own model and effort,
//! and recurring tasks through the client.
//! (When schedules fire is tested with a controllable clock in the
//! application crate.)

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ProjectId, TaskId,
    autonomy::AgentMode,
    connection::{ConfigScope, Purpose, RunDefaults, SetRunDefaults},
    employee::{CreateEmployee, Setting},
    project::CreateProject,
    schedule::{CreateTaskSchedule, DeleteTaskSchedule, Recurrence, RunTaskScheduleNow, SetTaskSchedulePaused, UpdateTaskSchedule},
    task::{CreateTask, RunSettings},
};
use aurelius_protocol::{CommandResult, TaskView};
use common::*;
use std::path::Path;
use std::time::Duration;

async fn exec(client: &Client, command: Command) -> CommandResult {
    client.execute(envelope(command, None)).await.unwrap().result
}

struct World {
    project: ProjectId,
    employee: aurelius_domain::EmployeeId,
    work: std::path::PathBuf,
}

async fn world(client: &Client, root: &Path) -> World {
    let work = root.join("werk");
    std::fs::create_dir_all(&work).unwrap();
    let CommandResult::ProjectCreated { project_id, .. } =
        exec(client, Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: Some(work.display().to_string()) })).await
    else {
        panic!()
    };
    let CommandResult::EmployeeCreated { employee_id, .. } =
        exec(client, Command::CreateEmployee(CreateEmployee { access_level: None, description: None, name: "Alex".into(), role: "Onderzoeker".into(), title: None, reports_to: None, instructions: None, folder: None })).await
    else {
        panic!()
    };
    let settings = RunDefaults { connection: Setting::Inherit, model: Setting::Value("fake-employee-model".into()), persona: Setting::Inherit, effort: Setting::Value("high".into()) };
    exec(client, Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Employee(employee_id), purpose: Purpose::Employee, settings })).await;
    World { project: project_id, employee: employee_id, work }
}

async fn finished(client: &Client, id: TaskId) -> TaskView {
    for _ in 0..400 {
        let t = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == id).unwrap();
        if t.status.is_terminal() {
            return t;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the task did not finish");
}

fn run_config(root: &Path, task: TaskId) -> serde_json::Value {
    let text: String = db(root)
        .query_row(
            "SELECT r.resolved_config_json FROM runs r JOIN tasks t ON t.conversation_id = r.conversation_id WHERE t.id = ?1 ORDER BY r.rowid LIMIT 1",
            [task.to_string()],
            |r| r.get(0),
        )
        .expect("a run of the task's conversation");
    serde_json::from_str(&text).unwrap()
}

fn new_task(w: &World, title: &str, goal: &str) -> CreateTask {
    CreateTask {
        confirm_secret: false,
        files: vec![],
        run: Default::default(),
        project_id: w.project,
        employee_id: Some(w.employee),
        title: title.into(),
        goal: goal.into(),
        acceptance_criteria: vec![],
        depends_on: vec![],
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_takes_files_a_multi_line_goal_and_its_own_model_and_effort() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let w = world(&client, dir.path()).await;
    let note = dir.path().join("notities.txt");
    std::fs::write(&note, "hallo").unwrap();
    let shot = dir.path().join("scherm.png");
    std::fs::write(&shot, [137, 80, 78, 71]).unwrap();

    let goal = "Regel een\n\n  ingesprongen regel\r\nLaatste regel";
    let mut cmd = new_task(&w, "Rapport", goal);
    cmd.files = vec![note.display().to_string(), shot.display().to_string()];
    cmd.run = RunSettings { mode: Some(AgentMode::AcceptEdits), connection_id: None, model: Some("fake-task-model".into()), effort: Some("low".into()), ..Default::default() };
    let CommandResult::TaskCreated { task_id } = client.create_task(cmd).await.unwrap().result else { panic!() };
    let plain = new_task(&w, "Gewoon", "Zonder eigen instellingen");
    let CommandResult::TaskCreated { task_id: plain_id } = exec(&client, Command::CreateTask(plain)).await else { panic!() };

    let t = finished(&client, task_id).await;
    assert_eq!(t.goal, goal, "the goal is kept exactly as typed, newlines included");
    assert_eq!(t.attachments.iter().map(|a| (a.name.as_str(), a.path.as_str(), a.byte_length)).collect::<Vec<_>>(), [
        ("notities.txt", ".aurelius/uploads/notities.txt", 5),
        ("scherm.png", ".aurelius/uploads/scherm.png", 4),
    ]);
    assert_eq!(std::fs::read_to_string(w.work.join(".aurelius/uploads/notities.txt")).unwrap(), "hallo");
    assert_eq!(std::fs::read_to_string(w.work.join(".aurelius/uploads/.gitignore")).unwrap(), "*\n", "git ignores the uploads");
    assert_eq!(t.run.model.as_deref(), Some("fake-task-model"));

    // The conversation opens with the goal as typed and tells the employee where the files are.
    let conversation = t.conversation_id.expect("the task's conversation");
    let messages = client.messages(conversation).await.unwrap();
    assert_eq!(messages.agent_mode, AgentMode::AcceptEdits);
    let brief = messages.messages.iter().find(|m| m.text.contains("**Goal**")).expect("the brief");
    assert!(brief.text.contains("Regel een\n\n  ingesprongen regel\r\nLaatste regel"), "{}", brief.text);
    assert!(brief.text.contains("notities.txt") && brief.text.contains("scherm.png") && brief.text.contains(".aurelius"), "{}", brief.text);

    // The task's own model and effort win over the employee's; without them the employee's apply.
    let config = run_config(dir.path(), task_id);
    assert_eq!((config["model"].as_str(), config["effort"].as_str()), (Some("fake-task-model"), Some("low")));
    assert_eq!(config["model_origin"], "task Rapport");
    finished(&client, plain_id).await;
    let config = run_config(dir.path(), plain_id);
    assert_eq!((config["model"].as_str(), config["effort"].as_str()), (Some("fake-employee-model"), Some("high")));

    // Too many files, or one that is missing, is refused and nothing is made.
    let tasks_before = client.tasks(None).await.unwrap().tasks.len();
    let mut many = new_task(&w, "Te veel", "x");
    many.files = vec![note.display().to_string(); 9];
    assert!(client.create_task(many).await.is_err());
    let mut missing = new_task(&w, "Weg", "x");
    missing.files = vec![dir.path().join("bestaat-niet.bin").display().to_string()];
    assert!(client.create_task(missing).await.is_err());
    assert_eq!(client.tasks(None).await.unwrap().tasks.len(), tasks_before);

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn recurring_tasks_are_listed_changed_paused_run_and_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let w = world(&client, dir.path()).await;
    let note = dir.path().join("bron.txt");
    std::fs::write(&note, "data").unwrap();

    let create = CreateTaskSchedule {
        confirm_secret: false,
        project_id: w.project,
        employee_id: w.employee,
        title: "Weekrapport".into(),
        goal: "Regel een\nRegel twee".into(),
        acceptance_criteria: vec!["Kort".into()],
        files: vec![note.display().to_string()],
        run: RunSettings { model: Some("fake-task-model".into()), effort: Some("low".into()), ..RunSettings::default() },
        recurrence: Recurrence::Weekly { weekdays: vec![4, 0], at_minute: 9 * 60 },
        paused: false,
    };
    let CommandResult::TaskScheduleCreated { schedule_id } = client.create_task_schedule(create).await.unwrap().result else { panic!() };
    let list = client.task_schedules(Some(w.project)).await.unwrap().schedules;
    assert_eq!(list.len(), 1);
    let s = &list[0];
    assert_eq!((s.id, s.employee_name.as_str(), s.goal.as_str()), (schedule_id, "Alex", "Regel een\nRegel twee"));
    assert_eq!(s.recurrence, Recurrence::Weekly { weekdays: vec![0, 4], at_minute: 540 });
    assert!(!s.paused && s.next_run_ms.is_some() && s.last_run_ms.is_none());
    assert_eq!(s.attachments[0].path, ".aurelius/uploads/bron.txt");
    assert!(client.task_schedules(Some(ProjectId::new())).await.unwrap().schedules.is_empty());

    // Run now: a normal task from the template, with its files, that runs with the template's model.
    let CommandResult::TaskScheduleRan { task_id, .. } = exec(&client, Command::RunTaskScheduleNow(RunTaskScheduleNow { schedule_id })).await else { panic!() };
    let t = finished(&client, task_id).await;
    assert_eq!((t.title.as_str(), t.goal.as_str(), t.schedule_id), ("Weekrapport", "Regel een\nRegel twee", Some(schedule_id)));
    assert_eq!(t.attachments.len(), 1);
    assert_eq!(run_config(dir.path(), task_id)["model"], "fake-task-model");
    let s = client.task_schedules(None).await.unwrap().schedules.remove(0);
    assert_eq!((s.last_task_id, s.open_task_id), (Some(task_id), None));

    // Pause and resume.
    exec(&client, Command::SetTaskSchedulePaused(SetTaskSchedulePaused { schedule_id, paused: true })).await;
    let s = client.task_schedules(None).await.unwrap().schedules.remove(0);
    assert!(s.paused && s.next_run_ms.is_none());
    exec(&client, Command::SetTaskSchedulePaused(SetTaskSchedulePaused { schedule_id, paused: false })).await;
    assert!(client.task_schedules(None).await.unwrap().schedules[0].next_run_ms.is_some());

    // Update: other text and recurrence, the file dropped.
    let update = UpdateTaskSchedule {
        confirm_secret: false,
        schedule_id,
        employee_id: w.employee,
        title: "Dagrapport".into(),
        goal: "Nieuw".into(),
        acceptance_criteria: vec![],
        add_files: vec![],
        remove_files: vec![".aurelius/uploads/bron.txt".into()],
        run: Default::default(),
        recurrence: Recurrence::EveryHours { hours: 6 },
    };
    client.update_task_schedule(update).await.unwrap();
    let s = client.task_schedules(None).await.unwrap().schedules.remove(0);
    assert_eq!((s.title.as_str(), s.recurrence.clone(), s.attachments.len()), ("Dagrapport", Recurrence::EveryHours { hours: 6 }, 0));

    exec(&client, Command::DeleteTaskSchedule(DeleteTaskSchedule { schedule_id })).await;
    assert!(client.task_schedules(None).await.unwrap().schedules.is_empty());
    assert!(client.tasks(None).await.unwrap().tasks.iter().any(|t| t.id == task_id), "the task it made stays");

    stop(dir.path(), daemon).await;
}
