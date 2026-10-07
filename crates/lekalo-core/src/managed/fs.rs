//! Confined managed filesystem access (issue #41).
//!
//! Every managed path is a project-relative coordinate resolved against one
//! project root. Raw `root.join` plus `create_dir_all`/`write` would follow
//! ancestor reparse points — NTFS junctions and mounted folders are not
//! `is_symlink()` on Windows — and let a plan escape the project, so every
//! read, write, create, and delete revalidates the whole ancestor chain
//! below the root: each existing component must be a real directory, never
//! a symlink, never a reparse point, and the leaf (when it exists) must be
//! a regular file or absent. Mutation paths revalidate immediately before
//! the byte moves, and callers re-run them after taking the exclusive
//! mutation lock so ownership is re-established under the lock.

use super::{detail, DomainResult, INPUT_INVALID};

/// The NTFS `FILE_ATTRIBUTE_REPARSE_POINT` flag: junctions, mounted
/// folders, and symlinks all carry it; ordinary directories never do.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;

#[cfg(windows)]
fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse(_metadata: &std::fs::Metadata) -> bool {
    false
}

fn unconfined(relative: &str, reason: &str) -> DomainResult {
    super::diagnostic::failure(
        INPUT_INVALID,
        &[("path", relative), ("reason", detail(reason)[0].1)],
    )
}

/// The confined logical-coordinate grammar: like the published
/// project-relative grammar, plus leading-dot segments so the reserved
/// `.lekalo` custody home is expressible. `..`, empty segments, and
/// backslashes stay refused; a leading dot never hides traversal.
fn confinable(relative: &str) -> bool {
    !relative.is_empty()
        && relative.len() <= 512
        && !relative.contains('\\')
        && !relative.contains("..")
        && relative.split('/').all(|segment| {
            !segment.is_empty() && {
                let body = segment.strip_prefix('.').unwrap_or(segment);
                !body.is_empty()
                    && body.as_bytes()[0].is_ascii_alphanumeric()
                    && body.bytes().all(|c| {
                        c.is_ascii_alphanumeric()
                            || c == b'_'
                            || c == b'.'
                            || c == b'@'
                            || c == b'-'
                    })
            }
        })
}

/// Validate every existing component below the project root: directories
/// only, never links or reparse aliases. Absent components are legal only
/// for the caller that creates them afterwards.
fn validate_chain(root: &std::path::Path, relative: &str) -> std::io::Result<()> {
    if !confinable(relative) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "rel-path",
        ));
    }
    let mut current = root.to_path_buf();
    if !current.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "root",
        ));
    }
    // Every parent component below the root must already be a real
    // directory — never a link, never a reparse alias. The leaf component
    // is validated by the caller (validate_leaf) according to operation.
    let components: Vec<&str> = relative.split('/').collect();
    for component in &components[..components.len() - 1] {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.is_symlink() || is_reparse(&metadata) || !metadata.is_dir() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "alias",
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // The remaining chain is created (and revalidated) by the
                // caller that owns the mutation.
                return Ok(());
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Validate one leaf file: a regular file, never a link or reparse alias.
fn validate_leaf(root: &std::path::Path, relative: &str) -> std::io::Result<()> {
    let metadata = match std::fs::symlink_metadata(root.join(relative)) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.is_symlink() || is_reparse(&metadata) || !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "leaf",
        ));
    }
    Ok(())
}

/// The exact bytes of one project-relative file, or `None` when absent.
/// Symlinks, reparse aliases, and special files fail closed.
pub fn read_file(root: &std::path::Path, relative: &str) -> std::io::Result<Option<Vec<u8>>> {
    validate_chain(root, relative)?;
    validate_leaf(root, relative)?;
    match std::fs::read(root.join(relative)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Write one project file, creating bounded parent directories. The whole
/// ancestor chain is validated before and revalidated after creation, so a
/// junction swapped in between checks still fails the write.
pub fn write_file(
    root: &std::path::Path,
    relative: &str,
    bytes: &[u8],
) -> Result<(), DomainResult> {
    let path = root.join(relative);
    let parent = match relative.rsplit_once('/') {
        Some((parent, _)) => parent.to_owned(),
        None => String::new(),
    };
    if validate_chain(root, relative).is_err() {
        return Err(unconfined(relative, "unconfined-path"));
    }
    if validate_leaf(root, relative).is_err() {
        return Err(unconfined(relative, "unconfined-leaf"));
    }
    if !parent.is_empty() && std::fs::create_dir_all(root.join(&parent)).is_err() {
        return Err(unconfined(relative, "publish-mkdir"));
    }
    // Revalidation under the mutation: creating the directories above is
    // exactly where an alias could have been interpolated.
    if validate_chain(root, relative).is_err() || validate_leaf(root, relative).is_err() {
        return Err(unconfined(relative, "unconfined-recheck"));
    }
    std::fs::write(&path, bytes).map_err(|_| unconfined(relative, "publish-write"))
}

/// Remove one project file; `Ok(false)` when it was already absent. The
/// leaf is validated so a link or reparse alias is never unlinked.
pub fn remove_file(root: &std::path::Path, relative: &str) -> Result<bool, DomainResult> {
    if validate_chain(root, relative).is_err() {
        return Err(unconfined(relative, "unconfined-path"));
    }
    match std::fs::symlink_metadata(root.join(relative)) {
        Ok(metadata) => {
            if metadata.is_symlink() || is_reparse(&metadata) || !metadata.is_file() {
                return Err(unconfined(relative, "unconfined-leaf"));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(unconfined(relative, "remove")),
    }
    match std::fs::remove_file(root.join(relative)) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(unconfined(relative, "publish-delete")),
    }
}

/// Create the bounded directory chain of one custody home below the root
/// and revalidate it, so every later custody write lands inside the
/// project even under ancestor aliasing.
pub fn ensure_dir(root: &std::path::Path, relative: &str) -> Result<(), DomainResult> {
    if validate_chain(root, relative).is_err() {
        return Err(unconfined(relative, "unconfined-path"));
    }
    if std::fs::create_dir_all(root.join(relative)).is_err() {
        return Err(unconfined(relative, "mkdir"));
    }
    // Revalidation under the mutation: every created component (and its
    // parents) must come back as a real directory.
    if validate_chain(root, relative).is_err() {
        return Err(unconfined(relative, "unconfined-recheck"));
    }
    let metadata = match std::fs::symlink_metadata(root.join(relative)) {
        Ok(metadata) => metadata,
        Err(_) => return Err(unconfined(relative, "unconfined-recheck")),
    };
    if metadata.is_symlink() || is_reparse(&metadata) || !metadata.is_dir() {
        return Err(unconfined(relative, "unconfined-recheck"));
    }
    Ok(())
}
