//! Thin CLI over the language-neutral runtime configuration service.
use clap::Subcommand;
use lekalo_core::runtime::{self, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create lekalo.yaml without executing or installing packages.
    Init {
        #[arg(long)]
        runtime: Option<String>,
        #[arg(long = "rules")]
        rules: Vec<String>,
        #[arg(long, default_value = ".")]
        path: String,
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    /// Resolve sources, negotiate APIs, and pin the selected composition.
    Lock {
        #[arg(long)]
        offline: bool,
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    /// Export a certified public fact to stdout for a dependent project.
    Export {
        #[arg(long)]
        component: String,
        #[arg(long)]
        package: String,
        #[arg(long)]
        rule: String,
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    /// Check source contracts with the locked runtime and rules, offline.
    Check {
        #[arg(long)]
        component: Option<String>,
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
}

pub fn emit(result: Result<Value>, json_output: bool) -> u8 {
    crate::emit(runtime::outcome(result), json_output)
}

pub fn configured(project: Option<&str>) -> bool {
    // A malformed file kind is still a selected config and must fail admission.
    std::fs::symlink_metadata(Path::new(project.unwrap_or(".")).join(runtime::config::CONFIG))
        .is_ok()
}

pub fn check(project: Option<&str>) -> lekalo_core::DomainResult {
    let result = std::fs::canonicalize(project.unwrap_or("."))
        .map_err(|_| runtime::error("project-missing"))
        .and_then(|root| runtime::check(&root, None));
    runtime::outcome(result)
}

pub fn run(command: Command, json_output: bool) -> u8 {
    let project = match &command {
        Command::Init { project, .. }
        | Command::Lock { project, .. }
        | Command::Export { project, .. }
        | Command::Check { project, .. } => project,
    };
    let root = match std::fs::canonicalize(project) {
        Ok(root) => root,
        Err(_) => return emit(Err(runtime::error("project-missing")), json_output),
    };
    let result = match command {
        Command::Init {
            runtime: source,
            rules,
            path,
            ..
        } => runtime::init(&root, source, rules, path),
        Command::Lock { offline, .. } => {
            runtime::lock(&root, offline).map(|lock| json!({"status":"valid","lock":lock}))
        }
        Command::Export {
            component,
            package,
            rule,
            ..
        } => runtime::export(&root, &component, &package, &rule),
        Command::Check { component, .. } => runtime::check(&root, component.as_deref()),
    };
    emit(result, json_output)
}
