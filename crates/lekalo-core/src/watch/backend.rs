//! Shared native/poll semantics. The CLI maps real notify events (inotify,
//! FSEvents/kqueue, ReadDirectoryChangesW) to these flags; inventory remains
//! authority after overflow, unpaired rename or subscription loss.
use super::request::WatchBackend;

/// The relevant facts of one native notification, without copying paths.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeEvent {
    pub error: bool,
    pub access_only: bool,
    pub metadata_only: bool,
    pub need_rescan: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeVerdict {
    Ignore,
    Deliver,
    Overflow,
    BackendFailed,
}
/// Event loss dominates metadata filtering: a need-rescan flag must not
/// disappear just because the associated event was access/metadata-only.
pub fn native_verdict(event: NativeEvent) -> NativeVerdict {
    if event.error {
        NativeVerdict::BackendFailed
    } else if event.need_rescan {
        NativeVerdict::Overflow
    } else if event.access_only || event.metadata_only {
        NativeVerdict::Ignore
    } else {
        NativeVerdict::Deliver
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Recovery {
    None,
    Reconcile,
    PollFallback,
    Stop,
}
/// The live host uses this same policy: explicit native never silently
/// switches backend; auto may recover through content-hash polling.
pub fn recovery(backend: WatchBackend, failed: bool, overflow: bool) -> Recovery {
    if failed {
        if backend == WatchBackend::Native {
            Recovery::Stop
        } else {
            Recovery::PollFallback
        }
    } else if overflow {
        Recovery::Reconcile
    } else {
        Recovery::None
    }
}

/// Poll compares content, not timestamps. Identical-byte saves are quiet;
/// creation/deletion are changes even when no timestamp can be observed.
pub fn poll_content_changed(recorded: Option<&str>, bytes: Option<&[u8]>) -> bool {
    match (recorded, bytes) {
        (Some(recorded), Some(bytes)) => {
            format!("sha256:{}", crate::digest::sha256_hex(bytes)) != recorded
        }
        (None, _) | (_, None) => true,
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod tests;
