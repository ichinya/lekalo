//! CLI syntax and immutable input loading. Core owns admission and decisions.
use clap::Args;
use lekalo_core::{
    ai_lint::{self as lint, input, Config, Evidence},
    DomainResult,
};
use std::io::Read;
#[derive(Debug, Args)]
pub(crate) struct AiLintArgs {
    #[arg(long)]
    pub symbol: Option<String>,
    #[arg(long)]
    pub module: Option<String>,
    #[arg(long)]
    pub all: bool,
    #[arg(long, default_value = "advisory")]
    pub lint_profile: String,
    #[arg(long)]
    pub config: Option<String>,
    #[arg(long)]
    pub evidence: Vec<String>,
    #[arg(long)]
    pub scan_target: Vec<String>,
    /// Explicit source inputs for the installed, read-only target collector.
    #[arg(long, requires = "scan_target")]
    pub source_file: Vec<String>,
    #[arg(long)]
    pub waivers: Option<String>,
    #[arg(long)]
    pub as_of: Option<String>,
    #[arg(long)]
    pub baseline: Option<String>,
    #[arg(long)]
    pub transitions: Option<String>,
    #[arg(long)]
    pub trace: Option<String>,
    #[arg(long)]
    pub artifacts: Option<String>,
    #[arg(long)]
    pub check: bool,
    #[arg(long)]
    pub spans: bool,
    #[arg(long)]
    pub project: Option<String>,
}
fn read(path: &str) -> Result<Vec<u8>, DomainResult> {
    let file = std::fs::File::open(path)
        .map_err(|_| lint::diagnostic::failure("ai-lint.input-invalid", "input-read"))?;
    let mut bytes = Vec::new();
    file.take(input::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| lint::diagnostic::failure("ai-lint.input-invalid", "input-read"))?;
    if bytes.len() > input::MAX_BYTES {
        return Err(lint::diagnostic::failure(
            "ai-lint.input-invalid",
            "input-limit",
        ));
    }
    Ok(bytes)
}
pub(crate) fn run(args: AiLintArgs) -> DomainResult {
    match execute(args) {
        Ok(r) | Err(r) => r,
    }
}
fn execute(args: AiLintArgs) -> Result<DomainResult, DomainResult> {
    if usize::from(args.symbol.is_some())
        + usize::from(args.module.is_some())
        + usize::from(args.all)
        != 1
        || args.as_of.as_deref().is_some_and(|s| !input::date(s))
        || args.evidence.len().saturating_add(args.scan_target.len()) > 64
    {
        return Err(DomainResult::usage_error());
    }
    let selection = lekalo_core::loader::LoadSelection {
        project: args
            .project
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let ctx = lekalo_core::observed::context(&selection)?;
    let compilation = lekalo_core::ir::compile(&ctx.model).map_err(|f| f.into_result())?;
    let semantic_profile = lekalo_core::validator::ValidationProfile::embedded_default()
        .map_err(|_| lint::diagnostic::failure("ai-lint.input-invalid", "validation-profile"))?;
    lekalo_core::validator::validate(&compilation, semantic_profile)
        .map_err(DomainResult::invalid)?;
    let scope = if let Some(symbol) = args.symbol {
        vec![lint::resolve_symbol(&compilation, &symbol)?]
    } else if let Some(module) = args.module {
        if !compilation
            .project
            .modules
            .iter()
            .any(|m| m.id.as_str() == module)
        {
            return Err(lint::diagnostic::failure(
                "ai-lint.input-invalid",
                "module-unknown",
            ));
        }
        vec![module]
    } else {
        compilation
            .project
            .definitions
            .iter()
            .map(|d| d.id().as_str().to_owned())
            .collect()
    };
    let config: Config = if let Some(path) = args.config {
        input::parse_config(&read(&path)?)?
    } else {
        lint::default_config()
    };
    let model_ref =
        input::digest(lekalo_core::loader::canonical_model_bytes(&ctx.model).as_bytes());
    let artifact_model_ref = match lekalo_core::loader::run(&selection, false) {
        DomainResult::Valid {
            payload: lekalo_core::result::SuccessPayload::Model { json, .. },
            ..
        } => input::digest(json.as_bytes()),
        refusal => return Err(refusal),
    };
    let ir_ref = input::digest(compilation.project.to_canonical_json().as_bytes());
    let fs = lekalo_core::project_fs::Fs::open(&ctx.root)
        .map_err(|_| lint::diagnostic::failure("ai-lint.input-invalid", "project-root"))?;
    let observed = lekalo_core::observed::load_index(&ctx).map_err(DomainResult::invalid)?;
    let mut evidence = Vec::<Evidence>::new();
    for path in &args.evidence {
        evidence.push(input::parse(&read(path)?)?);
    }
    for target in &args.scan_target {
        if evidence.iter().any(|e| &e.target == target) {
            return Err(lint::diagnostic::failure(
                "ai-lint.input-invalid",
                "duplicate-target",
            ));
        }
        let request = lint::collect::request(
            observed.as_ref(),
            &fs,
            &model_ref,
            &ir_ref,
            scope.clone(),
            args.source_file.clone(),
        )?;
        evidence.push(lint::collect::collect(&ctx.root, target, request)?);
    }
    evidence.sort_by(|a, b| a.target.cmp(&b.target));
    let waivers = if let Some(path) = &args.waivers {
        Some(input::parse_waivers(
            &read(path)?,
            &input::hash(&config),
            args.as_of.as_deref(),
        )?)
    } else {
        None
    };
    let baseline = if let Some(path) = &args.baseline {
        Some(lint::parse_report(&read(path)?)?)
    } else {
        None
    };
    let transitions = if let Some(path) = &args.transitions {
        let value = serde_json::from_slice(&read(path)?)
            .map_err(|_| lint::diagnostic::failure("ai-lint.input-invalid", "transitions-json"))?;
        Some(
            lekalo_core::invariant_transition::InvariantTransitionAttachment::from_value(&value)
                .map_err(DomainResult::invalid)?,
        )
    } else {
        None
    };
    let trace = if let Some(path) = &args.trace {
        Some(
            lekalo_core::trace::TraceManifest::parse(&read(path)?)
                .map_err(DomainResult::invalid)?,
        )
    } else {
        None
    };
    let artifacts = args
        .artifacts
        .as_ref()
        .map(|path| {
            read(path).and_then(|bytes| {
                lekalo_core::artifacts::ArtifactManifest::parse_canonical(&bytes).map_err(|_| {
                    lint::diagnostic::failure("ai-lint.input-invalid", "artifact-manifest")
                })
            })
        })
        .transpose()?;
    let report = lint::analyze(&lint::Request {
        compilation: &compilation,
        model_ref: &model_ref,
        artifact_model_ref: &artifact_model_ref,
        scope,
        config: &config,
        profile: &args.lint_profile,
        evidence: &evidence,
        waivers: waivers.as_ref(),
        as_of: args.as_of.as_deref(),
        baseline: baseline.as_ref(),
        check: args.check,
        transitions: transitions.as_ref(),
        trace: trace.as_ref(),
        artifacts: artifacts.as_ref(),
        observed: observed.as_ref(),
        fs: &fs,
    })?;
    let mut result = lint::render(&report, &config, args.check);
    if args.spans {
        let sources=evidence.iter().map(|e|serde_json::json!({"evidenceRef":input::hash(e),"sources":e.sources,"locations":e.locations})).collect::<Vec<_>>();
        let projection =
            serde_json::json!({"targets":sources,"model":compilation.source_map.entries()});
        let payload = match &mut result {
            DomainResult::Valid {
                payload: lekalo_core::result::SuccessPayload::Graph { json, .. },
                ..
            }
            | DomainResult::DeniedWithEvidence { json, .. } => json,
            _ => return Ok(result),
        };
        let mut value: serde_json::Value = serde_json::from_str(payload).expect("lint payload");
        value["sourceLocations"] = projection;
        *payload = value.to_string();
    }
    Ok(result)
}
