//! Compatibility classification, built-in profiles, and migration hints
//! (issue #18).
//!
//! Compatibility is evaluated per explicit profile, never as one global
//! verdict. The core result carries profile-independent change facts and
//! the baseline class of every change; each requested built-in profile
//! maps those facts through its closed v1 policy into an independent
//! decision with exact rule outcomes. Profiles are conservative: absent,
//! stale, conflicting, or unsupported evidence classifies as `unknown` or
//! `degraded`, never as optimistic compatibility.

use super::change::{ChangeKind, ChangeRecord, Summary};
use super::version::{POLICY_REVISION, PROFILE_VERSION};

/// The closed compatibility classes, in deterministic dominance order
/// (`unknown` first, `additive` last). Every ordered class set renders in
/// this order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum CompatibilityClass {
    Unknown,
    DataLossRisk,
    StorageMigrationRequired,
    WireBreaking,
    SourceBreaking,
    Behavioral,
    TargetSpecific,
    Additive,
}

impl CompatibilityClass {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::DataLossRisk => "data-loss-risk",
            Self::StorageMigrationRequired => "storage-migration-required",
            Self::WireBreaking => "wire-breaking",
            Self::SourceBreaking => "source-breaking",
            Self::Behavioral => "behavioral",
            Self::TargetSpecific => "target-specific",
            Self::Additive => "additive",
        }
    }

    /// The class whose wire spelling equals `key`, or `None`.
    pub fn from_key(key: &str) -> Option<Self> {
        ALL_CLASSES.iter().copied().find(|class| class.key() == key)
    }

    /// Whether the class breaks some consumer dimension.
    pub(crate) const fn is_breaking(self) -> bool {
        matches!(
            self,
            Self::DataLossRisk
                | Self::StorageMigrationRequired
                | Self::WireBreaking
                | Self::SourceBreaking
        )
    }
}

/// Every class in dominance order.
pub const ALL_CLASSES: [CompatibilityClass; 8] = [
    CompatibilityClass::Unknown,
    CompatibilityClass::DataLossRisk,
    CompatibilityClass::StorageMigrationRequired,
    CompatibilityClass::WireBreaking,
    CompatibilityClass::SourceBreaking,
    CompatibilityClass::Behavioral,
    CompatibilityClass::TargetSpecific,
    CompatibilityClass::Additive,
];

/// Order and deduplicate a class set into dominance order.
pub(crate) fn ordered_classes(mut classes: Vec<CompatibilityClass>) -> Vec<CompatibilityClass> {
    classes.sort();
    classes.dedup();
    classes
}

/// The closed built-in profile vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ProfileId {
    SourceConsumer,
    WireConsumer,
    StorageConsumer,
    TargetConsumer,
    Advisory,
}

impl ProfileId {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::SourceConsumer => "source-consumer",
            Self::WireConsumer => "wire-consumer",
            Self::StorageConsumer => "storage-consumer",
            Self::TargetConsumer => "target-consumer",
            Self::Advisory => "advisory",
        }
    }

    /// The profile whose wire spelling equals `key`, or `None`.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "source-consumer" => Some(Self::SourceConsumer),
            "wire-consumer" => Some(Self::WireConsumer),
            "storage-consumer" => Some(Self::StorageConsumer),
            "target-consumer" => Some(Self::TargetConsumer),
            "advisory" => Some(Self::Advisory),
            _ => None,
        }
    }

    /// Whether unknown or degraded evidence blocks the verdict.
    pub(crate) const fn is_strict(self) -> bool {
        !matches!(self, Self::Advisory)
    }

    /// The dimension the profile consumes; `None` for the advisory
    /// overview, which reports every dimension without blocking.
    pub(crate) fn dimension(self) -> Option<Dimension> {
        match self {
            Self::SourceConsumer => Some(Dimension::Source),
            Self::WireConsumer => Some(Dimension::Wire),
            Self::StorageConsumer => Some(Dimension::Storage),
            Self::TargetConsumer => Some(Dimension::Target),
            Self::Advisory => None,
        }
    }
}

/// The closed verdict of one profile decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ProfileVerdict {
    Compatible,
    Breaking,
    Blocked,
    Degraded,
}

/// The closed consumer dimension of one profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Dimension {
    Source,
    Wire,
    Storage,
    Target,
}

impl ProfileVerdict {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Compatible => "compatible",
            Self::Breaking => "breaking",
            Self::Blocked => "blocked",
            Self::Degraded => "degraded",
        }
    }
}

/// One per-change rule outcome of one profile.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ProfileOutcome {
    pub(crate) change_id: String,
    pub(crate) classes: Vec<CompatibilityClass>,
    pub(crate) reasons: Vec<String>,
}

impl ProfileOutcome {
    /// The referenced change identity.
    pub fn change_id(&self) -> &str {
        &self.change_id
    }

    /// The classified classes in dominance order.
    pub fn classes(&self) -> &[CompatibilityClass] {
        &self.classes
    }

    /// The direct rule reasons in canonical order.
    pub fn reasons(&self) -> &[String] {
        &self.reasons
    }
}

/// Why a strict profile blocked, with the change identities it blocks on.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct BlockedOn {
    pub(crate) reason: String,
    pub(crate) change_ids: Vec<String>,
}

impl BlockedOn {
    /// The blocking reason identifier.
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The blocked change identities in canonical order.
    pub fn change_ids(&self) -> &[String] {
        &self.change_ids
    }
}

/// One independent profile decision.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ProfileDecision {
    pub(crate) profile_id: ProfileId,
    pub(crate) policy_digest: String,
    pub(crate) verdict: ProfileVerdict,
    pub(crate) classes: Vec<CompatibilityClass>,
    pub(crate) outcomes: Vec<ProfileOutcome>,
    pub(crate) blocked_on: Option<BlockedOn>,
}

impl ProfileDecision {
    /// The built-in profile this decision evaluates.
    pub const fn profile_id(&self) -> ProfileId {
        self.profile_id
    }

    /// The digest of the exact policy bytes this decision evaluated.
    pub fn policy_digest(&self) -> &str {
        &self.policy_digest
    }

    /// The closed verdict.
    pub const fn verdict(&self) -> ProfileVerdict {
        self.verdict
    }

    /// The union of outcome classes in dominance order.
    pub fn classes(&self) -> &[CompatibilityClass] {
        &self.classes
    }

    /// The per-change rule outcomes in canonical change order.
    pub fn outcomes(&self) -> &[ProfileOutcome] {
        &self.outcomes
    }

    /// Why a strict profile blocked, when it blocked.
    pub const fn blocked_on(&self) -> Option<&BlockedOn> {
        self.blocked_on.as_ref()
    }

    /// The profile vocabulary discriminator.
    pub const fn profile_version(&self) -> &'static str {
        PROFILE_VERSION
    }

    /// The evaluated policy revision.
    pub const fn policy_revision(&self) -> &'static str {
        POLICY_REVISION
    }
}

/// The baseline (profile-independent) class set of one change: a pure
/// function of the taxonomy kind, the summaries, and the record's own
/// reasons. History or evidence reasons escalate to `unknown`.
pub(crate) fn baseline(record: &ChangeRecord) -> Vec<CompatibilityClass> {
    let mut classes = baseline_kind(record.kind(), record);
    if record
        .reasons()
        .iter()
        .any(|reason| super::reason::is_evidence_reason(reason))
    {
        classes.push(CompatibilityClass::Unknown);
    }
    ordered_classes(classes)
}

/// The kind-only baseline class set.
fn baseline_kind(kind: ChangeKind, record: &ChangeRecord) -> Vec<CompatibilityClass> {
    use CompatibilityClass::{
        Additive, Behavioral, DataLossRisk, SourceBreaking, TargetSpecific, WireBreaking,
    };
    match kind {
        ChangeKind::SymbolAdded => vec![Additive],
        ChangeKind::SymbolRemoved => vec![SourceBreaking, DataLossRisk],
        ChangeKind::SymbolRenamed => vec![SourceBreaking],
        ChangeKind::SymbolReplaced | ChangeKind::SymbolTombstoned => {
            vec![SourceBreaking, DataLossRisk]
        }
        ChangeKind::SymbolKindChanged => vec![SourceBreaking, WireBreaking],
        ChangeKind::SymbolVisibilityChanged => match record.before() {
            Summary::Text(value) if value == "project" => vec![SourceBreaking],
            _ => vec![Behavioral],
        },
        ChangeKind::SymbolPortabilityChanged => vec![TargetSpecific],
        ChangeKind::SymbolVersionChanged
        | ChangeKind::SymbolDerivedFromChanged
        | ChangeKind::ModuleImportsChanged => vec![Behavioral],
        ChangeKind::FieldAdded | ChangeKind::SignatureInputAdded => match record.after() {
            Summary::Flag(true) => vec![SourceBreaking],
            _ => vec![Additive],
        },
        ChangeKind::FieldRemoved | ChangeKind::SignatureInputRemoved => {
            vec![SourceBreaking, DataLossRisk]
        }
        ChangeKind::FieldTypeChanged
        | ChangeKind::SignatureInputTypeChanged
        | ChangeKind::TypeShapeChanged
        | ChangeKind::SignatureOutputChanged => vec![SourceBreaking, WireBreaking, DataLossRisk],
        ChangeKind::FieldNullabilityChanged if tightened(record) => {
            vec![SourceBreaking, WireBreaking, DataLossRisk]
        }
        ChangeKind::FieldNullabilityChanged => vec![Behavioral],
        ChangeKind::SignatureInputPresenceChanged if tightened(record) => vec![SourceBreaking],
        ChangeKind::SignatureInputPresenceChanged => vec![Additive],
        ChangeKind::FieldPresenceChanged => match record.after() {
            Summary::Flag(true) => vec![SourceBreaking],
            _ => vec![Additive],
        },
        ChangeKind::TypeMemberAdded => vec![Behavioral],
        ChangeKind::TypeMemberRemoved => vec![SourceBreaking, DataLossRisk],
        ChangeKind::SignatureReadsChanged
        | ChangeKind::SignatureEffectsChanged
        | ChangeKind::EffectEntityChanged
        | ChangeKind::EffectEmitsChanged
        | ChangeKind::PolicyDecisionChanged
        | ChangeKind::PolicyScopeChanged
        | ChangeKind::EndpointInvokesChanged
        | ChangeKind::ScenarioSummaryChanged
        | ChangeKind::ScenarioCoversChanged => vec![Behavioral],
        ChangeKind::InvariantIdentityChanged => vec![SourceBreaking, DataLossRisk],
        ChangeKind::EffectOperationChanged => vec![Behavioral, DataLossRisk],
        ChangeKind::EndpointMethodChanged | ChangeKind::EndpointPathChanged => vec![WireBreaking],
        ChangeKind::BindingTargetChanged => vec![TargetSpecific],
    }
}

/// Whether a nullability or presence record tightened: the before value
/// was present/optional (`true`) and the after value is not.
fn tightened(record: &ChangeRecord) -> bool {
    matches!(
        (record.before(), record.after()),
        (Summary::Flag(true), Summary::Flag(false))
    )
}

/// The profile mapping of one baseline class set into the profile's
/// dimension. Source and wire dimensions translate each other's breaking
/// classes; storage and target dimensions require evidence and classify
/// `unknown` when it is absent.
pub(crate) fn evaluate_dimension(
    profile: ProfileId,
    record: &ChangeRecord,
    base: &[CompatibilityClass],
) -> (Vec<CompatibilityClass>, Vec<String>) {
    let has = |class: CompatibilityClass| base.contains(&class);
    let mut reasons = Vec::new();
    let classes = match profile.dimension() {
        Some(Dimension::Source) => {
            if has(CompatibilityClass::SourceBreaking) {
                base.iter()
                    .copied()
                    .filter(|class| !matches!(*class, CompatibilityClass::WireBreaking))
                    .collect::<Vec<_>>()
            } else if has(CompatibilityClass::WireBreaking) {
                reasons.push(super::reason::kind_reason(record.kind().key()));
                vec![CompatibilityClass::SourceBreaking]
            } else {
                base.to_vec()
            }
        }
        Some(Dimension::Wire) => {
            if has(CompatibilityClass::WireBreaking) {
                base.iter()
                    .copied()
                    .filter(|class| !matches!(*class, CompatibilityClass::SourceBreaking))
                    .collect::<Vec<_>>()
            } else if has(CompatibilityClass::SourceBreaking) {
                reasons.push(super::reason::kind_reason(record.kind().key()));
                vec![CompatibilityClass::WireBreaking]
            } else {
                base.to_vec()
            }
        }
        Some(Dimension::Storage) => {
            reasons.push(super::reason::STORAGE_EVIDENCE_ABSENT.to_owned());
            vec![CompatibilityClass::Unknown]
        }
        Some(Dimension::Target) => {
            if has(CompatibilityClass::TargetSpecific) {
                vec![CompatibilityClass::TargetSpecific]
            } else {
                reasons.push(super::reason::TARGET_EVIDENCE_ABSENT.to_owned());
                vec![CompatibilityClass::Unknown]
            }
        }
        None => base.to_vec(),
    };
    (classes, reasons)
}

/// One non-executable migration hint.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct MigrationHint {
    pub(crate) change_id: String,
    pub(crate) hint: &'static str,
    pub(crate) detail: String,
}

impl MigrationHint {
    /// The change identity the hint explains.
    pub fn change_id(&self) -> &str {
        &self.change_id
    }

    /// The closed hint vocabulary key.
    pub const fn hint(&self) -> &'static str {
        self.hint
    }

    /// The bounded, path-free detail (a stable id or subject key).
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// The closed migration-hint vocabulary.
pub mod hints {
    /// A live replacement exists for the removed id.
    pub const REPLACEMENT_AVAILABLE: &str = "replacement-available";
    /// A storage projection must be reviewed before migrating.
    pub const STORAGE_REVIEW: &str = "storage-review";
    /// A wire contract must be reviewed before publishing.
    pub const WIRE_REVIEW: &str = "wire-review";
}
