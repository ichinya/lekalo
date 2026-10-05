//! Bounded read-only CLI adapter for issue #77; decisions belong to core.
use clap::Args;
use lekalo_core::{coupling, DomainResult};
use std::io::Read;

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("coupling-selector").required(true).multiple(false).args(["symbol","module","all","changed_input"])))]
pub struct CouplingArgs {
    #[arg(long)]
    symbol: Option<String>,
    #[arg(long)]
    module: Option<String>,
    #[arg(long)]
    all: bool,
    #[arg(long)]
    changed_input: Option<String>,
    #[arg(long, requires = "symbol")]
    field: Option<String>,
    #[arg(long, requires = "profiles")]
    coupling_profile: Option<String>,
    #[arg(long, requires = "coupling_profile")]
    profiles: Option<String>,
    #[arg(long)]
    baseline: Option<String>,
    #[arg(long)]
    evidence: Option<String>,
    /// Include the existing forward context plan with this explicit token budget.
    #[arg(long)]
    context_budget: Option<u64>,
    /// Caller-resolved current revision for native trace freshness (40/64 hex).
    #[arg(long)]
    source_revision: Option<String>,
    #[arg(long)]
    project: Option<String>,
}
fn read(path: &str) -> Result<Vec<u8>, lekalo_core::diagnostics::DiagnosticSet> {
    let file =
        std::fs::File::open(path).map_err(|_| coupling::diagnostic::invalid("input-read"))?;
    if !file
        .metadata()
        .map_err(|_| coupling::diagnostic::invalid("input-metadata"))?
        .is_file()
    {
        return Err(coupling::diagnostic::invalid("input-file"));
    }
    let mut bytes = vec![];
    file.take(coupling::wire::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| coupling::diagnostic::invalid("input-read"))?;
    if bytes.len() > coupling::wire::MAX_BYTES {
        return Err(coupling::diagnostic::invalid("input-size"));
    }
    Ok(bytes)
}
fn failure(set: lekalo_core::diagnostics::DiagnosticSet) -> DomainResult {
    if set
        .as_slice()
        .iter()
        .any(|d| d.id() == "coupling.profile-unsupported")
    {
        DomainResult::unsupported_version(set)
    } else {
        DomainResult::invalid(set)
    }
}
fn metric_text(value: &coupling::wire::State<u64>) -> String {
    use coupling::wire::State;
    match value {
        State::Known { value } => value.to_string(),
        State::Unknown => "unknown".into(),
        State::Withheld => "withheld".into(),
        State::Unsupported => "unsupported".into(),
    }
}
pub fn run(args: CouplingArgs) -> DomainResult {
    match execute(args) {
        Ok(result) => result,
        Err(set) => failure(set),
    }
}
fn execute(args: CouplingArgs) -> Result<DomainResult, lekalo_core::diagnostics::DiagnosticSet> {
    let profile = if let Some(path) = &args.profiles {
        let p = coupling::Profile::parse(&read(path)?)?;
        if Some(&p.profile_id) != args.coupling_profile.as_ref() {
            return Err(coupling::diagnostic::invalid("profile-id"));
        }
        p
    } else {
        coupling::Profile::default()
    };
    let selection = if let Some(symbol) = args.symbol {
        coupling::Selection::Symbol(symbol)
    } else if let Some(module) = args.module {
        coupling::Selection::Module(module)
    } else if let Some(path) = args.changed_input {
        coupling::Selection::Changed(coupling::ChangeInput::parse(&read(&path)?)?)
    } else if args.all {
        coupling::Selection::All
    } else {
        return Err(coupling::diagnostic::invalid("selector"));
    };
    let evidence = args
        .evidence
        .as_ref()
        .map(|p| read(p).and_then(|b| coupling::Evidence::parse(&b)))
        .transpose()?;
    let baseline = args.baseline.as_ref().map(|p| read(p)).transpose()?;
    let selection_loader = lekalo_core::loader::LoadSelection {
        project: args
            .project
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let model = match lekalo_core::loader::normalize_model(&selection_loader) {
        Ok(model) => model,
        Err(result) => return Ok(result),
    };
    let compilation = match lekalo_core::ir::compile(&model) {
        Ok(c) => c,
        Err(f) => return Ok(f.into_result()),
    };
    let request = coupling::Request {
        selection,
        field: args.field,
        context_budget: args.context_budget,
        source_revision: args.source_revision,
        model_digest: Some(format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(
                lekalo_core::loader::canonical_model_bytes(&model).as_bytes()
            )
        )),
    };
    let report = coupling::analyze(&request, &profile, &compilation, evidence.as_ref())?;
    let comparison = baseline
        .as_ref()
        .map(|b| {
            coupling::compare::parse_baseline(b)
                .and_then(|base| coupling::compare::compare(&base, &report, b))
        })
        .transpose()?;
    let denial = coupling::compare::denial(&profile, &report, comparison.as_ref());
    let mut diagnostics = report
        .findings
        .iter()
        .map(|f| coupling::diagnostic::diagnostic(&f.rule, &f.subject, &f.detail))
        .collect::<Result<Vec<_>, _>>()?;
    if !report.complete
        && !report
            .findings
            .iter()
            .any(|f| f.rule == "coupling.evidence-incomplete")
    {
        diagnostics.push(coupling::diagnostic::diagnostic(
            "coupling.evidence-incomplete",
            "coupling",
            "required-evidence-incomplete",
        )?);
    }
    if let Some(c) = &comparison {
        if !c.comparable || c.rows.iter().any(|r| r.state != "comparable") {
            diagnostics.push(coupling::diagnostic::diagnostic(
                "coupling.baseline-incomparable",
                "coupling",
                "unavailable-or-different-measurement",
            )?);
        }
        if c.regressions > 0 {
            diagnostics.push(coupling::diagnostic::diagnostic(
                "coupling.baseline-regression",
                "coupling",
                "configured-regression-allowance",
            )?);
        }
    }
    // Every diagnostic passes the shared canonical normalization and bounded-set
    // seam. Excess findings deny an input bound rather than disappearing.
    let diagnostic_set = lekalo_core::diagnostics::DiagnosticSet::try_from_unsorted(
        diagnostics,
        lekalo_core::result::Status::Valid,
    )
    .map_err(|_| coupling::diagnostic::invalid("diagnostic-bound"))?;
    let mut envelope = serde_json::json!({"status":"valid","coupling":report});
    if let Some(c) = comparison {
        envelope["couplingComparison"] = serde_json::to_value(c).expect("typed comparison");
    }
    let human = format!(
        "Coupling: {} subjects, {} public contracts, {} internal symbols.\nAssessment: {}.\n{}",
        report.subjects.len(),
        report.summary.public_contracts.len(),
        report.summary.internal_symbols.len(),
        profile.gate.mode,
        report
            .subjects
            .iter()
            .map(|s| format!(
                "{}: fan-in {}, fan-out {}, public radius {}",
                s.subject,
                metric_text(&s.metrics["fanInSymbols"]),
                metric_text(&s.metrics["fanOutSymbols"]),
                metric_text(&s.metrics["publicContractsAffected"])
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if denial.is_empty() {
        Ok(DomainResult::validation(
            envelope.to_string(),
            human,
            diagnostic_set.as_slice().to_vec(),
        ))
    } else {
        // Denial keeps all advisory findings in the evidence report. The terminal
        // registry set contains the status-compatible error, following #75.
        let detail = denial.join(",");
        let denied = coupling::diagnostic::set(
            "coupling.policy-denied",
            lekalo_core::result::Status::Denied,
            &detail,
        );
        Ok(DomainResult::denied_json(
            envelope.to_string(),
            human,
            denied,
        ))
    }
}
