//! Extension binding resolution: the real pinned implementation behind a
//! custom/foreign extension selection (issue #41).
//!
//! `validated: true` on a coverage row is evidence, not authority. The
//! selection resolves only when the pinned binding names a real
//! project file whose exact bytes carry the pinned `contractDigest`; a
//! missing or retargeted binding is `managed.extension-unresolved` and
//! never reaches a plan. The bound implementation stays maintained
//! source: the managed lane renders its explicit invocation but never
//! owns, regenerates, or cleans those bytes.

use super::fs;
use super::wire::{Coverage, TargetAssignment};
use super::{DomainResult, EXTENSION_UNRESOLVED};

fn unresolved(symbol: &str, reason: &str) -> DomainResult {
    super::diagnostic::failure(
        EXTENSION_UNRESOLVED,
        &[("requirement", symbol), ("reason", reason)],
    )
}

/// The project-relative file of one binding spelling: the binding is a
/// project-relative path, optionally followed by `#symbol`. The file part
/// must be a safe project-relative coordinate.
pub fn binding_file(binding: &str) -> Option<&str> {
    let file = binding.split('#').next()?;
    if !super::rel_path(file) {
        return None;
    }
    Some(file)
}

/// Resolve every validated extension of one coverage document against the
/// live project: the pinned binding file must exist with the exact pinned
/// digest. Selected-but-unvalidated rows are already refused by the
/// semantic admission; this check runs after it.
pub fn resolve(
    root: &std::path::Path,
    _target: &TargetAssignment,
    coverage: &Coverage,
) -> Result<(), DomainResult> {
    for check in &coverage.extensions {
        if !check.validated {
            continue;
        }
        let Some(file) = binding_file(&check.binding) else {
            return Err(unresolved(&check.symbol, "binding"));
        };
        match fs::read_file(root, file) {
            Ok(Some(bytes)) => {
                if super::digest(&bytes) != check.contract_digest {
                    return Err(unresolved(&check.symbol, "stale"));
                }
            }
            Ok(None) => return Err(unresolved(&check.symbol, "missing")),
            Err(_) => return Err(unresolved(&check.symbol, "unresolved")),
        }
    }
    Ok(())
}
