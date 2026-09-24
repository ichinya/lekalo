//! Quarantine custody helpers (issue #32).
//!
//! Quarantine is a state, not a flag: quarantined packages live under
//! `.lekalo/adapters/quarantine/<id>/<version>-<digest8>/` (opaque to
//! the live store), never in `packages/**`, and purge is the only
//! removal path. The custody paths are computed here so install, purge,
//! and release share one spelling.

/// The quarantine custody root.
pub const QUARANTINE_DIR: &str = ".lekalo/adapters/quarantine";

/// The packages custody root.
pub const PACKAGES_DIR: &str = ".lekalo/adapters/packages";

/// The quarantine custody path for one installed package row.
pub fn quarantine_path(id: &str, version: &str, digest: &str) -> String {
    let digest8: String = digest["sha256:".len()..].chars().take(8).collect();
    format!("{QUARANTINE_DIR}/{id}/{version}-{digest8}")
}

/// The packages custody path for one installed package row.
pub fn package_path(id: &str, version: &str, digest: &str) -> String {
    let digest8: String = digest["sha256:".len()..].chars().take(8).collect();
    format!("{PACKAGES_DIR}/{id}/{version}-{digest8}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarantine_path_matches_expected_format() {
        let id = "test-id";
        let version = "1.2.3";
        let digest = "sha256:aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";
        let expected = format!("{QUARANTINE_DIR}/{id}/{version}-aabbccdd");
        assert_eq!(quarantine_path(id, version, digest), expected);
    }

    #[test]
    fn package_path_matches_expected_format() {
        let id = "test-id";
        let version = "1.2.3";
        let digest = "sha256:aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";
        let expected = format!("{PACKAGES_DIR}/{id}/{version}-aabbccdd");
        assert_eq!(package_path(id, version, digest), expected);
    }
}
