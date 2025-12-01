/*
 * signal.rs
 *
 * Parse "TERM", "SIGTERM", "term", "15". Reject "SIGFOO", "999".
 *
 * Big match statement because we want "IOT" to work (alias for ABRT),
 * case insensitivity, optional SIG prefix. TERM and KILL first since
 * that's 99% of usage.
 *
 * Local Signal enum with libc constants - no nix dependency.
 */

use crate::error::{Result, TimeoutError};
use alloc::format;

/* POSIX signals as i32 values from libc. Copy/PartialEq for easy comparison. */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Signal {
    SIGHUP = libc::SIGHUP,
    SIGINT = libc::SIGINT,
    SIGQUIT = libc::SIGQUIT,
    SIGILL = libc::SIGILL,
    SIGTRAP = libc::SIGTRAP,
    SIGABRT = libc::SIGABRT,
    SIGBUS = libc::SIGBUS,
    SIGFPE = libc::SIGFPE,
    SIGKILL = libc::SIGKILL,
    SIGUSR1 = libc::SIGUSR1,
    SIGSEGV = libc::SIGSEGV,
    SIGUSR2 = libc::SIGUSR2,
    SIGPIPE = libc::SIGPIPE,
    SIGALRM = libc::SIGALRM,
    SIGTERM = libc::SIGTERM,
    SIGCHLD = libc::SIGCHLD,
    SIGCONT = libc::SIGCONT,
    SIGSTOP = libc::SIGSTOP,
    SIGTSTP = libc::SIGTSTP,
    SIGTTIN = libc::SIGTTIN,
    SIGTTOU = libc::SIGTTOU,
    SIGURG = libc::SIGURG,
    SIGXCPU = libc::SIGXCPU,
    SIGXFSZ = libc::SIGXFSZ,
    SIGVTALRM = libc::SIGVTALRM,
    SIGPROF = libc::SIGPROF,
    SIGWINCH = libc::SIGWINCH,
    SIGIO = libc::SIGIO,
    SIGSYS = libc::SIGSYS,
}

impl Signal {
    /* convert from raw signal number */
    pub fn try_from_raw(num: i32) -> Option<Self> {
        match num {
            libc::SIGHUP => Some(Self::SIGHUP),
            libc::SIGINT => Some(Self::SIGINT),
            libc::SIGQUIT => Some(Self::SIGQUIT),
            libc::SIGILL => Some(Self::SIGILL),
            libc::SIGTRAP => Some(Self::SIGTRAP),
            libc::SIGABRT => Some(Self::SIGABRT),
            libc::SIGBUS => Some(Self::SIGBUS),
            libc::SIGFPE => Some(Self::SIGFPE),
            libc::SIGKILL => Some(Self::SIGKILL),
            libc::SIGUSR1 => Some(Self::SIGUSR1),
            libc::SIGSEGV => Some(Self::SIGSEGV),
            libc::SIGUSR2 => Some(Self::SIGUSR2),
            libc::SIGPIPE => Some(Self::SIGPIPE),
            libc::SIGALRM => Some(Self::SIGALRM),
            libc::SIGTERM => Some(Self::SIGTERM),
            libc::SIGCHLD => Some(Self::SIGCHLD),
            libc::SIGCONT => Some(Self::SIGCONT),
            libc::SIGSTOP => Some(Self::SIGSTOP),
            libc::SIGTSTP => Some(Self::SIGTSTP),
            libc::SIGTTIN => Some(Self::SIGTTIN),
            libc::SIGTTOU => Some(Self::SIGTTOU),
            libc::SIGURG => Some(Self::SIGURG),
            libc::SIGXCPU => Some(Self::SIGXCPU),
            libc::SIGXFSZ => Some(Self::SIGXFSZ),
            libc::SIGVTALRM => Some(Self::SIGVTALRM),
            libc::SIGPROF => Some(Self::SIGPROF),
            libc::SIGWINCH => Some(Self::SIGWINCH),
            libc::SIGIO => Some(Self::SIGIO),
            libc::SIGSYS => Some(Self::SIGSYS),
            _ => None,
        }
    }

    /* get raw signal number */
    #[inline]
    pub const fn as_raw(self) -> i32 {
        self as i32
    }
}

/// Parse "TERM", "SIGKILL", "9", "hup" - all the ways to specify a signal.
///
/// # Examples
///
/// ```
/// use darwin_timeout::signal::{parse_signal, Signal};
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
        return Signal::try_from_raw(num)
            .ok_or_else(|| TimeoutError::InvalidSignal(format!("invalid signal number: {num}")));
    }

    /* strip optional SIG prefix without allocation */
    let name = input
        .strip_prefix("SIG")
        .or_else(|| input.strip_prefix("sig"))
        .or_else(|| input.strip_prefix("Sig"))
        .unwrap_or(input);

    /* case-insensitive comparison without heap allocation */
    /* ordered by frequency: TERM and KILL cover 99% of usage */
    if name.eq_ignore_ascii_case("TERM") {
        Ok(Signal::SIGTERM)
    } else if name.eq_ignore_ascii_case("KILL") {
        Ok(Signal::SIGKILL)
    } else if name.eq_ignore_ascii_case("INT") {
        Ok(Signal::SIGINT)
    } else if name.eq_ignore_ascii_case("HUP") {
        Ok(Signal::SIGHUP)
    } else if name.eq_ignore_ascii_case("QUIT") {
        Ok(Signal::SIGQUIT)
    } else if name.eq_ignore_ascii_case("ABRT") || name.eq_ignore_ascii_case("IOT") {
        Ok(Signal::SIGABRT)
    } else if name.eq_ignore_ascii_case("USR1") {
        Ok(Signal::SIGUSR1)
    } else if name.eq_ignore_ascii_case("USR2") {
        Ok(Signal::SIGUSR2)
    } else if name.eq_ignore_ascii_case("ALRM") {
        Ok(Signal::SIGALRM)
    } else if name.eq_ignore_ascii_case("CONT") {
        Ok(Signal::SIGCONT)
    } else if name.eq_ignore_ascii_case("STOP") {
        Ok(Signal::SIGSTOP)
    } else if name.eq_ignore_ascii_case("TSTP") {
        Ok(Signal::SIGTSTP)
    } else if name.eq_ignore_ascii_case("PIPE") {
        Ok(Signal::SIGPIPE)
    } else if name.eq_ignore_ascii_case("CHLD") {
        Ok(Signal::SIGCHLD)
    } else if name.eq_ignore_ascii_case("SEGV") {
        Ok(Signal::SIGSEGV)
    } else if name.eq_ignore_ascii_case("BUS") {
        Ok(Signal::SIGBUS)
    } else if name.eq_ignore_ascii_case("FPE") {
        Ok(Signal::SIGFPE)
    } else if name.eq_ignore_ascii_case("ILL") {
        Ok(Signal::SIGILL)
    } else if name.eq_ignore_ascii_case("TRAP") {
        Ok(Signal::SIGTRAP)
    } else if name.eq_ignore_ascii_case("TTIN") {
        Ok(Signal::SIGTTIN)
    } else if name.eq_ignore_ascii_case("TTOU") {
        Ok(Signal::SIGTTOU)
    } else if name.eq_ignore_ascii_case("URG") {
        Ok(Signal::SIGURG)
    } else if name.eq_ignore_ascii_case("XCPU") {
        Ok(Signal::SIGXCPU)
    } else if name.eq_ignore_ascii_case("XFSZ") {
        Ok(Signal::SIGXFSZ)
    } else if name.eq_ignore_ascii_case("VTALRM") {
        Ok(Signal::SIGVTALRM)
    } else if name.eq_ignore_ascii_case("PROF") {
        Ok(Signal::SIGPROF)
    } else if name.eq_ignore_ascii_case("WINCH") {
        Ok(Signal::SIGWINCH)
    } else if name.eq_ignore_ascii_case("IO") {
        Ok(Signal::SIGIO)
    } else if name.eq_ignore_ascii_case("SYS") {
        Ok(Signal::SIGSYS)
    } else {
        Err(TimeoutError::InvalidSignal(format!(
            "unknown signal: {input}"
        )))
    }
}

/* signal number for exit code (128 + signum) */
#[must_use]
#[inline]
pub const fn signal_number(signal: Signal) -> i32 {
    signal.as_raw()
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
