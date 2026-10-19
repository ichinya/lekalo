//! `lekalo fix`: syntax and presentation only; the core owns recipes,
//! risk/scope gates, fingerprints, validation and atomic custody.

use clap::Args;
use lekalo_core::fix::{self, Confirmation, PrepareOptions, Selection};
use lekalo_core::DomainResult;
use std::io::Read;

#[derive(Debug, Args)]
pub(crate) struct FixArgs {
    /// Diagnostic code/id or versioned recipe id (e.g. LEK-FORMAT-001).
    #[arg(conflicts_with_all = ["list", "safe", "recipe"])]
    selector: Option<String>,
    /// List admitted versioned recipes without loading a project or writing.
    #[arg(long, conflicts_with_all = ["safe", "recipe", "apply", "dry_run", "expect_plan", "policy", "allow_unsafe", "allow_breaking"])]
    list: bool,
    /// Select safe recipes only (preview unless --apply is explicit).
    #[arg(long, conflicts_with_all = ["recipe", "allow_unsafe", "allow_breaking"])]
    safe: bool,
    /// Closed recipe descriptor; adapter recipes must be namespaced/scoped.
    #[arg(long, value_name = "FILE")]
    recipe: Option<String>,
    /// Explicit preview; never writes (also the default).
    #[arg(long, conflicts_with = "apply")]
    dry_run: bool,
    /// Apply only after reviewing a complete preview; requires --expect-plan.
    #[arg(long)]
    apply: bool,
    /// Exact reviewed preview plan ID, including recipe/version/source pins.
    #[arg(long, value_name = "DIGEST")]
    expect_plan: Option<String>,
    /// Explicit acknowledgement for a selected unsafe recipe, never bulk safe.
    #[arg(long)]
    allow_unsafe: bool,
    /// Explicit acknowledgement for a selected breaking recipe; storage/data
    /// loss still needs reviewed migration evidence in --policy.
    #[arg(long)]
    allow_breaking: bool,
    /// Existing closed change policy for breaking semantic operations.
    #[arg(long, value_name = "FILE")]
    policy: Option<String>,
    /// Project root selector, relative to the invocation directory.
    #[arg(long, value_name = "DIR")]
    project: Option<String>,
}

fn read_bounded(path: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(fix::MAX_RECIPE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= fix::MAX_RECIPE_BYTES).then_some(bytes)
}

pub(crate) fn run(args: FixArgs) -> DomainResult {
    if args.list {
        return fix::list();
    }
    let recipe = match &args.recipe {
        Some(path) => match read_bounded(path) {
            Some(bytes) => Some(bytes),
            None => {
                return fix::FixError::Rule {
                    id: "fix.recipe-invalid",
                    detail: "recipe-read",
                }
                .into_result()
            }
        },
        None => None,
    };
    let selection = if let Some(bytes) = &recipe {
        Selection::Recipe(bytes)
    } else if args.safe {
        Selection::Safe
    } else if let Some(selector) = &args.selector {
        Selection::Diagnostic(selector)
    } else {
        return DomainResult::usage_error();
    };
    let policy = match &args.policy {
        Some(path) => match read_bounded(path) {
            Some(bytes) => Some(bytes),
            None => {
                return fix::FixError::Rule {
                    id: "fix.recipe-invalid",
                    detail: "policy-read",
                }
                .into_result()
            }
        },
        None => None,
    };
    let confirmation = args.apply.then_some(Confirmation {
        expect_plan: args.expect_plan.as_deref(),
        allow_unsafe: args.allow_unsafe,
        allow_breaking: args.allow_breaking,
        safe_only: args.safe,
    });
    fix::run(
        &PrepareOptions {
            project: args.project.as_deref(),
            selection,
            policy: policy.as_deref(),
            allow_unsafe: args.allow_unsafe,
            allow_breaking: args.allow_breaking,
        },
        confirmation.as_ref(),
    )
}
