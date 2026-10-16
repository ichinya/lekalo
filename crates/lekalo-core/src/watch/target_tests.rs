use super::*;
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[path = "target_cancel_tests.rs"]
mod cancellation;
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
#[path = "target_resource_tests.rs"]
mod resources;
use crate::watch::{
    snapshot::{self, CapturePolicy},
    RunContext, WatchService,
};

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[path = "target_selective_tests.rs"]
mod selective;

fn project() -> tempfile::TempDir {
    project_selective(false)
}
fn project_selective(selective: bool) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/watch/project");
    snapshot::materialize(&snapshot::capture(&fixture, false).unwrap(), root.path()).unwrap();
    std::fs::create_dir(root.path().join("adapters")).unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    let script = include_str!("../../../../tests/fixtures/watch/mock-adapter.mjs").replace(
        "const selective = false",
        if selective {
            "const selective = true"
        } else {
            "const selective = false"
        },
    );
    std::fs::write(root.path().join("adapters/fixture.mjs"), script.as_bytes()).unwrap();
    std::fs::write(root.path().join("src/control.txt"), "ok").unwrap();
    use crate::lockfile::{
        resolution::{CandidateAdapter, CandidateSet},
        types::*,
    };
    use crate::versioning::{
        compatibility::{AdapterCompatibilityManifest, ProtocolBounds},
        family::{IrContract, ProtocolContract, RegistryContract},
        ContractVersion,
    };
    let digest = Sha256Digest::parse(&format!(
        "sha256:{}",
        crate::digest::sha256_hex(script.as_bytes())
    ))
    .unwrap();
    let manifest = AdapterCompatibilityManifest::new(
        ContractVersion::<RegistryContract>::parse_canonical(
            crate::versioning::compatibility::MANIFEST_SCHEMA_VERSION,
        )
        .unwrap(),
        "watch-fixture".to_owned(),
        ContractVersion::<IrContract>::current(),
        ContractVersion::<IrContract>::current(),
        Some(ProtocolBounds {
            min: ContractVersion::<ProtocolContract>::parse_canonical(
                crate::target_protocol::version::VERSION,
            )
            .unwrap(),
            max: ContractVersion::<ProtocolContract>::parse_canonical(
                crate::target_protocol::version::VERSION,
            )
            .unwrap(),
        }),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let candidate = CandidateAdapter::new(
        ComponentId::parse("watch-fixture").unwrap(),
        SemVer::parse("0.8.0").unwrap(),
        digest.clone(),
        SourceRef::new(SourceKind::Project, "adapters/fixture.mjs", digest.clone()).unwrap(),
        vec![ArtifactPin::new(Platform::parse("any").unwrap(), digest)],
        manifest,
    );
    let candidates = CandidateSet::empty().with_adapter(candidate);
    let request = crate::lockfile::plan::LockService::request_at(root.path())
        .unwrap()
        .with_adapter(crate::lockfile::ComponentId::parse("watch-fixture").unwrap());
    let lock = crate::lockfile::resolution::LockResolver::resolve(
        &request,
        &candidates,
        crate::versioning::VersionRegistry::embedded().unwrap(),
    )
    .unwrap();
    std::fs::write(root.path().join("lekalo.lock"), lock.canonical_bytes()).unwrap();
    root
}
fn execute(
    root: &Path,
    policy: &CapturePolicy,
    timeout: u64,
    cancelled: bool,
) -> crate::watch::RunOutcome {
    let captured = snapshot::capture_with_policy(root, policy).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let mut service = WatchService::new(WatchRequest {
        targets: vec!["watch-fixture".to_owned()],
        adapter_timeout_ms: timeout,
        ..WatchRequest::default()
    })
    .unwrap();
    service.run_revision(
        &captured,
        scratch.path(),
        &RunContext {
            run_id: 1,
            generation: 0,
            full_rescan: true,
            cancel: &AtomicBool::new(cancelled),
        },
    )
}
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn policy(root: &Path) -> CapturePolicy {
    CapturePolicy {
        target_roots: preflight(root, &["watch-fixture".to_owned()], 5000).unwrap(),
        generate_check: false,
    }
}
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[test]
fn scan_and_validation_are_ephemeral_and_read_only_including_write_probe() {
    let root = project();
    let policy = policy(root.path());
    assert!(policy.target_roots.contains("src"));
    assert!(policy.target_roots.contains("adapters/fixture.mjs"));
    assert!(!policy
        .target_roots
        .iter()
        .any(|path| path.starts_with(".lekalo/cache")));
    let modes = if cfg!(windows) || cfg!(target_os = "macos") {
        ["ok", "write", "child"]
    } else {
        ["ok", "write", "linux-limits"]
    };
    for mode in modes {
        std::fs::write(root.path().join("src/control.txt"), mode).unwrap();
        let before = snapshot::capture_with_policy(root.path(), &policy).unwrap();
        let outcome = execute(root.path(), &policy, 5000, false);
        assert_eq!(
            outcome.status,
            Status::Valid,
            "{mode}: {:?}",
            outcome.result.diagnostics
        );
        assert!(outcome
            .result
            .stages
            .iter()
            .any(|r| r.stage == "target-scan" && r.state == "executed"));
        assert_eq!(outcome.result.pins.adapters.len(), 1);
        assert_eq!(outcome.result.pins.adapters[0].protocol_version, "0.3.2");
        assert!(outcome.result.pins.revisions.lock_digest.is_some());
        assert!(crate::watch::result_value_is_wellformed(
            &serde_json::to_value(&outcome.result).unwrap()
        ));
        assert_eq!(
            snapshot::capture_with_policy(root.path(), &policy).unwrap(),
            before
        );
        assert!(!root.path().join(".lekalo/cache").exists());
        assert!(!root.path().join(".lekalo/observed").exists());
    }
}
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[test]
fn crash_timeout_invalid_scan_and_later_recovery_do_not_destroy_core_results() {
    let root = project();
    let policy = policy(root.path());
    let mut service = WatchService::new(WatchRequest {
        targets: vec!["watch-fixture".to_owned()],
        adapter_timeout_ms: 5000,
        ..WatchRequest::default()
    })
    .unwrap();
    // Every outcome is committed into the SAME service. Constructing a
    // fresh service for each mode would not prove session recovery.
    for (index, (mode, status)) in [
        ("crash", Status::Unavailable),
        ("hang", Status::Unavailable),
        ("bad-scan", Status::Unavailable),
        ("bad-detail", Status::Invalid),
        ("invalid", Status::Invalid),
        ("ok", Status::Valid),
        ("crash", Status::Unavailable),
        ("ok", Status::Valid),
    ]
    .into_iter()
    .enumerate()
    {
        std::fs::write(root.path().join("src/control.txt"), mode).unwrap();
        let captured = snapshot::capture_with_policy(root.path(), &policy).unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let outcome = service.run_revision(
            &captured,
            scratch.path(),
            &RunContext {
                run_id: index as u64 + 1,
                generation: index as u64,
                full_rescan: index == 0,
                cancel: &AtomicBool::new(false),
            },
        );
        assert_eq!(
            outcome.status, status,
            "{mode}: {:?}",
            outcome.result.diagnostics
        );
        assert!(outcome
            .result
            .stages
            .iter()
            .any(|r| r.stage == "validate" && r.state == "executed"));
        if status == Status::Unavailable {
            assert!(outcome
                .result
                .diagnostics
                .iter()
                .any(|d| d.id() == "watch.adapter-recovery"));
        } else if status == Status::Valid {
            assert!(outcome
                .result
                .diagnostics
                .iter()
                .all(|d| d.id() != "watch.adapter-recovery"));
            assert_eq!(outcome.result.pins.adapters.len(), 1);
        }
        service.commit(outcome.staged);
        assert_eq!(
            service.accepted().unwrap().revision,
            outcome.source_revision
        );
    }
}
#[test]
fn checksum_refusal_precedes_execution_and_capture_cannot_fall_back_to_live_entry() {
    let root = project();
    let policy = CapturePolicy {
        target_roots: BTreeSet::from(["src".to_owned(), "adapters/fixture.mjs".to_owned()]),
        ..CapturePolicy::default()
    };
    std::fs::write(
        root.path().join("adapters/fixture.mjs"),
        "throw new Error('must never execute');",
    )
    .unwrap();
    let refused = execute(root.path(), &policy, 5000, false);
    assert_eq!(refused.status, Status::Denied);
    assert!(refused
        .result
        .diagnostics
        .iter()
        .any(|d| d.id() == "lock.digest-mismatch"));
    assert!(!refused
        .result
        .diagnostics
        .iter()
        .any(|d| d.id().starts_with("target.")));
    let cancelled = execute(root.path(), &policy, 5000, true);
    assert!(cancelled.stale);
    assert!(cancelled.staged.revision.is_empty());
}
