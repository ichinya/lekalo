//! Language-neutral projections and target mapping invariants (issue #62).
//!
//! The mapping vectors are pure data: Node's discriminated result, PHP's
//! typed result fields, and Go's `(T, error)` accessor shape all carry the
//! same canonical `{id, code, category, public payload}` quadruple, and
//! the public payload never contains private fields. Exception classes,
//! Go wrapper text, framework names, localized messages, and HTTP
//! statuses are projections — never identity, never a core contract
//! field. A catch-all target mapping is refused under the strict profile,
//! and a mapping that fails to preserve the canonical identity is
//! non-conformant by construction.

use super::diagnostic;
use super::id::ErrorId;
use super::normalize;
use super::registry::ErrorRegistry;
use super::result::PublicPayload;
use super::types::{ErrorContract, OperationKind};
use crate::diagnostics::DiagnosticSet;

/// The closed projection vocabulary a target may declare.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionForm {
    /// A discriminated result tag (Node `{ok,value}`).
    ResultTag,
    /// A typed exception or error object (PHP).
    Exception,
    /// Multiple return values (Go `(T, error)`).
    TupleError,
    /// An HTTP response body.
    HttpStatus,
}

impl ProjectionForm {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ResultTag => "result-tag",
            Self::Exception => "exception",
            Self::TupleError => "tuple-error",
            Self::HttpStatus => "http-status",
        }
    }
}

/// One declared target mapping entry for one error code. Entries are
/// identity-preserving by construction: an entry that only carries an
/// exception class, status, or localized text cannot be built.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappingEntry {
    pub(crate) form: ProjectionForm,
    pub(crate) preserves_identity: bool,
}

impl MappingEntry {
    /// Builds one identity-preserving entry for the projection form.
    pub const fn preserving(form: ProjectionForm) -> Self {
        Self {
            form,
            preserves_identity: true,
        }
    }

    /// The projection form.
    pub const fn form(&self) -> ProjectionForm {
        self.form
    }

    /// Whether the canonical identity is preserved (always true for
    /// publicly constructed entries).
    pub const fn preserves_identity(&self) -> bool {
        self.preserves_identity
    }
}

/// One declared target mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetMapping {
    pub(crate) target: String,
    pub(crate) strict: bool,
    pub(crate) entries: Vec<(String, MappingEntry)>,
    pub(crate) catch_all: bool,
}

impl TargetMapping {
    /// Validates and builds one mapping; a strict mapping with a catch-all
    /// or a status-only identity refusal fails closed here.
    pub fn new(
        target: &str,
        strict: bool,
        mut entries: Vec<(String, MappingEntry)>,
        catch_all: bool,
    ) -> Result<Self, DiagnosticSet> {
        if !valid_target(target) {
            return Err(diagnostic::mapping_invalid_set("target-name", target));
        }
        if entries.len() > super::version::MAX_UNION_MEMBERS {
            return Err(diagnostic::limit_exceeded_set("mapping-bound", "256"));
        }
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        if entries.windows(2).any(|window| window[0].0 == window[1].0) {
            return Err(diagnostic::mapping_invalid_set("duplicate-entry", target));
        }
        for (code, entry) in &entries {
            if super::id::ErrorCode::new(code).is_none() {
                return Err(diagnostic::mapping_invalid_set("entry-code", code));
            }
            // A mapping that preserves only an exception class, HTTP
            // status, localized message, or reason code is non-conformant
            // even if it can recover the original text.
            if !entry.preserves_identity {
                return Err(diagnostic::mapping_invalid_set(
                    "identity-not-preserved",
                    target,
                ));
            }
            // An HTTP status is a transport projection, never the sole
            // mapping of an error: status alone cannot carry identity.
            if entry.form == ProjectionForm::HttpStatus {
                return Err(diagnostic::mapping_invalid_set(
                    "status-only-mapping",
                    target,
                ));
            }
        }
        if strict && catch_all {
            return Err(diagnostic::mapping_invalid_set(
                "catch-all-in-strict",
                target,
            ));
        }
        Ok(Self {
            target: target.to_owned(),
            strict,
            entries,
            catch_all,
        })
    }

    /// The mapped target name.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Whether the mapping is strict.
    pub const fn strict(&self) -> bool {
        self.strict
    }

    /// The entries in canonical code order.
    pub fn entries(&self) -> &[(String, MappingEntry)] {
        &self.entries
    }

    /// Whether a catch-all branch is declared.
    pub const fn catch_all(&self) -> bool {
        self.catch_all
    }

    /// Checks this mapping against one operation's declared union: every
    /// member must be covered (strict), and every mapped code must belong
    /// to the union. The canonical id, code, and category are preserved
    /// by construction: the mapping keys on the immutable code.
    pub fn check_against(
        &self,
        registry: &ErrorRegistry,
        operation: &ErrorId,
    ) -> Result<(), DiagnosticSet> {
        let binding = registry.binding(operation).ok_or_else(|| {
            diagnostic::binding_invalid_set("unknown-operation", operation.as_str())
        })?;
        let mut covered = 0usize;
        for member in binding.errors().members() {
            let error = registry.error(member.id()).ok_or_else(|| {
                diagnostic::binding_invalid_set("unresolved-member", member.id().as_str())
            })?;
            let mapped = self
                .entries
                .iter()
                .any(|(code, _)| code == error.code().as_str());
            if mapped {
                covered += 1;
            } else if self.strict {
                return Err(diagnostic::mapping_missing_set(&self.target));
            }
        }
        if covered != self.entries.len() && !self.catch_all {
            return Err(diagnostic::mapping_invalid_set(
                "entry-outside-union",
                &self.target,
            ));
        }
        if self.catch_all && !self.strict {
            // A permissive catch-all is recorded, never a strict pass.
        }
        let _ = binding.kind();
        Ok(())
    }
}

/// The target-name grammar: bounded lowercase tokens (`node-typescript`).
fn valid_target(target: &str) -> bool {
    let bytes = target.as_bytes();
    !target.is_empty()
        && target.len() <= 63
        && bytes[0].is_ascii_lowercase()
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

/// The Node projection vector: a discriminated result carrying the
/// canonical identity and only public payload fields.
pub fn node_error_vector(error: &ErrorContract, payload: &PublicPayload) -> String {
    format!(
        "{{\"error\":{},\"ok\":false}}",
        normalize::identity_bytes(error, payload)
    )
}

/// The PHP projection vector: a typed result object whose fields expose
/// the same canonical identity.
pub fn php_error_vector(error: &ErrorContract, payload: &PublicPayload) -> String {
    format!(
        "{{\"result\":{},\"style\":\"typed-result\"}}",
        normalize::identity_bytes(error, payload)
    )
}

/// The Go projection vector: `(T, error)` with typed `ID/Code/Category`
/// accessors over the same canonical identity.
pub fn go_error_vector(error: &ErrorContract, payload: &PublicPayload) -> String {
    format!(
        "{{\"isError\":true,\"tuple\":{}}}",
        normalize::identity_bytes(error, payload)
    )
}

/// The generic safe infrastructure response: a fixed shape, no detail
/// echo, never claiming a declared semantic error occurred.
pub fn infrastructure_vector(kind: super::result::InfrastructureKind) -> String {
    format!(
        "{{\"error\":{{\"category\":\"infrastructure\",\"payload\":{{}},\"reason\":\"unknown\"}},\"ok\":false,\"kind\":\"{}\"}}",
        kind.as_str(),
    )
}

/// The operation kind marker used by every vector's success twin.
pub const fn kind_marker(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Command => "command",
        OperationKind::Query => "query",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infrastructure_vector_is_generic_and_fixed() {
        let vector = infrastructure_vector(super::super::result::InfrastructureKind::Provider);
        assert!(vector.contains("\"reason\":\"unknown\""));
        assert!(!vector.contains("stack"));
    }

    #[test]
    fn strict_mappings_reject_catch_alls_and_status_only() {
        let entry = |form| MappingEntry {
            form,
            preserves_identity: true,
        };
        assert!(TargetMapping::new(
            "node-typescript",
            true,
            vec![("LEK-ERR-001".to_owned(), entry(ProjectionForm::ResultTag))],
            true,
        )
        .is_err());
        assert!(TargetMapping::new(
            "node-typescript",
            true,
            vec![("LEK-ERR-001".to_owned(), entry(ProjectionForm::HttpStatus))],
            false,
        )
        .is_err());
        let non_preserving = MappingEntry {
            form: ProjectionForm::Exception,
            preserves_identity: false,
        };
        assert!(TargetMapping::new(
            "legacy",
            false,
            vec![("LEK-ERR-001".to_owned(), non_preserving)],
            false,
        )
        .is_err());
    }
}
