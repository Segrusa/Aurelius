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
mod management;
mod reads;
mod sql;
mod subscription;
mod users;
mod writes;

pub(crate) use bridge::serve as serve_bridge;

use crate::{Shared, VERSION};
use aurelius_application::Origin;
use aurelius_domain::{AppKeyId, ClientId, Command, app_key::{AppAudience, AppScope}};
use aurelius_host::ipc::{BoxStream, IpcError, read_frame};
use aurelius_protocol::remote::{Capability, Needs, grants, required};
use aurelius_protocol::{
    Call, ErrorCode, HelloResult, Method, Outcome, PROTOCOL_MAJOR, ProtocolError, Request, ResponseBody, ServerMessage, WriteMethod,
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
    /// The active board member of this connection (never for an app).
    pub user: Option<aurelius_domain::UserId>,
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
        Self { client_id: ClientId::new(), origin: Origin::LocalIpc, remote: None, app: None, user: None }
    }

    pub fn remote(client_id: ClientId, capabilities: Vec<Capability>, inbox: PathBuf) -> Self {
        Self { client_id, origin: Origin::Remote, remote: Some(RemotePeer { capabilities, inbox }), app: None, user: None }
    }

    /// An app that came in over the gateway with its key (WebSocket or `/v1/mcp`).
    pub fn remote_app(key: &aurelius_storage::app_keys::AppKeyRow) -> Self {
        Self { client_id: ClientId::new(), origin: Origin::Remote, remote: None, app: Some(AppPeer::of(key)), user: None }
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
    let session = tokio::spawn(run(shared, Peer::local(), req_rx, out_tx.clone()));

    loop {
        let value: serde_json::Value = match read_frame(&mut reader).await {
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
    let _ = session.await;
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
    // One event forwarder per session; a new subscribe replaces the old one
    // (two forwarders would deliver every event twice).
    let mut forwarder: Option<tokio::task::JoinHandle<()>> = None;
    // Terminals this session shows; dropping one detaches (the shell keeps running).
    let mut terminals: HashMap<String, aurelius_host::terminal::Attachment> = HashMap::new();
    // The hello said this client merges tool-use changes in live updates.
    let mut activity_changes = false;
    let mut team_channels = false;
    // The tries of `user.use` on this connection, and its new board members (S5-3).
    let mut attempts = users::Attempts::default();
    let mut creates = users::Attempts::default();
    while let Some(request) = requests.recv().await {
        let request_id = request.request_id;
        if let Err(err) = gate(&shared, &mut peer, &request).await {
            if respond(&out_tx, request_id, Err(err)).await.is_err() {
                break;
            }
            continue;
        }

        let outcome = match request.call {
            Call::Write(write) => match users::count_create(&mut creates, &write) {
                Ok(()) => writes::execute(&shared, &mut peer, request_id, request.idempotency_key, request.expected_version, write).await,
                Err(err) => Err(err),
            },
            Call::Read(Method::SessionHello(params)) => {
                activity_changes = params.accepts.iter().any(|a| a == aurelius_protocol::client_feature::ACTIVITY_CHANGES);
                team_channels = params.accepts.iter().any(|a| a == aurelius_protocol::client_feature::TEAM_CHANNELS);
                let remembered = params.user_id;
                match authenticate(&shared, &mut peer, params.app_key).await {
                    Ok(()) => users::remembered(&shared, &mut peer, remembered).await.map(|who| ResponseBody::Hello(hello(&shared, &peer, who))),
                    Err(err) => Err(err),
                }
            }
            Call::Read(Method::UserUse(params)) => users::use_user(&shared, &mut peer, &mut attempts, params).await.map(ResponseBody::WhoAmI),
            Call::Read(Method::UserWhoami) => users::whoami(&shared, peer.user).await.map(ResponseBody::WhoAmI),
            Call::Read(Method::EventSubscribe { after_seq }) => match subscription::subscribe(&shared, after_seq, activity_changes, team_channels, out_tx.clone()).await {
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
        if respond(&out_tx, request_id, outcome).await.is_err() {
            break;
        }
    }
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
    if peer.remote.is_some() && !peer.is_owner() {
        if let Some(err) = remote_run_defaults_denied(shared, &request.call).await {
            return Err(err);
        }
    }
    Ok(())
}

fn hello(shared: &Arc<Shared>, peer: &Peer, who: aurelius_domain::user::WhoAmI) -> HelloResult {
    HelloResult {
        host_id: shared.host_id,
        daemon_version: VERSION.into(),
        protocol: PROTOCOL_MAJOR,
        environment: aurelius_host::Environment::current().as_str().into(),
        host_name: aurelius_host::host_name(),
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
        board_members_exist: who.board_members_exist,
        user: who.user,
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
