//! Private, host-supervised compiler IPC. Neither sandbox may spawn a child.
//! Each peer has its own existing sandbox/job and empty inherited environment.
//! The compiler's project view is empty; source reaches it through VFS RPC only.
use std::cell::RefCell;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::transport::{self, Process, TransportFailure, TransportLimits, TransportSuccess};

type Input = Box<dyn Write + Send>;
type Output = Box<dyn Read + Send>;

#[derive(Clone, Copy)]
pub(super) enum Role {
    Producer,
    Compiler,
}

pub(super) struct Session {
    io: Mutex<Option<(Input, Output)>>,
    failure: Mutex<Option<TransportFailure>>,
    stop: AtomicBool,
    check: AtomicBool,
    verified: AtomicBool,
    deadline: Instant,
    relays: Mutex<Vec<std::thread::JoinHandle<()>>>,
}

impl Session {
    pub(super) fn new(limits: &TransportLimits) -> Arc<Self> {
        Arc::new(Self {
            io: Mutex::new(None),
            failure: Mutex::new(None),
            stop: AtomicBool::new(false),
            check: AtomicBool::new(false),
            verified: AtomicBool::new(false),
            deadline: Instant::now() + Duration::from_millis(limits.timeout_ms),
            relays: Mutex::new(Vec::new()),
        })
    }
    pub(super) fn fail(&self, failure: TransportFailure) {
        self.failure.lock().unwrap().get_or_insert(failure);
        self.stop.store(true, Ordering::Release);
    }
    pub(super) fn failure(&self) -> Option<TransportFailure> {
        *self.failure.lock().unwrap()
    }
    pub(super) fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
    pub(super) fn join_relays(&self) {
        self.io.lock().unwrap().take();
        for relay in self.relays.lock().unwrap().drain(..) {
            let _ = relay.join();
        }
    }
    pub(super) fn is_stopped(&self) -> bool {
        self.stopped()
    }
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
    fn deadline(&self) -> bool {
        Instant::now() >= self.deadline
    }
    /// Successful output is accepted only after the supervisor confirms the
    /// native peer is still alive. An unexpected clean exit is a failure too.
    pub(super) fn verify(&self, cancel: Option<&AtomicBool>) -> Result<(), TransportFailure> {
        self.check.store(true, Ordering::Release);
        while !self.verified.load(Ordering::Acquire) {
            if let Some(failure) = self.failure() {
                return Err(failure);
            }
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return Err(TransportFailure::Cancelled);
            }
            if self.deadline() {
                return Err(TransportFailure::Timeout);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.failure().map_or(Ok(()), Err)
    }
}

thread_local! { static CONTEXT: RefCell<Option<(Arc<Session>, Role)>> = const { RefCell::new(None) }; }

pub(super) fn with<T>(session: Arc<Session>, role: Role, run: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            CONTEXT.with(|value| value.borrow_mut().take());
        }
    }
    CONTEXT.with(|value| {
        assert!(value.borrow().is_none());
        *value.borrow_mut() = Some((session, role));
    });
    let _reset = Reset;
    run()
}

pub(super) fn context() -> Option<(Arc<Session>, Role)> {
    CONTEXT.with(|value| value.borrow().clone())
}

/// Reuse the existing deadline/output/resident-memory pump. Its logical stdout
/// is the producer's fd2; fd0/fd1 are exclusively the compiler RPC channel.
pub(super) fn pump(
    child: &mut impl Process,
    limits: &TransportLimits,
    cancel: Option<&AtomicBool>,
    session: Arc<Session>,
    role: Role,
) -> Result<TransportSuccess, TransportFailure> {
    let input = child.stdin().ok_or(TransportFailure::Spawn)?;
    let output = child.stdout().ok_or(TransportFailure::Spawn)?;
    let error = child.stderr().ok_or(TransportFailure::Spawn)?;
    let mut relays = Vec::new();
    let (stdout, stderr): (Output, Output) = match role {
        Role::Compiler => {
            *session.io.lock().unwrap() = Some((input, output));
            (Box::new(std::io::empty()), error)
        }
        Role::Producer => {
            let native_io = loop {
                if let Some(io) = session.io.lock().unwrap().take() {
                    break io;
                }
                if let Some(failure) = session.failure() {
                    return Err(failure);
                }
                if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                    return Err(TransportFailure::Cancelled);
                }
                if session.deadline() {
                    return Err(TransportFailure::Timeout);
                }
                std::thread::sleep(Duration::from_millis(2));
            };
            let (mut compiler_input, mut compiler_output) = native_io;
            let (mut producer_input, mut producer_output) = (input, output);
            let keepalive = session.clone();
            relays.push(std::thread::spawn(move || {
                let _ = std::io::copy(&mut producer_output, &mut compiler_input);
                // Do not let producer EOF terminate the native peer before the
                // supervisor's completion check and deliberate cleanup.
                while !keepalive.stopped() && !keepalive.deadline() {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }));
            relays.push(std::thread::spawn(move || {
                let _ = std::io::copy(&mut compiler_output, &mut producer_input);
            }));
            (error, Box::new(std::io::empty()))
        }
    };
    let mut peer = Peer {
        child,
        session: session.clone(),
        role,
        stdout: Some(stdout),
        stderr: Some(stderr),
    };
    let result = transport::pump_ordinary(&mut peer, b"", limits, cancel);
    match role {
        Role::Compiler => {
            if !session.stopped() {
                session.fail(
                    result
                        .as_ref()
                        .err()
                        .copied()
                        .unwrap_or(TransportFailure::Spawn),
                );
            }
        }
        Role::Producer => {
            // Scoped owner verifies liveness and then stops the compiler.
            // Relay readers are released when both sandbox runners terminate.
            if let Err(failure) = result {
                session.fail(failure);
            }
        }
    }
    // No unconditional join while the other job still owns a pipe. The scoped
    // owner joins both runners before destroying either sandbox; relay buffers
    // are fixed-size std::io::copy buffers and contain no detached process.
    session.relays.lock().unwrap().extend(relays);
    result
}

struct Peer<'a, P> {
    child: &'a mut P,
    session: Arc<Session>,
    role: Role,
    stdout: Option<Output>,
    stderr: Option<Output>,
}
impl<P: Process> Process for Peer<'_, P> {
    fn stdin(&mut self) -> Option<Input> {
        Some(Box::new(std::io::sink()))
    }
    fn stdout(&mut self) -> Option<Output> {
        self.stdout.take()
    }
    fn stderr(&mut self) -> Option<Output> {
        self.stderr.take()
    }
    fn poll(&mut self) -> std::io::Result<Option<i32>> {
        let observed = self.child.poll()?;
        if matches!(self.role, Role::Compiler) {
            if observed.is_some() && !self.session.stopped() {
                self.session.fail(TransportFailure::Spawn);
            }
            if observed.is_none() && self.session.check.load(Ordering::Acquire) {
                self.session.verified.store(true, Ordering::Release);
            }
        }
        if self.session.stopped() {
            self.child.terminate();
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        if self.session.deadline() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        Ok(observed)
    }
    fn terminate(&mut self) {
        self.child.terminate();
    }
    fn reap(&mut self) {
        self.child.reap();
    }
}
