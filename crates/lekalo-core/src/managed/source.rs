//! The optional project-owned policy uses the existing managed ownership
//! wire format. Reading it never installs a seed or claims native custody.

use std::path::Path;

use super::{diagnostic, Ownership};
use crate::result::DomainResult;

/// The fixed policy home used by semantic ownership changes.
pub const PROJECT_PATH: &str = "lekalo/ownership.json";

/// Read exact source bytes with confined, no-follow, hardlink-refusing I/O.
/// Absence is distinct from every malformed or unreadable present source.
pub fn read_bytes(root: &Path) -> Result<Option<Vec<u8>>, DomainResult> {
    let invalid =
        || diagnostic::failure(super::INPUT_INVALID, &[("reason", "ownership-source-read")]);
    let fs = crate::project_fs::Fs::open(root).map_err(|_| invalid())?;
    match fs.read_file_opt("lekalo", "ownership.json", super::wire::MAX_BYTES) {
        Ok(bytes) => Ok(bytes),
        Err(crate::project_fs::FsErrorKind::NotFound) => Ok(None),
        Err(_) => Err(invalid()),
    }
}

/// Bind the selected assignment to the Model. Unselected legacy assignments
/// need no repository-wide migration. Managed evidence is admitted separately.
pub fn validate_module(
    policy: &Ownership,
    project: &crate::ir::CompiledProject,
    module: &str,
) -> Result<(), DomainResult> {
    // The existing owner's `project` is its receipt namespace, not the
    // Model's semantic project ID. The loaded snapshot and Change history
    // bind the operation to the selected physical/semantic project.
    if !project
        .modules
        .iter()
        .any(|definition| definition.id.as_str() == module)
        || !policy
            .modules
            .iter()
            .any(|assignment| assignment.module == module)
    {
        return Err(diagnostic::failure(
            super::INPUT_INVALID,
            &[("module", module), ("reason", "ownership-module")],
        ));
    }
    Ok(())
}
