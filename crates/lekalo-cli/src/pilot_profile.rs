//! Independently reviewed profiles for the opt-in native pilot batteries.
//! The operator can trust an alternative profile by its exact byte digest.
use lekalo_core::runtime::{config, read};
use serde::{de::DeserializeOwned, Deserialize};
use std::{collections::BTreeSet, path::PathBuf};

fn reviewed<T: DeserializeOwned>(file: &str, variable: &str, pin: &str) -> Option<T> {
    let directory = match std::env::var_os("LEKALO_PILOT_PROFILES") {
        Some(path) => PathBuf::from(path),
        None => std::env::current_dir().ok()?.join("adapters/pilots"),
    };
    let bytes = read(&directory, file, 64 * 1024).ok()?;
    let expected = match std::env::var(variable) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => pin.to_owned(),
        Err(_) => return None,
    };
    if lekalo_core::digest::sha256_hex(&bytes) != expected {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

fn basename(s: &str) -> bool {
    config::token(s) && config::relative(s) && s != "." && !s.contains('/')
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Backend {
    pub target: String,
    pub lane: String,
    pub identity: String,
    pub entry: String,
    pub sha256: String,
    pub interpreter: String,
    pub interpreter_env: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Backends {
    schema: String,
    backends: Vec<Backend>,
}

pub(super) fn worklog() -> Vec<Backend> {
    let Some(profile) = reviewed::<Backends>(
        "worklog.json",
        "LEKALO_WORKLOG_PROFILE_SHA256",
        "742d9d7fa663ecde374208262711de9349f044496152c230dc01529717115277",
    ) else {
        return Vec::new();
    };
    let mut targets = BTreeSet::new();
    let mut lanes = BTreeSet::new();
    if profile.schema != "lekalo/worklog-backends/v1"
        || profile.backends.len() > 32
        || !profile.backends.iter().all(|b| {
            targets.insert(&b.target)
                && lanes.insert(&b.lane)
                && basename(&b.lane)
                && basename(&b.entry)
                && basename(&b.interpreter)
                && config::token(&b.target)
                && config::token(&b.identity)
                && !b.interpreter_env.is_empty()
                && b.interpreter_env
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c == b'_')
        })
    {
        return Vec::new();
    }
    profile.backends
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Artifact {
    pub path: String,
    pub sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Worker {
    schema: String,
    pub target: String,
    pub implementation: String,
    pub runtime: String,
    pub engine: String,
    pub interpreter: String,
    pub fixture: String,
    pub bootstrap: String,
    pub framework_source: String,
    pub lock: Artifact,
    pub worker: Artifact,
    pub store: Artifact,
}

pub(super) fn task_links() -> Option<Worker> {
    let profile = reviewed::<Worker>(
        "task-links.json",
        "LEKALO_TASK_LINKS_PROFILE_SHA256",
        "a3b19b2a603793a89a2d3bb9968c4b0efb993c3729b3949fe46396a19bd742bc",
    )?;
    (profile.schema == "lekalo/task-links-worker/v1"
        && basename(&profile.interpreter)
        && config::token(&profile.target)
        && [
            &profile.fixture,
            &profile.bootstrap,
            &profile.framework_source,
            &profile.lock.path,
            &profile.worker.path,
            &profile.store.path,
        ]
        .iter()
        .all(|p| config::relative(p) && !p.split('/').any(|c| c == ".")))
    .then_some(profile)
}
