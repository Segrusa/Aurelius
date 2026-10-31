//! Files sent to an employee with a message: copied into the conversation's
//! work folder (`.aurelius/uploads/`, ignored by git), named in the prompt of
//! that turn and kept with the message for display. An employee has task
//! conversations only, so the message goes to the conversation of one of its tasks.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId,
    conversation::{MessageRole, MessageStatus},
    employee::{CreateEmployee, SendEmployeeMessage},
    project::CreateProject,
    task::{CreateTask, TaskStatus},
};
use aurelius_protocol::{CommandResult, MessageFileView, MessageView};
use common::*;
use std::path::PathBuf;
use std::time::Duration;

/// An employee with a finished task: its conversation and the project's work folder.
async fn employee(client: &Client) -> (EmployeeId, ConversationId, PathBuf) {
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let folder = client.projects().await.unwrap().projects.into_iter().find(|p| p.id == project_id).unwrap().folder.expect("folder");
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
    let cmd = CreateTask { confirm_secret: false, files: vec![], run: Default::default(), project_id, employee_id: Some(employee_id), title: "Intro".into(), goal: "Say hello".into(), acceptance_criteria: vec![], depends_on: vec![] };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result else { panic!() };
    for _ in 0..400 {
        if let Some(t) = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == task_id && t.status == TaskStatus::Succeeded) {
            return (employee_id, t.conversation_id.expect("the task's conversation"), PathBuf::from(folder));
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the task did not finish");
}

fn send(employee_id: EmployeeId, conversation_id: ConversationId, text: &str, files: Vec<String>) -> Command {
    Command::SendEmployeeMessage(SendEmployeeMessage { confirm_secret: false, employee_id, text: text.into(), conversation_id, images: Vec::new(), files, interrupt: false })
}

/// Send and wait for the answer; returns (the user's message, the answer).
async fn ask(client: &Client, conversation: ConversationId, command: Command) -> (MessageView, MessageView) {
    let before = client.messages(conversation).await.unwrap().messages.len();
    client.execute(envelope(command, None)).await.unwrap();
    for _ in 0..400 {
        let messages = client.messages(conversation).await.unwrap().messages;
        if messages.len() >= before + 2 {
            if let Some(a) = messages.last().filter(|m| m.role == MessageRole::Assistant && m.status != MessageStatus::Partial) {
                return (messages[before].clone(), a.clone());
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no answer");
}

#[tokio::test(flavor = "multi_thread")]
async fn files_land_in_the_work_folder_and_are_named_on_their_turn() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (alex, conversation, folder) = employee(&client).await;
    let report = dir.path().join("report.pdf");
    std::fs::write(&report, b"%PDF-1.7 fourteen").unwrap();
    let report = report.display().to_string();

    // Text and a file: the file is copied, the turn names it in front of the text.
    let (sent, answer) = ask(&client, conversation, send(alex, conversation, "Summarize it", vec![report.clone()])).await;
    let id = format!("m:{}:0", sent.id);
    assert_eq!(sent.uploads, vec![MessageFileView { id: Some(id.clone()), name: "report.pdf".into(), path: ".aurelius/uploads/report.pdf".into(), byte_length: 17 }]);
    // Clients not on this computer fetch it by its id.
    assert_eq!(client.upload(&id).await.unwrap(), ("report.pdf".into(), "application/pdf".into(), b"%PDF-1.7 fourteen".to_vec()));
    let not_image = client.upload_thumbnail(&id, 128).await.unwrap_err();
    assert!(not_image.to_string().contains("not an image"), "{not_image}");
    for unknown in [format!("m:{}:1", sent.id), "t:nope:0".into(), "../../x".into()] {
        let gone = client.upload(&unknown).await.unwrap_err();
        assert!(gone.to_string().contains("no longer kept"), "{unknown}: {gone}");
    }
    assert_eq!(sent.text, "Summarize it", "the shown text stays the user's own");
    assert!(
        answer.text.contains("The user uploaded these files into your work folder: .aurelius/uploads/report.pdf (17 B)\n\nSummarize it"),
        "{}",
        answer.text
    );
    let uploads = folder.join(".aurelius").join("uploads");
    assert_eq!(std::fs::read(uploads.join("report.pdf")).unwrap(), b"%PDF-1.7 fourteen");
    assert_eq!(std::fs::read_to_string(uploads.join(".gitignore")).unwrap(), "*\n");

    // Files without text are fine; a second copy gets its own name.
    let (sent, answer) = ask(&client, conversation, send(alex, conversation, "", vec![report.clone()])).await;
    assert_eq!(sent.uploads[0].path, ".aurelius/uploads/report (2).pdf");
    assert!(answer.text.contains("work folder: .aurelius/uploads/report (2).pdf (17 B)"), "{}", answer.text);

    // The next turn does not repeat the line.
    let (sent, answer) = ask(&client, conversation, send(alex, conversation, "Thanks", Vec::new())).await;
    assert!(sent.uploads.is_empty());
    assert!(answer.text.contains("You asked: \u{201c}Thanks\u{201d}"), "{}", answer.text);

    // Clear errors: missing, too many; nothing is copied for a refused message.
    let missing = client.execute(envelope(send(alex, conversation, "x", vec![dir.path().join("gone.txt").display().to_string()]), None)).await.unwrap_err();
    assert!(missing.to_string().contains("gone.txt: the file is missing"), "{missing}");
    let many = client.execute(envelope(send(alex, conversation, "x", vec![report.clone(); 9]), None)).await.unwrap_err();
    assert!(many.to_string().contains("Too many files"), "{many}");
    let count = std::fs::read_dir(&uploads).unwrap().count();
    assert_eq!(count, 3, "report.pdf, report (2).pdf and .gitignore");

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn task_attachments_have_ids_and_thumbnails() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (alex, _, _) = employee(&client).await;
    let project_id = client.tasks(None).await.unwrap().tasks[0].project_id;
    let shot = dir.path().join("shot.png");
    image::RgbImage::from_pixel(600, 300, image::Rgb([10, 200, 30])).save(&shot).unwrap();
    let cmd = CreateTask {
        confirm_secret: false,
        files: vec![shot.display().to_string()],
        run: Default::default(),
        project_id,
        employee_id: Some(alex),
        title: "Look".into(),
        goal: "Look at it".into(),
        acceptance_criteria: vec![],
        depends_on: vec![],
    };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result else { panic!() };
    let task = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == task_id).unwrap();
    let id = task.attachments[0].id.clone().expect("an id");
    assert_eq!(id, format!("t:{task_id}:0"));
    let (name, media_type, bytes) = client.upload(&id).await.unwrap();
    assert_eq!((name.as_str(), media_type.as_str(), bytes), ("shot.png", "image/png", std::fs::read(&shot).unwrap()));
    let thumb = image::load_from_memory(&client.upload_thumbnail(&id, 120).await.unwrap()).unwrap();
    assert_eq!((thumb.width(), thumb.height()), (120, 60));

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_without_a_work_folder_refuses_files() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (alex, conversation, folder) = employee(&client).await;
    std::fs::remove_dir_all(&folder).unwrap();
    let note = dir.path().join("note.txt");
    std::fs::write(&note, "hi").unwrap();
    let refused = client.execute(envelope(send(alex, conversation, "x", vec![note.display().to_string()]), None)).await.unwrap_err();
    assert!(refused.to_string().contains("no work folder"), "{refused}");

    stop(dir.path(), daemon).await;
}
