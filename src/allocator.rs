/*
 * alloc.rs
 *
 * Custom global allocator using libc malloc/free directly.
 * Avoids std's allocator machinery (thread-locals, error handling, etc.)
 *
 * This is only used by the binary. Tests use std's allocator via
 * #[cfg(test)] extern crate std in main.rs.
 */

use core::alloc::{GlobalAlloc, Layout};

/// System allocator - thin wrapper around malloc/free.
///
/// # Safety
///
/// This allocator delegates to libc's malloc/free/realloc/calloc. It is safe
/// to use as a global allocator because:
///
/// - All Layout invariants are upheld by Rust's type system (size <= isize::MAX,
///   alignment is power of 2)
/// - malloc/calloc return null on failure (handled by alloc crate)
/// - free accepts null pointers safely
/// - realloc with null acts as malloc, with zero size acts as free
///
/// Thread safety: libc's allocator is thread-safe on all supported platforms.
#[allow(dead_code)] // only used in release builds via #[global_allocator]
pub struct SystemAlloc;

#[allow(clippy::multiple_unsafe_ops_per_block)]
// SAFETY: GlobalAlloc requires the implementor to be thread-safe. libc's malloc/free
// are thread-safe on macOS (and all modern POSIX systems). All methods follow the
// GlobalAlloc contract: returning aligned, non-overlapping memory or null on failure.
unsafe impl GlobalAlloc for SystemAlloc {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: malloc is safe to call with any size. Returns aligned memory for
        // alignments <= 16 bytes (guaranteed by macOS malloc). For larger alignments,
        // posix_memalign is used which guarantees the requested alignment.
        // Layout::align() is always a power of 2 and >= 1, satisfying posix_memalign
        // requirements that alignment be a power of 2 and multiple of sizeof(void*).
        unsafe {
            if layout.align() > 16 {
                let mut ptr: *mut libc::c_void = core::ptr::null_mut();
                if libc::posix_memalign(&mut ptr, layout.align(), layout.size()) == 0 {
                    ptr as *mut u8
                } else {
                    core::ptr::null_mut()
                }
            } else {
                libc::malloc(layout.size()) as *mut u8
            }
        }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        // SAFETY: Caller guarantees ptr was returned by this allocator's alloc/realloc.
        // free() is safe with any pointer from malloc/posix_memalign, including null.
        unsafe {
            libc::free(ptr as *mut libc::c_void);
        }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: Caller guarantees ptr was returned by this allocator's alloc/realloc.
        // realloc() is safe with any valid malloc'd pointer. new_size is bounded by
        // Layout invariants (caller must construct a valid Layout for the new size).
        //
        // # Warning: Alignment not preserved
        //
        // libc::realloc does NOT guarantee preservation of alignment > 16 bytes.
        // If the original allocation used posix_memalign for alignment > 16, the
        // reallocated memory may have only 16-byte alignment. This is permitted by
        // GlobalAlloc::realloc which documents that alignment may change.
        //
        // Callers requiring alignment > 16 bytes MUST NOT use realloc. Instead:
        //   1. Allocate new memory with alloc(new_layout)
        //   2. Copy data from old to new
        //   3. Deallocate old memory with dealloc(ptr, old_layout)
        //
        // Note: This debug_assert is intentionally stripped in release builds (panic=abort).
        // In release, the alignment warning is documented-only; callers must comply by design.
        debug_assert!(
            layout.align() <= 16,
            "realloc called with alignment > 16; alignment may not be preserved"
        );
        // SAFETY: see above - realloc is safe with valid malloc'd pointer
        unsafe { libc::realloc(ptr as *mut libc::c_void, new_size) as *mut u8 }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: calloc is safe with any count/size, returns zeroed memory or null.
        // For large alignments, we use alloc() then write_bytes() to zero.
        // write_bytes is safe because ptr is valid for layout.size() bytes (just allocated).
        unsafe {
            if layout.align() > 16 {
                let ptr = self.alloc(layout);
                if !ptr.is_null() {
                    core::ptr::write_bytes(ptr, 0, layout.size());
                }
                ptr
            } else {
                libc::calloc(1, layout.size()) as *mut u8
            }
        }
    }
}

#[cfg(not(any(debug_assertions, test)))]
#[global_allocator]
static ALLOCATOR: SystemAlloc = SystemAlloc;
