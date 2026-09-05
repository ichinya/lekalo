//! Namespaced adapter contributions (issue #18).
//!
//! Contributions are optional, namespaced, and non-authoritative: they may
//! append target-specific classes, reasons, and seeds to a profile
//! decision, but they can never mutate canonical change facts, stable IDs,
//! equality, or the baseline classes. Identity, digests, and trust are
//! validated at construction; contributions that reference unknown changes
//! or stale evidence are recorded with explicit degraded states instead of
//! being merged.

use super::change::Side;
use super::compatibility::{CompatibilityClass, ProfileId};
use super::identity::Subject;
use super::version::{MAX_ADAPTER_CONTRIBUTIONS, MAX_ADAPTER_EFFECTS, MAX_ADAPTER_SEEDS};

/// The closed trust state of one contribution.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum AdapterTrust {
    Verified,
    Stale,
    Unknown,
    Conflicting,
    Unsupported,
    Rejected,
}

impl AdapterTrust {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
            Self::Conflicting => "conflicting",
            Self::Unsupported => "unsupported",
            Self::Rejected => "rejected",
        }
    }

    /// Whether contributions of this trust may merge into decisions.
    pub(crate) const fn merges(self) -> bool {
        matches!(self, Self::Verified)
    }
}

/// One target-specific classification contributed for one change.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ContributionEffect {
    pub(crate) change_id: String,
    pub(crate) classes: Vec<CompatibilityClass>,
}

impl ContributionEffect {
    /// One effect referencing a change id, with classes restricted to the
    /// closed vocabulary and the `target-specific`/`unknown` tail.
    pub fn new(
        change_id: impl Into<String>,
        classes: Vec<CompatibilityClass>,
    ) -> Result<Self, super::AdapterError> {
        let change_id = change_id.into();
        if change_id.len() != 71 || !change_id.starts_with("sha256:") {
            return Err(super::AdapterError::ChangeReference);
        }
        let mut classes = classes;
        classes.sort();
        classes.dedup();
        if classes.is_empty() {
            return Err(super::AdapterError::EmptyClasses);
        }
        Ok(Self { change_id, classes })
    }

    /// The referenced change identity.
    pub fn change_id(&self) -> &str {
        &self.change_id
    }

    /// The contributed classes in dominance order.
    pub fn classes(&self) -> &[CompatibilityClass] {
        &self.classes
    }
}

/// One contributed target seed: a target-side subject the adapter reports
/// as affected. Seeds never enter the core seed set.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ContributionSeed {
    pub(crate) subject: Subject,
    pub(crate) side: Side,
}

impl ContributionSeed {
    /// One contributed seed for an explicit subject.
    pub fn new(subject: Subject, side: Side) -> Self {
        Self { subject, side }
    }

    /// The contributed subject.
    pub const fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The comparison side the seed belongs to.
    pub const fn side(&self) -> Side {
        self.side
    }
}

/// One validated adapter contribution input.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct AdapterInput {
    pub(crate) namespace: String,
    pub(crate) adapter_id: String,
    pub(crate) adapter_version: String,
    pub(crate) target_id: String,
    pub(crate) profile_ref: ProfileId,
    pub(crate) protocol_ref: String,
    pub(crate) input_ir_digest: String,
    pub(crate) evidence_revision: String,
    pub(crate) evidence_digest: String,
    pub(crate) trust: AdapterTrust,
    pub(crate) effects: Vec<ContributionEffect>,
    pub(crate) seeds: Vec<ContributionSeed>,
}

impl AdapterInput {
    /// Validate and assemble one contribution; every identity is
    /// grammar-checked and every bound enforced before the comparison
    /// runs.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        namespace: &str,
        adapter_id: &str,
        adapter_version: &str,
        target_id: &str,
        profile_ref: ProfileId,
        protocol_ref: &str,
        input_ir_digest: &str,
        evidence_revision: &str,
        evidence_digest: &str,
        trust: AdapterTrust,
        effects: Vec<ContributionEffect>,
        seeds: Vec<ContributionSeed>,
    ) -> Result<Self, super::AdapterError> {
        if !valid_namespace(namespace) {
            return Err(super::AdapterError::Namespace);
        }
        if adapter_id.is_empty() || adapter_id.len() > 512 || target_id.is_empty() {
            return Err(super::AdapterError::Identity);
        }
        if !valid_semver(adapter_version) {
            return Err(super::AdapterError::AdapterVersion);
        }
        if protocol_ref.is_empty() || protocol_ref.len() > 128 {
            return Err(super::AdapterError::ProtocolRef);
        }
        for digest in [input_ir_digest, evidence_digest] {
            if digest.len() != 71 || !digest.starts_with("sha256:") {
                return Err(super::AdapterError::Digest);
            }
        }
        if evidence_revision.is_empty() || evidence_revision.len() > 128 {
            return Err(super::AdapterError::EvidenceRevision);
        }
        if effects.len() > MAX_ADAPTER_EFFECTS || seeds.len() > MAX_ADAPTER_SEEDS {
            return Err(super::AdapterError::OverBound);
        }
        Ok(Self {
            namespace: namespace.to_owned(),
            adapter_id: adapter_id.to_owned(),
            adapter_version: adapter_version.to_owned(),
            target_id: target_id.to_owned(),
            profile_ref,
            protocol_ref: protocol_ref.to_owned(),
            input_ir_digest: input_ir_digest.to_owned(),
            evidence_revision: evidence_revision.to_owned(),
            evidence_digest: evidence_digest.to_owned(),
            trust,
            effects,
            seeds,
        })
    }

    /// The reverse-DNS namespace.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// The adapter identifier.
    pub fn adapter_id(&self) -> &str {
        &self.adapter_id
    }

    /// The exact adapter SemVer.
    pub fn adapter_version(&self) -> &str {
        &self.adapter_version
    }

    /// The target the evidence was captured for.
    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    /// The profile the contribution extends.
    pub const fn profile_ref(&self) -> ProfileId {
        self.profile_ref
    }

    /// The adapter protocol reference.
    pub fn protocol_ref(&self) -> &str {
        &self.protocol_ref
    }

    /// The IR digest the evidence was captured against.
    pub fn input_ir_digest(&self) -> &str {
        &self.input_ir_digest
    }

    /// The evidence revision string.
    pub fn evidence_revision(&self) -> &str {
        &self.evidence_revision
    }

    /// The evidence digest.
    pub fn evidence_digest(&self) -> &str {
        &self.evidence_digest
    }

    /// The declared trust state.
    pub const fn trust(&self) -> AdapterTrust {
        self.trust
    }

    /// The contributed effects.
    pub fn effects(&self) -> &[ContributionEffect] {
        &self.effects
    }

    /// The contributed seeds of the input (mirrored on the record).
    pub fn input_seeds(&self) -> &[ContributionSeed] {
        &self.seeds
    }
}

impl RecordedAdapter {
    /// The adapter identifier.
    pub fn adapter_id(&self) -> &str {
        &self.adapter_id
    }

    /// The declared trust state of the recorded contribution.
    pub const fn trust(&self) -> AdapterTrust {
        self.trust
    }

    /// The explicit degraded states of the recorded contribution.
    pub fn states(&self) -> &[String] {
        &self.states
    }

    /// The effects that survived reference validation.
    pub fn effects(&self) -> &[RecordedEffect] {
        &self.effects
    }

    /// The contributed seeds.
    pub fn seeds(&self) -> &[ContributionSeed] {
        &self.seeds
    }
}

/// Why one contribution envelope was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterError {
    Namespace,
    Identity,
    AdapterVersion,
    ProtocolRef,
    Digest,
    EvidenceRevision,
    OverBound,
    ChangeReference,
    EmptyClasses,
    TooMany,
}

/// Reverse-DNS namespace: two or more lowercase alphanumeric labels.
fn valid_namespace(namespace: &str) -> bool {
    let labels: Vec<&str> = namespace.split('.').collect();
    labels.len() >= 2
        && namespace.len() <= 128
        && labels.iter().all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// Exact `major.minor.patch` SemVer digits.
fn valid_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty() && part.len() <= 9 && part.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// Whether the request may accept one more contribution.
pub(crate) fn can_accept(count: usize) -> bool {
    count < MAX_ADAPTER_CONTRIBUTIONS
}

/// One effect merged (or recorded) from a contribution.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct RecordedEffect {
    pub(crate) change_id: String,
    pub(crate) classes: Vec<CompatibilityClass>,
}

/// One recorded contribution: the validated wire projection of an input
/// with its computed integrity digest and explicit degraded states.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct RecordedAdapter {
    pub(crate) namespace: String,
    pub(crate) adapter_id: String,
    pub(crate) adapter_version: String,
    pub(crate) target_id: String,
    pub(crate) profile_ref: ProfileId,
    pub(crate) protocol_ref: String,
    pub(crate) input_ir_digest: String,
    pub(crate) evidence_revision: String,
    pub(crate) evidence_digest: String,
    pub(crate) trust: AdapterTrust,
    pub(crate) contribution_digest: String,
    pub(crate) effects: Vec<RecordedEffect>,
    pub(crate) seeds: Vec<ContributionSeed>,
    pub(crate) states: Vec<String>,
}

impl RecordedAdapter {
    /// Project one validated input into its recorded wire form with the
    /// effects that survived reference validation and the explicit
    /// degraded states.
    pub(crate) fn from_input(
        input: &AdapterInput,
        effects: Vec<RecordedEffect>,
        mut states: Vec<String>,
        conflicting: bool,
        evidence_trust: AdapterTrust,
    ) -> Self {
        let trust = if conflicting {
            AdapterTrust::Conflicting
        } else {
            evidence_trust
        };
        let mut identity = String::new();
        identity.push_str(input.namespace());
        identity.push('\u{1f}');
        identity.push_str(input.adapter_id());
        identity.push('\u{1f}');
        identity.push_str(input.adapter_version());
        identity.push('\u{1f}');
        identity.push_str(input.evidence_digest());
        for effect in &effects {
            identity.push('\u{1f}');
            identity.push_str(&effect.change_id);
        }
        let contribution_digest = format!("sha256:{}", super::digest_hex(identity.as_bytes()));
        states.sort();
        states.dedup();
        Self {
            namespace: input.namespace.clone(),
            adapter_id: input.adapter_id.clone(),
            adapter_version: input.adapter_version.clone(),
            target_id: input.target_id.clone(),
            profile_ref: input.profile_ref,
            protocol_ref: input.protocol_ref.clone(),
            input_ir_digest: input.input_ir_digest.clone(),
            evidence_revision: input.evidence_revision.clone(),
            evidence_digest: input.evidence_digest.clone(),
            trust,
            contribution_digest,
            effects,
            seeds: input.input_seeds().to_vec(),
            states,
        }
    }

    /// Whether this recorded entry carries the same identity and content
    /// as one input (exact duplicates collapse).
    pub(crate) fn same_identity(&self, other: &AdapterInput) -> bool {
        self.namespace == other.namespace
            && self.adapter_id == other.adapter_id
            && self.adapter_version == other.adapter_version
            && self.target_id == other.target_id
            && self.evidence_digest == other.evidence_digest
            && self
                .effects
                .iter()
                .zip(other.effects())
                .all(|(recorded, input)| {
                    recorded.change_id == input.change_id()
                        && recorded.classes == input.classes().to_vec()
                })
            && self.effects.len() == other.effects().len()
    }

    /// Whether this recorded entry contradicts one input (same identity,
    /// different content).
    pub(crate) fn conflicts_with(&self, other: &AdapterInput) -> bool {
        let same_core = self.namespace == other.namespace
            && self.adapter_id == other.adapter_id
            && self.adapter_version == other.adapter_version
            && self.target_id == other.target_id
            && self.evidence_digest == other.evidence_digest;
        same_core && !self.same_identity(other)
    }
}
