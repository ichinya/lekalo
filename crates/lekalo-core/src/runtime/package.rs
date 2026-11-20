//! External package admission and immutable snapshots outside the core checkout.
use super::{config, digest, error, read, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub id: String,
    pub version: String,
    pub kind: String,
    /// Directory mounts for independently pinned library packages.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub libraries: BTreeMap<String, String>,
    pub files: Vec<String>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub rules: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub source: String,
    pub revision: Option<String>,
    pub manifest: Manifest,
    pub digest: String,
    pub files: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, Package>,
}

pub fn home() -> Result<PathBuf> {
    let base = std::env::var_os("LEKALO_PACKAGE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
                .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache")))
                .map(|p| p.join("lekalo/packages"))
        })
        .ok_or_else(|| error("package-home-missing"))?;
    std::fs::create_dir_all(&base).map_err(|_| error("package-home-unavailable"))?;
    std::fs::canonicalize(base).map_err(|_| error("package-home-unavailable"))
}

pub fn root(package: &Package) -> Result<PathBuf> {
    if !crate::target_protocol::wire::is_sha256_digest(&package.digest) {
        return Err(error("package-lock-invalid"));
    }
    Ok(home()?.join(package.digest.trim_start_matches("sha256:")))
}

fn manifest(bytes: &[u8], kind: &str) -> Result<Manifest> {
    let m: Manifest = serde_json::from_value(config::value(bytes)?)
        .map_err(|_| error("package-manifest-invalid"))?;
    if m.schema != "lekalo/package/v1"
        || !config::token(&m.id)
        || semver::Version::parse(&m.version).is_err()
        || m.kind != kind
        || !matches!(
            m.kind.as_str(),
            "runtime" | "rules" | "library" | "evidence"
        )
        || m.libraries.len() > 8
        || m.libraries.iter().any(|(mount, source)| {
            !mount
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                || mount.is_empty()
                || mount.len() > 48
                || m.files
                    .iter()
                    .any(|f| f.split('/').next() == Some(mount.as_str()))
                || coordinate(source).is_err()
                || !source.rsplit_once('#').is_some_and(|(_, rev)| {
                    rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit())
                })
        })
        || m.files.is_empty()
        || m.files.len() > 256
        || m.files.iter().any(|p| {
            !config::relative(p)
                || p == "lekalo.package.json"
                || p.split('/').any(|s| s.starts_with('.'))
        })
        || m.files
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != m.files.len()
    {
        return Err(error("package-manifest-invalid"));
    }
    match kind {
        "runtime" if m.command.is_empty() || m.command.len() > 16 || m.rules.is_some() => {
            return Err(error("runtime-entry-missing"))
        }
        "evidence"
            if !m.command.is_empty()
                || m.rules.is_some()
                || !m.libraries.is_empty()
                || !m.files.iter().any(|path| path == "payload.schema.json")
                || m.files.iter().any(|path| {
                    !matches!(path.as_str(), "payload.schema.json" | "bindings.json")
                }) =>
        {
            return Err(error("package-manifest-invalid"))
        }
        "library" if !m.command.is_empty() || m.rules.is_some() => {
            return Err(error("package-manifest-invalid"))
        }
        "rules"
            if !m.command.is_empty() || !m.rules.as_ref().is_some_and(|p| m.files.contains(p)) =>
        {
            return Err(error("rules-entry-missing"))
        }
        _ => (),
    }
    Ok(m)
}

fn git(directory: &Path, args: Vec<String>) -> Result<Vec<u8>> {
    use crate::target_protocol::transport::{self, AdapterCommand, TransportLimits};
    let mut argv = vec![
        "-c".into(),
        "core.hooksPath=/dev/null".into(),
        "-c".into(),
        "protocol.ext.allow=never".into(),
    ];
    argv.extend(args);
    let r = transport::run(
        &AdapterCommand {
            program: "git".into(),
            args: argv,
        },
        b"",
        &TransportLimits {
            timeout_ms: 120_000,
            max_output_bytes: 32 * 1024 * 1024,
            ..Default::default()
        },
        directory,
        false,
        None,
        &[("GIT_TERMINAL_PROMPT".into(), "0".into())],
    )
    .map_err(|_| error("package-fetch-failed"))?;
    if r.exit_code != 0 {
        return Err(error("package-fetch-failed"));
    }
    Ok(r.stdout)
}

fn coordinate(source: &str) -> Result<(String, String)> {
    let (source, reference) = source.rsplit_once('#').unwrap_or((source, "HEAD"));
    if reference.is_empty()
        || reference.starts_with('-')
        || reference.len() > 256
        || reference.chars().any(char::is_control)
    {
        return Err(error("source-invalid"));
    }
    let url = if let Some(short) = source.strip_prefix("gh/") {
        let parts: Vec<_> = short.split('/').collect();
        if parts.len() != 2
            || parts
                .iter()
                .any(|s| !config::token(s) || s.contains(['@', '/']))
        {
            return Err(error("source-invalid"));
        }
        format!("https://github.com/{short}.git")
    } else if let Some(url) = source.strip_prefix("git+") {
        // Explicit local Git URLs also make offline/private package testing possible.
        if !(url.starts_with("https://") || url.starts_with("file://"))
            || url.contains('@')
            || url.chars().any(char::is_control)
        {
            return Err(error("source-invalid"));
        }
        url.to_owned()
    } else {
        return Err(error("source-invalid"));
    };
    Ok((url, reference.to_owned()))
}

pub(crate) struct EvidencePackage {
    pub id: String,
    pub payload: serde_json::Value,
    pub bindings: Option<Vec<u8>>,
}

/// Read an explicitly selected project-local data package. Use the same
/// inventory digest as the runtime package store without populating the cache.
pub(crate) fn evidence_schema(
    project: &Path,
    source: &str,
    package_digest: &str,
    schema_digest: &str,
) -> Result<EvidencePackage> {
    let dir = source
        .strip_prefix("path:")
        .filter(|path| config::relative(path) && crate::project_fs::path_violation(path).is_none())
        .ok_or_else(|| error("evidence-package-source"))?;
    let manifest_path = "lekalo.package.json";
    let schema_path = "payload.schema.json";
    let manifest_bytes = read(project, &format!("{dir}/{manifest_path}"), 16 * 1024)?;
    let manifest = manifest(&manifest_bytes, "evidence")?;
    let schema_bytes = read(project, &format!("{dir}/{schema_path}"), 64 * 1024)?;
    let mut inventory = BTreeMap::from([
        (manifest_path.to_owned(), digest(&manifest_bytes)),
        (schema_path.to_owned(), digest(&schema_bytes)),
    ]);
    let bindings = if manifest.files.iter().any(|file| file == "bindings.json") {
        let bytes = read(project, &format!("{dir}/bindings.json"), 16 * 1024)?;
        inventory.insert("bindings.json".to_owned(), digest(&bytes));
        Some(bytes)
    } else {
        None
    };
    if inventory_digest(&inventory) != package_digest || inventory[schema_path] != schema_digest {
        return Err(error("evidence-package-pin"));
    }
    Ok(EvidencePackage {
        id: manifest.id,
        payload: config::value(&schema_bytes)?,
        bindings,
    })
}

fn inventory_digest(files: &BTreeMap<String, String>) -> String {
    digest(&serde_json::to_vec(files).expect("string inventory serializes"))
}

pub fn resolve(project: &Path, source: &str, kind: &str, offline: bool) -> Result<Package> {
    resolve_inner(project, source, kind, offline, 0, &mut 16)
}

fn resolve_inner(
    project: &Path,
    source: &str,
    kind: &str,
    offline: bool,
    depth: usize,
    remaining: &mut usize,
) -> Result<Package> {
    if depth > 4 || *remaining == 0 {
        return Err(error("package-dependency-limit"));
    }
    *remaining -= 1;
    let cache = home()?;
    if cache.starts_with(project) {
        return Err(error("package-home-inside-project"));
    }
    let checkout;
    let local;
    let revision;
    let mut git_root = None;
    if let Some(path) = source.strip_prefix("path:") {
        local = std::fs::canonicalize(project.join(path))
            .map_err(|_| error("package-source-missing"))?;
        revision = None;
    } else {
        if offline {
            return Err(error("offline-package-missing"));
        }
        let (url, reference) = coordinate(source)?;
        checkout = tempfile::tempdir_in(&cache).map_err(|_| error("package-stage-failed"))?;
        git(
            checkout.path(),
            vec![
                "clone".into(),
                "--bare".into(),
                "--no-local".into(),
                "--".into(),
                url,
                "repo".into(),
            ],
        )?;
        let repository = checkout.path().join("repo");
        let commit = String::from_utf8(git(
            &repository,
            vec![
                "rev-parse".into(),
                "--verify".into(),
                "--end-of-options".into(),
                format!("{reference}^{{commit}}"),
            ],
        )?)
        .map_err(|_| error("package-revision-invalid"))?
        .trim()
        .to_owned();
        if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(error("package-revision-invalid"));
        }
        revision = Some(commit);
        local = repository.clone();
        git_root = Some(repository);
    }
    let get = |path: &str| -> Result<Vec<u8>> {
        if let Some(repo) = &git_root {
            let commit = revision.as_ref().unwrap();
            let tree = git(
                repo,
                vec!["ls-tree".into(), commit.clone(), "--".into(), path.into()],
            )?;
            if !tree.starts_with(b"100644 blob ") && !tree.starts_with(b"100755 blob ") {
                return Err(error("package-file-type"));
            }
            git(repo, vec!["show".into(), format!("{commit}:{path}")])
        } else {
            read(&local, path, 32 * 1024 * 1024)
        }
    };
    let manifest_bytes = get("lekalo.package.json")?;
    let m = manifest(&manifest_bytes, kind)?;
    let mut content = BTreeMap::from([("lekalo.package.json".to_owned(), manifest_bytes)]);
    let mut total = 0;
    for path in &m.files {
        let bytes = get(path)?;
        total += bytes.len();
        if total > 128 * 1024 * 1024 {
            return Err(error("package-size-limit"));
        }
        content.insert(path.clone(), bytes);
    }
    let mut dependencies = BTreeMap::new();
    for (mount, coordinate) in &m.libraries {
        if coordinate.starts_with("git+file:")
            && !(source.starts_with("git+file:") || source.starts_with("path:"))
        {
            return Err(error("package-dependency-source"));
        }
        let dep = resolve_inner(
            project,
            coordinate,
            "library",
            offline,
            depth + 1,
            remaining,
        )?;
        let base = root(&dep)?;
        for path in dep.files.keys() {
            let bytes = read(&base, path, 32 * 1024 * 1024)?;
            total += bytes.len();
            if total > 128 * 1024 * 1024 || content.len() >= 256 {
                return Err(error("package-size-limit"));
            }
            content.insert(format!("{mount}/{path}"), bytes);
        }
        dependencies.insert(mount.clone(), dep);
    }
    let files: BTreeMap<_, _> = content
        .iter()
        .map(|(p, b)| (p.clone(), digest(b)))
        .collect();
    let hash = inventory_digest(&files);
    let p = Package {
        source: source.to_owned(),
        revision,
        manifest: m,
        dependencies,
        digest: hash,
        files,
    };
    let destination = root(&p)?;
    if !destination.exists() {
        let staged = tempfile::tempdir_in(&cache).map_err(|_| error("package-stage-failed"))?;
        for (path, bytes) in content {
            let path = staged.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap())
                .map_err(|_| error("package-stage-failed"))?;
            std::fs::write(&path, bytes).map_err(|_| error("package-stage-failed"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                // No setuid/setgid bits; a package can execute only its selected entry.
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
                    .map_err(|_| error("package-stage-failed"))?;
            }
        }
        if std::fs::rename(staged.path(), &destination).is_err() && !destination.is_dir() {
            return Err(error("package-store-conflict"));
        }
    }
    verify(&p)?;
    Ok(p)
}

/// Populate a fresh cache from the locked commit, or verify a local path package.
pub fn restore(project: &Path, p: &Package) -> Result<()> {
    if root(p)?.symlink_metadata().is_ok() {
        return verify(p);
    }
    let source = if p.source.starts_with("path:") && p.revision.is_none() {
        p.source.clone()
    } else {
        let rev = p
            .revision
            .as_deref()
            .ok_or_else(|| error("package-revision-invalid"))?;
        if rev.len() != 40 || !rev.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(error("package-revision-invalid"));
        }
        let base = p
            .source
            .rsplit_once('#')
            .map_or(p.source.as_str(), |(base, _)| base);
        format!("{base}#{rev}")
    };
    let loaded = resolve(project, &source, &p.manifest.kind, false)?;
    if loaded.digest != p.digest || loaded.revision != p.revision {
        return Err(error("package-changed"));
    }
    verify(p)
}

pub fn verify(package: &Package) -> Result<()> {
    if inventory_digest(&package.files) != package.digest {
        return Err(error("package-lock-invalid"));
    }
    let root = root(package)?;
    verify_inventory(package, &root, "", 0, &mut 16)?;
    let mut total = 0;
    for (path, hash) in &package.files {
        let bytes = read(&root, path, 32 * 1024 * 1024)?;
        total += bytes.len();
        if total > 128 * 1024 * 1024 || digest(&bytes) != *hash {
            return Err(error("package-changed"));
        }
    }
    Ok(())
}

fn verify_inventory(
    p: &Package,
    root: &Path,
    prefix: &str,
    depth: usize,
    remaining: &mut usize,
) -> Result<()> {
    if depth > 4
        || *remaining == 0
        || p.files.len() > 257
        || !p.dependencies.keys().eq(p.manifest.libraries.keys())
    {
        return Err(error("package-lock-invalid"));
    }
    *remaining -= 1;
    let actual = manifest(
        &read(root, &format!("{prefix}lekalo.package.json"), 128 * 1024)?,
        &p.manifest.kind,
    )?;
    if serde_json::to_value(actual).unwrap() != serde_json::to_value(&p.manifest).unwrap() {
        return Err(error("package-lock-invalid"));
    }
    let mut expected: std::collections::BTreeSet<String> =
        p.manifest.files.iter().cloned().collect();
    expected.insert("lekalo.package.json".into());
    for (mount, dep) in &p.dependencies {
        if p.manifest.libraries[mount] != dep.source
            || dep.manifest.kind != "library"
            || dep.revision.as_deref() != dep.source.rsplit_once('#').map(|(_, r)| r)
            || digest(&serde_json::to_vec(&dep.files).unwrap()) != dep.digest
        {
            return Err(error("package-lock-invalid"));
        }
        verify_inventory(
            dep,
            root,
            &format!("{prefix}{mount}/"),
            depth + 1,
            remaining,
        )?;
        for (file, hash) in &dep.files {
            let path = format!("{mount}/{file}");
            if p.files.get(&path) != Some(hash) || !expected.insert(path) {
                return Err(error("package-lock-invalid"));
            }
        }
    }
    if expected.iter().ne(p.files.keys()) {
        return Err(error("package-lock-invalid"));
    }
    Ok(())
}
