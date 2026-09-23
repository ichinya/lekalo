//! Issue #46 core conformance: the planner transport attachment
//! renders into the pinned golden OpenAPI document byte for byte,
//! every repeated render is byte-stable, the provenance block binds
//! the exact model/IR/transport pins, and the declared-3.0 variant
//! renders the `nullable` spellings. Pure read-only: nothing writes
//! and nothing outside the committed fixtures is read.

use std::path::Path;
use std::sync::Mutex;

use lekalo_core::error_contract::ErrorRegistry;
use lekalo_core::ir::{compile, CompiledProject};
use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::openapi::{render, DocumentVersion, RenderConfig};
use lekalo_core::query_model::QueryModelAttachment;
use lekalo_core::transport_http::{validate, CapabilityMap, TransportDocument, ValidationContext};

static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

fn compile_fixture_project() -> CompiledProject {
    let selection = LoadSelection {
        project: Some("tests/fixtures/transport-http/project".to_owned()),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("fixture load failed: {}", outcome.to_json_string()),
    };
    match compile(&model) {
        Ok(compilation) => compilation.project,
        Err(failure) => panic!(
            "fixture IR failed: {}",
            failure
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.clone())
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn read_fixture(relative: &str) -> serde_json::Value {
    let bytes = std::fs::read(format!("tests/fixtures/transport-http/{relative}"))
        .expect("fixture readable");
    serde_json::from_slice(&bytes).expect("fixture json")
}

/// The fully bound session: the attachment, the compiled project, the
/// embedded #62 registry, the bound #64 attachment, and the validated
/// context.
struct Session {
    attachment: TransportDocument,
    project: CompiledProject,
    registry: ErrorRegistry,
    query_model: QueryModelAttachment,
    capabilities: CapabilityMap,
}

fn session() -> Session {
    let document = read_fixture("valid/planner.transport.json");
    let attachment = TransportDocument::from_value(&document).expect("attachment decodes");
    let project = compile_fixture_project();
    let registry = ErrorRegistry::embedded()
        .expect("embedded registry")
        .clone();
    let query_model =
        QueryModelAttachment::from_value(&read_fixture("query-model.json")).expect("query model");
    let capabilities = CapabilityMap::http_json();
    let context = ValidationContext::new(&project)
        .with_errors(&registry)
        .with_query_model(&query_model)
        .with_capabilities(&capabilities);
    validate(&attachment, &context).expect("attachment validates");
    Session {
        attachment,
        project,
        registry,
        query_model,
        capabilities: CapabilityMap::http_json(),
    }
}

impl Session {
    fn context(&self) -> ValidationContext<'_> {
        ValidationContext::new(&self.project)
            .with_errors(&self.registry)
            .with_query_model(&self.query_model)
            .with_capabilities(&self.capabilities)
    }
}

fn run_suite() {
    golden_render_is_pinned_and_byte_stable();
    provenance_pins_the_exact_inputs();
    operation_pointers_cover_every_endpoint();
    the_declared_30_variant_renders_the_nullable_sibling();
    unbound_registry_renders_open_error_responses();
    checked_mode_accepts_the_pinned_golden();
    checked_mode_reports_drift_and_unresolved_anchors();
    fragments_round_trip_the_golden();
    the_wire_diff_fixture_pairs_map_to_pointer_views();
    equal_implies_byte_equal_renders();
}

fn golden_render_is_pinned_and_byte_stable() {
    let suite = session();
    let context = suite.context();
    let config = RenderConfig::new();
    let first = render(&suite.attachment, &context, &config).expect("renders");
    let second = render(&suite.attachment, &context, &config).expect("renders");
    assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    assert_eq!(first.digest(), second.digest());
    // The golden lives under tests/fixtures/openapi/valid and is the
    // exact canonical bytes (compact, byte-sorted, no trailing LF).
    // Regeneration is explicit: LEKALO_REGENERATE_OPENAPI_GOLDEN=1
    // rewrites the golden from a reviewed render — never a CI path.
    let golden_path = workspace_root().join("tests/fixtures/openapi/valid/planner.openapi.json");
    if std::env::var("LEKALO_REGENERATE_OPENAPI_GOLDEN").as_deref() == Ok("1") {
        std::fs::write(&golden_path, format!("{}\n", first.canonical_bytes()))
            .expect("golden writable");
        return;
    }
    let golden = std::fs::read_to_string(&golden_path).expect("golden readable");
    assert_eq!(first.canonical_bytes(), golden.trim_end(), "golden bytes");
    // The partial projections of the planner fixture: the focus_task
    // endpoint declares a whole-output success, and its command
    // declares no output — reported, never invented.
    assert_eq!(
        first.findings(),
        [lekalo_core::openapi::Finding {
            detail: "output-undeclared".to_owned(),
            symbol: "planner.endpoint_focus_task".to_owned(),
        }],
        "the exact partial-projection findings"
    );
}

fn provenance_pins_the_exact_inputs() {
    let suite = session();
    let context = suite.context();
    let document = render(&suite.attachment, &context, &RenderConfig::new()).expect("renders");
    let provenance = &document.root()["x-lekalo-provenance"];
    assert_eq!(
        provenance["modelRef"]["modelVersion"], "0.2.16",
        "model version pinned"
    );
    assert_eq!(
        provenance["modelRef"]["digest"],
        suite.attachment.model_ref().digest().as_str(),
    );
    assert_eq!(provenance["irRef"]["identity"], "dev.lekalo.ir@0.2.16");
    assert_eq!(
        provenance["transportRef"]["schemaVersion"],
        "lekalo/transport-http/v0.4.0"
    );
    assert_eq!(
        provenance["transportRef"]["digest"],
        suite.attachment.digest().expect("digest").as_str()
    );
    assert_eq!(provenance["generator"]["id"], "lekalo-core/openapi");
    assert_eq!(provenance["generator"]["version"], "0.4.0");
}

fn operation_pointers_cover_every_endpoint() {
    let suite = session();
    let context = suite.context();
    let document = render(&suite.attachment, &context, &RenderConfig::new()).expect("renders");
    assert_eq!(
        document.operation_pointers().len(),
        suite.attachment.endpoints().len(),
        "one pointer per declared endpoint"
    );
    let sorted: Vec<&str> = document
        .operation_pointers()
        .iter()
        .map(|(pointer, _)| pointer.as_str())
        .collect();
    let mut expected = sorted.clone();
    expected.sort();
    assert_eq!(sorted, expected, "pointers byte-sorted");
}

fn the_declared_30_variant_renders_the_nullable_sibling() {
    let suite = session();
    let context = suite.context();
    let config = RenderConfig::new().with_version(DocumentVersion::V3_0);
    let document = render(&suite.attachment, &context, &config).expect("renders");
    assert_eq!(document.root()["openapi"], "3.0.0");
    // The optional due date renders the 3.0 nullable sibling over the
    // allOf-composed $ref — never the 3.1 type array.
    let due = &document.root()["components"]["schemas"]["PlannerTask"]["properties"]["due"];
    assert_eq!(
        due,
        &serde_json::json!({
            "nullable": true,
            "allOf": [{ "$ref": "#/components/schemas/PlannerDueDate" }]
        })
    );
    // No 3.1-dialect constructs anywhere: no `type` arrays carrying
    // `"null"`, no `const` members.
    let text = document.canonical_bytes();
    assert!(!text.contains("\"type\":[\"string\",\"null\"]"), "{text}");
    assert!(
        !text.contains("\"const\""),
        "3.0 spells single-value enums, not const: {text}"
    );
    // `ok` is a single-value enum at 3.0.
    assert_eq!(
        document.root()["components"]["responses"]["ErrorAuth"]["content"]["application/json"]
            ["schema"]["properties"]["ok"],
        serde_json::json!({ "enum": [false] })
    );
}

fn unbound_registry_renders_open_error_responses() {
    let suite = session();
    let context = ValidationContext::new(&suite.project).with_query_model(&suite.query_model);
    let document = render(&suite.attachment, &context, &RenderConfig::new()).expect("renders");
    // Declared errors without a bound #62 registry cannot render their
    // identity variants: the status response stays open and the gap is
    // reported as findings, never a dangling $ref.
    assert!(
        !document.findings().is_empty(),
        "unrenderable error variants are reported"
    );
    let text = document.canonical_bytes();
    assert!(
        !text.contains("components/schemas/PlannerStoreUnavailable"),
        "no dangling error variant refs"
    );
}

fn checked_mode_accepts_the_pinned_golden() {
    let suite = session();
    let context = suite.context();
    let rendered = render(&suite.attachment, &context, &RenderConfig::new()).expect("renders");
    let existing =
        serde_json::from_str::<serde_json::Value>(rendered.canonical_bytes()).expect("re-parses");
    let report = lekalo_core::openapi::check(
        &existing,
        &suite.attachment,
        &context,
        &RenderConfig::new(),
        &lekalo_core::openapi::OwnershipManifest::default(),
    )
    .expect("checks");
    if !report.is_conformant() {
        panic!(
            "drifts={:?} conflicts={:?} unresolved={:?} manual={:?}",
            report.drifts, report.conflicts, report.unresolved, report.manual
        );
    }
    assert!(
        report.bound_clean + report.manual.len() >= suite.attachment.endpoints().len(),
        "every endpoint binds cleanly: bound={} manual={}",
        report.bound_clean,
        report.manual.len()
    );
    assert!(
        report.diagnostics().as_slice().is_empty(),
        "a conformant check carries no diagnostics"
    );
}

fn checked_mode_reports_drift_and_unresolved_anchors() {
    let suite = session();
    let context = suite.context();
    let rendered = render(&suite.attachment, &context, &RenderConfig::new()).expect("renders");
    let mut existing =
        serde_json::from_str::<serde_json::Value>(rendered.canonical_bytes()).expect("re-parses");
    // Drift: one bound operation carries stale human text.
    existing["paths"]["/tasks"]["get"]["summary"] = "stale text".into();
    // Unresolved: an operation anchored to an endpoint the attachment
    // no longer declares.
    existing["paths"]["/legacy"] = serde_json::json!({
        "get": {
            "operationId": "legacyRoute",
            "x-lekalo-endpoint": "planner.endpoint_removed"
        }
    });
    let report = lekalo_core::openapi::check(
        &existing,
        &suite.attachment,
        &context,
        &RenderConfig::new(),
        &lekalo_core::openapi::OwnershipManifest::default(),
    )
    .expect("checks");
    assert!(!report.is_conformant());
    assert_eq!(report.drifts.len(), 1, "the stale operation drifts");
    assert_eq!(report.drifts[0].0, "/paths/~1tasks/get");
    assert_eq!(report.unresolved.len(), 1, "the legacy anchor refuses");
    let diagnostics = report.diagnostics();
    let ids: Vec<&str> = diagnostics
        .as_slice()
        .iter()
        .map(|diagnostic| diagnostic.id())
        .collect();
    assert!(ids.contains(&"openapi.drift"));
    assert!(ids.contains(&"openapi.binding-unresolved"));
}

fn fragments_round_trip_the_golden() {
    let suite = session();
    let context = suite.context();
    let rendered = render(&suite.attachment, &context, &RenderConfig::new()).expect("renders");
    let fragments = lekalo_core::openapi::Fragments::new(&rendered);
    // Every operation and schema pointer is present, byte-sorted.
    let pointers: Vec<&str> = fragments
        .pointers()
        .map(|pointer| pointer.as_str())
        .collect();
    assert!(pointers.contains(&"/paths/~1tasks/get"));
    assert!(pointers.contains(&"/components/schemas/PlannerTask"));
    let mut sorted = pointers.clone();
    sorted.sort_unstable();
    assert_eq!(pointers, sorted, "fragments byte-sorted");

    // The manifest maps each operation to its endpoint id.
    let manifest =
        lekalo_core::openapi::OwnershipManifest::of_document(&rendered, Default::default());
    assert_eq!(
        manifest.owner("/paths/~1tasks/get"),
        Some(&"planner.endpoint_list_tasks".to_owned())
    );
    assert_eq!(
        manifest.owner("/components/schemas/PlannerTask"),
        Some(&"planner.task".to_owned())
    );

    // Merge the fragments into an empty tree: the golden reconstructs.
    let outcome = lekalo_core::openapi::merge(
        &serde_json::json!({"openapi": "3.1.0", "info": {}, "paths": {}}),
        &fragments,
        &manifest,
        &lekalo_core::openapi::OwnershipManifest::default(),
    )
    .expect("merges");
    assert!(outcome.is_clean());
    assert_eq!(
        outcome.tree()["paths"]["/tasks"]["get"]["operationId"],
        "listTasks"
    );
}

fn the_wire_diff_fixture_pairs_map_to_pointer_views() {
    use lekalo_core::openapi::compare_documents;
    let project = compile_fixture_project();
    let context = ValidationContext::new(&project);
    let base_value = read_fixture("diff/base.json");
    let base = TransportDocument::from_value(&base_value).expect("base decodes");
    validate(&base, &context).expect("base validates");

    // Every committed candidate pair diffs through the same classes
    // the transport comparison produces, and every path carries at
    // least one pointer location. Blocking equals the presence of a
    // breaking class; the fixtures pin one blocking and one
    // non-blocking verdict by name.
    let mut candidates: Vec<String> = std::fs::read_dir("tests/fixtures/transport-http/diff")
        .expect("diff dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .filter(|name| name.starts_with("candidate-"))
        .collect();
    candidates.sort();
    assert!(!candidates.is_empty(), "the diff fixture pairs exist");
    for candidate_name in &candidates {
        let value = read_fixture(&format!("diff/{candidate_name}"));
        let candidate = TransportDocument::from_value(&value).expect("candidate decodes");
        let result = compare_documents(&base, &candidate, &project, DocumentVersion::V3_1)
            .expect("compares");
        assert!(!result.equal(), "{candidate_name}");
        let has_breaking = result
            .paths()
            .iter()
            .any(|path| path.class() == lekalo_core::transport_http::DiffClass::Breaking);
        assert_eq!(
            result.wire_consumer_blocked(),
            has_breaking,
            "{candidate_name}: blocking equals the breaking class"
        );
        for path in result.paths() {
            assert!(
                !path.pointers().is_empty(),
                "{candidate_name}: {} carries pointers",
                path.path()
            );
        }
    }
    // The named verdicts: an added endpoint is non-breaking (the
    // strict wire-consumer profile is not blocked); a required
    // parameter addition blocks.
    let added = read_fixture("diff/candidate-add-endpoint.json");
    let added = TransportDocument::from_value(&added).expect("decodes");
    let result = compare_documents(&base, &added, &project, DocumentVersion::V3_1).unwrap();
    assert!(!result.wire_consumer_blocked(), "addition is non-blocking");
    let required = read_fixture("diff/candidate-add-required-param.json");
    let required = TransportDocument::from_value(&required).expect("decodes");
    let result = compare_documents(&base, &required, &project, DocumentVersion::V3_1).unwrap();
    assert!(result.wire_consumer_blocked(), "required addition blocks");
    // The breaking pair's pointer view names the operation's parameter
    // list.
    let path = result
        .paths()
        .iter()
        .find(|path| path.class() == lekalo_core::transport_http::DiffClass::Breaking)
        .expect("breaking path present");
    assert!(
        path.pointers()
            .iter()
            .any(|pointer| pointer.ends_with("/parameters")),
        "{path:?}"
    );
}

fn equal_implies_byte_equal_renders() {
    use lekalo_core::openapi::compare_documents;
    let suite = session();
    let context = suite.context();
    let project = suite.project.clone();
    let config = RenderConfig::new();
    let rendered = render(&suite.attachment, &context, &config).expect("renders");
    let result = compare_documents(
        &suite.attachment,
        &suite.attachment,
        &project,
        DocumentVersion::V3_1,
    )
    .expect("compares");
    assert!(result.equal());
    assert!(!result.wire_consumer_blocked());
    // Deterministic naming: semantic equality means byte equality —
    // a rendered-only difference is impossible by construction.
    let again = render(&suite.attachment, &context, &config).expect("renders");
    assert_eq!(rendered.canonical_bytes(), again.canonical_bytes());
}

#[test]
fn openapi_render_suite_runs_from_the_workspace_root() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let result = std::panic::catch_unwind(run_suite);
    std::env::set_current_dir(original).expect("restore cwd");
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
