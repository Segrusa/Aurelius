//! Remote access glue: the gateway opens ordinary daemon sessions
//! for paired devices; the desktop manages it through the `remote.*` writes.

use crate::Shared;
use crate::api::{self, Peer};
use aurelius_domain::ClientId;
use aurelius_protocol::remote::{DevicesSnapshot, PairingCode, PushConfig, RemoteStatus};
use aurelius_protocol::{ErrorCode, ProtocolError, Request, ServerMessage};
use aurelius_remote_gateway::{Device, Dispatch};
use std::sync::{Arc, Weak};
use tokio::sync::mpsc;

/// Weak: the hub lives inside `Shared`.
struct Sessions(Weak<Shared>);

impl Dispatch for Sessions {
    fn open(&self, device: Device, requests: mpsc::Receiver<Request>, out: mpsc::Sender<ServerMessage>) {
        let Some(shared) = self.0.upgrade() else { return };
        let peer = Peer::remote(device.client_id, device.capabilities, device.inbox);
        tokio::spawn(api::run(shared, peer, requests, out));
    }

}

/// After `Shared` exists: connect the gateway to sessions and events, and
/// start it again when remote access was on before.
pub async fn start(shared: &Arc<Shared>) {
    shared.remote.set_dispatch(Arc::new(Sessions(Arc::downgrade(shared))));
    // A copy of the data folder (or a test instance) shares the paired phones
    // of the real one: only this host's own data folder pushes to them.
    if shared.own_root {
        shared.remote.start_push(shared.events.subscribe());
    } else {
        tracing::info!("not this host's default data folder: no push notifications");
    }
    shared.remote.start_if_enabled().await;
}

// `viewer`: the board member of the connection. Remote access shows them only their own
// devices and pairing requests and those of nobody (feature `own_devices`).

pub async fn status(shared: &Arc<Shared>, viewer: Option<&str>) -> RemoteStatus {
    shared.remote.status(viewer).await
}

pub async fn enable(shared: &Arc<Shared>, viewer: Option<&str>) -> Result<RemoteStatus, ProtocolError> {
    shared.remote.enable(viewer).await
}

pub async fn disable(shared: &Arc<Shared>, viewer: Option<&str>) -> Result<RemoteStatus, ProtocolError> {
    shared.remote.disable(viewer).await
}

pub async fn pairing_start(shared: &Arc<Shared>, viewer: Option<&str>) -> Result<PairingCode, ProtocolError> {
    shared.remote.pairing_start(viewer).await
}

pub async fn pairing_decide(shared: &Arc<Shared>, pairing_id: &str, approve: bool, owner: bool, viewer: Option<&str>) -> Result<RemoteStatus, ProtocolError> {
    shared.remote.pairing_decide(pairing_id, approve, owner, viewer).await
}

pub async fn revoke(shared: &Arc<Shared>, client_id: ClientId, viewer: Option<&str>) -> Result<RemoteStatus, ProtocolError> {
    shared.remote.revoke(client_id, viewer).await
}

pub async fn push_configure(shared: &Arc<Shared>, config: PushConfig, viewer: Option<&str>) -> Result<RemoteStatus, ProtocolError> {
    shared.remote.push_configure(config, viewer).await
}

pub async fn push_test(shared: &Arc<Shared>, client_id: ClientId, viewer: Option<&str>) -> Result<(), ProtocolError> {
    shared.remote.push_test(client_id, viewer).await
}

/// `device.list`: the devices `user` paired.
pub async fn devices(shared: &Arc<Shared>, user: &str) -> DevicesSnapshot {
    DevicesSnapshot { devices: shared.remote.devices(user).await }
}

/// `device.revoke`: only one of `user`'s own devices; its sessions end and its connections close.
pub async fn revoke_own(shared: &Arc<Shared>, client_id: ClientId, user: &str) -> Result<(), ProtocolError> {
    if !shared.remote.devices(user).await.iter().any(|d| d.id == client_id) {
        return Err(ProtocolError::new(ErrorCode::NotFound, "This device does not exist."));
    }
    shared.remote.revoke(client_id, Some(user)).await?;
    shared.sessions.end_client(client_id, aurelius_domain::user::SignOutReason::Revoked);
    Ok(())
}
