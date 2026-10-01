//! The CI report unit suite (issue #103): the closed wire bytes, the
//! exit-policy table (required vs optional × default/strict/lenient),
//! the deterministic JUnit/SARIF/Markdown projections, and the
//! cross-field invariants the schema cannot express.

use crate::ci_report::build::{
    apply_check_policy, build, CaseDraft, CheckDraft, CiPolicy, CommandOutcome, SuiteDraft,
};
use crate::ci_report::model::{
    CaseRow, CiReport, CommandName, Coverage, EffectiveOutcome, FailureClass, SourceOutcome,
    SuiteKind, UnknownReason, ValueState,
};
use crate::ci_report::{junit, markdown, sarif};
use crate::diagnostics::normalize::build as diagnostic_build;
use crate::diagnostics::DataObject;
use crate::result::DomainResult;

fn known_input(version: &str, digest: &str) -> crate::ci_report::model::InputProvenance {
    crate::ci_report::model::InputProvenance {
        version: ValueState::known_version(version),
        digest: ValueState::known_digest(digest),
    }
}

fn sample_provenance() -> crate::ci_report::model::Provenance {
    crate::ci_report::model::Provenance {
        git: crate::ci_report::model::GitProvenance {
            commit: ValueState::known_revision("a".repeat(40)),
            dirty: ValueState::known_flag(false),
            working_set_digest: ValueState::known_digest(format!("sha256:{}", "b".repeat(64))),
        },
        model: known_input("0.2.16", &format!("sha256:{}", "1".repeat(64))),
        ir: known_input("0.2.16", &format!("sha256:{}", "2".repeat(64))),
        lock: known_input("0.6.3", &format!("sha256:{}", "3".repeat(64))),
        profiles: vec![crate::ci_report::model::ProfileProvenance {
            id: "validation-profile.default".to_owned(),
            version: ValueState::known_version("0.6.3"),
            digest: ValueState::known_digest(format!("sha256:{}", "4".repeat(64))),
        }],
        adapters: vec![crate::ci_report::model::AdapterProvenance {
            id: "node-typescript".to_owned(),
            version: ValueState::known_version("0.6.3"),
            digest: ValueState::known_digest(format!("sha256:{}", "5".repeat(64))),
        }],
    }
}

fn passing_outcome() -> CommandOutcome {
    CommandOutcome {
        command: CommandName::Validate,
        mode: "default".to_owned(),
        targets: Vec::new(),
        modules: Vec::new(),
        locked: false,
        provenance: sample_provenance(),
        checks: vec![CheckDraft::pass("model.validation")],
        suites: Vec::new(),
        result: DomainResult::version("0.6.3"),
        as_of: None,
    }
}

fn blocked_check(
    id: &str,
    required: bool,
    outcome: SourceOutcome,
    class: FailureClass,
) -> CheckDraft {
    CheckDraft {
        id: id.to_owned(),
        required,
        source_outcome: outcome,
        failure_class: class,
        diagnostic_indexes: Vec::new(),
        detail: if outcome == SourceOutcome::Pass {
            String::new()
        } else {
            "core.capability-unavailable".to_owned()
        },
    }
}

/// A valid registered diagnostic for the projection tests.
fn located_diagnostic() -> crate::diagnostics::Diagnostic {
    let source = crate::diagnostics::SourceLocation {
        path: Some("lekalo/modules/alpha/entities.yaml".to_owned()),
        range: Some(crate::diagnostics::Range {
            start: crate::diagnostics::Position {
                byte: 0,
                line: 3,
                column: 7,
            },
            end: crate::diagnostics::Position {
                byte: 12,
                line: 3,
                column: 19,
            },
        }),
    };
    diagnostic_build(
        "loader.reference-without-import",
        None,
        Some(source),
        DataObject::new(),
    )
    .expect("registered rule")
}

fn report_with(diagnostic: crate::diagnostics::Diagnostic) -> CiReport {
    let mut outcome = passing_outcome();
    outcome.checks.push(CheckDraft {
        id: "loader".to_owned(),
        required: true,
        source_outcome: SourceOutcome::Fail,
        failure_class: FailureClass::EvidenceInvalid,
        diagnostic_indexes: vec![0],
        detail: "loader.reference-without-import".to_owned(),
    });
    outcome.result = DomainResult::invalid(
        crate::diagnostics::DiagnosticSet::try_from_unsorted(
            vec![diagnostic.clone()],
            crate::result::Status::Invalid,
        )
        .expect("set"),
    );
    let report = build(outcome, CiPolicy::Default);
    crate::ci_report::build::with_diagnostics(report, vec![diagnostic])
}

#[test]
fn ready_run_exit_zero_and_deterministic_bytes() {
    let report = build(passing_outcome(), CiPolicy::Default);
    report.validate().expect("invariants hold");
    assert_eq!(
        report.evaluation.verdict,
        crate::ci_report::model::Verdict::Ready
    );
    assert_eq!(report.evaluation.exit_code, 0);
    assert_eq!(report.evaluation.coverage, Coverage::Complete);
    let first = report.to_json_string();
    let second = build(passing_outcome(), CiPolicy::Default).to_json_string();
    assert_eq!(first, second, "bytes are deterministic");
    assert!(first.ends_with('\n') && !first.ends_with("\n\n"));
    assert!(first.starts_with("{\"schema_version\":\"lekalo/ci-report/v0.6.3\""));
}

#[test]
fn required_failure_is_nonzero_optional_unavailable_is_policy_driven() {
    // Required unavailable: an error under every policy.
    for policy in [CiPolicy::Default, CiPolicy::Strict, CiPolicy::Lenient] {
        let mut outcome = passing_outcome();
        outcome.checks = vec![blocked_check(
            "native.gates",
            true,
            SourceOutcome::Unavailable,
            FailureClass::MissingComponent,
        )];
        let report = build(outcome, policy);
        report.validate().expect("invariants hold");
        assert_eq!(
            report.evaluation.verdict,
            crate::ci_report::model::Verdict::Blocked
        );
        assert_eq!(
            report.evaluation.exit_code, 4,
            "required absence stays error"
        );
        assert_eq!(report.evaluation.status, "unavailable");
    }
    // Optional unavailable: warn under default (exit 0), error under
    // strict (exit 4), skip under lenient (exit 0).
    let optional = |policy| {
        let mut outcome = passing_outcome();
        outcome.checks = vec![blocked_check(
            "trace.summary",
            false,
            SourceOutcome::Unavailable,
            FailureClass::MissingComponent,
        )];
        build(outcome, policy)
    };
    let default = optional(CiPolicy::Default);
    assert_eq!(default.evaluation.exit_code, 0);
    assert_eq!(
        default.evaluation.verdict,
        crate::ci_report::model::Verdict::Degraded
    );
    assert_eq!(default.checks[0].effective_outcome, EffectiveOutcome::Warn);
    assert_eq!(default.evaluation.coverage, Coverage::Incomplete);
    let strict = optional(CiPolicy::Strict);
    assert_eq!(strict.evaluation.exit_code, 4);
    assert_eq!(
        strict.evaluation.verdict,
        crate::ci_report::model::Verdict::Blocked
    );
    assert_eq!(strict.checks[0].effective_outcome, EffectiveOutcome::Error);
    let lenient = optional(CiPolicy::Lenient);
    assert_eq!(lenient.evaluation.exit_code, 0);
    assert_eq!(lenient.checks[0].effective_outcome, EffectiveOutcome::Skip);
}

#[test]
fn optional_assertion_failure_never_downgrades_to_a_pass() {
    let mut outcome = passing_outcome();
    outcome.checks = vec![blocked_check(
        "scenarios.execution",
        false,
        SourceOutcome::Fail,
        FailureClass::Assertion,
    )];
    let report = build(outcome, CiPolicy::Lenient);
    report.validate().expect("invariants hold");
    assert_eq!(
        report.evaluation.exit_code, 1,
        "a genuine failure still fails"
    );
}

#[test]
fn security_and_cancellation_rows_are_never_optional() {
    // The worst underlying class crosses the failing rows: a denied row
    // keeps the denied class even when the command result was invalid.
    for (outcome, class, expected_status, expected_exit) in [
        (SourceOutcome::Denied, FailureClass::Security, "denied", 3u8),
        (
            SourceOutcome::Cancelled,
            FailureClass::Infrastructure,
            "invalid",
            1u8,
        ),
    ] {
        let mut run = passing_outcome();
        run.checks = vec![blocked_check(
            "adapter.node-typescript",
            false,
            outcome,
            class,
        )];
        let report = build(run, CiPolicy::Lenient);
        assert_eq!(
            report.evaluation.status, expected_status,
            "{outcome:?} is never policy-waived",
        );
        assert_eq!(report.evaluation.exit_code, expected_exit);
    }
}

#[test]
fn underlying_command_failure_is_preserved_verbatim() {
    let mut outcome = passing_outcome();
    outcome.checks = vec![CheckDraft::pass("model.validation")];
    outcome.result = DomainResult::unsupported_version(
        crate::diagnostics::DiagnosticSet::try_from_unsorted(
            vec![crate::diagnostics::normalize::build(
                "versioning.unsupported-version",
                None,
                None,
                DataObject::new(),
            )
            .expect("registered rule")],
            crate::result::Status::UnsupportedVersion,
        )
        .expect("set"),
    );
    // A version refusal is never downgraded by a clean check list.
    let report = build(outcome, CiPolicy::Default);
    assert_eq!(report.command_result.status, "unsupported-version");
    assert_eq!(report.command_result.exit_code, 5);
    assert_eq!(report.evaluation.exit_code, 5);
}

#[test]
fn unknown_provenance_carries_no_fabricated_values() {
    let mut outcome = passing_outcome();
    outcome.provenance.git = crate::ci_report::model::GitProvenance {
        commit: ValueState::unknown(UnknownReason::NotARepository),
        dirty: ValueState::unknown(UnknownReason::NotARepository),
        working_set_digest: ValueState::unknown(UnknownReason::NotARepository),
    };
    outcome.provenance.lock = crate::ci_report::model::InputProvenance {
        version: ValueState::unknown(UnknownReason::Absent),
        digest: ValueState::unknown(UnknownReason::Absent),
    };
    let report = build(outcome, CiPolicy::Default);
    let json = report.to_json_string();
    assert!(json.contains("\"state\":\"unknown\",\"reason\":\"not-a-repository\""));
    assert!(json.contains("\"state\":\"unknown\",\"reason\":\"absent\""));
}

#[test]
fn suite_scenario_failures_and_infrastructure_separate_in_junit() {
    let mut outcome = passing_outcome();
    outcome.command = CommandName::Verify;
    outcome.mode = "full".to_owned();
    outcome.suites = vec![SuiteDraft {
        id: "scenario.node-typescript.planner".to_owned(),
        kind: SuiteKind::Scenario,
        target: Some("node-typescript".to_owned()),
        cases: vec![
            CaseDraft::pass("planner.focus_happy/emitted/0"),
            CaseDraft {
                id: "planner.focus_sad/emitted/0".to_owned(),
                required: true,
                source_outcome: SourceOutcome::Fail,
                failure_class: FailureClass::Assertion,
                diagnostic_indexes: Vec::new(),
                detail: "expectation-mismatch".to_owned(),
            },
            CaseDraft {
                id: "planner.focus_boot/run/0".to_owned(),
                required: true,
                source_outcome: SourceOutcome::Fail,
                failure_class: FailureClass::Infrastructure,
                diagnostic_indexes: Vec::new(),
                detail: "scenario.infrastructure".to_owned(),
            },
            CaseDraft {
                id: "planner.focus_unsupported/emitted/0".to_owned(),
                required: false,
                source_outcome: SourceOutcome::Unsupported,
                failure_class: FailureClass::MissingComponent,
                diagnostic_indexes: Vec::new(),
                detail: "scenario.unsupported-capability".to_owned(),
            },
        ],
    }];
    let report = build(outcome, CiPolicy::Default);
    report.validate().expect("invariants hold");
    let document = junit::render(&report);
    assert!(document.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(document.contains("<failure type=\"assertion\" message=\"expectation-mismatch\"/>"));
    assert!(document.contains("<error type=\"infrastructure\""));
    assert!(document.contains("<skipped message=\"unsupported\"/>"));
    assert!(!document.contains("system-out"));
    assert!(!document.contains("system-err"));
    // Deterministic bytes.
    let again = {
        let mut outcome = passing_outcome();
        outcome.command = CommandName::Verify;
        outcome.mode = "full".to_owned();
        outcome.suites = vec![SuiteDraft {
            id: "scenario.node-typescript.planner".to_owned(),
            kind: SuiteKind::Scenario,
            target: Some("node-typescript".to_owned()),
            cases: vec![
                CaseDraft::pass("planner.focus_happy/emitted/0"),
                CaseDraft {
                    id: "planner.focus_sad/emitted/0".to_owned(),
                    required: true,
                    source_outcome: SourceOutcome::Fail,
                    failure_class: FailureClass::Assertion,
                    diagnostic_indexes: Vec::new(),
                    detail: "expectation-mismatch".to_owned(),
                },
                CaseDraft {
                    id: "planner.focus_boot/run/0".to_owned(),
                    required: true,
                    source_outcome: SourceOutcome::Fail,
                    failure_class: FailureClass::Infrastructure,
                    diagnostic_indexes: Vec::new(),
                    detail: "scenario.infrastructure".to_owned(),
                },
                CaseDraft {
                    id: "planner.focus_unsupported/emitted/0".to_owned(),
                    required: false,
                    source_outcome: SourceOutcome::Unsupported,
                    failure_class: FailureClass::MissingComponent,
                    diagnostic_indexes: Vec::new(),
                    detail: "scenario.unsupported-capability".to_owned(),
                },
            ],
        }];
        junit::render(&build(outcome, CiPolicy::Default))
    };
    assert_eq!(document, again);
}

#[test]
fn junit_escapes_hostile_case_text() {
    let mut outcome = passing_outcome();
    outcome.suites = vec![SuiteDraft {
        id: "scenario.evil".to_owned(),
        kind: SuiteKind::Scenario,
        target: None,
        cases: vec![CaseDraft {
            id: "x\" onmouseover=\"<script>".to_owned(),
            required: true,
            source_outcome: SourceOutcome::Fail,
            failure_class: FailureClass::Assertion,
            diagnostic_indexes: Vec::new(),
            detail: "&<>'\"".to_owned(),
        }],
    }];
    let document = junit::render(&build(outcome, CiPolicy::Default));
    assert!(!document.contains("\" onmouseover="));
    assert!(document.contains("&quot; onmouseover=&quot;"));
}

#[test]
fn sarif_surfaces_diagnostics_with_safe_locations_inline() {
    let report = report_with(located_diagnostic());
    let document = sarif::render(&report);
    let parsed: serde_json::Value = serde_json::from_str(&document).expect("sarif parses");
    assert_eq!(parsed["version"], "2.1.0");
    let run = &parsed["runs"][0];
    assert_eq!(run["tool"]["driver"]["name"], "Lekalo");
    assert_eq!(run["tool"]["driver"]["semanticVersion"], "0.6.3");
    assert_eq!(run["columnKind"], "unicodeCodePoints");
    let result = &run["results"][0];
    assert_eq!(result["level"], "error");
    assert!(result["ruleId"].as_str().unwrap().starts_with("LEK-"));
    let location = &result["locations"][0]["physicalLocation"];
    assert_eq!(location["artifactLocation"]["uriBaseId"], "%SRCROOT%");
    assert_eq!(
        location["artifactLocation"]["uri"],
        "lekalo/modules/alpha/entities.yaml"
    );
    assert_eq!(location["region"]["startLine"], 3);
    assert_eq!(location["region"]["startColumn"], 7);
    assert_eq!(
        run["properties"]["lekaloReportDigest"],
        report.digest_spelling()
    );
    assert_eq!(run["properties"]["lekaloExitCode"], 1);
    // One rule per unique id, deterministic index.
    assert_eq!(run["tool"]["driver"]["rules"].as_array().unwrap().len(), 1);
    assert_eq!(result["ruleIndex"], 0);
}

#[test]
fn sarif_is_deterministic_and_locationless_rules_stay_present() {
    let mut first = report_with(located_diagnostic());
    let mut second = report_with(located_diagnostic());
    assert_eq!(sarif::render(&first), sarif::render(&second));
    // A second, locationless diagnostic stays in SARIF without a location.
    let locationless =
        diagnostic_build("cli.usage", None, None, DataObject::new()).expect("registered rule");
    second
        .diagnostics
        .push(crate::ci_report::model::SerializableDiagnostic::new(
            locationless,
        ));
    second.diagnostic_indexes.push(1);
    let document = sarif::render(&second);
    let parsed: serde_json::Value = serde_json::from_str(&document).expect("parses");
    assert_eq!(parsed["runs"][0]["results"].as_array().unwrap().len(), 2);
    assert!(parsed["runs"][0]["results"][1].get("locations").is_none());
    let _ = &mut first;
}

#[test]
fn markdown_summary_is_concise_bounded_and_escaped() {
    let report = report_with(located_diagnostic());
    let summary = markdown::render(&report);
    assert!(summary.contains("## Lekalo validate"));
    assert!(summary.contains("exit 1"));
    assert!(summary.contains("required failures: 1"));
    // Hostile text is escaped, table pipes intact.
    assert!(summary.contains("| pin | value |"));
    assert!(summary.contains("`lekalo/modules/alpha/entities.yaml:3:7`"));
    assert!(!summary.contains("<script>"));
    assert!(summary.ends_with('\n'));
    // Truncation note appears for an over-limit set.
    let mut big = report_with(located_diagnostic());
    for index in 0..30 {
        let diagnostic = diagnostic_build(
            "cli.usage",
            Some(format!("symbol.{index}")),
            None,
            DataObject::new(),
        )
        .expect("registered");
        big.diagnostics
            .push(crate::ci_report::model::SerializableDiagnostic::new(
                diagnostic,
            ));
        big.diagnostic_indexes.push(index + 1);
    }
    let truncated = markdown::render(&big);
    assert!(truncated.contains("more diagnostics"));
}

#[test]
fn diagnostic_indexes_stay_sorted_unique_and_dangling_free() {
    let report = report_with(located_diagnostic());
    assert_eq!(report.diagnostic_indexes, vec![0]);
    let mut outcome = passing_outcome();
    outcome.checks = vec![CheckDraft {
        id: "a".to_owned(),
        required: true,
        source_outcome: SourceOutcome::Fail,
        failure_class: FailureClass::Assertion,
        diagnostic_indexes: vec![5],
        detail: String::new(),
    }];
    let broken = build(outcome, CiPolicy::Default);
    assert!(broken.validate().is_err(), "dangling index fails closed");
}

#[test]
fn report_digest_is_content_bound() {
    let first = build(passing_outcome(), CiPolicy::Default);
    let mut changed_input = passing_outcome();
    changed_input.locked = true;
    let second = build(changed_input, CiPolicy::Default);
    assert_ne!(
        first.digest(),
        second.digest(),
        "any pin change moves the digest"
    );
    assert!(first.digest_spelling().starts_with("sha256:"));
}

#[test]
fn policy_levels_are_closed() {
    assert_eq!(CiPolicy::parse("default"), Some(CiPolicy::Default));
    assert_eq!(CiPolicy::parse("strict"), Some(CiPolicy::Strict));
    assert_eq!(CiPolicy::parse("lenient"), Some(CiPolicy::Lenient));
    assert_eq!(CiPolicy::parse("yolo"), None);
}

#[test]
fn check_rows_apply_policy_without_touching_source_outcome() {
    let draft = blocked_check(
        "trace.summary",
        false,
        SourceOutcome::Unavailable,
        FailureClass::MissingComponent,
    );
    let row = apply_check_policy(draft, CiPolicy::Strict);
    assert_eq!(row.source_outcome, SourceOutcome::Unavailable);
    assert_eq!(row.effective_outcome, EffectiveOutcome::Error);
}

#[test]
fn case_row_effective_outcome_follows_the_exit_policy() {
    let case = |required, outcome, class| CaseRow {
        id: "case".to_owned(),
        required,
        source_outcome: outcome,
        failure_class: class,
        diagnostic_indexes: Vec::new(),
        detail: String::new(),
    };
    assert_eq!(
        case(true, SourceOutcome::Pass, FailureClass::None).effective_outcome(),
        EffectiveOutcome::Pass
    );
    assert_eq!(
        case(false, SourceOutcome::Fail, FailureClass::Assertion).effective_outcome(),
        EffectiveOutcome::Fail
    );
    assert_eq!(
        case(
            false,
            SourceOutcome::Unavailable,
            FailureClass::MissingComponent
        )
        .effective_outcome(),
        EffectiveOutcome::Warn
    );
    assert_eq!(
        case(
            true,
            SourceOutcome::Unavailable,
            FailureClass::MissingComponent
        )
        .effective_outcome(),
        EffectiveOutcome::Error
    );
}
