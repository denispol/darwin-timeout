/*
 * lib.rs
 *
 * Exists mostly for testing. Integration tests need our types, doc tests
 * need a lib. You could use this as a library but honestly just shell out.
 *
 * no_std in release builds for minimal binary size. Tests and debug builds
 * use std for better error messages and easier debugging.
 */

#![cfg_attr(not(any(debug_assertions, test, doc)), no_std)]

//! # darwin-timeout
//!
//! A native macOS/Darwin implementation of the GNU timeout command.
//!
//! ## Quick Start
//!
//! ```rust
//! use darwin_timeout::{parse_duration, parse_signal, signal::Signal};
//! use core::time::Duration;
//!
//! // Parse duration strings
//! let dur = parse_duration("30s").unwrap();
//! assert_eq!(dur, Duration::from_secs(30));
//!
//! // Parse signal specifications
//! let sig = parse_signal("TERM").unwrap();
//! assert_eq!(sig, Signal::SIGTERM);
//! ```

extern crate alloc;

/* no_std support modules - custom allocator, panic handler, I/O primitives */
mod allocator;
pub mod io;
mod panic;
pub mod process;
pub mod sync;

pub mod args;
pub mod duration;
pub mod error;
pub mod runner;
pub mod signal;
pub mod wait;

pub use args::Args;
pub use duration::{is_no_timeout, parse_duration};
pub use error::{Result, TimeoutError, exit_codes};
pub use process::ResourceUsage;
pub use runner::{
    AttemptResult, Attempts, HookResult, MAX_RETRIES, RunConfig, RunResult, TimeoutReason,
    cleanup_signal_forwarding, run_command, run_with_retry, setup_signal_forwarding,
};
pub use signal::{parse_signal, signal_name, signal_number};
