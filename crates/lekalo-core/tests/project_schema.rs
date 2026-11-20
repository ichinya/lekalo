use std::fs;
use std::path::Path;

use lekalo_core::lockfile::Sha256Digest;
use lekalo_core::project_schema::{Registry, RegistryError};
use serde_json::json;
use tempfile::tempdir;

fn write(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join("lekalo/contracts").join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn validators_apply_exact_versions_and_pins_to_documents() {
    let root = tempdir().unwrap();
    let path = "action/1.0.0.schema.json";
    write(
        root.path(),
        path,
        br##"{"$defs":{"action":{"type":"string"}},"$ref":"#/$defs/action"}"##,
    );
    write(root.path(), "action/2.0.0.schema.json", b"false");

    let registry = Registry::load(root.path()).unwrap();
    let pins: Vec<_> = registry.pins().collect();
    assert_eq!(pins.len(), 2);
    let (first, first_digest) = &pins[0];
    let (second, second_digest) = &pins[1];
    assert_eq!(first.id().as_str(), "action");
    assert_eq!(first.version().as_str(), "1.0.0");
    assert_eq!(second.version().as_str(), "2.0.0");
    assert!(registry
        .validator(first, first_digest)
        .unwrap()
        .is_valid(&json!("read")));
    assert!(!registry
        .validator(first, first_digest)
        .unwrap()
        .is_valid(&json!(42)));
    assert!(!registry
        .validator(second, second_digest)
        .unwrap()
        .is_valid(&json!("read")));

    let digest: Sha256Digest = (*first_digest).clone();
    write(
        root.path(),
        path,
        "{\r\n\"$defs\":{\"action\":{\"type\":\"string\"}},\"$ref\":\"#/$defs/action\"}\r\n",
    );
    let changed = Registry::load(root.path()).unwrap();
    assert_eq!(
        changed.validator(first, &digest).unwrap_err(),
        RegistryError::DigestMismatch
    );
}

#[test]
fn admission_refuses_external_sources_conflicts_and_unbounded_inputs() {
    for (body, expected) in [
        (
            r#"{"$ref":"https://example.invalid/schema"}"#,
            RegistryError::Reference,
        ),
        (
            r#"{"$ref":"../another.schema.json"}"#,
            RegistryError::Reference,
        ),
        (
            r#"{"properties":{"name":{"$id":"dev.lekalo.lock@0.3.2"}}}"#,
            RegistryError::Identifier,
        ),
        (
            r#"{"$schema":"https://json-schema.org/draft/2019-09/schema"}"#,
            RegistryError::Dialect,
        ),
        (
            r#"{"type":"string","type":"number"}"#,
            RegistryError::Invalid,
        ),
        ("type: string", RegistryError::Invalid),
    ] {
        let root = tempdir().unwrap();
        write(root.path(), "action/1.0.0.schema.json", body);
        assert_eq!(Registry::load(root.path()).unwrap_err(), expected);
    }
    let root = tempdir().unwrap();
    let path = "action/1.0.0.schema.json";
    write(root.path(), path, " ".repeat(64 * 1024 + 1));
    assert_eq!(
        Registry::load(root.path()).unwrap_err(),
        RegistryError::Limit
    );
    let nested = format!("{{\"const\":{}false{}}}", "[".repeat(70), "]".repeat(70));
    write(root.path(), path, nested);
    assert_eq!(
        Registry::load(root.path()).unwrap_err(),
        RegistryError::Limit
    );
    for patch in 1..=32 {
        write(
            root.path(),
            &format!("action/1.0.{patch}.schema.json"),
            b"true",
        );
    }
    assert_eq!(
        Registry::load(root.path()).unwrap_err(),
        RegistryError::Limit
    );
}

#[test]
fn registry_paths_are_portable_and_single_owner() {
    let root = tempdir().unwrap();
    assert_eq!(Registry::load(root.path()).unwrap().pins().count(), 0);
    write(root.path(), "Action/1.0.0.schema.json", b"true");
    assert_eq!(
        Registry::load(root.path()).unwrap_err(),
        RegistryError::Path
    );
    fs::remove_dir_all(root.path().join("lekalo/contracts/Action")).unwrap();

    let outside = tempdir().unwrap();
    let external = outside.path().join("schema.json");
    fs::write(&external, b"true").unwrap();
    let directory = root.path().join("lekalo/contracts/action");
    fs::create_dir_all(&directory).unwrap();
    fs::hard_link(external, directory.join("1.0.0.schema.json")).unwrap();
    assert_eq!(
        Registry::load(root.path()).unwrap_err(),
        RegistryError::Path
    );
}

#[cfg(unix)]
#[test]
fn schemas_refuse_directory_and_file_symlinks() {
    use std::os::unix::fs::symlink;

    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let directory = root.path().join("lekalo/contracts");
    fs::create_dir_all(directory.parent().unwrap()).unwrap();
    symlink(outside.path(), &directory).unwrap();
    assert_eq!(
        Registry::load(root.path()).unwrap_err(),
        RegistryError::Path
    );

    fs::remove_file(&directory).unwrap();
    let directory = directory.join("action");
    fs::create_dir_all(&directory).unwrap();
    let external = outside.path().join("schema.json");
    fs::write(&external, b"true").unwrap();
    symlink(external, directory.join("1.0.0.schema.json")).unwrap();
    assert_eq!(
        Registry::load(root.path()).unwrap_err(),
        RegistryError::Path
    );
}
