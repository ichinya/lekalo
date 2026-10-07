//! Recommendations and the concrete selective preview (issue #136,
//! increment I3): every contract/API/effect/policy/test/binding
//! discrepancy between the confirmed semantics and the applicable
//! target evidence becomes a recommendation with reason, files, risk,
//! verification, and uncertainty; a supplied reviewed candidate patch
//! turns the selected candidate into an exact before/after write set.
//!
//! Without `--candidate` the plan is advisory only and can never apply.
//! Declining a preview writes nothing; unsupported transformations stay
//! advice.

use std::collections::BTreeSet;

use super::analysis::{
    analysis_digest, analyze, compute_pins, scope_source_paths, AdoptionContext, AnalysisScope,
    Outcome,
};
use super::diagnostic;
use super::version;
use super::wire::{self, PatchDocument, Recommendation, RefactorPlan, Scope, UnsupportedAdvice};
use crate::observed::types::{BindingStatus, ObservedIndex, SymbolKind};
use crate::result::DomainResult;

/// The requested refactor preview.
pub struct RefactorRequest {
    pub scope: AnalysisScope,
    pub target: Option<String>,
    pub profile: Option<String>,
    /// The closed verification specification bytes.
    pub verification: Vec<u8>,
    /// The optional supplied reviewed candidate patch bytes.
    pub candidate: Option<Vec<u8>>,
}

/// Build the refactor plan (preview; no writes).
pub fn plan(
    ctx: &AdoptionContext,
    index: Option<&ObservedIndex>,
    request: &RefactorRequest,
) -> Outcome<RefactorPlan> {
    let analysis = analyze(
        ctx,
        index,
        &request.scope,
        request.target.as_deref(),
        request.profile.as_deref(),
    )?;
    let analysis_ref = analysis_digest(&analysis);

    // The verification specification is mandatory advice infrastructure:
    // the declared obligations bind advice, candidate, and verification.
    let spec = wire::verification_of_bytes(&request.verification)
        .map_err(|error| error.domain_result())?;
    if spec.variant != "specification" {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "not-a-specification",
        )));
    }
    let spec_value = serde_json::to_value(&spec).expect("spec serializes");
    let spec_digest = wire::digest("adoption-verification", &spec_value);

    // The optional candidate patch: closed, data-only, scope-contained.
    let patch = match &request.candidate {
        Some(bytes) => Some(validate_patch(ctx, &analysis, bytes)?),
        None => None,
    };
    let patch_digest = patch.as_ref().map(|document| {
        let value = serde_json::to_value(document).expect("patch serializes");
        wire::digest("adoption-patch", &wire::without_member(&value, "patch"))
    });

    let (_, ids) = super::analysis::resolve_scope(ctx, index, &request.scope)?;
    let records: Vec<crate::observed::types::SymbolRecord> = ids
        .iter()
        .filter_map(|id| index.and_then(|index| index.symbol(id)).cloned())
        .collect();
    let registry = load_conformed(ctx);
    let conformed: BTreeSet<String> = registry
        .as_ref()
        .map(|registry| {
            registry
                .symbols
                .iter()
                .map(|symbol| symbol.id.clone())
                .collect()
        })
        .unwrap_or_default();
    let canonical: BTreeSet<&str> = ctx
        .model
        .definitions
        .iter()
        .map(|definition| definition.id.as_str())
        .collect();

    let mut recommendations: Vec<Recommendation> = Vec::new();
    let mut unsupported: Vec<UnsupportedAdvice> = Vec::new();
    let check_ids: Vec<String> = spec.checks.iter().map(|check| check.id.clone()).collect();
    for record in &records {
        if record.native_tests.is_empty() {
            recommendations.push(Recommendation {
                id: format!("test:{}", record.id),
                kind: "test".to_owned(),
                reason: "no-native-test-attached".to_owned(),
                files: record
                    .location
                    .as_ref()
                    .map(|location| vec![location.path.clone()])
                    .unwrap_or_default(),
                risk: "medium".to_owned(),
                verification: check_ids.clone(),
                uncertainty: None,
            });
        }
        if !matches!(
            record.status,
            BindingStatus::Explicit | BindingStatus::Confirmed
        ) {
            recommendations.push(Recommendation {
                id: format!("binding:{}", record.id),
                kind: "binding".to_owned(),
                reason: "binding-not-confirmed".to_owned(),
                files: Vec::new(),
                risk: "low".to_owned(),
                verification: Vec::new(),
                uncertainty: Some("inferred-evidence".to_owned()),
            });
        }
        if canonical.contains(record.id.as_str()) && !conformed.contains(&record.id) {
            recommendations.push(Recommendation {
                id: format!("contract:{}", record.id),
                kind: "contract".to_owned(),
                reason: "definition-not-declared".to_owned(),
                files: vec![format!(
                    "declarations/{}.json",
                    crate::observed::types::ObservedIndex::module_of(&record.id)
                        .unwrap_or_default()
                )],
                risk: "low".to_owned(),
                verification: check_ids.clone(),
                uncertainty: None,
            });
        }
        if record.kind == SymbolKind::Effect {
            recommendations.push(Recommendation {
                id: format!("effect:{}", record.id),
                kind: "effect".to_owned(),
                reason: "effect-evidence-unverified".to_owned(),
                files: record
                    .location
                    .as_ref()
                    .map(|location| vec![location.path.clone()])
                    .unwrap_or_default(),
                risk: "high".to_owned(),
                verification: check_ids.clone(),
                uncertainty: Some("runtime-effect-unknown".to_owned()),
            });
        }
        if record.kind == SymbolKind::Policy {
            unsupported.push(UnsupportedAdvice {
                kind: format!("policy:{}", record.id),
                reason: "policy-decision-unsupported".to_owned(),
            });
        }
    }
    if let Some(patch) = &patch {
        recommendations.push(Recommendation {
            id: format!("api:{}", patch.scope.symbol),
            kind: "api".to_owned(),
            reason: "supplied-reviewed-patch".to_owned(),
            files: patch.files.iter().map(|file| file.path.clone()).collect(),
            risk: "medium".to_owned(),
            verification: check_ids.clone(),
            uncertainty: None,
        });
    }
    recommendations.sort_by(|left, right| left.id.cmp(&right.id));
    unsupported.sort_by(|left, right| left.kind.cmp(&right.kind));

    let read_scope: Vec<String> = scope_source_paths(index, &records).into_iter().collect();
    let mutation_scope: Vec<String> = patch
        .as_ref()
        .map(|document| {
            document
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect()
        })
        .unwrap_or_default();
    let preservation: Vec<super::wire::Preservation> = spec
        .checks
        .iter()
        .map(|check| super::wire::Preservation {
            obligation: check.id.clone(),
            check: check.kind.clone(),
        })
        .collect();

    let pins = compute_pins(ctx, index, &scope_source_paths(index, &records))?;
    let project = analysis.project.clone();
    let partial = RefactorPlan {
        schema_version: version::REFACTOR_PLAN_SCHEMA_VERSION.to_owned(),
        identity: version::REFACTOR_PLAN_IDENTITY.to_owned(),
        plan: String::new(),
        project,
        analysis: analysis_ref,
        selection: request.scope.wire(),
        patch: patch_digest,
        verification: Some(spec_digest),
        recommendations,
        unsupported,
        read_scope,
        mutation_scope,
        preservation,
        pins,
        rollback: super::wire::RollbackHome {
            journal: String::new(),
            snapshots: String::new(),
        },
    };
    let value = serde_json::to_value(&partial).expect("plan serializes");
    let digest = wire::digest(
        "adoption-refactor-plan",
        &wire::without_member(&wire::without_member(&value, "plan"), "rollback"),
    );
    let (head, tail) = version::digest_dir(&digest).unwrap_or_default();
    Ok(RefactorPlan {
        plan: digest,
        rollback: super::wire::RollbackHome {
            journal: format!("{}/{}/{}/journal.json", version::RECOVERY_DIR, head, tail),
            snapshots: format!("{}/{}/{}/snapshots", version::RECOVERY_DIR, head, tail),
        },
        ..partial
    })
}

/// The conformed registry of the same project, when present, re-exported
/// for the coverage join; registry and loader failures keep the derived
/// view alive with explicit gaps instead of fabricating facts.
pub(crate) fn conformed_registry(
    ctx: &AdoptionContext,
) -> Option<crate::contracted::types::ConformedRegistry> {
    load_conformed(ctx)
}

/// The conformed registry of the same project, when present; registry
/// and loader failures keep the advisory preview alive with an explicit
/// unknown instead of fabricating conformance facts.
fn load_conformed(ctx: &AdoptionContext) -> Option<crate::contracted::types::ConformedRegistry> {
    let compilation = crate::ir::compile(&ctx.model).ok()?;
    let context = crate::contracted::Context {
        root: ctx.root.clone(),
        project: compilation.project,
    };
    crate::contracted::load_registry(&context).ok()?
}

/// Validate a candidate patch: closed shape, exact digests, bounded
/// content, and every path inside the analysis scope's native root or
/// recorded evidence set. A patch cannot embed executables, only data.
pub(crate) fn validate_patch(
    _ctx: &AdoptionContext,
    analysis: &super::wire::AnalysisDocument,
    bytes: &[u8],
) -> Outcome<PatchDocument> {
    let document = wire::patch_of_bytes(bytes).map_err(|error| error.domain_result())?;
    if document.files.is_empty() {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "candidate",
            "empty",
        )));
    }
    let mut allowed_roots: Vec<String> = Vec::new();
    if let Some(root) = &analysis.module.native_root {
        allowed_roots.push(format!("{root}/"));
    }
    for source in &analysis.pins.sources {
        allowed_roots.push(source.path.clone());
    }
    let mut seen = BTreeSet::new();
    for file in &document.files {
        if !version::is_rel_path(&file.path) {
            return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                "candidate",
                "path-grammar",
            )));
        }
        if !seen.insert(file.path.clone()) {
            return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                "candidate",
                "duplicate-path",
            )));
        }
        let contained = allowed_roots
            .iter()
            .any(|root| file.path == *root || file.path.starts_with(root.as_str()));
        if !contained {
            return Err(DomainResult::denied_json(
                serde_json::json!({ "path": file.path, "scope": analysis.scope }).to_string(),
                format!("candidate path {} is outside the selected scope", file.path),
                diagnostic::scope_refused_set(&file.path, "outside-scope"),
            ));
        }
        match file.action.as_str() {
            "create" => {
                if file.before.is_some() || file.content.is_none() {
                    return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                        "candidate",
                        "create-shape",
                    )));
                }
            }
            "replace" => {
                if file.before.is_none() || file.content.is_none() {
                    return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                        "candidate",
                        "replace-shape",
                    )));
                }
            }
            "delete" => {
                if file.after.is_some() || file.content.is_some() {
                    return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                        "candidate",
                        "delete-shape",
                    )));
                }
            }
            _ => {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "candidate",
                    "action",
                )))
            }
        }
        if let Some(content) = &file.content {
            if content.len() > version::MAX_FILE_BYTES {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "candidate",
                    "over-limit",
                )));
            }
            let digest = format!("sha256:{}", crate::digest::sha256_hex(content.as_bytes()));
            if file.after.as_deref() != Some(digest.as_str()) {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "candidate",
                    "content-digest",
                )));
            }
        }
    }
    Ok(document)
}

/// Re-fingerprint the tree digest the verification specification pins:
/// the sorted `path US sha256` lines of every source file in the pins.
pub(crate) fn tree_digest(pins: &super::wire::Pins) -> String {
    let mut payload = String::new();
    for source in &pins.sources {
        payload.push_str(&source.path);
        payload.push('\u{1f}');
        payload.push_str(&source.sha256);
        payload.push('\n');
    }
    format!("sha256:{}", crate::digest::sha256_hex(payload.as_bytes()))
}

/// The scope selector re-export for the CLI.
pub type ScopeSelector = AnalysisScope;

/// The closed wire scope of a selector (re-export for the CLI).
pub fn scope_wire(scope: &AnalysisScope) -> Scope {
    scope.wire()
}
