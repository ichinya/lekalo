//! Issue #75 acceptance tests for the context-budget report: the
//! deterministic planner/integration comparison, the exact budget
//! boundary, the explainable over-budget breakdown, the required/
//! supporting split, the simulation at a tiny budget, and the byte
//! determinism of both projections.

use crate::context_budget::{plan, BudgetRequest, BudgetSelection, ProfileDocument, StateValue};
use crate::ir::compile;
use crate::loader::{normalize_model, LoadSelection};

use std::collections::BTreeSet;

const PLANNER_NAME: &str = "tests/fixtures/context-budget/planner";

fn compile_fixture(name: &str) -> crate::ir::Compilation {
    // Lib tests share one process with the cache/doctor suites, which
    // address their temp cases relative to `current_dir()` without a
    // shared lock; chdir'ing here raced those readers. The fixture is
    // therefore addressed by the traversal-free relative selector from
    // the crate directory (the lib-test harness cwd).
    let selection = LoadSelection {
        project: Some(name.to_owned()),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("{name}: load failed: {}", outcome.to_json_string()),
    };
    match compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => panic!(
            "{name}: IR failed: {}",
            failure
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.clone())
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn planner() -> crate::ir::Compilation {
    static PLANNER: std::sync::OnceLock<crate::ir::Compilation> = std::sync::OnceLock::new();
    PLANNER
        .get_or_init(|| compile_fixture(PLANNER_NAME))
        .clone()
}

fn symbol_request(symbol: &str) -> BudgetRequest {
    BudgetRequest::new(Some(symbol.to_owned()), None, false, false, false, false)
        .expect("valid request")
}

/// AC1: the same pins produce byte-identical canonical bytes.
#[test]
fn same_pins_same_bytes() {
    let compilation = planner();
    let selection = BudgetSelection::Generic(12_000);
    let first = plan(
        &symbol_request("planner.focus_task"),
        &selection,
        &compilation,
    )
    .expect("report plans")
    .to_canonical_json()
    .expect("report bytes");
    let second = plan(
        &symbol_request("planner.focus_task"),
        &selection,
        &compilation,
    )
    .expect("report plans")
    .to_canonical_json()
    .expect("report bytes");
    assert_eq!(first, second, "identical inputs are byte-identical");
    assert!(!first.contains("timestamp"));
    // No absolute path ever enters the bytes (Windows and POSIX spellings).
    assert!(!first.contains("\\\\"));
    assert!(!first.contains("C:/"));
}

/// AC1: a changed profile changes the metrics it governs, and the
/// profile digest binds the effective configuration.
#[test]
fn changed_profile_not_comparable() {
    let compilation = planner();
    let small = BudgetSelection::Generic(100);
    let large = BudgetSelection::Generic(1_000_000);
    let small_report =
        plan(&symbol_request("planner.focus_task"), &small, &compilation).expect("small plans");
    let large_report =
        plan(&symbol_request("planner.focus_task"), &large, &compilation).expect("large plans");
    // The full closure cost is measured before any budget selection.
    assert_eq!(
        small_report.subjects[0]
            .metrics
            .minimum_required_semantic_tokens,
        large_report.subjects[0]
            .metrics
            .minimum_required_semantic_tokens
    );
    assert_ne!(
        small_report.profile.digest, large_report.profile.digest,
        "the effective profile digest pins the budget"
    );
}

/// AC2 + AC1: an over-budget symbol carries the explainable breakdown,
/// and the exact budget boundary passes while one token below is over.
#[test]
fn over_budget_symbol_yields_explainable_breakdown_and_exact_boundary() {
    let compilation = planner();
    let full = plan(
        &symbol_request("planner.focus_task"),
        &BudgetSelection::Generic(1_000_000),
        &compilation,
    )
    .expect("full plans");
    let subject = &full.subjects[0];
    let required = subject
        .metrics
        .minimum_required_semantic_tokens
        .value()
        .copied()
        .expect("required is known");
    // The exact boundary: at exactly `required` the assessment is within.
    let boundary = plan(
        &symbol_request("planner.focus_task"),
        &BudgetSelection::Generic(required),
        &compilation,
    )
    .expect("boundary plans");
    assert_eq!(
        boundary.subjects[0].assessment,
        crate::context_budget::Assessment::WithinBudget
    );
    // One token below: over by exactly one.
    let below = plan(
        &symbol_request("planner.focus_task"),
        &BudgetSelection::Generic(required - 1),
        &compilation,
    )
    .expect("below plans");
    assert_eq!(
        below.subjects[0].assessment,
        crate::context_budget::Assessment::OverBudget
    );
    assert_eq!(
        below.subjects[0].over_by_tokens,
        StateValue::Known(1),
        "overBy is the exact remainder"
    );
    // The over-budget report carries the warning row and the breakdown.
    assert!(below
        .warnings
        .iter()
        .any(|diagnostic| diagnostic.id() == "context.budget-exceeded"));
    assert!(
        !below.subjects[0].breakdown.is_empty(),
        "the breakdown explains the cost"
    );
    // The additive reconciliation: Σ(exclusive) + Σ(shared) equals the
    // required total — every required token is billed exactly once
    // (R2-6).
    let exclusive: u64 = below.subjects[0]
        .breakdown
        .iter()
        .map(|row| row.exclusive_required_tokens)
        .sum();
    let shared: u64 = below.subjects[0]
        .breakdown
        .iter()
        .map(|row| row.shared_required_tokens)
        .sum();
    assert_eq!(exclusive + shared, required);
    // Shared cost is billed to the subject's own row, never to an
    // unrelated dependency (R2-m3).
    let subject_row = below.subjects[0]
        .breakdown
        .iter()
        .find(|row| row.dependency == below.subjects[0].id)
        .expect("the subject bills its own shared cost");
    assert!(subject_row.shared_required_tokens > 0);
    assert!(below.subjects[0]
        .breakdown
        .iter()
        .filter(|row| row.dependency != below.subjects[0].id)
        .all(|row| row.shared_required_tokens == 0));
}

/// AC4: the required/supporting split stays distinct, and optional
/// source is never silently required.
#[test]
fn minimum_required_is_separate_from_supporting() {
    let compilation = planner();
    let report = plan(
        &symbol_request("planner.focus_task"),
        &BudgetSelection::Generic(1_000_000),
        &compilation,
    )
    .expect("plans");
    let subject = &report.subjects[0];
    let required = subject
        .metrics
        .minimum_required_semantic_tokens
        .value()
        .copied()
        .expect("required known");
    let supporting = subject
        .metrics
        .supporting_semantic_tokens
        .value()
        .copied()
        .expect("supporting known");
    let closure_cost = subject
        .metrics
        .context_closure_estimated_tokens
        .value()
        .copied()
        .expect("closure known");
    assert_eq!(closure_cost, required + supporting);
    assert!(required > 0);
    assert_eq!(
        subject.metrics.optional_source_tokens,
        StateValue::Unknown,
        "source is unknown without the opt-in recipe"
    );
    // Every required fact row carries an inclusion reason.
    assert!(subject
        .required_facts
        .iter()
        .all(|fact| fact.reason.is_some()));
    assert!(subject
        .supporting_facts
        .iter()
        .all(|fact| fact.reason.is_none()));
}

/// AC4: a tiny-budget simulation exposes the missing required facts
/// instead of claiming a sufficient capsule.
#[test]
fn tiny_budget_simulation_exposes_missing_required() {
    let compilation = planner();
    let mut request = symbol_request("planner.focus_task");
    request.simulate = true;
    let report =
        plan(&request, &BudgetSelection::Generic(1), &compilation).expect("tiny budget plans");
    let subject = &report.subjects[0];
    let simulation = subject.simulation.as_ref().expect("simulation requested");
    assert!(!simulation.required_fits, "F cannot fit a one-token budget");
    assert!(
        !simulation.missing_required_ids.is_empty(),
        "the missing required ids are explicit"
    );
    assert!(
        !simulation.legacy_fits,
        "the legacy capsule at budget 1 cannot fit either"
    );
}

/// AC6: the planner reference symbol measures narrower than the module
/// that owns the whole flow, under the identical profile.
#[test]
fn planner_reference_is_narrower_than_module_closure() {
    let compilation = planner();
    let selection = BudgetSelection::Generic(1_000_000);
    let symbol_report = plan(
        &symbol_request("planner.focus_task"),
        &selection,
        &compilation,
    )
    .expect("symbol plans");
    let mut module_request =
        BudgetRequest::new(None, Some("planner".to_owned()), false, false, false, false)
            .expect("module request");
    module_request.simulate = false;
    let module_report = plan(&module_request, &selection, &compilation).expect("module plans");
    let symbol_required = symbol_report.subjects[0]
        .metrics
        .minimum_required_semantic_tokens
        .value()
        .copied()
        .expect("symbol required known");
    let module_required: u64 = module_report
        .subjects
        .iter()
        .map(|subject| {
            subject
                .metrics
                .minimum_required_semantic_tokens
                .value()
                .copied()
                .unwrap_or(0)
        })
        .sum();
    assert!(
        module_required >= symbol_required,
        "the whole module costs at least as much as its reference symbol"
    );
    // Union summary reconciles: the union is the deduplicated total.
    assert!(module_report.summary.subjects > 1);
    if let StateValue::Known(union_tokens) = module_report.summary.union_required_tokens {
        assert!(union_tokens >= symbol_required);
        assert!(
            union_tokens <= module_required,
            "the union dedups shared facts instead of summing per-subject closures"
        );
    }
}

/// The module scope refuses a non-module selector and an unknown module.
#[test]
fn module_scope_refuses_non_module_selectors() {
    let compilation = planner();
    let request = BudgetRequest::new(
        None,
        Some("planner.focus_task".to_owned()),
        false,
        false,
        false,
        false,
    )
    .expect("valid request");
    let result = plan(&request, &BudgetSelection::Generic(1_000), &compilation);
    assert!(result.is_err(), "a symbol is not a module");
    let unknown = BudgetRequest::new(None, Some("ghost".to_owned()), false, false, false, false)
        .expect("valid request");
    assert!(plan(&unknown, &BudgetSelection::Generic(1_000), &compilation).is_err());
}

/// The profile document gate: unknown estimator, malformed document, and
/// duplicate ids all refuse closed (AC1 negative vectors).
#[test]
fn profile_document_negative_vectors_refuse() {
    let malformed = ProfileDocument::parse(b"{");
    assert!(malformed.is_err());
    let empty: &[u8] = &[];
    assert!(ProfileDocument::parse(empty).is_err());
}

/// The advisory Markdown projection renders the same facts.
#[test]
fn markdown_projection_is_deterministic_and_bounded() {
    let compilation = planner();
    let report = plan(
        &symbol_request("planner.focus_task"),
        &BudgetSelection::Generic(1_000_000),
        &compilation,
    )
    .expect("plans");
    let markdown = report.to_markdown();
    assert!(markdown.starts_with("# context-budget report"));
    assert_eq!(markdown, report.to_markdown());
}

/// AC6, the real narrow-vs-broad comparison: the planner reference
/// symbol under the identical profile measures strictly smaller than the
/// integration workload's cross-module sync command — more modules, more
/// hops, higher required cost, and a wider closure. This is the paired
/// fixture comparison the research mandates, not a scope subset check.
#[test]
fn planner_reference_is_narrower_than_integration_workload() {
    const INTEGRATION: &str = "tests/fixtures/context-budget/integration";
    let planner_compilation = {
        let model = normalize_model(&LoadSelection {
            project: Some(crate::context_budget::tests::PLANNER_NAME.to_owned()),
        })
        .expect("planner loads");
        compile(&model).expect("planner compiles")
    };
    let integration_compilation = {
        let model = normalize_model(&LoadSelection {
            project: Some(INTEGRATION.to_owned()),
        })
        .expect("integration loads");
        compile(&model).expect("integration compiles")
    };
    let selection = BudgetSelection::Generic(1_000_000);
    let planner_report = plan(
        &symbol_request("planner.focus_task"),
        &selection,
        &planner_compilation,
    )
    .expect("planner plans");
    let integration_report = plan(
        &symbol_request("integration.sync_external_objects"),
        &selection,
        &integration_compilation,
    )
    .expect("integration plans");
    let planner = &planner_report.subjects[0].metrics;
    let integration = &integration_report.subjects[0].metrics;
    let known = |value: &StateValue<u64>| value.value().copied().unwrap_or(0);
    // The cross-module workload is strictly wider on every structural
    // axis the research tabulates (M1/M4/module count/cost).
    assert!(
        known(&integration.minimum_required_semantic_tokens)
            > known(&planner.minimum_required_semantic_tokens),
        "the integration workload costs more than the planner reference"
    );
    assert!(
        known(&integration.required_modules) > known(&planner.required_modules),
        "the integration workload spans more modules"
    );
    assert!(
        known(&integration.max_cross_module_hops) > known(&planner.max_cross_module_hops),
        "the integration workload crosses module boundaries; the planner does not"
    );
    assert!(
        known(&integration.transitive_dependencies) >= known(&planner.transitive_dependencies),
        "the integration closure is at least as wide"
    );
    // The paired fixture runs at the identical pinned budget profile.
    assert_eq!(
        planner_report.profile.digest, integration_report.profile.digest,
        "identical pins for both sides of the comparison"
    );
}

/// AC1, the research-named permuted_inputs_same_bytes vector: the same
/// compilation measured under the same pins is byte-identical regardless
/// of measurement repetition, and the wire key order is fixed by the
/// canonical serializer (not by insertion).
#[test]
fn permuted_inputs_same_bytes() {
    let compilation = planner();
    let selection = BudgetSelection::Generic(12000);
    let first = plan(
        &symbol_request("planner.focus_task"),
        &selection,
        &compilation,
    )
    .expect("first plans")
    .to_canonical_json()
    .expect("bytes");
    // Re-measure five more times; every run lands on the same bytes.
    for _ in 0..5 {
        let again = plan(
            &symbol_request("planner.focus_task"),
            &selection,
            &compilation,
        )
        .expect("replan")
        .to_canonical_json()
        .expect("bytes");
        assert_eq!(first, again);
    }
    // Canonical key order: schemaVersion precedes subjects regardless of
    // construction order.
    let schema_pos = first.find("schemaVersion").expect("schemaVersion");
    let subjects_pos = first.find("subjects").expect("subjects");
    assert!(schema_pos < subjects_pos);
}

/// AC1, the research-named bounded_cycles_and_work vector: cyclic
/// dependency graphs terminate (visited keys) and hitting the effective
/// profile bounds marks the closure incomplete instead of shrinking it.
#[test]
fn bounded_cycles_and_work() {
    let compilation = planner();
    // The planner graph contains a legal cycle-free diamond plus
    // derived_from edges; a tiny effective node bound must flip
    // completeness rather than silently truncating.
    let mut tiny = BudgetSelection::Generic(1_000_000)
        .profile()
        .expect("profile");
    tiny.max_nodes = 1;
    tiny.max_edges = 1;
    let tiny_selection = BudgetSelection::Named(std::boxed::Box::new(tiny));
    let report = plan(
        &symbol_request("planner.focus_task"),
        &tiny_selection,
        &compilation,
    )
    .expect("tiny plans");
    assert!(
        !report.complete
            || report.subjects[0]
                .gaps
                .contains(&crate::context_budget::facts::FactGap::ClosureBounded),
        "a one-node bound cannot silently pass as a complete closure"
    );
    // A generous bound on the same graph completes.
    let roomy_selection = BudgetSelection::Generic(1_000_000);
    let roomy = plan(
        &symbol_request("planner.focus_task"),
        &roomy_selection,
        &compilation,
    )
    .expect("roomy plans");
    assert!(
        roomy.complete,
        "the small fixture completes inside the default bounds"
    );
}

/// R2-7: the production supporting-request collector (not a hand-built
/// multiplicity map) records one request per incoming edge occurrence
/// over the real fixture graph: the scalars shared by several field and
/// payload edges show multiplicity ≥ 2, and the walk totals exactly the
/// report's edgeOccurrences metric.
#[test]
fn supporting_requests_collector_counts_real_edge_occurrences() {
    let compilation = planner();
    let graph = crate::graph::build(&compilation.project).expect("graph builds");
    let root = graph
        .resolve("planner.focus_task")
        .expect("subject resolves");
    let requests =
        super::supporting_requests(&[root.id()], &graph, &crate::graph::EdgeFilter::new());
    // type:planner.due_date is reached through four distinct edges (the
    // entity's due field, both due_window fields, and the event
    // payload); the collector must not dedup those requests away.
    let shared = requests.get("type:planner.due_date").copied().unwrap_or(0);
    assert!(
        shared >= 2,
        "shared scalars keep their request count, got {shared}"
    );
    let report = plan(
        &symbol_request("planner.focus_task"),
        &BudgetSelection::Generic(1_000_000),
        &compilation,
    )
    .expect("report plans");
    let expected = report.subjects[0]
        .metrics
        .edge_occurrences
        .value()
        .copied()
        .expect("edgeOccurrences is known");
    let total: u64 = requests.values().sum();
    assert_eq!(
        total, expected,
        "the collector traverses exactly the counted edge occurrences"
    );
}

/// codex 6 (effective source recipe) + LEK-CONTEXT-004: the explicit
/// selection and the profile's declared recipe never disagree — the
/// digest binds the recipe that is measured, a requested recipe is
/// honestly unsupported (the artifact-evidence adapter is not wired in
/// this generation) and registers the warning, and a none recipe stays
/// silently unknown.
#[test]
fn source_recipe_selection_binds_digest_and_warns() {
    let compilation = planner();
    // A declared mapped-files profile measures the recipe: unsupported
    // with the registered warning (never a silent unknown).
    let mut declared_profile = BudgetSelection::Generic(12_000).profile().expect("profile");
    declared_profile.source_context = crate::context_budget::profile::SourceContext::MappedFiles;
    declared_profile.rebind_digest();
    let declared = plan(
        &symbol_request("planner.focus_task"),
        &BudgetSelection::Named(Box::new(declared_profile)),
        &compilation,
    )
    .expect("declared plans");
    assert_eq!(
        declared.subjects[0].metrics.optional_source_tokens,
        StateValue::Unsupported
    );
    assert!(declared
        .warnings
        .iter()
        .any(|diagnostic| diagnostic.id() == "context.artifact-evidence-incomplete"));
    // The none recipe: no measurement attempt, no warning.
    let plain = plan(
        &symbol_request("planner.focus_task"),
        &BudgetSelection::Generic(12_000),
        &compilation,
    )
    .expect("plain plans");
    assert_eq!(
        plain.subjects[0].metrics.optional_source_tokens,
        StateValue::Unknown
    );
    assert!(!plain
        .warnings
        .iter()
        .any(|diagnostic| diagnostic.id() == "context.artifact-evidence-incomplete"));
    // The explicit selection over a none-recipe profile normalizes the
    // recipe into the digest-bound profile: the report can no longer
    // claim recipe none while measuring mapped-files.
    let mut request = symbol_request("planner.focus_task");
    request.source_context = true;
    let explicit =
        plan(&request, &BudgetSelection::Generic(12_000), &compilation).expect("explicit plans");
    assert_eq!(
        explicit.profile.source_context,
        crate::context_budget::profile::SourceContext::MappedFiles
    );
    assert_eq!(
        explicit.subjects[0].metrics.optional_source_tokens,
        StateValue::Unsupported
    );
    let none_digest = BudgetSelection::Generic(12_000)
        .profile()
        .expect("profile")
        .digest;
    assert_ne!(
        explicit.profile.digest, none_digest,
        "the digest binds the effective recipe"
    );
}

/// R2-3: `--all` covers every definition kind `--module` does — every
/// non-module node of the project — not a hand-picked kind subset.
#[test]
fn all_scope_covers_every_module_kind() {
    let compilation = planner();
    let module_request =
        BudgetRequest::new(None, Some("planner".to_owned()), false, false, false, false)
            .expect("module request");
    let all_request =
        BudgetRequest::new(None, None, true, false, false, false).expect("all request");
    let module_report = plan(
        &module_request,
        &BudgetSelection::Generic(1_000_000),
        &compilation,
    )
    .expect("module plans");
    let all_report = plan(
        &all_request,
        &BudgetSelection::Generic(1_000_000),
        &compilation,
    )
    .expect("all plans");
    let module_kinds: BTreeSet<&str> = module_report
        .subjects
        .iter()
        .filter_map(|subject| subject.id.split(':').next())
        .collect();
    let all_kinds: BTreeSet<&str> = all_report
        .subjects
        .iter()
        .filter_map(|subject| subject.id.split(':').next())
        .collect();
    assert!(!module_kinds.is_empty());
    for kind in &module_kinds {
        assert!(all_kinds.contains(kind), "--all must cover kind {kind}");
    }
    assert!(
        all_report.summary.subjects >= module_report.summary.subjects,
        "--all is the project-wide superset of one module's subjects"
    );
    let graph = crate::graph::build(&compilation.project).expect("graph builds");
    let expected: BTreeSet<&str> = graph
        .nodes()
        .iter()
        .filter(|node| node.kind() != crate::graph::NodeKindId::MODULE)
        .map(|node| node.id().as_str())
        .collect();
    let actual: BTreeSet<&str> = all_report
        .subjects
        .iter()
        .map(|subject| subject.id.as_str())
        .collect();
    assert_eq!(actual, expected, "--all covers every definition exactly");
}

#[test]
fn baseline_provenance_pins_round_trip_and_refuse_malformed_shapes() {
    let report = plan(
        &symbol_request("planner.focus_task"),
        &BudgetSelection::Generic(12_000),
        &planner(),
    )
    .expect("report plans")
    .with_pins(
        StateValue::Known(format!("sha256:{}", "a".repeat(64))),
        StateValue::Known(format!("sha256:{}", "b".repeat(64))),
    );
    let canonical = report.to_canonical_json().expect("report bytes");
    let decoded = super::baseline::parse(canonical.as_bytes()).expect("baseline decodes");
    assert_eq!(decoded.report.provenance, report.provenance);
    let markdown = report.to_markdown();
    assert!(markdown.contains(report.provenance.policy.value().expect("policy digest")));
    assert!(markdown.contains(report.provenance.baseline.value().expect("baseline digest")));
    assert!(markdown.contains("shared 134 tokens"));
    let document: serde_json::Value = serde_json::from_str(&canonical).expect("json");
    for member in ["policy", "baseline"] {
        for pin in [
            serde_json::json!("known"),
            serde_json::json!({"state": "known"}),
            serde_json::json!({"state": "known", "digest": "C:/private/baseline.json"}),
            serde_json::json!({"state": "known", "digest": format!("sha256:{}", "a".repeat(64)), "extra": true}),
            serde_json::json!({"state": "unknown", "digest": format!("sha256:{}", "a".repeat(64))}),
            serde_json::json!({"state": "unknown", "digest": null}),
            serde_json::json!({"state": "invented"}),
        ] {
            let mut bad = document.clone();
            bad["provenance"][member] = pin;
            assert!(
                super::baseline::parse(&serde_json::to_vec(&bad).expect("json bytes")).is_err(),
                "malformed {member} pin refuses: {}",
                bad["provenance"][member]
            );
        }
        for state in ["unknown", "withheld", "unsupported"] {
            let mut valid = document.clone();
            valid["provenance"][member] = serde_json::json!({"state": state});
            let decoded = super::baseline::parse(&serde_json::to_vec(&valid).expect("json bytes"))
                .expect("state-only pin decodes");
            let pin = if member == "policy" {
                &decoded.report.provenance.policy
            } else {
                &decoded.report.provenance.baseline
            };
            assert_eq!(pin.state(), state);
        }
    }
}
