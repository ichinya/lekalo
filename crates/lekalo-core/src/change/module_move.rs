//! A physical module relocation preserves every source byte and semantic ID.
//! Its closed inventory is shared by preparation, the live transaction and
//! recovery; an extra file, changed byte, link or occupied destination refuses.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{pin_inventory_digest, receipt, ChangeError};
use crate::loader::ProjectLoad;
use crate::project_fs::{EntryType, Fs, FsErrorKind, StructureOutcome};

pub(crate) fn prepare_change(
    load: &ProjectLoad,
    request: &super::ChangeRequest,
    options: &super::PrepareOptions<'_>,
) -> Result<super::PreparedChange, ChangeError> {
    let super::Operation::ModuleMove { module, directory } = &request.operations[0] else {
        return Err(ChangeError::WriteBoundaryDenied);
    };
    let movement = ModuleMove::prepare(load, module, directory)?;
    let error_source = super::operation_error::read_source(&load.root)?;
    if let Some(bytes) = &error_source {
        let registry = crate::error_contract::ErrorRegistry::from_bytes(bytes)
            .map_err(|set| ChangeError::LoadFailed(crate::DomainResult::invalid(set)))?;
        let prefix = format!("lekalo/modules/{}/", movement.from);
        if registry
            .errors()
            .iter()
            .any(|error| error.source().path().starts_with(&prefix))
        {
            return Err(ChangeError::UnsupportedMember {
                operation: "module:move".to_owned(),
                member: "error-source-relocation",
            });
        }
    }
    let audit = super::plan::ResolvedOperation {
        operation: "module:move".to_owned(),
        subject: module.clone(),
        subject_family: "module".to_owned(),
        status: "resolved".to_owned(),
        detail: None,
    };
    let mut prepared = super::source_fix::prepare_source_only(
        load,
        request.to_canonical_json_string().into_bytes(),
        audit,
        options.policy,
        options.allow_breaking,
        vec![],
    )?;
    // The source-location guard and the sealed plan must observe the same
    // registry, including absence. A later read cannot adopt a new binding
    // whose source path this physical relocation never checked.
    let error_pin = super::operation_error::input_pin(error_source.as_deref());
    if prepared
        .inputs
        .iter()
        .find(|pin| pin.path == error_pin.path)
        .map(|pin| pin.digest.as_str())
        != Some(error_pin.digest.as_str())
    {
        return Err(ChangeError::PreconditionFailed {
            subject: "error-registry".to_owned(),
            detail: "registry-digest",
        });
    }
    if let Some(expected) = &request.expect_model {
        let current = pin_inventory_digest(&prepared.inputs);
        if expected != &current {
            return Err(ChangeError::PreconditionFailed {
                subject: "model".to_owned(),
                detail: "expect-model",
            });
        }
    }
    for file in &movement.files {
        let from = movement.before_path(&file.name);
        let to = movement.after_path(&file.name);
        let bytes = &load
            .documents
            .iter()
            .find(|document| document.document.path == from)
            .ok_or(ChangeError::WriteBoundaryDenied)?
            .bytes;
        let text = std::str::from_utf8(bytes).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let show_diff = bytes.len() <= super::MAX_FILE_DIFF_BYTES
            && text.lines().count() <= super::MAX_PATCH_LINES;
        if !show_diff {
            prepared.diff_complete = false;
        }
        for (path, before, after) in [(from, text, ""), (to, "", text)] {
            prepared.file_edits.push(super::plan::FileEdit {
                path: path.clone(),
                before_digest: receipt::bytes_digest(before.as_bytes()),
                after_digest: receipt::bytes_digest(after.as_bytes()),
            });
            if !show_diff {
                continue;
            }
            let (patch, complete) = super::plan::compute_patch(before, after);
            if !complete {
                prepared.diff_complete = false;
            }
            prepared.file_diffs.push(super::plan::FileDiff {
                path,
                before: before.to_owned(),
                after: after.to_owned(),
                patch,
            });
        }
    }
    prepared.file_edits.sort_by(|a, b| a.path.cmp(&b.path));
    prepared.file_diffs.sort_by(|a, b| a.path.cmp(&b.path));
    prepared.module_move = Some(movement);
    Ok(prepared)
}

const HOME: &str = "lekalo/modules";
const MAX_FILES: usize = 256;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FilePin {
    pub name: String,
    pub digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModuleMove {
    pub module: String,
    pub from: String,
    pub to: String,
    pub files: Vec<FilePin>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Position {
    Before,
    After,
}

pub(crate) fn directory_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
        && crate::project_fs::path_violation(&format!("{HOME}/{value}/module.yaml")).is_none()
}

fn file_valid(name: &str) -> bool {
    name == "module.yaml"
        || name
            .strip_suffix(".yaml")
            .is_some_and(|stem| crate::project_fs::KIND_STEMS.contains(&stem))
}

impl ModuleMove {
    pub(crate) fn prepare(
        load: &ProjectLoad,
        module: &str,
        directory: &str,
    ) -> Result<Self, ChangeError> {
        if !directory_valid(directory) {
            return Err(ChangeError::WriteBoundaryDenied);
        }
        let owner = load
            .documents
            .iter()
            .find(|document| {
                document.document.definitions.iter().any(|definition| {
                    definition.id == module && super::definition_kind_of(definition) == "module"
                })
            })
            .ok_or_else(|| ChangeError::SubjectConflict {
                subject: module.to_owned(),
                detail: "module-missing",
            })?;
        let from = owner
            .document
            .path
            .strip_prefix(&format!("{HOME}/"))
            .and_then(|rest| rest.strip_suffix("/module.yaml"))
            .filter(|leaf| directory_valid(leaf))
            .ok_or(ChangeError::WriteBoundaryDenied)?;
        if from == directory {
            return Err(ChangeError::SubjectConflict {
                subject: module.to_owned(),
                detail: "move-same-home",
            });
        }
        let prefix = format!("{HOME}/{from}/");
        let mut files = load
            .documents
            .iter()
            .filter_map(|document| {
                document
                    .document
                    .path
                    .strip_prefix(&prefix)
                    .map(|name| FilePin {
                        name: name.to_owned(),
                        digest: receipt::bytes_digest(&document.bytes),
                    })
            })
            .collect::<Vec<_>>();
        files.sort_by(|a, b| a.name.cmp(&b.name));
        let plan = Self {
            module: module.to_owned(),
            from: from.to_owned(),
            to: directory.to_owned(),
            files,
        };
        if plan.position(&load.root)? != Position::Before {
            return Err(plan.conflict("move-state"));
        }
        Ok(plan)
    }

    fn conflict(&self, detail: &'static str) -> ChangeError {
        ChangeError::PreconditionFailed {
            subject: self.module.clone(),
            detail,
        }
    }

    pub(crate) fn before_path(&self, file: &str) -> String {
        format!("{HOME}/{}/{file}", self.from)
    }
    pub(crate) fn after_path(&self, file: &str) -> String {
        format!("{HOME}/{}/{file}", self.to)
    }

    pub(crate) fn position(&self, root: &Path) -> Result<Position, ChangeError> {
        if !super::field_name_valid(&self.module)
            || !directory_valid(&self.from)
            || !directory_valid(&self.to)
            || self.from == self.to
            || self.files.is_empty()
            || self.files.len() > MAX_FILES
            || !self.files.iter().any(|file| file.name == "module.yaml")
            || self
                .files
                .iter()
                .any(|file| !file_valid(&file.name) || super::expect_digest(&file.digest).is_none())
            || self
                .files
                .windows(2)
                .any(|files| files[0].name >= files[1].name)
        {
            return Err(ChangeError::WriteBoundaryDenied);
        }
        let fs = Fs::open(root).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let entries = fs
            .entries(HOME)
            .map_err(|_| ChangeError::WriteBoundaryDenied)?;
        for (name, _) in &entries {
            if (name.eq_ignore_ascii_case(&self.from) && name != &self.from)
                || (name.eq_ignore_ascii_case(&self.to) && name != &self.to)
            {
                return Err(ChangeError::WriteBoundaryDenied);
            }
        }
        let from = fs.entry_type(HOME, &self.from);
        let to = fs.entry_type(HOME, &self.to);
        let (position, leaf) = match (from, to) {
            (Ok(EntryType::Directory), Err(FsErrorKind::NotFound)) => {
                (Position::Before, &self.from)
            }
            (Err(FsErrorKind::NotFound), Ok(EntryType::Directory)) => (Position::After, &self.to),
            _ => return Err(self.conflict("move-state")),
        };
        let modules = match Fs::validate_project(root) {
            StructureOutcome::Valid(report) => report.modules,
            _ => return Err(ChangeError::WriteBoundaryDenied),
        };
        if !modules.contains(leaf) {
            return Err(ChangeError::WriteBoundaryDenied);
        }
        let home = format!("{HOME}/{leaf}");
        let mut entries = fs
            .entries(&home)
            .map_err(|_| ChangeError::WriteBoundaryDenied)?;
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        if entries.len() != self.files.len()
            || entries
                .iter()
                .zip(&self.files)
                .any(|((name, kind), pin)| name != &pin.name || *kind != EntryType::File)
        {
            return Err(self.conflict("move-inventory"));
        }
        for file in &self.files {
            let bytes = fs
                .read_file_opt(&home, &file.name, crate::loader::source::MAX_DOCUMENT_BYTES)
                .ok()
                .flatten()
                .ok_or(ChangeError::WriteBoundaryDenied)?;
            if receipt::bytes_digest(&bytes) != file.digest {
                return Err(self.conflict("move-input-digest"));
            }
            if file.name == "module.yaml" {
                let document = crate::loader::decode_document_bytes(
                    &format!("{home}/module.yaml"),
                    crate::loader::DocKind::Module,
                    &bytes,
                )
                .map_err(|_| ChangeError::WriteBoundaryDenied)?;
                if !document.definitions.iter().any(|definition| {
                    definition.id == self.module
                        && super::definition_kind_of(definition) == "module"
                }) {
                    return Err(ChangeError::WriteBoundaryDenied);
                }
            }
        }
        Ok(position)
    }

    pub(crate) fn install(&self, root: &Path) -> Result<(), ChangeError> {
        if self.position(root)? != Position::Before {
            return Err(self.conflict("move-state"));
        }
        rename_confined(root, &self.from, &self.to).map_err(|_| self.conflict("move-rename"))?;
        if self.position(root)? != Position::After {
            return Err(self.conflict("move-postcondition"));
        }
        Ok(())
    }

    pub(crate) fn restore(&self, root: &Path) -> Result<(), ChangeError> {
        if self.position(root)? == Position::Before {
            return Ok(());
        }
        rename_confined(root, &self.to, &self.from).map_err(|_| self.conflict("move-restore"))?;
        if self.position(root)? != Position::Before {
            return Err(self.conflict("move-postcondition"));
        }
        Ok(())
    }
}

/// Keep the parent capability alive across the actual rename. Rechecking a
/// pathname alone would let a swapped parent redirect the write outside root.
#[cfg(unix)]
fn rename_confined(root: &Path, from: &str, to: &str) -> std::io::Result<()> {
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple",
        target_os = "redox"
    ))]
    {
        use rustix::fs::{openat, renameat_with, Mode, OFlags, RenameFlags, CWD};
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let root = openat(CWD, root, flags, Mode::empty())?;
        let lekalo = openat(&root, "lekalo", flags, Mode::empty())?;
        let modules = openat(&lekalo, "modules", flags, Mode::empty())?;
        renameat_with(&modules, from, &modules, to, RenameFlags::NOREPLACE)?;
        let parent = std::fs::File::from(modules);
        parent.sync_all()
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple",
        target_os = "redox"
    )))]
    {
        let _ = (root, from, to);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "atomic-module-move",
        ))
    }
}

#[cfg(windows)]
fn rename_confined(root: &Path, from: &str, to: &str) -> std::io::Result<()> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let mut parents = Vec::new();
    for path in [root.to_path_buf(), root.join("lekalo"), root.join(HOME)] {
        let parent = std::fs::OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        let metadata = parent.metadata()?;
        if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "module-parent-link",
            ));
        }
        parents.push(parent);
    }
    super::transaction::rename_no_replace(&root.join(HOME).join(from), &root.join(HOME).join(to))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::transaction::{self, ReplacementPlan, Transaction};

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

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/change/field-vertical"),
            root.path(),
        );
        root
    }

    fn replacement(path: &str) -> ReplacementPlan {
        let bytes = b"test transaction bytes".to_vec();
        ReplacementPlan {
            logical: path.to_owned(),
            before_digest: crate::change::absent_digest(),
            after_digest: receipt::bytes_digest(&bytes),
            after_bytes: bytes,
            creation: true,
        }
    }

    #[test]
    fn an_unrelated_error_registry_is_sealed_without_blocking_a_safe_move() {
        let root = tempfile::tempdir().unwrap();
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/model-v1/valid-planner"),
            root.path(),
        );
        let home = root.path().join("lekalo/modules/extra");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("module.yaml"),
            "schema_version: \"0.2.16\"\ndefinitions:\n  - id: extra\n    kind: module\n    version: 1\n").unwrap();
        let source = root
            .path()
            .join(crate::error_contract::source::PROJECT_PATH);
        let registry = crate::error_contract::REGISTRY_BYTES;
        std::fs::write(&source, registry).unwrap();
        let load = crate::loader::load_validated_root(root.path().to_path_buf()).unwrap();
        let bytes = br#"{"schemaVersion":"lekalo/change-request/v0.8.0","operations":[{"operation":"module:move","module":"extra","directory":"extra_home"}]}"#;
        let request = crate::change::ChangeRequest::decode(bytes).unwrap();
        let prepared = prepare_change(
            &load,
            &request,
            &crate::change::PrepareOptions {
                project: None,
                request: bytes,
                policy: None,
                allow_breaking: false,
            },
        )
        .unwrap();
        assert_eq!(
            prepared
                .inputs
                .iter()
                .find(|pin| pin.path == crate::error_contract::source::PROJECT_PATH)
                .unwrap()
                .digest,
            receipt::bytes_digest(registry)
        );
        let mut drift = registry.to_vec();
        drift.push(b'\n');
        std::fs::write(&source, &drift).unwrap();
        let options = crate::change::ApplyOptions {
            idempotency_key: "move-unrelated-errors",
            expect_plan: None,
            allow_breaking: false,
        };
        assert!(crate::change::apply_prepared(&prepared, &options).is_err());
        assert!(home.exists());
        assert!(!root.path().join("lekalo/modules/extra_home").exists());
        assert_eq!(std::fs::read(&source).unwrap(), drift);
        std::fs::write(&source, registry).unwrap();
        assert_eq!(
            crate::change::apply_prepared(&prepared, &options)
                .unwrap()
                .state(),
            crate::change::receipt::ReceiptState::Committed
        );
        assert!(!home.exists());
        assert!(root
            .path()
            .join("lekalo/modules/extra_home/module.yaml")
            .exists());
        assert_eq!(std::fs::read(source).unwrap(), registry);
    }

    #[test]
    fn interruption_at_each_move_publication_boundary_restores_exact_originals() {
        for phase in 0..=3 {
            let root = fixture();
            let load = crate::loader::load_validated_root(root.path().to_path_buf()).unwrap();
            let movement = ModuleMove::prepare(&load, "tasks", "new_home").unwrap();
            let plans = [
                replacement(crate::change::version::HISTORY_LOGICAL_PATH),
                replacement(crate::change::custody::LOGICAL_PATH),
            ];
            let mut tx = Transaction::begin_with_move(
                root.path(),
                "change-move-boundary",
                &format!("sha256:{}", "a".repeat(64)),
                &plans,
                Some(&movement),
            )
            .unwrap();
            for (index, plan) in plans.iter().enumerate() {
                tx.stage(index, plan).unwrap();
            }
            tx.mark_state(transaction::STATE_REPLACING).unwrap();
            if phase >= 1 {
                movement.install(root.path()).unwrap();
            }
            if phase >= 2 {
                std::fs::write(root.path().join(&plans[0].logical), &plans[0].after_bytes).unwrap();
            }
            if phase >= 3 {
                std::fs::write(root.path().join(&plans[1].logical), &plans[1].after_bytes).unwrap();
            }
            drop(tx); // model process death: no automatic rollback or cleanup
            assert!(
                !transaction::recover(root.path(), "change-move-boundary")
                    .unwrap()
                    .committed
            );
            assert_eq!(movement.position(root.path()).unwrap(), Position::Before);
            for document in &load.documents {
                assert_eq!(
                    std::fs::read(root.path().join(&document.document.path)).unwrap(),
                    document.bytes
                );
            }
            for plan in &plans {
                assert!(!root.path().join(&plan.logical).exists());
            }
            crate::loader::load_validated_root(root.path().to_path_buf()).unwrap();
        }
    }

    #[test]
    fn changed_move_input_blocks_all_recovery_writes_until_explicit_repair() {
        let root = fixture();
        let load = crate::loader::load_validated_root(root.path().to_path_buf()).unwrap();
        let movement = ModuleMove::prepare(&load, "tasks", "new_home").unwrap();
        let plans = [
            replacement(crate::change::version::HISTORY_LOGICAL_PATH),
            replacement(crate::change::custody::LOGICAL_PATH),
        ];
        let mut tx = Transaction::begin_with_move(
            root.path(),
            "change-move-drift",
            &format!("sha256:{}", "a".repeat(64)),
            &plans,
            Some(&movement),
        )
        .unwrap();
        for (index, plan) in plans.iter().enumerate() {
            tx.stage(index, plan).unwrap();
        }
        tx.mark_state(transaction::STATE_REPLACING).unwrap();
        movement.install(root.path()).unwrap();
        std::fs::write(root.path().join(&plans[0].logical), &plans[0].after_bytes).unwrap();
        let changed = root.path().join(movement.after_path("entities.yaml"));
        let original = std::fs::read(&changed).unwrap();
        let mut user_bytes = original.clone();
        user_bytes.extend_from_slice(b"# retained user edit\n");
        std::fs::write(&changed, &user_bytes).unwrap();
        assert!(transaction::recover(root.path(), "change-move-drift").is_err());
        assert_eq!(std::fs::read(&changed).unwrap(), user_bytes);
        assert_eq!(
            std::fs::read(root.path().join(&plans[0].logical)).unwrap(),
            plans[0].after_bytes
        );
        assert!(!root.path().join("lekalo/modules/tasks").exists());
        std::fs::write(&changed, original).unwrap();
        assert!(
            !transaction::recover(root.path(), "change-move-drift")
                .unwrap()
                .committed
        );
        assert_eq!(movement.position(root.path()).unwrap(), Position::Before);
    }

    #[test]
    fn failed_post_validation_with_user_drift_retains_an_actionable_recovery_barrier() {
        let root = fixture();
        let load = crate::loader::load_validated_root(root.path().to_path_buf()).unwrap();
        let bytes = br#"{"schemaVersion":"lekalo/change-request/v0.8.0","operations":[{"operation":"module:move","module":"tasks","directory":"new_home"}]}"#;
        let request = crate::change::ChangeRequest::decode(bytes).unwrap();
        let prepared = prepare_change(
            &load,
            &request,
            &crate::change::PrepareOptions {
                project: None,
                request: bytes,
                policy: None,
                allow_breaking: false,
            },
        )
        .unwrap();
        let changed = root.path().join("lekalo/modules/new_home/entities.yaml");
        let original =
            std::fs::read(root.path().join("lekalo/modules/tasks/entities.yaml")).unwrap();
        let mut user_bytes = original.clone();
        user_bytes.extend_from_slice(b"# retained user edit\n");
        let error = crate::change::apply_with_validation(
            &prepared,
            &crate::change::ApplyOptions {
                idempotency_key: "post-validation-drift",
                expect_plan: None,
                allow_breaking: false,
            },
            &|_| {
                std::fs::write(&changed, &user_bytes).unwrap();
                Err(ChangeError::WriteBoundaryDenied)
            },
        )
        .unwrap_err();
        let ChangeError::RecoveryRequired { transaction } = error else {
            panic!("an incomplete rollback must identify the pending transaction: {error:?}");
        };
        assert_eq!(std::fs::read(&changed).unwrap(), user_bytes);
        assert!(transaction::recover(root.path(), &transaction).is_err());
        std::fs::write(&changed, original).unwrap();
        assert!(
            !transaction::recover(root.path(), &transaction)
                .unwrap()
                .committed
        );
        assert!(root
            .path()
            .join("lekalo/modules/tasks/entities.yaml")
            .exists());
        assert!(!root.path().join("lekalo/modules/new_home").exists());
    }

    #[test]
    fn occupied_destination_and_hostile_journal_coordinates_never_move_files() {
        let root = fixture();
        let load = crate::loader::load_validated_root(root.path().to_path_buf()).unwrap();
        let movement = ModuleMove::prepare(&load, "tasks", "new_home").unwrap();
        std::fs::create_dir(root.path().join("lekalo/modules/new_home")).unwrap();
        assert!(movement.install(root.path()).is_err());
        std::fs::remove_dir(root.path().join("lekalo/modules/new_home")).unwrap();
        for coordinate in [
            "../escape",
            "/outside",
            "C:/outside",
            "tasks/other",
            "tasks\\other",
            "CON",
        ] {
            let mut hostile = movement.clone();
            hostile.to = coordinate.to_owned();
            assert!(hostile.install(root.path()).is_err());
            assert!(root
                .path()
                .join("lekalo/modules/tasks/module.yaml")
                .exists());
        }
    }
}
