/*
 * error.rs
 *
 * Exit codes match GNU coreutils. Scripts depend on these.
 * 124 = timed out, 125 = our fault, 126 = not executable, 127 = not found
 *
 * Don't change them. You'll break CI pipelines.
 */

use alloc::string::String;
use core::fmt;

/// exit codes per GNU coreutils convention. don't change these.
pub mod exit_codes {
    /// Command ran too long (timed out)
    pub const TIMEOUT: u8 = 124;
    /// timeout itself failed (internal error)
    pub const INTERNAL_ERROR: u8 = 125;
    /// Command found but couldn't be executed (permissions)
    pub const CANNOT_INVOKE: u8 = 126;
    /// Command not found
    pub const NOT_FOUND: u8 = 127;
}

/* everything that can go wrong */
#[derive(Debug)]
pub enum TimeoutError {
    InvalidDuration(String),
    NegativeDuration,
    DurationOverflow,
    InvalidSignal(String),
    CommandNotFound(String),
    PermissionDenied(String),
    SpawnError(i32),  // errno from spawn
    SignalError(i32), // errno from libc signal calls
    ProcessGroupError(String),
    Internal(String),
    WaitForFileTimeout(String), // file path that we timed out waiting for
    WaitForFileError(String, i32), // file path + errno from stat
}

impl fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDuration(s) => write!(f, "invalid duration: {s}"),
            Self::NegativeDuration => write!(f, "invalid duration: negative values not allowed"),
            Self::DurationOverflow => write!(f, "invalid duration: value too large"),
            Self::InvalidSignal(s) => write!(f, "invalid signal: {s}"),
            Self::CommandNotFound(s) => write!(f, "command not found: {s}"),
            Self::PermissionDenied(s) => write!(f, "permission denied: {s}"),
            Self::SpawnError(errno) => write!(f, "failed to spawn process: errno {errno}"),
            Self::SignalError(errno) => write!(f, "signal error: errno {errno}"),
            Self::ProcessGroupError(s) => write!(f, "process group error: {s}"),
            Self::Internal(s) => write!(f, "internal error: {s}"),
            Self::WaitForFileTimeout(path) => write!(f, "timed out waiting for file: {path}"),
            Self::WaitForFileError(path, errno) => {
                write!(f, "error checking file '{path}': errno {errno}")
            }
        }
    }
}

impl TimeoutError {
    /* map errors to exit codes. 126 vs 127 matters to scripts. */
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::CommandNotFound(_) => exit_codes::NOT_FOUND,
            Self::PermissionDenied(_) => exit_codes::CANNOT_INVOKE,
            Self::InvalidDuration(_)
            | Self::NegativeDuration
            | Self::DurationOverflow
            | Self::InvalidSignal(_)
            | Self::SpawnError(_)
            | Self::SignalError(_)
            | Self::ProcessGroupError(_)
            | Self::Internal(_)
            | Self::WaitForFileError(_, _) => exit_codes::INTERNAL_ERROR,
            // file-wait timeout uses same code as command timeout (124)
            Self::WaitForFileTimeout(_) => exit_codes::TIMEOUT,
        }
    }
}

pub type Result<T> = core::result::Result<T, TimeoutError>;
