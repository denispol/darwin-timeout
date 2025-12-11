/*
 * fuzz_targets/parse_args.rs
 *
 * fuzz target for CLI argument parsing. validates that parse_from_slice
 * never panics on arbitrary argument combinations.
 *
 * edge cases: "-pfv", "--unknown", "-s" (missing value), very long args,
 * conflicting flags (-v -q), embedded values (-sTERM)
 */

#![no_main]

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    /* split input on null bytes to simulate multiple arguments */
    let args: Vec<String> = data
        .split(|&b| b == 0)
        .filter_map(|chunk| core::str::from_utf8(chunk).ok())
        .filter(|s| !s.is_empty()) /* filter out empty strings from multiple nulls */
        .map(String::from)
        .collect();

    /* skip if any arg is exactly -V, --version, -h, or --help.
     * these call exit(0) which fuzzer treats as crash. this is expected behavior.
     * note: malformed clusters like -V--i2 are NOT skipped - they should error. */
    for arg in &args {
        if arg == "-V" || arg == "--version" || arg == "-h" || arg == "--help" {
            return;
        }
    }

    /* parse_from_slice must not panic on any argument combination */
    let _ = darwin_timeout::args::parse_from_slice(&args);
});
