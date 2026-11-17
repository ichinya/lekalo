//! Go project discovery; pure byte inspection, never `go` execution.
#![forbid(unsafe_code)]

pub const MANIFESTS: &[(&str, &str)] = &[("go.mod", "go")];
pub const MODULE_MANIFEST: &str = "go.mod";
pub const WORKSPACE_MANIFEST: &str = "go.work";
pub const FILE_HINTS: &[(&str, &str, &str)] = &[("go.mod", "go", "high")];
pub const GATES: &[(&str, &str, &str)] = &[
    ("go.mod", "build", "go build ./..."),
    ("go.mod", "test", "go test ./..."),
];

pub fn go_work_members(bytes: &[u8], limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Vec::new();
    };
    let mut members = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if line == "use (" {
            inside = true;
            continue;
        }
        if inside && line == ")" {
            break;
        }
        if inside {
            if let Some(member) = line.strip_prefix("./") {
                members.push(member.to_owned());
            } else if !line.is_empty() {
                members.push(line.to_owned());
            }
            if members.len() >= limit {
                break;
            }
        }
    }
    members
}

pub fn go_module_path_bytes(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("module ") {
            return Some(rest.trim().to_owned());
        }
    }
    None
}
