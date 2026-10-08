//! Managed runtime custody under the admitted generated-intermediate home
//! (issue #41).
//!
//! Layout inside `<project>/.lekalo/generated/manifests/managed/`:
//! - `current`: the committed generation digest (plain pin, private state),
//! - `objects/<hex>`: content-addressed exact byte snapshots AND the
//!   generation records themselves — a record is stored under its own
//!   generation digest, so the storage name is the content identity (the
//!   project path policy bounds every name at 64 characters, which rules
//!   out a `generations/<64hex>.json` spelling),
//! - `transactions/<id>.json`: bounded recovery journals.
//!
//! Adapters are denied this whole subtree; the artifact cleaner must treat
//! it as owner bookkeeping, never as application orphans. Every custody
//! read and write goes through the confined filesystem capability: ancestor
//! links and reparse aliases fail closed instead of escaping the project.

use super::diagnostic;
use super::fs;
use super::wire::Transaction;
use super::{detail, DomainResult, RECOVERY_REQUIRED, ROLLBACK_REFUSED};

/// The custody subtree inside the accepted generated-intermediate home.
pub const CUSTODY: &str = ".lekalo/generated/manifests/managed";
const CURRENT: &str = "current";
const OBJECTS: &str = "objects";
const TRANSACTIONS: &str = "transactions";
/// The exclusive project mutation lock (never held across processes).
pub const MUTATION_LOCK: &str = "mutation.lock";

fn recovery(reason: &str) -> DomainResult {
    diagnostic::failure(RECOVERY_REQUIRED, &detail(reason))
}

fn refused(reason: &str) -> DomainResult {
    diagnostic::failure(ROLLBACK_REFUSED, &detail(reason))
}

/// The project-relative custody coordinate of one custody-relative path.
pub fn custody_logical(relative: &str) -> String {
    format!("{CUSTODY}/{relative}")
}

/// Whether a project has any managed custody at all.
pub fn exists(root: &std::path::Path) -> bool {
    matches!(fs::read_file(root, &custody_logical(CURRENT)), Ok(Some(_)))
}

/// Read the committed generation digest, if any. Unreadable custody fails
/// closed instead of appearing absent.
pub fn current(root: &std::path::Path) -> Result<Option<String>, DomainResult> {
    match fs::read_file(root, &custody_logical(CURRENT)) {
        Ok(Some(bytes)) => {
            let text = String::from_utf8(bytes).map_err(|_| recovery("current-encoding"))?;
            let trimmed = text.trim();
            if super::pin(trimmed) {
                Ok(Some(trimmed.to_owned()))
            } else {
                Err(recovery("current-digest"))
            }
        }
        Ok(None) => Ok(None),
        Err(_) => Err(recovery("current-read")),
    }
}

/// Read one generation record by digest, fully verified.
pub fn record(
    root: &std::path::Path,
    digest: &str,
) -> Result<super::wire::GenerationRecord, DomainResult> {
    record_with_custody(root, digest).map(|(record, _)| record)
}

/// Read one generation record plus the custody digest of its exact stored
/// bytes (the manifest identity a rollback restores).
///
/// The record is trusted only after its content-bound identity checks: the
/// storage name must be the record's own stable generation digest, the
/// authenticated custody identity must cover the exact document
/// (`recordDigest`, finding 3 — binding the plan, ownership, and lock pins
/// a rehashed self-consistent inventory never authenticates), and every
/// retained object reference must resolve to bytes matching the recorded
/// output digest. A tampered record — any edited path, digest, role,
/// owner, plan pin, ownership pin, or lock pin — is refused without
/// touching a single project byte.
pub fn record_with_custody(
    root: &std::path::Path,
    digest: &str,
) -> Result<(super::wire::GenerationRecord, String), DomainResult> {
    let hex = digest.strip_prefix("sha256:").unwrap_or(digest);
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(refused("generation-digest"));
    }
    let bytes = match fs::read_file(root, &custody_logical(&format!("{OBJECTS}/{hex}"))) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Err(refused("generation-record")),
        Err(_) => return Err(refused("generation-record")),
    };
    // parse_record already verifies the authenticated custody identity
    // (recordDigest over the exact document) and every pin shape.
    let record = super::parse_record(&bytes).map_err(|_| refused("generation-record"))?;
    // Lookup identity: the storage name is the stable generation digest.
    if record.generation_digest != format!("sha256:{hex}") {
        return Err(refused("record-identity"));
    }
    // Content-bound identity: the recorded inventory re-derives the stable
    // generation digest and every retained object must exist; the after
    // bytes must match the recorded output digest exactly.
    if super::generation_digest(&record) != record.generation_digest {
        return Err(refused("record-identity"));
    }
    for output in &record.outputs {
        if let Some(reference) = output.before_ref.value() {
            object(root, reference)?;
        }
        if let Some(reference) = output.after_ref.value() {
            if super::digest(&object(root, reference)?) != output.digest {
                return Err(refused("record-object"));
            }
        }
    }
    Ok((record, super::digest(&bytes)))
}

/// Read one content-addressed object and verify its exact bytes.
pub fn object(root: &std::path::Path, reference: &str) -> Result<Vec<u8>, DomainResult> {
    let hex = reference.strip_prefix("sha256:").unwrap_or(reference);
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(refused("object-ref"));
    }
    let bytes = match fs::read_file(root, &custody_logical(&format!("{OBJECTS}/{hex}"))) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Err(refused("object")),
        Err(_) => return Err(refused("object")),
    };
    if super::digest(&bytes) != format!("sha256:{hex}") {
        return Err(refused("object-digest"));
    }
    Ok(bytes)
}

/// Enumerate unresolved journals. Any `prepared`, `publishing`, or
/// `restoring` transaction blocks further mutation until reconciliation.
///
/// Read-only scan (finding 12): a missing journal directory means *no
/// journals*, so previews and dry runs never create the custody tree.
pub fn unresolved(root: &std::path::Path) -> Result<Vec<Transaction>, DomainResult> {
    let directory = custody_logical(TRANSACTIONS);
    // The logical coordinate must not be a file; absence of the directory
    // is the legal empty state. Hostile leaves fail closed.
    match std::fs::symlink_metadata(root.join(&directory)) {
        Ok(metadata) => {
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err(recovery("transaction-scan"));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(recovery("transaction-scan")),
    }
    let entries = match std::fs::read_dir(root.join(&directory)) {
        Ok(entries) => entries,
        Err(_) => return Err(recovery("transaction-scan")),
    };
    let mut unresolved = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| recovery("transaction-scan"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(recovery("transaction-scan"));
        };
        if !name.ends_with(".json") {
            continue;
        }
        let bytes = match fs::read_file(root, &custody_logical(&format!("{TRANSACTIONS}/{name}"))) {
            Ok(Some(bytes)) => bytes,
            _ => return Err(recovery("transaction-read")),
        };
        let transaction =
            super::parse_transaction(&bytes).map_err(|_| recovery("transaction-shape"))?;
        if transaction.state.unresolved() {
            unresolved.push(transaction);
        }
    }
    unresolved.sort_by(|left, right| left.transaction_id.cmp(&right.transaction_id));
    Ok(unresolved)
}

/// The deterministic next journal identity for a plan: no clock, no
/// randomness, only the plan digest and the existing attempt count.
pub fn transaction_id(root: &std::path::Path, plan_digest: &str) -> Result<String, DomainResult> {
    let directory = custody_logical(TRANSACTIONS);
    // Creating the journal directory is fine here: the caller is inside a
    // confirmed mutation (the plan is about to be journaled).
    if fs::ensure_dir(root, &directory).is_err() {
        return Err(recovery("transaction-scan"));
    }
    let attempts = match std::fs::read_dir(root.join(&directory)) {
        Ok(entries) => entries.count(),
        Err(_) => return Err(recovery("transaction-scan")),
    };
    let stem = plan_digest
        .strip_prefix("sha256:")
        .unwrap_or(plan_digest)
        .chars()
        .take(16)
        .collect::<String>();
    Ok(format!("{stem}-{attempts:04}"))
}

/// Write one journal entry atomically enough for recovery: the bounded
/// transaction document under its own name.
pub fn write_transaction(
    root: &std::path::Path,
    transaction: &Transaction,
) -> Result<(), DomainResult> {
    let body = format!(
        "{}\n",
        serde_json::to_string_pretty(transaction).expect("typed transaction")
    );
    fs::write_file(
        root,
        &custody_logical(&format!(
            "{TRANSACTIONS}/{}.json",
            transaction.transaction_id
        )),
        body.as_bytes(),
    )
    .map_err(|_| recovery("transaction-write"))
}

/// Store one content-addressed object; storing is idempotent.
pub fn write_object(root: &std::path::Path, bytes: &[u8]) -> Result<String, DomainResult> {
    let digest = super::digest(bytes);
    let hex = digest.strip_prefix("sha256:").expect("digest");
    let logical = custody_logical(&format!("{OBJECTS}/{hex}"));
    if matches!(fs::read_file(root, &logical), Ok(Some(_))) {
        return Ok(digest);
    }
    fs::write_file(root, &logical, bytes).map_err(|_| recovery("object-write"))?;
    Ok(digest)
}

/// Store one generation record; storing is idempotent.
///
/// The storage name stays the record's stable generation identity (what
/// `current`, receipts, and `rollback --to` name); the authenticated
/// custody identity (`recordDigest`, finding 3) rides inside the document
/// and is verified on every read, binding the plan, ownership, and lock
/// pins the stable identity deliberately ignores.
pub fn write_record(
    root: &std::path::Path,
    record: &super::wire::GenerationRecord,
) -> Result<String, DomainResult> {
    let custody = format!(
        "{}\n",
        serde_json::to_string_pretty(record).expect("typed record")
    );
    let custody_digest = super::digest(custody.as_bytes());
    let hex = record
        .generation_digest
        .strip_prefix("sha256:")
        .expect("digest");
    let logical = custody_logical(&format!("{OBJECTS}/{hex}"));
    if matches!(fs::read_file(root, &logical), Ok(Some(_))) {
        return Ok(custody_digest);
    }
    fs::write_file(root, &logical, custody.as_bytes()).map_err(|_| recovery("record-write"))?;
    Ok(custody_digest)
}

/// Commit the generation pointer.
pub fn write_current(root: &std::path::Path, generation: &str) -> Result<(), DomainResult> {
    fs::write_file(
        root,
        &custody_logical(CURRENT),
        format!("{generation}\n").as_bytes(),
    )
    .map_err(|_| recovery("current-write"))
}

/// Remove the journal of a resolved transaction (the closed document with
/// its final state stays for audit until the reviewed retention decision).
pub fn close_transaction(
    root: &std::path::Path,
    transaction: &Transaction,
) -> Result<(), DomainResult> {
    write_transaction(root, transaction)
}

/// The centralized unresolved-journal barrier: every mutator calls this
/// before taking the mutation lock and again after acquiring it. Any
/// `prepared`, `publishing`, or `restoring` journal — interrupted
/// publication — blocks every further mutation until reconciliation.
pub fn assert_reconciled(root: &std::path::Path) -> Result<(), DomainResult> {
    let unresolved = unresolved(root)?;
    if let Some(transaction) = unresolved.first() {
        return Err(diagnostic::failure(
            RECOVERY_REQUIRED,
            &[
                ("pin", transaction.transaction_id.as_str()),
                ("reason", "unresolved-transaction"),
            ],
        ));
    }
    Ok(())
}

/// Take the exclusive mutation lock; an existing lock is an unresolved
/// mutation and blocks until reconciled.
pub fn lock(root: &std::path::Path) -> Result<Guard, DomainResult> {
    fs::ensure_dir(root, CUSTODY).map_err(|_| recovery("lock-write"))?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(custody_logical(MUTATION_LOCK)))
    {
        Ok(_) => Ok(Guard {
            path: root.join(custody_logical(MUTATION_LOCK)),
        }),
        Err(_) => Err(recovery("mutation-lock")),
    }
}

/// The lock guard; dropping releases the path.
pub struct Guard {
    path: std::path::PathBuf,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Delete every byte of managed custody for a project (tests only).
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn remove_all(root: &std::path::Path) -> std::io::Result<()> {
    std::fs::remove_dir_all(root.join(CUSTODY))
}
