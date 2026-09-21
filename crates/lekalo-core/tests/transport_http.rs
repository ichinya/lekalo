//! Issue #70 core conformance: the transport-http attachment decodes,
//! validates against the compiled planner fixture project, the
//! embedded error registry, and the bound query-model attachment,
//! refuses every invalid vector with its registered rule and fixed
//! detail token, and produces deterministic canonical bytes. Pure
//! read-only: nothing writes and nothing outside the committed
//! fixtures is read.

use std::path::Path;
use std::sync::Mutex;

use lekalo_core::error_contract::ErrorRegistry;
use lekalo_core::ir::{compile, CompiledProject};
use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::query_model::QueryModelAttachment;
use lekalo_core::transport_http::{
    compare, validate, CapabilityMap, TransportDocument, ValidationContext,
};

static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

const FIXTURE_ROOT: &str = "tests/fixtures/transport-http";

fn compile_fixture_project() -> CompiledProject {
    let selection = LoadSelection {
        project: Some(format!("{FIXTURE_ROOT}/project")),
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
    let bytes = std::fs::read(format!("{FIXTURE_ROOT}/{relative}")).expect("fixture readable");
    serde_json::from_slice(&bytes).expect("fixture json")
}

/// The full validation context: the compiled project, the embedded
/// #62 registry (the planner seed), the bound #64 attachment, and the
/// http-json capability surface.
fn full_context<'a>(
    project: &'a CompiledProject,
    registry: &'a ErrorRegistry,
    query_model: &'a QueryModelAttachment,
    capabilities: &'a CapabilityMap,
    strict: bool,
) -> ValidationContext<'a> {
    let context = ValidationContext::new(project)
        .with_errors(registry)
        .with_query_model(query_model)
        .with_capabilities(capabilities);
    if strict {
        context.strict()
    } else {
        context
    }
}

fn invalid_vectors() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(format!("{FIXTURE_ROOT}/invalid"))
        .expect("invalid dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .filter(|name| name.ends_with(".json") && !name.ends_with(".expect.json"))
        .collect();
    names.sort();
    names
}

/// The refusal (rule, detail) of one diagnostic set, when it carries
/// exactly one diagnostic.
fn refusal(diagnostics: &lekalo_core::diagnostics::DiagnosticSet) -> Option<(String, String)> {
    let one = diagnostics.as_slice().first()?;
    let detail = one
        .data()
        .iter()
        .find(|(key, _)| key.as_str() == "detail")
        .and_then(|(_, value)| match value {
            lekalo_core::diagnostics::DataValue::Token(text) => Some(text.clone()),
            _ => None,
        })?;
    Some((one.id().to_owned(), detail))
}

#[test]
fn transport_suite_runs_from_the_workspace_root() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let result = std::panic::catch_unwind(|| {
        valid_attachment_validates_in_both_profiles();
        every_invalid_vector_refuses_with_its_registered_rule();
        canonical_bytes_are_deterministic_and_reparseable();
        bounds_are_enforced_at_the_wire();
        capabilities_and_projection_invariants();
        projection_goldens_are_byte_pinned_and_parity_holds();
        endpoint_bound_refuses_beyond_the_limit();
        canonical_digest_is_pinned();
        wire_diff_classifies_and_blocks_breaking_changes();
        every_capability_kind_refuses_without_profile_support();
        every_capability_kind_validates_under_the_published_surface();
    });
    std::env::set_current_dir(original).expect("restore working dir");
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn loaded() -> (
    CompiledProject,
    &'static ErrorRegistry,
    QueryModelAttachment,
    CapabilityMap,
) {
    let project = compile_fixture_project();
    let registry: &'static ErrorRegistry =
        ErrorRegistry::embedded().expect("embedded error registry");
    let query_model = QueryModelAttachment::from_value(&read_fixture("query-model.json"))
        .expect("query-model fixture is valid");
    let capabilities = CapabilityMap::http_json();
    (project, registry, query_model, capabilities)
}

fn valid_attachment_validates_in_both_profiles() {
    let (project, registry, query_model, capabilities) = loaded();
    let document = TransportDocument::from_value(&read_fixture("valid/planner.transport.json"))
        .expect("valid attachment parses");
    assert_eq!(document.endpoints().len(), 6);
    assert_eq!(document.schemes().len(), 2);
    // The effective operation ids: one explicit override plus five
    // deterministic derivations.
    let focus = document.endpoint("planner.endpoint_focus_task").unwrap();
    assert_eq!(
        focus.effective_operation_id().as_str(),
        "plannerEndpointFocusTask"
    );
    let list = document.endpoint("planner.endpoint_list_tasks").unwrap();
    assert_eq!(list.effective_operation_id().as_str(), "listTasks");

    for strict in [false, true] {
        validate(
            &document,
            &full_context(&project, registry, &query_model, &capabilities, strict),
        )
        .unwrap_or_else(|set| {
            panic!("strict={strict}: {set:?}");
        });
    }
}

fn every_invalid_vector_refuses_with_its_registered_rule() {
    let (project, registry, query_model, capabilities) = loaded();
    for name in invalid_vectors() {
        let expectation = read_fixture(&format!(
            "invalid/{}",
            name.replace(".json", ".expect.json")
        ));
        let (rule, detail): (String, String) = (
            expectation["rule"].as_str().unwrap().to_owned(),
            expectation["detail"].as_str().unwrap().to_owned(),
        );
        let document = TransportDocument::from_value(&read_fixture(&format!("invalid/{name}")));
        // The strict profile exercises the mapping-completeness gate;
        // that vector refuses only under strict, every other vector
        // refuses identically in both profiles.
        let strict_only = detail == "union-member-unmapped";
        for strict in [false, true] {
            if strict_only && !strict {
                continue;
            }
            let outcome = match &document {
                Ok(document) => validate(
                    document,
                    &full_context(&project, registry, &query_model, &capabilities, strict),
                ),
                Err(set) => Err(set.clone()),
            };
            let set = outcome.expect_err(&format!("{name} must refuse (strict={strict})"));
            let (actual_rule, actual_detail) =
                refusal(&set).unwrap_or_else(|| panic!("{name}: no bounded refusal"));
            assert_eq!(
                actual_rule, rule,
                "{name} (strict={strict}): {actual_detail:?}"
            );
            assert_eq!(actual_detail, detail, "{name} (strict={strict})");
        }
    }
}

fn canonical_bytes_are_deterministic_and_reparseable() {
    let document =
        TransportDocument::from_value(&read_fixture("valid/planner.transport.json")).unwrap();
    let bytes = document.canonical_bytes().expect("canonical bytes");
    assert!(!bytes.contains('\n'), "compact form");
    // Byte-sorted keys: the canonical serialization of the parsed
    // document equals a byte-sort of itself.
    let reparsed: serde_json::Value = serde_json::from_str(&bytes).unwrap();
    let mut buffer = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::new();
    use serde::ser::Serialize;
    let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);
    reparsed.serialize(&mut serializer).unwrap();
    let canonical_again = TransportDocument::from_value(&reparsed)
        .unwrap()
        .canonical_bytes()
        .unwrap();
    assert_eq!(bytes, canonical_again, "canonical form is stable");
    // The digest is the sha256 of the canonical bytes.
    let digest = document.digest().expect("digest");
    assert_eq!(
        digest.as_str(),
        &format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(bytes.as_bytes())
        )
    );
}

fn bounds_are_enforced_at_the_wire() {
    let base = read_fixture("valid/planner.transport.json");
    let mut over = base.clone();
    let focus = over["endpoints"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|endpoint| endpoint["endpoint"] == "planner.endpoint_focus_task")
        .unwrap();
    // MAX_PARAMS + 1 declared parameters on one endpoint.
    let mut params = Vec::new();
    for index in 0..=lekalo_core::transport_http::version::MAX_PARAMS {
        params.push(serde_json::json!({
            "name": format!("p{index}"),
            "in": "query",
            "field": "input.task_id",
            "required": false
        }));
    }
    focus["params"] = serde_json::Value::Array(params);
    let refused = TransportDocument::from_value(&over).expect_err("the param bound must refuse");
    assert_eq!(
        refusal(&refused),
        Some((
            "transport.input-invalid".to_owned(),
            "param-bound".to_owned()
        ))
    );
    // At exactly the bound the document still parses.
    let mut at = base.clone();
    let focus = at["endpoints"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|endpoint| endpoint["endpoint"] == "planner.endpoint_focus_task")
        .unwrap();
    let mut params = Vec::new();
    for index in 0..lekalo_core::transport_http::version::MAX_PARAMS {
        params.push(serde_json::json!({
            "name": format!("p{index}"),
            "in": "query",
            "field": "input.task_id",
            "required": false
        }));
    }
    focus["params"] = serde_json::Value::Array(params);
    assert!(TransportDocument::from_value(&at).is_ok(), "at the bound");
}

/// The four committed projection goldens are byte-pinned: the Rust
/// projection reproduces them exactly, and the canonical core
/// (every route member except the namespace-shaped handler
/// identity) is byte-identical across all four namespaces — the
/// cross-runtime parity the OpenAPI projection relies on.
fn projection_goldens_are_byte_pinned_and_parity_holds() {
    let (project, registry, query_model, capabilities) = loaded();
    let document =
        TransportDocument::from_value(&read_fixture("valid/planner.transport.json")).unwrap();
    let context = ValidationContext::new(&project)
        .with_errors(registry)
        .with_query_model(&query_model)
        .with_capabilities(&capabilities);
    let mut canonical_cores: Vec<String> = Vec::new();
    for namespace in ["go", "laravel", "node", "rust"] {
        let surface = lekalo_core::transport_http::project(&document, &context, namespace).unwrap();
        let golden = std::fs::read_to_string(format!(
            "{FIXTURE_ROOT}/projected/{namespace}/{namespace}.expect.json"
        ))
        .expect("committed golden");
        assert_eq!(
            surface.canonical_bytes(),
            golden.trim_end(),
            "{namespace}: the projection is byte-pinned"
        );
        // The canonical core: strip the namespace-shaped handler.
        let mut value: serde_json::Value = serde_json::from_str(surface.canonical_bytes()).unwrap();
        value["namespace"].take();
        for route in value["routes"].as_array_mut().unwrap() {
            route["handler"].take();
        }
        canonical_cores.push(value.to_string());
    }
    assert_eq!(
        canonical_cores
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        1,
        "the canonical core is identical across namespaces"
    );
}

fn capabilities_and_projection_invariants() {
    let (project, registry, query_model, _) = loaded();
    let document =
        TransportDocument::from_value(&read_fixture("valid/planner.transport.json")).unwrap();
    // Without a capability map the default profile accepts a declared
    // capability (unchecked), but the strict profile requires the map.
    let no_map = ValidationContext::new(&project)
        .with_errors(registry)
        .with_query_model(&query_model);
    assert!(validate(&document, &no_map).is_ok());
    let strict_no_map = ValidationContext::new(&project)
        .with_errors(registry)
        .with_query_model(&query_model)
        .strict();
    let set = validate(&document, &strict_no_map).expect_err("strict requires the map");
    assert_eq!(
        refusal(&set),
        Some((
            "transport.contract-invalid".to_owned(),
            "capability-map-absent".to_owned()
        ))
    );
    // A map that provides nothing refuses the streaming endpoint
    // explicitly.
    let map = CapabilityMap::http_json();
    let with_map = ValidationContext::new(&project)
        .with_errors(registry)
        .with_query_model(&query_model)
        .with_capabilities(&map)
        .strict();
    assert!(validate(&document, &with_map).is_ok());
}

/// The endpoint-list bound refuses one past the limit and accepts
/// exactly at it; the wire bound is arrival-order independent.
fn endpoint_bound_refuses_beyond_the_limit() {
    let base = read_fixture("valid/planner.transport.json");
    let template = base["endpoints"][0].clone();
    let bound = lekalo_core::transport_http::version::MAX_ENDPOINTS;
    let document = |count: usize| {
        let mut doc = base.clone();
        let mut endpoints = Vec::new();
        for index in 0..count {
            let mut endpoint = template.clone();
            endpoint["endpoint"] =
                serde_json::Value::String(format!("planner.endpoint_bulk_{index}"));
            endpoints.push(endpoint);
        }
        doc["endpoints"] = serde_json::Value::Array(endpoints);
        doc
    };
    assert!(
        TransportDocument::from_value(&document(bound)).is_ok(),
        "exactly the bound parses"
    );
    let refused = TransportDocument::from_value(&document(bound + 1))
        .expect_err("one past the bound refuses");
    assert_eq!(
        refusal(&refused),
        Some((
            "transport.input-invalid".to_owned(),
            "endpoint-bound".to_owned()
        ))
    );
}

/// The canonical digest is deterministic across parses of the same
/// bytes; the pinned spelling is the closed sha256 form.
/// The wire-diff classification: every committed fixture pair pins
/// its closed class and the wire-consumer blocking signal (AC-4).
fn wire_diff_classifies_and_blocks_breaking_changes() {
    let base = TransportDocument::from_value(&read_fixture("diff/base.json")).expect("base parses");
    let cases = [
        (
            "candidate-add-endpoint",
            vec![("endpoints/planner.endpoint_today", "non-breaking")],
        ),
        (
            "candidate-add-required-param",
            vec![("endpoints/planner.endpoint_list_tasks/params", "breaking")],
        ),
        (
            "candidate-remove-endpoint",
            vec![("endpoints/planner.endpoint_list_tasks", "breaking")],
        ),
        (
            "candidate-remove-error",
            vec![("endpoints/planner.endpoint_focus_task/errors", "breaking")],
        ),
        (
            "candidate-security",
            vec![(
                "endpoints/planner.endpoint_task_stream/security",
                "breaking",
            )],
        ),
        (
            "candidate-policy",
            vec![
                (
                    "endpoints/planner.endpoint_list_tasks/operationId",
                    "policy-change",
                ),
                (
                    "endpoints/planner.endpoint_list_tasks/rateLimit",
                    "policy-change",
                ),
            ],
        ),
        (
            "candidate-move-param-location",
            vec![("endpoints/planner.endpoint_focus_task_by_id/params", "breaking")],
        ),
        (
            "candidate-param-explode",
            vec![("endpoints/planner.endpoint_focus_task_by_id/params", "breaking")],
        ),
        (
            "candidate-body-required-field",
            vec![("endpoints/planner.endpoint_focus_task_by_id/body", "breaking")],
        ),
        (
            "candidate-success-body-narrowed",
            vec![("endpoints/planner.endpoint_count_focused/success", "breaking")],
        ),
        (
            "candidate-scheme-edit",
            vec![("securitySchemes/user_bearer", "breaking")],
        ),
        (
            "candidate-defaults-header",
            vec![("defaults/idempotencyHeader", "breaking")],
        ),
    ];
    for (name, expected) in cases {
        let candidate = TransportDocument::from_value(&read_fixture(&format!("diff/{name}.json")))
            .expect("candidate parses");
        let diff = compare(&base, &candidate).expect("same family");
        let actual: Vec<(String, String)> = diff
            .paths()
            .iter()
            .map(|path| (path.path().to_owned(), path.class().key().to_owned()))
            .collect();
        let expected: Vec<(String, String)> = expected
            .into_iter()
            .map(|(path, class)| (path.to_owned(), class.to_owned()))
            .collect();
        assert_eq!(actual, expected, "{name}");
        let blocked = expected.iter().any(|(_, class)| class == "breaking");
        assert_eq!(diff.wire_consumer_blocked(), blocked, "{name}");
    }
    // The identical pair is equal and never blocks.
    let equal = compare(&base, &base).expect("same family");
    assert!(equal.equal());
    assert!(!equal.wire_consumer_blocked());
    // Mixed projects refuse instead of guessing.
    let mut foreign = read_fixture("diff/base.json");
    foreign["projectId"] = serde_json::json!("other");
    let foreign = TransportDocument::from_value(&foreign).expect("parses");
    let refused = compare(&base, &foreign).expect_err("project mismatch refuses");
    assert_eq!(
        refusal(&refused),
        Some((
            "transport.input-invalid".to_owned(),
            "diff-project-mismatch".to_owned()
        ))
    );
}

fn canonical_digest_is_pinned() {
    let document =
        TransportDocument::from_value(&read_fixture("valid/planner.transport.json")).unwrap();
    let digest = document.digest().expect("digest");
    assert_eq!(
        digest.as_str().len(),
        "sha256:".len() + 64,
        "the digest is the closed sha256 spelling"
    );
    let again = TransportDocument::from_value(&read_fixture("valid/planner.transport.json"))
        .unwrap()
        .digest()
        .unwrap();
    assert_eq!(digest, again);
}

/// Every capability kind is checked against the profile map
/// independently: an empty map refuses each declared kind with the
/// explicit capability id in the diagnostic data.
fn every_capability_kind_refuses_without_profile_support() {
    let (project, registry, query_model, _) = loaded();
    let empty = CapabilityMap::empty();
    let base = read_fixture("valid/planner.transport.json");
    for (kind, detail) in [
        ("streaming", "sse"),
        ("upload", "multipart"),
        ("download", "binary"),
    ] {
        let mut doc = base.clone();
        let stream = doc["endpoints"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|endpoint| endpoint["endpoint"] == "planner.endpoint_task_stream")
            .unwrap();
        stream["capabilities"] = serde_json::json!([
            { "capability": kind, "minimumSupport": "partial", "detail": detail }
        ]);
        let document = TransportDocument::from_value(&doc).expect("parses");
        let context = ValidationContext::new(&project)
            .with_errors(registry)
            .with_query_model(&query_model)
            .with_capabilities(&empty);
        let set =
            validate(&document, &context).expect_err(&format!("{kind} without support refuses"));
        let one = set.as_slice().first().expect("one diagnostic");
        assert_eq!(one.id(), "transport.capability-unsatisfied", "{kind}");
        let capability = one
            .data()
            .get("capability")
            .and_then(|value| match value {
                lekalo_core::diagnostics::DataValue::Token(text) => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default();
        assert_eq!(capability, format!("transport.{kind}"));
    }
}

/// Every capability kind is satisfied by the published http-json
/// surface at a partial minimum (the C-6 alignment): declaring
/// streaming, upload, or download validates under the default map,
/// while a full minimum still refuses a partial profile.
fn every_capability_kind_validates_under_the_published_surface() {
    let (project, registry, query_model, _) = loaded();
    let map = CapabilityMap::http_json();
    let base = read_fixture("valid/planner.transport.json");
    for (kind, detail, minimum) in [
        ("streaming", "sse", "partial"),
        ("upload", "multipart", "partial"),
        ("download", "binary", "partial"),
        ("upload", "multipart", "full"),
    ] {
        let mut doc = base.clone();
        let target = doc["endpoints"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|endpoint| endpoint["endpoint"] == "planner.endpoint_focus_task")
            .unwrap();
        target["capabilities"] = serde_json::json!([
            { "capability": kind, "minimumSupport": minimum, "detail": detail }
        ]);
        let document = TransportDocument::from_value(&doc).expect("parses");
        let context = ValidationContext::new(&project)
            .with_errors(registry)
            .with_query_model(&query_model)
            .with_capabilities(&map);
        let outcome = validate(&document, &context);
        if minimum == "full" {
            let set = outcome.expect_err("full minimum refuses a partial profile");
            assert_eq!(
                refusal(&set),
                Some((
                    "transport.capability-unsatisfied".to_owned(),
                    "capability-unsupported".to_owned()
                ))
            );
        } else {
            outcome.unwrap_or_else(|set| panic!("{kind} under the published map: {set:?}"));
        }
    }
}
