//! The `project_id` filter on the reads whose rows do not say their project (feature
//! `workspace_filters`), through the real daemon: a workspace sees that project's rows,
//! and without the filter nothing changes.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::Client;
use aurelius_domain::approval::ApprovalStatus;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::group_chat::SendGroupMessage;
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::TaskStatus;
use aurelius_domain::ticket::*;
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, ProjectId, RunId, TicketId};
use aurelius_protocol::{Method, ResponseBody};
use aurelius_storage::Store;
use std::path::Path;
use std::time::Duration;

async fn run(client: &Client, command: Command) -> CommandResult {
    client.execute(aurelius_client::envelope(command, None)).await.unwrap().result
}

async fn project(client: &Client, name: &str) -> (ProjectId, ConversationId) {
    let CommandResult::ProjectCreated { project_id, .. } = run(client, Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None })).await else { panic!() };
    let channel = client.channels().await.unwrap().channels.into_iter().find(|c| c.project_id == Some(project_id)).expect("its channel").conversation_id;
    (project_id, channel)
}

async fn hire(client: &Client) -> EmployeeId {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Forge".into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = run(client, Command::CreateEmployee(create)).await else { panic!() };
    employee_id
}

async fn ticket(client: &Client, project_id: ProjectId, owner: Option<EmployeeId>) -> TicketId {
    let CommandResult::TicketCreated { ticket_id, .. } = client.create_ticket(new_ticket(project_id, owner)).await.unwrap().result else { panic!("not created") };
    ticket_id
}

fn new_ticket(project_id: ProjectId, owner: Option<EmployeeId>) -> CreateTicket {
    CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id,
        title: "Work".into(),
        description: None,
        priority: TicketPriority::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: owner,
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: None,
        from_idea: None,
    }
}

async fn post(client: &Client, ticket_id: TicketId, text: &str) {
    run(client, Command::PostTicketMessage(PostTicketMessage { confirm_secret: false, ticket_id, text: text.into(), images: vec![], files: vec![], thread_parent: None })).await;
}

async fn say(client: &Client, channel: Option<ConversationId>, text: &str) {
    run(client, Command::SendGroupMessage(SendGroupMessage { confirm_secret: false, text: text.into(), images: vec![], files: vec![], channel, thread_parent: None })).await;
}

async fn live_run(root: &Path, conversation: ConversationId) -> RunId {
    for _ in 0..2000 {
        let found: Option<String> = db(root).query_row("SELECT id FROM runs WHERE conversation_id = ?1 AND status IN ('starting', 'running') LIMIT 1", [conversation.to_string()], |r| r.get(0)).ok();
        if let Some(run) = found {
            return RunId::parse(&run).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("no live run in {conversation}");
}

async fn notifications(client: &Client, project_id: Option<ProjectId>) -> Vec<(Option<TicketId>, Option<ConversationId>)> {
    let ResponseBody::Notifications(snapshot) = client.call(Method::NotificationList { project_id }).await.unwrap() else { panic!() };
    assert_eq!(snapshot.unread as usize, snapshot.notifications.iter().filter(|n| !n.read).count(), "unread counts what the filter shows");
    snapshot.notifications.into_iter().map(|n| (n.ticket_id, n.conversation_id)).collect()
}

async fn pending_approvals(client: &Client, project_id: Option<ProjectId>) -> usize {
    let ResponseBody::Approvals(snapshot) = client.call(Method::ApprovalList { project_id }).await.unwrap() else { panic!() };
    snapshot.approvals.iter().filter(|a| a.status == ApprovalStatus::Pending).count()
}

async fn activity_threads(client: &Client, project_id: Option<ProjectId>) -> Vec<String> {
    let method = Method::ActivityLogList { project_id, employee_id: None, thread_key: None, ticket_number: None, run_id: None, category: None, before_seq: None, limit: None };
    let ResponseBody::ActivityLog(page) = client.call(method).await.unwrap() else { panic!() };
    page.entries.into_iter().filter_map(|e| e.thread_key).collect()
}

async fn scheduled(client: &Client, project_id: Option<ProjectId>) -> Vec<String> {
    let ResponseBody::ScheduledMessages(snapshot) = client.call(Method::ScheduledMessageList { project_id }).await.unwrap() else { panic!() };
    snapshot.messages.into_iter().map(|m| m.text).collect()
}

/// A message `from` schedules: in a ticket's thread, or in a channel.
fn plan(root: &Path, from: EmployeeId, text: &str, thread_key: String, ticket: Option<TicketId>, discussion: Option<ConversationId>) {
    use aurelius_storage::scheduled::{self, NewScheduled};
    let store = Store::open(root).unwrap();
    let text = text.to_owned();
    store
        .write(move |tx| {
            scheduled::insert(tx, NewScheduled { from, to: from, content: &text, discussion, chat: None, thread_key: &thread_key, ticket, thread_parent: None, place: "there", hop: 0, login_user: None, planned_by_run: RunId::new(), due_at_ms: 4_000_000_000_000 })
        })
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_reads_of_a_workspace_show_only_that_project() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 400).await;
    let (alex, bea) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await);
    let forge = hire(&alex).await;
    let (atlas, atlas_channel) = project(&alex, "Atlas").await;
    let (zephyr, zephyr_channel) = project(&alex, "Zephyr").await;
    let (in_atlas, in_zephyr) = (ticket(&alex, atlas, None).await, ticket(&alex, zephyr, None).await);

    // A run in a ticket of Atlas: its approval and its steps are Atlas's.
    let started = ticket(&alex, atlas, Some(forge)).await;
    alex.ticket_command(Command::StartTicket(StartTicket { ticket_id: started, employee_id: None }), None).await.unwrap();
    let conversation = loop {
        let root = alex.ticket(started).await.unwrap().root_task;
        if let Some(conversation) = root.filter(|t| t.status == TaskStatus::Running || t.status == TaskStatus::Queued).and_then(|t| t.conversation_id) {
            break conversation;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    let run_id = live_run(dir.path(), conversation).await;
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let asked = tokio::spawn(async move { bridge.permission_ask(run_id, Some("bash-1".into()), "Bash".into(), serde_json::json!({ "command": "dir" })).await });
    for _ in 0..400 {
        if pending_approvals(&alex, None).await == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(pending_approvals(&alex, None).await, 1);
    assert_eq!(pending_approvals(&alex, Some(atlas)).await, 1);
    assert_eq!(pending_approvals(&alex, Some(zephyr)).await, 0);
    asked.abort();

    let of_ticket = format!("ticket:{started}");
    assert!(activity_threads(&alex, None).await.contains(&of_ticket));
    assert!(activity_threads(&alex, Some(atlas)).await.contains(&of_ticket));
    assert!(!activity_threads(&alex, Some(zephyr)).await.contains(&of_ticket));

    // Notifications: tickets and channels, and the general channel belongs to no project.
    post(&alex, in_atlas, "@bea look").await;
    post(&alex, in_zephyr, "@bea look").await;
    say(&alex, Some(atlas_channel), "@bea here").await;
    say(&alex, Some(zephyr_channel), "@bea there").await;
    say(&alex, None, "@bea everyone").await;
    let everything = notifications(&bea, None).await;
    // Two tickets, three channels, and the approval of the run: no ticket on it, it belongs through its task.
    assert_eq!(everything.len(), 6, "{everything:?}");
    let only_atlas = notifications(&bea, Some(atlas)).await;
    assert_eq!(only_atlas.len(), 3, "{only_atlas:?}");
    assert_eq!(notifications(&bea, Some(zephyr)).await.len(), 2);
    assert!(only_atlas.iter().any(|(t, _)| *t == Some(in_atlas)));
    assert!(only_atlas.iter().any(|(t, c)| t.is_none() && *c == Some(atlas_channel)));
    assert!(!only_atlas.iter().any(|(t, c)| *t == Some(in_zephyr) || *c == Some(zephyr_channel)));
    assert!(notifications(&bea, Some(ProjectId::new())).await.is_empty(), "an unknown project has nothing, it is no error");

    // Scheduled messages: a ticket's thread and a channel.
    let zephyr_discussion = alex.ticket(in_zephyr).await.unwrap().ticket.conversation_id;
    plan(dir.path(), forge, "in zephyr", format!("ticket:{in_zephyr}"), Some(in_zephyr), zephyr_discussion);
    plan(dir.path(), forge, "in atlas", format!("channel:{atlas_channel}"), None, Some(atlas_channel));
    let sorted = |mut v: Vec<String>| {
        v.sort();
        v
    };
    assert_eq!(sorted(scheduled(&alex, None).await), ["in atlas", "in zephyr"]);
    assert_eq!(scheduled(&alex, Some(atlas)).await, ["in atlas"]);
    assert_eq!(scheduled(&alex, Some(zephyr)).await, ["in zephyr"]);

    stop(dir.path(), daemon).await;
}
