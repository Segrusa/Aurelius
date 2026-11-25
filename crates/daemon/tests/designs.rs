//! A workspace's designs (feature `designs`), through the daemon's API: an HTML
//! page an employee writes and names in a work session is kept with its answer,
//! and `design.list` gives it as a page of that session's design, for its own
//! project only.

mod common;

use aurelius_client::envelope;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::work_session::StartWorkSession;
use aurelius_domain::{Command, CommandResult, ProjectId};
use aurelius_protocol::{Method, ResponseBody, feature};
use aurelius_runtime::{
    AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake,
};
use common::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Writes `design/landing.html` in the project's folder and names its path.
struct Designer {
    folder: PathBuf,
}

#[async_trait::async_trait]
impl AgentRuntime for Designer {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let page = self.folder.join("design").join("landing.html");
        std::fs::create_dir_all(page.parent().unwrap()).unwrap();
        std::fs::write(&page, "<!doctype html><h1>Tidewater</h1>").unwrap();
        let text = format!("The landing page is ready: {}.", page.display());
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            let _ = tx.send(RuntimeEvent::TextDelta(text)).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

async fn project(
    client: &aurelius_client::Client,
    name: &str,
    folder: Option<&std::path::Path>,
) -> ProjectId {
    let create = CreateProject {
        name: name.into(),
        description: None,
        folder: folder.map(|f| f.display().to_string()),
    };
    let CommandResult::ProjectCreated { project_id, .. } = client
        .execute(envelope(Command::CreateProject(create), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    project_id
}

async fn designs(
    client: &aurelius_client::Client,
    project_id: Option<ProjectId>,
) -> aurelius_domain::views::DesignsSnapshot {
    let ResponseBody::Designs(snapshot) = client
        .call(Method::DesignList { project_id })
        .await
        .unwrap()
    else {
        panic!("not designs")
    };
    snapshot
}

#[tokio::test(flavor = "multi_thread")]
async fn a_page_an_employee_writes_in_a_session_is_listed_as_a_design_of_its_project() {
    assert!(aurelius_protocol::FEATURES.contains(&feature::DESIGNS));
    let dir = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    git_repo(folder.path());
    let daemon = start_with(
        dir.path(),
        Arc::new(Designer {
            folder: folder.path().to_path_buf(),
        }),
    )
    .await;
    let client = connect(dir.path()).await;
    let tidewater = project(&client, "Tidewater", Some(folder.path())).await;
    let other = project(&client, "Other", None).await;
    let create = CreateEmployee {
        access_level: None,
        tool_policy: None,
        start_profile: None,
        description: None,
        name: "Pixel".into(),
        role: "Designer".into(),
        title: None,
        reports_to: None,
        instructions: None,
        folder: None,
    };
    let CommandResult::EmployeeCreated { employee_id, .. } = client
        .execute(envelope(Command::CreateEmployee(create), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    let start = StartWorkSession {
        project_id: tidewater,
        employee_id,
        text: "Design a landing page.".into(),
        images: vec![],
        files: vec![],
        worktree: false,
        own_branch: true,
        run: Default::default(),
        confirm_secret: false,
        design: true,
    };
    let CommandResult::WorkSessionStarted {
        conversation_id, ..
    } = client
        .execute(envelope(Command::StartWorkSession(start), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };

    let mut listed = designs(&client, Some(tidewater)).await;
    for _ in 0..400 {
        // A design session is listed from its start; wait for its first page.
        if listed.designs.iter().any(|d| !d.pages.is_empty()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        listed = designs(&client, Some(tidewater)).await;
    }
    let [design] = listed.designs.as_slice() else {
        panic!("one design: {listed:?}")
    };
    assert_eq!(
        (
            design.conversation_id,
            design.kind.as_str(),
            design.project_id
        ),
        (conversation_id, "session", tidewater)
    );
    let [page] = design.pages.as_slice() else {
        panic!("one page: {design:?}")
    };
    assert_eq!(
        (page.file.name.as_str(), page.file.media_type.as_str()),
        ("landing.html", "text/html")
    );
    let (_, _, bytes) = client.file(&page.file.id).await.unwrap();
    assert_eq!(
        bytes, b"<!doctype html><h1>Tidewater</h1>",
        "the page opens with file.get"
    );

    assert!(design.design, "a design session");
    assert!(aurelius_protocol::FEATURES.contains(&feature::DESIGN_SESSIONS));
    let ResponseBody::WorkSessions(listed_sessions) = client.call(Method::WorkSessionList { project_id: Some(tidewater) }).await.unwrap() else { panic!("not sessions") };
    let sessions = listed_sessions.sessions;
    assert!(sessions.iter().any(|s| s.conversation_id == conversation_id && s.design), "{sessions:?}");
    assert!(
        designs(&client, Some(other)).await.designs.is_empty(),
        "another project has none"
    );
    assert_eq!(
        designs(&client, None).await.designs.len(),
        1,
        "all projects"
    );
    stop(dir.path(), daemon).await;
}
