use super::*;
#[test]
fn cancellation_after_owned_scan_spawn_is_stale_and_preserves_accepted_state() {
    use std::sync::Arc;
    let root = project();
    let policy = policy(root.path());
    let mut service = WatchService::new(WatchRequest {
        targets: vec!["watch-fixture".to_owned()],
        ..WatchRequest::default()
    })
    .unwrap();
    let initial = snapshot::capture_with_policy(root.path(), &policy).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let accepted = service.run_revision(
        &initial,
        scratch.path(),
        &RunContext {
            run_id: 1,
            generation: 0,
            full_rescan: true,
            cancel: &AtomicBool::new(false),
        },
    );
    assert_eq!(accepted.status, Status::Valid);
    let revision = accepted.source_revision;
    service.commit(accepted.staged);
    std::fs::write(root.path().join("src/control.txt"), "hang").unwrap();
    let captured = snapshot::capture_with_policy(root.path(), &policy).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let spawned = Arc::new(AtomicBool::new(false));
    let (flag, marked) = (cancel.clone(), spawned.clone());
    SCAN_ON_SPAWN.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            marked.store(true, std::sync::atomic::Ordering::SeqCst);
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }))
    });
    let scratch = tempfile::tempdir().unwrap();
    let since = std::time::Instant::now();
    let outcome = service.run_revision(
        &captured,
        scratch.path(),
        &RunContext {
            run_id: 2,
            generation: 1,
            full_rescan: false,
            cancel: &cancel,
        },
    );
    assert!(
        spawned.load(std::sync::atomic::Ordering::SeqCst),
        "signal occurs after an owned process exists, not before the stage"
    );
    assert!(since.elapsed() < std::time::Duration::from_secs(5));
    assert!(outcome.stale);
    assert!(outcome.staged.revision.is_empty());
    service.commit(outcome.staged);
    assert_eq!(service.accepted().unwrap().revision, revision);
}
