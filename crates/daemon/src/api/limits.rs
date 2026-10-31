//! The limits on sign-in and new board members (B9), daemon-wide and in memory
//! (one daemon instance; a restart forgets them, which never blocks anyone).
//!
//! * `user.login`: per username (case does not count, known or not, so the
//!   answer tells nothing), the first `FREE_FAILURES` failed tries are free; each
//!   one after that makes the next try wait longer, from `FIRST_WAIT` doubling
//!   up to `MOST_WAIT`. Never a lasting block; a right password clears it.
//!   A try is reserved before its argon2 work (`login_begin`), so tries at the
//!   same time count too: never more than `FREE_FAILURES` checked at once, and
//!   in a wait one at a time (R1 M-3).
//! * `user.create`: at most `CREATES_PER_HOUR` an hour on the server, beside the
//!   five a minute per connection (`users::create`).

use aurelius_protocol::ProtocolError;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const FREE_FAILURES: u32 = 5;
pub const FIRST_WAIT: Duration = Duration::from_secs(60);
pub const MOST_WAIT: Duration = Duration::from_secs(15 * 60);
pub const CREATES_PER_HOUR: usize = 20;
const HOUR: Duration = Duration::from_secs(3_600);
/// Usernames tracked at most; the oldest quiet ones go first.
const TRACKED: usize = 10_000;

#[derive(Debug, Clone, Copy)]
struct Failures {
    count: u32,
    /// Tries begun and not yet answered.
    in_flight: u32,
    /// No try is taken before this.
    next_at: Instant,
    last_at: Instant,
}

#[derive(Debug, Default)]
pub struct Limits {
    logins: Mutex<HashMap<String, Failures>>,
    creates: Mutex<VecDeque<Instant>>,
}

/// The username as the limits count it (and the log names it): as the database
/// compares it (`COLLATE NOCASE`).
pub fn key(username: &str) -> String {
    username.trim().to_ascii_lowercase()
}

/// The wait after the `count`-th failure in a row.
fn wait_after(count: u32) -> Duration {
    match count.checked_sub(FREE_FAILURES) {
        None => Duration::ZERO,
        Some(extra) => FIRST_WAIT.saturating_mul(2u32.saturating_pow(extra.min(16))).min(MOST_WAIT),
    }
}

fn refused(wait: Duration) -> ProtocolError {
    ProtocolError::too_many_attempts(wait.as_millis().max(1) as u64)
}

/// A try of `user.login` begun for a username: answer it with `failed` or
/// `succeeded`. Dropped unanswered (the connection went away), it only frees
/// its place.
#[derive(Debug)]
pub struct LoginTry<'a> {
    limits: &'a Limits,
    key: String,
    open: bool,
}

impl LoginTry<'_> {
    /// The password was wrong: counted. Answers the failures in a row and the
    /// wait that starts now (zero while they are free).
    pub fn failed(mut self, now: Instant) -> (u32, Duration) {
        self.open = false;
        let mut logins = self.limits.logins.lock().unwrap();
        let entry = logins.entry(self.key.clone()).or_insert(Failures { count: 0, in_flight: 1, next_at: now, last_at: now });
        entry.in_flight = entry.in_flight.saturating_sub(1);
        entry.count = entry.count.saturating_add(1);
        let wait = wait_after(entry.count);
        entry.next_at = now + wait;
        entry.last_at = now;
        (entry.count, wait)
    }

    pub fn succeeded(mut self) {
        self.open = false;
        self.limits.logins.lock().unwrap().remove(&self.key);
    }
}

impl Drop for LoginTry<'_> {
    fn drop(&mut self) {
        if !self.open {
            return;
        }
        let mut logins = self.limits.logins.lock().unwrap();
        if let Some(entry) = logins.get_mut(&self.key) {
            entry.in_flight = entry.in_flight.saturating_sub(1);
            if entry.count == 0 && entry.in_flight == 0 {
                logins.remove(&self.key);
            }
        }
    }
}

impl Limits {
    /// Begin a try of `username` now, or refuse it: in a wait, or when the tries
    /// already under way would use up the free failures (one at a time then).
    pub fn login_begin(&self, username: &str, now: Instant) -> Result<LoginTry<'_>, ProtocolError> {
        let key = key(username);
        let mut logins = self.logins.lock().unwrap();
        if let Some(f) = logins.get(&key) {
            if f.next_at > now {
                return Err(refused(f.next_at - now));
            }
            if f.in_flight > 0 && f.count + f.in_flight >= FREE_FAILURES {
                return Err(refused(wait_after(f.count + f.in_flight).max(Duration::from_secs(1))));
            }
        }
        if logins.len() >= TRACKED && !logins.contains_key(&key) {
            // Forget the quietest one: a guesser cannot grow the map without end.
            if let Some(oldest) = logins.iter().filter(|(_, f)| f.in_flight == 0).min_by_key(|(_, f)| f.last_at).map(|(k, _)| k.clone()) {
                logins.remove(&oldest);
            }
        }
        let entry = logins.entry(key.clone()).or_insert(Failures { count: 0, in_flight: 0, next_at: now, last_at: now });
        entry.in_flight += 1;
        entry.last_at = now;
        Ok(LoginTry { limits: self, key, open: true })
    }

    /// Counts a new board member on the server, or refuses it.
    pub fn take_create(&self, now: Instant) -> Result<(), ProtocolError> {
        let mut creates = self.creates.lock().unwrap();
        while creates.front().is_some_and(|at| now.duration_since(*at) >= HOUR) {
            creates.pop_front();
        }
        if creates.len() >= CREATES_PER_HOUR {
            let oldest = *creates.front().expect("full");
            return Err(refused(HOUR - now.duration_since(oldest)));
        }
        creates.push_back(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_protocol::ErrorCode;

    fn fail(limits: &Limits, name: &str, at: Instant) -> (u32, Duration) {
        limits.login_begin(name, at).unwrap().failed(at)
    }

    #[test]
    fn five_free_failures_then_a_growing_wait_up_to_fifteen_minutes() {
        let limits = Limits::default();
        let start = Instant::now();
        for i in 1..FREE_FAILURES {
            assert_eq!(fail(&limits, "alex", start), (i, Duration::ZERO));
        }
        // The fifth failure: the next try waits a minute ("Try again in 1:00", Sentinel R1).
        assert_eq!(fail(&limits, "alex", start), (FREE_FAILURES, FIRST_WAIT));
        let err = limits.login_begin("ALEX ", start).unwrap_err();
        assert_eq!((err.code, err.retry_after_ms), (ErrorCode::TooManyAttempts, Some(60_000)));
        assert_eq!(err.message, "Too many attempts. Try again in 1:00.");
        // Another username is not affected.
        limits.login_begin("robin", start).unwrap().succeeded();
        // Each failure after that doubles the wait, never more than 15 minutes.
        let mut now = start;
        let mut waits = vec![];
        for _ in 0..8 {
            now += wait_after(FREE_FAILURES + waits.len() as u32);
            fail(&limits, "alex", now);
            waits.push(limits.login_begin("alex", now).unwrap_err().retry_after_ms.unwrap() / 1_000);
        }
        assert_eq!(waits, [120, 240, 480, 900, 900, 900, 900, 900]);
        // No lasting block: after the wait a try is taken, and a right one clears it all.
        limits.login_begin("alex", now + MOST_WAIT).unwrap().succeeded();
        limits.login_begin("alex", now).unwrap().succeeded();
    }

    /// R1 M-3 / SEC1 M2: tries at the same time are reserved before their work.
    #[test]
    fn tries_at_the_same_time_count_before_they_are_answered() {
        let limits = Limits::default();
        let now = Instant::now();
        let open: Vec<LoginTry<'_>> = (0..FREE_FAILURES).map(|_| limits.login_begin("alex", now).unwrap()).collect();
        assert_eq!(limits.login_begin("Alex", now).unwrap_err().code, ErrorCode::TooManyAttempts, "a sixth while five are under way");
        let mut open = open.into_iter();
        // One unanswered (its connection went away): only its place is freed.
        drop(open.next());
        let again = limits.login_begin("alex", now).unwrap();
        for t in open {
            t.failed(now);
        }
        let (count, wait) = again.failed(now);
        assert_eq!((count, wait), (FREE_FAILURES, FIRST_WAIT));
        assert!(limits.login_begin("alex", now).is_err());
        // In a wait, one try at a time after it.
        let later = now + FIRST_WAIT;
        let one = limits.login_begin("alex", later).unwrap();
        assert!(limits.login_begin("alex", later).is_err(), "one at a time once the free failures are used");
        drop(one);
        assert!(limits.login_begin("alex", later).is_ok());
    }

    #[test]
    fn twenty_new_board_members_an_hour_on_the_server() {
        let limits = Limits::default();
        let start = Instant::now();
        for i in 0..CREATES_PER_HOUR {
            limits.take_create(start + Duration::from_secs(i as u64)).unwrap();
        }
        let err = limits.take_create(start + Duration::from_secs(60)).unwrap_err();
        assert_eq!(err.code, ErrorCode::TooManyAttempts);
        assert_eq!(err.retry_after_ms, Some((HOUR - Duration::from_secs(60)).as_millis() as u64));
        assert!(limits.take_create(start + HOUR).is_ok(), "an hour later the first one is gone");
    }

    #[test]
    fn the_tracked_usernames_stay_bounded() {
        let limits = Limits::default();
        let now = Instant::now();
        for i in 0..TRACKED + 10 {
            fail(&limits, &format!("guess{i}"), now + Duration::from_millis(i as u64));
        }
        assert_eq!(limits.logins.lock().unwrap().len(), TRACKED);
    }
}
