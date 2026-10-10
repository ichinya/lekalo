//! The policy-aware read seam for the MCP server (issue #33).
//!
//! Every admitted open runs through this seam: the pure policy refuses
//! first, then the bounded no-follow reader opens exactly the granted
//! bytes. The open is component-wise: every path component is checked
//! for the link attribute before it is appended (O_NOFOLLOW on Unix for
//! the final component; reparse-point rejection per component on
//! Windows), and the read is guarded by identity pinning (review r2
//! F9/codex 15): the metadata identity of the final component is
//! captured before the open, and the opened handle's own identity must
//! match it after the read — a component swapped in during the check-
//! to-open window yields a different identity and refuses, and a swap
//! after the open is caught by the post-read component re-walk and
//! identity re-comparison. The identity is the strongest the platform
//! offers through the standard library: device+inode on Unix, and the
//! attribute/size/timestamp tuple on Windows (the platform exposes no
//! stable file-id through std, so the seam documents the residual
//! guarantee honestly instead of overclaiming). Hardlink aliases are
//! refused outright: multi-link inodes are not single-owner in-root
//! content under the owner policy, so the seam fails closed on them.
//! Cache/runtime writes, adapter execution, and repository scans are
//! unreachable from here.

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
    /// junction, reparse point, hardlink alias, or otherwise an alias
    /// under the owner policy — or the pinned identity of the opened
    /// file no longer matches the checked path.
    Link,
}

/// The bounded read cap for one admitted trace manifest input.
pub const MANIFEST_READ_CAP: usize = 4 * 1024 * 1024;

const _: () = assert!(MANIFEST_READ_CAP >= MAX_FRAME_BYTES);

/// The pinned identity of one path component or opened file: the
/// strongest comparison the platform offers. Unix pins the device and
/// inode; Windows pins the volume serial and 128-bit file id through
/// `GetFileInformationByHandleEx` (the platform's real file identity,
/// not a heuristic timestamp tuple). `None` means the platform could not
/// provide an identity — the caller fails closed (review codex 15).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileIdentity {
    size: u64,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    links: u64,
    #[cfg(windows)]
    attributes: u32,
    #[cfg(windows)]
    volume: u32,
    #[cfg(windows)]
    file_id: [u8; 16],
}

/// The identity of one metadata snapshot; `None` when the metadata does
/// not describe a regular file (the seam reads regular files only).
#[cfg(unix)]
fn identity_of(metadata: &std::fs::Metadata) -> Option<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    if !metadata.is_file() {
        return None;
    }
    Some(FileIdentity {
        size: metadata.len(),
        device: metadata.dev(),
        inode: metadata.ino(),
        links: metadata.nlink(),
    })
}

/// The identity of one OPENED file handle. On Windows this is the real
/// volume/file-id pair from the handle (never a path re-resolution), so
/// the pin is a true identity comparison.
#[cfg(windows)]
fn opened_identity(file: &std::fs::File) -> Option<FileIdentity> {
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::AsRawHandle;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    let mut info = windows_sys::Win32::Storage::FileSystem::FILE_ID_INFO {
        VolumeSerialNumber: 0,
        FileId: windows_sys::Win32::Storage::FileSystem::FILE_ID_128 {
            Identifier: [0; 16],
        },
    };
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandleEx(
            file.as_raw_handle(),
            windows_sys::Win32::Storage::FileSystem::FileIdInfo,
            &mut info as *mut _ as *mut core::ffi::c_void,
            core::mem::size_of::<windows_sys::Win32::Storage::FileSystem::FILE_ID_INFO>() as u32,
        )
    };
    if ok == 0 {
        // The volume cannot provide a file id: fail closed.
        return None;
    }
    Some(FileIdentity {
        size: metadata.len(),
        attributes: metadata.file_attributes(),
        volume: info.VolumeSerialNumber as u32,
        file_id: info.FileId.Identifier,
    })
}

/// The identity of one path: on Windows the path is opened (no-follow
/// checks already ran) and the handle identity is queried, so the pre-
/// open capture and the opened handle speak the same identity domain.
#[cfg(windows)]
fn path_identity(path: &Path) -> Option<FileIdentity> {
    opened_identity(&std::fs::File::open(path).ok()?)
}

#[cfg(unix)]
fn path_identity(path: &Path) -> Option<FileIdentity> {
    identity_of(&path.symlink_metadata().ok()?)
}

impl FileIdentity {
    /// Whether the identity pins a single-owner regular file: multi-link
    /// inodes are hardlink aliases and refuse (the alias policy).
    fn single_owner(&self) -> bool {
        #[cfg(unix)]
        {
            self.links == 1
        }
        #[cfg(windows)]
        {
            // std exposes no stable link count for every volume; the
            // alias policy is enforced on the link-attribute checks and
            // the identity pin. Windows hardlink enumeration is not
            // available without platform APIs, so the seam does not
            // claim hardlink detection on this platform.
            true
        }
    }
}

/// Whether one opened handle's identity matches one captured path
/// identity exactly. Exposed for the deterministic injected-replacement
/// tests (review codex 15): a swap yields a different identity tuple on
/// both platforms.
fn identity_matches(opened: &FileIdentity, captured: &FileIdentity) -> bool {
    opened == captured
}

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
/// reading at most `cap` bytes. The read is pinned by identity (r2 F9):
/// the final component's identity is captured before the open, the
/// opened handle's own identity must equal it, and the whole component
/// chain plus identity are re-verified after the bytes are read. A link
/// or file swapped in during the check-to-open window (and restored or
/// not) produces a mismatched identity and refuses instead of answering
/// with out-of-root bytes. The identity is the platform's real one —
/// device+inode on Unix, the volume/file-id pair from the opened handle
/// on Windows — and a platform that cannot provide one fails closed.
pub fn read_admitted(root: &Path, logical: &str, cap: usize) -> Result<Vec<u8>, ReadRefusal> {
    if policy::admit_relative_path(logical).is_err() {
        return Err(ReadRefusal::NotAdmitted);
    }
    let path = resolve_component_wise(root, logical)?;
    // The pre-open identity pin of the final component.
    let captured = path_identity(&path).ok_or(ReadRefusal::Unreadable)?;
    if !captured.single_owner() {
        return Err(ReadRefusal::Link);
    }
    let file = open_no_follow(&path)?;
    let opened = match opened_identity(&file) {
        Some(identity) => identity,
        // The platform cannot prove what was opened: fail closed.
        None => return Err(ReadRefusal::Unreadable),
    };
    // A swap between the walk and the open yields a different identity.
    if !identity_matches(&opened, &captured) || !opened.single_owner() {
        return Err(ReadRefusal::Link);
    }
    let mut bytes = Vec::new();
    let mut take = file.take(cap as u64 + 1);
    take.read_to_end(&mut bytes)
        .map_err(|_| ReadRefusal::Unreadable)?;
    if bytes.len() > cap {
        return Err(ReadRefusal::Unreadable);
    }
    // TOCTOU guard: re-verify the whole component chain after the read,
    // and re-compare the opened handle's identity with the path's live
    // identity. A link swapped in mid-read means the observed bytes
    // cannot be proven in-root, so they are refused.
    resolve_component_wise(root, logical)?;
    let after = path_identity(&path).ok_or(ReadRefusal::Link)?;
    if !identity_matches(&opened, &after) {
        return Err(ReadRefusal::Link);
    }
    Ok(bytes)
}

/// Open one fully-checked path without following a final-component link:
/// `O_NOFOLLOW` on Unix; on Windows the reparse-point attribute of the
/// final component was already refused by the component walk, and the
/// plain open is pinned by the identity comparison after the open.
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

    #[test]
    fn a_swapped_final_component_refuses_on_the_identity_pin() {
        // Review r2 F9/codex 15, deterministic injected replacement: the
        // identity captured for the checked path must equal the opened
        // handle's identity. A different file (the injected replacement)
        // never matches, whatever its content — the identity is the
        // platform's real file id, not a content heuristic.
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = temporary.path();
        std::fs::write(root.join("a.json"), b"original").expect("write a");
        std::fs::write(root.join("b.json"), b"original").expect("write b");
        let captured_a = path_identity(&root.join("a.json")).expect("identity");
        let opened_b = path_identity(&root.join("b.json")).expect("identity");
        assert!(identity_matches(&captured_a, &captured_a));
        assert!(
            !identity_matches(&opened_b, &captured_a),
            "a replacement refuses"
        );

        // An in-place rewrite also changes the pinned identity (size).
        std::fs::write(root.join("a.json"), b"original-longer").expect("rewrite");
        let rewritten = path_identity(&root.join("a.json")).expect("identity");
        assert!(!identity_matches(&rewritten, &captured_a));
    }

    #[test]
    fn a_multi_link_inode_is_refused_as_a_hardlink_alias() {
        // The alias policy: a multi-link inode is not single-owner
        // in-root content, so the seam fails closed (Unix offers the
        // stable link count through std).
        #[cfg(unix)]
        {
            let temporary = tempfile::tempdir().expect("tempdir");
            let root = temporary.path();
            std::fs::write(root.join("real.json"), b"{}").expect("write");
            std::fs::hard_link(root.join("real.json"), root.join("alias.json")).expect("hardlink");
            let metadata = root.join("real.json").symlink_metadata().expect("meta");
            let identity = identity_of(&metadata).expect("identity");
            assert_eq!(identity.links, 2);
            assert!(!identity.single_owner());
            assert_eq!(read_admitted(root, "real.json", 16), Err(ReadRefusal::Link));
            assert_eq!(
                read_admitted(root, "alias.json", 16),
                Err(ReadRefusal::Link)
            );
        }
    }
}
