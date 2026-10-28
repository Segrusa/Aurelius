//! B16 acceptance: timeline cards from meaningful events, causality,
//! keyset pagination and deduplicated notifications.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, EmployeeId, ProjectId,
    employee::CreateEmployee,
    project::CreateProject,
    task::{CreateTask, TaskStatus},
};
use aurelius_protocol::{CommandResult, TimelineQuery};
use common::*;
use std::time::Duration;

async fn setup(client: &Client, name: &str, employee: &str) -> (ProjectId, EmployeeId) {
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let CommandResult::EmployeeCreated { employee_id, .. } = client
        .execute(envelope(Command::CreateEmployee(CreateEmployee { name: employee.into(), role: "Productpartner".into(), title: None, reports_to: None, instructions: None, folder: None, department_id: None }), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    (project_id, employee_id)
}

fn query(project_id: Option<ProjectId>) -> TimelineQuery {
    TimelineQuery { project_id, limit: 50, ..Default::default() }
}

#[tokio::test(flavor = "multi_thread")]
async fn timeline_tells_the_story_and_notifies_once() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (atlas, employee) = setup(&client, "Atlas", "Alex").await;
    let (other, _) = setup(&client, "Ander project", "Bo").await;

    let cmd = CreateTask {
        files: vec![],
        run: Default::default(),
        project_id: atlas,
        employee_id: Some(employee),
        title: "Onboarding".into(),
        goal: "Vergelijk ideeën voor onboarding".into(),
        acceptance_criteria: vec!["Elk resultaat noemt een bron".into()],
        depends_on: vec![],
    };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result else { panic!() };
    for _ in 0..400 {
        if client.tasks(None).await.unwrap().tasks.iter().any(|t| t.id == task_id && t.status == TaskStatus::Succeeded) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    // Meaningful cards only, newest first, scoped to the project.
    let page = client.timeline(query(Some(atlas))).await.unwrap();
    let labels: Vec<&str> = page.cards.iter().map(|c| c.label.as_str()).collect();
    assert!(labels.contains(&"COMPLETED"), "{labels:?}");
    assert!(labels.contains(&"WORK STARTED"), "{labels:?}");
    assert!(labels.contains(&"PROJECT"), "{labels:?}");
    assert!(!labels.contains(&"ACTIVITY"), "{labels:?}");
    assert!(page.cards.windows(2).all(|w| w[0].seq > w[1].seq));
    let completed = page.cards.iter().find(|c| c.label == "COMPLETED").unwrap();
    assert_eq!(completed.title, "Onboarding");
    assert_eq!(completed.task_id, Some(task_id));
    // The other project's story does not leak in.
    let other_page = client.timeline(query(Some(other))).await.unwrap();
    assert!(other_page.cards.iter().all(|c| c.task_id.is_none()));

    // Causality: the task chain, from cause to outcome.
    let chain = client.timeline(TimelineQuery { correlation_id: Some(completed.correlation_id.clone()), ..query(None) }).await.unwrap();
    assert!(chain.cards.iter().any(|c| c.label == "WORK STARTED") && chain.cards.iter().any(|c| c.label == "COMPLETED"));

    // Activity includes the routine events.
    let all = client.timeline(TimelineQuery { activity: true, ..query(Some(atlas)) }).await.unwrap();
    assert!(all.cards.iter().any(|c| c.kind == "activity"));

    // Keyset pagination: pages do not overlap and cover everything.
    let mut seen = Vec::new();
    let mut before = None;
    loop {
        let page = client.timeline(TimelineQuery { before_seq: before, limit: 3, ..query(Some(atlas)) }).await.unwrap();
        seen.extend(page.cards.iter().map(|c| c.seq));
        match page.next_before_seq {
            Some(next) if !page.cards.is_empty() => before = Some(next),
            _ => break,
        }
    }
    let full: Vec<u64> = client.timeline(query(Some(atlas))).await.unwrap().cards.iter().map(|c| c.seq).collect();
    assert_eq!(seen, full);

    // Exactly one "result ready" notification for the task; reading clears unread.
    let inbox = client.notifications().await.unwrap();
    let ready: Vec<_> = inbox.notifications.iter().filter(|n| n.target_type == "task.succeeded").collect();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].subject, "Onboarding");
    assert!(inbox.unread >= 1);
    assert!(client.mark_notifications_read().await.unwrap() >= 1);
    assert_eq!(client.notifications().await.unwrap().unread, 0);
    stop(dir.path(), daemon).await;

    // Restart: nothing is replayed into new notifications.
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let inbox = client.notifications().await.unwrap();
    assert_eq!(inbox.unread, 0);
    assert_eq!(inbox.notifications.iter().filter(|n| n.target_type == "task.succeeded").count(), 1);
    let total = client.notifications().await.unwrap().notifications.len() as u64;
    assert!(total >= 1);
    assert_eq!(client.dismiss_all_notifications().await.unwrap(), total);
    assert!(client.notifications().await.unwrap().notifications.is_empty());
    assert_eq!(client.dismiss_all_notifications().await.unwrap(), 0);
    stop(dir.path(), daemon).await;
}
