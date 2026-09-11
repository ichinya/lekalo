//! The conformance gate (issue #40): every recorded binding is
//! re-verified against the current canonical model and tree.
//!
//! The check is hermetic and read-only: it re-fingerprints the
//! maintained sources, recomputes the canonical signature and declared
//! effects of every bound symbol from the typed IR, and re-digests every
//! fingerprinted support artifact. Any positive drift — a stale
//! fingerprint, a signature or effect mismatch, a stale artifact — is
//! one registered diagnostic per finding and fails the gate; bindings
//! without fingerprint evidence are `unknown`, never the absence of
//! conformance.

use serde::Serialize;

use super::diagnostic;
use super::types::{
    ConformedRegistry, DeclaredEffect, SignatureEvidence, SignatureField, SymbolKind,
};
use crate::ir::{CompiledProject, Definition, TypeRef};
use crate::project_fs::Fs;

/// The maximum source or artifact file accepted for fingerprinting;
/// larger files record `unknown` evidence instead of a truncated digest.
const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;

/// One drift finding of the gate, also carried as a registered
/// diagnostic.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize)]
pub struct DriftRow {
    /// The bound symbol, or the artifact path for artifact findings.
    pub subject: String,
    /// The fixed drift classification.
    pub detail: &'static str,
}

/// The receipt of a clean `contract check` run.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CheckReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub mode: &'static str,
    /// The checked module, when the gate was scoped to one.
    #[serde(rename = "module", skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub symbols: usize,
    pub conformant: usize,
    pub stale: usize,
    pub unknown: usize,
    pub artifacts: usize,
    #[serde(rename = "staleArtifacts")]
    pub stale_artifacts: usize,
    /// The sorted drift rows (empty on a clean receipt).
    pub drifts: Vec<DriftRow>,
}

/// Run the conformance gate over one registry; every drift becomes one
/// registered diagnostic and fails the run.
pub(super) fn run_check(
    ctx: &super::Context,
    registry: &ConformedRegistry,
    module: Option<&str>,
) -> Result<CheckReceipt, crate::diagnostics::DiagnosticSet> {
    let fs = Fs::open(&ctx.root).map_err(|_| diagnostic::registry_io_set("root-unreadable"))?;
    let mut drift_items: Vec<crate::diagnostics::Diagnostic> = Vec::new();
    let mut drifts: Vec<DriftRow> = Vec::new();
    let push_drift = |drifts: &mut Vec<DriftRow>,
                      items: &mut Vec<crate::diagnostics::Diagnostic>,
                      subject: &str,
                      detail: &'static str| {
        if let Some(item) = diagnostic::try_drift_item(subject, detail) {
            items.push(item);
        }
        drifts.push(DriftRow {
            subject: subject.to_owned(),
            detail,
        });
    };

    let mut conformant = 0usize;
    let mut stale = 0usize;
    let mut unknown = 0usize;
    let mut checked = 0usize;

    for record in &registry.symbols {
        if let Some(module) = module {
            if super::types::ConformedRegistry::module_of(&record.id) != Some(module) {
                continue;
            }
        }
        checked += 1;
        // Canonical existence and kind.
        let definition = find_definition(&ctx.project, &record.id);
        match definition {
            None => {
                push_drift(
                    &mut drifts,
                    &mut drift_items,
                    &record.id,
                    "symbol-missing-from-model",
                );
                continue;
            }
            Some(definition) => {
                if kind_word(definition) != record.kind.key() {
                    push_drift(&mut drifts, &mut drift_items, &record.id, "kind-mismatch");
                    continue;
                }
            }
        }
        // Fingerprint staleness.
        match &record.fingerprint {
            Some(digest) => match read_fingerprint(&fs, &record.source.path) {
                Ok(Some(actual)) if &actual == digest => conformant += 1,
                Ok(Some(_)) => {
                    stale += 1;
                    push_drift(
                        &mut drifts,
                        &mut drift_items,
                        &record.id,
                        "fingerprint-mismatch",
                    );
                }
                Ok(None) | Err(_) => {
                    stale += 1;
                    push_drift(&mut drifts, &mut drift_items, &record.id, "source-missing");
                }
            },
            None => unknown += 1,
        }
        // Native coverage: every bound operation needs at least one
        // attached native test or a scenario-skeleton support artifact.
        if matches!(record.kind, SymbolKind::Command | SymbolKind::Query) {
            let covered = !record.native_tests.is_empty()
                || registry.artifacts.iter().any(|artifact| {
                    artifact.symbol == record.id
                        && artifact.kind == super::types::SupportKind::TestSkeleton
                });
            if !covered {
                if let Some(item) = diagnostic::try_coverage_item(&record.id) {
                    drift_items.push(item);
                }
                drifts.push(DriftRow {
                    subject: record.id.clone(),
                    detail: "coverage-missing",
                });
            }
        }
        // Signature and declared-effect conformance.
        let members = canonical_members(&ctx.project, &record.id);
        if let (Some(claim), Some(canonical)) = (
            &record.signature,
            members.as_ref().and_then(|m| m.signature.as_ref()),
        ) {
            if !signature_matches(claim, canonical) {
                push_drift(&mut drifts, &mut drift_items, &record.id, "signature");
            }
        }
        if let Some(canonical) = members.as_ref() {
            if canonical.effects != record.effects {
                push_drift(&mut drifts, &mut drift_items, &record.id, "effects");
            }
        }
    }

    // Module scope: every canonical operation of the module must be
    // implemented; a contracted slice with unbound operations is drift.
    if let Some(module) = module {
        for definition in &ctx.project.definitions {
            let id = definition.id().as_str();
            let is_operation = matches!(definition, Definition::Command(_) | Definition::Query(_));
            if !is_operation
                || ConformedRegistry::module_of(id) != Some(module)
                || registry.symbols.iter().any(|record| record.id == id)
            {
                continue;
            }
            if let Some(item) = diagnostic::try_drift_item(id, "unimplemented") {
                drift_items.push(item);
            }
            drifts.push(DriftRow {
                subject: id.to_owned(),
                detail: "unimplemented",
            });
        }
    }

    // Support-artifact staleness.
    let mut stale_artifacts = 0usize;
    let mut artifacts_checked = 0usize;
    for artifact in &registry.artifacts {
        if let Some(module) = module {
            if super::types::ConformedRegistry::module_of(&artifact.symbol) != Some(module) {
                continue;
            }
        }
        artifacts_checked += 1;
        let Some(digest) = &artifact.digest else {
            // Scaffolded artifacts without a digest are existence-checked
            // only; the owner completes them by hand.
            match read_fingerprint(&fs, &artifact.path) {
                Ok(Some(_)) => {}
                _ => {
                    stale_artifacts += 1;
                    if let Some(item) = diagnostic::try_stale_artifact_item(&artifact.path) {
                        drift_items.push(item);
                    }
                    drifts.push(DriftRow {
                        subject: artifact.path.clone(),
                        detail: "artifact-missing",
                    });
                }
            }
            continue;
        };
        match read_fingerprint(&fs, &artifact.path) {
            Ok(Some(actual)) if &actual == digest => {}
            Ok(Some(_)) => {
                stale_artifacts += 1;
                if let Some(item) = diagnostic::try_stale_artifact_item(&artifact.path) {
                    drift_items.push(item);
                }
                drifts.push(DriftRow {
                    subject: artifact.path.clone(),
                    detail: "content-stale",
                });
            }
            Ok(None) | Err(_) => {
                stale_artifacts += 1;
                if let Some(item) = diagnostic::try_stale_artifact_item(&artifact.path) {
                    drift_items.push(item);
                }
                drifts.push(DriftRow {
                    subject: artifact.path.clone(),
                    detail: "artifact-missing",
                });
            }
        }
    }

    drifts.sort();
    drifts.dedup();
    if !drift_items.is_empty() {
        return Err(crate::diagnostics::DiagnosticSet::try_from_unsorted(
            drift_items,
            crate::result::Status::Invalid,
        )
        .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")));
    }

    Ok(CheckReceipt {
        status: "valid",
        operation: "conform",
        mode: "check",
        module: module.map(|module| module.to_owned()),
        symbols: checked,
        conformant,
        stale,
        unknown,
        artifacts: artifacts_checked,
        stale_artifacts,
        drifts,
    })
}

/// The canonical signature and declared-effect members of one
/// definition, recomputed from the typed IR.
pub(super) struct CanonicalMembers {
    pub signature: Option<SignatureEvidence>,
    pub effects: Vec<DeclaredEffect>,
}

/// Recompute the canonical members of one symbol from the typed IR.
pub(super) fn canonical_members(project: &CompiledProject, id: &str) -> Option<CanonicalMembers> {
    let definition = find_definition(project, id)?;
    Some(match definition {
        Definition::Command(command) => {
            let mut inputs: Vec<SignatureField> = command
                .input
                .iter()
                .map(|field| SignatureField {
                    name: field.name.as_str().to_owned(),
                    r#type: render_type(&field.r#type),
                    required: field.required,
                })
                .collect();
            inputs.sort_by(|left, right| left.name.cmp(&right.name));
            let mut effects: Vec<DeclaredEffect> = Vec::new();
            for effect_ref in &command.effects {
                if let Some(Definition::Effect(effect)) =
                    find_definition(project, effect_ref.as_str())
                {
                    effects.push(DeclaredEffect {
                        kind: effect.operation.as_str().to_owned(),
                        subject: effect.entity.as_str().to_owned(),
                    });
                    for emit in &effect.emits {
                        effects.push(DeclaredEffect {
                            kind: "emit-event".to_owned(),
                            subject: emit.as_str().to_owned(),
                        });
                    }
                }
            }
            effects.sort();
            effects.dedup();
            CanonicalMembers {
                signature: Some(SignatureEvidence {
                    inputs,
                    output: None,
                    reads: Vec::new(),
                }),
                effects,
            }
        }
        Definition::Query(query) => {
            let mut reads: Vec<String> = query
                .reads
                .iter()
                .map(|reference| reference.as_str().to_owned())
                .collect();
            reads.sort();
            reads.dedup();
            CanonicalMembers {
                signature: Some(SignatureEvidence {
                    inputs: Vec::new(),
                    output: query.returns.as_ref().map(render_type),
                    reads,
                }),
                effects: Vec::new(),
            }
        }
        _ => CanonicalMembers {
            signature: None,
            effects: Vec::new(),
        },
    })
}

/// Whether the claimed signature equals the canonical one: inputs are
/// compared as a name-keyed set (permutation-insensitive), output and
/// reads exactly.
pub(super) fn signature_matches(claim: &SignatureEvidence, canonical: &SignatureEvidence) -> bool {
    let mut claim_inputs = claim.inputs.clone();
    claim_inputs.sort_by(|left, right| left.name.cmp(&right.name));
    claim_inputs == canonical.inputs
        && claim.output == canonical.output
        && claim.reads == canonical.reads
}

fn find_definition<'a>(project: &'a CompiledProject, id: &str) -> Option<&'a Definition> {
    project
        .definitions
        .iter()
        .find(|definition| definition.id().as_str() == id)
}

pub(super) fn kind_word(definition: &Definition) -> &'static str {
    match definition {
        Definition::Scalar(_) => "scalar",
        Definition::Enum(_) => "enum",
        Definition::ValueObject(_) => "value-object",
        Definition::Entity(_) => "entity",
        Definition::Command(_) => "command",
        Definition::Query(_) => "query",
        Definition::Policy(_) => "policy",
        Definition::Event(_) => "event",
        Definition::Effect(_) => "effect",
        Definition::Endpoint(_) => "endpoint",
        Definition::Scenario(_) => "scenario",
        Definition::TargetBinding(_) => "target-binding",
    }
}

/// The canonical type rendering: `id`, `list(T)`, `optional(T)`.
pub(super) fn render_type(r#type: &TypeRef) -> String {
    match r#type {
        TypeRef::Ref(id) => id.as_str().to_owned(),
        TypeRef::List(inner) => format!("list({})", render_type(inner)),
        TypeRef::Optional(inner) => format!("optional({})", render_type(inner)),
    }
}

/// The sha256 of one project file through the confined fs; a missing
/// file is `None`, an over-large file is unknown (`None`).
pub(super) fn read_fingerprint(
    fs: &Fs,
    path: &str,
) -> Result<Option<String>, crate::diagnostics::DiagnosticSet> {
    let (dir, name) = split_logical(path);
    match fs.read_file_opt(dir, name, MAX_SOURCE_BYTES) {
        Ok(Some(bytes)) => Ok(Some(format!(
            "sha256:{}",
            crate::versioning::plan::sha256_hex(&bytes)
        ))),
        Ok(None) => Ok(None),
        Err(_) => Ok(None),
    }
}

/// Split `a/b/c.ts` into (`a/b`, `c.ts`); a bare name reads from the
/// project root.
fn split_logical(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(at) => (&path[..at], &path[at + 1..]),
        None => (".", path),
    }
}

/// The bound symbol kinds that may carry signature claims.
pub(super) fn signature_required(kind: SymbolKind) -> bool {
    matches!(kind, SymbolKind::Command | SymbolKind::Query)
}

/// The bound symbol kinds that may carry declared-effect claims.
pub(super) fn effects_allowed(kind: SymbolKind) -> bool {
    matches!(kind, SymbolKind::Command)
}
