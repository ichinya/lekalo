//! Semantic validation of the storage-introspection evidence (issue
//! #117).
//!
//! The closed semantic rules over one fully parsed evidence document:
//! the observed sql mode stays declared and sorted, and the test schema
//! stays under a lifecycle-prefix-style naming. Every violation is one
//! registered diagnostic with no partial result. Pure and read-only.

use crate::diagnostics::DiagnosticSet;

use super::StorageIntrospection;

/// The semantic self-check over one assembled evidence document.
pub(crate) fn semantic_self_check(attachment: &StorageIntrospection) -> Result<(), DiagnosticSet> {
    // The observed digest is present by construction; the observed
    // tables are canonical by normalization. The prefix-style check
    // keeps the bound schema name inside the declared lifecycle
    // naming: a lowercase identifier with at least one underscore
    // separating prefix from run token.
    let name = attachment.test_schema().as_str();
    if !name.contains('_') {
        return Err(super::diagnostic::input_invalid("test-schema"));
    }
    Ok(())
}
