//! Cooperative MCP worker cancellation at the CLI read/process boundary.
//!
//! A timed-out, cancelled, or EOF-abandoned worker may be inside a pure
//! core traversal that cannot be interrupted safely; it keeps its slot
//! until completion. Its thread-local guard nevertheless prevents NEW
//! child processes after abandonment (review r4 F10), and the bounded
//! doctor Git child is killed on a checkpoint during its wait.

use std::cell::RefCell;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct WorkGuard {
    abandoned: Arc<AtomicBool>,
    spawn_gate: Arc<Mutex<()>>,
    deadline: Instant,
}

impl WorkGuard {
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            abandoned: Arc::new(AtomicBool::new(false)),
            spawn_gate: Arc::new(Mutex::new(())),
            deadline: Instant::now() + Duration::from_millis(timeout_ms),
        }
    }

    pub fn abandon(&self) {
        // Serialize abandonment with a worker's actual process spawn:
        // after this returns, no new child can be created (r4 F10).
        let _gate = self.spawn_gate.lock().expect("spawn gate");
        self.abandoned.store(true, Ordering::SeqCst);
    }

    pub fn should_stop(&self) -> bool {
        self.abandoned.load(Ordering::SeqCst) || Instant::now() >= self.deadline
    }
}

thread_local! {
    static CURRENT: RefCell<Option<WorkGuard>> = const { RefCell::new(None) };
}

/// Enter one worker's guard for the duration of its CLI/core call; the
/// previous context is restored even if the worker panics.
pub fn with_guard<T>(guard: WorkGuard, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<WorkGuard>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let previous = CURRENT.with(|slot| slot.borrow_mut().replace(guard));
    let _restore = Restore(previous);
    work()
}

/// A spawn/read checkpoint from the CLI edge: ordinary CLI commands have
/// no MCP guard and continue unchanged.
pub fn should_stop() -> bool {
    CURRENT.with(|slot| slot.borrow().as_ref().is_some_and(WorkGuard::should_stop))
}

/// Serialize a worker's spawn with cancellation; ordinary CLI commands
/// still spawn normally. A returned `None` means the worker was already
/// abandoned/deadlined and must not start another process.
pub fn spawn_if_active(command: &mut Command) -> std::io::Result<Option<Child>> {
    CURRENT.with(|slot| {
        let borrowed = slot.borrow();
        let Some(guard) = borrowed.as_ref() else {
            return command.spawn().map(Some);
        };
        let _gate = guard.spawn_gate.lock().expect("spawn gate");
        if guard.should_stop() {
            Ok(None)
        } else {
            command.spawn().map(Some)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abandoned_worker_cannot_pass_a_spawn_checkpoint() {
        assert!(!should_stop(), "ordinary CLI has no MCP cancellation");
        let guard = WorkGuard::new(30_000);
        let clone = guard.clone();
        with_guard(guard, || {
            assert!(!should_stop());
            clone.abandon();
            assert!(should_stop());
            assert!(spawn_if_active(&mut Command::new("git")).unwrap().is_none());
        });
        assert!(!should_stop(), "guard must not leak into another call");
        with_guard(WorkGuard::new(0), || assert!(should_stop()));
    }
}
