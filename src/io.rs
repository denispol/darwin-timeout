/*
 * io.rs
 *
 * no_std I/O primitives.
 * direct writes to stdout/stderr via libc::write.
 *
 * no buffering - each write is a syscall. that's fine for CLI output
 * which is typically a single line at a time.
 */

use core::fmt::{self, Write};

/* file descriptors */
const STDOUT: i32 = 1;
const STDERR: i32 = 2;

unsafe extern "C" {
    fn write(fd: i32, buf: *const u8, count: usize) -> isize;
}

/// Write bytes to stdout
#[inline]
pub fn write_stdout(s: &[u8]) {
    // SAFETY: s is a valid byte slice, STDOUT is always valid
    unsafe {
        write(STDOUT, s.as_ptr(), s.len());
    }
}

/// Write bytes to stderr
#[inline]
pub fn write_stderr(s: &[u8]) {
    // SAFETY: s is a valid byte slice, STDERR is always valid
    unsafe {
        write(STDERR, s.as_ptr(), s.len());
    }
}

/// Write a string to stdout
#[inline]
pub fn print_str(s: &str) {
    write_stdout(s.as_bytes());
}

/// Write a string to stderr
#[inline]
pub fn eprint_str(s: &str) {
    write_stderr(s.as_bytes());
}

/// A writer that outputs to stderr via direct syscall.
/// Implements core::fmt::Write for use with write!/writeln! macros.
pub struct StderrWriter;

impl Write for StderrWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_stderr(s.as_bytes());
        Ok(())
    }
}

/// A writer that outputs to stdout via direct syscall.
pub struct StdoutWriter;

impl Write for StdoutWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_stdout(s.as_bytes());
        Ok(())
    }
}

/// Print to stderr (no newline)
#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::io::StderrWriter, $($arg)*);
    }};
}

/// Print to stderr with newline
#[macro_export]
macro_rules! eprintln {
    () => {{
        $crate::io::write_stderr(b"\n");
    }};
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::io::StderrWriter, $($arg)*);
        $crate::io::write_stderr(b"\n");
    }};
}

/// Print to stdout (no newline)
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::io::StdoutWriter, $($arg)*);
    }};
}

/// Print to stdout with newline
#[macro_export]
macro_rules! println {
    () => {{
        $crate::io::write_stdout(b"\n");
    }};
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::io::StdoutWriter, $($arg)*);
        $crate::io::write_stdout(b"\n");
    }};
}

/// Format to String (re-export of alloc::format! for convenience)
#[macro_export]
macro_rules! format {
    ($($arg:tt)*) => {{
        alloc::format!($($arg)*)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_stderr() {
        /* just verify it doesn't crash */
        write_stderr(b"test stderr write\n");
    }

    #[test]
    fn test_write_stdout() {
        write_stdout(b"test stdout write\n");
    }

    #[test]
    fn test_writer_fmt() {
        use core::fmt::Write;
        let mut w = StderrWriter;
        let _ = write!(w, "formatted: {} + {} = {}", 1, 2, 3);
    }
}
