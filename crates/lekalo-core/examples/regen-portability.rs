//! Issue #117 golden regenerator: emits the committed portability
//! report goldens from the committed profile goldens. Run from the
//! repository root:
//!
//!     cargo run -p lekalo-core --example regen-portability -- .
//!
//! The byte-stable report and the canonical fixture bytes make the
//! output path-independent; the Node gate re-proves the canonical
//! form independently.

use lekalo_core::storage_engine_profile::{
    named_postgres_divergences, portability, StorageEngineProfile,
};

fn read_profile(root: &str, name: &str) -> StorageEngineProfile {
    let text = std::fs::read_to_string(format!(
        "{root}/tests/fixtures/storage-engine-profile/valid/{name}.json"
    ))
    .expect("profile golden");
    let value: serde_json::Value = serde_json::from_str(text.trim()).expect("json");
    StorageEngineProfile::from_value(&value).expect("profile")
}

fn main() {
    let root = std::env::args().nth(1).expect("repo root");
    let mysql = read_profile(&root, "mysql-8.0");
    let mariadb = read_profile(&root, "mariadb-10.11");
    // mysql -> mariadb: the engine-family divergences in capability
    // form (sequences, shared-lock spelling).
    let report = portability(&mysql, &mariadb);
    let bytes = serde_json::to_string_pretty(&report).expect("json");
    std::fs::write(
        format!("{root}/tests/fixtures/storage-engine-profile/portability/mysql-to-mariadb.json"),
        format!("{bytes}\n"),
    )
    .expect("write");
    println!("wrote mysql-to-mariadb.json");
    let _ = named_postgres_divergences(report);
}
