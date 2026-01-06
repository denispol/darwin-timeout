/*
 * lib.rs
 *
 * no_std in release builds for minimal binary size. Tests and debug builds
 * use std for better error messages and easier debugging.
 */

//! # procguard
//!
//! The formally verified process supervisor for macOS.
//!
//! This crate provides both a CLI binary and a library for running commands
//! with time limits on macOS. It uses Darwin-specific APIs (`mach_continuous_time`,
//! `kqueue`, `libproc`) for accurate timing that survives system sleep.
//!
//! ## Platform Support
//!
//! **macOS only.** This crate uses Darwin kernel APIs not available on other platforms.
//! iOS support is planned for a future release (library subset only, no process spawning).
//!
//! ## Library Usage
//!
//! The primary entry points are [`run_command`] and [`run_with_retry`]:
//!
//! ```ignore
//! use procguard::{RunConfig, RunResult, Signal, run_command, setup_signal_forwarding};
//! use std::time::Duration;
//!
//! // Set up signal forwarding (optional but recommended)
//! let _ = setup_signal_forwarding();
//!
//! // Configure the timeout
//! let config = RunConfig {
//!     timeout: Duration::from_secs(30),
//!     signal: Signal::SIGTERM,
//!     kill_after: Some(Duration::from_secs(5)), // escalate to SIGKILL after 5s
//!     ..RunConfig::default()
//! };
//!
//! // Run a command
//! let args = ["-c".to_string(), "sleep 10".to_string()];
//! match run_command("sh", &args, &config) {
//!     Ok(RunResult::Completed { status, rusage }) => {
//!         println!("Command exited with code {:?}", status.code());
//!         println!("Peak memory: {} KB", rusage.max_rss_kb);
//!     }
//!     Ok(RunResult::TimedOut { signal, .. }) => {
//!         println!("Command timed out, sent {:?}", signal);
//!     }
//!     Ok(_) => println!("Other outcome"),
//!     Err(e) => eprintln!("Error: {}", e),
//! }
//! ```
//!
//! ## Parsing Utilities
//!
//! Helper functions for parsing duration and signal specifications:
//!
//! ```rust
//! use procguard::{parse_duration, parse_signal, signal::Signal};
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
//!
//! ## Features
//!
//! - **Zero-CPU waiting** via kqueue - no polling
//! - **Sleep-aware timing** via `mach_continuous_time`
//! - **Process group management** - kills child processes too
//! - **Signal forwarding** - SIGTERM/SIGINT passed to child
//! - **Resource limits** - memory, CPU time, CPU percent throttling
//! - **Retry with backoff** - automatic retry on timeout
//! - **JSON output** - machine-readable results (CLI only)
//!
//! ## Stability Note
//!
//! The library API is experimental. New fields may be added to [`RunConfig`]
//! in minor versions. Use `..RunConfig::default()` when constructing to ensure
//! forward compatibility. The [`RunResult`] enum is marked `#[non_exhaustive]`,
//! so match arms should include a wildcard pattern.

#![cfg_attr(not(any(debug_assertions, test, doc)), no_std)]

/* fail fast on unsupported platforms - darwin APIs required */
#[cfg(not(target_os = "macos"))]
compile_error!("procguard requires macOS (iOS support planned for future release)");

extern crate alloc;

/* no_std support modules - custom allocator, panic handler, I/O primitives */
mod allocator;
#[doc(hidden)]
pub mod io;
mod panic;
#[doc(hidden)]
pub mod proc_info;
pub mod process;
pub mod rlimit;
#[doc(hidden)]
pub mod sync;
#[doc(hidden)]
pub mod throttle;

pub mod args;
pub mod duration;
pub mod error;
pub mod runner;
pub mod signal;
pub mod time_math;
pub mod wait;

pub use args::Args;
pub use duration::{is_no_timeout, parse_duration};
pub use error::{Result, TimeoutError, exit_codes};
pub use process::ResourceUsage;
pub use rlimit::{ResourceLimits, parse_cpu_percent, parse_cpu_time, parse_mem_limit};
pub use runner::{
    AttemptResult, Attempts, HookResult, MAX_RETRIES, RunConfig, RunResult, TimeoutReason,
    cleanup_signal_forwarding, run_command, run_with_retry, setup_signal_forwarding,
};
pub use signal::{Signal, parse_signal, signal_name, signal_number};
