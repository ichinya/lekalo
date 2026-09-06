//! Target capability mapping: pure requirement-versus-snapshot
//! comparison (issue #24).
//!
//! #24 publishes typed requirement records and answers them against a
//! resolved target/profile snapshot supplied by the caller; it never
//! discovers adapters, resolves profiles, or negotiates. `unknown` is
//! never yes. The strict profile blocks on `unsupported`, `unknown`,
//! and unapproved `partial` support; the permissive profile degrades
//! explicitly and never passes. A `transaction: optional` operation is
//! never upgraded to a guarantee because a target happens to support
//! transactions.

use std::collections::BTreeMap;

use super::precondition::RequirementLevel;

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
                Some(existing) if *existing <= state => *existing,
                _ => state,
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
    pub(crate) requirement_id: String,
    pub(crate) capability: String,
    pub(crate) support: SnapshotSupport,
    pub(crate) blocked: bool,
    pub(crate) degraded: bool,
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
    pub(crate) profile: CapabilityProfile,
    pub(crate) blocked: bool,
    pub(crate) degraded: bool,
    pub(crate) verdicts: Vec<RequirementVerdict>,
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
    requirements: &[super::precondition::CapabilityRequirement],
    snapshot: &CapabilitySnapshot,
    profile: CapabilityProfile,
) -> CapabilityDecision {
    let mut verdicts = Vec::with_capacity(requirements.len());
    let mut blocked = false;
    let mut degraded = false;
    for requirement in requirements {
        let capability = requirement.capability().to_wire();
        let support = snapshot.support_of(&capability);
        let (requirement_blocked, requirement_degraded) = match profile {
            CapabilityProfile::Strict => match support {
                SnapshotSupport::Full => (false, false),
                SnapshotSupport::Partial => {
                    let approved = requirement.minimum() == RequirementLevel::Partial
                        && snapshot.partial_approved(&capability);
                    (true, !approved)
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
            requirement_id: requirement.requirement_id().as_str().to_owned(),
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
    use crate::scenario::id::NamespacedId;
    use crate::transaction_concurrency::precondition::{
        CapabilityId, CapabilityRequirement, RequirementLevel,
    };

    fn requirement(id: &str, capability: CapabilityId) -> CapabilityRequirement {
        CapabilityRequirement {
            requirement_id: NamespacedId::parse(id).expect("id"),
            capability,
            minimum: RequirementLevel::Full,
            reason: "owner-recorded reason".to_owned(),
        }
    }

    #[test]
    fn unknown_is_never_yes() {
        let requirements = [requirement(
            "planner.req/atomic",
            CapabilityId::TransactionAtomicGroup,
        )];
        let empty = CapabilitySnapshot::default();
        for profile in [CapabilityProfile::Strict, CapabilityProfile::Permissive] {
            let decision = map_capabilities(&requirements, &empty, profile);
            assert!(decision.degraded, "unknown must degrade");
            assert_eq!(
                decision.blocked,
                profile == CapabilityProfile::Strict,
                "unknown blocks only strict"
            );
            assert_eq!(decision.verdicts()[0].support, SnapshotSupport::Unknown);
        }
    }

    #[test]
    fn strict_blocks_unapproved_partial_and_permissive_degrades() {
        let requirements = [requirement(
            "planner.req/atomic",
            CapabilityId::TransactionAtomicGroup,
        )];
        let snapshot = CapabilitySnapshot::new([(
            "transaction.atomic_group".to_owned(),
            SnapshotSupport::Partial,
        )]);
        let strict = map_capabilities(&requirements, &snapshot, CapabilityProfile::Strict);
        assert!(strict.blocked);
        let approved = snapshot.clone().approve_partial("transaction.atomic_group");
        // The recorded requirement is minimum full, so approval never
        // rescues it: full requirements reject partial support.
        let strict_approved = map_capabilities(&requirements, &approved, CapabilityProfile::Strict);
        assert!(strict_approved.blocked);
        let permissive = map_capabilities(&requirements, &snapshot, CapabilityProfile::Permissive);
        assert!(!permissive.blocked && permissive.degraded);
    }

    #[test]
    fn full_support_passes_both_profiles() {
        let requirements = [requirement(
            "planner.req/atomic",
            CapabilityId::TransactionAtomicGroup,
        )];
        let snapshot = CapabilitySnapshot::new([(
            "transaction.atomic_group".to_owned(),
            SnapshotSupport::Full,
        )]);
        for profile in [CapabilityProfile::Strict, CapabilityProfile::Permissive] {
            let decision = map_capabilities(&requirements, &snapshot, profile);
            assert!(!decision.blocked && !decision.degraded);
        }
    }
}
