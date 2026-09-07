//! The typed operation outcome: `Result<Output, ErrorUnion>` with a
//! separate unknown-infrastructure channel (issue #62).
//!
//! [`OperationOutcome`] is the runtime shape every operation projects:
//! `Ok`, a declared error with its validated public payload, or a typed
//! [`InfrastructureFailure`]. Unknown infrastructure (timeout, crash,
//! invalid process, unknown IO, provider failure) is never coerced into a
//! declared validation/auth/conflict/not-found/domain error, never uses a
//! declared code or payload, and renders only as a safe generic summary.
//! Public payloads carry only public schema fields with values their
//! declared type accepts.

use std::collections::BTreeMap;

use super::diagnostic;
use super::types::{ErrorContract, Exposure, TypeExpr};
use crate::diagnostics::DiagnosticSet;
use crate::ir::CompiledProject;

/// One bounded payload value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldValue {
    /// Text (string-like scalars, enum members).
    Text(String),
    /// An integer (number scalars).
    Count(i64),
    /// A boolean.
    Flag(bool),
    /// A bounded list of texts (list-of-text-like fields).
    TextList(Vec<String>),
}

/// The maximum length of one text value.
const TEXT_LIMIT: usize = super::version::MAX_TEXT_BYTES;

/// The maximum length of one list value.
const LIST_LIMIT: usize = super::version::MAX_PAYLOAD_VALUES;

impl FieldValue {
    /// Whether the value fits its declared type expression, resolved
    /// against the compiled project.
    pub fn fits(&self, expr: &TypeExpr, project: &CompiledProject) -> bool {
        match expr {
            TypeExpr::List(inner) => match self {
                Self::TextList(values) => {
                    values.len() <= LIST_LIMIT
                        && values
                            .iter()
                            .all(|value| Self::Text(value.clone()).fits(inner, project))
                }
                _ => false,
            },
            TypeExpr::Optional(inner) => self.fits(inner, project),
            TypeExpr::Ref(id) => self.fits_ref(id.as_str(), project),
        }
    }

    fn fits_ref(&self, id: &str, project: &CompiledProject) -> bool {
        let Some(definition) = project
            .definitions
            .iter()
            .find(|definition| definition.id().as_str() == id)
        else {
            return false;
        };
        match definition {
            crate::ir::Definition::Scalar(scalar) => match scalar.base {
                crate::ir::ScalarBase::Number => matches!(self, Self::Count(_)),
                crate::ir::ScalarBase::Boolean => matches!(self, Self::Flag(_)),
                _ => matches!(self, Self::Text(text) if text.len() <= TEXT_LIMIT),
            },
            crate::ir::Definition::Enum(enum_def) => match self {
                Self::Text(text) => enum_def
                    .values
                    .iter()
                    .any(|value| value.value.as_str() == text),
                _ => false,
            },
            _ => false,
        }
    }
}

/// The public payload: only public schema fields, values typed to their
/// declared schema, canonically ordered by field name.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PublicPayload {
    values: BTreeMap<String, FieldValue>,
}

impl PublicPayload {
    /// An empty payload (for errors with no public fields).
    pub fn empty() -> Self {
        Self::default()
    }

    /// The values in canonical order.
    pub fn values(&self) -> &BTreeMap<String, FieldValue> {
        &self.values
    }

    /// Insert one value; construction validates it against `contract`.
    pub fn validated(
        values: BTreeMap<String, FieldValue>,
        contract: &ErrorContract,
        project: &CompiledProject,
    ) -> Result<Self, DiagnosticSet> {
        if values.len() > super::version::MAX_PAYLOAD_VALUES {
            return Err(diagnostic::limit_exceeded_set("payload-value-bound", "64"));
        }
        let declared_names: Vec<&str> = contract
            .payload()
            .fields()
            .iter()
            .map(|field| field.name())
            .collect();
        for field in contract.payload().fields() {
            let declared = field.name();
            match values.get(declared) {
                Some(value) => {
                    if field.exposure() != Exposure::Public {
                        return Err(diagnostic::payload_invalid_set(
                            "private-field-in-public-payload",
                            declared,
                        ));
                    }
                    if !value.fits(field.field_type(), project) {
                        return Err(diagnostic::payload_invalid_set("value-type", declared));
                    }
                }
                None if field.required() && field.exposure() == Exposure::Public => {
                    return Err(diagnostic::payload_invalid_set(
                        "required-public-field-missing",
                        declared,
                    ));
                }
                None => {}
            }
        }
        if let Some(unknown) = values
            .keys()
            .find(|name| !declared_names.contains(&name.as_str()))
        {
            return Err(diagnostic::payload_invalid_set("unknown-field", unknown));
        }
        Ok(Self { values })
    }
}

/// The closed unknown-infrastructure failure taxonomy. This is the
/// separate typed channel: never a declared error, never a declared code
/// or payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InfrastructureFailure {
    kind: InfrastructureKind,
    detail: String,
}

impl InfrastructureFailure {
    /// Constructs one bounded failure; the detail never exceeds the token
    /// bound and carries no control characters.
    pub fn new(kind: InfrastructureKind, detail: &str) -> Self {
        let bounded: String = detail
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .take(super::version::TOKEN_BYTES)
            .collect();
        Self {
            kind,
            detail: bounded,
        }
    }

    /// The closed kind.
    pub const fn kind(&self) -> InfrastructureKind {
        self.kind
    }

    /// The private bounded detail; it never enters public projections.
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// The safe generic public summary: fixed text, no detail echo.
    pub const fn public_summary(&self) -> &'static str {
        "infrastructure failure"
    }
}

/// The closed infrastructure failure kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InfrastructureKind {
    /// A deadline passed.
    Timeout,
    /// The executor crashed.
    Crash,
    /// The process was invalid.
    InvalidProcess,
    /// Unknown input/output failure.
    Io,
    /// The provider failed.
    Provider,
}

impl InfrastructureKind {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Crash => "crash",
            Self::InvalidProcess => "invalid-process",
            Self::Io => "io",
            Self::Provider => "provider",
        }
    }
}

/// The typed operation outcome: the runtime `Result<Output, ErrorUnion>`
/// with the separate infrastructure channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationOutcome<T> {
    /// The success value.
    Ok(T),
    /// One declared error with its validated public payload.
    Declared {
        /// The declared error id.
        id: super::id::ErrorId,
        /// The immutable machine code.
        code: super::id::ErrorCode,
        /// The public payload values.
        public: PublicPayload,
    },
    /// Unknown infrastructure: a distinct typed channel.
    Infrastructure(InfrastructureFailure),
}

impl<T> OperationOutcome<T> {
    /// Builds one declared outcome: the payload is validated against the
    /// contract schema resolved in `project`; the identity comes from the
    /// contract itself.
    pub fn declared(
        contract: &ErrorContract,
        public: PublicPayload,
        project: &CompiledProject,
    ) -> Result<Self, DiagnosticSet> {
        let values = PublicPayload::validated(public.values().clone(), contract, project)?;
        Ok(Self::Declared {
            id: contract.id().clone(),
            code: contract.code().clone(),
            public: values,
        })
    }

    /// Whether this outcome is the infrastructure channel.
    pub const fn is_infrastructure(&self) -> bool {
        matches!(self, Self::Infrastructure(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infrastructure_stays_a_separate_channel() {
        let failure = InfrastructureFailure::new(InfrastructureKind::Timeout, "deadline\x07passed");
        assert_eq!(failure.kind(), InfrastructureKind::Timeout);
        assert_eq!(failure.detail(), "deadline passed");
        assert_eq!(failure.public_summary(), "infrastructure failure");
        let outcome: OperationOutcome<()> = OperationOutcome::Infrastructure(failure);
        assert!(outcome.is_infrastructure());
    }

    #[test]
    fn infrastructure_detail_is_bounded() {
        let long = "x".repeat(1000);
        let failure = InfrastructureFailure::new(InfrastructureKind::Io, &long);
        assert_eq!(
            failure.detail().len(),
            crate::error_contract::version::TOKEN_BYTES
        );
    }
}
