//! Deterministic sorting, serialization, and result-building helpers
//! (issue #106): canonical write plans, artifact-manifest candidates,
//! and scenario/evidence result normalization.

use crate::generated::protocol;
use crate::ids::{plan_id, sha256};
use crate::json::{canonical, quote, Value};
use crate::paths::{is_logical_path, plan_scope_violation, protected_home};

fn write_entry(entry: &Value) -> Option<(String, String, Option<String>)> {
    let path = entry.get("path").and_then(Value::as_str)?;
    if !is_logical_path(path) {
        return None;
    }
    let action = entry.get("action").and_then(Value::as_str)?;
    if !protocol::enums::WRITEACTION.contains(&action) {
        return None;
    }
    match (action, entry.get("sha256")) {
        ("delete", None) | ("delete", Some(Value::Null)) => Some((path.to_owned(), action.to_owned(), None)),
        (_, Some(digest)) => {
            let digest = digest.as_str()?;
            if digest.len() == 71 && digest.starts_with("sha256:") {
                Some((path.to_owned(), action.to_owned(), Some(digest.to_owned())))
            } else {
                None
            }
        }
        (_, None) => None,
    }
}

/// Sort one write plan by path and verify the closed entry grammar.
pub fn sort_write_plan(entries: &Value) -> Option<Vec<(String, String, Option<String>)>> {
    let items = entries.as_array()?;
    if items.len() > protocol::bounds::MAX_PLAN_ENTRIES as usize {
        return None;
    }
    let mut sorted = Vec::new();
    for entry in items {
        sorted.push(write_entry(entry)?);
    }
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    Some(sorted)
}

/// The deterministic plan identifier of one already path-sorted plan.
pub fn canonical_plan_id(sorted: &[(String, String, Option<String>)]) -> String {
    let value = Value::Array(
        sorted
            .iter()
            .map(|(path, action, digest)| {
                let mut members = vec![
                    ("action".to_owned(), Value::Str(action.clone())),
                    ("path".to_owned(), Value::Str(path.clone())),
                ];
                if let Some(digest) = digest {
                    members.push(("sha256".to_owned(), Value::Str(digest.clone())));
                }
                Value::Object(members)
            })
            .collect(),
    );
    plan_id(&value)
}

/// The `sha256:` digest of the exact file content a write entry commits to.
pub fn write_entry_digest(content: &str) -> String {
    sha256(content)
}

/// Build one artifact-manifest candidate: path-sorted, digest-addressed.
pub fn build_artifact_manifest(entries: &[(String, String)]) -> Option<Value> {
    if entries.is_empty() || entries.len() > protocol::bounds::MAX_PLAN_ENTRIES as usize {
        return None;
    }
    let mut rows: Vec<(String, String, String)> = Vec::new();
    for (path, bytes) in entries {
        if !is_logical_path(path) {
            return None;
        }
        if protected_home(path).is_some() {
            return None;
        }
        rows.push((path.clone(), "create".to_owned(), sha256(bytes)));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let entries_value = Value::Array(
        rows.iter()
            .map(|(path, action, digest)| {
                Value::Object(vec![
                    ("action".to_owned(), Value::Str(action.clone())),
                    ("path".to_owned(), Value::Str(path.clone())),
                    ("sha256".to_owned(), Value::Str(digest.clone())),
                ])
            })
            .collect(),
    );
    Some(Value::Object(vec![
        ("algorithm".to_owned(), Value::Str("sha256".to_owned())),
        ("canonicalForm".to_owned(), Value::Str(canonical(&entries_value))),
        ("entries".to_owned(), entries_value),
    ]))
}

const SCENARIO_STATUSES: [&str; 5] = ["pass", "fail", "unsupported", "infrastructure", "degraded"];

/// Normalize one scenario result: closed status set, sorted findings,
/// bounded evidence. An unknown status never folds into `pass`.
pub fn normalize_scenario_result(result: &Value) -> Option<Value> {
    let status = result.get("status").and_then(Value::as_str)?;
    if !SCENARIO_STATUSES.contains(&status) {
        return None;
    }
    let findings = result.get("findings").and_then(Value::as_array).unwrap_or(&[]);
    if findings.len() > 10_000 {
        return None;
    }
    let mut sorted: Vec<(String, String, Option<String>)> = Vec::new();
    for finding in findings {
        let code = finding.get("code").and_then(Value::as_str)?;
        if code.is_empty() || code.chars().count() > 128 {
            return None;
        }
        let path = finding.get("path").and_then(Value::as_str)?;
        if !is_logical_path(path) {
            return None;
        }
        let detail = match finding.get("detail") {
            None | Some(Value::Null) => None,
            Some(detail) => {
                let detail = detail.as_str()?;
                if detail.chars().count() > 128 {
                    return None;
                }
                Some(detail.to_owned())
            }
        };
        sorted.push((path.to_owned(), code.to_owned(), detail));
    }
    sorted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let evidence_digest = match result.get("evidenceDigest") {
        None | Some(Value::Null) => Value::Null,
        Some(digest) => {
            let digest = digest.as_str()?;
            if digest.len() != 71 || !digest.starts_with("sha256:") {
                return None;
            }
            Value::Str(digest.to_owned())
        }
    };
    let findings_value = Value::Array(
        sorted
            .into_iter()
            .map(|(path, code, detail)| {
                let mut members = vec![
                    ("code".to_owned(), Value::Str(code)),
                    ("path".to_owned(), Value::Str(path)),
                ];
                if let Some(detail) = detail {
                    members.push(("detail".to_owned(), Value::Str(detail)));
                }
                Value::Object(members)
            })
            .collect(),
    );
    Some(Value::Object(vec![
        ("evidenceDigest".to_owned(), evidence_digest),
        ("findings".to_owned(), findings_value),
        ("status".to_owned(), Value::Str(status.to_owned())),
    ]))
}

/// Whether a plan entry violates its write scopes or a protected home.
pub fn write_plan_violation(path: &str, write_scopes: &[String]) -> Option<String> {
    plan_scope_violation(path, write_scopes)
}

/// Quote helper re-exported for builders (unused placeholder guard).
#[allow(unused)]
fn _quote_guard(text: &str) -> String {
    quote(text)
}
