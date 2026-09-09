//! The adoption skeleton: deterministic content, the planned-write list,
//! and the journaled no-overwrite writer.
//!
//! Every planned file is new by construction: writes go through
//! `create_new`, the operating system's atomic no-clobber, so "no
//! overwrite existing files" holds against races as well as preflight
//! checks. Created paths are journaled in order; a failed write rolls the
//! journal back in reverse; an incomplete rollback is reported, never
//! silently repaired.

use std::io::Write;
use std::path::Path;

/// The canonical path of the project document every adoption writes.
pub(crate) const PROJECT_PATH: &str = "lekalo/project.yaml";

/// One planned file: logical project-relative POSIX path plus exact bytes.
pub(crate) struct PlannedFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// The minimal `lekalo/project.yaml`: the project definition and nothing
/// else. No module, target, or unpublished field is emitted.
pub(crate) fn project_document(project_id: &str) -> PlannedFile {
    debug_assert!(super::valid_project_id(project_id));
    let bytes = format!(
        "{{\"schema_version\":\"1.0.0\",\"definitions\":[{{\"id\":\"{project_id}\",\"kind\":\"project\",\"version\":1,\"description\":\"Adopted existing project.\"}}]}}\n"
    )
    .into_bytes();
    PlannedFile {
        path: PROJECT_PATH.to_owned(),
        bytes,
    }
}

/// The minimal `lekalo/targets/<id>.yaml`, written only for an explicit
/// `--target`. Target documents stay opaque to the loader; an explicit
/// `--profile` selection is recorded verbatim in the same opaque document.
pub(crate) fn target_document(target: &str, profile: Option<&str>) -> PlannedFile {
    debug_assert!(super::detect::valid_target_id(target));
    debug_assert!(profile
        .map(crate::target_protocol::scopes::is_token)
        .unwrap_or(true));
    let bytes = match profile {
        Some(profile) => format!("{{\"target\":\"{target}\",\"profile\":\"{profile}\",\"note\":\"Adopted target selection.\"}}\n"),
        None => format!("{{\"target\":\"{target}\",\"note\":\"Adopted target selection.\"}}\n"),
    }
    .into_bytes();
    PlannedFile {
        path: format!("lekalo/targets/{target}.yaml"),
        bytes,
    }
}

/// The ordered write plan of one adoption.
pub(crate) fn build(
    project_id: &str,
    target: Option<&str>,
    profile: Option<&str>,
) -> Vec<PlannedFile> {
    let mut files = vec![project_document(project_id)];
    if let Some(target) = target {
        files.push(target_document(target, profile));
    }
    files
}

/// One journaled filesystem mutation.
enum JournalEntry {
    File(std::path::PathBuf),
    Dir(std::path::PathBuf),
}

/// The created-path journal of one adoption apply.
pub(crate) struct Journal(Vec<JournalEntry>);

impl Journal {
    fn new() -> Journal {
        Journal(Vec::new())
    }

    /// Rebuild a journal from on-disk created entries: the directory
    /// chain (outermost to innermost) then the file, exactly the order
    /// `apply` journals them in. Recovery tooling and tests use this to
    /// operate on exactly the entries apply would have journaled.
    pub(crate) fn from_created(root: &Path, created: &[String]) -> Journal {
        let mut entries = Vec::new();
        for relative in created {
            let mut current = root.to_path_buf();
            for segment in relative.split('/') {
                current.push(segment);
                if current.is_dir() {
                    entries.push(JournalEntry::Dir(current.clone()));
                }
            }
            let path = root.join(relative);
            if path.is_file() {
                entries.push(JournalEntry::File(path));
            }
        }
        Journal(entries)
    }

    /// Remove every journaled entry in reverse order.
    ///
    /// Returns the logical paths that could not be removed; callers turn a
    /// non-empty remainder into `init.adopt-recovery-required`.
    pub(crate) fn rollback(&self, root: &Path) -> Vec<String> {
        let mut remaining = Vec::new();
        for entry in self.0.iter().rev() {
            let (path, is_file) = match entry {
                JournalEntry::File(path) => (path, true),
                JournalEntry::Dir(path) => (path, false),
            };
            let result = if is_file {
                std::fs::remove_file(path)
            } else {
                std::fs::remove_dir(path)
            };
            if result.is_err() {
                if let Ok(relative) = path.strip_prefix(root) {
                    remaining.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        remaining.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        remaining.dedup();
        remaining
    }
}

/// Create `relative` under `root` as a new file with exact bytes.
///
/// Intermediate directories are created with no-follow checks. The journal
/// records every created entry so a later failure removes exactly what
/// this call added.
fn create_new(
    root: &Path,
    relative: &str,
    bytes: &[u8],
    journal: &mut Journal,
) -> Result<(), std::io::Error> {
    let mut current = root.to_path_buf();
    let segments: Vec<&str> = relative.split('/').collect();
    for segment in &segments[..segments.len() - 1] {
        current.push(segment);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || is_reparse(&metadata) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "project-entry",
                    ));
                }
                if !metadata.is_dir() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "project-entry",
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&current)?;
                journal.0.push(JournalEntry::Dir(current.clone()));
            }
            Err(error) => return Err(error),
        }
    }
    current.push(segments[segments.len() - 1]);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o644);
    }
    let mut file = options.open(&current)?;
    file.write_all(bytes)?;
    journal.0.push(JournalEntry::File(current.clone()));
    Ok(())
}

/// Windows reparse-point check for created-path parents.
fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x0400 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

/// What applying the plan produced.
pub(crate) enum ApplyOutcome {
    /// Every planned write happened.
    Applied,
    /// A planned write failed and the rollback removed every created path.
    WriteFailed { path: String, detail: &'static str },
    /// A rollback could not remove every created file.
    RecoveryRequired { paths: Vec<String> },
}

/// Apply the plan: create every planned file under `root`.
///
/// The caller has already refused conflicts and skipped identical paths,
/// so `create_new` is expected to succeed; any failure rolls the journal
/// back before returning, and an incomplete rollback is reported
/// explicitly.
pub(crate) fn apply(root: &Path, files: &[PlannedFile]) -> ApplyOutcome {
    let mut journal = Journal::new();
    for file in files {
        if let Err(error) = create_new(root, &file.path, &file.bytes, &mut journal) {
            let detail: &'static str = match error.kind() {
                std::io::ErrorKind::PermissionDenied => "create-parent",
                std::io::ErrorKind::AlreadyExists => "already-exists",
                _ => "create-file",
            };
            let remaining = journal.rollback(root);
            if remaining.is_empty() {
                return ApplyOutcome::WriteFailed {
                    path: file.path.clone(),
                    detail,
                };
            }
            return ApplyOutcome::RecoveryRequired { paths: remaining };
        }
    }
    ApplyOutcome::Applied
}

/// Read one planned path's current bytes for the preflight comparison.
pub(crate) fn observe(root: &Path, relative: &str) -> Option<Vec<u8>> {
    let path = root.join(relative);
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    std::fs::read(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rollback that cannot remove a created file is reported with the
    /// logical paths that remain, never silently repaired.
    #[test]
    fn rollback_failure_lists_the_remaining_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir(root.join("lekalo")).expect("create canonical dir");
        std::fs::write(root.join("lekalo").join("project.yaml"), b"{}\n").expect("create file");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root.join("lekalo"), std::fs::Permissions::from_mode(0o555))
                .expect("freeze the canonical dir");
        }
        #[cfg(windows)]
        let _guard = {
            // A share-none handle blocks deletion of the created file.
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(root.join("lekalo").join("project.yaml"))
                .expect("exclusive handle")
        };

        let journal = Journal::from_created(root, &["lekalo/project.yaml".to_owned()]);
        let remaining = journal.rollback(root);

        // Reverse-order removal fails on the frozen directory and on the
        // blocked file; both remain and are reported.
        assert_eq!(
            remaining,
            vec!["lekalo".to_owned(), "lekalo/project.yaml".to_owned()]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root.join("lekalo"), std::fs::Permissions::from_mode(0o755))
                .expect("thaw the canonical dir");
        }
        // The file genuinely survived the blocked rollback.
        assert!(root.join("lekalo").join("project.yaml").is_file());
    }

    /// A clean rollback removes exactly the journaled entries.
    #[test]
    fn rollback_removes_exactly_the_journaled_entries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let files = build("probe", None, None);
        assert!(matches!(apply(root, &files), ApplyOutcome::Applied));
        assert!(root.join("lekalo").join("project.yaml").is_file());
        let journal = Journal::from_created(root, &["lekalo/project.yaml".to_owned()]);
        assert!(journal.rollback(root).is_empty());
        assert!(!root.join("lekalo").exists());
    }
}
