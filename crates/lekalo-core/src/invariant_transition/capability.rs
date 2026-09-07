//! Target capability requirement records of the invariant-transition
//! attachment (issue #63).
//!
//! The closed v1 vocabulary names the state-machine and invariant
//! guarantees whose support a target must declare; provider-owned
//! adapter capabilities keep the accepted #14 provider grammar.
//! Requirement records are plain data for the capability and profile
//! owners (#27/#28/#29); this module never evaluates support.

use crate::scenario::id::NamespacedId;

/// Why one capability requirement declaration is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityError {
    /// The capability or minimum key is outside the closed vocabulary.
    Shape,
}

/// One declared target capability requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityRequirement {
    pub(crate) requirement_id: NamespacedId,
    pub(crate) capability: String,
    pub(crate) minimum: RequirementLevel,
    pub(crate) reason: String,
}

impl CapabilityRequirement {
    /// The stable requirement identifier.
    pub fn requirement_id(&self) -> &NamespacedId {
        &self.requirement_id
    }

    /// The declared capability key.
    pub fn capability(&self) -> &str {
        &self.capability
    }

    /// The declared minimum guarantee.
    pub const fn minimum(&self) -> RequirementLevel {
        self.minimum
    }

    /// The bounded reason.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// The closed minimum guarantee levels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RequirementLevel {
    /// Full support is required.
    Full,
    /// Partial support is acceptable.
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

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "full" => Some(Self::Full),
            "partial" => Some(Self::Partial),
            _ => None,
        }
    }
}

/// The closed v1 capability vocabulary of this family.
pub const CAPABILITIES: &[&str] = &[
    "state.transition_consistency",
    "state.reachability",
    "invariant.uniqueness",
    "invariant.one_active",
    "invariant.cardinality",
    "invariant.temporal_ordering",
    "invariant.immutable_after_state",
];

/// Whether one capability key is a provider-owned adapter capability.
pub(crate) fn is_provider_capability(key: &str) -> bool {
    let body = match key.strip_prefix("provider.") {
        Some(body) => body,
        None => return false,
    };
    let bytes = key.as_bytes();
    if key.len() < 12 || key.len() > 96 {
        return false;
    }
    body.split('.').all(super::id::lower_kebab) && bytes[0].is_ascii_lowercase()
}
