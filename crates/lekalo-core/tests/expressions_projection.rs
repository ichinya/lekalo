//! Issue #66 executed cross-target regressions for the projection
//! boundaries the shared vector document cannot carry. The vectors
//! contract only accepts evaluation-stage error tokens, so
//! binding-stage refusals (duplicate set members, malformed datetime
//! calendar fields), clock presence validation, raw number
//! spellings, and binding-root shapes have no shared vector; here
//! every generated Node/PHP/Go program is executed against those
//! exact shapes and its emitted row token must equal the closed
//! token the reference refuses with — no garbage value may reach
//! stdout. The same harness pins the document-boundary behavior:
//! strict UTF-8 decoding of the raw stdin bytes, exactly-one-document
//! consumption, exact case-sensitive envelope member names, bounded
//! refusals that never echo synthetic sensitive input, and the
//! clock-before-expression precedence on double-fault vectors.

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
    document: impl AsRef<[u8]>,
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
        serde_json::json!("lekalo/expressions/vectors/v0.2.16"),
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
        "schemaVersion": "lekalo/expressions/vectors/v0.2.16",
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
            "{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{{\"id\":\"probe\",\"expression\":\"expr.planner/overdue-check\",\"clock\":{clock_raw},\"bindings\":{{\"input\":{{\"due\":\"2026-01-01T00:00:00Z\",\"state\":\"todo\"}}}},\"expect\":{{\"value\":true}}}}]}}"
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
                "{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{{\"id\":\"probe\",\"expression\":\"{expression}\",\"clock\":{clock_raw},\"bindings\":{bindings}}}]}}"
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
            "{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{{\"id\":\"probe\",\"expression\":\"{expr}\",\"bindings\":{position}}}]}}",
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
                document_of(case),
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
            "{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{vector_json}]}}"
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
                "{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{vector_json}]}}"
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

/// Raw stdin bytes are validated as strict UTF-8 before the JSON
/// parse: a stray non-ASCII byte, an overlong encoding, a truncated
/// sequence, and a UTF-8-encoded surrogate refuse the document at
/// the runner boundary — in scalar string and set-member string
/// bindings alike — instead of being silently replaced into
/// binding data, while a legal U+FFFD string stays accepted and
/// computes identically in every target. The malformed documents
/// are fed as exact raw bytes, never re-encoded text.
#[test]
fn every_generated_program_refuses_malformed_utf8_bytes() {
    struct Utf8Case {
        id: &'static str,
        expression: &'static str,
        /// The exact bindings-object bytes pasted verbatim into the
        /// document.
        bindings: &'static [u8],
        /// Whether the assembled document is legal JSON in legal
        /// UTF-8 (only the U+FFFD control is).
        legal: bool,
        node: Outcome,
        php: Outcome,
        go: Outcome,
    }
    let cases: &[Utf8Case] = &[
        // The raw FF byte inside a scalar string binding.
        Utf8Case {
            id: "utf8-scalar-ff",
            expression: "expr.planner/wide-label",
            bindings: b"{\"input\":{\"left\":\"\xff\",\"right\":\"x\"}}",
            legal: false,
            node: Outcome::Doc,
            php: Outcome::Doc,
            go: Outcome::Doc,
        },
        // An overlong encoding of U+002F.
        Utf8Case {
            id: "utf8-scalar-overlong",
            expression: "expr.planner/wide-label",
            bindings: b"{\"input\":{\"left\":\"\xc0\xaf\",\"right\":\"x\"}}",
            legal: false,
            node: Outcome::Doc,
            php: Outcome::Doc,
            go: Outcome::Doc,
        },
        // A truncated three-byte sequence.
        Utf8Case {
            id: "utf8-scalar-truncated",
            expression: "expr.planner/wide-label",
            bindings: b"{\"input\":{\"left\":\"\xe2\x82\",\"right\":\"x\"}}",
            legal: false,
            node: Outcome::Doc,
            php: Outcome::Doc,
            go: Outcome::Doc,
        },
        // A UTF-8-encoded surrogate (U+D800).
        Utf8Case {
            id: "utf8-scalar-surrogate",
            expression: "expr.planner/wide-label",
            bindings: b"{\"input\":{\"left\":\"\xed\xa0\x80\",\"right\":\"x\"}}",
            legal: false,
            node: Outcome::Doc,
            php: Outcome::Doc,
            go: Outcome::Doc,
        },
        // The same raw FF byte inside a set:string member.
        Utf8Case {
            id: "utf8-set-string-ff",
            expression: "expr.planner/state-gate",
            bindings: b"{\"input\":{\"state\":\"todo\",\"project_tags\":[\"\xff\"]},\"actor\":{\"role\":null}}",
            legal: false,
            node: Outcome::Doc,
            php: Outcome::Doc,
            go: Outcome::Doc,
        },
        // The legal control: an actual U+FFFD string is not a
        // malformed sequence and must compute identically everywhere.
        Utf8Case {
            id: "utf8-replacement-ok",
            expression: "expr.planner/wide-label",
            bindings: b"{\"input\":{\"left\":\"\xef\xbf\xbd\",\"right\":\"x\"}}",
            legal: true,
            node: Outcome::Value("\"\u{FFFD}x\""),
            php: Outcome::Value("\"\u{FFFD}x\""),
            go: Outcome::Value("\"\u{FFFD}x\""),
        },
    ];

    let document_of = |case: &Utf8Case| -> Vec<u8> {
        let mut document = Vec::new();
        document.extend_from_slice(
            b"{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{\"id\":\"probe\",\"expression\":\"",
        );
        document.extend_from_slice(case.expression.as_bytes());
        document.extend_from_slice(b"\",\"bindings\":");
        document.extend_from_slice(case.bindings);
        document.extend_from_slice(b"}]}");
        document
    };

    // Reference side: the malformed documents are not even decodable
    // JSON input (invalid UTF-8 inside a string), while the legal
    // U+FFFD control decodes and computes the reference value.
    for case in cases {
        let document = document_of(case);
        let parsed: Result<serde_json::Value, _> = serde_json::from_slice(&document);
        if case.legal {
            let json = parsed.expect("control document must parse");
            let attachment = parse();
            let record = attachment
                .expression(case.expression)
                .unwrap_or_else(|| panic!("{}: unknown expression", case.id));
            let vectors = json["vectors"].as_array().expect("vectors array");
            let bindings = Bindings::from_json(record, &vectors[0]["bindings"])
                .expect("control bindings must validate");
            let clock = Clock::from_datetime("1970-01-01T00:00:00Z").expect("epoch");
            let value = evaluate(record, &bindings, &clock).expect("control evaluates");
            assert_eq!(
                value.to_json().to_string(),
                "\"\u{FFFD}x\"",
                "{}: control must compute the reference value",
                case.id
            );
        } else {
            assert!(
                parsed.is_err(),
                "{}: the reference JSON decode must refuse invalid UTF-8",
                case.id
            );
        }
    }

    let attachment = parse();
    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-utf8-{}-{}",
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
        for case in cases {
            let document = document_of(case);
            let expected = match target {
                Target::Node => case.node,
                Target::Php => case.php,
                Target::Go => case.go,
            };
            assert_target_outcome(
                tool,
                &args,
                &program,
                &document,
                &dir.join(format!("{}-{}.json", case.id, target.key())),
                expected,
                &format!("{}: {}", tool, case.id),
            );
        }
        executed += 1;
    }
    assert!(
        executed >= 1,
        "at least one target executed the UTF-8 probes"
    );
}

/// Exactly one JSON document is consumed: a complete document
/// followed by any non-whitespace trailing byte or value — an extra
/// closing brace, an extra document, a bare token, a bare number —
/// refuses at the runner boundary, while a whitespace-only suffix
/// and the bare document stay legal and compute identically.
#[test]
fn every_generated_program_consumes_exactly_one_json_document() {
    let attachment = parse();
    // The expect member is included so the reference decode accepts
    // the control document; every generated target ignores it.
    let control = "{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{\"id\":\"probe\",\"expression\":\"expr.planner/history-stamp\",\"bindings\":{},\"expect\":{\"value\":\"1969-12-31T23:59:59Z\"}}]}";
    let value = Outcome::Value("\"1969-12-31T23:59:59Z\"");
    // (id, document, node, php, go)
    let probes: &[(&str, String, Outcome, Outcome, Outcome)] = &[
        ("trailing-none-ok", control.to_owned(), value, value, value),
        (
            "trailing-whitespace-ok",
            format!("{control}\n\t "),
            value,
            value,
            value,
        ),
        (
            "trailing-brace",
            format!("{control}}}"),
            Outcome::Doc,
            Outcome::Doc,
            Outcome::Doc,
        ),
        (
            "trailing-object",
            format!("{control}{{}}"),
            Outcome::Doc,
            Outcome::Doc,
            Outcome::Doc,
        ),
        (
            "trailing-token",
            format!("{control}garbage"),
            Outcome::Doc,
            Outcome::Doc,
            Outcome::Doc,
        ),
        (
            "trailing-number",
            format!("{control}1"),
            Outcome::Doc,
            Outcome::Doc,
            Outcome::Doc,
        ),
    ];

    // Reference side: the exact control document decodes for the
    // reference evaluator.
    let json: serde_json::Value = serde_json::from_str(control).expect("control json");
    assert!(
        VectorsDocument::from_value(&json).is_ok(),
        "the control document must decode for the reference"
    );

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-trailing-{}-{}",
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
        for (id, document, node_expect, php_expect, go_expect) in probes {
            let expected = match target {
                Target::Node => *node_expect,
                Target::Php => *php_expect,
                Target::Go => *go_expect,
            };
            assert_target_outcome(
                tool,
                &args,
                &program,
                document,
                &dir.join(format!("{id}-{}.json", target.key())),
                expected,
                &format!("{tool}: {id}"),
            );
        }
        executed += 1;
    }
    assert!(
        executed >= 1,
        "at least one target executed the trailing-input probes"
    );
}

/// Envelope member names are exact and case-sensitive: a case
/// variant (`Bindings`, `BINDINGS`, `Vectors`, `ID`, `EXPRESSION`)
/// never satisfies the required lowercase member's presence. The
/// reference decodes exact keys and refuses the document; PHP
/// refuses the document; a case-variant `ID` leaves no string id to
/// consume, so Node refuses the document too; Go must refuse the
/// document instead of silently decoding the case variant into the
/// required member.
#[test]
fn every_generated_program_enforces_exact_envelope_member_names() {
    let attachment = parse();
    let wrap = |vector_json: &str| {
        format!("{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{vector_json}]}}")
    };
    let value = Outcome::Value("\"1969-12-31T23:59:59Z\"");
    // (id, full document, node, php, go). The control carries the
    // expect member so the reference decode accepts it; every
    // generated target ignores it.
    let probes: &[(&str, String, Outcome, Outcome, Outcome)] = &[
        (
            "exact-members-ok",
            wrap("{\"id\":\"probe\",\"expression\":\"expr.planner/history-stamp\",\"bindings\":{},\"expect\":{\"value\":\"1969-12-31T23:59:59Z\"}}"),
            value,
            value,
            value,
        ),
        (
            "exact-bindings-title",
            wrap("{\"id\":\"probe\",\"expression\":\"expr.planner/history-stamp\",\"Bindings\":{}}"),
            Outcome::Row("bindings-shape"),
            Outcome::Doc,
            Outcome::Doc,
        ),
        (
            "exact-bindings-upper",
            wrap("{\"id\":\"probe\",\"expression\":\"expr.planner/history-stamp\",\"BINDINGS\":{}}"),
            Outcome::Row("bindings-shape"),
            Outcome::Doc,
            Outcome::Doc,
        ),
        (
            "exact-vectors-title",
            "{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"Vectors\":[{\"id\":\"probe\",\"expression\":\"expr.planner/history-stamp\",\"bindings\":{}}]}".to_owned(),
            Outcome::Doc,
            Outcome::Doc,
            Outcome::Doc,
        ),
        (
            "exact-id-title",
            wrap("{\"ID\":\"probe\",\"expression\":\"expr.planner/history-stamp\",\"bindings\":{}}"),
            Outcome::Doc,
            Outcome::Doc,
            Outcome::Doc,
        ),
        (
            "exact-expression-upper",
            wrap("{\"id\":\"probe\",\"EXPRESSION\":\"expr.planner/history-stamp\",\"bindings\":{}}"),
            Outcome::Row("expression-unknown"),
            Outcome::Doc,
            Outcome::Doc,
        ),
    ];

    // Reference side: the exact lowercase members decode; every case
    // variant refuses the document.
    for (id, document, ..) in probes {
        let json: serde_json::Value = serde_json::from_str(document).expect("document json");
        let refused = VectorsDocument::from_value(&json).is_err();
        if *id == "exact-members-ok" {
            assert!(!refused, "{id}: the exact lowercase members must decode");
        } else {
            assert!(
                refused,
                "{id}: the reference must refuse a case-variant envelope member"
            );
        }
    }

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-exact-{}-{}",
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
        for (id, document, node_expect, php_expect, go_expect) in probes {
            let expected = match target {
                Target::Node => *node_expect,
                Target::Php => *php_expect,
                Target::Go => *go_expect,
            };
            assert_target_outcome(
                tool,
                &args,
                &program,
                document,
                &dir.join(format!("{id}-{}.json", target.key())),
                expected,
                &format!("{tool}: {id}"),
            );
        }
        executed += 1;
    }
    assert!(
        executed >= 1,
        "at least one target executed the exact-member probes"
    );
}

/// A malformed document that carries a synthetic sensitive binding
/// marker is refused with only the fixed bounded refusal: nonzero
/// exit, empty stdout, and a stderr reduced to the fixed message —
/// never the echoed document text, the raw binding data, an
/// exception name, a stack trace, or the host program path.
#[test]
fn every_generated_program_refuses_malformed_documents_privately() {
    let attachment = parse();
    const MARKER: &str = "SYNTHETIC_PRIVATE_MARKER_66_C4";
    // A syntactically malformed document (one extra closing brace
    // after the complete document) whose binding carries the
    // synthetic marker: the refusal must never echo it.
    let mut document = serde_json::to_string(&serde_json::json!({
        "schemaVersion": "lekalo/expressions/vectors/v0.2.16",
        "vectors": [{
            "id": "probe",
            "expression": "expr.planner/copy-tag",
            "bindings": {"input": {"kind": MARKER}}
        }]
    }))
    .expect("marker document");
    document.push('}');
    assert!(
        serde_json::from_str::<serde_json::Value>(&document).is_err(),
        "the marker document must be malformed JSON"
    );

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-privacy-{}-{}",
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
        let path = dir.join(format!("privacy-{}.json", target.key()));
        std::fs::write(&path, document.as_bytes()).expect("write probe document");
        let vectors_file = std::fs::File::open(&path).expect("open probe document");
        let output = Command::new(tool)
            .args(&args)
            .arg(&program)
            .stdin(Stdio::from(vectors_file))
            .output()
            .unwrap_or_else(|error| panic!("privacy {tool}: run the generated program: {error}"));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "privacy {tool}: must refuse the malformed document outright"
        );
        assert!(
            output.stdout.is_empty(),
            "privacy {tool}: refused but still printed results"
        );
        assert!(
            stderr.contains("lekalo vector input error"),
            "privacy {tool}: must emit the fixed bounded refusal, got {stderr:?}"
        );
        assert!(
            !stderr.contains(MARKER),
            "privacy {tool}: the refusal must not echo the synthetic binding marker"
        );
        assert!(
            !stderr.contains("schemaVersion"),
            "privacy {tool}: the refusal must not echo document text"
        );
        assert!(
            !stderr.contains("    at ") && !stderr.contains("SyntaxError"),
            "privacy {tool}: the refusal must not carry an exception or stack trace"
        );
        assert!(
            !stderr.contains(".cjs") && !stderr.contains(".php") && !stderr.contains(".go"),
            "privacy {tool}: the refusal must not disclose the host program path"
        );
        executed += 1;
    }
    assert!(
        executed >= 1,
        "at least one target executed the privacy probe"
    );
}

/// Double-fault precedence: a malformed supplied clock on a vector
/// whose expression is also unknown surfaces the clock refusal
/// first — the reference validates the clock at decode, before the
/// per-vector expression lookup, and every generated target must
/// report the closed clock token, not `expression-unknown`. The
/// canonical-clock control keeps the unknown-expression token
/// reachable.
#[test]
fn every_generated_program_refuses_clock_before_unknown_expression() {
    let attachment = parse();
    // Reference: the malformed clock refuses the whole document at
    // decode, before the per-vector expression lookup.
    let json: serde_json::Value = serde_json::from_str(
        "{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{\"id\":\"probe\",\"expression\":\"expr.planner/nonexistent\",\"clock\":\"2026-13-45T99:99:99Z\",\"bindings\":{}}]}",
    )
    .expect("document json");
    let rejection = VectorsDocument::from_value(&json)
        .expect_err("the malformed clock must refuse before the expression lookup");
    assert!(rejection
        .as_slice()
        .iter()
        .any(|d| d.id() == "expression.input-invalid"));

    // (id, vector member JSON, node, php, go)
    let probes: &[(&str, &str, Outcome, Outcome, Outcome)] = &[
        (
            "clock-first",
            "{\"id\":\"probe\",\"expression\":\"expr.planner/nonexistent\",\"clock\":\"2026-13-45T99:99:99Z\",\"bindings\":{}}",
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
        ),
        (
            "expression-unknown-control",
            "{\"id\":\"probe\",\"expression\":\"expr.planner/nonexistent\",\"bindings\":{}}",
            Outcome::Row("expression-unknown"),
            Outcome::Row("expression-unknown"),
            Outcome::Row("expression-unknown"),
        ),
    ];

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-clock-first-{}-{}",
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
        for (id, vector_json, node_expect, php_expect, go_expect) in probes {
            let document = format!(
                "{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{vector_json}]}}"
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
        "at least one target executed the double-fault precedence probes"
    );
}

/// A non-string `expression` selector never resolves through the
/// generated expression tables: an array (or nested array), an
/// object, a scalar number or boolean, and an explicit null can
/// never be coerced into an expression name and computed. Where the
/// target model is row-level the vector takes the closed
/// `expression-unknown` token; where the typed decode refuses
/// outright the document is refused. A malformed supplied clock
/// keeps its first place on a double-fault vector, and the
/// primitive-string control still computes. The reference refuses
/// every non-string selector at decode.
#[test]
fn every_generated_program_refuses_non_string_expression_selectors() {
    let attachment = parse();
    // (id, vector member JSON, node, php, go)
    let probes: &[(&str, &str, Outcome, Outcome, Outcome)] = &[
        (
            "selector-array",
            "{\"id\":\"probe\",\"expression\":[\"expr.planner/history-stamp\"],\"bindings\":{}}",
            Outcome::Row("expression-unknown"),
            Outcome::Row("expression-unknown"),
            Outcome::Doc,
        ),
        (
            "selector-nested-array",
            "{\"id\":\"probe\",\"expression\":[[\"expr.planner/history-stamp\"]],\"bindings\":{}}",
            Outcome::Row("expression-unknown"),
            Outcome::Row("expression-unknown"),
            Outcome::Doc,
        ),
        (
            "selector-object",
            "{\"id\":\"probe\",\"expression\":{\"name\":\"expr.planner/history-stamp\"},\"bindings\":{}}",
            Outcome::Row("expression-unknown"),
            Outcome::Row("expression-unknown"),
            Outcome::Doc,
        ),
        (
            "selector-number",
            "{\"id\":\"probe\",\"expression\":42,\"bindings\":{}}",
            Outcome::Row("expression-unknown"),
            Outcome::Row("expression-unknown"),
            Outcome::Doc,
        ),
        (
            "selector-boolean",
            "{\"id\":\"probe\",\"expression\":true,\"bindings\":{}}",
            Outcome::Row("expression-unknown"),
            Outcome::Row("expression-unknown"),
            Outcome::Doc,
        ),
        (
            "selector-null",
            "{\"id\":\"probe\",\"expression\":null,\"bindings\":{}}",
            Outcome::Row("expression-unknown"),
            Outcome::Doc,
            Outcome::Doc,
        ),
        // Double faults: the malformed supplied clock is validated
        // before the selector, so the row-level targets report
        // `clock-invalid` wherever their decode lets the vector
        // reach row evaluation.
        (
            "clock-first-array-selector",
            "{\"id\":\"probe\",\"expression\":[\"expr.planner/history-stamp\"],\"clock\":\"2026-13-45T99:99:99Z\",\"bindings\":{}}",
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
            Outcome::Doc,
        ),
        (
            "clock-first-null-selector",
            "{\"id\":\"probe\",\"expression\":null,\"clock\":\"2026-13-45T99:99:99Z\",\"bindings\":{}}",
            Outcome::Row("clock-invalid"),
            Outcome::Doc,
            Outcome::Doc,
        ),
        // An empty-string selector is a string, so the clock check
        // precedes it in every target; it resolves to nothing.
        (
            "clock-first-empty-selector",
            "{\"id\":\"probe\",\"expression\":\"\",\"clock\":\"2026-13-45T99:99:99Z\",\"bindings\":{}}",
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
            Outcome::Row("clock-invalid"),
        ),
        // The primitive-string control computes identically.
        (
            "selector-string-ok",
            "{\"id\":\"probe\",\"expression\":\"expr.planner/history-stamp\",\"bindings\":{},\"expect\":{\"value\":\"1969-12-31T23:59:59Z\"}}",
            Outcome::Value("\"1969-12-31T23:59:59Z\""),
            Outcome::Value("\"1969-12-31T23:59:59Z\""),
            Outcome::Value("\"1969-12-31T23:59:59Z\""),
        ),
    ];

    // Reference side: every non-string selector (and every
    // double-fault document) refuses at decode; the string control
    // decodes and evaluates to the epoch stamp. The reference-side
    // vectors carry the harness-owned expect member, so a refusal
    // is attributable to the selector alone.
    for (id, vector_json, ..) in probes {
        let reference_vector = if *id == "selector-string-ok" {
            (*vector_json).to_owned()
        } else {
            format!(
                "{},\"expect\":{{\"value\":true}}}}",
                &vector_json[..vector_json.len() - 1]
            )
        };
        let document_text = format!(
            "{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{reference_vector}]}}"
        );
        let json: serde_json::Value = serde_json::from_str(&document_text).expect("document json");
        let refused = VectorsDocument::from_value(&json).is_err();
        if *id == "selector-string-ok" {
            assert!(!refused, "{id}: the string control must decode");
        } else {
            assert!(
                refused,
                "{id}: the reference must refuse a non-string selector"
            );
        }
    }

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-selector-{}-{}",
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
        for (id, vector_json, node_expect, php_expect, go_expect) in probes {
            let document = format!(
                "{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{vector_json}]}}"
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
        "at least one target executed the selector probes"
    );
}

/// An undeclared expression name that matches an inherited
/// object-prototype member (`constructor`, `toString`, `__proto__`,
/// `hasOwnProperty`, `valueOf`) is unknown exactly like any other
/// undeclared name: the generated tables carry only the declared
/// records, so every target emits the closed `expression-unknown`
/// token — never a value computed from, or the runtime failure of,
/// the inherited member — and the declared-string control computes.
#[test]
fn every_generated_program_treats_prototype_names_as_unknown_expressions() {
    let attachment = parse();
    let token = Outcome::Row("expression-unknown");
    let value = Outcome::Value("\"1969-12-31T23:59:59Z\"");
    // (id, selector text, node, php, go)
    let probes: &[(&str, &str, Outcome, Outcome, Outcome)] = &[
        ("prototype-constructor", "constructor", token, token, token),
        ("prototype-to-string", "toString", token, token, token),
        ("prototype-proto", "__proto__", token, token, token),
        (
            "prototype-has-own-property",
            "hasOwnProperty",
            token,
            token,
            token,
        ),
        ("prototype-value-of", "valueOf", token, token, token),
        (
            "prototype-declared-ok",
            "expr.planner/history-stamp",
            value,
            value,
            value,
        ),
    ];

    // Reference side: every prototype-shaped selector fails the
    // canonical expression-name decode; the declared control
    // decodes and evaluates to the epoch stamp. The reference-side
    // vectors carry the harness-owned expect member.
    for (id, selector, ..) in probes {
        let document_text = format!(
            "{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{{\"id\":\"probe\",\"expression\":\"{selector}\",\"bindings\":{{}},\"expect\":{{\"value\":true}}}}]}}"
        );
        let json: serde_json::Value = serde_json::from_str(&document_text).expect("document json");
        let refused = VectorsDocument::from_value(&json).is_err();
        if *id == "prototype-declared-ok" {
            assert!(!refused, "{id}: the declared control must decode");
        } else {
            assert!(
                refused,
                "{id}: the reference must refuse the noncanonical name"
            );
        }
    }

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-prototype-{}-{}",
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
        for (id, selector, node_expect, php_expect, go_expect) in probes {
            let document = format!(
                "{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{{\"id\":\"probe\",\"expression\":\"{selector}\",\"bindings\":{{}},\"expect\":{{\"value\":\"1969-12-31T23:59:59Z\"}}}}]}}"
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
        "at least one target executed the prototype-name probes"
    );
}

/// The consumed `id` is echoed into the result row, so every target
/// requires a primitive string before any row exists: an explicit
/// null, a number, a boolean, an object, an array, and an absent id
/// refuse the document outright — no malformed or id-less value row
/// may reach the envelope — while the string control computes and
/// echoes its id exactly. The reference refuses every malformed id
/// at decode.
#[test]
fn every_generated_program_requires_primitive_string_vector_ids() {
    let attachment = parse();
    // (id, vector member JSON)
    let probes: &[(&str, &str)] = &[
        (
            "id-null",
            "{\"id\":null,\"expression\":\"expr.planner/history-stamp\",\"bindings\":{}}",
        ),
        (
            "id-number",
            "{\"id\":42,\"expression\":\"expr.planner/history-stamp\",\"bindings\":{}}",
        ),
        (
            "id-boolean",
            "{\"id\":true,\"expression\":\"expr.planner/history-stamp\",\"bindings\":{}}",
        ),
        (
            "id-object",
            "{\"id\":{},\"expression\":\"expr.planner/history-stamp\",\"bindings\":{}}",
        ),
        (
            "id-array",
            "{\"id\":[],\"expression\":\"expr.planner/history-stamp\",\"bindings\":{}}",
        ),
        (
            "id-absent",
            "{\"expression\":\"expr.planner/history-stamp\",\"bindings\":{}}",
        ),
    ];

    // Reference side: every malformed or absent id refuses at
    // decode; the string control decodes and evaluates. The
    // reference-side vectors carry the harness-owned expect member,
    // so a refusal is attributable to the id alone.
    for (id, vector_json) in probes {
        let reference_vector = format!(
            "{},\"expect\":{{\"value\":true}}}}",
            &vector_json[..vector_json.len() - 1]
        );
        let document_text = format!(
            "{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{reference_vector}]}}"
        );
        let json: serde_json::Value = serde_json::from_str(&document_text).expect("document json");
        assert!(
            VectorsDocument::from_value(&json).is_err(),
            "{id}: the reference must refuse the malformed id"
        );
    }
    let control_text = "{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{\"id\":\"probe\",\"expression\":\"expr.planner/history-stamp\",\"bindings\":{},\"expect\":{\"value\":\"1969-12-31T23:59:59Z\"}}]}";
    let control_json: serde_json::Value = serde_json::from_str(control_text).expect("control json");
    let control_document = VectorsDocument::from_value(&control_json).expect("control decodes");
    {
        let record = attachment
            .expression("expr.planner/history-stamp")
            .expect("control expression");
        let bindings = Bindings::from_json(record, &control_json["vectors"][0]["bindings"])
            .expect("control bindings");
        let clock = Clock::from_datetime("1970-01-01T00:00:00Z").expect("epoch");
        let case = &control_document.vectors[0];
        let value = evaluate(record, &bindings, &clock).expect("control evaluates");
        assert_eq!(
            value.to_json().to_string(),
            "\"1969-12-31T23:59:59Z\"",
            "the string-id control must evaluate"
        );
        assert_eq!(case.id, "probe", "the control id is consumed as a string");
    }

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-id-{}-{}",
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
        for (id, vector_json) in probes {
            let document = format!(
                "{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":[{vector_json}]}}"
            );
            assert_target_outcome(
                tool,
                &args,
                &program,
                &document,
                &dir.join(format!("{id}-{}.json", target.key())),
                Outcome::Doc,
                &format!("{tool}: {id}"),
            );
        }
        // The string control computes and echoes its id exactly: no
        // target may drop, rewrite, or coerce the consumed id.
        let control_path = dir.join(format!("id-control-{}.json", target.key()));
        std::fs::write(&control_path, control_text).expect("write control document");
        let vectors_file = std::fs::File::open(&control_path).expect("open control");
        let output = Command::new(tool)
            .args(&args)
            .arg(&program)
            .stdin(Stdio::from(vectors_file))
            .output()
            .unwrap_or_else(|error| panic!("id control {tool}: run: {error}"));
        assert!(
            output.status.success(),
            "id control {tool}: must compute, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("utf8 results");
        let envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
            panic!("id control {tool}: results envelope: {error}; stdout: {stdout}")
        });
        let rows = envelope["results"].as_array().expect("results array");
        assert_eq!(rows.len(), 1, "id control {tool}: one row");
        assert_eq!(
            rows[0]["id"], "probe",
            "id control {tool}: the string id must be echoed exactly"
        );
        assert!(
            rows[0].get("error").is_none(),
            "id control {tool}: must compute, got error {:?}",
            rows[0]["error"]
        );
        assert_eq!(
            rows[0]["value"],
            serde_json::json!("1969-12-31T23:59:59Z"),
            "id control {tool}: must compute the reference value"
        );
        executed += 1;
    }
    assert!(executed >= 1, "at least one target executed the id probes");
}

/// The `vectors` member is a JSON array of non-null objects before
/// any row is evaluated: an iterable string, a scalar, an object, a
/// boolean, null, and scalar/array/null elements refuse the document
/// with the bounded private refusal — nonzero exit, empty stdout, a
/// stderr reduced to the fixed message, never an echoed element text
/// and never a fabricated row — while the single-object control
/// computes. The reference refuses every such document at decode.
#[test]
fn every_generated_program_requires_a_vector_object_array() {
    const MARKER: &str = "SYNTHETIC_PRIVATE_MARKER_66_C5_VECTORS";
    let attachment = parse();
    let wrap = |vectors_json: &str| {
        format!("{{\"schemaVersion\":\"lekalo/expressions/vectors/v0.2.16\",\"vectors\":{vectors_json}}}")
    };
    // (id, document, distinct texts that must never reach stderr)
    let probes: &[(&str, String, &[&str])] = &[
        ("vectors-string", wrap(&format!("\"{MARKER}\"")), &[MARKER]),
        ("vectors-empty-string", wrap("\"\""), &[]),
        ("vectors-number", wrap("42"), &[]),
        ("vectors-object", wrap("{}"), &[]),
        ("vectors-boolean", wrap("true"), &[]),
        ("vectors-null", wrap("null"), &[]),
        ("vectors-element-number", wrap("[42]"), &[]),
        (
            "vectors-element-string",
            wrap("[\"probe-element-text\"]"),
            &["probe-element-text"],
        ),
        ("vectors-element-boolean", wrap("[true]"), &[]),
        ("vectors-element-array", wrap("[[]]"), &[]),
        ("vectors-element-null", wrap("[null]"), &[]),
    ];
    let control = wrap(
        "[{\"id\":\"probe\",\"expression\":\"expr.planner/history-stamp\",\"bindings\":{},\"expect\":{\"value\":\"1969-12-31T23:59:59Z\"}}]",
    );

    // Reference side: every non-array vectors member and every
    // non-object element refuses at decode; the control decodes.
    for (id, document, ..) in probes {
        let json: serde_json::Value = serde_json::from_str(document).expect("document json");
        assert!(
            VectorsDocument::from_value(&json).is_err(),
            "{id}: the reference must refuse the vectors shape"
        );
    }
    let control_json: serde_json::Value = serde_json::from_str(&control).expect("control json");
    assert!(
        VectorsDocument::from_value(&control_json).is_ok(),
        "the control must decode for the reference"
    );

    let dir = std::env::temp_dir().join(format!(
        "lekalo-expr-vectors-{}-{}",
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
        for (id, document, private) in probes {
            let path = dir.join(format!("{id}-{}.json", target.key()));
            std::fs::write(&path, document).expect("write probe document");
            let vectors_file = std::fs::File::open(&path).expect("open probe document");
            let output = Command::new(tool)
                .args(&args)
                .arg(&program)
                .stdin(Stdio::from(vectors_file))
                .output()
                .unwrap_or_else(|error| panic!("vectors {tool}: run: {error}"));
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                !output.status.success(),
                "vectors {tool}: {id} must refuse the document outright"
            );
            assert!(
                output.stdout.is_empty(),
                "vectors {tool}: {id} refused but still printed results"
            );
            assert!(
                stderr.contains("lekalo vector input error"),
                "vectors {tool}: {id} must emit the fixed bounded refusal, got {stderr:?}"
            );
            for text in *private {
                assert!(
                    !stderr.contains(text),
                    "vectors {tool}: {id} must not echo {text:?}, got {stderr:?}"
                );
            }
        }
        // The control computes.
        assert_target_outcome(
            tool,
            &args,
            &program,
            &control,
            &dir.join(format!("vectors-control-{}.json", target.key())),
            Outcome::Value("\"1969-12-31T23:59:59Z\""),
            &format!("vectors {tool}: control"),
        );
        executed += 1;
    }
    assert!(
        executed >= 1,
        "at least one target executed the vectors-shape probes"
    );
}
