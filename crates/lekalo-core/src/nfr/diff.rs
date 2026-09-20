//! Pure semantic comparison of two same-family NFR attachments
//! (issue #85).
//!
//! The comparison answers one question per changed path with one
//! closed class: **breaking** (a declared guarantee was removed or
//! weakened — a constraint removed, a scope moved, a bound tightened
//! or loosened against its comparator, a kind or dimension changed,
//! or enforcement dropped from mandatory to advisory),
//! **non-breaking** (a constraint added), and **policy-change**
//! (measurement custody, validity, capability, source-requirement, or
//! advisory-to-mandatory changes with unchanged guarantees). Invalid
//! inputs — foreign projects or mixed attachment/Model/IR revisions —
//! are the typed error set, never a guessed classification. Paths are
//! deterministic and byte-sorted.

use crate::diagnostics::DiagnosticSet;

use super::constraint::{Constraint, Enforcement, RequirementValue};
use super::diagnostic;
use super::wire::NfrAttachment;

/// The closed compatibility class of one changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiffClass {
    /// A required guarantee was removed or weakened.
    Breaking,
    /// An addition under the evolution policy.
    NonBreaking,
    /// A custody, validity, or strengthening change with unchanged
    /// guarantees.
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
    base: &NfrAttachment,
    candidate: &NfrAttachment,
) -> Result<DiffResult, DiagnosticSet> {
    if base.project_id().as_str() != candidate.project_id().as_str() {
        return Err(diagnostic::diff_invalid("diff-project-mismatch"));
    }
    if base.attachment_revision() != candidate.attachment_revision()
        || base.model_ref().1.as_str() != candidate.model_ref().1.as_str()
        || base.ir_ref().1.as_str() != candidate.ir_ref().1.as_str()
    {
        return Err(diagnostic::diff_invalid("diff-mixed-revision"));
    }
    let mut paths: Vec<DiffPath> = Vec::new();

    // Constraint removal is breaking, addition is non-breaking, and
    // every member change of a common constraint is classified below.
    for constraint in base.constraints() {
        let Some(other) = candidate
            .constraints()
            .iter()
            .find(|other| other.constraint_id() == constraint.constraint_id())
        else {
            push_path(
                &format!("constraints/{}", constraint.constraint_id().as_str()),
                DiffClass::Breaking,
                &mut paths,
            );
            continue;
        };
        compare_constraint(constraint, other, &mut paths);
    }
    for constraint in candidate.constraints() {
        if !base
            .constraints()
            .iter()
            .any(|other| other.constraint_id() == constraint.constraint_id())
        {
            push_path(
                &format!("constraints/{}", constraint.constraint_id().as_str()),
                DiffClass::NonBreaking,
                &mut paths,
            );
        }
    }

    // Open questions: the register follows the attachment; a removed
    // question is a policy change, an added one non-breaking.
    for question in base.open_questions() {
        if !candidate
            .open_questions()
            .iter()
            .any(|other| other.question_id() == question.question_id())
        {
            push_path(
                &format!("openQuestions/{}", question.question_id().as_str()),
                DiffClass::PolicyChange,
                &mut paths,
            );
        }
    }
    for question in candidate.open_questions() {
        if !base
            .open_questions()
            .iter()
            .any(|other| other.question_id() == question.question_id())
        {
            push_path(
                &format!("openQuestions/{}", question.question_id().as_str()),
                DiffClass::NonBreaking,
                &mut paths,
            );
        }
    }

    let mut sorted = paths.clone();
    sorted.sort();
    sorted.dedup();
    Ok(DiffResult {
        equal: sorted.is_empty(),
        paths: sorted,
    })
}

/// Classify every changed member of one common constraint.
fn compare_constraint(base: &Constraint, candidate: &Constraint, paths: &mut Vec<DiffPath>) {
    let prefix = format!("constraints/{}", base.constraint_id().as_str());

    // The dimension or kind can never change: that is a different
    // guarantee under the same id.
    if base.dimension() != candidate.dimension() {
        push_path(&format!("{prefix}/dimension"), DiffClass::Breaking, paths);
    }
    if base.kind() != candidate.kind() {
        push_path(&format!("{prefix}/kind"), DiffClass::Breaking, paths);
    }
    if base.scope() != candidate.scope() {
        push_path(&format!("{prefix}/scope"), DiffClass::Breaking, paths);
    }

    // Enforcement: mandatory→advisory weakens the guarantee;
    // advisory→mandatory strengthens it (a policy change).
    match (base.enforcement(), candidate.enforcement()) {
        (Enforcement::Mandatory, Enforcement::Advisory) => {
            push_path(&format!("{prefix}/enforcement"), DiffClass::Breaking, paths);
        }
        (Enforcement::Advisory, Enforcement::Mandatory) => {
            push_path(
                &format!("{prefix}/enforcement"),
                DiffClass::PolicyChange,
                paths,
            );
        }
        _ => {}
    }

    // The declared bound: any change of the requirement data that
    // moves the guarantee is breaking.
    if base.requirement() != candidate.requirement() {
        compare_requirement(base, candidate, &prefix, paths);
    }

    // Measurement custody, environments, capabilities, validity, and
    // the source link carry no guarantee change of their own.
    if base.measurement() != candidate.measurement() {
        push_path(
            &format!("{prefix}/measurement"),
            DiffClass::PolicyChange,
            paths,
        );
    }
    if base.environments() != candidate.environments() {
        push_path(
            &format!("{prefix}/environments"),
            DiffClass::PolicyChange,
            paths,
        );
    }
    if base.capabilities() != candidate.capabilities() {
        push_path(
            &format!("{prefix}/capabilities"),
            DiffClass::PolicyChange,
            paths,
        );
    }
    if base.validity() != candidate.validity() {
        push_path(
            &format!("{prefix}/validity"),
            DiffClass::PolicyChange,
            paths,
        );
    }
    if base.source_requirement() != candidate.source_requirement() {
        push_path(
            &format!("{prefix}/sourceRequirement"),
            DiffClass::PolicyChange,
            paths,
        );
    }
}

/// Classify the requirement delta. Bound changes are breaking: a
/// changed comparator, unit, percentile, or numeric bound moves the
/// guarantee; a scalar becomes a range (or the reverse) likewise.
fn compare_requirement(
    base: &Constraint,
    candidate: &Constraint,
    prefix: &str,
    paths: &mut Vec<DiffPath>,
) {
    let left = base.requirement();
    let right = candidate.requirement();
    let changed_bound = match (left.value(), right.value()) {
        // Any bound movement is breaking: the core classifies every
        // changed value as breaking, never guessing intent.
        (Some(RequirementValue::Scalar(a)), Some(RequirementValue::Scalar(b))) => a != b,
        (
            Some(RequirementValue::Range { min: a, max: b }),
            Some(RequirementValue::Range { min: c, max: d }),
        ) => a != c || b != d,
        (Some(_), Some(_)) => true,
        (None, None) => false,
        _ => true,
    };
    if changed_bound
        || left.comparator() != right.comparator()
        || left.unit() != right.unit()
        || left.percentile() != right.percentile()
        || left.metric() != right.metric()
        || left.resource() != right.resource()
        || left.mode() != right.mode()
        || left.tokens() != right.tokens()
        || left.max_attempts() != right.max_attempts()
        || left.window() != right.window()
        || left.reference() != right.reference()
    {
        push_path(&format!("{prefix}/requirement"), DiffClass::Breaking, paths);
    }
}

fn push_path(path: &str, class: DiffClass, paths: &mut Vec<DiffPath>) {
    paths.push(DiffPath {
        path: path.to_owned(),
        class,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_carry_the_closed_keys() {
        assert_eq!(DiffClass::Breaking.key(), "breaking");
        assert_eq!(DiffClass::NonBreaking.key(), "non-breaking");
        assert_eq!(DiffClass::PolicyChange.key(), "policy-change");
    }
}
