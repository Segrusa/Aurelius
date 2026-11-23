//! Secrets, verifiers and one-time pairing. Pairing state lives
//! in memory only: a code is valid for 5 minutes, single use, and dies after
//! five wrong guesses.

use aurelius_domain::ClientId;
use aurelius_protocol::remote::PairingRequestView;
use aurelius_storage::now_ms;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::Notify;

const CODE_MS: i64 = 5 * 60_000;
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const CODE_LEN: usize = 8;
const MAX_FAILURES: u32 = 5;
/// Pairing attempts per minute across all callers (behind Serve every caller is loopback).
const ATTEMPTS_PER_MINUTE: usize = 10;

/// 32 random bytes, URL-safe base64.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS random source");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// What the database keeps instead of a secret or token.
pub fn verifier(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

/// Compare without an early exit on the first differing byte.
pub fn same(a: &str, b: &str) -> bool {
    let (a, b) = (Sha256::digest(a.as_bytes()), Sha256::digest(b.as_bytes()));
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn new_code_text() -> String {
    let mut bytes = [0u8; CODE_LEN];
    getrandom::fill(&mut bytes).expect("OS random source");
    // 256 % 31 bias is negligible next to the 5-guess limit.
    bytes.iter().map(|b| CODE_ALPHABET[*b as usize % CODE_ALPHABET.len()] as char).collect()
}

/// Accept "k7m2-9qxa", "K7M2 9QXA" or "K7M29QXA".
pub fn normalize_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

struct ActiveCode {
    code: String,
    expires_at_ms: i64,
    failures: u32,
    /// The board member who asked for the code; the device becomes theirs.
    by: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PairState {
    Pending,
    Approved { client_id: ClientId, secret: String },
    Rejected,
}

#[derive(Clone)]
pub struct PairingRequest {
    pub id: String,
    pub device_name: String,
    pub platform: String,
    /// A computer asking for full access (owner).
    pub owner: bool,
    /// The board member whose code the device entered: only they see and decide the request.
    pub by: Option<String>,
    pub requested_at_ms: i64,
    pub expires_at_ms: i64,
    pub state: PairState,
    pub notify: Arc<Notify>,
}

impl PairingRequest {
    /// Whether `viewer` may see and decide this request.
    fn decided_by(&self, viewer: Option<&str>) -> bool {
        self.by.is_none() || self.by.as_deref() == viewer
    }
}

#[derive(Debug, PartialEq)]
pub enum ClaimError {
    TooManyAttempts,
    Invalid,
}

#[derive(Default)]
pub struct Pairings {
    code: Option<ActiveCode>,
    requests: HashMap<String, PairingRequest>,
    attempts: VecDeque<i64>,
}

impl Pairings {
    fn expire(&mut self) {
        let now = now_ms();
        if self.code.as_ref().is_some_and(|c| c.expires_at_ms <= now) {
            self.code = None;
        }
        self.requests.retain(|_, r| r.expires_at_ms > now);
    }

    /// A new code replaces any earlier one. `by`: the board member who asked for it.
    pub fn new_code(&mut self, by: Option<&str>) -> (String, i64) {
        let code = new_code_text();
        let expires_at_ms = now_ms() + CODE_MS;
        self.code = Some(ActiveCode { code: code.clone(), expires_at_ms, failures: 0, by: by.map(str::to_owned) });
        (code, expires_at_ms)
    }

    /// A device entered `code`. On success the code is used up and a pending
    /// request waits for the desktop.
    pub fn claim(&mut self, code: &str, device_name: &str, platform: &str, owner: bool) -> Result<PairingRequest, ClaimError> {
        self.expire();
        let now = now_ms();
        while self.attempts.front().is_some_and(|t| *t < now - 60_000) {
            self.attempts.pop_front();
        }
        if self.attempts.len() >= ATTEMPTS_PER_MINUTE {
            return Err(ClaimError::TooManyAttempts);
        }
        self.attempts.push_back(now);
        let Some(active) = self.code.as_mut() else { return Err(ClaimError::Invalid) };
        if !same(&active.code, &normalize_code(code)) {
            active.failures += 1;
            if active.failures >= MAX_FAILURES {
                self.code = None;
            }
            return Err(ClaimError::Invalid);
        }
        let by = self.code.take().and_then(|c| c.by);
        let request = PairingRequest {
            id: uuid::Uuid::now_v7().simple().to_string(),
            device_name: device_name.chars().take(64).collect(),
            platform: platform.into(),
            owner,
            by,
            requested_at_ms: now,
            expires_at_ms: now + CODE_MS,
            state: PairState::Pending,
            notify: Arc::new(Notify::new()),
        };
        self.requests.insert(request.id.clone(), request.clone());
        Ok(request)
    }

    /// The requests waiting for `viewer`: those of their own codes and of codes nobody asked for.
    pub fn pending(&mut self, viewer: Option<&str>) -> Vec<PairingRequestView> {
        self.expire();
        let mut out: Vec<_> = self
            .requests
            .values()
            .filter(|r| matches!(r.state, PairState::Pending) && r.decided_by(viewer))
            .map(|r| PairingRequestView {
                pairing_id: r.id.clone(),
                device_name: r.device_name.clone(),
                platform: r.platform.clone(),
                requested_at_ms: r.requested_at_ms,
                owner: r.owner,
            })
            .collect();
        out.sort_by_key(|r| r.requested_at_ms);
        out
    }

    /// A waiting request `viewer` may decide; another board member's is not there for them.
    pub fn pending_request(&mut self, id: &str, viewer: Option<&str>) -> Option<PairingRequest> {
        self.expire();
        self.requests.get(id).filter(|r| matches!(r.state, PairState::Pending) && r.decided_by(viewer)).cloned()
    }

    pub fn approve(&mut self, id: &str, client_id: ClientId, secret: String) {
        if let Some(request) = self.requests.get_mut(id) {
            request.state = PairState::Approved { client_id, secret };
            request.notify.notify_waiters();
        }
    }

    pub fn reject(&mut self, id: &str) {
        if let Some(request) = self.requests.get_mut(id) {
            request.state = PairState::Rejected;
            request.notify.notify_waiters();
        }
    }

    /// The device polls. An approved secret is handed out exactly once.
    pub fn poll(&mut self, id: &str) -> Option<(PairState, Arc<Notify>)> {
        self.expire();
        let request = self.requests.get(id)?;
        let state = request.state.clone();
        let notify = request.notify.clone();
        if !matches!(state, PairState::Pending) {
            self.requests.remove(id);
        }
        Some((state, notify))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_single_use_and_dies_after_five_wrong_guesses() {
        let mut p = Pairings::default();
        let (code, _) = p.new_code(None);
        let pretty = format!("{}-{}", &code[..4], code[4..].to_lowercase());
        let request = p.claim(&pretty, "iphone", "ios", false).unwrap();
        assert_eq!(p.claim(&code, "other", "ios", false).err(), Some(ClaimError::Invalid), "used up");
        assert_eq!(p.pending(None).len(), 1);

        let (code, _) = p.new_code(None);
        for _ in 0..MAX_FAILURES {
            assert_eq!(p.claim("WRONGWRO", "x", "ios", false).err(), Some(ClaimError::Invalid));
        }
        assert_eq!(p.claim(&code, "x", "ios", false).err(), Some(ClaimError::Invalid), "code burned after 5 failures");

        p.approve(&request.id, ClientId::new(), "secret".into());
        assert!(matches!(p.poll(&request.id), Some((PairState::Approved { .. }, _))));
        assert!(p.poll(&request.id).is_none(), "the secret is handed out once");
    }

    #[test]
    fn attempts_are_rate_limited() {
        let mut p = Pairings::default();
        p.new_code(None);
        let mut limited = false;
        for _ in 0..20 {
            p.new_code(None);
            if matches!(p.claim("AAAAAAAA", "x", "ios", false), Err(ClaimError::TooManyAttempts)) {
                limited = true;
            }
        }
        assert!(limited);
    }

    #[test]
    fn verifier_hides_the_secret() {
        let secret = random_token();
        assert_eq!(secret.len(), 43);
        assert_ne!(verifier(&secret), secret);
        assert!(same(&secret, &secret.clone()));
        assert!(!same(&secret, "x"));
    }

    #[test]
    fn a_request_is_seen_and_decided_only_by_whoever_asked_for_the_code() {
        let mut p = Pairings::default();
        let (code, _) = p.new_code(Some("alex"));
        let request = p.claim(&code, "Alex's iPhone", "ios", false).unwrap();
        assert_eq!(request.by.as_deref(), Some("alex"));
        assert_eq!(p.pending(Some("alex")).len(), 1);
        assert!(p.pending(Some("bea")).is_empty());
        assert!(p.pending(None).is_empty());
        assert!(p.pending_request(&request.id, Some("bea")).is_none());
        assert!(p.pending_request(&request.id, Some("alex")).is_some());
    }
}
