//! Issue #51 worklog-changelog pilot composition tests: the closed wire,
//! the pure join against the pinned compilation, and diagnostic
//! normalization.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lekalo_core::loader::LoadSelection;
use lekalo_core::worklog_changelog::{self, Composition};

const PILOT_JSON: &str = "tests/fixtures/pilot/worklog-changelog/attachments/pilot.json";
const MODEL_PROJECT: &str = "tests/fixtures/pilot/worklog-changelog/model";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

/// The process cwd is shared state; composition joins run one at a
/// time with the workspace root as the project-relative base.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn with_workspace_cwd<T>(step: impl FnOnce() -> T) -> T {
    let guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(step));
    std::env::set_current_dir(original).expect("restore cwd");
    drop(guard);
    outcome.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

fn compilation() -> lekalo_core::ir::Compilation {
    let selection = LoadSelection {
        project: Some(MODEL_PROJECT.to_owned()),
    };
    lekalo_core::cache::load_compiled(&selection, true)
        .map(|(_, compilation)| compilation)
        .expect("fixture compiles")
}

fn composition_bytes() -> Vec<u8> {
    std::fs::read(PILOT_JSON).expect("pilot composition fixture present")
}

#[test]
fn pilot_document_joins_the_pinned_compilation() {
    with_workspace_cwd(|| {
        let compilation = compilation();
        let current = format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(compilation.project.to_canonical_json().as_bytes())
        );
        let composition: Composition =
            worklog_changelog::parse(&composition_bytes()).expect("pilot document parses");
        println!("pinned ir  : {}", composition.ir_digest());
        println!("current ir : {current}");
        println!(
            "project    : {} vs {}",
            composition.project_id(),
            compilation
                .project
                .project
                .as_ref()
                .map(|project| project.id.as_str())
                .unwrap_or_default()
        );
        composition
            .validate(&compilation.project)
            .expect("composition joins the pinned compilation");
    });
}

/// The committed pilot scenario corpus: every document decodes through
/// the production Scenario IR module and its committed bytes are the
/// canonical form.
#[test]
fn pilot_scenario_corpus_decodes_and_canonicalizes() {
    use lekalo_core::scenario::ScenarioIr;
    let dir = workspace_root().join("tests/fixtures/pilot/worklog-changelog/scenarios");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("scenario corpus present") {
        let path = entry.expect("scenario entry").path();
        let bytes = std::fs::read(&path).expect("scenario bytes");
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).expect("scenario JSON parses");
        let ir = ScenarioIr::from_value(&value)
            .unwrap_or_else(|set| panic!("{}: rejected: {set:?}", path.display()));
        let text = std::str::from_utf8(&bytes).expect("utf8");
        let canonical = ir
            .canonical_bytes()
            .unwrap_or_else(|set| panic!("{}: export refused: {set:?}", path.display()));
        assert_eq!(canonical, text, "{}: canonical bytes diverge", path.display());
        checked += 1;
    }
    assert!(checked >= 12, "the corpus carries the full scenario set");
}
