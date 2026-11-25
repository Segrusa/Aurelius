//! Methods no client calls any more answer "gone" (`not_found` with a clear message),
//! as `group_chat.get` without a channel does since the Team chat went (OPR-3). Their
//! names, wire types and features stay: protocol 5 only adds.

mod common;

use aurelius_application::conversation::project_chat::PROJECT_CONVERSATIONS_GONE;
use aurelius_application::task::ideas::IDEA_START_GONE;
use aurelius_client::{Client, ClientError, envelope};
use aurelius_domain::employee::{CreateEmployee, OpenEmployeeChat};
use aurelius_domain::idea::{AddIdea, IdeaStatus, StartIdea};
use aurelius_domain::project::CreateProject;
use aurelius_domain::project_conversation::{SendProjectMessage, StartProjectConversation};
use aurelius_domain::{Command, CommandResult, EmployeeId, ErrorCode, ProjectId};
use aurelius_protocol::Method;
use common::*;

/// The error code and message of a refused read or write.
fn refusal(err: ClientError) -> (ErrorCode, String) {
    match err {
        ClientError::Protocol(e) => (e.code, e.message),
        other => panic!("not a refusal: {other:?}"),
    }
}

fn gone(message: &str) -> (ErrorCode, String) {
    (ErrorCode::NotFound, message.to_owned())
}

async fn hire(client: &Client, name: &str) -> EmployeeId {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Dev".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    employee_id
}

#[tokio::test(flavor = "multi_thread")]
async fn employee_chat_answers_gone() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let ann = hire(&client, "Ann").await;
    let err = client.call(Method::EmployeeChat { employee_id: ann }).await.unwrap_err();
    assert_eq!(refusal(err), gone(aurelius_application::EMPLOYEE_CHAT_GONE));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn employee_chats_answers_gone() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let err = client.call(Method::EmployeeChats).await.unwrap_err();
    assert_eq!(refusal(err), gone(aurelius_application::EMPLOYEE_CHAT_GONE));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn employee_open_chat_answers_gone() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let ann = hire(&client, "Ann").await;
    let err = client.execute(envelope(Command::OpenEmployeeChat(OpenEmployeeChat { employee_id: ann }), None)).await.unwrap_err();
    assert_eq!(refusal(err), gone(aurelius_application::EMPLOYEE_CHAT_GONE));
    stop(dir.path(), daemon).await;
}

async fn project(client: &Client) -> ProjectId {
    let CommandResult::ProjectCreated { project_id, .. } = client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result else { panic!() };
    project_id
}

#[tokio::test(flavor = "multi_thread")]
async fn project_conversation_list_answers_gone() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let project_id = project(&client).await;
    let err = client.call(Method::ProjectConversationList { project_id }).await.unwrap_err();
    assert_eq!(refusal(err), gone(PROJECT_CONVERSATIONS_GONE));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn project_conversation_start_answers_gone() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let project_id = project(&client).await;
    let err = client.execute(envelope(Command::StartProjectConversation(StartProjectConversation { project_id, start: Default::default() }), None)).await.unwrap_err();
    assert_eq!(refusal(err), gone(PROJECT_CONVERSATIONS_GONE));
    // project_conversation.send still makes a conversation with its first message (mobile uses it).
    let send = SendProjectMessage { confirm_secret: false, project_id, text: "Hello".into(), conversation_id: None, images: vec![], files: vec![], interrupt: false, start: Default::default() };
    assert!(matches!(client.execute(envelope(Command::SendProjectMessage(send), None)).await.unwrap().result, CommandResult::ProjectMessageSent { .. }));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn idea_start_answers_gone() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let project_id = project(&client).await;
    let employee_id = hire(&client, "Ann").await;
    let add = AddIdea { confirm_secret: false, project_id, title: "Dark mode".into(), note: None, kind: Default::default(), files: vec![] };
    let CommandResult::IdeaAdded { idea_id } = client.execute(envelope(Command::AddIdea(add), None)).await.unwrap().result else { panic!() };
    let err = client.execute(envelope(Command::StartIdea(StartIdea { idea_id, employee_id, run: Default::default() }), None)).await.unwrap_err();
    assert_eq!(refusal(err), gone(IDEA_START_GONE));
    // Nothing changed: the idea is still open, and no task was made.
    let idea = client.ideas(project_id).await.unwrap().ideas.into_iter().find(|i| i.id == idea_id).unwrap();
    assert_eq!((idea.status, idea.task_id), (IdeaStatus::Open, None));
    assert!(client.tasks(None).await.unwrap().tasks.is_empty());
    stop(dir.path(), daemon).await;
}
