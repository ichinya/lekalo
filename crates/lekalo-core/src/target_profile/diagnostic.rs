//! The target profile → diagnostic wire adapter (issue #29).
//!
//! Maps every closed [`ProfileFailure`] onto its registered
//! `target-profile.*` rule with typed bounded data. The status — and
//! therefore the exit class — lives here and nowhere else; list members
//! project one diagnostic each in sorted order, so a verdict never
//! depends on iteration order, and no raw document text enters a
//! diagnostic item.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataValue};
use crate::diagnostics::{DataObject, Scalar};
use crate::result::DomainResult;

use super::{support_token, ProfileFailure};

/// Build one wire diagnostic for a target-profile rule.
fn one(id: &str, data: DataObject) -> crate::diagnostics::Diagnostic {
    build(id, None, None, data).expect("target-profile rules are registered and active")
}

/// A sorted, bounded scalar list of stable tokens.
fn list(values: &[String]) -> DataValue {
    DataValue::List(
        values
            .iter()
            .map(|value| Scalar::Token(value.clone()))
            .collect(),
    )
}

/// Project one failure onto its registered diagnostics, in fixed order.
fn diagnostics(failure: &ProfileFailure) -> Vec<crate::diagnostics::Diagnostic> {
    let mut data = DataObject::new();
    match failure {
        ProfileFailure::DocumentInvalid { detail } => {
            data.insert("reason".to_owned(), token_value(detail));
            vec![one(failure.rule(), data)]
        }
        ProfileFailure::ComponentUnknown { component, axis } => {
            data.insert("component".to_owned(), token_value(component));
            data.insert("axis".to_owned(), token_value(axis));
            vec![one(failure.rule(), data)]
        }
        ProfileFailure::ReferenceInvalid { detail } => {
            data.insert("reason".to_owned(), token_value(detail));
            vec![one(failure.rule(), data)]
        }
        ProfileFailure::CombinationIncompatible { reasons } => {
            data.insert("reasons".to_owned(), list(reasons));
            vec![one(failure.rule(), data)]
        }
        ProfileFailure::CapabilityUnsatisfied { gaps } => gaps
            .iter()
            .map(|gap| {
                let mut gap_data = DataObject::new();
                gap_data.insert("capability".to_owned(), token_value(&gap.capability));
                gap_data.insert("required".to_owned(), token_value(gap.required.as_str()));
                gap_data.insert("actual".to_owned(), token_value(&support_token(gap.actual)));
                one(failure.rule(), gap_data)
            })
            .collect(),
        ProfileFailure::InheritanceWeakening { capabilities } => {
            data.insert("capabilities".to_owned(), list(capabilities));
            vec![one(failure.rule(), data)]
        }
    }
}

/// Wrap the projected diagnostics into the closed invalid envelope.
pub(crate) fn domain_result(failure: &ProfileFailure) -> DomainResult {
    let set = DomainResult::from_wire_set(ProfileFailure::status(), diagnostics(failure));
    DomainResult::Invalid { diagnostics: set }
}

#[cfg(test)]
mod tests {
    use super::super::component::Support;
    use super::super::CapabilityGap;
    use super::*;

    #[test]
    fn every_failure_projects_registered_diagnostics() {
        let failures: Vec<ProfileFailure> = vec![
            ProfileFailure::DocumentInvalid { detail: "shape" },
            ProfileFailure::ComponentUnknown {
                component: "ghost".to_owned(),
                axis: "runtime",
            },
            ProfileFailure::ReferenceInvalid {
                detail: "extends-cycle",
            },
            ProfileFailure::CombinationIncompatible {
                reasons: vec!["conflicts:a:b:c".to_owned()],
            },
            ProfileFailure::CapabilityUnsatisfied {
                gaps: vec![CapabilityGap {
                    capability: "runtime.async".to_owned(),
                    required: Support::Full,
                    actual: None,
                }],
            },
            ProfileFailure::InheritanceWeakening {
                capabilities: vec!["runtime.typing".to_owned()],
            },
        ];
        for failure in failures {
            let result = domain_result(&failure);
            assert!(matches!(result, DomainResult::Invalid { .. }));
        }
    }

    #[test]
    fn capability_gaps_project_one_diagnostic_each() {
        let failure = ProfileFailure::CapabilityUnsatisfied {
            gaps: vec![
                CapabilityGap {
                    capability: "storage.pooling".to_owned(),
                    required: Support::Full,
                    actual: Some(Support::Partial),
                },
                CapabilityGap {
                    capability: "transport.streaming".to_owned(),
                    required: Support::Full,
                    actual: None,
                },
            ],
        };
        let projected = diagnostics(&failure);
        assert_eq!(projected.len(), 2, "one diagnostic per sorted gap");
    }
}
