//! Error-union changes reuse the #62 registry and revision classifier. The
//! fixed canonical source is a sealed input of every Change plan, including
//! its absence, so Model edits cannot invalidate an unobserved attachment.

use std::path::Path;

use super::*;
use crate::error_contract::{self as owner, ErrorRegistry};

pub(crate) fn read_source(root: &Path) -> Result<Option<Vec<u8>>, ChangeError> {
    let bytes = owner::source::read_bytes(root)
        .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    if let Some(bytes) = &bytes {
        ErrorRegistry::from_bytes(bytes)
            .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    }
    Ok(bytes)
}

pub(crate) fn input_pin(bytes: Option<&[u8]>) -> InputPin {
    InputPin {
        path: owner::source::PROJECT_PATH.to_owned(),
        digest: receipt::bytes_digest(bytes.unwrap_or_default()),
    }
}

pub(crate) fn validate_source(
    bytes: Option<&[u8]>,
    project: &ir::CompiledProject,
) -> Result<(), ChangeError> {
    if let Some(bytes) = bytes {
        let registry = ErrorRegistry::from_bytes(bytes)
            .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
        owner::source::validate(&registry, project)
            .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    }
    Ok(())
}

pub(crate) fn validate_after(
    prepared: &PreparedChange,
    project: &ir::CompiledProject,
) -> Result<(), ChangeError> {
    let current = read_source(&prepared.root)?;
    let expected = prepared
        .file_edits
        .iter()
        .find(|edit| edit.path == owner::source::PROJECT_PATH)
        .map(|edit| &edit.after_digest)
        .or_else(|| {
            prepared
                .inputs
                .iter()
                .find(|pin| pin.path == owner::source::PROJECT_PATH)
                .map(|pin| &pin.digest)
        })
        .ok_or(ChangeError::WriteBoundaryDenied)?;
    if &input_pin(current.as_deref()).digest != expected {
        return Err(ChangeError::PreconditionFailed {
            subject: "error-registry".to_owned(),
            detail: "post-input-digest",
        });
    }
    validate_source(current.as_deref(), project)
}

fn output_type(reference: &ir::TypeRef) -> Result<owner::TypeExpr, ChangeError> {
    Ok(match reference {
        ir::TypeRef::Ref(id) => owner::TypeExpr::Ref(owner::ErrorId::new(id.as_str()).ok_or(
            ChangeError::UnsupportedMember {
                operation: "operation-error:set".to_owned(),
                member: "error-output-type",
            },
        )?),
        ir::TypeRef::List(inner) => owner::TypeExpr::List(Box::new(output_type(inner)?)),
        ir::TypeRef::Optional(inner) => owner::TypeExpr::Optional(Box::new(output_type(inner)?)),
    })
}

pub(crate) fn prepare(
    load: &crate::loader::ProjectLoad,
    request: &ChangeRequest,
    options: &PrepareOptions<'_>,
) -> Result<PreparedChange, ChangeError> {
    let Operation::OperationErrorSet {
        target,
        errors,
        registry_digest,
    } = &request.operations[0]
    else {
        return Err(ChangeError::WriteBoundaryDenied);
    };
    let before = read_source(&load.root)?.ok_or(ChangeError::PreconditionFailed {
        subject: "error-registry".to_owned(),
        detail: "project-registry-missing",
    })?;
    if receipt::bytes_digest(&before) != *registry_digest {
        return Err(ChangeError::PreconditionFailed {
            subject: "error-registry".to_owned(),
            detail: "registry-digest",
        });
    }
    let registry = ErrorRegistry::from_bytes(&before)
        .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    let compiled = ir::compile(&load.model)
        .map_err(|failure| ChangeError::LoadFailed(failure.into_result()))?;
    validate_source(Some(&before), &compiled.project)?;
    let definition = compiled
        .project
        .definitions
        .iter()
        .find(|definition| definition.id().as_str() == target)
        .ok_or(ChangeError::SubjectConflict {
            subject: target.clone(),
            detail: "definition-missing",
        })?;
    let (kind, family, output) = match definition {
        ir::Definition::Command(_) => (
            owner::OperationKind::Command,
            "command",
            owner::OutputType::Unit,
        ),
        ir::Definition::Query(query) => (
            owner::OperationKind::Query,
            "query",
            query
                .returns
                .as_ref()
                .map(output_type)
                .transpose()?
                .map(owner::OutputType::Value)
                .unwrap_or(owner::OutputType::Unit),
        ),
        _ => {
            return Err(ChangeError::SubjectConflict {
                subject: target.clone(),
                detail: "wrong-kind",
            })
        }
    };
    let union = owner::ErrorUnion::new(
        errors
            .iter()
            .map(|id| owner::ErrorRef::new(id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?,
    )
    .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    let replacement = owner::OperationErrorContract::new(target, kind, output, union)
        .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    let mut bindings = registry
        .bindings()
        .iter()
        .filter(|binding| binding.operation().as_str() != target)
        .cloned()
        .collect::<Vec<_>>();
    bindings.push(replacement);
    let candidate = ErrorRegistry::from_parts(
        registry.errors().to_vec(),
        bindings,
        registry.tombstones().to_vec(),
    )
    .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    owner::source::validate(&candidate, &compiled.project)
        .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    let difference = owner::diff(&registry, &candidate)
        .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    let candidate_digest = receipt::bytes_digest(candidate.canonical_bytes().as_bytes());
    let mut prepared = source_fix::prepare_source_only(
        load,
        request.to_canonical_json_string().into_bytes(),
        ResolvedOperation {
            operation: "operation-error:set".to_owned(),
            subject: target.clone(),
            subject_family: family.to_owned(),
            status: "resolved".to_owned(),
            detail: Some("project-error-registry".to_owned()),
        },
        options.policy,
        options.allow_breaking,
        vec![],
    )?;
    if prepared
        .inputs
        .iter()
        .find(|pin| pin.path == owner::source::PROJECT_PATH)
        .map(|pin| pin.digest.as_str())
        != Some(registry_digest)
    {
        return Err(ChangeError::PreconditionFailed {
            subject: "error-registry".to_owned(),
            detail: "registry-digest",
        });
    }
    if let Some(expected) = &request.expect_model {
        let actual = receipt::bytes_digest(
            prepared
                .inputs
                .iter()
                .map(|pin| format!("{}\u{1f}{}\n", pin.path, pin.digest))
                .collect::<String>()
                .as_bytes(),
        );
        if *expected != actual {
            return Err(ChangeError::PreconditionFailed {
                subject: "model".to_owned(),
                detail: "expect-model",
            });
        }
    }
    let mut classes = std::collections::BTreeSet::new();
    for change in difference.changes() {
        let (kind, class) = match (change.kind(), change.class()) {
            (owner::ErrorChangeKind::BindingAdded, owner::DiffClass::NonBreaking) => {
                ("error.binding-added", "additive")
            }
            (owner::ErrorChangeKind::UnionMemberAdded, owner::DiffClass::Breaking) => {
                ("error.union-member-added", "source-breaking")
            }
            (owner::ErrorChangeKind::UnionMemberRemoved, owner::DiffClass::Breaking) => {
                ("error.union-member-removed", "source-breaking")
            }
            _ => return Err(ChangeError::WriteBoundaryDenied),
        };
        classes.insert(class.to_owned());
        prepared.semantic_diff.changes.push(plan::ChangeRecordWire {
            change_id: format!(
                "sha256:{}",
                crate::digest::sha256_hex(
                    format!(
                        "{kind}\n{}\n{registry_digest}\n{candidate_digest}",
                        change.path()
                    )
                    .as_bytes()
                )
            ),
            kind: kind.to_owned(),
            subject: SubjectWire {
                family: family.to_owned(),
                id: target.clone(),
                member: change
                    .path()
                    .split_once("/errors/")
                    .map(|(_, id)| id.to_owned()),
            },
            reasons: vec![format!("error-contract.{}", change.class().as_str())],
        });
    }
    prepared.semantic_diff.equal = difference.equal();
    prepared.semantic_diff.classification = plan::CLASS_ORDER
        .iter()
        .filter(|class| classes.contains(**class))
        .map(|class| (*class).to_owned())
        .collect();
    if classes.contains("source-breaking")
        && !options.allow_breaking
        && !prepared.policy.allows("source-breaking")
    {
        return Err(ChangeError::CompatibilityDenied {
            classes: vec!["source-breaking".to_owned()],
        });
    }
    if !difference.equal() {
        let after = candidate.canonical_bytes().as_bytes().to_vec();
        prepared.file_edits.push(FileEdit {
            path: owner::source::PROJECT_PATH.to_owned(),
            before_digest: registry_digest.clone(),
            after_digest: receipt::bytes_digest(&after),
        });
        if before.len().max(after.len()) <= MAX_FILE_DIFF_BYTES {
            let before =
                std::str::from_utf8(&before).map_err(|_| ChangeError::WriteBoundaryDenied)?;
            let after =
                std::str::from_utf8(&after).map_err(|_| ChangeError::WriteBoundaryDenied)?;
            let (patch, complete) = plan::compute_patch(before, after);
            prepared.diff_complete = complete;
            prepared.file_diffs.push(FileDiff {
                path: owner::source::PROJECT_PATH.to_owned(),
                before: before.to_owned(),
                after: after.to_owned(),
                patch,
            });
        } else {
            prepared.diff_complete = false;
        }
        prepared
            .edits
            .push((owner::source::PROJECT_PATH.to_owned(), after));
        prepared.impact.seeds.push(SeedEntry {
            family: family.to_owned(),
            id: target.clone(),
            member: None,
            side: "candidate",
        });
        let reached = crate::graph::build(&compiled.project)
            .ok()
            .and_then(|graph| {
                graph
                    .resolve_id(target)
                    .map(|root| super::reverse_closure(&graph, root))
            });
        match reached {
            Some(reached) => {
                prepared.impact.complete = reached.complete;
                prepared.impact.affected.push(AffectedEntry {
                    subject: target.clone(),
                    subject_family: family.to_owned(),
                    direction: "candidate",
                    via: reached.ids.into_iter().collect(),
                });
            }
            None => prepared.impact.complete = false,
        }
    }
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::transaction;

    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &to.join(entry.file_name()));
            } else {
                std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
            }
        }
    }

    #[test]
    fn failed_error_post_validation_rolls_back_or_retains_repairable_custody() {
        for drift in [false, true] {
            let root = tempfile::tempdir().unwrap();
            copy(
                &Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/model-v1/valid-planner"),
                root.path(),
            );
            let source = root.path().join(owner::source::PROJECT_PATH);
            let before = owner::REGISTRY_BYTES;
            std::fs::write(&source, before).unwrap();
            let load = crate::loader::load_validated_root(root.path().to_path_buf()).unwrap();
            let bytes = serde_json::to_vec(&serde_json::json!({
                "schemaVersion":"lekalo/change-request/v0.8.0",
                "operations":[{"operation":"operation-error:set","operationTarget":"planner.list_tasks",
                    "definition":{"errors":["planner.store_unavailable"],"registryDigest":receipt::bytes_digest(before)}}]
            })).unwrap();
            let request = ChangeRequest::decode(&bytes).unwrap();
            let prepared = prepare(
                &load,
                &request,
                &PrepareOptions {
                    project: None,
                    request: &bytes,
                    policy: None,
                    allow_breaking: true,
                },
            )
            .unwrap();
            let after = prepared.edits[0].1.clone();
            let error = super::super::apply_with_validation(
                &prepared,
                &ApplyOptions {
                    idempotency_key: "error-post-failure",
                    expect_plan: None,
                    allow_breaking: true,
                },
                &|_| {
                    if drift {
                        std::fs::write(&source, b"retained external edit").unwrap();
                    }
                    Err(ChangeError::WriteBoundaryDenied)
                },
            )
            .unwrap_err();
            if drift {
                let ChangeError::RecoveryRequired { transaction } = error else {
                    panic!("rollback must identify its pending transaction: {error:?}");
                };
                assert_eq!(std::fs::read(&source).unwrap(), b"retained external edit");
                assert!(transaction::recover(root.path(), &transaction).is_err());
                std::fs::write(&source, after).unwrap();
                assert!(
                    !transaction::recover(root.path(), &transaction)
                        .unwrap()
                        .committed
                );
            } else {
                assert!(
                    matches!(
                        error,
                        ChangeError::CommitFailed {
                            detail: "post-validation",
                            ..
                        }
                    ),
                    "{error:?}"
                );
            }
            assert_eq!(std::fs::read(&source).unwrap(), before);
            assert!(!root.path().join(version::HISTORY_LOGICAL_PATH).exists());
            assert!(!root.path().join(custody::LOGICAL_PATH).exists());
        }
    }
}
