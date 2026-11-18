//! A parent reads declared children. Consumer-owned pins are verified offline;
//! only the parent has access to both the accepted and the candidate contract.
use clap::Subcommand;
use lekalo_core::{
    module_api, project_fs,
    result::{DomainResult, Status},
    runtime,
};
use runtime::{config, error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const FILE: &str = "lekalo.workspace.yaml";
const PUBLIC: &str = "lekalo/public-contract/v1";

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Validate this project, its declared children and consumer-owned pins.
    Check {
        #[arg(long, default_value = ".")]
        project: String,
        #[arg(long)]
        strict: bool,
    },
    /// Print a declared contract for review and installation by a consumer.
    Export {
        name: String,
        #[arg(long, default_value = ".")]
        project: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    #[serde(default)]
    children: BTreeMap<String, String>,
    #[serde(default)]
    exports: BTreeMap<String, Export>,
    #[serde(default)]
    imports: BTreeMap<String, Import>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Export {
    #[serde(rename = "runtime-fact")]
    Fact {
        component: String,
        package: String,
        rule: String,
    },
    #[serde(rename = "module-api")]
    Module { api: String, module: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Import {
    from: String,
    export: String,
    bundle: String,
    digest: String,
}

fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

fn manifest(root: &Path) -> Result<Option<Manifest>> {
    match root.join(FILE).symlink_metadata() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(error("workspace-manifest-unreadable")),
        Ok(_) => (),
    }
    let m: Manifest =
        serde_json::from_value(config::value(&runtime::read(root, FILE, 128 * 1024)?)?)
            .map_err(|_| error("workspace-manifest-invalid"))?;
    if m.schema != "lekalo/workspace/v1"
        || m.children.len() > 32
        || m.exports.len() > 32
        || m.imports.len() > 32
        || m.children.is_empty() && m.exports.is_empty() && m.imports.is_empty()
        || m.children
            .keys()
            .chain(m.exports.keys())
            .chain(m.imports.keys())
            .any(|s| !id(s))
    {
        return Err(error("workspace-manifest-invalid"));
    }
    for spec in m.exports.values() {
        let valid = match spec {
            Export::Fact {
                component,
                package,
                rule,
            } => [component, package, rule].iter().all(|s| config::token(s)),
            Export::Module { api, module } => {
                project_fs::path_violation(api).is_none() && config::token(module)
            }
        };
        if !valid {
            return Err(error("workspace-export-invalid"));
        }
    }
    for spec in m.imports.values() {
        if !id(&spec.from)
            || !id(&spec.export)
            || project_fs::path_violation(&spec.bundle).is_some()
            || !lekalo_core::target_protocol::wire::is_sha256_digest(&spec.digest)
        {
            return Err(error("workspace-import-invalid"));
        }
    }
    Ok(Some(m))
}

fn child_root(root: &Path, relative: &str) -> Result<PathBuf> {
    if project_fs::path_violation(relative).is_some() {
        return Err(error("workspace-child-path"));
    }
    let fs = project_fs::Fs::open(root).map_err(|_| error("workspace-root"))?;
    let (dir, name) = relative.rsplit_once('/').unwrap_or(("", relative));
    if fs.entry_type(dir, name) != Ok(project_fs::EntryType::Directory) {
        return Err(error("workspace-child-path"));
    }
    let path = root.join(relative);
    // Canonical paths also catch Windows reparse points and aliases.
    if path.canonicalize().ok().as_ref() != Some(&path) {
        return Err(error("workspace-child-path"));
    }
    Ok(path)
}

fn publish(root: &Path, spec: &Export, name: &str) -> Result<Value> {
    let (kind, body, digest) = match spec {
        Export::Fact {
            component,
            package,
            rule,
        } => {
            let doc = runtime::export(root, component, package, rule)?;
            if doc["status"] != "valid" {
                return Err(error("workspace-export-rule-failed"));
            }
            (
                "runtime-fact",
                doc["contract"].clone(),
                doc["digest"].clone(),
            )
        }
        Export::Module { api, module } => {
            let bundle = crate::module_api::public_contract(
                root.to_str().ok_or_else(|| error("workspace-root"))?,
                api,
                module,
            )
            .map_err(|_| error("workspace-module-export-invalid"))?;
            let digest = json!(bundle.digest);
            ("module-api", json!(bundle), digest)
        }
    };
    Ok(
        json!({"schema":PUBLIC, "status":"valid", "name":name,"kind":kind,"contract":body,"digest":digest}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    schema: String,
    status: String,
    name: String,
    kind: String,
    contract: Value,
    digest: String,
}

fn accepted(root: &Path, spec: &Import) -> Result<()> {
    let bytes = runtime::read(root, &spec.bundle, 8 * 1024 * 1024)?;
    let b: Bundle = serde_json::from_value(config::value(&bytes)?)
        .map_err(|_| error("workspace-bundle-invalid"))?;
    if b.schema != PUBLIC || b.status != "valid" || b.name != spec.export || b.digest != spec.digest
    {
        return Err(error("workspace-pin-mismatch"));
    }
    let bytes = serde_json::to_vec(&b.contract).unwrap();
    let digest = match b.kind.as_str() {
        "module-api" => {
            module_api::parse_bundle(&bytes)
                .map_err(|_| error("workspace-bundle-invalid"))?
                .digest
        }
        "runtime-fact" => {
            let map = b
                .contract
                .as_object()
                .ok_or_else(|| error("workspace-bundle-invalid"))?;
            if map.len() != 5
                || !map.contains_key("fact")
                || !["component", "package", "rule"].iter().all(|k| {
                    map.get(*k)
                        .and_then(Value::as_str)
                        .is_some_and(config::token)
                })
                || !map
                    .get("rules_digest")
                    .and_then(Value::as_str)
                    .is_some_and(lekalo_core::target_protocol::wire::is_sha256_digest)
            {
                return Err(error("workspace-bundle-invalid"));
            }
            runtime::digest(&bytes)
        }
        _ => return Err(error("workspace-bundle-invalid")),
    };
    if digest != b.digest {
        return Err(error("workspace-pin-mismatch"));
    }
    Ok(())
}

fn fail(detail: &str) -> DomainResult {
    runtime::outcome(Err(error(detail)))
}

pub fn run(command: Command, no_cache: bool) -> DomainResult {
    match command {
        Command::Check { project, strict } => {
            if !Path::new(&project).join(FILE).exists() {
                return fail("workspace-manifest-required");
            }
            validate(&Some(project), &None, strict, no_cache, None, None)
        }
        Command::Export { project, name } => {
            let root = Path::new(&project);
            let result = manifest(root).and_then(|m| {
                let m = m.ok_or_else(|| error("workspace-manifest-required"))?;
                let spec = m
                    .exports
                    .get(&name)
                    .ok_or_else(|| error("workspace-export-unknown"))?;
                publish(root, spec, &name)
            });
            runtime::outcome(result)
        }
    }
}

pub fn validate(
    project: &Option<String>,
    module: &Option<String>,
    strict: bool,
    no_cache: bool,
    api: Option<String>,
    lock: Option<String>,
) -> DomainResult {
    let path = Path::new(project.as_deref().unwrap_or("."));
    let m = match manifest(path) {
        Ok(None) => return crate::validate_project(project, module, strict, no_cache, api, lock),
        Ok(Some(m)) => m,
        Err(e) => return runtime::outcome(Err(e)),
    };
    let root = match path.canonicalize() {
        Ok(r) => r,
        Err(_) => return fail("workspace-root"),
    };
    let scoped = module.is_some() || api.is_some();
    let mut local = None;
    if m.children.is_empty()
        || crate::runtime_config::configured(project.as_deref())
        || path.join("lekalo/project.yaml").symlink_metadata().is_ok()
        || scoped
    {
        let result = crate::validate_project(project, module, strict, no_cache, api, lock);
        if result.status() != Status::Valid {
            return result;
        }
        if scoped {
            local = Some(result);
        }
    }
    let result = check_members(&root, &m, strict, no_cache);
    match result {
        Ok((_, errors)) if !errors.is_empty() => {
            let diagnostics = errors
                .into_iter()
                .flat_map(|detail| fail(&detail).diagnostics().to_vec())
                .collect();
            match lekalo_core::diagnostics::DiagnosticSet::try_from_unsorted(
                diagnostics,
                Status::Invalid,
            ) {
                Ok(set) => DomainResult::invalid(set),
                Err(_) => fail("workspace-check-failed"),
            }
        }
        Ok((report, _)) => local
            .unwrap_or_else(|| DomainResult::receipt(report.to_string(), "workspace valid".into())),
        Err(e) => runtime::outcome(Err(e)),
    }
}

fn check_members(
    root: &Path,
    m: &Manifest,
    strict: bool,
    no_cache: bool,
) -> Result<(Value, Vec<String>)> {
    let mut errors = Vec::new();
    for (name, spec) in &m.imports {
        if let Err(e) = accepted(root, spec) {
            errors.push(format!("{}/{}", e.detail, name));
        }
    }
    let mut members = BTreeMap::new();
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut published = BTreeMap::new();
    let mut imports = Vec::new();
    for (name, relative) in &m.children {
        let path = child_root(root, relative)?;
        if roots
            .iter()
            .any(|p| p.starts_with(&path) || path.starts_with(p))
        {
            return Err(error("workspace-child-overlap"));
        }
        roots.push(path.clone());
        let child = manifest(&path)?;
        if child.as_ref().is_some_and(|c| !c.children.is_empty()) {
            return Err(error("workspace-nested-members"));
        }
        let project = Some(
            path.to_str()
                .ok_or_else(|| error("workspace-root"))?
                .to_owned(),
        );
        if !crate::runtime_config::configured(project.as_deref())
            && !path.join("lekalo/project.yaml").is_file()
        {
            return Err(error(&format!("workspace-child-project/{name}")));
        }
        let checked = crate::validate_project(&project, &None, strict, no_cache, None, None);
        let valid = checked.status() == Status::Valid;
        members.insert(name, checked.status().as_str());
        if !valid {
            errors.push(format!(
                "workspace-child-invalid/{name}/{}",
                checked
                    .diagnostics()
                    .first()
                    .map(|d| d.id())
                    .unwrap_or("unknown")
            ));
        }
        if let Some(c) = child {
            for (alias, spec) in c.imports {
                if let Err(e) = accepted(&path, &spec) {
                    errors.push(format!("{}/{name}/{alias}", e.detail));
                }
                imports.push((name, alias, spec));
            }
            published.insert(name, (path, c.exports, valid));
        }
    }
    let mut candidates = BTreeMap::new();
    let mut dependencies = Vec::new();
    for (consumer, alias, import) in imports {
        let key = (import.from.clone(), import.export.clone());
        let digest = candidates.entry(key).or_insert_with(|| {
            let (path, exports, valid) = published.get(&import.from)?;
            if !valid {
                return None;
            }
            let doc = publish(path, exports.get(&import.export)?, &import.export).ok()?;
            doc["digest"].as_str().map(str::to_owned)
        });
        if digest.as_ref() == Some(&import.digest) {
            dependencies.push(json!({"consumer":consumer,"import":alias,"producer":import.from,"export":import.export,"digest":import.digest}));
        } else {
            let reason = if digest.is_some() {
                "workspace-contract-changed"
            } else {
                "workspace-export-unavailable"
            };
            errors.push(format!("{reason}/{consumer}/{alias}/{}", import.from));
        }
    }
    Ok((
        json!({"schema":"lekalo/workspace-check/v1","status":"valid","children":members,"local_imports":m.imports.len(),"dependencies":dependencies}),
        errors,
    ))
}
