//! Sentinel SEC0, the canary: a fake secret goes into a work run's environment,
//! the run tries every way to leak it (text split over deltas, a command, tool
//! output in every encoding, the group chat, memory, an idea, an approval, a
//! ticket's discussion, a delegated task, the log), with trace mode on; a
//! board member pastes it (Team chat, ticket discussion, a task's title); an
//! app's task posts its end to a local webhook receiver; and then no form of it
//! (E12) may be in the database files, any table (the traces too), a blob, a
//! backup, the daemon's log, the events a client got or a webhook's body. A
//! project's secret reaches the runs of that project only (SEC1 M2). Its own
//! test binary: it catches the log of the whole process.
//! Scratch data folder; the key in its own `secrets/`; no real Claude.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::bridge::BridgeClient;
use aurelius_domain::connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults};
use aurelius_domain::employee::Setting;
use aurelius_domain::memory::EmployeeMemoryOp;
use aurelius_domain::vault::VaultKind;
use aurelius_domain::app_key::{AppAudience, AppScope, CreateAppKey};
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::{CreateTask, RunSettings};
use aurelius_domain::ticket::{CreateTicket, PostTicketMessage, TicketPriority};
use aurelius_domain::{Command, CommandResult, conversation::SendChatMessage};
use aurelius_protocol::{EmployeeTaskOp, VaultCreateParams};
use aurelius_runtime::claude_code::VaultEnv;
use aurelius_runtime::{Activity, AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, ToolProfile, fake};
use base64::Engine;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// At least 32 characters, with every character that encodes differently.
const CANARY: &str = "sk-test-FAKE-0000 canary+/=&\"\\% value-QX7";
/// A secret of project A only.
const PROJECT_CANARY: &str = "sk-test-FAKE-0000-project-a-only-ZK3";

#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);

impl Write for Log {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// What a run got: its environment from the vault and its instructions.
#[derive(Default)]
struct Seen {
    env: Vec<(String, String)>,
    /// Per run, in the order they started: the names of its vault variables.
    runs: Vec<Vec<String>>,
    system: Vec<String>,
    profiles: Vec<bool>,
}

/// A runtime that leaks everything it gets.
#[derive(Clone)]
struct Leaky {
    root: PathBuf,
    vault: VaultEnv,
    seen: Arc<Mutex<Seen>>,
    /// The first run of project A delegates a part with the value, once.
    delegated: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait::async_trait]
impl AgentRuntime for Leaky {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }

    fn with_vault(&self, vault: VaultEnv) -> Option<Arc<dyn AgentRuntime>> {
        Some(Arc::new(Leaky { vault, ..self.clone() }))
    }

    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        {
            let mut seen = self.seen.lock().unwrap();
            seen.env.extend(self.vault.0.iter().map(|v| (v.name.clone(), v.value.to_string())));
            seen.runs.push(self.vault.0.iter().map(|v| v.name.clone()).collect());
            seen.system.push(request.system.clone().unwrap_or_default());
            seen.profiles.push(matches!(request.profile, ToolProfile::Work { .. }));
        }
        let value = self.vault.0.iter().find(|v| v.name == "CANARY_TOKEN").map(|v| v.value.to_string()).unwrap_or_default();
        let in_a = self.vault.0.iter().any(|v| v.name == "PROJECT_A_KEY");
        let delegate = in_a && !self.delegated.swap(true, std::sync::atomic::Ordering::SeqCst);
        let (tx, rx) = mpsc::channel(64);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        let (root, run) = (self.root.clone(), request.run_id);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            if value.is_empty() {
                let _ = tx.send(RuntimeEvent::TextDelta("nothing from the vault".into())).await;
                let _ = tx.send(RuntimeEvent::Completed).await;
                return;
            }
            // Text, the value split over two deltas.
            let (head, tail) = value.split_at(11);
            for delta in [format!("The token is {head}"), format!("{tail}, as asked. ")] {
                let _ = tx.send(RuntimeEvent::TextDelta(delta)).await;
            }
            // A command with it, and its output in every encoding.
            let forms = forms(&value);
            let _ = tx.send(RuntimeEvent::Activity(Activity::Started { call_id: "c1".into(), tool: "Bash".into(), input: serde_json::json!({ "command": format!("echo {value}") }), parent: None })).await;
            let _ = tx.send(RuntimeEvent::Activity(Activity::Finished { call_id: "c1".into(), ok: true, output: forms.join("\n") })).await;
            // Aurelius' own tools, through the bridge.
            let bridge = BridgeClient::connect(&root).await.unwrap();
            let _ = bridge.employee_act(run, EmployeeTaskOp::PostToGroupChat { message: format!("FYI the key is {value}"), channel: None }).await;
            let _ = bridge.memory_run(run, EmployeeMemoryOp::Remember { kind: "project_knowledge".into(), scope: "project".into(), content: format!("the deploy key is {value}") }).await;
            let _ = bridge.employee_act(run, EmployeeTaskOp::AddIdea { title: format!("rotate {value}"), note: value.clone(), kind: Default::default(), project: None }).await;
            // A ticket's discussion (refused outside a ticket), and once a delegated part.
            let _ = bridge.employee_act(run, EmployeeTaskOp::PostToDiscussion { message: format!("the deploy key: {value}") }).await;
            if delegate {
                let part = EmployeeTaskOp::Delegate { employee: "Bridge".into(), title: format!("Use {value}"), goal: format!("Deploy with the key {value}"), criteria: vec![format!("{value} works")], project: None };
                let _ = bridge.employee_act(run, part).await;
            }
            let listed = bridge.employee_act(run, EmployeeTaskOp::VaultList { project: None }).await.unwrap();
            let read = bridge.employee_act(run, EmployeeTaskOp::VaultRead { item: "CANARY_TOKEN".into(), project: None }).await.unwrap();
            let _ = tx.send(RuntimeEvent::Activity(Activity::Started { call_id: "c2".into(), tool: "mcp__aurelius__vault_read".into(), input: serde_json::json!({}), parent: None })).await;
            let _ = tx.send(RuntimeEvent::Activity(Activity::Finished { call_id: "c2".into(), ok: read.0, output: format!("{}\n{}", listed.1, read.1) })).await;
            // A command with the literal value in Ask: an approval (and its push).
            let (ask_root, ask_value) = (root.clone(), value.clone());
            tokio::spawn(async move {
                let ask = BridgeClient::connect(&ask_root).await.unwrap();
                let _ = ask.permission_ask(run, Some("c3".into()), "Bash".into(), serde_json::json!({ "command": format!("curl -H 'X-Key: {ask_value}' https://api.example") })).await;
            });
            tokio::time::sleep(Duration::from_millis(300)).await;
            tracing::warn!("leaky runtime: {value}");
            let _ = tx.send(RuntimeEvent::TextDelta(format!("Final answer with {value}"))).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

/// Every form of E12 the value may be printed in.
fn forms(value: &str) -> Vec<String> {
    use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
    let hex: String = value.bytes().map(|b| format!("{b:02x}")).collect();
    let percent: String = value.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    let percent_lower: String = value.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02x}") }).collect();
    let form = percent.replace("%20", "+");
    let json = serde_json::to_string(value).unwrap();
    vec![
        value.to_owned(),
        STANDARD.encode(value),
        URL_SAFE.encode(value),
        URL_SAFE_NO_PAD.encode(format!("user:{value}")),
        STANDARD.encode(format!("ab{value}")),
        // Offset 1 (SEC1 M2): one byte before it.
        STANDARD.encode(format!("a{value}")),
        hex.clone(),
        hex.to_uppercase(),
        percent.clone(),
        percent_lower,
        form,
        json[1..json.len() - 1].to_owned(),
        json.replace('/', "\\/"),
    ]
}

/// The parts of each form that would give the value away: its middle, so the
/// edges a neighbouring character changes do not count.
fn needles(value: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for form in forms(value) {
        let n = form.len();
        let core = form.get(n / 4..n - n / 4).unwrap_or(&form).to_owned();
        if core.len() >= 8 && !out.contains(&core) {
            out.push(core);
        }
    }
    out.push("FAKE-0000".into());
    out
}

/// The parts of a value without its forms' shared marker (`FAKE-0000`).
fn needles_of(value: &str) -> Vec<String> {
    needles(value).into_iter().filter(|n| n != "FAKE-0000").collect()
}

/// A local webhook receiver: every request's body, answered 200.
struct Receiver {
    url: String,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Receiver {
    async fn start() -> Self {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://127.0.0.1:{}/hooks", listener.local_addr().unwrap().port());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let into = seen.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let into = into.clone();
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    let mut chunk = [0u8; 4096];
                    loop {
                        let n = match stream.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => n,
                        };
                        buffer.extend_from_slice(&chunk[..n]);
                        let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
                        let head = String::from_utf8_lossy(&buffer[..at]).to_lowercase();
                        let length = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                        if buffer.len() >= at + 4 + length {
                            into.lock().unwrap().push(String::from_utf8_lossy(&buffer[at + 4..at + 4 + length]).into_owned());
                            break;
                        }
                    }
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n").await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        Self { url, seen }
    }

    fn bodies(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}

fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files(&path));
        } else if let Ok(bytes) = std::fs::read(&path) {
            out.push((path.display().to_string(), bytes));
        }
    }
    out
}

fn found_in(bytes: &[u8], needles: &[String]) -> Vec<String> {
    needles.iter().filter(|n| bytes.windows(n.len()).any(|w| w == n.as_bytes())).cloned().collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_canary_secret_reaches_the_runs_environment_and_nothing_else() {
    let log = Log::default();
    let writer = log.clone();
    // The daemon's own writer (main.rs): every record redacted.
    tracing_subscriber::fmt().with_env_filter("trace").with_writer(move || aurelius_runtime::redact::Writer(writer.clone())).with_ansi(false).init();

    let dir = tempfile::tempdir().unwrap();
    let seen = Arc::new(Mutex::new(Seen::default()));
    // SAFETY (test): its own binary; set before the daemon starts. The webhook's local receiver is plain http.
    unsafe {
        std::env::set_var("AURELIUS_REMOTE_PORT", "0");
        std::env::set_var("AURELIUS_ENV", "dev");
    }
    let runtime = Leaky { root: dir.path().to_path_buf(), vault: VaultEnv::default(), seen: seen.clone(), delegated: Arc::default() };
    let daemon = start_with(dir.path(), Arc::new(runtime)).await;
    let (client, mut events) = connect_events(dir.path()).await;
    client.subscribe(0).await.unwrap();
    let add = AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude Code".into(), runtime_path: Some(dir.path().join("no-claude").display().to_string()), default_model: None };
    let CommandResult::ProviderAdded { connection_id } = client.execute(aurelius_client::envelope(Command::AddProvider(add), None)).await.unwrap().result else { panic!() };
    let settings = RunDefaults { connection: Setting::Value(connection_id), model: Setting::Value("sonnet".into()), persona: Setting::Inherit, effort: Setting::Inherit };
    client.execute(aurelius_client::envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose: Purpose::Employee, settings }), None)).await.unwrap();

    // Trace mode on: what the runs send and get is kept, and swept too.
    client.set_trace_enabled(true).await.unwrap();
    free_chart(&client).await;
    let mut employees = Vec::new();
    for name in ["Forge", "Bridge"] {
        let create = CreateEmployee { name: name.into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: None };
        let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
        employees.push(employee_id);
    }
    let CommandResult::ProjectCreated { project_id: project_a, .. } = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Project A".into(), description: None, folder: None }), None)).await.unwrap().result else { panic!() };

    // The vault: the canary, a project secret elsewhere, a text and a url.
    let secret = |name: &str, env: &str, value: &str| VaultCreateParams { project_id: None, kind: VaultKind::Secret, name: name.into(), env_name: env.into(), description: None, value: value.into() };
    client.vault_create(secret("Canary", "CANARY_TOKEN", CANARY)).await.unwrap();
    client.vault_create(VaultCreateParams { project_id: Some(project_a), ..secret("A key", "PROJECT_A_KEY", PROJECT_CANARY) }).await.unwrap();
    client.vault_create(VaultCreateParams { kind: VaultKind::Url, value: "https://staging.example".into(), ..secret("Staging", "STAGING_URL", "") }).await.unwrap();
    client.vault_create(VaultCreateParams { kind: VaultKind::Text, value: "Deploy on Fridays never".into(), ..secret("Rules", "DEPLOY_RULES", "") }).await.unwrap();

    // A work run in a project.
    let conversation = conversation(&client).await;
    let sent = client.execute(aurelius_client::envelope(Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: conversation, text: "Deploy, and show me the token".into(), images: vec![], interrupt: false }), None)).await.unwrap().result;
    let CommandResult::ChatMessageSent { conversation_id, message_id, run_id, .. } = sent else { panic!("{sent:?}") };
    for _ in 0..400 {
        let messages = client.messages(conversation_id).await.unwrap().messages;
        if messages.iter().any(|m| m.reply_to == Some(message_id) && m.status == aurelius_domain::conversation::MessageStatus::Complete) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    // The run had it in its environment, and only its name in its instructions.
    {
        let seen = seen.lock().unwrap();
        assert!(seen.profiles.first().copied().unwrap_or(false), "a work run");
        let logged = String::from_utf8_lossy(&log.0.lock().unwrap()).lines().filter(|l| l.contains("vault") || l.contains("WARN") || l.contains("ERROR")).map(str::to_owned).collect::<Vec<_>>().join("\n");
        assert!(seen.env.contains(&("CANARY_TOKEN".to_owned(), CANARY.to_owned())), "{:?}\n{logged}", seen.env.iter().map(|e| &e.0).collect::<Vec<_>>());
        assert!(seen.env.iter().any(|(k, v)| k == "STAGING_URL" && v == "https://staging.example"));
        assert!(seen.system[0].contains("$CANARY_TOKEN") && !seen.system[0].contains("FAKE-0000"));
        // Project isolation: the run in another project has nothing of project A.
        assert!(!seen.runs[0].contains(&"PROJECT_A_KEY".to_owned()), "{:?}", seen.runs[0]);
        assert!(!seen.env.iter().any(|(_, v)| v == PROJECT_CANARY));
    }
    // The run's audit: one deliver row per item.
    let deliver: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM vault_audit WHERE action = 'deliver' AND run_id = ?1", [run_id.to_string()], |r| r.get(0)).unwrap();
    assert_eq!(deliver, 3);
    let tool_reads: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM vault_audit WHERE action = 'tool_read' AND run_id = ?1", [run_id.to_string()], |r| r.get(0)).unwrap();
    assert!(tool_reads >= 2, "vault_list and vault_read are audited");
    // The answer and the activity show the marker instead.
    let answer = client.messages(conversation_id).await.unwrap().messages.into_iter().find(|m| m.reply_to == Some(message_id)).unwrap();
    assert!(answer.text.contains("«secret:CANARY_TOKEN»"), "{}", answer.text);
    assert!(answer.text.contains("The token is «secret:CANARY_TOKEN», as asked."), "split over two deltas: {}", answer.text);

    // B10: a board member pastes it, confirmed, in the general channel.
    let paste = aurelius_domain::group_chat::SendGroupMessage { text: format!("the key: {CANARY}"), images: vec![], files: vec![], channel: None, confirm_secret: false };
    assert!(client.send_group_message(paste.clone()).await.is_err());
    client.send_group_message(aurelius_domain::group_chat::SendGroupMessage { confirm_secret: true, ..paste }).await.unwrap();

    // A ticket in project A, started for Forge: its run leaks into the discussion and delegates a part with the value.
    let ticket = CreateTicket {
        confirm_secret: false,
        project_id: project_a,
        title: "Deploy".into(),
        description: None,
        priority: TicketPriority::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: Some(employees[0]),
        run: Default::default(),
        files: vec![],
        start: true,
        from_message: None,
    };
    let CommandResult::TicketCreated { ticket_id, .. } = client.create_ticket(ticket).await.unwrap().result else { panic!() };
    // B10 on the discussion and on a task's title: refused with the name; the post confirmed, kept redacted.
    let post = |confirm_secret| Command::PostTicketMessage(PostTicketMessage { ticket_id, text: format!("use {CANARY}"), images: vec![], files: vec![], confirm_secret });
    let refused = client.ticket_command(post(false), None).await.unwrap_err();
    assert!(matches!(&refused, aurelius_client::ClientError::Protocol(e) if e.code == aurelius_domain::ErrorCode::LooksLikeSecret), "{refused:?}");
    client.ticket_command(post(true), None).await.unwrap();
    let titled = CreateTask { confirm_secret: false, project_id: project_a, employee_id: Some(employees[1]), title: format!("Rotate {CANARY}"), goal: "Rotate it".into(), acceptance_criteria: vec![], depends_on: vec![], run: RunSettings::default(), files: vec![] };
    let refused = client.create_task(titled).await.unwrap_err();
    assert!(matches!(&refused, aurelius_client::ClientError::Protocol(e) if e.code == aurelius_domain::ErrorCode::LooksLikeSecret), "{refused:?}");

    // The ticket's run and its delegation's run, both in project A.
    let ticket_text = ticket_id.to_string();
    let ticket_runs = || -> i64 { db(dir.path()).query_row("SELECT COUNT(DISTINCT run_id) FROM vault_audit WHERE action = 'deliver' AND ticket_id = ?1", [&ticket_text], |r| r.get(0)).unwrap() };
    for _ in 0..800 {
        if ticket_runs() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    // Then an app's task (so the delegation above stays the ticket's): its end goes to the app's webhook, a local receiver.
    let receiver = Receiver::start().await;
    let made = client
        .create_app_key(CreateAppKey { name: "Hooked".into(), scopes: vec![AppScope::Read, AppScope::Run], limit_per_hour: None, limit_concurrent: None, audience: AppAudience::All, webhook_url: Some(receiver.url.clone()) })
        .await
        .unwrap();
    let app = aurelius_client::Client::connect_app(&aurelius_host::endpoint(dir.path()), &made.secret, "hooked").await.unwrap();
    let app_task = CreateTask { confirm_secret: false, project_id: project_a, employee_id: Some(employees[1]), title: "Check the deploy".into(), goal: "Check it".into(), acceptance_criteria: vec![], depends_on: vec![], run: RunSettings::default(), files: vec![] };
    app.execute(aurelius_client::envelope(Command::CreateTask(app_task), None)).await.unwrap();

    for _ in 0..800 {
        if !receiver.bodies().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    // Their leaks ran (each waits 300 ms before its last delta).
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(ticket_runs() >= 2, "the ticket's main run and the delegation's run");
    assert!(!receiver.bodies().is_empty(), "the webhook got the app task's end");
    {
        let seen = seen.lock().unwrap();
        assert!(seen.runs.iter().any(|r| r.contains(&"PROJECT_A_KEY".to_owned())), "project A's runs get its secret: {:?}", seen.runs);
    }
    let discussion = client.ticket(ticket_id).await.unwrap().ticket.conversation_id.expect("the ticket's discussion");
    let messages = client.messages(discussion).await.unwrap().messages;
    let texts: Vec<&str> = messages.iter().map(|m| m.text.as_str()).collect();
    assert!(texts.iter().any(|t| t.contains("the deploy key: «secret:CANARY_TOKEN»")), "the run's post, redacted: {texts:?}");
    assert!(texts.iter().any(|t| *t == "use «secret:CANARY_TOKEN»"), "the board member's, confirmed: {texts:?}");
    let delegated: String = db(dir.path()).query_row("SELECT title FROM tasks WHERE parent_task_id IS NOT NULL ORDER BY created_at_ms LIMIT 1", [], |r| r.get(0)).unwrap();
    assert_eq!(delegated, "Use «secret:CANARY_TOKEN»");
    let traces: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM run_traces", [], |r| r.get(0)).unwrap();
    assert!(traces > 0, "trace mode kept the runs' traces");

    // A backup now.
    client.create_backup().await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut received = Vec::new();
    while let Ok(event) = events.try_recv() {
        received.push(format!("{event:?} {}", serde_json::to_string(&event).unwrap_or_default()));
    }
    drop(client);
    stop(dir.path(), daemon).await;

    // The sweep: every form, as bytes.
    let mut needles = needles(CANARY);
    needles.extend(needles_of(PROJECT_CANARY));
    let mut hits: Vec<String> = Vec::new();
    for (path, bytes) in files(dir.path()) {
        // The key file holds the key, not the value; the runtime's own files are its (R1, none here).
        let found = found_in(&bytes, &needles);
        if !found.is_empty() {
            hits.push(format!("{path}: {found:?}"));
        }
    }
    let logged = log.0.lock().unwrap().clone();
    let found = found_in(&logged, &needles);
    if !found.is_empty() {
        hits.push(format!("daemon log: {found:?}"));
    }
    for body in receiver.bodies() {
        let found = found_in(body.as_bytes(), &needles);
        if !found.is_empty() {
            hits.push(format!("webhook body: {found:?}"));
        }
    }
    for event in &received {
        let found = found_in(event.as_bytes(), &needles);
        if !found.is_empty() {
            hits.push(format!("event {}: {found:?}", &event[..event.len().min(80)]));
        }
    }
    assert!(hits.is_empty(), "the canary leaked:\n{}", hits.join("\n"));
    assert!(!received.is_empty(), "the client got events");
    assert!(String::from_utf8_lossy(&logged).contains("«secret:CANARY_TOKEN»"), "the runtime's log line was there, redacted");
    // The request hash of the vault write is no plain hash of the value.
    let hashes: Vec<String> = {
        let conn = db(dir.path());
        let mut stmt = conn.prepare("SELECT request_hash FROM command_receipts WHERE method = 'vault.create'").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(hashes.len(), 4);
    use sha2::Digest;
    assert!(!hashes.contains(&hex::encode(sha2::Sha256::digest(CANARY.as_bytes()))));
}
