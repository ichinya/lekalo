//! Quarantine custody helpers (issue #32).
//!
//! Quarantine is a state, not a flag: quarantined packages live under
//! `.lekalo/adapters/quarantine/<id>/<version>-<digest8>/` (opaque to
//! the live store), never in `packages/**`, and purge is the only
//! removal path. The custody paths are computed here so install, purge,
//! and release share one spelling.

/// The quarantine custody root.
pub const QUARANTINE_DIR: &str = ".lekalo/adapters/quarantine";

/// The quarantine custody path for one installed package row.
pub fn quarantine_path(id: &str, version: &str, digest: &str) -> String {
    let digest8: String = digest["sha256:".len()..].chars().take(8).collect();
    format!("{QUARANTINE_DIR}/{id}/{version}-{digest8}")
}
