//! `lekalo explain`: stable explanations for stable diagnostic codes
//! (issue #96).
//!
//! The catalog is data, embedded and validated once per process, and
//! versioned together with the diagnostic-registry union it explains.
//! One entry per registered code carries what the rule checks, why it
//! matters, common root causes, closed-class fix options (linked to the
//! versioned fix recipes where they exist), before/after examples,
//! related codes, severity/profile behavior, target notes, versioned
//! docs links and waiver policy.
//!
//! The service is read-only and offline: it never loads a project and
//! never claims that a generic fix applies to any current source.
//! Current diagnostic context, when requested, is attached structurally
//! from the caller-supplied report and stays separate from the stable
//! entry. Localized human output may vary; the code plus this document
//! are the stable machine contract.

use serde_json::{json, Value as Json};

pub mod catalog;
pub mod version;

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::token_value;
use crate::explain::catalog::{Catalog, ExplainError};
use crate::explain::version::{RESULT_IDENTITY, RESULT_SCHEMA_VERSION};
use crate::result::{DomainResult, Status};

/// The exact bound on a `--diagnostic-file` input. Reports are closed
/// projections and stay far below this; the bound only refuses abuse.
pub const MAX_REPORT_BYTES: usize = 1024 * 1024;

/// The maximum number of distinct codes resolved per invocation.
const MAX_SELECTORS: usize = 64;

/// Resolve selectors (mixed diagnostic codes and rule ids) against the
/// embedded catalog. Returns exactly one resolved document per distinct
/// selector, in input order; duplicates collapse to their first
/// occurrence. Unknown inputs produce the distinct
/// `explain.code-unexplained` unsupported outcome instead of a document.
pub fn resolve(selectors: &[String]) -> DomainResult {
    let catalog = match embedded() {
        Ok(catalog) => catalog,
        Err(result) => return result,
    };
    if selectors.is_empty() {
        return DomainResult::usage_error();
    }
    if selectors.len() > MAX_SELECTORS {
        return refused(Status::Invalid, "explain.input-invalid", "selector-limit");
    }
    let mut explanations = Vec::new();
    let mut unknown: Option<String> = None;
    let mut seen = std::collections::BTreeSet::new();
    for selector in selectors {
        if !seen.insert(selector.clone()) {
            continue;
        }
        let entry = catalog
            .entry(selector)
            .or_else(|_| catalog.entry_by_rule_id(selector));
        let entry = match entry {
            Ok(entry) => entry,
            Err(ExplainError::UnknownCode(_)) => {
                unknown = Some(selector.clone());
                break;
            }
            Err(_) => return catalog_invalid(),
        };
        explanations.push(entry_json(entry));
    }
    if let Some(selector) = unknown {
        return unexplained(catalog, &selector);
    }
    finish(catalog, "explained", explanations)
}

/// Explain every distinct code found in a caller-supplied diagnostic
/// report (`--diagnostic-file`). The report's diagnostics attach as
/// structural context to their explanations: index, location, message
/// and severity ride `source.report`, and nothing is inferred about any
/// source text.
pub fn explain_report(report_bytes: &[u8]) -> DomainResult {
    let catalog = match embedded() {
        Ok(catalog) => catalog,
        Err(result) => return result,
    };
    if report_bytes.len() > MAX_REPORT_BYTES {
        return refused(Status::Invalid, "explain.report-invalid", "over-limit");
    }
    let value: Json = match serde_json::from_slice(report_bytes) {
        Ok(value) => value,
        Err(_) => return refused(Status::Invalid, "explain.report-invalid", "json"),
    };
    let Some(diagnostics) = value.get("diagnostics").and_then(Json::as_array) else {
        return refused(
            Status::Invalid,
            "explain.report-invalid",
            "diagnostics-missing",
        );
    };
    if diagnostics.is_empty() {
        return refused(
            Status::Invalid,
            "explain.report-invalid",
            "diagnostics-empty",
        );
    }
    if diagnostics.len() > MAX_SELECTORS {
        return refused(
            Status::Invalid,
            "explain.report-invalid",
            "diagnostics-over-limit",
        );
    }
    // Distinct codes in first-appearance order; each explanation carries
    // the first report row that exhibited the code as its context.
    let mut order: Vec<String> = Vec::new();
    let mut contexts: std::collections::BTreeMap<String, Json> = std::collections::BTreeMap::new();
    for (index, item) in diagnostics.iter().enumerate() {
        let Some(code) = item.get("code").and_then(Json::as_str) else {
            return refused(Status::Invalid, "explain.report-invalid", "code-missing");
        };
        let message = item
            .get("message")
            .and_then(Json::as_str)
            .unwrap_or_default();
        if message.is_empty() || message.len() > 512 || message.chars().any(|c| c.is_control()) {
            return refused(Status::Invalid, "explain.report-invalid", "message");
        }
        let severity = item
            .get("severity")
            .and_then(Json::as_str)
            .unwrap_or("error");
        if !matches!(severity, "info" | "warning" | "error") {
            return refused(Status::Invalid, "explain.report-invalid", "severity");
        }
        let location = item.get("source").and_then(|source| {
            let path = source.get("path").and_then(Json::as_str)?;
            let line = source
                .get("range")
                .and_then(|range| range.get("start"))
                .and_then(|start| start.get("line"))
                .and_then(Json::as_u64);
            Some(match line {
                Some(line) => format!("{path}:{line}"),
                None => path.to_owned(),
            })
        });
        let location = location.map(|location| match location.len() {
            0..=512 => location,
            _ => location[..512].to_owned(),
        });
        if !contexts.contains_key(code) {
            order.push(code.to_owned());
        }
        contexts.entry(code.to_owned()).or_insert_with(|| {
            json!({
                "index": index,
                "location": location,
                "message": message,
                "severity": severity,
            })
        });
    }
    let mut explanations = Vec::new();
    for code in &order {
        let entry = match catalog.entry(code) {
            Ok(entry) => entry,
            Err(ExplainError::UnknownCode(_)) => return unexplained(catalog, code),
            Err(_) => return catalog_invalid(),
        };
        let mut explanation = entry_json(entry);
        if let Some(context) = contexts.get(code) {
            explanation["source"]["report"] = context.clone();
        }
        explanations.push(explanation);
    }
    finish(catalog, "explained", explanations)
}

/// The embedded, once-validated catalog, or the typed unavailable
/// refusal when the developer-owned table fails validation.
fn embedded() -> Result<&'static Catalog, DomainResult> {
    Catalog::embedded()
        .map_err(|_| refused(Status::Unavailable, "explain.catalog-invalid", "embedded"))
}

fn catalog_invalid() -> DomainResult {
    refused(
        Status::Unavailable,
        "explain.catalog-invalid",
        "entry-shape",
    )
}

/// The stable JSON projection of one catalog entry, including the
/// `source.provider: "core"` provenance marker (adapters may publish
/// namespaced explanations with their own provider identities).
fn entry_json(entry: &crate::explain::catalog::Entry) -> Json {
    let mut value = serde_json::to_value(entry).expect("catalog entry serializes");
    value["source"] = json!({ "provider": "core" });
    value
}

/// The success document: closed result envelope bytes plus the human
/// summary. The human projection is intentionally short; the document is
/// the machine contract.
fn finish(catalog: &Catalog, state: &str, explanations: Vec<Json>) -> DomainResult {
    let codes: Vec<String> = explanations
        .iter()
        .filter_map(|explanation| {
            explanation
                .get("code")
                .and_then(Json::as_str)
                .map(str::to_owned)
        })
        .collect();
    let document = json!({
        "status": "valid",
        "schemaVersion": RESULT_SCHEMA_VERSION,
        "identity": RESULT_IDENTITY,
        "state": state,
        "catalogVersion": catalog.catalog_version(),
        "registryIdentity": catalog.registry_identity(),
        "explanations": explanations,
    });
    let human = format!(
        "{} explanation(s): {} [catalog {}, explains {}]",
        codes.len(),
        codes.join(", "),
        catalog.catalog_version(),
        catalog.registry_identity()
    );
    DomainResult::receipt(
        serde_json::to_string_pretty(&document).expect("explain result serializes"),
        human,
    )
}

/// The distinct unknown-code outcome (exit 4, `unsupported`). The stable
/// `explain.code-unexplained` diagnostic rides the envelope and the
/// document still exposes `state: "unexplained"`, so a caller can tell
/// "this code has no explanation" from every other refusal.
fn unexplained(catalog: &Catalog, selector: &str) -> DomainResult {
    let document = json!({
        "status": "unsupported",
        "schemaVersion": RESULT_SCHEMA_VERSION,
        "identity": RESULT_IDENTITY,
        "state": "unexplained",
        "catalogVersion": catalog.catalog_version(),
        "registryIdentity": catalog.registry_identity(),
        "explanations": [],
    });
    let mut data = crate::diagnostics::DataObject::new();
    data.insert("selector".to_owned(), token_value(selector));
    let diagnostic = build("explain.code-unexplained", None, None, data)
        .expect("registered explain diagnostic builds");
    let human = format!("no explanation for {selector}: the code is not in this catalog");
    DomainResult::unsupported_json(
        serde_json::to_string_pretty(&document).expect("explain result serializes"),
        human,
        DomainResult::from_wire_set(Status::Unsupported, vec![diagnostic]),
    )
}

fn refused(status: Status, id: &'static str, detail: &'static str) -> DomainResult {
    let mut data = crate::diagnostics::DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    let diagnostic = build(id, None, None, data).expect("registered explain diagnostic builds");
    DomainResult::from_statuses(status, vec![diagnostic])
}

/// A public refusal with an explain-family diagnostic id and bounded
/// detail token, for callers outside the crate (the CLI's own read
/// failures refuse identically to the service's).
pub fn refusal(id: &'static str, detail: &'static str) -> DomainResult {
    refused(Status::Invalid, id, detail)
}
