//! Full frozen #121 contract admission, in addition to Store::get custody.
//! Only embedded schemas and their internal refs are resolvable. Validation
//! diagnostics never expose source values, schema paths or external URIs.
use super::{Error, Result};
use jsonschema::{Draft, Retrieve, Uri, Validator};
use serde_json::Value;
use std::sync::OnceLock;

struct EmbeddedOnly;

impl Retrieve for EmbeddedOnly {
    fn retrieve(
        &self,
        _uri: &Uri<String>,
    ) -> std::result::Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err("external schema retrieval is forbidden".into())
    }
}

fn compile(schema: &Value) -> std::result::Result<Validator, ()> {
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .should_validate_formats(true)
        .should_ignore_unknown_formats(false)
        .with_retriever(EmbeddedOnly)
        .build(schema)
        .map_err(|_| ())
}

type CachedValidator = OnceLock<std::result::Result<Validator, ()>>;

fn validate(value: &Value, bytes: &[u8], cache: &CachedValidator) -> Result<()> {
    let validator = cache
        .get_or_init(|| {
            let schema = serde_json::from_slice(bytes).map_err(|_| ())?;
            compile(&schema)
        })
        .as_ref()
        .map_err(|_| Error::Source)?;
    if validator.is_valid(value) {
        Ok(())
    } else {
        Err(Error::Source)
    }
}

pub(super) fn record(value: &Value) -> Result<()> {
    static VALIDATOR: CachedValidator = OnceLock::new();
    validate(
        value,
        include_bytes!("../../../../contracts/run-record.schema.v0.4.0.json"),
        &VALIDATOR,
    )
}

pub(super) fn assertions(value: &Value) -> Result<()> {
    static VALIDATOR: CachedValidator = OnceLock::new();
    validate(
        value,
        include_bytes!("../../../../contracts/run-assertions.schema.v0.4.0.json"),
        &VALIDATOR,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schema_compilation_and_resolution_fail_closed() {
        for schema in [
            json!({"type":"not-a-type"}),
            json!({"$ref":"https://example.invalid/private-schema.json"}),
            json!({"$ref":"file:///private-schema.json"}),
        ] {
            assert!(compile(&schema).is_err());
        }
        assert!(validate(&json!({}), b"{", &OnceLock::new()).is_err());
        assert!(validate(&json!({}), b"{\"type\":\"not-a-type\"}", &OnceLock::new()).is_err());
    }

    #[test]
    fn frozen_record_rejects_unchecked_nested_fields() {
        let valid: Value = serde_json::from_slice(include_bytes!(
            "../../../../tests/fixtures/run-history/valid/greenfield.json"
        ))
        .unwrap();
        record(&valid).unwrap();
        let mutations: &[(&str, Value)] = &[
            ("/recordedAt", json!("invalid-not-a-timestamp")),
            ("/operation/kind", json!("private-command")),
            ("/operation/affectedSemanticIds", json!(["bad/id"])),
            ("/measurementSources", json!([{"prompt":"private-canary"}])),
            ("/testGateSummaries", json!([{"prompt":"private-canary"}])),
            ("/diagnostics", json!([{"prompt":"private-canary"}])),
            (
                "/privacy/provenance/sourceRefs",
                json!(["https://private.invalid"]),
            ),
        ];
        for (pointer, value) in mutations {
            let mut changed = valid.clone();
            *changed.pointer_mut(pointer).unwrap() = value.clone();
            assert!(record(&changed).is_err(), "{pointer}");
        }
    }

    #[test]
    fn frozen_assertions_enforce_root_and_nested_contract() {
        let valid: Value = serde_json::from_slice(include_bytes!(
            "../../../../tests/fixtures/run-history/valid/assertion-set.json"
        ))
        .unwrap();
        assertions(&valid).unwrap();
        let mut root = valid.clone();
        root["prompt"] = json!("private-canary");
        assert!(assertions(&root).is_err());
        let mut scope = valid.clone();
        scope["scope"]["prompt"] = json!("private-canary");
        assert!(assertions(&scope).is_err());
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove("exportDisposition");
        assert!(assertions(&missing).is_err());
        for (pointer, value) in [
            ("/setId", json!("wrong-token")),
            ("/rows/0/evidenceRef", json!("invalid-evidence")),
            ("/rows/0/subjectSemanticId", json!("bad/id")),
            ("/rows/0/assertionId", json!("a".repeat(129))),
        ] {
            let mut changed = valid.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            assert!(assertions(&changed).is_err(), "{pointer}");
        }
        let mut duplicate = valid.clone();
        duplicate["rows"]
            .as_array_mut()
            .unwrap()
            .push(valid["rows"][0].clone());
        assert!(assertions(&duplicate).is_err());
        let mut over_bound = valid.clone();
        over_bound["rows"] = json!(vec![valid["rows"][0].clone(); 4097]);
        assert!(assertions(&over_bound).is_err());
    }
}
