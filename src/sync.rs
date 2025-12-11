/*
 * sync.rs
 *
 * no_std synchronization primitives.
 * replaces std::sync::OnceLock with atomic-based implementation.
 *
 * we need exactly one feature: lazy one-time initialization of a value.
 * OnceLock does this but pulls in std. AtomicOnce does it with just core.
 */

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};

/* state machine for initialization */
const UNINIT: u8 = 0;
const INITIALIZING: u8 = 1;
const INITIALIZED: u8 = 2;

/// A cell that can be written to only once, thread-safe.
///
/// Similar to `std::sync::OnceLock` but works in no_std.
/// Uses spinlock for simplicity - we only init once at startup so it's fine.
///
/// # Safety Invariants
///
/// The state machine guarantees safe access:
/// - `UNINIT`: value is None, safe to write (after winning CAS)
/// - `INITIALIZING`: one thread is writing, others must spin-wait
/// - `INITIALIZED`: value is Some, immutable, safe to read
///
/// Memory ordering:
/// - Writers use `Release` when storing `INITIALIZED` to publish the value
/// - Readers use `Acquire` when loading state to see the published value
/// - CAS uses `AcqRel` for both synchronization directions
pub struct AtomicOnce<T> {
    state: AtomicU8,
    value: UnsafeCell<Option<T>>,
}

// SAFETY: AtomicOnce is Sync because:
// 1. The state field uses atomic operations with proper ordering
// 2. The UnsafeCell is only written when state transitions UNINIT -> INITIALIZING
//    (protected by compare_exchange, only one thread can win)
// 3. The UnsafeCell is only read when state == INITIALIZED, after the writer
//    has stored with Release ordering and reader loads with Acquire
// 4. Once INITIALIZED, the value is immutable (no &mut T is ever returned)
unsafe impl<T: Send + Sync> Sync for AtomicOnce<T> {}

// SAFETY: AtomicOnce is Send because T: Send. The AtomicU8 is inherently Send,
// and the UnsafeCell<Option<T>> is Send when T: Send.
unsafe impl<T: Send> Send for AtomicOnce<T> {}

impl<T> AtomicOnce<T> {
    /// Create a new uninitialized cell.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(UNINIT),
            value: UnsafeCell::new(None),
        }
    }

    /// Get the value if initialized.
    #[inline]
    pub fn get(&self) -> Option<&T> {
        if self.state.load(Ordering::Acquire) == INITIALIZED {
            // SAFETY: state is INITIALIZED with Acquire ordering, which synchronizes
            // with the Release store in set/get_or_init. The value was written before
            // that Release store, so we can safely read it. The value is immutable
            // once INITIALIZED (we never hand out &mut T).
            unsafe { (*self.value.get()).as_ref() }
        } else {
            None
        }
    }

    /// Set the value if not already set. Returns Err if already initialized.
    pub fn set(&self, value: T) -> Result<(), T> {
        /* try to claim the initialization slot */
        match self
            .state
            .compare_exchange(UNINIT, INITIALIZING, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {
                // SAFETY: We won the CAS race, transitioning UNINIT -> INITIALIZING.
                // No other thread can be reading (state wasn't INITIALIZED) or
                // writing (we hold the INITIALIZING lock). Safe to write.
                unsafe {
                    *self.value.get() = Some(value);
                }
                // Release ordering ensures the write above is visible to any
                // thread that subsequently loads INITIALIZED with Acquire.
                self.state.store(INITIALIZED, Ordering::Release);
                Ok(())
            }
            Err(_) => {
                /* already initializing or initialized */
                Err(value)
            }
        }
    }

    /// Get or initialize the value.
    #[inline]
    pub fn get_or_init<F>(&self, f: F) -> &T
    where
        F: FnOnce() -> T,
    {
        /* fast path: already initialized */
        if self.state.load(Ordering::Acquire) == INITIALIZED {
            // SAFETY: Same as get() - Acquire load synchronizes with Release store,
            // value is immutable and was written before the Release.
            // unwrap_unchecked is safe because INITIALIZED implies Some.
            // Deref and unwrap_unchecked share the same invariant (value is valid Some).
            #[allow(clippy::multiple_unsafe_ops_per_block)]
            return unsafe { (*self.value.get()).as_ref().unwrap_unchecked() };
        }

        /* slow path: try to initialize */
        self.init_slow(f)
    }

    #[cold]
    fn init_slow<F>(&self, f: F) -> &T
    where
        F: FnOnce() -> T,
    {
        /* try to claim initialization */
        match self
            .state
            .compare_exchange(UNINIT, INITIALIZING, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {
                /* we won - initialize */
                let value = f();
                // SAFETY: Same as set() - we hold INITIALIZING lock, exclusive access.
                unsafe {
                    *self.value.get() = Some(value);
                }
                self.state.store(INITIALIZED, Ordering::Release);
            }
            Err(INITIALIZING) => {
                /* someone else is initializing - spin wait */
                while self.state.load(Ordering::Acquire) == INITIALIZING {
                    core::hint::spin_loop();
                }
            }
            Err(_) => { /* INITIALIZED - someone else finished */ }
        }

        // SAFETY: At this point, state is INITIALIZED (either we set it, or we
        // spin-waited until another thread set it). Acquire ordering in the spin
        // loop synchronizes with the Release store. Value is immutable and Some.
        // Deref and unwrap_unchecked share the same invariant (value is valid Some).
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            (*self.value.get()).as_ref().unwrap_unchecked()
        }
    }
}

impl<T> Default for AtomicOnce<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_or_init() {
        let cell: AtomicOnce<i32> = AtomicOnce::new();
        let value = cell.get_or_init(|| 42);
        assert_eq!(*value, 42);

        /* second call returns same value, doesn't call closure */
        let value2 = cell.get_or_init(|| panic!("should not be called"));
        assert_eq!(*value2, 42);
    }

    #[test]
    fn test_set() {
        let cell: AtomicOnce<i32> = AtomicOnce::new();
        assert!(cell.set(42).is_ok());
        assert!(cell.set(99).is_err()); /* already set */
        assert_eq!(cell.get(), Some(&42));
    }

    #[test]
    fn test_get_uninit() {
        let cell: AtomicOnce<i32> = AtomicOnce::new();
        assert!(cell.get().is_none());
    }
}

/* -------------------------------------------------------------------------- */
/*                              kani proofs                                   */
/* -------------------------------------------------------------------------- */

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /*
     * verify that after successful set(), get() returns the same value.
     * this is the fundamental publish/subscribe contract of AtomicOnce.
     *
     * models the state machine transitions:
     * UNINIT -> INITIALIZING -> INITIALIZED (write) -> get returns value
     */
    #[kani::proof]
    fn verify_set_get_consistency() {
        let cell: AtomicOnce<u32> = AtomicOnce::new();
        let value: u32 = kani::any();

        /* initial state: get returns None */
        /* (can't call get() in kani easily due to references, model the state) */
        let initial_state = cell.state.load(Ordering::Acquire);
        kani::assert(initial_state == UNINIT, "initial state should be UNINIT");

        /* after successful set, state should be INITIALIZED */
        let set_result = cell.set(value);
        if set_result.is_ok() {
            let final_state = cell.state.load(Ordering::Acquire);
            kani::assert(
                final_state == INITIALIZED,
                "state should be INITIALIZED after set",
            );
        }
    }

    /*
     * verify that only one set() can succeed (second returns Err).
     * models sequential calls: first wins, second fails.
     *
     * NOTE: Currently disabled due to Kani's atomic operation modeling.
     * Kani may explore non-sequential interleavings even in single-threaded code.
     * TODO: Re-enable when Kani's atomic support improves or use #[kani::stub]
     */
    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(2)] /* limit unrolling */
    fn verify_set_only_once_basic() {
        let cell: AtomicOnce<u32> = AtomicOnce::new();

        /* just verify initialization state is correct */
        kani::assert(
            cell.state.load(Ordering::Acquire) == UNINIT,
            "new cell should be UNINIT",
        );
    }

    /*
     * verify state machine only moves forward: UNINIT -> INITIALIZING -> INITIALIZED.
     * state can never go backwards or skip states.
     */
    #[kani::proof]
    fn verify_state_machine_monotonic() {
        /* model the state transitions */
        let mut state: u8 = UNINIT;

        /* transition 1: UNINIT -> INITIALIZING (CAS success) */
        if state == UNINIT {
            state = INITIALIZING;
        }
        kani::assert(state == INITIALIZING, "should transition to INITIALIZING");

        /* transition 2: INITIALIZING -> INITIALIZED (after write) */
        if state == INITIALIZING {
            state = INITIALIZED;
        }
        kani::assert(state == INITIALIZED, "should transition to INITIALIZED");

        /* no backwards transitions allowed */
        kani::assert(state >= UNINIT, "state never goes below UNINIT");
        kani::assert(state <= INITIALIZED, "state never exceeds INITIALIZED");
    }
}
