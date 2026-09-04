//! Deterministic canonical serialization of the lock wire (issue #10).
//!
//! Compact UTF-8 JSON, object keys in unsigned UTF-8 byte order, semantic
//! arrays in the documented sorted order, RFC 8259 mandatory escaping with
//! every other Unicode scalar verbatim — the same string writer the #7
//! loader and #8 IR use. The canonical payload never ends in whitespace:
//! the file is the payload plus exactly one LF, and [`LockDigest`] is
//! SHA-256 over the payload bytes without that LF, so the file can never
//! carry its own digest.

use super::types::Lockfile;
use super::LockDigest;
use crate::versioning::compatibility::AdapterCompatibilityManifest;

/// A canonical JSON value with structurally correct key order.
pub(crate) enum Canon {
    /// JSON `null`.
    Null,
    /// A JSON string.
    Str(String),
    /// A JSON array in exact emission order.
    Arr(Vec<Canon>),
    /// A JSON object; keys are sorted by raw bytes at construction.
    Obj(Vec<(&'static str, Canon)>),
}

impl Canon {
    /// Build one object with byte-sorted keys.
    pub(crate) fn object(mut fields: Vec<(&'static str, Canon)>) -> Canon {
        fields.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        debug_assert!(fields.windows(2).all(|pair| pair[0].0 != pair[1].0));
        Canon::Obj(fields)
    }

    /// Build one array from ordered items.
    pub(crate) fn array(items: Vec<Canon>) -> Canon {
        Canon::Arr(items)
    }

    /// One string value.
    pub(crate) fn str(text: impl Into<String>) -> Canon {
        Canon::Str(text.into())
    }

    /// Serialize compactly onto `out`.
    pub(crate) fn write(&self, out: &mut String) {
        match self {
            Canon::Null => out.push_str("null"),
            Canon::Str(text) => {
                crate::loader::canonical::write_json_string(text, out);
            }
            Canon::Arr(items) => {
                out.push('[');
                for (position, item) in items.iter().enumerate() {
                    if position > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Canon::Obj(fields) => {
                out.push('{');
                for (position, (key, value)) in fields.iter().enumerate() {
                    if position > 0 {
                        out.push(',');
                    }
                    crate::loader::canonical::write_json_string(key, out);
                    out.push(':');
                    value.write(out);
                }
                out.push('}');
            }
        }
    }

    /// The full canonical JSON text.
    pub(crate) fn to_json(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }
}

/// The canonical wire representation of one parsed lock.
pub(crate) fn lock_canon(lock: &Lockfile) -> Canon {
    Canon::object(vec![
        ("schema_version", Canon::str(super::types::SCHEMA_VERSION)),
        (
            "resolver",
            Canon::object(vec![
                (
                    "catalogs",
                    Canon::array(
                        lock.catalogs()
                            .iter()
                            .map(|catalog| {
                                Canon::object(vec![
                                    ("digest", Canon::str(catalog.digest().as_str())),
                                    ("id", Canon::str(catalog.id().as_str())),
                                ])
                            })
                            .collect(),
                    ),
                ),
                ("request_digest", Canon::str(lock.request_digest().as_str())),
                ("version", Canon::str(lock.resolver_version().as_str())),
            ]),
        ),
        (
            "core",
            Canon::object(vec![("version", Canon::str(lock.core_version().as_str()))]),
        ),
        (
            "contracts",
            Canon::object(vec![
                (
                    "ir",
                    Canon::object(vec![
                        ("digest", Canon::str(lock.ir_pin().digest().as_str())),
                        ("version", Canon::str(lock.ir_pin().version().as_str())),
                    ]),
                ),
                (
                    "model",
                    Canon::object(vec![
                        ("digest", Canon::str(lock.model_pin().digest().as_str())),
                        ("version", Canon::str(lock.model_pin().version().as_str())),
                    ]),
                ),
                (
                    "registry",
                    Canon::object(vec![
                        ("digest", Canon::str(lock.registry_pin().digest().as_str())),
                        (
                            "version",
                            Canon::str(lock.registry_pin().version().as_str()),
                        ),
                    ]),
                ),
                match lock.target_protocol() {
                    Some(protocol) => (
                        "target_protocol",
                        Canon::object(vec![
                            ("digest", Canon::str(protocol.digest().as_str())),
                            ("version", Canon::str(protocol.version().as_str())),
                        ]),
                    ),
                    None => ("target_protocol", Canon::Null),
                },
            ]),
        ),
        (
            "adapters",
            Canon::array(
                lock.adapters()
                    .iter()
                    .map(|adapter| {
                        Canon::object(vec![
                            ("id", Canon::str(adapter.id().as_str())),
                            ("version", Canon::str(adapter.version().as_str())),
                            ("digest", Canon::str(adapter.digest().as_str())),
                            (
                                "source",
                                Canon::object(vec![
                                    ("kind", Canon::str(adapter.source().kind().as_str())),
                                    ("id", Canon::str(adapter.source().id())),
                                    ("digest", Canon::str(adapter.source().digest().as_str())),
                                ]),
                            ),
                            (
                                "compatibility_digest",
                                Canon::str(adapter.compatibility_digest().as_str()),
                            ),
                            (
                                "artifacts",
                                Canon::array(
                                    adapter
                                        .artifacts()
                                        .iter()
                                        .map(|artifact| {
                                            Canon::object(vec![
                                                (
                                                    "platform",
                                                    Canon::str(artifact.platform().as_str()),
                                                ),
                                                ("digest", Canon::str(artifact.digest().as_str())),
                                            ])
                                        })
                                        .collect(),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "generators",
            Canon::array(
                lock.generators()
                    .iter()
                    .map(|generator| {
                        Canon::object(vec![
                            ("id", Canon::str(generator.id().as_str())),
                            ("version", Canon::str(generator.version().as_str())),
                            ("digest", Canon::str(generator.digest().as_str())),
                            (
                                "adapter",
                                Canon::object(vec![
                                    ("id", Canon::str(generator.adapter().id().as_str())),
                                    (
                                        "version",
                                        Canon::str(generator.adapter().version().as_str()),
                                    ),
                                ]),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "profiles",
            Canon::array(
                lock.profiles()
                    .iter()
                    .map(|profile| {
                        Canon::object(vec![
                            ("id", Canon::str(profile.id().as_str())),
                            ("version", Canon::str(profile.version().as_str())),
                            (
                                "source_digest",
                                Canon::str(profile.source_digest().as_str()),
                            ),
                            ("digest", Canon::str(profile.digest().as_str())),
                            (
                                "adapters",
                                Canon::array(
                                    profile.adapters().iter().map(component_ref_canon).collect(),
                                ),
                            ),
                            (
                                "generators",
                                Canon::array(
                                    profile
                                        .generators()
                                        .iter()
                                        .map(component_ref_canon)
                                        .collect(),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "capabilities",
            Canon::array(
                lock.capabilities()
                    .iter()
                    .map(|capability| {
                        Canon::object(vec![
                            ("target", Canon::str(capability.target().as_str())),
                            ("profile", Canon::str(capability.profile().as_str())),
                            ("id", Canon::str(capability.id().as_str())),
                            ("version", Canon::str(capability.version().as_str())),
                            ("support", Canon::str(capability.support().as_str())),
                            (
                                "provider",
                                Canon::object(vec![
                                    ("kind", Canon::str(capability.provider().kind().as_str())),
                                    ("id", Canon::str(capability.provider().id().as_str())),
                                    (
                                        "version",
                                        Canon::str(capability.provider().version().as_str()),
                                    ),
                                ]),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn component_ref_canon(reference: &super::types::ComponentRef) -> Canon {
    Canon::object(vec![
        ("id", Canon::str(reference.id().as_str())),
        ("version", Canon::str(reference.version().as_str())),
    ])
}

/// The exact canonical file bytes: payload plus exactly one LF.
pub(crate) fn file_bytes(lock: &Lockfile) -> Vec<u8> {
    let mut bytes = payload_bytes(lock);
    bytes.push(b'\n');
    bytes
}

/// The canonical payload bytes without the final LF.
pub(crate) fn payload_bytes(lock: &Lockfile) -> Vec<u8> {
    lock_canon(lock).to_json().into_bytes()
}

/// The lock digest: SHA-256 of the canonical payload bytes.
pub(crate) fn lock_digest(lock: &Lockfile) -> LockDigest {
    LockDigest::new(super::Sha256Digest::from_hex(
        &crate::versioning::plan::sha256_hex(&payload_bytes(lock)),
    ))
}

/// The canonical bytes of one `#9` adapter compatibility manifest: the
/// declared `compatibility_digest` domain.
pub(crate) fn manifest_bytes(manifest: &AdapterCompatibilityManifest) -> Vec<u8> {
    let (protocol_min, protocol_max) = match (
        manifest.protocol_min.as_ref(),
        manifest.protocol_max.as_ref(),
    ) {
        (Some(min), Some(max)) => (
            ("protocol_min", Canon::str(min.to_string())),
            ("protocol_max", Canon::str(max.to_string())),
        ),
        _ => (("protocol_min", Canon::Null), ("protocol_max", Canon::Null)),
    };
    Canon::object(vec![
        ("adapter_id", Canon::str(manifest.adapter_id.clone())),
        ("ir_max", Canon::str(manifest.ir_max.to_string())),
        ("ir_min", Canon::str(manifest.ir_min.to_string())),
        (
            "manifest_version",
            Canon::str(manifest.manifest_version.to_string()),
        ),
        (
            "optional_extensions",
            Canon::array(
                manifest
                    .optional_extensions
                    .iter()
                    .map(|extension| Canon::str(extension.clone()))
                    .collect(),
            ),
        ),
        protocol_max,
        protocol_min,
        (
            "required_extensions",
            Canon::array(
                manifest
                    .required_extensions
                    .iter()
                    .map(|extension| Canon::str(extension.clone()))
                    .collect(),
            ),
        ),
    ])
    .to_json()
    .into_bytes()
}
