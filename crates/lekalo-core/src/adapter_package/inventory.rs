//! The adapter inventory: the governed `.lekalo/adapters/**` store
//! document (issue #32).
//!
//! `inventory.json` is the single authority over installed adapter
//! packages: one row per installed `{id, version, digest}`, the
//! `selected` pin per id, the assigned trust level, and the install
//! provenance. Package bytes under `.lekalo/adapters/packages/**` are
//! immutable; the selected pin is the **only** mutable field — update
//! and rollback are pin repoints, never in-place edits.
//!
//! Quarantine custody is recorded here as rows with
//! `quarantined: true`: staged bytes under
//! `.lekalo/adapters/quarantine/**` are excluded from selection and
//! execution until an explicitly confirmed install plan promotes them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::manifest::ManifestDocument;
use super::trust::TrustLevel;
use super::types::PackageFailure;
use super::version::{INVENTORY_IDENTITY, INVENTORY_SCHEMA_VERSION};

/// The runtime home of the store document.
pub const INVENTORY_FILE: &str = ".lekalo/adapters/inventory.json";

/// One installed (or quarantined) package row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryRow {
    /// The adapter id.
    pub id: String,
    /// The exact installed version.
    pub version: String,
    /// The package digest over the verified byte set.
    pub digest: String,
    /// The manifest identity digest.
    #[serde(rename = "manifestDigest")]
    pub manifest_digest: String,
    /// The assigned trust level at install time.
    pub trust: String,
    /// The closed source coordinate token the package came from.
    pub source: String,
    /// The confirmed install plan id, when the package arrived through
    /// a plan (every non-synthesized install does).
    #[serde(rename = "installPlanId")]
    pub install_plan_id: Option<String>,
    /// Whether this row is the selected pin of its id.
    pub selected: bool,
    /// Whether the bytes are still in quarantine custody.
    pub quarantined: bool,
}

/// The parsed inventory document.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Inventory {
    rows: Vec<InventoryRow>,
}

impl Inventory {
    /// The exact wire document this inventory serializes to.
    pub fn to_wire(&self) -> Result<Vec<u8>, PackageFailure> {
        #[derive(Serialize)]
        struct Wire<'a> {
            #[serde(rename = "schemaVersion")]
            schema_version: &'static str,
            identity: &'static str,
            packages: &'a [InventoryRow],
        }
        let wire = Wire {
            schema_version: INVENTORY_SCHEMA_VERSION,
            identity: INVENTORY_IDENTITY,
            packages: &self.rows,
        };
        let mut bytes = serde_json::to_vec(&wire).map_err(|_| store_failure())?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Parse the inventory from bytes (strict: unknown members refuse).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PackageFailure> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(rename = "schemaVersion")]
            schema_version: String,
            identity: String,
            #[serde(default)]
            packages: Vec<InventoryRow>,
        }
        let wire: Wire = serde_json::from_slice(bytes).map_err(|_| store_failure())?;
        if wire.schema_version != INVENTORY_SCHEMA_VERSION || wire.identity != INVENTORY_IDENTITY {
            return Err(store_failure());
        }
        Ok(Self {
            rows: wire.packages,
        })
    }

    /// Load the inventory under a project root; a missing file is an
    /// empty store.
    pub fn load(root: &Path) -> Result<Self, PackageFailure> {
        let path = inventory_path(root);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(_) => return Err(store_failure()),
        };
        Self::from_bytes(&bytes)
    }

    /// Write the inventory under a project root.
    pub fn store(&self, root: &Path) -> Result<(), PackageFailure> {
        let path = inventory_path(root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| store_failure())?;
        }
        std::fs::write(&path, self.to_wire()?).map_err(|_| store_failure())
    }

    /// Every row, in stored order (sorted by id, version, digest).
    pub fn rows(&self) -> &[InventoryRow] {
        &self.rows
    }

    /// Whether any selected, non-quarantined row exists for the id.
    pub fn selected(&self, id: &str) -> Option<&InventoryRow> {
        self.rows
            .iter()
            .find(|row| row.id == id && row.selected && !row.quarantined)
    }

    /// Whether the exact package (id, version, digest) is installed and
    /// promoted (not quarantined).
    pub fn is_installed(&self, id: &str, version: &str, digest: &str) -> bool {
        self.rows.iter().any(|row| {
            row.id == id && row.version == version && row.digest == digest && !row.quarantined
        })
    }

    /// Insert or replace one row; rows are kept sorted by
    /// (id, version, digest) with no exact duplicates.
    pub fn upsert(&mut self, row: InventoryRow) {
        self.rows.retain(|existing| {
            existing.id != row.id
                || existing.version != row.version
                || existing.digest != row.digest
        });
        self.rows.push(row);
        self.rows.sort_by(|left, right| {
            (&left.id, &left.version, &left.digest).cmp(&(&right.id, &right.version, &right.digest))
        });
    }

    /// Repoint the selected pin of one id: exactly one row of the id is
    /// selected afterwards, and it must be an installed (promoted) row.
    /// This is the only mutation update/rollback may perform on bytes.
    pub fn select(&mut self, id: &str, version: &str, digest: &str) -> Result<(), PackageFailure> {
        let mut found = false;
        for row in &mut self.rows {
            if row.id == id {
                let matches = row.version == version && row.digest == digest && !row.quarantined;
                row.selected = matches;
                found |= matches;
            }
        }
        if found {
            Ok(())
        } else {
            Err(PackageFailure::SourceChanged)
        }
    }

    /// Build the row for one verified manifest, assigned trust, and
    /// confirmed plan id.
    pub fn row_for(
        manifest: &ManifestDocument,
        trust: TrustLevel,
        install_plan_id: Option<String>,
        quarantined: bool,
    ) -> InventoryRow {
        InventoryRow {
            id: manifest.adapter_id().to_owned(),
            version: manifest.adapter_version().to_string(),
            digest: manifest.package_digest().as_str().to_owned(),
            manifest_digest: manifest.digest().as_str().to_owned(),
            trust: trust.as_str().to_owned(),
            source: manifest.source_coordinate().to_owned(),
            install_plan_id,
            selected: false,
            quarantined,
        }
    }
}

fn store_failure() -> PackageFailure {
    PackageFailure::RecoveryRequired {
        stage: "inventory".to_owned(),
    }
}

/// The inventory document path under a project root.
pub fn inventory_path(root: &Path) -> PathBuf {
    root.join(INVENTORY_FILE.replace('/', std::path::MAIN_SEPARATOR_STR))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, version: &str, selected: bool, quarantined: bool) -> InventoryRow {
        InventoryRow {
            id: id.to_owned(),
            version: version.to_owned(),
            digest: format!("sha256:{}", "11".repeat(32)),
            manifest_digest: format!("sha256:{}", "22".repeat(32)),
            trust: "community".to_owned(),
            source: "release:ch/pkg".to_owned(),
            install_plan_id: Some(format!("sha256:{}", "33".repeat(32))),
            selected,
            quarantined,
        }
    }

    #[test]
    fn wire_round_trip_is_strict() {
        let mut inventory = Inventory::default();
        inventory.upsert(row("a", "1.0.0", false, false));
        inventory.upsert(row("b", "0.9.0", true, false));
        let bytes = inventory.to_wire().expect("wire");
        let parsed = Inventory::from_bytes(&bytes).expect("parses");
        assert_eq!(parsed, inventory);
        // Unknown members refuse.
        let mut tampered = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        tampered["extra"] = serde_json::json!(1);
        let bytes = serde_json::to_vec(&tampered).unwrap();
        assert!(Inventory::from_bytes(&bytes).is_err());
        // Wrong identity refuses.
        let mut tampered =
            serde_json::from_slice::<serde_json::Value>(&inventory.to_wire().unwrap()).unwrap();
        tampered["identity"] = serde_json::json!("dev.lekalo.adapter-inventory@9.9.9");
        let bytes = serde_json::to_vec(&tampered).unwrap();
        assert!(Inventory::from_bytes(&bytes).is_err());
    }

    #[test]
    fn select_is_a_pin_repoint_and_refuses_unknown_or_quarantined() {
        let mut inventory = Inventory::default();
        inventory.upsert(row("a", "1.0.0", true, false));
        inventory.upsert(row("a", "2.0.0", false, false));
        inventory
            .select("a", "2.0.0", &format!("sha256:{}", "11".repeat(32)))
            .expect("repoint");
        let selected = inventory.selected("a").expect("one selected");
        assert_eq!(selected.version, "2.0.0");
        assert_eq!(
            inventory
                .rows()
                .iter()
                .filter(|row| row.id == "a" && row.selected)
                .count(),
            1
        );
        // A quarantined row can never be selected.
        inventory.upsert(row("a", "3.0.0", false, true));
        assert!(inventory
            .select("a", "3.0.0", &format!("sha256:{}", "11".repeat(32)))
            .is_err());
        // Unknown versions refuse.
        assert!(inventory.select("a", "9.9.9", "sha256:00").is_err());
    }

    #[test]
    fn quarantined_rows_are_never_installed() {
        let mut inventory = Inventory::default();
        inventory.upsert(row("q", "1.0.0", false, true));
        assert!(!inventory.is_installed("q", "1.0.0", &format!("sha256:{}", "11".repeat(32))));
        inventory.upsert(row("q2", "1.0.0", false, false));
        assert!(inventory.is_installed("q2", "1.0.0", &format!("sha256:{}", "11".repeat(32))));
    }

    #[test]
    fn load_missing_is_empty_and_store_round_trips() {
        let root = std::env::temp_dir().join(format!("lekalo-ap-inv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let inventory = Inventory::load(&root).expect("empty");
        assert!(inventory.rows().is_empty());
        let mut inventory = inventory;
        inventory.upsert(row("x", "1.0.0", true, false));
        inventory.store(&root).expect("store");
        let reloaded = Inventory::load(&root).expect("reload");
        assert_eq!(reloaded, inventory);
        assert_eq!(reloaded.selected("x").expect("selected").version, "1.0.0");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rows_stay_sorted_and_deduplicated() {
        let mut inventory = Inventory::default();
        inventory.upsert(row("b", "1.0.0", false, false));
        inventory.upsert(row("a", "2.0.0", false, false));
        inventory.upsert(row("a", "1.0.0", false, false));
        inventory.upsert(row("a", "1.0.0", false, false));
        let ids: Vec<&str> = inventory.rows().iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "a", "b"]);
        let versions: Vec<&str> = inventory
            .rows()
            .iter()
            .filter(|row| row.id == "a")
            .map(|row| row.version.as_str())
            .collect();
        assert_eq!(versions, vec!["1.0.0", "2.0.0"]);
    }
}
