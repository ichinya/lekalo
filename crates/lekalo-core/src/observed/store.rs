//! Confined index persistence (issue #39).
//!
//! The only writer in the observed seam. Every write creates its
//! directory chain under the project root with per-component
//! symlink/junction/reparse rejection, writes a temporary file in the
//! final directory, and renames it into place. No absolute path ever
//! enters the persisted bytes.

use std::path::Path;

use super::diagnostic;

/// `FILE_ATTRIBUTE_REPARSE_POINT` — every Windows reparse point must be
/// rejected on sight.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;

/// Whether the metadata carries a symlink/junction/reparse point: the
/// shared rejection predicate of every confined writer and reader.
pub(crate) fn is_link(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(unix)]
    {
        false
    }
}

/// The confined logical-path grammar every writer and journal consumer
/// shares: each `/`-separated segment is a bounded, printable, non-empty
/// name that is never `.` or `..`, never a Windows drive root, and never
/// a separator smuggled inside a segment. The final file name follows
/// the same rules (plus: never empty).
pub(crate) fn is_confined_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment.len() <= 255
        && segment != "."
        && segment != ".."
        && !segment
            .bytes()
            .any(|byte| matches!(byte, b'/' | b'\\' | b':') || byte.is_ascii_control())
}

/// Write `bytes` as `<root>/<relative_dir>/<name>`. `relative_dir` is a
/// logical POSIX path (for example `.lekalo/import/observed`); every
/// created component is checked against links before creation, and the
/// final file is written through a temporary in the same directory and
/// renamed into place. The logical path itself is grammar-checked
/// first — `.`/`..`/absolute/separator-bearing segments never become
/// filesystem components at all, so an untrusted journal or document
/// path cannot walk outside the project root through this primitive.
pub(crate) fn write_confined(
    root: &Path,
    relative_dir: &str,
    name: &str,
    bytes: &[u8],
) -> Result<(), crate::diagnostics::DiagnosticSet> {
    if !is_confined_segment(name) {
        return Err(diagnostic::index_io_set("path-grammar"));
    }
    // Absolute and backslash-bearing logical paths never enter the
    // component walk at all (F1/R4).
    if relative_dir.starts_with('/') || relative_dir.contains('\\') {
        return Err(diagnostic::index_io_set("path-grammar"));
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|_| diagnostic::index_io_set("root-unreadable"))?;
    let mut prefix = canonical_root;
    for segment in relative_dir.split('/') {
        if segment.is_empty() {
            // The canonical logical path never carries empty segments
            // (a leading `/`, a trailing `/`, or `//`).
            return Err(diagnostic::index_io_set("path-grammar"));
        }
        if !is_confined_segment(segment) {
            return Err(diagnostic::index_io_set("path-grammar"));
        }
        prefix.push(segment);
        match std::fs::symlink_metadata(&prefix) {
            Ok(metadata) => {
                if is_link(&metadata) {
                    return Err(diagnostic::index_io_set("path-link"));
                }
                if !metadata.is_dir() {
                    return Err(diagnostic::index_io_set("path-not-directory"));
                }
            }
            Err(_) => {
                std::fs::create_dir(&prefix)
                    .map_err(|_| diagnostic::index_io_set("directory-create"))?;
            }
        }
    }
    let target = prefix.join(name);
    if let Ok(metadata) = std::fs::symlink_metadata(&target) {
        if is_link(&metadata) {
            return Err(diagnostic::index_io_set("path-link"));
        }
    }
    let temporary = prefix.join(format!("{name}.tmp"));
    let _ = std::fs::remove_file(&temporary);
    if std::fs::write(&temporary, bytes).is_err() {
        // Never leave a partial temporary behind on a failed write.
        let _ = std::fs::remove_file(&temporary);
        return Err(diagnostic::index_io_set("index-write"));
    }
    if std::fs::rename(&temporary, &target).is_err() {
        // An obstructed destination (a directory at the target, a
        // held lock) must not orphan the temporary file next to it.
        let _ = std::fs::remove_file(&temporary);
        return Err(diagnostic::index_io_set("index-rename"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// F1/R4: the shared confined primitive refuses every traversal and
    /// separator-bearing logical path before any filesystem component
    /// exists — an untrusted journal or document path can never walk
    /// outside the project root through `write_confined`.
    #[test]
    fn write_confined_refuses_traversal_and_separator_segments() {
        let root = tempfile::tempdir().expect("tempdir");
        for dir in [
            "..",
            "../outside",
            "a/../b",
            "a/./b",
            "/absolute",
            "a//b",
            "a\\b",
            "C:/tmp",
        ] {
            assert!(
                write_confined(root.path(), dir, "file.txt", b"x").is_err(),
                "dir {dir} must refuse"
            );
        }
        for name in ["..", ".", "../escape", "a/b", "a\\b", "", "con:file"] {
            assert!(
                write_confined(root.path(), ".lekalo/import", name, b"x").is_err(),
                "name {name:?} must refuse"
            );
        }
        // Nothing outside the root was created by the refusals.
        assert!(std::fs::read_dir(root.path())
            .expect("root readable")
            .all(|entry| entry.expect("entry").path().starts_with(root.path())));
        // The confined form still writes.
        assert!(write_confined(root.path(), ".lekalo/import", "file.txt", b"x").is_ok());
        assert!(root.path().join(".lekalo/import/file.txt").is_file());
    }
}
