//! The policy-aware read seam for the MCP server (issue #33).
//!
//! Every admitted open runs through this seam: the pure policy refuses
//! first, then the bounded no-follow reader opens exactly the granted
//! bytes. Trace manifests admitted through the catalog replace the
//! CLI's unrestricted `std::fs::read` path. Cache/runtime writes, adapter
//! execution, and repository scans are unreachable from here.

use std::io::Read;
use std::path::Path;

use lekalo_core::mcp::policy;
use lekalo_core::mcp::version::MAX_FRAME_BYTES;

/// Why one authorized read failed. Values are never echoed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadRefusal {
    /// The input is not admitted by the catalog or the pure policy.
    NotAdmitted,
    /// The open or read failed (missing, unreadable, over the bound).
    Unreadable,
    /// The path is a symlink, junction, reparse point, or hardlink alias
    /// under the owner policy.
    Link,
}

/// The bounded no-follow read of one admitted project-relative file.
///
/// `admit_relative_path` refuses first; the open then checks the final
/// component for symlink/junction/reparse-point aliases before reading at
/// most `cap` bytes. This is the same finite read set the pure policy
/// proved, enforced at the open boundary.
pub fn read_admitted(root: &Path, logical: &str, cap: usize) -> Result<Vec<u8>, ReadRefusal> {
    if policy::admit_relative_path(logical).is_err() {
        return Err(ReadRefusal::NotAdmitted);
    }
    let path = root.join(logical);
    let metadata = path
        .symlink_metadata()
        .map_err(|_| ReadRefusal::Unreadable)?;
    if metadata.file_type().is_symlink() {
        return Err(ReadRefusal::Link);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT covers junctions and mount points.
        const REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & REPARSE_POINT != 0 {
            return Err(ReadRefusal::Link);
        }
    }
    let file = std::fs::File::open(&path).map_err(|_| ReadRefusal::Unreadable)?;
    let mut bytes = Vec::new();
    file.take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ReadRefusal::Unreadable)?;
    if bytes.len() > cap {
        return Err(ReadRefusal::Unreadable);
    }
    Ok(bytes)
}

/// The bounded read cap for one admitted trace manifest input.
pub const MANIFEST_READ_CAP: usize = 4 * 1024 * 1024;

const _: () = assert!(MANIFEST_READ_CAP >= MAX_FRAME_BYTES);

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
}
