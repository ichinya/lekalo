//! Explicit installation of the embedded agent instruction (issue #157).
use clap::{Args, Subcommand};
use lekalo_core::{managed::fs, project_fs::Fs, DomainResult};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const PROMPT: &str = ".agents/lekalo.md";
const AGENTS: &str = "AGENTS.md";
const BEGIN: &str = "<!-- lekalo:begin -->";
const END: &str = "<!-- lekalo:end -->";
const BODY: &str = include_str!("agent_prompt.md");
// Exact previously shipped body; arbitrary self-declared content hashes do not
// establish ownership. This permits an ordinary upgrade without discarding edits.
const PREVIOUS_BODY_DIGEST: &str =
    "sha256:10ce92c1bab330cb7b1fb8822bd10f87e1d660429ca16ac930c613f401fc46f3";
const POINTER: &str = "Read [.agents/lekalo.md](.agents/lekalo.md) before working with Lekalo.\nThis instruction describes the CLI and authority boundaries; it grants no additional permission.\n";

#[derive(Debug, Subcommand)]
pub(crate) enum InstallCommands {
    /// Install or explicitly update the embedded Lekalo agent instruction.
    Prompt(Options),
}

#[derive(Debug, Args)]
pub(crate) struct Options {
    #[arg(long, default_value = ".", value_name = "DIR")]
    project: PathBuf,
    /// Replace manually edited managed content. Ambiguous markers still refuse.
    #[arg(long, conflicts_with = "check")]
    force: bool,
    /// Show every planned change without writing files or directories.
    #[arg(long, conflicts_with = "check")]
    dry_run: bool,
    /// Explicitly create AGENTS.md when it is absent.
    #[arg(long)]
    create_agents_md: bool,
    /// Report missing/stale managed instructions without writing.
    #[arg(long)]
    check: bool,
}

fn failure(reason: &str) -> DomainResult {
    lekalo_core::managed::diagnostic::failure(
        lekalo_core::managed::INPUT_INVALID,
        &[("path", "agents-prompt"), ("reason", reason)],
    )
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", lekalo_core::digest::sha256_hex(bytes))
}

fn metadata() -> String {
    let registry: Value = serde_json::from_slice(lekalo_core::versioning::registry::REGISTRY_BYTES)
        .expect("embedded registry JSON");
    format!(
        "<!-- lekalo:prompt v1 product={} registry={} content={} -->\n",
        env!("CARGO_PKG_VERSION"),
        digest(&serde_json::to_vec(&registry).expect("canonical registry JSON")),
        digest(BODY.replace("\r\n", "\n").as_bytes())
    )
}

fn owned(text: &str, expected_body: &str) -> bool {
    let text = text.replace("\r\n", "\n");
    let Some((header, body)) = text.split_once('\n') else {
        return false;
    };
    let Some(header) = header
        .strip_prefix("<!-- lekalo:prompt v1 product=")
        .and_then(|s| s.strip_suffix(" -->"))
    else {
        return false;
    };
    let fields: Vec<_> = header.split(' ').collect();
    if fields.len() != 3 {
        return false;
    }
    let Some(content) = fields[2].strip_prefix("content=") else {
        return false;
    };
    let recognized =
        content == digest(BODY.replace("\r\n", "\n").as_bytes()) || content == PREVIOUS_BODY_DIGEST;
    recognized
        && canonical_release(fields[0])
        && fields[1]
            .strip_prefix("registry=")
            .is_some_and(lekalo_core::target_protocol::wire::is_sha256_digest)
        && if expected_body == POINTER {
            body == POINTER
        } else {
            digest(body.as_bytes()) == content
        }
}

fn canonical_release(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    value.len() <= 32
        && parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (*part == "0" || !part.starts_with('0'))
        })
}

fn agents_update(
    before: Option<&str>,
    create: bool,
    force: bool,
) -> Result<Option<String>, DomainResult> {
    let Some(before) = before else {
        return Ok(create.then(|| format!("{BEGIN}\n{}{POINTER}{END}\n", metadata())));
    };
    let begins: Vec<_> = before.match_indices(BEGIN).map(|(i, _)| i).collect();
    let ends: Vec<_> = before.match_indices(END).map(|(i, _)| i).collect();
    let newline = if before.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let section = format!("{BEGIN}\n{}{POINTER}{END}", metadata()).replace('\n', newline);
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => {
            // Append only: every existing byte remains in the prefix.
            let separator = if before.is_empty() || before.ends_with('\n') {
                ""
            } else {
                newline
            };
            Ok(Some(format!("{before}{separator}{section}{newline}")))
        }
        ([start], [end]) if start < end => {
            let content = &before[start + BEGIN.len()..*end];
            let normalized = content.replace("\r\n", "\n");
            if !force
                && !owned(
                    normalized.strip_prefix('\n').unwrap_or(&normalized),
                    POINTER,
                )
            {
                return Err(failure("prompt-section-conflict"));
            }
            Ok(Some(format!(
                "{}{section}{}",
                &before[..*start],
                &before[end + END.len()..]
            )))
        }
        _ => Err(failure("prompt-markers-ambiguous")),
    }
}

fn read(root: &Path, path: &str) -> Result<Option<Vec<u8>>, DomainResult> {
    let fs = Fs::open(root).map_err(|_| failure("prompt-root-unconfined"))?;
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    match fs.read_file_opt(dir, name, 1024 * 1024) {
        Err(lekalo_core::project_fs::FsErrorKind::NotFound) => Ok(None),
        result => result.map_err(|_| failure("prompt-path-unconfined")),
    }
}

fn text(bytes: &Option<Vec<u8>>) -> Result<Option<&str>, DomainResult> {
    bytes
        .as_deref()
        .map(std::str::from_utf8)
        .transpose()
        .map_err(|_| failure("prompt-encoding"))
}

struct Change {
    path: &'static str,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
}

fn diff(change: &Change) -> String {
    let before = std::str::from_utf8(change.before.as_deref().unwrap_or_default()).unwrap();
    let after = std::str::from_utf8(&change.after).unwrap();
    let old: Vec<_> = before.split_inclusive('\n').collect();
    let new: Vec<_> = after.split_inclusive('\n').collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let removed = &old[prefix..old.len() - suffix];
    let added = &new[prefix..new.len() - suffix];
    let mut result = format!(
        "--- {}\n+++ b/{}\n@@ -{},{} +{},{} @@\n",
        if change.before.is_none() {
            "/dev/null".to_owned()
        } else {
            format!("a/{}", change.path)
        },
        change.path,
        prefix + usize::from(!removed.is_empty()),
        removed.len(),
        prefix + usize::from(!added.is_empty()),
        added.len()
    );
    for (sign, lines) in [('-', removed), ('+', added)] {
        for line in lines {
            result.push(sign);
            result.push_str(line);
            if !line.ends_with('\n') {
                result.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    result
}

struct Lock<'a> {
    root: &'a Path,
    file: Option<std::fs::File>,
}

fn rollback(root: &Path, changes: &[Change]) -> Result<(), DomainResult> {
    for previous in changes.iter().rev() {
        if read(root, previous.path)? != Some(previous.after.clone())
            || read(root, &format!("{}.stage", previous.path))?.is_some()
        {
            return Err(failure("prompt-rollback-conflict"));
        }
        match &previous.before {
            Some(bytes) => fs::write_file(root, previous.path, bytes)?,
            None => {
                fs::remove_file(root, previous.path)?;
            }
        }
    }
    Ok(())
}
impl Drop for Lock<'_> {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(self.root, ".agents/lekalo-install.lock");
    }
}

fn perform(options: Options) -> Result<Value, DomainResult> {
    for ancestor in options
        .project
        .ancestors()
        .filter(|p| !p.as_os_str().is_empty())
    {
        let meta =
            std::fs::symlink_metadata(ancestor).map_err(|_| failure("prompt-root-unconfined"))?;
        if !meta.is_dir() || lekalo_core::project_fs::metadata_is_physical_link(&meta) {
            return Err(failure("prompt-root-unconfined"));
        }
    }
    Fs::open(&options.project).map_err(|_| failure("prompt-root-unconfined"))?;
    let root =
        std::fs::canonicalize(&options.project).map_err(|_| failure("prompt-root-unconfined"))?;
    let prompt_before = read(&root, PROMPT)?;
    let agents_before = read(&root, AGENTS)?;
    if let Some(before) = text(&prompt_before)? {
        if !options.force && !owned(before, BODY) {
            return Err(failure("prompt-content-conflict"));
        }
    }
    let prompt = format!("{}{}", metadata(), BODY.replace("\r\n", "\n")).into_bytes();
    let agents = agents_update(
        text(&agents_before)?,
        options.create_agents_md,
        options.force,
    )?;
    let mut changes = Vec::new();
    if prompt_before.as_deref() != Some(prompt.as_slice()) {
        changes.push(Change {
            path: PROMPT,
            before: prompt_before,
            after: prompt,
        });
    }
    if let Some(agents) = agents {
        if agents_before.as_deref() != Some(agents.as_bytes()) {
            changes.push(Change {
                path: AGENTS,
                before: agents_before,
                after: agents.into_bytes(),
            });
        }
    }
    let planned: Vec<_> = changes.iter().map(|c|json!({"path":c.path,"beforeDigest":c.before.as_ref().map(|b|digest(b)),"afterDigest":digest(&c.after),"diff":diff(c)})).collect();
    if options.check && !changes.is_empty() {
        return Err(failure("prompt-stale"));
    }
    if !options.check && !options.dry_run && !changes.is_empty() {
        for change in &changes {
            if read(&root, &format!("{}.stage", change.path))?.is_some() {
                return Err(failure("prompt-stage-conflict"));
            }
        }
        fs::ensure_dir(&root, ".agents")?;
        // Cooperating invocations cannot overwrite each other's plans.
        let lock_path = root.join(".agents/lekalo-install.lock");
        let lock = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
            .map_err(|_| failure("prompt-install-busy"))?;
        let guard = Lock {
            root: &root,
            file: Some(lock),
        };
        // The lock is not a substitute for current byte/physical custody.
        for change in &changes {
            if read(&root, change.path)? != change.before {
                return Err(failure("prompt-plan-stale"));
            }
            if read(&root, &format!("{}.stage", change.path))?.is_some() {
                return Err(failure("prompt-stage-conflict"));
            }
        }
        for (index, change) in changes.iter().enumerate() {
            if read(&root, change.path)? != change.before
                || read(&root, &format!("{}.stage", change.path))?.is_some()
            {
                rollback(&root, &changes[..index])?;
                return Err(failure("prompt-plan-stale"));
            }
            if fs::write_file(&root, change.path, &change.after).is_err() {
                let stage = format!("{}.stage", change.path);
                if read(&root, &stage)? == Some(change.after.clone()) {
                    fs::remove_file(&root, &stage)?;
                }
                rollback(&root, &changes[..index])?;
                return Err(failure("prompt-write-failed"));
            }
        }
        drop(guard);
    }
    Ok(
        json!({"kind":"agent-prompt-install","productVersion":env!("CARGO_PKG_VERSION"),"mode":if options.check{"check"}else if options.dry_run{"dry-run"}else{"apply"},"state":if changes.is_empty(){"current"}else if options.dry_run{"planned"}else{"updated"},"changes":planned}),
    )
}

pub(crate) fn run(command: InstallCommands) -> DomainResult {
    let InstallCommands::Prompt(options) = command;
    match perform(options) {
        Ok(value) => {
            let details = value["changes"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|c| c["diff"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            DomainResult::receipt(
                serde_json::to_string_pretty(&value).unwrap(),
                format!(
                    "agent prompt : {}\n{details}",
                    value["state"].as_str().unwrap()
                ),
            )
        }
        Err(failure) => failure,
    }
}
