//! The read-only Git provenance adapter at the CLI edge for CI reports
//! (issue #103), reusing the accepted doctor seam's bounded invocation
//! discipline: argv only, no shell, no network, no writes, bounded
//! wait, stderr discarded. Raw diff text, repository identity, cwd,
//! branch names, and file names never survive into a snapshot.

use lekalo_core::ci_report::model::{Provenance, UnknownReason, ValueState};
use lekalo_core::ci_report::provenance::GitSnapshot;
use lekalo_core::loader::LoadSelection;

use crate::doctor_git;

/// The Git snapshot of one selection through the accepted adapter.
pub fn git_snapshot(selection: &LoadSelection) -> GitSnapshot {
    let root = match lekalo_core::orchestration::project_root(selection) {
        Ok(root) => root,
        Err(_) => return GitSnapshot::unknown(UnknownReason::NotARepository),
    };
    let facts = doctor_git::git_facts(&root);
    match facts.state {
        lekalo_core::doctor::GitState::Available => {
            let working_set = bounded_working_set(&root);
            GitSnapshot {
                commit: facts.commit,
                dirty: facts.dirty,
                working_set_digest: working_set,
                unavailable: None,
            }
        }
        lekalo_core::doctor::GitState::NotARepository => {
            GitSnapshot::unknown(UnknownReason::NotARepository)
        }
        lekalo_core::doctor::GitState::Unavailable => {
            GitSnapshot::unknown(UnknownReason::GitUnavailable)
        }
    }
}

/// The working-set digest over the sorted relative path/content-digest
/// inventory of the Git-tracked files. Untracked files are excluded:
/// the digest binds the reviewed input set, not scratch. Unreadable or
/// hostile trees leave the pin unknown instead of guessing. Bounded:
/// `git ls-files` output caps at 1 MiB and the deadline applies.
fn bounded_working_set(root: &std::path::Path) -> Option<String> {
    use std::process::{Command, Stdio};
    use std::time::Duration;
    let start = std::time::Instant::now();
    let output = Command::new("git")
        .current_dir(root)
        .args(["ls-files", "-z"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() || start.elapsed() > Duration::from_secs(10) {
        return None;
    }
    if output.stdout.len() > (1 << 20) {
        return None;
    }
    let mut entries: Vec<(String, String)> = Vec::new();
    for name in output.stdout.split(|byte| *byte == 0) {
        if name.is_empty() {
            continue;
        }
        let Ok(text) = std::str::from_utf8(name) else {
            return None;
        };
        // Traversal or absolute spellings leave the digest unknown; the
        // inventory binds portable relative paths only.
        if text.starts_with('/')
            || text.contains('\\')
            || text.split('/').any(|segment| segment == "..")
        {
            return None;
        }
        let bytes = std::fs::read(root.join(text)).ok()?;
        entries.push((text.to_owned(), lekalo_core::digest::sha256_hex(&bytes)));
    }
    entries.sort();
    let mut canonical = String::new();
    for (path, digest) in &entries {
        canonical.push_str(path);
        canonical.push('\0');
        canonical.push_str(digest);
        canonical.push('\n');
    }
    Some(format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(canonical.as_bytes())
    ))
}

/// An empty provenance block (unit-test helper): every leaf unknown.
#[cfg(test)]
/// An empty provenance block (unit-test helper): every leaf unknown.
#[cfg(test)]
pub(crate) fn empty_provenance() -> lekalo_core::ci_report::model::Provenance {
    let unknown = || ValueState::unknown(UnknownReason::NotApplicable);
    Provenance {
        git: lekalo_core::ci_report::model::GitProvenance {
            commit: unknown(),
            dirty: unknown(),
            working_set_digest: unknown(),
        },
        model: lekalo_core::ci_report::model::InputProvenance {
            version: unknown(),
            digest: unknown(),
        },
        ir: lekalo_core::ci_report::model::InputProvenance {
            version: unknown(),
            digest: unknown(),
        },
        lock: lekalo_core::ci_report::model::InputProvenance {
            version: unknown(),
            digest: unknown(),
        },
        profiles: Vec::new(),
        adapters: Vec::new(),
    }
}

/// The all-unknown provenance block bound to one Git snapshot: the
/// loader refused before any pin was readable, so every input pin stays
/// typed-unknown instead of fabricated.
pub(crate) fn empty_provenance_for(git: &GitSnapshot) -> Provenance {
    let git_block = lekalo_core::ci_report::provenance::git_block(git);
    Provenance {
        git: git_block,
        model: lekalo_core::ci_report::model::InputProvenance {
            version: ValueState::unknown(UnknownReason::Invalid),
            digest: ValueState::unknown(UnknownReason::Invalid),
        },
        ir: lekalo_core::ci_report::model::InputProvenance {
            version: ValueState::unknown(UnknownReason::Invalid),
            digest: ValueState::unknown(UnknownReason::Invalid),
        },
        lock: lekalo_core::ci_report::model::InputProvenance {
            version: ValueState::unknown(UnknownReason::Absent),
            digest: ValueState::unknown(UnknownReason::Absent),
        },
        profiles: Vec::new(),
        adapters: Vec::new(),
    }
}

/// The provenance block of a readiness run: the Git facts and the lock
/// pin; the model/IR pins stay unknown (the doctor surface is
/// deliberately read-only over the possibly-invalid project).
pub(crate) fn provenance_for_readiness(selection: &LoadSelection, git: &GitSnapshot) -> Provenance {
    let git_block = lekalo_core::ci_report::provenance::git_block(git);
    let lock = match lekalo_core::orchestration::project_root(selection) {
        Ok(root) => lekalo_core::ci_report::provenance::lock_block(&root),
        Err(_) => lekalo_core::ci_report::model::InputProvenance {
            version: ValueState::unknown(UnknownReason::NotARepository),
            digest: ValueState::unknown(UnknownReason::NotARepository),
        },
    };
    Provenance {
        git: git_block,
        model: lekalo_core::ci_report::model::InputProvenance {
            version: ValueState::unknown(UnknownReason::NotApplicable),
            digest: ValueState::unknown(UnknownReason::NotApplicable),
        },
        ir: lekalo_core::ci_report::model::InputProvenance {
            version: ValueState::unknown(UnknownReason::NotApplicable),
            digest: ValueState::unknown(UnknownReason::NotApplicable),
        },
        lock,
        profiles: Vec::new(),
        adapters: Vec::new(),
    }
}
