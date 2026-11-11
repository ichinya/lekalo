//! Thin ownership lifecycle CLI. All admission and transaction decisions live
//! in core; the existing qualified host owns actual confined process execution.
use clap::{Args, Subcommand};
use lekalo_core::{ownership as core, result::DomainResult};

#[derive(Debug, Args)]
pub struct Project {
    /// Explicit project root; normal environment/discovery selection otherwise.
    #[arg(long, value_name = "DIR")]
    project: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum OwnershipCommands {
    /// Report declared module defaults and scoped handoffs, not a PASS verdict.
    Status {
        #[command(flatten)]
        project: Project,
    },
    /// Export neutral derived transition evidence for HLV/AIFHub consumers.
    Trace {
        #[command(flatten)]
        project: Project,
        #[arg(long, value_name = "SHA256")]
        receipt: String,
    },
    /// Preview a reviewed scoped request without changing the project.
    Plan {
        #[command(flatten)]
        project: Project,
        #[arg(long, value_name = "FILE")]
        request: String,
    },
    /// Execute both pinned implementations against the declared oracle.
    Verify {
        #[command(flatten)]
        project: Project,
        #[arg(long, value_name = "FILE")]
        plan: String,
    },
    /// Reverify and commit one exact confirmed handoff.
    Apply {
        #[command(flatten)]
        project: Project,
        #[arg(long, value_name = "FILE")]
        plan: String,
        #[arg(long, value_name = "SHA256")]
        confirm: String,
    },
    /// Preview restoration of one locally retained transition receipt.
    Rollback {
        #[command(flatten)]
        project: Project,
        #[arg(long, value_name = "SHA256")]
        receipt: String,
        #[arg(long, value_name = "SHA256", conflicts_with = "dry_run")]
        confirm: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Inspect or explicitly recover the pending ownership transaction.
    Recovery {
        #[command(flatten)]
        project: Project,
        #[arg(long, value_name = "SHA256", conflicts_with = "dry_run")]
        confirm: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
}

fn run_inner(command: OwnershipCommands) -> Result<DomainResult, DomainResult> {
    let selector = match &command {
        OwnershipCommands::Plan { project, .. }
        | OwnershipCommands::Verify { project, .. }
        | OwnershipCommands::Apply { project, .. }
        | OwnershipCommands::Rollback { project, .. }
        | OwnershipCommands::Recovery { project, .. }
        | OwnershipCommands::Status { project, .. }
        | OwnershipCommands::Trace { project, .. } => &project.project,
    };
    let root = lekalo_core::loader::resolve_selection_root(&crate::selection_for(selector))?;
    let host = crate::adoption::ProcessHost;
    Ok(match command {
        OwnershipCommands::Status { .. } => core::render(&core::status(&root)?),
        OwnershipCommands::Trace { receipt, .. } => core::render(&core::trace(&root, &receipt)?),
        OwnershipCommands::Plan { request, .. } => {
            let request = core::parse_request(&core::document(&root, &request)?)?;
            core::render(&core::plan(&root, request)?)
        }
        OwnershipCommands::Verify { plan, .. } => {
            let plan = core::parse_plan(&core::document(&root, &plan)?)?;
            core::render_verification(&core::verify(&root, &plan, &host)?)
        }
        OwnershipCommands::Apply { plan, confirm, .. } => {
            let plan = core::parse_plan(&core::document(&root, &plan)?)?;
            core::render(&core::apply(&root, &plan, &confirm, &host)?)
        }
        OwnershipCommands::Rollback {
            receipt, confirm, ..
        } => core::render(&core::rollback(&root, &receipt, confirm.as_deref())?),
        OwnershipCommands::Recovery { confirm, .. } => {
            core::render(&core::recovery(&root, confirm.as_deref())?)
        }
    })
}

pub fn run(command: OwnershipCommands) -> DomainResult {
    run_inner(command).unwrap_or_else(|error| error)
}
