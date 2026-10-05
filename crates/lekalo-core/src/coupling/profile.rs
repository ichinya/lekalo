//! Threshold configuration is independent of semantic measurement identity.
use super::{
    diagnostic,
    wire::{self, State},
};
use crate::diagnostics::DiagnosticSet;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Measurement {
    pub recipe: String,
    pub include_observed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limit {
    pub metric: String,
    pub maximum: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rational {
    pub numerator: u64,
    pub denominator: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegressionLimit {
    pub metric: String,
    pub absolute_increase: u64,
    pub relative_increase: State<Rational>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub limits: Vec<Limit>,
    pub regression_limits: Vec<RegressionLimit>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleLimits {
    pub module: String,
    pub thresholds: Limits,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainGroup {
    pub id: String,
    pub modules: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Centrality {
    pub subject: String,
    pub reason: String,
    pub review_ref: String,
    pub rules: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Gate {
    pub mode: String,
    pub fail_on: Vec<String>,
    pub baseline_ready_ref: State<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub schema_version: String,
    pub identity: String,
    pub profile_id: String,
    pub profile_revision: u64,
    pub measurement: Measurement,
    pub project: Limits,
    pub modules: Vec<ModuleLimits>,
    pub domain_groups: Vec<DomainGroup>,
    pub centrality_declarations: Vec<Centrality>,
    pub gate: Gate,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            schema_version: format!("lekalo/coupling-profile/v{}", wire::VERSION),
            identity: format!("dev.lekalo.coupling-profile@{}", wire::VERSION),
            profile_id: "advisory".into(),
            profile_revision: 1,
            measurement: Measurement {
                recipe: wire::RECIPE.into(),
                include_observed: false,
            },
            project: Limits::default(),
            modules: vec![],
            domain_groups: vec![],
            centrality_declarations: vec![],
            gate: Gate {
                mode: "advisory".into(),
                fail_on: vec![],
                baseline_ready_ref: State::Unknown,
            },
        }
    }
}
fn token(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 192
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-/#".contains(&b))
}
fn distinct<T: Ord>(values: impl Iterator<Item = T>) -> bool {
    let mut set = BTreeSet::new();
    values.into_iter().all(|v| set.insert(v))
}
impl Profile {
    pub fn parse(bytes: &[u8]) -> Result<Self, DiagnosticSet> {
        let profile: Self = wire::decode(bytes)?;
        profile.validate()?;
        Ok(profile)
    }
    pub fn validate(&self) -> Result<(), DiagnosticSet> {
        if self.schema_version != format!("lekalo/coupling-profile/v{}", wire::VERSION)
            || self.identity != format!("dev.lekalo.coupling-profile@{}", wire::VERSION)
            || self.measurement.recipe != wire::RECIPE
        {
            return Err(diagnostic::unsupported("profile-version-or-recipe"));
        }
        let valid_limits = |l: &Limits| {
            l.limits.len() <= wire::METRICS.len()
                && l.regression_limits.len() <= wire::METRICS.len()
                && distinct(l.limits.iter().map(|v| &v.metric))
                && distinct(l.regression_limits.iter().map(|v| &v.metric))
                && l.limits.iter().all(|v| {
                    wire::METRICS.contains(&v.metric.as_str()) && v.maximum <= 9_007_199_254_740_991
                })
                && l.regression_limits.iter().all(|v| {
                    wire::METRICS.contains(&v.metric.as_str())
                        && v.absolute_increase <= 9_007_199_254_740_991
                        && match &v.relative_increase {
                            State::Known { value } => {
                                value.denominator > 0
                                    && value.denominator <= 9_007_199_254_740_991
                                    && value.numerator <= 9_007_199_254_740_991
                            }
                            State::Unknown => true,
                            _ => false,
                        }
                })
        };
        if !token(&self.profile_id)
            || self.profile_revision == 0
            || self.profile_revision > 9_007_199_254_740_991
            || !valid_limits(&self.project)
            || self.modules.len() > wire::MAX_SUBJECTS
            || self.domain_groups.len() > wire::MAX_SUBJECTS
            || self.centrality_declarations.len() > 256
            || !distinct(self.modules.iter().map(|m| &m.module))
            || !self
                .modules
                .iter()
                .all(|m| token(&m.module) && valid_limits(&m.thresholds))
            || !distinct(self.domain_groups.iter().map(|g| &g.id))
            || !distinct(self.domain_groups.iter().flat_map(|g| &g.modules))
            || !self.domain_groups.iter().all(|g| {
                token(&g.id)
                    && !g.modules.is_empty()
                    && g.modules.len() <= wire::MAX_SUBJECTS
                    && g.modules.iter().all(|m| token(m))
            })
            || !distinct(self.centrality_declarations.iter().map(|c| &c.subject))
            || !self.centrality_declarations.iter().all(|c| {
                token(&c.subject)
                    && token(&c.review_ref)
                    && !c.reason.is_empty()
                    && c.reason.len() <= 256
                    && !c.reason.chars().any(char::is_control)
                    && c.rules.len() <= 3
                    && distinct(c.rules.iter())
                    && c.rules.iter().all(|r| {
                        matches!(
                            r.as_str(),
                            "coupling.fan-exceeded"
                                | "coupling.public-contract-amplification"
                                | "coupling.shared-abstraction-radius"
                        )
                    })
            })
            || !matches!(self.gate.mode.as_str(), "advisory" | "strict")
            || !distinct(self.gate.fail_on.iter())
            || self.gate.fail_on.len() > 3
            || !self.gate.fail_on.iter().all(|r| {
                matches!(
                    r.as_str(),
                    "baseline-regression" | "required-incomplete" | "threshold-exceeded"
                )
            })
            || self
                .gate
                .baseline_ready_ref
                .value()
                .is_some_and(|v| !wire::is_digest(v))
            || !matches!(
                self.gate.baseline_ready_ref,
                State::Known { .. } | State::Unknown
            )
            || (self.gate.mode == "advisory" && !self.gate.fail_on.is_empty())
        {
            return Err(diagnostic::invalid("profile-invariant"));
        }
        Ok(())
    }
    pub fn limits(&self, module: &str) -> Limits {
        let mut result = self.project.clone();
        if let Some(m) = self.modules.iter().find(|m| m.module == module) {
            for l in &m.thresholds.limits {
                result.limits.retain(|v| v.metric != l.metric);
                result.limits.push(l.clone());
            }
            for l in &m.thresholds.regression_limits {
                result.regression_limits.retain(|v| v.metric != l.metric);
                result.regression_limits.push(l.clone());
            }
        }
        result
    }
}
