//! The CI report output seam of the CLI (issue #103).
//!
//! `--report-file PATH` writes the closed ci-report document for the
//! four CI-surfaced commands (validate, generate --check, verify,
//! readiness); `--report-format` selects the side-channel projection
//! (json, junit, sarif, md). A named file is a side channel: the
//! status-owned stream and the exit class are untouched by reporting —
//! except that a report that cannot be written is a typed
//! `ci.report-write-failed` unavailable result (exit 4), never a
//! silent success with a missing artifact.
//!
//! Paths are confined before any byte is written: the destination must
//! be a new or truncatable regular file in an existing directory, and
//! only the granted report path itself is ever touched. Report bytes
//! are deterministic; the writer stages nothing and writes exactly one
//! file per run.

use std::path::{Path, PathBuf};

use lekalo_core::ci_report;
use lekalo_core::ci_report::CiReport;
use lekalo_core::DomainResult;

/// The closed side-channel format vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ReportFormat {
    /// The canonical JSON report.
    Json,
    /// The JUnit XML projection.
    Junit,
    /// The SARIF 2.1.0 projection.
    Sarif,
    /// The Markdown job-summary projection.
    Md,
}

#[allow(dead_code)] // the stable spelling is used by docs/tests and the future action seam
impl ReportFormat {
    /// The stable spelling used in errors and docs.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Junit => "junit",
            Self::Sarif => "sarif",
            Self::Md => "md",
        }
    }
}

/// The report output request gathered from the CLI flags.
#[derive(Debug, Clone, Default)]
pub struct ReportRequest {
    /// The destination path (`--report-file`), when requested.
    pub file: Option<String>,
    /// The selected projection (defaults to JSON with a file, and must
    /// not be named without one).
    pub format: Option<ReportFormat>,
}

impl ReportRequest {
    /// Whether any report was requested.
    pub fn is_requested(&self) -> bool {
        self.file.is_some() || self.format.is_some()
    }

    /// Validate the flag combination: a format without a path and an
    /// ambiguous combination are the stable usage failure.
    pub fn validate(&self) -> Result<(), DomainResult> {
        if self.format.is_some() && self.file.is_none() {
            return Err(DomainResult::usage_error());
        }
        Ok(())
    }

    /// The resolved format (JSON is the default for a named file).
    pub fn resolved_format(&self) -> ReportFormat {
        self.format.unwrap_or(ReportFormat::Json)
    }
}

/// The typed report-write failure: the `ci.report-write-failed`
/// unavailable envelope with the closed detail token.
fn write_failure(detail: &'static str) -> DomainResult {
    DomainResult::unavailable(ci_report::build::diagnostics::report_write_failed(detail))
}

/// Validate the destination before any work: the parent must be an
/// existing directory and the target must be absent or a regular,
/// non-link file. The path is never canonicalized into the report.
fn validate_destination(path: &Path) -> Result<(), DomainResult> {
    let Some(parent) = path.parent() else {
        return Err(write_failure("path-invalid"));
    };
    let metadata = match std::fs::metadata(parent) {
        Ok(metadata) => metadata,
        Err(_) => return Err(write_failure("directory-missing")),
    };
    if !metadata.is_dir() {
        return Err(write_failure("directory-missing"));
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(write_failure("path-invalid")),
        Ok(metadata) if !metadata.is_file() => Err(write_failure("path-invalid")),
        Ok(_) => Ok(()),
        Err(_) => Ok(()),
    }
}

/// The exact projection bytes of one report.
fn projection(report: &CiReport, format: ReportFormat) -> String {
    match format {
        ReportFormat::Json => report.to_json_string(),
        ReportFormat::Junit => ci_report::junit::render(report),
        ReportFormat::Sarif => {
            let mut bytes = ci_report::sarif::render(report);
            bytes.push('\n');
            bytes
        }
        ReportFormat::Md => ci_report::markdown::render(report),
    }
}

/// Write one report to its granted destination. Returns the typed
/// write failure on any refusal; the caller composes it with the
/// command result (a report failure never masks a check failure and a
/// check failure is never overwritten by a report success).
pub fn write_report(report: &CiReport, request: &ReportRequest) -> Result<(), DomainResult> {
    let Some(file) = request.file.as_deref() else {
        return Ok(());
    };
    let path = PathBuf::from(file);
    validate_destination(&path)?;
    let bytes = projection(report, request.resolved_format());
    let write = std::fs::File::create(&path).and_then(|mut file| {
        use std::io::Write;
        file.write_all(bytes.as_bytes()).and_then(|()| file.flush())
    });
    match write {
        Ok(()) => Ok(()),
        Err(_) => Err(write_failure("write-denied")),
    }
}

/// Compose the terminal domain result of a reporting run: the command
/// result wins unless the report itself failed; a report failure after
/// a successful command becomes the typed unavailable result, and a
/// report failure after a failed command keeps the command's exit
/// class on the status-owned stream (both facts stay visible in the
/// process exit set).
pub fn compose(command: DomainResult, report_outcome: Result<(), DomainResult>) -> DomainResult {
    match report_outcome {
        Ok(()) => command,
        Err(write_failure) => {
            if command.exit_code() == 0 {
                write_failure
            } else {
                command
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_without_file_is_a_usage_failure() {
        let request = ReportRequest {
            file: None,
            format: Some(ReportFormat::Sarif),
        };
        assert!(request.validate().is_err());
        assert!(request.is_requested());
    }

    #[test]
    fn file_defaults_to_json_projection() {
        let request = ReportRequest {
            file: Some("report.json".to_owned()),
            format: None,
        };
        assert!(request.validate().is_ok());
        assert_eq!(request.resolved_format(), ReportFormat::Json);
    }

    #[test]
    fn a_missing_directory_refuses_before_any_work() {
        let report = CiReport {
            schema_version: ci_report::model::SCHEMA_VERSION,
            identity: ci_report::model::IDENTITY,
            producer: ci_report::model::Producer {
                id: "lekalo",
                version: ci_report::model::REPORT_VERSION,
            },
            invocation: ci_report::model::Invocation {
                command: ci_report::model::CommandName::Validate,
                mode: "default".to_owned(),
                targets: Vec::new(),
                modules: Vec::new(),
                locked: false,
                as_of: None,
            },
            provenance: crate::report_git::empty_provenance(),
            command_result: ci_report::model::Outcome {
                status: "valid",
                exit_code: 0,
            },
            evaluation: ci_report::model::Evaluation {
                status: "valid",
                exit_code: 0,
                verdict: ci_report::model::Verdict::Ready,
                coverage: ci_report::model::Coverage::Complete,
                complete: true,
            },
            checks: Vec::new(),
            suites: Vec::new(),
            diagnostic_indexes: Vec::new(),
            diagnostics: Vec::new(),
            publication: ci_report::model::Publication {
                classification: "ci-derived",
                decision: ci_report::model::PublicationDecision::Allowed,
            },
        };
        let request = ReportRequest {
            file: Some("definitely/missing/dir/report.json".to_owned()),
            format: None,
        };
        let outcome = write_report(&report, &request);
        assert_eq!(outcome.unwrap_err().exit_code(), 4);
    }
}
