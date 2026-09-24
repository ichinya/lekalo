//! The runtime-neutral engine input document (issue #69, acceptance
//! criterion 1).
//!
//! [`engine_input`] is the single document every runtime consumer —
//! Node.js, Laravel/PHP, Go, and the Rust runtime itself — receives
//! for one storage-engine profile over its bound projection: the
//! engine identity and version, the closed policy answers, and the
//! derived projection as declared data. The bytes are canonical and
//! byte-identical for value-equal inputs, so the three committed
//! runtime goldens are one document consumed three times, never three
//! projections. Nothing runtime-specific is invented: a consumer maps
//! the declared facts onto its own SQL library or ORM mapper.

use crate::diagnostics::DiagnosticSet;
use crate::storage_projection::derivation::project;
use crate::storage_projection::StorageProjectionAttachment;

use super::diagnostic;
use super::diagnostic::MAPPING_INVALID;
use super::StorageEngineAttachment;

/// Build the canonical engine input document for one profile over its
/// bound projection. The projectionRef binding is verified exactly as
/// the DDL renderer verifies it. Pure and read-only.
pub fn engine_input(
    profile: &StorageEngineAttachment,
    attachment: &StorageProjectionAttachment,
) -> Result<String, DiagnosticSet> {
    let binding = attachment.canonical_bytes()?;
    let digest = crate::digest::sha256_hex(binding.as_bytes());
    if format!("sha256:{digest}") != profile.projection_ref().as_str() {
        return Err(diagnostic::rule_invalid(
            MAPPING_INVALID,
            "projection-binding-mismatch",
            None,
        ));
    }
    let derived = project(attachment, crate::storage_projection::Namespace::Postgres)?;
    let bytes = crate::storage_projection::canonical::derived_bytes(&derived)?;
    let projection: serde_json::Value = serde_json::from_str(&bytes)
        .map_err(|_| diagnostic::rule_invalid(MAPPING_INVALID, "projection-undecodable", None))?;
    let policies = super::canonical::object(vec![
        (
            "array",
            Some(format!("\"{}\"", profile.policies().array().key())),
        ),
        (
            "enum",
            Some(format!("\"{}\"", profile.policies().enum_policy().key())),
        ),
        (
            "identifierQuote",
            Some(if profile.policies().identifier_quote_always() {
                "\"always\"".to_owned()
            } else {
                "\"never\"".to_owned()
            }),
        ),
        (
            "json",
            Some(format!("\"{}\"", profile.policies().json().key())),
        ),
        (
            "pagination",
            Some(super::canonical::object(vec![
                (
                    "cursor",
                    Some(if profile.policies().pagination().cursor_keyset() {
                        "\"keyset\"".to_owned()
                    } else {
                        "\"forbidden\"".to_owned()
                    }),
                ),
                (
                    "offset",
                    Some(if profile.policies().pagination().offset_allowed() {
                        "\"allowed\"".to_owned()
                    } else {
                        "\"forbidden\"".to_owned()
                    }),
                ),
            ])),
        ),
        (
            "time",
            Some(super::canonical::object(vec![
                (
                    "instant",
                    Some(if profile.policies().time().instant_timestamptz() {
                        "\"timestamptz\"".to_owned()
                    } else {
                        "\"timestamp\"".to_owned()
                    }),
                ),
                (
                    "local",
                    Some(if profile.policies().time().local_allowed() {
                        "\"allowed\"".to_owned()
                    } else {
                        "\"forbidden\"".to_owned()
                    }),
                ),
            ])),
        ),
    ]);
    let document = super::canonical::object(vec![
        ("engine", Some(format!("\"{}\"", profile.engine().key()))),
        (
            "engineVersion",
            Some(super::canonical::string(profile.engine_version().as_str())),
        ),
        ("policies", Some(policies)),
        ("projection", Some(projection.to_string())),
    ]);
    if document.len() > super::version::MAX_CANONICAL_BYTES {
        return Err(diagnostic::export_limit_set(document.len()));
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> StorageEngineAttachment {
        let value: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../tests/fixtures/storage-engine/valid/planner-postgres.json"
        ))
        .expect("fixture");
        StorageEngineAttachment::from_value(&value).expect("valid profile")
    }

    fn attachment() -> StorageProjectionAttachment {
        let value: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../tests/fixtures/storage-projection/valid/planner-storage.json"
        ))
        .expect("fixture");
        StorageProjectionAttachment::from_value(&value).expect("valid projection")
    }

    #[test]
    fn the_input_document_is_one_canonical_projection_with_policies() {
        let document = engine_input(&profile(), &attachment()).expect("builds");
        let parsed: serde_json::Value = serde_json::from_str(&document).expect("json");
        assert_eq!(parsed["engine"], "postgres");
        assert_eq!(parsed["engineVersion"], "16.4.0");
        assert_eq!(parsed["policies"]["json"], "jsonb");
        assert_eq!(parsed["policies"]["identifierQuote"], "always");
        assert_eq!(
            parsed["projection"]["namespace"], "postgres",
            "the derived projection rides beside the policies"
        );
        assert!(
            !parsed["projection"]["joins"].is_null(),
            "the join table is materialized in the same document"
        );
        // Determinism: two builds are byte-identical.
        let again = engine_input(&profile(), &attachment()).expect("builds");
        assert_eq!(document, again);
    }

    #[test]
    fn a_foreign_binding_refuses_the_input_document() {
        let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../tests/fixtures/storage-engine/valid/planner-postgres.json"
        ))
        .expect("fixture");
        value["projectionRef"] = serde_json::Value::String(
            "sha256:0606060606060606060606060606060606060606060606060606060606060606".into(),
        );
        let foreign = StorageEngineAttachment::from_value(&value).expect("valid profile");
        let error = engine_input(&foreign, &attachment()).expect_err("binding refused");
        assert_eq!(
            error.reason_ids().first().copied(),
            Some("storage-engine.mapping-invalid")
        );
    }
}

#[cfg(test)]
mod runtime_golden_tests {
    /// Acceptance criterion 1: one storage projection used by the
    /// Node/PHP/Go fixtures — the three committed goldens are
    /// byte-identical by construction.
    #[test]
    fn the_three_runtime_goldens_are_one_document() {
        let node = include_bytes!("../../../../tests/fixtures/storage-engine/runtimes/node.json");
        let php = include_bytes!("../../../../tests/fixtures/storage-engine/runtimes/php.json");
        let go = include_bytes!("../../../../tests/fixtures/storage-engine/runtimes/go.json");
        assert_eq!(node, php, "node and php receive the same bytes");
        assert_eq!(php, go, "php and go receive the same bytes");
        let document: serde_json::Value =
            serde_json::from_slice(node).expect("the golden document parses");
        assert_eq!(document["engine"], "postgres");
        assert_eq!(document["projection"]["namespace"], "postgres");
    }
}
