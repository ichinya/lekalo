//! Issue #10 lockfile integration tests: wire classification, canonical
//! determinism, hermetic resolution, verification with tamper detection,
//! plan/apply transaction behavior, and path-safety refusals.
//!
//! The tests are hermetic: every temporary project is copied from the
//! committed reference fixture into a numbered directory under the
//! package's `target/` tree, candidate and inventory values are constructed
//! in memory, and no test touches the network, spawns processes, or reads
//! anything outside the workspace and its own roots.

use std::path::Path;

use lekalo_core::loader::LoadSelection;
use lekalo_core::lockfile::plan::LockService;
use lekalo_core::lockfile::resolution::{
    CandidateAdapter, CandidateCapability, CandidateGenerator, CandidateProfile, CandidateSet,
    LockResolver, ResolutionRequest,
};
use lekalo_core::lockfile::types::{
    ArtifactPin, CapabilityId, ComponentId, ComponentRef, Platform, ProviderKind, ProviderRef,
    SemVer, Sha256Digest, SourceKind, SourceRef, Support, RESOLVER_VERSION,
};
use lekalo_core::lockfile::verify::{
    InventoryArtifact, InventoryComponent, LockRequirement, LockVerdict, LockVerifier,
    RuntimeInventory,
};
use lekalo_core::lockfile::{LockFailure, Lockfile};
use lekalo_core::versioning::compatibility::{AdapterCompatibilityManifest, ProtocolBounds};
use lekalo_core::versioning::family::{
    IrContract, ModelContract, ProtocolContract, RegistryContract,
};
use lekalo_core::versioning::{ContractVersion, VersionRegistry};

const GOLDEN: &[u8] =
    include_bytes!("../../../tests/fixtures/lockfile/valid/contract-only.lock.json");
const GOLDEN_DIGEST: &str =
    "sha256:a7d9178f099094629e1e1ac00284bfef32d6894e4b416853d8af1ac08a7689f5";
const MULTI: &[u8] =
    include_bytes!("../../../tests/fixtures/lockfile/valid/multi-adapter.lock.json");
const REFERENCE_PROJECT: &str = "../../tests/fixtures/lockfile/project";

fn ir_version() -> ContractVersion<IrContract> {
    ContractVersion::<IrContract>::current()
}

fn model_1_0() -> ContractVersion<ModelContract> {
    ContractVersion::<ModelContract>::parse_canonical("1.0.0").expect("1.0.0 is canonical")
}

/// The synthetic published-protocol world's current version: the
/// candidate-resolution tests below declare manifest bounds inside it.
fn protocol_version() -> ContractVersion<ProtocolContract> {
    ContractVersion::<ProtocolContract>::parse_canonical("1.1.0").expect("1.1.0 is canonical")
}

/// A synthetic registry identical to the embedded one except that the
/// protocol family publishes the base 1.0.0 and the #28 extension 1.1.0
/// (current) — the published-protocol world the multi-adapter resolver
/// tests run in.
fn published_protocol_registry() -> VersionRegistry {
    const JSON: &str = r#"
{
  "registry": "dev.lekalo.version-registry",
  "registryVersion": "1.0.0",
  "families": {
    "model": {
      "current": "1.0.0",
      "aliases": [],
      "versions": [
        {"version": "0.1.0", "state": "deprecated", "deprecatedSince": "1.0.0",
         "retirementNotBefore": "2.0.0", "classification": "additive",
         "reason": "Initial Model contract; deprecated by Model 1.0.0."},
        {"version": "1.0.0", "state": "supported", "classification": "breaking",
         "reason": "Model 1.0.0 tightened the module-ID grammar."}
      ],
      "migrations": []
    },
    "ir": {
      "current": "0.1.0",
      "aliases": [],
      "versions": [
        {"version": "0.1.0", "state": "supported", "classification": "additive",
         "reason": "The accepted IR contract."}
      ],
      "migrations": []
    },
    "protocol": {
      "current": "1.1.0",
      "aliases": [],
      "versions": [
        {"version": "1.0.0", "state": "supported", "classification": "additive",
         "reason": "Synthetic published protocol for issue #10 hermetic tests."},
        {"version": "1.1.0", "state": "supported", "classification": "additive",
         "reason": "Synthetic protocol extension for the capability discovery issue."}
      ],
      "migrations": []
    }
  }
}
"#;
    VersionRegistry::from_bytes(JSON.as_bytes()).expect("synthetic registry is valid")
}

fn request(registry: &VersionRegistry) -> ResolutionRequest {
    ResolutionRequest::new(
        registry,
        &model_1_0(),
        SemVer::parse(lekalo_core::lockfile::PRODUCT_VERSION).expect("product version"),
    )
}

fn manifest_for(adapter: &str) -> AdapterCompatibilityManifest {
    AdapterCompatibilityManifest::new(
        ContractVersion::<RegistryContract>::parse_canonical(
            lekalo_core::versioning::compatibility::MANIFEST_SCHEMA_VERSION,
        )
        .expect("manifest schema version is canonical"),
        adapter,
        ContractVersion::<IrContract>::parse_canonical("0.1.0").expect("ir min"),
        ContractVersion::<IrContract>::parse_canonical("0.1.0").expect("ir max"),
        Some(ProtocolBounds {
            min: protocol_version(),
            max: protocol_version(),
        }),
        Vec::new(),
        Vec::new(),
    )
    .expect("manifest is valid")
}

fn adapter_candidate(id: &str, version: &str, digest_byte: u8, platform: &str) -> CandidateAdapter {
    CandidateAdapter::new(
        ComponentId::parse(id).expect("adapter id"),
        SemVer::parse(version).expect("adapter version"),
        Sha256Digest::from_hex(&hex64(digest_byte)),
        SourceRef::new(
            SourceKind::Catalog,
            id,
            Sha256Digest::from_hex(&hex64(digest_byte.wrapping_add(1))),
        )
        .expect("source"),
        vec![ArtifactPin::new(
            Platform::parse(platform).expect("platform"),
            Sha256Digest::from_hex(&hex64(digest_byte.wrapping_add(2))),
        )],
        manifest_for(id),
    )
}

fn generator_candidate(id: &str, version: &str, adapter: &str) -> CandidateGenerator {
    CandidateGenerator::new(
        ComponentId::parse(id).expect("generator id"),
        SemVer::parse(version).expect("generator version"),
        Sha256Digest::from_hex(&hex64(0x30)),
        ComponentRef::new(
            ComponentId::parse(adapter).expect("adapter ref id"),
            SemVer::parse("0.1.2").expect("adapter ref version"),
        ),
    )
}

fn profile_candidate(id: &str, version: &str, adapter: &str) -> CandidateProfile {
    CandidateProfile::new(
        ComponentId::parse(id).expect("profile id"),
        SemVer::parse(version).expect("profile version"),
        Sha256Digest::from_hex(&hex64(0x40)),
        Sha256Digest::from_hex(&hex64(0x41)),
        vec![ComponentRef::new(
            ComponentId::parse(adapter).expect("profile adapter id"),
            SemVer::parse("0.1.2").expect("profile adapter version"),
        )],
        Vec::new(),
    )
}

fn capability_candidate(
    id: &str,
    support: Support,
    provider_kind: ProviderKind,
    provider_id: &str,
) -> CandidateCapability {
    CandidateCapability::new(
        Platform::parse("any").expect("platform"),
        ComponentId::parse("default").expect("profile"),
        CapabilityId::parse(id).expect("capability"),
        SemVer::parse("0.1.0").expect("version"),
        support,
        ProviderRef::new(
            provider_kind,
            ComponentId::parse(provider_id).expect("provider"),
            SemVer::parse("0.1.2").expect("version"),
        ),
    )
}

fn hex64(byte: u8) -> String {
    format!("{byte:064x}")
}

fn reason(failure: &LockFailure) -> String {
    failure
        .reason_code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "loader".to_owned())
}

fn expect_refused(verdict: LockVerdict) -> LockFailure {
    match verdict {
        LockVerdict::Refused(failure) => failure,
        LockVerdict::Satisfied { .. } => panic!("expected a refusal"),
    }
}

// ---------------------------------------------------------------------------
// Service-test plumbing: the selector grammar is invocation-relative, so the
// temp project is created under the package's target dir and addressed with
// a relative selector. Tests may run in parallel; every case gets its own
// numbered root.
// ---------------------------------------------------------------------------

static NEXT_ROOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn copy_dir(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read source") {
        let entry = entry.expect("entry");
        let target_path = target.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target_path);
        } else {
            std::fs::copy(entry.path(), target_path).expect("copy file");
        }
    }
}

/// Run a body against a fresh copy of the committed reference project.
/// The closure receives the relative selection and the absolute root.
fn with_project(body: impl FnOnce(&LoadSelection, &Path)) {
    let id = NEXT_ROOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        + u64::from(std::process::id());
    let relative = std::path::PathBuf::from("target")
        .join("lock-tests")
        .join(format!("case-{id}"));
    let absolute = std::env::current_dir()
        .expect("the test process has a working directory")
        .join(&relative);
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join(REFERENCE_PROJECT),
        &absolute,
    );
    let selection = LoadSelection {
        project: Some(relative.to_string_lossy().replace('\\', "/")),
    };
    body(&selection, &absolute);
    let _ = std::fs::remove_dir_all(&absolute);
}

// ---------------------------------------------------------------------------
// Wire: parse classification, canonicality, digests.
// ---------------------------------------------------------------------------

#[test]
fn golden_contract_only_lock_parses_and_matches_its_independent_digest() {
    let lock = Lockfile::parse_canonical(GOLDEN).expect("golden lock parses");
    assert_eq!(lock.digest().as_str(), GOLDEN_DIGEST);
    assert_eq!(lock.resolver_version().as_str(), RESOLVER_VERSION);
    assert_eq!(lock.core_version().as_str(), "0.2.7");
    let protocol = lock.target_protocol().expect("published protocol");
    assert_eq!(protocol.version().as_str(), "1.2.0");
    // Round-trip: canonical bytes are byte-identical to the committed file.
    assert_eq!(lock.canonical_bytes().as_ref(), GOLDEN);
}

#[test]
fn semantically_equal_noncanonical_documents_are_classified_never_rewritten() {
    let payload = std::str::from_utf8(GOLDEN)
        .expect("utf8")
        .trim_end_matches('\n');
    let cases: [(&str, String); 4] = [
        ("two final LFs", format!("{payload}\n\n")),
        ("leading whitespace", format!(" {payload}")),
        ("insignificant whitespace", {
            let value: serde_json::Value = serde_json::from_str(payload).expect("json");
            serde_json::to_string_pretty(&value).expect("pretty")
        }),
        ("CRLF final break", format!("{payload}\r\n")),
    ];
    for (name, bytes) in cases {
        let error = Lockfile::parse_canonical(bytes.as_bytes())
            .map(|_: Lockfile| ())
            .expect_err(name);
        assert_eq!(reason(&error), "lock.noncanonical", "{name}");
    }
}

#[test]
fn wire_refusals_carry_the_closed_reason_codes() {
    let payload = std::str::from_utf8(GOLDEN)
        .expect("utf8")
        .trim_end_matches('\n');
    let tampered = |old: &str, new: &str| payload.replace(old, new);
    let cases = [
        (
            "duplicate JSON key",
            payload.replace(
                "\"schema_version\":\"lekalo/lock/v1.0.0\"",
                "\"schema_version\":\"lekalo/lock/v1.0.0\",\"schema_version\":\"lekalo/lock/v1.0.0\"",
            ),
            "lock.noncanonical",
        ),
        (
            "future schema discriminator",
            tampered("lekalo/lock/v1.0.0", "lekalo/lock/v2.0.0"),
            "lock.unsupported-schema-version",
        ),
        (
            "unknown schema spelling",
            tampered("lekalo/lock/v1.0.0", "lekalo/lock/1"),
            "lock.schema-invalid",
        ),
        (
            "unknown top-level field",
            tampered("\"core\":", "\"core-x\":"),
            "lock.schema-invalid",
        ),
        (
            "uppercase digest",
            tampered("sha256:2cba65b0", "sha256:2CBA65B0"),
            "lock.schema-invalid",
        ),
        (
            "short digest",
            tampered("sha256:2cba65b0", "sha256:2cba65b"),
            "lock.schema-invalid",
        ),
        (
            "v-prefixed version",
            tampered("\"version\":\"0.2.7\"", "\"version\":\"v0.1.9\""),
            "lock.schema-invalid",
        ),
        (
            "build metadata version",
            tampered("\"version\":\"0.2.7\"", "\"version\":\"0.1.9+meta\""),
            "lock.schema-invalid",
        ),
        (
            "range version",
            tampered("\"version\":\"0.2.7\"", "\"version\":\"^0.1\""),
            "lock.schema-invalid",
        ),
        (
            "BOM prefix",
            format!("\u{feff}{payload}"),
            "lock.schema-invalid",
        ),
        (
            "trailing garbage",
            format!("{payload} garbage"),
            "lock.schema-invalid",
        ),
    ];
    for (name, document, expected) in cases {
        let error = Lockfile::parse_canonical(document.as_bytes())
            .map(|_: Lockfile| ())
            .expect_err(name);
        assert_eq!(reason(&error), expected, "{name}");
    }
    // Exit classes: unsupported schema is 5, schema-invalid is 1.
    let future =
        Lockfile::parse_canonical(tampered("lekalo/lock/v1.0.0", "lekalo/lock/v2.0.0").as_bytes())
            .map(|_: Lockfile| ())
            .expect_err("future");
    assert_eq!(future.exit_code(), 5);
    assert_eq!(future.status(), "unsupported-version");
}

#[test]
fn multi_adapter_wire_document_parses_with_references_resolved() {
    let lock = Lockfile::parse_canonical(MULTI).expect("multi-adapter lock parses");
    assert_eq!(lock.adapters().len(), 2);
    assert_eq!(lock.generators().len(), 1);
    assert_eq!(lock.profiles().len(), 1);
    assert_eq!(lock.capabilities().len(), 2);
    let protocol = lock.target_protocol().expect("published protocol");
    assert_eq!(protocol.version().as_str(), "1.1.0");
}

#[test]
fn unpublished_protocol_forbids_executable_components_on_the_wire() {
    let mut value: serde_json::Value = serde_json::from_slice(MULTI).expect("json");
    value["contracts"]["target_protocol"] = serde_json::json!(null);
    let document = serde_json::to_string(&value).expect("serialize");
    let error = Lockfile::parse_canonical(document.as_bytes())
        .map(|_: Lockfile| ())
        .expect_err("protocol-null with adapters");
    assert_eq!(reason(&error), "versioning.protocol-unpublished");
    assert_eq!(error.exit_code(), 5);
}

#[test]
fn unversioned_profile_is_refused_instead_of_fabricating_zero_zero_zero() {
    let mut value: serde_json::Value = serde_json::from_slice(MULTI).expect("json");
    value["profiles"][0]["version"] = serde_json::json!("0.0.0");
    let document = serde_json::to_string(&value).expect("serialize");
    let error = Lockfile::parse_canonical(document.as_bytes())
        .map(|_: Lockfile| ())
        .expect_err("profile 0.0.0");
    assert_eq!(reason(&error), "lock.profile-unversioned");
}

#[test]
fn unsorted_or_dangling_components_are_reference_invalid() {
    let mut value: serde_json::Value = serde_json::from_slice(MULTI).expect("json");
    // Reverse the sorted adapter order.
    let reversed = {
        let adapters = value["adapters"].as_array().expect("adapters").clone();
        serde_json::Value::Array(adapters.into_iter().rev().collect())
    };
    value["adapters"] = reversed;
    let document = serde_json::to_string(&value).expect("serialize");
    let error = Lockfile::parse_canonical(document.as_bytes())
        .map(|_: Lockfile| ())
        .expect_err("unsorted adapters");
    assert_eq!(reason(&error), "lock.reference-invalid");

    // Dangling generator adapter reference.
    let mut value: serde_json::Value = serde_json::from_slice(MULTI).expect("json");
    value["generators"][0]["adapter"]["version"] = serde_json::json!("9.9.9");
    let document = serde_json::to_string(&value).expect("serialize");
    let error = Lockfile::parse_canonical(document.as_bytes())
        .map(|_: Lockfile| ())
        .expect_err("dangling generator adapter");
    assert_eq!(reason(&error), "lock.reference-invalid");
}

#[test]
fn private_paths_urls_and_credential_shapes_never_serialize() {
    // Absolute path, URL, UNC shape, and a provider token prefix are all
    // refused with the exit-3 integrity denial.
    for candidate in [
        "/abs/path",
        "C:/abs/path",
        "\\\\server\\share",
        "https://example.com/pkg",
        "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
    ] {
        let error = SourceRef::new(
            SourceKind::Project,
            candidate,
            Sha256Digest::from_hex(&hex64(1)),
        )
        .map(|_: SourceRef| ())
        .expect_err(candidate);
        assert_eq!(reason(&error), "lock.private-data-forbidden", "{candidate}");
        assert_eq!(error.exit_code(), 3);
        assert_eq!(error.status(), "denied");
    }
}

// ---------------------------------------------------------------------------
// Resolution: hermetic, deterministic, exact-set.
// ---------------------------------------------------------------------------

#[test]
fn contract_only_resolution_is_deterministic() {
    let registry = VersionRegistry::embedded().expect("embedded registry");
    let request = request(registry);
    let resolved =
        LockResolver::resolve(&request, &CandidateSet::empty(), registry).expect("resolves");
    let resolved_again =
        LockResolver::resolve(&request, &CandidateSet::empty(), registry).expect("resolves");
    assert_eq!(
        resolved.canonical_bytes(),
        resolved_again.canonical_bytes(),
        "identical inputs produce byte-identical locks"
    );
}

/// A synthetic registry whose protocol family stays unpublished — the
/// pre-#27 world, kept alive so the publication gate stays tested.
fn unpublished_protocol_registry() -> VersionRegistry {
    const JSON: &str = r#"
{
  "registry": "dev.lekalo.version-registry",
  "registryVersion": "1.0.0",
  "families": {
    "model": {
      "current": "1.0.0",
      "aliases": [],
      "versions": [
        {"version": "1.0.0", "state": "supported", "classification": "additive",
         "reason": "Synthetic published model for issue #10 hermetic tests."}
      ],
      "migrations": []
    },
    "ir": {
      "current": "0.1.0",
      "aliases": [],
      "versions": [
        {"version": "0.1.0", "state": "supported", "classification": "additive",
         "reason": "Synthetic published IR for issue #10 hermetic tests."}
      ],
      "migrations": []
    },
    "protocol": {
      "current": null,
      "aliases": [],
      "versions": [],
      "migrations": []
    }
  }
}
"#;
    VersionRegistry::from_bytes(JSON.as_bytes()).expect("synthetic registry is valid")
}

#[test]
fn executable_candidates_with_unpublished_protocol_are_refused() {
    let registry = unpublished_protocol_registry();
    let candidates =
        CandidateSet::empty().with_adapter(adapter_candidate("node-typescript", "0.1.2", 1, "any"));
    let request =
        request(&registry).with_adapter(ComponentId::parse("node-typescript").expect("id"));
    let error = LockResolver::resolve(&request, &candidates, &registry)
        .map(|_: Lockfile| ())
        .expect_err("unpublished protocol");
    assert_eq!(reason(&error), "versioning.protocol-unpublished");
}

#[test]
fn multi_adapter_resolution_selects_highest_stable_and_is_order_independent() {
    let registry = published_protocol_registry();
    let request = request(&registry)
        .with_adapter(ComponentId::parse("node-typescript").expect("id"))
        .with_target(Platform::parse("any").expect("platform"));

    // Insertion order A: 0.1.2 then 0.1.11 then 0.2.0-rc.1 (prerelease).
    let candidates_a = CandidateSet::empty()
        .with_adapter(adapter_candidate("node-typescript", "0.2.0-rc.1", 1, "any"))
        .with_adapter(adapter_candidate("node-typescript", "0.1.11", 2, "any"))
        .with_adapter(adapter_candidate("node-typescript", "0.1.2", 3, "any"));
    // Insertion order B: exact reverse.
    let candidates_b = CandidateSet::empty()
        .with_adapter(adapter_candidate("node-typescript", "0.1.2", 3, "any"))
        .with_adapter(adapter_candidate("node-typescript", "0.1.11", 2, "any"))
        .with_adapter(adapter_candidate("node-typescript", "0.2.0-rc.1", 1, "any"));

    let resolved_a = LockResolver::resolve(&request, &candidates_a, &registry).expect("A");
    let resolved_b = LockResolver::resolve(&request, &candidates_b, &registry).expect("B");
    assert_eq!(resolved_a.canonical_bytes(), resolved_b.canonical_bytes());
    assert_eq!(resolved_a.adapters().len(), 1);
    assert_eq!(resolved_a.adapters()[0].version().as_str(), "0.1.11");
}

#[test]
fn prerelease_requires_the_exact_opt_in() {
    let registry = published_protocol_registry();
    let request = request(&registry)
        .with_adapter(ComponentId::parse("node-typescript").expect("id"))
        .with_target(Platform::parse("any").expect("platform"));
    let candidates = CandidateSet::empty().with_adapter(adapter_candidate(
        "node-typescript",
        "0.2.0-rc.1",
        1,
        "any",
    ));
    // Without the opt-in only a prerelease exists: nothing selectable.
    let error = LockResolver::resolve(&request, &candidates, &registry)
        .map(|_: Lockfile| ())
        .expect_err("no stable candidate");
    assert_eq!(reason(&error), "lock.component-unavailable");
    // With the exact opt-in the prerelease is selected.
    let request = request
        .with_exact_version("0.2.0-rc.1")
        .expect("exact spelling");
    let resolved = LockResolver::resolve(&request, &candidates, &registry).expect("opted in");
    assert_eq!(resolved.adapters()[0].version().as_str(), "0.2.0-rc.1");
}

#[test]
fn duplicate_identities_and_missing_platforms_and_incompatible_manifests_classify() {
    let registry = published_protocol_registry();
    let request = request(&registry)
        .with_adapter(ComponentId::parse("node-typescript").expect("id"))
        .with_target(Platform::parse("x86_64-pc-windows-msvc").expect("platform"));

    // Same id+version offered twice with different digests: ambiguous.
    let duplicated = CandidateSet::empty()
        .with_adapter(adapter_candidate("node-typescript", "0.1.2", 1, "any"))
        .with_adapter(adapter_candidate("node-typescript", "0.1.2", 9, "any"));
    let error = LockResolver::resolve(&request, &duplicated, &registry)
        .map(|_: Lockfile| ())
        .expect_err("duplicate identity");
    assert_eq!(reason(&error), "lock.resolution-ambiguous");

    // Only a linux artifact exists: platform unavailable (exit 4).
    let linux_only = CandidateSet::empty().with_adapter(adapter_candidate(
        "node-typescript",
        "0.1.2",
        1,
        "x86_64-unknown-linux-gnu",
    ));
    let error = LockResolver::resolve(&request, &linux_only, &registry)
        .map(|_: Lockfile| ())
        .expect_err("platform unavailable");
    assert_eq!(reason(&error), "lock.platform-unavailable");
    assert_eq!(error.exit_code(), 4);

    // A manifest requiring an IR extension can never be satisfied on #8.
    let incompatible = CandidateSet::empty().with_adapter(CandidateAdapter::new(
        ComponentId::parse("node-typescript").expect("id"),
        SemVer::parse("0.1.2").expect("version"),
        Sha256Digest::from_hex(&hex64(1)),
        SourceRef::new(
            SourceKind::Catalog,
            "node-typescript",
            Sha256Digest::from_hex(&hex64(2)),
        )
        .expect("source"),
        vec![ArtifactPin::new(
            Platform::parse("any").expect("platform"),
            Sha256Digest::from_hex(&hex64(3)),
        )],
        AdapterCompatibilityManifest::new(
            ContractVersion::<RegistryContract>::parse_canonical(
                lekalo_core::versioning::compatibility::MANIFEST_SCHEMA_VERSION,
            )
            .expect("manifest schema version is canonical"),
            "node-typescript",
            ir_version(),
            ir_version(),
            Some(ProtocolBounds {
                min: protocol_version(),
                max: protocol_version(),
            }),
            vec!["ir.ext.never".to_owned()],
            Vec::new(),
        )
        .expect("manifest"),
    ));
    let error = LockResolver::resolve(&request, &incompatible, &registry)
        .map(|_: Lockfile| ())
        .expect_err("extension incompatible");
    assert_eq!(reason(&error), "versioning.extension-incompatible");
    assert_eq!(error.exit_code(), 5);
}

#[test]
fn generators_profiles_and_required_capabilities_resolve_together() {
    let registry = published_protocol_registry();
    let request = request(&registry)
        .with_adapter(ComponentId::parse("node-typescript").expect("id"))
        .with_generator(ComponentId::parse("planner-gen").expect("id"))
        .with_profile(ComponentId::parse("default").expect("id"))
        .require_capability(CapabilityId::parse("planner.render").expect("capability"))
        .with_target(Platform::parse("any").expect("platform"));
    let candidates = CandidateSet::empty()
        .with_adapter(adapter_candidate("node-typescript", "0.1.2", 1, "any"))
        .with_generator(generator_candidate(
            "planner-gen",
            "0.3.0",
            "node-typescript",
        ))
        .with_profile(profile_candidate("default", "1.1.0", "node-typescript"))
        .with_capability(capability_candidate(
            "planner.render",
            Support::Full,
            ProviderKind::Adapter,
            "node-typescript",
        ));
    let resolved = LockResolver::resolve(&request, &candidates, &registry).expect("resolves");
    assert_eq!(resolved.adapters().len(), 1);
    assert_eq!(resolved.generators().len(), 1);
    assert_eq!(resolved.profiles().len(), 1);
    assert_eq!(resolved.capabilities().len(), 1);

    // Unknown support never satisfies a required capability.
    let unsupported = CandidateSet::empty()
        .with_adapter(adapter_candidate("node-typescript", "0.1.2", 1, "any"))
        .with_capability(capability_candidate(
            "planner.render",
            Support::Unknown,
            ProviderKind::Adapter,
            "node-typescript",
        ));
    let error = LockResolver::resolve(&request, &unsupported, &registry)
        .map(|_: Lockfile| ())
        .expect_err("unknown support");
    assert_eq!(reason(&error), "lock.component-unavailable");

    // Partial support satisfies only with the explicit accepted policy.
    let partial = CandidateSet::empty()
        .with_adapter(adapter_candidate("node-typescript", "0.1.2", 1, "any"))
        .with_generator(generator_candidate(
            "planner-gen",
            "0.3.0",
            "node-typescript",
        ))
        .with_profile(profile_candidate("default", "1.1.0", "node-typescript"))
        .with_capability(capability_candidate(
            "planner.render",
            Support::Partial,
            ProviderKind::Adapter,
            "node-typescript",
        ));
    let error = LockResolver::resolve(&request, &partial, &registry)
        .map(|_: Lockfile| ())
        .expect_err("partial without policy");
    assert_eq!(reason(&error), "lock.component-unavailable");
    let request_with_policy = request.with_partial_policy();
    let resolved =
        LockResolver::resolve(&request_with_policy, &partial, &registry).expect("with policy");
    assert_eq!(resolved.capabilities().len(), 1);
}

// ---------------------------------------------------------------------------
// Verification: staleness, tamper, requirement strength.
// ---------------------------------------------------------------------------

#[test]
fn changed_request_makes_a_lock_stale_while_newer_candidates_never_do() {
    let registry = published_protocol_registry();
    let pinned = request(&registry)
        .with_adapter(ComponentId::parse("node-typescript").expect("id"))
        .with_target(Platform::parse("any").expect("platform"));
    let candidates =
        CandidateSet::empty().with_adapter(adapter_candidate("node-typescript", "0.1.2", 1, "any"));
    let lock = LockResolver::resolve(&pinned, &candidates, &registry).expect("lock");
    let verdict = LockVerifier::verify(
        &lock,
        &pinned,
        &RuntimeInventory::empty(),
        &registry,
        LockRequirement::Optional,
    );
    assert!(matches!(verdict, LockVerdict::Satisfied { .. }));

    // A drifted request has a different digest: the same lock is stale even
    // though newer candidates appeared in the supply.
    let drifted_request = request(&registry)
        .with_adapter(ComponentId::parse("node-typescript").expect("id"))
        .with_profile(ComponentId::parse("default").expect("profile"))
        .with_target(Platform::parse("any").expect("platform"));
    let failure = expect_refused(LockVerifier::verify(
        &lock,
        &drifted_request,
        &RuntimeInventory::empty(),
        &registry,
        LockRequirement::Optional,
    ));
    assert_eq!(reason(&failure), "lock.stale");
    assert_eq!(failure.exit_code(), 1);
}

#[test]
fn same_version_changed_bytes_are_integrity_denials_not_validation_failures() {
    let registry = published_protocol_registry();
    let request = request(&registry)
        .with_adapter(ComponentId::parse("node-typescript").expect("id"))
        .with_target(Platform::parse("any").expect("platform"));
    let candidates =
        CandidateSet::empty().with_adapter(adapter_candidate("node-typescript", "0.1.2", 1, "any"));
    let lock = LockResolver::resolve(&request, &candidates, &registry).expect("lock");
    let adapter = &lock.adapters()[0];

    // Same identity, same version, different package bytes: tamper.
    let tampered = RuntimeInventory::empty()
        .with_adapter(InventoryComponent::new(
            ComponentId::parse("node-typescript").expect("id"),
            SemVer::parse("0.1.2").expect("version"),
            Sha256Digest::from_hex(&hex64(0xEE)),
        ))
        .with_artifact(InventoryArtifact::new(
            ComponentId::parse("node-typescript").expect("id"),
            Platform::parse("any").expect("platform"),
            Sha256Digest::from_hex(&hex64(0xEF)),
        ));
    let failure = expect_refused(LockVerifier::verify(
        &lock,
        &request,
        &tampered,
        &registry,
        LockRequirement::Required,
    ));
    assert_eq!(reason(&failure), "lock.digest-mismatch");
    assert_eq!(failure.exit_code(), 3);
    assert_eq!(failure.status(), "denied");

    // Missing local component under Required: unavailable (4), not tamper.
    let failure = expect_refused(LockVerifier::verify(
        &lock,
        &request,
        &RuntimeInventory::empty(),
        &registry,
        LockRequirement::Required,
    ));
    assert_eq!(reason(&failure), "lock.component-unavailable");
    assert_eq!(failure.exit_code(), 4);

    // The matching inventory satisfies Required.
    let honest = RuntimeInventory::empty()
        .with_adapter(InventoryComponent::new(
            ComponentId::parse("node-typescript").expect("id"),
            SemVer::parse("0.1.2").expect("version"),
            adapter.digest().clone(),
        ))
        .with_artifact(InventoryArtifact::new(
            ComponentId::parse("node-typescript").expect("id"),
            Platform::parse("any").expect("platform"),
            adapter.artifacts()[0].digest().clone(),
        ));
    let verdict = LockVerifier::verify(
        &lock,
        &request,
        &honest,
        &registry,
        LockRequirement::Required,
    );
    assert!(matches!(verdict, LockVerdict::Satisfied { .. }));
}

// ---------------------------------------------------------------------------
// Service: zero-write preview, deterministic plan identity, CAS apply.
// ---------------------------------------------------------------------------

#[test]
fn preview_creates_nothing_and_plan_identity_is_byte_stable() {
    with_project(|selection, root| {
        let prepared = LockService::plan(selection, CandidateSet::empty()).expect("plan");
        let preview = LockService::preview(&prepared);
        assert!(preview.changed);
        assert!(preview.before_digest.is_none());
        assert!(preview.changes.iter().all(|entry| entry.from.is_none()));

        // Dry-run creates no .lekalo directory, no stage file, no lock.
        assert!(
            !root.join(".lekalo").exists(),
            "dry-run must not create .lekalo"
        );
        assert!(!root.join("lekalo.lock").exists());
        assert!(!root.join("lekalo.lock.lekalo-new").exists());

        // Re-planning the unchanged project reproduces the exact plan id.
        let prepared_again = LockService::plan(selection, CandidateSet::empty()).expect("plan");
        assert_eq!(prepared.plan_id(), prepared_again.plan_id());
    });
}

#[test]
fn apply_writes_the_lock_and_a_second_identical_apply_is_a_no_op() {
    with_project(|selection, root| {
        let prepared = LockService::plan(selection, CandidateSet::empty()).expect("plan");
        let plan_id = prepared.plan_id().to_owned();
        let applied = LockService::apply(prepared, &plan_id).expect("apply");
        assert!(applied.changed);
        let first = std::fs::read(root.join("lekalo.lock")).expect("lock bytes");

        let prepared = LockService::plan(selection, CandidateSet::empty()).expect("plan");
        let plan_id = prepared.plan_id().to_owned();
        let applied = LockService::apply(prepared, &plan_id).expect("apply");
        assert!(!applied.changed, "an already-current update writes nothing");
        assert_eq!(
            std::fs::read(root.join("lekalo.lock")).expect("bytes"),
            first
        );
        // The only file the service ever wrote is the lock; no stage survives.
        assert!(!root.join("lekalo.lock.lekalo-new").exists());
    });
}

#[test]
fn a_wrong_plan_identity_and_drifted_before_state_write_nothing() {
    with_project(|selection, root| {
        let prepared = LockService::plan(selection, CandidateSet::empty()).expect("plan");
        let wrong = format!("sha256:{}", "0".repeat(64));
        let error = LockService::apply(prepared, &wrong).expect_err("wrong identity");
        assert_eq!(reason(&error), "lock.source-changed");
        assert!(!root.join("lekalo.lock").exists());

        // Drift: create the lock out-of-band, then apply the stale plan.
        let prepared = LockService::plan(selection, CandidateSet::empty()).expect("plan");
        std::fs::write(root.join("lekalo.lock"), b"{}\n").expect("out-of-band lock");
        let plan_id = prepared.plan_id().to_owned();
        let error = LockService::apply(prepared, &plan_id).expect_err("drifted");
        assert_eq!(reason(&error), "lock.source-changed");
    });
}

#[test]
fn lock_create_then_check_round_trip_and_check_refuses_missing() {
    with_project(|selection, root| {
        let receipt = LockService::lock(
            selection,
            CandidateSet::empty(),
            LockRequirement::Optional,
            true,
        )
        .expect("create");
        assert_eq!(receipt.mode, "create");
        assert!(receipt.changed);

        let receipt = LockService::lock(
            selection,
            CandidateSet::empty(),
            LockRequirement::Optional,
            false,
        )
        .expect("check");
        assert_eq!(receipt.mode, "check");
        assert!(!receipt.changed);
        assert!(root.join("lekalo.lock").exists());
    });

    // Outside any project root, a plain check is refused (exit 1).
    let error = LockService::lock(
        &LoadSelection {
            project: Some(".".to_owned()),
        },
        CandidateSet::empty(),
        LockRequirement::Optional,
        false,
    )
    .expect_err("check refuses missing");
    assert_eq!(error.exit_code(), 1);
}

#[cfg(unix)]
#[test]
fn a_symlinked_lock_is_a_policy_denial() {
    with_project(|selection, root| {
        std::fs::write(root.join("elsewhere.lock"), b"{}\n").expect("target");
        std::os::unix::fs::symlink("elsewhere.lock", root.join("lekalo.lock")).expect("symlink");
        let error = LockService::lock(
            selection,
            CandidateSet::empty(),
            LockRequirement::Optional,
            false,
        )
        .expect_err("symlink");
        assert_eq!(error.exit_code(), 3);
        assert_eq!(error.status(), "denied");
    });
}
