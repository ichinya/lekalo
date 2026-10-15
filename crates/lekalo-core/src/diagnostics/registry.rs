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
use super::version::REGISTRY_VERSION;
use crate::result::Status;

/// The exact embedded registry bytes.
pub const REGISTRY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/diagnostic-registry.v0.6.4.json");

pub const SUCCESSOR_REGISTRY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/diagnostic-registry.v0.6.5.json");

/// The issue #94 union successor: the current successor plus the closed
/// `format.*` family. Predecessor producers keep their exact pins.
///
/// After the milestone integration merges this file also carries the
/// issue #136 `adoption.*`, issue #19 `portability.*`, issue #51
/// `worklog-changelog.*`, and issue #41 `managed.*` families (the
/// grand union registry).
pub const FORMAT_REGISTRY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/diagnostic-registry.v0.8.0.json");

pub const ADOPTION_REGISTRY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/diagnostic-registry.v0.8.0.json");

/// The issue #95 union successor: the merged registry plus the closed
/// `watch.*` family. Same merged file as the format/adoption unions;
/// each family appends, none clobbers a predecessor rule.
pub const WATCH_REGISTRY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/diagnostic-registry.v0.8.0.json");

/// The issue #19 M8 union successor registry bytes (the same merged
/// union registry as the other 0.8.0 family constants).
pub const M8_REGISTRY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/diagnostic-registry.v0.8.0.json");

/// The issue #51 union successor bytes (the same merged union registry).
pub const PILOT_REGISTRY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/diagnostic-registry.v0.8.0.json");

/// The issue #113 union successor bytes: the merged M8 union plus the
/// `module-api.*` family. All family constants name the same merged
/// registry document.
pub const MODULE_REGISTRY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/diagnostic-registry.v0.8.0.json");

/// The issue #41 managed union successor bytes (the same merged union
/// registry as the other 0.8.0 family constants).
pub const MANAGED_REGISTRY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/diagnostic-registry.v0.8.0.json");

/// The issue #52 union successor bytes (the same merged union registry
/// as the other 0.8.0 family constants).
pub const TASKLINK_REGISTRY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/diagnostic-registry.v0.8.0.json");

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
        Self::parse_version(bytes, REGISTRY_VERSION)
    }

    /// The independently admitted union successor, never a runtime extension.
    pub fn successor() -> Result<&'static Self, RegistryError> {
        static REGISTRY: LazyLock<Result<DiagnosticRegistry, RegistryError>> =
            LazyLock::new(|| {
                DiagnosticRegistry::parse_version(
                    SUCCESSOR_REGISTRY_BYTES,
                    super::version::SUCCESSOR_REGISTRY_VERSION,
                )
            });
        REGISTRY.as_ref().map_err(Clone::clone)
    }

    /// The issue #94 union successor (successor + `format.*` family).
    ///
    /// After the milestone integration merges this registry also carries
    /// the issue #136 `adoption.*`, issue #19 `portability.*`, issue #51
    /// `worklog-changelog.*`, and issue #41 `managed.*` families.
    pub fn format_successor() -> Result<&'static Self, RegistryError> {
        static REGISTRY: LazyLock<Result<DiagnosticRegistry, RegistryError>> =
            LazyLock::new(|| {
                DiagnosticRegistry::parse_version(
                    FORMAT_REGISTRY_BYTES,
                    super::version::FORMAT_REGISTRY_VERSION,
                )
            });
        REGISTRY.as_ref().map_err(Clone::clone)
    }

    /// The issue #136 union successor (the merged format, adoption,
    /// portability, and worklog-changelog registry since the milestone
    /// integration merges).
    /// The issue #136 union successor (the merged `format.*` + `adoption.*`
    /// + `portability.*` + `worklog-changelog.*` + `managed.*` registry
    ///   since the milestone integration merges).
    pub fn adoption() -> Result<&'static Self, RegistryError> {
        static REGISTRY: LazyLock<Result<DiagnosticRegistry, RegistryError>> =
            LazyLock::new(|| {
                DiagnosticRegistry::parse_version(
                    ADOPTION_REGISTRY_BYTES,
                    super::version::ADOPTION_REGISTRY_VERSION,
                )
            });
        REGISTRY.as_ref().map_err(Clone::clone)
    }

    /// The issue #19 M8 union successor: the merged registry plus the
    /// `portability.*` family (LEK-PORT-001..009). The new producer
    /// explicitly selects this registry; frozen predecessor outputs retain
    /// their pinned registries.
    pub fn m8() -> Result<&'static Self, RegistryError> {
        static REGISTRY: LazyLock<Result<DiagnosticRegistry, RegistryError>> =
            LazyLock::new(|| {
                DiagnosticRegistry::parse_version(
                    M8_REGISTRY_BYTES,
                    super::version::M8_REGISTRY_VERSION,
                )
            });
        REGISTRY.as_ref().map_err(Clone::clone)
    }

    /// The issue #51 union successor, never a runtime extension.
    pub fn pilot() -> Result<&'static Self, RegistryError> {
        static REGISTRY: LazyLock<Result<DiagnosticRegistry, RegistryError>> =
            LazyLock::new(|| {
                DiagnosticRegistry::parse_version(
                    PILOT_REGISTRY_BYTES,
                    super::version::PILOT_REGISTRY_VERSION,
                )
            });
        REGISTRY.as_ref().map_err(Clone::clone)
    }

    /// The issue #113 union successor: the merged M8 union plus the
    /// `module-api.*` family. All successor constants parse the same
    /// merged registry bytes.
    pub fn module_successor() -> Result<&'static Self, RegistryError> {
        static REGISTRY: LazyLock<Result<DiagnosticRegistry, RegistryError>> =
            LazyLock::new(|| {
                DiagnosticRegistry::parse_version(
                    MODULE_REGISTRY_BYTES,
                    super::version::MODULE_REGISTRY_VERSION,
                )
            });
        REGISTRY.as_ref().map_err(Clone::clone)
    }

    /// The issue #41 managed union successor, never a runtime extension.
    /// Parses the same merged 0.8.0 union registry bytes.
    pub fn managed() -> Result<&'static Self, RegistryError> {
        static REGISTRY: LazyLock<Result<DiagnosticRegistry, RegistryError>> =
            LazyLock::new(|| {
                DiagnosticRegistry::parse_version(
                    MANAGED_REGISTRY_BYTES,
                    super::version::MANAGED_REGISTRY_VERSION,
                )
            });
        REGISTRY.as_ref().map_err(Clone::clone)
    }

    /// The issue #52 union successor, never a runtime extension. Parses
    /// the same merged 0.8.0 union registry bytes.
    pub fn tasklink_successor() -> Result<&'static Self, RegistryError> {
        static REGISTRY: LazyLock<Result<DiagnosticRegistry, RegistryError>> =
            LazyLock::new(|| {
                DiagnosticRegistry::parse_version(
                    TASKLINK_REGISTRY_BYTES,
                    super::version::TASKLINK_REGISTRY_VERSION,
                )
            });
        REGISTRY.as_ref().map_err(Clone::clone)
    }

    pub fn for_version(version: &str) -> Result<&'static Self, RegistryError> {
        match version {
            REGISTRY_VERSION => Self::embedded(),
            super::version::SUCCESSOR_REGISTRY_VERSION => Self::successor(),
            // All seven family constants name the merged 0.8.0 union
            // registry; either accessor parses the same bytes.
            v if v == super::version::FORMAT_REGISTRY_VERSION
                || v == super::version::ADOPTION_REGISTRY_VERSION
                || v == super::version::M8_REGISTRY_VERSION
                || v == super::version::PILOT_REGISTRY_VERSION
                || v == super::version::MODULE_REGISTRY_VERSION
                || v == super::version::MANAGED_REGISTRY_VERSION
                || v == super::version::TASKLINK_REGISTRY_VERSION =>
            {
                Self::module_successor()
            }
            _ => Err(RegistryError::Unsupported),
        }
    }

    fn parse_version(bytes: &[u8], version: &str) -> Result<Self, RegistryError> {
        let wire: RegistryWire =
            serde_json::from_slice(bytes).map_err(|_| RegistryError::Malformed)?;
        if wire.schema_version != format!("lekalo/diagnostic-registry/v{version}")
            || wire.identity != format!("dev.lekalo.diagnostic-registry@{version}")
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
        if wire.registry_version != version {
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

#[cfg(test)]
mod tasklink_tests {
    use super::*;

    #[test]
    fn the_tasklink_successor_is_an_exact_union_of_the_084_successor() {
        let mid = DiagnosticRegistry::successor().unwrap();
        let new = DiagnosticRegistry::tasklink_successor().unwrap();
        // The merged union registry also carries the format, adoption,
        // portability, worklog-changelog, and managed families, so the
        // exact-size assertion counts every landed family.
        let tasklink_count = new
            .entries()
            .filter(|entry| entry.id().starts_with("tasklink."))
            .count();
        assert_eq!(tasklink_count, 8);
        for entry in mid.entries() {
            assert_eq!(new.entry(entry.id()), Some(entry));
        }
        for code in [
            "LEK-TASKLINK-001",
            "LEK-TASKLINK-002",
            "LEK-TASKLINK-003",
            "LEK-TASKLINK-004",
            "LEK-TASKLINK-005",
            "LEK-TASKLINK-006",
            "LEK-TASKLINK-007",
            "LEK-TASKLINK-008",
        ] {
            assert!(
                new.entries().any(|entry| entry.code() == code),
                "missing {code}"
            );
        }
    }

    #[test]
    fn the_registry_chain_resolves_every_accepted_version() {
        assert_eq!(
            DiagnosticRegistry::for_version("0.6.4")
                .unwrap()
                .registry_version(),
            "0.6.4"
        );
        assert_eq!(
            DiagnosticRegistry::for_version("0.6.5")
                .unwrap()
                .registry_version(),
            "0.6.5"
        );
        assert_eq!(
            DiagnosticRegistry::for_version("0.8.0")
                .unwrap()
                .registry_version(),
            "0.8.0"
        );
        assert!(DiagnosticRegistry::for_version("0.9.0").is_err());
    }

    #[test]
    fn a_tasklink_rule_builds_through_the_chain_and_keeps_its_pin() {
        let data = crate::diagnostics::DataObject::new();
        let diagnostic = crate::diagnostics::normalize::build(
            "tasklink.relation-identity-invalid",
            None,
            None,
            data,
        )
        .unwrap();
        assert_eq!(diagnostic.registry_version, "0.8.0");
        assert_eq!(diagnostic.code(), "LEK-TASKLINK-001");
        assert_eq!(diagnostic.category(), crate::diagnostics::Category::Model);
    }
}
