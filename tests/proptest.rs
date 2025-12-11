/*
 * proptest.rs
 *
 * property-based tests for parsing functions.
 * generates thousands of inputs to find edge cases.
 */

use proptest::prelude::*;
use std::time::Duration;

use darwin_timeout::duration::parse_duration;
use darwin_timeout::rlimit::parse_mem_limit;
use darwin_timeout::signal::{Signal, parse_signal, signal_name};

/* ============================================================================
 * Duration Parsing Properties
 * ============================================================================ */

/* valid duration strings always parse successfully */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn duration_valid_seconds_parse(secs in 0u64..1_000_000) {
        let s = format!("{}s", secs);
        let d = parse_duration(&s).expect("valid seconds should parse");
        prop_assert_eq!(d.as_secs(), secs);
    }

    #[test]
    fn duration_valid_minutes_parse(mins in 0u64..10_000) {
        let s = format!("{}m", mins);
        let d = parse_duration(&s).expect("valid minutes should parse");
        prop_assert_eq!(d.as_secs(), mins * 60);
    }

    #[test]
    fn duration_valid_hours_parse(hours in 0u64..1000) {
        let s = format!("{}h", hours);
        let d = parse_duration(&s).expect("valid hours should parse");
        prop_assert_eq!(d.as_secs(), hours * 3600);
    }

    #[test]
    fn duration_valid_days_parse(days in 0u64..100) {
        let s = format!("{}d", days);
        let d = parse_duration(&s).expect("valid days should parse");
        prop_assert_eq!(d.as_secs(), days * 86400);
    }

    #[test]
    fn duration_valid_milliseconds_parse(ms in 0u64..1_000_000) {
        let s = format!("{}ms", ms);
        let d = parse_duration(&s).expect("valid milliseconds should parse");
        prop_assert_eq!(d, Duration::from_millis(ms));
    }

    #[test]
    fn duration_valid_microseconds_parse(us in 0u64..1_000_000) {
        let s = format!("{}us", us);
        let d = parse_duration(&s).expect("valid microseconds should parse");
        prop_assert_eq!(d, Duration::from_micros(us));
    }
}

/* duration ordering: if a > b numerically, then parse(a) >= parse(b) */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn duration_ordering_preserved(a in 0u64..100_000, b in 0u64..100_000) {
        let da = parse_duration(&format!("{}s", a)).unwrap();
        let db = parse_duration(&format!("{}s", b)).unwrap();
        if a > b {
            prop_assert!(da >= db);
        } else if a < b {
            prop_assert!(da <= db);
        } else {
            prop_assert_eq!(da, db);
        }
    }
}

/* fractional durations: 1.5s = 1500ms */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn duration_fractional_equivalence(whole in 0u32..1000, frac in 0u32..10) {
        /* X.Ys should equal X*1000 + Y*100 milliseconds */
        let s = format!("{}.{}s", whole, frac);
        let d = parse_duration(&s).expect("fractional should parse");
        let expected_ms = (whole as u64) * 1000 + (frac as u64) * 100;
        prop_assert_eq!(d.as_millis() as u64, expected_ms);
    }
}

/* whitespace handling */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn duration_whitespace_ignored(secs in 1u64..1000, spaces in 0usize..5) {
        let prefix: String = " ".repeat(spaces);
        let suffix: String = " ".repeat(spaces);
        let s = format!("{}{}s{}", prefix, secs, suffix);
        let d = parse_duration(&s).expect("whitespace should be trimmed");
        prop_assert_eq!(d.as_secs(), secs);
    }
}

/* case insensitivity */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn duration_case_insensitive(secs in 1u64..1000, upper in prop::bool::ANY) {
        let suffix = if upper { "S" } else { "s" };
        let s = format!("{}{}", secs, suffix);
        let d = parse_duration(&s).expect("case should be ignored");
        prop_assert_eq!(d.as_secs(), secs);
    }
}

/* no suffix = seconds */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn duration_no_suffix_means_seconds(secs in 0u64..1_000_000) {
        let d = parse_duration(&secs.to_string()).expect("no suffix should parse");
        prop_assert_eq!(d.as_secs(), secs);
    }
}

/* invalid suffixes always error */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn duration_invalid_suffix_errors(
        num in 1u64..1000,
        suffix in "[a-z&&[^smhd]]{1,3}"  // letters except valid suffixes
    ) {
        /* skip "us" and "ms" which are valid */
        prop_assume!(suffix != "us" && suffix != "ms");
        let s = format!("{}{}", num, suffix);
        prop_assert!(parse_duration(&s).is_err());
    }
}

/* negative always errors */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn duration_negative_errors(secs in 1i64..1000) {
        let s = format!("-{}", secs);
        prop_assert!(parse_duration(&s).is_err());
    }
}

/* ============================================================================
 * Signal Parsing Properties
 * ============================================================================ */

/* all valid signal numbers from the Signal enum parse and roundtrip */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn signal_valid_numbers_parse(num in prop::sample::select(vec![
        /* only signals defined in the Signal enum (darwin/POSIX subset) */
        1,  /* SIGHUP */
        2,  /* SIGINT */
        3,  /* SIGQUIT */
        4,  /* SIGILL */
        5,  /* SIGTRAP */
        6,  /* SIGABRT */
        8,  /* SIGFPE */
        9,  /* SIGKILL */
        10, /* SIGBUS */
        11, /* SIGSEGV */
        12, /* SIGSYS */
        13, /* SIGPIPE */
        14, /* SIGALRM */
        15, /* SIGTERM */
        /* skipping 16: SIGURG defined in enum but let's check */
        16, /* SIGURG */
        17, /* SIGSTOP */
        18, /* SIGTSTP */
        19, /* SIGCONT */
        20, /* SIGCHLD */
        21, /* SIGTTIN */
        22, /* SIGTTOU */
        23, /* SIGIO */
        24, /* SIGXCPU */
        25, /* SIGXFSZ */
        26, /* SIGVTALRM */
        27, /* SIGPROF */
        28, /* SIGWINCH */
        30, /* SIGUSR1 */
        31, /* SIGUSR2 */
    ])) {
        let sig = parse_signal(&num.to_string()).expect("valid signal number");
        prop_assert_eq!(sig.as_raw(), num);
    }
}

/* signal names roundtrip through signal_name */
#[test]
fn signal_name_roundtrip() {
    let signals = [
        Signal::SIGHUP,
        Signal::SIGINT,
        Signal::SIGQUIT,
        Signal::SIGILL,
        Signal::SIGTRAP,
        Signal::SIGABRT,
        Signal::SIGBUS,
        Signal::SIGFPE,
        Signal::SIGKILL,
        Signal::SIGUSR1,
        Signal::SIGSEGV,
        Signal::SIGUSR2,
        Signal::SIGPIPE,
        Signal::SIGALRM,
        Signal::SIGTERM,
        Signal::SIGCHLD,
        Signal::SIGCONT,
        Signal::SIGSTOP,
        Signal::SIGTSTP,
        Signal::SIGTTIN,
        Signal::SIGTTOU,
        Signal::SIGURG,
        Signal::SIGXCPU,
        Signal::SIGXFSZ,
        Signal::SIGVTALRM,
        Signal::SIGPROF,
        Signal::SIGWINCH,
        Signal::SIGIO,
        Signal::SIGSYS,
    ];

    for sig in signals {
        let name = signal_name(sig);
        let parsed = parse_signal(name).expect("signal_name output should parse");
        assert_eq!(parsed, sig);
    }
}

/* case insensitivity for signal names */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn signal_case_insensitive(name in prop::sample::select(vec![
        "term", "TERM", "Term", "TErm",
        "kill", "KILL", "Kill", "KiLl",
        "hup", "HUP", "Hup",
        "int", "INT", "Int",
    ])) {
        prop_assert!(parse_signal(name).is_ok());
    }
}

/* SIG prefix optional */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn signal_sig_prefix_optional(base in prop::sample::select(vec![
        "TERM", "KILL", "HUP", "INT", "USR1", "USR2"
    ])) {
        let with_prefix = format!("SIG{}", base);
        let without = parse_signal(base).expect("without prefix");
        let with = parse_signal(&with_prefix).expect("with prefix");
        prop_assert_eq!(without, with);
    }
}

/* invalid signal numbers error */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn signal_invalid_numbers_error(num in 32i32..1000) {
        prop_assert!(parse_signal(&num.to_string()).is_err());
    }

    #[test]
    fn signal_zero_errors(_dummy in 0..1) {
        prop_assert!(parse_signal("0").is_err());
    }

    #[test]
    fn signal_negative_errors(num in -1000i32..-1) {
        prop_assert!(parse_signal(&num.to_string()).is_err());
    }
}

/* ============================================================================
 * Memory Limit Parsing Properties
 * ============================================================================ */

/* valid memory sizes parse */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn mem_limit_bytes_parse(bytes in 0u64..1_000_000) {
        let m = parse_mem_limit(&bytes.to_string()).expect("bytes should parse");
        prop_assert_eq!(m, bytes);
    }

    #[test]
    fn mem_limit_kilobytes_parse(kb in 0u64..1_000_000) {
        let s = format!("{}K", kb);
        let m = parse_mem_limit(&s).expect("kilobytes should parse");
        prop_assert_eq!(m, kb * 1024);
    }

    #[test]
    fn mem_limit_megabytes_parse(mb in 0u64..100_000) {
        let s = format!("{}M", mb);
        let m = parse_mem_limit(&s).expect("megabytes should parse");
        prop_assert_eq!(m, mb * 1024 * 1024);
    }

    #[test]
    fn mem_limit_gigabytes_parse(gb in 0u64..1000) {
        let s = format!("{}G", gb);
        let m = parse_mem_limit(&s).expect("gigabytes should parse");
        prop_assert_eq!(m, gb * 1024 * 1024 * 1024);
    }
}

/* case insensitivity for memory suffixes */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn mem_limit_case_insensitive(
        val in 1u64..1000,
        suffix in prop::sample::select(vec!["k", "K", "kb", "KB", "Kb", "m", "M", "mb", "MB", "g", "G", "gb", "GB"])
    ) {
        let s = format!("{}{}", val, suffix);
        prop_assert!(parse_mem_limit(&s).is_ok());
    }
}

/* invalid suffixes error */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn mem_limit_invalid_suffix_errors(
        num in 1u64..1000,
        suffix in prop::sample::select(vec!["x", "X", "p", "P", "kb2", "mm"])
    ) {
        let s = format!("{}{}", num, suffix);
        prop_assert!(parse_mem_limit(&s).is_err());
    }
}

/* overflow detection */
#[test]
fn mem_limit_overflow_detection() {
    /* 16EB would overflow u64 */
    assert!(parse_mem_limit("18446744073709551615").is_ok()); // u64::MAX bytes
    assert!(parse_mem_limit("18446744073709551616").is_err()); // u64::MAX + 1
    assert!(parse_mem_limit("999999999999T").is_err()); // massive terabytes
}

/* whitespace handling */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn mem_limit_whitespace_ignored(val in 1u64..1000, spaces in 0usize..3) {
        let prefix: String = " ".repeat(spaces);
        let suffix_spaces: String = " ".repeat(spaces);
        let s = format!("{}{}M{}", prefix, val, suffix_spaces);
        let m = parse_mem_limit(&s).expect("whitespace should be trimmed");
        prop_assert_eq!(m, val * 1024 * 1024);
    }
}

/* ============================================================================
 * Cross-Module Properties
 * ============================================================================ */

/* parsing empty strings always errors */
#[test]
fn empty_strings_error() {
    assert!(parse_duration("").is_err());
    assert!(parse_duration("   ").is_err());
    assert!(parse_mem_limit("").is_err());
    assert!(parse_mem_limit("   ").is_err());
    /* parse_signal trims then errors on empty */
    assert!(parse_signal("").is_err());
}

/* parsing non-numeric garbage errors */
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn garbage_input_errors(s in "[^0-9]{1,10}") {
        /* filter out valid signal names */
        prop_assume!(!matches!(
            s.to_uppercase().as_str(),
            "TERM" | "KILL" | "HUP" | "INT" | "QUIT" | "ABRT" | "IOT" |
            "USR" | "SEGV" | "PIPE" | "ALRM" | "CONT" | "STOP" | "TSTP" |
            "CHLD" | "BUS" | "FPE" | "ILL" | "TRAP" | "TTIN" | "TTOU" |
            "URG" | "XCPU" | "XFSZ" | "VTALRM" | "PROF" | "WINCH" | "IO" | "SYS" |
            "S" | "M" | "H" | "D" | "MS" | "US" | "K" | "KB" | "MB" | "G" | "GB" | "T" | "TB" | "B"
        ));
        prop_assert!(parse_duration(&s).is_err());
        prop_assert!(parse_mem_limit(&s).is_err());
    }
}
