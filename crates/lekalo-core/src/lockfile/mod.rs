//! The committed `lekalo.lock`: closed wire, deterministic resolution,
//! verification, preview, and transactional update (issue #10).
//!
//! `lekalo.lock` is the canonical project input that freezes the exact
//! product, contract, adapter, generator, profile, and capability
//! identities that participated in a result. It is a physical regular file
//! at the accepted project home, serialized as strict canonical JSON (the
//! wire discriminator is `lekalo/lock/v1.0.0`, the schema identity is
//! `dev.lekalo.lock@1.0.0`), and it is never a runtime mutex, migration
//! journal, package cache, or release attestation.
//!
//! This module owns only the lock surface: the closed typed wire
//! ([`types`]), canonical serialization ([`canonical`]), strict parsing
//! ([`parse`]), the pure hermetic resolver ([`resolution`]), the verifier
//! with tamper detection ([`verify`]), and the preview/apply plan service
//! ([`plan`]). Candidate supply, adapter execution, profile semantics, and
//! package discovery belong to issues #27–#32; #91 owns the
//! `generate`/`verify --locked` orchestration and reuses
//! [`LockRequirement::Required`] from here.
//!
//! Product custody: this workspace carries prospective product
//! [`PRODUCT_VERSION`]; the lock schema version, resolver version, and
//! contract versions are independent of it by design.

pub mod canonical;
pub mod parse;
pub mod plan;
pub mod resolution;
pub mod types;
pub mod verify;

pub use verify::{LockRequirement, LockVerdict, RuntimeInventory};

use std::borrow::Cow;

use serde::Serialize;

pub use types::{
    CapabilityId, CatalogRef, ComponentId, ComponentRef, ContractPin, LockDigest, Lockfile,
    Platform, ProviderKind, ProviderRef, ResolvedAdapter, ResolvedCapability, ResolvedGenerator,
    ResolvedProfile, SemVer, Sha256Digest, SourceKind, SourceRef, Support,
};

/// The prospective product version this workspace carries (custody rule of
/// issue #10: 0.1.8 in every accepted path).
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Stable `#10` reason codes. The list is closed until issue #11 widens the
/// diagnostic schema.
pub mod reasons {
    /// The lock file does not exist but is required.
    pub const MISSING: &str = "lock.missing";
    /// The lock file is not a valid v1 wire document.
    pub const SCHEMA_INVALID: &str = "lock.schema-invalid";
    /// The lock file is semantically valid but not byte-canonical.
    pub const NONCANONICAL: &str = "lock.noncanonical";
    /// A cross-field, reference, sort, or duplicate invariant failed.
    pub const REFERENCE_INVALID: &str = "lock.reference-invalid";
    /// Two indistinguishable candidates remain for one identity.
    pub const RESOLUTION_AMBIGUOUS: &str = "lock.resolution-ambiguous";
    /// A profile has no exact version; 0.0.0 is never fabricated.
    pub const PROFILE_UNVERSIONED: &str = "lock.profile-unversioned";
    /// The lock no longer matches the project request or contracts.
    pub const STALE: &str = "lock.stale";
    /// A mutating update ran without `--dry-run` or `--apply`.
    pub const PREVIEW_REQUIRED: &str = "lock.preview-required";
    /// Project, catalog, or lock state changed between preview and apply.
    pub const SOURCE_CHANGED: &str = "lock.source-changed";
    /// Another update holds the runtime guard.
    pub const UPDATE_IN_PROGRESS: &str = "lock.update-in-progress";
    /// A staged write or replacement failed.
    pub const COMMIT_FAILED: &str = "lock.commit-failed";
    /// An ambiguous post-replace state demands re-reading before action.
    pub const RECOVERY_REQUIRED: &str = "lock.recovery-required";
    /// A physical path policy refusal inside the lock surface.
    pub const PATH_DENIED: &str = "lock.path-denied";
    /// A field carried a private path, URL, or credential shape.
    pub const PRIVATE_DATA_FORBIDDEN: &str = "lock.private-data-forbidden";
    /// A declared digest does not match the actual bytes in its domain.
    pub const DIGEST_MISMATCH: &str = "lock.digest-mismatch";
    /// A needed catalog snapshot is not available.
    pub const CATALOG_UNAVAILABLE: &str = "lock.catalog-unavailable";
    /// A locked component is not available locally.
    pub const COMPONENT_UNAVAILABLE: &str = "lock.component-unavailable";
    /// A locked platform artifact is not available locally.
    pub const PLATFORM_UNAVAILABLE: &str = "lock.platform-unavailable";
    /// The resolution provider seam cannot supply data.
    pub const RESOLUTION_PROVIDER_UNAVAILABLE: &str = "lock.resolution-provider-unavailable";
    /// The wire carries a future lock schema discriminator.
    pub const UNSUPPORTED_SCHEMA_VERSION: &str = "lock.unsupported-schema-version";
    /// A component or contract version is outside the accepted registry.
    pub const COMPONENT_VERSION_UNSUPPORTED: &str = "lock.component-version-unsupported";
}

/// Every deterministic refusal the lock surface produces.
///
/// The status, exit class, and stream are fixed per variant; nothing here
/// echoes raw operating-system messages, absolute paths, timestamps, or
/// environment data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LockFailure {
    /// The accepted loader refused the project; its terminal envelope is
    /// preserved verbatim (structure, encoding, version gates, recovery).
    Loader(crate::loader::LoadOutput),
    /// The lock file does not exist but is required (`exit 1`).
    Missing,
    /// The lock file is not a valid v1 wire document (`exit 1`).
    SchemaInvalid,
    /// The wire carries a future schema discriminator (`exit 5`).
    UnsupportedSchemaVersion {
        /// The exact rejected discriminator spelling.
        found: String,
    },
    /// Semantically valid but not byte-canonical (`exit 1`).
    Noncanonical,
    /// A cross-field or reference invariant failed (`exit 1`).
    ReferenceInvalid,
    /// Two indistinguishable candidates remain (`exit 1`).
    ResolutionAmbiguous {
        /// The contested identity.
        identity: String,
    },
    /// A profile has no exact version (`exit 1`).
    ProfileUnversioned {
        /// The refused profile id.
        id: String,
    },
    /// The lock does not match the current request (`exit 1`).
    Stale,
    /// A mutating update needs an explicit mode (`exit 1`).
    PreviewRequired,
    /// Preview/apply state drifted (`exit 1`, zero writes).
    SourceChanged,
    /// The runtime guard is held elsewhere (`exit 1`, zero writes).
    UpdateInProgress,
    /// A staged write or replacement failed (`exit 1`).
    CommitFailed,
    /// An ambiguous post-replace state (`exit 1`).
    RecoveryRequired,
    /// A physical path policy refusal (`exit 3`, stdout).
    PathDenied,
    /// A `structure.*` passthrough with the accepted classification.
    Structure {
        /// The stable structure code.
        code: String,
        /// Whether the accepted classification is a policy denial.
        denied: bool,
    },
    /// A private path, URL, or credential shape was refused (`exit 3`).
    PrivateData {
        /// The refused field's stable name.
        field: &'static str,
    },
    /// A declared digest differs from the actual bytes (`exit 3`).
    DigestMismatch {
        /// The stable digest-domain name.
        domain: &'static str,
        /// The contested component identity.
        identity: String,
    },
    /// A needed catalog snapshot is unavailable (`exit 4`).
    CatalogUnavailable,
    /// A locked component is unavailable (`exit 4`).
    ComponentUnavailable {
        /// The closed component kind (`adapter`, `generator`, `profile`).
        kind: &'static str,
        /// The missing component id.
        id: String,
    },
    /// A locked platform artifact is unavailable (`exit 4`).
    PlatformUnavailable {
        /// The adapter id.
        adapter: String,
        /// The unavailable platform.
        platform: String,
    },
    /// The provider seam cannot supply data (`exit 4`).
    ProviderUnavailable,
    /// A contract version is outside the accepted registry (`exit 5`).
    ComponentVersionUnsupported {
        /// The contract family (`model`, `ir`, `protocol`).
        family: &'static str,
        /// The refused version spelling.
        version: String,
    },
    /// The target protocol is unpublished (`exit 5`, reused `#9` code).
    ProtocolUnpublished,
    /// An adapter compatibility range refused the lock (`exit 5`).
    AdapterIncompatible {
        /// The refused adapter id.
        adapter: String,
    },
    /// A required IR extension is unavailable (`exit 5`).
    ExtensionIncompatible {
        /// The refused adapter id.
        adapter: String,
    },
}

impl LockFailure {
    /// The stable reason code; `None` for loader passthrough, which carries
    /// its own complete diagnostic envelope.
    pub fn reason_code(&self) -> Option<Cow<'static, str>> {
        match self {
            Self::Loader(_) => None,
            Self::Missing => Some(Cow::Borrowed(reasons::MISSING)),
            Self::SchemaInvalid => Some(Cow::Borrowed(reasons::SCHEMA_INVALID)),
            Self::UnsupportedSchemaVersion { .. } => {
                Some(Cow::Borrowed(reasons::UNSUPPORTED_SCHEMA_VERSION))
            }
            Self::Noncanonical => Some(Cow::Borrowed(reasons::NONCANONICAL)),
            Self::ReferenceInvalid => Some(Cow::Borrowed(reasons::REFERENCE_INVALID)),
            Self::ResolutionAmbiguous { .. } => Some(Cow::Borrowed(reasons::RESOLUTION_AMBIGUOUS)),
            Self::ProfileUnversioned { .. } => Some(Cow::Borrowed(reasons::PROFILE_UNVERSIONED)),
            Self::Stale => Some(Cow::Borrowed(reasons::STALE)),
            Self::PreviewRequired => Some(Cow::Borrowed(reasons::PREVIEW_REQUIRED)),
            Self::SourceChanged => Some(Cow::Borrowed(reasons::SOURCE_CHANGED)),
            Self::UpdateInProgress => Some(Cow::Borrowed(reasons::UPDATE_IN_PROGRESS)),
            Self::CommitFailed => Some(Cow::Borrowed(reasons::COMMIT_FAILED)),
            Self::RecoveryRequired => Some(Cow::Borrowed(reasons::RECOVERY_REQUIRED)),
            Self::PathDenied | Self::Structure { denied: true, .. } => {
                Some(Cow::Borrowed(reasons::PATH_DENIED))
            }
            Self::Structure { code, .. } => Some(Cow::Owned(code.clone())),
            Self::PrivateData { .. } => Some(Cow::Borrowed(reasons::PRIVATE_DATA_FORBIDDEN)),
            Self::DigestMismatch { .. } => Some(Cow::Borrowed(reasons::DIGEST_MISMATCH)),
            Self::CatalogUnavailable => Some(Cow::Borrowed(reasons::CATALOG_UNAVAILABLE)),
            Self::ComponentUnavailable { .. } => {
                Some(Cow::Borrowed(reasons::COMPONENT_UNAVAILABLE))
            }
            Self::PlatformUnavailable { .. } => Some(Cow::Borrowed(reasons::PLATFORM_UNAVAILABLE)),
            Self::ProviderUnavailable => {
                Some(Cow::Borrowed(reasons::RESOLUTION_PROVIDER_UNAVAILABLE))
            }
            Self::ComponentVersionUnsupported { .. } => {
                Some(Cow::Borrowed(reasons::COMPONENT_VERSION_UNSUPPORTED))
            }
            Self::ProtocolUnpublished => Some(Cow::Borrowed(
                crate::versioning::reasons::PROTOCOL_UNPUBLISHED,
            )),
            Self::AdapterIncompatible { .. } => Some(Cow::Borrowed(
                crate::versioning::reasons::ADAPTER_INCOMPATIBLE,
            )),
            Self::ExtensionIncompatible { .. } => Some(Cow::Borrowed(
                crate::versioning::reasons::EXTENSION_INCOMPATIBLE,
            )),
        }
    }

    /// The exit class on the accepted 0/1/3/4/5 envelope.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Loader(outcome) => outcome.status.exit_code(),
            Self::PathDenied
            | Self::Structure { denied: true, .. }
            | Self::PrivateData { .. }
            | Self::DigestMismatch { .. } => 3,
            Self::CatalogUnavailable
            | Self::ComponentUnavailable { .. }
            | Self::PlatformUnavailable { .. }
            | Self::ProviderUnavailable => 4,
            Self::UnsupportedSchemaVersion { .. }
            | Self::ComponentVersionUnsupported { .. }
            | Self::ProtocolUnpublished
            | Self::AdapterIncompatible { .. }
            | Self::ExtensionIncompatible { .. } => 5,
            _ => 1,
        }
    }

    /// The stable status spelling for the envelope.
    pub fn status(&self) -> &'static str {
        match self {
            Self::Loader(outcome) => outcome.status.as_str(),
            Self::PathDenied
            | Self::Structure { denied: true, .. }
            | Self::PrivateData { .. }
            | Self::DigestMismatch { .. } => "denied",
            Self::CatalogUnavailable
            | Self::ComponentUnavailable { .. }
            | Self::PlatformUnavailable { .. }
            | Self::ProviderUnavailable => "unavailable",
            Self::UnsupportedSchemaVersion { .. }
            | Self::ComponentVersionUnsupported { .. }
            | Self::ProtocolUnpublished
            | Self::AdapterIncompatible { .. }
            | Self::ExtensionIncompatible { .. } => "unsupported-version",
            _ => "invalid",
        }
    }

    /// Whether the failure payload belongs on stderr.
    pub fn writes_stderr(&self) -> bool {
        match self {
            Self::Loader(outcome) => outcome.status.writes_stderr(),
            Self::PathDenied
            | Self::Structure { .. }
            | Self::PrivateData { .. }
            | Self::DigestMismatch { .. }
            | Self::CatalogUnavailable
            | Self::ComponentUnavailable { .. }
            | Self::PlatformUnavailable { .. }
            | Self::ProviderUnavailable => false,
            _ => true,
        }
    }
}

/// The terminal result of one lock or update operation: exact JSON envelope
/// bytes, the stable human line, and the exit class.
pub struct LockOutcome {
    /// The stable status spelling (`valid`, `invalid`, `denied`,
    /// `unavailable`, `unsupported-version`, or a loader status).
    pub status: String,
    /// The exit code on the accepted 0/1/3/4/5 envelope.
    pub exit_code: u8,
    /// Whether the payload belongs on stderr.
    pub writes_stderr: bool,
    /// The exact JSON envelope (pretty, two-space, one trailing LF).
    pub json: String,
    /// The single stable human line (with trailing LF).
    pub human: String,
}

impl LockOutcome {
    /// Render a failure deterministically.
    pub fn failure(failure: &LockFailure) -> Self {
        if let LockFailure::Loader(outcome) = failure {
            return Self {
                status: outcome.status.as_str().to_owned(),
                exit_code: outcome.status.exit_code(),
                writes_stderr: outcome.status.writes_stderr(),
                json: format!("{}\n", outcome.json.trim_end_matches('\n')),
                human: format!("{}\n", outcome.human),
            };
        }
        let mut envelope = format!(
            "{{\n  \"status\": \"{}\",\n  \"reasonCodes\": [\n    \"{}\"\n  ]\n}}",
            failure.status(),
            failure
                .reason_code()
                .expect("fixed-code failures always carry a reason")
        );
        let mut human = format!(
            "{}: {}",
            failure.status(),
            failure
                .reason_code()
                .expect("fixed-code failures always carry a reason")
        );
        if let LockFailure::Structure {
            code,
            denied: false,
        } = failure
        {
            envelope = format!(
                "{{\n  \"status\": \"invalid\",\n  \"reasonCodes\": [\n    \"{code}\"\n  ]\n}}"
            );
            human = format!("invalid: {code}");
        }
        envelope.push('\n');
        human.push('\n');
        Self {
            status: failure.status().to_owned(),
            exit_code: failure.exit_code(),
            writes_stderr: failure.writes_stderr(),
            json: envelope,
            human,
        }
    }

    /// Render any closed success value on stdout.
    pub fn success<T: Serialize>(value: &T, human: String) -> Self {
        let json = format!(
            "{}\n",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
        Self {
            status: "valid".to_owned(),
            exit_code: 0,
            writes_stderr: false,
            json,
            human,
        }
    }
}

/// The component counts of one lock receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ComponentCounts {
    /// Selected adapters.
    pub adapters: usize,
    /// Selected generators.
    pub generators: usize,
    /// Selected profiles.
    pub profiles: usize,
    /// Resolved capability entries.
    pub capabilities: usize,
}

impl ComponentCounts {
    pub(crate) fn of(lock: &Lockfile) -> Self {
        Self {
            adapters: lock.adapters().len(),
            generators: lock.generators().len(),
            profiles: lock.profiles().len(),
            capabilities: lock.capabilities().len(),
        }
    }
}

/// One observed lock-file state at a validated project root.
#[derive(Clone, Debug)]
pub enum LockState {
    /// No lock file exists.
    Absent,
    /// A valid, canonical lock exists.
    Present(Box<Lockfile>),
}

impl LockState {
    /// The present lock, or the typed `lock.missing` refusal.
    pub fn present(&self) -> Result<&Lockfile, LockFailure> {
        match self {
            Self::Present(lock) => Ok(lock),
            Self::Absent => Err(LockFailure::Missing),
        }
    }

    /// The present lock, if any.
    pub fn as_lock(&self) -> Option<&Lockfile> {
        match self {
            Self::Present(lock) => Some(lock),
            Self::Absent => None,
        }
    }
}

/// The closed success wire object of `lekalo lock` (create or check).
/// Field order is normative.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LockReceipt {
    /// Always `valid`.
    pub status: &'static str,
    /// Always `lock`.
    pub operation: &'static str,
    /// `create` or `check`.
    pub mode: &'static str,
    /// Whether this operation wrote a lock file.
    pub changed: bool,
    /// The digest of the lock payload this operation observed or wrote.
    #[serde(rename = "lockDigest")]
    pub lock_digest: String,
    /// The resolver algorithm version of the observed or written lock.
    #[serde(rename = "resolverVersion")]
    pub resolver_version: String,
    /// Component counts of the observed or written lock.
    pub counts: ComponentCounts,
}

/// One version-plus-digest pair in a change entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VersionDigest {
    /// The component version.
    pub version: String,
    /// The component digest in its domain.
    pub digest: String,
}

/// One structured update change. `from` is `null` for additions, `to` is
/// `null` for removals.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChangeEntry {
    /// The closed component kind (`adapter`, `generator`, `profile`).
    pub kind: &'static str,
    /// The component id.
    pub id: String,
    /// The before identity, absent for additions.
    pub from: Option<VersionDigest>,
    /// The after identity, absent for removals.
    pub to: Option<VersionDigest>,
}

/// The closed success wire object of `lekalo update` (dry-run or apply).
/// Field order is normative.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UpdateReceipt {
    /// Always `valid`.
    pub status: &'static str,
    /// Always `update`.
    pub operation: &'static str,
    /// `dry-run` or `apply`.
    pub mode: &'static str,
    /// Whether any bytes changed (dry-run reports the planned change).
    pub changed: bool,
    /// The digest of the resulting lock payload.
    #[serde(rename = "lockDigest")]
    pub lock_digest: String,
    /// The resolver algorithm version.
    #[serde(rename = "resolverVersion")]
    pub resolver_version: String,
    /// The deterministic plan identity.
    #[serde(rename = "planId")]
    pub plan_id: String,
    /// The before digest, `null` when no lock exists.
    #[serde(rename = "beforeDigest")]
    pub before_digest: Option<String>,
    /// The after digest.
    #[serde(rename = "afterDigest")]
    pub after_digest: String,
    /// The sorted structured changes.
    pub changes: Vec<ChangeEntry>,
}
