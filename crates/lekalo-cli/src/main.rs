use clap::{error::ErrorKind, ColorChoice, Parser, Subcommand};
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

impl From<Commands> for Request {
    fn from(command: Commands) -> Self {
        match command {
            Commands::Validate => Self::Validate,
            Commands::Inspect { symbol } => Self::Inspect { symbol },
            Commands::Impact { symbol } => Self::Impact { symbol },
            Commands::Context { symbol, budget } => Self::Context { symbol, budget },
        }
    }
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
        Ok(cli) => {
            let request = Request::from(cli.command);
            emit(request.dispatch(), cli.json, OutputStream::Stdout)
        }
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
