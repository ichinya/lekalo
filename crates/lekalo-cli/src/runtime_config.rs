//! Thin CLI over the language-neutral runtime configuration service.
use clap::Subcommand;
use lekalo_core::runtime::{self, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Restore immutable package snapshots and verify the selected local executables.
    Restore {
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },

    /// Create lekalo.yaml without executing or installing packages.
    Init {
        #[arg(long)]
        runtime: Option<String>,
        #[arg(long, requires = "runtime")]
        target_version: Option<String>,
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
    let root = Path::new(project.unwrap_or("."));
    if std::fs::symlink_metadata(root.join(runtime::config::CONFIG)).is_err() {
        return false;
    }
    // A greenfield model has an empty editable component declaration until selection.
    !(root.join("lekalo/project.yaml").is_file()
        && runtime::load(root).is_ok_and(|c| c.components.is_empty() && c.services.is_empty()))
}

pub(crate) fn resolve_root(project: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(project).map_err(|_| runtime::error("project-missing"))
}

pub fn run(command: Command, json_output: bool) -> u8 {
    let project = match &command {
        Command::Restore { project }
        | Command::Init { project, .. }
        | Command::Lock { project, .. }
        | Command::Export { project, .. }
        | Command::Check { project, .. } => project,
    };
    let root = match resolve_root(project) {
        Ok(root) => root,
        Err(error) => return emit(Err(error), json_output),
    };
    let result = match command {
        Command::Restore { .. } => {
            runtime::restore(&root).map(|lock| json!({"status":"valid","lock":lock}))
        }
        Command::Init {
            runtime: source,
            target_version,
            rules,
            path,
            ..
        } => runtime::init(&root, source, target_version, rules, path),
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
