//! Harness² A: lessons from what went wrong in a task. A task without signals
//! gets no lesson; a failed task gets one (from the cheap extraction, with the
//! signal as its evidence), and a second failure reinforces it instead of
//! making a double.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, EmployeeId, ProjectId, TaskId,
    employee::CreateEmployee,
    memory::{MemoryKind, MemoryStatus},
    project::CreateProject,
    task::{CreateTask, TaskStatus},
};
use aurelius_protocol::{CommandResult, MemoryView};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, fake};
use common::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// The fake, but while `failing` an employee's task runs fail halfway; the
/// memory jobs (extraction, summaries) always work.
struct FailingTasks {
    failing: AtomicBool,
}

#[async_trait::async_trait]
impl AgentRuntime for FailingTasks {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let system = request.system.clone().unwrap_or_default();
        let background = [fake::MEMORY_MARKER, fake::SUMMARY_MARKER, fake::CONSOLIDATE_MARKER].iter().any(|m| system.contains(m));
        let behavior = if self.failing.load(Ordering::SeqCst) && !background { fake::FakeBehavior::FailAfterDeltas } else { fake::FakeBehavior::Normal };
        fake::FakeRuntime { delay: Duration::from_millis(1), behavior }.start(request).await
    }
}

async fn project(client: &Client, name: &str) -> ProjectId {
    match client.execute(envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap().result {
        CommandResult::ProjectCreated { project_id, .. } => project_id,
        other => panic!("{other:?}"),
    }
}

async fn employee(client: &Client, project_id: ProjectId, name: &str) -> EmployeeId {
    let create = CreateEmployee { name: name.into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None, department_id: None };
    match client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result {
        CommandResult::EmployeeCreated { employee_id, .. } => employee_id,
        other => panic!("{other:?}"),
    }
}

/// Give the employee a task and wait until it ended with `status`.
async fn work(client: &Client, project_id: ProjectId, employee_id: EmployeeId, title: &str, status: TaskStatus) -> TaskId {
    let task = CreateTask { project_id, employee_id: Some(employee_id), title: title.into(), goal: format!("{title} for the release"), acceptance_criteria: vec![], depends_on: vec![], run: Default::default(), files: vec![] };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(task), None)).await.unwrap().result else { panic!("not a task") };
    for _ in 0..400 {
        if client.tasks(Some(project_id)).await.unwrap().tasks.iter().any(|t| t.id == task_id && t.status == status) {
            return task_id;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the task did not reach {status:?}");
}

/// The memory of `project` and of `employee` (its own scope holds the lessons).
async fn wait_memory(client: &Client, project: ProjectId, employee: EmployeeId, done: impl Fn(&[MemoryView]) -> bool) -> Vec<MemoryView> {
    for _ in 0..400 {
        let items = client.memory_of(Some(project), Some(employee), None).await.unwrap().items;
        if done(&items) {
            return items;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("memory did not reach the expected state: {:#?}", client.memory_of(Some(project), Some(employee), None).await.unwrap().items);
}

/// The extraction job of a task has finished.
async fn extracted(root: &std::path::Path, task: TaskId) {
    for _ in 0..400 {
        let open: i64 = db(root)
            .query_row("SELECT COUNT(*) FROM jobs WHERE kind = 'memory.extract' AND json_extract(payload_json, '$.task_id') = ?1 AND state IN ('ready','leased')", [task.to_string()], |r| r.get(0))
            .unwrap();
        let any: i64 = db(root).query_row("SELECT COUNT(*) FROM jobs WHERE kind = 'memory.extract' AND json_extract(payload_json, '$.task_id') = ?1", [task.to_string()], |r| r.get(0)).unwrap();
        if any > 0 && open == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the extraction of {task} did not finish");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_task_leaves_a_lesson_that_a_second_failure_reinforces() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Arc::new(FailingTasks { failing: AtomicBool::new(false) });
    let daemon = start_with(dir.path(), runtime.clone()).await;
    let client = connect(dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    let alex = employee(&client, atlas, "Alex").await;

    // A task that simply succeeds: memories, but no lesson (the gate found nothing).
    let fine = work(&client, atlas, alex, "Changelog", TaskStatus::Succeeded).await;
    extracted(dir.path(), fine).await;
    let items = wait_memory(&client, atlas, alex, |m| !m.is_empty()).await;
    assert!(items.iter().all(|m| m.lesson.is_none()), "no signals, no lesson: {items:#?}");

    // A task whose run fails: the failure is the signal, the extraction still runs.
    runtime.failing.store(true, Ordering::SeqCst);
    let broken = work(&client, atlas, alex, "Migration", TaskStatus::Failed).await;
    extracted(dir.path(), broken).await;
    let items = wait_memory(&client, atlas, alex, |m| m.iter().any(|x| x.lesson.is_some())).await;
    let lessons: Vec<&MemoryView> = items.iter().filter(|m| m.lesson.is_some()).collect();
    assert_eq!(lessons.len(), 1, "{lessons:#?}");
    let lesson = lessons[0].clone();
    let detail = lesson.lesson.clone().unwrap();
    assert_eq!((lesson.kind, lesson.status, lesson.scope.as_str()), (MemoryKind::Episodic, MemoryStatus::Active, "employee"));
    assert!(lesson.content.starts_with("When a task like this one: "), "{}", lesson.content);
    assert_eq!(detail.reinforced, 0);
    assert_eq!(detail.tasks, vec![broken]);
    // S1 is the task's own failure (it is recorded before the run's), the signal the test runtime's lesson rests on.
    assert!(detail.evidence.iter().any(|e| e.label == "S1" && e.text.contains("failed")), "{detail:#?}");
    assert!(lesson.sources.iter().any(|s| s.label.starts_with("Task failed")), "the signal's event is the provenance: {:#?}", lesson.sources);
    let version: String = db(dir.path()).query_row("SELECT extraction_version FROM memory_items WHERE id = ?1", [lesson.id.to_string()], |r| r.get(0)).unwrap();
    assert_eq!(version, "aurelius-extract-2");

    // The same thing goes wrong again: the lesson is confirmed, not doubled.
    let again = work(&client, atlas, alex, "Migration again", TaskStatus::Failed).await;
    extracted(dir.path(), again).await;
    let items = wait_memory(&client, atlas, alex, |m| m.iter().any(|x| x.lesson.as_ref().is_some_and(|l| l.reinforced == 1))).await;
    let lessons: Vec<&MemoryView> = items.iter().filter(|m| m.lesson.is_some() && m.status == MemoryStatus::Active).collect();
    assert_eq!(lessons.len(), 1, "{lessons:#?}");
    let detail = lessons[0].lesson.clone().unwrap();
    assert_eq!(detail.tasks, vec![broken, again]);
    assert!(lessons[0].importance > lesson.importance);
    assert!(lessons[0].sources.iter().any(|s| s.label.starts_with("Lesson confirmed")), "{:#?}", lessons[0].sources);
    assert!(lessons[0].sources.iter().any(|s| s.label.starts_with("Lesson learned")), "{:#?}", lessons[0].sources);

    // In the employee's context it is a lesson, not an ordinary episode.
    let view = client.memory_context(Some(atlas), Some(alex)).await.unwrap();
    let item = view.items.iter().find(|i| i.memory_id == lesson.id).expect("the lesson is in the employee's memory context");
    assert_eq!(item.label, "Lesson · employee");

    stop(dir.path(), daemon).await;
}
