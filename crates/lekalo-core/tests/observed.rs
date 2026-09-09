//! Issue #39 library tests for the observed mode: adapter scan
//! normalization, the inferred/confirmed/explicit distinction, stable-key
//! move survival, stale detection, the confirmation gate, and the
//! planned/confirmed promotion into the canonical model. Every test runs
//! against a sandbox copy of the hermetic task-domain fixture; the
//! committed fixture is never mutated.

use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::observed;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const TASK_DOMAIN: &str = "tests/fixtures/observed/task-domain";
const INITIAL_SCAN: &str = "tests/fixtures/observed/task-domain/scans/initial.json";
const MOVED_SCAN: &str = "tests/fixtures/observed/task-domain/scans/moved.json";

/// Serializes tests that change the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

/// The canonicalized, alias-free spelling of `path`, including junction
/// and 8.3 TEMP aliases. Production selection guards intentionally
/// reject those aliases; strips the `\\?\` verbatim prefix `canonicalize`
/// produces on Windows drive paths.
fn alias_free(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("sandbox path must exist");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        // `\\?\C:\...` -> `C:\...`; UNC (`\\?\UNC\...`) stays verbatim.
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => canonical,
    }
    #[cfg(not(windows))]
    canonical
}

fn copy_dir(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read source") {
        let entry = entry.expect("reference entry");
        let target_path = target.join(entry.file_name());
        if entry.file_type().expect("entry type").is_dir() {
            copy_dir(&entry.path(), &target_path);
        } else {
            std::fs::copy(entry.path(), &target_path).expect("copy reference file");
        }
    }
}

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("lekalo-observed-core-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _guard = CWD_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let original = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(workspace_root()).expect("enter workspace root");
        copy_dir(Path::new(TASK_DOMAIN), &dir);
        std::env::set_current_dir(original).expect("restore cwd");
        Self {
            // Hosted Windows %TEMP% can arrive as an 8.3 or junction
            // alias of the physical tree and the selection policy denies
            // those spellings; build the sandbox from the resolved
            // spelling so the test exercises observed behavior, never
            // the alias refusal.
            root: alias_free(&dir),
        }
    }

    fn context(&self) -> lekalo_core::observed::ObservedContext {
        let _guard = CWD_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let original = std::env::current_dir().expect("current dir");
        // Selection policy denies absolute selectors; the sandbox root is
        // itself the project (find_root from the invocation directory).
        std::env::set_current_dir(&self.root).expect("enter sandbox");
        let context = observed::context(&LoadSelection { project: None });
        std::env::set_current_dir(original).expect("restore cwd");
        context.expect("observed context loads")
    }

    fn scan_bytes(&self, fixture: &str) -> Vec<u8> {
        std::fs::read(workspace_root().join(fixture)).expect("scan fixture bytes")
    }

    fn index(&self) -> observed::ObservedIndex {
        observed::load_index(&self.context())
            .expect("index readable")
            .expect("index present")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn update_records_the_scan_as_inferred_and_is_byte_stable() {
    let sandbox = Sandbox::new("fresh");
    let ctx = sandbox.context();
    let receipt = observed::update_index(&ctx, &sandbox.scan_bytes(INITIAL_SCAN)).expect("update");
    assert_eq!(receipt.status, "valid");
    assert_eq!(receipt.symbols, 7);
    assert_eq!(receipt.inferred, 7);
    assert_eq!(receipt.explicit, 0);
    assert_eq!(receipt.confirmed, 0);

    // Determinism: a second identical scan produces identical bytes.
    observed::update_index(&ctx, &sandbox.scan_bytes(INITIAL_SCAN)).expect("re-update");
    let first = std::fs::read(ctx.root.join(".lekalo/import/observed/index.json")).expect("bytes");
    observed::update_index(&ctx, &sandbox.scan_bytes(INITIAL_SCAN)).expect("re-update");
    let second = std::fs::read(ctx.root.join(".lekalo/import/observed/index.json")).expect("bytes");
    assert_eq!(first, second, "canonical index bytes are stable");
}

#[test]
fn scan_without_declared_module_is_refused() {
    let sandbox = Sandbox::new("module");
    let ctx = sandbox.context();
    let mut scan: serde_json::Value =
        serde_json::from_slice(&sandbox.scan_bytes(INITIAL_SCAN)).expect("scan json");
    scan["symbols"][0]["id"] = serde_json::json!("ghost.unknown");
    let bytes = serde_json::to_vec(&scan).expect("wire");
    let error = observed::update_index(&ctx, &bytes).expect_err("refused");
    let rendered = serde_json::to_string(error.as_slice()).expect("diagnostic wire");
    assert!(rendered.contains("observed.unknown-module"), "{rendered}");
}

#[test]
fn inferred_and_confirmed_facts_are_distinguishable_and_never_confused() {
    let sandbox = Sandbox::new("confirm");
    let ctx = sandbox.context();
    observed::update_index(&ctx, &sandbox.scan_bytes(INITIAL_SCAN)).expect("update");

    // Confirming flips exactly one binding; a second confirm refuses.
    observed::confirm_binding(&ctx, "taskboard.create_task").expect("confirm");
    let index = sandbox.index();
    let confirmed = index.symbol("taskboard.create_task").expect("record");
    assert_eq!(confirmed.status, observed::BindingStatus::Confirmed);
    let still_inferred = index.symbol("taskboard.list_tasks").expect("record");
    assert_eq!(still_inferred.status, observed::BindingStatus::Inferred);

    let error = observed::confirm_binding(&ctx, "taskboard.create_task").expect_err("refused");
    let rendered = serde_json::to_string(error.as_slice()).expect("diagnostic wire");
    assert!(rendered.contains("observed.confirm-refused"), "{rendered}");

    // A re-scan never downgrades the confirmed fact.
    observed::update_index(&ctx, &sandbox.scan_bytes(INITIAL_SCAN)).expect("re-update");
    let index = sandbox.index();
    assert_eq!(
        index
            .symbol("taskboard.create_task")
            .expect("record")
            .status,
        observed::BindingStatus::Confirmed
    );
}

#[test]
fn explicit_binding_survives_a_move_through_stable_key_resolution() {
    let sandbox = Sandbox::new("move");
    let ctx = sandbox.context();
    observed::update_index(&ctx, &sandbox.scan_bytes(INITIAL_SCAN)).expect("update");

    // Physically move the source tree (the file move under the binding).
    std::fs::create_dir_all(sandbox.root.join("src/todo")).expect("move target");
    std::fs::rename(
        sandbox.root.join("src/tasks.ts"),
        sandbox.root.join("src/todo/tasks.ts"),
    )
    .expect("move tasks");
    std::fs::rename(
        sandbox.root.join("src/ids.ts"),
        sandbox.root.join("src/todo/ids.ts"),
    )
    .expect("move ids");

    // The moved scan reports the same stable keys under the new path:
    // the binding survives with an explicit move history entry.
    let receipt = observed::update_index(&ctx, &sandbox.scan_bytes(MOVED_SCAN)).expect("moved");
    assert!(receipt.moved.contains(&"taskboard.create_task".to_owned()));
    let index = sandbox.index();
    let binding = index.symbol("taskboard.create_task").expect("record");
    assert_eq!(binding.state, observed::BindingState::Current);
    assert_eq!(
        binding.location.as_ref().expect("location").path,
        "src/todo/tasks.ts"
    );
    let moved_event = binding
        .history
        .iter()
        .any(|entry| entry.event == observed::HistoryEvent::Moved);
    assert!(moved_event, "the move is history, never a break");

    // The dropped symbol is stale, never silent.
    assert!(receipt
        .staled
        .contains(&"taskboard.task_focused".to_owned()));
    let dropped = index.symbol("taskboard.task_focused").expect("record");
    assert_eq!(dropped.state, observed::BindingState::Stale);
}

#[test]
fn the_staleness_gate_fails_on_source_drift_and_missing_source() {
    let sandbox = Sandbox::new("stale");
    let ctx = sandbox.context();
    observed::update_index(&ctx, &sandbox.scan_bytes(INITIAL_SCAN)).expect("update");

    // Drift: rewrite one bound source file.
    let path = sandbox.root.join("src/tasks.ts");
    let mut text = std::fs::read_to_string(&path).expect("source");
    text.push_str("\n// drift\n");
    std::fs::write(&path, text).expect("rewrite source");

    let error = observed::staleness(&ctx).expect_err("stale gate");
    let rendered = serde_json::to_string(error.as_slice()).expect("diagnostic wire");
    assert!(rendered.contains("observed.stale-binding"), "{rendered}");
    assert!(rendered.contains("fingerprint-mismatch"), "{rendered}");
}

#[test]
fn promotion_refuses_inferred_facts_and_applies_only_confirmed_plans() {
    let sandbox = Sandbox::new("promote");
    let ctx = sandbox.context();
    observed::update_index(&ctx, &sandbox.scan_bytes(INITIAL_SCAN)).expect("update");

    // An inferred fact never promotes: unknown evidence refuses.
    let selection = observed::PromotionSelection::Symbol("taskboard.task_id".to_owned());
    let error = observed::promote::plan(&ctx, &selection).expect_err("refused");
    let rendered = serde_json::to_string(error.as_slice()).expect("diagnostic wire");
    assert!(
        rendered.contains("observed.promotion-refused"),
        "{rendered}"
    );

    // Confirm, plan, apply.
    observed::confirm_binding(&ctx, "taskboard.task_id").expect("confirm");
    observed::confirm_binding(&ctx, "taskboard.task_state").expect("confirm");
    observed::confirm_binding(&ctx, "taskboard.task").expect("confirm");
    observed::confirm_binding(&ctx, "taskboard.list_tasks").expect("confirm");
    observed::confirm_binding(&ctx, "taskboard.create_task").expect("confirm");
    let plan = observed::promote::plan(&ctx, &selection).expect("plan");
    assert_eq!(plan.symbols, vec!["taskboard.task_id".to_owned()]);

    let wrong = "sha256:".to_owned() + &"a".repeat(64);
    let error = observed::promote::apply(&ctx, &selection, &wrong).expect_err("plan id is pinned");
    let rendered = serde_json::to_string(error.as_slice()).expect("diagnostic wire");
    assert!(
        rendered.contains("observed.promotion-plan-mismatch"),
        "{rendered}"
    );

    observed::promote::apply(&ctx, &selection, &plan.plan).expect("apply");
    let index = sandbox.index();
    let promoted = index.symbol("taskboard.task_id").expect("record");
    assert!(promoted.promoted);
    assert!(promoted.promotion.is_some(), "the adoption receipt is kept");

    // The promoted definition is canonical Model: the loader accepts it.
    let _guard = CWD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(&sandbox.root).expect("enter sandbox");
    let model = normalize_model(&LoadSelection { project: None });
    std::env::set_current_dir(original).expect("restore cwd");
    let model = model.expect("the promoted project loads");
    assert!(
        model
            .definitions
            .iter()
            .any(|definition| definition.id == "taskboard.task_id"),
        "the promoted definition is canonical"
    );
}

#[test]
fn observed_cards_state_their_own_completeness() {
    let sandbox = Sandbox::new("cards");
    let ctx = sandbox.context();
    observed::update_index(&ctx, &sandbox.scan_bytes(INITIAL_SCAN)).expect("update");
    let index = sandbox.index();

    // An inferred card is incomplete by construction.
    let inferred = observed::view::inspect_card(&index, "taskboard.list_tasks").expect("card");
    assert_eq!(
        inferred.completeness,
        observed::view::Completeness::Incomplete
    );

    // An observed impact always reports the recorded graph as incomplete.
    let impact = observed::view::impact_card(&index, "taskboard.task").expect("impact");
    assert_eq!(
        impact.completeness,
        observed::view::Completeness::Incomplete
    );
    assert!(impact
        .dependents
        .iter()
        .any(|dependent| dependent.from == "taskboard.list_tasks"));
}

#[test]
fn the_generated_manifest_can_never_claim_an_observed_file() {
    // The artifact path grammar is rooted at the managed root, so an
    // observed source path cannot even parse as a manifest entry.
    let claimed = lekalo_core::artifacts::GeneratedPath::claim("src/tasks.ts");
    assert!(claimed.is_none(), "observed files are unclaimable");
    let index_claim =
        lekalo_core::artifacts::GeneratedPath::claim(".lekalo/import/observed/index.json");
    assert!(index_claim.is_none(), "the observed index is unclaimable");
    let generated = lekalo_core::artifacts::GeneratedPath::claim(".lekalo/generated/app.ts");
    assert!(generated.is_some(), "generated paths parse");
}
