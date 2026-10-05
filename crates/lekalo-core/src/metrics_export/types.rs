//! The named #100 -> #102 seam. Measurements never arrive in this input.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase", deny_unknown_fields)]
pub enum ValueState<T> {
    Known { value: T },
    Unknown,
    Withheld,
    Unsupported,
}

impl<T> ValueState<T> {
    pub fn known(value: T) -> Self {
        Self::Known { value }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationInput {
    #[serde(rename = "schema_version")]
    pub schema_version: String,
    pub identity: String,
    pub protocol: String,
    pub approved: bool,
    pub trials: Vec<Trial>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Trial {
    pub unit: String,
    pub arm: Arm,
    pub run_id: String,
    pub required_assertions: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Arm {
    Baseline,
    LekaloAssisted,
}

impl Arm {
    pub fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::LekaloAssisted => "lekalo-assisted",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definition {
    #[serde(rename = "schema_version")]
    pub schema_version: String,
    pub identity: String,
    pub minimum_samples: u64,
    pub sampling_unit: String,
    pub recipe: String,
    pub fields: Vec<String>,
    pub alias_rule: String,
    pub rules: DefinitionRules,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DefinitionRules {
    pub population: String,
    pub numeric: String,
    pub cost: String,
    pub uncertainty: String,
    pub small_cells: String,
    pub comparison: String,
    pub release_budget: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExactRef {
    pub identity: String,
    pub digest: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Statistic {
    pub sample: ValueState<u64>,
    pub sum: ValueState<String>,
    pub mean: ValueState<Rational>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rational {
    pub numerator: String,
    pub denominator: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cohort {
    pub arm: Arm,
    pub pilot: String,
    pub scope_state: String,
    pub repository_role: String,
    pub repository_alias: String,
    pub profile_alias: ValueState<String>,
    pub model_alias: ValueState<String>,
    pub harness_alias: ValueState<String>,
    pub stack: ValueState<String>,
    pub context_coverage: ValueState<Rational>,
    pub core_version: ValueState<String>,
    pub profile_version: ValueState<String>,
    pub harness_version: ValueState<String>,
    pub sample: ValueState<u64>,
    pub outcomes: ValueState<std::collections::BTreeMap<String, u64>>,
    pub coverage: ValueState<std::collections::BTreeMap<String, u64>>,
    pub metrics: std::collections::BTreeMap<String, Statistic>,
    pub verified_success: ValueState<u64>,
    pub success_rate: ValueState<Rational>,
    pub cost: ValueState<Cost>,
    pub confidence: ValueState<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
    pub currency: String,
    pub basis: String,
    pub amount: String,
    pub per_success: ValueState<Rational>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicMetrics {
    #[serde(rename = "schema_version")]
    pub schema_version: &'static str,
    pub identity: &'static str,
    pub artifact_kind: &'static str,
    pub policy_ref: Value,
    pub authority_ref: Value,
    pub definition_ref: ExactRef,
    pub evaluation_protocol: &'static str,
    pub cohorts: Vec<Cohort>,
    pub comparisons: Vec<Comparison>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub baseline_cohort: u64,
    pub assisted_cohort: u64,
    pub success_lift: ValueState<Rational>,
}

pub(crate) struct Source {
    pub trial: Trial,
    pub record: Value,
    pub assertions: Option<Value>,
    pub record_digest: String,
    pub assertion_digest: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Custody {
    #[serde(rename = "schema_version")]
    pub schema_version: String,
    pub identity: String,
    pub view: String,
    pub generation: u64,
    pub tenant_scope_id: String,
    pub dependent_id: String,
    pub evaluation: EvaluationInput,
    pub projection: Value,
    pub payload_digest: String,
    pub manifest_digest: String,
    pub decision_template: Value,
}
