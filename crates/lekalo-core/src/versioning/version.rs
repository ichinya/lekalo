//! Strict canonical contract versions for issue #9.
//!
//! Three contract families evolve independently of each other and of the
//! product release: the user-facing Model schema, the compiled IR, and the
//! target/provider process protocol. [`ContractVersion`] wraps
//! `semver::Version` with private fields so library callers cannot forge a
//! version for a family it does not belong to, and
//! [`ContractVersion::parse_canonical`] accepts exactly one spelling per
//! version: ASCII `MAJOR.MINOR.PATCH` with an optional SemVer prerelease and
//! nothing else.
//!
//! Build metadata is rejected outright: SemVer precedence ignores it, so
//! accepting it would let two distinct spellings denote one version and
//! break deterministic registry bytes. Whitespace, a leading `v`, partial
//! components, leading zeroes, overflow, and non-ASCII digits all fail; the
//! round-trip check (`parsed.to_string() == input`) closes every residual
//! spelling gap the `semver` crate tolerates.

use std::fmt;

use super::family::ContractFamily;

/// A canonical contract version bound to one [`ContractFamily`].
///
/// Construction goes through [`ContractVersion::parse_canonical`] or the
/// exhaustive finite-enum conversions; both produce the same value space.
/// Ordering and equality are SemVer precedence (build metadata cannot exist,
/// so precedence is total).
#[derive(Clone, Debug)]
pub struct ContractVersion<F: ContractFamily> {
    inner: semver::Version,
    canonical: String,
    _family: std::marker::PhantomData<fn() -> F>,
}

impl<F: ContractFamily> ContractVersion<F> {
    /// Parse one canonical `MAJOR.MINOR.PATCH[-PRERELEASE]` spelling.
    ///
    /// Build metadata, surrounding whitespace, a leading `v`, missing
    /// components, and every non-canonical spelling are rejected. Prerelease
    /// spellings parse; whether such a version is *supported* is a registry
    /// decision, never a parsing decision.
    pub fn parse_canonical(text: &str) -> Result<Self, VersionParseError> {
        if text.is_empty() {
            return Err(VersionParseError::Empty);
        }
        if !text.is_ascii() {
            return Err(VersionParseError::NotAscii);
        }
        let parsed = semver::Version::parse(text).map_err(|_| VersionParseError::Malformed)?;
        if !parsed.build.is_empty() {
            return Err(VersionParseError::BuildMetadata);
        }
        if parsed.to_string() != text {
            return Err(VersionParseError::Malformed);
        }
        Ok(Self {
            inner: parsed,
            canonical: text.to_owned(),
            _family: std::marker::PhantomData,
        })
    }

    /// The major component.
    pub const fn major(&self) -> u64 {
        self.inner.major
    }

    /// The minor component.
    pub const fn minor(&self) -> u64 {
        self.inner.minor
    }

    /// The patch component.
    pub const fn patch(&self) -> u64 {
        self.inner.patch
    }

    /// The prerelease of this version, empty for a release.
    pub fn prerelease(&self) -> &semver::Prerelease {
        &self.inner.pre
    }

    /// Whether this is a prerelease (syntactically valid, registry-gated).
    pub fn is_prerelease(&self) -> bool {
        !self.inner.pre.is_empty()
    }

    /// The canonical spelling; identical to the accepted input.
    pub fn as_str(&self) -> &str {
        &self.canonical
    }
}

impl<F: ContractFamily> PartialEq for ContractVersion<F> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<F: ContractFamily> Eq for ContractVersion<F> {}

impl<F: ContractFamily> PartialOrd for ContractVersion<F> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<F: ContractFamily> Ord for ContractVersion<F> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}

impl<F: ContractFamily> fmt::Display for ContractVersion<F> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.canonical)
    }
}

/// Why a version spelling was rejected by
/// [`ContractVersion::parse_canonical`].
///
/// The variants are closed and stable; they carry no raw input so a failure
/// envelope can never echo a hostile selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VersionParseError {
    /// The selector carried no version text.
    Empty,
    /// The selector contained non-ASCII characters.
    NotAscii,
    /// Build metadata is not part of any Lekalo contract version.
    BuildMetadata,
    /// Everything else: partial components, leading `v` or zeroes,
    /// whitespace, overflow, malformed prereleases, non-canonical text.
    Malformed,
}

impl VersionParseError {
    /// The stable wire detail for diagnostics and human lines.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::NotAscii => "not-ascii",
            Self::BuildMetadata => "build-metadata",
            Self::Malformed => "malformed",
        }
    }
}
