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

fn is_link(metadata: &std::fs::Metadata) -> bool {
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

/// Write `bytes` as `<root>/<relative_dir>/<name>`. `relative_dir` is a
/// logical POSIX path (for example `.lekalo/import/observed`); every
/// created component is checked against links before creation, and the
/// final file is written through a temporary in the same directory and
/// renamed into place.
pub(crate) fn write_confined(
    root: &Path,
    relative_dir: &str,
    name: &str,
    bytes: &[u8],
) -> Result<(), crate::diagnostics::DiagnosticSet> {
    let canonical_root = root
        .canonicalize()
        .map_err(|_| diagnostic::index_io_set("root-unreadable"))?;
    let mut prefix = canonical_root;
    for segment in relative_dir
        .split('/')
        .filter(|segment| !segment.is_empty())
    {
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
    std::fs::write(&temporary, bytes).map_err(|_| diagnostic::index_io_set("index-write"))?;
    std::fs::rename(&temporary, &target).map_err(|_| diagnostic::index_io_set("index-rename"))?;
    Ok(())
}
