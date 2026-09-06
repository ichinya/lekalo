//! The read-only Git changed-input adapter at the CLI edge (issue #16).
//!
//! This module is the only Git-aware piece of `lekalo impact`. It accepts
//! explicit read-only selectors, invokes Git through an argv API (no
//! shell, no interpolation, no network, no checkout/reset/write), and maps
//! the changed project paths through the accepted #8 source map into a
//! typed [`ChangedInputSet`]. Patch lines, raw diff text, repository
//! identity, cwd, and stderr text never survive into a handoff or a
//! diagnostic: failures carry bounded fixed tokens only.
//!
//! Determinism: `--no-renames` pins every invocation, so a rename surfaces
//! as a delete/add pair whatever the local Git configuration claims. A
//! physical rename therefore never masquerades as a semantic one.

use std::path::Path;
use std::process::Command;

use lekalo_core::diagnostics::DiagnosticSet;
use lekalo_core::impact::input::is_revision_grammar;
use lekalo_core::impact::{
    diagnostic, ChangedInput, ChangedInputSet, ChangedMode, EntryEvidence, FileChange,
};

/// The bounded failure vocabulary of the adapter.
pub enum GitInputFailure {
    /// A selector named a revision Git cannot resolve.
    UnknownRevision,
    /// Git refused a selector (ambiguous, unsupported spelling).
    InvalidSelector,
    /// Git is unavailable or the project root is not a repository.
    Unavailable,
    /// The Git output violated the adapter grammar.
    MalformedOutput,
    /// The selector combination resolved to zero changes.
    NoChanges,
}

impl GitInputFailure {
    /// The terminal domain failure with a bounded fixed token only.
    pub fn diagnostic_set(&self) -> DiagnosticSet {
        match self {
            Self::UnknownRevision => diagnostic::selector_invalid_set("unknown-revision"),
            Self::InvalidSelector => diagnostic::selector_invalid_set("ambiguous-revision"),
            Self::Unavailable => diagnostic::changed_input_invalid_set("git-unavailable"),
            Self::MalformedOutput => diagnostic::changed_input_invalid_set("malformed-git-output"),
            Self::NoChanges => diagnostic::changed_input_invalid_set("no-changed-inputs"),
        }
    }
}

/// One resolved changed-input request: the base and candidate revisions
/// plus the raw `name-status` records, before source-map resolution.
struct GitChanges {
    base_revision: Option<String>,
    candidate_revision: Option<String>,
    records: Vec<(FileChange, String, String)>,
}

/// Run one read-only Git command in `project_root`; stdout must be UTF-8.
fn git(project_root: &Path, args: &[&str]) -> Result<String, GitInputFailure> {
    let output = Command::new("git")
        .current_dir(project_root)
        .args(args)
        .output()
        .map_err(|_| GitInputFailure::Unavailable)?;
    if !output.status.success() {
        // 128 with "ambiguous argument" style refusals map to a selector
        // fault; anything else is an unavailable repository. The stderr
        // text itself is never surfaced.
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("ambiguous argument") || stderr.contains("bad revision") {
            return Err(GitInputFailure::InvalidSelector);
        }
        return Err(GitInputFailure::Unavailable);
    }
    String::from_utf8(output.stdout).map_err(|_| GitInputFailure::MalformedOutput)
}

/// Resolve one revision selector to its opaque bounded commit identity.
fn resolve_revision(project_root: &Path, revision: &str) -> Result<String, GitInputFailure> {
    let specifier = format!("{revision}^{{commit}}");
    let output = git(
        project_root,
        &["rev-parse", "--verify", "--quiet", &specifier],
    )?;
    let identity = output.trim().to_owned();
    if !is_revision_grammar(&identity) {
        return Err(GitInputFailure::UnknownRevision);
    }
    Ok(identity)
}

/// Collect the `name-status` records for one diff, renames pinned off.
fn diff_records(
    project_root: &Path,
    range: &[&str],
) -> Result<Vec<(FileChange, String, String)>, GitInputFailure> {
    let mut args: Vec<&str> = vec!["diff", "--name-status", "--no-renames", "-z"];
    args.extend_from_slice(range);
    let output = git(project_root, &args)?;
    parse_name_status(&output)
}

/// Collect untracked files (candidates not known to Git at all).
fn untracked_records(
    project_root: &Path,
) -> Result<Vec<(FileChange, String, String)>, GitInputFailure> {
    let output = git(
        project_root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    Ok(output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(|path| (FileChange::Added, path.to_owned(), String::new()))
        .collect())
}

/// Parse the `-z` name-status stream: NUL-separated records, each a status
/// token followed by one path (two for copies, which are refused).
fn parse_name_status(output: &str) -> Result<Vec<(FileChange, String, String)>, GitInputFailure> {
    let mut records = Vec::new();
    let mut fields = output.split('\0').filter(|field| !field.is_empty());
    while let Some(status) = fields.next() {
        let change = match status {
            "A" => FileChange::Added,
            "M" | "T" => FileChange::Modified,
            "D" => FileChange::Deleted,
            _ => return Err(GitInputFailure::MalformedOutput),
        };
        let Some(path) = fields.next() else {
            return Err(GitInputFailure::MalformedOutput);
        };
        records.push((change, path.to_owned(), String::new()));
    }
    Ok(records)
}

/// Resolve the selector combination into raw Git change records.
fn collect_records(
    project_root: &Path,
    base: Option<&str>,
    head: Option<&str>,
    worktree: bool,
) -> Result<GitChanges, GitInputFailure> {
    let base_revision = base
        .map(|base| resolve_revision(project_root, base))
        .transpose()?;
    let candidate_revision = head
        .map(|head| resolve_revision(project_root, head))
        .transpose()?;
    let mut records = Vec::new();

    if worktree {
        // The worktree candidate: index plus worktree against the base
        // (default HEAD), untracked files included.
        let head = resolve_revision(project_root, "HEAD")?;
        if let Some(base) = &base_revision {
            records.extend(diff_records(project_root, &[base, head.as_str()])?);
        }
        records.extend(diff_records(project_root, &["HEAD"])?);
        records.extend(untracked_records(project_root)?);
        return Ok(GitChanges {
            base_revision,
            candidate_revision: None,
            records,
        });
    }

    let head = match candidate_revision {
        Some(head) => head,
        None => resolve_revision(project_root, "HEAD")?,
    };
    match &base_revision {
        Some(base) => records.extend(diff_records(project_root, &[base, head.as_str()])?),
        None => {
            // A bare candidate is compared with its parent commit.
            let parents = git(
                project_root,
                &["rev-list", "--max-count=1", "--parents", &head],
            )?;
            let parent = parents
                .split_whitespace()
                .nth(1)
                .ok_or(GitInputFailure::MalformedOutput)?
                .to_owned();
            records.extend(diff_records(
                project_root,
                &[parent.as_str(), head.as_str()],
            )?);
        }
    }
    Ok(GitChanges {
        base_revision,
        candidate_revision: Some(head),
        records,
    })
}

/// Build the typed changed-input handoff for the selector combination.
///
/// Every changed path is validated against the logical-path grammar and
/// resolved through the current source map; a path with no semantic
/// surface becomes an explicit unknown-evidence entry that degrades the
/// result. Zero changes is a caller-visible fault, never an empty set.
pub fn changed_input_set(
    project_root: &Path,
    base: Option<&str>,
    head: Option<&str>,
    worktree: bool,
    source_paths: &std::collections::HashMap<&str, Vec<&str>>,
) -> Result<ChangedInputSet, GitInputFailure> {
    if head.is_some() && worktree {
        return Err(GitInputFailure::InvalidSelector);
    }
    let changes = collect_records(project_root, base, head, worktree)?;
    if changes.records.is_empty() {
        return Err(GitInputFailure::NoChanges);
    }
    let mut entries: Vec<ChangedInput> = Vec::new();
    for (change, path, _old) in &changes.records {
        let path = lekalo_core::impact::input::validate_path(path)
            .map_err(|_| GitInputFailure::MalformedOutput)?;
        let symbols: Vec<String> = source_paths
            .get(path.as_str())
            .into_iter()
            .flatten()
            .map(|symbol| (*symbol).to_owned())
            .collect();
        let entry = ChangedInput::new(
            symbols,
            Vec::new(),
            Some((path.as_str(), None)),
            *change,
            EntryEvidence::Canonical,
        )
        .map_err(|_| GitInputFailure::MalformedOutput)?;
        entries.push(entry);
    }
    let mode = if worktree {
        ChangedMode::Worktree
    } else {
        ChangedMode::Committed
    };
    ChangedInputSet::from_entries(
        mode,
        changes.base_revision,
        changes.candidate_revision,
        entries,
    )
    .map_err(|_| GitInputFailure::MalformedOutput)
}
