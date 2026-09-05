//! Deterministic canonical serialization and strict parsing of the
//! ownership manifest (issue #21).
//!
//! The same discipline as the #10 lock: compact UTF-8 JSON, object keys in
//! unsigned UTF-8 byte order, closed arrays in the documented sorted order,
//! RFC 8259 mandatory escaping — the file is the payload plus exactly one
//! LF, and `manifest_digest` is SHA-256 over the canonical payload with the
//! `manifest_digest` property removed, so the document can never carry its
//! own digest. Classification order is fixed: size and encoding, JSON
//! parse, schema discriminator dispatch, closed v1 shape, full typed and
//! cross-field validation, digest verification, and only then the
//! byte-equality gate that classifies a semantically valid but noncanonical
//! document as noncanonical instead of ever rewriting it.

use serde::Deserialize;

use super::types::{
    AdapterRef, ArtifactEntry, ArtifactKey, ArtifactKind, ArtifactManifest, ArtifactPath,
    Lifecycle, LockRevision, ProjectRef, RegenerationPolicy, SemanticOwnerId, SourceMapBinding,
    SourceRange, IDENTITY, SCHEMA_VERSION,
};
use super::ArtifactFailure;
use crate::loader::canonical::Canonical;
use crate::loader::ModelVersion;
use crate::lockfile::types::ArtifactPin;
use crate::lockfile::{
    ComponentId, ComponentRef, ContractPin, LockDigest, Platform, SemVer, Sha256Digest,
};
use crate::versioning::plan::sha256_hex;

impl ArtifactManifest {
    /// Parse and fully validate exact manifest bytes (file or payload
    /// form). Any semantically equal but noncanonical spelling is refused
    /// and never rewritten.
    pub fn parse_canonical(bytes: &[u8]) -> Result<Self, ArtifactFailure> {
        if bytes.len() > super::types::MAX_MANIFEST_BYTES {
            return Err(ArtifactFailure::ManifestInvalid);
        }
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| ArtifactFailure::ManifestInvalid)?;
        let raw_schema = match value.get("schema_version") {
            Some(serde_json::Value::String(text)) => text.clone(),
            _ => return Err(ArtifactFailure::ManifestInvalid),
        };
        if raw_schema != SCHEMA_VERSION {
            let future = raw_schema.starts_with("lekalo/artifact-manifest/v");
            return Err(if future {
                ArtifactFailure::UnsupportedSchemaVersion { found: raw_schema }
            } else {
                ArtifactFailure::ManifestInvalid
            });
        }
        let raw: RawManifest =
            serde_json::from_value(value).map_err(|_| ArtifactFailure::ManifestInvalid)?;
        let manifest = build(raw)?;
        let canonical = super::canonical::payload_bytes(&manifest);
        let input = strip_final_lf(bytes);
        if input != canonical.as_slice() {
            return Err(ArtifactFailure::ManifestNoncanonical);
        }
        Ok(manifest)
    }

    /// The exact canonical file bytes: payload plus exactly one LF.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut payload = payload_bytes(self);
        payload.push(b'\n');
        payload
    }
}

fn strip_final_lf(bytes: &[u8]) -> &[u8] {
    match bytes.strip_suffix(b"\n") {
        Some(stripped) => stripped,
        None => bytes,
    }
}

/// The canonical payload bytes: the full document including
/// `manifest_digest`, without the final LF.
pub(crate) fn payload_bytes(manifest: &ArtifactManifest) -> Vec<u8> {
    manifest_canon(manifest, true).to_json().into_bytes()
}

/// The digest input: canonical payload bytes with the `manifest_digest`
/// property removed and no final LF (non-self-referential by construction).
pub(crate) fn digest_input_bytes(manifest: &ArtifactManifest) -> Vec<u8> {
    manifest_canon(manifest, false).to_json().into_bytes()
}

/// One object with byte-sorted keys (the canonical key order).
fn object(fields: Vec<(&'static str, Canonical)>) -> Canonical {
    let mut pairs: Vec<(String, Canonical)> = fields
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
    pairs.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    debug_assert!(pairs.windows(2).all(|pair| pair[0].0 != pair[1].0));
    Canonical::Map(pairs)
}

fn manifest_canon(manifest: &ArtifactManifest, with_digest: bool) -> Canonical {
    let mut fields = vec![
        ("schema_version", Canonical::Str(SCHEMA_VERSION.to_owned())),
        ("identity", Canonical::Str(IDENTITY.to_owned())),
        (
            "project_ref",
            Canonical::Str(manifest.project().as_str().to_owned()),
        ),
        (
            "lock_ref",
            object(vec![
                (
                    "schema_version",
                    Canonical::Str(crate::lockfile::types::SCHEMA_VERSION.to_owned()),
                ),
                (
                    "digest",
                    Canonical::Str(manifest.lock_ref().digest().as_str().to_owned()),
                ),
            ]),
        ),
        (
            "inputs",
            object(vec![
                ("model", pin_canon(manifest.model())),
                ("ir", pin_canon(manifest.ir())),
            ]),
        ),
        (
            "artifacts",
            Canonical::Seq(manifest.artifacts().iter().map(artifact_canon).collect()),
        ),
        (
            "source_maps",
            Canonical::Seq(manifest.source_maps().iter().map(map_canon).collect()),
        ),
    ];
    if with_digest {
        fields.push((
            "manifest_digest",
            Canonical::Str(manifest.manifest_digest().as_str().to_owned()),
        ));
    }
    object(fields)
}

fn pin_canon(pin: &ContractPin) -> Canonical {
    object(vec![
        ("version", Canonical::Str(pin.version().as_str().to_owned())),
        ("digest", Canonical::Str(pin.digest().as_str().to_owned())),
    ])
}

fn artifact_canon(entry: &ArtifactEntry) -> Canonical {
    let mut fields = vec![
        (
            "semantic_owner",
            Canonical::Str(entry.key().semantic_owner().as_str().to_owned()),
        ),
        (
            "path",
            Canonical::Str(entry.key().path().as_str().to_owned()),
        ),
        (
            "artifact_kind",
            Canonical::Str(entry.key().kind().as_str().to_owned()),
        ),
        (
            "lifecycle",
            Canonical::Str(entry.lifecycle().as_str().to_owned()),
        ),
        ("content", content_canon(entry.content())),
        (
            "input_refs",
            Canonical::Seq(
                entry
                    .input_refs()
                    .iter()
                    .map(|owner| Canonical::Str(owner.as_str().to_owned()))
                    .collect(),
            ),
        ),
        (
            "regeneration_policy",
            Canonical::Str(entry.lifecycle().policy().as_str().to_owned()),
        ),
    ];
    if let Some(adapter) = entry.adapter() {
        fields.push(("adapter_ref", adapter_canon(adapter)));
    }
    if let Some(generator) = entry.generator() {
        fields.push((
            "generator_ref",
            object(vec![
                ("id", Canonical::Str(generator.id().as_str().to_owned())),
                (
                    "version",
                    Canonical::Str(generator.version().as_str().to_owned()),
                ),
            ]),
        ));
    }
    object(fields)
}

fn adapter_canon(adapter: &AdapterRef) -> Canonical {
    object(vec![
        ("id", Canonical::Str(adapter.id().as_str().to_owned())),
        (
            "version",
            Canonical::Str(adapter.version().as_str().to_owned()),
        ),
        (
            "digest",
            Canonical::Str(adapter.digest().as_str().to_owned()),
        ),
        (
            "artifacts",
            Canonical::Seq(
                adapter
                    .artifacts()
                    .iter()
                    .map(|pin| {
                        object(vec![
                            (
                                "platform",
                                Canonical::Str(pin.platform().as_str().to_owned()),
                            ),
                            ("digest", Canonical::Str(pin.digest().as_str().to_owned())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "protocol_version",
            match adapter.protocol_version() {
                Some(version) => Canonical::Str(version.as_str().to_owned()),
                None => Canonical::Null,
            },
        ),
    ])
}

fn content_canon(digest: &Sha256Digest) -> Canonical {
    object(vec![
        ("algorithm", Canonical::Str("sha256".to_owned())),
        ("digest", Canonical::Str(digest.as_str().to_owned())),
        (
            "canonicalization",
            Canonical::Str("exact-file-bytes".to_owned()),
        ),
    ])
}

fn map_canon(binding: &SourceMapBinding) -> Canonical {
    object(vec![
        (
            "artifact",
            object(vec![
                (
                    "semantic_owner",
                    Canonical::Str(binding.key().semantic_owner().as_str().to_owned()),
                ),
                (
                    "path",
                    Canonical::Str(binding.key().path().as_str().to_owned()),
                ),
                (
                    "artifact_kind",
                    Canonical::Str(binding.key().kind().as_str().to_owned()),
                ),
            ]),
        ),
        (
            "input_revision",
            Canonical::Str(binding.input_revision().as_str().to_owned()),
        ),
        (
            "entries",
            Canonical::Seq(
                binding
                    .entries()
                    .iter()
                    .map(|range| {
                        object(vec![
                            (
                                "semantic_id",
                                Canonical::Str(range.semantic_id().as_str().to_owned()),
                            ),
                            ("start", Canonical::Int(range.start() as i128)),
                            ("end", Canonical::Int(range.end() as i128)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

#[derive(Deserialize)]
struct RawManifest {
    schema_version: String,
    identity: String,
    project_ref: String,
    lock_ref: RawLockRef,
    inputs: RawInputs,
    artifacts: Vec<RawArtifact>,
    source_maps: Vec<RawSourceMap>,
    manifest_digest: String,
}

#[derive(Deserialize)]
struct RawLockRef {
    schema_version: String,
    digest: String,
}

#[derive(Deserialize)]
struct RawInputs {
    model: RawPin,
    ir: RawPin,
}

#[derive(Deserialize)]
struct RawPin {
    version: String,
    digest: String,
}

#[derive(Deserialize)]
struct RawArtifact {
    semantic_owner: String,
    path: String,
    artifact_kind: String,
    lifecycle: String,
    adapter_ref: Option<RawAdapterRef>,
    generator_ref: Option<RawGeneratorRef>,
    content: RawContent,
    input_refs: Vec<String>,
    regeneration_policy: String,
}

#[derive(Deserialize)]
struct RawAdapterRef {
    id: String,
    version: String,
    digest: String,
    artifacts: Vec<RawArtifactPin>,
    protocol_version: Option<String>,
}

#[derive(Deserialize)]
struct RawArtifactPin {
    platform: String,
    digest: String,
}

#[derive(Deserialize)]
struct RawGeneratorRef {
    id: String,
    version: String,
}

#[derive(Deserialize)]
struct RawContent {
    algorithm: String,
    digest: String,
    canonicalization: String,
}

#[derive(Deserialize)]
struct RawSourceMap {
    artifact: RawKey,
    input_revision: String,
    entries: Vec<RawRange>,
}

#[derive(Deserialize)]
struct RawKey {
    semantic_owner: String,
    path: String,
    artifact_kind: String,
}

#[derive(Deserialize)]
struct RawRange {
    semantic_id: String,
    start: u32,
    end: u32,
}

fn build(raw: RawManifest) -> Result<ArtifactManifest, ArtifactFailure> {
    if raw.schema_version != SCHEMA_VERSION || raw.identity != IDENTITY {
        return Err(ArtifactFailure::ManifestInvalid);
    }
    let model_pin = pin(&raw.inputs.model)?;
    let ir_pin = pin(&raw.inputs.ir)?;
    let grammar_version = model_version(raw.inputs.model.version.as_str())?;
    let project = ProjectRef::parse(&raw.project_ref, grammar_version)
        .ok_or(ArtifactFailure::ManifestInvalid)?;
    if raw.lock_ref.schema_version != crate::lockfile::types::SCHEMA_VERSION {
        return Err(ArtifactFailure::ManifestInvalid);
    }
    let lock_ref = LockRevision::new(LockDigest::new(digest(&raw.lock_ref.digest)?));

    if raw.artifacts.len() > 65536 || raw.source_maps.len() > 65536 {
        return Err(ArtifactFailure::ManifestInvalid);
    }
    let mut artifacts = Vec::with_capacity(raw.artifacts.len());
    for artifact in &raw.artifacts {
        artifacts.push(artifact_entry(artifact, grammar_version)?);
    }
    require_sorted(&artifacts, |entry| {
        (
            entry.key().semantic_owner().as_str().to_owned(),
            entry.key().path().as_str().to_owned(),
            entry.key().kind().as_str().to_owned(),
        )
    })?;
    let mut source_maps = Vec::with_capacity(raw.source_maps.len());
    for map in &raw.source_maps {
        source_maps.push(source_map_binding(map, grammar_version, &artifacts)?);
    }
    require_sorted(&source_maps, |binding| {
        (
            binding.key().semantic_owner().as_str().to_owned(),
            binding.key().path().as_str().to_owned(),
            binding.key().kind().as_str().to_owned(),
        )
    })?;
    let declared = digest(&raw.manifest_digest)?;
    let draft = ArtifactManifest::new(
        project,
        lock_ref,
        model_pin,
        ir_pin,
        artifacts,
        source_maps,
        declared.clone(),
    );
    let computed = Sha256Digest::from_hex(&sha256_hex(&digest_input_bytes(&draft)));
    if computed.as_str() != declared.as_str() {
        return Err(ArtifactFailure::ManifestDigestMismatch);
    }
    Ok(draft)
}

fn artifact_entry(
    raw: &RawArtifact,
    grammar_version: ModelVersion,
) -> Result<ArtifactEntry, ArtifactFailure> {
    let owner = SemanticOwnerId::parse(&raw.semantic_owner, grammar_version)
        .ok_or(ArtifactFailure::ManifestInvalid)?;
    let path = ArtifactPath::parse(&raw.path).ok_or(ArtifactFailure::ManifestInvalid)?;
    let kind = ArtifactKind::parse(&raw.artifact_kind).ok_or(ArtifactFailure::ManifestInvalid)?;
    let lifecycle = Lifecycle::parse(&raw.lifecycle).ok_or(ArtifactFailure::ManifestInvalid)?;
    let policy = RegenerationPolicy::parse(&raw.regeneration_policy)
        .ok_or(ArtifactFailure::ManifestInvalid)?;
    if lifecycle.policy() != policy {
        return Err(ArtifactFailure::ManifestInvalid);
    }
    let adapter = match &raw.adapter_ref {
        Some(raw_adapter) => Some(adapter_ref(raw_adapter)?),
        None => None,
    };
    let generator = match &raw.generator_ref {
        Some(generator) => Some(ComponentRef::new(
            ComponentId::parse(&generator.id).map_err(|_| ArtifactFailure::ManifestInvalid)?,
            SemVer::parse(&generator.version).map_err(|_| ArtifactFailure::ManifestInvalid)?,
        )),
        None => None,
    };
    if raw.content.algorithm != "sha256" || raw.content.canonicalization != "exact-file-bytes" {
        return Err(ArtifactFailure::ManifestInvalid);
    }
    let content = digest(&raw.content.digest)?;
    let mut input_refs = Vec::with_capacity(raw.input_refs.len());
    for symbol in &raw.input_refs {
        input_refs.push(
            SemanticOwnerId::parse(symbol, grammar_version)
                .ok_or(ArtifactFailure::ManifestInvalid)?,
        );
    }
    require_sorted(&input_refs, |owner| owner.as_str().to_owned())?;
    input_refs.dedup();
    Ok(ArtifactEntry::new(
        ArtifactKey::new(owner, path, kind),
        lifecycle,
        adapter,
        generator,
        content,
        input_refs,
    ))
}

fn adapter_ref(raw: &RawAdapterRef) -> Result<AdapterRef, ArtifactFailure> {
    let id = ComponentId::parse(&raw.id).map_err(|_| ArtifactFailure::ManifestInvalid)?;
    let version = SemVer::parse(&raw.version).map_err(|_| ArtifactFailure::ManifestInvalid)?;
    let package = digest(&raw.digest)?;
    if raw.artifacts.is_empty() || raw.artifacts.len() > 64 {
        return Err(ArtifactFailure::ManifestInvalid);
    }
    let mut pins = Vec::with_capacity(raw.artifacts.len());
    for artifact in &raw.artifacts {
        pins.push(ArtifactPin::new(
            Platform::parse(&artifact.platform).map_err(|_| ArtifactFailure::ManifestInvalid)?,
            digest(&artifact.digest)?,
        ));
    }
    let protocol_version = match &raw.protocol_version {
        Some(version) => {
            Some(SemVer::parse(version).map_err(|_| ArtifactFailure::ManifestInvalid)?)
        }
        None => None,
    };
    AdapterRef::new(id, version, package, pins, protocol_version)
        .ok_or(ArtifactFailure::ReferenceInvalid)
}

fn source_map_binding(
    raw: &RawSourceMap,
    grammar_version: ModelVersion,
    artifacts: &[ArtifactEntry],
) -> Result<SourceMapBinding, ArtifactFailure> {
    let owner = SemanticOwnerId::parse(&raw.artifact.semantic_owner, grammar_version)
        .ok_or(ArtifactFailure::ManifestInvalid)?;
    let key = ArtifactKey::new(
        owner,
        ArtifactPath::parse(&raw.artifact.path).ok_or(ArtifactFailure::ManifestInvalid)?,
        ArtifactKind::parse(&raw.artifact.artifact_kind).ok_or(ArtifactFailure::ManifestInvalid)?,
    );
    if !artifacts.iter().any(|entry| entry.key() == &key) {
        return Err(ArtifactFailure::ReferenceInvalid);
    }
    let input_revision = digest(&raw.input_revision)?;
    if raw.entries.len() > 4096 {
        return Err(ArtifactFailure::ManifestInvalid);
    }
    let mut entries = Vec::with_capacity(raw.entries.len());
    for range in &raw.entries {
        if range.start >= range.end {
            return Err(ArtifactFailure::SourceMapInvalid);
        }
        entries.push(SourceRange::new(
            SemanticOwnerId::parse(&range.semantic_id, grammar_version)
                .ok_or(ArtifactFailure::ManifestInvalid)?,
            range.start,
            range.end,
        ));
    }
    entries.sort();
    entries.dedup();
    for pair in entries.windows(2) {
        if pair[0].semantic_id() == pair[1].semantic_id() && pair[1].start() < pair[0].end() {
            return Err(ArtifactFailure::SourceMapInvalid);
        }
    }
    Ok(SourceMapBinding::new(key, input_revision, entries))
}

fn pin(raw: &RawPin) -> Result<ContractPin, ArtifactFailure> {
    Ok(ContractPin::new(
        SemVer::parse(&raw.version).map_err(|_| ArtifactFailure::ManifestInvalid)?,
        digest(&raw.digest)?,
    ))
}

fn digest(text: &str) -> Result<Sha256Digest, ArtifactFailure> {
    Sha256Digest::parse(text).map_err(|_| ArtifactFailure::ManifestInvalid)
}

fn model_version(text: &str) -> Result<ModelVersion, ArtifactFailure> {
    match text {
        "0.1.0" => Ok(ModelVersion::V0_1_0),
        "1.0.0" => Ok(ModelVersion::V1_0_0),
        _ => Err(ArtifactFailure::ManifestInvalid),
    }
}

fn require_sorted<T, K: Ord>(items: &[T], key: impl Fn(&T) -> K) -> Result<(), ArtifactFailure> {
    items
        .windows(2)
        .all(|pair| key(&pair[0]) < key(&pair[1]))
        .then_some(())
        .ok_or(ArtifactFailure::ReferenceInvalid)
}
