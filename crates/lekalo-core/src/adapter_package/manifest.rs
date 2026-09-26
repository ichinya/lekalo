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
    stored: Vec<u8>,
    adapter_id: PackageId,
    adapter_name: String,
    adapter_version: SemVer,
    source_kind: SourceKind,
    source_coordinate: String,
    source_digest: Sha256Digest,
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
        let mut document = Self::from_value(value)?;
        document.stored = bytes.to_vec();
        Ok(document)
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
        let stored = super::canonical::canonical_file_bytes(&value);
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
            stored,
            adapter_id,
            adapter_name: wire.adapter.name,
            adapter_version,
            source_kind: SourceKind::parse(&wire.source.kind).ok_or_else(|| {
                PackageFailure::ManifestInvalid {
                    reason: "source-kind".to_owned(),
                }
            })?,
            source_coordinate: wire.source.coordinate,
            source_digest: Sha256Digest::parse(&wire.source.digest)?,
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

    /// The exact stored manifest bytes (the custody anchor the install
    /// stages into the package store).
    pub fn stored_bytes(&self) -> &[u8] {
        &self.stored
    }

    /// The manifest identity digest (SHA-256 over
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

    /// The declared source-snapshot digest.
    pub fn source_digest(&self) -> &Sha256Digest {
        &self.source_digest
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

    /// The manifest's canonical JSON value.
    fn canonical_member(&self, path: &[&str]) -> Option<serde_json::Value> {
        let mut current = &self.canonical;
        for key in path {
            current = current.get(*key)?;
        }
        Some(current.clone())
    }

    /// The declared capability operations, sorted (consistency check).
    pub fn operations(&self) -> Vec<String> {
        self.canonical_member(&["capabilities", "operations"])
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    /// The declared targets, sorted.
    pub fn targets(&self) -> Vec<String> {
        self.canonical_member(&["capabilities", "targets"])
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    /// The declared named-capability ids, sorted (the manifest's claim
    /// of which capability ids exist; states are describe's business).
    pub fn named(&self) -> Vec<String> {
        self.canonical_member(&["capabilities", "named"])
            .and_then(|value| serde_json::from_value::<serde_json::Map<String, Json>>(value).ok())
            .map(|map| {
                let mut ids: Vec<String> = map.keys().cloned().collect();
                ids.sort();
                ids
            })
            .unwrap_or_default()
    }

    /// The declared profiles, sorted.
    pub fn profiles(&self) -> Vec<String> {
        self.canonical_member(&["capabilities", "profiles"])
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    /// The declared read scopes, sorted.
    pub fn read_scopes(&self) -> Vec<String> {
        self.canonical_member(&["capabilities", "readScopes"])
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    /// The declared write scopes, sorted.
    pub fn write_scopes(&self) -> Vec<String> {
        self.canonical_member(&["capabilities", "writeScopes"])
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    /// The declared transports, sorted.
    pub fn transports(&self) -> Vec<String> {
        self.canonical_member(&["capabilities", "transports"])
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
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
    // Schema-required (parity fix round 2, cline F-5 / devin F-13): the
    // closed platform list is enforced here, not only at the JSON Schema
    // gate.
    platforms: Vec<String>,
    integrity: IntegrityWire,
    #[allow(dead_code)] // enforced-by-existence here; #89 reads the canonical JSON
    permissions: PermissionsWire,
    hooks: Vec<Json>,
    #[allow(dead_code)] // enforced-by-existence here; AIFHub imports read it
    conformance: ConformanceWire,
    status: String,
    #[serde(default)]
    revocation: Option<Json>,
    // The self-referential `manifestDigest` member is deliberately absent
    // from this wire: the schema omits it, so with `deny_unknown_fields`
    // both spellings (`manifestDigest` and snake_case) refuse identically
    // here and at the JSON Schema gate (parity fix round 2, devin F-13).
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
    #[allow(dead_code)] // validated-by-existence; projected by inventory docs
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
        // The closed platform vocabulary, enforced here — not only at the
        // JSON Schema gate (fix round 2, cline F-5 / devin F-13).
        if self.platforms.is_empty() || self.platforms.len() > 8 {
            return Err(invalid("platforms"));
        }
        for platform in &self.platforms {
            if !matches!(
                platform.as_str(),
                "any" | "windows-x64" | "linux-x64" | "linux-arm64" | "darwin-x64" | "darwin-arm64"
            ) {
                return Err(invalid("platforms"));
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
        // Permission member values are closed-vocabulary validated (fix
        // round 2, devin F-1): a schema-invalid value like
        // network.mode:"allowed" must refuse here, before the escalation
        // evaluator can mistake it for an un-widened member.
        self.permissions.validate()?;
        Ok(())
    }
}

impl PermissionsWire {
    /// Closed-vocabulary validation of every permission member value.
    fn validate(&self) -> Result<(), PackageFailure> {
        let invalid = |reason: &str| PackageFailure::ManifestInvalid {
            reason: reason.to_owned(),
        };
        let scopes = |member: &Json, key: &str| -> Result<(), PackageFailure> {
            // `member` IS the filesystem object: validate its `key` array
            // in place. (Fix round 4, devin N-2: the previous loop walked
            // `member.get("filesystem")…get(key)` — always `None` — so
            // `parse_scope` never ran and traversal spellings like
            // `"../escape"` were accepted at the Rust boundary while the
            // schema refused them.)
            let values = member
                .get(key)
                .and_then(|value| value.as_array())
                .ok_or_else(|| invalid("permissions-filesystem"))?;
            for scope in values.iter().filter_map(|item| item.as_str()) {
                super::types::parse_scope(scope)?;
            }
            Ok(())
        };
        for member in [
            &self.filesystem,
            &self.network,
            &self.environment,
            &self.processes,
            &self.secrets,
        ] {
            if !member.is_object() {
                return Err(invalid("permissions-shape"));
            }
        }
        let filesystem = &self.filesystem;
        scopes(filesystem, "readScopes")?;
        scopes(filesystem, "writeScopes")?;
        let network = &self.network;
        let mode = network
            .get("mode")
            .and_then(|mode| mode.as_str())
            .ok_or_else(|| invalid("permissions-network-mode"))?;
        if !matches!(mode, "denied" | "allowlist") {
            return Err(invalid("permissions-network-mode"));
        }
        if network
            .get("destinations")
            .and_then(|value| value.as_array())
            .is_none()
        {
            return Err(invalid("permissions-network-destinations"));
        }
        for destination in network
            .get("destinations")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| item.as_str())
        {
            let valid = !destination.is_empty()
                && destination.len() <= 64
                && destination
                    .strip_prefix("*.")
                    .unwrap_or(destination)
                    .bytes()
                    .all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || byte == b'.'
                            || byte == b'-'
                    });
            if !valid {
                return Err(invalid("permissions-host-token"));
            }
        }
        let environment = &self.environment;
        for name in environment
            .get("allowlist")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| item.as_str())
        {
            let valid = name
                .bytes()
                .next()
                .map(|first| first.is_ascii_uppercase())
                .unwrap_or(false)
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
                && !name.is_empty()
                && name.len() <= 64;
            if !valid {
                return Err(invalid("permissions-env-name"));
            }
        }
        let processes = &self.processes;
        match processes
            .get("children")
            .and_then(|children| children.as_str())
        {
            Some("denied") | Some("declared") => {}
            _ => return Err(invalid("permissions-processes")),
        }
        let secrets = &self.secrets;
        for handle in secrets
            .get("handles")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| item.as_str())
        {
            let valid = !handle.is_empty()
                && handle.len() <= 64
                && handle.starts_with(|c: char| c.is_ascii_lowercase())
                && handle
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
            if !valid {
                return Err(invalid("permissions-secret-handle"));
            }
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
        assert_eq!(document.adapter_version().as_str(), "0.4.0");
        assert_eq!(
            document.package_digest().as_str(),
            "sha256:b44fe06b9cc54ebc49148d5626306c0eba4d4bebd782144a73d008bd17d1a54c"
        );
    }
}

#[cfg(test)]
mod permission_validation_tests {
    use super::*;

    fn manifest_with_network_mode(mode: &str) -> Result<ManifestDocument, PackageFailure> {
        let json = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "perm-adapter", "name": "P", "version": "1.0.0" },
            "publisher": { "id": "p", "trustAnchor": "none" },
            "source": { "kind": "path", "coordinate": "path:x", "digest": format!("sha256:{}", "11".repeat(32)) },
            "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": { "protocolVersions": [crate::target_protocol::version::VERSION], "irVersions": [crate::ir::version::VERSION], "extensions": [] },
            "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
            "executable": { "runtime": { "kind": "node", "minVersion": "18.0.0" }, "entry": "a.mjs", "argvPreview": ["node", "a.mjs"], "assets": [] },
            "platforms": ["any"],
            "integrity": { "packageDigest": format!("sha256:{}", "22".repeat(32)), "files": [ { "path": "a.mjs", "digest": format!("sha256:{}", "33".repeat(32)), "bytes": 3 } ], "signaturePolicy": "unsigned", "signature": null },
            "permissions": {
                "filesystem": { "readScopes": ["src/**"], "writeScopes": ["generated/**"] },
                "network": { "mode": mode, "destinations": [] },
                "environment": { "allowlist": [] },
                "processes": { "children": "denied" },
                "secrets": { "handles": [] }
            },
            "hooks": [],
            "conformance": { "reportDigest": format!("sha256:{}", "44".repeat(32)), "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "status": "active",
            "revocation": null
        });
        ManifestDocument::from_value(json)
    }

    /// Regression (fix round 2, devin F-1): schema-invalid permission
    /// member values refuse at the manifest gate instead of installing
    /// past the escalation evaluator.
    #[test]
    fn schema_invalid_permission_values_refuse() {
        // network.mode "allowed" is not in the closed vocabulary.
        let error = manifest_with_network_mode("allowed").expect_err("invalid mode");
        assert!(matches!(error, PackageFailure::ManifestInvalid { .. }));
        // The closed values still parse.
        assert!(manifest_with_network_mode("denied").is_ok());
        assert!(manifest_with_network_mode("allowlist").is_ok());
    }

    #[test]
    fn invalid_env_and_child_values_refuse() {
        let base = manifest_with_network_mode("denied").expect("parses");
        let mut json = serde_json::from_slice::<serde_json::Value>(&base.canonical_bytes())
            .expect("canonical bytes are JSON");
        json["permissions"]["environment"]["allowlist"] = serde_json::json!(["not_Upper"]);
        json["integrity"]["packageDigest"] =
            serde_json::Value::String(format!("sha256:{}", "22".repeat(32)));
        let error = ManifestDocument::from_value(json).expect_err("bad env name");
        assert!(matches!(error, PackageFailure::ManifestInvalid { .. }));

        let mut json = serde_json::from_slice::<serde_json::Value>(&base.canonical_bytes())
            .expect("canonical bytes are JSON");
        json["permissions"]["processes"]["children"] = serde_json::json!("open");
        json["integrity"]["packageDigest"] =
            serde_json::Value::String(format!("sha256:{}", "22".repeat(32)));
        let error = ManifestDocument::from_value(json).expect_err("bad children");
        assert!(matches!(error, PackageFailure::ManifestInvalid { .. }));
    }
}

#[cfg(test)]
mod schema_parity_tests {
    use super::*;

    fn base_manifest() -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "parity-adapter", "name": "P", "version": "1.0.0" },
            "publisher": { "id": "p", "trustAnchor": "none" },
            "source": { "kind": "path", "coordinate": "path:x", "digest": format!("sha256:{}", "11".repeat(32)) },
            "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": { "protocolVersions": [crate::target_protocol::version::VERSION], "irVersions": [crate::ir::version::VERSION], "extensions": [] },
            "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
            "executable": { "runtime": { "kind": "node", "minVersion": "18.0.0" }, "entry": "a.mjs", "argvPreview": ["node", "a.mjs"], "assets": [] },
            "platforms": ["any"],
            "integrity": { "packageDigest": format!("sha256:{}", "22".repeat(32)), "files": [ { "path": "a.mjs", "digest": format!("sha256:{}", "33".repeat(32)), "bytes": 3 } ], "signaturePolicy": "unsigned", "signature": null },
            "permissions": {
                "filesystem": { "readScopes": [], "writeScopes": [] },
                "network": { "mode": "denied", "destinations": [] },
                "environment": { "allowlist": [] },
                "processes": { "children": "denied" },
                "secrets": { "handles": [] }
            },
            "hooks": [],
            "conformance": { "reportDigest": format!("sha256:{}", "44".repeat(32)), "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "status": "active",
            "revocation": null
        })
    }

    /// Regression (fix round 2, cline F-5 / devin F-13): `platforms` is
    /// schema-required and carries a closed vocabulary — the Rust wire
    /// enforces both, not just the JSON Schema gate.
    #[test]
    fn platforms_are_required_with_the_closed_vocabulary() {
        // Missing platforms refuses (schema lists it under `required`).
        let mut json = base_manifest();
        json.as_object_mut()
            .expect("object")
            .remove("platforms")
            .expect("present");
        let error = ManifestDocument::from_value(json).expect_err("missing platforms");
        assert!(matches!(error, PackageFailure::ManifestInvalid { .. }));

        // An out-of-vocabulary token refuses.
        let mut json = base_manifest();
        json["platforms"] = serde_json::json!(["any", "atari-2600"]);
        let error = ManifestDocument::from_value(json).expect_err("unknown platform");
        assert!(matches!(error, PackageFailure::ManifestInvalid { .. }));

        // Every schema enum token parses.
        for platform in [
            "any",
            "windows-x64",
            "linux-x64",
            "linux-arm64",
            "darwin-x64",
            "darwin-arm64",
        ] {
            let mut json = base_manifest();
            json["platforms"] = serde_json::json!([platform]);
            assert!(
                ManifestDocument::from_value(json).is_ok(),
                "{platform} must parse"
            );
        }
    }

    /// Regression (fix round 2, devin F-13): the self-referential digest
    /// member is absent from the wire, so both spellings refuse exactly
    /// like the JSON Schema (which omits the member). No spelling
    /// divergence: neither `manifestDigest` nor `manifest_digest` is
    /// silently accepted by one side and refused by the other.
    #[test]
    fn the_self_referential_digest_member_refuses_in_both_spellings() {
        for member in ["manifestDigest", "manifest_digest"] {
            let mut json = base_manifest();
            json[member] = serde_json::json!(format!("sha256:{}", "55".repeat(32)));
            let error = ManifestDocument::from_value(json)
                .expect_err("the self-referential digest member is not wire");
            assert!(
                matches!(error, PackageFailure::ManifestInvalid { .. }),
                "{member} must refuse"
            );
        }
    }

    /// Regression (fix round 2, cline F-5): the Rust package-path grammar
    /// refuses drive-like, control-character, and empty-segment spellings
    /// the old schema pattern tolerated; the wire is the stricter of the
    /// two and the schema now mirrors it.
    #[test]
    fn package_path_grammar_refuses_host_shaped_spellings() {
        for path in ["C:foo", "a//b", "a/", "dir/sub:/x.mjs", ".hidden"] {
            let mut json = base_manifest();
            json["integrity"]["files"][0]["path"] = serde_json::json!(path);
            let error = ManifestDocument::from_value(json).expect_err(path);
            assert!(
                matches!(error, PackageFailure::ManifestInvalid { .. }),
                "{path}"
            );
        }
    }
}

#[cfg(test)]
mod filesystem_scope_tests {
    use super::*;

    fn manifest_with_read_scope(scope: &str) -> Result<ManifestDocument, PackageFailure> {
        let json = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "scope-adapter", "name": "S", "version": "1.0.0" },
            "publisher": { "id": "p", "trustAnchor": "none" },
            "source": { "kind": "path", "coordinate": "path:x", "digest": format!("sha256:{}", "11".repeat(32)) },
            "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": { "protocolVersions": [crate::target_protocol::version::VERSION], "irVersions": [crate::ir::version::VERSION], "extensions": [] },
            "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
            "executable": { "entry": "a.mjs" },
            "platforms": ["any"],
            "integrity": { "packageDigest": format!("sha256:{}", "22".repeat(32)), "files": [ { "path": "a.mjs", "digest": format!("sha256:{}", "33".repeat(32)), "bytes": 3 } ], "signaturePolicy": "unsigned", "signature": null },
            "permissions": {
                "filesystem": { "readScopes": [scope], "writeScopes": [] },
                "network": { "mode": "denied", "destinations": [] },
                "environment": { "allowlist": [] },
                "processes": { "children": "denied" },
                "secrets": { "handles": [] }
            },
            "hooks": [],
            "conformance": { "reportDigest": format!("sha256:{}", "44".repeat(32)), "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "status": "active",
            "revocation": null
        });
        ManifestDocument::from_value(json)
    }

    /// Regression (fix round 4, devin N-2): the filesystem scope grammar
    /// actually runs at the Rust boundary. `../escape` must refuse — the
    /// schema refuses it, and the runtime gate must agree.
    #[test]
    fn traversal_scopes_refuse_at_the_rust_boundary() {
        for scope in ["../escape", "src/../../escape", "/absolute", "drive:C:x"] {
            let error = manifest_with_read_scope(scope)
                .expect_err("a traversal scope must refuse the manifest");
            assert!(
                matches!(error, PackageFailure::ManifestInvalid { .. }),
                "{scope} must refuse"
            );
        }
        // Honest scopes still parse, in both read and write positions.
        assert!(manifest_with_read_scope("src/**").is_ok());
        assert!(manifest_with_read_scope("generated/out/**").is_ok());
    }
}
