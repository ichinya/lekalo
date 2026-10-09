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
//! Admission is a bound join, never a trust of report-controlled
//! lists: the report is decoded through the complete closed wire
//! contract (the same member sets, types, bounds, enums, and
//! additional-property rejections the published 0.8.0 schema
//! enforces), pinned to the exact composition document by its
//! domain-prefixed digest, and joined to the pinned compilation's
//! scenario catalog and the composition's required capability
//! inventory. A report author cannot remove a required guarantee by
//! clearing a flag or deleting a row: the required inventory comes
//! from the composition and the compilation, not from the report.
//!
//! Under the strict profile the obligations are derived, not
//! self-claimed: every required target must claim real execution
//! with a bound backend whose identity and digest equal the engine
//! shell's own provenance receipt for the same compilation, every
//! required scenario must carry a passing per-target assertion and a
//! non-failing aggregate row, a `partial` row satisfies its minimum
//! only behind an explicit review approval, and the consumer witness
//! set is joined both directions against the compilation's declared
//! consumer inventory. Evidence contradicting the derived truth
//! refuses regardless of what the report claims.

use serde::Serialize;
use serde_json::Value as Json;

use super::diagnostic;
use super::types::Composition;
use super::version;

use crate::diagnostics::DiagnosticSet;
use crate::ir::{CompiledProject, Definition};
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

/// One target backend's engine-side provenance, resolved by the shell
/// from the same compilation the composition pins. The report's
/// backend identity and digest must equal this receipt exactly: a
/// backend bound to bytes the engine did not produce is forged
/// provenance, not evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendProvenance {
    /// The exact backend identity the engine rendered.
    pub identity: String,
    /// The exact `sha256:<64 hex>` digest of the backend bytes the
    /// engine executed.
    pub digest: String,
}

// ---------------------------------------------------------------------------
// The closed wire contract: the exact mirror of the published 0.8.0
// evidence schema. Every member set is closed, every required member
// is required, every enum is closed, every string is bounded, and
// nested objects reject additional members. Required-key presence is
// not schema parity.
// ---------------------------------------------------------------------------

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

const COMPOSITION_REF_KEYS: &[&str] = &["identity", "digest"];
const TOOLCHAIN_KEYS: &[&str] = &["harness", "node", "ajv"];
const TARGET_KEYS: &[&str] = &[
    "target",
    "backend",
    "execution",
    "capabilities",
    "assertions",
    "blocking",
];
const BACKEND_KEYS: &[&str] = &["state", "identity", "digest"];
const BACKEND_REQUIRED: &[&str] = &["state"];
const CAPABILITY_KEYS: &[&str] = &["capability", "support", "approval", "blocking", "note"];
const CAPABILITY_REQUIRED: &[&str] = &["capability", "support", "blocking", "note"];
const ASSERTION_KEYS: &[&str] = &["scenario", "status"];
const SCENARIO_RESULT_KEYS: &[&str] = &["scenario", "status", "targets"];
const CONSUMER_KEYS: &[&str] = &["consumer", "witness", "relation"];
const COMPLETENESS_KEYS: &[&str] = &["consumerClosure", "coverage", "unknownExternalConsumers"];
const VERDICT_KEYS: &[&str] = &["strict", "permissive", "degraded"];

/// The closed target vocabulary.
const TARGETS: &[&str] = &["php", "go"];
/// The closed per-scenario target vocabulary (the reference evaluator
/// is an admitted scenario target; it is not a portability backend).
const SCENARIO_TARGETS: &[&str] = &["php", "go", "fake-reference"];
/// The closed support vocabulary.
const SUPPORT: &[&str] = &["supported", "partial", "unsupported", "unknown"];
/// The closed assertion status vocabulary.
const ASSERTION_STATUS: &[&str] = &["pass", "fail", "unsupported", "not_executed"];
/// The closed execution vocabulary.
const EXECUTION: &[&str] = &["executed", "not_executed"];
/// The closed backend-state vocabulary.
const BACKEND_STATE: &[&str] = &["bound", "absent"];
/// The closed approval vocabulary.
const APPROVAL: &[&str] = &["not_required", "reviewed"];
/// The closed profile vocabulary.
const PROFILES: &[&str] = &["strict", "permissive"];
/// The closed completeness vocabularies.
const CLOSURE: &[&str] = &["complete", "incomplete"];
const COVERAGE: &[&str] = &["full", "partial"];
/// The closed verdict-claim vocabularies.
const STRICT_CLAIM: &[&str] = &["pass", "denied", "unsupported"];
const PERMISSIVE_CLAIM: &[&str] = &["pass", "degraded"];

/// `^[A-Za-z0-9_.:/@-]+$`, 1..=256.
fn pin_text(raw: &str) -> Result<(), DiagnosticSet> {
    let ok = !raw.is_empty()
        && raw.len() <= 256
        && raw.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'/' | b'@' | b'-')
        });
    if ok {
        Ok(())
    } else {
        Err(diagnostic::input_invalid("pin-text"))
    }
}

/// The closed lowercase semantic-id shape: dotted lowercase segments,
/// optional slash-separated trailing segments, 1..=256.
fn id_text(raw: &str) -> Result<(), DiagnosticSet> {
    let segment_ok = |segment: &str| {
        !segment.is_empty() && segment.len() <= 256 && {
            let bytes = segment.as_bytes();
            bytes[0].is_ascii_lowercase()
                && bytes[1..].iter().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || *byte == b'_'
                        || *byte == b'-'
                })
        }
    };
    // The exact schema grammar: a bare segment, or a dotted head —
    // at least one dot — with optional slash-separated trailing
    // segments of at least two characters. A slash tail never stands
    // in for the dot the published pattern requires.
    let dotted: Vec<&str> = raw.split('/').collect();
    let head = dotted[0];
    let head_ok = {
        let parts: Vec<&str> = head.split('.').collect();
        !parts.is_empty() && parts.iter().all(|part| segment_ok(part))
    };
    let tail_ok = dotted[1..]
        .iter()
        .all(|part| segment_ok(part) && part.len() >= 2);
    let dotted_ok = head.contains('.');
    if !raw.is_empty() && raw.len() <= 256 && head_ok && tail_ok && (dotted_ok || dotted.len() == 1)
    {
        Ok(())
    } else {
        Err(diagnostic::input_invalid("id-text"))
    }
}

/// 1..=2000 scalar units, no control characters.
fn note_text(raw: &str) -> Result<(), DiagnosticSet> {
    let ok = !raw.is_empty()
        && raw.len() <= 2000
        && !raw.chars().any(|character| character.is_control());
    if ok {
        Ok(())
    } else {
        Err(diagnostic::input_invalid("note-text"))
    }
}

/// `sha256:` plus exactly 64 lowercase hex digits.
fn digest_text(raw: &str) -> Result<(), DiagnosticSet> {
    let rest = raw
        .strip_prefix("sha256:")
        .ok_or_else(|| diagnostic::input_invalid("digest-shape"))?;
    if rest.len() == 64
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(diagnostic::input_invalid("digest-shape"))
    }
}

fn closed<'v>(raw: &str, vocabulary: &[&'v str]) -> Result<&'v str, DiagnosticSet> {
    if let Some(found) = vocabulary.iter().find(|candidate| **candidate == raw) {
        Ok(*found)
    } else {
        Err(diagnostic::input_invalid("closed-vocabulary"))
    }
}

fn members(object: &serde_json::Map<String, Json>, keys: &[&str]) -> Result<(), DiagnosticSet> {
    for key in object.keys() {
        if !keys.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    Ok(())
}

fn required(object: &serde_json::Map<String, Json>, keys: &[&str]) -> Result<(), DiagnosticSet> {
    for key in keys {
        if !object.contains_key(*key) {
            return Err(diagnostic::input_invalid("missing-member"));
        }
    }
    Ok(())
}

fn text_member<'a>(
    object: &'a serde_json::Map<String, Json>,
    key: &str,
) -> Result<&'a str, DiagnosticSet> {
    object
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("member-type"))
}

/// One decoded evidence report.
#[derive(Debug)]
struct EvidenceReport {
    composition_digest: String,
    targets: Vec<DecodedTarget>,
    scenarios: Vec<DecodedScenario>,
    consumers: Vec<String>,
    closure_complete: bool,
    strict_claim: &'static str,
    permissive_claim: &'static str,
}

#[derive(Debug)]
struct DecodedTarget {
    target: String,
    backend_state: &'static str,
    backend_identity: Option<String>,
    backend_digest: Option<String>,
    execution: &'static str,
    capabilities: Vec<DecodedCapability>,
    assertions: Vec<(String, &'static str)>,
    blocking: Vec<String>,
}

#[derive(Debug)]
struct DecodedCapability {
    capability: String,
    support: &'static str,
    approval: Option<&'static str>,
    blocking: bool,
}

#[derive(Debug)]
struct DecodedScenario {
    scenario: String,
    status: &'static str,
}

/// Decode the complete closed wire contract, or refuse.
fn decode(text: &str) -> Result<EvidenceReport, DomainResult> {
    let json = super::canonical::parse_unique(text).map_err(DomainResult::invalid)?;
    let object = json
        .as_object()
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("top-level-shape")))?;
    members(object, TOP_LEVEL_KEYS).map_err(DomainResult::invalid)?;
    required(object, TOP_LEVEL_KEYS).map_err(DomainResult::invalid)?;
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::EVIDENCE_SCHEMA_VERSION)
    {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "schema-version",
        )));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::EVIDENCE_IDENTITY) {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "contract-identity",
        )));
    }
    let report_version = text_member(object, "reportVersion").map_err(DomainResult::invalid)?;
    if report_version != version::VERSION {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "report-version",
        )));
    }
    let composition_ref = object
        .get("compositionRef")
        .and_then(Json::as_object)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("composition-ref-shape")))?;
    members(composition_ref, COMPOSITION_REF_KEYS).map_err(DomainResult::invalid)?;
    required(composition_ref, COMPOSITION_REF_KEYS).map_err(DomainResult::invalid)?;
    if text_member(composition_ref, "identity").map_err(DomainResult::invalid)? != version::IDENTITY
    {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "composition-identity",
        )));
    }
    let composition_digest = text_member(composition_ref, "digest")
        .map_err(DomainResult::invalid)?
        .to_owned();
    digest_text(&composition_digest).map_err(DomainResult::invalid)?;

    let toolchain = object
        .get("toolchain")
        .and_then(Json::as_object)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("toolchain-shape")))?;
    members(toolchain, TOOLCHAIN_KEYS).map_err(DomainResult::invalid)?;
    required(toolchain, TOOLCHAIN_KEYS).map_err(DomainResult::invalid)?;
    for key in TOOLCHAIN_KEYS {
        pin_text(text_member(toolchain, key).map_err(DomainResult::invalid)?)
            .map_err(DomainResult::invalid)?;
    }

    let profiles_json = object
        .get("profiles")
        .and_then(Json::as_array)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("profiles-shape")))?;
    if profiles_json.is_empty() || profiles_json.len() > PROFILES.len() {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "profiles-bound",
        )));
    }
    let mut profile_seen = std::collections::BTreeSet::new();
    for profile in profiles_json {
        let profile = profile
            .as_str()
            .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("profiles-shape")))?;
        if !profile_seen.insert(closed(profile, PROFILES).map_err(DomainResult::invalid)?) {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "profiles-unique",
            )));
        }
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
    let mut targets = Vec::with_capacity(targets_json.len());
    let mut target_seen = std::collections::BTreeSet::new();
    for target_json in targets_json {
        targets.push(decode_target(target_json, &mut target_seen)?);
    }

    let scenarios_json = object
        .get("scenarios")
        .and_then(Json::as_array)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("scenarios-shape")))?;
    if scenarios_json.is_empty() || scenarios_json.len() > version::MAX_SCENARIOS {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "scenarios-bound",
        )));
    }
    let mut scenarios = Vec::with_capacity(scenarios_json.len());
    let mut scenario_seen = std::collections::BTreeSet::new();
    for scenario_json in scenarios_json {
        let scenario_object = scenario_json
            .as_object()
            .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("scenarios-shape")))?;
        members(scenario_object, SCENARIO_RESULT_KEYS).map_err(DomainResult::invalid)?;
        required(scenario_object, SCENARIO_RESULT_KEYS).map_err(DomainResult::invalid)?;
        let scenario = text_member(scenario_object, "scenario").map_err(DomainResult::invalid)?;
        id_text(scenario).map_err(DomainResult::invalid)?;
        if !scenario_seen.insert(scenario.to_owned()) {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "scenarios-unique",
            )));
        }
        let status = closed(
            text_member(scenario_object, "status").map_err(DomainResult::invalid)?,
            ASSERTION_STATUS,
        )
        .map_err(DomainResult::invalid)?;
        let scenario_targets = scenario_object
            .get("targets")
            .and_then(Json::as_array)
            .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("scenarios-shape")))?;
        if scenario_targets.is_empty() || scenario_targets.len() > 8 {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "scenarios-bound",
            )));
        }
        let mut target_row_seen = std::collections::BTreeSet::new();
        for target in scenario_targets {
            let target = target.as_str().ok_or_else(|| {
                DomainResult::invalid(diagnostic::input_invalid("scenarios-shape"))
            })?;
            if !target_row_seen
                .insert(closed(target, SCENARIO_TARGETS).map_err(DomainResult::invalid)?)
            {
                return Err(DomainResult::invalid(diagnostic::input_invalid(
                    "scenarios-unique",
                )));
            }
        }
        scenarios.push(DecodedScenario {
            scenario: scenario.to_owned(),
            status,
        });
    }

    let consumers_json = object
        .get("consumers")
        .and_then(Json::as_array)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("consumers-shape")))?;
    if consumers_json.len() > version::MAX_CONSUMERS {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "consumers-bound",
        )));
    }
    let mut consumers = Vec::with_capacity(consumers_json.len());
    let mut consumer_seen = std::collections::BTreeSet::new();
    for consumer_json in consumers_json {
        let consumer_object = consumer_json
            .as_object()
            .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("consumers-shape")))?;
        members(consumer_object, CONSUMER_KEYS).map_err(DomainResult::invalid)?;
        required(consumer_object, CONSUMER_KEYS).map_err(DomainResult::invalid)?;
        let consumer = text_member(consumer_object, "consumer").map_err(DomainResult::invalid)?;
        id_text(consumer).map_err(DomainResult::invalid)?;
        if !consumer_seen.insert(consumer.to_owned()) {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "consumers-unique",
            )));
        }
        pin_text(text_member(consumer_object, "witness").map_err(DomainResult::invalid)?)
            .map_err(DomainResult::invalid)?;
        pin_text(text_member(consumer_object, "relation").map_err(DomainResult::invalid)?)
            .map_err(DomainResult::invalid)?;
        consumers.push(consumer.to_owned());
    }

    let completeness = object
        .get("completeness")
        .and_then(Json::as_object)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("completeness-shape")))?;
    members(completeness, COMPLETENESS_KEYS).map_err(DomainResult::invalid)?;
    required(completeness, COMPLETENESS_KEYS).map_err(DomainResult::invalid)?;
    let closure_complete = closed(
        text_member(completeness, "consumerClosure").map_err(DomainResult::invalid)?,
        CLOSURE,
    )
    .map_err(DomainResult::invalid)?
        == "complete";
    let _coverage = closed(
        text_member(completeness, "coverage").map_err(DomainResult::invalid)?,
        COVERAGE,
    )
    .map_err(DomainResult::invalid)?;
    if completeness.get("unknownExternalConsumers") != Some(&Json::Bool(true)) {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "completeness-unknown-consumers",
        )));
    }

    let verdict = object
        .get("verdict")
        .and_then(Json::as_object)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("verdict-shape")))?;
    members(verdict, VERDICT_KEYS).map_err(DomainResult::invalid)?;
    required(verdict, VERDICT_KEYS).map_err(DomainResult::invalid)?;
    let strict_claim = closed(
        text_member(verdict, "strict").map_err(DomainResult::invalid)?,
        STRICT_CLAIM,
    )
    .map_err(DomainResult::invalid)?;
    let permissive_claim = closed(
        text_member(verdict, "permissive").map_err(DomainResult::invalid)?,
        PERMISSIVE_CLAIM,
    )
    .map_err(DomainResult::invalid)?;
    if !matches!(verdict.get("degraded"), Some(Json::Bool(_))) {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "verdict-degraded-type",
        )));
    }

    Ok(EvidenceReport {
        composition_digest,
        targets,
        scenarios,
        consumers,
        closure_complete,
        strict_claim,
        permissive_claim,
    })
}

impl EvidenceReport {
    /// The declared consumer subject ids.
    fn consumers(&self) -> &[String] {
        &self.consumers
    }
}

fn decode_target(
    target_json: &Json,
    target_seen: &mut std::collections::BTreeSet<String>,
) -> Result<DecodedTarget, DomainResult> {
    let object = target_json
        .as_object()
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("targets-shape")))?;
    members(object, TARGET_KEYS).map_err(DomainResult::invalid)?;
    required(object, TARGET_KEYS).map_err(DomainResult::invalid)?;
    let target = closed(
        text_member(object, "target").map_err(DomainResult::invalid)?,
        TARGETS,
    )
    .map_err(DomainResult::invalid)?;
    if !target_seen.insert(target.to_owned()) {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "targets-unique",
        )));
    }
    let backend = object
        .get("backend")
        .and_then(Json::as_object)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("backend-shape")))?;
    members(backend, BACKEND_KEYS).map_err(DomainResult::invalid)?;
    required(backend, BACKEND_REQUIRED).map_err(DomainResult::invalid)?;
    let backend_state = closed(
        text_member(backend, "state").map_err(DomainResult::invalid)?,
        BACKEND_STATE,
    )
    .map_err(DomainResult::invalid)?;
    let backend_identity = match backend.get("identity") {
        Some(Json::Null) | None => None,
        Some(Json::String(raw)) => {
            if raw.is_empty() || raw.len() > 128 {
                return Err(DomainResult::invalid(diagnostic::input_invalid(
                    "backend-identity",
                )));
            }
            Some(raw.clone())
        }
        Some(_) => {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "backend-identity",
            )))
        }
    };
    let backend_digest = match backend.get("digest") {
        Some(Json::Null) | None => None,
        Some(Json::String(raw)) => {
            digest_text(raw).map_err(DomainResult::invalid)?;
            Some(raw.clone())
        }
        Some(_) => {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "backend-digest",
            )))
        }
    };
    let execution = closed(
        text_member(object, "execution").map_err(DomainResult::invalid)?,
        EXECUTION,
    )
    .map_err(DomainResult::invalid)?;

    let capabilities_json = object
        .get("capabilities")
        .and_then(Json::as_array)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("capabilities-shape")))?;
    if capabilities_json.len() > version::MAX_CAPABILITIES_PER_TARGET {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "capabilities-bound",
        )));
    }
    let mut capabilities = Vec::with_capacity(capabilities_json.len());
    let mut capability_seen = std::collections::BTreeSet::new();
    for capability_json in capabilities_json {
        let capability_object = capability_json.as_object().ok_or_else(|| {
            DomainResult::invalid(diagnostic::input_invalid("capabilities-shape"))
        })?;
        members(capability_object, CAPABILITY_KEYS).map_err(DomainResult::invalid)?;
        required(capability_object, CAPABILITY_REQUIRED).map_err(DomainResult::invalid)?;
        let capability_text =
            text_member(capability_object, "capability").map_err(DomainResult::invalid)?;
        pin_text(capability_text).map_err(DomainResult::invalid)?;
        if !capability_seen.insert(capability_text.to_owned()) {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "capabilities-unique",
            )));
        }
        let support = closed(
            text_member(capability_object, "support").map_err(DomainResult::invalid)?,
            SUPPORT,
        )
        .map_err(DomainResult::invalid)?;
        if !matches!(capability_object.get("blocking"), Some(Json::Bool(_))) {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "capability-blocking-type",
            )));
        }
        let blocking = capability_object["blocking"] == Json::Bool(true);
        note_text(text_member(capability_object, "note").map_err(DomainResult::invalid)?)
            .map_err(DomainResult::invalid)?;
        let approval = match capability_object.get("approval") {
            Some(Json::Null) | None => None,
            Some(raw) => Some(
                closed(
                    raw.as_str().ok_or_else(|| {
                        DomainResult::invalid(diagnostic::input_invalid("capability-approval"))
                    })?,
                    APPROVAL,
                )
                .map_err(DomainResult::invalid)?,
            ),
        };
        capabilities.push(DecodedCapability {
            capability: capability_text.to_owned(),
            support,
            approval,
            blocking,
        });
    }

    let assertions_json = object
        .get("assertions")
        .and_then(Json::as_array)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("assertions-shape")))?;
    if assertions_json.len() > version::MAX_ASSERTIONS_PER_SCENARIO * 64 {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "assertions-bound",
        )));
    }
    let mut assertions = Vec::with_capacity(assertions_json.len());
    let mut assertion_seen = std::collections::BTreeSet::new();
    for assertion_json in assertions_json {
        let assertion = assertion_json
            .as_object()
            .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("assertions-shape")))?;
        members(assertion, ASSERTION_KEYS).map_err(DomainResult::invalid)?;
        required(assertion, ASSERTION_KEYS).map_err(DomainResult::invalid)?;
        let scenario = text_member(assertion, "scenario").map_err(DomainResult::invalid)?;
        id_text(scenario).map_err(DomainResult::invalid)?;
        let status = closed(
            text_member(assertion, "status").map_err(DomainResult::invalid)?,
            ASSERTION_STATUS,
        )
        .map_err(DomainResult::invalid)?;
        if !assertion_seen.insert(format!("{scenario}:{status}")) {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "assertions-unique",
            )));
        }
        assertions.push((scenario.to_owned(), status));
    }

    let blocking_json = object
        .get("blocking")
        .and_then(Json::as_array)
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("blocking-shape")))?;
    if blocking_json.len() > version::MAX_CAPABILITIES_PER_TARGET {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "blocking-bound",
        )));
    }
    let mut blocking = Vec::with_capacity(blocking_json.len());
    let mut blocking_seen = std::collections::BTreeSet::new();
    for entry in blocking_json {
        let entry = entry
            .as_str()
            .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid("blocking-shape")))?;
        pin_text(entry).map_err(DomainResult::invalid)?;
        if !blocking_seen.insert(entry.to_owned()) {
            return Err(DomainResult::invalid(diagnostic::input_invalid(
                "blocking-unique",
            )));
        }
        blocking.push(entry.to_owned());
    }

    Ok(DecodedTarget {
        target: target.to_owned(),
        backend_state,
        backend_identity,
        backend_digest,
        execution,
        capabilities,
        assertions,
        blocking,
    })
}

/// Admit one evidence report under the selected profile, bound to the
/// exact composition document and the pinned compilation, or return
/// the typed rejection result. The receipt keeps every per-target row;
/// a strict refusal lists its blockers and still carries the rows.
pub fn admit(
    report_bytes: &[u8],
    composition_bytes: &[u8],
    project: &CompiledProject,
    declared_backends: &std::collections::BTreeMap<String, BackendProvenance>,
    profile: AdmissionProfile,
) -> Result<AdmissionReceipt, DomainResult> {
    if report_bytes.len() > version::MAX_DOC_BYTES
        || composition_bytes.len() > version::MAX_DOC_BYTES
    {
        return Err(DomainResult::UnsupportedOperation {
            diagnostics: singleton_set_evidence(),
        });
    }
    let text = std::str::from_utf8(report_bytes)
        .map_err(|_| DomainResult::invalid(io_failure("invalid-encoding")))?;
    let report = decode(text)?;
    // Provenance: the report pins the exact composition bytes through
    // the documented domain-prefixed digest, and the composition must
    // itself join the pinned compilation before any admission join.
    let composition_text = std::str::from_utf8(composition_bytes)
        .map_err(|_| DomainResult::invalid(io_failure("invalid-encoding")))?;
    if report.composition_digest != super::canonical::document_digest(composition_text) {
        return Err(DomainResult::invalid(diagnostic::input_invalid(
            "composition-pin-mismatch",
        )));
    }
    let composition: Composition =
        super::wire::parse(composition_bytes).map_err(DomainResult::invalid)?;
    composition
        .validate(project)
        .map_err(DomainResult::invalid)?;

    // The required scenario inventory comes from the pinned
    // compilation, never from the report.
    let mut required_scenarios = std::collections::BTreeSet::new();
    for definition in &project.definitions {
        if let Definition::Scenario(scenario) = definition {
            required_scenarios.insert(scenario.id.as_str().to_owned());
        }
    }
    let mut scenario_status: std::collections::BTreeMap<&str, &'static str> =
        std::collections::BTreeMap::new();
    for scenario in &report.scenarios {
        scenario_status.insert(scenario.scenario.as_str(), scenario.status);
    }

    // The declared consumer inventory of the pinned compilation: the
    // closure of the composition's public DTO subject. One subject per
    // query that returns the DTO, one per endpoint invoking such a
    // query, and one per scenario covering either — exactly the graph
    // consumers the consumer oracle declares. The report's consumer
    // witnesses join this set in both directions — a missing witness
    // and an unknown non-file consumer are both refusals — so clearing
    // `consumers[]` cannot erase the closure obligation.
    let dto_subject = composition.public_allowlist().dto().to_owned();
    // Whether a `returns` type names the DTO at some (possibly nested
    // list/optional) position.
    fn names_dto(type_ref: &crate::ir::TypeRef, dto: &str) -> bool {
        match type_ref {
            crate::ir::TypeRef::Ref(symbol) => symbol.as_str() == dto,
            crate::ir::TypeRef::List(inner) | crate::ir::TypeRef::Optional(inner) => {
                names_dto(inner, dto)
            }
        }
    }
    let mut declared_consumers = std::collections::BTreeSet::new();
    let mut covered_operations = std::collections::BTreeSet::new();
    let mut derived_consumer_blockers: Vec<String> = Vec::new();
    for definition in &project.definitions {
        match definition {
            Definition::Query(query) => {
                let returns_dto = query
                    .returns
                    .as_ref()
                    .is_some_and(|type_ref| names_dto(type_ref, &dto_subject));
                if returns_dto {
                    declared_consumers.insert(format!("operation:{}", query.id.as_str()));
                    covered_operations.insert(query.id.as_str().to_owned());
                }
            }
            _ => {}
        }
    }
    let mut covered_endpoints = std::collections::BTreeSet::new();
    for definition in &project.definitions {
        match definition {
            Definition::Endpoint(endpoint) => {
                if covered_operations.contains(endpoint.invokes.as_str()) {
                    declared_consumers.insert(format!("endpoint:{}", endpoint.id.as_str()));
                    covered_endpoints.insert(endpoint.id.as_str().to_owned());
                }
            }
            Definition::Scenario(scenario) => {
                let covers = scenario
                    .covers
                    .iter()
                    .any(|covered| {
                        covered_operations.contains(covered.as_str())
                            || covered_endpoints.contains(covered.as_str())
                    });
                if covers {
                    declared_consumers.insert(format!("scenario:{}", scenario.id.as_str()));
                }
            }
            _ => {}
        }
    }
    let mut covered_consumers = std::collections::BTreeSet::new();
    for row_consumer in report.consumers() {
        let resolved = ["operation", "endpoint", "scenario"]
            .iter()
            .find_map(|kind| {
                let subject = format!("{kind}:{row_consumer}");
                declared_consumers.contains(&subject).then_some(subject)
            });
        match resolved {
            Some(subject) => {
                covered_consumers.insert(subject);
            }
            // A consumer outside the compilation inventory is a file
            // witness only when its id is namespaced (`head/tail`); a
            // bare operation-shaped id the compilation does not declare
            // is invented evidence, refused outright.
            None => {
                if !row_consumer.contains('/') {
                    derived_consumer_blockers.push(format!("consumers:{row_consumer}:unknown"));
                }
            }
        }
    }
    for subject in &declared_consumers {
        if !covered_consumers.contains(subject) {
            derived_consumer_blockers.push(format!("consumers:{subject}:missing"));
        }
    }

    let mut blockers: Vec<String> = Vec::new();
    blockers.extend(derived_consumer_blockers);
    let mut receipt_targets = Vec::with_capacity(report.targets.len());
    for target in &report.targets {
        let mut target_blockers: Vec<String> = Vec::new();
        // Required capability coverage: every composition requirement
        // must be carried by every target at its declared minimum. The
        // inventory is the composition's, so deleting a row or
        // clearing a flag cannot remove a required guarantee, and
        // contradictory duplicate rows cannot dilute it.
        for requirement in composition.capability_requirements() {
            let rows: Vec<&DecodedCapability> = target
                .capabilities
                .iter()
                .filter(|row| row.capability == requirement.capability())
                .collect();
            if rows.is_empty() {
                target_blockers.push(format!(
                    "{}:capability-missing:{}",
                    target.target,
                    requirement.capability()
                ));
                continue;
            }
            for row in rows {
                // A `partial` row satisfies only a partial minimum and
                // only behind an explicit review approval: an unapproved
                // partial is an unsatisfied guarantee, whatever it
                // claims.
                let satisfied = match requirement.minimum() {
                    "full" => row.support == "supported",
                    _ => {
                        matches!(row.support, "supported" | "partial")
                            && (row.support == "supported" || row.approval == Some("reviewed"))
                    }
                };
                if !satisfied {
                    target_blockers.push(format!(
                        "{}:{}:{}",
                        target.target,
                        requirement.capability(),
                        row.support
                    ));
                }
            }
        }
        // Explicit declared blockers stay blockers, verbatim (they are
        // already target-qualified by the report author).
        for entry in &target.blocking {
            target_blockers.push(entry.clone());
        }
        // A row the report itself declares blocking stays a blocker
        // when it is not satisfied — for capabilities beyond the
        // composition's required inventory, the report's own blocking
        // declaration is the honest lower bound.
        for row in &target.capabilities {
            if row.blocking && row.support != "supported" {
                let required = composition
                    .capability_requirements()
                    .iter()
                    .any(|requirement| requirement.capability() == row.capability);
                if !required {
                    target_blockers.push(format!(
                        "{}:{}:{}",
                        target.target, row.capability, row.support
                    ));
                }
            }
        }
        // Execution is required, never self-claimed: under strict a
        // required target that did not run refuses, whatever its rows
        // assert.
        if profile == AdmissionProfile::Strict && target.execution != "executed" {
            target_blockers.push(format!("{}:execution:{}", target.target, target.execution));
        }
        // Backend binding: a claimed execution requires a bound
        // backend with identity and digest; an absent backend can
        // never be executed. A bound backend must additionally equal
        // the engine shell's provenance receipt for this compilation:
        // an identity or digest the engine did not produce is forged
        // provenance, refused under both profiles.
        if target.execution == "executed"
            && (target.backend_state != "bound"
                || target.backend_identity.is_none()
                || target.backend_digest.is_none())
        {
            target_blockers.push(format!("{}:backend:unbound", target.target));
        }
        if target.backend_state == "bound" {
            let forged = match declared_backends.get(target.target.as_str()) {
                Some(provenance) => {
                    Some(provenance.identity.as_str()) != target.backend_identity.as_deref()
                        || Some(provenance.digest.as_str()) != target.backend_digest.as_deref()
                }
                None => target.backend_identity.is_some() || target.backend_digest.is_some(),
            };
            if forged {
                target_blockers.push(format!("{}:backend:provenance", target.target));
            }
        }
        // Assertion coverage: every required scenario of the pinned
        // compilation must carry a per-target assertion, and a target
        // that claims execution must carry it as passed. Deleting a
        // row cannot delete the obligation.
        if target.execution == "executed" {
            let asserted: std::collections::BTreeSet<&str> = target
                .assertions
                .iter()
                .map(|(scenario, _)| scenario.as_str())
                .collect();
            for scenario in &required_scenarios {
                if !asserted.contains(scenario.as_str()) {
                    target_blockers.push(format!(
                        "{}:{}:assertion-missing",
                        target.target, scenario
                    ));
                }
            }
        }
        let mut passed = 0usize;
        for (scenario, status) in &target.assertions {
            if *status == "pass" {
                passed += 1;
                if target.execution != "executed" {
                    // A target that does not claim execution cannot
                    // carry passing rows.
                    target_blockers.push(format!("{}:{scenario}:{status}", target.target));
                }
            } else if target.execution == "executed" {
                // A target that claims execution must not carry
                // unexecuted rows: the claim itself is the blocker.
                target_blockers.push(format!("{}:{scenario}:{status}", target.target));
            }
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
            target: target.target.clone(),
            execution: target.execution.to_owned(),
            verdict,
            supported: target
                .capabilities
                .iter()
                .filter(|row| row.support == "supported")
                .count(),
            capabilities: target.capabilities.len(),
            passed,
            assertions: target.assertions.len(),
        });
    }
    receipt_targets.sort_by(|left, right| left.target.cmp(&right.target));

    // Scenario coverage: every declared scenario of the pinned
    // compilation is covered exactly once, and its aggregate status is
    // honest: `fail`, `unsupported`, and a `not_executed` row beneath a
    // passing execution all refuse; the compilation inventory decides,
    // never the report.
    for scenario in &required_scenarios {
        match scenario_status.get(scenario.as_str()) {
            None => blockers.push(format!("scenarios:{scenario}:missing")),
            Some(&"fail") => blockers.push(format!("scenarios:{scenario}:fail")),
            Some(&"unsupported") => {
                blockers.push(format!("scenarios:{scenario}:unsupported"))
            }
            Some(&"not_executed") => {
                let executed = report.targets.iter().any(|target| {
                    target.execution == "executed"
                        && target
                            .assertions
                            .iter()
                            .any(|(name, status)| name == scenario.as_str() && *status == "pass")
                });
                if executed {
                    blockers.push(format!("scenarios:{}:conflict", scenario.as_str()));
                } else if profile == AdmissionProfile::Strict {
                    blockers.push(format!("scenarios:{scenario}:not_executed"));
                }
            }
            Some(_) => {}
        }
    }
    for scenario in &report.scenarios {
        if scenario.status != "fail" {
            continue;
        }
        let contradicted = report.targets.iter().any(|target| {
            target
                .assertions
                .iter()
                .any(|(name, status)| name == &scenario.scenario && *status == "pass")
        });
        if contradicted {
            blockers.push(format!("scenarios:{}:conflict", scenario.scenario));
        }
    }

    // Completeness gaps are retained under every profile: an
    // incomplete closure degrades permissive admission instead of
    // passing it.
    if !report.closure_complete {
        blockers.push("consumers:closure:incomplete".to_owned());
    }

    // The report's own verdict claims cannot contradict the derived
    // rows: a claimed pass with open blockers (or a claimed refusal
    // with none) is itself a blocker.
    let derived_success = blockers.is_empty();
    let strict_claim_pass = report.strict_claim == "pass";
    let permissive_claim_pass = report.permissive_claim == "pass";
    if strict_claim_pass != derived_success {
        blockers.push("verdict:strict-claim".to_owned());
    }
    if permissive_claim_pass != derived_success {
        blockers.push("verdict:permissive-claim".to_owned());
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
        identity: version::EVIDENCE_IDENTITY,
        profile: profile.wire_name(),
        verdict,
        targets: receipt_targets,
        blockers,
    })
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

    /// Decode raw bytes the way admission does (the same IO failure
    /// mapping for a non-UTF-8 body).
    fn try_decode(bytes: &[u8]) -> Result<EvidenceReport, DomainResult> {
        decode(
            std::str::from_utf8(bytes)
                .map_err(|_| DomainResult::invalid(io_failure("invalid-encoding")))?,
        )
    }

    #[test]
    fn profile_names_are_the_wire_spellings() {
        assert_eq!(AdmissionProfile::Strict.wire_name(), "strict");
        assert_eq!(AdmissionProfile::Permissive.wire_name(), "permissive");
    }

    #[test]
    fn evidence_incomplete_is_a_registered_unsupported_rule() {
        let set = singleton_set_evidence();
        assert_eq!(set.as_slice().len(), 1);
        assert_eq!(
            set.as_slice()[0].id(),
            super::diagnostic::EVIDENCE_INCOMPLETE
        );
        assert_eq!(set.as_slice()[0].registry_version(), "0.8.0");
    }

    #[test]
    fn text_validators_mirror_the_schema_patterns() {
        assert!(pin_text("publish.approval").is_ok());
        assert!(pin_text("rendering.safe_url").is_ok());
        assert!(pin_text("").is_err());
        assert!(pin_text("with space").is_err());
        assert!(id_text("worklog.scenario.completion_without_worklog").is_ok());
        assert!(id_text("taskhub.dto/public-entry").is_ok());
        assert!(id_text("Uppercase").is_err());
        assert!(id_text(".dotted").is_err());
        // The exact published grammar: a slash tail requires a dotted
        // head, and every tail segment carries at least two characters.
        assert!(id_text("plain/nodot").is_err());
        assert!(id_text("a/b").is_err());
        assert!(id_text("head.two/tail").is_ok());
        assert!(id_text("head.two/t").is_err());
        assert!(digest_text(
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        )
        .is_ok());
        assert!(digest_text("sha256:short").is_err());
        assert!(digest_text(
            "SHA256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        )
        .is_err());
        assert!(note_text("plain sentence").is_ok());
        assert!(note_text("control\u{0007}bell").is_err());
    }

    #[test]
    fn malformed_reports_refuse_at_the_wire_layer() {
        // A structurally broken report: wrong reportVersion.
        let report = serde_json::json!({
            "schemaVersion": version::EVIDENCE_SCHEMA_VERSION,
            "identity": version::EVIDENCE_IDENTITY,
            "reportVersion": "9.9.9",
            "compositionRef": {"identity": version::IDENTITY, "digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"},
            "toolchain": {"harness": "h", "node": "n", "ajv": "a"},
            "profiles": ["strict"],
            "targets": [],
            "scenarios": [],
            "consumers": [],
            "completeness": {"consumerClosure": "complete", "coverage": "full", "unknownExternalConsumers": true},
            "verdict": {"strict": "unsupported", "permissive": "degraded", "degraded": true},
        });
        let bytes = serde_json::to_vec(&report).expect("report serializes");
        let set = try_decode(&bytes).unwrap_err();
        assert_eq!(set.status(), Status::Invalid);
        let rendered = serde_json::to_string(set.diagnostics().first().expect("one diagnostic"))
            .expect("diagnostic serializes");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("diagnostic JSON");
        assert_eq!(value["data"]["detail"], "report-version");
    }

    #[test]
    fn unknown_nested_members_are_structurally_refused() {
        let report = serde_json::json!({
            "schemaVersion": version::EVIDENCE_SCHEMA_VERSION,
            "identity": version::EVIDENCE_IDENTITY,
            "reportVersion": version::VERSION,
            "compositionRef": {"identity": version::IDENTITY, "digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"},
            "toolchain": {"harness": "h", "node": "n", "ajv": "a"},
            "profiles": ["strict"],
            "targets": [{
                "target": "php",
                "backend": {"state": "absent", "identity": null, "digest": null},
                "execution": "not_executed",
                "capabilities": [],
                "assertions": [{"scenario": "worklog.scenario.a", "status": "not_executed", "rogue": true}],
                "blocking": []
            }],
            "scenarios": [{"scenario": "worklog.scenario.a", "status": "not_executed", "targets": ["php"]}],
            "consumers": [],
            "completeness": {"consumerClosure": "complete", "coverage": "full", "unknownExternalConsumers": true},
            "verdict": {"strict": "unsupported", "permissive": "degraded", "degraded": true},
        });
        let bytes = serde_json::to_vec(&report).expect("report serializes");
        let set = try_decode(&bytes).unwrap_err();
        let rendered = serde_json::to_string(set.diagnostics().first().expect("one diagnostic"))
            .expect("diagnostic serializes");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("diagnostic JSON");
        assert_eq!(value["data"]["detail"], "unknown-field");
    }
}
