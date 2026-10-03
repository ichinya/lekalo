//! Module/project summaries are unions; overlapping root closures are never summed.
use super::{
    projection::Projection,
    wire::{self, State, Subject},
};
use crate::graph::{DependencyGraph, EdgeProvenance, ReferenceRole};
use std::collections::{BTreeMap, BTreeSet};
fn union(rows: &[&Subject], f: impl Fn(&Subject) -> &Vec<String>) -> Vec<String> {
    rows.iter()
        .flat_map(|s| f(s).iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
pub fn append(
    rows: &mut Vec<Subject>,
    p: &Projection,
    graph: &DependencyGraph,
    cycles: &[wire::Cycle],
) {
    let mut scopes: BTreeMap<String, Vec<&Subject>> = BTreeMap::new();
    let owners: BTreeSet<_> = rows
        .iter()
        .filter(|s| s.measurement_basis == "exact-declared")
        .map(|s| s.subject.as_str())
        .collect();
    for s in rows.iter() {
        if s.measurement_basis == "owner-conservative"
            && owners.contains(s.subject.split('#').next().unwrap_or_default())
        {
            continue;
        }
        scopes
            .entry(format!("module:{}", s.module))
            .or_default()
            .push(s);
        scopes
            .entry(format!(
                "project:{}",
                graph.project_id().unwrap_or("anonymous")
            ))
            .or_default()
            .push(s);
    }
    let mut summaries = vec![];
    for (id, selected) in scopes {
        let roots: BTreeSet<_> = selected
            .iter()
            .map(|s| s.subject.split('#').next().unwrap_or_default().to_owned())
            .collect();
        let ids: BTreeSet<_> = selected
            .iter()
            .flat_map(|s| s.impact.ids().cloned())
            .collect();
        let impact = p.partition(&ids);
        let fan_in = union(&selected, |s| &s.fan_in)
            .into_iter()
            .filter(|s| !roots.contains(s))
            .collect::<Vec<_>>();
        let fan_out = union(&selected, |s| &s.fan_out)
            .into_iter()
            .filter(|s| !roots.contains(s))
            .collect::<Vec<_>>();
        let own_modules = p.modules(roots.iter());
        let affected_modules = p.modules(ids.iter());
        let artifacts = union(&selected, |s| &s.artifacts);
        let tests = union(&selected, |s| &s.tests);
        let targets = union(&selected, |s| &s.targets);
        let checks = union(&selected, |s| &s.checks);
        let resources = union(&selected, |s| &s.resources);
        let shared_resources = union(&selected, |s| &s.shared_resources);
        let mut metrics = wire::METRICS
            .iter()
            .map(|m| ((*m).to_owned(), State::Unknown))
            .collect::<BTreeMap<_, _>>();
        for (name, n) in [
            ("fanInSymbols", fan_in.len()),
            ("fanOutSymbols", fan_out.len()),
            (
                "fanInModules",
                p.modules(fan_in.iter())
                    .iter()
                    .filter(|m| !own_modules.contains(m))
                    .count(),
            ),
            (
                "fanOutModules",
                p.modules(fan_out.iter())
                    .iter()
                    .filter(|m| !own_modules.contains(m))
                    .count(),
            ),
            ("publicContractsAffected", impact.public_contracts.len()),
            ("internalSymbolsAffected", impact.internal_symbols.len()),
            ("semanticSymbolsAffected", ids.len()),
            ("modulesAffected", affected_modules.len()),
            ("affectedArtifacts", artifacts.len()),
            ("affectedTests", tests.len()),
            ("affectedTargets", targets.len()),
            ("requiredChecks", checks.len()),
            ("sharedMutableResources", shared_resources.len()),
        ] {
            if selected.iter().all(|s| s.metrics[name].value().is_some()) {
                metrics.insert(name.into(), State::known(n as u64));
            }
        }
        metrics.insert(
            "crossModuleCycles".into(),
            State::known(
                cycles
                    .iter()
                    .filter(|c| {
                        if c.series == "symbols" {
                            c.members.iter().any(|m| ids.contains(m))
                        } else {
                            c.modules.iter().any(|m| affected_modules.contains(m))
                        }
                    })
                    .count() as u64,
            ),
        );
        let module = id.strip_prefix("module:").unwrap_or_default().to_owned();
        let gaps = union(&selected, |s| &s.gaps);
        let imports = graph
            .edges()
            .iter()
            .filter(|e| {
                matches!(
                    e.provenance(),
                    EdgeProvenance::CanonicalIr {
                        reference_role: ReferenceRole::ModuleImport,
                        ..
                    }
                )
            })
            .filter(|e| {
                own_modules
                    .iter()
                    .any(|m| e.key().from().semantic_id() == m || e.key().to().semantic_id() == m)
            })
            .map(|e| e.key().to_canonical_string())
            .collect::<BTreeSet<_>>();
        summaries.push(Subject {
            subject: id,
            module,
            measurement_basis: "aggregate-declared".into(),
            metrics,
            impact,
            possible: union(&selected, |s| &s.possible)
                .into_iter()
                .filter(|s| !ids.contains(s))
                .collect(),
            lower_bounds: BTreeMap::new(),
            fan_in,
            fan_out,
            affected_modules,
            artifacts,
            tests,
            targets,
            checks,
            resources,
            shared_resources,
            target_exposure: union(&selected, |s| &s.target_exposure),
            replica_obligations: union(&selected, |s| &s.replica_obligations),
            transaction_groups: union(&selected, |s| &s.transaction_groups),
            transaction_scopes: selected
                .iter()
                .flat_map(|s| s.transaction_scopes.iter().cloned())
                .map(|s| (s.group.clone(), s))
                .collect::<BTreeMap<_, _>>()
                .into_values()
                .collect(),
            witness_refs: union(&selected, |s| &s.witness_refs),
            gaps,
            import_edge_keys: imports.into_iter().collect(),
        });
    }
    rows.extend(summaries);
}
