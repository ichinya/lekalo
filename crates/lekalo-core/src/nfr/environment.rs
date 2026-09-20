//! The exact-equality environment identity of NFR evidence (issue
//! #85).
//!
//! One environment is the exact measured-conditions record: a stable
//! env id, the owner-held target-profile and adapter snapshots with
//! their digests, the runtime and platform tokens, and sorted labels.
//! Two environments are compatible only when every compared member is
//! equal — the same runtime on a different region is a different
//! environment, and its evidence is reported under its own key, never
//! merged and never satisfying a constraint bound.

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::Sha256Digest;
use crate::scenario::id::NamespacedId;

use super::canonical::sha256_hex;
use super::diagnostic;

/// One owner-held reference: an opaque namespaced id plus the exact
/// digest of the owner-held snapshot. The bytes never enter; the
/// digest is custody evidence.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct OwnerRef {
    id: NamespacedId,
    digest: Sha256Digest,
}

impl OwnerRef {
    /// Assemble from validated parts (wire internal).
    pub(crate) fn new(id: NamespacedId, digest: Sha256Digest) -> Self {
        Self { id, digest }
    }

    /// The namespaced reference id.
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    /// The exact snapshot digest.
    pub fn digest(&self) -> &str {
        self.digest.as_str()
    }
}

/// One bounded token: runtime, platform, or label spellings.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Token(String);

impl Token {
    /// Validate and keep the exact token text.
    pub fn parse(text: &str) -> Result<Self, DiagnosticSet> {
        let ok = !text.is_empty()
            && text.len() <= 64
            && text
                .bytes()
                .next()
                .is_some_and(|first| first.is_ascii_alphanumeric())
            && text.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
            });
        if !ok {
            return Err(diagnostic::document_invalid("token", None));
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The exact measured-conditions identity of one evidence set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Environment {
    env_id: Token,
    profile_ref: OwnerRef,
    adapter_ref: OwnerRef,
    runtime: Token,
    platform: Token,
    labels: Vec<Token>,
}

impl Environment {
    /// Assemble from validated parts (wire internal); labels are
    /// stored canonically sorted and deduplicated.
    pub(crate) fn assemble(
        env_id: Token,
        profile_ref: OwnerRef,
        adapter_ref: OwnerRef,
        runtime: Token,
        platform: Token,
        mut labels: Vec<Token>,
    ) -> Self {
        labels.sort();
        labels.dedup();
        Self {
            env_id,
            profile_ref,
            adapter_ref,
            runtime,
            platform,
            labels,
        }
    }

    /// The stable environment id.
    pub fn env_id(&self) -> &str {
        self.env_id.as_str()
    }

    /// The owner-held target-profile snapshot.
    pub fn profile_ref(&self) -> &OwnerRef {
        &self.profile_ref
    }

    /// The owner-held adapter snapshot.
    pub fn adapter_ref(&self) -> &OwnerRef {
        &self.adapter_ref
    }

    /// The runtime token.
    pub fn runtime(&self) -> &str {
        self.runtime.as_str()
    }

    /// The platform token.
    pub fn platform(&self) -> &str {
        self.platform.as_str()
    }

    /// The sorted labels.
    pub fn labels(&self) -> &[Token] {
        &self.labels
    }

    /// Exact-equality compatibility: both environments are compatible
    /// exactly when the env id, both reference digests, the runtime,
    /// the platform, and the sorted labels are all equal. Reference
    /// ids are provenance, not identity: the same profile id re-snapshotted
    /// is a different environment.
    pub fn compatible_with(&self, other: &Self) -> bool {
        self.env_id == other.env_id
            && self.profile_ref.digest == other.profile_ref.digest
            && self.adapter_ref.digest == other.adapter_ref.digest
            && self.runtime == other.runtime
            && self.platform == other.platform
            && self.labels == other.labels
    }

    /// Why `other` is a foreign environment, as the fixed reason token
    /// of the first mismatched member in canonical member order.
    /// `undeclared-environment` is decided by the caller.
    pub fn foreign_reason(&self, other: &Self) -> &'static str {
        if self.profile_ref.digest != other.profile_ref.digest {
            "profile-mismatch"
        } else if self.adapter_ref.digest != other.adapter_ref.digest {
            "adapter-mismatch"
        } else if self.runtime != other.runtime {
            "runtime-mismatch"
        } else if self.platform != other.platform {
            "platform-mismatch"
        } else if self.labels != other.labels {
            "labels-mismatch"
        } else {
            "undeclared-environment"
        }
    }

    /// The stable display key of this environment: the env id plus a
    /// short digest over the full canonical identity, so two distinct
    /// environments with one env id still get distinct report keys.
    pub fn env_key(&self) -> String {
        let mut canonical = String::new();
        canonical.push_str(self.env_id.as_str());
        canonical.push('\u{1f}');
        canonical.push_str(self.profile_ref.digest.as_str());
        canonical.push('\u{1f}');
        canonical.push_str(self.adapter_ref.digest.as_str());
        canonical.push('\u{1f}');
        canonical.push_str(self.runtime.as_str());
        canonical.push('\u{1f}');
        canonical.push_str(self.platform.as_str());
        for label in &self.labels {
            canonical.push('\u{1f}');
            canonical.push_str(label.as_str());
        }
        format!(
            "{}:{}",
            self.env_id.as_str(),
            &sha256_hex(canonical.as_bytes())[..16]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(env_id: &str, runtime: &str, labels: &[&str]) -> Environment {
        Environment::assemble(
            Token::parse(env_id).unwrap(),
            OwnerRef::new(
                NamespacedId::parse("targets/node-aws").unwrap(),
                Sha256Digest::parse(&format!("sha256:{}", "ab".repeat(32))).unwrap(),
            ),
            OwnerRef::new(
                NamespacedId::parse("adapters.node/runner-x").unwrap(),
                Sha256Digest::parse(&format!("sha256:{}", "cd".repeat(32))).unwrap(),
            ),
            Token::parse(runtime).unwrap(),
            Token::parse("linux-x64").unwrap(),
            labels
                .iter()
                .map(|label| Token::parse(label).unwrap())
                .collect(),
        )
    }

    #[test]
    fn identical_environments_are_compatible() {
        let left = environment("staging-eu", "node-22", &["k8s", "eu-west"]);
        let right = environment("staging-eu", "node-22", &["eu-west", "k8s"]);
        assert!(left.compatible_with(&right));
        assert_eq!(left.env_key(), right.env_key());
    }

    #[test]
    fn every_member_mismatch_is_a_different_environment() {
        let base = environment("staging-eu", "node-22", &["k8s"]);
        // Same runtime, different platform.
        let mut other = environment("staging-eu", "node-22", &["k8s"]);
        other.platform = Token::parse("linux-arm64").unwrap();
        assert!(!base.compatible_with(&other));
        assert_eq!(base.foreign_reason(&other), "platform-mismatch");
        // Different labels.
        let labelled = environment("staging-eu", "node-22", &["eu-west"]);
        assert!(!base.compatible_with(&labelled));
        assert_eq!(base.foreign_reason(&labelled), "labels-mismatch");
        // Different runtime.
        let newer = environment("staging-eu", "node-24", &["k8s"]);
        assert_eq!(base.foreign_reason(&newer), "runtime-mismatch");
        // Different profile digest.
        let mut reprofiled = environment("staging-eu", "node-22", &["k8s"]);
        reprofiled.profile_ref = OwnerRef::new(
            NamespacedId::parse("targets/node-aws").unwrap(),
            Sha256Digest::parse(&format!("sha256:{}", "ff".repeat(32))).unwrap(),
        );
        assert!(!base.compatible_with(&reprofiled));
        assert_eq!(base.foreign_reason(&reprofiled), "profile-mismatch");
        // Different env id.
        let renamed = environment("staging-us", "node-22", &["k8s"]);
        assert!(!base.compatible_with(&renamed));
    }

    #[test]
    fn env_keys_are_stable_and_distinct() {
        let left = environment("staging-eu", "node-22", &["k8s"]);
        let right = environment("staging-us", "node-22", &["k8s"]);
        assert_ne!(left.env_key(), right.env_key());
        assert!(left.env_key().starts_with("staging-eu:"));
    }

    #[test]
    fn labels_sort_canonically() {
        let left = environment("env", "node-22", &["b", "a"]);
        let right = environment("env", "node-22", &["a", "b"]);
        assert!(left.compatible_with(&right));
        assert_eq!(left.labels().len(), 2);
        assert_eq!(left.labels()[0].as_str(), "a");
    }
}
