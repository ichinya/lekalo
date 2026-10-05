//! Bounded repository-owned configuration IO; never another owner's home.
use super::{canonical, failure, validate_store, Store};
use crate::{ai_lint::input, project_fs::Fs, DomainResult};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn safe_path(root: &Path, logical: &str) -> Result<PathBuf, DomainResult> {
    Fs::open(root).map_err(|_| failure("waivers-store-root"))?;
    if !matches!(logical, "lekalo.waivers.json" | "waivers-update.guard.json") {
        return Err(failure("waivers-store-home"));
    }
    let path = root.join(logical);
    if let Ok(m) = fs::symlink_metadata(&path) {
        if !m.is_file() || m.file_type().is_symlink() {
            return Err(failure("waivers-store-physical"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes() & 0x400 != 0 {
                return Err(failure("waivers-store-physical"));
            }
            use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
            use windows_sys::Win32::Storage::FileSystem::{
                GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
            };
            let file = OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x00200000)
                .open(&path)
                .map_err(|_| failure("waivers-store-physical"))?;
            let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) } == 0
                || info.nNumberOfLinks != 1
            {
                return Err(failure("waivers-store-physical"));
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if m.nlink() != 1 {
                return Err(failure("waivers-store-physical"));
            }
        }
    }
    Ok(path)
}
pub fn read(root: &Path, logical: &str) -> Result<Option<Vec<u8>>, DomainResult> {
    if logical != "lekalo.waivers.json" {
        return Err(failure("waivers-store-home"));
    }
    let path = safe_path(root, logical)?;
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(failure("waivers-store-read")),
        Ok(_) => {}
    }
    Fs::open(root)
        .and_then(|f| f.read_file_opt("", logical, input::MAX_BYTES))
        .map_err(|_| failure("waivers-store-read"))
}
fn guard(root: &Path) -> Result<File, DomainResult> {
    let path = safe_path(root, "waivers-update.guard.json")?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0).custom_flags(0x00200000);
    }
    let file = options
        .open(path)
        .map_err(|_| failure("waivers-update-in-progress"))?;
    #[cfg(unix)]
    {
        rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
            .map_err(|_| failure("waivers-update-in-progress"))?;
    }
    Ok(file)
}
#[cfg(test)]
pub(super) fn test_guard(root: &Path) -> Result<File, DomainResult> {
    guard(root)
}
pub fn plan_id(before: Option<&[u8]>, candidate: &Store) -> String {
    input::hash(
        &serde_json::json!({"before":before.map(input::digest),"after":input::hash(candidate)}),
    )
}
pub fn apply(
    root: &Path,
    logical: &str,
    before: Option<&[u8]>,
    candidate: &Store,
    expected_plan: &str,
) -> Result<(), DomainResult> {
    if logical != "lekalo.waivers.json" {
        return Err(failure("waivers-store-home"));
    }
    validate_store(candidate)?;
    let path = safe_path(root, logical)?;
    if !input::is_digest(expected_plan) || plan_id(before, candidate) != expected_plan {
        return Err(failure("waivers-plan-changed"));
    }
    let _guard = guard(root)?;
    if read(root, logical)?.as_deref() != before {
        return Err(failure("waivers-source-changed"));
    }
    let bytes = canonical(candidate);
    let mut stage = tempfile::NamedTempFile::new_in(root).map_err(|_| failure("waivers-stage"))?;
    stage
        .write_all(&bytes)
        .and_then(|()| stage.as_file().sync_all())
        .map_err(|_| failure("waivers-stage"))?;
    safe_path(root, logical)?;
    if read(root, logical)?.as_deref() != before {
        return Err(failure("waivers-source-changed"));
    }
    stage
        .persist(path)
        .map_err(|_| failure("waivers-replace"))?;
    if read(root, logical)?.as_deref() != Some(bytes.as_slice()) {
        return Err(failure("waivers-recovery-required"));
    }
    Ok(())
}
