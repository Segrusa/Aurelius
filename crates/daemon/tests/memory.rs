//! B17 acceptance: scoped memory with provenance, no cross-employee/project
//! leakage, visible conflicts, gap-free summary coverage, and an explicit
//! block when mandatory context is too large.
//!
//! An employee answers in the conversation of a task, so each test gives its
//! employees a task first and then keeps talking in that conversation.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, ProjectId,
    conversation::{MessageRole, MessageStatus},
    employee::{CreateEmployee, SendEmployeeMessage},
    memory::{DecideMemory, ForgetMemory, MemoryKind, MemoryScope, MemoryStatus, Remember, ReviseMemory},
    project::CreateProject,
    task::{CreateTask, TaskStatus},
};
use aurelius_protocol::{CommandResult, MemoryView, MessageView};
use common::*;
use std::path::Path;
use std::time::Duration;

async fn start(root: &Path) -> Daemon {
    // SAFETY (test): all tests in this binary set the same value.
    unsafe { std::env::set_var("AURELIUS_CONTEXT_BUDGET", "4000") };
    start_with_delay(root, 1).await
}

async fn project(client: &Client, name: &str) -> ProjectId {
    match client.execute(envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap().result {
        CommandResult::ProjectCreated { project_id, .. } => project_id,
        other => panic!("{other:?}"),
    }
}

async fn employee(client: &Client, project_id: ProjectId, name: &str) -> EmployeeId {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Onderzoeker".into(), title: None, reports_to: None, instructions: None, folder: None };
    match client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result {
        CommandResult::EmployeeCreated { employee_id, .. } => employee_id,
        other => panic!("{other:?}"),
    }
}

/// Give the employee a task and wait until it is done: returns the task's conversation.
async fn work(client: &Client, project_id: ProjectId, employee_id: EmployeeId, title: &str, goal: &str) -> ConversationId {
    let task = CreateTask {
        confirm_secret: false,
        project_id,
        employee_id: Some(employee_id),
        title: title.into(),
        goal: goal.into(),
        acceptance_criteria: vec!["Elk resultaat noemt een bron".into()],
        depends_on: vec![],
        run: Default::default(),
        files: vec![],
    };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(task), None)).await.unwrap().result else { panic!("not a task") };
    for _ in 0..400 {
        let tasks = client.tasks(Some(project_id)).await.unwrap().tasks;
        if let Some(t) = tasks.iter().find(|t| t.id == task_id && t.status == TaskStatus::Succeeded) {
            return t.conversation_id.expect("a task conversation");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the task did not finish");
}

/// What the project's and the employee's memory views show (project, own, global).
async fn memory(client: &Client, project: ProjectId, employee: EmployeeId) -> Vec<MemoryView> {
    client.memory_of(Some(project), Some(employee)).await.unwrap().items
}

/// A message to the employee in a task conversation, and the answer to it.
async fn ask(client: &Client, employee: EmployeeId, conversation: ConversationId, text: &str) -> MessageView {
    let send = SendEmployeeMessage { confirm_secret: false, employee_id: employee, text: text.into(), conversation_id: conversation, images: Vec::new(), files: Vec::new(), interrupt: false };
    let CommandResult::EmployeeMessageSent { message_id, .. } = client.execute(envelope(Command::SendEmployeeMessage(send), None)).await.unwrap().result else { panic!("not sent") };
    for _ in 0..400 {
        let messages = client.messages(conversation).await.unwrap().messages;
        if let Some(a) = messages.iter().find(|m| m.role == MessageRole::Assistant && m.reply_to == Some(message_id) && m.status != MessageStatus::Partial) {
            return a.clone();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no answer");
}

async fn wait_memory(client: &Client, project: ProjectId, employee: EmployeeId, done: impl Fn(&[MemoryView]) -> bool) -> Vec<MemoryView> {
    for _ in 0..400 {
        let items = memory(client, project, employee).await;
        if done(&items) {
            return items;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("memory did not reach the expected state: {:#?}", memory(client, project, employee).await);
}

#[tokio::test(flavor = "multi_thread")]
async fn extraction_scopes_provenance_and_no_leakage() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    let other = project(&client, "Ander").await;
    let alex = employee(&client, atlas, "Alex").await;
    let sam = employee(&client, atlas, "Sam").await;
    let olga = employee(&client, other, "Olga").await;

    let alex_conv = work(&client, atlas, alex, "Onboarding", "Vergelijk ideeën voor de onboarding van nieuwe klanten").await;

    // Extraction: an automatic episode for Alex, a knowledge proposal for the project.
    let items = wait_memory(&client, atlas, alex, |m| m.len() >= 2).await;
    let episode = items.iter().find(|m| m.kind == MemoryKind::Episodic).unwrap().clone();
    assert_eq!((episode.scope.as_str(), episode.status, episode.decided_by.as_deref()), ("employee", MemoryStatus::Active, Some("policy")));
    assert_eq!(episode.employee_id, Some(alex));
    let proposal = items.iter().find(|m| m.kind == MemoryKind::ProjectKnowledge).unwrap().clone();
    assert_eq!((proposal.scope.as_str(), proposal.status), ("project", MemoryStatus::Proposed));
    // Provenance is mandatory: every memory points at real events.
    assert!(items.iter().all(|m| !m.sources.is_empty()), "{items:#?}");
    assert!(episode.sources[0].label.contains("Onboarding"));
    let atlas_ids: Vec<String> = items.iter().map(|m| m.id.to_string()).collect();

    // A proposal is not used until accepted.
    let sam_conv = work(&client, atlas, sam, "Support", "Help het team").await;
    let answer = ask(&client, sam, sam_conv, "Wat is het doel van de onboarding?").await;
    let ctx = client.context(answer.run_id.unwrap()).await.unwrap();
    assert!(!ctx.items.iter().any(|i| i.section == "memory" && i.id == proposal.id.to_string()));
    client.execute(envelope(Command::DecideMemory(DecideMemory { memory_id: proposal.id, accept: true }), None)).await.unwrap();

    // Sam (same project) sees project knowledge but never Alex's private episode.
    let answer = ask(&client, sam, sam_conv, "Wat is het doel van de onboarding van klanten?").await;
    let ctx = client.context(answer.run_id.unwrap()).await.unwrap();
    let used: Vec<&str> = ctx.items.iter().filter(|i| i.section == "memory" && i.included).map(|i| i.id.as_str()).collect();
    assert!(used.contains(&proposal.id.to_string().as_str()), "{ctx:#?}");
    assert!(!ctx.items.iter().any(|i| i.id == episode.id.to_string()), "employee memory leaked");
    // Alex sees both.
    let answer = ask(&client, alex, alex_conv, "Hoe ging de onboarding taak?").await;
    let ctx = client.context(answer.run_id.unwrap()).await.unwrap();
    assert!(ctx.items.iter().any(|i| i.id == episode.id.to_string() && i.included), "{ctx:#?}");
    // Another project: nothing from Atlas.
    let olga_conv = work(&client, other, olga, "Research", "Zoek iets op").await;
    let answer = ask(&client, olga, olga_conv, "Wat weet je over onboarding?").await;
    let ctx = client.context(answer.run_id.unwrap()).await.unwrap();
    assert!(!ctx.items.iter().any(|i| i.section == "memory" && atlas_ids.contains(&i.id)), "cross-project leakage: {ctx:#?}");
    assert!(memory(&client, other, olga).await.iter().all(|m| !atlas_ids.contains(&m.id.to_string())));

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn revise_supersedes_and_forget_erases() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    let alex = employee(&client, atlas, "Alex").await;
    let conv = work(&client, atlas, alex, "Setup", "Zet alles klaar").await;

    let remember = |content: &str| Command::Remember(Remember { scope: MemoryScope::Project(atlas), kind: MemoryKind::Decision, content: content.into(), pinned: false });
    let CommandResult::MemoryStored { memory_id: first } = client.execute(envelope(remember("Publicatie vereist voorafgaande review"), None)).await.unwrap().result else { panic!() };
    let CommandResult::MemoryStored { memory_id: second } =
        client.execute(envelope(Command::ReviseMemory(ReviseMemory { memory_id: first, content: "Publicatie vereist review door Alex".into() }), None)).await.unwrap().result
    else {
        panic!()
    };
    let items = memory(&client, atlas, alex).await;
    let old = items.iter().find(|m| m.id == first).unwrap();
    let new = items.iter().find(|m| m.id == second).unwrap();
    assert_eq!(old.status, MemoryStatus::Superseded, "history stays visible");
    assert_eq!(new.supersedes_id, Some(first));
    assert!(new.sources.len() >= 2, "provenance survives a revision");

    // Current decisions enter the context even without word overlap.
    let answer = ask(&client, alex, conv, "Mag ik het artikel morgen plaatsen?").await;
    let ctx = client.context(answer.run_id.unwrap()).await.unwrap();
    assert!(ctx.items.iter().any(|i| i.id == second.to_string() && i.included));
    assert!(!ctx.items.iter().any(|i| i.id == first.to_string()));

    client.execute(envelope(Command::ForgetMemory(ForgetMemory { memory_id: second }), None)).await.unwrap();
    let forgotten = memory(&client, atlas, alex).await.into_iter().find(|m| m.id == second).unwrap();
    assert_eq!((forgotten.status, forgotten.content.as_str()), (MemoryStatus::Forgotten, ""));
    let answer = ask(&client, alex, conv, "Mag ik het artikel morgen plaatsen?").await;
    let ctx = client.context(answer.run_id.unwrap()).await.unwrap();
    assert!(!ctx.items.iter().any(|i| i.id == second.to_string()));

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn summary_coverage_has_no_gap_and_oversized_context_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    let alex = employee(&client, atlas, "Alex").await;
    let conv = work(&client, atlas, alex, "Planning", "Plan het werk").await;

    // Enough complete messages pile up in the task's conversation: a summary follows.
    for i in 0..12 {
        ask(&client, alex, conv, &format!("Punt {i}: we bespreken de planning")).await;
        if memory(&client, atlas, alex).await.iter().any(|m| m.kind == MemoryKind::ConversationSummary) {
            break;
        }
    }
    let items = wait_memory(&client, atlas, alex, |m| m.iter().any(|x| x.kind == MemoryKind::ConversationSummary)).await;
    let summary = items.iter().find(|m| m.kind == MemoryKind::ConversationSummary).unwrap();
    assert_eq!(summary.scope, "employee");

    let answer = ask(&client, alex, conv, "En wat nu?").await;
    let ctx = client.context(answer.run_id.unwrap()).await.unwrap();
    let summary_item = ctx.items.iter().find(|i| i.section == "summary").expect("summary in context");
    assert!(summary_item.included, "{summary_item:?}");
    let covered: u32 = summary_item.label.rsplit(' ').next().unwrap().parse().expect("the label ends with the covered message number");
    // Exact turns after the summary only: no gap, no duplicate.
    let turns: Vec<u32> = ctx.items.iter().filter(|i| i.section == "turn" && i.included).map(|i| i.id.parse().unwrap()).collect();
    assert!(!turns.is_empty() && turns.iter().all(|o| *o > covered), "{turns:?} after {covered}");

    // Mandatory context larger than the budget: explicit failure, no silent cut.
    let huge = "woord ".repeat(5_000);
    let answer = ask(&client, alex, conv, &huge).await;
    assert_eq!(answer.status, MessageStatus::Failed);
    assert!(answer.text.contains("does not fit in the budget"), "{}", answer.text);

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn memory_context_shows_placement_and_tokens_per_employee() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    let alex = employee(&client, atlas, "Alex").await;
    let sam = employee(&client, atlas, "Sam").await;

    // A task for Alex leaves a private episode and a knowledge proposal.
    work(&client, atlas, alex, "Onboarding", "Vergelijk ideeën voor de onboarding van nieuwe klanten").await;
    let items = wait_memory(&client, atlas, alex, |m| m.len() >= 2).await;
    let proposal = items.iter().find(|m| m.status == MemoryStatus::Proposed).unwrap().id;
    let episode = items.iter().find(|m| m.kind == MemoryKind::Episodic).unwrap().id;

    let remember = |kind, content: String, pinned| Command::Remember(Remember { scope: MemoryScope::Project(atlas), kind, content, pinned });
    client.execute(envelope(remember(MemoryKind::Decision, "Nooit force-pushen naar main".into(), true), None)).await.unwrap();
    client.execute(envelope(remember(MemoryKind::ProjectKnowledge, "De klantportal draait op poort 8443".into(), false), None)).await.unwrap();
    for n in 0..40 {
        let content = format!("Beslissing {n}: {}", "uitleg over de afspraak ".repeat(30));
        client.execute(envelope(remember(MemoryKind::Decision, content, false), None)).await.unwrap();
    }

    let view = client.memory_context(Some(atlas), Some(sam)).await.unwrap();
    assert_eq!((view.project_id, view.employee_id, view.memory_budget_tokens), (Some(atlas), Some(sam), 4_000));
    let placement = |content: &str| view.items.iter().find(|i| i.content.contains(content)).unwrap_or_else(|| panic!("{content} missing")).placement.clone();
    assert_eq!(placement("Nooit force-pushen"), "always");
    assert_eq!(view.items[0].content, "Nooit force-pushen naar main", "pinned decisions come first");
    assert_eq!(placement("poort 8443"), "when_relevant");
    assert!(view.items.iter().any(|i| i.placement == "over_budget" && i.content.starts_with("Beslissing")));
    assert!(view.items.iter().all(|i| i.tokens > 0));
    let always: u32 = view.items.iter().filter(|i| i.placement == "always").map(|i| i.tokens).sum();
    assert_eq!(view.always_tokens, always);
    assert!(always <= view.memory_budget_tokens && view.system_tokens > 0);
    let order: Vec<&str> = view.items.iter().map(|i| i.placement.as_str()).collect();
    assert!(order.windows(2).all(|w| w[0] == w[1] || ["always", "when_relevant", "over_budget", "duplicate"].iter().position(|p| *p == w[0]) < ["always", "when_relevant", "over_budget", "duplicate"].iter().position(|p| *p == w[1])), "{order:?}");
    assert_eq!(view.items.iter().find(|i| i.content.contains("poort 8443")).unwrap().label, "Knowledge · project");
    // Neither the proposal nor Alex's private episode is in Sam's context.
    assert!(!view.items.iter().any(|i| i.memory_id == proposal || i.memory_id == episode));

    // Alex gets his own episode; the proposal is still absent.
    let alex_view = client.memory_context(Some(atlas), Some(alex)).await.unwrap();
    let own = alex_view.items.iter().find(|i| i.memory_id == episode).expect("own private memory");
    assert_eq!((own.scope.as_str(), own.employee_name.as_deref()), ("employee", Some("Alex")));
    assert!(!alex_view.items.iter().any(|i| i.memory_id == proposal));

    // No employee: project and global scopes only.
    let plain = client.memory_context(Some(atlas), None).await.unwrap();
    assert!(plain.items.iter().any(|i| i.content.contains("Nooit force-pushen")));
    assert!(!plain.items.iter().any(|i| i.memory_id == episode));

    stop(dir.path(), daemon).await;
}
