//! The #84 seam consumes resolved profile state; it never guesses future wires.
use super::wire::{Fact, ProfileRef, Selector, SelectorKind, State};
use crate::ai_lint::{input, wire::Config};
use crate::diagnostics::{registry::DiagnosticRegistry, types::Severity};
use crate::validator::ValidationProfile;

/// Applicable provenance dimensions must be known in both the decision and fact.
/// Inapplicability is admitted by policy, never inferred from a pin's value state.
#[derive(Clone, Copy, Debug)]
pub struct FingerprintRequirements {
    pub model: bool,
    pub ir: bool,
    pub adapter: bool,
    pub revision: bool,
    pub capabilities: bool,
}
impl FingerprintRequirements {
    pub fn model_only() -> Self {
        Self {
            model: true,
            ir: true,
            adapter: false,
            revision: true,
            capabilities: false,
        }
    }
    pub fn native() -> Self {
        Self {
            model: true,
            ir: true,
            adapter: true,
            revision: true,
            capabilities: true,
        }
    }
}

/// An admitted rule's effective policy, separate from its diagnostic fact.
#[derive(Clone, Debug)]
pub struct RuleState {
    pub enabled: bool,
    pub severity: Severity,
    pub required_evidence: bool,
    pub blocking: bool,
    pub waivable: bool,
}

fn admitted_producer_domain(fact: &Fact) -> bool {
    if fact.target != "model" {
        return true;
    }
    let (Some(model), Some(ir)) = (fact.fingerprint.model.known(), fact.fingerprint.ir.known())
    else {
        return false;
    };
    fact.id == crate::ai_lint::model_finding_id(&fact.subject, &fact.symbol)
        && fact.fingerprint.revision.known() == Some(&input::hash(&(model, ir)))
        && fact.fingerprint.adapter == State::Unsupported
        && fact.fingerprint.capabilities == State::Unsupported
        && fact.path == State::Unknown
}

/// Future architecture-profile owners implement this over their resolved state.
/// Unknown selectors return None and cannot grant acceptance.
pub trait ProfileState {
    fn reference(&self) -> ProfileRef;
    fn rule(&self, selector: &Selector, fact: &Fact) -> Option<RuleState>;
    /// Establish reserved producer claims from bound identity and provenance.
    /// A supplied target label alone cannot admit Model-only applicability.
    fn producer_domain_admitted(&self, fact: &Fact) -> bool {
        admitted_producer_domain(fact)
    }
    fn fingerprint_requirements(&self, fact: &Fact) -> FingerprintRequirements {
        if fact.target == "model" && self.producer_domain_admitted(fact) {
            FingerprintRequirements::model_only()
        } else {
            FingerprintRequirements::native()
        }
    }
}

/// Existing validation profiles preserve their error/non-downgrade rules.
pub struct ValidationState<'a> {
    pub profile: &'a ValidationProfile,
    pub digest: String,
}
impl ProfileState for ValidationState<'_> {
    fn reference(&self) -> ProfileRef {
        ProfileRef {
            id: self.profile.profile_id().into(),
            version: self.profile.version().into(),
            digest: self.digest.clone(),
        }
    }
    fn rule(&self, selector: &Selector, _fact: &Fact) -> Option<RuleState> {
        if selector.kind != SelectorKind::Rule {
            return None;
        }
        let registry = DiagnosticRegistry::embedded().ok()?;
        let entry = registry.entry(&selector.id)?;
        let selection = self.profile.selection(&selector.id)?;
        let severity = selection
            .severity_override()
            .unwrap_or(entry.default_severity());
        Some(RuleState {
            enabled: selection.enabled(),
            severity,
            required_evidence: severity == Severity::Error,
            blocking: severity == Severity::Error,
            waivable: severity != Severity::Error,
        })
    }
}

/// AI-lint config owns severity and required evidence, not the waiver file.
pub struct LintState<'a> {
    pub config: &'a Config,
    pub id: &'a str,
}
impl ProfileState for LintState<'_> {
    fn producer_domain_admitted(&self, fact: &Fact) -> bool {
        // Lint obligations retain the rule of their emitted occurrence. Other
        // profile families may derive their own selectors from a Model fact.
        admitted_producer_domain(fact)
            && (fact.target != "model" || fact.selector.id == crate::ai_lint::MODEL_DEPTH_RULE)
    }
    fn reference(&self) -> ProfileRef {
        // Pin the whole configuration, including coverage/regression policy.
        ProfileRef {
            id: self.id.into(),
            version: input::VERSION.into(),
            digest: input::hash(self.config),
        }
    }
    fn rule(&self, selector: &Selector, fact: &Fact) -> Option<RuleState> {
        let p = self.config.profiles.iter().find(|p| p.id == self.id)?;
        let rule = p.rules.iter().find(|r| r.id == selector.id)?;
        // A capability selector names the exact evidence obligation of this rule.
        let capability = selector.kind == SelectorKind::Capability;
        let required = p.gate.required_coverage.contains(&selector.id);
        let confidence = match fact.source_confidence.known()?.as_str() {
            "unknown" => crate::ai_lint::Confidence::Unknown,
            "low" => crate::ai_lint::Confidence::Low,
            "medium" => crate::ai_lint::Confidence::Medium,
            "high" => crate::ai_lint::Confidence::High,
            "exact" => crate::ai_lint::Confidence::Exact,
            _ => return None,
        };
        let severity = if fact.source_severity.known().is_some_and(|s| s == "info")
            || rule.severity.known().is_some_and(|s| s == "info")
        {
            Severity::Info
        } else {
            Severity::Warning
        };
        Some(RuleState {
            enabled: rule.enabled,
            severity,
            required_evidence: required,
            blocking: if capability {
                required
            } else {
                p.gate.fail_on_active_warnings
                    && severity == Severity::Warning
                    && confidence >= p.gate.minimum_confidence
            },
            waivable: !required,
        })
    }
}

/// Target profiles own capability requirements; a waiver never changes support.
pub struct TargetState<'a> {
    pub profile: &'a crate::target_profile::resolution::ResolvedProfile,
}
impl ProfileState for TargetState<'_> {
    fn reference(&self) -> ProfileRef {
        ProfileRef {
            id: self.profile.id.clone(),
            version: self.profile.version.clone(),
            digest: input::hash(self.profile),
        }
    }
    fn rule(&self, selector: &Selector, _fact: &Fact) -> Option<RuleState> {
        if selector.kind != SelectorKind::Capability {
            return None;
        }
        self.profile.capability(&selector.id)?;
        let required = self.profile.components.iter().any(|c| {
            crate::target_profile::component::definition(c.axis, &c.id)
                .is_some_and(|d| d.requires_capabilities.iter().any(|r| r.id == selector.id))
        });
        Some(RuleState {
            enabled: true,
            severity: if required {
                Severity::Error
            } else {
                Severity::Warning
            },
            required_evidence: required,
            blocking: required,
            waivable: !required,
        })
    }
}
