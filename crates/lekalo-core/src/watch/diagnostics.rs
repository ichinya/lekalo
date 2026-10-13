//! The `watch.*` diagnostic builders (issue #95): lifecycle/coverage
//! facts of the watch session, never duplicates of domain findings.
//! Every rule is registered in the merged 0.8.0 union registry
//! (`LEK-WATCH-001..007`); existing loader, lock, artifact, adapter,
//! validation, and version diagnostics pass through unchanged.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject, DataValue};
use crate::diagnostics::Diagnostic;

fn assembled(
    diagnostic: Result<Diagnostic, crate::diagnostics::normalize::BuildError>,
) -> Diagnostic {
    diagnostic.expect("watch rules are registered and active in the 0.8.0 union registry")
}

/// `watch.backend-unavailable` (LEK-WATCH-001): no requested backend
/// can run; a successful explicit auto fallback emits it with the
/// warning default under a valid status.
pub fn backend_unavailable(backend: &str) -> Diagnostic {
    let mut data = DataObject::new();
    data.insert("backend".to_owned(), token_value(backend));
    assembled(build("watch.backend-unavailable", None, None, data))
}

/// `watch.event-loss` (LEK-WATCH-002): events were lost or ambiguous;
/// bounded reconciliation recovered a complete snapshot. The closed
/// cause vocabulary is `overflow|ambiguous-rename|subscription-lost|inventory-mismatch`.
pub fn event_loss(cause: &str) -> Diagnostic {
    let mut data = DataObject::new();
    data.insert("cause".to_owned(), token_value(cause));
    assembled(build("watch.event-loss", None, None, data))
}

/// `watch.snapshot-unstable` (LEK-WATCH-003): bounded capture attempts
/// were exhausted; no torn result was committed and a newer dirty
/// generation stays pending.
pub fn snapshot_unstable(attempts: u64) -> Diagnostic {
    let mut data = DataObject::new();
    data.insert("attempts".to_owned(), DataValue::Count(attempts));
    assembled(build("watch.snapshot-unstable", None, None, data))
}

/// `watch.resource-limit` (LEK-WATCH-004): a hard bound prevents
/// complete safe work; carries the closed limit name and observed
/// count, never a raw path list.
pub fn resource_limit(limit: &str, count: u64) -> Diagnostic {
    let mut data = DataObject::new();
    data.insert("limit".to_owned(), token_value(limit));
    data.insert("count".to_owned(), DataValue::Count(count));
    assembled(build("watch.resource-limit", None, None, data))
}

/// `watch.selective-unavailable` (LEK-WATCH-005): a complete,
/// reason-labeled full fallback or an explicitly unavailable selective
/// stage; the closed stage/reason pair says which.
pub fn selective_unavailable(stage: &str, reason: &str) -> Diagnostic {
    let mut data = DataObject::new();
    data.insert("stage".to_owned(), token_value(stage));
    data.insert("reason".to_owned(), token_value(reason));
    assembled(build("watch.selective-unavailable", None, None, data))
}

/// `watch.adapter-recovery` (LEK-WATCH-006): a target cycle failed;
/// recovery is scheduled on the next affected save and the session
/// stays alive. The adapter's own classified diagnostic is retained
/// alongside.
pub fn adapter_recovery(target: &str) -> Diagnostic {
    let mut data = DataObject::new();
    data.insert("target".to_owned(), token_value(target));
    assembled(build("watch.adapter-recovery", None, None, data))
}

/// `watch.cancellation-incomplete` (LEK-WATCH-007): owned work could
/// not be proven terminated within its deadline; no overlapping
/// replacement run starts.
pub fn cancellation_incomplete(stage: &str) -> Diagnostic {
    let mut data = DataObject::new();
    data.insert("stage".to_owned(), token_value(stage));
    assembled(build("watch.cancellation-incomplete", None, None, data))
}
