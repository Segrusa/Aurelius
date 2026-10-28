//! Webhooks of app keys (feature `webhooks`): the pure rules of how Aurelius
//! posts events to an app. The signature, the retry schedule and the event
//! names are a contract with the receiver (see docs/aurelius-mcp.md), so they
//! are fixed here and tested against known vectors.
//!
//! One delivery is one POST with a JSON body. Its headers:
//!
//! * `X-Aurelius-Delivery`: the delivery id; the same on every retry, so the
//!   receiver can ignore a delivery it already handled (idempotency).
//! * `X-Aurelius-Event`: the event name, for example `task.finished`.
//! * `X-Aurelius-Timestamp`: unix time in seconds when this attempt was sent;
//!   new on every attempt.
//! * `X-Aurelius-Attempt`: 1 for the first attempt.
//! * `X-Aurelius-Signature`: `v1=<hex>`, the HMAC-SHA256 of
//!   `"{timestamp}.{delivery_id}.{body}"` with the webhook secret as key.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SECRET_PREFIX: &str = "whsec_";
pub const HEADER_DELIVERY: &str = "X-Aurelius-Delivery";
pub const HEADER_EVENT: &str = "X-Aurelius-Event";
pub const HEADER_TIMESTAMP: &str = "X-Aurelius-Timestamp";
pub const HEADER_ATTEMPT: &str = "X-Aurelius-Attempt";
pub const HEADER_SIGNATURE: &str = "X-Aurelius-Signature";
pub const SIGNATURE_VERSION: &str = "v1";

/// A receiver should refuse a timestamp further than this from its own clock.
pub const TOLERANCE_SECS: i64 = 300;
/// How long one attempt may take before it counts as failed.
pub const TIMEOUT_SECS: u64 = 10;
/// Attempts in all (the first plus the retries) before a delivery is given up.
pub const MAX_ATTEMPTS: u32 = 8;
/// Wait before the second attempt; it doubles after each failure.
pub const BACKOFF_BASE_MS: i64 = 5_000;
/// The longest wait between two attempts.
pub const BACKOFF_CAP_MS: i64 = 3_600_000;
/// Longest text of a message in an event.
pub const TEXT_MAX_CHARS: usize = 6_000;

/// The events an app can get.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub enum WebhookEvent {
    /// A manager answered a message of this app (`ask_management`).
    #[serde(rename = "manager.replied")]
    ManagerReplied,
    /// A manager asks the user a question or a decision.
    #[serde(rename = "question.for_you")]
    QuestionForYou,
    /// A task ended: succeeded, failed or cancelled.
    #[serde(rename = "task.finished")]
    TaskFinished,
    /// A task waits: for the user, a colleague, a quota or a reply.
    #[serde(rename = "task.waiting")]
    TaskWaiting,
}

impl WebhookEvent {
    pub const ALL: [WebhookEvent; 4] = [WebhookEvent::ManagerReplied, WebhookEvent::QuestionForYou, WebhookEvent::TaskFinished, WebhookEvent::TaskWaiting];

    pub fn as_str(self) -> &'static str {
        match self {
            WebhookEvent::ManagerReplied => "manager.replied",
            WebhookEvent::QuestionForYou => "question.for_you",
            WebhookEvent::TaskFinished => "task.finished",
            WebhookEvent::TaskWaiting => "task.waiting",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        WebhookEvent::ALL.into_iter().find(|e| e.as_str() == text)
    }
}

/// HMAC-SHA256 (RFC 2104) built on SHA-256.
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        key_block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(key_block.map(|b| b ^ 0x36));
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(key_block.map(|b| b ^ 0x5c));
    outer.update(inner);
    outer.finalize().into()
}

/// What is signed: `"{timestamp}.{delivery_id}.{body}"`.
pub fn string_to_sign(timestamp_secs: i64, delivery_id: &str, body: &str) -> String {
    format!("{timestamp_secs}.{delivery_id}.{body}")
}

/// The value of `X-Aurelius-Signature`: `v1=<lowercase hex>`.
pub fn signature(secret: &str, timestamp_secs: i64, delivery_id: &str, body: &str) -> String {
    let mac = hmac_sha256(secret.as_bytes(), string_to_sign(timestamp_secs, delivery_id, body).as_bytes());
    format!("{SIGNATURE_VERSION}={}", hex::encode(mac))
}

/// What a receiver does: the signature is right and the timestamp is within
/// the tolerance of `now_secs`. Compares in constant time.
pub fn verify(secret: &str, signature_header: &str, timestamp_secs: i64, delivery_id: &str, body: &str, now_secs: i64) -> bool {
    if (now_secs - timestamp_secs).abs() > TOLERANCE_SECS {
        return false;
    }
    let expected = signature(secret, timestamp_secs, delivery_id, body);
    let (a, b) = (expected.as_bytes(), signature_header.trim().as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The wait after attempt `attempts_made` failed, or `None` when it was the last:
/// 5 s, 10 s, 20 s, 40 s, 80 s, 160 s, 320 s (never over an hour). `base_ms`
/// is `BACKOFF_BASE_MS` except in tests.
pub fn retry_delay_ms(attempts_made: u32, base_ms: i64) -> Option<i64> {
    if attempts_made == 0 || attempts_made >= MAX_ATTEMPTS {
        return None;
    }
    Some(base_ms.saturating_mul(1i64 << (attempts_made - 1).min(30)).min(BACKOFF_CAP_MS.max(base_ms)))
}

/// Whether an HTTP status counts as delivered: any 2xx. Everything else (also
/// a redirect, which is not followed) is a failure and is retried.
pub fn delivered(status: u16) -> bool {
    (200..300).contains(&status)
}

/// A new webhook secret (`whsec_` and 64 hex characters).
pub fn new_secret(random: [u8; 32]) -> String {
    format!("{SECRET_PREFIX}{}", hex::encode(random))
}

/// The body of one delivery. `id` is the delivery id and the idempotency id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub created_at_ms: i64,
    pub app: EnvelopeApp,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeApp {
    pub id: String,
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_of(bytes: [u8; 32]) -> String {
        hex::encode(bytes)
    }

    #[test]
    fn hmac_matches_the_rfc_4231_vectors() {
        // Test case 1.
        assert_eq!(hex_of(hmac_sha256(&[0x0b; 20], b"Hi There")), "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7");
        // Test case 2.
        assert_eq!(hex_of(hmac_sha256(b"Jefe", b"what do ya want for nothing?")), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
        // Test case 6: a key longer than the block size.
        assert_eq!(
            hex_of(hmac_sha256(&[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First")),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn signatures_cover_timestamp_id_and_body() {
        let body = r#"{"id":"d1"}"#;
        let sig = signature("whsec_x", 1_700_000_000, "d1", body);
        assert!(sig.starts_with("v1=") && sig.len() == 3 + 64);
        assert_eq!(sig, signature("whsec_x", 1_700_000_000, "d1", body), "deterministic");
        for other in [
            signature("whsec_y", 1_700_000_000, "d1", body),
            signature("whsec_x", 1_700_000_001, "d1", body),
            signature("whsec_x", 1_700_000_000, "d2", body),
            signature("whsec_x", 1_700_000_000, "d1", r#"{"id":"d2"}"#),
        ] {
            assert_ne!(sig, other);
        }
        assert_eq!(string_to_sign(5, "id", "{}"), "5.id.{}");
    }

    #[test]
    fn a_receiver_checks_signature_and_clock() {
        let (secret, body) = ("whsec_x", "{}");
        let sig = signature(secret, 1_000, "d", body);
        assert!(verify(secret, &sig, 1_000, "d", body, 1_000 + TOLERANCE_SECS));
        assert!(!verify(secret, &sig, 1_000, "d", body, 1_000 + TOLERANCE_SECS + 1), "too old");
        assert!(!verify(secret, &sig, 1_000, "d", body, 1_000 - TOLERANCE_SECS - 1), "from the future");
        assert!(!verify("whsec_other", &sig, 1_000, "d", body, 1_000));
        assert!(!verify(secret, "v1=00", 1_000, "d", body, 1_000));
    }

    #[test]
    fn retries_double_and_stop_after_the_last_attempt() {
        let waits: Vec<Option<i64>> = (0..=MAX_ATTEMPTS).map(|n| retry_delay_ms(n, BACKOFF_BASE_MS)).collect();
        assert_eq!(
            waits,
            vec![None, Some(5_000), Some(10_000), Some(20_000), Some(40_000), Some(80_000), Some(160_000), Some(320_000), None]
        );
        assert_eq!(retry_delay_ms(7, 1_000_000), Some(BACKOFF_CAP_MS.max(1_000_000)), "capped");
        assert!(delivered(200) && delivered(204) && !delivered(301) && !delivered(404) && !delivered(500));
    }

    #[test]
    fn events_have_stable_names() {
        for event in WebhookEvent::ALL {
            assert_eq!(WebhookEvent::parse(event.as_str()), Some(event));
            assert_eq!(serde_json::to_value(event).unwrap(), event.as_str());
        }
        assert_eq!(new_secret([0xab; 32]).len(), SECRET_PREFIX.len() + 64);
    }
}
