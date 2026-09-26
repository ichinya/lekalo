//! The closed typed values of the adapter package surface (issue #32).
//!
//! Every wire string is parsed into a closed typed value before it can
//! exist in a [`ManifestDocument`](crate::adapter_package::ManifestDocument)
//! or an inventory row: the grammars reuse the accepted lock spellings
//! ([`crate::lockfile::types`]) and the scope grammar of the target
//! protocol. Nothing URL-shaped, credential-shaped, or path-escaping is
//! accepted; the refused spellings map onto [`PackageFailure`].

use std::fmt;

use crate::lockfile::types as lock;
use crate::lockfile::LockFailure;

/// Why an adapter package operation refused.
///
/// The status, exit class, and stream are fixed per variant; nothing here
/// echoes raw operating-system messages, absolute host paths, timestamps,
/// or environment data.
#[derive(Clone, Debug, PartialEq)]
pub enum PackageFailure {
    /// The manifest is not a valid closed-wire document (`exit 1`).
    ManifestInvalid {
        /// The bounded normalizer reason token (schema, grammar, bound).
        reason: String,
    },
    /// The describe outcome contradicts the verified manifest
    /// (`exit 3`).
    ManifestMismatch {
        /// The first disagreeing manifest member, in canonical order.
        field: String,
    },
    /// The manifest compatibility set excludes the current contracts
    /// (`exit 5`).
    Incompatible {
        /// The adapter id the refusal names.
        adapter: String,
    },
    /// Declared digests do not match the bytes on disk (`exit 3`).
    ChecksumMismatch {
        /// The digest domain: `manifest`, `package`, `file`, or `entry`.
        domain: String,
        /// The identity the digest is bound to (path or package id).
        identity: String,
    },
    /// A required signature cannot be verified (`exit 4`).
    SignatureUnverified {
        /// The declared scheme that has no shipped verifier.
        scheme: String,
    },
    /// The id/version is recorded in the revocation store (`exit 3`).
    Revoked {
        /// The revoked adapter id.
        id: String,
        /// The revoked version, or `*` for a whole-id revocation.
        version: String,
    },
    /// The package bytes are in quarantine custody (`exit 3`).
    Quarantined {
        /// The quarantined package id.
        id: String,
        /// The quarantined package version.
        version: String,
    },
    /// The trust level requires an explicit opt-in (`exit 3`).
    TrustInsufficient {
        /// The adapter id.
        id: String,
        /// The assigned trust level token.
        level: String,
    },
    /// The discovery source cannot be resolved (`exit 4`).
    SourceUnavailable {
        /// The closed source coordinate token.
        source: String,
    },
    /// A mutating operation ran without a confirmed plan (`exit 1`).
    InstallPlanRequired,
    /// Inputs drifted between preview and confirmation (`exit 1`).
    SourceChanged,
    /// The planned path is occupied by foreign content (`exit 3`).
    InstallConflict {
        /// The store-relative planned path token.
        path: String,
    },
    /// An install journal is ambiguous (`exit 1`).
    RecoveryRequired {
        /// The journal stage token the ambiguity was found at.
        stage: String,
    },
    /// The package declares hooks; v1 refuses non-empty (`exit 4`).
    HooksDeclared {
        /// The declared hook count.
        count: u64,
    },
    /// An update widens permissions without the policy (`exit 3`).
    PermissionEscalated {
        /// The adapter id the update targets.
        adapter: String,
        /// The first widened permission member, in canonical order.
        member: String,
    },
}

impl fmt::Display for PackageFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(crate::adapter_package::diagnostic::reason_of(self))
    }
}

impl From<LockFailure> for PackageFailure {
    fn from(failure: LockFailure) -> Self {
        match failure {
            // The shared grammars refuse with schema-invalid; the package
            // surface spells that as a manifest fault.
            LockFailure::SchemaInvalid | LockFailure::ReferenceInvalid => Self::ManifestInvalid {
                reason: "grammar".to_owned(),
            },
            LockFailure::PrivateData { field } => Self::ManifestInvalid {
                reason: field.to_owned(),
            },
            other => Self::ManifestInvalid {
                reason: format!("lock-{}", other_reason(&other)),
            },
        }
    }
}

fn other_reason(failure: &LockFailure) -> &'static str {
    match failure {
        LockFailure::Missing => "missing",
        LockFailure::Noncanonical => "noncanonical",
        LockFailure::SchemaInvalid | LockFailure::ReferenceInvalid => "grammar",
        LockFailure::PrivateData { .. } => "private-data",
        _ => "invalid",
    }
}

/// Re-export the shared grammars so every package value has exactly one
/// spelling of the closed shapes.
pub(crate) use lock::SemVer;
pub(crate) use lock::Sha256Digest;

/// The identifier of one adapter package: the accepted lowercase
/// component grammar (never a path).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct PackageId(String);

impl PackageId {
    pub(crate) fn parse(text: &str) -> Result<Self, PackageFailure> {
        if crate::lockfile::types::is_private_data(text) {
            return Err(PackageFailure::ManifestInvalid {
                reason: "id".to_owned(),
            });
        }
        let valid = !text.is_empty()
            && text.len() <= 128
            && text.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && text
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if !valid {
            return Err(PackageFailure::ManifestInvalid {
                reason: "id".to_owned(),
            });
        }
        Ok(Self(text.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A package-relative POSIX logical path: no leading dot, no dot
/// segments, no backslash, no drive, never absolute. The Rust grammar is
/// normative and stricter than the schema character class.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct PackagePath(String);

impl PackagePath {
    pub(crate) fn parse(text: &str) -> Result<Self, PackageFailure> {
        let invalid = || PackageFailure::ManifestInvalid {
            reason: "package-path".to_owned(),
        };
        if text.is_empty() || text.len() > 256 {
            return Err(invalid());
        }
        if text.starts_with('/') || text.contains('\\') || text.contains(':') {
            return Err(invalid());
        }
        for segment in text.split('/') {
            if segment.is_empty() || segment.starts_with('.') || segment == ".." {
                return Err(invalid());
            }
        }
        if text.chars().any(|c| c.is_control()) {
            return Err(invalid());
        }
        Ok(Self(text.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PackagePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A project-relative glob scope: never absolute, never a traversal. The
/// scope grammar matches the target-protocol declared-scope spelling.
pub(crate) fn parse_scope(text: &str) -> Result<String, PackageFailure> {
    if text.is_empty() || text.len() > 256 {
        return Err(PackageFailure::ManifestInvalid {
            reason: "scope".to_owned(),
        });
    }
    if text.starts_with('/') || text.starts_with('\\') || text.contains(':') {
        return Err(PackageFailure::ManifestInvalid {
            reason: "scope".to_owned(),
        });
    }
    for segment in text.split(['/', '\\']) {
        if segment == ".." {
            return Err(PackageFailure::ManifestInvalid {
                reason: "scope".to_owned(),
            });
        }
    }
    if text.chars().any(|c| c.is_control()) {
        return Err(PackageFailure::ManifestInvalid {
            reason: "scope".to_owned(),
        });
    }
    Ok(text.to_owned())
}
