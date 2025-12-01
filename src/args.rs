/*
 * args.rs
 *
 * manual CLI argument parsing - no clap dependency.
 * parsing loop is ~250 lines vs ~400KB of clap codegen.
 *
 * GNU compatible: scripts written for GNU timeout should just work.
 * We add --json but don't change existing flags.
 *
 * Uses indices into original argv slice to avoid string clones.
 * Only allocates when:
 * - env var fallback is used
 * - short option cluster contains embedded value (-sTERM)
 * - --option=value syntax (borrows from slice, but may need owned for env)
 */

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::ffi::{CStr, c_char, c_int};

/* Darwin-specific APIs to get argc/argv and environment */
unsafe extern "C" {
    fn _NSGetArgc() -> *const c_int;
    fn _NSGetArgv() -> *const *const *const c_char;
    fn getenv(name: *const c_char) -> *const c_char;
}

/* Helper to read environment variable */
pub fn get_env(name: &[u8]) -> Option<String> {
    // SAFETY: name must be null-terminated, getenv is safe with valid C string
    unsafe {
        let ptr = getenv(name.as_ptr() as *const c_char);
        if ptr.is_null() {
            None
        } else {
            Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
        }
    }
}

/* Get arguments from Darwin's _NSGetArgc/_NSGetArgv */
fn get_args_from_darwin() -> Vec<String> {
    // SAFETY: _NSGetArgc/_NSGetArgv always return valid pointers on macOS
    unsafe {
        let argc = *_NSGetArgc();
        let argv = *_NSGetArgv();
        let mut args = Vec::with_capacity(argc as usize);
        for i in 0..argc as isize {
            let arg_ptr = *argv.offset(i);
            if !arg_ptr.is_null() {
                args.push(CStr::from_ptr(arg_ptr).to_string_lossy().into_owned());
            }
        }
        args
    }
}

/// Parsed argument - either borrowed from argv or owned (env var / embedded value)
#[derive(Debug, Clone)]
pub enum ArgValue<'a> {
    Borrowed(&'a str),
    Owned(String),
}

impl<'a> ArgValue<'a> {
    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Borrowed(s) => s,
            Self::Owned(s) => s,
        }
    }

    /// Convert to owned String (for APIs that need ownership)
    pub fn into_owned(self) -> String {
        match self {
            Self::Borrowed(s) => s.to_string(),
            Self::Owned(s) => s,
        }
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.as_str().is_empty()
    }
}

impl Default for ArgValue<'_> {
    fn default() -> Self {
        Self::Borrowed("")
    }
}

#[derive(Debug, Clone, Default)]
pub struct Args<'a> {
    pub json: bool,
    pub signal: ArgValue<'a>,
    pub kill_after: Option<ArgValue<'a>>,
    pub preserve_status: bool,
    pub foreground: bool,
    pub verbose: bool,
    pub quiet: bool,
    pub timeout_exit_code: Option<u8>,
    pub on_timeout: Option<ArgValue<'a>>,
    pub on_timeout_limit: ArgValue<'a>,
    pub duration: Option<ArgValue<'a>>,
    pub command: Option<ArgValue<'a>>,
    pub args: Vec<ArgValue<'a>>,
}

/// Owned version for when we need 'static lifetime (after env var resolution)
#[derive(Debug, Clone, Default)]
pub struct OwnedArgs {
    pub json: bool,
    pub signal: String,
    pub kill_after: Option<String>,
    pub preserve_status: bool,
    pub foreground: bool,
    pub verbose: bool,
    pub quiet: bool,
    pub timeout_exit_code: Option<u8>,
    pub on_timeout: Option<String>,
    pub on_timeout_limit: String,
    pub duration: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
}

impl<'a> Args<'a> {
    /// Convert to owned version (for main.rs which needs 'static)
    pub fn into_owned(self) -> OwnedArgs {
        OwnedArgs {
            json: self.json,
            signal: self.signal.into_owned(),
            kill_after: self.kill_after.map(|v| v.into_owned()),
            preserve_status: self.preserve_status,
            foreground: self.foreground,
            verbose: self.verbose,
            quiet: self.quiet,
            timeout_exit_code: self.timeout_exit_code,
            on_timeout: self.on_timeout.map(|v| v.into_owned()),
            on_timeout_limit: self.on_timeout_limit.into_owned(),
            duration: self.duration.map(|v| v.into_owned()),
            command: self.command.map(|v| v.into_owned()),
            args: self.args.into_iter().map(|v| v.into_owned()).collect(),
        }
    }
}

/// parsing error with context
#[derive(Debug)]
pub struct ParseError {
    pub message: String,
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// parse from Darwin's argc/argv, applying env var fallbacks
/// returns OwnedArgs since we convert from C strings
pub fn parse_args() -> Result<OwnedArgs, ParseError> {
    let args = get_args_from_darwin();
    let parsed = parse_from_slice(&args[1..])?;
    let mut owned = parsed.into_owned();

    /* apply env var fallbacks: CLI > env > default */
    if owned.signal.is_empty() {
        owned.signal = get_env(b"TIMEOUT_SIGNAL\0").unwrap_or_else(|| "TERM".to_string());
    }
    if owned.kill_after.is_none() {
        owned.kill_after = get_env(b"TIMEOUT_KILL_AFTER\0");
    }
    if owned.duration.is_none() {
        owned.duration = get_env(b"TIMEOUT\0");
    }

    Ok(owned)
}

/// parse from slice (for testing and internal use)
pub fn parse_from_slice<'a>(args: &'a [String]) -> Result<Args<'a>, ParseError> {
    let mut result = Args {
        signal: ArgValue::Borrowed(""), // will apply env fallback later
        on_timeout_limit: ArgValue::Borrowed("5s"),
        ..Default::default()
    };

    let mut i = 0;
    let mut saw_separator = false;

    while i < args.len() {
        let arg = &args[i];

        /* after --, everything is command + args */
        if saw_separator {
            if result.command.is_none() {
                result.command = Some(ArgValue::Borrowed(arg));
            } else {
                result.args.push(ArgValue::Borrowed(arg));
            }
            i += 1;
            continue;
        }

        match arg.as_str() {
            "--" => {
                saw_separator = true;
            }
            "--help" | "-h" => {
                print_help();
                // SAFETY: exit is always safe
                unsafe { libc::exit(0) };
            }
            "--version" | "-V" => {
                print_version();
                // SAFETY: exit is always safe
                unsafe { libc::exit(0) };
            }
            "--json" => result.json = true,
            "-p" | "--preserve-status" => result.preserve_status = true,
            "-f" | "--foreground" => result.foreground = true,
            "-v" | "--verbose" => result.verbose = true,
            "-q" | "--quiet" => result.quiet = true,

            /* options with values: -s SIGNAL or --signal=SIGNAL */
            "-s" => {
                i += 1;
                result.signal = ArgValue::Borrowed(args.get(i).ok_or_else(|| ParseError {
                    message: "-s requires a signal name or number".to_string(),
                })?);
            }
            "--signal" => {
                i += 1;
                result.signal = ArgValue::Borrowed(args.get(i).ok_or_else(|| ParseError {
                    message: "--signal requires a value".to_string(),
                })?);
            }
            s if s.starts_with("--signal=") => {
                result.signal = ArgValue::Borrowed(&s[9..]);
            }

            "-k" => {
                i += 1;
                result.kill_after = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "-k requires a duration".to_string(),
                    }
                })?));
            }
            "--kill-after" => {
                i += 1;
                result.kill_after = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "--kill-after requires a value".to_string(),
                    }
                })?));
            }
            s if s.starts_with("--kill-after=") => {
                result.kill_after = Some(ArgValue::Borrowed(&s[13..]));
            }

            "--timeout-exit-code" => {
                i += 1;
                let val = args.get(i).ok_or_else(|| ParseError {
                    message: "--timeout-exit-code requires a value".to_string(),
                })?;
                result.timeout_exit_code = Some(val.parse().map_err(|_| ParseError {
                    message: format!("invalid exit code: '{val}' (must be 0-255)"),
                })?);
            }
            s if s.starts_with("--timeout-exit-code=") => {
                let val = &s[20..];
                result.timeout_exit_code = Some(val.parse().map_err(|_| ParseError {
                    message: format!("invalid exit code: '{val}' (must be 0-255)"),
                })?);
            }

            "--on-timeout" => {
                i += 1;
                result.on_timeout = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "--on-timeout requires a command".to_string(),
                    }
                })?));
            }
            s if s.starts_with("--on-timeout=") => {
                result.on_timeout = Some(ArgValue::Borrowed(&s[13..]));
            }

            "--on-timeout-limit" => {
                i += 1;
                result.on_timeout_limit =
                    ArgValue::Borrowed(args.get(i).ok_or_else(|| ParseError {
                        message: "--on-timeout-limit requires a duration".to_string(),
                    })?);
            }
            s if s.starts_with("--on-timeout-limit=") => {
                result.on_timeout_limit = ArgValue::Borrowed(&s[19..]);
            }

            /* unknown long option */
            s if s.starts_with("--") => {
                return Err(ParseError {
                    message: format!("unknown option: {s}"),
                });
            }

            /* short option cluster like -pfv or unknown -x */
            s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                /* could be a negative number for duration, check if we're in positional mode */
                if result.duration.is_some() || s.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
                {
                    /* looks like a negative number or command starting with - */
                    /* treat as positional */
                    if result.duration.is_none() {
                        result.duration = Some(ArgValue::Borrowed(arg));
                    } else if result.command.is_none() {
                        result.command = Some(ArgValue::Borrowed(arg));
                    } else {
                        result.args.push(ArgValue::Borrowed(arg));
                    }
                } else {
                    /* parse short option cluster */
                    let bytes = s.as_bytes();
                    let mut j = 1; // skip leading '-'
                    while j < bytes.len() {
                        match bytes[j] {
                            b'h' => {
                                print_help();
                                // SAFETY: exit is always safe
                                unsafe { libc::exit(0) };
                            }
                            b'V' => {
                                print_version();
                                // SAFETY: exit is always safe
                                unsafe { libc::exit(0) };
                            }
                            b'p' => result.preserve_status = true,
                            b'f' => result.foreground = true,
                            b'v' => result.verbose = true,
                            b'q' => result.quiet = true,
                            b's' => {
                                /* rest of cluster or next arg is the value */
                                if j + 1 < bytes.len() {
                                    /* embedded value like -sTERM - must allocate */
                                    result.signal = ArgValue::Owned(s[j + 1..].to_string());
                                    break;
                                } else {
                                    i += 1;
                                    result.signal =
                                        ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                                            ParseError {
                                                message: "-s requires a signal name".to_string(),
                                            }
                                        })?);
                                }
                            }
                            b'k' => {
                                if j + 1 < bytes.len() {
                                    result.kill_after =
                                        Some(ArgValue::Owned(s[j + 1..].to_string()));
                                    break;
                                } else {
                                    i += 1;
                                    result.kill_after =
                                        Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                                            ParseError {
                                                message: "-k requires a duration".to_string(),
                                            }
                                        })?));
                                }
                            }
                            c => {
                                return Err(ParseError {
                                    message: format!("unknown option: -{}", c as char),
                                });
                            }
                        }
                        j += 1;
                    }
                }
            }

            /* positional args: duration, command, args... */
            _ => {
                if result.duration.is_none() {
                    result.duration = Some(ArgValue::Borrowed(arg));
                } else if result.command.is_none() {
                    result.command = Some(ArgValue::Borrowed(arg));
                } else {
                    result.args.push(ArgValue::Borrowed(arg));
                }
            }
        }

        i += 1;
    }

    /* validate conflicts */
    if result.verbose && result.quiet {
        return Err(ParseError {
            message: "-v/--verbose cannot be used with -q/--quiet".to_string(),
        });
    }

    Ok(result)
}

/// for testing - parse from iterator without env fallbacks
#[cfg(test)]
pub fn try_parse_from<I, S>(args: I) -> Result<OwnedArgs, ParseError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    /* skip program name */
    let slice = if !args.is_empty() {
        &args[1..]
    } else {
        &args[..]
    };
    let parsed = parse_from_slice(slice)?;
    let mut owned = parsed.into_owned();
    /* for tests, apply default signal if empty (no env lookup) */
    if owned.signal.is_empty() {
        owned.signal = "TERM".to_string();
    }
    Ok(owned)
}

fn print_version() {
    crate::io::print_str("timeout ");
    crate::io::print_str(env!("CARGO_PKG_VERSION"));
    crate::io::print_str("\n");
}

fn print_help() {
    crate::io::print_str(
        r#"Usage: timeout [OPTIONS] DURATION COMMAND [ARG]...

Run a command with a time limit.

Arguments:
  DURATION  Time before sending signal (30, 30s, 1.5m, 2h, 1d)
  COMMAND   Command to run
  ARG       Arguments for the command

Options:
  -s, --signal <SIGNAL>           Signal to send on timeout [env: TIMEOUT_SIGNAL] [default: TERM]
  -k, --kill-after <DURATION>     Send KILL signal if still running after DURATION [env: TIMEOUT_KILL_AFTER]
  -p, --preserve-status           Exit with same status as COMMAND, even on timeout
  -f, --foreground                Allow COMMAND to read from TTY and get TTY signals
  -v, --verbose                   Diagnose to stderr any signal sent upon timeout
  -q, --quiet                     Suppress timeout's own diagnostic output to stderr
      --timeout-exit-code <CODE>  Exit with CODE instead of 124 when timeout occurs
      --on-timeout <CMD>          Run CMD before sending the timeout signal (%p = PID)
      --on-timeout-limit <DUR>    Timeout for the --on-timeout hook command [default: 5s]
      --json                      Output result as JSON (for scripting/CI)
  -h, --help                      Print help
  -V, --version                   Print version

Exit status:
  124 if COMMAND times out, and --preserve-status is not specified
  125 if the timeout command itself fails
  126 if COMMAND is found but cannot be invoked
  127 if COMMAND cannot be found
  137 if COMMAND (or timeout itself) is sent SIGKILL (128+9)
  the exit status of COMMAND otherwise

Environment:
  TIMEOUT         Default duration if not specified on command line
  TIMEOUT_SIGNAL  Default signal (overridden by -s)
  TIMEOUT_KILL_AFTER  Default kill-after duration (overridden by -k)
"#,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_args() {
        let args = try_parse_from(["timeout", "5", "sleep", "10"]).unwrap();
        assert_eq!(args.duration, Some("5".to_string()));
        assert_eq!(args.command, Some("sleep".to_string()));
        assert_eq!(args.args, vec!["10"]);
        assert_eq!(args.signal, "TERM");
        assert!(!args.preserve_status);
        assert!(!args.foreground);
        assert!(!args.verbose);
        assert!(!args.quiet);
        assert!(args.kill_after.is_none());
        assert!(!args.json);
        assert!(args.timeout_exit_code.is_none());
        assert!(args.on_timeout.is_none());
    }

    #[test]
    fn test_all_options() {
        let args = try_parse_from([
            "timeout",
            "-s",
            "KILL",
            "-k",
            "5s",
            "-p",
            "-f",
            "-v",
            "--timeout-exit-code",
            "42",
            "--on-timeout",
            "echo %p",
            "30s",
            "my_command",
            "arg1",
            "arg2",
        ])
        .unwrap();

        assert_eq!(args.signal, "KILL");
        assert_eq!(args.kill_after, Some("5s".to_string()));
        assert!(args.preserve_status);
        assert!(args.foreground);
        assert!(args.verbose);
        assert!(!args.quiet);
        assert_eq!(args.timeout_exit_code, Some(42));
        assert_eq!(args.on_timeout, Some("echo %p".to_string()));
        assert_eq!(args.duration, Some("30s".to_string()));
        assert_eq!(args.command, Some("my_command".to_string()));
        assert_eq!(args.args, vec!["arg1", "arg2"]);
    }

    #[test]
    fn test_long_options() {
        let args = try_parse_from([
            "timeout",
            "--signal=HUP",
            "--kill-after=10m",
            "--preserve-status",
            "--foreground",
            "--verbose",
            "1h",
            "cmd",
        ])
        .unwrap();

        assert_eq!(args.signal, "HUP");
        assert_eq!(args.kill_after, Some("10m".to_string()));
        assert!(args.preserve_status);
        assert!(args.foreground);
        assert!(args.verbose);
        assert!(!args.quiet);
        assert_eq!(args.duration, Some("1h".to_string()));
        assert_eq!(args.command, Some("cmd".to_string()));
    }

    #[test]
    fn test_quiet_verbose_conflict() {
        let result = try_parse_from(["timeout", "-q", "-v", "5s", "cmd"]);
        assert!(result.is_err(), "-q and -v should be mutually exclusive");
    }

    #[test]
    fn test_command_with_dashes() {
        let args = try_parse_from(["timeout", "5", "--", "-c", "echo", "hello"]).unwrap();
        assert_eq!(args.command, Some("-c".to_string()));
        assert_eq!(args.args, vec!["echo", "hello"]);
    }

    #[test]
    fn test_json_flag() {
        let args = try_parse_from(["timeout", "--json", "5s", "sleep", "1"]).unwrap();
        assert!(args.json);
        assert_eq!(args.duration, Some("5s".to_string()));
    }

    #[test]
    fn test_quiet_flag() {
        let args = try_parse_from(["timeout", "-q", "5s", "cmd"]).unwrap();
        assert!(args.quiet);
    }

    #[test]
    fn test_timeout_exit_code() {
        let args = try_parse_from(["timeout", "--timeout-exit-code", "99", "5s", "cmd"]).unwrap();
        assert_eq!(args.timeout_exit_code, Some(99));
    }

    #[test]
    fn test_on_timeout() {
        let args = try_parse_from(["timeout", "--on-timeout", "echo %p", "5s", "cmd"]).unwrap();
        assert_eq!(args.on_timeout, Some("echo %p".to_string()));
    }

    #[test]
    fn test_short_option_cluster() {
        let args = try_parse_from(["timeout", "-pfv", "5s", "cmd"]).unwrap();
        assert!(args.preserve_status);
        assert!(args.foreground);
        assert!(args.verbose);
    }

    #[test]
    fn test_equals_syntax() {
        let args = try_parse_from([
            "timeout",
            "--signal=INT",
            "--timeout-exit-code=200",
            "5s",
            "cmd",
        ])
        .unwrap();
        assert_eq!(args.signal, "INT");
        assert_eq!(args.timeout_exit_code, Some(200));
    }

    #[test]
    fn test_unknown_option() {
        let result = try_parse_from(["timeout", "--unknown", "5s", "cmd"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unknown option"));
    }

    #[test]
    fn test_missing_signal_value() {
        let result = try_parse_from(["timeout", "-s"]);
        assert!(result.is_err());
    }
}
