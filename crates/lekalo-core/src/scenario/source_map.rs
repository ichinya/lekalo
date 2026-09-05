//! Source-map sidecar references of the Scenario IR (issue #23).
//!
//! The source map itself is a separate digest-bound sidecar keyed by
//! scenario ID, step ID, role and member path, reference role, and
//! occurrence ordinal; it contains only accepted #7/#8 logical
//! project-relative paths and exact byte/line/column ranges — no
//! physical root, no source snippet, no fallback zero span, and no
//! reparsing here. The Scenario IR carries only this typed reference:
//! the exact sidecar identity, its digest, and its entry count.
//! Repeated operation, error, and field references keep separate
//! occurrences in the sidecar.

use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::Sha256Digest;

use super::diagnostic;
use super::version::SOURCE_MAP_IDENTITY;

/// The typed reference to one scenario source-map sidecar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMapRef {
    /// The digest of the exact canonical sidecar payload.
    pub digest: Sha256Digest,
    /// The declared entry count, bounded by the contract limit.
    pub entries: u32,
}

impl SourceMapRef {
    /// Normalize one wire source-map reference, or return the typed
    /// rejection set.
    pub(crate) fn from_json(json: &Json, role: &str) -> Result<SourceMapRef, DiagnosticSet> {
        let object = json
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("source-map-shape", Some(role)))?;
        if object
            .keys()
            .map(String::as_str)
            .collect::<Vec<&str>>()
            .as_slice()
            != ["digest", "entries", "identity"]
        {
            return Err(diagnostic::input_invalid("source-map-shape", Some(role)));
        }
        if object.get("identity").and_then(Json::as_str) != Some(SOURCE_MAP_IDENTITY) {
            return Err(diagnostic::input_invalid("source-map-identity", Some(role)));
        }
        let digest = Sha256Digest::parse(
            object
                .get("digest")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("source-map-digest", Some(role)))?,
        )
        .map_err(|_| diagnostic::input_invalid("source-map-digest", Some(role)))?;
        let entries = object
            .get("entries")
            .and_then(Json::as_u64)
            .filter(|entries| {
                (1..=super::version::MAX_SOURCE_MAP_ENTRIES as u64).contains(entries)
                    && !object.get("entries").expect("checked").is_f64()
            })
            .ok_or_else(|| diagnostic::limit_set("source-map-entries"))?;
        Ok(SourceMapRef {
            digest,
            entries: entries as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn source_map_references_pin_identity_digest_and_entries() {
        let reference = SourceMapRef::from_json(
            &json!({
                "identity": "dev.lekalo.scenario-sourcemap@1.0.0",
                "digest": format!("sha256:{}", "ab".repeat(32)),
                "entries": 7
            }),
            "s",
        )
        .expect("source map reference");
        assert_eq!(reference.entries, 7);
    }

    #[test]
    fn wrong_identity_counts_and_digests_are_rejected() {
        let base = json!({
            "identity": "dev.lekalo.scenario-sourcemap@1.0.0",
            "digest": format!("sha256:{}", "ab".repeat(32)),
            "entries": 7
        });
        let mut wrong_identity = base.clone();
        wrong_identity["identity"] = json!("dev.lekalo.scenario-sourcemap@2.0.0");
        assert!(SourceMapRef::from_json(&wrong_identity, "s").is_err());
        let mut zero = base.clone();
        zero["entries"] = json!(0);
        assert!(SourceMapRef::from_json(&zero, "s").is_err());
        let mut many = base.clone();
        many["entries"] = json!(super::super::version::MAX_SOURCE_MAP_ENTRIES as u64 + 1);
        assert!(SourceMapRef::from_json(&many, "s").is_err());
    }
}
