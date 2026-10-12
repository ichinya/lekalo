//! The engine-side execution battery of the worklog-changelog pilot
//! (issue #51 r5): the shell, never the report and never the lane,
//! derives the executed corpus and verifies the run against it.
//!
//! Round 5 established that executing the lane is necessary but not
//! sufficient: a program that only prints a captured output —
//! rewriting its own backend digest — received trusted-execution
//! admission, because every anchor (assertion rows, capability
//! tuples, invocation log, observations) was self-asserted by the
//! same bytes the engine hashed. The battery closes that class by
//! binding the receipt to artifacts the lane cannot have printed from
//! capture:
//!
//! - the **corpus** is custody-read from the committed scenario
//!   documents and pin-verified against the exact compilation the
//!   composition pinned (`modelRef`/`irRef`), so admission — not only
//!   the contracts gate — refuses a corpus pointing at other bytes;
//! - the **arrange** (`given`) preconditions are echoed by the lane
//!   only after it verified its own seeded state against them, so a
//!   mutated `given` or a lane that ignores the arrange contract
//!   refuses;
//! - every `when` step must appear as an ordered subsequence of the
//!   lane's invocation log and every `then` assertion is evaluated by
//!   the engine against the lane's structured facts — the lane's
//!   pass/fail labels are never trusted;
//! - a fresh **nonce probe** executes an engine-supplied operation
//!   whose outcome facts embed the per-run nonce, so captured output
//!   replayed by a printer carries stale facts and fails.
//!
//! The residual impossibility is documented: any deterministic check
//! an honest lane computes from public inputs can in principle be
//! computed by a dedicated fabricator; what the battery guarantees is
//! that execution trust binds to the executed battery — corpus,
//! arrange, invocations, assertions, and a fresh probe — instead of
//! to self-asserted output.

use lekalo_core::worklog_changelog::parse_unique_consumed;
use serde_json::{Map, Value as Json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// One engine-built execution battery: the exact bytes the lane must
/// consume, the nonce that freshens every run, and the verified
/// corpus documents the evaluation joins against.
pub(crate) struct Battery {
    /// The exact battery file bytes (the digest input).
    pub bytes: Vec<u8>,
    /// The per-run nonce embedded in the probe inputs.
    pub nonce: String,
    /// The pin-verified corpus documents, ordered by scenario id.
    pub scenarios: Vec<Json>,
}

/// The engine-side outcome of one lane battery run.
pub(crate) struct BatteryOutcome {
    /// The stable failure detail that refuses the target, if any.
    pub failure: Option<&'static str>,
    /// The engine-derived `(scenario, status)` rows — never the
    /// lane's self-reported labels.
    pub assertions: Vec<(String, String)>,
    /// The lane-declared capability tuples (compared verbatim
    /// against the report elsewhere).
    pub capabilities: Vec<(String, String, Option<String>, bool)>,
}

/// Resolve one path to its absolute physical form: canonicalized
/// (every junction, symlink, and reparse point resolved) with the
/// Windows verbatim prefix removed, so the result is directly usable
/// as a child-process argument. A path that cannot be canonicalized
/// is returned as its lexical absolute form.
pub(crate) fn physical_absolute(path: &Path) -> PathBuf {
    let lexical = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let physical = std::fs::canonicalize(&lexical).unwrap_or(lexical);
    let text = physical.to_string_lossy().into_owned();
    let stripped = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        PathBuf::from(text)
    };
    stripped
}

/// A fresh per-run nonce: time, process identity, and a counter,
/// hashed to 32 hex characters. Not a secret — a freshness challenge.
fn fresh_nonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|span| span.as_nanos())
        .unwrap_or_default();
    let pid = std::process::id() as u128;
    let tick = COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
    let mixed = format!("{nanos:x}|{pid:x}|{tick:x}");
    lekalo_core::digest::sha256_hex(mixed.as_bytes())
        .chars()
        .take(32)
        .collect()
}

/// Build the execution battery from the custody-read scenario
/// documents: every document's `modelRef`/`irRef` must pin the exact
/// compilation the composition pinned, and the document corpus must
/// be exactly the compilation's scenario catalog. Any drift is a
/// stable failure detail, never a vacuous battery.
pub(crate) fn build_battery(
    documents: &Result<Vec<Json>, &'static str>,
    model_digest: &str,
    ir_digest: &str,
    catalog: &BTreeSet<String>,
) -> Result<Battery, &'static str> {
    let documents = documents.as_ref().map_err(|detail| *detail)?;
    let mut ordered = documents.clone();
    ordered.sort_by_key(scenario_id);
    let mut ids = BTreeSet::new();
    for document in &ordered {
        let id = scenario_id(document).ok_or("battery-corpus")?;
        if !ids.insert(id) {
            return Err("battery-corpus");
        }
        // The corpus pins: each scenario document names the exact
        // Model and IR bytes it was authored against. A document
        // pinning anything else is a corpus pointing at other bytes.
        for (member, expected) in [("modelRef", model_digest), ("irRef", ir_digest)] {
            let pinned = document
                .get(member)
                .and_then(|reference| reference.get("digest"))
                .and_then(Json::as_str);
            if pinned != Some(expected) {
                return Err("battery-pins");
            }
        }
    }
    if ids != *catalog {
        return Err("battery-corpus");
    }
    let nonce = fresh_nonce();
    let corpus: Vec<Json> = ordered
        .iter()
        .map(|document| {
            serde_json::json!({
                "scenarioId": scenario_id(document),
                "given": document.get("given").cloned().unwrap_or(Json::Array(vec![])),
                "when": document.get("when").cloned().unwrap_or(Json::Array(vec![])),
                "then": document.get("then").cloned().unwrap_or(Json::Array(vec![])),
            })
        })
        .collect();
    let battery = serde_json::json!({
        "batteryVersion": "lekalo/worklog-changelog-battery/v0.8.0",
        "nonce": nonce,
        "corpus": corpus,
        "probe": {
            "entryId": format!("battery-probe-{nonce}"),
            "body": format!("battery probe body {nonce}"),
        },
    });
    let bytes = serde_json::to_vec(&battery).expect("battery serializes");
    Ok(Battery {
        bytes,
        nonce,
        scenarios: ordered,
    })
}

/// The scenario id of one corpus document.
fn scenario_id(document: &Json) -> Option<String> {
    document
        .get("scenarioId")
        .and_then(Json::as_str)
        .map(str::to_owned)
}

/// Run one lane under the battery and evaluate the run engine-side.
/// The lane is spawned with the absolute lane path and the battery
/// file path; the outcome carries the engine-derived assertion rows
/// and the first stable failure detail, if any.
pub(crate) fn run_battery(
    interpreters: &[String],
    lane_path: &Path,
    fixture_root: &Path,
    battery_path: &Path,
    battery: &Battery,
    allowlist_fields: &[String],
) -> BatteryOutcome {
    let mut spawned = false;
    let mut stdout: Option<Vec<u8>> = None;
    let mut refusal: Option<&'static str> = None;
    for interpreter in interpreters {
        let attempted = std::process::Command::new(interpreter)
            .arg(lane_path)
            .arg(battery_path)
            .current_dir(fixture_root)
            .output();
        spawned = true;
        match attempted {
            Ok(attempt) if attempt.status.success() => {
                stdout = Some(attempt.stdout);
                break;
            }
            // A launched lane that exits nonzero refused the battery:
            // its stderr names the refusal class when it is a battery
            // refusal; anything else is a plain lane failure.
            Ok(attempt) => {
                let stderr = String::from_utf8_lossy(&attempt.stderr);
                refusal = stderr.lines().find_map(battery_refusal_detail);
                if refusal.is_some() {
                    break;
                }
            }
            Err(_) => continue,
        }
    }
    if !spawned {
        return failed("engine-unavailable");
    }
    if let Some(detail) = refusal {
        return failed(detail);
    }
    let Some(stdout) = stdout else {
        // The lane launched but every run exited nonzero without a
        // battery refusal marker.
        return failed("lane-exit");
    };
    let text = String::from_utf8_lossy(&stdout);
    let Some(lane) = text
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Json>(line.trim()).ok())
    else {
        return failed("lane-output");
    };
    // The closed runtime surface: an output that does not declare
    // this contract is not a receipt.
    if lane.get("target").and_then(Json::as_str) != Some("php")
        || lane.get("execution").and_then(Json::as_str) != Some("executed")
        || lane.get("identity").and_then(Json::as_str)
            != Some("dev.lekalo.worklog-changelog-runtime@0.8.0")
    {
        return failed("lane-output");
    }
    // The battery echo: the lane must have consumed exactly these
    // bytes and this nonce, and hashed them itself.
    let echo = lane.get("battery");
    let digest = format!("sha256:{}", lekalo_core::digest::sha256_hex(&battery.bytes));
    if echo
        .and_then(|value| value.get("digest"))
        .and_then(Json::as_str)
        != Some(digest.as_str())
        || echo
            .and_then(|value| value.get("nonce"))
            .and_then(Json::as_str)
            != Some(battery.nonce.as_str())
    {
        return failed("battery-echo");
    }
    // The capability tuples are lane-declared and compared verbatim
    // against the report elsewhere; malformed rows are not a receipt.
    let mut capabilities = Vec::new();
    let Some(rows) = lane.get("capabilities").and_then(Json::as_array) else {
        return failed("lane-output");
    };
    for row in rows {
        let (Some(capability), Some(support), Some(blocking)) = (
            row.get("capability").and_then(Json::as_str),
            row.get("support").and_then(Json::as_str),
            row.get("blocking").and_then(Json::as_bool),
        ) else {
            return failed("lane-output");
        };
        capabilities.push((
            capability.to_owned(),
            support.to_owned(),
            row.get("approval")
                .and_then(Json::as_str)
                .map(str::to_owned),
            blocking,
        ));
    }
    // The corpus join: the lane must have run exactly the battery
    // corpus, echoed its arrange verification, bound its invocations,
    // and reported the facts the engine evaluates.
    let rows = lane
        .get("assertions")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default();
    let arrangements = lane.get("arrangements").and_then(Json::as_object);
    let mut derived: Vec<(String, String)> = Vec::new();
    for document in &battery.scenarios {
        let Some(id) = scenario_id(document) else {
            return failed("battery-corpus");
        };
        let row = rows
            .iter()
            .find(|row| row.get("scenario").and_then(Json::as_str) == Some(id.as_str()));
        let Some(row) = row else {
            return failed("battery-corpus");
        };
        let mut failures: Vec<&'static str> = Vec::new();
        // 1. The arrange echo: the lane verified its seeded state
        //    against the battery `given` preconditions.
        let expected_given = document.get("given").cloned().unwrap_or(Json::Null);
        let arrange = arrangements
            .and_then(|map| map.get(id.as_str()))
            .cloned()
            .unwrap_or(Json::Null);
        if arrange.get("given") != Some(&expected_given)
            || arrange.get("verified").and_then(Json::as_bool) != Some(true)
        {
            failures.push("battery-arrange");
        }
        // 2. The invocation binding: every `when` step is an ordered
        //    subsequence of the lane's invocation log.
        let log = row.get("invocations").and_then(Json::as_array);
        let when = document.get("when").and_then(Json::as_array);
        let invocations_bound = match (log, when) {
            (Some(log), Some(when)) => invocation_bound(log, when),
            (Some(_), None) => true,
            _ => false,
        };
        if !invocations_bound {
            failures.push("battery-invocations");
        }
        // 3. The assertion semantics: the engine evaluates every
        //    `then` assertion against the lane's reported facts.
        let facts = row.get("observations").cloned().unwrap_or(Json::Null);
        if let Some(then) = document.get("then").and_then(Json::as_array) {
            for step in then {
                let kind = step
                    .get("assertion")
                    .and_then(|assertion| assertion.get("kind"))
                    .and_then(Json::as_str)
                    .unwrap_or("");
                if kind == "forbidden_effect" {
                    // Asserted statically against the compiled effect
                    // projection; retained as runtime evidence.
                    continue;
                }
                let Some(step_id) = step.get("stepId").and_then(Json::as_str) else {
                    failures.push("battery-assertions");
                    continue;
                };
                let fact = facts.get(step_id);
                let assertion = step.get("assertion").unwrap_or(&Json::Null);
                let ok =
                    fact.is_some_and(|fact| evaluate_assertion(assertion, fact, allowlist_fields));
                if !ok {
                    failures.push("battery-assertions");
                }
            }
        }
        let status = if failures.is_empty() { "pass" } else { "fail" };
        if let Some(first) = failures.first() {
            return BatteryOutcome {
                failure: Some(first),
                assertions: derived,
                capabilities,
            };
        }
        derived.push((id, status.to_owned()));
    }
    // 4. The nonce probe: the engine-supplied operation's outcome
    //    facts must embed this run's nonce — captured output carries
    //    a stale nonce and fails here.
    let probe = lane.get("probe").cloned().unwrap_or(Json::Null);
    let probe_entry = format!("battery-probe-{}", battery.nonce);
    let probe_body = format!("battery probe body {}", battery.nonce);
    let probe_assertion = serde_json::json!({
        "kind": "entity_state",
        "entity": "changelog.entry",
        "where": [{"field": "entry_id", "equals": {"type": "string", "value": probe_entry}}],
        "expect": {"presence": "exists"},
        "fields": {
            "draft_body": {"value": {"type": "string", "value": probe_body}},
            "record_revision": {"value": {"type": "integer", "value": 1}},
        },
    });
    if !evaluate_assertion(&probe_assertion, &probe, allowlist_fields) {
        return failed("battery-probe");
    }
    BatteryOutcome {
        failure: None,
        assertions: derived,
        capabilities,
    }
}

/// A refused battery outcome with its stable detail.
fn failed(detail: &'static str) -> BatteryOutcome {
    BatteryOutcome {
        failure: Some(detail),
        assertions: Vec::new(),
        capabilities: Vec::new(),
    }
}

/// The stable refusal vocabulary a lane may name on stderr when it
/// refuses the battery itself; any other stderr text is not a
/// battery refusal.
fn battery_refusal_detail(line: &str) -> Option<&'static str> {
    match line.trim() {
        "battery-unreadable" | "battery-unparseable" => Some("battery-echo"),
        "battery-corpus" => Some("battery-corpus"),
        "battery-arrange" => Some("battery-arrange"),
        "battery-probe" => Some("battery-probe"),
        _ => None,
    }
}

/// The plain value of one typed Scenario-IR cell.
fn plain(cell: &Json) -> Json {
    cell.get("value").cloned().unwrap_or(Json::Null)
}

/// JS-parity string coercion for selector and field comparison.
fn coerce(value: &Json) -> String {
    match value {
        Json::String(text) => text.clone(),
        Json::Null => "null".to_owned(),
        other => other.to_string(),
    }
}

/// Value equality with the harness semantics: numeric fields compare
/// as numbers, everything else compares through string coercion.
fn values_equal(observed: &Json, expected: &Json) -> bool {
    if let (Some(left), Some(right)) = (observed.as_f64(), expected.as_f64()) {
        return left == right;
    }
    coerce(observed) == coerce(expected)
}

/// Whether one observed invocation input map equals one declared
/// typed input map (same keys, equal plain values).
fn same_input(observed: &Json, declared: &Map<String, Json>) -> bool {
    let Some(observed) = observed.as_object() else {
        return false;
    };
    if observed.len() != declared.len() {
        return false;
    }
    declared.iter().all(|(key, cell)| {
        let expected = plain(cell);
        observed
            .get(key)
            .is_some_and(|value| values_equal(value, &expected))
    })
}

/// Whether the declared `when` steps appear, in order, as a
/// subsequence of the lane's invocation log.
fn invocation_bound(log: &[Json], when: &[Json]) -> bool {
    let mut cursor = 0usize;
    for step in when {
        let Some(action) = step.get("action") else {
            return false;
        };
        let Some(operation) = action.get("operation").and_then(Json::as_str) else {
            return false;
        };
        let declared = action
            .get("input")
            .and_then(Json::as_object)
            .cloned()
            .unwrap_or_default();
        let found = log[cursor..].iter().position(|entry| {
            entry.get("operation").and_then(Json::as_str) == Some(operation)
                && same_input(entry.get("input").unwrap_or(&Json::Null), &declared)
        });
        match found {
            Some(index) => cursor += index + 1,
            None => return false,
        }
    }
    true
}

/// Evaluate one `then` assertion against one reported structured
/// fact, exactly the semantics the pilot harness applies.
fn evaluate_assertion(assertion: &Json, fact: &Json, allowlist_fields: &[String]) -> bool {
    let kind = assertion.get("kind").and_then(Json::as_str).unwrap_or("");
    if fact.get("kind").and_then(Json::as_str) != Some(kind) {
        return false;
    }
    match kind {
        "error" => {
            if fact.get("error") != assertion.get("error") {
                return false;
            }
            let Some(payload) = assertion.get("payload").and_then(Json::as_object) else {
                return true;
            };
            payload.iter().all(|(key, expected)| {
                fact.get("payload")
                    .and_then(|payload| payload.get(key))
                    .is_some_and(|observed| observed == expected)
            })
        }
        "entity_state" => {
            if fact.get("entity") != assertion.get("entity") {
                return false;
            }
            let empty = Vec::new();
            let rows = fact
                .get("rows")
                .and_then(Json::as_array)
                .cloned()
                .unwrap_or_default();
            let wheres = assertion
                .get("where")
                .and_then(Json::as_array)
                .unwrap_or(&empty);
            let matched: Vec<&Json> = rows
                .iter()
                .filter(|row| {
                    wheres.iter().all(|selector| {
                        let field = selector.get("field").and_then(Json::as_str).unwrap_or("");
                        let expected = selector.get("equals").map(plain).unwrap_or(Json::Null);
                        row.get(field)
                            .is_some_and(|observed| values_equal(observed, &expected))
                    })
                })
                .collect();
            let expect = assertion.get("expect");
            let count = expect
                .and_then(|expect| expect.get("count"))
                .filter(|count| !count.is_null());
            match count {
                Some(count) => {
                    let Some(count) = count.as_f64() else {
                        return false;
                    };
                    if matched.len() as f64 != count {
                        return false;
                    }
                }
                None => {
                    let presence = expect
                        .and_then(|expect| expect.get("presence"))
                        .and_then(Json::as_str)
                        .unwrap_or("exists");
                    if presence == "exists" {
                        if matched.is_empty() {
                            return false;
                        }
                    } else if !matched.is_empty() {
                        return false;
                    }
                }
            }
            let Some(fields) = assertion.get("fields").and_then(Json::as_object) else {
                return true;
            };
            fields.iter().all(|(field, expectation)| {
                let expected = expectation.get("value").map(plain).unwrap_or(Json::Null);
                matched.iter().all(|row| {
                    row.get(field)
                        .is_some_and(|observed| values_equal(observed, &expected))
                })
            })
        }
        "idempotency" => {
            if fact.get("replay") != assertion.get("replay") {
                return false;
            }
            let expected_duplicates = match assertion.get("duplicates") {
                Some(Json::String(text)) if text == "none" => 0.0,
                Some(value) => value.as_f64().unwrap_or(f64::NAN),
                None => f64::NAN,
            };
            if fact.get("duplicates").and_then(Json::as_f64) != Some(expected_duplicates) {
                return false;
            }
            fact.get("equivalence") == assertion.get("equivalence")
        }
        "contract_match" => {
            if fact.get("contract") != assertion.get("contract") {
                return false;
            }
            let mut served: Vec<String> = fact
                .get("projection")
                .and_then(Json::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(Json::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            served.sort();
            let mut declared: Vec<String> = assertion
                .get("projection")
                .and_then(Json::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(Json::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            declared.sort();
            if !declared.iter().all(|field| served.contains(field)) {
                return false;
            }
            // The served projection must be exactly the composition's
            // committed public allowlist, never a narrower set.
            let mut allowlist = allowlist_fields.to_vec();
            allowlist.sort();
            served == allowlist
        }
        _ => false,
    }
}

type ConfinedReader<'a> = &'a dyn Fn(&Path, &Path) -> Result<Vec<u8>, std::io::Error>;

/// Custody-read one fixture's scenario corpus through the shell's
/// confined reader: every `*.scenario.json` document under the
/// fixture root's `scenarios` directory, parsed through the
/// unique-key decoder. A missing directory, an escaping link, or an
/// unreadable document is a stable custody failure.
pub(crate) fn read_scenario_documents(
    fixture_root: &Path,
    confined_read: ConfinedReader<'_>,
) -> Result<Vec<Json>, &'static str> {
    let directory = fixture_root.join("scenarios");
    let entries = std::fs::read_dir(&directory).map_err(|_| "battery-custody")?;
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".scenario.json"))
        .collect();
    names.sort();
    if names.is_empty() {
        return Err("battery-custody");
    }
    let mut documents = Vec::new();
    for name in names {
        let path = directory.join(&name);
        let bytes = confined_read(fixture_root, &path).map_err(|_| "battery-custody")?;
        let text = std::str::from_utf8(&bytes).map_err(|_| "battery-custody")?;
        let parsed = parse_unique_consumed(text).map_err(|_| "battery-custody")?;
        documents.push(parsed);
    }
    Ok(documents)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Typed-cell plain values and JS-parity coercion.
    #[test]
    fn plain_values_compare_with_harness_semantics() {
        let cell = serde_json::json!({"type": "string", "value": "task-1"});
        assert_eq!(plain(&cell), Json::String("task-1".to_owned()));
        assert!(values_equal(&Json::from(1), &Json::from(1)));
        assert!(values_equal(&Json::from(1), &Json::from(1.0)));
        assert!(values_equal(&Json::String("1".into()), &Json::from(1)));
        assert!(!values_equal(
            &Json::String("open".into()),
            &Json::String("done".into())
        ));
    }

    /// The `when` binding is an ordered subsequence match.
    #[test]
    fn invocations_bind_as_ordered_subsequence() {
        let when = vec![serde_json::json!({
            "stepId": "a", "action": {"kind": "invoke", "operation": "worklog.complete_task",
                "input": {"task_id": {"type": "string", "value": "task-1"}}}
        })];
        let log = vec![
            serde_json::json!({"operation": "worklog.write_worklog", "input": {"task_id": "task-1"}}),
            serde_json::json!({"operation": "worklog.complete_task", "input": {"task_id": "task-1"}}),
        ];
        assert!(invocation_bound(&log, &when));
        let drifted = vec![serde_json::json!({
            "stepId": "a", "action": {"kind": "invoke", "operation": "worklog.complete_task",
                "input": {"task_id": {"type": "string", "value": "task-2"}}}
        })];
        assert!(!invocation_bound(&log, &drifted));
    }

    /// Entity-state assertions evaluate count, presence, and fields.
    #[test]
    fn entity_state_assertions_evaluate() {
        let fact = serde_json::json!({
            "kind": "entity_state", "entity": "worklog.task",
            "rows": [{"task_id": "task-1", "state": "done"}],
        });
        let assertion = serde_json::json!({
            "kind": "entity_state", "entity": "worklog.task",
            "where": [{"field": "task_id", "equals": {"type": "string", "value": "task-1"}}],
            "expect": {"presence": "exists"},
            "fields": {"state": {"value": {"type": "string", "value": "done"}}},
        });
        assert!(evaluate_assertion(&assertion, &fact, &[]));
        let mutated = serde_json::json!({
            "kind": "entity_state", "entity": "worklog.task",
            "where": [{"field": "task_id", "equals": {"type": "string", "value": "task-1"}}],
            "expect": {"presence": "exists"},
            "fields": {"state": {"value": {"type": "string", "value": "open"}}},
        });
        assert!(!evaluate_assertion(&mutated, &fact, &[]));
    }

    /// The battery refuses corpus pin drift and catalog mismatch.
    #[test]
    fn battery_build_refuses_pin_and_corpus_drift() {
        let document = serde_json::json!({
            "scenarioId": "worklog.scenario.one",
            "modelRef": {"digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"},
            "irRef": {"digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222"},
            "given": [], "when": [], "then": [],
        });
        let mut catalog = BTreeSet::new();
        catalog.insert("worklog.scenario.one".to_owned());
        let ok = build_battery(
            &Ok(vec![document.clone()]),
            "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            &catalog,
        );
        assert!(ok.is_ok());
        let pinned_wrong = build_battery(
            &Ok(vec![document.clone()]),
            "sha256:3333333333333333333333333333333333333333333333333333333333333333",
            "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            &catalog,
        );
        assert_eq!(pinned_wrong.err(), Some("battery-pins"));
        let pruned_catalog = BTreeSet::new();
        let pruned = build_battery(
            &Ok(vec![document]),
            "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            &pruned_catalog,
        );
        assert_eq!(pruned.err(), Some("battery-corpus"));
    }
}
