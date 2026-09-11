//! The hermetic conformance fixture project (issue #31).
//!
//! The suite always runs against its own embedded fixture project, never
//! against a user repository. The fixture carries one document per
//! fixture class of the issue: the minimal model IR (which also covers
//! the entity/command/query kinds), the invalid-references IR, and the
//! transaction/concurrency Scenario IR document. Canonical Lekalo and
//! OpenSpec homes exist so the immutability check has something real to
//! guard.
//!
//! Custody is fail-closed: the embedded scenario document is decoded
//! through the production Scenario IR module before any adapter runs,
//! and the fixture digests are pinned constants asserted by the test
//! suite.

use std::collections::BTreeMap;
use std::path::Path;

use super::check::{CheckId, CheckOutcome, CheckState};

/// The canonical IR fixture: the compiled typed IR of the accepted
/// full-kinds model fixture, byte-identical to its committed golden.
pub const IR_MINIMAL: &str =
    include_str!("../../../../tests/fixtures/adapter-conformance/inputs/ir-minimal.json");

/// The invalid-references fixture: the minimal IR with one reference
/// redirected to a symbol no definition declares.
pub const IR_INVALID_REFS: &str =
    include_str!("../../../../tests/fixtures/adapter-conformance/inputs/ir-invalid-refs.json");

/// The transaction/concurrency Scenario IR fixture: a concurrent focus
/// race whose replay must deduplicate through the idempotency key.
pub const SCENARIO_TXN: &str = include_str!(
    "../../../../tests/fixtures/adapter-conformance/inputs/scenario-txn-concurrency.json"
);

/// The canonical project marker of the fixture root.
pub const PROJECT_MARKER: &str =
    include_str!("../../../../tests/fixtures/adapter-conformance/project/lekalo/project.yaml");

/// The canonical OpenSpec marker whose immutability the suite guards.
pub const OPENSPEC_MARKER: &str = include_str!(
    "../../../../tests/fixtures/adapter-conformance/project/openspec/specs/conformance.md"
);

/// The logical IR path of the fixture input.
pub const IR_PATH: &str = ".lekalo/ir/minimal.json";
/// The logical path of the invalid-references input.
pub const IR_INVALID_PATH: &str = ".lekalo/ir/invalid-refs.json";
/// The logical path of the scenario input.
pub const SCENARIO_PATH: &str = ".lekalo/ir/scenario-txn-concurrency.json";

/// The fixture files, as logical path plus exact bytes.
pub const FILES: [(&str, &str); 5] = [
    ("lekalo/project.yaml", PROJECT_MARKER),
    ("openspec/specs/conformance.md", OPENSPEC_MARKER),
    (IR_PATH, IR_MINIMAL),
    (IR_INVALID_PATH, IR_INVALID_REFS),
    (SCENARIO_PATH, SCENARIO_TXN),
];

/// The observed fixture root: logical path to content digest, using the
/// protocol plan module's snapshot domain so `changed_paths` applies.
pub type Observation = BTreeMap<String, String>;

/// Materialize the fixture project under `root`, creating every parent
/// directory. Byte-exact writes; the root must already exist.
pub fn materialize(root: &Path) -> Result<(), std::io::Error> {
    for (path, bytes) in FILES {
        let target = root.join(logical_to_native(path));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, bytes)?;
    }
    Ok(())
}

/// Observe every file of the fixture root as a logical path to digest.
/// The fixture project carries no links, special entries, or empty
/// directories, so a plain sorted walk is exact for it.
pub fn observe(root: &Path) -> Result<Observation, std::io::Error> {
    let mut out = Observation::new();
    walk(root, root, &mut out)?;
    Ok(out)
}

/// The changed logical paths between two observations.
pub fn changed(before: &Observation, after: &Observation) -> Vec<String> {
    crate::target_protocol::plan::changed_paths(before, after)
}

/// Decode the scenario fixture through the production Scenario IR
/// module; a fixture that fails its own custody stops the run.
pub fn scenario_custody() -> Result<(), CheckOutcome> {
    let json: serde_json::Value = serde_json::from_str(SCENARIO_TXN).map_err(|_| {
        CheckOutcome::fail(
            CheckId::ScenarioNormalization,
            CheckId::ScenarioNormalization.class(),
            "fixture-json",
        )
    })?;
    crate::scenario::ScenarioIr::from_value(&json)
        .map(|_| ())
        .map_err(|_| CheckOutcome {
            id: CheckId::ScenarioNormalization,
            class: CheckId::ScenarioNormalization.class(),
            state: CheckState::Fail,
            detail: Some("fixture-custody"),
        })
}

fn walk(root: &Path, dir: &Path, out: &mut Observation) -> Result<(), std::io::Error> {
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out)?;
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let logical = if dir == root {
            name
        } else {
            let parent = dir
                .strip_prefix(root)
                .unwrap_or(dir)
                .to_string_lossy()
                .replace('\\', "/");
            format!("{parent}/{name}")
        };
        let bytes = std::fs::read(&path)?;
        let digest = crate::target_protocol::plan::sha256_hex(&bytes);
        out.insert(logical, digest);
    }
    Ok(())
}

fn logical_to_native(path: &str) -> std::path::PathBuf {
    path.split('/').collect::<std::path::PathBuf>()
}
