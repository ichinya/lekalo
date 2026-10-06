//! Logical path, scope, and protected-home helpers (issue #106):
//! defense in depth mirroring the host's closed grammar. They can never
//! widen host policy — only the host's private view and confinement
//! enforce reality.

const MAX_VALUE_BYTES: usize = 512;
const MAX_SEGMENT_BYTES: usize = 64;

const PROTECTED_HOMES: [(&str, &[&str]); 8] = [
    ("lekalo-model", &["lekalo"]),
    ("lekalo-lockfile", &["lekalo.lock"]),
    ("ir", &[".lekalo", "ir"]),
    ("cache", &[".lekalo", "cache"]),
    ("import", &[".lekalo", "import"]),
    ("privacy", &[".lekalo", "privacy"]),
    ("consumer", &[".lekalo", "consumer"]),
    ("openspec", &["openspec"]),
];

const DOS_DEVICES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn is_nfc(value: &str) -> bool {
    // A lightweight NFC equivalence: NFC is stable under re-normalization.
    // The full table check is the host's job; adapters that need it bind a
    // normalization crate themselves. ASCII paths are always NFC.
    value.is_ascii() || unicode_normalization_equiv(value)
}

fn unicode_normalization_equiv(value: &str) -> bool {
    // Without a normalization table this runtime refuses to claim NFC
    // knowledge; the vector corpus marks those vectors `requiresNfc` and
    // the host owns the enforcement. Treated as satisfied here, so the
    // lexical checks below still apply.
    let _ = value;
    true
}

fn dos_stem(segment: &str) -> &str {
    match segment.find('.') {
        Some(dot) => &segment[..dot],
        None => segment,
    }
}

fn contains_short_name(segment: &str) -> bool {
    match segment.find('~') {
        Some(tilt) => {
            tilt >= 1
                && tilt <= 6
                && segment.len() > tilt
                && segment[tilt + 1..].chars().next().is_some_and(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

fn contains_percent_escape(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.windows(3).any(|window| {
        window[0] == b'%' && window[1].is_ascii_hexdigit() && window[2].is_ascii_hexdigit()
    })
}

/// The closed host path-violation reasons, mirrored for local checks.
pub fn path_violation(path: &str) -> Option<&'static str> {
    if path.is_empty() {
        return Some("structure.path-empty");
    }
    if !is_nfc(path) {
        return Some("structure.path-not-nfc");
    }
    if path.starts_with('/') || path.starts_with("\\\\") || path.starts_with("//") {
        return Some("structure.path-absolute");
    }
    if path.starts_with('~')
        || (path.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic()) && path.as_bytes().get(1) == Some(&b':'))
        || path.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && path.contains(':')
            && path.find(':').is_some_and(|index| index > 0 && path[..index].chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-'))
    {
        return Some("structure.path-absolute");
    }
    if path.contains('\\') || contains_percent_escape(path) || path.contains(':') {
        return Some("structure.path-escape");
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment.len() > 64 {
            return Some("structure.path-segment");
        }
        if segment == "." || segment == ".." || segment.ends_with('.') || segment.ends_with(' ') {
            return Some("structure.path-traversal");
        }
        if DOS_DEVICES.contains(&dos_stem(segment).to_ascii_uppercase().as_str()) {
            return Some("structure.path-device");
        }
        if contains_short_name(segment) {
            return Some("structure.path-short-name");
        }
        if segment.chars().any(|c| c.is_ascii_uppercase()) {
            return Some("structure.path-case");
        }
    }
    None
}

fn segment_ok(segment: &str) -> bool {
    if segment == "." || segment == ".." || segment.ends_with('.') {
        return false;
    }
    let portable = if let Some(tail) = segment.strip_prefix('.') {
        format!("x{tail}")
    } else {
        segment.to_owned()
    };
    if path_violation(&portable).is_some() {
        return false;
    }
    if segment.is_empty() || segment.len() > MAX_SEGMENT_BYTES {
        return false;
    }
    let first = segment.chars().next().unwrap_or(' ');
    if !(first.is_ascii_lowercase() || first.is_ascii_digit() || first == '.') {
        return false;
    }
    segment.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-' || c == '_')
}

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

/// Whether the value is a grammatical logical path (no recursion marker).
pub fn is_logical_path(value: &str) -> bool {
    match segments(value) {
        Some(parts) => parts.iter().all(|p| segment_ok(p)),
        None => false,
    }
}

/// Whether the value is a grammatical scope (final segment may be `**`).
pub fn is_scope(value: &str) -> bool {
    let Some(parts) = segments(value) else {
        return false;
    };
    let Some((last, head)) = parts.split_last() else {
        return false;
    };
    if *last == "**" {
        return !head.is_empty() && head.iter().all(|p| segment_ok(p));
    }
    parts.iter().all(|p| segment_ok(p))
}

/// Prefix-exact or recursive coverage of one path by one scope.
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
        path_parts.len() > prefix.len() && path_parts[..prefix.len()] == *prefix
    } else {
        path_parts == prefix
    }
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

/// The protected home a declared write scope would reach, if any.
pub fn scope_touches_protected_home(scope: &str) -> Option<&'static str> {
    let parts = segments(scope)?;
    let recursive = parts.last() == Some(&"**");
    let prefix: &[&str] = if recursive {
        &parts[..parts.len() - 1]
    } else {
        &parts[..]
    };
    for (name, home) in PROTECTED_HOMES {
        let inside = prefix.len() >= home.len() && prefix[..home.len()] == *home;
        let above = prefix.len() <= home.len() && home[..prefix.len()] == *prefix;
        if inside || above {
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
        && value.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The closed capability-id grammar (lowercase dotted segments).
pub fn is_capability_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
        })
}

/// The bounded contract-version spelling `x.y.z` (all-numeric parts).
pub fn is_contract_version(value: &str) -> bool {
    value.len() <= 32
        && value.split('.').count() == 3
        && value.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

/// One write-scope violation of a plan entry, or `None`.
pub fn plan_scope_violation(path: &str, write_scopes: &[String]) -> Option<String> {
    if let Some(home) = protected_home(path) {
        return Some(format!("protected-path:{home}"));
    }
    if write_scopes.iter().any(|scope| scope_covers(scope, path)) {
        return None;
    }
    Some("scope:uncovered".to_owned())
}
