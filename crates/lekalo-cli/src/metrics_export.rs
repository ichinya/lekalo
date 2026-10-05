use clap::Subcommand;
use lekalo_core::result::DomainResult;

#[derive(Debug, Subcommand)]
pub enum MetricsCommands {
    /// Preview or confirm a privacy-safe aggregate from live local history.
    Export {
        /// The named evaluation-export-input contract (#100 trial membership).
        #[arg(long, value_name = "FILE")]
        evaluation: String,
        #[arg(long, value_name = "TOKEN")]
        scope: String,
        /// The exact #119 destination spec; publication prepares a local package.
        #[arg(long, default_value = "publish", value_name = "SPEC")]
        destination: String,
        /// Show exact payload, manifest, subject and redaction/leak reports; no writes.
        #[arg(long, required_unless_present = "confirm", conflicts_with = "confirm")]
        dry_run: bool,
        /// Confirm the digest of an authorized preview; stale inputs refuse.
        #[arg(
            long,
            value_name = "PREVIEW_DIGEST",
            required_unless_present = "dry_run"
        )]
        confirm: Option<String>,
        /// Complete #119 ExportDecisionInput with purpose-bound authorizing evidence.
        #[arg(long, value_name = "FILE")]
        authorization: Option<String>,
        /// Optional exact copy of the embedded versioned aggregation definition.
        #[arg(long, value_name = "FILE")]
        recipe: Option<String>,
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Recheck source deletion, byte integrity and current declared authorization.
    Status {
        export_id: String,
        #[arg(long, value_name = "TOKEN")]
        scope: String,
        #[arg(long, value_name = "FILE")]
        authorization: Option<String>,
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
}

pub fn run(command: MetricsCommands) -> DomainResult {
    let project = match &command {
        MetricsCommands::Export { project, .. } | MetricsCommands::Status { project, .. } => {
            project
        }
    };
    let root = match super::project_root_for(project) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let result = match command {
        MetricsCommands::Export {
            evaluation,
            scope,
            destination,
            authorization,
            recipe,
            confirm,
            ..
        } => {
            let Some(destination) =
                lekalo_core::privacy::export::DestinationSpec::parse(&destination)
            else {
                return refusal(lekalo_core::metrics_export::Error::Invalid);
            };
            lekalo_core::metrics_export::export(
                &root,
                &evaluation,
                &scope,
                destination,
                authorization.as_deref(),
                recipe.as_deref(),
                confirm.as_deref(),
            )
        }
        MetricsCommands::Status {
            export_id,
            scope,
            authorization,
            ..
        } => {
            lekalo_core::metrics_export::status(&root, &export_id, &scope, authorization.as_deref())
        }
    };
    match result {
        Ok(value) => DomainResult::receipt(
            format!("{}\n", serde_json::to_string(&value).unwrap()),
            "metrics export receipt".to_owned(),
        ),
        Err(error) => refusal(error),
    }
}

fn refusal(error: lekalo_core::metrics_export::Error) -> DomainResult {
    lekalo_core::metrics_export::refusal(error)
}
