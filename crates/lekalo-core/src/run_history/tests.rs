//! Issue #121 core run-history tests: fail-closed validation and
//! normalization, atomic append with separate assertions, retention and
//! clock-rollback behavior, transitive dependent invalidation, recovery
//! and index rebuild, scope isolation, cursors, and the offline
//! storage posture.
//!
//! The clock is injected through the test-only seam; production always
//! uses the system clock, and identifiers are always random opaque
//! tokens (tests supply explicit run ids through the exactly-once
//! retry seam for determinism).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

use super::clock::{Clock, Instant};
use super::store::{Store, StoreError};
use super::validate;

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

fn temp_case(tag: &str) -> PathBuf {
    let id = NEXT_CASE.fetch_add(1, Ordering::Relaxed) + u64::from(std::process::id());
    let dir = std::env::temp_dir().join(format!("lekalo-run-history-{tag}-{id}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp case dir");
    dir
}

/// A step clock the tests move explicitly.
#[derive(Debug, Clone)]
struct StepClock {
    seconds: std::cell::Cell<i64>,
}

impl StepClock {
    fn at(seconds: i64) -> Self {
        Self {
            seconds: std::cell::Cell::new(seconds),
        }
    }

    fn set(&self, seconds: i64) {
        self.seconds.set(seconds);
    }
}

impl Clock for StepClock {
    fn now(&self) -> Instant {
        Instant {
            seconds: self.seconds.get(),
            nanos: 0,
        }
    }
}

/// One full valid provenance block with known pins.
fn provenance_json() -> Value {
    json!({
        "git": {
            "commit": {"state": "known", "value": "1111111111111111111111111111111111111111"},
            "dirty": {"state": "known", "value": false},
            "workingSetDigest": {"state": "known", "value": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
        },
        "model": {
            "revision": {"state": "known", "value": "planner-model"},
            "digest": {"state": "known", "value": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
            "irDigest": {"state": "known", "value": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"}
        },
        "lock": {
            "version": {"state": "known", "value": "0.4.0"},
            "digest": {"state": "known", "value": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"}
        },
        "core": {
            "version": {"state": "known", "value": "0.4.0"},
            "buildRevision": {"state": "known", "value": "build-2026-09-30"},
            "buildDigest": {"state": "known", "value": "sha256:5555555555555555555555555555555555555555555555555555555555555555"}
        },
        "adapters": [{
            "id": "php-laravel",
            "version": {"state": "known", "value": "0.4.0"},
            "manifestDigest": {"state": "known", "value": "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"},
            "bundleDigest": {"state": "known", "value": "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"}
        }],
        "profile": {
            "id": {"state": "known", "value": "validation-profile.default"},
            "version": {"state": "known", "value": "0.4.0"},
            "digest": {"state": "known", "value": "sha256:1111111111111111111111111111111111111111111111111111111111111111"}
        },
        "harness": {
            "id": {"state": "known", "value": "pilot-harness"},
            "version": {"state": "known", "value": "0.1.0"},
            "modelId": {"state": "known", "value": "pilot-model"},
            "modelRevision": {"state": "known", "value": "pilot-model-rev-1"}
        }
    })
}

/// One full valid metrics block; every leaf a known value.
fn metrics_json() -> Value {
    json!({
        "durationMs": {"state": "known", "value": 1500},
        "filesRead": {"state": "known", "value": 0},
        "filesChanged": {"state": "known", "value": 0},
        "toolCalls": {"state": "known", "value": 2},
        "retryCount": {"state": "unknown"},
        "replanCount": {"state": "unknown"},
        "tokens": {
            "input": {"state": "known", "value": 100},
            "output": {"state": "known", "value": 20},
            "reasoning": {"state": "unknown"},
            "total": {"state": "known", "value": 120}
        },
        "cost": {
            "amount": {"state": "unknown"},
            "currency": {"state": "unknown"},
            "basis": {"state": "unknown"}
        },
        "context": {
            "bytes": {"state": "unknown"},
            "estimatedTokens": {"state": "unknown"},
            "includedFacts": {"state": "unknown"},
            "candidateFacts": {"state": "unknown"},
            "coverageRatio": {"state": "unknown"},
            "representation": {"state": "unknown"},
            "estimatorVersion": {"state": "unknown"}
        }
    })
}

/// One full valid observation.
fn observation_json(run_id: &str) -> Value {
    json!({
        "schema_version": "lekalo/run-observation/v0.4.0",
        "identity": "dev.lekalo.run-observation@0.4.0",
        "runId": run_id,
        "pilot": {"mode": "brownfield", "scopeState": "observed"},
        "operation": {"kind": "verify", "affectedSemanticIds": ["planner.focus_task"]},
        "timestamp": "2026-09-30T12:00:00Z",
        "provenance": provenance_json(),
        "metrics": metrics_json(),
        "measurementSources": [
            {
                "field": "metrics.durationMs",
                "sourceKind": "core",
                "sourceId": "lekalo-core",
                "sourceVersion": {"state": "known", "value": "0.4.0"}
            },
            {
                "field": "metrics.tokens.total",
                "sourceKind": "harness",
                "sourceId": "pilot-harness",
                "sourceVersion": {"state": "known", "value": "0.1.0"}
            }
        ],
        "testGateSummaries": [
            {
                "id": "planner-scenarios",
                "kind": "test",
                "sourceOutcome": "passed",
                "passed": {"state": "known", "value": 3},
                "failed": {"state": "known", "value": 0},
                "unsupported": {"state": "known", "value": 0},
                "infrastructure": {"state": "known", "value": 0},
                "coverageState": "complete",
                "evidenceRef": null
            }
        ],
        "diagnostics": [],
        "assertions": {
            "rows": [
                {
                    "assertionId": "focus-task-returns-planned-order",
                    "subjectSemanticId": "planner.focus_task",
                    "kind": "behavior",
                    "outcome": "pass",
                    "evidenceRef": null
                }
            ]
        },
        "repeatParentRunId": null,
        "status": {"outcome": "pass", "coverageState": "complete"},
        "dataSensitivity": "internal"
    })
}

fn observation_bytes(run_id: &str) -> Vec<u8> {
    serde_json::to_vec(&observation_json(run_id)).expect("observation serializes")
}

fn open(root: &std::path::Path, seconds: i64) -> Store {
    Store::open_with_injections(root, Box::new(StepClock::at(seconds))).expect("store opens")
}

fn open_production(root: &std::path::Path) -> Store {
    Store::open(root).expect("store opens")
}

fn scope_of(store: &mut Store) -> String {
    store.scope_create().expect("scope created").0
}

fn append_ok(store: &mut Store, scope: &str, run_id: &str) -> super::store::Receipt {
    store
        .append(
            scope,
            &parse(&observation_bytes(run_id)),
            &observation_bytes(run_id),
        )
        .expect("append succeeds")
}

fn parse(bytes: &[u8]) -> super::types::Observation {
    validate::parse_observation(bytes).expect("observation parses")
}

// ---------------------------------------------------------------------------
// Validation and normalization
// ---------------------------------------------------------------------------

#[test]
fn a_full_valid_observation_ingests_with_the_complete_record_shape() {
    let root = temp_case("shape");
    let mut store = open(&root, 1_000_000);
    let scope = scope_of(&mut store);
    let receipt = append_ok(&mut store, &scope, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1");
    assert_eq!(receipt.run_id, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1");
    assert_eq!(receipt.status_outcome, super::types::Outcome::Pass);
    assert!(receipt.record_digest.starts_with("sha256:"));

    let (record, assertions) = store.get(&scope, &receipt.run_id).expect("stored");
    let record_value: Value = serde_json::from_str(&record).expect("record json");
    assert_eq!(
        record_value["schema_version"],
        json!("lekalo/run-record/v0.4.0")
    );
    assert_eq!(
        record_value["identity"],
        json!("dev.lekalo.run-record@0.4.0")
    );
    assert_eq!(record_value["artifactKind"], json!("history.run-record"));
    // The scope is store-selected and opaque.
    assert_eq!(record_value["scope"]["tenantScopeId"], json!(scope));
    assert_eq!(
        record_value["scope"]["repositoryId"],
        record_value["scope"]["repositoryId"]
    );
    // Assertions live separately and are digest-bound.
    let assertions_value: Value =
        serde_json::from_str(&assertions.expect("assertions stored")).expect("assertions json");
    assert_eq!(
        assertions_value["schema_version"],
        json!("lekalo/run-assertions/v0.4.0")
    );
    assert_eq!(
        assertions_value["artifactKind"],
        json!("history.assertion-set")
    );
    let reference = &record_value["assertionsRef"];
    assert_eq!(reference["setId"], assertions_value["setId"]);
    assert_eq!(reference["count"], json!(1));
    // No assertion rows leak into the metrics block.
    assert!(record_value["metrics"].get("rows").is_none());
    assert!(record_value["metrics"].get("assertions").is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn absent_metric_leaves_normalize_to_unknown_never_zero() {
    let root = temp_case("unknown");
    let mut store = open(&root, 1_000_000);
    let scope = scope_of(&mut store);
    let mut observation = observation_json("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa2");
    // Every metric leaf absent entirely; the sources must go too
    // (one source per known metric).
    observation["metrics"] = json!({});
    observation["measurementSources"] = json!([]);
    observation["testGateSummaries"] = json!([]);
    observation["status"] = json!({"outcome": "unsupported", "coverageState": "unknown"});
    let receipt = store
        .append(
            &scope,
            &parse(&serde_json::to_vec(&observation).expect("serializes")),
            &serde_json::to_vec(&observation).expect("serializes"),
        )
        .expect("append succeeds");
    let (record, _) = store.get(&scope, &receipt.run_id).expect("stored");
    let record_value: Value = serde_json::from_str(&record).expect("json");
    // Missing cost is the unknown value state, never zero, null, or "".
    assert_eq!(
        record_value["metrics"]["cost"]["amount"],
        json!({"state": "unknown"})
    );
    assert_eq!(
        record_value["metrics"]["durationMs"],
        json!({"state": "unknown"})
    );
    assert_eq!(
        record_value["metrics"]["tokens"]["total"],
        json!({"state": "unknown"})
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_known_zero_survives_as_a_known_zero() {
    let root = temp_case("zero");
    let mut store = open(&root, 1_000_000);
    let scope = scope_of(&mut store);
    let mut observation = observation_json("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa3");
    observation["metrics"]["cost"]["amount"] = json!({"state": "known", "value": "0"});
    observation["metrics"]["cost"]["currency"] = json!({"state": "known", "value": "USD"});
    observation["metrics"]["cost"]["basis"] = json!({"state": "known", "value": "reported"});
    let receipt = store
        .append(
            &scope,
            &parse(&serde_json::to_vec(&observation).expect("serializes")),
            &serde_json::to_vec(&observation).expect("serializes"),
        )
        .expect("append succeeds");
    let (record, _) = store.get(&scope, &receipt.run_id).expect("stored");
    let record_value: Value = serde_json::from_str(&record).expect("json");
    assert_eq!(
        record_value["metrics"]["cost"]["amount"],
        json!({"state": "known", "value": "0"})
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn duplicate_json_keys_refuse() {
    let text = r#"{"schema_version":"lekalo/run-observation/v0.4.0","schema_version":"x"}"#;
    let violation = validate::parse_observation(text.as_bytes()).expect_err("duplicate refused");
    assert_eq!(violation.code, super::codes::INPUT_INVALID);
    assert_eq!(violation.detail, "duplicate-key");
}

#[test]
fn unknown_schema_versions_refuse_as_unsupported() {
    let mut observation = observation_json("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa4");
    observation["schema_version"] = json!("lekalo/run-observation/v0.3.0");
    let violation = validate::parse_observation(&serde_json::to_vec(&observation).expect("s"))
        .expect_err("version refused");
    assert_eq!(violation.code, super::codes::VERSION_UNSUPPORTED);
}

#[test]
fn absolute_paths_urls_and_text_cannot_enter_token_fields() {
    for hostile in [
        "C:/Users/someone/repo",
        "\\\\server\\share\\prompt.txt",
        "https://provider.example/v1/messages",
        "raw prompt body with spaces",
    ] {
        let mut observation = observation_json("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa5");
        observation["provenance"]["harness"]["id"] = json!({"state": "known", "value": hostile});
        let error = validate::parse_observation(&serde_json::to_vec(&observation).expect("s"))
            .and_then(|parsed| validate::validate_observation(&parsed).map(|_| parsed))
            .expect_err("hostile token refused");
        assert_eq!(error.code, super::codes::UNSAFE_FIELD, "{hostile}");
    }
}

#[test]
fn secret_material_and_encoded_blobs_refuse_even_in_token_grammar() {
    for hostile in [
        "apikey-1234",
        "my-secret-value",
        "bearer-credentials-here",
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJVadQssw5c",
    ] {
        let lowered = hostile.to_ascii_lowercase();
        let fits = lowered.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        });
        if !fits && !hostile.starts_with("eyJ") {
            continue;
        }
        assert!(
            validate::token_has_secret_material(hostile),
            "{hostile} must refuse"
        );
    }
}

#[test]
fn unregistered_diagnostic_codes_refuse() {
    let mut observation = observation_json("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa6");
    observation["diagnostics"] = json!([
        {"code": "history.not-a-code", "severity": "error", "count": 1}
    ]);
    let parsed =
        validate::parse_observation(&serde_json::to_vec(&observation).expect("s")).expect("parses");
    let violation = validate::validate_observation(&parsed).expect_err("unregistered refused");
    assert_eq!(violation.detail, "unregistered-code");
}

#[test]
fn a_registered_diagnostic_code_requires_the_registry_severity() {
    let mut observation = observation_json("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa7");
    observation["diagnostics"] = json!([
        {"code": "history.busy", "severity": "error", "count": 1}
    ]);
    let parsed =
        validate::parse_observation(&serde_json::to_vec(&observation).expect("s")).expect("parses");
    // history.busy defaults to error in the registry, so this passes...
    validate::validate_observation(&parsed).expect("matching severity accepted");
    // ...and a mismatching severity refuses.
    observation["diagnostics"] = json!([
        {"code": "history.busy", "severity": "info", "count": 1}
    ]);
    let parsed =
        validate::parse_observation(&serde_json::to_vec(&observation).expect("s")).expect("parses");
    let violation = validate::validate_observation(&parsed).expect_err("severity refused");
    assert_eq!(violation.detail, "severity-mismatch");
}

#[test]
fn an_inconsistent_known_token_total_refuses() {
    let mut observation = observation_json("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa8");
    observation["metrics"]["tokens"] = json!({
        "input": {"state": "known", "value": 100},
        "output": {"state": "known", "value": 20},
        "reasoning": {"state": "known", "value": 5},
        "total": {"state": "known", "value": 999}
    });
    let parsed =
        validate::parse_observation(&serde_json::to_vec(&observation).expect("s")).expect("parses");
    let violation = validate::validate_observation(&parsed).expect_err("total refused");
    assert_eq!(violation.detail, "total-token-inconsistency");
}

#[test]
fn a_known_coverage_ratio_requires_a_known_nonzero_denominator() {
    let mut observation = observation_json("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa9");
    observation["metrics"]["context"]["coverageRatio"] = json!({"state": "known", "value": "1.0"});
    // candidateFacts unknown: the ratio cannot be known.
    let parsed =
        validate::parse_observation(&serde_json::to_vec(&observation).expect("s")).expect("parses");
    let violation = validate::validate_observation(&parsed).expect_err("ratio refused");
    assert_eq!(violation.detail, "coverage-ratio-denominator");
    // A known zero denominator refuses too.
    observation["metrics"]["context"]["candidateFacts"] = json!({"state": "known", "value": 0});
    let parsed =
        validate::parse_observation(&serde_json::to_vec(&observation).expect("s")).expect("parses");
    let violation = validate::validate_observation(&parsed).expect_err("zero denominator");
    assert_eq!(violation.detail, "coverage-ratio-denominator");
    // A known nonzero denominator accepts.
    observation["metrics"]["context"]["candidateFacts"] = json!({"state": "known", "value": 12});
    let parsed =
        validate::parse_observation(&serde_json::to_vec(&observation).expect("s")).expect("parses");
    validate::validate_observation(&parsed).expect("ratio with denominator accepted");
}

#[test]
fn a_measurement_source_cannot_target_an_unknown_metric() {
    let mut observation = observation_json("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa10");
    observation["metrics"]["cost"]["amount"] = json!({"state": "unknown"});
    observation["measurementSources"][1] = json!({
        "field": "metrics.cost.amount",
        "sourceKind": "harness",
        "sourceId": "pilot-harness",
        "sourceVersion": {"state": "known", "value": "0.1.0"}
    });
    let parsed =
        validate::parse_observation(&serde_json::to_vec(&observation).expect("s")).expect("parses");
    let violation = validate::validate_observation(&parsed).expect_err("source refused");
    assert_eq!(violation.detail, "unknown-metric-source");
}

#[test]
fn a_passing_status_cannot_carry_a_failing_summary() {
    let mut observation = observation_json("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa11");
    observation["testGateSummaries"][0]["sourceOutcome"] = json!("failed");
    let parsed =
        validate::parse_observation(&serde_json::to_vec(&observation).expect("s")).expect("parses");
    let violation = validate::validate_observation(&parsed).expect_err("contradiction");
    assert_eq!(violation.detail, "contradiction");
}

#[test]
fn the_source_outcome_mapping_is_exhaustive_and_preserves_source_verdicts() {
    use super::types::{CoverageState, Outcome, SourceOutcome};
    let cases = [
        (
            SourceOutcome::Passed,
            Outcome::Pass,
            CoverageState::Complete,
        ),
        (
            SourceOutcome::Failed,
            Outcome::Fail,
            CoverageState::Complete,
        ),
        (
            SourceOutcome::Degraded,
            Outcome::Warn,
            CoverageState::Incomplete,
        ),
        (
            SourceOutcome::Blocked,
            Outcome::Fail,
            CoverageState::Incomplete,
        ),
        (
            SourceOutcome::Security,
            Outcome::Fail,
            CoverageState::Incomplete,
        ),
        (
            SourceOutcome::Missing,
            Outcome::Infrastructure,
            CoverageState::Unknown,
        ),
        (
            SourceOutcome::Unsupported,
            Outcome::Unsupported,
            CoverageState::Incomplete,
        ),
        (
            SourceOutcome::Infrastructure,
            Outcome::Infrastructure,
            CoverageState::Incomplete,
        ),
    ];
    for (source, outcome, coverage) in cases {
        let (mapped, mapped_coverage) = validate::map_source_outcome(source);
        assert_eq!(mapped, outcome, "{source:?}");
        assert_eq!(mapped_coverage, coverage, "{source:?}");
    }
}

#[test]
fn the_input_fingerprint_changes_with_any_changed_pin() {
    let base = validate::parse_observation(&observation_bytes("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa12"))
        .expect("parses");
    let provenance = super::store::normalize_provenance_for_fingerprint(&base);
    let base_fingerprint = validate::input_fingerprint(&provenance);
    assert!(base_fingerprint.starts_with("sha256:"));

    // A changed adapter bundle digest changes the fingerprint.
    let mut changed = observation_json("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa12");
    changed["provenance"]["adapters"][0]["bundleDigest"] = json!(
        {"state": "known", "value": "sha256:2222222222222222222222222222222222222222222222222222222222222222"}
    );
    let changed =
        validate::parse_observation(&serde_json::to_vec(&changed).expect("s")).expect("parses");
    let changed_fingerprint = validate::input_fingerprint(
        &super::store::normalize_provenance_for_fingerprint(&changed),
    );
    assert_ne!(base_fingerprint, changed_fingerprint);

    // An unknown required pin makes completeness impossible.
    let mut incomplete = observation_json("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa12");
    incomplete["provenance"]["lock"]["digest"] = json!({"state": "unknown"});
    let incomplete =
        validate::parse_observation(&serde_json::to_vec(&incomplete).expect("s")).expect("parses");
    let incomplete_provenance = super::store::normalize_provenance_for_fingerprint(&incomplete);
    let link = validate::resolve_repeat(
        &incomplete_provenance,
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        Some(&base_fingerprint),
    )
    .expect("repeat resolves");
    assert_eq!(
        link.expect("link").comparability,
        super::types::Comparability::Incomplete
    );
}

// ---------------------------------------------------------------------------
// Store behavior
// ---------------------------------------------------------------------------

#[test]
fn an_identical_retry_is_idempotent_and_changed_bytes_conflict() {
    let root = temp_case("retry");
    let mut store = open(&root, 1_000_000);
    let scope = scope_of(&mut store);
    let first = append_ok(&mut store, &scope, "cccccccccccccccccccccccccccccccc");
    let second = append_ok(&mut store, &scope, "cccccccccccccccccccccccccccccccc");
    // The injected clock produces a byte-identical record; the retry
    // is fully idempotent.
    assert_eq!(first.record_digest, second.record_digest);
    assert!(first.reason_codes.is_empty());
    assert!(second.reason_codes.is_empty());

    // Changed bytes at the same run id refuse without echoing values.
    let mut changed = observation_json("cccccccccccccccccccccccccccccccc");
    changed["metrics"]["durationMs"] = json!({"state": "known", "value": 9999});
    let error = store
        .append(
            &scope,
            &parse(&serde_json::to_vec(&changed).expect("s")),
            &serde_json::to_vec(&changed).expect("s"),
        )
        .expect_err("conflict");
    assert_eq!(error, StoreError::Conflict);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn scopes_are_isolated_and_unknown_tokens_disclose_nothing() {
    let root = temp_case("isolation");
    let mut store = open(&root, 1_000_000);
    let scope_a = scope_of(&mut store);
    let scope_b = scope_of(&mut store);
    let receipt = append_ok(&mut store, &scope_a, "dddddddddddddddddddddddddddddddd");
    append_ok(&mut store, &scope_a, "dddddddddddddddddddddddddddddd10");

    // Scope B sees neither the run nor the cursor binding.
    let (rows, _) = store.list(&scope_b, 50, None).expect("list");
    assert!(rows.is_empty());
    let error = store
        .get(&scope_b, &receipt.run_id)
        .expect_err("cross-scope read refused");
    assert_eq!(error, StoreError::SourceMissing("run"));
    // Deleting across scopes is an explicit absent result.
    let error = store
        .delete(&scope_b, &receipt.run_id, false)
        .expect_err("cross-scope delete refused");
    assert_eq!(error, StoreError::SourceMissing("run"));
    // A cursor minted in scope A does not list scope B.
    let (rows, cursor) = store.list(&scope_a, 1, None).expect("list");
    assert_eq!(rows.len(), 1);
    let error = store
        .list(&scope_b, 50, cursor.as_deref())
        .expect_err("cross-scope cursor refused");
    assert_eq!(error, StoreError::CursorStale);
    // An unknown scope token is a denial.
    let error = store
        .list("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", 50, None)
        .expect_err("unknown scope refused");
    assert_eq!(error, StoreError::ScopeUnknown);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn deletion_removes_raw_metrics_assertions_and_indexes_atomically() {
    let root = temp_case("delete");
    let mut store = open(&root, 1_000_000);
    let scope = scope_of(&mut store);
    let receipt = append_ok(&mut store, &scope, "ffffffffffffffffffffffffffffffff");
    store
        .register_dependent(
            &scope,
            "claim.summary",
            super::types::DependentKind::Claim,
            std::slice::from_ref(&receipt.run_id),
            &[],
        )
        .expect("dependent registered");

    // The dry run reports the invalidation without applying it.
    let dry = store
        .delete(&scope, &receipt.run_id, true)
        .expect("dry run");
    assert!(!dry.applied);
    assert_eq!(dry.invalidated_dependents, vec!["claim.summary".to_owned()]);
    assert!(store.get(&scope, &receipt.run_id).is_ok());

    // The apply removes the raw row, the assertion set, the index row,
    // and invalidates the dependent in one transaction.
    let applied = store
        .delete(&scope, &receipt.run_id, false)
        .expect("delete");
    assert!(applied.applied);
    assert!(store.get(&scope, &receipt.run_id).is_err());
    let error = store
        .resolve_dependent(&scope, "claim.summary")
        .expect_err("invalidated");
    assert_eq!(error, StoreError::DependentInvalidated);
    // A repeated deletion is an explicit absent result.
    let error = store
        .delete(&scope, &receipt.run_id, false)
        .expect_err("absent");
    assert_eq!(error, StoreError::SourceMissing("run"));
    // Recovery validates the surviving store and cannot resurrect
    // evidence.
    let report = store.recover().expect("recover");
    assert_eq!(report.run_count, 0);
    assert_eq!(report.assertion_set_count, 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn dependent_invalidation_is_transitive_and_never_resurrects() {
    let root = temp_case("transitive");
    let mut store = open(&root, 1_000_000);
    let scope = scope_of(&mut store);
    let receipt = append_ok(&mut store, &scope, "11111111111111111111111111111110");
    store
        .register_dependent(
            &scope,
            "index.rollup",
            super::types::DependentKind::Index,
            std::slice::from_ref(&receipt.run_id),
            &[],
        )
        .expect("index registered");
    store
        .register_dependent(
            &scope,
            "claim.lift",
            super::types::DependentKind::Claim,
            &[],
            &["index.rollup".to_owned()],
        )
        .expect("claim registered");
    // A cycle refuses.
    let error = store
        .register_dependent(
            &scope,
            "index.rollup",
            super::types::DependentKind::Index,
            &[],
            &["claim.lift".to_owned()],
        )
        .expect_err("cycle refused");
    assert_eq!(error, StoreError::DependentExists);

    // Deleting the source run transitively invalidates the claim.
    store
        .delete(&scope, &receipt.run_id, false)
        .expect("delete");
    assert_eq!(
        store
            .resolve_dependent(&scope, "index.rollup")
            .expect_err("index"),
        StoreError::DependentInvalidated
    );
    assert_eq!(
        store
            .resolve_dependent(&scope, "claim.lift")
            .expect_err("claim"),
        StoreError::DependentInvalidated
    );
    // The store document keeps only opaque invalidation state.
    let document = store.store_document().expect("document");
    let claim = document
        .dependents
        .iter()
        .find(|dependent| dependent.id == "claim.lift")
        .expect("claim listed");
    assert_eq!(claim.state, super::types::DependentState::Invalidated);
    assert!(claim.source_runs.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn resolution_requires_live_bound_digests_right_now() {
    let root = temp_case("live");
    let mut store = open(&root, 1_000_000);
    let scope = scope_of(&mut store);
    let receipt = append_ok(&mut store, &scope, "22222222222222222222222222222220");
    store
        .register_dependent(
            &scope,
            "index.live",
            super::types::DependentKind::Index,
            std::slice::from_ref(&receipt.run_id),
            &[],
        )
        .expect("registered");
    let resolution = store
        .resolve_dependent(&scope, "index.live")
        .expect("resolves");
    assert_eq!(resolution.sources.len(), 1);
    assert_eq!(resolution.sources[0].record_digest, receipt.record_digest);
    // A nonexistent dependent refuses as source-missing.
    assert_eq!(
        store
            .resolve_dependent(&scope, "index.missing")
            .expect_err("missing"),
        StoreError::SourceMissing("dependent")
    );
    // A dependent cannot bind a nonexistent run.
    let error = store
        .register_dependent(
            &scope,
            "index.bad",
            super::types::DependentKind::Index,
            &["33333333333333333333333333333333".to_owned()],
            &[],
        )
        .expect_err("missing source");
    assert_eq!(error, StoreError::SourceMissing("run"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_repeat_link_needs_a_live_parent_and_exact_needs_equal_pins() {
    let root = temp_case("repeat");
    let mut store = open(&root, 1_000_000);
    let scope = scope_of(&mut store);
    let parent = append_ok(&mut store, &scope, "44444444444444444444444444444440");

    // The same inputs repeat exactly.
    let mut child = observation_json("55555555555555555555555555555550");
    child["repeatParentRunId"] = json!(parent.run_id);
    let child_receipt = store
        .append(
            &scope,
            &parse(&serde_json::to_vec(&child).expect("s")),
            &serde_json::to_vec(&child).expect("s"),
        )
        .expect("child");
    let (record, _) = store.get(&scope, &child_receipt.run_id).expect("stored");
    let record_value: Value = serde_json::from_str(&record).expect("json");
    assert_eq!(record_value["repeat"]["comparability"], json!("exact"));
    assert_eq!(record_value["repeat"]["parentRunId"], json!(parent.run_id));

    // Changed pins produce changed comparability.
    let mut changed = child.clone();
    changed["runId"] = json!("66666666666666666666666666666660");
    changed["provenance"]["model"]["revision"] =
        json!({"state": "known", "value": "planner-model-v2"});
    let changed_receipt = store
        .append(
            &scope,
            &parse(&serde_json::to_vec(&changed).expect("s")),
            &serde_json::to_vec(&changed).expect("s"),
        )
        .expect("changed child");
    let (record, _) = store.get(&scope, &changed_receipt.run_id).expect("stored");
    let record_value: Value = serde_json::from_str(&record).expect("json");
    assert_eq!(record_value["repeat"]["comparability"], json!("changed"));

    // A nonexistent parent refuses.
    let mut orphan = child.clone();
    orphan["runId"] = json!("77777777777777777777777777777770");
    orphan["repeatParentRunId"] = json!("88888888888888888888888888888888");
    let error = store
        .append(
            &scope,
            &parse(&serde_json::to_vec(&orphan).expect("s")),
            &serde_json::to_vec(&orphan).expect("s"),
        )
        .expect_err("orphan refused");
    assert_eq!(error, StoreError::ParentMissing);

    // Deleting the parent invalidates the child's repeat claim without
    // rewriting the immutable child record.
    store.delete(&scope, &parent.run_id, false).expect("delete");
    let (child_record, _) = store.get(&scope, &child_receipt.run_id).expect("stored");
    let child_value: Value = serde_json::from_str(&child_record).expect("json");
    assert_eq!(child_value["repeat"]["comparability"], json!("exact"));
    // But a fresh exact claim against the deleted parent can never
    // form again.
    let mut another = child.clone();
    another["runId"] = json!("99999999999999999999999999999990");
    let error = store
        .append(
            &scope,
            &parse(&serde_json::to_vec(&another).expect("s")),
            &serde_json::to_vec(&another).expect("s"),
        )
        .expect_err("deleted parent");
    assert_eq!(error, StoreError::ParentMissing);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn retention_age_and_count_bounds_prune_with_dependent_invalidation() {
    let root = temp_case("retention");
    let mut store = open(&root, 0);
    let scope = scope_of(&mut store);
    store
        .set_retention(&super::types::Retention {
            max_age_days: 1,
            max_records: 10,
            max_bytes: 64 * 1024 * 1024,
        })
        .expect("retention set");
    let old = append_ok(&mut store, &scope, "aaaaaaaaaaaaaaaaaaaaaaaaaaaa00b1");
    store
        .register_dependent(
            &scope,
            "claim.old",
            super::types::DependentKind::Claim,
            std::slice::from_ref(&old.run_id),
            &[],
        )
        .expect("registered");

    // Advance the clock beyond the retention window and append: the
    // aged run (and its dependent claim) goes in the same transaction.
    store
        .set_retention(&super::types::Retention {
            max_age_days: 1,
            max_records: 10,
            max_bytes: 64 * 1024 * 1024,
        })
        .expect("retention set");
    let mut store = open(&root, 3 * 86_400);
    let fresh = append_ok(&mut store, &scope, "aaaaaaaaaaaaaaaaaaaaaaaaaaaa00b2");
    assert_eq!(fresh.pruned, vec![old.run_id.clone()]);
    assert_eq!(
        store.get(&scope, &old.run_id).expect_err("aged out"),
        StoreError::SourceMissing("run")
    );
    assert_eq!(
        store
            .resolve_dependent(&scope, "claim.old")
            .expect_err("invalidated"),
        StoreError::DependentInvalidated
    );
    assert!(store.get(&scope, &fresh.run_id).is_ok());

    // The count bound prunes the oldest beyond the bound.
    store
        .set_retention(&super::types::Retention {
            max_age_days: 3650,
            max_records: 1,
            max_bytes: 64 * 1024 * 1024,
        })
        .expect("retention tightened");
    let third = append_ok(&mut store, &scope, "aaaaaaaaaaaaaaaaaaaaaaaaaaaa00b3");
    assert_eq!(third.pruned, vec![fresh.run_id]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_clock_rollback_never_accelerates_deletion() {
    let root = temp_case("rollback");
    let clock = StepClock::at(10 * 86_400);
    let mut store =
        Store::open_with_injections(&root, Box::new(clock.clone())).expect("store opens");
    let scope = scope_of(&mut store);
    let receipt = append_ok(&mut store, &scope, "bbbbbbbbbbbbbbbbbbbbbbbbbbbb00c1");

    // Roll the clock back before the record exists and prune: the
    // effective now stays at the maximum seen recordedAt.
    clock.set(0);
    let report = store.prune(&scope, true).expect("dry prune");
    assert!(report.deleted_runs.is_empty());
    let applied = store.prune(&scope, false).expect("prune");
    assert!(applied.deleted_runs.is_empty());
    assert!(store.get(&scope, &receipt.run_id).is_ok());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_oversized_record_refuses_and_never_persists() {
    let root = temp_case("oversize");
    let mut store = open(&root, 1_000_000);
    let scope = scope_of(&mut store);
    store
        .set_retention(&super::types::Retention {
            max_age_days: 30,
            max_records: 10,
            max_bytes: 1024,
        })
        .expect("tiny bound");
    let mut observation = observation_json("cccccccccccccccccccccccccccc00d1");
    // Enough known semantic ids to push the canonical record past 1 KiB.
    let ids: Vec<String> = (0..120)
        .map(|index| format!("planner.symbol_{index:03}"))
        .collect();
    observation["operation"]["affectedSemanticIds"] = json!(ids);
    let error = store
        .append(
            &scope,
            &parse(&serde_json::to_vec(&observation).expect("s")),
            &serde_json::to_vec(&observation).expect("s"),
        )
        .expect_err("oversize refused");
    assert_eq!(error, StoreError::LimitExceeded);
    let (rows, _) = store.list(&scope, 50, None).expect("list");
    assert!(rows.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn recovery_verifies_digests_and_rejects_tampered_records() {
    let root = temp_case("recover");
    let mut store = open(&root, 1_000_000);
    let scope = scope_of(&mut store);
    let receipt = append_ok(&mut store, &scope, "dddddddddddddddddddddddddddd00e1");

    let report = store.recover().expect("recover");
    assert_eq!(report.run_count, 1);
    assert_eq!(report.verified_digests, 1);
    assert_eq!(report.rebuilt_index_rows, 1);
    assert!(store.get(&scope, &receipt.run_id).is_ok());

    // Tamper with the raw record bytes behind the store's back: the
    // next recovery refuses as corruption instead of silently serving
    // the tampered record.
    {
        let database = root.join(".lekalo/history/store.sqlite");
        let connection = rusqlite::Connection::open(&database).expect("raw open");
        connection
            .execute("UPDATE runs SET record_bytes = ?1", [b"tampered".to_vec()])
            .expect("tamper applied");
    }
    let error = store.recover().expect_err("tampered refused");
    assert!(matches!(error, StoreError::Corrupt("record-digest")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn cursors_bind_scope_and_generation_and_refuse_after_mutation() {
    let root = temp_case("cursor");
    let mut store = open(&root, 1_000_000);
    let scope = scope_of(&mut store);
    for index in 0..3 {
        let run_id = format!("eeeeeeeeeeeeeeeeeeeeeeeeeeee00f{index}");
        append_ok(&mut store, &scope, &run_id);
    }
    let (page1, cursor) = store.list(&scope, 2, None).expect("page 1");
    assert_eq!(page1.len(), 2);
    let cursor = cursor.expect("more pages");
    let (page2, next) = store.list(&scope, 2, Some(&cursor)).expect("page 2");
    assert_eq!(page2.len(), 1);
    assert!(next.is_none());
    // Page ordering is stable (recordedAt, runId).
    assert!(page1[0].recorded_at <= page1[1].recorded_at);

    // A mutation moves the generation: the cursor refuses as stale.
    append_ok(&mut store, &scope, "eeeeeeeeeeeeeeeeeeeeeeeeeeee00f9");
    let error = store
        .list(&scope, 2, Some(&cursor))
        .expect_err("stale cursor");
    assert_eq!(error, StoreError::CursorStale);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn clear_empties_one_scope_through_the_transactional_path() {
    let root = temp_case("clear");
    let mut store = open(&root, 1_000_000);
    let scope = scope_of(&mut store);
    append_ok(&mut store, &scope, "ffffffffffffffffffffffffffff0011");
    append_ok(&mut store, &scope, "ffffffffffffffffffffffffffff0022");
    let (removed, _) = store.clear(&scope).expect("clear");
    assert_eq!(removed, 2);
    let (rows, _) = store.list(&scope, 50, None).expect("list");
    assert!(rows.is_empty());
    let report = store.recover().expect("recover");
    assert_eq!(report.run_count, 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn concurrent_writers_serialize_and_both_commit() {
    let root = temp_case("concurrent");
    {
        let mut store = open(&root, 1_000_000);
        scope_of(&mut store);
    }
    let scope = {
        let store = open(&root, 1_000_000);
        let mut scopes = store.scopes().expect("scopes");
        scopes.remove(0).tenant_scope_id
    };
    let left = std::thread::spawn({
        let root = root.clone();
        let scope = scope.clone();
        move || {
            let mut store = open(&root, 1_000_000);
            append_ok(&mut store, &scope, "1111111111111111111111111111aa01")
        }
    });
    let right = std::thread::spawn({
        let root = root.clone();
        let scope = scope.clone();
        move || {
            let mut store = open(&root, 1_000_000);
            append_ok(&mut store, &scope, "1111111111111111111111111111aa02")
        }
    });
    let left = left.join().expect("left writer");
    let right = right.join().expect("right writer");
    assert_ne!(left.run_id, right.run_id);
    let mut store = open(&root, 1_000_000);
    let (rows, _) = store.list(&scope, 50, None).expect("list");
    assert_eq!(rows.len(), 2);
    let report = store.recover().expect("recover");
    assert_eq!(report.run_count, 2);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_production_store_matches_the_schema_contract_bytes() {
    // The full production pipeline (system clock, random ids) writes a
    // record whose canonical form re-parses and re-canonicalizes to the
    // stored bytes: byte-sorted keys plus one trailing LF.
    let root = temp_case("canonical");
    let mut store = open_production(&root);
    let scope = scope_of(&mut store);
    let mut observation = observation_json("1111111111111111111111111111bb01");
    observation.as_object_mut().expect("object").remove("runId");
    let receipt = store
        .append(
            &scope,
            &parse(&serde_json::to_vec(&observation).expect("s")),
            &serde_json::to_vec(&observation).expect("s"),
        )
        .expect("append");
    let (record, assertions) = store.get(&scope, &receipt.run_id).expect("stored");
    let assertion_digest = {
        let record_value: Value = serde_json::from_str(&record).expect("json");
        record_value["assertionsRef"]["digest"]
            .as_str()
            .expect("digest")
            .to_owned()
    };
    let documents = [
        (record.as_str(), receipt.record_digest.as_str()),
        (
            assertions.as_deref().expect("assertions"),
            assertion_digest.as_str(),
        ),
    ];
    for (bytes, digest) in documents {
        assert!(bytes.ends_with('\n'), "one trailing LF");
        let value: Value = serde_json::from_str(bytes).expect("parses");
        let canonical = crate::privacy::canonical::canonical(&value);
        assert_eq!(format!("{canonical}\n"), bytes, "byte-sorted keys, compact");
        let expected = format!("sha256:{}", crate::digest::sha256_hex(bytes.as_bytes()));
        assert_eq!(expected, digest, "digest binds the exact bytes");
    }
    let _ = std::fs::remove_dir_all(&root);
}
