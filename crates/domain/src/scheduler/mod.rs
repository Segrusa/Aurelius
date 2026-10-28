//! Scheduling rules. Pure functions with explicit time, so
//! tests run on a fake clock.

pub mod heartbeat;

use sha2::{Digest, Sha256};

/// Scheduling limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Active inference runs across the whole daemon.
    pub global_runs: u32,
    /// Mutating turns per conversation (one employee = one orchestration turn).
    pub per_conversation: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self { global_runs: 4, per_conversation: 1 }
    }
}

/// Why a run could not proceed, as reported by a runtime adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Network blip, provider 5xx, runtime not reachable: retry with backoff.
    Transient(String),
    /// Login missing or expired: wait for the user, never poll.
    Auth(String),
    /// Quota exhausted: wait until the known reset, or `QUOTA_RETRY_MS` if unknown.
    Quota { reset_at_ms: Option<i64> },
    /// Invalid request, unsupported model: retrying cannot help.
    Permanent(String),
}

/// What the scheduler does next with the job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    RetryAt(i64),
    WaitForUser { reason: &'static str },
    WaitUntil { at_ms: i64, reason: &'static str },
    Fail { code: String },
}

pub const BASE_BACKOFF_MS: i64 = 1_000;
pub const MAX_BACKOFF_MS: i64 = 60_000;

/// Exponential backoff with ±20% deterministic jitter (seeded by job id and attempt).
pub fn backoff_ms(base_ms: i64, attempt: u32, seed: &str) -> i64 {
    let exp = base_ms.saturating_mul(1_i64 << attempt.saturating_sub(1).min(20));
    let capped = exp.min(MAX_BACKOFF_MS.max(base_ms));
    let hash = Sha256::digest(format!("{seed}:{attempt}").as_bytes());
    let unit = u16::from_be_bytes([hash[0], hash[1]]) as f64 / u16::MAX as f64; // 0..1
    let factor = 0.8 + 0.4 * unit;
    ((capped as f64) * factor).round() as i64
}

/// Without a known reset a limited run tries again after this long, repeating.
pub const QUOTA_RETRY_MS: i64 = 20 * 60_000;
/// Past a known reset, a little slack so the limit has really lifted.
const QUOTA_SLACK_MS: i64 = 30_000;

/// When a run stopped by the usage limit goes again: just after the reset, or
/// after `QUOTA_RETRY_MS` when no (future) reset is known.
pub fn quota_wake_ms(reset_at_ms: Option<i64>, now_ms: i64) -> i64 {
    match reset_at_ms {
        Some(at) if at > now_ms => at + QUOTA_SLACK_MS,
        _ => now_ms + QUOTA_RETRY_MS,
    }
}

/// Decide the next step for a failed attempt. `attempt` counts from 1.
pub fn dispose(failure: &Failure, attempt: u32, max_attempts: u32, now_ms: i64, base_ms: i64, seed: &str) -> Disposition {
    match failure {
        Failure::Transient(code) if attempt >= max_attempts => Disposition::Fail { code: format!("retries_exhausted:{code}") },
        Failure::Transient(_) => Disposition::RetryAt(now_ms + backoff_ms(base_ms, attempt, seed)),
        Failure::Auth(_) => Disposition::WaitForUser { reason: "auth" },
        Failure::Quota { reset_at_ms: Some(at) } if *at > now_ms => Disposition::WaitUntil { at_ms: *at, reason: "quota" },
        Failure::Quota { reset_at_ms } => Disposition::WaitUntil { at_ms: quota_wake_ms(*reset_at_ms, now_ms), reason: "quota" },
        Failure::Permanent(code) => Disposition::Fail { code: code.clone() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_is_capped_and_jittered_deterministically() {
        let a1 = backoff_ms(1000, 1, "job");
        let a2 = backoff_ms(1000, 2, "job");
        let a10 = backoff_ms(1000, 10, "job");
        assert!((800..=1200).contains(&a1));
        assert!((1600..=2400).contains(&a2));
        assert!(a10 <= 72_000, "cap with jitter");
        assert_eq!(backoff_ms(1000, 3, "job"), backoff_ms(1000, 3, "job"), "same seed, same delay");
        assert_ne!(backoff_ms(1000, 3, "a"), backoff_ms(1000, 3, "b"), "jitter spreads jobs");
    }

    #[test]
    fn transient_failures_retry_then_fail() {
        let t = Failure::Transient("net".into());
        assert!(matches!(dispose(&t, 1, 3, 0, 1000, "j"), Disposition::RetryAt(at) if at > 0));
        assert_eq!(dispose(&t, 3, 3, 0, 1000, "j"), Disposition::Fail { code: "retries_exhausted:net".into() });
    }

    #[test]
    fn auth_and_quota_wait_instead_of_polling() {
        assert_eq!(dispose(&Failure::Auth("expired".into()), 1, 3, 0, 1000, "j"), Disposition::WaitForUser { reason: "auth" });
        assert_eq!(
            dispose(&Failure::Quota { reset_at_ms: Some(5_000) }, 1, 3, 1_000, 1000, "j"),
            Disposition::WaitUntil { at_ms: 5_000, reason: "quota" }
        );
        assert_eq!(dispose(&Failure::Quota { reset_at_ms: None }, 1, 3, 0, 1000, "j"), Disposition::WaitUntil { at_ms: QUOTA_RETRY_MS, reason: "quota" });
        assert_eq!(
            dispose(&Failure::Quota { reset_at_ms: Some(500) }, 1, 3, 1_000, 1000, "j"),
            Disposition::WaitUntil { at_ms: 1_000 + QUOTA_RETRY_MS, reason: "quota" },
            "a past reset counts as unknown: the task still waits instead of staying running"
        );
        assert_eq!(dispose(&Failure::Permanent("bad_model".into()), 1, 3, 0, 1000, "j"), Disposition::Fail { code: "bad_model".into() });
    }

    #[test]
    fn quota_wakes_after_the_reset_or_after_the_default_delay() {
        assert_eq!(quota_wake_ms(Some(10_000_000), 1_000), 10_000_000 + 30_000);
        assert_eq!(quota_wake_ms(None, 1_000), 1_000 + QUOTA_RETRY_MS);
        assert_eq!(quota_wake_ms(Some(500), 1_000), 1_000 + QUOTA_RETRY_MS, "a reset in the past counts as unknown");
    }
}
