//! `propose_guideline` (B5): SEC0 T13, T19–T22 and E11–E15 at the application
//! layer, and archiving an employee withdraws its open proposals. T15 (a vault
//! value is redacted) and text-only runs are the daemon's: `daemon/tests`.

use super::*;
use crate::task::task_runs::EmployeeAction;
use crate::*;
use aurelius_domain::{
    ConversationId, EmployeeId, MessageId, ProjectId, RunId, TaskId,
    autonomy::{AgentMode, SetAgentMode},
    employee::{CreateEmployee, LifecycleStatus, SetEmployeeStatus},
    guideline::{EffectiveGuideline, GuidelineLevel, GuidelineProposalView, GuidelineSource},
    project::CreateProject,
    project_conversation::SendProjectMessage,
    proposal::ProposalStatus,
    task::{CreateTask, TaskStatus},
};
use aurelius_storage::Store;

// ── Helpers ─────────────────────────────────────────────────────────────

fn ctx() -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app: None, user_id: None }
}

fn setup() -> (tempfile::TempDir, CommandService) {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    (dir, service)
}

fn run(s: &CommandService, command: Command) -> Result<Committed, AppError> {
    s.execute(&ctx(), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command })
}

fn project(s: &CommandService, name: &str) -> ProjectId {
    match run(s, Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None })).unwrap().receipt.result {
        CommandResult::ProjectCreated { project_id, .. } => project_id,
        other => panic!("unexpected {other:?}"),
    }
}

fn hire(s: &CommandService, name: &str) -> (EmployeeId, ConversationId) {
    let create = CreateEmployee { name: name.into(), role: "Dev".into(), title: None, reports_to: None, instructions: Some(format!("Focus of {name}.")), folder: None };
    match run(s, Command::CreateEmployee(create)).unwrap().receipt.result {
        CommandResult::EmployeeCreated { employee_id, conversation_id } => (employee_id, conversation_id),
        other => panic!("unexpected {other:?}"),
    }
}

/// A queued run in a conversation (what a bridge call belongs to).
fn chat_run(s: &CommandService, conversation: ConversationId) -> RunId {
    s.store().write(move |tx| Ok(crate::run::queue::queue_run(tx, &serde_json::json!({}), "test", conversation, MessageId::new())?.0)).unwrap()
}

/// A running task of `employee` in `project`, with a run in its conversation.
fn task_run(s: &CommandService, project: ProjectId, employee: EmployeeId, title: &str) -> (TaskId, RunId) {
    let create = CreateTask { confirm_secret: false, project_id: project, employee_id: Some(employee), title: title.into(), goal: "Do it".into(), acceptance_criteria: vec![], depends_on: vec![], run: Default::default(), files: vec![] };
    let CommandResult::TaskCreated { task_id } = run(s, Command::CreateTask(create)).unwrap().receipt.result else { panic!() };
    let st = s.store().clone();
    let run_id = s
        .store()
        .write(move |tx| {
            let (conversation, brief, _) = crate::task::task_threads::open_returning(tx, &st, task_id, employee, "brief")?;
            let _ = crate::task::task_line::set_status(tx, task_id, TaskStatus::Running, None)?;
            Ok(crate::run::queue::queue_run(tx, &serde_json::json!({}), &task_id.to_string(), conversation.unwrap(), brief.unwrap())?.0)
        })
        .unwrap();
    (task_id, run_id)
}

/// A guideline in the library (version 1), as the seed makes one.
fn guideline(s: &CommandService, title: &str, text: &str) -> GuidelineId {
    let id = GuidelineId::new();
    let (title, text) = (title.to_owned(), text.to_owned());
    s.store()
        .write(move |tx| {
            let v = guidelines::NewVersion { title: &title, text: &text, source: GuidelineSource::Seed, proposal_id: None, restored_from: None, by_user: None, now_ms: 1 };
            guidelines::create(tx, id, v)
        })
        .unwrap();
    id
}

/// A new version of a guideline in the library (after a run took its snapshot).
fn new_version(s: &CommandService, id: GuidelineId, expected: u32, text: &str) {
    let text = text.to_owned();
    s.store()
        .write(move |tx| {
            let title = guidelines::get(tx, id, None)?.unwrap().title;
            let v = guidelines::NewVersion { title: &title, text: &text, source: GuidelineSource::Seed, proposal_id: None, restored_from: None, by_user: None, now_ms: 2 };
            guidelines::add_version(tx, id, expected, v)
        })
        .unwrap();
}

/// What a run got in its block (`run_guidelines`, B6): (id, version, title).
fn snapshot(s: &CommandService, run_id: RunId, block: &[(GuidelineId, u32, &str)]) {
    let set: Vec<EffectiveGuideline> =
        block.iter().map(|(id, version, title)| EffectiveGuideline { id: *id, version: *version, title: (*title).into(), text: "…".into(), level: GuidelineLevel::Employee, chars: 1 }).collect();
    s.store().write(move |tx| guidelines::insert_for_run(tx, run_id, &set)).unwrap();
}

fn input(guideline: Option<&str>, title: &str, text: &str, evidence: &[&str]) -> GuidelineProposalInput {
    GuidelineProposalInput {
        guideline: guideline.map(Into::into),
        title: title.into(),
        text: text.into(),
        reason: "Twice a review found the same thing.".into(),
        evidence: evidence.iter().map(|e| e.to_string()).collect(),
    }
}

fn propose(s: &CommandService, run_id: RunId, guideline: Option<&str>, title: &str, text: &str) -> (bool, String) {
    let (ok, text, _) = s.propose_guideline(run_id, input(guideline, title, text, &["Migration"])).unwrap();
    (ok, text)
}

fn proposals(s: &CommandService) -> Vec<GuidelineProposalView> {
    s.store().read(|c| guidelines::proposals(c, None)).unwrap()
}

fn count(s: &CommandService, sql: &'static str) -> i64 {
    s.store().read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?)).unwrap()
}

/// Ann with a task "Migration" (the evidence) in project Atlas, and her chat.
struct Fixture {
    project: ProjectId,
    ann: (EmployeeId, ConversationId),
    task_run: RunId,
}

fn fixture(s: &CommandService) -> Fixture {
    let project = project(s, "Atlas");
    let ann = hire(s, "Ann");
    let (_, task_run) = task_run(s, project, ann.0, "Migration");
    Fixture { project, ann, task_run }
}

// ── E9 / T19: the target from the run's own block ───────────────────────

#[test]
fn t19_only_a_guideline_of_the_runs_own_block_and_at_its_snapshot_version() {
    let (_d, s) = setup();
    let f = fixture(&s);
    let rust = guideline(&s, "Rust style", "Run clippy.");
    let flutter = guideline(&s, "Flutter", "Run analyze.");
    snapshot(&s, f.task_run, &[(rust, 1, "Rust style")]);
    // The library moves on after the run started.
    new_version(&s, rust, 1, "Run clippy and fmt.");

    let (ok, text) = propose(&s, f.task_run, Some(&flutter.to_string()), "Flutter", "Run analyze twice.");
    assert!(!ok && text == NOT_IN_BLOCK, "an id in the library but not in the block: {text}");
    let (ok, text) = propose(&s, f.task_run, Some("Flutter"), "Flutter", "Run analyze twice.");
    assert!(!ok && text == NOT_IN_BLOCK, "a title in the library but not in the block: {text}");
    let (ok, text) = propose(&s, f.task_run, Some(&GuidelineId::new().to_string()), "Rust style", "Run clippy.");
    assert!(!ok && text == NOT_IN_BLOCK, "{text}");
    assert!(proposals(&s).is_empty());

    let (ok, text) = propose(&s, f.task_run, Some("rust STYLE"), "Rust style", "Run clippy, fix every warning.");
    assert!(ok, "{text}");
    let p = &proposals(&s)[0];
    assert_eq!((p.guideline_id, p.base_version, p.status), (Some(rust), Some(1), ProposalStatus::Pending), "base_version is the snapshot's, not the library's 2");
    assert_eq!((p.title.as_str(), p.text.as_str()), ("Rust style", "Run clippy, fix every warning."));
    assert_eq!(p.evidence[0].label, "Migration");
    assert_eq!((p.run_id, p.origin_conversation_id.is_some()), (Some(f.task_run), true));
    assert_eq!(s.store().read(move |c| guidelines::get(c, rust, None)).unwrap().unwrap().text, "Run clippy and fmt.", "a proposal changes nothing");
}

#[test]
fn t19_a_g_id_finds_it_and_an_archived_or_ambiguous_one_is_refused() {
    let (_d, s) = setup();
    let f = fixture(&s);
    let (a, b, gone) = (guideline(&s, "Alpha", "A."), guideline(&s, "Beta", "B."), guideline(&s, "Gone", "G."));
    let twin = GuidelineId::new();
    snapshot(&s, f.task_run, &[(a, 1, "Alpha"), (b, 1, "Beta"), (gone, 1, "Gone"), (twin, 1, "beta")]);
    s.store().write(move |tx| Ok(tx.execute("UPDATE guidelines SET archived_at_ms = 5 WHERE id = ?1", [gone.to_string()])?)).unwrap();

    let (ok, text) = propose(&s, f.task_run, Some(&format!("g:{a}@1")), "Alpha", "A, better.");
    assert!(ok, "the g: id the block shows: {text}");
    let (ok, text) = propose(&s, f.task_run, Some("Beta"), "Beta", "B, better.");
    assert!(!ok && text.contains("More than one") && text.contains("g: id"), "{text}");
    let (ok, text) = propose(&s, f.task_run, Some(&format!("g:{b}")), "Beta", "B, better.");
    assert!(ok, "by id it is clear: {text}");
    let (ok, text) = propose(&s, f.task_run, Some("Gone"), "Gone", "G, better.");
    assert!(!ok && text.contains("archived"), "{text}");
    assert_eq!(proposals(&s).len(), 2);
}

// ── E10 / T20: only an employee's own runs ──────────────────────────────

#[test]
fn t20_a_trial_the_project_assistant_and_the_dispatcher_are_refused() {
    let (_d, s) = setup();
    let f = fixture(&s);
    // The project assistant (a run without an employee).
    let sent = run(&s, Command::SendProjectMessage(SendProjectMessage { confirm_secret: false, project_id: f.project, text: "hi".into(), conversation_id: None, images: vec![], files: vec![], interrupt: false, start: Default::default() })).unwrap();
    let CommandResult::ProjectMessageSent { run_id: assistant, .. } = sent.receipt.result else { panic!() };
    // The dispatcher of a ticket: an employee's run without a conversation (triage).
    let (task, ann) = (s.store().read(|c| Ok(c.query_row("SELECT id FROM tasks", [], |r| r.get::<_, String>(0))?)).unwrap(), f.ann.0);
    let triage = RunId::new();
    let triage_task = task.clone();
    s.store()
        .write(move |tx| {
            Ok(tx.execute(
                "INSERT INTO runs(id, task_id, employee_id, attempt, runtime_kind, model_id, resolved_config_json, status) VALUES (?1, ?2, ?3, 1, 'claude_code', '', '{}', 'running')",
                rusqlite::params![triage.to_string(), triage_task, ann.to_string()],
            )?)
        })
        .unwrap();
    // A variant trial: a run in a conversation of the task's variants.
    let bob = hire(&s, "Bob");
    let trial = chat_run(&s, bob.1);
    s.store()
        .write(move |tx| {
            tx.execute("INSERT INTO task_variant_sets(task_id, phase, count, created_at_ms, updated_at_ms) VALUES (?1, 'probing', 2, 1, 1)", [&task])?;
            Ok(tx.execute("INSERT INTO task_variants(task_id, n, hint, status, conversation_id, created_at_ms) VALUES (?1, 1, 'small', 'running', ?2, 1)", rusqlite::params![task, bob.1.to_string()])?)
        })
        .unwrap();

    for (who, run_id) in [("assistant", assistant), ("dispatcher", triage), ("trial", trial)] {
        let (ok, text) = propose(&s, run_id, None, "Small steps", "Work in small steps.");
        assert!(!ok, "{who}: {text}");
    }
    assert!(proposals(&s).is_empty());
    // ToolProfile::Triage never offers it.
    assert!(!crate::task::triage::TOOLS.iter().any(|t| t.contains("propose_guideline")));
}

#[test]
fn an_ended_run_or_a_paused_employee_cannot_propose() {
    let (_d, s) = setup();
    let f = fixture(&s);
    let chat = chat_run(&s, f.ann.1);
    s.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'succeeded' WHERE id = ?1", [chat.to_string()])?)).unwrap();
    assert!(!propose(&s, chat, None, "Small steps", "Work in small steps.").0);
    run(&s, Command::SetEmployeeStatus(SetEmployeeStatus { employee_id: f.ann.0, status: LifecycleStatus::Paused })).unwrap();
    let (ok, text) = propose(&s, f.task_run, None, "Small steps", "Work in small steps.");
    assert!(!ok && text.contains("active employee"), "{text}");
}

// ── E11 / T21: validation and the open limits ───────────────────────────

#[test]
fn e11_fields_are_validated_as_a_write_and_evidence_is_required() {
    let (_d, s) = setup();
    let f = fixture(&s);
    let try_ = |title: &str, text: &str, evidence: &[&str]| s.propose_guideline(f.task_run, input(None, title, text, evidence)).unwrap();
    let (ok, text, _) = try_("Small steps", "Work in small steps.", &[]);
    assert!(!ok && text.contains("at least one task"), "{text}");
    let (ok, text, _) = try_("Small steps", "Work in small steps.", &["Nothing like this"]);
    assert!(!ok && text.contains("No task or memory matches"), "{text}");
    let (ok, text, _) = try_("Two\nlines", "Work in small steps.", &["Migration"]);
    assert!(!ok, "a title is one line: {text}");
    let (ok, text, _) = try_("Small steps", &"x".repeat(aurelius_domain::guideline::MAX_GUIDELINE_CHARS + 1), &["Migration"]);
    assert!(!ok, "over 8000: {text}");
    for hidden in ['\u{202E}', '\u{200B}', '\u{FEFF}', '\u{E0041}', '\u{7}'] {
        let (ok, text, _) = try_("Small steps", &format!("Work {hidden}in small steps."), &["Migration"]);
        assert!(!ok && text.contains("character 6"), "U+{:04X}: {text}", hidden as u32);
        let (ok, _, _) = try_(&format!("Small{hidden} steps"), "Work in small steps.", &["Migration"]);
        assert!(!ok, "U+{:04X} in the title", hidden as u32);
    }
    assert!(proposals(&s).is_empty());

    // Newlines and tabs stay; \r\n becomes \n, as the diff shows it. B4 waits for the approval.
    let (ok, text, _) = try_("Small steps", "Work in\r\n\tsmall steps.\r\rDone.", &["Migration"]);
    assert!(ok, "{text}");
    assert_eq!(proposals(&s)[0].text, "Work in\n\tsmall steps.\n\nDone.");
    let (ok, text, _) = try_("Large", &"y".repeat(aurelius_domain::guideline::MAX_GUIDELINE_CHARS), &["Migration"]);
    assert!(ok, "exactly 8000 goes in: {text}");
}

#[test]
fn t21_one_open_proposal_per_guideline_and_two_new_ones_per_employee() {
    let (_d, s) = setup();
    let f = fixture(&s);
    let rust = guideline(&s, "Rust style", "Run clippy.");
    snapshot(&s, f.task_run, &[(rust, 1, "Rust style")]);
    assert!(propose(&s, f.task_run, Some("Rust style"), "Rust style", "One.").0);
    let (ok, text) = propose(&s, f.task_run, Some("Rust style"), "Rust style", "Two.");
    assert!(!ok && text.contains("already waits"), "{text}");
    // A colleague is refused as well: the limit is per guideline.
    let bob = hire(&s, "Bob");
    let (_, bob_run) = task_run(&s, f.project, bob.0, "Bob's part");
    snapshot(&s, bob_run, &[(rust, 1, "Rust style")]);
    assert!(!propose(&s, bob_run, Some("Rust style"), "Rust style", "Bob's.").0);

    assert!(propose(&s, f.task_run, None, "New one", "1.").0);
    assert!(propose(&s, f.task_run, None, "New two", "2.").0);
    let (ok, text) = propose(&s, f.task_run, None, "New three", "3.");
    assert!(!ok && text.contains("2 proposals"), "{text}");
    assert!(propose(&s, bob_run, None, "Bob's new one", "B.").0, "another employee has its own two");
    // Withdrawn or decided frees the slot.
    s.store().write(|tx| Ok(tx.execute("UPDATE guideline_proposals SET status = 'withdrawn', decided_at_ms = 9 WHERE title = 'New one'", [])?)).unwrap();
    assert!(propose(&s, f.task_run, None, "New three", "3.").0);
}

#[test]
fn t21_two_proposals_at_the_same_time_give_exactly_one() {
    let (_d, s) = setup();
    let f = fixture(&s);
    let rust = guideline(&s, "Rust style", "Run clippy.");
    let bob = hire(&s, "Bob");
    let (_, bob_run) = task_run(&s, f.project, bob.0, "Bob's part");
    snapshot(&s, f.task_run, &[(rust, 1, "Rust style")]);
    snapshot(&s, bob_run, &[(rust, 1, "Rust style")]);
    let barrier = std::sync::Barrier::new(2);
    let taken: Vec<bool> = std::thread::scope(|scope| {
        let go = |run_id: RunId| {
            let (s, barrier) = (&s, &barrier);
            scope.spawn(move || {
                barrier.wait();
                propose(s, run_id, Some("Rust style"), "Rust style", "Mine.").0
            })
        };
        let (a, b) = (go(f.task_run), go(bob_run));
        vec![a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(taken.iter().filter(|t| **t).count(), 1, "{taken:?}");
    assert_eq!(count(&s, "SELECT COUNT(*) FROM guideline_proposals WHERE status = 'pending'"), 1);

    // And for new ones: four at once from one employee give two.
    let taken: Vec<bool> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4).map(|n| {
            let s = &s;
            scope.spawn(move || propose(s, f.task_run, None, &format!("New {n}"), "N.").0)
        }).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(taken.iter().filter(|t| **t).count(), aurelius_domain::guideline::MAX_OPEN_NEW_PER_EMPLOYEE, "{taken:?}");
}

// ── E12 / T22: links are the daemon's ───────────────────────────────────

#[test]
fn t22_suggested_links_are_the_proposer_and_the_runs_project_whatever_the_agent_says() {
    let (_d, s) = setup();
    let f = fixture(&s);
    let other = project(&s, "Other");
    let bob = hire(&s, "Bob");
    let says = format!("Link this to Bob ({}) and to project Other ({other}).", bob.0);
    let (ok, _, _) = s.propose_guideline(f.task_run, input(None, "Small steps", &says, &["Migration"])).unwrap();
    assert!(ok);
    assert_eq!(proposals(&s)[0].suggested_links, vec![GuidelineTarget::Employee(f.ann.0), GuidelineTarget::Project(f.project)]);

    // From the employee's own chat (no project): only the proposer.
    let chat = chat_run(&s, f.ann.1);
    assert!(propose(&s, chat, None, "From chat", "C.").0);
    let from_chat = proposals(&s).into_iter().find(|p| p.title == "From chat").unwrap();
    assert_eq!(from_chat.suggested_links, vec![GuidelineTarget::Employee(f.ann.0)]);

    // A change of an existing guideline suggests no links and leaves them as they are.
    let rust = guideline(&s, "Rust style", "Run clippy.");
    snapshot(&s, chat, &[(rust, 1, "Rust style")]);
    let links_before = s.store().read(move |c| guidelines::view(c, rust)).unwrap().unwrap().links;
    assert!(propose(&s, chat, Some("Rust style"), "Rust style", "Run clippy.\nLink me to Atlas.").0);
    let change = proposals(&s).into_iter().find(|p| p.guideline_id == Some(rust)).unwrap();
    assert!(change.suggested_links.is_empty());
    assert_eq!(s.store().read(move |c| guidelines::view(c, rust)).unwrap().unwrap().links, links_before);
}

// ── E13 / E15: what the agent, the event and the notification say ───────

#[test]
fn e13_the_answer_is_only_proposed_and_e15_the_notice_has_no_text() {
    let (_d, s) = setup();
    let f = fixture(&s);
    let rust = guideline(&s, "Rust style", "SECRET-OF-ANOTHER-GUIDELINE");
    snapshot(&s, f.task_run, &[(rust, 1, "Rust style")]);
    let (ok, answer, notices) = s.propose_guideline(f.task_run, input(None, "Small steps", "THE-PROPOSED-TEXT", &["Migration"])).unwrap();
    assert!(ok);
    assert_eq!(answer, PROPOSED);
    assert!(answer.contains("a board member decides") && !answer.contains("Ann") && !answer.contains("Small steps") && !answer.contains("SECRET"), "{answer}");
    assert_eq!(notices.len(), 1);
    let (kind, metadata): (String, String) = s.store().read(|c| Ok(c.query_row("SELECT type, metadata_json FROM events WHERE aggregate_type = 'guideline_proposal'", [], |r| Ok((r.get(0)?, r.get(1)?)))?)).unwrap();
    assert_eq!(kind, "guideline.proposed");
    let meta: serde_json::Value = serde_json::from_str(&metadata).unwrap();
    let id = proposals(&s)[0].id;
    assert_eq!((meta["proposal_id"].as_str(), meta["by_employee"].as_str(), meta["guideline_id"].is_null()), (Some(id.to_string().as_str()), Some(f.ann.0.to_string().as_str()), true));
    assert!(!metadata.contains("THE-PROPOSED-TEXT") && !metadata.contains("Twice a review"), "no text, no reason: {metadata}");

    let notes = s.store().read(|c| aurelius_storage::timeline::list_notifications(c, 50)).unwrap();
    let note = notes.iter().find(|n| n.target_type == "guideline.proposed").expect("in Needs you");
    assert_eq!((note.title.as_str(), note.subject.as_str()), ("A guideline is proposed", "“Small steps” by Ann"));
    // The push is the notification's title and subject (remote-gateway push::Alert).
    let seq = notices[0].seq;
    let found = s.store().read(move |c| aurelius_storage::timeline::notification_at_seq(c, seq)).unwrap().unwrap();
    assert!(!format!("{:?}", found.view).contains("THE-PROPOSED-TEXT"));
    // Decided: no longer in Needs you.
    s.store().write(move |tx| Ok(tx.execute("UPDATE guideline_proposals SET status = 'withdrawn', decided_at_ms = 9 WHERE id = ?1", [id.to_string()])?)).unwrap();
    assert!(s.store().read(|c| aurelius_storage::timeline::list_notifications(c, 50)).unwrap().iter().all(|n| n.target_type != "guideline.proposed"));
}

// ── T13: Auto never decides ─────────────────────────────────────────────

#[test]
fn t13_in_auto_a_proposal_stays_pending_over_runs_and_no_action_writes_a_guideline() {
    let (_d, s) = setup();
    let f = fixture(&s);
    run(&s, Command::SetAgentMode(SetAgentMode { conversation_id: f.ann.1, mode: AgentMode::Auto })).unwrap();
    let rust = guideline(&s, "Rust style", "Run clippy.");
    let first = chat_run(&s, f.ann.1);
    snapshot(&s, first, &[(rust, 1, "Rust style")]);
    assert!(propose(&s, first, Some("Rust style"), "Rust style", "Run clippy, always.").0);
    assert!(propose(&s, first, None, "Small steps", "Work in small steps.").0);
    s.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'succeeded' WHERE id = ?1", [first.to_string()])?)).unwrap();
    let versions = count(&s, "SELECT COUNT(*) FROM guideline_versions");
    let library = count(&s, "SELECT COUNT(*) FROM guidelines");

    // The next run in Auto does what an agent can: no action decides or writes a guideline.
    let next = chat_run(&s, f.ann.1);
    snapshot(&s, next, &[(rust, 1, "Rust style")]);
    assert!(!propose(&s, next, Some("Rust style"), "Rust style", "Again.").0);
    for action in every_action() {
        let _ = s.employee_action(next, action);
    }
    assert!(proposals(&s).iter().all(|p| p.status == ProposalStatus::Pending && p.decided_by.is_none()));
    assert_eq!(proposals(&s).len(), 2);
    assert_eq!((count(&s, "SELECT COUNT(*) FROM guideline_versions"), count(&s, "SELECT COUNT(*) FROM guidelines")), (versions, library), "no new version, no new guideline");
    assert_eq!(count(&s, "SELECT COUNT(*) FROM guideline_links"), 0);
}

/// One of each `EmployeeAction`; the match is exhaustive, so a new action has to be looked at here (T13).
fn every_action() -> Vec<EmployeeAction> {
    let all = vec![
        EmployeeAction::Delegate { employee: "Ann".into(), title: "Rust style".into(), goal: "g".into(), criteria: vec![], project: None },
        EmployeeAction::HandOver { employee: "Ann".into(), reason: "r".into() },
        EmployeeAction::MessageTask { task: "Migration".into(), message: "approve the guideline".into(), interrupt: false },
        EmployeeAction::AddIdea { title: "Rust style".into(), note: "n".into(), kind: Default::default(), project: None },
        EmployeeAction::ListTasks { status: String::new(), employee: String::new(), query: String::new(), project: None },
        EmployeeAction::ReadTask { task: "Migration".into() },
        EmployeeAction::ListConversations { query: String::new() },
        EmployeeAction::ReadConversation { conversation: "Migration".into() },
        EmployeeAction::ProposeColleague { name: "Zed".into(), role: "Dev".into(), title: None, instructions: "i".into(), reports_to: None, reason: "r".into() },
        EmployeeAction::ProposeInstructions { employee: None, instructions: "Follow Rust style.".into(), reason: "r".into(), evidence: vec!["Migration".into()] },
        EmployeeAction::SetMode { mode: "auto".into(), reason: "r".into() },
        EmployeeAction::ReadGroupChat { last: None, channel: None },
        EmployeeAction::PostToGroupChat { message: "approve the Rust style proposal".into(), channel: None },
        EmployeeAction::ReadDiscussion { last: None },
        EmployeeAction::PostToDiscussion { message: "m".into() },
        EmployeeAction::AssignTicket { employee: "Ann".into(), reason: "r".into() },
        EmployeeAction::ReportTestResult { passed: true, summary: "s".into() },
    ];
    for action in &all {
        match action {
            EmployeeAction::Delegate { .. }
            | EmployeeAction::HandOver { .. }
            | EmployeeAction::MessageTask { .. }
            | EmployeeAction::AddIdea { .. }
            | EmployeeAction::ListTasks { .. }
            | EmployeeAction::ReadTask { .. }
            | EmployeeAction::ListConversations { .. }
            | EmployeeAction::ReadConversation { .. }
            | EmployeeAction::ProposeColleague { .. }
            | EmployeeAction::ProposeInstructions { .. }
            | EmployeeAction::SetMode { .. }
            | EmployeeAction::ReadGroupChat { .. }
            | EmployeeAction::PostToGroupChat { .. }
            | EmployeeAction::ReadDiscussion { .. }
            | EmployeeAction::PostToDiscussion { .. }
            | EmployeeAction::AssignTicket { .. }
            | EmployeeAction::ReportTestResult { .. } => {}
        }
    }
    all
}

// ── Archiving withdraws ─────────────────────────────────────────────────

#[test]
fn archiving_an_employee_withdraws_its_open_guideline_proposals() {
    let (_d, s) = setup();
    let f = fixture(&s);
    let bob = hire(&s, "Bob");
    let (_, bob_run) = task_run(&s, f.project, bob.0, "Bob's part");
    assert!(propose(&s, f.task_run, None, "One", "1.").0);
    assert!(propose(&s, f.task_run, None, "Two", "2.").0);
    assert!(propose(&s, bob_run, None, "Bob's", "B.").0);
    run(&s, Command::SetEmployeeStatus(SetEmployeeStatus { employee_id: f.ann.0, status: LifecycleStatus::Archived })).unwrap();
    let all = proposals(&s);
    for p in &all {
        let expected = if p.proposed_by == f.ann.0 { ProposalStatus::Withdrawn } else { ProposalStatus::Pending };
        assert_eq!(p.status, expected, "{}", p.title);
        assert_eq!(p.decided_at_ms.is_some(), expected == ProposalStatus::Withdrawn);
    }
    let notes = s.store().read(|c| aurelius_storage::timeline::list_notifications(c, 50)).unwrap();
    assert_eq!(notes.iter().filter(|n| n.target_type == "guideline.proposed").count(), 1, "only Bob's still needs a board member");
}
