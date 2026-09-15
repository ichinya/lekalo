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
    crate::versioning::plan::sha256_hex(bytes)
}

/// A snapshot of one write scope: logical path to observed content digest.
/// Directories have a distinct marker. Unreadable, oversized and special
/// entries refuse the snapshot; unknown content never proves equality.
pub type Snapshot = BTreeMap<String, String>;

/// Inspect every entry in a private scoped view, including empty directories.
pub(super) fn snapshot_all(fs: &crate::project_fs::Fs) -> Result<Snapshot, SnapshotRejection> {
    let mut out = Snapshot::new();
    let entries = bounded_entries(fs, "")?;
    walk_entries(fs, "", &entries, &mut out, &mut 0)?;
    Ok(out)
}

/// Directory identity cannot be confused with a file digest.
const DIRECTORY: &str = "directory";

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
    snapshot_scope_bounded(fs, scope, out, &mut 0)
}

fn snapshot_scope_bounded(
    fs: &crate::project_fs::Fs,
    scope: &str,
    out: &mut Snapshot,
    bytes: &mut usize,
) -> Result<(), SnapshotRejection> {
    if !scopes::is_scope(scope) {
        return Err(SnapshotRejection::Scope);
    }
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
        match bounded_entries(fs, &dir) {
            Ok(entries) => {
                out.insert(dir.clone(), DIRECTORY.to_owned());
                walk_entries(fs, &dir, &entries, out, bytes)?;
            }
            Err(SnapshotRejection::Missing) => {}
            Err(error) => return Err(error),
        }
    } else {
        let logical = if dir.is_empty() {
            name.to_owned()
        } else {
            format!("{dir}/{name}")
        };
        match fs.entry_type(&dir, name) {
            Ok(crate::project_fs::EntryType::File) => record_file(fs, &logical, out, bytes)?,
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
    bytes: &mut usize,
) -> Result<(), SnapshotRejection> {
    for (child, kind) in entries {
        let logical = if dir.is_empty() {
            child.clone()
        } else {
            format!("{dir}/{child}")
        };
        if !scopes::is_logical_path(&logical) {
            return Err(SnapshotRejection::Scope);
        }
        if out.len() >= super::version::MAX_SCOPED_FILES {
            return Err(SnapshotRejection::Limit);
        }
        match kind {
            crate::project_fs::EntryType::File => {
                record_file(fs, &logical, out, bytes)?;
            }
            crate::project_fs::EntryType::Directory => {
                out.insert(logical.clone(), DIRECTORY.to_owned());
                let nested = bounded_entries(fs, &logical)?;
                walk_entries(fs, &logical, &nested, out, bytes)?;
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

/// Record one verified digest under the aggregate 64 MiB observation cap.
fn record_file(
    fs: &crate::project_fs::Fs,
    logical: &str,
    out: &mut Snapshot,
    observed_bytes: &mut usize,
) -> Result<(), SnapshotRejection> {
    let (dir, name) = split_logical(logical).ok_or(SnapshotRejection::Scope)?;
    match fs.read_file_opt(&dir, &name, super::version::MAX_FILE_BYTES) {
        Ok(Some(bytes)) => {
            *observed_bytes = observed_bytes.saturating_add(bytes.len());
            if *observed_bytes > 64 * 1024 * 1024 {
                return Err(SnapshotRejection::Limit);
            }
            out.insert(logical.to_owned(), sha256_hex(&bytes));
        }
        Ok(None) => return Err(SnapshotRejection::Io),
        Err(crate::project_fs::FsErrorKind::Limit { .. }) => return Err(SnapshotRejection::Limit),
        Err(_) => return Err(SnapshotRejection::Io),
    }
    Ok(())
}

/// Why a scope snapshot could not be taken.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotRejection {
    /// The named root is absent; internal walks treat this as unverified.
    Missing,
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
            Self::Missing => "verification-io",
            Self::Scope => "unverifiable-scope",
            Self::Limit => "scope-limit",
            Self::Io => "verification-io",
        }
    }
}

fn bounded_entries(
    fs: &crate::project_fs::Fs,
    logical: &str,
) -> Result<Vec<(String, crate::project_fs::EntryType)>, SnapshotRejection> {
    if logical.split('/').count() > 64 {
        return Err(SnapshotRejection::Limit);
    }
    fs.entries_bounded(logical, super::version::MAX_SCOPED_FILES)
        .map_err(|error| match error {
            crate::project_fs::FsErrorKind::NotFound => SnapshotRejection::Missing,
            crate::project_fs::FsErrorKind::Limit { .. } => SnapshotRejection::Limit,
            _ => SnapshotRejection::Io,
        })
}

/// Snapshot every declared write scope in canonical order.
pub fn snapshot_scopes(
    fs: &crate::project_fs::Fs,
    write_scopes: &[String],
) -> Result<Snapshot, SnapshotRejection> {
    let mut out = Snapshot::new();
    let mut bytes = 0;
    for scope in write_scopes {
        snapshot_scope_bounded(fs, scope, &mut out, &mut bytes)?;
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
    validate_preconditions(writes, before)?;
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
        // Creating the parent directories needed by a declared new file is
        // the only implicit mutation allowed by a file plan.
        if !before.contains_key(&path)
            && after.get(&path).is_some_and(|kind| kind == DIRECTORY)
            && writes.iter().any(|entry| {
                entry.action != WriteAction::Delete && entry.path.starts_with(&format!("{path}/"))
            })
        {
            continue;
        }
        if !declared.contains(path.as_str()) {
            return Err(super::TargetFailure::plan_mismatch(
                Some(path),
                "undeclared",
            ));
        }
    }
    Ok(())
}

/// Check action meaning before launching the applying child. A create can
/// never overwrite; replace/delete require an observed regular file.
pub fn validate_preconditions(
    writes: &[WriteEntry],
    before: &Snapshot,
) -> Result<(), super::TargetFailure> {
    for entry in writes {
        let previous = before.get(&entry.path);
        let valid = match entry.action {
            WriteAction::Create => previous.is_none(),
            WriteAction::Replace | WriteAction::Delete => {
                previous.is_some_and(|value| value != DIRECTORY)
            }
        };
        if !valid {
            return Err(super::TargetFailure::plan_mismatch(
                Some(entry.path.clone()),
                "precondition",
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
    fn snapshots_bound_directory_depth_and_enumeration_before_recursing() {
        let project = tempfile::tempdir().unwrap();
        let mut directory = project.path().join("out");
        std::fs::create_dir(&directory).unwrap();
        for _ in 0..64 {
            directory = directory.join("x");
            std::fs::create_dir(&directory).unwrap();
        }
        let fs = crate::project_fs::Fs::open(project.path()).unwrap();
        assert_eq!(
            snapshot_scopes(&fs, &["out/**".into()]),
            Err(SnapshotRejection::Limit)
        );
        let wide = tempfile::tempdir().unwrap();
        std::fs::write(wide.path().join("one"), b"1").unwrap();
        std::fs::write(wide.path().join("two"), b"2").unwrap();
        let fs = crate::project_fs::Fs::open(wide.path()).unwrap();
        assert!(matches!(
            fs.entries_bounded("", 1),
            Err(crate::project_fs::FsErrorKind::Limit { max: 1 })
        ));
        assert_eq!(fs.entries_bounded("", 2).unwrap().len(), 2);
    }

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
