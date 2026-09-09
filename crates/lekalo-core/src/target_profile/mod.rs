//! The composable target profile contract (issue #29).
//!
//! A target profile composes exactly one component per axis — `runtime`,
//! `storage`, `transport`, `testing`, `analysis`, and `deployment` — from
//! the embedded, closed component registry ([`component`]). Components
//! carry versioned capability contracts, exact sibling requirements,
//! capability requirements, and conflicts, so a target is assembled from
//! reusable parts instead of monolithic framework adapters: moving from
//! Node.js to Go reuses the storage, transport, and deployment components
//! and changes only the runtime-bound ones.
//!
//! The module owns:
//!
//! - the closed declaration document ([`document`]) with its canonical
//!   declared bytes (the lock's `profiles.source_digest` domain);
//! - deterministic resolution ([`resolution`]): inheritance with explicit
//!   precedence, conservative capability composition, compatibility
//!   constraints with sorted stable reasons, and the inheritance
//!   guarantee check that refuses hidden weakening without an explicit
//!   override acknowledgment;
//! - the immutable, machine-readable resolved snapshot whose canonical
//!   bytes are the lock's `profiles.digest` domain — one digest per
//!   profile, so a monorepo may carry several profiles;
//! 4. identity constraints — exact sibling requirements and conflicts
//!    (`combination-incompatible`, with all violations and reasons);
//! 5. capability requirements — every component requirement must be
//!    satisfied by the composed support (`capability-unsatisfied`);
//!
//! Every refusal is deterministic and explainable; nothing here launches
//! a process, reads the filesystem, or depends on the adapter protocol.
//! The adapter protocol consumes a resolved snapshot through its own
//! negotiated wire members (protocol 1.2.0), never raw YAML.

pub mod component;
pub mod diagnostic;
pub mod document;
pub mod portability;
pub mod resolution;
pub mod version;

use crate::result::{DomainResult, Status};
use component::Support;

/// Why one profile document or resolution was refused.
///
/// Every variant maps onto one registered `target-profile.*` rule via
/// [`diagnostic`]; the status — and therefore the exit class — is owned
/// by that mapping, never by severity or category. List members are
/// sorted, deduplicated, and bounded, so verdicts never depend on
/// document or registry iteration order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileFailure {
    /// The document violates the closed declaration contract.
    DocumentInvalid {
        /// Stable bounded detail token (`shape`, `id`, `axis`).
        detail: &'static str,
    },
    /// A declared component is unknown to the embedded registry.
    ComponentUnknown {
        /// The refused component id.
        component: String,
        /// The axis it was declared on.
        axis: &'static str,
    },
    /// An inheritance reference is unknown, cyclic, or too deep.
    ReferenceInvalid {
        /// Stable bounded detail token (`extends-unknown`, `extends-cycle`).
        detail: &'static str,
    },
    /// The resolved component combination violates registered identity
    /// constraints; every violation is reported with a stable reason.
    CombinationIncompatible {
        /// Sorted, deduplicated, bounded reason tokens.
        reasons: Vec<String>,
    },
    /// One capability requirement is not satisfied by the resolved
    /// components. Every unsatisfied capability is reported.
    CapabilityUnsatisfied {
        /// Sorted capability gaps, one per unsatisfied requirement.
        gaps: Vec<CapabilityGap>,
    },
    /// Inheritance would weaken a base capability guarantee without the
    /// explicit override acknowledgment accepting the weaker state.
    InheritanceWeakening {
        /// Sorted capability ids whose guarantee would silently weaken.
        capabilities: Vec<String>,
    },
}

/// One unsatisfied capability requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityGap {
    /// The stable dotted capability id.
    pub capability: String,
    /// The minimum support the profile was required to resolve to.
    pub required: Support,
    /// The support the profile actually resolves to, or `None` when no
    /// component in the profile provides the capability at all.
    pub actual: Option<Support>,
}

/// The bounded wire spelling of a resolved support state, including the
/// meaningful absence of any provider.
pub(crate) fn support_token(support: Option<Support>) -> String {
    match support {
        Some(support) => support.as_str().to_owned(),
        None => "absent".to_owned(),
    }
}

impl ProfileFailure {
    /// The registered rule id of one failure. Failures that carry lists
    /// project one diagnostic per member; this names the rule of the
    /// first projected diagnostic.
    pub fn rule(&self) -> &'static str {
        match self {
            Self::DocumentInvalid { .. } => "target-profile.document-invalid",
            Self::ComponentUnknown { .. } => "target-profile.component-unknown",
            Self::ReferenceInvalid { .. } => "target-profile.reference-invalid",
            Self::CombinationIncompatible { .. } => "target-profile.combination-incompatible",
            Self::CapabilityUnsatisfied { .. } => "target-profile.capability-unsatisfied",
            Self::InheritanceWeakening { .. } => "target-profile.inheritance-weakening",
        }
    }

    /// The closed status of every profile refusal: a project input that
    /// violates the contract is invalid, never denied or unavailable.
    pub const fn status() -> Status {
        Status::Invalid
    }
}

impl From<&ProfileFailure> for DomainResult {
    fn from(failure: &ProfileFailure) -> Self {
        diagnostic::domain_result(failure)
    }
}

impl From<ProfileFailure> for DomainResult {
    fn from(failure: ProfileFailure) -> Self {
        Self::from(&failure)
    }
}
