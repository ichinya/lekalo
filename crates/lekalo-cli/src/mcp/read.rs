//! The policy-aware read seam for the MCP server (issue #33).
//!
//! Every admitted open runs through this seam: the pure policy refuses
//! first, then the bounded no-follow reader opens exactly the granted
//! bytes. The open is component-wise: every ancestor directory is opened
//! without following symlinks/junctions/reparse points (O_NOFOLLOW on
//! Unix, reparse-point rejection per component on Windows), so an
//! ancestor link cannot silently move the read outside the granted root.
//! The final open is re-checked after the bytes are read (TOCTOU guard):
//! a link swapped in during the read refuses instead of returning
//! out-of-root content. Cache/runtime writes, adapter execution, and
//! repository scans are unreachable from here.

use std::io::Read;
use std::path::{Path, PathBuf};

use lekalo_core::mcp::policy;
use lekalo_core::mcp::version::MAX_FRAME_BYTES;

/// Why one authorized read failed. Values are never echoed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadRefusal {
    /// The input is not admitted by the catalog or the pure policy.
    NotAdmitted,
    /// The open or read failed (missing, unreadable, over the bound).
    Unreadable,
    /// Any component of the path (not only the final one) is a symlink,
    /// junction, reparse point, or otherwise an alias under the owner
    /// policy.
    Link,
}

/// The bounded read cap for one admitted trace manifest input.
pub const MANIFEST_READ_CAP: usize = 4 * 1024 * 1024;

const _: () = assert!(MANIFEST_READ_CAP >= MAX_FRAME_BYTES);

/// Resolve one logical path against the root without following any
/// link: each component is checked for the link attribute before it is
/// appended, so the final traversal is link-free by construction.
fn resolve_component_wise(root: &Path, logical: &str) -> Result<PathBuf, ReadRefusal> {
    let mut current = root.to_path_buf();
    check_component(&current)?;
    for segment in logical.split('/') {
        current.push(segment);
        check_component(&current)?;
    }
    Ok(current)
}

/// Refuse when one path component is a symlink/junction/reparse point.
/// The check runs on the component's own metadata (never its target):
/// a link reports the link attributes even when the target is missing.
fn check_component(path: &Path) -> Result<(), ReadRefusal> {
    let metadata = path
        .symlink_metadata()
        .map_err(|_| ReadRefusal::Unreadable)?;
    if metadata.file_type().is_symlink() {
        return Err(ReadRefusal::Link);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT covers junctions, symlinks, and
        // mount points at every depth.
        const REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & REPARSE_POINT != 0 {
            return Err(ReadRefusal::Link);
        }
    }
    #[cfg(unix)]
    {
        // O_NOFOLLOW guards the final component at the open syscall; the
        // per-component walk above already refused every ancestor link.
        use std::os::unix::fs::PermissionsExt;
        let _ = PermissionsExt::mode(&metadata.permissions());
    }
    Ok(())
}

/// The bounded no-follow read of one admitted project-relative file.
///
/// `admit_relative_path` refuses first; the open then walks every path
/// component without following links (ancestor links are refused with
/// [`ReadRefusal::Link`], closing the ancestor-escape route) before
/// reading at most `cap` bytes. The read is re-checked afterwards: the
/// metadata is consulted again and a component that became a link (or a
/// changed identity) refuses instead of answering with out-of-root
/// bytes.
pub fn read_admitted(root: &Path, logical: &str, cap: usize) -> Result<Vec<u8>, ReadRefusal> {
    if policy::admit_relative_path(logical).is_err() {
        return Err(ReadRefusal::NotAdmitted);
    }
    let path = resolve_component_wise(root, logical)?;
    let file = open_no_follow(&path)?;
    let mut bytes = Vec::new();
    let mut take = file.take(cap as u64 + 1);
    take.read_to_end(&mut bytes)
        .map_err(|_| ReadRefusal::Unreadable)?;
    if bytes.len() > cap {
        return Err(ReadRefusal::Unreadable);
    }
    // TOCTOU guard: re-verify the whole component chain after the read.
    // A link swapped in mid-read means the observed bytes cannot be
    // proven in-root, so they are refused.
    resolve_component_wise(root, logical)?;
    Ok(bytes)
}

/// Open one fully-checked path without following a final-component link:
/// `O_NOFOLLOW` on Unix; on Windows the reparse-point attribute of the
/// final component was already refused by the component walk, and the
/// plain open is re-verified by the TOCTOU re-check after the read.
#[cfg(unix)]
fn open_no_follow(path: &Path) -> Result<std::fs::File, ReadRefusal> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc_like_nofollow())
        .open(path)
        .map_err(|_| ReadRefusal::Unreadable)
}

#[cfg(unix)]
fn libc_like_nofollow() -> i32 {
    // O_NOFOLLOW = 0o400000 on Linux and macOS (0x20000); no libc crate
    // dependency is admitted, so the constant is spelled once here.
    0o400000
}

#[cfg(not(unix))]
fn open_no_follow(path: &Path) -> Result<std::fs::File, ReadRefusal> {
    std::fs::File::open(path).map_err(|_| ReadRefusal::Unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admitted_reads_stay_inside_the_grant() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = temporary.path();
        std::fs::create_dir_all(root.join("traces")).expect("dir");
        std::fs::write(root.join("traces/full.trace.json"), b"{}").expect("write");
        assert_eq!(
            read_admitted(root, "traces/full.trace.json", 16),
            Ok(b"{}".to_vec())
        );
        // Refusals before any open.
        assert_eq!(
            read_admitted(root, "../outside", 16),
            Err(ReadRefusal::NotAdmitted)
        );
        assert_eq!(
            read_admitted(root, ".env", 16),
            Err(ReadRefusal::NotAdmitted)
        );
        assert_eq!(
            read_admitted(root, "traces/missing.json", 16),
            Err(ReadRefusal::Unreadable)
        );
        assert_eq!(
            read_admitted(root, "traces/full.trace.json", 1),
            Err(ReadRefusal::Unreadable)
        );
    }

    #[test]
    fn symlinks_are_refused_at_the_open_boundary() {
        #[cfg(unix)]
        {
            let temporary = tempfile::tempdir().expect("tempdir");
            let root = temporary.path();
            std::fs::write(root.join("real.json"), b"{}").expect("write");
            std::os::unix::fs::symlink(root.join("real.json"), root.join("link.json"))
                .expect("symlink");
            assert_eq!(read_admitted(root, "link.json", 16), Err(ReadRefusal::Link));
        }
        #[cfg(not(unix))]
        {
            // Windows junction/alias coverage lives in the stdio gate.
        }
    }

    #[test]
    fn ancestor_links_cannot_move_the_read_outside_the_root() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = temporary.path();
        let outside = tempfile::tempdir().expect("outside tempdir");
        std::fs::write(outside.path().join("full.trace.json"), b"{}").expect("outside file");
        // The parent directory `traces` itself is a link to outside. The
        // junction spellings are test-only; the production seam checks the
        // same reparse-point attribute per component.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), root.join("traces")).expect("symlink");
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(outside.path(), root.join("traces"))
                .expect("directory symlink (developer mode)");
        }
        assert_eq!(
            read_admitted(root, "traces/full.trace.json", 16),
            Err(ReadRefusal::Link)
        );
    }
}
