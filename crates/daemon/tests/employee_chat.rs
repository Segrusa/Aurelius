//! Every employee has one persistent chat with the user, outside projects
//! (`employee.chat`); its task conversations work too. Other conversations take
//! no message from it.

mod common;

use aurelius_client::envelope;
use aurelius_domain::{
    Command, EmployeeId,
    conversation::{MessageRole, MessageStatus},
    employee::{CreateEmployee, SendEmployeeMessage},
    project::CreateProject,
    task::CreateTask,
};
use aurelius_protocol::CommandResult;
use common::*;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn an_employee_chats_outside_projects_and_keeps_its_task_conversations() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Alex".into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id: alex, conversation_id: chat } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else {
        panic!()
    };
    let message = |employee_id: EmployeeId, conversation_id| Command::SendEmployeeMessage(SendEmployeeMessage { confirm_secret: false, employee_id, text: "Hallo".into(), conversation_id, images: Vec::new(), files: Vec::new(), interrupt: false });

    // Its chat: no project, reachable with employee.chat, and it answers there.
    let opened = client.employee_chat(alex).await.unwrap();
    assert_eq!((opened.conversation_id, opened.employee_id, opened.project_id, opened.read_only), (chat, Some(alex), None, false));
    let CommandResult::EmployeeMessageSent { message_id, .. } = client.execute(envelope(message(alex, chat), None)).await.unwrap().result else { panic!() };
    let mut answered = false;
    for _ in 0..400 {
        let messages = client.employee_chat(alex).await.unwrap().messages;
        if messages.iter().any(|m| m.role == MessageRole::Assistant && m.reply_to == Some(message_id) && m.status == MessageStatus::Complete) {
            answered = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(answered, "the employee answers in its chat");
    // A conversation that is not its chat nor one of its tasks takes no message.
    assert!(client.execute(envelope(message(alex, aurelius_domain::ConversationId::new()), None)).await.is_err());
    assert!(client.employee_chat(EmployeeId::new()).await.is_err(), "an unknown employee has no chat");

    // Tasks keep their conversation, in the task's project, and the user can still write in it.
    let cmd = CreateTask { confirm_secret: false, project_id, employee_id: Some(alex), title: "Dark mode".into(), goal: "Werk aan dark mode".into(), acceptance_criteria: vec![], depends_on: vec![], run: Default::default(), files: vec![] };
    client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap();
    let mut conversation = None;
    for _ in 0..600 {
        conversation = client.tasks(Some(project_id)).await.unwrap().tasks.into_iter().find_map(|t| t.conversation_id.filter(|_| t.status.is_terminal()));
        if conversation.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let conversation = conversation.expect("the task ran in a conversation of its own");
    assert_eq!(client.messages(conversation).await.unwrap().project_id, Some(project_id));
    client.execute(envelope(message(alex, conversation), None)).await.expect("a task conversation is not closed");

    // Neither is offered as a project conversation.
    let listed = client.project_conversations(project_id).await.unwrap().conversations;
    assert!(listed.iter().all(|c| c.id != conversation && c.id != chat), "{listed:?}");

    stop(dir.path(), daemon).await;
}
