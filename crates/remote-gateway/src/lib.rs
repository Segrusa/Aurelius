//! Optional remote gateway.
//!
//! ```text
//! phone ── HTTPS/WSS over the tailnet ──► Tailscale Serve ──► 127.0.0.1:<port> (this gateway)
//!                                                              └─► daemon session (same dispatch as IPC)
//! ```
//!
//! * Off by default; binds only on loopback (`bind`). Tailscale Serve listens
//!   on the tailnet addresses, terminates TLS and decides which tailnet devices
//!   can reach it; pairing decides what a device may do. Another address only
//!   with `AURELIUS_REMOTE_BIND`, and with a warning in the log.
//! * A device pairs with a one-time code shown on the desktop and confirmed
//!   there, gets a long-lived device secret (stored on the phone in the
//!   Keychain, only its SHA-256 here) and trades it for 12-hour session tokens.
//! * The WebSocket carries the JSON of protocol 2.0 (`Request` in, `ServerMessage` out), one message per frame.
//!   Text streaming is converted to appends (`StreamAppend`).

mod auth;
pub mod bind;
mod http;
pub mod push;
pub mod tailscale;
mod web;
mod ws;

use aurelius_domain::{ClientId, EventNotice, HostId, PrincipalId};
use aurelius_protocol::remote::{Capability, PairingCode, PushConfig, PushStatus, RemoteClientView, RemoteStatus};
use aurelius_protocol::{ErrorCode, ProtocolError, Request, ServerMessage};
use aurelius_storage::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

/// Session lifetime; the phone renews with its device secret.
pub const SESSION_MS: i64 = 12 * 3_600_000;
/// Largest upload (photo, document) from a phone.
pub const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

/// Opens daemon sessions for paired devices (implemented by the daemon).
pub trait Dispatch: Send + Sync + 'static {
    /// Serve `requests` for `device` until the channel closes; write to `out`.
    fn open(&self, device: Device, requests: mpsc::Receiver<Request>, out: mpsc::Sender<ServerMessage>);
}

#[derive(Debug, Clone)]
pub struct Device {
    pub client_id: ClientId,
    pub name: String,
    pub capabilities: Vec<Capability>,
    /// Uploads from this device; `attach_file` may only point inside it.
    pub inbox: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Config {
    enabled: bool,
    #[serde(default)]
    push: PushIds,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PushIds {
    pub key_id: Option<String>,
    pub team_id: Option<String>,
    pub bundle_id: Option<String>,
}

const APNS_KEY_REF: &str = "remote/apns-key";

struct Server {
    addr: SocketAddr,
    stop: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

/// Open WebSockets of one device, and its kill switch (revoke/disable).
struct Live {
    connections: usize,
    kill: watch::Sender<bool>,
}

pub(crate) struct Inner {
    store: Arc<Store>,
    host_id: HostId,
    principal_id: PrincipalId,
    root: PathBuf,
    version: String,
    environment: String,
    dispatch: OnceLock<Arc<dyn Dispatch>>,
    config: Mutex<Config>,
    server: tokio::sync::Mutex<Option<Server>>,
    pairings: Mutex<auth::Pairings>,
    live: Mutex<HashMap<ClientId, Live>>,
    push: push::Pusher,
}

/// Remote access for one daemon. Cheap to clone.
#[derive(Clone)]
pub struct Hub {
    inner: Arc<Inner>,
}

fn validation(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::Validation, message)
}

fn internal(message: impl ToString) -> ProtocolError {
    ProtocolError::new(ErrorCode::Internal, message.to_string())
}

impl Hub {
    pub fn new(store: Arc<Store>, host_id: HostId, principal_id: PrincipalId, root: PathBuf, version: &str) -> Self {
        let config = std::fs::read(root.join("data").join("remote.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            inner: Arc::new(Inner {
                store,
                host_id,
                principal_id,
                root,
                version: version.into(),
                environment: aurelius_host::Environment::current().as_str().into(),
                dispatch: OnceLock::new(),
                config: Mutex::new(config),
                server: tokio::sync::Mutex::new(None),
                pairings: Mutex::new(auth::Pairings::default()),
                live: Mutex::new(HashMap::new()),
                push: push::Pusher::new(),
            }),
        }
    }

    pub fn set_dispatch(&self, dispatch: Arc<dyn Dispatch>) {
        let _ = self.inner.dispatch.set(dispatch);
    }

    /// Loopback port: `AURELIUS_REMOTE_PORT` (0 = any, tests), else 47613 (dev 47614).
    pub fn configured_port() -> u16 {
        if let Some(port) = std::env::var("AURELIUS_REMOTE_PORT").ok().and_then(|v| v.parse().ok()) {
            return port;
        }
        match aurelius_host::Environment::current() {
            aurelius_host::Environment::Dev => 47614,
            _ => 47613,
        }
    }

    /// Tailnet HTTPS port Tailscale Serve listens on (dev and production differ).
    pub fn https_port() -> u16 {
        match aurelius_host::Environment::current() {
            aurelius_host::Environment::Dev => 8444,
            _ => 8443,
        }
    }

    fn save_config(&self) -> Result<(), ProtocolError> {
        let config = self.inner.config.lock().unwrap().clone();
        let path = self.inner.root.join("data").join("remote.json");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&config).map_err(internal)?).map_err(internal)?;
        std::fs::rename(&tmp, &path).map_err(internal)
    }

    pub fn enabled(&self) -> bool {
        self.inner.config.lock().unwrap().enabled
    }

    /// At daemon start: bring the gateway back up when remote access was on.
    pub async fn start_if_enabled(&self) {
        if self.enabled() {
            if let Err(err) = self.start_server().await {
                tracing::warn!(error = %err, "remote gateway did not start");
            }
        }
    }

    /// The bound address, while running (tests use port 0).
    pub async fn local_addr(&self) -> Option<SocketAddr> {
        self.inner.server.lock().await.as_ref().map(|s| s.addr)
    }

    /// Where the gateway listens, or would listen: what Tailscale Serve targets.
    async fn target(&self) -> SocketAddr {
        match self.local_addr().await {
            Some(addr) => addr,
            None => SocketAddr::new(
                bind::from_env().unwrap_or(std::net::Ipv4Addr::LOCALHOST.into()),
                Self::configured_port(),
            ),
        }
    }

    async fn start_server(&self) -> Result<SocketAddr, ProtocolError> {
        let mut server = self.inner.server.lock().await;
        if let Some(running) = server.as_ref() {
            return Ok(running.addr);
        }
        // Loopback only (R06, MU3-07): Tailscale Serve is the single way in
        // from the tailnet. Another address only when set explicitly.
        let ip = bind::from_env().map_err(validation)?;
        let tailnet = if ip.is_loopback() {
            Vec::new()
        } else {
            let root = self.inner.root.clone();
            tokio::task::spawn_blocking(move || tailscale::own_ips(&root)).await.unwrap_or_default()
        };
        let listener = tokio::net::TcpListener::bind((ip, Self::configured_port()))
            .await
            .map_err(|e| validation(format!("The remote port is in use: {e}")))?;
        let addr = listener.local_addr().map_err(internal)?;
        let reach = bind::reach(addr.ip(), &tailnet);
        let (stop, stopped) = oneshot::channel::<()>();
        let app = http::router(self.clone());
        let task = tokio::spawn(async move {
            let served = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(async move {
                    let _ = stopped.await;
                })
                .await;
            if let Err(err) = served {
                tracing::warn!(error = %err, "remote gateway stopped");
            }
        });
        match bind::warning(addr, reach) {
            None => tracing::info!(%addr, "remote gateway listening on loopback"),
            Some(warning) => tracing::warn!(%addr, ?reach, "{warning}"),
        }
        *server = Some(Server { addr, stop, task });
        Ok(addr)
    }

    async fn stop_server(&self) {
        // Close every open WebSocket first; graceful shutdown waits for them.
        for live in self.inner.live.lock().unwrap().values() {
            let _ = live.kill.send(true);
        }
        if let Some(server) = self.inner.server.lock().await.take() {
            let _ = server.stop.send(());
            let _ = tokio::time::timeout(std::time::Duration::from_secs(3), server.task).await;
        }
    }

    /// Daemon stopping: close connections; the setting and Serve route stay.
    pub async fn shutdown(&self) {
        self.stop_server().await;
    }

    pub async fn enable(&self, viewer: Option<&str>) -> Result<RemoteStatus, ProtocolError> {
        let addr = self.start_server().await?;
        self.inner.config.lock().unwrap().enabled = true;
        self.save_config()?;
        let (https, root) = (Self::https_port(), self.inner.root.clone());
        let serve = tokio::task::spawn_blocking(move || tailscale::serve_on(&root, https, addr)).await.map_err(internal)?;
        let mut status = self.status(viewer).await;
        if let Err(err) = serve {
            status.error = Some(err);
        }
        Ok(status)
    }

    pub async fn disable(&self, viewer: Option<&str>) -> Result<RemoteStatus, ProtocolError> {
        self.inner.config.lock().unwrap().enabled = false;
        self.save_config()?;
        self.stop_server().await;
        let store = self.inner.store.clone();
        tokio::task::spawn_blocking(move || store.write(|tx| aurelius_storage::remote::revoke_all_sessions(tx)))
            .await
            .map_err(internal)?
            .map_err(internal)?;
        let (https, root) = (Self::https_port(), self.inner.root.clone());
        let serve = tokio::task::spawn_blocking(move || tailscale::serve_off(&root, https)).await.map_err(internal)?;
        let mut status = self.status(viewer).await;
        if let Err(err) = serve {
            status.error = Some(err);
        }
        Ok(status)
    }

    /// Settings → Remote access as `viewer` (the board member of the connection) sees it:
    /// their own devices and pairing requests and those of nobody, never another board member's.
    pub async fn status(&self, viewer: Option<&str>) -> RemoteStatus {
        let target = self.target().await;
        let port = target.port();
        let (https, root) = (Self::https_port(), self.inner.root.clone());
        let tailscale = tokio::task::spawn_blocking(move || tailscale::status(&root, https, target)).await.unwrap_or_default();
        let clients = self.list(viewer.map(str::to_owned), false).await;
        let url = tailscale.dns_name.as_ref().map(|dns| format!("https://{dns}:{https}"));
        RemoteStatus {
            enabled: self.enabled(),
            port,
            url,
            tailscale,
            clients,
            pending: self.inner.pairings.lock().unwrap().pending(viewer),
            push: self.push_status(),
            error: None,
        }
    }

    /// The devices `user` paired (`device.list`), newest first, with whether each is connected now.
    pub async fn devices(&self, user: &str) -> Vec<RemoteClientView> {
        self.list(Some(user.to_owned()), true).await
    }

    /// `own`: only `user`'s devices; else what the Remote access settings show `user`.
    async fn list(&self, user: Option<String>, own: bool) -> Vec<RemoteClientView> {
        use aurelius_storage::remote::{Devices, list_devices};
        let store = self.inner.store.clone();
        let mut clients = tokio::task::spawn_blocking(move || {
            store.read(|c| match (&user, own) {
                (Some(user), true) => list_devices(c, Devices::PairedBy(user)),
                (_, true) => Ok(Vec::new()),
                (user, false) => list_devices(c, Devices::VisibleTo(user.as_deref())),
            })
        })
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
        let live = self.inner.live.lock().unwrap();
        for client in &mut clients {
            client.connected = live.get(&client.id).is_some_and(|l| l.connections > 0);
        }
        clients
    }

    /// Whether `viewer` may manage this device: theirs, or a device of nobody.
    async fn may_manage(&self, client_id: ClientId, viewer: Option<&str>) -> Result<(), ProtocolError> {
        let store = self.inner.store.clone();
        let found = tokio::task::spawn_blocking(move || store.read(|c| aurelius_storage::remote::find_client(c, client_id)))
            .await
            .map_err(internal)?
            .map_err(internal)?;
        match found {
            Some(client) if !client.revoked && (client.paired_by.is_none() || client.paired_by.as_deref() == viewer) => Ok(()),
            _ => Err(ProtocolError::new(ErrorCode::NotFound, "This device does not exist.")),
        }
    }

    /// Where a phone should connect: the tailnet URL, or in dev without
    /// Tailscale the Android emulator's view of this PC's loopback.
    async fn phone_url(&self) -> Result<String, ProtocolError> {
        let status = self.status(None).await;
        if let Some(url) = status.url.filter(|_| status.tailscale.online) {
            return Ok(url);
        }
        if self.inner.environment == "dev" {
            return Ok(format!("http://10.0.2.2:{}", status.port));
        }
        Err(validation(
            status.tailscale.error.unwrap_or_else(|| "Tailscale is not connected on this PC.".into()),
        ))
    }

    /// The server's name as clients see it (SN-01): the chosen one, else the computer's.
    pub(crate) async fn server_name(&self) -> String {
        let store = self.inner.store.clone();
        tokio::task::spawn_blocking(move || aurelius_host::server_name(&store).name).await.unwrap_or_else(|_| aurelius_host::host_name())
    }

    /// A new pairing code for `viewer`: the device that enters it becomes theirs.
    pub async fn pairing_start(&self, viewer: Option<&str>) -> Result<PairingCode, ProtocolError> {
        if !self.enabled() {
            return Err(validation("Turn on remote access first."));
        }
        let url = self.phone_url().await?;
        let host_name = self.server_name().await;
        let (code, expires_at_ms) = self.inner.pairings.lock().unwrap().new_code(viewer);
        let payload = aurelius_protocol::remote::PairingPayload {
            v: 1,
            url,
            code: code.clone(),
            host_id: self.inner.host_id,
            host_name,
            env: self.inner.environment.clone(),
        };
        // Another computer reaches this one over the tailnet; in dev without
        // Tailscale, a second window on this PC uses loopback.
        let computer = aurelius_protocol::remote::PairingPayload {
            url: if payload.url.starts_with("http://10.0.2.2:") {
                bind::local_url(self.target().await)
            } else {
                payload.url.clone()
            },
            ..payload.clone()
        };
        Ok(PairingCode {
            code,
            qr: serde_json::to_string(&payload).map_err(internal)?,
            expires_at_ms,
            pairing_text: computer.to_text(),
        })
    }

    /// `owner`: the user confirmed full access for a computer that asked for
    /// it. A computer asking for it is refused without that confirmation; a
    /// phone never gets it.
    /// Only `viewer`'s own requests (or those of a code nobody asked for); the device
    /// belongs to whoever asked for the code, else to `viewer`.
    pub async fn pairing_decide(&self, pairing_id: &str, approve: bool, owner: bool, viewer: Option<&str>) -> Result<RemoteStatus, ProtocolError> {
        let request = self
            .inner
            .pairings
            .lock()
            .unwrap()
            .pending_request(pairing_id, viewer)
            .ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, "This pairing request expired. Start again."))?;
        if approve && request.owner && !owner {
            return Err(validation(format!(
                "{} asks for full access. Allow it as owner, or refuse.",
                request.device_name
            )));
        }
        if approve {
            let capabilities: Vec<Capability> =
                if request.owner && owner { Capability::OWNER.to_vec() } else { Capability::DEVICE_DEFAULT.to_vec() };
            let client_id = ClientId::new();
            let secret = auth::random_token();
            let verifier = auth::verifier(&secret);
            let store = self.inner.store.clone();
            let principal = self.inner.principal_id;
            let (name, platform) = (request.device_name.clone(), request.platform.clone());
            let paired_by = request.by.clone().or_else(|| viewer.map(str::to_owned));
            tokio::task::spawn_blocking(move || {
                store.write(move |tx| {
                    aurelius_storage::remote::create_client(tx, client_id, principal, &name, &platform, &verifier, &capabilities)?;
                    match &paired_by {
                        Some(user) => aurelius_storage::remote::set_paired_by(tx, client_id, user),
                        None => Ok(()),
                    }
                })
            })
            .await
            .map_err(internal)?
            .map_err(internal)?;
            tracing::info!(%client_id, device = %request.device_name, owner = request.owner && owner, "remote device paired");
            self.inner.pairings.lock().unwrap().approve(pairing_id, client_id, secret);
        } else {
            self.inner.pairings.lock().unwrap().reject(pairing_id);
        }
        Ok(self.status(viewer).await)
    }

    /// Revoke `viewer`'s own device or a device of nobody; another board member's is `not_found`.
    pub async fn revoke(&self, client_id: ClientId, viewer: Option<&str>) -> Result<RemoteStatus, ProtocolError> {
        self.may_manage(client_id, viewer).await?;
        let store = self.inner.store.clone();
        tokio::task::spawn_blocking(move || store.write(move |tx| aurelius_storage::remote::revoke_client(tx, client_id)))
            .await
            .map_err(internal)?
            .map_err(internal)?;
        self.disconnect(client_id);
        tracing::info!(%client_id, "remote device revoked");
        Ok(self.status(viewer).await)
    }

    /// Close every open connection of a device that was revoked.
    pub fn disconnect(&self, client_id: ClientId) {
        if let Some(live) = self.inner.live.lock().unwrap().get(&client_id) {
            let _ = live.kill.send(true);
        }
    }

    fn push_status(&self) -> PushStatus {
        let ids = self.inner.config.lock().unwrap().push.clone();
        let has_key = aurelius_host::secrets::get(APNS_KEY_REF).ok().flatten().is_some();
        PushStatus {
            apns: has_key && ids.key_id.is_some() && ids.team_id.is_some() && ids.bundle_id.is_some(),
            key_id: ids.key_id,
            team_id: ids.team_id,
            bundle_id: ids.bundle_id,
            last_error: self.inner.push.last_error(),
        }
    }

    pub async fn push_configure(&self, config: PushConfig, viewer: Option<&str>) -> Result<RemoteStatus, ProtocolError> {
        let trimmed = |s: &str| Some(s.trim().to_owned()).filter(|s| !s.is_empty());
        if let Some(key) = config.key_p8.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
            push::parse_p8(key).map_err(validation)?;
            aurelius_host::secrets::set(APNS_KEY_REF, key).map_err(|e| validation(format!("Could not store the key: {e}")))?;
        }
        {
            let mut current = self.inner.config.lock().unwrap();
            current.push = PushIds {
                key_id: trimmed(&config.key_id),
                team_id: trimmed(&config.team_id),
                bundle_id: trimmed(&config.bundle_id),
            };
        }
        self.save_config()?;
        Ok(self.status(viewer).await)
    }

    /// A test notification to `viewer`'s own device or a device of nobody.
    pub async fn push_test(&self, client_id: ClientId, viewer: Option<&str>) -> Result<(), ProtocolError> {
        self.may_manage(client_id, viewer).await?;
        let store = self.inner.store.clone();
        let target = tokio::task::spawn_blocking(move || store.read(aurelius_storage::remote::push_targets))
            .await
            .map_err(internal)?
            .map_err(internal)?
            .into_iter()
            .find(|t| t.client_id == client_id)
            .ok_or_else(|| validation("This device did not turn on notifications."))?;
        let alert = push::Alert {
            title: "Aurelius".into(),
            body: "Notifications from this PC work.".into(),
            data: serde_json::json!({ "target_type": "test" }),
            category: None,
        };
        self.deliver(&target, &alert).await.map_err(validation)
    }

    async fn deliver(&self, target: &aurelius_storage::remote::PushTarget, alert: &push::Alert) -> Result<(), String> {
        let ids = self.inner.config.lock().unwrap().push.clone();
        let key = aurelius_host::secrets::get(APNS_KEY_REF).ok().flatten();
        let outcome = self.inner.push.send(&ids, key.as_deref(), target, alert).await;
        if let Err(push::PushError::Unregistered) = &outcome {
            let store = self.inner.store.clone();
            let id = target.client_id;
            let _ = tokio::task::spawn_blocking(move || store.write(move |tx| aurelius_storage::remote::delete_push(tx, id))).await;
        }
        outcome.map_err(|e| e.to_string())
    }

    /// Push notifications for events that ask for the user (the same events
    /// that create Inbox notifications), to devices without an open WebSocket.
    pub fn start_push(&self, mut events: broadcast::Receiver<EventNotice>) {
        let hub = self.clone();
        tokio::spawn(async move {
            loop {
                let notice = match events.recv().await {
                    Ok(notice) => notice,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if !hub.enabled() {
                    continue;
                }
                let store = hub.inner.store.clone();
                let seq = notice.seq;
                let found = tokio::task::spawn_blocking(move || {
                    // A board member's own notification goes only to their devices (MU3-12 AC2).
                    store.read(|c| aurelius_storage::remote::pushes_for_seq(c, seq))
                })
                .await;
                let Ok(Ok(pushes)) = found else { continue };
                for (found, targets) in pushes {
                    let alert = push::Alert::for_notification(&found, notice.project_id);
                    for target in targets {
                        let connected = hub.inner.live.lock().unwrap().get(&target.client_id).is_some_and(|l| l.connections > 0);
                        if connected {
                            continue;
                        }
                        if let Err(err) = hub.deliver(&target, &alert).await {
                            tracing::warn!(client_id = %target.client_id, error = %err, "push failed");
                        }
                    }
                }
            }
        });
    }

    /// Register an open WebSocket; the guard unregisters it.
    fn connect(&self, client_id: ClientId) -> (watch::Receiver<bool>, LiveGuard) {
        let mut live = self.inner.live.lock().unwrap();
        let entry = live.entry(client_id).or_insert_with(|| Live { connections: 0, kill: watch::channel(false).0 });
        if *entry.kill.borrow() {
            // A previous revoke/disable; a fresh connection passed auth again.
            entry.kill = watch::channel(false).0;
        }
        entry.connections += 1;
        (entry.kill.subscribe(), LiveGuard { hub: self.clone(), client_id })
    }

    fn inbox(&self, client_id: ClientId) -> PathBuf {
        self.inner.root.join("remote-inbox").join(client_id.to_string())
    }
}

struct LiveGuard {
    hub: Hub,
    client_id: ClientId,
}

impl Drop for LiveGuard {
    fn drop(&mut self) {
        if let Some(live) = self.hub.inner.live.lock().unwrap().get_mut(&self.client_id) {
            live.connections = live.connections.saturating_sub(1);
        }
        let store = self.hub.inner.store.clone();
        let id = self.client_id;
        // Best effort; never block a runtime thread on the writer.
        std::thread::spawn(move || {
            let _ = store.write(move |tx| aurelius_storage::remote::touch(tx, id));
        });
    }
}
