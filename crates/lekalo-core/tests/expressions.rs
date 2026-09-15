//! Issue #66 integration: the fixture attachments, the exhaustive
//! static-typing rejections, the deterministic reference evaluator
//! over the shared cross-target vectors, canonical stability, the
//! semantic diff classes, and the managed-mode capability gate.

use lekalo_core::expressions::{
    canonical, canonical_bytes, check_builtin_support, compare, evaluate, render_program, Bindings,
    BuiltinSupport, DiffClass, ExpressionsAttachment, Target, VectorExpect, VectorsDocument,
};

const VALID: &[u8] = include_bytes!("../../../tests/fixtures/expressions/valid/planner.json");
const VECTORS: &[u8] = include_bytes!("../../../tests/fixtures/expressions/vectors.json");
const DIFF_BASE: &[u8] = include_bytes!("../../../tests/fixtures/expressions/diff/base.json");
const DIFF_CANDIDATE: &[u8] =
    include_bytes!("../../../tests/fixtures/expressions/diff/candidate.json");
const SUPPORT_FULL: &[u8] =
    include_bytes!("../../../tests/fixtures/expressions/builtin-support/full.json");
const SUPPORT_CORE: &[u8] =
    include_bytes!("../../../tests/fixtures/expressions/builtin-support/core-only.json");

/// Every invalid fixture with its single registered rule.
const INVALID: &[(&str, &str)] = &[
    ("unknown-top-field", "expression.input-invalid"),
    ("wrong-identity", "expression.input-invalid"),
    ("wrong-builtin-semantics", "expression.input-invalid"),
    ("bad-expression-id", "expression.contract-invalid"),
    ("duplicate-expression", "expression.contract-invalid"),
    ("eq-type-mismatch", "expression.type-invalid"),
    ("compare-unordered", "expression.type-invalid"),
    ("unknown-builtin", "expression.type-invalid"),
    ("builtin-arity", "expression.type-invalid"),
    ("builtin-arg-type", "expression.type-invalid"),
    ("nullable-unguarded", "expression.type-invalid"),
    ("null-operand", "expression.type-invalid"),
    ("condition-result", "expression.contract-invalid"),
    ("condition-target", "expression.contract-invalid"),
    ("assignment-target-missing", "expression.contract-invalid"),
    ("assignment-type-mismatch", "expression.contract-invalid"),
    ("depth-exceeded", "expression.complexity-limit"),
    ("duplicate-param", "expression.contract-invalid"),
    ("span-escape", "expression.input-invalid"),
    ("set-duplicate", "expression.input-invalid"),
    ("arithmetic-type", "expression.type-invalid"),
    ("membership-type", "expression.type-invalid"),
    ("if-branch-type", "expression.type-invalid"),
    ("unknown-record-field", "expression.input-invalid"),
    ("param-unknown", "expression.type-invalid"),
    ("int-literal-bound", "expression.input-invalid"),
    ("datetime-literal-shape", "expression.input-invalid"),
    ("target-readonly-scope", "expression.contract-invalid"),
];

fn parse(bytes: &[u8]) -> ExpressionsAttachment {
    let json: serde_json::Value = serde_json::from_slice(bytes).expect("fixture json");
    ExpressionsAttachment::from_value(&json).expect("valid fixture")
}

#[test]
fn the_planner_fixture_parses_with_the_full_grammar() {
    let attachment = parse(VALID);
    assert_eq!(attachment.project_id().as_str(), "planner");
    assert_eq!(attachment.builtin_semantics(), "0.2.16");
    assert_eq!(attachment.expressions().len(), 39);
    let conditions = attachment
        .expressions()
        .iter()
        .filter(|record| record.kind().key() == "condition")
        .count();
    assert_eq!(conditions, 19);
    // The span of the overdue record is declared and canonical bytes
    // carry it (explainable diagnostics have a source anchor).
    let overdue = attachment
        .expression("expr.planner/overdue-check")
        .expect("record");
    assert!(overdue.span().is_some());
    let bytes = canonical_bytes(&attachment).expect("canonical");
    assert!(bytes.contains("\"file\":\"modules/planner/expressions.yaml\""));
    // Determinism: two renders are byte-identical and round-trip.
    assert_eq!(bytes, canonical_bytes(&attachment).expect("canonical"));
    let reparsed: serde_json::Value = serde_json::from_str(&bytes).expect("canonical parses");
    let again = ExpressionsAttachment::from_value(&reparsed).expect("revalid");
    assert_eq!(canonical_bytes(&again).expect("canonical"), bytes);
}

#[test]
fn every_invalid_fixture_rejects_with_its_registered_rule() {
    for (name, rule) in INVALID {
        let path = format!("../../tests/fixtures/expressions/invalid/{name}.json");
        let bytes = std::fs::read(path).expect("fixture");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("fixture json");
        let rejection = ExpressionsAttachment::from_value(&json).expect_err(name);
        assert_eq!(rejection.reason_ids(), vec![*rule], "fixture {name}");
    }
}

#[test]
fn the_reference_evaluator_passes_the_shared_vectors() {
    let attachment = parse(VALID);
    let json: serde_json::Value = serde_json::from_slice(VECTORS).expect("vectors json");
    let vectors = VectorsDocument::from_value(&json).expect("vectors decode");
    assert_eq!(vectors.vectors.len(), 91);
    let mut matched = 0usize;
    for vector in &vectors.vectors {
        let record = attachment
            .expression(&vector.expression)
            .unwrap_or_else(|| panic!("vector {} references unknown expression", vector.id));
        let bindings = Bindings::from_json(record, &vector.bindings)
            .unwrap_or_else(|set| panic!("vector {} bindings: {:?}", vector.id, set.reason_ids()));
        let clock = vector
            .clock
            .map(lekalo_core::expressions::Clock::from_seconds)
            // The wire-legal omitted-clock default: the reference and
            // every generated target read the shared epoch.
            .unwrap_or_else(|| {
                lekalo_core::expressions::Clock::from_datetime("1970-01-01T00:00:00Z")
                    .expect("epoch")
            });
        match &vector.expect {
            VectorExpect::Value(expected) => {
                let actual = evaluate(record, &bindings, &clock).unwrap_or_else(|set| {
                    panic!("vector {} failed: {:?}", vector.id, set.reason_ids())
                });
                assert_eq!(
                    actual.to_json(),
                    expected.to_json(),
                    "vector {} value",
                    vector.id
                );
            }
            VectorExpect::Error(expected) => {
                let rejection = evaluate(record, &bindings, &clock).expect_err(&vector.id);
                assert!(
                    rejection
                        .as_slice()
                        .iter()
                        .any(|d| d.id() == "expression.eval-invalid"
                            && serde_json::to_string(d.data().get("detail").unwrap_or(
                                &lekalo_core::diagnostics::DataValue::Token(String::new())
                            ))
                            .unwrap_or_default()
                            .contains(expected.key())),
                    "vector {} expected error {}",
                    vector.id,
                    expected.key()
                );
            }
        }
        matched += 1;
    }
    assert_eq!(matched, vectors.vectors.len());
}

#[test]
fn the_clock_is_injected_and_deterministic() {
    let attachment = parse(VALID);
    let record = attachment
        .expression("expr.planner/overdue-check")
        .expect("record");
    assert!(record.uses_now());
    let json: serde_json::Value = serde_json::from_slice(VECTORS).expect("vectors json");
    let vectors = VectorsDocument::from_value(&json).expect("vectors");
    let vector = vectors
        .vectors
        .iter()
        .find(|vector| vector.id == "overdue-past-open")
        .expect("vector");
    let bindings = Bindings::from_json(record, &vector.bindings).expect("bindings");
    // The same injected instant reproduces the same verdict; a
    // different instant changes it. No wall clock is read.
    let past =
        lekalo_core::expressions::Clock::from_datetime("2026-09-12T00:00:00Z").expect("clock");
    assert_eq!(
        evaluate(record, &bindings, &past)
            .expect("evaluates")
            .to_json(),
        serde_json::json!(true)
    );
    let early =
        lekalo_core::expressions::Clock::from_datetime("2026-09-01T00:00:00Z").expect("clock");
    assert_eq!(
        evaluate(record, &bindings, &early)
            .expect("evaluates")
            .to_json(),
        serde_json::json!(false)
    );
}

#[test]
fn the_diff_classifies_removals_bodies_and_additions() {
    let base = parse(DIFF_BASE);
    let candidate = parse(DIFF_CANDIDATE);
    let result = compare(&base, &candidate).expect("compares");
    assert!(!result.equal());
    let breaking = result.paths().iter().any(|path| {
        path.path() == "expr.planner/title-open" && path.class() == DiffClass::Breaking
    });
    assert!(breaking, "removed expression breaks");
    let policy = result.paths().iter().any(|path| {
        path.path() == "expr.planner/search-match/body" && path.class() == DiffClass::PolicyChange
    });
    assert!(policy, "body change reshapes policy");
    let added = result.paths().iter().any(|path| {
        path.path() == "expr.planner/audit-tag" && path.class() == DiffClass::NonBreaking
    });
    assert!(added, "added expression is non-breaking");
    let narrowed = result.paths().iter().any(|path| {
        path.path() == "expr.planner/optional-note/params/input.note"
            && path.class() == DiffClass::Breaking
    });
    assert!(narrowed, "nullable widened to required breaks callers");
}

#[test]
fn managed_mode_blocks_builtins_the_target_does_not_declare() {
    let attachment = parse(VALID);
    let full_json: serde_json::Value = serde_json::from_slice(SUPPORT_FULL).expect("support json");
    let full = BuiltinSupport::from_value(&full_json).expect("support decodes");
    assert!(check_builtin_support(&attachment, &full).is_ok());
    let core_json: serde_json::Value = serde_json::from_slice(SUPPORT_CORE).expect("support json");
    let core_only = BuiltinSupport::from_value(&core_json).expect("support decodes");
    let rejection = check_builtin_support(&attachment, &core_only).expect_err("builtins missing");
    assert!(rejection
        .as_slice()
        .iter()
        .any(|d| d.id() == "expression.builtin-unsupported"));
    // The required capability set is closed and sorted.
    let capabilities = attachment.required_capabilities();
    assert!(capabilities.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(
        capabilities.first().map(String::as_str),
        Some("expression.builtin/cast-int-to-string")
    );
    assert!(capabilities.contains(&"expression.core".to_owned()));
    assert_eq!(capabilities.len(), 16);
}

#[test]
fn every_target_renders_a_deterministic_program() {
    let attachment = parse(VALID);
    for target in [Target::Node, Target::Php, Target::Go] {
        let first = render_program(&attachment, target);
        let second = render_program(&attachment, target);
        assert_eq!(first, second, "{target:?} renders deterministically");
        assert!(first.contains("expr.planner/overdue-check"), "{target:?}");
        assert!(first.contains("lekFail"), "{target:?}");
    }
}

#[test]
fn vector_expectations_are_compared_in_their_declared_encoding() {
    // Duration results encode as integer seconds and datetime
    // results as canonical strings; the shared fixture covers both
    // through review-due and the arithmetic records.
    let attachment = parse(VALID);
    let record = attachment
        .expression("expr.planner/review-due")
        .expect("record");
    let bindings_json = serde_json::json!({"entity": {"reviewed_at": "2026-09-01T00:00:00Z"}});
    let bindings = Bindings::from_json(record, &bindings_json).expect("bindings");
    let clock =
        lekalo_core::expressions::Clock::from_datetime("2026-09-12T00:00:00Z").expect("clock");
    let outcome = evaluate(record, &bindings, &clock).expect("evaluates");
    assert_eq!(outcome.to_json(), serde_json::json!("2026-09-08T00:00:00Z"));
    let _ = canonical::canonical_bytes(&attachment).expect("canonical");
}

/// The acceptance proof that closes the loop: every rendered target
/// program is actually executed against the shared vector document,
/// and its computed results must match the declared expectations
/// byte-for-byte in the wire encoding. A missing interpreter skips
/// that target loudly (the CI Linux image ships all three); a wrong
/// result always fails.
#[test]
fn every_generated_program_executes_the_shared_vectors_identically() {
    use std::process::{Command, Stdio};

    let attachment = parse(VALID);
    let vectors_json: serde_json::Value = serde_json::from_slice(VECTORS).expect("vectors json");

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-exec-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");

    let vectors_path = dir.join("vectors.json");
    std::fs::write(&vectors_path, VECTORS).expect("write vectors");

    let available = |tool: &str, probe: &[&str]| {
        Command::new(tool)
            .args(probe)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    };
    let node = available("node", &["--version"]);
    let php = available("php", &["--version"]);
    // `go --version` is not a command; the tool probes with `go version`.
    let go = available("go", &["version"]);
    assert!(node, "node must be available to execute the shared vectors");

    let mut executed: Vec<&'static str> = Vec::new();
    for (target, file, tool, args) in [
        (Target::Node, "planner.cjs", "node", vec![]),
        (Target::Php, "planner.php", "php", vec![]),
        (Target::Go, "planner.go", "go", vec!["run".to_owned()]),
    ] {
        let present = match tool {
            "node" => node,
            "php" => php,
            _ => go,
        };
        if !present {
            eprintln!("skipping target {target:?}: {tool} is not on this host");
            continue;
        }
        let program = render_program(&attachment, target);
        let path = dir.join(file);
        std::fs::write(&path, program).expect("write program");

        let vectors_file = std::fs::File::open(&vectors_path).expect("open vectors");
        let output = Command::new(tool)
            .args(&args)
            .arg(&path)
            .stdin(Stdio::from(vectors_file))
            .output()
            .expect("run the generated program");
        assert!(
            output.status.success(),
            "{tool} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("utf8 results");
        let envelope: serde_json::Value = serde_json::from_str(&stdout).expect("results envelope");
        let rows = envelope["results"].as_array().expect("results array");

        let cases = vectors_json["vectors"].as_array().expect("vectors");
        assert_eq!(rows.len(), cases.len(), "{tool}: one row per vector");
        for (row, case) in rows.iter().zip(cases.iter()) {
            assert_eq!(row["id"], case["id"], "{tool}: row order matches");
            let expect = &case["expect"];
            if let Some(expected_error) = expect.get("error") {
                assert_eq!(
                    &row["error"], expected_error,
                    "{tool}: {} must fail identically",
                    case["id"]
                );
            } else {
                assert_eq!(
                    &row["value"], &expect["value"],
                    "{tool}: {} must compute identically",
                    case["id"]
                );
            }
        }
        executed.push(tool);
    }
    eprintln!("executed targets on this host: {executed:?}");
    let _ = canonical_bytes(&attachment).expect("canonical");
}
