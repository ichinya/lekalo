//! `lekalo explain`: read-only, offline explanation of stable diagnostic
//! codes; the core owns the versioned catalog and its closed lookups.

use clap::Args;
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
    if let Some(path) = &args.diagnostic_file {
        let Some(bytes) = read_bounded(path) else {
            return explain::refusal("explain.report-invalid", "report-read");
        };
        return explain::explain_report(&bytes);
    }
    explain::resolve(&args.selectors)
}
