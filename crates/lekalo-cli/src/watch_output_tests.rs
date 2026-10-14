use super::*;
struct Blocked {
    entered: mpsc::SyncSender<()>,
    release: Receiver<()>,
}
impl Write for Blocked {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let _ = self.entered.send(());
        let _ = self.release.recv();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn slow_consumer_times_out_once_without_enqueuing_more_records() {
    let (entered, observed) = mpsc::sync_channel(1);
    let (release, wait) = mpsc::sync_channel(1);
    let output = Output::with_writer(
        true,
        Blocked {
            entered,
            release: wait,
        },
    );
    let failure = output
        .deliver("first\n".to_owned(), Duration::from_millis(25))
        .unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::TimedOut);
    observed.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(
        output
            .deliver("second\n".to_owned(), Duration::from_secs(1))
            .unwrap_err()
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    release.send(()).unwrap();
}
struct Broken;
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn broken_consumer_and_output_byte_cap_fail_closed() {
    let output = Output::with_writer(true, Broken);
    assert_eq!(
        output
            .deliver("x\n".to_owned(), Duration::from_secs(1))
            .unwrap_err()
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    let output = Output::with_writer(true, io::sink());
    assert!(output
        .deliver(
            "x".repeat(core::request::limits::MAX_OUTPUT_QUEUE_BYTES + 1),
            Duration::from_secs(1)
        )
        .is_err());
    assert!(output.failed.get());
}
#[test]
fn teardown_wait_never_claims_a_live_worker_has_finished() {
    let (release, wait) = mpsc::sync_channel(1);
    let join = std::thread::spawn(move || {
        let _ = wait.recv();
    });
    assert!(!wait_finished(&join, Duration::from_millis(20)));
    release.send(()).unwrap();
    assert!(wait_finished(&join, Duration::from_secs(1)));
    join.join().unwrap();
}
