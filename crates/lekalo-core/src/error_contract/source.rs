//! Optional canonical project registry. The embedded registry remains the
//! legacy default; a present project declaration is always parsed and bound
//! to the current compiled Model, never silently ignored or replaced.

use std::path::Path;

use crate::diagnostics::DiagnosticSet;
use crate::project_fs::{Fs, FsErrorKind};

use super::{diagnostic, ErrorRegistry};

/// The fixed declaration home; callers never supply a registry write path.
pub const PROJECT_PATH: &str = "lekalo/error-registry.json";

/// Read the exact confined source bytes. Absence is distinct from an empty
/// or unreadable declaration.
pub fn read_bytes(root: &Path) -> Result<Option<Vec<u8>>, DiagnosticSet> {
    let fs = Fs::open(root).map_err(|_| diagnostic::registry_invalid_set("project-root"))?;
    match fs.read_file_opt(
        "lekalo",
        "error-registry.json",
        super::version::MAX_EXPORT_BYTES,
    ) {
        Ok(bytes) => Ok(bytes),
        Err(FsErrorKind::NotFound) => Ok(None),
        Err(_) => Err(diagnostic::registry_invalid_set(
            "project-registry-unreadable",
        )),
    }
}

/// Load a present project declaration with the existing canonical decoder.
pub fn read(root: &Path) -> Result<Option<ErrorRegistry>, DiagnosticSet> {
    read_bytes(root)?
        .map(|bytes| ErrorRegistry::from_bytes(&bytes))
        .transpose()
}

/// Bind a present declaration to this exact Model. With no local source the
/// legacy embedded default keeps its existing behavior.
pub fn for_project(
    root: &Path,
    project: &crate::ir::CompiledProject,
) -> Result<ErrorRegistry, DiagnosticSet> {
    match read(root)? {
        Some(registry) => {
            validate(&registry, project)?;
            Ok(registry)
        }
        None => ErrorRegistry::embedded().cloned(),
    }
}

/// Validate a local registry without changing the owner's diagnostic set.
pub fn validate(
    registry: &ErrorRegistry,
    project: &crate::ir::CompiledProject,
) -> Result<(), DiagnosticSet> {
    let findings = super::validate(registry, project);
    if findings.as_slice().is_empty() {
        Ok(())
    } else {
        Err(findings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &to.join(entry.file_name()));
            } else {
                std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
            }
        }
    }

    #[test]
    fn a_present_local_registry_is_bound_and_cannot_fall_back_on_failure() {
        let root = tempfile::tempdir().unwrap();
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/model-v1/valid-planner"),
            root.path(),
        );
        let load = crate::loader::load_validated_root(root.path().to_path_buf()).unwrap();
        let project = crate::ir::compile(&load.model).unwrap().project;
        assert!(read(root.path()).unwrap().is_none());
        assert_eq!(
            for_project(root.path(), &project)
                .unwrap()
                .canonical_bytes()
                .as_bytes(),
            super::super::registry::REGISTRY_BYTES
        );
        let path = root.path().join(PROJECT_PATH);
        std::fs::write(&path, super::super::registry::REGISTRY_BYTES).unwrap();
        assert!(for_project(root.path(), &project).is_ok());
        for bytes in [
            b"".as_slice(),
            b"{}".as_slice(),
            b"not a registry".as_slice(),
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert!(for_project(root.path(), &project).is_err());
        }
        // The frozen command signature is unit, even though the generic
        // wire output grammar can also represent a query's value type.
        let mut wire: serde_json::Value =
            serde_json::from_slice(super::super::registry::REGISTRY_BYTES).unwrap();
        let binding = wire["bindings"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row["operation"] == "planner.focus_task")
            .unwrap();
        binding["output"] = serde_json::json!({"ref":"planner.task"});
        let bytes = serde_json::to_vec(&wire).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let findings = for_project(root.path(), &project).unwrap_err();
        assert_eq!(findings.as_slice().len(), 1);
        assert_eq!(findings.as_slice()[0].id(), "error.binding-invalid");
        assert_eq!(
            findings.as_slice()[0].data().get("detail"),
            Some(&crate::diagnostics::DataValue::Token(
                "output-mismatch".to_owned()
            ))
        );
    }
}
