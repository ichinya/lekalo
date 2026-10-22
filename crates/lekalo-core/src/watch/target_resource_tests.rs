// The legacy sandbox intentionally records unenforced RSS/process dimensions
// on non-Windows hosts. Watch must refuse before executing an adapter there.
use super::*;
#[test]
fn unbounded_host_refuses_target_execution_without_claiming_runtime_coverage() {
    let root = project();
    let refused = preflight(root.path(), &["watch-fixture".to_owned()], 5000).unwrap_err();
    assert_eq!(refused.status(), Status::Unsupported);
    let policy = CapturePolicy {
        target_roots: BTreeSet::from(["src".to_owned(), "adapters/fixture.mjs".to_owned()]),
        ..CapturePolicy::default()
    };
    let result = execute(root.path(), &policy, 5000, false);
    assert_eq!(result.status, Status::Unsupported);
    assert!(result.result.pins.adapters.is_empty());
    assert!(result
        .result
        .stages
        .iter()
        .any(|r| r.stage == "validate" && r.state == "executed"));
}
