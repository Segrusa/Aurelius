//! Paired devices, their sessions and push registrations.
//! Only verifiers are stored; the gateway hashes what a device presents.

use crate::{StorageError, now_ms};
use aurelius_domain::{ClientId, PrincipalId};
use aurelius_domain::remote::{Capability, RemoteClientView};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

#[derive(Debug, Clone)]
pub struct RemoteClient {
    pub id: ClientId,
    pub name: String,
    pub platform: String,
    pub credential_verifier: String,
    pub capabilities: Vec<Capability>,
    pub created_at_ms: i64,
    pub last_seen_ms: Option<i64>,
    pub revoked: bool,
    /// The board member who last logged in on it (migration 21).
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PushTarget {
    pub client_id: ClientId,
    /// apns | fcm
    pub platform: String,
    pub token: String,
    /// sandbox | production
    pub environment: String,
}

fn parse_id(text: String, col: usize) -> rusqlite::Result<ClientId> {
    ClientId::parse(&text).map_err(|e| rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, Box::new(e)))
}

fn parse_caps(json: &str) -> Vec<Capability> {
    serde_json::from_str::<Vec<String>>(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|c| Capability::parse(c))
        .collect()
}

fn client_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RemoteClient> {
    Ok(RemoteClient {
        id: parse_id(r.get(0)?, 0)?,
        name: r.get(1)?,
        platform: r.get(2)?,
        credential_verifier: r.get(3)?,
        capabilities: parse_caps(&r.get::<_, String>(4)?),
        created_at_ms: r.get(5)?,
        last_seen_ms: r.get(6)?,
        revoked: r.get::<_, Option<i64>>(7)?.is_some(),
        user_id: r.get(8)?,
    })
}

const SELECT: &str =
    "SELECT id, name, platform, credential_verifier, capabilities_json, created_at_ms, last_seen_ms, revoked_at_ms, user_id FROM remote_clients";

pub fn create_client(
    tx: &Transaction<'_>,
    id: ClientId,
    principal_id: PrincipalId,
    name: &str,
    platform: &str,
    credential_verifier: &str,
    capabilities: &[Capability],
) -> Result<(), StorageError> {
    let caps: Vec<&str> = capabilities.iter().map(|c| c.as_str()).collect();
    tx.execute(
        "INSERT INTO remote_clients(id, principal_id, name, platform, credential_verifier, capabilities_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id.to_string(), principal_id.to_string(), name, platform, credential_verifier, serde_json::to_string(&caps)?, now_ms()],
    )?;
    Ok(())
}

pub fn find_client(conn: &Connection, id: ClientId) -> Result<Option<RemoteClient>, StorageError> {
    Ok(conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id.to_string()], client_row).optional()?)
}

/// Active (not revoked) devices, newest first, as the desktop shows them.
/// `connected` is filled in by the gateway.
pub fn list_clients(conn: &Connection) -> Result<Vec<RemoteClientView>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE revoked_at_ms IS NULL ORDER BY created_at_ms DESC"
    ))?;
    let clients = stmt.query_map([], client_row)?.collect::<Result<Vec<_>, _>>()?;
    let mut push = conn.prepare("SELECT 1 FROM push_registrations WHERE client_id = ?1")?;
    clients
        .into_iter()
        .map(|c| {
            Ok(RemoteClientView {
                push: push.exists([c.id.to_string()])?,
                id: c.id,
                name: c.name,
                platform: c.platform,
                capabilities: c.capabilities,
                created_at_ms: c.created_at_ms,
                last_seen_ms: c.last_seen_ms,
                connected: false,
            })
        })
        .collect()
}

/// Revoke a device: its sessions end and its push registration is removed.
/// Returns false when it was unknown or already revoked.
pub fn revoke_client(tx: &Transaction<'_>, id: ClientId) -> Result<bool, StorageError> {
    let now = now_ms();
    let changed = tx.execute("UPDATE remote_clients SET revoked_at_ms = ?2 WHERE id = ?1 AND revoked_at_ms IS NULL", params![id.to_string(), now])?;
    tx.execute("UPDATE remote_sessions SET revoked_at_ms = ?2 WHERE client_id = ?1 AND revoked_at_ms IS NULL", params![id.to_string(), now])?;
    crate::user_sessions::revoke_for_client(tx, id, now)?;
    tx.execute("DELETE FROM push_registrations WHERE client_id = ?1", [id.to_string()])?;
    Ok(changed > 0)
}

pub fn touch(tx: &Transaction<'_>, id: ClientId) -> Result<(), StorageError> {
    tx.execute("UPDATE remote_clients SET last_seen_ms = ?2 WHERE id = ?1", params![id.to_string(), now_ms()])?;
    Ok(())
}

pub fn create_session(tx: &Transaction<'_>, id: &str, client_id: ClientId, token_verifier: &str, expires_at_ms: i64) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO remote_sessions(id, client_id, token_verifier, created_at_ms, expires_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, client_id.to_string(), token_verifier, now_ms(), expires_at_ms],
    )?;
    // Expired sessions are useless; keep the table small.
    tx.execute("DELETE FROM remote_sessions WHERE expires_at_ms < ?1", [now_ms() - 86_400_000])?;
    Ok(())
}

/// The device behind a valid session token: not expired, not revoked, and
/// its device not revoked either.
pub fn session_client(conn: &Connection, token_verifier: &str, now_ms: i64) -> Result<Option<RemoteClient>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT c.id, c.name, c.platform, c.credential_verifier, c.capabilities_json, c.created_at_ms, c.last_seen_ms, c.revoked_at_ms, c.user_id
             FROM remote_sessions s JOIN remote_clients c ON c.id = s.client_id
             WHERE s.token_verifier = ?1 AND s.revoked_at_ms IS NULL AND s.expires_at_ms > ?2 AND c.revoked_at_ms IS NULL",
            params![token_verifier, now_ms],
            client_row,
        )
        .optional()?)
}

/// Remote access turned off: every session ends; devices log in again later.
pub fn revoke_all_sessions(tx: &Transaction<'_>) -> Result<usize, StorageError> {
    Ok(tx.execute("UPDATE remote_sessions SET revoked_at_ms = ?1 WHERE revoked_at_ms IS NULL", [now_ms()])?)
}

pub fn set_push(tx: &Transaction<'_>, client_id: ClientId, platform: &str, token: &str, environment: &str) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO push_registrations(client_id, platform, token, environment, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(client_id) DO UPDATE SET platform = excluded.platform, token = excluded.token,
           environment = excluded.environment, updated_at_ms = excluded.updated_at_ms",
        params![client_id.to_string(), platform, token, environment, now_ms()],
    )?;
    Ok(())
}

pub fn delete_push(tx: &Transaction<'_>, client_id: ClientId) -> Result<(), StorageError> {
    tx.execute("DELETE FROM push_registrations WHERE client_id = ?1", [client_id.to_string()])?;
    Ok(())
}

pub fn push_targets(conn: &Connection) -> Result<Vec<PushTarget>, StorageError> {
    push_targets_for(conn, None)
}

/// The devices a notification for `user` may reach: only those of that board
/// member (`remote_clients.user_id`); `None` (a notification for everyone): all.
pub fn push_targets_for(conn: &Connection, user: Option<&str>) -> Result<Vec<PushTarget>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT p.client_id, p.platform, p.token, p.environment FROM push_registrations p
         JOIN remote_clients c ON c.id = p.client_id WHERE c.revoked_at_ms IS NULL AND (?1 IS NULL OR c.user_id = ?1)",
    )?;
    let rows = stmt
        .query_map([user], |r| {
            Ok(PushTarget { client_id: parse_id(r.get(0)?, 0)?, platform: r.get(1)?, token: r.get(2)?, environment: r.get(3)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The notification the event at `seq` made and the devices its push goes to:
/// a board member's own only to that board member's, one for everyone to all
/// (MU3-12 AC2, R2 M2). `None`: the event made no notification.
pub fn push_for_seq(conn: &Connection, seq: u64) -> Result<Option<(crate::timeline::FoundNotification, Vec<PushTarget>)>, StorageError> {
    let Some(found) = crate::timeline::notification_at_seq(conn, seq)? else { return Ok(None) };
    let targets = push_targets_for(conn, found.user_id.as_deref())?;
    Ok(Some((found, targets)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

    #[test]
    fn revoked_device_loses_sessions_and_push() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let id = ClientId::new();
        store
            .write(move |tx| {
                create_client(tx, id, PrincipalId::new(), "my-iphone", "ios", "v", &Capability::DEVICE_DEFAULT)?;
                create_session(tx, "s1", id, "tok", now_ms() + 60_000)?;
                set_push(tx, id, "apns", "abc", "sandbox")
            })
            .unwrap();
        let found = store.read(|c| session_client(c, "tok", now_ms())).unwrap().unwrap();
        assert_eq!((found.id, found.capabilities.len()), (id, 4));
        assert!(store.read(|c| session_client(c, "tok", now_ms() + 120_000)).unwrap().is_none(), "expired");
        let listed = store.read(list_clients).unwrap();
        assert!(listed[0].push);

        assert!(store.write(move |tx| revoke_client(tx, id)).unwrap());
        assert!(store.read(|c| session_client(c, "tok", now_ms())).unwrap().is_none());
        assert!(store.read(push_targets).unwrap().is_empty());
        assert!(store.read(list_clients).unwrap().is_empty());
        assert!(!store.write(move |tx| revoke_client(tx, id)).unwrap(), "second revoke is a no-op");
    }

    #[test]
    fn computers_pair_as_owners_and_the_rebuild_keeps_every_row() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let (phone, mac) = (ClientId::new(), ClientId::new());
        store
            .write(move |tx| {
                create_client(tx, phone, PrincipalId::new(), "iphone", "ios", "v1", &Capability::DEVICE_DEFAULT)?;
                create_client(tx, mac, PrincipalId::new(), "macbook", "macos", "v2", &Capability::OWNER)?;
                create_session(tx, "s1", phone, "tok", now_ms() + 60_000)?;
                set_push(tx, phone, "apns", "abc", "sandbox")
            })
            .unwrap();
        let found = store.read(list_clients).unwrap();
        let owner = found.iter().find(|c| c.id == mac).unwrap();
        assert!(owner.capabilities.contains(&Capability::Owner), "owner survives the stored JSON");

        // Every row is still there.
        assert_eq!(store.read(list_clients).unwrap().len(), 2);
        assert_eq!(store.read(|c| session_client(c, "tok", now_ms())).unwrap().map(|c| c.id), Some(phone));
        assert_eq!(store.read(push_targets).unwrap().len(), 1);
        let orphan = store.write(|tx| create_session(tx, "s2", ClientId::new(), "tok2", now_ms() + 60_000));
        assert!(orphan.is_err(), "the session still needs a paired device");
        let tablet = store.write(|tx| create_client(tx, ClientId::new(), PrincipalId::new(), "x", "tablet-os", "v", &[]));
        assert!(tablet.is_err(), "unknown platforms stay out");
    }
}
