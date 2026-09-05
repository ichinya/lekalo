//! The lock service: deterministic preview, compare-and-swap apply, and
//! the process-death-released runtime guard (issue #10).
//!
//! [`LockService::plan`] resolves against the sealed candidates, diffs the
//! existing lock, and pins the exact before state, canonical after bytes,
//! and deterministic plan identity into a [`PreparedLockUpdate`] that no
//! caller can forge (private fields, not `Deserialize`, consumed by
//! `apply`). `planId` is SHA-256 over the before-or-absent marker, the
//! after digest, the request digest, the ordered catalog digests, and the
//! resolver version — repeated identical inputs produce byte-identical
//! plans. `apply` recomputes and requires the exact plan, holds an
//! OS-owned guard under `.lekalo/cache/locks` that the kernel releases on
//! process death, stages create-new in the lock's own directory, verifies
//! every byte, renames atomically, and re-reads the committed file. A
//! plain mutating update without a bound preview is refused
//! (`lock.preview-required`); the lock file is the only file this service
//! ever writes.

use std::path::{Path, PathBuf};

use super::parse::MAX_LOCKFILE_BYTES;
use super::resolution::{CandidateSet, LockResolver, ResolutionRequest};
use super::types::{LockDigest, Lockfile, SemVer, Sha256Digest};
use super::verify::{LockRequirement, LockVerifier, RuntimeInventory};
use super::{
    canonical, ChangeEntry, LockFailure, LockReceipt, LockState, UpdateReceipt, VersionDigest,
};
use crate::loader::LoadSelection;
use crate::versioning::registry::VersionRegistry;

/// The single-file concurrency guard under the runtime cache; distinct
/// from the `#9` migration lock and never committed.
pub(crate) const GUARD: &str = ".lekalo/cache/locks/lockfile-update";

/// The staged sibling used for the atomic create/replace.
pub(crate) const STAGE: &str = "lekalo.lock.lekalo-new";

/// Why the embedded registry was refused is a developer fault; surface the
/// accepted `versioning.registry-invalid` envelope verbatim.
fn registry_failure() -> LockFailure {
    LockFailure::Loader(crate::loader::diagnostic::failure(
        crate::result::Status::Invalid,
        vec![crate::loader::error::Diagnostic::new(
            crate::versioning::reasons::REGISTRY_INVALID,
        )],
    ))
}

fn embedded_registry() -> Result<&'static VersionRegistry, LockFailure> {
    VersionRegistry::embedded().map_err(|_| registry_failure())
}

/// The immutable prepared update a caller can apply exactly once.
///
/// Fields are private and the type is not `Deserialize`: only the planner
/// produces it, and [`LockService::apply`] consumes it, so no caller can
/// replay or forge a plan the service did not compute.
pub struct PreparedLockUpdate {
    root: PathBuf,
    before: Option<Sha256Digest>,
    after_bytes: Vec<u8>,
    after_digest: LockDigest,
    plan_id: String,
    resolver_version: String,
    changes: Vec<ChangeEntry>,
    changed: bool,
}

impl PreparedLockUpdate {
    /// The deterministic plan identity.
    pub fn plan_id(&self) -> &str {
        &self.plan_id
    }

    /// Whether applying this plan changes any bytes.
    pub fn changed(&self) -> bool {
        self.changed
    }
}

/// The lock entry points used by the `#10` CLI and reusable by #91.
pub struct LockService;

impl LockService {
    /// Load the project through the accepted pipeline and build the base
    /// contract-only request pinned to its Model version and this
    /// workspace's product version.
    pub fn load_request(selection: &LoadSelection) -> Result<ResolutionRequest, LockFailure> {
        let registry = embedded_registry()?;
        let model = crate::loader::normalize_model(selection).map_err(LockFailure::Loader)?;
        Ok(request_for_model(registry, model.model_version))
    }

    /// `lekalo lock`: create a missing lock, or check an existing one and
    /// never update it. `create_if_absent` is cleared for `lock --check`,
    /// which is the headless CI gate.
    pub fn lock(
        selection: &LoadSelection,
        candidates: CandidateSet,
        requirement: LockRequirement,
        create_if_absent: bool,
    ) -> Result<LockReceipt, LockFailure> {
        let root = crate::loader::root_for_selection(selection).map_err(LockFailure::Loader)?;
        match Self::read_state_at(&root)? {
            LockState::Present(lock) => {
                let request = Self::request_at(&root)?;
                let registry = embedded_registry()?;
                let verdict = LockVerifier::verify(
                    &lock,
                    &request,
                    &RuntimeInventory::empty(),
                    registry,
                    requirement,
                );
                match verdict {
                    super::verify::LockVerdict::Satisfied { digest } => Ok(LockReceipt {
                        status: "valid",
                        operation: "lock",
                        mode: "check",
                        changed: false,
                        lock_digest: digest.as_str().to_owned(),
                        resolver_version: lock.resolver_version().as_str().to_owned(),
                        counts: super::ComponentCounts::of(&lock),
                    }),
                    super::verify::LockVerdict::Refused(failure) => Err(failure),
                }
            }
            LockState::Absent if create_if_absent => {
                let request = Self::request_at(&root)?;
                let registry = embedded_registry()?;
                let resolved = LockResolver::resolve(&request, &candidates, registry)?;
                let prepared = Self::prepare_at(&root, &request, resolved)?;
                let plan_id = prepared.plan_id.clone();
                let applied = Self::apply(prepared, &plan_id)?;
                let state = Self::read_state_at(&root)?;
                Ok(LockReceipt {
                    status: "valid",
                    operation: "lock",
                    mode: "create",
                    changed: applied.changed,
                    lock_digest: applied.after_digest.clone(),
                    resolver_version: applied.resolver_version.clone(),
                    counts: super::ComponentCounts::of(state.present()?),
                })
            }
            LockState::Absent => Err(LockFailure::Missing),
        }
    }

    /// Plan one update against the current project and lock state.
    pub fn plan(
        selection: &LoadSelection,
        candidates: CandidateSet,
    ) -> Result<PreparedLockUpdate, LockFailure> {
        let root = crate::loader::root_for_selection(selection).map_err(LockFailure::Loader)?;
        let request = Self::request_at(&root)?;
        let registry = embedded_registry()?;
        let resolved = LockResolver::resolve(&request, &candidates, registry)?;
        Self::prepare_at(&root, &request, resolved)
    }

    /// The pure dry-run projection of one plan: no guard, no writes, no
    /// `.lekalo` creation.
    pub fn preview(prepared: &PreparedLockUpdate) -> UpdateReceipt {
        UpdateReceipt {
            status: "valid",
            operation: "update",
            mode: "dry-run",
            changed: prepared.changed,
            lock_digest: prepared.after_digest.as_str().to_owned(),
            resolver_version: prepared.resolver_version.clone(),
            plan_id: prepared.plan_id.clone(),
            before_digest: prepared
                .before
                .as_ref()
                .map(|digest| digest.as_str().to_owned()),
            after_digest: prepared.after_digest.as_str().to_owned(),
            changes: prepared.changes.clone(),
        }
    }

    /// Apply one plan under its exact identity: guard, revalidate, stage,
    /// verify, atomically replace, and re-read. A no-op plan writes
    /// nothing.
    pub fn apply(
        prepared: PreparedLockUpdate,
        plan_id: &str,
    ) -> Result<UpdateReceipt, LockFailure> {
        if plan_id != prepared.plan_id || Sha256Digest::parse(plan_id).is_err() {
            return Err(LockFailure::SourceChanged);
        }
        if !prepared.changed {
            // Revalidate the before state, then write nothing.
            validate_unchanged(
                read_raw_lock_at(&prepared.root)?.as_deref(),
                prepared.before.as_ref(),
            )?;
            return Ok(receipt(&prepared, false));
        }
        let guard = acquire_guard(&prepared.root)?;
        let outcome = Self::apply_locked(&prepared);
        drop(guard);
        outcome?;
        Ok(receipt(&prepared, true))
    }

    fn apply_locked(prepared: &PreparedLockUpdate) -> Result<(), LockFailure> {
        // Revalidate the before state while holding the guard.
        let target = prepared.root.join("lekalo.lock");
        let stage = prepared.root.join(STAGE);
        let staged = || -> Result<(), LockFailure> {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&stage)
                .map_err(|_| LockFailure::CommitFailed)?;
            file.write_all(&prepared.after_bytes)
                .map_err(|_| LockFailure::CommitFailed)?;
            file.sync_all().map_err(|_| LockFailure::CommitFailed)?;
            drop(file);
            let written = std::fs::read(&stage).map_err(|_| LockFailure::CommitFailed)?;
            if written != prepared.after_bytes {
                return Err(LockFailure::CommitFailed);
            }
            // Create mode: the target must still be absent. Replace mode:
            // it must still be a reparse-free regular file.
            let existing = std::fs::symlink_metadata(&target);
            match prepared.before {
                None => {
                    if existing.is_ok() {
                        return Err(LockFailure::SourceChanged);
                    }
                }
                Some(_) => {
                    let metadata = existing.map_err(|_| LockFailure::CommitFailed)?;
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        return Err(LockFailure::PathDenied);
                    }
                    #[cfg(windows)]
                    {
                        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
                        use std::os::windows::fs::MetadataExt;
                        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                            return Err(LockFailure::PathDenied);
                        }
                    }
                }
            }
            std::fs::rename(&stage, &target).map_err(|_| LockFailure::CommitFailed)?;
            sync_parent(&prepared.root);
            // Ambiguous durability: re-read and verify the committed bytes.
            let committed = std::fs::read(&target).map_err(|_| LockFailure::RecoveryRequired)?;
            if committed != prepared.after_bytes {
                return Err(LockFailure::RecoveryRequired);
            }
            Ok(())
        };
        let result = staged();
        if result.is_err() {
            // Remove only the exact owned stage file; best effort.
            let _ = std::fs::remove_file(&stage);
        }
        result
    }

    fn request_at(root: &Path) -> Result<ResolutionRequest, LockFailure> {
        let registry = embedded_registry()?;
        let model =
            crate::loader::load_validated_root(root.to_path_buf()).map_err(LockFailure::Loader)?;
        Ok(request_for_model(registry, model.model.model_version))
    }

    fn prepare_at(
        root: &Path,
        request: &ResolutionRequest,
        resolved: Lockfile,
    ) -> Result<PreparedLockUpdate, LockFailure> {
        let existing = Self::read_state_at(root)?;
        let before = existing.as_lock().map(|lock| {
            Sha256Digest::from_hex(&crate::versioning::plan::sha256_hex(
                &canonical::payload_bytes(lock),
            ))
        });
        let after_bytes = canonical::file_bytes(&resolved);
        let after_digest = canonical::lock_digest(&resolved);
        let changes = diff_opt(existing.as_lock(), &resolved);
        let changed = match existing.as_lock() {
            Some(existing) => canonical::file_bytes(existing) != after_bytes,
            None => true,
        };
        let catalog_digests: Vec<String> = request
            .catalogs()
            .iter()
            .map(|catalog| catalog.digest().as_str().to_owned())
            .collect();
        let plan_id = compute_plan_id(
            before.as_ref(),
            &after_digest,
            request,
            &catalog_digests,
            resolved.resolver_version().as_str(),
        );
        Ok(PreparedLockUpdate {
            root: root.to_path_buf(),
            before,
            after_bytes,
            after_digest,
            plan_id,
            resolver_version: resolved.resolver_version().as_str().to_owned(),
            changes,
            changed,
        })
    }

    /// Read and fully validate the lock file at an already-selected root.
    pub fn read_state_at(root: &Path) -> Result<LockState, LockFailure> {
        let fs = crate::project_fs::Fs::open(root).map_err(|_| LockFailure::Structure {
            code: "structure.root-unreadable".to_owned(),
            denied: false,
        })?;
        match fs.entry_type("", "lekalo.lock") {
            Err(crate::project_fs::FsErrorKind::NotFound) => return Ok(LockState::Absent),
            Err(_) => {
                return Err(LockFailure::Structure {
                    code: "structure.lock-not-file".to_owned(),
                    denied: false,
                })
            }
            Ok(crate::project_fs::EntryType::Symlink) => {
                return Err(LockFailure::Structure {
                    code: "structure.path-link".to_owned(),
                    denied: true,
                })
            }
            Ok(crate::project_fs::EntryType::File) => {}
            Ok(_) => {
                return Err(LockFailure::Structure {
                    code: "structure.lock-not-file".to_owned(),
                    denied: false,
                })
            }
        }
        let bytes = fs
            .read_file_opt("", "lekalo.lock", MAX_LOCKFILE_BYTES)
            .map_err(|failure| match failure {
                crate::project_fs::FsErrorKind::NotFound => LockFailure::Missing,
                crate::project_fs::FsErrorKind::Limit { .. } => LockFailure::SchemaInvalid,
                crate::project_fs::FsErrorKind::Io => LockFailure::PathDenied,
            })?
            .ok_or(LockFailure::Missing)?;
        Ok(LockState::Present(Box::new(Lockfile::parse_canonical(
            &bytes,
        )?)))
    }
}

fn request_for_model(
    registry: &VersionRegistry,
    version: crate::loader::ModelVersion,
) -> ResolutionRequest {
    let model_version = <crate::versioning::ContractVersion<
        crate::versioning::family::ModelContract,
    >>::from(version);
    let core_version =
        SemVer::parse(super::PRODUCT_VERSION).expect("the workspace product version is canonical");
    ResolutionRequest::new(registry, &model_version, core_version)
}

fn validate_unchanged(
    current: Option<&[u8]>,
    before: Option<&Sha256Digest>,
) -> Result<(), LockFailure> {
    match (current, before) {
        (None, None) => Ok(()),
        (Some(bytes), Some(before)) => {
            let digest = Sha256Digest::from_hex(&crate::versioning::plan::sha256_hex(bytes));
            if digest == *before {
                Ok(())
            } else {
                Err(LockFailure::SourceChanged)
            }
        }
        _ => Err(LockFailure::SourceChanged),
    }
}

/// The exact raw bytes of the current lock file, for byte-level
/// compare-and-swap; `None` when absent. The bytes are deliberately not
/// parsed: a CAS mismatch is a source change, never a validation verdict.
fn read_raw_lock_at(root: &Path) -> Result<Option<Vec<u8>>, LockFailure> {
    let fs = crate::project_fs::Fs::open(root).map_err(|_| LockFailure::Structure {
        code: "structure.root-unreadable".to_owned(),
        denied: false,
    })?;
    match fs.entry_type("", "lekalo.lock") {
        Err(crate::project_fs::FsErrorKind::NotFound) => return Ok(None),
        Err(_) => {
            return Err(LockFailure::Structure {
                code: "structure.lock-not-file".to_owned(),
                denied: false,
            })
        }
        Ok(crate::project_fs::EntryType::Symlink) => {
            return Err(LockFailure::Structure {
                code: "structure.path-link".to_owned(),
                denied: true,
            })
        }
        Ok(crate::project_fs::EntryType::File) => {}
        Ok(_) => {
            return Err(LockFailure::Structure {
                code: "structure.lock-not-file".to_owned(),
                denied: false,
            })
        }
    }
    // Normalize the file form to the payload form (exactly one final LF
    // stripped) so the CAS digest matches the pinned payload digest.
    let mut bytes = fs
        .read_file_opt("", "lekalo.lock", MAX_LOCKFILE_BYTES)
        .map_err(|_| LockFailure::PathDenied)?
        .ok_or(LockFailure::Missing)?;
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    Ok(Some(bytes))
}

fn receipt(prepared: &PreparedLockUpdate, changed: bool) -> UpdateReceipt {
    UpdateReceipt {
        status: "valid",
        operation: "update",
        mode: "apply",
        changed,
        lock_digest: prepared.after_digest.as_str().to_owned(),
        resolver_version: prepared.resolver_version.clone(),
        plan_id: prepared.plan_id.clone(),
        before_digest: prepared
            .before
            .as_ref()
            .map(|digest| digest.as_str().to_owned()),
        after_digest: prepared.after_digest.as_str().to_owned(),
        changes: prepared.changes.clone(),
    }
}

/// The structured sorted diff between two locks over adapters, generators,
/// and profiles. Capability-set differences are implied by component
/// changes and are not separate entries.
pub(crate) fn diff_opt(before: Option<&Lockfile>, after: &Lockfile) -> Vec<ChangeEntry> {
    fn collect(lock: &Lockfile) -> Vec<(String, String, String, String)> {
        let mut items: Vec<(String, String, String, String)> = Vec::new();
        for adapter in lock.adapters() {
            items.push((
                "adapter".to_owned(),
                adapter.id().as_str().to_owned(),
                adapter.version().as_str().to_owned(),
                adapter.digest().as_str().to_owned(),
            ));
        }
        for generator in lock.generators() {
            items.push((
                "generator".to_owned(),
                generator.id().as_str().to_owned(),
                generator.version().as_str().to_owned(),
                generator.digest().as_str().to_owned(),
            ));
        }
        for profile in lock.profiles() {
            items.push((
                "profile".to_owned(),
                profile.id().as_str().to_owned(),
                profile.version().as_str().to_owned(),
                profile.digest().as_str().to_owned(),
            ));
        }
        items
    }
    fn kind_of(kind: &str) -> &'static str {
        match kind {
            "adapter" => "adapter",
            "generator" => "generator",
            _ => "profile",
        }
    }
    let before_map: std::collections::BTreeMap<(String, String), (String, String)> = before
        .map(collect)
        .unwrap_or_default()
        .into_iter()
        .map(|(kind, id, version, digest)| ((kind, id), (version, digest)))
        .collect();
    let after_map: std::collections::BTreeMap<(String, String), (String, String)> = collect(after)
        .into_iter()
        .map(|(kind, id, version, digest)| ((kind, id), (version, digest)))
        .collect();
    // Sorted keys give the sorted change order for free.
    let mut keys: std::collections::BTreeSet<(String, String)> =
        before_map.keys().cloned().collect();
    keys.extend(after_map.keys().cloned());
    let mut changes: Vec<ChangeEntry> = Vec::new();
    for (kind, id) in keys {
        let before = before_map.get(&(kind.clone(), id.clone()));
        let after = after_map.get(&(kind.clone(), id.clone()));
        let kind = kind_of(&kind);
        match (before, after) {
            (Some((version, digest)), Some((after_version, after_digest)))
                if version == after_version && digest == after_digest => {}
            (Some((version, digest)), Some((after_version, after_digest))) => {
                changes.push(ChangeEntry {
                    kind,
                    id: id.clone(),
                    from: Some(VersionDigest {
                        version: version.clone(),
                        digest: digest.clone(),
                    }),
                    to: Some(VersionDigest {
                        version: after_version.clone(),
                        digest: after_digest.clone(),
                    }),
                });
            }
            (None, Some((version, digest))) => {
                changes.push(ChangeEntry {
                    kind,
                    id: id.clone(),
                    from: None,
                    to: Some(VersionDigest {
                        version: version.clone(),
                        digest: digest.clone(),
                    }),
                });
            }
            (Some((version, digest)), None) => {
                changes.push(ChangeEntry {
                    kind,
                    id: id.clone(),
                    from: Some(VersionDigest {
                        version: version.clone(),
                        digest: digest.clone(),
                    }),
                    to: None,
                });
            }
            (None, None) => unreachable!("key came from one of the maps"),
        }
    }
    changes
}

/// SHA-256 over the documented plan-identity composition: before-or-absent
/// marker, after digest, request digest, ordered catalog digests, resolver
/// version.
fn compute_plan_id(
    before: Option<&Sha256Digest>,
    after: &LockDigest,
    request: &ResolutionRequest,
    catalog_digests: &[String],
    resolver_version: &str,
) -> String {
    let text = canonical::Canon::object(vec![
        ("after", canonical::Canon::str(after.as_str())),
        (
            "before",
            match before {
                Some(digest) => canonical::Canon::str(digest.as_str()),
                None => canonical::Canon::str("absent"),
            },
        ),
        (
            "catalogs",
            canonical::Canon::array(
                catalog_digests
                    .iter()
                    .map(|digest| canonical::Canon::str(digest.clone()))
                    .collect(),
            ),
        ),
        (
            "request",
            canonical::Canon::str(request.request_digest().as_str()),
        ),
        ("resolver", canonical::Canon::str(resolver_version)),
    ])
    .to_json();
    Sha256Digest::from_hex(&crate::versioning::plan::sha256_hex(text.as_bytes()))
        .as_str()
        .to_owned()
}

/// Best-effort parent-directory flush (POSIX only).
fn sync_parent(root: &Path) {
    #[cfg(unix)]
    {
        if let Ok(directory) = std::fs::File::open(root) {
            let _ = directory.sync_all();
        }
    }
    #[cfg(windows)]
    {
        let _ = root;
    }
}

/// The process-death-released update guard: an exclusive advisory lock on
/// an empty runtime file. Ownership is held by the open descriptor or
/// share-mode-zero handle; the kernel releases it when the process dies,
/// and the file contents carry no PID, path, or identity.
pub(crate) struct UpdateGuard {
    _file: GuardFile,
}

#[cfg(unix)]
struct GuardFile {
    // Held for RAII: dropping the file releases the OS lock.
    #[allow(dead_code)]
    file: std::fs::File,
}

#[cfg(windows)]
struct GuardFile {
    // Held for RAII: dropping the handle releases the OS lock.
    #[allow(dead_code)]
    file: std::fs::File,
}

fn acquire_guard(root: &Path) -> Result<UpdateGuard, LockFailure> {
    // Ensure the runtime directory chain exists (real directories only).
    let mut path = root.to_path_buf();
    for segment in [".lekalo", "cache", "locks"] {
        path.push(segment);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.is_symlink() || !metadata.is_dir() {
                    return Err(LockFailure::PathDenied);
                }
                #[cfg(windows)]
                {
                    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                        return Err(LockFailure::PathDenied);
                    }
                }
            }
            Err(_) => {
                std::fs::create_dir(&path).map_err(|_| LockFailure::CommitFailed)?;
            }
        }
    }
    let guard_path = root.join(GUARD);
    #[cfg(unix)]
    {
        use rustix::fs::{flock, FlockOperation};
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&guard_path)
            .map_err(|_| LockFailure::CommitFailed)?;
        flock(&file, FlockOperation::NonBlockingLockExclusive)
            .map_err(|_| LockFailure::UpdateInProgress)?;
        Ok(UpdateGuard {
            _file: GuardFile { file },
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(&guard_path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::PermissionDenied {
                    LockFailure::UpdateInProgress
                } else {
                    LockFailure::CommitFailed
                }
            })?;
        Ok(UpdateGuard {
            _file: GuardFile { file },
        })
    }
}
