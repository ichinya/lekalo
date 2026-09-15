//! Issue #42 library tests for the binding registry: proposal
//! derivation with full candidate sets, the ambiguity gate, single and
//! batch confirmation with provenance preservation, the target/profile
//! set-once rule, and the staleness audit over symbols and native test
//! bindings. Every test runs against a sandbox copy of the hermetic
//! bindings fixture; the committed fixture is never mutated.

use lekalo_core::observed;
use lekalo_core::observed::bindings;
use lekalo_core::observed::types::{BindingState, BindingStatus};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const APP: &str = "tests/fixtures/bindings/app";
const APP_SCAN: &str = "tests/fixtures/bindings/scans/app.scan.v1_1_0.json";
const AMBIGUOUS_SCAN: &str = "tests/fixtures/bindings/scans/app-ambiguous.scan.v1_1_0.json";

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
/// and 8.3 TEMP aliases the selection policy intentionally refuses.
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
        let entry = entry.expect("source entry");
        let target_path = target.join(entry.file_name());
        if entry.file_type().expect("entry type").is_dir() {
            copy_dir(&entry.path(), &target_path);
        } else {
            std::fs::copy(entry.path(), &target_path).expect("copy file");
        }
    }
}

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("lekalo-bindings-core-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        copy_dir(&workspace_root().join(APP), &dir.join("proj"));
        Self {
            root: alias_free(&dir.join("proj")),
        }
    }

    /// Load the observed context with the process working directory
    /// inside the sandbox: the selection policy refuses absolute and
    /// alias selectors, and the fixture root carries the marker files
    /// `find_root` needs.
    fn context(&self) -> observed::ObservedContext {
        let _guard = CWD_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let original = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(&self.root).expect("enter sandbox");
        let context = observed::context(&lekalo_core::loader::LoadSelection { project: None });
        std::env::set_current_dir(original).expect("restore cwd");
        context.expect("observed context loads")
    }

    fn scan_bytes(&self, fixture: &str) -> Vec<u8> {
        std::fs::read(workspace_root().join(fixture)).expect("scan fixture bytes")
    }

    fn merge(&self, fixture: &str) -> observed::UpdateReceipt {
        let context = self.context();
        observed::update_index(&context, &self.scan_bytes(fixture)).expect("scan merges")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn the_scan_document_merges_with_target_profile_and_test_bindings() {
    let sandbox = Sandbox::new("merge");
    let receipt = sandbox.merge(APP_SCAN);
    assert_eq!(receipt.symbols, 6);
    assert_eq!(receipt.inferred, 6);
    assert_eq!(receipt.endpoints, 0);
    let context = sandbox.context();
    let index = observed::load_index(&context)
        .expect("index loads")
        .expect("present");
    assert_eq!(index.target.as_deref(), Some("node-typescript"));
    assert_eq!(index.profile.as_deref(), Some("default"));
    assert_eq!(index.test_bindings.len(), 3);
    // The frozen 1.0.0 spellings keep merging through the same seam.
    let legacy = Sandbox::new("merge-legacy");
    let receipt = legacy.merge("tests/fixtures/observed/task-domain/scans/initial.json");
    assert_eq!(receipt.symbols, 7);
}

#[test]
fn proposals_derive_only_from_inferred_bindings_and_are_deterministic() {
    let sandbox = Sandbox::new("propose");
    sandbox.merge(APP_SCAN);
    let context = sandbox.context();

    let first = bindings::propose(&context).expect("propose");
    let second = bindings::propose(&context).expect("propose");
    assert_eq!(first.proposals, second.proposals, "two runs agree");
    assert_eq!(first.proposals.len(), 6);
    assert_eq!(first.ambiguous, 0);
    assert!(first
        .proposals
        .iter()
        .all(|proposal| proposal.relation == "implements"));
    assert!(first
        .proposals
        .iter()
        .all(|proposal| proposal.proposal.starts_with("prop-")));

    // Confirming one binding removes exactly its proposal: user-owned
    // facts never generate proposals again.
    let proposal = first.proposals[0].clone();
    bindings::confirm(&context, &proposal.proposal, None).expect("confirms");
    let after = bindings::propose(&context).expect("propose");
    assert_eq!(after.proposals.len(), 5);
    assert!(!after.proposals.iter().any(|p| p.symbol == proposal.symbol));
}

#[test]
fn ambiguous_candidates_are_never_resolved_by_first_match() {
    let sandbox = Sandbox::new("ambiguous");
    sandbox.merge(AMBIGUOUS_SCAN);
    let context = sandbox.context();
    let receipt = bindings::propose(&context).expect("propose");
    let proposal = receipt
        .proposals
        .iter()
        .find(|proposal| proposal.symbol == "taskboard.focus_task")
        .expect("focus task proposal");
    assert!(proposal.ambiguous);
    assert_eq!(proposal.candidates.len(), 2);
    // The recorded binding carries the candidate set and no location:
    // nothing was picked, not even deterministically.
    let index = observed::load_index(&context)
        .expect("index loads")
        .expect("present");
    let record = index.symbol("taskboard.focus_task").expect("recorded");
    assert!(record.location.is_none());
    assert!(record.fingerprint.is_none());
    assert_eq!(record.candidates.len(), 2);
    assert_eq!(record.state, BindingState::Unknown);

    // Confirming without naming a candidate refuses with the registered
    // rule; a candidate outside the set refuses the same way.
    let error =
        bindings::confirm(&context, &proposal.proposal, None).expect_err("ambiguous refuses");
    assert!(error
        .as_slice()
        .iter()
        .any(|item| item.id() == "bindings.ambiguous"));
    let error = bindings::confirm(&context, &proposal.proposal, Some("src/nowhere.ts#x"))
        .expect_err("unknown candidate refuses");
    assert!(error
        .as_slice()
        .iter()
        .any(|item| item.id() == "bindings.ambiguous"));

    // Naming the second candidate resolves to exactly that one: the
    // choice is the user's, never the first row of the list.
    let chosen = bindings::confirm(&context, &proposal.proposal, Some("src/tasks.ts#focusTask"))
        .expect("confirms");
    assert_eq!(chosen.native, "src/tasks.ts#focusTask");
    let index = observed::load_index(&context)
        .expect("index loads")
        .expect("present");
    let record = index.symbol("taskboard.focus_task").expect("recorded");
    assert_eq!(record.status, BindingStatus::Confirmed);
    assert_eq!(
        record.location.as_ref().expect("resolved").path,
        "src/tasks.ts"
    );
}

#[test]
fn confirmation_preserves_provenance_and_batch_never_drifts() {
    let sandbox = Sandbox::new("batch");
    sandbox.merge(APP_SCAN);
    let context = sandbox.context();
    let before = bindings::propose(&context).expect("propose");

    // Batch preview plans every unambiguous proposal.
    let preview = bindings::confirm_batch(&context, None).expect("preview");
    assert_eq!(preview.phase, "plan");
    assert_eq!(preview.entries.len(), 6);
    let plan_id = preview.plan.clone().expect("plan id");

    // A wrong plan id refuses; the exact previewed id applies.
    let error =
        bindings::confirm_batch(&context, Some("plan-deadbeef")).expect_err("wrong plan refuses");
    assert!(error
        .as_slice()
        .iter()
        .any(|item| item.id() == "bindings.plan-mismatch"));
    let applied = bindings::confirm_batch(&context, Some(&plan_id)).expect("applies");
    assert_eq!(applied.phase, "apply");
    assert_eq!(applied.confirmed.len(), 6);

    // Provenance survives: origin, adapter, and revision stay as the
    // scan recorded them; only the status changed.
    let index = observed::load_index(&context)
        .expect("index loads")
        .expect("present");
    for record in &index.symbols {
        assert_eq!(record.status, BindingStatus::Confirmed);
        assert_eq!(record.provenance.adapter, before.adapter);
        assert_eq!(record.provenance.revision, before.revision);
        assert!(record
            .history
            .iter()
            .any(|entry| entry.event == lekalo_core::observed::types::HistoryEvent::Confirmed));
    }

    // A replay of the same plan refuses: nothing is confirmed twice.
    assert!(bindings::confirm_batch(&context, Some(&plan_id)).is_err());
}

#[test]
fn unknown_proposal_ids_refuse_with_the_registered_rule() {
    let sandbox = Sandbox::new("unknown");
    sandbox.merge(APP_SCAN);
    let context = sandbox.context();
    let error = bindings::confirm(
        &context,
        "prop-0000000000000000000000000000000000000000000000000000000000000000",
        None,
    )
    .expect_err("unknown proposal refuses");
    assert!(error
        .as_slice()
        .iter()
        .any(|item| item.id() == "bindings.proposal-unknown"));
}

#[test]
fn the_target_is_set_once_and_the_audit_re_fingerprints_tests() {
    let sandbox = Sandbox::new("audit");
    sandbox.merge(APP_SCAN);
    let context = sandbox.context();

    // A different target for the same registry refuses.
    let ambiguous = sandbox.scan_bytes(AMBIGUOUS_SCAN);
    let other_target = String::from_utf8(ambiguous)
        .expect("utf8")
        .replace("node-typescript", "php-laravel");
    let error = observed::update_index(&context, other_target.as_bytes())
        .expect_err("target mismatch refuses");
    assert!(error
        .as_slice()
        .iter()
        .any(|item| item.id() == "observed.scan-invalid"));

    // A clean audit passes and covers the test bindings.
    let receipt = bindings::audit(&context).expect("clean audit");
    assert_eq!(receipt.symbols, 6);
    assert_eq!(receipt.current, 6);
    assert_eq!(receipt.test_bindings, 3);
    assert_eq!(receipt.tests_current, 3);

    // Drifting a scanned source file fails the gate.
    let tasks = sandbox.root.join("src/tasks.ts");
    let original = std::fs::read(&tasks).expect("read");
    std::fs::write(&tasks, b"export function drifted() {}\n").expect("drift");
    assert!(bindings::audit(&context).is_err());
    std::fs::write(&tasks, original).expect("restore");

    // Drifting a test file fails the gate too (the `verifies` rows are
    // first-class bindings).
    let tests = sandbox.root.join("src/tasks.test.ts");
    std::fs::write(&tests, b"// drifted test\n").expect("drift tests");
    assert!(bindings::audit(&context).is_err());
}
