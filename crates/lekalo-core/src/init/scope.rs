//! Versioned discovery policy for existing repositories (issue #149).
//! Paths are literal repository-relative roots, never include globs.

use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Serialize};

use crate::project_fs::{EntryType, Fs, FsErrorKind};
use crate::{DomainResult, Status};

pub const VERSION: &str = "0.9.0";
const SCHEMA: &str = "lekalo/discovery-scope/v0.9.0";
const IDENTITY: &str = "dev.lekalo.discovery-scope@0.9.0";
pub(crate) const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_IGNORE_FILES: usize = 128;
const MAX_IGNORE_LINES: usize = 1024;
const MAX_SKIPS: usize = 128;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryScope {
    pub schema_version: String,
    pub identity: String,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub git_ignore: String,
}

impl Default for DiscoveryScope {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA.to_owned(),
            identity: IDENTITY.to_owned(),
            includes: vec![".".to_owned()],
            excludes: Vec::new(),
            git_ignore: "respect".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputPin {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Skipped {
    pub path: String,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeReceipt {
    pub schema_version: &'static str,
    pub identity: &'static str,
    pub digest: String,
    pub source: &'static str,
    pub input: Option<InputPin>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub git_ignore: &'static str,
    pub ignore_files: Vec<InputPin>,
    pub complete: bool,
    pub limits: Vec<&'static str>,
    pub skipped: Vec<Skipped>,
    pub skipped_truncated: bool,
}

impl Default for ScopeReceipt {
    fn default() -> Self {
        Self {
            schema_version: "lekalo/discovery-scope-receipt/v0.9.0",
            identity: "dev.lekalo.discovery-scope-receipt@0.9.0",
            digest: String::new(),
            source: "builtin",
            input: None,
            includes: vec![".".to_owned()],
            excludes: Vec::new(),
            git_ignore: "respect",
            ignore_files: Vec::new(),
            complete: true,
            limits: Vec::new(),
            skipped: Vec::new(),
            skipped_truncated: false,
        }
    }
}

impl ScopeReceipt {
    pub(crate) fn limited(&mut self, reason: &'static str) {
        self.complete = false;
        if !self.limits.contains(&reason) {
            self.limits.push(reason);
        }
    }

    pub(crate) fn skip(&mut self, path: &str, reason: &'static str) {
        if self
            .skipped
            .iter()
            .any(|entry| entry.path == path && entry.reason == reason)
        {
            return;
        }
        if self.skipped.len() < MAX_SKIPS {
            self.skipped.push(Skipped {
                path: path.to_owned(),
                reason,
            });
        } else {
            self.skipped_truncated = true;
            self.limited("skip-receipt-limit");
        }
    }

    pub(crate) fn finish(mut self) -> Self {
        self.ignore_files.sort_by(|a, b| a.path.cmp(&b.path));
        self.skipped
            .sort_by(|a, b| (&a.path, a.reason).cmp(&(&b.path, b.reason)));
        self.skipped
            .dedup_by(|a, b| a.path == b.path && a.reason == b.reason);
        self.limits.sort();
        // Only policy inputs enter the policy digest. Generated skeleton
        // entries and skipped-source names cannot change the effective scope.
        self.digest = digest(
            &serde_json::to_vec(&(
                VERSION,
                self.source,
                &self.input,
                &self.includes,
                &self.excludes,
                self.git_ignore,
                &self.ignore_files,
            ))
            .expect("closed scope inputs serialize"),
        );
        self
    }
}

pub(crate) fn failure(code: &'static str, path: Option<&str>) -> DomainResult {
    let mut diagnostic = crate::loader::error::Diagnostic::new(code);
    if let Some(path) = path {
        diagnostic = diagnostic.with_path(path);
    }
    crate::loader::diagnostic::failure(Status::Denied, vec![diagnostic])
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", crate::versioning::plan::sha256_hex(bytes))
}

fn under(path: &str, prefix: &str) -> bool {
    prefix == "."
        || path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|s| s.starts_with('/'))
}

pub(crate) fn protected(path: &str) -> bool {
    path.split('/').any(|name| {
        let folded = name.to_ascii_lowercase();
        let name = folded.as_str();
        super::detect::SKIP_DIRS.contains(&name)
            || matches!(
                name,
                ".ssh" | ".aws" | ".gnupg" | ".git-credentials" | ".npmrc" | ".netrc"
            )
            || name == ".env"
            || name.starts_with(".env.")
    })
}

impl DiscoveryScope {
    pub(crate) fn included(&self, path: &str) -> bool {
        self.includes.iter().any(|root| under(path, root))
    }

    pub(crate) fn excluded(&self, path: &str) -> bool {
        self.excludes.iter().any(|root| under(path, root))
    }

    pub(crate) fn ancestor(&self, path: &str) -> bool {
        path == "." || self.includes.iter().any(|root| under(root, path))
    }

    pub(crate) fn load(
        fs: &Fs,
        root: &Path,
        input: Option<&str>,
    ) -> Result<(Self, ScopeReceipt), DomainResult> {
        let mut receipt = ScopeReceipt::default();
        let mut scope = if let Some(path) = input {
            validate_path(path, false)?;
            if protected(path) {
                return Err(failure("structure.path-escape", None));
            }
            validate_physical(fs, root, path, false, true)?;
            let (directory, name) = split(path);
            let bytes = fs
                .read_file_opt(directory, name, MAX_INPUT_BYTES)
                .map_err(|_| failure("structure.selection-unreadable", Some(path)))?
                .ok_or_else(|| failure("structure.selection-unreadable", Some(path)))?;
            receipt.source = "file";
            receipt.input = Some(InputPin {
                path: path.to_owned(),
                sha256: digest(&bytes),
            });
            serde_json::from_slice::<Self>(&bytes).map_err(|_| DomainResult::usage_error())?
        } else {
            Self::default()
        };
        if scope.schema_version != SCHEMA
            || scope.identity != IDENTITY
            || scope.git_ignore != "respect"
            || scope.includes.is_empty()
            || scope.includes.len() > 32
            || scope.excludes.len() > 64
        {
            return Err(DomainResult::usage_error());
        }
        for path in &scope.includes {
            validate_path(path, true)?;
            if protected(path) {
                return Err(failure("structure.path-escape", Some(path)));
            }
            validate_physical(fs, root, path, true, true)?;
        }
        for path in &scope.excludes {
            validate_path(path, true)?;
            // Missing exclusions are valid policy, but a present alias is
            // refused without inspecting its target or subtree.
            validate_physical(fs, root, path, false, false)?;
        }
        scope.includes.sort();
        scope.includes.dedup();
        scope.excludes.sort();
        scope.excludes.dedup();
        receipt.includes = scope.includes.clone();
        receipt.excludes = scope.excludes.clone();
        Ok((scope, receipt))
    }
}

fn validate_path(path: &str, dot: bool) -> Result<(), DomainResult> {
    if dot && path == "." {
        return Ok(());
    }
    if path.len() > 1024 {
        return Err(DomainResult::usage_error());
    }
    if let Some(code) = path_violation(path) {
        return Err(failure(code, None));
    }
    Ok(())
}

/// Discovery inputs may name hidden repository directories such as
/// `.orca` and `.bench`, and actual source-directory capitalization;
/// canonical Model paths keep their own grammar. Windows physical checks
/// still refuse case aliases rather than guessing the intended spelling.
/// Replace only a non-traversal leading dot while applying every shared
/// absolute/escape/device/length rule to the original-length spelling.
pub(crate) fn path_violation(path: &str) -> Option<&'static str> {
    let checked = path
        .split('/')
        .map(|part| {
            if part.starts_with('.') && part != "." && part != ".." {
                format!("a{}", &part[1..])
            } else {
                part.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    crate::project_fs::path_violation(&checked.to_ascii_lowercase())
}

fn validate_physical(
    fs: &Fs,
    root: &Path,
    path: &str,
    directory: bool,
    required: bool,
) -> Result<(), DomainResult> {
    #[cfg(not(windows))]
    let _ = root;
    if path == "." {
        return Ok(());
    }
    let mut parent = String::new();
    let segments: Vec<&str> = path.split('/').collect();
    for (i, name) in segments.iter().enumerate() {
        let kind = match fs.entry_type(&parent, name) {
            Ok(kind) => kind,
            Err(FsErrorKind::NotFound) if !required => return Ok(()),
            Err(_) => return Err(failure("structure.selection-unreadable", Some(path))),
        };
        if kind == EntryType::Symlink {
            return Err(failure("structure.path-link", Some(path)));
        }
        if kind == EntryType::Special {
            return Err(failure("structure.path-special", Some(path)));
        }
        if (directory || i + 1 < segments.len()) && kind != EntryType::Directory {
            return Err(failure("structure.project-not-directory", Some(path)));
        }
        if !parent.is_empty() {
            parent.push('/');
        }
        parent.push_str(name);
        #[cfg(windows)]
        {
            let actual = root
                .join(&parent)
                .canonicalize()
                .map_err(|_| failure("structure.selection-unreadable", Some(path)))?;
            if actual.file_name() != Some(std::ffi::OsStr::new(name)) {
                return Err(failure("structure.selection-alias", Some(path)));
            }
        }
    }
    Ok(())
}

pub(crate) fn split(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

pub(crate) fn ignore_file(
    fs: &Fs,
    root: &Path,
    directory: &str,
    receipt: &mut ScopeReceipt,
) -> Result<Option<Gitignore>, DomainResult> {
    let path = if directory.is_empty() {
        ".gitignore".to_owned()
    } else {
        format!("{directory}/.gitignore")
    };
    match fs.entry_type(directory, ".gitignore") {
        Err(FsErrorKind::NotFound) => return Ok(None),
        Ok(EntryType::Symlink) => return Err(failure("structure.path-link", Some(&path))),
        Ok(EntryType::File) => {}
        _ => return Err(failure("structure.selection-unreadable", Some(&path))),
    }
    if receipt.ignore_files.len() >= MAX_IGNORE_FILES {
        return Err(failure("structure.selection-unreadable", Some(&path)));
    }
    let bytes = fs
        .read_file_opt(directory, ".gitignore", MAX_INPUT_BYTES)
        .map_err(|_| failure("structure.selection-unreadable", Some(&path)))?
        .ok_or_else(|| failure("structure.selection-unreadable", Some(&path)))?;
    let text = std::str::from_utf8(&bytes).map_err(|_| DomainResult::usage_error())?;
    if text.lines().count() > MAX_IGNORE_LINES {
        return Err(DomainResult::usage_error());
    }
    let mut builder = GitignoreBuilder::new(root.join(directory));
    for line in text.lines() {
        builder
            .add_line(Some(root.join(&path)), line)
            .map_err(|_| DomainResult::usage_error())?;
    }
    let matcher = builder.build().map_err(|_| DomainResult::usage_error())?;
    receipt.ignore_files.push(InputPin {
        path,
        sha256: digest(&bytes),
    });
    Ok(Some(matcher))
}

pub(crate) fn ignored(matchers: &[Gitignore], path: &Path, directory: bool) -> bool {
    for matcher in matchers.iter().rev() {
        let matched = matcher.matched(path, directory);
        if matched.is_ignore() {
            return true;
        }
        if matched.is_whitelist() {
            return false;
        }
    }
    false
}
