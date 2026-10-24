//! Negotiated watch selection (0.8.0 only). Protocol, source Model and
//! IR admission are separate explicit declarations, never inferred.
use serde::{Deserialize, Serialize};

use super::{scopes, version, wire, TargetFailure};

/// Closed watch-selection changed-path bound, also enforced on revision pairs.
pub const MAX_CHANGED_PATHS: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchSupport {
    pub selection: bool,
    pub model_versions: Vec<String>,
}

impl WatchSupport {
    pub(super) fn valid(&self) -> bool {
        self.selection
            && !self.model_versions.is_empty()
            && self.model_versions.len() <= version::WATCH_MODEL_VERSIONS.len()
            && self.model_versions.windows(2).all(|p| p[0] < p[1])
            && self
                .model_versions
                .iter()
                .all(|v| version::WATCH_MODEL_VERSIONS.contains(&v.as_str()))
    }
}

/// Exact compiled input custody. Source bytes are bound by source_revision;
/// these pins bind their Model meaning and the actual disclosed IR bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchPins {
    pub model_version: String,
    pub ir_version: String,
    pub ir_digest: String,
}

impl WatchPins {
    pub fn for_compilation(compilation: &crate::ir::Compilation) -> Self {
        Self {
            model_version: compilation.project.model_version.as_str().to_owned(),
            ir_version: compilation.project.model_version.as_str().to_owned(),
            ir_digest: format!(
                "sha256:{}",
                crate::digest::sha256_hex(compilation.project.to_canonical_json().as_bytes())
            ),
        }
    }

    pub(super) fn valid(&self) -> bool {
        version::WATCH_MODEL_VERSIONS.contains(&self.model_version.as_str())
            && self.ir_version == self.model_version
            && wire::is_sha256_digest(&self.ir_digest)
    }

    /// Check captured IR before any adapter launch, including scan. A stale
    /// digest, mismatched discriminator or unknown Model is never downgraded.
    pub(super) fn matches_ir(&self, bytes: &[u8]) -> bool {
        if !self.valid() || self.ir_digest != format!("sha256:{}", crate::digest::sha256_hex(bytes))
        {
            return false;
        }
        let Ok(value) = crate::adoption::wire::unique_json(bytes) else {
            return false;
        };
        value["modelVersion"].as_str() == Some(self.model_version.as_str())
            && value["contract"].as_str()
                == Some(format!("dev.lekalo.ir@{}", self.ir_version).as_str())
    }
}

/// One module partition, or the mandatory project-level obligations. The
/// full captured IR is resolution context, never permission to validate
/// other module partitions. Removed symbols remain selection inputs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchSelection {
    pub source_revision: String,
    pub pins: WatchPins,
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
            && self.pins.valid()
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
            && self.changed_paths.len() <= MAX_CHANGED_PATHS
            && sorted(&self.changed_paths)
            && self
                .changed_paths
                .iter()
                .all(|p| scopes::is_logical_path(p))
    }
}

pub(super) fn validate(request: &wire::RequestEnvelope) -> Result<(), TargetFailure> {
    if let Some(selection) = &request.watch_request {
        if !version::supports_watch(&request.protocol_version)
            || !matches!(
                request.operation,
                wire::Operation::Scan | wire::Operation::Validate
            )
            || request.target.is_none()
            || request.ir_path.is_none()
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

    fn selection() -> WatchSelection {
        WatchSelection {
            source_revision: format!("sha256:{}", "0".repeat(64)),
            pins: WatchPins {
                model_version: "0.2.16".into(),
                ir_version: "0.2.16".into(),
                ir_digest: format!("sha256:{}", "1".repeat(64)),
            },
            scope: "module".into(),
            module: Some("planner".into()),
            symbols: vec!["planner.title".into()],
            changed_paths: vec![],
        }
    }

    #[test]
    fn wire_admission_requires_exact_version_read_only_operation_target_and_ir() {
        let mut request = super::super::base_envelope(
            wire::Operation::Scan,
            version::WATCH_VERSION.into(),
            wire::Limits {
                timeout_ms: Some(1000),
                max_output_bytes: Some(1024),
            },
        );
        request.target = Some("fixture".into());
        request.ir_path = Some("ir/project.json".into());
        request.watch_request = Some(selection());
        request.request_id = wire::request_id(&request);
        assert!(wire::decode_request(&serde_json::to_vec(&request).unwrap()).is_ok());
        request.protocol_version = version::SCAN_VERSION.into();
        request.request_id = wire::request_id(&request);
        assert!(wire::decode_request(&serde_json::to_vec(&request).unwrap()).is_ok());
        for version in ["0.3.2", "0.6.4"] {
            request.protocol_version = version.into();
            assert!(wire::validate_request(&request).is_err());
        }
        request.protocol_version = version::WATCH_VERSION.into();
        request.operation = wire::Operation::Describe;
        assert!(wire::validate_request(&request).is_err());
        request.operation = wire::Operation::Scan;
        request.target = None;
        assert!(wire::validate_request(&request).is_err());
        request.target = Some("fixture".into());
        request.ir_path = None;
        assert!(wire::validate_request(&request).is_err());
        assert!(serde_json::from_str::<WatchSelection>(r#"{"source_revision":"x","scope":"global","module":null,"symbols":[],"changed_paths":[]}"#).is_err());
    }

    #[test]
    fn selection_is_closed_sorted_owned_and_every_pin_is_digest_bound() {
        let mut selection = selection();
        assert!(selection.valid());
        let digest = selection.digest();
        selection.pins.model_version = "0.8.0".into();
        assert!(!selection.valid(), "a stale IR pin is refused");
        selection.pins.ir_version = "0.8.0".into();
        assert!(selection.valid());
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
    }

    #[test]
    fn changed_path_boundary_admits_4096_and_refuses_4097() {
        let mut selection = selection();
        selection.changed_paths = (0..MAX_CHANGED_PATHS)
            .map(|i| format!("src/{i:04}.txt"))
            .collect();
        assert!(selection.valid());
        selection.changed_paths.push("src/4096.txt".into());
        assert!(!selection.valid());
    }

    #[test]
    fn client_refuses_undeclared_or_stale_successor_pins_before_spawn() {
        use super::super::{transport::AdapterCommand, CallRequest, DescribeOutcome, TargetClient};
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("ir")).unwrap();
        let bytes = br#"{"contract":"dev.lekalo.ir@0.8.0","modelVersion":"0.8.0"}"#;
        std::fs::write(root.path().join("ir/project.json"), bytes).unwrap();
        let command = AdapterCommand {
            program: "must-never-launch".into(),
            args: vec![],
        };
        let mut selected = selection();
        selected.pins = WatchPins {
            model_version: "0.8.0".into(),
            ir_version: "0.8.0".into(),
            ir_digest: format!("sha256:{}", crate::digest::sha256_hex(bytes)),
        };
        let capabilities: wire::Capabilities = serde_json::from_value(serde_json::json!({
            "adapter":{"id":"fixture","version":"0.8.0","digest":format!("sha256:{}", "0".repeat(64))},
            "protocol_versions":["0.3.2","0.8.0"], "operations":["describe","scan","validate"],
            "transports":["stdin"], "targets":["fixture"], "read_scopes":["ir/**"],
            "ir_versions":["0.2.16","0.8.0"], "watch":{"selection":true,"model_versions":["0.2.16","0.8.0"]}
        })).unwrap();
        let call = || CallRequest {
            operation: wire::Operation::Scan,
            target: Some("fixture"),
            profile: None,
            profile_resolution: None,
            ir_path: Some("ir/project.json"),
            dry_run: None,
            plan_id: None,
            native_request: None,
            lint_request: None,
        };
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/watch/golden/pin-refusals.json"
        ))
        .unwrap();
        assert_eq!(golden["cases"].as_array().unwrap().len(), 6);
        for row in golden["cases"].as_array().unwrap() {
            let case = row["case"].as_str().unwrap();
            assert_eq!(row["spawned"], false);
            let mut caps = capabilities.clone();
            let mut pin = selected.clone();
            let mut negotiated_version = version::WATCH_VERSION;
            let expected = match case {
                "unknown" => {
                    pin.pins.model_version = "0.8.1".into();
                    TargetFailure::RequestInvalid {
                        detail: "watch-selection",
                    }
                }
                "mixed" => {
                    pin.pins.ir_version = "0.2.16".into();
                    TargetFailure::RequestInvalid {
                        detail: "watch-selection",
                    }
                }
                "stale" => {
                    pin.pins.ir_digest = format!("sha256:{}", "0".repeat(64));
                    TargetFailure::RequestInvalid {
                        detail: "watch-ir-pin",
                    }
                }
                "undeclared-model" => {
                    caps.watch.as_mut().unwrap().model_versions = vec!["0.2.16".into()];
                    TargetFailure::IrUnsupported
                }
                "undeclared-ir" => {
                    caps.ir_versions = vec!["0.2.16".into()];
                    TargetFailure::IrUnsupported
                }
                "legacy" => {
                    negotiated_version = version::BASE_VERSION;
                    TargetFailure::CapabilityUnsupported {
                        detail: "watch-selection",
                    }
                }
                _ => unreachable!(),
            };
            let detail = match expected {
                TargetFailure::RequestInvalid { detail } => detail,
                TargetFailure::IrUnsupported => "ir-unsupported",
                TargetFailure::CapabilityUnsupported { .. } => "watch-selection-unsupported",
                _ => unreachable!(),
            };
            assert_eq!(
                row["runtime"], detail,
                "{case}: golden refusal classification"
            );
            let mut client = TargetClient::new(super::super::transport::TransportLimits::default());
            client.described = Some((
                command.clone(),
                DescribeOutcome {
                    capabilities: caps,
                    capability_digest: "test-only".into(),
                    negotiated_version,
                },
            ));
            let before = super::super::transport::launches();
            assert_eq!(
                client.call_watch(&command, call(), root.path(), None, &pin),
                Err(expected),
                "{case}"
            );
            assert_eq!(
                super::super::transport::launches(),
                before,
                "{case}: refused before disclosure"
            );
        }
    }

    #[test]
    fn captured_ir_refuses_unknown_mixed_and_stale_pins() {
        for version in version::WATCH_MODEL_VERSIONS {
            let bytes =
                format!(r#"{{"contract":"dev.lekalo.ir@{version}","modelVersion":"{version}"}}"#);
            let pins = WatchPins {
                model_version: version.into(),
                ir_version: version.into(),
                ir_digest: format!("sha256:{}", crate::digest::sha256_hex(bytes.as_bytes())),
            };
            assert!(pins.matches_ir(bytes.as_bytes()));
            assert!(
                !pins.matches_ir(format!("{bytes}\n").as_bytes()),
                "stale exact bytes"
            );
            for bad in ["0.3.2", "0.8.1", "unknown"] {
                assert!(!WatchPins {
                    model_version: bad.into(),
                    ..pins.clone()
                }
                .matches_ir(bytes.as_bytes()));
                assert!(!WatchPins {
                    ir_version: bad.into(),
                    ..pins.clone()
                }
                .matches_ir(bytes.as_bytes()));
            }
            let mut wrong = pins.clone();
            wrong.ir_digest = format!("sha256:{}", "0".repeat(64));
            assert!(!wrong.matches_ir(bytes.as_bytes()));
        }
        assert!(!WatchSupport {
            selection: true,
            model_versions: vec![]
        }
        .valid());
        assert!(!WatchSupport {
            selection: true,
            model_versions: vec!["0.8.1".into()]
        }
        .valid());
        assert!(!WatchSupport {
            selection: true,
            model_versions: vec!["0.8.0".into(), "0.8.0".into()]
        }
        .valid());
    }
}
