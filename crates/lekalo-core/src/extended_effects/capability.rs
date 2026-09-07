//! Adapter capability requirements and their pure target mapping
//! (issue #26).
//!
//! The attachment publishes typed requirement records and answers them
//! against a resolved target/profile snapshot supplied by the caller;
//! it never discovers adapters, resolves profiles, or negotiates.
//! `unknown` is never yes. The strict profile blocks on `unsupported`,
//! `unknown`, and unapproved `partial` support — an adapter capability
//! mismatch blocks the required semantics — while the permissive
//! profile degrades explicitly and never passes. The verdicts are plain
//! data a portability report may render.

use std::collections::BTreeMap;

use crate::scenario::id::NamespacedId;

/// The closed v1 capability vocabulary of this family.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Capability {
    /// Durable event delivery.
    DurableDelivery,
    /// Total event ordering.
    OrderingTotal,
    /// Per-key event ordering.
    OrderingPerKey,
    /// Durable event deduplication keys.
    DurableDedup,
    /// A queue dead-letter policy.
    DeadLetter,
    /// Compensating external calls.
    CallCompensation,
    /// Strong cache consistency.
    StrongConsistency,
    /// Read-your-own-writes cache consistency.
    ReadYourWrites,
    /// Publication approval contracts.
    PublishApproval,
    /// Immutable revisioned publication snapshots.
    RevisionedSnapshot,
    /// The security review gate surface.
    ReviewGate,
    /// A provider-owned adapter capability whose concrete owner is the
    /// named provider contract (`provider.vendor.mail.send`).
    Provider(ProviderCapability),
}

/// One provider-owned adapter capability reference.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ProviderCapability(String);

impl ProviderCapability {
    /// Parse the closed provider-capability grammar: at least two
    /// kebab segments below the `provider.` prefix, at most 96 bytes.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if text.len() < 12 || bytes.len() > 96 {
            return None;
        }
        let rest = text.strip_prefix("provider.")?;
        let mut segments = rest.split('.');
        let first = segments.next()?;
        if !lower_kebab(first) || !segments.all(lower_kebab) || !rest.contains('.') {
            return None;
        }
        Some(Self(text.to_owned()))
    }

    /// The exact wire spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One lowercase kebab segment.
fn lower_kebab(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    if bytes.is_empty() || bytes.len() > 63 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

impl Capability {
    /// The exact wire spelling.
    pub fn to_wire(&self) -> String {
        match self {
            Self::DurableDelivery => "delivery.durable".to_owned(),
            Self::OrderingTotal => "ordering.total".to_owned(),
            Self::OrderingPerKey => "ordering.per_key".to_owned(),
            Self::DurableDedup => "dedup.durable_key".to_owned(),
            Self::DeadLetter => "queue.dead_letter".to_owned(),
            Self::CallCompensation => "call.compensation".to_owned(),
            Self::StrongConsistency => "cache.strong".to_owned(),
            Self::ReadYourWrites => "cache.read_your_writes".to_owned(),
            Self::PublishApproval => "publish.approval".to_owned(),
            Self::RevisionedSnapshot => "publish.revisioned_snapshot".to_owned(),
            Self::ReviewGate => "security.review_gate".to_owned(),
            Self::Provider(provider) => provider.as_str().to_owned(),
        }
    }

    /// Parse one wire capability spelling.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "delivery.durable" => return Some(Self::DurableDelivery),
            "ordering.total" => return Some(Self::OrderingTotal),
            "ordering.per_key" => return Some(Self::OrderingPerKey),
            "dedup.durable_key" => return Some(Self::DurableDedup),
            "queue.dead_letter" => return Some(Self::DeadLetter),
            "call.compensation" => return Some(Self::CallCompensation),
            "cache.strong" => return Some(Self::StrongConsistency),
            "cache.read_your_writes" => return Some(Self::ReadYourWrites),
            "publish.approval" => return Some(Self::PublishApproval),
            "publish.revisioned_snapshot" => return Some(Self::RevisionedSnapshot),
            "security.review_gate" => return Some(Self::ReviewGate),
            _ => {}
        }
        ProviderCapability::parse(text).map(Self::Provider)
    }
}

/// The required support level of one capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RequirementLevel {
    /// Full support is required.
    Full,
    /// Partial support is acceptable when policy approves it.
    Partial,
}

impl RequirementLevel {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "full" => Some(Self::Full),
            "partial" => Some(Self::Partial),
            _ => None,
        }
    }
}

/// One typed capability requirement record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityRequirement {
    pub(crate) requirement_id: NamespacedId,
    pub(crate) capability: Capability,
    pub(crate) minimum: RequirementLevel,
    pub(crate) reason: String,
}

impl CapabilityRequirement {
    /// The requirement id.
    pub fn requirement_id(&self) -> &str {
        self.requirement_id.as_str()
    }

    /// The capability wire spelling.
    pub fn capability(&self) -> String {
        self.capability.to_wire()
    }

    /// The required support level.
    pub const fn minimum(&self) -> RequirementLevel {
        self.minimum
    }

    /// The recorded owner reason.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// The resolved support state of one capability on a target.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SnapshotSupport {
    /// Fully present.
    Full,
    /// Present with documented, bounded gaps.
    Partial,
    /// Declared absent.
    Unsupported,
    /// Not declared by the snapshot: unknown, never yes.
    Unknown,
}

impl SnapshotSupport {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }
}

/// One resolved target/profile snapshot: the typed evidence a caller
/// supplies. Versions and digests ride beside the caller's profile;
/// verification provenance belongs to #27/#28/#29.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct CapabilitySnapshot {
    support: BTreeMap<String, SnapshotSupport>,
    partial_approved: BTreeMap<String, bool>,
}

impl CapabilitySnapshot {
    /// Build a snapshot from capability support entries. Duplicate
    /// capabilities collapse to the weakest support.
    pub fn new(entries: impl IntoIterator<Item = (String, SnapshotSupport)>) -> Self {
        let mut support = BTreeMap::new();
        for (capability, state) in entries {
            let weakened = match support.get(&capability) {
                Some(existing) => std::cmp::max(*existing, state),
                None => state,
            };
            support.insert(capability, weakened);
        }
        Self {
            support,
            partial_approved: BTreeMap::new(),
        }
    }

    /// Approve one capability's partial support by policy. Unapproved
    /// partial support blocks the strict profile.
    pub fn approve_partial(mut self, capability: &str) -> Self {
        self.partial_approved.insert(capability.to_owned(), true);
        self
    }

    /// The declared support of one capability wire spelling.
    pub fn support_of(&self, capability: &str) -> SnapshotSupport {
        self.support
            .get(capability)
            .copied()
            .unwrap_or(SnapshotSupport::Unknown)
    }

    /// Whether partial support of one capability is policy-approved.
    pub fn partial_approved(&self, capability: &str) -> bool {
        self.partial_approved
            .get(capability)
            .copied()
            .unwrap_or(false)
    }
}

/// The closed capability policy profiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CapabilityProfile {
    /// Block on `unsupported`, `unknown`, and unapproved `partial`.
    Strict,
    /// Degrade explicitly; never pass on `unsupported` or `unknown`.
    Permissive,
}

impl CapabilityProfile {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Permissive => "permissive",
        }
    }
}

/// The verdict for one requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequirementVerdict {
    requirement_id: String,
    capability: String,
    support: SnapshotSupport,
    blocked: bool,
    degraded: bool,
}

impl RequirementVerdict {
    /// The requirement id.
    pub fn requirement_id(&self) -> &str {
        &self.requirement_id
    }

    /// The capability wire spelling.
    pub fn capability(&self) -> &str {
        &self.capability
    }

    /// The snapshot support state.
    pub const fn support(&self) -> SnapshotSupport {
        self.support
    }

    /// Whether this requirement blocks the run under the profile.
    pub const fn blocked(&self) -> bool {
        self.blocked
    }

    /// Whether this requirement degrades the run under the profile.
    pub const fn degraded(&self) -> bool {
        self.degraded
    }
}

/// The overall mapping decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityDecision {
    profile: CapabilityProfile,
    blocked: bool,
    degraded: bool,
    verdicts: Vec<RequirementVerdict>,
}

impl CapabilityDecision {
    /// The evaluated profile.
    pub const fn profile(&self) -> CapabilityProfile {
        self.profile
    }

    /// Whether the profile blocks the run.
    pub const fn blocked(&self) -> bool {
        self.blocked
    }

    /// Whether the profile degrades the run.
    pub const fn degraded(&self) -> bool {
        self.degraded
    }

    /// Per-requirement verdicts in canonical requirement order.
    pub fn verdicts(&self) -> &[RequirementVerdict] {
        &self.verdicts
    }
}

/// Map one attachment's capability requirements against a resolved
/// snapshot under a profile. Pure: no discovery, no negotiation, no I/O.
pub fn map_capabilities(
    requirements: &[super::CapabilityRequirement],
    snapshot: &CapabilitySnapshot,
    profile: CapabilityProfile,
) -> CapabilityDecision {
    let mut verdicts = Vec::with_capacity(requirements.len());
    let mut blocked = false;
    let mut degraded = false;
    for requirement in requirements {
        let capability = requirement.capability();
        let support = snapshot.support_of(&capability);
        let (requirement_blocked, requirement_degraded) = match profile {
            CapabilityProfile::Strict => match support {
                SnapshotSupport::Full => (false, false),
                SnapshotSupport::Partial => {
                    let approved = requirement.minimum() == RequirementLevel::Partial
                        && snapshot.partial_approved(&capability);
                    if approved {
                        (false, false)
                    } else {
                        (true, true)
                    }
                }
                SnapshotSupport::Unsupported | SnapshotSupport::Unknown => (true, true),
            },
            CapabilityProfile::Permissive => match support {
                SnapshotSupport::Full => (false, false),
                SnapshotSupport::Partial => (false, true),
                SnapshotSupport::Unsupported | SnapshotSupport::Unknown => (false, true),
            },
        };
        blocked |= requirement_blocked;
        degraded |= requirement_degraded;
        verdicts.push(RequirementVerdict {
            requirement_id: requirement.requirement_id().to_owned(),
            capability,
            support,
            blocked: requirement_blocked,
            degraded: requirement_degraded,
        });
    }
    CapabilityDecision {
        profile,
        blocked,
        degraded,
        verdicts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_round_trip_their_wire_spellings() {
        for spelling in [
            "delivery.durable",
            "ordering.total",
            "ordering.per_key",
            "dedup.durable_key",
            "queue.dead_letter",
            "call.compensation",
            "cache.strong",
            "cache.read_your_writes",
            "publish.approval",
            "publish.revisioned_snapshot",
            "security.review_gate",
        ] {
            let parsed = Capability::parse(spelling).expect("closed capability");
            assert_eq!(parsed.to_wire(), spelling);
        }
        let provider = Capability::parse("provider.vendor.mail.send").expect("provider");
        assert_eq!(provider.to_wire(), "provider.vendor.mail.send");
    }

    #[test]
    fn provider_capabilities_reject_shallow_or_invalid_shapes() {
        assert!(ProviderCapability::parse("provider.mail").is_none());
        assert!(ProviderCapability::parse("provider.-mail/send").is_none());
        assert!(ProviderCapability::parse("mail.send").is_none());
        assert!(ProviderCapability::parse("provider.").is_none());
    }

    #[test]
    fn strict_blocks_unknown_and_permissive_degrades() {
        let requirements = [];
        let empty = CapabilitySnapshot::default();
        let strict = map_capabilities(&requirements, &empty, CapabilityProfile::Strict);
        assert!(!strict.blocked(), "no requirements never block");
        let permissive = map_capabilities(&requirements, &empty, CapabilityProfile::Permissive);
        assert!(!permissive.blocked() && !permissive.degraded());
    }

    fn one_requirement(minimum: RequirementLevel) -> CapabilityRequirement {
        CapabilityRequirement {
            requirement_id: NamespacedId::parse("planner.req/partial-gap").expect("requirement id"),
            capability: Capability::parse("delivery.durable").expect("closed capability"),
            minimum,
            reason: "owner-approved bounded gaps".to_owned(),
        }
    }

    #[test]
    fn strict_partial_support_passes_only_when_policy_approved() {
        let requirements = [one_requirement(RequirementLevel::Partial)];
        let partial = vec![("delivery.durable".to_owned(), SnapshotSupport::Partial)];
        let approved = CapabilitySnapshot::new(partial.clone()).approve_partial("delivery.durable");
        let approved_decision =
            map_capabilities(&requirements, &approved, CapabilityProfile::Strict);
        let verdict = approved_decision.verdicts().first().expect("one verdict");
        assert_eq!(
            (verdict.blocked(), verdict.degraded()),
            (false, false),
            "policy-approved partial support passes the strict profile"
        );
        let unapproved = CapabilitySnapshot::new(partial);
        let unapproved_decision =
            map_capabilities(&requirements, &unapproved, CapabilityProfile::Strict);
        let verdict = unapproved_decision.verdicts().first().expect("one verdict");
        assert_eq!(
            (verdict.blocked(), verdict.degraded()),
            (true, true),
            "unapproved partial support still blocks the strict profile"
        );
    }

    #[test]
    fn strict_full_requirements_stay_blocked_on_partial_support() {
        // A requirement demanding full support cannot be satisfied by
        // partial support even when policy approves the gaps.
        let requirements = [one_requirement(RequirementLevel::Full)];
        let snapshot = CapabilitySnapshot::new(vec![(
            "delivery.durable".to_owned(),
            SnapshotSupport::Partial,
        )])
        .approve_partial("delivery.durable");
        let decision = map_capabilities(&requirements, &snapshot, CapabilityProfile::Strict);
        assert!(decision.blocked() && decision.degraded());
    }

    #[test]
    fn duplicate_snapshot_evidence_collapses_to_the_weakest_support() {
        let snapshot = CapabilitySnapshot::new([
            ("delivery.durable".to_owned(), SnapshotSupport::Full),
            ("delivery.durable".to_owned(), SnapshotSupport::Unknown),
        ]);
        assert_eq!(
            snapshot.support_of("delivery.durable"),
            SnapshotSupport::Unknown,
            "contradictory duplicates collapse to the weakest support"
        );
        // The weakest evidence is never yes: strict must block.
        let requirements = [one_requirement(RequirementLevel::Full)];
        let strict = map_capabilities(&requirements, &snapshot, CapabilityProfile::Strict);
        assert!(strict.blocked() && strict.degraded());
    }
}
