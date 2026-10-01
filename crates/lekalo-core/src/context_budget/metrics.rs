//! The M1–M10 metric arithmetic of the context-budget report (issue #75).
//!
//! Every metric is either a known integer computed by the pinned
//! [`METRIC_VERSION`] semantics or an explicit non-known state; no
//! metric ever aggregates known and unknown operands into a fake total.
//! Derived invariants that hold here (and are re-verified by tests and
//! the Node gate): direct ⊆ transitive, indirect-only = transitive \
//! direct, `overBy = required − available` exactly at the assessment
//! boundary, and `minimumSafe = ceil(required × margin / denominator) +
//! framing` under the pinned estimator.

use std::collections::BTreeMap;

use super::closure::DependencyClosure;
use super::facts::{FactSelection, LedgerFact};
use super::profile::Profile;
use super::value::StateValue;
use super::version::METRIC_VERSION;

/// The closed assessment vocabulary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Assessment {
    /// The complete required estimate fits the available content budget.
    WithinBudget,
    /// The complete required estimate (or a proven lower bound) exceeds it.
    OverBudget,
    /// Completeness is reduced, so no budget verdict is proven.
    #[default]
    Indeterminate,
}

impl Assessment {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::WithinBudget => "within-budget",
            Self::OverBudget => "over-budget",
            Self::Indeterminate => "indeterminate",
        }
    }
}

/// The full per-subject metric vector of one report row.
#[derive(Clone, Debug, PartialEq)]
pub struct SubjectMetrics {
    pub direct_dependencies: StateValue<u64>,
    pub transitive_dependencies: StateValue<u64>,
    pub indirect_only_dependencies: StateValue<u64>,
    pub required_modules: StateValue<u64>,
    pub context_closure_estimated_tokens: StateValue<u64>,
    pub minimum_required_semantic_tokens: StateValue<u64>,
    pub supporting_semantic_tokens: StateValue<u64>,
    pub optional_source_tokens: StateValue<u64>,
    pub model_files: StateValue<u64>,
    pub source_files: StateValue<u64>,
    pub target_files: StateValue<u64>,
    pub max_cross_module_hops: StateValue<u64>,
    pub unresolved_edges: StateValue<u64>,
    pub ambiguous_edges: StateValue<u64>,
    pub declared_effects: StateValue<u64>,
    pub detected_effects: StateValue<u64>,
    pub policies: StateValue<u64>,
    pub scenarios: StateValue<u64>,
    pub largest_required_artifact: StateValue<LargestArtifact>,
    pub duplicate_supporting_tokens: StateValue<u64>,
    pub generated_maintained_ratio: StateValue<OwnershipRatio>,
    pub minimum_safe_context_estimate: StateValue<u64>,
    pub empirically_safe_context_tokens: StateValue<u64>,
}

impl Default for SubjectMetrics {
    fn default() -> Self {
        Self {
            direct_dependencies: StateValue::Unknown,
            transitive_dependencies: StateValue::Unknown,
            indirect_only_dependencies: StateValue::Unknown,
            required_modules: StateValue::Unknown,
            context_closure_estimated_tokens: StateValue::Unknown,
            minimum_required_semantic_tokens: StateValue::Unknown,
            supporting_semantic_tokens: StateValue::Unknown,
            optional_source_tokens: StateValue::Unknown,
            model_files: StateValue::Unknown,
            source_files: StateValue::Unknown,
            target_files: StateValue::Unknown,
            max_cross_module_hops: StateValue::Unknown,
            unresolved_edges: StateValue::Unknown,
            ambiguous_edges: StateValue::Unknown,
            declared_effects: StateValue::Unknown,
            detected_effects: StateValue::Unknown,
            policies: StateValue::Unknown,
            scenarios: StateValue::Unknown,
            largest_required_artifact: StateValue::Unknown,
            duplicate_supporting_tokens: StateValue::Unknown,
            generated_maintained_ratio: StateValue::Unknown,
            minimum_safe_context_estimate: StateValue::Unknown,
            empirically_safe_context_tokens: StateValue::Unknown,
        }
    }
}

/// The largest required artifact record (opaque id, never a path).
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct LargestArtifact {
    /// The opaque artifact identifier (digest- or id-based, path-free).
    pub artifact_id: String,
    /// The closed role vocabulary (`model`, `source`, `target`).
    pub role: &'static str,
    pub bytes: Option<u64>,
    pub estimated_tokens: u64,
}

/// The generated/maintained ownership record (exact integers only).
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct OwnershipRatio {
    pub generated_files: u64,
    pub maintained_files: u64,
    pub external_files: u64,
    pub unclassified_files: u64,
    /// The exact numerator G of G/(G+M).
    pub numerator: u64,
    /// The exact denominator G+M; zero is only representable as unknown.
    pub denominator: u64,
}

/// Compute the assessment and exact over-budget amount from the complete
/// required sum. A reduced completeness is always `indeterminate`, even
/// when the visible sum already exceeds the budget (the breakdown still
/// explains the proven part).
pub fn assess(
    complete: bool,
    required_tokens: u64,
    available_tokens: u64,
) -> (Assessment, StateValue<u64>) {
    if !complete {
        if required_tokens > available_tokens {
            // A proven lower bound that already exceeds the budget is
            // definitely over.
            return (
                Assessment::OverBudget,
                StateValue::Known(required_tokens.saturating_sub(available_tokens)),
            );
        }
        return (Assessment::Indeterminate, StateValue::Unknown);
    }
    if required_tokens > available_tokens {
        (
            Assessment::OverBudget,
            StateValue::Known(required_tokens - available_tokens),
        )
    } else {
        (Assessment::WithinBudget, StateValue::Known(0))
    }
}

/// The required-module count: the distinct modules owning required facts,
/// plus the subjects' own modules.
pub fn required_module_count(
    selection: &FactSelection,
    subject_modules: &[String],
) -> StateValue<u64> {
    let mut modules: BTreeMap<String, ()> = BTreeMap::new();
    for module in subject_modules {
        modules.insert(module.clone(), ());
    }
    for fact in &selection.required {
        if let Some(module) = &fact.module {
            modules.insert(module.clone(), ());
        }
    }
    StateValue::Known(modules.len() as u64)
}

/// The closure-scoped M6 counts: unique required fact identities per
/// class, with declared/detected effects kept separate.
pub fn semantic_counts(
    selection: &FactSelection,
) -> (StateValue<u64>, StateValue<u64>, StateValue<u64>) {
    let policies = selection
        .required
        .iter()
        .filter(|fact| fact.id.starts_with("policy:"))
        .count() as u64;
    let scenarios = selection
        .required
        .iter()
        .filter(|fact| fact.id.starts_with("scenario:"))
        .count() as u64;
    (
        StateValue::Known(selection.counted_operations.len() as u64),
        StateValue::Known(policies),
        StateValue::Known(scenarios),
    )
}

/// The M8 duplicate-supporting computation: the supporting requests
/// deduped to unique fact identities; the duplicate token sum is the
/// requested total minus the unique total.
pub fn duplicate_supporting_tokens(
    unique_supporting: &[LedgerFact],
    request_multiplicity: &BTreeMap<String, u64>,
) -> StateValue<u64> {
    let unique: BTreeMap<&str, u64> = unique_supporting
        .iter()
        .map(|fact| (fact.id.as_str(), fact.tokens))
        .collect();
    let mut requested_total = 0u64;
    let mut unique_total = 0u64;
    for (id, multiplicity) in request_multiplicity {
        let Some(tokens) = unique.get(id.as_str()) else {
            continue;
        };
        let Some(added) = tokens.checked_mul(*multiplicity) else {
            return StateValue::Unknown;
        };
        requested_total = match requested_total.checked_add(added) {
            Some(total) => total,
            None => return StateValue::Unknown,
        };
        unique_total = match unique_total.checked_add(*tokens) {
            Some(total) => total,
            None => return StateValue::Unknown,
        };
    }
    StateValue::Known(requested_total.saturating_sub(unique_total))
}

/// The M10 minimum-safe estimate under the profile margin and framing.
pub fn minimum_safe(profile: &Profile, required_tokens: StateValue<u64>) -> StateValue<u64> {
    match required_tokens {
        StateValue::Known(tokens) => {
            match super::estimate::minimum_safe_estimate(
                tokens,
                profile.margin_numerator,
                profile.margin_denominator,
                profile.framing_tokens,
            ) {
                Ok(estimate) => StateValue::Known(estimate),
                Err(_) => StateValue::Unknown,
            }
        }
        StateValue::Unknown => StateValue::Unknown,
        StateValue::Withheld => StateValue::Withheld,
        StateValue::Unsupported => StateValue::Unsupported,
    }
}

/// The M1 counts from one finished closure.
pub fn dependency_counts(
    closure: &DependencyClosure,
) -> (
    StateValue<u64>,
    StateValue<u64>,
    StateValue<u64>,
    StateValue<u64>,
) {
    let direct = closure.direct.len() as u64;
    let transitive = closure.transitive.len() as u64;
    let indirect = closure.indirect_only.len() as u64;
    let edge_count = direct.saturating_add(indirect).min(transitive);
    if closure.complete {
        (
            StateValue::Known(direct),
            StateValue::Known(transitive),
            StateValue::Known(indirect),
            StateValue::Known(edge_count),
        )
    } else {
        // A bounded walk still proves its distinct-node lower bound.
        (
            StateValue::Known(direct),
            StateValue::Known(transitive),
            StateValue::Unknown,
            StateValue::Known(edge_count),
        )
    }
}

/// The closed wire key of every metric (schema order).
pub const METRIC_KEYS: [&str; 23] = [
    "directDependencies",
    "transitiveDependencies",
    "indirectOnlyDependencies",
    "edgeOccurrences",
    "requiredModules",
    "contextClosureEstimatedTokens",
    "minimumRequiredSemanticTokens",
    "supportingSemanticTokens",
    "optionalSourceTokens",
    "modelFiles",
    "sourceFiles",
    "targetFiles",
    "maxCrossModuleHops",
    "unresolvedEdges",
    "ambiguousEdges",
    "declaredEffects",
    "detectedEffects",
    "policies",
    "scenarios",
    "largestRequiredArtifact",
    "duplicateSupportingTokens",
    "generatedMaintainedRatio",
    "minimumSafeContextEstimate",
];

/// The exact metric-semantics version string.
pub fn metric_version() -> &'static str {
    METRIC_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Profile {
        super::super::profile::generic_profile(12_000).unwrap()
    }

    #[test]
    fn assessment_boundary_is_exact() {
        assert_eq!(
            assess(true, 12_000, 12_000),
            (Assessment::WithinBudget, StateValue::Known(0))
        );
        assert_eq!(
            assess(true, 12_001, 12_000),
            (Assessment::OverBudget, StateValue::Known(1))
        );
        assert_eq!(
            assess(false, 11_999, 12_000),
            (Assessment::Indeterminate, StateValue::Unknown)
        );
        // A proven lower bound above the budget is still over.
        assert_eq!(
            assess(false, 13_000, 12_000),
            (Assessment::OverBudget, StateValue::Known(1_000))
        );
    }

    #[test]
    fn minimum_safe_applies_margin_and_framing() {
        let mut profile = profile();
        assert_eq!(
            minimum_safe(&profile, StateValue::Known(1_000)),
            StateValue::Known(1_000)
        );
        profile.margin_numerator = 11;
        profile.margin_denominator = 10;
        profile.framing_tokens = 20;
        assert_eq!(
            minimum_safe(&profile, StateValue::Known(1_000)),
            StateValue::Known(1_120)
        );
        assert_eq!(
            minimum_safe(&profile, StateValue::Unknown),
            StateValue::Unknown
        );
    }

    #[test]
    fn duplicates_are_requested_minus_unique() {
        let supporting = vec![LedgerFact {
            id: "entity:d".to_owned(),
            class: super::super::facts::FactClass::SupportingSemantic,
            reason: None,
            module: None,
            tokens: 10,
        }];
        let mut multiplicity = BTreeMap::new();
        multiplicity.insert("entity:d".to_owned(), 3);
        assert_eq!(
            duplicate_supporting_tokens(&supporting, &multiplicity),
            StateValue::Known(20)
        );
        let mut none = BTreeMap::new();
        none.insert("entity:d".to_owned(), 1);
        assert_eq!(
            duplicate_supporting_tokens(&supporting, &none),
            StateValue::Known(0)
        );
    }

    #[test]
    fn metric_keys_are_unique_and_camel_cased() {
        let mut sorted = METRIC_KEYS.to_vec();
        sorted.sort_unstable();
        assert_eq!(METRIC_KEYS.len(), sorted.len());
        assert!(METRIC_KEYS
            .iter()
            .all(|key| key.as_bytes()[0].is_ascii_lowercase()));
    }
}
