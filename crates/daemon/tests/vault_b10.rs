//! B10 (SEC0 E6) on every text command a board member sends (AUR-MU-04-F5):
//! without `confirm_secret` it is refused with the vault item's name only and
//! leaves nothing; with it the text is stored with «secret:ENV», and the
//! value is in no stored text, request hash or receipt.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::{Client, ClientError, bridge::BridgeClient};
use aurelius_domain::approval::{AnswerQuestion, ApprovalStatus, DecidePlan, PlanDecision};
use aurelius_domain::autonomy::AgentMode;
use aurelius_domain::conversation::SendChatMessage;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::idea::{AddIdea, UpdateIdea};
use aurelius_domain::project::CreateProject;
use aurelius_domain::project_conversation::SendProjectMessage;
use aurelius_domain::schedule::{CreateTaskSchedule, Recurrence, UpdateTaskSchedule};
use aurelius_domain::task::{CreateTask, RunSettings, TaskStatus};
use aurelius_domain::ticket::*;
use aurelius_domain::vault::VaultKind;
use aurelius_domain::{Command, CommandEnvelope, CommandId, CommandReceipt, CommandResult, ConversationId, EmployeeId, ErrorCode, ProjectId, RunId, TicketId};
use aurelius_protocol::VaultCreateParams;
use std::path::Path;
use std::time::Duration;

/// A vault secret of one command: its own name and value (the redactor is the
/// process's, shared by the daemons of this binary, so no value is another's prefix).
struct Secret {
    name: String,
    env: String,
    value: String,
}

impl Secret {
    fn marker(&self) -> String {
        format!("«secret:{}»", self.env)
    }
}

async fn secret(client: &Client, test: u8, n: u8) -> Secret {
    let s = Secret { name: format!("F5 key {test}-{n}"), env: format!("F5_KEY_{test}_{n}"), value: format!("sk-test-FAKE-0000-f5-{test}{n:02}-end") };
    let params = VaultCreateParams { project_id: None, kind: VaultKind::Secret, name: s.name.clone(), env_name: s.env.clone(), description: None, value: s.value.clone() };
    client.vault_create(params).await.unwrap();
    s
}

/// Where a text is kept: the tables (and their columns) and the blobs that hold it.
fn kept(root: &Path, needle: &str) -> Vec<String> {
    let conn = db(root);
    let tables: Vec<String> =
        conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'").unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
    let mut found = Vec::new();
    for table in tables {
        let columns: Vec<String> =
            conn.prepare(&format!("SELECT name FROM pragma_table_info('{table}')")).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
        for column in columns {
            let sql = format!("SELECT COUNT(*) FROM \"{table}\" WHERE instr(CAST(\"{column}\" AS TEXT), ?1) > 0");
            if conn.query_row(&sql, [needle], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0 {
                found.push(format!("{table}.{column}"));
            }
        }
    }
    for blob in walk(&root.join("blobs")) {
        if blob.windows(needle.len()).any(|w| w == needle.as_bytes()) {
            found.push("blob".into());
        }
    }
    found
}

fn walk(dir: &Path) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if let Ok(bytes) = std::fs::read(&path) {
            out.push(bytes);
        }
    }
    out
}

/// One command under B10. Without the flag: `looks_like_secret` with only the
/// item's name, no receipt. With it: stored with the marker, outside the
/// receipts too; the request hash is the redacted command's; the value nowhere.
async fn b10(root: &Path, client: &Client, s: &Secret, what: &str, make: impl Fn(bool) -> Command) -> CommandReceipt {
    let refused_key = CommandId::new();
    let refused = client.execute(CommandEnvelope { command_id: refused_key, expected_version: None, command: make(false) }).await.unwrap_err();
    let ClientError::Protocol(refused) = refused else { panic!("{what}: {refused:?}") };
    assert_eq!((refused.code, refused.matched_items.clone()), (ErrorCode::LooksLikeSecret, vec![s.name.clone()]), "{what}: refused with the item's name");
    assert!(!refused.message.contains(&s.value), "{what}");
    let receipts: i64 = db(root).query_row("SELECT COUNT(*) FROM command_receipts WHERE command_id = ?1", [refused_key.to_string()], |r| r.get(0)).unwrap();
    assert_eq!(receipts, 0, "{what}: a refusal leaves nothing");

    let key = CommandId::new();
    let receipt = client.execute(CommandEnvelope { command_id: key, expected_version: None, command: make(true) }).await.unwrap_or_else(|e| panic!("{what}: {e:?}"));
    assert!(!serde_json::to_string(&receipt).unwrap().contains(&s.value), "{what}: not in the receipt");
    let (hash, result): (String, String) = db(root).query_row("SELECT request_hash, result_json FROM command_receipts WHERE command_id = ?1", [key.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert!(!result.contains(&s.value), "{what}: not in the stored receipt");
    let sent = serde_json::to_string(&make(true)).unwrap();
    assert!(sent.contains(&s.value));
    let redacted: Command = serde_json::from_str(&sent.replace(&s.value, &s.marker())).unwrap();
    assert_eq!(hash, aurelius_application::request_hash(&CommandEnvelope { command_id: key, expected_version: None, command: redacted }), "{what}: the hash is the redacted command's");
    assert_ne!(hash, aurelius_application::request_hash(&CommandEnvelope { command_id: key, expected_version: None, command: make(true) }), "{what}");
    let kept_at = kept(root, &s.marker());
    assert!(kept_at.iter().any(|t| !t.starts_with("command_receipts.")), "{what}: stored with the marker: {kept_at:?}");
    assert_eq!(kept(root, &s.value), Vec::<String>::new(), "{what}: the value is kept nowhere");
    receipt
}

/// After the daemon stopped: no byte of any value in the database files or the blobs.
fn nothing_left(root: &Path, secrets: &[Secret]) {
    let mut bytes = Vec::new();
    for suffix in ["", "-wal", "-shm"] {
        bytes.extend(std::fs::read(root.join(format!("data/aurelius.sqlite{suffix}"))).unwrap_or_default());
    }
    let blobs = walk(&root.join("blobs"));
    for s in secrets {
        let holds = |h: &[u8]| h.windows(s.value.len()).any(|w| w == s.value.as_bytes());
        assert!(!holds(&bytes) && !blobs.iter().any(|b| holds(b)), "{} is left somewhere", s.name);
    }
}

async fn setup(client: &Client) -> (EmployeeId, ProjectId) {
    let create = CreateEmployee { name: "Builder".into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    let project = CreateProject { name: "Atlas".into(), description: None, folder: None };
    let CommandResult::ProjectCreated { project_id, .. } = client.execute(aurelius_client::envelope(Command::CreateProject(project), None)).await.unwrap().result else { panic!() };
    (employee_id, project_id)
}

fn new_ticket(project_id: ProjectId, owner: Option<EmployeeId>, description: String, mode: AgentMode, start: bool, confirm_secret: bool) -> CreateTicket {
    CreateTicket {
        responsible_user_id: None,
        confirm_secret,
        project_id,
        title: "Deploy".into(),
        description: Some(description),
        priority: TicketPriority::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: owner,
        run: RunSettings { mode: Some(mode), ..Default::default() },
        files: vec![],
        start,
        from_message: None,
        from_idea: None,
    }
}

async fn until(client: &Client, id: TicketId, column: ColumnKind, status: TaskStatus) -> TicketDetail {
    for _ in 0..400 {
        let detail = client.ticket(id).await.unwrap();
        if detail.ticket.column == column && detail.root_task.as_ref().map(|t| t.status) == Some(status) {
            return detail;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("never {column:?}/{status:?}");
}

async fn live_run(root: &Path, conversation: ConversationId) -> RunId {
    for _ in 0..400 {
        let found: Option<String> = db(root).query_row("SELECT id FROM runs WHERE conversation_id = ?1 AND status = 'running'", [conversation.to_string()], |r| r.get(0)).ok();
        if let Some(run) = found {
            return RunId::parse(&run).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no live run in {conversation}");
}

/// Messages, tasks, schedules, ideas and a ticket's texts, review and acceptance.
#[tokio::test(flavor = "multi_thread")]
async fn every_text_command_is_refused_by_name_and_stored_redacted_when_confirmed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let daemon = start(root).await;
    let client = connect(root).await;
    let (builder, project_id) = setup(&client).await;
    let mut secrets = Vec::new();
    for n in 0..11 {
        secrets.push(secret(&client, 1, n).await);
    }
    let [chat, project_chat, task, schedule, schedule_edit, idea, idea_edit, ticket, ticket_edit, send_back, accept] = &secrets[..] else { unreachable!() };

    let conversation_id = conversation(&client).await;
    b10(root, &client, chat, "send_chat_message", |confirm_secret| {
        Command::SendChatMessage(SendChatMessage { conversation_id, text: format!("use {}", chat.value), images: vec![], interrupt: false, confirm_secret })
    })
    .await;

    b10(root, &client, project_chat, "send_project_message", |confirm_secret| {
        Command::SendProjectMessage(SendProjectMessage {
            project_id,
            text: format!("use {}", project_chat.value),
            conversation_id: None,
            images: vec![],
            files: vec![],
            interrupt: false,
            start: Default::default(),
            confirm_secret,
        })
    })
    .await;

    b10(root, &client, task, "create_task", |confirm_secret| {
        Command::CreateTask(CreateTask {
            project_id,
            employee_id: Some(builder),
            title: "Rotate the key".into(),
            goal: format!("Rotate {}", task.value),
            acceptance_criteria: vec![format!("{} no longer works", task.value)],
            depends_on: vec![],
            run: RunSettings::default(),
            files: vec![],
            confirm_secret,
        })
    })
    .await;

    let plan = |goal: String, confirm_secret| CreateTaskSchedule {
        project_id,
        employee_id: builder,
        title: "Nightly".into(),
        goal,
        acceptance_criteria: vec![],
        files: vec![],
        run: RunSettings::default(),
        recurrence: Recurrence::Daily { at_minute: 120 },
        paused: true,
        confirm_secret,
    };
    let made = b10(root, &client, schedule, "create_task_schedule", |c| Command::CreateTaskSchedule(plan(format!("Check {}", schedule.value), c))).await;
    let CommandResult::TaskScheduleCreated { schedule_id } = made.result else { panic!("{:?}", made.result) };
    b10(root, &client, schedule_edit, "update_task_schedule", |confirm_secret| {
        Command::UpdateTaskSchedule(UpdateTaskSchedule {
            schedule_id,
            employee_id: builder,
            title: "Nightly".into(),
            goal: format!("Check {}", schedule_edit.value),
            acceptance_criteria: vec![],
            add_files: vec![],
            remove_files: vec![],
            run: RunSettings::default(),
            recurrence: Recurrence::Daily { at_minute: 180 },
            confirm_secret,
        })
    })
    .await;

    let made = b10(root, &client, idea, "add_idea", |confirm_secret| {
        Command::AddIdea(AddIdea { project_id, title: "Key rotation".into(), note: Some(format!("now {}", idea.value)), kind: Default::default(), files: vec![], confirm_secret })
    })
    .await;
    let CommandResult::IdeaAdded { idea_id } = made.result else { panic!("{:?}", made.result) };
    b10(root, &client, idea_edit, "update_idea", |confirm_secret| {
        Command::UpdateIdea(UpdateIdea { idea_id, title: "Key rotation".into(), note: Some(format!("now {}", idea_edit.value)), kind: None, add_files: vec![], remove_files: vec![], confirm_secret })
    })
    .await;

    let made = b10(root, &client, ticket, "create_ticket", |c| Command::CreateTicket(new_ticket(project_id, None, format!("Deploy with {}", ticket.value), AgentMode::Auto, false, c))).await;
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!("{:?}", made.result) };
    b10(root, &client, ticket_edit, "update_ticket", |confirm_secret| {
        Command::UpdateTicket(UpdateTicket {
            ticket_id,
            title: None,
            description: Some(format!("Deploy with {}", ticket_edit.value)),
            priority: None,
            labels: None,
            deadline_ms: None,
            clear_deadline: false,
            run: None,
            add_files: None,
            confirm_secret,
        })
    })
    .await;

    // A worked ticket in Review: sent back, then accepted.
    let made = client.create_ticket(new_ticket(project_id, Some(builder), "Write the notes.".into(), AgentMode::Auto, true, false)).await.unwrap();
    let CommandResult::TicketCreated { ticket_id: worked, .. } = made.result else { panic!() };
    until(&client, worked, ColumnKind::Review, TaskStatus::Review).await;
    b10(root, &client, send_back, "send_back_ticket", |confirm_secret| {
        Command::SendBackTicket(SendBackTicket { ticket_id: worked, feedback: format!("Leave out {}", send_back.value), confirm_secret })
    })
    .await;
    until(&client, worked, ColumnKind::Review, TaskStatus::Review).await;
    b10(root, &client, accept, "accept_ticket", |confirm_secret| Command::AcceptTicket(AcceptTicket { ticket_id: worked, note: Some(format!("Fine, {}", accept.value)), confirm_secret })).await;
    until(&client, worked, ColumnKind::Done, TaskStatus::Succeeded).await;

    drop(client);
    stop(root, daemon).await;
    nothing_left(root, &secrets);
}

/// A ticket's plan sent back for changes (RevisePlan).
#[tokio::test(flavor = "multi_thread")]
async fn a_revised_tickets_plan_is_refused_by_name_and_stored_redacted_when_confirmed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // Slow enough to hand a plan in while the run goes.
    let daemon = start_with_delay(root, 40).await;
    let client = connect(root).await;
    let (builder, project_id) = setup(&client).await;
    let revise = secret(&client, 2, 0).await;

    let made = client.create_ticket(new_ticket(project_id, Some(builder), "Export the report.".into(), AgentMode::Plan, true, false)).await.unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!() };
    let doing = until(&client, ticket_id, ColumnKind::Doing, TaskStatus::Running).await;
    let conversation = doing.root_task.unwrap().conversation_id.unwrap();
    let run = live_run(root, conversation).await;
    let bridge = BridgeClient::connect(root).await.unwrap();
    bridge.permission_ask(run, Some("plan-1".into()), "ExitPlanMode".into(), serde_json::json!({ "plan": "# Plan\n\n1. Export" })).await.unwrap();
    until(&client, ticket_id, ColumnKind::PlanReview, TaskStatus::Waiting).await;
    b10(root, &client, &revise, "revise_plan", |confirm_secret| Command::RevisePlan(RevisePlan { ticket_id, feedback: format!("Sign with {}", revise.value), confirm_secret })).await;

    drop(client);
    stop(root, daemon).await;
    nothing_left(root, &[revise]);
}

/// A plan and a question in the inbox: DecidePlan's feedback, AnswerQuestion's answers.
#[tokio::test(flavor = "multi_thread")]
async fn inbox_answers_are_refused_by_name_and_stored_redacted_when_confirmed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let daemon = start_with_delay(root, 400).await;
    let client = connect(root).await;
    let decide = secret(&client, 3, 0).await;
    let answer = secret(&client, 3, 1).await;
    let conversation_id = conversation(&client).await;
    let sent = client.execute(aurelius_client::envelope(Command::SendChatMessage(SendChatMessage { conversation_id, text: "keep going".into(), images: vec![], interrupt: false, confirm_secret: false }), None)).await.unwrap();
    let CommandResult::ChatMessageSent { .. } = sent.result else { panic!("{:?}", sent.result) };
    let run = live_run(root, conversation_id).await;
    let pending = |tool: &'static str| {
        let client = &client;
        async move {
            for _ in 0..200 {
                if let Some(a) = client.approvals().await.unwrap().approvals.into_iter().find(|a| a.status == ApprovalStatus::Pending && a.tool == tool) {
                    return a.id;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            panic!("{tool} never reached the inbox");
        }
    };

    let bridge = BridgeClient::connect(root).await.unwrap();
    let waiting = tokio::spawn(async move { bridge.permission_ask(run, Some("plan-1".into()), "ExitPlanMode".into(), serde_json::json!({ "plan": "# Plan\n\n1. Deploy" })).await.unwrap() });
    let approval_id = pending("claude.ExitPlanMode").await;
    b10(root, &client, &decide, "decide_plan", |confirm_secret| {
        Command::DecidePlan(DecidePlan { approval_id, decision: PlanDecision::Revise, feedback: Some(format!("Deploy with {}", decide.value)), confirm_secret })
    })
    .await;
    let told = waiting.await.unwrap().message.unwrap();
    assert!(told.contains(&decide.marker()) && !told.contains(&decide.value), "{told}");

    let bridge = BridgeClient::connect(root).await.unwrap();
    let question = serde_json::json!({ "questions": [{ "question": "Which token?", "header": "Token", "multiSelect": false,
        "options": [{ "label": "Staging", "description": "" }, { "label": "Production", "description": "" }] }] });
    let waiting = tokio::spawn(async move { bridge.permission_ask(run, Some("q-1".into()), "AskUserQuestion".into(), question).await.unwrap() });
    let approval_id = pending("claude.AskUserQuestion").await;
    b10(root, &client, &answer, "answer_question", |confirm_secret| {
        Command::AnswerQuestion(AnswerQuestion { approval_id, answers: [("Which token?".to_owned(), format!("This one: {}", answer.value))].into_iter().collect(), confirm_secret })
    })
    .await;
    let told = serde_json::to_string(&waiting.await.unwrap().updated_input).unwrap();
    assert!(!told.contains(&answer.value), "{told}");

    drop(client);
    stop(root, daemon).await;
    nothing_left(root, &[decide, answer]);
}
