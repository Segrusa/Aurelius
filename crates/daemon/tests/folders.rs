//! Every project has its own folder: created automatically, reports saved
//! there, and it can be changed. Conversations show the Git change of their folder.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, ProjectId,
    employee::CreateEmployee,
    project::{CreateProject, SetProjectFolder},
    task::{CreateTask, TaskStatus},
};
use aurelius_protocol::CommandResult;
use common::*;
use std::path::PathBuf;
use std::time::Duration;

async fn project(client: &Client, name: &str) -> (ProjectId, PathBuf) {
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let folder = client.projects().await.unwrap().projects.into_iter().find(|p| p.id == project_id).unwrap().folder.expect("folder");
    (project_id, PathBuf::from(folder))
}

async fn employee(client: &Client, project_id: ProjectId, name: &str) -> (EmployeeId, ConversationId) {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Writer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else {
        panic!()
    };
    (employee_id, conversation_id)
}

fn task(project_id: ProjectId, employee_id: Option<EmployeeId>, title: &str, goal: &str) -> Command {
    Command::CreateTask(CreateTask { confirm_secret: false, project_id, employee_id, title: title.into(), goal: goal.into(), acceptance_criteria: vec![], run: Default::default(), files: vec![] })
}

#[tokio::test(flavor = "multi_thread")]
async fn each_project_has_its_own_folder() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;

    let (atlas, atlas_dir) = project(&client, "Atlas").await;
    let (_, twin_dir) = project(&client, "Atlas").await;
    let (_, other_dir) = project(&client, "Phoenix: R&D?").await;
    assert!(atlas_dir.is_dir() && twin_dir.is_dir() && other_dir.is_dir());
    assert!(atlas_dir.starts_with(dir.path().canonicalize().unwrap().to_string_lossy().trim_start_matches(r"\\?\")), "{atlas_dir:?}");
    assert_ne!(atlas_dir, twin_dir, "same name, different folder");
    assert!(other_dir.file_name().unwrap().to_string_lossy().starts_with("Phoenix- R&D-"));

    // A finished task leaves its report in the project folder.
    let (alex, _) = employee(&client, atlas, "Alex").await;
    client
        .execute(envelope(
            Command::CreateTask(CreateTask {
                confirm_secret: false,
                project_id: atlas,
                employee_id: Some(alex),
                title: "Onboarding".into(),
                goal: "Compare ideas".into(),
                acceptance_criteria: vec!["Every result cites a source".into()],
                run: Default::default(),
                files: vec![],
            }),
            None,
        ))
        .await
        .unwrap();
    let report = atlas_dir.join("artifacts").join("Onboarding — report.md");
    for _ in 0..400 {
        if client.tasks(None).await.unwrap().tasks.iter().any(|t| t.status == TaskStatus::Succeeded) && report.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(std::fs::read_to_string(&report).unwrap().starts_with("# Onboarding"));

    // Another folder: the project shows it, and the next report goes there.
    let moved = dir.path().join("elsewhere");
    std::fs::create_dir_all(&moved).unwrap();
    client.execute(envelope(Command::SetProjectFolder(SetProjectFolder { project_id: atlas, path: moved.display().to_string() }), None)).await.unwrap();
    let shown = client.projects().await.unwrap().projects.into_iter().find(|p| p.id == atlas).unwrap().folder.unwrap();
    assert_eq!(PathBuf::from(shown).canonicalize().unwrap(), moved.canonicalize().unwrap());
    client.execute(envelope(task(atlas, Some(alex), "Second", "Again"), None)).await.unwrap();
    let report = moved.join("artifacts").join("Second — report.md");
    for _ in 0..400 {
        if report.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(report.exists(), "the report of the next task goes to the new folder");

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn chosen_project_folder_and_employee_workspaces() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;

    // The project uses the folder the user chose.
    let chosen = dir.path().join("my-atlas");
    std::fs::create_dir_all(&chosen).unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = client
        .execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: Some(chosen.display().to_string()) }), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    let folder = client.projects().await.unwrap().projects.into_iter().find(|p| p.id == project_id).unwrap().folder.unwrap();
    let canonical = chosen.canonicalize().unwrap().to_string_lossy().into_owned();
    let expected = canonical.trim_start_matches(r"\\?\").to_string();
    assert_eq!(folder, expected);
    // A second project can't take the same folder.
    let taken = client
        .execute(envelope(Command::CreateProject(CreateProject { name: "Copy".into(), description: None, folder: Some(chosen.display().to_string()) }), None))
        .await;
    assert!(taken.is_err());

    // Alex gets an own workspace; Sam works in the project folder only.
    let workspace = dir.path().join("alex-space");
    std::fs::create_dir_all(&workspace).unwrap();
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Alex".into(), role: "Writer".into(), title: None, reports_to: None, instructions: None, folder: Some(workspace.display().to_string()) };
    let CommandResult::EmployeeCreated { employee_id: alex, .. } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    let (sam, _) = employee(&client, project_id, "Sam").await;
    let folder_of = |employees: Vec<aurelius_protocol::EmployeeSummary>, id: EmployeeId| employees.into_iter().find(|m| m.id == id).unwrap().folder;
    let employees = client.employees().await.unwrap().employees;
    let alex_folder = folder_of(employees.clone(), alex).expect("Alex has a workspace");
    assert_eq!(PathBuf::from(alex_folder).canonicalize().unwrap(), workspace.canonicalize().unwrap());
    assert_eq!(folder_of(employees, sam), None, "Sam has none of his own");

    // Clearing the workspace removes it.
    client.execute(envelope(Command::SetEmployeeFolder(aurelius_domain::employee::SetEmployeeFolder { employee_id: alex, path: None }), None)).await.unwrap();
    assert_eq!(folder_of(client.employees().await.unwrap().employees, alex), None);

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_creates_a_folder_and_opens_into_it() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let elsewhere = tempfile::tempdir().unwrap();
    let place = elsewhere.path().to_path_buf();

    let listing = client.create_dir(place.display().to_string(), " Reports ".into()).await.unwrap();
    assert!(place.join("Reports").is_dir());
    assert!(listing.path.as_deref().is_some_and(|p| p.ends_with("Reports") && !p.starts_with(r"\?\")));
    assert!(listing.entries.is_empty());
    let parent = client.list_dir(Some(place.display().to_string())).await.unwrap();
    assert_eq!(parent.entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Reports"]);

    let again = client.create_dir(place.display().to_string(), "Reports".into()).await.unwrap_err();
    assert_eq!(again.to_string(), "A folder with this name already exists.");
    assert!(client.create_dir(place.display().to_string(), "NUL".into()).await.is_err());
    assert!(client.create_dir(place.display().to_string(), "a/b".into()).await.is_err());
    assert!(client.create_dir(place.join("gone").display().to_string(), "New".into()).await.is_err());

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_home_folder_is_refused_as_a_work_folder_but_a_normal_one_works() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (atlas, _) = project(&client, "Atlas").await;
    let (alex, _) = employee(&client, atlas, "Alex").await;

    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap();
    assert!(client.execute(envelope(Command::SetProjectFolder(SetProjectFolder { project_id: atlas, path: home.clone() }), None)).await.is_err());
    let set_employee = |path: String| Command::SetEmployeeFolder(aurelius_domain::employee::SetEmployeeFolder { employee_id: alex, path: Some(path) });
    assert!(client.execute(envelope(set_employee(home), None)).await.is_err());

    let work = dir.path().join("werkmap");
    std::fs::create_dir_all(&work).unwrap();
    client.execute(envelope(Command::SetProjectFolder(SetProjectFolder { project_id: atlas, path: work.display().to_string() }), None)).await.unwrap();
    client.execute(envelope(set_employee(work.display().to_string()), None)).await.unwrap();
    stop(dir.path(), daemon).await;
}
