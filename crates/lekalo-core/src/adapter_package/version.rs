//! Contract identity constants of the adapter package family (issue #32).
//!
//! The manifest schema version, the inventory schema version, and the
//! install-plan schema version are independent of the product release and
//! of the Model/IR/protocol contract versions, exactly as every other
//! closed family in this workspace.

/// The exact wire discriminator of the adapter package manifest contract.
pub const MANIFEST_SCHEMA_VERSION: &str = "lekalo/adapter-manifest/v0.3.2";

/// The embedded manifest schema artifact identity.
pub const MANIFEST_IDENTITY: &str = "dev.lekalo.adapter-manifest@0.3.2";

/// The exact wire discriminator of the adapter inventory contract.
pub const INVENTORY_SCHEMA_VERSION: &str = "lekalo/adapter-inventory/v0.3.2";

/// The embedded inventory schema artifact identity.
pub const INVENTORY_IDENTITY: &str = "dev.lekalo.adapter-inventory@0.3.2";

/// The exact wire discriminator of the adapter install plan contract.
pub const INSTALL_PLAN_SCHEMA_VERSION: &str = "lekalo/adapter-install-plan/v0.3.2";

/// The embedded install plan schema artifact identity.
pub const INSTALL_PLAN_IDENTITY: &str = "dev.lekalo.adapter-install-plan@0.3.2";

/// The current product version the package surface carries (custody rule:
/// identical to the workspace product version).
pub const PRODUCT_VERSION: &str = crate::lockfile::PRODUCT_VERSION;
