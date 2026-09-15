//! Issue #66: the closed, versioned typed-expression family.
//!
//! One independent, immutable contract family declaring the bounded
//! expression language of planner conditions and field assignments:
//! literals (including homogeneous sets), `input.*`/`actor.*`/
//! `entity.*`/`result.*` references, equality/comparison/null/set
//! membership, boolean combinators, checked integer and temporal
//! arithmetic, the branch-without-loop conditional, the deterministic
//! clock `now`, and fifteen target-neutral built-ins with versioned
//! semantics. Static typing is exhaustive: every operator
//! combination is decided at declaration time. The evaluator is the
//! reference implementation: pure, deterministic, clock-injected,
//! and with no representation for arbitrary calls, loops,
//! recursion, reflection, filesystem or network access, target code
//! snippets, or hidden mutable state — those shapes do not exist in
//! the grammar.
//!
//! Boundaries: this family is declaration plus reference evaluation
//! only. Model-bound field-type resolution stays with #107, runtime
//! enforcement stays with #24, authorization stays with #25, scenario
//! execution stays with #23, and code generation stays with the
//! target adapters; the renderer here produces the shared
//! cross-target fixtures those adapters confirm. Expressions that
//! exceed the closed grammar are refused with a registered
//! complexity diagnostic that routes to the foreign implementation
//! family (#30) instead of growing this DSL.
//!
//! Determinism and denial: canonical bytes are compact UTF-8 JSON
//! with byte-sorted keys; set literals normalize to sorted
//! duplicate-free form; combinators keep their declared operand
//! order. Every bound and every semantic contradiction rejects with
//! an explicit registered diagnostic and no partial result.

pub mod ast;
pub mod builtin;
pub mod canonical;
pub mod diagnostic;
pub mod diff;
pub mod eval;
pub mod projection;
pub mod types;
pub(crate) mod typing;
pub(crate) mod version;
pub(crate) mod wire;

use crate::diagnostics::SourceLocation;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::SemanticId;
pub use ast::{ExprNode, RefScope};
pub use builtin::{capability, lookup, Builtin, EvalError, BUILTINS};
pub use canonical::canonical_bytes;
pub use diagnostic::io_failure;
pub use diff::{compare, DiffClass, DiffPath, DiffResult};
pub use eval::{evaluate, Bindings, Clock};
pub use projection::{projection_identity, render_program, Target};
pub use types::{ExprType, Scalar, ScalarType, Value};
pub use wire::{check_builtin_support, BuiltinSupport, VectorCase, VectorExpect, VectorsDocument};

/// The bound source Model contract: exact accepted version plus digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelPin {
    /// The exact accepted Model version.
    pub(crate) model_version: SemVer,
    /// The canonical Model digest.
    pub(crate) digest: Sha256Digest,
}

impl ModelPin {
    /// The pinned Model version.
    pub fn model_version(&self) -> &SemVer {
        &self.model_version
    }

    /// The pinned Model digest.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
}

/// The record kind: a boolean condition or a field assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ExpressionKind {
    /// A boolean condition (a precondition, filter, or guard).
    Condition,
    /// A field assignment computing the written value.
    Assignment,
}

impl ExpressionKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Condition => "condition",
            Self::Assignment => "assignment",
        }
    }

    /// Parse one wire key.
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "condition" => Some(Self::Condition),
            "assignment" => Some(Self::Assignment),
            _ => None,
        }
    }
}

/// One declared typed reference of an expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParamDecl {
    /// The referenced scope.
    pub(crate) scope: RefScope,
    /// The referenced field (the bare name).
    pub(crate) field: String,
    /// The declared type.
    pub(crate) ty: ExprType,
    /// Whether the reference may be null at evaluation time. A
    /// nullable reference may only appear as the operand of
    /// `is-null`/`not-null`.
    pub(crate) nullable: bool,
}

impl ParamDecl {
    /// The declared scope.
    pub fn scope(&self) -> RefScope {
        self.scope
    }

    /// The declared field.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// The declared type.
    pub fn ty(&self) -> &ExprType {
        &self.ty
    }

    /// Whether the reference is nullable.
    pub const fn nullable(&self) -> bool {
        self.nullable
    }
}

/// The assignment target: the written field and its declared type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentTarget {
    /// The written scope (`input` or `entity` only; `actor` and
    /// `result` are read-only contexts).
    pub(crate) scope: RefScope,
    /// The written field.
    pub(crate) field: String,
    /// The declared type of the written field.
    pub(crate) ty: ExprType,
}

impl AssignmentTarget {
    /// The written scope.
    pub fn scope(&self) -> RefScope {
        self.scope
    }

    /// The written field.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// The declared type.
    pub fn ty(&self) -> &ExprType {
        &self.ty
    }
}

/// One finished expression record: immutable and safe to share.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpressionRecord {
    /// The namespaced identity, resolvable by the #63
    /// `ExpressionRef` grammar (`expr.planner/overdue-check`).
    pub(crate) id: String,
    /// The record kind.
    pub(crate) kind: ExpressionKind,
    /// The optional bounded description.
    pub(crate) description: Option<String>,
    /// The declared typed references.
    pub(crate) params: Vec<ParamDecl>,
    /// The declared result type.
    pub(crate) result: ExprType,
    /// The assignment target (assignments only).
    pub(crate) target: Option<AssignmentTarget>,
    /// The closed body.
    pub(crate) body: ExprNode,
    /// The optional declared source span.
    pub(crate) span: Option<SourceLocation>,
}

impl ExpressionRecord {
    /// The expression identity.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The record kind.
    pub const fn kind(&self) -> ExpressionKind {
        self.kind
    }

    /// The optional description.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// The declared typed references.
    pub fn params(&self) -> &[ParamDecl] {
        &self.params
    }

    /// The declared result type.
    pub fn result(&self) -> &ExprType {
        &self.result
    }

    /// The assignment target, when this record is an assignment.
    pub fn target(&self) -> Option<&AssignmentTarget> {
        self.target.as_ref()
    }

    /// The closed body.
    pub fn body(&self) -> &ExprNode {
        &self.body
    }

    /// The declared source span.
    pub fn span(&self) -> Option<&SourceLocation> {
        self.span.as_ref()
    }

    /// The built-ins this body uses, byte-sorted and duplicate-free.
    pub fn used_builtins(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        collect_builtins(&self.body, &mut names);
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Whether the body reads the deterministic clock.
    pub fn uses_now(&self) -> bool {
        let mut found = false;
        collect_now(&self.body, &mut found);
        found
    }
}

fn collect_builtins(node: &ExprNode, names: &mut Vec<&'static str>) {
    match node {
        ExprNode::Equal { left, right }
        | ExprNode::NotEqual { left, right }
        | ExprNode::Less { left, right }
        | ExprNode::LessEqual { left, right }
        | ExprNode::Greater { left, right }
        | ExprNode::GreaterEqual { left, right }
        | ExprNode::Add { left, right }
        | ExprNode::Subtract { left, right }
        | ExprNode::Multiply { left, right }
        | ExprNode::Divide { left, right }
        | ExprNode::Modulo { left, right } => {
            collect_builtins(left, names);
            collect_builtins(right, names);
        }
        ExprNode::IsNull { operand }
        | ExprNode::NotNull { operand }
        | ExprNode::Not { operand } => collect_builtins(operand, names),
        ExprNode::InSet { operand, set } | ExprNode::NotInSet { operand, set } => {
            collect_builtins(operand, names);
            collect_builtins(set, names);
        }
        ExprNode::And(operands) | ExprNode::Or(operands) => {
            for operand in operands {
                collect_builtins(operand, names);
            }
        }
        ExprNode::If {
            condition,
            then,
            otherwise,
        } => {
            collect_builtins(condition, names);
            collect_builtins(then, names);
            collect_builtins(otherwise, names);
        }
        ExprNode::Builtin { name, args } => {
            if let Some(entry) = builtin::lookup(name) {
                names.push(entry.name);
            }
            for arg in args {
                collect_builtins(arg, names);
            }
        }
        _ => {}
    }
}

fn collect_now(node: &ExprNode, found: &mut bool) {
    if *found {
        return;
    }
    match node {
        ExprNode::Now => *found = true,
        ExprNode::Equal { left, right }
        | ExprNode::NotEqual { left, right }
        | ExprNode::Less { left, right }
        | ExprNode::LessEqual { left, right }
        | ExprNode::Greater { left, right }
        | ExprNode::GreaterEqual { left, right }
        | ExprNode::Add { left, right }
        | ExprNode::Subtract { left, right }
        | ExprNode::Multiply { left, right }
        | ExprNode::Divide { left, right }
        | ExprNode::Modulo { left, right } => {
            collect_now(left, found);
            collect_now(right, found);
        }
        ExprNode::IsNull { operand }
        | ExprNode::NotNull { operand }
        | ExprNode::Not { operand } => collect_now(operand, found),
        ExprNode::InSet { operand, set } | ExprNode::NotInSet { operand, set } => {
            collect_now(operand, found);
            collect_now(set, found);
        }
        ExprNode::And(operands) | ExprNode::Or(operands) => {
            for operand in operands {
                collect_now(operand, found);
            }
        }
        ExprNode::If {
            condition,
            then,
            otherwise,
        } => {
            collect_now(condition, found);
            collect_now(then, found);
            collect_now(otherwise, found);
        }
        ExprNode::Builtin { args, .. } => {
            for arg in args {
                collect_now(arg, found);
            }
        }
        _ => {}
    }
}

/// One finished expressions attachment: immutable, deterministically
/// ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpressionsAttachment {
    /// The attachment's own revision.
    pub(crate) attachment_revision: SemVer,
    /// The owning project root identity.
    pub(crate) project_id: SemanticId,
    /// The bound source Model contract.
    pub(crate) model_ref: ModelPin,
    /// The exact bound IR identity.
    pub(crate) ir_identity: String,
    /// The canonical IR digest.
    pub(crate) ir_digest: Sha256Digest,
    /// The pinned built-in semantics version.
    pub(crate) builtin_semantics: String,
    /// The expression records, sorted by identity.
    pub(crate) expressions: Vec<ExpressionRecord>,
}

impl ExpressionsAttachment {
    /// Assemble one validated attachment; the constructor is
    /// crate-private so every instance passes the wire checks.
    pub(crate) fn assemble(
        attachment_revision: SemVer,
        project_id: SemanticId,
        model_ref: ModelPin,
        ir_identity: String,
        ir_digest: Sha256Digest,
        builtin_semantics: String,
        expressions: Vec<ExpressionRecord>,
    ) -> Self {
        Self {
            attachment_revision,
            project_id,
            model_ref,
            ir_identity,
            ir_digest,
            builtin_semantics,
            expressions,
        }
    }

    /// The attachment revision.
    pub fn attachment_revision(&self) -> &SemVer {
        &self.attachment_revision
    }

    /// The owning project root identity.
    pub fn project_id(&self) -> &SemanticId {
        &self.project_id
    }

    /// The bound source Model contract.
    pub fn model_ref(&self) -> &ModelPin {
        &self.model_ref
    }

    /// The bound IR identity.
    pub fn ir_identity(&self) -> &str {
        &self.ir_identity
    }

    /// The bound IR digest.
    pub fn ir_digest(&self) -> &Sha256Digest {
        &self.ir_digest
    }

    /// The pinned built-in semantics version.
    pub fn builtin_semantics(&self) -> &str {
        &self.builtin_semantics
    }

    /// The SHA-256 hex of the canonical bytes (no prefix); the
    /// stable identity of this attachment for diff and impact
    /// consumers.
    pub fn canonical_digest(&self) -> Result<String, crate::diagnostics::DiagnosticSet> {
        let bytes = canonical_bytes(self)?;
        Ok(crate::digest::sha256_hex(bytes.as_bytes()))
    }
    /// The expression records, sorted by identity.
    pub fn expressions(&self) -> &[ExpressionRecord] {
        &self.expressions
    }

    /// Look up one record by its exact identity.
    pub fn expression(&self, id: &str) -> Option<&ExpressionRecord> {
        self.expressions.iter().find(|record| record.id == id)
    }

    /// The required capability tokens of this attachment: the closed
    /// core grammar plus every used built-in, byte-sorted and
    /// duplicate-free. Managed-mode generation blocks a target whose
    /// declared capability snapshot lacks one of these.
    pub fn required_capabilities(&self) -> Vec<String> {
        let mut tokens: Vec<String> = vec![version::CORE_CAPABILITY.to_owned()];
        for record in &self.expressions {
            for name in record.used_builtins() {
                tokens.push(capability(name));
            }
        }
        tokens.sort();
        tokens.dedup();
        tokens
    }

    /// Normalize one wire document into a validated attachment, or
    /// return the typed rejection set with no partial attachment.
    /// Pure: no source, model, cache, report, network, process, or
    /// target access of any kind.
    pub fn from_value(json: &serde_json::Value) -> Result<Self, crate::diagnostics::DiagnosticSet> {
        wire::from_value(json)
    }
}

/// The fatal set for one vector referencing an expression the
/// attachment does not declare.
pub fn vector_expression_unknown(vector_id: &str) -> crate::diagnostics::DiagnosticSet {
    diagnostic::binding_invalid("expression-unknown", Some(vector_id))
}
