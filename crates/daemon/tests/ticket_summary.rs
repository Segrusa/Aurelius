//! A ticket's discussion follows its work (MU2-03) and its summary after Done
//! (MU2-12, B4): every step leaves one system line that wakes nobody; Done gives
//! one extraction over the whole ticket (its fields, discussion and the reports
//! of its tasks) that writes exactly one project-memory item, with no extraction
//! of its own for a task below it; a failed summary is visible and a board member
//! tries again without a second item. Runs use the fake runtime.

mod common;

use aurelius_client::{Client, bridge::BridgeClient};
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::TaskStatus;
use aurelius_domain::ticket::*;
use aurelius_domain::views::{MessageView, SystemLineKind};
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, ErrorCode, ProjectId, RunId, TicketId};
use aurelius_protocol::EmployeeTaskOp;
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The fake runtime: holds a run whose request says HOLD-MAIN (or HOLD-PART)
/// until that gate opens, fails memory extractions while `fail_memory` is set,
/// and keeps every extraction's prompt.
struct Scripted {
    main: tokio::sync::watch::Sender<bool>,
    part: tokio::sync::watch::Sender<bool>,
    fail_memory: AtomicBool,
    prompts: Mutex<Vec<String>>,
}

impl Scripted {
    fn new() -> Arc<Self> {
        Arc::new(Self { main: tokio::sync::watch::channel(false).0, part: tokio::sync::watch::channel(false).0, fail_memory: AtomicBool::new(false), prompts: Mutex::default() })
    }
}

#[async_trait::async_trait]
impl AgentRuntime for Scripted {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let normal = fake::FakeRuntime { delay: Duration::from_millis(1), behavior: fake::FakeBehavior::Normal };
        if request.system.as_deref().is_some_and(|s| s.contains(fake::MEMORY_MARKER)) {
            let system = request.system.clone().unwrap_or_default();
            self.prompts.lock().unwrap().push(std::iter::once(system).chain(request.turns.iter().map(|t| t.text.clone())).collect::<Vec<_>>().join("\n"));
            if self.fail_memory.load(Ordering::SeqCst) {
                return fake::FakeRuntime { behavior: fake::FakeBehavior::FailAfterDeltas, ..normal }.start(request).await;
            }
            return normal.start(request).await;
        }
        let said = |word: &str| request.turns.iter().any(|t| t.text.contains(word));
        let gate = if said("HOLD-PART") { Some(self.part.subscribe()) } else if said("HOLD-MAIN") { Some(self.main.subscribe()) } else { None };
        let Some(mut gate) = gate.filter(|g| !*g.borrow()) else { return normal.start(request).await };
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let (input_tx, _input_rx) = tokio::sync::mpsc::channel(8);
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            while !*gate.borrow() {
                tokio::select! {
                    changed = gate.changed() => if changed.is_err() { break },
                    _ = cancel_rx.changed() => {
                        let _ = tx.send(RuntimeEvent::Interrupted).await;
                        return;
                    }
                }
            }
            let _ = tx.send(RuntimeEvent::TextDelta("Report: the export works; a long paragraph follows that the line in the discussion leaves out. ".repeat(4))).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

async fn project(client: &Client, name: &str) -> ProjectId {
    let created = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = created.result else { panic!("{:?}", created.result) };
    project_id
}

async fn employee(client: &Client, name: &str, boss: Option<EmployeeId>) -> EmployeeId {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Engineer".into(), title: None, reports_to: boss, instructions: None, folder: None };
    let created = client.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap();
    let CommandResult::EmployeeCreated { employee_id, .. } = created.result else { panic!("{:?}", created.result) };
    employee_id
}

async fn ticket(client: &Client, project_id: ProjectId, title: &str, description: &str, owner: EmployeeId) -> TicketId {
    let made = client
        .create_ticket(CreateTicket {
            responsible_user_id: None,
            confirm_secret: false,
            project_id,
            title: title.into(),
            description: Some(description.into()),
            priority: TicketPriority::default(),
            labels: vec!["export".into()],
            deadline_ms: None,
            owner_employee_id: Some(owner),
            run: aurelius_domain::task::RunSettings { mode: Some(aurelius_domain::autonomy::AgentMode::Auto), ..Default::default() },
            files: vec![],
            start: true,
            from_message: None,
            from_idea: None,
            kind: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!("{:?}", made.result) };
    ticket_id
}

async fn until(client: &Client, id: TicketId, column: ColumnKind, status: TaskStatus) -> TicketDetail {
    for _ in 0..800 {
        let detail = client.ticket(id).await.unwrap();
        if detail.ticket.column == column && detail.root_task.as_ref().map(|t| t.status) == Some(status) {
            return detail;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let detail = client.ticket(id).await.unwrap();
    panic!("never {column:?}/{status:?}: {:?}/{:?}", detail.ticket.column, detail.root_task.map(|t| t.status));
}

async fn live_run(root: &std::path::Path, conversation: ConversationId) -> RunId {
    for _ in 0..800 {
        let found: Option<String> = common::db(root).query_row("SELECT id FROM runs WHERE conversation_id = ?1 AND status = 'running'", [conversation.to_string()], |r| r.get(0)).ok();
        if let Some(run) = found {
            return RunId::parse(&run).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no live run in {conversation}");
}

/// Until nothing runs in `conversation` any more.
async fn quiet(root: &std::path::Path, conversation: ConversationId) {
    for _ in 0..800 {
        let running: bool = common::db(root)
            .query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE conversation_id = ?1 AND status IN ('created','starting','running','waiting','cancelling')) OR EXISTS(SELECT 1 FROM employee_inbox WHERE conversation_id = ?1 AND state = 'queued')", [conversation.to_string()], |r| r.get(0))
            .unwrap();
        if !running {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{conversation} keeps running");
}

fn extractions_ended(root: &std::path::Path) -> bool {
    common::db(root).query_row("SELECT NOT EXISTS(SELECT 1 FROM jobs WHERE kind = 'memory.extract' AND state IN ('ready','leased'))", [], |r| r.get(0)).unwrap()
}

/// The extraction jobs of `task`, whatever their state.
fn extractions_of(root: &std::path::Path, task: aurelius_domain::TaskId) -> usize {
    common::db(root).query_row("SELECT COUNT(*) FROM jobs WHERE kind = 'memory.extract' AND json_extract(payload_json, '$.task_id') = ?1", [task.to_string()], |r| r.get::<_, i64>(0)).unwrap() as usize
}

async fn ticket_summaries(client: &Client, project_id: ProjectId) -> Vec<aurelius_protocol::MemoryView> {
    client.memory_of(Some(project_id), None).await.unwrap().items.into_iter().filter(|m| m.content.starts_with("Ticket #")).collect()
}

async fn discussion(client: &Client, id: TicketId) -> Vec<MessageView> {
    let conversation = client.ticket(id).await.unwrap().ticket.conversation_id.expect("a discussion");
    client.messages(conversation).await.unwrap().messages
}

fn lines(messages: &[MessageView]) -> Vec<(SystemLineKind, String)> {
    messages.iter().filter_map(|m| m.system.as_ref().map(|s| (s.kind, m.text.clone()))).collect()
}

/// Until the discussion has a system line of `kind`.
async fn line_of(client: &Client, id: TicketId, kind: SystemLineKind) -> MessageView {
    for _ in 0..800 {
        if let Some(m) = discussion(client, id).await.into_iter().rev().find(|m| m.system.as_ref().is_some_and(|s| s.kind == kind)) {
            return m;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no {kind:?} line: {:#?}", lines(&discussion(client, id).await));
}

/// One History page (feature `ticket_history`, ticket #57), narrowed by `query`.
async fn history(client: &Client, query: Option<&str>) -> Vec<aurelius_domain::history::HistoryEntry> {
    let method = aurelius_protocol::Method::TicketHistory { project_id: None, employee_id: None, from_ms: None, to_ms: None, query: query.map(Into::into), before_done_at_ms: None, before_number: None, limit: None };
    client.history(method).await.unwrap().entries
}

/// The `ticket.updated { changed: ["summary"] }` events of a ticket (live updates of the History).
fn summary_events(root: &std::path::Path, id: TicketId) -> usize {
    common::db(root)
        .query_row("SELECT COUNT(*) FROM events WHERE aggregate_type = 'ticket' AND aggregate_id = ?1 AND type = 'ticket.updated' AND json_extract(metadata_json, '$.changed[0]') = 'summary'", [id.to_string()], |r| r.get::<_, i64>(0))
        .unwrap() as usize
}

/// The summary kept on the ticket, as stored.
fn stored_summary(root: &std::path::Path, id: TicketId) -> (Option<String>, Option<String>) {
    common::db(root).query_row("SELECT summary_state, summary_text FROM tickets WHERE id = ?1", [id.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
}

fn code<T: std::fmt::Debug>(result: Result<T, aurelius_client::ClientError>) -> ErrorCode {
    match result {
        Err(aurelius_client::ClientError::Protocol(err)) => err.code,
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

/// Runs a discussion woke: none of them may come from a system line.
fn group_runs(root: &std::path::Path, discussion: ConversationId) -> usize {
    common::db(root)
        .query_row("SELECT COUNT(*) FROM jobs WHERE kind = 'run.chat' AND json_extract(payload_json, '$.group.conversation_id') = ?1", [discussion.to_string()], |r| r.get::<_, i64>(0))
        .unwrap() as usize
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tickets_work_leaves_system_lines_and_done_one_summary_over_the_whole_ticket() {
    let dir = tempfile::tempdir().unwrap();
    let rt = Scripted::new();
    let daemon = common::start_with(dir.path(), rt.clone()).await;
    let client = common::connect_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder", None).await;
    employee(&client, "Helper", Some(builder)).await;
    let atlas = project(&client, "Atlas").await;

    // Started with an owner; the main task gives a part to a colleague during its run.
    let id = ticket(&client, atlas, "CSV export", "HOLD-MAIN Export the monthly report as CSV.", builder).await;
    let doing = until(&client, id, ColumnKind::Doing, TaskStatus::Running).await;
    let main = doing.root_task.clone().unwrap();
    let run = live_run(dir.path(), main.conversation_id.unwrap()).await;
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let (ok, text) = bridge
        .employee_act(run, EmployeeTaskOp::Delegate { employee: "Helper".into(), title: "Column names".into(), goal: "HOLD-PART choose the column names".into(), criteria: vec![], project: None, ticket: None })
        .await
        .unwrap();
    assert!(ok, "{text}");
    let part = client.ticket(id).await.unwrap().tasks.into_iter().find(|t| t.parent_task_id == Some(main.id)).expect("the part");
    // The part reports (it succeeds at once, below the ticket), then the main task reports: Review.
    rt.part.send_replace(true);
    for _ in 0..800 {
        if client.ticket(id).await.unwrap().tasks.iter().any(|t| t.id == part.id && t.status == TaskStatus::Succeeded) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    rt.main.send_replace(true);
    until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    quiet(dir.path(), main.conversation_id.unwrap()).await;
    let detail = until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    // MU2-12 AC3: a task below a ticket gets no extraction of its own.
    assert_eq!(extractions_of(dir.path(), part.id), 0, "no extraction for the part");
    assert_eq!(extractions_of(dir.path(), main.id), 0, "none for the main task before Done");

    // A board member's word in Review goes to the owner (AUR-BUG-Q): a run of the
    // main task answers it and reports again, the ticket stays in Review; then Done.
    let runs = |root: &std::path::Path| -> i64 {
        common::db(root).query_row("SELECT (SELECT COUNT(*) FROM runs WHERE conversation_id = ?1) + (SELECT COUNT(*) FROM employee_inbox WHERE conversation_id = ?1 AND state = 'queued')", [main.conversation_id.unwrap().to_string()], |r| r.get(0)).unwrap()
    };
    let before = runs(dir.path());
    let posted = client.ticket_command(Command::PostTicketMessage(PostTicketMessage { confirm_secret: false, ticket_id: id, text: "Keep the old file name.".into(), images: vec![], files: vec![], thread_parent: None }), None).await;
    assert!(posted.is_ok(), "{posted:?}");
    // The post put the owner's run in its mailbox in the same commit.
    assert_eq!(runs(dir.path()), before + 1, "a run of the main task answers it");
    quiet(dir.path(), main.conversation_id.unwrap()).await;
    until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    // B1: in Review it is not done yet.
    assert!(history(&client, None).await.is_empty(), "nothing finished before Done");
    client.ticket_command(Command::AcceptTicket(AcceptTicket { confirm_secret: false, ticket_id: id, note: None }), None).await.unwrap();
    let done = until(&client, id, ColumnKind::Done, TaskStatus::Succeeded).await;
    let summary = line_of(&client, id, SystemLineKind::Summary).await;
    for _ in 0..800 {
        if extractions_ended(dir.path()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    // MU2-12 AC1: exactly one item, naming the ticket, with the ticket as locator; one extraction.
    let summaries = ticket_summaries(&client, atlas).await;
    assert_eq!(summaries.len(), 1, "{summaries:#?}");
    assert!(summaries[0].content.starts_with(&format!("Ticket #{} “CSV export” is done", done.ticket.number)), "{}", summaries[0].content);
    let locator: String = common::db(dir.path())
        .query_row("SELECT json_extract(locator_json, '$.ticket_id') FROM memory_items WHERE json_extract(locator_json, '$.ticket_id') IS NOT NULL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(locator, id.to_string());
    assert_eq!(extractions_of(dir.path(), main.id), 1, "one extraction for the ticket");
    assert_eq!(extractions_of(dir.path(), part.id), 0, "still none for the part");
    // H-01 (ticket #57): the same text on the ticket, without its head; the History shows it.
    let rows = history(&client, None).await;
    assert_eq!(rows.len(), 1, "{rows:#?}");
    let row = &rows[0];
    assert_eq!((row.ticket_id, row.number, row.title.as_str(), row.project_name.as_str()), (id, done.ticket.number, "CSV export", "Atlas"));
    assert_eq!((row.owner_employee_id, row.owner_name.as_deref()), (Some(builder), Some("Builder")));
    assert_eq!(row.done_at_ms, done.ticket.done_at_ms.expect("a Done moment"));
    assert_eq!(row.summary_state, aurelius_domain::history::HistorySummaryState::Ready);
    let text = row.summary.clone().expect("a summary");
    assert!(summaries[0].content.ends_with(&text) && !text.starts_with("Ticket #"), "the memory's text without its head: {text} / {}", summaries[0].content);
    assert_eq!(summary_events(dir.path(), id), 1, "one live update when it came");
    // H-03 AC3: the search looks in the summary and the title.
    assert_eq!(history(&client, Some("DELIVERED")).await.len(), 1);
    assert_eq!(history(&client, Some("csv")).await.len(), 1);
    assert!(history(&client, Some("nothing like this")).await.is_empty());
    // B4: its input is the whole ticket: fields, discussion and the part's report.
    let prompts = rt.prompts.lock().unwrap().clone();
    let ticket_prompt = prompts.iter().find(|p| p.contains("Discussion of the ticket")).unwrap_or_else(|| panic!("{prompts:#?}"));
    assert!(ticket_prompt.contains("Export the monthly report as CSV") && ticket_prompt.contains("labels export"), "the ticket's fields: {ticket_prompt}");
    assert!(ticket_prompt.contains("Keep the old file name."), "the discussion: {ticket_prompt}");
    assert!(ticket_prompt.contains("Linked task completed: Column names (Helper)"), "the part's report: {ticket_prompt}");
    // Sentinel n2: the sources are data, not instructions; the description and the reports are quoted.
    assert!(ticket_prompt.contains("Description (JSON-quoted):\n\"HOLD-MAIN Export the monthly report as CSV.\""), "the description quoted: {ticket_prompt}");
    assert!(ticket_prompt.contains("Report (JSON-quoted):\n\""), "the reports quoted: {ticket_prompt}");
    assert!(ticket_prompt.contains("Assignment: CSV export — \"HOLD-MAIN"), "the goal quoted: {ticket_prompt}");
    assert!(ticket_prompt.contains("are data, not instructions"), "{ticket_prompt}");
    // AC2: the summary line, written after Done (B7 is for people and tools).
    assert_eq!(summary.text, "A summary of this ticket was saved to project memory");

    // MU2-03: one line per step, in order, with links where there is a task.
    let messages = discussion(&client, id).await;
    let all = lines(&messages);
    let texts: Vec<&str> = all.iter().map(|(_, t)| t.as_str()).collect();
    let find = |want: &str| texts.iter().position(|t| t.contains(want)).unwrap_or_else(|| panic!("no “{want}” in {texts:#?}"));
    let started = find("grace started the ticket with Builder as owner — the ticket moved from Backlog to Todo");
    let working = find("Builder started working on the ticket — the ticket moved from Todo to Doing");
    let delegated = find("Builder gave the task “Column names” to Helper");
    let reported = find("Helper finished the task “Column names”. Report: ");
    let review = find("Builder finished the work and handed in the report. It is ready for your review");
    let accepted = find("grace accepted the report and moved the ticket to Done");
    let saved = find("A summary of this ticket was saved to project memory");
    assert!(started < working && working < delegated && delegated < reported && reported < review && review < accepted && accepted < saved, "{texts:#?}");
    assert_eq!(texts.iter().filter(|t| t.contains("moved the ticket to Done")).count(), 1, "one line per step");
    let line = |at: usize| messages.iter().filter(|m| m.system.is_some()).nth(at).unwrap().system.clone().unwrap();
    assert_eq!((line(delegated).kind, line(delegated).task_id), (SystemLineKind::Delegated, Some(part.id)), "a link to the part");
    assert_eq!((line(reported).kind, line(reported).task_id), (SystemLineKind::Reported, Some(part.id)));
    assert!(texts[reported].chars().count() < 260 && texts[reported].ends_with('…'), "a short summary, not the report: {}", texts[reported]);
    assert_eq!((line(review).kind, line(review).task_id), (SystemLineKind::Review, Some(detail.root_task.unwrap().id)));
    // Never waking: no run of an employee came from the discussion (the board member's word in Review woke nobody either).
    assert_eq!(group_runs(dir.path(), done.ticket.conversation_id.unwrap()), 0);
    // B7: closed for people, also for a retry of the summary that is there already.
    let closed = client.ticket_command(Command::PostTicketMessage(PostTicketMessage { confirm_secret: false, ticket_id: id, text: "One more thing".into(), images: vec![], files: vec![], thread_parent: None }), None).await;
    assert_eq!(code(closed), ErrorCode::Validation);
    assert_eq!(code(client.ticket_command(Command::RetryTicketSummary(RetryTicketSummary { ticket_id: id }), None).await), ErrorCode::Validation);
    assert_eq!(ticket_summaries(&client, atlas).await.len(), 1);

    drop(client);
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_summary_is_visible_and_a_retry_writes_it_once() {
    let dir = tempfile::tempdir().unwrap();
    let rt = Scripted::new();
    rt.main.send_replace(true);
    rt.fail_memory.store(true, Ordering::SeqCst);
    let daemon = common::start_with(dir.path(), rt.clone()).await;
    let client = common::connect_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder", None).await;
    let atlas = project(&client, "Atlas").await;
    let id = ticket(&client, atlas, "Login page", "A login page.", builder).await;
    until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    // Not on Done yet: nothing to try again.
    assert_eq!(code(client.ticket_command(Command::RetryTicketSummary(RetryTicketSummary { ticket_id: id }), None).await), ErrorCode::Validation);

    // AC4: the extraction fails (as on a usage limit): the ticket stays on Done, the discussion says so.
    client.ticket_command(Command::AcceptTicket(AcceptTicket { confirm_secret: false, ticket_id: id, note: None }), None).await.unwrap();
    let failed = line_of(&client, id, SystemLineKind::SummaryFailed).await;
    // Pulse m3: neutral words, never the runtime's error (which may name a path on the server).
    assert_eq!(failed.text, aurelius_application::memory::SUMMARY_FAILED);
    assert!(!failed.text.contains('/') && !failed.text.contains("error 2"), "{}", failed.text);
    assert!(extractions_ended(dir.path()), "it does not retry by itself");
    assert_eq!(client.ticket(id).await.unwrap().ticket.column, ColumnKind::Done);
    assert!(ticket_summaries(&client, atlas).await.is_empty());
    // H-01 AC5 (ticket #57): the History says so, never an endless "Summarizing…".
    let rows = history(&client, None).await;
    assert_eq!((rows.len(), rows[0].summary_state, rows[0].summary.clone()), (1, aurelius_domain::history::HistorySummaryState::NotAvailable, None));
    assert_eq!(stored_summary(dir.path(), id), (Some("failed".into()), None));
    assert_eq!(summary_events(dir.path(), id), 1, "a live update for the failure");

    // A board member tries again: one item, a summary line after the failure.
    // H-01 AC4: a vault value in the summary is kept as its marker, not as itself.
    let owner = dir.path().to_string_lossy().into_owned();
    aurelius_runtime::redact::add_to_set(&owner, aurelius_runtime::redact::Secret { name: "HISTORY_TITLE".into(), label: String::new(), value: zeroize::Zeroizing::new("Login page".into()) });
    rt.fail_memory.store(false, Ordering::SeqCst);
    client.ticket_command(Command::RetryTicketSummary(RetryTicketSummary { ticket_id: id }), None).await.unwrap();
    assert_eq!(summary_events(dir.path(), id), 2, "a live update: it is made again");
    // A second retry is always refused: while the first is made (pending) or once it is
    // there (the summary and the job's end are one transaction), never a second job.
    let twice = client.ticket_command(Command::RetryTicketSummary(RetryTicketSummary { ticket_id: id }), None).await;
    assert_eq!(code(twice), ErrorCode::Validation);
    line_of(&client, id, SystemLineKind::Summary).await;
    for _ in 0..800 {
        if extractions_ended(dir.path()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(ticket_summaries(&client, atlas).await.len(), 1, "exactly one");
    // Idempotent: once it is there, a retry adds nothing.
    assert_eq!(code(client.ticket_command(Command::RetryTicketSummary(RetryTicketSummary { ticket_id: id }), None).await), ErrorCode::Validation);
    assert_eq!(ticket_summaries(&client, atlas).await.len(), 1, "still exactly one");
    let kinds: Vec<SystemLineKind> = lines(&discussion(&client, id).await).into_iter().map(|(k, _)| k).filter(|k| matches!(k, SystemLineKind::Summary | SystemLineKind::SummaryFailed)).collect();
    assert_eq!(kinds, [SystemLineKind::SummaryFailed, SystemLineKind::Summary], "the newest line decides");
    // The retry's text is on the ticket, redacted when it was kept.
    let (state, text) = stored_summary(dir.path(), id);
    let text = text.expect("the retry's summary");
    assert_eq!(state.as_deref(), Some("ready"));
    assert!(text.contains(&aurelius_runtime::redact::marker("HISTORY_TITLE")) && !text.contains("Login page"), "{text}");
    let rows = history(&client, None).await;
    assert_eq!((rows[0].summary_state, rows[0].summary.as_deref()), (aurelius_domain::history::HistorySummaryState::Ready, Some(text.as_str())));
    aurelius_runtime::redact::install_set(&owner, Vec::new());

    drop(client);
    common::stop(dir.path(), daemon).await;
}
