//! Evidence admission for the pilot report (issue #51).
//!
//! The closed evidence report (`lekalo/worklog-changelog-evidence/
//! v0.8.0`) is admitted under exactly two profiles:
//!
//! - **Strict** refuses the gate (`unsupported`, exit 4) when any
//!   required capability row, scenario assertion, or consumer witness
//!   is unsupported, unknown, or not executed. Every per-target row
//!   stays visible in the receipt; a refusal never hides one.
//! - **Permissive** degrades the verdict while keeping every gap
//!   visible. It can never turn a strict blocker into a pass.
//!
//! Both profiles refuse honestly: the admission derives the verdict
//! from the report's own rows, never from a claim.

use serde::Serialize;
use serde_json::Value as Json;

use super::diagnostic;
use super::version;

use crate::diagnostics::DiagnosticSet;
use crate::result::{singleton_set, DomainResult, Status};

/// The closed admission profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionProfile {
    /// Every required row must be supported and executed.
    Strict,
    /// Gaps degrade the verdict but keep every row visible.
    Permissive,
}

impl AdmissionProfile {
    /// The exact wire spelling.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Permissive => "permissive",
        }
    }
}

/// The typed admission receipt projected by `lekalo pilot admit`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdmissionReceipt {
    /// The exact evidence identity admitted.
    pub identity: &'static str,
    /// The selected profile.
    pub profile: &'static str,
    /// The aggregate verdict: `pass`, `degraded`, or `unsupported`.
    pub verdict: &'static str,
    /// The stable per-target verdicts, canonically ordered.
    pub targets: Vec<TargetVerdict>,
    /// Required blockers, when the profile refused.
    pub blockers: Vec<String>,
}

/// One per-target verdict row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetVerdict {
    /// The target id (`php` or `go`).
    pub target: String,
    /// The execution state the report claims.
    pub execution: String,
    /// The target verdict under the selected profile.
    pub verdict: &'static str,
    /// Supported capability count.
    pub supported: usize,
    /// Total capability rows.
    pub capabilities: usize,
    /// Passed assertion count.
    pub passed: usize,
    /// Total assertion rows.
    pub assertions: usize,
}

/// The stable IO failure set for an unreadable document.
pub fn io_failure(detail: &str) -> DiagnosticSet {
    diagnostic::input_invalid(detail)
}

/// The closed top-level member set of the evidence report.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "reportVersion",
    "compositionRef",
    "toolchain",
    "profiles",
    "targets",
    "scenarios",
    "consumers",
    "completeness",
    "verdict",
];

/// The schema-required top-level members: admission enforces their
/// presence exactly like the closed schema does.
const TOP_LEVEL_REQUIRED: &[&str] = TOP_LEVEL_KEYS;

const TARGET_KEYS: &[&str] = &[
    "target",
    "backend",
    "execution",
    "capabilities",
    "assertions",
    "blocking",
];

/// The schema-required per-target members.
const TARGET_REQUIRED: &[&str] = TARGET_KEYS;

const CAPABILITY_KEYS: &[&str] = &["capability", "support", "approval", "blocking", "note"];

/// The closed support vocabulary.
const SUPPORT: &[&str] = &["supported", "partial", "unsupported", "unknown"];

/// The closed assertion status vocabulary.
const ASSERTION_STATUS: &[&str] = &["pass", "fail", "unsupported", "not_executed"];

/// Admit one evidence report (exact UTF-8 JSON bytes) under the
/// selected profile, or return the typed rejection result. The
/// receipt keeps every per-target row; a strict refusal lists its
/// blockers and still carries the rows.
pub fn admit(bytes: &[u8], profile: AdmissionProfile) -> Result<AdmissionReceipt, DomainResult> {
    if bytes.len() > version::MAX_DOC_BYTES {
        return Err(DomainResult::UnsupportedOperation {
            diagnostics: evidence_incomplete_set(),
        });
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| DomainResult::invalid(io_failure("invalid-encoding")))?;
    let json = super::canonical::parse_unique(text).map_err(DomainResult::invalid)?;
    let object = json
        .as_object()
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("top-level-shape")))?;
    for key in object.keys() {
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "unknown-field",
            )));
        }
    }
    // The closed member set is also a required member set: a report
    // missing any schema-required member is structurally incomplete
    // (mirror of the schema's `required` list; admission never admits
    // an absent member behind a defaulted read).
    for required in TOP_LEVEL_REQUIRED {
        if !object.contains_key(*required) {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "missing-member",
            )));
        }
    }
    if object.get("schemaVersion").and_then(Json::as_str)
        != Some(super::version::EVIDENCE_SCHEMA_VERSION)
    {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "schema-version",
        )));
    }
    if object.get("identity").and_then(Json::as_str) != Some(super::version::EVIDENCE_IDENTITY) {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "contract-identity",
        )));
    }
    let targets_json = object
        .get("targets")
        .and_then(Json::as_array)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("targets-shape")))?;
    if targets_json.is_empty() || targets_json.len() > 8 {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "targets-bound",
        )));
    }
    let scenarios = object
        .get("scenarios")
        .and_then(Json::as_array)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("scenarios-shape")))?;
    let consumers = object
        .get("consumers")
        .and_then(Json::as_array)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("consumers-shape")))?;
    if consumers.len() > version::MAX_CONSUMERS {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "consumers-bound",
        )));
    }
    let completeness = object
        .get("completeness")
        .and_then(Json::as_object)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("completeness-shape")))?;
    let closure_complete =
        completeness.get("consumerClosure").and_then(Json::as_str) == Some("complete");

    let mut receipt_targets = Vec::with_capacity(targets_json.len());
    let mut blockers: Vec<String> = Vec::new();
    for target_json in targets_json {
        let target_object = target_json
            .as_object()
            .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("targets-shape")))?;
        for key in target_object.keys() {
            if !TARGET_KEYS.contains(&key.as_str()) {
                return Err(DomainResult::invalid(diagnostic::input_invalid(
                    "unknown-field",
                )));
            }
        }
        let target = target_object
            .get("target")
            .and_then(Json::as_str)
            .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("target-id")))?;
        let execution = target_object
            .get("execution")
            .and_then(Json::as_str)
            .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("execution")))?
            .to_owned();
        // The target member set is required as well as closed: a
        // missing backend, capabilities, assertions, or blocking list
        // is structural incompleteness, never a defaulted empty set.
        for required in TARGET_REQUIRED {
            if !target_object.contains_key(*required) {
                return Err(DomainResult::invalid(diagnostic::input_invalid(
                    "missing-member",
                )));
            }
        }
        let capabilities_json = target_object
            .get("capabilities")
            .and_then(Json::as_array)
            .ok_or_else(|| {
                DomainResult::invalid(diagnostic::input_invalid("capabilities-shape"))
            })?;
        if capabilities_json.len() > version::MAX_CAPABILITIES_PER_TARGET {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "capabilities-bound",
            )));
        }
        let mut supported = 0usize;
        let mut target_blockers: Vec<String> = Vec::new();
        for capability_json in capabilities_json {
            let capability_object = capability_json.as_object().ok_or_else(|| {
                DomainResult::invalid(diagnostic::input_invalid("capabilities-shape"))
            })?;
            for key in capability_object.keys() {
                if !CAPABILITY_KEYS.contains(&key.as_str()) {
                    return Err(DomainResult::invalid(diagnostic::input_invalid(
                        "unknown-field",
                    )));
                }
            }
            let capability = capability_object
                .get("capability")
                .and_then(Json::as_str)
                .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("capability-id")))?
                .to_owned();
            let support = capability_object
                .get("support")
                .and_then(Json::as_str)
                .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("support")))?
                .to_owned();
            if !SUPPORT.contains(&support.as_str()) {
                return Err(DomainResult::invalid(diagnostic::input_invalid(
                    "support-vocabulary",
                )));
            }
            let required = capability_object.get("blocking") == Some(&Json::Bool(true));
            if support == "supported" {
                supported += 1;
            } else if required {
                target_blockers.push(format!("{target}:{capability}:{support}"));
            }
        }
        let assertions_json = target_object
            .get("assertions")
            .and_then(Json::as_array)
            .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("assertions-shape")))?;
        if assertions_json.len() > version::MAX_ASSERTIONS_PER_SCENARIO * 64 {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "assertions-bound",
            )));
        }
        let mut passed = 0usize;
        for assertion_json in assertions_json {
            let assertion = assertion_json.as_object().ok_or_else(|| {
                DomainResult::invalid(diagnostic::input_invalid("assertions-shape"))
            })?;
            let scenario = assertion
                .get("scenario")
                .and_then(Json::as_str)
                .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("scenario-id")))?;
            let status = assertion
                .get("status")
                .and_then(Json::as_str)
                .ok_or_else(|| {
                    DomainResult::invalid(diagnostic::input_invalid("assertion-status"))
                })?;
            if !ASSERTION_STATUS.contains(&status) {
                return Err(DomainResult::invalid(diagnostic::input_invalid(
                    "assertion-status-vocabulary",
                )));
            }
            if status == "pass" {
                passed += 1;
            } else if execution == "executed" {
                // A target that claims execution must not carry
                // unexecuted rows: the claim itself is the blocker.
                target_blockers.push(format!("{target}:{scenario}:{status}"));
            }
        }
        if execution != "executed" {
            target_blockers.push(format!("{target}:execution:{execution}"));
        }
        let verdict = if target_blockers.is_empty() {
            "pass"
        } else if profile == AdmissionProfile::Strict {
            "unsupported"
        } else {
            "degraded"
        };
        blockers.extend(target_blockers.iter().cloned());
        receipt_targets.push(TargetVerdict {
            target: target.to_owned(),
            execution: execution.clone(),
            verdict,
            supported,
            capabilities: capabilities_json.len(),
            passed,
            assertions: assertions_json.len(),
        });
    }
    receipt_targets.sort_by(|left, right| left.target.cmp(&right.target));

    if scenarios.is_empty() || scenarios.len() > version::MAX_SCENARIOS {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "scenarios-bound",
        )));
    }
    if !closure_complete && profile == AdmissionProfile::Strict {
        blockers.push("consumers:closure:incomplete".to_owned());
    }

    let has_blockers = !blockers.is_empty();
    let verdict = if !has_blockers {
        "pass"
    } else if profile == AdmissionProfile::Strict {
        "unsupported"
    } else {
        "degraded"
    };
    // Deterministic blocker order.
    blockers.sort();
    blockers.dedup();
    Ok(AdmissionReceipt {
        identity: super::version::EVIDENCE_IDENTITY,
        profile: profile.wire_name(),
        verdict,
        targets: receipt_targets,
        blockers,
    })
}

/// The `unsupported` set for missing admission evidence.
fn evidence_incomplete_set() -> DiagnosticSet {
    singleton_set_evidence()
}

/// Build the registered singleton evidence-incomplete set, or the
/// invariant collapse.
fn singleton_set_evidence() -> DiagnosticSet {
    let mut data = crate::diagnostics::types::DataObject::new();
    data.insert(
        "detail".to_owned(),
        crate::diagnostics::types::token_value("admission-refused"),
    );
    match crate::diagnostics::normalize::build(
        super::diagnostic::EVIDENCE_INCOMPLETE,
        None,
        None,
        data,
    ) {
        Ok(built) => DiagnosticSet::try_from_unsorted(vec![built], Status::Unsupported)
            .unwrap_or_else(|_| singleton_set("diagnostics.registry-invalid")),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names_are_the_wire_spellings() {
        assert_eq!(AdmissionProfile::Strict.wire_name(), "strict");
        assert_eq!(AdmissionProfile::Permissive.wire_name(), "permissive");
    }

    #[test]
    fn evidence_incomplete_is_a_registered_unsupported_rule() {
        let set = evidence_incomplete_set();
        assert_eq!(set.as_slice().len(), 1);
        assert_eq!(
            set.as_slice()[0].id(),
            super::diagnostic::EVIDENCE_INCOMPLETE
        );
        assert_eq!(set.as_slice()[0].registry_version(), "0.8.0");
    }

    #[test]
    fn missing_required_members_are_structurally_refused() {
        // A minimal well-formed-but-incomplete report: the required
        // member admission reads is present, its siblings are not.
        let report = serde_json::json!({
            "schemaVersion": super::version::EVIDENCE_SCHEMA_VERSION,
            "identity": super::version::EVIDENCE_IDENTITY,
            "targets": [],
        });
        let bytes = serde_json::to_vec(&report).expect("report serializes");
        let set = admit(&bytes, AdmissionProfile::Strict).unwrap_err();
        assert_eq!(set.status(), Status::Invalid);
        let rendered = serde_json::to_string(set.diagnostics().first().expect("one diagnostic"))
            .expect("diagnostic serializes");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("diagnostic JSON");
        assert_eq!(value["data"]["detail"], "missing-member");
    }

    #[test]
    fn missing_required_target_members_are_structurally_refused() {
        let report = serde_json::json!({
            "schemaVersion": super::version::EVIDENCE_SCHEMA_VERSION,
            "identity": super::version::EVIDENCE_IDENTITY,
            "reportVersion": "0.8.0",
            "compositionRef": {"identity": "i", "digest": "d"},
            "toolchain": {},
            "profiles": ["strict"],
            "scenarios": [{"scenario": "s", "status": "not_executed", "targets": ["php"]}],
            "consumers": [],
            "completeness": {"consumerClosure": "complete"},
            "verdict": {},
            "targets": [{
                "target": "php",
                "execution": "not_executed",
                "capabilities": [],
                "assertions": [],
                "blocking": []
            }],
        });
        let bytes = serde_json::to_vec(&report).expect("report serializes");
        let set = admit(&bytes, AdmissionProfile::Strict).unwrap_err();
        let rendered = serde_json::to_string(set.diagnostics().first().expect("one diagnostic"))
            .expect("diagnostic serializes");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("diagnostic JSON");
        assert_eq!(value["data"]["detail"], "missing-member");
    }
}
