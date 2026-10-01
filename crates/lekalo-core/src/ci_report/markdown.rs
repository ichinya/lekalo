//! The concise Markdown job-summary projection of a CI report
//! (issue #103).
//!
//! Command/verdict/exit, required-failed and optional-unavailable
//! counts, the exact git/model/lock/profile pins, a bounded diagnostic
//! table with safe positions, and explicit truncation counts. All
//! Markdown/HTML/link syntax inside borrowed text is escaped; the
//! document never carries raw details, URLs, or absolute paths.

use super::model::{CaseRow, CiReport, EffectiveOutcome, SourceOutcome};

/// Escape Markdown special characters and suppress raw HTML/link
/// syntax in any text the report echoes.
fn md_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            '*' => out.push_str("\\*"),
            '_' => out.push_str("\\_"),
            '[' | ']' => out.push_str(&format!("\\{ch}")),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '|' => out.push_str("\\|"),
            '\r' | '\n' => out.push(' '),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

/// The bounded diagnostic-table row count per summary.
const MAX_TABLE_ROWS: usize = 20;

/// Render the concise Markdown summary (one trailing LF).
pub fn render(report: &CiReport) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "## Lekalo {} — {} (exit {})",
        md_escape(report.invocation.command.as_str()),
        md_escape(report.evaluation.verdict.as_str()),
        report.evaluation.exit_code,
    ));
    lines.push(String::new());
    lines.push(format!(
        "- command result: `{}` (exit {}); evaluation: `{}` (exit {}), coverage `{}`, complete: {}",
        md_escape(report.command_result.status),
        report.command_result.exit_code,
        md_escape(report.evaluation.status),
        report.evaluation.exit_code,
        md_escape(report.evaluation.coverage.as_str()),
        report.evaluation.complete,
    ));
    // Counts from the check rows.
    let required_failed = report
        .checks
        .iter()
        .filter(|check| check.required && check.effective_outcome == EffectiveOutcome::Fail)
        .count();
    let optional_unavailable = report
        .checks
        .iter()
        .filter(|check| {
            !check.required
                && matches!(
                    check.source_outcome,
                    SourceOutcome::Unavailable | SourceOutcome::Unsupported
                )
        })
        .count();
    lines.push(format!(
        "- required failures: {required_failed}; optional unavailable/supported-absent: {optional_unavailable}"
    ));
    let suite_cases: usize = report.suites.iter().map(|suite| suite.cases.len()).sum();
    let suite_failures: usize = report
        .suites
        .iter()
        .flat_map(|suite| suite.cases.iter())
        .map(|case: &CaseRow| usize::from(case.effective_outcome() == EffectiveOutcome::Fail))
        .sum();
    let suite_errors: usize = report
        .suites
        .iter()
        .flat_map(|suite| suite.cases.iter())
        .map(|case: &CaseRow| usize::from(case.effective_outcome() == EffectiveOutcome::Error))
        .sum();
    lines.push(format!(
        "- suites: {} ({} cases, {} failures, {} errors)",
        report.suites.len(),
        suite_cases,
        suite_failures,
        suite_errors,
    ));
    lines.push(String::new());
    lines.push("### Revisions".to_owned());
    lines.push(String::new());
    lines.push("| pin | value |".to_owned());
    lines.push("| --- | --- |".to_owned());
    let pin = |state: &super::model::ValueState| match state {
        super::model::ValueState::Known { value } => match value {
            super::model::KnownValue::Revision(text) => format!("`{}`", md_escape(text)),
            super::model::KnownValue::Version(text) => format!("`{}`", md_escape(text)),
            super::model::KnownValue::Flag(flag) => format!("`{flag}`"),
            super::model::KnownValue::Text(text) => format!("`{}`", md_escape(text)),
        },
        super::model::ValueState::Unknown { reason } => {
            format!("`unknown/{}`", md_escape(reason.as_str()))
        }
    };
    lines.push(format!(
        "| git commit | {} |",
        pin(&report.provenance.git.commit)
    ));
    lines.push(format!(
        "| git dirty | {} |",
        pin(&report.provenance.git.dirty)
    ));
    lines.push(format!(
        "| model | {} |",
        pin(&report.provenance.model.version)
    ));
    lines.push(format!("| ir | {} |", pin(&report.provenance.ir.version)));
    lines.push(format!(
        "| lock | {} |",
        pin(&report.provenance.lock.digest)
    ));
    for profile in &report.provenance.profiles {
        lines.push(format!(
            "| profile {} | {} |",
            md_escape(&profile.id),
            pin(&profile.digest)
        ));
    }
    for adapter in &report.provenance.adapters {
        lines.push(format!(
            "| adapter {} | {} |",
            md_escape(&adapter.id),
            pin(&adapter.digest)
        ));
    }
    lines.push(String::new());
    if !report.diagnostics.is_empty() {
        lines.push("### Diagnostics".to_owned());
        lines.push(String::new());
        lines.push("| code | severity | id | at | message |".to_owned());
        lines.push("| --- | --- | --- | --- | --- |".to_owned());
        for diagnostic in report.diagnostics.iter().take(MAX_TABLE_ROWS) {
            let item = serde_json::to_value(diagnostic).expect("diagnostic serializes");
            let path = item["source"]["path"].as_str().unwrap_or_default();
            let line = item["source"]["range"]["start"]["line"]
                .as_u64()
                .unwrap_or(0);
            let column = item["source"]["range"]["start"]["column"]
                .as_u64()
                .unwrap_or(0);
            let at = if path.is_empty() {
                String::new()
            } else {
                format!("`{}:{line}:{column}`", md_escape(path))
            };
            lines.push(format!(
                "| `{}` | {} | `{}` | {} | {} |",
                md_escape(item["code"].as_str().unwrap_or_default()),
                md_escape(item["severity"].as_str().unwrap_or_default()),
                md_escape(item["id"].as_str().unwrap_or_default()),
                at,
                md_escape(item["message"].as_str().unwrap_or_default()),
            ));
        }
        let hidden = report.diagnostics.len().saturating_sub(MAX_TABLE_ROWS);
        if hidden > 0 {
            lines.push(format!("- … and {hidden} more diagnostics (truncated; the JSON/SARIF reports carry the full set)"));
        }
        lines.push(String::new());
    }
    lines.push(format!(
        "report digest: `{}`",
        md_escape(&report.digest_spelling())
    ));
    let mut out = lines.join("\n");
    out.push('\n');
    out
}
