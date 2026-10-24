//! Closed JSON Lines watch events. The decoder uses the same offline
//! Draft 2020-12 schema as the external gate, plus lifecycle consistency
//! checks; it refuses duplicate keys before serde can erase them.

use serde_json::{json, Value};
use std::sync::LazyLock;

use super::version::{EVENT_IDENTITY, EVENT_SCHEMA_VERSION};
use crate::result::Status;

pub const EVENTS: [&str; 6] = [
    "session-started",
    "run-started",
    "run-completed",
    "run-cancelled",
    "recovery-required",
    "session-stopped",
];
pub const RECOVERY_CAUSES: [&str; 4] = [
    "overflow",
    "ambiguous-rename",
    "subscription-lost",
    "inventory-mismatch",
];
pub const STOP_REASONS: [&str; 4] = [
    "interrupt",
    "stop-after-runs",
    "output-failure",
    "terminal-refusal",
];
pub const MAX_EVENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct WatchEvent(Value);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodeError {
    DuplicateKey,
    Malformed,
    Limit,
}

fn validator() -> &'static jsonschema::Validator {
    static VALIDATOR: LazyLock<jsonschema::Validator> = LazyLock::new(|| {
        let schema: Value = serde_json::from_slice(include_bytes!(
            "../../../../contracts/watch-event.schema.v0.8.14.json"
        ))
        .expect("embedded watch schema JSON");
        jsonschema::validator_for(&schema).expect("offline closed watch schema")
    });
    &VALIDATOR
}

/// Draft 2020-12 integer semantics also accept JSON spellings such as
/// `1.0`/`1e0`. Convert only exact nonnegative safe integers; validated
/// events must never panic in getters because of a numeric spelling.
pub(crate) fn counter(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|n| {
                n.is_finite() && *n >= 0.0 && *n <= 9_007_199_254_740_991.0 && n.fract() == 0.0
            })
            .map(|n| n as u64)
    })
}

impl WatchEvent {
    fn new(sequence: u64, session_id: &str, event: &str, payload: Value) -> Self {
        let mut value = json!({"schemaVersion":EVENT_SCHEMA_VERSION,"identity":EVENT_IDENTITY,"sequence":sequence,"sessionId":session_id,"event":event});
        value
            .as_object_mut()
            .expect("object")
            .extend(payload.as_object().expect("payload object").clone());
        Self(value)
    }
    pub fn event(&self) -> &str {
        self.0["event"].as_str().expect("event discriminator")
    }
    pub fn sequence(&self) -> u64 {
        counter(&self.0["sequence"]).expect("sequence")
    }
    pub fn run_id(&self) -> Option<u64> {
        self.0.get("runId").and_then(counter)
    }
    pub fn value(&self) -> &Value {
        &self.0
    }
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(&self.0).expect("JSON value serializes")
    }
    pub fn decode(line: &str) -> Result<Self, DecodeError> {
        if line.len() > MAX_EVENT_BYTES {
            return Err(DecodeError::Limit);
        }
        let value = crate::adoption::wire::unique_json(line.as_bytes()).map_err(|_| {
            if serde_json::from_str::<Value>(line).is_ok() {
                DecodeError::DuplicateKey
            } else {
                DecodeError::Malformed
            }
        })?;
        if !validator().is_valid(&value) || !consistent(&value) {
            return Err(DecodeError::Malformed);
        }
        Ok(Self(value))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn session_started(
        sequence: u64,
        session_id: &str,
        backend: &str,
        fallback: bool,
        stages: Vec<&str>,
        module: Option<String>,
        debounce_ms: u64,
        max_wait_ms: u64,
    ) -> Self {
        let mut payload = json!({"backend":backend,"fallback":fallback,"stages":stages,"debounceMs":debounce_ms,"maxWaitMs":max_wait_ms});
        if let Some(module) = module {
            payload["module"] = json!(module);
        }
        Self::new(sequence, session_id, "session-started", payload)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn run_started(
        sequence: u64,
        session_id: &str,
        run_id: u64,
        generation: u64,
        source_revision: &str,
        changed_files: u64,
        full_rescan: bool,
    ) -> Self {
        Self::new(
            sequence,
            session_id,
            "run-started",
            json!({"runId":run_id,"generation":generation,"sourceRevision":source_revision,"changedFiles":changed_files,"fullRescan":full_rescan}),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn run_completed(
        sequence: u64,
        session_id: &str,
        run_id: u64,
        generation: u64,
        source_revision: &str,
        status: Status,
        result: super::WatchResult,
    ) -> Self {
        Self::new(
            sequence,
            session_id,
            "run-completed",
            json!({"runId":run_id,"generation":generation,"sourceRevision":source_revision,"status":status.as_str(),"diagnosticsMode":"replace","result":result}),
        )
    }
    pub fn run_cancelled(
        sequence: u64,
        session_id: &str,
        run_id: u64,
        generation: u64,
        source_revision: &str,
        superseded_by_generation: u64,
    ) -> Self {
        Self::new(
            sequence,
            session_id,
            "run-cancelled",
            json!({"runId":run_id,"generation":generation,"sourceRevision":source_revision,"supersededByGeneration":superseded_by_generation,"committed":false}),
        )
    }
    pub fn recovery_required(sequence: u64, session_id: &str, cause: &str, action: &str) -> Self {
        Self::new(
            sequence,
            session_id,
            "recovery-required",
            json!({"cause":cause,"action":action}),
        )
    }
    pub fn recovery_update(
        sequence: u64,
        session_id: &str,
        cause: &str,
        backend: Option<&str>,
        refusal: Option<&crate::DomainResult>,
    ) -> Self {
        let mut payload = json!({"cause":cause,"action":"full-rescan"});
        if let Some(backend) = backend {
            payload["backend"] = json!(backend);
        }
        if let Some(refusal) = refusal {
            payload["status"] = json!(refusal.status().as_str());
            payload["diagnostics"] = json!(refusal.diagnostics());
        }
        Self::new(sequence, session_id, "recovery-required", payload)
    }
    /// Explicit fail-stop, not a claim that cancellation teardown succeeded.
    pub fn cancellation_incomplete(
        sequence: u64,
        session_id: &str,
        run_id: u64,
        generation: u64,
        source_revision: &str,
    ) -> Self {
        Self::new(
            sequence,
            session_id,
            "recovery-required",
            json!({
                "cause":"cancellation-incomplete", "action":"stop", "runId":run_id,
                "generation":generation, "sourceRevision":source_revision, "committed":false,
                "status":"unavailable", "diagnostics":[super::diagnostics::cancellation_incomplete("worker")],
            }),
        )
    }
    pub fn session_stopped(
        sequence: u64,
        session_id: &str,
        reason: &str,
        runs: u64,
        last_status: Option<Status>,
    ) -> Self {
        let mut payload = json!({"reason":reason,"runs":runs});
        if let Some(status) = last_status {
            payload["lastStatus"] = json!(status.as_str());
        }
        Self::new(sequence, session_id, "session-stopped", payload)
    }
}

fn consistent(value: &Value) -> bool {
    match value["event"].as_str() {
        Some("session-started") => counter(&value["maxWaitMs"]) >= counter(&value["debounceMs"]),
        Some("run-cancelled") => {
            counter(&value["supersededByGeneration"]) >= counter(&value["generation"])
        }
        Some("run-completed") => {
            let result = &value["result"];
            value["sourceRevision"] == result["sourceRevision"]
                && value["status"] == result["status"]
                && super::result_value_is_wellformed(result)
        }
        _ => true,
    }
}

pub fn line_contains_duplicate_keys(line: &str) -> bool {
    serde_json::from_str::<Value>(line).is_ok()
        && crate::adoption::wire::unique_json(line.as_bytes()).is_err()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_keys_include_escaped_spellings_and_nested_objects() {
        for line in [r#"{"a":1,"a":2}"#, r#"{"a":{"b":1,"\u0062":2}}"#] {
            assert!(line_contains_duplicate_keys(line));
        }
        assert!(!line_contains_duplicate_keys(r#"[{"a":1},{"a":2}]"#));
    }
    #[test]
    fn integral_float_spellings_follow_draft_integer_semantics() {
        let event = WatchEvent::session_started(
            1,
            "s-0123",
            "poll",
            false,
            vec!["validate"],
            None,
            150,
            1000,
        );
        let line = event
            .to_json_line()
            .replace("\"sequence\":1", "\"sequence\":1e0");
        assert_eq!(WatchEvent::decode(&line).unwrap().sequence(), 1);
        assert_eq!(
            WatchEvent::decode(&line.replace("1e0", "1e-1")),
            Err(DecodeError::Malformed)
        );
    }

    #[test]
    fn event_roundtrip_and_closed_members() {
        let event = WatchEvent::session_started(
            1,
            "s-0123",
            "native",
            false,
            vec!["validate"],
            None,
            150,
            1000,
        );
        assert_eq!(WatchEvent::decode(&event.to_json_line()).unwrap(), event);
        let mut forged = event.value().clone();
        forged["extra"] = json!(1);
        assert_eq!(
            WatchEvent::decode(&forged.to_string()),
            Err(DecodeError::Malformed)
        );
        let cancelled =
            WatchEvent::run_cancelled(2, "s-0123", 1, 3, &format!("sha256:{}", "a".repeat(64)), 4);
        assert_eq!(
            WatchEvent::decode(&cancelled.to_json_line()).unwrap(),
            cancelled
        );
        let backwards =
            WatchEvent::run_cancelled(3, "s-0123", 2, 5, &format!("sha256:{}", "a".repeat(64)), 4);
        assert_eq!(
            WatchEvent::decode(&backwards.to_json_line()),
            Err(DecodeError::Malformed)
        );
    }
}
