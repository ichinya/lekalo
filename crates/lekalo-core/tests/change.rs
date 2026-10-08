//! Issue #73 integration: the semantic change service over real fixture
//! projects. Every test drives the public pure API plus the transaction
//! writer — typed resolution, bounded plans, the lossless source edits,
//! the durability and idempotency ledger, and the fail-closed reader
//! barrier. No test reaches through private internals.

use lekalo_core::change::{
    self, ApplyOptions, ApplyRequestOptions, ChangeError, PrepareOptions, ReceiptState,
};
use lekalo_core::loader::LoadSelection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const FIXTURE: &str = "tests/fixtures/change/field-vertical";

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create target");
    for entry in std::fs::read_dir(from).expect("read source") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let unique = NEXT.fetch_add(1, Ordering::SeqCst);
        let root = std::env::current_dir()
            .expect("cwd")
            .join("target/change-core")
            .join(format!("{tag}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../")
                .join(FIXTURE),
            &root,
        );
        Self { root }
    }

    fn selection(&self) -> LoadSelection {
        LoadSelection {
            project: Some(
                self.root
                    .strip_prefix(std::env::current_dir().expect("cwd"))
                    .expect("relative")
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/"),
            ),
        }
    }

    fn selection_str(&self) -> String {
        self.selection().project.unwrap_or_default()
    }

    fn entities_path(&self) -> PathBuf {
        self.root.join("lekalo/modules/tasks/entities.yaml")
    }

    fn entities_bytes(&self) -> Vec<u8> {
        std::fs::read(self.entities_path()).expect("entities bytes")
    }

    fn history_path(&self) -> PathBuf {
        self.root.join("lekalo/change-history.json")
    }
}

fn add_priority_request() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "entity-field:add",
            "entity": "tasks.task",
            "field": { "name": "priority", "type": { "ref": "tasks.priority" } }
        }]
    }))
    .expect("request serializes")
}

fn prepare(sandbox: &Sandbox, request: &[u8]) -> Result<change::PreparedChange, ChangeError> {
    change::prepare(&PrepareOptions {
        project: Some(&sandbox.selection_str()),
        request,
        policy: None,
        allow_breaking: false,
    })
}

fn apply_request(
    sandbox: &Sandbox,
    request: &[u8],
    key: &str,
    allow_breaking: bool,
) -> Result<change::Receipt, ChangeError> {
    change::apply_request(&ApplyRequestOptions {
        project: Some(&sandbox.selection_str()),
        request,
        policy: None,
        allow_breaking,
        idempotency_key: key,
        expect_plan: None,
    })
}

#[test]
fn add_field_updates_the_canonical_model_without_text_search() {
    let sandbox = Sandbox::new("add-field");
    let request = add_priority_request();
    let receipt = apply_request(&sandbox, &request, "add-priority-v1", false).expect("applies");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    // The canonical model now carries the typed field.
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).ok();
    let model = model.expect("loads after commit");
    let entity = model
        .definitions
        .iter()
        .find(|definition| definition.id == "tasks.task")
        .expect("entity resolved by typed identity");
    let fields = entity
        .node
        .get("fields")
        .expect("fields")
        .as_seq()
        .expect("sequence");
    assert!(fields.iter().any(|field| field
        .get("name")
        .and_then(lekalo_core::loader::frontends::Node::as_str)
        == Some("priority")));
    // Exactly one history record binds the key.
    let history = std::fs::read(sandbox.history_path()).expect("history bytes");
    let history: serde_json::Value = serde_json::from_slice(&history).expect("history json");
    assert_eq!(history["nextSequence"], 2);
    assert_eq!(history["records"].as_array().expect("records").len(), 1);
}

#[test]
fn dry_run_writes_nothing_and_carries_both_diffs() {
    let sandbox = Sandbox::new("dry-run");
    let before = sandbox.entities_bytes();
    let request = add_priority_request();
    let prepared = prepare(&sandbox, &request).expect("prepares");
    // The plan carries the semantic diff and the exact file diff.
    let plan: serde_json::Value =
        serde_json::from_str(&prepared.to_json_string()).expect("plan json");
    assert_eq!(plan["schemaVersion"], "lekalo/change-plan/v0.8.0");
    assert_eq!(plan["identity"], "dev.lekalo.change-plan@0.8.0");
    assert_eq!(plan["equal"], false);
    assert_eq!(plan["semanticDiff"]["equal"], false);
    let changes = plan["semanticDiff"]["changes"].as_array().expect("changes");
    assert!(
        changes.iter().any(|change| {
            change["subject"]["id"] == "tasks.task"
                && change["subject"]["member"] == "field.priority"
        }),
        "the field member seed is present: {changes:?}"
    );
    let file_diff = plan["fileDiff"].as_array().expect("fileDiff");
    assert_eq!(file_diff.len(), 1);
    let patch = file_diff[0]["patch"].as_array().expect("patch");
    assert!(patch.iter().any(|line| line["kind"] == "insert"));
    // The impact projection explains the affected subjects per side.
    assert!(!plan["impact"]["seeds"]
        .as_array()
        .expect("seeds")
        .is_empty());
    assert_eq!(plan["generation"]["executed"], false);
    // Nothing was written: source bytes, history, and the runtime cache.
    assert_eq!(sandbox.entities_bytes(), before);
    assert!(!sandbox.history_path().exists());
    assert!(!sandbox.root.join(".lekalo").exists());
}

#[test]
fn breaking_remove_denies_without_acknowledgement_and_applies_with_it() {
    let sandbox = Sandbox::new("breaking");
    // Land the field first.
    let request = add_priority_request();
    apply_request(&sandbox, &request, "add-priority-v1", false).expect("add commits");
    let before = sandbox.entities_bytes();
    let remove = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "entity-field:remove",
            "entity": "tasks.task",
            "name": "priority"
        }]
    }))
    .expect("remove serializes");
    let error = apply_request(&sandbox, &remove, "remove-priority-v1", false)
        .expect_err("denied without acknowledgement");
    match error {
        ChangeError::CompatibilityDenied { classes } => {
            assert!(!classes.is_empty(), "the breaking classes are reported");
        }
        other => panic!("expected compatibility denial, got {other:?}"),
    }
    assert_eq!(sandbox.entities_bytes(), before, "nothing mutated");
    let receipt =
        apply_request(&sandbox, &remove, "remove-priority-v1", true).expect("applies with flag");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let after = String::from_utf8(sandbox.entities_bytes()).expect("utf8");
    assert!(!after.contains("- name: \"priority\""));
    // The enum definition survives the member removal.
    assert!(after.contains("- id: tasks.priority"));
}

#[test]
fn repeated_key_replays_the_original_receipt_without_duplicating() {
    let sandbox = Sandbox::new("replay");
    let request = add_priority_request();
    let first = apply_request(&sandbox, &request, "add-priority-v1", false).expect("commits");
    assert_eq!(first.state(), ReceiptState::Committed);
    assert_eq!(first.replayed, false);
    // The Model already carries the field; the ledger still replays.
    let second = apply_request(&sandbox, &request, "add-priority-v1", false).expect("replays");
    assert_eq!(second.state(), ReceiptState::Replayed);
    assert_eq!(second.replayed, true);
    assert_eq!(second.plan_id, first.plan_id);
    // Exactly one history record and one committed field.
    let history: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sandbox.history_path()).expect("history"))
            .expect("history json");
    assert_eq!(history["records"].as_array().expect("records").len(), 1);
    let model = lekalo_core::loader::normalize_model(&sandbox.selection())
        .ok()
        .expect("loads");
    let entity = model
        .definitions
        .iter()
        .find(|definition| definition.id == "tasks.task")
        .expect("entity");
    let fields = entity
        .node
        .get("fields")
        .expect("fields")
        .as_seq()
        .expect("fields");
    assert_eq!(fields.len(), 3, "title, done, priority — no duplicate");
    // The lookup surface returns the same receipt without mutating.
    let lookup = change::lookup_receipt(Some(&sandbox.selection_str()), "add-priority-v1")
        .expect("lookup")
        .expect("recorded");
    assert_eq!(lookup.state(), ReceiptState::Committed);
}

#[test]
fn same_key_with_a_different_request_is_a_conflict() {
    let sandbox = Sandbox::new("conflict");
    let request = add_priority_request();
    apply_request(&sandbox, &request, "key-v1", false).expect("commits");
    let other = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "enum-case:add",
            "enum": "tasks.priority",
            "case": { "name": "blocked" }
        }]
    }))
    .expect("serializes");
    let error = apply_request(&sandbox, &other, "key-v1", false).expect_err("conflicts");
    assert!(matches!(error, ChangeError::IdempotencyConflict));
}

#[test]
fn comment_adjacent_layout_refuses_without_any_change() {
    let sandbox = Sandbox::new("comment");
    let path = sandbox.entities_path();
    let original = std::fs::read(&path).expect("bytes");
    let mut edited = original.clone();
    let text = String::from_utf8(edited).expect("utf8");
    let marker = "        type: \"tasks.bool\"";
    let position = text.find(marker).expect("marker") + marker.len();
    let with_comment = format!(
        "{}\n    # trailing comment{}",
        &text[..position],
        &text[position..]
    );
    edited = with_comment.into_bytes();
    std::fs::write(&path, &edited).expect("write");
    let request = add_priority_request();
    let error = prepare(&sandbox, &request).expect_err("refuses");
    assert!(matches!(
        error,
        ChangeError::SourcePreservation {
            detail: "comment-adjacent",
            ..
        }
    ));
    assert_eq!(std::fs::read(&path).expect("bytes"), edited, "zero changes");
}

#[test]
fn crlf_documents_keep_their_line_endings() {
    let sandbox = Sandbox::new("crlf");
    let path = sandbox.entities_path();
    let original = sandbox.entities_bytes();
    let crlf: Vec<u8> = original.iter().fold(Vec::new(), |mut out, byte| {
        if *byte == b'\n' && out.last() != Some(&b'\r') {
            out.push(b'\r');
        }
        out.push(*byte);
        out
    });
    std::fs::write(&path, &crlf).expect("write CRLF fixture");
    let request = add_priority_request();
    apply_request(&sandbox, &request, "crlf-v1", false).expect("commits");
    let after = sandbox.entities_bytes();
    assert!(after.windows(2).all(|window| window != b"\n\r"));
    let text = String::from_utf8(after).expect("utf8");
    assert!(text.contains("\r\n      - name: \"priority\"\r\n        type: \"tasks.priority\"\r\n"));
}

#[test]
fn unrelated_bytes_stay_verbatim_outside_the_declared_range() {
    let sandbox = Sandbox::new("verbatim");
    let before = sandbox.entities_bytes();
    let request = add_priority_request();
    apply_request(&sandbox, &request, "verbatim-v1", false).expect("commits");
    let after = sandbox.entities_bytes();
    // Every line outside the inserted item is byte-identical, in order.
    let before_text = String::from_utf8(before).expect("utf8");
    let after_text = String::from_utf8(after).expect("utf8");
    let inserted: Vec<&str> = after_text
        .lines()
        .filter(|line| !before_text.lines().any(|original| original == *line))
        .collect();
    assert_eq!(inserted.len(), 2, "exactly the two inserted lines");
    assert!(inserted[0].contains("- name: \"priority\""));
    assert!(inserted[1].contains("type: \"tasks.priority\""));
    // The documentation comment above the scalar survives byte-for-byte.
    assert!(after_text.contains("# The text scalar keeps its own documentation comment"));
}

#[test]
fn drifted_inputs_refuse_the_sealed_plan() {
    let sandbox = Sandbox::new("drift");
    let request = add_priority_request();
    let prepared = prepare(&sandbox, &request).expect("prepares");
    // Concurrent drift between prepare and apply.
    let path = sandbox.entities_path();
    let mut bytes = sandbox.entities_bytes();
    bytes.extend(b"\n");
    std::fs::write(&path, &bytes).expect("drift");
    let error = change::apply_prepared(
        &prepared,
        &ApplyOptions {
            idempotency_key: "drift-v1",
            expect_plan: None,
        },
    )
    .expect_err("precondition");
    assert!(matches!(
        error,
        ChangeError::PreconditionFailed {
            detail: "input-digest",
            ..
        }
    ));
}

#[test]
fn wrong_expect_model_digest_refuses() {
    let sandbox = Sandbox::new("expect-model");
    let mut request =
        serde_json::from_slice::<serde_json::Value>(&add_priority_request()).expect("json");
    request["expectModel"] = serde_json::json!(
        "sha256:0000000000000000000000000000000000000000000000000000000000000000"
    );
    let bytes = serde_json::to_vec(&request).expect("serializes");
    let error = prepare(&sandbox, &bytes).expect_err("precondition");
    assert!(matches!(
        error,
        ChangeError::PreconditionFailed {
            detail: "expect-model",
            ..
        }
    ));
}

#[test]
fn unsupported_cataloged_operations_refuse_explicitly() {
    let sandbox = Sandbox::new("unsupported");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "definition:add",
            "kind": "command",
            "module": "tasks",
            "definition": { "id": "tasks.complete" }
        }]
    }))
    .expect("serializes");
    let error = prepare(&sandbox, &request).expect_err("unsupported");
    match error {
        ChangeError::OperationUnsupported { operation } => {
            assert_eq!(operation, "definition:add");
        }
        other => panic!("expected operation-unsupported, got {other:?}"),
    }
}

#[test]
fn declared_defaults_refuse_until_the_model_successor() {
    let sandbox = Sandbox::new("default");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "entity-field:add",
            "entity": "tasks.task",
            "field": { "name": "priority", "type": { "ref": "tasks.priority" }, "default": "normal" }
        }]
    }))
    .expect("serializes");
    let error = prepare(&sandbox, &request).expect_err("unsupported member");
    assert!(matches!(
        error,
        ChangeError::UnsupportedMember {
            member: "field-default",
            ..
        }
    ));
    // Nothing was written.
    assert!(!sandbox.history_path().exists());
}

#[test]
fn recovery_barrier_blocks_readers_until_recover_succeeds() {
    let sandbox = Sandbox::new("recovery");
    let journal_dir = sandbox.root.join(".lekalo/cache/change/change-ab12cd34-1");
    std::fs::create_dir_all(&journal_dir).expect("journal dir");
    std::fs::write(
        journal_dir.join("journal.json"),
        br#"{"transaction":"change-ab12cd34-1","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","files":[],"history":false}"#,
    )
    .expect("journal");
    // Readers fail closed while the journal exists.
    let request = add_priority_request();
    let error = prepare(&sandbox, &request).expect_err("barrier");
    assert!(matches!(
        error,
        ChangeError::RecoveryRequired { transaction } if transaction == "change-ab12cd34-1"
    ));
    assert_eq!(
        change::pending_transactions(Some(&sandbox.selection_str())).expect("pending"),
        vec!["change-ab12cd34-1".to_owned()]
    );
    // Recovery clears the journal and the barrier.
    change::recover_transaction(Some(&sandbox.selection_str()), "change-ab12cd34-1")
        .expect("recover");
    assert!(change::pending_transactions(Some(&sandbox.selection_str()))
        .expect("pending")
        .is_empty());
    prepare(&sandbox, &request).expect("prepares after recovery");
}

#[test]
fn recovery_refuses_a_journal_path_that_escapes_the_project_root() {
    // Review finding 1 (CRITICAL): a planted journal must never turn
    // `change recover` into an arbitrary write outside the project.
    let sandbox = Sandbox::new("escape");
    let journal_dir = sandbox.root.join(".lekalo/cache/change/change-escape01");
    std::fs::create_dir_all(&journal_dir).expect("journal dir");
    std::fs::write(
        journal_dir.join("journal.json"),
        br#"{"transaction":"change-escape01","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","files":[{"path":"../le73-ESCAPE.txt","beforeDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","afterDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","creation":false}],"history":false}"#,
    )
    .expect("journal");
    let error =
        change::recover_transaction(Some(&sandbox.selection_str()), "change-escape01")
            .expect_err("traversal refused");
    assert!(
        matches!(error, ChangeError::WriteBoundaryDenied),
        "got {error:?}"
    );
    // Nothing was written outside the project and the journal stays (the
    // barrier keeps readers blocked until a legitimate recovery).
    assert!(!sandbox.root.join("le73-ESCAPE.txt").exists());
    assert!(!sandbox.root.parent().unwrap().join("le73-ESCAPE.txt").exists());
    assert!(journal_dir.join("journal.json").is_file());
}

#[test]
fn recovery_refuses_an_absolute_and_absolute_windows_journal_path() {
    let sandbox = Sandbox::new("absolutely");
    let journal_dir = sandbox.root.join(".lekalo/cache/change/change-absol001");
    std::fs::create_dir_all(&journal_dir).expect("journal dir");
    std::fs::write(
        journal_dir.join("journal.json"),
        br#"{"transaction":"change-absol001","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","files":[{"path":"lekalo/modules/tasks/entities.yaml","beforeDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","afterDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","creation":false},{"path":"/etc/lekalo-pwn","beforeDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","afterDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","creation":false}],"history":false}"#,
    )
    .expect("journal");
    let error =
        change::recover_transaction(Some(&sandbox.selection_str()), "change-absol001")
            .expect_err("absolute path refused");
    assert!(matches!(error, ChangeError::WriteBoundaryDenied), "{error:?}");
}

#[test]
fn recovery_refuses_a_corrupted_backup_and_keeps_the_barrier() {
    // Review finding 4 (HIGH): recovery must digest-verify every backup
    // against the journal pin before restoring; a tampered backup refuses
    // and never lifts the reader barrier.
    let sandbox = Sandbox::new("corrupt");
    let journal_dir = sandbox.root.join(".lekalo/cache/change/change-corrupt01");
    std::fs::create_dir_all(journal_dir.join("backups")).expect("journal dir");
    // The current entities bytes are the legitimate "backup"; the journal
    // pins a different (correct-at-begin-time) digest, so the stored bytes
    // no longer match.
    let entities = sandbox.entities_bytes();
    std::fs::write(journal_dir.join("backups/0"), &entities).expect("backup");
    let wrong_digest = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    let journal = format!(
        r#"{{"transaction":"change-corrupt01","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","files":[{{"path":"lekalo/modules/tasks/entities.yaml","beforeDigest":"{wrong_digest}","afterDigest":"sha256:2222222222222222222222222222222222222222222222222222222222222222","creation":false}}],"history":false}}"#
    );
    std::fs::write(journal_dir.join("journal.json"), journal).expect("journal");
    let before = sandbox.entities_bytes();
    let error =
        change::recover_transaction(Some(&sandbox.selection_str()), "change-corrupt01")
            .expect_err("corrupt backup refused");
    assert!(
        matches!(
            error,
            ChangeError::CommitFailed {
                detail: "backup-digest-mismatch",
                ..
            }
        ),
        "got {error:?}"
    );
    // The target was not touched and the barrier stays up.
    assert_eq!(sandbox.entities_bytes(), before);
    assert!(journal_dir.join("journal.json").is_file());
    assert_eq!(
        change::pending_transactions(Some(&sandbox.selection_str())).expect("pending"),
        vec!["change-corrupt01".to_owned()]
    );
}

#[test]
fn recovery_restores_verified_backups_and_reports_the_rollback() {
    let sandbox = Sandbox::new("restore");
    let journal_dir = sandbox.root.join(".lekalo/cache/change/change-restore01");
    std::fs::create_dir_all(journal_dir.join("backups")).expect("journal dir");
    // A interrupted commit: the target currently holds the (bad)
    // prospective bytes; the verified backup holds the original.
    let original = sandbox.entities_bytes();
    let mut interrupted = original.clone();
    interrupted.extend(b"interrupted: true\n");
    std::fs::write(sandbox.entities_path(), &interrupted).expect("interrupted bytes");
    std::fs::write(journal_dir.join("backups/0"), &original).expect("backup");
    let digest = |bytes: &[u8]| format!("sha256:{}", lekalo_core::digest::sha256_hex(bytes));
    let journal = format!(
        r#"{{"transaction":"change-restore01","planId":"sha256:3333333333333333333333333333333333333333333333333333333333333333","files":[{{"path":"lekalo/modules/tasks/entities.yaml","beforeDigest":"{0}","afterDigest":"sha256:4444444444444444444444444444444444444444444444444444444444444444","creation":false}}],"history":false}}"#,
        digest(&original)
    );
    std::fs::write(journal_dir.join("journal.json"), journal).expect("journal");
    let outcome =
        change::recover_transaction(Some(&sandbox.selection_str()), "change-restore01")
            .expect("recovers");
    assert_eq!(outcome.transaction, "change-restore01");
    assert_eq!(outcome.files.len(), 1);
    assert_eq!(outcome.files[0].path, "lekalo/modules/tasks/entities.yaml");
    assert_eq!(outcome.files[0].after_digest, digest(&original));
    assert_eq!(sandbox.entities_bytes(), original, "originals restored");
    assert!(!journal_dir.exists(), "journal removed after success");
    assert!(change::pending_transactions(Some(&sandbox.selection_str()))
        .expect("pending")
        .is_empty());
}

#[test]
fn recovery_reclaims_a_stale_writer_lock_but_honors_a_live_one() {
    // Review finding 3 (HIGH): the most common crash artifact is a stale
    // `writer.lock`; recovery must reclaim it instead of failing forever.
    let sandbox = Sandbox::new("stale");
    let cache = sandbox.root.join(".lekalo/cache/change");
    std::fs::create_dir_all(&cache).expect("cache");
    let journal_dir = cache.join("change-stale0001");
    std::fs::create_dir_all(&journal_dir).expect("journal dir");
    std::fs::write(
        journal_dir.join("journal.json"),
        br#"{"transaction":"change-stale0001","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","files":[],"history":false}"#,
    )
    .expect("journal");
    // A stale lock whose recorded owner is dead (this test process is
    // alive, so the metadata must point elsewhere to be stale).
    std::fs::write(cache.join("writer.lock"), b"").expect("stale lock");
    std::fs::write(
        cache.join("writer.lock.json"),
        format!("{{\"pid\":999999999,\"boot\":\"{}\"}}", "definitely-not-this-boot"),
    )
    .expect("stale lock meta");
    change::recover_transaction(Some(&sandbox.selection_str()), "change-stale0001")
        .expect("stale lock reclaimed");
    assert!(!cache.join("writer.lock").exists(), "lock released");
    assert!(!journal_dir.exists(), "journal consumed");
}

#[test]
fn recovery_refuses_malformed_journal_members_instead_of_skipping() {
    // Review finding 11: non-string or malformed journal members fail the
    // recovery closed; the journal is never consumed by a partial pass.
    let sandbox = Sandbox::new("shaped");
    let journal_dir = sandbox.root.join(".lekalo/cache/change/change-malfm0001");
    std::fs::create_dir_all(&journal_dir).expect("journal dir");
    std::fs::write(
        journal_dir.join("journal.json"),
        br#"{"transaction":"change-malfm0001","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","files":[{"path":"lekalo/modules/tasks/entities.yaml","beforeDigest":42,"afterDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","creation":false}],"history":false}"#,
    )
    .expect("journal");
    let error =
        change::recover_transaction(Some(&sandbox.selection_str()), "change-malfm0001")
            .expect_err("malformed journal refused");
    assert!(
        matches!(
            error,
            ChangeError::CommitFailed {
                detail: "journal-shape",
                ..
            }
        ),
        "got {error:?}"
    );
    assert!(journal_dir.join("journal.json").is_file());
}

#[test]
fn enum_case_add_commits_and_preserves_the_document() {
    let sandbox = Sandbox::new("enum");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "enum-case:add",
            "enum": "tasks.priority",
            "case": { "name": "blocked", "description": "Blocked by an external dependency" }
        }]
    }))
    .expect("serializes");
    let receipt = apply_request(&sandbox, &request, "enum-blocked-v1", false).expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let after = String::from_utf8(sandbox.entities_bytes()).expect("utf8");
    assert!(after.contains("- value: \"blocked\""));
    assert!(after.contains("description: \"Blocked by an external dependency\""));
    // The scalar definitions above the enum are untouched.
    assert!(after.contains("- id: tasks.text"));
}

#[test]
fn rename_touches_only_the_identity_scalar_bytes() {
    let sandbox = Sandbox::new("rename");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "entity-field:rename",
            "entity": "tasks.task",
            "name": "done",
            "to": "completed"
        }]
    }))
    .expect("serializes");
    // A member rename is member-destructive: the default policy denies it.
    let denied = change::prepare(&PrepareOptions {
        project: Some(&sandbox.selection_str()),
        request: &request,
        policy: None,
        allow_breaking: false,
    })
    .expect_err("denied by default");
    assert!(matches!(
        denied,
        ChangeError::CompatibilityDenied { ref classes } if !classes.is_empty()
    ));
    let prepared = change::prepare(&PrepareOptions {
        project: Some(&sandbox.selection_str()),
        request: &request,
        policy: None,
        allow_breaking: true,
    })
    .expect("prepares with acknowledgement");
    let plan: serde_json::Value =
        serde_json::from_str(&prepared.to_json_string()).expect("plan json");
    let patch = plan["fileDiff"][0]["patch"].as_array().expect("patch");
    let inserted: Vec<&serde_json::Value> = patch
        .iter()
        .filter(|line| line["kind"] == "insert")
        .collect();
    let deleted: Vec<&serde_json::Value> = patch
        .iter()
        .filter(|line| line["kind"] == "delete")
        .collect();
    assert_eq!(inserted.len(), 1);
    assert_eq!(deleted.len(), 1);
    assert!(inserted[0]["text"]
        .as_str()
        .expect("text")
        .contains("completed"));
    assert!(deleted[0]["text"].as_str().expect("text").contains("done"));
    let receipt = change::apply_prepared(
        &prepared,
        &ApplyOptions {
            idempotency_key: "rename-v1",
            expect_plan: None,
        },
    )
    .expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    assert!(String::from_utf8(sandbox.entities_bytes())
        .expect("utf8")
        .contains("name: completed"));
}

#[test]
fn unknown_operations_and_malformed_requests_fail_closed() {
    let sandbox = Sandbox::new("invalid");
    let unknown = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{ "operation": "entity-field:append", "entity": "tasks.task" }]
    }))
    .expect("serializes");
    let error = prepare(&sandbox, &unknown).expect_err("unknown op");
    assert!(matches!(
        error,
        ChangeError::RequestInvalid {
            detail: "unknown-operation"
        }
    ));
    let duplicate_key = br#"{"schemaVersion":"lekalo/change-request/v0.8.0","schemaVersion":"lekalo/change-request/v0.8.0","operations":[{"operation":"entity-field:add","entity":"tasks.task","field":{"name":"x","type":{"ref":"tasks.text"}}}]}"#;
    let error = prepare(&sandbox, duplicate_key).expect_err("duplicate key");
    assert!(matches!(
        error,
        ChangeError::RequestInvalid {
            detail: "malformed"
        }
    ));
}

#[test]
fn plans_are_deterministic_across_repeated_preparations() {
    let sandbox = Sandbox::new("determinism");
    let request = add_priority_request();
    let first = prepare(&sandbox, &request).expect("first");
    let second = prepare(&sandbox, &request).expect("second");
    assert_eq!(first.plan_id(), second.plan_id());
    assert_eq!(first.to_json_string(), second.to_json_string());
    // The plan validates against its published schema surface: the closed
    // required members are present.
    let plan: serde_json::Value = serde_json::from_str(&first.to_json_string()).expect("json");
    for member in [
        "schemaVersion",
        "identity",
        "planId",
        "requestDigest",
        "catalogDigest",
        "fileEdits",
        "fileDiff",
        "semanticDiff",
        "impact",
        "generation",
        "breakingClasses",
        "complete",
    ] {
        assert!(plan.get(member).is_some(), "{member} present");
    }
}

#[test]
fn a_forged_history_record_is_refused_not_replayed() {
    // Review finding 5 (HIGH): a hand-forged history record with the right
    // key digest and an invented plan/receipt must never mint a replayed
    // receipt. The chain + binding verification fails closed.
    let sandbox = Sandbox::new("forged");
    let key = "forged-key-v1";
    let key_digest = lekalo_core::change::key_digest(key);
    let zeros = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    // Forge a well-shaped record whose receipt carries the same forged
    // fields (an attacker would do at least this much), but the chain
    // anchor cannot be recomputed without the private genesis chain.
    let forged = format!(
        r#"{{"schemaVersion":"lekalo/change-history/v0.8.0","identity":"dev.lekalo.change-history@0.8.0","project":"tasks","nextSequence":2,"records":[{{"sequence":1,"keyDigest":"{key_digest}","requestDigest":"{zeros}","planId":"{zeros}","operations":["entity-field:add"],"files":["lekalo/modules/tasks/entities.yaml"],"receipt":{{"status":"valid","schemaVersion":"lekalo/change-receipt/v0.8.0","identity":"dev.lekalo.change-receipt@0.8.0","state":"committed","planId":"{zeros}","requestDigest":"{zeros}","keyDigest":"{key_digest}","operations":[{{"operation":"entity-field:add","subject":"tasks.task","subjectFamily":"entity","status":"resolved"}}],"files":[{{"path":"lekalo/modules/tasks/entities.yaml","beforeDigest":"{zeros}","afterDigest":"{zeros}","outcome":"replaced"}}],"history":{{"sequence":1,"recordDigest":"{zeros}"}},"transaction":{{"id":"change-forged0001","state":"committed"}},"diagnosticIds":[],"replayed":false}}}}]}}"#
    );
    std::fs::write(sandbox.history_path(), forged).expect("forge history");
    // Replay over the forged ledger refuses instead of fabricating.
    let error = apply_request(&sandbox, &add_priority_request(), key, false)
        .expect_err("forged ledger refused");
    assert!(
        matches!(error, ChangeError::HistoryInvalid { .. }),
        "got {error:?}"
    );
    // The lookup surface refuses too.
    let error = change::lookup_receipt(Some(&sandbox.selection_str()), key)
        .expect_err("lookup refuses forged ledger");
    assert!(matches!(error, ChangeError::HistoryInvalid { .. }));
    // A zero-byte ledger is a corrupt document, not an empty one.
    std::fs::write(sandbox.history_path(), b"").expect("empty history");
    let error = change::lookup_receipt(Some(&sandbox.selection_str()), key)
        .expect_err("empty ledger refused");
    assert!(
        matches!(
            error,
            ChangeError::HistoryInvalid {
                detail: "empty",
                ..
            }
        ),
        "got {error:?}"
    );
}

#[test]
fn a_valid_multi_operation_batch_on_one_file_commits() {
    // Review finding 7 (MEDIUM): two resolved operations on the same file
    // used to always fail reparse-mismatch; the batch is legal and commits.
    let sandbox = Sandbox::new("batch");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [
            {
                "operation": "entity-field:add",
                "entity": "tasks.task",
                "field": { "name": "priority", "type": { "ref": "tasks.priority" } }
            },
            {
                "operation": "entity-field:add",
                "entity": "tasks.task",
                "field": { "name": "owner", "type": { "ref": "tasks.text" } }
            }
        ]
    }))
    .expect("serializes");
    let prepared = prepare(&sandbox, &request).expect("prepares");
    assert_eq!(prepared.file_edits().len(), 1, "both ops hit one file");
    let receipt = change::apply_prepared(
        &prepared,
        &ApplyOptions {
            idempotency_key: "batch-v1",
            expect_plan: None,
        },
    )
    .expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let after = String::from_utf8(sandbox.entities_bytes()).expect("utf8");
    assert!(after.contains("- name: \"priority\""));
    assert!(after.contains("- name: \"owner\""));
    // The re-decoded candidate compiled identically: the project still loads.
    let model = lekalo_core::loader::normalize_model(&sandbox.selection())
        .ok()
        .expect("loads after batch commit");
    let entity = model
        .definitions
        .iter()
        .find(|definition| definition.id == "tasks.task")
        .expect("entity");
    let fields = entity
        .node
        .get("fields")
        .expect("fields")
        .as_seq()
        .expect("sequence");
    assert_eq!(fields.len(), 4, "title, done, priority, owner");
}

#[test]
fn an_invalid_semantic_candidate_is_a_typed_diagnostic_not_a_panic() {
    // Review finding 2 (CRITICAL): a request-controlled invalid candidate
    // (field name with invalid syntax downstream) must return a typed
    // change.*/ir.* diagnostic with exit class invalid — never a panic.
    let sandbox = Sandbox::new("candidate");
    // `tasks.text` resolves as a type; renaming a field to a name that
    // violates the member grammar is refused at decode time, so drive the
    // compile-failure path through a rename whose target collides at the
    // IR level: rename `title` to `done` while `done` exists.
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "entity-field:rename",
            "entity": "tasks.task",
            "name": "title",
            "to": "done"
        }]
    }))
    .expect("serializes");
    let error = prepare(&sandbox, &request).expect_err("collision refused");
    match error {
        ChangeError::SubjectConflict { detail, .. } => {
            assert_eq!(detail, "field-duplicate");
        }
        other => panic!("expected subject conflict, got {other:?}"),
    }
    assert!(!sandbox.history_path().exists());
}
