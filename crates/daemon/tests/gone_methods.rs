//! Methods no client calls any more answer "gone" (`not_found` with a clear message),
//! as `group_chat.get` without a channel does since the Team chat went (OPR-3). Their
//! names, wire types and features stay: protocol 5 only adds.

mod common;

use aurelius_client::{Client, ClientError, envelope};
use aurelius_domain::employee::{CreateEmployee, OpenEmployeeChat};
use aurelius_domain::{Command, CommandResult, EmployeeId, ErrorCode};
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
