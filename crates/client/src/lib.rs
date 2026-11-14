//! Client side of protocol 2.0. Owns no state of truth: it sends reads and
//! writes and gets snapshots + typed events. The same `Client` talks to the
//! service on this computer (named pipe) or to a server elsewhere over the
//! gateway's WebSocket; see `Target`. The handshake checks the
//! protocol major only.

pub mod bridge;
pub mod remote;
pub mod servers;
mod stream;
mod transport;

use aurelius_domain::{Command, CommandId, CommandEnvelope, CommandReceipt};
use aurelius_host::ipc::Endpoint;
use aurelius_protocol::{
    Call, ErrorCode, Event, HealthResult, HelloParams, HelloResult, HostResult, HostWrite, JobsSnapshot, MessagesSnapshot,
    Method, Outcome, PROTOCOL_MAJOR, ProjectsSnapshot, ProtocolError, Receipt, Request, ResponseBody, ServerMessage, StreamDelta, WriteMethod,
    WriteResult,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

pub use remote::RemoteServer;

pub const DAEMON_BINARY: &str = "aurelius-daemon";

/// Pick the expected variant of a read answer.
macro_rules! body {
    ($variant:ident) => {
        |b| match b {
            ResponseBody::$variant(v) => Some(v),
            _ => None,
        }
    };
}

/// Pick the expected variant of a host write's answer.
macro_rules! done {
    ($variant:ident) => {
        |r| match r {
            HostResult::$variant(v) => Some(v),
            _ => None,
        }
    };
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("the Aurelius background service is not reachable: {0}")]
    Unreachable(String),
    #[error("the connection to the background service was lost")]
    Disconnected,
    /// No answer within the request limit; the session is dropped and reconnected.
    #[error("the server did not answer within {0} seconds; reconnecting")]
    TimedOut(u64),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("unexpected response from the background service")]
    UnexpectedResponse,
    /// Revoked, never paired, or paired without full access; reconnecting won't help.
    #[error("{0}")]
    Unauthorized(String),
    /// The address answered as another Aurelius server than the one paired.
    #[error("{0}")]
    WrongServer(String),
    /// Something on this computer (a file to upload, a pairing text).
    #[error("{0}")]
    Local(String),
    /// The app and the server speak another protocol major: one of them must be
    /// updated; reconnecting won't help. The text starts with "Update needed".
    #[error("{0}")]
    UpdateNeeded(String),
}

impl ClientError {
    pub fn is_disconnect(&self) -> bool {
        matches!(self, ClientError::Disconnected | ClientError::TimedOut(_) | ClientError::Unreachable(_))
    }

    /// Trying again later can't fix it: pair again or pick another server.
    pub fn is_final(&self) -> bool {
        matches!(self, ClientError::Unauthorized(_) | ClientError::WrongServer(_) | ClientError::UpdateNeeded(_))
    }
}

/// Which server a client talks to.
#[derive(Debug, Clone)]
pub enum Target {
    /// The service on this computer for data folder `root`; `spawn` starts it
    /// when it isn't running.
    Local { root: PathBuf, spawn: bool },
    /// A server on another computer, paired as owner.
    Remote(RemoteServer),
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Outcome>>>>;

/// Where `power_changed` pushes go (see `Client::power_changes`).
type PowerRoute = Arc<Mutex<Option<mpsc::UnboundedSender<aurelius_protocol::PowerState>>>>;

/// Where `load_changed` pushes go (see `Client::load_changes`).
type LoadRoute = Arc<Mutex<Option<mpsc::UnboundedSender<aurelius_protocol::LoadSnapshot>>>>;

/// What the server says about this connection's own session (protocol 5).
#[derive(Debug, Clone, PartialEq)]
pub enum SessionPush {
    /// What `claude auth login` asks, for the link this connection started.
    ClaudeLoginPrompt(aurelius_protocol::ClaudeLoginPrompt),
    /// The session ended; the server closes the connection right after.
    SignedOut(aurelius_domain::user::SignOutReason),
}

/// Where session pushes go (see `Client::session_pushes`).
type SessionRoute = Arc<Mutex<Option<mpsc::UnboundedSender<SessionPush>>>>;

/// A remote connection's HTTP side (uploads), with its session token.
struct RemoteLink {
    http: reqwest::Client,
    server: RemoteServer,
    token: tokio::sync::Mutex<String>,
}

pub struct Client {
    outgoing: mpsc::Sender<Request>,
    pending: Pending,
    next_id: AtomicU64,
    pub hello: HelloResult,
    remote: Option<RemoteLink>,
    revoked: Arc<AtomicBool>,
    /// The server closed the connection (a call after that fails at once).
    closed: Arc<AtomicBool>,
    loads: LoadRoute,
    powers: PowerRoute,
    sessions: SessionRoute,
    /// The client features of the hello (`aurelius_protocol::client_feature`),
    /// said again when the client says hello again (`identify`).
    accepts: Vec<String>,
}

/// The client features a hello names: `activity_changes` and `server_name`
/// always (this crate reads every event kind it was built with), plus `extra`.
fn accepted(extra: &[&str]) -> Vec<String> {
    let mut accepts = vec![aurelius_protocol::client_feature::ACTIVITY_CHANGES.to_owned(), aurelius_protocol::client_feature::SERVER_NAME.to_owned()];
    accepts.extend(extra.iter().filter(|f| !accepts.iter().any(|a| a == *f)).map(|f| (*f).to_owned()).collect::<Vec<_>>());
    accepts
}

fn empty_hello() -> HelloResult {
    HelloResult {
        host_id: Default::default(),
        daemon_version: String::new(),
        protocol: PROTOCOL_MAJOR,
        environment: String::new(),
        host_name: String::new(),
        os: String::new(),
        remote: false,
        client_id: None,
        capabilities: vec![],
        features: vec![],
        board_members_exist: false,
        user: None,
        must_change_password: false,
        claude_login: None,
    }
}

/// How long a call may take before the connection counts as gone, local or
/// remote: one stuck answer would otherwise block the caller's loop forever.
/// The slow ones (downloads, backups, the credential store, waiting for the
/// user) have no limit.
fn request_timeout(call: &Call) -> Option<Duration> {
    let slow = match call {
        Call::Read(Method::SubscriptionGet { .. }) => true,
        Call::Read(_) => false,
        Call::Write(write) => matches!(
            write.name().as_str(),
            "update.prepare" | "update.apply" | "update.prepare_app" | "update.apply_app" | "server.restart" | "backup.create" | "backup.restore" | "backup.resume" | "secret.set" | "project.export" | "plugin.preview" | "plugin.install" | "plugin.update"
        ),
    };
    (!slow).then_some(Duration::from_secs(60))
}

/// The server speaks another protocol major: nothing else can work.
/// The server speaks another major (`server`, when its hello said so; else it
/// refused ours with `incompatible_protocol` and `message`).
fn incompatible(server: Option<u16>, message: Option<&str>) -> ClientError {
    let text = match (server, message) {
        (Some(p), _) if p > PROTOCOL_MAJOR => format!("Update needed: the server speaks protocol {p}, this app {PROTOCOL_MAJOR}. Update the app."),
        (Some(p), _) => format!("Update needed: the server speaks protocol {p}, this app {PROTOCOL_MAJOR}. Update the server."),
        (None, Some(m)) if m.starts_with("Update needed") => m.to_string(),
        (None, _) => format!("Update needed: the server does not speak protocol {PROTOCOL_MAJOR}. Update the app or the server."),
    };
    ClientError::UpdateNeeded(text)
}

impl Client {
    /// Connect to the server in `target` and handshake. `accepts`: the client
    /// features it names in its hello besides `activity_changes`
    /// (`aurelius_protocol::client_feature`, e.g. `TEAM_CHANNELS` for the live
    /// messages of the projects' channels).
    pub async fn open(
        target: &Target,
        client_name: &str,
        events: mpsc::UnboundedSender<Event>,
        streams: Option<mpsc::UnboundedSender<StreamDelta>>,
        accepts: &[&str],
    ) -> Result<Self, ClientError> {
        let accepts = accepted(accepts);
        match target {
            Target::Local { root, spawn: true } => spawn_and_connect(root, client_name, events, streams, accepts).await,
            Target::Local { root, spawn: false } => Self::start(transport::pipe(&aurelius_host::endpoint(root)).await?, None, client_name, events, streams, accepts).await,
            Target::Remote(server) => Self::open_remote(server, client_name, events, streams, accepts).await,
        }
    }

    /// Connect and handshake. Committed events are delivered to `events`.
    pub async fn connect(endpoint: &Endpoint, client_name: &str, events: mpsc::UnboundedSender<Event>) -> Result<Self, ClientError> {
        Self::connect_with_streams(endpoint, client_name, events, None).await
    }

    /// Like `connect`, also delivering ephemeral text deltas to `streams`.
    pub async fn connect_with_streams(
        endpoint: &Endpoint,
        client_name: &str,
        events: mpsc::UnboundedSender<Event>,
        streams: Option<mpsc::UnboundedSender<StreamDelta>>,
    ) -> Result<Self, ClientError> {
        let duplex = transport::pipe(endpoint).await?;
        Self::start(duplex, None, client_name, events, streams, accepted(&[])).await
    }

    /// A server on another computer: a session token, then the WebSocket.
    /// Refuses a server that isn't the paired one, and a pairing without
    /// full access (a desktop needs it).
    pub async fn connect_remote(
        server: &RemoteServer,
        client_name: &str,
        events: mpsc::UnboundedSender<Event>,
        streams: Option<mpsc::UnboundedSender<StreamDelta>>,
    ) -> Result<Self, ClientError> {
        Self::open_remote(server, client_name, events, streams, accepted(&[])).await
    }

    async fn open_remote(
        server: &RemoteServer,
        client_name: &str,
        events: mpsc::UnboundedSender<Event>,
        streams: Option<mpsc::UnboundedSender<StreamDelta>>,
        accepts: Vec<String>,
    ) -> Result<Self, ClientError> {
        let http = remote::http();
        let token = remote::session(&http, server).await?;
        let duplex = transport::websocket(&server.url, &token).await?;
        let link = RemoteLink { http, server: server.clone(), token: tokio::sync::Mutex::new(token) };
        let client = Self::start(duplex, Some(link), client_name, events, streams, accepts).await?;
        if client.hello.host_id != server.host_id {
            return Err(ClientError::WrongServer(format!("{} now answers as another Aurelius server. Pair this computer again.", server.url)));
        }
        if !client.hello.capabilities.contains(&aurelius_protocol::remote::Capability::Owner) {
            return Err(ClientError::Unauthorized(format!(
                "This computer is paired with {} without full access. Pair it again and choose \"Allow as owner\".",
                server.host_name
            )));
        }
        Ok(client)
    }

    async fn start(
        duplex: transport::Duplex,
        remote: Option<RemoteLink>,
        client_name: &str,
        events: mpsc::UnboundedSender<Event>,
        streams: Option<mpsc::UnboundedSender<StreamDelta>>,
        accepts: Vec<String>,
    ) -> Result<Self, ClientError> {
        Self::start_as(duplex, remote, client_name, events, streams, accepts).await
    }

    async fn start_as(
        duplex: transport::Duplex,
        remote: Option<RemoteLink>,
        client_name: &str,
        events: mpsc::UnboundedSender<Event>,
        streams: Option<mpsc::UnboundedSender<StreamDelta>>,
        accepts: Vec<String>,
    ) -> Result<Self, ClientError> {
        let transport::Duplex { outgoing, mut incoming, revoked } = duplex;
        let pending: Pending = Arc::default();
        let pending_reader = pending.clone();
        let closed: Arc<AtomicBool> = Arc::default();
        let closed_reader = closed.clone();
        let loads: LoadRoute = Arc::default();
        let load_route = loads.clone();
        let powers: PowerRoute = Arc::default();
        let power_route = powers.clone();
        let sessions: SessionRoute = Arc::default();
        let session_route = sessions.clone();
        tokio::spawn(async move {
            let mut rebuild = stream::Rebuild::default();
            let mut calls = stream::Calls::default();
            while let Some(message) = incoming.recv().await {
                match message {
                    ServerMessage::Response { request_id, outcome } => {
                        if let Some(waiter) = pending_reader.lock().unwrap().remove(&request_id) {
                            let _ = waiter.send(outcome);
                        }
                    }
                    ServerMessage::Event(event) => {
                        rebuild.on_event(&event);
                        calls.on_event(&event);
                        let _ = events.send(event);
                    }
                    // Every call of the message again, from the changed ones.
                    ServerMessage::Stream(delta) => {
                        if let Some(streams) = &streams {
                            let _ = streams.send(calls.apply(delta));
                        }
                    }
                    // Remote connections: the full text again, as over IPC.
                    ServerMessage::StreamAppend(append) => {
                        if let (Some(streams), Some(delta)) = (&streams, rebuild.apply(append)) {
                            let _ = streams.send(calls.apply(delta));
                        }
                    }
                    ServerMessage::LoadChanged(snapshot) => {
                        if let Some(tx) = load_route.lock().unwrap().as_ref() {
                            let _ = tx.send(snapshot);
                        }
                    }
                    ServerMessage::PowerChanged(state) => {
                        if let Some(tx) = power_route.lock().unwrap().as_ref() {
                            let _ = tx.send(state);
                        }
                    }
                    ServerMessage::ClaudeLoginPrompt(prompt) => {
                        if let Some(tx) = session_route.lock().unwrap().as_ref() {
                            let _ = tx.send(SessionPush::ClaudeLoginPrompt(prompt));
                        }
                    }
                    ServerMessage::SignedOut { reason } => {
                        if let Some(tx) = session_route.lock().unwrap().as_ref() {
                            let _ = tx.send(SessionPush::SignedOut(reason));
                        }
                    }
                }
            }
            // Connection closed: wake all waiters with a disconnect.
            closed_reader.store(true, Ordering::SeqCst);
            pending_reader.lock().unwrap().clear();
        });

        let mut client = Self { outgoing, pending, next_id: AtomicU64::new(1), hello: empty_hello(), remote, revoked, closed, loads, powers, sessions, accepts: accepts.clone() };
        let hello = client
            .call(Method::SessionHello(HelloParams { client_name: client_name.into(), client_version: env!("CARGO_PKG_VERSION").into(), accepts, session_token: None }))
            .await;
        match hello {
            Ok(ResponseBody::Hello(result)) => {
                if result.protocol != PROTOCOL_MAJOR {
                    return Err(incompatible(Some(result.protocol), None));
                }
                let mine = aurelius_host::Environment::current().as_str();
                if !result.environment.is_empty() && result.environment != mine {
                    return Err(ClientError::Unreachable(format!(
                        "this background service runs in {}, this app in {mine}; dev and production never mix",
                        result.environment
                    )));
                }
                client.hello = result;
            }
            Ok(_) => return Err(ClientError::UnexpectedResponse),
            Err(ClientError::Protocol(err)) if err.code == ErrorCode::IncompatibleProtocol => return Err(incompatible(None, Some(&err.message))),
            Err(err) => return Err(err),
        }
        Ok(client)
    }

    /// Connected to a server on another computer.
    pub fn is_remote(&self) -> bool {
        self.remote.is_some()
    }

    /// A file on this computer, as a path the server can read: as is over
    /// local IPC; uploaded into this device's inbox on a remote server.
    pub async fn stage_file(&self, local: &Path) -> Result<String, ClientError> {
        let Some(link) = &self.remote else { return Ok(local.display().to_string()) };
        let token = link.token.lock().await.clone();
        match remote::upload(&link.http, &link.server, &token, local).await? {
            Ok(path) => Ok(path),
            Err(_unauthorized) => {
                // The 12-hour session ran out while connected; renew once.
                let fresh = remote::session(&link.http, &link.server).await?;
                *link.token.lock().await = fresh.clone();
                match remote::upload(&link.http, &link.server, &fresh, local).await? {
                    Ok(path) => Ok(path),
                    Err(_) => Err(ClientError::Unauthorized("The server no longer accepts this computer. Pair it again.".into())),
                }
            }
        }
    }

    async fn send(&self, mut request: Request) -> Result<ResponseBody, ClientError> {
        let request_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        request.request_id = request_id;
        let limit = request_timeout(&request.call);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(request_id, tx);
        // Closed before the waiter was in (the reader cleared the others): nobody would answer.
        if self.closed.load(Ordering::SeqCst) || self.outgoing.send(request).await.is_err() {
            self.pending.lock().unwrap().remove(&request_id);
            return Err(self.gone());
        }
        let answer = match limit {
            Some(limit) => match tokio::time::timeout(limit, rx).await {
                Ok(answer) => answer,
                Err(_) => {
                    self.pending.lock().unwrap().remove(&request_id);
                    return Err(ClientError::TimedOut(limit.as_secs()));
                }
            },
            None => rx.await,
        };
        match answer.map_err(|_| self.gone())? {
            Outcome::Ok(body) => Ok(body),
            Outcome::Error(err) => Err(err.into()),
        }
    }

    /// A read or session method.
    pub async fn call(&self, method: Method) -> Result<ResponseBody, ClientError> {
        self.send(Request::read(0, method)).await
    }

    /// A write with its idempotency key: the same key with the same write is
    /// answered from the receipt (safe to retry after a disconnect).
    pub async fn write(&self, key: CommandId, expected_version: Option<u64>, write: WriteMethod) -> Result<Receipt, ClientError> {
        let mut request = Request::write(0, key, write);
        request.expected_version = expected_version;
        match self.send(request).await? {
            ResponseBody::Receipt(receipt) => Ok(receipt),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// A domain command with its envelope; the answer as a command receipt.
    pub async fn execute(&self, envelope: CommandEnvelope) -> Result<CommandReceipt, ClientError> {
        let receipt = self.write(envelope.command_id, envelope.expected_version, WriteMethod::Domain(envelope.command)).await?;
        match receipt.result {
            WriteResult::Command(result) => Ok(CommandReceipt { command_id: receipt.idempotency_key, committed_seq: receipt.committed_seq, replayed: receipt.replayed, result }),
            WriteResult::Host(_) => Err(ClientError::UnexpectedResponse),
        }
    }

    /// A read, with the expected answer picked out.
    async fn read<T>(&self, method: Method, pick: impl FnOnce(ResponseBody) -> Option<T>) -> Result<T, ClientError> {
        pick(self.call(method).await?).ok_or(ClientError::UnexpectedResponse)
    }

    /// A host write under a fresh key, with the expected result picked out.
    async fn host<T>(&self, write: HostWrite, pick: impl FnOnce(HostResult) -> Option<T>) -> Result<T, ClientError> {
        match self.write(CommandId::new(), None, WriteMethod::Host(write)).await?.result {
            WriteResult::Host(result) => pick(result).ok_or(ClientError::UnexpectedResponse),
            WriteResult::Command(_) => Err(ClientError::UnexpectedResponse),
        }
    }

    /// A host write whose answer carries nothing to read.
    async fn host_ok(&self, write: HostWrite) -> Result<(), ClientError> {
        self.host(write, |_| Some(())).await
    }

    /// Why the connection ended: `Unauthorized` when the server removed this
    /// computer, else `Disconnected`.
    pub fn end_reason(&self) -> ClientError {
        self.gone()
    }

    /// The connection has ended (the server closed it, or it dropped).
    pub fn is_closed(&self) -> bool {
        self.outgoing.is_closed()
    }

    fn gone(&self) -> ClientError {
        if self.revoked.load(Ordering::SeqCst) {
            ClientError::Unauthorized("This computer was removed on the server (or remote access was turned off).".into())
        } else {
            ClientError::Disconnected
        }
    }

    // ---- files, folders ----

    /// Folders on the server.
    pub async fn list_dir(&self, path: Option<String>) -> Result<aurelius_protocol::DirListing, ClientError> {
        self.read(Method::FsListDir { path }, body!(DirListing)).await
    }

    /// A new folder `name` in `parent` on the server, answered with its listing
    /// so a folder browser opens straight into it.
    pub async fn create_dir(&self, parent: String, name: String) -> Result<aurelius_protocol::DirListing, ClientError> {
        self.host(HostWrite::FsCreateDir { parent, name }, done!(DirListing)).await
    }

    /// The subagents running in a conversation's answers.
    pub async fn subagents(&self, conversation_id: aurelius_domain::ConversationId) -> Result<Vec<aurelius_protocol::AgentView>, ClientError> {
        self.read(Method::SubagentList { conversation_id }, body!(Subagents)).await
    }

    /// One tool call whole, where messages and live updates carry it shortened
    /// (`ActivityView::truncated`; feature `activity_preview`).
    pub async fn activity(&self, message_id: aurelius_domain::MessageId, call_id: &str) -> Result<aurelius_protocol::ActivityView, ClientError> {
        self.read(Method::ActivityGet { message_id, call_id: call_id.to_owned() }, body!(Activity)).await
    }

    /// The server supports an additive feature of this protocol major
    /// (`aurelius_protocol::FEATURES`), e.g. `FEATURE_TERMINAL_MULTI`.
    pub fn supports(&self, feature: &str) -> bool {
        self.hello.features.iter().any(|f| f == feature)
    }

    /// Where Claude Code is installed on the server.
    pub async fn detect_runtime(&self) -> Result<Option<String>, ClientError> {
        self.read(Method::ProviderDetectRuntime, |b| match b {
            ResponseBody::RuntimeDetected { claude_code } => Some(claude_code),
            _ => None,
        })
        .await
    }

    // ---- reads ----

    pub async fn health(&self) -> Result<HealthResult, ClientError> {
        self.read(Method::ServerGetHealth, body!(Health)).await
    }

    /// The server's name, whether a board member chose it and the computer's name (feature `server_name`).
    pub async fn server_name(&self) -> Result<aurelius_domain::server::ServerNameView, ClientError> {
        self.read(Method::ServerGetName, body!(ServerName)).await
    }

    pub async fn projects(&self) -> Result<ProjectsSnapshot, ClientError> {
        self.read(Method::ProjectList, body!(Projects)).await
    }

    pub async fn jobs(&self) -> Result<JobsSnapshot, ClientError> {
        self.read(Method::JobList, body!(Jobs)).await
    }

    pub async fn messages(&self, conversation_id: aurelius_domain::ConversationId) -> Result<MessagesSnapshot, ClientError> {
        self.read(Method::ConversationGet { conversation_id }, body!(Messages)).await
    }

    pub async fn connections(&self) -> Result<aurelius_protocol::ProvidersSnapshot, ClientError> {
        self.read(Method::ProviderList, body!(Connections)).await
    }

    /// The company's employees (not archived).
    pub async fn employees(&self) -> Result<aurelius_protocol::EmployeesSnapshot, ClientError> {
        self.read(Method::EmployeeList, body!(Employees)).await
    }

    /// An employee's persistent chat with the user (no project).
    pub async fn employee_chat(&self, employee_id: aurelius_domain::EmployeeId) -> Result<MessagesSnapshot, ClientError> {
        self.read(Method::EmployeeChat { employee_id }, body!(Messages)).await
    }

    /// The signed-in board member's chats with employees, the latest first.
    pub async fn employee_chats(&self) -> Result<aurelius_domain::views::EmployeeChatsSnapshot, ClientError> {
        self.read(Method::EmployeeChats, body!(EmployeeChats)).await
    }

    /// Messages employees scheduled: what waits, then what went out.
    pub async fn scheduled_messages(&self) -> Result<aurelius_domain::views::ScheduledMessagesSnapshot, ClientError> {
        self.read(Method::ScheduledMessageList { project_id: None }, body!(ScheduledMessages)).await
    }

    /// Who waits to pick up a ticket's thread and what is scheduled in it.
    pub async fn ticket_waiting(&self, ticket_id: aurelius_domain::TicketId) -> Result<aurelius_domain::views::ThreadWaitingSnapshot, ClientError> {
        self.read(Method::ThreadWaiting { ticket_id: Some(ticket_id), conversation_id: None }, body!(ThreadWaiting)).await
    }

    /// An employee's scratchpad.
    /// `employee.tool_options`.
    pub async fn employee_tool_options(&self) -> Result<aurelius_domain::employee_tools::ToolOptions, ClientError> {
        self.read(Method::EmployeeToolOptions, body!(EmployeeToolOptions)).await
    }

    /// `employee.start_profiles`.
    pub async fn employee_start_profiles(&self) -> Result<aurelius_domain::employee_tools::StartProfiles, ClientError> {
        self.read(Method::EmployeeStartProfiles, body!(EmployeeStartProfiles)).await
    }

    /// The model and effort a new run of each employee uses.
    pub async fn employee_runs(&self) -> Result<aurelius_domain::views::EmployeeRunsView, ClientError> {
        self.read(Method::EmployeeRunOverview, body!(EmployeeRuns)).await
    }

    pub async fn employee_scratchpad(&self, employee_id: aurelius_domain::EmployeeId) -> Result<aurelius_domain::views::ScratchpadView, ClientError> {
        self.read(Method::EmployeeScratchpad { employee_id }, body!(EmployeeScratchpad)).await
    }

    /// What is in an employee's mailbox now.
    pub async fn activity_overview(&self) -> Result<aurelius_domain::views::ActivityOverview, ClientError> {
        self.read(Method::ActivityOverview, body!(ActivityOverview)).await
    }

    pub async fn employee_queue(&self, employee_id: aurelius_domain::EmployeeId) -> Result<aurelius_domain::views::EmployeeQueue, ClientError> {
        self.read(Method::EmployeeQueue { employee_id }, body!(EmployeeQueue)).await
    }

    /// The newest rows of the activity log of a run, or of everything without one.
    pub async fn activity_log(&self, run_id: Option<aurelius_domain::RunId>, employee_id: Option<aurelius_domain::EmployeeId>) -> Result<aurelius_domain::views::ActivityLogPage, ClientError> {
        self.read(Method::ActivityLogList { project_id: None, employee_id, thread_key: None, ticket_number: None, run_id, category: None, before_seq: None, limit: None }, body!(ActivityLog)).await
    }

    /// The company's group chat (Team chat): the user and every employee.
    pub async fn group_chat(&self) -> Result<MessagesSnapshot, ClientError> {
        self.read(Method::GroupChatGet { channel: None }, body!(Messages)).await
    }

    /// One channel of the Team chat (feature `team_channels`); reading it marks it read.
    pub async fn channel_messages(&self, channel: aurelius_domain::ConversationId) -> Result<MessagesSnapshot, ClientError> {
        self.read(Method::GroupChatGet { channel: Some(channel) }, body!(Messages)).await
    }

    /// One thread of a channel: `message_id` starts it; its replies oldest
    /// first. Reading it marks it read. Only on a server that `supports`
    /// `aurelius_protocol::feature::CHAT_THREADS` (an older one refuses the method).
    pub async fn thread(&self, message_id: aurelius_domain::MessageId) -> Result<aurelius_domain::views::ThreadSnapshot, ClientError> {
        self.read(Method::ThreadRead { message_id }, body!(Thread)).await
    }

    /// Whom `@Everyone` would reach in `channel` (none: the general channel), for the
    /// count before sending. Only on a server that `supports`
    /// `aurelius_protocol::feature::EVERYONE_MENTION` (an older one refuses the method).
    pub async fn discussion_audience(&self, channel: Option<aurelius_domain::ConversationId>) -> Result<aurelius_domain::views::DiscussionAudience, ClientError> {
        self.read(Method::DiscussionAudience { channel }, body!(DiscussionAudience)).await
    }

    /// Where a message of a channel or a ticket's discussion is, for a link to it.
    /// Only on a server that `supports` `aurelius_protocol::feature::MESSAGE_LOCATE`.
    pub async fn message_locate(&self, message_id: aurelius_domain::MessageId) -> Result<aurelius_domain::views::MessageLocation, ClientError> {
        self.read(Method::MessageLocate { message_id }, body!(MessageLocation)).await
    }

    /// Who follows a ticket (`ticket.followers`, feature `ticket_follow`).
    pub async fn ticket_followers(&self, ticket_id: aurelius_domain::TicketId) -> Result<aurelius_domain::ticket::TicketFollowersView, ClientError> {
        self.read(Method::TicketFollowers { ticket_id }, body!(TicketFollowers)).await
    }

    /// The Team chat's channels (feature `team_channels`): General first, then one per project.
    pub async fn channels(&self) -> Result<aurelius_domain::views::ChannelsSnapshot, ClientError> {
        self.read(Method::ChannelList, body!(Channels)).await
    }

    /// The company in one read: employees, the org chart setting and the roots.
    pub async fn company(&self) -> Result<aurelius_protocol::CompanySnapshot, ClientError> {
        self.read(Method::CompanyGet, body!(Company)).await
    }

    pub async fn approvals(&self) -> Result<aurelius_protocol::ApprovalsSnapshot, ClientError> {
        self.read(Method::ApprovalList { project_id: None }, body!(Approvals)).await
    }

    /// Tasks of one project, or of the whole company (`None`).
    pub async fn tasks(&self, project_id: Option<aurelius_domain::ProjectId>) -> Result<aurelius_protocol::TasksSnapshot, ClientError> {
        self.read(Method::TaskList { project_id, employee_id: None, state: None }, body!(Tasks)).await
    }

    /// Tasks company-wide with every filter of `task.list`.
    pub async fn tasks_filtered(
        &self,
        project_id: Option<aurelius_domain::ProjectId>,
        employee_id: Option<aurelius_domain::EmployeeId>,
        state: Option<aurelius_protocol::method::TaskListState>,
    ) -> Result<aurelius_protocol::TasksSnapshot, ClientError> {
        self.read(Method::TaskList { project_id, employee_id, state }, body!(Tasks)).await
    }

    pub async fn timeline(&self, query: aurelius_protocol::TimelineQuery) -> Result<aurelius_protocol::TimelinePage, ClientError> {
        self.read(Method::TimelineList(query), body!(Timeline)).await
    }

    /// Global memory plus a project's (none: the whole company's).
    pub async fn memory(&self, project_id: Option<aurelius_domain::ProjectId>) -> Result<aurelius_protocol::MemorySnapshot, ClientError> {
        self.read(Method::MemoryList { project_id, employee_id: None }, body!(Memory)).await
    }

    /// Memory with every `memory.list` filter: a project's and an employee's
    /// scopes, global always.
    pub async fn memory_of(
        &self,
        project_id: Option<aurelius_domain::ProjectId>,
        employee_id: Option<aurelius_domain::EmployeeId>,
    ) -> Result<aurelius_protocol::MemorySnapshot, ClientError> {
        self.read(Method::MemoryList { project_id, employee_id }, body!(Memory)).await
    }

    /// What goes into an employee's context (in a project's task when given; else its chat).
    pub async fn memory_context(&self, project_id: Option<aurelius_domain::ProjectId>, employee_id: Option<aurelius_domain::EmployeeId>) -> Result<aurelius_protocol::MemoryContextView, ClientError> {
        self.read(Method::MemoryGetContext { project_id, employee_id }, body!(MemoryContext)).await
    }

    /// The conversations of a project.
    pub async fn project_conversations(&self, project_id: aurelius_domain::ProjectId) -> Result<aurelius_protocol::ProjectConversationsSnapshot, ClientError> {
        self.read(Method::ProjectConversationList { project_id }, body!(ProjectConversations)).await
    }

    /// A screenshot the agent took, as JPEG bytes.
    pub async fn screenshot(&self, id: &str) -> Result<Vec<u8>, ClientError> {
        use base64::Engine;
        let jpeg_base64 = self
            .read(Method::ScreenshotGet { id: id.to_owned() }, |b| match b {
                ResponseBody::Screenshot { jpeg_base64, .. } => Some(jpeg_base64),
                _ => None,
            })
            .await?;
        base64::engine::general_purpose::STANDARD.decode(jpeg_base64).map_err(|_| ClientError::UnexpectedResponse)
    }

    /// A file kept with a message: (name, media type, bytes).
    pub async fn file(&self, id: &str) -> Result<(String, String, Vec<u8>), ClientError> {
        use base64::Engine;
        let (name, media_type, data_base64) = self
            .read(Method::FileGet { id: id.to_owned() }, |b| match b {
                ResponseBody::File { name, media_type, data_base64, .. } => Some((name, media_type, data_base64)),
                _ => None,
            })
            .await?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(data_base64).map_err(|_| ClientError::UnexpectedResponse)?;
        Ok((name, media_type, bytes))
    }

    /// A file the user uploaded (`MessageFileView::id`), fetched part by part: (name, media type, bytes).
    pub async fn upload(&self, id: &str) -> Result<(String, String, Vec<u8>), ClientError> {
        use base64::Engine;
        let mut bytes = Vec::new();
        loop {
            let offset = bytes.len() as u64;
            let part = self.read(Method::UploadGet { id: id.to_owned(), offset: Some(offset), max_side: None }, body!(Upload)).await?;
            bytes.extend(base64::engine::general_purpose::STANDARD.decode(&part.data_base64).map_err(|_| ClientError::UnexpectedResponse)?);
            // An empty part before the end would loop forever.
            if part.done || bytes.len() as u64 == offset {
                return Ok((part.name, part.media_type, bytes));
            }
        }
    }

    /// An uploaded image as a JPEG that fits `max_side` pixels.
    pub async fn upload_thumbnail(&self, id: &str, max_side: u32) -> Result<Vec<u8>, ClientError> {
        use base64::Engine;
        let part = self.read(Method::UploadGet { id: id.to_owned(), offset: None, max_side: Some(max_side) }, body!(Upload)).await?;
        base64::engine::general_purpose::STANDARD.decode(part.data_base64).map_err(|_| ClientError::UnexpectedResponse)
    }

    /// The ideas of every project.
    pub async fn all_ideas(&self) -> Result<aurelius_domain::views::AllIdeasSnapshot, ClientError> {
        self.read(Method::IdeaListAll {}, body!(AllIdeas)).await
    }

    /// A project's ideas.
    pub async fn ideas(&self, project_id: aurelius_domain::ProjectId) -> Result<aurelius_protocol::IdeasSnapshot, ClientError> {
        self.read(Method::IdeaList { project_id }, body!(Ideas)).await
    }

    /// What employees proposed, company-wide.
    pub async fn proposals(&self) -> Result<aurelius_protocol::ProposalsSnapshot, ClientError> {
        self.read(Method::ProposalList, body!(Proposals)).await
    }

    /// The versions of an employee's instructions, newest first (feature `instruction_proposals`).
    pub async fn employee_instruction_versions(&self, employee_id: aurelius_domain::EmployeeId) -> Result<aurelius_protocol::EmployeeInstructionVersionsSnapshot, ClientError> {
        self.read(Method::EmployeeInstructionVersions { employee_id }, body!(EmployeeInstructionVersions)).await
    }

    /// Markdown files of a work folder.
    pub async fn markdown_files(&self, scope: aurelius_protocol::FileScope) -> Result<aurelius_protocol::MarkdownFileList, ClientError> {
        self.read(Method::MarkdownList { scope }, body!(MarkdownFiles)).await
    }

    /// One Markdown file, relative or absolute inside the work folder.
    pub async fn markdown_file(&self, scope: aurelius_protocol::FileScope, path: &str) -> Result<aurelius_protocol::MarkdownFileContent, ClientError> {
        self.read(Method::MarkdownGet { scope, path: path.to_owned() }, body!(MarkdownFile)).await
    }

    pub async fn context(&self, run_id: aurelius_domain::RunId) -> Result<aurelius_protocol::ContextView, ClientError> {
        self.read(Method::RunGetContext { run_id }, body!(Context)).await
    }

    pub async fn usage(&self) -> Result<aurelius_protocol::UsageSnapshot, ClientError> {
        self.read(Method::UsageGet, body!(Usage)).await
    }

    /// Usage per hour of the server-local day containing `day_start_ms`.
    pub async fn query_usage_hours(&self, day_start_ms: i64) -> Result<aurelius_protocol::UsageHours, ClientError> {
        self.read(Method::UsageGetHours { day_start_ms }, body!(UsageHours)).await
    }

    /// What a conversation or a task used, with the tasks given away from it apart (feature `run_usage`).
    pub async fn usage_totals(&self, conversation_id: Option<aurelius_domain::ConversationId>, task_id: Option<aurelius_domain::TaskId>) -> Result<aurelius_protocol::UsageTotals, ClientError> {
        self.read(Method::UsageGetTotals { conversation_id, task_id, ticket_id: None }, body!(UsageTotals)).await
    }

    /// What a run sent and what each of its model calls cost (feature `run_trace`).
    pub async fn run_trace(&self, run_id: aurelius_domain::RunId) -> Result<aurelius_protocol::RunTrace, ClientError> {
        self.read(Method::RunGetTrace { run_id }, body!(RunTrace)).await
    }

    pub async fn trace_setting(&self) -> Result<aurelius_protocol::TraceSetting, ClientError> {
        self.read(Method::TraceGetSetting, body!(TraceSetting)).await
    }

    /// Trace mode on or off for the next runs (off forgets the kept traces); needs the Approve right.
    pub async fn set_trace_enabled(&self, enabled: bool) -> Result<aurelius_protocol::TraceSetting, ClientError> {
        self.host(HostWrite::TraceSetEnabled { enabled }, done!(TraceSetting)).await
    }

    pub async fn subscription(&self, refresh: bool) -> Result<aurelius_protocol::SubscriptionUsage, ClientError> {
        self.read(Method::SubscriptionGet { refresh }, body!(Subscription)).await
    }

    pub async fn notifications(&self) -> Result<aurelius_protocol::NotificationsSnapshot, ClientError> {
        self.read(Method::NotificationList { project_id: None }, body!(Notifications)).await
    }

    /// Replay events after `after_seq`, then live events (and load/power pushes).
    pub async fn subscribe(&self, after_seq: u64) -> Result<u64, ClientError> {
        self.read(Method::EventSubscribe { after_seq }, |b| match b {
            ResponseBody::Subscribed { replayed_through_seq } => Some(replayed_through_seq),
            _ => None,
        })
        .await
    }

    // ---- board members and sessions (protocol 5) ----

    /// The board members, oldest first (disabled ones too). Without a session
    /// the list is empty (Titan, SEC1 m9): use `board_members_exist` from the
    /// hello or `user_whoami` for the sign-in screen.
    pub async fn user_list(&self) -> Result<aurelius_domain::user::UsersSnapshot, ClientError> {
        self.read(Method::UserList, body!(Users)).await
    }

    /// Whether the server has board members, and this connection's session's one.
    pub async fn user_whoami(&self) -> Result<aurelius_domain::user::WhoAmI, ClientError> {
        self.read(Method::UserWhoami, body!(WhoAmI)).await
    }

    /// A new board member. Without a session only the first one (the onboarding),
    /// and this connection then has its session. Errors: `invalid_username`,
    /// `weak_password`, `username_taken`, `too_many_attempts`, `unauthenticated`.
    pub async fn user_create(&self, username: &str, password: &str) -> Result<CommandReceipt, ClientError> {
        let params = aurelius_protocol::UserCreateParams { username: username.into(), password: password.into() };
        let receipt = self.write(CommandId::new(), None, WriteMethod::Host(HostWrite::UserCreate(params))).await?;
        match receipt.result {
            WriteResult::Command(result) => Ok(CommandReceipt { command_id: receipt.idempotency_key, committed_seq: receipt.committed_seq, replayed: receipt.replayed, result }),
            WriteResult::Host(_) => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Sign in: this connection gets a session; keep `session_token` for the
    /// server and give it to `resume` on the next connection. Errors:
    /// `wrong_password` (username or password), `too_many_attempts` (with `retry_after_ms`).
    pub async fn user_login(&self, username: &str, password: &str, device_name: &str) -> Result<aurelius_domain::user::LoginResult, ClientError> {
        let params = aurelius_protocol::LoginParams { username: username.into(), password: password.into(), device_name: device_name.into() };
        self.read(Method::UserLogin(params), body!(Login)).await
    }

    /// Say hello again with the session this device kept for the server: its
    /// board member is this connection's when it is still valid (`HelloResult::user`),
    /// else none (sign in again).
    pub async fn resume(&self, session_token: &str) -> Result<HelloResult, ClientError> {
        let params = HelloParams { client_name: "aurelius".into(), client_version: env!("CARGO_PKG_VERSION").into(), accepts: self.accepts.clone(), session_token: Some(session_token.into()) };
        self.read(Method::SessionHello(params), body!(Hello)).await
    }

    /// End this device's session; forget its token.
    pub async fn user_logout(&self) -> Result<aurelius_domain::user::WhoAmI, ClientError> {
        self.read(Method::UserLogout, body!(WhoAmI)).await
    }

    /// The signed-in devices of a board member (none: the own).
    pub async fn sessions(&self, user_id: Option<aurelius_domain::UserId>) -> Result<aurelius_domain::user::SessionsSnapshot, ClientError> {
        self.read(Method::SessionList { user_id }, body!(Sessions)).await
    }

    /// End a session (any board member's); its device is signed out at once.
    pub async fn revoke_session(&self, session_id: aurelius_domain::SessionId) -> Result<(), ClientError> {
        self.host_ok(HostWrite::SessionRevoke { session_id }).await
    }

    /// The own password (with `old_password`; the other sessions end), or a
    /// temporary one for another board member (all their sessions end).
    pub async fn set_password(&self, user_id: Option<aurelius_domain::UserId>, old_password: Option<&str>, new_password: &str) -> Result<(), ClientError> {
        let params = aurelius_protocol::SetPasswordParams { user_id, old_password: old_password.map(str::to_owned), new_password: new_password.into() };
        self.host_ok(HostWrite::UserSetPassword(params)).await
    }

    /// Disable a board member (never the last active one: `last_board_member`).
    pub async fn disable_user(&self, user_id: aurelius_domain::UserId) -> Result<aurelius_domain::user::UserView, ClientError> {
        self.host(HostWrite::UserDisable { user_id }, done!(User)).await
    }

    // ---- the own Claude login (protocol 5) ----

    /// The session's own Claude login.
    pub async fn claude_login(&self) -> Result<aurelius_domain::user::ClaudeLoginView, ClientError> {
        self.read(Method::ClaudeLoginStatus, body!(ClaudeLogin)).await
    }

    /// Start connecting the own Claude login; its prompts come as
    /// `SessionPush::ClaudeLoginPrompt` (see `session_pushes`). Answers the link id.
    pub async fn claude_login_link(&self) -> Result<String, ClientError> {
        self.host(HostWrite::ClaudeLoginLink, |r| match r {
            HostResult::ClaudeLoginLink { link_id } => Some(link_id),
            _ => None,
        })
        .await
    }

    /// The code Claude showed, for the running link.
    pub async fn claude_login_input(&self, link_id: &str, text: &str) -> Result<aurelius_domain::user::ClaudeLoginView, ClientError> {
        let params = aurelius_protocol::ClaudeLoginInputParams { link_id: link_id.into(), text: text.into() };
        self.read(Method::ClaudeLoginInput(params), body!(ClaudeLogin)).await
    }

    pub async fn claude_login_cancel(&self, link_id: &str) -> Result<aurelius_domain::user::ClaudeLoginView, ClientError> {
        self.host(HostWrite::ClaudeLoginCancel { link_id: link_id.into() }, done!(ClaudeLogin)).await
    }

    pub async fn claude_login_unlink(&self) -> Result<aurelius_domain::user::ClaudeLoginView, ClientError> {
        self.host(HostWrite::ClaudeLoginUnlink, done!(ClaudeLogin)).await
    }

    // ---- the vault (feature `vault`) ----

    /// The company's items (none) or a project's, with the company's apart.
    pub async fn vault_list(&self, project_id: Option<aurelius_domain::ProjectId>) -> Result<aurelius_domain::vault::VaultList, ClientError> {
        self.read(Method::VaultList { project_id }, body!(Vault)).await
    }

    /// One item's value; written to the audit log as shown or copied.
    pub async fn vault_reveal(&self, item_id: aurelius_domain::VaultItemId, purpose: aurelius_domain::vault::RevealPurpose) -> Result<aurelius_domain::vault::VaultValue, ClientError> {
        self.read(Method::VaultReveal { item_id, purpose }, body!(VaultValue)).await
    }

    /// The audit log, newest first: the company's, a project's or (`all`) every scope's.
    pub async fn vault_audit(
        &self,
        project_id: Option<aurelius_domain::ProjectId>,
        all: bool,
        item_id: Option<aurelius_domain::VaultItemId>,
        before: Option<i64>,
        limit: Option<u32>,
    ) -> Result<aurelius_domain::vault::VaultAuditPage, ClientError> {
        self.read(Method::VaultAudit { project_id, all, item_id, before, limit }, body!(VaultAudit)).await
    }

    /// The server's vault key, to keep apart, for the signed-in board member's
    /// password; written to the audit log.
    pub async fn vault_key_export(&self, password: String) -> Result<aurelius_domain::vault::VaultKeyExport, ClientError> {
        self.read(Method::VaultKeyExport(aurelius_protocol::KeyExportParams { password }), body!(VaultKey)).await
    }

    pub async fn vault_create(&self, params: aurelius_protocol::VaultCreateParams) -> Result<aurelius_domain::vault::VaultItemView, ClientError> {
        self.host(HostWrite::VaultCreate(params), done!(VaultItem)).await
    }

    /// Change an item; `vault_kind_locked` for a kind to or from secret.
    pub async fn vault_update(&self, params: aurelius_protocol::VaultUpdateParams) -> Result<aurelius_domain::vault::VaultItemView, ClientError> {
        self.host(HostWrite::VaultUpdate(params), done!(VaultItem)).await
    }

    pub async fn vault_archive(&self, item_id: aurelius_domain::VaultItemId) -> Result<aurelius_domain::vault::VaultItemView, ClientError> {
        self.host(HostWrite::VaultArchive { item_id }, done!(VaultItem)).await
    }

    // ---- the board and its tickets (protocol 4) ----

    /// The board's columns with their cards, for one project or all.
    pub async fn board(&self, project_id: Option<aurelius_domain::ProjectId>) -> Result<aurelius_domain::ticket::BoardView, ClientError> {
        self.read(Method::BoardGet { project_id }, body!(Board)).await
    }

    /// `ticket.history` (feature `ticket_history`): one page of the tickets on Done,
    /// narrowed by `method`'s filters (a `Method::TicketHistory`).
    pub async fn history(&self, method: Method) -> Result<aurelius_domain::history::HistoryPage, ClientError> {
        self.read(method, body!(History)).await
    }

    /// One ticket with its main task and linked tasks.
    pub async fn ticket(&self, ticket_id: aurelius_domain::TicketId) -> Result<aurelius_domain::ticket::TicketDetail, ClientError> {
        self.read(Method::TicketGet { ticket_id }, body!(Ticket)).await
    }

    /// Tickets in board order; every given filter applies.
    pub async fn tickets(
        &self,
        project_id: Option<aurelius_domain::ProjectId>,
        column: Option<aurelius_domain::ticket::ColumnKind>,
        query: Option<String>,
    ) -> Result<aurelius_domain::ticket::TicketsSnapshot, ClientError> {
        self.read(Method::TicketList { project_id, column, query }, body!(Tickets)).await
    }

    /// Create a ticket, with files from this computer (staged like `stage_file`).
    pub async fn create_ticket(&self, mut ticket: aurelius_domain::ticket::CreateTicket) -> Result<CommandReceipt, ClientError> {
        for path in ticket.files.iter_mut() {
            *path = self.stage_file(Path::new(path.as_str())).await?;
        }
        self.execute(envelope(Command::CreateTicket(ticket), None)).await
    }

    /// Change a ticket, adding files from this computer (staged like `stage_file`;
    /// feature `ticket_add_files`).
    pub async fn update_ticket(&self, mut ticket: aurelius_domain::ticket::UpdateTicket, expected_version: Option<u64>) -> Result<CommandReceipt, ClientError> {
        for path in ticket.add_files.iter_mut().flatten() {
            *path = self.stage_file(Path::new(path.as_str())).await?;
        }
        self.execute(envelope(Command::UpdateTicket(ticket), expected_version)).await
    }

    /// Choose who is responsible for a ticket; none: nobody (`ticket.set_responsible`,
    /// feature `ticket_responsible`).
    pub async fn set_ticket_responsible(&self, ticket_id: aurelius_domain::TicketId, responsible_user_id: Option<aurelius_domain::UserId>) -> Result<CommandReceipt, ClientError> {
        self.execute(envelope(Command::SetTicketResponsible(aurelius_domain::ticket::SetTicketResponsible { ticket_id, responsible_user_id }), None)).await
    }

    /// Start (`on`) or stop following a ticket (`ticket.follow`, feature `ticket_follow`).
    pub async fn follow_ticket(&self, ticket_id: aurelius_domain::TicketId, on: bool) -> Result<CommandReceipt, ClientError> {
        self.execute(envelope(Command::FollowTicket(aurelius_domain::ticket::FollowTicket { ticket_id, on }), None)).await
    }

    /// Any other ticket, board or company write (`Command::MoveTicket`, …) under a fresh key.
    pub async fn ticket_command(&self, command: Command, expected_version: Option<u64>) -> Result<CommandReceipt, ClientError> {
        self.execute(envelope(command, expected_version)).await
    }

    // ---- tasks, schedules, messages (domain commands, with files staged) ----

    /// Create a task, with files from this computer (staged like `stage_file`).
    pub async fn create_task(&self, mut task: aurelius_domain::task::CreateTask) -> Result<CommandReceipt, ClientError> {
        for path in task.files.iter_mut() {
            *path = self.stage_file(Path::new(path.as_str())).await?;
        }
        self.execute(envelope(Command::CreateTask(task), None)).await
    }

    /// Recurring tasks of a project (`None`: all).
    pub async fn task_schedules(&self, project_id: Option<aurelius_domain::ProjectId>) -> Result<aurelius_protocol::TaskSchedulesSnapshot, ClientError> {
        self.read(Method::RecurringTaskList { project_id }, body!(TaskSchedules)).await
    }

    /// Create a recurring task, with files from this computer.
    pub async fn create_task_schedule(&self, mut schedule: aurelius_domain::schedule::CreateTaskSchedule) -> Result<CommandReceipt, ClientError> {
        for path in schedule.files.iter_mut() {
            *path = self.stage_file(Path::new(path.as_str())).await?;
        }
        self.execute(envelope(Command::CreateTaskSchedule(schedule), None)).await
    }

    /// Change a recurring task, adding files from this computer.
    pub async fn update_task_schedule(&self, mut schedule: aurelius_domain::schedule::UpdateTaskSchedule) -> Result<CommandReceipt, ClientError> {
        for path in schedule.add_files.iter_mut() {
            *path = self.stage_file(Path::new(path.as_str())).await?;
        }
        self.execute(envelope(Command::UpdateTaskSchedule(schedule), None)).await
    }

    /// Send a message to an employee with images and files from this computer.
    pub async fn send_employee_message(&self, mut send: aurelius_domain::employee::SendEmployeeMessage) -> Result<CommandReceipt, ClientError> {
        for path in send.images.iter_mut().chain(send.files.iter_mut()) {
            *path = self.stage_file(Path::new(path.as_str())).await?;
        }
        self.execute(envelope(Command::SendEmployeeMessage(send), None)).await
    }

    /// A message in the group chat with images and files from this computer.
    pub async fn send_group_message(&self, mut send: aurelius_domain::group_chat::SendGroupMessage) -> Result<CommandReceipt, ClientError> {
        for path in send.images.iter_mut().chain(send.files.iter_mut()) {
            *path = self.stage_file(Path::new(path.as_str())).await?;
        }
        self.execute(envelope(Command::SendGroupMessage(send), None)).await
    }

    /// A message in a project conversation (a new one without `conversation_id`).
    pub async fn send_project_message(&self, mut send: aurelius_domain::project_conversation::SendProjectMessage) -> Result<CommandReceipt, ClientError> {
        for path in send.images.iter_mut().chain(send.files.iter_mut()) {
            *path = self.stage_file(Path::new(path.as_str())).await?;
        }
        self.execute(envelope(Command::SendProjectMessage(send), None)).await
    }

    /// A new, empty project conversation.
    pub async fn start_project_conversation(&self, start: aurelius_domain::project_conversation::StartProjectConversation) -> Result<CommandReceipt, ClientError> {
        self.execute(envelope(Command::StartProjectConversation(start), None)).await
    }

    /// Delete a message that still waits for its answer (`MessageView::queued`).
    pub async fn delete_queued_message(&self, conversation_id: aurelius_domain::ConversationId, message_id: aurelius_domain::MessageId) -> Result<CommandReceipt, ClientError> {
        let delete = aurelius_domain::conversation::DeleteQueuedMessage { conversation_id, message_id };
        self.execute(envelope(Command::DeleteQueuedMessage(delete), None)).await
    }

    /// Answer a waiting message now: what runs ahead of it stops.
    pub async fn send_queued_now(&self, conversation_id: aurelius_domain::ConversationId, message_id: aurelius_domain::MessageId) -> Result<CommandReceipt, ClientError> {
        let send = aurelius_domain::conversation::SendQueuedNow { conversation_id, message_id };
        self.execute(envelope(Command::SendQueuedNow(send), None)).await
    }

    // ---- load, browser, MCP ----

    /// The server's load and when new runs may start.
    pub async fn load_snapshot(&self) -> Result<aurelius_protocol::LoadSnapshot, ClientError> {
        self.read(Method::LoadGet, body!(Load)).await
    }

    /// Change the load limits (`None` leaves one as it is); the server applies
    /// them at once and keeps them. Answers with the new snapshot.
    pub async fn set_load_limits(&self, update: aurelius_protocol::LoadLimitsUpdate) -> Result<aurelius_protocol::LoadSnapshot, ClientError> {
        self.host(HostWrite::LoadSetLimits(update), done!(Load)).await
    }

    /// Which browser Claude in Chrome uses.
    pub async fn browser_setting(&self) -> Result<aurelius_protocol::BrowserSetting, ClientError> {
        self.read(Method::BrowserGet, body!(Browser)).await
    }

    /// Choose the browser; the server keeps it and uses it from the next run.
    pub async fn set_browser(&self, choice: aurelius_protocol::BrowserChoice) -> Result<aurelius_protocol::BrowserSetting, ClientError> {
        self.host(HostWrite::BrowserSet { choice }, done!(Browser)).await
    }

    /// Claude in Chrome or Playwright; the server uses it from the next run.
    pub async fn set_browser_backend(&self, backend: aurelius_protocol::BrowserBackend) -> Result<aurelius_protocol::BrowserSetting, ClientError> {
        self.host(HostWrite::BrowserSetBackend { backend }, done!(Browser)).await
    }

    /// Whether management chooses its own task mode (feature `management_chooses_mode`).
    pub async fn management_mode_setting(&self) -> Result<aurelius_protocol::ManagementModeSetting, ClientError> {
        self.read(Method::ManagementModeGetSetting, body!(ManagementModeSetting)).await
    }

    /// Turn "Management chooses its own mode" on or off for the next runs; needs the Approve right.
    pub async fn set_management_mode_enabled(&self, enabled: bool) -> Result<aurelius_protocol::ManagementModeSetting, ClientError> {
        self.host(HostWrite::ManagementModeSetEnabled { enabled }, done!(ManagementModeSetting)).await
    }

    /// Whether the safe read-only tools pass without a question, and which they are.
    pub async fn auto_allow_setting(&self) -> Result<aurelius_protocol::AutoAllowSetting, ClientError> {
        self.read(Method::AutoAllowGetSetting, body!(AutoAllowSetting)).await
    }

    /// Turn auto-allow on or off from the next tool call; needs the Approve right.
    pub async fn set_auto_allow_enabled(&self, enabled: bool) -> Result<aurelius_protocol::AutoAllowSetting, ClientError> {
        self.host(HostWrite::AutoAllowSetEnabled { enabled }, done!(AutoAllowSetting)).await
    }

    /// The user's MCP servers and which ones every run uses.
    pub async fn mcp_list(&self) -> Result<aurelius_protocol::McpServers, ClientError> {
        self.read(Method::McpList, body!(McpServers)).await
    }

    /// Replace the server-wide list of enabled MCP servers; needs the Approve right.
    pub async fn set_mcp(&self, enabled: Vec<String>) -> Result<aurelius_protocol::McpServers, ClientError> {
        self.host(HostWrite::McpSet { enabled }, done!(McpServers)).await
    }

    /// Add an MCP server to Claude Code at user scope; needs the Owner right.
    pub async fn mcp_add(&self, params: aurelius_protocol::McpAddParams) -> Result<aurelius_protocol::McpServers, ClientError> {
        self.host(HostWrite::McpAdd(params), done!(McpServers)).await
    }

    /// Remove a user-scope MCP server; needs the Owner right.
    pub async fn mcp_remove(&self, name: &str) -> Result<aurelius_protocol::McpServers, ClientError> {
        self.host(HostWrite::McpRemove { name: name.to_owned() }, done!(McpServers)).await
    }

    /// `mcp.login_start`: open `authorization_url` in a browser. On the server's
    /// computer the login finishes by itself (event `mcp.login_finished`); elsewhere
    /// send the address the browser ended on to [`Client::mcp_login_complete`].
    pub async fn mcp_login_start(&self, name: &str) -> Result<aurelius_protocol::McpLogin, ClientError> {
        self.host(HostWrite::McpLoginStart { name: name.to_owned() }, done!(McpLogin)).await
    }

    /// `mcp.login_complete` with the pasted `http://localhost:<port>/callback?code=…&state=…`.
    pub async fn mcp_login_complete(&self, login_id: &str, redirect_url: &str) -> Result<aurelius_protocol::McpLogin, ClientError> {
        self.host(HostWrite::McpLoginComplete { login_id: login_id.to_owned(), redirect_url: redirect_url.to_owned() }, done!(McpLogin)).await
    }

    /// `mcp.login_get`.
    pub async fn mcp_login_get(&self, login_id: &str) -> Result<aurelius_protocol::McpLogin, ClientError> {
        self.read(Method::McpLoginGet { login_id: login_id.to_owned() }, body!(McpLogin)).await
    }

    /// `mcp.logout`: forget the server's tokens; answers like `mcp.list`.
    pub async fn mcp_logout(&self, name: &str) -> Result<aurelius_protocol::McpServers, ClientError> {
        self.host(HostWrite::McpLogout { name: name.to_owned() }, done!(McpServers)).await
    }

    /// The MCP tools the user always allows (filled by approving with 'Always allow').
    pub async fn mcp_always_allow(&self) -> Result<aurelius_protocol::McpAlwaysAllow, ClientError> {
        self.read(Method::McpAlwaysAllowList, body!(McpAlwaysAllow)).await
    }

    /// Take a tool off the always-allow list; needs the Approve right.
    pub async fn mcp_always_allow_remove(&self, tool: &str) -> Result<aurelius_protocol::McpAlwaysAllow, ClientError> {
        self.host(HostWrite::McpAlwaysAllowRemove { tool: tool.to_owned() }, done!(McpAlwaysAllow)).await
    }

    // ---- pushes ----

    /// `load_changed` pushes from now on (after `subscribe`; a new call
    /// replaces the previous receiver).
    pub fn load_changes(&self) -> mpsc::UnboundedReceiver<aurelius_protocol::LoadSnapshot> {
        let (tx, rx) = mpsc::unbounded_channel();
        *self.loads.lock().unwrap() = Some(tx);
        rx
    }

    /// What the server says about this connection's session (a Claude login's
    /// prompts, a sign-out) from now on (a new call replaces the previous receiver).
    pub fn session_pushes(&self) -> mpsc::UnboundedReceiver<SessionPush> {
        let (tx, rx) = mpsc::unbounded_channel();
        *self.sessions.lock().unwrap() = Some(tx);
        rx
    }

    /// `power_changed` pushes from now on (a new call replaces the previous receiver).
    pub fn power_changes(&self) -> mpsc::UnboundedReceiver<aurelius_protocol::PowerState> {
        let (tx, rx) = mpsc::unbounded_channel();
        *self.powers.lock().unwrap() = Some(tx);
        rx
    }

    // ---- power ----

    /// Shut the server's PC down, now (60 s countdown) or when all work is
    /// done; replaces a pending request.
    pub async fn power_shutdown(&self, when: aurelius_protocol::ShutdownWhen) -> Result<aurelius_protocol::PowerState, ClientError> {
        self.host(HostWrite::PowerShutdown { when }, done!(Power)).await
    }

    /// Clear the pending shutdown.
    pub async fn power_cancel(&self) -> Result<aurelius_protocol::PowerState, ClientError> {
        self.host(HostWrite::PowerCancel, done!(Power)).await
    }

    /// The pending shutdown, if any.
    pub async fn query_power(&self) -> Result<aurelius_protocol::PowerState, ClientError> {
        self.read(Method::PowerGet, body!(Power)).await
    }

    // ---- updates, backups, export ----

    pub async fn update_status(&self, check: bool) -> Result<aurelius_protocol::UpdateStatus, ClientError> {
        self.read(Method::UpdateGet { check }, body!(Update)).await
    }

    pub async fn prepare_update(&self) -> Result<aurelius_protocol::UpdateStatus, ClientError> {
        self.host(HostWrite::UpdatePrepare, done!(Update)).await
    }

    pub async fn apply_update(&self) -> Result<aurelius_protocol::UpdateStatus, ClientError> {
        self.host(HostWrite::UpdateApply, done!(Update)).await
    }

    /// The desktop app's own release. Returns the app's status.
    pub async fn prepare_app_update(&self) -> Result<aurelius_protocol::UpdateStatus, ClientError> {
        self.host(HostWrite::UpdatePrepareApp, done!(Update)).await
    }

    pub async fn apply_app_update(&self) -> Result<aurelius_protocol::UpdateStatus, ClientError> {
        self.host(HostWrite::UpdateApplyApp, done!(Update)).await
    }

    pub async fn backups(&self) -> Result<aurelius_protocol::BackupsSnapshot, ClientError> {
        self.read(Method::BackupList, body!(Backups)).await
    }

    pub async fn create_backup(&self) -> Result<aurelius_protocol::BackupsSnapshot, ClientError> {
        self.host(HostWrite::BackupCreate, done!(Backups)).await
    }

    /// Restore at the next start (the service stops now).
    pub async fn restore_backup(&self, id: String) -> Result<aurelius_protocol::BackupsSnapshot, ClientError> {
        self.host(HostWrite::BackupRestore { id }, done!(Backups)).await
    }

    pub async fn resume_backups(&self) -> Result<aurelius_protocol::BackupsSnapshot, ClientError> {
        self.host(HostWrite::BackupResume, done!(Backups)).await
    }

    /// Write a readable Markdown + JSON export of a project into its folder.
    pub async fn export_project(&self, project_id: aurelius_domain::ProjectId) -> Result<(String, String), ClientError> {
        self.host(HostWrite::ProjectExport { project_id }, |r| match r {
            HostResult::Exported { markdown_path, json_path } => Some((markdown_path, json_path)),
            _ => None,
        })
        .await
    }

    // ---- notifications ----

    pub async fn mark_notifications_read(&self) -> Result<u64, ClientError> {
        self.host(HostWrite::NotificationMarkAllRead, |r| if let HostResult::Count { count } = r { Some(count) } else { None }).await
    }

    /// Mark one notification read, for this board member only; 0 when there was nothing to read.
    pub async fn mark_notification_read(&self, id: String) -> Result<u64, ClientError> {
        self.host(HostWrite::NotificationMarkRead { id }, |r| if let HostResult::Count { count } = r { Some(count) } else { None }).await
    }

    /// Remove one notification.
    pub async fn dismiss_notification(&self, id: String) -> Result<u64, ClientError> {
        self.host(HostWrite::NotificationDismiss { id }, |r| if let HostResult::Count { count } = r { Some(count) } else { None }).await
    }

    /// Remove every notification; returns how many went.
    pub async fn dismiss_all_notifications(&self) -> Result<u64, ClientError> {
        self.host(HostWrite::NotificationDismissAll, |r| if let HostResult::Count { count } = r { Some(count) } else { None }).await
    }

    // ---- remote access, pairing, push ----

    pub async fn remote_status(&self) -> Result<aurelius_protocol::remote::RemoteStatus, ClientError> {
        self.read(Method::RemoteGetStatus, body!(Remote)).await
    }

    /// Enable, disable, decide a pairing, revoke a device or configure push:
    /// answered with the remote access status.
    pub async fn remote(&self, write: HostWrite) -> Result<aurelius_protocol::remote::RemoteStatus, ClientError> {
        self.host(write, done!(Remote)).await
    }

    pub async fn pairing_start(&self) -> Result<aurelius_protocol::remote::PairingCode, ClientError> {
        self.host(HostWrite::RemoteStartPairing, done!(PairingCode)).await
    }

    pub async fn push_test(&self, client_id: aurelius_domain::ClientId) -> Result<(), ClientError> {
        self.host_ok(HostWrite::PushTest { client_id }).await
    }

    // ---- secrets, server ----

    /// Store a connection's API key in the server's credential store.
    pub async fn set_secret(&self, connection_id: aurelius_domain::ProviderId, secret: String) -> Result<(), ClientError> {
        self.host_ok(HostWrite::SecretSet { connection_id, secret }).await
    }

    /// Let the server stop and start again. The connection drops shortly after
    /// the answer; reconnect as after an update.
    pub async fn restart_server(&self) -> Result<(), ClientError> {
        match self.host_ok(HostWrite::ServerRestart).await {
            Ok(()) | Err(ClientError::Disconnected) => Ok(()),
            Err(err) => Err(err),
        }
    }
}

/// New envelope with a fresh command ID. Keep it to retry idempotently.
pub fn envelope(command: Command, expected_version: Option<u64>) -> CommandEnvelope {
    CommandEnvelope { command_id: CommandId::new(), expected_version, command }
}

/// Connect, starting the daemon next to this executable if it is not running.
pub async fn connect_or_spawn(root: &Path, client_name: &str, events: mpsc::UnboundedSender<Event>) -> Result<Client, ClientError> {
    connect_or_spawn_with_streams(root, client_name, events, None).await
}

pub async fn connect_or_spawn_with_streams(
    root: &Path,
    client_name: &str,
    events: mpsc::UnboundedSender<Event>,
    streams: Option<mpsc::UnboundedSender<StreamDelta>>,
) -> Result<Client, ClientError> {
    spawn_and_connect(root, client_name, events, streams, accepted(&[])).await
}

async fn spawn_and_connect(
    root: &Path,
    client_name: &str,
    events: mpsc::UnboundedSender<Event>,
    streams: Option<mpsc::UnboundedSender<StreamDelta>>,
    accepts: Vec<String>,
) -> Result<Client, ClientError> {
    let endpoint = aurelius_host::endpoint(root);
    let connect = || async { Client::start(transport::pipe(&endpoint).await?, None, client_name, events.clone(), streams.clone(), accepts.clone()).await };
    if let Ok(client) = connect().await {
        return Ok(client);
    }
    let daemon = daemon_path().map_err(|e| ClientError::Unreachable(e.to_string()))?;
    aurelius_host::spawn_daemon_detached(&daemon, root).map_err(|e| ClientError::Unreachable(format!("{}: {e}", daemon.display())))?;
    let mut last = None;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        match connect().await {
            Ok(client) => return Ok(client),
            Err(err) => last = Some(err),
        }
    }
    Err(last.unwrap_or_else(|| ClientError::Unreachable("timed out".into())))
}

fn daemon_path() -> std::io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("AURELIUS_DAEMON_PATH") {
        return Ok(PathBuf::from(path));
    }
    aurelius_host::sibling_executable(DAEMON_BINARY)
}

/// True for errors after which the same envelope may be re-sent safely.
pub fn retryable(err: &ClientError) -> bool {
    err.is_disconnect() || matches!(err, ClientError::Protocol(p) if p.code == ErrorCode::StorageUnavailable)
}
