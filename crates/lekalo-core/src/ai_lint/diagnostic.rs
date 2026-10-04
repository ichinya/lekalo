//! Every diagnostic resolves the embedded registry, including policy failures.
use super::wire::Finding;
use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{DomainResult, Status};

pub fn summary(id: &str, detail: &str) -> Diagnostic {
    let mut data = DataObject::new();
    data.insert("detail".into(), token_value(detail));
    build(id, None, None, data).expect("embedded AI lint rule")
}
pub fn failure(id: &str, detail: &str) -> DomainResult {
    let status = if id == "ai-lint.version-unsupported" {
        Status::UnsupportedVersion
    } else {
        Status::Invalid
    };
    let set = DiagnosticSet::try_from_unsorted(vec![summary(id, detail)], status)
        .expect("registered AI lint failure status");
    if status == Status::UnsupportedVersion {
        DomainResult::UnsupportedVersion { diagnostics: set }
    } else {
        DomainResult::invalid(set)
    }
}
pub fn finding(f: &Finding) -> Diagnostic {
    let mut data = DataObject::new();
    for (key, value) in [
        ("finding", f.id.clone()),
        (
            "evidence",
            f.evidence
                .first()
                .cloned()
                .unwrap_or_else(|| "model".into()),
        ),
        (
            "confidence",
            serde_json::to_value(f.confidence)
                .expect("enum")
                .as_str()
                .expect("enum string")
                .into(),
        ),
        (
            "claim",
            serde_json::to_value(f.claim)
                .expect("enum")
                .as_str()
                .expect("enum string")
                .into(),
        ),
    ] {
        data.insert(key.into(), token_value(&value));
    }
    let mut d = build(&f.rule_id, f.semantic_symbol.known().cloned(), None, data)
        .expect("embedded AI lint rule");
    if f.severity == "info" {
        d.severity = crate::diagnostics::types::Severity::Info;
    }
    d
}
