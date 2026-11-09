//! The vault (feature `vault`): its reads and writes, answered here before the
//! host pipeline, so a value never reaches the pipeline's receipts, request
//! hashes or events (MU4-05 AC2, Sentinel SEC0 E5). Board members with a
//! session only. The work is
//! in `crate::vault`.

use super::users::Attempts;
use super::{Peer, app_task, internal, storage_task, writes};
use crate::Shared;
use crate::vault::{self, BoardActor};
use aurelius_protocol::{ErrorCode, HostResult, HostWrite, KeyExportParams, Method, ProtocolError, Receipt, ResponseBody, WriteResult};
use aurelius_domain::CommandId;
use std::sync::Arc;

fn board_member(peer: &Peer) -> Result<aurelius_domain::UserId, ProtocolError> {
    match (peer.session.as_ref(), peer.user) {
        (Some(session), Some(user)) if !session.must_change_password => Ok(user),
        (Some(_), Some(_)) => Err(ProtocolError::new(ErrorCode::PasswordChangeRequired, "Change your temporary password first.")),
        _ => Err(ProtocolError::new(ErrorCode::BoardMemberRequired, "Only a signed-in board member uses the vault.")),
    }
}

/// The board member with their session and its device ("Via" in the audit).
async fn actor(shared: &Arc<Shared>, peer: &Peer) -> Result<BoardActor, ProtocolError> {
    let user = board_member(peer)?;
    let session_id = peer.session.as_ref().and_then(|s| s.id.clone());
    let via = match session_id.clone() {
        Some(id) => {
            let store = shared.store().clone();
            storage_task(move || store.read(|c| aurelius_storage::user_sessions::get(c, &id))).await?.map(|s| s.device_name)
        }
        None => None,
    };
    Ok(BoardActor { user, session_id, via })
}

pub(super) async fn read(shared: &Arc<Shared>, peer: &Peer, method: Method) -> Result<ResponseBody, ProtocolError> {
    let by = actor(shared, peer).await?;
    let s = shared.clone();
    match method {
        Method::VaultList { project_id } => app_task(move || {
            let keys = s.vault.keys().ok().flatten();
            s.store().read(|c| vault::list(c, keys.as_deref(), project_id)).map(ResponseBody::Vault).map_err(vault::storage)
        })
        .await,
        Method::VaultReveal { item_id, purpose } => app_task(move || {
            let Some(keys) = s.vault.keys().map_err(|_| undecryptable())? else { return Err(undecryptable()) };
            s.store().write(move |tx| vault::reveal(tx, &keys, item_id, purpose, &by)).map_err(vault::storage)?.map(ResponseBody::VaultValue)
        })
        .await,
        Method::VaultAudit { project_id, all, item_id, before, limit } => {
            app_task(move || s.store().read(|c| vault::audit(c, project_id, all, item_id, before, limit)).map(ResponseBody::VaultAudit).map_err(vault::storage)).await
        }
        _ => Err(internal("not a vault read")),
    }
}

fn undecryptable() -> ProtocolError {
    ProtocolError::new(ErrorCode::VaultUndecryptable, "The vault key of this server is missing or does not fit.")
}

/// `vault.key_export` (E9): a session that needs no new password, the
/// password asked again (counted like a sign-in), then the key.
pub(super) async fn key_export(shared: &Arc<Shared>, peer: &Peer, attempts: &mut Attempts, params: KeyExportParams) -> Result<ResponseBody, ProtocolError> {
    let by = actor(shared, peer).await?;
    super::users::confirm_password(shared, peer, attempts, params.password).await?;
    let s = shared.clone();
    let (export, notices) = app_task(move || vault::key_export(&s, &by)).await?;
    shared.publish(notices);
    Ok(ResponseBody::VaultKey(export))
}

pub(super) async fn write(shared: &Arc<Shared>, peer: &Peer, request_id: u64, key: CommandId, expected_version: Option<u64>, write: HostWrite) -> Result<ResponseBody, ProtocolError> {
    let by = actor(shared, peer).await?;
    let meta = writes::context(shared, peer, request_id).metadata();
    let s = shared.clone();
    let written = app_task(move || match write {
        HostWrite::VaultCreate(params) => vault::create(&s, key, expected_version, params, by, meta),
        HostWrite::VaultUpdate(params) => vault::update(&s, key, expected_version, params, by, meta),
        HostWrite::VaultArchive { item_id } => vault::archive(&s, key, item_id, by, meta),
        _ => Err(internal("not a vault write")),
    })
    .await?;
    shared.publish(written.notices);
    // A replay answers without the value (as its receipt keeps none).
    let result = if written.replayed { HostResult::VaultItem(written.item).redacted() } else { HostResult::VaultItem(written.item) };
    Ok(ResponseBody::Receipt(Receipt { idempotency_key: key, committed_seq: written.seq, replayed: written.replayed, result: WriteResult::Host(result) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R1 m3: only a signed-in board member without a temporary password
    /// (behind the gate, which already turns the others away: the last line).
    #[test]
    fn only_a_signed_in_board_member_uses_the_vault() {
        let session = |must_change_password| super::super::users::PeerSession { id: None, must_change_password, expires_at_ms: i64::MAX, seen_at_ms: 0, checked_at_ms: 0 };
        let user = aurelius_domain::UserId::new();
        let mut peer = Peer::local();
        assert_eq!(board_member(&peer).unwrap_err().code, ErrorCode::BoardMemberRequired, "no session");
        peer.session = Some(session(false));
        assert_eq!(board_member(&peer).unwrap_err().code, ErrorCode::BoardMemberRequired, "a session without a board member");
        peer.user = Some(user);
        assert_eq!(board_member(&peer).unwrap(), user);
        peer.session = Some(session(true));
        assert_eq!(board_member(&peer).unwrap_err().code, ErrorCode::PasswordChangeRequired);
    }
}
