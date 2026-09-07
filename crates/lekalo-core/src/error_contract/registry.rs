//! The embedded error registry (issue #62).
//!
//! [`ErrorRegistry::embedded`] parses and validates the exact
//! `include_bytes!` registry once per process. The registry is the only
//! source of error identity: codes, categories, payload schemas, message
//! templates, retry/idempotency/effect metadata, coverage, and source
//! requirements all come from validated constructors, and the embedded
//! bytes must equal the canonical re-rendering exactly (duplicate keys,
//! unknown fields, and noncanonical bytes fail closed). Retired codes are
//! tombstoned forever and never reassigned.

use std::sync::LazyLock;

use serde::Deserialize;

use super::diagnostic;
use super::id::{ErrorCode, ErrorId, MessageTemplateId};
use super::normalize;
use super::types::{
    build_payload, build_union, valid_field_name, validate_contract_shape, Coverage, ErrorContract,
    ErrorField, Messages, OperationErrorContract, OperationKind, OutputType, RetryCondition,
    RetryPolicy, SourceSpan, TypeExpr, Waiver,
};
use super::version;
use crate::diagnostics::DiagnosticSet;

/// The exact embedded registry bytes.
pub const REGISTRY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/error-registry.v1.0.0.json");

/// One retired code: tombstoned forever, never reassigned or reused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tombstone {
    pub(crate) code: ErrorCode,
    pub(crate) reason: String,
}

impl Tombstone {
    /// The retired code.
    pub fn code(&self) -> &ErrorCode {
        &self.code
    }

    /// The bounded retirement reason.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// The validated error registry.
#[derive(Clone, Debug)]
pub struct ErrorRegistry {
    errors: Vec<ErrorContract>,
    bindings: Vec<OperationErrorContract>,
    tombstones: Vec<Tombstone>,
    canonical: String,
}

impl ErrorRegistry {
    /// Parses and validates registry bytes; rejects noncanonical bytes,
    /// duplicate keys, unknown fields, and every invariant violation.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DiagnosticSet> {
        if bytes.len() > version::MAX_EXPORT_BYTES {
            return Err(diagnostic::limit_exceeded_set("registry-bound", "33554432"));
        }
        let wire: WireRegistry = serde_json::from_slice(bytes)
            .map_err(|_| diagnostic::registry_invalid_set("wire-malformed"))?;
        if wire.schema_version != version::REGISTRY_SCHEMA_VERSION
            || wire.identity != version::REGISTRY_IDENTITY
            || wire.closed != Some(true)
        {
            return Err(diagnostic::registry_invalid_set("identity"));
        }
        if wire.errors.len() > version::MAX_ERRORS
            || wire.bindings.len() > version::MAX_BINDINGS
            || wire.tombstones.len() > version::MAX_TOMBSTONES
        {
            return Err(diagnostic::limit_exceeded_set("registry-bound", "10000"));
        }

        // Errors: every entry validated through the shared constructors.
        let mut errors = Vec::with_capacity(wire.errors.len());
        for entry in &wire.errors {
            errors.push(error_from_wire(entry)?);
        }

        // Tombstones: sorted, unique, never colliding with a live code.
        let mut tombstones = Vec::with_capacity(wire.tombstones.len());
        for entry in &wire.tombstones {
            let code = ErrorCode::new(&entry.code)
                .ok_or_else(|| diagnostic::registry_invalid_set("tombstone-code"))?;
            if entry.reason.is_empty() || entry.reason.len() > 500 {
                return Err(diagnostic::registry_invalid_set("tombstone-reason"));
            }
            tombstones.push(Tombstone {
                code,
                reason: entry.reason.clone(),
            });
        }

        // Bindings: every entry validated through the shared constructors.
        let mut bindings = Vec::with_capacity(wire.bindings.len());
        for entry in &wire.bindings {
            bindings.push(binding_from_wire(entry)?);
        }

        let registry = Self::from_parts(errors, bindings, tombstones)?;
        if registry.canonical.as_bytes() != bytes {
            return Err(diagnostic::registry_invalid_set("noncanonical-bytes"));
        }
        Ok(registry)
    }

    /// The embedded registry; parsed once per process.
    pub fn embedded() -> Result<&'static Self, DiagnosticSet> {
        static EMBEDDED: LazyLock<Option<ErrorRegistry>> =
            LazyLock::new(|| ErrorRegistry::from_bytes(REGISTRY_BYTES).ok());
        EMBEDDED
            .as_ref()
            .ok_or_else(|| diagnostic::registry_invalid_set("embedded"))
    }

    /// Assembles and validates one registry from typed parts (the
    /// in-memory path for consumers and tests). The same invariants as
    /// [`ErrorRegistry::from_bytes`] apply: canonical ordering, unique
    /// ids and codes, no tombstone collision, and every union member
    /// resolves.
    pub fn from_parts(
        mut errors: Vec<ErrorContract>,
        mut bindings: Vec<OperationErrorContract>,
        mut tombstones: Vec<Tombstone>,
    ) -> Result<Self, DiagnosticSet> {
        if errors.len() > version::MAX_ERRORS
            || bindings.len() > version::MAX_BINDINGS
            || tombstones.len() > version::MAX_TOMBSTONES
        {
            return Err(diagnostic::limit_exceeded_set("registry-bound", "10000"));
        }
        errors.sort_by(|left, right| left.id().cmp(right.id()));
        if errors
            .windows(2)
            .any(|window| window[0].id() == window[1].id())
        {
            return Err(diagnostic::registry_invalid_set("duplicate-id"));
        }
        let mut codes: Vec<&str> = errors.iter().map(|error| error.code().as_str()).collect();
        codes.sort_unstable();
        if codes.windows(2).any(|window| window[0] == window[1]) {
            return Err(diagnostic::code_reused_set(
                "duplicate-code",
                codes.first().copied().unwrap_or_default(),
            ));
        }
        tombstones.sort_by(|left, right| left.code.cmp(&right.code));
        if tombstones
            .windows(2)
            .any(|window| window[0].code == window[1].code)
        {
            return Err(diagnostic::code_reused_set(
                "duplicate-tombstone",
                tombstones
                    .first()
                    .map(|tombstone| tombstone.code.as_str())
                    .unwrap_or_default(),
            ));
        }
        for tombstone in &tombstones {
            if errors
                .iter()
                .any(|error| error.code().as_str() == tombstone.code.as_str())
            {
                return Err(diagnostic::code_reused_set(
                    "tombstone-collision",
                    tombstone.code.as_str(),
                ));
            }
        }
        bindings.sort_by(|left, right| left.operation().cmp(right.operation()));
        if bindings
            .windows(2)
            .any(|window| window[0].operation() == window[1].operation())
        {
            return Err(diagnostic::registry_invalid_set("duplicate-binding"));
        }
        for binding in &bindings {
            for member in binding.errors().members() {
                if !errors.iter().any(|error| error.id() == member.id()) {
                    return Err(diagnostic::binding_invalid_set(
                        "unresolved-member",
                        member.id().as_str(),
                    ));
                }
            }
        }
        let mut registry = Self {
            errors,
            bindings,
            tombstones,
            canonical: String::new(),
        };
        registry.canonical = normalize::registry_bytes(&registry)?;
        Ok(registry)
    }

    /// The declared errors in canonical id order.
    pub fn errors(&self) -> &[ErrorContract] {
        &self.errors
    }

    /// The operation bindings in canonical operation order.
    pub fn bindings(&self) -> &[OperationErrorContract] {
        &self.bindings
    }

    /// The retired codes.
    pub fn tombstones(&self) -> &[Tombstone] {
        &self.tombstones
    }

    /// The canonical bytes.
    pub fn canonical_bytes(&self) -> &str {
        &self.canonical
    }

    /// Looks up one declared error by id.
    pub fn error(&self, id: &ErrorId) -> Option<&ErrorContract> {
        self.errors
            .binary_search_by(|error| error.id().cmp(id))
            .ok()
            .map(|index| &self.errors[index])
    }

    /// Looks up one operation binding.
    pub fn binding(&self, operation: &ErrorId) -> Option<&OperationErrorContract> {
        self.bindings
            .binary_search_by(|binding| binding.operation().cmp(operation))
            .ok()
            .map(|index| &self.bindings[index])
    }

    /// Whether one code is retired or otherwise unavailable.
    pub fn code_taken(&self, code: &ErrorCode) -> bool {
        self.errors.iter().any(|error| error.code() == code)
            || self
                .tombstones
                .iter()
                .any(|tombstone| &tombstone.code == code)
    }
}

/// Build one validated contract from its wire form.
fn error_from_wire(wire: &WireError) -> Result<ErrorContract, DiagnosticSet> {
    let id = ErrorId::new(&wire.id)
        .ok_or_else(|| diagnostic::contract_invalid_set("error-id", &wire.id))?;
    let code = ErrorCode::new(&wire.code)
        .ok_or_else(|| diagnostic::contract_invalid_set("error-code", &wire.code))?;
    let category = super::types::ErrorCategory::parse(&wire.category)
        .ok_or_else(|| diagnostic::contract_invalid_set("category", &wire.id))?;

    let mut fields = Vec::with_capacity(wire.payload.fields.len());
    for field in &wire.payload.fields {
        fields.push(ErrorField {
            name: field.name.clone(),
            field_type: type_from_wire(&field.field_type)
                .ok_or_else(|| diagnostic::payload_invalid_set("field-type", &field.name))?,
            required: field.required,
            exposure: exposure_from_wire(&field.exposure)
                .ok_or_else(|| diagnostic::payload_invalid_set("field-exposure", &field.name))?,
        });
    }
    let payload = build_payload(fields)?;

    let public_template = template_from_wire(&wire.messages.public)?;
    let private_template = wire
        .messages
        .private
        .as_ref()
        .map(template_from_wire)
        .transpose()?;
    let messages = Messages {
        public: public_template,
        private: private_template,
    };

    let retry = retry_from_wire(&wire.retry)?;
    let idempotency = super::types::Idempotency::parse(&wire.idempotency)
        .ok_or_else(|| diagnostic::contract_invalid_set("idempotency", &wire.id))?;
    let effect = super::types::EffectClass::parse(&wire.effect)
        .ok_or_else(|| diagnostic::contract_invalid_set("effect", &wire.id))?;
    let observability = super::types::Observability::parse(&wire.observability)
        .ok_or_else(|| diagnostic::contract_invalid_set("observability", &wire.id))?;

    let coverage = coverage_from_wire(&wire.coverage)?;
    let source = source_from_wire(&wire.source, &wire.id)?;
    validate_contract_shape(
        &payload,
        &messages,
        &coverage,
        retry,
        idempotency,
        effect,
        &source,
        &wire.invariant,
    )?;
    Ok(ErrorContract {
        id,
        code,
        category,
        payload,
        messages,
        retry,
        idempotency,
        effect,
        observability,
        coverage,
        source,
        invariant: wire.invariant.clone(),
    })
}

/// Build one validated binding from its wire form.
fn binding_from_wire(wire: &WireBinding) -> Result<OperationErrorContract, DiagnosticSet> {
    let operation = ErrorId::new(&wire.operation)
        .ok_or_else(|| diagnostic::binding_invalid_set("operation-id", &wire.operation))?;
    let kind = match wire.kind.as_str() {
        "command" => OperationKind::Command,
        "query" => OperationKind::Query,
        _ => return Err(diagnostic::binding_invalid_set("kind", &wire.operation)),
    };
    let output = match &wire.output {
        None => OutputType::Unit,
        Some(expr) => OutputType::Value(
            type_from_wire(expr)
                .ok_or_else(|| diagnostic::binding_invalid_set("output-type", &wire.operation))?,
        ),
    };
    let mut members = Vec::with_capacity(wire.errors.len());
    for id in &wire.errors {
        let id =
            ErrorId::new(id).ok_or_else(|| diagnostic::binding_invalid_set("member-id", id))?;
        members.push(super::types::ErrorRef(id));
    }
    let errors = build_union(members)?;
    Ok(OperationErrorContract {
        operation,
        kind,
        output,
        errors,
    })
}

fn type_from_wire(wire: &WireType) -> Option<TypeExpr> {
    if let Some(id) = &wire.reference {
        return Some(TypeExpr::Ref(ErrorId::new(id)?));
    }
    if let Some(inner) = &wire.list {
        return Some(TypeExpr::List(Box::new(type_from_wire(inner)?)));
    }
    if let Some(inner) = &wire.optional {
        return Some(TypeExpr::Optional(Box::new(type_from_wire(inner)?)));
    }
    None
}

fn exposure_from_wire(text: &str) -> Option<super::types::Exposure> {
    match text {
        "public" => Some(super::types::Exposure::Public),
        "private" => Some(super::types::Exposure::Private),
        _ => None,
    }
}

fn retry_from_wire(wire: &WireRetry) -> Result<RetryPolicy, DiagnosticSet> {
    match wire.policy.as_str() {
        "never" => Ok(RetryPolicy::Never),
        "safe" => Ok(RetryPolicy::Safe),
        "conditional" => {
            let condition = match wire.condition.as_deref() {
                Some("idempotency-key") => RetryCondition::IdempotencyKey,
                Some("reconciliation") => RetryCondition::Reconciliation,
                _ => {
                    return Err(diagnostic::retry_conflict_set(
                        "conditional-requires-condition",
                    ))
                }
            };
            Ok(RetryPolicy::Conditional(condition))
        }
        _ => Err(diagnostic::retry_conflict_set("policy")),
    }
}

fn coverage_from_wire(wire: &WireCoverage) -> Result<Coverage, DiagnosticSet> {
    let refs = |values: &[String]| -> Result<Vec<super::types::CoverageRef>, DiagnosticSet> {
        values
            .iter()
            .map(|value| {
                super::types::CoverageRef::new(value)
                    .ok_or_else(|| diagnostic::coverage_invalid_set("coverage-ref"))
            })
            .collect()
    };
    if let Some(scenarios) = &wire.scenarios {
        return Ok(Coverage::Scenarios(refs(scenarios)?));
    }
    if let Some(tests) = &wire.tests {
        return Ok(Coverage::Tests(refs(tests)?));
    }
    if let Some(waiver) = &wire.waiver {
        let reference = super::types::CoverageRef::new(&waiver.reference)
            .ok_or_else(|| diagnostic::coverage_invalid_set("waiver-ref"))?;
        return Ok(Coverage::Waiver(Waiver {
            reference,
            owner: waiver.owner.clone(),
            reason: waiver.reason.clone(),
            expires: waiver.expires.clone(),
        }));
    }
    Err(diagnostic::coverage_invalid_set("coverage-form"))
}

fn template_from_wire(wire: &WireTemplate) -> Result<super::types::MessageTemplate, DiagnosticSet> {
    let template = MessageTemplateId::new(&wire.template)
        .ok_or_else(|| diagnostic::contract_invalid_set("template-id", &wire.template))?;
    for field in &wire.fields {
        if !valid_field_name(field) {
            return Err(diagnostic::payload_invalid_set(
                "template-field-name",
                field,
            ));
        }
    }
    let mut fields = wire.fields.clone();
    fields.sort();
    Ok(super::types::MessageTemplate { template, fields })
}

fn source_from_wire(wire: &WireSource, subject: &str) -> Result<SourceSpan, DiagnosticSet> {
    let _ = subject;
    Ok(SourceSpan {
        path: wire.path.clone(),
        start: super::types::Position {
            byte: wire.start.byte,
            line: wire.start.line,
            column: wire.start.column,
        },
        end: super::types::Position {
            byte: wire.end.byte,
            line: wire.end.line,
            column: wire.end.column,
        },
    })
}

// Wire forms: private, closed under `deny_unknown_fields`, never exposed.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRegistry {
    schema_version: String,
    identity: String,
    closed: Option<bool>,
    errors: Vec<WireError>,
    bindings: Vec<WireBinding>,
    tombstones: Vec<WireTombstone>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireError {
    id: String,
    code: String,
    category: String,
    payload: WirePayload,
    messages: WireMessages,
    retry: WireRetry,
    idempotency: String,
    effect: String,
    observability: String,
    coverage: WireCoverage,
    source: WireSource,
    invariant: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePayload {
    fields: Vec<WireField>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireField {
    name: String,
    #[serde(rename = "type")]
    field_type: WireType,
    required: bool,
    exposure: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireType {
    #[serde(rename = "ref")]
    reference: Option<String>,
    list: Option<Box<WireType>>,
    optional: Option<Box<WireType>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireMessages {
    public: WireTemplate,
    private: Option<WireTemplate>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireTemplate {
    template: String,
    fields: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRetry {
    policy: String,
    condition: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCoverage {
    scenarios: Option<Vec<String>>,
    tests: Option<Vec<String>>,
    waiver: Option<WireWaiver>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireWaiver {
    #[serde(rename = "ref")]
    reference: String,
    owner: String,
    reason: String,
    expires: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSource {
    path: String,
    start: WirePosition,
    end: WirePosition,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePosition {
    byte: usize,
    line: usize,
    column: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireBinding {
    operation: String,
    kind: String,
    output: Option<WireType>,
    errors: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireTombstone {
    code: String,
    reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_registry_parses_and_round_trips() {
        let registry = ErrorRegistry::embedded().expect("embedded registry is valid");
        assert_eq!(registry.errors().len(), 5);
        assert_eq!(registry.bindings().len(), 3);
        assert_eq!(registry.tombstones().len(), 1);
        assert_eq!(registry.canonical_bytes().as_bytes(), REGISTRY_BYTES);
    }

    #[test]
    fn lookups_resolve_in_canonical_order() {
        let registry = ErrorRegistry::embedded().expect("embedded registry is valid");
        let id = ErrorId::new("planner.task_not_found").expect("id");
        let error = registry.error(&id).expect("declared");
        assert_eq!(error.code().as_str(), "LEK-ERR-005");
        assert_eq!(
            error.category(),
            super::super::types::ErrorCategory::NotFound
        );
        let operation = ErrorId::new("planner.focus_task").expect("id");
        let binding = registry.binding(&operation).expect("bound");
        assert_eq!(binding.errors().members().len(), 5);
    }

    #[test]
    fn tampered_bytes_fail_closed() {
        let text = std::str::from_utf8(REGISTRY_BYTES).expect("utf8");
        let bumped = text.replace("not-found", "gone-soon");
        assert!(ErrorRegistry::from_bytes(bumped.as_bytes()).is_err());
        let trailing = format!("{text}\n");
        assert!(ErrorRegistry::from_bytes(trailing.as_bytes()).is_err());
    }
}
