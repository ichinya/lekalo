//! Read-only repository detection for `lekalo init --adopt`.
//!
//! Detection is evidence, never proof: every observation carries its
//! evidence path and a closed confidence, and nothing here executes a
//! discovered script, package manager, or adapter. The walk is bounded and
//! no-follow: depth, entries, and per-collection caps are fixed, symlinks
//! and Windows reparse points are skipped, and output order is canonical
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::scope::{self, DiscoveryScope, ScopeReceipt};
use crate::project_fs::{EntryType, Fs, FsErrorKind};
use crate::DomainResult;
use ignore::gitignore::Gitignore;
use serde_json::Value;

use super::receipt::{
    Adapter, Confidence, Detection, Evidence, GateProposal, Hint, Layout, ObservedModule,
    WorkspaceRoot,
};

/// Maximum directory depth the detection walk descends.
pub(crate) const MAX_DETECTION_DEPTH: usize = 4;
/// Maximum entries one detection walk may visit.
pub(crate) const MAX_DETECTION_ENTRIES: usize = 10_000;
/// Maximum recorded entries per closed detection collection.
pub(crate) const MAX_PER_COLLECTION: usize = 64;
/// Maximum recorded OpenAPI/schema evidence entries.
pub(crate) const MAX_OPENAPI: usize = 32;
/// Maximum recorded target adapters.
pub(crate) const MAX_ADAPTERS: usize = 32;
/// Maximum bytes of one manifest read.
pub(crate) const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;

/// Directories the detection walk never descends into: dependency and
/// build trees, plus the externally owned OpenSpec/AI Factory/HLV homes
/// and the canonical Lekalo trees themselves.
pub(crate) const SKIP_DIRS: [&str; 18] = [
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "vendor",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".idea",
    ".vscode",
    ".lekalo",
    "lekalo",
    "openspec",
    ".ai-factory",
    ".hlv",
];

/// Manifest file names and their closed ecosystem tokens.
const MANIFESTS: [(&str, &str); 15] = [
    ("package.json", "node"),
    ("Cargo.toml", "rust"),
    ("go.mod", "go"),
    ("composer.json", "php"),
    ("pyproject.toml", "python"),
    ("requirements.txt", "python"),
    ("setup.py", "python"),
    ("Gemfile", "ruby"),
    ("pom.xml", "java"),
    ("build.gradle", "java"),
    ("build.gradle.kts", "java"),
    ("settings.gradle", "java"),
    ("settings.gradle.kts", "java"),
    ("deno.json", "deno"),
    ("deno.jsonc", "deno"),
];

/// Lockfile names and their closed package-manager tokens.
const LOCKFILES: [(&str, &str); 9] = [
    ("package-lock.json", "npm"),
    ("npm-shrinkwrap.json", "npm"),
    ("yarn.lock", "yarn"),
    ("pnpm-lock.yaml", "pnpm"),
    ("pnpm-workspace.yaml", "pnpm"),
    ("bun.lockb", "bun"),
    ("bun.lock", "bun"),
    ("Cargo.lock", "cargo"),
    ("composer.lock", "composer"),
];

/// Directory names recorded as source-directory evidence.
const SOURCE_DIRS: [&str; 7] = ["src", "lib", "app", "source", "sources", "internal", "pkg"];

/// Directory names recorded as test-directory evidence.
const TEST_DIRS: [&str; 7] = [
    "test",
    "tests",
    "spec",
    "specs",
    "e2e",
    "__tests__",
    "testing",
];

/// OpenAPI/schema file stems (with either closed extension set).
const OPENAPI_STEMS: [&str; 6] = [
    "openapi.json",
    "openapi.yaml",
    "openapi.yml",
    "swagger.json",
    "swagger.yaml",
    "swagger.yml",
];

/// `package.json` dependency label hints: (dependency, label, kind).
const NODE_HINTS: [(&str, &str, HintKind); 16] = [
    ("typescript", "typescript", HintKind::Language),
    ("react", "react", HintKind::Framework),
    ("vue", "vue", HintKind::Framework),
    ("svelte", "svelte", HintKind::Framework),
    ("@angular/core", "angular", HintKind::Framework),
    ("next", "next", HintKind::Framework),
    ("nuxt", "nuxt", HintKind::Framework),
    ("express", "express", HintKind::Framework),
    ("fastify", "fastify", HintKind::Framework),
    ("@nestjs/core", "nestjs", HintKind::Framework),
    ("vite", "vite", HintKind::Framework),
    ("webpack", "webpack", HintKind::Framework),
    ("jest", "jest", HintKind::Framework),
    ("vitest", "vitest", HintKind::Framework),
    ("@playwright/test", "playwright", HintKind::Framework),
    ("cypress", "cypress", HintKind::Framework),
];

/// Whether one hint is a language or a framework.
#[derive(Clone, Copy)]
enum HintKind {
    Language,
    Framework,
}

impl HintKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Language => "language",
            Self::Framework => "framework",
        }
    }
}

/// One visited tree entry: logical project-relative POSIX path plus kind.
struct Visited {
    path: String,
    is_dir: bool,
}

/// The bounded, sorted, no-follow snapshot of one repository tree.
struct Walk {
    entries: Vec<Visited>,
    files: BTreeSet<String>,
    fs: Fs,
    root: PathBuf,
    policy: DiscoveryScope,
    scope: ScopeReceipt,
    seen: usize,
    read_bytes: Cell<usize>,
    read_failures: RefCell<Vec<String>>,
}

impl Walk {
    fn run(root: &Path, input: Option<&str>) -> Result<Walk, DomainResult> {
        let fs = Fs::open(root).map_err(|_| scope::failure("structure.root-unreadable", None))?;
        let (policy, receipt) = DiscoveryScope::load(&fs, root, input)?;
        let mut walk = Walk {
            entries: Vec::new(),
            files: BTreeSet::new(),
            fs,
            root: root.to_path_buf(),
            policy,
            scope: receipt,
            seen: 0,
            read_bytes: Cell::new(0),
            read_failures: RefCell::new(Vec::new()),
        };
        walk.visit("", 0, &mut Vec::new())?;
        Ok(walk)
    }

    fn visit(
        &mut self,
        prefix: &str,
        depth: usize,
        matchers: &mut Vec<Gitignore>,
    ) -> Result<(), DomainResult> {
        let logical = if prefix.is_empty() { "." } else { prefix };
        if self.scope.limits.contains(&"entry-limit") {
            return Ok(());
        }
        if self.policy.excluded(logical) {
            self.scope.skip(logical, "excluded");
            return Ok(());
        }
        if depth > MAX_DETECTION_DEPTH {
            self.scope.limited("depth-limit");
            self.scope.skip(logical, "depth-limit");
            return Ok(());
        }
        let pushed = if let Some(matcher) =
            scope::ignore_file(&self.fs, &self.root, prefix, &mut self.scope)?
        {
            matchers.push(matcher);
            true
        } else {
            false
        };
        let names = match self
            .fs
            .entries_bounded(prefix, MAX_DETECTION_ENTRIES.saturating_sub(self.seen))
        {
            Ok(names) => names,
            Err(FsErrorKind::Limit { .. }) => {
                self.scope.limited("entry-limit");
                self.scope.skip(logical, "entry-limit");
                if pushed {
                    matchers.pop();
                }
                return Ok(());
            }
            Err(_) => {
                self.scope.limited("directory-unreadable");
                self.scope.skip(logical, "directory-unreadable");
                if pushed {
                    matchers.pop();
                }
                return Ok(());
            }
        };
        for (name, kind) in names {
            if self.seen >= MAX_DETECTION_ENTRIES {
                self.scope.limited("entry-limit");
                self.scope.skip(logical, "entry-limit");
                break;
            }
            self.seen += 1;
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let is_dir = kind == EntryType::Directory;
            let selected = self.policy.included(&path);
            let excluded = self.policy.excluded(&path);
            if scope::protected(&path) {
                // Record the policy reason without publishing sensitive
                // filenames or names inside protected homes.
                self.scope.skip(".", "protected");
                // Preserve the pre-existing closed owner-layout metadata
                // hint. These roots never grant a source/content read.
                if prefix.is_empty()
                    && is_dir
                    && selected
                    && !excluded
                    && matches!(name.as_str(), "openspec" | ".ai-factory" | ".hlv")
                    && !scope::ignored(matchers, &self.root.join(&path), true)
                {
                    self.entries.push(Visited { path, is_dir });
                }
                continue;
            }
            if excluded {
                self.scope.skip(&path, "excluded");
                continue;
            }
            if !(selected || is_dir && self.policy.ancestor(&path)) {
                self.scope.skip(&path, "outside-includes");
                continue;
            }
            if scope::ignored(matchers, &self.root.join(&path), is_dir) {
                self.scope.skip(&path, "gitignored");
                continue;
            }
            if kind == EntryType::Symlink || kind == EntryType::Special {
                self.scope.limited("physical-entry-refused");
                self.scope.skip(&path, "physical-entry-refused");
                continue;
            }
            // Manifest basenames have established uppercase spellings;
            // directory roots still use the shared portable grammar.
            if is_dir && scope::path_violation(&path).is_some() {
                self.scope.limited("non-portable-directory");
                self.scope.skip(&path, "non-portable-directory");
                continue;
            }
            if selected {
                if !is_dir {
                    self.files.insert(path.clone());
                }
                self.entries.push(Visited {
                    path: path.clone(),
                    is_dir,
                });
            }
            if is_dir {
                self.visit(&path, depth + 1, matchers)?;
                if self.scope.limits.contains(&"entry-limit") {
                    break;
                }
            }
        }
        if pushed {
            matchers.pop();
        }
        Ok(())
    }

    fn has_file(&self, path: &str) -> bool {
        self.files.contains(path)
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        if !self.has_file(path) {
            return None;
        }
        let remaining = (32usize * 1024 * 1024).saturating_sub(self.read_bytes.get());
        let (directory, name) = scope::split(path);
        match self
            .fs
            .read_file_opt(directory, name, remaining.min(MAX_MANIFEST_BYTES as usize))
        {
            Ok(Some(bytes)) => {
                self.read_bytes.set(self.read_bytes.get() + bytes.len());
                Some(bytes)
            }
            _ => {
                self.read_failures.borrow_mut().push(path.to_owned());
                None
            }
        }
    }

    fn json(&self, path: &str) -> Option<Value> {
        let bytes = self.read(path)?;
        match serde_json::from_slice(&bytes) {
            Ok(value) => Some(value),
            Err(_) => {
                self.read_failures.borrow_mut().push(path.to_owned());
                None
            }
        }
    }

    fn finish_scope(mut self) -> ScopeReceipt {
        for path in self.read_failures.into_inner() {
            self.scope.limited("manifest-read-or-parse-refused");
            self.scope.skip(&path, "manifest-read-or-parse-refused");
        }
        self.scope
    }
}

/// Windows reparse-point check: every reparse attribute is skipped.
fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x0400 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

/// Read a bounded file; `None` on absence, oversize, or read failure.
fn read_bounded(path: &Path) -> Option<Vec<u8>> {
    let fs = Fs::open(path.parent()?).ok()?;
    fs.read_file_opt("", path.file_name()?.to_str()?, MAX_MANIFEST_BYTES as usize)
        .ok()?
}

/// Parse a bounded JSON manifest.
fn parse_json(path: &Path) -> Option<Value> {
    let bytes = read_bounded(path)?;
    serde_json::from_slice(&bytes).ok()
}

/// The closed workspace-root kinds detection recognizes.
pub(crate) enum WorkspaceKind {
    Node,
    Go,
    Rust,
    Gradle,
}

/// Whether `dir` declares a workspace root, and of which ecosystem.
pub(crate) fn workspace_marker(dir: &Path) -> Option<WorkspaceKind> {
    let package = dir.join("package.json");
    if package.is_file() {
        if let Some(value) = parse_json(&package) {
            if node_workspace_members(&value).is_some() {
                return Some(WorkspaceKind::Node);
            }
        }
    }
    if dir.join("pnpm-workspace.yaml").is_file() {
        if let Some(members) = pnpm_workspace_members(&dir.join("pnpm-workspace.yaml")) {
            if !members.is_empty() {
                return Some(WorkspaceKind::Node);
            }
        }
    }
    if dir.join("go.work").is_file() {
        return Some(WorkspaceKind::Go);
    }
    let cargo = dir.join("Cargo.toml");
    if cargo.is_file() {
        if let Some(bytes) = read_bounded(&cargo) {
            if has_cargo_workspace(&bytes) {
                return Some(WorkspaceKind::Rust);
            }
        }
    }
    if dir.join("settings.gradle").is_file() || dir.join("settings.gradle.kts").is_file() {
        return Some(WorkspaceKind::Gradle);
    }
    None
}

/// Whether any closed ecosystem manifest exists directly in `dir`.
pub(crate) fn any_manifest(dir: &Path) -> bool {
    MANIFESTS.iter().any(|(name, _)| dir.join(name).is_file())
}

/// The `workspaces` member globs of one parsed `package.json`.
fn node_workspace_members(value: &Value) -> Option<Vec<String>> {
    let field = value.get("workspaces")?;
    let items = match field {
        Value::Array(items) => items.as_slice(),
        Value::Object(map) => map.get("packages")?.as_array()?.as_slice(),
        _ => return None,
    };
    Some(
        items
            .iter()
            .filter_map(Value::as_str)
            .filter(|glob| !glob.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

/// The `packages` member globs of one `pnpm-workspace.yaml`.
fn pnpm_workspace_members(path: &Path) -> Option<Vec<String>> {
    let bytes = read_bounded(path)?;
    pnpm_workspace_members_bytes(&bytes)
}

fn pnpm_workspace_members_bytes(bytes: &[u8]) -> Option<Vec<String>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let index = crate::loader::source::LineIndex::new(text);
    let parsed = crate::loader::frontends::parse_document(text, &index).ok()?;
    let root = match parsed {
        crate::loader::frontends::Parsed::Root(node) => node,
        crate::loader::frontends::Parsed::Empty => return Some(Vec::new()),
    };
    let packages = root.get("packages")?.as_seq()?;
    Some(
        packages
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
    )
}

/// Whether Cargo.toml bytes declare a `[workspace]` section.
fn has_cargo_workspace(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes)
        .map(|text| {
            text.lines().any(|line| {
                let line = line.trim();
                line == "[workspace]" || line.starts_with("[workspace.")
            })
        })
        .unwrap_or(false)
}

/// The `use (...)` member directories of one `go.work`.
fn go_work_members(bytes: &[u8]) -> Vec<String> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Vec::new();
    };
    let mut members = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if line == "use (" {
            inside = true;
            continue;
        }
        if inside && line == ")" {
            break;
        }
        if inside {
            if let Some(member) = line.strip_prefix("./") {
                members.push(member.to_owned());
            } else if !line.is_empty() {
                members.push(line.to_owned());
            }
            if members.len() >= MAX_PER_COLLECTION {
                break;
            }
        }
    }
    members
}

/// The `[workspace] members` of one Cargo.toml (line-scan evidence).
fn cargo_workspace_members(bytes: &[u8]) -> Vec<String> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Vec::new();
    };
    let mut members = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if !inside {
            inside = line == "members = [" || line.starts_with("members = [");
            let inline = line.trim_start_matches("members = [").trim_end_matches(']');
            if inside {
                for part in inline.split(',') {
                    push_member(&mut members, part);
                }
                if line.ends_with(']') {
                    break;
                }
                continue;
            }
            continue;
        }
        if line.starts_with(']') || line.ends_with(']') {
            push_member(&mut members, line.trim_end_matches(']'));
            break;
        }
        push_member(&mut members, line);
        if members.len() >= MAX_PER_COLLECTION {
            break;
        }
    }
    members
}

/// Push one quoted member entry if it is non-empty.
fn push_member(members: &mut Vec<String>, raw: &str) {
    let raw = raw
        .trim()
        .trim_end_matches(',')
        .trim()
        .trim_matches('"')
        .trim_matches('\'');
    if !raw.is_empty() && members.len() < MAX_PER_COLLECTION {
        members.push(raw.to_owned());
    }
}

/// The `[package] name` of one Cargo.toml (line-scan evidence).
pub(crate) fn cargo_package_name(path: &Path) -> Option<String> {
    let bytes = read_bounded(path)?;
    cargo_package_name_bytes(&bytes)
}

fn cargo_package_name_bytes(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = trimmed == "[package]";
            continue;
        }
        if inside {
            if let Some(rest) = trimmed.strip_prefix("name") {
                let rest = rest.trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    let value = rest.trim().trim_matches('"');
                    if !value.is_empty() {
                        return Some(value.to_owned());
                    }
                }
            }
        }
    }
    None
}

/// The npm/composer package name of one JSON manifest.
pub(crate) fn json_name(path: &Path) -> Option<String> {
    parse_json(path)?.get("name")?.as_str().map(str::to_owned)
}

/// The `module` path of one go.mod.
pub(crate) fn go_module_path(path: &Path) -> Option<String> {
    let bytes = read_bounded(path)?;
    go_module_path_bytes(&bytes)
}

fn go_module_path_bytes(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("module ") {
            return Some(rest.trim().to_owned());
        }
    }
    None
}

/// The declared script names of one parsed `package.json`.
fn node_scripts(value: &Value) -> Vec<String> {
    value
        .get("scripts")
        .and_then(Value::as_object)
        .map(|scripts| scripts.keys().cloned().collect())
        .unwrap_or_default()
}

/// Run the full closed detection pass over one adopted root.
///
/// `requested_target` (when `--target` was passed) is always reported, so
/// a missing adapter is visible evidence rather than a silent guess.
pub(crate) fn detect(
    root: &Path,
    requested_target: Option<&str>,
    input: Option<&str>,
) -> Result<Detection, DomainResult> {
    let mut detection = Detection::default();
    let walk = Walk::run(root, input)?;

    // Layouts: externally owned homes at the adopted root.
    for (kind, name, evidence) in [
        ("openspec", "openspec", "openspec/specs"),
        ("openspec", "openspec", "openspec/changes"),
        ("ai-factory", ".ai-factory", ".ai-factory"),
        ("hlv", ".hlv", ".hlv"),
    ] {
        if detection.layouts.len() >= 3 {
            break;
        }
        if !walk
            .entries
            .iter()
            .any(|entry| entry.path == name && entry.is_dir)
            || detection.layouts.iter().any(|item| item.path == name)
        {
            continue;
        }
        let (_, leaf) = scope::split(evidence);
        let confidence = if evidence == name
            || matches!(walk.fs.entry_type(name, leaf), Ok(EntryType::Directory))
        {
            Confidence::High
        } else {
            Confidence::Medium
        };
        detection.layouts.push(Layout {
            kind,
            path: name.to_owned(),
            confidence,
        });
    }

    let mut node_manifests: Vec<(String, Value)> = Vec::new();
    let mut composer_manifests: Vec<(String, Value)> = Vec::new();
    let mut artisan_paths = BTreeSet::new();

    for entry in &walk.entries {
        if entry.is_dir {
            let name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
            if detection.source_dirs.len() < MAX_PER_COLLECTION
                && SOURCE_DIRS.contains(&name)
                && !detection.source_dirs.contains(&entry.path)
            {
                detection.source_dirs.push(entry.path.clone());
            }
            if detection.test_dirs.len() < MAX_PER_COLLECTION
                && TEST_DIRS.contains(&name)
                && !detection.test_dirs.contains(&entry.path)
            {
                detection.test_dirs.push(entry.path.clone());
            }
            continue;
        }
        let file = Path::new(&entry.path);
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let Some(name) = name else { continue };
        if let Some((_, ecosystem)) = MANIFESTS.iter().find(|(stem, _)| *stem == name) {
            if detection.manifests.len() < MAX_PER_COLLECTION {
                detection.manifests.push(Evidence {
                    path: entry.path.clone(),
                    detail: (*ecosystem).to_owned(),
                });
            }
        }
        if let Some((_, manager)) = LOCKFILES.iter().find(|(stem, _)| *stem == name) {
            if detection.package_managers.len() < MAX_PER_COLLECTION {
                detection.package_managers.push(Evidence {
                    path: entry.path.clone(),
                    detail: (*manager).to_owned(),
                });
            }
        }
        if name == "package.json" {
            if let Some(value) = walk.json(&entry.path) {
                scan_node_manifest(&entry.path, &value, &mut detection);
                if node_manifests.len() < MAX_PER_COLLECTION {
                    node_manifests.push((entry.path.clone(), value));
                }
            }
        }
        if name == "composer.json" {
            if let Some(value) = walk.json(&entry.path) {
                scan_composer_manifest(&entry.path, &value, &mut detection);
                if composer_manifests.len() < MAX_PER_COLLECTION {
                    composer_manifests.push((entry.path.clone(), value));
                }
            }
        }
        if name == "artisan" {
            artisan_paths.insert(entry.path.clone());
        }
        if name == "tsconfig.json" && detection.languages.len() < MAX_PER_COLLECTION {
            push_hint(
                &mut detection.languages,
                Hint {
                    label: "typescript".to_owned(),
                    kind: HintKind::Language.as_str(),
                    path: entry.path.clone(),
                    field: None,
                    confidence: Confidence::Medium,
                },
            );
        }
        if name == "go.mod" {
            push_hint(
                &mut detection.languages,
                Hint {
                    label: "go".to_owned(),
                    kind: HintKind::Language.as_str(),
                    path: entry.path.clone(),
                    field: None,
                    confidence: Confidence::High,
                },
            );
        }
        if name == "Cargo.toml" {
            push_hint(
                &mut detection.languages,
                Hint {
                    label: "rust".to_owned(),
                    kind: HintKind::Language.as_str(),
                    path: entry.path.clone(),
                    field: None,
                    confidence: Confidence::High,
                },
            );
        }
        if name == "Gemfile" {
            push_hint(
                &mut detection.languages,
                Hint {
                    label: "ruby".to_owned(),
                    kind: HintKind::Language.as_str(),
                    path: entry.path.clone(),
                    field: None,
                    confidence: Confidence::Medium,
                },
            );
        }
        if detection.openapi.len() < MAX_OPENAPI
            && (OPENAPI_STEMS.contains(&name.as_str())
                || (name.ends_with(".yaml") || name.ends_with(".yml") || name.ends_with(".json"))
                    && (name.ends_with(".openapi.yaml")
                        || name.ends_with(".openapi.yml")
                        || name.ends_with(".openapi.json")))
        {
            detection.openapi.push(Evidence {
                path: entry.path.clone(),
                detail: "openapi-schema".to_owned(),
            });
        }
    }

    // Workspace roots at the adopted root itself, then anywhere inside
    // the walked tree, plus their members.
    let mut marked: Vec<(String, WorkspaceKind)> = Vec::new();
    if let Some(kind) = workspace_marker_in(&walk, ".") {
        marked.push((".".to_owned(), kind));
    }
    for entry in &walk.entries {
        if !entry.is_dir {
            continue;
        }
        if let Some(kind) = workspace_marker_in(&walk, &entry.path) {
            marked.push((entry.path.clone(), kind));
        }
    }
    for (path, kind) in marked {
        if detection.workspace_roots.len() >= MAX_PER_COLLECTION {
            break;
        }
        let (ecosystem, members) = match kind {
            WorkspaceKind::Node => {
                let manifest = child_path(&path, "package.json");
                let members = walk
                    .json(&manifest)
                    .and_then(|value| node_workspace_members(&value))
                    .or_else(|| {
                        walk.read(&child_path(&path, "pnpm-workspace.yaml"))
                            .and_then(|bytes| pnpm_workspace_members_bytes(&bytes))
                    })
                    .unwrap_or_default();
                ("node", members)
            }
            WorkspaceKind::Go => (
                "go",
                walk.read(&child_path(&path, "go.work"))
                    .map(|bytes| go_work_members(&bytes))
                    .unwrap_or_default(),
            ),
            WorkspaceKind::Rust => (
                "rust",
                walk.read(&child_path(&path, "Cargo.toml"))
                    .map(|bytes| cargo_workspace_members(&bytes))
                    .unwrap_or_default(),
            ),
            WorkspaceKind::Gradle => ("gradle", Vec::new()),
        };
        detection.workspace_roots.push(WorkspaceRoot {
            path,
            ecosystem,
            members,
            confidence: Confidence::High,
        });
    }

    // Monorepo tooling evidence without workspace semantics.
    for entry in &walk.entries {
        if entry.is_dir {
            continue;
        }
        if matches!(entry.path.as_str(), "lerna.json" | "turbo.json" | "nx.json") {
            detection.monorepo_tools.push(Evidence {
                path: entry.path.clone(),
                detail: "monorepo-tooling".to_owned(),
            });
        }
    }

    // Observed modules: workspace members carrying a manifest.
    for workspace in &detection.workspace_roots {
        for glob in &workspace.members {
            for module in expand_member_glob(&walk, workspace, glob) {
                push_module(&mut detection.modules, module);
            }
        }
    }
    if detection.modules.is_empty() {
        for (path, value) in &node_manifests {
            if path.split('/').count() == 1 || detection.workspace_roots.is_empty() {
                if let Some(name) = value.get("name").and_then(Value::as_str) {
                    push_module(
                        &mut detection.modules,
                        ObservedModule {
                            name: name.to_owned(),
                            path: path
                                .rsplit_once('/')
                                .map(|(directory, _)| directory)
                                .unwrap_or(".")
                                .to_owned(),
                            mode: "observed",
                            evidence: vec![path.clone()],
                            confidence: Confidence::High,
                        },
                    );
                }
                if !detection.workspace_roots.is_empty() {
                    break;
                }
            }
        }
    }

    // Native gates: package.json scripts verbatim; ecosystem conventions.
    if input.is_some() {
        for (path, value) in &composer_manifests {
            if let Some(name) = value.get("name").and_then(Value::as_str) {
                let directory = path
                    .rsplit_once('/')
                    .map(|(directory, _)| directory)
                    .unwrap_or(".");
                push_module(
                    &mut detection.modules,
                    ObservedModule {
                        name: name.to_owned(),
                        path: directory.to_owned(),
                        mode: "observed",
                        evidence: vec![path.clone()],
                        confidence: Confidence::High,
                    },
                );
            }
        }
    }
    let node_manager = detection
        .package_managers
        .iter()
        .map(|evidence| evidence.detail.as_str())
        .find(|manager| matches!(*manager, "npm" | "yarn" | "pnpm" | "bun"))
        .unwrap_or("npm")
        .to_owned();
    for (path, value) in &node_manifests {
        let runner = if *path == "package.json" {
            node_manager.as_str()
        } else {
            "npm"
        };
        for script in node_scripts(value) {
            if let Some(gate) = gate_name(&script) {
                detection.gates.push(GateProposal {
                    gate,
                    command: format!("{runner} run {script}"),
                    source: "workflow",
                    evidence: format!("{path}#scripts.{script}"),
                    confidence: Confidence::High,
                });
            }
        }
        if *path == "package.json" {
            break;
        }
    }
    for (path, _) in &composer_manifests {
        let directory = path
            .rsplit_once('/')
            .map(|(directory, _)| directory)
            .unwrap_or(".");
        let artisan = child_path(directory, "artisan");
        if artisan_paths.contains(&artisan) {
            detection.gates.push(GateProposal {
                gate: "test",
                command: "php artisan test".to_owned(),
                source: "derived",
                evidence: artisan,
                confidence: Confidence::Medium,
            });
        }
    }
    for manifest in &detection.manifests {
        let commands = match manifest.path.rsplit('/').next() {
            Some("go.mod") => Some([("build", "go build ./..."), ("test", "go test ./...")]),
            Some("Cargo.toml") => Some([("build", "cargo build"), ("test", "cargo test")]),
            _ => None,
        };
        if let Some(commands) = commands {
            for (gate, command) in commands {
                detection.gates.push(GateProposal {
                    gate,
                    command: command.to_owned(),
                    source: "derived",
                    evidence: manifest.path.clone(),
                    confidence: Confidence::Medium,
                });
            }
        }
    }

    // Installed target adapters: a bounded PATH lookup, never an execution.
    let installed = installed_adapters(root, &walk);
    let mut probed: Vec<String> = installed.clone();
    if let Some(target) = requested_target {
        if !probed.iter().any(|candidate| candidate == target) {
            probed.push(target.to_owned());
        }
    }
    probed.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    for target in probed.into_iter().take(MAX_ADAPTERS) {
        let is_installed = installed.contains(&target);
        detection.adapters.push(Adapter {
            target: target.clone(),
            installed: is_installed,
            evidence: "PATH",
            confidence: if is_installed {
                Confidence::High
            } else {
                Confidence::Medium
            },
        });
    }

    detection
        .manifests
        .sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    detection
        .package_managers
        .sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    detection
        .languages
        .sort_by(|left, right| (&left.label, &left.path).cmp(&(&right.label, &right.path)));
    detection
        .workspace_roots
        .sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    detection
        .monorepo_tools
        .sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    detection
        .source_dirs
        .sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    detection
        .test_dirs
        .sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    detection
        .openapi
        .sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    detection
        .layouts
        .sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    detection
        .gates
        .sort_by(|left, right| (&left.gate, &left.command).cmp(&(&right.gate, &right.command)));
    detection
        .adapters
        .sort_by(|left, right| left.target.as_bytes().cmp(right.target.as_bytes()));
    detection
        .modules
        .sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    detection.scope = walk.finish_scope();
    if detection.manifests.len() >= MAX_PER_COLLECTION
        || detection.modules.len() >= MAX_PER_COLLECTION
        || detection.languages.len() >= MAX_PER_COLLECTION
        || detection.openapi.len() >= MAX_OPENAPI
        || detection.gates.len() > MAX_PER_COLLECTION
    {
        detection.scope.limited("collection-limit");
    }
    detection.gates.truncate(MAX_PER_COLLECTION);
    detection.scope = detection.scope.finish();
    Ok(detection)
}

fn child_path(directory: &str, name: &str) -> String {
    if directory == "." {
        name.to_owned()
    } else {
        format!("{directory}/{name}")
    }
}

/// Workspace detection uses only admitted catalog entries, including for
/// roots nested below another workspace. No membership glob can open a
/// new directory or manifest outside the scoped walk.
fn workspace_marker_in(walk: &Walk, directory: &str) -> Option<WorkspaceKind> {
    if walk
        .json(&child_path(directory, "package.json"))
        .as_ref()
        .and_then(node_workspace_members)
        .is_some()
        || walk
            .read(&child_path(directory, "pnpm-workspace.yaml"))
            .and_then(|bytes| pnpm_workspace_members_bytes(&bytes))
            .is_some_and(|members| !members.is_empty())
    {
        return Some(WorkspaceKind::Node);
    }
    if walk.has_file(&child_path(directory, "go.work")) {
        return Some(WorkspaceKind::Go);
    }
    if walk
        .read(&child_path(directory, "Cargo.toml"))
        .is_some_and(|bytes| has_cargo_workspace(&bytes))
    {
        return Some(WorkspaceKind::Rust);
    }
    if ["settings.gradle", "settings.gradle.kts"]
        .iter()
        .any(|name| walk.has_file(&child_path(directory, name)))
    {
        return Some(WorkspaceKind::Gradle);
    }
    None
}

/// Insert one hint, keeping collections bounded and duplicate-free.
fn push_hint(hints: &mut Vec<Hint>, hint: Hint) {
    if hints.len() >= MAX_PER_COLLECTION {
        return;
    }
    if hints
        .iter()
        .any(|item| item.label == hint.label && item.path == hint.path)
    {
        return;
    }
    hints.push(hint);
}

/// Insert one observed module, deduplicated by module path.
fn push_module(modules: &mut Vec<ObservedModule>, module: ObservedModule) {
    if modules.len() >= MAX_PER_COLLECTION || modules.iter().any(|item| item.path == module.path) {
        return;
    }
    modules.push(module);
}

/// Expand one workspace member glob to the observed modules it names.
///
/// Only the closed `a/b/*` shape (literal segments plus one trailing or
/// interior `*` level) is expanded, against on-disk directory entries.
fn expand_member_glob(walk: &Walk, workspace: &WorkspaceRoot, glob: &str) -> Vec<ObservedModule> {
    let segments: Vec<&str> = glob.split('/').collect();
    if segments.is_empty()
        || segments.len() > 3
        || segments
            .iter()
            .any(|part| *part != "*" && scope::path_violation(part).is_some())
    {
        return Vec::new();
    }
    let base = if workspace.path == "." {
        String::new()
    } else {
        format!("{}/", workspace.path)
    };
    let mut modules = Vec::new();
    for entry in walk.entries.iter().filter(|entry| entry.is_dir) {
        let Some(relative) = entry.path.strip_prefix(&base) else {
            continue;
        };
        let parts: Vec<&str> = relative.split('/').collect();
        if parts.len() != segments.len()
            || !parts
                .iter()
                .zip(&segments)
                .all(|(part, pattern)| *pattern == "*" || part == pattern)
        {
            continue;
        }
        let Some(manifest) = [
            "package.json",
            "composer.json",
            "go.mod",
            "Cargo.toml",
            "pyproject.toml",
        ]
        .into_iter()
        .map(|name| child_path(&entry.path, name))
        .find(|path| walk.has_file(path)) else {
            continue;
        };
        let name = if manifest.ends_with("package.json") || manifest.ends_with("composer.json") {
            walk.json(&manifest)
                .and_then(|value| value.get("name").and_then(Value::as_str).map(str::to_owned))
        } else if manifest.ends_with("Cargo.toml") {
            walk.read(&manifest)
                .and_then(|bytes| cargo_package_name_bytes(&bytes))
        } else if manifest.ends_with("go.mod") {
            walk.read(&manifest)
                .and_then(|bytes| go_module_path_bytes(&bytes))
                .map(|module| module.rsplit('/').next().unwrap_or(&module).to_owned())
        } else {
            None
        };
        modules.push(ObservedModule {
            name: name.unwrap_or_else(|| relative.to_owned()),
            path: entry.path.clone(),
            mode: "observed",
            evidence: vec![manifest],
            confidence: Confidence::High,
        });
    }
    modules
}

/// The closed workflow gate names adopt recognizes.
fn gate_name(script: &str) -> Option<&'static str> {
    match script {
        "build" | "compile" => Some("build"),
        "test" => Some("test"),
        "lint" => Some("lint"),
        "check" | "ci" => Some("check"),
        _ => None,
    }
}

/// Apply the closed node dependency hint table to one manifest.
fn scan_node_manifest(path: &str, value: &Value, detection: &mut Detection) {
    for field in ["dependencies", "devDependencies", "peerDependencies"] {
        let Some(map) = value.get(field).and_then(Value::as_object) else {
            continue;
        };
        for (dependency, _) in map {
            if let Some((_, label, kind)) =
                NODE_HINTS.iter().find(|(name, _, _)| name == dependency)
            {
                push_hint(
                    &mut detection.languages,
                    Hint {
                        label: (*label).to_owned(),
                        kind: kind.as_str(),
                        path: path.to_owned(),
                        field: Some(field.to_owned()),
                        confidence: Confidence::High,
                    },
                );
            }
        }
    }
}

/// Apply the closed composer hint table to one manifest.
fn scan_composer_manifest(path: &str, value: &Value, detection: &mut Detection) {
    for field in ["require", "require-dev"] {
        let Some(map) = value.get(field).and_then(Value::as_object) else {
            continue;
        };
        for (dependency, _) in map {
            let framework = match dependency.as_str() {
                "laravel/framework" => Some("laravel"),
                "symfony/framework-bundle" => Some("symfony"),
                _ => None,
            };
            if let Some(framework) = framework {
                push_hint(
                    &mut detection.languages,
                    Hint {
                        label: framework.to_owned(),
                        kind: HintKind::Framework.as_str(),
                        path: path.to_owned(),
                        field: Some(field.to_owned()),
                        confidence: Confidence::High,
                    },
                );
            }
        }
    }
    if let Some(scripts) = value.get("scripts").and_then(Value::as_object) {
        for script in scripts.keys() {
            if let Some(gate) = gate_name(script) {
                detection.gates.push(GateProposal {
                    gate,
                    command: format!("composer {script}"),
                    source: "workflow",
                    evidence: format!("{path}#scripts.{script}"),
                    confidence: Confidence::High,
                });
            }
        }
    }
}

/// The `lekalo-target-<id>` executables available on `PATH`.
fn installed_adapters(root: &Path, walk: &Walk) -> Vec<String> {
    let Some(path_var) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let mut found: Vec<String> = Vec::new();
    let physical_root = root.canonicalize().ok();
    'dirs: for dir in std::env::split_paths(&path_var) {
        // PATH is an independent environment probe. A directory inside
        // this repository still cannot bypass its discovery scope.
        if let (Some(root), Ok(directory)) = (&physical_root, dir.canonicalize()) {
            if let Ok(relative) = directory.strip_prefix(root) {
                let logical = relative.to_string_lossy().replace('\\', "/");
                if logical.is_empty() {
                    if !walk.policy.included(".") || walk.policy.excluded(".") {
                        continue;
                    }
                } else if scope::protected(&logical)
                    || !walk
                        .entries
                        .iter()
                        .any(|entry| entry.is_dir && entry.path == logical)
                {
                    continue;
                }
            }
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|entry| entry.ok()) {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_file() || is_reparse(&metadata) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".exe").unwrap_or(&name);
            if let Some(target) = stem.strip_prefix("lekalo-target-") {
                if valid_target_id(target) && !found.iter().any(|item| item == target) {
                    found.push(target.to_owned());
                    if found.len() >= MAX_ADAPTERS {
                        break 'dirs;
                    }
                }
            }
        }
    }
    found.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    found
}

/// The closed target-id grammar of `lekalo/targets/<id>.yaml`.
pub fn valid_target_id(target: &str) -> bool {
    let mut chars = target.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    target.len() <= 63
        && target.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'
                || character == '-'
        })
}
