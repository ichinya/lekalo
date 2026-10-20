//! Explicit, revision-bound confirmation through the observed binding owner.
//! Only the fixed observed-index attachment is writable; native source is
//! evidence, never a destination. No adapter or inference executes here.

use std::path::Path;

use super::{plan, receipt, ChangeError, ChangeRequest, Operation, PrepareOptions, PreparedChange};
use crate::project_fs::{Fs, FsErrorKind};

pub(crate) const INDEX_PATH: &str = ".lekalo/import/observed/index.json";

pub(crate) fn require_target(root: &Path, target: &str) -> Result<(), ChangeError> {
    let fs = Fs::open(root).map_err(|_| ChangeError::WriteBoundaryDenied)?;
    let entries = fs
        .entries("lekalo/targets")
        .map_err(|_| conflict("target-missing"))?;
    if entries.len() > 64 {
        return Err(conflict("target-inventory-limit"));
    }
    let mut matches = 0;
    for (name, kind) in entries {
        if kind != crate::project_fs::EntryType::File || !name.ends_with(".yaml") {
            continue;
        }
        let bytes = fs
            .read_file_opt("lekalo/targets", &name, 1024 * 1024)
            .map_err(|_| conflict("target-unreadable"))?
            .ok_or_else(|| conflict("target-missing"))?;
        let declared = super::request::unique_json(&bytes)
            .ok()
            .and_then(|value| {
                value
                    .get("target")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .or_else(|| super::serde_yaml_strict(&bytes));
        if declared.as_deref() == Some(target) {
            matches += 1;
        }
    }
    if matches != 1 {
        return Err(conflict("target-missing-or-ambiguous"));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) struct SealedConfirmation {
    pub index: plan::InputPin,
    pub native: plan::InputPin,
    pub after: String,
}

impl SealedConfirmation {
    pub fn is_input(&self, path: &str) -> bool {
        path == self.index.path || path == self.native.path
    }

    pub fn validate_after(&self, root: &Path) -> Result<(), ChangeError> {
        let fs = Fs::open(root).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let index = read_index(&fs)
            .map_err(|_| ChangeError::WriteBoundaryDenied)?
            .ok_or(ChangeError::WriteBoundaryDenied)?;
        if receipt::bytes_digest(&index) != self.after
            || crate::observed::index::parse_index(&index).is_err()
            || read_native(&fs, &self.native.path).map(|bytes| receipt::bytes_digest(&bytes))?
                != self.native.digest
        {
            return Err(conflict("confirmation-drift"));
        }
        Ok(())
    }
}

fn conflict(detail: &'static str) -> ChangeError {
    ChangeError::PreconditionFailed {
        subject: "binding".to_owned(),
        detail,
    }
}

pub(crate) fn read_index(fs: &Fs) -> Result<Option<Vec<u8>>, FsErrorKind> {
    fs.read_file_opt(
        ".lekalo/import/observed",
        "index.json",
        crate::observed::version::MAX_INDEX_BYTES,
    )
}

pub(crate) fn read_native(fs: &Fs, path: &str) -> Result<Vec<u8>, ChangeError> {
    // The path comes from a selected owner-derived candidate, still treated
    // as untrusted persisted input at the read boundary.
    if path.len() > 256
        || crate::project_fs::path_violation(path).is_some()
        || path.starts_with(".lekalo/")
        || path.starts_with("lekalo/")
    {
        return Err(ChangeError::WriteBoundaryDenied);
    }
    let (directory, name) = path.rsplit_once('/').unwrap_or(("", path));
    fs.read_file_opt(directory, name, 16 * 1024 * 1024)
        .map_err(|_| conflict("native-unreadable"))?
        .ok_or_else(|| conflict("native-missing"))
}

pub(crate) fn prepare_confirmation(
    load: &crate::loader::ProjectLoad,
    request: &ChangeRequest,
    options: &PrepareOptions<'_>,
) -> Result<PreparedChange, ChangeError> {
    let Operation::BindingConfirm {
        symbol,
        proposal,
        candidate,
        index_digest,
    } = &request.operations[0]
    else {
        return Err(ChangeError::WriteBoundaryDenied);
    };
    let module = symbol
        .split('.')
        .next()
        .ok_or(ChangeError::WriteBoundaryDenied)?;
    if super::definition_kind_of(super::find_definition(&load.documents, module)?) != "module" {
        return Err(ChangeError::WriteBoundaryDenied);
    }
    let fs = Fs::open(&load.root).map_err(|_| ChangeError::WriteBoundaryDenied)?;
    let before = read_index(&fs)
        .map_err(|_| conflict("index-unreadable"))?
        .ok_or_else(|| conflict("index-missing"))?;
    if receipt::bytes_digest(&before) != *index_digest {
        return Err(conflict("index-digest"));
    }
    let ctx = crate::observed::ObservedContext {
        model: load.model.clone(),
        root: load.root.clone(),
    };
    let index = crate::observed::index::load_index(&ctx)
        .map_err(|set| ChangeError::LoadFailed(crate::DomainResult::invalid(set)))?
        .ok_or_else(|| conflict("index-missing"))?;
    if crate::observed::index::index_digest(&index).ok().as_deref() != Some(index_digest) {
        return Err(conflict("index-digest"));
    }
    let offer = crate::observed::bindings::proposals_of(&index)
        .into_iter()
        .find(|offer| offer.proposal == *proposal && offer.symbol == *symbol)
        .ok_or_else(|| conflict("proposal-missing"))?;
    let record = index
        .symbols
        .iter()
        .find(|record| record.id == *symbol)
        .ok_or_else(|| conflict("symbol-missing"))?;
    if record.state == crate::observed::types::BindingState::Stale {
        return Err(conflict("binding-stale"));
    }
    let chosen = crate::observed::bindings::ranked_candidates(record)
        .into_iter()
        .find(|entry| entry.native == *candidate)
        .ok_or_else(|| conflict("candidate-missing"))?;
    let native_bytes = read_native(&fs, &chosen.path)?;
    let native_digest = receipt::bytes_digest(&native_bytes);
    if chosen.fingerprint.as_deref() != Some(&native_digest) {
        return Err(conflict("native-fingerprint"));
    }
    let mut owned = index;
    crate::observed::bindings::apply_confirmation(&mut owned, &offer, &chosen)
        .ok_or_else(|| conflict("proposal-missing"))?;
    let after = crate::observed::index::canonical_bytes(&owned)
        .map_err(|set| ChangeError::LoadFailed(crate::DomainResult::invalid(set)))?
        .into_bytes();
    let mut prepared = super::source_fix::prepare_source_only(
        load,
        request.to_canonical_json_string().into_bytes(),
        plan::ResolvedOperation {
            operation: "binding:add".to_owned(),
            subject: symbol.clone(),
            subject_family: offer.kind.to_owned(),
            status: "resolved".to_owned(),
            detail: Some("observed-confirmation".to_owned()),
        },
        options.policy,
        options.allow_breaking,
        vec![],
    )?;
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
            return Err(conflict("expect-model"));
        }
    }
    let sealed = SealedConfirmation {
        index: plan::InputPin {
            path: INDEX_PATH.to_owned(),
            digest: index_digest.clone(),
        },
        native: plan::InputPin {
            path: chosen.path,
            digest: native_digest,
        },
        after: receipt::bytes_digest(&after),
    };
    prepared
        .inputs
        .extend([sealed.index.clone(), sealed.native.clone()]);
    prepared.inputs.sort_by(|a, b| a.path.cmp(&b.path));
    if prepared.inputs.len() > super::version::MAX_PLAN_INPUTS {
        return Err(ChangeError::PlanExportLimit {
            subject: "binding-inputs",
            measured: prepared.inputs.len() as u64,
            allowed: super::version::MAX_PLAN_INPUTS as u64,
        });
    }
    prepared.file_edits.push(plan::FileEdit {
        path: INDEX_PATH.to_owned(),
        before_digest: index_digest.clone(),
        after_digest: sealed.after.clone(),
    });
    if before.len().max(after.len()) <= super::MAX_FILE_DIFF_BYTES {
        let before = std::str::from_utf8(&before).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let after = std::str::from_utf8(&after).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let (patch, complete) = plan::compute_patch(before, after);
        prepared.diff_complete = complete;
        prepared.file_diffs.push(plan::FileDiff {
            path: INDEX_PATH.to_owned(),
            before: before.to_owned(),
            after: after.to_owned(),
            patch,
        });
    } else {
        prepared.diff_complete = false;
    }
    prepared.edits.push((INDEX_PATH.to_owned(), after));
    prepared.impact.complete = false;
    prepared.binding = Some(sealed);
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::transaction::{self, ReplacementPlan, Transaction};

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
    fn interrupted_binding_index_and_audit_publication_recovers_as_one_set() {
        for phase in 0..=3 {
            let root = tempfile::tempdir().unwrap();
            copy(
                &Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/change/field-vertical"),
                root.path(),
            );
            let index = root.path().join(INDEX_PATH);
            std::fs::create_dir_all(index.parent().unwrap()).unwrap();
            let before = std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/observed/golden/task-domain.index.json"),
            )
            .unwrap();
            std::fs::write(&index, &before).unwrap();
            let paths = [
                INDEX_PATH,
                crate::change::version::HISTORY_LOGICAL_PATH,
                crate::change::custody::LOGICAL_PATH,
            ];
            let plans: Vec<_> = paths
                .iter()
                .enumerate()
                .map(|(i, path)| {
                    let after = format!("owned staged bytes {i}").into_bytes();
                    ReplacementPlan {
                        logical: (*path).to_owned(),
                        before_digest: receipt::bytes_digest(if i == 0 { &before } else { b"" }),
                        after_digest: receipt::bytes_digest(&after),
                        after_bytes: after,
                        creation: i != 0,
                    }
                })
                .collect();
            let mut tx = Transaction::begin(
                root.path(),
                "change-binding-boundary",
                &format!("sha256:{}", "a".repeat(64)),
                &plans,
            )
            .unwrap();
            for (i, plan) in plans.iter().enumerate() {
                tx.stage(i, plan).unwrap();
            }
            tx.mark_state(transaction::STATE_REPLACING).unwrap();
            for plan in plans.iter().take(phase) {
                std::fs::write(root.path().join(&plan.logical), &plan.after_bytes).unwrap();
            }
            drop(tx);
            if phase == 3 {
                std::fs::write(&index, b"retained later edit").unwrap();
                assert!(transaction::recover(root.path(), "change-binding-boundary").is_err());
                assert_eq!(
                    std::fs::read(root.path().join(paths[1])).unwrap(),
                    plans[1].after_bytes
                );
                assert_eq!(std::fs::read(&index).unwrap(), b"retained later edit");
                std::fs::write(&index, &plans[0].after_bytes).unwrap();
            }
            assert!(
                !transaction::recover(root.path(), "change-binding-boundary")
                    .unwrap()
                    .committed
            );
            assert_eq!(std::fs::read(&index).unwrap(), before);
            assert!(!root.path().join(paths[1]).exists());
            assert!(!root.path().join(paths[2]).exists());
        }
    }
}
