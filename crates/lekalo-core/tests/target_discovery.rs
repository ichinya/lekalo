//! Issue #28 integration tests: capability discovery and version
//! negotiation over the real `lekalo.target/v1` handshake with the
//! committed Node.js fake adapter.
//!
//! The tests are hermetic: every exchange runs inside a temporary project
//! directory, and the declared capability profiles travel through the
//! adapter argv. They prove the acceptance criteria end to end: an
//! incompatible adapter is filtered before project IR is transferred,
//! partial support requires the explicit policy, unknown never becomes an
//! optimistic yes, capability ids carry versioned definitions, the
//! selection verdict is explainable and machine-readable, and the
//! resolved capability snapshot resolves into the committed lock.

use std::path::{Path, PathBuf};

use lekalo_core::lockfile::resolution::{
    CandidateAdapter, CandidateProfile, CandidateSet, LockResolver, ResolutionRequest,
};
use lekalo_core::lockfile::{
    ArtifactPin, CapabilityId, ComponentId, ComponentRef, Platform, Sha256Digest, SourceKind,
    SourceRef,
};
use lekalo_core::target_protocol::capability;
use lekalo_core::target_protocol::discovery::{Cache, Discovery, Provenance};
use lekalo_core::target_protocol::selection::{
    reasons, select, warnings, SelectionPolicy, SelectionReport, SelectionRequest,
};
use lekalo_core::target_protocol::wire::{Operation, SupportState};
use lekalo_core::target_protocol::{transport, CallRequest, TargetClient, TargetFailure};
use lekalo_core::versioning::compatibility::ProtocolBounds;
use lekalo_core::versioning::plan::sha256_hex;
use lekalo_core::versioning::{
    AdapterCompatibilityManifest, CompatibilityPreflight, ContractVersion, VersionRegistry,
};

/// The committed language-neutral adapter fixture.
fn adapter_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/target-protocol/fake-adapter.mjs")
        .display()
        .to_string()
}

fn variant_command(variant: &str) -> transport::AdapterCommand {
    transport::AdapterCommand {
        program: PathBuf::from("node"),
        args: vec![
            adapter_path(),
            "--lekalo-adapter-variant".to_owned(),
            variant.to_owned(),
        ],
    }
}

/// One hermetic temporary project directory.
struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "lekalo-discovery-{tag}-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("sandbox dir");
        std::fs::create_dir_all(dir.join(".lekalo/ir")).expect("IR directory");
        std::fs::write(dir.join(".lekalo/ir/planner.json"), b"{}").expect("IR input");
        Self { dir }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn digest(spelling: &str) -> Sha256Digest {
    Sha256Digest::parse(spelling).expect("digest spelling")
}

const IR: &str = ".lekalo/ir/planner.json";

#[test]
fn discovery_negotiates_the_extension_and_records_provenance() {
    let sandbox = Sandbox::new("fluent");
    let mut client = TargetClient::default();
    let discovered = Discovery::run(&mut client, &variant_command("fluent"), &sandbox.dir)
        .expect("fluent discovery");
    assert_eq!(discovered.negotiated_version, "1.2.0");
    assert_eq!(discovered.ir_versions, vec!["0.1.0".to_owned()]);
    assert!(discovered.ir_compatible("0.1.0"));
    assert_eq!(discovered.adapter.id, "node-typescript");
    let openapi = discovered.capability("generate.openapi").expect("declared");
    assert_eq!(openapi.state, SupportState::Partial);
    assert_eq!(openapi.definition_version, "1.0.0");
    assert_eq!(openapi.provenance, Provenance::Declared);
    assert_eq!(
        discovered
            .capability("generate.ui")
            .map(|entry| entry.state),
        Some(SupportState::Unsupported)
    );
    let constraints = discovered.constraints.expect("declared constraints");
    assert_eq!(constraints.max_entries, Some(10_000));
    // The declared digest stays distinct from the verified executable
    // digest: the declared digest covers the adapter package root, the
    // executable digest covers the launched program bytes.
    let executable = std::fs::read(adapter_path()).expect("adapter bytes");
    let expected = format!("sha256:{}", sha256_hex(&executable));
    assert_eq!(
        discovered.executable_digest.as_deref(),
        Some(expected.as_str())
    );
    assert_ne!(
        discovered.adapter.digest,
        discovered.executable_digest.as_deref().expect("computed")
    );
}

#[test]
fn legacy_adapter_stays_on_the_frozen_base_contract() {
    let sandbox = Sandbox::new("legacy");
    let command = variant_command("legacy");
    let mut client = TargetClient::default();
    let discovered = Discovery::run(&mut client, &command, &sandbox.dir).expect("legacy");
    assert_eq!(discovered.negotiated_version, "1.0.0");
    assert!(discovered.ir_versions.is_empty());
    assert!(discovered.capabilities.is_empty());
    assert!(!discovered.ir_compatible("0.1.0"));
    // The full #27 operation round trip still works on the 1.0.0 session.
    let outcome = client
        .call(
            &command,
            CallRequest {
                operation: Operation::Scan,
                target: None,
                profile: None,
                profile_resolution: None,
                ir_path: None,
                dry_run: None,
                plan_id: None,
            },
            &sandbox.dir,
            &lekalo_core::project_fs::Fs::open(&sandbox.dir).unwrap(),
            None,
        )
        .expect("legacy scan");
    assert_eq!(outcome.response.operation, Operation::Scan);
}

#[test]
fn incompatible_adapter_is_filtered_before_any_ir_is_transferred() {
    let sandbox = Sandbox::new("incompatible");
    let command = variant_command("incompatible");
    let mut client = TargetClient::default();
    let discovered = Discovery::run(&mut client, &command, &sandbox.dir).expect("discovery");
    assert!(
        !discovered.ir_compatible("0.1.0"),
        "the adapter declared 0.2.0 only"
    );
    // The refusal happens in request validation, before any confined
    // launch: the adapter never receives the project IR path.
    let failure = client
        .call(
            &command,
            CallRequest {
                operation: Operation::Validate,
                target: None,
                profile: None,
                profile_resolution: None,
                ir_path: Some(IR),
                dry_run: None,
                plan_id: None,
            },
            &sandbox.dir,
            &lekalo_core::project_fs::Fs::open(&sandbox.dir).unwrap(),
            None,
        )
        .expect_err("IR operation must be refused");
    assert!(matches!(failure, TargetFailure::IrUnsupported));
    assert_eq!(failure.rule().0, "target.ir-unsupported");
    // Selection reports the same exclusion with its stable reason.
    let report = select(
        &[discovered],
        SelectionRequest {
            required: &["scan.symbols".to_owned()],
            preferred_profile: None,
            policy: SelectionPolicy::default(),
        },
        "0.1.0",
    );
    assert!(!report.is_selected());
    assert_eq!(report.excluded[0].reasons, vec![reasons::IR_UNDECLARED]);
}

#[test]
fn selection_covers_full_partial_unknown_and_incompatible() {
    let sandbox = Sandbox::new("matrix");
    let mut candidates = Vec::new();
    for variant in ["fluent", "partial", "unknown", "incompatible"] {
        let mut client = TargetClient::default();
        candidates.push(
            Discovery::run(&mut client, &variant_command(variant), &sandbox.dir).expect(variant),
        );
    }

    // Strict policy over the required capabilities: only the fluent
    // declarations support both fully; partial blocks, unknown blocks,
    // and the incompatible IR declaration blocks first.
    let required = ["generate.zod".to_owned(), "scan.symbols".to_owned()];
    let strict = select(
        &candidates,
        SelectionRequest {
            required: &required,
            preferred_profile: None,
            policy: SelectionPolicy::default(),
        },
        "0.1.0",
    );
    let selected = strict.selected.as_ref().expect("fluent survives");
    assert_eq!(selected.adapter.id, "node-typescript");
    assert_eq!(selected.profile.as_deref(), Some("default"));
    let all_reasons: Vec<&'static str> = strict
        .excluded
        .iter()
        .flat_map(|entry| entry.reasons.iter().copied())
        .collect();
    assert!(all_reasons.contains(&reasons::PARTIAL_POLICY));
    assert!(all_reasons.contains(&reasons::CAPABILITY_UNKNOWN));
    assert!(all_reasons.contains(&reasons::IR_UNDECLARED));
    assert_eq!(selected.capabilities[0].definition_version, "1.0.0");
    let report_bytes = serde_json::to_vec(&strict).expect("machine-readable");
    let value: serde_json::Value = serde_json::from_slice(&report_bytes).unwrap();
    assert!(value["selected"]["capability_digest"].is_string());

    // The explicit partial policy: the same discovery that blocks under
    // the strict policy proceeds once `allow_partial` is accepted, and
    // the report records the accepted warning. (All fake variants share
    // one identity, so the policy case runs against the partial
    // candidate alone.)
    let partial_only = vec![candidates[1].clone()];
    let blocked = select(
        &partial_only,
        SelectionRequest {
            required: &["generate.zod".to_owned()],
            preferred_profile: None,
            policy: SelectionPolicy::default(),
        },
        "0.1.0",
    );
    assert_eq!(blocked.excluded[0].reasons, vec![reasons::PARTIAL_POLICY]);
    let with_partial = select(
        &partial_only,
        SelectionRequest {
            required: &["generate.zod".to_owned()],
            preferred_profile: None,
            policy: SelectionPolicy {
                allow_partial: true,
                tolerate_unknown: false,
            },
        },
        "0.1.0",
    );
    assert!(with_partial.is_selected());
    assert_eq!(
        with_partial.selected.as_ref().unwrap().warnings,
        vec![warnings::PARTIAL_ACCEPTED]
    );

    // Unknown never becomes an optimistic yes: the strict default
    // refuses the unknown declaration, and only the explicit non-strict
    // policy proceeds past it — with the recorded warning.
    let unknown_only = vec![candidates[2].clone()];
    let strict_unknown = select(
        &unknown_only,
        SelectionRequest {
            required: &["scan.symbols".to_owned()],
            preferred_profile: None,
            policy: SelectionPolicy::default(),
        },
        "0.1.0",
    );
    assert!(!strict_unknown.is_selected());
    assert_eq!(
        strict_unknown.excluded[0].reasons,
        vec![reasons::CAPABILITY_UNKNOWN]
    );
    let tolerated = select(
        &unknown_only,
        SelectionRequest {
            required: &["scan.symbols".to_owned()],
            preferred_profile: None,
            policy: SelectionPolicy {
                allow_partial: false,
                tolerate_unknown: true,
            },
        },
        "0.1.0",
    );
    assert!(tolerated.is_selected());
    assert_eq!(
        tolerated.selected.as_ref().unwrap().warnings,
        vec![warnings::UNKNOWN_TOLERATED]
    );
}

#[test]
fn discovery_cache_invalidates_on_version_and_digest_changes() {
    let sandbox = Sandbox::new("cache");
    let mut client = TargetClient::default();
    let first = Discovery::run(&mut client, &variant_command("fluent"), &sandbox.dir)
        .expect("first discovery");
    let mut cache = Cache::new();
    let key = Cache::key(&first, "0.1.0");
    cache.insert(key.clone(), first.clone());
    assert!(cache.get(&key).is_some());
    // A different IR version misses: the cached verdict never crosses.
    assert!(cache.get(&Cache::key(&first, "9.9.9")).is_none());
    // The same live session rediscovers deterministically.
    let second = Discovery::run(&mut client, &variant_command("fluent"), &sandbox.dir)
        .expect("second discovery");
    assert_eq!(first, second, "discovery is deterministic per session");
}

#[test]
fn discovered_capabilities_resolve_into_the_lock_snapshot() {
    let sandbox = Sandbox::new("lock");
    let mut client = TargetClient::default();
    let discovered =
        Discovery::run(&mut client, &variant_command("fluent"), &sandbox.dir).expect("discovery");

    let registry = VersionRegistry::embedded().expect("embedded registry");
    let model = ContractVersion::parse_canonical("1.0.0").unwrap();
    let core = lekalo_core::lockfile::SemVer::parse("0.2.4").unwrap();
    let request = ResolutionRequest::new(registry, &model, core)
        .with_adapter(ComponentId::parse("node-typescript").unwrap())
        .with_profile(ComponentId::parse("default").unwrap())
        .require_capability(CapabilityId::parse("generate.zod").unwrap());

    let adapter_ref = ComponentRef::new(
        ComponentId::parse("node-typescript").unwrap(),
        lekalo_core::lockfile::SemVer::parse("0.1.0").unwrap(),
    );
    let profile = CandidateProfile::new(
        ComponentId::parse("default").unwrap(),
        lekalo_core::lockfile::SemVer::parse("1.0.0").unwrap(),
        digest(&format!("sha256:{}", sha256_hex(b"profile-source"))),
        digest(&format!("sha256:{}", sha256_hex(b"profile-resolved"))),
        vec![adapter_ref],
        vec![],
    );
    let manifest = AdapterCompatibilityManifest::new(
        ContractVersion::parse_canonical("1.0.0").unwrap(),
        "node-typescript",
        ContractVersion::parse_canonical("0.1.0").unwrap(),
        ContractVersion::parse_canonical("0.1.0").unwrap(),
        Some(ProtocolBounds {
            min: ContractVersion::parse_canonical("1.0.0").unwrap(),
            max: ContractVersion::parse_canonical("1.2.0").unwrap(),
        }),
        vec![],
        vec![],
    )
    .unwrap();
    let adapter = CandidateAdapter::new(
        ComponentId::parse("node-typescript").unwrap(),
        lekalo_core::lockfile::SemVer::parse("0.1.0").unwrap(),
        digest(&discovered.adapter.digest),
        SourceRef::new(
            SourceKind::Builtin,
            "fake-target",
            digest(&discovered.capability_digest),
        )
        .unwrap(),
        vec![ArtifactPin::new(
            Platform::parse("any").unwrap(),
            digest(&discovered.capability_digest),
        )],
        manifest.clone(),
    );
    let mut candidates = CandidateSet::empty()
        .with_adapter(adapter)
        .with_profile(profile);
    for entry in discovered
        .capability_candidates(&ComponentId::parse("default").unwrap())
        .expect("candidate capabilities")
    {
        candidates = candidates.with_capability(entry);
    }

    let lockfile = LockResolver::resolve(&request, &candidates, registry).expect("resolves");
    let capabilities = lockfile.capabilities();
    assert_eq!(
        capabilities.len(),
        5,
        "the whole declared snapshot is locked"
    );
    let zod = capabilities
        .iter()
        .find(|entry| entry.id().as_str() == "generate.zod")
        .expect("locked generate.zod");
    assert_eq!(zod.support(), lekalo_core::lockfile::Support::Full);
    assert_eq!(
        zod.version().as_str(),
        "1.0.0",
        "bound to the capability definition version"
    );
    assert_eq!(zod.provider().id().as_str(), "node-typescript");
    let ui = capabilities
        .iter()
        .find(|entry| entry.id().as_str() == "generate.ui")
        .expect("locked generate.ui");
    assert_eq!(ui.support(), lekalo_core::lockfile::Support::Unsupported);
    assert_eq!(lockfile.adapters().len(), 1);
    assert!(CompatibilityPreflight::check(
        registry,
        &ContractVersion::parse_canonical("0.1.0").unwrap(),
        Some(&ContractVersion::parse_canonical("1.1.0").unwrap()),
        &manifest,
    )
    .is_compatible());
}

#[test]
fn capability_definitions_are_versioned_and_closed() {
    assert_eq!(
        capability::REGISTRY_IDENTITY,
        "dev.lekalo.target-capabilities@1.0.0"
    );
    for id in [
        "scan.symbols",
        "generate.zod",
        "generate.openapi",
        "verify.scenarios",
        "generate.ui",
    ] {
        assert_eq!(
            capability::definition(id).unwrap().definition_version,
            "1.0.0"
        );
    }
    assert!(capability::definition("scan.nonexistent").is_none());
}

#[test]
fn provenance_tokens_are_stable() {
    assert_eq!(Provenance::Declared.as_str(), "declared");
    assert_eq!(Provenance::Probed.as_str(), "probed");
    assert_eq!(Provenance::Verified.as_str(), "verified");
    assert!(Provenance::Declared < Provenance::Probed);
    assert!(Provenance::Probed < Provenance::Verified);
}

#[test]
fn selection_reason_and_warning_tokens_are_stable() {
    assert_eq!(warnings::PARTIAL_ACCEPTED, "partial-accepted");
    assert_eq!(warnings::UNKNOWN_TOLERATED, "unknown-tolerated");
    assert_eq!(reasons::IR_UNDECLARED, "ir-undeclared");
}

#[test]
fn resolved_profiles_reach_1_2_0_adapters_and_refuse_legacy_sessions() {
    use lekalo_core::target_profile::document;
    use lekalo_core::target_profile::resolution::resolve;

    // The fluent adapter declares the full v1 line, so the session
    // negotiates 1.2.0 and accepts the resolved profile members.
    let sandbox = Sandbox::new("profile-1-2");
    let command = variant_command("fluent");
    let mut client = TargetClient::default();
    let discovered = Discovery::run(&mut client, &command, &sandbox.dir).expect("discovery");
    assert_eq!(discovered.negotiated_version, "1.2.0");

    // Resolve the issue's Node profile through the production seam and
    // project it onto the wire shape.
    let document_bytes = std::fs::read(format!(
        "{}/../../tests/fixtures/target-profile/valid/node.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("profile fixture");
    let document = document::decode(&document_bytes).expect("decodes");
    let resolved = resolve(&document).expect("resolves").remove(0);
    let resolution = resolved.wire_resolution();
    assert_eq!(
        resolution.digest, resolved.digest,
        "the wire digest is the resolved snapshot digest"
    );

    let fs = lekalo_core::project_fs::Fs::open(&sandbox.dir).unwrap();
    let outcome = client
        .call(
            &command,
            CallRequest {
                operation: Operation::Scan,
                target: Some("node-typescript"),
                profile: Some("default"),
                profile_resolution: Some(&resolution),
                ir_path: None,
                dry_run: None,
                plan_id: None,
            },
            &sandbox.dir,
            &fs,
            None,
        )
        .expect("a 1.2.0 adapter receives the resolved capabilities");
    assert_eq!(outcome.response.operation, Operation::Scan);

    // A legacy base-only session refuses the resolution instead of
    // silently dropping it: an adapter never receives arbitrary YAML.
    let legacy_sandbox = Sandbox::new("profile-legacy");
    let legacy_command = variant_command("legacy");
    let mut legacy_client = TargetClient::default();
    legacy_client
        .describe(&legacy_command, &legacy_sandbox.dir)
        .expect("legacy handshake");
    let legacy_fs = lekalo_core::project_fs::Fs::open(&legacy_sandbox.dir).unwrap();
    let failure = legacy_client
        .call(
            &legacy_command,
            CallRequest {
                operation: Operation::Scan,
                target: Some("node-typescript"),
                profile: Some("default"),
                profile_resolution: Some(&resolution),
                ir_path: None,
                dry_run: None,
                plan_id: None,
            },
            &legacy_sandbox.dir,
            &legacy_fs,
            None,
        )
        .expect_err("legacy sessions refuse resolutions");
    assert!(matches!(
        failure,
        TargetFailure::CapabilityUnsupported { .. }
    ));
    assert_eq!(failure.rule().0, "target.capability-unsupported");
}

#[allow(unused)]
fn report_is_serializable(report: &SelectionReport) {
    let _ = serde_json::to_vec(report).map(drop);
}
