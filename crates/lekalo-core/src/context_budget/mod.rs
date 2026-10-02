//! Issue #75: the context-budget and local-understandability report.
//!
//! One report measures how much deterministic context one symbol,
//! module, or project needs for an AI to work under a bounded content
//! budget: the full dependency closure before any budget selection, the
//! required semantic fact set F separated from supporting facts O and
//! optional source S, cross-module hops, unresolved/ambiguous edges,
//! effect/policy/scenario counts, the largest required artifact,
//! duplicate supporting requests, the ownership ratio, and the
//! minimum-safe structural estimate. An advisory extraction-suggestion
//! pass and an opt-in capsule simulation ride the same ledger. Every
//! metric is a known value or an explicit non-known state; a reduced
//! completeness is always visible, never a silently smaller closure.
//!
//! Determinism: the same compilation, scope, profile, evidence, selector,
//! and metric version always produce byte-identical canonical JSON. No
//! timestamps, random ids, host paths, absolute paths, raw source text,
//! or locale formatting ever enter the wire. The report is advisory by
//! default: a mandatory budget policy is the only path to `denied`, and
//! it can be selected only by an explicit caller-supplied policy handle.

pub mod baseline;
pub mod closure;
pub mod compare;
pub mod diagnostic;
pub mod estimate;
pub mod facts;
pub mod metrics;
pub mod policy;
pub mod profile;
pub mod request;
#[cfg(test)]
mod tests;
pub mod value;
pub mod version;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::diagnostics::DiagnosticSet;
use crate::effects::EffectGraph;
use crate::graph::{build as graph_build, DependencyGraph, EdgeFilter, NodeId};
use crate::ir::Compilation;

pub use facts::{FactClass, InclusionReason};
pub use metrics::{Assessment, SubjectMetrics};
pub use profile::{Profile, ProfileDocument};
pub use request::{BudgetRequest, BudgetSelection, Scope};
pub use value::StateValue;

/// One dependency-breakdown row of an over-budget subject.
#[derive(Clone, Debug, PartialEq)]
pub struct BreakdownRow {
    /// The reachable dependency id (kind-qualified).
    pub dependency: String,
    /// The owning module of the dependency, when the graph has one.
    pub module: Option<String>,
    /// The minimum module-boundary hops to reach it.
    pub hops: Option<u64>,
    /// Required tokens attributed exclusively to this dependency.
    pub exclusive_required_tokens: u64,
    /// Required tokens shared with other dependencies (attributed once,
    /// to the canonically first owner).
    pub shared_required_tokens: u64,
}

/// One suggested extraction boundary (advisory, never executed).
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    /// The proposed boundary id (the dominant crossing module).
    pub boundary_id: String,
    /// The distinct dependency ids behind the boundary.
    pub cut_dependencies: Vec<String>,
    /// Required fact ids that must stay visible across the boundary.
    pub preserved_facts: Vec<String>,
    /// The hypothetical before-tokens (the current required sum).
    pub before_tokens: u64,
    /// The hypothetical after-tokens if the boundary were adopted.
    pub after_tokens: u64,
    /// Why the suggestion cannot be applied mechanically, when so.
    pub blocker: Option<String>,
}

/// The opt-in simulation block: the legacy capsule at the same budget
/// alongside the new required/supporting selection.
#[derive(Clone, Debug, PartialEq)]
pub struct Simulation {
    pub candidate_facts: u64,
    pub included_facts: u64,
    pub required_facts: u64,
    pub included_required_facts: u64,
    pub estimated_tokens: u64,
    pub required_fits: bool,
    pub all_candidates_fit: bool,
    /// The missing required fact ids when F does not fit.
    pub missing_required_ids: Vec<String>,
    /// The legacy capsule's own minimum-required total (its own set).
    pub legacy_minimum_required: u64,
    /// The legacy capsule's estimated total at this budget.
    pub legacy_estimated: u64,
    /// Whether the legacy capsule fit its whole candidate list.
    pub legacy_fits: bool,
}

/// One subject row of the report.
#[derive(Clone, Debug, PartialEq)]
pub struct SubjectReport {
    /// The kind-qualified subject id.
    pub id: String,
    /// The subject's owning module, when the graph has one.
    pub module: Option<String>,
    pub metrics: SubjectMetrics,
    pub assessment: Assessment,
    pub over_by_tokens: StateValue<u64>,
    /// The explainable dependency breakdown, canonical order, bounded.
    pub breakdown: Vec<BreakdownRow>,
    /// Whether the breakdown was truncated at the recorded row bound.
    pub breakdown_truncated: bool,
    /// The required/supporting ledger rows of this subject.
    pub required_facts: Vec<facts::LedgerFact>,
    pub supporting_facts: Vec<facts::LedgerFact>,
    pub gaps: Vec<facts::FactGap>,
    pub suggestions: Vec<Suggestion>,
    pub simulation: Option<Simulation>,
}

/// The summary over all subject rows.
#[derive(Clone, Debug, PartialEq)]
pub struct ReportSummary {
    pub subjects: u64,
    pub over_budget_subjects: u64,
    pub indeterminate_subjects: u64,
    /// The union required-token estimate over distinct required facts.
    pub union_required_tokens: StateValue<u64>,
}

/// The input provenance tuple of one report: the exact pins a consumer
/// needs to bind a report to what it measured (AC1/AC7). Digests bind
/// the accepted projections; a missing pin is an explicit unknown state,
/// never an absent field.
#[derive(Clone, Debug, PartialEq)]
pub struct Provenance {
    /// The exact source Model version.
    pub model_version: String,
    /// The sha256 of the canonical IR the accepted projections built from.
    pub ir_digest: String,
    /// The graph contract identity.
    pub graph_identity: String,
    /// The effect-graph contract identity.
    pub effect_identity: String,
    /// The policy document pin: digest + verdict when a mandatory policy
    /// was selected (an advisory run records the advisory mode).
    pub policy: StateValue<String>,
    /// The baseline file pin: the candidate comparison was computed.
    pub baseline: StateValue<String>,
}

/// One finished context-budget report: the normalized product both the
/// JSON and human projections render.
#[derive(Clone, Debug, PartialEq)]
pub struct BudgetReport {
    pub scope: Scope,
    pub provenance: Provenance,
    pub profile: Profile,
    pub subjects: Vec<SubjectReport>,
    pub summary: ReportSummary,
    pub complete: bool,
    /// The advisory diagnostics attached to the valid envelope.
    pub warnings: Vec<crate::diagnostics::Diagnostic>,
}

impl BudgetReport {
    /// Record the caller-selected policy and baseline pins into the
    /// provenance block (the CLI layer owns those file reads; core stays
    /// filesystem-free). The policy pin carries the policy digest.
    pub fn with_pins(
        mut self,
        policy_pin: StateValue<String>,
        baseline_pin: StateValue<String>,
    ) -> Self {
        self.provenance.policy = policy_pin;
        self.provenance.baseline = baseline_pin;
        self
    }

    /// The canonical JSON payload bytes (without the envelope wrapper
    /// and without a trailing newline).
    pub fn to_canonical_json(&self) -> Result<String, DiagnosticSet> {
        let bytes = wire::report_json(self);
        if bytes.len() > version::MAX_REPORT_BYTES {
            return Err(diagnostic::input_invalid("report-bytes-limit"));
        }
        Ok(bytes)
    }

    /// The deterministic Markdown document (the human projection).
    pub fn to_markdown(&self) -> String {
        wire::report_markdown(self)
    }
}

/// Plan one context-budget report over a compiled project.
///
/// Load and IR failures never reach this seam: callers compile through
/// the accepted surfaces first, and this entry builds the accepted graph
/// and effect projections itself. A fatal input (selector, budget, unknown
/// subject) is an `invalid`/`unsupported-version` diagnostic set; every
/// other outcome is a valid advisory report carrying its warnings.
pub fn plan(
    request: &BudgetRequest,
    selection: &BudgetSelection,
    compilation: &Compilation,
) -> Result<BudgetReport, DiagnosticSet> {
    let profile = selection.profile()?;
    let project = &compilation.project;
    let graph = graph_build(project)?;
    let effects = crate::effects::build(project)?;
    let context = facts::FactContext::new(project);

    // Resolve the subject set per scope.
    let subjects: Vec<(&NodeId, Option<String>)> = match &request.scope {
        Scope::Symbol(symbol) => {
            let node = graph
                .resolve(symbol)
                .ok_or_else(|| crate::graph::diagnostic::unknown_node_set(symbol))?;
            vec![(node.id(), node.module().map(str::to_owned))]
        }
        Scope::Module(module) => {
            let node = graph
                .resolve(module)
                .ok_or_else(|| crate::graph::diagnostic::unknown_node_set(module))?;
            if node.kind() != crate::graph::NodeKindId::MODULE {
                return Err(diagnostic::input_invalid_detail(
                    "not-a-module",
                    Some(module),
                ));
            }
            module_subjects(&graph, module)
        }
        Scope::All => {
            let mut found: Vec<(&NodeId, Option<String>)> = graph
                .nodes()
                .iter()
                .filter(|node| {
                    matches!(
                        node.kind(),
                        crate::graph::NodeKindId::OPERATION
                            | crate::graph::NodeKindId::ENTITY
                            | crate::graph::NodeKindId::TYPE
                            | crate::graph::NodeKindId::EVENT
                            | crate::graph::NodeKindId::EFFECT
                    )
                })
                .map(|node| (node.id(), node.module().map(str::to_owned)))
                .collect();
            found.sort_by_key(|(id, _)| id.as_str().to_owned());
            found.dedup();
            found
        }
    };
    if subjects.is_empty() {
        return Err(diagnostic::input_invalid("empty-subject-set"));
    }
    if subjects.len() as u64 > profile.max_subjects {
        return Err(diagnostic::input_invalid("subject-limit-exceeded"));
    }

    let provenance = Provenance {
        model_version: graph.model_version().as_str().to_owned(),
        ir_digest: effects.ir_digest().to_owned(),
        graph_identity: graph.identity().to_owned(),
        effect_identity: crate::effects::version::IDENTITY.to_owned(),
        policy: StateValue::Unknown,
        baseline: StateValue::Unknown,
    };
    let mut rows: Vec<SubjectReport> = Vec::with_capacity(subjects.len());
    let mut union_required: BTreeMap<String, u64> = BTreeMap::new();
    let mut over_budget = 0u64;
    let mut indeterminate = 0u64;
    let mut bounded_subjects: Vec<String> = Vec::new();
    let mut over_budget_subjects: Vec<String> = Vec::new();
    let mut warnings: Vec<crate::diagnostics::Diagnostic> = Vec::new();

    for (node_id, module) in &subjects {
        let root_nodes: Vec<&crate::graph::GraphNode> =
            vec![graph.node(node_id).expect("resolved subject")];
        let root_ids: Vec<&NodeId> = root_nodes.iter().map(|node| node.id()).collect();
        let limits = closure::ClosureLimits::effective(&profile);
        let selection_facts = facts::collect_facts(&root_nodes, &graph, &effects, &context, limits);
        let closure = closure::dependency_closure(&root_ids, &graph, &EdgeFilter::new(), limits);
        let hops = closure::module_hop_attribution(&root_ids, &graph, &EdgeFilter::new(), limits);

        let complete = selection_facts.complete && closure.complete && hops.complete;
        let required_tokens = selection_facts.required_tokens();
        for (id, tokens) in selection_facts
            .required
            .iter()
            .map(|fact| (fact.id.clone(), fact.tokens))
        {
            union_required.insert(id, tokens);
        }

        // The profile-scoped minimum-safe estimate carries the declared
        // margin/framing; assessment and over-by compare that effective
        // cost against the available content budget, so a profile with
        // framing cannot slip its own overhead past the gate.
        let effective_required =
            metrics::minimum_safe(&profile, StateValue::Known(required_tokens))
                .value()
                .copied()
                .unwrap_or(required_tokens);
        let (assessment, over_by) = metrics::assess(
            complete,
            effective_required,
            profile.available_content_tokens,
        );
        match assessment {
            Assessment::OverBudget => {
                over_budget += 1;
                over_budget_subjects.push(node_id.as_str().to_owned());
            }
            Assessment::Indeterminate => indeterminate += 1,
            Assessment::WithinBudget => {}
        }
        if !complete {
            bounded_subjects.push(node_id.as_str().to_owned());
        }

        // The per-dependency attribution: each required fact is billed
        // once to its owning dependency (the fact's own node when the
        // fact is that node's contract; effect-edge facts bill to the
        // operation that declares them). A fact whose owner is not a
        // closure dependency (the subject itself, or a synthesized fact
        // of the subject) is shared cost of the subject row and is
        // attributed to the canonically first dependency so that the
        // exclusive + shared sums always reconcile to F.
        let subject_id = node_id.as_str().to_owned();
        let mut attribution: BTreeMap<String, u64> = BTreeMap::new();
        let mut shared_total: u64 = 0;
        for fact in &selection_facts.required {
            let owner: String = match fact.id.rfind("->") {
                // Synthesized effect-edge fact: bill the declaring
                // operation (the text between the first ':' and the
                // first "->").
                Some(split) => {
                    let head = &fact.id[..split];
                    match head.split_once(':') {
                        Some((_, operation)) => operation.to_owned(),
                        None => subject_id.clone(),
                    }
                }
                None => fact.id.clone(),
            };
            if owner == subject_id || !closure.transitive.contains(&owner) {
                shared_total = shared_total.saturating_add(fact.tokens);
                continue;
            }
            let entry = attribution.entry(owner).or_insert(0);
            *entry = entry.saturating_add(fact.tokens);
        }
        // Shared cost lands on the canonically first dependency so the
        // additive reconciliation (exclusive + shared = required) holds.
        let first_dependency = closure.transitive.iter().next().cloned();
        let mut breakdown: Vec<BreakdownRow> = Vec::new();
        let mut breakdown_truncated = false;
        for dependency in &closure.transitive {
            if breakdown.len() >= version::MAX_BREAKDOWN_ROWS {
                breakdown_truncated = true;
                break;
            }
            let module = graph
                .resolve(dependency)
                .and_then(|node| node.module())
                .map(str::to_owned);
            let hop = hops.hops.get(dependency).copied();
            let exclusive = attribution.get(dependency).copied().unwrap_or(0);
            let shared = if first_dependency.as_deref() == Some(dependency.as_str()) {
                shared_total
            } else {
                0
            };
            breakdown.push(BreakdownRow {
                dependency: dependency.clone(),
                module,
                hops: hop,
                exclusive_required_tokens: exclusive,
                shared_required_tokens: shared,
            });
        }
        breakdown.sort_by(|left, right| {
            (
                std::cmp::Reverse(right.exclusive_required_tokens + right.shared_required_tokens),
                left.dependency.clone(),
            )
                .cmp(&(
                    std::cmp::Reverse(left.exclusive_required_tokens + left.shared_required_tokens),
                    right.dependency.clone(),
                ))
        });

        // Advisory extraction suggestions: the single dominant crossing
        // module candidate, ranked by its exclusive token weight.
        let mut suggestions: Vec<Suggestion> = Vec::new();
        if request.suggest {
            // Crossing-module candidate weights: the exclusive cost of
            // the dependencies behind each non-subject module boundary.
            let mut module_weights: BTreeMap<String, u64> = BTreeMap::new();
            for row in &breakdown {
                if let Some(row_module) = &row.module {
                    if row_module != module.as_deref().unwrap_or("") {
                        *module_weights.entry(row_module.clone()).or_insert(0) +=
                            row.exclusive_required_tokens;
                    }
                }
            }
            let mut candidates: Vec<(String, u64)> = module_weights.into_iter().collect();
            candidates
                .sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
            for (boundary_id, weight) in candidates.into_iter().take(version::MAX_SUGGESTIONS) {
                if weight == 0 {
                    continue;
                }
                let cut: Vec<String> = closure
                    .transitive
                    .iter()
                    .filter(|dependency| {
                        graph
                            .resolve(dependency)
                            .and_then(|node| node.module())
                            .map(str::to_owned)
                            .as_deref()
                            == Some(boundary_id.as_str())
                    })
                    .cloned()
                    .collect();
                let after = required_tokens.saturating_sub(weight);
                suggestions.push(Suggestion {
                    boundary_id: boundary_id.clone(),
                    cut_dependencies: cut,
                    preserved_facts: Vec::new(),
                    before_tokens: required_tokens,
                    after_tokens: after,
                    // An advisory cut never hides required authorization,
                    // transaction, or effect facts mechanically: the
                    // preserved set must be wired by a real refactor.
                    blocker: Some("advisory-only-extraction".to_owned()),
                });
            }
        }

        // The opt-in simulation: the unchanged legacy capsule at the same
        // effective budget plus the new required/supporting selection.
        let simulation = if request.simulate {
            let scope = crate::context::CapsuleScope::Symbol(node_id.semantic_id().to_owned());
            match crate::context::plan(&scope, profile.available_content_tokens, false, compilation)
            {
                Ok(capsule) => {
                    // One deterministic greedy selection over the required
                    // facts in canonical order: a fact joins only when it
                    // still fits cumulatively. Inclusion and the missing
                    // set derive from that selection, never from an
                    // "individually cheaper than the whole budget" test.
                    let mut included: Vec<&facts::LedgerFact> = Vec::new();
                    let mut missing: Vec<String> = Vec::new();
                    let mut used: u64 = 0;
                    for fact in &selection_facts.required {
                        if used + fact.tokens <= profile.available_content_tokens {
                            used += fact.tokens;
                            included.push(fact);
                        } else {
                            missing.push(fact.id.clone());
                        }
                    }
                    Some(Simulation {
                        candidate_facts: capsule.candidate_count() as u64,
                        included_facts: capsule.included_count() as u64,
                        required_facts: selection_facts.required.len() as u64,
                        included_required_facts: included.len() as u64,
                        estimated_tokens: capsule.estimated_tokens(),
                        // Required facts are atomic as a sufficiency set:
                        // F fits only when the whole set fits and the
                        // selection was complete.
                        required_fits: complete
                            && required_tokens <= profile.available_content_tokens,
                        all_candidates_fit: capsule.fits(),
                        missing_required_ids: missing,
                        // The legacy capsule measures its own different
                        // fact set; both stay explicit, never merged.
                        legacy_minimum_required: capsule.minimum_required_tokens(),
                        legacy_estimated: capsule.estimated_tokens(),
                        legacy_fits: capsule.fits(),
                    })
                }
                Err(_) => None,
            }
        } else {
            None
        };

        let edge_occurrences = {
            // Distinct admitted edge occurrences over the closure walk:
            // every outgoing edge of the subject set and its reached
            // dependency-bearing nodes, counted once per canonical edge.
            let mut edges: BTreeSet<(String, String, String, u32)> = BTreeSet::new();
            let filter = EdgeFilter::new();
            let mut queue: std::collections::VecDeque<&NodeId> = VecDeque::new();
            let mut seen: BTreeSet<String> =
                root_ids.iter().map(|id| id.as_str().to_owned()).collect();
            for root in &root_ids {
                queue.push_back(root);
            }
            while let Some(node) = queue.pop_front() {
                for edge in graph.direct_dependencies(node, &filter) {
                    let key = edge.key();
                    edges.insert((
                        key.from().as_str().to_owned(),
                        key.relation().key().to_owned(),
                        key.to().as_str().to_owned(),
                        key.occurrence().get(),
                    ));
                    let target = key.to();
                    if seen.insert(target.as_str().to_owned()) {
                        queue.push_back(target);
                    }
                }
            }
            StateValue::Known(edges.len() as u64)
        };
        let (policies_count, scenarios_count, effects_count) =
            semantic_counts_of(&selection_facts, &effects);
        let hops_max = hop_maximum(&hops);
        rows.push(SubjectReport {
            id: node_id.as_str().to_owned(),
            module: module.clone(),
            metrics: SubjectMetrics {
                direct_dependencies: StateValue::Known(closure.direct.len() as u64),
                transitive_dependencies: StateValue::Known(closure.transitive.len() as u64),
                indirect_only_dependencies: if closure.complete {
                    StateValue::Known(closure.indirect_only.len() as u64)
                } else {
                    StateValue::Unknown
                },
                required_modules: {
                    let mut subject_modules: Vec<String> = module.clone().into_iter().collect();
                    subject_modules.extend(
                        selection_facts
                            .required
                            .iter()
                            .filter_map(|fact| fact.module.clone()),
                    );
                    metrics::required_module_count(&selection_facts, &subject_modules)
                },
                context_closure_estimated_tokens: StateValue::Known(
                    required_tokens.saturating_add(selection_facts.supporting_tokens()),
                ),
                minimum_required_semantic_tokens: StateValue::Known(required_tokens),
                supporting_semantic_tokens: StateValue::Known(selection_facts.supporting_tokens()),
                optional_source_tokens: if request.source_context {
                    // The mapped-files recipe is declared but the artifact
                    // evidence adapter is not wired in this generation:
                    // honestly unsupported, with the registered warning.
                    StateValue::Unsupported
                } else {
                    StateValue::Unknown
                },
                model_files: model_file_count(&selection_facts, compilation, &graph),
                source_files: StateValue::Unknown,
                target_files: StateValue::Unknown,
                max_cross_module_hops: hops_max,
                unresolved_edges: StateValue::Known(0),
                ambiguous_edges: StateValue::Known(0),
                declared_effects: effects_count,
                detected_effects: if effects.envelope_count() == 0 {
                    // No detection evidence was contributed: the metric is
                    // unknown, never an optimistic zero.
                    StateValue::Unknown
                } else {
                    // Evidence exists: count the distinct detected edge
                    // identities that were attached (a confirmed empty set
                    // is a known zero, never silently optimistic).
                    StateValue::Known(effects.detected().len() as u64)
                },
                policies: policies_count,
                scenarios: scenarios_count,
                // No artifact-evidence adapter is wired, so the file
                // metric is honestly unknown; the largest semantic fact
                // is reported in its own field (never labeled a file).
                largest_required_artifact: StateValue::Unknown,
                largest_required_semantic_fact: largest_required_fact(&selection_facts),
                edge_occurrences,
                duplicate_supporting_tokens: metrics::duplicate_supporting_tokens(
                    &selection_facts.supporting,
                    &supporting_requests(&root_ids, &graph, &EdgeFilter::new()),
                ),
                generated_maintained_ratio: StateValue::Unknown,
                minimum_safe_context_estimate: metrics::minimum_safe(
                    &profile,
                    StateValue::Known(required_tokens),
                ),
                empirically_safe_context_tokens: StateValue::Unknown,
            },
            assessment,
            over_by_tokens: over_by,
            breakdown,
            breakdown_truncated,
            required_facts: selection_facts.required,
            supporting_facts: selection_facts.supporting,
            gaps: selection_facts.gaps,
            suggestions,
            simulation,
        });
    }

    warnings.extend(diagnostic::budget_exceeded(&over_budget_subjects));
    warnings.extend(diagnostic::closure_incomplete(
        &bounded_subjects,
        "closure-bounded",
    ));

    let union_tokens = union_required
        .values()
        .copied()
        .try_fold(0u64, |total, tokens| total.checked_add(tokens));
    let summary = ReportSummary {
        subjects: rows.len() as u64,
        over_budget_subjects: over_budget,
        indeterminate_subjects: indeterminate,
        union_required_tokens: match union_tokens {
            Some(total) => StateValue::Known(total),
            None => StateValue::Unknown,
        },
    };
    Ok(BudgetReport {
        scope: request.scope.clone(),
        provenance,
        profile,
        subjects: rows,
        summary,
        complete: bounded_subjects.is_empty(),
        warnings,
    })
}

fn semantic_counts_of(
    selection: &facts::FactSelection,
    _effects: &EffectGraph,
) -> (StateValue<u64>, StateValue<u64>, StateValue<u64>) {
    let policies = selection
        .required
        .iter()
        .filter(|fact| fact.reason == Some(InclusionReason::PolicyApplicability))
        .count() as u64;
    let scenarios = selection
        .required
        .iter()
        .filter(|fact| fact.reason == Some(InclusionReason::ScenarioCoverage))
        .count() as u64;
    (
        StateValue::Known(policies),
        StateValue::Known(scenarios),
        StateValue::Known(selection.counted_operations.len() as u64),
    )
}

fn hop_maximum(hops: &closure::HopAttribution) -> StateValue<u64> {
    if !hops.complete {
        return StateValue::Unknown;
    }
    StateValue::Known(hops.hops.values().copied().max().unwrap_or(0))
}

fn model_file_count(
    selection: &facts::FactSelection,
    compilation: &Compilation,
    graph: &DependencyGraph,
) -> StateValue<u64> {
    let mut files: BTreeSet<String> = BTreeSet::new();
    for fact in &selection.required {
        let Some(node) = graph.resolve(&fact.id) else {
            continue;
        };
        let semantic = node.id().semantic_id();
        for entry in compilation.source_map.entries() {
            if entry.semantic_id.as_deref() == Some(semantic) {
                files.insert(entry.path.clone());
            }
        }
    }
    StateValue::Known(files.len() as u64)
}

fn largest_required_fact(selection: &facts::FactSelection) -> StateValue<metrics::LargestArtifact> {
    let mut best: Option<&facts::LedgerFact> = None;
    for fact in &selection.required {
        if best.map_or(true, |current| {
            (fact.tokens, &fact.id) > (current.tokens, &current.id)
        }) {
            best = Some(fact);
        }
    }
    match best {
        Some(fact) => StateValue::Known(metrics::LargestArtifact {
            artifact_id: fact.id.clone(),
            role: "model",
            bytes: None,
            estimated_tokens: fact.tokens,
        }),
        None => StateValue::Unknown,
    }
}

/// The bounded request provenance of the closure: count every edge
/// occurrence that requested a node during the walk (before dedup), so
/// a node reached through several parents records one request per
/// incoming edge — the M8 duplicate-supporting input. Request pairs are
/// (root, referring-edge) bounded by the recorded edge bound.
fn supporting_requests(
    roots: &[&NodeId],
    graph: &DependencyGraph,
    filter: &EdgeFilter,
) -> BTreeMap<String, u64> {
    let mut requests: BTreeMap<String, u64> = BTreeMap::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<&NodeId> = VecDeque::new();
    for root in roots {
        visited.insert(root.as_str().to_owned());
        queue.push_back(root);
    }
    while let Some(node) = queue.pop_front() {
        if requests.len() > version::MAX_COMPARISON_ROWS {
            break;
        }
        for edge in graph.direct_dependencies(node, filter) {
            let target = edge.key().to();
            // One request per incoming edge occurrence of the target.
            *requests.entry(target.as_str().to_owned()).or_insert(0) += 1;
            if visited.insert(target.as_str().to_owned()) {
                queue.push_back(target);
            }
        }
    }
    requests
}

fn module_subjects<'a>(
    graph: &'a DependencyGraph,
    module: &str,
) -> Vec<(&'a NodeId, Option<String>)> {
    let mut found: Vec<(&NodeId, Option<String>)> = graph
        .nodes()
        .iter()
        .filter(|node| node.module() == Some(module))
        .filter(|node| node.kind() != crate::graph::NodeKindId::MODULE)
        .map(|node| (node.id(), node.module().map(str::to_owned)))
        .collect();
    found.sort_by_key(|(id, _)| id.as_str().to_owned());
    found
}

/// The canonical wire projection (byte-sorted object keys, compact UTF-8).
mod wire {
    use super::*;

    fn state_value_u64(value: &StateValue<u64>) -> String {
        match value {
            StateValue::Known(tokens) => crate::context::canonical::object(vec![
                ("state", crate::context::canonical::string("known")),
                ("value", crate::context::canonical::number(*tokens)),
            ]),
            state => crate::context::canonical::object(vec![(
                "state",
                crate::context::canonical::string(state.state()),
            )]),
        }
    }

    fn metrics_json(metrics: &SubjectMetrics) -> String {
        crate::context::canonical::object(vec![
            ("ambiguousEdges", state_value_u64(&metrics.ambiguous_edges)),
            (
                "contextClosureEstimatedTokens",
                state_value_u64(&metrics.context_closure_estimated_tokens),
            ),
            (
                "declaredEffects",
                state_value_u64(&metrics.declared_effects),
            ),
            (
                "detectedEffects",
                state_value_u64(&metrics.detected_effects),
            ),
            (
                "directDependencies",
                state_value_u64(&metrics.direct_dependencies),
            ),
            (
                "duplicateSupportingTokens",
                state_value_u64(&metrics.duplicate_supporting_tokens),
            ),
            (
                "empiricallySafeContextTokens",
                state_value_u64(&metrics.empirically_safe_context_tokens),
            ),
            (
                "generatedMaintainedRatio",
                match &metrics.generated_maintained_ratio {
                    StateValue::Known(ratio) => crate::context::canonical::object(vec![
                        (
                            "denominator",
                            crate::context::canonical::number(ratio.denominator),
                        ),
                        (
                            "externalFiles",
                            crate::context::canonical::number(ratio.external_files),
                        ),
                        (
                            "generatedFiles",
                            crate::context::canonical::number(ratio.generated_files),
                        ),
                        (
                            "maintainedFiles",
                            crate::context::canonical::number(ratio.maintained_files),
                        ),
                        (
                            "numerator",
                            crate::context::canonical::number(ratio.numerator),
                        ),
                        ("state", crate::context::canonical::string("known")),
                        (
                            "unclassifiedFiles",
                            crate::context::canonical::number(ratio.unclassified_files),
                        ),
                    ]),
                    state => crate::context::canonical::object(vec![(
                        "state",
                        crate::context::canonical::string(state.state()),
                    )]),
                },
            ),
            (
                "indirectOnlyDependencies",
                state_value_u64(&metrics.indirect_only_dependencies),
            ),
            (
                "largestRequiredArtifact",
                match &metrics.largest_required_artifact {
                    StateValue::Known(artifact) => {
                        let mut fields = vec![
                            (
                                "artifactId",
                                crate::context::canonical::string(&artifact.artifact_id),
                            ),
                            (
                                "estimatedTokens",
                                crate::context::canonical::number(artifact.estimated_tokens),
                            ),
                            ("role", crate::context::canonical::string(artifact.role)),
                        ];
                        fields.push((
                            "bytes",
                            match artifact.bytes {
                                Some(bytes) => crate::context::canonical::number(bytes),
                                None => "null".to_owned(),
                            },
                        ));
                        crate::context::canonical::object(fields)
                    }
                    state => crate::context::canonical::object(vec![(
                        "state",
                        crate::context::canonical::string(state.state()),
                    )]),
                },
            ),
            (
                "edgeOccurrences",
                state_value_u64(&metrics.edge_occurrences),
            ),
            (
                "largestRequiredSemanticFact",
                match &metrics.largest_required_semantic_fact {
                    StateValue::Known(artifact) => crate::context::canonical::object(vec![
                        (
                            "artifactId",
                            crate::context::canonical::string(&artifact.artifact_id),
                        ),
                        (
                            "estimatedTokens",
                            crate::context::canonical::number(artifact.estimated_tokens),
                        ),
                        ("role", crate::context::canonical::string(artifact.role)),
                        (
                            "bytes",
                            match artifact.bytes {
                                Some(bytes) => crate::context::canonical::number(bytes),
                                None => "null".to_owned(),
                            },
                        ),
                    ]),
                    state => crate::context::canonical::object(vec![(
                        "state",
                        crate::context::canonical::string(state.state()),
                    )]),
                },
            ),
            (
                "maxCrossModuleHops",
                state_value_u64(&metrics.max_cross_module_hops),
            ),
            (
                "minimumRequiredSemanticTokens",
                state_value_u64(&metrics.minimum_required_semantic_tokens),
            ),
            (
                "minimumSafeContextEstimate",
                state_value_u64(&metrics.minimum_safe_context_estimate),
            ),
            ("modelFiles", state_value_u64(&metrics.model_files)),
            (
                "optionalSourceTokens",
                state_value_u64(&metrics.optional_source_tokens),
            ),
            ("policies", state_value_u64(&metrics.policies)),
            (
                "requiredModules",
                state_value_u64(&metrics.required_modules),
            ),
            ("scenarios", state_value_u64(&metrics.scenarios)),
            ("sourceFiles", state_value_u64(&metrics.source_files)),
            (
                "supportingSemanticTokens",
                state_value_u64(&metrics.supporting_semantic_tokens),
            ),
            ("targetFiles", state_value_u64(&metrics.target_files)),
            (
                "transitiveDependencies",
                state_value_u64(&metrics.transitive_dependencies),
            ),
            (
                "unresolvedEdges",
                state_value_u64(&metrics.unresolved_edges),
            ),
        ])
    }

    fn breakdown_json(row: &BreakdownRow) -> String {
        crate::context::canonical::object(vec![
            (
                "dependency",
                crate::context::canonical::string(&row.dependency),
            ),
            (
                "exclusiveRequiredTokens",
                crate::context::canonical::number(row.exclusive_required_tokens),
            ),
            (
                "hops",
                match row.hops {
                    Some(hops) => crate::context::canonical::number(hops),
                    None => "null".to_owned(),
                },
            ),
            (
                "module",
                match &row.module {
                    Some(module) => crate::context::canonical::string(module),
                    None => "null".to_owned(),
                },
            ),
            (
                "sharedRequiredTokens",
                crate::context::canonical::number(row.shared_required_tokens),
            ),
        ])
    }

    fn fact_json(fact: &facts::LedgerFact) -> String {
        let mut fields = vec![
            ("id", crate::context::canonical::string(&fact.id)),
            (
                "module",
                match &fact.module {
                    Some(module) => crate::context::canonical::string(module),
                    None => "null".to_owned(),
                },
            ),
            ("tokens", crate::context::canonical::number(fact.tokens)),
        ];
        if let Some(reason) = fact.reason {
            fields.push(("reason", crate::context::canonical::string(reason.key())));
        }
        fields.push(("class", crate::context::canonical::string(fact.class.key())));
        crate::context::canonical::object(fields)
    }

    fn suggestion_json(suggestion: &Suggestion) -> String {
        crate::context::canonical::object(vec![
            (
                "afterTokens",
                crate::context::canonical::number(suggestion.after_tokens),
            ),
            (
                "beforeTokens",
                crate::context::canonical::number(suggestion.before_tokens),
            ),
            (
                "boundaryId",
                crate::context::canonical::string(&suggestion.boundary_id),
            ),
            (
                "blocker",
                match &suggestion.blocker {
                    Some(blocker) => crate::context::canonical::string(blocker),
                    None => "null".to_owned(),
                },
            ),
            (
                "cutDependencies",
                crate::context::canonical::array(
                    suggestion
                        .cut_dependencies
                        .iter()
                        .map(|id| crate::context::canonical::string(id)),
                ),
            ),
            (
                "preservedFacts",
                crate::context::canonical::array(
                    suggestion
                        .preserved_facts
                        .iter()
                        .map(|id| crate::context::canonical::string(id)),
                ),
            ),
        ])
    }

    fn simulation_json(simulation: &Simulation) -> String {
        crate::context::canonical::object(vec![
            (
                "allCandidatesFit",
                crate::context::canonical::boolean(simulation.all_candidates_fit),
            ),
            (
                "candidateFacts",
                crate::context::canonical::number(simulation.candidate_facts),
            ),
            (
                "estimatedTokens",
                crate::context::canonical::number(simulation.estimated_tokens),
            ),
            (
                "includedFacts",
                crate::context::canonical::number(simulation.included_facts),
            ),
            (
                "includedRequiredFacts",
                crate::context::canonical::number(simulation.included_required_facts),
            ),
            (
                "legacyEstimated",
                crate::context::canonical::number(simulation.legacy_estimated),
            ),
            (
                "legacyFits",
                crate::context::canonical::boolean(simulation.legacy_fits),
            ),
            (
                "legacyMinimumRequired",
                crate::context::canonical::number(simulation.legacy_minimum_required),
            ),
            (
                "missingRequiredIds",
                crate::context::canonical::array(
                    simulation
                        .missing_required_ids
                        .iter()
                        .map(|id| crate::context::canonical::string(id)),
                ),
            ),
            (
                "requiredFacts",
                crate::context::canonical::number(simulation.required_facts),
            ),
            (
                "requiredFits",
                crate::context::canonical::boolean(simulation.required_fits),
            ),
        ])
    }

    fn subject_json(subject: &SubjectReport) -> String {
        let fields = vec![
            (
                "assessment",
                crate::context::canonical::string(subject.assessment.key()),
            ),
            (
                "breakdown",
                crate::context::canonical::array(subject.breakdown.iter().map(breakdown_json)),
            ),
            (
                "breakdownTruncated",
                crate::context::canonical::boolean(subject.breakdown_truncated),
            ),
            (
                "gaps",
                crate::context::canonical::array(
                    subject
                        .gaps
                        .iter()
                        .map(|gap| crate::context::canonical::string(gap.key()))
                        .collect::<Vec<_>>(),
                ),
            ),
            ("id", crate::context::canonical::string(&subject.id)),
            ("metrics", metrics_json(&subject.metrics)),
            (
                "module",
                match &subject.module {
                    Some(module) => crate::context::canonical::string(module),
                    None => "null".to_owned(),
                },
            ),
            ("overByTokens", state_value_u64(&subject.over_by_tokens)),
            (
                "requiredFacts",
                crate::context::canonical::array(subject.required_facts.iter().map(fact_json)),
            ),
            (
                "simulation",
                match &subject.simulation {
                    Some(simulation) => simulation_json(simulation),
                    None => "null".to_owned(),
                },
            ),
            (
                "suggestions",
                crate::context::canonical::array(subject.suggestions.iter().map(suggestion_json)),
            ),
            (
                "supportingFacts",
                crate::context::canonical::array(subject.supporting_facts.iter().map(fact_json)),
            ),
        ];
        crate::context::canonical::object(fields)
    }

    /// The canonical payload bytes of one report.
    pub fn report_json(report: &BudgetReport) -> String {
        let scope = match &report.scope {
            Scope::Symbol(symbol) => crate::context::canonical::object(vec![
                ("id", crate::context::canonical::string(symbol)),
                ("kind", crate::context::canonical::string("symbol")),
            ]),
            Scope::Module(module) => crate::context::canonical::object(vec![
                ("id", crate::context::canonical::string(module)),
                ("kind", crate::context::canonical::string("module")),
            ]),
            Scope::All => crate::context::canonical::object(vec![
                ("id", crate::context::canonical::string("*")),
                ("kind", crate::context::canonical::string("project")),
            ]),
        };
        crate::context::canonical::object(vec![
            (
                "complete",
                crate::context::canonical::boolean(report.complete),
            ),
            (
                "estimator",
                crate::context::canonical::object(vec![
                    (
                        "digest",
                        crate::context::canonical::string(&report.profile.estimator_digest),
                    ),
                    (
                        "identity",
                        crate::context::canonical::string(&report.profile.estimator_identity),
                    ),
                    (
                        "version",
                        crate::context::canonical::string(&report.profile.estimator_version),
                    ),
                ]),
            ),
            (
                "identity",
                crate::context::canonical::string(version::IDENTITY),
            ),
            (
                "metricVersion",
                crate::context::canonical::string(version::METRIC_VERSION),
            ),
            (
                "profile",
                crate::context::canonical::object(vec![
                    (
                        "availableContentTokens",
                        crate::context::canonical::number(report.profile.available_content_tokens),
                    ),
                    (
                        "digest",
                        crate::context::canonical::string(&report.profile.digest),
                    ),
                    (
                        "framingTokens",
                        crate::context::canonical::number(report.profile.framing_tokens),
                    ),
                    ("id", crate::context::canonical::string(&report.profile.id)),
                    (
                        "sourceContext",
                        crate::context::canonical::string(report.profile.source_context.key()),
                    ),
                    (
                        "version",
                        crate::context::canonical::string(&report.profile.version),
                    ),
                ]),
            ),
            (
                "provenance",
                crate::context::canonical::object(vec![
                    (
                        "baseline",
                        crate::context::canonical::string(report.provenance.baseline.state()),
                    ),
                    (
                        "effectIdentity",
                        crate::context::canonical::string(&report.provenance.effect_identity),
                    ),
                    (
                        "graphIdentity",
                        crate::context::canonical::string(&report.provenance.graph_identity),
                    ),
                    (
                        "irDigest",
                        crate::context::canonical::string(&report.provenance.ir_digest),
                    ),
                    (
                        "modelVersion",
                        crate::context::canonical::string(&report.provenance.model_version),
                    ),
                    (
                        "policy",
                        match &report.provenance.policy {
                            StateValue::Known(digest) => crate::context::canonical::object(vec![
                                ("digest", crate::context::canonical::string(digest)),
                                ("state", crate::context::canonical::string("known")),
                            ]),
                            state => crate::context::canonical::object(vec![(
                                "state",
                                crate::context::canonical::string(state.state()),
                            )]),
                        },
                    ),
                ]),
            ),
            (
                "schemaVersion",
                crate::context::canonical::string(version::SCHEMA_VERSION),
            ),
            ("scope", scope),
            (
                "subjects",
                crate::context::canonical::array(report.subjects.iter().map(subject_json)),
            ),
            (
                "summary",
                crate::context::canonical::object(vec![
                    (
                        "indeterminateSubjects",
                        crate::context::canonical::number(report.summary.indeterminate_subjects),
                    ),
                    (
                        "overBudgetSubjects",
                        crate::context::canonical::number(report.summary.over_budget_subjects),
                    ),
                    (
                        "subjects",
                        crate::context::canonical::number(report.summary.subjects),
                    ),
                    (
                        "unionRequiredTokens",
                        state_value_u64(&report.summary.union_required_tokens),
                    ),
                ]),
            ),
        ])
    }

    /// The deterministic Markdown document of one report.
    pub fn report_markdown(report: &BudgetReport) -> String {
        let mut lines: Vec<String> = vec!["# context-budget report".to_owned()];
        lines.push(format!("- identity: {}", version::IDENTITY));
        lines.push(format!("- contract: {}", version::SCHEMA_VERSION));
        lines.push(format!(
            "- scope: {}",
            match &report.scope {
                Scope::Symbol(symbol) => format!("symbol {symbol}"),
                Scope::Module(module) => format!("module {module}"),
                Scope::All => "project *".to_owned(),
            }
        ));
        lines.push(format!(
            "- profile: {} v{} ({})",
            report.profile.id, report.profile.version, report.profile.digest
        ));
        lines.push(format!(
            "- estimator: {} tokens",
            report.profile.available_content_tokens
        ));
        lines.push(format!("- complete: {}", report.complete));
        lines.push(format!(
            "- summary: {} subject(s), {} over budget, {} indeterminate",
            report.summary.subjects,
            report.summary.over_budget_subjects,
            report.summary.indeterminate_subjects
        ));
        for subject in &report.subjects {
            lines.push(String::new());
            lines.push(format!("## {}", subject.id));
            lines.push(format!("- assessment: {}", subject.assessment.key()));
            if let StateValue::Known(over) = subject.over_by_tokens {
                lines.push(format!("- over by: {over} tokens"));
            }
            lines.push(format!(
                "- required semantic: {} tokens; supporting: {} tokens",
                subject
                    .metrics
                    .minimum_required_semantic_tokens
                    .value()
                    .copied()
                    .unwrap_or(0),
                subject
                    .metrics
                    .supporting_semantic_tokens
                    .value()
                    .copied()
                    .unwrap_or(0),
            ));
            if !subject.breakdown.is_empty() {
                lines.push("### breakdown".to_owned());
                for row in subject.breakdown.iter().take(10) {
                    let module = row.module.as_deref().unwrap_or("none");
                    let hops = row
                        .hops
                        .map(|hops| hops.to_string())
                        .unwrap_or_else(|| "?".to_owned());
                    lines.push(format!(
                        "- {} (module {module}, hops {hops}, exclusive {} tokens)",
                        row.dependency, row.exclusive_required_tokens
                    ));
                }
                if subject.breakdown_truncated {
                    lines.push(format!(
                        "- … breakdown truncated at {} rows",
                        version::MAX_BREAKDOWN_ROWS
                    ));
                }
            }
            for suggestion in &subject.suggestions {
                lines.push(format!(
                    "- suggestion: extract {} (before {} tokens, after {} tokens, advisory{})",
                    suggestion.boundary_id,
                    suggestion.before_tokens,
                    suggestion.after_tokens,
                    suggestion
                        .blocker
                        .as_deref()
                        .map(|blocker| format!("; {blocker}"))
                        .unwrap_or_default(),
                ));
            }
            if let Some(simulation) = &subject.simulation {
                lines.push(format!(
                    "- simulation: {} candidates, {} included, required fits {}",
                    simulation.candidate_facts, simulation.included_facts, simulation.required_fits
                ));
            }
            if !subject.gaps.is_empty() {
                lines.push(format!(
                    "- gaps: {}",
                    subject
                        .gaps
                        .iter()
                        .map(|gap| gap.key())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        lines.join("\n")
    }
}
