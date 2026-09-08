//! Exercise the shared directory capability, including missing children that
//! must not hide a linked ancestor. Windows uses owned junctions, Unix symlinks.
#![cfg(any(unix, windows))]

use lekalo_core::project_fs::{EntryType, Fs, FsErrorKind};
use std::fs;

#[test]
fn directory_operations_reject_linked_ancestors_before_missing_children() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let external = temp.path().join("external");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&external).unwrap();
    let link = root.join("link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&external, &link).unwrap();
    #[cfg(windows)]
    assert!(std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&external)
        .output()
        .unwrap()
        .status
        .success());
    let cap = Fs::open(&root).unwrap();
    assert_eq!(cap.entry_type("", "link").unwrap(), EntryType::Symlink);
    for logical in ["link", "link/missing", "link/missing/deeper"] {
        assert!(
            matches!(cap.entries(logical), Err(FsErrorKind::Io)),
            "{logical}"
        );
        assert!(
            matches!(cap.entry_type(logical, "missing"), Err(FsErrorKind::Io)),
            "{logical}"
        );
        assert!(
            matches!(
                cap.read_file_opt(logical, "missing", 16),
                Err(FsErrorKind::Io)
            ),
            "{logical}"
        );
    }
    fs::create_dir(root.join("ordinary")).unwrap();
    assert!(cap.entries("ordinary").unwrap().is_empty());
    assert!(matches!(
        cap.entries("ordinary/missing"),
        Err(FsErrorKind::NotFound)
    ));
}
