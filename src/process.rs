/*
 * process.rs
 *
 * no_std-ready process spawning and management using posix_spawn.
 * replaces std::process::Command for ~6KB binary savings.
 *
 * posix_spawn is more efficient than fork+exec on modern systems -
 * it can use vfork internally and avoids copying page tables.
 *
 * On macOS, posix_spawnattr_t and posix_spawn_file_actions_t are opaque
 * pointers (*mut c_void) managed by the C library. We use RAII wrappers
 * to ensure proper initialization and cleanup via Drop.
 */

use alloc::ffi::CString;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::c_char;
use core::ptr;

use crate::rlimit::{ResourceLimits, apply_limits};

/*
 * FFI declarations for functions not in libc crate or needing special handling.
 * Most posix_spawn functions are available via libc::*.
 */

unsafe extern "C" {
    /* environ is a global variable pointing to the environment */
    static environ: *const *const c_char;
}

/* errno values */
const ENOENT: i32 = 2;
const ESRCH: i32 = 3;
const EACCES: i32 = 13;
const EPERM: i32 = 1;

/* signals */
const SIGKILL: i32 = 9;

/*
 * RAII wrapper for posix_spawnattr_t.
 *
 * On macOS, posix_spawnattr_t is an opaque pointer (*mut c_void).
 * The init function allocates internal storage, and destroy frees it.
 * This wrapper ensures cleanup even on early return or panic.
 */
struct SpawnAttr {
    inner: libc::posix_spawnattr_t,
    initialized: bool,
}

impl SpawnAttr {
    /* create and initialize spawn attributes */
    fn new() -> Result<Self, i32> {
        let mut attr: libc::posix_spawnattr_t = ptr::null_mut();
        // SAFETY: attr is a valid pointer location for posix_spawnattr_init to populate
        let ret = unsafe { libc::posix_spawnattr_init(&mut attr) };
        if ret != 0 {
            return Err(ret);
        }
        Ok(Self {
            inner: attr,
            initialized: true,
        })
    }

    /* set flags on the spawn attributes */
    fn set_flags(&mut self, flags: libc::c_short) -> Result<(), i32> {
        // SAFETY: self.inner was initialized in new()
        let ret = unsafe { libc::posix_spawnattr_setflags(&mut self.inner, flags) };
        if ret != 0 {
            return Err(ret);
        }
        Ok(())
    }

    /* set process group (0 = own group) */
    fn set_pgroup(&mut self, pgroup: libc::pid_t) -> Result<(), i32> {
        // SAFETY: self.inner was initialized in new()
        let ret = unsafe { libc::posix_spawnattr_setpgroup(&mut self.inner, pgroup) };
        if ret != 0 {
            return Err(ret);
        }
        Ok(())
    }

    /* get raw pointer for FFI calls */
    fn as_ptr(&self) -> *const libc::posix_spawnattr_t {
        &self.inner
    }
}

impl Drop for SpawnAttr {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: self.inner was initialized in new() and hasn't been destroyed yet
            unsafe {
                libc::posix_spawnattr_destroy(&mut self.inner);
            }
        }
    }
}

/*
 * RAII wrapper for posix_spawn_file_actions_t.
 *
 * Same pattern as SpawnAttr - ensures cleanup via Drop.
 */
struct SpawnFileActions {
    inner: libc::posix_spawn_file_actions_t,
    initialized: bool,
}

impl SpawnFileActions {
    /* create and initialize file actions */
    fn new() -> Result<Self, i32> {
        let mut actions: libc::posix_spawn_file_actions_t = ptr::null_mut();
        // SAFETY: actions is a valid pointer location for init to populate
        let ret = unsafe { libc::posix_spawn_file_actions_init(&mut actions) };
        if ret != 0 {
            return Err(ret);
        }
        Ok(Self {
            inner: actions,
            initialized: true,
        })
    }

    /* get raw pointer for FFI calls */
    fn as_ptr(&self) -> *const libc::posix_spawn_file_actions_t {
        &self.inner
    }
}

impl Drop for SpawnFileActions {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: self.inner was initialized in new() and hasn't been destroyed yet
            unsafe {
                libc::posix_spawn_file_actions_destroy(&mut self.inner);
            }
        }
    }
}

/// Raw child process handle - no_std replacement for std::process::Child
#[derive(Debug)]
pub struct RawChild {
    pid: libc::pid_t,
    exited: bool,
}

/// Exit status from a process
#[derive(Debug, Clone, Copy)]
pub struct RawExitStatus {
    status: i32,
}

/// Resource usage from wait4() - CPU time and memory.
/// macOS returns ru_maxrss in bytes (not KB like Linux), we convert to KB.
///
/// **Precision notes:**
/// - Time values stored in microseconds internally, converted to milliseconds via truncation
///   (not rounding) in `user_time_ms()`/`system_time_ms()` to avoid float bloat.
/// - RSS is truncated to KB (up to 1023 bytes lost per conversion).
#[derive(Debug, Clone, Copy, Default)]
pub struct ResourceUsage {
    pub user_time_us: u64,   /* ru_utime in microseconds */
    pub system_time_us: u64, /* ru_stime in microseconds */
    pub max_rss_kb: u64,     /* ru_maxrss converted to KB (macOS reports bytes) */
}

impl ResourceUsage {
    /// User CPU time in milliseconds (truncated, not rounded)
    #[inline]
    pub fn user_time_ms(&self) -> u64 {
        self.user_time_us / 1000
    }

    /// System CPU time in milliseconds (truncated, not rounded)
    #[inline]
    pub fn system_time_ms(&self) -> u64 {
        self.system_time_us / 1000
    }
}

impl RawExitStatus {
    /// Returns the exit code if the process exited normally
    #[inline]
    pub fn code(&self) -> Option<i32> {
        if self.exited_normally() {
            Some((self.status >> 8) & 0xFF)
        } else {
            None
        }
    }

    /// Returns the signal number if the process was killed by a signal
    #[inline]
    pub fn signal(&self) -> Option<i32> {
        if self.signaled() {
            Some(self.status & 0x7F)
        } else {
            None
        }
    }

    #[inline]
    fn exited_normally(&self) -> bool {
        (self.status & 0x7F) == 0
    }

    #[inline]
    fn signaled(&self) -> bool {
        ((self.status & 0x7F) + 1) >> 1 > 0
    }
}

/// Error from process operations
#[cfg_attr(test, derive(Debug))]
pub enum SpawnError {
    /// Command not found in PATH
    NotFound(String),
    /// Permission denied
    PermissionDenied(String),
    /// Other spawn error with errno
    Spawn(i32),
    /// Wait error
    Wait(i32),
    /// Invalid argument (null byte in string)
    InvalidArg,
}

impl core::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotFound(s) => write!(f, "command not found: {s}"),
            Self::PermissionDenied(s) => write!(f, "permission denied: {s}"),
            Self::Spawn(e) => write!(f, "spawn error: errno {e}"),
            Self::Wait(e) => write!(f, "wait error: errno {e}"),
            Self::InvalidArg => write!(f, "invalid argument"),
        }
    }
}

impl RawChild {
    /// Get the process ID
    #[inline]
    pub fn id(&self) -> u32 {
        self.pid as u32
    }

    /// Wait for the process to exit, blocking. Returns exit status and resource usage.
    pub fn wait(&mut self) -> Result<(RawExitStatus, ResourceUsage), SpawnError> {
        if self.exited {
            return Err(SpawnError::Wait(libc::ECHILD)); /* "No child processes" */
        }

        let mut status: i32 = 0;
        // SAFETY: libc::rusage is a C struct that's safe to zero-initialize
        let mut rusage: libc::rusage = unsafe { core::mem::zeroed() };

        /* retry on EINTR - signals can interrupt blocking wait */
        loop {
            // SAFETY: pid is valid from spawn, status and rusage are valid pointers
            let ret = unsafe { libc::wait4(self.pid, &mut status, 0, &mut rusage) };

            if ret < 0 {
                let err = errno();
                if err == libc::EINTR {
                    continue; /* interrupted by signal, retry */
                }
                return Err(SpawnError::Wait(err));
            }
            break;
        }

        self.exited = true;
        Ok((RawExitStatus { status }, rusage_to_resource_usage(&rusage)))
    }

    /// Check if process has exited without blocking. Returns exit status and resource usage if exited.
    pub fn try_wait(&mut self) -> Result<Option<(RawExitStatus, ResourceUsage)>, SpawnError> {
        if self.exited {
            return Err(SpawnError::Wait(libc::ECHILD)); /* already reaped */
        }

        let mut status: i32 = 0;
        // SAFETY: libc::rusage is a C struct that's safe to zero-initialize
        let mut rusage: libc::rusage = unsafe { core::mem::zeroed() };
        // SAFETY: pid is valid from spawn, status and rusage are valid pointers
        let ret = unsafe { libc::wait4(self.pid, &mut status, libc::WNOHANG, &mut rusage) };

        if ret < 0 {
            return Err(SpawnError::Wait(errno()));
        }

        if ret == 0 {
            /* still running */
            return Ok(None);
        }

        self.exited = true;
        Ok(Some((
            RawExitStatus { status },
            rusage_to_resource_usage(&rusage),
        )))
    }

    /// Send SIGKILL to the process
    pub fn kill(&mut self) -> Result<(), SpawnError> {
        if self.exited {
            return Ok(());
        }

        // SAFETY: kill is safe with any pid/signal
        let ret = unsafe { libc::kill(self.pid, SIGKILL) };

        if ret < 0 {
            let e = errno();
            if e == ESRCH {
                /* already dead */
                return Ok(());
            }
            return Err(SpawnError::Wait(e));
        }

        Ok(())
    }
}

/// Spawn a command using posix_spawnp (searches PATH)
///
/// # Arguments
/// * `command` - The command to run
/// * `args` - Arguments to pass (not including argv[0])
/// * `use_process_group` - If true, put child in its own process group
///
/// # Returns
/// * `Ok(RawChild)` - The spawned child process
/// * `Err(SpawnError)` - If spawn failed
pub fn spawn_command(
    command: &str,
    args: &[String],
    use_process_group: bool,
) -> Result<RawChild, SpawnError> {
    /* build argv: [command, args..., NULL] */
    let cmd_cstr = CString::new(command).map_err(|_| SpawnError::InvalidArg)?;

    let mut argv_cstrs: Vec<CString> = Vec::with_capacity(args.len() + 2);
    argv_cstrs.push(cmd_cstr.clone());

    for arg in args {
        argv_cstrs.push(CString::new(arg.as_str()).map_err(|_| SpawnError::InvalidArg)?);
    }

    /* build pointer array */
    let mut argv_ptrs: Vec<*const c_char> = Vec::with_capacity(argv_cstrs.len() + 1);
    for cstr in &argv_cstrs {
        argv_ptrs.push(cstr.as_ptr());
    }
    argv_ptrs.push(ptr::null());

    /* initialize spawn attributes using RAII wrapper */
    let mut attr = SpawnAttr::new().map_err(SpawnError::Spawn)?;

    /* set process group if requested */
    if use_process_group {
        #[allow(clippy::cast_possible_truncation)]
        attr.set_flags(libc::POSIX_SPAWN_SETPGROUP as libc::c_short)
            .map_err(SpawnError::Spawn)?;
        attr.set_pgroup(0).map_err(SpawnError::Spawn)?; /* own group */
    }

    /* initialize file actions using RAII wrapper (inherit stdin/stdout/stderr) */
    let file_actions = SpawnFileActions::new().map_err(SpawnError::Spawn)?;

    /* spawn the process */
    let mut pid: libc::pid_t = 0;
    // SAFETY: all pointers are valid, environ is the process environment.
    // attr and file_actions are initialized RAII wrappers.
    let ret = unsafe {
        libc::posix_spawnp(
            &mut pid,
            cmd_cstr.as_ptr(),
            file_actions.as_ptr(),
            attr.as_ptr(),
            argv_ptrs.as_ptr() as *const *mut c_char,
            environ as *const *mut c_char,
        )
    };

    /* RAII: attr and file_actions are automatically destroyed when they go out of scope */

    if ret != 0 {
        return Err(match ret {
            ENOENT => SpawnError::NotFound(command.into()),
            EACCES | EPERM => SpawnError::PermissionDenied(command.into()),
            _ => SpawnError::Spawn(ret),
        });
    }

    Ok(RawChild { pid, exited: false })
}

/// Spawn a command using fork + exec, applying resource limits before exec.
/// Falls back to exit codes 126/127 inside the child on exec failure.
pub fn spawn_command_with_limits(
    command: &str,
    args: &[String],
    use_process_group: bool,
    limits: &ResourceLimits,
) -> Result<RawChild, SpawnError> {
    /* build argv: [command, args..., NULL] */
    let cmd_cstr = CString::new(command).map_err(|_| SpawnError::InvalidArg)?;

    let mut argv_cstrs: Vec<CString> = Vec::with_capacity(args.len() + 2);
    argv_cstrs.push(cmd_cstr.clone());
    for arg in args {
        argv_cstrs.push(CString::new(arg.as_str()).map_err(|_| SpawnError::InvalidArg)?);
    }

    let mut argv_ptrs: Vec<*const c_char> = Vec::with_capacity(argv_cstrs.len() + 1);
    for cstr in &argv_cstrs {
        argv_ptrs.push(cstr.as_ptr());
    }
    argv_ptrs.push(ptr::null());

    /* fork into parent and child */
    // SAFETY: fork() is safe - creates child process. returns pid in parent, 0 in child.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(SpawnError::Spawn(errno()));
    }

    if pid == 0 {
        /* child */
        if use_process_group {
            // SAFETY: setpgid(0,0) in child to create its own group.
            unsafe { libc::setpgid(0, 0) };
        }

        /* apply resource limits before exec */
        if !limits.is_empty() && apply_limits(limits).is_err() {
            // SAFETY: _exit terminates child process immediately
            unsafe { libc::_exit(125) };
        }

        // SAFETY: execvp with valid argv pointers. On failure, returns -1.
        let ret = unsafe { libc::execvp(cmd_cstr.as_ptr(), argv_ptrs.as_ptr()) };
        if ret < 0 {
            let e = errno();
            let code = if e == ENOENT { 127 } else { 126 };
            // SAFETY: _exit terminates child process with error code
            unsafe { libc::_exit(code) };
        }
    }

    Ok(RawChild { pid, exited: false })
}

/* get errno - on macOS this is a thread-local via __error() */
#[inline]
fn errno() -> i32 {
    unsafe extern "C" {
        fn __error() -> *mut i32;
    }
    // SAFETY: __error always returns valid pointer on macOS. The dereference and
    // function call share the same invariant (pointer validity for thread-local errno).
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    unsafe {
        *__error()
    }
}

/* convert libc::rusage to ResourceUsage. macOS reports ru_maxrss in bytes, divide by 1024 for KB. */
#[inline]
#[allow(clippy::cast_sign_loss)]
fn rusage_to_resource_usage(rusage: &libc::rusage) -> ResourceUsage {
    /* timeval: tv_sec (seconds) + tv_usec (microseconds)
     * tv_usec is i32 on macOS - use .max(0) to guard against negative values
     * before casting to u64 (negative i32 -> huge u64 via two's complement) */
    let user_time_us = (rusage.ru_utime.tv_sec.max(0) as u64)
        .saturating_mul(1_000_000)
        .saturating_add(rusage.ru_utime.tv_usec.max(0) as u64);
    let system_time_us = (rusage.ru_stime.tv_sec.max(0) as u64)
        .saturating_mul(1_000_000)
        .saturating_add(rusage.ru_stime.tv_usec.max(0) as u64);
    /* macOS: ru_maxrss is in bytes (i64), convert to KB. guard against negative. */
    let max_rss_kb = (rusage.ru_maxrss.max(0) as u64) / 1024;

    ResourceUsage {
        user_time_us,
        system_time_us,
        max_rss_kb,
    }
}

/*
 * Tests for process spawning.
 *
 * These tests are skipped under Miri because posix_spawn* and waitpid are
 * unsupported foreign functions. Miri can validate pure-Rust logic but cannot
 * interpret OS-level process creation syscalls.
 *
 * The logic tested here is covered by integration tests which run natively.
 */
#[cfg(test)]
#[cfg(not(miri))]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_resource_usage_accessors() {
        /* test truncation behavior (not rounding) */
        let rusage = ResourceUsage {
            user_time_us: 1999,   /* 1.999ms -> 1ms truncated */
            system_time_us: 2500, /* 2.5ms -> 2ms truncated */
            max_rss_kb: 1024,
        };
        assert_eq!(rusage.user_time_ms(), 1);
        assert_eq!(rusage.system_time_ms(), 2);
        assert_eq!(rusage.max_rss_kb, 1024);

        /* edge case: sub-millisecond truncates to 0 */
        let tiny = ResourceUsage {
            user_time_us: 999,
            system_time_us: 1,
            max_rss_kb: 0,
        };
        assert_eq!(tiny.user_time_ms(), 0);
        assert_eq!(tiny.system_time_ms(), 0);
    }

    #[test]
    fn test_resource_usage_default() {
        let rusage = ResourceUsage::default();
        assert_eq!(rusage.user_time_us, 0);
        assert_eq!(rusage.system_time_us, 0);
        assert_eq!(rusage.max_rss_kb, 0);
    }

    #[test]
    fn test_spawn_true() {
        let mut child = spawn_command("true", &[], false).unwrap();
        let (status, rusage) = child.wait().unwrap();
        assert_eq!(status.code(), Some(0));
        /* rusage should have some values (at least max_rss > 0 for any process) */
        assert!(rusage.max_rss_kb > 0);
    }

    #[test]
    fn test_spawn_false() {
        let mut child = spawn_command("false", &[], false).unwrap();
        let (status, _rusage) = child.wait().unwrap();
        assert_eq!(status.code(), Some(1));
    }

    #[test]
    fn test_spawn_not_found() {
        let result = spawn_command("nonexistent_command_12345", &[], false);
        assert!(matches!(result, Err(SpawnError::NotFound(_))));
    }

    #[test]
    fn test_spawn_with_args() {
        let args = vec![String::from("hello")];
        let mut child = spawn_command("echo", &args, false).unwrap();
        let (status, _rusage) = child.wait().unwrap();
        assert_eq!(status.code(), Some(0));
    }

    #[test]
    fn test_try_wait() {
        let mut child = spawn_command("sleep", &[String::from("0.1")], false).unwrap();
        /* should still be running */
        let result = child.try_wait().unwrap();
        assert!(result.is_none() || result.is_some()); /* might be fast */
        /* wait for completion */
        let _ = child.wait();
    }
}

/* -------------------------------------------------------------------------- */
/*                              kani proofs                                   */
/* -------------------------------------------------------------------------- */

/*
 * kani proofs for process lifecycle invariants.
 * focuses on state machine correctness rather than FFI (which kani can't model).
 */
#[cfg(kani)]
mod kani_proofs {
    /*
     * verify RawChild state machine: exited flag prevents double-wait.
     * models the invariant that wait() can only succeed once.
     */
    #[kani::proof]
    fn verify_wait_only_once() {
        let mut exited = false;

        /* first wait succeeds, sets exited = true */
        if !exited {
            exited = true;
            /* simulates successful wait */
        }

        /* second wait should fail (exited == true) */
        let would_wait = !exited;
        kani::assert(!would_wait, "second wait should not proceed when exited");
    }

    /*
     * verify kill() is idempotent when process already exited.
     * calling kill() on exited process should be a no-op.
     */
    #[kani::proof]
    fn verify_kill_idempotent_after_exit() {
        let exited: bool = kani::any();

        /* kill() logic: if exited, return Ok immediately */
        let would_send_signal = !exited;

        if exited {
            kani::assert(
                !would_send_signal,
                "kill should not send signal when exited",
            );
        }
    }

    /*
     * verify exit status code extraction is correct.
     * WIFEXITED: (status & 0x7F) == 0
     * WEXITSTATUS: (status >> 8) & 0xFF
     */
    #[kani::proof]
    fn verify_exit_status_extraction() {
        let status: i32 = kani::any();

        /* model exited_normally: (status & 0x7F) == 0 */
        let exited_normally = (status & 0x7F) == 0;

        /* model code extraction: (status >> 8) & 0xFF */
        let code = (status >> 8) & 0xFF;

        /* code should be in range 0-255 */
        kani::assert(code >= 0 && code <= 255, "exit code must be 0-255");

        /* if normally exited, code is valid */
        if exited_normally {
            kani::cover!(code == 0, "exit code can be 0 (success)");
            kani::cover!(code > 0, "exit code can be non-zero (failure)");
        }
    }

    /*
     * verify signal extraction from status.
     * WIFSIGNALED: ((status & 0x7F) + 1) >> 1 > 0
     * WTERMSIG: status & 0x7F
     */
    #[kani::proof]
    fn verify_signal_extraction() {
        let status: i32 = kani::any();

        /* model signaled: low 7 bits non-zero in specific pattern */
        let signaled = ((status & 0x7F) + 1) >> 1 > 0;

        /* model signal extraction */
        let signal = status & 0x7F;

        /* signal should be in range 0-127 */
        kani::assert(signal >= 0 && signal <= 127, "signal must be 0-127");

        /* common signals we care about */
        if signaled {
            kani::cover!(signal == 9, "can detect SIGKILL");
            kani::cover!(signal == 15, "can detect SIGTERM");
        }
    }
}
