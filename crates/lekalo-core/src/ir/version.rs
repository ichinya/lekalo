//! Issue #8 IR contract identity.
//!
//! The IR contract version is its own family: it is independent of the
//! product version and of every other contract family. One compiled IR
//! binds the exact source Model version it was built from; it never
//! upgrades, downgrades, or migrates. The family carries one accepted
//! contract version per accepted source Model version: Model 0.2.16
//! sources compile to `dev.lekalo.ir@0.2.16` with byte-identical
//! canonical output, and Model 0.8.0 sources (the typed
//! relations/secondary-uniqueness successor) compile to
//! `dev.lekalo.ir@0.8.0`. There is no alias under 0.2.16: a 0.2.16
//! document can never carry the successor's constructs.

/// The IR contract family identifier.
pub const FAMILY: &str = "dev.lekalo.ir";

/// The exact IR contract version compiled from Model 0.2.16 sources.
pub const VERSION: &str = "0.2.16";

/// The exact contract identity compiled from Model 0.2.16 sources.
pub const IDENTITY: &str = "dev.lekalo.ir@0.2.16";

/// The exact IR contract version compiled from Model 0.8.0 sources
/// (the typed relation and secondary-uniqueness successor).
pub const SUCCESSOR_VERSION: &str = "0.8.0";

/// The exact contract identity compiled from Model 0.8.0 sources.
pub const SUCCESSOR_IDENTITY: &str = "dev.lekalo.ir@0.8.0";

/// The exact contract identity of one compilation: the IR contract
/// version follows the exact source Model version it was built from.
pub fn identity_for(version: crate::loader::ModelVersion) -> &'static str {
    match version {
        crate::loader::ModelVersion::Current => IDENTITY,
        crate::loader::ModelVersion::Successor => SUCCESSOR_IDENTITY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.ir");
        assert_eq!(VERSION, "0.2.16");
        assert_eq!(SUCCESSOR_IDENTITY, format!("{FAMILY}@{SUCCESSOR_VERSION}"));
    }
}
