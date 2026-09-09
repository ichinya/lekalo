//! The pure check: bind the manifest to the exact current lock, inputs,
//! and adapter identity, then verify every recorded artifact and the
//! declared managed root (issue #21).
//!
//! The service is hermetic: it reads through the accepted `Fs` capability
//! only, never spawns adapters, never writes, and orders every finding by
//! fixed bytes so no filesystem, thread, locale, or timezone order can
//! leak into a verdict.

use super::types::{
    ArtifactEntry, ArtifactManifest, DriftVerdict, Lifecycle, MANIFEST_DIR, MANIFEST_NAME,
};
use super::{ArtifactFailure, CheckReceipt, DriftFinding, VerdictCounts};
use crate::loader::{self, LoadSelection, NormalizedModel};
use crate::lockfile::plan::LockService;
use crate::lockfile::{ContractPin, LockFailure, LockState, Lockfile, SemVer, Sha256Digest};
use crate::project_fs::{EntryType, Fs, FsErrorKind};
use crate::result::DomainResult;

/// One prepared project: the validated root, its read capability, the
/// exact lock revision, the current inputs, and the parsed manifest.
pub(crate) struct Prepared {
    root: std::path::PathBuf,
    fs: Fs,
    lock: Lockfile,
    inputs: Inputs,
    manifest: Option<ArtifactManifest>,
}

/// The current generation inputs.
pub(crate) struct Inputs {
    model: ContractPin,
    ir: ContractPin,
}

impl Inputs {
    pub fn model(&self) -> &ContractPin {
        &self.model
    }

    pub fn ir(&self) -> &ContractPin {
        &self.ir
    }
}

impl Prepared {
    /// Run the accepted pipeline: select and validate the root, load the
    /// canonical model, compile the typed IR, read the exact lock, and
    /// parse the manifest. Every failure is terminal and classified.
    pub(crate) fn prepare(selection: &LoadSelection) -> Result<Self, ArtifactFailure> {
        let root = loader::root_for_selection(selection).map_err(classify_loader_refusal)?;
        let fs = Fs::open(&root).map_err(|_| ArtifactFailure::Structure {
            code: "structure.root-unreadable",
            denied: false,
        })?;
        let inputs = current_inputs(selection)?;
        let lock = match LockService::read_state_at(&root)? {
            LockState::Present(lock) => lock,
            LockState::Absent => return Err(ArtifactFailure::Lock(LockFailure::Missing)),
        };
        let manifest = read_manifest(&fs)?;
        Ok(Self {
            root,
            fs,
            lock: *lock,
            inputs,
            manifest,
        })
    }

    pub(crate) fn fs(&self) -> &Fs {
        &self.fs
    }

    pub(crate) fn lock(&self) -> &Lockfile {
        &self.lock
    }

    pub(crate) fn manifest(&self) -> Option<&ArtifactManifest> {
        self.manifest.as_ref()
    }

    pub(crate) fn inputs(&self) -> &Inputs {
        &self.inputs
    }

    pub(crate) fn root(&self) -> &std::path::Path {
        &self.root
    }
}

/// Classify a loader-pipeline refusal for the generate surface. When the
/// accepted #4 structure gate denies through a policy rule (a link, a
/// special file, an alias, a hostile name, or a placement violation
/// anywhere in the project tree or the runtime area), the refusal is the
/// mandated artifacts structure denial with the same rule identity and
/// the denied exit class, no matter which seam observed the entry first:
/// the loader walk runs before the managed-root scan, so on a no-follow
/// filesystem the link is found there first. Any other refusal passes
/// through as the terminal loader result, unchanged.
fn classify_loader_refusal(result: DomainResult) -> ArtifactFailure {
    let mut codes = result
        .diagnostics()
        .iter()
        .map(|d| crate::project_fs::structure_policy_code(d.id()));
    match codes.next() {
        // The structure scan short-circuits on the first refusal, so a
        Some(Some(code)) if codes.all(|c| c.is_some()) => {
            ArtifactFailure::Structure { code, denied: true }
        }
        _ => ArtifactFailure::Loader(result),
    }
}

/// Compile the current inputs: the canonical Model payload digest and the
/// canonical typed-IR payload digest, plus their exact contract versions.
fn current_inputs(selection: &LoadSelection) -> Result<Inputs, ArtifactFailure> {
    let model_json = match loader::run(selection, false) {
        crate::result::DomainResult::Valid {
            payload: crate::result::SuccessPayload::Model { json, .. },
            ..
        } => json,
        other => return Err(classify_loader_refusal(other)),
    };
    let model: NormalizedModel =
        loader::normalize_model(selection).map_err(classify_loader_refusal)?;
    let compilation = crate::ir::compile(&model)
        .map_err(|failure| classify_loader_refusal(failure.into_result()))?;
    let ir_json = compilation.project.to_canonical_json();
    let model_version = SemVer::parse(model.model_version.as_str())
        .map_err(|_| ArtifactFailure::ManifestInvalid)?;
    let ir_version =
        SemVer::parse(crate::ir::version::VERSION).map_err(|_| ArtifactFailure::ManifestInvalid)?;
    let model = ContractPin::new(model_version, hash(model_json.as_bytes()));
    let ir = ContractPin::new(ir_version, hash(ir_json.as_bytes()));
    Ok(Inputs { model, ir })
}

/// The canonical inputs object over two pins (compact JSON, byte-sorted
/// keys); its SHA-256 is the inputs revision digest bound by source maps.
pub(crate) fn inputs_canon(model: &ContractPin, ir: &ContractPin) -> String {
    fn one(pin: &ContractPin) -> String {
        crate::loader::canonical::Canonical::Map(vec![
            (
                "digest".to_owned(),
                crate::loader::canonical::Canonical::Str(pin.digest().as_str().to_owned()),
            ),
            (
                "version".to_owned(),
                crate::loader::canonical::Canonical::Str(pin.version().as_str().to_owned()),
            ),
        ])
        .to_json()
    }
    // Byte-sorted object keys: "ir" < "model".
    format!("{{\"ir\":{},\"model\":{}}}", one(ir), one(model))
}

pub(crate) fn inputs_revision(model: &ContractPin, ir: &ContractPin) -> Sha256Digest {
    let bytes = inputs_canon(model, ir);
    Sha256Digest::from_hex(&crate::versioning::plan::sha256_hex(bytes.as_bytes()))
}

pub(crate) fn hash(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_hex(&crate::versioning::plan::sha256_hex(bytes))
}

/// Read the derived manifest; absence is the legal empty state.
fn read_manifest(fs: &Fs) -> Result<Option<ArtifactManifest>, ArtifactFailure> {
    match fs.entry_type(MANIFEST_DIR, MANIFEST_NAME) {
        Err(FsErrorKind::NotFound) => return Ok(None),
        Err(FsErrorKind::Limit { .. }) => return Err(ArtifactFailure::ManifestInvalid),
        Err(FsErrorKind::Io) => return Err(ArtifactFailure::Io("manifest-read")),
        Ok(EntryType::Symlink) => {
            return Err(ArtifactFailure::Structure {
                code: "structure.path-link",
                denied: true,
            })
        }
        Ok(EntryType::File) => {}
        Ok(_) => return Err(ArtifactFailure::ManifestInvalid),
    }
    let bytes = fs
        .read_file_opt(
            MANIFEST_DIR,
            MANIFEST_NAME,
            super::types::MAX_MANIFEST_BYTES,
        )
        .map_err(|failure| match failure {
            FsErrorKind::NotFound | FsErrorKind::Limit { .. } => ArtifactFailure::ManifestInvalid,
            FsErrorKind::Io => ArtifactFailure::Io("manifest-read"),
        })?
        .ok_or(ArtifactFailure::ManifestInvalid)?;
    ArtifactManifest::parse_canonical(&bytes).map(Some)
}

/// One observed file identity used by checks and clean plans.
pub(crate) struct Observed {
    pub digest: Sha256Digest,
    pub size: u64,
}

/// Read and hash one file's exact bytes through the governed capability.
pub(crate) fn observe(fs: &Fs, path: &str) -> Result<Option<Observed>, ArtifactFailure> {
    let (dir, name) = split(path);
    match fs.entry_type(dir, name) {
        Err(FsErrorKind::NotFound) => return Ok(None),
        Err(FsErrorKind::Limit { .. }) => return Err(ArtifactFailure::Io("artifact-limit")),
        Err(FsErrorKind::Io) => return Err(ArtifactFailure::Io("artifact-read")),
        Ok(EntryType::Symlink) => {
            return Err(ArtifactFailure::Structure {
                code: "structure.path-link",
                denied: true,
            })
        }
        Ok(EntryType::Special) => {
            return Err(ArtifactFailure::Structure {
                code: "structure.path-special",
                denied: true,
            })
        }
        Ok(EntryType::Directory) => return Ok(None),
        Ok(EntryType::File) => {}
    }
    let bytes = fs
        .read_file_opt(dir, name, super::types::MAX_ARTIFACT_BYTES)
        .map_err(|failure| match failure {
            FsErrorKind::NotFound | FsErrorKind::Io => ArtifactFailure::Io("artifact-read"),
            FsErrorKind::Limit { .. } => ArtifactFailure::Io("artifact-limit"),
        })?
        .ok_or(ArtifactFailure::Io("artifact-read"))?;
    Ok(Some(Observed {
        digest: hash(&bytes),
        size: bytes.len() as u64,
    }))
}

/// Split one logical path into its parent directory and file name.
pub(crate) fn split(path: &str) -> (&str, &str) {
    match path.rsplit_once('/') {
        Some((dir, name)) => (dir, name),
        None => ("", path),
    }
}

/// The typed adapter identity view of one exact lock revision.
pub(crate) struct AdapterInventory<'a> {
    lock: &'a Lockfile,
}

impl<'a> AdapterInventory<'a> {
    /// Whether one manifest adapter reference is deep-equal to the locked
    /// component: same id, version, package digest, every platform
    /// artifact pin, and the locked target protocol version.
    pub(crate) fn matches(&self, adapter: &super::AdapterRef) -> Result<(), ()> {
        let resolved = self
            .lock
            .adapters()
            .iter()
            .find(|candidate| candidate.id().as_str() == adapter.id().as_str())
            .ok_or(())?;
        if resolved.version().as_str() != adapter.version().as_str()
            || resolved.digest().as_str() != adapter.digest().as_str()
        {
            return Err(());
        }
        let locked_pins = resolved.artifacts();
        if locked_pins.len() != adapter.artifacts().len() {
            return Err(());
        }
        for pin in adapter.artifacts() {
            if !locked_pins.iter().any(|locked| {
                locked.platform().as_str() == pin.platform().as_str()
                    && locked.digest().as_str() == pin.digest().as_str()
            }) {
                return Err(());
            }
        }
        let protocol = self
            .lock
            .target_protocol()
            .map(|pin| pin.version().as_str());
        let recorded = adapter.protocol_version().map(SemVer::as_str);
        if protocol != recorded {
            return Err(());
        }
        Ok(())
    }
}

/// The full check: binding, inventory, source maps, and orphans.
pub(crate) fn run_check(prepared: &Prepared) -> Result<CheckReceipt, ArtifactFailure> {
    let mut findings: Vec<DriftFinding> = Vec::new();
    let mut blocking: Vec<DriftFinding> = Vec::new();
    let mut counts = VerdictCounts::default();

    let Some(manifest) = prepared.manifest() else {
        // No manifest: the managed root must also be empty; anything under
        // it is orphaned derived data no manifest claims.
        let orphans = super::clean::scan_orphans(&prepared.fs, &[])?;
        for path in orphans {
            blocking.push(DriftFinding::orphan(path));
        }
        counts.orphan = blocking.len();
        return finish(prepared, findings, blocking, counts, None);
    };

    // Exact bindings first: a manifest from another lock revision or other
    // inputs is stale as a whole; regenerate it, never patch it.
    if manifest.lock_ref().digest().as_str() != prepared.lock.digest().as_str()
        || manifest.model() != prepared.inputs.model()
        || manifest.ir() != prepared.inputs.ir()
    {
        return Err(ArtifactFailure::StaleManifest);
    }

    let inventory = AdapterInventory {
        lock: prepared.lock(),
    };
    for entry in manifest.artifacts() {
        counts.artifacts += 1;
        // Adapter identity binds exactly when the lock pins a published
        // target protocol AND names locked adapters: an adapter ref under
        // an unpublished protocol is unverifiable, a published protocol
        // without an adapter ref is an unbound artifact, and an entry that
        // names no adapter while the lock names adapters is unbound too.
        // All of these are staleness, never a silent pass. A lock that
        // names no adapters (the only kind the catalog can produce before
        // #91) keeps the v1 byte-drift semantics.
        let adapter_binding_required =
            prepared.lock.target_protocol().is_some() && !prepared.lock.adapters().is_empty();
        let verdict = match (entry.adapter(), adapter_binding_required) {
            (Some(adapter), true) if inventory.matches(adapter).is_ok() => {
                match observe(&prepared.fs, entry.key().path().as_str())? {
                    None => DriftVerdict::Missing,
                    Some(observed) if observed.digest.as_str() == entry.content().as_str() => {
                        DriftVerdict::Clean
                    }
                    Some(_) => DriftVerdict::ManualDrift,
                }
            }
            (None, false) => match observe(&prepared.fs, entry.key().path().as_str())? {
                None => DriftVerdict::Missing,
                Some(observed) if observed.digest.as_str() == entry.content().as_str() => {
                    DriftVerdict::Clean
                }
                Some(_) => DriftVerdict::ManualDrift,
            },
            _ => DriftVerdict::Stale,
        };
        match verdict {
            DriftVerdict::Clean => counts.clean += 1,
            DriftVerdict::Stale => {
                counts.stale += 1;
                push_finding(&mut findings, &mut blocking, entry, verdict);
            }
            DriftVerdict::ManualDrift => {
                counts.manual_drift += 1;
                push_finding(&mut findings, &mut blocking, entry, verdict);
            }
            DriftVerdict::Missing => {
                counts.missing += 1;
                push_finding(&mut findings, &mut blocking, entry, verdict);
            }
            DriftVerdict::Orphan => unreachable!("orphans come from the managed-root scan"),
        }
    }

    super::source_map::validate(manifest, prepared)?;

    let claimed: Vec<&str> = manifest
        .artifacts()
        .iter()
        .map(|entry| entry.key().path().as_str())
        .collect();
    let orphans = super::clean::scan_orphans(&prepared.fs, &claimed)?;
    counts.orphan = orphans.len();
    for path in orphans {
        blocking.push(DriftFinding::orphan(path));
    }

    finish(
        prepared,
        findings,
        blocking,
        counts,
        Some(manifest.manifest_digest().clone()),
    )
}

/// Generated findings block the check; every other lifecycle is reported
/// without repair and never overwritten.
fn push_finding(
    findings: &mut Vec<DriftFinding>,
    blocking: &mut Vec<DriftFinding>,
    entry: &ArtifactEntry,
    verdict: DriftVerdict,
) {
    let finding = DriftFinding::entry(entry, verdict);
    if entry.lifecycle() == Lifecycle::Generated {
        blocking.push(finding);
    } else {
        findings.push(finding);
    }
}

fn finish(
    prepared: &Prepared,
    mut findings: Vec<DriftFinding>,
    blocking: Vec<DriftFinding>,
    mut counts: VerdictCounts,
    manifest_digest: Option<Sha256Digest>,
) -> Result<CheckReceipt, ArtifactFailure> {
    if !blocking.is_empty() {
        return Err(ArtifactFailure::Drift(blocking));
    }
    findings.sort();
    counts.reported = findings.len();
    Ok(CheckReceipt {
        status: "valid",
        operation: "generate",
        mode: "check",
        manifest_digest: manifest_digest.map(|digest| digest.as_str().to_owned()),
        lock_digest: prepared.lock.digest().as_str().to_owned(),
        verdict: if findings.is_empty() {
            "clean"
        } else {
            "reported"
        },
        counts,
        findings,
    })
}
