//! Explain contract version identity (issue #96).
//!
//! The explanation-catalog and explain-result schema versions are their
//! own contracts. They take the product version of the commit that
//! introduced them (0.8.0) and move only with a wire-shape change of
//! their own, exactly like the change-catalog pair they mirror.

/// The exact wire discriminator of the explanation-catalog contract.
pub const CATALOG_SCHEMA_VERSION: &str = "lekalo/explain-catalog/v0.8.0";

/// The embedded explanation-catalog identity.
pub const CATALOG_IDENTITY: &str = "dev.lekalo.explain-catalog@0.8.0";

/// The exact wire discriminator of the explain-result contract.
pub const RESULT_SCHEMA_VERSION: &str = "lekalo/explain-result/v0.8.0";

/// The explain-result identity.
pub const RESULT_IDENTITY: &str = "dev.lekalo.explain-result@0.8.0";
