//! Canonical serialization of the reference-evaluation trace (issue
//! #107).
//!
//! Compact UTF-8 JSON with no insignificant whitespace. Every
//! object's members are written in unsigned UTF-8 byte order of their
//! keys; set-like collections are written sorted; behaviorally
//! ordered collections (`given`, `when`, effects, assertions) keep
//! their exact execution order — canonicalization never sorts away
//! behavior. The bytes are path- and host-independent: no physical
//! root, wall-clock value, locale, host name, or adapter transcript
//! ever enters them, and the writer appends no trailing LF.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use crate::diagnostics::DiagnosticSet;
use crate::scenario::value::TypedValue;

use super::diagnostic;
use super::trace::{
    AssertionRecord, EffectRecord, GivenRecord, Outcome, ReferenceTrace, RowSnapshot, Verdict,
    WhenRecord,
};
use super::version;

/// Serialize one whole trace to canonical bytes, or refuse beyond the
/// payload bound.
pub(crate) fn trace_bytes(trace: &ReferenceTrace) -> Result<String, DiagnosticSet> {
    let text = payload(trace);
    if text.len() > version::MAX_CANONICAL_BYTES {
        return Err(diagnostic::export_limit_set(text.len()));
    }
    Ok(text)
}

/// The digest of one canonical payload, spelled `sha256:<hex>`.
pub(crate) fn digest_of(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut hex = String::with_capacity(64);
    for byte in digest.iter() {
        let _ = write!(hex, "{byte:02x}");
    }
    format!("sha256:{hex}")
}

/// The canonical payload of one trace.
fn payload(trace: &ReferenceTrace) -> String {
    object(vec![
        (
            "assertions",
            Some(array(
                &trace
                    .assertions
                    .iter()
                    .map(assertion_bytes)
                    .collect::<Vec<_>>(),
            )),
        ),
        (
            "attachmentRevision",
            Some(string(&trace.attachment_revision)),
        ),
        ("capabilities", Some(capabilities())),
        (
            "effects",
            Some(array(
                &trace.effects.iter().map(effect_bytes).collect::<Vec<_>>(),
            )),
        ),
        ("effectDigest", Some(string(&trace.effect_digest))),
        ("identity", Some(string(version::IDENTITY))),
        ("irDigest", Some(string(&trace.ir_digest))),
        ("modelVersion", Some(string(&trace.model_version))),
        ("refusal", trace.refusal.map(string)),
        (
            "scenario",
            Some(object(vec![
                ("digest", Some(string(&trace.scenario_digest))),
                ("id", Some(string(&trace.scenario_id))),
                ("version", Some(string(&trace.scenario_version))),
            ])),
        ),
        ("schemaVersion", Some(string(version::SCHEMA_VERSION))),
        ("semantics", Some(string(version::SEMANTICS_IDENTITY))),
        (
            "state",
            Some(array(
                &trace.state.iter().map(row_bytes).collect::<Vec<_>>(),
            )),
        ),
        ("stateDigest", Some(string(&trace.state_digest))),
        ("status", Some(string(trace.status.as_str()))),
        (
            "steps",
            Some(array(
                &trace
                    .given
                    .iter()
                    .map(given_bytes)
                    .chain(trace.when.iter().map(when_bytes))
                    .collect::<Vec<_>>(),
            )),
        ),
    ])
}

/// The closed capability sets of the reference backend, sorted.
fn capabilities() -> String {
    let supported: Vec<String> = super::SUPPORTED_CAPABILITIES
        .iter()
        .map(|entry| string(entry))
        .collect();
    let absent: Vec<String> = super::ABSENT_CAPABILITIES
        .iter()
        .map(|entry| string(entry))
        .collect();
    object(vec![
        ("absent", Some(array(&absent))),
        ("supported", Some(array(&supported))),
    ])
}

/// The canonical bytes of one `given` record.
fn given_bytes(record: &GivenRecord) -> String {
    object(vec![
        ("entity", record.entity.as_deref().map(string)),
        ("kind", Some(string(record.kind))),
        ("row", record.row.as_deref().map(string)),
        ("status", Some(string(record_status(record)))),
        ("stepId", Some(string(&record.step_id))),
    ])
}

/// The wire status of one given record.
fn record_status(record: &GivenRecord) -> &'static str {
    match &record.status {
        Ok(()) => "established",
        Err(reason) => reason,
    }
}

/// The canonical bytes of one `when` record.
fn when_bytes(record: &WhenRecord) -> String {
    let empty: Vec<String> = Vec::new();
    let declared = match &record.outcome {
        Outcome::Error { declared, .. } => declared.iter().map(|id| string(id)).collect::<Vec<_>>(),
        _ => empty.clone(),
    };
    let violations = match &record.outcome {
        Outcome::Error { violations, .. } => {
            violations.iter().map(|id| string(id)).collect::<Vec<_>>()
        }
        _ => empty,
    };
    object(vec![
        ("clock", Some(string(&record.clock))),
        ("declared", Some(array(&declared))),
        (
            "derivedIds",
            Some(array(
                &record
                    .derived_ids
                    .iter()
                    .map(typed_value)
                    .collect::<Vec<_>>(),
            )),
        ),
        (
            "effects",
            Some(array(
                &record
                    .effects
                    .iter()
                    .map(|index| number(*index as i64))
                    .collect::<Vec<_>>(),
            )),
        ),
        ("operation", Some(string(&record.operation))),
        ("outcome", Some(string(record.outcome.key()))),
        (
            "output",
            Some(match &record.outcome {
                Outcome::Ok { output } => typed_value(output),
                _ => typed_value(&TypedValue::Null),
            }),
        ),
        (
            "reason",
            match &record.outcome {
                Outcome::Unsupported { reason } => Some(string(reason)),
                _ => None,
            },
        ),
        ("replayOf", record.replay_of.as_deref().map(string)),
        ("role", Some(string("when"))),
        ("stateSpace", record.state_space.as_deref().map(string)),
        ("stepId", Some(string(&record.step_id))),
        (
            "token",
            match &record.outcome {
                Outcome::Error { token, .. } => Some(string(token.as_str())),
                _ => None,
            },
        ),
        ("violations", Some(array(&violations))),
    ])
}

/// The canonical bytes of one effect record.
fn effect_bytes(record: &EffectRecord) -> String {
    object(vec![
        ("entity", record.entity.as_deref().map(string)),
        (
            "fields",
            Some(array(
                &record
                    .fields
                    .iter()
                    .map(|field| string(field))
                    .collect::<Vec<_>>(),
            )),
        ),
        ("index", Some(number(record.index as i64))),
        ("kind", Some(string(record.kind.as_str()))),
        ("operation", record.operation.map(string)),
        ("row", record.row.as_deref().map(string)),
        ("step", Some(string(&record.step))),
        ("target", record.target.as_deref().map(string)),
    ])
}

/// The canonical bytes of one row snapshot.
fn row_bytes(snapshot: &RowSnapshot) -> String {
    let fields: Vec<String> = snapshot
        .fields
        .iter()
        .map(|(name, value)| {
            object(vec![
                ("field", Some(string(name))),
                ("value", Some(typed_value(value))),
            ])
        })
        .collect();
    object(vec![
        ("entity", Some(string(&snapshot.entity))),
        ("fields", Some(array(&fields))),
        ("key", Some(string(&snapshot.key))),
    ])
}

/// The canonical bytes of one assertion record.
fn assertion_bytes(record: &AssertionRecord) -> String {
    let (verdict, reason) = match &record.verdict {
        Verdict::Pass => ("pass", None),
        Verdict::Fail(reason) => ("fail", Some(string(reason))),
        Verdict::Unsupported(reason) => ("unsupported", Some(string(reason))),
    };
    object(vec![
        ("kind", Some(string(record.kind))),
        ("observes", Some(string(&record.observes))),
        ("reason", reason),
        ("stepId", Some(string(&record.step_id))),
        ("verdict", Some(string(verdict))),
    ])
}

/// The canonical spelling of one runtime typed value, identical to
/// the Scenario IR leaf form.
pub(crate) fn typed_value(value: &TypedValue) -> String {
    match value {
        TypedValue::Null => object(vec![
            ("type", Some(string("null"))),
            ("value", Some(raw("null"))),
        ]),
        TypedValue::Boolean(flag) => object(vec![
            ("type", Some(string("boolean"))),
            ("value", Some(raw(if *flag { "true" } else { "false" }))),
        ]),
        TypedValue::Integer(value) => object(vec![
            ("type", Some(string("integer"))),
            ("value", Some(number(*value))),
        ]),
        TypedValue::String(text) => object(vec![
            ("type", Some(string("string"))),
            ("value", Some(string(text))),
        ]),
        TypedValue::Decimal(text) => object(vec![
            ("type", Some(string("decimal"))),
            ("value", Some(string(text))),
        ]),
        TypedValue::Date(text) => object(vec![
            ("type", Some(string("date"))),
            ("value", Some(string(text))),
        ]),
        TypedValue::Datetime(text) => object(vec![
            ("type", Some(string("datetime"))),
            ("value", Some(string(text))),
        ]),
        TypedValue::Uuid(text) => object(vec![
            ("type", Some(string("uuid"))),
            ("value", Some(string(text))),
        ]),
        TypedValue::Uri(text) => object(vec![
            ("type", Some(string("uri"))),
            ("value", Some(string(text))),
        ]),
        TypedValue::List(items) => object(vec![
            ("type", Some(string("list"))),
            (
                "value",
                Some(array(&items.iter().map(typed_value).collect::<Vec<_>>())),
            ),
        ]),
        TypedValue::Object(entries) => {
            let mut sorted: Vec<&(crate::scenario::id::FieldName, TypedValue)> =
                entries.iter().collect();
            sorted.sort_by(|left, right| left.0.as_str().cmp(right.0.as_str()));
            let members: Vec<String> = sorted
                .iter()
                .map(|(name, value)| format!("{}:{}", string(name.as_str()), typed_value(value)))
                .collect();
            object(vec![
                ("type", Some(string("object"))),
                ("value", Some(format!("{{{}}}", members.join(",")))),
            ])
        }
    }
}

/// One JSON object assembled from already-canonical members with
/// byte-sorted keys; absent optional members contribute nothing.
pub(crate) fn object(members: Vec<(&str, Option<String>)>) -> String {
    let mut present: Vec<(&str, String)> = members
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect();
    present.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let body: Vec<String> = present
        .iter()
        .map(|(key, value)| format!("{}:{}", string(key), value))
        .collect();
    format!("{{{}}}", body.join(","))
}

/// One JSON array assembled from already-canonical members.
pub(crate) fn array(members: &[String]) -> String {
    format!("[{}]", members.join(","))
}

/// One JSON string literal with minimal canonical escaping.
pub(crate) fn string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('"');
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                escaped.push_str(&format!("\\u{:04x}", control as u32))
            }
            other => escaped.push(other),
        }
    }
    escaped.push('"');
    escaped
}

/// One JSON number.
pub(crate) fn number(value: i64) -> String {
    value.to_string()
}

/// Pre-serialized JSON content passed through verbatim.
pub(crate) fn raw(text: &str) -> String {
    text.to_owned()
}
