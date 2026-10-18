use super::*;

#[test]
fn native_event_matrix_and_loss_precedence() {
    for (event, verdict) in [
        (NativeEvent::default(), NativeVerdict::Deliver),
        (
            NativeEvent {
                access_only: true,
                ..Default::default()
            },
            NativeVerdict::Ignore,
        ),
        (
            NativeEvent {
                metadata_only: true,
                ..Default::default()
            },
            NativeVerdict::Ignore,
        ),
        (
            NativeEvent {
                metadata_only: true,
                need_rescan: true,
                ..Default::default()
            },
            NativeVerdict::Overflow,
        ),
        (
            NativeEvent {
                error: true,
                need_rescan: true,
                ..Default::default()
            },
            NativeVerdict::BackendFailed,
        ),
    ] {
        assert_eq!(native_verdict(event), verdict);
    }
}

#[test]
fn loss_reconciles_but_explicit_native_failure_never_silently_falls_back() {
    for backend in [WatchBackend::Auto, WatchBackend::Native, WatchBackend::Poll] {
        assert_eq!(recovery(backend, false, false), Recovery::None);
        assert_eq!(recovery(backend, false, true), Recovery::Reconcile);
        assert_eq!(
            recovery(backend, true, true),
            if backend == WatchBackend::Native {
                Recovery::Stop
            } else {
                Recovery::PollFallback
            }
        );
    }
}

#[test]
fn poll_verdicts_compare_content_not_timestamps() {
    let bytes = b"schema_version: \"0.2.16\"\n";
    let digest = format!("sha256:{}", crate::digest::sha256_hex(bytes));
    assert!(!poll_content_changed(Some(&digest), Some(bytes)));
    assert!(poll_content_changed(Some(&digest), Some(b"edited\n")));
    assert!(poll_content_changed(Some(&digest), None));
    assert!(poll_content_changed(None, Some(bytes)));
}
