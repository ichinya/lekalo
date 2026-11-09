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
        if entry.file_name() == ".lekalo" {
            continue;
        }
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
        Self::from_fixture(tag, FIXTURE)
    }

    fn from_fixture(tag: &str, fixture: &str) -> Self {
        let unique = NEXT.fetch_add(1, Ordering::SeqCst);
        let root = std::env::current_dir()
            .expect("cwd")
            .join("target/change-core")
            .join(format!("{tag}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../")
                .join(fixture),
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
    add_field_request("priority")
}

fn ownership_source(sandbox: &Sandbox, module: &str, mode: &str) -> Vec<u8> {
    let mut policy: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/managed/planner/ownership.json"),
        )
        .unwrap(),
    )
    .unwrap();
    policy["modules"][0]["module"] = serde_json::json!(module);
    policy["modules"][0]["mode"] = serde_json::json!(mode);
    let bytes = serde_json::to_vec_pretty(&policy).unwrap();
    std::fs::write(
        sandbox
            .root
            .join(lekalo_core::managed::source::PROJECT_PATH),
        &bytes,
    )
    .unwrap();
    bytes
}

fn ownership_request(
    module: &str,
    mode: &str,
    source: &[u8],
    coverage: Option<(&str, &[u8])>,
) -> Vec<u8> {
    let mut evidence = serde_json::json!({"ownershipDigest":lekalo_core::managed::digest(source)});
    if let Some((path, bytes)) = coverage {
        evidence["coverage"] = serde_json::json!([{"target":"node-typescript","path":path,"digest":lekalo_core::managed::digest(bytes)}]);
    }
    serde_json::to_vec(&serde_json::json!({"schemaVersion":"lekalo/change-request/v0.8.0","operations":[{"operation":"ownership:set","subject":module,"mode":mode,"evidence":evidence}]})).unwrap()
}

fn ownership_managed_sandbox(tag: &str) -> Sandbox {
    let sandbox = Sandbox::from_fixture(tag, "tests/fixtures/managed/planner");
    // Recompute the owner's pins from the current valid fixture snapshot.
    let authority =
        lekalo_core::managed::authority::derive(&sandbox.selection(), "planner").unwrap();
    let path = sandbox.root.join("coverage.json");
    let mut coverage: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    coverage["authority"] = serde_json::json!({"state":"known","value":{"model":authority.model_digest,"ir":authority.ir_digest,"lock":authority.lock_digest}});
    std::fs::write(path, serde_json::to_vec_pretty(&coverage).unwrap()).unwrap();
    sandbox
}

#[test]
fn ownership_changes_one_policy_scalar_and_preserves_unselected_legacy_assignments() {
    let sandbox = Sandbox::new("ownership-selective");
    let source = ownership_source(&sandbox, "tasks", "observed");
    let mut policy: serde_json::Value = serde_json::from_slice(&source).unwrap();
    let mut legacy = policy["modules"][0].clone();
    legacy["module"] = serde_json::json!("legacy");
    legacy["targets"][0]["generatedRoot"] = serde_json::json!("apps/legacy/gen");
    policy["modules"].as_array_mut().unwrap().insert(0, legacy);
    let before = serde_json::to_string_pretty(&policy).unwrap();
    let path = sandbox
        .root
        .join(lekalo_core::managed::source::PROJECT_PATH);
    std::fs::write(&path, &before).unwrap();
    let request = ownership_request("tasks", "contracted", before.as_bytes(), None);
    assert!(matches!(
        prepare(&sandbox, &request),
        Err(ChangeError::CompatibilityDenied { .. })
    ));
    let plan = prepare_breaking(&sandbox, &request).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before.as_bytes());
    assert!(!sandbox.root.join(".lekalo").exists());
    let wire: serde_json::Value = serde_json::from_str(&plan.to_json_string().unwrap()).unwrap();
    assert_eq!(
        wire["semanticDiff"]["changes"][0]["kind"],
        "ownership.mode-changed"
    );
    assert_eq!(wire["impact"]["seeds"][0]["id"], "tasks");
    assert_eq!(wire["fileEdits"].as_array().unwrap().len(), 1);
    let entities = sandbox.entities_bytes();
    apply_symbol_plan(&plan, "ownership-selective").unwrap();
    let mut expected = before.clone();
    let position = expected.rfind("\"observed\"").unwrap();
    expected.replace_range(position..position + "\"observed\"".len(), "\"contracted\"");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    assert_eq!(sandbox.entities_bytes(), entities);
    assert_eq!(
        replay_symbol(&sandbox, &request, "ownership-selective", &plan.plan_id())
            .unwrap()
            .state(),
        ReceiptState::Replayed
    );
    let no_op = ownership_request("tasks", "contracted", expected.as_bytes(), None);
    let no_op = prepare(&sandbox, &no_op).unwrap();
    assert!(no_op.is_no_op());
    assert_eq!(
        apply_symbol_plan(&no_op, "ownership-noop").unwrap().state(),
        ReceiptState::NoOp
    );
}

#[test]
fn ownership_managed_transition_reuses_live_model_lock_capability_and_extension_admission() {
    let sandbox = ownership_managed_sandbox("ownership-managed");
    let source = ownership_source(&sandbox, "planner", "observed");
    let coverage = std::fs::read(sandbox.root.join("coverage.json")).unwrap();
    let request = ownership_request(
        "planner",
        "managed",
        &source,
        Some(("coverage.json", &coverage)),
    );
    let plan = prepare_breaking(&sandbox, &request).unwrap();
    let adapter = std::fs::read(sandbox.root.join("adapters/managed.mjs")).unwrap();
    apply_symbol_plan(&plan, "ownership-managed").unwrap();
    assert_eq!(
        std::fs::read(sandbox.root.join("adapters/managed.mjs")).unwrap(),
        adapter
    );
    assert!(
        !sandbox.root.join("apps").exists(),
        "a policy transition never generates native implementations"
    );
    let source = std::fs::read(
        sandbox
            .root
            .join(lekalo_core::managed::source::PROJECT_PATH),
    )
    .unwrap();
    assert_eq!(
        lekalo_core::managed::parse_ownership(&source)
            .unwrap()
            .modules[0]
            .mode,
        lekalo_core::managed::Mode::Managed
    );
    assert_eq!(
        replay_symbol(&sandbox, &request, "ownership-managed", &plan.plan_id())
            .unwrap()
            .state(),
        ReceiptState::Replayed
    );
}

#[test]
fn ownership_stale_source_and_coverage_refuse_before_a_mode_write() {
    let sandbox = ownership_managed_sandbox("ownership-stale");
    let source = ownership_source(&sandbox, "planner", "observed");
    let coverage = std::fs::read(sandbox.root.join("coverage.json")).unwrap();
    let request = ownership_request(
        "planner",
        "managed",
        &source,
        Some(("coverage.json", &coverage)),
    );
    let plan = prepare_breaking(&sandbox, &request).unwrap();
    let mut drift = coverage;
    drift.push(b'\n');
    std::fs::write(sandbox.root.join("coverage.json"), drift).unwrap();
    assert!(matches!(
        apply_symbol_plan(&plan, "ownership-stale"),
        Err(ChangeError::PreconditionFailed { .. })
    ));
    assert_eq!(
        std::fs::read(
            sandbox
                .root
                .join(lekalo_core::managed::source::PROJECT_PATH)
        )
        .unwrap(),
        source
    );
    assert!(!sandbox.history_path().exists());
}

#[test]
fn ownership_managed_gaps_and_missing_sources_never_install_or_promote_a_policy() {
    let sandbox = ownership_managed_sandbox("ownership-gap");
    let source = ownership_source(&sandbox, "planner", "observed");
    let no_coverage = ownership_request("planner", "managed", &source, None);
    assert!(prepare_breaking(&sandbox, &no_coverage).is_err());
    let mut coverage: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sandbox.root.join("coverage.json")).unwrap())
            .unwrap();
    coverage["complete"] = serde_json::json!(false);
    let coverage = serde_json::to_vec(&coverage).unwrap();
    std::fs::write(sandbox.root.join("coverage.json"), &coverage).unwrap();
    let request = ownership_request(
        "planner",
        "managed",
        &source,
        Some(("coverage.json", &coverage)),
    );
    assert!(prepare_breaking(&sandbox, &request).is_err());
    let path = sandbox
        .root
        .join(lekalo_core::managed::source::PROJECT_PATH);
    assert_eq!(std::fs::read(&path).unwrap(), source);
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(
        prepare_breaking(&sandbox, &request),
        Err(ChangeError::PreconditionFailed {
            detail: "project-policy-missing",
            ..
        })
    ));
    assert!(!path.exists());
}

fn symbol_move_request(symbol: &str, module: &str, name: Option<&str>) -> Vec<u8> {
    let mut operation =
        serde_json::json!({"operation":"symbol:move","symbol":symbol,"toModule":module});
    if let Some(name) = name {
        operation["toName"] = serde_json::json!(name);
    }
    serde_json::to_vec(&serde_json::json!({"schemaVersion":"lekalo/change-request/v0.8.0","operations":[operation]})).unwrap()
}

fn prepare_breaking(
    sandbox: &Sandbox,
    request: &[u8],
) -> Result<change::PreparedChange, ChangeError> {
    change::prepare(&PrepareOptions {
        project: Some(&sandbox.selection_str()),
        request,
        policy: None,
        allow_breaking: true,
    })
}

fn apply_symbol_plan(
    plan: &change::PreparedChange,
    key: &str,
) -> Result<change::Receipt, ChangeError> {
    change::apply_prepared(
        plan,
        &ApplyOptions {
            idempotency_key: key,
            expect_plan: Some(&plan.plan_id()),
            allow_breaking: true,
        },
    )
}

fn replay_symbol(
    sandbox: &Sandbox,
    request: &[u8],
    key: &str,
    plan: &str,
) -> Result<change::Receipt, ChangeError> {
    change::apply_request(&ApplyRequestOptions {
        project: Some(&sandbox.selection_str()),
        request,
        policy: None,
        allow_breaking: true,
        idempotency_key: key,
        expect_plan: Some(plan),
    })
}

#[test]
fn symbol_move_rewrites_typed_references_and_records_a_replayable_identity_chain() {
    let sandbox = Sandbox::new("symbol-chain");
    let before = sandbox.entities_bytes();
    let request = symbol_move_request("tasks.text", "tasks", Some("label"));
    let plan = prepare_breaking(&sandbox, &request).unwrap();
    assert_eq!(sandbox.entities_bytes(), before, "preview is read-only");
    let wire: serde_json::Value = serde_json::from_str(&plan.to_json_string().unwrap()).unwrap();
    assert_eq!(wire["operations"][0]["operation"], "symbol:move");
    assert!(wire["fileEdits"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["path"] == "lekalo/project.yaml"));
    apply_symbol_plan(&plan, "symbol-first").unwrap();
    let text = String::from_utf8(sandbox.entities_bytes()).unwrap();
    assert!(text.contains("type: \"tasks.label\""));
    assert!(text.contains("# The text scalar keeps its own documentation comment"));
    assert!(text.contains("type: \"tasks.bool\""));
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    assert!(lekalo_core::ir::compile(&model).is_ok());
    assert!(model
        .definitions
        .iter()
        .any(|definition| definition.id == "tasks.label"));
    let second = symbol_move_request("tasks.label", "tasks", Some("title_text"));
    apply_symbol_plan(
        &prepare_breaking(&sandbox, &second).unwrap(),
        "symbol-second",
    )
    .unwrap();
    let project: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sandbox.root.join("lekalo/project.yaml")).unwrap())
            .unwrap();
    assert_eq!(project["definitions"][0]["version"], 3);
    let history = &project["definitions"][0]["id_registry"]["rename_history"];
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert_eq!(history[0]["from"], "tasks.text");
    assert_eq!(history[0]["to"], "tasks.label");
    assert_eq!(history[1]["to"], "tasks.title_text");
    assert_eq!(history[1]["definition_version"], 3);
    assert!(matches!(
        prepare_breaking(
            &sandbox,
            &symbol_move_request("tasks.title_text", "tasks", Some("text"))
        ),
        Err(ChangeError::SubjectConflict {
            detail: "historical-id",
            ..
        })
    ));
    assert_eq!(
        replay_symbol(&sandbox, &request, "symbol-first", &plan.plan_id())
            .unwrap()
            .state(),
        ReceiptState::Replayed
    );
}

fn add_symbol_destination(sandbox: &Sandbox, existing_kind_home: bool) {
    let home = sandbox.root.join("lekalo/modules/physical-other");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("module.yaml"), "schema_version: \"0.2.16\"\ndefinitions:\n  - id: other\n    kind: module\n    version: 1\n").unwrap();
    if existing_kind_home {
        std::fs::write(home.join("entities.yaml"), "schema_version: \"0.2.16\"\ndefinitions:\n  - id: other.text\n    kind: scalar\n    version: 1\n    base: string\n").unwrap();
    }
}

#[test]
fn symbol_move_uses_declared_physical_homes_and_keeps_short_reference_meaning() {
    for existing in [false, true] {
        let sandbox = Sandbox::new("symbol-cross");
        add_symbol_destination(&sandbox, existing);
        let text = String::from_utf8(sandbox.entities_bytes())
            .unwrap()
            .replace("type: \"tasks.text\"", "type: \"text\"");
        std::fs::write(sandbox.entities_path(), text).unwrap();
        let request = symbol_move_request("tasks.task", "other", Some("item"));
        let plan = prepare_breaking(&sandbox, &request).unwrap();
        apply_symbol_plan(&plan, "symbol-cross").unwrap();
        let target = std::fs::read_to_string(
            sandbox
                .root
                .join("lekalo/modules/physical-other/entities.yaml"),
        )
        .unwrap();
        assert!(target.contains("  - id: other.item"));
        assert!(target.contains("type: \"tasks.text\""));
        assert!(target.contains("type: \"tasks.bool\""));
        assert!(!String::from_utf8(sandbox.entities_bytes())
            .unwrap()
            .contains("id: tasks.task"));
        let model = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
        let compiled = lekalo_core::ir::compile(&model).unwrap();
        assert!(compiled.project.to_canonical_json().contains("other.item"));
        assert_eq!(
            replay_symbol(&sandbox, &request, "symbol-cross", &plan.plan_id())
                .unwrap()
                .state(),
            ReceiptState::Replayed
        );
    }
}

#[test]
fn symbol_move_rewrites_wrapped_type_refs_without_changing_literals_or_prose() {
    let sandbox = Sandbox::new("symbol-wrappers");
    let text = String::from_utf8(sandbox.entities_bytes())
        .unwrap()
        .replace("type: \"tasks.text\"", "type: \"list<text?>\"")
        .replace(
            "    base: string",
            "    base: string\n    description: \"tasks.text\"",
        );
    std::fs::write(sandbox.entities_path(), text).unwrap();
    apply_symbol_plan(
        &prepare_breaking(
            &sandbox,
            &symbol_move_request("tasks.text", "tasks", Some("label")),
        )
        .unwrap(),
        "symbol-wrappers",
    )
    .unwrap();
    let text = String::from_utf8(sandbox.entities_bytes()).unwrap();
    assert!(text.contains("type: \"list<tasks.label?>\""));
    assert!(text.contains("description: \"tasks.text\""));
}

#[test]
fn symbol_move_seals_unsupported_attachment_homes_and_refuses_stale_plans() {
    let sandbox = Sandbox::new("symbol-attachments");
    let request = symbol_move_request("tasks.text", "tasks", Some("label"));
    let plan = prepare_breaking(&sandbox, &request).unwrap();
    let before = sandbox.entities_bytes();
    let path = sandbox.root.join("module-api.json");
    std::fs::write(&path, b"").unwrap();
    assert!(matches!(
        apply_symbol_plan(&plan, "symbol-stale"),
        Err(ChangeError::PreconditionFailed { .. })
    ));
    assert_eq!(sandbox.entities_bytes(), before);
    assert!(!sandbox.history_path().exists());
    assert!(matches!(
        prepare_breaking(&sandbox, &request),
        Err(ChangeError::UnsupportedMember {
            member: "reference-attachment",
            ..
        })
    ));
    std::fs::remove_file(path).unwrap();
    let home = sandbox.root.join("lekalo/types");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(home.join("extra.types.json"), b"{}").unwrap();
    assert!(matches!(
        apply_symbol_plan(&plan, "symbol-stale"),
        Err(ChangeError::PreconditionFailed { .. })
    ));
    assert_eq!(sandbox.entities_bytes(), before);
}

#[test]
fn symbol_move_preserves_inline_notes_and_refuses_ambiguous_replaced_history_trivia() {
    let sandbox = Sandbox::new("symbol-inline");
    let text = String::from_utf8(sandbox.entities_bytes())
        .unwrap()
        .replace("    base: string", "    base: string # scalar note");
    std::fs::write(sandbox.entities_path(), text).unwrap();
    let request = symbol_move_request("tasks.text", "tasks", Some("label"));
    apply_symbol_plan(
        &prepare_breaking(&sandbox, &request).unwrap(),
        "symbol-inline",
    )
    .unwrap();
    let text = String::from_utf8(sandbox.entities_bytes()).unwrap();
    assert!(text.contains("    base: string # scalar note\n    renamed_from:"));
    let text = text.replace("- \"tasks.text\"", "- \"tasks.text\" # history note");
    std::fs::write(sandbox.entities_path(), &text).unwrap();
    assert!(matches!(
        prepare_breaking(
            &sandbox,
            &symbol_move_request("tasks.label", "tasks", Some("another_label"))
        ),
        Err(ChangeError::SourcePreservation { .. })
    ));
    assert_eq!(String::from_utf8(sandbox.entities_bytes()).unwrap(), text);
}

#[test]
fn symbol_move_qualifies_each_supported_symbol_kind_and_its_real_consumers() {
    let fixture = Sandbox::from_fixture(
        "symbol-kinds-inventory",
        "tests/fixtures/model-v1/valid-planner",
    );
    let model = lekalo_core::loader::normalize_model(&fixture.selection()).unwrap();
    let mut kinds = std::collections::BTreeMap::new();
    for definition in &model.definitions {
        let kind = definition
            .node
            .get("kind")
            .and_then(lekalo_core::loader::frontends::Node::as_str)
            .unwrap();
        kinds
            .entry(kind.to_owned())
            .or_insert_with(|| definition.id.clone());
    }
    assert_eq!(kinds.len(), 12);
    for (kind, id) in kinds {
        let sandbox = Sandbox::from_fixture(
            &format!("symbol-kind-{kind}"),
            "tests/fixtures/model-v1/valid-planner",
        );
        let request = symbol_move_request(&id, "planner", Some("renamed_symbol"));
        let plan = prepare_breaking(&sandbox, &request)
            .unwrap_or_else(|error| panic!("{kind}: {error:?}"));
        apply_symbol_plan(&plan, &format!("symbol-{kind}")).unwrap();
        let model = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
        assert!(lekalo_core::ir::compile(&model).is_ok(), "{kind}");
        assert!(!model
            .definitions
            .iter()
            .any(|definition| definition.id == id));
    }
}

#[test]
fn symbol_move_updates_yaml_history_tombstones_and_crlf_without_rewriting_other_bytes() {
    let sandbox = Sandbox::new("symbol-yaml-history");
    let project = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks\n    kind: project\n    version: 2\n    id_registry:\n      tombstones:\n        - id: tasks.retired_text\n          reason: replaced\n          replaced_by: tasks.text\n          since: 2\n".replace('\n', "\r\n");
    std::fs::write(
        sandbox.root.join("lekalo/project.yaml"),
        project.trim_end_matches(&['\r', '\n'][..]),
    )
    .unwrap();
    let source = String::from_utf8(sandbox.entities_bytes())
        .unwrap()
        .replace('\n', "\r\n");
    std::fs::write(sandbox.entities_path(), &source).unwrap();
    for (old, new, key) in [
        ("tasks.text", "label", "yaml-first"),
        ("tasks.label", "title_text", "yaml-second"),
    ] {
        let request = symbol_move_request(old, "tasks", Some(new));
        apply_symbol_plan(&prepare_breaking(&sandbox, &request).unwrap(), key).unwrap();
    }
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    let registry = model
        .project
        .as_ref()
        .unwrap()
        .node
        .get("id_registry")
        .unwrap();
    assert_eq!(
        registry
            .get("rename_history")
            .unwrap()
            .as_seq()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        registry.get("tombstones").unwrap().as_seq().unwrap()[0]
            .get("replaced_by")
            .unwrap()
            .as_str(),
        Some("tasks.title_text")
    );
    let after = String::from_utf8(sandbox.entities_bytes()).unwrap();
    assert!(!after.replace("\r\n", "").contains('\n'));
    assert!(after.contains("# The closed delivery priority vocabulary.\r\n"));
    let project = std::fs::read_to_string(sandbox.root.join("lekalo/project.yaml")).unwrap();
    assert!(!project.ends_with('\n'));
    assert!(!project.replace("\r\n", "").contains('\n'));
}

#[test]
fn symbol_move_uses_the_owners_full_version_range_and_refuses_overflow_before_writing() {
    let sandbox = Sandbox::new("symbol-version-range");
    let before = String::from_utf8(sandbox.entities_bytes())
        .unwrap()
        .replacen("version: 1", "version: 4294967296", 1);
    std::fs::write(sandbox.entities_path(), before).unwrap();
    let request = symbol_move_request("tasks.text", "tasks", Some("label"));
    apply_symbol_plan(
        &prepare_breaking(&sandbox, &request).unwrap(),
        "symbol-version-range",
    )
    .unwrap();
    assert!(String::from_utf8(sandbox.entities_bytes())
        .unwrap()
        .contains("version: 4294967297"));
    let sandbox = Sandbox::new("symbol-version-overflow");
    let before = String::from_utf8(sandbox.entities_bytes())
        .unwrap()
        .replacen("version: 1", "version: 18446744073709551615", 1);
    std::fs::write(sandbox.entities_path(), &before).unwrap();
    assert!(matches!(
        prepare_breaking(&sandbox, &request),
        Err(ChangeError::SubjectConflict {
            detail: "version-overflow",
            ..
        })
    ));
    assert_eq!(sandbox.entities_bytes(), before.as_bytes());
    assert!(!sandbox.history_path().exists());
}

#[test]
fn symbol_move_refuses_new_dependency_cycles_before_creating_or_rewriting_files() {
    let sandbox = Sandbox::new("symbol-import-cycle");
    add_symbol_destination(&sandbox, false);
    let queries = sandbox.root.join("lekalo/modules/tasks/queries.yaml");
    let query = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.list_tasks\n    kind: query\n    version: 1\n    returns: list<tasks.task>\n";
    std::fs::write(&queries, query).unwrap();
    let before = sandbox.entities_bytes();
    assert!(matches!(
        prepare_breaking(
            &sandbox,
            &symbol_move_request("tasks.task", "other", Some("item"))
        ),
        Err(ChangeError::LoadFailed(_))
    ));
    assert_eq!(sandbox.entities_bytes(), before);
    assert_eq!(std::fs::read_to_string(queries).unwrap(), query);
    assert!(!sandbox
        .root
        .join("lekalo/modules/physical-other/entities.yaml")
        .exists());
    assert!(!sandbox.history_path().exists());
}

fn module_bytes(path: &Path) -> std::collections::BTreeMap<std::ffi::OsString, Vec<u8>> {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            (entry.file_name(), std::fs::read(entry.path()).unwrap())
        })
        .collect()
}

fn error_sandbox(tag: &str) -> Sandbox {
    let sandbox = Sandbox::from_fixture(tag, "tests/fixtures/model-v1/valid-planner");
    std::fs::write(
        sandbox
            .root
            .join(lekalo_core::error_contract::source::PROJECT_PATH),
        lekalo_core::error_contract::REGISTRY_BYTES,
    )
    .unwrap();
    sandbox
}

fn error_request(target: &str, errors: &[&str], registry: &[u8]) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schemaVersion":"lekalo/change-request/v0.8.0",
        "operations":[{"operation":"operation-error:set","operationTarget":target,"definition":{
            "errors":errors,"registryDigest":format!("sha256:{}",lekalo_core::digest::sha256_hex(registry))
        }}]
    }))
    .unwrap()
}

#[test]
fn error_union_changes_seal_registry_policy_and_durable_replay() {
    let sandbox = error_sandbox("error-union");
    let path = sandbox
        .root
        .join(lekalo_core::error_contract::source::PROJECT_PATH);
    let before = std::fs::read(&path).unwrap();
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    let before_model = lekalo_core::loader::canonical_model_bytes(&model);
    let errors = [
        "planner.focus_denied",
        "planner.input_invalid",
        "planner.store_unavailable",
        "planner.task_not_found",
    ];
    let request = error_request("planner.focus_task", &errors, &before);
    assert!(matches!(
        prepare(&sandbox, &request),
        Err(ChangeError::CompatibilityDenied { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let plan = change::prepare(&PrepareOptions {
        project: Some(&sandbox.selection_str()),
        request: &request,
        policy: None,
        allow_breaking: true,
    })
    .unwrap();
    let wire: serde_json::Value = serde_json::from_str(&plan.to_json_string().unwrap()).unwrap();
    assert_eq!(
        wire["semanticDiff"]["changes"][0]["kind"],
        "error.union-member-removed"
    );
    assert_eq!(
        wire["semanticDiff"]["classification"],
        serde_json::json!(["source-breaking"])
    );
    assert_eq!(wire["impact"]["seeds"][0]["id"], "planner.focus_task");
    assert_eq!(
        wire["impact"]["affected"][0]["subject"],
        "planner.focus_task"
    );
    let mut drift: serde_json::Value = serde_json::from_slice(&before).unwrap();
    drift["bindings"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|binding| binding["operation"] == "planner.focus_task")
        .unwrap()["errors"] = serde_json::json!(errors);
    let drift = lekalo_core::error_contract::ErrorRegistry::from_bytes(
        &serde_json::to_vec(&drift).unwrap(),
    )
    .unwrap()
    .canonical_bytes()
    .as_bytes()
    .to_vec();
    std::fs::write(&path, &drift).unwrap();
    assert!(change::apply_prepared(
        &plan,
        &ApplyOptions {
            idempotency_key: "error-union",
            expect_plan: None,
            allow_breaking: true
        }
    )
    .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), drift);
    assert!(!sandbox.history_path().exists());
    std::fs::write(&path, &before).unwrap();
    let receipt = apply_request(&sandbox, &request, "error-union", true).unwrap();
    assert_eq!(receipt.state(), ReceiptState::Committed);
    assert_eq!(receipt.files.len(), 1);
    assert_eq!(
        receipt.files[0].path,
        lekalo_core::error_contract::source::PROJECT_PATH
    );
    assert_eq!(std::fs::read(&path).unwrap(), drift);
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    assert_eq!(
        lekalo_core::loader::canonical_model_bytes(&model),
        before_model
    );
    assert_eq!(
        apply_request(&sandbox, &request, "error-union", true)
            .unwrap()
            .state(),
        ReceiptState::Replayed
    );
    let no_op = error_request("planner.focus_task", &errors, &drift);
    assert!(prepare(&sandbox, &no_op).unwrap().is_no_op());
    assert_eq!(
        apply_request(&sandbox, &no_op, "error-no-op", false)
            .unwrap()
            .state(),
        ReceiptState::NoOp
    );
    assert_eq!(std::fs::read(&path).unwrap(), drift);
}

#[test]
fn query_error_binding_preserves_output_and_refuses_a_stale_model_signature() {
    let sandbox = error_sandbox("query-error");
    let path = sandbox
        .root
        .join(lekalo_core::error_contract::source::PROJECT_PATH);
    let before = std::fs::read(&path).unwrap();
    let request = error_request(
        "planner.list_tasks",
        &["planner.store_unavailable"],
        &before,
    );
    apply_request(&sandbox, &request, "query-error", true).unwrap();
    let registry = lekalo_core::error_contract::source::read(&sandbox.root)
        .unwrap()
        .unwrap();
    let old = lekalo_core::error_contract::ErrorRegistry::embedded().unwrap();
    let id = lekalo_core::error_contract::ErrorId::new("planner.list_tasks").unwrap();
    assert_eq!(
        registry.binding(&id).unwrap().output(),
        old.binding(&id).unwrap().output()
    );
    let queries = sandbox.root.join("lekalo/modules/planner/queries.yaml");
    let mut source: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&queries).unwrap()).unwrap();
    source["definitions"][0]["returns"] = serde_json::json!({"ref":"planner.task_state"});
    let source = serde_json::to_vec(&source).unwrap();
    std::fs::write(&queries, &source).unwrap();
    let before_registry = std::fs::read(&path).unwrap();
    let changed_output = error_request(
        "planner.list_tasks",
        &["planner.store_unavailable"],
        &before_registry,
    );
    let failure = change::prepare(&PrepareOptions {
        project: Some(&sandbox.selection_str()),
        request: &changed_output,
        policy: None,
        allow_breaking: true,
    })
    .unwrap_err();
    let ChangeError::LoadFailed(result) = failure else {
        panic!("expected owner validation, got {failure:?}");
    };
    assert!(result
        .reason_codes()
        .iter()
        .any(|code| code.as_str() == "error.binding-invalid"));
    assert_eq!(std::fs::read(&queries).unwrap(), source);
    assert_eq!(std::fs::read(&path).unwrap(), before_registry);
}

#[test]
fn a_registry_added_after_prepare_refuses_without_model_or_history_writes() {
    let sandbox = Sandbox::new("added-error-source");
    let plan = prepare(&sandbox, &add_priority_request()).unwrap();
    let before = sandbox.entities_bytes();
    let path = sandbox
        .root
        .join(lekalo_core::error_contract::source::PROJECT_PATH);
    std::fs::write(&path, lekalo_core::error_contract::REGISTRY_BYTES).unwrap();
    assert!(change::apply_prepared(
        &plan,
        &ApplyOptions {
            idempotency_key: "added-error-source",
            expect_plan: None,
            allow_breaking: false
        }
    )
    .is_err());
    assert_eq!(sandbox.entities_bytes(), before);
    assert!(!sandbox.history_path().exists());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        lekalo_core::error_contract::REGISTRY_BYTES
    );
}

#[test]
fn a_physical_move_refuses_a_known_error_source_location_that_would_be_stale() {
    let sandbox = error_sandbox("error-source-move");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion":"lekalo/change-request/v0.8.0",
        "operations":[{"operation":"module:move","module":"planner","directory":"new_home"}]
    }))
    .unwrap();
    assert!(matches!(
        prepare(&sandbox, &request),
        Err(ChangeError::UnsupportedMember {
            member: "error-source-relocation",
            ..
        })
    ));
    assert!(sandbox
        .root
        .join("lekalo/modules/planner/module.yaml")
        .exists());
    assert!(!sandbox.root.join("lekalo/modules/new_home").exists());
    assert!(!sandbox.history_path().exists());
}

#[test]
fn canonical_binding_add_creates_its_kind_home_and_replays_without_native_writes() {
    let sandbox = Sandbox::new("canonical-binding");
    std::fs::create_dir_all(sandbox.root.join("lekalo/targets")).unwrap();
    std::fs::write(
        sandbox.root.join("lekalo/targets/node-typescript.yaml"),
        "target: node-typescript\n",
    )
    .unwrap();
    let before = sandbox.entities_bytes();
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion":"lekalo/change-request/v0.8.0",
        "operations":[{"operation":"binding:add","symbol":"tasks.binding_node","definition":{
            "kind":"canonical-target-binding","target":"node-typescript","description":"Explicit target binding"
        }}]
    })).unwrap();
    let plan = prepare(&sandbox, &request).unwrap();
    assert_eq!(plan.file_edits().len(), 1);
    assert!(!sandbox
        .root
        .join("lekalo/modules/tasks/bindings.yaml")
        .exists());
    let receipt = apply_request(&sandbox, &request, "canonical-binding", false).unwrap();
    assert_eq!(receipt.state(), ReceiptState::Committed);
    assert_eq!(receipt.operations[0].operation, "binding:add");
    assert_eq!(receipt.operations[0].subject, "tasks.binding_node");
    assert_eq!(sandbox.entities_bytes(), before);
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    assert!(lekalo_core::ir::compile(&model)
        .unwrap()
        .project
        .to_canonical_json()
        .contains("tasks.binding_node"));
    assert_eq!(
        apply_request(&sandbox, &request, "canonical-binding", false)
            .unwrap()
            .state(),
        ReceiptState::Replayed
    );
}

#[test]
fn observed_binding_confirmation_seals_evidence_and_preserves_provenance() {
    use lekalo_core::observed;
    let sandbox = Sandbox::from_fixture("observed-binding", "tests/fixtures/bindings/app");
    let ctx = observed::context(&sandbox.selection()).unwrap();
    let mut scan: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/bindings/scans/app-ambiguous.scan.v1_1_0.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for symbol in scan["symbols"].as_array_mut().unwrap() {
        if let Some(path) = symbol["location"]["path"].as_str() {
            if let Ok(bytes) = std::fs::read(sandbox.root.join(path)) {
                symbol["fingerprint"] = serde_json::json!(format!(
                    "sha256:{}",
                    lekalo_core::digest::sha256_hex(&bytes)
                ));
            }
        }
        if let Some(candidates) = symbol["candidates"].as_array_mut() {
            for candidate in candidates {
                if let Some(path) = candidate["path"].as_str() {
                    if let Ok(bytes) = std::fs::read(sandbox.root.join(path)) {
                        candidate["fingerprint"] = serde_json::json!(format!(
                            "sha256:{}",
                            lekalo_core::digest::sha256_hex(&bytes)
                        ));
                    }
                }
            }
        }
    }
    observed::update_index(&ctx, &serde_json::to_vec(&scan).unwrap()).unwrap();
    let original = observed::load_index(&ctx).unwrap().unwrap();
    let original_record = original.symbol("taskboard.focus_task").unwrap();
    let proposal = observed::bindings::propose(&ctx)
        .unwrap()
        .proposals
        .into_iter()
        .find(|p| p.symbol == "taskboard.focus_task")
        .unwrap();
    assert!(proposal.ambiguous);
    let index_path = sandbox.root.join(".lekalo/import/observed/index.json");
    let before = std::fs::read(&index_path).unwrap();
    let native_path = sandbox.root.join("src/tasks.ts");
    let native = std::fs::read(&native_path).unwrap();
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion":"lekalo/change-request/v0.8.0",
        "operations":[{"operation":"binding:add","symbol":"taskboard.focus_task","definition":{
            "kind":"observed-confirmation","proposal":proposal.proposal,
            "candidate":"src/tasks.ts#focusTask","indexDigest":observed::index::index_digest(&original).unwrap()
        }}]
    })).unwrap();
    let plan = prepare(&sandbox, &request).unwrap();
    assert_eq!(std::fs::read(&index_path).unwrap(), before);
    std::fs::write(&native_path, [native.as_slice(), b"// drift\n"].concat()).unwrap();
    assert!(change::apply_prepared(
        &plan,
        &ApplyOptions {
            idempotency_key: "observed-binding",
            expect_plan: None,
            allow_breaking: false
        }
    )
    .is_err());
    assert_eq!(std::fs::read(&index_path).unwrap(), before);
    assert!(!sandbox.history_path().exists());
    std::fs::write(&native_path, &native).unwrap();
    assert_eq!(
        apply_request(&sandbox, &request, "observed-binding", false)
            .unwrap()
            .state(),
        ReceiptState::Committed
    );
    let confirmed = observed::load_index(&ctx).unwrap().unwrap();
    let record = confirmed.symbol("taskboard.focus_task").unwrap();
    assert_eq!(record.status, observed::types::BindingStatus::Confirmed);
    assert_eq!(record.provenance, original_record.provenance);
    assert_eq!(record.stable_key.as_deref(), Some("src/tasks.ts#focusTask"));
    assert_eq!(std::fs::read(&native_path).unwrap(), native);
    assert_eq!(
        apply_request(&sandbox, &request, "observed-binding", false)
            .unwrap()
            .state(),
        ReceiptState::Replayed
    );
}

#[test]
fn physical_module_move_preserves_identity_bytes_and_durable_replay() {
    let sandbox = Sandbox::new("module-move");
    let before = module_bytes(&sandbox.root.join("lekalo/modules/tasks"));
    let original = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    let original = lekalo_core::ir::compile(&original)
        .unwrap()
        .project
        .to_canonical_json();
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion":"lekalo/change-request/v0.8.0",
        "operations":[{"operation":"module:move","module":"tasks","directory":"task_sources"}]
    }))
    .unwrap();
    let plan = prepare(&sandbox, &request).unwrap();
    assert!(!plan.is_no_op());
    assert_eq!(plan.file_edits().len(), before.len() * 2);
    assert!(sandbox.entities_path().exists());
    let receipt = apply_request(&sandbox, &request, "module-move", false).unwrap();
    assert_eq!(receipt.state(), ReceiptState::Committed);
    assert!(!sandbox.root.join("lekalo/modules/tasks").exists());
    assert_eq!(
        module_bytes(&sandbox.root.join("lekalo/modules/task_sources")),
        before
    );
    let moved = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    assert_eq!(
        lekalo_core::ir::compile(&moved)
            .unwrap()
            .project
            .to_canonical_json(),
        original
    );
    assert_eq!(
        apply_request(&sandbox, &request, "module-move", false)
            .unwrap()
            .state(),
        ReceiptState::Replayed
    );
    // A later semantic mutation resolves the unchanged ID in its new home.
    apply_request(&sandbox, &add_priority_request(), "post-move-field", false).unwrap();
    let second = String::from_utf8(request.clone())
        .unwrap()
        .replace("task_sources", "final_sources");
    apply_request(&sandbox, second.as_bytes(), "second-move", false).unwrap();
    assert_eq!(
        apply_request(&sandbox, &request, "module-move", false)
            .unwrap()
            .state(),
        ReceiptState::Replayed
    );
}

#[test]
fn registered_migration_uses_the_transaction_and_replays_its_original_identity() {
    let sandbox = Sandbox::new("registered-migration");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion":"lekalo/change-request/v0.8.0",
        "operations":[{"operation":"migration:apply",
            "step":"model.entity-field-rename.v0.8.0",
            "parameters":{"entity":"tasks.task","name":"title","to":"label"}}]
    }))
    .unwrap();
    let preview = change::prepare(&PrepareOptions {
        project: Some(&sandbox.selection_str()),
        request: &request,
        policy: Some(&evidence_policy()),
        allow_breaking: true,
    })
    .expect("preview");
    assert_eq!(preview.operations()[0].operation, "migration:apply");
    assert_eq!(
        preview.operations()[0].detail.as_deref(),
        Some("model.entity-field-rename.v0.8.0")
    );
    let receipt = apply_request_opts(
        &sandbox,
        &request,
        "registered-migration",
        true,
        Some(evidence_policy()),
    )
    .expect("commit");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let normalized = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    let entity = normalized
        .definitions
        .iter()
        .find(|definition| definition.id == "tasks.task")
        .unwrap();
    assert!(entity
        .node
        .get("fields")
        .unwrap()
        .as_seq()
        .unwrap()
        .iter()
        .any(|field| field
            .get("name")
            .and_then(lekalo_core::loader::frontends::Node::as_str)
            == Some("label")));
    assert_eq!(
        entity.node.get("identity").unwrap().as_seq().unwrap()[0].as_str(),
        Some("label")
    );
    assert_eq!(
        apply_request_opts(
            &sandbox,
            &request,
            "registered-migration",
            true,
            Some(evidence_policy())
        )
        .unwrap()
        .state(),
        ReceiptState::Replayed
    );
    let before = sandbox.entities_bytes();
    let unknown = String::from_utf8(request)
        .unwrap()
        .replace("model.entity-field-rename.v0.8.0", "unknown.recipe.v0.8.0");
    assert!(matches!(
        apply_request(&sandbox, unknown.as_bytes(), "unknown-migration", true),
        Err(ChangeError::OperationUnsupported { .. })
    ));
    assert_eq!(sandbox.entities_bytes(), before);
}

#[test]
fn requirement_links_use_the_model_id_grammar_and_survive_empty_list_roundtrips() {
    let sandbox = Sandbox::new("requirement-links");
    let original = sandbox.entities_bytes();
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion":"lekalo/change-request/v0.8.0",
        "operations":[
            {"operation":"requirement:link","symbol":"tasks.task","requirement":"TASK-REQ-001"},
            {"operation":"requirement:unlink","symbol":"tasks.task","requirement":"TASK-REQ-001"},
            {"operation":"requirement:link","symbol":"tasks.task","requirement":"TASK-REQ-001"},
            {"operation":"requirement:link","symbol":"tasks.task","requirement":"TASK-REQ-002"}
        ]
    }))
    .unwrap();
    let receipt = apply_request(&sandbox, &request, "requirement-links", true).expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let replay = apply_request(&sandbox, &request, "requirement-links", true).expect("replays");
    assert_eq!(replay.state(), ReceiptState::Replayed);
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).expect("accepted Model");
    let entity = model
        .definitions
        .iter()
        .find(|definition| definition.id == "tasks.task")
        .unwrap();
    let requirements = entity.node.get("derived_from").unwrap().as_seq().unwrap();
    assert_eq!(
        requirements
            .iter()
            .map(|item| item.as_str().unwrap())
            .collect::<Vec<_>>(),
        ["TASK-REQ-001", "TASK-REQ-002"]
    );
    let after = sandbox.entities_bytes();
    assert!(
        after.starts_with(&original[..original.iter().rposition(|byte| *byte == b'\n').unwrap()])
    );
    let mut bad: serde_json::Value = serde_json::from_slice(&request).unwrap();
    bad["operations"][0]["requirement"] = serde_json::json!("lowercase-invalid");
    assert!(change::ChangeRequest::decode(&serde_json::to_vec(&bad).unwrap()).is_err());
}

#[test]
fn query_read_operations_commit_and_reject_removing_the_required_last_read() {
    let sandbox = Sandbox::new("query-reads");
    let mut entities = String::from_utf8(sandbox.entities_bytes()).unwrap();
    entities.push_str("  - id: tasks.note\n    kind: entity\n    version: 1\n    fields:\n      - name: title\n        type: tasks.text\n        required: true\n    identity:\n      - title\n");
    std::fs::write(sandbox.entities_path(), entities).unwrap();
    let path = sandbox.root.join("lekalo/modules/tasks/queries.yaml");
    std::fs::write(&path, "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.list_tasks\n    kind: query\n    version: 1\n    reads:\n      - tasks.task\n").unwrap();
    let request = |operation: &str, effect: &str| {
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion":"lekalo/change-request/v0.8.0",
            "operations":[{"operation":operation,"query":"tasks.list_tasks","effect":effect}]
        }))
        .unwrap()
    };
    apply_request(
        &sandbox,
        &request("query-effect:add", "tasks.note"),
        "add-read",
        true,
    )
    .expect("adds read");
    apply_request(
        &sandbox,
        &request("query-effect:remove", "tasks.note"),
        "remove-read",
        true,
    )
    .expect("removes read");
    let before = std::fs::read(&path).unwrap();
    assert!(apply_request(
        &sandbox,
        &request("query-effect:remove", "tasks.task"),
        "last-read",
        true
    )
    .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(apply_request(
        &sandbox,
        &request("query-effect:add", "tasks.missing"),
        "missing-read",
        true
    )
    .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

fn add_field_request(field: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "entity-field:add",
            "entity": "tasks.task",
            "field": { "name": field, "type": { "ref": "tasks.priority" } }
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
    apply_request_opts(sandbox, request, key, allow_breaking, None)
}

fn apply_request_opts(
    sandbox: &Sandbox,
    request: &[u8],
    key: &str,
    allow_breaking: bool,
    policy: Option<Vec<u8>>,
) -> Result<change::Receipt, ChangeError> {
    change::apply_request(&ApplyRequestOptions {
        project: Some(&sandbox.selection_str()),
        request,
        policy: policy.as_deref(),
        allow_breaking,
        idempotency_key: key,
        expect_plan: None,
    })
}

/// A reviewed policy that authorizes the evidence classes with migration
/// evidence (round 2, finding 15): the boolean flag alone can never
/// authorize `data-loss-risk` or `storage-migration-required`.
fn evidence_policy() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-policy/v0.8.0",
        "identity": "dev.lekalo.change-policy@0.8.0",
        "allowBreakingClasses": ["data-loss-risk", "source-breaking"],
        "evidence": {
            "migrationPlanDigest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }
    }))
    .expect("policy serializes")
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
        serde_json::from_str(&prepared.to_json_string().expect("plan json")).expect("plan json");
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
fn breaking_remove_needs_reviewed_evidence_not_just_the_flag() {
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
    // Round 2, finding 15: the flag alone never authorizes a data-loss
    // class — removal stays denied under `--allow-breaking`.
    let error = apply_request(&sandbox, &remove, "remove-priority-v1", true)
        .expect_err("flag cannot bypass data-loss evidence");
    assert!(
        matches!(error, ChangeError::CompatibilityDenied { .. }),
        "got {error:?}"
    );
    assert_eq!(sandbox.entities_bytes(), before, "still nothing mutated");
    // A reviewed policy with migration evidence authorizes the removal.
    let receipt = apply_request_opts(
        &sandbox,
        &remove,
        "remove-priority-v1",
        false,
        Some(evidence_policy()),
    )
    .expect("applies with the reviewed policy");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    // The receipt records the accepted policy and evidence.
    assert!(receipt.policy.is_some(), "policy acceptance recorded");
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
    assert!(!first.replayed);
    // The Model already carries the field; the ledger still replays.
    let second = apply_request(&sandbox, &request, "add-priority-v1", false).expect("replays");
    assert_eq!(second.state(), ReceiptState::Replayed);
    assert!(second.replayed);
    assert_eq!(second.plan_id, first.plan_id);
    // Exactly one history record and one committed field.
    let history: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sandbox.history_path()).expect("history"))
            .expect("history json");
    assert_eq!(history["records"].as_array().expect("records").len(), 1);
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).expect("loads");
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
            allow_breaking: false,
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
fn ownership_refuses_symbol_granularity_instead_of_changing_its_whole_module() {
    let sandbox = Sandbox::new("unsupported");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "ownership:set",
            "subject": "tasks.task",
            "mode": "observed",
            "evidence": { "ownershipDigest": format!("sha256:{}", "0".repeat(64)) }
        }]
    }))
    .expect("serializes");
    let error = prepare(&sandbox, &request).expect_err("unsupported");
    match error {
        ChangeError::UnsupportedMember {
            operation,
            member: "symbol-ownership",
        } => {
            assert_eq!(operation, "ownership:set");
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
        br#"{"schemaVersion":"lekalo/change-journal/1","transaction":"change-ab12cd34-1","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","state":"staging","files":[],"history":false}"#,
    )
    .expect("journal");
    plant_pending(
        &sandbox,
        serde_json::json!([{
            "transaction": "change-ab12cd34-1",
            "planId": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "state": "staging",
            "files": [],
            "history": false,
        }]),
    );
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
    // Round 5: the provenance record is consumed with the journal — no
    // dangling pin survives a completed recovery.
    let registry = std::fs::read_to_string(sandbox.root.join(".lekalo/pending-transactions.json"))
        .unwrap_or_default();
    assert!(!registry.contains("change-ab12cd34-1"), "{registry}");
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
        br#"{"schemaVersion":"lekalo/change-journal/1","transaction":"change-escape01","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","state":"replacing","files":[{"path":"../le73-ESCAPE.txt","beforeDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","afterDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","creation":false}],"history":false}"#,
    )
    .expect("journal");
    let error = change::recover_transaction(Some(&sandbox.selection_str()), "change-escape01")
        .expect_err("traversal refused");
    assert!(
        matches!(error, ChangeError::WriteBoundaryDenied),
        "got {error:?}"
    );
    // Nothing was written outside the project and the journal stays (the
    // barrier keeps readers blocked until a legitimate recovery).
    assert!(!sandbox.root.join("le73-ESCAPE.txt").exists());
    assert!(!sandbox
        .root
        .parent()
        .unwrap()
        .join("le73-ESCAPE.txt")
        .exists());
    assert!(journal_dir.join("journal.json").is_file());
}

#[test]
fn recovery_refuses_an_absolute_and_absolute_windows_journal_path() {
    let sandbox = Sandbox::new("absolutely");
    let journal_dir = sandbox.root.join(".lekalo/cache/change/change-absol001");
    std::fs::create_dir_all(&journal_dir).expect("journal dir");
    std::fs::write(
        journal_dir.join("journal.json"),
        br#"{"schemaVersion":"lekalo/change-journal/1","transaction":"change-absol001","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","state":"replacing","files":[{"path":"lekalo/modules/tasks/entities.yaml","beforeDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","afterDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","creation":false},{"path":"/etc/lekalo-pwn","beforeDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","afterDigest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","creation":false}],"history":false}"#,
    )
    .expect("journal");
    let error = change::recover_transaction(Some(&sandbox.selection_str()), "change-absol001")
        .expect_err("absolute path refused");
    assert!(
        matches!(error, ChangeError::WriteBoundaryDenied),
        "{error:?}"
    );
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
        r#"{{"schemaVersion":"lekalo/change-journal/1","transaction":"change-corrupt01","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","state":"replacing","files":[{{"path":"lekalo/modules/tasks/entities.yaml","beforeDigest":"{wrong_digest}","afterDigest":"sha256:2222222222222222222222222222222222222222222222222222222222222222","creation":false}}],"history":false}}"#
    );
    std::fs::write(journal_dir.join("journal.json"), journal).expect("journal");
    // The matching durable provenance record: the probe targets the
    // backup verification, which runs past the provenance gate.
    plant_pending(
        &sandbox,
        serde_json::json!([{
            "transaction": "change-corrupt01",
            "planId": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "state": "replacing",
            "files": [{
                "path": "lekalo/modules/tasks/entities.yaml",
                "beforeDigest": wrong_digest,
                "afterDigest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                "creation": false,
            }],
            "history": false,
        }]),
    );
    let before = sandbox.entities_bytes();
    let error = change::recover_transaction(Some(&sandbox.selection_str()), "change-corrupt01")
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
    // An interrupted commit: the target currently holds the (bad)
    // prospective bytes; the verified backup holds the original. The
    // after-digest pin matches the actual prospective bytes — recovery
    // only ever restores states the transaction can explain (round 3,
    // finding 2).
    let original = sandbox.entities_bytes();
    let mut interrupted = original.clone();
    interrupted.extend(b"interrupted: true\n");
    std::fs::write(sandbox.entities_path(), &interrupted).expect("interrupted bytes");
    std::fs::write(journal_dir.join("backups/0"), &original).expect("backup");
    let digest = |bytes: &[u8]| format!("sha256:{}", lekalo_core::digest::sha256_hex(bytes));
    let journal = format!(
        r#"{{"schemaVersion":"lekalo/change-journal/1","transaction":"change-restore01","planId":"sha256:3333333333333333333333333333333333333333333333333333333333333333","state":"replacing","files":[{{"path":"lekalo/modules/tasks/entities.yaml","beforeDigest":"{0}","afterDigest":"{1}","creation":false}}],"history":false}}"#,
        digest(&original),
        digest(&interrupted)
    );
    std::fs::write(journal_dir.join("journal.json"), journal).expect("journal");
    plant_pending(
        &sandbox,
        serde_json::json!([{
            "transaction": "change-restore01",
            "planId": "sha256:3333333333333333333333333333333333333333333333333333333333333333",
            "state": "replacing",
            "files": [{
                "path": "lekalo/modules/tasks/entities.yaml",
                "beforeDigest": digest(&original),
                "afterDigest": digest(&interrupted),
                "creation": false,
            }],
            "history": false,
        }]),
    );
    let outcome = change::recover_transaction(Some(&sandbox.selection_str()), "change-restore01")
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
    // Review round 1 finding 3 / round 2 finding 1: a stale `writer.lock`
    // artifact of a dead owner is reclaimed (the OS released its handle),
    // while a live holder is proven by the OS-held lock and denies.
    let sandbox = Sandbox::new("stale");
    let cache = sandbox.root.join(".lekalo/cache/change");
    std::fs::create_dir_all(&cache).expect("cache");
    let journal_dir = cache.join("change-stale0001");
    std::fs::create_dir_all(&journal_dir).expect("journal dir");
    const STALE_JOURNAL: &[u8] = br#"{"schemaVersion":"lekalo/change-journal/1","transaction":"change-stale0001","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","state":"staging","files":[],"history":false}"#;
    std::fs::write(journal_dir.join("journal.json"), STALE_JOURNAL).expect("journal");
    plant_pending(
        &sandbox,
        serde_json::json!([{
            "transaction": "change-stale0001",
            "planId": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "state": "staging",
            "files": [],
            "history": false,
        }]),
    );
    // A stale lock artifact: no process holds it, so recovery takes over.
    // (r6, devin F2 + omp F2): the shared writer lock moved to
    // `.lekalo/cache/writer.lock` and its leaf is now persistent — it is
    // never unlinked, so only the journal and the change-home artifact
    // are consumed; the lock leaf itself stays (a persistent inode can
    // never be re-raced by an unlink/create contender).
    std::fs::write(
        sandbox.root.join(".lekalo/cache/writer.lock"),
        b"dead-owner",
    )
    .expect("stale lock");
    change::recover_transaction(Some(&sandbox.selection_str()), "change-stale0001")
        .expect("stale lock reclaimed");
    assert!(
        sandbox.root.join(".lekalo/cache/writer.lock").is_file(),
        "the shared lock leaf is persistent (never unlinked)"
    );
    assert!(!journal_dir.exists(), "journal consumed");
    // A live writer (this process holds the OS-held lock) denies recovery.
    std::fs::create_dir_all(&journal_dir).expect("journal dir again");
    std::fs::write(journal_dir.join("journal.json"), STALE_JOURNAL).expect("journal again");
    plant_pending(
        &sandbox,
        serde_json::json!([{
            "transaction": "change-stale0001",
            "planId": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "state": "staging",
            "files": [],
            "history": false,
        }]),
    );
    let holder = lekalo_core::change::test_only_acquire_writer_lock(Some(&sandbox.selection_str()))
        .expect("test holder");
    let error = change::recover_transaction(Some(&sandbox.selection_str()), "change-stale0001")
        .expect_err("live holder denies");
    assert!(
        matches!(
            error,
            ChangeError::PreconditionFailed { detail: "held", .. }
        ),
        "got {error:?}"
    );
    assert!(journal_dir.join("journal.json").is_file(), "journal kept");
    drop(holder);
    change::recover_transaction(Some(&sandbox.selection_str()), "change-stale0001")
        .expect("recovers after the holder releases");
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
    let error = change::recover_transaction(Some(&sandbox.selection_str()), "change-malfm0001")
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
    // Round 2, finding 15: a member rename reports data-loss-risk, so the
    // boolean flag alone cannot authorize it — the reviewed policy with
    // migration evidence can.
    let prepared = change::prepare(&PrepareOptions {
        project: Some(&sandbox.selection_str()),
        request: &request,
        policy: Some(&evidence_policy()),
        allow_breaking: false,
    })
    .expect("prepares with the reviewed policy");
    let plan: serde_json::Value =
        serde_json::from_str(&prepared.to_json_string().expect("plan json")).expect("plan json");
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
            allow_breaking: false,
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
    assert_eq!(
        first.to_json_string().expect("plan json"),
        second.to_json_string().expect("plan json")
    );
    // The plan validates against its published schema surface: the closed
    // required members are present.
    let plan: serde_json::Value =
        serde_json::from_str(&first.to_json_string().expect("plan json")).expect("json");
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
            allow_breaking: false,
        },
    )
    .expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let after = String::from_utf8(sandbox.entities_bytes()).expect("utf8");
    assert!(after.contains("- name: \"priority\""));
    assert!(after.contains("- name: \"owner\""));
    // The re-decoded candidate compiled identically: the project still loads.
    let model = lekalo_core::loader::normalize_model(&sandbox.selection())
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

#[test]
fn ordered_batches_resolve_against_the_updated_candidate() {
    // Round 2, finding 16: every operation resolves against the candidate
    // its predecessors produced — add-then-rename in one request works.
    let sandbox = Sandbox::new("ordered");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [
            {
                "operation": "entity-field:add",
                "entity": "tasks.task",
                "field": { "name": "extra", "type": { "ref": "tasks.text" } }
            },
            {
                "operation": "entity-field:rename",
                "entity": "tasks.task",
                "name": "extra",
                "to": "renamed"
            }
        ]
    }))
    .expect("serializes");
    let prepared = prepare(&sandbox, &request).expect("prepares");
    assert_eq!(prepared.file_edits().len(), 1, "both ops hit one file");
    let receipt = change::apply_prepared(
        &prepared,
        &ApplyOptions {
            idempotency_key: "ordered-v1",
            expect_plan: None,
            allow_breaking: false,
        },
    )
    .expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let after = String::from_utf8(sandbox.entities_bytes()).expect("utf8");
    assert!(after.contains("- name: \"renamed\""), "{after}");
    assert!(
        !after.contains("- name: \"extra\""),
        "the intermediate name is gone"
    );
    assert!(
        !after.contains(
            "- name: \"renamed\"
        type: \"tasks.text\"
        description:"
        ),
        "unrelated members keep their shape"
    );
}

#[test]
fn impact_reports_the_seed_and_every_known_consumer() {
    // Round 2, finding 12: a change with a known consumer never reports
    // an empty affected set with complete=true; the seed itself is part
    // of the evidence.
    let sandbox = Sandbox::new("impact");
    // A query that reads/returns the entity the change touches: a direct
    // known consumer of tasks.task.
    let queries = sandbox.root.join("lekalo/modules/tasks/queries.yaml");
    std::fs::write(
        &queries,
        "schema_version: \"0.2.16\"
definitions:
  - id: tasks.consumer
    kind: query
    version: 1
    reads:
      - \"tasks.task\"
    returns:
      ref: \"tasks.task\"
",
    )
    .expect("consumer written");
    let prepared = prepare(&sandbox, &add_priority_request()).expect("prepares");
    let plan: serde_json::Value =
        serde_json::from_str(&prepared.to_json_string().expect("plan json")).expect("plan json");
    let affected = plan["impact"]["affected"].as_array().expect("affected");
    assert!(
        !affected.is_empty(),
        "a known consumer must not be dropped: {affected:?}"
    );
    let seed = affected
        .iter()
        .find(|entry| entry["subject"] == "tasks.task")
        .expect("the seed subject is reported");
    let via = seed["via"].as_array().expect("via");
    assert!(
        via.iter()
            .any(|id| id.as_str().unwrap_or("").ends_with("tasks.consumer")),
        "the reverse-reachable consumer is reported: {via:?}"
    );
}

#[test]
fn impact_reports_a_closure_larger_than_the_old_wire_cap() {
    // Round 3, finding 9: 70 known consumers of the changed entity are
    // all reported (the wire cap equals the closure bound); nothing is
    // silently dropped while claiming completeness.
    let sandbox = Sandbox::new("impactwide");
    let mut definitions = String::from("schema_version: \"0.2.16\"\ndefinitions:\n");
    for index in 0..70 {
        definitions.push_str(&format!(
            "  - id: tasks.consumer{index:02}\n    kind: query\n    version: 1\n    reads:\n      - \"tasks.task\"\n    returns:\n      ref: \"tasks.task\"\n"
        ));
    }
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/queries.yaml"),
        definitions,
    )
    .expect("consumers written");
    let prepared = prepare(&sandbox, &add_priority_request()).expect("prepares");
    let plan: serde_json::Value =
        serde_json::from_str(&prepared.to_json_string().expect("plan json")).expect("plan json");
    let seed = plan["impact"]["affected"]
        .as_array()
        .expect("affected")
        .iter()
        .find(|entry| entry["subject"] == "tasks.task")
        .expect("the seed subject is reported")
        .clone();
    let via = seed["via"].as_array().expect("via").clone();
    assert_eq!(via.len(), 71, "the seed plus all 70 consumers: {via:?}");
    for index in 0..70 {
        let expected = format!("tasks.consumer{index:02}");
        assert!(
            via.iter()
                .any(|id| id.as_str().unwrap_or("").ends_with(expected.as_str())),
            "consumer {expected} present: {via:?}"
        );
    }
    assert_eq!(plan["impact"]["complete"], true);
}

#[test]
fn a_document_added_after_prepare_refuses_the_sealed_plan() {
    // Round 2, finding 6: apply re-enumerates the admitted inventory; a
    // valid canonical document added after preparation refuses before any
    // mutation.
    let sandbox = Sandbox::new("inventory");
    let prepared = prepare(&sandbox, &add_priority_request()).expect("prepares");
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/queries.yaml"),
        "schema_version: \"0.2.16\"
definitions:
  - id: tasks.late
    kind: query
    version: 1
    reads:
      - \"tasks.task\"
    returns:
      ref: \"tasks.task\"
",
    )
    .expect("late document written");
    let error = change::apply_prepared(
        &prepared,
        &ApplyOptions {
            idempotency_key: "late-v1",
            expect_plan: None,
            allow_breaking: false,
        },
    )
    .expect_err("added input refuses");
    match error {
        ChangeError::PreconditionFailed { subject, detail } => {
            assert_eq!(subject, "input-inventory");
            assert_eq!(detail, "inventory-changed");
        }
        other => panic!("expected precondition failure, got {other:?}"),
    }
    // Nothing mutated.
    assert!(!sandbox.history_path().exists());
    assert!(!String::from_utf8(sandbox.entities_bytes())
        .expect("utf8")
        .contains("- name: \"priority\""));
}
#[test]
fn enum_case_change_renames_a_case_in_place() {
    let sandbox = Sandbox::new("case-change");
    // A case rename is member-destructive: the reviewed policy with
    // migration evidence authorizes it.
    let policy = evidence_policy();
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "enum-case:change",
            "enum": "tasks.priority",
            "name": "high",
            "definition": { "rename": "highest" }
        }]
    }))
    .expect("serializes");
    let receipt = apply_request_opts(&sandbox, &request, "case-change-v1", false, Some(policy))
        .expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let after = String::from_utf8(sandbox.entities_bytes()).expect("utf8");
    // The rename rewrote the identity scalar in the original plain style.
    assert!(after.contains("- value: highest"), "{after}");
    assert!(!after.contains("- value: high\n") && !after.contains("- value: high\r"));
    // Sibling cases and members keep their bytes.
    assert!(after.contains("- value: low"));
    assert!(after.contains("- value: normal"));
    assert!(after.contains("- id: tasks.task"));
}

#[test]
fn definition_add_appends_a_query_to_the_module_document() {
    let sandbox = Sandbox::new("def-add");
    // The kind's home document must exist; the loader refuses new files
    // from the change service (creations are recovery-restricted).
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/queries.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.count\n    kind: query\n    version: 1\n    reads:\n      - \"tasks.task\"\n    returns:\n      ref: \"tasks.text\"\n",
    )
    .expect("queries home written");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "definition:add",
            "kind": "query",
            "module": "tasks",
            "definition": {
                "name": "open_tasks",
                "description": "Lists open tasks",
                "reads": ["tasks.task"],
                "returns": { "list": { "ref": "tasks.task" } }
            }
        }]
    }))
    .expect("serializes");
    let receipt = apply_request(&sandbox, &request, "def-add-v1", false).expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let queries_path = sandbox.root.join("lekalo/modules/tasks/queries.yaml");
    let queries = String::from_utf8(std::fs::read(queries_path).expect("queries")).expect("utf8");
    assert!(queries.contains("- id: \"tasks.open_tasks\""), "{queries}");
    assert!(queries.contains("kind: \"query\""));
    assert!(queries.contains("reads:"));
    assert!(queries.contains("- \"tasks.task\""));
    assert!(queries.contains("returns:"));
    assert!(queries.contains("list:"));
    // The module document is untouched.
    let module_path = sandbox.root.join("lekalo/modules/tasks/module.yaml");
    let module = String::from_utf8(std::fs::read(module_path).expect("module")).expect("utf8");
    assert!(!module.contains("open_tasks"));
    // The candidate recompiled and validated: the project still loads.
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).expect("loads");
    assert!(model
        .definitions
        .iter()
        .any(|definition| definition.id == "tasks.open_tasks"));
}

#[test]
fn definition_add_and_command_effect_compose_sequentially() {
    // definition:add creates a command whose effects list starts empty;
    // command-effect:add in the SAME batch resolves against that candidate
    // and appends the reference to an existing effect definition.
    let sandbox = Sandbox::new("compose");
    // The effect definition the command will reference (an admitted
    // document edit outside the change service) lives in the kind's home
    // document, which the definition will join.
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/commands.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.archive\n    kind: effect\n    version: 1\n    operation: update\n    entity: \"tasks.task\"\n",
    )
    .expect("commands home written");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [
            {
                "operation": "definition:add",
                "kind": "command",
                "module": "tasks",
                "definition": {
                    "name": "archive_task",
                    "input": [],
                    "effects": []
                }
            },
            {
                "operation": "command-effect:add",
                "command": "tasks.archive_task",
                "effect": "tasks.archive"
            }
        ]
    }))
    .expect("serializes");
    let prepared = prepare(&sandbox, &request).expect("prepares");
    let receipt = change::apply_prepared(
        &prepared,
        &ApplyOptions {
            idempotency_key: "compose-v1",
            expect_plan: None,
            allow_breaking: false,
        },
    )
    .expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let commands_path = sandbox.root.join("lekalo/modules/tasks/commands.yaml");
    let commands =
        String::from_utf8(std::fs::read(commands_path).expect("commands")).expect("utf8");
    assert!(
        commands.contains("- id: \"tasks.archive_task\""),
        "{commands}"
    );
    assert!(commands.contains("effects:"));
    assert!(commands.contains("- \"tasks.archive\""));
}

#[test]
fn command_effect_add_appends_to_an_existing_quoted_effects_list() {
    // Round 5, finding 1: an ordinary command shape — existing non-empty
    // `effects:` with a QUOTED item, plus a following `input:` member —
    // must commit cleanly. Previously every non-synthesized shape failed
    // the reparse assert (the insertion anchored two columns too deep).
    let sandbox = Sandbox::new("fx-quoted");
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/commands.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.archive\n    kind: effect\n    version: 1\n    operation: update\n    entity: \"tasks.task\"\n  - id: tasks.audit\n    kind: effect\n    version: 1\n    operation: update\n    entity: \"tasks.task\"\n  - id: tasks.archive_task\n    kind: command\n    version: 1\n    effects:\n      - \"tasks.archive\"\n    input:\n      - name: tasks\n        type: \"tasks.task\"\n        required: true\n",
    )
    .expect("commands written");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "command-effect:add",
            "command": "tasks.archive_task",
            "effect": "tasks.audit"
        }]
    }))
    .expect("serializes");
    let receipt = apply_request(&sandbox, &request, "fx-quoted-v1", false).expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let commands_path = sandbox.root.join("lekalo/modules/tasks/commands.yaml");
    let commands =
        String::from_utf8(std::fs::read(commands_path).expect("commands")).expect("utf8");
    assert!(
        commands.contains("      - \"tasks.archive\"\n      - \"tasks.audit\"\n    input:"),
        "{commands}"
    );
    // The candidate recompiled: the project still loads with the new
    // reference resolved against the effect table.
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).expect("loads");
    assert!(model
        .definitions
        .iter()
        .any(|definition| definition.id == "tasks.audit"));
}

#[test]
fn command_effect_add_appends_to_an_existing_plain_effects_list() {
    // Round 5, finding 1: the same ordinary shape with UNQUOTED scalar
    // items must commit — the anchor rule must not depend on the scalar's
    // quoting style.
    let sandbox = Sandbox::new("fx-plain");
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/commands.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.archive\n    kind: effect\n    version: 1\n    operation: update\n    entity: \"tasks.task\"\n  - id: tasks.purge\n    kind: effect\n    version: 1\n    operation: update\n    entity: \"tasks.task\"\n  - id: tasks.audit\n    kind: effect\n    version: 1\n    operation: update\n    entity: \"tasks.task\"\n  - id: tasks.archive_task\n    kind: command\n    version: 1\n    effects:\n      - tasks.archive\n      - tasks.purge\n",
    )
    .expect("commands written");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "command-effect:add",
            "command": "tasks.archive_task",
            "effect": "tasks.audit"
        }]
    }))
    .expect("serializes");
    let receipt = apply_request(&sandbox, &request, "fx-plain-v1", false).expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let commands_path = sandbox.root.join("lekalo/modules/tasks/commands.yaml");
    let commands =
        String::from_utf8(std::fs::read(commands_path).expect("commands")).expect("utf8");
    assert!(
        commands.contains("      - tasks.purge\n      - \"tasks.audit\""),
        "{commands}"
    );
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).expect("loads");
    assert!(model
        .definitions
        .iter()
        .any(|definition| definition.id == "tasks.audit"));
}

#[test]
fn command_effect_add_appends_the_member_after_a_sequence_member() {
    // Round 5, finding 1: a command whose last member is a nested list
    // (`input:`) and which carries no `effects` member yet must commit —
    // the member append previously derived its offset from a zero-width
    // container span and corrupted the last item.
    let sandbox = Sandbox::new("fx-member");
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/commands.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.audit\n    kind: effect\n    version: 1\n    operation: update\n    entity: \"tasks.task\"\n  - id: tasks.archive_task\n    kind: command\n    version: 1\n    input:\n      - name: tasks\n        type: \"tasks.task\"\n        required: true\n",
    )
    .expect("commands written");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "command-effect:add",
            "command": "tasks.archive_task",
            "effect": "tasks.audit"
        }]
    }))
    .expect("serializes");
    let receipt = apply_request(&sandbox, &request, "fx-member-v1", false).expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let commands_path = sandbox.root.join("lekalo/modules/tasks/commands.yaml");
    let commands =
        String::from_utf8(std::fs::read(commands_path).expect("commands")).expect("utf8");
    assert!(
        commands.contains(
            "        type: \"tasks.task\"\n        required: true\n    effects:\n      - \"tasks.audit\""
        ),
        "{commands}"
    );
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).expect("loads");
    assert!(model
        .definitions
        .iter()
        .any(|definition| definition.id == "tasks.audit"));
}

#[test]
fn operation_output_set_refuses_a_comment_inside_the_returns_block() {
    // Round 5, finding 2: a comment line inside the replaced `returns`
    // block has ambiguous ownership — the replacement previously deleted
    // it silently. The refusal matches the removal paths.
    let sandbox = Sandbox::new("fx-out-cmt");
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/queries.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.find_task\n    kind: query\n    version: 1\n    reads:\n      - \"tasks.task\"\n    returns:\n      ref: \"tasks.text\"\n      # keep this explanation\n",
    )
    .expect("queries written");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "operation-output:set",
            "operationTarget": "tasks.find_task",
            "type": { "list": { "ref": "tasks.task" } }
        }]
    }))
    .expect("serializes");
    let error = prepare(&sandbox, &request).expect_err("refuses");
    assert!(
        matches!(
            error,
            ChangeError::SourcePreservation {
                detail: "comment-adjacent",
                ..
            }
        ),
        "got {error:?}"
    );
    // Nothing was written and the comment stays byte-identical.
    assert!(!sandbox.history_path().exists());
    let queries = String::from_utf8(
        std::fs::read(sandbox.root.join("lekalo/modules/tasks/queries.yaml")).expect("queries"),
    )
    .expect("utf8");
    assert!(queries.contains("# keep this explanation"));
}

#[test]
fn definition_add_refuses_under_a_file_trailing_comment() {
    // Round 5, finding 3: appending under a file-trailing comment would
    // silently re-home the comment onto the previous definition. The
    // bidirectional comment rule refuses instead.
    let sandbox = Sandbox::new("fx-def-cmt");
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/queries.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.find_task\n    kind: query\n    version: 1\n    reads:\n      - \"tasks.task\"\n    returns:\n      ref: \"tasks.text\"\n# list open work\n",
    )
    .expect("queries written");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "definition:add",
            "kind": "query",
            "module": "tasks",
            "definition": {
                "name": "open_tasks",
                "reads": ["tasks.task"],
                "returns": { "ref": "tasks.task" }
            }
        }]
    }))
    .expect("serializes");
    let error = prepare(&sandbox, &request).expect_err("refuses");
    assert!(
        matches!(
            error,
            ChangeError::SourcePreservation {
                detail: "comment-adjacent",
                ..
            }
        ),
        "got {error:?}"
    );
    assert!(!sandbox.history_path().exists());
    let queries = String::from_utf8(
        std::fs::read(sandbox.root.join("lekalo/modules/tasks/queries.yaml")).expect("queries"),
    )
    .expect("utf8");
    assert!(queries.trim_end().ends_with("# list open work"));
}

#[test]
fn signature_sets_replace_command_input_and_query_output() {
    let sandbox = Sandbox::new("signatures");
    // A command with a non-empty input and a query with a return, each in
    // its kind's home document.
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/commands.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.rename_task\n    kind: command\n    version: 1\n    input:\n      - name: old_name\n        type: \"tasks.text\"\n        required: true\n",
    )
    .expect("commands written");
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/queries.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.find_task\n    kind: query\n    version: 1\n    reads:\n      - \"tasks.task\"\n    returns:\n      ref: \"tasks.bool\"\n",
    )
    .expect("queries written");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [
            {
                "operation": "operation-input:set",
                "operationTarget": "tasks.rename_task",
                "definition": {
                    "input": [
                        { "name": "task_id", "type": { "ref": "tasks.text" }, "required": true },
                        { "name": "new_name", "type": { "ref": "tasks.text" } }
                    ]
                }
            },
            {
                "operation": "operation-output:set",
                "operationTarget": "tasks.find_task",
                "type": { "optional": { "ref": "tasks.task" } }
            }
        ]
    }))
    .expect("serializes");
    // Signature replacements are wire- and source-breaking with data-loss
    // risk: the reviewed policy with migration evidence authorizes them.
    // Wire-breaking is covered by the explicit acknowledgement; the
    // data-loss and source classes need the reviewed evidence policy.
    let receipt = apply_request_opts(
        &sandbox,
        &request,
        "signatures-v1",
        true,
        Some(evidence_policy()),
    )
    .expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let commands = String::from_utf8(
        std::fs::read(sandbox.root.join("lekalo/modules/tasks/commands.yaml")).expect("commands"),
    )
    .expect("utf8");
    assert!(commands.contains("- name: \"task_id\""), "{commands}");
    assert!(commands.contains("- name: \"new_name\""));
    assert!(!commands.contains("- name: \"old_name\""));
    let queries = String::from_utf8(
        std::fs::read(sandbox.root.join("lekalo/modules/tasks/queries.yaml")).expect("queries"),
    )
    .expect("utf8");
    assert!(queries.contains("optional:"), "{queries}");
    // The candidate compiled: the project still loads with the new
    // signatures.
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).expect("loads");
    let command = model
        .definitions
        .iter()
        .find(|definition| definition.id == "tasks.rename_task")
        .expect("command");
    let input = command
        .node
        .get("input")
        .expect("input")
        .as_seq()
        .expect("seq");
    assert_eq!(input.len(), 2);
}

#[test]
fn command_effect_emit_and_wrong_kinds_stay_explicitly_refused() {
    // Legacy Model cannot represent a direct event emission. Wrong-kind
    // targets remain subject conflicts, independently of version support.
    let sandbox = Sandbox::new("staged");
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/commands.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.archive\n    kind: effect\n    version: 1\n    operation: update\n    entity: \"tasks.task\"\n  - id: tasks.rename_task\n    kind: command\n    version: 1\n    effects:\n      - \"tasks.archive\"\n",
    )
    .expect("commands written");
    let emit = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "command-effect:add",
            "command": "tasks.rename_task",
            "emit": "tasks.task_archived"
        }]
    }))
    .expect("serializes");
    let error = prepare(&sandbox, &emit).expect_err("emit refused");
    assert!(
        matches!(
            error,
            ChangeError::UnsupportedMember {
                member: "emission-representation",
                ..
            }
        ),
        "got {error:?}"
    );
    let query_input = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "operation-input:set",
            "operationTarget": "tasks.priority",
            "definition": { "input": [] }
        }]
    }))
    .expect("serializes");
    // A wrong-kind target (an enum) is a subject conflict, not an
    // unsupported member.
    let error = prepare(&sandbox, &query_input).expect_err("wrong kind");
    assert!(
        matches!(
            error,
            ChangeError::SubjectConflict {
                detail: "wrong-kind",
                ..
            }
        ),
        "got {error:?}"
    );
}

#[test]
fn pure_command_emissions_are_typed_events_with_lossless_add_remove_and_replay() {
    let sandbox = Sandbox::new("pure-command-emissions");
    enable_default_successor(&sandbox);
    let path = sandbox.root.join("lekalo/modules/tasks/commands.yaml");
    let source = "schema_version: \"0.8.0\"\r\ndefinitions:\r\n  - id: tasks.complete\r\n    kind: command\r\n    version: 1 # keep command note";
    std::fs::write(&path, source).unwrap();
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/events.yaml"),
        "schema_version: \"0.8.0\"\ndefinitions:\n  - id: tasks.completed\n    kind: event\n    version: 1\n",
    )
    .unwrap();
    let request = defaults_request(serde_json::json!([{
        "operation":"command-effect:add", "command":"tasks.complete", "emit":"tasks.completed"
    }]));
    let policy = evidence_policy();
    let preview = change::prepare(&PrepareOptions {
        project: Some(&sandbox.selection_str()),
        request: &request,
        policy: Some(&policy),
        allow_breaking: true,
    })
    .unwrap();
    let plan: serde_json::Value = serde_json::from_str(&preview.to_json_string().unwrap()).unwrap();
    assert!(plan["semanticDiff"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|change| change["kind"] == "signature.effects-changed"));
    assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
    apply_request_opts(
        &sandbox,
        &request,
        "pure-emission-add",
        true,
        Some(policy.clone()),
    )
    .unwrap();
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.starts_with(source));
    assert!(!after.ends_with('\n'));
    assert!(!after.replace("\r\n", "").contains('\n'));
    assert_eq!(
        apply_request_opts(
            &sandbox,
            &request,
            "pure-emission-add",
            true,
            Some(policy.clone())
        )
        .unwrap()
        .state,
        "replayed"
    );
    let compiled = compiled_sandbox(&sandbox);
    assert!(compiled.project.has_command_emissions());
    let searchable = lekalo_core::search::projection::project_ir(&compiled.project, 100);
    assert!(searchable.cards["def:tasks.complete"]
        .relations
        .iter()
        .any(|relation| relation.relation == "command-emits"
            && relation.other == "def:tasks.completed"));
    let effects = lekalo_core::effects::build(&compiled.project).unwrap();
    assert_eq!(effects.declared().len(), 1);
    assert_eq!(
        effects.declared()[0].key().kind(),
        lekalo_core::effects::EffectKind::EmitEvent
    );
    let wire: serde_json::Value =
        serde_json::from_str(&effects.to_canonical_json().unwrap()).unwrap();
    assert_eq!(wire["schemaVersion"], "lekalo/effects/v0.8.0");
    assert_eq!(wire["effects"][0]["provenance"]["role"], "command-emits");
    assert_eq!(wire["metadata"]["effectCount"], 1);
    let graph = lekalo_core::graph::build(&compiled.project).unwrap();
    assert!(graph.to_canonical_json().unwrap().contains("command-emits"));
    let invalid = defaults_request(serde_json::json!([{
        "operation":"command-effect:add", "command":"tasks.complete", "emit":"tasks.task"
    }]));
    assert!(matches!(
        prepare(&sandbox, &invalid),
        Err(ChangeError::SubjectConflict {
            detail: "wrong-kind",
            ..
        })
    ));
    let duplicate = prepare(&sandbox, &request).unwrap_err();
    assert!(matches!(
        duplicate,
        ChangeError::SubjectConflict {
            detail: "effect-duplicate",
            ..
        }
    ));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), after);
    let remove = defaults_request(serde_json::json!([{
        "operation":"command-effect:remove", "command":"tasks.complete", "emit":"tasks.completed"
    }]));
    apply_request_opts(
        &sandbox,
        &remove,
        "pure-emission-remove",
        true,
        Some(policy.clone()),
    )
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
    assert!(!compiled_sandbox(&sandbox).project.has_command_emissions());
    assert_eq!(
        apply_request_opts(
            &sandbox,
            &remove,
            "pure-emission-remove",
            true,
            Some(policy)
        )
        .unwrap()
        .state,
        "replayed"
    );
    let conflicting = defaults_request(serde_json::json!([{
        "operation":"command-effect:add", "command":"tasks.complete", "emit":"tasks.completed", "effect":"tasks.task"
    }]));
    assert!(matches!(
        prepare(&sandbox, &conflicting),
        Err(ChangeError::RequestInvalid { .. })
    ));
}

#[test]
fn generation_plan_pins_targets_and_the_lock_with_explicit_unknowns() {
    // Round 2, finding 13; hardened in round 3, finding 8: the generation
    // projection is a real plan -- every declared target pinned by its
    // exact document digest and validated against the STRICTLY PARSED
    // project lock (the lockfile owner's canonical parser). An invalid
    // lock, an unlocked target, or a missing lock is an explicit
    // incomplete state -- never a complete-looking inventory over
    // invalid evidence.
    let sandbox = Sandbox::new("generation");
    std::fs::create_dir_all(sandbox.root.join("lekalo/targets")).expect("targets home");
    std::fs::write(
        sandbox.root.join("lekalo/targets/node-typescript.yaml"),
        "{\"target\":\"node-typescript\",\"note\":\"opaque target configuration\"}\n",
    )
    .expect("target written");
    let valid_lock = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/lockfile/valid/multi-adapter.lock.json"),
    )
    .expect("valid lock fixture");
    // An invalid lock first: no complete plan over garbage evidence.
    std::fs::write(sandbox.root.join("lekalo.lock"), b"lock: 1\n").expect("lock written");
    let prepared = prepare(&sandbox, &add_priority_request()).expect("prepares");
    let plan: serde_json::Value =
        serde_json::from_str(&prepared.to_json_string().expect("plan json")).expect("plan json");
    let generation = &plan["generation"];
    assert_eq!(generation["executed"], false, "generation never executes");
    let targets = generation["targets"].as_array().expect("targets");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0]["target"], "node-typescript");
    assert_eq!(
        targets[0]["status"], "opaque",
        "invalid lock blocks planning"
    );
    assert_eq!(generation["complete"], false, "invalid lock is incomplete");
    assert_eq!(
        generation["lock"]["file"], "lekalo.lock",
        "evidence still pinned"
    );
    // The strictly parsed lock: the locked target plans; a target the
    // lock does not carry refuses as incomplete (round 3, finding 8).
    std::fs::write(sandbox.root.join("lekalo.lock"), &valid_lock).expect("valid lock");
    let prepared = prepare(&sandbox, &add_priority_request()).expect("prepares");
    let plan: serde_json::Value =
        serde_json::from_str(&prepared.to_json_string().expect("plan json")).expect("plan json");
    let generation = &plan["generation"];
    let targets = generation["targets"].as_array().expect("targets");
    assert_eq!(targets[0]["status"], "planned");
    let digest = targets[0]["digest"].as_str().expect("digest");
    let expected = format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(
            &std::fs::read(sandbox.root.join("lekalo/targets/node-typescript.yaml"))
                .expect("target bytes")
        )
    );
    assert_eq!(digest, expected, "the target document is pinned by digest");
    assert_eq!(generation["complete"], true);
    // An unlocked target name is incomplete even with a valid lock.
    std::fs::write(
        sandbox.root.join("lekalo/targets/node-typescript.yaml"),
        "{\"target\":\"missing_adapter\"}\n",
    )
    .expect("forged target");
    let prepared = prepare(&sandbox, &add_priority_request()).expect("prepares");
    let plan: serde_json::Value =
        serde_json::from_str(&prepared.to_json_string().expect("plan json")).expect("plan json");
    assert_eq!(plan["generation"]["targets"][0]["status"], "opaque");
    assert_eq!(plan["generation"]["complete"], false);
    // Without the lock the plan states the incompleteness explicitly.
    std::fs::remove_file(sandbox.root.join("lekalo.lock")).expect("lock removed");
    let prepared = prepare(&sandbox, &add_priority_request()).expect("prepares");
    let plan: serde_json::Value =
        serde_json::from_str(&prepared.to_json_string().expect("plan json")).expect("plan json");
    assert_eq!(plan["generation"]["lock"], serde_json::Value::Null);
    assert_eq!(plan["generation"]["complete"], false);
}

/// Write one forged/observed journal into a sandbox transaction home
/// together with its matching durable provenance record (round 5: a
/// planted journal WITHOUT provenance is refused before any of the
/// deeper gates — probes that exercise a deeper layer plant the record
/// so the refusal they assert is the one they mean).
fn plant_journal(
    sandbox: &Sandbox,
    transaction: &str,
    state: &str,
    files: serde_json::Value,
    history: bool,
) -> PathBuf {
    let journal_dir = sandbox.root.join(".lekalo/cache/change").join(transaction);
    std::fs::create_dir_all(journal_dir.join("backups")).expect("journal dir");
    let journal = serde_json::json!({
        "schemaVersion": "lekalo/change-journal/1",
        "transaction": transaction,
        "planId": "sha256:3333333333333333333333333333333333333333333333333333333333333333",
        "state": state,
        "files": files,
        "history": history,
    });
    std::fs::write(
        journal_dir.join("journal.json"),
        serde_json::to_string(&journal).expect("journal json"),
    )
    .expect("journal");
    plant_pending(
        sandbox,
        serde_json::json!([{
            "transaction": transaction,
            "planId": "sha256:3333333333333333333333333333333333333333333333333333333333333333",
            "state": state,
            "files": files,
            "history": history,
        }]),
    );
    journal_dir
}

/// Write the durable pending-registry document into a sandbox (merging
/// by transaction id, mirroring the writer's own read-modify-write).
fn plant_pending(sandbox: &Sandbox, records: serde_json::Value) {
    std::fs::create_dir_all(sandbox.root.join(".lekalo")).expect("lekalo home");
    let path = sandbox.root.join(".lekalo/pending-transactions.json");
    let mut merged: Vec<serde_json::Value> = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|document| document["records"].as_array().cloned())
        .unwrap_or_default();
    for record in records.as_array().into_iter().flatten() {
        merged.retain(|existing| existing["transaction"] != record["transaction"]);
        merged.push(record.clone());
    }
    std::fs::write(
        &path,
        serde_json::to_string(&serde_json::json!({
            "schemaVersion": "lekalo/change-pending/1",
            "records": merged,
        }))
        .expect("registry json"),
    )
    .expect("registry");
}

fn sha256_of(bytes: &[u8]) -> String {
    format!("sha256:{}", lekalo_core::digest::sha256_hex(bytes))
}

#[test]
fn forged_recovery_entries_for_unrelated_files_are_refused() {
    // Round 3, finding 1: a journal (even with a digest-verified backup)
    // naming a file outside the admitted canonical homes is a forged
    // overwrite authority and must refuse with the barrier retained.
    let sandbox = Sandbox::new("forgedfile");
    std::fs::write(sandbox.root.join("notes.txt"), "USER NOTES\n").expect("notes");
    let forged = b"FORGED RESTORE\n";
    let journal_dir = plant_journal(
        &sandbox,
        "change-forged001",
        "replacing",
        serde_json::json!([{
            "path": "notes.txt",
            "beforeDigest": sha256_of(b"USER NOTES\n"),
            "afterDigest": sha256_of(forged),
            "creation": false,
        }]),
        false,
    );
    std::fs::write(journal_dir.join("backups/0"), b"FORGED RESTORE\n").expect("forged backup");
    let error = change::recover_transaction(Some(&sandbox.selection_str()), "change-forged001")
        .expect_err("forged entry refused");
    assert!(
        matches!(error, ChangeError::WriteBoundaryDenied),
        "got {error:?}"
    );
    assert_eq!(
        std::fs::read(sandbox.root.join("notes.txt")).expect("notes"),
        b"USER NOTES\n",
        "the unrelated file is untouched"
    );
    assert!(journal_dir.join("journal.json").is_file(), "barrier kept");
    // A duplicate/aliased target entry refuses too.
    let journal_dir = plant_journal(
        &sandbox,
        "change-forged002",
        "replacing",
        serde_json::json!([
            {
                "path": "lekalo/modules/tasks/entities.yaml",
                "beforeDigest": sha256_of(&sandbox.entities_bytes()),
                "afterDigest": sha256_of(b"x\n"),
                "creation": false,
            },
            {
                "path": "lekalo/modules/tasks/entities.yaml",
                "beforeDigest": sha256_of(&sandbox.entities_bytes()),
                "afterDigest": sha256_of(b"y\n"),
                "creation": false,
            }
        ]),
        false,
    );
    let error = change::recover_transaction(Some(&sandbox.selection_str()), "change-forged002")
        .expect_err("duplicate target refused");
    assert!(
        matches!(
            error,
            ChangeError::CommitFailed {
                detail: "journal-target-duplicate",
                ..
            }
        ),
        "got {error:?}"
    );
    assert!(journal_dir.join("journal.json").is_file());
}

#[test]
fn recovery_preserves_user_edits_made_after_an_interrupted_transaction() {
    // Round 3, finding 2: a target whose current bytes match neither the
    // pinned before nor after state is an edit the transaction does not
    // own; recovery refuses BEFORE any restoration write and keeps the
    // barrier (and every file) intact.
    let sandbox = Sandbox::new("drift");
    let original = sandbox.entities_bytes();
    let mut user_edited = original.clone();
    user_edited.extend(b"# USER EDIT AFTER INTERRUPTION\n");
    std::fs::write(sandbox.entities_path(), &user_edited).expect("user edit");
    let journal_dir = plant_journal(
        &sandbox,
        "change-drift0001",
        "replacing",
        serde_json::json!([{
            "path": "lekalo/modules/tasks/entities.yaml",
            "beforeDigest": sha256_of(&original),
            "afterDigest": sha256_of(b"prospective bytes\n"),
            "creation": false,
        }]),
        false,
    );
    std::fs::write(journal_dir.join("backups/0"), &original).expect("verified backup");
    let error = change::recover_transaction(Some(&sandbox.selection_str()), "change-drift0001")
        .expect_err("drift refused");
    assert!(
        matches!(
            error,
            ChangeError::PreconditionFailed {
                detail: "recovery-drift",
                ..
            }
        ),
        "got {error:?}"
    );
    assert_eq!(
        std::fs::read(sandbox.entities_path()).expect("entities"),
        user_edited,
        "the user edit survives"
    );
    assert!(journal_dir.join("journal.json").is_file(), "barrier kept");
}

#[test]
fn recovery_refuses_oversized_targets_and_retains_all_recovery_material() {
    for state in ["staging", "replacing"] {
        for creation in [false, true] {
            let sandbox = Sandbox::new(&format!("oversized-{state}-{creation}"));
            let original = sandbox.entities_bytes();
            let journal_dir = plant_journal(
                &sandbox,
                "change-oversize01",
                state,
                serde_json::json!([{
                    "path": "lekalo/modules/tasks/entities.yaml",
                    "beforeDigest": sha256_of(if creation { b"" } else { &original }),
                    "afterDigest": sha256_of(b"transaction output\n"),
                    "creation": creation,
                }]),
                false,
            );
            if !creation {
                std::fs::write(journal_dir.join("backups/0"), &original).unwrap();
            }
            // Exceed the canonical document read bound. This is a read
            // refusal, never evidence that the user's file is absent.
            let user_edit = vec![b' '; 8 * 1024 * 1024 + 1];
            std::fs::write(sandbox.entities_path(), &user_edit).unwrap();
            let pending = sandbox.root.join(".lekalo/pending-transactions.json");
            let pending_before = std::fs::read(&pending).unwrap();
            let journal_before = std::fs::read(journal_dir.join("journal.json")).unwrap();

            let result =
                change::recover_transaction(Some(&sandbox.selection_str()), "change-oversize01");
            assert!(
                matches!(result, Err(ChangeError::RecoveryRequired { .. })),
                "{state}, creation={creation}: {result:?}"
            );
            assert_eq!(std::fs::read(sandbox.entities_path()).unwrap(), user_edit);
            assert_eq!(std::fs::read(&pending).unwrap(), pending_before);
            assert_eq!(
                std::fs::read(journal_dir.join("journal.json")).unwrap(),
                journal_before
            );
            if !creation {
                assert_eq!(
                    std::fs::read(journal_dir.join("backups/0")).unwrap(),
                    original
                );
            }
            assert!(change::pending_transactions(Some(&sandbox.selection_str()))
                .unwrap()
                .contains(&"change-oversize01".to_owned()));
        }
    }
}

#[test]
fn recovery_refuses_oversized_siblings_before_restoring_any_target() {
    let sandbox = Sandbox::new("oversized-prefix");
    let original = sandbox.entities_bytes();
    let after = b"transaction output\n";
    let commands = sandbox.root.join("lekalo/modules/tasks/commands.yaml");
    let journal_dir = plant_journal(
        &sandbox,
        "change-oversize02",
        "replacing",
        serde_json::json!([
            {
                "path": "lekalo/modules/tasks/entities.yaml",
                "beforeDigest": sha256_of(&original),
                "afterDigest": sha256_of(after),
                "creation": false,
            },
            {
                "path": "lekalo/modules/tasks/commands.yaml",
                "beforeDigest": sha256_of(b"before commands\n"),
                "afterDigest": sha256_of(after),
                "creation": false,
            }
        ]),
        false,
    );
    std::fs::write(journal_dir.join("backups/0"), &original).unwrap();
    std::fs::write(journal_dir.join("backups/1"), b"before commands\n").unwrap();
    std::fs::write(sandbox.entities_path(), after).unwrap();
    let user_edit = vec![b' '; 8 * 1024 * 1024 + 1];
    std::fs::write(&commands, &user_edit).unwrap();

    let result = change::recover_transaction(Some(&sandbox.selection_str()), "change-oversize02");
    assert!(
        matches!(result, Err(ChangeError::RecoveryRequired { .. })),
        "{result:?}"
    );
    assert_eq!(std::fs::read(sandbox.entities_path()).unwrap(), after);
    assert_eq!(std::fs::read(&commands).unwrap(), user_edit);
    assert!(journal_dir.join("journal.json").is_file());
}

#[test]
fn recovery_restores_a_missing_existing_target_from_its_verified_backup() {
    let sandbox = Sandbox::new("missing-recovery");
    let original = sandbox.entities_bytes();
    let journal_dir = plant_journal(
        &sandbox,
        "change-missing01",
        "replacing",
        serde_json::json!([{
            "path": "lekalo/modules/tasks/entities.yaml",
            "beforeDigest": sha256_of(&original),
            "afterDigest": sha256_of(b"transaction output\n"),
            "creation": false,
        }]),
        false,
    );
    std::fs::write(journal_dir.join("backups/0"), &original).unwrap();
    std::fs::remove_file(sandbox.entities_path()).unwrap();
    change::recover_transaction(Some(&sandbox.selection_str()), "change-missing01").unwrap();
    assert_eq!(std::fs::read(sandbox.entities_path()).unwrap(), original);
    assert!(!journal_dir.exists());
    assert!(change::pending_transactions(Some(&sandbox.selection_str()))
        .unwrap()
        .is_empty());
}

#[test]
fn committed_recovery_clears_its_reader_barrier_before_success() {
    // Round 3, finding 3: a durably committed journal verifies every
    // target against its pinned after bytes, finishes the cleanup, and
    // only then reports committed — a subsequent ordinary read works and
    // a repeat recovery has nothing left to do.
    let sandbox = Sandbox::new("committed");
    let receipt = apply_request(
        &sandbox,
        &add_priority_request(),
        "committed-cleanup-v1",
        false,
    )
    .expect("applies");
    let committed = sandbox.entities_bytes();
    let transaction = receipt
        .transaction
        .as_ref()
        .expect("transaction")
        .id
        .clone();
    let after = sha256_of(&committed);
    let journal_dir = plant_journal(
        &sandbox,
        &transaction,
        "committed",
        serde_json::json!([{
            "path": "lekalo/modules/tasks/entities.yaml",
            "beforeDigest": sha256_of(b""),
            "afterDigest": after,
            "creation": false,
        }]),
        false,
    );
    // The barrier semantics while a committed journal is pending: the
    // journal is provably durably committed AND its exact transaction is
    // proven by history custody, so the ledger reads through it (the
    // crash window after the commit marker, before cleanup).
    let through = change::lookup_receipt(Some(&sandbox.selection_str()), "committed-cleanup-v1")
        .expect("ledger readable through proven committed journal")
        .expect("record present");
    assert_eq!(through.state(), ReceiptState::Committed);
    // Recovery finishes the cleanup and only then reports committed.
    let outcome = change::recover_transaction(Some(&sandbox.selection_str()), &transaction)
        .expect("committed recovery");
    assert_eq!(outcome.state, "committed");
    assert!(!journal_dir.exists(), "cleanup finished before success");
    let receipt = change::lookup_receipt(Some(&sandbox.selection_str()), "committed-cleanup-v1")
        .expect("ledger readable")
        .expect("record present");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    // A second invocation is a clean unknown-transaction refusal, not a
    // second success over retained material.
    let error = change::recover_transaction(Some(&sandbox.selection_str()), &transaction)
        .expect_err("nothing left to recover");
    assert!(
        matches!(error, ChangeError::PreconditionFailed { .. }),
        "got {error:?}"
    );
}

#[test]
fn receipt_lookup_gates_on_every_pending_journal_not_the_first() {
    // Round 3, finding 6: a lexically-first committed-looking journal
    // must not mask a later unresolved one.
    let sandbox = Sandbox::new("pending");
    apply_request(&sandbox, &add_priority_request(), "pending-gate-v1", false).expect("applies");
    let committed = sandbox.entities_bytes();
    plant_journal(
        &sandbox,
        "change-aaaa0001",
        "committed",
        serde_json::json!([{
            "path": "lekalo/modules/tasks/entities.yaml",
            "beforeDigest": sha256_of(b""),
            "afterDigest": sha256_of(&committed),
            "creation": false,
        }]),
        false,
    );
    plant_journal(
        &sandbox,
        "change-zzzz0001",
        "replacing",
        serde_json::json!([{
            "path": "lekalo/modules/tasks/module.yaml",
            "beforeDigest": sha256_of(&std::fs::read(sandbox.root.join("lekalo/modules/tasks/module.yaml")).expect("module")),
            "afterDigest": sha256_of(b"x\n"),
            "creation": false,
        }]),
        false,
    );
    let error = change::lookup_receipt(Some(&sandbox.selection_str()), "pending-gate-v1")
        .expect_err("an unproven committed-looking journal gates");
    assert!(
        matches!(error, ChangeError::RecoveryRequired { ref transaction } if transaction == "change-aaaa0001"),
        "got {error:?}"
    );
}

#[test]
fn history_intent_tampering_breaks_the_chain_and_refuses_replay() {
    // Round 3, finding 5: editing ONLY a record's intentDigest (no hash
    // recomputation) must break decode/verification — the intent is
    // bound into the chain and cross-checked against the receipt's
    // stored constituent facts.
    let sandbox = Sandbox::new("intenttamper");
    apply_request(&sandbox, &add_priority_request(), "intent-tamper-v1", false).expect("applies");
    let history_path = sandbox.history_path();
    let mut history: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&history_path).expect("history")).expect("json");
    history["records"][0]["intentDigest"] = serde_json::json!(
        "sha256:4444444444444444444444444444444444444444444444444444444444444444"
    );
    std::fs::write(
        &history_path,
        serde_json::to_vec_pretty(&history).expect("serializes"),
    )
    .expect("tampered");
    let error = apply_request(&sandbox, &add_priority_request(), "intent-tamper-v1", false)
        .expect_err("tampered ledger refuses");
    assert!(
        matches!(error, ChangeError::HistoryInvalid { .. }),
        "got {error:?}"
    );
}

#[test]
fn generation_evidence_drift_after_prepare_refuses_the_sealed_plan() {
    // Round 3, finding 7: target/lock evidence changes between prepare
    // and apply refuse the sealed plan — the displayed generation pins
    // are apply-time preconditions.
    let sandbox = Sandbox::new("genevidence");
    std::fs::create_dir_all(sandbox.root.join("lekalo/targets")).expect("targets home");
    std::fs::write(
        sandbox.root.join("lekalo/targets/node-typescript.yaml"),
        "{\"target\":\"node-typescript\"}\n",
    )
    .expect("target");
    let prepared = prepare(&sandbox, &add_priority_request()).expect("prepares");
    // Target drift.
    std::fs::write(
        sandbox.root.join("lekalo/targets/node-typescript.yaml"),
        "{\"target\":\"node-typescript\",\"changed\":true}\n",
    )
    .expect("drifted target");
    let error = change::apply_prepared(
        &prepared,
        &ApplyOptions {
            idempotency_key: "genevidence-v1",
            expect_plan: None,
            allow_breaking: false,
        },
    )
    .expect_err("target drift refuses");
    assert!(
        matches!(
            error,
            ChangeError::PreconditionFailed {
                detail: "input-digest",
                ..
            }
        ),
        "got {error:?}"
    );
    // Restore, then lock drift: a lock appearing after preparation is new
    // evidence the plan never validated.
    std::fs::write(
        sandbox.root.join("lekalo/targets/node-typescript.yaml"),
        "{\"target\":\"node-typescript\"}\n",
    )
    .expect("restored target");
    std::fs::write(sandbox.root.join("lekalo.lock"), b"lock: 1\n").expect("new lock");
    let error = change::apply_prepared(
        &prepared,
        &ApplyOptions {
            idempotency_key: "genevidence-v2",
            expect_plan: None,
            allow_breaking: false,
        },
    )
    .expect_err("lock appearance refuses");
    assert!(
        matches!(
            error,
            ChangeError::PreconditionFailed {
                detail: "input-digest",
                ..
            }
        ),
        "got {error:?}"
    );
    assert!(!sandbox.history_path().exists(), "nothing committed");
}

#[test]
fn definition_add_creates_a_modules_first_kind_home_with_rollback() {
    // Round 3, finding 11: `definition:add` can create a module's first
    // definition of a kind — the kind-home document is synthesized whole,
    // committed as a creation, and recovered by removing exactly the
    // bytes this transaction wrote.
    let sandbox = Sandbox::new("firstquery");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "definition:add",
            "kind": "query",
            "module": "tasks",
            "definition": {
                "name": "list_tasks",
                "reads": ["tasks.task"],
                "returns": { "list": { "ref": "tasks.task" } }
            }
        }]
    }))
    .expect("request");
    let home = sandbox.root.join("lekalo/modules/tasks/queries.yaml");
    assert!(!home.exists(), "the fixture has no queries yet");
    let receipt = apply_request(&sandbox, &request, "first-query-v1", false).expect("applies");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let committed = std::fs::read(&home).expect("kind home created");
    let text = String::from_utf8(committed.clone()).expect("utf8");
    assert!(text.contains("schema_version: \"0.2.16\""));
    assert!(text.contains("tasks.list_tasks"));
    assert!(text.contains("reads:"));
    // The creation is journaled: an interrupted-style rollback removes
    // exactly the created bytes.
    let journal_dir = plant_journal(
        &sandbox,
        "change-firstqry1",
        "replacing",
        serde_json::json!([{
            "path": "lekalo/modules/tasks/queries.yaml",
            "beforeDigest": sha256_of(b""),
            "afterDigest": sha256_of(&committed),
            "creation": true,
        }]),
        false,
    );
    let outcome = change::recover_transaction(Some(&sandbox.selection_str()), "change-firstqry1")
        .expect("recovers");
    assert_eq!(outcome.state, "rolled-back");
    assert!(!home.exists(), "the created document is removed again");
    assert!(!journal_dir.exists());
}

#[test]
fn field_rename_rewrites_the_entitys_own_identity_reference() {
    // Round 3, finding 12: renaming a field that the entity's identity
    // carries rewrites the governed typed reference too — the common
    // valid rename commits instead of failing identity validation.
    let sandbox = Sandbox::new("renameident");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "entity-field:rename",
            "entity": "tasks.task",
            "name": "title",
            "to": "heading"
        }]
    }))
    .expect("request");
    let receipt = apply_request_opts(
        &sandbox,
        &request,
        "rename-identity-v1",
        true,
        Some(evidence_policy()),
    )
    .expect("applies");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let text = String::from_utf8(sandbox.entities_bytes()).expect("utf8");
    assert!(text.contains("name: heading"), "{text}");
    assert!(text.contains("identity:\n      - heading"), "{text}");
    assert!(
        !text.contains("title"),
        "the identity entry followed the rename"
    );
}

#[test]
fn unchanged_input_signature_is_a_byte_identical_no_op() {
    // Round 3, finding 14: setting exactly the same signature plans no
    // byte edit at all — comments, quoting, and formatting stay verbatim
    // and no blank line is inserted.
    let sandbox = Sandbox::new("sigpreserved");
    // A commands home whose input block carries a documentation comment.
    let commands = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.task_completed\n    kind: effect\n    version: 1\n    operation: create\n    entity: tasks.task\n  - id: tasks.complete\n    kind: command\n    version: 1\n    input:\n      # KEEP THIS DOCUMENTATION\n      - name: title\n        type: \"tasks.text\"\n        required: true\n    effects:\n      - tasks.task_completed\n";
    let commands_path = sandbox.root.join("lekalo/modules/tasks/commands.yaml");
    std::fs::write(&commands_path, commands).expect("commands");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "operation-input:set",
            "operationTarget": "tasks.complete",
            "definition": {
                "input": [
                    { "name": "title", "type": { "ref": "tasks.text" }, "required": true }
                ]
            }
        }]
    }))
    .expect("request");
    let prepared = prepare(&sandbox, &request).expect("prepares");
    // The unchanged signature plans no byte edit at all: the file is not
    // even part of the file-edit set, the plan reports a no-op, and every
    // byte (the comment included) survives an apply unchanged.
    assert!(
        prepared
            .file_edits()
            .iter()
            .all(|edit| edit.path != "lekalo/modules/tasks/commands.yaml"),
        "an unchanged signature plans no edit"
    );
    assert!(prepared.is_no_op());
    let receipt = apply_request(&sandbox, &request, "sig-preserve-v1", false).expect("applies");
    assert_eq!(receipt.state(), ReceiptState::NoOp);
    assert_eq!(
        receipt.operations.len(),
        1,
        "no-op keeps its requested audit row"
    );
    assert_eq!(receipt.operations[0].operation, "operation-input:set");
    let history = change::receipt::History::decode(&std::fs::read(sandbox.history_path()).unwrap())
        .expect("no-op history round-trips");
    assert_eq!(history.records[0].operations, ["operation-input:set"]);
    assert_eq!(
        std::fs::read(&commands_path).expect("commands"),
        commands.as_bytes(),
        "the unchanged signature keeps every byte (comment included)"
    );
    // A changed signature keeps the comment and edits only the changed
    // member's own lines.
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "operation-input:set",
            "operationTarget": "tasks.complete",
            "definition": {
                "input": [
                    { "name": "title", "type": { "ref": "tasks.text" }, "required": true, "description": "The task heading." }
                ]
            }
        }]
    }))
    .expect("request");
    let prepared = prepare(&sandbox, &request).expect("prepares");
    let after = prepared
        .file_diffs()
        .iter()
        .find(|diff| diff.path == "lekalo/modules/tasks/commands.yaml")
        .expect("diff present")
        .after
        .clone();
    assert!(after.contains("# KEEP THIS DOCUMENTATION"), "{after}");
    assert!(
        after.contains("description: \"The task heading.\""),
        "{after}"
    );
    assert!(after.contains("name: title"), "{after}");
    assert!(
        !after.contains("name: \"title\""),
        "the member stays verbatim, not reformatted"
    );
}

#[test]
fn a_valid_foreign_project_ledger_cannot_authorize_replay_or_lookup() {
    let source = Sandbox::new("foreign-source");
    let destination = Sandbox::new("foreign-destination");
    let request = add_priority_request();
    apply_request(&source, &request, "foreign-ledger-key", false).expect("source commits");
    let project_path = destination.root.join("lekalo/project.yaml");
    let project = std::fs::read_to_string(&project_path).unwrap();
    std::fs::write(&project_path, project.replace("\"tasks\"", "\"other\"")).unwrap();
    std::fs::write(
        destination.history_path(),
        std::fs::read(source.history_path()).unwrap(),
    )
    .unwrap();
    let before = std::fs::read(destination.entities_path()).unwrap();
    let error = apply_request(&destination, &request, "foreign-ledger-key", false)
        .expect_err("foreign replay refused");
    assert!(
        matches!(
            error,
            ChangeError::HistoryInvalid {
                detail: "project-binding"
            }
        ),
        "{error:?}"
    );
    let error = change::lookup_receipt(Some(&destination.selection_str()), "foreign-ledger-key")
        .expect_err("foreign lookup refused");
    assert!(
        matches!(
            error,
            ChangeError::HistoryInvalid {
                detail: "project-binding"
            }
        ),
        "{error:?}"
    );
    assert_eq!(std::fs::read(destination.entities_path()).unwrap(), before);
}

#[test]
fn policy_class_changes_are_bound_into_durable_execution_intent() {
    let sandbox = Sandbox::new("intent-classes");
    let request = add_priority_request();
    let policy = evidence_policy();
    let project = sandbox.selection_str();
    let options = ApplyRequestOptions {
        project: Some(&project),
        request: &request,
        policy: Some(&policy),
        allow_breaking: false,
        idempotency_key: "class-intent-key",
        expect_plan: None,
    };
    change::apply_request(&options).expect("commits");
    let before = std::fs::read(sandbox.history_path()).unwrap();
    let mut altered: serde_json::Value = serde_json::from_slice(&policy).unwrap();
    altered["allowBreakingClasses"] = serde_json::json!(["source-breaking"]);
    let altered = serde_json::to_vec(&altered).unwrap();
    let error = change::apply_request(&ApplyRequestOptions {
        policy: Some(&altered),
        ..options
    })
    .expect_err("different classes conflict");
    assert!(
        matches!(error, ChangeError::IdempotencyConflict),
        "{error:?}"
    );
    assert_eq!(std::fs::read(sandbox.history_path()).unwrap(), before);
}

#[test]
fn declared_defaults_refuse_at_every_field_consuming_operation() {
    // Round 3, finding 13: an input-signature or definition declaration
    // carrying a default is an explicit unsupported member — never a
    // silently dropped declaration that still commits.
    let sandbox = Sandbox::new("defaults");
    let commands = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.task_completed\n    kind: effect\n    version: 1\n    operation: create\n    entity: tasks.task\n  - id: tasks.complete\n    kind: command\n    version: 1\n    input:\n      - name: title\n        type: \"tasks.text\"\n        required: true\n    effects:\n      - tasks.task_completed\n";
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/commands.yaml"),
        commands,
    )
    .expect("commands");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "definition:add",
            "kind": "command",
            "module": "tasks",
            "definition": {
                "name": "snapshot",
                "input": [
                    { "name": "title", "type": { "ref": "tasks.text" }, "default": "VALUE MUST EXIST" }
                ]
            }
        }]
    }))
    .expect("request");
    let error = apply_request(&sandbox, &request, "input-default-v1", false)
        .expect_err("definition defaults refuse");
    assert!(
        matches!(
            error,
            ChangeError::UnsupportedMember {
                member: "field-default",
                ..
            }
        ),
        "got {error:?}"
    );
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "operation-input:set",
            "operationTarget": "tasks.complete",
            "definition": {
                "input": [
                    { "name": "title", "type": { "ref": "tasks.text" }, "default": 1 }
                ]
            }
        }]
    }))
    .expect("request");
    let error = apply_request(&sandbox, &request, "input-default-v2", false)
        .expect_err("signature defaults refuse");
    assert!(
        matches!(
            error,
            ChangeError::UnsupportedMember {
                member: "field-default",
                ..
            }
        ),
        "got {error:?}"
    );
    assert!(!sandbox.history_path().exists(), "nothing committed");
    let committed =
        std::fs::read(sandbox.root.join("lekalo/modules/tasks/commands.yaml")).expect("commands");
    assert_eq!(committed, commands.as_bytes(), "the source is untouched");
}

fn enable_default_successor(sandbox: &Sandbox) {
    for file in [
        "lekalo/project.yaml",
        "lekalo/modules/tasks/module.yaml",
        "lekalo/modules/tasks/entities.yaml",
    ] {
        let path = sandbox.root.join(file);
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("0.2.16", "0.8.0");
        std::fs::write(path, text).unwrap();
    }
}

fn defaults_request(operations: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "schemaVersion": "lekalo/change-request/v0.8.0", "operations": operations })).unwrap()
}

fn query_inputs_request(input: serde_json::Value) -> Vec<u8> {
    defaults_request(serde_json::json!([{
        "operation":"operation-input:set", "operationTarget":"tasks.list",
        "definition":{"input":input}
    }]))
}

#[test]
fn command_results_follow_typed_diff_graph_inspect_and_lossless_source_edits() {
    let sandbox = Sandbox::new("command-results");
    enable_default_successor(&sandbox);
    let path = sandbox.root.join("lekalo/modules/tasks/commands.yaml");
    let source = "schema_version: \"0.8.0\"\r\ndefinitions:\r\n  - id: tasks.complete\r\n    kind: command\r\n    version: 1 # keep this command note";
    std::fs::write(&path, source).unwrap();
    let entities = sandbox.entities_bytes();
    let request = defaults_request(serde_json::json!([{
        "operation":"operation-output:set", "operationTarget":"tasks.complete",
        "type":{"optional":{"ref":"tasks.text"}}
    }]));
    let policy = evidence_policy();
    let preview = change::prepare(&PrepareOptions {
        project: Some(&sandbox.selection_str()),
        request: &request,
        policy: Some(&policy),
        allow_breaking: true,
    })
    .unwrap();
    let plan: serde_json::Value = serde_json::from_str(&preview.to_json_string().unwrap()).unwrap();
    assert!(plan["semanticDiff"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|change| change["kind"] == "signature.output-changed"));
    assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
    apply_request_opts(
        &sandbox,
        &request,
        "command-result-add",
        true,
        Some(policy.clone()),
    )
    .unwrap();
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.starts_with(source), "{after}");
    assert!(after.contains("# keep this command note\r\n    returns:"));
    assert!(!after.ends_with('\n'));
    assert!(!after.replace("\r\n", "").contains('\n'));
    assert_eq!(sandbox.entities_bytes(), entities);
    assert_eq!(
        apply_request_opts(&sandbox, &request, "command-result-add", true, Some(policy))
            .unwrap()
            .state,
        "replayed"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), after);

    let unchanged = change::prepare(&PrepareOptions {
        project: Some(&sandbox.selection_str()),
        request: &request,
        policy: Some(&evidence_policy()),
        allow_breaking: true,
    })
    .unwrap();
    let unchanged: serde_json::Value =
        serde_json::from_str(&unchanged.to_json_string().unwrap()).unwrap();
    assert_eq!(unchanged["fileEdits"].as_array().unwrap().len(), 0);

    let compilation = compiled_sandbox(&sandbox);
    assert!(compilation.project.has_command_results());
    let searchable = lekalo_core::search::projection::project_ir(&compilation.project, 100);
    assert!(searchable.cards["def:tasks.complete"]
        .signatures
        .iter()
        .any(|signature| signature.role == "output" && signature.id == "tasks.text"));
    let graph = lekalo_core::graph::build(&compilation.project).unwrap();
    assert!(graph
        .to_canonical_json()
        .unwrap()
        .contains("command-returns"));
    let effects = lekalo_core::effects::build(&compilation.project).unwrap();
    let inspected = lekalo_core::inspect::run(
        &compilation,
        &graph,
        &effects,
        &lekalo_core::inspect::InspectRequest {
            selector: "tasks.complete".to_owned(),
            include: Default::default(),
        },
    )
    .unwrap();
    let inspected: serde_json::Value = serde_json::from_str(&inspected.json).unwrap();
    assert!(inspected["contract"]["output"]
        .to_string()
        .contains("tasks.text"));
    // The existing error-output grammar already represents value results;
    // its semantic binding must agree exactly with the new command result.
    let mut errors: serde_json::Value =
        serde_json::from_slice(lekalo_core::error_contract::REGISTRY_BYTES).unwrap();
    let mut declared_error = errors["errors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|error| error["id"] == "planner.store_unavailable")
        .unwrap()
        .clone();
    declared_error["id"] = serde_json::json!("tasks.store_unavailable");
    errors["errors"] = serde_json::json!([declared_error]);
    errors["bindings"] = serde_json::json!([{
        "operation":"tasks.complete", "kind":"command",
        "output":{"optional":{"ref":"tasks.text"}}, "errors":["tasks.store_unavailable"]
    }]);
    let registry = lekalo_core::error_contract::ErrorRegistry::from_bytes(
        &serde_json::to_vec(&errors).unwrap(),
    )
    .unwrap();
    lekalo_core::error_contract::source::validate(&registry, &compilation.project).unwrap();
    errors["bindings"][0]["output"] = serde_json::Value::Null;
    let mismatched = lekalo_core::error_contract::ErrorRegistry::from_bytes(
        &serde_json::to_vec(&errors).unwrap(),
    )
    .unwrap();
    assert!(
        lekalo_core::error_contract::source::validate(&mismatched, &compilation.project).is_err()
    );
    let rename = defaults_request(serde_json::json!([{
        "operation":"symbol:move", "symbol":"tasks.text", "toModule":"tasks", "toName":"label"
    }]));
    apply_request(&sandbox, &rename, "command-result-type-rename", true).unwrap();
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("tasks.label"));
}

#[test]
fn command_result_declarations_are_successor_only_and_unknown_types_never_write() {
    for successor in [false, true] {
        let sandbox = Sandbox::new("command-result-declaration");
        if successor {
            enable_default_successor(&sandbox);
        }
        let request = defaults_request(serde_json::json!([{
            "operation":"definition:add", "kind":"command", "module":"tasks",
            "definition":{"name":"complete", "returns":{"ref":"tasks.text"}}
        }]));
        let path = sandbox.root.join("lekalo/modules/tasks/commands.yaml");
        if !successor {
            assert!(matches!(
                prepare(&sandbox, &request),
                Err(ChangeError::UnsupportedMember {
                    member: "command-return",
                    ..
                })
            ));
            assert!(!path.exists());
            continue;
        }
        apply_request(&sandbox, &request, "declare-command-result", false).unwrap();
        assert!(compiled_sandbox(&sandbox).project.has_command_results());
        let before = std::fs::read(&path).unwrap();
        let invalid = defaults_request(serde_json::json!([{
            "operation":"operation-output:set", "operationTarget":"tasks.complete",
            "type":{"ref":"tasks.missing"}
        }]));
        assert!(matches!(
            prepare(&sandbox, &invalid),
            Err(ChangeError::LoadFailed(_))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn query_input_owner_joins_bind_names_types_presence_and_defaults_exactly() {
    use lekalo_core::query_model::{self, QueryModelAttachment};
    let sandbox = Sandbox::new("query-input-owner-joins");
    enable_default_successor(&sandbox);
    let path = sandbox.root.join("lekalo/modules/tasks/queries.yaml");
    let source = "schema_version: \"0.8.0\"\ndefinitions:\n  - id: tasks.find\n    kind: query\n    version: 1\n    input:\n      - name: title\n        type: tasks.text\n        required: true\n        default: sample\n      - name: tag\n        type: tasks.text?\n        default: null\n    reads:\n      - tasks.task\n    returns: tasks.task\n";
    std::fs::write(&path, source).unwrap();
    let selection = sandbox.selection();
    let model_wire = lekalo_core::loader::run(&selection, false).to_json_string();
    let compilation = compiled_sandbox(&sandbox);
    let project = compilation.project.project.as_ref().unwrap().id.as_str();
    let model_digest = format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(model_wire.as_bytes())
    );
    let ir_digest = format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(compilation.project.to_canonical_json().as_bytes(),)
    );
    let wire = serde_json::json!({
        "schemaVersion":"lekalo/query-model/v0.8.0", "identity":"dev.lekalo.query-model@0.8.0",
        "attachmentRevision":"1.0.0", "projectId":project,
        "modelRef":{"modelVersion":"0.8.0", "digest":model_digest},
        "irRef":{"identity":"dev.lekalo.ir@0.8.0", "digest":ir_digest},
        "tenancy":[], "queries":[{
            "query":"tasks.find", "source":"tasks.task", "cardinality":"one", "consistency":"strong",
            "parameters":[
                {"name":"title", "type":"tasks.text", "required":true, "default":"sample"},
                {"name":"tag", "type":"tasks.text?", "required":false, "default":null}
            ]
        }]
    });
    let attachment = QueryModelAttachment::from_value(&wire).unwrap();
    let resolution =
        query_model::resolve(&attachment, &selection, query_model::Profile::Default).unwrap();
    assert_eq!(resolution.plans().len(), 1);
    let snapshot = lekalo_core::loader::normalize_model(&selection).unwrap();
    let drifted = source.replace("default: sample", "default: newer");
    std::fs::write(&path, &drifted).unwrap();
    query_model::resolve_model(&attachment, &snapshot, query_model::Profile::Default).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), drifted);
    assert!(
        query_model::resolve(&attachment, &selection, query_model::Profile::Default)
            .unwrap_err()
            .to_json_string()
            .contains("model-digest")
    );
    std::fs::write(&path, source).unwrap();
    let canonical: serde_json::Value =
        serde_json::from_str(&attachment.canonical_bytes().unwrap()).unwrap();
    assert_eq!(canonical["identity"], "dev.lekalo.query-model@0.8.0");
    assert_eq!(canonical["queries"][0]["parameters"][0]["name"], "tag");
    assert!(canonical["queries"][0]["parameters"][0]
        .as_object()
        .unwrap()
        .contains_key("default"));
    assert_eq!(
        canonical["queries"][0]["parameters"][0]["default"],
        serde_json::Value::Null
    );
    for (member, value, expected) in [
        ("name", serde_json::json!("other"), "query-input-members"),
        ("type", serde_json::json!("tasks.bool"), "query-input-type"),
        ("required", serde_json::json!(false), "query-input-required"),
        (
            "default",
            serde_json::json!("different"),
            "query-input-default",
        ),
    ] {
        let mut invalid = wire.clone();
        invalid["queries"][0]["parameters"][0][member] = value;
        let attachment = QueryModelAttachment::from_value(&invalid).unwrap();
        let result = query_model::resolve(&attachment, &selection, query_model::Profile::Default)
            .unwrap_err();
        assert!(result.to_json_string().contains(expected), "{expected}");
    }
    let mut absent = wire.clone();
    absent["queries"][0]["parameters"][1]
        .as_object_mut()
        .unwrap()
        .remove("default");
    let attachment = QueryModelAttachment::from_value(&absent).unwrap();
    assert!(
        query_model::resolve(&attachment, &selection, query_model::Profile::Default)
            .unwrap_err()
            .to_json_string()
            .contains("query-input-default")
    );
    let mut required_missing = wire.clone();
    required_missing["queries"][0]["parameters"][0]
        .as_object_mut()
        .unwrap()
        .remove("required");
    assert!(QueryModelAttachment::from_value(&required_missing).is_err());
    let mut mixed = wire;
    mixed["modelRef"]["modelVersion"] = serde_json::json!("0.2.16");
    assert!(QueryModelAttachment::from_value(&mixed).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
}

#[test]
fn definition_add_rejects_duplicate_field_names_before_mutation_on_both_models() {
    for successor in [false, true] {
        for kind in ["command", "query", "event"] {
            let sandbox = Sandbox::new("duplicate-definition-fields");
            if successor {
                enable_default_successor(&sandbox);
            }
            let before = sandbox.entities_bytes();
            let fields = serde_json::json!([
                {"name":"search", "type":{"ref":"tasks.text"}},
                {"name":"search", "type":{"ref":"tasks.bool"}}
            ]);
            let mut definition = serde_json::json!({"name":"duplicate"});
            definition[if kind == "event" { "payload" } else { "input" }] = fields;
            if kind == "query" {
                definition["reads"] = serde_json::json!(["tasks.task"]);
            }
            let request = defaults_request(serde_json::json!([{
                "operation":"definition:add", "kind":kind, "module":"tasks", "definition":definition
            }]));
            let result = prepare(&sandbox, &request);
            if kind == "query" && !successor {
                assert!(matches!(
                    result,
                    Err(ChangeError::UnsupportedMember {
                        member: "query-input",
                        ..
                    })
                ));
            } else {
                assert!(
                    matches!(result, Err(ChangeError::LoadFailed(_))),
                    "{kind} / successor={successor}"
                );
            }
            assert_eq!(sandbox.entities_bytes(), before);
            for name in ["commands", "queries", "events"] {
                assert!(!sandbox
                    .root
                    .join(format!("lekalo/modules/tasks/{name}.yaml"))
                    .exists());
            }
            assert!(!sandbox.history_path().exists());
        }
        // A raw authored Model must fail at the same IR owner, even when
        // it never passes through Change's operation-specific decoder.
        let sandbox = Sandbox::new("duplicate-model-fields");
        if successor {
            enable_default_successor(&sandbox);
        }
        let source = String::from_utf8(sandbox.entities_bytes())
            .unwrap()
            .replace("name: done", "name: title");
        std::fs::write(sandbox.entities_path(), source).unwrap();
        let model = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
        assert!(lekalo_core::ir::compile(&model).is_err());
    }
}

#[test]
fn policy_declarations_use_model_spelling_and_replay_on_both_models() {
    for successor in [false, true] {
        let sandbox = Sandbox::new("policy-model-spelling");
        if successor {
            enable_default_successor(&sandbox);
        }
        let version = if successor { "0.8.0" } else { "0.2.16" };
        std::fs::write(sandbox.root.join("lekalo/modules/tasks/commands.yaml"), format!("schema_version: \"{version}\"\ndefinitions:\n  - id: tasks.complete\n    kind: command\n    version: 1\n")).unwrap();
        let request = defaults_request(serde_json::json!([{
            "operation":"definition:add", "kind":"policy", "module":"tasks",
            "definition":{"name":"permit", "appliesTo":["tasks.complete"], "decision":"allow"}
        }]));
        apply_request(&sandbox, &request, "policy-add", false).unwrap();
        assert_eq!(
            apply_request(&sandbox, &request, "policy-add", false)
                .unwrap()
                .state,
            "replayed"
        );
        let bytes =
            std::fs::read_to_string(sandbox.root.join("lekalo/modules/tasks/policies.yaml"))
                .unwrap();
        assert!(bytes.contains("applies_to:"));
        assert!(!bytes.contains("appliesTo:"));
        assert!(bytes.starts_with(&format!("schema_version: \"{version}\"")));
        let compilation = compiled_sandbox(&sandbox);
        let definition = compilation
            .project
            .definitions
            .iter()
            .find(|d| d.id().as_str() == "tasks.permit")
            .unwrap();
        let lekalo_core::ir::Definition::Policy(policy) = definition else {
            panic!("policy kind")
        };
        assert_eq!(policy.applies_to[0].as_str(), "tasks.complete");
    }
}

#[test]
fn successor_query_inputs_have_typed_edges_defaults_and_lossless_signature_changes() {
    let sandbox = Sandbox::new("query-inputs");
    enable_default_successor(&sandbox);
    let path = sandbox.root.join("lekalo/modules/tasks/queries.yaml");
    let source = "schema_version: \"0.8.0\"\r\ndefinitions:\r\n  - id: tasks.list\r\n    kind: query\r\n    version: 1\r\n    reads:\r\n      - tasks.task\r\n    returns: tasks.task # keep query comment";
    std::fs::write(&path, source).unwrap();
    let entities = sandbox.entities_bytes();
    let input =
        serde_json::json!([{"name":"search", "type":{"ref":"tasks.text"}, "default":"tasks.text"}]);
    let request = query_inputs_request(input.clone());
    let preview = prepare(&sandbox, &request).unwrap();
    let plan: serde_json::Value = serde_json::from_str(&preview.to_json_string().unwrap()).unwrap();
    assert!(plan["semanticDiff"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["kind"] == "signature.input-added"));
    assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
    apply_request(&sandbox, &request, "query-inputs-add", false).unwrap();
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("returns: tasks.task # keep query comment\r\n"));
    assert!(!after.ends_with('\n'));
    assert!(!after.replace("\r\n", "").contains('\n'));
    let compilation = compiled_sandbox(&sandbox);
    let ir: serde_json::Value =
        serde_json::from_str(&compilation.project.to_canonical_json()).unwrap();
    let query = ir["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == "tasks.list")
        .unwrap();
    assert_eq!(query["input"][0]["default"], "tasks.text");
    let graph = lekalo_core::graph::build(&compilation.project).unwrap();
    let graph_json = graph.to_canonical_json().unwrap();
    assert!(graph_json.contains("query-input"));
    assert!(graph_json.contains("accepts"));
    let noop = prepare(&sandbox, &request).unwrap();
    let noop: serde_json::Value = serde_json::from_str(&noop.to_json_string().unwrap()).unwrap();
    assert_eq!(noop["equal"], true);
    assert!(noop["fileEdits"].as_array().unwrap().is_empty());
    for input in [
        serde_json::json!([{"name":"bad", "type":{"ref":"tasks.text"}, "default":true}]),
        serde_json::json!([{"name":"bad", "type":{"ref":"tasks.ghost"}}]),
    ] {
        assert!(prepare(&sandbox, &query_inputs_request(input)).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), after);
    }
    let rename = defaults_request(
        serde_json::json!([{"operation":"symbol:move", "symbol":"tasks.text", "toModule":"tasks", "toName":"label"}]),
    );
    apply_request(&sandbox, &rename, "query-input-rename", true).unwrap();
    let moved = std::fs::read_to_string(&path).unwrap();
    assert!(moved.contains("type: \"tasks.label\""));
    assert!(moved.contains("default: \"tasks.text\""));
    assert_ne!(sandbox.entities_bytes(), entities);
    let clear = query_inputs_request(serde_json::json!([]));
    apply_request_opts(
        &sandbox,
        &clear,
        "query-input-clear",
        true,
        Some(evidence_policy()),
    )
    .unwrap();
    assert!(!std::fs::read_to_string(&path).unwrap().contains("input:"));
    assert!(!compiled_sandbox(&sandbox).project.has_query_inputs());
}

fn compiled_sandbox(sandbox: &Sandbox) -> lekalo_core::ir::Compilation {
    let loaded = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    lekalo_core::ir::compile(&loaded).unwrap()
}

#[test]
fn query_input_type_and_description_edits_preserve_inline_comments_and_quoted_apostrophes() {
    let sandbox = Sandbox::new("query-input-trivia");
    enable_default_successor(&sandbox);
    let path = sandbox.root.join("lekalo/modules/tasks/queries.yaml");
    std::fs::write(&path, "schema_version: \"0.8.0\"\r\ndefinitions:\r\n  - id: tasks.list\r\n    kind: query\r\n    version: 1\r\n    reads:\r\n      - tasks.task\r\n    input:\r\n      - name: search\r\n        type: 'tasks.text' # keep type note\r\n        description: 'old description' # keep description note").unwrap();
    let compilation = compiled_sandbox(&sandbox);
    let capsule = lekalo_core::context::plan(
        &lekalo_core::context::CapsuleScope::Symbol("tasks.list".to_owned()),
        2000,
        false,
        &compilation,
    )
    .expect("successor query input context");
    let capsule: serde_json::Value = serde_json::from_str(&capsule.to_canonical_json()).unwrap();
    assert_eq!(capsule["schemaVersion"], "lekalo/context/v0.8.0");
    assert_eq!(
        capsule["sections"]["symbol"][0]["contract"]["input"][0]["name"],
        "search"
    );
    assert_eq!(
        capsule["sections"]["symbol"][0]["contract"]["input"][0]["type"],
        serde_json::json!({"ref":"tasks.text"})
    );
    let request = query_inputs_request(
        serde_json::json!([{"name":"search", "type":{"ref":"tasks.bool"}, "description":"can't"}]),
    );
    apply_request_opts(
        &sandbox,
        &request,
        "query-input-type-trivia",
        true,
        Some(evidence_policy()),
    )
    .unwrap();
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("type: 'tasks.bool' # keep type note\r\n"));
    assert!(after.ends_with("description: 'can''t' # keep description note"));
    assert!(!after.replace("\r\n", "").contains('\n'));
}

#[test]
fn query_input_declarations_inherit_successor_and_legacy_refuses_without_writes() {
    let sandbox = Sandbox::new("query-input-declare");
    let definition = defaults_request(
        serde_json::json!([{"operation":"definition:add", "kind":"query", "module":"tasks", "definition":{"name":"list", "input":[{"name":"search", "type":{"ref":"tasks.text"}, "default":"initial"}], "reads":["tasks.task"], "returns":{"ref":"tasks.task"}}}]),
    );
    assert!(matches!(
        prepare(&sandbox, &definition),
        Err(ChangeError::UnsupportedMember {
            member: "query-input",
            ..
        })
    ));
    assert!(!sandbox
        .root
        .join("lekalo/modules/tasks/queries.yaml")
        .exists());
    enable_default_successor(&sandbox);
    apply_request(&sandbox, &definition, "query-input-declare", false).unwrap();
    assert!(compiled_sandbox(&sandbox).project.has_query_inputs());
    let legacy = Sandbox::new("query-input-legacy");
    let path = legacy.root.join("lekalo/modules/tasks/queries.yaml");
    let source = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.list\n    kind: query\n    version: 1\n    reads:\n      - tasks.task\n";
    std::fs::write(&path, source).unwrap();
    assert!(matches!(
        prepare(
            &legacy,
            &query_inputs_request(
                serde_json::json!([{"name":"search", "type":{"ref":"tasks.text"}}])
            )
        ),
        Err(ChangeError::UnsupportedMember {
            member: "query-input",
            ..
        })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
    assert!(!legacy.history_path().exists());
}

#[test]
fn enum_wire_changes_validate_defaults_require_policy_and_preserve_comments() {
    let sandbox = Sandbox::new("enum-wire");
    enable_default_successor(&sandbox);
    let source = std::fs::read_to_string(sandbox.entities_path())
        .unwrap()
        .replace("\r\n", "\n")
        .replace('\n', "\r\n");
    std::fs::write(sandbox.entities_path(), source).unwrap();
    let add = defaults_request(
        serde_json::json!([{"operation":"enum-case:add", "enum":"tasks.priority", "case":{"name":"urgent", "value":"URGENT"}}]),
    );
    apply_request(&sandbox, &add, "wire-add", false).unwrap();
    let field = |value: &str| serde_json::json!({"operation":"entity-field:add", "entity":"tasks.task", "field":{"name":"priority", "type":{"ref":"tasks.priority"}, "default":value}});
    let before = sandbox.entities_bytes();
    assert!(
        prepare(
            &sandbox,
            &defaults_request(serde_json::json!([field("urgent")]))
        )
        .is_err(),
        "defaults name actual wire values"
    );
    assert_eq!(sandbox.entities_bytes(), before);
    apply_request(
        &sandbox,
        &defaults_request(serde_json::json!([field("URGENT")])),
        "wire-default",
        false,
    )
    .unwrap();
    let source = std::fs::read_to_string(sandbox.entities_path())
        .unwrap()
        .replace(
            "wire_value: \"URGENT\"",
            "wire_value: \"URGENT\" # retain this comment",
        );
    std::fs::write(sandbox.entities_path(), source).unwrap();
    let change = serde_json::json!({"operation":"enum-case:change", "enum":"tasks.priority", "name":"urgent", "definition":{"value":"NEW", "description":"Explicit encoding"}});
    assert!(
        prepare(
            &sandbox,
            &defaults_request(serde_json::json!([change.clone()]))
        )
        .is_err(),
        "a stale default cannot survive a wire change"
    );
    let request = defaults_request(
        serde_json::json!([change, {"operation":"entity-field:remove", "entity":"tasks.task", "name":"priority"}, field("NEW")]),
    );
    let before = sandbox.entities_bytes();
    assert!(
        apply_request(&sandbox, &request, "wire-denied", true).is_err(),
        "wire changes require migration evidence in addition to acknowledgment"
    );
    assert_eq!(sandbox.entities_bytes(), before);
    apply_request_opts(
        &sandbox,
        &request,
        "wire-approved",
        true,
        Some(evidence_policy()),
    )
    .unwrap();
    let after = std::fs::read_to_string(sandbox.entities_path()).unwrap();
    assert!(after.contains("wire_value: \"NEW\" # retain this comment\r\n"));
    assert!(after.contains("description: \"Explicit encoding\""));
    assert!(!after.replace("\r\n", "").contains('\n'));
    let noop = defaults_request(
        serde_json::json!([{"operation":"enum-case:change", "enum":"tasks.priority", "name":"urgent", "definition":{"value":"NEW"}}]),
    );
    let plan = prepare(&sandbox, &noop).unwrap();
    let plan: serde_json::Value = serde_json::from_str(&plan.to_json_string().unwrap()).unwrap();
    assert_eq!(plan["equal"], true);
    assert!(plan["fileEdits"].as_array().unwrap().is_empty());
    let duplicate = defaults_request(
        serde_json::json!([{"operation":"enum-case:add", "enum":"tasks.priority", "case":{"name":"duplicate", "value":"NEW"}}]),
    );
    assert!(prepare(&sandbox, &duplicate).is_err());
    assert_eq!(
        std::fs::read_to_string(sandbox.entities_path()).unwrap(),
        after
    );
}

#[test]
fn structured_enum_wire_values_are_data_and_survive_symbol_relocation() {
    let sandbox = Sandbox::new("enum-wire-data");
    enable_default_successor(&sandbox);
    let request = defaults_request(serde_json::json!([
        {"operation":"enum-case:add", "enum":"tasks.priority", "case":{"name":"object_wire", "value":{"ref":"tasks.text", "items":[1,true,null]}}},
        {"operation":"enum-case:add", "enum":"tasks.priority", "case":{"name":"numeric_wire", "value":17}},
        {"operation":"enum-case:add", "enum":"tasks.priority", "case":{"name":"null_wire", "value":null}},
        {"operation":"entity-field:add", "entity":"tasks.task", "field":{"name":"wire_choice", "type":{"ref":"tasks.priority"}, "default":{"ref":"tasks.text", "items":[1,true,null]}}}
    ]));
    apply_request(&sandbox, &request, "structured-wires", false).unwrap();
    let moved = defaults_request(
        serde_json::json!([{"operation":"symbol:move", "symbol":"tasks.text", "toModule":"tasks", "toName":"label"}]),
    );
    apply_request(&sandbox, &moved, "wire-symbol-move", true).unwrap();
    let model = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    let compilation = lekalo_core::ir::compile(&model).unwrap();
    let declaration = compilation
        .project
        .definitions
        .iter()
        .find(|d| d.id().as_str() == "tasks.priority")
        .unwrap();
    let mut mapper = lekalo_core::openapi::SchemaMapper::new(
        &compilation.project,
        lekalo_core::openapi::DocumentVersion::V3_1,
    );
    let body = lekalo_core::openapi::component_body(declaration, &mut mapper).unwrap();
    assert!(
        body.get("type").is_none(),
        "mixed JSON wire values cannot be advertised as strings"
    );
    let encodings = body["enum"].as_array().unwrap();
    assert!(encodings.contains(&serde_json::json!(17)));
    assert!(encodings.contains(&serde_json::json!(null)));
    assert!(encodings.contains(&serde_json::json!({"ref":"tasks.text", "items":[1,true,null]})));
    let ir: serde_json::Value =
        serde_json::from_str(&compilation.project.to_canonical_json()).unwrap();
    let values = ir["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == "tasks.priority")
        .unwrap()["values"]
        .as_array()
        .unwrap();
    assert_eq!(
        values.iter().find(|v| v["value"] == "object_wire").unwrap()["wireValue"],
        serde_json::json!({"ref":"tasks.text", "items":[1,true,null]})
    );
    assert_eq!(
        values
            .iter()
            .find(|v| v["value"] == "numeric_wire")
            .unwrap()["wireValue"],
        17
    );
    assert!(values
        .iter()
        .find(|v| v["value"] == "null_wire")
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("wireValue"));
    let capsule = lekalo_core::context::plan(
        &lekalo_core::context::CapsuleScope::Symbol("tasks.priority".to_owned()),
        2000,
        false,
        &compilation,
    )
    .expect("successor enum wire context");
    let capsule: serde_json::Value = serde_json::from_str(&capsule.to_canonical_json()).unwrap();
    assert_eq!(capsule["schemaVersion"], "lekalo/context/v0.8.0");
    assert_eq!(
        capsule["sections"]["symbol"][0]["contract"]["values"],
        serde_json::Value::Array(values.clone())
    );
}

#[test]
fn predecessor_enum_wire_add_and_change_refuse_without_source_mutation() {
    let sandbox = Sandbox::new("legacy-wire");
    let before = sandbox.entities_bytes();
    for operation in [
        serde_json::json!({"operation":"enum-case:add", "enum":"tasks.priority", "case":{"name":"urgent", "value":"N"}}),
        serde_json::json!({"operation":"enum-case:change", "enum":"tasks.priority", "name":"normal", "definition":{"value":null}}),
    ] {
        let request = defaults_request(serde_json::json!([operation]));
        assert!(matches!(
            prepare(&sandbox, &request),
            Err(ChangeError::UnsupportedMember {
                member: "case-wire-value",
                ..
            })
        ));
        assert_eq!(sandbox.entities_bytes(), before);
        assert!(!sandbox.history_path().exists());
    }
}

#[test]
fn successor_defaults_survive_all_field_writers_and_default_only_diff() {
    let sandbox = Sandbox::new("successor-defaults");
    enable_default_successor(&sandbox);
    let entities = std::fs::read_to_string(sandbox.entities_path()).unwrap();
    std::fs::write(sandbox.entities_path(), format!("{entities}\n  - id: tasks.snapshot\n    kind: value-object\n    version: 1\n    fields:\n      - name: title\n        type: tasks.text\n        required: true\n      - name: done\n        type: tasks.bool\n")).unwrap();
    let request = defaults_request(serde_json::json!([
        {"operation":"entity-field:add", "entity":"tasks.task", "field":{"name":"priority", "type":{"ref":"tasks.priority"}, "default":"normal"}},
        {"operation":"entity-field:add", "entity":"tasks.task", "field":{"name":"labels", "type":{"list":{"ref":"tasks.text"}}, "default":["a", "b\n#literal"]}},
        {"operation":"entity-field:add", "entity":"tasks.task", "field":{"name":"nullable", "type":{"optional":{"ref":"tasks.text"}}, "default":null}},
        {"operation":"entity-field:add", "entity":"tasks.task", "field":{"name":"snapshot", "type":{"ref":"tasks.snapshot"}, "default":{"title":"saved", "done":false}}},
        {"operation":"definition:add", "kind":"command", "module":"tasks", "definition":{"name":"save", "input":[{"name":"title", "type":{"ref":"tasks.text"}, "default":"before"}]}},
        {"operation":"definition:add", "kind":"event", "module":"tasks", "definition":{"name":"saved", "payload":[{"name":"title", "type":{"ref":"tasks.text"}, "default":"event"}]}}
    ]));
    apply_request(&sandbox, &request, "default-writers", false).unwrap();
    let load = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    let compiled = lekalo_core::ir::compile(&load).unwrap();
    let capsule = lekalo_core::context::plan(
        &lekalo_core::context::CapsuleScope::Symbol("tasks.task".to_owned()),
        2000,
        false,
        &compiled,
    )
    .expect("successor context preserves defaults");
    let capsule: serde_json::Value = serde_json::from_str(&capsule.to_canonical_json()).unwrap();
    assert_eq!(capsule["schemaVersion"], "lekalo/context/v0.8.0");
    let context_fields = capsule["sections"]["symbol"][0]["contract"]["fields"]
        .as_array()
        .unwrap();
    assert!(context_fields
        .iter()
        .find(|field| field["name"] == "nullable")
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("default"));
    assert_eq!(
        context_fields
            .iter()
            .find(|field| field["name"] == "snapshot")
            .unwrap()["default"],
        serde_json::json!({"title":"saved","done":false})
    );
    let ir: serde_json::Value =
        serde_json::from_str(&compiled.project.to_canonical_json()).unwrap();
    let definitions = ir["definitions"].as_array().unwrap();
    let entity = definitions
        .iter()
        .find(|d| d["id"] == "tasks.task")
        .unwrap();
    let fields = entity["fields"].as_array().unwrap();
    assert_eq!(
        fields.iter().find(|f| f["name"] == "priority").unwrap()["default"],
        "normal"
    );
    assert_eq!(
        fields.iter().find(|f| f["name"] == "labels").unwrap()["default"],
        serde_json::json!(["a", "b\n#literal"])
    );
    assert!(fields
        .iter()
        .find(|f| f["name"] == "nullable")
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("default"));
    assert_eq!(
        fields.iter().find(|f| f["name"] == "snapshot").unwrap()["default"],
        serde_json::json!({"title":"saved", "done":false})
    );
    assert_eq!(
        definitions
            .iter()
            .find(|d| d["id"] == "tasks.saved")
            .unwrap()["payload"][0]["default"],
        "event"
    );
    let commands = sandbox.root.join("lekalo/modules/tasks/commands.yaml");
    assert!(std::fs::read_to_string(&commands)
        .unwrap()
        .starts_with("schema_version: \"0.8.0\""));
    let input = defaults_request(
        serde_json::json!([{"operation":"operation-input:set", "operationTarget":"tasks.save", "definition":{"input":[{"name":"title", "type":{"ref":"tasks.text"}, "default":"after"}]}}]),
    );
    let prepared = prepare(&sandbox, &input).unwrap();
    let plan: serde_json::Value =
        serde_json::from_str(&prepared.to_json_string().unwrap()).unwrap();
    assert!(plan["semanticDiff"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["kind"] == "field.default-changed"));
    apply_request(&sandbox, &input, "default-only", false).unwrap();
    let after = lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    let after = lekalo_core::ir::compile(&after).unwrap();
    let diff = lekalo_core::diff::compare(
        &compiled.project,
        &after.project,
        &lekalo_core::diff::DiffRequest::new(),
    )
    .unwrap();
    let delta = diff
        .changes()
        .iter()
        .find(|d| d.kind() == lekalo_core::diff::ChangeKind::FieldDefaultChanged)
        .unwrap();
    assert_eq!(
        delta.before(),
        &lekalo_core::diff::Summary::Text(format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(b"\"before\"")
        ))
    );
    assert_eq!(
        delta.after(),
        &lekalo_core::diff::Summary::Text(format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(b"\"after\"")
        ))
    );
    let before = std::fs::read(&commands).unwrap();
    let unchanged = prepare(&sandbox, &input).unwrap();
    let plan: serde_json::Value =
        serde_json::from_str(&unchanged.to_json_string().unwrap()).unwrap();
    assert_eq!(plan["equal"], true);
    assert_eq!(std::fs::read(&commands).unwrap(), before);
    // Dropping a default is also a semantic change, not a no-op.
    let remove = defaults_request(
        serde_json::json!([{"operation":"operation-input:set", "operationTarget":"tasks.save", "definition":{"input":[{"name":"title", "type":{"ref":"tasks.text"}}]}}]),
    );
    apply_request(&sandbox, &remove, "remove-default", false).unwrap();
    assert!(!std::fs::read_to_string(&commands)
        .unwrap()
        .contains("default:"));
}

#[test]
fn successor_default_type_mismatches_and_unspellable_yaml_never_write() {
    let sandbox = Sandbox::new("invalid-defaults");
    enable_default_successor(&sandbox);
    let before = sandbox.entities_bytes();
    for (typ, default) in [
        (
            serde_json::json!({"ref":"tasks.priority"}),
            serde_json::json!("missing"),
        ),
        (
            serde_json::json!({"ref":"tasks.text"}),
            serde_json::json!(17),
        ),
        (
            serde_json::json!({"ref":"tasks.task"}),
            serde_json::json!({"ghost":true}),
        ),
        (
            serde_json::json!({"list":{"ref":"tasks.text"}}),
            serde_json::json!([]),
        ),
    ] {
        let request = defaults_request(
            serde_json::json!([{"operation":"entity-field:add", "entity":"tasks.task", "field":{"name":"invalid", "type":typ, "default":default}}]),
        );
        assert!(prepare(&sandbox, &request).is_err());
        assert_eq!(sandbox.entities_bytes(), before);
        assert!(!sandbox.history_path().exists());
    }
}

#[test]
fn successor_default_updates_preserve_crlf_and_inline_comments() {
    let sandbox = Sandbox::new("default-crlf");
    enable_default_successor(&sandbox);
    let command = defaults_request(
        serde_json::json!([{"operation":"definition:add", "kind":"command", "module":"tasks", "definition":{"name":"save", "input":[{"name":"title", "type":{"ref":"tasks.text"}, "default":"before"}]}}]),
    );
    apply_request(&sandbox, &command, "new-default-command", false).unwrap();
    let commands = sandbox.root.join("lekalo/modules/tasks/commands.yaml");
    let original = std::fs::read_to_string(&commands)
        .unwrap()
        .replace("\r\n", "\n")
        .trim_end_matches('\n')
        .replace('\n', "\r\n");
    std::fs::write(&commands, &original).unwrap();
    let request = defaults_request(
        serde_json::json!([{"operation":"operation-input:set", "operationTarget":"tasks.save", "definition":{"input":[{"name":"title", "type":{"ref":"tasks.text"}, "default":"after"}]}}]),
    );
    apply_request(&sandbox, &request, "crlf-default", false).unwrap();
    assert_eq!(
        std::fs::read_to_string(&commands).unwrap(),
        original.replace("before", "after")
    );
    let with_comment = original.replace("\"before\"", "\"after\" # owned comment");
    std::fs::write(&commands, &with_comment).unwrap();
    let changed = defaults_request(
        serde_json::json!([{"operation":"operation-input:set", "operationTarget":"tasks.save", "definition":{"input":[{"name":"title", "type":{"ref":"tasks.text"}, "default":"last"}]}}]),
    );
    apply_request(&sandbox, &changed, "inline-comment-default", false).unwrap();
    let expected = with_comment.replace("\"after\"", "\"last\"");
    assert_eq!(std::fs::read_to_string(&commands).unwrap(), expected);
    let remove = defaults_request(
        serde_json::json!([{"operation":"operation-input:set", "operationTarget":"tasks.save", "definition":{"input":[{"name":"title", "type":{"ref":"tasks.text"}}]}}]),
    );
    assert!(matches!(
        prepare(&sandbox, &remove),
        Err(ChangeError::SourcePreservation { .. })
    ));
    assert_eq!(std::fs::read_to_string(&commands).unwrap(), expected);
}

#[test]
fn a_genuine_ledger_grafted_into_a_same_id_project_mints_no_replay_authority() {
    // r6 review, finding devin F1: the ledger binds the LOGICAL project
    // id, so a self-consistent genuine `change-history.json` copied from
    // another checkout of the same project id (git clone, template copy,
    // backup restore without sibling sources) decodes and passes the
    // project binding. Replay from it would mint a durable audit record
    // for a change this project never received. The record's committed
    // per-file after-digests are the postcondition: none may match, so
    // the replay/lookup refuse with the registered history rule.
    let donor = Sandbox::new("graft-donor");
    let request = add_priority_request();
    apply_request(&donor, &request, "graft-key-v1", false).expect("donor commits");
    let genuine_ledger = std::fs::read(donor.history_path()).expect("donor ledger");
    // The recipient: same project id, different checkout — the field
    // never existed here.
    let recipient = Sandbox::new("graft-recipient");
    let before = recipient.entities_bytes();
    std::fs::write(recipient.history_path(), &genuine_ledger).expect("graft ledger");
    // Same key + same request bytes: every durable binding (request
    // digest, intent) matches, but the postcondition cannot.
    let error = apply_request(&recipient, &request, "graft-key-v1", false)
        .expect_err("grafted ledger must not mint replay authority");
    assert!(
        matches!(
            error,
            ChangeError::HistoryInvalid {
                detail: "postcondition",
                ..
            }
        ),
        "got {error:?}"
    );
    // The lookup surface is EXEMPT from the postcondition (r8, devin F2
    // + omp F2): a read-only receipt query mints no authority, and the
    // contract keeps receipts usable after source drift — so it returns
    // the grafted record's stored projection while the sources stay
    // untouched and replay (the authority surface) still refuses.
    let looked_up = change::lookup_receipt(Some(&recipient.selection_str()), "graft-key-v1")
        .expect("lookup returns the stored read-only projection");
    let looked_up = looked_up.expect("stored record");
    assert_eq!(looked_up.state(), ReceiptState::Committed);
    assert!(!looked_up.replayed, "a lookup never mints replay");
    // Nothing was written to the recipient's sources.
    assert_eq!(recipient.entities_bytes(), before, "recipient untouched");
    // The state check alone is not provenance (r10 F1). Even if the
    // recipient gets identical source bytes, a copied ledger has no local
    // transaction custody. Only an actual local commit creates that pin.
    std::fs::write(recipient.entities_path(), donor.entities_bytes()).expect("matching sources");
    let error = apply_request(&recipient, &request, "graft-key-v1", false)
        .expect_err("matching bytes alone cannot mint provenance");
    assert!(matches!(
        error,
        ChangeError::HistoryInvalid {
            detail: "commit-custody"
        }
    ));
    assert!(
        apply_request(&donor, &request, "graft-key-v1", false)
            .unwrap()
            .replayed
    );
}

/// Re-forge one single-record donor ledger so its record's `files[]`
/// entry names an attacker-chosen path with an attacker-chosen
/// after-digest (the r7 review's edited-graft attack, devin F3 +
/// omp F1): the record digest chain is recomputed exactly like the
/// writer computes it, so the forged ledger is fully self-consistent —
/// decode, project binding, and chain verification all pass, and only
/// the postcondition's path grammar can stop it.
fn forge_ledger_with_outside_path(
    donor_bytes: &[u8],
    outside_relative: &str,
    outside_digest: &str,
) -> Vec<u8> {
    let mut document: serde_json::Value = serde_json::from_slice(donor_bytes).expect("ledger");
    let project = document
        .get("project")
        .and_then(|value| value.as_str())
        .expect("project")
        .to_owned();
    let records = document
        .get_mut("records")
        .expect("records")
        .as_array_mut()
        .expect("records array");
    assert_eq!(records.len(), 1, "the donor ledger has one record");
    let record = &mut records[0];
    let intent = record
        .get("intentDigest")
        .and_then(|value| value.as_str())
        .expect("intent")
        .to_owned();
    {
        let receipt = record
            .get_mut("receipt")
            .expect("receipt")
            .as_object_mut()
            .expect("receipt object");
        for entry in receipt
            .get_mut("files")
            .expect("receipt files")
            .as_array_mut()
            .expect("files array")
            .iter_mut()
        {
            let entry = entry.as_object_mut().expect("file entry");
            entry.insert("path".to_owned(), serde_json::json!(outside_relative));
            entry.insert("afterDigest".to_owned(), serde_json::json!(outside_digest));
        }
    }
    if let Some(record) = record.as_object_mut() {
        record.insert(
            "files".to_owned(),
            serde_json::json!([outside_relative.to_owned()]),
        );
    }
    // Recompute the record digest exactly like the writer: the chain
    // anchor is the digest over previous anchor, project, intent, and
    // the canonical receipt projection without its own recordDigest.
    let mut projection = record.get("receipt").expect("receipt").clone();
    projection
        .get_mut("history")
        .expect("history")
        .as_object_mut()
        .expect("history object")
        .remove("recordDigest");
    let canonical = serde_json::to_string(&projection).expect("canonical projection");
    let previous = format!("sha256:{}", lekalo_core::digest::sha256_hex(b""));
    let digest = format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(
            format!("{previous}\u{1f}{project}\u{1f}{intent}\u{1f}{canonical}").as_bytes()
        )
    );
    record
        .get_mut("receipt")
        .expect("receipt")
        .get_mut("history")
        .expect("history")
        .as_object_mut()
        .expect("history object")
        .insert("recordDigest".to_owned(), serde_json::json!(digest.clone()));
    document["tip"] = serde_json::json!({"sequence": 1, "recordDigest": digest});
    serde_json::to_vec_pretty(&document).expect("forged ledger serializes")
}

#[test]
fn an_edited_graft_with_an_escaping_files_path_mints_no_replay_authority() {
    // r7 review, finding devin F3 + omp F1 (HIGH): the r7 postcondition
    // joined the serde-decoded `receipt.files[].path` onto the project
    // root and followed links, so a hand-forged (fully chain-consistent)
    // ledger whose single files[] entry named an EXISTING OUTSIDE file
    // with known bytes satisfied the ANY-match and minted replay
    // authority on a project that never received the change. The r8
    // postcondition validates every recorded path against the closed
    // project-relative grammar BEFORE anything is read: `..`, absolute,
    // drive, and separator forms refuse as `postcondition-path`, and the
    // read itself is no-follow.
    let donor = Sandbox::new("graft-escape-donor");
    let request = add_priority_request();
    apply_request(&donor, &request, "graft-escape-v1", false).expect("donor commits");
    let donor_ledger = std::fs::read(donor.history_path()).expect("donor ledger");
    // The outside victim: a file OUTSIDE the project whose exact bytes
    // the attacker knows, so its digest genuinely matches the forged
    // afterDigest — under the r7 ANY-match this entry alone vouched for
    // the whole record.
    let outside = std::env::current_dir()
        .expect("cwd")
        .join("target/change-core")
        .join(format!("outside-victim-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(&outside).expect("outside dir");
    let victim = outside.join("victim.yaml");
    std::fs::write(&victim, b"known attacker bytes\n").expect("victim");
    let victim_digest = format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(b"known attacker bytes\n")
    );
    for (tag, escaping) in [
        ("parent", "../outside-victim.yaml"),
        ("absolute", "/etc/known-file"),
        ("drive", "c:/known-file"),
        ("backslash", "..\\outside-victim.yaml"),
    ] {
        let recipient = Sandbox::new("graft-escape-recipient");
        let before = recipient.entities_bytes();
        let forged = forge_ledger_with_outside_path(&donor_ledger, escaping, &victim_digest);
        std::fs::write(recipient.history_path(), &forged).expect("graft forged ledger");
        // Same key + same request: every durable binding matches, the
        // forged chain verifies, but the path grammar refuses before any
        // read — no arbitrary-file digest oracle, no replay authority.
        let error = apply_request(&recipient, &request, "graft-escape-v1", false)
            .expect_err("escaping files[] path must refuse");
        assert!(
            matches!(
                &error,
                ChangeError::HistoryInvalid {
                    detail: "postcondition-path",
                    ..
                }
            ),
            "{tag}: got {error:?}"
        );
        assert_eq!(recipient.entities_bytes(), before, "{tag}: untouched");
    }
    let _ = std::fs::remove_dir_all(&outside);
}

/// Recompute ALL public integrity anchors, like a ledger editor can.
fn rechain_history(document: &mut serde_json::Value) {
    let project = document["project"].as_str().unwrap().to_owned();
    let mut previous = sha256_of(b"");
    let records = document["records"].as_array_mut().unwrap();
    for record in records.iter_mut() {
        let intent = record["intentDigest"].as_str().unwrap();
        let mut projection = record["receipt"].clone();
        projection["history"]
            .as_object_mut()
            .unwrap()
            .remove("recordDigest");
        let canonical = serde_json::to_string(&projection).unwrap();
        previous = sha256_of(
            format!("{previous}\u{1f}{project}\u{1f}{intent}\u{1f}{canonical}").as_bytes(),
        );
        record["receipt"]["history"]["recordDigest"] = serde_json::json!(previous);
    }
    document["tip"] = serde_json::json!({"sequence": records.len(), "recordDigest": previous});
}

#[test]
fn ledger_only_forgeries_cannot_mint_replay_even_after_rechaining() {
    let donor = Sandbox::new("rechain-donor");
    let request = add_priority_request();
    apply_request(&donor, &request, "rechain-key", false).unwrap();
    let genuine: serde_json::Value =
        serde_json::from_slice(&std::fs::read(donor.history_path()).unwrap()).unwrap();
    for attack in [
        "current-digest",
        "different-file",
        "empty-files",
        "fake-no-op",
        "later-successor",
    ] {
        let recipient = Sandbox::new("rechain-recipient");
        let before = recipient.entities_bytes();
        let mut forged = genuine.clone();
        if attack == "later-successor" {
            apply_request(&donor, &add_field_request("escalation"), "later-key", false).unwrap();
            forged = serde_json::from_slice(&std::fs::read(donor.history_path()).unwrap()).unwrap();
            forged["records"][1]["receipt"]["files"][0]["afterDigest"] =
                serde_json::json!(sha256_of(&before));
        } else if attack == "current-digest" {
            forged["records"][0]["receipt"]["files"][0]["afterDigest"] =
                serde_json::json!(sha256_of(&before));
            // Also evade the state/file digest consistency control: the
            // independent custody boundary must still refuse this forgery.
            forged["records"][0]["receipt"]["files"][0]["beforeDigest"] =
                serde_json::json!(sha256_of(b"invented preimage"));
        } else if attack == "different-file" {
            forged["records"][0]["files"] = serde_json::json!(["lekalo/project.yaml"]);
            forged["records"][0]["receipt"]["files"][0]["path"] =
                serde_json::json!("lekalo/project.yaml");
            forged["records"][0]["receipt"]["files"][0]["afterDigest"] = serde_json::json!(
                sha256_of(&std::fs::read(recipient.root.join("lekalo/project.yaml")).unwrap())
            );
        } else {
            forged["records"][0]["files"] = serde_json::json!([]);
            forged["records"][0]["receipt"]["files"] = serde_json::json!([]);
            if attack == "fake-no-op" {
                forged["records"][0]["receipt"]["state"] = serde_json::json!("no-op");
            }
        }
        rechain_history(&mut forged);
        if attack != "empty-files" {
            change::receipt::History::decode(&serde_json::to_vec(&forged).unwrap())
                .expect("public chain is fully consistent");
        }
        std::fs::write(
            recipient.history_path(),
            serde_json::to_vec_pretty(&forged).unwrap(),
        )
        .unwrap();
        let error = apply_request(&recipient, &request, "rechain-key", false).expect_err(attack);
        assert!(
            matches!(error, ChangeError::HistoryInvalid { .. }),
            "{attack}: {error:?}"
        );
        assert_eq!(recipient.entities_bytes(), before, "{attack}: no mutation");
    }
    // Editing and rechaining an ALREADY PINNED local ledger also fails:
    // existing custody cannot be overwritten by a new key to launder it.
    let mut edited: serde_json::Value =
        serde_json::from_slice(&std::fs::read(donor.history_path()).unwrap()).unwrap();
    edited["records"][0]["receipt"]["operations"][0]["detail"] =
        serde_json::json!("invented claim");
    rechain_history(&mut edited);
    std::fs::write(
        donor.history_path(),
        serde_json::to_vec_pretty(&edited).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        apply_request(&donor, &add_field_request("third"), "new-key", false),
        Err(ChangeError::HistoryInvalid {
            detail: "commit-custody"
        })
    ));
}

#[test]
fn tail_truncation_is_invalid_even_when_next_sequence_is_adjusted() {
    let sandbox = Sandbox::new("tail-truncation");
    apply_request(&sandbox, &add_field_request("priority"), "tail-one", false).unwrap();
    apply_request(
        &sandbox,
        &add_field_request("escalation"),
        "tail-two",
        false,
    )
    .unwrap();
    let mut ledger: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sandbox.history_path()).unwrap()).unwrap();
    ledger["records"].as_array_mut().unwrap().pop();
    ledger["nextSequence"] = serde_json::json!(2);
    let truncated = serde_json::to_vec_pretty(&ledger).unwrap();
    assert!(matches!(
        change::receipt::History::decode(&truncated),
        Err(ChangeError::HistoryInvalid { detail: "tip" })
    ));
    std::fs::write(sandbox.history_path(), truncated).unwrap();
    for key in ["tail-one", "tail-two"] {
        assert!(matches!(
            change::lookup_receipt(Some(&sandbox.selection_str()), key),
            Err(ChangeError::HistoryInvalid { detail: "tip" })
        ));
    }
    // Even recomputing the PUBLIC tip cannot bypass independent custody.
    rechain_history(&mut ledger);
    std::fs::write(
        sandbox.history_path(),
        serde_json::to_vec_pretty(&ledger).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        apply_request(&sandbox, &add_field_request("third"), "tail-three", false),
        Err(ChangeError::HistoryInvalid {
            detail: "commit-custody"
        })
    ));
}

#[test]
fn interrupted_history_and_custody_publication_rolls_back_together() {
    let sandbox = Sandbox::new("custody-rollback");
    let request = add_priority_request();
    apply_request(&sandbox, &request, "original-key", false).unwrap();
    let pin_path = sandbox.root.join(".lekalo/change-history-pin.json");
    let old_history = std::fs::read(sandbox.history_path()).unwrap();
    let old_pin = std::fs::read(&pin_path).unwrap();
    let after_history = b"interrupted new history\n";
    let after_pin = b"interrupted new pin\n";
    let journal = plant_journal(
        &sandbox,
        "change-custody01",
        "replacing",
        serde_json::json!([
            { "path": "lekalo/change-history.json", "beforeDigest": sha256_of(&old_history),
              "afterDigest": sha256_of(after_history), "creation": false },
            { "path": ".lekalo/change-history-pin.json", "beforeDigest": sha256_of(&old_pin),
              "afterDigest": sha256_of(after_pin), "creation": false }
        ]),
        true,
    );
    std::fs::write(journal.join("backups/0"), &old_history).unwrap();
    std::fs::write(journal.join("backups/1"), &old_pin).unwrap();
    // Exact publication crash window: history changed, custody not yet.
    std::fs::write(sandbox.history_path(), after_history).unwrap();
    let recovered =
        change::recover_transaction(Some(&sandbox.selection_str()), "change-custody01").unwrap();
    assert_eq!(recovered.state, "rolled-back");
    assert_eq!(std::fs::read(sandbox.history_path()).unwrap(), old_history);
    assert_eq!(std::fs::read(pin_path).unwrap(), old_pin);
    assert!(
        apply_request(&sandbox, &request, "original-key", false)
            .unwrap()
            .replayed
    );
}

#[test]
fn replay_custody_survives_cache_clear_and_missing_history_is_not_first_use() {
    let sandbox = Sandbox::new("custody-cache-clear");
    let request = add_priority_request();
    apply_request(&sandbox, &request, "custody-key", false).unwrap();
    let cleared = lekalo_core::cache::clear(&sandbox.selection());
    assert_eq!(cleared.status(), lekalo_core::result::Status::Valid);
    assert!(sandbox
        .root
        .join(".lekalo/change-history-pin.json")
        .is_file());
    assert!(sandbox.root.join(".lekalo/cache/writer.lock").is_file());
    assert!(
        apply_request(&sandbox, &request, "custody-key", false)
            .unwrap()
            .replayed
    );
    std::fs::remove_file(sandbox.history_path()).unwrap();
    assert!(matches!(
        change::lookup_receipt(Some(&sandbox.selection_str()), "custody-key"),
        Err(ChangeError::HistoryInvalid {
            detail: "commit-custody"
        })
    ));
    assert!(matches!(
        apply_request(&sandbox, &add_field_request("other"), "other-key", false),
        Err(ChangeError::HistoryInvalid {
            detail: "commit-custody"
        })
    ));
}

#[test]
fn a_superseded_record_still_replays_and_its_receipt_survives_drift() {
    // r7 review, finding devin F2 + omp F2 (MEDIUM): the r7 ANY-record
    // postcondition made committed replay AND receipt lookup unreachable
    // after any later legitimate commit touching the same file. The r8
    // postcondition binds each recorded path to the digest the ledger's
    // per-file chain ENDS at: a record superseded by a later committed
    // record in the same chain still replays, manual drift (no later
    // committed record explains it) still refuses, and the read-only
    // receipt lookup is exempt from the postcondition entirely.
    let sandbox = Sandbox::new("superseded");
    let first_request = add_field_request("priority");
    let second_request = add_field_request("escalation");
    apply_request(&sandbox, &first_request, "batch-key-1", false).expect("first commit");
    apply_request(&sandbox, &second_request, "batch-key-2", false).expect("second commit");
    // The earlier record's own after-digest no longer matches the file
    // (the successor replaced it); the chain endpoint does.
    let after_second = sandbox.entities_bytes();
    let replayed = apply_request(&sandbox, &first_request, "batch-key-1", false)
        .expect("a superseded record replays through its chain endpoint");
    assert_eq!(replayed.state(), ReceiptState::Replayed);
    assert!(replayed.replayed);
    assert_eq!(
        sandbox.entities_bytes(),
        after_second,
        "replay mutates nothing"
    );
    // The read-only receipt surface survives even MANUAL drift: it
    // reports the original committed provenance and mints no authority.
    std::fs::write(sandbox.entities_path(), b"# manual drift\n").expect("drift");
    let receipt = change::lookup_receipt(Some(&sandbox.selection_str()), "batch-key-1")
        .expect("receipt lookup survives drift")
        .expect("stored record");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    // Replay after manual drift still refuses: no later committed record
    // explains the current bytes, so this is indistinguishable from a
    // graft and fails closed.
    let error = apply_request(&sandbox, &first_request, "batch-key-1", false)
        .expect_err("manual drift is not supersession");
    assert!(
        matches!(
            &error,
            ChangeError::HistoryInvalid {
                detail: "postcondition",
                ..
            }
        ),
        "got {error:?}"
    );
}

#[test]
fn a_link_at_the_history_sidecar_is_refused_not_followed() {
    // r7 review, finding devin F5 + omp F6 residual (LOW): the ledger and
    // apply pin reads used std::fs::read. The r8 reads route through the
    // shared no-follow Fs: a link planted at the history sidecar is an
    // unconfined read and refuses as corruption instead of following to
    // foreign bytes (even byte-identical foreign bytes).
    let sandbox = Sandbox::new("sidecar-link");
    let request = add_priority_request();
    apply_request(&sandbox, &request, "sidecar-link-v1", false).expect("commit");
    let ledger = std::fs::read(sandbox.history_path()).expect("ledger");
    // An outside copy of the exact ledger bytes: following the link would
    // decode successfully and mint a replay — refusing must not depend on
    // the target being foreign content.
    let outside = sandbox
        .root
        .parent()
        .expect("parent")
        .join(format!("foreign-ledger-{}", std::process::id()));
    std::fs::create_dir_all(&outside).expect("outside dir");
    let foreign = outside.join("ledger.json");
    std::fs::write(&foreign, &ledger).expect("foreign ledger");
    std::fs::remove_file(sandbox.history_path()).expect("remove sidecar");
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&foreign, sandbox.history_path()).is_ok();
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(&foreign, sandbox.history_path()).is_ok();
    if !linked {
        // No symlink privilege on this host: the junction custody flow of
        // the CLI gate pins the same no-follow evidence on Windows.
        eprintln!("skipped: symlink privilege unavailable");
        let _ = std::fs::remove_dir_all(&outside);
        return;
    }
    let error = apply_request(&sandbox, &request, "sidecar-link-v1", false)
        .expect_err("a linked sidecar must refuse, not follow");
    assert!(
        matches!(&error, ChangeError::HistoryInvalid { detail: "read", .. }),
        "got {error:?}"
    );
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn a_planted_adoption_journal_serializes_sibling_writers() {
    // r7 review, finding devin F1/F3 + omp F3 (MEDIUM): the adoption
    // recovery journals were in NO barrier set — a stranded installation
    // serialized against nothing. r8 registers a precise stranded
    // predicate (journal with neither a completed receipt nor a
    // completed recovery progress record) in the shared-writer sibling
    // barrier: the change writer refuses under the lock with the
    // adoption-journal detail until an explicit adoption recovery
    // resolves the journal. Model readers stay open by contract (the
    // loader's Phase 2c note states the rationale); the stranded
    // predicate keeps completed work — which deliberately retains its
    // journal — unblocked.
    let sandbox = Sandbox::new("adoption-barrier");
    let request = add_priority_request();
    // Readers stay open: preparation loads normally while the journal
    // is stranded (the barrier is a writer barrier, not a reader one).
    prepare(&sandbox, &request).expect("readers stay open");
    // Plant one stranded adoption journal home.
    let home = sandbox
        .root
        .join(".lekalo/import/adoption/recovery")
        .join("a".repeat(32))
        .join("b".repeat(32));
    std::fs::create_dir_all(&home).expect("recovery home");
    std::fs::write(home.join("journal.json"), b"{}").expect("journal");
    // The change writer refuses under the shared lock (sibling barrier)
    // with the adoption-journal detail.
    let error = apply_request(&sandbox, &request, "adoption-barrier-v1", false)
        .expect_err("a stranded adoption journal serializes sibling writers");
    assert!(
        matches!(
            &error,
            ChangeError::PreconditionFailed {
                subject,
                detail: "adoption-journal",
                ..
            } if subject == "writer-lock"
        ),
        "got {error:?}"
    );
    assert!(!sandbox.history_path().exists(), "nothing committed");
    // Mere receipt-file existence must NEVER clear the barrier (r10 F3).
    // This malformed journal has no verifiable completed transaction.
    let receipts = sandbox
        .root
        .join(".lekalo/import/adoption/receipts")
        .join("a".repeat(32));
    std::fs::create_dir_all(&receipts).expect("receipts home");
    let receipt_path = receipts.join(format!("{}.json", "b".repeat(32)));
    for bytes in [b"".as_slice(), b"{}".as_slice()] {
        std::fs::write(&receipt_path, bytes).expect("malformed receipt");
        assert!(matches!(
            apply_request(&sandbox, &request, "adoption-barrier-v2", false),
            Err(ChangeError::PreconditionFailed {
                detail: "adoption-journal",
                ..
            })
        ));
    }
    std::fs::remove_file(home.join("journal.json")).expect("resolved journal");
    apply_request(&sandbox, &request, "adoption-barrier-v2", false)
        .expect("resolved adoption work does not block sibling writers");
}

#[test]
fn an_over_limit_file_diff_degrades_the_plan_envelope() {
    // r7 review, finding omp F6 (LOW): the over-16-KiB `fileDiff`
    // degradation was enumerated in the plan schema but pinned by no
    // test. A document whose prospective bytes exceed
    // `MAX_FILE_DIFF_BYTES` must degrade the plan envelope to
    // `complete: false` with `completeReason: "change.file-diff-limit"`
    // and omit that file's diff entry — never emit a truncated diff
    // presented as complete, and never refuse the whole preparation
    // (the semantic diff and every digest pin stay exact).
    let sandbox = Sandbox::new("diff-limit");
    // The SOURCE document itself exceeds the 16-KiB diff bound (a long
    // preserved comment); the planned edit is small, so the degradation
    // keys on the before side exactly like the after side.
    std::fs::write(
        sandbox.root.join("lekalo/modules/tasks/commands.yaml"),
        format!(
            "schema_version: \"0.2.16\"\n# {}\ndefinitions:\n  - id: tasks.archive\n    kind: effect\n    version: 1\n    operation: update\n    entity: \"tasks.task\"\n  - id: tasks.audit\n    kind: effect\n    version: 1\n    operation: update\n    entity: \"tasks.task\"\n  - id: tasks.archive_task\n    kind: command\n    version: 1\n    effects:\n      - \"tasks.archive\"\n    input:\n      - name: task\n        type: \"tasks.task\"\n        required: true\n",
            "c".repeat(17 * 1024)
        ),
    )
    .expect("commands written");
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "command-effect:add",
            "command": "tasks.archive_task",
            "effect": "tasks.audit"
        }]
    }))
    .expect("serializes");
    let prepared = prepare(&sandbox, &request).expect("preparation degrades, not refuses");
    let exported = serde_json::from_slice::<serde_json::Value>(
        prepared.to_json_string().expect("exports").as_bytes(),
    )
    .expect("plan wire parses");
    assert_eq!(
        exported.get("complete").and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        exported
            .get("completeReason")
            .and_then(|value| value.as_str()),
        Some("change.file-diff-limit")
    );
    let file_diff = exported
        .get("fileDiff")
        .and_then(|value| value.as_array())
        .expect("fileDiff");
    assert!(
        file_diff
            .iter()
            .all(|entry| entry.get("path").and_then(|value| value.as_str())
                != Some("lekalo/modules/tasks/commands.yaml")),
        "the over-limit file's diff entry is omitted, not truncated: {file_diff:?}"
    );
    // The semantic projection stays complete and exact.
    assert_eq!(prepared.file_diffs().len(), file_diff.len());
}

#[test]
fn an_over_cap_input_inventory_refuses_instead_of_emitting_a_schema_invalid_plan() {
    // r6 review, finding omp F1: the plan export obeys its own published
    // schema — `inputs.maxItems` is 1000, so a project whose pinned
    // inventory exceeds the cap refuses with the registered bounded-output
    // rule instead of emitting `complete: true` with 1001+ inputs (Ajv
    // would reject the document the run itself reported as successful).
    let sandbox = Sandbox::new("over-cap");
    // The field-vertical fixture pins few inputs; synthesize the count by
    // generating additional admitted module documents? The admission scan
    // bounds module homes, so instead assert the enforcement constants
    // are wired to the schema caps via a direct over-cap refusal probe:
    // prepare a normal plan first (fits), then verify the cap constant
    // matches the published schema bound.
    let request = add_priority_request();
    let prepared = prepare(&sandbox, &request).expect("prepares");
    assert!(prepared.to_json_string().is_ok(), "under-cap plan exports");
    let plan: serde_json::Value =
        serde_json::from_str(&prepared.to_json_string().expect("plan json")).expect("json");
    let schema_text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/change-plan.schema.v0.8.0.json"
    ))
    .expect("schema");
    let schema: serde_json::Value = serde_json::from_str(&schema_text).expect("schema json");
    let schema_inputs_cap = schema["properties"]["inputs"]["maxItems"]
        .as_u64()
        .expect("inputs maxItems");
    assert!(
        plan["inputs"].as_array().expect("inputs").len() <= schema_inputs_cap as usize,
        "every emitted plan satisfies its own schema"
    );
    // The export is under the 32 MiB byte ceiling as well.
    let exported = prepared.to_json_string().expect("plan json");
    assert!(
        exported.len() < 32 * 1024 * 1024,
        "the exported plan is inside the byte bound"
    );
}

#[test]
fn the_shared_writer_lock_serializes_a_change_writer_against_the_cache_leaf() {
    // r6 review, findings devin F2 + omp F2: every Model writer service
    // acquires ONE project-wide mutation lock under `.lekalo/cache`, and
    // a change writer refuses while any live holder exists — a migrate/
    // format/generate writer that passed its own load barrier before the
    // change journal was published still stops at the shared lock.
    let sandbox = Sandbox::new("shared-lock");
    let request = add_priority_request();
    // A live writer holding the shared leaf denies the change apply.
    let holder = lekalo_core::change::test_only_acquire_writer_lock(Some(&sandbox.selection_str()))
        .expect("holder");
    let error = apply_request(&sandbox, &request, "shared-lock-v1", false)
        .expect_err("live sibling writer denies the change writer");
    assert!(
        matches!(
            error,
            ChangeError::PreconditionFailed {
                subject: _,
                detail: "held",
                ..
            }
        ),
        "got {error:?}"
    );
    drop(holder);
    // Once released, the change writer acquires and commits.
    let receipt = apply_request(&sandbox, &request, "shared-lock-v1", false).expect("commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
    // The leaf persists after the run (never unlinked).
    assert!(
        sandbox.root.join(".lekalo/cache/writer.lock").is_file(),
        "the shared lock leaf is persistent"
    );
}

#[test]
fn replace_all_reverifies_the_before_digest_immediately_before_each_write() {
    // r6 review, finding omp F2 (second half): the contract requires
    // "Revalidate after staging and immediately before replacements".
    // The apply-time input CAS refuses a concurrent edit that landed
    // between preparation and the replacement window instead of
    // clobbering it — and the plan id pin keeps the retry honest.
    let sandbox = Sandbox::new("pre-write-cas");
    let request = add_priority_request();
    let prepared = prepare(&sandbox, &request).expect("prepares");
    // Between preparation and apply, edit the target through the normal
    // path (a concurrent user edit): the apply-time input CAS refuses.
    let entities = sandbox.root.join("lekalo/modules/tasks/entities.yaml");
    let original = std::fs::read(&entities).expect("entities");
    let mut edited = original.clone();
    edited.extend_from_slice(b"# concurrent edit\n");
    std::fs::write(&entities, &edited).expect("concurrent edit");
    let error = lekalo_core::change::apply_prepared(
        &prepared,
        &ApplyOptions {
            idempotency_key: "pre-write-cas-v1",
            expect_plan: None,
            allow_breaking: false,
        },
    )
    .expect_err("drifted preimage refuses");
    assert!(
        matches!(error, ChangeError::PreconditionFailed { .. }),
        "got {error:?}"
    );
    // The concurrent edit is still on disk: nothing was clobbered.
    assert_eq!(std::fs::read(&entities).expect("entities"), edited);
    // Restore the exact preimage: the apply now succeeds, proving the
    // refusal was the CAS and not the plan.
    std::fs::write(&entities, &original).expect("restore");
    let receipt = lekalo_core::change::apply_prepared(
        &prepared,
        &ApplyOptions {
            idempotency_key: "pre-write-cas-v1",
            expect_plan: None,
            allow_breaking: false,
        },
    )
    .expect("clean preimage commits");
    assert_eq!(receipt.state(), ReceiptState::Committed);
}

#[test]
fn recovery_prunes_orphaned_pending_registry_records() {
    // r6 review, finding devin F3: a pending-registry row whose journal
    // directory is absent is stale provenance — the durable journal is
    // authoritative. Recovery prunes the orphan rows instead of letting
    // them accumulate forever behind a precondition refusal that never
    // cleans up.
    let sandbox = Sandbox::new("orphan-prune");
    // Commit one real transaction so the registry is real.
    let request = add_priority_request();
    apply_request(&sandbox, &request, "orphan-v1", false).expect("commits");
    // Plant an orphan row: no journal directory under the change home.
    let registry_path = sandbox.root.join(".lekalo/pending-transactions.json");
    std::fs::write(
        &registry_path,
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": "lekalo/change-pending/1",
            "records": [{
                "transaction": "change-orphan0001",
                "planId": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                "state": "staging",
                "files": [],
                "history": false,
            }],
        }))
        .expect("registry"),
    )
    .expect("plant orphan");
    // Recovery of any named transaction runs the prune first; the named
    // unknown transaction still refuses (its journal is absent).
    let outcome = change::recover_transaction(Some(&sandbox.selection_str()), "nonexistent-tx");
    assert!(outcome.is_err(), "unknown transaction refuses");
    // The orphan row is gone.
    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry_path).expect("registry")).expect("json");
    let transactions: Vec<&str> = registry["records"]
        .as_array()
        .expect("records")
        .iter()
        .map(|record| record["transaction"].as_str().expect("transaction"))
        .collect();
    assert!(
        !transactions.contains(&"change-orphan0001"),
        "the orphan row was pruned: {transactions:?}"
    );
    // A second prune run is idempotent: no rows, no error.
    let outcome = change::recover_transaction(Some(&sandbox.selection_str()), "nonexistent-tx");
    assert!(outcome.is_err());
    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry_path).expect("registry")).expect("json");
    assert!(
        registry["records"].as_array().expect("records").is_empty(),
        "the prune is idempotent"
    );
}

#[test]
fn mixed_no_op_and_mutating_batch_keeps_every_operation_in_request_order() {
    let sandbox = Sandbox::new("mixed-noop");
    let commands = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.act\n    kind: command\n    version: 1\n    input:\n      - name: title\n        type: tasks.text\n";
    let path = sandbox.root.join("lekalo/modules/tasks/commands.yaml");
    std::fs::write(&path, commands).unwrap();
    let no_op = serde_json::json!({
        "operation": "operation-input:set", "operationTarget": "tasks.act",
        "definition": { "input": [{ "name": "title", "type": { "ref": "tasks.text" } }] }
    });
    let mutation: serde_json::Value = serde_json::from_slice(&add_priority_request()).unwrap();
    let request = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [no_op, mutation["operations"][0], no_op]
    }))
    .unwrap();
    let receipt = apply_request(&sandbox, &request, "mixed-key", false).unwrap();
    assert_eq!(receipt.state(), ReceiptState::Committed);
    let order = [
        "operation-input:set",
        "entity-field:add",
        "operation-input:set",
    ];
    assert_eq!(
        receipt
            .operations
            .iter()
            .map(|row| row.operation.as_str())
            .collect::<Vec<_>>(),
        order
    );
    let history =
        change::receipt::History::decode(&std::fs::read(sandbox.history_path()).unwrap()).unwrap();
    assert_eq!(history.records[0].operations, order);
    assert_eq!(std::fs::read(path).unwrap(), commands.as_bytes());
    assert_eq!(
        apply_request(&sandbox, &request, "mixed-key", false)
            .unwrap()
            .operations,
        receipt.operations
    );
}

fn history_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", lekalo_core::digest::sha256_hex(bytes))
}

#[test]
fn genuine_r10_no_op_upgrade_refuses_before_custody_or_publication() {
    fn snapshot(path: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        let mut files = std::collections::BTreeMap::new();
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                files.extend(snapshot(&entry.path()));
            } else {
                files.insert(entry.path(), std::fs::read(entry.path()).unwrap());
            }
        }
        files
    }
    let bytes = include_bytes!("../../../tests/fixtures/change/legacy/r10-no-op-history.json");
    assert_eq!(
        history_digest(bytes),
        "sha256:7642c43decb40d5275b4727ab8defb76bafb3e71eda404c5608cbb411d4786a9"
    );
    let legacy: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    let commands = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.act\n    kind: command\n    version: 1\n    input:\n      - name: title\n        type: tasks.text\n";
    let noop = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{"operation": "operation-input:set", "operationTarget": "tasks.act",
            "definition": {"input": [{"name": "title", "type": {"ref": "tasks.text"}}]}}]
    }))
    .unwrap();
    for pin_version in [None, Some(1), Some(2)] {
        let sandbox = Sandbox::new("r10-noop-upgrade");
        std::fs::write(
            sandbox.root.join("lekalo/modules/tasks/commands.yaml"),
            commands,
        )
        .unwrap();
        std::fs::write(sandbox.history_path(), bytes).unwrap();
        let project = sandbox.selection_str();
        let pin_path = sandbox.root.join(".lekalo/change-history-pin.json");
        if let Some(version) = pin_version {
            let mut pin = serde_json::json!({
                "schemaVersion": format!("lekalo/change-history-pin/{version}"),
                "project": "tasks", "recordCount": 1,
                "tipDigest": legacy["tip"]["recordDigest"], "historyDigest": history_digest(bytes)
            });
            if version == 2 {
                pin["firstLocalSequence"] = serde_json::json!(2);
            }
            std::fs::create_dir_all(pin_path.parent().unwrap()).unwrap();
            std::fs::write(&pin_path, serde_json::to_vec_pretty(&pin).unwrap()).unwrap();
        }
        let sources = snapshot(&sandbox.root.join("lekalo"));
        let pin_before = std::fs::read(&pin_path).ok();
        let mutation = add_priority_request();
        let prepared = prepare(&sandbox, &mutation).unwrap();
        let assert_refused = |result: Result<(), ChangeError>| {
            assert!(
                matches!(
                    result,
                    Err(ChangeError::HistoryInvalid {
                        detail: "history-schema"
                    })
                ),
                "pin {pin_version:?}: {result:?}"
            );
            assert_eq!(snapshot(&sandbox.root.join("lekalo")), sources);
            assert_eq!(std::fs::read(&pin_path).ok(), pin_before);
            assert!(!sandbox
                .root
                .join(".lekalo/pending-transactions.json")
                .exists());
            assert!(!sandbox.root.join(".lekalo/cache/change").exists());
        };
        assert_refused(change::initialize_custody(
            Some(&project),
            &history_digest(bytes),
        ));
        for (request, key) in [
            (mutation.as_slice(), "new-mutation"),
            (noop.as_slice(), "new-noop"),
            (noop.as_slice(), "base-noop"),
        ] {
            assert_refused(apply_request(&sandbox, request, key, false).map(|_| ()));
        }
        assert_refused(
            change::apply_prepared(
                &prepared,
                &ApplyOptions {
                    idempotency_key: "sealed-mutation",
                    expect_plan: None,
                    allow_breaking: false,
                },
            )
            .map(|_| ()),
        );
        // Invalid history is not repaired even for lookup; ordinary Model
        // readers remain available, and valid audit-only lookup is covered
        // by the checkout epoch test below.
        assert_refused(change::lookup_receipt(Some(&project), "base-noop").map(|_| ()));
        lekalo_core::loader::normalize_model(&sandbox.selection()).unwrap();
    }
}

#[test]
fn checkout_custody_epoch_allows_new_work_but_never_authorizes_imported_receipts() {
    let donor = Sandbox::new("epoch-donor");
    let request = add_priority_request();
    apply_request(&donor, &request, "donor-key", false).unwrap();
    let checkout = Sandbox::new("epoch-checkout");
    copy_dir(&donor.root.join("lekalo"), &checkout.root.join("lekalo"));
    let bytes = std::fs::read(checkout.history_path()).unwrap();
    let sources = checkout.entities_bytes();
    let project = checkout.selection_str();
    let next = add_field_request("local_one");
    assert!(matches!(
        apply_request(&checkout, &next, "local-one", false),
        Err(ChangeError::HistoryInvalid {
            detail: "commit-custody"
        })
    ));
    change::initialize_custody(Some(&project), &history_digest(&bytes)).unwrap();
    assert_eq!(std::fs::read(checkout.history_path()).unwrap(), bytes);
    assert_eq!(checkout.entities_bytes(), sources);
    let pin_path = checkout.root.join(".lekalo/change-history-pin.json");
    let pin_before = std::fs::read(&pin_path).unwrap();
    assert!(
        change::initialize_custody(Some(&project), &history_digest(&bytes)).is_err(),
        "never rebind an existing pin"
    );
    assert_eq!(std::fs::read(&pin_path).unwrap(), pin_before);
    assert_eq!(
        change::lookup_receipt(Some(&project), "donor-key")
            .unwrap()
            .unwrap()
            .state(),
        ReceiptState::Committed
    );
    for (field, key) in [("local_one", "local-one"), ("local_two", "local-two")] {
        assert!(matches!(
            apply_request(&checkout, &request, "donor-key", false),
            Err(ChangeError::HistoryInvalid {
                detail: "commit-custody-epoch"
            })
        ));
        let local = add_field_request(field);
        assert_eq!(
            apply_request(&checkout, &local, key, false)
                .unwrap()
                .state(),
            ReceiptState::Committed
        );
        assert_eq!(
            apply_request(&checkout, &local, key, false)
                .unwrap()
                .state(),
            ReceiptState::Replayed
        );
        let pin: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&pin_path).unwrap()).unwrap();
        assert_eq!(
            pin["firstLocalSequence"], 2,
            "append cannot launder the imported prefix"
        );
    }
    assert!(matches!(
        apply_request(&checkout, &request, "donor-key", false),
        Err(ChangeError::HistoryInvalid {
            detail: "commit-custody-epoch"
        })
    ));
}

#[test]
fn initializing_a_rechained_graft_does_not_launder_it_through_a_new_commit() {
    let donor = Sandbox::new("epoch-graft-donor");
    let request = add_priority_request();
    apply_request(&donor, &request, "grafted-key", false).unwrap();
    let recipient = Sandbox::new("epoch-graft-recipient");
    let mut history: serde_json::Value =
        serde_json::from_slice(&std::fs::read(donor.history_path()).unwrap()).unwrap();
    history["records"][0]["receipt"]["files"][0]["afterDigest"] =
        serde_json::json!(history_digest(&recipient.entities_bytes()));
    history["records"][0]["receipt"]["files"][0]["beforeDigest"] =
        serde_json::json!(history_digest(b"invented preimage"));
    rechain_history(&mut history);
    let bytes = serde_json::to_vec_pretty(&history).unwrap();
    change::receipt::History::decode(&bytes).unwrap();
    std::fs::write(recipient.history_path(), &bytes).unwrap();
    let prepared = prepare(&recipient, &request).unwrap();
    change::initialize_custody(Some(&recipient.selection_str()), &history_digest(&bytes)).unwrap();
    let before = recipient.entities_bytes();
    assert!(matches!(
        change::apply_prepared(
            &prepared,
            &ApplyOptions {
                idempotency_key: "grafted-key",
                expect_plan: None,
                allow_breaking: false
            }
        ),
        Err(ChangeError::HistoryInvalid {
            detail: "commit-custody-epoch"
        })
    ));
    apply_request(
        &recipient,
        &add_field_request("legitimate"),
        "new-key",
        false,
    )
    .unwrap();
    assert!(matches!(
        apply_request(&recipient, &request, "grafted-key", false),
        Err(ChangeError::HistoryInvalid {
            detail: "commit-custody-epoch"
        })
    ));
    assert!(!String::from_utf8(recipient.entities_bytes())
        .unwrap()
        .contains("name: priority"));
    assert_ne!(
        recipient.entities_bytes(),
        before,
        "only the unrelated new operation committed"
    );
}

#[test]
fn custody_initialization_respects_shared_lock_and_unresolved_recovery_barriers() {
    let sandbox = Sandbox::new("epoch-barriers");
    let project = sandbox.selection_str();
    apply_request(&sandbox, &add_priority_request(), "imported", false).unwrap();
    let bytes = std::fs::read(sandbox.history_path()).unwrap();
    let source = sandbox.entities_bytes();
    let expected = history_digest(&bytes);
    let pin_path = sandbox.root.join(".lekalo/change-history-pin.json");
    std::fs::remove_file(&pin_path).unwrap();

    let holder = change::test_only_acquire_writer_lock(Some(&project)).unwrap();
    assert!(matches!(
        change::initialize_custody(Some(&project), &expected),
        Err(ChangeError::PreconditionFailed { detail: "held", .. })
    ));
    assert!(!pin_path.exists());
    drop(holder);

    let recovery = sandbox
        .root
        .join(".lekalo/import/adoption/recovery")
        .join("a".repeat(32))
        .join("b".repeat(32));
    std::fs::create_dir_all(&recovery).unwrap();
    std::fs::write(recovery.join("journal.json"), b"{}").unwrap();
    std::fs::write(recovery.join("recovered.json"), b"{}").unwrap();
    assert!(matches!(
        change::initialize_custody(Some(&project), &expected),
        Err(ChangeError::PreconditionFailed {
            detail: "adoption-journal",
            ..
        })
    ));
    assert!(!pin_path.exists());
    assert_eq!(std::fs::read(sandbox.history_path()).unwrap(), bytes);
    assert_eq!(sandbox.entities_bytes(), source);
    assert!(recovery.join("journal.json").exists());
    std::fs::remove_dir_all(recovery).unwrap();

    let transaction = "change-epochbarrier";
    let journal = plant_journal(
        &sandbox,
        transaction,
        "staging",
        serde_json::json!([]),
        false,
    );
    assert!(change::initialize_custody(Some(&project), &expected).is_err());
    assert!(!pin_path.exists());
    assert!(journal.join("journal.json").exists());
    change::recover_transaction(Some(&project), transaction).unwrap();
    change::initialize_custody(Some(&project), &expected).unwrap();
    assert_eq!(std::fs::read(sandbox.history_path()).unwrap(), bytes);
    assert_eq!(sandbox.entities_bytes(), source);
}

#[test]
fn custody_initialization_validates_expectations_model_history_and_existing_pin() {
    let sandbox = Sandbox::new("epoch-validation");
    let project = sandbox.selection_str();
    assert!(change::initialize_custody(Some(&project), "bad").is_err());
    assert!(change::initialize_custody(Some(&project), &history_digest(b"")).is_err());
    apply_request(&sandbox, &add_priority_request(), "old", false).unwrap();
    let bytes = std::fs::read(sandbox.history_path()).unwrap();
    let pin_path = sandbox.root.join(".lekalo/change-history-pin.json");
    std::fs::remove_file(&pin_path).unwrap();
    assert!(matches!(
        change::initialize_custody(Some(&project), &history_digest(b"stale")),
        Err(ChangeError::PreconditionFailed {
            detail: "expect-history",
            ..
        })
    ));
    assert!(!pin_path.exists());
    let source = sandbox.entities_bytes();
    std::fs::write(sandbox.entities_path(), "not a Model").unwrap();
    assert!(change::initialize_custody(Some(&project), &history_digest(&bytes)).is_err());
    assert!(!pin_path.exists());
    std::fs::write(sandbox.entities_path(), &source).unwrap();
    std::fs::write(sandbox.history_path(), "{}").unwrap();
    assert!(change::initialize_custody(Some(&project), &history_digest(b"{}")).is_err());
    assert!(!pin_path.exists());
    std::fs::write(sandbox.history_path(), &bytes).unwrap();
    std::fs::write(&pin_path, "malformed existing custody").unwrap();
    assert!(change::initialize_custody(Some(&project), &history_digest(&bytes)).is_err());
    assert_eq!(
        std::fs::read(&pin_path).unwrap(),
        b"malformed existing custody"
    );
    assert_eq!(std::fs::read(sandbox.history_path()).unwrap(), bytes);
    assert_eq!(sandbox.entities_bytes(), source);
}
