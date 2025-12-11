/*
 * rlimit.rs
 *
 * resource limit parsing and application (RLIMIT_AS, RLIMIT_CPU).
 * no floats, integer-only parsing for binary size and determinism.
 */

use alloc::format;
use alloc::string::ToString;
use core::num::NonZeroU32;
use core::time::Duration;

use crate::error::{Result, TimeoutError};

#[derive(Debug, Clone, Copy, Default)]
pub struct ResourceLimits {
    pub mem_bytes: Option<u64>, /* RLIMIT_AS */
    pub cpu_time: Option<Duration>, /* RLIMIT_CPU (seconds) */
}

impl ResourceLimits {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.mem_bytes.is_none() && self.cpu_time.is_none()
    }
}

/* parse memory strings like 1G, 512M, 1024, 64K. binary units. */
pub fn parse_mem_limit(input: &str) -> Result<u64> {
    let s = input.trim();
    if s.is_empty() {
        return Err(TimeoutError::InvalidMemoryLimit("empty".to_string()));
    }

    let (num, suffix) = split_number_suffix(s);
    let value = parse_u64(num).map_err(|_| {
        TimeoutError::InvalidMemoryLimit(format!("invalid memory value: '{s}'"))
    })?;

    let mult: u64 = match suffix.to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" => 1024,
        "m" | "mb" => 1024 * 1024,
        "g" | "gb" => 1024 * 1024 * 1024,
        "t" | "tb" => 1024_u64
            .saturating_mul(1024)
            .saturating_mul(1024)
            .saturating_mul(1024),
        _ => {
            return Err(TimeoutError::InvalidMemoryLimit(format!(
                "invalid memory suffix in '{s}'"
            )))
        }
    };

    value
        .checked_mul(mult)
        .ok_or_else(|| TimeoutError::InvalidMemoryLimit(format!("memory limit overflow: '{s}'")))
}

/* parse cpu time like 50s, 2m, 1h using Duration parser to reuse logic. */
pub fn parse_cpu_time(input: &str) -> Result<Duration> {
    let dur = crate::duration::parse_duration(input)?;
    Ok(dur)
}

/* parse cpu percent. allows >100 for multi-core (e.g., 400 = 4 cores max).
 * value is unbounded - will naturally max at machine's available cores. */
pub fn parse_cpu_percent(input: &str) -> Result<NonZeroU32> {
    let val: u32 = input
        .trim()
        .parse()
        .map_err(|_| TimeoutError::InvalidCpuPercent(format!("invalid cpu percent: '{input}'")))?;
    
    if val == 0 {
        return Err(TimeoutError::InvalidCpuPercent(format!(
            "cpu percent must be > 0: {val}"
        )));
    }
    Ok(NonZeroU32::new(val).unwrap())
}

pub fn apply_limits(limits: &ResourceLimits) -> Result<()> {
    /* apply RLIMIT_AS if set
     * NOTE: macOS does NOT enforce RLIMIT_AS (returns EINVAL).
     * We try anyway for potential future support or compatibility. */
    if let Some(bytes) = limits.mem_bytes {
        let rlim = libc::rlimit {
            rlim_cur: bytes,
            rlim_max: bytes,
        };
        let ret = unsafe { libc::setrlimit(libc::RLIMIT_AS, &rlim) };
        if ret != 0 {
            /* EINVAL on macOS is expected - silently continue.
             * Other errors are still reported. */
            let e = errno();
            if e != libc::EINVAL {
                return Err(TimeoutError::ResourceLimitError(e));
            }
        }
    }

    if let Some(cpu) = limits.cpu_time {
        /* RLIMIT_CPU uses seconds granularity */
        let secs = cpu.as_secs();
        let rlim = libc::rlimit {
            rlim_cur: secs,
            rlim_max: secs,
        };
        let ret = unsafe { libc::setrlimit(libc::RLIMIT_CPU, &rlim) };
        if ret != 0 {
            return Err(TimeoutError::ResourceLimitError(errno()));
        }
    }

    Ok(())
}

#[inline]
fn split_number_suffix(s: &str) -> (&str, &str) {
    let mut idx = s.len();
    for (i, c) in s.char_indices().rev() {
        if c.is_ascii_alphabetic() {
            idx = i;
        } else {
            break;
        }
    }
    if idx == s.len() {
        (s, "")
    } else {
        s.split_at(idx)
    }
}

#[inline]
fn parse_u64(s: &str) -> core::result::Result<u64, ()> {
    let mut result: u64 = 0;
    for b in s.as_bytes() {
        if !b.is_ascii_digit() {
            return Err(());
        }
        let digit = (b - b'0') as u64;
        result = result
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or(())?;
    }
    Ok(result)
}

#[inline]
fn errno() -> i32 {
    unsafe extern "C" {
        fn __error() -> *mut i32;
    }
    // SAFETY: __error always returns valid pointer on macOS.
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    unsafe {
        *__error()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_cpu_percent_basic() {
        assert_eq!(parse_cpu_percent("50").unwrap().get(), 50);
        assert_eq!(parse_cpu_percent("100").unwrap().get(), 100);
        assert_eq!(parse_cpu_percent("1").unwrap().get(), 1);
    }
    
    #[test]
    fn test_parse_cpu_percent_multicore() {
        /* multi-core values should be allowed */
        assert_eq!(parse_cpu_percent("200").unwrap().get(), 200);
        assert_eq!(parse_cpu_percent("400").unwrap().get(), 400);
        assert_eq!(parse_cpu_percent("1400").unwrap().get(), 1400); /* 14-core M4 Pro */
    }
    
    #[test]
    fn test_parse_cpu_percent_zero_rejected() {
        assert!(parse_cpu_percent("0").is_err());
    }
    
    #[test]
    fn test_parse_cpu_percent_invalid() {
        assert!(parse_cpu_percent("").is_err());
        assert!(parse_cpu_percent("abc").is_err());
        assert!(parse_cpu_percent("-50").is_err());
        assert!(parse_cpu_percent("50%").is_err());
    }
    
    #[test]
    fn test_parse_cpu_percent_whitespace() {
        assert_eq!(parse_cpu_percent("  50  ").unwrap().get(), 50);
        assert_eq!(parse_cpu_percent("\t100\n").unwrap().get(), 100);
    }
    
    #[test]
    fn test_parse_mem_limit_basic() {
        assert_eq!(parse_mem_limit("1024").unwrap(), 1024);
        assert_eq!(parse_mem_limit("1K").unwrap(), 1024);
        assert_eq!(parse_mem_limit("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_mem_limit("1G").unwrap(), 1024 * 1024 * 1024);
    }
    
    #[test]
    fn test_parse_mem_limit_case_insensitive() {
        assert_eq!(parse_mem_limit("1k").unwrap(), 1024);
        assert_eq!(parse_mem_limit("1m").unwrap(), 1024 * 1024);
        assert_eq!(parse_mem_limit("1g").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_mem_limit("512MB").unwrap(), 512 * 1024 * 1024);
        assert_eq!(parse_mem_limit("2gb").unwrap(), 2 * 1024 * 1024 * 1024);
    }
    
    #[test]
    fn test_parse_mem_limit_zero() {
        /* zero is technically valid (no limit), but useless */
        assert_eq!(parse_mem_limit("0").unwrap(), 0);
        assert_eq!(parse_mem_limit("0M").unwrap(), 0);
    }
}
