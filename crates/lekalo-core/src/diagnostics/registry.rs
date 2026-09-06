//! The embedded diagnostic rule registry (issue #11).
//!
//! [`DiagnosticRegistry::embedded`] parses and validates the exact
//! `include_bytes!` registry data once per process. The registry is the only
//! source of rule identity: code, category, default severity, allowed
//! statuses, default message, location requirement, and closed data fields.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

use super::id::{DiagnosticCode, DiagnosticId, MessageId};
use super::types::{Category, DataFieldType, LocationRequirement, Severity};
use super::version::{REGISTRY_IDENTITY, REGISTRY_SCHEMA_VERSION, REGISTRY_VERSION};
use crate::result::Status;

/// The exact embedded registry bytes.
pub const REGISTRY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/diagnostic-registry.v1.4.0.json");

/// Why the embedded registry could not be trusted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    /// The bytes are not valid JSON or do not match the registry schema.
    Malformed,
    /// The discriminator or identity is unknown (unsupported, fail closed).
    Unsupported,
    /// A structural invariant (sort, uniqueness, closure) is violated.
    Invariant,
}

/// One registered rule with its immutable semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryEntry {
    id: DiagnosticId,
    code: DiagnosticCode,
    category: Category,
    default_severity: Severity,
    allowed_statuses: Vec<Status>,
    message_id: MessageId,
    default_message: String,
    location_requirement: LocationRequirement,
    data_fields: Vec<(String, DataFieldType)>,
    allowed_fix_ids: Vec<DiagnosticId>,
    lifecycle: Lifecycle,
}

/// The closed rule lifecycle. Retired entries are tombstoned forever and
/// never reassigned; adding a rule without a wire-shape change is a registry
/// minor increment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lifecycle {
    /// Currently emitted by a producer.
    Active,
    /// Frozen for a future owner; never emitted from this issue.
    Reserved,
    /// Recognized but no longer emitted; never reassigned.
    Retired,
}

impl Lifecycle {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "active" => Some(Self::Active),
            "reserved" => Some(Self::Reserved),
            "retired" => Some(Self::Retired),
            _ => None,
        }
    }
}

impl RegistryEntry {
    /// The dotted rule id.
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    /// The immutable `LEK-SUBSYSTEM-NNN` code.
    pub fn code(&self) -> &str {
        self.code.as_str()
    }

    pub fn category(&self) -> Category {
        self.category
    }

    pub fn default_severity(&self) -> Severity {
        self.default_severity
    }

    /// Whether the rule may appear under the given envelope status.
    pub fn allows_status(&self, status: Status) -> bool {
        self.allowed_statuses.contains(&status)
    }

    /// The registry default message (stable English in v1).
    pub fn default_message(&self) -> &str {
        &self.default_message
    }

    pub fn location_requirement(&self) -> LocationRequirement {
        self.location_requirement
    }

    /// The declared type of one data field, if the field is registered.
    pub fn data_field(&self, name: &str) -> Option<DataFieldType> {
        self.data_fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, kind)| *kind)
    }

    /// Whether the fix id is registered for this rule.
    pub fn allows_fix(&self, fix: &str) -> bool {
        self.allowed_fix_ids.iter().any(|id| id.as_str() == fix)
    }

    pub fn lifecycle(&self) -> Lifecycle {
        self.lifecycle
    }
}

/// The validated diagnostic registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticRegistry {
    registry_version: String,
    entries: BTreeMap<String, RegistryEntry>,
}

impl DiagnosticRegistry {
    /// The embedded, once-validated registry.
    ///
    /// A validation failure is a developer fault: the error is cached and
    /// every caller fails closed instead of trusting an unvalidated table.
    pub fn embedded() -> Result<&'static Self, RegistryError> {
        static REGISTRY: LazyLock<Result<DiagnosticRegistry, RegistryError>> =
            LazyLock::new(|| DiagnosticRegistry::from_bytes(REGISTRY_BYTES));
        REGISTRY.as_ref().map_err(Clone::clone)
    }

    /// Parse and validate registry bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RegistryError> {
        let wire: RegistryWire =
            serde_json::from_slice(bytes).map_err(|_| RegistryError::Malformed)?;
        if wire.schema_version != REGISTRY_SCHEMA_VERSION
            || wire.identity != REGISTRY_IDENTITY
            || !wire.closed
        {
            return Err(RegistryError::Unsupported);
        }
        let mut entries = BTreeMap::new();
        let mut previous: Option<String> = None;
        let mut seen_codes = std::collections::BTreeSet::new();
        for wire_entry in wire.entries {
            if let Some(previous) = &previous {
                if wire_entry.id.as_str() <= previous {
                    // Entries must be sorted by unsigned UTF-8 id bytes and
                    // unique; a stale or reordered registry fails closed.
                    return Err(RegistryError::Invariant);
                }
            }
            previous = Some(wire_entry.id.clone());
            if !seen_codes.insert(wire_entry.code.clone()) {
                return Err(RegistryError::Invariant);
            }
            if wire_entry.message_id != wire_entry.id {
                return Err(RegistryError::Invariant);
            }
            if wire_entry.default_message.is_empty() || wire_entry.default_message.len() > 256 {
                return Err(RegistryError::Invariant);
            }
            let entry = wire_entry.convert().ok_or(RegistryError::Invariant)?;
            entries.insert(entry.id.as_str().to_owned(), entry);
        }
        if wire.registry_version != REGISTRY_VERSION {
            return Err(RegistryError::Invariant);
        }
        Ok(Self {
            registry_version: wire.registry_version,
            entries,
        })
    }

    /// The registry version (independent contract version).
    pub fn registry_version(&self) -> &str {
        &self.registry_version
    }

    /// Look up one rule by id.
    pub fn entry(&self, id: &str) -> Option<&RegistryEntry> {
        self.entries.get(id)
    }

    /// Every entry, sorted by id.
    pub fn entries(&self) -> impl Iterator<Item = &RegistryEntry> {
        self.entries.values()
    }

    /// Number of registered rules.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry is empty (never true for the embedded one).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryWire {
    schema_version: String,
    identity: String,
    registry_version: String,
    closed: bool,
    entries: Vec<EntryWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    id: String,
    code: String,
    category: String,
    default_severity: String,
    allowed_statuses: Vec<String>,
    message_id: String,
    default_message: String,
    location_requirement: String,
    data_fields: Vec<DataFieldWire>,
    allowed_fix_ids: Vec<String>,
    lifecycle: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DataFieldWire {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

impl EntryWire {
    fn convert(self) -> Option<RegistryEntry> {
        let id = DiagnosticId::new(self.id)?;
        let code = DiagnosticCode::new(self.code)?;
        let category = Category::parse(&self.category)?;
        let default_severity = Severity::parse(&self.default_severity)?;
        let mut allowed_statuses = Vec::with_capacity(self.allowed_statuses.len());
        for status in &self.allowed_statuses {
            allowed_statuses.push(match status.as_str() {
                "valid" => Status::Valid,
                "invalid" => Status::Invalid,
                "denied" => Status::Denied,
                "unsupported" => Status::Unsupported,
                "unavailable" => Status::Unavailable,
                "unsupported-version" => Status::UnsupportedVersion,
                _ => return None,
            });
        }
        let message_id = MessageId::new(self.message_id)?;
        let location_requirement = LocationRequirement::parse(&self.location_requirement)?;
        let mut data_fields = Vec::with_capacity(self.data_fields.len());
        for field in self.data_fields {
            data_fields.push((field.name, DataFieldType::parse(&field.kind)?));
        }
        let mut allowed_fix_ids = Vec::with_capacity(self.allowed_fix_ids.len());
        for fix in self.allowed_fix_ids {
            allowed_fix_ids.push(DiagnosticId::new(fix)?);
        }
        let lifecycle = Lifecycle::parse(&self.lifecycle)?;
        Some(RegistryEntry {
            id,
            code,
            category,
            default_severity,
            allowed_statuses,
            message_id,
            default_message: self.default_message,
            location_requirement,
            data_fields,
            allowed_fix_ids,
            lifecycle,
        })
    }
}
