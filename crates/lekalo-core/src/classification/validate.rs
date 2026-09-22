//! Validation of one classification pair against the pinned
//! compilation (issue #87).
//!
//! Custody checks bind the attachment and policy to the exact project,
//! Model pin, and IR pin — the declared `modelRef`/`irRef` digests are
//! compared against the exact digests of the loaded compilation (the
//! canonical Model bytes and the canonical IR bytes, the same spelling
//! the effect-graph builder records), and any mismatch refuses before
//! any work; subject resolution rejects unknown symbols and
//! unaddressable members; the policy/grant coherence checks encode
//! the hard rules — every resolved kind has a policy row, grants are
//! strict lowerings through declared roles, `credential` never
//! declassifies, and approvals are never self-referential.

use crate::diagnostics::DiagnosticSet;
use crate::ir::CompiledProject;

use super::diagnostic;
use super::policy::PolicyAttachment;
use super::resolve::{Resolution, ResolvedKind};
use super::types::DataKind;
use super::wire::Attachment;

/// Why one subject does not resolve in the bound compilation.
pub enum SubjectError {
    /// The head semantic id does not resolve.
    UnknownSymbol,
    /// A field path segment does not exist on the resolved definition.
    UnknownField,
}

/// One validation finding: the registered rule id and the subject
/// reference. Metadata-only; never a value.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingRow {
    /// The registered rule id (`classification.*`).
    pub rule: String,
    /// The subject reference (a bounded path string).
    pub subject: String,
}

impl FindingRow {
    /// The canonical wire object.
    pub fn to_wire(&self) -> serde_json::Value {
        serde_json::json!({
            "rule": self.rule,
            "subject": self.subject,
        })
    }
}

/// The aggregate outcome of the policy and grant validation.
#[derive(Clone, Debug, Default)]
pub struct ValidationOutcome {
    /// Whether any error-severity finding exists.
    pub invalid: bool,
    /// The findings in canonical order.
    pub rows: Vec<FindingRow>,
}

impl ValidationOutcome {
    /// The number of findings.
    pub fn finding_count(&self) -> usize {
        self.rows.len()
    }

    /// The canonical findings array wire fragment (leading `[`).
    pub fn wire_findings(&self) -> String {
        let mut json = String::from(",\"findings\":[");
        for (index, row) in self.rows.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str(&row.to_wire().to_string());
        }
        json.push(']');
        json
    }
}

/// Bind the attachment and policy to the exact project custody: same
/// project id, same Model pin, same IR pin across the two documents
/// and the bound compilation. The Model pin binds the exact canonical
/// Model bytes the compilation came from; the IR pin binds the exact
/// canonical IR bytes (the same digest the effect-graph builder
/// records). A stale or foreign attachment is a refusal, never a pass.
pub fn validate_custody(
    attachment: &Attachment,
    policy: &PolicyAttachment,
    project: &CompiledProject,
    model_json: &str,
) -> Result<(), DiagnosticSet> {
    let Some(declared_project) = project.project.as_ref() else {
        return Err(diagnostic::document_invalid("custody-project", None));
    };
    let project_id = declared_project.id.as_str();
    if attachment.project_id().as_str() != project_id {
        return Err(diagnostic::document_invalid("custody-project", None));
    }
    if policy.project_id().as_str() != project_id {
        return Err(diagnostic::document_invalid("custody-project", None));
    }
    // The Model pin: the exact canonical Model bytes of the loaded
    // project (the same digest the #85 NFR custody check records).
    let model_digest = format!(
        "sha256:{}",
        crate::digest::sha256_hex(model_json.as_bytes())
    );
    for pin in [attachment.model_ref(), policy.model_ref()] {
        if pin.0 != project.model_version.as_str() || pin.1.as_str() != model_digest {
            return Err(diagnostic::document_invalid("custody-model", None));
        }
    }
    // The IR pin: the exact canonical IR bytes of the compilation.
    let ir_digest = format!(
        "sha256:{}",
        crate::digest::sha256_hex(project.to_canonical_json().as_bytes())
    );
    for pin in [attachment.ir_ref(), policy.ir_ref()] {
        if pin.1.as_str() != ir_digest {
            return Err(diagnostic::document_invalid("custody-ir", None));
        }
    }
    Ok(())
}

/// Resolve every declared subject against the pinned compilation and
/// reject unknown symbols or field paths. Returns the resolution over
/// the validated attachment.
pub fn validate_subjects(
    attachment: &Attachment,
    compilation: &crate::ir::Compilation,
) -> Result<Resolution, DiagnosticSet> {
    let project = &compilation.project;
    for entry in attachment.classifications() {
        check_subject_resolves(entry.subject(), project)?;
    }
    for grant in attachment.declassifications() {
        check_subject_resolves(grant.subject(), project)?;
    }
    for question in attachment.open_questions() {
        if let Some(subject) = question.subject() {
            check_subject_resolves(subject, project)?;
        }
    }
    Ok(Resolution::build(attachment))
}

/// Whether one subject resolves in the pinned compilation.
fn check_subject_resolves(
    subject: &super::types::SubjectPath,
    project: &CompiledProject,
) -> Result<(), DiagnosticSet> {
    let definition = project
        .definitions
        .iter()
        .find(|definition| definition.id().as_str() == subject.semantic_id());
    let Some(definition) = definition else {
        return Err(diagnostic::unknown_subject(
            "unknown-symbol",
            subject.as_str(),
        ));
    };
    let mut fields = field_map(definition);
    for (index, segment) in subject.field_segments().enumerate() {
        let Some(target_symbol) = fields.remove(segment) else {
            return Err(diagnostic::unknown_subject(
                if index == 0 {
                    "unknown-field"
                } else {
                    "unknown-payload-field"
                },
                subject.as_str(),
            ));
        };
        // The next segments traverse the referenced definition's fields;
        // a scalar/enum leaf closes the path (deeper segments reject).
        fields = match project
            .definitions
            .iter()
            .find(|definition| definition.id().as_str() == target_symbol)
        {
            Some(referenced) => field_map(referenced),
            None => std::collections::BTreeMap::new(),
        };
    }
    Ok(())
}

/// The field-to-definition map of one definition: entity and
/// value-object fields map to the referenced definitions.
fn field_map(definition: &crate::ir::Definition) -> std::collections::BTreeMap<String, String> {
    let members: &[crate::ir::Field] = match definition {
        crate::ir::Definition::Entity(entity) => &entity.fields,
        crate::ir::Definition::ValueObject(value_object) => &value_object.fields,
        crate::ir::Definition::Command(command) => &command.input,
        crate::ir::Definition::Event(event) => &event.payload,
        _ => &[],
    };
    field_map_of(members)
}

/// The field map of an explicit member list.
fn field_map_of(members: &[crate::ir::Field]) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    for field in members {
        let target = match &field.r#type {
            crate::ir::TypeRef::Ref(symbol) => symbol.as_str().to_owned(),
            crate::ir::TypeRef::List(inner) | crate::ir::TypeRef::Optional(inner) => {
                match inner.as_ref() {
                    crate::ir::TypeRef::Ref(symbol) => symbol.as_str().to_owned(),
                    _ => continue,
                }
            }
        };
        map.insert(field.name.as_str().to_owned(), target);
    }
    map
}

/// Validate the policy/grant coherence of one resolved attachment:
/// policy coverage of every declared kind, grant strict lowering with
/// real approvals, and the credential seal. Returns the aggregate
/// outcome; a structured wire refusal (a malformed grant) aborts with
/// the typed set instead.
pub fn validate_policy_and_grants(
    attachment: &Attachment,
    policy: &PolicyAttachment,
    resolution: &Resolution,
    project: &CompiledProject,
) -> Result<ValidationOutcome, DiagnosticSet> {
    validate_policy_and_grants_as_of(attachment, policy, resolution, project, DEFAULT_AS_OF)
}

/// The fixed validation as-of date (pure: no wall-clock input, the
/// report and validate surfaces stay deterministic functions of their
/// bytes).
pub const DEFAULT_AS_OF: &str = "2026-01-01T00:00:00Z";

/// [`validate_policy_and_grants`] with an explicit evaluation date for
/// the grant-expiry rule (LEK-CLS-007).
pub fn validate_policy_and_grants_as_of(
    attachment: &Attachment,
    policy: &PolicyAttachment,
    resolution: &Resolution,
    project: &CompiledProject,
    as_of: &str,
) -> Result<ValidationOutcome, DiagnosticSet> {
    let mut outcome = ValidationOutcome::default();
    // Every resolvable kind must have a policy row (plan §1.2: policy
    // coverage is total over the declared kinds).
    for entry in attachment.classifications() {
        if let ResolvedKind::Classified(kind) = resolution.resolve(entry.subject()) {
            if policy.rule(kind).is_none() {
                outcome.invalid = true;
                outcome.rows.push(FindingRow {
                    rule: "classification.kind-rule-missing".to_owned(),
                    subject: entry.subject().as_str().to_owned(),
                });
            }
        }
    }
    for grant in attachment.declassifications() {
        // Credential never declassifies downward (the wire already
        // refuses credential rows with roles; the seal is the rule).
        if !grant.from_kind().declassifiable() {
            return Err(diagnostic::invalid_declassification(
                "credential-sealed",
                grant.subject().as_str(),
            ));
        }
        if grant.to_kind() == grant.from_kind() {
            return Err(diagnostic::invalid_declassification(
                "same-kind",
                grant.subject().as_str(),
            ));
        }
        if !grant.to_kind().strict_lowering_of(grant.from_kind()) {
            return Err(diagnostic::invalid_declassification(
                "not-a-lowering",
                grant.subject().as_str(),
            ));
        }
        // The review reference must be present (the wire enforces the
        // shape) and the from-kind rule must exist to carry roles.
        let from_rule = match policy.rule(grant.from_kind()) {
            None => {
                outcome.invalid = true;
                outcome.rows.push(FindingRow {
                    rule: "classification.kind-rule-missing".to_owned(),
                    subject: grant.subject().as_str().to_owned(),
                });
                continue;
            }
            Some(rule) => rule,
        };
        if from_rule.declassify_roles().is_empty() {
            outcome.invalid = true;
            outcome.rows.push(FindingRow {
                rule: "classification.missing-approval".to_owned(),
                subject: grant.subject().as_str().to_owned(),
            });
            continue;
        }
        // Expiry (LEK-CLS-007): a grant past its `expiresAt` is dead —
        // reported against the fixed validation as-of date, never
        // silently lowering anything.
        if grant
            .expires_at()
            .is_some_and(|expires_at| expires_at.as_str() < as_of)
        {
            outcome.invalid = true;
            outcome.rows.push(FindingRow {
                rule: "classification.expired-declassification".to_owned(),
                subject: grant.subject().as_str().to_owned(),
            });
        }
        // Self-approval: the grant's own id as the review reference.
        if grant.approved_by().as_str() == grant.id().as_str() {
            outcome.invalid = true;
            outcome.rows.push(FindingRow {
                rule: "classification.self-approved".to_owned(),
                subject: grant.subject().as_str().to_owned(),
            });
        }
    }
    // The strict-profile sensitive-sink rule (plan §2.3): declared
    // graph subjects with no explicit classification reject.
    outcome
        .rows
        .extend(strict_sensitive_sink_findings(attachment, project));
    if outcome
        .rows
        .iter()
        .any(|row| row.rule == "classification.unclassified-sensitive-sink")
    {
        outcome.invalid = true;
    }
    // The grant profile rule: unclassified subjects never resolve
    // through a grant; nothing to check here beyond the resolution.
    Ok(outcome)
}

/// The strict-profile sensitive-sink rule over the compilation (plan
/// §2.3): every sensitive declared-graph subject must be explicitly
/// classified — a definition-level entry, a field-level entry, or a
/// grant — never just the profile default. A subject whose kind comes
/// only from `unclassifiedFields`/`unclassifiedPayloads` is the
/// unclassified-on-a-sensitive-sink state the strict profile rejects.
pub fn strict_sensitive_sink_findings(
    attachment: &Attachment,
    project: &CompiledProject,
) -> Vec<FindingRow> {
    let mut rows = Vec::new();
    if attachment.defaults().profile() != super::types::Profile::Strict {
        return rows;
    }
    let resolution = super::resolve::Resolution::build(attachment);
    // The sensitive subjects of the declared graph: every entity the
    // queries read and the declared effects write, plus every emitted
    // event. Unclassified counts as sensitive (unknown is never safe).
    for subject in declared_graph_subjects(project) {
        let kind = resolution.resolve(&subject);
        let covered = resolution.exact(&subject).is_some()
            || resolution.definition_default(&subject).is_some();
        if covered {
            continue;
        }
        if !kind.is_sensitive() {
            continue;
        }
        rows.push(FindingRow {
            rule: "classification.unclassified-sensitive-sink".to_owned(),
            subject: subject.as_str().to_owned(),
        });
    }
    rows
}

/// Every definition-level subject the declared effect graph touches:
/// the entities of declared reads and effects, plus the events they
/// emit.
fn declared_graph_subjects(project: &CompiledProject) -> Vec<super::types::SubjectPath> {
    let mut subjects = std::collections::BTreeSet::new();
    for definition in &project.definitions {
        match definition {
            crate::ir::Definition::Query(query) => {
                for read in &query.reads {
                    push_subject(&mut subjects, read.as_str());
                }
            }
            crate::ir::Definition::Command(command) => {
                for effect in &command.effects {
                    if let Some(crate::ir::Definition::Effect(effect_definition)) = project
                        .definitions
                        .iter()
                        .find(|definition| definition.id().as_str() == effect.as_str())
                    {
                        push_subject(&mut subjects, effect_definition.entity.as_str());
                        for emitted in &effect_definition.emits {
                            push_subject(&mut subjects, emitted.as_str());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    subjects.into_iter().collect()
}

/// Parse and record one definition-level subject; malformed ids cannot
/// reach compilation, so a parse failure here is a skip, not an error.
fn push_subject(subjects: &mut std::collections::BTreeSet<super::types::SubjectPath>, id: &str) {
    if let Ok(subject) = super::types::SubjectPath::parse(id) {
        subjects.insert(subject);
    }
}

/// Whether the kind is sink-eligible for the given ceiling.
pub fn kind_within_ceiling(kind: DataKind, ceiling: DataKind) -> bool {
    kind.rank() <= ceiling.rank()
}

/// The conventional attachment home under the project root.
pub const ATTACHMENT_PATH: &str = "lekalo/classification.json";
/// The conventional policy home under the project root.
pub const POLICY_PATH: &str = "lekalo/classification-policy.json";

/// The discovered classification pair of one project root, or `None`
/// when the project declares no classification attachment.
pub fn discover(
    root: &std::path::Path,
) -> Result<Option<(Attachment, PolicyAttachment)>, DiagnosticSet> {
    let attachment_path = root.join(ATTACHMENT_PATH);
    let Ok(attachment_bytes) = std::fs::read(&attachment_path) else {
        return Ok(None);
    };
    let attachment = Attachment::parse(&attachment_bytes)?;
    let policy_bytes = std::fs::read(root.join(POLICY_PATH))
        .map_err(|_| diagnostic::policy_missing("policy-document-unreadable"))?;
    let policy = PolicyAttachment::parse(&policy_bytes)?;
    Ok(Some((attachment, policy)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The expiry rule (LEK-CLS-007): a grant past its `expiresAt`
    /// produces `classification.expired-declassification` against the
    /// fixed as-of date; a live grant does not.
    #[test]
    fn expired_grants_are_findings_and_live_grants_pass() {
        let grant = |expires_at: &str| {
            format!(
                r#"{{
          "id": "grant.core.export-user-derived@1.0.0",
          "subject": "notify.user_id",
          "fromKind": "personal",
          "toKind": "derived",
          "approvedBy": "review-2025-001",
          "justification": "Aggregated analytics only.",
          "expiresAt": "{expires_at}"
        }}"#
            )
        };
        let attachment_for = |grant: &str| {
            let model = format!("sha256:{}", "a".repeat(64));
            let ir = format!("sha256:{}", "b".repeat(64));
            let json = format!(
                r#"{{
      "schemaVersion": "lekalo/data-classification/v0.4.0",
      "identity": "dev.lekalo.data-classification@0.4.0",
      "attachmentRevision": "1.0.0",
      "projectId": "planner",
      "modelRef": {{"modelVersion": "0.2.16", "digest": "{model}"}},
      "irRef": {{"irVersion": "0.2.16", "digest": "{ir}"}},
      "defaults": {{"profile": "default", "unclassifiedFields": "internal", "unclassifiedPayloads": "confidential"}},
      "classifications": [
        {{"subject": "notify.user_id", "kind": "personal"}}
      ],
      "declassifications": [{grant}],
      "openQuestions": []
    }}"#
            );
            Attachment::parse(json.as_bytes()).expect("parses")
        };
        let model = format!("sha256:{}", "a".repeat(64));
        let ir = format!("sha256:{}", "b".repeat(64));
        let policy_json = format!(
            r#"{{
      "schemaVersion": "lekalo/classification-policy/v0.4.0",
      "identity": "dev.lekalo.classification-policy@0.4.0",
      "attachmentRevision": "1.0.0",
      "projectId": "planner",
      "modelRef": {{"modelVersion": "0.2.16", "digest": "{model}"}},
      "irRef": {{"irVersion": "0.2.16", "digest": "{ir}"}},
      "kinds": [{{
        "kind": "personal",
        "readers": ["tenant"], "writers": ["tenant"],
        "destinations": ["internal-service"],
        "masking": {{"strategy": "tokenize", "policyRef": "privacy-policy.masking.personal"}},
        "consentRequired": false, "crossTenant": "forbidden",
        "declassifyRoles": ["data-steward"]
      }}],
      "sinks": {{"logs": {{"maxKind": "internal"}}, "traces": {{"maxKind": "internal"}}, "contextCapsules": {{"maxKind": "internal"}}, "diagnostics": {{"maxKind": "internal"}}, "evidence": {{"maxKind": "public"}}, "exports": {{"maxKind": "derived"}}}},
      "openQuestions": []
    }}"#
        );
        let policy = PolicyAttachment::parse(policy_json.as_bytes()).expect("policy parses");

        let expired = attachment_for(&grant("2020-01-01T00:00:00Z"));
        let resolution = Resolution::build(&expired);
        let outcome = validate_policy_and_grants(&expired, &policy, &resolution, empty_project())
            .expect("runs");
        assert!(outcome.invalid);
        assert!(
            outcome
                .rows
                .iter()
                .any(|row| row.rule == "classification.expired-declassification"),
            "{outcome:?}"
        );

        let live = attachment_for(&grant("2999-01-01T00:00:00Z"));
        let resolution = Resolution::build(&live);
        let outcome =
            validate_policy_and_grants(&live, &policy, &resolution, empty_project()).expect("runs");
        assert!(!outcome.invalid, "{outcome:?}");
    }

    /// An empty compilation (the grant checks run over the attachment
    /// and policy alone).
    fn empty_project() -> &'static CompiledProject {
        // The strict sensitive-sink rule only reads definitions; an
        // empty definition list means no graph subjects.
        static PROJECT: std::sync::OnceLock<CompiledProject> = std::sync::OnceLock::new();
        PROJECT.get_or_init(|| {
            let model = crate::loader::NormalizedModel {
                model_version: crate::loader::project_docs::ModelVersion::Current,
                project: None,
                modules: Vec::new(),
                definitions: Vec::new(),
            };
            crate::ir::compile(&model)
                .expect("empty model compiles")
                .project
        })
    }
}
