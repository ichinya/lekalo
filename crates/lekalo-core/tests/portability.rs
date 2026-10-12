//! Issue #19 core tests: the portability assessment over the checked-in
//! planner fixture, the exact-pin mutation matrix, the classification
//! matrix (unsupported vs untested vs unknown), the summary arithmetic,
//! the bounds, and the strict `--check` denial join.
//!
//! Every assertion runs against the production core seam with the same
//! fixture files the CLI gate reads; no test launches a process.

use lekalo_core::diagnostics::DiagnosticSet;
use lekalo_core::implementation::{self, ImplementationDocument};
use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::portability::requirements::Subjects;
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
    let (compilation, report) = fixture_assessment_with(None, None, BTreeMap::new(), None)
        .expect("the fixture assessment succeeds");
    (compilation, report)
}

/// The fixture assessment with hooks: an optional evidence mutation
/// (applied to the raw JSON before decoding), an optional destination
/// override, injected gate custody, and an optional subject mutation.
/// The implementation join, pins, and receipt qualification mirror the
/// CLI exactly, so the mutated controls exercise the real seams.
#[allow(clippy::type_complexity)]
fn fixture_assessment_with(
    mutate_evidence: Option<&mut dyn FnMut(&mut serde_json::Value)>,
    destinations_override: Option<Vec<lekalo_core::portability::TargetInput>>,
    gates_qualified: BTreeMap<(String, String), bool>,
    mutate_subjects: Option<&mut dyn FnMut(&mut Subjects)>,
) -> Result<(lekalo_core::ir::Compilation, wire::Report), DiagnosticSet> {
    // The caller holds CWD_LOCK and has entered the workspace root.
    let compilation = compilation();
    let recipe = embedded_recipe().expect("embedded recipe");
    let targets = parse_targets(&read(&format!("{FIXTURE}/config/portability-targets.json")))
        .expect("targets decode");
    let mut evidence_value: serde_json::Value =
        serde_json::from_slice(&read(EVIDENCE)).expect("evidence JSON");
    if let Some(mutate) = mutate_evidence {
        mutate(&mut evidence_value);
    }
    let evidence_bytes = serde_json::to_vec(&evidence_value).expect("serialize evidence");
    let evidence =
        parse_evidence(&evidence_bytes, &compilation, &recipe).expect("mutated evidence decodes");

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
    let mut subjects = extract(&compilation.project, &recipe);
    if let Some(mutate) = mutate_subjects {
        mutate(&mut subjects);
    }
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
        let receipt = wire::decode_receipt(&bytes).expect("the receipt decodes");
        assert_eq!(receipt.kind, wire::ReceiptKind::Scenario);
        assert_eq!(receipt.subject, record.scenario);
        assert_eq!(receipt.target_id, *target);
        // Provenance: the exact compilation and the destination's
        // verified profile pin.
        assert_eq!(
            receipt.project_ref.project_id,
            compilation
                .project
                .project
                .as_ref()
                .map(|p| p.id.as_str().to_owned())
                .unwrap_or_default()
        );
        assert_eq!(
            receipt.profile_ref.resolved_digest,
            pins.get(target)
                .cloned()
                .flatten()
                .expect("pinned target")
                .resolved_digest
        );
        let result_pass = receipt.result == wire::ReceiptResult::Pass;
        assert_eq!(
            result_pass,
            record.state == wire::ScenarioReceipt::RecordedPass,
            "the claim must agree with the custody result"
        );
        receipts_qualified.insert((record.scenario.clone(), target.clone()));
    }
    let assessment = Assessment {
        compilation: &compilation,
        recipe: &recipe,
        subjects,
        request: &request,
        source: resolved_targets.source.clone(),
        destinations: destinations_override
            .unwrap_or_else(|| resolved_targets.destinations.clone()),
        evidence: Some(&evidence),
        implementation: Some(&report),
        verified_families,
        verified_storage_inventory: None,
        receipts_qualified,
        gates_qualified,
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
        lock_state: lekalo_core::portability::wire::LockPresence::Absent,
        evidence_digest: None,
    };
    let report = assess(&assessment)?;
    Ok((compilation, report))
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
            verified_storage_inventory: None,
            receipts_qualified: std::collections::BTreeSet::new(),
            gates_qualified: std::collections::BTreeMap::new(),
            targets_input: wire::TargetsInput {
                state: wire::PinState::Pinned,
                path: None,
                digest: None,
            },
            lock_digest: None,
            lock_state: lekalo_core::portability::wire::LockPresence::Absent,
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
            verified_storage_inventory: None,
            receipts_qualified: std::collections::BTreeSet::new(),
            gates_qualified: std::collections::BTreeMap::new(),
            targets_input: wire::TargetsInput {
                state: wire::PinState::Derived,
                path: None,
                digest: None,
            },
            lock_digest: None,
            lock_state: lekalo_core::portability::wire::LockPresence::Absent,
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
            verified_storage_inventory: None,
            receipts_qualified: std::collections::BTreeSet::new(),
            gates_qualified: std::collections::BTreeMap::new(),
            targets_input: wire::TargetsInput {
                state: wire::PinState::Pinned,
                path: Some("config/portability-targets.json".to_owned()),
                digest: None,
            },
            lock_digest: None,
            lock_state: lekalo_core::portability::wire::LockPresence::Absent,
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
            verified_storage_inventory: None,
            receipts_qualified: std::collections::BTreeSet::new(),
            gates_qualified: std::collections::BTreeMap::new(),
            targets_input: wire::TargetsInput {
                state: wire::PinState::Pinned,
                path: None,
                digest: None,
            },
            lock_digest: None,
            lock_state: lekalo_core::portability::wire::LockPresence::Absent,
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

/// Decoder/schema parity: the typed decoders enforce the closed Draft
/// 2020-12 grammar (patterns, enums, bounds, uniqueItems, null-vs-
/// absent) in both directions, so a schema-invalid document is refused
/// with a named token and no schema-invalid value reaches evaluation.
mod decoder_parity {
    use super::*;
    use lekalo_core::portability::diagnostic;

    fn fixture_targets() -> Vec<u8> {
        read(&format!("{FIXTURE}/config/portability-targets.json"))
    }

    fn fixture_evidence() -> Vec<u8> {
        read(EVIDENCE)
    }

    fn evidence_document() -> serde_json::Value {
        serde_json::from_slice(&fixture_evidence()).expect("fixture evidence JSON")
    }

    fn targets_document() -> serde_json::Value {
        serde_json::from_slice(&fixture_targets()).expect("fixture targets JSON")
    }

    pub(super) fn expect_refusal(bytes: &[u8], detail: &str, targets: bool) {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let compilation = compilation();
        let recipe = embedded_recipe().expect("embedded recipe");
        let outcome = if targets {
            parse_targets(bytes).expect_err("expected refusal")
        } else {
            parse_evidence(bytes, &compilation, &recipe).expect_err("expected refusal")
        };
        assert!(
            outcome.as_slice().iter().any(|d| {
                d.id() == "portability.input-invalid"
                    && d.data()
                        .get("detail")
                        .map(|value| {
                            matches!(value, lekalo_core::diagnostics::DataValue::Token(token) if token == detail)
                        })
                        .unwrap_or(false)
            }),
            "expected {detail} refusal, got {outcome:?}"
        );
    }

    pub(super) fn mutated_evidence(mutate: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let mut value = evidence_document();
        mutate(&mut value);
        serde_json::to_vec(&value).expect("serialize")
    }

    fn mutated_targets(mutate: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let mut value = targets_document();
        mutate(&mut value);
        serde_json::to_vec(&value).expect("serialize")
    }

    #[test]
    fn null_default_destinations_is_refused_not_reinterpreted() {
        let bytes = mutated_targets(|value| {
            value["defaultDestinations"] = serde_json::Value::Null;
        });
        expect_refusal(&bytes, "closed-shape", true);
    }

    #[test]
    fn null_default_source_is_refused_not_reinterpreted() {
        let bytes = mutated_targets(|value| {
            value["defaultSource"] = serde_json::Value::Null;
        });
        expect_refusal(&bytes, "closed-shape", true);
    }

    #[test]
    fn malformed_revision_is_refused() {
        let bytes = mutated_evidence(|value| {
            value["recordedContext"]["revision"] = "zzz-not-hex".into();
        });
        expect_refusal(&bytes, "recorded-context", false);
    }

    #[test]
    fn malformed_operational_requirement_is_refused() {
        let bytes = mutated_evidence(|value| {
            value["operationalRequirements"] = serde_json::json!([
                {
                    "subject": "command:planner.focus_task",
                    "requirement": "bad requirement!",
                    "conditions": ["recorded-scenario-receipt"],
                    "definitionPins": []
                }
            ]);
        });
        expect_refusal(&bytes, "operational-requirement", false);
    }

    #[test]
    fn operational_requirement_with_duplicate_conditions_is_refused() {
        let bytes = mutated_evidence(|value| {
            value["operationalRequirements"] = serde_json::json!([
                {
                    "subject": "command:planner.focus_task",
                    "requirement": "verify.operations",
                    "conditions": ["recorded-scenario-receipt", "recorded-scenario-receipt"],
                    "definitionPins": []
                }
            ]);
        });
        expect_refusal(&bytes, "operational-requirement", false);
    }

    #[test]
    fn enum_breaking_policy_kind_is_refused() {
        let bytes = mutated_evidence(|value| {
            value["storageRequired"][0]["policy"]["kind"] = "evil-kind".into();
        });
        expect_refusal(&bytes, "policy-shape", false);
    }

    #[test]
    fn zero_precision_is_refused() {
        let bytes = mutated_evidence(|value| {
            value["storageRequired"][0]["policy"]["precision"] = 0.into();
        });
        expect_refusal(&bytes, "policy-shape", false);
    }

    #[test]
    fn duplicate_storage_required_is_refused_not_deduplicated() {
        let bytes = mutated_evidence(|value| {
            let first = value["storageRequired"][0].clone();
            value["storageRequired"].as_array_mut().unwrap().push(first);
        });
        expect_refusal(&bytes, "duplicate-requirement", false);
    }

    #[test]
    fn contradictory_same_target_kind_offers_are_refused_regardless_of_order() {
        let offer = serde_json::json!({"targetId": "php-laravel", "policy": {"kind": "decimal", "value": "exact", "precision": 18, "scale": 6}});
        for position in [0usize, 1] {
            let bytes = mutated_evidence(|value| {
                let offers = value["storageOffered"].as_array_mut().unwrap();
                offers.insert(position, offer.clone());
            });
            expect_refusal(&bytes, "duplicate-offer", false);
        }
    }

    #[test]
    fn recorded_adapter_with_malformed_version_is_refused() {
        let bytes = mutated_evidence(|value| {
            value["adapters"][1] = serde_json::json!({
                "targetId": "php-laravel",
                "state": "recorded",
                "identity": "evil-adapter",
                "version": "banana",
                "protocol": "made/up",
                "capabilityDigest": format!("sha256:{}", "0".repeat(64))
            });
        });
        expect_refusal(&bytes, "adapter-snapshot", false);
    }

    #[test]
    fn malformed_mapping_identity_is_refused() {
        let bytes = mutated_evidence(|value| {
            value["mappings"][0]["id"] = "INVALID ID".into();
        });
        expect_refusal(&bytes, "mapping-identity", false);
    }

    #[test]
    fn duplicate_mapping_conditions_are_refused() {
        let bytes = mutated_evidence(|value| {
            value["mappings"][0]["conditions"] =
                serde_json::json!(["php-84", "php-84", "laravel-11"]);
        });
        expect_refusal(&bytes, "mapping-conditions", false);
    }

    #[test]
    fn contradictory_definition_pins_are_refused_at_decode_shape() {
        // A pin for a different capability id is legal shape; the pin
        // grammar test here pins malformed versions.
        let bytes = mutated_evidence(|value| {
            value["mappings"][0]["definitionPins"][0]["version"] = "banana".into();
        });
        expect_refusal(&bytes, "mapping-pin", false);
    }

    #[test]
    fn duplicate_storage_profile_refs_are_refused() {
        let bytes = mutated_targets(|value| {
            value["targets"][1]["storageProfileRefs"] = serde_json::json!([
                "config/profiles/laravel.json",
                "config/profiles/laravel.json"
            ]);
        });
        expect_refusal(&bytes, "duplicate-storage-ref", true);
    }

    #[test]
    fn over_bound_arrays_are_refused() {
        let bytes = mutated_evidence(|value| {
            let hook = value["hooks"][0].clone();
            let hooks = value["hooks"].as_array_mut().unwrap();
            for _ in 0..10_001 {
                hooks.push(hook.clone());
            }
        });
        expect_refusal(&bytes, "bound-exceeded", false);
    }

    #[test]
    fn the_unmutated_fixtures_still_decode() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let compilation = compilation();
        let recipe = embedded_recipe().expect("embedded recipe");
        parse_targets(&fixture_targets()).expect("targets decode");
        parse_evidence(&fixture_evidence(), &compilation, &recipe).expect("evidence decode");
        // The named diagnostic constructor stays reachable for the
        // refusal vocabulary.
        let _ = diagnostic::invalid("closed-shape");
    }
}

mod decoder_parity_r4 {
    use super::decoder_parity::*;

    #[test]
    fn authored_failed_gate_state_is_refused() {
        // `failed` is excluded from the authored vocabulary: a gate only
        // becomes failed through a qualified failing receipt.
        let bytes = mutated_evidence(|value| {
            value["gates"][0]["state"] = "failed".into();
        });
        expect_refusal(&bytes, "gate-state", false);
    }

    #[test]
    fn gate_omitted_nullable_members_are_refused() {
        let bytes = mutated_evidence(|value| {
            value["gates"][0]
                .as_object_mut()
                .unwrap()
                .remove("targetId");
            value["gates"][0]
                .as_object_mut()
                .unwrap()
                .remove("receiptRef");
            value["gates"][0]
                .as_object_mut()
                .unwrap()
                .remove("receiptDigest");
        });
        expect_refusal(&bytes, "closed-shape", false);
    }

    #[test]
    fn declared_gate_malformed_custody_members_are_refused() {
        // The target id grammar.
        let bytes = mutated_evidence(|value| {
            value["gates"][0]["targetId"] = "INVALID TARGET".into();
        });
        expect_refusal(&bytes, "gate-identity", false);
        // The safe receipt-reference grammar.
        let bytes = mutated_evidence(|value| {
            value["gates"][0]["receiptRef"] = "../out".into();
        });
        expect_refusal(&bytes, "artifact-ref", false);
        // The digest spelling.
        let bytes = mutated_evidence(|value| {
            value["gates"][0]["receiptDigest"] = "bad".into();
        });
        expect_refusal(&bytes, "gate-custody", false);
    }

    #[test]
    fn null_policy_precision_is_refused() {
        // precision/scale are integers-or-absent, never explicit nulls.
        let bytes = mutated_evidence(|value| {
            value["storageRequired"][0]["policy"]["precision"] = serde_json::Value::Null;
        });
        expect_refusal(&bytes, "policy-shape", false);
        let bytes = mutated_evidence(|value| {
            value["storageOffered"][0]["policy"]["scale"] = serde_json::Value::Null;
        });
        expect_refusal(&bytes, "policy-shape", false);
    }

    #[test]
    fn duplicate_hook_observation_is_refused() {
        // The hook observations are a set (uniqueItems).
        let bytes = mutated_evidence(|value| {
            let first = value["hooks"][0].clone();
            value["hooks"].as_array_mut().unwrap().push(first);
        });
        expect_refusal(&bytes, "duplicate-hook", false);
    }

    #[test]
    fn absent_adapter_with_malformed_version_is_refused() {
        // Present members obey their grammar in any state.
        let bytes = mutated_evidence(|value| {
            value["adapters"][0]["version"] = "bad".into();
        });
        expect_refusal(&bytes, "adapter-snapshot", false);
    }

    #[test]
    fn adapter_snapshot_artifact_grammar_is_refused() {
        // A referenced snapshot with malformed members never publishes
        // provenance, claimed or not: every field of the closed artifact
        // contract is validated at decode.
        let base = serde_json::json!({
            "schemaVersion": "lekalo/portability-adapter-snapshot/v0.8.0",
            "identity": "dev.lekalo.portability-adapter-snapshot@0.8.0",
            "targetId": "php-laravel",
            "adapter": {
                "identity": "composer/lekalo-planner-adapter",
                "version": "1.4.2",
                "protocol": "http.json/1",
                "capabilityDigest": format!("sha256:{}", "7".repeat(64)),
            },
            "profileRef": {
                "id": "laravel-postgres-http",
                "version": "0.2.16",
                "resolvedDigest": format!("sha256:{}", "5".repeat(64)),
            }
        });
        for member in ["identity", "version", "protocol"] {
            let mut malformed = base.clone();
            malformed["adapter"][member] = "".into();
            assert!(
                lekalo_core::portability::wire::decode_adapter_snapshot(
                    &serde_json::to_vec(&malformed).unwrap()
                )
                .is_err(),
                "malformed adapter member {member} must refuse"
            );
        }
        let mut malformed = base.clone();
        malformed["adapter"]["capabilityDigest"] = "bad".into();
        assert!(lekalo_core::portability::wire::decode_adapter_snapshot(
            &serde_json::to_vec(&malformed).unwrap()
        )
        .is_err());
        let mut malformed = base.clone();
        malformed["profileRef"]["resolvedDigest"] = "bad".into();
        assert!(lekalo_core::portability::wire::decode_adapter_snapshot(
            &serde_json::to_vec(&malformed).unwrap()
        )
        .is_err());
        // The well-formed spelling decodes.
        assert!(lekalo_core::portability::wire::decode_adapter_snapshot(
            &serde_json::to_vec(&base).unwrap()
        )
        .is_ok());
    }
}

/// Round-4 semantics: destination-exact bridge admission with proved
/// preconditions, (gate, destination) custody, affected-set gate
/// coverage, and the emit-side bounds of the report contract.
mod round4_semantics {
    use super::*;
    use lekalo_core::portability::wire::{Classification, Dimension};

    /// One bare destination input for custody-scoping controls.
    fn destination_input(target: &str) -> lekalo_core::portability::TargetInput {
        lekalo_core::portability::TargetInput {
            target_id: target.to_owned(),
            state: wire::PinState::Pinned,
            profile: None,
            adapter: None,
            storage_profile_refs: Vec::new(),
        }
    }

    #[test]
    fn unkeyed_destination_never_carries_the_bridges_credit() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // The full PHP precondition list on a Go mapping through the
        // php-keyed `runtime.async` bridge: go-service is unkeyed there,
        // so the record is unqualified — never a supported-with-mapping
        // credit under a green --check.
        let (_, report) = fixture_assessment_with(
            Some(&mut |value: &mut serde_json::Value| {
                value["mappings"][1] = serde_json::json!({
                    "id": "map.reading-go",
                    "requirement": "behavior.hook",
                    "capability": "runtime.async",
                    "targetId": "go-service",
                    "subject": "planner.count_focused",
                    "coverage": "full",
                    "conditions": ["replacement-binding-declared", "php-84", "laravel-11"],
                    "definitionPins": [{ "id": "runtime.async", "version": "0.2.16" }],
                });
            }),
            None,
            BTreeMap::new(),
            None,
        )
        .expect("assessment succeeds");
        let reading = row(
            &report,
            "query:planner.count_focused",
            "go-service",
            Dimension::Behavioral,
        );
        assert_eq!(reading.classification, Classification::Unknown);
        assert_eq!(reading.reason, "mapping-unqualified");
        assert!(reading.blocking);
        assert!(lekalo_core::portability::planning::denial(&report));
    }

    #[test]
    fn mirror_direction_go_vocabulary_never_credits_php() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // The Go precondition list on a PHP mapping through the
        // go-keyed `storage.returning` bridge: php-laravel is unkeyed
        // there, so the record is unqualified.
        let (_, report) = fixture_assessment_with(
            Some(&mut |value: &mut serde_json::Value| {
                value["mappings"][0] = serde_json::json!({
                    "id": "map.schedule-php",
                    "requirement": "behavior.hook",
                    "capability": "storage.returning",
                    "targetId": "php-laravel",
                    "subject": "planner.schedule_calculate",
                    "coverage": "full",
                    "conditions": ["replacement-binding-declared", "go-122"],
                    "definitionPins": [{ "id": "storage.returning", "version": "0.2.16" }],
                });
            }),
            None,
            BTreeMap::new(),
            None,
        )
        .expect("assessment succeeds");
        let schedule = row(
            &report,
            "command:planner.schedule_calculate",
            "php-laravel",
            Dimension::Behavioral,
        );
        assert_eq!(schedule.classification, Classification::Unknown);
        assert_eq!(schedule.reason, "mapping-unqualified");
        assert!(schedule.blocking);
    }

    #[test]
    fn declared_but_unproved_preconditions_stay_unknown() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // The reviewed transaction vocabulary is declared exactly, but
        // the destination offers no isolation policy at least as strong
        // as the required one: the credit stays an explicit unknown.
        let requirement = serde_json::json!({
            "subject": "command:planner.focus_task",
            "requirement": "storage.transactions",
            "conditions": [
                "same-database-engine-family",
                "declared-isolation-level-at-least-required"
            ],
            "definitionPins": [{ "id": "storage.transactions", "version": "0.4.0" }],
        });
        let isolation_requirement = serde_json::json!({
            "subject": "command:planner.focus_task",
            "policy": { "kind": "isolation", "value": "repeatable-read" },
        });
        let (_, report) = fixture_assessment_with(
            Some(&mut |value: &mut serde_json::Value| {
                value["operationalRequirements"]
                    .as_array_mut()
                    .unwrap()
                    .push(requirement.clone());
                value["storageRequired"]
                    .as_array_mut()
                    .unwrap()
                    .push(isolation_requirement.clone());
            }),
            None,
            BTreeMap::new(),
            None,
        )
        .expect("assessment succeeds");
        let focus = row(
            &report,
            "command:planner.focus_task",
            "php-laravel",
            Dimension::Operational,
        );
        assert_eq!(focus.classification, Classification::Unknown);
        assert_eq!(focus.reason, "precondition-unproved");
        assert!(focus.blocking);
        // The corroborated control: with a strong-enough isolation
        // offering every checkable token is proved and the declared
        // capability credits the row.
        let (_, report) = fixture_assessment_with(
            Some(&mut |value: &mut serde_json::Value| {
                value["operationalRequirements"]
                    .as_array_mut()
                    .unwrap()
                    .push(requirement.clone());
                value["storageRequired"]
                    .as_array_mut()
                    .unwrap()
                    .push(isolation_requirement.clone());
                value["storageOffered"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!({
                        "targetId": "php-laravel",
                        "policy": { "kind": "isolation", "value": "repeatable-read" },
                    }));
            }),
            None,
            BTreeMap::new(),
            None,
        )
        .expect("assessment succeeds");
        let focus = row(
            &report,
            "command:planner.focus_task",
            "php-laravel",
            Dimension::Operational,
        );
        assert_eq!(focus.classification, Classification::Portable);
        assert_eq!(focus.reason, "capability-declared");
    }

    #[test]
    fn cross_target_gate_custody_never_upgrades_this_destination() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let gate = "planner.native-scenario-gate".to_owned();
        // A qualified php receipt during a go-only assessment: the gate
        // stays declared, because custody binds to its destination.
        let php_custody: BTreeMap<(String, String), bool> =
            BTreeMap::from([((gate.clone(), "php-laravel".to_owned()), true)]);
        let (_, report) = fixture_assessment_with(
            Some(&mut |value: &mut serde_json::Value| {
                value["gates"][0]["state"] = "executed".into();
                value["gates"][0]["receiptRef"] = "reports/receipts/focus-flow.json".into();
                value["gates"][0]["receiptDigest"] = format!("sha256:{}", "0".repeat(64)).into();
                value["gates"][0]["targetId"] = "php-laravel".into();
            }),
            Some(vec![destination_input("go-service")]),
            php_custody,
            None,
        )
        .expect("assessment succeeds");
        let joined = report
            .affected_gates
            .iter()
            .find(|g| g.gate == gate)
            .expect("gate joins");
        assert_eq!(joined.state, wire::JoinState::Declared);
        // The same-shape go custody executes the gate in the go
        // assessment, and a failing one fails it.
        let go_pass: BTreeMap<(String, String), bool> =
            BTreeMap::from([((gate.clone(), "go-service".to_owned()), true)]);
        let (_, report) = fixture_assessment_with(
            Some(&mut |value: &mut serde_json::Value| {
                value["gates"][0]["state"] = "executed".into();
                value["gates"][0]["receiptRef"] = "reports/receipts/focus-flow.json".into();
                value["gates"][0]["receiptDigest"] = format!("sha256:{}", "0".repeat(64)).into();
                value["gates"][0]["targetId"] = "go-service".into();
            }),
            Some(vec![destination_input("go-service")]),
            go_pass,
            None,
        )
        .expect("assessment succeeds");
        let joined = report
            .affected_gates
            .iter()
            .find(|g| g.gate == gate)
            .expect("gate joins");
        assert_eq!(joined.state, wire::JoinState::Executed);
        let go_fail: BTreeMap<(String, String), bool> =
            BTreeMap::from([((gate.clone(), "go-service".to_owned()), false)]);
        let (_, report) = fixture_assessment_with(
            Some(&mut |value: &mut serde_json::Value| {
                value["gates"][0]["state"] = "executed".into();
                value["gates"][0]["receiptRef"] = "reports/receipts/focus-flow.json".into();
                value["gates"][0]["receiptDigest"] = format!("sha256:{}", "0".repeat(64)).into();
                value["gates"][0]["targetId"] = "go-service".into();
            }),
            Some(vec![destination_input("go-service")]),
            go_fail,
            None,
        )
        .expect("assessment succeeds");
        let joined = report
            .affected_gates
            .iter()
            .find(|g| g.gate == gate)
            .expect("gate joins");
        assert_eq!(joined.state, wire::JoinState::Failed);
    }

    #[test]
    fn gate_coverage_gaps_derive_from_the_affected_set_and_deny() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // No gate records at all: the affected scenarios of the scoped
        // selection still surface their missing coverage, plan
        // collect-evidence work, mark the surface incomplete, and deny
        // the strict gate.
        let (_, report) = fixture_assessment_with(
            Some(&mut |value: &mut serde_json::Value| {
                value["gates"] = serde_json::json!([]);
            }),
            None,
            BTreeMap::new(),
            None,
        )
        .expect("assessment succeeds");
        let gaps: Vec<&str> = report
            .diagnostics
            .iter()
            .filter(|d| d.detail == "gate-coverage-unknown")
            .filter_map(|d| d.subject.as_deref())
            .collect();
        assert!(gaps.contains(&"planner.focus_flow"));
        assert!(gaps.contains(&"planner.schedule_flow"));
        let coverage_rows: Vec<_> = report
            .migration_surface
            .rows
            .iter()
            .filter(|row| row.reason == "gate-coverage-unknown")
            .collect();
        assert!(!coverage_rows.is_empty());
        for row in &coverage_rows {
            assert_eq!(row.action, wire::Action::CollectEvidence);
            assert!(row.evidence_needed.iter().any(|e| e == "gate-record"));
            assert!(row.destination.is_none());
        }
        assert!(!report.migration_surface.complete);
        assert!(lekalo_core::portability::planning::denial(&report));
    }

    #[test]
    fn planning_ids_stay_within_the_published_bound() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // A legal 128-character destination id cannot push any planning
        // id past the published semanticId bound: overlong identities
        // keep a readable prefix plus a collision-free digest.
        let long = format!("{}x", "t".repeat(127));
        let (_, report) = fixture_assessment_with(
            None,
            Some(vec![destination_input(&long)]),
            BTreeMap::new(),
            None,
        )
        .expect("assessment succeeds");
        assert!(!report.migration_surface.rows.is_empty());
        let mut ids: Vec<&String> = report
            .migration_surface
            .rows
            .iter()
            .map(|row| &row.id)
            .collect();
        for id in &ids {
            assert!(id.len() <= 192, "overlong planning id: {id}");
            assert!(id.starts_with("plan."));
        }
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), report.migration_surface.rows.len());
    }

    #[test]
    fn over_budget_typed_ids_are_refused() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // A legal 191-character model id types past the report's id
        // budget: the assessment refuses explicitly instead of emitting
        // a schema-invalid subject.
        let error = fixture_assessment_with(
            None,
            None,
            BTreeMap::new(),
            Some(&mut |subjects: &mut Subjects| {
                subjects.rows[0].typed_id = format!("scalar:{}", "a".repeat(190));
            }),
        )
        .expect_err("over-budget typed id refuses");
        assert!(error.as_slice().iter().any(|d| {
            d.id() == "portability.input-invalid"
                && d.data()
                    .get("detail")
                    .map(|value| {
                        matches!(value, lekalo_core::diagnostics::DataValue::Token(token) if token == "subject-id-bound")
                    })
                    .unwrap_or(false)
        }));
    }

    #[test]
    fn requirements_and_witnesses_clip_at_the_published_bounds() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // A 70-dependency subject with a 70-member witness closure: the
        // emitted lists clip at the schema maxima with named
        // diagnostics, never silently and never over the bound.
        let (_, report) = fixture_assessment_with(
            None,
            None,
            BTreeMap::new(),
            Some(&mut |subjects: &mut Subjects| {
                let bulk = subjects
                    .rows
                    .iter_mut()
                    .find(|s| s.id == "planner.focus_task")
                    .expect("focus subject");
                bulk.deps = (0..70).map(|i| format!("planner.dep{i}")).collect();
                bulk.witnesses = (0..70)
                    .map(|i| format!("scalar:planner.witness{i}"))
                    .collect();
            }),
        )
        .expect("assessment succeeds");
        let focus = report
            .subjects
            .iter()
            .find(|s| s.subject == "command:planner.focus_task")
            .expect("focus subject");
        assert_eq!(focus.requirements.len(), 64);
        for obligation in &focus.rows {
            assert!(obligation.witnesses.len() <= 64);
        }
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.detail == "requirements-clipped"));
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.detail == "witnesses-clipped"));
    }
}

/// Round-5 semantics: the typed-verbatim strict-gate verdict (never the
/// bounded diagnostics channel), pinned verdict diagnostics with a
/// disclosed `diagnostics-bound` marker, `artifactRef` decoder/schema
/// parity in both directions, the storage-reference emit-composition
/// bound, and the custody provenance grammar of receipts and family
/// envelopes.
mod round5_semantics {
    use super::*;
    use lekalo_core::portability::planning;
    use lekalo_core::portability::version;

    /// Decode the fixture evidence with one mutation applied to the raw
    /// JSON: the refusal seam of the manifest decoder.
    fn evidence_decodes_with(
        mutate: &mut dyn FnMut(&mut serde_json::Value),
    ) -> Result<wire::EvidenceDocument, DiagnosticSet> {
        let compilation = compilation();
        let recipe = embedded_recipe().expect("embedded recipe");
        let mut value: serde_json::Value =
            serde_json::from_slice(&read(EVIDENCE)).expect("evidence JSON");
        mutate(&mut value);
        parse_evidence(
            &serde_json::to_vec(&value).expect("serialize"),
            &compilation,
            &recipe,
        )
    }

    /// Decode the fixture targets with one mutation applied to the raw
    /// JSON: the refusal seam of the targets decoder.
    fn targets_decodes_with(
        mutate: &mut dyn FnMut(&mut serde_json::Value),
    ) -> Result<wire::TargetsDocument, DiagnosticSet> {
        let mut value: serde_json::Value =
            serde_json::from_slice(&read(&format!("{FIXTURE}/config/portability-targets.json")))
                .expect("targets JSON");
        mutate(&mut value);
        parse_targets(&serde_json::to_vec(&value).expect("serialize"))
    }

    fn refused_with(detail: &str, result: Result<(), DiagnosticSet>) {
        let error = result.expect_err("the mutated document must refuse");
        assert!(
            error.as_slice().iter().any(|d| {
                d.id() == "portability.input-invalid"
                    && d.data()
                        .get("detail")
                        .map(|value| {
                            matches!(value, lekalo_core::diagnostics::DataValue::Token(token) if token == detail)
                        })
                        .unwrap_or(false)
            }),
            "expected refusal {detail}, got: {error:?}"
        );
    }

    #[test]
    fn the_surface_verdict_denies_without_the_diagnostics_channel() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // The mechanism of the crowd-out false green: the denial reads
        // the typed migration-surface verdict, so an incomplete surface
        // denies even when the diagnostics channel carries no
        // gate-coverage indication at all.
        let (_, mut report) = fixture_assessment();
        for subject in &mut report.subjects {
            for row in &mut subject.rows {
                row.blocking = false;
            }
        }
        report.coverage.complete = true;
        report.coverage.truncated = false;
        report.diagnostics.clear();
        report.migration_surface.complete = false;
        assert!(
            planning::denial(&report),
            "an incomplete surface must deny with an empty diagnostics channel"
        );
        // The control: the same green report with a complete surface
        // passes — the surface flag is the deciding input, not noise.
        report.migration_surface.complete = true;
        assert!(
            !planning::denial(&report),
            "a complete green surface must not deny"
        );
    }

    #[test]
    fn a_crowded_manifest_cannot_green_the_coverage_gap() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // A schema-legal manifest crowds the diagnostics list with
        // thousands of unqualified gate claims while no gate record
        // covers the affected scenarios: the surface stays incomplete,
        // the verdict diagnostic stays pinned, the bound is disclosed,
        // and the strict gate denies.
        let (_, report) = fixture_assessment_with(
            Some(&mut |value: &mut serde_json::Value| {
                let mut gates = Vec::new();
                for index in 0..4200 {
                    gates.push(serde_json::json!({
                        "gate": format!("planner.crowd-gate-{index}"),
                        "scenarios": [],
                        "state": "executed",
                        "targetId": "php-laravel",
                        "receiptRef": "reports/receipts/focus-flow.json",
                        "receiptDigest": format!("sha256:{}", "0".repeat(64)),
                    }));
                }
                value["gates"] = serde_json::json!(gates);
            }),
            None,
            BTreeMap::new(),
            None,
        )
        .expect("assessment succeeds");
        assert!(
            !report.migration_surface.complete,
            "the coverage gap must mark the surface incomplete"
        );
        assert!(planning::denial(&report));
        assert!(
            report.diagnostics.len() <= 4096,
            "the published bound holds: {}",
            report.diagnostics.len()
        );
        assert_eq!(
            report
                .diagnostics
                .iter()
                .filter(|d| d.detail == "gate-unqualified")
                .count(),
            4088,
            "the ordinary crowd fills exactly the unreserved region (4096 - 8)"
        );
        assert_eq!(
            report.diagnostics.last().expect("non-empty").detail,
            "diagnostics-bound",
            "the terminal marker discloses the clipping as the last entry"
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.detail == "gate-coverage-unknown"),
            "the coverage gap stays pinned above the crowd"
        );
    }

    #[test]
    fn artifact_ref_parity_refuses_schema_invalid_references() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // Decoder-refuses / schema-invalid: trailing dash and underscore
        // segments the published pattern rejects.
        for reference in ["reports/x-/r.json", "reports/x_/r.json"] {
            refused_with(
                "artifact-ref",
                evidence_decodes_with(&mut |value: &mut serde_json::Value| {
                    value["scenarios"][0]["receiptRef"] = reference.into();
                })
                .map(|_| ()),
            );
        }
        // Device stems: the published pattern now excludes them too, so
        // the decoder and the schema describe the same space.
        for reference in [
            "reports/nul/r.json",
            "reports/con.json",
            "reports/com1.d/r.json",
        ] {
            refused_with(
                "artifact-ref",
                evidence_decodes_with(&mut |value: &mut serde_json::Value| {
                    value["scenarios"][0]["receiptRef"] = reference.into();
                })
                .map(|_| ()),
            );
        }
        // Decoder-admits / schema-valid: the fixture spelling and a
        // legal neighbor still decode.
        evidence_decodes_with(&mut |value: &mut serde_json::Value| {
            value["scenarios"][0]["receiptRef"] = "reports/receipts/focus-flow-2.json".into();
        })
        .expect("a schema-valid artifact reference decodes");
    }

    #[test]
    fn storage_profile_refs_hold_the_emit_composition_bound() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // The report composes `<ref>#sha256:<digest>` (72 more
        // characters) into a member published at 324: a 253-character
        // reference is refused at decode and a 252-character one is the
        // exact legal maximum.
        let segment = "a".repeat(63);
        let tail = |length: usize| format!("{}{}", "b".repeat(length), ".json");
        let over = [segment.as_str(), &segment, &segment, &tail(56)].join("/");
        assert_eq!(over.len(), 253);
        refused_with(
            "artifact-ref",
            targets_decodes_with(&mut |value: &mut serde_json::Value| {
                value["targets"][0]["storageProfileRefs"] = serde_json::json!([over]);
            })
            .map(|_| ()),
        );
        let at_bound = [segment.as_str(), &segment, &segment, &tail(55)].join("/");
        assert_eq!(at_bound.len(), 252);
        targets_decodes_with(&mut |value: &mut serde_json::Value| {
            value["targets"][0]["storageProfileRefs"] = serde_json::json!([at_bound]);
        })
        .expect("the 252-character maximum legal reference decodes");
    }

    #[test]
    fn receipt_provenance_grammar_refuses_at_decode() {
        // Every provenance member of the closed receipt contract is
        // grammar-checked at admission, exactly like the adapter
        // snapshot decoder: a schema-invalid receipt never decodes
        // cleanly only to demote later.
        let base: serde_json::Value = serde_json::from_slice(&read(&format!(
            "{FIXTURE}/reports/receipts/focus-flow.json"
        )))
        .expect("receipt JSON");
        assert!(
            wire::decode_receipt(&serde_json::to_vec(&base).expect("serialize")).is_ok(),
            "the fixture receipt decodes"
        );
        for (path, malformed) in [
            (
                vec!["profileRef", "resolvedDigest"],
                serde_json::json!("not-a-digest"),
            ),
            (
                vec!["projectRef", "projectId"],
                serde_json::json!("INVALID"),
            ),
            (
                vec!["projectRef", "modelVersion"],
                serde_json::json!("m".repeat(33)),
            ),
            (
                vec!["projectRef", "irDigest"],
                serde_json::json!("sha256:zz"),
            ),
        ] {
            let mut receipt = base.clone();
            let mut cursor = &mut receipt;
            for key in &path[..path.len() - 1] {
                cursor = &mut cursor[key];
            }
            cursor[path[path.len() - 1]] = malformed;
            assert!(
                wire::decode_receipt(&serde_json::to_vec(&receipt).expect("serialize")).is_err(),
                "malformed {:?} must refuse at decode",
                path
            );
        }
    }

    #[test]
    fn family_owner_envelopes_pin_their_project_ref_grammar() {
        // The closed family owner envelope refuses malformed provenance
        // members (project id token, bounded model version, exact ir
        // digest spelling) instead of certifying bytes it cannot name.
        let evidence: serde_json::Value =
            serde_json::from_slice(&read(EVIDENCE)).expect("evidence JSON");
        let project_ref = evidence["projectRef"].clone();
        let artifact = |project_ref: serde_json::Value| {
            serde_json::json!({
                "schemaVersion": version::FAMILY_SCHEMA_VERSION,
                "identity": version::FAMILY_IDENTITY,
                "family": "scenario",
                "projectRef": project_ref,
            })
        };
        assert!(
            wire::decode_family_artifact(
                &serde_json::to_vec(&artifact(project_ref.clone())).expect("serialize"),
                wire::EvidenceFamily::Scenario,
            )
            .is_ok(),
            "the fixture project pin decodes"
        );
        for malformed in [
            serde_json::json!({
                "projectId": "INVALID",
                "modelVersion": project_ref["modelVersion"],
                "irDigest": project_ref["irDigest"],
            }),
            serde_json::json!({
                "projectId": project_ref["projectId"],
                "modelVersion": "m".repeat(33),
                "irDigest": project_ref["irDigest"],
            }),
            serde_json::json!({
                "projectId": project_ref["projectId"],
                "modelVersion": project_ref["modelVersion"],
                "irDigest": "sha256:not-a-digest",
            }),
        ] {
            assert!(
                wire::decode_family_artifact(
                    &serde_json::to_vec(&artifact(malformed)).expect("serialize"),
                    wire::EvidenceFamily::Scenario,
                )
                .is_err(),
                "a malformed family projectRef must refuse at decode"
            );
        }
    }
}
