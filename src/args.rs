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
    // SAFETY: name must be null-terminated (caller ensures this), getenv returns
    // a valid C string pointer or null. CStr::from_ptr is safe with non-null result.
    // Multiple unsafe ops allowed: getenv + CStr::from_ptr share same validity invariant.
    #[allow(clippy::multiple_unsafe_ops_per_block)]
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
    // SAFETY: _NSGetArgc/_NSGetArgv always return valid pointers on macOS.
    // argc is the valid count, argv[0..argc] are valid null-terminated C strings.
    // Multiple unsafe ops allowed: all share the same invariant (valid argv array).
    #[allow(clippy::multiple_unsafe_ops_per_block)]
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

/// Time confinement mode - what clock to use for timeout measurement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Confine {
    /// Wall clock time (includes system sleep) - default, uses mach_continuous_time
    #[default]
    Wall,
    /// Active/awake time only (excludes system sleep) - uses CLOCK_MONOTONIC_RAW
    /// ~28% faster, useful for benchmarks where idle time shouldn't count
    Active,
}

impl Confine {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "wall" => Some(Self::Wall),
            "active" => Some(Self::Active),
            _ => None,
        }
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
    pub confine: Confine,
    pub wait_for_file: Option<ArgValue<'a>>,
    pub wait_for_file_timeout: Option<ArgValue<'a>>,
    pub retry: Option<ArgValue<'a>>,
    pub retry_delay: Option<ArgValue<'a>>,
    pub retry_backoff: Option<ArgValue<'a>>,
    pub heartbeat: Option<ArgValue<'a>>,
    pub stdin_timeout: Option<ArgValue<'a>>,
    pub stdin_passthrough: bool, /* non-consuming stdin watchdog */
    pub mem_limit: Option<ArgValue<'a>>, /* e.g. 1G */
    pub cpu_time: Option<ArgValue<'a>>,  /* cpu seconds via rlimit */
    pub cpu_percent: Option<ArgValue<'a>>, /* cpu throttling percentage */
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
    pub confine: Confine,
    pub wait_for_file: Option<String>,
    pub wait_for_file_timeout: Option<String>,
    pub retry: Option<String>,
    pub retry_delay: Option<String>,
    pub retry_backoff: Option<String>,
    pub heartbeat: Option<String>,
    pub stdin_timeout: Option<String>,
    pub stdin_passthrough: bool,
    pub mem_limit: Option<String>,
    pub cpu_time: Option<String>,
    pub cpu_percent: Option<String>,
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
            confine: self.confine,
            wait_for_file: self.wait_for_file.map(|v| v.into_owned()),
            wait_for_file_timeout: self.wait_for_file_timeout.map(|v| v.into_owned()),
            retry: self.retry.map(|v| v.into_owned()),
            retry_delay: self.retry_delay.map(|v| v.into_owned()),
            retry_backoff: self.retry_backoff.map(|v| v.into_owned()),
            heartbeat: self.heartbeat.map(|v| v.into_owned()),
            stdin_timeout: self.stdin_timeout.map(|v| v.into_owned()),
            stdin_passthrough: self.stdin_passthrough,
            mem_limit: self.mem_limit.map(|v| v.into_owned()),
            cpu_time: self.cpu_time.map(|v| v.into_owned()),
            cpu_percent: self.cpu_percent.map(|v| v.into_owned()),
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
    if owned.wait_for_file.is_none() {
        owned.wait_for_file = get_env(b"TIMEOUT_WAIT_FOR_FILE\0");
    }
    if owned.wait_for_file_timeout.is_none() {
        owned.wait_for_file_timeout = get_env(b"TIMEOUT_WAIT_FOR_FILE_TIMEOUT\0");
    }
    if owned.retry.is_none() {
        owned.retry = get_env(b"TIMEOUT_RETRY\0").filter(|s| !s.is_empty());
    }
    if owned.heartbeat.is_none() {
        owned.heartbeat = get_env(b"TIMEOUT_HEARTBEAT\0");
    }
    if owned.stdin_timeout.is_none() {
        owned.stdin_timeout = get_env(b"TIMEOUT_STDIN_TIMEOUT\0");
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

        /* check for -- separator before the command-is-set check */
        if arg == "--" {
            saw_separator = true;
            i += 1;
            continue;
        }

        /* once command is set, all remaining args go to the command */
        if result.command.is_some() {
            result.args.push(ArgValue::Borrowed(arg));
            i += 1;
            continue;
        }

        match arg.as_str() {
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

            "--confine" | "-c" => {
                i += 1;
                let val = args.get(i).ok_or_else(|| ParseError {
                    message: "--confine requires a value (wall or active)".to_string(),
                })?;
                result.confine = Confine::from_str(val).ok_or_else(|| ParseError {
                    message: format!("invalid confine mode: '{}' (use 'wall' or 'active')", val),
                })?;
            }
            s if s.starts_with("--confine=") => {
                let val = &s[10..];
                result.confine = Confine::from_str(val).ok_or_else(|| ParseError {
                    message: format!("invalid confine mode: '{}' (use 'wall' or 'active')", val),
                })?;
            }

            "--wait-for-file" => {
                i += 1;
                result.wait_for_file = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "--wait-for-file requires a path".to_string(),
                    }
                })?));
            }
            s if s.starts_with("--wait-for-file=") => {
                result.wait_for_file = Some(ArgValue::Borrowed(&s[16..]));
            }

            "--wait-for-file-timeout" => {
                i += 1;
                result.wait_for_file_timeout =
                    Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                        ParseError {
                            message: "--wait-for-file-timeout requires a duration".to_string(),
                        }
                    })?));
            }
            s if s.starts_with("--wait-for-file-timeout=") => {
                result.wait_for_file_timeout = Some(ArgValue::Borrowed(&s[24..]));
            }

            "--retry" => {
                i += 1;
                result.retry = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "--retry requires a count".to_string(),
                    }
                })?));
            }
            s if s.starts_with("--retry=") => {
                result.retry = Some(ArgValue::Borrowed(&s[8..]));
            }

            "--retry-delay" => {
                i += 1;
                result.retry_delay = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "--retry-delay requires a duration".to_string(),
                    }
                })?));
            }
            s if s.starts_with("--retry-delay=") => {
                result.retry_delay = Some(ArgValue::Borrowed(&s[14..]));
            }

            "--retry-backoff" => {
                i += 1;
                result.retry_backoff = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "--retry-backoff requires a multiplier (e.g., 2x)".to_string(),
                    }
                })?));
            }
            s if s.starts_with("--retry-backoff=") => {
                result.retry_backoff = Some(ArgValue::Borrowed(&s[16..]));
            }

            "-H" => {
                i += 1;
                result.heartbeat = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "-H requires a duration".to_string(),
                    }
                })?));
            }
            "--heartbeat" => {
                i += 1;
                result.heartbeat = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "--heartbeat requires a duration".to_string(),
                    }
                })?));
            }
            s if s.starts_with("--heartbeat=") => {
                result.heartbeat = Some(ArgValue::Borrowed(&s[12..]));
            }

            "-S" => {
                i += 1;
                result.stdin_timeout = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "-S requires a duration".to_string(),
                    }
                })?));
            }
            "--stdin-timeout" => {
                i += 1;
                result.stdin_timeout = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "--stdin-timeout requires a duration".to_string(),
                    }
                })?));
            }
            s if s.starts_with("--stdin-timeout=") => {
                result.stdin_timeout = Some(ArgValue::Borrowed(&s[16..]));
            }

            "--mem-limit" => {
                i += 1;
                result.mem_limit = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "--mem-limit requires a value".to_string(),
                    }
                })?));
            }
            s if s.starts_with("--mem-limit=") => {
                result.mem_limit = Some(ArgValue::Borrowed(&s[12..]));
            }

            "--cpu-time" => {
                i += 1;
                result.cpu_time = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "--cpu-time requires a duration".to_string(),
                    }
                })?));
            }
            s if s.starts_with("--cpu-time=") => {
                result.cpu_time = Some(ArgValue::Borrowed(&s[11..]));
            }

            "--cpu-percent" => {
                i += 1;
                result.cpu_percent = Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                    ParseError {
                        message: "--cpu-percent requires a value".to_string(),
                    }
                })?));
            }
            s if s.starts_with("--cpu-percent=") => {
                result.cpu_percent = Some(ArgValue::Borrowed(&s[14..]));
            }

            "--stdin-passthrough" => {
                result.stdin_passthrough = true;
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
                            b'c' => {
                                if j + 1 < bytes.len() {
                                    let val = &s[j + 1..];
                                    result.confine =
                                        Confine::from_str(val).ok_or_else(|| ParseError {
                                            message: format!(
                                                "invalid confine mode: '{}' (use 'wall' or 'active')",
                                                val
                                            ),
                                        })?;
                                    break;
                                } else {
                                    i += 1;
                                    let val = args.get(i).ok_or_else(|| ParseError {
                                        message: "-c requires a value (wall or active)".to_string(),
                                    })?;
                                    result.confine =
                                        Confine::from_str(val).ok_or_else(|| ParseError {
                                            message: format!(
                                                "invalid confine mode: '{}' (use 'wall' or 'active')",
                                                val
                                            ),
                                        })?;
                                }
                            }
                            b'r' => {
                                if j + 1 < bytes.len() {
                                    result.retry = Some(ArgValue::Owned(s[j + 1..].to_string()));
                                    break;
                                } else {
                                    i += 1;
                                    result.retry =
                                        Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                                            ParseError {
                                                message: "-r requires a retry count".to_string(),
                                            }
                                        })?));
                                }
                            }
                            b'H' => {
                                if j + 1 < bytes.len() {
                                    result.heartbeat =
                                        Some(ArgValue::Owned(s[j + 1..].to_string()));
                                    break;
                                } else {
                                    i += 1;
                                    result.heartbeat =
                                        Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                                            ParseError {
                                                message: "-H requires a duration".to_string(),
                                            }
                                        })?));
                                }
                            }
                            b'S' => {
                                if j + 1 < bytes.len() {
                                    result.stdin_timeout =
                                        Some(ArgValue::Owned(s[j + 1..].to_string()));
                                    break;
                                } else {
                                    i += 1;
                                    result.stdin_timeout =
                                        Some(ArgValue::Borrowed(args.get(i).ok_or_else(|| {
                                            ParseError {
                                                message: "-S requires a duration".to_string(),
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
    crate::io::print_str(
        "\nCopyright (c) 2025 Alexandre Bouveur\nLicense: MIT <https://opensource.org/licenses/MIT>\n",
    );
}

fn print_help() {
    crate::io::print_str(
        r#"Usage: timeout [OPTIONS] DURATION COMMAND [ARG]...

Enforce a strict wall-clock deadline on a command (sleep-aware).

Arguments:
  DURATION  Time before sending signal (30, 30s, 100ms, 500us, 1.5m, 2h, 1d)
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
  -c, --confine <MODE>            Time measurement mode: 'wall' (default, includes sleep) or
                                  'active' (excludes system sleep, faster, for benchmarks)
      --wait-for-file <PATH>      Wait for file to exist before starting command
                                  [env: TIMEOUT_WAIT_FOR_FILE]
      --wait-for-file-timeout <DUR>  Timeout for --wait-for-file (default: wait forever)
                                  [env: TIMEOUT_WAIT_FOR_FILE_TIMEOUT]
  -r, --retry <N>                 Retry command up to N times on timeout [env: TIMEOUT_RETRY]
      --retry-delay <DURATION>    Delay between retries [default: 0]
      --retry-backoff <Nx>        Multiply delay by N each retry (e.g., 2x for exponential)
  -H, --heartbeat <DURATION>      Print status to stderr at regular intervals (for CI)
                                  [env: TIMEOUT_HEARTBEAT]
  -S, --stdin-timeout <DURATION>  Kill command if stdin has no activity for DURATION
      --stdin-passthrough         Use non-consuming stdin idle detection (paired with -S)
                                  [env: TIMEOUT_STDIN_TIMEOUT]
      --json                      Output result as JSON (for scripting/CI)
  -h, --help                      Print help
  -V, --version                   Print version
      --mem-limit <BYTES>         Soft memory limit enforced via polling (e.g., 512M, 2G)
                                  Note: checked every 100ms; rapid spikes may escape detection
      --cpu-time <DURATION>       Set RLIMIT_CPU (total CPU time) for the command
      --cpu-percent <PCT>         Throttle CPU to PCT via SIGSTOP/SIGCONT
                                  (100 = 1 core, 400 = 4 cores; low values may stutter)

Exit status:
  124 if COMMAND times out, and --preserve-status is not specified
  124 if --wait-for-file times out
  124 if --stdin-timeout triggers (stdin idle)
  125 if the timeout command itself fails
  126 if COMMAND is found but cannot be invoked
  127 if COMMAND cannot be found
  137 if COMMAND (or timeout itself) is sent SIGKILL (128+9)
  the exit status of COMMAND otherwise

Environment:
  TIMEOUT         Default duration if not specified on command line
  TIMEOUT_SIGNAL  Default signal (overridden by -s)
  TIMEOUT_KILL_AFTER  Default kill-after duration (overridden by -k)
  TIMEOUT_RETRY   Default retry count (overridden by -r/--retry)
  TIMEOUT_HEARTBEAT  Default heartbeat interval (overridden by -H/--heartbeat)
  TIMEOUT_STDIN_TIMEOUT  Default stdin idle timeout
  TIMEOUT_WAIT_FOR_FILE  Default file to wait for
  TIMEOUT_WAIT_FOR_FILE_TIMEOUT  Default timeout for wait-for-file
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
        assert_eq!(args.confine, Confine::Wall);
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

    #[test]
    fn test_confine_wall() {
        let args = try_parse_from(["timeout", "--confine=wall", "5s", "cmd"]).unwrap();
        assert_eq!(args.confine, Confine::Wall);
    }

    #[test]
    fn test_confine_active() {
        let args = try_parse_from(["timeout", "--confine=active", "5s", "cmd"]).unwrap();
        assert_eq!(args.confine, Confine::Active);
    }

    #[test]
    fn test_confine_default() {
        let args = try_parse_from(["timeout", "5s", "cmd"]).unwrap();
        assert_eq!(args.confine, Confine::Wall); // default
    }

    #[test]
    fn test_confine_invalid() {
        let result = try_parse_from(["timeout", "--confine=invalid", "5s", "cmd"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("invalid confine mode"));
    }

    #[test]
    fn test_confine_case_insensitive() {
        let args = try_parse_from(["timeout", "--confine=ACTIVE", "5s", "cmd"]).unwrap();
        assert_eq!(args.confine, Confine::Active);
    }

    #[test]
    fn test_confine_short_flag() {
        let args = try_parse_from(["timeout", "-c", "active", "5s", "cmd"]).unwrap();
        assert_eq!(args.confine, Confine::Active);
    }

    #[test]
    fn test_confine_short_flag_embedded() {
        let args = try_parse_from(["timeout", "-cwall", "5s", "cmd"]).unwrap();
        assert_eq!(args.confine, Confine::Wall);
    }

    #[test]
    fn test_wait_for_file() {
        let args =
            try_parse_from(["timeout", "--wait-for-file", "/tmp/ready", "5s", "cmd"]).unwrap();
        assert_eq!(args.wait_for_file, Some("/tmp/ready".to_string()));
        assert!(args.wait_for_file_timeout.is_none());
    }

    #[test]
    fn test_wait_for_file_equals_syntax() {
        let args = try_parse_from(["timeout", "--wait-for-file=/tmp/ready", "5s", "cmd"]).unwrap();
        assert_eq!(args.wait_for_file, Some("/tmp/ready".to_string()));
    }

    #[test]
    fn test_wait_for_file_with_timeout() {
        let args = try_parse_from([
            "timeout",
            "--wait-for-file",
            "/tmp/ready",
            "--wait-for-file-timeout",
            "30s",
            "5s",
            "cmd",
        ])
        .unwrap();
        assert_eq!(args.wait_for_file, Some("/tmp/ready".to_string()));
        assert_eq!(args.wait_for_file_timeout, Some("30s".to_string()));
    }

    #[test]
    fn test_wait_for_file_timeout_equals_syntax() {
        let args = try_parse_from([
            "timeout",
            "--wait-for-file=/tmp/ready",
            "--wait-for-file-timeout=1m",
            "5s",
            "cmd",
        ])
        .unwrap();
        assert_eq!(args.wait_for_file, Some("/tmp/ready".to_string()));
        assert_eq!(args.wait_for_file_timeout, Some("1m".to_string()));
    }

    #[test]
    fn test_wait_for_file_missing_path() {
        let result = try_parse_from(["timeout", "--wait-for-file"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("requires a path"));
    }

    #[test]
    fn test_wait_for_file_timeout_missing_duration() {
        let result = try_parse_from(["timeout", "--wait-for-file-timeout"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("requires a duration"));
    }

    /* retry argument tests */

    #[test]
    fn test_retry_short_flag() {
        let args = try_parse_from(["timeout", "-r", "3", "5s", "cmd"]).unwrap();
        assert_eq!(args.retry, Some("3".to_string()));
    }

    #[test]
    fn test_retry_long_flag() {
        let args = try_parse_from(["timeout", "--retry", "5", "5s", "cmd"]).unwrap();
        assert_eq!(args.retry, Some("5".to_string()));
    }

    #[test]
    fn test_retry_equals_syntax() {
        let args = try_parse_from(["timeout", "--retry=2", "5s", "cmd"]).unwrap();
        assert_eq!(args.retry, Some("2".to_string()));
    }

    #[test]
    fn test_retry_delay() {
        let args = try_parse_from([
            "timeout",
            "--retry",
            "3",
            "--retry-delay",
            "2s",
            "5s",
            "cmd",
        ])
        .unwrap();
        assert_eq!(args.retry, Some("3".to_string()));
        assert_eq!(args.retry_delay, Some("2s".to_string()));
    }

    #[test]
    fn test_retry_delay_equals_syntax() {
        let args =
            try_parse_from(["timeout", "--retry=3", "--retry-delay=500ms", "5s", "cmd"]).unwrap();
        assert_eq!(args.retry, Some("3".to_string()));
        assert_eq!(args.retry_delay, Some("500ms".to_string()));
    }

    #[test]
    fn test_retry_backoff() {
        let args = try_parse_from([
            "timeout",
            "--retry",
            "3",
            "--retry-delay",
            "1s",
            "--retry-backoff",
            "2x",
            "5s",
            "cmd",
        ])
        .unwrap();
        assert_eq!(args.retry, Some("3".to_string()));
        assert_eq!(args.retry_delay, Some("1s".to_string()));
        assert_eq!(args.retry_backoff, Some("2x".to_string()));
    }

    #[test]
    fn test_retry_backoff_equals_syntax() {
        let args = try_parse_from([
            "timeout",
            "--retry=5",
            "--retry-delay=100ms",
            "--retry-backoff=3x",
            "5s",
            "cmd",
        ])
        .unwrap();
        assert_eq!(args.retry, Some("5".to_string()));
        assert_eq!(args.retry_delay, Some("100ms".to_string()));
        assert_eq!(args.retry_backoff, Some("3x".to_string()));
    }

    #[test]
    fn test_retry_with_short_r() {
        let args =
            try_parse_from(["timeout", "-r", "2", "--retry-delay", "1s", "5s", "cmd"]).unwrap();
        assert_eq!(args.retry, Some("2".to_string()));
        assert_eq!(args.retry_delay, Some("1s".to_string()));
    }

    #[test]
    fn test_retry_missing_count() {
        let result = try_parse_from(["timeout", "--retry"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("requires a count"));
    }

    #[test]
    fn test_retry_delay_missing_duration() {
        let result = try_parse_from(["timeout", "--retry-delay"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("requires a duration"));
    }

    #[test]
    fn test_retry_backoff_missing_multiplier() {
        let result = try_parse_from(["timeout", "--retry-backoff"]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message
                .contains("requires a multiplier")
        );
    }

    #[test]
    fn test_retry_combined_with_other_flags() {
        let args = try_parse_from([
            "timeout",
            "-v",
            "--json",
            "--retry",
            "3",
            "--retry-delay",
            "1s",
            "-s",
            "INT",
            "5s",
            "cmd",
        ])
        .unwrap();
        assert!(args.verbose);
        assert!(args.json);
        assert_eq!(args.retry, Some("3".to_string()));
        assert_eq!(args.retry_delay, Some("1s".to_string()));
        assert_eq!(args.signal, "INT");
    }

    /* heartbeat argument tests */

    #[test]
    fn test_heartbeat_long_flag() {
        let args = try_parse_from(["timeout", "--heartbeat", "60s", "5s", "cmd"]).unwrap();
        assert_eq!(args.heartbeat, Some("60s".to_string()));
    }

    #[test]
    fn test_heartbeat_short_flag() {
        let args = try_parse_from(["timeout", "-H", "30s", "5s", "cmd"]).unwrap();
        assert_eq!(args.heartbeat, Some("30s".to_string()));
    }

    #[test]
    fn test_heartbeat_equals_syntax() {
        let args = try_parse_from(["timeout", "--heartbeat=1m", "5s", "cmd"]).unwrap();
        assert_eq!(args.heartbeat, Some("1m".to_string()));
    }

    #[test]
    fn test_heartbeat_missing_duration() {
        let result = try_parse_from(["timeout", "--heartbeat"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("requires a duration"));
    }

    #[test]
    fn test_heartbeat_short_flag_missing_duration() {
        let result = try_parse_from(["timeout", "-H"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("requires a duration"));
    }

    #[test]
    fn test_heartbeat_combined_with_other_flags() {
        let args =
            try_parse_from(["timeout", "-v", "--json", "--heartbeat", "60s", "5m", "cmd"]).unwrap();
        assert!(args.verbose);
        assert!(args.json);
        assert_eq!(args.heartbeat, Some("60s".to_string()));
        assert_eq!(args.duration, Some("5m".to_string()));
    }

    #[test]
    fn test_heartbeat_short_flag_embedded() {
        let args = try_parse_from(["timeout", "-H30s", "5s", "cmd"]).unwrap();
        assert_eq!(args.heartbeat, Some("30s".to_string()));
    }

    /* stdin timeout argument tests */

    #[test]
    fn test_stdin_timeout_long_flag() {
        let args = try_parse_from(["timeout", "--stdin-timeout", "30s", "5s", "cmd"]).unwrap();
        assert_eq!(args.stdin_timeout, Some("30s".to_string()));
    }

    #[test]
    fn test_stdin_passthrough_flag() {
        let args = try_parse_from([
            "timeout",
            "--stdin-timeout",
            "10s",
            "--stdin-passthrough",
            "5s",
            "cmd",
        ])
        .unwrap();

        assert!(args.stdin_passthrough);
        assert_eq!(args.stdin_timeout, Some("10s".to_string()));
    }

    #[test]
    fn test_stdin_timeout_equals_syntax() {
        let args = try_parse_from(["timeout", "--stdin-timeout=1m", "5s", "cmd"]).unwrap();
        assert_eq!(args.stdin_timeout, Some("1m".to_string()));
    }

    #[test]
    fn test_stdin_timeout_missing_duration() {
        let result = try_parse_from(["timeout", "--stdin-timeout"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("requires a duration"));
    }

    #[test]
    fn test_stdin_timeout_combined_with_other_flags() {
        let args = try_parse_from([
            "timeout",
            "-v",
            "--json",
            "--stdin-timeout",
            "30s",
            "5m",
            "cmd",
        ])
        .unwrap();
        assert!(args.verbose);
        assert!(args.json);
        assert_eq!(args.stdin_timeout, Some("30s".to_string()));
        assert_eq!(args.duration, Some("5m".to_string()));
    }

    #[test]
    fn test_stdin_timeout_with_main_timeout() {
        let args = try_parse_from(["timeout", "--stdin-timeout", "30s", "5m", "cmd"]).unwrap();
        assert_eq!(args.stdin_timeout, Some("30s".to_string()));
        assert_eq!(args.duration, Some("5m".to_string()));
    }

    #[test]
    fn test_stdin_timeout_short_flag() {
        let args = try_parse_from(["timeout", "-S", "30s", "5s", "cmd"]).unwrap();
        assert_eq!(args.stdin_timeout, Some("30s".to_string()));
    }

    #[test]
    fn test_stdin_timeout_short_flag_embedded() {
        let args = try_parse_from(["timeout", "-S30s", "5s", "cmd"]).unwrap();
        assert_eq!(args.stdin_timeout, Some("30s".to_string()));
    }

    #[test]
    fn test_stdin_timeout_short_flag_missing_duration() {
        let result = try_parse_from(["timeout", "-S"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("requires a duration"));
    }
}
