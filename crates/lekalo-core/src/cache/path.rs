//! The governed cache home under `.lekalo/cache/**` (issue #20).
//!
//! Every physical path is re-checked component by component: symlinks,
//! Windows junctions/reparse points, and special entries are policy
//! denials on sight, matching the accepted #4 read path. The cache never
//! writes outside its home and never touches `lekalo/**`, `lekalo.lock`,
//! `.lekalo/cache/migrations/**` (issue #9 custody), or any other owner
//! tree. The historical `.lekalo/cache.sqlite` placement is denied.

use std::path::{Path, PathBuf};

use super::limits::MAX_QUARANTINE_ITEMS;

/// `FILE_ATTRIBUTE_REPARSE_POINT` — every Windows reparse point (symlink,
/// junction, mount point) is denied on sight.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;

/// The validated cache home.
#[derive(Debug)]
pub(crate) struct CacheHome {
    project_root: PathBuf,
}

/// One cache-home failure: a policy denial with a stable `structure.*`
/// code, or an ordinary I/O condition that degrades to recomputation.
#[derive(Clone, Debug)]
pub(crate) enum HomeFailure {
    Denied { code: &'static str, logical: String },
    Io,
}

impl CacheHome {
    /// The read-only existence probe of the governed cache home: true
    /// when `.lekalo/cache` (or a link in its place) is present. Never
    /// creates anything; the denial classification stays with
    /// [`Self::resolve`].
    pub(crate) fn home_present(project_root: &Path) -> bool {
        std::fs::symlink_metadata(project_root.join(".lekalo").join("cache")).is_ok()
    }

    /// Resolve and preflight the home for an already-validated absolute
    /// project root. Creates `.lekalo/cache` when absent.
    pub(crate) fn resolve(project_root: &Path) -> Result<Self, HomeFailure> {
        let home = Self {
            project_root: project_root.to_path_buf(),
        };
        home.ensure_dir(".lekalo")?;
        home.ensure_dir(".lekalo/cache")?;
        Ok(home)
    }

    /// The physical path of one logical home-relative path; every existing
    /// component is reparse-checked. Missing components are returned
    /// unresolved only for the final segment.
    fn physical(&self, logical: &str) -> Result<PathBuf, HomeFailure> {
        let mut path = self.project_root.clone();
        let segments: Vec<&str> = logical.split('/').filter(|s| !s.is_empty()).collect();
        for (position, segment) in segments.iter().enumerate() {
            path.push(segment);
            let last = position + 1 == segments.len();
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() {
                        return Err(Self::denied("structure.path-link", logical));
                    }
                    #[cfg(windows)]
                    if reparse_point(&metadata) {
                        return Err(Self::denied("structure.path-link", logical));
                    }
                }
                // The final component may legitimately not exist yet (a
                // fresh database, an absent sidecar); intermediate
                // components must exist.
                Err(error) if last && error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(HomeFailure::Io),
            }
        }
        Ok(path)
    }

    /// Ensure one logical directory chain exists with real directories,
    /// rejecting any existing symlink/reparse or non-directory component.
    fn ensure_dir(&self, logical: &str) -> Result<(), HomeFailure> {
        let mut path = self.project_root.clone();
        for segment in logical.split('/').filter(|segment| !segment.is_empty()) {
            path.push(segment);
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() {
                        return Err(Self::denied("structure.path-link", logical));
                    }
                    #[cfg(windows)]
                    if reparse_point(&metadata) {
                        return Err(Self::denied("structure.path-link", logical));
                    }
                    if !metadata.is_dir() {
                        return Err(Self::denied("structure.runtime-unexpected-entry", logical));
                    }
                }
                Err(_) => {
                    std::fs::create_dir(&path).map_err(|_| HomeFailure::Io)?;
                }
            }
        }
        Ok(())
    }

    /// The physical database file path after a final reparse check.
    pub(crate) fn database_path(&self) -> Result<PathBuf, HomeFailure> {
        let path = self.physical(".lekalo/cache/cache.sqlite")?;
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() {
                return Err(Self::denied(
                    "structure.path-link",
                    ".lekalo/cache/cache.sqlite",
                ));
            }
            #[cfg(windows)]
            if reparse_point(&metadata) {
                return Err(Self::denied(
                    "structure.path-link",
                    ".lekalo/cache/cache.sqlite",
                ));
            }
        }
        Ok(path)
    }

    /// Whether the database file exists as a regular entry.
    pub(crate) fn database_exists(&self) -> Result<bool, HomeFailure> {
        match std::fs::symlink_metadata(self.physical(".lekalo/cache/cache.sqlite")?) {
            Ok(metadata) => Ok(metadata.is_file()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(HomeFailure::Io),
        }
    }

    /// Quarantine one corrupt store: move the database and its sidecars
    /// into `.lekalo/cache/quarantine/` under a digest-derived opaque
    /// name. Never overwrites an existing item; bounded item count; the
    /// bytes are gone when every slot is taken.
    pub(crate) fn quarantine_store(&self, sidecars: &[&str]) -> Result<(), HomeFailure> {
        self.ensure_dir(".lekalo/cache/quarantine")?;
        let quarantine = self.physical(".lekalo/cache/quarantine")?;
        let existing = std::fs::read_dir(&quarantine).map_err(|_| HomeFailure::Io)?;
        if existing.count() >= MAX_QUARANTINE_ITEMS {
            // The quarantine is full: drop the corrupt bytes instead.
            for name in sidecars {
                if let Ok(path) = self.physical(name) {
                    let _ = std::fs::remove_file(path);
                }
            }
            return Ok(());
        }
        let stamp = super::canonical::hex_digest(
            &std::fs::read(self.physical(sidecars[0])?).map_err(|_| HomeFailure::Io)?,
        )[..16]
            .to_owned();
        for name in sidecars {
            let file = std::fs::symlink_metadata(self.physical(name)?);
            let Ok(_) = file else { continue };
            let file_name = name.rsplit('/').next().unwrap_or(name);
            let extension = file_name
                .split_once('.')
                .map(|(_, rest)| rest)
                .unwrap_or("bin");
            for slot in 0..MAX_QUARANTINE_ITEMS {
                let target = quarantine.join(format!("cache-{stamp}-{slot}.{extension}"));
                if std::fs::symlink_metadata(&target).is_ok() {
                    continue;
                }
                let _ = std::fs::rename(self.physical(name)?, target);
                break;
            }
        }
        Ok(())
    }

    /// Remove every cache entry except the migration home (issue #9
    /// custody: journals and immutable backups stay). Returns the number
    /// of top-level entries removed.
    pub(crate) fn clear(&self) -> Result<usize, HomeFailure> {
        let cache = self.physical(".lekalo/cache")?;
        let entries = std::fs::read_dir(&cache).map_err(|_| HomeFailure::Io)?;
        let mut removed = 0usize;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name == "migrations" {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => return Err(HomeFailure::Io),
            };
            if metadata.is_symlink() {
                return Err(Self::denied(
                    "structure.path-link",
                    &format!(".lekalo/cache/{name}"),
                ));
            }
            #[cfg(windows)]
            if reparse_point(&metadata) {
                return Err(Self::denied(
                    "structure.path-link",
                    &format!(".lekalo/cache/{name}"),
                ));
            }
            let path = cache.join(name);
            let result = if metadata.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            result.map_err(|_| HomeFailure::Io)?;
            removed += 1;
        }
        Ok(removed)
    }

    /// The bounded total bytes of the home, excluding the migration home.
    /// Returns `None` when the home is not readable.
    pub(crate) fn total_bytes(&self) -> Option<u64> {
        let cache = self.physical(".lekalo/cache").ok()?;
        Self::walk_bytes(&cache, 0)
    }

    fn walk_bytes(dir: &Path, depth: usize) -> Option<u64> {
        if depth > super::super::project_fs::MAX_WALK_DEPTH {
            return None;
        }
        let entries = std::fs::read_dir(dir).ok()?;
        let mut total = 0u64;
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let name = entry.file_name();
            if name.to_str() == Some("migrations") {
                continue;
            }
            if metadata.is_dir() {
                total += Self::walk_bytes(&entry.path(), depth + 1)?;
            } else if metadata.is_file() {
                total += metadata.len();
            }
        }
        Some(total)
    }

    fn denied(code: &'static str, logical: &str) -> HomeFailure {
        HomeFailure::Denied {
            code,
            logical: logical.to_owned(),
        }
    }
}

#[cfg(windows)]
fn reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lekalo-cache-home-{tag}-{}-{}",
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
    fn resolve_creates_the_governed_home() {
        let root = temp_root("create");
        let home = CacheHome::resolve(&root).expect("home resolves");
        assert!(root.join(".lekalo/cache").is_dir());
        assert!(!home.database_exists().expect("probe"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_symlinked_cache_home_is_a_policy_denial() {
        let root = temp_root("symlink");
        std::fs::create_dir_all(root.join(".lekalo")).expect("runtime dir");
        let outside = temp_root("symlink-target");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join(".lekalo/cache")).expect("symlink");
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&outside, root.join(".lekalo/cache")).expect("symlink");
        let failure = CacheHome::resolve(&root).expect_err("denied");
        match failure {
            HomeFailure::Denied { code, logical } => {
                assert_eq!(code, "structure.path-link");
                assert_eq!(logical, ".lekalo/cache");
            }
            HomeFailure::Io => panic!("a link is a denial, not an I/O miss"),
        }
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn clear_removes_entries_but_keeps_the_migration_home() {
        let root = temp_root("clear");
        let home = CacheHome::resolve(&root).expect("home resolves");
        std::fs::create_dir_all(root.join(".lekalo/cache/migrations/journal")).expect("migrations");
        std::fs::write(root.join(".lekalo/cache/migrations/journal/plan"), b"keep")
            .expect("journal");
        std::fs::write(root.join(".lekalo/cache/cache.sqlite"), b"stale").expect("db");
        let removed = home.clear().expect("clear");
        assert_eq!(removed, 1);
        assert!(!root.join(".lekalo/cache/cache.sqlite").exists());
        assert_eq!(
            std::fs::read(root.join(".lekalo/cache/migrations/journal/plan")).expect("kept"),
            b"keep"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clear_denies_a_linked_entry_inside_the_home() {
        let root = temp_root("clear-link");
        let home = CacheHome::resolve(&root).expect("home resolves");
        let outside = temp_root("clear-link-target");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join(".lekalo/cache/escape")).expect("link");
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&outside, root.join(".lekalo/cache/escape"))
            .expect("link");
        let failure = home.clear().expect_err("denied");
        assert!(matches!(failure, HomeFailure::Denied { .. }));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }
}
