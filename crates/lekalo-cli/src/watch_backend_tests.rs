//! Hermetic platform event traces through the REAL callback, bounded
//! channel, classified dirty-path reader and scheduler (not a mock loop).
use super::*;
use notify::event::{AccessKind, Flag, MetadataKind, ModifyKind, RenameMode};

#[test]
fn inotify_overflow_and_fsevents_loss_are_not_silent() {
    let (tx, rx) = mpsc::sync_channel(1);
    let lost = AtomicBool::new(false);
    let failed = AtomicBool::new(false);
    // notify represents inotify IN_Q_OVERFLOW / FSEvents MustScanSubDirs
    // as NeedRescan, which must dominate even a metadata-only event.
    let event = notify::Event::new(EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)))
        .set_flag(Flag::Rescan);
    native_event(Ok(event), &tx, &lost, &failed);
    assert!(lost.swap(false, Ordering::SeqCst));
    assert!(rx.try_recv().is_err());
    native_event(
        Err(notify::Error::generic("synthetic subscription loss")),
        &tx,
        &lost,
        &failed,
    );
    assert!(failed.load(Ordering::SeqCst));
    assert_eq!(
        core::backend::recovery(WatchBackend::Auto, true, false),
        core::backend::Recovery::PollFallback
    );
    assert_eq!(
        core::backend::recovery(WatchBackend::Native, true, false),
        core::backend::Recovery::Stop
    );
    let mut scheduler = Scheduler::new(&WatchRequest::default());
    scheduler.require_full_rescan(0);
    assert!(matches!(
        scheduler.poll(150),
        Decision::Start {
            full_rescan: true,
            ..
        }
    ));
}

#[test]
fn bounded_native_queue_preserves_loss_instead_of_silently_dropping_edits() {
    let (tx, rx) = mpsc::sync_channel(core::request::limits::MAX_PENDING_PATHS);
    let lost = AtomicBool::new(false);
    let failed = AtomicBool::new(false);
    for i in 0..=core::request::limits::MAX_PENDING_PATHS {
        native_event(
            Ok(notify::Event::new(EventKind::Any).add_path(PathBuf::from(format!("p{i}")))),
            &tx,
            &lost,
            &failed,
        );
    }
    assert_eq!(
        rx.try_iter().count(),
        core::request::limits::MAX_PENDING_PATHS
    );
    assert!(lost.load(Ordering::SeqCst));
    assert!(!failed.load(Ordering::SeqCst));
}

#[test]
fn split_rename_access_and_same_byte_saves_use_the_live_dirty_reader() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/watch/project");
    let root = tempfile::tempdir().unwrap();
    snapshot::materialize(&snapshot::capture(&fixture, false).unwrap(), root.path()).unwrap();
    let inventory = snapshot::capture(root.path(), false)
        .unwrap()
        .without_bytes();
    let old = root.path().join("lekalo/modules/planner/entities.yaml");
    let new = root.path().join("lekalo/modules/planner/moved.yaml");
    let (tx, rx) = mpsc::sync_channel(16);
    let lost = AtomicBool::new(false);
    let failed = AtomicBool::new(false);
    let mut scheduler = Scheduler::new(&WatchRequest::default());
    // Reading a file is not an edit. Nor is an identical-byte content
    // notification, even if a real backend delivered it after a touch.
    native_event(
        Ok(notify::Event::new(EventKind::Access(AccessKind::Any)).add_path(old.clone())),
        &tx,
        &lost,
        &failed,
    );
    native_event(
        Ok(notify::Event::new(EventKind::Modify(ModifyKind::Any)).add_path(old.clone())),
        &tx,
        &lost,
        &failed,
    );
    dirty(
        &rx,
        root.path(),
        &CapturePolicy::default(),
        &inventory,
        &mut scheduler,
        0,
    );
    assert_eq!(scheduler.poll(150), Decision::None);
    std::fs::rename(&old, &new).unwrap();
    // inotify may split a rename into From/To; macOS may coalesce it.
    // Both halves go through the callback, then the content inventory
    // determines the actual removal/creation, never the event order.
    for (path, mode) in [(old, RenameMode::From), (new, RenameMode::To)] {
        native_event(
            Ok(notify::Event::new(EventKind::Modify(ModifyKind::Name(mode))).add_path(path)),
            &tx,
            &lost,
            &failed,
        );
    }
    dirty(
        &rx,
        root.path(),
        &CapturePolicy::default(),
        &inventory,
        &mut scheduler,
        200,
    );
    let Decision::Start {
        paths, full_rescan, ..
    } = scheduler.poll(350)
    else {
        panic!("rename must settle")
    };
    assert_eq!(
        paths,
        BTreeSet::from([
            "lekalo/modules/planner/entities.yaml".to_owned(),
            "lekalo/modules/planner/moved.yaml".to_owned(),
        ])
    );
    assert!(!full_rescan);
    assert!(!lost.load(Ordering::SeqCst));
}
