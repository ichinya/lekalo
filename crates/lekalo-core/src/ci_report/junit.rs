//! The JUnit XML projection of a CI report (issue #103).
//!
//! One `<testsuite>` per report suite plus one synthetic gate suite
//! carrying the check rows, inside a single `<testsuites>` document.
//! Scenario and case counts are assertion counts; infrastructure,
//! evidence, and required-unavailable rows become `<error>`, evaluated
//! assertion failures become `<failure>`, and policy-allowed optional
//! absences become `<skipped>`. No `<system-out>`/`<system-err>`, no
//! source snippets, no raw child transcripts, no timing.

use super::model::{CaseRow, CiReport, EffectiveOutcome, FailureClass, SourceOutcome};

/// Escape one XML attribute value; control characters are collapsed to
/// spaces so the document is always well-formed.
fn xml_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// The JUnit row kind of one case: pass, failure (assertion), or error
/// (infrastructure/required-absence/denied), or skipped.
enum Row<'a> {
    Pass,
    Failure(&'a str),
    Error(&'a str),
    Skipped(&'a str),
}

/// Classify one case row into its JUnit child element.
fn row_of(case: &CaseRow) -> Row<'_> {
    match case.effective_outcome() {
        EffectiveOutcome::Pass => Row::Pass,
        EffectiveOutcome::Warn | EffectiveOutcome::Skip => {
            Row::Skipped(match case.source_outcome {
                SourceOutcome::Unsupported => "unsupported",
                _ => "unavailable",
            })
        }
        EffectiveOutcome::Fail => match case.failure_class {
            FailureClass::Infrastructure => Row::Error("infrastructure"),
            FailureClass::Boot => Row::Error("boot"),
            FailureClass::EvidenceInvalid => Row::Error("evidence-invalid"),
            FailureClass::Security => Row::Error("security"),
            FailureClass::Assertion => Row::Failure("assertion"),
            FailureClass::StaticAnalysis => Row::Failure("static-analysis"),
            _ => Row::Failure("assertion"),
        },
        EffectiveOutcome::Error => Row::Error(match case.failure_class {
            FailureClass::MissingComponent => "required-unavailable",
            FailureClass::Policy => "policy",
            _ => "required-unavailable",
        }),
    }
}

/// The deterministic JUnit document. Exit-neutral rows are skipped;
/// blocking rows carry `<failure>`/`<error>` children; counts derive
/// from the emitted cases (`errors` separately from `failures`).
pub fn render(report: &CiReport) -> String {
    let mut suites: Vec<(String, usize, usize, usize, String)> = Vec::new();
    // The synthetic gate suite: one testcase per check row, grouped by
    // the report command so distinct command scopes never merge.
    let gate_id = format!("lekalo.{}", report.invocation.command.as_str());
    {
        let tests = report.checks.len();
        let mut failures = 0usize;
        let mut errors = 0usize;
        let mut body = String::new();
        for check in &report.checks {
            let case = CaseRow {
                id: check.id.clone(),
                required: check.required,
                source_outcome: check.source_outcome,
                failure_class: check.failure_class,
                diagnostic_indexes: check.diagnostic_indexes.clone(),
                detail: check.detail.clone(),
            };
            let child = match row_of(&case) {
                Row::Pass => String::new(),
                Row::Failure(kind) => {
                    failures += 1;
                    format!(
                        "    <failure type=\"{}\" message=\"{}\"/>\n",
                        xml_attr(kind),
                        xml_attr(&case.detail),
                    )
                }
                Row::Error(kind) => {
                    errors += 1;
                    format!(
                        "    <error type=\"{}\" message=\"{}\"/>\n",
                        xml_attr(kind),
                        xml_attr(&case.detail),
                    )
                }
                Row::Skipped(reason) => {
                    format!("    <skipped message=\"{}\"/>\n", xml_attr(reason))
                }
            };
            body.push_str(&format!(
                "  <testcase name=\"{}\" classname=\"{}\">\n{}</testcase>\n",
                xml_attr(&case.id),
                xml_attr(&gate_id),
                child,
            ));
        }
        if tests > 0 {
            suites.push((gate_id, tests, failures, errors, body));
        }
    }
    for suite in &report.suites {
        let tests = suite.cases.len();
        let mut failures = 0usize;
        let mut errors = 0usize;
        let mut body = String::new();
        let classbase = format!("lekalo.{}.{}", report.invocation.command.as_str(), suite.id);
        for case in &suite.cases {
            let child = match row_of(case) {
                Row::Pass => String::new(),
                Row::Failure(kind) => {
                    failures += 1;
                    format!(
                        "    <failure type=\"{}\" message=\"{}\"/>\n",
                        xml_attr(kind),
                        xml_attr(&case.detail),
                    )
                }
                Row::Error(kind) => {
                    errors += 1;
                    format!(
                        "    <error type=\"{}\" message=\"{}\"/>\n",
                        xml_attr(kind),
                        xml_attr(&case.detail),
                    )
                }
                Row::Skipped(reason) => {
                    format!("    <skipped message=\"{}\"/>\n", xml_attr(reason))
                }
            };
            body.push_str(&format!(
                "  <testcase name=\"{}\" classname=\"{}\">\n{}</testcase>\n",
                xml_attr(&case.id),
                xml_attr(&classbase),
                child,
            ));
        }
        suites.push((suite.id.clone(), tests, failures, errors, body));
    }
    let total_tests: usize = suites.iter().map(|(_, tests, _, _, _)| *tests).sum();
    let total_failures: usize = suites.iter().map(|(_, _, failures, _, _)| *failures).sum();
    let total_errors: usize = suites.iter().map(|(_, _, _, errors, _)| *errors).sum();
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<testsuites name=\"lekalo ci report\" tests=\"{}\" failures=\"{}\" errors=\"{}\">\n",
        total_tests, total_failures, total_errors,
    ));
    for (id, tests, failures, errors, body) in &suites {
        let skipped = tests - failures - errors;
        out.push_str(&format!(
            "<testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\">\n",
            xml_attr(id),
            tests,
            failures,
            errors,
            skipped,
        ));
        out.push_str(body);
        out.push_str("</testsuite>\n");
    }
    out.push_str("</testsuites>\n");
    out
}
