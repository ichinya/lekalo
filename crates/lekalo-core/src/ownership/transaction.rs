//! One recoverable handoff. All before-images precede the pending barrier;
//! receipt publication is the commit point. Recovery never overwrites edits.
use super::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

const PENDING: &str = ".lekalo/generated/manifests/ownership-transitions/pending.json";

#[cfg(test)]
thread_local! { static STOP: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) }; }

#[cfg(test)]
fn checkpoint() -> Outcome<()> {
    STOP.with(|stop| match stop.get() {
        Some(0) => {
            stop.set(None);
            Err(fail("ownership-test-interruption"))
        }
        Some(left) => {
            stop.set(Some(left - 1));
            Ok(())
        }
        None => Ok(()),
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema_version: String,
    phase: String,
    plan: TransitionPlan,
    verification: Verification,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryView {
    pub schema_version: String,
    pub plan_digest: String,
    pub phase: String,
    pub files: Vec<String>,
}

fn key(pin: &str) -> Outcome<&str> {
    if !mg::pin(pin) {
        return Err(fail("ownership-digest"));
    }
    Ok(&pin[7..])
}
fn receipt_path(plan: &str) -> Outcome<String> {
    Ok(format!("{HOME}/receipts/{}", key(plan)?))
}
fn journal_path(plan: &str) -> Outcome<String> {
    Ok(format!("{HOME}/journals/{}", key(plan)?))
}
fn object_path(pin: &str) -> Outcome<String> {
    Ok(format!("{HOME}/objects/{}", key(pin)?))
}

fn object(root: &Path, pin: &str) -> Outcome<Vec<u8>> {
    let result = required(root, &object_path(pin)?)?;
    if mg::digest(&result) != pin {
        return Err(fail("ownership-snapshot-digest"));
    }
    Ok(result)
}

fn retain(root: &Path, value: &[u8]) -> Outcome<()> {
    let pin = mg::digest(value);
    let path = object_path(&pin)?;
    match read(root, &path)? {
        Some(current) if current != value => Err(fail("ownership-snapshot-conflict")),
        Some(_) => Ok(()),
        None => mg::fs::write_file(root, &path, value),
    }
}

fn lock(root: &Path) -> Outcome<crate::project_writer::WriterLock> {
    crate::project_writer::WriterLock::mutation(
        root,
        crate::project_writer::MutationFamily::Ownership,
    )
    .map_err(|_| fail("ownership-writer-locked"))
}

pub(crate) fn barrier(root: &Path) -> bool {
    // Unreadable/malformed is pending too; uncertainty never authorizes writes.
    !matches!(mg::fs::read_file(root, PENDING), Ok(None))
        || !matches!(state(root), Ok(value) if value.pending.is_none())
}

pub(crate) fn recovery_code(fs: &crate::project_fs::Fs) -> Option<&'static str> {
    match fs.read_file_opt("lekalo", "ownership-state.json", MAX_BYTES) {
        Ok(Some(value)) => match decode::<OwnershipState>(&value) {
            Ok(value) if value.pending.is_none() => (),
            _ => return Some("ownership.recovery-required"),
        },
        Ok(None) | Err(crate::project_fs::FsErrorKind::NotFound) => (),
        _ => return Some("ownership.recovery-required"),
    }
    match fs.read_file_opt(HOME, "pending.json", MAX_BYTES) {
        Ok(None) | Err(crate::project_fs::FsErrorKind::NotFound) => None,
        _ => Some("ownership.recovery-required"),
    }
}

fn state_edit(journal: &Journal) -> Outcome<&Edit> {
    journal
        .plan
        .edits
        .iter()
        .find(|edit| edit.path == STATE)
        .ok_or_else(|| fail("ownership-journal-state"))
}

fn anchor(root: &Path, journal: &Journal) -> Outcome<()> {
    let edit = state_edit(journal)?;
    let restoring = journal.phase == "restoring";
    if !matches(root, edit, !restoring)? {
        return Err(fail("ownership-publish-conflict"));
    }
    let mut current = state(root)?;
    current.pending = Some(PendingGrant {
        plan_digest: journal.plan.plan_digest.clone(),
        journal_digest: mg::digest(&bytes(journal)),
        phase: journal.phase.clone(),
        base_digest: if restoring {
            edit.after.clone()
        } else {
            edit.before.clone()
        },
    });
    mg::fs::write_file(root, STATE, &bytes(&current))
}

fn authorized(root: &Path, journal: &Journal) -> Outcome<()> {
    let edit = state_edit(journal)?;
    let mut current = state(root)?;
    let grant = current
        .pending
        .take()
        .ok_or_else(|| fail("ownership-journal-authority"))?;
    let restoring = journal.phase == "restoring";
    let expected = if restoring {
        &edit.content
    } else {
        &edit.before_content
    };
    let expected: OwnershipState = expected
        .as_ref()
        .map(|text| decode(text.as_bytes()))
        .transpose()?
        .unwrap_or_default();
    if grant.plan_digest != journal.plan.plan_digest
        || grant.journal_digest != mg::digest(&bytes(journal))
        || grant.phase != journal.phase
        || grant.base_digest
            != if restoring {
                edit.after.clone()
            } else {
                edit.before.clone()
            }
        || current != expected
    {
        return Err(fail("ownership-journal-authority"));
    }
    Ok(())
}

fn pending_marker(root: &Path, plan: &str) -> Outcome<()> {
    mg::fs::write_file(
        root,
        PENDING,
        &bytes(&serde_json::json!({"planDigest":plan})),
    )
}

fn publish_receipt(root: &Path, receipt: &mut Receipt) -> Outcome<()> {
    if let Some(previous) = read(root, &receipt_path(&receipt.plan_digest)?)? {
        retain(root, &previous)?;
        receipt.previous_receipt = Some(mg::digest(&previous));
    }
    mg::fs::write_file(root, &receipt_path(&receipt.plan_digest)?, &bytes(receipt))
}

fn load_journal(root: &Path, plan_digest: &str) -> Outcome<Journal> {
    let journal: Journal = decode(&required(root, &journal_path(plan_digest)?)?)?;
    if journal.schema_version != format!("lekalo/ownership-journal/v{VERSION}")
        || journal.plan.plan_digest != plan_digest
        || mg::plan_digest(&journal.plan) != plan_digest
        || journal.verification.plan_digest != plan_digest
        || journal.verification.verdict != "match"
        || journal.verification.runner_digest != journal.plan.request.runner.digest
        || journal.verification.counterexample.is_some()
        || journal.verification.observations
            != journal
                .plan
                .request
                .runner
                .expected
                .iter()
                .map(|row| {
                    let digest = mg::hash(row);
                    ObservationEvidence {
                        scenario: row.scenario.clone(),
                        baseline_digest: Some(digest.clone()),
                        candidate_digest: Some(digest.clone()),
                        expected_digest: digest,
                    }
                })
                .collect::<Vec<_>>()
        || !["publishing", "restoring"].contains(&journal.phase.as_str())
    {
        return Err(fail("ownership-journal"));
    }
    // Re-derive every mutation from the retained baseline. Editable journal rows
    // cannot invent write coordinates or smuggle arbitrary restoration bytes.
    let mut files = std::collections::BTreeMap::new();
    for pin in &journal.plan.inputs {
        if !path_ok(&pin.path) {
            return Err(fail("ownership-journal-path"));
        }
        files.insert(
            pin.path.clone(),
            pin.digest
                .as_deref()
                .map(|pin| object(root, pin))
                .transpose()?,
        );
    }
    let baseline = verification::snapshot(&files)?;
    if plan(baseline.path(), journal.plan.request.clone())? != journal.plan {
        return Err(fail("ownership-journal-derivation"));
    }
    Ok(journal)
}

fn receipt_matches(receipt: &Receipt, journal: &Journal) -> bool {
    receipt.plan_digest == journal.plan.plan_digest
        && receipt.project == journal.plan.project
        && receipt.module == journal.plan.request.module
        && receipt.symbol == journal.plan.request.symbol
        && receipt.from == journal.plan.from
        && receipt.to == journal.plan.request.to
        && receipt.verification == journal.verification
        && receipt.rollback == journal.plan.plan_digest
        && receipt.files
            == journal
                .plan
                .edits
                .iter()
                .map(|edit| Pin {
                    path: edit.path.clone(),
                    digest: edit.after.clone(),
                })
                .collect::<Vec<_>>()
}

fn matches(root: &Path, edit: &Edit, restored: bool) -> Outcome<bool> {
    let actual = read(root, &edit.path)?.as_deref().map(mg::digest);
    Ok(actual
        == if restored {
            edit.before.clone()
        } else {
            edit.after.clone()
        })
}

fn check_unchanged_inputs(root: &Path, journal: &Journal) -> Outcome<()> {
    for pin in &journal.plan.inputs {
        if !journal.plan.edits.iter().any(|edit| edit.path == pin.path)
            && read(root, &pin.path)?.as_deref().map(mg::digest) != pin.digest
        {
            return Err(fail("ownership-rollback-conflict"));
        }
    }
    Ok(())
}

fn restore(root: &Path, journal: &Journal) -> Outcome<()> {
    authorized(root, journal)?;
    check_unchanged_inputs(root, journal)?;
    // Preflight the complete cohort before restoring even the first file.
    for edit in &journal.plan.edits {
        if edit.path == STATE {
            continue;
        }
        if !matches(root, edit, false)? && !matches(root, edit, true)? {
            return Err(fail("ownership-rollback-conflict"));
        }
        if let Some(pin) = &edit.before {
            object(root, pin)?;
        }
    }
    for edit in journal.plan.edits.iter().rev() {
        if edit.path == STATE {
            continue;
        }
        if matches(root, edit, true)? {
            continue;
        }
        if !matches(root, edit, false)? {
            return Err(fail("ownership-rollback-conflict"));
        }
        if let Some(pin) = &edit.before {
            mg::fs::write_file(root, &edit.path, &object(root, pin)?)?;
        } else {
            mg::fs::remove_file(root, &edit.path)?;
        }
        #[cfg(test)]
        checkpoint()?;
    }
    // Retain the canonical grant until the entire cohort has been restored.
    authorized(root, journal)?;
    let edit = state_edit(journal)?;
    if let Some(pin) = &edit.before {
        mg::fs::write_file(root, STATE, &object(root, pin)?)?;
    } else {
        mg::fs::remove_file(root, STATE)?;
    }
    #[cfg(test)]
    checkpoint()?;
    Ok(())
}

pub fn apply(
    root: &Path,
    saved: &TransitionPlan,
    confirm: &str,
    host: &dyn VerificationHost,
) -> Outcome<Receipt> {
    if confirm != saved.plan_digest || mg::plan_digest(saved) != saved.plan_digest {
        return Err(fail("ownership-confirmation"));
    }
    let _guard = lock(root)?;
    if barrier(root) {
        return Err(fail("ownership-recovery-required"));
    }
    if let Some(value) = read(root, &receipt_path(confirm)?)? {
        let mut receipt: Receipt = decode(&value)?;
        if receipt.plan_digest != confirm {
            return Err(fail("ownership-replay-conflict"));
        }
        if receipt.state == "committed" {
            let journal = load_journal(root, confirm)?;
            if journal.plan != *saved
                || !receipt_matches(&receipt, &journal)
                || check_unchanged_inputs(root, &journal).is_err()
                || saved
                    .edits
                    .iter()
                    .any(|edit| !matches(root, edit, false).unwrap_or(false))
            {
                return Err(fail("ownership-replay-conflict"));
            }
            receipt.state = "replayed".to_owned();
            return Ok(receipt);
        }
        if receipt.state != "rolled-back" {
            return Err(fail("ownership-replay-conflict"));
        }
    }
    revalidate(root, saved)?;
    let verified = verify(root, saved, host)?;
    if verified.verdict != "match" {
        return Err(fail("ownership-equivalence-failed"));
    }
    revalidate(root, saved)?;
    // Retain every consumed preimage, including untouched Model/adapter files,
    // so independent recovery can re-derive the exact prepared plan offline.
    for pin in &saved.inputs {
        let current = read(root, &pin.path)?;
        if current.as_deref().map(mg::digest) != pin.digest {
            return Err(fail("ownership-plan-stale"));
        }
        if let Some(value) = current {
            retain(root, &value)?;
        }
    }
    revalidate(root, saved)?;
    let journal = Journal {
        schema_version: format!("lekalo/ownership-journal/v{VERSION}"),
        phase: "publishing".to_owned(),
        plan: saved.clone(),
        verification: verified.clone(),
    };
    if bytes(&journal).len() > MAX_BYTES {
        return Err(fail("ownership-journal-bound"));
    }
    mg::fs::write_file(root, &journal_path(confirm)?, &bytes(&journal))?;
    anchor(root, &journal)?;
    #[cfg(test)]
    checkpoint()?;
    pending_marker(root, confirm)?;
    #[cfg(test)]
    checkpoint()?;
    for edit in &saved.edits {
        if edit.path == STATE {
            continue;
        }
        if !matches(root, edit, true)? {
            return Err(fail("ownership-publish-conflict"));
        }
        if let Some(content) = &edit.content {
            mg::fs::write_file(root, &edit.path, content.as_bytes())?;
        } else {
            mg::fs::remove_file(root, &edit.path)?;
        }
        #[cfg(test)]
        checkpoint()?;
    }
    for edit in &saved.edits {
        if edit.path == STATE {
            continue;
        }
        if !matches(root, edit, false)? {
            return Err(fail("ownership-publish-postcondition"));
        }
    }
    check_unchanged_inputs(root, &journal)?;
    authorized(root, &journal)?;
    let mut receipt = Receipt {
        schema_version: format!("lekalo/ownership-transition-receipt/v{VERSION}"),
        plan_digest: confirm.to_owned(),
        project: saved.project.clone(),
        state: "committed".to_owned(),
        module: saved.request.module.clone(),
        symbol: saved.request.symbol.clone(),
        from: saved.from,
        to: saved.request.to,
        verification: verified,
        files: saved
            .edits
            .iter()
            .map(|edit| Pin {
                path: edit.path.clone(),
                digest: edit.after.clone(),
            })
            .collect(),
        rollback: confirm.to_owned(),
        previous_receipt: None,
    };
    publish_receipt(root, &mut receipt)?;
    #[cfg(test)]
    checkpoint()?;
    authorized(root, &journal)?;
    let content = state_edit(&journal)?
        .content
        .as_ref()
        .ok_or_else(|| fail("ownership-journal-state"))?;
    mg::fs::write_file(root, STATE, content.as_bytes())?;
    #[cfg(test)]
    checkpoint()?;
    mg::fs::remove_file(root, PENDING)?;
    Ok(receipt)
}

/// Rollback identifies a locally retained receipt, never an external document.
pub fn rollback(root: &Path, plan_digest: &str, confirm: Option<&str>) -> Outcome<RecoveryView> {
    let _guard = confirm.map(|_| lock(root)).transpose()?;
    if barrier(root) {
        return Err(fail("ownership-recovery-required"));
    }
    let receipt: Receipt = decode(&required(root, &receipt_path(plan_digest)?)?)?;
    let mut journal = load_journal(root, plan_digest)?;
    if !receipt_matches(&receipt, &journal) {
        return Err(fail("ownership-receipt-conflict"));
    }
    let view = RecoveryView {
        schema_version: format!("lekalo/ownership-rollback-plan/v{VERSION}"),
        plan_digest: plan_digest.to_owned(),
        phase: "rollback-preview".to_owned(),
        files: journal
            .plan
            .edits
            .iter()
            .map(|edit| edit.path.clone())
            .collect(),
    };
    if receipt.state == "rolled-back" {
        return Ok(RecoveryView {
            phase: "rolled-back".to_owned(),
            ..view
        });
    }
    if receipt.state != "committed"
        || journal
            .plan
            .edits
            .iter()
            .any(|edit| !matches(root, edit, false).unwrap_or(false))
    {
        return Err(fail("ownership-rollback-conflict"));
    }
    check_unchanged_inputs(root, &journal)?;
    let Some(confirm) = confirm else {
        return Ok(view);
    };
    if confirm != plan_digest {
        return Err(fail("ownership-confirmation"));
    }
    journal.phase = "restoring".to_owned();
    mg::fs::write_file(root, &journal_path(plan_digest)?, &bytes(&journal))?;
    anchor(root, &journal)?;
    #[cfg(test)]
    checkpoint()?;
    pending_marker(root, plan_digest)?;
    #[cfg(test)]
    checkpoint()?;
    restore(root, &journal)?;
    let mut receipt = receipt;
    receipt.state = "rolled-back".to_owned();
    publish_receipt(root, &mut receipt)?;
    mg::fs::remove_file(root, PENDING)?;
    Ok(RecoveryView {
        phase: "rolled-back".to_owned(),
        ..view
    })
}

#[cfg(test)]
#[path = "transaction_tests.rs"]
mod tests;

pub fn recovery(root: &Path, confirm: Option<&str>) -> Outcome<RecoveryView> {
    let _guard = confirm.map(|_| lock(root)).transpose()?;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Pending {
        plan_digest: String,
    }
    let grant = state(root)?.pending;
    let pending = if let Some(grant) = &grant {
        if mg::digest(&required(root, &journal_path(&grant.plan_digest)?)?) != grant.journal_digest
        {
            return Err(fail("ownership-journal-authority"));
        }
        if let Some(value) = read(root, PENDING)? {
            let marker: Pending = decode(&value)?;
            if marker.plan_digest != grant.plan_digest {
                return Err(fail("ownership-journal-authority"));
            }
        }
        Pending {
            plan_digest: grant.plan_digest.clone(),
        }
    } else {
        decode(&required(root, PENDING)?)?
    };
    let journal = load_journal(root, &pending.plan_digest)?;
    if grant.is_some() {
        authorized(root, &journal)?;
    }
    let mut view = RecoveryView {
        schema_version: format!("lekalo/ownership-rollback-plan/v{VERSION}"),
        plan_digest: pending.plan_digest.clone(),
        phase: journal.phase.clone(),
        files: journal
            .plan
            .edits
            .iter()
            .map(|edit| edit.path.clone())
            .collect(),
    };
    let Some(confirm) = confirm else {
        return Ok(view);
    };
    if confirm != pending.plan_digest {
        return Err(fail("ownership-confirmation"));
    }
    // Recovery may start in the window after the canonical grant was installed
    // but before the private marker existed. Keep that marker through its own
    // final source restoration so a second interruption remains discoverable.
    if grant.is_some() {
        pending_marker(root, confirm)?;
    }
    if journal.phase == "publishing" {
        if let Some(value) = read(root, &receipt_path(confirm)?)? {
            let receipt: Receipt = decode(&value)?;
            if receipt.state == "committed" {
                if !receipt_matches(&receipt, &journal) {
                    return Err(fail("ownership-receipt-conflict"));
                }
                if journal
                    .plan
                    .edits
                    .iter()
                    .filter(|edit| grant.is_none() || edit.path != STATE)
                    .any(|edit| !matches(root, edit, false).unwrap_or(false))
                {
                    return Err(fail("ownership-recovery-conflict"));
                }
                check_unchanged_inputs(root, &journal)?;
                if grant.is_some() {
                    authorized(root, &journal)?;
                    let content = state_edit(&journal)?
                        .content
                        .as_ref()
                        .ok_or_else(|| fail("ownership-journal-state"))?;
                    mg::fs::write_file(root, STATE, content.as_bytes())?;
                }
                mg::fs::remove_file(root, PENDING)?;
                view.phase = "committed".to_owned();
                return Ok(view);
            }
        }
    }
    if grant.is_some() {
        restore(root, &journal)?;
    } else if journal
        .plan
        .edits
        .iter()
        .any(|edit| !matches(root, edit, true).unwrap_or(false))
    {
        // An editable cache cannot authorize a source write. Without the source
        // grant only cache cleanup for an already restored cohort is permitted.
        return Err(fail("ownership-journal-authority"));
    }
    let mut receipt = Receipt {
        schema_version: format!("lekalo/ownership-transition-receipt/v{VERSION}"),
        plan_digest: confirm.to_owned(),
        project: journal.plan.project.clone(),
        state: "rolled-back".to_owned(),
        module: journal.plan.request.module.clone(),
        symbol: journal.plan.request.symbol.clone(),
        from: journal.plan.from,
        to: journal.plan.request.to,
        verification: journal.verification.clone(),
        files: journal
            .plan
            .edits
            .iter()
            .map(|edit| Pin {
                path: edit.path.clone(),
                digest: edit.after.clone(),
            })
            .collect(),
        rollback: confirm.to_owned(),
        previous_receipt: None,
    };
    publish_receipt(root, &mut receipt)?;
    mg::fs::remove_file(root, PENDING)?;
    view.phase = "restored".to_owned();
    Ok(view)
}

/// Neutral derived evidence for HLV/AIFHub, containing pins and measured
/// observation hashes, never source snippets or application result payloads.
pub fn trace(root: &Path, plan_digest: &str) -> Outcome<OwnershipTrace> {
    if barrier(root) {
        return Err(fail("ownership-recovery-required"));
    }
    let receipt_bytes = required(root, &receipt_path(plan_digest)?)?;
    let receipt: Receipt = decode(&receipt_bytes)?;
    let journal = load_journal(root, plan_digest)?;
    if !receipt_matches(&receipt, &journal) {
        return Err(fail("ownership-receipt-conflict"));
    }
    let current = receipt.state == "committed"
        && check_unchanged_inputs(root, &journal).is_ok()
        && journal
            .plan
            .edits
            .iter()
            .all(|edit| matches(root, edit, false).unwrap_or(false));
    Ok(OwnershipTrace {
        schema_version: format!("lekalo/ownership-trace/v{VERSION}"),
        authority: "derived".to_owned(),
        project: receipt.project,
        plan_digest: plan_digest.to_owned(),
        request_digest: mg::hash(&journal.plan.request),
        subject: receipt.symbol.clone().unwrap_or(receipt.module),
        from: receipt.from,
        to: receipt.to,
        state: receipt.state,
        current,
        semantic_diff: journal.plan.semantic_diff,
        inputs: journal.plan.inputs,
        verification: receipt.verification,
        files: receipt.files,
        rollback: receipt.rollback,
        receipt_digest: mg::digest(&receipt_bytes),
        previous_receipt: receipt.previous_receipt,
    })
}
