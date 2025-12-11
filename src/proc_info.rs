/*
 * proc_info.rs
 *
 * darwin libproc API for getting process resource usage without entitlements.
 * uses proc_pid_rusage() - no special permissions needed, works on all macOS.
 *
 * IMPORTANT: rusage_info structs are unstable across macOS versions.
 * the kernel may write more bytes than older struct definitions expect.
 * we use an oversized buffer to prevent stack corruption.
 *
 * macOS SDK rusage_info_v4 has 36 uint64_t fields after uuid = 304 bytes total.
 * we allocate 512 bytes for future-proofing against v5/v6 additions.
 */

use crate::error::{Result, TimeoutError};
use alloc::format;

const RUSAGE_BUFFER_SIZE: usize = 512;
const RUSAGE_INFO_V4: i32 = 4;

/* field offsets in rusage_info_v4 - verified against macOS SDK.
 * calculation: 16 (uuid) + N * 8 (u64 fields)
 *
 * offset 0:  ri_uuid[16]           = 16 bytes
 * offset 16: ri_user_time          = 16 + 0*8
 * offset 24: ri_system_time        = 16 + 1*8
 * offset 32: ri_pkg_idle_wkups     = 16 + 2*8
 * offset 40: ri_interrupt_wkups    = 16 + 3*8
 * offset 48: ri_pageins            = 16 + 4*8
 * offset 56: ri_wired_size         = 16 + 5*8
 * offset 64: ri_resident_size      = 16 + 6*8
 * offset 72: ri_phys_footprint     = 16 + 7*8  <-- memory metric
 */
const OFFSET_USER_TIME: usize = 16; /* 16 + 0*8 */
const OFFSET_SYSTEM_TIME: usize = 24; /* 16 + 1*8 */
const OFFSET_PHYS_FOOTPRINT: usize = 72; /* 16 + 7*8 */

/* force alignment to 8 bytes to match uint64_t alignment requirements.
 * although [u8; N] has alignment 1, the kernel treats the pointer as a struct
 * containing u64s. misaligned writes may cause SIGBUS on strict ARM64 configs. */
#[repr(C, align(8))]
struct AlignedBuffer([u8; RUSAGE_BUFFER_SIZE]);

unsafe extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut u8) -> i32;
}

/* read u64 from buffer at offset (little-endian on arm64/x86_64) */
#[inline]
fn read_u64(buf: &[u8; RUSAGE_BUFFER_SIZE], offset: usize) -> u64 {
    let bytes: [u8; 8] = buf[offset..offset + 8].try_into().unwrap_or([0; 8]);
    u64::from_ne_bytes(bytes)
}

/* call proc_pid_rusage into oversized buffer, return raw buffer on success */
#[inline(never)]
fn get_rusage_raw(pid: i32) -> Option<[u8; RUSAGE_BUFFER_SIZE]> {
    /* use aligned wrapper - kernel expects u64-aligned buffer */
    let mut aligned = AlignedBuffer([0u8; RUSAGE_BUFFER_SIZE]);

    // SAFETY: proc_pid_rusage writes to buffer. we provide 512 bytes which
    // is more than any known rusage_info version needs (~304 bytes for v4).
    // this guards against kernel writing extra bytes in newer macOS versions.
    // buffer is 8-byte aligned via AlignedBuffer wrapper.
    let ret = unsafe { proc_pid_rusage(pid, RUSAGE_INFO_V4, aligned.0.as_mut_ptr()) };

    if ret < 0 {
        return None;
    }

    Some(aligned.0)
}

/* get process memory usage (phys_footprint) in bytes */
pub fn get_process_memory(pid: i32) -> Option<u64> {
    let buf = get_rusage_raw(pid)?;
    Some(read_u64(&buf, OFFSET_PHYS_FOOTPRINT))
}

/* get CPU time in nanoseconds (user + system) */
pub fn get_process_cpu_time(pid: i32) -> Option<u64> {
    let buf = get_rusage_raw(pid)?;
    let user = read_u64(&buf, OFFSET_USER_TIME);
    let system = read_u64(&buf, OFFSET_SYSTEM_TIME);
    Some(user.saturating_add(system))
}

/* get both memory and CPU time in one call for efficiency */
pub fn get_process_stats(pid: i32) -> Result<ProcessStats> {
    let buf = get_rusage_raw(pid)
        .ok_or_else(|| TimeoutError::Internal(format!("proc_pid_rusage failed for pid {}", pid)))?;

    Ok(ProcessStats {
        memory_bytes: read_u64(&buf, OFFSET_PHYS_FOOTPRINT),
        cpu_time_ns: read_u64(&buf, OFFSET_USER_TIME)
            .saturating_add(read_u64(&buf, OFFSET_SYSTEM_TIME)),
    })
}

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, Copy)]
pub struct ProcessStats {
    pub memory_bytes: u64,
    pub cpu_time_ns: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_get_process_memory_self() {
        /* we should be able to get our own memory usage */
        // SAFETY: getpid() always succeeds
        let pid = unsafe { libc::getpid() };
        let mem = get_process_memory(pid);
        assert!(mem.is_some(), "should get memory for self");
        /* sanity: process should use at least 1MB */
        assert!(mem.unwrap() > 1_000_000, "memory should be > 1MB");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_get_process_cpu_time_self() {
        /* we should be able to get our own cpu time */
        // SAFETY: getpid() always succeeds
        let pid = unsafe { libc::getpid() };
        let cpu = get_process_cpu_time(pid);
        assert!(cpu.is_some(), "should get cpu time for self");
        /* cpu time should be positive (we've been running) */
        assert!(cpu.unwrap() > 0, "cpu time should be > 0");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_get_process_stats_self() {
        // SAFETY: getpid() always succeeds
        let pid = unsafe { libc::getpid() };
        let stats = get_process_stats(pid);
        assert!(stats.is_ok(), "should get stats for self");
        let stats = stats.unwrap();
        assert!(stats.memory_bytes > 1_000_000);
        assert!(stats.cpu_time_ns > 0);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_invalid_pid() {
        /* pid -1 should fail */
        assert!(get_process_memory(-1).is_none());
        assert!(get_process_cpu_time(-1).is_none());
        assert!(get_process_stats(-1).is_err());
    }
}
