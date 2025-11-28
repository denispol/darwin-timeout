/*
 * error.rs
 *
 * Exit codes match GNU coreutils. Scripts depend on these.
 * 124 = timed out, 125 = our fault, 126 = not executable, 127 = not found
 *
 * Don't change them. You'll break CI pipelines.
 */

use std::process::ExitCode;
use thiserror::Error;

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
#[derive(Error, Debug)]
pub enum TimeoutError {
    #[error("invalid duration: {0}")]
    InvalidDuration(String),

    #[error("invalid duration: negative values not allowed")]
    NegativeDuration,

    #[error("invalid duration: value too large")]
    DurationOverflow,

    #[error("invalid signal: {0}")]
    InvalidSignal(String),

    #[error("command not found: {0}")]
    CommandNotFound(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("failed to spawn process: {0}")]
    SpawnError(#[from] std::io::Error),

    #[error("signal error: {0}")]
    SignalError(#[from] nix::Error),

    #[error("process group error: {0}")]
    ProcessGroupError(String),

    #[error("internal error: {0}")]
    Internal(String),
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
