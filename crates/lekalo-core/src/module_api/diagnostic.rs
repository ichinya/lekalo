//! Registry-owned messages for the `module-api.*` rules; producers supply
//! only bounded evidence tokens (issue #113). Every failure finalizes
//! through the shared registry-backed constructor; a registry failure
//! collapses to the registry-invariant set instead of panicking.

use super::wire;
use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject, DataValue, Scalar};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::Status;

/// One typed boundary finding, the report projection of a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    pub rule: &'static str,
    pub subject: String,
    pub detail: String,
    pub consumer: Option<String>,
    pub producer: Option<String>,
    pub export: Option<String>,
    pub member: Option<String>,
    pub path: Vec<String>,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub gaps: Vec<String>,
}

impl Evidence {
    pub fn new(rule: &'static str, subject: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            rule,
            subject: subject.into(),
            detail: detail.into(),
            consumer: None,
            producer: None,
            export: None,
            member: None,
            path: Vec::new(),
            expected: None,
            actual: None,
            gaps: Vec::new(),
        }
    }
}

/// The registered rule metadata (mirrors diagnostic-registry.v0.8.0.json).
pub fn meta(rule: &str) -> (&'static str, &'static str, &'static str) {
    // (code, severity, category)
    match rule {
        "module-api.input-invalid" => ("LEK-MODULE-001", "error", "infrastructure"),
        "module-api.version-unsupported" => ("LEK-MODULE-002", "error", "compatibility"),
        "module-api.symbol-not-exported" => ("LEK-MODULE-003", "error", "security"),
        "module-api.import-undeclared" => ("LEK-MODULE-004", "error", "semantic"),
        "module-api.public-private-exposure" => ("LEK-MODULE-005", "error", "security"),
        "module-api.public-target-exposure" => ("LEK-MODULE-006", "error", "compatibility"),
        "module-api.cross-module-write-forbidden" => ("LEK-MODULE-007", "error", "security"),
        "module-api.dependency-cycle" => ("LEK-MODULE-008", "error", "semantic"),
        "module-api.consumer-incompatible" => ("LEK-MODULE-009", "warning", "compatibility"),
        "module-api.version-policy-violation" => ("LEK-MODULE-010", "warning", "compatibility"),
        "module-api.deprecation-window-violation" => ("LEK-MODULE-011", "warning", "compatibility"),
        "module-api.evidence-incomplete" => ("LEK-MODULE-012", "warning", "semantic"),
        "module-api.lock-stale" => ("LEK-MODULE-013", "warning", "compatibility"),
        "module-api.policy-denied" => ("LEK-MODULE-014", "error", "compatibility"),
        _ => ("LEK-MODULE-001", "error", "infrastructure"),
    }
}

/// Project one typed evidence into the closed report finding shape.
pub fn finding(evidence: &Evidence) -> wire::Finding {
    let (code, severity, category) = meta(evidence.rule);
    wire::Finding {
        rule: evidence.rule.to_owned(),
        code: code.to_owned(),
        severity: severity.to_owned(),
        category: category.to_owned(),
        subject: evidence.subject.clone(),
        detail: evidence.detail.clone(),
        consumer: evidence.consumer.clone(),
        producer: evidence.producer.clone(),
        export: evidence.export.clone(),
        member: evidence.member.clone(),
        path: evidence.path.clone(),
        expected: evidence.expected.clone(),
        actual: evidence.actual.clone(),
    }
}

/// One bounded diagnostic token: the same cleaning/bounding the shared
/// token seam applies.
fn bounded(text: &str) -> String {
    match token_value(text) {
        DataValue::Token(token) => token,
        _ => String::new(),
    }
}

/// Build one registered diagnostic from typed evidence.
pub(crate) fn diagnostic(evidence: &Evidence) -> Result<Diagnostic, DiagnosticSet> {
    let mut data = DataObject::from([
        ("detail".to_owned(), token_value(&evidence.detail)),
        ("subject".to_owned(), token_value(&evidence.subject)),
    ]);
    match evidence.rule {
        "module-api.public-private-exposure" | "module-api.public-target-exposure" => {
            data.insert(
                "export".to_owned(),
                token_value(evidence.export.as_deref().unwrap_or_default()),
            );
            data.insert(
                "member".to_owned(),
                token_value(evidence.member.as_deref().unwrap_or_default()),
            );
            data.insert(
                "path".to_owned(),
                DataValue::List(
                    evidence
                        .path
                        .iter()
                        .map(|step| Scalar::Token(bounded(step)))
                        .collect(),
                ),
            );
        }
        "module-api.symbol-not-exported" | "module-api.import-undeclared" => {
            // The registry allowlist admits producer/export for both rules;
            // `import-undeclared` never declares a consumer field.
            if evidence.rule == "module-api.symbol-not-exported" {
                data.insert(
                    "consumer".to_owned(),
                    token_value(evidence.consumer.as_deref().unwrap_or_default()),
                );
            }
            data.insert(
                "producer".to_owned(),
                token_value(evidence.producer.as_deref().unwrap_or_default()),
            );
            data.insert(
                "export".to_owned(),
                token_value(evidence.export.as_deref().unwrap_or_default()),
            );
        }
        "module-api.cross-module-write-forbidden" => {
            data.insert(
                "operation".to_owned(),
                token_value(evidence.export.as_deref().unwrap_or_default()),
            );
            data.insert(
                "resource".to_owned(),
                token_value(evidence.member.as_deref().unwrap_or_default()),
            );
            data.insert(
                "effect".to_owned(),
                token_value(
                    evidence
                        .path
                        .first()
                        .map(String::as_str)
                        .unwrap_or_default(),
                ),
            );
            data.insert(
                "grant".to_owned(),
                token_value(evidence.expected.as_deref().unwrap_or("none")),
            );
        }
        "module-api.dependency-cycle" => {
            data.insert(
                "cycle".to_owned(),
                DataValue::List(
                    evidence
                        .path
                        .iter()
                        .map(|step| Scalar::Token(bounded(step)))
                        .collect(),
                ),
            );
        }
        "module-api.consumer-incompatible" => {
            data.insert(
                "consumer".to_owned(),
                token_value(evidence.consumer.as_deref().unwrap_or_default()),
            );
            data.insert(
                "producer".to_owned(),
                token_value(evidence.producer.as_deref().unwrap_or_default()),
            );
            data.insert(
                "policy".to_owned(),
                token_value(
                    evidence
                        .path
                        .first()
                        .map(String::as_str)
                        .unwrap_or_default(),
                ),
            );
            // expected/actual stay report-only: the registry data
            // allowlist for this rule carries no version witnesses.
        }
        "module-api.lock-stale" => {
            data.insert(
                "expected".to_owned(),
                token_value(evidence.expected.as_deref().unwrap_or_default()),
            );
            data.insert(
                "actual".to_owned(),
                token_value(evidence.actual.as_deref().unwrap_or_default()),
            );
            // consumer/producer/policy stay report-only: the registry data
            // allowlist for this rule carries only the pin witnesses.
        }
        "module-api.version-policy-violation" | "module-api.deprecation-window-violation" => {
            data.insert(
                "expected".to_owned(),
                token_value(evidence.expected.as_deref().unwrap_or_default()),
            );
            data.insert(
                "actual".to_owned(),
                token_value(evidence.actual.as_deref().unwrap_or_default()),
            );
            if evidence.rule == "module-api.version-policy-violation" {
                data.insert(
                    "policy".to_owned(),
                    token_value(
                        evidence
                            .path
                            .first()
                            .map(String::as_str)
                            .unwrap_or_default(),
                    ),
                );
            }
        }
        "module-api.evidence-incomplete" => {
            data.insert(
                "gaps".to_owned(),
                DataValue::List(
                    evidence
                        .gaps
                        .iter()
                        .map(|gap| Scalar::Token(bounded(gap)))
                        .collect(),
                ),
            );
        }
        "module-api.version-unsupported" => {
            data.insert(
                "version".to_owned(),
                token_value(evidence.expected.as_deref().unwrap_or_default()),
            );
            data.insert(
                "state".to_owned(),
                token_value(evidence.actual.as_deref().unwrap_or("unregistered")),
            );
        }
        "module-api.policy-denied" => {
            data.insert(
                "findings".to_owned(),
                DataValue::List(
                    evidence
                        .path
                        .iter()
                        .map(|step| Scalar::Token(bounded(step)))
                        .collect(),
                ),
            );
        }
        _ => {}
    }
    build(evidence.rule, None, None, data)
        .map_err(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
}

/// The registry-allowed statuses of one rule (mirrors
/// diagnostic-registry.v0.8.0.json). Producers must never discover them
/// through a failed set construction: a mismatched status collapses the
/// whole typed report into one generic registry invariant diagnostic and
/// destroys the evidence.
pub(crate) fn allowed_statuses(rule: &str) -> &'static [Status] {
    match rule {
        "module-api.input-invalid" => &[Status::Invalid],
        "module-api.version-unsupported" => &[Status::UnsupportedVersion],
        "module-api.symbol-not-exported"
        | "module-api.import-undeclared"
        | "module-api.public-private-exposure"
        | "module-api.public-target-exposure"
        | "module-api.cross-module-write-forbidden"
        | "module-api.dependency-cycle" => &[Status::Valid, Status::Invalid],
        "module-api.consumer-incompatible"
        | "module-api.version-policy-violation"
        | "module-api.deprecation-window-violation"
        | "module-api.lock-stale" => &[Status::Valid, Status::Denied],
        "module-api.evidence-incomplete" => &[Status::Valid, Status::Unavailable],
        "module-api.policy-denied" => &[Status::Denied],
        _ => &[Status::Invalid],
    }
}

/// Build a validated set from evidence under one status. Evidence whose
/// rule does not allow the status is deterministically excluded here (it
/// stays in the typed report payload) instead of collapsing the entire
/// report to the registry-invariant set; an over-limit admissible set is
/// truncated in the same canonical order with the report carrying the
/// incompleteness. Only a true registry invariant (unknown rule, registry
/// failure) still collapses.
pub fn set(evidence: &[Evidence], status: Status) -> Result<DiagnosticSet, DiagnosticSet> {
    let mut admissible: Vec<&Evidence> = evidence
        .iter()
        .filter(|finding| allowed_statuses(finding.rule).contains(&status))
        .collect();
    admissible.sort_by(|left, right| {
        (left.rule, left.subject.as_str(), left.detail.as_str()).cmp(&(
            right.rule,
            right.subject.as_str(),
            right.detail.as_str(),
        ))
    });
    admissible.dedup_by(|left, right| {
        left.rule == right.rule && left.subject == right.subject && left.detail == right.detail
    });
    let mut taken = admissible.len();
    loop {
        let diagnostics = admissible[..taken]
            .iter()
            .map(|finding| diagnostic(finding))
            .collect::<Result<Vec<_>, _>>()?;
        match DiagnosticSet::try_from_unsorted(diagnostics, status) {
            Ok(set) => return Ok(set),
            Err(crate::diagnostics::normalize::SetError::OverLimit) => {
                let bound = crate::diagnostics::types::limits::DIAGNOSTICS_PER_RESULT;
                if taken <= bound {
                    taken = taken.saturating_sub(1);
                } else {
                    taken = bound;
                }
            }
            Err(_) => return Err(crate::result::singleton_set("diagnostics.registry-invalid")),
        }
    }
}

/// The denied seam: only denied-allowed evidence plus one terminal
/// policy-denied marker, so a stale lock mixed with boundary findings can
/// never collapse the report (finding: diagnostic envelope collapse).
pub fn denied_set(
    evidence: &[Evidence],
    subject: &str,
    detail: &str,
) -> Result<DiagnosticSet, DiagnosticSet> {
    let mut marker = Evidence::new("module-api.policy-denied", subject, detail);
    marker.path = evidence
        .iter()
        .map(|finding| finding.rule.to_owned())
        .collect();
    let mut all: Vec<Evidence> = evidence.to_vec();
    all.push(marker);
    set(&all, Status::Denied)
}

/// One invalid input set.
pub fn invalid(detail: &str) -> DiagnosticSet {
    set(
        &[Evidence::new(
            "module-api.input-invalid",
            "module-api",
            detail,
        )],
        Status::Invalid,
    )
    .unwrap_or_else(|set| set)
}

/// One unsupported-version set.
pub fn unsupported_version(version: &str) -> DiagnosticSet {
    let mut evidence = Evidence::new(
        "module-api.version-unsupported",
        "module-api",
        "unknown-version",
    );
    evidence.expected = Some(version.to_owned());
    evidence.actual = Some("unregistered".to_owned());
    set(&[evidence], Status::UnsupportedVersion).unwrap_or_else(|set| set)
}
