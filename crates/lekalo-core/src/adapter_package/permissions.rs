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
use crate::target_protocol::scopes;

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
    let read_scopes = scopes(permissions, "readScopes");
    let write_scopes = scopes(permissions, "writeScopes");
    // Issue #89 (fix round 2, C-F12): the manifest schema's cap grammar
    // is looser than the runtime scope grammar, so a cap like `SRC/**`,
    // `a//b`, or `**` parses but can only ever match nothing. Refusing
    // the projection makes that explicit and diagnosable
    // (`ManifestInvalid`) instead of a silent empty match.
    for scope in read_scopes.iter().chain(write_scopes.iter()) {
        if !scopes::is_scope(scope) {
            return Err(format!(
                "permissions.filesystem scope `{scope}` violates the scope grammar"
            ));
        }
    }
    Ok(DeclaredPermissions {
        read_scopes,
        write_scopes,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_package::budget::SessionBudget;
    use crate::adapter_package::types::PackageFailure;

    fn manifest_with_caps(read: &[&str], write: &[&str]) -> ManifestDocument {
        let json = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "cap-adapter", "name": "C", "version": "1.0.0" },
            "publisher": { "id": "p", "trustAnchor": "none" },
            "source": { "kind": "path", "coordinate": "path:x", "digest": format!("sha256:{}", "11".repeat(32)) },
            "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": { "protocolVersions": [crate::target_protocol::version::VERSION], "irVersions": [crate::ir::version::VERSION], "extensions": [] },
            "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
            "executable": { "entry": "a.mjs" },
            "platforms": ["any"],
            "integrity": { "packageDigest": format!("sha256:{}", "22".repeat(32)), "files": [ { "path": "a.mjs", "digest": format!("sha256:{}", "33".repeat(32)), "bytes": 3 } ], "signaturePolicy": "unsigned", "signature": null },
            "permissions": {
                "filesystem": {
                    "readScopes": read.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                    "writeScopes": write.iter().map(|s| s.to_string()).collect::<Vec<_>>()
                },
                "network": { "mode": "denied", "destinations": [] },
                "environment": { "allowlist": [] },
                "processes": { "children": "denied" },
                "secrets": { "handles": [] }
            },
            "hooks": [],
            "conformance": { "reportDigest": format!("sha256:{}", "44".repeat(32)), "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "status": "active",
            "revocation": null
        });
        ManifestDocument::from_value(json)
            .expect("the manifest itself parses (its cap grammar is looser)")
    }

    /// Issue #89 (fix round 2, C-F12): a manifest cap that the schema
    /// admits but the runtime scope grammar rejects refuses the
    /// projection explicitly (`ManifestInvalid`), naming the scope.
    #[test]
    fn non_grammatical_caps_refuse_the_projection() {
        for scope in ["SRC/**", "src//**", "**", "a b/**", "src/**/x", "src/"] {
            let document = manifest_with_caps(&[scope], &[]);
            let error = declared_permissions(&document)
                .expect_err("a non-grammatical cap refuses the projection");
            assert!(
                error.contains(scope) && error.contains("scope grammar"),
                "the refusal names the cap: {scope} → {error}"
            );
            let document = manifest_with_caps(&[], &[scope]);
            assert!(
                declared_permissions(&document).is_err(),
                "write cap {scope}"
            );
        }
        // Grammatical caps still project, in both positions.
        let document = manifest_with_caps(&[".lekalo/ir/**", "src/**"], &["gen/**"]);
        let declared = declared_permissions(&document).expect("grammatical caps");
        assert_eq!(declared.read_scopes, [".lekalo/ir/**", "src/**"]);
        assert_eq!(declared.write_scopes, ["gen/**"]);
    }

    /// The refusal rides the budget projection as `ManifestInvalid`,
    /// so an ungrammatical cap fails the package gate, closed.
    #[test]
    fn an_ungrammatical_cap_fails_the_budget_projection() {
        let document = manifest_with_caps(&["SRC/**"], &[]);
        let error = SessionBudget::budget_for(Some(&document))
            .expect_err("the budget refuses the ungrammatical cap");
        assert!(
            matches!(error, PackageFailure::ManifestInvalid { .. }),
            "ManifestInvalid, got {error:?}"
        );
    }
}
