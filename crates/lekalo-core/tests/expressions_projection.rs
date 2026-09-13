//! Issue #66 executed cross-target regressions for the projection
//! boundaries the shared vector document cannot carry. The vectors
//! contract only accepts evaluation-stage error tokens, so
//! binding-stage refusals (duplicate set members, malformed datetime
//! calendar fields), clock presence validation, raw number
//! spellings, and binding-root shapes have no shared vector; here
//! every generated Node/PHP/Go program is executed against those
//! exact shapes and its emitted row token must equal the closed
//! token the reference refuses with — no garbage value may reach
//! stdout.

use std::process::{Command, Stdio};

use lekalo_core::expressions::{
    evaluate, render_program, Bindings, Clock, ExpressionsAttachment, Target, VectorsDocument,
};

const VALID: &[u8] = include_bytes!("../../../tests/fixtures/expressions/valid/planner.json");

fn parse() -> ExpressionsAttachment {
    let json: serde_json::Value = serde_json::from_slice(VALID).expect("fixture json");
    ExpressionsAttachment::from_value(&json).expect("valid fixture")
}

/// One probe row: the raw vector JSON the generated programs receive,
/// plus the reference outcome it must agree with.
struct Probe {
    id: &'static str,
    expression: &'static str,
    clock: Option<&'static str>,
    bindings: serde_json::Value,
    /// `Ok(json)` — the reference computes this value; `Err(token)` —
    /// the reference refuses and every target must emit this token.
    expect: Result<&'static str, &'static str>,
}

fn probes() -> Vec<Probe> {
    let json = |text: &str| serde_json::from_str(text).expect("probe bindings");
    vec![
        // Duplicate set members refuse identically in every target.
        Probe {
            id: "dup-int",
            expression: "expr.planner/number-set",
            clock: None,
            bindings: json(r#"{"input": {"nums": [1, 2, 2]}}"#),
            expect: Err("binding-value"),
        },
        Probe {
            id: "dup-bool",
            expression: "expr.planner/flag-set",
            clock: None,
            bindings: json(r#"{"input": {"flags": [true, true]}}"#),
            expect: Err("binding-value"),
        },
        Probe {
            id: "dup-datetime",
            expression: "expr.planner/legacy-days",
            clock: None,
            bindings: json(
                r#"{"entity": {"days": ["2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"]}}"#,
            ),
            expect: Err("binding-value"),
        },
        Probe {
            id: "dup-duration",
            expression: "expr.planner/span-set",
            clock: None,
            bindings: json(r#"{"input": {"spans": [10, 10]}}"#),
            expect: Err("binding-value"),
        },
        // Malformed datetime calendar fields refuse as binding-value,
        // never as a garbage epoch.
        Probe {
            id: "malformed-leap-day",
            expression: "expr.planner/archive-year",
            clock: None,
            bindings: json(r#"{"entity": {"created": "2025-02-29T00:00:00Z"}}"#),
            expect: Err("binding-value"),
        },
        Probe {
            id: "malformed-month",
            expression: "expr.planner/archive-year",
            clock: None,
            bindings: json(r#"{"entity": {"created": "2026-13-01T00:00:00Z"}}"#),
            expect: Err("binding-value"),
        },
        Probe {
            id: "malformed-day",
            expression: "expr.planner/archive-year",
            clock: None,
            bindings: json(r#"{"entity": {"created": "2026-02-30T00:00:00Z"}}"#),
            expect: Err("binding-value"),
        },
        Probe {
            id: "malformed-hour",
            expression: "expr.planner/archive-year",
            clock: None,
            bindings: json(r#"{"entity": {"created": "2026-01-01T24:00:00Z"}}"#),
            expect: Err("binding-value"),
        },
        Probe {
            id: "malformed-second",
            expression: "expr.planner/archive-year",
            clock: None,
            bindings: json(r#"{"entity": {"created": "2026-01-01T00:00:60Z"}}"#),
            expect: Err("binding-value"),
        },
        Probe {
            id: "malformed-set-member",
            expression: "expr.planner/legacy-days",
            clock: None,
            bindings: json(
                r#"{"entity": {"days": ["2026-01-01T00:00:00Z", "2026-00-01T00:00:00Z"]}}"#,
            ),
            expect: Err("binding-value"),
        },
        // The leap-day boundary is accepted and computes as a value.
        Probe {
            id: "leap-day-ok",
            expression: "expr.planner/archive-year",
            clock: None,
            bindings: json(r#"{"entity": {"created": "2024-02-29T12:00:00Z"}}"#),
            expect: Ok("false"),
        },
        // Signed and positive-signed datetime fields refuse as
        // binding-value in scalar and set bindings.
        Probe {
            id: "date-sign",
            expression: "expr.planner/archive-year",
            clock: None,
            bindings: json(r#"{"entity": {"created": "2026-01-01T-1:00:00Z"}}"#),
            expect: Err("binding-value"),
        },
        Probe {
            id: "date-plus",
            expression: "expr.planner/archive-year",
            clock: None,
            bindings: json(r#"{"entity": {"created": "2026-+1-01T00:00:00Z"}}"#),
            expect: Err("binding-value"),
        },
        Probe {
            id: "date-newline",
            expression: "expr.planner/archive-year",
            clock: None,
            bindings: json("{\"entity\": {\"created\": \"2026-01-01T00:00:00Z\\n\"}}"),
            expect: Err("binding-value"),
        },
        Probe {
            id: "set-datetime-sign",
            expression: "expr.planner/legacy-days",
            clock: None,
            bindings: json(
                r#"{"entity": {"days": ["2026-01-01T00:00:00Z", "2026-01-01T-2:00:00Z"]}}"#,
            ),
            expect: Err("binding-value"),
        },
        // The family value-domain bounds refuse as binding-value.
        Probe {
            id: "int-bound",
            expression: "expr.planner/spread",
            clock: None,
            bindings: json(r#"{"input": {"total": 9007199254740992, "parts": 1}}"#),
            expect: Err("binding-value"),
        },
        Probe {
            id: "duration-bound",
            expression: "expr.planner/period-check",
            clock: None,
            bindings: json(
                r#"{"entity": {"age": 31536000001, "created": "2026-01-01T00:00:00Z"}}"#,
            ),
            expect: Err("binding-value"),
        },
        Probe {
            id: "set-duration-bound",
            expression: "expr.planner/span-set",
            clock: None,
            bindings: json(r#"{"input": {"spans": [10, 31536000001]}}"#),
            expect: Err("binding-value"),
        },
        Probe {
            id: "string-control-char",
            expression: "expr.planner/wide-label",
            clock: None,
            bindings: json(r#"{"input": {"left": "ab\tc", "right": "d"}}"#),
            expect: Err("binding-value"),
        },
        Probe {
            id: "string-non-bmp",
            expression: "expr.planner/copy-tag",
            clock: None,
            bindings: json("{\"input\": {\"kind\": \"\u{1F600}\"}}"),
            expect: Err("binding-value"),
        },
        Probe {
            id: "string-byte-bound",
            expression: "expr.planner/copy-tag",
            clock: None,
            bindings: serde_json::json!({"input": {"kind": "x".repeat(300)}}),
            expect: Err("binding-value"),
        },
        Probe {
            id: "set-size-bound",
            expression: "expr.planner/number-set",
            clock: None,
            bindings: serde_json::json!({"input": {"nums": (0i64..65).collect::<Vec<i64>>()}}),
            expect: Err("binding-value"),
        },
        // Structural refusals: unknown scope, unknown field, missing
        // declared (nullable included), null on a required reference,
        // and non-object scopes.
        Probe {
            id: "unknown-scope",
            expression: "expr.planner/spread",
            clock: None,
            bindings: json(r#"{"input": {"total": 7, "parts": 2}, "bogus": {}}"#),
            expect: Err("binding-unknown"),
        },
        Probe {
            id: "unknown-field",
            expression: "expr.planner/spread",
            clock: None,
            bindings: json(r#"{"input": {"total": 7, "parts": 2, "bogus": 1}}"#),
            expect: Err("binding-unknown"),
        },
        Probe {
            id: "missing-nullable",
            expression: "expr.planner/overdue-check",
            clock: None,
            bindings: json(r#"{"input": {"state": "open"}}"#),
            expect: Err("binding-missing"),
        },
        Probe {
            id: "missing-required",
            expression: "expr.planner/spread",
            clock: None,
            bindings: json(r#"{"input": {"total": 7}}"#),
            expect: Err("binding-missing"),
        },
        Probe {
            id: "null-required",
            expression: "expr.planner/spread",
            clock: None,
            bindings: json(r#"{"input": {"total": null, "parts": 1}}"#),
            expect: Err("binding-null"),
        },
        Probe {
            id: "scope-shape",
            expression: "expr.planner/overdue-check",
            clock: None,
            bindings: json(r#"{"input": 5}"#),
            expect: Err("bindings-shape"),
        },
        // Invalid values on untaken branches must still refuse:
        // short-circuiting the body must not short-circuit the
        // binding validation stage.
        Probe {
            id: "skipped-duplicate-set",
            expression: "expr.planner/state-gate",
            clock: None,
            bindings: json(
                r#"{"input": {"state": "done", "project_tags": ["a", "a"]}, "actor": {"role": null}}"#,
            ),
            expect: Err("binding-value"),
        },
        Probe {
            id: "skipped-mistyped-sibling",
            expression: "expr.planner/overdue-check",
            clock: None,
            bindings: json(r#"{"input": {"due": null, "state": 5}}"#),
            expect: Err("binding-value"),
        },
        // An omitted clock on a now-reading body evaluates at the
        // shared epoch default in every target.
        Probe {
            id: "omitted-clock-epoch",
            expression: "expr.planner/overdue-check",
            clock: None,
            bindings: json(r#"{"input": {"due": "1960-01-01T00:00:00Z", "state": "open"}}"#),
            expect: Ok("true"),
        },
        // Membership evaluates the operand before the set (the
        // reference order): with both children failing, the operand's
        // closed domain token wins in every target.
        Probe {
            id: "order-operand-div",
            expression: "expr.planner/set-order-operand",
            clock: None,
            bindings: json(r#"{}"#),
            expect: Err("divide-by-zero"),
        },
        Probe {
            id: "order-operand-mod",
            expression: "expr.planner/set-order-operand-rev",
            clock: None,
            bindings: json(r#"{}"#),
            expect: Err("modulo-by-zero"),
        },
        // A field legally named `constructor` is an ordinary declared
        // reference: an empty scope object refuses binding-missing,
        // never the inherited Object.prototype member.
        Probe {
            id: "ctor-absent",
            expression: "expr.planner/ctor-guard",
            clock: None,
            bindings: json(r#"{"input": {}}"#),
            expect: Err("binding-missing"),
        },
        // The reference's two structural passes keep their order:
        // every scope name and shape is checked before any field
        // name, and every field name before any declared value, so a
        // multi-defect document refuses with the reference token.
        Probe {
            id: "validation-order-shape",
            expression: "expr.planner/spread",
            clock: None,
            bindings: json(r#"{"actor": {"bogus": 1}, "input": 5}"#),
            expect: Err("bindings-shape"),
        },
        Probe {
            id: "validation-order-name-shape",
            expression: "expr.planner/spread",
            clock: None,
            bindings: json(r#"{"input": 5, "zscope": {}}"#),
            expect: Err("bindings-shape"),
        },
        Probe {
            id: "validation-order-unknown",
            expression: "expr.planner/spread",
            clock: None,
            bindings: json(r#"{"input": {"bogus": 1}}"#),
            expect: Err("binding-unknown"),
        },
    ]
}

/// The generated program text of one target written to a temp file.
fn write_program(
    dir: &std::path::Path,
    attachment: &ExpressionsAttachment,
    target: Target,
) -> std::path::PathBuf {
    let file = match target {
        Target::Node => "probe.cjs",
        Target::Php => "probe.php",
        Target::Go => "probe.go",
    };
    let path = dir.join(file);
    std::fs::write(&path, render_program(attachment, target)).expect("write program");
    path
}

fn available(tool: &str, probe: &[&str]) -> bool {
    Command::new(tool)
        .args(probe)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// The consumer-visible outcome one generated target must produce
/// for one raw document: refuse the document outright (nonzero exit,
/// empty stdout), emit one row carrying the closed token, or compute
/// the reference value.
#[derive(Clone, Copy, PartialEq)]
enum Outcome {
    Doc,
    Row(&'static str),
    Value(&'static str),
}

/// Run one generated program over one raw document and require the
/// declared outcome. `context` names the probe in every failure
/// message.
fn assert_target_outcome(
    tool: &str,
    args: &[String],
    program: &std::path::Path,
    document: &str,
    path: &std::path::Path,
    expected: Outcome,
    context: &str,
) {
    std::fs::write(path, document).expect("write probe document");
    let vectors_file = std::fs::File::open(path).expect("open probe document");
    let output = Command::new(tool)
        .args(args)
        .arg(program)
        .stdin(Stdio::from(vectors_file))
        .output()
        .unwrap_or_else(|error| panic!("{context}: run the generated program: {error}"));
    match expected {
        Outcome::Doc => {
            assert!(
                !output.status.success(),
                "{context}: must refuse the document outright, stdout: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            assert!(
                output.stdout.is_empty(),
                "{context}: refused the document but still printed results"
            );
        }
        Outcome::Row(token) => {
            assert!(
                output.status.success(),
                "{context}: expected row token {token} but the program exited with {:?} and stderr {:?}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).expect("utf8 results");
            let envelope: serde_json::Value =
                serde_json::from_str(&stdout).unwrap_or_else(|error| {
                    panic!("{context}: results envelope: {error}; stdout: {stdout}")
                });
            let rows = envelope["results"].as_array().expect("results array");
            assert_eq!(rows.len(), 1, "{context}: one row");
            assert_eq!(rows[0]["id"], "probe", "{context}: row identity");
            assert_eq!(
                rows[0]["error"], token,
                "{context}: must refuse with the closed token"
            );
            assert!(
                rows[0].get("value").is_none(),
                "{context}: emitted a value next to the refusal"
            );
        }
        Outcome::Value(expected_json) => {
            assert!(
                output.status.success(),
                "{context}: must compute, stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).expect("utf8 results");
            let envelope: serde_json::Value =
                serde_json::from_str(&stdout).expect("results envelope");
            let rows = envelope["results"].as_array().expect("results array");
            assert_eq!(rows.len(), 1, "{context}: one row");
            assert!(
                rows[0].get("error").is_none(),
                "{context}: computed but emitted an error: {:?}",
                rows[0]["error"]
            );
            let expected: serde_json::Value =
                serde_json::from_str(expected_json).expect("expected json");
            assert_eq!(
                rows[0]["value"], expected,
                "{context}: must compute the reference value"
            );
        }
    }
}

/// The reference outcome of one probe row.
fn reference_outcome(
    attachment: &ExpressionsAttachment,
    probe: &Probe,
) -> Result<serde_json::Value, String> {
    let detail_of = |set: &lekalo_core::diagnostics::DiagnosticSet| {
        set.as_slice()
            .iter()
            .filter_map(|d| {
                serde_json::to_string(
                    d.data()
                        .get("detail")
                        .unwrap_or(&lekalo_core::diagnostics::DataValue::Token(String::new())),
                )
                .ok()
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let record = attachment
        .expression(probe.expression)
        .unwrap_or_else(|| panic!("probe {} references unknown expression", probe.id));
    let bindings = match Bindings::from_json(record, &probe.bindings) {
        Ok(bindings) => bindings,
        Err(set) => {
            let detail = detail_of(&set);
            let expected = match &probe.expect {
                Err(token) => *token,
                Ok(_) => panic!(
                    "probe {} bindings refused but a value was expected",
                    probe.id
                ),
            };
            assert!(
                detail.contains(expected),
                "probe {} must refuse with {expected}, got {detail}",
                probe.id
            );
            return Err(expected.to_owned());
        }
    };
    let clock = probe
        .clock
        .map(Clock::from_datetime)
        .unwrap_or_else(|| Some(Clock::from_datetime("1970-01-01T00:00:00Z").expect("epoch")))
        .expect("clock");
    match evaluate(record, &bindings, &clock) {
        Ok(value) => Ok(value.to_json()),
        Err(set) => Err(detail_of(&set)),
    }
}

#[test]
fn every_generated_program_refuses_binding_stage_shapes_identically() {
    let attachment = parse();
    let probes = probes();

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-projection-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");

    // The document the targets receive. Clocks here are either
    // absent (the shared epoch default on a now-reading body) or
    // canonical; a malformed clock makes the reference refuse the
    // whole document at decode, which the dedicated clock test pins
    // separately.
    let mut document = serde_json::Map::new();
    document.insert(
        "schemaVersion".to_owned(),
        serde_json::json!("lekalo/expressions/vectors/v1.0.0"),
    );
    let rows: Vec<serde_json::Value> = probes
        .iter()
        .map(|probe| {
            let mut row = serde_json::Map::new();
            row.insert("id".to_owned(), serde_json::json!(probe.id));
            row.insert("expression".to_owned(), serde_json::json!(probe.expression));
            if let Some(clock) = probe.clock {
                row.insert("clock".to_owned(), serde_json::json!(clock));
            }
            row.insert("bindings".to_owned(), probe.bindings.clone());
            serde_json::Value::Object(row)
        })
        .collect();
    document.insert("vectors".to_owned(), serde_json::Value::Array(rows));
    let document = serde_json::Value::Object(document);
    let document_path = dir.join("probe-vectors.json");
    std::fs::write(&document_path, document.to_string()).expect("write probe document");

    // Reference side first: compute the authoritative outcome of every
    // probe and require the closed refusal token where declared.
    for probe in &probes {
        let outcome = reference_outcome(&attachment, probe);
        match (&probe.expect, outcome) {
            (Ok(expected), Ok(value)) => {
                let expected: serde_json::Value =
                    serde_json::from_str(expected).expect("expected json");
                assert_eq!(
                    value, expected,
                    "probe {} must compute the reference value",
                    probe.id
                );
            }
            (Err(token), Err(detail)) => assert!(
                detail.contains(token),
                "probe {} must refuse with {token}, got {detail}",
                probe.id
            ),
            (expected, outcome) => panic!(
                "probe {} expectation {:?} diverges from reference {:?}",
                probe.id, expected, outcome
            ),
        }
    }

    let node = available("node", &["--version"]);
    let php = available("php", &["--version"]);
    let go = available("go", &["version"]);
    assert!(node, "node must be available to execute the probes");

    let mut executed = 0usize;
    for (target, tool, args) in [
        (Target::Node, "node", vec![]),
        (Target::Php, "php", vec![]),
        (Target::Go, "go", vec!["run".to_owned()]),
    ] {
        let present = match target {
            Target::Node => node,
            Target::Php => php,
            Target::Go => go,
        };
        if !present {
            eprintln!("skipping target {target:?}: {tool} is not on this host");
            continue;
        }
        let program = write_program(&dir, &attachment, target);
        let vectors_file = std::fs::File::open(&document_path).expect("open probe document");
        let output = Command::new(tool)
            .args(&args)
            .arg(&program)
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
        assert_eq!(rows.len(), probes.len(), "{tool}: one row per probe");
        for (row, probe) in rows.iter().zip(probes.iter()) {
            assert_eq!(row["id"], probe.id, "{tool}: row order matches");
            match &probe.expect {
                Ok(expected) => {
                    let expected: serde_json::Value =
                        serde_json::from_str(expected).expect("expected json");
                    assert!(
                        row.get("error").is_none(),
                        "{tool}: {} must compute, got error {:?}",
                        probe.id,
                        row["error"]
                    );
                    assert_eq!(
                        row["value"], expected,
                        "{tool}: {} must compute identically",
                        probe.id
                    );
                }
                Err(token) => {
                    assert_eq!(
                        &row["error"], token,
                        "{tool}: {} must refuse with the closed token",
                        probe.id
                    );
                    assert!(
                        row.get("value").is_none(),
                        "{tool}: {} must not emit a value next to the refusal",
                        probe.id
                    );
                }
            }
        }
        executed += 1;
    }
    assert!(executed >= 1, "at least one target executed the probes");
}

#[test]
fn the_reference_refuses_a_malformed_clock_at_decode() {
    // A clock outside the canonical calendar never becomes a garbage
    // epoch on the reference side: the document itself is refused.
    let json: serde_json::Value = serde_json::from_str(
        r#"{
        "schemaVersion": "lekalo/expressions/vectors/v1.0.0",
        "vectors": [{
            "id": "bad-clock",
            "expression": "expr.planner/overdue-check",
            "clock": "2026-13-45T99:99:99Z",
            "bindings": {"input": {"due": "2026-01-01T00:00:00Z", "state": "todo"}},
            "expect": {"value": true}
        }]
    }"#,
    )
    .expect("document json");
    let rejection = VectorsDocument::from_value(&json).expect_err("malformed clock");
    assert!(rejection
        .as_slice()
        .iter()
        .any(|d| d.id() == "expression.input-invalid"));
}

/// Clock refusals: a wire-illegal clock (malformed string, explicit
/// null, non-string value) never becomes a garbage epoch, and a
/// supplied clock validates eagerly even when the body never reads
/// `now`. The reference refuses each document at decode; a directly
/// fed generated program refuses the row with the closed clock
/// token, except where the target's typed decode refuses the
/// document outright (Go decodes the clock member itself).
#[test]
fn every_generated_program_refuses_malformed_clocks_identically() {
    let attachment = parse();
    // (id, raw clock member JSON, now-reading body?, node, php, go)
    let probes: &[(&str, &str, bool, Outcome, Outcome, Outcome)] = &[
        (
            "clock-sign",
            "\"2026-01-01T-1:00:00Z\"",
            true,
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
        ),
        (
            "clock-plus",
            "\"2026-+1-01T00:00:00Z\"",
            true,
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
        ),
        (
            "clock-negative-field",
            "\"2026-01-01T00:-5:00Z\"",
            true,
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
        ),
        (
            "clock-newline",
            "\"2026-01-01T00:00:00Z\\n\"",
            true,
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
        ),
        // An explicit null is not an omission: it refuses even on a
        // body that never reads now.
        (
            "clock-null",
            "null",
            true,
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
            Outcome::Doc,
        ),
        (
            "clock-null-unused",
            "null",
            false,
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
            Outcome::Doc,
        ),
        // A non-string clock refuses too.
        (
            "clock-nonstring",
            "42",
            true,
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
            Outcome::Doc,
        ),
        // A malformed supplied clock refuses even when the body is a
        // constant and never reads the clock.
        (
            "clock-unused-bad",
            "\"2026-13-45T99:99:99Z\"",
            false,
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
        ),
    ];
    let reference_refuses = |clock_raw: &str| {
        let document_text = format!(
            "{{\"schemaVersion\":\"lekalo/expressions/vectors/v1.0.0\",\"vectors\":[{{\"id\":\"probe\",\"expression\":\"expr.planner/overdue-check\",\"clock\":{clock_raw},\"bindings\":{{\"input\":{{\"due\":\"2026-01-01T00:00:00Z\",\"state\":\"todo\"}}}},\"expect\":{{\"value\":true}}}}]}}"
        );
        let json: serde_json::Value = serde_json::from_str(&document_text).expect("document json");
        VectorsDocument::from_value(&json).is_err()
    };

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-clock-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");

    let node = available("node", &["--version"]);
    let php = available("php", &["--version"]);
    let go = available("go", &["version"]);
    assert!(node, "node must be available to execute the probes");

    let mut executed = 0usize;
    for (target, tool, args) in [
        (Target::Node, "node", vec![]),
        (Target::Php, "php", vec![]),
        (Target::Go, "go", vec!["run".to_owned()]),
    ] {
        let present = match target {
            Target::Node => node,
            Target::Php => php,
            Target::Go => go,
        };
        if !present {
            eprintln!("skipping target {target:?}: {tool} is not on this host");
            continue;
        }
        let program = write_program(&dir, &attachment, target);
        for (id, clock_raw, now_reading, node_expect, php_expect, go_expect) in probes {
            let expression = if *now_reading {
                "expr.planner/overdue-check"
            } else {
                // A constant body that never reads `now`; the valid
                // bindings keep the focus on the clock refusal.
                "expr.planner/wide-label"
            };
            let bindings = if *now_reading {
                "{\"input\":{\"due\":\"2026-01-01T00:00:00Z\",\"state\":\"todo\"}}"
            } else {
                "{\"input\":{\"left\":\"a\",\"right\":\"b\"}}"
            };
            let document = format!(
                "{{\"schemaVersion\":\"lekalo/expressions/vectors/v1.0.0\",\"vectors\":[{{\"id\":\"probe\",\"expression\":\"{expression}\",\"clock\":{clock_raw},\"bindings\":{bindings}}}]}}"
            );
            assert!(
                reference_refuses(clock_raw),
                "reference must refuse the {id} document"
            );
            let expected = match target {
                Target::Node => *node_expect,
                Target::Php => *php_expect,
                Target::Go => *go_expect,
            };
            assert_target_outcome(
                tool,
                &args,
                &program,
                &document,
                &dir.join(format!("{id}-{}.json", target.key())),
                expected,
                &format!("{tool}: {id}"),
            );
        }
        executed += 1;
    }
    assert!(
        executed >= 1,
        "at least one target executed the clock probes"
    );
}

/// The consumer-visible outcome one raw-binding case expects of a
/// target: refuse the document outright, emit a row with the closed
/// token, or compute the reference value.
struct RawCase {
    id: &'static str,
    expression: &'static str,
    /// The raw text pasted verbatim into the template.
    raw: &'static str,
    /// Whether the assembled document is syntactically valid JSON
    /// (the lone-surrogate escape is refused by the reference JSON
    /// decode itself, so no valid document exists for it).
    syntax_valid: bool,
    /// The reference bindings-stage refusal token; `None` marks the
    /// surrogate case whose refusal is the JSON decode itself, or
    /// the control which computes.
    reference_token: Option<&'static str>,
    node: Outcome,
    php: Outcome,
    go: Outcome,
}

/// Raw binding shapes the typed probe document cannot carry: a
/// fractional integer spelling, an exponent spelling, the
/// negative-zero spelling (scalar, duration, and set member), and a
/// lone surrogate escape survive only as raw document text. Each
/// assembled document is checked for syntactic validity first, the
/// reference rejection is exercised explicitly, and every generated
/// target must produce its declared refusal — a document-level
/// refusal or the closed row token, never a garbage value, and never
/// a refusal that is only an unrelated JSON syntax failure.
#[test]
fn every_generated_program_refuses_raw_binding_shapes() {
    let attachment = parse();
    let cases: &[RawCase] = &[
        // bindings {"input": {"total": 42.0, "parts": 1}}
        RawCase {
            id: "int-fractional",
            expression: "expr.planner/spread",
            raw: "42.0",
            syntax_valid: true,
            reference_token: Some("binding-value"),
            node: Outcome::Doc,
            php: Outcome::Row("binding-value"),
            go: Outcome::Row("binding-value"),
        },
        // bindings {"input": {"total": 1e2, "parts": 1}}
        RawCase {
            id: "int-exponent",
            expression: "expr.planner/spread",
            raw: "1e2",
            syntax_valid: true,
            reference_token: Some("binding-value"),
            node: Outcome::Doc,
            php: Outcome::Row("binding-value"),
            go: Outcome::Row("binding-value"),
        },
        // bindings {"input": {"total": -0, "parts": 1}}: the
        // reference preserves the spelling as a float and refuses;
        // no target may normalize it to the value 0.
        RawCase {
            id: "int-negative-zero",
            expression: "expr.planner/spread",
            raw: "-0",
            syntax_valid: true,
            reference_token: Some("binding-value"),
            node: Outcome::Doc,
            php: Outcome::Doc,
            go: Outcome::Row("binding-value"),
        },
        // bindings {"entity": {"age": -0, ...}}: the duration getter
        // refuses the same spelling.
        RawCase {
            id: "duration-negative-zero",
            expression: "expr.planner/period-check",
            raw: "-0",
            syntax_valid: true,
            reference_token: Some("binding-value"),
            node: Outcome::Doc,
            php: Outcome::Doc,
            go: Outcome::Row("binding-value"),
        },
        // bindings {"input": {"nums": [1, -0]}}: an integer-like set
        // member refuses too.
        RawCase {
            id: "set-member-negative-zero",
            expression: "expr.planner/number-set",
            raw: "[1, -0]",
            syntax_valid: true,
            reference_token: Some("binding-value"),
            node: Outcome::Doc,
            php: Outcome::Doc,
            go: Outcome::Row("binding-value"),
        },
        // bindings {"input": {"left": "\ud800", "right": "x"}}: the
        // reference JSON decode itself refuses the lone surrogate.
        RawCase {
            id: "string-lone-surrogate",
            expression: "expr.planner/wide-label",
            raw: "\"\\ud800\",\"right\":\"x\"",
            syntax_valid: false,
            reference_token: None,
            node: Outcome::Doc,
            php: Outcome::Doc,
            go: Outcome::Doc,
        },
        // The valid control: the same template with a canonical
        // integer computes the reference value everywhere.
        RawCase {
            id: "int-control",
            expression: "expr.planner/spread",
            raw: "42",
            syntax_valid: true,
            reference_token: None,
            node: Outcome::Value("42"),
            php: Outcome::Value("42"),
            go: Outcome::Value("42"),
        },
    ];

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-raw-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");

    // The templates are syntactically complete documents: the raw
    // value text is pasted verbatim at __RAW__, so no format-string
    // escaping applies and every brace below is accounted for.
    let document_of = |case: &RawCase| -> String {
        let position = match case.expression {
            "expr.planner/spread" => "{\"input\":{\"total\":__RAW__,\"parts\":1}}".to_owned(),
            "expr.planner/period-check" => {
                "{\"entity\":{\"age\":__RAW__,\"created\":\"2026-01-01T00:00:00Z\"}}".to_owned()
            }
            "expr.planner/number-set" => "{\"input\":{\"nums\":__RAW__}}".to_owned(),
            _ => "{\"input\":{\"left\":__RAW__}}".to_owned(),
        };
        format!(
            "{{\"schemaVersion\":\"lekalo/expressions/vectors/v1.0.0\",\"vectors\":[{{\"id\":\"probe\",\"expression\":\"{expr}\",\"bindings\":{position}}}]}}",
            expr = case.expression,
            position = position.replace("__RAW__", case.raw),
        )
    };

    // Reference side first: syntax validity, then the exact binding
    // rejection (or the computed value) from the reference decoder
    // and evaluator.
    for case in cases {
        let document = document_of(case);
        let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(&document);
        if case.syntax_valid {
            let json = parsed.unwrap_or_else(|error| {
                panic!(
                    "{}: template must assemble into valid JSON: {error}",
                    case.id
                )
            });
            let record = attachment
                .expression(case.expression)
                .unwrap_or_else(|| panic!("{}: unknown expression", case.id));
            let vectors = json["vectors"].as_array().expect("vectors array");
            match case.reference_token {
                Some(token) => {
                    let rejection = Bindings::from_json(record, &vectors[0]["bindings"])
                        .expect_err("reference must refuse the raw binding");
                    assert!(
                        rejection
                            .as_slice()
                            .iter()
                            .any(|d| format!("{:?}", d.data()).contains(token)),
                        "{}: reference must refuse with {token}",
                        case.id
                    );
                }
                None => {
                    let bindings = Bindings::from_json(record, &vectors[0]["bindings"])
                        .expect("control bindings must validate");
                    let clock = Clock::from_datetime("1970-01-01T00:00:00Z").expect("epoch");
                    let value = evaluate(record, &bindings, &clock).expect("control evaluates");
                    assert_eq!(
                        value.to_json().to_string(),
                        "42",
                        "{}: control must compute the reference value",
                        case.id
                    );
                }
            }
        } else {
            assert!(
                parsed.is_err(),
                "{}: the assembled document must fail the reference JSON decode",
                case.id
            );
        }
    }

    let node = available("node", &["--version"]);
    let php = available("php", &["--version"]);
    let go = available("go", &["version"]);
    assert!(node, "node must be available to execute the probes");

    let mut executed = 0usize;
    for (target, tool, args) in [
        (Target::Node, "node", vec![]),
        (Target::Php, "php", vec![]),
        (Target::Go, "go", vec!["run".to_owned()]),
    ] {
        let present = match target {
            Target::Node => node,
            Target::Php => php,
            Target::Go => go,
        };
        if !present {
            eprintln!("skipping target {target:?}: {tool} is not on this host");
            continue;
        }
        let program = write_program(&dir, &attachment, target);
        for case in cases {
            let expected = match target {
                Target::Node => case.node,
                Target::Php => case.php,
                Target::Go => case.go,
            };
            assert_target_outcome(
                tool,
                &args,
                &program,
                &document_of(case),
                &dir.join(format!("{}-{}.json", case.id, target.key())),
                expected,
                &format!("{}: {}", tool, case.id),
            );
        }
        executed += 1;
    }
    assert!(executed >= 1, "at least one target executed the raw probes");
}

/// The binding root is distinguished exactly: an absent or null
/// `bindings` member refuses (the reference refuses the document at
/// decode), an array root refuses, and the empty object stays a
/// valid root for a parameterless record — which must compute from
/// `{}` exactly like from any legal root, never from a missing or
/// nil one.
#[test]
fn every_generated_program_distinguishes_binding_roots_identically() {
    let attachment = parse();
    // (id, vector member JSON, node, php, go)
    let probes: &[(&str, &str, Outcome, Outcome, Outcome)] = &[
        // A parameterless constant record with the member omitted.
        (
            "roots-absent",
            "{\"id\":\"probe\",\"expression\":\"expr.planner/history-stamp\"}",
            Outcome::Row("bindings-shape"),
            Outcome::Doc,
            Outcome::Doc,
        ),
        (
            "roots-null",
            "{\"id\":\"probe\",\"expression\":\"expr.planner/history-stamp\",\"bindings\":null}",
            Outcome::Row("bindings-shape"),
            Outcome::Doc,
            Outcome::Doc,
        ),
        (
            "roots-array",
            "{\"id\":\"probe\",\"expression\":\"expr.planner/history-stamp\",\"bindings\":[]}",
            Outcome::Row("bindings-shape"),
            Outcome::Row("bindings-shape"),
            Outcome::Doc,
        ),
        // The empty object is the legal parameterless root. (The
        // expect member is included so the reference decode accepts
        // the control document; every generated target ignores it.)
        (
            "roots-empty-ok",
            "{\"id\":\"probe\",\"expression\":\"expr.planner/history-stamp\",\"bindings\":{},\"expect\":{\"value\":\"1969-12-31T23:59:59Z\"}}",
            Outcome::Value("\"1969-12-31T23:59:59Z\""),
            Outcome::Value("\"1969-12-31T23:59:59Z\""),
            Outcome::Value("\"1969-12-31T23:59:59Z\""),
        ),
        // The same refusals on a param-bearing record.
        (
            "roots-absent-params",
            "{\"id\":\"probe\",\"expression\":\"expr.planner/spread\"}",
            Outcome::Row("bindings-shape"),
            Outcome::Doc,
            Outcome::Doc,
        ),
    ];

    let reference_refuses = |vector_json: &str| -> bool {
        let document_text = format!(
            "{{\"schemaVersion\":\"lekalo/expressions/vectors/v1.0.0\",\"vectors\":[{vector_json}]}}"
        );
        let json: serde_json::Value = serde_json::from_str(&document_text).expect("document json");
        VectorsDocument::from_value(&json).is_err()
    };

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-roots-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");

    // Reference side: absent and null roots and the array root
    // refuse the document; the empty object decodes and evaluates to
    // the constant.
    for (id, vector_json, ..) in probes {
        let refused = reference_refuses(vector_json);
        if *id == "roots-empty-ok" {
            assert!(!refused, "{id}: the empty object root must decode");
        } else {
            assert!(refused, "{id}: the reference must refuse this root");
        }
    }

    let node = available("node", &["--version"]);
    let php = available("php", &["--version"]);
    let go = available("go", &["version"]);
    assert!(node, "node must be available to execute the probes");

    let mut executed = 0usize;
    for (target, tool, args) in [
        (Target::Node, "node", vec![]),
        (Target::Php, "php", vec![]),
        (Target::Go, "go", vec!["run".to_owned()]),
    ] {
        let present = match target {
            Target::Node => node,
            Target::Php => php,
            Target::Go => go,
        };
        if !present {
            eprintln!("skipping target {target:?}: {tool} is not on this host");
            continue;
        }
        let program = write_program(&dir, &attachment, target);
        for (id, vector_json, node_expect, php_expect, go_expect) in probes {
            let document = format!(
                "{{\"schemaVersion\":\"lekalo/expressions/vectors/v1.0.0\",\"vectors\":[{vector_json}]}}"
            );
            let expected = match target {
                Target::Node => *node_expect,
                Target::Php => *php_expect,
                Target::Go => *go_expect,
            };
            assert_target_outcome(
                tool,
                &args,
                &program,
                &document,
                &dir.join(format!("{id}-{}.json", target.key())),
                expected,
                &format!("{tool}: {id}"),
            );
        }
        executed += 1;
    }
    assert!(
        executed >= 1,
        "at least one target executed the binding-root probes"
    );
}
