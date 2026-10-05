//! Bounded, read-only offline evidence commands. No provider or tool launches.
use clap::{Args, Subcommand};
use lekalo_core::{framework_lift as eval, DomainResult};
use serde_json::Value;
use std::{io::Read, path::Path};

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Validate one closed local evaluation document; does not attest execution.
    Validate {
        #[arg(long)]
        family: String,
        #[arg(long)]
        input: String,
    },
    /// Verify approved protocol pins and baseline file bytes before an executor call.
    Preflight {
        #[command(flatten)]
        inputs: Inputs,
        #[arg(long)]
        workspace: String,
    },
    /// Admit one recorded arm against its exact preregistration (no execution).
    RecordArm {
        #[command(flatten)]
        inputs: Inputs,
        #[arg(long)]
        input: String,
    },
    /// Compare all scheduled slots; retains missing, failed and retried attempts.
    Compare {
        #[command(flatten)]
        inputs: Inputs,
        #[arg(long)]
        arm: Vec<String>,
        #[arg(long)]
        consumer_alias: String,
    },
}
#[derive(Debug, Args)]
pub struct Inputs {
    #[arg(long)]
    baseline: String,
    #[arg(long)]
    task: String,
    #[arg(long)]
    campaign: String,
}
fn read(path: &str) -> Result<Vec<u8>, eval::Failure> {
    let file =
        std::fs::File::open(path).map_err(|_| ("evaluation.protocol-invalid", "input-read"))?;
    if !file
        .metadata()
        .map_err(|_| ("evaluation.protocol-invalid", "input-read"))?
        .is_file()
    {
        return Err(("evaluation.protocol-invalid", "input-file"));
    }
    let mut bytes = Vec::new();
    file.take(eval::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ("evaluation.protocol-invalid", "input-read"))?;
    if bytes.len() > eval::MAX_BYTES {
        return Err(("evaluation.protocol-invalid", "input-size"));
    }
    Ok(bytes)
}
fn load(family: &str, path: &str) -> Result<Value, eval::Failure> {
    eval::parse(family, &read(path)?)
}
fn inputs(i: &Inputs) -> Result<(Value, Value, Value), eval::Failure> {
    let b = load("baseline", &i.baseline)?;
    let t = load("task", &i.task)?;
    let c = load("campaign", &i.campaign)?;
    eval::preflight(&b, &t, &c)?;
    Ok((b, t, c))
}
fn check_files(b: &Value, workspace: &str) -> Result<(), eval::Failure> {
    let root = Path::new(workspace)
        .canonicalize()
        .map_err(|_| ("evaluation.baseline-drift", "workspace"))?;
    for file in b["files"].as_array().unwrap() {
        let path = file["path"].as_str().unwrap();
        let mut current = root.clone();
        for part in path.split('/') {
            current.push(part);
            let m = std::fs::symlink_metadata(&current)
                .map_err(|_| ("evaluation.baseline-drift", "file-missing"))?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if m.file_attributes() & 0x400 != 0 {
                    return Err(("evaluation.baseline-drift", "reparse-point"));
                }
            }
            if m.file_type().is_symlink() {
                return Err(("evaluation.baseline-drift", "symlink"));
            }
        }
        let bytes = read(
            current
                .to_str()
                .ok_or(("evaluation.baseline-drift", "path"))?,
        )?;
        if file["digest"] != format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)) {
            return Err(("evaluation.baseline-drift", "file-bytes"));
        }
    }
    Ok(())
}
pub fn run(command: Commands) -> DomainResult {
    let result = (|| match command {
        Commands::Validate { family, input } => load(&family, &input),
        Commands::Preflight {
            inputs: i,
            workspace,
        } => {
            let (b, _, c) = inputs(&i)?;
            check_files(&b, &workspace)?;
            Ok(c)
        }
        Commands::RecordArm { inputs: i, input } => {
            let (b, t, c) = inputs(&i)?;
            let a = load("arm", &input)?;
            eval::admit_arm(&b, &t, &c, &a)?;
            Ok(a)
        }
        Commands::Compare {
            inputs: i,
            arm,
            consumer_alias,
        } => {
            let (b, t, c) = inputs(&i)?;
            let arms = arm
                .iter()
                .map(|p| load("arm", p))
                .collect::<Result<Vec<_>, _>>()?;
            eval::compare(&b, &t, &c, &arms, &consumer_alias)
        }
    })();
    match result {
        Ok(value) => DomainResult::receipt(
            eval::canonical(&value),
            "Recorded evaluation evidence validated; no agent was executed.".into(),
        ),
        Err(e) => eval::failure(e),
    }
}
