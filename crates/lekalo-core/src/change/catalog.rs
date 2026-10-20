//! The embedded change operation catalog (issue #73).
//!
//! [`Catalog::embedded`] parses and validates the exact `include_str!`
//! catalog once per process. The catalog is the machine-readable discovery
//! surface for CLI sugar and a future MCP tool: it declares every request
//! discriminator, whether the running implementation provides it, and the
//! mutation class and evidence each operation requires. The catalog and the
//! sealed executors bind one-to-one.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

use super::version;
use crate::digest::sha256_hex;

/// Why the embedded catalog could not be trusted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogError {
    /// The bytes are not valid JSON or do not match the catalog schema.
    Malformed,
    /// The discriminator or identity is unknown (unsupported, fail closed).
    Unsupported,
    /// A structural invariant (sort, uniqueness) is violated.
    Invariant,
}

/// The closed implementation status of one catalog operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationStatus {
    /// The operation has a sealed executor in this build.
    Implemented,
    /// The operation is cataloged but refuses with `change.operation-unsupported`.
    Unsupported,
}

impl OperationStatus {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "implemented" => Some(Self::Implemented),
            "unsupported" => Some(Self::Unsupported),
            _ => None,
        }
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Implemented => "implemented",
            Self::Unsupported => "unsupported",
        }
    }
}

/// The closed mutation class of one catalog operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum MutationClass {
    /// Purely additive: no existing member changes meaning.
    Additive,
    /// Adds one member to an existing definition.
    MemberAdditive,
    /// Removes, renames, or redescribes one existing member.
    MemberDestructive,
    /// Adds, removes, or relocates whole definitions.
    Destructive,
    /// Changes adoption, custody, or physical-home metadata only.
    Administrative,
}

impl MutationClass {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "additive" => Some(Self::Additive),
            "member-additive" => Some(Self::MemberAdditive),
            "member-destructive" => Some(Self::MemberDestructive),
            "destructive" => Some(Self::Destructive),
            "administrative" => Some(Self::Administrative),
            _ => None,
        }
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Additive => "additive",
            Self::MemberAdditive => "member-additive",
            Self::MemberDestructive => "member-destructive",
            Self::Destructive => "destructive",
            Self::Administrative => "administrative",
        }
    }
}

/// One registered operation with its immutable discovery facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogEntry {
    operation: String,
    status: OperationStatus,
    title: String,
    mutation_class: MutationClass,
    required_evidence: Vec<String>,
}

impl CatalogEntry {
    /// The stable operation discriminator (`family:action`).
    pub fn operation(&self) -> &str {
        &self.operation
    }

    /// Whether the running implementation provides this operation.
    pub const fn status(&self) -> OperationStatus {
        self.status
    }

    /// The stable one-line title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The closed mutation class.
    pub const fn mutation_class(&self) -> MutationClass {
        self.mutation_class
    }

    /// The required-evidence capabilities declared for this operation.
    pub fn required_evidence(&self) -> &[String] {
        &self.required_evidence
    }
}

/// The validated change operation catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Catalog {
    catalog_version: String,
    entries: BTreeMap<String, CatalogEntry>,
    digest: String,
}

/// The exact embedded catalog bytes.
pub const CATALOG_BYTES: &str = include_str!("../../../../contracts/change-catalog.v0.8.0.json");

impl Catalog {
    /// The embedded, once-validated catalog.
    ///
    /// A validation failure is a developer fault: the error is cached and
    /// every caller fails closed instead of trusting an unvalidated table.
    pub fn embedded() -> Result<&'static Self, CatalogError> {
        static CATALOG: LazyLock<Result<Catalog, CatalogError>> =
            LazyLock::new(|| Catalog::from_bytes(CATALOG_BYTES.as_bytes()));
        CATALOG.as_ref().map_err(Clone::clone)
    }

    /// Parse and validate catalog bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CatalogError> {
        let wire: CatalogWire =
            serde_json::from_slice(bytes).map_err(|_| CatalogError::Malformed)?;
        if wire.schema_version != version::CATALOG_SCHEMA_VERSION
            || wire.identity != version::CATALOG_IDENTITY
            || !wire.closed
            || wire.operations.is_empty()
        {
            return Err(CatalogError::Unsupported);
        }
        let mut entries = BTreeMap::new();
        let mut previous: Option<String> = None;
        for wire_entry in wire.operations {
            if let Some(previous) = &previous {
                if wire_entry.operation.as_str() <= previous.as_str() {
                    // Entries must be sorted by unsigned UTF-8 operation
                    // bytes and unique; a stale or reordered catalog fails
                    // closed.
                    return Err(CatalogError::Invariant);
                }
            }
            let operation = wire_entry.operation.clone();
            previous = Some(operation.clone());
            if wire_entry.title.is_empty() || wire_entry.title.len() > 128 {
                return Err(CatalogError::Invariant);
            }
            let status =
                OperationStatus::parse(&wire_entry.status).ok_or(CatalogError::Invariant)?;
            let mutation_class =
                MutationClass::parse(&wire_entry.mutation_class).ok_or(CatalogError::Invariant)?;
            let mut required_evidence = Vec::with_capacity(wire_entry.required_evidence.len());
            for evidence in wire_entry.required_evidence {
                if evidence.is_empty() || evidence.len() > 64 {
                    return Err(CatalogError::Invariant);
                }
                required_evidence.push(evidence);
            }
            entries.insert(
                operation.clone(),
                CatalogEntry {
                    operation,
                    status,
                    title: wire_entry.title,
                    mutation_class,
                    required_evidence,
                },
            );
        }
        Ok(Self {
            catalog_version: version::CATALOG_IDENTITY
                .rsplit_once('@')
                .map(|(_, tail)| tail.to_owned())
                .ok_or(CatalogError::Invariant)?,
            entries,
            digest: format!("sha256:{}", sha256_hex(bytes)),
        })
    }

    /// The catalog contract version (independent of the product release).
    pub fn catalog_version(&self) -> &str {
        &self.catalog_version
    }

    /// The exact `sha256:` digest of the embedded catalog bytes.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Look up one operation by discriminator.
    pub fn entry(&self, operation: &str) -> Option<&CatalogEntry> {
        self.entries.get(operation)
    }

    /// Every entry in canonical (operation byte) order.
    pub fn entries(&self) -> impl Iterator<Item = &CatalogEntry> {
        self.entries.values()
    }

    /// Number of registered operations.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the catalog is empty (never true for the embedded one).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogWire {
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    identity: String,
    closed: bool,
    operations: Vec<EntryWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    operation: String,
    status: String,
    title: String,
    #[serde(rename = "mutationClass")]
    mutation_class: String,
    #[serde(rename = "requiredEvidence")]
    required_evidence: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_is_sorted_unique_and_closed() {
        let catalog = Catalog::embedded().expect("embedded catalog validates");
        assert_eq!(catalog.catalog_version(), "0.8.0");
        assert!(catalog.len() >= 20);
        let mut previous: Option<&str> = None;
        for entry in catalog.entries() {
            if let Some(previous) = previous {
                assert!(previous < entry.operation(), "catalog order");
            }
            previous = Some(entry.operation());
        }
    }

    #[test]
    fn query_output_evidence_names_its_actual_domain() {
        let catalog = Catalog::embedded().expect("catalog");
        let evidence = catalog
            .entry("operation-output:set")
            .expect("output")
            .required_evidence();
        assert!(evidence.iter().any(|item| item == "query-return-review"));
        assert!(!evidence.iter().any(|item| item == "command-return-review"));
    }

    #[test]
    fn the_issue_operations_are_cataloged_exactly_once() {
        let catalog = Catalog::embedded().expect("embedded catalog validates");
        for operation in [
            "entity-field:add",
            "entity-field:remove",
            "entity-field:rename",
            "enum-case:add",
            "enum-case:change",
            "definition:add",
            "operation-input:set",
            "operation-output:set",
            "operation-error:set",
            "command-effect:add",
            "command-effect:remove",
            "binding:add",
            "ownership:set",
            "requirement:link",
            "requirement:unlink",
            "symbol:move",
            "module:move",
            "migration:apply",
        ] {
            let entry = catalog
                .entry(operation)
                .unwrap_or_else(|| panic!("{operation} missing from catalog"));
            assert_eq!(entry.operation(), operation);
        }
        assert_eq!(catalog.entry("entity-field:append"), None);
    }

    #[test]
    fn the_field_vertical_is_implemented_and_the_rest_is_explicit() {
        let catalog = Catalog::embedded().expect("embedded catalog validates");
        for operation in [
            "entity-field:add",
            "entity-field:remove",
            "entity-field:rename",
            "enum-case:add",
            "enum-case:change",
            "definition:add",
            "command-effect:add",
            "command-effect:remove",
            "operation-input:set",
            "operation-output:set",
            "query-effect:add",
            "query-effect:remove",
            "requirement:link",
            "requirement:unlink",
            "migration:apply",
        ] {
            assert_eq!(
                catalog.entry(operation).expect("entry").status(),
                OperationStatus::Implemented,
                "{operation}"
            );
        }
        assert_eq!(
            catalog.entry("binding:add").expect("entry").status(),
            OperationStatus::Unsupported
        );
        assert_eq!(
            catalog
                .entry("operation-error:set")
                .expect("entry")
                .status(),
            OperationStatus::Unsupported
        );
    }

    #[test]
    fn the_unsupported_set_is_exactly_the_documented_deferral_matrix() {
        // Regression (r5 review, finding 4): P1 family coverage is
        // explicitly scope-deferred — the remaining unsupported operations are
        // the documented deferral matrix of docs/change.md (increments I4
        // and I5 of docs/m8/issue-73-research.md). The catalog stays
        // honest by construction: an operation flips to `implemented`
        // exactly when its sealed executor lands, and this pin refuses a
        // silent drift in either direction (a new refusal without a
        // documented deferral, or an executor landing without flipping
        // its status).
        let catalog = Catalog::embedded().expect("embedded catalog validates");
        let deferred = [
            // Research I4: signature and effect verticals.
            "operation-error:set",
            // Research I5: bindings, moves, and ownership.
            "binding:add",
            "ownership:set",
            "symbol:move",
            "module:move",
        ];
        let mut unsupported = catalog
            .entries()
            .filter(|entry| entry.status() == OperationStatus::Unsupported)
            .map(|entry| entry.operation().to_owned())
            .collect::<Vec<_>>();
        unsupported.sort_unstable();
        let mut expected = deferred.to_vec();
        expected.sort_unstable();
        assert_eq!(unsupported, expected);
    }
}
