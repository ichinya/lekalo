//! The cached load pipeline (issue #20).
//!
//! The pipeline re-expresses the accepted loader phases 1-8 through the
//! same typed helpers, with one seam: the strict frontend decode of each
//! source document consults the parsed-fragment cache. Unchanged documents
//! are restored from the cache-owned snapshot; changed or unknown documents
//! decode exactly as the loader decodes them and are stored. Phases 7-8
//! always run in memory from the reused documents. The bypass path calls
//! the loader directly, so `--no-cache` is the published load path itself.
//!
//! Failures are byte-identical: every diagnostic of phases 1-6 is produced
//! by the same helpers with the same paths and spans; a corrupted or locked
//! store silently degrades to full recomputation, while a denied cache home
//! stays a fail-closed `structure.*` denial.

use crate::loader::error::Diagnostic;
use crate::loader::project_docs::{DocKind, Document, ModelVersion};
use crate::loader::source::{check_document_limit, check_total_limit, Source, MAX_DOCUMENT_BYTES};
use crate::loader::LoadSelection;
use crate::result::{DomainResult, Status};

use super::canonical::{canonical_bytes, sha256_digest};
use super::key::{Key, RecordKind};
use super::limits::{MAX_ENTRIES, MAX_TOTAL_PAYLOAD_BYTES};
use super::path::{CacheHome, HomeFailure};
use super::record::{Binding, PayloadField, PayloadValue, Record};
use super::snapshot::{document_mirror, document_restore, normalized_digest};
use super::sqlite::SqliteStore;
use super::StoreError;

/// Per-run statistics for the cache tests and benchmark harnesses; never
/// persisted, never part of any semantic output. The lib path itself
/// consumes only `parsed_key_digests`.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct LoadStats {
    pub(crate) sources: usize,
    pub(crate) decoded: usize,
    pub(crate) restored: usize,
    pub(crate) parsed_key_digests: Vec<String>,
}

/// One command's cache session: the validated home and, once the load
/// passed the structure gates, the open store. A session without a store
/// is a full-recompute session.
#[derive(Debug)]
pub struct Session {
    home: Option<CacheHome>,
    store: std::cell::RefCell<Option<SqliteStore>>,
}

impl Session {
    /// Open the session for an invoked command. `bypass` (`--no-cache`)
    /// touches nothing on disk. A denied cache home is a fail-closed
    /// `structure.*` denial; every other cache unavailability degrades to
    /// full recomputation. The store itself opens lazily, after the
    /// structure scan has passed: the cache never races the scan that
    /// governs its own home.
    pub fn open(selection: &LoadSelection, bypass: bool) -> Result<Session, DomainResult> {
        if bypass {
            return Ok(Self::inactive());
        }
        let root = crate::loader::root_for_selection(selection)?;
        let home = match CacheHome::resolve(&root) {
            Ok(home) => home,
            Err(HomeFailure::Denied { code, logical }) => {
                return Err(crate::loader::diagnostic::failure(
                    Status::Denied,
                    vec![Diagnostic::new(code).with_path(logical)],
                ));
            }
            Err(HomeFailure::Io) => return Ok(Self::inactive()),
        };
        if let Err(HomeFailure::Denied { code, logical }) = home.database_path() {
            return Err(crate::loader::diagnostic::failure(
                Status::Denied,
                vec![Diagnostic::new(code).with_path(logical)],
            ));
        }
        Ok(Session {
            home: Some(home),
            store: std::cell::RefCell::new(None),
        })
    }

    fn inactive() -> Session {
        Session {
            home: None,
            store: std::cell::RefCell::new(None),
        }
    }

    /// Open the store for this session (after the structure gates).
    fn ensure_store(&self) {
        let Some(home) = self.home.as_ref() else {
            return;
        };
        let mut store = self.store.borrow_mut();
        if store.is_none() {
            *store = open_store(home);
        }
    }

    /// Whether the session has a usable store.
    pub fn is_active(&self) -> bool {
        self.store.borrow().is_some()
    }

    /// The cached load: the exact loader aggregate with reused documents.
    pub fn load_model(
        &self,
        selection: &LoadSelection,
    ) -> Result<crate::loader::NormalizedModel, DomainResult> {
        self.load_internal(selection).map(|(model, _)| model)
    }

    /// The cached load plus the typed IR compilation, with the IR fragment
    /// recorded against the parsed input digests.
    pub fn load_compiled(
        &self,
        selection: &LoadSelection,
    ) -> Result<(crate::loader::NormalizedModel, crate::ir::Compilation), DomainResult> {
        let (model, stats) = self.load_internal(selection)?;
        let compilation = match crate::ir::compile(&model) {
            Err(failure) => return Err(failure.into_result()),
            Ok(compilation) => compilation,
        };
        self.record_ir(
            &stats.parsed_key_digests,
            model.model_version.as_str(),
            &compilation,
        );
        Ok((model, compilation))
    }

    /// Record the graph fragment: the canonical graph bytes keyed by the
    /// canonical IR digest. Best-effort; never fails the command.
    pub fn record_graph(
        &self,
        compilation: &crate::ir::Compilation,
        graph: &crate::graph::DependencyGraph,
    ) {
        self.ensure_store();
        let store_cell = self.store.borrow();
        let Some(store) = store_cell.as_ref() else {
            return;
        };
        let Ok(graph_bytes) = graph.to_canonical_json() else {
            return;
        };
        let graph_digest = sha256_digest(graph_bytes.as_bytes());
        let key = Key::GraphFragment {
            ir_digest: ir_digest(compilation),
        };
        let record = Record::new(
            RecordKind::GraphFragment,
            key,
            Binding::producer(&[
                (
                    "dev.lekalo.model",
                    model_identity(graph.model_version().as_str()),
                ),
                ("dev.lekalo.ir", crate::ir::IDENTITY),
                ("dev.lekalo.graph", crate::graph::IDENTITY),
            ]),
            PayloadValue::new(
                graph_digest,
                vec![
                    PayloadField {
                        name: "edges".to_owned(),
                        value: graph.edges().len() as u64,
                    },
                    PayloadField {
                        name: "nodes".to_owned(),
                        value: graph.nodes().len() as u64,
                    },
                ],
            ),
        )
        .sealed();
        put_if_absent(store, &record);
    }

    /// Record the effect fragment: the canonical effect bytes keyed by the
    /// canonical graph digest. Best-effort; never fails the command.
    pub fn record_effects(&self, graph: &crate::effects::EffectGraph) {
        self.ensure_store();
        let store_cell = self.store.borrow();
        let Some(store) = store_cell.as_ref() else {
            return;
        };
        let Ok(effect_bytes) = graph.to_canonical_json() else {
            return;
        };
        let graph_digest = sha256_digest(effect_bytes.as_bytes());
        let key = Key::EffectFragment {
            graph_digest: graph.ir_digest().to_owned(),
        };
        let record = Record::new(
            RecordKind::EffectFragment,
            key,
            Binding::producer(&[
                ("dev.lekalo.ir", crate::ir::IDENTITY),
                ("dev.lekalo.graph", crate::graph::IDENTITY),
                ("dev.lekalo.effects", crate::effects::IDENTITY),
            ]),
            PayloadValue::new(
                graph_digest,
                vec![
                    PayloadField {
                        name: "declared".to_owned(),
                        value: graph.declared().len() as u64,
                    },
                    PayloadField {
                        name: "detected".to_owned(),
                        value: graph.detected().len() as u64,
                    },
                ],
            ),
        )
        .sealed();
        put_if_absent(store, &record);
    }

    fn record_ir(
        &self,
        parsed_digests: &[String],
        model_version: &str,
        compilation: &crate::ir::Compilation,
    ) {
        self.ensure_store();
        let store_cell = self.store.borrow();
        let Some(store) = store_cell.as_ref() else {
            return;
        };
        let key = Key::IrFragment {
            model_version: model_version.to_owned(),
            parsed_digests: parsed_digests.to_vec(),
        };
        let record = Record::new(
            RecordKind::IrFragment,
            key,
            Binding::producer(&[
                ("dev.lekalo.model", model_identity(model_version)),
                ("dev.lekalo.ir", crate::ir::IDENTITY),
            ]),
            PayloadValue::new(ir_digest(compilation), Vec::new()),
        )
        .sealed();
        put_if_absent(store, &record);
    }

    /// The internal load with statistics and parsed key digests.
    pub(crate) fn load_internal(
        &self,
        selection: &LoadSelection,
    ) -> Result<(crate::loader::NormalizedModel, LoadStats), DomainResult> {
        let bypass = self.home.is_none();
        if bypass {
            // `--no-cache` is the published load path itself.
            let model = crate::loader::normalize_model(selection)?;
            return Ok((model, LoadStats::default()));
        }
        let failure = |status, diagnostics: Vec<Diagnostic>| {
            Err(crate::loader::diagnostic::failure(status, diagnostics))
        };

        // Phase 1: selection/discovery (shared with the loader).
        let root = crate::loader::root_for_selection(selection)?;

        // Phase 2: accepted #4 structure validation.
        let report = match crate::project_fs::Fs::validate_project(&root) {
            crate::project_fs::StructureOutcome::Valid(report) => report,
            crate::project_fs::StructureOutcome::Invalid(reasons) => {
                return Err(structure_failure(
                    crate::project_fs::StructureOutcome::Invalid(reasons),
                ))
            }
            crate::project_fs::StructureOutcome::Denied(reasons) => {
                return Err(structure_failure(
                    crate::project_fs::StructureOutcome::Denied(reasons),
                ))
            }
        };
        let fs = match crate::project_fs::Fs::open(&root) {
            Ok(fs) => fs,
            Err(_) => {
                return failure(
                    Status::Invalid,
                    vec![Diagnostic::new("structure.root-unreadable")],
                )
            }
        };

        // Phase 2b (issue #9): migration journal / runtime lock gate.
        if let Some(code) = crate::versioning::migration_recovery_code(&fs) {
            return failure(Status::Invalid, vec![Diagnostic::new(code)]);
        }

        // The structure gates have passed: now — and only now — the store
        // opens, so the cache never races the scan that governs its home.
        self.ensure_store();
        let store_cell = self.store.borrow();
        let store = store_cell.as_ref();

        // Phase 3: capability-safe enumeration and reads (the cache always
        // re-reads and re-hashes: content digests, never mtime).
        let mut sources: Vec<(String, DocKind)> =
            vec![("lekalo/project.yaml".to_owned(), DocKind::Project)];
        let mut module_directories = report.modules.clone();
        module_directories.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        for directory in &module_directories {
            sources.push((
                format!("lekalo/modules/{directory}/module.yaml"),
                DocKind::Module,
            ));
            for stem in crate::project_fs::KIND_STEMS {
                sources.push((
                    format!("lekalo/modules/{directory}/{stem}.yaml"),
                    DocKind::Symbol,
                ));
            }
        }
        sources.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let mut read_sources: Vec<Source> = Vec::new();
        let mut total_bytes = 0usize;
        for (logical, kind) in &sources {
            let parent = logical
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .unwrap_or_default();
            let name = logical.rsplit('/').next().unwrap_or_default();
            match fs.read_file_opt(parent, name, MAX_DOCUMENT_BYTES) {
                Ok(Some(bytes)) => {
                    total_bytes += bytes.len();
                    if let Err(diagnostic) = check_total_limit(total_bytes) {
                        diagnostics.push(diagnostic.with_path(logical.clone()));
                        break;
                    }
                    if let Err(diagnostic) = check_document_limit(logical, bytes.len()) {
                        diagnostics.push(diagnostic);
                        break;
                    }
                    read_sources.push(Source {
                        logical_path: logical.clone(),
                        bytes,
                    });
                }
                Ok(None) if *kind == DocKind::Symbol => {}
                Ok(None) => {
                    diagnostics.push(
                        Diagnostic::new("loader.io")
                            .with_path(logical.clone())
                            .with_data(serde_json::json!({ "detail": "document-missing" })),
                    );
                }
                Err(crate::project_fs::FsErrorKind::NotFound) if *kind == DocKind::Symbol => {}
                Err(crate::project_fs::FsErrorKind::Limit { max }) => {
                    diagnostics.push(
                        Diagnostic::new("loader.limit-exceeded")
                            .with_path(logical.clone())
                            .with_data(
                                serde_json::json!({ "limit": "document-bytes", "max": max }),
                            ),
                    );
                }
                Err(_) => {
                    diagnostics.push(
                        Diagnostic::new("loader.io")
                            .with_path(logical.clone())
                            .with_data(serde_json::json!({ "detail": "read-failed" })),
                    );
                }
            }
        }
        if !diagnostics.is_empty() {
            return failure(Status::Invalid, diagnostics);
        }

        // Phases 3b/4 with the parsed-fragment seam: unchanged documents
        // are restored from the cache-owned snapshot; the rest decode
        // exactly as the loader decodes them.
        let mut stats = LoadStats {
            sources: read_sources.len(),
            ..LoadStats::default()
        };
        let mut parsed_documents: Vec<Document> = Vec::new();
        let mut parsed_key_digests: Vec<String> = Vec::new();
        let mut parse_diagnostics: Vec<Diagnostic> = Vec::new();
        for source in &read_sources {
            let raw_digest = sha256_digest(&source.bytes);
            let kind = sources
                .iter()
                .find(|(logical, _)| *logical == source.logical_path)
                .map(|(_, kind)| *kind)
                .unwrap_or(DocKind::Symbol);
            match restore_parsed(store, &source.logical_path, &raw_digest) {
                Some(document) => {
                    stats.restored += 1;
                    parsed_documents.push(document);
                }
                None => {
                    stats.decoded += 1;
                    match crate::loader::decode_document_bytes(
                        &source.logical_path,
                        kind,
                        &source.bytes,
                    ) {
                        Ok(document) => {
                            store_parsed(store, &document, &raw_digest);
                            parsed_documents.push(document);
                        }
                        Err(mut errors) => {
                            for diagnostic in errors.iter_mut() {
                                if diagnostic.path.is_none() {
                                    diagnostic.path = Some(source.logical_path.clone());
                                }
                            }
                            parse_diagnostics.extend(errors);
                        }
                    }
                }
            }
        }
        if !parse_diagnostics.is_empty() {
            return failure(Status::Invalid, parse_diagnostics);
        }

        // The parsed-fragment key digests, sorted: the IR fragment input.
        for document in &parsed_documents {
            let raw_digest = read_sources
                .iter()
                .find(|source| source.logical_path == document.path)
                .map(|source| sha256_digest(&source.bytes))
                .unwrap_or_default();
            parsed_key_digests.push(
                Key::ParsedFragment {
                    path: document.path.clone(),
                    raw_digest,
                }
                .digest(),
            );
        }
        parsed_key_digests.sort();
        parsed_key_digests.dedup();
        stats.parsed_key_digests = parsed_key_digests;

        // Phase 5/6: the exact version gate over all documents.
        let mut unsupported: Vec<(String, String)> = Vec::new();
        let mut versions: Vec<(String, String)> = Vec::new();
        for document in &parsed_documents {
            if ModelVersion::parse_exact(&document.version).is_none() {
                unsupported.push((document.path.clone(), document.version.clone()));
            }
            versions.push((document.path.clone(), document.version.clone()));
        }
        if !unsupported.is_empty() {
            unsupported.sort();
            return failure(
                Status::UnsupportedVersion,
                vec![Diagnostic::new("versioning.unsupported-version").with_data(
                    serde_json::json!({
                        "documents": unsupported
                            .into_iter()
                            .map(|(path, version)| serde_json::json!({ "path": path, "version": version }))
                            .collect::<Vec<_>>(),
                    }),
                )],
            );
        }
        let mut distinct: Vec<&str> = versions
            .iter()
            .map(|(_, version)| version.as_str())
            .collect();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() > 1 {
            let mut sorted = versions;
            sorted.sort();
            return failure(
                Status::Invalid,
                vec![Diagnostic::new("versioning.mixed-versions").with_data(
                    serde_json::json!({
                        "documents": sorted
                            .into_iter()
                            .map(|(path, version)| serde_json::json!({ "path": path, "version": version }))
                            .collect::<Vec<_>>(),
                    }),
                )],
            );
        }
        let model_version = ModelVersion::parse_exact(distinct[0]).expect("gated exact literal");

        // Phase 6b (issue #9): the shared support-policy gate.
        if let Some(failure) = crate::versioning::gate_loaded_model_version(model_version) {
            return Err(failure);
        }

        // Phases 7-8: always run in memory over the reused documents.
        let model = crate::loader::build_normalized_model(model_version, parsed_documents)?;
        Ok((model, stats))
    }
}

/// The canonical IR bytes digest of one compilation.
fn ir_digest(compilation: &crate::ir::Compilation) -> String {
    sha256_digest(compilation.project.to_canonical_json().as_bytes())
}

/// The model contract identity for one exact model version.
fn model_identity(model_version: &str) -> &'static str {
    match model_version {
        "0.1.0" => "dev.lekalo.model@0.1.0",
        _ => "dev.lekalo.model@1.0.0",
    }
}

/// Open the store, quarantining a globally corrupt database once; any
/// other store unavailability degrades to `None` (full recompute).
fn open_store(home: &CacheHome) -> Option<SqliteStore> {
    let Ok(path) = home.database_path() else {
        return None;
    };
    match SqliteStore::open_create(&path) {
        Ok(store) => {
            if store.integrity_ok() {
                let _ = store.evict(MAX_ENTRIES, MAX_TOTAL_PAYLOAD_BYTES);
                Some(store)
            } else {
                home.quarantine_store(&SIDECARS).ok()?;
                SqliteStore::open_create(&path).ok()
            }
        }
        Err(StoreError::Corrupt) | Err(StoreError::UnsupportedVersion) => {
            home.quarantine_store(&SIDECARS).ok()?;
            SqliteStore::open_create(&path).ok()
        }
        Err(_) => None,
    }
}

/// The database file and its journal sidecars, quarantine candidates all.
const SIDECARS: [&str; 3] = [
    ".lekalo/cache/cache.sqlite",
    ".lekalo/cache/cache.sqlite-wal",
    ".lekalo/cache/cache.sqlite-shm",
];

/// Restore one parsed fragment; every validation failure is a miss.
fn restore_parsed(store: Option<&SqliteStore>, path: &str, raw_digest: &str) -> Option<Document> {
    let store = store?;
    let key = Key::ParsedFragment {
        path: path.to_owned(),
        raw_digest: raw_digest.to_owned(),
    };
    if !key.is_well_formed() {
        return None;
    }
    let key_digest = key.digest();
    let Ok(Some(entry)) = store.get(&key_digest) else {
        return None;
    };
    if sha256_digest(&entry.payload) != entry.payload_digest {
        return None;
    }
    if entry.record_kind != RecordKind::ParsedFragment.as_str() {
        return None;
    }
    if entry.key_bytes != canonical_bytes(&key) {
        return None;
    }
    // The summary payload must bind the snapshot bytes exactly.
    let Ok(summary) = serde_json::from_slice::<super::record::PayloadValue>(&entry.payload) else {
        return None;
    };
    let snapshot = entry.snapshot.as_deref()?;
    if sha256_digest(snapshot) != summary.digest {
        return None;
    }
    let Ok(mirror) = serde_json::from_slice::<super::snapshot::DocumentMirror>(snapshot) else {
        return None;
    };
    Some(document_restore(mirror))
}

/// Store one freshly decoded parsed fragment plus its source record.
pub(crate) fn store_parsed(store: Option<&SqliteStore>, document: &Document, raw_digest: &str) {
    let Some(store) = store else { return };
    let mirror = document_mirror(document);
    let normalized = normalized_digest(document);
    let mirror_bytes = serde_json::to_vec(&mirror).expect("document mirror serializes");

    // The source record: raw and normalized content digests, bounded size.
    let source_key = Key::Source {
        path: document.path.clone(),
        raw_digest: raw_digest.to_owned(),
        normalized_digest: normalized.clone(),
    };
    let source_record = Record::new(
        RecordKind::Source,
        source_key,
        Binding::pipeline(),
        PayloadValue::new(
            normalized.clone(),
            vec![PayloadField {
                name: "bytes".to_owned(),
                value: mirror_bytes.len() as u64,
            }],
        ),
    )
    .sealed();
    put_if_absent(store, &source_record);

    // The parsed-fragment record: the snapshot is the payload value's
    // bound producer snapshot; the `input` edge targets the source record.
    let parse_key = Key::ParsedFragment {
        path: document.path.clone(),
        raw_digest: raw_digest.to_owned(),
    };
    let record = Record::new(
        RecordKind::ParsedFragment,
        parse_key,
        Binding::pipeline(),
        PayloadValue::new(
            sha256_digest(&mirror_bytes),
            vec![PayloadField {
                name: "definitions".to_owned(),
                value: document.definitions.len() as u64,
            }],
        ),
    )
    .with_input(source_record_key_digest(
        document.path.as_str(),
        raw_digest,
        &normalized,
    ))
    .sealed();
    let _ = store.put(&record.key.digest(), &record, Some(&mirror_bytes));
}
/// The source record's key digest (the dependency edge target).
fn source_record_key_digest(path: &str, raw_digest: &str, normalized_digest: &str) -> String {
    Key::Source {
        path: path.to_owned(),
        raw_digest: raw_digest.to_owned(),
        normalized_digest: normalized_digest.to_owned(),
    }
    .digest()
}

/// Idempotent put for summary records (no snapshot bytes).
fn put_if_absent(store: &SqliteStore, record: &Record) {
    let _ = store.put(&record.key.digest(), record, None);
}

/// Map a structure outcome onto the accepted failure classes exactly as
/// the loader maps it.
fn structure_failure(outcome: crate::project_fs::StructureOutcome) -> DomainResult {
    let (status, reasons) = match outcome {
        crate::project_fs::StructureOutcome::Valid(_) => {
            unreachable!("valid has no failure")
        }
        crate::project_fs::StructureOutcome::Invalid(reasons) => (Status::Invalid, reasons),
        crate::project_fs::StructureOutcome::Denied(reasons) => (Status::Denied, reasons),
    };
    let diagnostics = reasons
        .into_iter()
        .map(|reason| {
            let mut diagnostic = Diagnostic::new(reason.code);
            if let Some(path) = reason.path {
                diagnostic = diagnostic.with_path(path);
            }
            diagnostic
        })
        .collect();
    crate::loader::diagnostic::failure(status, diagnostics)
}
