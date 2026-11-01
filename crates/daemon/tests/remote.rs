//! Remote gateway end to end: pairing confirmed
//! on the desktop, sessions, the WebSocket protocol (the `Request` /
//! `ServerMessage` JSON of protocol 2.0), rights, uploads, revocation and
//! idempotent reconnects.

mod common;

use aurelius_client::{Client, bridge::BridgeClient, envelope};
use aurelius_domain::{
    ClientId, Command, CommandId, ConversationId, EventKind,
    approval::{ApprovalStatus, DecideApproval},
    conversation::SendChatMessage,
    files::AttachFile,
    employee::{CreateEmployee, SendEmployeeMessage},
    project::CreateProject,
    remote::Capability,
    task::CreateTask,
};
use aurelius_protocol::{
    CommandResult, ErrorCode, HelloParams, HelloResult, HostResult, HostWrite, Method, Outcome, PROTOCOL_MAJOR, Request, ResponseBody, ServerMessage, ShutdownWhen, WriteMethod, WriteResult,
    remote::{PairingPayload, RemoteStatus},
};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use base64::Engine;
use common::*;
use futures_util::{SinkExt, StreamExt};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{Notify, mpsc};
use tokio_tungstenite::tungstenite::{self, Message, client::IntoClientRequest};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// The environment of a server with remote access, set before any daemon starts.
fn remote_env() {
    // SAFETY (test): every test in this binary sets the same values.
    unsafe {
        std::env::set_var("AURELIUS_REMOTE_PORT", "0");
        // Pairing without Tailscale uses the dev fallback address; production requires Tailscale.
        std::env::set_var("AURELIUS_ENV", "dev");
        // Power requests never reach a real power-off: a harmless command and a countdown nobody waits for.
        std::env::set_var("AURELIUS_POWER_COMMAND", "echo power-test");
        std::env::set_var("AURELIUS_POWER_GRACE_SECS", "3600");
    }
}

async fn start_remote(root: &Path) -> Daemon {
    remote_env();
    start_with_delay(root, 5).await
}

/// A runtime whose answer stays alive until released, so its run can ask permissions.
struct Hold {
    release: Arc<Notify>,
}

#[async_trait::async_trait]
impl AgentRuntime for Hold {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let release = self.release.clone();
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            release.notified().await;
            let _ = tx.send(RuntimeEvent::TextDelta("done".into())).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

struct Phone {
    base: String,
    http: reqwest::Client,
    client_id: String,
    secret: String,
}

/// Enable remote access, pair a phone through the desktop, return its credentials.
async fn pair(desktop: &Client) -> (Phone, RemoteStatus) {
    pair_device(desktop, "my-iphone", "ios", None, false).await
}

/// Pair any device; `role` is what it asks for, `owner` what the desktop confirms.
async fn pair_device(desktop: &Client, name: &str, platform: &str, role: Option<&str>, owner: bool) -> (Phone, RemoteStatus) {
    let status = desktop.remote(HostWrite::RemoteEnable).await.unwrap();
    assert!(status.enabled);
    let base = format!("http://127.0.0.1:{}", status.port);
    let code = desktop.pairing_start().await.unwrap();
    let payload: PairingPayload = serde_json::from_str(&code.qr).unwrap();
    assert_eq!(payload.code, code.code);
    assert_eq!(payload.v, 1);

    let http = reqwest::Client::new();
    let pending: serde_json::Value = http
        .post(format!("{base}/v1/pair"))
        .json(&serde_json::json!({ "code": code.code, "device_name": name, "platform": platform, "role": role }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let pairing_id = pending["pairing_id"].as_str().unwrap().to_owned();

    // The phone waits (long poll) while the desktop decides.
    let poll = tokio::spawn({
        let (http, url) = (http.clone(), format!("{base}/v1/pair/{pairing_id}"));
        async move { http.get(url).send().await.unwrap().json::<serde_json::Value>().await.unwrap() }
    });
    let status = desktop.remote_status().await.unwrap();
    assert_eq!(status.pending.len(), 1);
    assert_eq!(status.pending[0].device_name, name);
    assert_eq!(status.pending[0].owner, role == Some("owner"));
    let status = desktop.remote(HostWrite::RemoteDecidePairing { pairing_id, approve: true, owner }).await.unwrap();
    let approved = poll.await.unwrap();
    assert_eq!(approved["status"], "approved", "{approved}");
    let phone = Phone {
        base,
        http,
        client_id: approved["client_id"].as_str().unwrap().to_owned(),
        secret: approved["device_secret"].as_str().unwrap().to_owned(),
    };
    (phone, status)
}

impl Phone {
    async fn session(&self) -> Result<String, reqwest::StatusCode> {
        let basic = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", self.client_id, self.secret));
        let response = self.http.post(format!("{}/v1/session", self.base)).header("authorization", format!("Basic {basic}")).send().await.unwrap();
        if !response.status().is_success() {
            return Err(response.status());
        }
        Ok(response.json::<serde_json::Value>().await.unwrap()["token"].as_str().unwrap().to_owned())
    }

    async fn socket(&self, token: &str) -> Result<Socket, tungstenite::Error> {
        let mut request = format!("{}/v1/ws", self.base.replace("http://", "ws://")).into_client_request().unwrap();
        request.headers_mut().insert("authorization", format!("Bearer {token}").parse().unwrap());
        tokio_tungstenite::connect_async(request).await.map(|(socket, _)| socket)
    }

    /// Upload a file into this device's inbox; the answer has `name` and `path`.
    async fn upload(&self, token: &str, name: &str, bytes: Vec<u8>) -> serde_json::Value {
        let form = reqwest::multipart::Form::new().part("file", reqwest::multipart::Part::bytes(bytes).file_name(name.to_owned()));
        self.http
            .post(format!("{}/v1/upload", self.base))
            .header("authorization", format!("Bearer {token}"))
            .multipart(form)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }
}

/// Send one request and wait for its response.
async fn send(socket: &mut Socket, request: Request) -> Outcome {
    let id = request.request_id;
    socket.send(Message::Text(serde_json::to_string(&request).unwrap().into())).await.unwrap();
    loop {
        if let ServerMessage::Response { request_id, outcome } = next(socket).await {
            if request_id == id {
                return outcome;
            }
        }
    }
}

/// A read or session method.
async fn call(socket: &mut Socket, id: u64, method: Method) -> Outcome {
    send(socket, Request::read(id, method)).await
}

/// A write under a fresh idempotency key.
async fn write(socket: &mut Socket, id: u64, write: WriteMethod) -> Outcome {
    send(socket, Request::write(id, CommandId::new(), write)).await
}

async fn command(socket: &mut Socket, id: u64, command: Command) -> Outcome {
    write(socket, id, WriteMethod::Domain(command)).await
}

async fn host(socket: &mut Socket, id: u64, host: HostWrite) -> Outcome {
    write(socket, id, WriteMethod::Host(host)).await
}

/// The command result of a successful write.
fn done(outcome: Outcome) -> CommandResult {
    match outcome {
        Outcome::Ok(ResponseBody::Receipt(receipt)) => match receipt.result {
            WriteResult::Command(result) => result,
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

fn accepted(outcome: &Outcome) -> bool {
    matches!(outcome, Outcome::Ok(ResponseBody::Receipt(_)))
}

async fn next(socket: &mut Socket) -> ServerMessage {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), socket.next()).await.expect("message in time").expect("open").unwrap();
        if let Message::Text(text) = message {
            return serde_json::from_str(text.as_str()).unwrap();
        }
    }
}

async fn hello(socket: &mut Socket) -> HelloResult {
    let params = HelloParams { client_name: "phone".into(), client_version: "0".into(), app_key: None, accepts: Vec::new(), session_token: None };
    match call(socket, 1, Method::SessionHello(params)).await {
        Outcome::Ok(ResponseBody::Hello(h)) => {
            assert_eq!(h.protocol, PROTOCOL_MAJOR);
            assert!(h.capabilities.contains(&Capability::Approve));
            assert!(h.remote);
            assert!(h.client_id.is_some());
            sign_in_phone(socket).await;
            h
        }
        other => panic!("{other:?}"),
    }
}

/// The board member the desktop's `connect` made signs in on the phone (protocol 5: the gate).
async fn sign_in_phone(socket: &mut Socket) {
    let login = aurelius_protocol::LoginParams { username: BOARD_MEMBER.into(), password: BOARD_PASSWORD.into(), device_name: "Phone".into() };
    match call(socket, 1_000_000, Method::UserLogin(login)).await {
        Outcome::Ok(ResponseBody::Login(login)) => assert_eq!(login.user.username, BOARD_MEMBER),
        other => panic!("sign in on the phone: {other:?}"),
    }
}

fn forbidden(outcome: Outcome) -> bool {
    matches!(outcome, Outcome::Error(e) if e.code == ErrorCode::Forbidden)
}

/// Waits for the close frame of a revoked connection; its code.
async fn closed_with(socket: &mut Socket) -> Option<u16> {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), socket.next()).await.unwrap() {
            Some(Ok(Message::Close(frame))) => return frame.map(|f| u16::from(f.code)),
            Some(Ok(_)) => continue,
            _ => return None,
        }
    }
}

fn chat(conversation_id: ConversationId, text: &str, images: Vec<String>) -> Command {
    Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id, text: text.into(), images, interrupt: false })
}

#[tokio::test(flavor = "multi_thread")]
async fn paired_phone_uses_the_same_sessions_with_limits() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let desktop = connect(dir.path()).await;

    // R06: off after a fresh install.
    assert!(!desktop.remote_status().await.unwrap().enabled);
    assert!(desktop.pairing_start().await.is_err(), "no codes while remote access is off");

    let (phone, status) = pair(&desktop).await;
    assert_eq!(status.clients.len(), 1);
    let talk = common::conversation(&desktop).await;
    assert_eq!(status.clients[0].name, "my-iphone");

    // Wrong credentials and tokens get nothing (R04).
    let wrong = Phone { base: phone.base.clone(), http: phone.http.clone(), client_id: phone.client_id.clone(), secret: "nope".into() };
    assert_eq!(wrong.session().await.unwrap_err(), reqwest::StatusCode::UNAUTHORIZED);
    assert!(phone.socket("not-a-token").await.is_err());

    let token = phone.session().await.unwrap();
    let mut socket = phone.socket(&token).await.unwrap();
    hello(&mut socket).await;
    assert!(desktop.remote_status().await.unwrap().clients[0].connected);

    // The phone may look for and install server updates (signed releases only).
    assert!(matches!(call(&mut socket, 6, Method::UpdateGet { check: false }).await, Outcome::Ok(ResponseBody::Update(_))));
    assert!(!forbidden(host(&mut socket, 7, HostWrite::UpdateApply).await), "dev has no updates, but the phone is allowed to ask");

    // Server management and host folders stay on the PC.
    assert!(forbidden(call(&mut socket, 2, Method::RemoteGetStatus).await));
    assert!(forbidden(host(&mut socket, 3, HostWrite::BackupCreate).await));
    let with_folder = Command::CreateProject(CreateProject { name: "X".into(), description: None, folder: Some(dir.path().display().to_string()) });
    assert!(forbidden(command(&mut socket, 4, with_folder).await));
    let host_file = dir.path().join("data").join("host_id").display().to_string();
    let outside = Command::AttachFile(AttachFile { conversation_id: talk, path: host_file.clone() });
    assert!(forbidden(command(&mut socket, 5, outside).await), "no arbitrary host files");
    assert!(forbidden(command(&mut socket, 8, chat(talk, "kijk", vec![host_file.clone()])).await), "no arbitrary host files as images");

    // R02: the same idempotency key after reconnects creates one project.
    let key = CommandId::new();
    let create = WriteMethod::Domain(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }));
    for round in 0..20u64 {
        let mut again = phone.socket(&token).await.unwrap();
        hello(&mut again).await;
        match send(&mut again, Request::write(10 + round, key, create.clone())).await {
            Outcome::Ok(ResponseBody::Receipt(r)) => assert_eq!(r.replayed, round > 0),
            other => panic!("{other:?}"),
        }
        drop(again);
    }
    let projects = desktop.projects().await.unwrap().projects;
    assert_eq!(projects.iter().filter(|p| p.name == "Atlas").count(), 1);

    // A write without an idempotency key is refused.
    let keyless = Request { idempotency_key: None, ..Request::write(30, CommandId::new(), create.clone()) };
    assert!(matches!(send(&mut socket, keyless).await, Outcome::Error(e) if e.code == ErrorCode::InvalidRequest));

    // Live chat: events and streamed text arrive as appends.
    match call(&mut socket, 40, Method::EventSubscribe { after_seq: desktop.health().await.unwrap().last_seq }).await {
        Outcome::Ok(ResponseBody::Subscribed { .. }) => {}
        other => panic!("{other:?}"),
    }
    assert!(accepted(&command(&mut socket, 41, chat(talk, "Tell me a long story", Vec::new())).await));
    let (mut appends, mut completed) = (0, false);
    while !completed {
        match next(&mut socket).await {
            ServerMessage::StreamAppend(_) => appends += 1,
            ServerMessage::Stream(_) => panic!("remote clients get appends"),
            ServerMessage::Event(e) if e.kind == EventKind::MessageAssistantCommitted => completed = true,
            _ => {}
        }
    }
    assert!(appends > 0);

    // Upload, then attach from the inbox.
    let uploaded = phone.upload(&token, "../note.txt", b"hello from the phone".to_vec()).await;
    assert_eq!(uploaded["name"], "note.txt");
    let attach = Command::AttachFile(AttachFile { conversation_id: talk, path: uploaded["path"].as_str().unwrap().into() });
    assert!(accepted(&command(&mut socket, 42, attach).await));

    // A photo: uploaded, then sent with a message (it stays with the message).
    let png = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=")
        .unwrap();
    let uploaded = phone.upload(&token, "photo.png", png).await;
    let fresh = common::conversation(&desktop).await;
    let CommandResult::ChatMessageSent { conversation_id: conversation, .. } = done(command(&mut socket, 43, chat(fresh, "", vec![uploaded["path"].as_str().unwrap().into()])).await) else { panic!() };
    let messages = desktop.messages(conversation).await.unwrap().messages;
    assert_eq!(messages[0].files.len(), 1);
    assert!(messages[0].files[0].name.starts_with("photo.") && messages[0].files[0].media_type.starts_with("image/"));

    // A file for an employee: only from this device's inbox; it lands in the work folder.
    let atlas = projects.iter().find(|p| p.name == "Atlas").unwrap();
    let CommandResult::EmployeeCreated { employee_id, .. } = desktop
        .execute(envelope(
            Command::CreateEmployee(CreateEmployee { name: "Iris".into(), role: "Writer".into(), title: None, reports_to: None, instructions: None, folder: None }),
            None,
        ))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    // The message goes to the conversation of one of its tasks.
    let task = CreateTask { confirm_secret: false, files: vec![], run: Default::default(), project_id: atlas.id, employee_id: Some(employee_id), title: "Read".into(), goal: "Read what comes".into(), acceptance_criteria: vec![], depends_on: vec![] };
    desktop.execute(envelope(Command::CreateTask(task), None)).await.unwrap();
    let mut thread = None;
    for _ in 0..400 {
        thread = desktop.tasks(Some(atlas.id)).await.unwrap().tasks.into_iter().find_map(|t| t.conversation_id);
        if thread.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let thread = thread.expect("the task's conversation");
    let send_files = |files: Vec<String>| Command::SendEmployeeMessage(SendEmployeeMessage { confirm_secret: false, employee_id, text: "Read this".into(), conversation_id: thread, images: Vec::new(), files, interrupt: false });
    assert!(forbidden(command(&mut socket, 44, send_files(vec![host_file])).await), "no arbitrary host files as uploads");
    let uploaded = phone.upload(&token, "brief.md", b"# Brief".to_vec()).await;
    assert!(accepted(&command(&mut socket, 45, send_files(vec![uploaded["path"].as_str().unwrap().into()])).await));
    let messages = desktop.messages(thread).await.unwrap().messages;
    let sent = messages.iter().find(|m| !m.uploads.is_empty()).expect("the message with the file");
    assert_eq!(sent.uploads.len(), 1);
    assert_eq!(sent.uploads[0].path, ".aurelius/uploads/brief.md");
    let folder = Path::new(atlas.folder.as_deref().unwrap());
    assert_eq!(std::fs::read_to_string(folder.join(".aurelius/uploads/brief.md")).unwrap(), "# Brief");

    // Revoking closes the open stream and the credential stops working (R04).
    let client_id = ClientId::parse(&phone.client_id).unwrap();
    desktop.remote(HostWrite::RemoteRevoke { client_id }).await.unwrap();
    assert_eq!(closed_with(&mut socket).await, Some(4401));
    assert_eq!(phone.session().await.unwrap_err(), reqwest::StatusCode::UNAUTHORIZED);
    assert!(phone.socket(&token).await.is_err());

    desktop.remote(HostWrite::RemoteDisable).await.unwrap();
    assert!(reqwest::get(format!("{}/v1/health", phone.base)).await.is_err(), "gateway stopped");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_of_another_protocol_major_is_refused_and_bad_json_is_answered() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let desktop = connect(dir.path()).await;
    let (phone, _) = pair(&desktop).await;
    let mut socket = phone.socket(&phone.session().await.unwrap()).await.unwrap();
    hello(&mut socket).await;

    let old = Request { protocol: 1, ..Request::read(2, Method::ServerGetHealth) };
    assert!(matches!(send(&mut socket, old).await, Outcome::Error(e) if e.code == ErrorCode::IncompatibleProtocol));
    assert!(matches!(call(&mut socket, 3, Method::ServerGetHealth).await, Outcome::Ok(ResponseBody::Health(_))), "the session goes on");

    // Methods of the agent bridge are not part of the client protocol: unknown here, answered for their own id.
    // Text that is not JSON has no id: invalid_request for id 0.
    for (id, text, code) in [
        (4, serde_json::json!({ "protocol": 2, "request_id": 4, "method": "permission.ask", "params": { "run_id": aurelius_domain::RunId::new(), "call_id": null, "tool": "Bash", "input": {} } }).to_string(), ErrorCode::UnknownMethod),
        (5, serde_json::json!({ "protocol": 2, "request_id": 5, "method": "daemon.shutdown" }).to_string(), ErrorCode::UnknownMethod),
        (0, "{oops".to_owned(), ErrorCode::InvalidRequest),
    ] {
        socket.send(Message::Text(text.into())).await.unwrap();
        loop {
            if let ServerMessage::Response { request_id, outcome } = next(&mut socket).await
                && request_id == id
            {
                assert!(matches!(outcome, Outcome::Error(ref e) if e.code == code), "{outcome:?}");
                break;
            }
        }
    }
    assert!(matches!(call(&mut socket, 6, Method::ServerGetHealth).await, Outcome::Ok(ResponseBody::Health(_))), "the session goes on");

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn desktop_and_phone_approving_together_run_one_action() {
    let dir = tempfile::tempdir().unwrap();
    remote_env();
    let release = Arc::new(Notify::new());
    let daemon = start_with(dir.path(), Arc::new(Hold { release: release.clone() })).await;
    let desktop = connect(dir.path()).await;
    let (phone, _) = pair(&desktop).await;
    let token = phone.session().await.unwrap();
    let mut socket = phone.socket(&token).await.unwrap();
    hello(&mut socket).await;

    // A run that stays alive asks permission through the agent bridge.
    let talk = common::conversation(&desktop).await;
    let CommandResult::ChatMessageSent { run_id, .. } = desktop.execute(envelope(chat(talk, "Samen", Vec::new()), None)).await.unwrap().result else { panic!() };
    for _ in 0..200 {
        let status: Option<String> = db(dir.path()).query_row("SELECT status FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).ok();
        if matches!(status.as_deref(), Some("starting" | "running")) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let waiting = tokio::spawn(async move { bridge.permission_ask(run_id, None, "Bash".into(), serde_json::json!({ "command": "dir" })).await.unwrap() });
    let approval = loop {
        if let Some(a) = desktop.approvals().await.unwrap().approvals.into_iter().find(|a| a.status == ApprovalStatus::Pending) {
            break a;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    let decide = || Command::DecideApproval(DecideApproval { approval_id: approval.id, action_digest: approval.action_digest.clone(), approve: true, always_allow: false });
    let from_desktop = desktop.execute(envelope(decide(), None));
    let from_phone = command(&mut socket, 2, decide());
    let (desktop_result, phone_result) = tokio::join!(from_desktop, from_phone);
    assert!(desktop_result.is_ok() ^ accepted(&phone_result), "exactly one decision wins (R05)");
    assert!(waiting.await.unwrap().allow, "the agent is told to go ahead once");

    release.notify_one();
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn refused_pairing_creates_no_device() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let desktop = connect(dir.path()).await;
    let status = desktop.remote(HostWrite::RemoteEnable).await.unwrap();
    let base = format!("http://127.0.0.1:{}", status.port);
    let code = desktop.pairing_start().await.unwrap();
    let http = reqwest::Client::new();

    let bad = http.post(format!("{base}/v1/pair")).json(&serde_json::json!({ "code": "AAAA2222", "device_name": "x", "platform": "ios" })).send().await.unwrap();
    assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);

    let pending: serde_json::Value = http
        .post(format!("{base}/v1/pair"))
        .json(&serde_json::json!({ "code": code.code, "device_name": "stranger", "platform": "android" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let pairing_id = pending["pairing_id"].as_str().unwrap().to_owned();
    desktop.remote(HostWrite::RemoteDecidePairing { pairing_id: pairing_id.clone(), approve: false, owner: false }).await.unwrap();
    let refused = http.get(format!("{base}/v1/pair/{pairing_id}")).send().await.unwrap();
    assert_eq!(refused.status(), reqwest::StatusCode::FORBIDDEN);
    assert!(desktop.remote_status().await.unwrap().clients.is_empty());

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn phone_changes_model_and_effort_but_never_the_connection() {
    use aurelius_domain::connection::{ConfigScope, Purpose, SetRunDefaults};
    use aurelius_domain::employee::Setting;
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let desktop = connect(dir.path()).await;
    let (phone, _) = pair(&desktop).await;
    let token = phone.session().await.unwrap();
    let mut socket = phone.socket(&token).await.unwrap();
    hello(&mut socket).await;

    let current = match call(&mut socket, 2, Method::ProviderList).await {
        Outcome::Ok(ResponseBody::Connections(c)) => c
            .defaults
            .into_iter()
            .find(|d| d.scope == ConfigScope::Global && d.purpose == Purpose::Employee)
            .map(|d| d.settings)
            .unwrap_or_default(),
        other => panic!("{other:?}"),
    };
    let set = |settings| Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose: Purpose::Employee, settings });

    let mut model_only = current.clone();
    model_only.model = Setting::Value("opus".into());
    model_only.effort = Setting::Value("high".into());
    assert!(accepted(&command(&mut socket, 3, set(model_only)).await));

    let mut other_connection = current.clone();
    other_connection.connection = Setting::Value(aurelius_domain::ProviderId::new());
    assert!(forbidden(command(&mut socket, 4, set(other_connection)).await), "a phone never switches the (possibly paid) connection");

    let mut persona = current;
    persona.persona = Setting::Value("Marcus".into());
    assert!(forbidden(command(&mut socket, 5, set(persona)).await));

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn phone_sets_an_employees_model_and_effort_but_not_its_connection() {
    use aurelius_domain::connection::{ConfigScope, Purpose, RunDefaults, SetRunDefaults};
    use aurelius_domain::employee::Setting;
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let desktop = connect(dir.path()).await;
    let CommandResult::ProjectCreated { project_id, .. } =
        desktop.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let CommandResult::EmployeeCreated { employee_id, .. } = desktop
        .execute(envelope(
            Command::CreateEmployee(CreateEmployee { name: "Iris".into(), role: "Writer".into(), title: None, reports_to: None, instructions: None, folder: None }),
            None,
        ))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    // The global employee default has a persona; the employee's own layer has no row yet.
    let global = RunDefaults { persona: Setting::Value("Marcus".into()), ..Default::default() };
    desktop.execute(envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose: Purpose::Employee, settings: global }), None)).await.unwrap();
    let (phone, _) = pair(&desktop).await;
    let token = phone.session().await.unwrap();
    let mut socket = phone.socket(&token).await.unwrap();
    hello(&mut socket).await;
    let set = |purpose, settings| Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Employee(employee_id), purpose, settings });

    // A new employee row: connection and persona stay Inherit, model and effort change.
    let model_only = RunDefaults { model: Setting::Value("opus".into()), effort: Setting::Value("high".into()), ..Default::default() };
    assert!(accepted(&command(&mut socket, 2, set(Purpose::Employee, model_only.clone())).await));
    // And again, now against the row it just wrote.
    let mut again = model_only.clone();
    again.model = Setting::Value("sonnet".into());
    assert!(accepted(&command(&mut socket, 3, set(Purpose::Employee, again.clone())).await));
    let snapshot = desktop.connections().await.unwrap();
    let mine = snapshot.defaults.iter().find(|d| d.scope == ConfigScope::Employee(employee_id) && d.purpose == Purpose::Employee).unwrap();
    assert_eq!(mine.settings, again);

    let mut other_connection = again.clone();
    other_connection.connection = Setting::Value(aurelius_domain::ProviderId::new());
    assert!(forbidden(command(&mut socket, 4, set(Purpose::Employee, other_connection)).await), "a phone never switches an employee's connection");
    let mut persona = again.clone();
    persona.persona = Setting::Value("Marcus".into());
    assert!(forbidden(command(&mut socket, 5, set(Purpose::Employee, persona)).await), "the employee's own persona is compared, not the global one");
    // Its subagents: model and effort too, never the connection.
    assert!(accepted(&command(&mut socket, 6, set(Purpose::Subagent, model_only.clone())).await));
    let mut subagent_connection = model_only.clone();
    subagent_connection.connection = Setting::Value(aurelius_domain::ProviderId::new());
    assert!(forbidden(command(&mut socket, 7, set(Purpose::Subagent, subagent_connection.clone())).await));
    // Other purposes at an employee's scope still need an owner.
    assert!(forbidden(command(&mut socket, 8, set(Purpose::Memory, model_only.clone())).await));

    // The global default for subagents, against the current row.
    let snapshot = desktop.connections().await.unwrap();
    let mut subagent = snapshot.defaults.iter().find(|d| d.scope == ConfigScope::Global && d.purpose == Purpose::Subagent).map(|d| d.settings.clone()).unwrap_or_default();
    subagent.model = Setting::Value("haiku".into());
    let global = |purpose, settings| Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose, settings });
    assert!(accepted(&command(&mut socket, 11, global(Purpose::Subagent, subagent.clone())).await));
    subagent.connection = Setting::Value(aurelius_domain::ProviderId::new());
    assert!(forbidden(command(&mut socket, 12, global(Purpose::Subagent, subagent)).await), "the global subagent connection stays as it is");
    // Global defaults for memory need an owner.
    assert!(forbidden(command(&mut socket, 13, global(Purpose::Memory, model_only)).await));

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_computer_paired_as_owner_may_manage_the_server() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let desktop = connect(dir.path()).await;
    let (mac, status) = pair_device(&desktop, "macbook", "macos", Some("owner"), true).await;
    assert!(status.clients[0].capabilities.contains(&Capability::Owner));
    let token = mac.session().await.unwrap();
    let mut socket = mac.socket(&token).await.unwrap();
    let hello = hello(&mut socket).await;
    assert!(hello.capabilities.contains(&Capability::Owner));
    assert_eq!(hello.client_id, Some(ClientId::parse(&mac.client_id).unwrap()));
    assert!(!hello.host_name.is_empty());

    // Server management: remote access, folders on the server, backups.
    assert!(matches!(call(&mut socket, 2, Method::RemoteGetStatus).await, Outcome::Ok(ResponseBody::Remote(_))));
    let folder = dir.path().join("work");
    std::fs::create_dir_all(folder.join("src")).unwrap();
    let with_folder = Command::CreateProject(CreateProject { name: "Owned".into(), description: None, folder: Some(folder.display().to_string()) });
    assert!(accepted(&command(&mut socket, 3, with_folder).await));
    match call(&mut socket, 4, Method::FsListDir { path: Some(folder.display().to_string()) }).await {
        Outcome::Ok(ResponseBody::DirListing(listing)) => {
            assert_eq!(listing.entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["src"]);
            assert!(!listing.roots.is_empty());
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(call(&mut socket, 5, Method::ProviderDetectRuntime).await, Outcome::Ok(ResponseBody::RuntimeDetected { .. })));
    assert!(matches!(call(&mut socket, 6, Method::BackupList).await, Outcome::Ok(ResponseBody::Backups(_))));
    assert!(matches!(
        host(&mut socket, 7, HostWrite::BackupCreate).await,
        Outcome::Ok(ResponseBody::Receipt(r)) if matches!(r.result, WriteResult::Host(HostResult::Backups(_)))
    ));

    // Revoking an owner closes its stream like a phone's (R04).
    desktop.remote(HostWrite::RemoteRevoke { client_id: ClientId::parse(&mac.client_id).unwrap() }).await.unwrap();
    assert_eq!(closed_with(&mut socket).await, Some(4401));

    stop(dir.path(), daemon).await;
}

async fn ask_pairing(http: &reqwest::Client, base: &str, code: &str, name: &str, platform: &str, role: Option<&str>) -> reqwest::Response {
    http.post(format!("{base}/v1/pair"))
        .json(&serde_json::json!({ "code": code, "device_name": name, "platform": platform, "role": role }))
        .send()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn full_access_needs_both_the_request_and_the_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let desktop = connect(dir.path()).await;
    let status = desktop.remote(HostWrite::RemoteEnable).await.unwrap();
    let base = format!("http://127.0.0.1:{}", status.port);
    let http = reqwest::Client::new();

    // A phone cannot ask for full access.
    let code = desktop.pairing_start().await.unwrap();
    let phone = ask_pairing(&http, &base, &code.code, "phone", "ios", Some("owner")).await;
    assert_eq!(phone.status(), reqwest::StatusCode::BAD_REQUEST);
    let parsed = PairingPayload::from_text(&code.pairing_text).unwrap();
    assert_eq!(parsed.code, code.code);
    assert!(parsed.url.starts_with("http://127.0.0.1:"), "a computer in dev uses loopback: {}", parsed.url);

    // A computer that asks is not allowed without the confirmation...
    let code = desktop.pairing_start().await.unwrap();
    let pending: serde_json::Value = ask_pairing(&http, &base, &code.code, "pc", "windows", Some("owner")).await.json().await.unwrap();
    let pairing_id = pending["pairing_id"].as_str().unwrap().to_owned();
    let refused = desktop.remote(HostWrite::RemoteDecidePairing { pairing_id: pairing_id.clone(), approve: true, owner: false }).await;
    assert!(refused.is_err(), "full access must be confirmed");
    assert!(desktop.remote_status().await.unwrap().clients.is_empty());
    // ...and with it, it becomes an owner.
    let status = desktop.remote(HostWrite::RemoteDecidePairing { pairing_id, approve: true, owner: true }).await.unwrap();
    assert!(status.clients[0].capabilities.contains(&Capability::Owner));

    // Confirming "owner" for a phone that did not ask gives it phone rights only.
    let (_, status) = pair_device(&desktop, "pixel", "android", None, true).await;
    let pixel = status.clients.iter().find(|c| c.name == "pixel").unwrap();
    assert!(!pixel.capabilities.contains(&Capability::Owner));

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_that_is_not_an_owner_may_not_shut_the_pc_down() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let desktop = connect(dir.path()).await;
    let (phone, _) = pair(&desktop).await;
    let mut socket = phone.socket(&phone.session().await.unwrap()).await.unwrap();
    hello(&mut socket).await;
    assert!(forbidden(host(&mut socket, 2, HostWrite::PowerShutdown { when: ShutdownWhen::Now }).await));
    assert!(forbidden(host(&mut socket, 3, HostWrite::PowerCancel).await));
    assert!(forbidden(call(&mut socket, 4, Method::PowerGet).await));
    assert_eq!(desktop.query_power().await.unwrap().pending, None, "nothing was requested");

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_paired_as_owner_may_shut_the_pc_down() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let desktop = connect(dir.path()).await;
    let (owner, _) = pair_device(&desktop, "macbook", "macos", Some("owner"), true).await;
    let mut socket = owner.socket(&owner.session().await.unwrap()).await.unwrap();
    hello(&mut socket).await;
    let power = |outcome: Outcome| match outcome {
        Outcome::Ok(ResponseBody::Receipt(r)) => match r.result {
            WriteResult::Host(HostResult::Power(state)) => state,
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    };
    let state = power(host(&mut socket, 2, HostWrite::PowerShutdown { when: ShutdownWhen::WhenIdle }).await);
    assert!(state.pending.is_some());
    let state = power(host(&mut socket, 3, HostWrite::PowerCancel).await);
    assert_eq!(state.pending, None);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn every_subscriber_gets_power_changed() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let desktop = connect(dir.path()).await;
    let (phone, _) = pair(&desktop).await;

    // A phone (not an owner) subscribes: the push reaches it like any other session.
    let mut socket = phone.socket(&phone.session().await.unwrap()).await.unwrap();
    hello(&mut socket).await;
    let after_seq = desktop.health().await.unwrap().last_seq;
    assert!(matches!(call(&mut socket, 2, Method::EventSubscribe { after_seq }).await, Outcome::Ok(ResponseBody::Subscribed { .. })));

    desktop.power_shutdown(ShutdownWhen::Now).await.unwrap();
    loop {
        if let ServerMessage::PowerChanged(state) = next(&mut socket).await {
            assert!(state.pending.is_some());
            break;
        }
    }
    desktop.power_cancel().await.unwrap();
    loop {
        if let ServerMessage::PowerChanged(state) = next(&mut socket).await {
            assert_eq!(state.pending, None);
            break;
        }
    }
    stop(dir.path(), daemon).await;
}

/// MU3-03 AC1: a sign-in on a phone is a session of that paired device (and the
/// device remembers its board member); revoking the device ends the session too.
#[tokio::test(flavor = "multi_thread")]
async fn a_phones_session_belongs_to_the_phone_and_ends_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let desktop = connect(dir.path()).await;
    let (phone, _) = pair(&desktop).await;
    let mut socket = phone.socket(&phone.session().await.unwrap()).await.unwrap();
    hello(&mut socket).await;
    let alex = desktop.user_whoami().await.unwrap().user.unwrap().id;
    let db = db(dir.path());
    let (client_id, device): (Option<String>, String) =
        db.query_row("SELECT client_id, device_name FROM user_sessions WHERE device_name = 'Phone' AND revoked_at_ms IS NULL", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((client_id.as_deref(), device.as_str()), (Some(phone.client_id.as_str()), "Phone"));
    let user: Option<String> = db.query_row("SELECT user_id FROM remote_clients WHERE id = ?1", [&phone.client_id], |r| r.get(0)).unwrap();
    assert_eq!(user, Some(alex.to_string()));
    // Its session is the phone's: the desktop lists it with the device.
    let listed = desktop.sessions(None).await.unwrap().sessions;
    assert!(listed.iter().any(|s| s.client_id.map(|c| c.to_string()) == Some(phone.client_id.clone())));

    desktop.remote(HostWrite::RemoteRevoke { client_id: ClientId::parse(&phone.client_id).unwrap() }).await.unwrap();
    assert_eq!(closed_with(&mut socket).await, Some(4401));
    let open: i64 = db.query_row("SELECT COUNT(*) FROM user_sessions WHERE client_id = ?1 AND revoked_at_ms IS NULL", [&phone.client_id], |r| r.get(0)).unwrap();
    assert_eq!(open, 0, "the device's sessions ended with it");
    assert!(!desktop.sessions(None).await.unwrap().sessions.iter().any(|s| s.client_id.is_some()));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_pairing_and_its_poll_give_the_chosen_name_and_only_an_owner_renames_remotely() {
    use aurelius_domain::server::SetServerName;
    let rename = |name: &str| Command::SetServerName(SetServerName { name: name.into() });
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_remote(dir.path()).await;
    let desktop = connect(dir.path()).await;
    desktop.execute(envelope(rename("Kantoor"), None)).await.unwrap();

    // The QR code, the pairing text and the poll's answer (pair_device checks the poll) name it.
    desktop.remote(HostWrite::RemoteEnable).await.unwrap();
    let code = desktop.pairing_start().await.unwrap();
    assert_eq!(serde_json::from_str::<PairingPayload>(&code.qr).unwrap().host_name, "Kantoor");
    assert_eq!(PairingPayload::from_text(&code.pairing_text).unwrap().host_name, "Kantoor");
    let (phone, _) = pair(&desktop).await;
    let mut socket = phone.socket(&phone.session().await.unwrap()).await.unwrap();
    let hello_phone = hello(&mut socket).await;
    assert_eq!(hello_phone.host_name, "Kantoor");
    assert!(!hello_phone.capabilities.contains(&Capability::Owner), "a phone: the client hides the button");

    // A phone reads the name but cannot change it (A-04).
    assert!(matches!(call(&mut socket, 2, Method::ServerGetName).await, Outcome::Ok(ResponseBody::ServerName(v)) if v.name == "Kantoor"));
    assert!(forbidden(command(&mut socket, 3, rename("Telefoon")).await));

    // A computer with full access can; an empty name brings the computer's name back.
    let (mac, _) = pair_device(&desktop, "macbook", "macos", Some("owner"), true).await;
    let mut owner = mac.socket(&mac.session().await.unwrap()).await.unwrap();
    hello(&mut owner).await;
    assert_eq!(done(command(&mut owner, 2, rename("Thuis")).await), CommandResult::ServerNameSet { name: "Thuis".into(), version: 2 });
    let code = desktop.pairing_start().await.unwrap();
    assert_eq!(PairingPayload::from_text(&code.pairing_text).unwrap().host_name, "Thuis");
    assert!(accepted(&command(&mut owner, 3, rename("")).await));
    let code = desktop.pairing_start().await.unwrap();
    assert_eq!(PairingPayload::from_text(&code.pairing_text).unwrap().host_name, aurelius_host::host_name());

    // The poll of a new pairing gives the name of that moment.
    desktop.execute(envelope(rename("Na de koppeling"), None)).await.unwrap();
    let status = desktop.remote_status().await.unwrap();
    let base = format!("http://127.0.0.1:{}", status.port);
    let http = reqwest::Client::new();
    let code = desktop.pairing_start().await.unwrap();
    let pending: serde_json::Value = ask_pairing(&http, &base, &code.code, "pixel", "android", None).await.json().await.unwrap();
    let pairing_id = pending["pairing_id"].as_str().unwrap().to_owned();
    desktop.remote(HostWrite::RemoteDecidePairing { pairing_id: pairing_id.clone(), approve: true, owner: false }).await.unwrap();
    let approved: serde_json::Value = http.get(format!("{base}/v1/pair/{pairing_id}")).send().await.unwrap().json().await.unwrap();
    assert_eq!(approved["host_name"], "Na de koppeling");

    stop(dir.path(), daemon).await;
}
