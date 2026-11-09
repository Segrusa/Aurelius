//! A board member's lasting chat with an employee, with the real dispatcher: the
//! employee answers, the other board member does not get in, and a ticket that
//! reached Review is told once, in the employee's words.

mod common;
use common::*;

use aurelius_client::envelope;
use aurelius_domain::{
    Command, ConversationId,
    conversation::{MessageRole, MessageStatus, SendChatMessage},
    employee::{CreateEmployee, OpenEmployeeChat},
    project::CreateProject,
};
use aurelius_protocol::CommandResult;
use std::time::Duration;

async fn answers(client: &aurelius_client::Client, chat: ConversationId, wanted: usize) -> Vec<aurelius_protocol::MessageView> {
    for _ in 0..600 {
        let all = client.messages(chat).await.unwrap().messages;
        let done = all.iter().filter(|m| m.role == MessageRole::Assistant && m.status == MessageStatus::Complete).count();
        if done >= wanted {
            return all;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no {wanted} answers in {chat}");
}

async fn open(client: &aurelius_client::Client, employee_id: aurelius_domain::EmployeeId) -> ConversationId {
    match client.execute(envelope(Command::OpenEmployeeChat(OpenEmployeeChat { employee_id }), None)).await.unwrap().result {
        CommandResult::EmployeeChatOpened { conversation_id } => conversation_id,
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_board_member_talks_with_an_employee_and_hears_of_a_ticket_in_review_once() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    let robin = connect_as(dir.path(), "robin").await;
    let CommandResult::ProjectCreated { project_id, .. } = alex.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result else { panic!() };
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Ada".into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id: main } = alex.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    let chat = open(&alex, employee_id).await;
    assert_eq!(open(&alex, employee_id).await, chat, "it lasts");
    let theirs = open(&robin, employee_id).await;
    assert_ne!(chat, theirs);
    assert_ne!(chat, main);
    assert!(alex.employee_chats().await.unwrap().chats.is_empty(), "nothing said yet");

    alex.execute(envelope(Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: chat, text: "Hallo".into(), images: vec![], interrupt: false }), None)).await.unwrap();
    let all = answers(&alex, chat, 1).await;
    assert_eq!(all.iter().find(|m| m.role == MessageRole::User).and_then(|m| m.author_user.as_ref()).map(|u| u.username.as_str()), Some(BOARD_MEMBER));

    assert!(robin.messages(chat).await.is_err(), "only its board member reads it");
    assert!(robin.employee_chats().await.unwrap().chats.is_empty());
    let mine = alex.employee_chats().await.unwrap().chats;
    assert_eq!((mine.len(), mine[0].conversation_id, mine[0].employee_id, mine[0].employee_name.as_str(), mine[0].unread), (1, chat, employee_id, "Ada", 0));

    // A ticket from this chat that has reached Review, as `review_lines` records it.
    let ticket = backlog_ticket(&alex, project_id, "Rename the host").await;
    let write = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    write.busy_timeout(Duration::from_secs(5)).unwrap();
    write.execute("INSERT INTO chat_tickets(ticket_id, conversation_id, review_arrivals, reviews_told, created_at_ms) VALUES (?1, ?2, 1, 0, 1)", [ticket.to_string(), chat.to_string()]).unwrap();
    drop(write);

    let all = answers(&alex, chat, 2).await;
    assert!(all.iter().all(|m| m.update.as_deref() != Some("ticket_update")), "the trigger is the employee's, not shown");
    assert_eq!(all.iter().filter(|m| m.role == MessageRole::User).count(), 1, "the board member said one thing");
    let told: (i64, i64) = db(dir.path()).query_row("SELECT review_arrivals, reviews_told FROM chat_tickets WHERE ticket_id = ?1", [ticket.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(told, (1, 1));
    // Several dispatcher passes later there is still one update.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let triggers: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM events WHERE aggregate_id = ?1 AND metadata_json LIKE '%ticket_update%'", [chat.to_string()], |r| r.get(0)).unwrap();
    assert_eq!(triggers, 1);
    stop(dir.path(), daemon).await;
}
