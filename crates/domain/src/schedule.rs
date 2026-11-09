//! Recurring tasks (protocol 1.14): a schedule holds a task template and a
//! recurrence; each time it is due the daemon makes a normal task from it.
//! Times are the server's local time zone; the pure rules live here so they
//! can be tested with any clock.

use crate::{
    DomainError, EmployeeId, ProjectId, TaskScheduleId,
    conversation::UPLOADS_MAX,
    task::{CreateTask, RunSettings},
};
use serde::{Deserialize, Serialize};

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;
pub const EVERY_HOURS_MAX: u32 = 8_760;

/// When a schedule is due. Weekdays count from Monday = 0 to Sunday = 6;
/// `at_minute` is minutes since local midnight (09:30 = 570).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Recurrence {
    EveryHours { hours: u32 },
    Daily { at_minute: u32 },
    Weekly { weekdays: Vec<u8>, at_minute: u32 },
}

impl Recurrence {
    /// Checked and in canonical form (weekdays sorted, without duplicates).
    pub fn validated(&self) -> Result<Recurrence, DomainError> {
        let out_of_range = |field: &str, detail: &str| DomainError::OutOfRange { field: field.into(), detail: detail.into() };
        match self {
            Recurrence::EveryHours { hours } if !(1..=EVERY_HOURS_MAX).contains(hours) => Err(out_of_range("hours", "between 1 hour and a year")),
            Recurrence::Daily { at_minute } | Recurrence::Weekly { at_minute, .. } if *at_minute >= 1440 => Err(out_of_range("at_minute", "0 to 1439")),
            Recurrence::Weekly { weekdays, at_minute } => {
                let mut days = weekdays.clone();
                days.sort_unstable();
                days.dedup();
                if days.is_empty() {
                    return Err(DomainError::Required { field: "weekdays".into() });
                }
                if days.iter().any(|d| *d > 6) {
                    return Err(out_of_range("weekdays", "0 (Monday) to 6 (Sunday)"));
                }
                Ok(Recurrence::Weekly { weekdays: days, at_minute: *at_minute })
            }
            other => Ok(other.clone()),
        }
    }

    /// The recurrence that matches an old per-task repeat (`every_minutes`)
    /// of a task due at `at_ms`: 1440 is daily and 10080 weekly, both at that
    /// time of day (`offset_min`: the server's UTC offset); anything else is
    /// every whole hour, rounded up (an hour at least).
    pub fn from_repeat(every_minutes: u32, at_ms: i64, offset_min: i64) -> Recurrence {
        let local = at_ms + offset_min * MINUTE_MS;
        let at_minute = (local.rem_euclid(DAY_MS) / MINUTE_MS) as u32;
        // 1970-01-01 was a Thursday (Monday = 0).
        let weekday = (local.div_euclid(DAY_MS) + 3).rem_euclid(7) as u8;
        match every_minutes {
            1440 => Recurrence::Daily { at_minute },
            10080 => Recurrence::Weekly { weekdays: vec![weekday], at_minute },
            m => Recurrence::EveryHours { hours: m.div_ceil(60).clamp(1, EVERY_HOURS_MAX) },
        }
    }

    /// The first occurrence strictly after `after_ms`. `anchor_ms` (when the
    /// schedule started or last fired) only matters for "every N hours":
    /// occurrences are the anchor plus whole intervals, so they don't drift.
    /// `offset_at` gives the server's UTC offset in minutes at a moment, so a
    /// time of day stays the same local time across a daylight saving change.
    pub fn next_after(&self, anchor_ms: i64, after_ms: i64, offset_at: impl Fn(i64) -> i64) -> i64 {
        match self {
            Recurrence::EveryHours { hours } => {
                let step = i64::from(*hours).max(1) * HOUR_MS;
                anchor_ms + ((after_ms - anchor_ms).div_euclid(step) + 1).max(1) * step
            }
            Recurrence::Daily { at_minute } => time_of_day(after_ms, &offset_at, *at_minute, |_| true),
            Recurrence::Weekly { weekdays, at_minute } => time_of_day(after_ms, &offset_at, *at_minute, |d| weekdays.contains(&d)),
        }
    }
}

/// The moment (Unix ms) at which local clocks read `local_ms` (ms since
/// 1970-01-01 00:00 local). The offset is taken at that moment, not now;
/// a time skipped by a clock change lands just after it.
pub fn local_to_utc(local_ms: i64, offset_at: impl Fn(i64) -> i64) -> i64 {
    let guess = local_ms - offset_at(local_ms) * MINUTE_MS;
    local_ms - offset_at(guess) * MINUTE_MS
}

fn time_of_day(after_ms: i64, offset_at: &impl Fn(i64) -> i64, at_minute: u32, allowed: impl Fn(u8) -> bool) -> i64 {
    let local_day = (after_ms + offset_at(after_ms) * MINUTE_MS).div_euclid(DAY_MS);
    for day in local_day..=local_day + 8 {
        // 1970-01-01 was a Thursday (Monday = 0).
        let weekday = (day + 3).rem_euclid(7) as u8;
        if !allowed(weekday) {
            continue;
        }
        let at = local_to_utc(day * DAY_MS + i64::from(at_minute) * MINUTE_MS, offset_at);
        if at > after_ms {
            return at;
        }
    }
    after_ms + DAY_MS
}

/// A new recurring task for an employee.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateTaskSchedule {
    pub project_id: ProjectId,
    pub employee_id: EmployeeId,
    pub title: String,
    /// Kept as typed (newlines included), apart from surrounding blank space.
    pub goal: String,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    /// Files to attach to every task made from it (host paths, or this
    /// device's upload inbox); copied when the schedule is saved.
    #[serde(default)]
    pub files: Vec<String>,
    /// Mode, model and effort of the tasks; unset fields follow the employee.
    #[serde(default, skip_serializing_if = "RunSettings::is_empty")]
    pub run: RunSettings,
    pub recurrence: Recurrence,
    /// Start paused: nothing is made until it is resumed.
    #[serde(default)]
    pub paused: bool,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// Replace a schedule's template and recurrence (the project stays the same).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateTaskSchedule {
    pub schedule_id: TaskScheduleId,
    pub employee_id: EmployeeId,
    pub title: String,
    pub goal: String,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    /// New files to attach (as `CreateTaskSchedule::files`).
    #[serde(default)]
    pub add_files: Vec<String>,
    /// Attachments to drop, by their `path` in `TaskScheduleView::attachments`.
    #[serde(default)]
    pub remove_files: Vec<String>,
    #[serde(default, skip_serializing_if = "RunSettings::is_empty")]
    pub run: RunSettings,
    pub recurrence: Recurrence,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteTaskSchedule {
    pub schedule_id: TaskScheduleId,
}

/// Pause (`true`) or resume (`false`). Resuming plans the next occurrence
/// from now; what was due while paused is not made up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetTaskSchedulePaused {
    pub schedule_id: TaskScheduleId,
    pub paused: bool,
}

/// Make a task from the schedule right now (also while paused, and even when
/// an earlier one is still open). The next scheduled time does not change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunTaskScheduleNow {
    pub schedule_id: TaskScheduleId,
}

/// A schedule's template as checked text: (title, goal, criteria, run).
pub type Template = (String, String, Vec<String>, RunSettings);

/// Check a template with the rules of a task, and the number of files.
pub fn validate_template(project_id: ProjectId, employee_id: EmployeeId, title: &str, goal: &str, criteria: &[String], run: &RunSettings, files: usize) -> Result<Template, DomainError> {
    if files > UPLOADS_MAX {
        return Err(DomainError::File(format!("Too many files: attach at most {UPLOADS_MAX}.")));
    }
    let probe = CreateTask {
        confirm_secret: false,
        project_id,
        employee_id: Some(employee_id),
        title: title.into(),
        goal: goal.into(),
        acceptance_criteria: criteria.to_vec(),
        run: run.clone(),
        files: vec![],
    };
    let valid = crate::task::validate(&probe)?;
    Ok((valid.title, valid.goal, valid.criteria, run.validated()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-05 (a Monday) 00:00 UTC.
    const MONDAY: i64 = 1_791_158_400_000;

    #[test]
    fn an_old_repeat_becomes_the_closest_recurrence() {
        // Monday 09:30 UTC, seen from UTC+2: 11:30.
        let at = MONDAY + 9 * HOUR_MS + 30 * MINUTE_MS;
        assert_eq!(Recurrence::from_repeat(60, at, 0), Recurrence::EveryHours { hours: 1 });
        assert_eq!(Recurrence::from_repeat(1440, at, 120), Recurrence::Daily { at_minute: 11 * 60 + 30 });
        assert_eq!(Recurrence::from_repeat(10080, at, 0), Recurrence::Weekly { weekdays: vec![0], at_minute: 9 * 60 + 30 });
        // The local weekday counts: 23:30 UTC on Monday is Tuesday in UTC+2.
        assert_eq!(Recurrence::from_repeat(10080, MONDAY + 23 * HOUR_MS + 30 * MINUTE_MS, 120), Recurrence::Weekly { weekdays: vec![1], at_minute: 90 });
        assert_eq!(Recurrence::from_repeat(5, at, 0), Recurrence::EveryHours { hours: 1 });
        assert_eq!(Recurrence::from_repeat(90, at, 0), Recurrence::EveryHours { hours: 2 });
        assert_eq!(Recurrence::from_repeat(2880, at, 0), Recurrence::EveryHours { hours: 48 });
        assert_eq!(Recurrence::from_repeat(525_600, at, 0), Recurrence::EveryHours { hours: EVERY_HOURS_MAX });
    }

    #[test]
    fn the_test_date_is_a_monday() {
        assert_eq!(((MONDAY / DAY_MS) + 3).rem_euclid(7), 0);
    }

    #[test]
    fn every_n_hours_follows_the_anchor_without_drifting() {
        let r = Recurrence::EveryHours { hours: 6 };
        assert_eq!(r.next_after(MONDAY, MONDAY, |_| 0), MONDAY + 6 * HOUR_MS);
        // Late by a day and a bit: the next one is on the grid, after now.
        assert_eq!(r.next_after(MONDAY, MONDAY + 30 * HOUR_MS, |_| 0), MONDAY + 36 * HOUR_MS);
    }

    #[test]
    fn daily_uses_the_local_time_of_day() {
        let r = Recurrence::Daily { at_minute: 9 * 60 + 30 };
        let at = MONDAY + 7 * HOUR_MS + 30 * MINUTE_MS;
        // UTC+2: 09:30 local is 07:30 UTC.
        assert_eq!(r.next_after(0, MONDAY, |_| 120), at);
        // Just after it today: tomorrow.
        assert_eq!(r.next_after(0, MONDAY + 8 * HOUR_MS, |_| 120), at + DAY_MS);
        // Exactly at it: strictly after, so tomorrow.
        assert_eq!(r.next_after(0, at, |_| 120), at + DAY_MS);
    }

    #[test]
    fn weekly_picks_the_next_chosen_weekday() {
        // Wednesday (2) and Sunday (6) at 08:00 UTC.
        let r = Recurrence::Weekly { weekdays: vec![6, 2], at_minute: 8 * 60 };
        assert_eq!(r.next_after(0, MONDAY, |_| 0), MONDAY + 2 * DAY_MS + 8 * HOUR_MS);
        assert_eq!(r.next_after(0, MONDAY + 2 * DAY_MS + 8 * HOUR_MS, |_| 0), MONDAY + 6 * DAY_MS + 8 * HOUR_MS);
        // After the Sunday one: Wednesday of the next week.
        assert_eq!(r.next_after(0, MONDAY + 6 * DAY_MS + 9 * HOUR_MS, |_| 0), MONDAY + 9 * DAY_MS + 8 * HOUR_MS);
        // The weekday follows local time: Sunday 21:00 UTC is Sunday 23:00 at
        // UTC+2, so Monday 01:00 local (= Sunday 23:00 UTC) comes next.
        let monday_local = Recurrence::Weekly { weekdays: vec![0], at_minute: 60 };
        assert_eq!(monday_local.next_after(0, MONDAY - 3 * HOUR_MS, |_| 120), MONDAY - HOUR_MS);
    }

    #[test]
    fn daily_keeps_the_local_time_across_a_dst_change() {
        // Brussels: UTC+2 until 2026-10-25 01:00 UTC, UTC+1 after.
        let change = MONDAY + 20 * DAY_MS + HOUR_MS;
        let brussels = |ms: i64| if ms < change { 120 } else { 60 };
        let r = Recurrence::Daily { at_minute: 9 * 60 };
        let saturday = MONDAY + 19 * DAY_MS;
        // Saturday 09:00 is 07:00 UTC, Sunday 09:00 (after the change) 08:00 UTC.
        assert_eq!(r.next_after(0, saturday, brussels), saturday + 7 * HOUR_MS);
        assert_eq!(r.next_after(0, saturday + 7 * HOUR_MS, brussels), saturday + DAY_MS + 8 * HOUR_MS);
        // Seen from before the change, with the offset at the result: still 09:00.
        assert_eq!(r.next_after(0, saturday + 23 * HOUR_MS, brussels), saturday + DAY_MS + 8 * HOUR_MS);
        let weekly = Recurrence::Weekly { weekdays: vec![0], at_minute: 9 * 60 };
        assert_eq!(weekly.next_after(0, saturday, brussels), saturday + 2 * DAY_MS + 8 * HOUR_MS);
    }

    #[test]
    fn recurrences_are_checked_and_made_canonical() {
        assert!(Recurrence::EveryHours { hours: 0 }.validated().is_err());
        assert!(Recurrence::Daily { at_minute: 1440 }.validated().is_err());
        assert!(Recurrence::Weekly { weekdays: vec![], at_minute: 0 }.validated().is_err());
        assert!(Recurrence::Weekly { weekdays: vec![7], at_minute: 0 }.validated().is_err());
        assert_eq!(Recurrence::Weekly { weekdays: vec![4, 1, 4], at_minute: 5 }.validated().unwrap(), Recurrence::Weekly { weekdays: vec![1, 4], at_minute: 5 });
    }

    #[test]
    fn the_wire_form_is_tagged_by_kind() {
        let json = serde_json::to_value(Recurrence::Weekly { weekdays: vec![0, 4], at_minute: 570 }).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "weekly", "weekdays": [0, 4], "at_minute": 570 }));
        assert_eq!(serde_json::to_value(Recurrence::EveryHours { hours: 6 }).unwrap(), serde_json::json!({ "kind": "every_hours", "hours": 6 }));
    }
}
