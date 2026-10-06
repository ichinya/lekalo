//! Issue #94: the `lekalo format` command surface. The core owns every
//! decision; this module only shapes the selector, reads the editor
//! buffer, and hands the request to the pure core service.

use clap::Args;
use lekalo_core::format::{FormatMode, FormatScope};
use lekalo_core::loader::source::MAX_DOCUMENT_BYTES;
use lekalo_core::{format, DomainResult};
use std::io::Read;

/// The `format` argument surface: the scoped selector family, the
/// read-only check gate, the explicit lossy opt-in, and the exclusive
/// editor variant.
#[derive(Debug, Args)]
pub struct FormatArgs {
    /// Safe project-relative admitted document or module directory
    /// (`lekalo/modules/planner`); omitted selects the whole admitted
    /// Model source set.
    #[arg(value_name = "PATH")]
    path: Option<String>,
    /// Project root selector, relative to the invocation directory.
    #[arg(long, value_name = "DIR")]
    project: Option<String>,
    /// Read-only gate: report the canonical diff without writing.
    #[arg(long)]
    check: bool,
    /// Accept the enumerated comment losses of the plan (never semantic
    /// loss, never unknown schemas, never unsafe writes).
    #[arg(long)]
    allow_lossy_comments: bool,
    /// Editor mode: read one unsaved buffer snapshot from stdin and
    /// return the verified full-document replacement; never writes.
    #[arg(
        long,
        requires = "stdin_filepath",
        conflicts_with_all = ["path", "check", "allow_lossy_comments"]
    )]
    stdin: bool,
    /// The admitted document path the editor buffer belongs to.
    #[arg(long, value_name = "PATH", requires = "stdin")]
    stdin_filepath: Option<String>,
}

/// Run one `format` command to its terminal domain result.
pub fn run(args: FormatArgs) -> DomainResult {
    let mode = if args.stdin {
        FormatMode::Stdin
    } else if args.check {
        FormatMode::Check
    } else {
        FormatMode::Apply
    };
    let scope = if args.stdin {
        FormatScope::Document(args.stdin_filepath.as_ref().expect("clap requires").clone())
    } else {
        match args.path.as_deref() {
            None => FormatScope::All,
            Some(path) => match resolve_scope(path) {
                Ok(scope) => scope,
                Err(result) => return result,
            },
        }
    };
    let stdin_bytes = if args.stdin {
        let mut buffer = Vec::new();
        let read = std::io::stdin()
            .lock()
            .take((MAX_DOCUMENT_BYTES as u64) + 1)
            .read_to_end(&mut buffer);
        if read.is_err() {
            return DomainResult::usage_error();
        }
        Some(buffer)
    } else {
        None
    };
    let request = format::FormatRequest {
        project: args
            .project
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
        scope,
        mode,
        allow_lossy_comments: args.allow_lossy_comments,
    };
    format::run(&request, stdin_bytes.as_deref())
}

/// Shape the project-relative selector: an exact module directory maps to
/// the module scope, everything else must match the admitted inventory
/// exactly (the core refuses non-admitted documents).
fn resolve_scope(path: &str) -> Result<format::FormatScope, DomainResult> {
    let mut segments = path.split('/');
    if segments.next() == Some("lekalo")
        && segments.next() == Some("modules")
        && segments.next().is_some()
        && segments.next().is_none()
    {
        return Ok(FormatScope::Module(path.to_owned()));
    }
    Ok(FormatScope::Document(path.to_owned()))
}
