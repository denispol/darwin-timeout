/*
 * duration.rs
 *
 * Parse "30s", "5m", "1.5h", "0.5d". No suffix means seconds.
 * Zero means run forever (useful for process group handling without timeout).
 * Cap at u64::MAX seconds so we don't panic. Case insensitive.
 */

use std::time::Duration;

use crate::error::{Result, TimeoutError};

/* cap at u64::MAX seconds - nobody needs a 292-year timeout */
#[allow(clippy::cast_precision_loss)]
const MAX_SECONDS: f64 = u64::MAX as f64;

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

    let value: f64 = num_str
        .parse()
        .map_err(|_| TimeoutError::InvalidDuration(format!("invalid number '{num_str}'")))?;

    if value < 0.0 {
        return Err(TimeoutError::NegativeDuration);
    }

    if value.is_nan() {
        return Err(TimeoutError::InvalidDuration(
            "NaN is not allowed".to_string(),
        ));
    }
    if value.is_infinite() {
        return Err(TimeoutError::DurationOverflow);
    }

    /* convert to seconds, case insensitive */
    let multiplier = match suffix.to_ascii_lowercase().as_str() {
        "" | "s" => 1.0,
        "m" => 60.0,
        "h" => 3600.0,
        "d" => 86400.0,
        _ => {
            return Err(TimeoutError::InvalidDuration(format!(
                "invalid suffix '{suffix}'"
            )));
        }
    };

    let total_seconds = value * multiplier;

    if total_seconds > MAX_SECONDS {
        return Err(TimeoutError::DurationOverflow);
    }

    Ok(Duration::from_secs_f64(total_seconds))
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
