//! Issue #20 core cache tests: store integrity and recovery, warm/cold
//! byte-equivalence against the published loader path, module locality,
//! and the read-only status projection.
//!
//! Every temporary project lives under the crate's `target/` tree with a
//! cwd-relative selector, so no Windows 8.3 alias spelling can enter the
//! selection grammar.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::cache::path::CacheHome;
use crate::cache::pipeline::Session;
use crate::cache::sqlite::SqliteStore;
use crate::loader::LoadSelection;

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

fn temp_case(tag: &str) -> PathBuf {
    let id = NEXT_CASE.fetch_add(1, Ordering::Relaxed) + u64::from(std::process::id());
    let relative = PathBuf::from("target")
        .join("cache-tests")
        .join(format!("{tag}-{id}"));
    let absolute = std::env::current_dir()
        .expect("the test process has a working directory")
        .join(&relative);
    let _ = std::fs::remove_dir_all(&absolute);
    std::fs::create_dir_all(&absolute).expect("temp case dir");
    absolute
}

fn copy_dir(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read source") {
        let entry = entry.expect("entry");
        let target_path = target.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target_path);
        } else {
            std::fs::copy(entry.path(), target_path).expect("copy file");
        }
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/loader")
        .join(name)
}

fn selection_for(absolute: &Path) -> LoadSelection {
    let cwd = std::env::current_dir().expect("cwd");
    let relative = absolute
        .strip_prefix(&cwd)
        .expect("temp case lives under the crate")
        .to_string_lossy()
        .replace('\\', "/");
    LoadSelection {
        project: Some(relative),
    }
}

const THREE_MODULES: &str = "valid-direct-visibility";

fn cached_model_bytes(selection: &LoadSelection, bypass: bool, spans: bool) -> String {
    let session = Session::open(selection, bypass).expect("session opens");
    let model = session.load_model(selection).expect("loads");
    crate::loader::render_model_envelope(&model, spans).to_json_string()
}

#[test]
fn store_round_trips_a_summary_record() {
    let root = temp_case("store");
    let home = CacheHome::resolve(&root).expect("home");
    let path = home.database_path().expect("db path");
    let store = SqliteStore::open_create(&path).expect("store opens");
    let key = crate::cache::key::Key::Source {
        path: "lekalo/project.yaml".to_owned(),
        raw_digest: crate::cache::canonical::sha256_digest(b"bytes"),
        normalized_digest: crate::cache::canonical::sha256_digest(b"normalized"),
    };
    let record = crate::cache::record::Record::new(
        crate::cache::key::RecordKind::Source,
        key,
        crate::cache::record::Binding::pipeline(),
        crate::cache::record::PayloadValue::new(
            crate::cache::canonical::sha256_digest(b"normalized"),
            Vec::new(),
        ),
    )
    .sealed();
    let digest = record.key.digest();
    assert!(store.put(&digest, &record, None).expect("put"));
    assert!(!store.put(&digest, &record, None).expect("put again"));
    let entry = store.get(&digest).expect("get").expect("present");
    assert_eq!(entry.record_kind, "source");
    assert_eq!(
        crate::cache::canonical::sha256_digest(&entry.payload),
        entry.payload_digest
    );
    assert_eq!(
        store.record_counts().expect("counts"),
        vec![(crate::cache::key::RecordKind::Source, 1)]
    );
    assert_eq!(store.dependency_edge_count().expect("edges"), 0);
}

#[test]
fn garbage_database_is_corrupt_never_a_hit() {
    let root = temp_case("garbage");
    let home = CacheHome::resolve(&root).expect("home");
    let path = home.database_path().expect("db path");
    std::fs::write(&path, b"this is not a database").expect("garbage");
    assert!(matches!(
        SqliteStore::open_create(&path),
        Err(crate::cache::StoreError::Corrupt)
    ));
}

#[test]
fn unsupported_schema_version_is_quarantined_and_rebuilt() {
    let root = temp_case("quarantine");
    copy_dir(&fixture(THREE_MODULES), &root);
    let home = CacheHome::resolve(&root).expect("home");
    let path = home.database_path().expect("db path");
    {
        let store = SqliteStore::open_create(&path).expect("store opens");
        assert!(store.integrity_ok());
    }
    // A store written by a different cache contract version is corruption
    // for this implementation.
    let connection = rusqlite::Connection::open(&path).expect("reopen");
    connection
        .execute(
            "UPDATE cache_meta SET value = 'dev.lekalo.cache@0.0.9' WHERE key = 'identity'",
            [],
        )
        .expect("tamper");
    drop(connection);
    let session = Session::open(&selection_for(&root), false).expect("session opens");
    let _ = session
        .load_model(&selection_for(&root))
        .expect("loads after rebuild");
    assert!(session.is_active(), "the store rebuilds after quarantine");
    let quarantine = root.join(".lekalo/cache/quarantine");
    let items = std::fs::read_dir(&quarantine)
        .expect("quarantine exists")
        .count();
    assert!(items >= 1, "the corrupt store is quarantined");
    // The rebuilt store is healthy and answers status.
    let selection = selection_for(&root);
    let result = crate::cache::status(&selection);
    assert!(result.to_json_string().contains("\"state\":\"ok\""));
}

#[test]
fn cold_and_warm_loads_match_the_published_loader_bytes() {
    let root = temp_case("equivalence");
    copy_dir(&fixture(THREE_MODULES), &root);
    let selection = selection_for(&root);
    for spans in [false, true] {
        let published = crate::loader::run(&selection, spans).to_json_string();
        let cold = cached_model_bytes(&selection, false, spans);
        assert_eq!(cold, published, "cold cache run, spans={spans}");
        let warm = cached_model_bytes(&selection, false, spans);
        assert_eq!(warm, published, "warm cache run, spans={spans}");
        let bypass = cached_model_bytes(&selection, true, spans);
        assert_eq!(bypass, published, "--no-cache run, spans={spans}");
    }
}

#[test]
fn warm_runs_restore_every_document_without_decoding() {
    let root = temp_case("warm");
    copy_dir(&fixture(THREE_MODULES), &root);
    let selection = selection_for(&root);
    let session = Session::open(&selection, false).expect("session opens");
    let (_, cold) = session.load_internal(&selection).expect("cold load");
    assert_eq!(cold.restored, 0);
    assert_eq!(cold.decoded, cold.sources);
    let (_, warm) = session.load_internal(&selection).expect("warm load");
    assert_eq!(
        warm.sources, 7,
        "3 modules with one entities file each plus project"
    );
    assert_eq!(warm.restored, 7, "every document is reused");
    assert_eq!(warm.decoded, 0, "nothing is re-decoded");
}

#[test]
fn one_changed_document_recomputes_only_itself() {
    let root = temp_case("locality");
    copy_dir(&fixture(THREE_MODULES), &root);
    let selection = selection_for(&root);
    let session = Session::open(&selection, false).expect("session opens");
    let (_, cold) = session.load_internal(&selection).expect("cold load");
    assert_eq!(cold.decoded, cold.sources);
    let (_, warm) = session.load_internal(&selection).expect("warm load");
    assert_eq!(warm.decoded, 0);

    // Touch exactly one document with a comment: new bytes, same tree.
    let touched = root.join("lekalo/modules/beta/entities.yaml");
    let mut text = std::fs::read_to_string(&touched).expect("read");
    text.push_str("# touched\n");
    std::fs::write(&touched, text).expect("write");

    let (model, stats) = session.load_internal(&selection).expect("reload");
    assert_eq!(stats.decoded, 1, "only the changed document re-decodes");
    assert_eq!(stats.restored, stats.sources - 1, "unrelated modules hit");

    // The semantic projection is byte-identical to a clean full rebuild.
    let bypass = Session::open(&selection, true).expect("bypass session");
    let rebuilt = bypass.load_model(&selection).expect("clean rebuild");
    assert_eq!(
        crate::loader::render_model_envelope(&model, false).to_json_string(),
        crate::loader::render_model_envelope(&rebuilt, false).to_json_string()
    );
}

#[test]
fn deleted_documents_invalidate_their_fragments() {
    let root = temp_case("tombstone");
    copy_dir(&fixture(THREE_MODULES), &root);
    let selection = selection_for(&root);
    let session = Session::open(&selection, false).expect("session opens");
    let _ = session.load_internal(&selection).expect("warm load");
    std::fs::remove_file(root.join("lekalo/modules/gamma/entities.yaml")).expect("delete");
    let (_, after) = session.load_internal(&selection).expect("reload");
    assert_eq!(after.restored, after.sources, "no stale fragment survives");
    assert_eq!(after.sources, 6, "the deleted source is simply gone");
}

#[test]
fn invalid_projects_fail_identically_with_and_without_cache() {
    let root = temp_case("invalid");
    copy_dir(&fixture("invalid-duplicate-definition"), &root);
    let selection = selection_for(&root);
    let published = crate::loader::run(&selection, false).to_json_string();
    let session = Session::open(&selection, false).expect("session opens");
    let failure = session.load_model(&selection).expect_err("invalid fails");
    assert_eq!(failure.to_json_string(), published);
    // The failed run also stores nothing that turns into a hit later.
    let again = session.load_model(&selection).expect_err("still invalid");
    assert_eq!(again.to_json_string(), published);
}

#[test]
fn bypass_touches_nothing_on_disk() {
    let root = temp_case("bypass");
    copy_dir(&fixture(THREE_MODULES), &root);
    let selection = selection_for(&root);
    let session = Session::open(&selection, true).expect("bypass session");
    assert!(!session.is_active());
    let _ = session.load_model(&selection).expect("loads");
    assert!(!root.join(".lekalo").exists(), "no runtime files appear");
}

#[test]
fn status_reports_missing_then_ok_then_state_survives_clear() {
    let root = temp_case("status");
    copy_dir(&fixture(THREE_MODULES), &root);
    let selection = selection_for(&root);
    let missing = crate::cache::status(&selection);
    assert!(missing.to_json_string().contains("\"state\":\"missing\""));

    {
        let session = Session::open(&selection, false).expect("session opens");
        let _ = session.load_internal(&selection).expect("load");
    }
    let ok = crate::cache::status(&selection);
    let json = ok.to_json_string();
    assert!(json.contains("\"state\":\"ok\""), "{json}");
    assert!(json.contains("\"parsed-fragment\""), "{json}");
    assert!(
        json.contains("\"schemaVersion\":\"lekalo/cache/v1.0.0\""),
        "{json}"
    );
    assert!(
        !json.contains('\\'),
        "no native path fragments in the wire: {json}"
    );

    let cleared = crate::cache::clear(&selection);
    assert_eq!(cleared.exit_code(), 0);
    let after = crate::cache::status(&selection);
    assert!(after.to_json_string().contains("\"state\":\"missing\""));
}

#[test]
fn a_symlinked_cache_home_denies_commands_fail_closed() {
    let root = temp_case("deny");
    copy_dir(&fixture(THREE_MODULES), &root);
    std::fs::create_dir_all(root.join(".lekalo/cache")).expect("home");
    let outside = temp_case("deny-target");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join(".lekalo/cache/cache.sqlite")).expect("symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&outside, root.join(".lekalo/cache/cache.sqlite"))
        .expect("symlink");
    let selection = selection_for(&root);
    let result = Session::open(&selection, false).expect_err("denied");
    assert_eq!(result.exit_code(), 3, "denied exits 3 on stdout");
    assert!(result.to_json_string().contains("structure.path-link"));
}

#[test]
fn parsed_fragment_records_round_trip_with_snapshots() {
    let root = temp_case("probe");
    let home = CacheHome::resolve(&root).expect("home");
    let path = home.database_path().expect("db path");
    let store = SqliteStore::open_create(&path).expect("store opens");
    let raw = crate::cache::canonical::sha256_digest(b"raw");
    let source_key = crate::cache::key::Key::Source {
        path: "lekalo/project.yaml".to_owned(),
        raw_digest: raw.clone(),
        normalized_digest: crate::cache::canonical::sha256_digest(b"normalized"),
    };
    let source_record = crate::cache::record::Record::new(
        crate::cache::key::RecordKind::Source,
        source_key,
        crate::cache::record::Binding::pipeline(),
        crate::cache::record::PayloadValue::new(raw.clone(), Vec::new()),
    )
    .sealed();
    assert!(store
        .put(&source_record.key.digest(), &source_record, None)
        .expect("source put"));
    let parse_key = crate::cache::key::Key::ParsedFragment {
        path: "lekalo/project.yaml".to_owned(),
        raw_digest: raw.clone(),
    };
    let mirror = b"{\"kind\":\"parsed\"}";
    let record = crate::cache::record::Record::new(
        crate::cache::key::RecordKind::ParsedFragment,
        parse_key,
        crate::cache::record::Binding::pipeline(),
        crate::cache::record::PayloadValue::new(
            crate::cache::canonical::sha256_digest(mirror),
            vec![crate::cache::record::PayloadField {
                name: "definitions".to_owned(),
                value: 1,
            }],
        ),
    )
    .with_input(source_record.key.digest())
    .sealed();
    assert!(record.validate());
    assert!(store
        .put(&record.key.digest(), &record, Some(mirror))
        .expect("parse put"));
    let entry = store
        .get(&record.key.digest())
        .expect("get")
        .expect("present");
    assert_eq!(entry.snapshot.as_deref(), Some(mirror.as_slice()));
    assert_eq!(
        store.dependency_edge_count().expect("edges"),
        1,
        "the input edge to the source record persists"
    );
}

// ---------------------------------------------------------------------------
// Issue #20 benchmark shapes: the closed small/medium/large protocols.
// ---------------------------------------------------------------------------

fn bench_generate(root: &Path, modules: usize, entities_per_module: usize) -> usize {
    use std::fs;
    let lekalo = root.join("lekalo");
    fs::create_dir_all(lekalo.join("modules")).expect("lekalo tree");
    fs::write(
        lekalo.join("project.yaml"),
        "schema_version: \"1.0.0\"\ndefinitions:\n  - id: bench\n    kind: project\n    version: 1\n",
    )
    .expect("project.yaml");
    let mut files = 1usize;
    for module in 0..modules {
        let name = format!("m{module:04}");
        let dir = lekalo.join("modules").join(&name);
        fs::create_dir_all(&dir).expect("module dir");
        files += 1;
        let mut defs = String::from("schema_version: \"1.0.0\"\ndefinitions:\n");
        defs.push_str(&format!(
            "  - id: {name}\n    kind: module\n    version: 1\n"
        ));
        for entity in 0..entities_per_module {
            defs.push_str(&format!(
                "  - id: {name}.entity{entity:03}\n    kind: entity\n    version: 1\n"
            ));
        }
        fs::write(
            dir.join("module.yaml"),
            format!("schema_version: \"1.0.0\"\ndefinitions:\n  - id: {name}\n    kind: module\n    version: 1\n"),
        )
        .expect("module.yaml");
        files += 1;
        fs::write(dir.join("entities.yaml"), defs).expect("entities");
        // Commands and queries reference the previous module's entities:
        // cross-module edges the dependency closure must follow.
        let target = if module == 0 { 0 } else { module - 1 };
        let mut commands = String::from("schema_version: \"1.0.0\"\ndefinitions:\n");
        for entity in 0..entities_per_module {
            commands.push_str(&format!(
                "  - id: {name}.create{entity:03}_cmd\n    kind: command\n    version: 1\n    effect: {name}.create{entity:03}\n"
            ));
            commands.push_str(&format!(
                "  - id: {name}.create{entity:03}\n    kind: effect\n    version: 1\n    operation: create\n    resource: m{target:04}.entity{entity:03}\n"
            ));
        }
        fs::write(dir.join("commands.yaml"), commands).expect("commands");
        files += 1;
        let mut queries = String::from("schema_version: \"1.0.0\"\ndefinitions:\n");
        for entity in 0..entities_per_module {
            queries.push_str(&format!(
                "  - id: {name}.get{entity:03}_query\n    kind: query\n    version: 1\n    reads: m{target:04}.entity{entity:03}\n"
            ));
        }
        fs::write(dir.join("queries.yaml"), queries).expect("queries");
        files += 1;
    }
    files
}

fn run_shape(tag: &str, modules: usize, entities_per_module: usize) {
    let root = temp_case(tag);
    let files = bench_generate(&root, modules, entities_per_module);
    let selection = selection_for(&root);
    let session = Session::open(&selection, false).expect("session opens");

    // Clean full build: every document decodes.
    let (_, clean) = session.load_internal(&selection).expect("clean build");
    assert!(session.is_active(), "the store opened after the gates");
    assert_eq!(clean.decoded, files, "clean build decodes everything");
    assert_eq!(clean.restored, 0);

    // Warm identical build: zero recomputation.
    let (model_warm, warm) = session.load_internal(&selection).expect("warm build");
    assert_eq!(warm.decoded, 0, "warm build re-decodes nothing");
    assert_eq!(warm.restored, files, "warm build reuses every document");

    // The reference: a bypass run over the same bytes.
    let bypass = Session::open(&selection, true).expect("bypass session");
    let model_bypass = bypass.load_model(&selection).expect("bypass build");
    assert_eq!(
        crate::loader::render_model_envelope(&model_warm, false).to_json_string(),
        crate::loader::render_model_envelope(&model_bypass, false).to_json_string(),
        "clean and incremental outputs are byte-identical"
    );

    // One-module source byte change: exactly that document re-decodes and
    // every unrelated module stays cached; the semantic output equals a
    // clean full rebuild byte for byte.
    let changed = root.join("lekalo/modules/m0001/entities.yaml");
    let mut text = std::fs::read_to_string(&changed).expect("read changed file");
    text.push_str("# touched\n");
    std::fs::write(&changed, text).expect("touch changed file");
    let (model_after, after) = session
        .load_internal(&selection)
        .expect("incremental build");
    assert_eq!(after.decoded, 1, "only the changed module re-decodes");
    assert_eq!(after.restored, files - 1, "unrelated modules stay cached");
    let bypass_after = bypass.load_model(&selection).expect("bypass rebuild");
    assert_eq!(
        crate::loader::render_model_envelope(&model_after, false).to_json_string(),
        crate::loader::render_model_envelope(&bypass_after, false).to_json_string(),
        "incremental result equals clean rebuild after one module change"
    );

    eprintln!(
        "{tag}: sources={files} warm_restored={} after_decoded={} after_restored={}",
        warm.restored, after.decoded, after.restored
    );
}

#[test]
fn small_shape_keeps_the_closure_local_and_bytes_identical() {
    run_shape("bench-small", 2, 5);
}

#[test]
#[ignore = "benchmark shape; run with --ignored --nocapture"]
fn medium_shape_keeps_the_closure_local_and_bytes_identical() {
    run_shape("bench-medium", 20, 10);
}

#[test]
#[ignore = "benchmark shape; run with --ignored --nocapture"]
fn large_shape_keeps_the_closure_local_and_bytes_identical() {
    run_shape("bench-large", 100, 10);
}

// ---------------------------------------------------------------------------
// Golden wire fixtures (tests/fixtures/cache/**): the committed vectors are
// the cross-language contract evidence. The Node release gate re-derives
// every digest with its own canonicalizer; this side proves the committed
// bytes still validate against the Rust implementation.
// ---------------------------------------------------------------------------

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/cache")
        .canonicalize()
        .expect("cache fixture directory exists")
}

#[test]
fn golden_record_fixtures_validate_and_match_their_pinned_digests() {
    use crate::cache::canonical::{canonical_bytes, sha256_digest};
    use crate::cache::record::Record;
    let records = fixture_dir().join("records");
    let mut checked = 0usize;
    for entry in std::fs::read_dir(&records).expect("records directory") {
        let path = entry.expect("entry").path();
        assert_eq!(
            path.extension().and_then(|e| e.to_str()),
            Some("json"),
            "{path:?}"
        );
        let bytes = std::fs::read(&path).expect("fixture bytes");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("fixture parses");
        let key_digest = value["keyDigest"].as_str().expect("keyDigest").to_owned();
        let payload_digest = value["payloadDigest"]
            .as_str()
            .expect("payloadDigest")
            .to_owned();
        let record: Record = serde_json::from_value(serde_json::Value::Object(
            value.as_object().expect("object").clone(),
        ))
        .expect("envelope deserializes");
        assert!(record.validate(), "{}: envelope validates", path.display());
        assert_eq!(
            record.key.digest(),
            key_digest,
            "{}: key digest pin",
            path.display()
        );
        assert_eq!(
            sha256_digest(&canonical_bytes(&record.payload.value)),
            payload_digest,
            "{}: payload digest pin",
            path.display()
        );
        checked += 1;
    }
    assert!(
        checked >= 5,
        "all five v1 record kinds are pinned: {checked}"
    );
}

#[test]
fn the_health_fixture_matches_the_closed_projection_vocabulary() {
    let bytes = std::fs::read(fixture_dir().join("health/ok.json")).expect("health fixture");
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("parses");
    assert_eq!(value["schemaVersion"], "lekalo/cache/v1.0.0");
    assert_eq!(value["identity"], "dev.lekalo.cache@1.0.0");
    assert_eq!(value["backend"], "sqlite");
    let states = [
        "ok",
        "empty",
        "missing",
        "disabled",
        "corrupt",
        "quarantined",
        "locked",
        "unreadable",
    ];
    assert!(states.contains(&value["state"].as_str().expect("state")));
}
