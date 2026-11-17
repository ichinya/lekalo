//! Cargo manifest discovery; line-scan evidence, not Cargo/TOML execution.
#![forbid(unsafe_code)]

pub const MANIFESTS: &[(&str, &str)] = &[("Cargo.toml", "rust")];
pub const MODULE_MANIFEST: &str = "Cargo.toml";
pub const LOCKFILES: &[(&str, &str)] = &[("Cargo.lock", "cargo")];
pub const FILE_HINTS: &[(&str, &str, &str)] = &[("Cargo.toml", "rust", "high")];
pub const GATES: &[(&str, &str, &str)] = &[
    ("Cargo.toml", "build", "cargo build"),
    ("Cargo.toml", "test", "cargo test"),
];

pub fn has_cargo_workspace(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes)
        .map(|text| {
            text.lines().any(|line| {
                let line = line.trim();
                line == "[workspace]" || line.starts_with("[workspace.")
            })
        })
        .unwrap_or(false)
}

pub fn cargo_workspace_members(bytes: &[u8], limit: usize) -> Vec<String> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Vec::new();
    };
    let mut members = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if !inside {
            inside = line == "members = [" || line.starts_with("members = [");
            let inline = line.trim_start_matches("members = [").trim_end_matches(']');
            if inside {
                for part in inline.split(',') {
                    push_member(&mut members, part, limit);
                }
                if line.ends_with(']') {
                    break;
                }
                continue;
            }
            continue;
        }
        if line.starts_with(']') || line.ends_with(']') {
            push_member(&mut members, line.trim_end_matches(']'), limit);
            break;
        }
        push_member(&mut members, line, limit);
        if members.len() >= limit {
            break;
        }
    }
    members
}

fn push_member(members: &mut Vec<String>, raw: &str, limit: usize) {
    let raw = raw
        .trim()
        .trim_end_matches(',')
        .trim()
        .trim_matches('"')
        .trim_matches('\'');
    if !raw.is_empty() && members.len() < limit {
        members.push(raw.to_owned());
    }
}

pub fn cargo_package_name_bytes(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = trimmed == "[package]";
            continue;
        }
        if inside {
            if let Some(rest) = trimmed.strip_prefix("name") {
                let rest = rest.trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    let value = rest.trim().trim_matches('"');
                    if !value.is_empty() {
                        return Some(value.to_owned());
                    }
                }
            }
        }
    }
    None
}
