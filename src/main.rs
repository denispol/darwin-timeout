/*
 * main.rs
 *
 * Parse args, call runner, format output. Boring on purpose.
 * The interesting stuff is in runner.rs.
 *
 * --json is for CI. Format is stable, don't change field names.
 */

use std::io::{BufWriter, Write};
use std::process::ExitCode;
use std::time::Instant;

use darwin_timeout::args::{OwnedArgs, parse_args};
use darwin_timeout::duration::parse_duration;
use darwin_timeout::error::exit_codes;
use darwin_timeout::runner::{RunConfig, RunResult, run_command, setup_signal_forwarding};

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

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("timeout: {e}");
            return ExitCode::from(exit_codes::INTERNAL_ERROR);
        }
    };

    /* handle --completions early and exit */
    if let Some(shell) = args.completions {
        Args::print_completions(shell);
        return ExitCode::SUCCESS;
    }

    let timeout_env = std::env::var("TIMEOUT").ok();
    let (duration_str, command, extra_args) = resolve_args(&args, timeout_env.as_deref());

    let (duration_str, command) = match (duration_str, command) {
        (Some(d), Some(c)) => (d, c),
        (None, _) => {
            if !args.quiet {
                eprintln!("timeout: missing duration (provide as argument or set TIMEOUT env var)");
            }
            return ExitCode::from(exit_codes::INTERNAL_ERROR);
        }
        (Some(_), None) => {
            if !args.quiet {
                eprintln!("timeout: missing command");
            }
            return ExitCode::from(exit_codes::INTERNAL_ERROR);
        }
    };

    let config = match RunConfig::from_args(&args, &duration_str) {
        Ok(config) => config,
        Err(e) => {
            if !args.quiet {
                eprintln!("timeout: {e}");
            }
            return e.exit_code();
        }
    };

    /* Set up signal forwarding before spawning child */
    let _ = setup_signal_forwarding();

    let start = Instant::now();
    let result = run_command(&command, &extra_args, &config);
    let elapsed_ms = start.elapsed().as_millis() as u64;

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

            ExitCode::from(exit_code)
        }
        Err(e) => {
            if args.json {
                print_json_error(&e, elapsed_ms);
            } else if !args.quiet {
                eprintln!("timeout: {e}");
            }
            e.exit_code()
        }
    }
}

fn print_json_output(result: &RunResult, elapsed_ms: u64, exit_code: u8) {
    /* Schema version 2: added hook_* fields for on-timeout results */
    const SCHEMA_VERSION: u8 = 2;
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    match result {
        RunResult::Completed(status) => {
            let code = status.code().unwrap_or(-1);
            let _ = writeln!(
                out,
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

            /* Build hook fields if hook was run */
            let hook_json =
                hook.as_ref()
                    .map(|h| (h.ran, h.exit_code, h.timed_out, h.elapsed_ms));

            let _ = write!(
                out,
                r#"{{"schema_version":{},"status":"timeout","signal":"{}","signal_num":{},"killed":{},"command_exit_code":{},"exit_code":{},"elapsed_ms":{}"#,
                SCHEMA_VERSION,
                darwin_timeout::signal::signal_name(*signal),
                sig_num,
                killed,
                status_code,
                exit_code,
                elapsed_ms
            );
            if let Some((ran, hook_exit, timed_out, hook_ms)) = hook_json {
                let _ = match hook_exit {
                    Some(c) => write!(
                        out,
                        r#","hook_ran":{},"hook_exit_code":{},"hook_timed_out":{},"hook_elapsed_ms":{}"#,
                        ran, c, timed_out, hook_ms
                    ),
                    None => write!(
                        out,
                        r#","hook_ran":{},"hook_exit_code":null,"hook_timed_out":{},"hook_elapsed_ms":{}"#,
                        ran, timed_out, hook_ms
                    ),
                };
            }
            let _ = writeln!(out, "}}");
        }
        RunResult::SignalForwarded { signal, status } => {
            let sig_num = darwin_timeout::signal::signal_number(*signal);
            let status_code = status.and_then(|s| s.code()).unwrap_or(-1);
            let _ = writeln!(
                out,
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
    /* BufWriter flushes on drop */
}

fn print_json_error(err: &darwin_timeout::error::TimeoutError, elapsed_ms: u64) {
    use darwin_timeout::error::{TimeoutError, exit_codes};
    const SCHEMA_VERSION: u8 = 2;
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    let exit_code = match err {
        TimeoutError::CommandNotFound(_) => exit_codes::NOT_FOUND,
        TimeoutError::PermissionDenied(_) => exit_codes::CANNOT_INVOKE,
        _ => exit_codes::INTERNAL_ERROR,
    };
    /* Escape control characters for valid JSON */
    let msg = escape_json_string(&err.to_string());
    let _ = writeln!(
        out,
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
                use std::fmt::Write;
                let _ = write!(result, "\\u{:04x}", c as u32);
            }
            c => result.push(c),
        }
    }
    result
}
