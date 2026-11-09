//! `schedule_message` (R7): what an employee may ask for, pure and with explicit time.

/// Soonest and latest a scheduled message may come.
pub const MIN_DELAY_MS: i64 = 60_000;
pub const MAX_DELAY_MS: i64 = 30 * 24 * 3_600_000;
/// The longest text of one message.
pub const CONTENT_MAX: usize = 4_000;
/// What an employee may have waiting at a time.
pub const MAX_PENDING_PER_EMPLOYEE: u32 = 20;
/// What one run may schedule.
pub const MAX_PER_RUN: u32 = 5;

/// The states of a scheduled message; text in the store and on the wire.
pub const SCHEDULED: &str = "scheduled";
pub const DELIVERED: &str = "delivered";
/// A board member took it back.
pub const CANCELLED: &str = "cancelled";
/// It came due, but could not go out (`outcome` says why).
pub const NOT_DELIVERED: &str = "not_delivered";

/// The delay of `schedule_message` in milliseconds: `90s`, `10m`, `2h`, `1d` or
/// several of them (`1h30m`), within the limits above. A bare number has no unit
/// and is refused, so "5" can never mean five seconds to one and five days to another.
pub fn parse_delay(text: &str) -> Result<i64, String> {
    let usage = || format!("Give the delay as a number with a unit: 90s, 10m, 2h, 1d or 1h30m (not {:?}).", text.trim());
    let text = text.trim().to_ascii_lowercase();
    if text.is_empty() {
        return Err(usage());
    }
    let mut total: i64 = 0;
    let mut digits = String::new();
    for c in text.chars() {
        match c {
            '0'..='9' => digits.push(c),
            's' | 'm' | 'h' | 'd' => {
                let amount: i64 = digits.parse().map_err(|_| usage())?;
                digits.clear();
                let unit = match c {
                    's' => 1_000,
                    'm' => 60_000,
                    'h' => 3_600_000,
                    _ => 24 * 3_600_000,
                };
                total = total.checked_add(amount.checked_mul(unit).ok_or_else(usage)?).ok_or_else(usage)?;
            }
            ' ' => {}
            _ => return Err(usage()),
        }
    }
    if !digits.is_empty() {
        return Err(usage());
    }
    if total < MIN_DELAY_MS {
        return Err("A scheduled message comes at least a minute from now.".into());
    }
    if total > MAX_DELAY_MS {
        return Err("A scheduled message comes at most 30 days from now; a recurring task is for what comes back.".into());
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_delay_has_a_unit_and_stays_within_its_limits() {
        assert_eq!(parse_delay("10m"), Ok(600_000));
        assert_eq!(parse_delay(" 1h30m "), Ok(5_400_000));
        assert_eq!(parse_delay("90s"), Ok(90_000));
        assert_eq!(parse_delay("2D"), Ok(172_800_000));
        for bad in ["", "5", "m", "10 minutes", "1.5h", "-5m"] {
            assert!(parse_delay(bad).is_err(), "{bad}");
        }
        assert!(parse_delay("30s").unwrap_err().contains("minute"));
        assert!(parse_delay("31d").unwrap_err().contains("30 days"));
        assert!(parse_delay("99999999999999999999d").is_err(), "no overflow");
    }
}
