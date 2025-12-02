/*
 * runner.rs
 *
 * Spawn child, watch clock, kill if needed. All the tricky bits live here.
 *
 * kqueue: we tell the kernel "wake me when the process exits or the timer
 * fires" and then sleep. Zero CPU while waiting. Polling would be dumb.
 *
 * mach_continuous_time: the old mach_absolute_time stops when your laptop
 * sleeps. So a 1 hour timeout could take 8 hours if you close the lid.
 * mach_continuous_time keeps counting through sleep. That's what people expect.
 *
 * Process groups: when you timeout a shell script that spawns children, you
 * want to kill all of them, not just the shell. setpgid + killpg handles that.
 * --foreground disables this for interactive stuff.
 *
 * Signal forwarding: if timeout gets SIGTERM (docker stop, system shutdown),
 * we forward it to the child before dying. Otherwise you get orphans.
 * Self-pipe trick: handler writes to pipe, kqueue watches it.
 */

use alloc::format;
use alloc::string::{String, ToString};
use core::sync::atomic::{AtomicI32, Ordering};
use core::time::Duration;

use crate::args::{Confine, OwnedArgs};
use crate::duration::{is_no_timeout, parse_duration};
use crate::error::{Result, TimeoutError, exit_codes};
use crate::process::{RawChild, RawExitStatus, SpawnError, spawn_command};
use crate::signal::{Signal, parse_signal, signal_name, signal_number};
use crate::sync::AtomicOnce;

type RawFd = i32;

/*
 * Self-pipe trick for signal forwarding.
 *
 * Problem: we're blocked in kevent() waiting. If we get SIGTERM, we need to
 * forward it to the child. But signal handlers can only call write() and
 * _exit(), so we can't do much from the handler.
 *
 * Fix: create a pipe, handler writes a byte, kqueue watches the read end.
 * Signal arrives, pipe becomes readable, kevent returns. Simple.
 *
 * signalfd would work but that's Linux only. EVFILT_SIGNAL exists but doesn't
 * play nice with process monitoring. Pipe works everywhere.
 *
 * We forward SIGTERM, SIGINT, SIGHUP. The "please die" signals.
 */

static SIGNAL_PIPE: AtomicOnce<RawFd> = AtomicOnce::new();

/// Set up signal handlers for forwarding SIGTERM/SIGINT/SIGHUP to child.
/// Returns fd that becomes readable when signal arrives. Call before spawn.
///
/// Fds close automatically on exit. For library use (long-running process),
/// call [`cleanup_signal_forwarding`] to avoid fd leaks.
#[must_use]
pub fn setup_signal_forwarding() -> Option<RawFd> {
    /* Create pipe - write end for signal handler, read end for kqueue */
    let mut fds = [0i32; 2];
    // SAFETY: fds is a valid 2-element array, pipe() writes exactly 2 fds
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return None;
    }
    let read_fd = fds[0];
    let write_fd = fds[1];

    // SAFETY: read_fd and write_fd are valid fds just returned by pipe().
    // fcntl with F_GETFL/F_SETFL/F_SETFD are safe operations on valid fds.
    // Multiple ops share the same invariant (fd validity).
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    unsafe {
        let flags = libc::fcntl(read_fd, libc::F_GETFL);
        /* Only set O_NONBLOCK if F_GETFL succeeded (flags >= 0) */
        if flags >= 0 {
            libc::fcntl(read_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
        /* Signal handler must not block - set write_fd non-blocking too */
        let write_flags = libc::fcntl(write_fd, libc::F_GETFL);
        if write_flags >= 0 {
            libc::fcntl(write_fd, libc::F_SETFL, write_flags | libc::O_NONBLOCK);
        }
        libc::fcntl(read_fd, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(write_fd, libc::F_SETFD, libc::FD_CLOEXEC);
    }

    /* Try to claim the signal pipe slot first, before setting up handlers */
    if SIGNAL_PIPE.set(read_fd).is_err() {
        /* Already set (re-entry) - close fds to avoid leak */
        // SAFETY: read_fd and write_fd are valid fds from pipe() above.
        // Both close calls share the same invariant (fd validity from pipe()).
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            libc::close(read_fd);
            libc::close(write_fd);
        }
        return SIGNAL_PIPE.get().copied();
    }

    /* Store write_fd for signal handler AFTER confirming we own the pipe slot.
     * This prevents a race where the signal handler could see a write_fd
     * that's about to be closed by a concurrent re-entry. */
    SIGNAL_WRITE_FD.store(write_fd, Ordering::SeqCst);

    // SAFETY: sigaction struct is zeroed then properly initialized.
    // signal_handler is an extern "C" fn with correct signature.
    // sigemptyset and sigaction are standard POSIX calls with valid args.
    // All ops share the invariant of setting up signal handlers atomically.
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    unsafe {
        let mut sa: libc::sigaction = core::mem::zeroed();
        sa.sa_sigaction = signal_handler as *const () as usize;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&raw mut sa.sa_mask);

        libc::sigaction(libc::SIGTERM, &sa, core::ptr::null_mut());
        libc::sigaction(libc::SIGINT, &sa, core::ptr::null_mut());
        libc::sigaction(libc::SIGHUP, &sa, core::ptr::null_mut());
    }

    Some(read_fd)
}

/* Global write fd for signal handler (atomic for signal safety) */
static SIGNAL_WRITE_FD: AtomicI32 = AtomicI32::new(-1);

/// Close signal pipe fds and reset handlers to default.
///
/// Not needed for CLI (fds close on exit). For library use where you call
/// `run_command` multiple times, this prevents fd leaks.
///
/// Don't call while another thread is inside `run_command`.
pub fn cleanup_signal_forwarding() {
    /* Close write fd first to prevent signal handler from writing to closed fd */
    let write_fd = SIGNAL_WRITE_FD.swap(-1, Ordering::SeqCst);
    if write_fd >= 0 {
        // SAFETY: write_fd was set by setup_signal_forwarding and is valid.
        unsafe {
            libc::close(write_fd);
        }
    }

    /* Read fd is in SIGNAL_PIPE - we can't "unset" AtomicOnce, but we can close it.
     * The fd will be invalid if used again, but setup_signal_forwarding checks
     * if SIGNAL_PIPE is already set and returns early, so this is fine. */
    if let Some(&read_fd) = SIGNAL_PIPE.get() {
        // SAFETY: read_fd was set by setup_signal_forwarding and is valid.
        unsafe {
            libc::close(read_fd);
        }
    }

    /* Reset signal handlers to default */
    // SAFETY: SIG_DFL is the standard default handler, sigaction is safe with valid args.
    // All ops share the invariant of resetting signal handlers atomically.
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    unsafe {
        let mut sa: libc::sigaction = core::mem::zeroed();
        sa.sa_sigaction = libc::SIG_DFL;
        sa.sa_flags = 0;
        libc::sigemptyset(&raw mut sa.sa_mask);

        libc::sigaction(libc::SIGTERM, &sa, core::ptr::null_mut());
        libc::sigaction(libc::SIGINT, &sa, core::ptr::null_mut());
        libc::sigaction(libc::SIGHUP, &sa, core::ptr::null_mut());
    }
}

/* Minimal signal handler - write the signal number to the pipe */
extern "C" fn signal_handler(sig: i32) {
    let fd = SIGNAL_WRITE_FD.load(Ordering::SeqCst);
    if fd >= 0 {
        // SAFETY: fd was validated >= 0 and set by setup_signal_forwarding().
        // write() with a 1-byte buffer is async-signal-safe per POSIX.
        // We ignore errors since we can't do anything useful in a signal handler.
        // Store actual signal number (SIGTERM=15, SIGINT=2, SIGHUP=1 all fit in u8).
        unsafe {
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let byte: u8 = sig as u8;
            let _ = libc::write(fd, (&raw const byte).cast(), 1);
        }
    }
}

/* check if signal pipe has data (signal was received) */
fn read_signal_from_pipe(fd: RawFd) -> Option<Signal> {
    let mut buf = [0u8; 1];
    // SAFETY: buf is a valid 1-byte buffer, fd is the read end of our pipe.
    // read() will return -1 with EAGAIN if no data (non-blocking fd).
    let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), 1) };
    if n > 0 {
        /* Decode the signal number written by signal_handler */
        Signal::try_from_raw(buf[0] as i32).or(Some(Signal::SIGTERM))
    } else {
        None
    }
}

/*
 * Timing APIs - two modes based on --confine flag:
 *
 * Wall mode (default): mach_continuous_time()
 *   - Continues counting during system sleep
 *   - A 1-hour timeout fires when you open the lid after 7 hours of sleep
 *   - This is what most users expect from a timeout
 *
 * Active mode: clock_gettime_nsec_np(CLOCK_MONOTONIC_RAW)
 *   - Only counts awake/active time, pauses during sleep
 *   - ~28% faster (no timebase conversion needed)
 *   - Useful for benchmarks where idle time shouldn't count
 *
 * Empirically tested: CLOCK_MONOTONIC_RAW does NOT advance during pmset sleepnow.
 * See tests/clock_api_comparison.rs for benchmarks and verification.
 */

#[repr(C)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

unsafe extern "C" {
    fn mach_continuous_time() -> u64;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
    fn clock_gettime_nsec_np(clock_id: libc::clockid_t) -> u64;
}

const CLOCK_MONOTONIC_RAW: libc::clockid_t = 4;

/* get timebase ratio, cached forever */
fn get_timebase_info() -> (u64, u64) {
    static TIMEBASE: AtomicOnce<(u64, u64)> = AtomicOnce::new();

    *TIMEBASE.get_or_init(|| {
        let mut info = MachTimebaseInfo { numer: 0, denom: 0 };
        // SAFETY: info is a valid MachTimebaseInfo struct with correct layout (#[repr(C)]).
        // mach_timebase_info always succeeds on macOS and fills in numer/denom.
        unsafe {
            mach_timebase_info(&raw mut info);
        }
        (u64::from(info.numer), u64::from(info.denom))
    })
}

/* Wall time in nanoseconds - includes system sleep (mach_continuous_time) */
#[inline]
fn wall_now_ns() -> u64 {
    let (numer, denom) = get_timebase_info();
    // SAFETY: mach_continuous_time() has no preconditions, always returns valid u64.
    let abs_time = unsafe { mach_continuous_time() };

    /* Apple Silicon: numer == denom == 1, fast path avoids division */
    if numer == denom {
        return abs_time;
    }

    /* Intel: need conversion. u128 intermediate avoids overflow. */
    #[allow(clippy::cast_possible_truncation)]
    ((u128::from(abs_time) * u128::from(numer) / u128::from(denom)) as u64)
}

/* Active time in nanoseconds - excludes system sleep (CLOCK_MONOTONIC_RAW) */
#[inline]
fn active_now_ns() -> u64 {
    // SAFETY: clock_gettime_nsec_np with valid clock_id always succeeds on macOS
    unsafe { clock_gettime_nsec_np(CLOCK_MONOTONIC_RAW) }
}

/* Get current time based on confine mode */
#[inline]
fn precise_now_ns(confine: Confine) -> u64 {
    match confine {
        Confine::Wall => wall_now_ns(),
        Confine::Active => active_now_ns(),
    }
}

/* max ns that fits in isize (~292 years on 64-bit) */
const MAX_TIMER_NS: u64 = isize::MAX as u64;

/* duration to ns, clamped for kqueue */
#[inline]
fn duration_to_ns(d: Duration) -> u64 {
    d.as_secs()
        .saturating_mul(1_000_000_000)
        .saturating_add(u64::from(d.subsec_nanos()))
        .min(MAX_TIMER_NS)
}

/* what happened when we ran the on-timeout hook */
#[derive(Debug, Clone, Default)]
pub struct HookResult {
    pub ran: bool,              /* did we actually run it? */
    pub exit_code: Option<i32>, /* None if timed out or failed to start */
    pub timed_out: bool,        /* killed because it took too long? */
    pub elapsed_ms: u64,        /* how long it ran */
}

/* what happened when we ran the command */
#[derive(Debug)]
pub enum RunResult {
    Completed(RawExitStatus),
    TimedOut {
        signal: Signal,
        killed: bool, /* true if we had to escalate to SIGKILL */
        status: Option<RawExitStatus>,
        hook: Option<HookResult>, /* on-timeout hook result if configured */
    },
    SignalForwarded {
        /* we got SIGTERM/SIGINT/SIGHUP, passed it on */
        signal: Signal,
        status: Option<RawExitStatus>,
    },
}

impl RunResult {
    /* what exit code to return per GNU spec */
    #[must_use]
    pub fn exit_code(&self, preserve_status: bool, timeout_exit_code: u8) -> u8 {
        match self {
            Self::Completed(status) => status_to_exit_code(status),
            Self::TimedOut {
                signal,
                killed,
                status,
                hook: _,
            } => {
                if preserve_status {
                    status.map_or_else(
                        || {
                            let sig = if *killed { Signal::SIGKILL } else { *signal };
                            signal_exit_code(sig)
                        },
                        |s| status_to_exit_code(&s),
                    )
                } else {
                    timeout_exit_code
                }
            }
            Self::SignalForwarded { signal, status } => {
                /* We got killed by a signal - return 128 + signum like the child would */
                status.map_or_else(|| signal_exit_code(*signal), |s| status_to_exit_code(&s))
            }
        }
    }
}

/* POSIX: exit_code = 128 + signum */
#[inline]
#[allow(clippy::cast_sign_loss)]
const fn signal_exit_code(signal: Signal) -> u8 {
    ((128i32 + signal_number(signal)) & 0xFF) as u8
}

/* exit status to 8-bit code, POSIX style */
#[allow(clippy::cast_sign_loss)]
fn status_to_exit_code(status: &RawExitStatus) -> u8 {
    if let Some(sig) = status.signal() {
        return ((128i32 + sig) & 0xFF) as u8;
    }

    (status.code().unwrap_or(1) & 0xFF) as u8
}

/* runtime config built from CLI args */
#[derive(Debug)]
pub struct RunConfig {
    pub timeout: Duration,            /* how long before we send the signal */
    pub signal: Signal,               /* what to send (default: SIGTERM) */
    pub kill_after: Option<Duration>, /* if set, SIGKILL after this grace period */
    pub foreground: bool,             /* don't create process group */
    pub verbose: bool,                /* print signal diagnostics */
    pub quiet: bool,                  /* suppress our stderr */
    pub timeout_exit_code: u8,        /* exit code on timeout (default: 124) */
    pub on_timeout: Option<String>,   /* hook to run before killing (%p = PID) */
    pub on_timeout_limit: Duration,   /* how long hook gets to run */
    pub confine: Confine,             /* wall (includes sleep) or active (excludes sleep) */
}

impl RunConfig {
    /* build config from CLI args. fails if duration/signal is bogus. */
    pub fn from_args(args: &OwnedArgs, duration_str: &str) -> Result<Self> {
        let timeout = parse_duration(duration_str)?;
        let signal = parse_signal(&args.signal)?;
        let kill_after = args
            .kill_after
            .as_ref()
            .map(|s| parse_duration(s))
            .transpose()?;
        let on_timeout_limit = parse_duration(&args.on_timeout_limit)?;

        /* Warn if on-timeout-limit exceeds main timeout (when hook is set) */
        if args.on_timeout.is_some() && on_timeout_limit > timeout && !is_no_timeout(&timeout) {
            crate::eprintln!(
                "warning: --on-timeout-limit ({}) exceeds main timeout ({})",
                args.on_timeout_limit,
                duration_str
            );
        }

        Ok(Self {
            timeout,
            signal,
            kill_after,
            foreground: args.foreground,
            verbose: args.verbose,
            quiet: args.quiet,
            timeout_exit_code: args.timeout_exit_code.unwrap_or(exit_codes::TIMEOUT),
            on_timeout: args.on_timeout.clone(),
            on_timeout_limit,
            confine: args.confine,
        })
    }
}

/// Spawn command and enforce timeout.
///
/// Errors: command not found, permission denied, spawn failed, signal failed.
pub fn run_command(command: &str, args: &[String], config: &RunConfig) -> Result<RunResult> {
    /* put child in its own process group unless foreground mode */
    let use_process_group = !config.foreground;

    let mut child = spawn_command(command, args, use_process_group).map_err(|e| match e {
        SpawnError::NotFound(s) => TimeoutError::CommandNotFound(s),
        SpawnError::PermissionDenied(s) => TimeoutError::PermissionDenied(s),
        SpawnError::Spawn(errno) => TimeoutError::SpawnError(errno),
        SpawnError::Wait(errno) => TimeoutError::SpawnError(errno),
        SpawnError::InvalidArg => TimeoutError::Internal("invalid argument".to_string()),
    })?;

    /* zero timeout = run forever */
    if is_no_timeout(&config.timeout) {
        let status = child.wait().map_err(|e| match e {
            SpawnError::Wait(errno) => TimeoutError::SpawnError(errno),
            _ => TimeoutError::Internal("wait failed".to_string()),
        })?;
        return Ok(RunResult::Completed(status));
    }

    monitor_with_timeout(&mut child, config)
}

/*
 * main timeout logic using kqueue. kernel wakes us on process exit
 * or timer expiry - zero CPU while waiting.
 */
fn monitor_with_timeout(child: &mut RawChild, config: &RunConfig) -> Result<RunResult> {
    #[allow(clippy::cast_possible_wrap)]
    let pid = child.id() as i32;

    /* wait for exit or timeout */
    let exit_result = wait_with_kqueue(child, pid, config.timeout, config.confine)?;

    match exit_result {
        WaitResult::Exited(status) => {
            return Ok(RunResult::Completed(status));
        }
        WaitResult::ReceivedSignal(sig) => {
            /* We received SIGTERM/SIGINT/SIGHUP - forward to child and exit */
            if config.verbose && !config.quiet {
                crate::eprintln!("timeout: forwarding signal {} to command", signal_name(sig));
            }
            send_signal(pid, sig, config.foreground)?;
            let status = child.wait().ok();
            return Ok(RunResult::SignalForwarded {
                signal: sig,
                status,
            });
        }
        WaitResult::TimedOut => { /* Continue to timeout handling below */ }
    }

    /* Run on-timeout hook if specified */
    let hook_result = config
        .on_timeout
        .as_ref()
        .map(|cmd| run_on_timeout_hook(cmd, pid, config));

    /* time's up, send the signal */
    if config.verbose && !config.quiet {
        crate::eprintln!(
            "timeout: sending signal {} to command",
            signal_name(config.signal)
        );
    }

    send_signal(pid, config.signal, config.foreground)?;

    /* if --kill-after, give it a grace period then escalate to SIGKILL */
    if let Some(kill_after) = config.kill_after {
        let grace_result = wait_with_kqueue(child, pid, kill_after, config.confine)?;

        match grace_result {
            WaitResult::Exited(status) => {
                return Ok(RunResult::TimedOut {
                    signal: config.signal,
                    killed: false,
                    status: Some(status),
                    hook: hook_result,
                });
            }
            WaitResult::ReceivedSignal(sig) => {
                /* Forward signal during grace period */
                if config.verbose && !config.quiet {
                    crate::eprintln!("timeout: forwarding signal {} to command", signal_name(sig));
                }
                send_signal(pid, sig, config.foreground)?;
                let status = child.wait().ok();
                return Ok(RunResult::SignalForwarded {
                    signal: sig,
                    status,
                });
            }
            WaitResult::TimedOut => { /* Continue to SIGKILL below */ }
        }

        /* still alive? SIGKILL it */
        if config.verbose && !config.quiet {
            crate::eprintln!("timeout: sending signal SIGKILL to command");
        }

        send_signal(pid, Signal::SIGKILL, config.foreground)?;

        let status = child.wait().map_err(|e| match e {
            SpawnError::Wait(errno) => TimeoutError::SpawnError(errno),
            _ => TimeoutError::Internal("wait failed".to_string()),
        })?;

        Ok(RunResult::TimedOut {
            signal: config.signal,
            killed: true,
            status: Some(status),
            hook: hook_result,
        })
    } else {
        /* no kill-after, just wait for it to die */
        let status = child.wait().map_err(|e| match e {
            SpawnError::Wait(errno) => TimeoutError::SpawnError(errno),
            _ => TimeoutError::Internal("wait failed".to_string()),
        })?;

        Ok(RunResult::TimedOut {
            signal: config.signal,
            killed: false,
            status: Some(status),
            hook: hook_result,
        })
    }
}

/*
 * What happened when we waited on the process.
 */
enum WaitResult {
    Exited(RawExitStatus),
    TimedOut,
    /// Received a signal that should be forwarded to the child
    ReceivedSignal(Signal),
}

/* get errno - on macOS this is a thread-local via __error() */
#[inline]
fn errno() -> i32 {
    unsafe extern "C" {
        fn __error() -> *mut i32;
    }
    // SAFETY: __error always returns valid pointer on macOS. The dereference and
    // function call share the same invariant (pointer validity for thread-local errno).
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    unsafe {
        *__error()
    }
}

/*
 * wait using kqueue - EVFILT_PROC for exit, EVFILT_TIMER with NOTE_NSECONDS
 * for nanosecond precision, and optionally EVFILT_READ for signal pipe.
 * Direct libc because nix kqueue API keeps changing.
 */
fn wait_with_kqueue(
    child: &mut RawChild,
    pid: i32,
    timeout: Duration,
    confine: Confine,
) -> Result<WaitResult> {
    let start_ns = precise_now_ns(confine);
    let timeout_ns = duration_to_ns(timeout);

    /* Get signal pipe fd if available */
    let signal_fd = SIGNAL_PIPE.get().copied();

    /* create kqueue fd */
    // SAFETY: kqueue() has no preconditions, returns -1 on error (checked below).
    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        return Err(TimeoutError::Internal(format!(
            "kqueue failed: errno {}",
            errno()
        )));
    }

    /*
     * kqueue filters:
     * - EVFILT_PROC + NOTE_EXIT: wake when process dies, no polling needed
     * - EVFILT_TIMER + NOTE_NSECONDS: nanosecond timer (kernel scheduler adds
     *   ~15-30ms latency anyway, but we're not the bottleneck)
     * - EVFILT_READ on signal pipe: self-pipe trick for forwarding signals
     *
     * EV_ONESHOT on proc/timer means auto-delete after firing.
     * Signal pipe stays registered for multiple signals.
     */
    /* Use fixed-size array instead of Vec to avoid heap allocation */
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    let mut changes = [
        /* Watch for process exit */
        libc::kevent {
            ident: pid as usize,
            filter: libc::EVFILT_PROC,
            flags: libc::EV_ADD | libc::EV_ONESHOT,
            fflags: libc::NOTE_EXIT,
            data: 0,
            udata: core::ptr::null_mut(),
        },
        /* High-precision timer - data is nanoseconds */
        libc::kevent {
            ident: 1, /* Timer identifier (arbitrary, just needs to be unique) */
            filter: libc::EVFILT_TIMER,
            flags: libc::EV_ADD | libc::EV_ONESHOT,
            fflags: libc::NOTE_NSECONDS,
            /* Safe: duration_to_ns clamps to isize::MAX */
            data: timeout_ns as isize,
            udata: core::ptr::null_mut(),
        },
        /* Signal pipe watcher (may be unused if no signal fd) */
        libc::kevent {
            ident: signal_fd.unwrap_or(0) as usize,
            filter: libc::EVFILT_READ,
            flags: if signal_fd.is_some() { libc::EV_ADD } else { 0 },
            fflags: 0,
            data: 0,
            udata: core::ptr::null_mut(),
        },
    ];
    let num_changes = if signal_fd.is_some() { 3 } else { 2 };

    /* Buffer for returned events - we only need one */
    let mut event = libc::kevent {
        ident: 0,
        filter: 0,
        flags: 0,
        fflags: 0,
        data: 0,
        udata: core::ptr::null_mut(),
    };

    /*
     * kevent() atomically registers filters and waits. No race condition.
     *
     * EINTR: signal interrupted us, recalculate remaining time and retry.
     * ESRCH: process already dead, just reap it.
     *
     * No timeout arg to kevent, the timer filter handles it.
     */
    let deadline_ns = start_ns.saturating_add(timeout_ns);

    loop {
        /* check if we've passed deadline */
        let now_ns = precise_now_ns(confine);
        if now_ns >= deadline_ns {
            // SAFETY: kq is a valid fd, close is always safe
            unsafe { libc::close(kq) };
            return Ok(WaitResult::TimedOut);
        }
        let remaining_ns = deadline_ns.saturating_sub(now_ns);

        /* update timer */
        #[allow(clippy::cast_possible_wrap)]
        {
            changes[1].data = remaining_ns.min(MAX_TIMER_NS) as isize;
        }

        // SAFETY: kq is a valid kqueue fd. changes is a valid slice of kevent structs.
        // event is a valid buffer for one kevent. Timeout is null (wait forever).
        // kevent() is the standard BSD API for event notification.
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let n = unsafe {
            libc::kevent(
                kq,
                changes.as_ptr(),
                num_changes,
                &raw mut event,
                1,
                core::ptr::null(), /* No timeout - timer event handles it */
            )
        };

        if n < 0 {
            let err = errno();
            /* EINTR: signal interrupted us, retry with remaining time */
            if err == libc::EINTR {
                continue;
            }
            /* ESRCH: process already gone, reap it */
            if err == libc::ESRCH {
                // SAFETY: kq is a valid fd
                unsafe { libc::close(kq) };
                if let Ok(Some(status)) = child.try_wait() {
                    return Ok(WaitResult::Exited(status));
                }
                /* race: kernel says dead but try_wait says no - use blocking wait */
                let status = child.wait().map_err(|e| match e {
                    SpawnError::Wait(errno) => TimeoutError::SpawnError(errno),
                    _ => TimeoutError::Internal("wait failed".to_string()),
                })?;
                return Ok(WaitResult::Exited(status));
            }
            // SAFETY: kq is a valid fd
            unsafe { libc::close(kq) };
            return Err(TimeoutError::Internal(format!(
                "kevent failed: errno {err}"
            )));
        }

        /* got result, exit loop */
        break;
    }

    /* check for registration errors */
    if (event.flags & libc::EV_ERROR) != 0 {
        #[allow(clippy::cast_possible_truncation)]
        let err_code = event.data as i32;
        /* ESRCH = process gone, that's fine */
        if err_code != libc::ESRCH {
            // SAFETY: kq is a valid fd
            unsafe { libc::close(kq) };
            return Err(TimeoutError::Internal(format!(
                "kqueue event registration failed: errno {}",
                err_code
            )));
        }
        /* ESRCH - reap it */
        // SAFETY: kq is a valid fd
        unsafe { libc::close(kq) };
        if let Ok(Some(status)) = child.try_wait() {
            return Ok(WaitResult::Exited(status));
        }
        let status = child.wait().map_err(|e| match e {
            SpawnError::Wait(errno) => TimeoutError::SpawnError(errno),
            _ => TimeoutError::Internal("wait failed".to_string()),
        })?;
        return Ok(WaitResult::Exited(status));
    }

    // SAFETY: kq is a valid fd
    unsafe { libc::close(kq) };

    /* EVFILT_PROC = exited, EVFILT_TIMER = timed out, EVFILT_READ = signal received */
    if event.filter == libc::EVFILT_PROC {
        let status = child.wait().map_err(|e| match e {
            SpawnError::Wait(errno) => TimeoutError::SpawnError(errno),
            _ => TimeoutError::Internal("wait failed".to_string()),
        })?;
        return Ok(WaitResult::Exited(status));
    }

    if event.filter == libc::EVFILT_READ {
        /* Signal pipe became readable - a signal was received */
        if let Some(fd) = signal_fd
            && let Some(sig) = read_signal_from_pipe(fd)
        {
            return Ok(WaitResult::ReceivedSignal(sig));
        }
    }

    Ok(WaitResult::TimedOut)
}

/*
 * Run the on-timeout hook command with PID substitution.
 * The hook has a time limit to prevent hanging. We log but don't fail
 * if the hook fails - the main timeout behavior must proceed.
 *
 * Substitution: %p -> PID, %% -> literal %
 */
fn run_on_timeout_hook(cmd: &str, pid: i32, config: &RunConfig) -> HookResult {
    let start_ns = precise_now_ns(config.confine);

    /* Expand %p to PID, %% to literal % */
    let expanded_cmd = cmd
        .replace("%%", "\x00PERCENT\x00") /* placeholder for %% */
        .replace("%p", &format!("{}", pid))
        .replace("\x00PERCENT\x00", "%"); /* restore literal % */

    if config.verbose && !config.quiet {
        crate::eprintln!("timeout: running on-timeout hook: {}", expanded_cmd);
    }

    /* Run via shell to support complex commands */
    let spawn_result = spawn_command("sh", &[String::from("-c"), expanded_cmd], false);

    let mut child = match spawn_result {
        Ok(c) => c,
        Err(e) => {
            if config.verbose && !config.quiet {
                crate::eprintln!("timeout: on-timeout hook failed to start: {:?}", e);
            }
            return HookResult {
                ran: false,
                exit_code: None,
                timed_out: false,
                elapsed_ms: precise_now_ns(config.confine).saturating_sub(start_ns) / 1_000_000,
            };
        }
    };

    /* Wait using kqueue for zero-CPU waiting */
    let hook_wait_result =
        wait_for_hook_with_kqueue(&mut child, config.on_timeout_limit, config.confine);
    let elapsed_ms = precise_now_ns(config.confine).saturating_sub(start_ns) / 1_000_000;

    match hook_wait_result {
        HookWaitResult::Exited(status) => {
            let exit_code = status.code();
            if config.verbose
                && !config.quiet
                && let Some(code) = exit_code
                && code != 0
            {
                crate::eprintln!("timeout: on-timeout hook exited with code {}", code);
            }
            HookResult {
                ran: true,
                exit_code,
                timed_out: false,
                elapsed_ms,
            }
        }
        HookWaitResult::TimedOut => {
            if config.verbose && !config.quiet {
                crate::eprintln!("timeout: on-timeout hook timed out, killing");
            }
            let _ = child.kill();
            let _ = child.wait();
            HookResult {
                ran: true,
                exit_code: None,
                timed_out: true,
                elapsed_ms,
            }
        }
        HookWaitResult::Error(e) => {
            if config.verbose && !config.quiet {
                crate::eprintln!("timeout: on-timeout hook wait failed: {}", e);
            }
            HookResult {
                ran: true,
                exit_code: None,
                timed_out: false,
                elapsed_ms,
            }
        }
    }
}

/* Result of waiting for hook process */
enum HookWaitResult {
    Exited(RawExitStatus),
    TimedOut,
    Error(String),
}

/*
 * Wait for hook process using kqueue - zero CPU while waiting.
 * Simpler than wait_with_kqueue since we don't need signal forwarding.
 */
fn wait_for_hook_with_kqueue(
    child: &mut RawChild,
    timeout: Duration,
    confine: Confine,
) -> HookWaitResult {
    #[allow(clippy::cast_possible_wrap)]
    let pid = child.id() as i32;
    let start_ns = precise_now_ns(confine);
    let timeout_ns = duration_to_ns(timeout);
    let deadline_ns = start_ns.saturating_add(timeout_ns);

    /* create kqueue fd */
    // SAFETY: kqueue() has no preconditions, returns -1 on error (checked below).
    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        return HookWaitResult::Error(format!("kqueue failed: errno {}", errno()));
    }

    /* Watch for process exit and set timer */
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    let mut changes = [
        libc::kevent {
            ident: pid as usize,
            filter: libc::EVFILT_PROC,
            flags: libc::EV_ADD | libc::EV_ONESHOT,
            fflags: libc::NOTE_EXIT,
            data: 0,
            udata: core::ptr::null_mut(),
        },
        libc::kevent {
            ident: 2, /* different from main timer */
            filter: libc::EVFILT_TIMER,
            flags: libc::EV_ADD | libc::EV_ONESHOT,
            fflags: libc::NOTE_NSECONDS,
            data: timeout_ns as isize,
            udata: core::ptr::null_mut(),
        },
    ];

    let mut event = libc::kevent {
        ident: 0,
        filter: 0,
        flags: 0,
        fflags: 0,
        data: 0,
        udata: core::ptr::null_mut(),
    };

    loop {
        /* Recalculate remaining time (handles EINTR correctly) */
        let now_ns = precise_now_ns(confine);
        if now_ns >= deadline_ns {
            // SAFETY: kq is a valid fd from kqueue() above.
            unsafe { libc::close(kq) };
            return HookWaitResult::TimedOut;
        }
        let remaining_ns = deadline_ns.saturating_sub(now_ns);
        changes[1].data = remaining_ns.min(MAX_TIMER_NS) as isize;

        // SAFETY: kq is a valid kqueue fd. changes is a valid slice of kevent structs.
        // event is a valid buffer for one kevent. Timeout is null (wait forever).
        #[allow(clippy::cast_possible_wrap)]
        let n = unsafe {
            libc::kevent(
                kq,
                changes.as_ptr(),
                changes.len() as i32,
                &raw mut event,
                1,
                core::ptr::null(),
            )
        };

        if n < 0 {
            let err = errno();
            if err == libc::EINTR {
                continue;
            }
            if err == libc::ESRCH {
                /* Process already gone */
                // SAFETY: kq is a valid fd from kqueue() above.
                unsafe { libc::close(kq) };
                return match child.try_wait() {
                    Ok(Some(status)) => HookWaitResult::Exited(status),
                    Ok(None) => match child.wait() {
                        Ok(status) => HookWaitResult::Exited(status),
                        Err(e) => HookWaitResult::Error(format!("{:?}", e)),
                    },
                    Err(e) => HookWaitResult::Error(format!("{:?}", e)),
                };
            }
            // SAFETY: kq is a valid fd from kqueue() above.
            unsafe { libc::close(kq) };
            return HookWaitResult::Error(format!("kevent failed: errno {}", err));
        }
        break;
    }

    /* Check for registration errors */
    if (event.flags & libc::EV_ERROR) != 0 {
        #[allow(clippy::cast_possible_truncation)]
        let err_code = event.data as i32;
        // SAFETY: kq is a valid fd from kqueue() above.
        unsafe { libc::close(kq) };
        if err_code == libc::ESRCH {
            return match child.wait() {
                Ok(status) => HookWaitResult::Exited(status),
                Err(e) => HookWaitResult::Error(format!("{:?}", e)),
            };
        }
        return HookWaitResult::Error(format!("kqueue registration failed: errno {}", err_code));
    }

    // SAFETY: kq is a valid fd from kqueue() above.
    unsafe { libc::close(kq) };

    if event.filter == libc::EVFILT_PROC {
        match child.wait() {
            Ok(status) => HookWaitResult::Exited(status),
            Err(e) => HookWaitResult::Error(format!("{:?}", e)),
        }
    } else {
        HookWaitResult::TimedOut
    }
}

/*
 * Send signal to child.
 *
 * Normal mode: killpg() signals the whole process group, catches shell
 * scripts with children. ESRCH means it's already dead, that's fine.
 *
 * Foreground mode: just signal the one process. For interactive stuff
 * that needs TTY. Grandchildren won't get the signal though.
 *
 * killpg can fail with ESRCH even when process exists (race conditions),
 * so we fall back to regular kill().
 */
fn send_signal(pid: i32, signal: Signal, foreground: bool) -> Result<()> {
    let sig = signal.as_raw();

    if foreground {
        // SAFETY: kill() is safe with any pid/signal combo, returns -1 on error
        let ret = unsafe { libc::kill(pid, sig) };
        if ret == 0 {
            return Ok(());
        }
        let err = errno();
        if err == libc::ESRCH {
            return Ok(()); // already dead, that's fine
        }
        return Err(TimeoutError::SignalError(err));
    }

    /*
     * try process group first. if ESRCH, fall back to just the process.
     * orphaned children get reparented to init - that's unix for you.
     */
    // SAFETY: killpg() is safe with any pid/signal combo, returns -1 on error
    let ret = unsafe { libc::killpg(pid, sig) };
    if ret == 0 {
        return Ok(());
    }

    let err = errno();
    if err == libc::ESRCH {
        /* group gone, try process directly */
        // SAFETY: kill() is safe with any pid/signal combo, returns -1 on error
        let ret = unsafe { libc::kill(pid, sig) };
        if ret == 0 {
            return Ok(());
        }
        let err = errno();
        if err == libc::ESRCH {
            return Ok(()); // already dead
        }
        return Err(TimeoutError::SignalError(err));
    }

    Err(TimeoutError::SignalError(err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_result_exit_code_timeout() {
        let result = RunResult::TimedOut {
            signal: Signal::SIGTERM,
            killed: false,
            status: None,
            hook: None,
        };

        assert_eq!(result.exit_code(false, 124), 124);
        assert_eq!(result.exit_code(true, 124), 143); /* 128 + 15 */
    }

    #[test]
    fn test_run_result_exit_code_killed() {
        let result = RunResult::TimedOut {
            signal: Signal::SIGTERM,
            killed: true,
            status: None,
            hook: None,
        };

        assert_eq!(result.exit_code(false, 124), 124);
        assert_eq!(result.exit_code(true, 124), 137); /* 128 + 9 */
    }

    #[test]
    fn test_custom_timeout_exit_code() {
        let result = RunResult::TimedOut {
            signal: Signal::SIGTERM,
            killed: false,
            status: None,
            hook: None,
        };

        assert_eq!(result.exit_code(false, 42), 42);
        assert_eq!(result.exit_code(false, 0), 0);
    }

    /* Skip under Miri: libc::kill is an unsupported foreign function */
    #[test]
    #[cfg(not(miri))]
    fn test_send_signal_to_nonexistent_process() {
        /* ESRCH should be handled gracefully */
        let fake_pid = 99999i32;
        let result = send_signal(fake_pid, Signal::SIGTERM, true);
        assert!(result.is_ok(), "ESRCH should be handled gracefully");
    }
}
