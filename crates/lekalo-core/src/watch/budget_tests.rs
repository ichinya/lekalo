use super::*;
use crate::watch::{snapshot, RunContext, WatchRequest, WatchService};
use std::{path::Path, sync::atomic::AtomicBool};
fn fixture() -> InputSnapshot {
    snapshot::capture(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/watch/project"),
        false,
    )
    .unwrap()
}
#[test]
fn model_admission_precedes_materialization_and_cannot_commit() {
    let mut captured = fixture();
    let record = captured.files.get_mut("lekalo/project.yaml").unwrap();
    let bytes = record.bytes.as_mut().unwrap();
    bytes.extend(std::iter::repeat(b' ').take(MAX_MODEL_BYTES as usize));
    record.len = bytes.len() as u64;
    record.sha256 = format!("sha256:{}", crate::digest::sha256_hex(bytes));
    let mut service = WatchService::new(WatchRequest::default()).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let outcome = service.run_revision(
        &captured,
        scratch.path(),
        &RunContext {
            run_id: 1,
            generation: 0,
            full_rescan: true,
            cancel: &AtomicBool::new(false),
        },
    );
    assert_eq!(outcome.status, crate::Status::Denied);
    assert!(outcome
        .result
        .diagnostics
        .iter()
        .any(|d| d.id() == "watch.resource-limit"));
    assert!(std::fs::read_dir(scratch.path()).unwrap().next().is_none());
    service.commit(outcome.staged);
    assert!(service.accepted().is_none());
}
#[test]
fn cache_pressure_preserves_mandatory_fingerprints_ownership_and_findings() {
    let mut service = WatchService::new(WatchRequest::default()).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let mut staged = service
        .run_revision(
            &fixture(),
            scratch.path(),
            &RunContext {
                run_id: 1,
                generation: 0,
                full_rescan: true,
                cancel: &AtomicBool::new(false),
            },
        )
        .staged;
    let mut mandatory = staged.clone();
    mandatory.documents.clear();
    mandatory.fragments = Default::default();
    mandatory.graph_fragments = Default::default();
    let limit = mandatory.charged_bytes();
    assert!(staged.charged_bytes() > limit);
    staged.fit_budget(limit).unwrap();
    assert!(staged.documents.is_empty());
    assert_eq!(staged.fragments.charged_bytes(), 0);
    assert_eq!(staged.graph_fragments.charged_bytes(), 0);
    assert_eq!(staged.fingerprints, mandatory.fingerprints);
    assert_eq!(
        staged.module_index.owned_paths,
        mandatory.module_index.owned_paths
    );
    assert_eq!(staged.module_index.imports, mandatory.module_index.imports);
    assert_eq!(staged.partitions, mandatory.partitions);
    assert_eq!(staged.global_partition, mandatory.global_partition);
    assert!(staged.fit_budget(0).is_err());
}
#[test]
fn ast_charge_counts_allocated_capacity_not_only_live_text() {
    let root = crate::loader::frontends::Node::scalar(
        Scalar::Str(String::with_capacity(1024)),
        crate::loader::error::Span::EMPTY_EOF,
    );
    assert!(node_heap(&root) >= 1024);
    let sequence = Node {
        span: root.span,
        value: Value::Seq(Vec::with_capacity(100)),
    };
    assert!(node_heap(&sequence) >= 100 * size_of::<Node>());
}
