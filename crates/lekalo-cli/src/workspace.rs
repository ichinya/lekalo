//! A parent reads declared children. Consumer-owned pins are verified offline;
//! only the parent has access to both the accepted and the candidate contract.
use clap::Subcommand;
use lekalo_core::{
    module_api, project_fs,
    result::{DomainResult, Status},
    runtime,
};
use runtime::{config, error, Result};
use serde::{Deserialize, Serialize};
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

struct Member<'a> {
    name: &'a str,
    relative: &'a str,
    path: PathBuf,
    selector: String,
    manifest: Option<Manifest>,
}

fn members<'a>(root: &Path, selector: &str, parent: &'a Manifest) -> Result<Vec<Member<'a>>> {
    let mut members: Vec<Member<'a>> = Vec::new();
    for (name, relative) in &parent.children {
        let path = child_root(root, relative)?;
        if members
            .iter()
            .any(|member| member.path.starts_with(&path) || path.starts_with(&member.path))
        {
            return Err(error("workspace-child-overlap"));
        }
        let child = manifest(&path)?;
        if child
            .as_ref()
            .is_some_and(|manifest| !manifest.children.is_empty())
        {
            return Err(error("workspace-nested-members"));
        }
        // Model commands retain the invocation-relative selector. File reads
        // use the separately confined absolute child root.
        let selector = format!("{}/{relative}", selector.trim_end_matches('/'));
        members.push(Member {
            name,
            relative,
            path,
            selector,
            manifest: child,
        });
    }
    Ok(members)
}

fn publish(root: &Path, selector: &str, spec: &Export, name: &str) -> Result<Bundle> {
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
                doc["digest"]
                    .as_str()
                    .ok_or_else(|| error("workspace-export-rule-failed"))?
                    .to_owned(),
            )
        }
        Export::Module { api, module } => {
            let bundle = crate::module_api::public_contract(selector, api, module)
                .map_err(|_| error("workspace-module-export-invalid"))?;
            let digest = bundle.digest.clone();
            ("module-api", json!(bundle), digest)
        }
    };
    Ok(Bundle {
        schema: PUBLIC.into(),
        status: "valid".into(),
        name: name.into(),
        kind: kind.into(),
        contract: body,
        digest,
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    schema: String,
    status: String,
    name: String,
    kind: String,
    contract: Value,
    digest: String,
}

fn accepted(root: &Path, spec: &Import) -> Result<Bundle> {
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
    Ok(b)
}

fn compatibility(pinned: &Bundle, candidate: &Bundle) -> &'static str {
    if pinned.kind != candidate.kind {
        return "kind-changed";
    }
    if pinned.kind != "module-api" {
        return "unknown";
    }
    let before = serde_json::to_vec(&pinned.contract).expect("public bundle serializes");
    let after = serde_json::to_vec(&candidate.contract).expect("public bundle serializes");
    match module_api::compare_public(&before, &after, None, None) {
        Ok((report, _)) => match report.verdict.as_str() {
            "breaking" => "breaking",
            "compatible" => "compatible",
            "policy-denied" => "policy-denied",
            _ => "unknown",
        },
        // Two valid module bundles can identify different producers.
        Err(_) => "unknown",
    }
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
                publish(root, &project, spec, &name).map(|bundle| json!(bundle))
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
    let selector = project.as_deref().unwrap_or(".");
    let path = Path::new(selector);
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
    let result = check_members(&root, selector, &m, strict, no_cache);
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
    selector: &str,
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
    let mut checked_members = BTreeMap::new();
    let mut published = BTreeMap::new();
    let mut imports: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for Member {
        name,
        path,
        selector: child_selector,
        manifest: child,
        ..
    } in members(root, selector, m)?
    {
        let project = Some(child_selector.clone());
        if !crate::runtime_config::configured(project.as_deref())
            && !path.join("lekalo/project.yaml").is_file()
        {
            return Err(error(&format!("workspace-child-project/{name}")));
        }
        let checked = crate::validate_project(&project, &None, strict, no_cache, None, None);
        let valid = checked.status() == Status::Valid;
        checked_members.insert(name, checked.status().as_str());
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
                imports
                    .entry((spec.from.clone(), spec.export.clone()))
                    .or_default()
                    .push((name, alias, spec, path.clone()));
            }
            published.insert(name, (path, child_selector, c.exports, valid));
        }
    }
    let mut dependencies = Vec::new();
    for ((producer, export), consumers) in imports {
        let candidate = std::cell::OnceCell::new();
        for (consumer, alias, import, root) in consumers {
            let pinned = match accepted(&root, &import) {
                Ok(bundle) => bundle,
                Err(e) => {
                    errors.push(format!("{}/{consumer}/{alias}", e.detail));
                    continue;
                }
            };
            let candidate = candidate.get_or_init(|| {
                let (path, selector, exports, valid) = published.get(producer.as_str())?;
                if !valid {
                    return None;
                }
                publish(path, selector, exports.get(&export)?, &export).ok()
            });
            match candidate {
                Some(bundle) if bundle.kind == pinned.kind && bundle.digest == pinned.digest => {
                    dependencies.push((consumer, alias, import));
                }
                Some(bundle) => errors.push(format!(
                    "workspace-contract-changed/{consumer}/{alias}/{producer}/{}",
                    compatibility(&pinned, bundle)
                )),
                None => errors.push(format!(
                    "workspace-export-unavailable/{consumer}/{alias}/{producer}"
                )),
            }
        }
    }
    dependencies.sort_by(|left, right| (left.0, &left.1).cmp(&(right.0, &right.1)));
    let dependencies: Vec<_> = dependencies.into_iter().map(|(consumer, alias, spec)| {
        json!({"consumer":consumer,"import":alias,"producer":spec.from,"export":spec.export,"digest":spec.digest})
    }).collect();
    Ok((
        json!({"schema":"lekalo/workspace-check/v1","status":"valid","children":checked_members,"local_imports":m.imports.len(),"dependencies":dependencies}),
        errors,
    ))
}

/// Report pins and accepted bundles without running a runtime or restoring packages.
pub fn doctor(selector: &str) -> DomainResult {
    match inspect_workspace(selector) {
        Ok(report) => {
            let count = report["children"]
                .as_object()
                .map_or(0, |children| children.len());
            let verdict = report["verdict"]
                .as_str()
                .expect("workspace doctor verdict");
            let human = format!("workspace doctor {verdict} ({count} children)");
            DomainResult::receipt(serde_json::to_string_pretty(&report).unwrap(), human)
        }
        Err(error) => runtime::outcome(Err(error)),
    }
}

fn inspect_workspace(selector: &str) -> Result<Value> {
    let root = crate::runtime_config::resolve_root(Path::new(selector))?;
    let workspace = manifest(&root)?.ok_or_else(|| error("workspace-manifest-required"))?;
    let members = members(&root, selector, &workspace)?;
    let mut blocked = false;
    let mut pending = false;
    let mut children = BTreeMap::new();
    let mut edges = Vec::new();
    for (alias, import) in &workspace.imports {
        let edge = inspect_import(&root, None, alias, import, &members);
        blocked |= edge["state"] == "blocked";
        edges.push(edge);
    }
    for member in &members {
        let has_model = member
            .path
            .join("lekalo/project.yaml")
            .symlink_metadata()
            .is_ok();
        let model = if has_model {
            let result = crate::run_validate(Some(member.selector.clone()), None, false, true);
            json!({"state":result.status().as_str(), "diagnostics":result.diagnostics().iter().map(|d| d.id()).collect::<Vec<_>>()})
        } else {
            json!({"state":"not-configured", "diagnostics":[]})
        };
        let configured = crate::runtime_config::configured(Some(&member.selector));
        let runtime = if configured {
            match runtime::check_lock(&member.path) {
                Ok(lock) => runtime_inventory(&lock),
                Err(error) => json!({"state":"blocked", "reason":error.detail}),
            }
        } else {
            json!({"state":"not-configured"})
        };
        let schema_pins = match lekalo_core::lockfile::plan::LockService::check_project_schemas_at(
            &member.path,
        ) {
            Ok(_) => json!({"state":"valid"}),
            Err(failure) => {
                let result = DomainResult::from(&failure);
                blocked = true;
                json!({"state":result.status().as_str(), "diagnostics":result.diagnostics().iter().map(|d| d.id()).collect::<Vec<_>>()})
            }
        };
        blocked |= (!has_model && !configured)
            || (has_model && model["state"] != "valid")
            || runtime["state"] == "blocked";
        pending |= runtime["state"] == "pinned";
        children.insert(
            member.name,
            json!({"path":member.relative, "model":model, "runtime":runtime, "schemaPins":schema_pins}),
        );
        if let Some(child) = &member.manifest {
            for (alias, import) in &child.imports {
                let edge = inspect_import(&member.path, Some(member.name), alias, import, &members);
                blocked |= matches!(edge["state"].as_str(), Some("blocked" | "changed"));
                pending |= edge["state"] == "not-checked";
                edges.push(edge);
            }
        }
    }
    let verdict = if blocked {
        "blocked"
    } else if pending {
        "incomplete"
    } else {
        "ready"
    };
    Ok(json!({
        "status":"valid",
        "schemaVersion":"lekalo/workspace-doctor/v1.3.1",
        "identity":"dev.lekalo.workspace-doctor@1.3.1",
        "verdict":verdict,
        "children":children,
        "contracts":edges,
    }))
}

fn runtime_inventory(lock: &runtime::Lock) -> Value {
    let components: BTreeMap<_, _> =
        lock.components
            .iter()
            .map(|(id, pin)| {
                let rules: Vec<_> = pin.rules.iter().map(|rule| json!({
            "id":rule.manifest.id, "version":rule.manifest.version, "digest":rule.digest
        })).collect();
                (
                    id,
                    json!({"runtime":runtime_pin(&pin.runtime), "rules":rules}),
                )
            })
            .collect();
    let services: BTreeMap<_, _> = lock
        .services
        .iter()
        .map(|(id, pin)| (id, runtime_pin(pin)))
        .collect();
    json!({"state":"pinned", "sourceCheck":"not-run", "configDigest":lock.config_digest,
        "components":components, "services":services})
}

fn runtime_pin(pin: &runtime::RuntimePin) -> Value {
    json!({"id":pin.description.id, "version":pin.description.version,
        "executableDigest":pin.files[&pin.command[0]],
        "packageDigest":pin.package.as_ref().map(|package| &package.digest)})
}

fn inspect_import(
    root: &Path,
    consumer: Option<&str>,
    alias: &str,
    import: &Import,
    members: &[Member<'_>],
) -> Value {
    let (state, detail) = match accepted(root, import) {
        Err(error) => ("blocked", error.detail),
        Ok(_) if consumer.is_none() => ("pinned", "accepted-bundle".into()),
        Ok(pinned) => {
            let export = members
                .iter()
                .find(|member| member.name == import.from)
                .and_then(|member| {
                    Some((
                        member,
                        member.manifest.as_ref()?.exports.get(&import.export)?,
                    ))
                });
            match export {
                Some((_, Export::Fact { .. })) if pinned.kind != "runtime-fact" => {
                    ("changed", "kind-changed".into())
                }
                Some((_, Export::Fact { .. })) => ("not-checked", "runtime-export".into()),
                Some((member, spec @ Export::Module { .. })) => {
                    match publish(&member.path, &member.selector, spec, &import.export) {
                        Ok(candidate)
                            if pinned.kind == candidate.kind
                                && pinned.digest == candidate.digest =>
                        {
                            ("current", "matching-export".into())
                        }
                        Ok(candidate) => ("changed", compatibility(&pinned, &candidate).into()),
                        Err(error) => ("blocked", error.detail),
                    }
                }
                None => ("blocked", "workspace-export-unavailable".into()),
            }
        }
    };
    json!({"consumer":consumer, "import":alias, "producer":import.from, "export":import.export,
        "digest":import.digest, "state":state, "detail":detail})
}
