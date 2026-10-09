//! The independent module API version surface (issue #113).
//!
//! Module API SemVer, the integer definition revision, and the Lekalo
//! product/contract versions are separate quantities; nothing here derives
//! from `Common.version` or the product version. Canonical stable SemVer is
//! the only accepted spelling; prerelease/build syntax beyond the selected
//! policy is unsupported, never approximated.

use super::wire::VersionPolicyWire;

/// One parsed canonical `MAJOR.MINOR.PATCH` API version.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ApiVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

impl ApiVersion {
    /// Parse one canonical stable SemVer spelling (no prerelease/build).
    pub fn parse(text: &str) -> Option<Self> {
        if text.is_empty() || text.len() > 32 {
            return None;
        }
        let mut parts = text.split('.');
        let major = parts.next()?;
        let minor = parts.next()?;
        let patch = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        let parse = |part: &str| -> Option<u64> {
            if part.is_empty() || (part.len() > 1 && part.starts_with('0')) {
                return None;
            }
            part.bytes()
                .all(|byte| byte.is_ascii_digit())
                .then(|| part.parse().ok())?
        };
        Some(Self {
            major: parse(major)?,
            minor: parse(minor)?,
            patch: parse(patch)?,
        })
    }

    pub const fn major(&self) -> u64 {
        self.major
    }
    pub const fn minor(&self) -> u64 {
        self.minor
    }
    pub const fn patch(&self) -> u64 {
        self.patch
    }
}

impl std::fmt::Display for ApiVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The bounded compatibility interval of a `version-range` policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionInterval {
    pub lower: ApiVersion,
    pub lower_inclusive: bool,
    pub upper: Option<ApiVersion>,
    pub upper_inclusive: bool,
}

impl VersionInterval {
    /// Parse and check the interval: an empty or inverted interval is
    /// invalid, never a silent always-match.
    pub fn parse(
        lower: &str,
        lower_inclusive: bool,
        upper: Option<&str>,
        upper_inclusive: bool,
    ) -> Option<Self> {
        let lower = ApiVersion::parse(lower)?;
        let upper = match upper.map(ApiVersion::parse) {
            Some(Some(parsed)) => Some(parsed),
            Some(None) => return None,
            None => None,
        };
        if let Some(upper) = &upper {
            match upper.cmp(&lower) {
                std::cmp::Ordering::Less => return None,
                std::cmp::Ordering::Equal if !(lower_inclusive && upper_inclusive) => {
                    return None;
                }
                _ => {}
            }
        }
        Some(Self {
            lower,
            lower_inclusive,
            upper,
            upper_inclusive,
        })
    }

    /// Whether one version lies inside the declared interval.
    pub fn contains(&self, version: &ApiVersion) -> bool {
        let lower_ok = match version.cmp(&self.lower) {
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => self.lower_inclusive,
            std::cmp::Ordering::Greater => true,
        };
        if !lower_ok {
            return false;
        }
        match &self.upper {
            None => true,
            Some(upper) => match version.cmp(upper) {
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Equal => self.upper_inclusive,
                std::cmp::Ordering::Greater => false,
            },
        }
    }
}

/// The typed consumer resolution policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConsumerPolicy {
    ExactDigest(String),
    ExactVersion(ApiVersion),
    Range(VersionInterval),
}

impl ConsumerPolicy {
    /// The stable policy spelling used by reports and diagnostics.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ExactDigest(_) => "exact-digest",
            Self::ExactVersion(_) => "exact-version",
            Self::Range(_) => "version-range",
        }
    }

    /// Parse a wire policy; malformed intervals and digests are invalid
    /// input, not unknown policy.
    pub fn parse(policy: &VersionPolicyWire) -> Option<Self> {
        match policy {
            VersionPolicyWire::ExactDigest { digest } => {
                is_digest(digest).then(|| Self::ExactDigest(digest.clone()))
            }
            VersionPolicyWire::ExactVersion { version } => {
                ApiVersion::parse(version).map(Self::ExactVersion)
            }
            VersionPolicyWire::VersionRange {
                lower,
                lower_inclusive,
                upper,
                upper_inclusive,
            } => {
                VersionInterval::parse(lower, *lower_inclusive, upper.as_deref(), *upper_inclusive)
                    .map(Self::Range)
            }
        }
    }

    /// Whether the policy accepts the public resolution: the exact digest,
    /// the exact version, or a contained version.
    pub fn accepts(&self, digest: &str, version: Option<&str>) -> bool {
        match self {
            Self::ExactDigest(expected) => expected == digest,
            Self::ExactVersion(expected) => version
                .and_then(ApiVersion::parse)
                .is_some_and(|actual| actual == *expected),
            Self::Range(interval) => version
                .and_then(ApiVersion::parse)
                .is_some_and(|actual| interval.contains(&actual)),
        }
    }
}

/// The documented bump policy for stable majors (>= 1): an incompatible
/// public change demands a major bump, additive public surface at least a
/// minor bump, and compatible behavior a patch bump. Pre-1.0 modules
/// explicitly pin minor-breaking behavior; digest-only modules carry no
/// version claim at all.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequiredBump {
    None,
    Patch,
    Minor,
    Major,
}

impl RequiredBump {
    /// The stable spelling used by reports and diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Patch => "patch",
            Self::Minor => "minor",
            Self::Major => "major",
        }
    }

    /// Evaluate the required bump between two API versions under the
    /// declared policy kind of the module. `changed` reports whether any
    /// semantic change exists at all: identical bundles demand no bump
    /// regardless of version equality (finding: identical bundles diffed
    /// as breaking with a phantom patch requirement).
    pub fn evaluate(
        before: Option<&ApiVersion>,
        after: Option<&ApiVersion>,
        changed: bool,
        breaking: bool,
        additive: bool,
    ) -> Self {
        let (Some(before), Some(after)) = (before, after) else {
            // Digest-only or newly versioned: no SemVer claim to evaluate.
            return Self::None;
        };
        if !changed {
            return Self::None;
        }
        if after.major != before.major {
            return Self::None; // the major segment changed; compliant
        }
        if breaking {
            if before.major >= 1 {
                return Self::Major;
            }
            // Pre-1.0 minor-breaking policy: a breaking change must move
            // the minor segment.
            return if after.minor != before.minor {
                Self::None
            } else {
                Self::Minor
            };
        }
        if additive {
            // Additive public surface needs at least a minor bump on
            // stable majors; pre-1.0 modules may ship additive surface in
            // a patch (finding: patch bump accepted additive change).
            return if after.minor != before.minor {
                Self::None
            } else if after.major >= 1 {
                Self::Minor
            } else {
                Self::None
            };
        }
        if after.patch != before.patch || after.minor != before.minor {
            Self::None
        } else {
            Self::Patch
        }
    }
}

/// Whether `text` is a well-formed `sha256:<64 lowercase hex>` digest.
pub fn is_digest(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("sha256:") else {
        return false;
    };
    rest.len() == 64
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_versions_parse_and_sort() {
        let a = ApiVersion::parse("1.2.0").expect("parses");
        let b = ApiVersion::parse("1.10.0").expect("parses");
        assert!(a < b);
        assert_eq!(a.to_string(), "1.2.0");
        assert!(ApiVersion::parse("1.2").is_none());
        assert!(ApiVersion::parse("01.2.0").is_none());
        assert!(ApiVersion::parse("1.2.0-beta").is_none());
        assert!(ApiVersion::parse("1.2.0+build").is_none());
    }

    #[test]
    fn intervals_reject_empty_and_inverted_bounds() {
        let closed = VersionInterval::parse("1.0.0", true, Some("2.0.0"), false).expect("parses");
        assert!(closed.contains(&ApiVersion::parse("1.0.0").unwrap()));
        assert!(!closed.contains(&ApiVersion::parse("2.0.0").unwrap()));
        assert!(closed.contains(&ApiVersion::parse("1.9.9").unwrap()));
        assert!(VersionInterval::parse("2.0.0", true, Some("1.0.0"), true).is_none());
        assert!(VersionInterval::parse("1.0.0", false, Some("1.0.0"), false).is_none());
        assert!(VersionInterval::parse("1.0.0", true, Some("1.0.0"), true).is_some());
        let open = VersionInterval::parse("1.0.0", true, None, false).expect("open upper");
        assert!(open.contains(&ApiVersion::parse("99.0.0").unwrap()));
    }

    #[test]
    fn policies_accept_only_their_selected_resolution() {
        let digest = ConsumerPolicy::ExactDigest(format!("sha256:{}", "a".repeat(64)));
        assert!(digest.accepts(&format!("sha256:{}", "a".repeat(64)), Some("1.0.0")));
        assert!(!digest.accepts(&format!("sha256:{}", "b".repeat(64)), Some("1.0.0")));
        let exact = ConsumerPolicy::ExactVersion(ApiVersion::parse("1.2.0").unwrap());
        assert!(exact.accepts("other", Some("1.2.0")));
        assert!(!exact.accepts("other", Some("1.2.1")));
        let range = ConsumerPolicy::Range(
            VersionInterval::parse("1.0.0", true, Some("2.0.0"), false).unwrap(),
        );
        assert!(range.accepts("other", Some("1.9.0")));
        assert!(!range.accepts("other", Some("2.0.0")));
        assert!(!range.accepts("other", None), "no version, no range match");
    }

    #[test]
    fn bump_policy_follows_stable_major_and_pre_1_rules() {
        let v = |text: &str| ApiVersion::parse(text).expect("version");
        // Stable majors require a major bump on breaking changes.
        assert_eq!(
            RequiredBump::evaluate(Some(&v("2.0.0")), Some(&v("2.0.0")), true, true, false),
            RequiredBump::Major
        );
        assert_eq!(
            RequiredBump::evaluate(Some(&v("2.0.0")), Some(&v("2.1.0")), true, true, false),
            RequiredBump::Major
        );
        assert_eq!(
            RequiredBump::evaluate(Some(&v("2.0.0")), Some(&v("3.0.0")), true, true, false),
            RequiredBump::None
        );
        // Pre-1.0 minor-breaking policy.
        assert_eq!(
            RequiredBump::evaluate(Some(&v("0.3.0")), Some(&v("0.3.0")), true, true, false),
            RequiredBump::Minor
        );
        assert_eq!(
            RequiredBump::evaluate(Some(&v("0.3.0")), Some(&v("0.4.0")), true, true, false),
            RequiredBump::None
        );
        // Additive surface needs at least a minor on stable majors; a
        // patch-only bump is a violation (finding 11).
        assert_eq!(
            RequiredBump::evaluate(Some(&v("1.2.0")), Some(&v("1.2.0")), true, false, true),
            RequiredBump::Minor
        );
        assert_eq!(
            RequiredBump::evaluate(Some(&v("1.2.0")), Some(&v("1.2.1")), true, false, true),
            RequiredBump::Minor
        );
        assert_eq!(
            RequiredBump::evaluate(Some(&v("1.2.0")), Some(&v("1.3.0")), true, false, true),
            RequiredBump::None
        );
        // Pre-1.0 additive surface may ship in a patch.
        assert_eq!(
            RequiredBump::evaluate(Some(&v("0.3.0")), Some(&v("0.3.1")), true, false, true),
            RequiredBump::None
        );
        // Compatible behavior needs a patch.
        assert_eq!(
            RequiredBump::evaluate(Some(&v("1.2.0")), Some(&v("1.2.0")), true, false, false),
            RequiredBump::Patch
        );
        // Identical bundles never demand a bump, independent of version
        // equality (finding 9).
        assert_eq!(
            RequiredBump::evaluate(Some(&v("1.2.0")), Some(&v("1.2.0")), false, false, false),
            RequiredBump::None
        );
        assert_eq!(
            RequiredBump::evaluate(Some(&v("3.1.4")), Some(&v("3.1.4")), false, false, false),
            RequiredBump::None
        );
        // Digest-only modules carry no SemVer claim.
        assert_eq!(
            RequiredBump::evaluate(None, None, true, true, false),
            RequiredBump::None
        );
    }
}
