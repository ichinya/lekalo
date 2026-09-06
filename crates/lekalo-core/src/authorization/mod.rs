//! The closed authorization contract: actors, scopes, policies,
//! composition, field permissions, protected-effect coverage, mapping
//! evidence, and deny-by-default strict evaluation (issue #25).
//!
//! Authorization is part of the semantic model: an operation cannot be
//! generated or implemented without a declared actor, scope, and policy.
//! This module owns that surface and nothing else — identity-provider
//! lookups, middleware generation, and adapter execution stay with the
//! runtime and adapter owners.
//!
//! Family layout, following the accepted house convention:
//!
//! - [`version`] pins the independent contract identity and the closed
//!   owner-approved bounds;
//! - [`document`] is the typed, fail-closed document gate;
//! - [`parse`] is the one bounded file seam for the canonical
//!   `lekalo/authorization.yaml` (the accepted #4 successor path);
//! - [`canonical`] renders the deterministic canonical bytes and
//!   digest;
//! - [`evaluate`] is the pure deny-by-default evaluator over explicit
//!   principal/request contexts;
//! - [`review`] is the static review over an immutable
//!   [`Compilation`](crate::ir::Compilation): reference integrity is
//!   invalid in every profile, coverage/staleness/mapping block only
//!   the built-in strict profile;
//! - [`facts`] supplies the typed contribution facts for the graph
//!   (#13), impact (#16), and diff (#18) seams;
//! - [`diagnostic`] routes every failure through the accepted #11
//!   registry with bounded tokens only.

pub mod canonical;
pub mod diagnostic;
pub mod document;
pub mod evaluate;
pub mod facts;
pub mod parse;
pub mod review;
pub mod version;

pub use document::{
    ActorField, ActorType, Binding, Composition, Decision, Document, FieldPermissions, Literal,
    Mapping, MappingState, ModelRef, Operand, Ownership, Policy, Predicate, Relation, ScopeAnchor,
    ScopeDimension, ScopeMap, ScopeSource, SymbolDecl,
};
pub use evaluate::{
    satisfies_scenario_assertion, DenyReason, Evaluation, FieldAccess, Principal, Request,
    RequestScope, ResourceView,
};
pub use review::{protected_operations, review_selection, Review};
pub use version::{
    limits, FAMILY, IDENTITY, PROFILE_IDENTITY, SCHEMA_ID, SCHEMA_VERSION, SOURCE_PATH, VERSION,
};
