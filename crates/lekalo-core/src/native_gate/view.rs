//! The composite observed view of the native gates (issue #48): the
//! read-only join of gate receipts and the workspace package graph onto
//! current observed records by native id, symbol, fingerprint and
//! revision. Missing evidence is unknown; staleness degrades to
//! incomplete. This never extends the frozen observed-index or
//! observed-scan JSON documents and never grants execution authority.

use serde::{Deserialize, Serialize};

use super::{NativePlan, NativeRunResult, VIEW_SCHEMA_VERSION};

/// One package row of the composite view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NativePackageViewRow {
    pub id: String,
    pub root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_commands: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_kinds: Option<Vec<String>>,
}

/// One symbol row of the composite view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NativeSymbolViewRow {
    pub native_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_id: Option<String>,
    pub package_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    pub freshness: String,
}

/// One gate row of the composite view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

/// One graph edge of the composite view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NativeGraphEdgeRow {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub provenance: String,
}

/// The graph evidence section of the composite view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NativeGraphEvidence {
    pub completeness: String,
    pub edges: Vec<NativeGraphEdgeRow>,
}

/// The composite native gate observed view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    pub packages: Vec<NativePackageViewRow>,
    pub symbols: Vec<NativeSymbolViewRow>,
    pub gates: Vec<NativeGateViewRow>,
    pub graph_evidence: NativeGraphEvidence,
}

/// Project one terminal run receipt plus its plan into the composite
/// observed view. The projection is read-only and deterministic: the
/// same receipt and plan always produce byte-identical view JSON.
pub fn build_view(
    receipt: &NativeRunResult,
    scan_revision: &str,
    observed_ref: &str,
    plan: &NativePlan,
) -> NativeObservedView {
    let gates = receipt
        .commands
        .iter()
        .map(|command| {
            let planned = plan
                .commands
                .iter()
                .find(|candidate| candidate.id == command.command_id);
            NativeGateViewRow {
                command_id: command.command_id.clone(),
                package_id: command.package_id.clone(),
                gate: planned.map(|candidate| candidate.gate.clone()),
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
            }
        })
        .collect();
    let degraded = receipt.outcome == "security" || receipt.outcome == "infrastructure";
    let view = NativeObservedView {
        schema_version: VIEW_SCHEMA_VERSION.to_owned(),
        kind: "native-gate-view".to_owned(),
        completeness: if degraded { "incomplete" } else { "complete" }.to_owned(),
        completeness_detail: None,
        scan_revision: scan_revision.to_owned(),
        observed_ref: observed_ref.to_owned(),
        plan_digest: Some(receipt.plan_digest.clone()),
        run_digest: None,
        packages: plan
            .workspace
            .packages
            .iter()
            .map(|package| {
                let affected = plan
                    .affected
                    .iter()
                    .any(|entry| entry.package_id == package.id);
                let selected_commands: Vec<String> = plan
                    .commands
                    .iter()
                    .filter(|command| command.package_id == package.id)
                    .map(|command| command.id.clone())
                    .collect();
                let reason_kinds: Vec<String> = plan
                    .affected
                    .iter()
                    .filter(|entry| entry.package_id == package.id)
                    .flat_map(|entry| entry.reasons.iter().map(|reason| reason.kind.clone()))
                    .collect();
                NativePackageViewRow {
                    id: package.id.clone(),
                    root: package.root.clone(),
                    affected: Some(affected),
                    selected_commands: if selected_commands.is_empty() {
                        None
                    } else {
                        Some(selected_commands)
                    },
                    reason_kinds: if reason_kinds.is_empty() {
                        None
                    } else {
                        Some(reason_kinds)
                    },
                }
            })
            .collect(),
        symbols: Vec::new(),
        gates,
        graph_evidence: NativeGraphEvidence {
            completeness: if plan.workspace.completeness == "complete" {
                "complete"
            } else {
                "incomplete"
            }
            .to_owned(),
            edges: plan
                .workspace
                .edges
                .iter()
                .map(|edge| NativeGraphEdgeRow {
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    kind: serde_json::to_value(edge.kind)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                    provenance: edge.provenance.clone(),
                })
                .collect(),
        },
    };
    view
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
        if !matches!(
            gate.state.as_str(),
            "planned"
                | "passed"
                | "failed"
                | "missing"
                | "blocked"
                | "unsupported"
                | "infrastructure"
                | "security"
                | "not-run"
        ) {
            return Err("gate-state");
        }
    }
    for package in &view.packages {
        let root_ok =
            package.root == "." || crate::target_protocol::scopes::is_logical_path(&package.root);
        if !root_ok || !crate::native_gate::wire::package_id_is_valid(&package.id) {
            return Err("package-id");
        }
    }
    Ok(())
}

#[cfg(test)]
mod view_tests {
    use super::*;

    #[test]
    fn the_golden_view_deserializes_and_validates() {
        let bytes = include_bytes!(
            "../../../../tests/fixtures/node-native-gates/protocol/view.golden.json"
        );
        let view: NativeObservedView = serde_json::from_slice(bytes)
            .map_err(|error| panic!("golden view decodes: {error}"))
            .expect("decodes");
        assert!(validate_view(&view).is_ok());
        assert_eq!(view.schema_version, VIEW_SCHEMA_VERSION);
        assert!(!view.packages.is_empty());
        assert!(!view.gates.is_empty());
        assert!(!view.graph_evidence.edges.is_empty());
    }
}
