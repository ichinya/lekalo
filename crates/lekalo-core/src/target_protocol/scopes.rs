//! Adapter scopes and the protected canonical homes (issue #27).
//!
//! An adapter declares the logical read and write scopes it needs during the
//! `describe` handshake. Core enforces three closed rules before any
//! operation may run:
//!
//! 1. Scope grammar: portable lowercase segments joined by `/`, optionally
//!    ending in the recursive `/**` segment. Segments may start with `.`
//!    (the governed `.lekalo` homes), but the traversal segment `..` never
//!    parses, and absolute paths, backslashes, and drive letters never do.
//! 2. Protected homes: the canonical Lekalo and OpenSpec trees can never be
//!    covered by a declared write scope and can never appear in a write
//!    plan. Reading them through a read scope stays legal (the IR home is
//!    exactly how an adapter consumes compiled input).
//! 3. Coverage: every write a plan declares must sit inside one declared
//!    write scope, and every request path the client sends (the IR home)
//!    must sit inside a declared read scope.

/// The closed list of canonical homes adapters may never write.
///
/// Segment sets: a logical path is protected when its leading segments are
/// exactly one of these sets. `lekalo.lock` is a file; the rest are trees
/// (the canonical `lekalo/` model home, the OpenSpec home, and the governed
/// `.lekalo/` runtime homes their owning issues adjudicate).
pub const PROTECTED_HOMES: [(&str, &[&str]); 8] = [
    ("lekalo-model", &["lekalo"]),
    ("lekalo-lockfile", &["lekalo.lock"]),
    ("ir", &[".lekalo", "ir"]),
    ("cache", &[".lekalo", "cache"]),
    ("import", &[".lekalo", "import"]),
    ("privacy", &[".lekalo", "privacy"]),
    ("consumer", &[".lekalo", "consumer"]),
    ("openspec", &["openspec"]),
];

/// Maximum path or scope length in bytes.
const MAX_VALUE_BYTES: usize = 512;

/// Maximum one-segment length in bytes.
const MAX_SEGMENT_BYTES: usize = 64;

/// Whether one path segment is a portable scope/path segment.
fn segment_ok(segment: &str) -> bool {
    let portable = if let Some(tail) = segment.strip_prefix('.') {
        format!("x{tail}")
    } else {
        segment.to_owned()
    };
    if segment == "."
        || segment == ".."
        || segment.ends_with('.')
        || crate::project_fs::path_violation(&portable).is_some()
        || segment.is_empty()
        || segment.len() > MAX_SEGMENT_BYTES
    {
        return false;
    }
    let mut chars = segment.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit() || first == '.' => {}
        _ => return false,
    }
    segment
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-' || c == '_')
}

/// Split a logical path or scope into segments; `None` when malformed.
fn segments(value: &str) -> Option<Vec<&str>> {
    if value.is_empty() || value.len() > MAX_VALUE_BYTES || value.starts_with('/') {
        return None;
    }
    let parts: Vec<&str> = value.split('/').collect();
    if parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    Some(parts)
}

/// Whether `value` is a grammatical logical path (no recursion marker).
pub fn is_logical_path(value: &str) -> bool {
    match segments(value) {
        Some(parts) => parts.iter().all(|p| segment_ok(p)),
        None => false,
    }
}

/// Whether `value` is a grammatical scope: like a logical path, but the
/// final segment may be the recursive `**`.
pub fn is_scope(value: &str) -> bool {
    match segments(value) {
        Some(parts) => match parts.split_last() {
            Some((last, head)) => {
                head.iter().all(|p| segment_ok(p))
                    && ((*last == "**" && !head.is_empty()) || segment_ok(last))
            }
            None => false,
        },
        None => false,
    }
}

pub fn scope_covers(scope: &str, path: &str) -> bool {
    if !is_scope(scope) || !is_logical_path(path) {
        return false;
    }
    let (Some(scope_parts), Some(path_parts)) = (segments(scope), segments(path)) else {
        return false;
    };
    let recursive = scope_parts.last() == Some(&"**");
    let prefix: &[&str] = if recursive {
        &scope_parts[..scope_parts.len() - 1]
    } else {
        &scope_parts[..]
    };
    if recursive {
        // `a/b/**` covers everything strictly below `a/b`.
        path_parts.len() > prefix.len() && path_parts[..prefix.len()] == *prefix
    } else {
        path_parts == prefix
    }
}

/// The protected home a write scope would cover, if any.
///
/// A scope whose leading segments reach a protected home (recursive or
/// exact) makes the whole declaration a policy refusal.
pub fn scope_touches_protected_home(scope: &str) -> Option<&'static str> {
    let parts = segments(scope)?;
    let recursive = parts.last() == Some(&"**");
    let prefix: &[&str] = if recursive {
        &parts[..parts.len() - 1]
    } else {
        &parts[..]
    };
    for (name, home) in PROTECTED_HOMES {
        // The scope covers the home when its prefix is shallower than the
        // home (`.lekalo/**`) or reaches inside it (`.lekalo/ir/x/**`).
        let inside = prefix.len() >= home.len() && prefix[..home.len()] == *home;
        let above = prefix.len() <= home.len() && home[..prefix.len()] == *prefix;
        if inside || above {
            return Some(name);
        }
    }
    None
}

/// The protected home a concrete path sits in, if any.
pub fn protected_home(path: &str) -> Option<&'static str> {
    let parts = segments(path)?;
    for (name, home) in PROTECTED_HOMES {
        if parts.len() >= home.len() && parts[..home.len()] == *home {
            return Some(name);
        }
    }
    None
}

/// Whether one adapter identity id/target/profile token is grammatical.
pub fn is_token(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_grammar_accepts_portable_forms_only() {
        assert!(is_scope(".lekalo/generated/node-typescript/**"));
        assert!(is_scope("src/**"));
        assert!(is_scope("gen/out.txt"));
        assert!(!is_scope("/abs"));
        assert!(!is_scope("a/../b"));
        assert!(!is_scope(".."));
        assert!(!is_scope("A/up"));
        assert!(!is_scope("a//b"));
        assert!(!is_scope(""));
        assert!(!is_scope("src/**/x"));
        assert!(!is_logical_path("src/**"));
        assert!(!is_logical_path("a/../b"));
        assert!(is_logical_path(".lekalo/ir/planner.json"));
    }

    #[test]
    fn coverage_is_prefix_exact() {
        assert!(scope_covers("src/**", "src/a/b.ts"));
        assert!(scope_covers("src/**", "src/a"));
        assert!(!scope_covers("src/**", "srcx/a"));
        assert!(scope_covers("gen/out.txt", "gen/out.txt"));
        assert!(!scope_covers("gen/out.txt", "gen/other.txt"));
        assert!(!scope_covers("src/**", "src"));
    }

    #[test]
    fn protected_homes_refuse_writes_but_not_reads() {
        assert_eq!(protected_home("lekalo/project.yaml"), Some("lekalo-model"));
        assert_eq!(protected_home("lekalo.lock"), Some("lekalo-lockfile"));
        assert_eq!(protected_home(".lekalo/ir/planner.json"), Some("ir"));
        assert_eq!(protected_home(".lekalo/cache/x"), Some("cache"));
        assert_eq!(protected_home("openspec/change/1.md"), Some("openspec"));
        assert_eq!(protected_home("src/main.ts"), None);
        assert_eq!(
            scope_touches_protected_home("lekalo/**"),
            Some("lekalo-model")
        );
        assert_eq!(scope_touches_protected_home(".lekalo/**"), Some("ir"));
        assert_eq!(scope_touches_protected_home(".lekalo/generated/**"), None);
        assert_eq!(scope_touches_protected_home("src/**"), None);
    }

    #[test]
    fn tokens_are_lowercase_identifiers() {
        assert!(is_token("node-typescript"));
        assert!(is_token("default"));
        assert!(!is_token("Node"));
        assert!(!is_token(""));
        assert!(!is_token("a_b"));
    }
}
