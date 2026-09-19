//! The adapter package manifest, discovery, install and trust model
//! (issue #32).
//!
//! This module owns the custody chain between "bytes on a disk" and "a
//! target adapter the core may execute": the closed package manifest
//! ([`manifest`]), the discovery sources ([`discovery`]), the integrity
//! and signature gates ([`integrity`], [`signature`]), the trust
//! vocabulary with the revocation store ([`trust`]), the governed
//! `.lekalo/adapters/**` store ([`inventory`]), the previewed-and-
//! confirmed atomic install ([`install`]), and the manifest-to-manifest
//! update diff ([`diff`]).
//!
//! Security invariants, in gate order — every execution path enters
//! through [`resolve`], and no child process exists before the gates
//! pass:
//!
//! 1. **Manifest gate.** The package manifest decodes and validates
//!    against the closed contract (`adapter.manifest-invalid`).
//! 2. **Compatibility gate.** The manifest's exact-set compatibility
//!    must cover the current protocol and IR contracts
//!    (`adapter.incompatible`).
//! 3. **Integrity gate.** Every declared file digest and the package
//!    digest are recomputed from the bytes on disk **before any
//!    execution, describe included** (`adapter.checksum-mismatch`); the
//!    declared signature policy is then evaluated honestly
//!    (`adapter.signature-unverified`).
//! 4. **Trust gate.** The revocation store overrides everything
//!    (`adapter.revoked`); the assigned trust level decides selection
//!    eligibility and quarantine posture (`adapter.trust-insufficient`).
//! 5. **Describe.** The safe handshake runs under the gate-derived
//!    posture, and its self-asserted outcome is cross-checked against
//!    the verified manifest (`adapter.manifest-mismatch`).
//!
//! Trust never widens runtime permissions: the level selects candidacy
//! and confinement strictness only; the declared `permissions` block is
//! the policy the execution-isolation issue enforces. Discovery is never
//! install, and install is never trust: auto-discovery neither installs
//! nor promotes anything.

pub mod canonical;
pub mod diagnostic;
pub mod diff;
pub mod discovery;
pub mod install;
pub mod integrity;
pub mod inventory;
pub mod manifest;
pub mod permissions;
pub mod quarantine;
pub mod signature;
pub mod trust;
pub mod types;
pub mod version;

pub use manifest::{ManifestDigest, ManifestDocument};
pub use types::PackageFailure;

/// The stable module-local reason codes. Every refusal maps onto a
/// registered `adapter.*` rule through [`diagnostic`]; the list here is
/// the closed spelling the callers match on.
pub mod reasons {
    /// The manifest is not a valid closed-wire document.
    pub const MANIFEST_INVALID: &str = "adapter.manifest-invalid";
    /// The describe outcome contradicts the verified manifest.
    pub const MANIFEST_MISMATCH: &str = "adapter.manifest-mismatch";
    /// The manifest compatibility set excludes the current contracts.
    pub const INCOMPATIBLE: &str = "adapter.incompatible";
    /// Declared digests do not match the bytes on disk.
    pub const CHECKSUM_MISMATCH: &str = "adapter.checksum-mismatch";
    /// A required signature cannot be verified by any shipped verifier.
    pub const SIGNATURE_UNVERIFIED: &str = "adapter.signature-unverified";
    /// The adapter id/version is recorded in the revocation store.
    pub const REVOKED: &str = "adapter.revoked";
    /// The package bytes are in quarantine custody.
    pub const QUARANTINED: &str = "adapter.quarantined";
    /// The trust level requires an explicit opt-in that was not given.
    pub const TRUST_INSUFFICIENT: &str = "adapter.trust-insufficient";
    /// The discovery source cannot be resolved (including offline).
    pub const SOURCE_UNAVAILABLE: &str = "adapter.source-unavailable";
    /// A mutating install/update/rollback ran without a confirmed plan.
    pub const INSTALL_PLAN_REQUIRED: &str = "adapter.install-plan-required";
    /// Inputs drifted between preview and confirmation.
    pub const SOURCE_CHANGED: &str = "adapter.source-changed";
    /// The planned path is occupied by foreign content.
    pub const INSTALL_CONFLICT: &str = "adapter.install-conflict";
    /// An install journal is ambiguous; explicit recovery is required.
    pub const RECOVERY_REQUIRED: &str = "adapter.recovery-required";
    /// The package declares hooks; v1 refuses non-empty hooks.
    pub const HOOKS_DECLARED: &str = "adapter.hooks-declared";
    /// An update widens permissions without the explicit policy.
    pub const PERMISSION_ESCALATED: &str = "adapter.permission-escalated";
}
