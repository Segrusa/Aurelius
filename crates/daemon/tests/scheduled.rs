//! A message an employee schedules, through the real bridge channel and dispatcher:
//! it waits, comes due, wakes the employee, and its outcome shows on the Scheduled
//! page; a board member can take one back. The scratchpad the same way.

mod common;
use common::*;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::{Client, envelope};
use aurelius_domain::conversation::{MessageRole, MessageStatus};
use aurelius_domain::employee::{CancelScheduledMessage, CreateEmployee, SendEmployeeMessage};
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, RunId};
use aurelius_protocol::EmployeeTaskOp;
use std::path::Path;
use std::time::Duration;

async fn hire(client: &Client, name: &str) -> (EmployeeId, ConversationId) {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Dev".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    (employee_id, conversation_id)
}

async fn answers(client: &Client, chat: ConversationId, wanted: usize) -> Vec<aurelius_protocol::MessageView> {
    for _ in 0..800 {
        let all = client.messages(chat).await.unwrap().messages;
        if all.iter().filter(|m| m.role == MessageRole::Assistant && m.status == MessageStatus::Complete).count() >= wanted {
            return all;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no {wanted} answers in {chat}");
}

async fn say(client: &Client, employee_id: EmployeeId, conversation_id: ConversationId) -> RunId {
    let message = SendEmployeeMessage { employee_id, text: "hello".into(), conversation_id, images: Vec::new(), files: Vec::new(), interrupt: false, confirm_secret: false };
    let sent = client.execute(envelope(Command::SendEmployeeMessage(message), None)).await.unwrap().result;
    let CommandResult::EmployeeMessageSent { run_id, .. } = sent else { panic!("{sent:?}") };
    run_id
}

fn due_now(root: &Path) {
    let write = rusqlite::Connection::open(root.join("data/aurelius.sqlite")).unwrap();
    write.busy_timeout(Duration::from_secs(5)).unwrap();
    write.execute("UPDATE scheduled_messages SET due_at_ms = 1 WHERE state = 'scheduled'", []).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scheduled_message_wakes_the_employee_and_shows_what_came_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect_as(dir.path(), "grace").await;
    let (ann, chat) = hire(&client, "Ann").await;
    let run = say(&client, ann, chat).await;
    answers(&client, chat, 1).await;

    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let (ok, text) = bridge.employee_act(run, EmployeeTaskOp::ScheduleMessage { delay: "10m".into(), content: "Check the CI of PR 12.".into(), to: None }).await.unwrap();
    assert!(ok && text.contains("10m"), "{text}");
    let waiting = client.scheduled_messages().await.unwrap().messages;
    assert_eq!((waiting.len(), waiting[0].state.as_str(), waiting[0].to_name.as_str()), (1, "scheduled", "Ann"));

    // A board member takes one back.
    let (ok, _) = bridge.employee_act(run, EmployeeTaskOp::ScheduleMessage { delay: "1h".into(), content: "Never mind this one.".into(), to: None }).await.unwrap();
    assert!(ok);
    let soonest_last = client.scheduled_messages().await.unwrap().messages;
    let later = soonest_last.iter().find(|m| m.text.starts_with("Never")).unwrap().id;
    let done = client.execute(envelope(Command::CancelScheduledMessage(CancelScheduledMessage { message_id: later }), None)).await.unwrap().result;
    assert_eq!(done, CommandResult::ScheduledMessageCancelled { message_id: later });

    due_now(dir.path());
    let all = answers(&client, chat, 2).await;
    assert!(all.iter().any(|m| m.update.as_deref() == Some("scheduled") && m.text.ends_with("Check the CI of PR 12.")), "{all:#?}");
    let listed = client.scheduled_messages().await.unwrap().messages;
    let sent = listed.iter().find(|m| m.text.starts_with("Check")).unwrap();
    assert_eq!(sent.state, "delivered");
    assert!(sent.run_id.is_some());
    assert_eq!(listed.iter().find(|m| m.text.starts_with("Never")).unwrap().state, "cancelled");

    // The scratchpad.
    let (ok, text) = bridge.employee_act(run, EmployeeTaskOp::Scratchpad { content: "Focus: the CSV export.".into() }).await.unwrap();
    assert!(ok, "{text}");
    let pad = client.employee_scratchpad(ann).await.unwrap();
    assert_eq!((pad.text.as_str(), pad.updated_at_ms.is_some()), ("Focus: the CSV export.", true));
    stop(dir.path(), daemon).await;
}
