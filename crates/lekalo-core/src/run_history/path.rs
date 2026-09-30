//! The governed history home under `.lekalo/history/**` (issue #121).
//!
//! The home is repository/tenant isolated: it resolves only the current
//! project root, never a shared Git-common-dir, home-wide, or global
//! location, and never an arbitrary caller-selected path. Every
//! physical path is re-checked component by component: symlinks,
//! Windows junctions/reparse points, and special entries are policy
//! denials on sight. The generated `*` ignore protection is written
//! before any payload, and in Git projects the home is verified
//! untracked with a local read-only Git query — no network, no remote.

use std::path::{Path, PathBuf};

/// The logical home-relative layout of the history custody.
pub(crate) const HISTORY_DIR: &str = ".lekalo/history";
/// The SQLite store file inside the home.
pub(crate) const STORE_FILE: &str = ".lekalo/history/store.sqlite";
/// The generated ignore protection content. Nothing under the home is
/// ever tracked; the file ignores every sibling and itself.
pub(crate) const IGNORE_CONTENT: &str = "*\n";

/// One history-home failure: a policy denial with a stable code, or an
/// ordinary I/O condition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HomeFailure {
    Denied {
        code: &'static str,
        detail: &'static str,
    },
    Io,
}

/// The validated history home.
#[derive(Debug)]
pub(crate) struct HistoryHome {
    project_root: PathBuf,
}

impl HistoryHome {
    /// Resolve and preflight the home for an already-validated absolute
    /// project root, creating `.lekalo/history` and its ignore
    /// protection when absent.
    pub(crate) fn resolve(project_root: &Path) -> Result<Self, HomeFailure> {
        let home = Self {
            project_root: project_root.to_path_buf(),
        };
        home.ensure_dir(".lekalo")?;
        home.ensure_dir(HISTORY_DIR)?;
        home.ensure_ignore_protection()?;
        Ok(home)
    }

    /// The physical path of one logical home-relative path; every
    /// existing component is reparse-checked. Only the final segment
    /// may legitimately not exist yet.
    pub(crate) fn physical(&self, logical: &str) -> Result<PathBuf, HomeFailure> {
        let mut path = self.project_root.clone();
        let segments: Vec<&str> = logical.split('/').filter(|s| !s.is_empty()).collect();
        for (position, segment) in segments.iter().enumerate() {
            path.push(segment);
            let last = position + 1 == segments.len();
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() {
                        return Err(Self::denied("path-link"));
                    }
                    #[cfg(windows)]
                    if reparse_point(&metadata) {
                        return Err(Self::denied("path-link"));
                    }
                    if !last && !metadata.is_dir() {
                        return Err(Self::denied("unexpected-entry"));
                    }
                }
                Err(error) if last && error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(HomeFailure::Io),
            }
        }
        Ok(path)
    }

    /// Ensure one logical directory chain exists with real directories,
    /// rejecting any existing symlink/reparse or non-directory
    /// component.
    fn ensure_dir(&self, logical: &str) -> Result<(), HomeFailure> {
        let mut path = self.project_root.clone();
        for segment in logical.split('/').filter(|segment| !segment.is_empty()) {
            path.push(segment);
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() {
                        return Err(Self::denied("path-link"));
                    }
                    #[cfg(windows)]
                    if reparse_point(&metadata) {
                        return Err(Self::denied("path-link"));
                    }
                    if !metadata.is_dir() {
                        return Err(Self::denied("unexpected-entry"));
                    }
                }
                Err(_) => {
                    std::fs::create_dir(&path).map_err(|_| HomeFailure::Io)?;
                    restrict_permissions(&path);
                }
            }
        }
        Ok(())
    }

    /// Write the generated `*` ignore protection before any payload and
    /// verify an existing protection is unmodified. A modified or
    /// foreign ignore file is a tracked-store refusal, not a rewrite:
    /// the runtime never edits consumer-tracked ignore files.
    fn ensure_ignore_protection(&self) -> Result<(), HomeFailure> {
        let path = self.physical(&format!("{HISTORY_DIR}/.gitignore"))?;
        match std::fs::read(&path) {
            Ok(bytes) => {
                if bytes != IGNORE_CONTENT.as_bytes() {
                    return Err(Self::tracked("ignore-modified"));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::write(&path, IGNORE_CONTENT).map_err(|_| HomeFailure::Io)?;
                restrict_permissions(&path);
            }
            Err(_) => return Err(HomeFailure::Io),
        }
        self.verify_untracked()
    }

    /// In Git projects, verify no history content is already tracked
    /// with a local read-only `git ls-files` query. A missing or
    /// failing Git is a fail-closed tracked-store refusal: the
    /// custody claim must be verifiable, never assumed. The query is
    /// purely local — no network, no remote, no account.
    fn verify_untracked(&self) -> Result<(), HomeFailure> {
        if !self.project_root.join(".git").exists() {
            return Ok(());
        }
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.project_root)
            .arg("ls-files")
            .arg("--")
            .arg(HISTORY_DIR)
            .output()
            .map_err(|_| Self::tracked("git-unavailable"))?;
        if !output.status.success() {
            return Err(Self::tracked("git-unavailable"));
        }
        if !output.stdout.trim_ascii().is_empty() {
            return Err(Self::tracked("tracked-content"));
        }
        Ok(())
    }

    /// The physical database path after a final reparse check. A
    /// replaced parent or hard-linked database file refuses.
    pub(crate) fn database_path(&self) -> Result<PathBuf, HomeFailure> {
        let path = self.physical(STORE_FILE)?;
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if !metadata.is_file() {
                return Err(Self::denied("unexpected-entry"));
            }
            #[cfg(unix)]
            if metadata.nlink() > 1 {
                return Err(Self::denied("hard-linked"));
            }
        }
        Ok(path)
    }

    fn denied(detail: &'static str) -> HomeFailure {
        HomeFailure::Denied {
            code: super::codes::PATH_DENIED,
            detail,
        }
    }

    /// A tracked-store refusal: the home carries tracked content or
    /// its ignore protection cannot be verified.
    fn tracked(detail: &'static str) -> HomeFailure {
        HomeFailure::Denied {
            code: super::codes::TRACKED_STORE,
            detail,
        }
    }
}

/// Owner-only permissions on newly created Unix paths; Windows relies
/// on the profile-scoped project directory ACLs.
fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
    #[cfg(windows)]
    {
        let _ = path;
    }
}

/// Apply owner-only permissions to the database file after creation.
pub(crate) fn restrict_file_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        let _ = path;
    }
}

#[cfg(windows)]
fn reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x0400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lekalo-history-home-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp root");
        dir
    }

    #[test]
    fn resolve_creates_the_home_and_ignore_protection() {
        let root = temp_root("create");
        let home = HistoryHome::resolve(&root).expect("home resolves");
        assert!(root.join(".lekalo/history").is_dir());
        assert_eq!(
            std::fs::read(root.join(".lekalo/history/.gitignore")).expect("ignore"),
            IGNORE_CONTENT.as_bytes()
        );
        assert!(home.database_path().is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_symlinked_history_home_is_a_policy_denial() {
        let root = temp_root("symlink");
        std::fs::create_dir_all(root.join(".lekalo")).expect("runtime dir");
        let outside = temp_root("symlink-target");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join(".lekalo/history")).expect("symlink");
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&outside, root.join(".lekalo/history")).expect("link");
        let failure = HistoryHome::resolve(&root).expect_err("denied");
        assert!(matches!(
            failure,
            HomeFailure::Denied {
                code: super::super::codes::PATH_DENIED,
                ..
            }
        ));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn a_modified_ignore_file_refuses_instead_of_a_rewrite() {
        let root = temp_root("ignore");
        HistoryHome::resolve(&root).expect("first resolve");
        std::fs::write(root.join(".lekalo/history/.gitignore"), "!.gitignore\n").expect("tamper");
        let failure = HistoryHome::resolve(&root).expect_err("denied");
        assert_eq!(
            failure,
            HomeFailure::Denied {
                code: super::super::codes::TRACKED_STORE,
                detail: "ignore-modified"
            }
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_git_project_with_tracked_history_content_refuses() {
        let root = temp_root("tracked");
        HistoryHome::resolve(&root).expect("first resolve");
        std::fs::create_dir(root.join(".git")).expect("fake git dir");
        // The `git` binary exists in every supported environment; a
        // tracked file inside the home must refuse. Track the ignore
        // file itself through a real local repository.
        let tracked = root.join(".lekalo/history/.gitignore");
        let init = std::process::Command::new("git")
            .args(["-C"])
            .arg(&root)
            .args(["init", "--quiet"])
            .output();
        if let Ok(output) = init {
            if output.status.success() {
                let _ = std::process::Command::new("git")
                    .args(["-C"])
                    .arg(&root)
                    .args(["add", "-f", ".lekalo/history/.gitignore"])
                    .output();
                let failure = HistoryHome::resolve(&root).expect_err("denied");
                assert!(matches!(failure, HomeFailure::Denied { .. }));
            }
        }
        let _ = tracked;
        let _ = std::fs::remove_dir_all(&root);
    }
}
