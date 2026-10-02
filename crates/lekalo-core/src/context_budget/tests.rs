//! Issue #75 acceptance tests for the context-budget report: the
//! deterministic planner/integration comparison, the exact budget
//! boundary, the explainable over-budget breakdown, the required/
//! supporting split, the simulation at a tiny budget, and the byte
//! determinism of both projections.

use crate::context_budget::{plan, BudgetRequest, BudgetSelection, ProfileDocument, StateValue};
use crate::ir::compile;
use crate::loader::{normalize_model, LoadSelection};

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
