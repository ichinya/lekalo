//! Bounded read-only CLI adapter for issue #19; every decision belongs
//! to the core.
//!
//! This module owns the classified reads (byte-bounded, duplicate-key
//! rejecting, safe project-relative selectors), the exact profile-pin
//! verification against the resolved profile documents, the qualified
//! implementation join through the existing producer, and the human/JSON
//! rendering. The core [`lekalo_core::portability`] performs the whole
//! assessment over typed inputs and launches nothing.

use clap::Args as ClapArgs;
use lekalo_core::diagnostics::DiagnosticSet;
use lekalo_core::portability::{
    self as core, assess, embedded_recipe, extract, parse_evidence, parse_targets, profile_pin,
    resolve_targets, verify_profile_pin, Request, Scope,
};
use lekalo_core::{DomainResult, Status};
use std::collections::BTreeMap;
use std::io::Read;

/// The `lekalo portability` flag surface (issue #19).
#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Project root selector, relative to the invocation directory.
    #[arg(long, value_name = "DIR")]
    pub project: Option<String>,
    /// The source target identifier.
    #[arg(long, value_name = "TARGET")]
    pub from: Option<String>,
    /// One destination target identifier; repeatable.
    #[arg(long = "to", value_name = "TARGET")]
    pub to: Vec<String>,
    /// The closed target-binding configuration (default
    /// `config/portability-targets.json`).
    #[arg(long, value_name = "PATH")]
    pub targets: Option<String>,
    /// Qualified recorded evidence manifest.
    #[arg(long, value_name = "PATH")]
    pub evidence: Option<String>,
    /// Scope the assessment to one symbol.
    #[arg(long, value_name = "ID", conflicts_with = "module")]
    pub symbol: Option<String>,
    /// Scope the assessment to one module.
    #[arg(long, value_name = "ID")]
    pub module: Option<String>,
    /// Strict gate: unresolved required obligations deny.
    #[arg(long)]
    pub check: bool,
}

/// The default project-relative configuration home, outside the
/// reserved `lekalo/**` trees.
const DEFAULT_TARGETS: &str = "config/portability-targets.json";
/// The lock custody file of the project, when present.
const LOCK_PATH: &str = "lekalo/lock.json";

fn invalid(detail: &'static str) -> DiagnosticSet {
    lekalo_core::portability::diagnostic::invalid(detail)
}

/// One bounded, classified read of a project-relative file: safe-path
/// grammar, regular file, byte limit, no follow.
fn read_bounded(root: &std::path::Path, relative: &str) -> Result<Vec<u8>, DiagnosticSet> {
    if lekalo_core::project_fs::path_violation(relative).is_some() {
        return Err(invalid("artifact-ref"));
    }
    let path = root.join(relative);
    let file = std::fs::File::open(&path).map_err(|_| invalid("input-read"))?;
    if !file
        .metadata()
        .map_err(|_| invalid("input-metadata"))?
        .is_file()
    {
        return Err(invalid("input-file"));
    }
    let mut bytes = vec![];
    file.take(lekalo_core::portability::version::MAX_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("input-read"))?;
    if bytes.len() > lekalo_core::portability::version::MAX_INPUT_BYTES {
        return Err(invalid("input-size"));
    }
    Ok(bytes)
}

fn failure(set: DiagnosticSet) -> DomainResult {
    if set
        .as_slice()
        .iter()
        .any(|d| d.id() == "portability.version-unsupported")
    {
        DomainResult::unsupported_version(set)
    } else {
        DomainResult::invalid(set)
    }
}

pub fn run(args: Args) -> DomainResult {
    match execute(args) {
        Ok(result) => result,
        Err(set) => failure(set),
    }
}

/// Decode one profile document and verify the configured pin exactly.
fn load_profile(
    root: &std::path::Path,
    binding: &lekalo_core::portability::wire::TargetBinding,
) -> Result<Option<lekalo_core::portability::ProfilePin>, DiagnosticSet> {
    let bytes = read_bounded(root, &binding.profile_document_ref)?;
    let document = lekalo_core::target_profile::document::decode(&bytes)
        .map_err(|_| lekalo_core::portability::diagnostic::invalid("profile-document"))?;
    let resolved = lekalo_core::target_profile::resolution::resolve(&document)
        .map_err(|_| lekalo_core::portability::diagnostic::invalid("profile-resolution"))?;
    let found = resolved
        .iter()
        .find(|profile| profile.id == binding.profile_ref.id)
        .ok_or_else(|| invalid("profile-pin"))?;
    verify_profile_pin(&binding.profile_ref, found)?;
    Ok(Some(profile_pin(found)))
}

/// Join the qualified implementation projection: the evidence manifest
/// must declare the family complete, pin the artifact digest, and the
/// producer must validate the document against this exact compilation.
fn load_implementation(
    root: &std::path::Path,
    evidence: &lekalo_core::portability::EvidenceDocument,
    compilation: &lekalo_core::ir::Compilation,
) -> Result<Option<lekalo_core::implementation::PortabilityReport>, DiagnosticSet> {
    let family = evidence
        .families
        .iter()
        .find(|f| f.family == lekalo_core::portability::wire::EvidenceFamily::Implementation);
    let Some(family) = family else {
        return Ok(None);
    };
    if !family.complete {
        return Ok(None);
    }
    let (Some(reference), Some(digest)) = (&family.artifact_ref, &family.digest) else {
        return Ok(None);
    };
    let bytes = read_bounded(root, reference)?;
    if format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)) != *digest {
        return Err(invalid("implementation-digest"));
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| invalid("implementation-shape"))?;
    let document = lekalo_core::implementation::ImplementationDocument::from_value(&value)
        .map_err(|_| invalid("implementation-shape"))?;
    match lekalo_core::implementation::validate(&document, compilation) {
        Ok(report) => Ok(Some(report.portability().clone())),
        Err(set) => Err(set),
    }
}

fn execute(args: Args) -> Result<DomainResult, DiagnosticSet> {
    let selection = lekalo_core::loader::LoadSelection {
        project: args
            .project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let model = match lekalo_core::loader::normalize_model(&selection) {
        Ok(model) => model,
        Err(result) => return Ok(result),
    };
    let compilation = match lekalo_core::ir::compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => return Ok(failure.into_result()),
    };
    let root =
        std::path::PathBuf::from(selection.project.clone().unwrap_or_else(|| ".".to_owned()));
    let recipe = embedded_recipe()?;

    // Targets configuration: the explicit selector or the default home;
    // an absent default derives the targets from the Model declarations.
    let targets_path = args
        .targets
        .clone()
        .unwrap_or_else(|| DEFAULT_TARGETS.to_owned());
    let targets_bytes = read_bounded(&root, &targets_path);
    let (targets, targets_input) = match targets_bytes {
        Ok(bytes) => {
            let document = parse_targets(&bytes)?;
            let digest = format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes));
            (
                Some(document),
                lekalo_core::portability::wire::TargetsInput {
                    state: lekalo_core::portability::wire::PinState::Pinned,
                    path: Some(targets_path.clone()),
                    digest: Some(digest),
                },
            )
        }
        Err(set) => {
            if args.targets.is_some() {
                return Err(set);
            }
            (
                None,
                lekalo_core::portability::wire::TargetsInput {
                    state: lekalo_core::portability::wire::PinState::Derived,
                    path: None,
                    digest: None,
                },
            )
        }
    };

    // Evidence: optional, pinned, and recipe/project verified.
    let mut evidence_digest: Option<String> = None;
    let evidence = match &args.evidence {
        Some(path) => {
            let bytes = read_bounded(&root, path)?;
            evidence_digest = Some(format!(
                "sha256:{}",
                lekalo_core::digest::sha256_hex(&bytes)
            ));
            let document = parse_evidence(&bytes, &compilation, &recipe)?;
            Some(document)
        }
        None => None,
    };

    // The request scope and selection.
    let request = Request {
        scope: match (&args.symbol, &args.module) {
            (Some(symbol), _) => Scope::Symbol(symbol.clone()),
            (None, Some(module)) => Scope::Module(module.clone()),
            (None, None) => Scope::Project,
        },
        source: args.from.clone(),
        destinations: args.to.clone(),
    };
    request.validate()?;

    // Profile pins and adapter snapshots per configured target.
    let subjects = extract(&compilation.project, &recipe);
    let mut pins: BTreeMap<String, Option<lekalo_core::portability::ProfilePin>> = BTreeMap::new();
    let mut adapters: BTreeMap<String, Option<lekalo_core::portability::wire::AdapterSnapshot>> =
        BTreeMap::new();
    let mut storage_refs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(document) = &targets {
        for binding in &document.targets {
            pins.insert(binding.target_id.clone(), load_profile(&root, binding)?);
            storage_refs.insert(
                binding.target_id.clone(),
                binding.storage_profile_refs.clone(),
            );
        }
    }
    if let Some(evidence) = &evidence {
        for adapter in &evidence.adapters {
            adapters.insert(
                adapter.target_id.clone(),
                Some(lekalo_core::portability::wire::AdapterSnapshot {
                    state: adapter.state,
                    identity: adapter.identity.clone(),
                    version: adapter.version.clone(),
                    protocol: adapter.protocol.clone(),
                    capability_digest: adapter.capability_digest.clone(),
                }),
            );
        }
    }
    let resolved = resolve_targets(
        &request,
        targets.as_ref(),
        &subjects,
        &pins,
        &adapters,
        &storage_refs,
    )?;

    // The lock custody pin, recorded separately from the evidence trust.
    let lock_digest = read_bounded(&root, LOCK_PATH)
        .ok()
        .map(|bytes| format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)));

    // The qualified implementation join.
    let implementation = match &evidence {
        Some(document) => load_implementation(&root, document, &compilation)?,
        None => None,
    };

    let assessment = core::Assessment {
        compilation: &compilation,
        recipe: &recipe,
        subjects,
        request: &request,
        source: resolved.source.clone(),
        destinations: resolved.destinations.clone(),
        evidence: evidence.as_ref(),
        implementation: implementation.as_ref(),
        targets_input,
        lock_digest,
        evidence_digest,
    };
    let report = assess(&assessment)?;

    // The envelope diagnostics: one per distinct triggered rule, in the
    // registry chain, carrying bounded tokens only.
    let mut diagnostics: Vec<lekalo_core::diagnostics::Diagnostic> = Vec::new();
    let mut codes: Vec<&str> = Vec::new();
    for finding in &report.diagnostics {
        if !codes.contains(&finding.code.as_str()) {
            codes.push(match finding.code.as_str() {
                "portability.evidence-incomplete" => "portability.evidence-incomplete",
                "portability.mapping-partial" => "portability.mapping-partial",
                "portability.capability-unsupported" => "portability.capability-unsupported",
                "portability.implementation-required" => "portability.implementation-required",
                "portability.behavior-loss" => "portability.behavior-loss",
                "portability.analysis-incomplete" => "portability.analysis-incomplete",
                _ => continue,
            });
        }
    }
    for code in codes {
        diagnostics.push(lekalo_core::portability::diagnostic::diagnostic(
            code,
            "portability",
            "finding",
        )?);
    }

    let human = human_summary(&report);
    let json = format!(
        "{{\"status\":\"valid\",\"portability\":{}}}",
        lekalo_core::portability::canonical::canonical_bytes(&report)
    );

    // The strict gate: unresolved required obligations deny while the
    // report stays visible in the denied envelope.
    if args.check && core::planning::denial(&report) {
        let mut denied = diagnostics;
        denied.push(lekalo_core::portability::diagnostic::diagnostic(
            "portability.check-denied",
            "portability",
            "unresolved-required-obligations",
        )?);
        let set = DiagnosticSet::try_from_unsorted(denied, Status::Denied)
            .map_err(|_| invalid("diagnostic-bound"))?;
        return Ok(DomainResult::denied_json(json, human, set));
    }

    let set = DiagnosticSet::try_from_unsorted(diagnostics, Status::Valid)
        .map_err(|_| invalid("diagnostic-bound"))?;
    Ok(DomainResult::validation(
        json,
        human,
        set.as_slice().to_vec(),
    ))
}

/// The human projection: the four risk summaries before anything else,
/// then blockers, partial mappings, custom/foreign/external obligations,
/// behavior loss, and unknown verification.
fn human_summary(report: &lekalo_core::portability::Report) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Portability: {} subjects, {} destinations, source {}.",
        report.coverage.eligible_subjects,
        report.destinations.len(),
        report.source.target_id
    ));
    for summary in &report.summaries {
        let structural = summary
            .dimensions
            .get(&lekalo_core::portability::Dimension::Structural)
            .expect("structural summary");
        let ratio = match structural.structural_portable {
            Some(ratio) => format!("{}/{}", ratio.numerator, ratio.denominator),
            None => "unknown".to_owned(),
        };
        lines.push(format!(
            "{}: structural portable {}, mapped {}, partial {}, target-specific {}, unsupported {}, unknown {} (ratio {})",
            summary.destination,
            structural.portable,
            structural.supported_with_mapping,
            structural.partial,
            structural.target_specific,
            structural.unsupported,
            structural.unknown,
            ratio,
        ));
        lines.push(format!(
            "{}: blockers {}, unknowns {}, custom obligations {}",
            summary.destination, summary.blockers, summary.unknowns, summary.custom_obligations
        ));
    }
    let mut blockers = Vec::new();
    let mut partials = Vec::new();
    let mut customs = Vec::new();
    let mut losses = Vec::new();
    let mut unknowns = Vec::new();
    for subject in &report.subjects {
        for row in &subject.rows {
            if row.blocking {
                blockers.push(format!(
                    "{} -> {}: {} ({})",
                    subject.subject,
                    row.destination,
                    row.dimension.as_str(),
                    row.reason
                ));
            }
            if row.classification == lekalo_core::portability::Classification::Partial {
                partials.push(format!(
                    "{} -> {}: {}",
                    subject.subject, row.destination, row.reason
                ));
            }
            if matches!(
                row.binding,
                Some(
                    lekalo_core::portability::Binding::Custom
                        | lekalo_core::portability::Binding::Foreign
                        | lekalo_core::portability::Binding::External
                )
            ) {
                customs.push(format!(
                    "{} -> {}: {}",
                    subject.subject,
                    row.destination,
                    row.binding.map(|b| b.as_str()).unwrap_or_default()
                ));
            }
            if row.reason == "storage-policy-loss" || row.reason == "implicit-hook-unsupported" {
                losses.push(format!(
                    "{} -> {}: {}",
                    subject.subject, row.destination, row.reason
                ));
            }
            if row.verification == lekalo_core::portability::Verification::Untested
                && row.classification != lekalo_core::portability::Classification::NonApplicable
            {
                unknowns.push(format!(
                    "{} -> {}: untested",
                    subject.subject, row.destination
                ));
            }
        }
    }
    if !blockers.is_empty() {
        lines.push(format!("Blockers ({}):", blockers.len()));
        lines.extend(blockers.iter().take(16).map(|l| format!("  {l}")));
    }
    if !partials.is_empty() {
        lines.push(format!("Partial mappings ({}):", partials.len()));
        lines.extend(partials.iter().take(16).map(|l| format!("  {l}")));
    }
    if !customs.is_empty() {
        lines.push(format!(
            "Custom/foreign/external obligations ({}):",
            customs.len()
        ));
        lines.extend(customs.iter().take(16).map(|l| format!("  {l}")));
    }
    if !losses.is_empty() {
        lines.push(format!("Behavior loss ({}):", losses.len()));
        lines.extend(losses.iter().take(16).map(|l| format!("  {l}")));
    }
    if !unknowns.is_empty() {
        lines.push(format!(
            "Untested verification states ({} shown of {}):",
            unknowns.iter().take(16).count(),
            unknowns.len()
        ));
        lines.extend(unknowns.iter().take(16).map(|l| format!("  {l}")));
    }
    lines.push(format!(
        "Migration surface: {} missing implementations, {} storage reviews, {} affected scenarios, {} affected gates, planning rows {}.",
        report.migration_surface.missing_implementations,
        report.migration_surface.storage_reviews,
        report.migration_surface.affected_scenarios,
        report.migration_surface.affected_gates,
        report.migration_surface.rows.len(),
    ));
    lines.join("\n")
}
