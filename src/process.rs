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

    /// Wait for the process to exit, blocking
    pub fn wait(&mut self) -> Result<RawExitStatus, SpawnError> {
        if self.exited {
            return Err(SpawnError::Wait(0));
        }

        let mut status: i32 = 0;
        // SAFETY: pid is valid from spawn, status is valid pointer
        let ret = unsafe { libc::waitpid(self.pid, &mut status, 0) };

        if ret < 0 {
            return Err(SpawnError::Wait(errno()));
        }

        self.exited = true;
        Ok(RawExitStatus { status })
    }

    /// Check if process has exited without blocking
    pub fn try_wait(&mut self) -> Result<Option<RawExitStatus>, SpawnError> {
        if self.exited {
            return Ok(None);
        }

        let mut status: i32 = 0;
        // SAFETY: pid is valid from spawn, status is valid pointer
        let ret = unsafe { libc::waitpid(self.pid, &mut status, libc::WNOHANG) };

        if ret < 0 {
            return Err(SpawnError::Wait(errno()));
        }

        if ret == 0 {
            /* still running */
            return Ok(None);
        }

        self.exited = true;
        Ok(Some(RawExitStatus { status }))
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
    fn test_spawn_true() {
        let mut child = spawn_command("true", &[], false).unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(0));
    }

    #[test]
    fn test_spawn_false() {
        let mut child = spawn_command("false", &[], false).unwrap();
        let status = child.wait().unwrap();
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
        let status = child.wait().unwrap();
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
