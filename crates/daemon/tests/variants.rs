//! Harness² C: a task tried in several ways first. Propose (the cheap model's
//! approaches), probe (each trial in its own git worktree, sandboxed), judge,
//! compose (the chosen diff goes into the work folder, the other worktrees go,
//! the task goes on), and a stop that stops every trial.

mod common;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, EmployeeId, ProjectId, TaskId,
    autonomy::AgentMode,
    employee::CreateEmployee,
    project::CreateProject,
    task::{CancelTask, CreateTask, RunSettings, TaskStatus},
    variants::{ChooseTaskVariant, VariantPhase, VariantStatus},
};
use aurelius_protocol::{CommandResult, TaskVariantsView};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, fake};
use common::*;
use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// The fake, but a trial writes `trial.txt` (its approach) into its folder, and
/// while `hold` is set the first trial waits until released.
struct Trials {
    hold: AtomicBool,
    release: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl AgentRuntime for Trials {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let system = request.system.clone().unwrap_or_default();
        if system.contains("This conversation is one trial of a task") {
            let approach = system.lines().find_map(|l| l.strip_prefix("Your approach in this trial: ")).unwrap_or("?").to_owned();
            // The test runtime is no Claude Code, so it gets no folders: the brief names the worktree.
            let brief = request.turns.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join("\n");
            let folder = brief.split_once("separate copy of the work folder: ").and_then(|(_, rest)| rest.split_once(". Its build folder")).map(|(path, _)| PathBuf::from(path.trim()));
            if let Some(folder) = folder {
                std::fs::write(folder.join("trial.txt"), format!("{approach}\n")).unwrap();
            }
            if self.hold.swap(false, Ordering::SeqCst) {
                self.release.notified().await;
            }
        }
        fake::FakeRuntime { delay: Duration::from_millis(1), ..Default::default() }.start(request).await
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Process::new("git").arg("-C").arg(dir).args(args).output().expect("git runs");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A git repository with one commit.
fn repo(root: &Path) -> PathBuf {
    let dir = root.join("repo");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("readme.md"), "hello\n").unwrap();
    let steps: [&[&str]; 6] = [
        &["init", "--quiet", "--initial-branch=main"],
        &["config", "user.name", "Test"],
        &["config", "user.email", "t@example.com"],
        &["config", "core.autocrlf", "false"],
        &["add", "-A"],
        &["commit", "--quiet", "-m", "initial"],
    ];
    for args in steps {
        git(&dir, args);
    }
    dir
}

async fn setup(client: &Client, folder: Option<&Path>, name: &str) -> (ProjectId, EmployeeId) {
    let create = CreateProject { name: "Atlas".into(), description: None, folder: folder.map(|f| f.display().to_string()) };
    let CommandResult::ProjectCreated { project_id, .. } = client.execute(envelope(Command::CreateProject(create), None)).await.unwrap().result else { panic!() };
    let employee = CreateEmployee { name: name.into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None, department_id: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(envelope(Command::CreateEmployee(employee), None)).await.unwrap().result else { panic!() };
    (project_id, employee_id)
}

fn task(project_id: ProjectId, employee: EmployeeId, variants: u8, mode: Option<AgentMode>) -> Command {
    Command::CreateTask(CreateTask {
        project_id,
        employee_id: Some(employee),
        title: "Greeting".into(),
        goal: "Add a greeting file".into(),
        acceptance_criteria: vec![],
        depends_on: vec![],
        run: RunSettings { variants: Some(variants), mode, ..Default::default() },
        files: vec![],
    })
}

async fn wait_variants(client: &Client, task: TaskId, done: impl Fn(&TaskVariantsView) -> bool) -> TaskVariantsView {
    let mut last = None;
    for _ in 0..600 {
        if let Ok(view) = client.task_variants(task).await {
            if done(&view) {
                return view;
            }
            last = Some(view);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the variants did not get there: {last:#?}");
}

async fn wait_status(client: &Client, project: ProjectId, task: TaskId, status: TaskStatus) {
    for _ in 0..600 {
        if client.tasks(Some(project)).await.unwrap().tasks.iter().any(|t| t.id == task && t.status == status) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the task did not get {status:?}");
}

fn probes(repo: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(repo.join("target")).map(|d| d.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("wt-probe-")).collect()).unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn two_trials_are_compared_and_the_chosen_one_is_applied() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let runtime = Arc::new(Trials { hold: AtomicBool::new(true), release: tokio::sync::Notify::new() });
    let daemon = start_with(dir.path(), runtime.clone()).await;
    let client = connect(dir.path()).await;
    let (project, alex) = setup(&client, Some(&repo), "Alex").await;
    let (other, bob) = setup(&client, None, "Bob").await;

    // Off by default; then a work folder that is no git repository is refused.
    let refused = client.execute(envelope(task(project, alex, 2, None), None)).await.unwrap_err().to_string();
    assert!(refused.contains("variants are off"), "{refused}");
    assert!(client.set_variants_enabled(true).await.unwrap().enabled);
    assert!(client.variants_setting().await.unwrap().enabled);
    let refused = client.execute(envelope(task(other, bob, 2, None), None)).await.unwrap_err().to_string();
    assert!(refused.contains("git repository"), "{refused}");
    assert!(client.execute(envelope(task(project, alex, 4, None), None)).await.is_err(), "three at most");

    let CommandResult::TaskCreated { task_id } = client.execute(envelope(task(project, alex, 2, None), None)).await.unwrap().result else { panic!() };

    // The first trial runs (held) in its own worktree: the sandbox keeps it there.
    let view = wait_variants(&client, task_id, |v| v.variants.first().is_some_and(|x| x.status == VariantStatus::Running && x.run_id.is_some())).await;
    assert_eq!((view.phase, view.count), (VariantPhase::Probing, 2));
    assert!(view.variants.iter().all(|v| v.hint.starts_with("Approach ")), "{view:#?}");
    let run = view.variants[0].run_id.unwrap();
    let worktree = probes(&repo).into_iter().next().expect("a worktree under target/");
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let ask = |tool: &str, input: serde_json::Value| bridge.permission_ask(run, None, tool.to_string(), input);
    let push = ask("Bash", serde_json::json!({ "command": "git push origin main" })).await.unwrap();
    assert!(!push.allow && push.message.as_deref().unwrap_or("").starts_with("probe: not allowed"), "{push:?}");
    assert!(!ask("Write", serde_json::json!({ "file_path": repo.join("readme.md").display().to_string() })).await.unwrap().allow, "the real folder is outside");
    assert!(!ask("mcp__notes-app__create_note", serde_json::json!({})).await.unwrap().allow, "MCP tools are outside actions");
    assert!(ask("Write", serde_json::json!({ "file_path": worktree.join("x.txt").display().to_string() })).await.unwrap().allow, "its own worktree is fine");
    assert!(client.approvals().await.unwrap().approvals.is_empty(), "nobody is asked during a trial");
    runtime.release.notify_one();

    // Both ran; the judge compared them; the user chooses (Needs you).
    let view = wait_variants(&client, task_id, |v| v.phase == VariantPhase::Ready).await;
    assert!(view.variants.iter().all(|v| v.status == VariantStatus::Done && v.files == 1 && v.diff.contains("trial.txt") && !v.report.is_empty()), "{view:#?}");
    assert_eq!(view.judge_pick, Some(2), "the test judge prefers the last");
    assert_eq!(view.variants.iter().map(|v| v.rank).collect::<Vec<_>>(), vec![Some(2), Some(1)]);
    assert_eq!(probes(&repo).len(), 2, "both worktrees wait for the choice");
    wait_status(&client, project, task_id, TaskStatus::Waiting).await;
    let notes = client.notifications().await.unwrap().notifications;
    assert!(notes.iter().any(|n| n.target_type == "task.variants_ready"), "{notes:#?}");
    assert_eq!(std::fs::read_to_string(repo.join("readme.md")).unwrap(), "hello\n");
    assert!(!repo.join("trial.txt").exists(), "nothing is in the real folder before the choice");

    // The user takes the first: its diff is applied, the worktrees go, the task goes on.
    let chosen = client.execute(envelope(Command::ChooseTaskVariant(ChooseTaskVariant { task_id, variant: Some(1) }), None)).await.unwrap().result;
    assert!(matches!(chosen, CommandResult::TaskVariantChosen { variant: 1, .. }), "{chosen:?}");
    wait_status(&client, project, task_id, TaskStatus::Succeeded).await;
    let view = wait_variants(&client, task_id, |v| v.phase == VariantPhase::Done).await;
    assert_eq!((view.chosen, view.chosen_by.as_deref()), (Some(1), Some("user")));
    assert_eq!(std::fs::read_to_string(repo.join("trial.txt")).unwrap(), format!("{}\n", view.variants[0].hint));
    for _ in 0..200 {
        if probes(&repo).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(probes(&repo).is_empty(), "no worktree is left: {:?}", probes(&repo));
    assert!(client.notifications().await.unwrap().notifications.iter().all(|n| n.target_type != "task.variants_ready"), "chosen: no longer in Needs you");
    let tasks = client.tasks(Some(project)).await.unwrap().tasks;
    let done = tasks.iter().find(|t| t.id == task_id).unwrap();
    assert_eq!((done.run.variants, done.variant_phase), (Some(2), Some(VariantPhase::Done)));

    // The chosen approach is a signal for the task's lesson (Harness² A).
    let mut lesson = None;
    for _ in 0..400 {
        lesson = client.memory_of(Some(project), Some(alex), None).await.unwrap().items.into_iter().find(|m| m.lesson.is_some());
        if lesson.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let lesson = lesson.expect("a lesson from the chosen approach");
    assert!(lesson.lesson.unwrap().evidence.iter().any(|e| e.text.contains("the user chose")), "{:?}", lesson.content);

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn in_auto_the_judge_chooses_and_a_cancel_stops_every_trial() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let runtime = Arc::new(Trials { hold: AtomicBool::new(false), release: tokio::sync::Notify::new() });
    let daemon = start_with(dir.path(), runtime.clone()).await;
    let client = connect(dir.path()).await;
    let (project, alex) = setup(&client, Some(&repo), "Alex").await;
    client.set_variants_enabled(true).await.unwrap();

    // Auto: nobody is asked; the judge's pick (the second) is applied and the task finishes.
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(task(project, alex, 2, Some(AgentMode::Auto)), None)).await.unwrap().result else { panic!() };
    wait_status(&client, project, task_id, TaskStatus::Succeeded).await;
    let view = wait_variants(&client, task_id, |v| v.phase == VariantPhase::Done).await;
    assert_eq!((view.chosen, view.chosen_by.as_deref()), (Some(2), Some("judge")));
    assert_eq!(std::fs::read_to_string(repo.join("trial.txt")).unwrap(), format!("{}\n", view.variants[1].hint));
    assert!(client.notifications().await.unwrap().notifications.iter().all(|n| n.target_type != "task.variants_ready"), "Auto does not ask");

    // A cancelled task stops its trials; their worktrees go.
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "greeting"]);
    runtime.hold.store(true, Ordering::SeqCst);
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(task(project, alex, 3, None), None)).await.unwrap().result else { panic!() };
    wait_variants(&client, task_id, |v| v.variants.first().is_some_and(|x| x.status == VariantStatus::Running)).await;
    client.execute(envelope(Command::CancelTask(CancelTask { task_id }), None)).await.unwrap();
    runtime.release.notify_one();
    let view = wait_variants(&client, task_id, |v| v.phase == VariantPhase::Stopped && v.variants.iter().all(|x| x.status == VariantStatus::Cancelled)).await;
    assert_eq!(view.variants.len(), 3);
    for _ in 0..200 {
        if probes(&repo).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(probes(&repo).is_empty(), "no worktree is left: {:?}", probes(&repo));
    wait_status(&client, project, task_id, TaskStatus::Cancelled).await;

    stop(dir.path(), daemon).await;
}
