//! The typed context-budget request and its closed validation (issue #75).
//!
//! Exactly one of `--symbol`, `--module`, `--all` selects the scope;
//! an explicit budget or a named profile is required (no default
//! threshold exists). The request carries pre-validated profile handles
//! only — every filesystem read belongs to the CLI layer, and core
//! metric computation never opens paths or shells out.

use super::diagnostic;
use super::profile::Profile;
use crate::diagnostics::DiagnosticSet;

/// The closed analysis scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Scope {
    /// One resolved symbol (kind-qualified or unambiguous semantic id).
    Symbol(String),
    /// Every definition owned by one module.
    Module(String),
    /// Every definition of the project.
    All,
}

/// The typed, validated request of one report.
#[derive(Clone, Debug)]
pub struct BudgetRequest {
    pub scope: Scope,
    pub simulate: bool,
    pub suggest: bool,
    pub source_context: bool,
}

impl BudgetRequest {
    /// Validate the selector combination and flags into a typed request.
    pub fn new(
        symbol: Option<String>,
        module: Option<String>,
        all: bool,
        simulate: bool,
        suggest: bool,
        source_context: bool,
    ) -> Result<Self, DiagnosticSet> {
        let selected = [symbol.is_some(), module.is_some(), all]
            .iter()
            .filter(|selected| **selected)
            .count();
        if selected != 1 {
            return Err(diagnostic::input_invalid("selector-exactly-one"));
        }
        Ok(Self {
            scope: if let Some(symbol) = symbol {
                Scope::Symbol(symbol)
            } else if let Some(module) = module {
                Scope::Module(module)
            } else {
                Scope::All
            },
            simulate,
            suggest,
            source_context,
        })
    }
}

/// The effective budget handle: either the generic chars-4 profile with
/// an explicit token count, or one resolved named profile. The two are
/// exclusive in v1.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BudgetSelection {
    /// `--budget N`: the documented generic chars-4 profile.
    Generic(u64),
    /// `--budget-profile ID --profile-version V` resolved in the
    /// parsed profile document.
    Named(Box<Profile>),
}

impl BudgetSelection {
    /// The effective profile of this selection.
    pub fn profile(&self) -> Result<Profile, DiagnosticSet> {
        match self {
            Self::Generic(budget) => super::profile::generic_profile(*budget),
            Self::Named(profile) => Ok((**profile).clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_one_selector_is_required() {
        assert!(BudgetRequest::new(
            Some("planner.focus_task".to_owned()),
            None,
            false,
            false,
            false,
            false
        )
        .is_ok());
        assert!(
            BudgetRequest::new(None, Some("planner".to_owned()), false, false, false, false)
                .is_ok()
        );
        assert!(BudgetRequest::new(None, None, true, false, false, false).is_ok());
        assert!(BudgetRequest::new(None, None, false, false, false, false).is_err());
        assert!(BudgetRequest::new(
            Some("a".to_owned()),
            Some("b".to_owned()),
            false,
            false,
            false,
            false
        )
        .is_err());
    }

    #[test]
    fn generic_selection_resolves_the_profile() {
        let selection = BudgetSelection::Generic(5_000);
        let profile = selection.profile().unwrap();
        assert_eq!(profile.available_content_tokens, 5_000);
        assert!(BudgetSelection::Generic(0).profile().is_err());
    }
}
