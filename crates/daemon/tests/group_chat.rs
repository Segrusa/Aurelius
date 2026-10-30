//! The company's group chat (`group_chat.get` / `group_chat.send`): without a
//! name the CEO answers, `@A @B` wakes both, each answers from its own main chat
//! and the answer comes back in the group under its name, streamed there live.
//! Employees read and post with `read_group_chat` / `post_to_group_chat`; a post
//! wakes only the colleagues it names, and the hop limit stops ping-pong.

mod common;

use aurelius_client::bridge::{BridgeClient, EmployeeTaskOp};
use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, RunId,
    conversation::{DeleteConversation, MessageRole, SendChatMessage},
    employee::CreateEmployee,
    group_chat::{GROUP_KIND, MAX_HOPS, SendGroupMessage},
    views::MessageView,
};
use aurelius_protocol::{CommandResult, StreamDelta};
use common::*;
use std::time::Duration;
use tokio::sync::mpsc;

async fn hire(client: &Client, name: &str, role: &str, reports_to: Option<EmployeeId>) -> (EmployeeId, ConversationId) {
    let create = CreateEmployee { name: name.into(), role: role.into(), title: None, reports_to, instructions: None, folder: None, department_id: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!("not hired") };
    (employee_id, conversation_id)
}

async fn send(client: &Client, text: &str) -> (Vec<EmployeeId>, Vec<RunId>) {
    let send = SendGroupMessage { text: text.into(), images: vec![], files: vec![] };
    let CommandResult::GroupMessageSent { to, run_ids, .. } = client.execute(envelope(Command::SendGroupMessage(send), None)).await.unwrap().result else { panic!("not sent") };
    (to, run_ids)
}

/// The group's messages by `author` (an employee's), once there are `count` of them.
async fn answers_of(client: &Client, author: &str, count: usize) -> Vec<MessageView> {
    for _ in 0..800 {
        let found: Vec<MessageView> = client.group_chat().await.unwrap().messages.into_iter().filter(|m| m.author.as_deref() == Some(author)).collect();
        if found.len() >= count {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{author} did not post {count} message(s) in the group chat");
}

/// The run the group chat woke in `chat` (an employee's main chat) at `hop`.
fn woken_run(root: &std::path::Path, chat: ConversationId, hop: u32) -> Option<RunId> {
    db(root)
        .query_row(
            "SELECT id FROM jobs WHERE json_extract(payload_json, '$.conversation_id') = ?1 AND json_extract(payload_json, '$.group.hop') = ?2 ORDER BY rowid DESC LIMIT 1",
            rusqlite::params![chat.to_string(), hop],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|id| RunId::parse(&id).ok())
}

async fn wait_for_run(root: &std::path::Path, chat: ConversationId, hop: u32) -> RunId {
    for _ in 0..400 {
        if let Some(run) = woken_run(root, chat, hop) {
            return run;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no run at hop {hop}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ceo_answers_named_employees_answer_and_posts_stop_at_the_hop_limit() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (events_tx, events) = mpsc::unbounded_channel();
    let (streams_tx, mut streams) = mpsc::unbounded_channel::<StreamDelta>();
    let client = Client::connect_with_streams(&aurelius_host::endpoint(dir.path()), "test", events_tx, Some(streams_tx)).await.unwrap();
    std::mem::forget(events);
    let after = client.health().await.unwrap().last_seq;
    client.subscribe(after).await.unwrap();
    let (atlas, atlas_chat) = hire(&client, "Atlas", "CEO", None).await;
    let (ann, ann_chat) = hire(&client, "Ann", "Developer", Some(atlas)).await;
    let (bo, bo_chat) = hire(&client, "Bo", "Reviewer", Some(atlas)).await;

    // One group chat, readable before anyone wrote in it.
    let group = client.group_chat().await.unwrap();
    assert!(group.messages.is_empty());
    let group_id = group.conversation_id;
    assert_eq!((group.employee_id, group.project_id), (None, None));

    // Without a name: the CEO answers, from its own main chat, and the answer is copied to the group.
    let (to, runs) = send(&client, "Hallo team, hoe staat het?").await;
    assert_eq!((to, runs.len()), (vec![atlas], 1));
    let answer = answers_of(&client, "Atlas", 1).await.remove(0);
    assert_eq!(answer.update.as_deref(), Some(GROUP_KIND));
    assert!(!answer.text.is_empty());
    let forwarded = client.employee_chat(atlas).await.unwrap().messages;
    let update = forwarded.iter().find(|m| m.update.as_deref() == Some(GROUP_KIND)).expect("the message reached the CEO's main chat");
    assert_eq!(update.author.as_deref(), Some("Owner"));
    assert!(update.text.contains("Hallo team, hoe staat het?"), "{}", update.text);
    assert!(forwarded.iter().any(|m| m.role == MessageRole::Assistant && m.reply_to == Some(update.id)), "the CEO answered in its main chat");
    // The answer streamed live in the group, under the id it got there.
    let mut streamed = false;
    while let Ok(delta) = streams.try_recv() {
        streamed |= delta.conversation_id == group_id && delta.message_id == answer.id;
    }
    assert!(streamed, "the answer streamed in the group chat");

    // @Ann @bo (any case): both answer, each under its own name; the CEO does not.
    let (to, runs) = send(&client, "@Ann en @bo: wie pakt de release op?").await;
    assert_eq!((to, runs.len()), (vec![ann, bo], 2));
    answers_of(&client, "Ann", 1).await;
    answers_of(&client, "Bo", 1).await;
    assert_eq!(answers_of(&client, "Atlas", 1).await.len(), 1, "the CEO was not woken");
    let group = client.group_chat().await.unwrap().messages;
    assert_eq!(group.iter().filter(|m| m.role == MessageRole::User).count(), 2, "the user's messages are plain user messages");
    // Bo's update carries the earlier group messages as context.
    let bo_update = client.employee_chat(bo).await.unwrap().messages.into_iter().find(|m| m.update.as_deref() == Some(GROUP_KIND)).unwrap();
    assert!(bo_update.text.contains("Owner: Hallo team, hoe staat het?") && bo_update.text.contains("Atlas: "), "{}", bo_update.text);

    // An employee reads the group chat and posts in it; a post without a name wakes nobody,
    // naming itself neither.
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let ann_run = wait_for_run(dir.path(), ann_chat, 0).await;
    let (ok, read) = bridge.employee_act(ann_run, EmployeeTaskOp::ReadGroupChat { last: None }).await.unwrap();
    assert!(ok && read.contains("Owner: @Ann en @bo: wie pakt de release op?"), "{read}");
    let (ok, posted) = bridge.employee_act(ann_run, EmployeeTaskOp::PostToGroupChat { message: "Ik pak het op, @Ann noteert.".into() }).await.unwrap();
    assert_eq!((ok, posted.as_str()), (true, "Posted in the group chat."));
    let posts = answers_of(&client, "Ann", 2).await;
    assert!(posts.iter().any(|m| m.text == "Ik pak het op, @Ann noteert."), "the post is in the group");

    // Ping-pong: Ann (hop 0) wakes Bo (hop 1), Bo wakes Ann (2), Ann wakes Bo (3); Bo can no longer wake Ann.
    let (ok, text) = bridge.employee_act(ann_run, EmployeeTaskOp::PostToGroupChat { message: "Klaar. @Bo, review jij?".into() }).await.unwrap();
    assert!(ok && text.contains("Bo will answer"), "{text}");
    let mut run = wait_for_run(dir.path(), bo_chat, 1).await;
    let chats = [(ann_chat, "Ann"), (bo_chat, "Bo")];
    for hop in 2..=MAX_HOPS {
        let (chat, name) = chats[(hop % 2) as usize];
        let (ok, text) = bridge.employee_act(run, EmployeeTaskOp::PostToGroupChat { message: format!("@{name}, nog één ding.") }).await.unwrap();
        assert!(ok && text.contains(&format!("{name} will answer")), "hop {hop}: {text}");
        run = wait_for_run(dir.path(), chat, hop).await;
    }
    let (ok, text) = bridge.employee_act(run, EmployeeTaskOp::PostToGroupChat { message: "@Ann, en nog één.".into() }).await.unwrap();
    assert!(ok && text.contains("will not be woken"), "{text}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(woken_run(dir.path(), ann_chat, MAX_HOPS + 1).is_none() && woken_run(dir.path(), bo_chat, MAX_HOPS + 1).is_none(), "nobody woke past the hop limit");
    let refused_post = client.group_chat().await.unwrap().messages.into_iter().any(|m| m.text == "@Ann, en nog één.");
    assert!(refused_post, "the post itself is still shown");

    // The group chat is not a project conversation: no chat.send, no delete. The main chats stay.
    let wrong = Command::SendChatMessage(SendChatMessage { conversation_id: group_id, text: "x".into(), images: vec![], interrupt: false });
    assert!(client.execute(envelope(wrong, None)).await.is_err());
    assert!(client.execute(envelope(Command::DeleteConversation(DeleteConversation { conversation_id: group_id }), None)).await.is_err());
    assert_eq!(client.group_chat().await.unwrap().conversation_id, group_id);
    let employees = client.employees(None).await.unwrap().employees;
    assert_eq!(employees.len(), 3);
    assert!(employees.iter().any(|e| e.id == atlas && e.conversation_id == atlas_chat));

    stop(dir.path(), daemon).await;
}
