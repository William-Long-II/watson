//! WAT-201: usage-weighted ranking math.
//!
//! The fuzzy-match score from skim stays the primary sort key. This
//! module computes a secondary `usage_bonus` that breaks ties so
//! frequently-used apps float above identically-scored rivals. The bonus
//! decays exponentially — an app launched once five years ago shouldn't
//! still win tie-breaks against an app launched twice last week.
//!
//! Formula:
//!
//! ```text
//! usage_bonus = launch_count * exp(-age_days / HALF_LIFE_DAYS)
//! ```
//!
//! Properties:
//! - `launch_count == 0` → `0.0` (no bonus; app never launched).
//! - `age_days == 0` → `launch_count` (full credit; just launched).
//! - `age_days == HALF_LIFE_DAYS` → `launch_count / e` (≈37%).
//! - Monotone non-increasing in age. Clamp negative age to 0 so a
//!   clock-skew future timestamp doesn't inflate the bonus.

const HALF_LIFE_DAYS: f64 = 14.0;
const SECONDS_PER_DAY: f64 = 86_400.0;

/// Compute the usage bonus for a single app. `now` is the current
/// timestamp in seconds (so tests can pin it); production callers pass
/// `chrono::Utc::now().timestamp()`.
pub fn usage_bonus(launch_count: i32, last_launched: Option<i64>, now: i64) -> f64 {
    if launch_count <= 0 {
        return 0.0;
    }
    let Some(last) = last_launched else {
        // Never launched but count > 0 — shouldn't happen in practice
        // (record_launch always stamps both), but if it does, give full
        // credit so the data isn't ignored.
        return launch_count as f64;
    };
    let age_secs = ((now - last).max(0)) as f64;
    let age_days = age_secs / SECONDS_PER_DAY;
    (launch_count as f64) * (-age_days / HALF_LIFE_DAYS).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) {
        assert!(
            (a - b).abs() < 1e-9,
            "expected ≈{b}, got {a} (diff {:.3e})",
            (a - b).abs()
        );
    }

    #[test]
    fn zero_launch_count_is_zero_bonus() {
        approx_eq(usage_bonus(0, Some(1_700_000_000), 1_700_000_000), 0.0);
    }

    #[test]
    fn just_launched_returns_full_count() {
        // age = 0 → exp(0) = 1 → bonus = count
        approx_eq(usage_bonus(5, Some(1_700_000_000), 1_700_000_000), 5.0);
    }

    #[test]
    fn one_half_life_decays_to_one_over_e() {
        let now = 2_000_000_000;
        let last = now - (HALF_LIFE_DAYS * SECONDS_PER_DAY) as i64;
        let bonus = usage_bonus(1, Some(last), now);
        // e^-1 ≈ 0.3679
        assert!((bonus - (-1.0_f64).exp()).abs() < 1e-9);
    }

    #[test]
    fn two_half_lives_decays_to_one_over_e_squared() {
        let now = 2_000_000_000;
        let last = now - (2.0 * HALF_LIFE_DAYS * SECONDS_PER_DAY) as i64;
        let bonus = usage_bonus(1, Some(last), now);
        assert!((bonus - (-2.0_f64).exp()).abs() < 1e-9);
    }

    #[test]
    fn bonus_is_monotone_decreasing_in_age() {
        // Two apps launched the same number of times; the more recent
        // one must have the larger (or equal) bonus.
        let now = 2_000_000_000;
        let week_ago = now - 7 * 86_400;
        let month_ago = now - 30 * 86_400;
        let recent = usage_bonus(5, Some(week_ago), now);
        let old = usage_bonus(5, Some(month_ago), now);
        assert!(
            recent > old,
            "recent launch should outrank older launch (recent={recent}, old={old})"
        );
    }

    #[test]
    fn bonus_is_monotone_increasing_in_count() {
        // Two apps with the same last_launched; the more-launched wins.
        let now = 2_000_000_000;
        let last = now - 86_400;
        let low = usage_bonus(1, Some(last), now);
        let high = usage_bonus(10, Some(last), now);
        assert!(high > low);
    }

    #[test]
    fn future_timestamp_is_clamped_to_zero_age() {
        // Clock skew defense: if last_launched is in the future, treat
        // it as "just launched" rather than letting the formula blow up.
        let now = 1_700_000_000;
        let future = now + 10_000;
        let bonus = usage_bonus(3, Some(future), now);
        approx_eq(bonus, 3.0);
    }

    #[test]
    fn missing_last_launched_with_positive_count_gives_full_credit() {
        // Defensive edge case — shouldn't occur with record_launch, but
        // don't let bad data silently lose the ranking signal.
        approx_eq(usage_bonus(7, None, 1_700_000_000), 7.0);
    }

    #[test]
    fn negative_launch_count_is_zero() {
        // Defensive: a corrupted DB row returning -1 shouldn't poison
        // the sort with a negative bonus.
        approx_eq(usage_bonus(-1, Some(1_700_000_000), 1_700_000_000), 0.0);
    }
}
