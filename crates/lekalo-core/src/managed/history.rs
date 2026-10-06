//! Managed runtime custody under the admitted generated-intermediate home
//! (issue #41).
//!
//! Layout inside `<project>/.lekalo/generated/manifests/managed/`:
//! - `current`: the committed generation digest (plain pin, private state),
//! - `generations/<digest>.json`: one closed generation record per commit,
//! - `objects/<hex>`: content-addressed exact byte snapshots,
//! - `transactions/<id>.json`: bounded recovery journals.
//!
//! Adapters are denied this whole subtree; the artifact cleaner must treat
//! it as owner bookkeeping, never as application orphans.

use super::diagnostic;
use super::wire::Transaction;
use super::{detail, DomainResult, RECOVERY_REQUIRED, ROLLBACK_REFUSED};

/// The custody subtree inside the accepted generated-intermediate home.
pub const CUSTODY: &str = ".lekalo/generated/manifests/managed";
const CURRENT: &str = "current";
const GENERATIONS: &str = "generations";
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

fn custody_path(root: &std::path::Path, relative: &str) -> std::path::PathBuf {
    root.join(CUSTODY).join(relative)
}

/// Whether a project has any managed custody at all.
pub fn exists(root: &std::path::Path) -> bool {
    custody_path(root, CURRENT).is_file()
}

/// Read the committed generation digest, if any. Unreadable custody fails
/// closed instead of appearing absent.
pub fn current(root: &std::path::Path) -> Result<Option<String>, DomainResult> {
    let path = custody_path(root, CURRENT);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let text = String::from_utf8(bytes).map_err(|_| recovery("current-encoding"))?;
            let trimmed = text.trim();
            if super::pin(trimmed) {
                Ok(Some(trimmed.to_owned()))
            } else {
                Err(recovery("current-digest"))
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(recovery("current-read")),
    }
}

/// Read one generation record by digest.
pub fn record(
    root: &std::path::Path,
    digest: &str,
) -> Result<super::wire::GenerationRecord, DomainResult> {
    record_with_custody(root, digest).map(|(record, _)| record)
}

/// Read one generation record plus the custody digest of its exact stored
/// bytes (the manifest identity a rollback restores).
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
    let path = custody_path(root, &format!("{GENERATIONS}/{hex}.json"));
    let bytes = std::fs::read(&path).map_err(|_| refused("generation-record"))?;
    let record = super::parse_record(&bytes).map_err(|_| refused("generation-record"))?;
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
    let bytes = std::fs::read(custody_path(root, &format!("{OBJECTS}/{hex}")))
        .map_err(|_| refused("object"))?;
    if super::digest(&bytes) != format!("sha256:{hex}") {
        return Err(refused("object-digest"));
    }
    Ok(bytes)
}

/// Enumerate unresolved journals. Any `prepared`, `publishing`, or
/// `restoring` transaction blocks further mutation until reconciliation.
pub fn unresolved(root: &std::path::Path) -> Result<Vec<Transaction>, DomainResult> {
    let directory = custody_path(root, TRANSACTIONS);
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(recovery("transaction-scan")),
    };
    let mut unresolved = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| recovery("transaction-scan"))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|_| recovery("transaction-read"))?;
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
    let directory = custody_path(root, TRANSACTIONS);
    let attempts = match std::fs::read_dir(&directory) {
        Ok(entries) => entries.count(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
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
    let directory = custody_path(root, TRANSACTIONS);
    std::fs::create_dir_all(&directory).map_err(|_| recovery("transaction-write"))?;
    let body = format!(
        "{}\n",
        serde_json::to_string_pretty(transaction).expect("typed transaction")
    );
    std::fs::write(
        custody_path(
            root,
            &format!("{TRANSACTIONS}/{}.json", transaction.transaction_id),
        ),
        body,
    )
    .map_err(|_| recovery("transaction-write"))
}

/// Store one content-addressed object; storing is idempotent.
pub fn write_object(root: &std::path::Path, bytes: &[u8]) -> Result<String, DomainResult> {
    let digest = super::digest(bytes);
    let hex = digest.strip_prefix("sha256:").expect("digest");
    let directory = custody_path(root, OBJECTS);
    std::fs::create_dir_all(&directory).map_err(|_| recovery("object-write"))?;
    let path = custody_path(root, &format!("{OBJECTS}/{hex}"));
    if !path.is_file() {
        std::fs::write(&path, bytes).map_err(|_| recovery("object-write"))?;
    }
    Ok(digest)
}

/// Store one generation record; storing is idempotent.
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
    let directory = custody_path(root, GENERATIONS);
    std::fs::create_dir_all(&directory).map_err(|_| recovery("record-write"))?;
    let path = custody_path(root, &format!("{GENERATIONS}/{hex}.json"));
    if !path.is_file() {
        std::fs::write(&path, custody.as_bytes()).map_err(|_| recovery("record-write"))?;
    }
    Ok(custody_digest)
}

/// Commit the generation pointer.
pub fn write_current(root: &std::path::Path, generation: &str) -> Result<(), DomainResult> {
    let directory = custody_path(root, "");
    std::fs::create_dir_all(&directory).map_err(|_| recovery("current-write"))?;
    std::fs::write(custody_path(root, CURRENT), format!("{generation}\n"))
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

/// Take the exclusive mutation lock; an existing lock is an unresolved
/// mutation and blocks until reconciled.
pub fn lock(root: &std::path::Path) -> Result<Guard, DomainResult> {
    let directory = custody_path(root, "");
    std::fs::create_dir_all(&directory).map_err(|_| recovery("lock-write"))?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(custody_path(root, MUTATION_LOCK))
    {
        Ok(_) => Ok(Guard {
            path: custody_path(root, MUTATION_LOCK),
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
