/*
 * build.rs
 *
 * Build script for procguard.
 * Ensures libc is linked for the no_std binary.
 */

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    /* darwin-only link flags */
    if target_os == "macos" {
        // Link with system libc - required because we use #![no_std]
        // which causes rustc to pass -nodefaultlibs to the linker.
        // Without this, symbols like malloc, write, etc. are undefined.
        println!("cargo:rustc-link-lib=c");
        println!("cargo:rustc-link-lib=System");
    }
}
