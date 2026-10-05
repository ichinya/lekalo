use super::types::*;
use super::{Error, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const FIELDS: &[(&str, &str)] = &[
    ("durationMs", "/metrics/durationMs"),
    ("filesRead", "/metrics/filesRead"),
    ("filesChanged", "/metrics/filesChanged"),
    ("toolCalls", "/metrics/toolCalls"),
    ("retryCount", "/metrics/retryCount"),
    ("replanCount", "/metrics/replanCount"),
    ("inputTokens", "/metrics/tokens/input"),
    ("outputTokens", "/metrics/tokens/output"),
    ("reasoningTokens", "/metrics/tokens/reasoning"),
    ("totalTokens", "/metrics/tokens/total"),
    ("contextBytes", "/metrics/context/bytes"),
    ("contextTokens", "/metrics/context/estimatedTokens"),
    ("includedFacts", "/metrics/context/includedFacts"),
    ("candidateFacts", "/metrics/context/candidateFacts"),
];

fn state(v: Option<&Value>) -> &str {
    v.and_then(|v| v.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
}

fn unavailable<T>(values: &[&str]) -> ValueState<T> {
    if values.contains(&"withheld") {
        ValueState::Withheld
    } else if !values.is_empty() && values.iter().all(|v| *v == "unsupported") {
        ValueState::Unsupported
    } else {
        ValueState::Unknown
    }
}

fn stats(sources: &[&Source], pointer: &str, k: u64) -> Result<Statistic> {
    if sources.len() < k as usize {
        return Ok(Statistic {
            sample: ValueState::Withheld,
            sum: ValueState::Withheld,
            mean: ValueState::Withheld,
        });
    }
    let states: Vec<_> = sources
        .iter()
        .map(|s| state(s.record.pointer(pointer)))
        .collect();
    if states.iter().any(|v| *v != "known") {
        return Ok(Statistic {
            sample: unavailable(&states),
            sum: unavailable(&states),
            mean: unavailable(&states),
        });
    }
    let mut sum = 0u128;
    for source in sources {
        let v = source
            .record
            .pointer(pointer)
            .and_then(|v| v.get("value"))
            .and_then(Value::as_u64)
            .ok_or(Error::Invalid)?;
        sum = sum.checked_add(v as u128).ok_or(Error::Invalid)?;
    }
    Ok(Statistic {
        sample: ValueState::known(sources.len() as u64),
        sum: ValueState::known(sum.to_string()),
        mean: ValueState::known(Rational {
            numerator: sum.to_string(),
            denominator: sources.len().to_string(),
        }),
    })
}

fn version(sources: &[&Source], path: &str) -> ValueState<String> {
    if sources.len() < 5 {
        return ValueState::Withheld;
    }
    let values: Vec<_> = sources.iter().map(|s| s.record.pointer(path)).collect();
    if values.iter().any(|v| state(*v) != "known") {
        return unavailable(&values.iter().map(|v| state(*v)).collect::<Vec<_>>());
    }
    let tokens: BTreeSet<_> = values
        .iter()
        .filter_map(|v| v.and_then(|v| v.get("value")).and_then(Value::as_str))
        .collect();
    if tokens.len() == 1 {
        let token = *tokens.iter().next().unwrap();
        if token.split('.').count() == 3
            && token
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        {
            return ValueState::known(token.to_owned());
        }
    }
    ValueState::Unknown
}

fn alias(sources: &[&Source], path: &str, name: String) -> ValueState<String> {
    if sources.len() < 5 {
        return ValueState::Withheld;
    }
    let states: Vec<_> = sources
        .iter()
        .map(|s| state(s.record.pointer(path)))
        .collect();
    if states.iter().all(|s| *s == "known") {
        ValueState::known(name)
    } else {
        unavailable(&states)
    }
}

fn context_coverage(metrics: &BTreeMap<String, Statistic>) -> ValueState<Rational> {
    match (
        &metrics["includedFacts"].sum,
        &metrics["candidateFacts"].sum,
    ) {
        (ValueState::Known { value: n }, ValueState::Known { value: d }) if d != "0" => {
            ValueState::known(Rational {
                numerator: n.clone(),
                denominator: d.clone(),
            })
        }
        (ValueState::Withheld, _) | (_, ValueState::Withheld) => ValueState::Withheld,
        (ValueState::Unsupported, ValueState::Unsupported) => ValueState::Unsupported,
        _ => ValueState::Unknown,
    }
}

fn success(source: &Source) -> Option<bool> {
    if source.trial.required_assertions.is_empty() {
        return None;
    }
    let rows = source.assertions.as_ref()?.get("rows")?.as_array()?;
    let mut complete = true;
    for id in &source.trial.required_assertions {
        match rows
            .iter()
            .find(|r| r.get("assertionId").and_then(Value::as_str) == Some(id))
            .and_then(|r| r.get("outcome"))
            .and_then(Value::as_str)
        {
            Some("fail") => return Some(false),
            Some("pass") => (),
            _ => complete = false,
        }
    }
    match source
        .record
        .pointer("/status/outcome")
        .and_then(Value::as_str)
    {
        Some("fail") => return Some(false),
        Some("pass") => (),
        _ => complete = false,
    }
    if source
        .record
        .pointer("/status/coverageState")
        .and_then(Value::as_str)
        != Some("complete")
    {
        complete = false;
    }
    if complete {
        Some(true)
    } else {
        None
    }
}

fn distribution(
    sources: &[&Source],
    path: &str,
    keys: &[&str],
    k: u64,
) -> Result<ValueState<BTreeMap<String, u64>>> {
    if sources.len() < k as usize {
        return Ok(ValueState::Withheld);
    }
    let mut counts: BTreeMap<String, u64> = keys.iter().map(|key| (key.to_string(), 0)).collect();
    for source in sources {
        let key = source
            .record
            .pointer(path)
            .and_then(Value::as_str)
            .ok_or(Error::Invalid)?;
        *counts.get_mut(key).ok_or(Error::Invalid)? += 1;
    }
    // Suppress the whole distribution, including complementary totals.
    if counts.values().any(|v| *v > 0 && *v < k) {
        Ok(ValueState::Withheld)
    } else {
        Ok(ValueState::known(counts))
    }
}

fn cost(sources: &[&Source], k: u64, successes: &ValueState<u64>) -> Result<ValueState<Cost>> {
    if sources.len() < k as usize {
        return Ok(ValueState::Withheld);
    }
    let states: Vec<_> = sources
        .iter()
        .flat_map(|s| {
            ["amount", "currency", "basis"]
                .map(|key| state(s.record.pointer(&format!("/metrics/cost/{key}"))))
        })
        .collect();
    if states.iter().any(|v| *v != "known") {
        return Ok(unavailable(&states));
    }
    let mut currency = None;
    let mut basis = None;
    let mut total = 0u128;
    // Micro-units, exactly represented: reject rather than round extra precision.
    for source in sources {
        let get = |key: &str| {
            source
                .record
                .pointer(&format!("/metrics/cost/{key}/value"))
                .and_then(Value::as_str)
                .ok_or(Error::Invalid)
        };
        let c = get("currency")?;
        let b = get("basis")?;
        if currency.is_some_and(|v| v != c) || basis.is_some_and(|v| v != b) {
            return Ok(ValueState::Unknown);
        }
        if !crate::run_history::validate::is_currency(c) || !["reported", "estimated"].contains(&b)
        {
            return Err(Error::Invalid);
        }
        currency = Some(c);
        basis = Some(b);
        let amount = get("amount")?;
        if !crate::run_history::validate::is_decimal(amount) {
            return Err(Error::Invalid);
        }
        let (whole, fraction) = amount.split_once('.').unwrap_or((amount, ""));
        if fraction.len() > 6 {
            return Err(Error::Invalid);
        }
        let scaled = whole
            .parse::<u128>()
            .map_err(|_| Error::Invalid)?
            .checked_mul(1_000_000)
            .ok_or(Error::Invalid)?;
        let fraction = format!("{fraction:0<6}")
            .parse::<u128>()
            .map_err(|_| Error::Invalid)?;
        total = total
            .checked_add(scaled.checked_add(fraction).ok_or(Error::Invalid)?)
            .ok_or(Error::Invalid)?;
    }
    let per_success = match successes {
        ValueState::Known { value } if *value > 0 => ValueState::known(Rational {
            numerator: total.to_string(),
            denominator: ((*value as u128) * 1_000_000).to_string(),
        }),
        ValueState::Withheld => ValueState::Withheld,
        _ => ValueState::Unknown,
    };
    Ok(ValueState::known(Cost {
        currency: currency.unwrap().to_owned(),
        basis: basis.unwrap().to_owned(),
        amount: format!("{}.{:06}", total / 1_000_000, total % 1_000_000),
        per_success,
    }))
}

pub(crate) fn aggregate(sources: &[Source], definition_ref: ExactRef) -> Result<PublicMetrics> {
    let k = 5;
    // Private provenance keys determine grouping and aliases; none is serialized.
    let mut groups: BTreeMap<(Arm, String, String, String), Vec<&Source>> = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for source in sources {
        let r = &source.record;
        let text = |path: &str| {
            r.pointer(path)
                .and_then(Value::as_str)
                .ok_or(Error::Invalid)
                .map(str::to_owned)
        };
        let public_pins = serde_json::json!({"core":r.pointer("/provenance/core/version"),"profile":r.pointer("/provenance/profile"),"harness":r.pointer("/provenance/harness"),"adapters":r.pointer("/provenance/adapters")});
        let identity = super::canon(&public_pins);
        identities.insert(identity.clone());
        groups
            .entry((
                source.trial.arm,
                text("/pilot/mode")?,
                text("/pilot/scopeState")?,
                identity,
            ))
            .or_default()
            .push(source);
    }
    if groups.len() > 64 {
        return Err(Error::Invalid);
    }
    let aliases: BTreeMap<_, _> = identities
        .into_iter()
        .enumerate()
        .map(|(i, key)| (key, i + 1))
        .collect();
    let mut cohorts = Vec::new();
    let mut pairing = BTreeMap::<(String, String, String), [Option<usize>; 2]>::new();
    for ((arm, pilot, scope_state, pins), runs) in groups {
        let i = cohorts.len();
        let alias = aliases[&pins];
        let mut metrics = BTreeMap::new();
        for (field, pointer) in FIELDS {
            metrics.insert((*field).to_owned(), stats(&runs, pointer, k)?);
        }
        for field in [
            "cachedTokens",
            "fixCycles",
            "firstPassSuccess",
            "humanInterventions",
        ] {
            let missing = if runs.len() < k as usize {
                ValueState::Withheld
            } else {
                ValueState::Unknown
            };
            metrics.insert(
                field.to_owned(),
                Statistic {
                    sample: missing.clone(),
                    sum: if runs.len() < k as usize {
                        ValueState::Withheld
                    } else {
                        ValueState::Unknown
                    },
                    mean: if runs.len() < k as usize {
                        ValueState::Withheld
                    } else {
                        ValueState::Unknown
                    },
                },
            );
        }
        let mut outcomes = distribution(
            &runs,
            "/status/outcome",
            &["pass", "warn", "fail", "unsupported", "infrastructure"],
            k,
        )?;
        let mut coverage = distribution(
            &runs,
            "/status/coverageState",
            &["complete", "incomplete", "unknown"],
            k,
        )?;
        let observations: Vec<_> = runs.iter().map(|s| success(s)).collect();
        let successes = observations.iter().filter(|v| **v == Some(true)).count() as u64;
        let mut verified_success = if runs.len() < k as usize {
            ValueState::Withheld
        } else if observations.contains(&None) {
            ValueState::Unknown
        } else if (successes > 0 && successes < k)
            || (runs.len() as u64 - successes > 0 && runs.len() as u64 - successes < k)
        {
            ValueState::Withheld
        } else {
            ValueState::known(successes)
        };
        let mut success_rate = match &verified_success {
            ValueState::Known { value } => ValueState::known(Rational {
                numerator: value.to_string(),
                denominator: runs.len().to_string(),
            }),
            ValueState::Withheld => ValueState::Withheld,
            _ => ValueState::Unknown,
        };
        let role = runs[0]
            .record
            .pointer("/scope/repositoryRole")
            .and_then(Value::as_str)
            .ok_or(Error::Invalid)?
            .to_owned();
        let suppress = runs.len() < k as usize
            || matches!(outcomes, ValueState::Withheld)
            || matches!(coverage, ValueState::Withheld)
            || matches!(verified_success, ValueState::Withheld);
        let sample = if suppress {
            ValueState::Withheld
        } else {
            ValueState::known(runs.len() as u64)
        };
        let c = if suppress {
            ValueState::Withheld
        } else {
            cost(&runs, k, &verified_success)?
        };
        if suppress {
            outcomes = ValueState::Withheld;
            coverage = ValueState::Withheld;
            verified_success = ValueState::Withheld;
            success_rate = ValueState::Withheld;
            for statistic in metrics.values_mut() {
                statistic.sample = ValueState::Withheld;
                statistic.sum = ValueState::Withheld;
                statistic.mean = ValueState::Withheld;
            }
        }
        let context_coverage = context_coverage(&metrics);
        let row = Cohort {
            arm,
            pilot: pilot.clone(),
            scope_state: scope_state.clone(),
            repository_role: role,
            repository_alias: "consumer-01".to_owned(),
            profile_alias: self::alias(
                &runs,
                "/provenance/profile/id",
                format!("profile-{alias:02}"),
            ),
            model_alias: self::alias(
                &runs,
                "/provenance/harness/modelId",
                format!("model-{alias:02}"),
            ),
            harness_alias: self::alias(
                &runs,
                "/provenance/harness/id",
                format!("harness-{alias:02}"),
            ),
            stack: ValueState::Unknown,
            context_coverage,
            core_version: version(&runs, "/provenance/core/version"),
            profile_version: version(&runs, "/provenance/profile/version"),
            harness_version: version(&runs, "/provenance/harness/version"),
            sample,
            outcomes,
            coverage,
            metrics,
            verified_success,
            success_rate,
            cost: c,
            confidence: ValueState::Unknown,
        };
        pairing
            .entry((pilot, scope_state, pins))
            .or_insert([None, None])[if arm == Arm::Baseline { 0 } else { 1 }] = Some(i);
        cohorts.push(row);
    }
    let mut comparisons = Vec::new();
    for indices in pairing.values() {
        if let [Some(a), Some(b)] = indices {
            let complete = sources.iter().all(|s| {
                [
                    "/provenance/git/commit",
                    "/provenance/git/dirty",
                    "/provenance/git/workingSetDigest",
                    "/provenance/harness/id",
                    "/provenance/harness/version",
                    "/provenance/harness/modelId",
                    "/provenance/harness/modelRevision",
                    "/provenance/profile/id",
                    "/provenance/profile/version",
                    "/provenance/profile/digest",
                ]
                .iter()
                .all(|pointer| state(s.record.pointer(pointer)) == "known")
            });
            let lift = if !complete {
                ValueState::Unknown
            } else {
                match (&cohorts[*a].success_rate, &cohorts[*b].success_rate) {
                    (ValueState::Known { value: a }, ValueState::Known { value: b }) => {
                        let an = a.numerator.parse::<i128>().map_err(|_| Error::Invalid)?;
                        let ad = a.denominator.parse::<i128>().map_err(|_| Error::Invalid)?;
                        let bn = b.numerator.parse::<i128>().map_err(|_| Error::Invalid)?;
                        let bd = b.denominator.parse::<i128>().map_err(|_| Error::Invalid)?;
                        ValueState::known(Rational {
                            numerator: (bn * ad - an * bd).to_string(),
                            denominator: (ad * bd).to_string(),
                        })
                    }
                    (ValueState::Withheld, _) | (_, ValueState::Withheld) => ValueState::Withheld,
                    _ => ValueState::Unknown,
                }
            };
            comparisons.push(Comparison {
                baseline_cohort: *a as u64,
                assisted_cohort: *b as u64,
                success_lift: lift,
            });
        }
    }
    Ok(PublicMetrics {
        schema_version: "lekalo/public-metrics/v0.6.4",
        identity: "dev.lekalo.public-metrics@0.6.4",
        artifact_kind: "aggregate.artifact",
        policy_ref: serde_json::to_value(crate::privacy::types::PolicyRef::frozen()).unwrap(),
        authority_ref:
            serde_json::to_value(crate::privacy::types::AuthorityRef::frozen_authority()).unwrap(),
        definition_ref,
        evaluation_protocol: "framework-lift-paired-trials/1",
        cohorts,
        comparisons,
    })
}
