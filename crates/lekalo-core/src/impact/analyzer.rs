//! The deterministic impact analyzer (issue #16).
//!
//! One entry point: [`analyze`] consumes the accepted #8 compiled
//! project, the #13 dependency graph, the #14 effect graph, a validated
//! [`ImpactRequest`], and an optional typed [`ChangedInputSet`], and
//! assembles the complete [`ImpactResult`]. Traversal is a bounded,
//! iterative, multi-source reverse walk over the graph's own reverse
//! index — never a source rescan, never recursion, never an all-pairs
//! expansion. The mandatory-public closure runs as a second, depth-free
//! walk restricted to public and structural nodes, so a low `--depth`
//! can never hide public impact; it still obeys the hard global caps.
//!
//! Failures: malformed requests and bound exhaustion are `invalid`
//! (`Err(ImpactFailure::Invalid)`); a strict profile with required
//! unknown or stale evidence denies (`Err(ImpactFailure::Denied)`).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use crate::diagnostics::Diagnostic;
use crate::effects::{EffectGraph, EffectKind, EffectProvenance, OperationId, TrustState};
use crate::graph::model::GraphEdge;
use crate::graph::{DependencyGraph, EdgeFilter, NodeId};
use crate::ir::{CompiledProject, Definition, Tombstone};

use super::canonical;
use super::diagnostic;
use super::evidence;
use super::gate;
use super::input::ChangedInputSet;
use super::path;
use super::request::ImpactRequest;
use super::risk::{self, OperationObservation, SymbolFact};
use super::version::{MAX_ITEMS, MAX_PATH_EDGES};
use super::{
    ChangedMode, Confidence, EvidenceSection, EvidenceState, ExplanationPath, GateSection,
    ImpactItem, ImpactProfile, ImpactResult, ImpactSection, InputMode, NamedSection,
    ProvenanceKind, RiskDimension, RiskSection, Scope, SectionState, SectionSummary, TargetItem,
    TargetSection,
};

/// The terminal failure classes of one analysis.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImpactFailure {
    /// Malformed request or bound exhaustion: invalid, exit 1.
    Invalid(crate::diagnostics::DiagnosticSet),
    /// Strict-profile gate denial: denied, exit 3.
    Denied(crate::diagnostics::DiagnosticSet),
}

/// Analyze the impact of one symbol or one typed changed-input set.
pub fn analyze(
    project: &CompiledProject,
    graph: &DependencyGraph,
    effects: &EffectGraph,
    request: &ImpactRequest,
    changed: Option<&ChangedInputSet>,
) -> Result<ImpactResult, ImpactFailure> {
    let surface = Surface::collect(project, graph);
    let filter = build_filter(graph, request).map_err(ImpactFailure::Invalid)?;

    // Resolve roots: symbol mode straight from the request; changed mode
    // through the handoff's resolved symbols.
    let mut unresolved_entries = 0usize;
    let mut root_ids: Vec<NodeId> = Vec::new();
    if let Some(set) = changed {
        for entry in set.entries() {
            let mut resolved = false;
            for symbol in entry.symbol_ids() {
                if let Some(node) = graph.resolve_id(symbol) {
                    let node = node.clone();
                    if !root_ids.contains(&node) {
                        root_ids.push(node);
                    }
                    resolved = true;
                }
            }
            if !resolved {
                unresolved_entries += 1;
            }
        }
        if root_ids.is_empty() {
            return Err(ImpactFailure::Invalid(
                diagnostic::changed_input_invalid_set("no-resolvable-symbols"),
            ));
        }
    } else {
        for symbol in request.roots() {
            let Some(node) = graph.resolve_id(symbol) else {
                return Err(ImpactFailure::Invalid(diagnostic::symbol_unknown_set(
                    symbol,
                )));
            };
            root_ids.push(node.clone());
        }
    }
    root_ids.sort_by(|left, right| node_order(graph, left).cmp(&node_order(graph, right)));

    // Pass A: the depth-bounded reverse radius with parent chains.
    let bounded = walk(
        graph,
        &root_ids,
        &filter,
        Some(usize::from(request.depth())),
    )?;
    // Pass B: the depth-free mandatory-public closure.
    let public = walk_public(graph, &root_ids, &filter, &surface)?;

    let mut explanations: Vec<ExplanationPath> = Vec::new();

    // Direct and transitive items from pass A.
    let mut direct_items: Vec<ImpactItem> = Vec::new();
    let mut transitive_items: Vec<ImpactItem> = Vec::new();
    for (subject, distance) in &bounded.distances {
        if *distance == 0 {
            continue;
        }
        let mut item = base_item(subject.clone(), *distance);
        item.scope = if *distance == 1 {
            Scope::Direct
        } else {
            Scope::Transitive
        };
        attach_path(
            &mut item,
            subject,
            &bounded.parents,
            &root_ids,
            &mut explanations,
        );
        if item.scope == Scope::Direct {
            direct_items.push(item);
        } else {
            transitive_items.push(item);
        }
    }

    // Mandatory-public items from pass B; subjects already discovered by
    // pass A keep their tighter distance and appear with public reasons
    // only when pass B is their only discovery.
    let mut public_items: Vec<ImpactItem> = Vec::new();
    for (subject, distance) in &public.distances {
        if *distance == 0 {
            continue;
        }
        let known = direct_items
            .iter()
            .chain(transitive_items.iter())
            .any(|item| item.subject == *subject);
        if known || !surface.is_public(subject) {
            continue;
        }
        let mut item = base_item(subject.clone(), *distance);
        item.scope = Scope::MandatoryPublic;
        item.reason_refs = vec![risk::reason::PUBLIC_VISIBILITY.to_owned()];
        attach_path(
            &mut item,
            subject,
            &public.parents,
            &root_ids,
            &mut explanations,
        );
        public_items.push(item);
    }

    // Order everything canonically.
    direct_items.sort_by(|left, right| compare_items(graph, left, right));
    transitive_items.sort_by(|left, right| compare_items(graph, left, right));
    public_items.sort_by(|left, right| compare_items(graph, left, right));
    explanations.sort_by(|left, right| left.path_id.cmp(&right.path_id));
    let mut all_items: Vec<ImpactItem> = direct_items
        .iter()
        .chain(transitive_items.iter())
        .chain(public_items.iter())
        .cloned()
        .collect();
    // Roots participate in the effect and policy observations: an affected
    // root operation carries its own effect surface.
    for root in &root_ids {
        if !all_items.iter().any(|item| item.subject == *root) {
            all_items.push(base_item(root.clone(), 0));
        }
    }

    // Effect observations over every affected operation or effect.
    let observations = collect_observations(graph, effects, &all_items, &surface);

    // Radius facts for the risk vector (affected subjects plus roots).
    let mut radius: BTreeMap<NodeId, SymbolFact> = BTreeMap::new();
    for item in &all_items {
        radius.insert(item.subject.clone(), surface.fact(&item.subject));
    }
    for root in &root_ids {
        radius
            .entry(root.clone())
            .or_insert_with(|| surface.fact(root));
    }

    // Affected-module target bindings, filtered by the target selector.
    let mut affected_modules: HashSet<Option<String>> = HashSet::new();
    for item in &all_items {
        affected_modules.insert(node_module(graph, &item.subject));
    }
    for root in &root_ids {
        affected_modules.insert(node_module(graph, root));
    }
    let bindings: Vec<(NodeId, String)> = surface
        .bindings()
        .iter()
        .filter(|(node, _, _)| affected_modules.contains(&node_module(graph, node)))
        .filter(|(_, target, _)| match request.target() {
            Some(wanted) => wanted == target,
            None => true,
        })
        .map(|(node, target, _)| (node.clone(), target.clone()))
        .collect();

    let seeds: Vec<super::MemberSeed> = changed
        .map(|set| {
            set.entries()
                .iter()
                .flat_map(|entry| entry.member_seeds().iter().cloned())
                .collect()
        })
        .unwrap_or_default();

    // The closed risk vector.
    let risk_items = risk::compute(
        &radius,
        &observations,
        &bindings,
        &seeds,
        unresolved_entries,
    );

    // Target, scenario, artifact, and test projections.
    let target_items: Vec<TargetItem> = bindings
        .iter()
        .map(|(node, target)| TargetItem {
            binding: node.clone(),
            target: target.clone(),
            reason_refs: vec![risk::reason::BINDING_REFERENCED.to_owned()],
            confidence: Confidence::Canonical,
        })
        .collect();
    let scenario_items: Vec<super::NamedItem> = all_items
        .iter()
        .filter(|item| surface.fact(&item.subject).kind == "scenario")
        .map(|item| super::NamedItem {
            id: item.subject.clone(),
            state: EvidenceState::Canonical,
            reason_refs: vec![risk::reason::SCENARIO_COVERS.to_owned()],
            confidence: Confidence::Canonical,
        })
        .collect();

    // Gates: neutral selection, strict denial check.
    let (gate_items, blocked) =
        gate::select(&risk_items, request.profile(), unresolved_entries > 0);
    if blocked
        || (request.profile() == ImpactProfile::Strict
            && gate::has_required_unknown_evidence(&gate_items))
    {
        return Err(ImpactFailure::Denied(diagnostic::gate_blocked_set(
            "required-gates-unknown-evidence",
        )));
    }

    // Backfill per-item risk and gate references.
    let dimensions_of = |subject: &NodeId| -> Vec<RiskDimension> {
        risk_items
            .iter()
            .filter(|risk| risk.subject_refs.contains(subject))
            .map(|risk| risk.dimension)
            .collect()
    };
    let backfill = |item: &mut ImpactItem| {
        let dimensions = dimensions_of(&item.subject);
        item.risk_refs = dimensions.clone();
        item.required_gate_refs = gate::required_refs(&gate_items, &dimensions);
    };
    let mut direct_final = direct_items;
    let mut transitive_final = transitive_items;
    let mut public_final = public_items;
    for item in direct_final.iter_mut() {
        backfill(item);
    }
    for item in transitive_final.iter_mut() {
        backfill(item);
    }
    for item in public_final.iter_mut() {
        backfill(item);
    }
    let public_truncated = public.truncated;
    let public_returned = public_final.len();

    // Evidence and warnings.
    let effect_degraded = observations
        .iter()
        .any(|(_, observation)| observation.degraded_evidence);
    let has_envelopes = effects.envelope_count() > 0;
    let changed_mode = changed.is_some();
    let changed_resolved = unresolved_entries == 0;
    let report = evidence::report(
        true,
        effect_degraded,
        has_envelopes,
        changed_mode,
        changed_resolved,
    );
    let degraded =
        report.degraded || bounded.horizon > 0 || public_truncated || unresolved_entries > 0;
    let mut reason_refs: Vec<String> = Vec::new();
    if unresolved_entries > 0 {
        reason_refs.push(diagnostic::CHANGED_INPUT_INCOMPLETE.to_owned());
    }
    if effect_degraded {
        reason_refs.push(diagnostic::EFFECT_STALE.to_owned());
    }
    reason_refs.push(diagnostic::EVIDENCE_UNKNOWN.to_owned());
    if public_truncated || unresolved_entries > 0 {
        reason_refs.push(diagnostic::PUBLIC_IMPACT_INCOMPLETE.to_owned());
    }
    reason_refs.sort();
    reason_refs.dedup();

    let warnings = collect_warnings(
        unresolved_entries,
        effect_degraded,
        public_truncated,
        has_envelopes,
    );
    let diagnostic_refs: Vec<String> = warnings
        .iter()
        .map(|warning| warning.id().to_owned())
        .collect();

    let evidence_section = EvidenceSection {
        summary: SectionSummary {
            state: if degraded {
                SectionState::Incomplete
            } else {
                SectionState::Complete
            },
            complete: !degraded,
            returned: report.surfaces.len(),
            omitted: 0,
            frontier: 0,
            reason_refs: reason_refs.clone(),
            provenance: vec![ProvenanceKind::CanonicalIr, ProvenanceKind::Evidence],
            confidence: evidence::meet(report.surfaces.iter().map(|surface| surface.confidence)),
        },
        items: report.surfaces,
    };

    let completeness = SectionSummary {
        state: if degraded {
            SectionState::Incomplete
        } else {
            SectionState::Complete
        },
        complete: !degraded,
        returned: all_items.len(),
        omitted: bounded.horizon,
        frontier: if public_truncated { public_returned } else { 0 },
        reason_refs,
        provenance: vec![ProvenanceKind::CanonicalIr],
        confidence: if degraded {
            Confidence::Unknown
        } else {
            Confidence::Canonical
        },
    };

    let mut result = ImpactResult {
        project: graph.project_id().map(str::to_owned),
        model_version: project.model_version,
        input_mode: match changed {
            None => InputMode::Symbol,
            Some(set) => match set.mode() {
                ChangedMode::Committed => InputMode::Committed,
                ChangedMode::Index => InputMode::Index,
                ChangedMode::Worktree => InputMode::Worktree,
                ChangedMode::Mixed => InputMode::Mixed,
            },
        },
        base_revision_ref: changed.and_then(|set| set.base_revision_ref().map(str::to_owned)),
        candidate_revision_ref: changed
            .and_then(|set| set.candidate_revision_ref().map(str::to_owned)),
        changed_input_digest: changed
            .map(|set| set.digest().to_owned())
            .unwrap_or_else(|| ChangedInputSet::empty().digest().to_owned()),
        request: request.clone(),
        roots: root_ids,
        direct: ImpactSection {
            summary: section_summary(direct_final.len(), bounded.horizon == 0, 0),
            items: direct_final,
        },
        transitive: ImpactSection {
            summary: section_summary(transitive_final.len(), true, bounded.horizon),
            items: transitive_final,
        },
        mandatory_public: ImpactSection {
            summary: section_summary(
                public_returned,
                !public_truncated && unresolved_entries == 0,
                0,
            ),
            items: public_final,
        },
        risks: RiskSection {
            summary: complete_summary(risk_items.len()),
            items: risk_items,
        },
        targets: TargetSection {
            summary: complete_summary(target_items.len()),
            items: target_items,
        },
        artifacts: NamedSection {
            summary: unknown_summary(),
            items: Vec::new(),
        },
        scenarios: NamedSection {
            summary: complete_summary(scenario_items.len()),
            items: scenario_items,
        },
        tests: NamedSection {
            summary: unsupported_summary(),
            items: Vec::new(),
        },
        gates: GateSection {
            summary: complete_summary(gate_items.len()),
            items: gate_items,
        },
        explanations,
        evidence: evidence_section,
        completeness,
        diagnostic_refs,
        warnings,
        digest: String::new(),
    };
    result.digest = canonical::digest_of(&result);
    Ok(result)
}

/// The canonical order of one node: kind rank, module, semantic id,
/// subkind (the graph's own node order).
pub(crate) fn node_order(
    graph: &DependencyGraph,
    node: &NodeId,
) -> (u16, Option<String>, String, Option<&'static str>) {
    match graph.node(node) {
        Some(node) => (
            node.kind().rank(),
            node.module().map(str::to_owned),
            node.id().semantic_id().to_owned(),
            node.subkind(),
        ),
        None => (u16::MAX, None, node.semantic_id().to_owned(), None),
    }
}

fn node_module(graph: &DependencyGraph, node: &NodeId) -> Option<String> {
    graph
        .node(node)
        .and_then(|node| node.module().map(str::to_owned))
}

fn compare_items(
    graph: &DependencyGraph,
    left: &ImpactItem,
    right: &ImpactItem,
) -> std::cmp::Ordering {
    left.distance
        .cmp(&right.distance)
        .then_with(|| node_order(graph, &left.subject).cmp(&node_order(graph, &right.subject)))
}

fn base_item(subject: NodeId, distance: usize) -> ImpactItem {
    ImpactItem {
        subject,
        scope: Scope::Direct,
        distance,
        reason_refs: vec![risk::reason::DEPENDENCY_EDGE.to_owned()],
        path_refs: Vec::new(),
        evidence_state: EvidenceState::Canonical,
        confidence: Confidence::Canonical,
        risk_refs: Vec::new(),
        required_gate_refs: Vec::new(),
    }
}

/// One completed walk: distances, first-discovery parent edges, a horizon
/// count (admitted neighbors beyond the bound), and a truncation marker.
struct Walk {
    distances: HashMap<NodeId, usize>,
    parents: HashMap<NodeId, GraphEdge>,
    horizon: usize,
    truncated: bool,
}

/// Reconstruct the explanation path for one subject and register it.
fn attach_path(
    item: &mut ImpactItem,
    subject: &NodeId,
    parents: &HashMap<NodeId, GraphEdge>,
    roots: &[NodeId],
    explanations: &mut Vec<ExplanationPath>,
) {
    let mut cursor = subject.clone();
    let mut chain_root = subject.clone();
    // The parent map is a BFS forest rooted at the request roots; the
    // guard keeps an impossible cycle bounded instead of hanging.
    let mut guard = 0usize;
    while !roots.contains(&cursor) && guard < 1024 {
        let Some(parent) = parents.get(&cursor) else {
            break;
        };
        cursor = parent.key().to().clone();
        chain_root = cursor.clone();
        guard += 1;
    }
    let (edges, relation_kinds, confidence) = path::reconstruct(parents, subject, &chain_root);
    if edges.is_empty() {
        return;
    }
    item.confidence = confidence;
    let id = path::path_id(&chain_root, subject, &edges);
    item.path_refs.push(id.clone());
    explanations.push(ExplanationPath {
        path_id: id,
        root: chain_root.clone(),
        subject: subject.clone(),
        ordered_edges: edges,
        relation_kinds,
        confidence,
        provenance_refs: path::provenance_refs(&edge_chain(parents, subject, &chain_root)),
    });
}

fn edge_chain(
    parents: &HashMap<NodeId, GraphEdge>,
    subject: &NodeId,
    root: &NodeId,
) -> Vec<GraphEdge> {
    let mut edges = Vec::new();
    let mut cursor = subject.clone();
    let mut guard = 0usize;
    while cursor != *root && guard < 1024 {
        let Some(parent) = parents.get(&cursor) else {
            break;
        };
        edges.push(parent.clone());
        cursor = parent.key().to().clone();
        guard += 1;
    }
    edges
}

/// The depth-bounded multi-source reverse walk.
fn walk(
    graph: &DependencyGraph,
    roots: &[NodeId],
    filter: &EdgeFilter,
    max_depth: Option<usize>,
) -> Result<Walk, ImpactFailure> {
    let mut distances: HashMap<NodeId, usize> = HashMap::new();
    let mut parents: HashMap<NodeId, GraphEdge> = HashMap::new();
    let mut horizon: HashSet<NodeId> = HashSet::new();
    let mut queue: VecDeque<(NodeId, usize)> = VecDeque::new();
    for root in roots {
        distances.insert(root.clone(), 0);
        queue.push_back((root.clone(), 0));
    }
    while let Some((node, depth)) = queue.pop_front() {
        let at_depth_limit = max_depth.is_some_and(|limit| depth >= limit);
        for edge in graph.reverse_dependencies(&node, filter) {
            let next = edge.key().from().clone();
            if distances.contains_key(&next) {
                continue;
            }
            if at_depth_limit {
                horizon.insert(next);
                if horizon.len() > MAX_ITEMS {
                    return Err(ImpactFailure::Invalid(diagnostic::traversal_limit_set(
                        "item-limit",
                    )));
                }
                continue;
            }
            if distances.len() + horizon.len() > MAX_ITEMS {
                return Err(ImpactFailure::Invalid(diagnostic::traversal_limit_set(
                    "item-limit",
                )));
            }
            if parents.len() + 1 > MAX_PATH_EDGES {
                return Err(ImpactFailure::Invalid(diagnostic::traversal_limit_set(
                    "path-limit",
                )));
            }
            distances.insert(next.clone(), depth + 1);
            parents.insert(next.clone(), edge.clone());
            queue.push_back((next, depth + 1));
        }
    }
    Ok(Walk {
        distances,
        parents,
        horizon: horizon.len(),
        truncated: false,
    })
}

/// The depth-free mandatory-public walk: only public and structural
/// nodes are discovered and traversed, so a low depth can never hide
/// public impact. Cap exhaustion degrades completeness visibly instead
/// of rejecting the whole result (the owner decision in ADR-0017).
fn walk_public(
    graph: &DependencyGraph,
    roots: &[NodeId],
    filter: &EdgeFilter,
    surface: &Surface,
) -> Result<Walk, ImpactFailure> {
    let mut distances: HashMap<NodeId, usize> = HashMap::new();
    let mut parents: HashMap<NodeId, GraphEdge> = HashMap::new();
    let mut queue: VecDeque<(NodeId, usize)> = VecDeque::new();
    for root in roots {
        distances.insert(root.clone(), 0);
        queue.push_back((root.clone(), 0));
    }
    let mut truncated = false;
    while let Some((node, depth)) = queue.pop_front() {
        for edge in graph.reverse_dependencies(&node, filter) {
            let next = edge.key().from().clone();
            if distances.contains_key(&next) {
                continue;
            }
            if !surface.is_public(&next) {
                continue;
            }
            if distances.len() > MAX_ITEMS {
                truncated = true;
                break;
            }
            distances.insert(next.clone(), depth + 1);
            parents.insert(next.clone(), edge.clone());
            queue.push_back((next, depth + 1));
        }
        if truncated {
            break;
        }
    }
    Ok(Walk {
        distances,
        parents,
        horizon: 0,
        truncated,
    })
}

/// Collect the effect observations of every affected operation or effect.
fn collect_observations(
    graph: &DependencyGraph,
    effects: &EffectGraph,
    items: &[ImpactItem],
    surface: &Surface,
) -> Vec<(NodeId, OperationObservation)> {
    let mut observations: Vec<(NodeId, OperationObservation)> = Vec::new();
    for item in items {
        let fact = surface.fact(&item.subject);
        if !matches!(fact.kind, "operation" | "command" | "query" | "effect") {
            continue;
        }
        let mut observation = OperationObservation {
            is_command: fact.subkind == Some("command"),
            has_policy: graph
                .incoming(&item.subject, &EdgeFilter::new())
                .iter()
                .any(|edge| edge.key().relation().key() == "authorizes"),
            ..OperationObservation::default()
        };
        let edges: Vec<crate::effects::EffectEdge> = if fact.kind != "effect" {
            OperationId::from_semantic(item.subject.semantic_id())
                .map(|operation| {
                    effects
                        .operation_edges(&operation)
                        .into_iter()
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        } else {
            let wanted = format!("effect:{}", item.subject.semantic_id());
            effects
                .declared()
                .iter()
                .chain(effects.detected().iter())
                .filter(|edge| edge.key().origin().to_canonical_string() == wanted)
                .cloned()
                .collect()
        };
        for edge in edges {
            match edge.key().kind() {
                EffectKind::Read => observation.reads = true,
                EffectKind::Create
                | EffectKind::Update
                | EffectKind::Delete
                | EffectKind::WriteField { .. } => {
                    observation.writes = true;
                    observation.mutates = true;
                }
                EffectKind::EmitEvent => observation.emits = true,
                _ => {}
            }
            if let Some(group) = edge.transaction_group() {
                observation.has_transaction_evidence = true;
                observation.transaction_group = Some(group.as_str().to_owned());
            }
            if is_degraded(edge.provenance()) {
                observation.degraded_evidence = true;
            }
        }
        observations.push((item.subject.clone(), observation));
    }
    observations
}

/// Whether one effect provenance carries stale or unknown trust.
fn is_degraded(provenance: &EffectProvenance) -> bool {
    matches!(
        provenance,
        EffectProvenance::Evidence {
            trust: TrustState::Stale | TrustState::Unknown,
            ..
        }
    )
}

/// Validate the filters against the closed graph registries.
fn build_filter(
    graph: &DependencyGraph,
    request: &ImpactRequest,
) -> Result<EdgeFilter, crate::diagnostics::DiagnosticSet> {
    let registry = graph.registry();
    let mut filter = EdgeFilter::new();
    for relation in request.relations() {
        if registry.relation(relation).is_none() {
            return Err(diagnostic::selector_invalid_set("unknown-relation"));
        }
    }
    if !request.relations().is_empty() {
        let keys: Vec<&str> = request.relations().iter().map(String::as_str).collect();
        filter = filter
            .with_relation_keys(registry, &keys)
            .map_err(|_| diagnostic::selector_invalid_set("relation-filter"))?;
    }
    for kind in request.kinds() {
        if registry.kind(kind).is_none() {
            return Err(diagnostic::selector_invalid_set("unknown-kind"));
        }
    }
    if !request.kinds().is_empty() {
        let keys: Vec<&str> = request.kinds().iter().map(String::as_str).collect();
        filter = filter
            .with_kind_keys(registry, &keys)
            .map_err(|_| diagnostic::selector_invalid_set("kind-filter"))?;
    }
    if let Some(module) = request.module() {
        filter = filter
            .with_module(module)
            .map_err(|_| diagnostic::selector_invalid_set("module-filter"))?;
    }
    Ok(filter)
}

fn complete_summary(returned: usize) -> SectionSummary {
    SectionSummary {
        state: SectionState::Complete,
        complete: true,
        returned,
        omitted: 0,
        frontier: 0,
        reason_refs: Vec::new(),
        provenance: vec![ProvenanceKind::CanonicalIr],
        confidence: Confidence::Canonical,
    }
}

fn section_summary(returned: usize, complete: bool, frontier: usize) -> SectionSummary {
    if complete {
        return complete_summary(returned);
    }
    SectionSummary {
        state: SectionState::Incomplete,
        complete: false,
        returned,
        omitted: 0,
        frontier,
        reason_refs: vec![diagnostic::EVIDENCE_UNKNOWN.to_owned()],
        provenance: vec![ProvenanceKind::CanonicalIr],
        confidence: Confidence::Unknown,
    }
}

fn unknown_summary() -> SectionSummary {
    SectionSummary {
        state: SectionState::Unknown,
        complete: false,
        returned: 0,
        omitted: 0,
        frontier: 0,
        reason_refs: vec![diagnostic::EVIDENCE_UNKNOWN.to_owned()],
        provenance: vec![ProvenanceKind::Evidence],
        confidence: Confidence::Unknown,
    }
}

fn unsupported_summary() -> SectionSummary {
    SectionSummary {
        state: SectionState::Unsupported,
        complete: false,
        returned: 0,
        omitted: 0,
        frontier: 0,
        reason_refs: vec![diagnostic::EVIDENCE_UNKNOWN.to_owned()],
        provenance: vec![ProvenanceKind::Evidence],
        confidence: Confidence::Unknown,
    }
}

fn collect_warnings(
    unresolved_entries: usize,
    effect_degraded: bool,
    public_truncated: bool,
    has_envelopes: bool,
) -> Vec<Diagnostic> {
    let mut warnings: Vec<Diagnostic> = Vec::new();
    if unresolved_entries > 0 {
        warnings.push(
            diagnostic::changed_input_incomplete_warning("unresolved-changed-inputs")
                .expect("registered warning"),
        );
    }
    if effect_degraded {
        warnings.push(
            diagnostic::warning(diagnostic::EFFECT_STALE, "stale-detected-evidence")
                .expect("registered warning"),
        );
    }
    if public_truncated || unresolved_entries > 0 {
        warnings.push(
            diagnostic::warning(
                diagnostic::PUBLIC_IMPACT_INCOMPLETE,
                "public-closure-limited",
            )
            .expect("registered warning"),
        );
    }
    if !has_envelopes {
        warnings.push(
            diagnostic::warning(
                diagnostic::EVIDENCE_UNKNOWN,
                "detected-evidence-unavailable",
            )
            .expect("registered warning"),
        );
    }
    warnings.push(
        diagnostic::warning(diagnostic::EVIDENCE_UNKNOWN, "manifests-unavailable")
            .expect("registered warning"),
    );
    warnings.push(
        diagnostic::warning(diagnostic::EVIDENCE_UNKNOWN, "tests-unsupported")
            .expect("registered warning"),
    );
    warnings
}

/// The collected semantic surface of the compiled project.
struct Surface {
    facts: HashMap<String, SymbolFact>,
    bindings: Vec<(NodeId, String, String)>,
}

impl Surface {
    fn collect(project: &CompiledProject, graph: &DependencyGraph) -> Self {
        let mut facts: HashMap<String, SymbolFact> = HashMap::new();
        let mut bindings: Vec<(NodeId, String, String)> = Vec::new();
        for definition in &project.definitions {
            let id = definition.id().as_str().to_owned();
            let mut fact = risk::fact_of(definition);
            if let Some(node) = graph.resolve_id(&id) {
                fact.subkind = graph.node(node).and_then(|node| node.subkind());
            }
            if let Definition::TargetBinding(binding) = definition {
                if let Some(node) = graph.resolve_id(&id) {
                    bindings.push((node.clone(), binding.target.as_str().to_owned(), id.clone()));
                }
            }
            facts.insert(id, fact);
        }
        // Rename history and tombstones from the 1.0.0 project registry.
        if let Some(registry) = project
            .project
            .as_ref()
            .and_then(|project| project.id_registry.as_ref())
        {
            for entry in &registry.rename_history {
                if let Some(fact) = facts.get_mut(entry.to.as_str()) {
                    fact.renamed_from.push(entry.from.as_str().to_owned());
                }
            }
            for tombstone in &registry.tombstones {
                let id = match tombstone {
                    Tombstone::Replaced { id, .. } => id.as_str(),
                    Tombstone::Deleted { id, .. } => id.as_str(),
                };
                if let Some(fact) = facts.get_mut(id) {
                    fact.tombstoned = true;
                }
            }
        }
        // The binding triple collapses to (node, target) at query time;
        // node resolution happens lazily through the graph.
        Self { facts, bindings }
    }

    fn fact(&self, node: &NodeId) -> SymbolFact {
        self.facts
            .get(node.semantic_id())
            .cloned()
            .unwrap_or(SymbolFact {
                kind: match node.as_str().split_once(':') {
                    Some((kind, _)) if matches!(kind, "project" | "module" | "requirement") => {
                        match kind {
                            "project" => "project",
                            "module" => "module",
                            _ => "requirement",
                        }
                    }
                    _ => "",
                },
                subkind: None,
                visibility: crate::ir::Visibility::Project,
                portability: None,
                renamed_from: Vec::new(),
                tombstoned: false,
            })
    }

    /// Whether one node is public or structural (always traversable).
    fn is_public(&self, node: &NodeId) -> bool {
        match self.facts.get(node.semantic_id()) {
            Some(fact) => risk::is_public(fact),
            None => true,
        }
    }

    fn bindings(&self) -> &[(NodeId, String, String)] {
        &self.bindings
    }
}
