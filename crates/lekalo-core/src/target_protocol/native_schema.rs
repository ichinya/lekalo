//! Independent closed admission for lossless native-plan exchanges.
//! The embedded schema never retrieves a host file or network resource.
use jsonschema::{Draft, Retrieve, Uri, Validator};
use serde_json::Value;
use std::sync::OnceLock;

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

pub(super) fn valid(bytes: &[u8]) -> bool {
    static VALIDATOR: OnceLock<Result<Validator, ()>> = OnceLock::new();
    let validator = VALIDATOR.get_or_init(|| {
        let schema = serde_json::from_slice(include_bytes!(
            "../../../../contracts/target-protocol.schema.v0.9.1.json"
        ))
        .map_err(|_| ())?;
        compile(&schema)
    });
    let value = std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| crate::classification::parse_checked(text).ok());
    match (validator, value) {
        (Ok(validator), Some(value)) => validator.is_valid(&value),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_are_embedded_only_and_malformed_schemas_refuse() {
        for value in [
            serde_json::json!({"$ref":"https://example.invalid/native.json"}),
            serde_json::json!({"$ref":"file:///native.json"}),
            serde_json::json!({"type":"not-a-type"}),
        ] {
            assert!(compile(&value).is_err());
        }
    }

    #[test]
    fn complete_native_plans_bind_digest_summary_and_envelope_identity() {
        let plan = crate::native_gate::validate_plan(include_bytes!(
            "../../../../tests/fixtures/node-native-gates/protocol/plan.golden.json"
        ))
        .unwrap();
        let response = serde_json::json!({"protocol":"lekalo.target/v1", "protocol_version":"0.9.1", "operation":"plan-native", "status":"ok", "request_id":format!("req-{}","0".repeat(64)),
            "evidence":{"adapter":plan.adapter}, "result":{"native_plan":{"kind":"native-plan","digest":plan.plan_digest,"commands":plan.commands.len(),"packages":plan.workspace.packages.len(),"plan":plan}}});
        let encode = |value: &Value| serde_json::to_vec(value).unwrap();
        assert!(super::super::wire::decode_response(&encode(&response)).is_ok());
        let mut missing = response.clone();
        missing["result"]["native_plan"]
            .as_object_mut()
            .unwrap()
            .remove("plan");
        assert!(super::super::wire::decode_response(&encode(&missing)).is_err());
        let mut stale = response.clone();
        stale["result"]["native_plan"]["plan"]["planner_version"] = serde_json::json!("0.4.1");
        assert!(super::super::wire::decode_response(&encode(&stale)).is_err());
        let mut summary = response.clone();
        summary["result"]["native_plan"]["commands"] = serde_json::json!(0);
        assert!(super::super::wire::decode_response(&encode(&summary)).is_err());
        let mut identity = response.clone();
        let mut foreign = plan.clone();
        foreign.adapter.version = "99.0.0".into();
        foreign.plan_digest = crate::native_gate::plan_digest(&foreign);
        identity["result"]["native_plan"]["digest"] = serde_json::json!(foreign.plan_digest);
        identity["result"]["native_plan"]["plan"] = serde_json::to_value(foreign).unwrap();
        assert!(super::super::wire::decode_response(&encode(&identity)).is_err());
        let mut predecessor = response.clone();
        predecessor["protocol_version"] = serde_json::json!("0.9.0");
        assert!(super::super::wire::decode_response(&encode(&predecessor)).is_err());
        let raw = serde_json::to_string(&response).unwrap();
        let key = format!("\"planner_version\":\"{}\"", plan.planner_version);
        let duplicate = raw.replace(
            &key,
            &format!(
                "{key},\"planner_versio\\u006e\":\"{}\"",
                plan.planner_version
            ),
        );
        assert_ne!(raw, duplicate);
        assert!(super::super::wire::decode_response(duplicate.as_bytes()).is_err());
    }
}
