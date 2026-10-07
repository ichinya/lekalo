//! Read-only task-links pilot commands (issue #52). No provider call,
//! no outbox write, no token import, no report persistence: the
//! commands join the closed attachment to the current compilation and
//! project typed views only. Every digest pin is recomputed here (the
//! adapter owns the reads; the core stays pure) and a divergent pin
//! fails closed before any view, validation, or projection runs.

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
    /// evidence for the requested symbol; missing evidence stays
    /// unavailable. An unknown symbol is a typed not-found refusal.
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
        /// Optional admitted evidence documents; rows without admitted
        /// evidence stay honestly `unknown`.
        #[arg(long, value_name = "PATH")]
        evidence: Vec<String>,
    },
}

fn invalid(detail: &'static str) -> DomainResult {
    tl::into_result(&tl::input_failure(detail))
}

/// Open one file no-follow and read at most [`tl::MAX_BYTES`] bytes.
///
/// A symlink or Windows reparse point at the operand path is refused
/// (`attachment-file`): the same no-follow discipline the project
/// filesystem applies to every project-owned path, applied to the
/// command's file operands so a planted link cannot redirect a read
/// outside the intended root.
fn read_bounded(path: &str) -> Result<Vec<u8>, DomainResult> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| invalid("attachment-unreadable"))?;
    if is_link(&metadata) || !metadata.is_file() {
        return Err(invalid("attachment-file"));
    }
    if metadata.len() as usize > tl::MAX_BYTES {
        return Err(invalid("attachment-bound"));
    }
    let file = std::fs::File::open(path).map_err(|_| invalid("attachment-unreadable"))?;
    let mut bytes = vec![];
    file.take(tl::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("attachment-unreadable"))?;
    if bytes.len() > tl::MAX_BYTES {
        return Err(invalid("attachment-bound"));
    }
    Ok(bytes)
}

/// Whether raw metadata identifies a symlink or Windows reparse point.
fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x0400 != 0
    }
    #[cfg(unix)]
    {
        metadata.file_type().is_symlink()
    }
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

/// The accepted owner-contract pin digests, hashed from the exact
/// checkout contract files. The working directory is the checkout root
/// for every documented invocation (all family gates run from it).
fn accepted_owner_pins() -> Vec<tl::OwnerPin> {
    let mut pins = Vec::new();
    for (family, version) in [
        ("authorization", "v0.2.16"),
        ("transaction-concurrency", "v0.2.16"),
        ("extended-effects", "v0.2.16"),
        ("invariant-transition", "v0.2.16"),
        ("storage-projection", "v0.4.0"),
    ] {
        let file_name = format!("contracts/{family}.schema.{version}.json");
        let Ok(bytes) = std::fs::read(&file_name) else {
            continue;
        };
        let identity_version = version.trim_start_matches('v');
        pins.push(tl::OwnerPin {
            family: family.to_owned(),
            identity: format!("dev.lekalo.{family}@{identity_version}"),
            digest: tl::wire_digest(&bytes),
        });
    }
    pins.sort();
    pins
}

/// Whether the environment asserts the qualified-native contract (the
/// qualified CI job's provisioning flag). Only then may `native`
/// evidence be admitted; a forged native claim elsewhere fails closed.
fn native_qualified() -> bool {
    std::env::var("LEKALO_TASKLINKS_NATIVE").as_deref() == Ok("1")
}

fn check_profile(profile: &str) -> Result<(), DomainResult> {
    if !tl::PROFILES.contains(&profile) {
        return Err(invalid("profile"));
    }
    Ok(())
}

fn profile_ref(path: &str) -> Result<tl::portability::ProfileRef, DomainResult> {
    let bytes = read_bounded(path)?;
    let digest = format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes));
    // The profile document is a closed single-member object: unknown
    // members, duplicate members, and shape drift fail closed before
    // any row is built.
    let value: serde_json::Value = tl::json_parse(&bytes).map_err(|_| invalid("profile-shape"))?;
    let object = value.as_object().ok_or_else(|| invalid("profile-shape"))?;
    if object.len() != 1 || !object.contains_key("profileId") {
        return Err(invalid("profile-shape"));
    }
    let profile_id = object
        .get("profileId")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("profile-shape"))?;
    if profile_id.is_empty() || profile_id.len() > 256 {
        return Err(invalid("profile-shape"));
    }
    Ok(tl::portability::ProfileRef {
        profile_id: profile_id.to_owned(),
        digest,
    })
}

/// Parse and fully verify one attachment against the live compilation:
/// closed parse, recomputed IR digest, recomputed attachment revision,
/// and verified owner-contract pins. Any divergence fails closed
/// before the attachment is used.
fn verified_attachment(
    path: &str,
    compilation: &lekalo_core::ir::Compilation,
) -> Result<tl::Attachment, DomainResult> {
    let bytes = read_bounded(path)?;
    let attachment = tl::parse(&bytes).map_err(|f| tl::into_result(&f))?;
    tl::verify_pins(&attachment, compilation, &accepted_owner_pins(), &bytes)
        .map_err(|f| tl::into_result(&f))?;
    Ok(attachment)
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
            let (compilation, _) = compilation(&project)?;
            let attachment = verified_attachment(&path, &compilation)?;
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
            let (compilation, project_id) = compilation(&project)?;
            let attachment = verified_attachment(&attachment_path, &compilation)?;
            if attachment.project_id() != project_id {
                return Err(invalid("project-mismatch"));
            }
            // Inspect joins the semantic validation: a malformed
            // attachment never renders as an authoritative view.
            let validation = tl::validate(&attachment, &compilation, "strict")
                .map_err(|failure| tl::into_result(&failure))?;
            if tl::invalidates(&validation) {
                return Ok(tl::render_invalid(&validation, &validation.findings));
            }
            let admitted = match &evidence {
                Some(path) => Some(
                    tl::admit_evidence(&read_bounded(path)?, &attachment, native_qualified())
                        .map_err(|f| tl::into_result(&f))?,
                ),
                None => None,
            };
            let view = tl::view(&attachment, admitted);
            // The operand is the contract: resolve the symbol against the
            // view's rows before anything renders. An unknown symbol is
            // one typed not-found refusal, never the whole view.
            if !tl::view_contains_symbol(&view, &symbol) {
                return Err(invalid("symbol-not-found"));
            }
            Ok(tl::render(&view))
        }
        Command::Portability {
            path,
            project,
            from_profile,
            to_profile,
            profile,
            evidence,
        } => {
            check_profile(&profile)?;
            let (compilation, project_id) = compilation(&project)?;
            let attachment = verified_attachment(&path, &compilation)?;
            if attachment.project_id() != project_id {
                return Err(invalid("project-mismatch"));
            }
            let from = profile_ref(&from_profile)?;
            let to = profile_ref(&to_profile)?;
            // Portability never bypasses semantic validation: the shared
            // admission runs first and an invalid attachment is a
            // refusal, not a report with gaps.
            let validation = tl::validate(&attachment, &compilation, "strict")
                .map_err(|failure| tl::into_result(&failure))?;
            if tl::invalidates(&validation) {
                return Ok(tl::render_invalid(&validation, &validation.findings));
            }
            // Admitted evidence joins the report; every --evidence path
            // goes through the same admission as inspect.
            let mut mock_evidence = Vec::new();
            let mut native_evidence = Vec::new();
            for evidence_path in &evidence {
                let summary = tl::admit_evidence(
                    &read_bounded(evidence_path)?,
                    &attachment,
                    native_qualified(),
                )
                .map_err(|f| tl::into_result(&f))?;
                if summary.origin == "native" {
                    native_evidence.push(summary.evidence_id);
                } else {
                    mock_evidence.push(summary.evidence_id);
                }
            }
            let report = tl::portability::report(
                &attachment,
                from,
                to,
                &profile,
                mock_evidence,
                native_evidence,
            );
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
