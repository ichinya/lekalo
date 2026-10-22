//! Module policy transitions reuse the managed owner and its admission gates.
//! Only one scalar in the fixed project policy is written. Coverage, adapter
//! and extension evidence are read-only, pinned inputs of the transaction.

use std::collections::BTreeMap;

use super::*;
use crate::managed::{self as owner, Mode, Ownership};
use request::OwnershipCoverage;

#[derive(Clone, Debug)]
pub(crate) struct SealedOwnership {
    module: String,
    mode: Mode,
    coverage: Vec<OwnershipCoverage>,
    pins: Vec<InputPin>,
    after_digest: String,
}

fn domain(result: DomainResult) -> ChangeError {
    ChangeError::LoadFailed(result)
}

fn stale(subject: &str, detail: &'static str) -> ChangeError {
    ChangeError::PreconditionFailed {
        subject: subject.to_owned(),
        detail,
    }
}

fn read_source(root: &Path) -> Result<(Vec<u8>, Ownership), ChangeError> {
    let bytes = owner::source::read_bytes(root)
        .map_err(domain)?
        .ok_or_else(|| stale("ownership", "project-policy-missing"))?;
    let document = owner::parse_ownership(&bytes).map_err(domain)?;
    Ok((bytes, document))
}

fn native_path(path: &str) -> Result<(), ChangeError> {
    if path.len() > 256
        || !owner::rel_path(path)
        || !owner::credential_free(path)
        || path.starts_with("lekalo/")
        || path == "lekalo.lock"
    {
        return Err(ChangeError::UnsupportedMember {
            operation: "ownership:set".to_owned(),
            member: "evidence-home",
        });
    }
    Ok(())
}

fn read_native(root: &Path, path: &str) -> Result<Vec<u8>, ChangeError> {
    native_path(path)?;
    let fs = crate::project_fs::Fs::open(root).map_err(|_| stale(path, "input-read"))?;
    let (directory, name) = path.rsplit_once('/').unwrap_or(("", path));
    fs.read_file_opt(directory, name, owner::wire::MAX_BYTES)
        .map_err(|_| stale(path, "input-read"))?
        .ok_or_else(|| stale(path, "input-missing"))
}

fn insert_pin(
    pins: &mut BTreeMap<String, String>,
    path: &str,
    digest: String,
) -> Result<(), ChangeError> {
    if pins.get(path).is_some_and(|before| *before != digest) {
        return Err(stale(path, "evidence-conflict"));
    }
    pins.insert(path.to_owned(), digest);
    Ok(())
}

/// The managed owner's admission is recomputed over the exact loaded Model.
/// Every mutable non-Model read also becomes an explicit byte pin.
fn proofs(
    load: &crate::loader::ProjectLoad,
    policy: &Ownership,
    module: &str,
    coverage: &[OwnershipCoverage],
) -> Result<Vec<InputPin>, ChangeError> {
    let compiled = ir::compile(&load.model).map_err(|failure| domain(failure.into_result()))?;
    owner::source::validate_module(policy, &compiled.project, module).map_err(domain)?;
    let assignment = policy
        .modules
        .iter()
        .find(|assignment| assignment.module == module)
        .ok_or_else(|| ChangeError::SubjectConflict {
            subject: module.to_owned(),
            detail: "ownership-assignment-missing",
        })?;
    if assignment.mode != Mode::Managed {
        if !coverage.is_empty() {
            return Err(ChangeError::RequestInvalid {
                detail: "coverage-without-managed",
            });
        }
        return Ok(Vec::new());
    }
    if coverage.len() != assignment.targets.len()
        || assignment
            .targets
            .iter()
            .any(|target| !coverage.iter().any(|row| row.target == target.target))
    {
        return Err(domain(owner::diagnostic::failure(
            owner::CAPABILITY_GAP,
            &[("module", module), ("reason", "coverage-targets")],
        )));
    }
    let authority = owner::authority::derive_loaded(load, module).map_err(domain)?;
    let mut pins = BTreeMap::new();
    for target in &assignment.targets {
        let row = coverage
            .iter()
            .find(|row| row.target == target.target)
            .ok_or(ChangeError::WriteBoundaryDenied)?;
        let bytes = read_native(&load.root, &row.path)?;
        if receipt::bytes_digest(&bytes) != row.digest {
            return Err(stale(&row.path, "coverage-digest"));
        }
        let document = owner::parse_coverage(&bytes).map_err(domain)?;
        native_path(&document.adapter.entry)?;
        for extension in &document.extensions {
            let path = owner::bindings::binding_file(&extension.binding)
                .ok_or(ChangeError::WriteBoundaryDenied)?;
            native_path(path)?;
        }
        owner::coverage::admit(assignment, target, &document).map_err(domain)?;
        owner::authority::qualify(&load.root, &document, &authority).map_err(domain)?;
        owner::authority::ledger_complete(&document, &authority).map_err(domain)?;
        owner::authority::resolve_target_provenance(target, &authority).map_err(domain)?;
        owner::bindings::resolve(&load.root, target, &document).map_err(domain)?;
        insert_pin(&mut pins, &row.path, row.digest.clone())?;
        let entry = read_native(&load.root, &document.adapter.entry)?;
        let digest = receipt::bytes_digest(&entry);
        if authority
            .adapter
            .as_ref()
            .map(|adapter| adapter.entry_digest.as_str())
            != Some(digest.as_str())
        {
            return Err(stale(&document.adapter.entry, "adapter-digest"));
        }
        insert_pin(&mut pins, &document.adapter.entry, digest)?;
        for extension in &document.extensions {
            let path = owner::bindings::binding_file(&extension.binding)
                .ok_or(ChangeError::WriteBoundaryDenied)?;
            let bytes = read_native(&load.root, path)?;
            let digest = receipt::bytes_digest(&bytes);
            if digest != extension.contract_digest {
                return Err(stale(path, "extension-digest"));
            }
            insert_pin(&mut pins, path, digest)?;
        }
    }
    Ok(pins
        .into_iter()
        .map(|(path, digest)| InputPin { path, digest })
        .collect())
}

impl SealedOwnership {
    pub(crate) fn is_input(&self, path: &str) -> bool {
        self.pins.iter().any(|pin| pin.path == path)
    }

    pub(crate) fn input_digest(&self, root: &Path, path: &str) -> Result<String, ChangeError> {
        let bytes = if path == owner::source::PROJECT_PATH {
            read_source(root)?.0
        } else {
            read_native(root, path)?
        };
        Ok(receipt::bytes_digest(&bytes))
    }

    fn validate(&self, load: &crate::loader::ProjectLoad, after: bool) -> Result<(), ChangeError> {
        let (bytes, mut policy) = read_source(&load.root)?;
        let expected = if after {
            &self.after_digest
        } else {
            &self
                .pins
                .iter()
                .find(|pin| pin.path == owner::source::PROJECT_PATH)
                .ok_or(ChangeError::WriteBoundaryDenied)?
                .digest
        };
        if receipt::bytes_digest(&bytes) != *expected {
            return Err(stale("ownership", "policy-digest"));
        }
        let assignment = policy
            .modules
            .iter_mut()
            .find(|assignment| assignment.module == self.module)
            .ok_or(ChangeError::WriteBoundaryDenied)?;
        if after && assignment.mode != self.mode {
            return Err(stale("ownership", "post-mode"));
        }
        assignment.mode = self.mode;
        let current = proofs(load, &policy, &self.module, &self.coverage)?;
        let expected: Vec<InputPin> = self
            .pins
            .iter()
            .filter(|pin| pin.path != owner::source::PROJECT_PATH)
            .cloned()
            .collect();
        if current != expected {
            return Err(stale("ownership-evidence", "input-inventory"));
        }
        Ok(())
    }

    pub(crate) fn validate_before(
        &self,
        load: &crate::loader::ProjectLoad,
    ) -> Result<(), ChangeError> {
        self.validate(load, false)
    }
    pub(crate) fn validate_after(
        &self,
        load: &crate::loader::ProjectLoad,
    ) -> Result<(), ChangeError> {
        self.validate(load, true)
    }
}

pub(crate) fn prepare(
    load: &crate::loader::ProjectLoad,
    request: &ChangeRequest,
    options: &PrepareOptions<'_>,
) -> Result<PreparedChange, ChangeError> {
    let Operation::OwnershipSet {
        subject,
        mode,
        ownership_digest,
        coverage,
    } = &request.operations[0]
    else {
        return Err(ChangeError::WriteBoundaryDenied);
    };
    if !field_name_valid(subject) {
        return Err(ChangeError::UnsupportedMember {
            operation: "ownership:set".to_owned(),
            member: "symbol-ownership",
        });
    }
    let (before, policy) = read_source(&load.root)?;
    if receipt::bytes_digest(&before) != *ownership_digest {
        return Err(stale("ownership", "policy-digest"));
    }
    let mut candidate = policy.clone();
    let assignment = candidate
        .modules
        .iter_mut()
        .find(|assignment| assignment.module == *subject)
        .ok_or_else(|| ChangeError::SubjectConflict {
            subject: subject.clone(),
            detail: "ownership-assignment-missing",
        })?;
    let changed = assignment.mode != *mode;
    assignment.mode = *mode;
    let proof_pins = proofs(load, &candidate, subject, coverage)?;
    let after = if changed {
        let text = std::str::from_utf8(&before).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let parsed = crate::loader::frontends::parse_document(
            text,
            &crate::loader::source::LineIndex::new(text),
        )
        .map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let crate::loader::frontends::Parsed::Root(root) = parsed else {
            return Err(ChangeError::WriteBoundaryDenied);
        };
        let node = root
            .get("modules")
            .and_then(Node::as_seq)
            .and_then(|rows| {
                rows.iter()
                    .find(|row| row.get("module").and_then(Node::as_str) == Some(subject.as_str()))
            })
            .and_then(|row| row.get("mode"))
            .ok_or(ChangeError::WriteBoundaryDenied)?;
        let edit = ByteEdit {
            start: node.span.start.byte,
            end: node.span.end.byte,
            replacement: serde_json::to_string(mode)
                .map_err(|_| ChangeError::WriteBoundaryDenied)?,
        };
        let after = edit.apply(&before);
        if owner::parse_ownership(&after).map_err(domain)? != candidate {
            return Err(ChangeError::SourcePreservation {
                subject: owner::source::PROJECT_PATH.to_owned(),
                detail: "reparse-mismatch",
            });
        }
        after
    } else {
        before.clone()
    };
    let mut prepared = source_fix::prepare_source_only(
        load,
        request.to_canonical_json_string().into_bytes(),
        ResolvedOperation {
            operation: "ownership:set".to_owned(),
            subject: subject.clone(),
            subject_family: "module".to_owned(),
            status: "resolved".to_owned(),
            detail: Some("managed-ownership-policy".to_owned()),
        },
        options.policy,
        options.allow_breaking,
        Vec::new(),
    )?;
    let mut pins = proof_pins;
    pins.push(InputPin {
        path: owner::source::PROJECT_PATH.to_owned(),
        digest: ownership_digest.clone(),
    });
    pins.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    prepared.inputs.extend(pins.iter().cloned());
    prepared
        .inputs
        .sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    if prepared.inputs.len() > version::MAX_PLAN_INPUTS {
        return Err(ChangeError::PlanExportLimit {
            subject: "inputs",
            measured: prepared.inputs.len() as u64,
            allowed: version::MAX_PLAN_INPUTS as u64,
        });
    }
    if let Some(expected) = &request.expect_model {
        let current = pin_inventory_digest(&prepared.inputs);
        if *expected != current {
            return Err(stale("model", "expect-model"));
        }
    }
    let after_digest = receipt::bytes_digest(&after);
    if changed {
        if !options.allow_breaking && !prepared.policy.allows("source-breaking") {
            return Err(ChangeError::CompatibilityDenied {
                classes: vec!["source-breaking".to_owned()],
            });
        }
        prepared.semantic_diff.equal = false;
        prepared.semantic_diff.classification = vec!["source-breaking".to_owned()];
        prepared.semantic_diff.changes.push(plan::ChangeRecordWire {
            change_id: receipt::bytes_digest(
                format!("ownership.mode-changed\n{subject}\n{ownership_digest}\n{after_digest}")
                    .as_bytes(),
            ),
            kind: "ownership.mode-changed".to_owned(),
            subject: SubjectWire {
                family: "module".to_owned(),
                id: subject.clone(),
                member: None,
            },
            reasons: vec!["ownership.policy-transition".to_owned()],
        });
        prepared.file_edits.push(FileEdit {
            path: owner::source::PROJECT_PATH.to_owned(),
            before_digest: ownership_digest.clone(),
            after_digest: after_digest.clone(),
        });
        if before.len().max(after.len()) <= MAX_FILE_DIFF_BYTES {
            let before_text =
                std::str::from_utf8(&before).map_err(|_| ChangeError::WriteBoundaryDenied)?;
            let after_text =
                std::str::from_utf8(&after).map_err(|_| ChangeError::WriteBoundaryDenied)?;
            let (patch, complete) = plan::compute_patch(before_text, after_text);
            prepared.diff_complete = complete;
            prepared.file_diffs.push(FileDiff {
                path: owner::source::PROJECT_PATH.to_owned(),
                before: before_text.to_owned(),
                after: after_text.to_owned(),
                patch,
            });
        } else {
            prepared.diff_complete = false;
        }
        prepared
            .edits
            .push((owner::source::PROJECT_PATH.to_owned(), after));
        prepared.impact.seeds.push(SeedEntry {
            family: "module".to_owned(),
            id: subject.clone(),
            member: None,
            side: "candidate",
        });
        let compiled = ir::compile(&load.model).map_err(|failure| domain(failure.into_result()))?;
        if let Some(reached) = crate::graph::build(&compiled.project)
            .ok()
            .and_then(|graph| {
                graph
                    .resolve_id(subject)
                    .map(|root| super::reverse_closure(&graph, root))
            })
        {
            prepared.impact.complete = reached.complete;
            prepared.impact.affected.push(AffectedEntry {
                subject: subject.clone(),
                subject_family: "module".to_owned(),
                direction: "candidate",
                via: reached.ids.into_iter().collect(),
            });
        } else {
            prepared.impact.complete = false;
        }
    }
    prepared.ownership = Some(SealedOwnership {
        module: subject.clone(),
        mode: *mode,
        coverage: coverage.clone(),
        pins,
        after_digest,
    });
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(source: &Path, target: &Path) {
        std::fs::create_dir_all(target).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &target.join(entry.file_name()));
            } else {
                std::fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
            }
        }
    }

    #[test]
    fn policy_post_validation_failure_restores_bytes_or_retains_foreign_edits_behind_custody() {
        for drift in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            copy(
                &repo.join("tests/fixtures/change/field-vertical"),
                root.path(),
            );
            let mut policy: Json = serde_json::from_slice(
                &std::fs::read(repo.join("tests/fixtures/managed/planner/ownership.json")).unwrap(),
            )
            .unwrap();
            policy["modules"][0]["module"] = serde_json::json!("tasks");
            policy["modules"][0]["mode"] = serde_json::json!("observed");
            let before = serde_json::to_vec_pretty(&policy).unwrap();
            let path = root.path().join(owner::source::PROJECT_PATH);
            std::fs::write(&path, &before).unwrap();
            let load = crate::loader::load_validated_root(root.path().to_path_buf()).unwrap();
            let bytes = serde_json::to_vec(&serde_json::json!({"schemaVersion":"lekalo/change-request/v0.8.0","operations":[{"operation":"ownership:set","subject":"tasks","mode":"contracted","evidence":{"ownershipDigest":receipt::bytes_digest(&before)}}]})).unwrap();
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
            let failure = super::super::apply_with_validation(
                &prepared,
                &super::super::ApplyOptions {
                    idempotency_key: "ownership-post-failure",
                    expect_plan: None,
                    allow_breaking: true,
                },
                &|_| {
                    if drift {
                        std::fs::write(&path, b"retained external policy edit").unwrap();
                    }
                    Err(ChangeError::WriteBoundaryDenied)
                },
            )
            .unwrap_err();
            if drift {
                assert!(matches!(failure, ChangeError::RecoveryRequired { .. }));
                assert_eq!(
                    std::fs::read(&path).unwrap(),
                    b"retained external policy edit"
                );
            } else {
                assert!(matches!(failure, ChangeError::CommitFailed { .. }));
                assert_eq!(std::fs::read(&path).unwrap(), before);
            }
        }
    }
}
