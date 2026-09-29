//! Issue #60 library tests for the PHP routes join: the hermetic routes
//! fixture through the accepted loader seam, the closed input shape,
//! and the four-authority join (compiled IR endpoints, the transport
//! attachment, the staged evidence digests, the bound types/operations
//! inputs). Every refusal happens before any generated write.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lekalo_core::ir::CompiledProject;
use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::php_routes::{check_join, digest_context, parse_input, Finding, Mode};

const FIXTURE: &str = "tests/fixtures/php-laravel/routes/model";
const INPUT_TEMPLATE: &str = "tests/fixtures/php-laravel/routes/inputs/planner.routes.json";
const TRANSPORT_TEMPLATE: &str = "tests/fixtures/php-laravel/routes/inputs/transport.json";
const OPENAPI_GOLDEN: &str =
    "tests/fixtures/php-laravel/routes/inputs/openapi/planner.openapi.json";
const ZEROS: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

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
        let dir =
            std::env::temp_dir().join(format!("lekalo-php-routes-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".lekalo/cache/ir")).expect("sandbox evidence home");
        std::fs::create_dir_all(dir.join(".lekalo/cache/transport")).expect("transport home");
        std::fs::create_dir_all(dir.join(".lekalo/cache/openapi")).expect("openapi home");
        std::fs::create_dir_all(dir.join("lekalo/types")).expect("types home");
        std::fs::create_dir_all(dir.join("lekalo/operations")).expect("operations home");
        std::fs::create_dir_all(dir.join("lekalo/routes")).expect("routes home");
        Self { dir }
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        std::fs::write(self.dir.join(relative), bytes).expect("sandbox write");
    }

    /// Stage the exact fixture bytes and derive the real digest set.
    fn stage_fixture(&self, compilation: &CompiledProject) -> StagedDigests {
        let ir_bytes = compilation.to_canonical_json();
        self.write(".lekalo/cache/ir/planner.json", ir_bytes.as_bytes());
        let ir_digest = format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(ir_bytes.as_bytes())
        );
        // The transport fixture pins the real IR digest; its bytes are
        // canonical, so the staged evidence is the exact document.
        let transport_bytes =
            std::fs::read(workspace_root().join(TRANSPORT_TEMPLATE)).expect("transport fixture");
        assert!(transport_bytes.ends_with(b"\n"), "the fixture ends with LF");
        let canonical_transport = &transport_bytes[..transport_bytes.len() - 1];
        self.write(".lekalo/cache/transport/planner.json", canonical_transport);
        self.write("lekalo/transport.yaml", canonical_transport);
        let transport_digest = format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(canonical_transport)
        );
        let openapi_bytes =
            std::fs::read(workspace_root().join(OPENAPI_GOLDEN)).expect("openapi golden");
        let canonical_openapi = &openapi_bytes[..openapi_bytes.len() - 1];
        self.write(".lekalo/cache/openapi/planner.json", canonical_openapi);
        let types_input = format!(
            "{{\"identity\":\"dev.lekalo.php-types-input@0.4.0\",\"irDigest\":\"{ir_digest}\",\
             \"projectId\":\"planner\",\"schemaVersion\":\"lekalo/php-types-input/v0.4.0\"}}\n"
        );
        self.write("lekalo/types/planner.types.json", types_input.as_bytes());
        let types_digest = format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(types_input.as_bytes())
        );
        let operations_template = std::fs::read_to_string(
            workspace_root()
                .join("tests/fixtures/php-laravel/routes/inputs/planner.operations.json"),
        )
        .expect("operations template");
        let operations_bytes = operations_template
            .replace(
                &format!("\"irDigest\": \"{ZEROS}\""),
                &format!("\"irDigest\": \"{ir_digest}\""),
            )
            .replace(
                &format!("\"typesInputDigest\": \"{ZEROS}\""),
                &format!("\"typesInputDigest\": \"{types_digest}\""),
            );
        self.write(
            "lekalo/operations/planner.operations.json",
            operations_bytes.as_bytes(),
        );
        let operations_digest = format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(operations_bytes.as_bytes())
        );
        StagedDigests {
            ir_digest,
            transport_digest,
            types_digest,
            operations_digest,
        }
    }

    /// Stage the routes input from the committed template with real digests.
    fn stage_routes_input(&self, digests: &StagedDigests) -> String {
        let template =
            std::fs::read_to_string(workspace_root().join(INPUT_TEMPLATE)).expect("template");
        let bytes = digests.routes_bytes(&template);
        self.write("lekalo/routes/planner.routes.json", bytes.as_bytes());
        bytes
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct StagedDigests {
    ir_digest: String,
    transport_digest: String,
    types_digest: String,
    operations_digest: String,
}

impl StagedDigests {
    /// The routes input template with every pin replaced by the staged
    /// digest set, as raw JSON text.
    fn routes_bytes(&self, template: &str) -> String {
        template
            .replace(
                &format!("\"irDigest\": \"{ZEROS}\""),
                &format!("\"irDigest\": \"{}\"", self.ir_digest),
            )
            .replace(
                &format!("\"transportDigest\": \"{ZEROS}\""),
                &format!("\"transportDigest\": \"{}\"", self.transport_digest),
            )
            .replace(
                &format!("\"typesInputDigest\": \"{ZEROS}\""),
                &format!("\"typesInputDigest\": \"{}\"", self.types_digest),
            )
            .replace(
                &format!("\"operationsInputDigest\": \"{ZEROS}\""),
                &format!("\"operationsInputDigest\": \"{}\"", self.operations_digest),
            )
    }
}

fn finding_codes(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| finding.code.clone())
        .collect()
}

/// Parse the (possibly tampered) input bytes into the typed shape.
fn typed(bytes: &str) -> lekalo_core::php_routes::RoutesInput {
    parse_input(bytes.as_bytes()).expect("the tampered input stays shape-valid")
}

#[test]
fn the_committed_template_parses_into_the_closed_shape() {
    let _original = enter_workspace();
    let bytes = std::fs::read(workspace_root().join(INPUT_TEMPLATE)).expect("template");
    let input = parse_input(&bytes).expect("the committed template input parses");
    assert_eq!(input.project_id, "planner");
    assert_eq!(input.namespace_prefix, "Lekalo\\Generated\\Routes");
    assert_eq!(input.middleware.len(), 1);
    assert_eq!(input.middleware[0].scheme, "user_bearer");
    assert_eq!(input.middleware[0].middleware, "fixture.auth");
    assert_eq!(input.routes.len(), 11);
    assert_eq!(input.routes[0].id, "planner.endpoint_backlog");
    assert_eq!(input.routes[0].mode, Mode::Managed);
    assert_eq!(input.routes[7].id, "planner.endpoint_tasks_focus");
    assert_eq!(input.routes[7].mode, Mode::Checked);
    let entry = input.routes[7].entry.as_ref().expect("checked entry");
    assert_eq!(entry.fqn, "App\\Http\\Controllers\\TaskFocusController");
    assert_eq!(entry.method, "focus");
    assert_eq!(input.routes[10].id, "planner.endpoint_unplan");
}

#[test]
fn the_accepted_join_is_empty_over_the_staged_fixture() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("accepted");
    let digests = sandbox.stage_fixture(&compilation);
    let bytes = sandbox.stage_routes_input(&digests);
    let input = parse_input(bytes.as_bytes()).expect("input parses");
    let findings = check_join(sandbox.dir.as_path(), &input, &compilation);
    assert!(findings.is_empty(), "unexpected findings: {findings:?}");
    let context =
        digest_context(sandbox.dir.as_path(), "planner").expect("the digest context resolves");
    assert_eq!(context.ir_digest, digests.ir_digest);
    assert_eq!(context.transport_digest, digests.transport_digest);
    assert_eq!(context.types_input_digest, digests.types_digest);
    assert_eq!(context.operations_input_digest, digests.operations_digest);
}

#[test]
fn a_missing_home_refuses_as_evidence_unavailable() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("empty");
    let bytes = std::fs::read_to_string(workspace_root().join(INPUT_TEMPLATE)).expect("template");
    let input = parse_input(bytes.as_bytes()).expect("input parses");
    let findings = check_join(sandbox.dir.as_path(), &input, &compilation);
    assert_eq!(
        finding_codes(&findings),
        vec!["routes.evidence-unavailable"]
    );
}

#[test]
fn diverging_digests_are_typed_findings() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("digests");
    let digests = sandbox.stage_fixture(&compilation);
    let template =
        std::fs::read_to_string(workspace_root().join(INPUT_TEMPLATE)).expect("template");
    let bytes = digests.routes_bytes(&template);
    // Tamper with each pin one at a time.
    for (member, expected) in [
        ("irDigest", "routes.ir-digest"),
        ("transportDigest", "routes.transport-digest"),
        ("typesInputDigest", "routes.types-unbound"),
        ("operationsInputDigest", "routes.operations-unbound"),
    ] {
        let mut value: serde_json::Value = serde_json::from_str(&bytes).expect("raw input");
        value[member] = serde_json::Value::String(format!("sha256:{}", "f".repeat(64)));
        let tampered = typed(&value.to_string());
        let findings = check_join(sandbox.dir.as_path(), &tampered, &compilation);
        assert!(
            finding_codes(&findings).contains(&expected.to_owned()),
            "{member}: {findings:?}"
        );
    }
}

#[test]
fn unresolved_symbols_and_broken_pairings_are_typed_findings() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("symbols");
    let digests = sandbox.stage_fixture(&compilation);
    let template =
        std::fs::read_to_string(workspace_root().join(INPUT_TEMPLATE)).expect("template");
    let bytes = digests.routes_bytes(&template);

    // A foreign endpoint id (aaa keeps the canonical array order).
    let mut value: serde_json::Value = serde_json::from_str(&bytes).expect("raw input");
    value["routes"][0]["id"] = serde_json::Value::String("planner.endpoint_aaa".to_owned());
    value["routes"][0]["operation"] = serde_json::Value::String("planner.focus_task".to_owned());
    let findings = check_join(
        sandbox.dir.as_path(),
        &typed(&value.to_string()),
        &compilation,
    );
    assert!(
        finding_codes(&findings).contains(&"routes.endpoint-unresolved".to_owned()),
        "a foreign endpoint refuses: {findings:?}"
    );

    // An operation the endpoint never invokes.
    let mut value: serde_json::Value = serde_json::from_str(&bytes).expect("raw input");
    value["routes"][0]["operation"] = serde_json::Value::String("planner.list_tasks".to_owned());
    let findings = check_join(
        sandbox.dir.as_path(),
        &typed(&value.to_string()),
        &compilation,
    );
    assert!(
        finding_codes(&findings).contains(&"routes.invokes-mismatch".to_owned()),
        "an invokes mismatch refuses: {findings:?}"
    );

    // A non-command/query operation symbol.
    let mut value: serde_json::Value = serde_json::from_str(&bytes).expect("raw input");
    value["routes"][0]["operation"] = serde_json::Value::String("planner.task".to_owned());
    let findings = check_join(
        sandbox.dir.as_path(),
        &typed(&value.to_string()),
        &compilation,
    );
    assert!(
        finding_codes(&findings).contains(&"routes.invokes-mismatch".to_owned()),
        "a foreign operation symbol refuses: {findings:?}"
    );

    // Checked without entry (the checked record rides index 7 of the
    // canonical id order).
    let mut value: serde_json::Value = serde_json::from_str(&bytes).expect("raw input");
    value["routes"][7] = serde_json::json!({
        "id": "planner.endpoint_tasks_focus",
        "operation": "planner.focus_task",
        "mode": "checked",
    });
    let findings = check_join(
        sandbox.dir.as_path(),
        &typed(&value.to_string()),
        &compilation,
    );
    assert!(
        finding_codes(&findings).contains(&"routes.entry-required".to_owned()),
        "checked without entry refuses: {findings:?}"
    );

    // Managed with a declared entry.
    let mut value: serde_json::Value = serde_json::from_str(&bytes).expect("raw input");
    value["routes"][0]["entry"] = serde_json::json!({
        "fqn": "App\\Http\\Controllers\\TaskFocusController",
        "method": "focus",
    });
    let findings = check_join(
        sandbox.dir.as_path(),
        &typed(&value.to_string()),
        &compilation,
    );
    assert!(
        finding_codes(&findings).contains(&"routes.entry-required".to_owned()),
        "managed with a foreign entry refuses: {findings:?}"
    );
}

#[test]
fn an_unbound_endpoint_and_a_foreign_scheme_are_typed_findings() {
    let _original = enter_workspace();
    let compilation = compile_fixture();
    let sandbox = Sandbox::new("wire");
    let digests = sandbox.stage_fixture(&compilation);
    let template =
        std::fs::read_to_string(workspace_root().join(INPUT_TEMPLATE)).expect("template");
    let bytes = digests.routes_bytes(&template);

    // Drop the today binding from the transport home: the route record
    // no longer joins the wire.
    let transport = std::fs::read_to_string(workspace_root().join(TRANSPORT_TEMPLATE))
        .expect("transport fixture");
    let mut document: serde_json::Value = serde_json::from_str(&transport).expect("transport");
    document["endpoints"] = serde_json::Value::Array(
        document["endpoints"]
            .as_array()
            .expect("endpoints")
            .iter()
            .filter(|endpoint| endpoint["endpoint"] != serde_json::json!("planner.endpoint_today"))
            .cloned()
            .collect(),
    );
    // The canonical member order is part of the wire; rebuild the
    // document bytes canonically.
    let canonical = canonical_json(&document);
    sandbox.write(".lekalo/cache/transport/planner.json", canonical.as_bytes());
    sandbox.write("lekalo/transport.yaml", canonical.as_bytes());
    // Re-pin the transport digest so only the binding join fails.
    let transport_digest = format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(canonical.as_bytes())
    );
    let mut value: serde_json::Value = serde_json::from_str(&bytes).expect("raw input");
    value["transportDigest"] = serde_json::Value::String(transport_digest);
    let input = typed(&value.to_string());
    let findings = check_join(sandbox.dir.as_path(), &input, &compilation);
    assert!(
        finding_codes(&findings).contains(&"routes.transport-unbound".to_owned()),
        "a dropped wire binding refuses: {findings:?}"
    );

    // A middleware scheme the attachment never declares.
    let fixture_bytes = std::fs::read(workspace_root().join(TRANSPORT_TEMPLATE)).expect("fixture");
    let canonical_transport = &fixture_bytes[..fixture_bytes.len() - 1];
    sandbox.write(".lekalo/cache/transport/planner.json", canonical_transport);
    sandbox.write("lekalo/transport.yaml", canonical_transport);
    let mut value: serde_json::Value = serde_json::from_str(&bytes).expect("raw input");
    value["policy"]["middleware"] = serde_json::json!([
        {"scheme": "svc_apikey", "middleware": "fixture.auth"},
    ]);
    let findings = check_join(
        sandbox.dir.as_path(),
        &typed(&value.to_string()),
        &compilation,
    );
    assert!(
        finding_codes(&findings).contains(&"routes.scheme-unresolved".to_owned()),
        "a foreign scheme mapping refuses: {findings:?}"
    );
}

/// Canonical compact JSON with byte-sorted keys (the staged transport
/// form inside the sandbox).
fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", inner.join(","))
        }
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys
                .into_iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("key"),
                        canonical_json(&map[key])
                    )
                })
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        other => other.to_string(),
    }
}
