//! The provenance snapshot builder (issue #103).
//!
//! Collects the exact git/model/IR/lock/profile/adapter pins through
//! their accepted owners. Every leaf is a value state: a missing lock is
//! `unknown/absent`, never the hash of an empty file; a non-Git tree is
//! `unknown/not-a-repository`; nothing leaks absolute paths.

use std::path::Path;

use crate::loader::LoadSelection;
use crate::lockfile::plan::LockService;
use crate::lockfile::LockState;

use super::model::{
    AdapterProvenance, GitProvenance, InputProvenance, ProfileProvenance, Provenance,
    UnknownReason, ValueState,
};

/// The Git facts one report binds, produced at the CLI edge by the
/// read-only Git adapter (the doctor handoff).
#[derive(Clone, Debug, Default)]
pub struct GitSnapshot {
    /// The resolved HEAD revision, when Git is a repository here.
    pub commit: Option<String>,
    /// Whether the working tree is dirty, when Git answered.
    pub dirty: Option<bool>,
    /// The working-set digest over the sorted relative path/content
    /// inventory of tracked inputs, when computable.
    pub working_set_digest: Option<String>,
    /// Why Git facts are absent, when they are.
    pub unavailable: Option<UnknownReason>,
}

impl GitSnapshot {
    /// The all-unknown snapshot (non-repository or Git unavailable).
    pub fn unknown(reason: UnknownReason) -> Self {
        Self {
            commit: None,
            dirty: None,
            working_set_digest: None,
            unavailable: Some(reason),
        }
    }
}

/// The validated model/IR pins of one selection (the accepted inputs
/// receipt seam), or the loader refusal spelled as unknown states.
pub struct InputSnapshot {
    /// The canonical model version/digest.
    pub model: InputProvenance,
    /// The typed IR version/digest.
    pub ir: InputProvenance,
}

/// Build the Git block from one snapshot.
pub fn git_block(snapshot: &GitSnapshot) -> GitProvenance {
    let unknown_reason = snapshot
        .unavailable
        .unwrap_or(UnknownReason::GitUnavailable);
    GitProvenance {
        commit: match &snapshot.commit {
            Some(commit) => ValueState::known_revision(commit.clone()),
            None => ValueState::unknown(unknown_reason),
        },
        dirty: match snapshot.dirty {
            Some(dirty) => ValueState::known_flag(dirty),
            None => ValueState::unknown(unknown_reason),
        },
        working_set_digest: match &snapshot.working_set_digest {
            Some(digest) => ValueState::known_digest(digest),
            None => ValueState::unknown(unknown_reason),
        },
    }
}

/// Read the committed lock pin of one root: `unknown/absent` when the
/// lock is missing, exact version/digest when present.
pub fn lock_block(root: &Path) -> InputProvenance {
    match LockService::read_state_at(root) {
        Ok(LockState::Present(lock)) => InputProvenance {
            version: ValueState::known_version(lock.core_version().clone()),
            digest: ValueState::known_digest(lock.request_digest().as_str()),
        },
        Ok(LockState::Absent) => InputProvenance {
            version: ValueState::unknown(UnknownReason::Absent),
            digest: ValueState::unknown(UnknownReason::Absent),
        },
        Err(_) => InputProvenance {
            version: ValueState::unknown(UnknownReason::Invalid),
            digest: ValueState::unknown(UnknownReason::Invalid),
        },
    }
}

/// The profiles block: the effective validation profile pin. The two
/// built-ins are embedded contracts with exact versions; the CI surface
/// records the one the command would select.
pub fn profiles_block(strict: bool) -> Vec<ProfileProvenance> {
    let profile = if strict { "strict" } else { "default" };
    vec![ProfileProvenance {
        id: format!("validation-profile.{profile}"),
        version: ValueState::known_version(crate::validator::PROFILE_VERSION),
        digest: ValueState::known_digest(format!(
            "sha256:{}",
            if strict {
                crate::digest::sha256_hex(crate::validator::STRICT_PROFILE_BYTES)
            } else {
                crate::digest::sha256_hex(crate::validator::DEFAULT_PROFILE_BYTES)
            }
        )),
    }]
}

/// The adapters block from the committed lock, when one exists.
pub fn adapters_block(root: &Path) -> Vec<AdapterProvenance> {
    match LockService::read_state_at(root) {
        Ok(LockState::Present(lock)) => lock
            .adapters()
            .iter()
            .map(|adapter| AdapterProvenance {
                id: adapter.id().as_str().to_owned(),
                version: ValueState::known_version(adapter.version().clone()),
                digest: ValueState::known_digest(adapter.digest().as_str()),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The full provenance block of one selection: the loader's accepted
/// pins or explicit unknown states, never a guessed value.
pub fn provenance_block(
    selection: &LoadSelection,
    git: &GitSnapshot,
    strict: bool,
) -> Result<Provenance, crate::artifacts::ArtifactFailure> {
    let inputs = crate::artifacts::GenerateService::inputs(selection)?;
    let root = crate::orchestration::project_root(selection).map_err(|_| {
        crate::artifacts::ArtifactFailure::Structure {
            code: "structure.root-not-found",
            denied: false,
        }
    })?;
    Ok(Provenance {
        git: git_block(git),
        model: InputProvenance {
            version: ValueState::known_version(inputs.model_version),
            digest: ValueState::known_digest(inputs.model_digest),
        },
        ir: InputProvenance {
            version: ValueState::known_version(inputs.ir_version),
            digest: ValueState::known_digest(inputs.ir_digest),
        },
        lock: lock_block(&root),
        profiles: profiles_block(strict),
        adapters: adapters_block(&root),
    })
}
