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
        for max in [0, 1, 16] {
            assert!(
                matches!(cap.entries_bounded(logical, max), Err(FsErrorKind::Io)),
                "bounded enumeration must validate {logical} before applying limit {max}"
            );
        }
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
    assert!(cap.entries_bounded("ordinary", 0).unwrap().is_empty());
    fs::write(root.join("ordinary/one"), b"one").unwrap();
    assert!(matches!(
        cap.entries_bounded("ordinary", 0),
        Err(FsErrorKind::Limit { max: 0 })
    ));
    assert_eq!(cap.entries_bounded("ordinary", 1).unwrap().len(), 1);
    fs::write(root.join("ordinary/two"), b"two").unwrap();
    assert!(matches!(
        cap.entries_bounded("ordinary", 1),
        Err(FsErrorKind::Limit { max: 1 })
    ));
    assert!(matches!(
        cap.entries("ordinary/missing"),
        Err(FsErrorKind::NotFound)
    ));
}

/// Regression (fix round 4, devin N-1): the governed adapter store is a
/// legal runtime home. A project with a fully installed adapter —
/// packages custody, inventory, evidence receipts, and the quarantined
/// record tree — validates clean, and an unknown `.lekalo/adapters`
/// child still refuses.
#[test]
fn an_installed_adapter_store_validates_as_a_runtime_home() {
    use lekalo_core::project_fs::{Fs, StructureOutcome};

    let root = std::env::temp_dir().join(format!("lekalo-struct-adapters-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // The project marker find_root anchors on.
    std::fs::create_dir_all(root.join("lekalo")).expect("lekalo dir");
    // macOS temp dirs live under the /var -> /private/var symlink; the
    // selection policy refuses alias spellings, so validate the resolved
    // spelling (the Windows 8.3 short-name alias resolves the same way,
    // and canonicalize's \\?\ verbatim prefix is stripped per convention).
    let root = root.canonicalize().expect("canonical temp root");
    #[cfg(windows)]
    let root = match root.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => std::path::PathBuf::from(rest),
        _ => root,
    };
    std::fs::write(
        root.join("lekalo/project.yaml"),
        "project: adapters-structure\n",
    )
    .expect("project marker");
    // The full installed-store spelling, exactly as the custody code
    // writes it.
    for dir in [
        ".lekalo/adapters/packages/a/1.0.0-277089d9",
        ".lekalo/adapters/quarantine/b/0.3.2-1e65cdbc",
        ".lekalo/adapters/staging",
        ".lekalo/adapters/evidence/installs/a",
    ] {
        std::fs::create_dir_all(root.join(dir.replace('/', std::path::MAIN_SEPARATOR_STR)))
            .expect("custody dir");
    }
    std::fs::write(
        root.join(".lekalo/adapters/packages/a/1.0.0-277089d9/adapter.mjs"),
        b"export const g = true;\n",
    )
    .expect("staged bytes");
    std::fs::write(
        root.join(".lekalo/adapters/inventory.json"),
        br#"{"schemaVersion":"lekalo/adapter-inventory/v0.3.2","identity":"dev.lekalo.adapter-inventory@0.3.2","packages":[]}"#,
    )
    .expect("inventory");
    std::fs::write(
        root.join(".lekalo/adapters/evidence/revocations.json"),
        br#"{"schemaVersion":"lekalo/adapter-revocations/v0.3.2","records":[]}"#,
    )
    .expect("revocations");
    std::fs::write(
        root.join(".lekalo/adapters/evidence/installs/a/receipt.json"),
        b"{}",
    )
    .expect("receipt");

    let outcome = Fs::validate_project(&root);
    assert!(
        matches!(outcome, StructureOutcome::Valid(_)),
        "an installed adapter store must validate: {outcome:?}"
    );

    // An unknown child of the adapters home still refuses.
    let stray = format!("{}", root.join(".lekalo/adapters/other").display());
    std::fs::create_dir_all(stray.replace('/', std::path::MAIN_SEPARATOR_STR)).expect("stray dir");
    let outcome = Fs::validate_project(&root);
    match outcome {
        StructureOutcome::Denied(reasons) | StructureOutcome::Invalid(reasons) => assert!(
            reasons
                .iter()
                .any(|reason| reason.code == "structure.runtime-unexpected-entry"),
            "the stray adapters child must refuse: {reasons:?}"
        ),
        StructureOutcome::Valid(_) => panic!("the stray adapters child must refuse"),
    }
    let _ = std::fs::remove_dir_all(&root);
}
