//! Typed values of the generated-artifact ownership manifest (issue #21).
//!
//! Every externally meaningful value is a closed, private-field newtype or
//! enum: hostile bytes can never construct one, and every accessor returns
//! validated data only. Versions reuse the strict lockfile [`SemVer`]
//! spellings, digests reuse [`Sha256Digest`] (`sha256:<64 lowercase hex>`),
//! and component identity reuses [`ComponentId`], [`ComponentRef`],
//! [`ContractPin`], and [`ArtifactPin`] from the accepted #10 lock so a
//! manifest can never drift into its own competing identity grammar.

use crate::ir::grammar::{is_project_id, is_symbol_id};
use crate::loader::ModelVersion;
use crate::lockfile::types::ArtifactPin;
use crate::lockfile::{ComponentId, ComponentRef, ContractPin, LockDigest, SemVer, Sha256Digest};
use crate::project_fs;

/// The closed v1 discriminator.
pub const SCHEMA_VERSION: &str = "lekalo/artifact-manifest/v1.0.0";
/// The independent contract identity; unrelated to every other family.
pub const IDENTITY: &str = "dev.lekalo.artifact-manifest@1.0.0";
/// The exact derived-manifest home inside the reserved `.lekalo/generated`
/// runtime area.
pub const MANIFEST_DIR: &str = ".lekalo/generated/manifests";
/// The manifest leaf file name (owner decision, ADR-0015).
pub const MANIFEST_NAME: &str = "ownership.json";
/// The declared managed root scanned for orphans in v1; adapters with typed
/// write plans (#27/#91) extend this later, they never replace the check.
pub const GENERATED_ROOT: &str = ".lekalo/generated";
/// Maximum accepted manifest size; hostile inputs stay bounded.
pub const MAX_MANIFEST_BYTES: usize = 1 << 20;
/// Maximum bytes read from one artifact while hashing its exact content.
pub const MAX_ARTIFACT_BYTES: usize = 4 << 20;

/// One stable semantic symbol that owns a generated artifact.
///
/// Validated against the accepted #6 grammar for the manifest's Model
/// version; never an authority-matrix canonical owner and never free text.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct SemanticOwnerId(String);

impl SemanticOwnerId {
    pub(crate) fn parse(text: &str, version: ModelVersion) -> Option<Self> {
        is_symbol_id(version, text).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The one-segment immutable project id a manifest is bound to.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ProjectRef(String);

impl ProjectRef {
    pub(crate) fn parse(text: &str, version: ModelVersion) -> Option<Self> {
        is_project_id(version, text).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One project-relative POSIX logical path with portable lowercase
/// segments, rejected by the accepted path policy otherwise.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ArtifactPath(String);

impl ArtifactPath {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        if text.starts_with('/')
            || text.ends_with('/')
            || text.split('/').any(|segment| segment.is_empty())
        {
            return None;
        }
        project_fs::path_violation(text)
            .is_none()
            .then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The closed v1 artifact kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ArtifactKind {
    Source,
    Test,
    Schema,
    Config,
    Docs,
    Data,
}

impl ArtifactKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Test => "test",
            Self::Schema => "schema",
            Self::Config => "config",
            Self::Docs => "docs",
            Self::Data => "data",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "source" => Self::Source,
            "test" => Self::Test,
            "schema" => Self::Schema,
            "config" => Self::Config,
            "docs" => Self::Docs,
            "data" => Self::Data,
            _ => return None,
        })
    }
}

/// One project-relative path inside the declared managed root
/// (`.lekalo/generated/...`). The prefix is fixed; the remainder must
/// pass the accepted path policy. Manifest artifacts can never live under
/// the runtime home, so this type is exclusive to orphans and clean plans.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct GeneratedPath(String);

impl GeneratedPath {
    /// The public claim seam: a generated-artifact path must be rooted
    /// at the managed root, so observed files can never be claimed (the
    /// issue #39 ownership regression surface).
    pub fn claim(text: &str) -> Option<Self> {
        Self::parse(text)
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        let rest = text.strip_prefix(".lekalo/generated/")?;
        if rest.is_empty() {
            return None;
        }
        project_fs::path_violation(rest)
            .is_none()
            .then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The closed v1 ownership lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Lifecycle {
    /// Adapter-reproducible output; manual modification is forbidden and
    /// every drift, staleness, or absence blocks the check.
    Generated,
    /// Created once, then owned by the implementation; never overwritten,
    /// findings are reported without repair.
    Scaffolded,
    /// Hand-written source the adapter only validates; drift is reported,
    /// never repaired.
    Checked,
    /// Reference-only identity; no generation or clean authority.
    External,
    /// Target-specific escape hatch; never overwritten or generic-cleaned.
    Custom,
}

impl Lifecycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Scaffolded => "scaffolded",
            Self::Checked => "checked",
            Self::External => "external",
            Self::Custom => "custom",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "generated" => Self::Generated,
            "scaffolded" => Self::Scaffolded,
            "checked" => Self::Checked,
            "external" => Self::External,
            "custom" => Self::Custom,
            _ => return None,
        })
    }

    /// The one regeneration policy this lifecycle may carry.
    pub(crate) const fn policy(self) -> RegenerationPolicy {
        match self {
            Self::Generated => RegenerationPolicy::OnInputChange,
            Self::Scaffolded => RegenerationPolicy::Once,
            Self::Checked => RegenerationPolicy::ValidateOnly,
            Self::External => RegenerationPolicy::ReferenceOnly,
            Self::Custom => RegenerationPolicy::ManualOnly,
        }
    }
}

/// The closed v1 regeneration policy; pairs one-to-one with [`Lifecycle`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum RegenerationPolicy {
    OnInputChange,
    Once,
    ValidateOnly,
    ReferenceOnly,
    ManualOnly,
}

impl RegenerationPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OnInputChange => "on-input-change",
            Self::Once => "once",
            Self::ValidateOnly => "validate-only",
            Self::ReferenceOnly => "reference-only",
            Self::ManualOnly => "manual-only",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "on-input-change" => Self::OnInputChange,
            "once" => Self::Once,
            "validate-only" => Self::ValidateOnly,
            "reference-only" => Self::ReferenceOnly,
            "manual-only" => Self::ManualOnly,
            _ => return None,
        })
    }
}

/// The exact locked adapter identity that produced or verifies an artifact.
///
/// Deep-equal to the resolved lock component: id, canonical version,
/// package digest, every platform artifact pin, and the locked target
/// protocol version. A version-only match is invalid by construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterRef {
    id: ComponentId,
    version: SemVer,
    digest: Sha256Digest,
    artifacts: Vec<ArtifactPin>,
    protocol_version: Option<SemVer>,
}

impl AdapterRef {
    pub(crate) fn new(
        id: ComponentId,
        version: SemVer,
        digest: Sha256Digest,
        mut artifacts: Vec<ArtifactPin>,
        protocol_version: Option<SemVer>,
    ) -> Option<Self> {
        if artifacts.is_empty() {
            return None;
        }
        artifacts.sort_by(|left, right| {
            (left.platform().as_str(), left.digest().as_str())
                .cmp(&(right.platform().as_str(), right.digest().as_str()))
        });
        if artifacts.windows(2).any(|pair| {
            pair[0].platform().as_str() == pair[1].platform().as_str()
                && pair[0].digest().as_str() == pair[1].digest().as_str()
        }) {
            return None;
        }
        Some(Self {
            id,
            version,
            digest,
            artifacts,
            protocol_version,
        })
    }

    pub fn id(&self) -> &ComponentId {
        &self.id
    }

    pub fn version(&self) -> &SemVer {
        &self.version
    }

    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }

    pub fn artifacts(&self) -> &[ArtifactPin] {
        &self.artifacts
    }

    pub fn protocol_version(&self) -> Option<&SemVer> {
        self.protocol_version.as_ref()
    }
}

/// The unique artifact identity: owner, logical path, and kind.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ArtifactKey {
    semantic_owner: SemanticOwnerId,
    path: ArtifactPath,
    kind: ArtifactKind,
}

impl ArtifactKey {
    pub(crate) fn new(
        semantic_owner: SemanticOwnerId,
        path: ArtifactPath,
        kind: ArtifactKind,
    ) -> Self {
        Self {
            semantic_owner,
            path,
            kind,
        }
    }

    pub fn semantic_owner(&self) -> &SemanticOwnerId {
        &self.semantic_owner
    }

    pub fn path(&self) -> &ArtifactPath {
        &self.path
    }

    pub fn kind(&self) -> ArtifactKind {
        self.kind
    }
}

/// One half-open generated byte range bound to one semantic id.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct SourceRange {
    semantic_id: SemanticOwnerId,
    start: u32,
    end: u32,
}

impl SourceRange {
    pub(crate) fn new(semantic_id: SemanticOwnerId, start: u32, end: u32) -> Self {
        Self {
            semantic_id,
            start,
            end,
        }
    }

    pub fn semantic_id(&self) -> &SemanticOwnerId {
        &self.semantic_id
    }

    pub fn start(&self) -> u32 {
        self.start
    }

    pub fn end(&self) -> u32 {
        self.end
    }
}

/// The source map of one artifact: its exact key, the inputs revision the
/// ranges were generated from, and the sorted half-open ranges.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMapBinding {
    key: ArtifactKey,
    input_revision: Sha256Digest,
    entries: Vec<SourceRange>,
}

impl SourceMapBinding {
    pub(crate) fn new(
        key: ArtifactKey,
        input_revision: Sha256Digest,
        mut entries: Vec<SourceRange>,
    ) -> Self {
        entries.sort();
        entries.dedup();
        Self {
            key,
            input_revision,
            entries,
        }
    }

    pub fn key(&self) -> &ArtifactKey {
        &self.key
    }

    pub fn input_revision(&self) -> &Sha256Digest {
        &self.input_revision
    }

    pub fn entries(&self) -> &[SourceRange] {
        &self.entries
    }
}

/// One recorded artifact: identity, lifecycle, the exact locked producers
/// when the bound lock pins a published target protocol, the exact
/// observed content digest, and its traced input symbols.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactEntry {
    key: ArtifactKey,
    lifecycle: Lifecycle,
    adapter: Option<AdapterRef>,
    generator: Option<ComponentRef>,
    content: Sha256Digest,
    input_refs: Vec<SemanticOwnerId>,
}

impl ArtifactEntry {
    pub(crate) fn new(
        key: ArtifactKey,
        lifecycle: Lifecycle,
        adapter: Option<AdapterRef>,
        generator: Option<ComponentRef>,
        content: Sha256Digest,
        input_refs: Vec<SemanticOwnerId>,
    ) -> Self {
        Self {
            key,
            lifecycle,
            adapter,
            generator,
            content,
            input_refs,
        }
    }

    pub fn key(&self) -> &ArtifactKey {
        &self.key
    }

    pub fn lifecycle(&self) -> Lifecycle {
        self.lifecycle
    }

    pub fn adapter(&self) -> Option<&AdapterRef> {
        self.adapter.as_ref()
    }

    pub fn generator(&self) -> Option<&ComponentRef> {
        self.generator.as_ref()
    }

    pub fn content(&self) -> &Sha256Digest {
        &self.content
    }

    pub fn input_refs(&self) -> &[SemanticOwnerId] {
        &self.input_refs
    }
}

/// The exact lockfile revision a manifest was generated under.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockRevision {
    digest: LockDigest,
}

impl LockRevision {
    pub(crate) fn new(digest: LockDigest) -> Self {
        Self { digest }
    }

    pub fn digest(&self) -> &LockDigest {
        &self.digest
    }
}

/// The fully validated, closed ownership manifest.
///
/// Construction goes exclusively through
/// [`super::ArtifactManifest::parse_canonical`]; every field is private and
/// the canonical bytes are recomputable, so a value exists only after every
/// closed-format invariant passed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactManifest {
    project: ProjectRef,
    lock_ref: LockRevision,
    model: ContractPin,
    ir: ContractPin,
    artifacts: Vec<ArtifactEntry>,
    source_maps: Vec<SourceMapBinding>,
    digest: Sha256Digest,
}

impl ArtifactManifest {
    pub(crate) fn new(
        project: ProjectRef,
        lock_ref: LockRevision,
        model: ContractPin,
        ir: ContractPin,
        artifacts: Vec<ArtifactEntry>,
        source_maps: Vec<SourceMapBinding>,
        digest: Sha256Digest,
    ) -> Self {
        Self {
            project,
            lock_ref,
            model,
            ir,
            artifacts,
            source_maps,
            digest,
        }
    }

    pub fn project(&self) -> &ProjectRef {
        &self.project
    }

    pub fn lock_ref(&self) -> &LockRevision {
        &self.lock_ref
    }

    pub fn model(&self) -> &ContractPin {
        &self.model
    }

    pub fn ir(&self) -> &ContractPin {
        &self.ir
    }

    pub fn artifacts(&self) -> &[ArtifactEntry] {
        &self.artifacts
    }

    pub fn source_maps(&self) -> &[SourceMapBinding] {
        &self.source_maps
    }

    /// The manifest digest recorded on the document (digest over the
    /// canonical payload with the `manifest_digest` property removed).
    pub fn manifest_digest(&self) -> &Sha256Digest {
        &self.digest
    }

    /// Look up one entry by its exact key.
    pub fn entry(&self, key: &ArtifactKey) -> Option<&ArtifactEntry> {
        self.artifacts.iter().find(|entry| entry.key() == key)
    }
}

/// One observed drift verdict per artifact or orphan path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriftVerdict {
    /// Inputs, lock, adapters, and exact bytes all match the manifest.
    Clean,
    /// The current inputs, lock revision, or adapter identity no longer
    /// match the recorded generation inputs.
    Stale,
    /// The observed bytes differ from the recorded content digest.
    ManualDrift,
    /// A manifest-required artifact file is absent.
    Missing,
    /// A file inside the declared managed root has no manifest entry.
    Orphan,
}

impl DriftVerdict {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Stale => "stale",
            Self::ManualDrift => "manual-drift",
            Self::Missing => "missing",
            Self::Orphan => "orphan",
        }
    }
}
