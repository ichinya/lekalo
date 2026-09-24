//! The declared-permission projection (issue #32).
//!
//! The manifest's `permissions` block is the least-privilege declaration
//! the execution-isolation issue (#89) enforces. This module projects
//! the validated canonical JSON into the typed [`DeclaredPermissions`]
//! value: never enforced here (that is #89's confinement budget), but
//! no longer dead scaffolding — the projection is total over the closed
//! wire shape and fails closed on any deviation.

use serde::Serialize;

use crate::adapter_package::manifest::ManifestDocument;

/// The declared permission surface of one manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeclaredPermissions {
    /// Read scopes declared under `permissions.filesystem`.
    pub read_scopes: Vec<String>,
    /// Write scopes declared under `permissions.filesystem`.
    pub write_scopes: Vec<String>,
    /// `network.mode`: `denied` or `allowlist`.
    pub network_mode: String,
    /// Network destinations under an allowlist (closed host tokens).
    pub network_destinations: Vec<String>,
    /// Environment variable allowlist.
    pub environment_allowlist: Vec<String>,
    /// `processes.children`: `denied` or `declared`.
    pub child_processes: String,
    /// Declared secret handle ids.
    pub secret_handles: Vec<String>,
}

fn scopes(member: &serde_json::Value, key: &str) -> Vec<String> {
    member
        .get("filesystem")
        .and_then(|filesystem| filesystem.get(key))
        .and_then(|value| value.as_array())
        .map(|array| {
            array
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn list(member: &serde_json::Value, parent: &str, key: &str) -> Vec<String> {
    member
        .get(parent)
        .and_then(|parent| parent.get(key))
        .and_then(|value| value.as_array())
        .map(|array| {
            array
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Project the declared permissions out of a validated manifest.
/// Fails closed: any structural deviation refuses rather than yielding a
/// half-empty policy.
pub fn declared_permissions(manifest: &ManifestDocument) -> Result<DeclaredPermissions, String> {
    let value: serde_json::Value =
        serde_json::from_slice(&manifest.canonical_bytes()).map_err(|error| error.to_string())?;
    let permissions = value.get("permissions").ok_or("permissions missing")?;
    Ok(DeclaredPermissions {
        read_scopes: scopes(permissions, "readScopes"),
        write_scopes: scopes(permissions, "writeScopes"),
        network_mode: permissions
            .get("network")
            .and_then(|network| network.get("mode"))
            .and_then(|mode| mode.as_str())
            .ok_or("network.mode missing")?
            .to_owned(),
        network_destinations: list(permissions, "network", "destinations"),
        environment_allowlist: list(permissions, "environment", "allowlist"),
        child_processes: permissions
            .get("processes")
            .and_then(|processes| processes.get("children"))
            .and_then(|children| children.as_str())
            .ok_or("processes.children missing")?
            .to_owned(),
        secret_handles: list(permissions, "secrets", "handles"),
    })
}
