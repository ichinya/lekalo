//! Confined immutable activation, using directory capabilities on Unix and
//! directory handles denying rename/delete while writing on Windows.
use super::{Error, Result};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

pub(crate) fn read(project: &Path, relative: &str) -> Result<Vec<u8>> {
    if !portable(relative) {
        return Err(Error::Storage);
    }
    let p = Path::new(relative);
    let dir = p
        .parent()
        .and_then(Path::to_str)
        .filter(|v| !v.is_empty())
        .unwrap_or(".");
    let name = p
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or(Error::Storage)?;
    let fs = crate::project_fs::Fs::open(project).map_err(|_| Error::Storage)?;
    fs.read_file_opt(dir, name, 1_048_576)
        .map_err(|_| Error::Storage)?
        .ok_or(Error::Storage)
}

pub(crate) fn write(project: &Path, relative: &str, bytes: &[u8]) -> Result<()> {
    if !relative.starts_with(".lekalo/privacy/") || !portable(relative) {
        return Err(Error::Storage);
    }
    platform_write(project, relative, bytes)
}

fn portable(relative: &str) -> bool {
    // The loader grammar excludes hidden model-input directories. This owner
    // admits only its fixed private custody home, retaining the same grammar
    // for every caller-controlled or content-addressed suffix.
    relative == ".lekalo/privacy/.gitignore"
        || crate::project_fs::path_violation(
            relative
                .strip_prefix(".lekalo/privacy/")
                .unwrap_or(relative),
        )
        .is_none()
}

pub(crate) fn verify_untracked(project: &Path) -> Result<()> {
    if !project.join(".git").exists() {
        return Ok(());
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["ls-files", "--", ".lekalo/privacy"])
        .output()
        .map_err(|_| Error::Storage)?;
    if !output.status.success() || !output.stdout.trim_ascii().is_empty() {
        return Err(Error::Storage);
    }
    Ok(())
}

fn contents(mut file: File, bytes: &[u8], new: bool) -> Result<()> {
    let metadata = file.metadata().map_err(|_| Error::Storage)?;
    if !metadata.is_file() {
        return Err(Error::Storage);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(Error::Storage);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if metadata.file_attributes() & 0x400 != 0
            || unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0
            || info.nNumberOfLinks != 1
        {
            return Err(Error::Storage);
        }
    }
    if new {
        file.write_all(bytes).map_err(|_| Error::Storage)?;
        file.sync_all().map_err(|_| Error::Storage)
    } else {
        let mut existing = Vec::new();
        file.take(1_048_577)
            .read_to_end(&mut existing)
            .map_err(|_| Error::Storage)?;
        if existing != bytes {
            Err(Error::Storage)
        } else {
            Ok(())
        }
    }
}

#[cfg(unix)]
fn platform_write(project: &Path, relative: &str, bytes: &[u8]) -> Result<()> {
    use rustix::fs::{fsync, mkdirat, openat, Mode, OFlags, CWD};
    let dirs = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut parent = openat(CWD, project, dirs, Mode::empty()).map_err(|_| Error::Storage)?;
    let parts: Vec<_> = relative.split('/').collect();
    for component in &parts[..parts.len() - 1] {
        match mkdirat(&parent, *component, Mode::from_raw_mode(0o700)) {
            Ok(()) => fsync(&parent).map_err(|_| Error::Storage)?,
            Err(rustix::io::Errno::EXIST) => (),
            Err(_) => return Err(Error::Storage),
        }
        parent = openat(&parent, *component, dirs, Mode::empty()).map_err(|_| Error::Storage)?;
    }
    let name = *parts.last().ok_or(Error::Storage)?;
    let create =
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let (file, new) = match openat(&parent, name, create, Mode::from_raw_mode(0o600)) {
        Ok(fd) => (File::from(fd), true),
        Err(rustix::io::Errno::EXIST) => (
            File::from(
                openat(
                    &parent,
                    name,
                    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|_| Error::Storage)?,
            ),
            false,
        ),
        Err(_) => return Err(Error::Storage),
    };
    contents(file, bytes, new)?;
    fsync(&parent).map_err(|_| Error::Storage)
}

#[cfg(windows)]
fn platform_write(project: &Path, relative: &str, bytes: &[u8]) -> Result<()> {
    use std::fs::{self, OpenOptions};
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let mut pins = Vec::new();
    let pin = |path: &Path| -> Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|_| Error::Storage)?;
        let meta = file.metadata().map_err(|_| Error::Storage)?;
        if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
            return Err(Error::Storage);
        }
        Ok(file)
    };
    let mut current = project.to_path_buf();
    pins.push(pin(&current)?);
    let parts: Vec<_> = relative.split('/').collect();
    for component in &parts[..parts.len() - 1] {
        current.push(component);
        match fs::create_dir(&current) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(_) => return Err(Error::Storage),
        }
        pins.push(pin(&current)?);
    }
    current.push(parts.last().ok_or(Error::Storage)?);
    let (file, new) = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&current)
    {
        Ok(file) => (file, true),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (
            OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&current)
                .map_err(|_| Error::Storage)?,
            false,
        ),
        Err(_) => return Err(Error::Storage),
    };
    contents(file, bytes, new)
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Value> {
    crate::run_history::validate::parse_no_duplicate_keys(bytes).map_err(|_| Error::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_hardlinked_existing_output_is_refused_even_with_matching_bytes() {
        let root = tempfile::tempdir().unwrap();
        let relative = ".lekalo/privacy/aggregates/payload.json";
        write(root.path(), relative, b"safe\n").unwrap();
        std::fs::hard_link(root.path().join(relative), root.path().join("outside.json")).unwrap();
        assert!(write(root.path(), relative, b"safe\n").is_err());
    }
    #[test]
    fn a_linked_home_cannot_receive_export_bytes() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".lekalo")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), root.path().join(".lekalo/privacy")).unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let status = std::process::Command::new("cmd")
                .creation_flags(0x08000000)
                .args(["/c", "mklink", "/J"])
                .arg(root.path().join(".lekalo").join("privacy"))
                .arg(outside.path())
                .output()
                .unwrap();
            assert!(
                status.status.success(),
                "junction creation prerequisite: {}",
                String::from_utf8_lossy(&status.stderr)
            );
        }
        assert!(write(root.path(), ".lekalo/privacy/payload.json", b"safe\n").is_err());
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}
