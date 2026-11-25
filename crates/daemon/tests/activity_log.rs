//! The activity log and an employee's mailbox through the real dispatcher: what one
//! message goes through is findable per employee and run.

mod common;
use common::*;

use aurelius_client::{Client, envelope};
use aurelius_domain::conversation::{MessageRole, MessageStatus};
use aurelius_domain::employee::{ActivityState, CreateEmployee, SendEmployeeMessage};
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, RunId};
use std::time::Duration;

async fn hire(client: &Client, name: &str) -> (EmployeeId, ConversationId) {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Dev".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    (employee_id, conversation_id)
}

async fn answered(client: &Client, chat: ConversationId) {
    for _ in 0..800 {
        let all = client.messages(chat).await.unwrap().messages;
        if all.iter().any(|m| m.role == MessageRole::Assistant && m.status == MessageStatus::Complete) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no answer in {chat}");
}

fn steps(page: &aurelius_domain::views::ActivityLogPage) -> Vec<&str> {
    page.entries.iter().rev().map(|e| e.step.as_str()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn one_message_is_in_the_log_from_delivery_to_the_end_of_its_run() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect_as(dir.path(), "grace").await;
    let (ann, chat) = hire(&client, "Ann").await;
    let message = SendEmployeeMessage { employee_id: ann, text: "hello".into(), conversation_id: chat, images: Vec::new(), files: Vec::new(), interrupt: false, confirm_secret: false };
    let sent = client.execute(envelope(Command::SendEmployeeMessage(message), None)).await.unwrap().result;
    let CommandResult::EmployeeMessageSent { run_id, .. } = sent else { panic!("{sent:?}") };
    let run: RunId = run_id;
    answered(&client, chat).await;

    let by_run = client.activity_log(Some(run), None).await.unwrap();
    let wanted = ["delivered", "claimed", "started", "completed"];
    assert_eq!(steps(&by_run).into_iter().filter(|s| wanted.contains(s)).collect::<Vec<_>>(), wanted, "{:?}", steps(&by_run));
    let delivered = by_run.entries.iter().find(|e| e.step == "delivered").unwrap();
    assert_eq!(delivered.category, "delivery");
    assert_eq!(delivered.employee_name.as_deref(), Some("Ann"));
    assert_eq!(delivered.detail.as_deref(), Some("message from board"));
    assert_eq!(delivered.place.as_ref().unwrap().label, "Ann's own chat");
    assert!(steps(&client.activity_log(None, Some(ann)).await.unwrap()).contains(&"completed"));
    assert!(client.activity_log(None, Some(EmployeeId::new())).await.unwrap().entries.is_empty());

    for _ in 0..200 {
        let queue = client.employee_queue(ann).await.unwrap();
        let state = client.employees().await.unwrap().employees.into_iter().find(|e| e.id == ann).unwrap().activity_state;
        if queue.items.is_empty() && state == ActivityState::Idle {
            stop(dir.path(), daemon).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("its queue did not empty or it did not become idle");
}

