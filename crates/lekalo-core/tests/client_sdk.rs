//! Issue #72 core conformance: the client-SDK projection derives one
//! typed contract from the validated planner transport attachment, the
//! compiled fixture project, the embedded #62 error registry, and the
//! bound #64 query-model attachment; preserves the semantic error
//! identity (id, code, category) exactly; derives retry authorization
//! conservatively; identifies affected SDK artifacts and consumers for
//! changed endpoints (including removals); and reproduces byte-stable
//! canonical bytes. Pure read-only: nothing writes and nothing outside
//! the committed fixtures is read.

use std::path::Path;
use std::sync::Mutex;

use lekalo_core::client_sdk::{
    affected_clients, authorize, project, retry_permitted, AffectedClient, ClientArtifactEntry,
    ClientArtifactIndex, ClientConfig, ConsumerId, Language, RetryAuthorization, ResultShape,
    ScalarMapping, TypeKind, COMPATIBILITY_IDENTITY, IDENTITY, SCHEMA_VERSION,
};
use lekalo_core::error_contract::ErrorRegistry;
use lekalo_core::ir::CompiledProject;
use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::query_model::QueryModelAttachment;
use lekalo_core::scenario::id::SemanticId;
use lekalo_core::transport_http::{CapabilityMap, TransportDocument, ValidationContext};

static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

const FIXTURE_ROOT: &str = "tests/fixtures/transport-http";

fn read_fixture(relative: &str) -> serde_json::Value {
    serde_json::from_str(
        &std::fs::read_to_string(format!("{FIXTURE_ROOT}/{relative}")).expect("fixture readable"),
    )
    .expect("fixture json")
}

fn compile_fixture_project() -> CompiledProject {
    let selection = LoadSelection {
        project: Some(format!("{FIXTURE_ROOT}/project")),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("fixture load failed: {}", outcome.to_json_string()),
    };
    match lekalo_core::ir::compile(&model) {
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

/// One fully-bound projection session: every input the projection
/// requires, owned locally (the embedded registry is `'static`).
struct Session<'a> {
    document: TransportDocument,
    context: ValidationContext<'a>,
}

fn session<'a>(
    project: &'a CompiledProject,
    registry: &'a ErrorRegistry,
    query_model: &'a QueryModelAttachment,
    capabilities: &'a CapabilityMap,
) -> Session<'a> {
    let document =
        TransportDocument::from_value(&read_fixture("valid/planner.transport.json")).expect("ok");
    let context = ValidationContext::new(project)
        .with_errors(registry)
        .with_query_model(query_model)
        .with_capabilities(capabilities);
    Session { document, context }
}

/// The SDK index fixture: one TypeScript and one Go artifact covering
/// the planner endpoints, with declared consumers.
fn sdk_index() -> ClientArtifactIndex {
    let endpoint = |id: &str| SemanticId::parse(id).expect("id");
    ClientArtifactIndex::new(vec![
        ClientArtifactEntry {
            artifact_id: "planner.clients.go".to_owned(),
            path: ".lekalo/generated/clients/go".to_owned(),
            language: Language::Go,
            contract_digest: ZERO_DIGEST.to_owned(),
            endpoints: vec![endpoint("planner.endpoint_focus_task")],
            consumers: vec![ConsumerId::parse("billing").expect("consumer")],
        },
        ClientArtifactEntry {
            artifact_id: "planner.clients.typescript".to_owned(),
            path: ".lekalo/generated/clients/typescript".to_owned(),
            language: Language::Typescript,
            contract_digest: ZERO_DIGEST.to_owned(),
            endpoints: vec![
                endpoint("planner.endpoint_focus_task"),
                endpoint("planner.endpoint_list_tasks"),
            ],
            consumers: vec![
                ConsumerId::parse("billing").expect("consumer"),
                ConsumerId::parse("web_console").expect("consumer"),
            ],
        },
    ])
    .expect("index")
}

const ZERO_DIGEST: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

#[test]
fn client_sdk_suite_runs_from_the_workspace_root() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let result = std::panic::catch_unwind(|| {
        projection_covers_every_endpoint_and_preserves_identity();
        error_variants_preserve_semantic_codes_and_derive_retry();
        type_collection_covers_every_reachable_named_type();
        pagination_helper_and_shape_are_explicit();
        missing_context_refuses_instead_of_projecting();
        canonical_bytes_are_deterministic_and_digested();
        changed_endpoint_impact_names_artifacts_and_consumers();
        removed_endpoint_still_resolves_its_old_consumers();
        unrelated_artifacts_are_excluded_from_impact();
        unsafe_automatic_retries_are_rejected();
    });
    std::env::set_current_dir(original).expect("restore working dir");
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

/// The projection covers every declared endpoint with its exact
/// operation id, method, and path binding.
fn projection_covers_every_endpoint_and_preserves_identity() {
    let compiled = compile_fixture_project();
    let registry = ErrorRegistry::embedded().expect("registry");
    let query_model = QueryModelAttachment::from_value(&read_fixture("query-model.json")).expect("ok");
    let capabilities = CapabilityMap::http_json();
    let session = session(&compiled, registry, &query_model, &capabilities);
    let config = ClientConfig::generated();
    let contract = project(&session.document, &session.context, &config).expect("projection");
    assert_eq!(contract.identity(), IDENTITY);
    assert_eq!(contract.schema_version(), SCHEMA_VERSION);
    assert_eq!(contract.identity(), "dev.lekalo.client-sdk@0.4.0");
    assert_eq!(
        COMPATIBILITY_IDENTITY,
        "dev.lekalo.client-sdk-compatibility@0.4.0"
    );
    assert_eq!(contract.operations().len(), session.document.endpoints().len());
    let focus = contract
        .operation("plannerEndpointFocusTask")
        .expect("focus projected");
    assert_eq!(focus.endpoint.as_str(), "planner.endpoint_focus_task");
    assert_eq!(focus.method, "POST");
    assert_eq!(focus.path, "/tasks/focus");
    assert_eq!(focus.ident.as_str(), "plannerEndpointFocusTask");
    // The idempotency-key binding projects with its requirement.
    let idempotency = focus.idempotency.as_ref().expect("idempotency declared");
    assert_eq!(idempotency.header, "Idempotency-Key");
    assert!(idempotency.required);
    // Correlation headers carry their exact declared names (the
    // transport attachment normalizes them to sorted order).
    let correlation = focus.correlation.as_ref().expect("correlation declared");
    assert_eq!(correlation.headers, vec!["X-Correlation-Id", "X-Request-Id"]);
    // The derived list operation keeps the deterministic operation id.
    let list = contract.operation("listTasks").expect("list projected");
    assert_eq!(list.method, "GET");
    assert_eq!(list.path, "/tasks");
}

/// Every error variant keeps the #62 semantic identity: the exact id,
/// its immutable code, its closed category, and the conservative
/// retry authorization derived from its own contract.
fn error_variants_preserve_semantic_codes_and_derive_retry() {
    let compiled = compile_fixture_project();
    let registry = ErrorRegistry::embedded().expect("registry");
    let query_model = QueryModelAttachment::from_value(&read_fixture("query-model.json")).expect("ok");
    let capabilities = CapabilityMap::http_json();
    let session = session(&compiled, registry, &query_model, &capabilities);
    let contract = project(&session.document, &session.context, &ClientConfig::generated())
        .expect("projection");
    let focus = contract.operation("plannerEndpointFocusTask").expect("focus");
    let by_id = |id: &str| {
        focus
            .errors
            .iter()
            .find(|error| error.error.as_str() == id)
            .unwrap_or_else(|| panic!("{id} projected"))
    };
    let conflict = by_id("planner.focus_conflict");
    assert_eq!(conflict.code, "LEK-ERR-001");
    assert_eq!(conflict.category, "conflict");
    assert_eq!(conflict.status, 409);
    assert_eq!(conflict.retry, RetryAuthorization::ReconciliationOnly);
    // Public payload only: the private focused_by never crosses.
    assert!(conflict.payload.iter().all(|field| field.name != "focused_by"));
    assert!(conflict.payload.iter().any(|field| field.name == "task_id"));

    let denied = by_id("planner.focus_denied");
    assert_eq!(denied.code, "LEK-ERR-002");
    assert_eq!(denied.category, "auth");
    assert_eq!(denied.retry, RetryAuthorization::Never);

    let store = by_id("planner.store_unavailable");
    assert_eq!(store.code, "LEK-ERR-004");
    assert_eq!(store.category, "infrastructure");
    assert_eq!(store.retry, RetryAuthorization::KeyRequired);

    // The union never invents a member the attachment does not map.
    assert_eq!(focus.errors.len(), 5);
}

/// The named-type collection covers the reachable scalars, enums, and
/// entities with their explicit mappings (nullable and required stay
/// distinct; decimal stays an explicit policy, never a default).
fn type_collection_covers_every_reachable_named_type() {
    let compiled = compile_fixture_project();
    let registry = ErrorRegistry::embedded().expect("registry");
    let query_model = QueryModelAttachment::from_value(&read_fixture("query-model.json")).expect("ok");
    let capabilities = CapabilityMap::http_json();
    let session = session(&compiled, registry, &query_model, &capabilities);
    let contract = project(&session.document, &session.context, &ClientConfig::generated())
        .expect("projection");
    let by_id = |id: &str| {
        contract
            .types()
            .iter()
            .find(|kind| kind.symbol.as_str() == id)
            .unwrap_or_else(|| panic!("{id} collected"))
    };
    let task = by_id("planner.task");
    match &task.kind {
        TypeKind::Object(fields) => {
            let due = fields.iter().find(|field| field.name == "due").expect("due");
            assert!(due.nullable, "Optional(due_date) is nullable");
            assert!(!due.required, "due is not required");
            let title = fields.iter().find(|field| field.name == "title").expect("title");
            assert!(!title.nullable);
            assert!(title.required);
        }
        other => panic!("planner.task projects an object, got {other:?}"),
    }
    let state = by_id("planner.task_state");
    match &state.kind {
        TypeKind::Enum(values) => {
            assert_eq!(
                values,
                &vec!["backlog".to_owned(), "focused".to_owned(), "done".to_owned()]
            );
        }
        other => panic!("planner.task_state projects an enum, got {other:?}"),
    }
    match &by_id("planner.task_id").kind {
        TypeKind::Scalar(ScalarMapping::Uuid) => {}
        other => panic!("task_id maps uuid, got {other:?}"),
    }
    // Types sort by semantic id, and every type id is its full
    // semantic id: cross-module references stay unambiguous.
    let ids: Vec<&str> = contract.types().iter().map(|kind| kind.symbol.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "type order is canonical");
    // Decimal mapping is explicit configuration over a declared
    // scalar; the default projection never maps text to decimal.
    let mut decimal_config = ClientConfig::generated();
    decimal_config.decimal_scalars.push("planner.text".to_owned());
    let decimal = project(&session.document, &session.context, &decimal_config).expect("decimal");
    match &decimal
        .types()
        .iter()
        .find(|kind| kind.symbol.as_str() == "planner.text")
        .expect("text")
        .kind
    {
        TypeKind::Scalar(ScalarMapping::DecimalString) => {}
        other => panic!("configured decimal maps decimal-string, got {other:?}"),
    }
    // An unknown decimal scalar refuses instead of mapping nothing.
    let mut unknown = ClientConfig::generated();
    unknown.decimal_scalars.push("planner.not_a_scalar".to_owned());
    assert!(project(&session.document, &session.context, &unknown).is_err());
}

/// Pagination projects as a bounded helper only when the endpoint
/// declares it, and a cursor binding carries its declared field;
/// plain queries project the value shape.
fn pagination_helper_and_shape_are_explicit() {
    let compiled = compile_fixture_project();
    let registry = ErrorRegistry::embedded().expect("registry");
    let query_model = QueryModelAttachment::from_value(&read_fixture("query-model.json")).expect("ok");
    let capabilities = CapabilityMap::http_json();
    let session = session(&compiled, registry, &query_model, &capabilities);
    let contract = project(&session.document, &session.context, &ClientConfig::generated())
        .expect("projection");
    let paged = contract
        .operation("plannerEndpointTasksByProject")
        .expect("paged operation");
    let pagination = paged.pagination.as_ref().expect("cursor pagination");
    assert_eq!(pagination.style, "cursor");
    assert_eq!(pagination.limit_param, "limit");
    assert!(pagination.cursor_param.is_some());
    assert!(pagination.cursor_field.is_some(), "termination stays explicit");
    assert_eq!(paged.result_shape, ResultShape::Page);
    // The focused-count query projects a plain value shape with no
    // pagination helper.
    let count = contract.operation("plannerEndpointCountFocused").expect("count");
    assert!(count.pagination.is_none());
    assert_eq!(count.result_shape, ResultShape::Value);
}

/// The projection refuses to run on partial evidence: a context
/// without the error registry or without the query-model attachment
/// is a refusal, never a silently narrowed SDK.
fn missing_context_refuses_instead_of_projecting() {
    let compiled = compile_fixture_project();
    let registry = ErrorRegistry::embedded().expect("registry");
    let query_model_value = read_fixture("query-model.json");
    let query_model = QueryModelAttachment::from_value(&query_model_value).expect("ok");
    let capabilities = CapabilityMap::http_json();
    let document =
        TransportDocument::from_value(&read_fixture("valid/planner.transport.json")).expect("ok");
    let config = ClientConfig::generated();
    // No registry bound.
    let no_errors = ValidationContext::new(&compiled)
        .with_query_model(&query_model)
        .with_capabilities(&capabilities);
    assert!(project(&document, &no_errors, &config).is_err());
    // No query-model bound.
    let no_query = ValidationContext::new(&compiled)
        .with_errors(registry)
        .with_capabilities(&capabilities);
    assert!(project(&document, &no_query, &config).is_err());
    // An unknown generation mode refuses.
    let full = ValidationContext::new(&compiled)
        .with_errors(registry)
        .with_query_model(&query_model)
        .with_capabilities(&capabilities);
    let mut bad_mode = ClientConfig::generated();
    bad_mode.mode = "yolo".to_owned();
    assert!(project(&document, &full, &bad_mode).is_err());
}

/// Canonical bytes are compact, byte-stable across repeats, and their
/// digest is the sha256 over the exact bytes.
fn canonical_bytes_are_deterministic_and_digested() {
    let compiled = compile_fixture_project();
    let registry = ErrorRegistry::embedded().expect("registry");
    let query_model = QueryModelAttachment::from_value(&read_fixture("query-model.json")).expect("ok");
    let capabilities = CapabilityMap::http_json();
    let session = session(&compiled, registry, &query_model, &capabilities);
    let first = project(&session.document, &session.context, &ClientConfig::generated())
        .expect("projection")
        .canonical_bytes()
        .expect("bytes");
    for _ in 0..3 {
        let again = project(&session.document, &session.context, &ClientConfig::generated())
            .expect("projection")
            .canonical_bytes()
            .expect("bytes");
        assert_eq!(first, again, "generation is deterministic");
    }
    assert!(!first.contains('\n'), "compact form");
    // The key order is byte-sorted: re-parsing and re-serializing
    // through serde_json's canonical object form is identical.
    let reparsed: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert_eq!(first, reparsed.to_string());
    let digest = project(&session.document, &session.context, &ClientConfig::generated())
        .expect("projection")
        .digest()
        .expect("digest");
    assert_eq!(
        digest.as_str(),
        format!("sha256:{}", lekalo_core::digest::sha256_hex(first.as_bytes()))
    );
}

/// A changed endpoint identifies exactly the affected SDK artifacts
/// and their declared consumers.
fn changed_endpoint_impact_names_artifacts_and_consumers() {
    let before = sdk_index();
    let base = TransportDocument::from_value(&read_fixture("diff/base.json")).expect("base");
    let candidate =
        TransportDocument::from_value(&read_fixture("diff/candidate-remove-error.json"))
            .expect("candidate");
    let (affected, complete) = affected_clients(&base, &candidate, &before).expect("impact");
    assert!(complete);
    // The removed error entry changes the focus endpoint's typed
    // union: exactly the artifacts covering the focus endpoint are
    // affected.
    let focus_hits: Vec<&AffectedClient> = affected
        .iter()
        .filter(|finding| {
            finding.endpoint.as_ref().map(|endpoint| endpoint.as_str())
                == Some("planner.endpoint_focus_task")
        })
        .collect();
    assert!(
        !focus_hits.is_empty(),
        "the focus endpoint change affects its covering artifacts"
    );
    assert!(
        focus_hits
            .iter()
            .all(|finding| finding.artifact_id == "planner.clients.typescript"
                || finding.artifact_id == "planner.clients.go"),
        "only registered artifacts are named"
    );
    let ts_hit = focus_hits
        .iter()
        .find(|finding| finding.artifact_id == "planner.clients.typescript")
        .expect("ts artifact affected");
    assert!(
        ts_hit.consumers.contains(&"web_console".to_owned()),
        "the web console consumer of the TS artifact is named"
    );
    // Equality: an unchanged pair affects nothing.
    let (none, complete) = affected_clients(&base, &base, &before).expect("equal");
    assert!(none.is_empty());
    assert!(complete);
}

/// A removed endpoint still resolves its old consumers through the
/// before index: the artifact that covered it is named, even though
/// the candidate no longer declares the endpoint.
fn removed_endpoint_still_resolves_its_old_consumers() {
    let before = sdk_index();
    let base = TransportDocument::from_value(&read_fixture("diff/base.json")).expect("base");
    let candidate =
        TransportDocument::from_value(&read_fixture("diff/candidate-remove-endpoint.json"))
            .expect("candidate");
    assert!(
        candidate.endpoint("planner.endpoint_list_tasks").is_none(),
        "the fixture removes the listing endpoint"
    );
    let (affected, _) = affected_clients(&base, &candidate, &before).expect("impact");
    let hits: Vec<&AffectedClient> = affected
        .iter()
        .filter(|finding| {
            finding.endpoint.as_ref().map(|endpoint| endpoint.as_str())
                == Some("planner.endpoint_list_tasks")
                && finding.artifact_id == "planner.clients.typescript"
        })
        .collect();
    assert!(
        !hits.is_empty(),
        "removal must still find the old consumers"
    );
    assert!(
        hits.iter()
            .all(|finding| finding.consumers.contains(&"web_console".to_owned())),
        "the web console consumer of the listing endpoint is named"
    );
}

/// Unrelated endpoints (no artifact covers them) produce no affected
/// clients for those artifacts; the impact never over-reports. An
/// empty before-index is incompleteness, never a clean pass.
fn unrelated_artifacts_are_excluded_from_impact() {
    let before = sdk_index();
    let base = TransportDocument::from_value(&read_fixture("diff/base.json")).expect("base");
    let candidate =
        TransportDocument::from_value(&read_fixture("diff/candidate-remove-endpoint.json"))
            .expect("candidate");
    let (affected, _) = affected_clients(&base, &candidate, &before).expect("impact");
    assert!(
        affected
            .iter()
            .filter(|finding| {
                finding.endpoint.as_ref().map(|endpoint| endpoint.as_str())
                    == Some("planner.endpoint_list_tasks")
            })
            .all(|finding| finding.artifact_id != "planner.clients.go"),
        "uncovered artifacts are excluded"
    );
    let (empty, complete) =
        affected_clients(&base, &candidate, &ClientArtifactIndex::empty()).expect("impact");
    assert!(empty.is_empty());
    assert!(!complete, "no inventory is incompleteness, not cleanliness");
}

/// Unsafe automatic retries are rejected: a positive budget alone
/// never authorizes a retry; only a declared error's own contract
/// does; unknown failures make one attempt; over-bound budgets
/// refuse instead of clamping.
fn unsafe_automatic_retries_are_rejected() {
    use lekalo_core::client_sdk::plan_attempts;
    use lekalo_core::error_contract::types::{EffectClass, Idempotency, RetryCondition, RetryPolicy};
    // A write without a guaranteed idempotency contract never
    // retries, even under `safe`.
    assert_eq!(
        authorize(RetryPolicy::Safe, Idempotency::NotGuaranteed, EffectClass::Write),
        RetryAuthorization::Never
    );
    // Reconciliation is a caller duty, never an automatic retry.
    assert_eq!(
        authorize(
            RetryPolicy::Conditional(RetryCondition::Reconciliation),
            Idempotency::KeyRequired,
            EffectClass::Write
        ),
        RetryAuthorization::ReconciliationOnly
    );
    // A requested budget of zero refuses: retries must come from the
    // declared contract, never from a caller's optimism.
    let safe = [RetryAuthorization::Safe];
    assert!(plan_attempts(&safe, 0).is_err());
    // Over-bound budgets refuse, never clamp silently.
    assert!(plan_attempts(&safe, lekalo_core::client_sdk::MAX_ATTEMPTS).is_err());
    // Unknown failures (no declared authorization) make one attempt.
    let plan = plan_attempts(&safe, 1).expect("plan");
    assert!(!retry_permitted(None, Some("key"), 1, plan));
}
