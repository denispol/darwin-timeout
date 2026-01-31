/*
 * fuzz_targets/parse_duration.rs
 *
 * fuzz target for duration parsing. validates that parse_duration never panics
 * on arbitrary input, only returns Ok or Err gracefully.
 *
 * edge cases: "", "999999999h", "-1", "1.2.3s", unicode, huge numbers
 */

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    /* convert to str - invalid UTF-8 should be handled gracefully */
    if let Ok(s) = core::str::from_utf8(data) {
        /* parse_duration must not panic on any valid UTF-8 string */
        let _ = procguard::duration::parse_duration(s);
    }
});
