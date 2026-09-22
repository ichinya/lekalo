//! The capability snapshot bridge (issue #117).
//!
//! [`to_snapshot`] projects the declared capability map onto the #24
//! [`CapabilitySnapshot`] so the engine-neutral
//! `transaction_concurrency::map_capabilities` machinery can answer
//! requirement attachments against exact engine evidence. The bridge is
//! pure: `full` maps to full, `partial` to partial, `unsupported` to
//! unsupported, and absence stays `unknown` — never upgraded to yes.

use crate::transaction_concurrency::capability::{CapabilitySnapshot, SnapshotSupport};

use super::id::ProfileCapabilityId;
use super::StorageEngineProfile;

/// The profile-side support view: the closed three states plus the
/// explicit unknown of absence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProfileSupport {
    /// Fully present.
    Full,
    /// Present with documented, bounded gaps.
    Partial,
    /// Declared absent.
    Unsupported,
    /// Not declared by the profile: unknown, never yes.
    Unknown,
}

impl ProfileSupport {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }

    /// Resolve the support of one id against one profile.
    pub fn of(profile: &StorageEngineProfile, id: ProfileCapabilityId) -> Self {
        profile
            .capability(&id)
            .map(|capability| match capability.support() {
                super::Support::Full => Self::Full,
                super::Support::Partial => Self::Partial,
                super::Support::Unsupported => Self::Unsupported,
            })
            .unwrap_or(Self::Unknown)
    }
}

/// Project one profile's capability map onto a #24 snapshot. Every
/// declared id contributes its support state; the ids whose wire
/// spellings overlap the #24 vocabulary drive the transaction,
/// isolation, locking, concurrency, and idempotency verdicts.
pub fn to_snapshot(profile: &StorageEngineProfile) -> CapabilitySnapshot {
    let mut entries: Vec<(String, SnapshotSupport)> = Vec::new();
    for (id, capability) in profile.capabilities() {
        let support = match capability.support() {
            super::Support::Full => SnapshotSupport::Full,
            super::Support::Partial => SnapshotSupport::Partial,
            super::Support::Unsupported => SnapshotSupport::Unsupported,
        };
        entries.push((id.key().to_owned(), support));
    }
    CapabilitySnapshot::new(entries)
}

/// The declared support of one capability id, with absence surfaced as
/// `unknown` — never yes.
pub fn support_of(profile: &StorageEngineProfile, id: ProfileCapabilityId) -> ProfileSupport {
    ProfileSupport::of(profile, id)
}
