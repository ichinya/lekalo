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
        /// Emit the sourceMap alongside the canonical model or IR (JSON only).
        #[arg(long)]
        spans: bool,
        /// Compile the loaded model into the typed, deterministic Lekalo IR
        /// and emit its canonical bytes instead of the preserved model.
        #[arg(long)]
        ir: bool,
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
            Commands::Load { project, spans, ir } => run_load(project, spans, ir, cli.json),
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
fn run_load(project: Option<String>, spans: bool, ir: bool, json: bool) -> ExitCode {
    let selection = LoadSelection {
        project: project.or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let outcome = match ir {
        false => lekalo_core::loader::run(&selection, spans),
        true => match lekalo_core::loader::normalize_model(&selection) {
            Err(outcome) => outcome,
            Ok(model) => match lekalo_core::ir::compile(&model) {
                Err(failure) => failure.load_output(),
                Ok(compilation) => render_ir_success(&model, &compilation, spans),
            },
        },
    };
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
/// Render the typed IR success envelope: the fixed key order `status`,
/// `modelVersion`, `ir`, and the sorted IR sourceMap when `--spans` was
/// requested. The IR object itself is the canonical IR bytes.
fn render_ir_success(
    model: &lekalo_core::loader::NormalizedModel,
    compilation: &lekalo_core::ir::Compilation,
    spans: bool,
) -> lekalo_core::loader::LoadOutput {
    let mut json = String::from("{\"status\":\"valid\",\"modelVersion\":");
    json.push_str(
        &serde_json::to_string(model.model_version.as_str()).expect("version serializes"),
    );
    json.push_str(",\"ir\":");
    json.push_str(&compilation.project.to_canonical_json());
    if spans {
        json.push_str(",\"sourceMap\":");
        json.push_str(&compilation.source_map.to_json());
    }
    json.push('}');
    lekalo_core::loader::LoadOutput {
        status: lekalo_core::loader::LoadStatus::Valid,
        human: format!(
            "compiled ir {}: {} modules, {} definitions",
            lekalo_core::ir::IDENTITY,
            model.modules.len(),
            model.definitions.len()
        ),
        json,
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
