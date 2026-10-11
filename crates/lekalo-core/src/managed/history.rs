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
/// The exclusive project mutation lock (held by one live process, proven
/// via an OS-level advisory lock — see [`history::prove_lock_stale`]).
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

/// Read the committed generation record and prove it is the custody
/// statement of the published manifest (finding 4): every recorded output
/// appears exactly once in the public manifest with the same digest, and
/// every manifest entry names a recorded output. A caller-retargeted record
/// (paths rewritten outside the generated roots, both self-hashes
/// recomputed) does not biject with the published custody statement and is
/// refused without touching a project byte. Every consumer of the *current*
/// record goes through this gate.
pub fn current_record(
    root: &std::path::Path,
) -> Result<super::wire::GenerationRecord, DomainResult> {
    let digest = match current(root)? {
        Some(digest) => digest,
        None => return Err(refused("no-generation")),
    };
    let record = record(root, &digest)?;
    let bytes = match fs::read_file(root, super::custody::MANIFEST_LOGICAL) {
        Ok(Some(bytes)) => bytes,
        _ => return Err(refused("manifest-read")),
    };
    let manifest = crate::artifacts::ArtifactManifest::parse_canonical(&bytes)
        .map_err(|_| refused("manifest-read"))?;
    let recorded: std::collections::BTreeMap<&str, &str> = record
        .outputs
        .iter()
        .map(|output| (output.path.as_str(), output.digest.as_str()))
        .collect();
    let published: std::collections::BTreeMap<&str, &str> = manifest
        .artifacts()
        .iter()
        .map(|entry| (entry.key().path().as_str(), entry.content().as_str()))
        .collect();
    if recorded != published {
        return Err(refused("record-manifest"));
    }
    Ok(record)
}

/// Read one retained generation plan by its plan digest (finding 5): the
/// reviewed plan bytes are content-addressed in the object store, so the
/// scan admits only a fully closed plan document whose own digest is
/// exactly the requested pin. Records that merely name a plan digest are
/// never trusted without these retained bytes.
pub fn plan(
    root: &std::path::Path,
    plan_digest: &str,
) -> Result<super::wire::GenerationPlan, DomainResult> {
    for candidate in objects(root)? {
        let bytes = match read_object_bytes(root, &candidate) {
            Ok(Some(bytes)) => bytes,
            _ => continue,
        };
        if let Ok(plan) = super::verify_plan(&bytes) {
            if plan.plan_digest == plan_digest {
                return Ok(plan);
            }
        }
    }
    Err(refused("retained-plan"))
}

/// Read one retained rollback plan by its plan digest (finding 5): the
/// reviewed rollback intent bounds every pending move of a rollback
/// journal the same way a generation plan bounds an apply journal.
pub fn rollback_plan(
    root: &std::path::Path,
    plan_digest: &str,
) -> Result<super::wire::RollbackPlan, DomainResult> {
    for candidate in objects(root)? {
        let bytes = match read_object_bytes(root, &candidate) {
            Ok(Some(bytes)) => bytes,
            _ => continue,
        };
        if let Ok(plan) = super::verify_rollback_plan(&bytes) {
            if plan.plan_digest == plan_digest {
                return Ok(plan);
            }
        }
    }
    Err(refused("retained-plan"))
}

/// Read one retained coverage document by its canonical digest (finding 13):
/// rollback re-qualifies adapter and binding dependencies from the exact
/// retained evidence, never from caller-supplied bytes.
pub fn coverage(
    root: &std::path::Path,
    coverage_digest: &str,
) -> Result<super::wire::Coverage, DomainResult> {
    for candidate in objects(root)? {
        let bytes = match read_object_bytes(root, &candidate) {
            Ok(Some(bytes)) => bytes,
            _ => continue,
        };
        if let Ok(coverage) = super::parse_coverage(&bytes) {
            if super::hash(&coverage) == coverage_digest {
                return Ok(coverage);
            }
        }
    }
    Err(refused("retained-coverage"))
}

/// The object names (64 lowercase hex) currently retained in custody.
fn objects(root: &std::path::Path) -> Result<Vec<String>, DomainResult> {
    let directory = custody_logical(OBJECTS);
    let entries = match std::fs::read_dir(root.join(&directory)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(refused("object-scan")),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| refused("object-scan"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.len() == 64
            && name
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            names.push(name.to_owned());
        }
    }
    Ok(names)
}

/// The exact bytes of one retained object, verified against its name.
fn read_object_bytes(root: &std::path::Path, hex: &str) -> Result<Option<Vec<u8>>, DomainResult> {
    match fs::read_file(root, &custody_logical(&format!("{OBJECTS}/{hex}"))) {
        Ok(Some(bytes)) => {
            if super::digest(&bytes) == format!("sha256:{hex}") {
                Ok(Some(bytes))
            } else {
                Ok(None)
            }
        }
        Ok(None) => Ok(None),
        Err(_) => Err(refused("object")),
    }
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
        // A directory inside the journal home can never be a legal journal
        // (the emitted id grammar is a single path component, r4 finding
        // 11): fail closed instead of leaving smuggled state the scanner
        // cannot see.
        if entry
            .metadata()
            .map_err(|_| recovery("transaction-scan"))?
            .is_dir()
        {
            return Err(recovery("transaction-scan"));
        }
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

/// Write one journal entry atomically (finding 11): the bounded
/// transaction document is staged fresh and renamed over its name, so a
/// crash mid-write can never leave a torn journal, and a staged alias
/// (hardlink at the stage name) refuses instead of being truncated.
pub fn write_transaction(
    root: &std::path::Path,
    transaction: &Transaction,
) -> Result<(), DomainResult> {
    let body = format!(
        "{}\n",
        serde_json::to_string_pretty(transaction).expect("typed transaction")
    );
    let logical = custody_logical(&format!(
        "{TRANSACTIONS}/{}.json",
        transaction.transaction_id
    ));
    let stage = custody_logical(&format!(
        "{TRANSACTIONS}/{}.json.stage",
        transaction.transaction_id
    ));
    fs::write_staged(root, &logical, &stage, body.as_bytes())
        .map_err(|_| recovery("transaction-write"))
}

/// Store one content-addressed object; storing is idempotent. A conflicting
/// object under the same content address is refused: the storage name is
/// the exact bytes digest, so different bytes under one name are custody
/// corruption, never a hit.
pub fn write_object(root: &std::path::Path, bytes: &[u8]) -> Result<String, DomainResult> {
    let digest = super::digest(bytes);
    let hex = digest.strip_prefix("sha256:").expect("digest");
    let logical = custody_logical(&format!("{OBJECTS}/{hex}"));
    // The stage spelling stays inside the 64-character project path policy
    // (a full-hex stage would exceed it and break project scans after a
    // crash); the mutation lock serializes writers, so the short prefix is
    // unambiguous.
    let stage = custody_logical(&format!("{OBJECTS}/{}.stage", &hex[..8]));
    match fs::read_file(root, &logical) {
        Ok(Some(existing)) => {
            if super::digest(&existing) == digest {
                return Ok(digest);
            }
            return Err(recovery("object-conflict"));
        }
        Ok(None) => {}
        Err(_) => return Err(recovery("object-write")),
    }
    fs::write_staged(root, &logical, &stage, bytes).map_err(|_| recovery("object-write"))?;
    Ok(digest)
}

/// Whether `digest` names the currently committed generation or one of
/// its retained ancestors (bounded walk over the `previous_generation`
/// links — the committed lineage).
fn lineage_contains(root: &std::path::Path, digest: &str) -> Result<bool, DomainResult> {
    let mut current = current(root)?;
    let mut depth = 0usize;
    while let Some(generation) = current {
        if depth > 64 {
            return Err(recovery("lineage"));
        }
        if generation == digest {
            return Ok(true);
        }
        let record = record(root, &generation)?;
        current = record
            .previous_generation
            .value()
            .map(|previous| previous.generation_digest.clone());
        depth += 1;
    }
    Ok(false)
}

/// Store one generation record; storing is idempotent.
///
/// The storage name stays the record's stable generation identity (what
/// `current`, receipts, and `rollback --to` name); the authenticated
/// custody identity (`recordDigest`, finding 3) rides inside the document
/// and is verified on every read, binding the plan, ownership, and lock
/// pins the stable identity deliberately ignores.
///
/// Since r6 (devin F4) adoption is first-writer-wins **only for the
/// committed lineage's own record**: a different, self-consistent record
/// already sitting at a generation name is adopted verbatim only when it
/// IS the currently committed record (a re-derivation of the live state);
/// anything else — the pre-planted forged-record shape, whose `recordDigest`
/// is self-computable and whose lineage pins are attacker-chosen — is a
/// `record-conflict`, never silent adoption. A caller-writable object
/// store is integrity against corruption, not against a writer: the
/// transaction's own minted record is the only new custody a mutation may
/// create.
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
    let stage = custody_logical(&format!("{OBJECTS}/{}.stage", &hex[..8]));
    match fs::read_file(root, &logical) {
        Ok(Some(existing)) => {
            if super::digest(&existing) == custody_digest {
                return Ok(custody_digest);
            }
            // Same generation identity, different document: adoptability
            // requires proof the retained bytes already sit on the
            // committed lineage — the live committed record or one of its
            // retained ancestors (the legitimate regenerate → cleanup →
            // restore cycle re-derives an older inventory whose identity
            // recurs). A self-consistent planted record that names no
            // committed generation — the pre-planted forged-record shape,
            // whose `recordDigest` is self-computable and whose lineage
            // pins are attacker-chosen — is custody corruption-by-writer,
            // not a hit (r6, devin F4).
            let retained = super::parse_record(&existing).ok().filter(|retained| {
                retained.generation_digest == record.generation_digest
                    && super::verify_record_identity(retained).is_ok()
            });
            if let Some(retained) = retained {
                if lineage_contains(root, &retained.generation_digest)? {
                    return Ok(super::digest(&existing));
                }
            }
            return Err(recovery("record-conflict"));
        }
        Ok(None) => {}
        Err(_) => return Err(recovery("record-write")),
    }
    fs::write_staged(root, &logical, &stage, custody.as_bytes())
        .map_err(|_| recovery("record-write"))?;
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
/// mutation and blocks until reconciled. The lock leaf is created fresh
/// (`create_new`, never a truncate of an existing file — finding 9) and is
/// held with an OS-level exclusive advisory lock for the guard's lifetime,
/// so a live holder is provable: recovery clears a lock only after
/// acquiring that same advisory lock positively proves no process still
/// holds it (finding 3).
pub fn lock(root: &std::path::Path) -> Result<Guard, DomainResult> {
    fs::ensure_dir(root, CUSTODY).map_err(|_| recovery("lock-write"))?;
    let path = root.join(custody_logical(MUTATION_LOCK));
    #[cfg(unix)]
    {
        use rustix::fs::{flock, FlockOperation};
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| recovery("mutation-lock"))?;
        if flock(&file, FlockOperation::NonBlockingLockExclusive).is_err() {
            drop(file);
            let _ = std::fs::remove_file(&path);
            return Err(recovery("mutation-lock"));
        }
        Ok(Guard {
            path,
            _file: Some(file),
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // The exclusive-share handle IS the lock: a live holder's handle
        // denies every other open of the leaf.
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&path)
            .map_err(|_| recovery("mutation-lock"))?;
        Ok(Guard {
            path,
            _file: Some(file),
        })
    }
}

/// Prove an existing mutation lock is abandoned before recovery may touch
/// the project (finding 3): the lock leaf must be a plain single-link file
/// inside the custody home (a link, reparse alias, or hardlink refuses
/// through the confined capability), and acquiring its OS-level exclusive
/// advisory lock must succeed — a live mutator holds that lock for the
/// whole mutation, so success is positive stale-owner evidence, not a
/// heuristic.
pub fn prove_lock_stale(root: &std::path::Path) -> Result<(), DomainResult> {
    let logical = custody_logical(MUTATION_LOCK);
    // A hostile lock leaf (a link, a directory, a shared inode) is not
    // provably abandoned: refuse instead of clearing it.
    match fs::read_file(root, &logical) {
        Ok(Some(_)) => {}
        Ok(None) => return Ok(()),
        Err(_) => return Err(recovery("recovery-lock")),
    }
    let path = root.join(&logical);
    #[cfg(unix)]
    {
        use rustix::fs::{flock, FlockOperation};
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|_| recovery("recovery-lock"))?;
        if flock(&file, FlockOperation::NonBlockingLockExclusive).is_err() {
            return Err(recovery("recovery-lock-live"));
        }
        let _ = flock(&file, FlockOperation::NonBlockingUnlock);
        Ok(())
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // A live mutator holds the leaf with an exclusive-share handle:
        // acquiring the same share proves abandonment.
        match std::fs::OpenOptions::new()
            .write(true)
            .share_mode(0)
            .open(&path)
        {
            Ok(_) => Ok(()),
            Err(_) => Err(recovery("recovery-lock-live")),
        }
    }
}

/// The lock guard; dropping releases the advisory lock and the path.
pub struct Guard {
    path: std::path::PathBuf,
    /// Held for its `Drop`: releasing the OS-level exclusivity (the
    /// advisory lock on Unix, the exclusive-share handle on Windows).
    _file: Option<std::fs::File>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        // Release the exclusivity BEFORE unlinking: an open exclusive-
        // share handle denies deletion (share_mode(0) shares nothing).
        self._file = None;
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Delete every byte of managed custody for a project (tests only).
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn remove_all(root: &std::path::Path) -> std::io::Result<()> {
    std::fs::remove_dir_all(root.join(CUSTODY))
}
