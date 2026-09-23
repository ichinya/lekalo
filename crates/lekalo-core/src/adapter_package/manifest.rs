//! The closed adapter package manifest (issue #32).
//!
//! [`ManifestDocument`] is the typed, canonical form of one
//! `lekalo/adapter-manifest/v0.3.2` document. Parsing is fail-closed:
//! duplicate keys, unknown members, non-canonical member shapes, and
//! every grammar/bound violation refuse before a value exists. The
//! manifest digest is SHA-256 over the canonical bytes with the
//! self-referential `manifestDigest` member excluded — the exact
//! pattern of the lock digest — so a manifest never carries a digest it
//! could forge.

use std::fmt;

use serde::Deserialize;
use serde_json::Value as Json;

use super::canonical;
use super::types::{parse_scope, PackageFailure, PackageId, PackagePath, SemVer, Sha256Digest};
use super::version::{MANIFEST_IDENTITY, MANIFEST_SCHEMA_VERSION};

/// A declared per-file integrity row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileIntegrity {
    pub(crate) path: PackagePath,
    pub(crate) digest: Sha256Digest,
    pub(crate) bytes: u64,
}

impl FileIntegrity {
    /// The package-relative path.
    pub fn path(&self) -> &str {
        self.path.as_str()
    }

    /// The declared SHA-256 over the exact file bytes.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }

    /// The declared byte length.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// The declared signature block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    pub(crate) scheme: SignatureScheme,
    pub(crate) bundle_digest: Sha256Digest,
    pub(crate) signer_identity: Option<String>,
}

impl Signature {
    /// The closed scheme token.
    pub const fn scheme(&self) -> SignatureScheme {
        self.scheme
    }

    /// The digest-addressed signature material.
    pub fn bundle_digest(&self) -> &Sha256Digest {
        &self.bundle_digest
    }

    /// The declared signer identity, when present.
    pub fn signer_identity(&self) -> Option<&str> {
        self.signer_identity.as_deref()
    }
}

/// The closed signature scheme vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignatureScheme {
    Minisign,
    SigstoreBundle,
    PgpCleartext,
}

impl SignatureScheme {
    /// Parse the closed wire token.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "minisign" => Some(Self::Minisign),
            "sigstore-bundle" => Some(Self::SigstoreBundle),
            "pgp-cleartext" => Some(Self::PgpCleartext),
            _ => None,
        }
    }

    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minisign => "minisign",
            Self::SigstoreBundle => "sigstore-bundle",
            Self::PgpCleartext => "pgp-cleartext",
        }
    }
}

/// The closed signature policy vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignaturePolicy {
    /// No signature exists; digest verification is still mandatory.
    Unsigned,
    /// A signature may be declared; its verification is best-effort.
    Optional,
    /// A signature must verify under a shipped verifier.
    Required,
}

impl SignaturePolicy {
    /// Parse the closed wire token.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "unsigned" => Some(Self::Unsigned),
            "optional" => Some(Self::Optional),
            "required" => Some(Self::Required),
            _ => None,
        }
    }

    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unsigned => "unsigned",
            Self::Optional => "optional",
            Self::Required => "required",
        }
    }
}

/// The closed lifecycle status vocabulary (publisher self-report).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestStatus {
    Active,
    Yanked,
    Revoked,
}

impl ManifestStatus {
    /// Parse the closed wire token.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "active" => Some(Self::Active),
            "yanked" => Some(Self::Yanked),
            "revoked" => Some(Self::Revoked),
            _ => None,
        }
    }

    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Yanked => "yanked",
            Self::Revoked => "revoked",
        }
    }
}

/// The closed source-kind vocabulary of the manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SourceKind {
    Path,
    PathExec,
    Release,
    Registry,
}

impl SourceKind {
    /// Parse the closed wire token.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "path" => Some(Self::Path),
            "path-exec" => Some(Self::PathExec),
            "release" => Some(Self::Release),
            "registry" => Some(Self::Registry),
            _ => None,
        }
    }

    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::PathExec => "path-exec",
            Self::Release => "release",
            Self::Registry => "registry",
        }
    }
}

/// The typed, validated adapter package manifest.
#[derive(Clone, Debug)]
pub struct ManifestDocument {
    canonical: Json,
    adapter_id: PackageId,
    adapter_name: String,
    adapter_version: SemVer,
    source_kind: SourceKind,
    source_coordinate: String,
    entry: PackagePath,
    package_digest: Sha256Digest,
    files: Vec<FileIntegrity>,
    signature_policy: SignaturePolicy,
    signature: Option<Signature>,
    status: ManifestStatus,
    protocol_versions: Vec<String>,
    ir_versions: Vec<String>,
    extensions: Vec<String>,
}

impl ManifestDocument {
    /// Parse and validate one manifest from raw JSON bytes. Duplicate
    /// keys are refused by the raw reader; every member shape, grammar,
    /// bound, and cross-field invariant is validated here.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PackageFailure> {
        let value: Json = serde_json::from_slice(bytes).map_err(duplicate_or_malformed)?;
        Self::from_value(value)
    }

    /// Parse and validate one manifest from an in-memory JSON value.
    pub fn from_value(value: Json) -> Result<Self, PackageFailure> {
        let wire: ManifestWire = serde_json::from_value(value.clone()).map_err(|error| {
            PackageFailure::ManifestInvalid {
                reason: bounded_reason(&error.to_string()),
            }
        })?;
        wire.validate()?;
        let package_digest = Sha256Digest::parse(&wire.integrity.package_digest)?;
        let adapter_id = PackageId::parse(&wire.adapter.id)?;
        let adapter_version = SemVer::parse(&wire.adapter.version)?;
        let entry = PackagePath::parse(&wire.executable.entry)?;
        let mut files = Vec::with_capacity(wire.integrity.files.len());
        for file in &wire.integrity.files {
            files.push(FileIntegrity {
                path: PackagePath::parse(&file.path)?,
                digest: Sha256Digest::parse(&file.digest)?,
                bytes: file.bytes,
            });
        }
        // Sorted by path, unique: the canonical per-file order.
        files.sort_by(|left, right| left.path.cmp(&right.path));
        if files.windows(2).any(|pair| pair[0].path >= pair[1].path) {
            return Err(PackageFailure::ManifestInvalid {
                reason: "files-order".to_owned(),
            });
        }
        let mut protocol_versions = wire.compatibility.protocol_versions.clone();
        protocol_versions.sort();
        protocol_versions.dedup();
        let mut ir_versions = wire.compatibility.ir_versions.clone();
        ir_versions.sort();
        ir_versions.dedup();
        let mut extensions = wire.compatibility.extensions.clone();
        extensions.sort();
        extensions.dedup();
        let signature = match &wire.integrity.signature {
            Some(signature) => Some(Signature {
                scheme: SignatureScheme::parse(&signature.scheme).ok_or_else(|| {
                    PackageFailure::ManifestInvalid {
                        reason: "signature-scheme".to_owned(),
                    }
                })?,
                bundle_digest: Sha256Digest::parse(&signature.bundle_digest)?,
                signer_identity: signature.signer_identity.clone(),
            }),
            None => None,
        };
        Ok(Self {
            canonical: canonical::without_member(&value, "manifestDigest"),
            adapter_id,
            adapter_name: wire.adapter.name,
            adapter_version,
            source_kind: SourceKind::parse(&wire.source.kind).ok_or_else(|| {
                PackageFailure::ManifestInvalid {
                    reason: "source-kind".to_owned(),
                }
            })?,
            source_coordinate: wire.source.coordinate,
            entry,
            package_digest,
            files,
            signature_policy: SignaturePolicy::parse(&wire.integrity.signature_policy).ok_or_else(
                || PackageFailure::ManifestInvalid {
                    reason: "signature-policy".to_owned(),
                },
            )?,
            signature,
            status: ManifestStatus::parse(&wire.status).ok_or_else(|| {
                PackageFailure::ManifestInvalid {
                    reason: "status".to_owned(),
                }
            })?,
            protocol_versions,
            ir_versions,
            extensions,
        })
    }

    /// The canonical bytes the manifest digest is computed over: the
    /// stored document minus the self-referential `manifestDigest`
    /// member, no trailing LF.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        canonical::canonical_bytes(&self.canonical)
    }

    /// The package-digest-domain bytes: the canonical form with the
    /// self-referential `integrity.packageDigest` zeroed — a digest
    /// cannot cover the member it defines. The JS generator applies the
    /// same exclusion (issue #32 fix round 1, finding F-2).
    pub fn digest_domain_bytes(&self) -> Vec<u8> {
        let mut value = self.canonical.clone();
        if let serde_json::Value::Object(ref mut map) = value {
            if let Some(serde_json::Value::Object(integrity)) = map.get_mut("integrity") {
                integrity.insert(
                    "packageDigest".to_owned(),
                    serde_json::Value::String(format!("sha256:{}", "0".repeat(64))),
                );
            }
        }
        super::canonical::canonical_bytes(&value)
    }

    /// The manifest identity digest (SHA-256 over
    /// [`ManifestDocument::canonical_bytes`]).
    pub fn digest(&self) -> ManifestDigest {
        ManifestDigest(Sha256Digest::from_hex(&crate::digest::sha256_hex(
            &self.canonical_bytes(),
        )))
    }

    /// The adapter identifier.
    pub fn adapter_id(&self) -> &str {
        self.adapter_id.as_str()
    }

    /// The bounded adapter display name.
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// The exact adapter version.
    pub fn adapter_version(&self) -> &SemVer {
        &self.adapter_version
    }

    /// The declared source kind.
    pub const fn source_kind(&self) -> SourceKind {
        self.source_kind
    }

    /// The closed source coordinate token.
    pub fn source_coordinate(&self) -> &str {
        &self.source_coordinate
    }

    /// The package-relative entry path.
    pub fn entry(&self) -> &str {
        self.entry.as_str()
    }

    /// The declared package-root digest.
    pub fn package_digest(&self) -> &Sha256Digest {
        &self.package_digest
    }

    /// The sorted per-file integrity rows.
    pub fn files(&self) -> &[FileIntegrity] {
        &self.files
    }

    /// The declared signature policy.
    pub const fn signature_policy(&self) -> SignaturePolicy {
        self.signature_policy
    }

    /// The declared signature block, when present.
    pub const fn signature(&self) -> Option<&Signature> {
        self.signature.as_ref()
    }

    /// The declared lifecycle status.
    pub const fn status(&self) -> ManifestStatus {
        self.status
    }

    /// The sorted declared protocol versions.
    pub fn protocol_versions(&self) -> &[String] {
        &self.protocol_versions
    }

    /// The sorted declared IR versions.
    pub fn ir_versions(&self) -> &[String] {
        &self.ir_versions
    }

    /// The sorted declared required extensions.
    pub fn extensions(&self) -> &[String] {
        &self.extensions
    }

    /// Whether the compatibility set covers the exact version strings
    /// (the current core contracts at gate time).
    pub fn covers(&self, protocol_version: &str, ir_version: &str) -> bool {
        self.protocol_versions.iter().any(|v| v == protocol_version)
            && self.ir_versions.iter().any(|v| v == ir_version)
    }
}

impl PartialEq for ManifestDocument {
    fn eq(&self, other: &Self) -> bool {
        self.canonical_bytes() == other.canonical_bytes()
    }
}

impl Eq for ManifestDocument {}

impl fmt::Display for ManifestDocument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {} ({})",
            self.adapter_id,
            self.adapter_version,
            self.digest()
        )
    }
}

/// The manifest identity digest: SHA-256 over the canonical bytes minus
/// the self-referential `manifestDigest` member.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ManifestDigest(Sha256Digest);

impl ManifestDigest {
    /// The exact wire spelling (`sha256:<hex>`).
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for ManifestDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

fn duplicate_or_malformed(_: serde_json::Error) -> PackageFailure {
    PackageFailure::ManifestInvalid {
        reason: "json".to_owned(),
    }
}

fn bounded_reason(detail: &str) -> String {
    let mut token: String = detail
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    token.truncate(48);
    while token.ends_with('-') {
        token.pop();
    }
    if token.is_empty() {
        token.push_str("invalid");
    }
    token
}

// ---------- wire ----------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestWire {
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    identity: String,
    adapter: AdapterWire,
    publisher: PublisherWire,
    source: SourceWire,
    license: LicenseWire,
    compatibility: CompatibilityWire,
    capabilities: CapabilitiesWire,
    executable: ExecutableWire,
    #[serde(default)]
    #[allow(dead_code)] // schema-complete wire; the closed platform set is
    // enforced by the JSON Schema gate
    platforms: Option<Vec<String>>,
    integrity: IntegrityWire,
    permissions: PermissionsWire,
    hooks: Vec<Json>,
    conformance: ConformanceWire,
    status: String,
    #[serde(default)]
    revocation: Option<Json>,
    #[serde(default)]
    #[allow(dead_code)] // self-referential member, excluded from the digest
    manifest_digest: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdapterWire {
    id: String,
    name: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublisherWire {
    id: String,
    #[serde(default)]
    #[allow(dead_code)] // display-only member; the gate reads id + trustAnchor
    name: Option<String>,
    #[serde(rename = "trustAnchor")]
    trust_anchor: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceWire {
    kind: String,
    coordinate: String,
    digest: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LicenseWire {
    spdx: String,
    file: String,
    #[serde(rename = "fileDigest")]
    file_digest: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompatibilityWire {
    #[serde(rename = "protocolVersions")]
    protocol_versions: Vec<String>,
    #[serde(rename = "irVersions")]
    ir_versions: Vec<String>,
    #[serde(default)]
    extensions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilitiesWire {
    #[serde(default)]
    #[allow(dead_code)]
    operations: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    targets: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    profiles: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    named: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    #[allow(dead_code)]
    constraints: Option<Json>,
    #[serde(default, rename = "readScopes")]
    read_scopes: Vec<String>,
    #[serde(default, rename = "writeScopes")]
    write_scopes: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    transports: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutableWire {
    #[serde(default)]
    #[allow(dead_code)]
    runtime: Option<Json>,
    entry: String,
    #[serde(default, rename = "argvPreview")]
    #[allow(dead_code)]
    argv_preview: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    assets: Vec<Json>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegrityWire {
    #[serde(rename = "packageDigest")]
    package_digest: String,
    files: Vec<FileWire>,
    #[serde(rename = "signaturePolicy")]
    signature_policy: String,
    #[serde(default)]
    signature: Option<SignatureWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileWire {
    path: String,
    digest: String,
    bytes: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignatureWire {
    scheme: String,
    #[serde(rename = "bundleDigest")]
    bundle_digest: String,
    #[serde(default, rename = "signerIdentity")]
    signer_identity: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PermissionsWire {
    #[allow(dead_code)] // schema-complete wire; the execution policy lives
    // in permissions.rs (#89) and reads the canonical JSON
    filesystem: Json,
    #[allow(dead_code)]
    network: Json,
    #[allow(dead_code)]
    environment: Json,
    #[allow(dead_code)]
    processes: Json,
    #[allow(dead_code)]
    secrets: Json,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConformanceWire {
    #[serde(rename = "reportDigest")]
    #[allow(dead_code)]
    report_digest: String,
    #[allow(dead_code)]
    badge: Json,
    #[serde(rename = "suiteRegistry")]
    suite_registry: String,
}

impl ManifestWire {
    fn validate(&self) -> Result<(), PackageFailure> {
        let invalid = |reason: &str| PackageFailure::ManifestInvalid {
            reason: reason.to_owned(),
        };
        if self.schema_version != MANIFEST_SCHEMA_VERSION || self.identity != MANIFEST_IDENTITY {
            return Err(invalid("identity"));
        }
        if self.adapter.id.is_empty()
            || self.adapter.id.len() > 128
            || self.adapter.name.is_empty()
            || self.adapter.name.len() > 128
        {
            return Err(invalid("adapter"));
        }
        SemVer::parse(&self.adapter.version)?;
        if self.source.coordinate.len() < 5 || self.source.coordinate.len() > 256 {
            return Err(invalid("source-coordinate"));
        }
        if self.source.coordinate.contains("://") || self.source.coordinate.contains("..") {
            return Err(invalid("source-coordinate"));
        }
        Sha256Digest::parse(&self.source.digest)?;
        {
            PackageId::parse(&self.publisher.id).map_err(|_| invalid("publisher"))?;
            if !matches!(
                self.publisher.trust_anchor.as_str(),
                "builtin" | "registry" | "publisher-key" | "none"
            ) {
                return Err(invalid("trust-anchor"));
            }
        }
        {
            if self.license.spdx.is_empty() || self.license.spdx.len() > 64 {
                return Err(invalid("license"));
            }
            PackagePath::parse(&self.license.file).map_err(|_| invalid("license"))?;
            Sha256Digest::parse(&self.license.file_digest)?;
        }
        if self.compatibility.protocol_versions.is_empty()
            || self.compatibility.protocol_versions.len() > 8
            || self.compatibility.ir_versions.is_empty()
            || self.compatibility.ir_versions.len() > 8
        {
            return Err(invalid("compatibility"));
        }
        for version in self
            .compatibility
            .protocol_versions
            .iter()
            .chain(self.compatibility.ir_versions.iter())
        {
            if version.len() > 32 || version.parse::<semver::Version>().is_err() {
                return Err(invalid("compatibility"));
            }
        }
        {
            for scope in self
                .capabilities
                .read_scopes
                .iter()
                .chain(&self.capabilities.write_scopes)
            {
                parse_scope(scope)?;
            }
        }
        if self.integrity.files.is_empty() || self.integrity.files.len() > 512 {
            return Err(invalid("integrity-files"));
        }
        for file in &self.integrity.files {
            PackagePath::parse(&file.path)?;
            Sha256Digest::parse(&file.digest)?;
            if file.bytes == 0 || file.bytes > 268_435_456 {
                return Err(invalid("integrity-files"));
            }
        }
        Sha256Digest::parse(&self.integrity.package_digest)?;
        let policy = SignaturePolicy::parse(&self.integrity.signature_policy)
            .ok_or_else(|| invalid("signature-policy"))?;
        match (&self.integrity.signature, policy) {
            (None, SignaturePolicy::Unsigned) => {}
            (Some(_), SignaturePolicy::Unsigned) => {
                return Err(invalid("signature-policy"));
            }
            (None, SignaturePolicy::Optional) => {}
            (Some(signature), SignaturePolicy::Optional) => {
                SignatureScheme::parse(&signature.scheme)
                    .ok_or_else(|| invalid("signature-scheme"))?;
                Sha256Digest::parse(&signature.bundle_digest)?;
            }
            // `required` may declare the signature block; without a
            // shipped verifier the gate answers honestly (§ signature).
            (None, SignaturePolicy::Required) => {
                return Err(invalid("signature-policy"));
            }
            (Some(signature), SignaturePolicy::Required) => {
                SignatureScheme::parse(&signature.scheme)
                    .ok_or_else(|| invalid("signature-scheme"))?;
                Sha256Digest::parse(&signature.bundle_digest)?;
            }
        }
        ManifestStatus::parse(&self.status).ok_or_else(|| invalid("status"))?;
        if self.status == "active" && self.revocation.is_some() {
            return Err(invalid("revocation"));
        }
        if matches!(self.status.as_str(), "yanked" | "revoked") && self.revocation.is_none() {
            return Err(invalid("revocation"));
        }
        // No scripts by default is structural: hooks are pinned empty.
        if !self.hooks.is_empty() {
            return Err(invalid("hooks"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod committed_exemplar_tests {
    /// The shipped exemplar must parse through the exact wire structs:
    /// this is the structural schema-parity guard for the serde spellings
    /// (issue #32 fix round 1, finding F-1).
    #[test]
    fn the_committed_adapter_manifest_parses() {
        let bytes = include_bytes!("../../../../adapters/node-typescript/adapter.manifest.json",);
        let document = super::ManifestDocument::from_bytes(bytes)
            .expect("the committed adapter manifest must parse");
        assert_eq!(document.adapter_id(), "lekalo-target-node-typescript");
        assert_eq!(document.adapter_version().as_str(), "0.3.2");
        assert_eq!(
            document.package_digest().as_str(),
            "sha256:8bb2a397509c4b36f25a8e91fb829fc1b7c761eb9490f3ebe11f72568648731e"
        );
    }
}
