use clap::{error::ErrorKind, ColorChoice, Parser, Subcommand};
use lekalo_core::loader::LoadSelection;
use lekalo_core::{DomainResult, Request};
use std::ffi::OsStr;
use std::io::{self, Write};
use std::process::ExitCode;

const PROGRAM_NAME: &str = "lekalo";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const OUTPUT_FAILURE: u8 = 1;

#[derive(Debug, Parser)]
#[command(
    name = "lekalo",
    version,
    about = "Compiler and validator foundation for Lekalo",
    disable_help_subcommand = true,
    color = ColorChoice::Never
)]
struct Cli {
    /// Emit the stable JSON envelope.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Validate a Lekalo project (recognized; implementation follows in a later issue).
    Validate,
    /// Load YAML/JSON sources, resolve imports, and emit the canonical model.
    Load {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Emit the sourceMap alongside the canonical model (JSON only).
        #[arg(long)]
        spans: bool,
    },
    /// Inspect one semantic symbol (recognized; implementation follows in a later issue).
    Inspect { symbol: String },
    /// Report the impact of one semantic symbol (recognized; implementation follows later).
    Impact { symbol: String },
    /// Build a bounded context for one semantic symbol (recognized; implementation follows later).
    Context {
        symbol: String,
        #[arg(long, value_name = "TOKENS")]
        budget: u64,
    },
}

#[derive(Clone, Copy)]
enum OutputStream {
    Stdout,
    Stderr,
}

fn main() -> ExitCode {
    let json_requested = std::env::args_os()
        .skip(1)
        .take_while(|argument| argument != OsStr::new("--"))
        .any(|argument| argument == OsStr::new("--json"));

    match Cli::try_parse() {
        Ok(cli) => match cli.command {
            Commands::Load { project, spans } => run_load(project, spans, cli.json),
            Commands::Validate => {
                emit(Request::Validate.dispatch(), cli.json, OutputStream::Stdout)
            }
            Commands::Inspect { symbol } => emit(
                Request::Inspect { symbol }.dispatch(),
                cli.json,
                OutputStream::Stdout,
            ),
            Commands::Impact { symbol } => emit(
                Request::Impact { symbol }.dispatch(),
                cli.json,
                OutputStream::Stdout,
            ),
            Commands::Context { symbol, budget } => emit(
                Request::Context { symbol, budget }.dispatch(),
                cli.json,
                OutputStream::Stdout,
            ),
        },
        Err(error) => match error.kind() {
            ErrorKind::DisplayHelp => {
                if error.print().is_ok() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(OUTPUT_FAILURE)
                }
            }
            ErrorKind::DisplayVersion => emit(
                DomainResult::version(VERSION),
                json_requested,
                OutputStream::Stdout,
            ),
            _ => emit(
                DomainResult::usage_error(),
                json_requested,
                OutputStream::Stderr,
            ),
        },
    }
}

/// Resolve the selection (explicit `--project` beats `LEKALO_PROJECT`) and
/// run the loader; render the typed outcome to its protocol stream.
fn run_load(project: Option<String>, spans: bool, json: bool) -> ExitCode {
    let selection = LoadSelection {
        project: project.or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let outcome = lekalo_core::loader::run(&selection, spans);
    let exit_code = outcome.status.exit_code();
    let stream = if outcome.status.writes_stderr() {
        OutputStream::Stderr
    } else {
        OutputStream::Stdout
    };
    let mut handle: Box<dyn Write> = match stream {
        OutputStream::Stdout => Box::new(io::stdout().lock()),
        OutputStream::Stderr => Box::new(io::stderr().lock()),
    };
    let rendered = if json { outcome.json } else { outcome.human };
    let write = handle
        .write_all(rendered.as_bytes())
        .and_then(|()| handle.write_all(b"\n"));
    if write.is_ok() {
        ExitCode::from(exit_code)
    } else {
        ExitCode::from(OUTPUT_FAILURE)
    }
}

fn emit(result: DomainResult, json: bool, stream: OutputStream) -> ExitCode {
    let exit_code = result.exit_code();
    let write_result = match stream {
        OutputStream::Stdout => {
            let stdout = io::stdout();
            render(&mut stdout.lock(), &result, json)
        }
        OutputStream::Stderr => {
            let stderr = io::stderr();
            render(&mut stderr.lock(), &result, json)
        }
    };

    if write_result.is_ok() {
        ExitCode::from(exit_code)
    } else {
        ExitCode::from(OUTPUT_FAILURE)
    }
}

fn render(writer: &mut impl Write, result: &DomainResult, json: bool) -> io::Result<()> {
    let rendered = if json {
        serde_json::to_string_pretty(result).map_err(io::Error::other)?
    } else {
        result.human_line(PROGRAM_NAME)
    };

    writer.write_all(rendered.as_bytes())?;
    writer.write_all(b"\n")
}
