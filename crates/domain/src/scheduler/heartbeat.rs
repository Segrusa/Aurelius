//! Heartbeat timing, pure and deterministic. A heartbeat is an
//! opt-in interval rule with quiet hours and a daily token budget. Missed
//! occurrences (sleep, clock jumps) coalesce into one wake; the occurrence key
//! is the scheduled time, never the moment of execution.

use crate::schedule::local_to_utc;
use serde::{Deserialize, Serialize};

const MINUTE_MS: i64 = 60_000;
const DAY_MIN: i64 = 24 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HeartbeatRule {
    pub every_minutes: u32,
    /// Local wall-clock minutes since midnight; start == end means no quiet hours.
    pub quiet_start_min: u32,
    pub quiet_end_min: u32,
    /// Background tokens per local day for this heartbeat's wakes.
    pub daily_token_budget: u64,
}

impl HeartbeatRule {
    pub const DEFAULT_QUIET: (u32, u32) = (22 * 60, 7 * 60);
    pub const DEFAULT_BUDGET: u64 = 20_000;
    /// Choices offered in the UI (minutes); 0 = off.
    pub const CHOICES: [u32; 4] = [0, 30, 60, 240];

    pub fn new(every_minutes: u32) -> Self {
        Self { every_minutes, quiet_start_min: Self::DEFAULT_QUIET.0, quiet_end_min: Self::DEFAULT_QUIET.1, daily_token_budget: Self::DEFAULT_BUDGET }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(5..=7 * DAY_MIN as u32).contains(&self.every_minutes) {
            return Err("A heartbeat runs between every 5 minutes and once a week.".into());
        }
        if self.quiet_start_min >= DAY_MIN as u32 || self.quiet_end_min >= DAY_MIN as u32 {
            return Err("Quiet hours must be times of day.".into());
        }
        if self.daily_token_budget == 0 {
            return Err("The daily budget must be above zero.".into());
        }
        Ok(())
    }

    fn interval_ms(&self) -> i64 {
        i64::from(self.every_minutes) * MINUTE_MS
    }

    /// Is `at_ms` inside quiet hours? `offset_at` gives the local UTC offset
    /// in minutes at a moment.
    pub fn is_quiet(&self, at_ms: i64, offset_at: impl Fn(i64) -> i64) -> bool {
        let (s, e) = (i64::from(self.quiet_start_min), i64::from(self.quiet_end_min));
        if s == e {
            return false;
        }
        let local = (at_ms / MINUTE_MS + offset_at(at_ms)).rem_euclid(DAY_MIN);
        if s < e { (s..e).contains(&local) } else { local >= s || local < e }
    }

    /// First moment after `at_ms` when quiet hours end, on the local clock
    /// of that moment (a daylight saving change in between counts).
    pub fn quiet_end_after(&self, at_ms: i64, offset_at: impl Fn(i64) -> i64) -> i64 {
        let local_min = at_ms / MINUTE_MS + offset_at(at_ms);
        let mut wait = i64::from(self.quiet_end_min) - local_min.rem_euclid(DAY_MIN);
        if wait <= 0 {
            wait += DAY_MIN;
        }
        local_to_utc((local_min + wait) * MINUTE_MS, offset_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Not due; keep (or correct) the next due moment.
    Wait { next_due_ms: i64 },
    /// Due but in quiet hours: no wake; check again when they end.
    Quiet { next_due_ms: i64 },
    /// Due: at most one evaluation for all missed occurrences.
    Due { occurrence_ms: i64, missed: u32, next_due_ms: i64 },
}

/// Decide what to do at `now_ms` for a heartbeat whose next occurrence was `next_due_ms`.
pub fn plan(rule: &HeartbeatRule, next_due_ms: i64, now_ms: i64, offset_at: impl Fn(i64) -> i64) -> Plan {
    let interval = rule.interval_ms();
    if now_ms < next_due_ms {
        // The clock went back (or the rule got shorter): never wait longer than one interval.
        return Plan::Wait { next_due_ms: next_due_ms.min(now_ms + interval) };
    }
    if rule.is_quiet(now_ms, &offset_at) {
        return Plan::Quiet { next_due_ms: rule.quiet_end_after(now_ms, &offset_at) };
    }
    let behind = (now_ms - next_due_ms) / interval;
    let occurrence_ms = next_due_ms + behind * interval;
    let missed = u32::try_from(behind + 1).unwrap_or(u32::MAX);
    Plan::Due { occurrence_ms, missed, next_due_ms: occurrence_ms + interval }
}

/// Local midnight (start of today) for budget accounting.
pub fn local_day_start(now_ms: i64, offset_at: impl Fn(i64) -> i64) -> i64 {
    let local_min = now_ms / MINUTE_MS + offset_at(now_ms);
    local_to_utc((local_min - local_min.rem_euclid(DAY_MIN)) * MINUTE_MS, offset_at)
}

/// A heartbeat reply that means "nothing needs the user" stays silent.
pub fn is_meaningful(reply: &str) -> bool {
    let first = reply.trim().lines().next().unwrap_or("").trim().trim_end_matches('.').to_ascii_lowercase();
    !(first.is_empty() || first == "nothing to report" || first.starts_with("nothing to report"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 60 * MINUTE_MS;
    // 2026-03-02 10:00 UTC (a Monday; no quiet hours in the way).
    const T0: i64 = 1_772_445_600_000;

    fn rule(minutes: u32) -> HeartbeatRule {
        HeartbeatRule { quiet_start_min: 0, quiet_end_min: 0, ..HeartbeatRule::new(minutes) }
    }

    #[test]
    fn twenty_missed_occurrences_give_one_wake() {
        let r = rule(30);
        // Laptop slept for 10 hours: 20 occurrences missed.
        let now = T0 + 10 * H - 1;
        let Plan::Due { occurrence_ms, missed, next_due_ms } = plan(&r, T0, now, |_| 0) else { panic!() };
        assert_eq!(missed, 20);
        assert_eq!(occurrence_ms, T0 + 19 * 30 * MINUTE_MS, "the key is the last scheduled occurrence");
        assert!(next_due_ms > now && next_due_ms - now <= 30 * MINUTE_MS, "no burst of catch-up wakes");
        // Evaluating again at the same moment is not due any more.
        assert!(matches!(plan(&r, next_due_ms, now, |_| 0), Plan::Wait { .. }));
    }

    #[test]
    fn same_inputs_same_occurrence_key() {
        let r = rule(60);
        assert_eq!(plan(&r, T0, T0 + 5 * MINUTE_MS, |_| 0), plan(&r, T0, T0 + 5 * MINUTE_MS, |_| 0));
        let Plan::Due { occurrence_ms, .. } = plan(&r, T0, T0 + 59 * MINUTE_MS, |_| 0) else { panic!() };
        assert_eq!(occurrence_ms, T0, "execution time doesn't change the key");
    }

    #[test]
    fn clock_back_never_waits_more_than_one_interval() {
        let r = rule(30);
        let Plan::Wait { next_due_ms } = plan(&r, T0 + 50 * H, T0, |_| 0) else { panic!() };
        assert_eq!(next_due_ms, T0 + 30 * MINUTE_MS);
    }

    #[test]
    fn quiet_hours_defer_to_their_end_and_wrap_midnight() {
        let r = HeartbeatRule::new(30); // 22:00–07:00 local
        let offset = |_: i64| 120; // UTC+2
        // 23:30 local = 21:30 UTC.
        let late = T0 - 10 * H + 21 * H + 30 * MINUTE_MS; // 2026-03-02 21:30 UTC
        assert!(r.is_quiet(late, offset));
        let Plan::Quiet { next_due_ms } = plan(&r, late - H, late, offset) else { panic!() };
        // 07:00 local next day = 05:00 UTC.
        assert_eq!(next_due_ms, T0 - 10 * H + 24 * H + 5 * H);
        assert!(!r.is_quiet(next_due_ms, offset));
        // 12:00 local is not quiet.
        assert!(!r.is_quiet(T0, offset));
    }

    #[test]
    fn dst_shift_moves_quiet_hours_with_the_wall_clock() {
        let r = HeartbeatRule::new(60);
        // 06:30 UTC is 07:30 at UTC+1 (awake) but 06:30... at UTC+0 still quiet.
        let at = T0 - 3 * H - 30 * MINUTE_MS;
        assert!(!r.is_quiet(at, |_| 60));
        assert!(r.is_quiet(at, |_| 0));
    }

    #[test]
    fn quiet_hours_end_on_the_wall_clock_after_a_dst_change() {
        // Brussels: UTC+2 until 2026-10-25 01:00 UTC, UTC+1 after.
        let change = 1_792_890_000_000;
        let brussels = |ms: i64| if ms < change { 120 } else { 60 };
        let r = HeartbeatRule::new(30); // 22:00–07:00 local
        // Saturday 23:00 local (21:00 UTC) waits for Sunday 07:00 CET = 06:00 UTC.
        let late = change - 4 * H;
        assert_eq!(r.quiet_end_after(late, brussels), change + 5 * H);
        // Local midnight of the change day is still in summer time: 22:00 UTC.
        assert_eq!(local_day_start(change + 5 * H, brussels), change - 3 * H);
    }

    #[test]
    fn day_start_and_meaning() {
        assert_eq!(local_day_start(T0, |_| 0), T0 - 10 * H);
        assert_eq!(local_day_start(T0, |_| 120), T0 - 12 * H);
        assert!(!is_meaningful("Nothing to report."));
        assert!(!is_meaningful("  nothing to report\nall quiet"));
        assert!(is_meaningful("Task 'Launch' failed; I suggest a retry with a narrower goal."));
        assert!(HeartbeatRule::new(1).validate().is_err());
        assert!(HeartbeatRule::new(30).validate().is_ok());
    }
}
