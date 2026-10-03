//! Read-only architecture facts and change-radius advice (issue #77).
//! Filesystem, provider execution, scheduling, and refactoring stay with callers.
mod aggregation;
pub mod compare;
pub mod diagnostic;
pub mod evidence;
pub mod input;
pub mod profile;
mod projection;
pub mod wire;

use crate::{diagnostics::DiagnosticSet, ir::Compilation};
pub use evidence::Evidence;
pub use input::ChangeInput;
pub use profile::Profile;
use std::collections::{BTreeMap, BTreeSet};
use wire::{Finding, Report, State, Subject, Witness};

#[derive(Clone, Debug)]
pub enum Selection {
    Symbol(String),
    Module(String),
    All,
    Changed(crate::impact::ChangedInputSet),
}
#[derive(Clone, Debug)]
pub struct Request {
    pub selection: Selection,
    pub field: Option<String>,
    pub context_budget: Option<u64>,
    pub source_revision: Option<String>,
    pub model_digest: Option<String>,
}

fn count(n: usize) -> State<u64> {
    State::known(n as u64)
}
fn union(values: impl Iterator<Item = String>) -> Vec<String> {
    values.collect::<BTreeSet<_>>().into_iter().collect()
}
#[derive(Default)]
struct WitnessStore {
    rows: BTreeMap<String, Witness>,
    bytes: usize,
}
impl WitnessStore {
    fn insert(&mut self, witness: Witness) -> Result<String, DiagnosticSet> {
        let key = witness.id.clone();
        if !self.rows.contains_key(&key) {
            let size = serde_json::to_vec(&witness).expect("typed witness").len();
            if self.rows.len() >= 50_000 || self.bytes.saturating_add(size) > wire::MAX_BYTES {
                return Err(diagnostic::invalid("witness-size-bound"));
            }
            self.bytes += size;
            self.rows.insert(key.clone(), witness);
        }
        Ok(key)
    }
}
fn add_witness(
    walk: &projection::Walk,
    p: &projection::Projection,
    root: &str,
    id: &str,
    direction: &str,
    witnesses: &mut WitnessStore,
) -> Result<String, DiagnosticSet> {
    let edges = walk
        .paths
        .get(id)
        .into_iter()
        .flatten()
        .map(|i| p.edges[*i].clone())
        .collect::<Vec<_>>();
    let mut w = Witness {
        id: String::new(),
        root: root.into(),
        subject: id.into(),
        direction: direction.into(),
        edges,
        evidence_refs: vec![],
        confidence: "canonical".into(),
    };
    w.id = wire::digest(&w);
    witnesses.insert(w)
}
fn warn(
    findings: &mut Vec<Finding>,
    rule: &str,
    subject: &Subject,
    metric: &str,
    detail: &str,
    profile: &Profile,
) {
    let centrality = profile
        .centrality_declarations
        .iter()
        .find(|c| c.subject == subject.subject && c.rules.iter().any(|r| r == rule))
        .map(|c| State::known(c.review_ref.clone()))
        .unwrap_or(State::Unknown);
    findings.push(Finding {
        rule: rule.into(),
        subject: subject.subject.clone(),
        metric: metric.into(),
        detail: detail.into(),
        witness_refs: subject.witness_refs.clone(),
        centrality,
    });
}
pub fn analyze(
    request: &Request,
    profile: &Profile,
    compilation: &Compilation,
    evidence: Option<&Evidence>,
) -> Result<Report, DiagnosticSet> {
    profile.validate()?;
    let project = &compilation.project;
    let graph = crate::graph::build(project)?;
    let effects = crate::effects::build(project)?;
    let p = projection::Projection::new(project, &graph);
    let semantic = crate::diff::projection::project(project)
        .map_err(|_| diagnostic::invalid("semantic-size"))?;
    let pins = wire::Pins {
        project: graph.project_id().unwrap_or("anonymous").into(),
        model_version: project.model_version.as_str().into(),
        source_revision: request
            .source_revision
            .clone()
            .map(State::known)
            .unwrap_or(State::Unknown),
        model_digest: request
            .model_digest
            .clone()
            .map(State::known)
            .unwrap_or(State::Unknown),
        ir_digest: format!(
            "sha256:{}",
            crate::digest::sha256_hex(project.to_canonical_json().as_bytes())
        ),
        semantic_digest: format!(
            "sha256:{}",
            crate::digest::sha256_hex(
                crate::diff::projection::canonical_bytes(&semantic).as_bytes()
            )
        ),
        graph_digest: format!(
            "sha256:{}",
            crate::digest::sha256_hex(graph.to_canonical_json()?.as_bytes())
        ),
        effect_digest: format!(
            "sha256:{}",
            crate::digest::sha256_hex(effects.to_canonical_json()?.as_bytes())
        ),
    };
    if request
        .source_revision
        .as_ref()
        .is_some_and(|s| !crate::trace::id::is_revision(s))
    {
        return Err(diagnostic::invalid("source-revision"));
    }
    if request
        .model_digest
        .as_ref()
        .is_some_and(|s| !wire::is_digest(s))
    {
        return Err(diagnostic::invalid("model-digest"));
    }
    for m in profile
        .modules
        .iter()
        .map(|m| &m.module)
        .chain(profile.domain_groups.iter().flat_map(|g| &g.modules))
    {
        if !project.modules.iter().any(|n| n.id.as_str() == m) {
            return Err(diagnostic::invalid("profile-module-id"));
        }
    }
    for c in &profile.centrality_declarations {
        let owner = c.subject.split('#').next().unwrap_or_default();
        if !p.nodes.contains_key(owner) {
            return Err(diagnostic::invalid("centrality-subject"));
        }
    }
    let joined = match evidence {
        Some(e) => e.join(&pins, project, &effects)?,
        None => evidence::Joined::default(),
    };
    let mut changes_complete = true;
    let mut changed_pin = State::Unknown;
    let mut requested_fields = BTreeMap::<String, BTreeSet<String>>::new();
    let (scope_kind, roots) = match &request.selection {
        Selection::Symbol(s) => {
            let id = graph
                .resolve_id(s)
                .ok_or_else(|| diagnostic::invalid("symbol-missing-or-ambiguous"))?
                .as_str()
                .to_owned();
            if matches!(p.nodes[&id].class, "unclassified") {
                return Err(diagnostic::invalid("symbol-kind"));
            }
            ("symbol", vec![id])
        }
        Selection::Module(module) => {
            if !project.modules.iter().any(|m| m.id.as_str() == module) {
                return Err(diagnostic::invalid("module-missing"));
            }
            (
                "module",
                p.nodes
                    .iter()
                    .filter(|(_, n)| n.module == *module && n.class != "unclassified")
                    .map(|(id, _)| id.clone())
                    .collect(),
            )
        }
        Selection::All => (
            "all",
            p.nodes
                .iter()
                .filter(|(_, n)| n.class != "unclassified")
                .map(|(id, _)| id.clone())
                .collect(),
        ),
        Selection::Changed(changes) => {
            changed_pin = State::known(changes.digest().into());
            let mut roots = BTreeSet::new();
            for entry in changes.entries() {
                changes_complete &= matches!(
                    entry.evidence(),
                    crate::impact::EntryEvidence::Canonical
                        | crate::impact::EntryEvidence::Verified
                );
                for s in entry.symbol_ids() {
                    match graph.resolve_id(s) {
                        Some(id) => {
                            roots.insert(id.as_str().to_owned());
                        }
                        None => changes_complete = false,
                    }
                }
                for seed in entry.member_seeds() {
                    if let Some(id) = graph.resolve_id(seed.entity()) {
                        roots.insert(id.as_str().to_owned());
                        requested_fields
                            .entry(id.as_str().to_owned())
                            .or_default()
                            .insert(seed.field().into());
                    } else {
                        changes_complete = false;
                    }
                }
            }
            ("changed", roots.into_iter().collect())
        }
    };
    if let Some(field) = &request.field {
        if scope_kind != "symbol" {
            return Err(diagnostic::invalid("field-requires-symbol"));
        }
        requested_fields
            .entry(roots[0].clone())
            .or_default()
            .insert(field.clone());
    }
    let mut subjects_to_measure = vec![];
    for root in &roots {
        let selected = requested_fields.get(root);
        if selected.is_none() {
            subjects_to_measure.push((root.clone(), None));
        }
        let fields = p.fields.get(root);
        if let Some(selected) = selected {
            for field in selected {
                if !fields.is_some_and(|fs| fs.iter().any(|(f, _)| f == field)) {
                    return Err(diagnostic::invalid("field-missing"));
                }
                subjects_to_measure.push((root.clone(), Some(field.clone())));
            }
        } else if matches!(request.selection, Selection::All | Selection::Module(_)) {
            for (field, _) in fields.into_iter().flatten() {
                subjects_to_measure.push((root.clone(), Some(field.clone())));
            }
        }
    }
    if subjects_to_measure.len() > wire::MAX_SUBJECTS {
        return Err(diagnostic::invalid("subject-limit"));
    }
    let cycles = p.cycles();
    let extra_per_subject = effects.declared().len()
        + cycles
            .iter()
            .map(|c| c.members.len() + c.modules.len() + c.edge_keys.len())
            .sum::<usize>()
        + joined.artifacts.as_ref().map_or(0, |m| m.artifacts().len())
        + joined
            .transactions
            .as_ref()
            .map_or(0, |t| t.group_membership().len());
    let extras = subjects_to_measure
        .len()
        .checked_mul(extra_per_subject)
        .ok_or_else(|| diagnostic::invalid("analysis-work-bound"))?;
    if extras + p.edges.len() > 1_000_000 {
        return Err(diagnostic::invalid("analysis-work-bound"));
    }
    let mut work = p.edges.len() + extras;
    let mut witnesses = WitnessStore::default();
    let mut row_bytes = serde_json::to_vec(&p.snapshot())
        .expect("typed projection")
        .len();
    let mut rows = vec![];
    let mut findings = vec![];
    let mut writer_resources = BTreeMap::<String, BTreeMap<String, Vec<String>>>::new();
    for edge in effects.declared() {
        if matches!(
            edge.key().kind(),
            crate::effects::EffectKind::Create
                | crate::effects::EffectKind::Update
                | crate::effects::EffectKind::Delete
                | crate::effects::EffectKind::WriteField { .. }
                | crate::effects::EffectKind::CacheWrite
                | crate::effects::EffectKind::CacheInvalidate
        ) {
            let resource = edge.key().subject().resource().as_str();
            let field = edge
                .key()
                .subject()
                .field()
                .map(|f| format!("#{}", f.as_str()))
                .unwrap_or_default();
            writer_resources
                .entry(format!("{resource}{field}"))
                .or_default()
                .entry(edge.key().operation().as_str().into())
                .or_default()
                .push(edge.key().to_canonical_string());
        }
    }
    for (root, field) in subjects_to_measure {
        let reverse = p.walk(&root, true, &mut work);
        let forward = p.walk(&root, false, &mut work);
        let complete = reverse.complete && changes_complete && field.is_none();
        let mut exact: BTreeSet<String> = reverse.paths.keys().cloned().collect();
        let mut possible = vec![];
        let mut refs = vec![];
        let mut gaps = vec![];
        let fan_in = union(
            p.incoming
                .get(&root)
                .into_iter()
                .flatten()
                .map(|i| p.edges[*i].from.clone())
                .filter(|s| s != &root),
        );
        let mut fan_out = union(
            p.outgoing
                .get(&root)
                .into_iter()
                .flatten()
                .map(|i| p.edges[*i].to.clone())
                .filter(|s| s != &root),
        );
        if let Some(field) = &field {
            possible = reverse
                .paths
                .keys()
                .filter(|s| *s != &root)
                .cloned()
                .collect();
            exact = BTreeSet::from([root.clone()]);
            fan_out = p.fields[&root]
                .iter()
                .find(|(f, _)| f == field)
                .map(|(_, rs)| union(rs.iter().filter(|s| *s != &root).cloned()))
                .unwrap_or_default();
            // Current typed query uses prove these consumers, but unrepresented whole
            // record consumers remain possible. Never call this complete field impact.
            if let Some(queries) = &joined.queries {
                for q in queries.queries() {
                    if q.source.as_str()
                        == root.split_once(':').map(|(_, id)| id).unwrap_or_default()
                        && evidence::query_fields(q).contains(field)
                    {
                        if let Some(id) = graph.resolve_id(q.query.as_str()) {
                            exact.insert(id.as_str().into());
                        }
                    }
                }
            }
            possible.retain(|s| !exact.contains(s));
            gaps.push("field-owner-conservative".into());
        }
        if !reverse.complete || !forward.complete {
            gaps.push("traversal-bound".into());
        }
        if !changes_complete {
            gaps.push("changed-input-incomplete".into());
        }
        for id in exact.iter().chain(&possible) {
            refs.push(add_witness(
                &reverse,
                &p,
                &root,
                id,
                "reverse",
                &mut witnesses,
            )?);
        }
        for id in &fan_out {
            if forward.paths.contains_key(id) {
                refs.push(add_witness(
                    &forward,
                    &p,
                    &root,
                    id,
                    "forward",
                    &mut witnesses,
                )?);
            }
        }
        let partition = p.partition(&exact);
        let affected_modules = p.modules(exact.iter());
        let mut metrics: BTreeMap<String, State<u64>> = wire::METRICS
            .iter()
            .map(|m| ((*m).into(), State::Unknown))
            .collect();
        metrics.insert(
            "fanInSymbols".into(),
            if field.is_none() {
                count(fan_in.len())
            } else {
                State::Unknown
            },
        );
        metrics.insert("fanOutSymbols".into(), count(fan_out.len()));
        metrics.insert(
            "fanInModules".into(),
            if field.is_none() {
                count(
                    p.modules(fan_in.iter())
                        .iter()
                        .filter(|m| **m != p.nodes[&root].module)
                        .count(),
                )
            } else {
                State::Unknown
            },
        );
        metrics.insert(
            "fanOutModules".into(),
            count(
                p.modules(fan_out.iter())
                    .iter()
                    .filter(|m| **m != p.nodes[&root].module)
                    .count(),
            ),
        );
        let mut lower_bounds = BTreeMap::new();
        for (name, n) in [
            ("publicContractsAffected", partition.public_contracts.len()),
            ("internalSymbolsAffected", partition.internal_symbols.len()),
            ("semanticSymbolsAffected", exact.len()),
            ("modulesAffected", affected_modules.len()),
        ] {
            metrics.insert(
                name.into(),
                if complete { count(n) } else { State::Unknown },
            );
            if !complete && field.is_none() {
                lower_bounds.insert(name.into(), n as u64);
            }
        }
        let relevant_cycles = cycles
            .iter()
            .filter(|c| {
                if c.series == "symbols" {
                    c.members.iter().any(|m| exact.contains(m))
                } else {
                    c.modules.iter().any(|m| affected_modules.contains(m))
                }
            })
            .count();
        metrics.insert("crossModuleCycles".into(), count(relevant_cycles));
        let mut resources = vec![];
        let mut shared_resources = vec![];
        let mut effect_refs = vec![];
        for (resource, writers) in &writer_resources {
            let matches_root = root
                .split_once(':')
                .is_some_and(|(_, id)| resource == id || resource.starts_with(&format!("{id}#")));
            if matches_root || writers.keys().any(|op| exact.contains(op)) {
                resources.push(resource.clone());
                if writers.len() > 1 {
                    shared_resources.push(resource.clone());
                }
                effect_refs.extend(writers.values().flatten().cloned());
            }
        }
        metrics.insert(
            "sharedMutableResources".into(),
            if field.is_none() {
                count(shared_resources.len())
            } else {
                State::Unknown
            },
        );
        if !effect_refs.is_empty() {
            let mut w = Witness {
                id: String::new(),
                root: root.clone(),
                subject: root.clone(),
                direction: "effects".into(),
                edges: vec![],
                evidence_refs: union(effect_refs.into_iter()),
                confidence: "canonical".into(),
            };
            w.id = wire::digest(&w);
            refs.push(w.id.clone());
            witnesses.insert(w)?;
        }
        let mut artifacts = vec![];
        let mut targets = vec![];
        let mut tests = vec![];
        let mut checks = vec![];
        if let Some(m) = &joined.artifacts {
            for a in m.artifacts() {
                let owners = a
                    .input_refs()
                    .iter()
                    .map(|s| s.as_str())
                    .chain(std::iter::once(a.key().semantic_owner().as_str()));
                if owners.into_iter().any(|s| {
                    graph
                        .resolve_id(s)
                        .is_some_and(|id| exact.contains(id.as_str()))
                }) {
                    artifacts.push(format!(
                        "{}|{}|{}",
                        a.key().semantic_owner().as_str(),
                        a.key().kind().as_str(),
                        a.key().path().as_str()
                    ));
                    // An adapter is a producer, not a target identity. Never count it as a target.
                }
            }
            metrics.insert(
                "affectedArtifacts".into(),
                if complete {
                    count(artifacts.len())
                } else {
                    State::Unknown
                },
            );
        }
        metrics.insert("affectedTargets".into(), State::Unsupported);
        if let Some(t) = &joined.trace {
            let m = t.manifest();
            let by_id: BTreeMap<_, _> = m.nodes.iter().map(|n| (&n.node_id, n)).collect();
            let selected: BTreeSet<_> = m
                .nodes
                .iter()
                .filter(|n| {
                    n.semantic_id.as_ref().is_some_and(|s| {
                        graph
                            .resolve_id(s)
                            .is_some_and(|id| exact.contains(id.as_str()))
                    })
                })
                .map(|n| n.node_id.clone())
                .collect();
            let mut reachable = selected;
            let mut progress = true;
            while progress {
                progress = false;
                for rel in &m.relations {
                    work += 1;
                    if work > 1_000_000 {
                        break;
                    }
                    if rel.status == crate::trace::provenance::Status::Confirmed
                        && matches!(
                            rel.confidence,
                            crate::trace::provenance::Confidence::Exact
                                | crate::trace::provenance::Confidence::High
                        )
                        && matches!(
                            rel.relation_kind,
                            crate::trace::relation::RelationKind::Verifies
                                | crate::trace::relation::RelationKind::Covers
                                | crate::trace::relation::RelationKind::Evidences
                        )
                        && reachable.contains(&rel.to_node)
                    {
                        progress |= reachable.insert(rel.from_node.clone());
                    }
                }
                if work > 1_000_000 {
                    break;
                }
            }
            for id in reachable {
                let n = by_id[&id];
                match n.node_kind {
                    crate::trace::node::NodeKind::NativeTest => {
                        tests.push(n.test_id.clone().unwrap_or_default())
                    }
                    crate::trace::node::NodeKind::Gate => {
                        checks.push(n.gate_id.clone().unwrap_or_default())
                    }
                    _ => {}
                }
            }
            let trace_complete = complete
                && m.completeness == crate::trace::completeness::Completeness::Full
                && pins.source_revision.value() == Some(&m.source_revision)
                && work <= 1_000_000;
            metrics.insert(
                "affectedTests".into(),
                if trace_complete {
                    count(tests.len())
                } else {
                    State::Unknown
                },
            );
            metrics.insert(
                "requiredChecks".into(),
                if trace_complete {
                    count(checks.len())
                } else {
                    State::Unknown
                },
            );
            if !trace_complete {
                gaps.push("trace-incomplete".into());
            }
        }
        let mut transaction_groups = vec![];
        let mut transaction_scopes = vec![];
        if let Some(tx) = &joined.transactions {
            let members = tx.group_membership();
            transaction_groups = union(
                members
                    .iter()
                    .filter(|m| {
                        graph
                            .resolve_id(&m.operation)
                            .is_some_and(|id| exact.contains(id.as_str()))
                    })
                    .map(|m| m.group_id.clone()),
            );
            for group in &transaction_groups {
                let group_effects: BTreeSet<_> = members
                    .iter()
                    .filter(|m| &m.group_id == group)
                    .map(|m| m.effect.as_str())
                    .collect();
                let related_modules = union(
                    members
                        .iter()
                        .filter(|m| &m.group_id == group)
                        .filter_map(|m| graph.resolve_id(&m.operation))
                        .filter_map(|id| p.nodes.get(id.as_str()))
                        .map(|n| n.module.clone())
                        .chain(
                            effects
                                .declared()
                                .iter()
                                .filter(|e| {
                                    group_effects.contains(e.key().to_canonical_string().as_str())
                                })
                                .filter_map(|e| {
                                    graph.resolve_id(e.key().subject().resource().as_str())
                                })
                                .filter_map(|id| p.nodes.get(id.as_str()))
                                .map(|n| n.module.clone()),
                        ),
                );
                let classified = !profile.domain_groups.is_empty()
                    && related_modules
                        .iter()
                        .all(|m| profile.domain_groups.iter().any(|g| g.modules.contains(m)));
                let domain_groups = profile
                    .domain_groups
                    .iter()
                    .filter(|g| g.modules.iter().any(|m| related_modules.contains(m)))
                    .map(|g| g.id.clone())
                    .collect::<Vec<_>>();
                transaction_scopes.push(wire::TransactionScope {
                    group: group.clone(),
                    modules: related_modules,
                    domain_groups: union(domain_groups.into_iter()),
                    classified,
                });
            }
            if field.is_none()
                && complete
                && !profile.domain_groups.is_empty()
                && transaction_scopes.iter().all(|s| s.classified)
            {
                metrics.insert(
                    "transactionSpread".into(),
                    count(
                        transaction_scopes
                            .iter()
                            .filter(|s| s.domain_groups.len() > 1)
                            .count(),
                    ),
                );
            }
        }
        metrics.insert(
            "sharedAbstractionRadius".into(),
            if root.starts_with("type:") && complete {
                count(
                    partition
                        .public_contracts
                        .len()
                        .saturating_sub(usize::from(partition.public_contracts.contains(&root))),
                )
            } else if root.starts_with("type:") {
                State::Unknown
            } else {
                count(0)
            },
        );
        let exposure = if p.nodes[&root].class == "public" {
            forward
                .paths
                .keys()
                .filter(|id| *id != &root && p.nodes[*id].target_specific)
                .cloned()
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        for id in &exposure {
            refs.push(add_witness(
                &forward,
                &p,
                &root,
                id,
                "forward",
                &mut witnesses,
            )?);
        }
        metrics.insert(
            "publicTargetExposure".into(),
            if forward.complete && field.is_none() {
                count(exposure.len())
            } else {
                State::Unknown
            },
        );
        metrics.insert(
            "duplicationDivergence".into(),
            if joined.replica_subjects.contains(&root) {
                count(joined.replica_divergences.get(&root).map_or(0, Vec::len))
            } else {
                State::Unsupported
            },
        );
        targets.sort();
        artifacts.sort();
        tests.sort();
        checks.sort();
        let subject = Subject {
            subject: field
                .as_ref()
                .map(|f| format!("{root}#field.{f}"))
                .unwrap_or(root.clone()),
            module: p.nodes[&root].module.clone(),
            measurement_basis: if field.is_some() {
                "owner-conservative"
            } else {
                "exact-declared"
            }
            .into(),
            metrics,
            impact: partition,
            possible,
            lower_bounds,
            fan_in,
            fan_out,
            affected_modules,
            artifacts: union(artifacts.into_iter()),
            tests: union(tests.into_iter()),
            targets,
            checks: union(checks.into_iter()),
            resources,
            shared_resources,
            target_exposure: exposure,
            replica_obligations: joined
                .replica_divergences
                .get(&root)
                .cloned()
                .unwrap_or_default(),
            transaction_groups,
            transaction_scopes,
            witness_refs: union(refs.into_iter()),
            gaps: union(gaps.into_iter()),
            import_edge_keys: vec![],
        };
        for (metric, rule) in [
            ("crossModuleCycles", "coupling.cross-module-cycle"),
            ("sharedMutableResources", "coupling.shared-mutable-state"),
            ("transactionSpread", "coupling.transaction-spread"),
            ("publicTargetExposure", "coupling.public-target-exposure"),
            ("duplicationDivergence", "coupling.duplication-divergence"),
        ] {
            if subject.metrics[metric].value().is_some_and(|n| *n > 0) {
                warn(
                    &mut findings,
                    rule,
                    &subject,
                    metric,
                    "declared-risk-evidence",
                    profile,
                );
            }
        }
        for limit in profile.limits(&subject.module).limits {
            if subject.metrics[&limit.metric]
                .value()
                .is_some_and(|n| *n > limit.maximum)
            {
                let rule = match limit.metric.as_str() {
                    "fanInSymbols" | "fanOutSymbols" | "fanInModules" | "fanOutModules" => {
                        "coupling.fan-exceeded"
                    }
                    "publicContractsAffected" => "coupling.public-contract-amplification",
                    "sharedAbstractionRadius" => "coupling.shared-abstraction-radius",
                    _ => "coupling.change-amplification",
                };
                warn(
                    &mut findings,
                    rule,
                    &subject,
                    &limit.metric,
                    "configured-threshold-exceeded",
                    profile,
                );
            }
        }
        let required = profile.limits(&subject.module);
        if !subject.gaps.is_empty()
            || required
                .limits
                .iter()
                .any(|l| subject.metrics[&l.metric].value().is_none())
            || required
                .regression_limits
                .iter()
                .any(|l| subject.metrics[&l.metric].value().is_none())
        {
            warn(
                &mut findings,
                "coupling.evidence-incomplete",
                &subject,
                "semanticSymbolsAffected",
                "required-evidence-incomplete",
                profile,
            );
        }
        row_bytes =
            row_bytes.saturating_add(serde_json::to_vec(&subject).expect("typed subject").len());
        if row_bytes.saturating_add(witnesses.bytes) > wire::MAX_BYTES {
            return Err(diagnostic::invalid("report-size-bound"));
        }
        rows.push(subject);
    }
    aggregation::append(&mut rows, &p, &graph, &cycles);
    if rows.len() > wire::MAX_SUBJECTS {
        return Err(diagnostic::invalid("aggregate-subject-limit"));
    }
    for s in rows
        .iter()
        .filter(|s| s.measurement_basis == "aggregate-declared")
    {
        for limit in profile.limits(&s.module).limits {
            if s.metrics[&limit.metric]
                .value()
                .is_some_and(|n| *n > limit.maximum)
            {
                let rule = if limit.metric.starts_with("fan") {
                    "coupling.fan-exceeded"
                } else if limit.metric == "publicContractsAffected" {
                    "coupling.public-contract-amplification"
                } else {
                    "coupling.change-amplification"
                };
                warn(
                    &mut findings,
                    rule,
                    s,
                    &limit.metric,
                    "configured-threshold-exceeded",
                    profile,
                );
            }
        }
    }
    rows.sort_by(|a, b| a.subject.cmp(&b.subject));
    let summary = p.partition(&rows.iter().flat_map(|s| s.impact.ids().cloned()).collect());
    let all_gaps = union(
        rows.iter()
            .flat_map(|s| s.gaps.clone())
            .chain(if changes_complete {
                vec![]
            } else {
                vec!["changed-input-incomplete".into()]
            })
            .chain(if profile.measurement.include_observed {
                vec!["observed-projection-unsupported".into()]
            } else {
                vec![]
            }),
    );
    let budget = if let Some(tokens) = request.context_budget {
        let req = crate::context_budget::BudgetRequest::new(
            match &request.selection {
                Selection::Symbol(s) => Some(s.clone()),
                _ => None,
            },
            match &request.selection {
                Selection::Module(m) => Some(m.clone()),
                _ => None,
            },
            matches!(request.selection, Selection::All | Selection::Changed(_)),
            true,
            false,
            false,
        )?;
        State::known(
            crate::context_budget::plan(
                &req,
                &crate::context_budget::BudgetSelection::Generic(tokens),
                compilation,
            )?
            .to_canonical_json()?,
        )
    } else {
        State::Unknown
    };
    let conflicts = if matches!(request.selection, Selection::Changed(_)) {
        let operations = roots
            .iter()
            .filter_map(|s| crate::effects::OperationId::from_qualified(s))
            .filter(|op| effects.knows_operation(op))
            .collect::<Vec<_>>();
        if operations.is_empty() || !changes_complete {
            State::Unknown
        } else {
            let report =
                effects.conflicts(&crate::effects::conflict::ChangeSet::new(operations)?)?;
            let items=report.items().iter().map(|i|serde_json::json!({"classification":i.classification().key(),"left":i.left().as_str(),"right":i.right().as_str(),"subject":i.subject(),"leftKey":i.left_key(),"rightKey":i.right_key()})).collect::<Vec<_>>();
            State::known(serde_json::json!({"complete":report.complete(),"items":items,"coverage":"declared-only"}).to_string())
        }
    } else {
        State::Unknown
    };
    let planning = wire::Planning {
        public_contracts: summary.public_contracts.clone(),
        internal_symbols: summary.internal_symbols.clone(),
        resources: union(rows.iter().flat_map(|s| s.resources.clone())),
        transaction_groups: union(rows.iter().flat_map(|s| s.transaction_groups.clone())),
        required_checks: union(rows.iter().flat_map(|s| s.checks.clone())),
        evidence_gaps: all_gaps.clone(),
        context_budget: budget,
        runtime_conflicts: conflicts,
        review_overlap: summary.public_contracts.clone(),
    };
    let coverage = BTreeMap::from([
        ("declared".into(), State::known("canonical-ir".into())),
        ("observed".into(), State::Unsupported),
        (
            "artifacts".into(),
            if joined.artifacts.is_some() {
                State::known("pinned-manifest".into())
            } else {
                State::Unknown
            },
        ),
        (
            "tests".into(),
            if joined.trace.is_some() {
                State::known("pinned-trace".into())
            } else {
                State::Unknown
            },
        ),
        (
            "members".into(),
            if joined.queries.is_some() {
                State::known("pinned-query-model-partial".into())
            } else {
                State::Unknown
            },
        ),
        (
            "transactions".into(),
            if joined.transactions.is_some() {
                State::known("pinned-transaction-concurrency".into())
            } else {
                State::Unknown
            },
        ),
    ]);
    let suggestions = rows
        .iter()
        .filter(|s| findings.iter().any(|f| f.subject == s.subject) && !s.witness_refs.is_empty())
        .take(16)
        .map(|s| wire::Suggestion {
            subject: s.subject.clone(),
            action: "review-consumer-projection-or-boundary-cut".into(),
            witness_refs: s.witness_refs.clone(),
            preserve: vec![
                "semantic-identity".into(),
                "public-contract-compatibility".into(),
                "types-nullability-presence".into(),
                "policy-errors".into(),
                "effects-transaction-boundaries".into(),
                "invariants-ownership".into(),
            ],
            applied: false,
        })
        .collect();
    findings
        .sort_by(|a, b| (&a.rule, &a.subject, &a.metric).cmp(&(&b.rule, &b.subject, &b.metric)));
    let report = Report {
        schema_version: format!("lekalo/coupling-report/v{}", wire::VERSION),
        identity: format!("dev.lekalo.coupling-report@{}", wire::VERSION),
        metric_version: wire::METRIC_VERSION.into(),
        scope: wire::Scope {
            kind: scope_kind.into(),
            selector: match &request.selection {
                Selection::Symbol(_) => roots.first().cloned().unwrap_or_default(),
                Selection::Module(m) => m.clone(),
                Selection::All => "all".into(),
                Selection::Changed(_) => "changed".into(),
            },
            roots,
            changed_input: changed_pin,
        },
        recipe: wire::RECIPE.into(),
        profile: profile.clone(),
        provenance: wire::Provenance {
            inputs: pins,
            evidence: joined.digest,
            profile_digest: wire::digest(profile),
            measurement_digest: wire::digest(
                &serde_json::json!({"measurement":profile.measurement,"domainGroups":profile.domain_groups,"classification":"project-visible-contracts/1"}),
            ),
            projection_digest: wire::digest(&p.snapshot()),
        },
        projection: p.snapshot(),
        coverage,
        subjects: rows,
        public_contract_counts: ["type", "entity", "operation", "event", "endpoint"]
            .into_iter()
            .map(|kind| {
                (
                    kind.to_owned(),
                    summary
                        .public_contracts
                        .iter()
                        .filter(|id| id.starts_with(&format!("{kind}:")))
                        .count() as u64,
                )
            })
            .collect(),
        summary,
        cycles,
        witnesses: witnesses.rows.into_values().collect(),
        findings,
        suggestions,
        planning,
        complete: all_gaps.is_empty(),
    };
    if serde_json::to_vec(&report).expect("typed report").len() > wire::MAX_BYTES {
        return Err(diagnostic::invalid("report-size"));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_map_relayout_does_not_change_semantic_measurements() {
        let model = crate::loader::normalize_model(&crate::loader::LoadSelection {
            project: Some("tests/fixtures/coupling/planner".into()),
        })
        .unwrap();
        let mut compiled = crate::ir::compile(&model).unwrap();
        let request = Request {
            selection: Selection::Symbol("planner.task".into()),
            field: None,
            context_budget: None,
            source_revision: None,
            model_digest: None,
        };
        let before = analyze(&request, &Profile::default(), &compiled, None).unwrap();
        let entries = compiled
            .source_map
            .entries()
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, mut entry)| {
                entry.path = format!("relocated/part-{}.yaml", i % 2);
                entry.start.byte += 100;
                entry.end.byte += 100;
                entry
            })
            .collect();
        compiled.source_map = crate::ir::SourceMap::new(entries);
        assert_eq!(
            before,
            analyze(&request, &Profile::default(), &compiled, None).unwrap()
        );
    }
}
