//! Issue #66 correction round 1: executed cross-target regressions for
//! the projection boundaries the shared vector document cannot carry.
//! The vectors contract only accepts evaluation-stage error tokens, so
//! binding-stage refusals (duplicate set members, malformed datetime
//! calendar fields) and clock validation have no shared vector; here
//! every generated Node/PHP/Go program is executed against those exact
//! shapes and its emitted row token must equal the closed token the
//! reference refuses with — no garbage value may reach stdout.

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
            assert!(
                detail.contains("binding-value"),
                "probe {} refused with unexpected detail {detail}",
                probe.id
            );
            return Err("binding-value".to_owned());
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

    // The document the targets receive. The clock-invalid row cannot
    // live here — a malformed clock makes the reference refuse the
    // whole document at decode, which the second half of this test
    // pins — so every clock here is either absent or canonical, and
    // absent clocks only occur on bodies that never read `now`.
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
