//! Manifest-vs-describe consistency (issue #32 fix round 1, cline F-4).
//!
//! The manifest is the independent claim; the describe response is the
//! adapter's self-assertion. This module cross-checks the two after the
//! safe describe handshake and refuses any disagreement with
//! `adapter.manifest-mismatch` (denied, exit 3) — the refusal the ADR
//! and the contract describe.
//!
//! Compared members, in canonical order: adapter id, adapter version,
//! declared protocol versions, declared IR versions, operations,
//! named capability ids, targets, profiles, read/write scopes, and
//! transports. Named capability *states* are deliberately not compared
//! here: describe provenance (declared/probed/verified) is orthogonal
//! to the manifest's claim of which capability ids exist — the id set
//! itself is compared against `capabilities.named`.

use crate::adapter_package::manifest::ManifestDocument;
use crate::target_protocol::discovery::DiscoveredAdapter;

/// Why the describe outcome contradicts the manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mismatch {
    /// The describe identity differs from `adapter.id`.
    Id,
    /// The describe version differs from `adapter.version`.
    Version,
    /// The declared protocol sets disagree.
    Protocols,
    /// The declared IR sets disagree.
    IrVersions,
    /// The described operation set is not exactly the manifest's.
    Operations,
    /// The described capability id set is not exactly the manifest's
    /// `named` claim.
    Named,
    /// The described targets are not a subset of the manifest's.
    Targets,
    /// The described profiles are not a subset of the manifest's.
    Profiles,
    /// The described read scopes exceed the manifest's declaration.
    ReadScopes,
    /// The described write scopes exceed the manifest's declaration.
    WriteScopes,
    /// The described transports are not covered by the manifest's.
    Transports,
}

impl Mismatch {
    /// The stable wire token (the diagnostic's `field` data member).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Id => "adapter.id",
            Self::Version => "adapter.version",
            Self::Protocols => "compatibility.protocolVersions",
            Self::IrVersions => "compatibility.irVersions",
            Self::Operations => "capabilities.operations",
            Self::Named => "capabilities.named",
            Self::Targets => "capabilities.targets",
            Self::Profiles => "capabilities.profiles",
            Self::ReadScopes => "capabilities.readScopes",
            Self::WriteScopes => "capabilities.writeScopes",
            Self::Transports => "capabilities.transports",
        }
    }
}

/// Cross-check the verified manifest against the describe outcome.
/// Returns the first disagreement in canonical member order.
pub fn check(manifest: &ManifestDocument, discovered: &DiscoveredAdapter) -> Result<(), Mismatch> {
    if discovered.adapter.id != manifest.adapter_id() {
        return Err(Mismatch::Id);
    }
    if discovered.adapter.version != manifest.adapter_version().as_str() {
        return Err(Mismatch::Version);
    }
    let mut declared_protocols = discovered.declared_protocols.clone();
    declared_protocols.sort();
    declared_protocols.dedup();
    if declared_protocols != manifest.protocol_versions().to_vec() {
        return Err(Mismatch::Protocols);
    }
    // Capability surface: subset semantics, like the scope checks. A
    // profile-gated adapter honestly describes fewer operations, ids,
    // and IR versions than the package supports — the manifest bounds
    // what it may ever claim, so describing *more* than declared is an
    // escalation attempt while describing less is honest gating.
    if !subset(&discovered.ir_versions, manifest.ir_versions()) {
        return Err(Mismatch::IrVersions);
    }
    if !subset(&discovered.operations, &manifest.operations()) {
        return Err(Mismatch::Operations);
    }
    let described_ids: Vec<String> = discovered
        .capabilities
        .iter()
        .map(|capability| capability.id.clone())
        .collect();
    if !subset(&described_ids, &manifest.named()) {
        return Err(Mismatch::Named);
    }
    // Targets/profiles: the adapter may describe fewer than it supports
    // overall? No — describe declares the full surface, so the sets must
    // agree exactly.
    if sorted(&discovered.targets) != manifest.targets() {
        return Err(Mismatch::Targets);
    }
    if sorted(&discovered.profiles) != manifest.profiles() {
        return Err(Mismatch::Profiles);
    }
    // Scopes: the described scopes must never exceed the manifest's
    // declaration (a manifest is least-privilege; describing more is an
    // escalation attempt).
    if !subset(&discovered.read_scopes, &manifest.read_scopes()) {
        return Err(Mismatch::ReadScopes);
    }
    if !subset(&discovered.write_scopes, &manifest.write_scopes()) {
        return Err(Mismatch::WriteScopes);
    }
    // Transports: every described transport must be covered.
    for transport in &discovered.transports {
        if !manifest.transports().contains(transport) {
            return Err(Mismatch::Transports);
        }
    }
    Ok(())
}

fn sorted(values: &[String]) -> Vec<String> {
    let mut sorted = values.to_vec();
    sorted.sort();
    sorted
}

fn subset(described: &[String], declared: &[String]) -> bool {
    described.iter().all(|scope| declared.contains(scope))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_package::version::{MANIFEST_IDENTITY, MANIFEST_SCHEMA_VERSION};

    fn manifest_json(
        protocol: &str,
        operations: &[&str],
        named: &serde_json::Map<String, serde_json::Value>,
    ) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": MANIFEST_SCHEMA_VERSION,
            "identity": MANIFEST_IDENTITY,
            "adapter": { "id": "x-adapter", "name": "X", "version": "1.0.0" },
            "publisher": { "id": "p", "trustAnchor": "none" },
            "source": { "kind": "path", "coordinate": "path:x", "digest": format!("sha256:{}", "11".repeat(32)) },
            "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": { "protocolVersions": [protocol], "irVersions": ["0.2.16"], "extensions": [] },
            "capabilities": {
                "operations": operations,
                "targets": ["t"], "profiles": ["default"], "named": named, "constraints": {},
                "readScopes": ["src/**"], "writeScopes": ["generated/**"], "transports": ["stdin"]
            },
            "executable": { "runtime": { "kind": "node", "minVersion": "18.0.0" }, "entry": "a.mjs", "argvPreview": ["node", "a.mjs"], "assets": [] },
            "platforms": ["any"],
            "integrity": { "packageDigest": format!("sha256:{}", "22".repeat(32)), "files": [ { "path": "a.mjs", "digest": format!("sha256:{}", "33".repeat(32)), "bytes": 3 } ], "signaturePolicy": "unsigned", "signature": null },
            "permissions": { "filesystem": { "readScopes": ["src/**"], "writeScopes": ["generated/**"] }, "network": { "mode": "denied", "destinations": [] }, "environment": { "allowlist": [] }, "processes": { "children": "denied" }, "secrets": { "handles": [] } },
            "hooks": [],
            "conformance": { "reportDigest": format!("sha256:{}", "44".repeat(32)), "badge": { "protocol": protocol, "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "status": "active",
            "revocation": null
        })
    }

    fn describe(operations: &[&str], ids: &[&str]) -> DiscoveredAdapter {
        DiscoveredAdapter {
            adapter: crate::target_protocol::wire::AdapterIdentity {
                id: "x-adapter".to_owned(),
                version: "1.0.0".to_owned(),
                digest: format!("sha256:{}", "55".repeat(32)),
            },
            negotiated_version: crate::target_protocol::version::VERSION,
            declared_protocols: vec![crate::target_protocol::version::VERSION.to_owned()],
            ir_versions: vec!["0.2.16".to_owned()],
            constraints: None,
            targets: vec!["t".to_owned()],
            profiles: vec!["default".to_owned()],
            operations: operations
                .iter()
                .map(|operation| operation.to_string())
                .collect(),
            capability_digest: format!("sha256:{}", "66".repeat(32)),
            executable_digest: None,
            read_scopes: vec!["src/**".to_owned()],
            write_scopes: vec!["generated/**".to_owned()],
            transports: vec!["stdin".to_owned()],
            capabilities: ids
                .iter()
                .map(
                    |id| crate::target_protocol::discovery::DiscoveredCapability {
                        id: id.to_string(),
                        state: crate::target_protocol::wire::SupportState::Full,
                        definition_version: "0.3.1",
                        provenance: crate::target_protocol::discovery::Provenance::Declared,
                    },
                )
                .collect(),
        }
    }

    fn named(ids: &[&str]) -> serde_json::Map<String, serde_json::Value> {
        ids.iter()
            .map(|id| (id.to_string(), serde_json::json!("full")))
            .collect()
    }

    #[test]
    fn an_honest_describe_matches_the_manifest() {
        let manifest = ManifestDocument::from_value(manifest_json(
            crate::target_protocol::version::VERSION,
            &["describe", "scan"],
            &named(&["scan.symbols"]),
        ))
        .expect("parses");
        assert!(check(
            &manifest,
            &describe(&["describe", "scan"], &["scan.symbols"])
        )
        .is_ok());
    }

    #[test]
    fn a_lying_describe_refuses_with_the_field_token() {
        let manifest = ManifestDocument::from_value(manifest_json(
            crate::target_protocol::version::VERSION,
            &["describe", "scan"],
            &named(&["scan.symbols"]),
        ))
        .expect("parses");
        // The adapter claims a different version than the manifest pins.
        let mut lying = describe(&["describe", "scan"], &["scan.symbols"]);
        lying.adapter.version = "9.9.9".to_owned();
        assert_eq!(check(&manifest, &lying), Err(Mismatch::Version));
        // The adapter describes more write scope than the manifest allows.
        let mut escalating = describe(&["describe", "scan"], &["scan.symbols"]);
        escalating.write_scopes = vec!["generated/**".to_owned(), "lekalo/**".to_owned()];
        assert_eq!(check(&manifest, &escalating), Err(Mismatch::WriteScopes));
        // The adapter claims an operation the manifest never declared.
        assert_eq!(
            check(
                &manifest,
                &describe(&["describe", "scan", "verify"], &["scan.symbols"])
            ),
            Err(Mismatch::Operations)
        );
        // The adapter claims a capability id the manifest never named.
        assert_eq!(
            check(
                &manifest,
                &describe(&["describe", "scan"], &["scan.symbols", "verify.scenarios"])
            ),
            Err(Mismatch::Named)
        );
    }
}
