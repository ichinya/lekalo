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

/// The number of directory entries referencing the file: `1` for an
/// exclusively owned file, more for a hardlink. A hardlink is a regular
/// file, so the reparse-point checks above accept it — but an in-place
/// write would truncate the bytes of every alias, including one outside
/// the project (finding 9). Multi-link leaves are refused on every
/// mutation path; single-link leaves can only alias themselves. std
/// exposes no stable link count on Windows (`windows_by_handle` is
/// unstable), so the count rides the already-pinned `windows-sys`
/// dependency; an unreadable count refuses (fail closed).
fn link_count(path: &std::path::Path, metadata: &std::fs::Metadata) -> Option<u64> {
    #[cfg(windows)]
    {
        let _ = metadata;
        windows_link_count(path).map(u64::from)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        use std::os::unix::fs::MetadataExt;
        Some(metadata.nlink())
    }
}

#[cfg(windows)]
fn windows_link_count(path: &std::path::Path) -> Option<u32> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let file = std::fs::File::open(path).ok()?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) };
    if ok == 0 {
        return None;
    }
    Some(info.nNumberOfLinks)
}

/// Whether the existing leaf is a plain, exclusively owned regular file:
/// never a link or reparse alias, and never a shared inode (finding 9). An
/// unreadable link count refuses.
fn exclusive_leaf(path: &std::path::Path, metadata: &std::fs::Metadata) -> bool {
    !metadata.is_symlink()
        && !is_reparse(metadata)
        && metadata.is_file()
        && link_count(path, metadata) == Some(1)
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

/// Whether one existing leaf is a shared alias: a link, reparse point, or
/// multi-link inode (a hardlink). Exposed for preflights that must refuse a
/// blocked destination before any staged byte is spent (finding 9).
pub fn is_shared_alias(path: &std::path::Path, metadata: &std::fs::Metadata) -> bool {
    metadata.is_symlink()
        || is_reparse(metadata)
        || !metadata.is_file()
        || link_count(path, metadata) != Some(1)
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
/// junction swapped in between checks still fails the write. A multi-link
/// leaf (a hardlink alias, possibly outside the project) is refused before
/// any byte moves: an in-place write must never truncate another owner's
/// bytes (finding 9).
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
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if !exclusive_leaf(&path, &metadata) {
            return Err(unconfined(relative, "shared-inode"));
        }
    }
    if !parent.is_empty() && std::fs::create_dir_all(root.join(&parent)).is_err() {
        return Err(unconfined(relative, "publish-mkdir"));
    }
    // Revalidation under the mutation: creating the directories above is
    // exactly where an alias could have been interpolated.
    if validate_chain(root, relative).is_err() || validate_leaf(root, relative).is_err() {
        return Err(unconfined(relative, "unconfined-recheck"));
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if !exclusive_leaf(&path, &metadata) {
            return Err(unconfined(relative, "shared-inode"));
        }
    }
    // The bytes land through a fresh staged file and an atomic rename
    // (finding 9): an in-place write would truncate a shared inode, so the
    // destination entry is replaced atomically instead — an alias planted
    // at the path keeps its own bytes and the project leaf becomes an
    // exclusively owned file.
    let stage = format!("{relative}.stage");
    write_staged(root, relative, &stage, bytes)
}

/// Stage fresh bytes at a reserved staging coordinate and atomically move
/// them onto `destination`: the stage file is created with `create_new`, so
/// an existing leaf — including a hardlink planted at the stage name — is
/// never truncated, and the final `rename` replaces the destination's
/// directory entry instead of writing through a shared inode. Both
/// coordinates are confined; the stage must be absent or removable as a
/// plain single-link file before anything is created (finding 9).
pub fn write_staged(
    root: &std::path::Path,
    destination: &str,
    stage: &str,
    bytes: &[u8],
) -> Result<(), DomainResult> {
    // The staging coordinate is validated exactly like the destination
    // (r4 finding 8): every current caller derives it from the validated
    // destination, but a future caller passing a caller-influenced stage
    // name must not be able to stage outside the confined grammar.
    if !confinable(stage) || validate_chain(root, stage).is_err() {
        return Err(unconfined(stage, "unconfined-path"));
    }
    let stage_path = root.join(stage);
    // The stage leaf must not exist, or must be removable as a plain
    // single-link file (a stale stage from a crashed run). A link,
    // directory, or shared inode at the stage name refuses.
    match std::fs::symlink_metadata(&stage_path) {
        Ok(metadata) => {
            if !exclusive_leaf(&stage_path, &metadata) {
                return Err(unconfined(stage, "unconfined-leaf"));
            }
            remove_file(root, stage).map_err(|_| unconfined(stage, "stage"))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(unconfined(stage, "stage")),
    }
    // The destination must be a confinable coordinate whose parent chain
    // is real directories (the destination leaf itself may be absent or a
    // plain single-link file — the rename replaces it atomically). The
    // parent chain is created and revalidated here, like write_file.
    let parent = match destination.rsplit_once('/') {
        Some((parent, _)) => parent,
        None => "",
    };
    if !parent.is_empty() && ensure_dir(root, parent).is_err() {
        return Err(unconfined(destination, "publish-mkdir"));
    }
    if validate_chain(root, destination).is_err() {
        return Err(unconfined(destination, "unconfined-path"));
    }
    if let Ok(metadata) = std::fs::symlink_metadata(root.join(destination)) {
        if !exclusive_leaf(&root.join(destination), &metadata) {
            return Err(unconfined(destination, "unconfined-leaf"));
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stage_path)
        .map_err(|_| unconfined(stage, "stage"))?;
    use std::io::Write;
    if file.write_all(bytes).is_err() || file.flush().is_err() {
        let _ = std::fs::remove_file(&stage_path);
        return Err(unconfined(stage, "stage"));
    }
    drop(file);
    if std::fs::rename(&stage_path, root.join(destination)).is_err() {
        let _ = std::fs::remove_file(&stage_path);
        return Err(unconfined(destination, "publish-write"));
    }
    Ok(())
}

/// Remove one project file; `Ok(false)` when it was already absent. The
/// leaf is validated so a link or reparse alias is never unlinked, and a
/// shared inode (hardlink alias) is refused like any other mutation
/// (finding 9): removal is confined to exclusively owned leaves.
pub fn remove_file(root: &std::path::Path, relative: &str) -> Result<bool, DomainResult> {
    if validate_chain(root, relative).is_err() {
        return Err(unconfined(relative, "unconfined-path"));
    }
    match std::fs::symlink_metadata(root.join(relative)) {
        Ok(metadata) => {
            if !exclusive_leaf(&root.join(relative), &metadata) {
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
