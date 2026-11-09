//! Remote access glue: the gateway opens ordinary daemon sessions
//! for paired devices; the desktop manages it through the `remote.*` writes.

use crate::Shared;
use crate::api::{self, Peer};
use aurelius_domain::ClientId;
use aurelius_protocol::remote::{PairingCode, PushConfig, RemoteStatus};
use aurelius_protocol::{ProtocolError, Request, ServerMessage};
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

pub async fn status(shared: &Arc<Shared>) -> RemoteStatus {
    shared.remote.status().await
}

pub async fn enable(shared: &Arc<Shared>) -> Result<RemoteStatus, ProtocolError> {
    shared.remote.enable().await
}

pub async fn disable(shared: &Arc<Shared>) -> Result<RemoteStatus, ProtocolError> {
    shared.remote.disable().await
}

pub async fn pairing_start(shared: &Arc<Shared>) -> Result<PairingCode, ProtocolError> {
    shared.remote.pairing_start().await
}

pub async fn pairing_decide(shared: &Arc<Shared>, pairing_id: &str, approve: bool, owner: bool) -> Result<RemoteStatus, ProtocolError> {
    shared.remote.pairing_decide(pairing_id, approve, owner).await
}

pub async fn revoke(shared: &Arc<Shared>, client_id: ClientId) -> Result<RemoteStatus, ProtocolError> {
    shared.remote.revoke(client_id).await
}

pub async fn push_configure(shared: &Arc<Shared>, config: PushConfig) -> Result<RemoteStatus, ProtocolError> {
    shared.remote.push_configure(config).await
}

pub async fn push_test(shared: &Arc<Shared>, client_id: ClientId) -> Result<(), ProtocolError> {
    shared.remote.push_test(client_id).await
}
