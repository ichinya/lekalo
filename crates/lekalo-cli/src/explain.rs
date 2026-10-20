//! `lekalo explain`: read-only, offline explanation of stable diagnostic
//! codes; the core owns the versioned catalog and its closed lookups.

use clap::{Args, ValueEnum};
use lekalo_core::{explain, DomainResult};
use std::io::Read;

#[derive(Debug, Args)]
pub(crate) struct ExplainArgs {
    /// Diagnostic code or rule id (e.g. LEK-HIDDEN-009, loader.limit).
    #[arg(conflicts_with = "diagnostic_file")]
    selectors: Vec<String>,
    /// Explain every code found in an existing diagnostic report
    /// (validation output, any failure envelope) instead of selectors.
    #[arg(long, value_name = "FILE", conflicts_with = "selectors")]
    diagnostic_file: Option<String>,
    /// Optional offline provider explanation-reference catalog.
    #[arg(
        long,
        value_name = "FILE",
        requires = "diagnostic_file",
        conflicts_with = "selectors"
    )]
    provider_catalog: Option<String>,
    /// Human-output language. JSON bytes are independent of this flag.
    #[arg(long, value_enum, default_value_t = Locale::En)]
    locale: Locale,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Locale {
    En,
    Ru,
}

fn read_bounded(path: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(explain::MAX_REPORT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= explain::MAX_REPORT_BYTES).then_some(bytes)
}

pub(crate) fn run(args: ExplainArgs) -> DomainResult {
    if args.provider_catalog.is_some() && args.diagnostic_file.is_none() {
        return DomainResult::usage_error();
    }
    if let Some(path) = &args.diagnostic_file {
        let Some(bytes) = read_bounded(path) else {
            return explain::refusal("explain.report-invalid", "report-read");
        };
        let provider_bytes = match args.provider_catalog.as_deref().map(read_bounded) {
            Some(Some(bytes)) => Some(bytes),
            Some(None) => return explain::refusal("explain.report-invalid", "provider-read"),
            None => None,
        };
        return explain::localized(
            explain::explain_report_with_providers(&bytes, provider_bytes.as_deref()),
            matches!(args.locale, Locale::Ru),
        );
    }
    explain::localized(
        explain::resolve(&args.selectors),
        matches!(args.locale, Locale::Ru),
    )
}
