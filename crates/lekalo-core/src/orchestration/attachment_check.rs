//! Host custody for external pre-generation attachment checks.
use crate::diagnostics::{DataObject, DataValue, Scalar};
use crate::ir::CompiledProject;
use crate::result::{DomainResult, Status};
use crate::runtime::{self, services};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Finding {
    code: String,
    semantic_id: Option<String>,
    pointer: Option<String>,
    detail: String,
}

fn refusal(project: &str, family: &str, rule: &str, findings: &[Finding]) -> DomainResult {
    let first = &findings[0];
    let mut data = DataObject::new();
    data.insert(
        "projectId".into(),
        crate::diagnostics::types::token_value(project),
    );
    data.insert(
        "codes".into(),
        DataValue::List(
            findings
                .iter()
                .take(16)
                .map(|f| Scalar::Token(crate::diagnostics::types::bound_token(&f.code)))
                .collect(),
        ),
    );
    let pointer = first
        .pointer
        .as_ref()
        .map_or(String::new(), |p| format!(" at {p}"));
    let detail = format!(
        "{}{pointer}: {}",
        first.semantic_id.as_deref().unwrap_or(family),
        first.detail
    );
    data.insert(
        "detail".into(),
        crate::diagnostics::types::token_value(&detail),
    );
    let diagnostic = crate::diagnostics::normalize::build(rule, None, None, data)
        .expect("registered attachment diagnostic");
    DomainResult::Invalid {
        diagnostics: super::generate::wire_set(Status::Invalid, vec![diagnostic]),
    }
}

pub(super) fn check(
    root: &Path,
    family: &str,
    rule: &str,
    compilation: &CompiledProject,
    project: &str,
) -> Result<(), DomainResult> {
    const LIMIT: usize = 8 * 1024 * 1024;
    let relative = format!("lekalo/{family}/{project}.{family}.json");
    if matches!(std::fs::symlink_metadata(root.join(&relative)),Err(e) if e.kind()==std::io::ErrorKind::NotFound)
    {
        return Ok(());
    }
    let bytes = runtime::read(root, &relative, LIMIT).map_err(|_| {
        refusal(
            project,
            family,
            rule,
            &[Finding {
                code: format!("{family}.input-unreadable"),
                semantic_id: None,
                pointer: None,
                detail: format!("the {family} input document is unreadable"),
            }],
        )
    })?;
    let invoke = || -> runtime::Result<Vec<Finding>> {
        let service = services::one(root, &format!("attachments/{family}"))?;
        let mut digests = BTreeMap::new();
        for (key, path) in [
            ("ir", format!(".lekalo/cache/ir/{project}.json")),
            ("types", format!("lekalo/types/{project}.types.json")),
            (
                "operations",
                format!("lekalo/operations/{project}.operations.json"),
            ),
        ] {
            if let Ok(bytes) = runtime::read(root, &path, LIMIT) {
                digests.insert(key, runtime::digest(&bytes));
            }
        }
        let registry = crate::error_contract::source::for_project(root, compilation).ok();
        let bindings = registry.as_ref().map(|r| {
            r.bindings()
                .iter()
                .map(|b| {
                    let errors: Vec<_> = b
                        .errors()
                        .members()
                        .iter()
                        .map(|member| member.id().as_str())
                        .collect();
                    (
                        b.operation().as_str(),
                        json!({"kind":b.kind().as_str(),"errors":errors}),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        });
        let mut transport = Value::Null;
        let mut transport_errors = Vec::new();
        if family == "routes" {
            if let Ok(Some(doc)) = crate::transport_http::read_document(root) {
                if let Ok(canonical) = doc.canonical_bytes() {
                    digests.insert("transport", runtime::digest(canonical.as_bytes()));
                    transport = serde_json::from_str(&canonical)
                        .map_err(|_| runtime::error("transport-invalid"))?;
                }
                if let Some(registry) = &registry {
                    if let Err(diagnostics) = crate::transport_http::validate(
                        &doc,
                        &crate::transport_http::ValidationContext::new(compilation)
                            .with_errors(registry),
                    ) {
                        transport_errors = diagnostics
                            .reason_ids()
                            .into_iter()
                            .map(str::to_owned)
                            .collect();
                    }
                }
            }
        }
        let ir: Value = serde_json::from_str(&compilation.to_canonical_json())
            .map_err(|_| runtime::error("ir-invalid"))?;
        let document =
            std::str::from_utf8(&bytes).map_err(|_| runtime::error("document-encoding"))?;
        let result=service.invoke("attachments.check",json!({"schema":"lekalo/attachment-check/v1","family":family,"document":document,
            "evidence":{"project":project,"ir":ir,"registry":bindings,"digests":digests,"transport":transport,"transport_errors":transport_errors}}))?;
        if result["schema"] != "lekalo/attachment-findings/v1"
            || result.as_object().map(|r| r.len()) != Some(2)
        {
            return Err(runtime::error("attachment-response-invalid"));
        }
        let findings: Vec<Finding> = serde_json::from_value(result["findings"].clone())
            .map_err(|_| runtime::error("attachment-response-invalid"))?;
        if findings.len() > 8192 || findings.iter().any(|f| !runtime::config::token(&f.code)) {
            return Err(runtime::error("attachment-response-invalid"));
        }
        Ok(findings)
    };
    let findings = invoke().map_err(|error| runtime::outcome(Err(error)))?;
    if findings.is_empty() {
        Ok(())
    } else {
        Err(refusal(project, family, rule, &findings))
    }
}
