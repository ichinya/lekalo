//! Watch's macOS resident-memory supervisor. This is a sampled kill
//! threshold, NOT a kernel allocation ceiling: overshoot between the 5 ms
//! transport polls is possible and evidence must remain `degraded`.
//! Seatbelt denies fork, so the one observed process is the whole payload.
use super::transport::Process;
use std::io::{Read, Write};

pub(super) trait ResidentDriver {
    fn resident_bytes(&mut self) -> std::io::Result<u64>;
}

pub(super) struct Monitored<P, D> {
    pub process: P,
    pub driver: D,
    pub limit: u64,
}

impl<P: Process, D: ResidentDriver> Process for Monitored<P, D> {
    fn stdin(&mut self) -> Option<Box<dyn Write + Send>> {
        self.process.stdin()
    }
    fn stdout(&mut self) -> Option<Box<dyn Read + Send>> {
        self.process.stdout()
    }
    fn stderr(&mut self) -> Option<Box<dyn Read + Send>> {
        self.process.stderr()
    }
    fn poll(&mut self) -> std::io::Result<Option<i32>> {
        let status = self.process.poll()?;
        if status.is_some() {
            return Ok(status);
        }
        match self.driver.resident_bytes() {
            Ok(bytes) if bytes <= self.limit => Ok(None),
            sample => {
                // proc_pidinfo can lose a race to exit. Accept only positively
                // observed exit, otherwise fail closed and let pump kill/reap.
                if let Some(code) = self.process.poll()? {
                    return Ok(Some(code));
                }
                Err(std::io::Error::new(
                    std::io::ErrorKind::OutOfMemory,
                    if sample.is_err() {
                        "resident-monitor-unavailable"
                    } else {
                        "resident-memory-limit"
                    },
                ))
            }
        }
    }
    fn terminate(&mut self) {
        self.process.terminate();
    }
    fn reap(&mut self) {
        self.process.reap();
    }
}

#[cfg(target_os = "macos")]
pub(super) struct MacResident(pub u32);
#[cfg(target_os = "macos")]
impl ResidentDriver for MacResident {
    fn resident_bytes(&mut self) -> std::io::Result<u64> {
        let mut info = std::mem::MaybeUninit::<libc::proc_taskinfo>::zeroed();
        // SAFETY: proc_pidinfo writes at most the supplied struct size to a
        // valid, aligned buffer. No field is read unless it returned the
        // complete fixed-size record for the owned child PID.
        let size = std::mem::size_of::<libc::proc_taskinfo>();
        let written = unsafe {
            libc::proc_pidinfo(
                self.0 as i32,
                libc::PROC_PIDTASKINFO,
                0,
                info.as_mut_ptr().cast(),
                size as i32,
            )
        };
        if written != size as i32 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { info.assume_init() }.pti_resident_size)
    }
}

#[cfg(target_os = "macos")]
pub(super) fn available() -> bool {
    MacResident(std::process::id()).resident_bytes().is_ok()
        && std::path::Path::new("/usr/bin/sandbox-exec").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target_protocol::transport::{self, TransportFailure, TransportLimits};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    struct Driver(std::io::Result<u64>);
    impl ResidentDriver for Driver {
        fn resident_bytes(&mut self) -> std::io::Result<u64> {
            self.0
                .as_ref()
                .copied()
                .map_err(|e| std::io::Error::new(e.kind(), "synthetic"))
        }
    }
    struct Fake {
        done: bool,
        killed: Arc<AtomicBool>,
        reaped: Arc<AtomicBool>,
    }
    impl Process for Fake {
        fn stdin(&mut self) -> Option<Box<dyn Write + Send>> {
            Some(Box::new(std::io::sink()))
        }
        fn stdout(&mut self) -> Option<Box<dyn Read + Send>> {
            Some(Box::new(std::io::empty()))
        }
        fn stderr(&mut self) -> Option<Box<dyn Read + Send>> {
            Some(Box::new(std::io::empty()))
        }
        fn poll(&mut self) -> std::io::Result<Option<i32>> {
            Ok(self.done.then_some(0))
        }
        fn terminate(&mut self) {
            self.killed.store(true, Ordering::SeqCst);
        }
        fn reap(&mut self) {
            self.reaped.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn macos_driver_threshold_and_probe_failure_kill_and_reap_fail_closed() {
        // Hermetic driver qualification on every host. This executes the
        // production pump/monitor, not Darwin's proc_pidinfo or seatbelt.
        for sample in [
            Ok(101),
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        ] {
            let killed = Arc::new(AtomicBool::new(false));
            let reaped = Arc::new(AtomicBool::new(false));
            let mut child = Monitored {
                process: Fake {
                    done: false,
                    killed: killed.clone(),
                    reaped: reaped.clone(),
                },
                driver: Driver(sample),
                limit: 100,
            };
            let failure =
                transport::pump(&mut child, b"", &TransportLimits::default(), None).unwrap_err();
            assert_eq!(failure, TransportFailure::ResourceLimit);
            assert!(killed.load(Ordering::SeqCst) && reaped.load(Ordering::SeqCst));
        }
    }
    #[test]
    fn a_confirmed_exit_does_not_require_a_post_exit_memory_sample() {
        let mut child = Monitored {
            process: Fake {
                done: true,
                killed: Arc::new(AtomicBool::new(false)),
                reaped: Arc::new(AtomicBool::new(false)),
            },
            driver: Driver(Err(std::io::Error::from(std::io::ErrorKind::NotFound))),
            limit: 100,
        };
        assert_eq!(
            transport::pump(&mut child, b"", &TransportLimits::default(), None)
                .unwrap()
                .exit_code,
            0
        );
    }
    #[test]
    fn resident_threshold_is_inclusive_and_still_honors_cancel() {
        let mut child = Monitored {
            process: Fake {
                done: false,
                killed: Arc::new(AtomicBool::new(false)),
                reaped: Arc::new(AtomicBool::new(false)),
            },
            driver: Driver(Ok(100)),
            limit: 100,
        };
        assert_eq!(child.poll().unwrap(), None);
        assert_eq!(
            transport::pump(
                &mut child,
                b"",
                &TransportLimits::default(),
                Some(&AtomicBool::new(true))
            )
            .unwrap_err(),
            TransportFailure::Cancelled
        );
        assert!(child.process.killed.load(Ordering::SeqCst));
    }
}
