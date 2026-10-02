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
/// lock is missing, exact version/digest when present. The digest is
/// the canonical lock payload digest (`Lockfile::digest`, the same
/// binding verify's receipt and the manifest check pin) — never the
/// resolver request digest.
pub fn lock_block(root: &Path) -> InputProvenance {
    match LockService::read_state_at(root) {
        Ok(LockState::Present(lock)) => InputProvenance {
            version: ValueState::known_version(lock.core_version().clone()),
            digest: ValueState::known_digest(lock.digest().as_str()),
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

/// The full provenance block of one selection: every pin is read from
/// its actual owner independently of unrelated preparation failures
/// (review F3/F10). A valid model is bound even without a lock —
/// `validate` is deliberately lock-free; an unreadable model is
/// `unknown/invalid`, never a fabricated digest.
pub fn provenance_block(
    selection: &LoadSelection,
    git: &GitSnapshot,
    strict: bool,
) -> Result<Provenance, crate::artifacts::ArtifactFailure> {
    let root = crate::orchestration::project_root(selection).map_err(|_| {
        crate::artifacts::ArtifactFailure::Structure {
            code: "structure.root-not-found",
            denied: false,
        }
    })?;
    // Model/IR pins: computed directly through the accepted loader/IR
    // owners, never gated on the lock or the artifact manifest.
    let (model, ir) = input_pins(selection);
    Ok(Provenance {
        git: git_block(git),
        model,
        ir,
        lock: lock_block(&root),
        profiles: profiles_block(strict),
        adapters: adapters_block(&root),
    })
}

/// The exact model/IR pins of one selection through the accepted
/// loader/IR owners: `unknown/invalid` when the loader or compiler
/// refused (never a guessed digest), exact canonical payload pins
/// otherwise.
fn input_pins(selection: &LoadSelection) -> (InputProvenance, InputProvenance) {
    let unknown = InputProvenance {
        version: ValueState::unknown(UnknownReason::Invalid),
        digest: ValueState::unknown(UnknownReason::Invalid),
    };
    let model = match crate::loader::normalize_model(selection) {
        Ok(model) => model,
        Err(_) => return (unknown.clone(), unknown),
    };
    let model_version = model.model_version.as_str().to_owned();
    let model_json = crate::loader::canonical_model_bytes(&model);
    let compilation = match crate::ir::compile(&model) {
        Ok(compilation) => compilation,
        Err(_) => {
            let model = InputProvenance {
                version: ValueState::known_version(model_version),
                digest: ValueState::known_digest(format!(
                    "sha256:{}",
                    crate::digest::sha256_hex(model_json.as_bytes())
                )),
            };
            return (model, unknown);
        }
    };
    let ir_json = compilation.project.to_canonical_json();
    (
        InputProvenance {
            version: ValueState::known_version(model_version),
            digest: ValueState::known_digest(format!(
                "sha256:{}",
                crate::digest::sha256_hex(model_json.as_bytes())
            )),
        },
        InputProvenance {
            version: ValueState::known_version(crate::ir::version::VERSION),
            digest: ValueState::known_digest(format!(
                "sha256:{}",
                crate::digest::sha256_hex(ir_json.as_bytes())
            )),
        },
    )
}
