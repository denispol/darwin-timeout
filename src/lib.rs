/*
 * lib.rs
 *
 * Exists mostly for testing. Integration tests need our types, doc tests
 * need a lib. You could use this as a library but honestly just shell out.
 */

//! # darwin-timeout
//!
//! A native macOS/Darwin implementation of the GNU timeout command.
//!
//! ## Quick Start
//!
//! ```rust
//! use darwin_timeout::{parse_duration, parse_signal, signal::Signal};
//! use std::time::Duration;
//!
//! // Parse duration strings
//! let dur = parse_duration("30s").unwrap();
//! assert_eq!(dur, Duration::from_secs(30));
//!
//! // Parse signal specifications
//! let sig = parse_signal("TERM").unwrap();
//! assert_eq!(sig, Signal::SIGTERM);
//! ```

pub mod args;
pub mod duration;
pub mod error;
pub mod runner;
pub mod signal;

pub use args::Args;
pub use duration::{is_no_timeout, parse_duration};
pub use error::{Result, TimeoutError, exit_codes};
pub use runner::{
    HookResult, RunConfig, RunResult, cleanup_signal_forwarding, run_command,
    setup_signal_forwarding,
};
pub use signal::{parse_signal, signal_name, signal_number};
