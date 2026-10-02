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
use lekalo_core::ci_report::build::CiPolicy;
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

/// The clap surface of the closed CI policy vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CiPolicyArg {
    /// Optional absences warn (exit 0, incomplete coverage).
    Default,
    /// Optional absences promote to the unavailable class (exit 4).
    Strict,
    /// Optional absences are skipped (exit 0).
    Lenient,
}

impl CiPolicyArg {
    /// The closed policy this argument selects.
    pub const fn policy(self) -> ci_report::build::CiPolicy {
        match self {
            Self::Default => ci_report::build::CiPolicy::Default,
            Self::Strict => ci_report::build::CiPolicy::Strict,
            Self::Lenient => ci_report::build::CiPolicy::Lenient,
        }
    }
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
#[derive(Debug, Clone)]
pub struct ReportRequest {
    /// The destination path (`--report-file`), when requested.
    pub file: Option<String>,
    /// The selected projection (defaults to JSON with a file, and must
    /// not be named without one).
    pub format: Option<ReportFormat>,
    /// The closed CI policy level (`--ci-policy`): how optional
    /// absences translate into the gated exit. `default` warns,
    /// `strict` promotes to the unavailable class, `lenient` skips.
    pub policy: CiPolicy,
}

impl Default for ReportRequest {
    fn default() -> Self {
        Self {
            file: None,
            format: None,
            policy: CiPolicy::Default,
        }
    }
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

/// The protected project-relative homes a report must never overlap:
/// model sources, the lock, the ownership manifests, generated artifacts,
/// history, caches, and the adjudicated imports (research line 228:
/// reject destinations overlapping source/model/locks/baselines/history).
const PROTECTED_PREFIXES: &[&str] = &["lekalo/", ".lekalo/", "apps/"];

/// Whether one path component chain resolves to, through, or beside a
/// symbolic link. Every existing ancestor is no-follow checked so a
/// linked parent cannot redirect the write out of the granted tree.
fn parent_chain_has_links(path: &Path) -> bool {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(name) => {
                prefix.push(name);
                let Ok(metadata) = std::fs::symlink_metadata(&prefix) else {
                    return false; // missing parts cannot link
                };
                if metadata.file_type().is_symlink() {
                    return true;
                }
            }
            _ => prefix.push(component.as_os_str()),
        }
    }
    false
}

/// Validate the destination before any work. The destination must be
/// absent (the writer creates it) or a pre-created empty regular file;
/// an existing non-empty file is refused outright, so a report can
/// never truncate a source, the lock, a baseline, or any other
/// analyzed input. The parent must be an existing directory, every
/// existing ancestor must be link-free, and the destination must not
/// sit inside a protected project home. The path is never
/// canonicalized into the report.
fn validate_destination(path: &Path, project_root: Option<&Path>) -> Result<(), DomainResult> {
    use std::io::Read;
    let Some(parent) = path.parent() else {
        return Err(write_failure("path-invalid"));
    };
    let metadata = match std::fs::symlink_metadata(parent) {
        Ok(metadata) => metadata,
        Err(_) => return Err(write_failure("directory-missing")),
    };
    if metadata.is_symlink() || !metadata.is_dir() {
        return Err(write_failure("directory-missing"));
    }
    if parent_chain_has_links(path) {
        return Err(write_failure("path-invalid"));
    }
    // Protected-home overlap: reject destination paths inside the
    // project's model/lock/manifest/generated/history/cache homes. The
    // destination may be relative to the invocation directory while the
    // root is absolute: compare the canonicalized spellings when both
    // resolve (an unresolvable destination falls through to the
    // existence checks below).
    if let Some(root) = project_root {
        let destination_absolute = if path.is_absolute() {
            Some(path.to_path_buf())
        } else {
            std::env::current_dir().ok().map(|cwd| cwd.join(path))
        };
        if let Some(absolute) = destination_absolute {
            if let Ok(relative) = absolute.strip_prefix(root) {
                let text = relative.to_string_lossy().replace('\\', "/");
                if PROTECTED_PREFIXES
                    .iter()
                    .any(|prefix| text.starts_with(prefix))
                {
                    return Err(write_failure("protected-path"));
                }
            }
        }
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(write_failure("path-invalid")),
        Ok(metadata) if !metadata.is_file() => Err(write_failure("path-invalid")),
        Ok(metadata) => {
            // Pre-created regular file: only an exactly-empty one is
            // acceptable (the atomic-create pattern below then fills
            // it). Any existing content means a real file was named;
            // truncating analyzed inputs is never permitted.
            if metadata.len() == 0 {
                Ok(())
            } else {
                // Belt and braces: refuse anything non-empty even if a
                // race emptied it between stat and open.
                let empty = std::fs::File::open(path)
                    .and_then(|mut file| file.read_to_end(&mut Vec::new()).map(|_| Vec::new()))
                    .map(|bytes: Vec<u8>| bytes.is_empty())
                    .unwrap_or(false);
                if empty {
                    Ok(())
                } else {
                    Err(write_failure("file-exists"))
                }
            }
        }
        Err(_) => Ok(()),
    }
}

/// The exact projection bytes of one report, or the typed secret
/// refusal when the pre-publication scan finds secret material in the
/// rendered bytes (issue #103: admission runs before every sink).
fn projection(report: &CiReport, format: ReportFormat) -> Result<String, DomainResult> {
    let rendered = match format {
        ReportFormat::Json => report.to_json_string(),
        ReportFormat::Junit => ci_report::junit::render(report),
        ReportFormat::Sarif => {
            let mut bytes = ci_report::sarif::render(report);
            bytes.push('\n');
            bytes
        }
        ReportFormat::Md => ci_report::markdown::render(report),
    };
    if let Some(refusal) = ci_report::build::scan_rendered(&rendered) {
        return Err(write_failure(refusal.as_str()));
    }
    Ok(rendered)
}

/// Write one report to its granted destination. The destination is
/// re-validated with the project root (protected-home overlap), then
/// created through `create_new` (an atomic fail-if-exists open) or
/// opened write-only on an exactly-empty pre-created file — never
/// through a truncating create. Returns the typed write failure on any
/// refusal; the caller composes it with the command result (a report
/// failure never masks a check failure and a check failure is never
/// overwritten by a report success).
pub fn write_report(
    report: &CiReport,
    request: &ReportRequest,
    project_root: Option<&Path>,
) -> Result<(), DomainResult> {
    let Some(file) = request.file.as_deref() else {
        return Ok(());
    };
    let path = PathBuf::from(file);
    validate_destination(&path, project_root)?;
    let bytes = projection(report, request.resolved_format())?;
    let write = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(bytes.as_bytes()).and_then(|()| file.flush())
        }
        Err(_) if path.is_file() => {
            // The exactly-empty pre-created file admitted by validation
            // (e.g. a touch-created CI artifact placeholder): open
            // without truncation and overwrite the zero bytes.
            use std::io::Write;
            std::fs::OpenOptions::new()
                .write(true)
                .truncate(false)
                .open(&path)
                .and_then(|mut file| file.write_all(bytes.as_bytes()).and_then(|()| file.flush()))
        }
        Err(_) => Err(std::io::Error::other("refused")),
    };
    match write {
        Ok(()) => Ok(()),
        Err(_) => Err(write_failure("write-denied")),
    }
}

/// Compose the terminal domain result of a reporting run: the command
/// result wins unless the report itself failed; a report failure after
/// a successful command becomes the typed unavailable result, and a
/// report failure after a failed command **joins** the failed
/// command's diagnostics (review F10): the write-refusal fact is
/// appended to the returned envelope so it is never silently dropped,
/// while the command's exit class stays authoritative.
pub fn compose(
    mut command: DomainResult,
    report_outcome: Result<(), DomainResult>,
) -> DomainResult {
    match report_outcome {
        Ok(()) => command,
        Err(write_failure) => {
            if command.exit_code() == 0 {
                write_failure
            } else {
                // Preserve the command's class; the report-refusal
                // diagnostics ride along so the fact is observable.
                let mut joined: Vec<lekalo_core::diagnostics::Diagnostic> =
                    command.diagnostics().to_vec();
                joined.extend(write_failure.diagnostics().iter().cloned());
                if let Ok(set) = lekalo_core::diagnostics::DiagnosticSet::try_from_unsorted(
                    joined,
                    command.status(),
                ) {
                    command = match command.status() {
                        lekalo_core::result::Status::Invalid => DomainResult::invalid(set),
                        lekalo_core::result::Status::Denied => DomainResult::denied(set),
                        lekalo_core::result::Status::Unavailable => DomainResult::unavailable(set),
                        lekalo_core::result::Status::UnsupportedVersion => {
                            DomainResult::unsupported_version(set)
                        }
                        // The Valid and Unsupported classes carry no
                        // report-refusal composition (a successful or
                        // negotiated-unsupported command never reaches
                        // this branch with a failing write).
                        lekalo_core::result::Status::Valid
                        | lekalo_core::result::Status::Unsupported => command,
                    };
                }
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
            ..ReportRequest::default()
        };
        assert!(request.validate().is_err());
        assert!(request.is_requested());
    }

    #[test]
    fn file_defaults_to_json_projection() {
        let request = ReportRequest {
            file: Some("report.json".to_owned()),
            format: None,
            ..ReportRequest::default()
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
            ..ReportRequest::default()
        };
        let outcome = write_report(&report, &request, None);
        assert_eq!(outcome.unwrap_err().exit_code(), 4);
    }
}
