//! B22 acceptance: a backup restores tasks and chats; the restore starts
//! paused; content deleted afterwards is scrubbed from the backup; a project
//! exports to readable Markdown + JSON.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId,
    conversation::{DeleteConversation, MessageRole, MessageStatus, SendChatMessage},
    employee::CreateEmployee,
    project::CreateProject,
    task::{CreateTask, TaskStatus},
};
use aurelius_protocol::CommandResult;
use common::*;
use std::path::Path;
use std::time::Duration;

async fn send(client: &Client, conversation_id: Option<ConversationId>, text: &str) -> ConversationId {
    let env = envelope(Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: match conversation_id { Some(id) => id, None => common::conversation(client).await }, text: text.into(), images: Vec::new(), interrupt: false }), None);
    match client.execute(env).await.unwrap().result {
        CommandResult::ChatMessageSent { conversation_id, .. } => conversation_id,
        other => panic!("{other:?}"),
    }
}

async fn answered(client: &Client, id: ConversationId, answers: usize, tries: usize) -> bool {
    for _ in 0..tries {
        let messages = client.messages(id).await.unwrap().messages;
        let done = messages.iter().filter(|m| m.role == MessageRole::Assistant && m.status != MessageStatus::Partial).count();
        if done >= answers && messages.iter().all(|m| m.status != MessageStatus::Partial) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    false
}

fn contains(dir: &Path, needle: &[u8]) -> bool {
    std::fs::read_dir(dir).into_iter().flatten().flatten().any(|e| {
        let p = e.path();
        if p.is_dir() { contains(&p, needle) } else { std::fs::read(&p).is_ok_and(|b| b.windows(needle.len()).any(|w| w == needle)) }
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn backup_restore_paused_scrub_and_export() {
    // SAFETY (test): the only test in this binary.
    unsafe { std::env::set_var("AURELIUS_NO_AUTO_BACKUP", "1") };
    let dir = tempfile::tempdir().unwrap();
    let mut daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;

    // State worth keeping: a project, a finished task with its report, two chats.
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: Some("Kept".into()), folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let CommandResult::EmployeeCreated { employee_id, .. } = client
        .execute(envelope(
            Command::CreateEmployee(CreateEmployee { name: "Alex".into(), role: "Writer".into(), title: None, reports_to: None, instructions: None, folder: None }),
            None,
        ))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    let task = CreateTask {
        confirm_secret: false,
        files: vec![],
        run: Default::default(),
        project_id,
        employee_id: Some(employee_id),
        title: "Onboarding".into(),
        goal: "Compare ideas".into(),
        acceptance_criteria: vec!["Cites a source".into()],
        depends_on: vec![],
    };
    client.execute(envelope(Command::CreateTask(task), None)).await.unwrap();
    for _ in 0..400 {
        if client.tasks(None).await.unwrap().tasks.iter().any(|t| t.status == TaskStatus::Succeeded) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(client.tasks(None).await.unwrap().tasks.iter().any(|t| t.status == TaskStatus::Succeeded), "task finished before the backup");
    let keep = send(&client, None, "keep this chat").await;
    let gone = send(&client, None, "zebra-unicorn plan").await;
    assert!(answered(&client, keep, 1, 400).await && answered(&client, gone, 1, 400).await);
    let project_of = |snapshot: aurelius_protocol::MessagesSnapshot| snapshot.project_id.expect("a project conversation");
    let (keep_project, gone_project) = (project_of(client.messages(keep).await.unwrap()), project_of(client.messages(gone).await.unwrap()));

    let snap = client.create_backup().await.unwrap();
    assert!(snap.error.is_none(), "{:?}", snap.error);
    assert_eq!(snap.backups.len(), 1);
    let backup_id = snap.backups[0].id.clone();
    let backup_dir = dir.path().join("backups").join(&backup_id);
    let manifest = std::fs::read_to_string(backup_dir.join("manifest.json")).unwrap();
    assert!(manifest.contains("\"schema_version\"") && manifest.contains("Not encrypted"));
    assert!(contains(&backup_dir, b"zebra-unicorn"), "the chat is in the backup before it is deleted");

    // Deleting the chat erases it from the backup too.
    client.execute(envelope(Command::DeleteConversation(DeleteConversation { conversation_id: gone }), None)).await.unwrap();
    assert!(!contains(&backup_dir, b"zebra-unicorn"), "a deleted chat must not come back through a backup");
    assert!(client.backups().await.unwrap().backups[0].scrubbed);

    // Changes after the backup...
    client.execute(envelope(Command::CreateProject(CreateProject { name: "After".into(), description: None, folder: None }), None)).await.unwrap();

    // ...are undone by a restore, which happens at the next start.
    let snap = client.restore_backup(backup_id.clone()).await.unwrap();
    assert!(snap.error.is_none(), "{:?}", snap.error);
    assert_eq!(snap.restore_pending.as_deref(), Some(backup_id.as_str()));
    tokio::time::timeout(Duration::from_secs(10), &mut daemon).await.expect("the service stops for the restore").unwrap().unwrap();
    drop(client);

    daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let projects = client.projects().await.unwrap().projects;
    assert!(projects.iter().any(|p| p.name == "Atlas") && !projects.iter().any(|p| p.name == "After"));
    assert!(client.tasks(None).await.unwrap().tasks.iter().any(|t| t.title == "Onboarding" && t.status == TaskStatus::Succeeded));
    let listed = |snapshot: aurelius_protocol::ProjectConversationsSnapshot, id| snapshot.conversations.iter().any(|c| c.id == id);
    assert!(listed(client.project_conversations(keep_project).await.unwrap(), keep), "the kept conversation is back");
    assert!(!listed(client.project_conversations(gone_project).await.unwrap(), gone), "the deleted conversation stays deleted");
    assert!(dir.path().read_dir().unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("before-restore-")), "the previous data is kept aside");

    // Paused: nothing runs until resumed.
    let snap = client.backups().await.unwrap();
    assert!(snap.paused.is_some());
    send(&client, Some(keep), "while paused").await;
    assert!(!answered(&client, keep, 2, 20).await, "no answer while paused");
    assert!(client.resume_backups().await.unwrap().paused.is_none());
    assert!(answered(&client, keep, 2, 400).await, "the answer comes after resuming");

    // Readable export into the project folder.
    let (md, json) = client.export_project(project_id).await.unwrap();
    let md = std::fs::read_to_string(md).unwrap();
    assert!(md.starts_with("# Atlas") && md.contains("Onboarding"), "{md}");
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(json).unwrap()).unwrap();
    assert_eq!(json["project"]["name"], "Atlas");

    stop(dir.path(), daemon).await;
}
