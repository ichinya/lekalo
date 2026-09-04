//! The closed provider diagnostic wire and its normalization (issue #11).
//!
//! Untrusted producers may propose diagnostics only through
//! [`ProviderDiagnosticWire`], which carries a registered rule id, a bounded
//! structured payload, and namespaced `original_code` metadata. Raw provider
//! text — messages, stacks, stdout/stderr, paths, argv, URLs — is not
//! representable. Anything malformed, over-limit, or unsafe collapses into
//! one core-owned `adapter.diagnostic-invalid` infrastructure diagnostic;
//! input is never truncated by arrival order.

use std::collections::BTreeMap;

use serde::de::{Deserializer, MapAccess, Visitor};
use serde::Deserialize;

use super::id::{OriginalCode, ProviderNamespace};
use super::normalize::{build, BuildError};
use super::types::limits;
use super::{DataObject, Diagnostic, SourceLocation};

/// Why a provider-proposed diagnostic was refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderRejection {
    /// The rule id is not registered or not currently emitted.
    UnknownRule,
    /// A path is absolute, relative-escaping, or otherwise unsafe.
    UnsafePath,
    /// A field is malformed, over-limit, or of the wrong type.
    Malformed,
    /// A required namespaced original code is missing.
    MissingOriginalCode,
}

/// The closed, bounded wire one untrusted provider may propose.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderDiagnosticWire {
    /// The registered rule id being proposed.
    id: String,
    /// Optional bounded semantic symbol.
    symbol: Option<String>,
    /// Optional logical location (validated safe path plus range).
    source: Option<SourceLocation>,
    /// The versioned reverse-DNS provider namespace.
    namespace: ProviderNamespace,
    /// The provider's own bounded original code.
    original_code: OriginalCode,
    /// The bounded structured payload.
    data: DataObject,
}

impl ProviderDiagnosticWire {
    /// The proposed rule id.
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Normalize one provider proposal into a core diagnostic.
pub fn normalize(wire: ProviderDiagnosticWire) -> Result<Diagnostic, ProviderRejection> {
    if let Some(path) = wire.source.as_ref().and_then(|source| source.path.as_ref()) {
        if crate::project_fs::path_violation(path).is_some() {
            return Err(ProviderRejection::UnsafePath);
        }
    }
    let mut diagnostic =
        build(&wire.id, wire.symbol, wire.source, wire.data).map_err(|error| match error {
            BuildError::UnknownRule(_) | BuildError::Inactive(_) => ProviderRejection::UnknownRule,
            BuildError::Registry => ProviderRejection::Malformed,
        })?;
    let mut metadata = BTreeMap::new();
    let mut entry = BTreeMap::new();
    entry.insert("original_code".to_owned(), wire.original_code);
    metadata.insert(wire.namespace.to_string(), entry);
    diagnostic.metadata = metadata;
    Ok(diagnostic)
}

/// Collapse any provider failure into the single bounded core diagnostic.
pub fn rejection_diagnostic(reason: &str) -> Diagnostic {
    let mut data = DataObject::new();
    data.insert(
        "reason".to_owned(),
        super::types::DataValue::Token(bounded(reason)),
    );
    build("adapter.diagnostic-invalid", None, None, data)
        .expect("adapter.diagnostic-invalid is a registered active rule")
}

/// Bound any text to the fixed token size; hostile input never scales the
/// envelope. Control characters collapse to spaces.
pub(crate) fn bounded(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    if cleaned.len() <= limits::TOKEN_BYTES {
        return cleaned;
    }
    let mut bound = limits::TOKEN_BYTES;
    while !cleaned.is_char_boundary(bound) {
        bound -= 1;
    }
    cleaned[..bound].to_owned()
}

impl<'de> Deserialize<'de> for ProviderDiagnosticWire {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct WireVisitor;
        impl<'de> Visitor<'de> for WireVisitor {
            type Value = ProviderDiagnosticWire;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a bounded provider diagnostic object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut id: Option<String> = None;
                let mut symbol: Option<String> = None;
                let mut path: Option<String> = None;
                let mut range: Option<super::Range> = None;
                let mut namespace: Option<String> = None;
                let mut original_code: Option<String> = None;
                let mut data: Option<serde_json::Value> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => id = Some(map.next_value()?),
                        "symbol" => symbol = Some(map.next_value()?),
                        "path" => path = Some(map.next_value()?),
                        "range" => range = Some(map.next_value()?),
                        "namespace" => namespace = Some(map.next_value()?),
                        "original_code" => original_code = Some(map.next_value()?),
                        "data" => data = Some(map.next_value::<serde_json::Value>()?),
                        _ => return Err(serde::de::Error::unknown_field(&key, FIELDS)),
                    }
                }
                let id = id.ok_or_else(|| serde::de::Error::missing_field("id"))?;
                if id.len() > 128 {
                    return Err(serde::de::Error::custom("id over limit"));
                }
                let symbol = match symbol {
                    Some(symbol) if symbol.len() <= limits::TOKEN_BYTES => Some(symbol),
                    Some(_) => return Err(serde::de::Error::custom("symbol over limit")),
                    None => None,
                };
                let namespace = ProviderNamespace::new(
                    namespace.ok_or_else(|| serde::de::Error::missing_field("namespace"))?,
                )
                .ok_or_else(|| serde::de::Error::custom("malformed namespace"))?;
                let original_code = OriginalCode::new(
                    original_code
                        .ok_or_else(|| serde::de::Error::missing_field("original_code"))?,
                )
                .ok_or_else(|| serde::de::Error::custom("malformed original_code"))?;
                let source = match (path, range) {
                    (None, None) => None,
                    (path, range) => Some(SourceLocation { path, range }),
                };
                let data = match data {
                    Some(value) => super::types::data_object_strict(&value)
                        .ok_or_else(|| serde::de::Error::custom("malformed data"))?,
                    None => DataObject::new(),
                };
                Ok(Self::Value {
                    id,
                    symbol,
                    source,
                    namespace,
                    original_code,
                    data,
                })
            }
        }

        const FIELDS: &[&str] = &[
            "id",
            "symbol",
            "path",
            "range",
            "namespace",
            "original_code",
            "data",
        ];
        deserializer.deserialize_map(WireVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::normalize::DiagnosticSet;
    use crate::result::Status;

    fn wire(id: &str, path: Option<&str>) -> ProviderDiagnosticWire {
        let namespace =
            ProviderNamespace::new("com.example.provider:1.0.0".to_owned()).expect("namespace");
        let original = OriginalCode::new("E5123".to_owned()).expect("original code");
        ProviderDiagnosticWire {
            id: id.to_owned(),
            symbol: None,
            source: path.map(|path| SourceLocation {
                path: Some(path.to_owned()),
                range: None,
            }),
            namespace,
            original_code: original,
            data: DataObject::new(),
        }
    }

    #[test]
    fn provider_original_code_is_namespaced_and_preserved() {
        let diagnostic = normalize(wire("adapter.diagnostic-invalid", None)).expect("normalizes");
        let namespace = diagnostic.metadata.keys().next().expect("namespace key");
        assert_eq!(namespace, "com.example.provider:1.0.0");
        assert_eq!(
            diagnostic.metadata[namespace]["original_code"].as_str(),
            "E5123"
        );
        let set = DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Invalid)
            .expect("provider output is a valid set");
        assert_eq!(set.reason_ids(), ["adapter.diagnostic-invalid"]);
    }

    #[test]
    fn unsafe_paths_are_refused_not_rewritten() {
        for path in [
            "../escape.yaml",
            "/absolute/project.yaml",
            "C:\\windows\\project.yaml",
            "a/../../b.yaml",
        ] {
            assert_eq!(
                normalize(wire("adapter.diagnostic-invalid", Some(path))),
                Err(ProviderRejection::UnsafePath),
                "{path}"
            );
        }
    }

    #[test]
    fn unregistered_rules_are_refused() {
        assert_eq!(
            normalize(wire("loader.not-a-rule", None)),
            Err(ProviderRejection::UnknownRule)
        );
    }

    #[test]
    fn over_limit_input_becomes_one_bounded_infrastructure_diagnostic() {
        let diagnostic = rejection_diagnostic("a provider proposed a rule that is not registered");
        assert_eq!(diagnostic.id(), "adapter.diagnostic-invalid");
        assert_eq!(diagnostic.code(), "LEK-ADP-001");
        let set = DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Invalid)
            .expect("rejection is a valid set");
        assert_eq!(set.reason_ids(), ["adapter.diagnostic-invalid"]);
    }

    #[test]
    fn wire_deserialization_is_closed_and_bounded() {
        let text = "{\"id\": \"adapter.diagnostic-invalid\", \"namespace\": \
             \"com.example:1.0.0\", \"original_code\": \"E1\", \"unknown\": 1}";
        let result: Result<ProviderDiagnosticWire, _> = serde_json::from_str(text);
        assert!(result.is_err(), "unknown fields are refused");

        let long_code = "x".repeat(limits::ORIGINAL_CODE_BYTES + 1);
        let text = format!(
            "{{\"id\": \"adapter.diagnostic-invalid\", \"namespace\": \"com.example:1.0.0\", \
             \"original_code\": \"{long_code}\"}}"
        );
        let result: Result<ProviderDiagnosticWire, _> = serde_json::from_str(&text);
        assert!(result.is_err(), "over-limit original codes are refused");
    }
}
