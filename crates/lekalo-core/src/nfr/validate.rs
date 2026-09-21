//! Semantic validation of the NFR attachment against the bound IR and
//! its evidence (issue #85).
//!
//! Custody comes first: the attachment binds exactly one project, one
//! Model state, and one IR state; anything else refuses before any
//! work. Scope references then resolve against the compiled IR with
//! the closed kind mapping (project scope → project id, module scope →
//! module id, operation scope → command definition, endpoint scope →
//! endpoint definition). Evidence sets are checked for coherence:
//! every result names a declared constraint, every measurement unit
//! matches the declared requirement unit, and latency rows carry the
//! declared percentile. Revision mismatches and expiry are not
//! validation failures — they are first-class report statuses.

use crate::diagnostics::DiagnosticSet;
use crate::ir::Compilation;
use crate::ir::DefinitionKind;
use crate::loader::LoadSelection;
use crate::result::DomainResult;

use super::constraint::{Kind, RuntimeKind, ScopeKind};
use super::diagnostic;
use super::evidence::EvidenceSet;
use super::wire::NfrAttachment;

/// Validate one attachment against its project selection: load and
/// compile, verify the project/Model/IR custody, and resolve every
/// scope reference with its closed kind mapping. Pure and read-only;
/// loader and IR failures pass through unchanged.
pub fn validate(attachment: &NfrAttachment, selection: &LoadSelection) -> Result<(), DomainResult> {
    let model_json = match crate::loader::run(selection, false) {
        DomainResult::Valid {
            payload: crate::result::SuccessPayload::Model { json, .. },
            ..
        } => json,
        other => return Err(other),
    };
    let model = crate::loader::normalize_model(selection)?;
    let compilation = crate::ir::compile(&model).map_err(|failure| failure.into_result())?;
    validate_compiled(
        attachment,
        &model_json,
        model.model_version.as_str(),
        &compilation,
    )
}

/// The compiled form of [`validate`]: custody plus scope resolution
/// over an already-loaded compilation.
pub(crate) fn validate_compiled(
    attachment: &NfrAttachment,
    model_json: &str,
    model_version: &str,
    compilation: &Compilation,
) -> Result<(), DomainResult> {
    // Custody: the attachment binds exactly one project and one
    // Model/IR state; anything else refuses before any work.
    let project_id = compilation
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str())
        .unwrap_or_default();
    if attachment.project_id().as_str() != project_id {
        return Err(DomainResult::denied(diagnostic::model_ref_mismatch(
            "project-id",
        )));
    }
    if attachment.model_ref().0 != model_version {
        return Err(DomainResult::denied(diagnostic::model_ref_mismatch(
            "model-version",
        )));
    }
    if attachment.model_ref().1.as_str()
        != crate::requirements::sha256_digest(model_json.as_bytes())
    {
        return Err(DomainResult::denied(diagnostic::model_ref_mismatch(
            "model-digest",
        )));
    }
    let ir_digest =
        crate::requirements::sha256_digest(compilation.project.to_canonical_json().as_bytes());
    if attachment.ir_ref().1.as_str() != ir_digest {
        return Err(DomainResult::denied(diagnostic::model_ref_mismatch(
            "ir-digest",
        )));
    }
    check_scopes(attachment, compilation).map_err(DomainResult::invalid)
}

/// Resolve every scope reference against the compiled IR with the
/// closed kind mapping.
pub(crate) fn check_scopes(
    attachment: &NfrAttachment,
    compilation: &Compilation,
) -> Result<(), DiagnosticSet> {
    let project_id = compilation
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str().to_owned());
    let modules: std::collections::BTreeSet<&str> = compilation
        .project
        .modules
        .iter()
        .map(|module| module.id.as_str())
        .collect();
    let definitions: std::collections::BTreeMap<&str, DefinitionKind> = compilation
        .project
        .definitions
        .iter()
        .map(|definition| (definition.id().as_str(), definition.kind()))
        .collect();
    for constraint in attachment.constraints() {
        let scope = constraint.scope();
        let resolved = match scope.kind() {
            ScopeKind::Project => project_id.as_deref() == Some(scope.reference()),
            ScopeKind::Module => modules.contains(scope.reference()),
            ScopeKind::Operation => {
                definitions.get(scope.reference()) == Some(&DefinitionKind::Command)
            }
            ScopeKind::Endpoint => {
                definitions.get(scope.reference()) == Some(&DefinitionKind::Endpoint)
            }
        };
        if !resolved {
            return Err(diagnostic::scope_unknown(
                scope.kind().as_str(),
                Some(scope.reference()),
            ));
        }
    }
    Ok(())
}

/// Validate one evidence set against its attachment: the project must
/// match, every result must name a declared constraint, and every
/// measurement must agree with the declared requirement unit and
/// percentile. Revision mismatches and expiry stay report statuses.
pub fn validate_evidence(
    attachment: &NfrAttachment,
    evidence: &EvidenceSet,
) -> Result<(), DiagnosticSet> {
    if attachment.project_id().as_str() != evidence.project_id().as_str() {
        return Err(diagnostic::evidence_invalid("project-mismatch", None));
    }
    let constraints: std::collections::BTreeMap<&str, &super::constraint::Constraint> = attachment
        .constraints()
        .iter()
        .map(|constraint| (constraint.constraint_id().as_str(), constraint))
        .collect();
    for result in evidence.results() {
        let Some(constraint) = constraints.get(result.constraint_id().as_str()) else {
            return Err(diagnostic::evidence_invalid(
                "unknown-constraint",
                Some(result.constraint_id().as_str()),
            ));
        };
        let constraint = *constraint;
        for measurement in result.measurements() {
            // A numeric constraint's evidence must measure the same
            // unit; a declared unit on the constraint is exact.
            if let Some(unit) = constraint.requirement().unit() {
                if measurement.unit() != unit.as_str() {
                    return Err(diagnostic::evidence_invalid(
                        "unit-mismatch",
                        Some(result.constraint_id().as_str()),
                    ));
                }
            }
            // Latency evidence carries the declared percentile.
            if let Kind::Runtime(RuntimeKind::Latency) = constraint.kind() {
                if measurement.percentile() != constraint.requirement().percentile() {
                    return Err(diagnostic::evidence_invalid(
                        "percentile-mismatch",
                        Some(result.constraint_id().as_str()),
                    ));
                }
            }
        }
    }
    Ok(())
}
