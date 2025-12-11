/*
 * fuzz_targets/parse_mem_limit.rs
 *
 * fuzz target for memory limit parsing. validates that parse_mem_limit
 * never panics on arbitrary input strings.
 *
 * edge cases: "", "0", "1G", "99999999T", "-1", "abc", "512mb", "1.5G"
 */

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = core::str::from_utf8(data) {
        /* parse_mem_limit must not panic on any valid UTF-8 string */
        let _ = darwin_timeout::rlimit::parse_mem_limit(s);
    }
});
