//! The client API (protocol 2.0): one session per client, over local IPC
//! (`serve`) or the remote gateway (`run`).
//!
//! Every request carries the protocol major, which must equal ours (else a hard
//! "update the app or the server"). A request then passes the rights of its
//! peer (`required`), and is either a read (`reads`), a session method (hello,
//! subscribe, terminals) or a write: every write, domain command or host
//! write, goes through the one pipeline in `writes`.
//!
//! A session of an outside app (an app key, feature `app_keys`: given in the
//! hello over local IPC, or on the gateway's WebSocket and `/v1/mcp`) passes
//! the scopes of `aurelius_protocol::app` instead, with its key looked up again
//! for every request, so a revoked key stops at once.
//!
//! The internal channel of the agent bridge is separate (`bridge`).

mod bridge;
mod git;
mod host;
pub mod limits;
mod management;
mod reads;
pub mod sessions;
mod sql;
mod subscription;
mod users;
mod vault;
mod writes;

pub(crate) use bridge::serve as serve_bridge;
pub use users::SESSION_CHECK_MS;

use crate::{Shared, VERSION};
use aurelius_application::Origin;
use aurelius_domain::{AppKeyId, ClientId, Command, app_key::{AppAudience, AppScope}};
use aurelius_host::ipc::{BoxStream, IpcError, read_frame};
use aurelius_protocol::remote::{Capability, Needs, grants, required};
use aurelius_protocol::{
    Call, ErrorCode, HelloResult, HostWrite, Method, Outcome, PROTOCOL_MAJOR, ProtocolError, Request, ResponseBody, ServerMessage, WriteMethod,
    reject_unparsed,
};
use aurelius_storage::StorageError;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

pub const OUTBOX: usize = 256;

/// Who is on the other end. Local IPC is the OS user (everything allowed);
/// a paired device carries its capabilities; a paired computer
/// approved as owner may do everything except what stays local.
#[derive(Debug, Clone)]
pub struct Peer {
    pub client_id: ClientId,
    pub origin: Origin,
    pub remote: Option<RemotePeer>,
    /// An outside app: its scopes decide, not the device rights.
    pub app: Option<AppPeer>,
    /// The board member of this connection's session (never for an app); set
    /// only by `hello { session_token }`, `user.login` and the first `user.create`.
    pub user: Option<aurelius_domain::UserId>,
    /// The session behind `user` (protocol 5); none: the gate lets only the
    /// sign-in through.
    pub session: Option<users::PeerSession>,
    /// This connection, for closing it when its session ends (`sessions::Hub`).
    pub conn: u64,
}

/// The app key a session acts for; its scopes as of the last request.
#[derive(Debug, Clone)]
pub struct AppPeer {
    pub key_id: AppKeyId,
    pub name: String,
    pub scopes: Vec<AppScope>,
    pub audience: AppAudience,
}

impl AppPeer {
    fn of(key: &aurelius_storage::app_keys::AppKeyRow) -> Self {
        Self { key_id: key.id, name: key.name.clone(), scopes: key.scopes.clone(), audience: key.audience }
    }
}

#[derive(Debug, Clone)]
pub struct RemotePeer {
    pub capabilities: Vec<Capability>,
    /// Uploads from this device land here; `attachment.add` may only point inside it.
    pub inbox: PathBuf,
}

impl Peer {
    pub fn local() -> Self {
        Self { client_id: ClientId::new(), origin: Origin::LocalIpc, remote: None, app: None, user: None, session: None, conn: 0 }
    }

    pub fn remote(client_id: ClientId, capabilities: Vec<Capability>, inbox: PathBuf) -> Self {
        Self { client_id, origin: Origin::Remote, remote: Some(RemotePeer { capabilities, inbox }), app: None, user: None, session: None, conn: 0 }
    }

    /// An app that came in over the gateway with its key (WebSocket or `/v1/mcp`).
    pub fn remote_app(key: &aurelius_storage::app_keys::AppKeyRow) -> Self {
        Self { client_id: ClientId::new(), origin: Origin::Remote, remote: None, app: Some(AppPeer::of(key)), user: None, session: None, conn: 0 }
    }

    fn is_owner(&self) -> bool {
        self.remote.as_ref().is_some_and(|r| r.capabilities.contains(&Capability::Owner))
    }

    /// `Some(error)` when a remote device may not send this.
    fn denied(&self, call: &Call) -> Option<ProtocolError> {
        // An app's scopes are checked in `gate`, against its key as it is now.
        if self.app.is_some() {
            return None;
        }
        let remote = self.remote.as_ref()?;
        let needs = required(call);
        if !grants(&remote.capabilities, needs) {
            let message = match needs {
                Needs::Owner => "This needs full access. Pair this computer as owner on the server.".to_owned(),
                Needs::Cap(cap) => format!("This device may not do this (needs \"{}\").", cap.as_str()),
            };
            return Some(ProtocolError::new(ErrorCode::Forbidden, message));
        }
        if self.is_owner() {
            // An owner can grant any folder on the server; its attachments
            // and run defaults need no phone-sized limits.
            return None;
        }
        if let Call::Write(WriteMethod::Domain(command)) = call {
            let inside = |path: &str| {
                std::fs::canonicalize(path)
                    .ok()
                    .zip(std::fs::canonicalize(&remote.inbox).ok())
                    .is_some_and(|(path, inbox)| path.starts_with(inbox))
            };
            let files: Vec<&String> = match command {
                Command::AttachFile(attach) => vec![&attach.path],
                Command::SendChatMessage(send) => send.images.iter().collect(),
                Command::SendEmployeeMessage(send) => send.images.iter().chain(&send.files).collect(),
                Command::SendGroupMessage(send) => send.images.iter().chain(&send.files).collect(),
                Command::SendProjectMessage(send) => send.images.iter().chain(&send.files).collect(),
                Command::CreateTask(task) => task.files.iter().collect(),
                Command::CreateTaskSchedule(schedule) => schedule.files.iter().collect(),
                Command::UpdateTaskSchedule(schedule) => schedule.add_files.iter().collect(),
                Command::AddIdea(idea) => idea.files.iter().collect(),
                Command::UpdateIdea(idea) => idea.add_files.iter().collect(),
                Command::CreateTicket(ticket) => ticket.files.iter().collect(),
                Command::UpdateTicket(ticket) => ticket.add_files.iter().flatten().collect(),
                _ => Vec::new(),
            };
            if !files.iter().all(|path| inside(path)) {
                return Some(ProtocolError::new(ErrorCode::Forbidden, "Upload the file from this device first."));
            }
        }
        None
    }
}

/// Local IPC adapter: length-prefixed JSON frames in, the same frames out.
pub async fn serve(shared: Arc<Shared>, stream: BoxStream) {
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (out_tx, mut out_rx) = mpsc::channel::<ServerMessage>(OUTBOX);
    let (req_tx, req_rx) = mpsc::channel::<Request>(16);

    let writer_task = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        // Never stops silently: a message over the frame limit is answered
        // with an error (a response) or dropped (anything else), and logged.
        while let Some(message) = out_rx.recv().await {
            let bytes = match aurelius_host::frame::encode(&message) {
                Ok(bytes) => bytes,
                Err(err) => match oversized(&message, &err).map(|reply| aurelius_host::frame::encode(&reply)) {
                    Some(Ok(bytes)) => bytes,
                    _ => continue,
                },
            };
            let written = async {
                writer.write_all(&bytes).await?;
                writer.flush().await
            };
            if let Err(err) = written.await {
                tracing::info!(error = %err, "client connection: writing failed, closing");
                break;
            }
        }
    });
    let mut session = tokio::spawn(run(shared, Peer::local(), req_rx, out_tx.clone()));
    let mut ended = false;

    loop {
        // The session may end on its own (signed out elsewhere): then the connection closes.
        let frame = tokio::select! {
            frame = read_frame(&mut reader) => frame,
            _ = &mut session => {
                ended = true;
                break;
            }
        };
        let value: serde_json::Value = match frame {
            Ok(Some(value)) => value,
            Ok(None) => break,
            Err(IpcError::Frame(err @ aurelius_host::frame::FrameError::TooLarge(_))) => {
                // Oversized: the body was not read, so the stream is out of sync. Answer once, then drop.
                let _ = out_tx
                    .send(ServerMessage::Response {
                        request_id: 0,
                        outcome: Outcome::Error(ProtocolError::new(ErrorCode::FrameTooLarge, err.to_string())),
                    })
                    .await;
                break;
            }
            Err(IpcError::Frame(err)) => {
                // Not JSON, but the frame was read whole: answer and keep the connection.
                if out_tx.send(reject_unparsed(None, &err)).await.is_err() {
                    break;
                }
                continue;
            }
            Err(IpcError::Io(_)) => break,
        };
        // Not a request (e.g. a method this server does not know): answer for its id, keep the connection.
        let request: Request = match serde_json::from_value(value.clone()) {
            Ok(request) => request,
            Err(err) => {
                if out_tx.send(reject_unparsed(Some(&value), &err)).await.is_err() {
                    break;
                }
                continue;
            }
        };
        if req_tx.send(request).await.is_err() {
            break;
        }
    }
    drop(req_tx);
    if !ended {
        let _ = session.await;
    }
    drop(out_tx);
    let _ = writer_task.await;
}

/// A message the local frame cannot carry (or cannot encode): logged, and for a
/// response the error that replaces it, under the same request id. Older
/// clients know `frame_too_large`, so it is the code; a new one would make
/// their reader fail on the frame and drop the connection.
pub(crate) fn oversized(message: &ServerMessage, err: &aurelius_host::frame::FrameError) -> Option<ServerMessage> {
    let kind = match message {
        ServerMessage::Response { .. } => "response",
        ServerMessage::Event(_) => "event",
        ServerMessage::Stream(_) => "stream update",
        ServerMessage::StreamAppend(_) => "stream append",
        ServerMessage::TerminalOutput(_) => "terminal output",
        ServerMessage::TerminalExit(_) => "terminal exit",
        ServerMessage::LoadChanged(_) => "load update",
        ServerMessage::PowerChanged(_) => "power update",
        ServerMessage::ClaudeLoginPrompt(_) => "claude login prompt",
        ServerMessage::SignedOut { .. } => "sign-out",
    };
    match message {
        ServerMessage::Response { request_id, .. } => {
            tracing::warn!(request_id, error = %err, "client connection: {kind} too large to send, answering with an error");
            Some(ServerMessage::Response {
                request_id: *request_id,
                outcome: Outcome::Error(ProtocolError::new(
                    ErrorCode::FrameTooLarge,
                    format!("The server's reply is too large to send ({err}). Update Aurelius to load it in parts."),
                )),
            })
        }
        _ => {
            tracing::warn!(error = %err, "client connection: {kind} too large to send, dropped");
            None
        }
    }
}

/// One client session over any transport: requests in order; responses,
/// events and stream deltas out. Ends when `requests` closes.
pub async fn run(shared: Arc<Shared>, mut peer: Peer, mut requests: mpsc::Receiver<Request>, out_tx: mpsc::Sender<ServerMessage>) {
    peer.conn = shared.sessions.connection();
    // Ending this connection's session elsewhere (revoke, a new password, disable) closes it.
    // `users` puts the connection in the hub when it gets a board member (before the
    // session is stored or checked, R1 M-2); losing one takes it out (below).
    let (end_tx, mut end_rx) = mpsc::unbounded_channel::<aurelius_domain::user::SignOutReason>();
    // One event forwarder per session; a new subscribe replaces the old one
    // (two forwarders would deliver every event twice).
    let mut forwarder: Option<tokio::task::JoinHandle<()>> = None;
    // Terminals this session shows; dropping one detaches (the shell keeps running).
    let mut terminals: HashMap<String, aurelius_host::terminal::Attachment> = HashMap::new();
    // The hello said this client merges tool-use changes in live updates.
    let mut activity_changes = false;
    let mut team_channels = false;
    let mut server_name_events = false;
    // The tries of `user.login` on this connection, and its new board members (S5-3).
    let mut attempts = users::Attempts::default();
    let mut creates = users::Attempts::default();
    loop {
        // An end of the session goes first: no request of an ended session is answered.
        let request = tokio::select! {
            biased;
            Some(reason) = end_rx.recv() => {
                let _ = out_tx.send(ServerMessage::SignedOut { reason }).await;
                break;
            }
            request = requests.recv() => match request {
                Some(request) => request,
                None => break,
            },
        };
        let request_id = request.request_id;
        if let Err(reason) = users::keep_session(&shared, &mut peer).await {
            shared.sessions.forget(peer.conn);
            let _ = out_tx.send(ServerMessage::SignedOut { reason }).await;
            break;
        }
        if let Err(err) = gate(&shared, &mut peer, &request).await {
            if respond(&out_tx, request_id, Err(err)).await.is_err() {
                break;
            }
            continue;
        }
        let before = peer.user;

        let outcome = match request.call {
            // Accounts and sessions: never into a request hash or a receipt of the host pipeline.
            // Connecting the session's own Claude login: the prompt goes to this connection only (C4).
            Call::Write(WriteMethod::Host(HostWrite::ClaudeLoginLink)) => match request.idempotency_key {
                Some(key) => {
                    let ctx = writes::context(&shared, &peer, request_id);
                    users::claude_link(&shared, &peer, ctx, key, out_tx.clone()).await
                }
                None => Err(ProtocolError::new(ErrorCode::InvalidRequest, "writes need an idempotency_key")),
            },
            Call::Write(WriteMethod::Host(
                write @ (HostWrite::SessionRevoke { .. }
                | HostWrite::UserSetPassword(_)
                | HostWrite::UserDisable { .. }
                | HostWrite::ClaudeLoginCancel { .. }
                | HostWrite::ClaudeLoginUnlink),
            )) => match request.idempotency_key {
                Some(key) => {
                    let ctx = writes::context(&shared, &peer, request_id);
                    users::session_write(&shared, &mut peer, ctx, key, &mut attempts, write).await
                }
                None => Err(ProtocolError::new(ErrorCode::InvalidRequest, "writes need an idempotency_key")),
            },
            // Counted against the limits of `user.create` only once it is hashed (R5).
            Call::Write(WriteMethod::Host(HostWrite::UserCreate(params))) => match request.idempotency_key {
                Some(key) => {
                    let ctx = writes::context(&shared, &peer, request_id);
                    users::create(&shared, &mut peer, &mut creates, &end_tx, ctx, key, request.expected_version, params).await.map(ResponseBody::Receipt)
                }
                None => Err(ProtocolError::new(ErrorCode::InvalidRequest, "writes need an idempotency_key")),
            },
            // The vault: a value never enters the host pipeline (its receipt, request hash or event).
            Call::Write(WriteMethod::Host(write @ (HostWrite::VaultCreate(_) | HostWrite::VaultUpdate(_) | HostWrite::VaultArchive { .. }))) => match request.idempotency_key {
                Some(key) => vault::write(&shared, &peer, request_id, key, request.expected_version, write).await,
                None => Err(ProtocolError::new(ErrorCode::InvalidRequest, "writes need an idempotency_key")),
            },
            Call::Write(write) => writes::execute(&shared, &mut peer, request_id, request.idempotency_key, request.expected_version, write).await,
            Call::Read(Method::SessionHello(params)) => {
                activity_changes = params.accepts.iter().any(|a| a == aurelius_protocol::client_feature::ACTIVITY_CHANGES);
                team_channels = params.accepts.iter().any(|a| a == aurelius_protocol::client_feature::TEAM_CHANNELS);
                server_name_events = params.accepts.iter().any(|a| a == aurelius_protocol::client_feature::SERVER_NAME);
                match authenticate(&shared, &mut peer, params.app_key).await {
                    Ok(()) => match users::hello_session(&shared, &mut peer, &end_tx, params.session_token).await {
                        Ok(who) => {
                            let host_name = server_name(&shared).await.name;
                            Ok(ResponseBody::Hello(hello(&shared, &peer, who, host_name)))
                        }
                        Err(err) => Err(err),
                    },
                    Err(err) => Err(err),
                }
            }
            Call::Read(Method::UserLogin(params)) => users::login(&shared, &mut peer, &mut attempts, &end_tx, params).await.map(ResponseBody::Login),
            // Without a session the board members are not named (Titan, SEC1 m9): the sign-in
            // screen needs only `board_members_exist` from the hello or whoami. Apps keep the list.
            Call::Read(Method::UserList) if peer.app.is_none() && peer.session.is_none() => Ok(ResponseBody::Users(aurelius_domain::user::UsersSnapshot { users: Vec::new() })),
            Call::Read(Method::UserLogout) => users::logout(&shared, &mut peer).await.map(ResponseBody::WhoAmI),
            Call::Read(Method::UserWhoami) => users::whoami_of(&shared, &peer).await.map(ResponseBody::WhoAmI),
            Call::Read(method @ (Method::SessionList { .. } | Method::ClaudeLoginStatus | Method::ClaudeLoginInput(_))) => users::session_read(&shared, &peer, method).await,
            // The password is checked again and counted like a sign-in (E9).
            Call::Read(Method::VaultKeyExport(params)) => vault::key_export(&shared, &peer, &mut attempts, params).await,
            Call::Read(method @ (Method::VaultList { .. } | Method::VaultReveal { .. } | Method::VaultAudit { .. })) => vault::read(&shared, &peer, method).await,
            Call::Read(Method::EventSubscribe { after_seq }) => match subscription::subscribe(&shared, after_seq, activity_changes, team_channels, server_name_events, out_tx.clone()).await {
                Ok((body, task)) => {
                    if let Some(old) = forwarder.replace(task) {
                        old.abort();
                    }
                    Ok(body)
                }
                Err(err) => Err(err),
            },
            Call::Read(Method::TerminalOpen { conversation_id, cols, rows, fresh, terminal_id, additional }) => {
                // A plain open keeps the old one-terminal-per-conversation view of this session.
                let only = terminal_id.is_none() && !additional;
                let options = aurelius_host::terminal::OpenOptions { fresh, terminal_id, additional };
                match aurelius_host::terminal::open(&shared, conversation_id, cols, rows, options, out_tx.clone()).await {
                    Ok((view, mut attachment)) => {
                        // Drop an earlier attachment to this terminal before the new one replays.
                        terminals.remove(&view.terminal_id);
                        // The response goes out before any output: the client knows the id first.
                        if respond(&out_tx, request_id, Ok(ResponseBody::Terminal(view.clone()))).await.is_err() {
                            break;
                        }
                        attachment.release();
                        if only {
                            terminals.retain(|_, a| !a.is_for(conversation_id));
                        }
                        terminals.insert(view.terminal_id, attachment);
                        continue;
                    }
                    Err(err) => Err(err),
                }
            }
            // Dropping the attachment stops the output; the shell and its scrollback stay.
            Call::Read(Method::TerminalDetach { terminal_id }) => match terminals.remove(&terminal_id) {
                Some(_) => Ok(ResponseBody::TerminalDone),
                None if shared.terminals.exists(&terminal_id) => Ok(ResponseBody::TerminalDone),
                None => Err(ProtocolError::new(ErrorCode::NotFound, "This terminal has ended.")),
            },
            // The board member reading a discussion marks it read for themselves (not an app).
            Call::Read(method) => match (reads::handle(&shared, method, peer.user.filter(|_| peer.app.is_none())).await, peer.app.as_ref().filter(|a| a.audience == AppAudience::Management)) {
                // What a management app reads is cut down to the management (and its own work).
                (Ok(body), Some(app)) => management::filter(&shared, app, body).await,
                (outcome, _) => outcome,
            },
        };
        // The board member is gone (signed out, disabled themselves, a hello with a
        // token that no longer works, an app key): out of the hub, and no more
        // events or terminal output on this connection (MU3-06 AC2, R1 M-1).
        // Another board member signed in instead keeps the hub, not the terminals
        // or a sign-in to Claude of the earlier one (R4 m-2).
        if before.is_some() && peer.user != before {
            if peer.user.is_none() {
                shared.sessions.forget(peer.conn);
                if let Some(task) = forwarder.take() {
                    task.abort();
                }
            }
            terminals.clear();
            shared.claude_links.cancel_conn(peer.conn);
        }
        if respond(&out_tx, request_id, outcome).await.is_err() {
            break;
        }
    }
    shared.sessions.forget(peer.conn);
    // A sign-in to Claude of this connection ends with it (C13).
    shared.claude_links.cancel_conn(peer.conn);
    if let Some(task) = forwarder {
        task.abort();
    }
}

async fn respond(out_tx: &mpsc::Sender<ServerMessage>, request_id: u64, outcome: Result<ResponseBody, ProtocolError>) -> Result<(), ()> {
    let outcome = match outcome {
        Ok(body) => Outcome::Ok(body),
        Err(err) => Outcome::Error(err),
    };
    out_tx.send(ServerMessage::Response { request_id, outcome }).await.map_err(|_| ())
}

/// A hello with an app key: from now on this session acts for that app. A
/// session that already acts for an app keeps it (a hello never widens it).
/// An app has no board member, so one named before on this session is dropped.
async fn authenticate(shared: &Arc<Shared>, peer: &mut Peer, key: Option<String>) -> Result<(), ProtocolError> {
    let Some(secret) = key else { return Ok(()) };
    let s = shared.clone();
    let key = app_task(move || s.service.authenticate_app(&secret)).await?;
    match &peer.app {
        Some(app) if app.key_id != key.id => Err(ProtocolError::new(ErrorCode::Forbidden, "This session already acts for another app.")),
        _ => {
            peer.app = Some(AppPeer::of(&key));
            peer.user = None;
            peer.session = None;
            Ok(())
        }
    }
}

/// An app's request: its key still works (looked up now, so revoking stops
/// open sessions too), its scopes cover the call, and what the call points
/// at is in a project, never one of the user's own chats.
async fn app_gate(shared: &Arc<Shared>, app: &mut AppPeer, call: &Call) -> Result<(), ProtocolError> {
    let s = shared.clone();
    let key_id = app.key_id;
    let key = app_task(move || s.service.app_key_for_request(key_id)).await?;
    *app = AppPeer::of(&key);
    aurelius_protocol::app::allowed_for(&app.scopes, app.audience, call).map_err(|why| ProtocolError::new(ErrorCode::Forbidden, why))?;
    // A management app reaches the management only: checked against the company as it is now.
    if app.audience == AppAudience::Management {
        return management::check(shared, app, call).await;
    }
    let conversation = match call {
        Call::Read(Method::ConversationGet { conversation_id }) => Some(*conversation_id),
        Call::Write(WriteMethod::Domain(Command::SetAgentMode(m))) => Some(m.conversation_id),
        Call::Write(WriteMethod::Domain(Command::SendEmployeeMessage(m))) => Some(m.conversation_id),
        Call::Write(WriteMethod::Domain(Command::SendProjectMessage(m))) => m.conversation_id,
        _ => None,
    };
    if let Some(conversation) = conversation {
        let store = shared.store().clone();
        if !storage_task(move || store.read(|c| aurelius_storage::app_keys::project_conversation(c, conversation))).await? {
            return Err(ProtocolError::new(ErrorCode::Forbidden, "Apps only reach the conversations of projects."));
        }
    }
    Ok(())
}

/// What every request must pass first: the protocol major, then the rights of the peer.
async fn gate(shared: &Arc<Shared>, peer: &mut Peer, request: &Request) -> Result<(), ProtocolError> {
    if request.protocol != PROTOCOL_MAJOR {
        return Err(ProtocolError::new(ErrorCode::IncompatibleProtocol, aurelius_protocol::incompatible_message(request.protocol)));
    }
    if let Some(err) = peer.denied(&request.call) {
        return Err(err);
    }
    if let Some(app) = peer.app.as_mut() {
        return app_gate(shared, app, &request.call).await;
    }
    // The gate (MU3-06): nothing without a session but the sign-in; a temporary
    // password only lets its change through.
    match &peer.session {
        None if !without_session(&request.call) => return Err(users::unauthenticated()),
        Some(session) if session.must_change_password && !while_changing_password(&request.call, peer.user) => {
            return Err(ProtocolError::new(ErrorCode::PasswordChangeRequired, "Choose a new password first."));
        }
        _ => {}
    }
    if peer.remote.is_some() && !peer.is_owner() {
        if let Some(err) = remote_run_defaults_denied(shared, &request.call).await {
            return Err(err);
        }
    }
    Ok(())
}

/// What a connection without a session may call: the handshake, who is there,
/// the sign-in and the first board member (MU3-06 AC1).
pub fn without_session(call: &Call) -> bool {
    matches!(
        call,
        Call::Read(Method::SessionHello(_) | Method::UserWhoami | Method::UserLogin(_) | Method::UserList) | Call::Write(WriteMethod::Host(HostWrite::UserCreate(_)))
    )
}

/// What a session with a temporary password may call (S7, SEC1 M4): the
/// handshake, who is there, signing out or in again, and its own new password.
/// Nothing else, so not `user.create` or `user.list` either.
fn while_changing_password(call: &Call, me: Option<aurelius_domain::UserId>) -> bool {
    match call {
        Call::Write(WriteMethod::Host(HostWrite::UserSetPassword(p))) => p.user_id.is_none() || p.user_id == me,
        Call::Read(Method::SessionHello(_) | Method::UserWhoami | Method::UserLogout | Method::UserLogin(_)) => true,
        _ => false,
    }
}

/// The server's name as clients see it (SN-01): the chosen one, else the computer's.
pub(super) async fn server_name(shared: &Arc<Shared>) -> aurelius_domain::server::ServerNameView {
    let store = shared.store().clone();
    tokio::task::spawn_blocking(move || aurelius_host::server_name(&store))
        .await
        .unwrap_or_else(|_| aurelius_domain::server::ServerNameView::of(None, aurelius_host::host_name(), 0))
}

fn hello(shared: &Arc<Shared>, peer: &Peer, who: users::HelloWho, host_name: String) -> HelloResult {
    HelloResult {
        host_id: shared.host_id,
        daemon_version: VERSION.into(),
        protocol: PROTOCOL_MAJOR,
        environment: aurelius_host::Environment::current().as_str().into(),
        host_name,
        os: std::env::consts::OS.into(),
        remote: peer.remote.is_some() || peer.origin == Origin::Remote,
        client_id: peer.remote.as_ref().map(|_| peer.client_id),
        capabilities: match (&peer.app, &peer.remote) {
            // An app has scopes (`app`), no device rights.
            (Some(_), _) => Vec::new(),
            (None, None) => Capability::ALL.to_vec(),
            (None, Some(remote)) => remote.capabilities.clone(),
        },
        features: aurelius_protocol::FEATURES.iter().map(|f| (*f).to_owned()).collect(),
        app: peer.app.as_ref().map(|a| aurelius_domain::app_key::AppSession { key_id: a.key_id, name: a.name.clone(), scopes: a.scopes.clone(), audience: a.audience }),
        board_members_exist: who.who.board_members_exist,
        user: who.who.user,
        must_change_password: who.who.must_change_password,
        claude_login: who.claude_login,
    }
}

/// A phone may pick the model and effort, never the connection (which may
/// bill per use) or the persona: those must equal the current default of the
/// same scope and purpose (Inherit when that layer has no row yet).
async fn remote_run_defaults_denied(shared: &Arc<Shared>, call: &Call) -> Option<ProtocolError> {
    let Call::Write(WriteMethod::Domain(Command::SetRunDefaults(change))) = call else { return None };
    let store = shared.store().clone();
    let current = match storage_task(move || store.read(aurelius_storage::connections::list_defaults)).await {
        Ok(defaults) => defaults,
        Err(err) => return Some(err),
    };
    let current = current
        .into_iter()
        .find(|d| d.scope == change.scope && d.purpose == change.purpose)
        .map(|d| d.settings)
        .unwrap_or_default();
    if change.settings.connection != current.connection || change.settings.persona != current.persona {
        return Some(ProtocolError::new(ErrorCode::Forbidden, "Choose the connection in Aurelius on the PC; the phone changes only model and effort."));
    }
    None
}

/// Work to do once when the daemon starts, before the first connection needs it.
pub fn warm_up() {
    users::warm_up();
}

pub(crate) fn internal(err: impl ToString) -> ProtocolError {
    ProtocolError::new(ErrorCode::Internal, err.to_string())
}

fn storage(err: StorageError) -> ProtocolError {
    ProtocolError::new(ErrorCode::StorageUnavailable, err.to_string())
}

/// Runs blocking storage work off the async threads.
pub(crate) async fn storage_task<T: Send + 'static>(f: impl FnOnce() -> Result<T, StorageError> + Send + 'static) -> Result<T, ProtocolError> {
    tokio::task::spawn_blocking(f).await.map_err(internal)?.map_err(storage)
}

/// Runs blocking application work off the async threads.
pub(crate) async fn app_task<T: Send + 'static>(f: impl FnOnce() -> Result<T, ProtocolError> + Send + 'static) -> Result<T, ProtocolError> {
    tokio::task::spawn_blocking(f).await.map_err(internal)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_host::frame::{FrameError, MAX_FRAME_BYTES, encode};

    #[test]
    fn an_oversized_reply_becomes_an_error_for_its_request() {
        let huge = ServerMessage::Response { request_id: 42, outcome: Outcome::Error(ProtocolError::new(ErrorCode::Internal, "x".repeat(MAX_FRAME_BYTES + 1))) };
        let err = encode(&huge).unwrap_err();
        assert!(matches!(err, FrameError::TooLarge(_)));
        let reply = oversized(&huge, &err).expect("a response is answered");
        match &reply {
            ServerMessage::Response { request_id: 42, outcome: Outcome::Error(e) } => assert_eq!(e.code, ErrorCode::FrameTooLarge),
            other => panic!("{other:?}"),
        }
        assert!(encode(&reply).is_ok(), "the error itself fits");
    }

    #[test]
    fn an_oversized_push_is_dropped() {
        let push = ServerMessage::TerminalExit(aurelius_protocol::TerminalExit { terminal_id: "t".into(), code: None });
        assert!(oversized(&push, &FrameError::TooLarge(MAX_FRAME_BYTES + 1)).is_none());
    }
}
