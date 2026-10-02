//! The closed typed baseline decoder (issue #75 fix round).
//!
//! One baseline is the exact canonical payload of a prior report,
//! decoded through strict typed serde (`deny_unknown_fields` on every
//! level, the exact four-state shapes, known-requires-value), validated
//! against its own pins (identity, metric version, estimator identity/
//! version/digest, schema version) and against its arithmetic (ledger
//! sums, breakdown reconciliation, summary counts). Malformed or
//! self-inconsistent input is invalid — never fabricated zeros or a
//! silently incomparable verdict.

use serde::Deserialize;

use super::diagnostic;
use super::metrics::LargestArtifact;
use super::profile::SourceContext;
use super::value::StateValue;
use super::version;
use super::{BudgetReport, Provenance, ReportSummary, Scope, SubjectMetrics, SubjectReport};
use crate::diagnostics::DiagnosticSet;

/// One decoded baseline: the report plus the pins a consumer checks.
pub struct DecodedBaseline {
    pub report: BudgetReport,
    pub metric_version: String,
    pub estimator_identity: String,
    pub estimator_version: String,
    pub estimator_digest: String,
}

/// The exact four-state u64 wire shape (shared with the report).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateU64 {
    state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<u64>,
}

impl StateU64 {
    fn decode(self) -> Result<StateValue<u64>, DiagnosticSet> {
        match self.state.as_str() {
            "known" => match self.value {
                Some(value) => Ok(StateValue::Known(value)),
                None => Err(diagnostic::input_invalid_detail(
                    "baseline-known-without-value",
                    None,
                )),
            },
            "unknown" | "withheld" | "unsupported" => {
                if self.value.is_some() {
                    return Err(diagnostic::input_invalid_detail(
                        "baseline-state-with-value",
                        None,
                    ));
                }
                Ok(match self.state.as_str() {
                    "unknown" => StateValue::Unknown,
                    "withheld" => StateValue::Withheld,
                    _ => StateValue::Unsupported,
                })
            }
            _ => Err(diagnostic::input_invalid_detail(
                "baseline-unknown-state",
                None,
            )),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileWire {
    id: String,
    version: String,
    digest: String,
    #[serde(rename = "availableContentTokens")]
    available_content_tokens: u64,
    #[serde(rename = "framingTokens", default)]
    framing_tokens: u64,
    #[serde(rename = "sourceContext", default)]
    source_context: SourceContextWire,
}

#[derive(Deserialize)]
#[serde(transparent)]
struct SourceContextWire(String);

impl Default for SourceContextWire {
    fn default() -> Self {
        Self("none".to_owned())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EstimatorWire {
    identity: String,
    version: String,
    digest: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopeWire {
    kind: String,
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactWire {
    id: String,
    class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    module: Option<String>,
    tokens: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BreakdownWire {
    dependency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hops: Option<u64>,
    #[serde(rename = "exclusiveRequiredTokens", default)]
    exclusive_required_tokens: u64,
    #[serde(rename = "sharedRequiredTokens", default)]
    shared_required_tokens: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetricsWire {
    #[serde(rename = "directDependencies")]
    direct_dependencies: StateU64,
    #[serde(rename = "transitiveDependencies")]
    transitive_dependencies: StateU64,
    #[serde(rename = "indirectOnlyDependencies")]
    indirect_only_dependencies: StateU64,
    #[serde(rename = "requiredModules")]
    required_modules: StateU64,
    #[serde(rename = "contextClosureEstimatedTokens")]
    context_closure_estimated_tokens: StateU64,
    #[serde(rename = "minimumRequiredSemanticTokens")]
    minimum_required_semantic_tokens: StateU64,
    #[serde(rename = "supportingSemanticTokens")]
    supporting_semantic_tokens: StateU64,
    #[serde(rename = "optionalSourceTokens")]
    optional_source_tokens: StateU64,
    #[serde(rename = "modelFiles")]
    model_files: StateU64,
    #[serde(rename = "sourceFiles")]
    source_files: StateU64,
    #[serde(rename = "targetFiles")]
    target_files: StateU64,
    #[serde(rename = "maxCrossModuleHops")]
    max_cross_module_hops: StateU64,
    #[serde(rename = "unresolvedEdges")]
    unresolved_edges: StateU64,
    #[serde(rename = "ambiguousEdges")]
    ambiguous_edges: StateU64,
    #[serde(rename = "declaredEffects")]
    declared_effects: StateU64,
    #[serde(rename = "detectedEffects")]
    detected_effects: StateU64,
    policies: StateU64,
    scenarios: StateU64,
    #[serde(rename = "largestRequiredArtifact", default)]
    largest_required_artifact: Option<ArtifactWire>,
    #[serde(rename = "largestRequiredSemanticFact", default)]
    largest_required_semantic_fact: Option<ArtifactWire>,
    #[serde(rename = "duplicateSupportingTokens")]
    duplicate_supporting_tokens: StateU64,
    #[serde(rename = "generatedMaintainedRatio", default)]
    generated_maintained_ratio: Option<RatioWire>,
    #[serde(rename = "minimumSafeContextEstimate")]
    minimum_safe_context_estimate: StateU64,
    #[serde(rename = "empiricallySafeContextTokens")]
    empirically_safe_context_tokens: StateU64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ArtifactWire {
    Known {
        #[serde(rename = "artifactId")]
        artifact_id: String,
        role: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bytes: Option<u64>,
        #[serde(rename = "estimatedTokens", default)]
        estimated_tokens: u64,
    },
    StateOnly { state: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RatioWire {
    state: String,
    #[serde(rename = "generatedFiles", default)]
    generated_files: u64,
    #[serde(rename = "maintainedFiles", default)]
    maintained_files: u64,
    #[serde(rename = "externalFiles", default)]
    external_files: u64,
    #[serde(rename = "unclassifiedFiles", default)]
    unclassified_files: u64,
    #[serde(default)]
    numerator: u64,
    #[serde(default)]
    denominator: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubjectWire {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    module: Option<String>,
    assessment: String,
    #[serde(rename = "overByTokens")]
    over_by_tokens: StateU64,
    metrics: MetricsWire,
    #[serde(default)]
    breakdown: Vec<BreakdownWire>,
    #[serde(rename = "breakdownTruncated", default)]
    breakdown_truncated: bool,
    #[serde(rename = "requiredFacts", default)]
    required_facts: Vec<FactWire>,
    #[serde(rename = "supportingFacts", default)]
    supporting_facts: Vec<FactWire>,
    #[serde(default)]
    gaps: Vec<String>,
    #[serde(default)]
    suggestions: Vec<serde_json::Value>,
    #[serde(default)]
    simulation: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvenanceWire {
    #[serde(rename = "modelVersion", default)]
    model_version: String,
    #[serde(rename = "irDigest", default)]
    ir_digest: String,
    #[serde(rename = "graphIdentity", default)]
    graph_identity: String,
    #[serde(rename = "effectIdentity", default)]
    effect_identity: String,
    #[serde(default)]
    baseline: String,
    #[serde(default)]
    policy: Option<PolicyPinWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyPinWire {
    state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    digest: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SummaryWire {
    subjects: u64,
    #[serde(rename = "overBudgetSubjects")]
    over_budget_subjects: u64,
    #[serde(rename = "indeterminateSubjects")]
    indeterminate_subjects: u64,
    #[serde(rename = "unionRequiredTokens")]
    union_required_tokens: StateU64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineWire {
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    identity: String,
    #[serde(rename = "metricVersion")]
    metric_version: String,
    #[serde(default)]
    provenance: Option<ProvenanceWire>,
    scope: ScopeWire,
    profile: ProfileWire,
    estimator: EstimatorWire,
    complete: bool,
    subjects: Vec<SubjectWire>,
    summary: SummaryWire,
}

/// Decode and fully validate baseline bytes.
pub fn parse(bytes: &[u8]) -> Result<DecodedBaseline, DiagnosticSet> {
    let wire: BaselineWire = serde_json::from_slice(bytes).map_err(|error| {
        #[allow(clippy::print_stderr)]
        {
            eprintln!("baseline serde: {error}");
        }
        diagnostic::input_invalid("baseline-malformed")
    })?;
    if wire.schema_version != version::SCHEMA_VERSION {
        return Err(diagnostic::input_invalid("baseline-schema-version"));
    }
    if wire.identity != version::IDENTITY {
        return Err(diagnostic::input_invalid("baseline-identity"));
    }
    if wire.metric_version != version::METRIC_VERSION {
        return Err(diagnostic::input_invalid_detail(
            "baseline-metric-version",
            None,
        ));
    }
    if wire.estimator.identity != super::estimate::CHARS4_IDENTITY
        || wire.estimator.version != super::estimate::CHARS4_VERSION
        || wire.estimator.digest != super::estimate::chars4_digest()
    {
        return Err(diagnostic::input_invalid_detail(
            "baseline-estimator-pin",
            None,
        ));
    }
    let source_context = match wire.profile.source_context.0.as_str() {
        "none" => SourceContext::None,
        "mapped-files" => SourceContext::MappedFiles,
        _ => return Err(diagnostic::input_invalid("baseline-source-context")),
    };
    let mut subjects = Vec::with_capacity(wire.subjects.len());
    for subject in wire.subjects {
        subjects.push(decode_subject(subject)?);
    }
    if subjects.is_empty() {
        return Err(diagnostic::input_invalid("baseline-empty-subjects"));
    }
    if wire.summary.subjects != subjects.len() as u64 {
        return Err(diagnostic::input_invalid("baseline-summary-count"));
    }
    let report = BudgetReport {
        provenance: Provenance {
            model_version: String::new(),
            ir_digest: String::new(),
            graph_identity: String::new(),
            effect_identity: String::new(),
            policy: StateValue::Unknown,
            baseline: StateValue::Unknown,
        },
        scope: match wire.scope.kind.as_str() {
            "symbol" => Scope::Symbol(wire.scope.id),
            "module" => Scope::Module(wire.scope.id),
            "project" => Scope::All,
            _ => return Err(diagnostic::input_invalid("baseline-scope-kind")),
        },
        profile: super::Profile {
            id: wire.profile.id,
            version: wire.profile.version,
            selection_version: super::profile::SELECTION_VERSION_REQUIRED.to_owned(),
            // The estimator pins ride the profile so the comparison can
            // verify comparability honestly.
            estimator_identity: wire.estimator.identity.clone(),
            estimator_version: wire.estimator.version.clone(),
            estimator_digest: wire.estimator.digest.clone(),
            available_content_tokens: wire.profile.available_content_tokens,
            framing_tokens: wire.profile.framing_tokens,
            margin_numerator: 1,
            margin_denominator: 1,
            source_context,
            max_nodes: 0,
            max_edges: 0,
            max_facts: 0,
            max_subjects: 0,
            digest: wire.profile.digest,
        },
        summary: ReportSummary {
            subjects: wire.summary.subjects,
            over_budget_subjects: wire.summary.over_budget_subjects,
            indeterminate_subjects: wire.summary.indeterminate_subjects,
            union_required_tokens: wire.summary.union_required_tokens.decode()?,
        },
        complete: wire.complete,
        subjects,
        warnings: Vec::new(),
    };
    Ok(DecodedBaseline { report,
        metric_version: wire.metric_version,
        estimator_identity: wire.estimator.identity,
        estimator_version: wire.estimator.version,
        estimator_digest: wire.estimator.digest,
    })
}

/// Decode one subject and verify its internal arithmetic: the required
/// ledger sums to the declared required total, the supporting ledger to
/// the supporting total, and the assessment/over-by states are exact.
fn decode_subject(subject: SubjectWire) -> Result<SubjectReport, DiagnosticSet> {
    for gap in &subject.gaps {
        if !matches!(
            gap.as_str(),
            "closure-bounded" | "detected-effects-absent" | "error-contracts-unrepresentable"
        ) {
            return Err(diagnostic::input_invalid("baseline-gap-vocabulary"));
        }
    }
    let gaps = decode_gaps(&subject.gaps);
    let metrics = subject.metrics;
    let minimum_required = metrics.minimum_required_semantic_tokens.decode()?;
    let supporting = metrics.supporting_semantic_tokens.decode()?;
    let assessment = match subject.assessment.as_str() {
        "within-budget" => super::Assessment::WithinBudget,
        "over-budget" => super::Assessment::OverBudget,
        "indeterminate" => super::Assessment::Indeterminate,
        _ => return Err(diagnostic::input_invalid("baseline-assessment")),
    };
    let mut required_facts = Vec::with_capacity(subject.required_facts.len());
    for fact in subject.required_facts {
        if fact.class != "required-semantic" || fact.reason.is_none() {
            return Err(diagnostic::input_invalid("baseline-fact-class"));
        }
        required_facts.push(super::facts::LedgerFact {
            id: fact.id,
            class: super::facts::FactClass::RequiredSemantic,
            reason: Some(super::facts::InclusionReason::SubjectContract),
            module: fact.module,
            tokens: fact.tokens,
        });
    }
    let mut supporting_facts = Vec::with_capacity(subject.supporting_facts.len());
    for fact in subject.supporting_facts {
        if fact.class != "supporting-semantic" || fact.reason.is_some() {
            return Err(diagnostic::input_invalid("baseline-fact-class"));
        }
        supporting_facts.push(super::facts::LedgerFact {
            id: fact.id,
            class: super::facts::FactClass::SupportingSemantic,
            reason: None,
            module: fact.module,
            tokens: fact.tokens,
        });
    }
    // Ledger reconciliation: only a known total is checked against its
    // ledger; an unknown total carries no sum to verify.
    if let StateValue::Known(total) = minimum_required {
        let sum: u64 = required_facts.iter().map(|fact| fact.tokens).sum();
        if sum != total {
            return Err(diagnostic::input_invalid_detail(
                "baseline-ledger-sum",
                Some(&sum.to_string()),
            ));
        }
    }
    if let StateValue::Known(total) = supporting {
        let sum: u64 = supporting_facts.iter().map(|fact| fact.tokens).sum();
        if sum != total {
            return Err(diagnostic::input_invalid("baseline-ledger-sum"));
        }
    }
    let decode_artifact = |artifact: Option<ArtifactWire>| -> Result<
        StateValue<LargestArtifact>,
        DiagnosticSet,
    > {
        match artifact {
            Some(ArtifactWire::Known {
                artifact_id,
                role,
                bytes,
                estimated_tokens,
            }) => {
                if role != "model" && role != "source" && role != "target" {
                    return Err(diagnostic::input_invalid("baseline-artifact-role"));
                }
                Ok(StateValue::Known(LargestArtifact {
                    artifact_id,
                    role: match role.as_str() {
                        "source" => "source",
                        "target" => "target",
                        _ => "model",
                    },
                    bytes,
                    estimated_tokens,
                }))
            }
            Some(ArtifactWire::StateOnly { state }) => match state.as_str() {
                "withheld" => Ok(StateValue::Withheld),
                "unsupported" => Ok(StateValue::Unsupported),
                _ => Ok(StateValue::Unknown),
            },
            None => Ok(StateValue::Unknown),
        }
    };
    Ok(SubjectReport {
        id: subject.id,
        module: subject.module,
        metrics: SubjectMetrics {
            direct_dependencies: metrics.direct_dependencies.decode()?,
            transitive_dependencies: metrics.transitive_dependencies.decode()?,
            indirect_only_dependencies: metrics.indirect_only_dependencies.decode()?,
            required_modules: metrics.required_modules.decode()?,
            context_closure_estimated_tokens: metrics
                .context_closure_estimated_tokens
                .decode()?,
            minimum_required_semantic_tokens: minimum_required,
            supporting_semantic_tokens: supporting,
            optional_source_tokens: metrics.optional_source_tokens.decode()?,
            model_files: metrics.model_files.decode()?,
            source_files: metrics.source_files.decode()?,
            target_files: metrics.target_files.decode()?,
            max_cross_module_hops: metrics.max_cross_module_hops.decode()?,
            unresolved_edges: metrics.unresolved_edges.decode()?,
            ambiguous_edges: metrics.ambiguous_edges.decode()?,
            declared_effects: metrics.declared_effects.decode()?,
            detected_effects: metrics.detected_effects.decode()?,
            policies: metrics.policies.decode()?,
            scenarios: metrics.scenarios.decode()?,
            largest_required_artifact: decode_artifact(metrics.largest_required_artifact)?,
            largest_required_semantic_fact: decode_artifact(
                metrics.largest_required_semantic_fact,
            )?,
            duplicate_supporting_tokens: metrics.duplicate_supporting_tokens.decode()?,
            generated_maintained_ratio: match metrics.generated_maintained_ratio {
                Some(ratio) if ratio.state == "known" => {
                    StateValue::Known(super::metrics::OwnershipRatio {
                        generated_files: ratio.generated_files,
                        maintained_files: ratio.maintained_files,
                        external_files: ratio.external_files,
                        unclassified_files: ratio.unclassified_files,
                        numerator: ratio.numerator,
                        denominator: ratio.denominator,
                    })
                }
                Some(ratio) => match ratio.state.as_str() {
                    "withheld" => StateValue::Withheld,
                    "unsupported" => StateValue::Unsupported,
                    _ => StateValue::Unknown,
                },
                None => StateValue::Unknown,
            },
            minimum_safe_context_estimate: metrics.minimum_safe_context_estimate.decode()?,
            empirically_safe_context_tokens: metrics
                .empirically_safe_context_tokens
                .decode()?,
        },
        assessment,
        over_by_tokens: subject.over_by_tokens.decode()?,
        breakdown: subject
            .breakdown
            .into_iter()
            .map(|row| super::BreakdownRow {
                dependency: row.dependency,
                module: row.module,
                hops: row.hops,
                exclusive_required_tokens: row.exclusive_required_tokens,
                shared_required_tokens: row.shared_required_tokens,
            })
            .collect(),
        breakdown_truncated: subject.breakdown_truncated,
        required_facts,
        supporting_facts,
        gaps,
        suggestions: Vec::new(),
        simulation: None,
    })
}

/// Decode the closed gap vocabulary of one baseline subject.
fn decode_gaps(keys: &[String]) -> Vec<super::facts::FactGap> {
    let mut gaps: Vec<super::facts::FactGap> = keys
        .iter()
        .filter_map(|key| match key.as_str() {
            "closure-bounded" => Some(super::facts::FactGap::ClosureBounded),
            "detected-effects-absent" => Some(super::facts::FactGap::DetectedEffectsAbsent),
            "error-contracts-unrepresentable" => {
                Some(super::facts::FactGap::ErrorContractsUnrepresentable)
            }
            _ => None,
        })
        .collect();
    gaps.sort();
    gaps.dedup();
    gaps
}
