//! The closed vocabulary enums of the frozen #120 policy (exact wire
//! spellings).
//!
//! Every member is a closed, copyable token with the exact wire
//! spelling of the digest-pinned `privacy-policy.v0.3.2.json`
//! vocabularies (or, for evidence states, of the pinned
//! authorizing-evidence contract). Unknown values are unrepresentable:
//! construction is deny-unknown by type. The trusted-context loader
//! proves at load time that the embedded policy vocabularies equal
//! these spellings, so the typed surface and the trusted bytes cannot
//! drift apart.

/// Why one input member is rejected before any decision rule runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VocabularyError {
    /// The text is not a member of the closed vocabulary.
    Unknown,
}

/// The closed `dataSensitivity` vocabulary (nine independent labels).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DataSensitivity {
    /// `public`.
    Public,
    /// `internal`.
    Internal,
    /// `confidential`.
    Confidential,
    /// `personal-pii`.
    PersonalPii,
    /// `credential-secret`.
    CredentialSecret,
    /// `financial`.
    Financial,
    /// `health-special-category`.
    HealthSpecialCategory,
    /// `tenant-scoped`.
    TenantScoped,
    /// `retention-limited`.
    RetentionLimited,
}

impl DataSensitivity {
    /// Every label in policy vocabulary order.
    pub const ALL: [DataSensitivity; 9] = [
        DataSensitivity::Public,
        DataSensitivity::Internal,
        DataSensitivity::Confidential,
        DataSensitivity::PersonalPii,
        DataSensitivity::CredentialSecret,
        DataSensitivity::Financial,
        DataSensitivity::HealthSpecialCategory,
        DataSensitivity::TenantScoped,
        DataSensitivity::RetentionLimited,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "public" => Self::Public,
            "internal" => Self::Internal,
            "confidential" => Self::Confidential,
            "personal-pii" => Self::PersonalPii,
            "credential-secret" => Self::CredentialSecret,
            "financial" => Self::Financial,
            "health-special-category" => Self::HealthSpecialCategory,
            "tenant-scoped" => Self::TenantScoped,
            "retention-limited" => Self::RetentionLimited,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Confidential => "confidential",
            Self::PersonalPii => "personal-pii",
            Self::CredentialSecret => "credential-secret",
            Self::Financial => "financial",
            Self::HealthSpecialCategory => "health-special-category",
            Self::TenantScoped => "tenant-scoped",
            Self::RetentionLimited => "retention-limited",
        }
    }
}

/// The closed `exportDisposition` vocabulary (six dispositions).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportDisposition {
    /// `local-private`.
    LocalPrivate,
    /// `consumer-repository-only`.
    ConsumerRepositoryOnly,
    /// `shareable-with-redaction`.
    ShareableWithRedaction,
    /// `public-aggregate`.
    PublicAggregate,
    /// `public-fixture`.
    PublicFixture,
    /// `forbidden-to-export`.
    ForbiddenToExport,
}

impl ExportDisposition {
    /// Every disposition in policy vocabulary order.
    pub const ALL: [ExportDisposition; 6] = [
        ExportDisposition::LocalPrivate,
        ExportDisposition::ConsumerRepositoryOnly,
        ExportDisposition::ShareableWithRedaction,
        ExportDisposition::PublicAggregate,
        ExportDisposition::PublicFixture,
        ExportDisposition::ForbiddenToExport,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "local-private" => Self::LocalPrivate,
            "consumer-repository-only" => Self::ConsumerRepositoryOnly,
            "shareable-with-redaction" => Self::ShareableWithRedaction,
            "public-aggregate" => Self::PublicAggregate,
            "public-fixture" => Self::PublicFixture,
            "forbidden-to-export" => Self::ForbiddenToExport,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalPrivate => "local-private",
            Self::ConsumerRepositoryOnly => "consumer-repository-only",
            Self::ShareableWithRedaction => "shareable-with-redaction",
            Self::PublicAggregate => "public-aggregate",
            Self::PublicFixture => "public-fixture",
            Self::ForbiddenToExport => "forbidden-to-export",
        }
    }
}

/// The closed `operation` vocabulary (five operations).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationId {
    /// `local-use`.
    LocalUse,
    /// `repository-store`.
    RepositoryStore,
    /// `derive`.
    Derive,
    /// `transfer`.
    Transfer,
    /// `publish`.
    Publish,
}

impl OperationId {
    /// Every operation in policy vocabulary order.
    pub const ALL: [OperationId; 5] = [
        OperationId::LocalUse,
        OperationId::RepositoryStore,
        OperationId::Derive,
        OperationId::Transfer,
        OperationId::Publish,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "local-use" => Self::LocalUse,
            "repository-store" => Self::RepositoryStore,
            "derive" => Self::Derive,
            "transfer" => Self::Transfer,
            "publish" => Self::Publish,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalUse => "local-use",
            Self::RepositoryStore => "repository-store",
            Self::Derive => "derive",
            Self::Transfer => "transfer",
            Self::Publish => "publish",
        }
    }
}

/// The closed `repositoryRole` vocabulary (nine roles).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryRole {
    /// `local-workspace`.
    LocalWorkspace,
    /// `consumer-repository`.
    ConsumerRepository,
    /// `lekalo-repository`.
    LekaloRepository,
    /// `openspec-repository`.
    OpenspecRepository,
    /// `ai-factory-repository`.
    AiFactoryRepository,
    /// `hlv-repository`.
    HlvRepository,
    /// `source-native-repository`.
    SourceNativeRepository,
    /// `external-repository`.
    ExternalRepository,
    /// `public-channel`.
    PublicChannel,
}

impl RepositoryRole {
    /// Every role in policy vocabulary order.
    pub const ALL: [RepositoryRole; 9] = [
        RepositoryRole::LocalWorkspace,
        RepositoryRole::ConsumerRepository,
        RepositoryRole::LekaloRepository,
        RepositoryRole::OpenspecRepository,
        RepositoryRole::AiFactoryRepository,
        RepositoryRole::HlvRepository,
        RepositoryRole::SourceNativeRepository,
        RepositoryRole::ExternalRepository,
        RepositoryRole::PublicChannel,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "local-workspace" => Self::LocalWorkspace,
            "consumer-repository" => Self::ConsumerRepository,
            "lekalo-repository" => Self::LekaloRepository,
            "openspec-repository" => Self::OpenspecRepository,
            "ai-factory-repository" => Self::AiFactoryRepository,
            "hlv-repository" => Self::HlvRepository,
            "source-native-repository" => Self::SourceNativeRepository,
            "external-repository" => Self::ExternalRepository,
            "public-channel" => Self::PublicChannel,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalWorkspace => "local-workspace",
            Self::ConsumerRepository => "consumer-repository",
            Self::LekaloRepository => "lekalo-repository",
            Self::OpenspecRepository => "openspec-repository",
            Self::AiFactoryRepository => "ai-factory-repository",
            Self::HlvRepository => "hlv-repository",
            Self::SourceNativeRepository => "source-native-repository",
            Self::ExternalRepository => "external-repository",
            Self::PublicChannel => "public-channel",
        }
    }

    /// Whether the role is repository-backed: every role except the
    /// local workspace and the public channel must carry a non-null
    /// opaque repository ref.
    pub const fn is_repository_backed(self) -> bool {
        !matches!(self, Self::LocalWorkspace | Self::PublicChannel)
    }
}

/// The closed `trustBoundary` vocabulary (six boundaries).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustBoundary {
    /// `same-local-workspace`.
    SameLocalWorkspace,
    /// `same-repository`.
    SameRepository,
    /// `same-tenant`.
    SameTenant,
    /// `cross-repository`.
    CrossRepository,
    /// `cross-tenant`.
    CrossTenant,
    /// `public`.
    Public,
}

impl TrustBoundary {
    /// Every boundary in policy vocabulary order.
    pub const ALL: [TrustBoundary; 6] = [
        TrustBoundary::SameLocalWorkspace,
        TrustBoundary::SameRepository,
        TrustBoundary::SameTenant,
        TrustBoundary::CrossRepository,
        TrustBoundary::CrossTenant,
        TrustBoundary::Public,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "same-local-workspace" => Self::SameLocalWorkspace,
            "same-repository" => Self::SameRepository,
            "same-tenant" => Self::SameTenant,
            "cross-repository" => Self::CrossRepository,
            "cross-tenant" => Self::CrossTenant,
            "public" => Self::Public,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SameLocalWorkspace => "same-local-workspace",
            Self::SameRepository => "same-repository",
            Self::SameTenant => "same-tenant",
            Self::CrossRepository => "cross-repository",
            Self::CrossTenant => "cross-tenant",
            Self::Public => "public",
        }
    }
}

/// The closed `repositoryRelation` vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryRelation {
    /// `not-applicable`.
    NotApplicable,
    /// `same-origin`.
    SameOrigin,
    /// `different-repository`.
    DifferentRepository,
}

impl RepositoryRelation {
    /// Every relation in policy vocabulary order.
    pub const ALL: [RepositoryRelation; 3] = [
        RepositoryRelation::NotApplicable,
        RepositoryRelation::SameOrigin,
        RepositoryRelation::DifferentRepository,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "not-applicable" => Self::NotApplicable,
            "same-origin" => Self::SameOrigin,
            "different-repository" => Self::DifferentRepository,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotApplicable => "not-applicable",
            Self::SameOrigin => "same-origin",
            Self::DifferentRepository => "different-repository",
        }
    }
}

/// The closed `tenantRelation` vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TenantRelation {
    /// `same-tenant`.
    SameTenant,
    /// `cross-tenant`.
    CrossTenant,
    /// `not-applicable`.
    NotApplicable,
}

impl TenantRelation {
    /// Every relation in policy vocabulary order.
    pub const ALL: [TenantRelation; 3] = [
        TenantRelation::SameTenant,
        TenantRelation::CrossTenant,
        TenantRelation::NotApplicable,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "same-tenant" => Self::SameTenant,
            "cross-tenant" => Self::CrossTenant,
            "not-applicable" => Self::NotApplicable,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SameTenant => "same-tenant",
            Self::CrossTenant => "cross-tenant",
            Self::NotApplicable => "not-applicable",
        }
    }
}

/// The closed `audience` vocabulary (five audiences).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Audience {
    /// `operator-only`.
    OperatorOnly,
    /// `repository-collaborators`.
    RepositoryCollaborators,
    /// `tenant-members`.
    TenantMembers,
    /// `named-external`.
    NamedExternal,
    /// `public`.
    Public,
}

impl Audience {
    /// Every audience in policy vocabulary order.
    pub const ALL: [Audience; 5] = [
        Audience::OperatorOnly,
        Audience::RepositoryCollaborators,
        Audience::TenantMembers,
        Audience::NamedExternal,
        Audience::Public,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "operator-only" => Self::OperatorOnly,
            "repository-collaborators" => Self::RepositoryCollaborators,
            "tenant-members" => Self::TenantMembers,
            "named-external" => Self::NamedExternal,
            "public" => Self::Public,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OperatorOnly => "operator-only",
            Self::RepositoryCollaborators => "repository-collaborators",
            Self::TenantMembers => "tenant-members",
            Self::NamedExternal => "named-external",
            Self::Public => "public",
        }
    }
}

/// The closed state vocabulary shared by `resourcePath` and
/// `valueState` (`known`, `unknown`, `withheld`, `unsupported`).
/// Only `known` carries a value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SensitivityState {
    /// `known` — the only state that carries a value.
    Known,
    /// `unknown`.
    Unknown,
    /// `withheld`.
    Withheld,
    /// `unsupported`.
    Unsupported,
}

impl SensitivityState {
    /// Every state in policy vocabulary order.
    pub const ALL: [SensitivityState; 4] = [
        SensitivityState::Known,
        SensitivityState::Unknown,
        SensitivityState::Withheld,
        SensitivityState::Unsupported,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "known" => Self::Known,
            "unknown" => Self::Unknown,
            "withheld" => Self::Withheld,
            "unsupported" => Self::Unsupported,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Known => "known",
            Self::Unknown => "unknown",
            Self::Withheld => "withheld",
            Self::Unsupported => "unsupported",
        }
    }
}

/// The closed conflict-state vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictState {
    /// `none`.
    None,
    /// `resolved-allow`.
    ResolvedAllow,
    /// `resolved-deny`.
    ResolvedDeny,
    /// `unresolved`.
    Unresolved,
}

impl ConflictState {
    /// Every state in policy vocabulary order.
    pub const ALL: [ConflictState; 4] = [
        ConflictState::None,
        ConflictState::ResolvedAllow,
        ConflictState::ResolvedDeny,
        ConflictState::Unresolved,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "none" => Self::None,
            "resolved-allow" => Self::ResolvedAllow,
            "resolved-deny" => Self::ResolvedDeny,
            "unresolved" => Self::Unresolved,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ResolvedAllow => "resolved-allow",
            Self::ResolvedDeny => "resolved-deny",
            Self::Unresolved => "unresolved",
        }
    }
}

/// The closed constraint-scope vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstraintScope {
    /// `project`.
    Project,
    /// `profile`.
    Profile,
    /// `operation`.
    Operation,
}

impl ConstraintScope {
    /// Every scope in policy vocabulary order.
    pub const ALL: [ConstraintScope; 3] = [
        ConstraintScope::Project,
        ConstraintScope::Profile,
        ConstraintScope::Operation,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "project" => Self::Project,
            "profile" => Self::Profile,
            "operation" => Self::Operation,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Profile => "profile",
            Self::Operation => "operation",
        }
    }
}

/// The closed transform vocabulary (six transforms).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransformId {
    /// `redact-content`.
    RedactContent,
    /// `redact-secrets`.
    RedactSecrets,
    /// `redact-pii`.
    RedactPii,
    /// `aggregate-no-source-rows`.
    AggregateNoSourceRows,
    /// `replace-repository-identity`.
    ReplaceRepositoryIdentity,
    /// `normalize-project-relative-paths`.
    NormalizeProjectRelativePaths,
}

impl TransformId {
    /// Every transform in policy vocabulary order.
    pub const ALL: [TransformId; 6] = [
        TransformId::RedactContent,
        TransformId::RedactSecrets,
        TransformId::RedactPii,
        TransformId::AggregateNoSourceRows,
        TransformId::ReplaceRepositoryIdentity,
        TransformId::NormalizeProjectRelativePaths,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "redact-content" => Self::RedactContent,
            "redact-secrets" => Self::RedactSecrets,
            "redact-pii" => Self::RedactPii,
            "aggregate-no-source-rows" => Self::AggregateNoSourceRows,
            "replace-repository-identity" => Self::ReplaceRepositoryIdentity,
            "normalize-project-relative-paths" => Self::NormalizeProjectRelativePaths,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RedactContent => "redact-content",
            Self::RedactSecrets => "redact-secrets",
            Self::RedactPii => "redact-pii",
            Self::AggregateNoSourceRows => "aggregate-no-source-rows",
            Self::ReplaceRepositoryIdentity => "replace-repository-identity",
            Self::NormalizeProjectRelativePaths => "normalize-project-relative-paths",
        }
    }
}

/// The closed provenance-origin vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProvenanceOrigin {
    /// `synthetic`.
    Synthetic,
    /// `consumer-repository`.
    ConsumerRepository,
    /// `lekalo-repository`.
    LekaloRepository,
    /// `tool-runtime`.
    ToolRuntime,
    /// `derived`.
    Derived,
    /// `external`.
    External,
}

impl ProvenanceOrigin {
    /// Every origin in policy order (the closed checker enumeration).
    pub const ALL: [ProvenanceOrigin; 6] = [
        ProvenanceOrigin::Synthetic,
        ProvenanceOrigin::ConsumerRepository,
        ProvenanceOrigin::LekaloRepository,
        ProvenanceOrigin::ToolRuntime,
        ProvenanceOrigin::Derived,
        ProvenanceOrigin::External,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "synthetic" => Self::Synthetic,
            "consumer-repository" => Self::ConsumerRepository,
            "lekalo-repository" => Self::LekaloRepository,
            "tool-runtime" => Self::ToolRuntime,
            "derived" => Self::Derived,
            "external" => Self::External,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Synthetic => "synthetic",
            Self::ConsumerRepository => "consumer-repository",
            Self::LekaloRepository => "lekalo-repository",
            Self::ToolRuntime => "tool-runtime",
            Self::Derived => "derived",
            Self::External => "external",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every enum member round-trips through its exact wire spelling,
    /// and every spelling parses back to exactly one member.
    #[test]
    fn every_member_round_trips_its_wire_spelling() {
        for member in DataSensitivity::ALL {
            assert_eq!(DataSensitivity::parse(member.as_str()), Some(member));
        }
        for member in ExportDisposition::ALL {
            assert_eq!(ExportDisposition::parse(member.as_str()), Some(member));
        }
        for member in OperationId::ALL {
            assert_eq!(OperationId::parse(member.as_str()), Some(member));
        }
        for member in RepositoryRole::ALL {
            assert_eq!(RepositoryRole::parse(member.as_str()), Some(member));
        }
        for member in TrustBoundary::ALL {
            assert_eq!(TrustBoundary::parse(member.as_str()), Some(member));
        }
        for member in RepositoryRelation::ALL {
            assert_eq!(RepositoryRelation::parse(member.as_str()), Some(member));
        }
        for member in TenantRelation::ALL {
            assert_eq!(TenantRelation::parse(member.as_str()), Some(member));
        }
        for member in Audience::ALL {
            assert_eq!(Audience::parse(member.as_str()), Some(member));
        }
        for member in SensitivityState::ALL {
            assert_eq!(SensitivityState::parse(member.as_str()), Some(member));
        }
        for member in ConflictState::ALL {
            assert_eq!(ConflictState::parse(member.as_str()), Some(member));
        }
        for member in ConstraintScope::ALL {
            assert_eq!(ConstraintScope::parse(member.as_str()), Some(member));
        }
        for member in TransformId::ALL {
            assert_eq!(TransformId::parse(member.as_str()), Some(member));
        }
        for member in ProvenanceOrigin::ALL {
            assert_eq!(ProvenanceOrigin::parse(member.as_str()), Some(member));
        }
    }

    /// Unknown spellings are refused.
    #[test]
    fn unknown_spellings_are_refused() {
        assert_eq!(DataSensitivity::parse("personal"), None);
        assert_eq!(ExportDisposition::parse("shareable"), None);
        assert_eq!(OperationId::parse("export"), None);
        assert_eq!(RepositoryRole::parse(""), None);
        assert_eq!(TrustBoundary::parse("internet"), None);
        assert_eq!(SensitivityState::parse("KNOWN"), None);
        assert_eq!(TransformId::parse("redact"), None);
        assert_eq!(ProvenanceOrigin::parse("synthetic-"), None);
    }
}

/// Every closed vocabulary token serializes as its exact wire
/// spelling (a JSON string), never as a variant name.
macro_rules! wire_serialize {
    ($($enum:ty),* $(,)?) => {
        $(impl serde::Serialize for $enum {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        })*
    };
}

wire_serialize!(
    DataSensitivity,
    ExportDisposition,
    OperationId,
    RepositoryRole,
    TrustBoundary,
    RepositoryRelation,
    TenantRelation,
    Audience,
    SensitivityState,
    ConflictState,
    ConstraintScope,
    TransformId,
    ProvenanceOrigin,
);
