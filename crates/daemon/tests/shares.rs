//! B18 acceptance: only the selected content moves, no hidden project access,
//! revoking stops future use (including memory made from it).

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, ProjectId, TaskId,
    conversation::{MessageRole, MessageStatus, SendChatMessage},
    employee::{CreateEmployee, SendEmployeeMessage},
    memory::{MemoryKind, MemoryStatus},
    project::CreateProject,
    share::{DiscussTask, RevokeShare, ShareMessage, ShareMode, ShareTarget},
    task::{CreateTask, TaskStatus},
};
use aurelius_protocol::{CommandResult, MessageView};
use common::*;
use std::time::Duration;

async fn project(client: &Client, name: &str) -> ProjectId {
    match client.execute(envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap().result {
        CommandResult::ProjectCreated { project_id, .. } => project_id,
        other => panic!("{other:?}"),
    }
}

async fn employee(client: &Client, project_id: ProjectId, name: &str) -> EmployeeId {
    match client
        .execute(envelope(
            Command::CreateEmployee(CreateEmployee { name: name.into(), role: "Researcher".into(), title: None, reports_to: None, instructions: None, folder: None }),
            None,
        ))
        .await
        .unwrap()
        .result
    {
        CommandResult::EmployeeCreated { employee_id, .. } => employee_id,
        other => panic!("{other:?}"),
    }
}

async fn give_task(client: &Client, project_id: ProjectId, employee: EmployeeId, title: &str, goal: &str) -> TaskId {
    let cmd = CreateTask { confirm_secret: false, files: vec![], run: Default::default(), project_id, employee_id: Some(employee), title: title.into(), goal: goal.into(), acceptance_criteria: vec![], depends_on: vec![] };
    match client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result {
        CommandResult::TaskCreated { task_id } => task_id,
        other => panic!("{other:?}"),
    }
}

/// The `count`-th finished answer (not a task update) in a conversation.
async fn last_answer(client: &Client, conversation: ConversationId, count: usize) -> MessageView {
    for _ in 0..400 {
        let done: Vec<MessageView> = client
            .messages(conversation)
            .await
            .unwrap()
            .messages
            .into_iter()
            .filter(|m| m.role == MessageRole::Assistant && m.update.is_none() && m.status != MessageStatus::Partial)
            .collect();
        if done.len() >= count {
            return done[count - 1].clone();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no answer");
}

/// The conversation a task is worked on in, once it exists.
async fn task_conversation(client: &Client, task: TaskId) -> ConversationId {
    for _ in 0..400 {
        if let Some(c) = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == task).and_then(|t| t.conversation_id) {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the task never got a conversation");
}

/// The context of the employee's `count`-th answer in the task's conversation.
async fn context_of_answer(client: &Client, conversation: ConversationId, count: usize) -> aurelius_protocol::ContextView {
    let answer = last_answer(client, conversation, count).await;
    client.context(answer.run_id.unwrap()).await.unwrap()
}

fn shared_in(ctx: &aurelius_protocol::ContextView) -> Vec<String> {
    ctx.items.iter().filter(|i| i.included && i.label.starts_with("Shared")).map(|i| i.label.clone()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn sharing_moves_only_the_snapshot_and_revoking_stops_it() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    let other = project(&client, "Other").await;
    let sam = employee(&client, atlas, "Sam").await;
    let olga = employee(&client, other, "Olga").await;

    // An answer in a project conversation to share.
    let CommandResult::ChatMessageSent { conversation_id: chat, .. } = client
        .execute(envelope(
            Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "Pricing idea: three tiers".into(), images: Vec::new(), interrupt: false }),
            None,
        ))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    let answer = last_answer(&client, chat, 1).await;

    // Context share to the Atlas project: Atlas employees see it, others don't.
    let CommandResult::Shared { share_id, memory_id: None } = client
        .execute(envelope(Command::ShareMessage(ShareMessage { conversation_id: chat, message_id: answer.id, target: ShareTarget::Project(atlas), mode: ShareMode::Context }), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    let sam_task = give_task(&client, atlas, sam, "Pricing", "What do we know about pricing?").await;
    let sam_conv = task_conversation(&client, sam_task).await;
    let ctx = context_of_answer(&client, sam_conv, 1).await;
    assert_eq!(shared_in(&ctx).len(), 1, "{ctx:#?}");
    let olga_task = give_task(&client, other, olga, "Pricing", "What do we know about pricing?").await;
    let olga_conv = task_conversation(&client, olga_task).await;
    let ctx = context_of_answer(&client, olga_conv, 1).await;
    assert!(shared_in(&ctx).is_empty(), "no hidden access to another project");
    let shares = client.shares(atlas).await.unwrap();
    assert_eq!(shares.len(), 1);
    assert!(shares[0].source.starts_with("Answer"));

    // Memory share to Sam: a proposal with the share as its source.
    let CommandResult::Shared { share_id: memory_share, memory_id: Some(memory) } = client
        .execute(envelope(Command::ShareMessage(ShareMessage { conversation_id: chat, message_id: answer.id, target: ShareTarget::Employee(sam), mode: ShareMode::Memory }), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    let items = client.memory_of(Some(atlas), Some(sam)).await.unwrap().items;
    let proposal = items.iter().find(|m| m.id == memory).unwrap();
    assert_eq!((proposal.status, proposal.kind, proposal.employee_id), (MemoryStatus::Proposed, MemoryKind::ProjectKnowledge, Some(sam)));

    // Revoke both: no longer in context, the derived memory is rejected.
    client.execute(envelope(Command::RevokeShare(RevokeShare { share_id }), None)).await.unwrap();
    client.execute(envelope(Command::RevokeShare(RevokeShare { share_id: memory_share }), None)).await.unwrap();
    client
        .execute(envelope(
            Command::SendEmployeeMessage(SendEmployeeMessage { confirm_secret: false, employee_id: sam, text: "And now, pricing?".into(), conversation_id: sam_conv, images: Vec::new(), files: Vec::new(), interrupt: false }),
            None,
        ))
        .await
        .unwrap();
    let ctx = context_of_answer(&client, sam_conv, 2).await;
    assert!(shared_in(&ctx).is_empty(), "{ctx:#?}");
    let items = client.memory_of(Some(atlas), Some(sam)).await.unwrap().items;
    assert_eq!(items.iter().find(|m| m.id == memory).unwrap().status, MemoryStatus::Rejected);
    assert!(client.shares(atlas).await.unwrap().iter().all(|s| s.revoked));

    // Messages from an employee's task conversation can't be shared out.
    let sam_answer = last_answer(&client, sam_conv, 1).await;
    let refused = client
        .execute(envelope(Command::ShareMessage(ShareMessage { conversation_id: sam_conv, message_id: sam_answer.id, target: ShareTarget::Project(other), mode: ShareMode::Context }), None))
        .await;
    assert!(refused.is_err());

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn discussing_a_task_gives_its_result_to_the_lead_employees_chat() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    let alex = employee(&client, atlas, "Alex").await;
    let cmd = CreateTask {
        confirm_secret: false,
        files: vec![],
        run: Default::default(),
        project_id: atlas,
        employee_id: Some(alex),
        title: "Onboarding".into(),
        goal: "Compare ideas".into(),
        acceptance_criteria: vec!["Every result cites a source".into()],
        depends_on: vec![],
    };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result else { panic!() };
    for _ in 0..400 {
        if client.tasks(None).await.unwrap().tasks.iter().any(|t| t.id == task_id && t.status == TaskStatus::Succeeded) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let CommandResult::TaskDiscussed { conversation_id, employee_id } = client.execute(envelope(Command::DiscussTask(DiscussTask { task_id }), None)).await.unwrap().result else { panic!() };
    // The lead employee's own chat, not a new conversation.
    assert_eq!(employee_id, alex);
    let main = client.employees().await.unwrap().employees.into_iter().find(|e| e.id == alex).unwrap().conversation_id;
    assert_eq!(conversation_id, main);
    let snapshot = client.messages(conversation_id).await.unwrap();
    assert_eq!(snapshot.shares.len(), 1);
    assert!(snapshot.shares[0].excerpt.starts_with("# Onboarding"));

    client
        .execute(envelope(
            Command::SendEmployeeMessage(SendEmployeeMessage { confirm_secret: false, employee_id: alex, text: "Summarize the result".into(), conversation_id, images: Vec::new(), files: Vec::new(), interrupt: false }),
            None,
        ))
        .await
        .unwrap();
    let answer = last_answer(&client, conversation_id, 1).await;
    let ctx = client.context(answer.run_id.unwrap()).await.unwrap();
    assert_eq!(shared_in(&ctx).len(), 1, "{ctx:#?}");
    stop(dir.path(), daemon).await;
}
