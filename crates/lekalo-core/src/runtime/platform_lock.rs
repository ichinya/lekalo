//! Portable v2 lock rows collected by the OS and architecture that verified them.
use super::{config, error, package, read, Lock, Result, RuntimePin, LOCK_LIMIT};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

const SCHEMA: &str = "lekalo/runtime-lock/v3";
const MAX_PLATFORMS: usize = 16;

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum LockDocument {
    Single(Lock),
    ByPlatform(PlatformLock),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformLock {
    schema: String,
    config_digest: String,
    platforms: BTreeMap<String, Lock>,
}

fn host() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn platform_key(key: &str) -> bool {
    key.split_once('-').is_some_and(|(os, arch)| {
        [os, arch].iter().all(|part| {
            !part.is_empty()
                && part.len() <= 20
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
    })
}

impl LockDocument {
    pub(super) fn for_host(&self) -> Result<&Lock> {
        match self {
            Self::Single(lock) => Ok(lock),
            Self::ByPlatform(lock) => lock
                .platforms
                .get(&host())
                .ok_or_else(|| error("lock-platform-unavailable")),
        }
    }
}

fn read_value(root: &Path) -> Result<Value> {
    config::value(&read(root, config::LOCK, LOCK_LIMIT)?)
}

pub(super) fn read_document(root: &Path) -> Result<LockDocument> {
    let value = read_value(root)?;
    if value["schema"] == SCHEMA {
        return parse_platforms(value).map(LockDocument::ByPlatform);
    }
    serde_json::from_value(value)
        .map(LockDocument::Single)
        .map_err(|_| error("lock-invalid"))
}

fn parse_platforms(value: Value) -> Result<PlatformLock> {
    let document: PlatformLock =
        serde_json::from_value(value).map_err(|_| error("lock-invalid"))?;
    let first = document
        .platforms
        .values()
        .next()
        .ok_or_else(|| error("lock-platforms-invalid"))?;
    if document.platforms.len() > MAX_PLATFORMS
        || !crate::target_protocol::wire::is_sha256_digest(&document.config_digest)
        || document.platforms.iter().any(|(platform, row)| {
            !platform_key(platform)
                || row.schema != "lekalo/runtime-lock/v2"
                || row.config_digest != document.config_digest
                || !same_composition(first, row)
        })
    {
        return Err(error("lock-platforms-invalid"));
    }
    Ok(document)
}

fn same_package(left: &package::Package, right: &package::Package) -> bool {
    left.source == right.source && left.revision == right.revision && left.digest == right.digest
}

fn same_runtime(left: &RuntimePin, right: &RuntimePin) -> bool {
    left.command == right.command
        && left.description.id == right.description.id
        && left.description.version == right.description.version
        && match (&left.package, &right.package) {
            (None, None) => true,
            (Some(left), Some(right)) => same_package(left, right),
            _ => false,
        }
}

fn same_composition(left: &Lock, right: &Lock) -> bool {
    left.components.keys().eq(right.components.keys())
        && left.services.keys().eq(right.services.keys())
        && left
            .components
            .values()
            .zip(right.components.values())
            .all(|(left, right)| {
                same_runtime(&left.runtime, &right.runtime)
                    && left.rules.len() == right.rules.len()
                    && left
                        .rules
                        .iter()
                        .zip(&right.rules)
                        .all(|(left, right)| same_package(left, right))
            })
        && left
            .services
            .values()
            .zip(right.services.values())
            .all(|(left, right)| same_runtime(left, right))
}

/// Replace the current platform row under the project writer lock. A v1/v2
/// row has no host provenance, so migration rebuilds it on the current host.
pub(super) fn replace(root: &Path, selected: Lock, per_platform: bool) -> Result<LockDocument> {
    let previous = match std::fs::symlink_metadata(root.join(config::LOCK)) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            match read_value(root) {
                Ok(value) if value["schema"] == SCHEMA => Some(parse_platforms(value)?),
                Err(error) if per_platform => return Err(error),
                _ => None,
            }
        }
        Ok(_) => return Err(error("lock-path-invalid")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(error("lock-path-invalid")),
    };
    let current = host();
    let mut document = match previous {
        Some(document) => document,
        None if !per_platform => return Ok(LockDocument::Single(selected)),
        None => PlatformLock {
            schema: SCHEMA.into(),
            config_digest: selected.config_digest.clone(),
            platforms: BTreeMap::new(),
        },
    };
    if !platform_key(&current)
        || (document.platforms.len() >= MAX_PLATFORMS && !document.platforms.contains_key(&current))
    {
        return Err(error("lock-platforms-invalid"));
    }
    if document.config_digest != selected.config_digest {
        return Err(error("lock-platform-config-changed"));
    }
    if document
        .platforms
        .iter()
        .any(|(key, row)| key != &current && !same_composition(row, &selected))
    {
        return Err(error("lock-platform-composition-changed"));
    }
    document.platforms.insert(current, selected);
    Ok(LockDocument::ByPlatform(document))
}
