//! The open connections of the board members' sessions, so ending a session
//! (sign out, revoke, a new password, disable, a revoked device) closes them at
//! once (MU3-04 AC2, MU3-06 AC3), and the session tokens themselves.
//!
//! A token is `aus_` and 32 random bytes (URL-safe base64), shown to the client
//! once; the database keeps only the SHA-256 of all of it
//! (`user_sessions.token_verifier`). The prefix makes a leaked one easy to spot.

use aurelius_domain::user::SignOutReason;
use aurelius_domain::{ClientId, UserId};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

/// What every session token starts with (Sentinel S1).
pub const TOKEN_PREFIX: &str = "aus_";

/// A new session token.
pub fn new_token() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS random source");
    format!("{TOKEN_PREFIX}{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

/// What the database keeps of a token.
pub fn verifier(token: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// One connection with a board member.
#[derive(Debug)]
struct Open {
    /// None: the first board member of the onboarding, kept on this connection only.
    session_id: Option<String>,
    user: UserId,
    client: Option<ClientId>,
    end: mpsc::UnboundedSender<SignOutReason>,
}

#[derive(Debug, Default)]
pub struct Hub {
    next: AtomicU64,
    open: Mutex<HashMap<u64, Open>>,
}

impl Hub {
    /// A number for a new connection.
    pub fn connection(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// The connection's board member and session now (none: forget it).
    pub fn set(&self, conn: u64, now: Option<(Option<String>, UserId, Option<ClientId>)>, end: &mpsc::UnboundedSender<SignOutReason>) {
        let mut open = self.open.lock().unwrap();
        match now {
            Some((session_id, user, client)) => {
                open.insert(conn, Open { session_id, user, client, end: end.clone() });
            }
            None => {
                open.remove(&conn);
            }
        }
    }

    pub fn forget(&self, conn: u64) {
        self.open.lock().unwrap().remove(&conn);
    }

    /// Tell every connection that matches (but `except`) that its session ended;
    /// they close. Returns how many.
    fn end(&self, matches: impl Fn(&Open) -> bool, reason: SignOutReason, except: Option<u64>) -> usize {
        let mut open = self.open.lock().unwrap();
        let ended: Vec<u64> = open.iter().filter(|(c, o)| Some(**c) != except && matches(o)).map(|(c, _)| *c).collect();
        for conn in &ended {
            if let Some(o) = open.remove(conn) {
                let _ = o.end.send(reason);
            }
        }
        ended.len()
    }

    pub fn end_session(&self, session_id: &str, reason: SignOutReason, except: Option<u64>) -> usize {
        self.end(|o| o.session_id.as_deref() == Some(session_id), reason, except)
    }

    /// Every connection of a board member, but those of the session `keep`.
    pub fn end_user(&self, user: UserId, keep: Option<&str>, reason: SignOutReason, except: Option<u64>) -> usize {
        self.end(|o| o.user == user && (keep.is_none() || o.session_id.as_deref() != keep), reason, except)
    }

    /// Every board member's connection from a device (it was revoked).
    pub fn end_client(&self, client: ClientId, reason: SignOutReason) -> usize {
        self.end(|o| o.client == Some(client), reason, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_32_random_bytes_and_only_its_sha_256_is_kept() {
        let (one, two) = (new_token(), new_token());
        assert_ne!(one, two);
        assert!(one.starts_with("aus_"), "{one}");
        assert_eq!(one.len(), 4 + 43, "the prefix and 32 bytes in URL-safe base64 without padding");
        assert_eq!(verifier(&one).len(), 64);
        assert_eq!(verifier(&one), verifier(&one));
        assert!(!verifier(&one).contains(&one));
    }

    #[test]
    fn ending_a_session_reaches_only_its_connections() {
        let hub = Hub::default();
        let (alex, robin) = (UserId::new(), UserId::new());
        let phone = ClientId::new();
        let mut ends = Vec::new();
        for (session, user, client) in [(Some("s1"), alex, None), (Some("s1"), alex, None), (Some("s2"), alex, Some(phone)), (Some("s3"), robin, None), (None, robin, None)] {
            let (tx, rx) = mpsc::unbounded_channel();
            let conn = hub.connection();
            hub.set(conn, Some((session.map(str::to_owned), user, client)), &tx);
            ends.push((conn, rx));
        }
        let ended = |ends: &mut Vec<(u64, mpsc::UnboundedReceiver<SignOutReason>)>| ends.iter_mut().map(|(_, rx)| rx.try_recv().ok()).collect::<Vec<_>>();
        // A sign-out of the first connection closes the other one of the same session.
        assert_eq!(hub.end_session("s1", SignOutReason::LoggedOut, Some(ends[0].0)), 1);
        assert_eq!(ended(&mut ends), [None, Some(SignOutReason::LoggedOut), None, None, None]);
        // Alex's new password keeps the session that changed it.
        hub.set(ends[0].0, Some((Some("s1".into()), alex, None)), &mpsc::unbounded_channel().0);
        assert_eq!(hub.end_user(alex, Some("s1"), SignOutReason::PasswordChanged, None), 1);
        assert_eq!(ended(&mut ends)[2], Some(SignOutReason::PasswordChanged));
        // Robin disabled: every connection, the onboarding one too.
        assert_eq!(hub.end_user(robin, None, SignOutReason::Disabled, None), 2);
        assert_eq!(hub.end_client(phone, SignOutReason::Revoked), 0, "already gone");
    }
}
