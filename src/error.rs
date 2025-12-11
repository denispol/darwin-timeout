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
    InvalidMemoryLimit(String),
    InvalidCpuTime(String),
    InvalidCpuPercent(String),
    InvalidSignal(String),
    CommandNotFound(String),
    PermissionDenied(String),
    SpawnError(i32),  // errno from spawn
    SignalError(i32), // errno from libc signal calls
    ProcessGroupError(String),
    ResourceLimitError(i32),
    ThrottleAttachError(i32),
    ThrottleControlError(i32),
    Internal(String),
    WaitForFileTimeout(String), // file path that we timed out waiting for
    WaitForFileError(String, i32), // file path + errno from stat
    TimebaseError,              // mach_timebase_info returned invalid data (zero denominator)
}

impl fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDuration(s) => write!(f, "invalid duration: {s}"),
            Self::NegativeDuration => write!(f, "invalid duration: negative values not allowed"),
            Self::DurationOverflow => write!(f, "invalid duration: value too large"),
            Self::InvalidMemoryLimit(s) => write!(f, "invalid memory limit: {s}"),
            Self::InvalidCpuTime(s) => write!(f, "invalid cpu time: {s}"),
            Self::InvalidCpuPercent(s) => write!(f, "invalid cpu percent: {s}"),
            Self::InvalidSignal(s) => write!(f, "invalid signal: {s}"),
            Self::CommandNotFound(s) => write!(f, "command not found: {s}"),
            Self::PermissionDenied(s) => write!(f, "permission denied: {s}"),
            Self::SpawnError(errno) => write!(f, "failed to spawn process: errno {errno}"),
            Self::SignalError(errno) => write!(f, "signal error: errno {errno}"),
            Self::ProcessGroupError(s) => write!(f, "process group error: {s}"),
            Self::ResourceLimitError(errno) => write!(f, "failed to apply resource limits: errno {errno}"),
            Self::ThrottleAttachError(errno) => {
                write!(f, "failed to attach CPU throttle: kern_return {errno}")
            }
            Self::ThrottleControlError(errno) => {
                write!(f, "failed to control CPU throttle: kern_return {errno}")
            }
            Self::Internal(s) => write!(f, "internal error: {s}"),
            Self::WaitForFileTimeout(path) => write!(f, "timed out waiting for file: {path}"),
            Self::WaitForFileError(path, errno) => {
                write!(f, "error checking file '{path}': errno {errno}")
            }
            Self::TimebaseError => {
                write!(f, "invalid mach timebase info (zero denominator)")
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
            | Self::InvalidMemoryLimit(_)
            | Self::InvalidCpuTime(_)
            | Self::InvalidCpuPercent(_)
            | Self::InvalidSignal(_)
            | Self::SpawnError(_)
            | Self::SignalError(_)
            | Self::ProcessGroupError(_)
            | Self::ResourceLimitError(_)
            | Self::ThrottleAttachError(_)
            | Self::ThrottleControlError(_)
            | Self::Internal(_)
            | Self::WaitForFileError(_, _)
            | Self::TimebaseError => exit_codes::INTERNAL_ERROR,
            // file-wait timeout uses same code as command timeout (124)
            Self::WaitForFileTimeout(_) => exit_codes::TIMEOUT,
        }
    }
}

pub type Result<T> = core::result::Result<T, TimeoutError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timebase_error_display() {
        let err = TimeoutError::TimebaseError;
        let msg = alloc::format!("{}", err);
        assert!(
            msg.contains("zero denominator"),
            "error message should mention zero denominator"
        );
    }

    #[test]
    fn test_timebase_error_exit_code() {
        let err = TimeoutError::TimebaseError;
        assert_eq!(
            err.exit_code(),
            exit_codes::INTERNAL_ERROR,
            "timebase error should return internal error exit code"
        );
    }
}
