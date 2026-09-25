//! The observed baseline metrics artifact (issue #49).
//!
//! The saved baseline the pilot compares later runs against: one
//! deterministic document over the observed index (its digest, its
//! binding-state counts, its scan identity) plus the identity of the
//! confirmed native gate plan at baseline time. It is a thin view over
//! the accepted observed store — the only writer in the seam — and it
//! introduces no clock: identical index and plan identity produce
//! byte-identical documents.
//!
//! Home: `.lekalo/import/observed/baseline.json`, inside the accepted
//! `lekalo.observed-model-draft` authority home the index already uses.
//! A separate `.lekalo/observed/**` home would need a new authority
//! registration; the frozen matrix is not extended by this issue.

use serde::Serialize;

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::store;
use super::types::{BindingState, BindingStatus, ObservedIndex};
use super::version;

/// The wire identity of the baseline document: the observed family's
/// current generation, additive to the index identity.
pub const BASELINE_SCHEMA_VERSION: &str = "lekalo/observed-baseline/v0.2.16";

/// The baseline file name inside the observed-model-draft home.
pub const BASELINE_NAME: &str = "baseline.json";

/// The confirmed native gate plan identity recorded in the baseline:
/// the #48 plan digest plus the production run decision for it (the
/// production binary never launches a gate command; the typed refusal
/// is part of the recorded identity).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BaselineNativePlan {
    pub plan_digest: String,
    pub outcome: String,
    pub reason_codes: Vec<String>,
}

/// The baseline document: the deterministic metrics view of one index.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BaselineDocument {
    pub schema_version: &'static str,
    pub kind: &'static str,
    pub mode: &'static str,
    pub project: String,
    pub index_digest: String,
    pub revision: String,
    pub adapter: BaselineAdapter,
    pub counts: BaselineCounts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_plan: Option<BaselineNativePlan>,
}

/// The adapter identity that produced the baselined index.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BaselineAdapter {
    pub id: String,
    pub version: String,
    pub digest: Option<String>,
}

/// The binding-state counts of the baselined index.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BaselineCounts {
    pub symbols: usize,
    pub endpoints: usize,
    pub schemas: usize,
    pub test_bindings: usize,
    pub explicit: usize,
    pub confirmed: usize,
    pub inferred: usize,
    pub stale: usize,
    pub promoted: usize,
}

/// The receipt of `observe baseline`: what was written, where, and the
/// identity of the exact bytes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BaselineReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub mode: &'static str,
    pub project: String,
    pub path: &'static str,
    pub index_digest: String,
    pub baseline_digest: String,
    pub counts: BaselineCounts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_plan: Option<BaselineNativePlan>,
}

/// Build the baseline document over one loaded index (no I/O). The
/// canonical bytes are the compact serde rendering — fixed field order,
/// sorted records upstream, no timestamps. The index digest is
/// recomputed here: a loaded index is canonically verified, so a digest
/// failure propagates instead of degrading to an `unknown` spelling.
pub fn document_of(
    index: &ObservedIndex,
    native_plan: Option<BaselineNativePlan>,
) -> Result<BaselineDocument, DiagnosticSet> {
    Ok(BaselineDocument {
        schema_version: BASELINE_SCHEMA_VERSION,
        kind: "observed-baseline",
        mode: version::MODE,
        project: index.project.clone(),
        index_digest: super::index::index_digest(index)?,
        revision: index.revision.clone(),
        adapter: BaselineAdapter {
            id: index.adapter.id.clone(),
            version: index.adapter.version.clone(),
            digest: index.adapter.digest.clone(),
        },
        counts: counts_of(index),
        native_plan,
    })
}

/// The binding-state counts of one index (the accepted #39/#42 states).
pub fn counts_of(index: &ObservedIndex) -> BaselineCounts {
    BaselineCounts {
        symbols: index.symbols.len(),
        endpoints: index.endpoints.len(),
        schemas: index.schemas.len(),
        test_bindings: index.test_bindings.len(),
        explicit: index
            .symbols
            .iter()
            .filter(|record| record.status == BindingStatus::Explicit)
            .count(),
        confirmed: index
            .symbols
            .iter()
            .filter(|record| record.status == BindingStatus::Confirmed)
            .count(),
        inferred: index
            .symbols
            .iter()
            .filter(|record| record.status == BindingStatus::Inferred)
            .count(),
        stale: index
            .symbols
            .iter()
            .filter(|record| record.state == BindingState::Stale)
            .count(),
        promoted: index
            .symbols
            .iter()
            .filter(|record| record.promoted)
            .count(),
    }
}

/// The canonical bytes of one baseline document (compact, fixed field
/// order); over-bound documents refuse instead of truncating.
pub fn baseline_bytes(document: &BaselineDocument) -> Result<Vec<u8>, DiagnosticSet> {
    let bytes = serde_json::to_vec(document)
        .map_err(|_| diagnostic::index_io_set("baseline-unserializable"))?;
    if bytes.len() > version::MAX_INDEX_BYTES {
        return Err(diagnostic::scan_limit_set("baseline-bytes", bytes.len()));
    }
    Ok(bytes)
}

/// Record the baseline over the project's current observed index. The
/// native plan identity, when supplied, is validated through the #48
/// production run seam first (a refused or undecodable plan never
/// enters a baseline); the confirmed command surface itself is what the
/// plan records — the production binary never launches it.
pub fn record(
    ctx: &super::ObservedContext,
    native_plan: Option<BaselineNativePlan>,
) -> Result<BaselineReceipt, DiagnosticSet> {
    let index = super::index::load_index(ctx)?.ok_or_else(diagnostic::missing_index_set)?;
    let document = document_of(&index, native_plan)?;
    let bytes = baseline_bytes(&document)?;
    store::write_confined(&ctx.root, version::INDEX_DIR, BASELINE_NAME, &bytes)?;
    let baseline_digest = format!("sha256:{}", crate::digest::sha256_hex(&bytes));
    Ok(BaselineReceipt {
        status: "valid",
        operation: "baseline",
        mode: version::MODE,
        project: index.project.clone(),
        path: "lekalo.observed-model-draft:.lekalo/import/observed/baseline.json",
        index_digest: document.index_digest,
        baseline_digest,
        counts: document.counts,
        native_plan: document.native_plan,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observed::types::AdapterIdentity;

    fn empty_index() -> ObservedIndex {
        ObservedIndex::empty(
            "taskhub",
            AdapterIdentity {
                id: "lekalo-target-node-typescript".to_owned(),
                version: "0.4.0".to_owned(),
                digest: None,
            },
            format!("sha256:{}", "a".repeat(64)).as_str(),
        )
    }

    #[test]
    fn the_document_is_deterministic_and_counts_the_states() {
        let mut index = empty_index();
        let mut record = crate::observed::types::SymbolRecord {
            id: "taskhub.task".to_owned(),
            kind: crate::observed::types::SymbolKind::Entity,
            stable_key: Some("ts1-a".to_owned()),
            location: Some(crate::observed::types::SourceLocation {
                path: "src/tasks.ts".to_owned(),
                line: Some(1),
            }),
            fingerprint: Some(format!("sha256:{}", "b".repeat(64))),
            status: crate::observed::types::BindingStatus::Confirmed,
            state: crate::observed::types::BindingState::Current,
            promoted: false,
            promotion: None,
            evidence: Default::default(),
            provenance: crate::observed::types::Provenance {
                origin: crate::observed::types::Origin::Observed,
                confidence: crate::observed::types::Confidence::Medium,
                adapter: "lekalo-target-node-typescript".to_owned(),
                revision: format!("sha256:{}", "a".repeat(64)),
            },
            native_tests: Vec::new(),
            gates: Vec::new(),
            history: Vec::new(),
            candidates: Vec::new(),
        };
        index.symbols.push(record.clone());
        record.status = crate::observed::types::BindingStatus::Inferred;
        index.symbols.push(record.clone());
        record.state = crate::observed::types::BindingState::Stale;
        index.symbols.push(record);

        let document = document_of(&index, None).expect("document builds");
        assert_eq!(document.schema_version, BASELINE_SCHEMA_VERSION);
        assert_eq!(document.mode, "observed");
        assert_eq!(document.project, "taskhub");
        assert_eq!(document.counts.symbols, 3);
        assert_eq!(document.counts.confirmed, 1);
        assert_eq!(document.counts.inferred, 2);
        assert_eq!(document.counts.stale, 1);
        assert_eq!(document.counts.promoted, 0);
        assert!(document.native_plan.is_none());

        let bytes = baseline_bytes(&document).expect("baseline bytes");
        let again = document_of(&index, None).expect("document builds again");
        assert_eq!(
            document, again,
            "identical index produces identical documents"
        );
        let again_bytes = baseline_bytes(&again).expect("baseline bytes");
        assert_eq!(
            bytes, again_bytes,
            "identical index produces identical bytes"
        );
        assert!(!String::from_utf8_lossy(&bytes).contains("timestamp"));
    }

    #[test]
    fn the_native_plan_identity_is_carried_verbatim() {
        let index = empty_index();
        let plan = BaselineNativePlan {
            plan_digest: format!("sha256:{}", "c".repeat(64)),
            outcome: "unsupported".to_owned(),
            reason_codes: vec!["fixture-runner-not-shipped".to_owned()],
        };
        let document = document_of(&index, Some(plan.clone())).expect("document builds");
        assert_eq!(document.native_plan.as_ref(), Some(&plan));
        let bytes = baseline_bytes(&document).expect("baseline bytes");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("fixture-runner-not-shipped"), "{text}");
    }
}
