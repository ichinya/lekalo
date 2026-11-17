//! External runtimes and portable contract rules. No language registry.
pub mod config;
mod package;
pub mod services;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const PROTOCOL: &str = "lekalo/runtime/v1";
pub const RULES_API: &str = "lekalo/contract-rules/v1";
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Serialize)]
pub struct Error {
    pub code: String,
    pub status: String,
    pub detail: String,
}
impl Error {
    fn version(detail: &str) -> Self {
        Self {
            code: "runtime.incompatible".into(),
            status: "unsupported-version".into(),
            detail: detail.into(),
        }
    }
    pub fn exit_code(&self) -> u8 {
        if self.status == "unsupported-version" {
            5
        } else {
            1
        }
    }
}
pub fn error(detail: &str) -> Error {
    Error {
        code: "runtime.refused".into(),
        status: "invalid".into(),
        detail: detail.into(),
    }
}
pub fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", crate::digest::sha256_hex(bytes))
}

/// Project runtime results through the shared CLI diagnostic and exit contract.
pub fn outcome(result: Result<Value>) -> crate::result::DomainResult {
    use crate::diagnostics::{normalize::build, types::token_value, DataObject};
    use crate::result::{DomainResult, Status};
    let (status, diagnostics) = match result {
        Ok(report) if report["status"] == "valid" => {
            return DomainResult::receipt(
                serde_json::to_string_pretty(&report).unwrap(),
                "runtime valid".into(),
            )
        }
        Ok(report) => {
            let mut diagnostics = Vec::new();
            for component in report["components"].as_array().into_iter().flatten() {
                for finding in component["findings"].as_array().into_iter().flatten() {
                    let mut data = DataObject::new();
                    for (key, value) in [
                        ("component", &component["component"]),
                        ("package", &finding["package"]),
                        ("rule", &finding["rule"]),
                    ] {
                        data.insert(key.into(), token_value(value.as_str().unwrap_or("unknown")));
                    }
                    diagnostics.push(
                        build("runtime.rule-failed", None, None, data)
                            .expect("registered runtime diagnostic"),
                    );
                }
            }
            (Status::Invalid, diagnostics)
        }
        Err(error) => {
            let status = if error.status == "unsupported-version" {
                Status::UnsupportedVersion
            } else {
                Status::Invalid
            };
            let mut data = DataObject::new();
            data.insert("detail".into(), token_value(&error.detail));
            (
                status,
                vec![build(&error.code, None, None, data).expect("registered runtime diagnostic")],
            )
        }
    };
    let diagnostics = DomainResult::from_wire_set(status, diagnostics);
    if status == Status::UnsupportedVersion {
        DomainResult::UnsupportedVersion { diagnostics }
    } else {
        DomainResult::invalid(diagnostics)
    }
}

/// Read only a regular file through a symlink-free project-relative path.
pub fn read(root: &Path, path: &str, limit: usize) -> Result<Vec<u8>> {
    if !config::relative(path) {
        return Err(error("path-invalid"));
    }
    let path = Path::new(path);
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| error("path-invalid"))?;
    let directory = path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_string_lossy();
    crate::project_fs::Fs::open(root)
        .map_err(|_| error("directory-unreadable"))?
        .read_file_opt(&directory, name, limit)
        .map_err(|_| error("file-unreadable"))?
        .ok_or_else(|| error("file-missing"))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Description {
    pub id: String,
    pub version: String,
    pub rules_api: Vec<String>,
    pub capabilities: Vec<String>,
    pub operations: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePin {
    pub command: Vec<String>,
    pub files: BTreeMap<String, String>,
    pub description: Description,
    pub package: Option<package::Package>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentPin {
    pub runtime: RuntimePin,
    pub rules: Vec<package::Package>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lock {
    pub schema: String,
    pub config_digest: String,
    pub components: BTreeMap<String, ComponentPin>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub services: BTreeMap<String, RuntimePin>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub schema: String,
    pub requires: Vec<String>,
    pub checks: Vec<Check>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub id: String,
    pub schema: Value,
}

fn rules(package: &package::Package) -> Result<Rules> {
    package::verify(package)?;
    let bytes = read(
        &package::root(package)?,
        package
            .manifest
            .rules
            .as_deref()
            .ok_or_else(|| error("rules-entry-missing"))?,
        1024 * 1024,
    )?;
    let rules: Rules =
        serde_json::from_value(config::value(&bytes)?).map_err(|_| error("rules-invalid"))?;
    if rules.schema != RULES_API {
        return Err(Error::version("rules-api"));
    }
    if rules.requires.len() > 64
        || rules.requires.iter().any(|s| !config::token(s))
        || rules.checks.is_empty()
        || rules.checks.len() > 256
        || rules.checks.iter().any(|c| !config::token(&c.id))
        || rules
            .checks
            .iter()
            .map(|c| &c.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != rules.checks.len()
    {
        return Err(error("rules-invalid"));
    }
    for check in &rules.checks {
        schema(&check.schema)?;
    }
    Ok(rules)
}

fn schema(value: &Value) -> Result<jsonschema::Validator> {
    fn admit(value: &Value) -> bool {
        match value {
            Value::Object(map) => map.iter().all(|(key, value)| {
                if matches!(key.as_str(), "$ref" | "$dynamicRef" | "$recursiveRef") {
                    value
                        .as_str()
                        .is_some_and(|s| s == "#" || s.starts_with("#/"))
                } else if key == "$schema" {
                    value.as_str() == Some("https://json-schema.org/draft/2020-12/schema")
                } else if key == "$id" {
                    false
                } else if matches!(
                    key.as_str(),
                    "properties"
                        | "patternProperties"
                        | "$defs"
                        | "definitions"
                        | "dependentSchemas"
                ) {
                    value.as_object().is_some_and(|map| map.values().all(admit))
                } else if matches!(key.as_str(), "allOf" | "anyOf" | "oneOf" | "prefixItems") {
                    value
                        .as_array()
                        .is_some_and(|items| items.iter().all(admit))
                } else if matches!(
                    key.as_str(),
                    "not"
                        | "if"
                        | "then"
                        | "else"
                        | "items"
                        | "contains"
                        | "additionalProperties"
                        | "unevaluatedProperties"
                        | "unevaluatedItems"
                        | "propertyNames"
                        | "contentSchema"
                ) {
                    admit(value)
                } else {
                    true
                }
            }),
            Value::Array(items) => items.iter().all(admit),
            _ => true,
        }
    }
    if !admit(value) {
        return Err(error("rules-schema-reference"));
    }
    jsonschema::draft202012::new(value).map_err(|_| error("rules-schema-invalid"))
}

fn executable(project: &Path, name: &str) -> Result<PathBuf> {
    let candidate = |path: PathBuf| {
        if path.is_file() {
            Some(path)
        } else if cfg!(windows)
            && path.extension().is_none()
            && path.with_extension("exe").is_file()
        {
            Some(path.with_extension("exe"))
        } else {
            None
        }
    };
    let found = if Path::new(name).is_absolute() || name.contains(['/', '\\']) {
        candidate(project.join(name))
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .find_map(|p| candidate(p.join(name)))
    }
    .ok_or_else(|| error("executable-missing"))?;
    std::fs::canonicalize(found).map_err(|_| error("executable-missing"))
}

fn executable_digest(path: &Path) -> Result<String> {
    let parent = path.parent().ok_or_else(|| error("executable-invalid"))?;
    Ok(digest(&read(
        parent,
        path.file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| error("executable-invalid"))?,
        256 * 1024 * 1024,
    )?))
}

fn pin_command(
    project: &Path,
    argv: &[String],
    package: Option<&package::Package>,
) -> Result<(Vec<String>, BTreeMap<String, String>)> {
    if argv.is_empty()
        || argv.len() > 16
        || argv
            .iter()
            .any(|a| a.is_empty() || a.len() > 4096 || a.contains('\0'))
    {
        return Err(error("command-invalid"));
    }
    let base = if let Some(package) = package {
        package::root(package)?
    } else {
        project.to_path_buf()
    };
    let program = if package.is_some_and(|p| p.manifest.files.contains(&argv[0])) {
        base.join(&argv[0])
    } else {
        executable(project, &argv[0])?
    };
    let program = program.to_string_lossy().into_owned();
    let mut files = BTreeMap::from([(program.clone(), executable_digest(Path::new(&program))?)]);
    let mut command = vec![program];
    for (index, arg) in argv.iter().enumerate().skip(1) {
        if index == 1 && base.join(arg).is_file() {
            let path = std::fs::canonicalize(base.join(arg)).map_err(|_| error("entry-missing"))?;
            files.insert(
                path.to_string_lossy().into_owned(),
                executable_digest(&path)?,
            );
            command.push(path.to_string_lossy().into_owned());
        } else {
            command.push(arg.clone());
        }
    }
    Ok((command, files))
}

fn verify_runtime(pin: &RuntimePin) -> Result<()> {
    if pin.command.is_empty() || !pin.files.contains_key(&pin.command[0]) || pin.files.len() > 2 {
        return Err(error("runtime-lock-invalid"));
    }
    if let Some(package) = &pin.package {
        package::verify(package)?;
    }
    verify_executables(&pin.files)
}

fn verify_executables(files: &BTreeMap<String, String>) -> Result<()> {
    for (path, hash) in files {
        if executable_digest(Path::new(path))? != *hash {
            return Err(error("runtime-changed"));
        }
    }
    Ok(())
}

fn exchange(
    root: &Path,
    command: &[String],
    package: Option<&package::Package>,
    operation: &str,
    input: Value,
) -> Result<Value> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Response {
        protocol: String,
        request_id: String,
        result: Value,
    }
    let id = digest(&serde_json::to_vec(&json!({"operation":operation,"input":input})).unwrap());
    let request = serde_json::to_vec(
        &json!({"protocol":PROTOCOL,"request_id":id,"operation":operation,"input":input}),
    )
    .unwrap();
    let limits = crate::target_protocol::transport::TransportLimits {
        timeout_ms: 30_000,
        max_request_bytes: 8 * 1024 * 1024,
        max_output_bytes: 8 * 1024 * 1024,
        ..Default::default()
    };
    let package_root = package.map(package::root).transpose()?;
    let response = crate::target_protocol::runtime_exchange(
        &crate::target_protocol::transport::AdapterCommand {
            program: command[0].clone().into(),
            args: command[1..].to_vec(),
        },
        &request,
        root,
        &limits,
        package
            .zip(package_root.as_deref())
            .map(|(p, root)| (root, &p.files)),
    )
    .map_err(|_| error("runtime-execution-failed"))?;
    if response.exit_code != 0 {
        return Err(error("runtime-exit-failed"));
    }
    // The config permits YAML; the process wire is exactly one JSON object.
    serde_json::from_slice::<Value>(&response.stdout)
        .map_err(|_| error("runtime-response-invalid"))?;
    let response: Response = serde_json::from_value(config::value(&response.stdout)?)
        .map_err(|_| error("runtime-response-invalid"))?;
    if response.protocol != PROTOCOL || response.request_id != id {
        return Err(error("runtime-response-mismatch"));
    }
    Ok(response.result)
}

fn describe_service(
    root: &Path,
    command: &[String],
    package: Option<&package::Package>,
) -> Result<Description> {
    let d: Description =
        serde_json::from_value(exchange(root, command, package, "describe", json!({}))?)
            .map_err(|_| error("runtime-description-invalid"))?;
    if !config::token(&d.id)
        || semver::Version::parse(&d.version).is_err()
        || [&d.capabilities, &d.rules_api, &d.operations]
            .iter()
            .any(|values| {
                values.len() > 128
                    || values.iter().any(|s| !config::token(s))
                    || values
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        != values.len()
            })
    {
        return Err(Error::version("runtime-api"));
    }
    Ok(d)
}

fn describe(
    root: &Path,
    command: &[String],
    package: Option<&package::Package>,
) -> Result<Description> {
    let d = describe_service(root, command, package)?;
    if !d.rules_api.iter().any(|s| s == RULES_API) || !d.operations.iter().any(|s| s == "check") {
        return Err(Error::version("runtime-api"));
    }
    Ok(d)
}
fn service_pin(root: &Path, source: &config::Source) -> Result<RuntimePin> {
    let session = Session::new(root, source)?;
    let (command, files) = pin_command(root, &session.command, session.package.as_ref())?;
    let description = describe_service(root, &command, session.package.as_ref())?;
    if session.package.as_ref().is_some_and(|p| {
        p.manifest.id != description.id || p.manifest.version != description.version
    }) {
        return Err(error("runtime-identity-mismatch"));
    }
    let pin = RuntimePin {
        command,
        files,
        description,
        package: session.package,
    };
    verify_runtime(&pin)?;
    Ok(pin)
}

pub fn load(root: &Path) -> Result<config::Config> {
    config::decode(&read(root, config::CONFIG, 128 * 1024)?)
}
fn config_digest(config: &config::Config) -> String {
    digest(&serde_json::to_vec(config).unwrap())
}

fn compatible(runtime: &Description, package: &package::Package) -> Result<Rules> {
    let r = rules(package)?;
    if r.requires.iter().any(|c| !runtime.capabilities.contains(c)) {
        return Err(Error::version("rules-capability-missing"));
    }
    Ok(r)
}

pub fn lock(root: &Path, offline: bool) -> Result<Lock> {
    let config = load(root)?;
    if config.components.is_empty() && config.services.is_empty() {
        return Err(error("components-empty"));
    }
    if offline {
        return check_lock(root);
    }
    let mut components = BTreeMap::new();
    for (id, component) in &config.components {
        let package = match &component.runtime {
            config::Source::Package(source) => {
                Some(package::resolve(root, source, "runtime", false)?)
            }
            config::Source::Executable(_) => None,
        };
        let argv = match (&component.runtime, &package) {
            (_, Some(p)) => &p.manifest.command,
            (config::Source::Executable(e), _) => &e.command,
            _ => unreachable!(),
        };
        let (command, files) = pin_command(root, argv, package.as_ref())?;
        let description = describe(root, &command, package.as_ref())?;
        if package.as_ref().is_some_and(|p| {
            p.manifest.id != description.id || p.manifest.version != description.version
        }) {
            return Err(error("runtime-identity-mismatch"));
        }
        let runtime = RuntimePin {
            command,
            files,
            description,
            package,
        };
        verify_runtime(&runtime)?;
        let mut selected_rules = Vec::new();
        for source in &component.rules {
            let package = package::resolve(root, source, "rules", false)?;
            compatible(&runtime.description, &package)?;
            if selected_rules
                .iter()
                .any(|p: &package::Package| p.manifest.id == package.manifest.id)
            {
                return Err(error("rules-duplicate"));
            }
            selected_rules.push(package);
        }
        components.insert(
            id.clone(),
            ComponentPin {
                runtime,
                rules: selected_rules,
            },
        );
    }
    let services = config
        .services
        .iter()
        .map(|(id, source)| Ok((id.clone(), service_pin(root, source)?)))
        .collect::<Result<_>>()?;
    let lock = Lock {
        schema: "lekalo/runtime-lock/v1".into(),
        config_digest: config_digest(&config),
        components,
        services,
    };
    write_lock(root, &lock)?;
    Ok(lock)
}

fn write_lock(root: &Path, lock: &Lock) -> Result<()> {
    use std::io::Write;
    // Never publish a lock that check_lock cannot read. Preserve an existing
    // usable lock when a composition exceeds the shared document budget.
    let bytes = serde_json::to_vec_pretty(lock).map_err(|_| error("lock-write-failed"))?;
    if bytes.len() > LOCK_LIMIT {
        return Err(error("lock-limit"));
    }
    let path = root.join(config::LOCK);
    if std::fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
        return Err(error("lock-path-invalid"));
    }
    let mut temporary =
        tempfile::NamedTempFile::new_in(root).map_err(|_| error("lock-write-failed"))?;
    temporary
        .write_all(&bytes)
        .map_err(|_| error("lock-write-failed"))?;
    temporary
        .persist(path)
        .map_err(|_| error("lock-write-failed"))?;
    Ok(())
}

const LOCK_LIMIT: usize = 8 * 1024 * 1024;

pub fn check_lock(root: &Path) -> Result<Lock> {
    let config = load(root)?;
    if config.components.is_empty() && config.services.is_empty() {
        return Err(error("components-empty"));
    }
    let lock: Lock = serde_json::from_value(config::value(&read(root, config::LOCK, LOCK_LIMIT)?)?)
        .map_err(|_| error("lock-invalid"))?;
    if lock.schema != "lekalo/runtime-lock/v1"
        || lock.config_digest != config_digest(&config)
        || !lock.components.keys().eq(config.components.keys())
        || !lock.services.keys().eq(config.services.keys())
    {
        return Err(error("lock-stale"));
    }
    for (id, pin) in &lock.services {
        verify_runtime(pin)?;
        let (command, files) = match &config.services[id] {
            config::Source::Executable(e) if pin.package.is_none() => {
                pin_command(root, &e.command, None)?
            }
            config::Source::Package(source) => {
                let p = pin.package.as_ref().ok_or_else(|| error("lock-stale"))?;
                if *source != p.source || p.manifest.kind != "runtime" {
                    return Err(error("lock-stale"));
                }
                pin_command(root, &p.manifest.command, Some(p))?
            }
            _ => return Err(error("lock-stale")),
        };
        if command != pin.command || files != pin.files {
            return Err(error("runtime-changed"));
        }
    }
    for (id, pin) in &lock.components {
        verify_runtime(&pin.runtime)?;
        let declaration = &config.components[id];
        if declaration.rules.len() != pin.rules.len() {
            return Err(error("lock-stale"));
        }
        for (source, p) in declaration.rules.iter().zip(&pin.rules) {
            if *source != p.source || p.manifest.kind != "rules" {
                return Err(error("lock-stale"));
            }
            compatible(&pin.runtime.description, p)?;
        }
        let (command, files) = match &declaration.runtime {
            config::Source::Executable(e) if pin.runtime.package.is_none() => {
                pin_command(root, &e.command, None)?
            }
            config::Source::Package(source) => {
                let p = pin
                    .runtime
                    .package
                    .as_ref()
                    .ok_or_else(|| error("lock-stale"))?;
                if *source != p.source || p.manifest.kind != "runtime" {
                    return Err(error("lock-stale"));
                }
                pin_command(root, &p.manifest.command, Some(p))?
            }
            _ => return Err(error("lock-stale")),
        };
        if command != pin.runtime.command || files != pin.runtime.files {
            return Err(error("runtime-changed"));
        }
    }
    Ok(lock)
}

fn sources(root: &Path, component: &config::Component) -> Result<Vec<Value>> {
    use crate::project_fs::{EntryType, Fs};
    let fs = Fs::open(root).map_err(|_| error("project-unreadable"))?;
    let base = Path::new(&component.path)
        .components()
        .filter_map(|part| match part {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    let mut files = Vec::new();
    let mut total = 0;
    let mut seen = 0;
    let mut pending = vec![(base.clone(), 0usize)];
    while let Some((dir, depth)) = pending.pop() {
        if depth > 32 {
            return Err(error("source-depth-limit"));
        }
        let entries = fs
            .entries_bounded(&dir, 4096)
            .map_err(|_| error("source-unreadable"))?;
        for (name, kind) in entries {
            seen += 1;
            if seen > 4096 {
                return Err(error("source-entry-limit"));
            }
            if name.starts_with('.') || matches!(name.as_str(), config::CONFIG | config::LOCK) {
                continue;
            }
            let path = if dir.is_empty() {
                name.clone()
            } else {
                format!("{dir}/{name}")
            };
            if kind == EntryType::Directory {
                pending.push((path, depth + 1));
                continue;
            }
            if kind != EntryType::File {
                return Err(error("source-file-type"));
            }
            let relative = if base.is_empty() {
                path
            } else {
                path[base.len() + 1..].to_owned()
            };
            let bytes = fs
                .read_file_opt(&dir, &name, 1024 * 1024)
                .map_err(|_| error("source-unreadable"))?
                .ok_or_else(|| error("source-changed"))?;
            total += bytes.len();
            if files.len() >= 1024 || total > 4 * 1024 * 1024 {
                return Err(error("source-size-limit"));
            }
            let hash = digest(&bytes);
            let content = String::from_utf8(bytes).map_err(|_| error("source-encoding"))?;
            files.push(json!({"path":relative,"digest":hash,"content":content}));
        }
    }
    files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(files)
}

pub fn check(root: &Path, selected: Option<&str>) -> Result<Value> {
    let config = load(root)?;
    let lock = check_lock(root)?;
    if selected.is_some_and(|id| !config.components.contains_key(id)) {
        return Err(error("component-unknown"));
    }
    let mut reports = Vec::new();
    let mut finding_count = 0;
    for (id, component) in &config.components {
        if selected.is_some_and(|selected| selected != id) {
            continue;
        }
        let pin = &lock.components[id];
        let current = describe(root, &pin.runtime.command, pin.runtime.package.as_ref())?;
        if serde_json::to_value(&current).unwrap()
            != serde_json::to_value(&pin.runtime.description).unwrap()
        {
            return Err(error("runtime-description-changed"));
        }
        let packs: Vec<Value> = pin.rules.iter().map(|p| Ok(json!({"id":p.manifest.id,"version":p.manifest.version,"digest":p.digest,"rules":compatible(&current,p)?}))).collect::<Result<_>>()?;
        let files = sources(root, component)?;
        let input = json!({"component":id,"files":files,"rules":packs});
        let source_digest = digest(&serde_json::to_vec(&input["files"]).unwrap());
        let response = exchange(
            root,
            &pin.runtime.command,
            pin.runtime.package.as_ref(),
            "check",
            input,
        )?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ResultRecord {
            state: String,
            facts: BTreeMap<String, BTreeMap<String, Value>>,
        }
        let result: ResultRecord =
            serde_json::from_value(response).map_err(|_| error("runtime-result-invalid"))?;
        if result.state != "complete" {
            return Err(error("runtime-incomplete"));
        }
        if result.facts.len() != pin.rules.len() {
            return Err(error("rules-result-missing"));
        }
        let mut findings = Vec::new();
        for package in &pin.rules {
            let pack = compatible(&current, package)?;
            let facts = result
                .facts
                .get(&package.manifest.id)
                .ok_or_else(|| error("rules-result-missing"))?;
            if facts.len() != pack.checks.len() {
                return Err(error("rules-result-missing"));
            }
            for check in pack.checks {
                let fact = facts
                    .get(&check.id)
                    .ok_or_else(|| error("rule-result-missing"))?;
                if !schema(&check.schema)?.is_valid(fact) {
                    finding_count += 1;
                    if finding_count > crate::diagnostics::types::limits::DIAGNOSTICS_PER_RESULT {
                        return Err(error("finding-limit"));
                    }
                    findings.push(json!({"package":package.manifest.id,"rule":check.id,"code":"runtime.rule-failed"}));
                }
            }
        }
        verify_runtime(&pin.runtime)?;
        if digest(&serde_json::to_vec(&sources(root, component)?).unwrap()) != source_digest {
            return Err(error("source-changed"));
        }
        reports.push(json!({"component":id,"runtime":current.id,"status":if findings.is_empty(){"valid"}else{"invalid"},"findings":findings,"source_digest":source_digest}));
    }
    Ok(
        json!({"schema":"lekalo/runtime-check/v1","status":if reports.iter().all(|r|r["status"]=="valid"){"valid"}else{"invalid"},"components":reports}),
    )
}

pub fn init(
    root: &Path,
    runtime: Option<String>,
    rules: Vec<String>,
    path: String,
) -> Result<Value> {
    use std::io::Write;
    let mut components = BTreeMap::new();
    if let Some(runtime) = runtime {
        let source = if let Some(command) = runtime.strip_prefix("exec:") {
            config::Source::Executable(config::Executable {
                command: vec![command.to_owned()],
            })
        } else {
            config::Source::Package(runtime)
        };
        components.insert(
            "app".into(),
            config::Component {
                path,
                runtime: source,
                rules,
            },
        );
    } else if !rules.is_empty() {
        return Err(error("runtime-required"));
    }
    let config = config::Config {
        schema: config::SCHEMA.into(),
        components,
        services: BTreeMap::new(),
    };
    let bytes = serde_json::to_vec_pretty(&config).unwrap();
    config::decode(&bytes)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(config::CONFIG))
        .map_err(|_| error("config-exists"))?;
    file.write_all(&bytes)
        .map_err(|_| error("config-write-failed"))?;
    Ok(json!({"schema":config::SCHEMA,"status":"valid","path":config::CONFIG}))
}

/// A data-only provider pinned once for one host operation. Package resolution
/// and executable admission happen before the first exchange; subsequent calls
/// verify the same admitted executable and package bytes. A caller starts a new
/// session for a new analysis operation.
pub struct Session {
    root: PathBuf,
    command: Vec<String>,
    files: BTreeMap<String, String>,
    package: Option<package::Package>,
}
impl Session {
    /// Resolve a package or installed executable without a language registry.
    pub fn new(root: &Path, source: &config::Source) -> Result<Self> {
        let package = match source {
            config::Source::Package(source) => {
                Some(package::resolve(root, source, "runtime", false)?)
            }
            config::Source::Executable(_) => None,
        };
        let argv = match source {
            config::Source::Package(_) => &package.as_ref().unwrap().manifest.command,
            config::Source::Executable(command) => &command.command,
        };
        let (command, files) = pin_command(root, argv, package.as_ref())?;
        Ok(Self {
            root: root.to_path_buf(),
            command,
            files,
            package,
        })
    }
    /// Perform a bounded exchange in an empty strict sandbox.
    pub fn invoke(&self, operation: &str, input: Value) -> Result<Value> {
        if !config::token(operation) {
            return Err(error("operation-invalid"));
        }
        verify_executables(&self.files)?;
        let result = exchange(
            &self.root,
            &self.command,
            self.package.as_ref(),
            operation,
            input,
        )?;
        // Do not accept a response across a concurrent provider replacement.
        verify_executables(&self.files)?;
        Ok(result)
    }
}
/// Invoke a single operation with a fresh provider session.
pub fn invoke(
    root: &Path,
    source: &config::Source,
    operation: &str,
    input: Value,
) -> Result<Value> {
    Session::new(root, source)?.invoke(operation, input)
}

#[cfg(test)]
mod review_regressions {
    use super::*;

    #[test]
    fn provider_session_refuses_replaced_program_or_entry_before_execution() {
        for changed in ["provider", "entry"] {
            let root = tempfile::tempdir().unwrap();
            for name in ["provider", "entry"] {
                std::fs::write(root.path().join(name), b"admitted bytes").unwrap();
            }
            let source = config::Source::Executable(config::Executable {
                command: vec![
                    root.path().join("provider").to_string_lossy().into_owned(),
                    "entry".into(),
                ],
            });
            let session = Session::new(root.path(), &source).unwrap();
            verify_executables(&session.files).unwrap();
            std::fs::write(root.path().join(changed), b"replacement bytes").unwrap();
            assert_eq!(
                session.invoke("describe", json!({})).unwrap_err().detail,
                "runtime-changed"
            );
        }
    }

    #[test]
    fn schema_keywords_in_property_names_and_literals_are_data() {
        let validator = schema(&json!({"type":"object", "properties": {
            "$ref": {"type":"string"}, "$id": {"const":{"$ref":"https://example.invalid"}},
            "$schema": {"enum":[{"$id":"literal"}]}
        }}))
        .unwrap();
        assert!(validator.is_valid(&json!({"$ref":"value", "$id":{"$ref":"https://example.invalid"}, "$schema":{"$id":"literal"}})));
        assert!(!validator.is_valid(&json!({"$ref": 3})));
        assert!(
            schema(&json!({"$defs":{"str":{"type":"string"}},"$ref":"#/$defs/str"}))
                .unwrap()
                .is_valid(&json!("value"))
        );
        for value in [
            json!({"properties":{"x":{"$ref":"file:///etc/passwd"}}}),
            json!({"allOf":[{"$id":"https://example.invalid"}]}),
            json!({"const":{"$ref":"https://example.invalid"},"$ref":"#/const"}),
        ] {
            assert!(schema(&value).is_err());
        }
    }

    #[test]
    fn unsigned_json_numbers_survive_strict_frontend() {
        for value in [
            i64::MIN.to_string(),
            i64::MAX.to_string(),
            "9223372036854775808".into(),
            u64::MAX.to_string(),
        ] {
            let document = format!("{{\"value\":{value}}}");
            assert_eq!(
                config::value(document.as_bytes()).unwrap(),
                serde_json::from_str::<Value>(&document).unwrap()
            );
        }
        assert!(config::value(b"{\"value\":18446744073709551616}").is_err());
    }

    #[test]
    fn lock_budget_roundtrips_large_locks_and_preserves_previous_on_overflow() {
        let root = tempfile::tempdir().unwrap();
        let mut lock = Lock {
            schema: "lekalo/runtime-lock/v1".into(),
            config_digest: "x".repeat(2 * 1024 * 1024),
            components: BTreeMap::new(),
            services: BTreeMap::new(),
        };
        write_lock(root.path(), &lock).unwrap();
        let bytes = read(root.path(), config::LOCK, LOCK_LIMIT).unwrap();
        let decoded: Lock = serde_json::from_value(config::value(&bytes).unwrap()).unwrap();
        assert_eq!(decoded.config_digest, lock.config_digest);
        lock.config_digest = "x".repeat(LOCK_LIMIT);
        assert_eq!(
            write_lock(root.path(), &lock).unwrap_err().detail,
            "lock-limit"
        );
        assert_eq!(
            std::fs::read(root.path().join(config::LOCK)).unwrap(),
            bytes
        );
    }
}
