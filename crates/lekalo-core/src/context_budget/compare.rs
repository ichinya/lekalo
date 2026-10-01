//! Baseline comparison of two context-budget reports (issue #75).
//!
//! Comparability requires the same metric/fact-selection version, the
//! same estimator identity/spec digest, and an explicitly pinned
//! effective profile; model/revision provenance intentionally differs
//! across revisions and is never compared. Unknown provenance,
//! mismatched capabilities, or missing metrics produce
//! `incomparable` rows — never zero deltas or a pass. Signed absolute
//! deltas and relative deltas (unavailable with reason `zero-baseline`
//! on a zero base) are computed per metric; threshold crossings are the
//! caller policy's decision.

use super::metrics::METRIC_KEYS;
use super::value::AbsoluteDelta;
use super::version::METRIC_VERSION;
use serde::Serialize;

/// The closed row-level comparability verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RowVerdict {
    Comparable,
    Incomparable,
}

/// One per-subject metric delta row.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricDelta {
    pub metric: &'static str,
    pub verdict: RowVerdict,
    /// The closed incomparability reason; present exactly when
    /// `verdict` is `incomparable`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<AbsoluteDelta>,
    /// Relative increase (numerator/denominator over the base);
    /// unavailable with `zero-baseline` when the base is zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative: Option<(u64, u64, u64)>,
}

/// One subject-level comparison.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectComparison {
    pub subject: String,
    pub verdict: RowVerdict,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    pub deltas: Vec<MetricDelta>,
}

/// One whole comparison of two reports.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub metric_version: &'static str,
    pub comparable: bool,
    /// The closed configuration-change reason when the two reports ran
    /// under different pinned profiles or estimator identities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration_change: Option<&'static str>,
    pub subjects: Vec<SubjectComparison>,
}

/// Compare a baseline (base) against a candidate report. Subjects are
/// matched by exact kind-qualified id; added or removed subjects are
/// explicit rows, never compared against zero.
pub fn compare(base: &super::BudgetReport, candidate: &super::BudgetReport) -> Comparison {
    let mut configuration_change = None;
    if base.profile.digest != candidate.profile.digest {
        configuration_change = Some("profile-digest");
    } else if base.profile.estimator_identity != candidate.profile.estimator_identity {
        configuration_change = Some("estimator-identity");
    }
    // A profile or estimator change is a configuration change: the
    // comparison records both sides but can never produce a trend row
    // (remeasure under one pinned profile to obtain a trend).
    if configuration_change.is_some() {
        return Comparison {
            metric_version: METRIC_VERSION,
            comparable: false,
            configuration_change,
            subjects: Vec::new(),
        };
    }
    let base_subjects: std::collections::BTreeMap<&str, &super::SubjectReport> = base
        .subjects
        .iter()
        .map(|subject| (subject.id.as_str(), subject))
        .collect();
    let candidate_subjects: std::collections::BTreeMap<&str, &super::SubjectReport> = candidate
        .subjects
        .iter()
        .map(|subject| (subject.id.as_str(), subject))
        .collect();
    let mut subjects = Vec::new();
    let mut comparable_any = true;
    for (id, base_subject) in &base_subjects {
        let Some(candidate_subject) = candidate_subjects.get(id) else {
            comparable_any = false;
            subjects.push(SubjectComparison {
                subject: (*id).to_owned(),
                verdict: RowVerdict::Incomparable,
                reason: Some("subject-removed"),
                deltas: Vec::new(),
            });
            continue;
        };
        let (verdict, deltas) = compare_subject(base_subject, candidate_subject);
        if verdict == RowVerdict::Incomparable {
            comparable_any = false;
        }
        subjects.push(SubjectComparison {
            subject: (*id).to_owned(),
            verdict,
            reason: None,
            deltas,
        });
    }
    for id in candidate_subjects.keys() {
        if !base_subjects.contains_key(id) {
            comparable_any = false;
            subjects.push(SubjectComparison {
                subject: (*id).to_owned(),
                verdict: RowVerdict::Incomparable,
                reason: Some("subject-added"),
                deltas: Vec::new(),
            });
        }
    }
    subjects.sort_by(|left, right| left.subject.cmp(&right.subject));
    Comparison {
        metric_version: METRIC_VERSION,
        comparable: comparable_any,
        configuration_change,
        subjects,
    }
}

/// Compare one matched subject pair over the closed metric keys.
fn compare_subject(
    base: &super::SubjectReport,
    candidate: &super::SubjectReport,
) -> (RowVerdict, Vec<MetricDelta>) {
    let mut deltas = Vec::new();
    let mut comparable = true;
    for key in METRIC_KEYS {
        let base_value = metric_u64(&base.metrics, key);
        let candidate_value = metric_u64(&candidate.metrics, key);
        let (base_value, candidate_value) = match (base_value, candidate_value) {
            (Some(base), Some(candidate)) => (base, candidate),
            (None, None) => {
                // Both sides agree the metric is unavailable: the row is
                // comparable with no delta (a shared unknown is a fact,
                // never a fake zero).
                deltas.push(MetricDelta {
                    metric: key,
                    verdict: RowVerdict::Comparable,
                    reason: None,
                    delta: None,
                    relative: None,
                });
                continue;
            }
            _ => {
                comparable = false;
                deltas.push(MetricDelta {
                    metric: key,
                    verdict: RowVerdict::Incomparable,
                    reason: Some("metric-unknown"),
                    delta: None,
                    relative: None,
                });
                continue;
            }
        };
        let delta = AbsoluteDelta {
            base: base_value,
            candidate: candidate_value,
            delta: candidate_value as i64 - base_value as i64,
        };
        let relative = if base_value == 0 {
            None
        } else {
            // Rational relative delta: (candidate - base)/base as
            // numerator over the base, only when it fits.
            let difference = candidate_value as i128 - base_value as i128;
            let scaled = difference.unsigned_abs();
            if scaled <= u64::MAX as u128 {
                Some((
                    scaled as u64,
                    base_value,
                    if difference < 0 { 1 } else { 0 },
                ))
            } else {
                None
            }
        };
        deltas.push(MetricDelta {
            metric: key,
            verdict: RowVerdict::Comparable,
            reason: None,
            delta: Some(delta),
            relative,
        });
    }
    (verdict_of(comparable), deltas)
}

fn verdict_of(comparable: bool) -> RowVerdict {
    if comparable {
        RowVerdict::Comparable
    } else {
        RowVerdict::Incomparable
    }
}

/// One metric's known u64 value by closed wire key; `None` when the
/// metric is missing or not known on this subject.
fn metric_u64(metrics: &super::SubjectMetrics, key: &str) -> Option<u64> {
    use super::StateValue;
    let value = match key {
        "directDependencies" => &metrics.direct_dependencies,
        "transitiveDependencies" => &metrics.transitive_dependencies,
        "indirectOnlyDependencies" => &metrics.indirect_only_dependencies,
        "requiredModules" => &metrics.required_modules,
        "contextClosureEstimatedTokens" => &metrics.context_closure_estimated_tokens,
        "minimumRequiredSemanticTokens" => &metrics.minimum_required_semantic_tokens,
        "supportingSemanticTokens" => &metrics.supporting_semantic_tokens,
        "optionalSourceTokens" => &metrics.optional_source_tokens,
        "modelFiles" => &metrics.model_files,
        "sourceFiles" => &metrics.source_files,
        "targetFiles" => &metrics.target_files,
        "maxCrossModuleHops" => &metrics.max_cross_module_hops,
        "unresolvedEdges" => &metrics.unresolved_edges,
        "ambiguousEdges" => &metrics.ambiguous_edges,
        "declaredEffects" => &metrics.declared_effects,
        "detectedEffects" => &metrics.detected_effects,
        "policies" => &metrics.policies,
        "scenarios" => &metrics.scenarios,
        "duplicateSupportingTokens" => &metrics.duplicate_supporting_tokens,
        "minimumSafeContextEstimate" => &metrics.minimum_safe_context_estimate,
        "empiricallySafeContextTokens" => &metrics.empirically_safe_context_tokens,
        // Structured metrics compare through their token proxies and are
        // never silently zero.
        "largestRequiredArtifact" | "generatedMaintainedRatio" | "edgeOccurrences" => return None,
        _ => return None,
    };
    match value {
        StateValue::Known(known) => Some(*known),
        _ => None,
    }
}

/// Whether any comparable metric row exceeds the configured allowance
/// (deny when **either** the absolute or the relative bound is exceeded;
/// equality passes; the relative allowance is skipped at a zero base
/// while the absolute bound still applies).
pub fn regression_verdict(
    comparison: &Comparison,
    limits: &[super::policy::RegressionLimitWire],
) -> super::policy::BaselineVerdict {
    use super::policy::{BaselineVerdict, MetricWire};
    if !comparison.comparable {
        return BaselineVerdict::Incomparable;
    }
    if limits.is_empty() {
        return BaselineVerdict::Comparable;
    }
    for subject in &comparison.subjects {
        for delta in &subject.deltas {
            for limit in limits {
                let metric_matches = match limit.metric {
                    MetricWire::MinimumRequiredSemanticTokens => {
                        delta.metric == "minimumRequiredSemanticTokens"
                    }
                    MetricWire::ContextClosureEstimatedTokens => {
                        delta.metric == "contextClosureEstimatedTokens"
                    }
                };
                if !metric_matches {
                    continue;
                }
                let Some(absolute) = &delta.delta else {
                    continue;
                };
                if absolute.delta > 0 {
                    let increase = absolute.delta as u64;
                    if increase > limit.absolute_increase {
                        return BaselineVerdict::Regressed;
                    }
                    if let Some((numerator, denominator, _sign)) = &delta.relative {
                        if *denominator > 0
                            && (*numerator as u128 * limit.relative_increase.denominator as u128)
                                > (limit.relative_increase.numerator as u128 * *denominator as u128)
                        {
                            return BaselineVerdict::Regressed;
                        }
                    }
                }
            }
        }
    }
    BaselineVerdict::Comparable
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_budget::{plan, BudgetRequest, BudgetSelection, StateValue as _StateValue};
    use crate::ir::compile;
    use crate::loader::{normalize_model, LoadSelection};

    fn planner() -> crate::ir::Compilation {
        let model = normalize_model(&LoadSelection {
            project: Some("tests/fixtures/context-budget/planner".to_owned()),
        })
        .ok()
        .expect("fixture loads");
        compile(&model).expect("fixture compiles")
    }

    fn symbol_request(symbol: &str) -> BudgetRequest {
        BudgetRequest::new(Some(symbol.to_owned()), None, false, false, false, false)
            .expect("valid request")
    }

    /// AC5: the same pins compare; a doubled budget is a configuration
    /// change and the required-token delta is exact.
    #[test]
    fn comparable_delta_and_configuration_change() {
        let compilation = planner();
        let base = plan(
            &symbol_request("planner.focus_task"),
            &BudgetSelection::Generic(1_000),
            &compilation,
        )
        .expect("base plans");
        let candidate = plan(
            &symbol_request("planner.focus_task"),
            &BudgetSelection::Generic(1_000),
            &compilation,
        )
        .expect("candidate plans");
        let same = compare(&base, &candidate);
        assert!(same.comparable);
        assert_eq!(same.configuration_change, None);
        let changed = plan(
            &symbol_request("planner.focus_task"),
            &BudgetSelection::Generic(2_000),
            &compilation,
        )
        .expect("changed plans");
        let different = compare(&base, &changed);
        assert_eq!(different.configuration_change, Some("profile-digest"));
        assert!(!different.comparable, "changed profiles are incomparable");
    }

    /// A removed subject is an explicit incomparable row, never a zero.
    #[test]
    fn removed_subject_is_explicit() {
        let compilation = planner();
        let full = plan(
            &symbol_request("planner.focus_task"),
            &BudgetSelection::Generic(1_000),
            &compilation,
        )
        .expect("full plans");
        let empty_selection = BudgetRequest::new(None, None, true, false, false, false).unwrap();
        let _ = empty_selection;
        // Compare a subject against a report without it: synthesize by
        // comparing full against itself with one subject cleared.
        let mut reduced = plan(
            &symbol_request("planner.focus_task"),
            &BudgetSelection::Generic(1_000),
            &compilation,
        )
        .expect("reduced plans");
        reduced.subjects.clear();
        reduced.summary.subjects = 0;
        let comparison = compare(&full, &reduced);
        assert!(!comparison.comparable);
        assert_eq!(comparison.subjects[0].reason, Some("subject-removed"));
    }

    /// The unknown metric never produces a fake zero delta.
    #[test]
    fn unknown_metric_is_incomparable() {
        let compilation = planner();
        let mut base = plan(
            &symbol_request("planner.focus_task"),
            &BudgetSelection::Generic(1_000),
            &compilation,
        )
        .expect("base plans");
        let candidate = plan(
            &symbol_request("planner.focus_task"),
            &BudgetSelection::Generic(1_000),
            &compilation,
        )
        .expect("candidate plans");
        let mut candidate_unknown = candidate.clone();
        base.subjects[0].metrics.detected_effects = _StateValue::Known(2);
        candidate_unknown.subjects[0].metrics.detected_effects = _StateValue::Unknown;
        let comparison = compare(&base, &candidate_unknown);
        let row = comparison
            .subjects
            .iter()
            .flat_map(|subject| subject.deltas.iter())
            .find(|delta| delta.metric == "detectedEffects")
            .expect("detectedEffects row");
        assert_eq!(row.verdict, RowVerdict::Incomparable);
        assert_eq!(row.reason, Some("metric-unknown"));
    }

    /// The regression verdict applies the either-bound semantics.
    #[test]
    fn regression_verdict_applies_allowances() {
        use crate::context_budget::policy::{BaselineVerdict, MetricWire, RegressionLimitWire};
        let limits = vec![RegressionLimitWire {
            metric: MetricWire::MinimumRequiredSemanticTokens,
            absolute_increase: 100,
            relative_increase: crate::context_budget::policy::RelativeIncreaseWire {
                numerator: 1,
                denominator: 10,
            },
        }];
        let compilation = planner();
        let base = plan(
            &symbol_request("planner.focus_task"),
            &BudgetSelection::Generic(1_000),
            &compilation,
        )
        .expect("base plans");
        let candidate = plan(
            &symbol_request("planner.focus_task"),
            &BudgetSelection::Generic(1_000),
            &compilation,
        )
        .expect("candidate plans");
        let comparison = compare(&base, &candidate);
        assert_eq!(
            regression_verdict(&comparison, &limits),
            BaselineVerdict::Comparable
        );
        // Zero baseline: the relative bound is skipped, absolute applies.
        let mut zeroed = base.clone();
        for fact in &mut zeroed.subjects[0].required_facts {
            fact.tokens = 0;
        }
        zeroed.subjects[0].metrics.minimum_required_semantic_tokens = _StateValue::Known(0);
        let zero_comparison = compare(&zeroed, &candidate);
        assert_eq!(
            regression_verdict(&zero_comparison, &limits),
            BaselineVerdict::Regressed,
            "a zero base with a large absolute increase still regresses"
        );
    }
}
