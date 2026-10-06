//! Read-only task-links pilot commands (issue #52). No provider call,
//! no outbox write, no token import, no report persistence: the
//! commands join the closed attachment to the current compilation and
//! project typed views only.

use clap::Subcommand;
use lekalo_core::task_links as tl;
use lekalo_core::{loader::LoadSelection, DomainResult};
use std::io::Read;

/// The `task-links` subcommand surface: `validate`, `inspect`, and
/// `portability` with the exact proposed grammar.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Join the attachment to the current compilation and report the
    /// static obligations.
    Validate {
        /// The closed integration attachment path.
        #[arg(value_name = "PATH")]
        path: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The capability admission profile.
        #[arg(long, value_name = "PROFILE", default_value = "strict")]
        profile: String,
    },
    /// Display typed relations, decisions, effects, and admitted
    /// evidence; missing evidence stays unavailable.
    Inspect {
        /// The symbol to inspect.
        #[arg(value_name = "SYMBOL")]
        symbol: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The closed integration attachment path.
        #[arg(long, value_name = "PATH")]
        attachment: String,
        /// Optional admitted evidence document.
        #[arg(long, value_name = "PATH")]
        evidence: Option<String>,
    },
    /// Join resolved target/component profiles and provider contract
    /// requirements into the closed portability projection.
    Portability {
        /// The closed integration attachment path.
        #[arg(value_name = "PATH")]
        path: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The source profile document.
        #[arg(long, value_name = "PATH")]
        from_profile: String,
        /// The target profile document.
        #[arg(long, value_name = "PATH")]
        to_profile: String,
        /// The capability admission profile.
        #[arg(long, value_name = "PROFILE", default_value = "strict")]
        profile: String,
    },
}

fn invalid(detail: &'static str) -> DomainResult {
    tl::into_result(&tl::input_failure(detail))
}

fn read_bounded(path: &str) -> Result<Vec<u8>, DomainResult> {
    let file = std::fs::File::open(path).map_err(|_| invalid("attachment-unreadable"))?;
    if !file
        .metadata()
        .map_err(|_| invalid("attachment-unreadable"))?
        .is_file()
    {
        return Err(invalid("attachment-file"));
    }
    let mut bytes = vec![];
    file.take(tl::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("attachment-unreadable"))?;
    if bytes.len() > tl::MAX_BYTES {
        return Err(invalid("attachment-bound"));
    }
    Ok(bytes)
}

fn compilation(
    project: &Option<String>,
) -> Result<(lekalo_core::ir::Compilation, String), DomainResult> {
    let model = lekalo_core::loader::normalize_model(&LoadSelection {
        project: project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    })?;
    let compilation = lekalo_core::ir::compile(&model).map_err(|failure| failure.into_result())?;
    let project_id = compilation
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str().to_owned())
        .unwrap_or_default();
    Ok((compilation, project_id))
}

fn check_profile(profile: &str) -> Result<(), DomainResult> {
    if !tl::PROFILES.contains(&profile) {
        return Err(invalid("profile"));
    }
    Ok(())
}

/// One minimal target/component profile document: id plus digest pin.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetProfile {
    /// The profile id.
    #[serde(rename = "profileId")]
    profile_id: String,
}

fn profile_ref(path: &str) -> Result<tl::portability::ProfileRef, DomainResult> {
    let bytes = read_bounded(path)?;
    let digest = format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes));
    let profile: TargetProfile =
        serde_json::from_slice(&bytes).map_err(|_| invalid("profile-shape"))?;
    if profile.profile_id.is_empty() || profile.profile_id.len() > 256 {
        return Err(invalid("profile-shape"));
    }
    Ok(tl::portability::ProfileRef {
        profile_id: profile.profile_id,
        digest,
    })
}

pub fn run(command: Command) -> DomainResult {
    execute(command).unwrap_or_else(|result| result)
}

fn execute(command: Command) -> Result<DomainResult, DomainResult> {
    match command {
        Command::Validate {
            path,
            project,
            profile,
        } => {
            check_profile(&profile)?;
            let attachment = tl::parse(&read_bounded(&path)?).map_err(|f| tl::into_result(&f))?;
            let (compilation, _) = compilation(&project)?;
            let report = tl::validate(&attachment, &compilation, &profile)
                .map_err(|failure| tl::into_result(&failure))?;
            if tl::invalidates(&report) {
                Ok(tl::render_invalid(&report, &report.findings))
            } else if report.status == "gaps-admitted" {
                Ok(tl::render_with_gaps(&report, &report.findings))
            } else {
                Ok(tl::render(&report))
            }
        }
        Command::Inspect {
            symbol,
            project,
            attachment: attachment_path,
            evidence,
        } => {
            let _ = symbol;
            let attachment =
                tl::parse(&read_bounded(&attachment_path)?).map_err(|f| tl::into_result(&f))?;
            let (_, project_id) = compilation(&project)?;
            if attachment.project_id() != project_id {
                return Err(invalid("project-mismatch"));
            }
            let admitted = match &evidence {
                Some(path) => Some(
                    tl::admit_evidence(&read_bounded(path)?, &attachment)
                        .map_err(|f| tl::into_result(&f))?,
                ),
                None => None,
            };
            Ok(tl::render(&tl::view(&attachment, admitted)))
        }
        Command::Portability {
            path,
            project,
            from_profile,
            to_profile,
            profile,
        } => {
            check_profile(&profile)?;
            let attachment = tl::parse(&read_bounded(&path)?).map_err(|f| tl::into_result(&f))?;
            let (_, project_id) = compilation(&project)?;
            if attachment.project_id() != project_id {
                return Err(invalid("project-mismatch"));
            }
            let from = profile_ref(&from_profile)?;
            let to = profile_ref(&to_profile)?;
            let report =
                tl::portability::report(&attachment, from, to, &profile, Vec::new(), Vec::new());
            let gaps = tl::portability::findings(&report);
            if profile == "strict" && !gaps.is_empty() {
                Ok(tl::render_invalid(&report, &gaps))
            } else if !gaps.is_empty() {
                Ok(tl::render_with_gaps(&report, &gaps))
            } else {
                Ok(tl::render(&report))
            }
        }
    }
}
