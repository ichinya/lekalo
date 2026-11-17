//! TypeScript/Node project metadata inspection; no package scripts execute.
#![forbid(unsafe_code)]
use serde_json::Value;

pub const MANIFESTS: &[(&str, &str)] = &[("package.json", "node")];
pub const MODULE_MANIFEST: &str = "package.json";
pub const WORKSPACE_MANIFEST: &str = "pnpm-workspace.yaml";
pub const WORKSPACE_MEMBERS_FIELD: &str = "packages";
pub const MANAGERS: &[&str] = &["npm", "yarn", "pnpm", "bun"];
pub const LOCKFILES: &[(&str, &str)] = &[
    ("package-lock.json", "npm"),
    ("npm-shrinkwrap.json", "npm"),
    ("yarn.lock", "yarn"),
    ("pnpm-lock.yaml", "pnpm"),
    ("pnpm-workspace.yaml", "pnpm"),
    ("bun.lockb", "bun"),
    ("bun.lock", "bun"),
];
pub const FILE_HINTS: &[(&str, &str, &str)] = &[("tsconfig.json", "typescript", "medium")];
pub const MONOREPO_TOOLS: &[&str] = &["lerna.json", "turbo.json", "nx.json"];

const NODE_HINTS: [(&str, &str, &str); 16] = [
    ("typescript", "typescript", "language"),
    ("react", "react", "framework"),
    ("vue", "vue", "framework"),
    ("svelte", "svelte", "framework"),
    ("@angular/core", "angular", "framework"),
    ("next", "next", "framework"),
    ("nuxt", "nuxt", "framework"),
    ("express", "express", "framework"),
    ("fastify", "fastify", "framework"),
    ("@nestjs/core", "nestjs", "framework"),
    ("vite", "vite", "framework"),
    ("webpack", "webpack", "framework"),
    ("jest", "jest", "framework"),
    ("vitest", "vitest", "framework"),
    ("@playwright/test", "playwright", "framework"),
    ("cypress", "cypress", "framework"),
];

pub fn node_workspace_members(value: &Value) -> Option<Vec<String>> {
    let field = value.get("workspaces")?;
    let items = match field {
        Value::Array(items) => items.as_slice(),
        Value::Object(map) => map.get("packages")?.as_array()?.as_slice(),
        _ => return None,
    };
    Some(
        items
            .iter()
            .filter_map(Value::as_str)
            .filter(|glob| !glob.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

pub fn node_scripts(value: &Value) -> Vec<String> {
    value
        .get("scripts")
        .and_then(Value::as_object)
        .map(|scripts| scripts.keys().cloned().collect())
        .unwrap_or_default()
}

/// Dependency hints retain field provenance and canonical dependency order.
pub fn dependency_hints(value: &Value) -> Vec<(&'static str, &'static str, &'static str)> {
    let mut hints = Vec::new();
    for field in ["dependencies", "devDependencies", "peerDependencies"] {
        let Some(map) = value.get(field).and_then(Value::as_object) else {
            continue;
        };
        for dependency in map.keys() {
            if let Some((_, label, kind)) =
                NODE_HINTS.iter().find(|(name, _, _)| *name == dependency)
            {
                hints.push((*label, *kind, field));
            }
        }
    }
    hints
}
