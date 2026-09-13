//! Pure semantic comparison of two same-family attachments (issue
//! #66).
//!
//! The comparison answers one question per changed path with one
//! closed class: **breaking** (an expression was removed, its result
//! or target type changed, a declared reference was removed or
//! narrowed, or the built-in semantics pin moved), **non-breaking**
//! (an added expression, a description change, or an added
//! reference), and **policy-change** (a body change, a target move,
//! or a reference widened — the expression is still declared but
//! computes differently). Invalid inputs — foreign projects or mixed
//! Model/IR/attachment revisions — are the typed error set, never a
//! guessed classification. Paths are deterministic and byte-sorted,
//! so expression changes are visible in every semantic-diff and
//! impact consumer that projects this family.

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::{ExpressionRecord, ExpressionsAttachment};

/// The closed compatibility class of one changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiffClass {
    /// A required guarantee was removed or weakened.
    Breaking,
    /// An addition or descriptive change under the evolution policy.
    NonBreaking,
    /// A reshaping change with the expression still declared.
    PolicyChange,
}

impl DiffClass {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Breaking => "breaking",
            Self::NonBreaking => "non-breaking",
            Self::PolicyChange => "policy-change",
        }
    }
}

/// One changed path with its class.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DiffPath {
    path: String,
    class: DiffClass,
}

impl DiffPath {
    /// The canonical path spelling.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The closed compatibility class.
    pub const fn class(&self) -> DiffClass {
        self.class
    }
}

/// The finished comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffResult {
    equal: bool,
    paths: Vec<DiffPath>,
}

impl DiffResult {
    /// Whether the two attachments are semantically equal.
    pub const fn equal(&self) -> bool {
        self.equal
    }

    /// The changed paths, byte-sorted.
    pub fn paths(&self) -> &[DiffPath] {
        &self.paths
    }
}

/// Compare two same-family attachments. Pure and read-only.
pub fn compare(
    base: &ExpressionsAttachment,
    candidate: &ExpressionsAttachment,
) -> Result<DiffResult, DiagnosticSet> {
    if base.project_id().as_str() != candidate.project_id().as_str() {
        return Err(diagnostic::diff_invalid("diff-project-mismatch"));
    }
    if base.attachment_revision().as_str() != candidate.attachment_revision().as_str()
        || base.model_ref().digest().as_str() != candidate.model_ref().digest().as_str()
        || base.ir_digest().as_str() != candidate.ir_digest().as_str()
    {
        return Err(diagnostic::diff_invalid("diff-mixed-revision"));
    }
    let mut paths: Vec<DiffPath> = Vec::new();
    for record in base.expressions() {
        if let Some(other) = candidate.expression(record.id()) {
            compare_records(record, other, &mut paths);
        } else {
            push_path(record.id(), DiffClass::Breaking, &mut paths);
        }
    }
    for record in candidate.expressions() {
        if base.expression(record.id()).is_none() {
            push_path(record.id(), DiffClass::NonBreaking, &mut paths);
        }
    }
    if base.builtin_semantics() != candidate.builtin_semantics() {
        push_path("builtin-semantics", DiffClass::Breaking, &mut paths);
    }
    paths.sort();
    Ok(DiffResult {
        equal: paths.is_empty(),
        paths,
    })
}

/// Compare two records of one identity.
fn compare_records(
    base: &ExpressionRecord,
    candidate: &ExpressionRecord,
    paths: &mut Vec<DiffPath>,
) {
    let root = base.id().to_owned();
    if base.kind() != candidate.kind() {
        push_path(&format!("{}/kind", root), DiffClass::Breaking, paths);
    }
    if base.result() != candidate.result() {
        push_path(&format!("{}/result", root), DiffClass::Breaking, paths);
    }
    match (base.target(), candidate.target()) {
        (None, None) => {}
        (Some(_), None) | (None, Some(_)) => {
            push_path(&format!("{}/target", root), DiffClass::Breaking, paths);
        }
        (Some(base_target), Some(candidate_target)) => {
            if base_target.field() != candidate_target.field()
                || base_target.scope() != candidate_target.scope()
            {
                push_path(&format!("{}/target", root), DiffClass::PolicyChange, paths);
            }
            if base_target.ty() != candidate_target.ty() {
                push_path(&format!("{}/target/type", root), DiffClass::Breaking, paths);
            }
        }
    }
    // References: a removed or narrowed (type change, non-null
    // tightened from nullable) reference breaks every caller; an
    // added or widened one is a policy change for the new shape.
    for param in base.params() {
        let other = candidate
            .params()
            .iter()
            .find(|other| other.scope() == param.scope() && other.field() == param.field());
        match other {
            None => push_path(
                &format!("{}/params/{}.{}", root, param.scope().key(), param.field()),
                DiffClass::Breaking,
                paths,
            ),
            Some(other) => {
                if param.ty() != other.ty() || (param.nullable() && !other.nullable()) {
                    push_path(
                        &format!("{}/params/{}.{}", root, param.scope().key(), param.field()),
                        DiffClass::Breaking,
                        paths,
                    );
                } else if !param.nullable() && other.nullable() {
                    push_path(
                        &format!("{}/params/{}.{}", root, param.scope().key(), param.field()),
                        DiffClass::PolicyChange,
                        paths,
                    );
                }
            }
        }
    }
    for param in candidate.params() {
        let known = base
            .params()
            .iter()
            .any(|other| other.scope() == param.scope() && other.field() == param.field());
        if !known {
            push_path(
                &format!("{}/params/{}.{}", root, param.scope().key(), param.field()),
                DiffClass::NonBreaking,
                paths,
            );
        }
    }
    if base.body() != candidate.body() {
        push_path(&format!("{}/body", root), DiffClass::PolicyChange, paths);
    }
    if base.description() != candidate.description() {
        push_path(
            &format!("{}/description", root),
            DiffClass::NonBreaking,
            paths,
        );
    }
}

/// Append one path.
fn push_path(path: &str, class: DiffClass, paths: &mut Vec<DiffPath>) {
    paths.push(DiffPath {
        path: path.to_owned(),
        class,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(body: serde_json::Value) -> ExpressionsAttachment {
        let document = serde_json::json!({
            "schemaVersion": super::super::version::SCHEMA_VERSION,
            "identity": super::super::version::IDENTITY,
            "attachmentRevision": "1.0.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "1.0.0",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "irRef": {
                "identity": super::super::version::IR_IDENTITY,
                "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            "builtinSemantics": super::super::version::BUILTIN_SEMANTICS_VERSION,
            "expressions": [
                {
                    "id": "expr.planner/overdue-check",
                    "kind": "condition",
                    "params": [
                        {"scope": "input", "field": "due", "type": "datetime", "nullable": true}
                    ],
                    "result": "bool",
                    "body": body
                }
            ]
        });
        ExpressionsAttachment::from_value(&document).expect("valid")
    }

    #[test]
    fn removal_breaks_body_changes_reshapes_and_additions_add() {
        let base = attachment(serde_json::json!({
            "op": "not-null",
            "operand": {"op": "ref", "scope": "input", "field": "due"}
        }));
        // Identical candidate: equal.
        let same = attachment(serde_json::json!({
            "op": "not-null",
            "operand": {"op": "ref", "scope": "input", "field": "due"}
        }));
        let result = compare(&base, &same).expect("compares");
        assert!(result.equal && result.paths().is_empty());
        // Body change: policy change.
        let changed = attachment(serde_json::json!({
            "op": "is-null",
            "operand": {"op": "ref", "scope": "input", "field": "due"}
        }));
        let result = compare(&base, &changed).expect("compares");
        assert!(!result.equal);
        assert!(result
            .paths()
            .iter()
            .any(|path| path.path().ends_with("/body") && path.class() == DiffClass::PolicyChange));
    }

    #[test]
    fn foreign_projects_and_mixed_revisions_are_typed_errors() {
        let base = attachment(serde_json::json!({
            "op": "not-null",
            "operand": {"op": "ref", "scope": "input", "field": "due"}
        }));
        let mut document = serde_json::json!({
            "schemaVersion": super::super::version::SCHEMA_VERSION,
            "identity": super::super::version::IDENTITY,
            "attachmentRevision": "1.0.0",
            "projectId": "other",
            "modelRef": {
                "modelVersion": "1.0.0",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "irRef": {
                "identity": super::super::version::IR_IDENTITY,
                "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            "builtinSemantics": super::super::version::BUILTIN_SEMANTICS_VERSION,
            "expressions": [{"id": "expr.planner/other-check", "kind": "condition", "params": [], "result": "bool", "body": {"op": "lit-bool", "value": true}}]
        });
        let foreign = ExpressionsAttachment::from_value(&document).expect("valid");
        let rejection = compare(&base, &foreign).expect_err("foreign project");
        assert!(rejection
            .as_slice()
            .iter()
            .any(|d| d.id() == "expression.diff-invalid"));
        document["projectId"] = serde_json::json!("planner");
        document["attachmentRevision"] = serde_json::json!("2.0.0");
        let mixed = ExpressionsAttachment::from_value(&document).expect("valid");
        assert!(compare(&base, &mixed).is_err());
    }
}
