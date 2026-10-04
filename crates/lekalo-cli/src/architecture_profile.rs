//! Read-only architecture policy selection. No source, lock, or baseline writes.
use clap::{Args, Subcommand};
use lekalo_core::{architecture_profile as ap, DomainResult};
use std::io::Read;

#[derive(Debug, Args)]
pub struct DocumentArgs {
    /// Closed profile collection; omission selects the embedded collection.
    #[arg(long)]
    architecture_profiles: Option<String>,
}
#[derive(Debug, Args)]
pub struct ResolveArgs {
    #[command(flatten)]
    document: DocumentArgs,
    #[arg(long)]
    architecture_profile: String,
    #[arg(long, default_value = "1")]
    profile_version: String,
}
#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("architecture-scope").required(true).multiple(false).args(["module","all"])))]
pub struct AssessArgs {
    #[command(flatten)]
    document: DocumentArgs,
    /// Explicit selection, or the exact project assignment; otherwise legacy-observed.
    #[arg(long)]
    architecture_profile: Option<String>,
    #[arg(long, default_value = "1")]
    profile_version: String,
    #[arg(long)]
    module: Option<String>,
    #[arg(long)]
    all: bool,
    #[arg(long)]
    project: Option<String>,
    /// Verify the dedicated architecture sidecar against the current document.
    #[arg(long)]
    architecture_lock: Option<String>,
    /// Immutable prior architecture report; never created or overwritten here.
    #[arg(long)]
    baseline: Option<String>,
    #[arg(long, requires_all = ["baseline", "as_of"])]
    adoption: Option<String>,
    #[arg(long, requires = "adoption")]
    as_of: Option<String>,
    /// Deny unmet selected evidence obligations and calibrated measured limits.
    #[arg(long)]
    check: bool,
}
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Emit the versioned language-neutral rule catalog and its coverage boundaries.
    Catalog,
    /// Resolve an exact named profile, parent chain and effective rule digest.
    Resolve(ResolveArgs),
    /// Emit a dedicated architecture lock sidecar for explicit caller persistence.
    Lock(DocumentArgs),
    /// Assess selected modules; source remains read-only and advisory without --check.
    Assess(AssessArgs),
    /// Compare two admitted profile documents, independently of Model changes.
    Diff {
        #[arg(long)]
        base_profiles: String,
        #[arg(long)]
        candidate_profiles: String,
    },
}
fn invalid(detail: &str) -> DomainResult {
    ap::failure("architecture-profile.input-invalid", detail)
}
fn read(path: &str) -> Result<Vec<u8>, DomainResult> {
    let file = std::fs::File::open(path).map_err(|_| invalid("input-read"))?;
    if !file
        .metadata()
        .map_err(|_| invalid("input-metadata"))?
        .is_file()
    {
        return Err(invalid("input-file"));
    }
    let mut bytes = vec![];
    file.take(ap::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("input-read"))?;
    if bytes.len() > ap::MAX_BYTES {
        return Err(invalid("input-bound"));
    }
    Ok(bytes)
}
fn document(args: DocumentArgs) -> Result<ap::Document, DomainResult> {
    args.architecture_profiles
        .map_or_else(|| Ok(ap::embedded()), |p| ap::parse(&read(&p)?))
}
fn version(value: &str) -> Result<(), DomainResult> {
    if value != "1" {
        return Err(ap::failure(
            "architecture-profile.version-unsupported",
            "profile-version",
        ));
    }
    Ok(())
}
pub fn run(command: Command) -> DomainResult {
    execute(command).unwrap_or_else(|r| r)
}
fn execute(command: Command) -> Result<DomainResult, DomainResult> {
    match command {
        Command::Catalog => Ok(ap::render(&ap::catalog(), false)),
        Command::Resolve(args) => {
            version(&args.profile_version)?;
            Ok(ap::render(
                &ap::resolve(&document(args.document)?, &args.architecture_profile)?,
                false,
            ))
        }
        Command::Lock(args) => Ok(ap::render(&ap::snapshot(&document(args)?)?, false)),
        Command::Diff {
            base_profiles,
            candidate_profiles,
        } => Ok(ap::render(
            &ap::policy_diff(
                &ap::parse(&read(&base_profiles)?)?,
                &ap::parse(&read(&candidate_profiles)?)?,
            )?,
            false,
        )),
        Command::Assess(args) => {
            version(&args.profile_version)?;
            let d = document(args.document)?;
            if let Some(path) = args.architecture_lock {
                ap::verify_lock(&read(&path)?, &d)?;
            }
            let baseline = args
                .baseline
                .map(|p| ap::decode::<ap::Report>(&read(&p)?, "architecture-profile-report"))
                .transpose()?;
            let adoption = args
                .adoption
                .map(|p| ap::decode::<ap::Adoption>(&read(&p)?, "architecture-adoption"))
                .transpose()?;
            let model =
                lekalo_core::loader::normalize_model(&lekalo_core::loader::LoadSelection {
                    project: args
                        .project
                        .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
                })?;
            let compilation = lekalo_core::ir::compile(&model).map_err(|f| f.into_result())?;
            let project = compilation
                .project
                .project
                .as_ref()
                .map(|p| p.id.as_str())
                .unwrap_or("anonymous");
            let selected = args
                .architecture_profile
                .or_else(|| {
                    d.assignments
                        .iter()
                        .find(|x| x.scope.kind == "project" && x.scope.id == project)
                        .map(|x| x.profile_ref.id.clone())
                })
                .unwrap_or_else(|| "legacy-observed".into());
            let report = ap::assess(ap::Assessment {
                document: &d,
                profile: &selected,
                compilation: &compilation,
                model_ref: ap::digest(
                    lekalo_core::loader::canonical_model_bytes(&model).as_bytes(),
                ),
                module: args.module,
                check: args.check,
                baseline: baseline.as_ref(),
                adoption: adoption.as_ref(),
                as_of: args.as_of.as_deref(),
            })?;
            Ok(ap::render_report(&report))
        }
    }
}
