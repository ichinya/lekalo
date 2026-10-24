//! Explicit production execution authority, independent of historical fixture labels.
//! Source and control custody is checked before any adapter or gate process exists.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{NativeGateFailure as Failure, NativePlan, NativeRunResult};
use crate::adapter_package::{
    self, budget::SessionBudget, consistency, DiscoverySource, ResolveContext,
};
use crate::project_fs::{EntryType, Fs, FsErrorKind};
use crate::target_protocol::{
    discovery::Discovery,
    scopes,
    transport::{AdapterCommand, TransportLimits},
    wire, CallRequest, NativeSandbox, TargetClient,
};

const PLAN_VERSION: &str = "lekalo/native-execution-plan/v0.9.1";
const POLICY_VERSION: &str = "lekalo/native-execution-policy/v0.9.1";
const CATALOG_VERSION: &str = "lekalo/native-tool-catalog/v0.9.1";
const RUNTIME_PROFILE: &str = "windows-node-lpac-job-v1";
const DOMAIN: &str = "lekalo.native-execution-plan.v0.9.1";
const TOOLS_PLAN_VERSION: &str = "lekalo/native-execution-plan/v1.0.0";
#[path = "execution_tools.rs"]
mod tools;
pub(super) use tools::asset_path as tool_output_path;
pub use tools::AssetPin;
const MAX_DOCUMENT: usize = 1024 * 1024;
const MAX_FILES: usize = 4096;
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;

/// Explicit local control documents; host paths never enter the published plan.
pub struct ExecutionInputs {
    pub root: PathBuf,
    pub adapter: PathBuf,
    pub policy: PathBuf,
    pub profile: PathBuf,
    pub catalog: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionInput {
    pub path: String,
    pub digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionTool {
    pub id: String,
    pub artifact_digest: String,
    pub program_digest: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_digest: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCustody {
    pub policy_digest: String,
    pub profile_digest: String,
    pub catalog_digest: String,
    pub adapter_manifest_digest: String,
    pub adapter_package_digest: String,
    pub adapter_entry_digest: String,
    pub source_manifest_digest: String,
}

/// Approval must name this outer digest, never only an inner fixture plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPlan {
    pub schema_version: String,
    pub kind: String,
    pub plan_digest: String,
    pub native_plan: NativePlan,
    pub custody: ExecutionCustody,
    pub inputs: Vec<ExecutionInput>,
    pub tools: Vec<ExecutionTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<AssetPin>,
}

/// Produced in this invocation; never accepted as a substitute for execution.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionReport {
    pub schema_version: String,
    pub kind: String,
    pub run: NativeRunResult,
    pub outputs: Vec<CommandOutput>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandOutput {
    pub digest: String,
    pub output: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema_version: String,
    allow_execution: bool,
    platforms: Vec<String>,
    runtime_profile: String,
    planning_policy: Value,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema_version: String,
    tools: Vec<CatalogTool>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogTool {
    id: String,
    program: PathBuf,
    artifact_digest: String,
    program_digest: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Profile {
    id: String,
    mode: String,
    target: String,
    read_roots: Vec<ReadRoot>,
    exclusions: Vec<String>,
    provenance: Value,
    #[serde(default)]
    target_resolution: Option<Value>,
    #[serde(default)]
    local_reference: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadRoot {
    kind: String,
    path: String,
}

fn adapter_failure(
    failure: crate::target_protocol::TargetFailure,
    detail: &'static str,
) -> Failure {
    use crate::target_protocol::TargetFailure;
    match failure {
        TargetFailure::CapabilityUnsupported { .. } | TargetFailure::IrUnsupported => {
            Failure::CapabilityMissing {
                detail: "native-planning-capability",
            }
        }
        TargetFailure::Timeout
        | TargetFailure::Cancelled
        | TargetFailure::OutputLimit { .. }
        | TargetFailure::Crash { .. }
        | TargetFailure::TransportFailed { .. } => Failure::RunInfrastructure,
        TargetFailure::ScopeViolation { .. }
        | TargetFailure::ProtectedPath { .. }
        | TargetFailure::SecurityRefusal { .. }
        | TargetFailure::DryRunMutation { .. } => Failure::SecurityViolation,
        _ => invalid(detail),
    }
}

fn invalid(detail: &'static str) -> Failure {
    Failure::PlanInvalid { detail }
}
fn hash(bytes: &[u8]) -> String {
    format!("sha256:{}", crate::digest::sha256_hex(bytes))
}
fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>, Failure> {
    serde_json::to_vec(&serde_json::to_value(value).map_err(|_| invalid("execution-document"))?)
        .map_err(|_| invalid("execution-document"))
}
fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, Failure> {
    if bytes.len() > MAX_DOCUMENT {
        return Err(invalid("execution-document-limit"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("execution-document"))?;
    let value =
        crate::classification::parse_checked(text).map_err(|_| invalid("execution-document"))?;
    serde_json::from_value(value).map_err(|_| invalid("execution-document"))
}

/// Read one explicitly selected control through the same no-link capability as source.
pub fn read_execution_document(path: &Path) -> Result<Vec<u8>, Failure> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| invalid("execution-control-path"))?;
    Fs::open(parent)
        .map_err(|_| invalid("execution-control-path"))?
        .read_file_opt("", name, MAX_DOCUMENT)
        .map_err(|_| invalid("execution-control-read"))?
        .ok_or_else(|| invalid("execution-control-read"))
}

fn execution_digest(plan: &ExecutionPlan) -> Result<String, Failure> {
    let mut value = serde_json::to_value(plan).map_err(|_| invalid("execution-plan"))?;
    value
        .as_object_mut()
        .ok_or_else(|| invalid("execution-plan"))?
        .remove("plan_digest");
    let mut bytes = if plan.schema_version == TOOLS_PLAN_VERSION {
        b"lekalo.native-execution-plan.v1.0.0".to_vec()
    } else {
        DOMAIN.as_bytes().to_vec()
    };
    bytes.extend(canonical(&value)?);
    Ok(hash(&bytes))
}

pub fn validate_execution_plan(bytes: &[u8]) -> Result<ExecutionPlan, Failure> {
    if bytes.len() > MAX_DOCUMENT {
        return Err(invalid("execution-document-limit"));
    }
    let successor = decode::<Value>(bytes)?
        .get("schema_version")
        .and_then(Value::as_str)
        == Some(TOOLS_PLAN_VERSION);
    if !super::execution_schema::valid(
        bytes,
        if successor {
            super::execution_schema::Document::PlanTools
        } else {
            super::execution_schema::Document::Plan
        },
    ) {
        return Err(invalid("execution-plan-schema"));
    }
    let plan: ExecutionPlan = decode(bytes)?;
    if ![PLAN_VERSION, TOOLS_PLAN_VERSION].contains(&plan.schema_version.as_str())
        || plan.kind != "native-execution-plan"
        || plan.inputs.is_empty()
        || plan.inputs.len() > MAX_FILES
        || plan.tools.is_empty()
        || plan.tools.len() > 32
    {
        return Err(invalid("execution-plan-shape"));
    }
    super::validate_plan(&canonical(&plan.native_plan)?)
        .map_err(|_| invalid("execution-native-plan"))?;
    if hash(&canonical(&plan.inputs)?) != plan.custody.source_manifest_digest
        || plan.native_plan.input_manifest_digest != plan.custody.source_manifest_digest
        || plan.native_plan.adapter.digest != plan.custody.adapter_entry_digest
        || plan
            .inputs
            .windows(2)
            .any(|pair| pair[0].path >= pair[1].path)
        || plan.tools.windows(2).any(|pair| pair[0].id >= pair[1].id)
        || plan.native_plan.tools.iter().any(|tool| {
            !plan.tools.iter().any(|pin| {
                pin.id == tool.id
                    && pin.artifact_digest == tool.artifact_digest
                    && tool.version.as_deref() == Some(pin.version.as_str())
                    && (!successor || tool.entry_digest == pin.entry_digest)
            })
        })
    {
        return Err(invalid("execution-plan-custody"));
    }
    if execution_digest(&plan)? != plan.plan_digest {
        return Err(Failure::PlanStale);
    }
    Ok(plan)
}

fn profile(bytes: &[u8]) -> Result<Profile, Failure> {
    if bytes.len() > 8192 {
        return Err(invalid("execution-profile-limit"));
    }
    let profile: Profile = decode(bytes)?;
    if !scopes::is_token(&profile.id)
        || !matches!(profile.mode.as_str(), "observed" | "contracted")
        || profile.target != "node-typescript"
        || profile.read_roots.is_empty()
        || profile.read_roots.len() > 32
        || profile.exclusions.len() > 64
        || profile.local_reference.is_some()
        || !profile.provenance.is_object()
        || profile
            .target_resolution
            .as_ref()
            .is_some_and(|v| !v.is_object())
    {
        return Err(invalid("execution-profile"));
    }
    for root in &profile.read_roots {
        if !matches!(root.kind.as_str(), "file" | "tree")
            || !source_path(&root.path)
            || crate::init::scope::protected(&root.path)
        {
            return Err(invalid("execution-read-root"));
        }
    }
    for exclusion in &profile.exclusions {
        if !source_path(exclusion.strip_suffix("/**").unwrap_or(exclusion)) {
            return Err(invalid("execution-exclusion"));
        }
    }
    Ok(profile)
}

fn source_path(path: &str) -> bool {
    scopes::is_logical_path(path)
        && path.len() <= 256
        && path.bytes().all(|b| !b.is_ascii_uppercase())
}
fn excluded(profile: &Profile, path: &str) -> bool {
    crate::init::scope::protected(path)
        || profile.exclusions.iter().any(|exclusion| {
            let root = exclusion.strip_suffix("/**").unwrap_or(exclusion);
            path == root
                || path
                    .strip_prefix(root)
                    .is_some_and(|rest| rest.starts_with('/'))
        })
}

/// Verify exact spelling and physical type at every directory component.
fn lookup(fs: &Fs, path: &str) -> Result<Option<EntryType>, Failure> {
    let mut dir = String::new();
    let parts: Vec<_> = path.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        let entries = match fs.entries_bounded(&dir, MAX_FILES) {
            Ok(entries) => entries,
            Err(FsErrorKind::NotFound) => return Ok(None),
            Err(_) => return Err(invalid("execution-source-read")),
        };
        if entries
            .iter()
            .any(|(name, _)| name.as_str() != *part && name.eq_ignore_ascii_case(part))
        {
            return Err(invalid("execution-source-alias"));
        }
        let Some((_, kind)) = entries.into_iter().find(|(name, _)| name == *part) else {
            return Ok(None);
        };
        if matches!(kind, EntryType::Symlink | EntryType::Special) {
            return Err(Failure::SecurityViolation);
        }
        if i + 1 == parts.len() {
            return Ok(Some(kind));
        }
        if kind != EntryType::Directory {
            return Err(invalid("execution-source-type"));
        }
        if !dir.is_empty() {
            dir.push('/');
        }
        dir.push_str(part);
    }
    Ok(None)
}

/// Bounded no-follow capture. Exclusions prune before opening their contents.
fn capture(root: &Path, profile: &Profile) -> Result<BTreeMap<String, Vec<u8>>, Failure> {
    let fs = Fs::open(root).map_err(|_| invalid("execution-source-root"))?;
    let mut files = BTreeMap::new();
    let mut dirs = BTreeSet::new();
    let mut stack = Vec::new();
    for read in &profile.read_roots {
        if excluded(profile, &read.path) {
            continue;
        }
        match (read.kind.as_str(), lookup(&fs, &read.path)?) {
            (_, None) => continue,
            ("tree", Some(EntryType::Directory)) => stack.push(read.path.clone()),
            ("file", Some(EntryType::File)) => {
                files.insert(read.path.clone(), read_source(&fs, &read.path)?);
            }
            _ => return Err(invalid("execution-source-type")),
        }
    }
    while let Some(dir) = stack.pop() {
        if !dirs.insert(dir.clone()) {
            continue;
        }
        if dirs.len() > MAX_FILES {
            return Err(invalid("execution-source-limit"));
        }
        for (name, kind) in fs
            .entries_bounded(&dir, MAX_FILES)
            .map_err(|_| invalid("execution-source-limit"))?
        {
            let path = format!("{dir}/{name}");
            if excluded(profile, &path) {
                continue;
            }
            if !source_path(&path) {
                return Err(invalid("execution-source-path"));
            }
            match kind {
                EntryType::Directory => stack.push(path),
                EntryType::File => {
                    files.insert(path.clone(), read_source(&fs, &path)?);
                }
                _ => return Err(Failure::SecurityViolation),
            }
            if files.len() > MAX_FILES
                || files.values().map(Vec::len).sum::<usize>() > MAX_SOURCE_BYTES
            {
                return Err(invalid("execution-source-limit"));
            }
        }
    }
    if files.len() > MAX_FILES || files.values().map(Vec::len).sum::<usize>() > MAX_SOURCE_BYTES {
        return Err(invalid("execution-source-limit"));
    }
    if files.is_empty() {
        return Err(invalid("execution-source-empty"));
    }
    Ok(files)
}
fn read_source(fs: &Fs, path: &str) -> Result<Vec<u8>, Failure> {
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    fs.read_file_opt(dir, name, MAX_FILE_BYTES)
        .map_err(|_| invalid("execution-source-read"))?
        .ok_or_else(|| invalid("execution-source-read"))
}
fn manifest(files: &BTreeMap<String, Vec<u8>>) -> Vec<ExecutionInput> {
    files
        .iter()
        .map(|(path, bytes)| ExecutionInput {
            path: path.clone(),
            digest: hash(bytes),
        })
        .collect()
}

type ControlBytes = (Vec<u8>, Vec<u8>, Vec<u8>);

fn controls(input: &ExecutionInputs) -> Result<ControlBytes, Failure> {
    Ok((
        read_execution_document(&input.policy)?,
        read_execution_document(&input.profile)?,
        read_execution_document(&input.catalog)?,
    ))
}
fn checked_policy(bytes: &[u8]) -> Result<(Policy, super::policy::ExecutionPolicy), Failure> {
    if !super::execution_schema::valid(bytes, super::execution_schema::Document::Policy) {
        return Err(invalid("execution-policy-schema"));
    }
    let policy: Policy = decode(bytes)?;
    if policy.schema_version != POLICY_VERSION
        || policy.runtime_profile != RUNTIME_PROFILE
        || policy.platforms != ["windows"]
    {
        return Err(invalid("execution-policy"));
    }
    let planning_bytes = canonical(&policy.planning_policy)?;
    if planning_bytes.len() > 16384 {
        return Err(invalid("execution-policy-limit"));
    }
    let planning = super::validate_policy(&planning_bytes)
        .map_err(|_| invalid("execution-policy-planning"))?;
    if planning.env_recipe != json!({"allowed_names":[],"bindings":[]}) {
        return Err(invalid("execution-environment-profile"));
    }
    Ok((policy, planning))
}

struct Prepared {
    plan: ExecutionPlan,
    stage: tempfile::TempDir,
    source_root: PathBuf,
    catalog: super::RunnerCatalog,
    catalog_tools: Vec<CatalogTool>,
    assets_root: Option<PathBuf>,
    runtime_version: String,
}

fn prepare(
    input: &ExecutionInputs,
    changes: wire::NativeChanges,
    expected: Option<&ExecutionPlan>,
    cancel: Option<&AtomicBool>,
) -> Result<Prepared, Failure> {
    let (policy_bytes, profile_bytes, catalog_bytes) = controls(input)?;
    let (policy, planning_policy) = checked_policy(&policy_bytes)?;
    let profile = profile(&profile_bytes)?;
    let successor = decode::<Value>(&catalog_bytes)?
        .get("schema_version")
        .and_then(Value::as_str)
        == Some(tools::TOOLS_CATALOG_VERSION);
    if !super::execution_schema::valid(
        &catalog_bytes,
        if successor {
            super::execution_schema::Document::CatalogTools
        } else {
            super::execution_schema::Document::Catalog
        },
    ) {
        return Err(invalid("execution-catalog-schema"));
    }
    let captured_tools = if successor {
        Some(tools::capture(&catalog_bytes)?)
    } else {
        None
    };
    let mut catalog: Catalog = match &captured_tools {
        Some(tools) => tools.catalog.clone(),
        None => decode(&catalog_bytes)?,
    };
    if ![CATALOG_VERSION, tools::TOOLS_CATALOG_VERSION].contains(&catalog.schema_version.as_str())
        || catalog.tools.is_empty()
        || catalog.tools.len() > 32
    {
        return Err(invalid("execution-catalog"));
    }
    if !successor
        && planning_policy
            .confirmations
            .iter()
            .any(|c| c.tool_ref.entry_digest.is_some())
    {
        return Err(invalid("execution-tool-entry-profile"));
    }
    let runtime_version = captured_tools
        .as_ref()
        .map(|tools| tools.pin.runtime_version.clone())
        .unwrap_or_else(|| catalog.tools[0].version.clone());
    let mut tools = Vec::new();
    let mut runner_catalog = super::RunnerCatalog::new();
    let mut tool_ids = BTreeSet::new();
    let mut runtime_bytes = Vec::new();
    for tool in &catalog.tools {
        if !scopes::is_token(&tool.id)
            || !tool_ids.insert(&tool.id)
            || !tool.program.is_absolute()
            || tool.program.file_stem().and_then(|n| n.to_str()) != Some("node")
            || !runtime_version.starts_with("24.")
            || tool.version == "unknown"
            || tool.version.is_empty()
            || tool.version.len() > 32
            || (!successor && tool.artifact_digest != tool.program_digest)
        {
            return Err(invalid("execution-tool"));
        }
        let parent = tool
            .program
            .parent()
            .ok_or_else(|| invalid("execution-tool"))?;
        let name = tool
            .program
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| invalid("execution-tool"))?;
        let bytes = Fs::open(parent)
            .map_err(|_| invalid("execution-tool"))?
            .read_file_opt("", name, super::runner::STAGE_MAX_FILE_BYTES)
            .map_err(|_| invalid("execution-tool"))?
            .ok_or_else(|| invalid("execution-tool"))?;
        if hash(&bytes) != tool.program_digest {
            return Err(Failure::PlanStale);
        }
        if runtime_bytes.is_empty() {
            runtime_bytes = bytes;
        } else if tool.program_digest != catalog.tools[0].program_digest
            || (!successor && tool.version != catalog.tools[0].version)
        {
            return Err(invalid("execution-runtime-profile"));
        }
        tools.push(ExecutionTool {
            id: tool.id.clone(),
            artifact_digest: tool.artifact_digest.clone(),
            program_digest: tool.program_digest.clone(),
            version: tool.version.clone(),
            entry: captured_tools
                .as_ref()
                .and_then(|tools| tools.entries.get(&tool.id))
                .map(|(entry, _)| entry.clone()),
            entry_digest: captured_tools
                .as_ref()
                .and_then(|tools| tools.entries.get(&tool.id))
                .map(|(_, digest)| digest.clone()),
        });
        runner_catalog.insert(
            tool.id.clone(),
            super::CatalogEntry {
                program: tool.program.clone(),
                artifact_digest: tool.artifact_digest.clone(),
                version: Some(tool.version.clone()),
            },
        );
    }
    tools.sort_by(|a, b| a.id.cmp(&b.id));
    let files = capture(&input.root, &profile)?;
    let inputs = manifest(&files);
    let resolved = adapter_package::resolve(
        &DiscoverySource::Path(input.adapter.clone()),
        &ResolveContext {
            root: Some(input.root.clone()),
            offline: true,
        },
    )
    .map_err(|failure| match failure {
        adapter_package::PackageFailure::ChecksumMismatch { .. } => Failure::PlanStale,
        _ => invalid("execution-adapter-custody"),
    })?;
    let adapter = &resolved.candidate.manifest;
    let package_root = resolved
        .candidate
        .package_root
        .as_ref()
        .ok_or_else(|| invalid("execution-adapter-package"))?;
    if adapter.files().len() != 1 || adapter.files()[0].path() != adapter.entry() {
        return Err(invalid("execution-adapter-single-file"));
    }
    let entry_bytes = read_source_with_limit(package_root, adapter.entry(), 32 * 1024 * 1024)?;
    let custody = ExecutionCustody {
        policy_digest: hash(&policy_bytes),
        profile_digest: hash(&profile_bytes),
        catalog_digest: hash(&catalog_bytes),
        adapter_manifest_digest: hash(adapter.stored_bytes()),
        adapter_package_digest: adapter.package_digest().as_str().to_owned(),
        adapter_entry_digest: hash(&entry_bytes),
        source_manifest_digest: hash(&canonical(&inputs)?),
    };
    if custody.adapter_entry_digest != adapter.files()[0].digest().as_str() {
        return Err(Failure::PlanStale);
    }
    if let Some(expected) = expected {
        if expected.custody != custody
            || expected.inputs != inputs
            || expected.tools != tools
            || expected.assets != captured_tools.as_ref().map(|tools| tools.pin.clone())
        {
            return Err(Failure::PlanStale);
        }
    }
    if cancel.is_some_and(|token| token.load(Ordering::SeqCst)) {
        return Err(Failure::RunInfrastructure);
    }
    let stage = tempfile::Builder::new()
        .prefix("lekalo-native-prepared-")
        .tempdir()
        .map_err(|_| Failure::RunInfrastructure)?;
    let runtime = stage
        .path()
        .join(if cfg!(windows) { "node.exe" } else { "node" });
    std::fs::write(&runtime, runtime_bytes).map_err(|_| Failure::RunInfrastructure)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| Failure::RunInfrastructure)?;
    }
    for tool in &mut catalog.tools {
        tool.program = runtime.clone();
        runner_catalog.insert(
            tool.id.clone(),
            super::CatalogEntry {
                program: runtime.clone(),
                artifact_digest: tool.artifact_digest.clone(),
                version: Some(tool.version.clone()),
            },
        );
        if let Some((entry, entry_digest)) = captured_tools
            .as_ref()
            .and_then(|tools| tools.entries.get(&tool.id))
        {
            runner_catalog.set_recipe(
                tool.id.clone(),
                super::runner::ToolRecipe {
                    entry: entry.clone(),
                    entry_digest: entry_digest.clone(),
                    program_digest: tool.program_digest.clone(),
                },
            );
        }
    }
    let assets_root = if let Some(tools) = &captured_tools {
        let root = stage.path().join("node_modules");
        tools.stage(&root)?;
        Some(root)
    } else {
        None
    };
    let source_root = stage.path().join("project");
    std::fs::create_dir(&source_root).map_err(|_| Failure::RunInfrastructure)?;
    for (path, bytes) in &files {
        let target = source_root.join(path);
        std::fs::create_dir_all(
            target
                .parent()
                .ok_or_else(|| invalid("execution-source-path"))?,
        )
        .map_err(|_| Failure::RunInfrastructure)?;
        std::fs::write(target, bytes).map_err(|_| Failure::RunInfrastructure)?;
    }
    let entry = stage.path().join("adapter.mjs");
    std::fs::write(&entry, entry_bytes).map_err(|_| Failure::RunInfrastructure)?;
    let command = AdapterCommand {
        program: catalog.tools[0].program.clone(),
        args: vec![
            entry.to_string_lossy().into_owned(),
            "--lekalo-project-profile-json".into(),
            String::from_utf8(canonical(&decode::<Value>(&profile_bytes)?)?)
                .map_err(|_| invalid("execution-profile"))?,
            "--lekalo-native-policy-json".into(),
            String::from_utf8(canonical(&policy.planning_policy)?)
                .map_err(|_| invalid("execution-policy"))?,
        ],
    };
    let mut client = TargetClient::new(TransportLimits {
        timeout_ms: 60_000,
        max_output_bytes: MAX_DOCUMENT,
        ..TransportLimits::default()
    });
    client.set_budget(
        SessionBudget::budget_for(Some(adapter))
            .map_err(|_| invalid("execution-adapter-budget"))?,
    );
    let discovered = Discovery::run_with_cancel(&mut client, &command, &source_root, cancel)
        .map_err(|failure| adapter_failure(failure, "execution-adapter-describe"))?;
    consistency::check(adapter, &discovered).map_err(|_| invalid("execution-adapter-manifest"))?;
    if discovered.negotiated_version != "0.9.1" {
        return Err(Failure::CapabilityMissing {
            detail: "native-full-plan-protocol",
        });
    }
    let catalog_digest = planning_catalog_digest(&planning_policy)?;
    let request = wire::NativeRequest {
        changes,
        scan_ref: wire::NativeContentRef {
            digest: custody.source_manifest_digest.clone(),
            revision: None,
            adapter: None,
        },
        observed_ref: None,
        execution_policy_ref: wire::NativeContentRef {
            digest: planning_policy.policy_digest.clone(),
            revision: None,
            adapter: None,
        },
        input_manifest_digest: custody.source_manifest_digest.clone(),
        tool_catalog_digest: catalog_digest.clone(),
        capability_snapshot_digest: discovered.capability_digest.clone(),
    };
    let fs = Fs::open(&source_root).map_err(|_| Failure::RunInfrastructure)?;
    let result = client
        .call(
            &command,
            CallRequest {
                operation: wire::Operation::PlanNative,
                target: Some("node-typescript"),
                profile: Some(&profile.id),
                profile_resolution: None,
                ir_path: None,
                dry_run: None,
                plan_id: None,
                native_request: Some(&request),
                lint_request: None,
            },
            &source_root,
            &fs,
            cancel,
        )
        .map_err(|failure| adapter_failure(failure, "execution-adapter-planning"))?;
    let native_plan = result
        .response
        .result
        .and_then(|result| result.native_plan)
        .and_then(|result| result.plan)
        .ok_or_else(|| invalid("execution-native-plan"))?;
    for (mismatch, detail) in [
        (
            native_plan.adapter.id != discovered.adapter.id
                || native_plan.adapter.version != discovered.adapter.version,
            "execution-adapter-identity",
        ),
        (
            native_plan.input_manifest_digest != custody.source_manifest_digest,
            "execution-source-manifest",
        ),
        (
            native_plan.tool_catalog_digest != catalog_digest,
            "execution-logical-tool-catalog",
        ),
        (
            native_plan.capability_snapshot_digest != discovered.capability_digest,
            "execution-capability-snapshot",
        ),
        (native_plan.commands.is_empty(), "execution-no-commands"),
        (
            native_plan.workspace.completeness != "complete",
            "execution-workspace-incomplete",
        ),
    ] {
        if mismatch {
            return Err(invalid(detail));
        }
    }
    if !matches!(
        native_plan.workspace.manager.as_str(),
        "pnpm-workspace" | "npm-standalone" | "npm-workspaces"
    ) {
        return Err(Failure::ManagerUnsupported);
    }
    if native_plan.workspace.manager == "npm-workspaces" && !successor {
        return Err(Failure::ManagerUnsupported);
    }
    if native_plan.workspace.manager == "npm-workspaces" {
        let root = captured_package(&files, ".")?;
        let patterns = root
            .get("workspaces")
            .and_then(Value::as_array)
            .ok_or(Failure::ManagerUnsupported)?;
        for pattern in patterns {
            let pattern = pattern.as_str().ok_or(Failure::ManagerUnsupported)?;
            let prefix = pattern.strip_suffix("/*").unwrap_or(pattern);
            if !source_path(prefix)
                || prefix.contains(['*', '?', '!'])
                || !profile.read_roots.iter().any(|root| {
                    root.kind == "tree"
                        && (root.path == prefix || prefix.starts_with(&format!("{}/", root.path)))
                })
            {
                return Err(Failure::CapabilityMissing {
                    detail: "native-workspace-scope",
                });
            }
        }
    }
    if native_plan.required_capabilities != ["plan.native-gates"] {
        return Err(Failure::CapabilityMissing {
            detail: "native-required-capability",
        });
    }
    if native_plan.write_policy != planning_policy.write_policy
        || native_plan.limits != planning_policy.limits
        || native_plan.trust != planning_policy.trust
        || native_plan.repository_role != planning_policy.repository_role
        || native_plan.authority_ref != planning_policy.authority_ref
        || native_plan.policy_ref != planning_policy.policy_ref
        || native_plan.classification_ref != planning_policy.classification_ref
        || native_plan.execution_policy_ref.id != planning_policy.identity.id
        || native_plan.execution_policy_ref.version != planning_policy.identity.version
        || native_plan.execution_policy_ref.digest != planning_policy.policy_digest
        || native_plan.profile_ref.id != profile.id
        || !native_plan.env.allowed_names.is_empty()
        || !native_plan.env.bindings.is_empty()
    {
        return Err(Failure::TrustInsufficient);
    }
    // Adapter consistency is not authorization: independently intersect every
    // proposed command with the selected policy and captured package bytes.
    for package in &native_plan.workspace.packages {
        let document = captured_package(&files, &package.root)?;
        let name = document.get("name").and_then(Value::as_str);
        if hash(&canonical(&document)?) != package.manifest_digest
            || name != package.name.as_deref()
            || package.id != format!("{}={}", package.root, name.unwrap_or("(unnamed)"))
        {
            return Err(Failure::TrustInsufficient);
        }
    }
    if successor
        && planning_policy
            .fallback_rule
            .get("mode")
            .and_then(Value::as_str)
            == Some("release-full")
    {
        for confirmation in &planning_policy.confirmations {
            if captured_package(&files, &confirmation.cwd).is_ok()
                && planning_policy
                    .allowed_gate_kinds
                    .contains(&confirmation.gate)
                && !native_plan.commands.iter().any(|command| {
                    command.gate_id == confirmation.gate_id
                        && command.package_id == confirmation.package_id
                })
            {
                return Err(invalid("execution-required-confirmation-omitted"));
            }
        }
    }
    for command in &native_plan.commands {
        let confirmation = planning_policy
            .confirmations
            .iter()
            .find(|confirmation| {
                confirmation.package_id == command.package_id
                    && confirmation.gate_id == command.gate_id
                    && confirmation.gate == command.gate
                    && confirmation.gate_kind == command.gate_kind
                    && confirmation.required == command.required
                    && confirmation.cwd == command.cwd
                    && confirmation.script_name == command.script_name
                    && confirmation.argv == command.argv
                    && confirmation.tool_ref.id == command.tool_ref
                    && confirmation.script_digest == command.script_digest
                    && confirmation.rule_digest == command.confirmation_ref
            })
            .ok_or(Failure::TrustInsufficient)?;
        let document = captured_package(&files, &command.cwd)?;
        let script = document
            .get("scripts")
            .and_then(|scripts| scripts.get(&command.script_name))
            .and_then(Value::as_str)
            .ok_or(Failure::TrustInsufficient)?;
        if hash(&canonical(&document)?) != confirmation.manifest_digest
            || script != command.argv.join(" ")
            || hash(script.as_bytes()) != confirmation.script_digest
            || command.limits != planning_policy.limits
            || !command.env.is_empty()
        {
            return Err(Failure::TrustInsufficient);
        }
        let tool = catalog
            .tools
            .iter()
            .find(|tool| tool.id == command.tool_ref)
            .ok_or_else(|| invalid("execution-tool-missing"))?;
        let recorded = native_plan
            .tools
            .iter()
            .find(|recorded| recorded.id == tool.id)
            .ok_or_else(|| invalid("execution-tool-missing"))?;
        let recipe = runner_catalog.recipe(&command.tool_ref);
        if command.argv.first().map(String::as_str)
            != Some(if successor {
                command.tool_ref.as_str()
            } else {
                "node"
            })
            || recorded.artifact_digest != tool.artifact_digest
            || recorded.version.as_deref() != Some(tool.version.as_str())
            || recorded.entry_digest.as_deref() != recipe.map(|recipe| recipe.entry_digest.as_str())
            || confirmation.tool_ref.entry_digest.as_deref()
                != recipe.map(|recipe| recipe.entry_digest.as_str())
            || command.argv.len() < 2
        {
            return Err(invalid("execution-tool-recipe"));
        }
        if successor {
            continue;
        }
        let script = &command.argv[1];
        let path = if command.cwd == "." {
            script.clone()
        } else {
            format!("{}/{script}", command.cwd)
        };
        if !source_path(&path) || !files.contains_key(&path) {
            return Err(Failure::CapabilityMissing {
                detail: "native-script-missing",
            });
        }
    }
    let mut plan = ExecutionPlan {
        schema_version: if successor {
            TOOLS_PLAN_VERSION
        } else {
            PLAN_VERSION
        }
        .into(),
        kind: "native-execution-plan".into(),
        plan_digest: String::new(),
        native_plan: *native_plan,
        custody,
        inputs,
        tools,
        assets: captured_tools.as_ref().map(|tools| tools.pin.clone()),
    };
    plan.plan_digest = execution_digest(&plan)?;
    validate_execution_plan(&canonical(&plan)?)?;
    Ok(Prepared {
        plan,
        stage,
        source_root,
        catalog: runner_catalog,
        catalog_tools: catalog.tools,
        assets_root,
        runtime_version,
    })
}

fn captured_package(files: &BTreeMap<String, Vec<u8>>, root: &str) -> Result<Value, Failure> {
    let path = if root == "." {
        "package.json".to_owned()
    } else {
        format!("{root}/package.json")
    };
    decode(files.get(&path).ok_or(Failure::TrustInsufficient)?)
}

fn read_source_with_limit(root: &Path, path: &str, limit: usize) -> Result<Vec<u8>, Failure> {
    let fs = Fs::open(root).map_err(|_| invalid("execution-adapter-read"))?;
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    fs.read_file_opt(dir, name, limit)
        .map_err(|_| invalid("execution-adapter-read"))?
        .ok_or_else(|| invalid("execution-adapter-read"))
}

fn planning_catalog_digest(policy: &super::policy::ExecutionPolicy) -> Result<String, Failure> {
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for confirmation in &policy.confirmations {
        let tool = &confirmation.tool_ref;
        if !seen.insert(&tool.id) {
            continue;
        }
        let mut entry = json!({"id":tool.id, "name":confirmation.argv[0], "version":tool.version.as_deref().unwrap_or("unknown"), "artifact_digest":tool.artifact_digest,
            "platform": if cfg!(target_os="macos") { "darwin" } else { std::env::consts::OS }, "provenance":"declared-manifest"});
        if let Some(digest) = &tool.entry_digest {
            entry["entry_digest"] = json!(digest);
        }
        entries.push(entry);
    }
    Ok(hash(&canonical(&entries)?))
}

/// Read-only plan generation; the adapter may inspect a private bounded copy only.
pub fn create_execution_plan(
    input: &ExecutionInputs,
    changes: wire::NativeChanges,
) -> Result<ExecutionPlan, Failure> {
    let prepared = prepare(input, changes, None, None)?;
    let plan = prepared.plan;
    prepared
        .stage
        .close()
        .map_err(|_| Failure::RunInfrastructure)?;
    Ok(plan)
}

/// A fresh authorization/custody fence after read-only planning, and again
/// after the OS probe. An editor or policy revocation during preparation
/// must not admit the first project command on an outdated authorization.
fn execution_fence(input: &ExecutionInputs, plan: &ExecutionPlan) -> Result<(), Failure> {
    let (policy, profile_bytes, catalog_bytes) = controls(input)?;
    if decode::<Policy>(&policy).is_ok_and(|policy| !policy.allow_execution) {
        return Err(Failure::TrustInsufficient);
    }
    if hash(&policy) != plan.custody.policy_digest
        || hash(&profile_bytes) != plan.custody.profile_digest
        || hash(&catalog_bytes) != plan.custody.catalog_digest
    {
        return Err(Failure::PlanStale);
    }
    let current =
        capture(&input.root, &profile(&profile_bytes)?).map_err(|_| Failure::RunInfrastructure)?;
    if manifest(&current) != plan.inputs {
        return Err(Failure::PlanStale);
    }
    let captured = if plan.assets.is_some() {
        Some(tools::capture(&catalog_bytes)?)
    } else {
        None
    };
    if captured.as_ref().map(|tools| &tools.pin) != plan.assets.as_ref() {
        return Err(Failure::PlanStale);
    }
    let catalog: Catalog = match captured {
        Some(tools) => tools.catalog,
        None => decode(&catalog_bytes)?,
    };
    for tool in catalog.tools {
        let parent = tool
            .program
            .parent()
            .ok_or_else(|| invalid("execution-tool"))?;
        let name = tool
            .program
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("execution-tool"))?;
        let bytes = Fs::open(parent)
            .map_err(|_| Failure::RunInfrastructure)?
            .read_file_opt("", name, super::runner::STAGE_MAX_FILE_BYTES)
            .map_err(|_| Failure::RunInfrastructure)?
            .ok_or(Failure::PlanStale)?;
        if hash(&bytes) != tool.program_digest {
            return Err(Failure::PlanStale);
        }
    }
    let adapter = adapter_package::resolve(
        &DiscoverySource::Path(input.adapter.clone()),
        &ResolveContext {
            root: Some(input.root.clone()),
            offline: true,
        },
    )
    .map_err(|_| Failure::PlanStale)?;
    if hash(adapter.candidate.manifest.stored_bytes()) != plan.custody.adapter_manifest_digest
        || adapter.candidate.manifest.package_digest().as_str()
            != plan.custody.adapter_package_digest
    {
        return Err(Failure::PlanStale);
    }
    Ok(())
}

/// Execute after exact external approval and fresh selected policy authorization.
pub fn run_execution_plan(
    input: &ExecutionInputs,
    bytes: &[u8],
    approved_digest: &str,
    cancellation: Option<&AtomicBool>,
) -> Result<NativeRunResult, Failure> {
    run_execution_report(input, bytes, approved_digest, cancellation).map(|report| report.run)
}

/// Execute with bounded redacted artifacts retained alongside their references.
pub fn run_execution_report(
    input: &ExecutionInputs,
    bytes: &[u8],
    approved_digest: &str,
    cancellation: Option<&AtomicBool>,
) -> Result<ExecutionReport, Failure> {
    let plan = validate_execution_plan(bytes)?;
    if approved_digest != plan.plan_digest {
        return Err(Failure::ApprovalMissing);
    }
    let (policy_bytes, profile_bytes, catalog_bytes) = controls(input)?;
    let (policy, _) = checked_policy(&policy_bytes)?;
    if !policy.allow_execution {
        return Err(Failure::TrustInsufficient);
    }
    if hash(&policy_bytes) != plan.custody.policy_digest
        || hash(&profile_bytes) != plan.custody.profile_digest
        || hash(&catalog_bytes) != plan.custody.catalog_digest
    {
        return Err(Failure::PlanStale);
    }
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err(Failure::CapabilityMissing {
            detail: "native-execution-platform",
        });
    }
    let changes = serde_json::from_value(
        serde_json::to_value(&plan.native_plan.changes)
            .map_err(|_| invalid("execution-changes"))?,
    )
    .map_err(|_| invalid("execution-changes"))?;
    let prepared = prepare(input, changes, Some(&plan), cancellation)?;
    if prepared.plan != plan {
        return Err(Failure::PlanStale);
    }
    execution_fence(input, &plan)?;
    let mut sandbox =
        NativeSandbox::prepare_assets(&prepared.source_root, prepared.assets_root.as_deref())
            .map_err(|_| Failure::CapabilityMissing {
                detail: "native-execution-confinement",
            })?;
    let runtime = &prepared.catalog_tools[0];
    let proof = sandbox
        .qualify(
            &runtime.program,
            &runtime.program_digest,
            Some(&prepared.runtime_version),
        )
        .map_err(|_| Failure::CapabilityMissing {
            detail: "native-execution-confinement",
        })?;
    if proof.memory_bytes != 512 * 1024 * 1024 || proof.process_limit != 32 {
        return Err(Failure::CapabilityMissing {
            detail: "native-execution-resources",
        });
    }
    execution_fence(input, &plan)?;
    let capability = super::RuntimeCapability {
        network_denial: proof.network.into(),
        process_containment: proof.processes.into(),
        backend: proof.backend.into(),
    };
    let execution_started = std::time::Instant::now();
    let outputs = std::cell::RefCell::new(BTreeMap::new());
    let mut receipt = super::runner::run_confirmed_plan_in_sandbox(
        &super::ConfirmedRun {
            plan: &plan.native_plan,
            approved_plan_digest: &plan.native_plan.plan_digest,
            catalog: &prepared.catalog,
            source_root: &prepared.source_root,
            capability: &capability,
            cancellation,
            host_paths: vec![
                std::fs::canonicalize(&input.root)
                    .map_err(|_| Failure::RunInfrastructure)?
                    .to_string_lossy()
                    .into_owned(),
                prepared.stage.path().to_string_lossy().into_owned(),
            ],
            secrets: Vec::new(),
        },
        sandbox,
        &outputs,
    )?;
    receipt.provenance = Some(plan.plan_digest.clone());
    receipt.capability_evidence.receipt_digest = Some(proof.digest);
    let unchanged = (|| {
        let (a, b, c) = controls(input)?;
        let current = capture(&input.root, &profile(&profile_bytes)?)?;
        Ok::<_, Failure>(
            manifest(&current) == plan.inputs
                && hash(&a) == plan.custody.policy_digest
                && hash(&b) == plan.custody.profile_digest
                && hash(&c) == plan.custody.catalog_digest,
        )
    })();
    match unchanged {
        Ok(true) => {}
        Ok(false) => {
            receipt.original_verification.state = "mutated".into();
            receipt.outcome = "security".into();
            receipt.reason_codes.push("original-custody-changed".into());
        }
        Err(_) => {
            receipt.original_verification.state = "unverifiable".into();
            receipt.outcome = "infrastructure".into();
            receipt
                .reason_codes
                .push("original-custody-unverifiable".into());
        }
    }
    if prepared.stage.close().is_err() {
        receipt.cleanup.state = "failed".into();
        if receipt.outcome != "security" {
            receipt.outcome = "infrastructure".into();
        }
        receipt.reason_codes.push("cleanup-failed".into());
    }
    // Include original custody verification and outer stage cleanup in the
    // terminal budget. No late cleanup/verification may turn into PASS.
    let late_reason = if cancellation.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        Some(super::evidence::reasons::CANCELLED)
    } else if execution_started.elapsed().as_millis()
        >= u128::from(plan.native_plan.limits.timeout_ms_per_run)
    {
        Some(super::evidence::reasons::RUN_DEADLINE)
    } else {
        None
    };
    if let Some(reason) = late_reason {
        if receipt.outcome != "security" {
            receipt.outcome = "infrastructure".into();
        }
        if !receipt.reason_codes.iter().any(|value| value == reason) {
            receipt.reason_codes.push(reason.into());
        }
    }
    receipt.verdict = super::evidence::rollup(&receipt);
    let run = super::validate_run_result(&canonical(&receipt)?)
        .map_err(|_| invalid("execution-result"))?;
    let outputs: Vec<_> = outputs
        .into_inner()
        .into_iter()
        .map(|(digest, output)| CommandOutput { digest, output })
        .collect();
    if run.commands.iter().any(|command|matches!(&command.output_ref,Some(super::types::NativeOutputRef::Digest(digest)) if !outputs.iter().any(|output|&output.digest==digest))) {return Err(Failure::RunInfrastructure);}
    let report = ExecutionReport {
        schema_version: "lekalo/native-execution-report/v1.0.0".into(),
        kind: "native-execution-report".into(),
        run,
        outputs,
    };
    if !super::execution_schema::valid(
        &canonical(&report)?,
        super::execution_schema::Document::Report,
    ) {
        return Err(invalid("execution-report"));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposed(
        inputs: Vec<ExecutionInput>,
        custody: ExecutionCustody,
        runtime_digest: &str,
    ) -> ExecutionPlan {
        let mut native = super::super::validate_plan(include_bytes!(
            "../../../../tests/fixtures/node-native-gates/protocol/plan.golden.json"
        ))
        .unwrap();
        native.input_manifest_digest = custody.source_manifest_digest.clone();
        native.adapter.digest = custody.adapter_entry_digest.clone();
        for command in &mut native.commands {
            command.read_manifest_ref = custody.source_manifest_digest.clone();
        }
        for tool in &mut native.tools {
            tool.version = Some("24.0.0".into());
            tool.artifact_digest = runtime_digest.into();
        }
        native.plan_digest = super::super::plan_digest(&native);
        let tools = native
            .tools
            .iter()
            .map(|tool| ExecutionTool {
                id: tool.id.clone(),
                artifact_digest: runtime_digest.into(),
                program_digest: runtime_digest.into(),
                version: "24.0.0".into(),
                entry: None,
                entry_digest: None,
            })
            .collect();
        let mut plan = ExecutionPlan {
            schema_version: PLAN_VERSION.into(),
            kind: "native-execution-plan".into(),
            plan_digest: String::new(),
            native_plan: native,
            custody,
            inputs,
            tools,
            assets: None,
        };
        plan.plan_digest = execution_digest(&plan).unwrap();
        validate_execution_plan(&canonical(&plan).unwrap()).unwrap()
    }

    fn setup(allow_execution: bool) -> (tempfile::TempDir, ExecutionInputs, ExecutionPlan) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("package.json"),
            br#"{"name":"@native/execution","version":"24.0.0"}"#,
        )
        .unwrap();
        std::fs::write(root.join("src/gate.mjs"), b"not executed").unwrap();
        let mut policy: Value = serde_json::from_slice(include_bytes!(
            "../../../../tests/fixtures/node-native-gates/protocol/policy.golden.json"
        ))
        .unwrap();
        for confirmation in policy["confirmations"].as_array_mut().unwrap() {
            confirmation["tool_ref"]
                .as_object_mut()
                .unwrap()
                .remove("entry_digest");
        }
        policy["env_recipe"] = json!({"allowed_names":[],"bindings":[]});
        let policy_bytes = canonical(&json!({"schema_version":POLICY_VERSION,"allow_execution":allow_execution,"platforms":["windows"],"runtime_profile":RUNTIME_PROFILE,"planning_policy":policy})).unwrap();
        let profile_bytes = canonical(&json!({"id":"standalone","mode":"observed","target":"node-typescript","readRoots":[{"path":"package.json","kind":"file"},{"path":"src","kind":"tree"}],"exclusions":[],"provenance":{"origin":"declared","revision":"test-1","disposition":"public-fixture"}})).unwrap();
        let program = temp
            .path()
            .join(if cfg!(windows) { "node.exe" } else { "node" });
        std::fs::write(&program, b"never an executable").unwrap();
        let program_digest = hash(b"never an executable");
        let catalog_bytes = canonical(&json!({"schema_version":CATALOG_VERSION,"tools":[{"id":"fixture-node","program":program,"artifact_digest":program_digest,"program_digest":program_digest,"version":"24.0.0"}]})).unwrap();
        let input = ExecutionInputs {
            root,
            adapter: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/node-typescript"),
            policy: temp.path().join("policy.json"),
            profile: temp.path().join("profile.json"),
            catalog: temp.path().join("catalog.json"),
        };
        std::fs::write(&input.policy, &policy_bytes).unwrap();
        std::fs::write(&input.profile, &profile_bytes).unwrap();
        std::fs::write(&input.catalog, &catalog_bytes).unwrap();
        let adapter = adapter_package::resolve(
            &DiscoverySource::Path(input.adapter.clone()),
            &ResolveContext {
                root: Some(input.root.clone()),
                offline: true,
            },
        )
        .unwrap();
        let manifest = &adapter.candidate.manifest;
        let inputs = manifest_inputs(&input.root, &profile_bytes);
        let custody = ExecutionCustody {
            policy_digest: hash(&policy_bytes),
            profile_digest: hash(&profile_bytes),
            catalog_digest: hash(&catalog_bytes),
            adapter_manifest_digest: hash(manifest.stored_bytes()),
            adapter_package_digest: manifest.package_digest().as_str().into(),
            adapter_entry_digest: manifest.files()[0].digest().as_str().into(),
            source_manifest_digest: hash(&canonical(&inputs).unwrap()),
        };
        let plan = proposed(inputs, custody, &program_digest);
        (temp, input, plan)
    }

    fn manifest_inputs(root: &Path, bytes: &[u8]) -> Vec<ExecutionInput> {
        manifest(&capture(root, &profile(bytes).unwrap()).unwrap())
    }

    #[test]
    fn absent_approval_and_explicit_denial_launch_nothing_even_with_fixture_labels() {
        let (_temp, input, plan) = setup(false);
        let before = crate::target_protocol::transport::launches();
        assert_eq!(
            run_execution_plan(&input, &canonical(&plan).unwrap(), "wrong", None),
            Err(Failure::ApprovalMissing)
        );
        assert_eq!(
            run_execution_plan(&input, &canonical(&plan).unwrap(), &plan.plan_digest, None),
            Err(Failure::TrustInsufficient)
        );
        assert_eq!(crate::target_protocol::transport::launches(), before);
    }

    #[test]
    fn changed_policy_bytes_refuse_before_adapter_or_gate_spawn() {
        let (_temp, input, plan) = setup(true);
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&input.policy)
            .unwrap()
            .write_all(b"\n")
            .unwrap();
        let before = crate::target_protocol::transport::launches();
        assert_eq!(
            run_execution_plan(&input, &canonical(&plan).unwrap(), &plan.plan_digest, None),
            Err(Failure::PlanStale)
        );
        assert_eq!(crate::target_protocol::transport::launches(), before);
    }

    #[cfg(windows)]
    #[test]
    fn changed_source_refuses_before_even_the_read_only_adapter_process() {
        let (_temp, input, plan) = setup(true);
        std::fs::write(input.root.join("src/gate.mjs"), b"changed").unwrap();
        let before = crate::target_protocol::transport::launches();
        assert_eq!(
            run_execution_plan(&input, &canonical(&plan).unwrap(), &plan.plan_digest, None),
            Err(Failure::PlanStale)
        );
        assert_eq!(crate::target_protocol::transport::launches(), before);
    }

    #[test]
    fn nested_duplicate_keys_and_escaped_aliases_never_collapse() {
        for document in [r#"{"a":{"x":1,"x":2}}"#, r#"{"a":{"x":1,"\u0078":2}}"#] {
            assert!(decode::<Value>(document.as_bytes()).is_err());
        }
    }
}
