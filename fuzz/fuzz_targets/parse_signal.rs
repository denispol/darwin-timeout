/*
 * fuzz_targets/parse_signal.rs
 *
 * fuzz target for signal parsing. validates that parse_signal never panics
 * on arbitrary input strings representing signal names or numbers.
 *
 * edge cases: "SIGFOO", "999", "-1", "term", "Term", "  TERM  ", ""
 */

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = core::str::from_utf8(data) {
        /* parse_signal must not panic on any valid UTF-8 string */
        let _ = procguard::signal::parse_signal(s);
    }
});
