//! The fragment output model and the generated-sidecar ownership
//! manifest (issue #46, plan §4.1–§4.2).
//!
//! Fragment mode renders the same document [`crate::openapi::render`]
//! produces, then splits it into per-JSON-pointer fragments: one
//! per-endpoint path-item operation, one per reusable schema, response,
//! and security scheme. The merge unit is the pointer. Ownership is
//! two-layered: the artifact manifest owns files (the #45/#70
//! convention), and the `lekalo/openapi-map/v0.4.0` sidecar manifest
//! owns pointers — the generated-sidecar precedent of the zod-map
//! contract. The manifest is an adapter-owned micro-contract that
//! travels as an ordinary generated write; it is pure data, sorted by
//! pointer bytes.

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json};

use super::diagnostic;
use super::id::{escape_pointer, paths_pointer};
use super::version::{GENERATOR_ID, GENERATOR_VERSION, OWNERSHIP_CONTRACT};
use super::OpenApiDocument;

/// The pointer-level fragments of one rendered document: every
/// mergeable unit, byte-sorted by pointer.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Fragments {
    pointers: BTreeMap<String, Json>,
}

impl Fragments {
    /// Assemble from a pointer map (crate internal and tests).
    pub(crate) fn from_map(pointers: BTreeMap<String, Json>) -> Self {
        Self { pointers }
    }

    /// Split one rendered document into fragments.
    pub fn new(document: &OpenApiDocument) -> Self {
        let mut pointers = BTreeMap::new();
        let root = document.root();
        if let Some(paths) = root.get("paths").and_then(Json::as_object) {
            for (template, item) in paths {
                let Some(item) = item.as_object() else {
                    continue;
                };
                for (method, operation) in item {
                    pointers.insert(paths_pointer(template, method), operation.clone());
                }
            }
        }
        if let Some(components) = root.get("components").and_then(Json::as_object) {
            for section in ["schemas", "responses", "securitySchemes"] {
                let Some(members) = components.get(section).and_then(Json::as_object) else {
                    continue;
                };
                for (name, value) in members {
                    pointers.insert(
                        format!("/components/{section}/{}", super::id::escape_pointer(name)),
                        value.clone(),
                    );
                }
            }
        }
        Self { pointers }
    }

    /// Every fragment pointer, byte-sorted.
    pub fn pointers(&self) -> impl Iterator<Item = &String> {
        self.pointers.keys()
    }

    /// One fragment by pointer.
    pub fn get(&self, pointer: &str) -> Option<&Json> {
        self.pointers.get(pointer)
    }

    /// The owned (name-sorted) map.
    pub fn as_map(&self) -> &BTreeMap<String, Json> {
        &self.pointers
    }
}

/// The generated-sidecar ownership manifest of one fragment-mode
/// document: every generated pointer with the semantic id that owns
/// it. Operations are owned by their endpoint id; reusable schemas by
/// their `x-lekalo-symbol`.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct OwnershipManifest {
    generator: (String, String),
    inputs: BTreeMap<String, String>,
    pointers: BTreeMap<String, String>,
}

impl OwnershipManifest {
    /// Assemble from parts (crate internal and tests).
    pub(crate) fn from_map(
        generator: (String, String),
        inputs: BTreeMap<String, String>,
        pointers: BTreeMap<String, String>,
    ) -> Self {
        Self {
            generator,
            inputs,
            pointers,
        }
    }

    /// Assemble the manifest of one rendered document. `inputs` are
    /// the canonical input digests (`model`, `ir`, `transport`).
    /// Operations are owned by their endpoint id; schemas by their
    /// `x-lekalo-symbol`; shared category responses and security
    /// schemes by the generator itself — every generated fragment
    /// pointer is claimed, so a merge never classifies our own bytes
    /// as manual content.
    pub fn of_document(document: &OpenApiDocument, inputs: BTreeMap<String, String>) -> Self {
        let mut pointers = BTreeMap::new();
        // Operations: owned by the endpoint id (the render's pointer
        // list is the authority).
        for (pointer, endpoint) in document.operation_pointers() {
            pointers.insert(pointer.clone(), endpoint.clone());
        }
        // Components: schemas carry their symbol; shared responses and
        // security schemes are generator-owned.
        if let Some(components) = document.root().get("components").and_then(Json::as_object) {
            for (section, owner) in [
                ("schemas", None),
                ("responses", Some(GENERATOR_ID)),
                ("securitySchemes", Some(GENERATOR_ID)),
            ] {
                let Some(members) = components.get(section).and_then(Json::as_object) else {
                    continue;
                };
                for (name, value) in members {
                    let pointer = format!("/components/{section}/{}", escape_pointer(name));
                    if pointers.contains_key(&pointer) {
                        continue;
                    }
                    let owner = match owner {
                        Some(generator) => generator.to_owned(),
                        None => value
                            .get("x-lekalo-symbol")
                            .and_then(Json::as_str)
                            .map(str::to_owned)
                            .unwrap_or_else(|| GENERATOR_ID.to_owned()),
                    };
                    pointers.insert(pointer, owner);
                }
            }
        }
        Self {
            generator: (GENERATOR_ID.to_owned(), GENERATOR_VERSION.to_owned()),
            inputs,
            pointers,
        }
    }

    /// The owner semantic id of one pointer, or nothing.
    pub fn owner(&self, pointer: &str) -> Option<&String> {
        self.pointers.get(pointer)
    }

    /// Every owned pointer, byte-sorted.
    pub fn pointers(&self) -> impl Iterator<Item = (&String, &String)> {
        self.pointers.iter()
    }

    /// The wire form (canonical JSON value: byte-sorted keys).
    pub fn to_value(&self) -> Json {
        let mut object = Map::new();
        object.insert(
            "contract".to_owned(),
            Json::String(OWNERSHIP_CONTRACT.to_owned()),
        );
        let mut generator = Map::new();
        generator.insert("id".to_owned(), Json::String(self.generator.0.clone()));
        generator.insert("version".to_owned(), Json::String(self.generator.1.clone()));
        object.insert("generator".to_owned(), Json::Object(generator));
        object.insert("inputs".to_owned(), to_object(&self.inputs));
        object.insert("pointers".to_owned(), to_object(&self.pointers));
        Json::Object(object)
    }

    /// The canonical sidecar bytes (compact JSON, byte-sorted keys).
    pub fn canonical_bytes(&self) -> String {
        self.to_value().to_string()
    }

    /// Decode one sidecar document, or refuse with the registered
    /// input rule. Unknown shapes refuse closed: the manifest is a
    /// generated artifact, never hand-maintained.
    pub fn from_value(json: &Json) -> Result<Self, super::diagnostic::DiagnosticSet> {
        let Some(object) = json.as_object() else {
            return Err(diagnostic::input_invalid("manifest-shape"));
        };
        if object.get("contract").and_then(Json::as_str) != Some(OWNERSHIP_CONTRACT) {
            return Err(diagnostic::input_invalid("manifest-contract"));
        }
        let mut inputs = BTreeMap::new();
        match object.get("inputs") {
            Some(Json::Object(entries)) => {
                for (key, value) in entries {
                    let Some(digest) = value.as_str() else {
                        return Err(diagnostic::input_invalid("manifest-input"));
                    };
                    inputs.insert(key.clone(), digest.to_owned());
                }
            }
            _ => return Err(diagnostic::input_invalid("manifest-shape")),
        }
        let mut pointers = BTreeMap::new();
        match object.get("pointers") {
            Some(Json::Object(entries)) => {
                for (pointer, owner) in entries {
                    let Some(owner) = owner.as_str() else {
                        return Err(diagnostic::input_invalid("manifest-owner"));
                    };
                    if !pointer.starts_with('/') {
                        return Err(diagnostic::input_invalid("manifest-pointer"));
                    }
                    pointers.insert(pointer.clone(), owner.to_owned());
                }
            }
            _ => return Err(diagnostic::input_invalid("manifest-shape")),
        }
        Ok(Self {
            generator: (
                object
                    .get("generator")
                    .and_then(|generator| generator.get("id"))
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                object
                    .get("generator")
                    .and_then(|generator| generator.get("version"))
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            ),
            inputs,
            pointers,
        })
    }
}

/// One byte-sorted JSON object from a byte-sorted map.
fn to_object(map: &BTreeMap<String, String>) -> Json {
    Json::Object(
        map.iter()
            .map(|(key, value)| (key.clone(), Json::String(value.clone())))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_manifest_round_trips_canonically() {
        let mut inputs = BTreeMap::new();
        inputs.insert("model".to_owned(), "sha256:a".to_owned());
        let manifest = OwnershipManifest {
            generator: (GENERATOR_ID.to_owned(), GENERATOR_VERSION.to_owned()),
            inputs,
            pointers: BTreeMap::new(),
        };
        let decoded = OwnershipManifest::from_value(&manifest.to_value()).expect("decodes");
        assert_eq!(decoded, manifest);
        assert_eq!(
            decoded.to_value()["contract"],
            Json::String(OWNERSHIP_CONTRACT.to_owned())
        );
    }

    #[test]
    fn foreign_manifests_refuse_closed() {
        assert!(OwnershipManifest::from_value(&json!({})).is_err());
        assert!(OwnershipManifest::from_value(&json!({
            "contract": "lekalo/other/v1",
            "inputs": {},
            "pointers": {}
        }))
        .is_err());
        assert!(OwnershipManifest::from_value(&json!({
            "contract": OWNERSHIP_CONTRACT,
            "inputs": {},
            "pointers": { "paths/x": "planner.task" }
        }))
        .is_err());
    }
}
