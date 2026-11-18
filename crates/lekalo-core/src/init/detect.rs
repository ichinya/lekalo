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

// Legacy technologies not yet migrated to discovery services.
const OTHER_MANIFESTS: &[(&str, &str)] = &[
    ("Gemfile", "ruby"),
    ("pom.xml", "java"),
    ("build.gradle", "java"),
    ("build.gradle.kts", "java"),
    ("settings.gradle", "java"),
    ("settings.gradle.kts", "java"),
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

/// Root selection uses the same external discovery API as adoption.
pub(crate) fn probe(
    dir: &Path,
    providers: &DiscoveryProviders,
) -> Result<(bool, bool), DomainResult> {
    let fs = Fs::open(dir).map_err(|_| scope::failure("structure.root-unreadable", None))?;
    let names: Vec<String> = providers
        .filenames()
        .into_iter()
        .filter(|name| matches!(fs.entry_type("", name), Ok(EntryType::File)))
        .collect();
    let data = providers.inspect(&names, |path| {
        fs.read_file_opt("", path, MAX_MANIFEST_BYTES as usize)
            .ok()
            .flatten()
    })?;
    let workspace = !data.workspaces.is_empty()
        || ["settings.gradle", "settings.gradle.kts"]
            .iter()
            .any(|name| matches!(fs.entry_type("", name), Ok(EntryType::File)));
    let manifest = !data.manifests.is_empty()
        || OTHER_MANIFESTS
            .iter()
            .any(|(name, _)| matches!(fs.entry_type("", name), Ok(EntryType::File)));
    Ok((workspace, manifest))
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
    let providers = DiscoveryProviders::load(root)?;
    let data = providers.inspect(&walk.files.iter().cloned().collect::<Vec<_>>(), |path| {
        walk.read(path)
    })?;
    for path in &data.refused {
        walk.read_failures.borrow_mut().push(path.clone());
    }
    let module_evidence = data.modules.clone();
    data.apply(&mut detection);

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
        if let Some((_, ecosystem)) = OTHER_MANIFESTS.iter().find(|(stem, _)| *stem == name) {
            if detection.manifests.len() < MAX_PER_COLLECTION {
                detection.manifests.push(Evidence {
                    path: entry.path.clone(),
                    detail: (*ecosystem).to_owned(),
                });
            }
        }
        if name == "Gemfile" {
            push_hint(
                &mut detection.languages,
                Hint {
                    label: "ruby".to_owned(),
                    kind: "language",
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

    for directory in std::iter::once(".").chain(
        walk.entries
            .iter()
            .filter(|e| e.is_dir)
            .map(|e| e.path.as_str()),
    ) {
        if ["settings.gradle", "settings.gradle.kts"]
            .iter()
            .any(|name| walk.has_file(&child_path(directory, name)))
        {
            detection.workspace_roots.push(WorkspaceRoot {
                path: directory.into(),
                ecosystem: "gradle".into(),
                members: Vec::new(),
                confidence: Confidence::High,
            });
        }
    }
    // Observed modules: workspace members carrying a manifest.
    for workspace in &detection.workspace_roots {
        for glob in &workspace.members {
            for module in expand_member_glob(&walk, workspace, glob, &module_evidence) {
                push_module(&mut detection.modules, module);
            }
        }
    }
    if detection.modules.is_empty() {
        for module in &module_evidence {
            if module.standalone && (module.path == "." || detection.workspace_roots.is_empty()) {
                if let Some(name) = &module.name {
                    push_module(
                        &mut detection.modules,
                        ObservedModule {
                            name: name.clone(),
                            path: module.path.clone(),
                            mode: "observed",
                            evidence: vec![module.manifest.clone()],
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
    if input.is_some() {
        for module in module_evidence.iter().filter(|m| m.observe_when_scoped) {
            if let Some(name) = &module.name {
                push_module(
                    &mut detection.modules,
                    ObservedModule {
                        name: name.clone(),
                        path: module.path.clone(),
                        mode: "observed",
                        evidence: vec![module.manifest.clone()],
                        confidence: Confidence::High,
                    },
                );
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
    if [
        detection.package_managers.len(),
        detection.workspace_roots.len(),
        detection.monorepo_tools.len(),
    ]
    .iter()
    .any(|n| *n >= MAX_PER_COLLECTION)
    {
        detection.scope.limited("collection-limit");
    }
    detection.manifests.truncate(MAX_PER_COLLECTION);
    detection.package_managers.truncate(MAX_PER_COLLECTION);
    detection.workspace_roots.truncate(MAX_PER_COLLECTION);
    detection.monorepo_tools.truncate(MAX_PER_COLLECTION);
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
fn expand_member_glob(
    walk: &Walk,
    workspace: &WorkspaceRoot,
    glob: &str,
    evidence: &[ModuleEvidence],
) -> Vec<ObservedModule> {
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
        let selected = evidence
            .iter()
            .filter(|m| m.path == entry.path)
            .min_by_key(|m| m.priority);
        let Some(selected) = selected else {
            continue;
        };
        let (manifest, name) = (selected.manifest.clone(), selected.name.clone());
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

use crate::runtime::{self, services::Service};
use serde::Deserialize;
use serde_json::json;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiscoveryDescriptor {
    schema: String,
    files: Vec<String>,
    reads: Vec<String>,
}
pub(crate) struct DiscoveryProviders(Vec<(Service, DiscoveryDescriptor)>);
impl DiscoveryProviders {
    pub(crate) fn load(root: &Path) -> Result<Self, DomainResult> {
        Self::load_inner(root).map_err(|e| runtime::outcome(Err(e)))
    }
    /// Root inference uses the nearest selected discovery services. Keep
    /// their session and lock rooted at the declaring directory while
    /// probing ancestors; an explicit embedding override remains authoritative.
    pub(crate) fn load_for_ancestors(root: &Path) -> Result<Self, DomainResult> {
        if std::env::var("LEKALO_SERVICES").is_ok() {
            return Self::load(root);
        }
        for candidate in root.ancestors().take(crate::project_fs::MAX_WALK_DEPTH) {
            let providers = Self::load(candidate)?;
            if !providers.0.is_empty() {
                return Ok(providers);
            }
        }
        Ok(Self(Vec::new()))
    }
    fn load_inner(root: &Path) -> runtime::Result<Self> {
        let mut providers = Vec::new();
        for provider in runtime::services::selected(root, "discovery")? {
            let descriptor: DiscoveryDescriptor = serde_json::from_value(provider.invoke(
                "discovery.describe",
                json!({"schema":"lekalo/discovery/v1"}),
            )?)
            .map_err(|_| runtime::error("discovery-descriptor"))?;
            if descriptor.schema != "lekalo/discovery/v1"
                || descriptor.files.len() > 64
                || descriptor.reads.len() > 64
                || descriptor.files.iter().any(|p| {
                    p.contains('/') || scope::protected(p) || scope::path_violation(p).is_some()
                })
                || descriptor
                    .reads
                    .iter()
                    .any(|p| !descriptor.files.contains(p))
            {
                return Err(runtime::error("discovery-descriptor"));
            }
            providers.push((provider, descriptor));
        }
        Ok(Self(providers))
    }
    fn filenames(&self) -> BTreeSet<String> {
        self.0.iter().flat_map(|(_, d)| d.files.clone()).collect()
    }
    fn inspect(
        &self,
        paths: &[String],
        read: impl Fn(&str) -> Option<Vec<u8>>,
    ) -> Result<DiscoveryData, DomainResult> {
        self.inspect_inner(paths, read)
            .map_err(|e| runtime::outcome(Err(e)))
    }
    fn inspect_inner(
        &self,
        paths: &[String],
        read: impl Fn(&str) -> Option<Vec<u8>>,
    ) -> runtime::Result<DiscoveryData> {
        let mut combined = DiscoveryData::default();
        for (provider, descriptor) in &self.0 {
            let selected: Vec<_> = paths
                .iter()
                .filter(|p| {
                    p.rsplit('/')
                        .next()
                        .is_some_and(|n| descriptor.files.iter().any(|f| f == n))
                })
                .collect();
            if selected.is_empty() {
                continue;
            }
            let mut total = 0;
            let mut files = Vec::new();
            for path in &selected {
                let bytes = if descriptor
                    .reads
                    .iter()
                    .any(|p| Some(p.as_str()) == path.rsplit('/').next())
                {
                    read(path)
                } else {
                    None
                };
                total += bytes.as_ref().map_or(0, |b| b.len());
                if total > 4 * 1024 * 1024 {
                    return Err(runtime::error("discovery-read-limit"));
                }
                let parsed = bytes
                    .as_deref()
                    .and_then(|b| runtime::config::value(b).ok());
                files.push(json!({"path":path,"content":bytes.and_then(|b|String::from_utf8(b).ok()),"value":parsed}));
            }
            let data: DiscoveryData = serde_json::from_value(provider.invoke(
                "discovery.inspect",
                json!({"schema":"lekalo/discovery/v1","files":files}),
            )?)
            .map_err(|_| runtime::error("discovery-response"))?;
            if !data.admitted(&selected) {
                return Err(runtime::error("discovery-response"));
            }
            combined.extend(data);
            if combined.count() > MAX_DETECTION_ENTRIES {
                return Err(runtime::error("discovery-response-limit"));
            }
        }
        Ok(combined)
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModuleEvidence {
    path: String,
    manifest: String,
    name: Option<String>,
    standalone: bool,
    #[serde(default)]
    observe_when_scoped: bool,
    priority: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameEvidence {
    path: String,
    name: String,
    source: String,
    priority: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathEvidence {
    path: String,
    detail: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HintEvidence {
    label: String,
    kind: String,
    path: String,
    field: Option<String>,
    confidence: Confidence,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceEvidence {
    path: String,
    ecosystem: String,
    members: Vec<String>,
    confidence: Confidence,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GateEvidence {
    gate: String,
    command: String,
    source: String,
    evidence: String,
    confidence: Confidence,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiscoveryData {
    schema: String,
    manifests: Vec<PathEvidence>,
    managers: Vec<PathEvidence>,
    hints: Vec<HintEvidence>,
    workspaces: Vec<WorkspaceEvidence>,
    tools: Vec<PathEvidence>,
    gates: Vec<GateEvidence>,
    modules: Vec<ModuleEvidence>,
    names: Vec<NameEvidence>,
    refused: Vec<String>,
}
impl DiscoveryData {
    fn count(&self) -> usize {
        self.manifests.len()
            + self.managers.len()
            + self.hints.len()
            + self.workspaces.len()
            + self.tools.len()
            + self.gates.len()
            + self.modules.len()
            + self.names.len()
            + self.refused.len()
    }
    fn admitted(&self, files: &[&String]) -> bool {
        let present = |p: &str| files.iter().any(|f| f.as_str() == p);
        let directory = |p: &str| {
            (p == "." || scope::path_violation(p).is_none())
                && files
                    .iter()
                    .any(|f| f.rsplit_once('/').map(|(d, _)| d).unwrap_or(".") == p)
        };
        let text = |s: &str| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control);
        self.schema == "lekalo/discovery/v1"
            && [
                self.manifests.len(),
                self.managers.len(),
                self.hints.len(),
                self.workspaces.len(),
                self.tools.len(),
                self.gates.len(),
                self.modules.len(),
                self.names.len(),
                self.refused.len(),
            ]
            .iter()
            .all(|n| *n <= MAX_DETECTION_ENTRIES)
            && self
                .manifests
                .iter()
                .chain(&self.managers)
                .chain(&self.tools)
                .all(|r| present(&r.path) && runtime::config::token(&r.detail))
            && self.hints.iter().all(|r| {
                present(&r.path)
                    && runtime::config::token(&r.label)
                    && matches!(r.kind.as_str(), "language" | "framework")
                    && r.field.as_ref().is_none_or(|s| text(s))
            })
            && self.workspaces.iter().all(|r| {
                directory(&r.path)
                    && runtime::config::token(&r.ecosystem)
                    && r.members.len() <= 64
                    && r.members.iter().all(|m| text(m))
            })
            && self.gates.iter().all(|r| {
                present(r.evidence.split('#').next().unwrap_or_default())
                    && matches!(r.gate.as_str(), "build" | "test" | "lint" | "check")
                    && matches!(r.source.as_str(), "derived" | "workflow")
                    && text(&r.command)
                    && r.evidence.len() <= 768
            })
            && self.modules.iter().all(|r| {
                directory(&r.path)
                    && present(&r.manifest)
                    && r.manifest.rsplit_once('/').map(|(d, _)| d).unwrap_or(".") == r.path
                    && r.name.as_ref().is_none_or(|s| text(s))
            })
            && self
                .names
                .iter()
                .all(|r| present(&r.path) && text(&r.name) && text(&r.source))
            && self.refused.iter().all(|p| present(p))
    }
    fn extend(&mut self, other: Self) {
        self.manifests.extend(other.manifests);
        self.managers.extend(other.managers);
        self.hints.extend(other.hints);
        self.workspaces.extend(other.workspaces);
        self.tools.extend(other.tools);
        self.gates.extend(other.gates);
        self.modules.extend(other.modules);
        self.names.extend(other.names);
        self.refused.extend(other.refused);
    }
    fn apply(self, d: &mut Detection) {
        d.manifests
            .extend(self.manifests.into_iter().map(|r| Evidence {
                path: r.path,
                detail: r.detail,
            }));
        d.package_managers
            .extend(self.managers.into_iter().map(|r| Evidence {
                path: r.path,
                detail: r.detail,
            }));
        d.monorepo_tools
            .extend(self.tools.into_iter().map(|r| Evidence {
                path: r.path,
                detail: r.detail,
            }));
        for r in self.hints {
            push_hint(
                &mut d.languages,
                Hint {
                    label: r.label,
                    kind: if r.kind == "language" {
                        "language"
                    } else {
                        "framework"
                    },
                    path: r.path,
                    field: r.field,
                    confidence: r.confidence,
                },
            );
        }
        d.workspace_roots
            .extend(self.workspaces.into_iter().map(|r| WorkspaceRoot {
                path: r.path,
                ecosystem: r.ecosystem,
                members: r.members,
                confidence: r.confidence,
            }));
        d.gates.extend(self.gates.into_iter().map(|r| GateProposal {
            gate: gate_name(&r.gate).unwrap(),
            command: r.command,
            source: if r.source == "workflow" {
                "workflow"
            } else {
                "derived"
            },
            evidence: r.evidence,
            confidence: r.confidence,
        }));
        d.name_candidates.extend(
            self.names
                .into_iter()
                .map(|r| (r.priority, r.path, r.name, r.source)),
        );
    }
}
