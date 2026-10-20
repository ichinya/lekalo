//! Safe source-only fixes lowered into the existing sealed change plan.
//! No caller-controlled byte patches are admitted. The formatter owns its
//! equality-checked edits; physical moves and binding confirmations reuse the
//! validated base plan before adding their separately sealed owner metadata.

use super::*;

/// Ledger-first fix replay uses exactly the change service's custody and
/// endpoint proofs, under the same writer lock. A reviewed old plan remains
/// idempotent at the CLI without treating an editable receipt as authority.
pub(crate) fn replay(
    project: Option<&str>,
    expected: &str,
    recipe_prefix: &str,
    recipe_digest: Option<&str>,
    policy: Option<&[u8]>,
    allow_breaking: bool,
    validate: &dyn Fn(&crate::loader::ProjectLoad) -> Result<(), ChangeError>,
) -> Result<Option<Receipt>, ChangeError> {
    if expect_digest(expected).is_none() {
        return Err(ChangeError::PreconditionFailed {
            subject: "plan".to_owned(),
            detail: "expect-plan",
        });
    }
    let policy = policy
        .map(Policy::decode)
        .transpose()?
        .unwrap_or_else(Policy::restrictive);
    let root = resolve_root(project)?;
    let _lock = crate::project_writer::WriterLock::mutation(
        &root,
        crate::project_writer::MutationFamily::Change,
    )
    .map_err(change_mutation_refusal)?;
    let bytes = read_history_bytes(&root)?;
    let custody = custody::verify(&root, bytes.as_deref())?;
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    let history = History::decode(&bytes)?;
    verify_history_project(&root, &history)?;
    let key = receipt::key_digest(&format!("fix:{expected}"));
    let Some(record) = history.by_key(&key) else {
        return Ok(None);
    };
    if record.plan_id != expected
        || recipe_digest.is_some_and(|digest| record.request_digest != digest)
        || record.intent_digest
            != receipt::intent_digest(policy.digest.as_deref(), Some(expected), allow_breaking)
    {
        return Err(ChangeError::IdempotencyConflict);
    }
    custody.require_local(record.sequence)?;
    verify_record_postcondition(&root, &history, record)?;
    let receipt = replayed_receipt(record)?;
    if !receipt.operations.iter().any(|operation| {
        operation.operation == "fix:apply"
            && operation
                .detail
                .as_deref()
                .is_some_and(|detail| detail.starts_with(recipe_prefix))
    }) {
        return Err(ChangeError::IdempotencyConflict);
    }
    let load = crate::loader::load_validated_root(root).map_err(ChangeError::LoadFailed)?;
    validate(&load)?;
    Ok(Some(receipt))
}

pub(crate) fn prepare_format(
    load: &crate::loader::ProjectLoad,
    recipe_bytes: Vec<u8>,
    audit: ResolvedOperation,
    policy: Option<&[u8]>,
    allow_breaking: bool,
) -> Result<PreparedChange, ChangeError> {
    let edits = crate::format::fix_candidates(load).map_err(ChangeError::LoadFailed)?;
    prepare_source_only(load, recipe_bytes, audit, policy, allow_breaking, edits)
}

pub(crate) fn prepare_source_only(
    load: &crate::loader::ProjectLoad,
    recipe_bytes: Vec<u8>,
    audit: ResolvedOperation,
    policy: Option<&[u8]>,
    allow_breaking: bool,
    edits: Vec<(String, Vec<u8>)>,
) -> Result<PreparedChange, ChangeError> {
    // Policy and acknowledgement remain exact audit intent even for a safe
    // executor. Admit the same closed policy on preparation and replay; never
    // silently replace supplied options with restrictive/null defaults.
    let policy = policy
        .map(Policy::decode)
        .transpose()?
        .unwrap_or_else(Policy::restrictive);
    let compiled = ir::compile(&load.model)
        .map_err(|failure| ChangeError::LoadFailed(failure.into_result()))?;
    let profile = ValidationProfile::embedded_default()
        .map_err(|_| ChangeError::RequestInvalid { detail: "profile" })?;
    crate::validator::validate(&compiled, profile)
        .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    let mut inputs: Vec<InputPin> = load
        .documents
        .iter()
        .map(|document| InputPin {
            path: document.document.path.clone(),
            digest: receipt::bytes_digest(&document.bytes),
        })
        .collect();
    let history = read_history_bytes(&load.root)?;
    inputs.push(InputPin {
        path: version::HISTORY_LOGICAL_PATH.to_owned(),
        digest: receipt::bytes_digest(history.as_deref().unwrap_or_default()),
    });
    inputs.sort_by(|a, b| a.path.cmp(&b.path));
    if inputs.len() > version::MAX_PLAN_INPUTS || edits.len() > version::MAX_PLAN_FILES {
        return Err(ChangeError::PlanExportLimit {
            subject: "fix-inputs",
            measured: inputs.len() as u64,
            allowed: version::MAX_PLAN_INPUTS as u64,
        });
    }
    let mut file_edits = Vec::new();
    let mut file_diffs = Vec::new();
    for (path, after) in &edits {
        let before = &load
            .documents
            .iter()
            .find(|d| &d.document.path == path)
            .ok_or(ChangeError::WriteBoundaryDenied)?
            .bytes;
        // A fix must show the WHOLE diff before consent, never a truncated
        // preview (also bounds the line-LCS allocation before computing it).
        if before.len().max(after.len()) > version::MAX_FILE_DIFF_BYTES {
            return Err(ChangeError::SourcePreservation {
                subject: path.clone(),
                detail: "fix-diff-limit",
            });
        }
        let before = std::str::from_utf8(before).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let after = std::str::from_utf8(after).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        if before.lines().count().max(after.lines().count()) > version::MAX_PATCH_LINES {
            return Err(ChangeError::SourcePreservation {
                subject: path.clone(),
                detail: "fix-diff-limit",
            });
        }
        let (patch, complete) = plan::compute_patch(before, after);
        if !complete {
            return Err(ChangeError::SourcePreservation {
                subject: path.clone(),
                detail: "fix-diff-limit",
            });
        }
        file_edits.push(FileEdit {
            path: path.clone(),
            before_digest: receipt::bytes_digest(before.as_bytes()),
            after_digest: receipt::bytes_digest(after.as_bytes()),
        });
        file_diffs.push(FileDiff {
            path: path.clone(),
            before: before.to_owned(),
            after: after.to_owned(),
            patch,
        });
    }
    let (generation, generation_pins) = build_generation_plan(&load.root)?;
    Ok(PreparedChange {
        binding: None,
        module_move: None,
        request_bytes: recipe_bytes,
        policy,
        operations: vec![audit],
        inputs,
        file_edits,
        file_diffs,
        semantic_diff: SemanticDiffSummary {
            equal: true,
            classification: vec![],
            changes: vec![],
            complete: true,
            complete_reason: None,
        },
        impact: plan::ImpactProjection {
            seeds: vec![],
            affected: vec![],
            complete: true,
        },
        generation,
        denied_breaking_classes: vec![],
        mode: "preview",
        project_id: load
            .model
            .project
            .as_ref()
            .ok_or(ChangeError::WriteBoundaryDenied)?
            .id
            .clone(),
        edits,
        creations: vec![],
        generation_pins,
        root: load.root.clone(),
        diff_complete: true,
        allow_breaking,
    })
}
