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
 * Uses u64::checked_sub which returns None only when now < start,
 * correctly handling all valid u64 time differences.
 */
#[inline]
pub const fn elapsed_ns(start_ns: u64, now_ns: u64) -> Option<u64> {
    now_ns.checked_sub(start_ns)
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
pub const fn idle_timeout_exceeded(
    last_activity_ns: u64,
    now_ns: u64,
    timeout_ns: u64,
) -> Option<bool> {
    match elapsed_ns(last_activity_ns, now_ns) {
        Some(idle_ns) => Some(idle_ns >= timeout_ns),
        None => None,
    }
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
pub const fn time_to_idle_timeout(
    last_activity_ns: u64,
    now_ns: u64,
    timeout_ns: u64,
) -> Option<u64> {
    match elapsed_ns(last_activity_ns, now_ns) {
        Some(idle_ns) => Some(timeout_ns.saturating_sub(idle_ns)),
        None => None,
    }
}

/* -------------------------------------------------------------------------- */
/*                              kani proofs                                   */
/* -------------------------------------------------------------------------- */

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /*
     * verify elapsed_ns returns None when clock goes backwards (now < start).
     * this catches invariant violations rather than silently clamping to 0.
     */
    #[kani::proof]
    fn verify_elapsed_ns_none_on_backwards() {
        let start: u64 = kani::any();
        let now: u64 = kani::any();

        let result = elapsed_ns(start, now);

        /* if now < start, must return None (backwards clock) */
        if now < start {
            kani::assert(
                result.is_none(),
                "elapsed_ns should return None when now < start",
            );
        } else {
            /* if now >= start, must return Some with correct difference */
            kani::assert(
                result.is_some(),
                "elapsed_ns should return Some when now >= start",
            );
            kani::assert(
                result.unwrap() == now - start,
                "elapsed_ns should return correct difference",
            );
        }
    }

    /*
     * verify remaining_ns saturates to 0 when past deadline (no negative values).
     * unlike elapsed_ns, overshoot is expected and should clamp, not error.
     */
    #[kani::proof]
    fn verify_remaining_ns_saturates_to_zero() {
        let now: u64 = kani::any();
        let deadline: u64 = kani::any();

        let result = remaining_ns(now, deadline);

        /* result must never be negative (guaranteed by u64, but verify logic) */
        if now >= deadline {
            kani::assert(result == 0, "remaining_ns should be 0 when past deadline");
        } else {
            kani::assert(
                result == deadline - now,
                "remaining_ns should return correct remaining time",
            );
        }
    }

    /*
     * verify advance_ns saturates on overflow instead of wrapping.
     * wrapping would create a small deadline from a large one - catastrophic.
     */
    #[kani::proof]
    fn verify_advance_ns_saturates_on_overflow() {
        let base: u64 = kani::any();
        let offset: u64 = kani::any();

        let result = advance_ns(base, offset);

        /* check for overflow scenario */
        if base.checked_add(offset).is_none() {
            kani::assert(
                result == u64::MAX,
                "advance_ns should saturate to MAX on overflow",
            );
        } else {
            kani::assert(
                result == base + offset,
                "advance_ns should return correct sum when no overflow",
            );
        }
    }

    /*
     * verify adjust_ns returns None on underflow (would go negative).
     */
    #[kani::proof]
    fn verify_adjust_ns_none_on_underflow() {
        let base: u64 = kani::any();
        let offset: i64 = kani::any();

        let result = adjust_ns(base, offset);

        /* check underflow: base + offset < 0 when offset is negative */
        if offset < 0 && (offset.unsigned_abs() > base) {
            kani::assert(
                result.is_none(),
                "adjust_ns should return None on underflow",
            );
        }
        /* check overflow: base + offset > u64::MAX when offset is positive */
        else if offset > 0 && base.checked_add(offset as u64).is_none() {
            kani::assert(result.is_none(), "adjust_ns should return None on overflow");
        } else {
            kani::assert(
                result.is_some(),
                "adjust_ns should return Some for valid inputs",
            );
        }
    }

    /*
     * verify deadline_reached is consistent with remaining_ns.
     */
    #[kani::proof]
    fn verify_deadline_reached_consistency() {
        let now: u64 = kani::any();
        let deadline: u64 = kani::any();

        let reached = deadline_reached(now, deadline);
        let remaining = remaining_ns(now, deadline);

        /* deadline reached iff remaining time is 0 */
        kani::assert(
            reached == (remaining == 0),
            "deadline_reached should be true iff remaining_ns is 0",
        );
    }
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
    fn test_elapsed_ns_large_delta() {
        /* difference exceeds i64::MAX but is valid u64 - should succeed */
        let start = 0;
        let now = i64::MAX as u64 + 100;
        /* checked_sub correctly handles this, unlike checked_signed_diff */
        assert_eq!(elapsed_ns(start, now), Some(now));
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
