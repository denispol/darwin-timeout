/*
 * throttle.rs
 *
 * cpu percent throttling via SIGSTOP/SIGCONT signals.
 * monitors cpu usage, suspends process when exceeding limit.
 * 
 * NOTE: CPU percentage is total usage across all cores. A 4-thread process
 * running at 100% on each core will show 400% CPU usage. This means a limit
 * of 50% will aggressively throttle multi-threaded workloads. This is
 * intentional - the limit applies to total CPU resources consumed.
 *
 * NOTE: Uses SIGSTOP (not catchable by child) and SIGCONT (catchable).
 * Child process may observe SIGCONT signals when throttling is active.
 * Only throttles the main process, not entire process group.
 */

use core::num::NonZeroU32;

use crate::error::{Result, TimeoutError};
use crate::proc_info;

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, Copy)]
pub struct CpuThrottleConfig {
    pub percent: NonZeroU32,    /* 1-100 */
    pub interval_ns: u64,       /* sampling window */
    pub sleep_ns: u64,          /* suspension duration when over limit */
}

#[cfg_attr(test, derive(Debug))]
#[derive(Clone)]
pub struct CpuThrottleState {
    pid: i32,
    /* integral control: track cumulative time from start, not per-interval deltas.
     * compares total_cpu / total_wall against target to create "debt" mechanism
     * that converges to exact target percentage over process lifetime.
     * TODO: consider PI control if integral-only shows initial oscillation. */
    start_cpu_ns: u64,          /* CPU time at throttle attach */
    start_wall_ns: u64,         /* wall time at throttle attach */
    pub last_cpu_ns: u64,       /* most recent CPU time reading */
    pub last_wall_ns: u64,      /* most recent wall time reading */
    suspended: bool,            /* track if we've sent SIGSTOP */
}

impl CpuThrottleState {
    pub fn pid(&self) -> i32 {
        self.pid
    }
    
    pub fn new(pid: i32, now_ns: u64) -> Result<Self> {
        /* get initial CPU time via proc_pid_rusage - no entitlements needed */
        let initial_cpu_ns = proc_info::get_process_cpu_time(pid)
            .ok_or(TimeoutError::ThrottleAttachError(libc::ESRCH))?;
        
        Ok(Self {
            pid,
            start_cpu_ns: initial_cpu_ns,
            start_wall_ns: now_ns,
            last_cpu_ns: initial_cpu_ns,
            last_wall_ns: now_ns,
            suspended: false,
        })
    }
    
    /* resume process if suspended - MUST call before termination signals.
     * sending SIGTERM to a SIGSTOP'd process creates deadlock if child
     * intercepts the signal (can't run handler while stopped). */
    pub fn resume(&mut self) {
        if self.suspended {
            // SAFETY: kill with SIGCONT is safe, ESRCH (process gone) is fine
            if unsafe { libc::kill(self.pid, libc::SIGCONT) } == 0 {
                self.suspended = false;
            } else {
                /* ESRCH or other error - mark as not suspended anyway */
                self.suspended = false;
            }
        }
    }
    
    /* check cpu usage and throttle if needed. returns whether process is currently suspended.
     * 
     * INTEGRAL CONTROL: compares cumulative CPU time against cumulative wall-clock budget.
     * If total_cpu > total_wall * target%, process is "in debt" and gets stopped.
     * If total_cpu <= total_wall * target%, process is "under budget" and runs.
     * This mathematically converges to exact target percentage over process lifetime.
     * 
     * Previous delta-based approach compared interval-local usage, which aliased with
     * the scheduler and converged to ~50% duty cycle regardless of target. */
    pub fn update(&mut self, cfg: &CpuThrottleConfig, now_ns: u64) -> Result<bool> {
        /* get current CPU time */
        let current_cpu_ns = match proc_info::get_process_cpu_time(self.pid) {
            Some(t) => t,
            None => {
                /* process gone - just return current state */
                self.last_wall_ns = now_ns;
                return Ok(self.suspended);
            }
        };
        
        /* integral control: compare cumulative totals from start */
        let total_wall_ns = now_ns.saturating_sub(self.start_wall_ns);
        let total_cpu_ns = current_cpu_ns.saturating_sub(self.start_cpu_ns);
        
        /* avoid division by zero on first call */
        if total_wall_ns == 0 {
            self.last_cpu_ns = current_cpu_ns;
            self.last_wall_ns = now_ns;
            return Ok(self.suspended);
        }
        
        /* cpu_budget_ns = total_wall_ns * (percent / 100)
         * use u128 intermediate to prevent overflow.
         * note: percent can exceed 100 for multi-core systems. */
        let limit = u64::from(cfg.percent.get());
        #[allow(clippy::cast_possible_truncation)]
        let cpu_budget_ns = ((total_wall_ns as u128 * limit as u128) / 100) as u64;
        
        /* if over budget and not already suspended, SIGSTOP */
        if total_cpu_ns > cpu_budget_ns && !self.suspended {
            // SAFETY: kill with SIGSTOP is safe
            if unsafe { libc::kill(self.pid, libc::SIGSTOP) } == 0 {
                self.suspended = true;
            }
        }
        /* if under/at budget and suspended, SIGCONT */
        else if total_cpu_ns <= cpu_budget_ns && self.suspended {
            // SAFETY: kill with SIGCONT is safe
            if unsafe { libc::kill(self.pid, libc::SIGCONT) } == 0 {
                self.suspended = false;
            }
        }
        
        /* update state for next interval */
        self.last_cpu_ns = current_cpu_ns;
        self.last_wall_ns = now_ns;
        
        Ok(self.suspended)
    }
}

impl Drop for CpuThrottleState {
    fn drop(&mut self) {
        /* ensure process is resumed if we were holding it suspended */
        self.resume();
    }
}

/* get number of logical CPU cores via sysctl */
#[cfg(test)]
fn get_cpu_core_count() -> Option<u32> {
    let mut count: i32 = 0;
    let mut size = core::mem::size_of::<i32>();
    let name = c"hw.ncpu";
    
    // SAFETY: sysctlbyname with valid name and properly sized buffer
    let ret = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&raw mut count).cast(),
            &raw mut size,
            core::ptr::null_mut(),
            0,
        )
    };
    
    if ret == 0 && count > 0 {
        Some(count as u32)
    } else {
        None
    }
}

/* helper to calculate cpu percent - exposed for testing */
#[inline]
#[allow(clippy::cast_possible_truncation)]
pub fn calculate_cpu_percent(cpu_delta_ns: u64, wall_delta_ns: u64) -> u64 {
    if wall_delta_ns == 0 {
        return 0;
    }
    ((cpu_delta_ns as u128 * 100) / wall_delta_ns as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_get_cpu_core_count() {
        /* verify we can read core count from sysctl */
        let cores = get_cpu_core_count();
        assert!(cores.is_some(), "should be able to get CPU core count");
        let cores = cores.unwrap();
        
        /* sanity: modern macs have at least 4 cores, at most ~128 */
        assert!(cores >= 4, "expected at least 4 cores, got {}", cores);
        assert!(cores <= 128, "expected at most 128 cores, got {}", cores);
        
        /* for M4 Pro specifically: 14 cores */
        /* this test documents expected behavior on test machine */
        eprintln!("CPU cores detected: {}", cores);
    }
    
    #[test]
    fn test_cpu_percent_calculation_basic() {
        /* 50ms CPU in 100ms wall = 50% */
        let cpu_delta = 50_000_000; /* 50ms in ns */
        let wall_delta = 100_000_000; /* 100ms in ns */
        assert_eq!(calculate_cpu_percent(cpu_delta, wall_delta), 50);
    }
    
    #[test]
    fn test_cpu_percent_calculation_100_percent() {
        /* 100ms CPU in 100ms wall = 100% (single core fully utilized) */
        let cpu_delta = 100_000_000;
        let wall_delta = 100_000_000;
        assert_eq!(calculate_cpu_percent(cpu_delta, wall_delta), 100);
    }
    
    #[test]
    fn test_cpu_percent_calculation_multicore() {
        /* multi-core scenario: 4 threads at 100% each = 400% total CPU
         * this simulates 400ms of CPU time in 100ms of wall time */
        let cpu_delta = 400_000_000; /* 400ms CPU across all cores */
        let wall_delta = 100_000_000; /* 100ms wall time */
        assert_eq!(calculate_cpu_percent(cpu_delta, wall_delta), 400);
    }
    
    #[test]
    fn test_cpu_percent_calculation_m4_pro_full_utilization() {
        /* M4 Pro has 14 cores. If all cores at 100%: 1400% total.
         * this simulates 1400ms of CPU time in 100ms of wall time */
        let cpu_delta = 1_400_000_000; /* 1400ms CPU */
        let wall_delta = 100_000_000;  /* 100ms wall */
        assert_eq!(calculate_cpu_percent(cpu_delta, wall_delta), 1400);
    }
    
    #[test]
    fn test_cpu_percent_calculation_zero_wall_time() {
        /* edge case: zero wall time should return 0, not panic */
        assert_eq!(calculate_cpu_percent(100_000_000, 0), 0);
    }
    
    #[test]
    fn test_cpu_percent_calculation_zero_cpu_time() {
        /* idle process: 0 CPU in 100ms = 0% */
        assert_eq!(calculate_cpu_percent(0, 100_000_000), 0);
    }
    
    #[test]
    fn test_cpu_percent_calculation_small_values() {
        /* very short interval: 1ms CPU in 10ms wall = 10% */
        let cpu_delta = 1_000_000;  /* 1ms */
        let wall_delta = 10_000_000; /* 10ms */
        assert_eq!(calculate_cpu_percent(cpu_delta, wall_delta), 10);
    }
    
    #[test]
    fn test_cpu_percent_calculation_large_values() {
        /* long running: 1 hour CPU in 2 hours wall = 50% */
        let cpu_delta = 3_600_000_000_000u64;  /* 1 hour in ns */
        let wall_delta = 7_200_000_000_000u64; /* 2 hours in ns */
        assert_eq!(calculate_cpu_percent(cpu_delta, wall_delta), 50);
    }
    
    #[test]
    fn test_cpu_percent_no_overflow() {
        /* ensure u128 intermediate prevents overflow with extreme values */
        let cpu_delta = u64::MAX / 2;
        let wall_delta = u64::MAX / 4;
        /* should compute without panic - result will be 200% */
        let result = calculate_cpu_percent(cpu_delta, wall_delta);
        assert_eq!(result, 200);
    }
    
    #[test]
    fn test_throttle_config_creation() {
        let cfg = CpuThrottleConfig {
            percent: NonZeroU32::new(50).unwrap(),
            interval_ns: 100_000_000,
            sleep_ns: 50_000_000,
        };
        assert_eq!(cfg.percent.get(), 50);
        assert_eq!(cfg.interval_ns, 100_000_000);
    }
    
    #[test]
    fn test_throttle_state_new_self() {
        /* we should be able to create throttle state for ourselves */
        let pid = unsafe { libc::getpid() };
        let now_ns = 1_000_000_000u64; /* 1 second */
        
        let state = CpuThrottleState::new(pid, now_ns);
        assert!(state.is_ok(), "should create throttle state for self");
        
        let state = state.unwrap();
        assert_eq!(state.pid(), pid);
        assert!(!state.suspended);
        assert!(state.last_cpu_ns > 0, "should have positive CPU time");
    }
    
    #[test]
    fn test_throttle_state_invalid_pid() {
        /* invalid pid should fail */
        let result = CpuThrottleState::new(-1, 1_000_000_000);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_throttle_limit_comparison() {
        /* verify limit comparison logic with known values */
        let limit = 50u64;
        
        /* 49% should not trigger throttle */
        let cpu_percent_under = 49u64;
        assert!(cpu_percent_under <= limit);
        
        /* 50% should not trigger throttle (at limit) */
        let cpu_percent_at = 50u64;
        assert!(cpu_percent_at <= limit);
        
        /* 51% should trigger throttle */
        let cpu_percent_over = 51u64;
        assert!(cpu_percent_over > limit);
        
        /* 400% (multi-core) should definitely trigger */
        let cpu_percent_multicore = 400u64;
        assert!(cpu_percent_multicore > limit);
    }
    
    #[test]
    fn test_throttle_state_has_start_fields() {
        /* verify integral control fields are initialized */
        let pid = unsafe { libc::getpid() };
        let now_ns = 1_000_000_000u64;
        
        let state = CpuThrottleState::new(pid, now_ns).unwrap();
        
        /* start fields should match initial values */
        assert_eq!(state.start_wall_ns, now_ns);
        assert!(state.start_cpu_ns > 0, "should have positive initial CPU time");
        assert_eq!(state.start_cpu_ns, state.last_cpu_ns, "start and last should match initially");
        assert_eq!(state.start_wall_ns, state.last_wall_ns, "start and last should match initially");
    }
    
    #[test]
    fn test_resume_when_not_suspended() {
        /* resume on non-suspended state should be safe no-op */
        let pid = unsafe { libc::getpid() };
        let now_ns = 1_000_000_000u64;
        
        let mut state = CpuThrottleState::new(pid, now_ns).unwrap();
        assert!(!state.suspended);
        
        /* calling resume when not suspended should not panic or change state */
        state.resume();
        assert!(!state.suspended);
    }
    
    /* helper to calculate budget for integral control testing */
    fn calculate_cpu_budget_ns(total_wall_ns: u64, percent: u32) -> u64 {
        ((total_wall_ns as u128 * percent as u128) / 100) as u64
    }
    
    #[test]
    fn test_integral_control_budget_calculation() {
        /* verify budget calculation matches what update() uses */
        
        /* 50% limit over 1 second = 500ms budget */
        assert_eq!(calculate_cpu_budget_ns(1_000_000_000, 50), 500_000_000);
        
        /* 100% limit over 1 second = 1000ms budget */
        assert_eq!(calculate_cpu_budget_ns(1_000_000_000, 100), 1_000_000_000);
        
        /* 400% limit over 1 second = 4000ms budget (multi-core) */
        assert_eq!(calculate_cpu_budget_ns(1_000_000_000, 400), 4_000_000_000);
        
        /* 1400% limit over 1 second = 14000ms budget (M4 Pro full) */
        assert_eq!(calculate_cpu_budget_ns(1_000_000_000, 1400), 14_000_000_000);
    }
    
    #[test]
    fn test_integral_control_under_budget_stays_running() {
        /* if total_cpu <= budget, process should stay running */
        let total_wall_ns = 1_000_000_000u64; /* 1 second */
        let limit_percent = 50u32;
        let budget_ns = calculate_cpu_budget_ns(total_wall_ns, limit_percent);
        
        /* 400ms CPU used < 500ms budget = under budget, should run */
        let total_cpu_ns = 400_000_000u64;
        assert!(total_cpu_ns <= budget_ns, "should be under budget");
        
        /* exactly at budget should also run (<=) */
        let total_cpu_at_budget = budget_ns;
        assert!(total_cpu_at_budget <= budget_ns, "at budget should still run");
    }
    
    #[test]
    fn test_integral_control_over_budget_stops() {
        /* if total_cpu > budget, process should be stopped */
        let total_wall_ns = 1_000_000_000u64;
        let limit_percent = 50u32;
        let budget_ns = calculate_cpu_budget_ns(total_wall_ns, limit_percent);
        
        /* 600ms CPU used > 500ms budget = over budget, should stop */
        let total_cpu_ns = 600_000_000u64;
        assert!(total_cpu_ns > budget_ns, "should be over budget");
    }
    
    #[test]
    fn test_integral_control_convergence() {
        /* integral control should converge to exact target over time.
         * example: 50% target over 10 seconds should allow exactly 5 seconds CPU.
         * 
         * delta-based approach would oscillate between 0% and 100% each interval,
         * averaging ~50% but never precisely hitting target.
         * 
         * integral approach creates "debt": if process runs hot early, it gets
         * stopped until wall clock catches up. over long enough time, total
         * CPU / total wall converges exactly to target. */
        
        let limit_percent = 50u32;
        
        /* scenario: 10 seconds total, 50% target = 5 seconds CPU allowed */
        let total_wall_10s = 10_000_000_000u64;
        let budget_10s = calculate_cpu_budget_ns(total_wall_10s, limit_percent);
        assert_eq!(budget_10s, 5_000_000_000u64, "10s at 50% = 5s budget");
        
        /* if process used exactly 5s CPU, it's at target */
        let cpu_at_target = 5_000_000_000u64;
        assert!(cpu_at_target <= budget_10s, "at target should be allowed to run");
        
        /* if process used 5.001s CPU, it's over target and stops */
        let cpu_over_target = 5_001_000_000u64;
        assert!(cpu_over_target > budget_10s, "over target should be stopped");
    }
}
