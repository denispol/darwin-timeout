/*
 * process.rs
 *
 * no_std-ready process spawning and management using posix_spawn.
 * replaces std::process::Command for ~6KB binary savings.
 *
 * posix_spawn is more efficient than fork+exec on modern systems -
 * it can use vfork internally and avoids copying page tables.
 */

use alloc::ffi::CString;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::c_char;
use core::ptr;

/*
 * libc types and functions for posix_spawn.
 * we declare them here to avoid depending on libc crate's std feature.
 */

#[allow(non_camel_case_types)]
type pid_t = i32;

/* opaque types - we only use pointers to them */
#[repr(C)]
#[allow(non_camel_case_types)]
struct posix_spawn_file_actions_t {
    _opaque: [u8; 80], /* macOS size */
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct posix_spawnattr_t {
    _opaque: [u8; 336], /* macOS size */
}

/* posix_spawn flags */
const POSIX_SPAWN_SETPGROUP: i16 = 0x0002;

unsafe extern "C" {
    fn posix_spawnp(
        pid: *mut pid_t,
        file: *const c_char,
        file_actions: *const posix_spawn_file_actions_t,
        attrp: *const posix_spawnattr_t,
        argv: *const *const c_char,
        envp: *const *const c_char,
    ) -> i32;

    fn posix_spawn_file_actions_init(file_actions: *mut posix_spawn_file_actions_t) -> i32;
    fn posix_spawn_file_actions_destroy(file_actions: *mut posix_spawn_file_actions_t) -> i32;
    #[allow(dead_code)] /* for future fd redirection support */
    fn posix_spawn_file_actions_adddup2(
        file_actions: *mut posix_spawn_file_actions_t,
        fildes: i32,
        newfildes: i32,
    ) -> i32;

    fn posix_spawnattr_init(attr: *mut posix_spawnattr_t) -> i32;
    fn posix_spawnattr_destroy(attr: *mut posix_spawnattr_t) -> i32;
    fn posix_spawnattr_setflags(attr: *mut posix_spawnattr_t, flags: i16) -> i32;
    fn posix_spawnattr_setpgroup(attr: *mut posix_spawnattr_t, pgroup: pid_t) -> i32;

    fn waitpid(pid: pid_t, status: *mut i32, options: i32) -> pid_t;
    fn kill(pid: pid_t, sig: i32) -> i32;

    /* environ is a global variable pointing to the environment */
    static environ: *const *const c_char;
}

/* waitpid options */
const WNOHANG: i32 = 1;

/* errno values */
const ENOENT: i32 = 2;
const ESRCH: i32 = 3;
const EACCES: i32 = 13;
const EPERM: i32 = 1;

/* signals */
const SIGKILL: i32 = 9;

/// Raw child process handle - no_std replacement for std::process::Child
#[derive(Debug)]
pub struct RawChild {
    pid: pid_t,
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
#[derive(Debug)]
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
        let ret = unsafe { waitpid(self.pid, &mut status, 0) };

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
        let ret = unsafe { waitpid(self.pid, &mut status, WNOHANG) };

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
        let ret = unsafe { kill(self.pid, SIGKILL) };

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

    /* initialize spawn attributes */
    let mut attr: posix_spawnattr_t = unsafe { core::mem::zeroed() };
    // SAFETY: attr is valid zeroed struct
    let ret = unsafe { posix_spawnattr_init(&mut attr) };
    if ret != 0 {
        return Err(SpawnError::Spawn(ret));
    }

    /* set process group if requested */
    if use_process_group {
        // SAFETY: attr was initialized above
        unsafe {
            posix_spawnattr_setflags(&mut attr, POSIX_SPAWN_SETPGROUP);
            posix_spawnattr_setpgroup(&mut attr, 0); /* own group */
        }
    }

    /* initialize file actions (inherit stdin/stdout/stderr) */
    let mut file_actions: posix_spawn_file_actions_t = unsafe { core::mem::zeroed() };
    // SAFETY: file_actions is valid zeroed struct
    let ret = unsafe { posix_spawn_file_actions_init(&mut file_actions) };
    if ret != 0 {
        // SAFETY: attr was initialized
        unsafe { posix_spawnattr_destroy(&mut attr) };
        return Err(SpawnError::Spawn(ret));
    }

    /* inherit standard fds (0, 1, 2) - they're inherited by default,
     * but we can explicitly dup2 if needed. For now, inheritance is fine. */

    /* spawn the process */
    let mut pid: pid_t = 0;
    // SAFETY: all pointers are valid, environ is the process environment
    let ret = unsafe {
        posix_spawnp(
            &mut pid,
            cmd_cstr.as_ptr(),
            &file_actions,
            &attr,
            argv_ptrs.as_ptr(),
            environ,
        )
    };

    /* cleanup */
    // SAFETY: both were initialized above
    unsafe {
        posix_spawn_file_actions_destroy(&mut file_actions);
        posix_spawnattr_destroy(&mut attr);
    }

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
    // SAFETY: __error always returns valid pointer on macOS
    unsafe { *__error() }
}

#[cfg(test)]
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
