/*
 * time_math.rs
 *
 * Compile-time safe timing calculations using Rust 1.91.0+ strict/checked
 * integer operations. These methods provide fine-grained control over
 * mixed signed/unsigned arithmetic critical for timeout calculations.
 *
 * Why not saturating_sub everywhere?
 * - saturating_sub(a, b) returns 0 when b > a, silently masking bugs
 * - checked_signed_diff detects invariant violations (e.g., now < start)
 * - strict_add_signed panics on overflow in debug, UB-free in release
 *
 * Key operations:
 * - elapsed_ns(start, now): time difference with invariant checking
 * - remaining_ns(now, deadline): clamped remaining time (0 on overshoot)
 * - advance_deadline(base, offset_ns): safely add signed offset to u64
 */

/*
 * Calculate elapsed time in nanoseconds: now - start
 *
 * Returns None if now < start (invariant violation - clock went backwards
 * or arguments swapped). Callers should handle this as a bug, not silently
 * clamp to 0 like saturating_sub would.
 *
 * Uses u64::checked_signed_diff (stabilized 1.90.0) which returns Option<i64>.
 * Since we expect now >= start, a negative result indicates an error.
 */
#[inline]
pub fn elapsed_ns(start_ns: u64, now_ns: u64) -> Option<u64> {
    /* checked_signed_diff returns Some(diff) where diff = self - other as i64 */
    /* if diff would overflow i64 (> 2^63-1), returns None */
    /* if diff is negative, the i64 is negative */
    now_ns.checked_signed_diff(start_ns).and_then(|diff| {
        if diff >= 0 {
            /* safe: non-negative i64 always fits in u64 */
            Some(diff as u64)
        } else {
            /* now < start: invariant violation */
            None
        }
    })
}

/*
 * Calculate remaining time until deadline, clamped to 0 on overshoot.
 *
 * Unlike elapsed_ns, overshooting a deadline is expected (not a bug).
 * Returns 0 when now >= deadline, otherwise returns deadline - now.
 *
 * Uses saturating_sub intentionally - deadline overshoot is normal.
 */
#[inline]
pub const fn remaining_ns(now_ns: u64, deadline_ns: u64) -> u64 {
    deadline_ns.saturating_sub(now_ns)
}

/*
 * Check if deadline has been reached: now >= deadline
 *
 * Clearer intent than `remaining_ns(now, deadline) == 0`
 */
#[inline]
pub const fn deadline_reached(now_ns: u64, deadline_ns: u64) -> bool {
    now_ns >= deadline_ns
}

/*
 * Advance a timestamp by a nanosecond offset.
 *
 * Uses saturating_add - overflow to u64::MAX is acceptable for deadlines
 * (effectively "never timeout" rather than wrap to small value).
 */
#[inline]
pub const fn advance_ns(base_ns: u64, offset_ns: u64) -> u64 {
    base_ns.saturating_add(offset_ns)
}

/*
 * Adjust a timestamp by a signed offset (can be negative).
 *
 * Uses u64::checked_add_signed (stabilized 1.66.0) for safety.
 * Returns None if result would be negative or overflow u64.
 *
 * Useful for clock skew adjustments or relative time calculations.
 */
#[inline]
pub const fn adjust_ns(base_ns: u64, offset_ns: i64) -> Option<u64> {
    base_ns.checked_add_signed(offset_ns)
}

/*
 * Check if idle timeout exceeded: (now - last_activity) >= timeout
 *
 * Returns:
 * - Some(true) if idle timeout exceeded
 * - Some(false) if still within timeout
 * - None if now < last_activity (invariant violation)
 *
 * Using checked_signed_diff ensures we detect clock anomalies rather than
 * silently returning "not idle" due to saturating_sub clamping to 0.
 */
#[inline]
pub fn idle_timeout_exceeded(last_activity_ns: u64, now_ns: u64, timeout_ns: u64) -> Option<bool> {
    elapsed_ns(last_activity_ns, now_ns).map(|idle_ns| idle_ns >= timeout_ns)
}

/*
 * Calculate time remaining until idle timeout.
 *
 * Returns:
 * - Some(remaining) if still within timeout (remaining > 0)
 * - Some(0) if timeout already exceeded
 * - None if now < last_activity (invariant violation)
 */
#[inline]
pub fn time_to_idle_timeout(last_activity_ns: u64, now_ns: u64, timeout_ns: u64) -> Option<u64> {
    elapsed_ns(last_activity_ns, now_ns).map(|idle_ns| timeout_ns.saturating_sub(idle_ns))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elapsed_ns_normal() {
        /* normal case: now > start */
        assert_eq!(elapsed_ns(100, 150), Some(50));
        assert_eq!(elapsed_ns(0, 1_000_000_000), Some(1_000_000_000));
    }

    #[test]
    fn test_elapsed_ns_same() {
        /* edge case: now == start */
        assert_eq!(elapsed_ns(100, 100), Some(0));
    }

    #[test]
    fn test_elapsed_ns_backwards() {
        /* invariant violation: now < start */
        assert_eq!(elapsed_ns(150, 100), None);
        assert_eq!(elapsed_ns(1, 0), None);
    }

    #[test]
    fn test_elapsed_ns_large_values() {
        /* large but valid u64 values */
        let start = u64::MAX - 1000;
        let now = u64::MAX;
        assert_eq!(elapsed_ns(start, now), Some(1000));
    }

    #[test]
    fn test_elapsed_ns_overflow_i64() {
        /* difference exceeds i64::MAX - checked_signed_diff returns None */
        let start = 0;
        let now = i64::MAX as u64 + 100;
        /* this should return None because the diff can't fit in i64 */
        assert!(elapsed_ns(start, now).is_none());
    }

    #[test]
    fn test_remaining_ns_normal() {
        /* time remaining until deadline */
        assert_eq!(remaining_ns(100, 150), 50);
        assert_eq!(remaining_ns(0, 1_000_000_000), 1_000_000_000);
    }

    #[test]
    fn test_remaining_ns_at_deadline() {
        assert_eq!(remaining_ns(100, 100), 0);
    }

    #[test]
    fn test_remaining_ns_past_deadline() {
        /* overshoot clamps to 0 (intentional, not a bug) */
        assert_eq!(remaining_ns(150, 100), 0);
        assert_eq!(remaining_ns(u64::MAX, 0), 0);
    }

    #[test]
    fn test_deadline_reached() {
        assert!(!deadline_reached(99, 100));
        assert!(deadline_reached(100, 100));
        assert!(deadline_reached(101, 100));
    }

    #[test]
    fn test_advance_ns_normal() {
        assert_eq!(advance_ns(100, 50), 150);
    }

    #[test]
    fn test_advance_ns_overflow() {
        /* saturates to MAX instead of wrapping */
        assert_eq!(advance_ns(u64::MAX - 10, 100), u64::MAX);
    }

    #[test]
    fn test_adjust_ns_positive() {
        assert_eq!(adjust_ns(100, 50), Some(150));
    }

    #[test]
    fn test_adjust_ns_negative() {
        assert_eq!(adjust_ns(100, -50), Some(50));
        assert_eq!(adjust_ns(100, -100), Some(0));
    }

    #[test]
    fn test_adjust_ns_underflow() {
        /* would go negative - returns None */
        assert_eq!(adjust_ns(50, -100), None);
        assert_eq!(adjust_ns(0, -1), None);
    }

    #[test]
    fn test_adjust_ns_overflow() {
        /* would exceed u64::MAX - returns None */
        assert_eq!(adjust_ns(u64::MAX, 1), None);
    }

    #[test]
    fn test_idle_timeout_exceeded() {
        /* not yet exceeded */
        assert_eq!(idle_timeout_exceeded(100, 150, 100), Some(false));
        /* exactly at timeout */
        assert_eq!(idle_timeout_exceeded(100, 200, 100), Some(true));
        /* past timeout */
        assert_eq!(idle_timeout_exceeded(100, 250, 100), Some(true));
        /* invariant violation */
        assert_eq!(idle_timeout_exceeded(150, 100, 100), None);
    }

    #[test]
    fn test_time_to_idle_timeout() {
        /* 50ns remaining */
        assert_eq!(time_to_idle_timeout(100, 150, 100), Some(50));
        /* exactly at timeout */
        assert_eq!(time_to_idle_timeout(100, 200, 100), Some(0));
        /* past timeout */
        assert_eq!(time_to_idle_timeout(100, 250, 100), Some(0));
        /* invariant violation */
        assert_eq!(time_to_idle_timeout(150, 100, 100), None);
    }
}
