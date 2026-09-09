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
#[derive(Debug)]
pub(crate) struct PlannedFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// The minimal `lekalo/project.yaml`: the project definition and nothing
/// else. No module, target, or unpublished field is emitted.
///
/// Fails closed: a project id outside the closed one-segment grammar
/// yields `None` instead of a guessed or interpolated document.
pub(crate) fn project_document(project_id: &str) -> Option<PlannedFile> {
    if !super::valid_project_id(project_id) {
        return None;
    }
    let bytes = format!(
        "{{\"schema_version\":\"1.0.0\",\"definitions\":[{{\"id\":\"{project_id}\",\"kind\":\"project\",\"version\":1,\"description\":\"Adopted existing project.\"}}]}}\n"
    )
    .into_bytes();
    Some(PlannedFile {
        path: PROJECT_PATH.to_owned(),
        bytes,
    })
}

/// The minimal `lekalo/targets/<id>.yaml`, written only for an explicit
/// `--target`. Target documents stay opaque to the loader; an explicit
/// `--profile` selection is recorded verbatim in the same opaque document.
///
/// Fails closed: both persisted values must satisfy their closed
/// grammars, so the path stays inside `lekalo/targets/` and the bytes
/// stay structurally valid JSON in debug and release alike.
pub(crate) fn target_document(target: &str, profile: Option<&str>) -> Option<PlannedFile> {
    if !super::detect::valid_target_id(target) {
        return None;
    }
    if let Some(profile) = profile {
        if !crate::target_protocol::scopes::is_token(profile) {
            return None;
        }
    }
    let bytes = match profile {
        Some(profile) => format!("{{\"target\":\"{target}\",\"profile\":\"{profile}\",\"note\":\"Adopted target selection.\"}}\n"),
        None => format!("{{\"target\":\"{target}\",\"note\":\"Adopted target selection.\"}}\n"),
    }
    .into_bytes();
    Some(PlannedFile {
        path: format!("lekalo/targets/{target}.yaml"),
        bytes,
    })
}

/// The registered `cli.usage` set for a planned value outside its closed
/// grammar: the writer seam refuses the plan instead of writing it.
fn request_refusal() -> crate::diagnostics::DiagnosticSet {
    crate::result::singleton_set(crate::result::CLI_USAGE)
}

/// The ordered write plan of one adoption.
///
/// Fails closed at the writer seam: the project id, the explicit target,
/// and any explicit profile must satisfy their closed grammars, a
/// profile requires an explicit target, and any violation refuses the
/// whole plan with the registered `cli.usage` set before a single path
/// or byte is planned.
pub(crate) fn build(
    project_id: &str,
    target: Option<&str>,
    profile: Option<&str>,
) -> Result<Vec<PlannedFile>, crate::diagnostics::DiagnosticSet> {
    let mut files = Vec::new();
    match project_document(project_id) {
        Some(file) => files.push(file),
        None => return Err(request_refusal()),
    }
    match target {
        Some(target) => match target_document(target, profile) {
            Some(file) => files.push(file),
            None => return Err(request_refusal()),
        },
        None if profile.is_some() => return Err(request_refusal()),
        None => {}
    }
    Ok(files)
}

/// One journaled filesystem mutation.
#[derive(Debug)]
enum JournalEntry {
    File(std::path::PathBuf),
    Dir(std::path::PathBuf),
}

/// The created-path journal of one adoption apply: every entry this
/// apply genuinely created, in creation order. Nothing pre-existing
/// ever enters the journal.
#[derive(Debug)]
pub(crate) struct Journal(Vec<JournalEntry>);

impl Journal {
    fn new() -> Journal {
        Journal(Vec::new())
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
/// this call added. The file is journaled the moment `create_new` owns
/// it — before the first fallible byte write — so a partial write leaves
/// the created path inside the journal, never outside it.
fn create_new(
    root: &Path,
    relative: &str,
    bytes: &[u8],
    write: impl Fn(&mut std::fs::File, &[u8]) -> Result<(), std::io::Error>,
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
    // Journal ownership immediately after the successful create: the
    // entry is now a real mutation even if the byte write below fails,
    // so every later rollback removes exactly it.
    journal.0.push(JournalEntry::File(current.clone()));
    let written = write(&mut file, bytes);
    // Release the OS handle before the error can reach any rollback: on
    // Windows a file with an open handle cannot be removed.
    drop(file);
    written
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
#[derive(Debug)]
pub(crate) enum ApplyOutcome {
    /// Every planned write happened; the journal holds exactly what this
    /// apply created, in creation order — the only authoritative rollback
    /// set for anything that fails afterwards.
    Applied(Journal),
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
/// explicitly. On success the exact mutation journal is returned so a
/// later failure rolls back only what this apply genuinely created, in
/// reverse creation order — never an inferred reconstruction.
pub(crate) fn apply(root: &Path, files: &[PlannedFile]) -> ApplyOutcome {
    apply_with(root, files, |file, bytes| file.write_all(bytes))
}

/// The apply seam with an injected byte writer: production passes
/// `File::write_all`; the fault-path tests pass a deterministic failing
/// writer to prove failure-after-create without touching real storage
/// limits.
fn apply_with(
    root: &Path,
    files: &[PlannedFile],
    write: impl Fn(&mut std::fs::File, &[u8]) -> Result<(), std::io::Error>,
) -> ApplyOutcome {
    let mut journal = Journal::new();
    for file in files {
        if let Err(error) = create_new(root, &file.path, &file.bytes, &write, &mut journal) {
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
    ApplyOutcome::Applied(journal)
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
    /// logical paths that remain, never silently repaired. The journal is
    /// the exact one a real apply returned, and the blocked file genuinely
    /// stays owned by this apply.
    #[test]
    fn rollback_failure_lists_the_remaining_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let files = build("probe", None, None).expect("grammar-clean plan");
        let journal = match apply(root, &files) {
            ApplyOutcome::Applied(journal) => journal,
            outcome => panic!("unexpected apply outcome: {outcome:?}"),
        };

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

        let remaining = journal.rollback(root);

        // Reverse-order removal fails on the blocked file and then on the
        // directory that still holds it; both remain and are reported.
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

    /// A clean rollback removes exactly the journaled entries: the
    /// journal `apply` returns, not an on-disk reconstruction.
    #[test]
    fn rollback_removes_exactly_the_journaled_entries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let files = build("probe", None, None).expect("grammar-clean plan");
        let journal = match apply(root, &files) {
            ApplyOutcome::Applied(journal) => journal,
            outcome => panic!("unexpected apply outcome: {outcome:?}"),
        };
        assert_eq!(journal.0.len(), 2, "the created dir and the created file");
        assert!(root.join("lekalo").join("project.yaml").is_file());
        assert!(journal.rollback(root).is_empty());
        assert!(!root.join("lekalo").exists());
    }

    /// A write that fails after the create leaves the created file inside
    /// the journal: the rollback removes exactly it, and nothing created
    /// survives outside the journal (deterministic injected fault — no
    /// full disk, no global setup).
    #[test]
    fn partial_write_failure_removes_the_created_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let files = build("probe", None, None).expect("grammar-clean plan");
        let outcome = apply_with(root, &files, |file, bytes| {
            // A deterministic partial write: half the bytes land, then the
            // injected fault fires.
            file.write_all(&bytes[..bytes.len() / 2])?;
            Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "injected-fault",
            ))
        });
        match outcome {
            ApplyOutcome::WriteFailed { path, detail } => {
                assert_eq!(path, "lekalo/project.yaml");
                assert_eq!(detail, "create-file");
            }
            outcome => panic!("expected a write failure, got {outcome:?}"),
        }
        // The created file and its created parent directory are gone:
        // ownership was journaled before the fallible byte write.
        assert!(!root.join("lekalo").join("project.yaml").exists());
        assert!(!root.join("lekalo").exists());
        assert_eq!(snapshot_paths(root), Vec::<String>::new());
    }

    /// The returned journal owns exactly what this apply created: a
    /// pre-existing user directory is never journaled, and parents shared
    /// by several planned files are journaled exactly once, in creation
    /// order.
    #[test]
    fn journal_owns_exactly_what_apply_created() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let user_bytes = b"the user owned this directory first\n".to_vec();
        std::fs::create_dir(root.join("lekalo")).expect("pre-existing user dir");
        std::fs::write(root.join("lekalo").join("user-notes.txt"), &user_bytes)
            .expect("pre-existing user file");

        let files =
            build("probe", Some("node-typescript"), Some("strict")).expect("grammar-clean plan");
        let journal = match apply(root, &files) {
            ApplyOutcome::Applied(journal) => journal,
            outcome => panic!("unexpected apply outcome: {outcome:?}"),
        };

        // Exactly the three created entries: the pre-existing `lekalo`
        // never appears, and `lekalo/targets` is journaled once.
        assert_eq!(journal.0.len(), 3);
        assert!(
            matches!(&journal.0[0], JournalEntry::File(path) if path.ends_with("lekalo/project.yaml"))
        );
        assert!(
            matches!(&journal.0[1], JournalEntry::Dir(path) if path.ends_with("lekalo/targets"))
        );
        assert!(
            matches!(&journal.0[2], JournalEntry::File(path) if path.ends_with("lekalo/targets/node-typescript.yaml"))
        );

        // The rollback removes exactly those three; the user-owned
        // directory and file stay byte-identical.
        assert!(journal.rollback(root).is_empty());
        assert_eq!(
            snapshot_paths(root),
            vec!["lekalo".to_owned(), "lekalo/user-notes.txt".to_owned()]
        );
        assert_eq!(
            std::fs::read(root.join("lekalo").join("user-notes.txt")).expect("user file"),
            user_bytes
        );
    }

    /// `(relative path, is_dir)`-free helper: relative paths of every
    /// entry under `root`, deterministic order.
    fn snapshot_paths(root: &Path) -> Vec<String> {
        fn walk(current: &Path, prefix: &str, out: &mut Vec<String>) {
            let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(current)
                .expect("entries readable")
                .filter_map(|entry| entry.ok())
                .collect();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let name = entry.file_name().to_string_lossy().into_owned();
                let relative = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                };
                out.push(relative.clone());
                if entry.file_type().expect("entry type").is_dir() {
                    walk(&entry.path(), &relative, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(root, "", &mut out);
        out
    }

    /// The writer seam fails closed on every value outside its closed
    /// grammar, in debug and release builds alike.
    #[test]
    fn build_refuses_values_outside_the_closed_grammars() {
        for (project_id, target, profile) in [
            ("probe", Some("../../../escaped-outside"), None),
            ("probe", Some("/abs-escape"), None),
            ("probe", Some("back\\slash"), None),
            ("probe", Some("Bad_Target"), None),
            ("Bad_ID", Some("node-typescript"), None),
            ("probe", Some("node-typescript"), Some("Default_Profile")),
            (
                "probe",
                Some("node-typescript"),
                Some("x\", \"injected\": true}"),
            ),
            ("probe", None, Some("default")),
        ] {
            let refusal = build(project_id, target, profile).expect_err("the seam must refuse");
            assert_eq!(
                refusal.as_slice()[0].id(),
                crate::CLI_USAGE,
                "{project_id:?} {target:?} {profile:?}"
            );
        }
        // The accepted forms still plan exactly the canonical files.
        assert_eq!(build("probe", None, None).expect("no profile").len(), 1);
        assert_eq!(
            build("probe", Some("node-typescript"), Some("strict"))
                .expect("explicit profile")
                .len(),
            2
        );
    }
}
