//! The read-only Git provenance adapter at the CLI edge for CI reports
//! (issue #103), reusing the accepted doctor seam's bounded invocation
//! discipline: argv only, no shell, no network, no writes, bounded
//! wait, stderr discarded. Raw diff text, repository identity, cwd,
//! branch names, and file names never survive into a snapshot.

use lekalo_core::ci_report::model::{Provenance, UnknownReason, ValueState};
use lekalo_core::ci_report::provenance::GitSnapshot;
use lekalo_core::loader::LoadSelection;

use std::time::Duration;

use crate::doctor_git;

/// The bounded wait for one `git ls-files` invocation; a hung Git is
/// killed at the deadline (review F8/F13) instead of stalling the
/// reported command.
const LS_FILES_TIMEOUT: Duration = Duration::from_secs(10);

/// The poll interval of the bounded wait.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// The aggregate byte cap of the working-set content reads.
const WORKING_SET_BYTE_CAP: u64 = 8 << 20;

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
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::Instant;
    let start = std::time::Instant::now();
    let mut child = Command::new("git")
        .current_dir(root)
        .args(["ls-files", "-z"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Bounded wait: kill and reap at the deadline (review F13); the
    // 1 MiB name cap applies during collection.
    let deadline = start + LS_FILES_TIMEOUT;
    let mut stdout = Vec::new();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                // Drain incrementally so a chatty child cannot fill
                // the pipe and deadlock; the cap still applies.
                if let Some(pipe) = child.stdout.as_mut() {
                    let mut chunk = [0u8; 8192];
                    match pipe.read(&mut chunk) {
                        Ok(0) => {}
                        Ok(read) => {
                            stdout.extend_from_slice(&chunk[..read]);
                            if stdout.len() > (1 << 20) {
                                let _ = child.kill();
                                let _ = child.wait();
                                return None;
                            }
                        }
                        Err(_) => {
                            let _ = child.kill();
                            let _ = child.wait();
                            return None;
                        }
                    }
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let status = status?;
    if !status.success() || start.elapsed() > LS_FILES_TIMEOUT {
        return None;
    }
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut budget = WORKING_SET_BYTE_CAP;
    for name in stdout.split(|byte| *byte == 0) {
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
        let path = root.join(text);
        // Bounded content read: a metadata cap first, then a capped
        // read with the shared deadline checked between files.
        let Ok(metadata) = std::fs::metadata(&path) else {
            return None;
        };
        if !metadata.is_file() || metadata.len() > budget {
            return None;
        }
        budget -= metadata.len();
        if Instant::now() >= deadline {
            return None;
        }
        let bytes = std::fs::read(&path).ok()?;
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
