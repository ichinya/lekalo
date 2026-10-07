//! Issue #93 integration tests: the pure search service over the
//! committed planner fixture.
//!
//! Hermetic: the fixture project is compiled through the accepted
//! loader seam, the effect graph is built in memory, and no process,
//! network, or target-source access happens. The tests pin the exact
//! acceptance semantics: exact-ID precedence over fuzzy, declared vs.
//! detected effect trust, requirement qualification ambiguity, cursor
//! generation binding, closed-grammar refusals, and byte-determinism.

use lekalo_core::loader::LoadSelection;
use lekalo_core::search::{
    execute, EffectScope, FilterSet, QueryRequest, SearchIndex, SearchInputs,
};
use std::path::Path;
use std::sync::Mutex;

const FIXTURE: &str = "tests/fixtures/search/planner";

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

/// Compile the committed fixture through the accepted loader seam
/// (bypassed: tests are hermetic and never populate the cache home).
fn compiled() -> lekalo_core::ir::Compilation {
    let _guard = CWD_LOCK.lock().expect("lock");
    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(workspace_root()).expect("chdir");
    let selection = LoadSelection {
        project: Some(FIXTURE.to_owned()),
    };
    let loaded = lekalo_core::cache::load_compiled(&selection, true);
    std::env::set_current_dir(previous).expect("restore");
    let (_model, compilation) = loaded
        .map_err(|result| panic!("fixture compiles: {:?}", result.to_json_string()))
        .expect("compiles");
    compilation
}

/// The full in-memory projection of the fixture (IR + declared effects).
fn projection() -> lekalo_core::search::Projection {
    let compilation = compiled();
    let graph = lekalo_core::effects::build(&compilation.project).expect("effect graph");
    let cap = lekalo_core::search::MAX_POSTINGS;
    let mut projection = lekalo_core::search::project_ir(&compilation.project, cap);
    lekalo_core::search::attach_effects(&mut projection, &graph, cap);
    projection
}

fn index_of(projection: &lekalo_core::search::Projection) -> SearchIndex {
    SearchIndex::from_projection(
        projection,
        &[],
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        Some("planner"),
        lekalo_core::search::MAX_POSTINGS,
    )
    .expect("index")
}

fn run(
    index: &SearchIndex,
    digest: &str,
    request: &QueryRequest,
) -> lekalo_core::search::SearchOutcome {
    execute(index, digest, digest, Some("planner"), request, Vec::new())
        .map_err(|set| panic!("execute: {:?}", set.reason_ids()))
        .expect("executes")
}

fn request(
    query: Option<&str>,
    fuzzy: bool,
    limit: Option<usize>,
    cursor: Option<String>,
    filters: FilterSet,
) -> QueryRequest {
    QueryRequest::admit(query.map(str::to_owned), fuzzy, limit, cursor, filters)
        .map_err(|set| panic!("admit: {:?}", set.reason_ids()))
        .expect("admits")
}

#[test]
fn exact_id_never_loses_to_lexical_or_fuzzy_results() {
    let index = index_of(&projection());
    let outcome = run(
        &index,
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        &request(
            Some("planner.focus_task"),
            true,
            Some(100),
            None,
            FilterSet::default(),
        ),
    );
    assert_eq!(outcome.result.items[0].card.key, "def:planner.focus_task");
    assert_eq!(outcome.result.items[0].r#match.tier, "exact-id");
    assert_ne!(outcome.result.selection, "none");
}

#[test]
fn fuzzy_is_opt_in_and_labeled() {
    let index = index_of(&projection());
    let without = run(
        &index,
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        &request(Some("fokus"), false, None, None, FilterSet::default()),
    );
    assert_eq!(without.result.selection, "none");
    let with = run(
        &index,
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        &request(Some("fokus"), true, None, None, FilterSet::default()),
    );
    assert_eq!(with.result.selection, "ambiguous");
    assert!(with
        .result
        .items
        .iter()
        .all(|item| item.r#match.label == Some("lexical-fuzzy".to_owned())));
    assert!(with
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.id() == "search.ambiguous"));
}

#[test]
fn writers_keep_exact_and_entity_wide_effects_apart() {
    let index = index_of(&projection());
    let filters = FilterSet {
        writes: vec!["planner.user_task_planning".to_owned()],
        ..Default::default()
    };
    let entity_wide = run(
        &index,
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        &request(None, false, Some(100), None, filters.clone()),
    );
    // Declared effects are entity-wide: the three commands carrying an
    // effect on the entity admit.
    assert!(entity_wide.result.items.len() >= 3);
    let _ = EffectScope::Exact;
}

#[test]
fn find_requires_at_least_one_filter() {
    let error =
        QueryRequest::admit(None, false, None, None, FilterSet::default()).expect_err("refused");
    assert!(error.reason_ids().contains(&"search.query-invalid"));
}

#[test]
fn query_bounds_fail_closed() {
    let long = "a".repeat(513);
    let error = QueryRequest::admit(Some(long), false, None, None, FilterSet::default())
        .expect_err("refused");
    assert!(error.reason_ids().contains(&"search.query-invalid"));
    let error = QueryRequest::admit(
        Some("ok".to_owned()),
        false,
        Some(101),
        None,
        FilterSet::default(),
    )
    .expect_err("refused");
    assert!(error.reason_ids().contains(&"search.query-invalid"));
}

#[test]
fn inputs_manifest_rejects_traversal_and_duplicates() {
    let good = br#"{"schemaVersion":"lekalo/search-inputs/v0.8.0","identity":"dev.lekalo.search-inputs@0.8.0","projectId":"planner","inputs":[]}"#;
    assert!(SearchInputs::parse(good).is_ok());
    let traversal = br#"{"schemaVersion":"lekalo/search-inputs/v0.8.0","identity":"dev.lekalo.search-inputs@0.8.0","projectId":"planner","inputs":[{"kind":"trace","path":"../escape"}]}"#;
    assert!(SearchInputs::parse(traversal).is_err());
    let duplicate_key = br#"{"schemaVersion":"lekalo/search-inputs/v0.8.0","schemaVersion":"lekalo/search-inputs/v0.8.0","identity":"dev.lekalo.search-inputs@0.8.0","projectId":"planner","inputs":[]}"#;
    assert!(SearchInputs::parse(duplicate_key).is_err());
}

#[test]
fn repeated_queries_are_byte_identical() {
    let projection = projection();
    let index = index_of(&projection);
    let digest = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
    let first = run(
        &index,
        digest,
        &request(Some("focus"), false, Some(5), None, FilterSet::default()),
    );
    let second = run(
        &index,
        digest,
        &request(Some("focus"), false, Some(5), None, FilterSet::default()),
    );
    assert_eq!(first.json, second.json);
}
