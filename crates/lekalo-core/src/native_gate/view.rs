//! The composite observed view of the native gates (issue #48): the
//! read-only join of gate receipts onto current observed records by
//! native id, symbol, fingerprint, and revision. Missing evidence is
//! unknown; staleness degrades to incomplete. This never extends the
//! frozen observed-index or observed-scan JSON documents.

use serde::Serialize;

use super::{NativeRunResult, VIEW_SCHEMA_VERSION};

/// One gate row of the composite view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NativeGateViewRow {
    pub command_id: String,
    pub package_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
    /// The closed state set of the view row: planned receipts, terminal
    /// receipt outcomes, and the explicit not-run state.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_codes: Option<Vec<String>>,
}

/// The composite native gate observed view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NativeObservedView {
    pub schema_version: String,
    pub kind: String,
    pub completeness: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completeness_detail: Option<String>,
    pub scan_revision: String,
    pub observed_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_digest: Option<String>,
    pub gates: Vec<NativeGateViewRow>,
}

/// Project one terminal run receipt into the composite observed view
/// rows. The projection is read-only and deterministic: the same
/// receipt always produces byte-identical view JSON.
pub fn build_view(
    receipt: &NativeRunResult,
    scan_revision: &str,
    observed_ref: &str,
) -> NativeObservedView {
    let gates = receipt
        .commands
        .iter()
        .map(|command| NativeGateViewRow {
            command_id: command.command_id.clone(),
            package_id: command.package_id.clone(),
            gate: None,
            state: command.outcome.clone(),
            exit: command.exit.as_ref().and_then(|exit| exit.value),
            duration_ms: command
                .duration_ms
                .as_ref()
                .and_then(|duration| duration.value),
            reason_codes: if command.reason_codes.is_empty() {
                None
            } else {
                Some(command.reason_codes.clone())
            },
        })
        .collect();
    let completeness = if receipt.outcome == "security" || receipt.outcome == "infrastructure" {
        "incomplete"
    } else {
        "complete"
    };
    NativeObservedView {
        schema_version: VIEW_SCHEMA_VERSION.to_owned(),
        kind: "native-gate-view".to_owned(),
        completeness: completeness.to_owned(),
        completeness_detail: None,
        scan_revision: scan_revision.to_owned(),
        observed_ref: observed_ref.to_owned(),
        plan_digest: Some(receipt.plan_digest.clone()),
        run_digest: None,
        gates,
    }
}

/// Validate one view document against its closed shape (the schema
/// document is `contracts/native-gate-view.schema.v0.3.2.json`).
pub fn validate_view(view: &NativeObservedView) -> Result<(), &'static str> {
    if view.schema_version != VIEW_SCHEMA_VERSION || view.kind != "native-gate-view" {
        return Err("schema-version");
    }
    if !matches!(
        view.completeness.as_str(),
        "complete" | "incomplete" | "unknown"
    ) {
        return Err("completeness");
    }
    if !crate::target_protocol::wire::is_sha256_digest(&view.scan_revision)
        || !crate::target_protocol::wire::is_sha256_digest(&view.observed_ref)
    {
        return Err("refs");
    }
    if let Some(digest) = &view.plan_digest {
        if !crate::target_protocol::wire::is_sha256_digest(digest) {
            return Err("plan-digest");
        }
    }
    for gate in &view.gates {
        if ![
            "planned",
            "passed",
            "failed",
            "missing",
            "blocked",
            "unsupported",
            "infrastructure",
            "security",
            "not-run",
        ]
        .contains(&gate.state.as_str())
        {
            return Err("gate-state");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden_run() -> NativeRunResult {
        let bytes = include_bytes!(
            "../../../../tests/fixtures/node-native-gates/protocol/run-result.golden.json"
        );
        serde_json::from_slice(bytes).expect("golden run decodes")
    }

    #[test]
    fn the_view_projection_is_deterministic_and_valid() {
        let receipt = golden_run();
        let view = build_view(
            &receipt,
            &format!("sha256:{}", "2".repeat(64)),
            &format!("sha256:{}", "3".repeat(64)),
        );
        assert!(validate_view(&view).is_ok());
        let first = serde_json::to_vec(&view).unwrap();
        let second = serde_json::to_vec(&build_view(
            &receipt,
            &format!("sha256:{}", "2".repeat(64)),
            &format!("sha256:{}", "3".repeat(64)),
        ))
        .unwrap();
        assert_eq!(first, second, "the same receipt projects byte-identically");
    }

    #[test]
    fn a_security_outcome_degrades_completeness() {
        let mut receipt = golden_run();
        receipt.outcome = "security".into();
        let view = build_view(
            &receipt,
            &format!("sha256:{}", "2".repeat(64)),
            &format!("sha256:{}", "3".repeat(64)),
        );
        assert_eq!(view.completeness, "incomplete");
    }
}
