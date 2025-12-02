/*
 * main.rs
 *
 * Parse args, call runner, format output. Boring on purpose.
 * The interesting stuff is in runner.rs.
 *
 * --json is for CI. Format is stable, don't change field names.
 */

#![cfg_attr(not(any(debug_assertions, test, doc)), no_std)]
#![cfg_attr(not(any(debug_assertions, test, doc)), no_main)]

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write as FmtWrite;

use darwin_timeout::args::{OwnedArgs, parse_args};
use darwin_timeout::duration::parse_duration;
use darwin_timeout::error::exit_codes;
use darwin_timeout::runner::{RunConfig, RunResult, run_command, setup_signal_forwarding};
use darwin_timeout::wait::wait_for_file;
use darwin_timeout::{eprintln, println};

/* import alloc crate in no_std mode */
#[cfg(not(any(debug_assertions, test, doc)))]
extern crate alloc;

/* in debug/test mode, use std's alloc */
#[cfg(any(debug_assertions, test, doc))]
use std as alloc;

/* mach_continuous_time for elapsed timing - same as runner.rs */
#[repr(C)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

unsafe extern "C" {
    fn mach_continuous_time() -> u64;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
}

use core::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

/* cached timebase (packed as numer << 32 | denom, 0 = not initialized) */
static TIMEBASE_CACHE: AtomicU64 = AtomicU64::new(0);

/* current time in nanoseconds */
#[inline]
fn precise_now_ns() -> u64 {
    let cached = TIMEBASE_CACHE.load(AtomicOrdering::Relaxed);
    let (numer, denom) = if cached == 0 {
        let mut info = MachTimebaseInfo { numer: 0, denom: 0 };
        // SAFETY: info is valid MachTimebaseInfo struct
        unsafe {
            mach_timebase_info(&raw mut info);
        }
        let packed = (u64::from(info.numer) << 32) | u64::from(info.denom);
        TIMEBASE_CACHE.store(packed, AtomicOrdering::Relaxed);
        (u64::from(info.numer), u64::from(info.denom))
    } else {
        ((cached >> 32), (cached & 0xFFFF_FFFF))
    };

    // SAFETY: mach_continuous_time has no preconditions
    let abs_time = unsafe { mach_continuous_time() };
    if numer == denom {
        return abs_time;
    }
    #[allow(clippy::cast_possible_truncation)]
    ((u128::from(abs_time) * u128::from(numer) / u128::from(denom)) as u64)
}

/// Resolve duration/command from args and TIMEOUT env var.
///
/// When TIMEOUT env is set, the user may omit the duration from CLI.
/// Clap still parses positional args left-to-right, so "echo hello"
/// becomes duration="echo", command="hello". We detect this by checking
/// if the parsed "duration" is actually valid. If not, and TIMEOUT env
/// is set, we shift: env becomes duration, the parsed "duration" becomes command.
#[inline]
fn resolve_args(
    args: &OwnedArgs,
    timeout_env: Option<&str>,
) -> (Option<String>, Option<String>, Vec<String>) {
    match (&args.duration, &args.command, timeout_env) {
        /* Both duration and command provided on CLI, AND env var set - need disambiguation */
        (Some(dur), Some(cmd), Some(env_dur)) => {
            /* Check if this is actually an env fallback case:
             * If TIMEOUT env is set and the "duration" doesn't parse,
             * then user intended: TIMEOUT=dur cmd arg1 arg2
             * Clap saw: duration=cmd, command=arg1, args=[arg2...]
             */
            if parse_duration(dur).is_err() {
                /* Shift: env=duration, dur=command, cmd+args=args */
                let mut new_args = vec![cmd.clone()];
                new_args.extend(args.args.iter().cloned());
                (Some(env_dur.to_string()), Some(dur.clone()), new_args)
            } else {
                /* Both CLI duration and TIMEOUT env are valid - warn about ambiguity */
                eprintln!(
                    "warning: TIMEOUT env var set but '{}' also looks like a valid duration; \
                     using CLI argument (use -- separator to disambiguate)",
                    dur
                );
                (Some(dur.clone()), Some(cmd.clone()), args.args.clone())
            }
        }
        /* Both provided on CLI, no env - fast path, no parse_duration check needed */
        (Some(dur), Some(cmd), None) => (Some(dur.clone()), Some(cmd.clone()), args.args.clone()),
        /* Only one positional: duration from env, first positional is command */
        (Some(first_pos), None, Some(env_dur)) => (
            Some(env_dur.to_string()),
            Some(first_pos.clone()),
            args.args.clone(),
        ),
        /* Only duration provided, no command */
        (Some(dur), None, None) => (Some(dur.clone()), None, args.args.clone()),
        /* No positionals, but env set */
        (None, _, Some(env_dur)) => (Some(env_dur.to_string()), None, args.args.clone()),
        /* Nothing provided */
        (None, _, None) => (None, None, args.args.clone()),
    }
}

/* release build entry point - C ABI */
#[cfg(not(any(debug_assertions, test, doc)))]
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const i8) -> i32 {
    run_main() as i32
}

/* debug/test builds use standard Rust entry point */
#[cfg(any(debug_assertions, test, doc))]
fn main() {
    std::process::exit(run_main() as i32);
}

/* shared implementation */
fn run_main() -> u8 {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("timeout: {}", e);
            return exit_codes::INTERNAL_ERROR;
        }
    };

    let timeout_env = darwin_timeout::args::get_env(b"TIMEOUT\0");
    let (duration_str, command, extra_args) = resolve_args(&args, timeout_env.as_deref());

    let (duration_str, command) = match (duration_str, command) {
        (Some(d), Some(c)) => (d, c),
        (None, _) => {
            if !args.quiet {
                eprintln!("timeout: missing duration (provide as argument or set TIMEOUT env var)");
            }
            return exit_codes::INTERNAL_ERROR;
        }
        (Some(_), None) => {
            if !args.quiet {
                eprintln!("timeout: missing command");
            }
            return exit_codes::INTERNAL_ERROR;
        }
    };

    let config = match RunConfig::from_args(&args, &duration_str) {
        Ok(config) => config,
        Err(e) => {
            if !args.quiet {
                eprintln!("timeout: {}", e);
            }
            return e.exit_code();
        }
    };

    /* Wait for file if --wait-for-file is set (before starting command) */
    if let Some(ref path) = args.wait_for_file {
        let wait_timeout = args
            .wait_for_file_timeout
            .as_ref()
            .map(|s| parse_duration(s))
            .transpose();

        let wait_timeout = match wait_timeout {
            Ok(t) => t,
            Err(e) => {
                if !args.quiet {
                    eprintln!("timeout: invalid --wait-for-file-timeout: {}", e);
                }
                return exit_codes::INTERNAL_ERROR;
            }
        };

        if args.verbose && !args.quiet {
            match wait_timeout {
                Some(d) => {
                    let secs = d.as_secs();
                    let tenths = d.subsec_millis() / 100;
                    eprintln!(
                        "timeout: waiting for file '{}' (timeout: {}.{}s)",
                        path, secs, tenths
                    );
                }
                None => eprintln!("timeout: waiting for file '{}' (no timeout)", path),
            }
        }

        if let Err(e) = wait_for_file(path, wait_timeout, config.confine) {
            if args.json {
                print_json_error(&e, 0);
            } else if !args.quiet {
                eprintln!("timeout: {}", e);
            }
            return e.exit_code();
        }

        if args.verbose && !args.quiet {
            eprintln!("timeout: file '{}' found, starting command", path);
        }
    }

    /* Set up signal forwarding before spawning child */
    let _ = setup_signal_forwarding();

    let start_ns = precise_now_ns();
    let result = run_command(&command, &extra_args, &config);
    let elapsed_ms = (precise_now_ns() - start_ns) / 1_000_000;

    match result {
        Ok(run_result) => {
            let exit_code = run_result.exit_code(args.preserve_status, config.timeout_exit_code);

            /* Warn if custom exit code conflicts with reserved codes (only when timeout occurs) */
            if let Some(code) = args.timeout_exit_code
                && matches!(run_result, RunResult::TimedOut { .. })
                && (125..=137).contains(&code)
            {
                eprintln!(
                    "warning: --timeout-exit-code {} may conflict with reserved exit codes (125-137)",
                    code
                );
            }

            if args.json {
                print_json_output(&run_result, elapsed_ms, exit_code);
            }

            exit_code
        }
        Err(e) => {
            if args.json {
                print_json_error(&e, elapsed_ms);
            } else if !args.quiet {
                eprintln!("timeout: {}", e);
            }
            e.exit_code()
        }
    }
}

fn print_json_output(result: &RunResult, elapsed_ms: u64, exit_code: u8) {
    /* Schema version 2: added hook_* fields for on-timeout results */
    const SCHEMA_VERSION: u8 = 2;

    match result {
        RunResult::Completed(status) => {
            let code = status.code().unwrap_or(-1);
            println!(
                r#"{{"schema_version":{},"status":"completed","exit_code":{},"elapsed_ms":{}}}"#,
                SCHEMA_VERSION, code, elapsed_ms
            );
        }
        RunResult::TimedOut {
            signal,
            killed,
            status,
            hook,
        } => {
            let sig_num = darwin_timeout::signal::signal_number(*signal);
            let status_code = status.and_then(|s| s.code()).unwrap_or(-1);
            let sig_name = darwin_timeout::signal::signal_name(*signal);

            /* Build the JSON incrementally */
            let mut json = String::with_capacity(256);
            let _ = write!(
                json,
                r#"{{"schema_version":{},"status":"timeout","signal":"{}","signal_num":{},"killed":{},"command_exit_code":{},"exit_code":{},"elapsed_ms":{}"#,
                SCHEMA_VERSION, sig_name, sig_num, killed, status_code, exit_code, elapsed_ms
            );

            /* Add hook fields if hook was run */
            if let Some(h) = hook {
                match h.exit_code {
                    Some(c) => {
                        let _ = write!(
                            json,
                            r#","hook_ran":{},"hook_exit_code":{},"hook_timed_out":{},"hook_elapsed_ms":{}"#,
                            h.ran, c, h.timed_out, h.elapsed_ms
                        );
                    }
                    None => {
                        let _ = write!(
                            json,
                            r#","hook_ran":{},"hook_exit_code":null,"hook_timed_out":{},"hook_elapsed_ms":{}"#,
                            h.ran, h.timed_out, h.elapsed_ms
                        );
                    }
                }
            }
            json.push('}');
            println!("{}", json);
        }
        RunResult::SignalForwarded { signal, status } => {
            let sig_num = darwin_timeout::signal::signal_number(*signal);
            let status_code = status.and_then(|s| s.code()).unwrap_or(-1);
            println!(
                r#"{{"schema_version":{},"status":"signal_forwarded","signal":"{}","signal_num":{},"command_exit_code":{},"exit_code":{},"elapsed_ms":{}}}"#,
                SCHEMA_VERSION,
                darwin_timeout::signal::signal_name(*signal),
                sig_num,
                status_code,
                exit_code,
                elapsed_ms
            );
        }
    }
}

fn print_json_error(err: &darwin_timeout::error::TimeoutError, elapsed_ms: u64) {
    const SCHEMA_VERSION: u8 = 2;

    let exit_code = err.exit_code();
    /* Escape control characters for valid JSON */
    let msg = escape_json_string(&err.to_string());
    println!(
        r#"{{"schema_version":{},"status":"error","error":"{}","exit_code":{},"elapsed_ms":{}}}"#,
        SCHEMA_VERSION, msg, exit_code, elapsed_ms
    );
}

/* escape string for JSON - handles quotes, backslashes, control chars */
fn escape_json_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c < '\x20' => {
                /* Escape other control characters as \uXXXX */
                let _ = write!(result, "\\u{:04x}", c as u32);
            }
            c => result.push(c),
        }
    }
    result
}
