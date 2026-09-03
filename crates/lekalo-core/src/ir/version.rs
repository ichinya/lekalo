//! Issue #8 IR contract identity.
//!
//! The IR contract version is its own family: it is independent of the
//! product version, of the source Model versions (`0.1.0` / `1.0.0`), and of
//! every other contract family. One compiled IR binds the exact source Model
//! version it was built from; it never upgrades, downgrades, or migrates.

/// The IR contract family identifier.
pub const FAMILY: &str = "dev.lekalo.ir";

/// The exact IR contract version.
pub const VERSION: &str = "0.1.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.ir@0.1.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.ir");
        assert_eq!(VERSION, "0.1.0");
    }
}
