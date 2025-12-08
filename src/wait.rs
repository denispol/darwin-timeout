/*
 * wait.rs
 *
 * Wait for external conditions before starting the command.
 *
 * Currently supports: --wait-for-file <path>
 * Waits for a file to exist before proceeding. Useful for orchestration
 * scenarios where one process signals readiness by creating a file.
 *
 * Uses stat-based polling with exponential backoff (10ms → 1s) to minimize
 * CPU usage while maintaining reasonable responsiveness.
 */

use alloc::string::String;
use core::time::Duration;

use crate::args::Confine;
use crate::error::{Result, TimeoutError};
use crate::sync::AtomicOnce;

/* Timing helpers - reimplemented here to avoid circular deps with runner */
#[repr(C)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

unsafe extern "C" {
    fn mach_continuous_time() -> u64;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
    fn clock_gettime_nsec_np(clock_id: libc::clockid_t) -> u64;
    fn nanosleep(rqtp: *const libc::timespec, rmtp: *mut libc::timespec) -> i32;
}

const CLOCK_MONOTONIC_RAW: libc::clockid_t = 4;

/* Cached timebase info for mach_continuous_time conversion */
static TIMEBASE: AtomicOnce<Option<(u64, u64)>> = AtomicOnce::new();

fn get_timebase_info() -> Option<(u64, u64)> {
    TIMEBASE
        .get_or_init(|| {
            let mut info = MachTimebaseInfo { numer: 0, denom: 0 };
            // SAFETY: info is a valid MachTimebaseInfo struct with correct layout
            unsafe {
                mach_timebase_info(&raw mut info);
            }
            /* fail if denom is zero (invalid FFI data) - shouldn't happen on real hardware */
            if info.denom == 0 {
                return None;
            }
            Some((u64::from(info.numer), u64::from(info.denom)))
        })
        .as_ref()
        .copied()
}

/* Get current time in nanoseconds based on confine mode */
#[inline]
fn now_ns(confine: Confine) -> u64 {
    match confine {
        Confine::Wall => wall_now_ns(),
        Confine::Active => active_now_ns(),
    }
}

#[inline]
fn wall_now_ns() -> u64 {
    let (numer, denom) = get_timebase_info().unwrap_or((1, 1));

    // SAFETY: mach_continuous_time() has no preconditions
    let abs_time = unsafe { mach_continuous_time() };

    if numer == denom {
        return abs_time;
    }

    /* use checked_div for safety, fallback to abs_time on error */
    let intermediate = u128::from(abs_time).saturating_mul(u128::from(numer));
    #[allow(clippy::cast_possible_truncation)]
    intermediate
        .checked_div(u128::from(denom))
        .map(|r| r as u64)
        .unwrap_or(abs_time)
}

#[inline]
fn active_now_ns() -> u64 {
    // SAFETY: clock_gettime_nsec_np with valid clock_id always succeeds
    unsafe { clock_gettime_nsec_np(CLOCK_MONOTONIC_RAW) }
}

/* Sleep for given milliseconds, handling EINTR by continuing with remaining time */
fn sleep_ms(ms: u64) {
    let mut ts = libc::timespec {
        tv_sec: (ms / 1000) as libc::time_t,
        tv_nsec: ((ms % 1000) * 1_000_000) as libc::c_long,
    };
    let mut remaining = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: ts and remaining are valid timespecs
    while unsafe { nanosleep(&ts, &mut remaining) } != 0 {
        /* EINTR - continue sleeping with remaining time */
        if errno() != libc::EINTR {
            break; /* other error, give up */
        }
        ts = remaining;
        if ts.tv_sec == 0 && ts.tv_nsec == 0 {
            break; /* no time remaining */
        }
    }
}

/* Duration to milliseconds as u64 (avoids u128 from as_millis()) */
#[inline]
fn duration_ms(d: Duration) -> u64 {
    d.as_secs()
        .saturating_mul(1000)
        .saturating_add(u64::from(d.subsec_millis()))
}

/*
 * kqueue-based delay - zero CPU while waiting, handles EINTR.
 * used for retry delays between command attempts.
 *
 * returns true if delay completed normally, false if interrupted by signal.
 * caller should check signal pipe after false return.
 */
pub fn kqueue_delay(d: Duration, signal_fd: Option<i32>) -> bool {
    if d.is_zero() {
        return true;
    }

    /* track deadline for EINTR recalculation */
    let start_ns = active_now_ns();
    let deadline_ns = start_ns.saturating_add(duration_to_ns(d));

    /* create kqueue fd */
    // SAFETY: kqueue() has no preconditions, returns -1 on error
    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        /* fallback to nanosleep on kqueue failure */
        sleep_ms(duration_ms(d));
        return true;
    }

    /* max ns that fits in isize (~292 years on 64-bit) */
    const MAX_TIMER_NS: u64 = isize::MAX as u64;

    let num_changes: i32 = if signal_fd.is_some() { 2 } else { 1 };

    let mut event = libc::kevent {
        ident: 0,
        filter: 0,
        flags: 0,
        fflags: 0,
        data: 0,
        udata: core::ptr::null_mut(),
    };

    loop {
        /* recalculate remaining time after each EINTR */
        let current_ns = active_now_ns();
        if current_ns >= deadline_ns {
            // SAFETY: kq is valid fd
            unsafe { libc::close(kq) };
            return true; /* deadline reached */
        }
        let remaining_ns = deadline_ns.saturating_sub(current_ns);

        /* set up timer event, optionally watch signal pipe */
        #[allow(clippy::cast_possible_wrap)]
        let changes = [
            libc::kevent {
                ident: 1,
                filter: libc::EVFILT_TIMER,
                flags: libc::EV_ADD | libc::EV_ONESHOT,
                fflags: libc::NOTE_NSECONDS,
                data: remaining_ns.min(MAX_TIMER_NS) as isize,
                udata: core::ptr::null_mut(),
            },
            libc::kevent {
                ident: signal_fd.unwrap_or(0) as usize,
                filter: libc::EVFILT_READ,
                flags: if signal_fd.is_some() { libc::EV_ADD } else { 0 },
                fflags: 0,
                data: 0,
                udata: core::ptr::null_mut(),
            },
        ];

        // SAFETY: kq is valid, changes/event are valid kevent structs
        #[allow(clippy::cast_possible_wrap)]
        let n = unsafe {
            libc::kevent(
                kq,
                changes.as_ptr(),
                num_changes,
                &raw mut event,
                1,
                core::ptr::null(),
            )
        };

        if n < 0 {
            let err = errno();
            if err == libc::EINTR {
                continue; /* retry with recalculated timer */
            }
            /* other error - close and fallback with REMAINING time */
            // SAFETY: kq is valid fd
            unsafe { libc::close(kq) };
            let remaining_ms = (deadline_ns.saturating_sub(active_now_ns())) / 1_000_000;
            if remaining_ms > 0 {
                sleep_ms(remaining_ms);
            }
            return true;
        }
        break;
    }

    // SAFETY: kq is valid fd
    unsafe { libc::close(kq) };

    /* check what woke us up */
    if event.filter == libc::EVFILT_READ {
        /* signal pipe readable - signal received during delay */
        return false;
    }

    true /* timer expired normally */
}

/* Check if file exists using stat */
fn file_exists(path: &str) -> core::result::Result<bool, i32> {
    /* Need null-terminated string for libc */
    let mut path_buf = [0u8; 4096];
    let path_bytes = path.as_bytes();
    if path_bytes.len() >= path_buf.len() {
        return Err(libc::ENAMETOOLONG);
    }
    path_buf[..path_bytes.len()].copy_from_slice(path_bytes);
    // path_buf is already zero-initialized, so null terminator is in place

    // SAFETY: libc::stat is a C struct with no invalid bit patterns; zeroing is valid.
    let mut stat_buf: libc::stat = unsafe { core::mem::zeroed() };

    // SAFETY: path_buf is null-terminated, stat_buf is valid
    let ret = unsafe { libc::stat(path_buf.as_ptr().cast(), &raw mut stat_buf) };

    if ret == 0 {
        Ok(true)
    } else {
        let err = errno();
        if err == libc::ENOENT || err == libc::ENOTDIR {
            Ok(false) // File doesn't exist (yet)
        } else {
            Err(err) // Real error (permission denied, etc.)
        }
    }
}

/* Get errno - on macOS this is a thread-local via __error() */
#[inline]
fn errno() -> i32 {
    unsafe extern "C" {
        fn __error() -> *mut i32;
    }
    // SAFETY: __error always returns valid pointer on macOS
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    unsafe {
        *__error()
    }
}

/* Duration to nanoseconds */
#[inline]
fn duration_to_ns(d: Duration) -> u64 {
    d.as_secs()
        .saturating_mul(1_000_000_000)
        .saturating_add(u64::from(d.subsec_nanos()))
}

/// Wait for a file to exist.
///
/// Uses exponential backoff polling: starts at 10ms, caps at 1s.
/// If timeout is None, waits indefinitely.
///
/// # Race Condition (TOCTOU)
///
/// There is an inherent race window between when this function returns
/// and when the caller uses the file. If the file could be deleted
/// between detection and use, callers should handle ENOENT gracefully.
/// For atomic coordination, consider using file locks or advisory locks.
///
/// # Errors
///
/// - `WaitForFileTimeout` if timeout expires before file appears
/// - `WaitForFileError` if stat() fails with an error other than ENOENT
pub fn wait_for_file(path: &str, timeout: Option<Duration>, confine: Confine) -> Result<()> {
    /* Check immediately first - avoid sleeping if file already exists */
    match file_exists(path) {
        Ok(true) => return Ok(()),
        Ok(false) => { /* Continue to wait loop */ }
        Err(e) => return Err(TimeoutError::WaitForFileError(String::from(path), e)),
    }

    let deadline_ns = timeout.map(|d| now_ns(confine).saturating_add(duration_to_ns(d)));

    /* Exponential backoff: 10ms -> 20ms -> 40ms -> ... -> 1000ms (cap) */
    const INITIAL_POLL_MS: u64 = 10;
    const MAX_POLL_MS: u64 = 1000;
    let mut poll_interval_ms = INITIAL_POLL_MS;

    loop {
        /* Check timeout BEFORE sleeping to avoid overshoot */
        let sleep_time = if let Some(dl) = deadline_ns {
            let current = now_ns(confine);
            if current >= dl {
                return Err(TimeoutError::WaitForFileTimeout(String::from(path)));
            }
            /* cap sleep to remaining time */
            let remaining_ms = (dl.saturating_sub(current)) / 1_000_000;
            poll_interval_ms.min(remaining_ms.max(1))
        } else {
            poll_interval_ms
        };

        sleep_ms(sleep_time);

        /* Check if file exists */
        match file_exists(path) {
            Ok(true) => return Ok(()),
            Ok(false) => { /* Continue waiting */ }
            Err(e) => return Err(TimeoutError::WaitForFileError(String::from(path), e)),
        }

        /* Increase poll interval with exponential backoff */
        poll_interval_ms = (poll_interval_ms * 2).min(MAX_POLL_MS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_file_exists_when_present() {
        // Cargo.toml definitely exists in the project root
        let result = file_exists("Cargo.toml");
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn test_file_exists_when_absent() {
        let result = file_exists("/tmp/nonexistent_file_12345");
        assert_eq!(result, Ok(false));
    }

    #[test]
    fn test_wait_for_file_already_exists() {
        // Should return immediately for existing file
        let result = wait_for_file(
            "Cargo.toml",
            Some(Duration::from_millis(100)),
            Confine::Wall,
        );
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)] // Miri doesn't support mach_continuous_time
    fn test_wait_for_file_timeout() {
        // Should timeout waiting for nonexistent file
        let start = std::time::Instant::now();
        let result = wait_for_file(
            "/tmp/this_file_will_never_exist_98765",
            Some(Duration::from_millis(50)),
            Confine::Wall,
        );
        let elapsed = start.elapsed();

        assert!(matches!(result, Err(TimeoutError::WaitForFileTimeout(_))));
        // Should have taken at least 50ms (the timeout)
        assert!(elapsed >= Duration::from_millis(50));
        // But not too long (allow margin for CI scheduling jitter)
        assert!(elapsed < Duration::from_millis(500));
    }

    #[test]
    #[cfg_attr(miri, ignore)] // Miri doesn't support mach_continuous_time
    fn test_wait_for_file_created_during_wait() {
        let test_file = "/tmp/darwin_timeout_test_wait_file";

        // Clean up any leftover file
        let _ = fs::remove_file(test_file);

        // Spawn thread to create file after a delay
        let path = test_file.to_string();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            fs::write(&path, "ready").unwrap();
        });

        // Wait for file (should succeed after ~30ms)
        let start = std::time::Instant::now();
        let result = wait_for_file(test_file, Some(Duration::from_secs(1)), Confine::Wall);
        let elapsed = start.elapsed();

        // Clean up
        let _ = fs::remove_file(test_file);

        assert!(result.is_ok());
        // Should have completed in a reasonable time
        assert!(elapsed < Duration::from_millis(500));
    }
}
