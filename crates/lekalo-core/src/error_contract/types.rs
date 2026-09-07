//! The closed error-contract surface: categories, payloads, messages,
//! retry/idempotency/effect metadata, coverage, source requirements, and
//! the typed operation binding (issue #62).
//!
//! Every type is private-field and constructor-validated: there is no
//! `Deserialize` escape hatch, no `serde_json::Value`, no raw `Path`, and
//! no unchecked constructor. Unknown vocabulary fails closed. Payload
//! schemas are closed named-field records with explicit public/private
//! exposure; public message templates may reference only public payload
//! fields; retry, idempotency, and effect metadata must agree.

use super::diagnostic;
use super::id::{ErrorCode, ErrorId, MessageTemplateId};
use super::version::{MAX_COVERAGE_REFS, MAX_PAYLOAD_FIELDS, MAX_TEXT_BYTES, MAX_UNION_MEMBERS};
use crate::diagnostics::DiagnosticSet;

/// The closed semantic category of one error.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ErrorCategory {
    /// The request was malformed.
    Validation,
    /// The actor is not authorized.
    Auth,
    /// The request conflicts with current state.
    Conflict,
    /// The referenced subject does not exist.
    NotFound,
    /// A domain rule refused the request.
    Domain,
    /// A declared, named dependency is unavailable.
    Infrastructure,
}

impl ErrorCategory {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Auth => "auth",
            Self::Conflict => "conflict",
            Self::NotFound => "not-found",
            Self::Domain => "domain",
            Self::Infrastructure => "infrastructure",
        }
    }

    /// Parses the closed vocabulary; `None` fails closed.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "validation" => Some(Self::Validation),
            "auth" => Some(Self::Auth),
            "conflict" => Some(Self::Conflict),
            "not-found" => Some(Self::NotFound),
            "domain" => Some(Self::Domain),
            "infrastructure" => Some(Self::Infrastructure),
            _ => None,
        }
    }
}

/// Whether one payload field may cross the public boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Exposure {
    /// May appear in public projections.
    Public,
    /// Never appears in public projections.
    Private,
}

impl Exposure {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
        }
    }
}

/// The closed payload type grammar: the accepted Model type expression
/// over references (`ref`, `list`, `optional`; depth at most four).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeExpr {
    /// A named reference.
    Ref(ErrorId),
    /// A list of the inner type.
    List(Box<TypeExpr>),
    /// An optional of the inner type.
    Optional(Box<TypeExpr>),
}

impl TypeExpr {
    /// The maximum `list`/`optional` nesting of this expression.
    pub fn depth(&self) -> usize {
        match self {
            Self::Ref(_) => 1,
            Self::List(inner) | Self::Optional(inner) => inner.depth().saturating_add(1),
        }
    }

    /// The leaf reference of this expression.
    pub fn leaf(&self) -> &ErrorId {
        match self {
            Self::Ref(id) => id,
            Self::List(inner) | Self::Optional(inner) => inner.leaf(),
        }
    }
}

/// One closed payload field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorField {
    pub(crate) name: String,
    pub(crate) field_type: TypeExpr,
    pub(crate) required: bool,
    pub(crate) exposure: Exposure,
}

impl ErrorField {
    /// The field name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The field type expression.
    pub fn field_type(&self) -> &TypeExpr {
        &self.field_type
    }

    /// Whether values must be present.
    pub fn required(&self) -> bool {
        self.required
    }

    /// Whether the field may cross the public boundary.
    pub fn exposure(&self) -> Exposure {
        self.exposure
    }
}

/// The closed named-field payload schema, canonically sorted by field
/// name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorPayload {
    pub(crate) fields: Vec<ErrorField>,
}

impl ErrorPayload {
    /// The declared fields in canonical order.
    pub fn fields(&self) -> &[ErrorField] {
        &self.fields
    }

    /// The public fields in canonical order.
    pub fn public_fields(&self) -> impl Iterator<Item = &ErrorField> {
        self.fields
            .iter()
            .filter(|field| field.exposure == Exposure::Public)
    }
}

/// One message template reference plus the payload fields it may render.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageTemplate {
    pub(crate) template: MessageTemplateId,
    pub(crate) fields: Vec<String>,
}

impl MessageTemplate {
    /// The stable template id.
    pub fn template(&self) -> &str {
        self.template.as_str()
    }

    /// The referenced payload fields in canonical order.
    pub fn fields(&self) -> &[String] {
        &self.fields
    }
}

/// The public/private message pair; catalog text lives outside the
/// canonical identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Messages {
    pub(crate) public: MessageTemplate,
    pub(crate) private: Option<MessageTemplate>,
}

impl Messages {
    /// The public template (never absent).
    pub fn public(&self) -> &MessageTemplate {
        &self.public
    }

    /// The private template, when declared.
    pub fn private(&self) -> Option<&MessageTemplate> {
        self.private.as_ref()
    }
}

/// The closed retry policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryPolicy {
    /// Retrying cannot help.
    Never,
    /// Retrying is always safe.
    Safe,
    /// Retrying is safe only under the declared condition.
    Conditional(RetryCondition),
}

impl RetryPolicy {
    /// The exact wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Safe => "safe",
            Self::Conditional(_) => "conditional",
        }
    }

    /// The declared condition, when conditional.
    pub const fn condition(self) -> Option<RetryCondition> {
        match self {
            Self::Conditional(condition) => Some(condition),
            _ => None,
        }
    }
}

/// The closed retry condition vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryCondition {
    /// A client-supplied idempotency key guards the retry.
    IdempotencyKey,
    /// A reconciliation step guards the retry.
    Reconciliation,
}

impl RetryCondition {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IdempotencyKey => "idempotency-key",
            Self::Reconciliation => "reconciliation",
        }
    }
}

/// The closed idempotency guarantee of the owning operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Idempotency {
    /// Replays converge to the same state.
    Guaranteed,
    /// Replays are safe with a client key.
    KeyRequired,
    /// Replays may repeat effects.
    NotGuaranteed,
    /// No effects: the question does not apply.
    NotApplicable,
}

impl Idempotency {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Guaranteed => "guaranteed",
            Self::KeyRequired => "key-required",
            Self::NotGuaranteed => "not-guaranteed",
            Self::NotApplicable => "not-applicable",
        }
    }

    /// Parses the closed vocabulary; `None` fails closed.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "guaranteed" => Some(Self::Guaranteed),
            "key-required" => Some(Self::KeyRequired),
            "not-guaranteed" => Some(Self::NotGuaranteed),
            "not-applicable" => Some(Self::NotApplicable),
            _ => None,
        }
    }
}

/// The narrow declared effect class needed for the error contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum EffectClass {
    /// No effect.
    None,
    /// Read-only.
    Read,
    /// Local write.
    Write,
    /// Destructive local effect.
    Destructive,
    /// External boundary.
    External,
}

impl EffectClass {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Read => "read",
            Self::Write => "write",
            Self::Destructive => "destructive",
            Self::External => "external",
        }
    }

    /// Parses the closed vocabulary; `None` fails closed.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Self::None),
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            "destructive" => Some(Self::Destructive),
            "external" => Some(Self::External),
            _ => None,
        }
    }

    /// Whether this class is at least as strong as `other`.
    pub const fn at_least(self, other: Self) -> bool {
        self as u8 >= other as u8
    }
}

/// The independent closed observability severity. It never computes a
/// process exit and never substitutes a diagnostic severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Observability {
    /// Informational.
    Info,
    /// Worth a warning.
    Warning,
    /// An error worth attention.
    Error,
    /// Critical attention.
    Critical,
}

impl Observability {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        }
    }

    /// Parses the closed vocabulary; `None` fails closed.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

/// One opaque, bounded coverage reference (scenario or test); it stays
/// opaque until the scenario owner publishes its registry.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct CoverageRef(String);

impl CoverageRef {
    /// Validates the bounded opaque grammar; `None` when malformed.
    pub fn new(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if text.is_empty()
            || bytes.len() > super::version::TOKEN_BYTES
            || !bytes[0].is_ascii_alphanumeric()
            || !bytes.iter().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-' | b'#')
            })
        {
            return None;
        }
        Some(Self(text.to_owned()))
    }

    /// The exact spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The explicit waiver replacing live coverage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Waiver {
    pub(crate) reference: CoverageRef,
    pub(crate) owner: String,
    pub(crate) reason: String,
    pub(crate) expires: Option<String>,
}

impl Waiver {
    /// The waiver reference.
    pub fn reference(&self) -> &str {
        self.reference.as_str()
    }

    /// The accountable owner.
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// The bounded reason.
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The optional expiry date (`YYYY-MM-DD`).
    pub fn expires(&self) -> Option<&str> {
        self.expires.as_deref()
    }
}

/// The closed coverage requirement: scenarios, tests, or an explicit
/// waiver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Coverage {
    /// Reachable through the declared scenarios.
    Scenarios(Vec<CoverageRef>),
    /// Covered by the declared source tests.
    Tests(Vec<CoverageRef>),
    /// Explicitly waived.
    Waiver(Waiver),
}

impl Coverage {
    /// The closed form tag.
    pub const fn form(&self) -> &'static str {
        match self {
            Self::Scenarios(_) => "scenarios",
            Self::Tests(_) => "tests",
            Self::Waiver(_) => "waiver",
        }
    }
}

/// One 0-based half-open byte position with 1-based line/column.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    pub(crate) byte: usize,
    pub(crate) line: usize,
    pub(crate) column: usize,
}

impl Position {
    /// Validates and builds one position; line and column are 1-based,
    /// so there is no zero fallback.
    pub fn new(byte: usize, line: usize, column: usize) -> Result<Self, DiagnosticSet> {
        if line == 0 || column == 0 {
            return Err(super::diagnostic::contract_invalid_set(
                "source-range",
                "position",
            ));
        }
        Ok(Self { byte, line, column })
    }

    /// The 0-based byte offset.
    pub const fn byte(&self) -> usize {
        self.byte
    }

    /// The 1-based line.
    pub const fn line(&self) -> usize {
        self.line
    }

    /// The 1-based column.
    pub const fn column(&self) -> usize {
        self.column
    }
}

/// The logical source requirement: a project-relative logical path plus a
/// half-open range. No native paths, no absolute/UNC/drive forms, no
/// fallback zeros.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub(crate) path: String,
    pub(crate) start: Position,
    pub(crate) end: Position,
}

impl SourceSpan {
    /// The logical project-relative path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The half-open range start.
    pub fn start(&self) -> Position {
        self.start
    }

    /// The half-open range end.
    pub fn end(&self) -> Position {
        self.end
    }
}

/// One declared error contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorContract {
    pub(crate) id: ErrorId,
    pub(crate) code: ErrorCode,
    pub(crate) category: ErrorCategory,
    pub(crate) payload: ErrorPayload,
    pub(crate) messages: Messages,
    pub(crate) retry: RetryPolicy,
    pub(crate) idempotency: Idempotency,
    pub(crate) effect: EffectClass,
    pub(crate) observability: Observability,
    pub(crate) coverage: Coverage,
    pub(crate) source: SourceSpan,
    pub(crate) invariant: String,
}

impl ErrorContract {
    /// The stable semantic id.
    pub fn id(&self) -> &ErrorId {
        &self.id
    }

    /// The immutable machine code.
    pub fn code(&self) -> &ErrorCode {
        &self.code
    }

    /// The closed category.
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    /// The closed payload schema.
    pub fn payload(&self) -> &ErrorPayload {
        &self.payload
    }

    /// The public/private messages.
    pub const fn messages(&self) -> &Messages {
        &self.messages
    }

    /// The retry policy.
    pub const fn retry(&self) -> RetryPolicy {
        self.retry
    }

    /// The idempotency guarantee.
    pub const fn idempotency(&self) -> Idempotency {
        self.idempotency
    }

    /// The declared effect class.
    pub const fn effect(&self) -> EffectClass {
        self.effect
    }

    /// The observability severity.
    pub const fn observability(&self) -> Observability {
        self.observability
    }

    /// The coverage requirement.
    pub const fn coverage(&self) -> &Coverage {
        &self.coverage
    }

    /// The source requirement.
    pub const fn source(&self) -> &SourceSpan {
        &self.source
    }

    /// The declared source invariant.
    pub fn invariant(&self) -> &str {
        &self.invariant
    }
}

/// One union member: a typed reference to a registry-declared error.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ErrorRef(pub(crate) ErrorId);

impl ErrorRef {
    /// The referenced error id.
    pub fn id(&self) -> &ErrorId {
        &self.0
    }
}

/// The closed, non-empty, canonically sorted operation error union
/// (`Result<Output, ErrorUnion>`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorUnion {
    members: Vec<ErrorRef>,
}

impl ErrorUnion {
    /// The members in canonical order.
    pub fn members(&self) -> &[ErrorRef] {
        &self.members
    }

    /// Whether the union declares `id`.
    pub fn contains(&self, id: &ErrorId) -> bool {
        self.members.iter().any(|member| member.id() == id)
    }
}

/// The closed operation kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationKind {
    /// A command: unit output.
    Command,
    /// A query: typed output.
    Query,
}

impl OperationKind {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Query => "query",
        }
    }
}

/// The operation result type: `Unit` for commands, the declared type
/// expression for queries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutputType {
    /// The command unit.
    Unit,
    /// The query result type.
    Value(TypeExpr),
}

/// The typed operation binding: `Result<Output, ErrorUnion>` for one
/// command or query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationErrorContract {
    pub(crate) operation: ErrorId,
    pub(crate) kind: OperationKind,
    pub(crate) output: OutputType,
    pub(crate) errors: ErrorUnion,
}

impl OperationErrorContract {
    /// The bound operation id.
    pub fn operation(&self) -> &ErrorId {
        &self.operation
    }

    /// The bound operation kind.
    pub const fn kind(&self) -> OperationKind {
        self.kind
    }

    /// The declared output type.
    pub const fn output(&self) -> &OutputType {
        &self.output
    }

    /// The closed declared error union.
    pub const fn errors(&self) -> &ErrorUnion {
        &self.errors
    }
}

/// Validate a field name against the closed lowercase grammar.
pub(crate) fn valid_field_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !name.is_empty()
        && name.len() <= 63
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

/// Validate the bounded opaque owner/reason text of a waiver.
pub(crate) fn valid_waiver_text(text: &str, max: usize) -> bool {
    !text.is_empty() && text.len() <= max && !text.chars().any(char::is_control)
}

/// Validate the logical source path against the #3 project-relative
/// grammar: forward slashes, no absolute/drive/UNC forms, no traversal or
/// alias segments.
pub(crate) fn valid_logical_path(path: &str) -> bool {
    if path.is_empty()
        || path.len() > super::version::TOKEN_BYTES
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.contains("//")
    {
        return false;
    }
    path.split('/').all(|segment| {
        if segment.is_empty() || segment.len() > 63 {
            return false;
        }
        if segment == "." || segment == ".." {
            return false;
        }
        if segment.ends_with('.') {
            return false;
        }
        let lower = segment.to_ascii_lowercase();
        let base = lower.split('.').next().unwrap_or("");
        if matches!(
            base,
            "con"
                | "prn"
                | "aux"
                | "nul"
                | "com1"
                | "com2"
                | "com3"
                | "com4"
                | "com5"
                | "com6"
                | "com7"
                | "com8"
                | "com9"
                | "lpt1"
                | "lpt2"
                | "lpt3"
                | "lpt4"
                | "lpt5"
                | "lpt6"
                | "lpt7"
                | "lpt8"
                | "lpt9"
                | "conin$"
                | "conout$"
                | "clock$"
        ) {
            return false;
        }
        segment.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    })
}

/// Validate the `YYYY-MM-DD` expiry spelling.
pub(crate) fn valid_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..].iter().all(u8::is_ascii_digit)
}

/// Validate one payload field list and build the sorted closed schema.
pub(crate) fn build_payload(mut fields: Vec<ErrorField>) -> Result<ErrorPayload, DiagnosticSet> {
    if fields.len() > MAX_PAYLOAD_FIELDS {
        return Err(diagnostic::limit_exceeded_set("payload-field-bound", "64"));
    }
    fields.sort_by(|left, right| left.name.cmp(&right.name));
    if fields
        .windows(2)
        .any(|window| window[0].name == window[1].name)
    {
        let duplicate = fields
            .windows(2)
            .find(|window| window[0].name == window[1].name)
            .map(|window| window[0].name.clone())
            .unwrap_or_default();
        return Err(diagnostic::payload_invalid_set(
            "duplicate-field",
            &duplicate,
        ));
    }
    for field in &fields {
        if !valid_field_name(&field.name) {
            return Err(diagnostic::payload_invalid_set("field-name", &field.name));
        }
        if field.field_type.depth() > 4 {
            return Err(diagnostic::payload_invalid_set("type-depth", &field.name));
        }
    }
    Ok(ErrorPayload { fields })
}

/// Build the canonically sorted closed union.
pub(crate) fn build_union(mut members: Vec<ErrorRef>) -> Result<ErrorUnion, DiagnosticSet> {
    if members.is_empty() {
        return Err(diagnostic::contract_invalid_set("empty-union", ""));
    }
    if members.len() > MAX_UNION_MEMBERS {
        return Err(diagnostic::limit_exceeded_set("union-bound", "256"));
    }
    members.sort_by(|left, right| left.0.cmp(&right.0));
    members.dedup();
    Ok(ErrorUnion { members })
}

/// The shared constructor checks for one declared error contract: message
/// references, coverage shape, retry/effect/idempotency agreement, source
/// invariants, and bounded text. Field/type/grammar checks live with the
/// wire decoder; this is the semantic layer.
#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_contract_shape(
    payload: &ErrorPayload,
    messages: &Messages,
    coverage: &Coverage,
    retry: RetryPolicy,
    idempotency: Idempotency,
    effect: EffectClass,
    source: &SourceSpan,
    invariant: &str,
) -> Result<(), DiagnosticSet> {
    // Message templates may reference only payload fields; public
    // templates only public ones.
    let names: Vec<&str> = payload
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    let public_names: Vec<&str> = payload
        .public_fields()
        .map(|field| field.name.as_str())
        .collect();
    let check_template = |template: &MessageTemplate, public: bool| -> Result<(), DiagnosticSet> {
        if template.fields.len() > MAX_PAYLOAD_FIELDS {
            return Err(diagnostic::payload_invalid_set(
                "template-field-bound",
                template.template.as_str(),
            ));
        }
        if template
            .fields
            .windows(2)
            .any(|window| window[0] >= window[1])
        {
            return Err(diagnostic::payload_invalid_set(
                "template-fields-unsorted",
                template.template.as_str(),
            ));
        }
        for field in &template.fields {
            if !names.contains(&field.as_str()) {
                return Err(diagnostic::payload_invalid_set(
                    "template-unknown-field",
                    field,
                ));
            }
            if public && !public_names.contains(&field.as_str()) {
                return Err(diagnostic::payload_invalid_set(
                    "template-private-field",
                    field,
                ));
            }
        }
        Ok(())
    };
    check_template(&messages.public, true)?;
    if let Some(private) = &messages.private {
        check_template(private, false)?;
    }

    // Coverage: non-empty bounded refs or a fully populated waiver.
    match coverage {
        Coverage::Scenarios(refs) | Coverage::Tests(refs) => {
            if refs.is_empty() || refs.len() > MAX_COVERAGE_REFS {
                return Err(diagnostic::coverage_invalid_set("coverage-empty"));
            }
            if refs.windows(2).any(|window| window[0] >= window[1]) {
                return Err(diagnostic::coverage_invalid_set("coverage-unsorted"));
            }
        }
        Coverage::Waiver(waiver) => {
            if !valid_waiver_text(&waiver.owner, MAX_TEXT_BYTES)
                || !valid_waiver_text(&waiver.reason, 500)
            {
                return Err(diagnostic::coverage_invalid_set("waiver-text"));
            }
            if let Some(expires) = &waiver.expires {
                if !valid_date(expires) {
                    return Err(diagnostic::coverage_invalid_set("waiver-expiry"));
                }
            }
        }
    }

    // Retry, idempotency, and effect must agree.
    check_retry_consistency(retry, idempotency, effect)?;

    // Source invariants: logical path, half-open non-empty range, no
    // fallback positions.
    if !valid_logical_path(&source.path) {
        return Err(diagnostic::contract_invalid_set(
            "source-path",
            &source.path,
        ));
    }
    if source.start.byte >= source.end.byte
        || source.start.line == 0
        || source.start.column == 0
        || source.end.line == 0
        || source.end.column == 0
    {
        return Err(diagnostic::contract_invalid_set(
            "source-range",
            &source.path,
        ));
    }
    if invariant.is_empty() || invariant.len() > 500 || invariant.chars().any(char::is_control) {
        return Err(diagnostic::contract_invalid_set("invariant", ""));
    }
    Ok(())
}

/// The retry/idempotency/effect agreement table (ADR-0022):
///
/// - `none`/`read`: no effects, so idempotency cannot apply and only
///   `never`/`safe` retries exist (a conditional retry would demand a key
///   for a read);
/// - `write`: `safe` requires a guaranteed-idempotent write, `conditional`
///   requires a key or an already-guaranteed replay;
/// - `destructive`: never safe to retry blind, never "guaranteed"
///   idempotent;
/// - `external`: `conditional` retries may be key-guarded (deduplicated
///   calls) or reconciliation-guarded.
pub(crate) fn check_retry_consistency(
    retry: RetryPolicy,
    idempotency: Idempotency,
    effect: EffectClass,
) -> Result<(), DiagnosticSet> {
    let conflict = |detail: &'static str| diagnostic::retry_conflict_set(detail);
    match effect {
        EffectClass::None | EffectClass::Read => {
            if idempotency != Idempotency::NotApplicable {
                return Err(conflict("effect-requires-not-applicable"));
            }
            if matches!(retry, RetryPolicy::Conditional(_)) {
                return Err(conflict("read-cannot-be-conditional"));
            }
        }
        EffectClass::Write => match retry {
            RetryPolicy::Safe if idempotency != Idempotency::Guaranteed => {
                return Err(conflict("safe-retry-requires-guaranteed"));
            }
            RetryPolicy::Conditional(_)
                if !matches!(
                    idempotency,
                    Idempotency::KeyRequired | Idempotency::Guaranteed
                ) =>
            {
                return Err(conflict("conditional-retry-requires-key"));
            }
            _ => {}
        },
        EffectClass::Destructive => {
            if retry == RetryPolicy::Safe {
                return Err(conflict("destructive-cannot-be-safe"));
            }
            if idempotency == Idempotency::Guaranteed {
                return Err(conflict("destructive-cannot-be-guaranteed"));
            }
            if let RetryPolicy::Conditional(RetryCondition::IdempotencyKey) = retry {
                if idempotency != Idempotency::KeyRequired {
                    return Err(conflict("key-retry-requires-key-mode"));
                }
            }
        }
        EffectClass::External => {
            if retry == RetryPolicy::Safe && idempotency != Idempotency::Guaranteed {
                return Err(conflict("safe-retry-requires-guaranteed"));
            }
            if let RetryPolicy::Conditional(RetryCondition::IdempotencyKey) = retry {
                if idempotency != Idempotency::KeyRequired {
                    return Err(conflict("key-retry-requires-key-mode"));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_parse_the_closed_vocabulary() {
        for spelling in [
            "validation",
            "auth",
            "conflict",
            "not-found",
            "domain",
            "infrastructure",
        ] {
            assert!(ErrorCategory::parse(spelling).is_some());
        }
        assert!(ErrorCategory::parse("unknown").is_none());
        assert!(ErrorCategory::parse("").is_none());
        assert!(ErrorCategory::parse("validationx").is_none());
    }

    #[test]
    fn logical_paths_reject_absolute_and_alias_forms() {
        assert!(valid_logical_path("lekalo/modules/planner/commands.yaml"));
        assert!(!valid_logical_path("/absolute/path"));
        assert!(!valid_logical_path(r"back\slash"));
        assert!(!valid_logical_path("drive:C:/path"));
        assert!(!valid_logical_path("a/../b"));
        assert!(!valid_logical_path("a/./b"));
        assert!(!valid_logical_path("a/trailing./b"));
        assert!(!valid_logical_path("CON/device"));
        assert!(!valid_logical_path("//unc/share"));
        assert!(!valid_logical_path(""));
    }

    #[test]
    fn retry_table_rejects_the_contradictions() {
        // Reads: never/safe only, idempotency cannot apply.
        assert!(check_retry_consistency(
            RetryPolicy::Safe,
            Idempotency::NotApplicable,
            EffectClass::Read
        )
        .is_ok());
        assert!(check_retry_consistency(
            RetryPolicy::Safe,
            Idempotency::Guaranteed,
            EffectClass::Read
        )
        .is_err());
        assert!(check_retry_consistency(
            RetryPolicy::Conditional(RetryCondition::Reconciliation),
            Idempotency::NotApplicable,
            EffectClass::Read
        )
        .is_err());
        // Writes: safe requires guaranteed; conditional requires a key.
        assert!(check_retry_consistency(
            RetryPolicy::Safe,
            Idempotency::Guaranteed,
            EffectClass::Write
        )
        .is_ok());
        assert!(check_retry_consistency(
            RetryPolicy::Safe,
            Idempotency::NotGuaranteed,
            EffectClass::Write
        )
        .is_err());
        assert!(check_retry_consistency(
            RetryPolicy::Conditional(RetryCondition::IdempotencyKey),
            Idempotency::NotGuaranteed,
            EffectClass::Write
        )
        .is_err());
        assert!(check_retry_consistency(
            RetryPolicy::Conditional(RetryCondition::IdempotencyKey),
            Idempotency::KeyRequired,
            EffectClass::Write
        )
        .is_ok());
        // Destructive: never safe, never guaranteed.
        assert!(check_retry_consistency(
            RetryPolicy::Safe,
            Idempotency::Guaranteed,
            EffectClass::Destructive
        )
        .is_err());
        assert!(check_retry_consistency(
            RetryPolicy::Never,
            Idempotency::NotGuaranteed,
            EffectClass::Destructive
        )
        .is_ok());
        // External: key retries require the key mode.
        assert!(check_retry_consistency(
            RetryPolicy::Conditional(RetryCondition::IdempotencyKey),
            Idempotency::KeyRequired,
            EffectClass::External
        )
        .is_ok());
        assert!(check_retry_consistency(
            RetryPolicy::Conditional(RetryCondition::IdempotencyKey),
            Idempotency::NotGuaranteed,
            EffectClass::External
        )
        .is_err());
    }
}
