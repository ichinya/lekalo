//! Offline, harness-neutral recorded evaluation admission. Never launches an agent.
//! Committed closed schemas are also checked by exact Draft 2020-12 Ajv gates.
mod schema;
use crate::diagnostics::{
    normalize::build,
    types::{token_value, DataObject},
    DiagnosticSet,
};
use crate::{DomainResult, Status};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const FAMILIES: &[&str] = &["baseline", "task", "campaign", "arm", "result"];
pub type Failure = (&'static str, &'static str);
pub fn failure((id, detail): Failure) -> DomainResult {
    let data = DataObject::from([("detail".into(), token_value(detail))]);
    let set = build(id, None, None, data)
        .ok()
        .and_then(|d| DiagnosticSet::try_from_unsorted(vec![d], Status::Invalid).ok());
    DomainResult::invalid(
        set.unwrap_or_else(|| crate::result::singleton_set("diagnostics.registry-invalid")),
    )
}
pub fn canonical(value: &Value) -> String {
    format!("{}\n", serde_json::to_string(value).expect("JSON value"))
}
pub fn digest(value: &Value) -> String {
    format!(
        "sha256:{}",
        crate::digest::sha256_hex(canonical(value).as_bytes())
    )
}
pub fn content_digest(value: &Value) -> String {
    let mut body = value.clone();
    body.as_object_mut()
        .expect("validated object")
        .remove("approval");
    digest(&body)
}
pub fn parse(family: &str, bytes: &[u8]) -> Result<Value, Failure> {
    if bytes.len() > MAX_BYTES {
        return Err(("evaluation.protocol-invalid", "input-size"));
    }
    let value = schema::decode(bytes).map_err(|_| ("evaluation.protocol-invalid", "json-shape"))?;
    schema::validate(family, &value)
        .map_err(|_| ("evaluation.protocol-invalid", "closed-contract"))?;
    match family {
        "baseline" | "campaign" => {
            if value["approval"]["contentDigest"] != content_digest(&value) {
                return Err(("evaluation.baseline-drift", "approval-digest"));
            }
            if family == "baseline" {
                let mut paths = BTreeSet::new();
                for file in arr(&value, "files") {
                    if !safe_path(s(file, "path"))
                        || !paths.insert(s(file, "path").to_ascii_lowercase())
                    {
                        return Err(("evaluation.protocol-invalid", "file-path"));
                    }
                }
            } else {
                validate_schedule(&value)?;
            }
        }
        "task" => {
            let mut ids = BTreeSet::new();
            for key in [
                "requiredAssertions",
                "regressionAssertions",
                "holdoutAssertions",
            ] {
                for id in arr(&value, key) {
                    if !ids.insert(id.as_str().unwrap()) {
                        return Err(("evaluation.protocol-invalid", "assertion-duplicate"));
                    }
                }
            }
        }
        "arm" => {
            validate_arm_shape(&value)?;
        }
        "result" => {
            validate_result(&value)?;
        }
        _ => return Err(("evaluation.protocol-invalid", "family")),
    }
    Ok(value)
}
pub fn safe_path(path: &str) -> bool {
    path.split('/').all(|p| {
        !p.is_empty()
            && p != "."
            && p != ".."
            && !p.ends_with('.')
            && !matches!(
                p.split('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase()
                    .as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            )
    })
}
fn arr<'a>(v: &'a Value, key: &str) -> &'a Vec<Value> {
    v[key].as_array().expect("schema array")
}
fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().expect("schema string")
}
fn n(v: &Value, key: &str) -> u64 {
    v[key].as_u64().expect("schema count")
}
fn known(v: &Value) -> Option<u64> {
    if v["state"] == "known" {
        v["value"].as_u64()
    } else {
        None
    }
}
// Input/output are disjoint under framework-lift-metrics-1. Cache and reasoning
// only lower-bound their respective parent when that parent is unavailable.
fn token_lower_bound(m: &Value) -> u64 {
    known(&m["inputTokens"])
        .unwrap_or(0)
        .max(known(&m["cachedInputTokens"]).unwrap_or(0))
        + known(&m["outputTokens"])
            .unwrap_or(0)
            .max(known(&m["reasoningTokens"]).unwrap_or(0))
}
fn validate_token_totals(m: &Value) -> Result<(), Failure> {
    if let Some(total) = known(&m["totalTokens"]) {
        if total < token_lower_bound(m)
            || matches!(
                (known(&m["inputTokens"]), known(&m["outputTokens"])),
                (Some(input), Some(output)) if total != input + output
            )
        {
            return Err(("evaluation.metric-inconsistent", "token-total"));
        }
    }
    Ok(())
}
fn state(v: Option<u64>) -> Value {
    v.map_or_else(
        || json!({"state":"unknown"}),
        |v| json!({"state":"known","value":v}),
    )
}
fn validate_schedule(c: &Value) -> Result<(), Failure> {
    let mut pairs = std::collections::BTreeMap::<String, (u64, BTreeSet<String>)>::new();
    for slot in arr(c, "slots") {
        let entry = pairs
            .entry(s(slot, "pairId").into())
            .or_insert((n(slot, "repetition"), BTreeSet::new()));
        if entry.0 != n(slot, "repetition") || !entry.1.insert(s(slot, "arm").into()) {
            return Err(("evaluation.protocol-invalid", "schedule-duplicate"));
        }
    }
    if pairs.len() < 2 || pairs.values().any(|(_, arms)| arms.len() != 2) {
        return Err(("evaluation.protocol-invalid", "schedule-unpaired"));
    }
    let p = &c["profile"];
    if n(&p["sampling"], "temperatureMicros") > 2_000_000
        || n(&p["sampling"], "topPMicros") > 1_000_000
        || n(&p["sampling"], "maxOutputTokens") > n(&p["limits"], "maxTokens")
    {
        return Err(("evaluation.protocol-invalid", "sampling-budget"));
    }
    if n(&p["limits"], "maxProviderRetries") > 16 || n(&p["limits"], "maxFixCycles") > 100 {
        return Err(("evaluation.protocol-invalid", "retry-limit"));
    }
    Ok(())
}
fn validate_arm_shape(a: &Value) -> Result<(), Failure> {
    let mut ids = BTreeSet::new();
    for assertion in arr(a, "assertions") {
        if !ids.insert(s(assertion, "id")) {
            return Err(("evaluation.protocol-invalid", "assertion-duplicate"));
        }
    }
    let mut sources = BTreeSet::new();
    for source in arr(a, "measurementSources") {
        if !sources.insert(s(source, "metric")) {
            return Err(("evaluation.metric-inconsistent", "source-duplicate"));
        }
    }
    for (metric, value) in a["metrics"].as_object().unwrap() {
        if (value["state"] == "known") != sources.contains(metric.as_str()) {
            return Err(("evaluation.metric-inconsistent", "source-coverage"));
        }
    }
    let m = &a["metrics"];
    validate_token_totals(m)?;
    for (part, total) in [
        ("cachedInputTokens", "inputTokens"),
        ("reasoningTokens", "outputTokens"),
        ("includedFacts", "candidateFacts"),
        ("includedRequiredFacts", "requiredFacts"),
        ("unrelatedFiles", "filesChanged"),
    ] {
        if let (Some(part), Some(total)) = (known(&m[part]), known(&m[total])) {
            if part > total {
                return Err(("evaluation.metric-inconsistent", "subset-count"));
            }
        }
    }
    for (i, event) in arr(a, "events").iter().enumerate() {
        if n(event, "sequence") != i as u64 {
            return Err(("evaluation.metric-inconsistent", "event-order"));
        }
    }
    Ok(())
}
fn matches_ref(reference: &Value, id: &str, doc: &Value) -> bool {
    reference["id"] == doc[id] && reference["digest"] == digest(doc)
}
pub fn preflight(b: &Value, t: &Value, c: &Value) -> Result<(), Failure> {
    if !matches_ref(&t["baselineRef"], "baselineId", b)
        || !matches_ref(&c["baselineRef"], "baselineId", b)
        || !matches_ref(&c["taskRef"], "taskId", t)
        || b["pilot"] != t["pilot"]
        || b["oracleDigest"] != t["oracleDigest"]
        || t["limits"] != c["profile"]["limits"]
    {
        return Err(("evaluation.arm-incomparable", "shared-pins"));
    }
    if t["pilot"] == "brownfield-observed" && c["profile"]["network"] != "local-only" {
        return Err(("evaluation.private-egress-denied", "private-provider"));
    }
    Ok(())
}
pub fn admit_arm(b: &Value, t: &Value, c: &Value, a: &Value) -> Result<(), Failure> {
    preflight(b, t, c)?;
    if !matches_ref(&a["campaignRef"], "campaignId", c)
        || !matches_ref(&a["taskRef"], "taskId", t)
        || !matches_ref(&a["baselineRef"], "baselineId", b)
        || a["profileDigest"] != digest(&c["profile"])
        || a["applicationDigest"] != content_digest(b)
        || !arr(c, "slots").contains(&a["slot"])
    {
        return Err(("evaluation.arm-incomparable", "arm-pins"));
    }
    let assisted = a["slot"]["arm"] == "B";
    if a["policy"]["lekalo"] != assisted
        || a["policy"]["semanticExposure"] != assisted
        || a["policy"]["network"] != c["profile"]["network"]
        || (!assisted && arr(a, "events").iter().any(|e| e["tool"] == "lekalo"))
    {
        return Err(("evaluation.arm-incomparable", "treatment-policy"));
    }
    if n(a, "attempt") > n(&c["profile"]["limits"], "maxProviderRetries") {
        return Err(("evaluation.protocol-invalid", "attempt-limit"));
    }
    for assertion in arr(a, "assertions") {
        if assertion["candidateDigest"] != a["candidateDigest"]
            || assertion["oracleDigest"] != t["oracleDigest"]
        {
            return Err(("evaluation.required-evidence-missing", "stale-assertion"));
        }
        if ![
            "requiredAssertions",
            "regressionAssertions",
            "holdoutAssertions",
        ]
        .iter()
        .any(|k| arr(t, k).contains(&assertion["id"]))
        {
            return Err((
                "evaluation.required-evidence-missing",
                "unapproved-assertion",
            ));
        }
    }
    let events = arr(a, "events");
    let metrics = &a["metrics"];
    for (metric, kind) in [
        ("toolCalls", "tool-call"),
        ("replanCount", "replan"),
        ("fixCycles", "fix"),
        ("humanInterventions", "human"),
    ] {
        let count = events.iter().filter(|e| e["kind"] == kind).count() as u64;
        if let Some(observed) = known(&metrics[metric]) {
            if observed != count {
                return Err(("evaluation.metric-inconsistent", "event-count"));
            }
        }
    }
    Ok(())
}
fn acceptance(t: &Value, a: &Value) -> (&'static str, bool, bool) {
    let f = arr(a, "failures");
    if f.iter().any(|e| e["class"] == "custody-security") {
        return ("custody-security", false, false);
    }
    if token_lower_bound(&a["metrics"]) > n(&t["limits"], "maxTokens") {
        return ("task", false, false);
    }
    for (metric, limit) in [
        ("totalTokens", "maxTokens"),
        ("toolCalls", "maxToolCalls"),
        ("durationMs", "deadlineMs"),
        ("fixCycles", "maxFixCycles"),
    ] {
        if let Some(value) = known(&a["metrics"][metric]) {
            if value > n(&t["limits"], limit) {
                return ("task", false, false);
            }
        }
    }
    if arr(a, "assertions").iter().any(|e| e["outcome"] == "fail")
        || known(&a["metrics"]["escapedRegressions"]).is_some_and(|v| v > 0)
        || f.iter().any(|e| e["class"] == "task")
    {
        return ("task", false, false);
    }
    for class in ["provider", "infrastructure", "unsupported", "interrupted"] {
        if f.iter().any(|e| e["class"] == class) {
            return (class, false, false);
        }
    }
    for metric in ["totalTokens", "toolCalls", "durationMs", "fixCycles"] {
        if known(&a["metrics"][metric]).is_none() {
            return ("unsupported", false, false);
        }
    }
    let required = [
        "requiredAssertions",
        "regressionAssertions",
        "holdoutAssertions",
    ]
    .iter()
    .flat_map(|key| arr(t, key));
    if a["coverage"] != "complete"
        || known(&a["metrics"]["escapedRegressions"]) != Some(0)
        || required.into_iter().any(|id| {
            !arr(a, "assertions")
                .iter()
                .any(|e| e["id"] == *id && e["outcome"] == "pass")
        })
    {
        return ("unsupported", false, false);
    }
    let first = known(&a["metrics"]["fixCycles"]) == Some(0)
        && known(&a["metrics"]["humanInterventions"]) == Some(0)
        && arr(a, "events")
            .iter()
            .filter(|e| e["kind"] == "submit")
            .count()
            == 1;
    ("success", true, first)
}
fn interval(success: u64, total: u64) -> Value {
    let z = 1.959963984540054;
    let n = total as f64;
    let p = success as f64 / n;
    let d = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / d;
    let radius = z * ((p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt()) / d;
    json!({"method":"wilson-95","unit":"parts-per-million","lower":((center-radius).max(0.0)*1_000_000.0).floor() as u64,"upper":((center+radius).min(1.0)*1_000_000.0).ceil() as u64})
}
fn lift(numerator: i64, denominator: u64) -> Value {
    json!({"numerator":100*numerator,"denominator":denominator})
}
fn validate_result(r: &Value) -> Result<(), Failure> {
    let invalid = ("evaluation.metric-inconsistent", "result-arithmetic");
    let mut keys = BTreeSet::new();
    let mut attempts = std::collections::BTreeMap::<(&str, &str), Vec<&Value>>::new();
    let mut external = false;
    for row in arr(r, "rows") {
        validate_token_totals(&row["metrics"])?;
        if !keys.insert((
            s(&row["slot"], "pairId"),
            s(&row["slot"], "arm"),
            n(row, "attempt"),
        )) || (row["verifiedSuccess"] == true) != (row["status"] == "success")
            || (row["firstPassSuccess"] == true && row["verifiedSuccess"] != true)
        {
            return Err(invalid);
        }
        let pending = row["status"] == "not-started";
        if pending {
            if n(row, "attempt") != 0
                || row["armDigest"] != json!({"state":"unknown"})
                || row["origin"] != json!({"state":"unknown"})
                || row["metrics"] != schema::unknown_metrics()
            {
                return Err(invalid);
            }
        } else if row["armDigest"]["state"] != "known" || row["origin"]["state"] != "known" {
            return Err(invalid);
        }
        external |= row["origin"]["value"] == "recorded-external";
        if row["verifiedSuccess"] == true
            && (known(&row["metrics"]["escapedRegressions"]) != Some(0)
                || ["totalTokens", "toolCalls", "durationMs", "fixCycles"]
                    .iter()
                    .any(|m| known(&row["metrics"][*m]).is_none()))
        {
            return Err(invalid);
        }
        attempts
            .entry((s(&row["slot"], "pairId"), s(&row["slot"], "arm")))
            .or_default()
            .push(row);
    }
    if r["evidenceStatus"]
        != if external {
            "recorded-unverified"
        } else {
            "recorded-simulation"
        }
    {
        return Err(invalid);
    }
    for rows in attempts.values_mut() {
        rows.sort_by_key(|row| n(row, "attempt"));
        if rows
            .iter()
            .enumerate()
            .any(|(i, row)| n(row, "attempt") != i as u64)
            || rows
                .windows(2)
                .any(|pair| pair[0]["status"] != "provider" || pair[0]["slot"] != pair[1]["slot"])
        {
            return Err(invalid);
        }
    }
    for (i, arm) in ["A", "B"].iter().enumerate() {
        let summary = &arr(r, "summaries")[i];
        if summary["arm"] != *arm {
            return Err(invalid);
        }
        let rows: Vec<_> = arr(r, "rows")
            .iter()
            .filter(|row| row["slot"]["arm"] == *arm)
            .collect();
        let slots: BTreeSet<_> = rows.iter().map(|row| s(&row["slot"], "pairId")).collect();
        let successes = slots
            .iter()
            .filter(|id| {
                rows.iter()
                    .any(|row| s(&row["slot"], "pairId") == **id && row["verifiedSuccess"] == true)
            })
            .count() as u64;
        if slots.len() as u64 != n(summary, "scheduled")
            || successes != n(summary, "successes")
            || summary["successRate"] != json!({"numerator": successes, "denominator": slots.len()})
        {
            return Err(invalid);
        }
        if summary["successInterval"] != interval(successes, slots.len() as u64) {
            return Err(invalid);
        }
        let started: BTreeSet<_> = rows
            .iter()
            .filter(|row| row["status"] != "not-started")
            .map(|row| s(&row["slot"], "pairId"))
            .collect();
        let actual: Vec<_> = rows
            .iter()
            .filter(|row| row["status"] != "not-started")
            .collect();
        let costs: Vec<_> = actual
            .iter()
            .filter_map(|row| known(&row["metrics"]["costMicros"]))
            .collect();
        let total = costs
            .iter()
            .try_fold(0u64, |sum, x| sum.checked_add(*x))
            .ok_or(invalid)?;
        let complete = costs.len() == actual.len();
        if n(summary, "started") != started.len() as u64
            || n(summary, "attempts") != actual.len() as u64
            || n(summary, "costKnownRuns") != costs.len() as u64
            || summary["totalCostMicros"] != state(complete.then_some(total))
        {
            return Err(invalid);
        }
        let reason = if successes == 0 {
            "no-success"
        } else if !complete {
            "cost-incomplete"
        } else {
            "complete"
        };
        let value = if reason == "complete" {
            json!({"state":"known","value":{"numerator":total,"denominator":successes}})
        } else {
            json!({"state":"unknown"})
        };
        if summary["costPerSuccess"]
            != json!({"state":if reason=="complete"{"known"}else{"unknown"},"reason":reason,"value":value})
        {
            return Err(invalid);
        }
        let first = slots
            .iter()
            .filter(|id| {
                rows.iter()
                    .any(|row| s(&row["slot"], "pairId") == **id && row["firstPassSuccess"] == true)
            })
            .count() as u64;
        if n(summary, "firstPassSuccesses") != first {
            return Err(invalid);
        }
    }
    let mut outcomes = std::collections::BTreeMap::new();
    for row in arr(r, "rows") {
        let key = (
            s(&row["slot"], "pairId").to_owned(),
            s(&row["slot"], "arm").to_owned(),
        );
        let entry = outcomes.entry(key).or_insert((false, false));
        entry.0 |= row["verifiedSuccess"] == true;
        entry.1 |= matches!(s(row, "status"), "success" | "task");
        if row["firstPassSuccess"] == true
            && (n(row, "attempt") != 0
                || known(&row["metrics"]["fixCycles"]) != Some(0)
                || known(&row["metrics"]["humanInterventions"]) != Some(0))
        {
            return Err(invalid);
        }
    }
    let mut paired = [0u64; 5];
    let mut missing = [0u64; 2];
    for ((pair, arm), (a, complete_a)) in &outcomes {
        if arm != "A" {
            continue;
        }
        let (b, complete_b) = outcomes
            .get(&(pair.clone(), "B".into()))
            .copied()
            .ok_or(invalid)?;
        missing[0] += u64::from(!complete_a);
        missing[1] += u64::from(!complete_b);
        let i = if !complete_a || !complete_b {
            4
        } else {
            match (*a, b) {
                (true, true) => 0,
                (true, false) => 1,
                (false, true) => 2,
                (false, false) => 3,
            }
        };
        paired[i] += 1;
    }
    if outcomes.len() != paired.iter().sum::<u64>() as usize * 2
        || r["paired"]
            != json!({"bothSuccess":paired[0],"aOnly":paired[1],"bOnly":paired[2],"neither":paired[3],"incomplete":paired[4]})
    {
        return Err(invalid);
    }
    let summaries = arr(r, "summaries");
    let total = n(&summaries[0], "scheduled");
    if total != n(&summaries[1], "scheduled") {
        return Err(invalid);
    }
    let difference = n(&summaries[1], "successes") as i64 - n(&summaries[0], "successes") as i64;
    for (value, expected) in [
        (&r["liftPercentagePoints"], lift(difference, total)),
        (
            &r["uncertainty"]["minimumLift"],
            lift(difference - missing[0] as i64, total),
        ),
        (
            &r["uncertainty"]["maximumLift"],
            lift(difference + missing[1] as i64, total),
        ),
    ] {
        if *value != expected {
            return Err(invalid);
        }
    }
    Ok(())
}
pub fn compare(
    b: &Value,
    t: &Value,
    c: &Value,
    arms: &[Value],
    alias: &str,
) -> Result<Value, Failure> {
    preflight(b, t, c)?;
    if !schema::token(alias) {
        return Err(("evaluation.protocol-invalid", "alias"));
    }
    let mut seen = BTreeSet::new();
    if arms.len() > 1024 {
        return Err(("evaluation.protocol-invalid", "arm-limit"));
    }
    for a in arms {
        admit_arm(b, t, c, a)?;
        if !seen.insert(s(a, "runId").to_owned()) {
            return Err(("evaluation.protocol-invalid", "duplicate-run"));
        }
    }
    let mut rows = Vec::new();
    for slot in arr(c, "slots") {
        let mut attempts: Vec<_> = arms.iter().filter(|a| a["slot"] == *slot).collect();
        attempts.sort_by_key(|a| n(a, "attempt"));
        if attempts
            .iter()
            .enumerate()
            .any(|(i, a)| n(a, "attempt") != i as u64)
        {
            return Err(("evaluation.protocol-invalid", "attempt-gap"));
        }
        if attempts
            .windows(2)
            .any(|pair| acceptance(t, pair[0]).0 != "provider")
        {
            return Err(("evaluation.protocol-invalid", "retry-after-terminal"));
        }
        if attempts.is_empty() {
            rows.push(json!({"slot":slot,"attempt":0,"armDigest":{"state":"unknown"},"origin":{"state":"unknown"},"status":"not-started","verifiedSuccess":false,"firstPassSuccess":false,"metrics":schema::unknown_metrics()}));
        }
        for a in attempts {
            let (status, pass, first) = acceptance(t, a);
            rows.push(json!({"slot":slot,"attempt":a["attempt"],"armDigest":{"state":"known","value":digest(a)},"origin":{"state":"known","value":a["origin"]},"status":status,"verifiedSuccess":pass,"firstPassSuccess":first&&n(a,"attempt")==0,"metrics":a["metrics"]}));
        }
    }
    let mut summaries = Vec::new();
    let mut slot_outcomes = std::collections::BTreeMap::new();
    for arm in ["A", "B"] {
        let slots: Vec<_> = arr(c, "slots")
            .iter()
            .filter(|slot| slot["arm"] == arm)
            .collect();
        let arm_rows: Vec<_> = rows
            .iter()
            .filter(|row| row["slot"]["arm"] == arm)
            .collect();
        let mut success = 0;
        let mut first = 0;
        let mut started = 0;
        for slot in &slots {
            let related: Vec<_> = arm_rows
                .iter()
                .filter(|row| row["slot"] == **slot)
                .collect();
            let pass = related.iter().any(|row| row["verifiedSuccess"] == true);
            let begun = related.iter().any(|row| row["status"] != "not-started");
            success += u64::from(pass);
            started += u64::from(begun);
            first += u64::from(related.iter().any(|row| row["firstPassSuccess"] == true));
            slot_outcomes.insert(
                (s(slot, "pairId").to_owned(), arm.to_owned()),
                (
                    pass,
                    pass || related.last().is_some_and(|row| s(row, "status") == "task"),
                ),
            );
        }
        let actual: Vec<_> = arm_rows
            .into_iter()
            .filter(|row| row["status"] != "not-started")
            .collect();
        let costs: Vec<_> = actual
            .iter()
            .filter_map(|row| known(&row["metrics"]["costMicros"]))
            .collect();
        let total = costs
            .iter()
            .try_fold(0u64, |sum, x| sum.checked_add(*x))
            .filter(|v| *v <= 9_007_199_254_740_991)
            .ok_or(("evaluation.metric-inconsistent", "cost-overflow"))?;
        let complete = costs.len() == actual.len();
        let reason = if success == 0 {
            "no-success"
        } else if !complete {
            "cost-incomplete"
        } else {
            "complete"
        };
        let ratio = if reason == "complete" {
            json!({"state":"known","value":{"numerator":total,"denominator":success}})
        } else {
            json!({"state":"unknown"})
        };
        summaries.push(json!({"arm":arm,"scheduled":slots.len(),"started":started,"attempts":actual.len(),"successes":success,"firstPassSuccesses":first,"costKnownRuns":costs.len(),"totalCostMicros":state(complete.then_some(total)),"costPerSuccess":{"state":if reason=="complete"{"known"}else{"unknown"},"reason":reason,"value":ratio},"successRate":{"numerator":success,"denominator":slots.len()},"successInterval":interval(success,slots.len() as u64)}));
    }
    let mut paired = [0u64; 5];
    for ((pair, arm), (a, complete_a)) in &slot_outcomes {
        if arm != "A" {
            continue;
        }
        let (b, complete_b) = slot_outcomes[&(pair.clone(), "B".into())];
        let idx = if !complete_a || !complete_b {
            4
        } else {
            match (*a, b) {
                (true, true) => 0,
                (true, false) => 1,
                (false, true) => 2,
                (false, false) => 3,
            }
        };
        paired[idx] += 1;
    }
    let incomplete = |arm: &str| {
        slot_outcomes
            .iter()
            .filter(|((_, a), (_, complete))| a == arm && !*complete)
            .count() as i64
    };
    let denom = n(&summaries[0], "scheduled");
    let difference = n(&summaries[1], "successes") as i64 - n(&summaries[0], "successes") as i64;
    let result = json!({"schemaVersion":"lekalo/framework-lift-result/v0.6.4","identity":"dev.lekalo.framework-lift-result@0.6.4","campaignRef":{"id":c["campaignId"],"digest":digest(c)},"taskRef":{"id":t["taskId"],"digest":digest(t)},"baselineRef":{"id":b["baselineId"],"digest":digest(b)},"profileDigest":digest(&c["profile"]),"evidenceStatus":if arms.iter().all(|a|a["origin"]=="recorded-simulation"){"recorded-simulation"}else{"recorded-unverified"},"exportDisposition":"local-private","consumerRole":"consumer-repository","consumerAlias":alias,"pilot":t["pilot"],"analysis":c["analysis"],"rows":rows,"summaries":summaries,"paired":{"bothSuccess":paired[0],"aOnly":paired[1],"bOnly":paired[2],"neither":paired[3],"incomplete":paired[4]},"liftPercentagePoints":lift(difference,denom),"uncertainty":{"method":"attrition-bounds","minimumLift":lift(difference-incomplete("A"),denom),"maximumLift":lift(difference+incomplete("B"),denom)},"claim":"tested-task-profile-only"});
    schema::validate("result", &result)
        .map_err(|_| ("evaluation.metric-inconsistent", "result-bounds"))?;
    validate_result(&result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_parents_use_disjoint_subset_bounds_without_double_counting() {
        let mut m = schema::unknown_metrics();
        m["cachedInputTokens"] = state(Some(6_000));
        m["reasoningTokens"] = state(Some(6_000));
        assert_eq!(token_lower_bound(&m), 12_000);
        m["inputTokens"] = state(Some(6_000));
        m["outputTokens"] = state(Some(6_000));
        assert_eq!(token_lower_bound(&m), 12_000);
        m["totalTokens"] = state(Some(12_000));
        assert_eq!(validate_token_totals(&m), Ok(()));
        m["totalTokens"] = state(Some(12_001));
        assert_eq!(
            validate_token_totals(&m),
            Err(("evaluation.metric-inconsistent", "token-total"))
        );
        m["totalTokens"] = json!({"state":"unsupported"});
        assert_eq!(validate_token_totals(&m), Ok(()));
    }

    #[test]
    fn summing_maximum_admitted_components_does_not_overflow_or_hide_excess() {
        let maximum = 9_007_199_254_740_991;
        let mut m = schema::unknown_metrics();
        m["inputTokens"] = state(Some(maximum));
        m["outputTokens"] = state(Some(maximum));
        m["totalTokens"] = state(Some(maximum));
        assert_eq!(token_lower_bound(&m), maximum * 2);
        assert!(validate_token_totals(&m).is_err());
    }
}
