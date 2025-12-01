/*
 * panic.rs
 *
 * Minimal panic handler for no_std binary.
 * With panic=abort in Cargo.toml, panics go straight to abort().
 * This handler exists to satisfy the compiler.
 *
 * Only used in release builds without tests. Debug builds and tests use std's
 * panic handler for better error messages.
 */

#[cfg(not(any(debug_assertions, test, doc)))]
use core::panic::PanicInfo;
/// Panic handler - just abort immediately.
///
/// With panic=abort, the compiler generates code that calls abort()
/// directly on panic, so this handler is mostly dead code. But we
/// need it to link.
///
/// # Safety Considerations
///
/// This handler intentionally provides no diagnostic output because:
/// 1. Formatting requires allocation, which may have caused the panic
/// 2. Writing to stderr requires syscalls that may fail
/// 3. With panic=abort, the goal is immediate termination
///
/// In release builds, panics indicate programming errors (violated invariants).
/// For user-facing errors, we use Result types and proper error handling.
#[cfg(not(any(debug_assertions, test, doc)))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // SAFETY: libc::abort() terminates the process immediately.
    // This is the intended behavior for panic=abort builds.
    // abort() has no preconditions and never returns.
    unsafe { libc::abort() }
}

/// Required lang item for exception handling personality function.
///
/// Even with panic=abort, some code paths reference this symbol during
/// linking (particularly when using extern "C" functions). We provide
/// an empty stub since unwinding is disabled.
///
/// # Safety
///
/// This function is never actually called when panic=abort is set.
/// It exists solely to satisfy the linker. The empty body is safe
/// because the unwinding machinery is completely disabled.
#[cfg(not(any(debug_assertions, test, doc)))]
#[unsafe(no_mangle)]
pub extern "C" fn rust_eh_personality() {}
