/*
 * duration.rs
 *
 * Parse "30s", "5m", "1.5h", "0.5d". No suffix means seconds.
 * Zero means run forever (useful for process group handling without timeout).
 *
 * Uses integer math internally (nanosecond precision) to avoid pulling in
 * the ~6KB f64::from_str machinery from libstd.
 */

use alloc::format;
use alloc::string::ToString;
use core::time::Duration;

use crate::error::{Result, TimeoutError};

/// Parse "30", "30s", "1.5m", "2h", "0.5d". No suffix = seconds.
///
/// # Examples
///
/// ```
/// use darwin_timeout::duration::parse_duration;
/// use std::time::Duration;
///
/// assert_eq!(parse_duration("30").unwrap(), Duration::from_secs(30));
/// assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
/// assert_eq!(parse_duration("1.5m").unwrap(), Duration::from_secs(90));
/// assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
/// assert_eq!(parse_duration("0.5d").unwrap(), Duration::from_secs(43200));
/// assert_eq!(parse_duration("0").unwrap(), Duration::ZERO);
/// ```
pub fn parse_duration(input: &str) -> Result<Duration> {
    let input = input.trim();

    if input.is_empty() {
        return Err(TimeoutError::InvalidDuration("empty duration".to_string()));
    }

    let (num_str, suffix) = split_number_and_suffix(input);

    if num_str.is_empty() {
        return Err(TimeoutError::InvalidDuration(format!(
            "no numeric value in '{input}'"
        )));
    }

    /* parse as nanoseconds to preserve precision without floats */
    let nanos = parse_decimal_to_nanos(num_str)?;

    /* multiplier in nanoseconds, case insensitive */
    let multiplier: u128 = match suffix.to_ascii_lowercase().as_str() {
        "" | "s" => 1_000_000_000, // 1 second
        "m" => 60_000_000_000,     // 60 seconds
        "h" => 3_600_000_000_000,  // 3600 seconds
        "d" => 86_400_000_000_000, // 86400 seconds
        _ => {
            return Err(TimeoutError::InvalidDuration(format!(
                "invalid suffix '{suffix}'"
            )));
        }
    };

    let total_nanos = nanos
        .checked_mul(multiplier)
        .and_then(|n| n.checked_div(1_000_000_000)) // scale back from fixed-point
        .ok_or(TimeoutError::DurationOverflow)?;

    /* cap at u64::MAX seconds */
    if total_nanos > u64::MAX as u128 {
        return Err(TimeoutError::DurationOverflow);
    }

    let secs = (total_nanos / 1_000_000_000) as u64;
    let subsec_nanos = (total_nanos % 1_000_000_000) as u32;

    Ok(Duration::new(secs, subsec_nanos))
}

/// Parse decimal string to fixed-point nanoseconds (9 decimal places).
/// "1.5" -> 1_500_000_000 (representing 1.5 in fixed-point)
fn parse_decimal_to_nanos(s: &str) -> Result<u128> {
    /* check for negative */
    if s.starts_with('-') {
        return Err(TimeoutError::NegativeDuration);
    }

    let (int_part, frac_part) = match s.find('.') {
        Some(pos) => (&s[..pos], &s[pos + 1..]),
        None => (s, ""),
    };

    /* parse integer part */
    let int_val: u128 = if int_part.is_empty() {
        0
    } else {
        int_part
            .parse()
            .map_err(|_| TimeoutError::InvalidDuration(format!("invalid number '{s}'")))?
    };

    /* parse fractional part, pad/truncate to 9 digits */
    let frac_val: u128 = if frac_part.is_empty() {
        0
    } else {
        /* stack buffer - no heap allocation */
        let mut frac_buf = [b'0'; 9];
        for (i, b) in frac_part.bytes().take(9).enumerate() {
            if !b.is_ascii_digit() {
                return Err(TimeoutError::InvalidDuration(format!(
                    "invalid number '{s}'"
                )));
            }
            frac_buf[i] = b;
        }
        /* SAFETY: frac_buf contains only ASCII digits */
        let frac_str = unsafe { core::str::from_utf8_unchecked(&frac_buf) };
        frac_str
            .parse()
            .map_err(|_| TimeoutError::InvalidDuration(format!("invalid number '{s}'")))?
    };

    /* combine: int_val * 10^9 + frac_val */
    int_val
        .checked_mul(1_000_000_000)
        .and_then(|n| n.checked_add(frac_val))
        .ok_or(TimeoutError::DurationOverflow)
}

/* find where the number ends and suffix begins */
fn split_number_and_suffix(input: &str) -> (&str, &str) {
    let suffix_start = input
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_ascii_digit() || *c == '.')
        .map_or(0, |(i, c)| i + c.len_utf8());

    (&input[..suffix_start], &input[suffix_start..])
}

/* zero duration = no timeout, run forever */
#[must_use]
pub const fn is_no_timeout(duration: &Duration) -> bool {
    duration.is_zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_seconds() {
        assert_eq!(parse_duration("30").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("30S").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn test_parse_minutes() {
        assert_eq!(parse_duration("1m").unwrap(), Duration::from_secs(60));
        assert_eq!(parse_duration("1.5m").unwrap(), Duration::from_secs(90));
        assert_eq!(parse_duration("2M").unwrap(), Duration::from_secs(120));
    }

    #[test]
    fn test_parse_hours() {
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("2H").unwrap(), Duration::from_secs(7200));
    }

    #[test]
    fn test_parse_days() {
        assert_eq!(parse_duration("1d").unwrap(), Duration::from_secs(86400));
        assert_eq!(parse_duration("0.5D").unwrap(), Duration::from_secs(43200));
    }

    #[test]
    fn test_parse_fractional() {
        assert_eq!(parse_duration("0.5").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("0.001s").unwrap(), Duration::from_millis(1));
    }

    #[test]
    fn test_parse_zero() {
        assert_eq!(parse_duration("0").unwrap(), Duration::ZERO);
        assert!(is_no_timeout(&parse_duration("0").unwrap()));
    }

    #[test]
    fn test_parse_whitespace() {
        assert_eq!(parse_duration("  30s  ").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn test_invalid_empty() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("   ").is_err());
    }

    #[test]
    fn test_invalid_suffix() {
        assert!(parse_duration("30x").is_err());
        assert!(parse_duration("30ms").is_err());
    }

    #[test]
    fn test_invalid_negative() {
        assert!(matches!(
            parse_duration("-5"),
            Err(TimeoutError::NegativeDuration)
        ));
    }

    #[test]
    fn test_invalid_format() {
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("s").is_err());
    }
}
