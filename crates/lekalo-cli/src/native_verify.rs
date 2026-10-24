//! Verify composes fresh semantic evaluation and actual native execution.
//! No result-file input can substitute for a run in this invocation.
use clap::Args;
use lekalo_core::{native_gate::execution, DomainResult, Status};
use serde_json::{json, Value};
use std::{io::Write, path::PathBuf};

#[derive(Debug, Default, Args)]
pub(crate) struct NativeVerifyArgs {
    /// Execute this exact plan after semantic verification; JSON becomes v1.0.0.
    #[arg(long,requires_all=["native_confirm","native_adapter","native_policy","native_profile","native_catalog"],conflicts_with_all=["report_file","report_format","ci_policy"])]
    pub native_plan: Option<String>,
    #[arg(long, requires = "native_plan")]
    native_confirm: Option<String>,
    #[arg(long, requires = "native_plan")]
    native_adapter: Option<PathBuf>,
    #[arg(long, requires = "native_plan")]
    native_policy: Option<PathBuf>,
    #[arg(long, requires = "native_plan")]
    native_profile: Option<PathBuf>,
    #[arg(long, requires = "native_plan")]
    native_catalog: Option<PathBuf>,
    #[arg(long, requires = "native_plan")]
    native_cancel_file: Option<PathBuf>,
}

fn status(result: &DomainResult) -> &'static str {
    match result.status() {
        Status::Valid => "passed",
        Status::Denied => "denied",
        Status::Unavailable => "infrastructure",
        Status::Unsupported | Status::UnsupportedVersion => "unsupported",
        Status::Invalid => "failed",
    }
}
fn value(result: &DomainResult) -> Value {
    serde_json::from_str(&result.to_json_string()).expect("domain JSON")
}

pub(crate) fn run(
    args: NativeVerifyArgs,
    root: PathBuf,
    json_output: bool,
    mut semantic: impl FnMut() -> DomainResult,
) -> u8 {
    let before = semantic();
    let before_value = value(&before);
    let mut exit = before.exit_code();
    let mut outcome = status(&before);
    let mut verdict = if exit == 0 {
        "ready"
    } else if exit == 4 || exit == 5 {
        "degraded"
    } else {
        "blocked"
    };
    let mut reasons: Vec<&str> = Vec::new();
    let mut native = json!({"state":"not-run","reason":"semantic-verification-refused"});
    let mut after = json!({"state":"not-run"});
    if exit == 0 {
        let result = (|| {
            let bytes = crate::native_execution::read_plan(
                args.native_plan.as_deref().expect("selected native plan"),
            )
            .map_err(|failure| DomainResult::from(&failure))?;
            crate::native_execution::execute_report(
                execution::ExecutionInputs {
                    root,
                    adapter: args.native_adapter.expect("required adapter"),
                    policy: args.native_policy.expect("required policy"),
                    profile: args.native_profile.expect("required profile"),
                    catalog: args.native_catalog.expect("required catalog"),
                },
                &bytes,
                args.native_confirm.as_deref().expect("required approval"),
                args.native_cancel_file,
            )
        })();
        match result {
            Ok(report) => {
                exit = match report.run.outcome.as_str() {
                    "passed" if report.run.verdict == "passed" => 0,
                    "security" => 3,
                    "missing" | "unsupported" | "infrastructure" => 4,
                    _ => 1,
                };
                outcome = match report.run.outcome.as_str() {
                    "passed" => "passed",
                    "security" => "security",
                    "infrastructure" => "infrastructure",
                    "missing" | "unsupported" => "unsupported",
                    _ => "failed",
                };
                verdict = match report.run.verdict.as_str() {
                    "passed" => "ready",
                    "degraded" => "degraded",
                    _ => "blocked",
                };
                native = serde_json::to_value(report).expect("actual native report");
            }
            Err(failure) => {
                exit = failure.exit_code();
                outcome = status(&failure);
                verdict = if exit == 4 || exit == 5 {
                    "degraded"
                } else {
                    "blocked"
                };
                native =
                    json!({"state":"error","execution":"unverified","diagnostic":value(&failure)});
            }
        }
        let checked = semantic();
        let checked_value = value(&checked);
        let priority = |code| match code {
            3 => 4,
            4 | 5 => 3,
            1 => 2,
            _ => 0,
        };
        if checked.exit_code() != 0 {
            reasons.push("semantic-after-refused");
            if priority(checked.exit_code()) > priority(exit) {
                exit = checked.exit_code();
                outcome = status(&checked);
                verdict = "blocked";
            }
        }
        if checked_value != before_value {
            reasons.push("semantic-custody-changed");
            if priority(exit) < priority(4) {
                exit = 4;
                outcome = "infrastructure";
                verdict = "blocked";
            }
        }
        after = json!({"state":"checked","report":checked_value});
    }
    let report = json!({"schema_version":"lekalo/native-verify-report/v1.0.0","kind":"native-verify-report","status":outcome,"verdict":verdict,"semantic":before_value,"semantic_after":after,"native":native,"exit_code":exit,"reason_codes":reasons});
    let rendered = if json_output {
        serde_json::to_string_pretty(&report).expect("verify report")
    } else {
        format!("verify : {outcome} ({verdict}); native output is retained by --json")
    };
    let written = if exit == 1 || exit == 5 {
        writeln!(std::io::stderr().lock(), "{rendered}")
    } else {
        writeln!(std::io::stdout().lock(), "{rendered}")
    };
    if written.is_err() {
        1
    } else {
        exit
    }
}
