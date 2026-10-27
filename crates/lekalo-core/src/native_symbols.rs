//! Bounded native declaration metadata carried by the 1.1.0 scan seam.
//! Names and locations are evidence, never inferred business semantics.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Maximum declarations in one accepted native inventory.
pub const MAX_SYMBOLS: usize = 10_000;
/// Maximum native declarations owned by one public scan entry.
pub const MAX_ENTRY_SYMBOLS: usize = 256;

/// An explicitly retained unknown/any type surface. It does not make the
/// declaration inventory incomplete or prove native contract conformance.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeUncertainty {
    pub path: String,
    pub kind: String,
    pub detail: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub line: Option<u64>,
    pub source_digest: String,
}

/// Closed, source-bound uncertainty records; other uncertainty still refuses.
pub fn valid_uncertainty(rows: &[NativeUncertainty]) -> bool {
    rows.len() <= 4096
        && rows.windows(2).all(|p| p[0] < p[1])
        && rows.iter().all(|r| {
            crate::project_fs::path_violation(&r.path).is_none()
                && r.kind == "any-surface"
                && text(&r.detail, 64)
                && r.line.is_none_or(|l| l > 0 && l <= 1_000_000)
                && crate::trace::id::is_digest(&r.source_digest)
        })
}

/// One native declaration from a confined, source-digest-bound scan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSymbol {
    /// Adapter-owned stable native identity.
    pub native: String,
    /// Exact lexical name, preserving case and Unicode.
    pub qualified_name: String,
    /// Sorted declaration-family tokens from the producer.
    pub family: String,
    /// Portable project-relative source path.
    pub path: String,
    /// One-based first source line.
    pub line: u64,
    /// One-based final source line.
    pub end_line: u64,
    /// Static/instance identity slot, not an execution claim.
    pub slot: String,
    /// Whether the declaration comes from a declaration-only source file.
    pub declaration_only: bool,
    /// Lexical owner's native identity; absent for roots.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub member_of: Option<String>,
    /// Structural signature digest; absence stays unknown.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub signature: Option<String>,
}

fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

fn text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

fn family(value: &str) -> bool {
    let words: Vec<_> = value.split('+').collect();
    words.len() <= 10
        && words.iter().all(|v| {
            matches!(
                *v,
                "class"
                    | "interface"
                    | "enum"
                    | "type-alias"
                    | "function"
                    | "method"
                    | "variable"
                    | "property"
                    | "module"
                    | "symbol"
            )
        })
        && words.windows(2).all(|p| p[0] < p[1])
}

/// Validate an ordered, closed forest. Reject duplicates, dangling owners,
/// cycles and children outside the owner's source span before persistence.
pub fn valid(symbols: &[NativeSymbol]) -> bool {
    if symbols.len() > MAX_SYMBOLS || !symbols.windows(2).all(|p| p[0].native < p[1].native) {
        return false;
    }
    let all: BTreeMap<_, _> = symbols.iter().map(|s| (s.native.as_str(), s)).collect();
    let mut owners = BTreeMap::<&str, usize>::new();
    for symbol in symbols {
        if !text(&symbol.native, 256)
            || !text(&symbol.qualified_name, 256)
            || !family(&symbol.family)
            || crate::project_fs::path_violation(&symbol.path).is_some()
            || symbol.line == 0
            || symbol.end_line < symbol.line
            || symbol.end_line > 1_000_000
            || !matches!(symbol.slot.as_str(), "static" | "instance")
            || symbol
                .signature
                .as_ref()
                .is_some_and(|s| !crate::trace::id::is_digest(s))
        {
            return false;
        }
        let mut current = symbol;
        let mut visited = BTreeSet::new();
        while let Some(owner) = &current.member_of {
            if visited.len() >= 64 || !visited.insert(current.native.as_str()) {
                return false;
            }
            let Some(parent) = all.get(owner.as_str()) else {
                return false;
            };
            if parent.path != current.path
                || parent.line > current.line
                || parent.end_line < current.end_line
            {
                return false;
            }
            current = parent;
        }
        let count = owners.entry(current.native.as_str()).or_default();
        *count += 1;
        if *count > MAX_ENTRY_SYMBOLS {
            return false;
        }
    }
    true
}

/// A scan entry owns exactly one complete lexical subtree in one file.
pub fn valid_entry(symbols: &[NativeSymbol], native: &str, path: &str) -> bool {
    valid(symbols)
        && !symbols.is_empty()
        && symbols.len() <= MAX_ENTRY_SYMBOLS
        && symbols.iter().all(|s| s.path == path)
        && symbols.iter().filter(|s| s.member_of.is_none()).count() == 1
        && symbols
            .iter()
            .any(|s| s.native == native && s.member_of.is_none())
}
