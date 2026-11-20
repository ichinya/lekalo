//! Confined, offline project-owned JSON Schema registry.
use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::loader::{frontends, source::LineIndex};
use crate::lockfile::{ComponentId, ComponentRef, SemVer, Sha256Digest};
use crate::project_fs::{path_violation, EntryType, Fs, FsErrorKind};
use crate::runtime::{compile_schema, config, SchemaError};

const DIRECTORY: &str = "lekalo/contracts";
const SUFFIX: &str = ".schema.json";
pub(crate) const MAX_SCHEMAS: usize = 32;
const MAX_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 32;

type Result<T> = std::result::Result<T, RegistryError>;

/// Stable refusal categories for project schema admission and selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryError {
    Path,
    Limit,
    Identifier,
    Reference,
    Dialect,
    Invalid,
    Missing,
    DigestMismatch,
}

impl RegistryError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Path => "project-schema.path-denied",
            Self::Limit => "project-schema.limit",
            Self::Identifier => "project-schema.identifier-conflict",
            Self::Reference => "project-schema.external-reference",
            Self::Dialect => "project-schema.unsupported-dialect",
            Self::Invalid => "project-schema.invalid",
            Self::Missing => "project-schema.missing",
            Self::DigestMismatch => "project-schema.digest-mismatch",
        }
    }
}

/// One schema identity and its exact file digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pin {
    pub identity: ComponentRef,
    pub digest: Sha256Digest,
}

impl Pin {
    pub(crate) fn parse(name: &str, version: &str, digest: &str) -> Result<Self> {
        let logical = format!("{DIRECTORY}/{name}/{version}{SUFFIX}");
        if name.contains('/') || version.contains('/') || path_violation(&logical).is_some() {
            return Err(RegistryError::Path);
        }
        let id = ComponentId::parse(name).map_err(|_| RegistryError::Path)?;
        let version = SemVer::parse(version).map_err(|_| RegistryError::Path)?;
        let digest = Sha256Digest::parse(digest).map_err(|_| RegistryError::Invalid)?;
        Ok(Self {
            identity: ComponentRef::new(id, version),
            digest,
        })
    }
}

fn fs_error(error: FsErrorKind) -> RegistryError {
    match error {
        FsErrorKind::Limit { .. } => RegistryError::Limit,
        _ => RegistryError::Path,
    }
}

/// One in-memory snapshot of exact file digests and compiled schemas.
#[derive(Debug, Default)]
pub struct Registry {
    schemas: BTreeMap<(ComponentId, SemVer), Schema>,
}

#[derive(Debug)]
struct Schema {
    digest: Sha256Digest,
    validator: jsonschema::Validator,
}

impl Registry {
    /// Read and compile every schema from an already-selected project root.
    pub fn load(root: &Path) -> Result<Self> {
        let fs = Fs::open(root).map_err(fs_error)?;
        let root_entries = match fs.entries_bounded("lekalo", 64) {
            Ok(entries) => entries,
            Err(FsErrorKind::NotFound) => return Ok(Self::default()),
            Err(error) => return Err(fs_error(error)),
        };
        let mut present = false;
        for (name, kind) in root_entries {
            if name.eq_ignore_ascii_case("contracts") {
                if name != "contracts" || kind != EntryType::Directory {
                    return Err(RegistryError::Path);
                }
                present = true;
            }
        }
        if !present {
            return Ok(Self::default());
        }
        let mut schemas = BTreeMap::new();
        for (name, kind) in fs
            .entries_bounded(DIRECTORY, MAX_SCHEMAS)
            .map_err(fs_error)?
        {
            let directory = format!("{DIRECTORY}/{name}");
            let id = ComponentId::parse(&name).map_err(|_| RegistryError::Path)?;
            if kind != EntryType::Directory || path_violation(&directory).is_some() {
                return Err(RegistryError::Path);
            }
            let files = fs
                .entries_bounded(&directory, MAX_SCHEMAS - schemas.len())
                .map_err(fs_error)?;
            for (filename, kind) in files {
                let logical = format!("{directory}/{filename}");
                if kind != EntryType::File || path_violation(&logical).is_some() {
                    return Err(RegistryError::Path);
                }
                let version = filename.strip_suffix(SUFFIX).ok_or(RegistryError::Path)?;
                let version = SemVer::parse(version).map_err(|_| RegistryError::Path)?;
                let bytes = fs
                    .read_logical(&logical, MAX_BYTES)
                    .map_err(fs_error)?
                    .ok_or(RegistryError::Path)?;
                let validator = compile(&bytes)?;
                let digest = Sha256Digest::from_hex(&crate::digest::sha256_hex(&bytes));
                schemas.insert((id.clone(), version), Schema { digest, validator });
            }
        }
        Ok(Self { schemas })
    }

    /// Exact identities and digests for this snapshot in canonical order.
    pub fn pins(&self) -> impl Iterator<Item = (ComponentRef, &Sha256Digest)> {
        self.schemas.iter().map(|((name, version), schema)| {
            (
                ComponentRef::new(name.clone(), version.clone()),
                &schema.digest,
            )
        })
    }

    pub(crate) fn owned_pins(&self) -> Vec<Pin> {
        self.pins()
            .map(|(identity, digest)| Pin {
                identity,
                digest: digest.clone(),
            })
            .collect()
    }

    /// Select an exact version and check its digest before use.
    pub fn validator(
        &self,
        identity: &ComponentRef,
        expected: &Sha256Digest,
    ) -> Result<&jsonschema::Validator> {
        let key = (identity.id().clone(), identity.version().clone());
        let schema = self.schemas.get(&key).ok_or(RegistryError::Missing)?;
        if &schema.digest != expected {
            return Err(RegistryError::DigestMismatch);
        }
        Ok(&schema.validator)
    }
}

fn compile(bytes: &[u8]) -> Result<jsonschema::Validator> {
    let text = std::str::from_utf8(bytes).map_err(|_| RegistryError::Invalid)?;
    let document = frontends::json::parse(text, &LineIndex::new(text)).map_err(|diagnostics| {
        if diagnostics
            .iter()
            .any(|item| item.code == "loader.limit-exceeded")
        {
            RegistryError::Limit
        } else {
            RegistryError::Invalid
        }
    })?;
    let frontends::Parsed::Root(node) = document else {
        return Err(RegistryError::Invalid);
    };
    let schema = config::value_from_node(*node).map_err(|_| RegistryError::Invalid)?;
    check_depth(&schema, 0)?;
    compile_schema(&schema).map_err(|error| match error {
        SchemaError::Identifier => RegistryError::Identifier,
        SchemaError::Reference => RegistryError::Reference,
        SchemaError::Dialect => RegistryError::Dialect,
        SchemaError::KeywordShape | SchemaError::Invalid => RegistryError::Invalid,
    })
}

fn check_depth(value: &Value, depth: usize) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(RegistryError::Limit);
    }
    match value {
        Value::Array(items) => items.iter().try_for_each(|v| check_depth(v, depth + 1)),
        Value::Object(fields) => fields.values().try_for_each(|v| check_depth(v, depth + 1)),
        _ => Ok(()),
    }
}
