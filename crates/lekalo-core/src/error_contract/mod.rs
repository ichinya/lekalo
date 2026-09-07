//! Issue #62: the typed, independent error-contract family.
//!
//! Errors are a formal part of the behavior contract, not catch-all
//! exceptions or free text. This module owns the closed wire contracts
//! (`lekalo/error-contract/v1.0.0`, `lekalo/error-registry/v1.0.0`), the
//! typed private Rust surface (validated constructors, no `Deserialize`
//! escape hatch), the canonical registry, the pure revision comparison,
//! the language-neutral projection vectors, and the explicit
//! `Result<Output, ErrorUnion>` operation binding with a separate
//! unknown-infrastructure channel.
//!
//! Ownership boundaries: generic Model/IR semantics stay with #8/#12; the
//! error-specific rules here run only after that validation and never
//! re-read source files. Diagnostics are routed through the accepted #11
//! registry once; a diagnostic id is never an error id. Scenario and test
//! references stay opaque until #23 publishes its registry; HTTP status
//! defaults stay with the transport/profile owners (#27/#29). No adapter
//! executes and no transport runs here: everything is declaration,
//! validation, and pure data.

pub mod diagnostic;
pub mod diff;
pub mod id;
pub mod mapping;
pub mod normalize;
pub mod registry;
pub mod result;
pub mod types;
pub mod version;

use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::ir::{CompiledProject, TypeRef};
use crate::result::Status;

pub use diff::{diff, DiffClass, ErrorChange, ErrorChangeKind, ErrorDiff};
pub use id::{ErrorCode, ErrorId, MessageTemplateId};
pub use mapping::{
    go_error_vector, infrastructure_vector, node_error_vector, php_error_vector, MappingEntry,
    ProjectionForm, TargetMapping,
};
pub use registry::{ErrorRegistry, Tombstone, REGISTRY_BYTES};
pub use result::{
    FieldValue, InfrastructureFailure, InfrastructureKind, OperationOutcome, PublicPayload,
};
pub use types::{
    Coverage, CoverageRef, EffectClass, ErrorCategory, ErrorContract, ErrorField, ErrorPayload,
    ErrorRef, ErrorUnion, Exposure, Idempotency, MessageTemplate, Messages, Observability,
    OperationErrorContract, OperationKind, OutputType, Position, RetryCondition, RetryPolicy,
    SourceSpan, TypeExpr, Waiver,
};
pub use version::{
    FAMILY, IDENTITY, REGISTRY_FAMILY, REGISTRY_IDENTITY, REGISTRY_SCHEMA_VERSION,
    REGISTRY_VERSION, SCHEMA_VERSION,
};

/// One full-constructor failure type: every violation is a finalized
/// `invalid` diagnostic set.
pub type ContractError = DiagnosticSet;

impl ErrorField {
    /// Validates and builds one payload field.
    pub fn new(
        name: &str,
        field_type: TypeExpr,
        required: bool,
        exposure: Exposure,
    ) -> Result<Self, ContractError> {
        if !types::valid_field_name(name) {
            return Err(diagnostic::payload_invalid_set("field-name", name));
        }
        if field_type.depth() > 4 {
            return Err(diagnostic::payload_invalid_set("type-depth", name));
        }
        Ok(Self {
            name: name.to_owned(),
            field_type,
            required,
            exposure,
        })
    }
}

impl ErrorPayload {
    /// Validates and builds the closed sorted payload schema.
    pub fn new(fields: Vec<ErrorField>) -> Result<Self, ContractError> {
        types::build_payload(fields)
    }
}

impl MessageTemplate {
    /// Validates and builds one template reference; the fields are
    /// canonicalized (sorted, duplicates collapsed).
    pub fn new(template: &str, mut fields: Vec<String>) -> Result<Self, ContractError> {
        let template = MessageTemplateId::new(template)
            .ok_or_else(|| diagnostic::contract_invalid_set("template-id", template))?;
        fields.sort();
        fields.dedup();
        for field in &fields {
            if !types::valid_field_name(field) {
                return Err(diagnostic::payload_invalid_set(
                    "template-field-name",
                    field,
                ));
            }
        }
        Ok(Self { template, fields })
    }
}

impl Messages {
    /// Validates and builds the message pair.
    pub fn new(
        public: MessageTemplate,
        private: Option<MessageTemplate>,
    ) -> Result<Self, ContractError> {
        Ok(Self { public, private })
    }
}

impl Waiver {
    /// Validates and builds one explicit waiver.
    pub fn new(
        reference: &str,
        owner: &str,
        reason: &str,
        expires: Option<&str>,
    ) -> Result<Self, ContractError> {
        let reference = CoverageRef::new(reference)
            .ok_or_else(|| diagnostic::coverage_invalid_set("waiver-ref"))?;
        if !types::valid_waiver_text(owner, version::MAX_TEXT_BYTES)
            || !types::valid_waiver_text(reason, 500)
        {
            return Err(diagnostic::coverage_invalid_set("waiver-text"));
        }
        let expires = expires.map(str::to_owned);
        if let Some(expires) = &expires {
            if !types::valid_date(expires) {
                return Err(diagnostic::coverage_invalid_set("waiver-expiry"));
            }
        }
        Ok(Self {
            reference,
            owner: owner.to_owned(),
            reason: reason.to_owned(),
            expires,
        })
    }
}

impl SourceSpan {
    /// Validates and builds one logical source requirement.
    pub fn new(path: &str, start: Position, end: Position) -> Result<Self, ContractError> {
        if !types::valid_logical_path(path) {
            return Err(diagnostic::contract_invalid_set("source-path", path));
        }
        if start.byte >= end.byte
            || start.line == 0
            || start.column == 0
            || end.line == 0
            || end.column == 0
        {
            return Err(diagnostic::contract_invalid_set("source-range", path));
        }
        Ok(Self {
            path: path.to_owned(),
            start,
            end,
        })
    }
}

impl ErrorContract {
    /// The full validated constructor: every closed vocabulary, the
    /// payload/message exposure rules, coverage, retry/effect/idempotency
    /// agreement, and the source invariants are checked here. There is no
    /// unchecked path to an [`ErrorContract`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &str,
        code: &str,
        category: ErrorCategory,
        payload: ErrorPayload,
        messages: Messages,
        retry: RetryPolicy,
        idempotency: Idempotency,
        effect: EffectClass,
        observability: Observability,
        coverage: Coverage,
        source: SourceSpan,
        invariant: &str,
    ) -> Result<Self, ContractError> {
        let id =
            ErrorId::new(id).ok_or_else(|| diagnostic::contract_invalid_set("error-id", id))?;
        let code = ErrorCode::new(code)
            .ok_or_else(|| diagnostic::contract_invalid_set("error-code", code))?;
        types::validate_contract_shape(
            &payload,
            &messages,
            &coverage,
            retry,
            idempotency,
            effect,
            &source,
            invariant,
        )?;
        Ok(Self {
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
            invariant: invariant.to_owned(),
        })
    }
}

impl ErrorRef {
    /// Validates and builds one union member.
    pub fn new(id: &str) -> Result<Self, ContractError> {
        Ok(Self(ErrorId::new(id).ok_or_else(|| {
            diagnostic::contract_invalid_set("member-id", id)
        })?))
    }
}

impl ErrorUnion {
    /// Validates and builds the closed sorted union: non-empty, unique,
    /// canonically ordered. This is the `ErrorUnion` of
    /// `Result<Output, ErrorUnion>`; there is no catch-all member.
    pub fn new(members: Vec<ErrorRef>) -> Result<Self, ContractError> {
        types::build_union(members)
    }
}

impl OperationErrorContract {
    /// Validates and builds one operation binding.
    pub fn new(
        operation: &str,
        kind: OperationKind,
        output: OutputType,
        errors: ErrorUnion,
    ) -> Result<Self, ContractError> {
        let operation = ErrorId::new(operation)
            .ok_or_else(|| diagnostic::binding_invalid_set("operation-id", operation))?;
        Ok(Self {
            operation,
            kind,
            output,
            errors,
        })
    }
}

/// The error-specific validation over an accepted, compiled project
/// (issue #62 narrow slice). Generic Model/IR semantics are already the
/// #12 owner's result by the time this runs; these checks add exactly the
/// error-contract rules:
///
/// - every binding's operation exists and is a command/query of the
///   declared kind with the declared output type;
/// - every union member resolves to a declared error;
/// - every payload field's leaf type resolves to a declared scalar or
///   enum;
/// - every declared error is reachable: in some union, scenario-covered,
///   or explicitly waived.
///
/// An empty set means the error contracts agree with the project.
pub fn validate(registry: &ErrorRegistry, project: &CompiledProject) -> DiagnosticSet {
    let mut findings: Vec<Diagnostic> = Vec::new();

    for binding in registry.bindings() {
        let operation = project
            .definitions
            .iter()
            .find(|definition| definition.id().as_str() == binding.operation().as_str());
        let Some(definition) = operation else {
            findings.extend(diagnostic::finding(
                diagnostic::BINDING_INVALID,
                data_two("operation-unresolved", binding.operation().as_str()),
            ));
            continue;
        };
        match (binding.kind(), definition) {
            (OperationKind::Command, crate::ir::Definition::Command(_)) => {}
            (OperationKind::Query, crate::ir::Definition::Query(query)) => {
                match (binding.output(), &query.returns) {
                    (OutputType::Unit, None) => {}
                    (OutputType::Value(expr), Some(returns))
                        if type_expr_matches(expr, returns) => {}
                    _ => {
                        findings.extend(diagnostic::finding(
                            diagnostic::BINDING_INVALID,
                            data_two("output-mismatch", binding.operation().as_str()),
                        ));
                    }
                }
            }
            _ => {
                findings.extend(diagnostic::finding(
                    diagnostic::BINDING_INVALID,
                    data_two("kind-mismatch", binding.operation().as_str()),
                ));
            }
        }
        for member in binding.errors().members() {
            if registry.error(member.id()).is_none() {
                findings.extend(diagnostic::finding(
                    diagnostic::BINDING_INVALID,
                    data_two("unresolved-member", member.id().as_str()),
                ));
            }
        }
    }

    // Payload leaves must resolve to declared scalars or enums.
    for error in registry.errors() {
        for field in error.payload().fields() {
            let leaf = field.field_type().leaf();
            let resolved = project
                .definitions
                .iter()
                .find(|definition| definition.id().as_str() == leaf.as_str());
            match resolved {
                Some(crate::ir::Definition::Scalar(_) | crate::ir::Definition::Enum(_)) => {}
                _ => {
                    findings.extend(diagnostic::finding(
                        diagnostic::PAYLOAD_INVALID,
                        data_two("unresolved-type", leaf.as_str()),
                    ));
                }
            }
        }
    }

    // Reachability: union membership, scenario coverage, or waiver.
    for error in registry.errors() {
        let bound = registry.bindings().iter().any(|binding| {
            binding
                .errors()
                .members()
                .iter()
                .any(|member| member.id() == error.id())
        });
        if !bound
            && !matches!(
                error.coverage(),
                Coverage::Scenarios(_) | Coverage::Waiver(_)
            )
        {
            findings.extend(diagnostic::finding(
                diagnostic::UNREACHABLE,
                one_subject(error.id().as_str()),
            ));
        }
    }

    findings.sort_by(|left, right| {
        (left.id.as_str(), data_keys(left)).cmp(&(right.id.as_str(), data_keys(right)))
    });
    DiagnosticSet::try_from_unsorted(findings, Status::Invalid)
        .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
}

/// One `{detail, subject}` data record for the two-field rules.
fn data_two(detail: &str, subject: &str) -> crate::diagnostics::DataObject {
    let mut data = crate::diagnostics::DataObject::new();
    data.insert(
        "detail".to_owned(),
        crate::diagnostics::DataValue::Token(crate::diagnostics::types::bound_token(detail)),
    );
    data.insert(
        "subject".to_owned(),
        crate::diagnostics::DataValue::Token(crate::diagnostics::types::bound_token(subject)),
    );
    data
}

/// One `{subject}` data record for the single-field rules.
fn one_subject(subject: &str) -> crate::diagnostics::DataObject {
    let mut data = crate::diagnostics::DataObject::new();
    data.insert(
        "subject".to_owned(),
        crate::diagnostics::DataValue::Token(crate::diagnostics::types::bound_token(subject)),
    );
    data
}

/// The sorted data keys of one diagnostic, for deterministic finding
/// order.
fn data_keys(diagnostic: &Diagnostic) -> Vec<&str> {
    let mut keys: Vec<&str> = diagnostic.data.keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys
}

/// Whether the contract type expression matches the IR type reference
/// shape-for-shape.
fn type_expr_matches(expr: &TypeExpr, reference: &TypeRef) -> bool {
    match (expr, reference) {
        (TypeExpr::Ref(id), TypeRef::Ref(symbol)) => id.as_str() == symbol.as_str(),
        (TypeExpr::List(left), TypeRef::List(right)) => type_expr_matches(left, right),
        (TypeExpr::Optional(left), TypeRef::Optional(right)) => type_expr_matches(left, right),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_constants_are_independent() {
        assert_ne!(FAMILY, REGISTRY_FAMILY);
        assert_eq!(IDENTITY, "dev.lekalo.error-contract@1.0.0");
        assert_eq!(REGISTRY_IDENTITY, "dev.lekalo.error-registry@1.0.0");
    }
}
