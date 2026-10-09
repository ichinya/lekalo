//! The boundary check: binding validation, export closure, undeclared
//! imports, cross-owner writes, and cycle witnesses (issue #113).
//!
//! `check` is the pure core of `module api check` and the `validate
//! --module-api` seam. Advisory mode records well-formed boundary findings
//! as evidence; strict mode rejects exact semantic violations. Malformed
//! inputs always fail invalid; policy verdicts ride the dedicated denied
//! seam of the caller. The report retains every finding and witness so a
//! denied run never hides its rows.

use super::diagnostic::Evidence;
use super::effects;
use super::graph;
use super::public::export_public;
use super::resolve::{resolve, Resolved};
use super::wire::{self, ModuleSummary, PublicBundle, Report};
use crate::diagnostics::DiagnosticSet;
use crate::ir::{Compilation, Portability, Visibility};

/// One boundary finding classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingClass {
    /// Malformed input: always invalid regardless of mode.
    Invalid,
    /// A semantic boundary violation: advisory in default mode, invalid
    /// under strict.
    Boundary,
    /// A required evidence gap: advisory `indeterminate`, unavailable when
    /// required by the command.
    Evidence,
}

/// The typed verdict of one boundary run.
pub struct Verdict {
    pub report: Report,
    pub evidence: Vec<Evidence>,
    /// The most severe classification among the recorded findings.
    pub class: FindingClass,
}

/// Run the complete boundary check over one compilation plus attachment.
pub fn check(
    attachment_bytes: &[u8],
    compilation: &Compilation,
    strict: bool,
    module_filter: Option<&str>,
) -> Result<Verdict, DiagnosticSet> {
    // An unknown --module scope matches nothing and would silently pass;
    // the scope must name a real project module.
    if let Some(scope) = module_filter {
        if !compilation
            .project
            .modules
            .iter()
            .any(|module| module.id.as_str() == scope)
        {
            return Err(super::diagnostic::invalid("unknown-module"));
        }
    }
    let resolved = resolve(attachment_bytes, compilation)?;
    let mut evidence: Vec<Evidence> = Vec::new();
    let project = &compilation.project;
    let ir_digest = format!(
        "sha256:{}",
        crate::digest::sha256_hex(project.to_canonical_json().as_bytes())
    );
    // 1. Declared import cycles, before any graph construction.
    let edges = graph::import_edges(&project.modules);
    let cycles = graph::import_sccs(&edges);
    for cycle in &cycles {
        let mut finding = Evidence::new(
            "module-api.dependency-cycle",
            cycle
                .members
                .first()
                .map(String::as_str)
                .unwrap_or_default(),
            "boundary-dependency-scc",
        );
        finding.path = cycle.path.clone();
        evidence.push(finding);
    }
    // 2. Cross-module references against the export allowlists.
    let mut exports_of: std::collections::BTreeMap<&str, Vec<&str>> =
        std::collections::BTreeMap::new();
    for module in resolved.modules.values() {
        let list = exports_of.entry(module.module_id.as_str()).or_default();
        for entry in &module.exports {
            list.push(entry.export.as_str());
        }
    }
    let declared_imports: std::collections::BTreeMap<&str, Vec<&str>> = project
        .modules
        .iter()
        .map(|module| {
            (
                module.id.as_str(),
                module
                    .imports
                    .iter()
                    .map(|import| import.as_str())
                    .collect(),
            )
        })
        .collect();
    for definition in project.definitions.iter() {
        let consumer = match definition.id().as_str().split_once('.') {
            Some((owner, _)) => owner,
            None => continue,
        };
        for reference in effects::references_of(definition) {
            let Some(producer) = reference.split_once('.').map(|(owner, _)| owner) else {
                continue;
            };
            if producer == consumer {
                continue;
            }
            // The dedicated cross-owner-write rule owns effect-entity write
            // sites; the export allowlist rule never double-reports them.
            if matches!(definition, crate::ir::Definition::Effect(effect) if effect.entity.as_str() == reference)
            {
                continue;
            }
            // Cross-module references resolve against the producer's export
            // allowlist. Module-private targets are unexported by
            // definition, so they report through the same rule instead of a
            // silent skip (the legacy LEK-SEM-020 direct finding keeps its
            // own seam in the validator).
            match exports_of.get(producer) {
                Some(exported) => {
                    if !exported.contains(&reference) {
                        let mut finding = Evidence::new(
                            "module-api.symbol-not-exported",
                            definition.id().as_str(),
                            "unexported-cross-module-reference",
                        );
                        finding.consumer = Some(consumer.to_owned());
                        finding.producer = Some(producer.to_owned());
                        finding.export = Some(reference.to_owned());
                        finding.path = vec![
                            format!("{consumer}->{producer}"),
                            format!("import:{reference}"),
                            format!("export:{reference}"),
                        ];
                        evidence.push(finding);
                    }
                }
                None => {
                    // A producer absent from the attachment has no allowlist
                    // at all: the reference is a boundary finding, never a
                    // silent pass (unattached producers cannot vacate the
                    // boundary by omission).
                    let mut finding = Evidence::new(
                        "module-api.symbol-not-exported",
                        definition.id().as_str(),
                        "unattached-producer",
                    );
                    finding.consumer = Some(consumer.to_owned());
                    finding.producer = Some(producer.to_owned());
                    finding.export = Some(reference.to_owned());
                    finding.path = vec![
                        format!("{consumer}->{producer}"),
                        format!("reference:{reference}"),
                        "attachment:absent".to_owned(),
                    ];
                    evidence.push(finding);
                }
            }
            // Declared imports: every resolved cross-module use edge must
            // agree with the Model imports (unused imports stay visible,
            // never an error).
            let imports = declared_imports.get(consumer).cloned().unwrap_or_default();
            if !imports.contains(&producer) {
                let mut finding = Evidence::new(
                    "module-api.import-undeclared",
                    definition.id().as_str(),
                    "undeclared-import",
                );
                finding.producer = Some(producer.to_owned());
                finding.export = Some(reference.to_owned());
                finding.path = vec![format!("{consumer}->{producer}")];
                evidence.push(finding);
            }
            // Target-specific exposure inside the portable closure is a
            // direct portability finding for the owner rule; the boundary
            // adds the public closure path only for exported signatures.
        }
    }
    // 3. Public closure of every exported signature: private and
    // target-specific exposure with exact member paths.
    for module in resolved.modules.values() {
        if module.exports.is_empty() {
            continue;
        }
        closure_findings(&resolved, compilation, &module.module_id, &mut evidence)?;
    }
    // 4. Cross-owner writes over typed effect facts.
    let effect_graph = crate::effects::build(project)?;
    for finding in effects::check_writes(&resolved, compilation, &effect_graph) {
        evidence.push(finding);
    }
    // Public dependency projection cycles (module-level public bundles):
    // one memoized export session computes every bundle once, so legal
    // DAGs stay linear and real cycles carry the active path.
    let mut bundles = std::collections::BTreeMap::new();
    let mut public_cycle_evidence = Vec::new();
    let mut session = super::public::ExportSession::new(&resolved, compilation);
    for module in resolved.modules.values() {
        if module.exports.is_empty() {
            continue;
        }
        match session.bundle_checked(&module.module_id) {
            Ok(bundle) => {
                bundles.insert(module.module_id.clone(), bundle);
            }
            Err(super::public::ExportFailure::Cycle(path)) => {
                let mut finding = Evidence::new(
                    "module-api.dependency-cycle",
                    &module.module_id,
                    "public-dependency-scc",
                );
                finding.path = path;
                public_cycle_evidence.push(finding);
                let mut gap = Evidence::new(
                    "module-api.evidence-incomplete",
                    &module.module_id,
                    "public-closure-incomplete",
                );
                gap.gaps = vec!["public-dependency-cycle".to_owned()];
                public_cycle_evidence.push(gap);
            }
            Err(_) => {
                let mut finding = Evidence::new(
                    "module-api.evidence-incomplete",
                    &module.module_id,
                    "public-closure-incomplete",
                );
                finding.gaps = vec!["public-closure-incomplete".to_owned()];
                public_cycle_evidence.push(finding);
            }
        }
    }
    evidence.extend(public_cycle_evidence);
    // Filter owned diagnostics when --module scopes the report; mandatory
    // cross-module errors (cycles, writes with foreign producers) stay.
    if let Some(scope) = module_filter {
        evidence.retain(|finding| match finding.rule {
            "module-api.symbol-not-exported" | "module-api.import-undeclared" => {
                finding.consumer.as_deref() == Some(scope)
            }
            "module-api.public-private-exposure" | "module-api.public-target-exposure" => {
                finding.producer.as_deref() == Some(scope)
                    || finding.subject.starts_with(&format!("{scope}."))
            }
            "module-api.cross-module-write-forbidden" => {
                finding.consumer.as_deref() == Some(scope)
                    || finding.producer.as_deref() == Some(scope)
            }
            _ => true,
        });
    }
    evidence.sort_by(|left, right| {
        (left.rule, left.subject.as_str(), left.detail.as_str()).cmp(&(
            right.rule,
            right.subject.as_str(),
            right.detail.as_str(),
        ))
    });
    evidence.dedup_by(|left, right| {
        left.rule == right.rule
            && left.subject == right.subject
            && left.detail == right.detail
            && left.export == right.export
            && left.member == right.member
            && left.path == right.path
    });
    // Summary rows per module with public digests.
    let mut modules = Vec::new();
    let mut gaps = Vec::new();
    for module in project.modules.iter() {
        let id = module.id.as_str();
        let policy = resolved.modules.get(id);
        let bundle_digest = bundles.get(id).map(|bundle| bundle.digest.clone());
        modules.push(ModuleSummary {
            module_id: id.to_owned(),
            api_version: policy.and_then(|policy| policy.api_version.clone()),
            attachment: policy.is_some(),
            exports: policy
                .map(|policy| policy.exports.len() as u64)
                .unwrap_or(0),
            imports: module
                .imports
                .iter()
                .map(|import| import.as_str().to_owned())
                .collect(),
            consumers: policy
                .map(|policy| policy.consumers.len() as u64)
                .unwrap_or(0),
            public_digest: bundle_digest,
        });
        modules.sort_by(|left, right| left.module_id.cmp(&right.module_id));
        modules.dedup_by(|left, right| left.module_id == right.module_id);
    }
    modules.sort_by(|left, right| left.module_id.cmp(&right.module_id));
    if bundles.len()
        < resolved
            .modules
            .values()
            .filter(|m| !m.exports.is_empty())
            .count()
    {
        gaps.push("public-closure-incomplete".to_owned());
    }
    // The verdict: exact semantic violations are invalid under strict;
    // advisory default keeps the run valid with explicit findings.
    let boundary_violations = evidence
        .iter()
        .filter(|finding| {
            matches!(
                finding.rule,
                "module-api.symbol-not-exported"
                    | "module-api.import-undeclared"
                    | "module-api.public-private-exposure"
                    | "module-api.public-target-exposure"
                    | "module-api.cross-module-write-forbidden"
                    | "module-api.dependency-cycle"
            )
        })
        .count();
    let (assessment, class) = if boundary_violations > 0 {
        (
            "fail",
            if strict {
                FindingClass::Invalid
            } else {
                FindingClass::Boundary
            },
        )
    } else if !gaps.is_empty() {
        ("indeterminate", FindingClass::Evidence)
    } else {
        ("pass", FindingClass::Boundary)
    };
    let mut report = Report {
        schema_version: format!("{}/v{}", wire::REPORT_SCHEMA, wire::VERSION),
        identity: format!("{}@{}", wire::REPORT_IDENTITY, wire::VERSION),
        assessment: assessment.to_owned(),
        command: "check".to_owned(),
        strict,
        module_filter: module_filter.map(str::to_owned),
        pins: wire::ReportPins {
            project: resolved.project.clone(),
            model_version: wire::MODEL_VERSION.to_owned(),
            ir_digest,
            attachment_digest: resolved.digest.clone(),
            registry_version: wire::REGISTRY_VERSION.to_owned(),
        },
        modules,
        findings: evidence.iter().map(super::diagnostic::finding).collect(),
        cycles,
        gaps: gaps.clone(),
        complete: gaps.is_empty(),
    };
    enforce_report_bounds(&mut report);
    Ok(Verdict {
        report,
        evidence,
        class,
    })
}

/// Walk the public closure of one module's exports: every reachable local
/// symbol must be exported (or a legal private helper outside the public
/// signature), and no target-specific type may enter the portable path.
fn closure_findings(
    resolved: &Resolved,
    compilation: &Compilation,
    module: &str,
    evidence: &mut Vec<Evidence>,
) -> Result<(), DiagnosticSet> {
    let project = &compilation.project;
    let by_id: std::collections::BTreeMap<&str, &crate::ir::Definition> = project
        .definitions
        .iter()
        .map(|definition| (definition.id().as_str(), definition))
        .collect();
    let exported: Vec<String> = resolved
        .modules
        .get(module)
        .map(|policy| {
            policy
                .exports
                .iter()
                .map(|entry| entry.export.clone())
                .collect()
        })
        .unwrap_or_default();
    // Bounded worklist over (symbol, path) pairs with a visited set.
    let mut visited: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut work: Vec<(String, Vec<String>)> = exported
        .iter()
        .map(|symbol| (symbol.clone(), vec![symbol.clone()]))
        .collect();
    while let Some((symbol, path)) = work.pop() {
        if !visited.insert(symbol.clone()) {
            continue;
        }
        if path.len() > wire::MAX_DEPENDENCY_NESTING {
            continue;
        }
        let Some(definition) = by_id.get(symbol.as_str()) else {
            continue;
        };
        // The root export itself is admitted by resolve; anything reachable
        // beyond it must be exported too. A module-private symbol can never
        // become a public contract member: the walk reports the exact path
        // instead of silently skipping it.
        if path.len() > 1
            && owner_of(&symbol) == Some(module)
            && (definition.common().visibility == Some(Visibility::Module)
                || !exported.contains(&symbol))
        {
            let mut finding = Evidence::new(
                "module-api.public-private-exposure",
                module,
                "nonexported-type-in-public-closure",
            );
            finding.producer = Some(module.to_owned());
            finding.export = path.first().cloned();
            finding.member = Some(symbol.clone());
            finding.path = path.clone();
            evidence.push(finding);
            continue;
        }
        if definition.common().portability == Some(Portability::TargetSpecific)
            && !exported.is_empty()
        {
            let mut finding = Evidence::new(
                "module-api.public-target-exposure",
                module,
                "target-specific-type-in-public-closure",
            );
            finding.producer = Some(module.to_owned());
            finding.export = path.first().cloned();
            finding.member = Some(symbol.clone());
            finding.path = path.clone();
            evidence.push(finding);
            continue;
        }
        for reference in shape_references_of(definition) {
            let Some(owner) = reference.split_once('.').map(|(owner, _)| owner) else {
                continue;
            };
            let mut next_path = path.clone();
            next_path.push(reference.to_owned());
            if owner == module {
                work.push((reference.to_owned(), next_path));
            } else {
                // Foreign references are the public dependency seam; the
                // pin check happens in the bundle export.
                let _ = next_path;
            }
        }
    }
    Ok(())
}

/// The public *shape* references of one definition: exported type closure
/// walks field/return/payload types only. Query reads stay private query
/// plans; command effect obligations name entities by identity without
/// forcing their internal aggregate shapes into the public closure; and
/// policy/endpoint/scenario wiring is not a type shape.
fn shape_references_of(definition: &crate::ir::Definition) -> Vec<&str> {
    let mut references = Vec::new();
    fn push_type<'a>(type_ref: &'a crate::ir::TypeRef, references: &mut Vec<&'a str>) {
        match type_ref {
            crate::ir::TypeRef::Ref(symbol) => references.push(symbol.as_str()),
            crate::ir::TypeRef::List(inner) | crate::ir::TypeRef::Optional(inner) => {
                push_type(inner, references)
            }
        }
    }
    match definition {
        crate::ir::Definition::ValueObject(value_object) => {
            for field in &value_object.fields {
                push_type(&field.r#type, &mut references);
            }
        }
        crate::ir::Definition::Entity(entity) => {
            for field in &entity.fields {
                push_type(&field.r#type, &mut references);
            }
        }
        crate::ir::Definition::Command(command) => {
            for field in &command.input {
                push_type(&field.r#type, &mut references);
            }
        }
        crate::ir::Definition::Query(query) => {
            if let Some(returns) = &query.returns {
                push_type(returns, &mut references);
            }
        }
        crate::ir::Definition::Event(event) => {
            for field in &event.payload {
                push_type(&field.r#type, &mut references);
            }
        }
        _ => {}
    }
    references
}

fn owner_of(symbol: &str) -> Option<&str> {
    symbol.split_once('.').map(|(owner, _)| owner)
}

/// The recorded report bound: a canonical report above `MAX_REPORT_BYTES`
/// is deterministically truncated (the last canonical findings row is
/// dropped first), explicitly incomplete, and never silently oversized.
fn enforce_report_bounds(report: &mut Report) {
    while wire::canonical(&*report).len() > wire::MAX_REPORT_BYTES
        && (!report.findings.is_empty() || !report.modules.is_empty())
    {
        if !report.findings.is_empty() {
            report.findings.pop();
        } else {
            report.modules.pop();
        }
        report.gaps.push("report-bytes-truncated".to_owned());
        report.gaps.sort();
        report.gaps.dedup();
        report.complete = false;
    }
}

/// Bundle accessor used by the export command.
pub fn bundle(
    resolved: &Resolved,
    compilation: &Compilation,
    module: &str,
) -> Result<PublicBundle, DiagnosticSet> {
    export_public(resolved, compilation, module)
}
