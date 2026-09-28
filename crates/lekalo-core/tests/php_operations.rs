//! Issue #59 library tests for the PHP operations join: the hermetic
//! operations fixture through the accepted loader seam, the closed
//! input shape, the four-authority join (compiled IR, embedded #62
//! registry, staged evidence digests, closed recipes), and the bounded
//! negative matrix. Every refusal happens before any generated write.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lekalo_core::ir::CompiledProject;
use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::php_operations::{
    check_join, digest_context, parse_input, Finding, Mode, Recipe, TransactionMode,
};

const FIXTURE: &str = "tests/fixtures/php-laravel/operations/model";
const INPUT_TEMPLATE: &str = "tests/fixtures/php-laravel/operations/inputs/planner.operations.json";

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

fn enter_workspace() -> PathBuf {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    original
}

fn compile_fixture() -> CompiledProject {
    let selection = LoadSelection {
        project: Some(FIXTURE.to_owned()),
    };
    let model = normalize_model(&selection)
        .unwrap_or_else(|outcome| panic!("fixture load failed: {}", outcome.to_json_string()));
    lekalo_core::ir::compile(&model)
        .unwrap_or_else(|failure| panic!("fixture IR failed: {:?}", failure))
        .project
}

/// One sandbox with the staged evidence homes of the fixture project.
struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "lekalo-php-operations-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".lekalo/cache/ir")).expect("sandbox evidence home");
        std::fs::create_dir_all(dir.join("lekalo/types")).expect("sandbox types home");
        Self { dir }
    }

    /// Stage the exact fixture bytes: canonical IR evidence, a bound
    /// types input, and the operations input with real digests.
    fn stage_fixture(&self, compilation: &CompiledProject) -> lekalo_core::php_operations::OperationsInput {
        let ir_bytes = compilation.to_canonical_json();
        std::fs::write(
            self.dir.join(".lekalo/cache/ir/planner.json"),
            ir_bytes.as_bytes(),
        )
        .expect("staged ir evidence");
        let ir_digest = format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(ir_bytes.as_bytes())
        );
        let types_input = format!(
            "{{\"identity\":\"dev.lekalo.php-types-input@0.4.0\",\"irDigest\":\"{ir_digest}\",\
             \"projectId\":\"planner\",\"schemaVersion\":\"lekalo/php-types-input/v0.4.0\"}}\n"
        );
        std::fs::write(
            self.dir.join("lekalo/types/planner.types.json"),
            types_input.as_bytes(),
        )
        .expect("staged types input");
        let template = std::fs::read_to_string(workspace_root().join(INPUT_TEMPLATE))
            .expect("operations input template");
        let types_digest = format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(types_input.as_bytes())
        );
        let zeros = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
        let input_bytes = template.replace(
            &format!("\"irDigest\": \"{zeros}\""),
            &format!("\"irDigest\": \"{ir_digest}\""),
        ).replace(
            &format!("\"typesInputDigest\": \"{zeros}\""),
            &format!("\"typesInputDigest\": \"{types_digest}\""),
        );
        std::fs::write(self.dir.join("lekalo/operations.placeholder"), "").ok();
        let input = parse_input(input_bytes.as_bytes()).expect("template input parses");
        // Rewrite the input bytes in the sandbox for digest-context tests.
        std::fs::create_dir_all(self.dir.join("lekalo/operations")).expect("operations home");
        std::fs::write(
            self.dir.join("lekalo/operations/planner.operations.json"),
            input_bytes.as_bytes(),
        )
        .expect("staged operations input");
        input
    }

    fn rewrite_input(&self, input_bytes: &[u8]) {
        std::fs::write(
            self.dir.join("lekalo/operations/planner.operations.json"),
            input_bytes,
        )
        .expect("rewrite operations input");
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn finding_codes(findings: &[Finding]) -> Vec<String> {
    findings.iter().map(|finding| finding.code.clone()).collect()
}

#[test]
fn the_fixture_input_parses_into_the_closed_shape() {
    let _original = enter_workspace();
    let bytes = std::fs::read(workspace_root().join(INPUT_TEMPLATE)).expect("template");
    let input = parse_input(&bytes).expect("the committed template input parses");
    assert_eq!(input.project_id, "planner");
    assert_eq!(input.namespace_prefix, "Lekalo\\Generated\\Operations");
    assert_eq!(input.operations.len(), 2);
    assert_eq!(input.operations[0].id, "planner.count_focused");
    assert_eq!(input.operations[1].id, "planner.focus_task");
    assert_eq!(input.operations[0].mode, Mode::Managed);
    assert_eq!(
        input.operations[1].transaction,
        TransactionMode::Required
    );
    match input.operations[1].recipe.as_ref().expect("managed recipe") {
        Recipe::SingleEntityUpdate {
            entity, key, kept, ..
        } => {
            assert_eq!(entity.as_str(), "planner.task");
            assert_eq!(key.as_str(), "task_id");
            assert_eq!(kept.as_slice(), ["title"].map(str::to_owned));
        }
        other => panic!("unexpected recipe: {other:?}"),
    }
}

#[test]
fn the_coherent_fixture_joins_clean() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("clean");
    let input = sandbox.stage_fixture(&compilation);
    let findings = check_join(&sandbox.dir, &input, &compilation);
    assert!(
        findings.is_empty(),
        "the coherent fixture join must be clean: {findings:?}"
    );
    let digests = digest_context(&sandbox.dir, "planner").expect("digests");
    assert_eq!(digests.ir_digest, input.ir_digest);
}

#[test]
fn a_digest_divergence_is_a_binding_finding() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("stale");
    let mut input = sandbox.stage_fixture(&compilation);
    input.ir_digest = "sha256:1111111111111111111111111111111111111111111111111111111111111111"
        .to_owned();
    let codes = finding_codes(&check_join(&sandbox.dir, &input, &compilation));
    assert!(codes.contains(&"operations.ir-digest".to_owned()), "{codes:?}");
}

#[test]
fn a_wrong_reference_kind_is_a_finding() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("kind");
    let input = sandbox.stage_fixture(&compilation);
    let mut tampered = input.clone();
    tampered.operations[0].kind = lekalo_core::php_operations::OperationKind::Command;
    let codes = finding_codes(&check_join(&sandbox.dir, &tampered, &compilation));
    assert!(codes.contains(&"operations.kind-mismatch".to_owned()), "{codes:?}");
}

#[test]
fn undeclared_or_wrong_errors_refuse_against_the_registry_binding() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("errors");
    let input = sandbox.stage_fixture(&compilation);
    let mut tampered = input.clone();
    tampered.operations[1].errors = vec!["planner.task_not_found".to_owned()];
    let codes = finding_codes(&check_join(&sandbox.dir, &tampered, &compilation));
    assert!(codes.contains(&"operations.registry-binding".to_owned()), "{codes:?}");
    // The embedded registry carries no binding for an unknown operation.
    tampered.operations[0].id = "planner.unbound_query".to_owned();
    let codes = finding_codes(&check_join(&sandbox.dir, &tampered, &compilation));
    assert!(
        codes.contains(&"operations.operation-unresolved".to_owned())
            && codes.contains(&"operations.registry-binding".to_owned()),
        "{codes:?}"
    );
}

#[test]
fn a_query_never_carries_a_write_recipe() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("query-write");
    let input = sandbox.stage_fixture(&compilation);
    let mut tampered = input.clone();
    tampered.operations[0].recipe = Some(Recipe::SingleEntityUpdate {
        entity: tampered.operations[0].id.clone(),
        key: "task_id".to_owned(),
        assignments: Vec::new(),
        kept: Vec::new(),
        preconditions: Vec::new(),
        missing_error: "planner.store_unavailable".to_owned(),
        emissions: Vec::new(),
    });
    let codes = finding_codes(&check_join(&sandbox.dir, &tampered, &compilation));
    assert!(codes.contains(&"operations.query-write".to_owned()), "{codes:?}");
}

#[test]
fn a_recipe_operand_type_mismatch_is_a_finding() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("operand");
    let input = sandbox.stage_fixture(&compilation);
    // The emit payload operand `{fromEntity: state}` carries the enum,
    // while the event field needs the uuid - an exact type mismatch.
    let mut tampered = input.clone();
    if let Some(Recipe::SingleEntityUpdate { emissions, .. }) =
        tampered.operations[1].recipe.as_mut()
    {
        emissions[0].1.insert(
            "task_id".to_owned(),
            lekalo_core::php_operations::Operand::FromEntity("state".to_owned()),
        );
    }
    let codes = finding_codes(&check_join(&sandbox.dir, &tampered, &compilation));
    assert!(codes.contains(&"operations.type-mismatch".to_owned()), "{codes:?}");
    // A precondition failure error outside the declared binding set is
    // a registry-binding finding.
    let mut tampered = input.clone();
    if let Some(Recipe::SingleEntityUpdate { preconditions, .. }) =
        tampered.operations[1].recipe.as_mut()
    {
        preconditions[0].error = "planner.unknown_error".to_owned();
    }
    let codes = finding_codes(&check_join(&sandbox.dir, &tampered, &compilation));
    assert!(codes.contains(&"operations.registry-binding".to_owned()), "{codes:?}");
}

#[test]
fn a_policy_binding_must_apply_to_the_operation() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("policy");
    let input = sandbox.stage_fixture(&compilation);
    let mut tampered = input.clone();
    tampered.operations[0].policy = Some("planner.deny_bulk_focus".to_owned());
    let codes = finding_codes(&check_join(&sandbox.dir, &tampered, &compilation));
    assert!(codes.contains(&"operations.policy-unresolved".to_owned()), "{codes:?}");
}

#[test]
fn missing_staged_evidence_refuses_before_any_join() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("absent");
    // Nothing staged: the digest context is unavailable.
    assert!(digest_context(&sandbox.dir, "planner").is_none());
    let input = sandbox.stage_fixture(&compilation);
    // Remove the evidence after staging.
    std::fs::remove_file(sandbox.dir.join(".lekalo/cache/ir/planner.json")).expect("remove");
    let codes = finding_codes(&check_join(&sandbox.dir, &input, &compilation));
    assert_eq!(codes, vec!["operations.evidence-unavailable".to_owned()]);
}

#[test]
fn unchecked_modes_require_the_declared_entrypoint() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("entry");
    let input = sandbox.stage_fixture(&compilation);
    let mut tampered = input.clone();
    tampered.operations[1].mode = Mode::Checked;
    let codes = finding_codes(&check_join(&sandbox.dir, &tampered, &compilation));
    assert!(codes.contains(&"operations.entry-required".to_owned()), "{codes:?}");
    assert!(!Mode::Checked.emits(), "checked never emits");
    assert!(!Mode::Custom.emits(), "custom never emits");
    assert!(Mode::ScaffoldOnce.emits());
}
