//! The session interrupt seam (issue #95): a cross-platform Ctrl-C
//! handler that flips one atomic flag the watch host polls between
//! runs. A normal interrupt emits the terminal `session-stopped` event
//! and exits 0; it never aborts a stage mid-flight (cancellation stays
//! cooperative between bounded work units).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

fn mark_interrupted() {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

/// Whether an interrupt was delivered since `install` was called.
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// Shared cancellation input for bounded startup adapter discovery.
pub fn cancellation_flag() -> &'static AtomicBool {
    &INTERRUPTED
}

/// Install the platform handler. Returns false when the platform
/// refused the registration (the host then reports the session without
/// a graceful-interrupt guarantee rather than pretending).
pub fn install() -> bool {
    install_platform()
}

#[cfg(windows)]
fn install_platform() -> bool {
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
    // SAFETY: the handler only performs an atomic store; the API
    // contract allows exactly that inside its delivery constraints.
    unsafe { SetConsoleCtrlHandler(Some(handler), 1) != 0 }
}

#[cfg(windows)]
unsafe extern "system" fn handler(_ctrl_type: u32) -> i32 {
    mark_interrupted();
    1
}

#[cfg(unix)]
fn install_platform() -> bool {
    // SAFETY: `signal` with a handler that only performs an atomic
    // store is async-signal-safe; SIGINT keeps its default disposition
    // before the call and the handler is installed once per process.
    unsafe {
        libc::signal(libc::SIGINT, handler as usize) != libc::SIG_ERR
            && libc::signal(libc::SIGTERM, handler as usize) != libc::SIG_ERR
    }
}

#[cfg(unix)]
extern "C" fn handler(_signal: libc::c_int) {
    mark_interrupted();
}

/// A poll guard the host holds; reserved for future stage-bound checks.
pub struct InterruptFlag(Arc<AtomicBool>);

impl InterruptFlag {
    /// A fresh flag mirroring the process-wide interrupt state.
    pub fn shared() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Whether this flag or the process handler was tripped.
    pub fn check(&self) -> bool {
        self.0.load(Ordering::SeqCst) || interrupted()
    }

    /// Set this flag.
    pub fn set(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
