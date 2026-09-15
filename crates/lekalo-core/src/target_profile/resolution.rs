//! Deterministic profile resolution with explainable verdicts (issue #29).
//!
//! [`resolve`] consumes one decoded [`ProfileDocument`] and produces one
//! immutable [`ResolvedProfile`] per declaration. Two runs over the same
//! document and registry always produce byte-identical results: profiles
//! are processed in id order, per-axis provenance names the nearest
//! declaring profile, capability composition takes the weakest provided
//! state, every constraint verdict carries a sorted stable reason token,
//! and the digests are taken over canonical JSON.
//!
//! The fixed resolution order per profile is:
//!
//! 1. the inheritance chain (unknown base, cycle, or depth over the
//!    bound is refused before anything else);
//! 2. component lookup — every declared axis must resolve to a known
//!    registry definition (`target-profile.component-unknown`);
//! 3. capability composition — the union of the resolved components'
//!    provided capabilities, each at the weakest provided state, so a
//!    profile only ever claims what every contributing component
//!    actually guarantees;
//! 4. identity constraints — exact sibling requirements and conflicts
//!    (`combination-incompatible`, with all violations and reasons);
//! 5. capability requirements — every component requirement must be
//!    satisfied by the composed support (`capability-unsatisfied`);
//! 6. the inheritance guarantee check — the composed capability map may
//!    not weaken the base profile's map unless the profile carries an
//!    explicit override acknowledgment accepting exactly the weaker
//!    state (`inheritance-weakening`). Removing a base capability
//!    entirely is never overridable: a new base profile is the honest
//!    spelling.
//!
//! The resolved snapshot's digest domain is the canonical JSON of `{id,
//! version, components, capabilities}` — the effective semantics a run
//! binds to. Provenance and override evidence are deliberately outside
//! the digest: two resolutions with identical effective semantics are
//! the same snapshot, whatever declared route produced them.

use std::collections::BTreeMap;

use serde::Serialize;

use super::component::{self, Axis, Support};
use super::document::{ProfileDeclaration, ProfileDocument};
use super::version;
use super::{CapabilityGap, ProfileFailure};
use crate::lockfile::SemVer;
use crate::versioning::plan::sha256_hex;

/// One resolved axis: the component and the definition version its
/// semantics were read under.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedComponent {
    /// The axis the component fills.
    pub axis: Axis,
    /// The component id.
    pub id: String,
    /// The registry definition version of the component semantics.
    pub definition_version: &'static str,
}

/// One resolved capability of the profile, at the weakest provided state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedCapability {
    /// The stable dotted capability id.
    pub id: String,
    /// The support the whole profile guarantees.
    pub support: Support,
}

/// Where one resolved axis came from.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// The profile declared the component itself.
    Declared,
    /// The axis was inherited from the named base profile.
    Inherited { from: String },
}

/// The per-axis provenance evidence of one resolution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AxisProvenance {
    /// The axis.
    pub axis: Axis,
    /// Where the component came from.
    pub origin: Origin,
}

/// The immutable, machine-readable resolved profile: the closed value a
/// run binds to and the adapter protocol receives, never raw YAML.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedProfile {
    /// The profile identifier.
    pub id: String,
    /// The exact profile version.
    pub version: String,
    /// The resolved component per axis, byte-sorted by axis.
    pub components: Vec<ResolvedComponent>,
    /// The composed capability map, byte-sorted by id.
    pub capabilities: Vec<ResolvedCapability>,
    /// The canonical resolved snapshot digest (`sha256:…`). This is the
    /// lock's `profiles.digest` domain.
    pub digest: String,
    /// The canonical declared input digest (`sha256:…`). This is the
    /// lock's `profiles.source_digest` domain.
    pub source_digest: String,
    /// Per-axis provenance evidence, byte-sorted by axis.
    pub provenance: Vec<AxisProvenance>,
    /// The capability ids whose weaker resolution was explicitly
    /// accepted by an override, byte-sorted. Evidence, not semantics.
    pub overrides_applied: Vec<String>,
}

impl ResolvedProfile {
    /// The composed support of one capability, or `None` when the
    /// profile provides nothing for it.
    pub fn capability(&self, id: &str) -> Option<Support> {
        self.capabilities
            .iter()
            .find(|capability| capability.id == id)
            .map(|capability| capability.support)
    }

    /// The resolved component id of one axis.
    pub fn component(&self, axis: Axis) -> Option<&str> {
        self.components
            .iter()
            .find(|component| component.axis == axis)
            .map(|component| component.id.as_str())
    }
}

impl ResolvedProfile {
    /// Project the snapshot onto the adapter protocol's 1.2.0 resolved
    /// profile request members: the snapshot digest plus the capability
    /// pairs in canonical id order. This is the only path by which a
    /// profile reaches an adapter — never raw YAML.
    pub fn wire_resolution(&self) -> crate::target_protocol::wire::ProfileResolution {
        crate::target_protocol::wire::ProfileResolution {
            digest: self.digest.clone(),
            capabilities: self
                .capabilities
                .iter()
                .map(
                    |capability| crate::target_protocol::wire::ProfileCapability {
                        id: capability.id.clone(),
                        support: match capability.support {
                            Support::Full => crate::target_protocol::wire::SupportState::Full,
                            Support::Partial => crate::target_protocol::wire::SupportState::Partial,
                        },
                    },
                )
                .collect(),
        }
    }
}

/// Resolve every profile in the document.
///
/// Profiles are processed in id order; the first failing profile fails
/// the whole document fail-closed. A successful return contains exactly
/// one snapshot per declaration.
pub fn resolve(document: &ProfileDocument) -> Result<Vec<ResolvedProfile>, ProfileFailure> {
    let mut ordered: Vec<&ProfileDeclaration> = document.profiles.iter().collect();
    ordered.sort_by(|left, right| left.id.as_bytes().cmp(right.id.as_bytes()));
    let mut resolved = Vec::with_capacity(ordered.len());
    for declaration in ordered {
        resolved.push(resolve_one(document, declaration)?);
    }
    Ok(resolved)
}

/// The inheritance chain of one profile: `[self, parent, …]`. Unknown
/// bases, cycles, and over-deep chains are refused before any semantic
/// work so the verdict is always explainable.
fn chain<'a>(
    document: &'a ProfileDocument,
    declaration: &'a ProfileDeclaration,
) -> Result<Vec<&'a ProfileDeclaration>, ProfileFailure> {
    let by_id = |id: &str| -> Option<&'a ProfileDeclaration> {
        document.profiles.iter().find(|entry| entry.id == id)
    };
    let mut chain = vec![declaration];
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    seen.insert(declaration.id.as_str());
    let mut cursor = declaration;
    while let Some(base) = &cursor.extends {
        let Some(parent) = by_id(base) else {
            return Err(ProfileFailure::ReferenceInvalid {
                detail: "extends-unknown",
            });
        };
        if !seen.insert(parent.id.as_str()) {
            return Err(ProfileFailure::ReferenceInvalid {
                detail: "extends-cycle",
            });
        }
        if chain.len() > version::MAX_EXTEND_DEPTH {
            return Err(ProfileFailure::ReferenceInvalid {
                detail: "extends-depth",
            });
        }
        chain.push(parent);
        cursor = parent;
    }
    Ok(chain)
}

/// Compose the capability map of an effective component map: the union
/// of every provided capability at the weakest provided state.
fn compose(
    components: &BTreeMap<Axis, String>,
) -> Result<BTreeMap<String, Support>, ProfileFailure> {
    let mut capabilities: BTreeMap<String, Support> = BTreeMap::new();
    for axis in Axis::ALL {
        let Some(id) = components.get(&axis) else {
            continue;
        };
        let Some(definition) = component::definition(axis, id) else {
            return Err(ProfileFailure::ComponentUnknown {
                component: id.clone(),
                axis: axis.as_str(),
            });
        };
        for provide in definition.provides {
            let support = match capabilities.get(provide.id) {
                Some(existing) if *existing <= provide.support => *existing,
                _ => provide.support,
            };
            capabilities.insert(provide.id.to_owned(), support);
        }
    }
    Ok(capabilities)
}

/// The effective component map, provenance, and composed capabilities of
/// one inheritance chain.
fn effective(chain: &[&ProfileDeclaration]) -> Result<EffectiveView, ProfileFailure> {
    let mut components: BTreeMap<Axis, String> = BTreeMap::new();
    let mut provenance: BTreeMap<Axis, Origin> = BTreeMap::new();
    // Walk from the farthest ancestor to the profile itself: the nearest
    // declaration of an axis wins (explicit precedence).
    for declaration in chain.iter().rev() {
        for (axis, component) in &declaration.components {
            components.insert(*axis, component.clone());
            provenance.insert(
                *axis,
                if declaration.id == chain[0].id {
                    Origin::Declared
                } else {
                    Origin::Inherited {
                        from: declaration.id.clone(),
                    }
                },
            );
        }
    }
    let capabilities = compose(&components)?;
    let provenance = provenance
        .into_iter()
        .map(|(axis, origin)| AxisProvenance { axis, origin })
        .collect();
    Ok(EffectiveView {
        components,
        provenance,
        capabilities,
    })
}

/// The effective component map, provenance, and composed capabilities of
/// one inheritance chain, shared by parents and children.
struct EffectiveView {
    components: BTreeMap<Axis, String>,
    capabilities: BTreeMap<String, Support>,
    provenance: Vec<AxisProvenance>,
}

/// Resolve one profile through the fixed six-stage order.
fn resolve_one(
    document: &ProfileDocument,
    declaration: &ProfileDeclaration,
) -> Result<ResolvedProfile, ProfileFailure> {
    let chain = chain(document, declaration)?;
    let EffectiveView {
        components,
        provenance,
        capabilities,
    } = effective(&chain)?;

    // Stage 4: identity constraints — exact sibling requirements and
    // conflicts, every violation collected with a stable reason token.
    let mut reasons: Vec<String> = Vec::new();
    for axis in Axis::ALL {
        let Some(id) = components.get(&axis) else {
            continue;
        };
        let Some(definition) = component::definition(axis, id) else {
            return Err(ProfileFailure::ComponentUnknown {
                component: id.clone(),
                axis: axis.as_str(),
            });
        };
        for requirement in definition.requires_components {
            if components.get(&requirement.axis).map(String::as_str) != Some(requirement.component)
            {
                reasons.push(format!(
                    "requires-component:{}:{}:{}",
                    definition.id,
                    requirement.axis.as_str(),
                    requirement.component
                ));
            }
        }
        for conflict in definition.conflicts {
            if components.get(&conflict.axis).map(String::as_str) == Some(conflict.component) {
                reasons.push(format!(
                    "conflicts:{}:{}:{}",
                    definition.id,
                    conflict.axis.as_str(),
                    conflict.component
                ));
            }
        }
    }
    if !reasons.is_empty() {
        reasons.sort();
        reasons.dedup();
        return Err(ProfileFailure::CombinationIncompatible { reasons });
    }

    // Stage 5: capability requirements of every resolved component.
    let mut gaps: Vec<CapabilityGap> = Vec::new();
    for axis in Axis::ALL {
        let Some(id) = components.get(&axis) else {
            continue;
        };
        let Some(definition) = component::definition(axis, id) else {
            return Err(ProfileFailure::ComponentUnknown {
                component: id.clone(),
                axis: axis.as_str(),
            });
        };
        for requirement in definition.requires_capabilities {
            let actual = capabilities.get(requirement.id).copied();
            let satisfied =
                actual.is_some_and(|support| Support::satisfies(requirement.minimum, support));
            if !satisfied {
                gaps.push(CapabilityGap {
                    capability: requirement.id.to_owned(),
                    required: requirement.minimum,
                    actual,
                });
            }
        }
    }
    if !gaps.is_empty() {
        gaps.sort_by(|left, right| left.capability.as_bytes().cmp(right.capability.as_bytes()));
        return Err(ProfileFailure::CapabilityUnsatisfied { gaps });
    }

    // Stage 6: the inheritance guarantee check against the composed base
    // map. Weakening to a weaker provided state needs an explicit
    // override accepting exactly that state; removal is never accepted.
    let mut overrides_applied: Vec<String> = Vec::new();
    if chain.len() > 1 {
        let parent_chain: Vec<&ProfileDeclaration> = chain[1..].to_vec();
        let parent_capabilities = effective(&parent_chain)?.capabilities;
        let mut weakened: Vec<String> = Vec::new();
        for (capability, base_support) in &parent_capabilities {
            let child_support = capabilities.get(capability).copied();
            let weaker = child_support.map_or(true, |support| support < *base_support);
            if !weaker {
                continue;
            }
            let accepted = declaration
                .overrides
                .iter()
                .find(|override_item| override_item.capability == *capability);
            match (accepted, child_support) {
                (Some(override_item), Some(support)) if override_item.accept == support => {
                    overrides_applied.push(capability.clone());
                }
                _ => weakened.push(capability.clone()),
            }
        }
        for override_item in &declaration.overrides {
            if !parent_capabilities.contains_key(&override_item.capability) {
                return Err(ProfileFailure::DocumentInvalid {
                    detail: "override-unknown-capability",
                });
            }
        }
        if !weakened.is_empty() {
            weakened.sort();
            weakened.dedup();
            return Err(ProfileFailure::InheritanceWeakening {
                capabilities: weakened,
            });
        }
    }

    let resolved_components: Vec<ResolvedComponent> = Axis::ALL
        .iter()
        .filter_map(|axis| {
            components.get(axis).map(|id| ResolvedComponent {
                axis: *axis,
                id: id.clone(),
                definition_version: component::definition(*axis, id)
                    .expect("stage 2 proved the definition exists")
                    .definition_version,
            })
        })
        .collect();
    let resolved_capabilities: Vec<ResolvedCapability> = capabilities
        .iter()
        .map(|(id, support)| ResolvedCapability {
            id: id.clone(),
            support: *support,
        })
        .collect();
    let digest = snapshot_digest(
        &declaration.id,
        declaration.version.as_str(),
        &resolved_components,
        &resolved_capabilities,
    );
    Ok(ResolvedProfile {
        id: declaration.id.clone(),
        version: declaration.version.as_str().to_owned(),
        components: resolved_components,
        capabilities: resolved_capabilities,
        digest,
        source_digest: String::from("sha256:") + &sha256_hex(&declaration.canonical_bytes()),
        provenance,
        overrides_applied,
    })
}

/// The canonical resolved snapshot bytes: the lock's `profiles.digest`
/// domain. Canonical JSON over exactly `{id, version, components,
/// capabilities}`; evidence and provenance are outside the domain.
pub(crate) fn snapshot_digest(
    id: &str,
    version_spelling: &str,
    components: &[ResolvedComponent],
    capabilities: &[ResolvedCapability],
) -> String {
    let snapshot = serde_json::json!({
        "capabilities": capabilities.iter().map(|capability| {
            serde_json::json!({
                "id": capability.id,
                "support": capability.support.as_str(),
            })
        }).collect::<Vec<_>>(),
        "components": components.iter().map(|component| {
            serde_json::json!({
                "axis": component.axis.as_str(),
                "definition_version": component.definition_version,
                "id": component.id,
            })
        }).collect::<Vec<_>>(),
        "id": id,
        "version": version_spelling,
    });
    let bytes = serde_json::to_vec(&snapshot).expect("canonical snapshot serializes");
    String::from("sha256:") + &sha256_hex(&bytes)
}

/// Re-derive the resolved snapshot digest from a lock's stored profile
/// fields, so a verifier can recompute the `profiles.digest` domain from
/// the committed resolved snapshot bytes alone.
pub fn digest_of_resolved_bytes(resolved: &ResolvedProfile) -> String {
    snapshot_digest(
        &resolved.id,
        &resolved.version,
        &resolved.components,
        &resolved.capabilities,
    )
}

/// Parse one profile version spelling the same way the lock does.
pub fn parse_version(spelling: &str) -> Result<SemVer, ProfileFailure> {
    SemVer::parse(spelling).map_err(|_| ProfileFailure::DocumentInvalid { detail: "version" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target_profile::document::decode;

    const NODE: &str = r#"{
        "schema_version": "lekalo/target-profile/v1.0.0",
        "profiles": [{
            "id": "node-postgres-http",
            "version": "1.0.0",
            "components": {
                "runtime": "node-typescript",
                "storage": "postgres-sql",
                "transport": "http-json",
                "testing": "node-native",
                "analysis": "typescript-native",
                "deployment": "container"
            }
        }]
    }"#;

    #[test]
    fn resolves_the_node_profile_deterministically() {
        let document = decode(NODE.as_bytes()).expect("decodes");
        let first = resolve(&document).expect("resolves");
        let second = resolve(&document).expect("resolves");
        assert_eq!(first, second, "two runs are byte-identical");
        assert_eq!(first.len(), 1);
        let profile = &first[0];
        assert_eq!(profile.id, "node-postgres-http");
        assert_eq!(profile.component(Axis::Runtime), Some("node-typescript"));
        assert_eq!(profile.component(Axis::Storage), Some("postgres-sql"));
        // Runtime-bound axes resolved; storage and transport are the
        // reusable components the issue demands.
        for axis in Axis::ALL {
            assert!(
                matches!(
                    profile
                        .provenance
                        .iter()
                        .find(|p| p.axis == axis)
                        .map(|p| &p.origin),
                    Some(Origin::Declared)
                ),
                "every axis is declared"
            );
        }
        // The composed map carries the weakest provided state: node
        // runtime is fully async, http streaming is partial.
        assert_eq!(profile.capability("runtime.async"), Some(Support::Full));
        assert_eq!(
            profile.capability("transport.streaming"),
            Some(Support::Partial)
        );
        assert_eq!(
            profile.capability("storage.transactions"),
            Some(Support::Full)
        );
        assert!(profile.overrides_applied.is_empty());
        assert!(profile.digest.starts_with("sha256:"));
        assert!(profile.source_digest.starts_with("sha256:"));
    }

    #[test]
    fn reuses_storage_transport_deployment_across_runtimes() {
        let component_of = |runtime: &str, testing: &str, analysis: &str| {
            let text = format!(
                r#"{{
                    "schema_version": "lekalo/target-profile/v1.0.0",
                    "profiles": [{{
                        "id": "profile",
                        "version": "1.0.0",
                        "components": {{
                            "runtime": "{runtime}",
                            "storage": "postgres-sql",
                            "transport": "http-json",
                            "testing": "{testing}",
                            "analysis": "{analysis}",
                            "deployment": "container"
                        }}
                    }}]
                }}"#
            );
            let document = decode(text.as_bytes()).expect("decodes");
            resolve(&document).expect("resolves").remove(0)
        };
        let node = component_of("node-typescript", "node-native", "typescript-native");
        let php = component_of("php-laravel", "laratesto", "mago");
        let go = component_of("go-standard", "go-native", "go-vet");
        for axis in [Axis::Storage, Axis::Transport, Axis::Deployment] {
            assert_eq!(node.component(axis), php.component(axis));
            assert_eq!(php.component(axis), go.component(axis));
        }
        for axis in [Axis::Runtime, Axis::Testing, Axis::Analysis] {
            assert_ne!(node.component(axis), php.component(axis));
        }
        // The shared snapshots differ in runtime-bound semantics only.
        assert_ne!(node.digest, php.digest);
    }

    #[test]
    fn incompatible_combinations_block_with_sorted_reasons() {
        // grpc-proto needs runtime.async full; php-laravel provides only
        // partial, and the testing component still pins the runtime.
        let text = r#"{
            "schema_version": "lekalo/target-profile/v1.0.0",
            "profiles": [{
                "id": "laravel-grpc",
                "version": "1.0.0",
                "components": {
                    "runtime": "php-laravel",
                    "storage": "postgres-sql",
                    "transport": "grpc-proto",
                    "testing": "laratesto",
                    "analysis": "mago",
                    "deployment": "container"
                }
            }]
        }"#;
        let document = decode(text.as_bytes()).expect("decodes");
        let failure = resolve(&document).expect_err("capability gap blocks");
        assert_eq!(
            failure,
            ProfileFailure::CapabilityUnsatisfied {
                gaps: vec![CapabilityGap {
                    capability: "runtime.async".to_owned(),
                    required: Support::Full,
                    actual: Some(Support::Partial),
                }],
            }
        );
        // Identity constraints: serverless cannot host a file database
        // and needs the pooled storage guarantee.
        let text = r#"{
            "schema_version": "lekalo/target-profile/v1.0.0",
            "profiles": [{
                "id": "edge-sqlite",
                "version": "1.0.0",
                "components": {
                    "runtime": "node-typescript",
                    "storage": "sqlite-file",
                    "transport": "http-json",
                    "testing": "node-native",
                    "analysis": "typescript-native",
                    "deployment": "serverless"
                }
            }]
        }"#;
        let document = decode(text.as_bytes()).expect("decodes");
        let failure = resolve(&document).expect_err("conflict blocks");
        // Identity constraints run first: the serverless deployment
        // declares the file database incompatible, whatever capability
        // requirements would also fail.
        assert_eq!(
            failure,
            ProfileFailure::CombinationIncompatible {
                reasons: vec!["conflicts:serverless:storage:sqlite-file".to_owned()],
            }
        );
        // A wrong-ecosystem analysis under a Laravel testing framework
        // is refused by identity, not by capabilities.
        let text = r#"{
            "schema_version": "lekalo/target-profile/v1.0.0",
            "profiles": [{
                "id": "laravel-ts",
                "version": "1.0.0",
                "components": {
                    "runtime": "php-laravel",
                    "storage": "postgres-sql",
                    "transport": "http-json",
                    "testing": "laratesto",
                    "analysis": "typescript-native",
                    "deployment": "container"
                }
            }]
        }"#;
        let document = decode(text.as_bytes()).expect("decodes");
        let failure = resolve(&document).expect_err("ecosystem conflict blocks");
        assert_eq!(
            failure,
            ProfileFailure::CombinationIncompatible {
                reasons: vec![
                    "conflicts:laratesto:analysis:typescript-native".to_owned(),
                    "requires-component:typescript-native:runtime:node-typescript".to_owned(),
                ],
            }
        );
    }
    #[test]
    fn pooling_requirement_distinguishes_the_sql_storages() {
        // serverless requires full storage pooling: postgres satisfies
        // it, mysql only partially, so the same deployment component
        // accepts one storage and blocks the other with a reason.
        let resolve_with_storage = |storage: &str| {
            let text = format!(
                r#"{{
                    "schema_version": "lekalo/target-profile/v1.0.0",
                    "profiles": [{{
                        "id": "edge-sql",
                        "version": "1.0.0",
                        "components": {{
                            "runtime": "node-typescript",
                            "storage": "{storage}",
                            "transport": "http-json",
                            "testing": "node-native",
                            "analysis": "typescript-native",
                            "deployment": "serverless"
                        }}
                    }}]
                }}"#
            );
            let document = decode(text.as_bytes()).expect("decodes");
            resolve(&document)
        };
        let postgres = resolve_with_storage("postgres-sql").expect("postgres satisfies pooling");
        assert_eq!(
            postgres[0].capability("storage.pooling"),
            Some(Support::Full)
        );
        assert_eq!(
            resolve_with_storage("mysql-sql"),
            Err(ProfileFailure::CapabilityUnsatisfied {
                gaps: vec![CapabilityGap {
                    capability: "storage.pooling".to_owned(),
                    required: Support::Full,
                    actual: Some(Support::Partial),
                }],
            })
        );
    }

    #[test]
    fn unknown_components_and_bases_are_refused() {
        let text = r#"{
            "schema_version": "lekalo/target-profile/v1.0.0",
            "profiles": [{
                "id": "ghost",
                "version": "1.0.0",
                "components": {
                    "runtime": "deno-typescript",
                    "storage": "postgres-sql",
                    "transport": "http-json",
                    "testing": "node-native",
                    "analysis": "typescript-native",
                    "deployment": "container"
                }
            }]
        }"#;
        let document = decode(text.as_bytes()).expect("decodes");
        assert_eq!(
            resolve(&document),
            Err(ProfileFailure::ComponentUnknown {
                component: "deno-typescript".to_owned(),
                axis: "runtime",
            })
        );
        let text = r#"{
            "schema_version": "lekalo/target-profile/v1.0.0",
            "profiles": [{
                "id": "derived",
                "version": "1.0.0",
                "extends": "missing-base",
                "components": { "runtime": "node-typescript" }
            }]
        }"#;
        let document = decode(text.as_bytes()).expect("decodes");
        assert_eq!(
            resolve(&document),
            Err(ProfileFailure::ReferenceInvalid {
                detail: "extends-unknown",
            })
        );
    }

    #[test]
    fn inheritance_cycles_and_depth_are_refused() {
        let text = r#"{
            "schema_version": "lekalo/target-profile/v1.0.0",
            "profiles": [
                {
                    "id": "a",
                    "version": "1.0.0",
                    "extends": "b",
                    "components": { "runtime": "node-typescript" }
                },
                {
                    "id": "b",
                    "version": "1.0.0",
                    "extends": "a",
                    "components": { "runtime": "node-typescript" }
                }
            ]
        }"#;
        let document = decode(text.as_bytes()).expect("decodes");
        assert_eq!(
            resolve(&document),
            Err(ProfileFailure::ReferenceInvalid {
                detail: "extends-cycle",
            })
        );
    }

    #[test]
    fn deep_chains_inherit_with_explicit_precedence() {
        let text = r#"{
            "schema_version": "lekalo/target-profile/v1.0.0",
            "profiles": [
                {
                    "id": "base-web",
                    "version": "1.0.0",
                    "components": {
                        "runtime": "node-typescript",
                        "storage": "postgres-sql",
                        "transport": "http-json",
                        "testing": "node-native",
                        "analysis": "typescript-native",
                        "deployment": "container"
                    }
                },
                {
                    "id": "derived-serverless",
                    "version": "1.0.0",
                    "extends": "base-web",
                    "components": { "deployment": "serverless" },
                    "overrides": [
                        { "capability": "deployment.replicas", "accept": "partial" },
                        { "capability": "deployment.reproducible", "accept": "partial" }
                    ]
                },
                {
                    "id": "derived-serverless-observed",
                    "version": "1.0.0",
                    "extends": "derived-serverless",
                    "components": { "deployment": "container" }
                }
            ]
        }"#;
        let document = decode(text.as_bytes()).expect("decodes");
        let resolved = resolve(&document).expect("resolves");
        let derived = resolved
            .iter()
            .find(|profile| profile.id == "derived-serverless")
            .expect("derived resolves");
        assert_eq!(derived.component(Axis::Deployment), Some("serverless"));
        assert_eq!(derived.component(Axis::Runtime), Some("node-typescript"));
        let origin = |profile: &ResolvedProfile, axis: Axis| {
            profile
                .provenance
                .iter()
                .find(|entry| entry.axis == axis)
                .map(|entry| entry.origin.clone())
                .expect("provenance")
        };
        assert_eq!(origin(derived, Axis::Deployment), Origin::Declared);
        assert_eq!(
            origin(derived, Axis::Runtime),
            Origin::Inherited {
                from: "base-web".to_owned()
            }
        );
        assert_eq!(
            derived.overrides_applied,
            vec![
                "deployment.replicas".to_owned(),
                "deployment.reproducible".to_owned()
            ]
        );
        assert_eq!(
            derived.capability("deployment.replicas"),
            Some(Support::Partial)
        );
        // The farthest profile inherits through the middle one; the
        // nearest declaration wins for the overridden axis.
        let observed = resolved
            .iter()
            .find(|profile| profile.id == "derived-serverless-observed")
            .expect("observed resolves");
        assert_eq!(observed.component(Axis::Deployment), Some("container"));
        assert_eq!(
            origin(observed, Axis::Deployment),
            Origin::Declared,
            "the nearest declaration wins"
        );
        assert_eq!(
            origin(observed, Axis::Transport),
            Origin::Inherited {
                from: "base-web".to_owned()
            },
            "axes the middle profile never declared still inherit from the base"
        );
        assert!(observed.overrides_applied.is_empty());
        assert_eq!(
            observed.capability("deployment.replicas"),
            Some(Support::Full),
            "restoring the stronger component is never weakening"
        );
    }
    #[test]
    fn hidden_weakening_is_refused_and_explicit_override_is_recorded() {
        // The container deployment provides full guarantees; serverless
        // provides only partial ones. Inheriting it weakens both.
        let base = r#"
                {
                    "id": "base",
                    "version": "1.0.0",
                    "components": {
                        "runtime": "node-typescript",
                        "storage": "postgres-sql",
                        "transport": "http-json",
                        "testing": "node-native",
                        "analysis": "typescript-native",
                        "deployment": "container"
                    }
                }"#;
        let weakened = format!(
            r#"{{ "schema_version": "lekalo/target-profile/v1.0.0", "profiles": [{base},
                {{
                    "id": "weakened",
                    "version": "1.0.0",
                    "extends": "base",
                    "components": {{ "deployment": "serverless" }}
                }}] }}"#
        );
        let document = decode(weakened.as_bytes()).expect("decodes");
        let failure = resolve(&document).expect_err("hidden weakening blocks");
        assert_eq!(
            failure,
            ProfileFailure::InheritanceWeakening {
                capabilities: vec![
                    "deployment.replicas".to_owned(),
                    "deployment.reproducible".to_owned(),
                ],
            }
        );
        // An explicit override accepting exactly the weaker states turns
        // the same resolution legal, and the evidence names what was
        // accepted.
        let explicit = format!(
            r#"{{ "schema_version": "lekalo/target-profile/v1.0.0", "profiles": [{base},
                {{
                    "id": "weakened",
                    "version": "1.0.0",
                    "extends": "base",
                    "components": {{ "deployment": "serverless" }},
                    "overrides": [
                        {{ "capability": "deployment.replicas", "accept": "partial" }},
                        {{ "capability": "deployment.reproducible", "accept": "partial" }}
                    ]
                }}] }}"#
        );
        let document = decode(explicit.as_bytes()).expect("decodes");
        let resolved = resolve(&document).expect("explicit overrides resolve");
        let derived = resolved
            .iter()
            .find(|profile| profile.id == "weakened")
            .expect("derived resolves");
        assert_eq!(
            derived.overrides_applied,
            vec![
                "deployment.replicas".to_owned(),
                "deployment.reproducible".to_owned()
            ]
        );
        assert_eq!(
            derived.capability("deployment.replicas"),
            Some(Support::Partial)
        );
        // An override that accepts a weaker state can never accept the
        // outright removal of a base capability: a transport switch that
        // drops the base's rpc capability stays blocked even with an
        // override spelled.
        let grpc_base = r#"
                {
                    "id": "base",
                    "version": "1.0.0",
                    "components": {
                        "runtime": "node-typescript",
                        "storage": "postgres-sql",
                        "transport": "grpc-proto",
                        "testing": "node-native",
                        "analysis": "typescript-native",
                        "deployment": "container"
                    }
                }"#;
        let removal = format!(
            r#"{{ "schema_version": "lekalo/target-profile/v1.0.0", "profiles": [{grpc_base},
                {{
                    "id": "downgraded",
                    "version": "1.0.0",
                    "extends": "base",
                    "components": {{ "transport": "http-json" }},
                    "overrides": [
                        {{ "capability": "transport.rpc", "accept": "partial" }}
                    ]
                }}] }}"#
        );
        let document = decode(removal.as_bytes()).expect("decodes");
        let failure = resolve(&document).expect_err("removal stays blocked");
        assert_eq!(
            failure,
            ProfileFailure::InheritanceWeakening {
                capabilities: vec!["transport.rpc".to_owned(), "transport.streaming".to_owned(),],
            }
        );
    }

    #[test]
    fn removal_of_a_base_capability_is_never_overridable() {
        // Switching the runtime to Go removes the base's typing
        // capability entirely: no override token accepts an absent
        // capability, so inheritance refuses even a spelled override.
        // The analysis switch weakens types full->partial, which the
        // explicit override does accept.
        let text = r#"{
            "schema_version": "lekalo/target-profile/v1.0.0",
            "profiles": [
                {
                    "id": "base",
                    "version": "1.0.0",
                    "components": {
                        "runtime": "node-typescript",
                        "storage": "postgres-sql",
                        "transport": "http-json",
                        "testing": "node-native",
                        "analysis": "typescript-native",
                        "deployment": "container"
                    }
                },
                {
                    "id": "derived",
                    "version": "1.0.0",
                    "extends": "base",
                    "components": {
                        "runtime": "go-standard",
                        "testing": "go-native",
                        "analysis": "go-vet"
                    },
                    "overrides": [
                        { "capability": "analysis.types", "accept": "partial" },
                        { "capability": "runtime.typing", "accept": "partial" }
                    ]
                }
            ]
        }"#;
        let document = decode(text.as_bytes()).expect("decodes");
        let failure = resolve(&document).expect_err("removal blocks");
        assert_eq!(
            failure,
            ProfileFailure::InheritanceWeakening {
                capabilities: vec!["runtime.typing".to_owned()],
            }
        );
    }
    #[test]
    fn overrides_must_name_base_capabilities() {
        let text = r#"{
            "schema_version": "lekalo/target-profile/v1.0.0",
            "profiles": [
                {
                    "id": "base",
                    "version": "1.0.0",
                    "components": {
                        "runtime": "node-typescript",
                        "storage": "postgres-sql",
                        "transport": "http-json",
                        "testing": "node-native",
                        "analysis": "typescript-native",
                        "deployment": "container"
                    }
                },
                {
                    "id": "derived",
                    "version": "1.0.0",
                    "extends": "base",
                    "components": { "deployment": "container" },
                    "overrides": [
                        { "capability": "analysis.nonexistent", "accept": "partial" }
                    ]
                }
            ]
        }"#;
        let document = decode(text.as_bytes()).expect("decodes");
        assert_eq!(
            resolve(&document),
            Err(ProfileFailure::DocumentInvalid {
                detail: "override-unknown-capability",
            })
        );
    }

    #[test]
    fn multiple_profiles_resolve_independently() {
        let text = r#"{
            "schema_version": "lekalo/target-profile/v1.0.0",
            "profiles": [
                {
                    "id": "zeta-node",
                    "version": "1.0.0",
                    "components": {
                        "runtime": "node-typescript",
                        "storage": "postgres-sql",
                        "transport": "http-json",
                        "testing": "node-native",
                        "analysis": "typescript-native",
                        "deployment": "container"
                    }
                },
                {
                    "id": "alpha-laravel",
                    "version": "1.0.0",
                    "components": {
                        "runtime": "php-laravel",
                        "storage": "postgres-sql",
                        "transport": "http-json",
                        "testing": "laratesto",
                        "analysis": "mago",
                        "deployment": "container"
                    }
                }
            ]
        }"#;
        let document = decode(text.as_bytes()).expect("decodes");
        let resolved = resolve(&document).expect("both profiles resolve");
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].id, "alpha-laravel");
        assert_eq!(resolved[1].id, "zeta-node");
        assert_ne!(resolved[0].digest, resolved[1].digest);
    }

    #[test]
    fn digests_bind_effective_semantics_only() {
        let text = r#"{
            "schema_version": "lekalo/target-profile/v1.0.0",
            "profiles": [
                {
                    "id": "base",
                    "version": "1.0.0",
                    "components": {
                        "runtime": "node-typescript",
                        "storage": "postgres-sql",
                        "transport": "http-json",
                        "testing": "node-native",
                        "analysis": "typescript-native",
                        "deployment": "container"
                    }
                },
                {
                    "id": "same",
                    "version": "1.0.0",
                    "extends": "base",
                    "components": {
                        "runtime": "node-typescript",
                        "storage": "postgres-sql",
                        "transport": "http-json",
                        "testing": "node-native",
                        "analysis": "typescript-native",
                        "deployment": "container"
                    }
                }
            ]
        }"#;
        let document = decode(text.as_bytes()).expect("decodes");
        let resolved = resolve(&document).expect("resolves");
        let base = resolved.iter().find(|p| p.id == "base").expect("base");
        let same = resolved.iter().find(|p| p.id == "same").expect("same");
        // The snapshot domain is {id, version, components, capabilities}:
        // identical effective components under a different profile id
        // produce different snapshot digests, because a run binds to the
        // named profile, and different declared routes.
        assert_ne!(base.source_digest, same.source_digest);
        assert_eq!(digest_of_resolved_bytes(base), base.digest);
        let mut tampered = base.clone();
        tampered.capabilities.clear();
        assert_ne!(digest_of_resolved_bytes(&tampered), base.digest);
    }

    #[test]
    fn resolution_order_is_fixed_and_deterministic() {
        // A profile that violates two stages reports the earlier stage:
        // An unknown component (stage 2) beats a conflict (stage 4).
        let text = r#"{
            "schema_version": "lekalo/target-profile/v1.0.0",
            "profiles": [{
                "id": "messy",
                "version": "1.0.0",
                "components": {
                    "runtime": "ghost-runtime",
                    "storage": "postgres-sql",
                    "transport": "http-json",
                    "testing": "node-native",
                    "analysis": "typescript-native",
                    "deployment": "serverless"
                }
            }]
        }"#;
        let document = decode(text.as_bytes()).expect("decodes");
        assert_eq!(
            resolve(&document),
            Err(ProfileFailure::ComponentUnknown {
                component: "ghost-runtime".to_owned(),
                axis: "runtime",
            })
        );
    }
}
