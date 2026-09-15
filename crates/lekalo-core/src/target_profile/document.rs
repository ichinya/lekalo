//! The closed target profile declaration document (issue #29).
//!
//! A profile document is a closed JSON artifact: a schema discriminator
//! and a unique, bounded set of profile declarations. One declaration
//! names its id and exact version, optionally one base profile it
//! extends, the component id per axis it selects (a complete map when no
//! base exists, a partial overriding map with one), and explicit override
//! acknowledgments. Unknown members, unknown axes, ungrammatical ids,
//! non-canonical versions, and cross-member nonsense are refused before
//! any resolution runs: decoding never guesses.
//!
//! The resolved snapshot and its digests are produced by
//! [`super::resolution`]; this module only owns the declared input shape
//! and the canonical declared bytes that feed `profiles.source_digest`
//! in the lock.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::component::{Axis, Support};
use super::{version, ProfileFailure};
use crate::lockfile::SemVer;

/// One explicit override acknowledgment: the profile accepts the named
/// capability resolving to exactly this support state, even when the base
/// profile provided a stronger one. Removing a base capability entirely is
/// never overridable; a weakened guarantee must be spelled out or
/// inheritance refuses it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverrideDeclaration {
    /// The stable dotted capability id.
    pub capability: String,
    /// The exact weaker state the profile explicitly accepts.
    pub accept: Support,
}

/// One declared profile inside a document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileDeclaration {
    /// The closed profile identifier.
    pub id: String,
    /// The exact profile version (`0.0.0` never exists on the wire).
    pub version: SemVer,
    /// The base profile this profile extends, when any.
    pub extends: Option<String>,
    /// The declared component per axis (partial exactly when extending).
    pub components: BTreeMap<Axis, String>,
    /// The explicit override acknowledgments, sorted and unique.
    pub overrides: Vec<OverrideDeclaration>,
}

impl ProfileDeclaration {
    /// The canonical declared bytes: compact JSON with sorted keys over
    /// the closed declaration members only. This is the
    /// `profiles.source_digest` domain of the lock.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut declaration = serde_json::Map::new();
        declaration.insert(
            "components".to_owned(),
            serde_json::Value::Object(
                self.components
                    .iter()
                    .map(|(axis, component)| {
                        (
                            axis.as_str().to_owned(),
                            serde_json::Value::String(component.clone()),
                        )
                    })
                    .collect(),
            ),
        );
        if let Some(extends) = &self.extends {
            declaration.insert(
                "extends".to_owned(),
                serde_json::Value::String(extends.clone()),
            );
        }
        declaration.insert("id".to_owned(), serde_json::Value::String(self.id.clone()));
        let overrides = self
            .overrides
            .iter()
            .map(|override_item| {
                serde_json::Value::Object(serde_json::Map::from_iter([
                    (
                        "accept".to_owned(),
                        serde_json::Value::String(override_item.accept.as_str().to_owned()),
                    ),
                    (
                        "capability".to_owned(),
                        serde_json::Value::String(override_item.capability.clone()),
                    ),
                ]))
            })
            .collect();
        declaration.insert("overrides".to_owned(), serde_json::Value::Array(overrides));
        declaration.insert(
            "version".to_owned(),
            serde_json::Value::String(self.version.as_str().to_owned()),
        );
        serde_json::to_vec(&serde_json::Value::Object(declaration))
            .expect("canonical declaration serializes")
    }
}

/// One decoded profile document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileDocument {
    /// The declared profiles, in document order; resolution applies its
    /// own deterministic order.
    pub profiles: Vec<ProfileDeclaration>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireDocument {
    schema_version: String,
    profiles: Vec<WireProfile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireProfile {
    id: String,
    version: String,
    #[serde(default)]
    extends: Option<String>,
    #[serde(default)]
    components: BTreeMap<String, String>,
    #[serde(default)]
    overrides: Vec<WireOverride>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireOverride {
    capability: String,
    accept: String,
}

/// Decode and validate the closed profile document contract.
pub fn decode(bytes: &[u8]) -> Result<ProfileDocument, ProfileFailure> {
    let invalid = |detail| Err(ProfileFailure::DocumentInvalid { detail });
    let wire: WireDocument = serde_json::from_slice(bytes)
        .map_err(|_| ProfileFailure::DocumentInvalid { detail: "shape" })?;
    if wire.schema_version != version::SCHEMA_VERSION {
        return invalid("schema-version");
    }
    if wire.profiles.is_empty() {
        return invalid("profiles-empty");
    }
    if wire.profiles.len() > version::MAX_PROFILES {
        return invalid("profiles-limit");
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut profiles = Vec::with_capacity(wire.profiles.len());
    for wire_profile in wire.profiles {
        if !version::is_profile_id(&wire_profile.id) {
            return invalid("id");
        }
        if !seen.insert(wire_profile.id.clone()) {
            return invalid("duplicate-id");
        }
        if wire_profile.version == "0.0.0" {
            return invalid("version");
        }
        let version = SemVer::parse(&wire_profile.version)
            .map_err(|_| ProfileFailure::DocumentInvalid { detail: "version" })?;
        if let Some(extends) = &wire_profile.extends {
            if !version::is_profile_id(extends) || *extends == wire_profile.id {
                return invalid("extends");
            }
        }
        if wire_profile.components.is_empty() {
            return invalid("components-empty");
        }
        if wire_profile.components.len() > Axis::ALL.len() {
            return invalid("components-limit");
        }
        let extends_any = wire_profile.extends.is_some();
        let mut components = BTreeMap::new();
        for (axis, component) in wire_profile.components {
            let Some(axis) = Axis::parse(&axis) else {
                return invalid("axis");
            };
            if !version::is_profile_id(&component) {
                return invalid("component");
            }
            if components.insert(axis, component).is_some() {
                return invalid("axis");
            }
        }
        if !extends_any && components.len() != Axis::ALL.len() {
            return invalid("components-incomplete");
        }
        if wire_profile.overrides.len() > version::MAX_OVERRIDES {
            return invalid("overrides-limit");
        }
        if !extends_any && !wire_profile.overrides.is_empty() {
            return invalid("override-without-extends");
        }
        let mut overrides = Vec::with_capacity(wire_profile.overrides.len());
        let mut override_ids = std::collections::BTreeSet::new();
        for wire_override in wire_profile.overrides {
            if !crate::target_protocol::wire::is_capability_id(&wire_override.capability) {
                return invalid("override-capability");
            }
            if !override_ids.insert(wire_override.capability.clone()) {
                return invalid("override-duplicate");
            }
            let Some(accept) = Support::parse(&wire_override.accept) else {
                return invalid("override-accept");
            };
            overrides.push(OverrideDeclaration {
                capability: wire_override.capability,
                accept,
            });
        }
        // Canonical declared bytes never depend on document order.
        overrides
            .sort_by(|left, right| left.capability.as_bytes().cmp(right.capability.as_bytes()));
        profiles.push(ProfileDeclaration {
            id: wire_profile.id,
            version,
            extends: wire_profile.extends,
            components,
            overrides,
        });
    }
    Ok(ProfileDocument { profiles })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(profiles: serde_json::Value) -> Vec<u8> {
        serde_json::json!({ "schema_version": version::SCHEMA_VERSION, "profiles": profiles })
            .to_string()
            .into_bytes()
    }

    fn complete_components() -> serde_json::Value {
        serde_json::json!({
            "runtime": "node-typescript",
            "storage": "postgres-sql",
            "transport": "http-json",
            "testing": "node-native",
            "analysis": "typescript-native",
            "deployment": "container"
        })
    }

    #[test]
    fn decodes_a_complete_profile() {
        let bytes = document(serde_json::json!([{
            "id": "node-postgres-http",
            "version": "1.0.0",
            "components": complete_components(),
        }]));
        let document = decode(&bytes).expect("decodes");
        assert_eq!(document.profiles.len(), 1);
        let profile = &document.profiles[0];
        assert_eq!(profile.id, "node-postgres-http");
        assert_eq!(profile.version.as_str(), "1.0.0");
        assert_eq!(profile.extends, None);
        assert_eq!(profile.components.len(), 6);
        assert!(profile.overrides.is_empty());
    }

    #[test]
    fn canonical_declaration_bytes_are_key_sorted_and_stable() {
        let extending = serde_json::json!({
            "id": "derived",
            "version": "1.1.0",
            "extends": "base",
            "components": { "transport": "grpc-proto" },
            "overrides": [
                { "capability": "transport.streaming", "accept": "partial" }
            ]
        });
        let bytes = document(serde_json::json!([extending]));
        let profile = &decode(&bytes).expect("decodes").profiles[0];
        assert_eq!(
            String::from_utf8(profile.canonical_bytes()).expect("utf8"),
            "{\"components\":{\"transport\":\"grpc-proto\"},\"extends\":\"base\",\
             \"id\":\"derived\",\"overrides\":[{\"accept\":\"partial\",\
             \"capability\":\"transport.streaming\"}],\"version\":\"1.1.0\"}"
        );
    }

    #[test]
    fn refuses_closed_shape_violations() {
        let invalid_cases: Vec<serde_json::Value> = vec![
            serde_json::json!({ "unknown": true }),
            serde_json::json!([1]),
            serde_json::json!({ "id": "a", "components": complete_components() }),
        ];
        for case in invalid_cases {
            assert!(decode(&document(serde_json::json!([case]))).is_err());
        }
        assert!(
            decode(b"{\"schema_version\":\"other\",\"profiles\":[]}").is_err(),
            "schema discriminator mismatch is refused"
        );
        for detail in [
            "profiles-empty",
            "id",
            "duplicate-id",
            "version",
            "extends",
            "components-empty",
            "components-incomplete",
            "axis",
            "component",
            "override-without-extends",
            "override-duplicate",
            "override-accept",
            "override-capability",
        ] {
            let failure = decode(&document(refusal_case(detail)))
                .expect_err("closed refusal detail must fire");
            assert!(
                matches!(failure, ProfileFailure::DocumentInvalid { .. }),
                "detail {detail}"
            );
        }
    }

    fn refusal_case(detail: &str) -> serde_json::Value {
        match detail {
            "profiles-empty" => serde_json::json!([]),
            "id" => {
                serde_json::json!({ "id": "-bad", "version": "1.0.0", "components": complete_components() })
            }
            "duplicate-id" => serde_json::json!([
                { "id": "a", "version": "1.0.0", "components": complete_components() },
                { "id": "a", "version": "1.0.1", "components": complete_components() }
            ]),
            "version" => {
                serde_json::json!({ "id": "a", "version": "0.0.0", "components": complete_components() })
            }
            "extends" => {
                serde_json::json!({ "id": "a", "version": "1.0.0", "extends": "a", "components": complete_components() })
            }
            "components-empty" => {
                serde_json::json!({ "id": "a", "version": "1.0.0", "extends": "b", "components": {} })
            }
            "components-incomplete" => {
                serde_json::json!({ "id": "a", "version": "1.0.0", "components": { "runtime": "node-typescript" } })
            }
            "axis" => {
                serde_json::json!({ "id": "a", "version": "1.0.0", "components": { "middle": "x" } })
            }
            "component" => {
                serde_json::json!({ "id": "a", "version": "1.0.0", "extends": "b", "components": { "runtime": "Bad/Id" } })
            }
            "override-without-extends" => {
                serde_json::json!({ "id": "a", "version": "1.0.0", "components": complete_components(), "overrides": [
                { "capability": "x.y", "accept": "partial" }
            ] })
            }
            "override-duplicate" => {
                serde_json::json!({ "id": "a", "version": "1.0.0", "extends": "b", "components": { "runtime": "node-typescript" }, "overrides": [
                { "capability": "x.y", "accept": "partial" },
                { "capability": "x.y", "accept": "full" }
            ] })
            }
            "override-accept" => {
                serde_json::json!({ "id": "a", "version": "1.0.0", "extends": "b", "components": { "runtime": "node-typescript" }, "overrides": [
                { "capability": "x.y", "accept": "unknown" }
            ] })
            }
            "override-capability" => {
                serde_json::json!({ "id": "a", "version": "1.0.0", "extends": "b", "components": { "runtime": "node-typescript" }, "overrides": [
                { "capability": "X.Y", "accept": "partial" }
            ] })
            }
            _ => serde_json::json!([]),
        }
    }
}
