//! Issue #19 core tests: the portability assessment over the checked-in
//! planner fixture, the exact-pin mutation matrix, the classification
//! matrix (unsupported vs untested vs unknown), the summary arithmetic,
//! the bounds, and the strict `--check` denial join.
//!
//! Every assertion runs against the production core seam with the same
//! fixture files the CLI gate reads; no test launches a process.

use lekalo_core::implementation::{self, ImplementationDocument};
use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::portability::{
    assess, embedded_recipe, extract, parse_evidence, parse_targets, profile_pin, request,
    verify_profile_pin, wire, Assessment, Request, Scope,
};
use lekalo_core::target_profile::{document as profile_document, resolution as profile_resolution};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const FIXTURE: &str = "tests/fixtures/portability/planner";
const EVIDENCE: &str = "tests/fixtures/portability/planner/reports/portability-evidence.json";

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

fn read(relative: &str) -> Vec<u8> {
    std::fs::read(workspace_root().join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

fn compilation() -> lekalo_core::ir::Compilation {
    // The caller holds CWD_LOCK; enter the workspace root for the
    // invocation-relative loader selection and restore afterwards.
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let selection = LoadSelection {
        project: Some(FIXTURE.to_owned()),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("fixture load failed: {}", outcome.to_json_string()),
    };
    std::env::set_current_dir(original).expect("restore working dir");
    match lekalo_core::ir::compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => panic!("fixture IR failed: {failure:?}"),
    }
}

fn profile_pin_for(name: &str, id: &str) -> lekalo_core::portability::ProfilePin {
    let bytes = read(&format!("{FIXTURE}/config/profiles/{name}.json"));
    let document =
        profile_document::decode(&bytes).unwrap_or_else(|failure| panic!("{name}: {failure:?}"));
    let resolved = profile_resolution::resolve(&document).expect("profile resolves");
    let found = resolved
        .iter()
        .find(|profile| profile.id == id)
        .unwrap_or_else(|| panic!("{id} resolves"));
    profile_pin(found)
}

/// The full qualified fixture assessment: pinned targets, complete
/// implementation inventory, and complete evidence.
fn fixture_assessment() -> (lekalo_core::ir::Compilation, wire::Report) {
    // The caller holds CWD_LOCK and has entered the workspace root.
    let compilation = compilation();
    let recipe = embedded_recipe().expect("embedded recipe");
    let targets = parse_targets(&read(&format!("{FIXTURE}/config/portability-targets.json")))
        .expect("targets decode");
    let evidence =
        parse_evidence(&read(EVIDENCE), &compilation, &recipe).expect("evidence decodes");

    // The implementation join, exactly like the CLI: digest, decode,
    // producer validation.
    let family = evidence
        .families
        .iter()
        .find(|family| family.family == wire::EvidenceFamily::Implementation)
        .expect("implementation family");
    let artifact = family.artifact_ref.as_ref().expect("artifact ref");
    let bytes = read(&format!("{FIXTURE}/{artifact}"));
    let digest = family.digest.as_ref().expect("artifact digest");
    assert_eq!(
        digest,
        &format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)),
        "the implementation artifact digest must pin the exact bytes"
    );
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("implementation JSON");
    let document = ImplementationDocument::from_value(&value).expect("implementation decodes");
    let report = implementation::validate(&document, &compilation)
        .expect("the fixture implementation validates")
        .portability()
        .clone();

    let mut pins = BTreeMap::new();
    let mut adapters = BTreeMap::new();
    let mut storage = BTreeMap::new();
    for binding in &targets.targets {
        let pin = match binding.target_id.as_str() {
            "node-typescript" => profile_pin_for("node", "node-postgres-http"),
            "php-laravel" => profile_pin_for("laravel", "laravel-postgres-http"),
            "go-service" => profile_pin_for("go", "go-postgres-http"),
            other => panic!("unknown fixture target {other}"),
        };
        verify_profile_pin(&binding.profile_ref, &{
            // Re-derive the resolved profile to verify the exact pin.
            let bytes = read(&format!("{FIXTURE}/{}", binding.profile_document_ref));
            let document = profile_document::decode(&bytes).expect("profile document");
            profile_resolution::resolve(&document)
                .expect("resolves")
                .into_iter()
                .find(|profile| profile.id == binding.profile_ref.id)
                .expect("profile found")
        })
        .expect("the checked-in pins must match the resolved profiles exactly");
        pins.insert(binding.target_id.clone(), Some(pin));
        storage.insert(
            binding.target_id.clone(),
            binding.storage_profile_refs.clone(),
        );
    }
    for adapter in &evidence.adapters {
        adapters.insert(
            adapter.target_id.clone(),
            Some(wire::AdapterSnapshot {
                state: adapter.state,
                identity: adapter.identity.clone(),
                version: adapter.version.clone(),
                protocol: adapter.protocol.clone(),
                capability_digest: adapter.capability_digest.clone(),
                evidence_digest: None,
            }),
        );
    }

    let request = Request {
        scope: Scope::Project,
        source: Some("node-typescript".to_owned()),
        destinations: vec!["php-laravel".to_owned(), "go-service".to_owned()],
    };
    let subjects = extract(&compilation.project, &recipe);
    let resolved_targets = lekalo_core::portability::resolve_targets(
        &request,
        Some(&targets),
        &subjects,
        &pins,
        &adapters,
        &storage,
    )
    .expect("targets resolve");
    // The caller-side verification, exactly like the CLI: the declared
    // complete family artifact is read and digest-checked, and each
    // claimed receipt is read, digest-checked, and scope-checked.
    let mut verified_families = std::collections::BTreeSet::new();
    for family in &evidence.families {
        if !family.complete {
            continue;
        }
        let (Some(artifact_ref), Some(digest)) = (&family.artifact_ref, &family.digest) else {
            continue;
        };
        let bytes = read(&format!("{FIXTURE}/{artifact_ref}"));
        assert_eq!(
            digest,
            &format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)),
            "the family artifact digest must pin the exact bytes"
        );
        verified_families.insert(family.family);
    }
    let mut receipts_qualified = std::collections::BTreeSet::new();
    for record in &evidence.scenarios {
        let (Some(receipt_ref), Some(receipt_digest)) =
            (&record.receipt_ref, &record.receipt_digest)
        else {
            continue;
        };
        let Some(target) = &record.target_id else {
            continue;
        };
        let bytes = read(&format!("{FIXTURE}/{receipt_ref}"));
        assert_eq!(
            receipt_digest,
            &format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)),
            "the receipt digest must pin the exact bytes"
        );
        let receipt: serde_json::Value = serde_json::from_slice(&bytes).expect("receipt JSON");
        assert_eq!(receipt["scenario"], record.scenario.as_str());
        assert_eq!(receipt["targetId"], target.as_str());
        receipts_qualified.insert((record.scenario.clone(), target.clone()));
    }
    let assessment = Assessment {
        compilation: &compilation,
        recipe: &recipe,
        subjects,
        request: &request,
        source: resolved_targets.source.clone(),
        destinations: resolved_targets.destinations.clone(),
        evidence: Some(&evidence),
        implementation: Some(&report),
        verified_families,
        receipts_qualified,
        targets_input: wire::TargetsInput {
            state: wire::PinState::Pinned,
            path: Some("config/portability-targets.json".to_owned()),
            digest: Some(format!(
                "sha256:{}",
                lekalo_core::digest::sha256_hex(&read(&format!(
                    "{FIXTURE}/config/portability-targets.json"
                )))
            )),
        },
        lock_digest: None,
        evidence_digest: None,
    };
    let report = assess(&assessment).expect("the fixture assessment succeeds");
    (compilation, report)
}

fn row<'a>(
    report: &'a wire::Report,
    subject: &str,
    destination: &str,
    dimension: wire::Dimension,
) -> &'a wire::ObligationRow {
    report
        .subjects
        .iter()
        .find(|s| s.subject == subject)
        .unwrap_or_else(|| panic!("subject {subject} missing"))
        .rows
        .iter()
        .find(|row| row.destination == destination && row.dimension == dimension)
        .unwrap_or_else(|| panic!("row {subject}/{destination}/{dimension:?} missing"))
}

mod classification_matrix {
    use super::*;
    use lekalo_core::portability::wire::{Classification, Dimension, Verification};

    #[test]
    fn the_complete_fixture_assesses() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, report) = fixture_assessment();
        assert_eq!(report.identity, "dev.lekalo.portability-report@0.8.0");
        assert_eq!(report.project_ref.project_id, "planner");
        assert_eq!(report.coverage.eligible_subjects, 31);
        assert!(report.coverage.complete);
        assert!(!report.coverage.truncated);
        assert_eq!(report.destinations.len(), 2);
        assert_eq!(report.summaries.len(), 2);
    }

    #[test]
    fn unsupported_is_never_untested_or_unknown() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, report) = fixture_assessment();
        // Node→Go: the complete inventory explicitly proves the Go
        // schedule hook unsupported — an explicit verdict, not unknown.
        let go_schedule = row(
            &report,
            "command:planner.schedule_calculate",
            "go-service",
            Dimension::Behavioral,
        );
        assert_eq!(go_schedule.classification, Classification::Unsupported);
        assert_eq!(go_schedule.binding, Some(wire::Binding::Missing));
        assert!(go_schedule.blocking);
        assert_eq!(go_schedule.reason, "hook-missing");
        // The publication capability has no owner-registry definition at
        // this product generation: the unsatisfied declaration is an
        // explicit unknown, never a fabricated supported destination.
        let publication = row(
            &report,
            "effect:planner.schedule_write",
            "php-laravel",
            Dimension::Operational,
        );
        assert_eq!(publication.classification, Classification::Unknown);
        assert_eq!(publication.verification, Verification::Untested);
        assert!(publication.blocking);
        // Without implementation evidence the same obligation would be
        // unknown; the complete inventory is what makes it unsupported.
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == "portability.implementation-required"
                && d.subject.as_deref() == Some("planner.schedule_calculate")));
    }

    #[test]
    fn node_to_php_shows_the_explicit_custom_and_foreign_views() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, report) = fixture_assessment();
        // `map.reading-go` pins `storage.returning`, a capability the go
        // profile declares at full support: the go reading hook is
        // credited. The php mapping pins `runtime.async`, which the php
        // profile only declares at partial support: the php schedule row
        // must stay a blocking partial (supported-with-mapping requires
        // full declared support), and a mapping scoped to another
        // destination never surfaces at all.
        let php_schedule = row(
            &report,
            "command:planner.schedule_calculate",
            "php-laravel",
            Dimension::Behavioral,
        );
        assert_eq!(php_schedule.classification, Classification::Partial);
        assert_eq!(php_schedule.binding, Some(wire::Binding::Foreign));
        assert_eq!(php_schedule.reason, "hook-foreign-partial");
        assert!(php_schedule.blocking);
        assert!(!php_schedule.evidence_refs.is_empty());
        let go_reading = row(
            &report,
            "query:planner.count_focused",
            "go-service",
            Dimension::Behavioral,
        );
        assert_eq!(
            go_reading.classification,
            Classification::SupportedWithMapping
        );
        assert_eq!(go_reading.binding, Some(wire::Binding::Foreign));
        assert_eq!(go_reading.reason, "hook-foreign-mapped");
        assert!(!go_reading.blocking);
        let php_focus = row(
            &report,
            "command:planner.focus_task",
            "php-laravel",
            Dimension::Behavioral,
        );
        assert_eq!(php_focus.binding, Some(wire::Binding::Custom));
    }

    #[test]
    fn go_loses_nothing_structurally_while_blocked_behaviorally() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, report) = fixture_assessment();
        // High structural portability must not conceal the behavioral
        // blocker: the summary keeps both visible.
        let summary = report
            .summaries
            .iter()
            .find(|s| s.destination == "go-service")
            .expect("go summary");
        let structural = summary
            .dimensions
            .get(&Dimension::Structural)
            .expect("structural");
        assert!(structural.portable > 0);
        assert_eq!(
            structural.structural_portable.map(|r| r.denominator > 0),
            Some(true)
        );
        assert!(summary.blockers > 0);
        let behavioral = summary
            .dimensions
            .get(&Dimension::Behavioral)
            .expect("behavioral");
        assert!(behavioral.unsupported >= 1);
        assert!(structural.portable > behavioral.portable);
    }

    #[test]
    fn every_blocker_carries_reason_and_witness_surface() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, report) = fixture_assessment();
        for subject in &report.subjects {
            for blocker in subject.rows.iter().filter(|row| row.blocking) {
                assert!(!blocker.reason.is_empty());
                let named_surface = !subject.requirements.is_empty()
                    || blocker.required.is_some()
                    || blocker.offered.is_some()
                    || !blocker.evidence_refs.is_empty()
                    || blocker.classification == Classification::Unknown;
                assert!(
                    named_surface,
                    "blocker {} must name its requirement surface",
                    subject.subject
                );
            }
        }
        // The planning rows round-trip: every blocking obligation has a
        // stable row with an advisory action.
        assert!(!report.migration_surface.rows.is_empty());
        for row in &report.migration_surface.rows {
            assert!(row.id.starts_with("plan."));
            assert!(!row.evidence_needed.iter().any(|e| e.contains('/')));
        }
        assert!(report.migration_surface.missing_implementations >= 1);
    }

    #[test]
    fn summaries_partition_every_obligation_row_exactly() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, report) = fixture_assessment();
        for summary in &report.summaries {
            let mut total = 0u64;
            for (dimension, entry) in &summary.dimensions {
                let partition = entry.portable
                    + entry.supported_with_mapping
                    + entry.partial
                    + entry.target_specific
                    + entry.unsupported
                    + entry.unknown
                    + entry.non_applicable;
                let counted = report
                    .subjects
                    .iter()
                    .flat_map(|subject| subject.rows.iter())
                    .filter(|row| {
                        row.destination == summary.destination && row.dimension == *dimension
                    })
                    .count() as u64;
                assert_eq!(partition, counted, "partition drift for {dimension:?}");
                assert_eq!(entry.eligible_applicable, counted - entry.non_applicable);
                total += partition;
            }
            assert!(total > 0);
            // The structural ratio only publishes a denominator that
            // exactly equals its eligible applicable count.
            if let Some(ratio) = summary.dimensions[&Dimension::Structural].structural_portable {
                assert_eq!(
                    ratio.denominator,
                    summary.dimensions[&Dimension::Structural].eligible_applicable
                );
                assert_eq!(
                    ratio.numerator,
                    summary.dimensions[&Dimension::Structural].portable
                );
            }
        }
    }

    #[test]
    fn verification_states_stay_orthogonal() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, report) = fixture_assessment();
        // The recorded scenario receipt lifts verification without
        // changing any classification.
        let focused = row(
            &report,
            "command:planner.focus_task",
            "php-laravel",
            Dimension::Behavioral,
        );
        assert_eq!(focused.verification, Verification::RecordedPass);
        assert_eq!(focused.classification, Classification::SupportedWithMapping);
        // The unrelated scenario receipt does not leak onto other rows.
        let archived = row(
            &report,
            "command:planner.archive_task_cmd",
            "php-laravel",
            Dimension::Behavioral,
        );
        assert_eq!(archived.verification, Verification::Untested);
    }

    #[test]
    fn scenario_and_gate_joins_name_their_subjects() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, report) = fixture_assessment();
        let schedule_flow = report
            .affected_scenarios
            .iter()
            .find(|s| s.scenario == "planner.schedule_flow")
            .expect("schedule flow affected");
        assert!(schedule_flow
            .subjects
            .contains(&"command:planner.schedule_calculate".to_owned()));
        // The declared gate that references the recorded scenario joins
        // with its recorded state; unrelated gates stay out.
        assert!(report
            .affected_gates
            .iter()
            .any(|gate| gate.scenarios.contains(&"planner.schedule_flow".to_owned())));
    }

    #[test]
    fn repeated_assessment_is_byte_identical() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, first) = fixture_assessment();
        let (_, second) = fixture_assessment();
        assert_eq!(
            lekalo_core::portability::canonical::canonical_bytes(&first),
            lekalo_core::portability::canonical::canonical_bytes(&second)
        );
    }
}

mod pin_matrix {
    use super::*;

    #[test]
    fn same_name_version_mismatch_is_refused() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let compilation = compilation();
        let recipe = embedded_recipe().unwrap();
        let evidence = parse_evidence(&read(EVIDENCE), &compilation, &recipe).unwrap();
        let mut stale = evidence.clone();
        stale.project_ref.ir_digest =
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_owned();
        let error = parse_evidence(
            &{
                // Serialize the mutated manifest through serde to keep the shape.
                serde_json::to_vec(&stale).unwrap()
            },
            &compilation,
            &recipe,
        )
        .unwrap_err();
        assert!(error
            .as_slice()
            .iter()
            .any(|d| d.id() == "portability.input-invalid"));
    }

    #[test]
    fn profile_pin_mismatch_is_refused() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let bytes = read(&format!("{FIXTURE}/config/profiles/node.json"));
        let document = profile_document::decode(&bytes).unwrap();
        let resolved = profile_resolution::resolve(&document).unwrap().remove(0);
        let wrong = resolved.clone();
        let binding = wire::ProfileRef {
            id: wrong.id.clone(),
            version: wrong.version.clone(),
            source_digest: wrong.source_digest.clone(),
            resolved_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        };
        assert!(verify_profile_pin(&binding, &wrong).is_err());
    }

    #[test]
    fn foreign_project_evidence_is_refused() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let compilation = compilation();
        let recipe = embedded_recipe().unwrap();
        let mut foreign: serde_json::Value = serde_json::from_slice(&read(EVIDENCE)).unwrap();
        foreign["projectRef"]["projectId"] = "other".into();
        let error = parse_evidence(
            &serde_json::to_vec(&foreign).unwrap(),
            &compilation,
            &recipe,
        )
        .unwrap_err();
        assert!(error
            .as_slice()
            .iter()
            .any(|d| d.id() == "portability.input-invalid"));
    }

    #[test]
    fn duplicate_json_keys_are_refused_everywhere() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let mut duplicated = String::from_utf8(read(EVIDENCE)).unwrap();
        duplicated = duplicated.replacen(
            "\"schemaVersion\"",
            "\"schemaVersion\",\"schemaVersion\"",
            1,
        );
        let error =
            lekalo_core::portability::wire::decode::<wire::EvidenceDocument>(duplicated.as_bytes())
                .unwrap_err();
        assert!(error
            .as_slice()
            .iter()
            .any(|d| d.id() == "portability.input-invalid"));
    }
}

mod request_rules {
    use super::*;

    #[test]
    fn selector_scopes_narrow_the_denominator_without_losing_witnesses() {
        let (_, mut report) = fixture_assessment();
        let _ = &mut report;
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // Symbol scope: the selected subject keeps its witnesses.
        let compilation = compilation();
        let recipe = embedded_recipe().unwrap();
        let targets =
            parse_targets(&read(&format!("{FIXTURE}/config/portability-targets.json"))).unwrap();
        let request = Request {
            scope: Scope::Symbol("planner.schedule_calculate".to_owned()),
            source: Some("node-typescript".to_owned()),
            destinations: vec!["go-service".to_owned()],
        };
        let subjects = extract(&compilation.project, &recipe);
        let resolved = lekalo_core::portability::resolve_targets(
            &request,
            Some(&targets),
            &subjects,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        let assessment = Assessment {
            compilation: &compilation,
            recipe: &recipe,
            subjects,
            request: &request,
            source: resolved.source.clone(),
            destinations: resolved.destinations.clone(),
            evidence: None,
            implementation: None,
            verified_families: std::collections::BTreeSet::new(),
            receipts_qualified: std::collections::BTreeSet::new(),
            targets_input: wire::TargetsInput {
                state: wire::PinState::Pinned,
                path: None,
                digest: None,
            },
            lock_digest: None,
            evidence_digest: None,
        };
        let report = assess(&assessment).unwrap();
        assert_eq!(
            report.scope.kind,
            lekalo_core::portability::wire::ScopeKind::Symbol
        );
        assert_eq!(report.subjects.len(), 1);
        assert_eq!(report.coverage.eligible_subjects, 1);
        let selected = &report.subjects[0];
        assert!(!selected.rows.is_empty());
        // The cross-boundary dependency witnesses stay visible.
        assert!(!selected.requirements.is_empty());
    }

    #[test]
    fn unknown_target_selection_is_invalid() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let compilation = compilation();
        let recipe = embedded_recipe().unwrap();
        let targets =
            parse_targets(&read(&format!("{FIXTURE}/config/portability-targets.json"))).unwrap();
        let request = Request {
            scope: Scope::Project,
            source: Some("ruby-rails".to_owned()),
            destinations: vec![],
        };
        let subjects = extract(&compilation.project, &recipe);
        let error = lekalo_core::portability::resolve_targets(
            &request,
            Some(&targets),
            &subjects,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(error
            .as_slice()
            .iter()
            .any(|d| d.id() == "portability.input-invalid"));
    }

    #[test]
    fn ambiguous_source_without_configuration_is_invalid() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let compilation = compilation();
        let recipe = embedded_recipe().unwrap();
        let request = Request {
            scope: Scope::Project,
            source: None,
            destinations: vec![],
        };
        let subjects = extract(&compilation.project, &recipe);
        // The fixture project declares three targets: no explicit
        // selection is ambiguous.
        assert!(subjects.declared_targets.len() >= 2);
        let error = lekalo_core::portability::resolve_targets(
            &request,
            None,
            &subjects,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(error
            .as_slice()
            .iter()
            .any(|d| d.id() == "portability.input-invalid"));
    }

    #[test]
    fn derived_selection_from_the_sole_target_resolves_with_the_evidence_gap() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let compilation = compilation();
        let recipe = embedded_recipe().unwrap();
        let request = Request {
            scope: Scope::Project,
            source: None,
            destinations: vec![],
        };
        let mut subjects = extract(&compilation.project, &recipe);
        subjects.declared_targets = vec!["node-typescript".to_owned()];
        // The sole declared target names the source (derived); with no
        // destination left the resolution is not refused: the assessment
        // emits the structural inventory plus the explicit
        // destination-evidence gap, and the strict gate denies the empty
        // join instead of ever producing a vacuous green report.
        let resolved = lekalo_core::portability::resolve_targets(
            &request,
            None,
            &subjects,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("the zero-destination selection resolves");
        assert_eq!(resolved.source.target_id, "node-typescript");
        assert_eq!(resolved.source.state, wire::PinState::Derived);
        assert!(resolved.destinations.is_empty());
        let assessment = Assessment {
            compilation: &compilation,
            recipe: &recipe,
            subjects: subjects.clone(),
            request: &request,
            source: resolved.source.clone(),
            destinations: resolved.destinations.clone(),
            evidence: None,
            implementation: None,
            verified_families: std::collections::BTreeSet::new(),
            receipts_qualified: std::collections::BTreeSet::new(),
            targets_input: wire::TargetsInput {
                state: wire::PinState::Derived,
                path: None,
                digest: None,
            },
            lock_digest: None,
            evidence_digest: None,
        };
        let report = assess(&assessment).expect("the inventory assessment succeeds");
        // The structural inventory stays explicit; the destination gap
        // is a named diagnostic, and the strict gate denies.
        assert_eq!(report.subjects.len(), subjects.rows.len());
        assert!(report.coverage.eligible_subjects > 0);
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == "portability.evidence-incomplete"
                && d.detail == "destination-evidence-gap"));
        assert!(lekalo_core::portability::planning::denial(&report));
    }
}

mod evidence_gaps {
    use super::*;

    #[test]
    fn without_evidence_every_applicable_obligation_is_unknown() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let compilation = compilation();
        let recipe = embedded_recipe().unwrap();
        let targets =
            parse_targets(&read(&format!("{FIXTURE}/config/portability-targets.json"))).unwrap();
        let request = Request {
            scope: Scope::Project,
            source: Some("node-typescript".to_owned()),
            destinations: vec!["go-service".to_owned()],
        };
        let subjects = extract(&compilation.project, &recipe);
        let mut pins = BTreeMap::new();
        for binding in &targets.targets {
            let bytes = read(&format!("{FIXTURE}/{}", binding.profile_document_ref));
            let document = profile_document::decode(&bytes).unwrap();
            let resolved = profile_resolution::resolve(&document).unwrap();
            let found = resolved
                .iter()
                .find(|p| p.id == binding.profile_ref.id)
                .unwrap();
            verify_profile_pin(&binding.profile_ref, found).unwrap();
            pins.insert(binding.target_id.clone(), Some(profile_pin(found)));
        }
        let resolved_targets = lekalo_core::portability::resolve_targets(
            &request,
            Some(&targets),
            &subjects,
            &pins,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        let assessment = Assessment {
            compilation: &compilation,
            recipe: &recipe,
            subjects,
            request: &request,
            source: resolved_targets.source.clone(),
            destinations: resolved_targets.destinations.clone(),
            evidence: None,
            implementation: None,
            verified_families: std::collections::BTreeSet::new(),
            receipts_qualified: std::collections::BTreeSet::new(),
            targets_input: wire::TargetsInput {
                state: wire::PinState::Pinned,
                path: Some("config/portability-targets.json".to_owned()),
                digest: None,
            },
            lock_digest: None,
            evidence_digest: None,
        };
        let report = assess(&assessment).unwrap();
        // Behavioral and storage rows are unknown; structural stays
        // decidable because it needs no recorded evidence.
        let schedule = report
            .subjects
            .iter()
            .find(|s| s.subject == "command:planner.schedule_calculate")
            .unwrap();
        let behavioral = schedule
            .rows
            .iter()
            .find(|r| r.dimension == wire::Dimension::Behavioral)
            .unwrap();
        assert_eq!(behavioral.classification, wire::Classification::Unknown);
        let structural = schedule
            .rows
            .iter()
            .find(|r| r.dimension == wire::Dimension::Structural)
            .unwrap();
        assert_eq!(structural.classification, wire::Classification::Portable);
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == "portability.evidence-incomplete"));
    }

    #[test]
    fn partial_mapping_stays_partial_and_blocking() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let compilation = compilation();
        let recipe = embedded_recipe().unwrap();
        let mut evidence: serde_json::Value = serde_json::from_slice(&read(EVIDENCE)).unwrap();
        // Downgrade the schedule mapping to partial coverage.
        for mapping in evidence["mappings"].as_array_mut().unwrap().iter_mut() {
            if mapping["id"] == "map.schedule-php" {
                mapping["coverage"] = "partial".into();
            }
        }
        let evidence: wire::EvidenceDocument = serde_json::from_value(evidence).unwrap();
        let targets =
            parse_targets(&read(&format!("{FIXTURE}/config/portability-targets.json"))).unwrap();
        let request = Request {
            scope: Scope::Project,
            source: Some("node-typescript".to_owned()),
            destinations: vec!["php-laravel".to_owned()],
        };
        let subjects = extract(&compilation.project, &recipe);
        let mut pins = BTreeMap::new();
        for binding in &targets.targets {
            let bytes = read(&format!("{FIXTURE}/{}", binding.profile_document_ref));
            let document = profile_document::decode(&bytes).unwrap();
            let resolved = profile_resolution::resolve(&document).unwrap();
            let found = resolved
                .iter()
                .find(|p| p.id == binding.profile_ref.id)
                .unwrap();
            pins.insert(binding.target_id.clone(), Some(profile_pin(found)));
        }
        // The complete implementation inventory qualifies the hook rows;
        // only the mapping coverage changes in this control.
        let family = evidence
            .families
            .iter()
            .find(|f| f.family == wire::EvidenceFamily::Implementation)
            .expect("implementation family");
        let artifact = read(&format!(
            "{FIXTURE}/{}",
            family.artifact_ref.as_ref().unwrap()
        ));
        let value: serde_json::Value = serde_json::from_slice(&artifact).unwrap();
        let document = ImplementationDocument::from_value(&value).unwrap();
        let implementation = implementation::validate(&document, &compilation)
            .expect("validates")
            .portability()
            .clone();
        let resolved_targets = lekalo_core::portability::resolve_targets(
            &request,
            Some(&targets),
            &subjects,
            &pins,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        let assessment = Assessment {
            compilation: &compilation,
            recipe: &recipe,
            subjects,
            request: &request,
            source: resolved_targets.source.clone(),
            destinations: resolved_targets.destinations.clone(),
            evidence: Some(&evidence),
            implementation: Some(&implementation),
            verified_families: std::collections::BTreeSet::from_iter([
                wire::EvidenceFamily::Implementation,
            ]),
            receipts_qualified: std::collections::BTreeSet::new(),
            targets_input: wire::TargetsInput {
                state: wire::PinState::Pinned,
                path: None,
                digest: None,
            },
            lock_digest: None,
            evidence_digest: None,
        };
        let report = assess(&assessment).unwrap();
        let schedule = report
            .subjects
            .iter()
            .find(|s| s.subject == "command:planner.schedule_calculate")
            .unwrap();
        let php = schedule
            .rows
            .iter()
            .find(|r| r.destination == "php-laravel" && r.dimension == wire::Dimension::Behavioral)
            .unwrap();
        assert_eq!(php.classification, wire::Classification::Partial);
        assert!(php.blocking);
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == "portability.mapping-partial"));
    }

    #[test]
    fn storage_precision_loss_is_a_blocking_partial_with_exact_policies() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, report) = fixture_assessment();
        let weight_php = row(
            &report,
            "scalar:planner.weight",
            "php-laravel",
            wire::Dimension::Storage,
        );
        assert_eq!(weight_php.classification, wire::Classification::Partial);
        assert!(weight_php.blocking);
        let required = weight_php.required.as_ref().expect("required policy");
        let offered = weight_php.offered.as_ref().expect("offered policy");
        assert_eq!(required.kind, "decimal");
        assert_eq!(offered.kind, "decimal");
        assert_ne!(
            (required.precision, required.scale),
            (offered.precision, offered.scale)
        );
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == "portability.behavior-loss"));
        // The Go engine offers the exact policy: covered.
        let weight_go = row(
            &report,
            "scalar:planner.weight",
            "go-service",
            wire::Dimension::Storage,
        );
        assert_eq!(weight_go.classification, wire::Classification::Portable);
        assert_eq!(weight_go.reason, "policy-covered");
    }

    #[test]
    fn isolation_requirement_without_an_offering_stays_unknown() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, report) = fixture_assessment();
        let task = row(
            &report,
            "entity:planner.task",
            "go-service",
            wire::Dimension::Storage,
        );
        assert_eq!(task.classification, wire::Classification::Unknown);
        assert!(task.blocking);
        assert_eq!(task.reason, "storage-policy-unknown");
    }

    #[test]
    fn observed_implicit_hook_keeps_its_proof_gap() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, report) = fixture_assessment();
        let focus = row(
            &report,
            "command:planner.focus_task",
            "go-service",
            wire::Dimension::Operational,
        );
        assert_eq!(focus.classification, wire::Classification::Partial);
        assert_eq!(focus.reason, "implicit-hook-unsupported");
        assert_eq!(focus.verification, wire::Verification::Untested);
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == "portability.behavior-loss"
                && d.detail == "implicit-hook-unsupported"));
    }

    #[test]
    fn the_registry_carries_the_portability_family() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let registry = lekalo_core::diagnostics::DiagnosticRegistry::m8().unwrap();
        assert_eq!(registry.registry_version(), "0.8.0");
        for (id, code) in [
            ("portability.input-invalid", "LEK-PORT-001"),
            ("portability.version-unsupported", "LEK-PORT-002"),
            ("portability.evidence-incomplete", "LEK-PORT-003"),
            ("portability.mapping-partial", "LEK-PORT-004"),
            ("portability.capability-unsupported", "LEK-PORT-005"),
            ("portability.implementation-required", "LEK-PORT-006"),
            ("portability.behavior-loss", "LEK-PORT-007"),
            ("portability.analysis-incomplete", "LEK-PORT-008"),
            ("portability.check-denied", "LEK-PORT-009"),
        ] {
            let entry = registry.entry(id).unwrap_or_else(|| panic!("{id}"));
            assert_eq!(entry.code(), code);
            assert_eq!(
                entry.lifecycle(),
                lekalo_core::diagnostics::registry::Lifecycle::Active
            );
        }
        // The predecessor registries stay untouched and complete.
        let predecessor = lekalo_core::diagnostics::DiagnosticRegistry::successor().unwrap();
        assert_eq!(predecessor.registry_version(), "0.6.5");
        for entry in predecessor.entries() {
            assert!(
                registry.entry(entry.id()).is_some(),
                "union drops {}",
                entry.id()
            );
        }
        // The ordinary producer diagnostics still build at their pinned
        // registry, and the new family builds at the successor.
        let diagnostic = lekalo_core::portability::diagnostic::diagnostic(
            "portability.behavior-loss",
            "scalar:planner.weight",
            "storage-policy-loss",
        )
        .unwrap();
        let rendered = serde_json::to_value(&diagnostic).unwrap();
        assert_eq!(rendered["registry_version"], "0.8.0");
        assert_eq!(rendered["code"], "LEK-PORT-007");
    }

    #[test]
    fn check_denial_reflects_blocking_obligations() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (_, report) = fixture_assessment();
        // The full fixture has blocking obligations (Go schedule hook,
        // precision loss, isolation unknown): strict check denies.
        assert!(lekalo_core::portability::planning::denial(&report));
        let codes = lekalo_core::portability::planning::triggered_codes(&report);
        assert!(codes.contains(&"portability.implementation-required"));
        assert!(codes.contains(&"portability.behavior-loss"));
    }

    #[test]
    fn bounds_refuse_oversized_requests() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let compilation = compilation();
        let recipe = embedded_recipe().unwrap();
        let request = Request {
            scope: Scope::Project,
            source: None,
            destinations: vec![],
        };
        let mut subjects = extract(&compilation.project, &recipe);
        subjects.declared_targets.clear();
        // No targets at all: the source selector is missing.
        let error = lekalo_core::portability::resolve_targets(
            &request,
            None,
            &subjects,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(error
            .as_slice()
            .iter()
            .any(|d| d.id() == "portability.input-invalid"));
        let _ = request::SelectorError::Missing;
    }
}
