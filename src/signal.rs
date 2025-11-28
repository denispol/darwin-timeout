/*
 * signal.rs
 *
 * Parse "TERM", "SIGTERM", "term", "15". Reject "SIGFOO", "999".
 *
 * Big match statement instead of nix's from_str because we want "IOT"
 * to work (alias for ABRT), case insensitivity, optional SIG prefix.
 * TERM and KILL first since that's 99% of usage.
 */

use nix::sys::signal::Signal;

use crate::error::{Result, TimeoutError};

/// Parse "TERM", "SIGKILL", "9", "hup" - all the ways to specify a signal.
///
/// # Examples
///
/// ```
/// use darwin_timeout::signal::parse_signal;
/// use nix::sys::signal::Signal;
///
/// assert_eq!(parse_signal("TERM").unwrap(), Signal::SIGTERM);
/// assert_eq!(parse_signal("SIGTERM").unwrap(), Signal::SIGTERM);
/// assert_eq!(parse_signal("term").unwrap(), Signal::SIGTERM);
/// assert_eq!(parse_signal("15").unwrap(), Signal::SIGTERM);
/// assert_eq!(parse_signal("9").unwrap(), Signal::SIGKILL);
/// ```
pub fn parse_signal(input: &str) -> Result<Signal> {
    let input = input.trim();

    /* try as number first */
    if let Ok(num) = input.parse::<i32>() {
        return Signal::try_from(num)
            .map_err(|_| TimeoutError::InvalidSignal(format!("invalid signal number: {num}")));
    }

    /* Normalize to uppercase, strip optional SIG prefix */
    let name = input.to_ascii_uppercase();
    let name = name.strip_prefix("SIG").unwrap_or(&name);

    /* all the POSIX signals, ordered by how often people use them */
    match name {
        "TERM" => Ok(Signal::SIGTERM),
        "KILL" => Ok(Signal::SIGKILL),
        "INT" => Ok(Signal::SIGINT),
        "HUP" => Ok(Signal::SIGHUP),
        "QUIT" => Ok(Signal::SIGQUIT),
        "ABRT" | "IOT" => Ok(Signal::SIGABRT),
        "USR1" => Ok(Signal::SIGUSR1),
        "USR2" => Ok(Signal::SIGUSR2),
        "ALRM" => Ok(Signal::SIGALRM),
        "CONT" => Ok(Signal::SIGCONT),
        "STOP" => Ok(Signal::SIGSTOP),
        "TSTP" => Ok(Signal::SIGTSTP),
        "PIPE" => Ok(Signal::SIGPIPE),
        "CHLD" => Ok(Signal::SIGCHLD),
        "SEGV" => Ok(Signal::SIGSEGV),
        "BUS" => Ok(Signal::SIGBUS),
        "FPE" => Ok(Signal::SIGFPE),
        "ILL" => Ok(Signal::SIGILL),
        "TRAP" => Ok(Signal::SIGTRAP),
        "TTIN" => Ok(Signal::SIGTTIN),
        "TTOU" => Ok(Signal::SIGTTOU),
        "URG" => Ok(Signal::SIGURG),
        "XCPU" => Ok(Signal::SIGXCPU),
        "XFSZ" => Ok(Signal::SIGXFSZ),
        "VTALRM" => Ok(Signal::SIGVTALRM),
        "PROF" => Ok(Signal::SIGPROF),
        "WINCH" => Ok(Signal::SIGWINCH),
        "IO" => Ok(Signal::SIGIO),
        "SYS" => Ok(Signal::SIGSYS),
        _ => Err(TimeoutError::InvalidSignal(format!(
            "unknown signal: {input}"
        ))),
    }
}

/* signal number for exit code (128 + signum) */
#[must_use]
pub const fn signal_number(signal: Signal) -> i32 {
    signal as i32
}

/* human-readable name for verbose output */
#[must_use]
pub const fn signal_name(signal: Signal) -> &'static str {
    match signal {
        Signal::SIGHUP => "SIGHUP",
        Signal::SIGINT => "SIGINT",
        Signal::SIGQUIT => "SIGQUIT",
        Signal::SIGILL => "SIGILL",
        Signal::SIGTRAP => "SIGTRAP",
        Signal::SIGABRT => "SIGABRT",
        Signal::SIGBUS => "SIGBUS",
        Signal::SIGFPE => "SIGFPE",
        Signal::SIGKILL => "SIGKILL",
        Signal::SIGUSR1 => "SIGUSR1",
        Signal::SIGSEGV => "SIGSEGV",
        Signal::SIGUSR2 => "SIGUSR2",
        Signal::SIGPIPE => "SIGPIPE",
        Signal::SIGALRM => "SIGALRM",
        Signal::SIGTERM => "SIGTERM",
        Signal::SIGCHLD => "SIGCHLD",
        Signal::SIGCONT => "SIGCONT",
        Signal::SIGSTOP => "SIGSTOP",
        Signal::SIGTSTP => "SIGTSTP",
        Signal::SIGTTIN => "SIGTTIN",
        Signal::SIGTTOU => "SIGTTOU",
        Signal::SIGURG => "SIGURG",
        Signal::SIGXCPU => "SIGXCPU",
        Signal::SIGXFSZ => "SIGXFSZ",
        Signal::SIGVTALRM => "SIGVTALRM",
        Signal::SIGPROF => "SIGPROF",
        Signal::SIGWINCH => "SIGWINCH",
        Signal::SIGIO => "SIGIO",
        Signal::SIGSYS => "SIGSYS",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_by_name() {
        assert_eq!(parse_signal("TERM").unwrap(), Signal::SIGTERM);
        assert_eq!(parse_signal("KILL").unwrap(), Signal::SIGKILL);
        assert_eq!(parse_signal("HUP").unwrap(), Signal::SIGHUP);
        assert_eq!(parse_signal("INT").unwrap(), Signal::SIGINT);
        assert_eq!(parse_signal("USR1").unwrap(), Signal::SIGUSR1);
        assert_eq!(parse_signal("USR2").unwrap(), Signal::SIGUSR2);
    }

    #[test]
    fn test_parse_with_sig_prefix() {
        assert_eq!(parse_signal("SIGTERM").unwrap(), Signal::SIGTERM);
        assert_eq!(parse_signal("SIGKILL").unwrap(), Signal::SIGKILL);
        assert_eq!(parse_signal("SIGHUP").unwrap(), Signal::SIGHUP);
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(parse_signal("term").unwrap(), Signal::SIGTERM);
        assert_eq!(parse_signal("Term").unwrap(), Signal::SIGTERM);
        assert_eq!(parse_signal("sigterm").unwrap(), Signal::SIGTERM);
        assert_eq!(parse_signal("SigTerm").unwrap(), Signal::SIGTERM);
    }

    #[test]
    fn test_parse_by_number() {
        assert_eq!(parse_signal("15").unwrap(), Signal::SIGTERM);
        assert_eq!(parse_signal("9").unwrap(), Signal::SIGKILL);
        assert_eq!(parse_signal("1").unwrap(), Signal::SIGHUP);
        assert_eq!(parse_signal("2").unwrap(), Signal::SIGINT);
    }

    #[test]
    fn test_parse_whitespace() {
        assert_eq!(parse_signal("  TERM  ").unwrap(), Signal::SIGTERM);
        assert_eq!(parse_signal("  15  ").unwrap(), Signal::SIGTERM);
    }

    #[test]
    fn test_invalid_name() {
        assert!(parse_signal("INVALID").is_err());
        assert!(parse_signal("SIGFOO").is_err());
    }

    #[test]
    fn test_invalid_number() {
        assert!(parse_signal("0").is_err());
        assert!(parse_signal("999").is_err());
        assert!(parse_signal("-1").is_err());
    }

    #[test]
    fn test_signal_number() {
        assert_eq!(signal_number(Signal::SIGTERM), 15);
        assert_eq!(signal_number(Signal::SIGKILL), 9);
        assert_eq!(signal_number(Signal::SIGHUP), 1);
    }

    #[test]
    fn test_signal_name() {
        assert_eq!(signal_name(Signal::SIGTERM), "SIGTERM");
        assert_eq!(signal_name(Signal::SIGKILL), "SIGKILL");
    }
}
