//! `ResolvedProjectProfile` admission, mirroring the node kernel's
//! validator (issue #146): closed member set, lexical root spellings,
//! derived `scope` per root, no overlapping roots, and the canonical
//! profile digest the 0.9.0+ scan custody binds to.

use lekalo_adapter_sdk::ids::sha256_hex;
use lekalo_adapter_sdk::json::{canonical, strict_parse, Value};
use lekalo_adapter_sdk::paths::{is_logical_path, is_scope, is_token, path_violation};

pub const PROFILE_FLAG: &str = "--lekalo-project-profile-json";
const PROFILE_MAX_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug)]
pub struct Root {
    pub kind: RootKind,
    pub path: String,
    pub scope: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RootKind {
    File,
    Tree,
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub id: String,
    pub roots: Vec<Root>,
    pub exclusions: Vec<String>,
    pub digest: String,
}

impl Profile {
    /// Whether `path` is pruned by a declared exclusion. A bare logical
    /// path excludes the named entry and everything below it; a `dir/**`
    /// scope excludes the subtree.
    pub fn excludes(&self, path: &str) -> bool {
        self.exclusions.iter().any(|exclusion| {
            lekalo_adapter_sdk::paths::scope_covers(exclusion, path)
                || path == exclusion
                || path.starts_with(&format!("{exclusion}/"))
        })
    }
}

/// Parse and validate the injected launch profile. Refusals return the
/// closed reason token; the process exits before serving any request.
pub fn load(args: &[String]) -> Result<Option<Profile>, &'static str> {
    let markers: Vec<usize> = args
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| (arg == PROFILE_FLAG).then_some(index))
        .collect();
    if markers.is_empty() {
        return Ok(None);
    }
    if markers.len() != 1 {
        return Err("profile-flag-duplicate");
    }
    let Some(raw) = args.get(markers[0] + 1) else {
        return Err("profile-flag-value");
    };
    if raw.len() > PROFILE_MAX_BYTES {
        return Err("profile-oversize");
    }
    let document = strict_parse(raw).map_err(|_| "profile-invalid")?;
    validate(&document)
}

fn validate(candidate: &Value) -> Result<Option<Profile>, &'static str> {
    let invalid = |reason: &'static str| Err(reason);
    if !candidate.is_object() {
        return invalid("profile-invalid");
    }
    const ALLOWED: &[&str] = &[
        "id",
        "mode",
        "target",
        "readRoots",
        "exclusions",
        "targetResolution",
        "provenance",
        "localReference",
    ];
    for key in candidate.member_names() {
        if !ALLOWED.contains(&key) {
            return invalid("profile-invalid");
        }
    }
    let Some(id) = candidate.get("id").and_then(Value::as_str) else {
        return invalid("profile-invalid");
    };
    if !is_token(id) {
        return invalid("profile-invalid");
    }
    if candidate.get("mode").and_then(Value::as_str) != Some("observed") {
        return invalid("profile-invalid");
    }
    if candidate.get("target").and_then(Value::as_str) != Some(crate::TARGET) {
        return invalid("profile-invalid");
    }
    let Some(roots_value) = candidate.get("readRoots").and_then(Value::as_array) else {
        return invalid("profile-invalid");
    };
    if roots_value.is_empty() || roots_value.len() > 64 {
        return invalid("profile-invalid");
    }
    let mut roots: Vec<Root> = Vec::with_capacity(roots_value.len());
    for root in roots_value {
        let mut keys: Vec<&str> = root.member_names();
        keys.sort_unstable();
        if keys != ["kind", "path"] && keys != ["kind", "path", "scope"] {
            return invalid("profile-invalid");
        }
        let kind = match root.get("kind").and_then(Value::as_str) {
            Some("file") => RootKind::File,
            Some("tree") => RootKind::Tree,
            _ => return invalid("profile-invalid"),
        };
        let Some(spelling) = root.get("path").and_then(Value::as_str) else {
            return invalid("profile-invalid");
        };
        if path_violation(spelling).is_some() || !is_logical_path(spelling) {
            return invalid("profile-invalid");
        }
        if kind == RootKind::Tree && spelling.ends_with('/') {
            return invalid("profile-invalid");
        }
        let scope = if kind == RootKind::Tree {
            format!("{spelling}/**")
        } else {
            spelling.to_owned()
        };
        if roots
            .iter()
            .any(|other: &Root| other.kind == kind && other.path == spelling)
        {
            return invalid("profile-invalid");
        }
        for other in &roots {
            if prefix_of(&scope_segments(&scope), &scope_segments(&other.scope))
                || prefix_of(&scope_segments(&other.scope), &scope_segments(&scope))
            {
                return invalid("profile-invalid");
            }
        }
        roots.push(Root {
            kind,
            path: spelling.to_owned(),
            scope,
        });
    }
    let mut exclusions: Vec<String> = Vec::new();
    if let Some(exclusions_value) = candidate.get("exclusions") {
        let Some(items) = exclusions_value.as_array() else {
            return invalid("profile-invalid");
        };
        if items.len() > 64 {
            return invalid("profile-invalid");
        }
        for exclusion in items {
            let Some(spelling) = exclusion.as_str() else {
                return invalid("profile-invalid");
            };
            if !((is_scope(spelling) && spelling.ends_with("/**")) || is_logical_path(spelling)) {
                return invalid("profile-invalid");
            }
            exclusions.push(spelling.to_owned());
        }
    }
    let Some(provenance) = candidate.get("provenance") else {
        return invalid("profile-invalid");
    };
    {
        let mut keys: Vec<&str> = provenance.member_names();
        keys.sort_unstable();
        if keys != ["disposition", "origin", "revision"] {
            return invalid("profile-invalid");
        }
        let origin_ok = matches!(
            provenance.get("origin").and_then(Value::as_str),
            Some("declared" | "observed" | "inferred")
        );
        let revision_ok = provenance
            .get("revision")
            .and_then(Value::as_str)
            .is_some_and(|r| (1..=256).contains(&r.chars().count()));
        let disposition_ok = matches!(
            provenance.get("disposition").and_then(Value::as_str),
            Some("public-fixture" | "user-confirmed" | "unconfirmed")
        );
        if !(origin_ok && revision_ok && disposition_ok) {
            return invalid("profile-invalid");
        }
    }
    // The normalized profile is exactly what the scan custody digests:
    // declared members plus the derived `scope` on every root.
    let normalized = Value::Object(vec![
        ("id".to_owned(), Value::Str(id.to_owned())),
        ("mode".to_owned(), Value::Str("observed".to_owned())),
        (
            "target".to_owned(),
            Value::Str(crate::TARGET.to_owned()),
        ),
        (
            "readRoots".to_owned(),
            Value::Array(
                roots
                    .iter()
                    .map(|root| {
                        Value::Object(vec![
                            (
                                "kind".to_owned(),
                                Value::Str(
                                    match root.kind {
                                        RootKind::File => "file",
                                        RootKind::Tree => "tree",
                                    }
                                    .to_owned(),
                                ),
                            ),
                            ("path".to_owned(), Value::Str(root.path.clone())),
                            ("scope".to_owned(), Value::Str(root.scope.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "exclusions".to_owned(),
            Value::Array(exclusions.iter().cloned().map(Value::Str).collect()),
        ),
        ("provenance".to_owned(), provenance.clone()),
    ]);
    let digest = format!("sha256:{}", sha256_hex(&canonical(&normalized)));
    Ok(Some(Profile {
        id: id.to_owned(),
        roots,
        exclusions,
        digest,
    }))
}

fn scope_segments(scope: &str) -> Vec<&str> {
    scope.split('/').collect()
}

fn prefix_of(prefix: &[&str], candidate: &[&str]) -> bool {
    let prefix: &[&str] = if prefix.last() == Some(&"**") {
        &prefix[..prefix.len() - 1]
    } else {
        prefix
    };
    let candidate: &[&str] = if candidate.last() == Some(&"**") {
        &candidate[..candidate.len() - 1]
    } else {
        candidate
    };
    prefix.len() <= candidate.len() && prefix.iter().zip(candidate.iter()).all(|(a, b)| a == b)
}
