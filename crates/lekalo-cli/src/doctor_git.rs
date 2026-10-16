//! The read-only Git facts adapter at the CLI edge (issue #92).
//!
//! This module is the only Git-aware piece of `lekalo doctor`, `status`,
//! and `readiness`. It invokes Git through an argv API (no shell, no
//! interpolation, no network, no checkout/reset/write), under a bounded
//! wait, and maps the result onto a typed [`GitFacts`] handoff: the exact
//! commit identity, the dirty flag as a boolean, and closed state tokens.
//! Raw diff text, repository identity, cwd, stderr text, branch names,
//! and file names never survive into a handoff.

use std::path::Path;
use std::process::{Command, Stdio};

use lekalo_core::doctor::{GitFacts, GitState};

/// Collect the read-only Git facts of one project root.
pub fn git_facts(project_root: &Path) -> GitFacts {
    let commit = match bounded_git(project_root, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
        Ok(commit) if lekalo_core::impact::input::is_revision_grammar(&commit) => commit,
        Ok(_) => {
            // Git answered but the identity violated the closed revision
            // grammar: treat the tool as unusable rather than emitting an
            // unvalidated value.
            return GitFacts {
                state: GitState::Unavailable,
                commit: None,
                dirty: None,
            };
        }
        Err(Failure::NotARepository) => {
            return GitFacts {
                state: GitState::NotARepository,
                commit: None,
                dirty: None,
            }
        }
        Err(Failure::Unavailable) => {
            return GitFacts {
                state: GitState::Unavailable,
                commit: None,
                dirty: None,
            }
        }
    };
    let dirty = bounded_git(project_root, &["status", "--porcelain"])
        .map(|output| !output.trim().is_empty())
        .ok();
    GitFacts {
        state: GitState::Available,
        commit: Some(commit),
        dirty,
    }
}

enum Failure {
    /// Git is unavailable or the invocation failed.
    Unavailable,
    /// Git runs but the root has no resolvable `HEAD` (not a repository,
    /// or an empty repository: there is no revision to report either way).
    NotARepository,
}

/// Run one read-only Git command with a bounded wait; stdout must be
/// valid UTF-8. On timeout the child is killed and Git counts as
/// unavailable.
fn bounded_git(project_root: &Path, args: &[&str]) -> Result<String, Failure> {
    // MCP abandonment is checked at the ACTUAL spawn boundary, not just
    // before model traversal; a cancelled status worker cannot spawn Git
    // seconds after its reply was suppressed (issue #33 r4 F10).
    let mut command = Command::new("git");
    command
        .current_dir(project_root)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let output = crate::mcp::work::git_output(&mut command).map_err(|_| Failure::Unavailable)?;
    if !output.status.success() {
        return Err(Failure::NotARepository);
    }
    let text = String::from_utf8(output.stdout).map_err(|_| Failure::Unavailable)?;
    Ok(text.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_facts_of_a_non_repository_stay_typed() {
        let dir = std::env::temp_dir().join(format!("lekalo-git-facts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let facts = git_facts(&dir);
        assert!(matches!(
            facts.state,
            GitState::NotARepository | GitState::Unavailable
        ));
        assert!(facts.commit.is_none());
        assert!(facts.dirty.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
