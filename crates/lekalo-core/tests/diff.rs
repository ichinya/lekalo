//! Issue #18 core tests: the golden fixture matrix over
//! `tests/fixtures/diff/cases`, the live acceptance criteria
//! (formatting-only equality, breaking removals and nullability
//! tightening, additive optional fields, separate effect records,
//! explainable reasons, namespaced adapter contributions), determinism,
//! mixed-version rejection, history resolution, and profile decisions.
//!
//! Golden bytes are regenerated with `LEKALO_DIFF_REGENERATE=1 cargo test
//! -p lekalo-core --test diff` and are otherwise compared byte-identical;
//! the Node contract gate (`scripts/test-semantic-diff-contracts.mjs`)
//! validates every golden against the closed wire schema independently.
//!
//! Project selections are relative to the workspace root and every chdir
//! runs under the process-wide lock (the Windows selection policy denies
//! absolute and alias-spelled paths before any command logic).

use lekalo_core::diff::{
    compare, AdapterInput, AdapterTrust, CompatibilityClass, ContributionEffect, DiffRequest,
    ProfileId, ProfileVerdict,
};
use lekalo_core::ir::CompiledProject;
use lekalo_core::loader::LoadSelection;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

const CASES: &[&str] = &[
    "behavior",
    "equal-formatting",
    "fields",
    "fields-tighten",
    "history",
    "signature",
    "symbols",
    "taxonomy-rest",
];

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

/// Load and compile one fixture side with the workspace root as cwd.
fn load(relative: &str) -> CompiledProject {
    let selection = LoadSelection {
        project: Some(relative.to_owned()),
    };
    let model = lekalo_core::loader::normalize_model(&selection)
        .unwrap_or_else(|result| panic!("{relative} must load: {:?}", result.to_json_string()));
    lekalo_core::ir::compile(&model)
        .unwrap_or_else(|failure| {
            panic!(
                "{relative} must compile: {:?}",
                failure.into_result().to_json_string()
            )
        })
        .project
}

/// Both sides of one fixture case, loaded under the cwd lock.
fn case(name: &str) -> (CompiledProject, CompiledProject) {
    let _guard = CWD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let pair = (
        load(&format!("tests/fixtures/diff/cases/{name}/base")),
        load(&format!("tests/fixtures/diff/cases/{name}/candidate")),
    );
    std::env::set_current_dir(original).expect("restore cwd");
    pair
}

/// Run `step` with the workspace root as cwd, under the lock.
fn with_workspace_root<T>(step: impl FnOnce() -> T) -> T {
    let _guard = CWD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let value = step();
    std::env::set_current_dir(original).expect("restore cwd");
    value
}

fn canonical(result: &lekalo_core::diff::DiffResult) -> String {
    result
        .to_canonical_json()
        .expect("canonical bytes within bound")
}

fn reason_ids(set: &lekalo_core::diagnostics::DiagnosticSet) -> Vec<String> {
    set.as_slice()
        .iter()
        .map(|diagnostic| diagnostic.id().to_owned())
        .collect()
}

#[test]
fn golden_cases_match_their_canonical_bytes() {
    let regenerate = std::env::var("LEKALO_DIFF_REGENERATE").is_ok();
    let results = with_workspace_root(|| {
        CASES
            .iter()
            .map(|case| {
                let load = |side: &str| load(&format!("tests/fixtures/diff/cases/{case}/{side}"));
                (load("base"), load("candidate"))
            })
            .collect::<Vec<_>>()
    });
    for (case, (base, candidate)) in CASES.iter().zip(results) {
        let facts = compare(&base, &candidate, &DiffRequest::new()).expect("facts compare");
        let profiled = compare(&base, &candidate, &DiffRequest::new().with_all_profiles())
            .expect("profiled compare");
        let facts_bytes = canonical(&facts);
        let profile_bytes = canonical(&profiled);

        let facts_path = format!("tests/fixtures/diff/cases/{case}/expect.json");
        let profiles_path = format!("tests/fixtures/diff/cases/{case}/expect.profiles.json");
        let expected_facts = with_workspace_root(|| {
            if regenerate {
                fs::write(&facts_path, &facts_bytes).expect("write facts golden");
            }
            fs::read_to_string(&facts_path)
                .unwrap_or_else(|error| panic!("read {facts_path}: {error}"))
        });
        let expected_profiles = with_workspace_root(|| {
            if regenerate {
                fs::write(&profiles_path, &profile_bytes).expect("write profile golden");
            }
            fs::read_to_string(&profiles_path)
                .unwrap_or_else(|error| panic!("read {profiles_path}: {error}"))
        });
        assert_eq!(
            facts_bytes, expected_facts,
            "case {case}: facts golden drifted"
        );
        assert_eq!(
            profile_bytes, expected_profiles,
            "case {case}: profile golden drifted"
        );
    }
}

#[test]
fn formatting_only_permutations_are_semantically_equal() {
    let (base, candidate) = case("equal-formatting");
    let result = compare(&base, &candidate, &DiffRequest::new()).expect("compare");
    assert!(result.equal(), "formatting-only diff must be equal");
    assert!(result.changes().is_empty());
    assert!(result.seeds().is_empty());
    assert!(result.classification().is_empty());
    // Repeating the comparison is byte-identical.
    let again = compare(&base, &candidate, &DiffRequest::new()).expect("compare again");
    assert_eq!(canonical(&result), canonical(&again));
    let reversed = compare(&candidate, &base, &DiffRequest::new()).expect("reverse compare");
    assert!(reversed.equal());
}

#[test]
fn field_removal_and_nullable_tightening_break_while_optional_addition_does_not() {
    let (base, candidate) = case("fields");
    let result = compare(&base, &candidate, &DiffRequest::new()).expect("compare");
    assert!(!result.equal());
    let kinds: BTreeSet<String> = result
        .changes()
        .iter()
        .map(|record| record.kind().key().to_owned())
        .collect();
    assert!(kinds.contains("field.removed"));
    assert!(kinds.contains("invariant.identity-changed"));
    assert!(result
        .classification()
        .contains(&CompatibilityClass::SourceBreaking));
    assert!(result
        .classification()
        .contains(&CompatibilityClass::DataLossRisk));

    let added_optional = result
        .changes()
        .iter()
        .find(|record| {
            record.kind().key() == "field.added" && record.subject().member() == Some("field.tag")
        })
        .expect("optional addition record");
    assert!(added_optional.reasons().contains(&"field.added".to_owned()));
    assert!(result
        .classification()
        .contains(&CompatibilityClass::Additive));

    let (base, candidate) = case("fields-tighten");
    let result =
        compare(&base, &candidate, &DiffRequest::new().with_all_profiles()).expect("compare");
    let wire = result
        .profiles()
        .iter()
        .find(|profile| profile.profile_id() == ProfileId::WireConsumer)
        .expect("wire decision");
    assert_eq!(wire.verdict(), ProfileVerdict::Breaking);
    assert!(wire.classes().contains(&CompatibilityClass::WireBreaking));
}

#[test]
fn effect_changes_are_separate_records_from_signature_changes() {
    let (base, candidate) = case("signature");
    let result = compare(&base, &candidate, &DiffRequest::new()).expect("compare");
    let effect = result
        .changes()
        .iter()
        .find(|record| record.kind().key() == "effect.operation-changed")
        .expect("separate effect record");
    assert_eq!(effect.subject().id(), "shop.persist_item");
    assert!(effect
        .reasons()
        .contains(&"effect.operation-changed".to_owned()));
    let signature = result
        .changes()
        .iter()
        .filter(|record| record.kind().key().starts_with("signature."))
        .count();
    assert!(signature >= 4, "signature records stay separate");
}

#[test]
fn history_resolves_renames_replacements_tombstones_and_conflicts() {
    let (base, candidate) = case("history");
    let result = compare(&base, &candidate, &DiffRequest::new()).expect("compare");

    let renamed = result
        .changes()
        .iter()
        .find(|record| record.subject().id() == "shop.label")
        .expect("direct rename");
    assert_eq!(renamed.kind().key(), "symbol.renamed");
    assert!(renamed
        .reasons()
        .contains(&"symbol.rename-history".to_owned()));

    let multihop = result
        .changes()
        .iter()
        .find(|record| record.subject().id() == "shop.sku3")
        .expect("multi-hop rename");
    assert_eq!(multihop.kind().key(), "symbol.renamed");
    assert!(multihop
        .reasons()
        .contains(&"symbol.rename-history-multihop".to_owned()));

    let replaced = result
        .changes()
        .iter()
        .find(|record| record.kind().key() == "symbol.replaced")
        .expect("replacement tombstone");
    assert_eq!(replaced.subject().id(), "shop.item_state");

    let tombstoned = result
        .changes()
        .iter()
        .find(|record| record.kind().key() == "symbol.tombstoned")
        .expect("deleted tombstone");
    assert_eq!(tombstoned.subject().id(), "shop.create_flow");

    let reuse = result
        .changes()
        .iter()
        .find(|record| record.subject().id() == "shop.legacy")
        .expect("tombstone reuse");
    assert!(reuse
        .reasons()
        .contains(&"symbol.tombstone-reuse".to_owned()));
    assert!(result
        .classification()
        .contains(&CompatibilityClass::Unknown));

    let removed = result
        .changes()
        .iter()
        .find(|record| record.subject().id() == "shop.old_note")
        .expect("conflicting claim removal");
    assert_eq!(removed.kind().key(), "symbol.removed");
    assert!(removed
        .reasons()
        .contains(&"history.conflicting".to_owned()));
    let added = result
        .changes()
        .iter()
        .find(|record| record.subject().id() == "shop.new_note")
        .expect("conflicting claim addition");
    assert!(added.reasons().contains(&"history.conflicting".to_owned()));
}

#[test]
fn profiles_decide_independently_and_block_on_absent_evidence() {
    let (base, candidate) = case("fields");
    let result =
        compare(&base, &candidate, &DiffRequest::new().with_all_profiles()).expect("compare");
    assert_eq!(result.profiles().len(), 5);

    let storage = result
        .profiles()
        .iter()
        .find(|profile| profile.profile_id() == ProfileId::StorageConsumer)
        .expect("storage decision");
    assert_eq!(storage.verdict(), ProfileVerdict::Blocked);
    assert_eq!(
        storage.blocked_on().expect("blocked").reason(),
        "profile.storage-evidence-absent"
    );

    let target = result
        .profiles()
        .iter()
        .find(|profile| profile.profile_id() == ProfileId::TargetConsumer)
        .expect("target decision");
    assert_eq!(target.verdict(), ProfileVerdict::Blocked);

    let advisory = result
        .profiles()
        .iter()
        .find(|profile| profile.profile_id() == ProfileId::Advisory)
        .expect("advisory decision");
    assert_ne!(advisory.verdict(), ProfileVerdict::Blocked);

    // A verified adapter contribution merges into its profile decision
    // without touching core facts or equality.
    let source_change = result
        .changes()
        .iter()
        .find(|record| record.kind().key() == "field.removed")
        .expect("removal record")
        .change_id()
        .to_owned();
    let adapter = AdapterInput::new(
        "com.example.storage",
        "storage-probe",
        "1.2.3",
        "warehouse-postgres",
        ProfileId::SourceConsumer,
        "adapter-protocol/0.9.0",
        result.candidate_ref().ir_digest(),
        "revision-7",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        AdapterTrust::Verified,
        vec![ContributionEffect::new(
            source_change,
            vec![CompatibilityClass::StorageMigrationRequired],
        )
        .expect("effect")],
        Vec::new(),
    )
    .expect("adapter input");
    let request = DiffRequest::new()
        .with_profile(ProfileId::SourceConsumer)
        .with_adapter(adapter);
    let adapted = compare(&base, &candidate, &request).expect("adapted compare");
    let source = adapted
        .profiles()
        .iter()
        .find(|profile| profile.profile_id() == ProfileId::SourceConsumer)
        .expect("source decision");
    assert!(source
        .classes()
        .contains(&CompatibilityClass::StorageMigrationRequired));
    assert_eq!(adapted.changes().len(), result.changes().len());
    assert_eq!(adapted.equal(), result.equal());
    assert_eq!(adapted.adapters().len(), 1);
}

#[test]
fn stale_adapter_evidence_is_recorded_and_never_merges() {
    let (base, candidate) = case("fields");
    let facts = compare(&base, &candidate, &DiffRequest::new()).expect("facts");
    let adapter = AdapterInput::new(
        "com.example.target",
        "capability-probe",
        "0.9.0",
        "wasm-runtime",
        ProfileId::TargetConsumer,
        "adapter-protocol/0.9.0",
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        "revision-2",
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        AdapterTrust::Verified,
        Vec::new(),
        Vec::new(),
    )
    .expect("adapter input");
    let request = DiffRequest::new()
        .with_profile(ProfileId::TargetConsumer)
        .with_adapter(adapter);
    let result = compare(&base, &candidate, &request).expect("compare");
    let recorded = result
        .adapters()
        .iter()
        .find(|entry| entry.adapter_id() == "capability-probe")
        .expect("recorded contribution");
    assert_eq!(recorded.trust().key(), "stale", "digest mismatch is stale");
    assert!(recorded.states().contains(&"adapter.stale".to_owned()));
    assert_eq!(result.changes().len(), facts.changes().len());
}

#[test]
fn mixed_model_versions_reject_without_a_partial_result() {
    let (base, _) = case("fields");
    let error = with_workspace_root(|| {
        let selection = LoadSelection {
            project: Some("tests/fixtures/versioning/migration/golden-0.1.0".to_owned()),
        };
        let old_model = lekalo_core::loader::normalize_model(&selection).expect("0.1.0 loads");
        let old = lekalo_core::ir::compile(&old_model)
            .expect("0.1.0 compiles")
            .project;
        compare(&base, &old, &DiffRequest::new()).err()
    })
    .expect("mixed versions reject");
    assert_eq!(reason_ids(&error), vec!["diff.input-invalid".to_owned()]);
}

#[test]
fn profile_terms_reject_unknown_and_empty_terms() {
    let error = lekalo_core::diff::parse_profile_terms("source-consumer,warp-drive")
        .expect_err("unknown term rejects");
    assert_eq!(reason_ids(&error), vec!["diff.profile-invalid".to_owned()]);
    let error = lekalo_core::diff::parse_profile_terms(",").expect_err("empty terms reject");
    assert_eq!(reason_ids(&error), vec!["diff.profile-invalid".to_owned()]);
    let parsed =
        lekalo_core::diff::parse_profile_terms("advisory, source-consumer").expect("valid terms");
    assert_eq!(parsed, vec![ProfileId::Advisory, ProfileId::SourceConsumer]);
}
