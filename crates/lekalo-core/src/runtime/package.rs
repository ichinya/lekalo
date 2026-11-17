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
        || !matches!(m.kind.as_str(), "runtime" | "rules")
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

pub fn resolve(project: &Path, source: &str, kind: &str, offline: bool) -> Result<Package> {
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
    let files: BTreeMap<_, _> = content
        .iter()
        .map(|(p, b)| (p.clone(), digest(b)))
        .collect();
    let hash = digest(&serde_json::to_vec(&files).unwrap());
    let p = Package {
        source: source.to_owned(),
        revision,
        manifest: m,
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
        std::fs::rename(staged.path(), &destination)
            .map_err(|_| error("package-store-conflict"))?;
    }
    verify(&p)?;
    Ok(p)
}

pub fn verify(package: &Package) -> Result<()> {
    if digest(&serde_json::to_vec(&package.files).unwrap()) != package.digest {
        return Err(error("package-lock-invalid"));
    }
    let root = root(package)?;
    for (path, hash) in &package.files {
        if digest(&read(&root, path, 32 * 1024 * 1024)?) != *hash {
            return Err(error("package-changed"));
        }
    }
    let expected: std::collections::BTreeSet<_> = package
        .manifest
        .files
        .iter()
        .map(String::as_str)
        .chain(std::iter::once("lekalo.package.json"))
        .collect();
    if expected != package.files.keys().map(String::as_str).collect() {
        return Err(error("package-lock-invalid"));
    }
    let actual = manifest(
        &read(&root, "lekalo.package.json", 128 * 1024)?,
        &package.manifest.kind,
    )?;
    if serde_json::to_value(&actual).unwrap() != serde_json::to_value(&package.manifest).unwrap() {
        return Err(error("package-lock-invalid"));
    }
    Ok(())
}
