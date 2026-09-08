//! Declared output plans and their verification against the project
//! (issue #27).
//!
//! A planning exchange (`generate` with `dry_run: true`, or `plan-clean`)
//! produces the declared output plan: the exact logical paths, the action,
//! and for `create`/`replace` the SHA-256 of the exact resulting bytes. An
//! apply exchange must reproduce the identical plan (deterministic
//! adapters make plans reproducible; nothing persists between the two
//! child processes), and after the child exits core verifies the observed
//! project state against the plan: every declared write matches the
//! declared digest, every declared deletion is gone, and nothing else
//! inside the declared write scopes changed. Dry-run exchanges must change
//! nothing at all.

use std::collections::BTreeMap;

use super::scopes;
use super::wire::{plan_id, WriteAction, WriteEntry};

/// SHA-256 of arbitrary bytes as lowercase hex (no prefix).
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A snapshot of one write scope: logical path to observed content digest.
/// Files that are unreadable or oversized record a stable marker so a
/// change is still detected, never silently ignored.
pub type Snapshot = BTreeMap<String, String>;

/// The unreadable-content marker recorded in snapshots.
const UNREADABLE: &str = "unreadable";

/// Observe one declared scope: recursive scopes walk their root, exact
/// scopes record the single named file. A scope over a not-yet-existing
/// root observes nothing.
///
/// Refuses when the scope holds more entries than the verification bound:
/// an unverifiable scope is a policy refusal, never a silent pass.
/// Observe one declared scope and record its files into the snapshot.
pub fn snapshot_scope(
    fs: &crate::project_fs::Fs,
    scope: &str,
    out: &mut Snapshot,
) -> Result<(), SnapshotRejection> {
    let parts: Vec<&str> = scope.split('/').collect();
    let recursive = parts.last() == Some(&"**");
    let (dir, name) = match parts.split_last() {
        Some((last, head)) => (head.join("/"), *last),
        None => return Err(SnapshotRejection::Scope),
    };
    if recursive {
        if name != "**" {
            return Err(SnapshotRejection::Scope);
        }
        // A scope over a not-yet-existing root observes nothing; any other
        // refusal (non-directory root, unreadable entry) is an unverifiable
        // scope, never a silent pass.
        match fs.entries(&dir) {
            Ok(entries) => walk_entries(fs, &dir, &entries, out)?,
            Err(crate::project_fs::FsErrorKind::NotFound) => {}
            Err(_) => return Err(SnapshotRejection::Scope),
        }
    } else {
        let logical = if dir.is_empty() {
            name.to_owned()
        } else {
            format!("{dir}/{name}")
        };
        match fs.entry_type(&dir, name) {
            Ok(crate::project_fs::EntryType::File) => record_file(fs, &logical, out)?,
            Err(crate::project_fs::FsErrorKind::NotFound) => {}
            _ => return Err(SnapshotRejection::Scope),
        }
    }
    if out.len() > super::version::MAX_SCOPED_FILES {
        return Err(SnapshotRejection::Limit);
    }
    Ok(())
}

/// Walk one directory's already-listed entries recursively, recording every
/// regular file.
fn walk_entries(
    fs: &crate::project_fs::Fs,
    dir: &str,
    entries: &[(String, crate::project_fs::EntryType)],
    out: &mut Snapshot,
) -> Result<(), SnapshotRejection> {
    for (child, kind) in entries {
        let logical = if dir.is_empty() {
            child.clone()
        } else {
            format!("{dir}/{child}")
        };
        match kind {
            crate::project_fs::EntryType::File => {
                record_file(fs, &logical, out)?;
            }
            crate::project_fs::EntryType::Directory => {
                let nested = fs.entries(&logical).map_err(|_| SnapshotRejection::Io)?;
                walk_entries(fs, &logical, &nested, out)?;
            }
            // Symlinks and special entries are hostile inside a declared
            // write scope: refuse rather than verify around them.
            _ => return Err(SnapshotRejection::Scope),
        }
        if out.len() > super::version::MAX_SCOPED_FILES {
            return Err(SnapshotRejection::Limit);
        }
    }
    Ok(())
}

/// Split a logical path into `(dir, name)` for the descriptor API; a
/// root-level file splits into the empty directory.
fn split_logical(logical: &str) -> Option<(String, String)> {
    match logical.rsplit_once('/') {
        Some((dir, name)) => Some((dir.to_owned(), name.to_owned())),
        None => Some((String::new(), logical.to_owned())),
    }
}

/// Record one file's digest (or the unreadable marker).
fn record_file(
    fs: &crate::project_fs::Fs,
    logical: &str,
    out: &mut Snapshot,
) -> Result<(), SnapshotRejection> {
    let (dir, name) = split_logical(logical).ok_or(SnapshotRejection::Scope)?;
    match fs.read_file_opt(&dir, &name, super::version::MAX_FILE_BYTES) {
        Ok(Some(bytes)) => {
            out.insert(logical.to_owned(), sha256_hex(&bytes));
        }
        Ok(None) => {
            out.insert(logical.to_owned(), UNREADABLE.to_owned());
        }
        Err(crate::project_fs::FsErrorKind::Limit { .. }) => {
            out.insert(logical.to_owned(), UNREADABLE.to_owned());
        }
        Err(_) => return Err(SnapshotRejection::Io),
    }
    Ok(())
}

/// Why a scope snapshot could not be taken.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotRejection {
    /// The scope names a non-directory/non-file entry or is not a verifiable
    /// recursive form.
    Scope,
    /// The scope holds more files than the verification bound.
    Limit,
    /// The filesystem refused the walk.
    Io,
}

impl SnapshotRejection {
    /// The bounded wire token carried in diagnostic data.
    pub fn detail(self) -> &'static str {
        match self {
            Self::Scope => "unverifiable-scope",
            Self::Limit => "scope-limit",
            Self::Io => "verification-io",
        }
    }
}

/// Snapshot every declared write scope in canonical order.
pub fn snapshot_scopes(
    fs: &crate::project_fs::Fs,
    write_scopes: &[String],
) -> Result<Snapshot, SnapshotRejection> {
    let mut out = Snapshot::new();
    for scope in write_scopes {
        snapshot_scope(fs, scope, &mut out)?;
    }
    Ok(out)
}

/// Compare two snapshots: the sorted list of paths whose digest changed,
/// appeared, or disappeared.
pub fn changed_paths(before: &Snapshot, after: &Snapshot) -> Vec<String> {
    let mut changed = Vec::new();
    for (path, digest) in before {
        if after.get(path) != Some(digest) {
            changed.push(path.clone());
        }
    }
    for path in after.keys() {
        if !before.contains_key(path) {
            changed.push(path.clone());
        }
    }
    changed.sort();
    changed.dedup();
    changed
}

/// Verify the observed project state against the declared plan after an
/// apply exchange: `create`/`replace` entries exist with the declared
/// digest, `delete` entries are gone, and the only snapshot changes are the
/// declared paths.
pub fn verify_applied(
    fs: &crate::project_fs::Fs,
    writes: &[WriteEntry],
    before: &Snapshot,
    after: &Snapshot,
) -> Result<(), super::TargetFailure> {
    for entry in writes {
        let (dir, name) = split_logical(&entry.path)
            .ok_or_else(|| super::TargetFailure::plan_mismatch(Some(entry.path.clone()), "path"))?;
        let observed = match fs.read_file_opt(&dir, &name, super::version::MAX_FILE_BYTES) {
            Ok(opt) => Ok(opt),
            // A missing file is the absent signal, not a refusal.
            Err(crate::project_fs::FsErrorKind::NotFound) => Ok(None),
            Err(other) => Err(other),
        };
        match (&entry.action, observed) {
            (WriteAction::Delete, Ok(None)) => {}
            (WriteAction::Delete, _) => {
                return Err(super::TargetFailure::plan_mismatch(
                    Some(entry.path.clone()),
                    "not-deleted",
                ));
            }
            (_, Ok(Some(bytes))) => {
                let Some(expected) = &entry.sha256 else {
                    return Err(super::TargetFailure::plan_mismatch(
                        Some(entry.path.clone()),
                        "digest",
                    ));
                };
                let observed_digest = format!("sha256:{}", sha256_hex(&bytes));
                if &observed_digest != expected {
                    return Err(super::TargetFailure::plan_mismatch(
                        Some(entry.path.clone()),
                        "digest",
                    ));
                }
            }
            (_, Ok(None)) => {
                return Err(super::TargetFailure::plan_mismatch(
                    Some(entry.path.clone()),
                    "missing",
                ));
            }
            (_, Err(_)) => {
                return Err(super::TargetFailure::plan_mismatch(
                    Some(entry.path.clone()),
                    "verification-io",
                ));
            }
        }
    }
    let declared: std::collections::BTreeSet<&str> =
        writes.iter().map(|e| e.path.as_str()).collect();
    for path in changed_paths(before, after) {
        if !declared.contains(path.as_str()) {
            return Err(super::TargetFailure::plan_mismatch(
                Some(path),
                "undeclared",
            ));
        }
    }
    Ok(())
}

/// Whether two plan renderings are the identical plan: same length, same
/// order, same paths, actions, and digests.
pub fn plans_equal(a: &[WriteEntry], b: &[WriteEntry]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.path == y.path && x.action == y.action && x.sha256 == y.sha256)
}

/// The deterministic identifier of one plan.
pub fn plan_identity(entries: &[WriteEntry]) -> String {
    plan_id(entries)
}

/// Whether the logical path sits inside at least one declared scope.
pub fn covered_by(path: &str, scopes_list: &[String]) -> bool {
    scopes_list
        .iter()
        .any(|scope| scopes::scope_covers(scope, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_known_vectors() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn plan_equality_is_exact_and_order_sensitive() {
        let entry = |path: &str, digest: Option<&str>| WriteEntry {
            path: path.to_owned(),
            action: WriteAction::Create,
            sha256: digest.map(str::to_owned),
        };
        let a = vec![
            entry("a.ts", Some("sha256:00")),
            entry("b.ts", Some("sha256:01")),
        ];
        let same = vec![
            entry("a.ts", Some("sha256:00")),
            entry("b.ts", Some("sha256:01")),
        ];
        let reordered = vec![
            entry("b.ts", Some("sha256:01")),
            entry("a.ts", Some("sha256:00")),
        ];
        assert!(plans_equal(&a, &same));
        assert!(!plans_equal(&a, &reordered));
    }

    #[test]
    fn changed_paths_report_both_sides() {
        let mut before = Snapshot::new();
        before.insert("a".to_owned(), "1".to_owned());
        before.insert("b".to_owned(), "2".to_owned());
        let mut after = Snapshot::new();
        after.insert("a".to_owned(), "1".to_owned());
        after.insert("b".to_owned(), "x".to_owned());
        after.insert("c".to_owned(), "3".to_owned());
        assert_eq!(changed_paths(&before, &after), ["b", "c"]);
    }
}
