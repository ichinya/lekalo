//! Admission against the frozen, closed history contract, in addition to
//! the receipt/chain/custody checks. No imported prefix may be adopted or
//! republished merely because its public hashes are internally consistent.

use jsonschema::{Draft, Retrieve, Uri, Validator};
use serde_json::Value;
use std::sync::OnceLock;

use super::ChangeError;

struct EmbeddedOnly;

impl Retrieve for EmbeddedOnly {
    fn retrieve(
        &self,
        _uri: &Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err("external schema retrieval is forbidden".into())
    }
}

fn compile(schema: &Value) -> Result<Validator, ()> {
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .with_retriever(EmbeddedOnly)
        .build(schema)
        .map_err(|_| ())
}

pub(super) fn validate(value: &Value) -> Result<(), ChangeError> {
    static VALIDATOR: OnceLock<Result<Validator, ()>> = OnceLock::new();
    let invalid = || ChangeError::HistoryInvalid {
        detail: "history-schema",
    };
    let validator = VALIDATOR
        .get_or_init(|| {
            let schema = serde_json::from_slice(include_bytes!(
                "../../../../contracts/change-history.schema.v0.8.0.json"
            ))
            .map_err(|_| ())?;
            compile(&schema)
        })
        .as_ref()
        .map_err(|_| invalid())?;
    if validator.is_valid(value) {
        Ok(())
    } else {
        // Never expose untrusted operation values, paths, or schema URIs.
        Err(invalid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn history_schema_is_embedded_only_and_fails_closed() {
        for schema in [
            json!({"type": "not-a-type"}),
            json!({"$ref": "https://example.invalid/history.json"}),
            json!({"$ref": "file:///private-history.json"}),
        ] {
            assert!(compile(&schema).is_err());
        }
        let golden = serde_json::from_slice(include_bytes!(
            "../../../../tests/fixtures/change/golden/change-history.json"
        ))
        .unwrap();
        validate(&golden).unwrap();
        assert!(validate(&json!({})).is_err());
    }
}
