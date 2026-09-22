//! The checked mode of the OpenAPI projection (issue #46, plan §4.3).
//!
//! [`check`] verifies one maintained document against the current
//! attachment: bind every existing operation (`x-lekalo-endpoint`
//! first, `operationId` second), recompute the fragments from the
//! validated attachment, and classify every pointer. The verifier *is*
//! the merge ([`super::merge`]) run in verification posture — merge is
//! verified, not just performed. A manual operation binding nothing is
//! preserved and reported informationally; an operation whose anchor
//! resolves to nothing is `openapi.binding-unresolved`; a bound
//! operation differing from its recomputed fragment is
//! `openapi.drift`; a pointer under manual ownership colliding with a
//! generated one is `openapi.merge-conflict`.

use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;
use crate::result::Status;
use crate::transport_http::{TransportDocument, ValidationContext};

use super::bind::BindingKey;
use super::diagnostic;
use super::fragments::{Fragments, OwnershipManifest};
use super::merge::merge;
use super::types::RenderConfig;

/// One finished check: the per-pointer classification plus the merged
/// (would-be) tree.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct CheckReport {
    /// Bound operations differing from their recomputed fragments:
    /// `(pointer, endpoint id)`.
    pub drifts: Vec<(String, String)>,
    /// Generated pointers under manual ownership: `(pointer, owner)`.
    pub conflicts: Vec<(String, String)>,
    /// Manual operations binding nothing: `(pointer, detail)` —
    /// informational, preserved.
    pub manual: Vec<(String, String)>,
    /// Operations whose anchor or id resolves to nothing:
    /// `(pointer, detail)`.
    pub unresolved: Vec<(String, String)>,
    /// Bound manual operations identical to their recomputed fragment.
    pub bound_clean: usize,
}

impl CheckReport {
    /// Whether the maintained document is conformant: no drift, no
    /// conflict, no unresolved binding.
    pub fn is_conformant(&self) -> bool {
        self.drifts.is_empty() && self.conflicts.is_empty() && self.unresolved.is_empty()
    }

    /// The registered diagnostics of one report. Any drift, conflict,
    /// or unresolved binding is an `invalid` set; a conformant check
    /// with unbound-manual inventory stays `valid` (the manual
    /// operations are informational).
    pub fn diagnostics(&self) -> DiagnosticSet {
        let mut all = Vec::new();
        for (pointer, endpoint) in &self.drifts {
            let set = diagnostic::drift(
                pointer,
                &format!("endpoint:{}", diagnostic::bounded(endpoint)),
            );
            all.extend(set.as_slice().iter().cloned());
        }
        for (pointer, owner) in &self.conflicts {
            let set = diagnostic::merge_conflict(
                pointer,
                &format!("manual-owner:{}", diagnostic::bounded(owner)),
            );
            all.extend(set.as_slice().iter().cloned());
        }
        for (pointer, detail) in &self.unresolved {
            let set = diagnostic::binding_unresolved(pointer, detail);
            all.extend(set.as_slice().iter().cloned());
        }
        DiagnosticSet::try_from_unsorted(all, Status::Invalid)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
    }
}

/// Check one maintained document tree against the current attachment.
/// `ownership` is the document's existing ownership sidecar manifest
/// (empty when none was shipped); pure and read-only.
pub fn check(
    existing: &Json,
    attachment: &TransportDocument,
    context: &ValidationContext<'_>,
    config: &RenderConfig,
    ownership: &OwnershipManifest,
) -> Result<CheckReport, DiagnosticSet> {
    let rendered = super::render::render(attachment, context, config)?;
    let fragments = Fragments::new(&rendered);
    let generated = OwnershipManifest::of_document(&rendered, Default::default());
    let outcome = merge(existing, &fragments, &generated, ownership)?;

    let mut report = CheckReport {
        drifts: outcome.drifts().to_vec(),
        conflicts: outcome.conflicts().to_vec(),
        manual: Vec::new(),
        unresolved: Vec::new(),
        bound_clean: 0,
    };
    for (pointer, detail) in outcome.manual() {
        match detail.as_str() {
            "bound-identical" => report.bound_clean += 1,
            other => report.manual.push((pointer.clone(), other.to_owned())),
        }
    }

    // Anchors resolving to nothing: every existing operation carrying
    // an `x-lekalo-endpoint` that no generated endpoint of this render
    // declares is a registered refusal (LEK-OAPI-006).
    let endpoints = rendered_endpoints(&rendered);
    for (pointer, (binding, _)) in existing_operations(existing) {
        let Some(anchor) = binding.and_then(|key| key.endpoint) else {
            continue;
        };
        if !endpoints.iter().any(|id| *id == anchor) {
            report.unresolved.push((
                pointer,
                format!("endpoint:{}", diagnostic::bounded(&anchor)),
            ));
        }
    }
    Ok(report)
}

/// The endpoint ids of one rendered document, byte-sorted.
fn rendered_endpoints(rendered: &super::render::OpenApiDocument) -> Vec<String> {
    let mut endpoints: Vec<String> = rendered
        .operation_pointers()
        .iter()
        .map(|(_, endpoint)| endpoint.clone())
        .collect();
    endpoints.sort();
    endpoints.dedup();
    endpoints
}

/// Every existing operation of one document: pointer → binding key.
fn existing_operations(existing: &Json) -> Vec<(String, (Option<BindingKey>, Json))> {
    let mut operations = Vec::new();
    let Some(paths) = existing.get("paths").and_then(Json::as_object) else {
        return operations;
    };
    for (template, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for (method, operation) in item {
            if !is_method(method) {
                continue;
            }
            operations.push((
                format!("/paths/{}/{}", super::id::escape_pointer(template), method),
                (BindingKey::of(operation), operation.clone()),
            ));
        }
    }
    operations
}

/// Whether one path-item member key is an operation slot.
fn is_method(key: &str) -> bool {
    matches!(
        key,
        "get" | "put" | "post" | "delete" | "options" | "head" | "patch" | "trace"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(
        drifts: Vec<(String, String)>,
        conflicts: Vec<(String, String)>,
        unresolved: Vec<(String, String)>,
    ) -> CheckReport {
        CheckReport {
            drifts,
            conflicts,
            manual: vec![("/paths/~1health/get".to_owned(), "unbound".to_owned())],
            unresolved,
            bound_clean: 2,
        }
    }

    #[test]
    fn conformant_reports_stay_valid_with_informational_manual() {
        let report = report(vec![], vec![], vec![]);
        assert!(report.is_conformant());
        assert_eq!(
            report.diagnostics().as_slice().len(),
            0,
            "a conformant report carries no diagnostics; the manual inventory is data, not refusal"
        );
    }

    #[test]
    fn drift_and_conflict_and_unresolved_classify_their_rules() {
        let report = report(
            vec![(
                "/paths/~1tasks/get".to_owned(),
                "planner.endpoint_list_tasks".to_owned(),
            )],
            vec![("/paths/~1legacy/get".to_owned(), String::new())],
            vec![(
                "/paths/~1old/get".to_owned(),
                "endpoint:planner.endpoint_removed".to_owned(),
            )],
        );
        assert!(!report.is_conformant());
        let set = report.diagnostics();
        let ids: Vec<&str> = set.as_slice().iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"openapi.drift"));
        assert!(ids.contains(&"openapi.merge-conflict"));
        assert!(ids.contains(&"openapi.binding-unresolved"));
    }
}
