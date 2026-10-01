//! The closed mandatory budget policy document (issue #75).
//!
//! A policy upgrades specific advisory findings of one pinned profile
//! into gate denials: over-budget subjects, reduced required
//! completeness, or baseline regressions. The default without a policy
//! is advisory; a policy is enforceable only in invocations that select
//! it, and a denied report still renders in full — the verdict never
//! suppresses its evidence.

use serde::Deserialize;

use super::diagnostic;
use super::profile::Profile;
use crate::diagnostics::DiagnosticSet;

/// The exact wire discriminator of the policy contract.
pub const SCHEMA_VERSION: &str = "lekalo/context-budget-policy/v0.6.3";
/// The exact policy contract identity.
pub const IDENTITY: &str = "dev.lekalo.context-budget-policy@0.6.3";

/// One signed regression allowance: deny when **either** bound is
/// exceeded; equality passes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegressionLimitWire {
    pub metric: MetricWire,
    #[serde(rename = "absoluteIncrease")]
    pub absolute_increase: u64,
    #[serde(rename = "relativeIncrease")]
    pub relative_increase: RelativeIncreaseWire,
}

/// The closed metric vocabulary of one allowance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricWire {
    #[serde(rename = "minimumRequiredSemanticTokens")]
    MinimumRequiredSemanticTokens,
    #[serde(rename = "contextClosureEstimatedTokens")]
    ContextClosureEstimatedTokens,
}

/// The rational relative bound (numerator/denominator, both positive).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelativeIncreaseWire {
    pub numerator: u64,
    pub denominator: u64,
}

/// The profile pin: id, version, and exact digest.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileRefWire {
    pub id: String,
    pub version: String,
    pub digest: String,
}

/// The whole closed policy document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy {
    /// `mandatory` is the only selectable mode in this generation.
    pub mode: PolicyMode,
    pub profile_ref: ProfileRefWire,
    pub fail_on_over_budget: bool,
    pub fail_on_required_incomplete: bool,
    pub fail_on_baseline_regression: bool,
    pub regression_limits: Vec<RegressionLimitWire>,
}

/// The closed policy-mode vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyMode {
    Mandatory,
}

/// Parse and validate policy bytes.
pub fn parse(bytes: &[u8]) -> Result<Policy, DiagnosticSet> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct DocumentWire {
        #[serde(rename = "schemaVersion")]
        schema_version: String,
        identity: String,
        mode: String,
        #[serde(rename = "profileRef")]
        profile_ref: ProfileRefWire,
        #[serde(rename = "failOn")]
        fail_on: Vec<String>,
        #[serde(rename = "regressionLimits", default)]
        regression_limits: Vec<RegressionLimitWire>,
    }
    let wire: DocumentWire = serde_json::from_slice(bytes)
        .map_err(|_| diagnostic::input_invalid("policy-document-malformed"))?;
    if wire.schema_version != SCHEMA_VERSION || wire.identity != IDENTITY {
        return Err(diagnostic::profile_unsupported(
            "policy-contract-version",
            None,
        ));
    }
    if wire.mode != "mandatory" {
        return Err(diagnostic::input_invalid("policy-mode-unsupported"));
    }
    if wire.fail_on.is_empty() {
        return Err(diagnostic::input_invalid("policy-fail-on-empty"));
    }
    let mut policy = Policy {
        mode: PolicyMode::Mandatory,
        profile_ref: wire.profile_ref,
        fail_on_over_budget: false,
        fail_on_required_incomplete: false,
        fail_on_baseline_regression: false,
        regression_limits: wire.regression_limits,
    };
    for reason in &wire.fail_on {
        match reason.as_str() {
            "over-budget" => policy.fail_on_over_budget = true,
            "required-incomplete" => policy.fail_on_required_incomplete = true,
            "baseline-regression" => policy.fail_on_baseline_regression = true,
            _ => return Err(diagnostic::input_invalid("policy-fail-on-unknown")),
        }
    }
    for limit in &policy.regression_limits {
        if limit.relative_increase.denominator == 0 {
            return Err(diagnostic::input_invalid("policy-relative-denominator"));
        }
    }
    Ok(policy)
}

impl Policy {
    /// Whether this policy is satisfied by (and pinned to) the effective
    /// profile of the report. A digest mismatch denies before any metric
    /// evaluation: an advisory-local invocation can never be represented
    /// as passing a mandatory QA policy.
    pub fn pins(&self, profile: &Profile) -> bool {
        self.profile_ref.id == profile.id
            && self.profile_ref.version == profile.version
            && self.profile_ref.digest == profile.digest
    }

    /// Evaluate the mandatory gate over one report's aggregate facts.
    /// Returns the denial reason token, or `None` when the gate passes.
    pub fn evaluate(
        &self,
        over_budget: bool,
        complete: bool,
        baseline_regression: Option<&BaselineVerdict>,
    ) -> Option<&'static str> {
        if self.fail_on_over_budget && over_budget {
            return Some("over-budget");
        }
        if self.fail_on_required_incomplete && !complete {
            return Some("required-incomplete");
        }
        if self.fail_on_baseline_regression {
            match baseline_regression {
                Some(BaselineVerdict::Regressed) => return Some("baseline-regression"),
                Some(BaselineVerdict::Incomparable) => return Some("baseline-incomparable"),
                Some(BaselineVerdict::Comparable) | None => {}
            }
        }
        None
    }
}

/// The closed baseline verdict of one comparison.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BaselineVerdict {
    /// Every compared metric stayed inside every allowance.
    Comparable,
    /// At least one allowance was exceeded.
    Regressed,
    /// The baseline could not support a verdict at all.
    Incomparable,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> String {
        format!(
            r#"{{"schemaVersion":"{SCHEMA_VERSION}","identity":"{IDENTITY}","mode":"mandatory","profileRef":{{"id":"local-12k","version":"1","digest":"sha256:1111111111111111111111111111111111111111111111111111111111111111"}},"failOn":["over-budget","required-incomplete"],"regressionLimits":[{{"metric":"minimumRequiredSemanticTokens","absoluteIncrease":1000,"relativeIncrease":{{"numerator":1,"denominator":10}}}}]}}"#
        )
    }

    #[test]
    fn mandatory_document_parses() {
        let policy = parse(document().as_bytes()).expect("parses");
        assert!(policy.fail_on_over_budget);
        assert!(policy.fail_on_required_incomplete);
        assert!(!policy.fail_on_baseline_regression);
        assert_eq!(policy.regression_limits.len(), 1);
    }

    #[test]
    fn negatives_refuse_closed() {
        assert!(parse(b"{").is_err());
        let empty = document().replace(
            "\"failOn\":[\"over-budget\",\"required-incomplete\"]",
            "\"failOn\":[]",
        );
        assert!(parse(empty.as_bytes()).is_err());
        let advisory = document().replace("\"mode\":\"mandatory\"", "\"mode\":\"advisory\"");
        assert!(parse(advisory.as_bytes()).is_err());
        let unknown = document().replace("\"over-budget\",", "\"over-salary\",");
        assert!(parse(unknown.as_bytes()).is_err());
        let extra = document().replace("\"mode\":", "\"wat\":1,\"mode\":");
        assert!(parse(extra.as_bytes()).is_err());
    }

    #[test]
    fn pins_reject_a_different_profile_digest() {
        let policy = parse(document().as_bytes()).expect("parses");
        let mut profile = crate::context_budget::profile::generic_profile(12_000).unwrap();
        assert!(!policy.pins(&profile));
        profile.id = "local-12k".to_owned();
        profile.version = "1".to_owned();
        profile.digest =
            "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned();
        assert!(policy.pins(&profile));
    }

    #[test]
    fn evaluate_maps_the_three_denials() {
        let policy = parse(document().as_bytes()).expect("parses");
        assert_eq!(policy.evaluate(true, true, None), Some("over-budget"));
        assert_eq!(
            policy.evaluate(false, false, None),
            Some("required-incomplete")
        );
        assert_eq!(policy.evaluate(false, true, None), None);
    }
}
