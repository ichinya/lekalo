//! Negotiated watch selection (0.8.0 only). A declaration is not inferred
//! from scan support: the upgraded describe must explicitly opt in.
use serde::{Deserialize, Serialize};

use super::{scopes, wire, TargetFailure};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchSupport {
    pub selection: bool,
}

/// One module partition, or the mandatory project-level obligations. The
/// full captured IR is resolution context, never permission to validate
/// other module partitions. Removed symbols remain selection inputs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchSelection {
    pub source_revision: String,
    pub scope: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub module: Option<String>,
    pub symbols: Vec<String>,
    pub changed_paths: Vec<String>,
}

fn present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

impl WatchSelection {
    pub fn digest(&self) -> String {
        format!(
            "sha256:{}",
            super::plan::sha256_hex(
                &serde_json::to_vec(&serde_json::to_value(self).expect("selection value"))
                    .expect("selection serialization")
            )
        )
    }

    pub(super) fn valid(&self) -> bool {
        let sorted = |items: &[String]| items.windows(2).all(|p| p[0] < p[1]);
        wire::is_sha256_digest(&self.source_revision)
            && match self.scope.as_str() {
                "module" => self.module.as_deref().is_some_and(scopes::is_token),
                "global" => self.module.is_none() && self.symbols.is_empty(),
                _ => false,
            }
            && self.symbols.len() <= 4096
            && sorted(&self.symbols)
            && self.symbols.iter().all(|id| {
                crate::trace::id::is_semantic_id(id)
                    && self
                        .module
                        .as_ref()
                        .is_some_and(|module| id == module || id.starts_with(&format!("{module}.")))
            })
            && self.changed_paths.len() <= 4096
            && sorted(&self.changed_paths)
            && self
                .changed_paths
                .iter()
                .all(|p| scopes::is_logical_path(p))
    }
}

pub(super) fn validate(request: &wire::RequestEnvelope) -> Result<(), TargetFailure> {
    if let Some(selection) = &request.watch_request {
        if request.protocol_version != super::version::WATCH_VERSION
            || !matches!(
                request.operation,
                wire::Operation::Scan | wire::Operation::Validate
            )
            || request.target.is_none()
            || !selection.valid()
        {
            return Err(TargetFailure::RequestInvalid {
                detail: "watch-selection",
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_admission_requires_exact_version_read_only_operation_and_target() {
        let selection = WatchSelection {
            source_revision: format!("sha256:{}", "0".repeat(64)),
            scope: "module".into(),
            module: Some("planner".into()),
            symbols: vec!["planner.title".into()],
            changed_paths: vec![],
        };
        let mut request = super::super::base_envelope(
            wire::Operation::Scan,
            super::super::version::WATCH_VERSION.into(),
            wire::Limits {
                timeout_ms: Some(1000),
                max_output_bytes: Some(1024),
            },
        );
        request.target = Some("fixture".into());
        request.watch_request = Some(selection);
        request.request_id = wire::request_id(&request);
        assert!(wire::decode_request(&serde_json::to_vec(&request).unwrap()).is_ok());
        for version in ["0.3.2", "0.6.4"] {
            request.protocol_version = version.into();
            assert!(wire::validate_request(&request).is_err());
        }
        request.protocol_version = super::super::version::WATCH_VERSION.into();
        request.operation = wire::Operation::Describe;
        assert!(wire::validate_request(&request).is_err());
        request.operation = wire::Operation::Scan;
        request.target = None;
        assert!(wire::validate_request(&request).is_err());
        assert!(serde_json::from_str::<WatchSelection>(r#"{"source_revision":"x","scope":"global","module":null,"symbols":[],"changed_paths":[]}"#).is_err());
    }

    #[test]
    fn selection_is_closed_sorted_owned_and_digest_bound() {
        let mut selection = WatchSelection {
            source_revision: format!("sha256:{}", "0".repeat(64)),
            scope: "module".into(),
            module: Some("planner".into()),
            symbols: vec!["planner.title".into()],
            changed_paths: vec!["lekalo/modules/planner/entities.yaml".into()],
        };
        assert!(selection.valid());
        let digest = selection.digest();
        selection.source_revision = format!("sha256:{}", "1".repeat(64));
        assert_ne!(selection.digest(), digest);
        selection.symbols.push("unrelated.title".into());
        assert!(!selection.valid());
        selection.symbols.clear();
        selection.scope = "global".into();
        assert!(!selection.valid());
        selection.module = None;
        assert!(selection.valid());
        selection.changed_paths.push("../escape".into());
        assert!(!selection.valid());
        let bytes = serde_json::to_string(&selection)
            .unwrap()
            .replace("\"scope\"", "\"unknown\"");
        assert!(serde_json::from_str::<WatchSelection>(&bytes).is_err());
    }
}
