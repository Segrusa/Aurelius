//! Guidelines over the protocol (AUR-GL-01-F2): the reads and writes of a board
//! member, nothing for apps (SEC0 T10), B10 on every write (T14), the block a
//! run gets with the redactor as its safety net (T18), a new session when the
//! set of versions changes while a running run keeps its own (B6), and the seed
//! step at the start, once.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::{Client, ClientError, envelope};
use aurelius_domain::connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults};
use aurelius_domain::conversation::MessageStatus;
use aurelius_domain::employee::{CreateEmployee, SendEmployeeMessage, Setting};
use aurelius_domain::guideline::*;
use aurelius_domain::project::CreateProject;
use aurelius_domain::vault::VaultKind;
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, ErrorCode, GuidelineId, GuidelineProposalId, MessageId, ProjectId, RunId};
use aurelius_protocol::{Method, ResponseBody, VaultCreateParams};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, SessionUse, ToolProfile, fake};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// What one run asked of the runtime.
#[derive(Debug, Clone)]
struct Seen {
    run: RunId,
    session: SessionUse,
    system: String,
}

/// A runtime that keeps sessions like Claude Code does and records what each run
/// got; the first run whose message holds `[wait]` waits for `release`.
#[derive(Clone, Default)]
struct Sessions {
    seen: Arc<Mutex<Vec<Seen>>>,
    kept: Arc<Mutex<HashSet<String>>>,
    release: Arc<tokio::sync::Notify>,
    waited: Arc<std::sync::atomic::AtomicBool>,
}

impl QuotaOnce {
    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

impl Sessions {
    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    fn of(&self, run: RunId) -> Seen {
        self.seen().into_iter().find(|s| s.run == run).expect("the run reached the runtime")
    }
}

#[async_trait::async_trait]
impl AgentRuntime for Sessions {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }

    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let id = request.session_id.clone().unwrap_or_else(|| request.run_id.to_string());
        if matches!(request.profile, ToolProfile::Work { .. }) {
            self.seen.lock().unwrap().push(Seen { run: request.run_id, session: request.session, system: request.system.clone().unwrap_or_default() });
        }
        if request.session == SessionUse::Continue && !self.kept.lock().unwrap().contains(&id) {
            return Err(RuntimeError::Unavailable("session gone".into()));
        }
        let wait = matches!(request.profile, ToolProfile::Work { .. })
            && request.turns.iter().any(|t| t.text.contains("[wait]"))
            && !self.waited.swap(true, std::sync::atomic::Ordering::SeqCst);
        let release = self.release.clone();
        let text = fake::answer_for(&request);
        let (keep, kept) = (request.keep_session && request.session != SessionUse::None, self.kept.clone());
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            if wait {
                release.notified().await;
            }
            if keep {
                kept.lock().unwrap().insert(id);
            }
            let _ = tx.send(RuntimeEvent::TextDelta(text)).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }

    fn keeps_sessions(&self) -> bool {
        true
    }

    fn has_session(&self, id: &str) -> bool {
        self.kept.lock().unwrap().contains(id)
    }

    fn remove_session(&self, id: &str) {
        self.kept.lock().unwrap().remove(id);
    }
}

/// Employees run on Claude Code (the test runtime answers), so they work in a folder.
async fn claude_code(client: &Client, root: &Path) {
    let add = AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude Code".into(), runtime_path: Some(root.join("no-claude.exe").display().to_string()), default_model: None };
    let CommandResult::ProviderAdded { connection_id } = client.execute(envelope(Command::AddProvider(add), None)).await.unwrap().result else { panic!("not added") };
    let settings = RunDefaults { connection: Setting::Value(connection_id), model: Setting::Value("sonnet".into()), persona: Setting::Inherit, effort: Setting::Inherit };
    client.execute(envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose: Purpose::Employee, settings }), None)).await.unwrap();
}

async fn hire(client: &Client, name: &str) -> (EmployeeId, ConversationId) {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Engineer".into(), title: None, reports_to: None, instructions: Some(format!("Focus of {name}.")), folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    (employee_id, conversation_id)
}

async fn project(client: &Client, name: &str) -> ProjectId {
    let create = CreateProject { name: name.into(), description: None, folder: None };
    let CommandResult::ProjectCreated { project_id, .. } = client.execute(envelope(Command::CreateProject(create), None)).await.unwrap().result else { panic!() };
    project_id
}

async fn write(client: &Client, command: Command, expected: Option<u64>) -> Result<CommandResult, aurelius_protocol::ProtocolError> {
    match client.execute(envelope(command, expected)).await {
        Ok(receipt) => Ok(receipt.result),
        Err(ClientError::Protocol(e)) => Err(e),
        Err(other) => panic!("{other:?}"),
    }
}

async fn create(client: &Client, title: &str, text: &str, links: Vec<GuidelineTarget>) -> GuidelineId {
    let made = write(client, Command::CreateGuideline(CreateGuideline { title: title.into(), text: text.into(), links }), None).await.unwrap();
    let CommandResult::GuidelineChanged { guideline_id, version: 1 } = made else { panic!("{made:?}") };
    guideline_id
}

async fn read(client: &Client, method: Method) -> Result<ResponseBody, aurelius_protocol::ProtocolError> {
    match client.call(method).await {
        Ok(body) => Ok(body),
        Err(ClientError::Protocol(e)) => Err(e),
        Err(other) => panic!("{other:?}"),
    }
}

async fn run_guidelines(client: &Client, run: RunId) -> Vec<(GuidelineId, u32)> {
    let ResponseBody::RunGuidelines(r) = read(client, Method::RunGuidelines { run_id: run }).await.unwrap() else { panic!() };
    r.guidelines.iter().map(|g| (g.guideline_id, g.version)).collect()
}

/// Send `text` to the employee's chat; the run.
async fn send(client: &Client, employee: EmployeeId, chat: ConversationId, text: &str) -> (MessageId, RunId) {
    let send = SendEmployeeMessage { confirm_secret: false, employee_id: employee, text: text.into(), conversation_id: chat, images: vec![], files: vec![], interrupt: false };
    let CommandResult::EmployeeMessageSent { message_id, run_id, .. } = client.execute(envelope(Command::SendEmployeeMessage(send), None)).await.unwrap().result else { panic!() };
    (message_id, run_id)
}

async fn answered(client: &Client, conversation: ConversationId, message: MessageId) {
    for _ in 0..400 {
        let messages = client.messages(conversation).await.unwrap().messages;
        if messages.iter().any(|m| m.reply_to == Some(message) && m.status == MessageStatus::Complete) {
            tokio::time::sleep(Duration::from_millis(50)).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no answer to {message}");
}

async fn ask(client: &Client, employee: EmployeeId, chat: ConversationId, text: &str) -> RunId {
    let (message, run) = send(client, employee, chat, text).await;
    answered(client, chat, message).await;
    run
}

fn sql(root: &Path, statement: &str, params: impl rusqlite::Params) {
    let conn = rusqlite::Connection::open(root.join("data/aurelius.sqlite")).unwrap();
    conn.busy_timeout(Duration::from_secs(5)).unwrap();
    conn.execute(statement, params).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_board_member_writes_and_reads_guidelines() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (ann, _) = hire(&client, "Ann").await;
    let atlas = project(&client, "Atlas").await;
    let id = create(&client, "Rust rules", "Use clippy.", vec![GuidelineTarget::Employee(ann)]).await;

    let ResponseBody::Guidelines(list) = read(&client, Method::GuidelineList { archived: false }).await.unwrap() else { panic!() };
    assert_eq!(list.guidelines.iter().map(|g| (g.id, g.version, g.links.clone())).collect::<Vec<_>>(), vec![(id, 1, vec![GuidelineTarget::Employee(ann)])]);
    // The editor's version: a stale one is refused.
    let update = |text: &str| Command::UpdateGuideline(UpdateGuideline { guideline_id: id, title: None, text: Some(text.into()) });
    assert_eq!(write(&client, update("Use clippy and fmt."), Some(1)).await.unwrap(), CommandResult::GuidelineChanged { guideline_id: id, version: 2 });
    assert_eq!(write(&client, update("Stale."), Some(1)).await.unwrap_err().code, ErrorCode::VersionConflict);
    let ResponseBody::Guideline(detail) = read(&client, Method::GuidelineGet { guideline_id: id, version: Some(1) }).await.unwrap() else { panic!() };
    assert_eq!((detail.guideline.version, detail.version.version, detail.version.text.as_str()), (2, 1, "Use clippy."));
    let ResponseBody::GuidelineVersions(versions) = read(&client, Method::GuidelineVersions { guideline_id: id }).await.unwrap() else { panic!() };
    assert_eq!(versions.versions.iter().map(|v| v.version).collect::<Vec<_>>(), vec![2, 1]);
    let ResponseBody::EffectiveGuidelines(set) = read(&client, Method::GuidelineEffective { employee_id: Some(ann), project_id: Some(atlas) }).await.unwrap() else { panic!() };
    assert_eq!((set.guidelines.len(), set.total_chars, set.limit_chars), (1, 19, 16_000));
    let ResponseBody::GuidelineAffected(affected) = read(&client, Method::GuidelineAffected { guideline_id: id }).await.unwrap() else { panic!() };
    assert_eq!((affected.employees, affected.projects, affected.conversations), (1, 0, 0));

    // Errors with their codes.
    assert_eq!(write(&client, Command::CreateGuideline(CreateGuideline { title: "RUST rules".into(), text: "x".into(), links: vec![] }), None).await.unwrap_err().code, ErrorCode::GuidelineTitleTaken);
    assert_eq!(read(&client, Method::GuidelineGet { guideline_id: GuidelineId::new(), version: None }).await.unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(read(&client, Method::RunGuidelines { run_id: RunId::new() }).await.unwrap_err().code, ErrorCode::NotFound);
    let big = |c: &str| c.repeat(6_000);
    create(&client, "Big one", &big("a"), vec![GuidelineTarget::Employee(ann)]).await;
    create(&client, "Big two", &big("b"), vec![GuidelineTarget::Project(atlas)]).await;
    let over = write(&client, Command::CreateGuideline(CreateGuideline { title: "Big three".into(), text: big("c"), links: vec![GuidelineTarget::Project(atlas)] }), None).await.unwrap_err();
    assert_eq!(over.code, ErrorCode::GuidelineTooLarge);
    assert_eq!(over.too_large, Some(GuidelineTooLarge { chars: 18_019, limit: 16_000, employee_id: Some(ann), project_id: Some(atlas) }));

    // The starter set: its templates, then in the library.
    let ResponseBody::StarterSet(starter) = read(&client, Method::GuidelineStarterSet).await.unwrap() else { panic!() };
    assert_eq!(starter.templates.len(), 7);
    assert!(starter.templates.iter().all(|t| !t.in_library && t.chars as usize == t.text.chars().count()));
    let added = write(&client, Command::AddStarterGuidelines(AddStarterGuidelines { keys: vec!["g1".into(), "p2".into()] }), None).await.unwrap();
    let CommandResult::StarterGuidelinesAdded { guideline_ids } = added else { panic!() };
    assert_eq!(guideline_ids.len(), 2);
    let ResponseBody::StarterSet(starter) = read(&client, Method::GuidelineStarterSet).await.unwrap() else { panic!() };
    assert_eq!(starter.templates.iter().filter(|t| t.in_library).map(|t| t.key.as_str()).collect::<Vec<_>>(), vec!["g1", "p2"]);

    // Archived ones only with `archived`.
    write(&client, Command::ArchiveGuideline(ArchiveGuideline { guideline_id: id }), None).await.unwrap();
    let ResponseBody::Guidelines(archived) = read(&client, Method::GuidelineList { archived: true }).await.unwrap() else { panic!() };
    assert_eq!(archived.guidelines.iter().map(|g| g.id).collect::<Vec<_>>(), vec![id]);
    let ResponseBody::GuidelineProposals(proposals) = read(&client, Method::GuidelineProposals { status: None }).await.unwrap() else { panic!() };
    assert!(proposals.proposals.is_empty());
    stop(dir.path(), daemon).await;
}

/// T14: a vault value in a title or text is refused with the item's name, and
/// nothing is kept; `confirm_secret` is no field of a guideline write.
#[tokio::test(flavor = "multi_thread")]
async fn t14_b10_refuses_a_vault_value_in_every_guideline_write() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (ann, _) = hire(&client, "Ann").await;
    let value = "sk-test-FAKE-0000-gl-f2-t14-end";
    client
        .vault_create(VaultCreateParams { project_id: None, kind: VaultKind::Secret, name: "GL token".into(), env_name: "GL_TOKEN".into(), description: None, value: value.into() })
        .await
        .unwrap();
    let id = create(&client, "Clean", "Nothing secret.", vec![]).await;
    let proposal = GuidelineProposalId::new();
    sql(
        dir.path(),
        "INSERT INTO guideline_proposals(id, guideline_id, base_version, proposed_by, title, text, reason, evidence_json, suggested_links_json, status, created_at_ms)
         VALUES (?1, ?2, 1, ?3, 'Clean', 'Better.', 'Twice', '[]', '[]', 'pending', 1)",
        [proposal.to_string(), id.to_string(), ann.to_string()],
    );
    let refused = [
        Command::CreateGuideline(CreateGuideline { title: "Token".into(), text: format!("Use {value}."), links: vec![] }),
        Command::CreateGuideline(CreateGuideline { title: format!("Token {value}"), text: "x".into(), links: vec![] }),
        Command::UpdateGuideline(UpdateGuideline { guideline_id: id, title: None, text: Some(format!("Now {value}.")) }),
        Command::DecideGuidelineProposal(DecideGuidelineProposal { proposal_id: proposal, approve: true, title: None, text: Some(format!("Use {value}.")), links: None, note: None }),
    ];
    for command in refused {
        let name = command.method();
        let err = write(&client, command, None).await.unwrap_err();
        assert_eq!((err.code, err.matched_items.clone()), (ErrorCode::LooksLikeSecret, vec!["GL token".to_string()]), "{name}");
    }
    let receipts: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM command_receipts WHERE method LIKE 'guideline.%'", [], |r| r.get(0)).unwrap();
    assert_eq!(receipts, 1, "only the clean create");
    let held: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM guideline_versions WHERE instr(text, ?1) > 0 OR instr(title, ?1) > 0", [value], |r| r.get(0)).unwrap();
    assert_eq!(held, 0);
    // There is no "Send anyway" for a guideline: the field does not exist.
    let plain = serde_json::to_value(Command::CreateGuideline(CreateGuideline { title: "T".into(), text: "x".into(), links: vec![] })).unwrap();
    let mut with_flag = plain.clone();
    with_flag["confirm_secret"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Command>(plain).is_ok());
    assert!(serde_json::from_value::<Command>(with_flag).is_err(), "refused as an unknown field");
    stop(dir.path(), daemon).await;
}

/// T18: a value that became a vault secret after the guideline was written:
/// the next run gets it redacted, and its conversation says so by the item's name.
#[tokio::test(flavor = "multi_thread")]
async fn t18_the_next_run_gets_a_later_vault_value_redacted() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    let (ann, chat) = hire(&client, "Ann").await;
    let value = "sk-test-FAKE-0000-gl-f2-t18-end";
    create(&client, "Deploys", &format!("Deploy with {value} on Fridays."), vec![GuidelineTarget::Employee(ann)]).await;
    client
        .vault_create(VaultCreateParams { project_id: None, kind: VaultKind::Secret, name: "Deploy key".into(), env_name: "DEPLOY_KEY".into(), description: None, value: value.into() })
        .await
        .unwrap();
    let run = ask(&client, ann, chat, "How do we deploy?").await;
    let seen = runtime.of(run);
    assert!(!seen.system.contains(value) && seen.system.contains("Deploy with «secret:DEPLOY_KEY» on Fridays."), "{}", seen.system);
    let messages = client.messages(chat).await.unwrap().messages;
    let told: Vec<&str> = messages.iter().filter(|m| m.system.as_ref().is_some_and(|l| l.kind == aurelius_domain::views::SystemLineKind::Limit)).map(|m| m.text.as_str()).collect();
    assert!(told.len() == 1 && told[0].contains("Deploy key") && told[0].contains("“Deploys”") && !told[0].contains(value), "{told:?}");
    stop(dir.path(), daemon).await;
}

/// B6: the conversation goes on in its session while the versions stay; a new
/// version starts a new session with it; a run that is already running keeps the
/// version it started with, and its snapshot says so.
#[tokio::test(flavor = "multi_thread")]
async fn b6_another_version_starts_a_new_session_and_a_running_run_keeps_its_version() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    let (ann, chat) = hire(&client, "Ann").await;
    let (bo, bo_chat) = hire(&client, "Bo").await;
    let id = create(&client, "Rule", "Version one.", vec![GuidelineTarget::Employee(ann)]).await;

    let first = ask(&client, ann, chat, "First question").await;
    let second = ask(&client, ann, chat, "Second question").await;
    let bo_first = ask(&client, bo, bo_chat, "Bo's first").await;
    let edit = |text: &str| Command::UpdateGuideline(UpdateGuideline { guideline_id: id, title: None, text: Some(text.into()) });
    write(&client, edit("Version two."), None).await.unwrap();
    let ResponseBody::GuidelineAffected(affected) = read(&client, Method::GuidelineAffected { guideline_id: id }).await.unwrap() else { panic!() };
    assert_eq!((affected.employees, affected.conversations), (1, 1), "Ann's chat holds version one");
    let third = ask(&client, ann, chat, "Third question").await;
    let fourth = ask(&client, ann, chat, "Fourth question").await;
    let bo_second = ask(&client, bo, bo_chat, "Bo's second").await;

    let session = |run| runtime.of(run).session;
    assert_eq!((session(first), session(second)), (SessionUse::Keep, SessionUse::Continue), "unchanged: goes on");
    assert_eq!(session(third), SessionUse::Keep, "another version: a new session");
    assert!(runtime.of(third).system.contains("Version two.") && !runtime.of(third).system.contains("Version one."));
    assert!(runtime.of(second).system.contains("Version one."), "the session kept its text");
    assert_eq!(session(fourth), SessionUse::Continue, "then it goes on in the new one");
    assert_eq!((session(bo_first), session(bo_second)), (SessionUse::Keep, SessionUse::Continue), "another conversation is not touched");
    assert_eq!(run_guidelines(&client, first).await, vec![(id, 1)]);
    assert_eq!(run_guidelines(&client, second).await, vec![(id, 1)]);
    assert_eq!(run_guidelines(&client, third).await, vec![(id, 2)]);
    assert!(run_guidelines(&client, bo_first).await.is_empty());

    // A running run keeps the version it started with.
    let (message, running) = send(&client, ann, chat, "Long one [wait]").await;
    for _ in 0..400 {
        if runtime.seen().iter().any(|s| s.run == running) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    write(&client, edit("Version three."), None).await.unwrap();
    runtime.release.notify_one();
    answered(&client, chat, message).await;
    assert!(runtime.of(running).system.contains("Version two.") && !runtime.of(running).system.contains("Version three."));
    assert_eq!(run_guidelines(&client, running).await, vec![(id, 2)]);
    let next = ask(&client, ann, chat, "After the change").await;
    assert_eq!((session(next), run_guidelines(&client, next).await), (SessionUse::Keep, vec![(id, 3)]));
    stop(dir.path(), daemon).await;
}

/// A runtime without sessions whose first work run hits the usage limit; the
/// later ones answer. It keeps each work run's system text.
#[derive(Clone, Default)]
struct QuotaOnce {
    seen: Arc<Mutex<Vec<Seen>>>,
    hit: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait::async_trait]
impl AgentRuntime for QuotaOnce {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }

    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        if !matches!(request.profile, ToolProfile::Work { .. }) {
            return fake::FakeRuntime::default().start(request).await;
        }
        self.seen.lock().unwrap().push(Seen { run: request.run_id, session: request.session, system: request.system.clone().unwrap_or_default() });
        if self.hit.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return fake::FakeRuntime::default().start(request).await;
        }
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            let _ = tx.send(RuntimeEvent::Quota { reset_at_ms: Some(aurelius_storage::now_ms() + 3_600_000) }).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

/// Trace m2: a run parked at the usage limit without the conversation's session
/// goes again with the plan's system text; its snapshot is that text's versions,
/// not those of its first start.
#[tokio::test(flavor = "multi_thread")]
async fn a_parked_run_without_a_session_gets_the_snapshot_of_its_new_system_text() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = QuotaOnce::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    let (ann, chat) = hire(&client, "Ann").await;
    let id = create(&client, "Rule", "Version one.", vec![GuidelineTarget::Employee(ann)]).await;
    let (message, run) = send(&client, ann, chat, "A question").await;
    for _ in 0..400 {
        let status: Option<String> = db(dir.path()).query_row("SELECT error_code FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).unwrap_or(None);
        if status.as_deref() == Some("quota") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(run_guidelines(&client, run).await, vec![(id, 1)], "the first start");
    write(&client, Command::UpdateGuideline(UpdateGuideline { guideline_id: id, title: None, text: Some("Version two.".into()) }), None).await.unwrap();
    drop(client);
    stop(dir.path(), daemon).await;

    // The limit has reset by the time the daemon is back.
    sql(dir.path(), "UPDATE jobs SET not_before_ms = 0 WHERE state = 'ready'", []);
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    answered(&client, chat, message).await;
    let again = runtime.seen().into_iter().filter(|s| s.run == run).last().expect("it went again");
    assert!(again.system.contains("Version two.") && !again.system.contains("Version one."), "{}", again.system);
    assert_eq!(run_guidelines(&client, run).await, vec![(id, 2)], "the snapshot is what its system text has");
    stop(dir.path(), daemon).await;
}

/// B7: the seed step runs at the start, once: into an empty library with a
/// project "Aurelius", linking by name; a restart adds nothing.
#[tokio::test(flavor = "multi_thread")]
async fn the_seed_runs_at_the_start_once() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    // A new server has no project "Aurelius" at its first start: only the marker.
    let ResponseBody::Guidelines(list) = read(&client, Method::GuidelineList { archived: false }).await.unwrap() else { panic!() };
    assert!(list.guidelines.is_empty());
    let marker: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM settings WHERE key = 'guidelines_seeded'", [], |r| r.get(0)).unwrap();
    assert_eq!(marker, 1);
    let aurelius = project(&client, "Aurelius").await;
    let (forge, _) = hire(&client, "Forge").await;
    drop(client);
    stop(dir.path(), daemon).await;

    // A database of before the seed step (no marker) with the project: seeded at the start.
    sql(dir.path(), "DELETE FROM settings WHERE key = 'guidelines_seeded'", []);
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let ResponseBody::Guidelines(list) = read(&client, Method::GuidelineList { archived: false }).await.unwrap() else { panic!() };
    assert_eq!(list.guidelines.len(), 7);
    let ResponseBody::EffectiveGuidelines(set) = read(&client, Method::GuidelineEffective { employee_id: Some(forge), project_id: Some(aurelius) }).await.unwrap() else { panic!() };
    assert!(set.guidelines.len() >= 3 && set.dropped.is_empty(), "Forge's and the project's: {:?}", set.guidelines.iter().map(|g| &g.title).collect::<Vec<_>>());
    let sources: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM guideline_versions WHERE source = 'seed' AND by_user IS NULL", [], |r| r.get(0)).unwrap();
    assert_eq!(sources, 7);
    drop(client);
    stop(dir.path(), daemon).await;

    // Restarted: nothing more (the marker).
    let daemon = start(dir.path()).await;
    let count: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM guidelines", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 7);
    stop(dir.path(), daemon).await;
}
