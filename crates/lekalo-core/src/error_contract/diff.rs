//! The pure error-contract revision comparison (issue #62).
//!
//! [`diff`] compares two validated registries and classifies every
//! semantic change as `breaking`, `non-breaking`, `policy-change`, or
//! `invalid`. The classification never infers compatibility from numeric
//! versions, HTTP statuses, exception names, or target languages, and the
//! comparison never mutates the old registry's bytes or tombstones.
//! Changes render in fixed unsigned-byte path order.

use super::diagnostic;
use super::registry::ErrorRegistry;
use super::types::{Idempotency, RetryCondition, RetryPolicy};
use crate::diagnostics::DiagnosticSet;

/// The closed compatibility class of one change.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum DiffClass {
    /// Exhaustive consumers break; safety tightens; identity moves.
    Breaking,
    /// The declared surface changes without breaking anyone.
    NonBreaking,
    /// A policy decision the owner must approve explicitly.
    PolicyChange,
    /// The revision reuses retired identity: an error, not a guess.
    Invalid,
}

impl DiffClass {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Breaking => "breaking",
            Self::NonBreaking => "non-breaking",
            Self::PolicyChange => "policy-change",
            Self::Invalid => "invalid",
        }
    }
}

/// The closed change taxonomy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ErrorChangeKind {
    /// One error contract was added.
    ErrorAdded,
    /// One error contract was removed.
    ErrorRemoved,
    /// One error's machine code changed.
    CodeChanged,
    /// A retired or live code was reused for a different error.
    CodeReused,
    /// A union member was added: breaking for exhaustive consumers.
    UnionMemberAdded,
    /// A union member was removed.
    UnionMemberRemoved,
    /// The category changed.
    CategoryChanged,
    /// A payload field was added.
    PayloadFieldAdded,
    /// A payload field was removed.
    PayloadFieldRemoved,
    /// A payload field's type changed.
    PayloadFieldTypeChanged,
    /// A payload field's exposure changed.
    PayloadFieldExposureChanged,
    /// A payload field's requiredness changed.
    PayloadFieldRequiredChanged,
    /// The public message template changed.
    PublicMessageChanged,
    /// The private message template changed.
    PrivateMessageChanged,
    /// The retry policy changed.
    RetryChanged,
    /// The idempotency guarantee changed.
    IdempotencyChanged,
    /// The declared effect class changed.
    EffectChanged,
    /// The observability severity changed.
    ObservabilityChanged,
    /// The coverage or waiver policy changed.
    CoverageChanged,
    /// The source requirement changed.
    SourceChanged,
    /// The declared invariant changed.
    InvariantChanged,
    /// One operation binding was added.
    BindingAdded,
    /// One operation binding was removed.
    BindingRemoved,
    /// One binding's output type changed.
    BindingOutputChanged,
    /// One tombstone was added.
    TombstoneAdded,
}

impl ErrorChangeKind {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ErrorAdded => "error-added",
            Self::ErrorRemoved => "error-removed",
            Self::CodeChanged => "code-changed",
            Self::CodeReused => "code-reused",
            Self::UnionMemberAdded => "union-member-added",
            Self::UnionMemberRemoved => "union-member-removed",
            Self::CategoryChanged => "category-changed",
            Self::PayloadFieldAdded => "payload-field-added",
            Self::PayloadFieldRemoved => "payload-field-removed",
            Self::PayloadFieldTypeChanged => "payload-field-type-changed",
            Self::PayloadFieldExposureChanged => "payload-field-exposure-changed",
            Self::PayloadFieldRequiredChanged => "payload-field-required-changed",
            Self::PublicMessageChanged => "public-message-changed",
            Self::PrivateMessageChanged => "private-message-changed",
            Self::RetryChanged => "retry-changed",
            Self::IdempotencyChanged => "idempotency-changed",
            Self::EffectChanged => "effect-changed",
            Self::ObservabilityChanged => "observability-changed",
            Self::CoverageChanged => "coverage-changed",
            Self::SourceChanged => "source-changed",
            Self::InvariantChanged => "invariant-changed",
            Self::BindingAdded => "binding-added",
            Self::BindingRemoved => "binding-removed",
            Self::BindingOutputChanged => "binding-output-changed",
            Self::TombstoneAdded => "tombstone-added",
        }
    }
}

/// One classified semantic change.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ErrorChange {
    pub(crate) path: String,
    pub(crate) kind: ErrorChangeKind,
    pub(crate) class: DiffClass,
}

impl ErrorChange {
    /// The canonical dotted path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The change kind.
    pub const fn kind(&self) -> ErrorChangeKind {
        self.kind
    }

    /// The compatibility class.
    pub const fn class(&self) -> DiffClass {
        self.class
    }
}

/// The complete revision comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorDiff {
    changes: Vec<ErrorChange>,
}

impl ErrorDiff {
    /// The changes in canonical path order.
    pub fn changes(&self) -> &[ErrorChange] {
        &self.changes
    }

    /// Whether the registries are semantically identical.
    pub fn equal(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Compare two validated registries.
pub fn diff(old: &ErrorRegistry, new: &ErrorRegistry) -> Result<ErrorDiff, DiagnosticSet> {
    let mut changes: Vec<ErrorChange> = Vec::new();

    // Error contracts.
    for old_error in old.errors() {
        let id = old_error.id();
        match new.error(id) {
            None => changes.push(ErrorChange {
                path: format!("error/{}", id.as_str()),
                kind: ErrorChangeKind::ErrorRemoved,
                class: DiffClass::Breaking,
            }),
            Some(new_error) => {
                diff_error(old_error, new_error, &mut changes);
            }
        }
    }
    for new_error in new.errors() {
        if old.error(new_error.id()).is_none() {
            changes.push(ErrorChange {
                path: format!("error/{}", new_error.id().as_str()),
                kind: ErrorChangeKind::ErrorAdded,
                class: DiffClass::NonBreaking,
            });
            // A new entry carrying an old code (or any taken code) is a
            // reuse: an error, never a guessed compatibility.
            if old.code_taken(new_error.code())
                || new
                    .errors()
                    .iter()
                    .any(|other| other.id() != new_error.id() && other.code() == new_error.code())
            {
                changes.push(ErrorChange {
                    path: format!("error/{}/code", new_error.id().as_str()),
                    kind: ErrorChangeKind::CodeReused,
                    class: DiffClass::Invalid,
                });
            }
        }
    }

    // Operation bindings.
    for old_binding in old.bindings() {
        match new.binding(old_binding.operation()) {
            None => changes.push(ErrorChange {
                path: format!("binding/{}", old_binding.operation().as_str()),
                kind: ErrorChangeKind::BindingRemoved,
                class: DiffClass::Breaking,
            }),
            Some(new_binding) => {
                if old_binding.output() != new_binding.output() {
                    changes.push(ErrorChange {
                        path: format!("binding/{}/output", old_binding.operation().as_str()),
                        kind: ErrorChangeKind::BindingOutputChanged,
                        class: DiffClass::Breaking,
                    });
                }
                diff_union(
                    old_binding.operation().as_str(),
                    old_binding
                        .errors()
                        .members()
                        .iter()
                        .map(|member| member.id().as_str()),
                    new_binding
                        .errors()
                        .members()
                        .iter()
                        .map(|member| member.id().as_str()),
                    &mut changes,
                );
            }
        }
    }
    for new_binding in new.bindings() {
        if old.binding(new_binding.operation()).is_none() {
            changes.push(ErrorChange {
                path: format!("binding/{}", new_binding.operation().as_str()),
                kind: ErrorChangeKind::BindingAdded,
                class: DiffClass::NonBreaking,
            });
        }
    }

    // Tombstones only grow.
    for old_tombstone in old.tombstones() {
        if !new
            .tombstones()
            .iter()
            .any(|tombstone| tombstone.code == old_tombstone.code)
        {
            return Err(diagnostic::diff_invalid_set("tombstone-vanished"));
        }
    }
    for new_tombstone in new.tombstones() {
        if !old
            .tombstones()
            .iter()
            .any(|tombstone| tombstone.code == new_tombstone.code)
        {
            changes.push(ErrorChange {
                path: format!("tombstone/{}", new_tombstone.code.as_str()),
                kind: ErrorChangeKind::TombstoneAdded,
                class: DiffClass::NonBreaking,
            });
        }
    }

    changes.sort_by(|left, right| {
        (left.path.as_bytes(), left.kind).cmp(&(right.path.as_bytes(), right.kind))
    });
    Ok(ErrorDiff { changes })
}

/// Diff one unchanged-id error contract field by field.
fn diff_error(
    old: &super::types::ErrorContract,
    new: &super::types::ErrorContract,
    changes: &mut Vec<ErrorChange>,
) {
    let base = format!("error/{}", old.id().as_str());
    if old.code() != new.code() {
        changes.push(ErrorChange {
            path: format!("{base}/code"),
            kind: ErrorChangeKind::CodeChanged,
            class: DiffClass::Breaking,
        });
    }
    if old.category() != new.category() {
        changes.push(ErrorChange {
            path: format!("{base}/category"),
            kind: ErrorChangeKind::CategoryChanged,
            class: DiffClass::Breaking,
        });
    }
    diff_payload(old, new, &base, changes);
    if old.messages().public().template() != new.messages().public().template()
        || old.messages().public().fields() != new.messages().public().fields()
    {
        changes.push(ErrorChange {
            path: format!("{base}/messages/public"),
            kind: ErrorChangeKind::PublicMessageChanged,
            class: DiffClass::Breaking,
        });
    }
    match (old.messages().private(), new.messages().private()) {
        (None, None) => {}
        (Some(old_private), Some(new_private)) => {
            if old_private.template() != new_private.template()
                || old_private.fields() != new_private.fields()
            {
                changes.push(ErrorChange {
                    path: format!("{base}/messages/private"),
                    kind: ErrorChangeKind::PrivateMessageChanged,
                    class: DiffClass::NonBreaking,
                });
            }
        }
        _ => changes.push(ErrorChange {
            path: format!("{base}/messages/private"),
            kind: ErrorChangeKind::PrivateMessageChanged,
            class: DiffClass::NonBreaking,
        }),
    }
    if old.retry() != new.retry() {
        changes.push(ErrorChange {
            path: format!("{base}/retry"),
            kind: ErrorChangeKind::RetryChanged,
            class: retry_class(old.retry(), new.retry()),
        });
    }
    if old.idempotency() != new.idempotency() {
        changes.push(ErrorChange {
            path: format!("{base}/idempotency"),
            kind: ErrorChangeKind::IdempotencyChanged,
            class: idempotency_class(old.idempotency(), new.idempotency()),
        });
    }
    if old.effect() != new.effect() {
        changes.push(ErrorChange {
            path: format!("{base}/effect"),
            kind: ErrorChangeKind::EffectChanged,
            class: if new.effect().at_least(old.effect()) {
                DiffClass::Breaking
            } else {
                DiffClass::PolicyChange
            },
        });
    }
    if old.observability() != new.observability() {
        changes.push(ErrorChange {
            path: format!("{base}/observability"),
            kind: ErrorChangeKind::ObservabilityChanged,
            class: DiffClass::NonBreaking,
        });
    }
    if old.coverage() != new.coverage() {
        changes.push(ErrorChange {
            path: format!("{base}/coverage"),
            kind: ErrorChangeKind::CoverageChanged,
            class: DiffClass::PolicyChange,
        });
    }
    if old.source() != new.source() {
        changes.push(ErrorChange {
            path: format!("{base}/source"),
            kind: ErrorChangeKind::SourceChanged,
            class: DiffClass::NonBreaking,
        });
    }
    if old.invariant() != new.invariant() {
        changes.push(ErrorChange {
            path: format!("{base}/invariant"),
            kind: ErrorChangeKind::InvariantChanged,
            class: DiffClass::PolicyChange,
        });
    }
}

fn diff_payload(
    old: &super::types::ErrorContract,
    new: &super::types::ErrorContract,
    base: &str,
    changes: &mut Vec<ErrorChange>,
) {
    for old_field in old.payload().fields() {
        match new
            .payload()
            .fields()
            .iter()
            .find(|field| field.name() == old_field.name())
        {
            None => changes.push(ErrorChange {
                path: format!("{base}/payload/{}", old_field.name()),
                kind: ErrorChangeKind::PayloadFieldRemoved,
                class: DiffClass::Breaking,
            }),
            Some(new_field) => {
                if old_field.field_type() != new_field.field_type() {
                    changes.push(ErrorChange {
                        path: format!("{base}/payload/{}/type", old_field.name()),
                        kind: ErrorChangeKind::PayloadFieldTypeChanged,
                        class: DiffClass::Breaking,
                    });
                }
                if old_field.exposure() != new_field.exposure() {
                    changes.push(ErrorChange {
                        path: format!("{base}/payload/{}/exposure", old_field.name()),
                        kind: ErrorChangeKind::PayloadFieldExposureChanged,
                        class: DiffClass::Breaking,
                    });
                }
                if old_field.required() != new_field.required() {
                    changes.push(ErrorChange {
                        path: format!("{base}/payload/{}/required", old_field.name()),
                        kind: ErrorChangeKind::PayloadFieldRequiredChanged,
                        class: DiffClass::Breaking,
                    });
                }
            }
        }
    }
    for new_field in new.payload().fields() {
        if old
            .payload()
            .fields()
            .iter()
            .all(|field| field.name() != new_field.name())
        {
            changes.push(ErrorChange {
                path: format!("{base}/payload/{}", new_field.name()),
                kind: ErrorChangeKind::PayloadFieldAdded,
                class: if new_field.required() {
                    DiffClass::Breaking
                } else {
                    DiffClass::PolicyChange
                },
            });
        }
    }
}

fn diff_union<'a>(
    operation: &str,
    old: impl Iterator<Item = &'a str>,
    new: impl Iterator<Item = &'a str>,
    changes: &mut Vec<ErrorChange>,
) {
    let old_members: Vec<&str> = old.collect();
    let new_members: Vec<&str> = new.collect();
    for member in &old_members {
        if !new_members.contains(member) {
            changes.push(ErrorChange {
                path: format!("binding/{operation}/errors/{member}"),
                kind: ErrorChangeKind::UnionMemberRemoved,
                class: DiffClass::Breaking,
            });
        }
    }
    for member in &new_members {
        if !old_members.contains(member) {
            changes.push(ErrorChange {
                path: format!("binding/{operation}/errors/{member}"),
                kind: ErrorChangeKind::UnionMemberAdded,
                class: DiffClass::Breaking,
            });
        }
    }
}

/// Tightening a retry (or its condition) is breaking; loosening is a
/// policy decision.
fn retry_class(old: RetryPolicy, new: RetryPolicy) -> DiffClass {
    let strength = |policy: RetryPolicy| match policy {
        RetryPolicy::Never => 0u8,
        RetryPolicy::Conditional(RetryCondition::Reconciliation) => 1,
        RetryPolicy::Conditional(RetryCondition::IdempotencyKey) => 2,
        RetryPolicy::Safe => 3,
    };
    if strength(new) < strength(old) {
        DiffClass::Breaking
    } else if old == new {
        DiffClass::NonBreaking
    } else {
        DiffClass::PolicyChange
    }
}

/// Weakening a guarantee is breaking; strengthening or moving to or from
/// `not-applicable` is a policy decision.
fn idempotency_class(old: Idempotency, new: Idempotency) -> DiffClass {
    let strength = |mode: Idempotency| match mode {
        Idempotency::NotApplicable => 0u8,
        Idempotency::NotGuaranteed => 1,
        Idempotency::KeyRequired => 2,
        Idempotency::Guaranteed => 3,
    };
    if strength(new) < strength(old) {
        DiffClass::Breaking
    } else {
        DiffClass::PolicyChange
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_render_in_the_closed_vocabulary() {
        assert_eq!(DiffClass::Breaking.as_str(), "breaking");
        assert_eq!(DiffClass::NonBreaking.as_str(), "non-breaking");
        assert_eq!(DiffClass::PolicyChange.as_str(), "policy-change");
        assert_eq!(DiffClass::Invalid.as_str(), "invalid");
    }

    #[test]
    fn retry_tightening_is_breaking_loosening_is_policy() {
        assert_eq!(
            retry_class(RetryPolicy::Safe, RetryPolicy::Never),
            DiffClass::Breaking
        );
        assert_eq!(
            retry_class(RetryPolicy::Never, RetryPolicy::Safe),
            DiffClass::PolicyChange
        );
        assert_eq!(
            retry_class(RetryPolicy::Safe, RetryPolicy::Safe),
            DiffClass::NonBreaking
        );
    }

    #[test]
    fn weakening_idempotency_is_breaking() {
        assert_eq!(
            idempotency_class(Idempotency::Guaranteed, Idempotency::NotGuaranteed),
            DiffClass::Breaking
        );
        assert_eq!(
            idempotency_class(Idempotency::NotGuaranteed, Idempotency::Guaranteed),
            DiffClass::PolicyChange
        );
    }
}
