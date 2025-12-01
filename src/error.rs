/*
 * error.rs
 *
 * Exit codes match GNU coreutils. Scripts depend on these.
 * 124 = timed out, 125 = our fault, 126 = not executable, 127 = not found
 *
 * Don't change them. You'll break CI pipelines.
 */

use std::fmt;
use std::process::ExitCode;

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
    SpawnError(std::io::Error),
    SignalError(i32), // errno from libc signal calls
    ProcessGroupError(String),
    Internal(String),
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
            Self::SpawnError(e) => write!(f, "failed to spawn process: {e}"),
            Self::SignalError(errno) => {
                write!(
                    f,
                    "signal error: {}",
                    std::io::Error::from_raw_os_error(*errno)
                )
            }
            Self::ProcessGroupError(s) => write!(f, "process group error: {s}"),
            Self::Internal(s) => write!(f, "internal error: {s}"),
        }
    }
}

impl std::error::Error for TimeoutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SpawnError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TimeoutError {
    fn from(e: std::io::Error) -> Self {
        Self::SpawnError(e)
    }
}

impl TimeoutError {
    /* map errors to exit codes. 126 vs 127 matters to scripts. */
    #[must_use]
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::CommandNotFound(_) => ExitCode::from(exit_codes::NOT_FOUND),
            Self::PermissionDenied(_) => ExitCode::from(exit_codes::CANNOT_INVOKE),
            Self::InvalidDuration(_)
            | Self::NegativeDuration
            | Self::DurationOverflow
            | Self::InvalidSignal(_)
            | Self::SpawnError(_)
            | Self::SignalError(_)
            | Self::ProcessGroupError(_)
            | Self::Internal(_) => ExitCode::from(exit_codes::INTERNAL_ERROR),
        }
    }
}

pub type Result<T> = std::result::Result<T, TimeoutError>;
