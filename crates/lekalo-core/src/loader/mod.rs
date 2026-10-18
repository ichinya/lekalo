//! Issue #7 loader: YAML/JSON sources, imports, normalization, canonical model.
//!
//! The pipeline runs in fixed phases and never runs a later phase once an
//! earlier phase produced diagnostics:
//!
//! 1. root selection/discovery (`structure.*` outcomes pass through),
//! 2. #4 structure validation (`structure.*` outcomes pass through),
//! 3. capability-safe enumeration/reads and the encoding/size gates,
//! 4. strict spanned parsing in sorted logical-path order,
//! 5. document shape and `schema_version` extraction,
//! 6. exact-version gate (`0.1.0` / `1.0.0` only),
//! 7. version-dispatched decoding: imports, collisions, graph, cycles,
//! 8. reference/type normalization,
//! 9. canonical aggregate plus optional source map.
//!
//! See `docs/loader.md` for the normative details.

pub mod canonical;
pub mod diagnostic;
pub mod error;
pub mod frontends;
pub mod imports;
pub mod normalize;
pub mod project_docs;
pub mod source;

use std::path::{Path, PathBuf};

use crate::result::{DomainResult, Status};
use canonical::Canonical;
use error::{bounded_import_echo, Diagnostic};
use frontends::{Node, Span};
use project_docs::decode;
pub use project_docs::{DocKind, Document, ModelVersion};
use source::{check_document_limit, check_total_limit, LineIndex, Source, MAX_DOCUMENT_BYTES};

/// Root selection for one load: explicit `--project`/`LEKALO_PROJECT`, or
/// discovery from the working directory.
#[derive(Clone, Debug, Default)]
pub struct LoadSelection {
    pub project: Option<String>,
}

/// The successful aggregate: the preserved model plus canonical bytes.
pub struct LoadedProject {
    pub model_version: ModelVersion,
    /// Canonical model JSON (`{"project":...,"modules":[...],"definitions":[...]}`).
    pub model_json: String,
    /// Sorted `sourceMap` entries when `--spans` was requested.
    pub source_map: Vec<SourceMapEntry>,
}

/// One source-map entry tying a model node back to its source span.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SourceMapEntry {
    pub path: String,
    #[serde(rename = "semanticId", skip_serializing_if = "Option::is_none")]
    pub semantic_id: Option<String>,
    pub pointer: String,
    pub start: Position,
    pub end: Position,
}

/// A 1-based line/column plus byte offset.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Position {
    pub byte: usize,
    pub line: usize,
    pub column: usize,
}

impl Position {
    fn of(pos: error::SpanPos) -> Self {
        Self {
            byte: pos.byte,
            line: pos.line,
            column: pos.column,
        }
    }
}

/// Run one load to completion. All input selection is relative to the
/// process working directory; the loader never writes and never reads
/// `.lekalo/**` as model input.
pub fn run(selection: &LoadSelection, spans_requested: bool) -> DomainResult {
    match normalize_model(selection) {
        Ok(model) => render_load_output(&model, spans_requested),
        Err(outcome) => outcome,
    }
}

/// One normalized definition with the identity, provenance, and source
/// crumbs the loader recorded while normalizing its reference surfaces.
#[derive(Clone, Debug)]
pub struct NormalizedDefinition {
    /// Fully qualified semantic ID (project or module ID, or symbol ID).
    pub id: String,
    /// Logical project-relative POSIX path of the declaring document.
    pub path: String,
    /// The normalized, spanned definition node.
    pub node: Node,
    /// Span of the definition mapping itself.
    pub span: Span,
    /// `(pointer suffix relative to this definition, span)` crumbs recorded
    /// during normalization, in walk order.
    pub crumbs: Vec<(String, Span)>,
}

/// The normalized aggregate of one successful load (phases 1-8): the exact
/// model version plus every definition in canonical output order with its
/// spans and reference crumbs.
///
/// This is the typed seam issue #8 builds on; `run` renders it into the
/// CLI envelope, and the IR compiler consumes it without re-parsing source.
#[derive(Clone, Debug)]
pub struct NormalizedModel {
    pub model_version: ModelVersion,
    pub project: Option<NormalizedDefinition>,
    /// Modules in canonical (semantic-ID byte) order.
    pub modules: Vec<NormalizedDefinition>,
    /// Symbol definitions in canonical (semantic-ID byte) order.
    pub definitions: Vec<NormalizedDefinition>,
}

/// One loaded source document plus its exact original bytes.
#[derive(Clone)]
pub(crate) struct LoadedDocument {
    pub document: Document,
    pub bytes: Vec<u8>,
}

/// The normalized aggregate plus the pinned inputs the migration planner
/// reuses: the validated root, its filesystem capability, and every loaded
/// document with the exact bytes it was loaded from.
pub(crate) struct ProjectLoad {
    pub model: NormalizedModel,
    pub root: PathBuf,
    pub documents: Vec<LoadedDocument>,
}

/// Decode one in-memory document exactly as phase 3b/4/5 would: encoding
/// gate, strict spanned parse, document shape, and version extraction.
pub(crate) fn decode_document_bytes(
    path: &str,
    kind: DocKind,
    bytes: &[u8],
) -> Result<Document, Vec<Diagnostic>> {
    let text = Source::validate_encoding(path, bytes).map_err(|diagnostic| vec![diagnostic])?;
    let index = LineIndex::new(&text);
    let parsed = match frontends::parse_document(&text, &index) {
        Ok(parsed) => parsed,
        Err(mut errors) => {
            for diagnostic in errors.iter_mut() {
                if diagnostic.path.is_none() {
                    diagnostic.path = Some(path.to_owned());
                }
            }
            return Err(errors);
        }
    };
    decode(path, kind, &parsed)
}

/// Terminal outcome of a loader phase: the normalized aggregate, or the
/// terminal failure. No rendering happens here; `run` owns the envelope.
pub fn normalize_model(selection: &LoadSelection) -> Result<NormalizedModel, DomainResult> {
    load_with_snapshot(selection).map(|loaded| loaded.model)
}

/// Load an already-resolved internal root through all accepted
/// structure/encoding/version/normalization gates. This is for private
/// captured roots; CLI selectors still use the invocation-relative
/// policy and must not be translated to this seam before selection.
pub fn normalize_captured_root(root: &Path) -> Result<NormalizedModel, DomainResult> {
    load_validated_root(root.to_owned()).map(|loaded| loaded.model)
}

/// The validated absolute project root of one selection (the public seam
/// for project-bound attachment reads through the classified filesystem
/// boundary; issue #113 r3).
pub fn project_root(selection: &LoadSelection) -> Result<PathBuf, DomainResult> {
    root_for_selection(selection)
}

/// Public phase-1 seam: resolve the selection to the absolute,
/// alias-free project root without reading model documents. Commands
/// that read project-relative artifacts after loading must bind those
/// reads to this exact root — never to the raw invocation-relative
/// selector, which resolves differently from another working directory.
/// (The migration service keeps the `pub(crate)` spelling for its
/// crash-recovery use.)
pub fn resolve_selection_root(selection: &LoadSelection) -> Result<PathBuf, DomainResult> {
    root_for_selection(selection)
}

/// Phase 1 only: resolve the selection to an absolute project root
/// without reading model documents. The migration service uses this to
/// recover a crashed transaction before the loader's fail-closed gate.
pub(crate) fn root_for_selection(selection: &LoadSelection) -> Result<PathBuf, DomainResult> {
    let failure =
        |status, diagnostics: Vec<Diagnostic>| Err(diagnostic::failure(status, diagnostics));
    match &selection.project {
        Some(selector) => {
            if let Some(code) = crate::project_fs::selection_violation(selector) {
                return failure(Status::Invalid, vec![Diagnostic::new(code)]);
            }
            crate::project_fs::Fs::check_selection(selector, "structure.project-not-directory")
                .map_err(structure_failure)
        }
        None => {
            let cwd = std::env::current_dir().map_err(|_| {
                diagnostic::failure(
                    Status::Invalid,
                    vec![Diagnostic::new("structure.root-unreadable")],
                )
            })?;
            match crate::project_fs::Fs::find_root(&cwd) {
                Ok(Some(path)) => Ok(path),
                Ok(None) => failure(
                    Status::Invalid,
                    vec![Diagnostic::new("structure.root-not-found")],
                ),
                Err(outcome) => Err(structure_failure(outcome)),
            }
        }
    }
}

/// `normalize_model` plus the pinned snapshot the issue #9 migration
/// planner consumes; the read path is byte-identical.
pub(crate) fn load_with_snapshot(selection: &LoadSelection) -> Result<ProjectLoad, DomainResult> {
    let root = resolve_selection_root(selection)?;
    load_validated_root(root)
}

/// [`load_with_snapshot`] with one document's bytes overlaid before any
/// parsing (finding r4 9): the editor's unsaved buffer is the authority
/// for exactly its declared document, so a malformed saved copy must
/// not fail the load before the buffer is formed. Every other document
/// is still read, parsed, and validated; every filesystem, structure,
/// and version boundary stays armed. The overlay only substitutes bytes
/// for a path the confined inventory actually enumerates.
pub(crate) fn load_with_snapshot_and_overlay(
    selection: &LoadSelection,
    overlay_path: &str,
    overlay_bytes: &[u8],
) -> Result<ProjectLoad, DomainResult> {
    let root = resolve_selection_root(selection)?;
    load_validated_root_opt_with_overlay(root, false, Some((overlay_path, overlay_bytes)))
}

/// Load an already-selected absolute root through phases 2-8 (the migration
/// post-apply verification reuses the exact read path this way).
pub(crate) fn load_validated_root(root: PathBuf) -> Result<ProjectLoad, DomainResult> {
    load_validated_root_opt(root, false)
}

/// [`load_validated_root`] with the formatter recovery gate made
/// journal-neutral: only the formatter's own post-restore semantic
/// oracle may pass `true`, after it has already restored every pinned
/// preimage and before it clears the journal (finding r4 5). The
/// migration gate (2b) and every other phase stay armed; the option
/// never widens any read — it only defers the one gate the recovering
/// owner has already satisfied by construction.
pub(crate) fn load_validated_root_opt(
    root: PathBuf,
    ignore_format_journal: bool,
) -> Result<ProjectLoad, DomainResult> {
    load_validated_root_opt_with_overlay(root, ignore_format_journal, None)
}

/// [`load_validated_root_opt`] with one document's bytes substituted
/// before any parsing (finding r4 9). `document_overlay` names a path of
/// the confined inventory; its bytes replace the saved copy for the read
/// so a malformed saved document cannot fail the load before the editor
/// buffer is formed. Existence semantics stay: a required document
/// missing on disk is still an error even when overlaid.
pub(crate) fn load_validated_root_opt_with_overlay(
    root: PathBuf,
    ignore_format_journal: bool,
    document_overlay: Option<(&str, &[u8])>,
) -> Result<ProjectLoad, DomainResult> {
    load_validated_root_reusing(root, ignore_format_journal, document_overlay, None)
}

/// Read-only incremental frontend seam. Reuse is supplied only by an
/// in-memory owner that checks exact bytes/path/kind against a previously
/// accepted decode; all structure, limits, version and aggregate gates
/// below still execute, and deleted/renamed inputs are re-enumerated.
pub(crate) type DocumentReuse<'a> = dyn FnMut(&str, DocKind, &[u8]) -> Option<Document> + 'a;
pub(crate) fn load_validated_root_reusing(
    root: PathBuf,
    ignore_format_journal: bool,
    document_overlay: Option<(&str, &[u8])>,
    mut reuse: Option<&mut DocumentReuse<'_>>,
) -> Result<ProjectLoad, DomainResult> {
    let failure =
        |status, diagnostics: Vec<Diagnostic>| Err(diagnostic::failure(status, diagnostics));

    // Phase 2: accepted #4 structure validation.
    let validation = crate::project_fs::Fs::validate_project(&root);
    let report = match validation {
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

    // Phase 2b (issue #9): fail closed while a migration journal or runtime
    // migration lock exists; readers never observe a half-migrated project.
    // Issue #73 extends the same barrier to interrupted change transactions
    // under `.lekalo/cache/change`.
    if let Some(code) = crate::versioning::migration_recovery_code(&fs) {
        return failure(Status::Invalid, vec![Diagnostic::new(code)]);
    }
    if let Some(code) = crate::change::recovery_barrier_code(&fs) {
        // The change rule is admitted for the denied class: the barrier
        // outcome is denied (exit 3) instead of invalid.
        return failure(Status::Denied, vec![Diagnostic::new(code)]);
    }

    // Phase 2c (issue #94): the same fail-closed rule for an interrupted
    // formatter transaction; readers refuse, only an explicit mutating
    // format recovers. The recovering owner's own post-restore oracle
    // (finding r4 5) is journal-neutral by construction: it runs after
    // every pinned preimage has been restored, while the journal that
    // authorizes the restore is still in place.
    if let Some(code) = crate::format::transaction::recovery_code(&fs) {
        if !ignore_format_journal {
            return failure(Status::Invalid, vec![Diagnostic::new(code)]);
        }
    }

    // NOTE (issue #73 r8, devin F1/F3 + omp F3): an interrupted adoption
    // installation deliberately does NOT gate this reader path. The
    // adoption journals are registered in the SHARED-WRITER sibling
    // barrier (project_writer.rs) with a precise stranded predicate —
    // every non-adoption mutating writer refuses until an explicit
    // adoption recovery resolves the journal — while Model readers stay
    // open by contract: adoption's compare-and-swap writes are per-file
    // atomic renames (an interrupted apply leaves each file internally
    // consistent, never a half-written document), and the family's own
    // self-heal contract (pinned by test-adoption-refactor.mjs) requires
    // its mutating entries — including re-apply and the root-resolved
    // recovery surfaces — to stay reachable while a journal is stranded;
    // a reader barrier here would strand the resolver behind the very
    // state it exists to resolve.

    // Phase 3: capability-safe enumeration and reads.
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
        // The editor overlay (finding r4 9): the declared document's
        // buffer replaces the saved bytes before any parse, so a
        // malformed saved copy cannot block the load. Existence
        // semantics stay: a required document missing on disk is an
        // error even when overlaid; an optional kind document absent on
        // disk is supplied by the overlay instead of being skipped.
        let overlaid = matches!(document_overlay, Some((path, _)) if path == logical);
        match fs.read_file_opt(parent, name, MAX_DOCUMENT_BYTES) {
            Ok(Some(bytes)) => {
                let bytes = match document_overlay {
                    Some((_, overlay_bytes)) if overlaid => overlay_bytes.to_vec(),
                    _ => bytes,
                };
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
            // Absence is legal only for the seven optional kind files —
            // and for the overlaid document, whose buffer supplies it.
            Ok(None) if *kind == DocKind::Symbol => {
                if let Some((_, overlay_bytes)) = document_overlay {
                    if overlaid {
                        read_sources.push(Source {
                            logical_path: logical.clone(),
                            bytes: overlay_bytes.to_vec(),
                        });
                    }
                }
            }
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
                        .with_data(serde_json::json!({ "limit": "document-bytes", "max": max })),
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

    // Phase 3b: encoding gate, then Phase 4: parsing.
    let mut parsed_documents: Vec<Document> = Vec::new();
    let mut parse_diagnostics: Vec<Diagnostic> = Vec::new();
    for source in &read_sources {
        let kind = sources
            .iter()
            .find(|(logical, _)| *logical == source.logical_path)
            .map(|(_, kind)| *kind)
            .unwrap_or(DocKind::Symbol);
        if let Some(reuse) = reuse.as_deref_mut() {
            if let Some(document) = reuse(&source.logical_path, kind, &source.bytes) {
                parsed_documents.push(document);
                continue;
            }
        }
        let text = match Source::validate_encoding(&source.logical_path, &source.bytes) {
            Ok(text) => text,
            Err(diagnostic) => {
                parse_diagnostics.push(diagnostic);
                continue;
            }
        };
        let index = LineIndex::new(&text);
        match frontends::parse_document(&text, &index) {
            Ok(parsed) => match decode(&source.logical_path, kind, &parsed) {
                Ok(document) => parsed_documents.push(document),
                Err(errors) => parse_diagnostics.extend(errors),
            },
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
    if !parse_diagnostics.is_empty() {
        return failure(Status::Invalid, parse_diagnostics);
    }

    // Phase 5/6: version gate across all documents.
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

    // Phase 6b (issue #9): the shared support-policy gate. Deprecated stays
    // loadable; unregistered and retired versions fail closed with exit 5
    // before any canonicalization or write. One check serves load, IR, and
    // migrate alike; there is no second IR-only gate.
    if let Some(failure) = crate::versioning::gate_loaded_model_version(model_version) {
        return Err(failure);
    }

    // Pin the loaded documents with their exact bytes for the issue #9
    // migration planner before the aggregate consumes them.
    let snapshot: Vec<LoadedDocument> = parsed_documents
        .iter()
        .map(|document| LoadedDocument {
            document: document.clone(),
            bytes: read_sources
                .iter()
                .find(|source| source.logical_path == document.path)
                .map(|source| source.bytes.clone())
                .unwrap_or_default(),
        })
        .collect();
    let model = build_normalized_model(model_version, parsed_documents)?;
    Ok(ProjectLoad {
        model,
        root,
        documents: snapshot,
    })
}

/// Phases 7-8 over already-gated documents: collisions, imports, graph,
/// normalization, and the typed aggregate.
pub(crate) fn build_normalized_model(
    model_version: ModelVersion,
    parsed_documents: Vec<Document>,
) -> Result<NormalizedModel, DomainResult> {
    let failure =
        |status, diagnostics: Vec<Diagnostic>| Err(diagnostic::failure(status, diagnostics));
    // Phase 7: version-dispatched decoding: collisions, imports, graph.
    let mut collision_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut project_document: Option<Document> = None;
    let mut module_vertices: Vec<imports::ModuleVertex> = Vec::new();
    let mut module_documents: Vec<Document> = Vec::new();
    let mut symbol_documents: Vec<Document> = Vec::new();

    for document in &parsed_documents {
        match document.kind {
            DocKind::Project => {
                if project_document.replace(document.clone()).is_some() {
                    // Enforced as definitions-count during decode; defensive.
                    collision_diagnostics.push(
                        Diagnostic::new("loader.document-shape")
                            .with_path(document.path.clone())
                            .with_data(serde_json::json!({ "detail": "definitions-count" })),
                    );
                }
            }
            DocKind::Module => {
                let definition = &document.definitions[0];
                // Imports: module.yaml alone declares them.
                let mut module_imports: Vec<String> = Vec::new();
                if let Some(imports_entry) = definition.node.get_entry("imports") {
                    match imports_entry.value.as_seq() {
                        Some(items) => {
                            for item in items {
                                match item.as_str() {
                                    Some(text) => module_imports.push(text.to_owned()),
                                    None => {
                                        collision_diagnostics.push(
                                            Diagnostic::new("loader.import-invalid")
                                                .with_path(document.path.clone())
                                                .with_span(item.span)
                                                .with_data(serde_json::json!({
                                                    "detail": "import-not-string",
                                                })),
                                        );
                                    }
                                }
                            }
                        }
                        None => collision_diagnostics.push(
                            Diagnostic::new("loader.import-invalid")
                                .with_path(document.path.clone())
                                .with_span(imports_entry.value.span)
                                .with_data(serde_json::json!({ "detail": "imports-not-array" })),
                        ),
                    }
                }
                for import in &module_imports {
                    let span = definition
                        .node
                        .get_entry("imports")
                        .and_then(|entry| entry.value.as_seq())
                        .and_then(|items| {
                            items
                                .iter()
                                .find(|item| item.as_str() == Some(import.as_str()))
                                .map(|item| item.span)
                        })
                        .unwrap_or(definition.span);
                    // Path-policy denial first: exit 3.
                    if import.contains('/')
                        || import.contains('\\')
                        || import.contains("..")
                        || import.contains('%')
                        || import.contains(':')
                        || import.contains('~')
                        || import.starts_with('/')
                    {
                        collision_diagnostics.push(
                            Diagnostic::new("loader.path-escape")
                                .with_path(document.path.clone())
                                .with_span(span)
                                .with_data(
                                    serde_json::json!({ "import": bounded_import_echo(import) }),
                                ),
                        );
                        continue;
                    }
                    if !model_version.module_id_valid(import) {
                        collision_diagnostics.push(
                            Diagnostic::new("loader.import-invalid")
                                .with_path(document.path.clone())
                                .with_span(span)
                                .with_data(
                                    serde_json::json!({ "import": bounded_import_echo(import) }),
                                ),
                        );
                    }
                }
                // Duplicate imports inside one declaration.
                let mut seen = std::collections::BTreeSet::new();
                for import in &module_imports {
                    if !seen.insert(import.as_str()) {
                        collision_diagnostics.push(
                            Diagnostic::new("loader.import-invalid")
                                .with_path(document.path.clone())
                                .with_data(serde_json::json!({
                                    "detail": "duplicate-import",
                                    "import": bounded_import_echo(import),
                                })),
                        );
                    }
                }
                let directory = document
                    .path
                    .strip_prefix("lekalo/modules/")
                    .and_then(|rest| rest.strip_suffix("/module.yaml"))
                    .unwrap_or_default()
                    .to_owned();
                module_vertices.push(imports::ModuleVertex {
                    module_id: definition.id.clone(),
                    directory,
                    imports: module_imports,
                });
                module_documents.push(document.clone());
            }
            DocKind::Symbol => symbol_documents.push(document.clone()),
        }
    }

    // Duplicate symbol IDs within one document; conflicting declarations
    // across documents (even byte-identical payloads).
    for document in &symbol_documents {
        let mut seen: std::collections::BTreeMap<&str, ()> = std::collections::BTreeMap::new();
        for definition in &document.definitions {
            if seen.insert(definition.id.as_str(), ()).is_some() {
                collision_diagnostics.push(
                    Diagnostic::new("loader.duplicate-definition")
                        .with_path(document.path.clone())
                        .with_span(definition.id_span)
                        .with_data(serde_json::json!({ "id": definition.id })),
                );
            }
        }
    }
    {
        let mut declarations: std::collections::BTreeMap<&str, std::collections::BTreeSet<&str>> =
            std::collections::BTreeMap::new();
        for document in &symbol_documents {
            for definition in &document.definitions {
                declarations
                    .entry(definition.id.as_str())
                    .or_default()
                    .insert(document.path.as_str());
            }
        }
        for (id, paths) in declarations {
            // Symbols conflict across documents project-wide; module IDs
            // are handled by the dedicated duplicate-module-id check.
            if paths.len() > 1 {
                let sorted_paths: Vec<&str> = paths.into_iter().collect();
                collision_diagnostics.push(
                    Diagnostic::new("loader.conflicting-declaration")
                        .with_path(sorted_paths[0].to_owned())
                        .with_data(serde_json::json!({ "id": id, "paths": sorted_paths })),
                );
            }
        }
    }
    collision_diagnostics.extend(imports::duplicate_module_ids(&module_vertices));
    if !collision_diagnostics.is_empty() {
        return failure(Status::Invalid, collision_diagnostics);
    }

    // Import graph: missing imports, then cycles.
    let missing = imports::missing_imports(&module_vertices);
    if !missing.is_empty() {
        return failure(Status::Invalid, missing);
    }
    let cycles = imports::import_cycles(&module_vertices);
    if !cycles.is_empty() {
        return failure(Status::Invalid, cycles);
    }

    // Phase 8: normalization over the output-ordered model.
    let mut modules: Vec<(String, Document)> = module_documents
        .into_iter()
        .map(|document| (document.definitions[0].id.clone(), document))
        .collect();
    modules.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

    let symbols_by_module: Vec<(String, Vec<String>)> = {
        let mut map: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for document in &symbol_documents {
            // The declaring module of a symbol is its ID's first segment.
            for definition in &document.definitions {
                if let Some(prefix) = definition.id.split('.').next() {
                    map.entry(prefix.to_owned())
                        .or_default()
                        .push(definition.id.clone());
                }
            }
        }
        let mut list: Vec<(String, Vec<String>)> = map.into_iter().collect();
        for (_, symbols) in &mut list {
            symbols.sort();
        }
        list
    };
    let declared_modules: Vec<String> = modules.iter().map(|(id, _)| id.clone()).collect();
    let import_lists: Vec<(String, Vec<String>)> = module_vertices
        .iter()
        .map(|vertex| (vertex.module_id.clone(), vertex.imports.clone()))
        .collect();
    let symbol_index =
        normalize::SymbolIndex::build(symbols_by_module, declared_modules, import_lists);

    let mut normalized_modules: Vec<NormalizedDefinition> = Vec::new();
    for (_, document) in modules.iter() {
        let definition = &document.definitions[0];
        let mut node = definition.node.clone();
        let mut context = normalize::NormalizeContext::new(&symbol_index, &definition.id);
        normalize::normalize_definition(&mut node, &mut context);
        normalize::sort_imports(&mut node);
        if !context.diagnostics.is_empty() {
            let diagnostics = context
                .diagnostics
                .into_iter()
                .map(|diagnostic| {
                    if diagnostic.path.is_none() {
                        diagnostic.with_path(document.path.clone())
                    } else {
                        diagnostic
                    }
                })
                .collect();
            return failure(Status::Invalid, diagnostics);
        }
        normalized_modules.push(NormalizedDefinition {
            id: definition.id.clone(),
            path: document.path.clone(),
            node,
            span: definition.span,
            crumbs: context.source_entries,
        });
    }

    let mut normalized_definitions: Vec<NormalizedDefinition> = Vec::new();
    for document in &symbol_documents {
        for definition in &document.definitions {
            let mut node = definition.node.clone();
            let current_module = definition.id.split('.').next().unwrap_or_default();
            let mut context = normalize::NormalizeContext::new(&symbol_index, current_module);
            normalize::normalize_definition(&mut node, &mut context);
            if !context.diagnostics.is_empty() {
                let diagnostics = context
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| {
                        if diagnostic.path.is_none() {
                            diagnostic.with_path(document.path.clone())
                        } else {
                            diagnostic
                        }
                    })
                    .collect();
                return failure(Status::Invalid, diagnostics);
            }
            normalized_definitions.push(NormalizedDefinition {
                id: definition.id.clone(),
                path: document.path.clone(),
                node,
                span: definition.span,
                crumbs: context.source_entries,
            });
        }
    }
    normalized_definitions.sort_by(|left, right| left.id.as_bytes().cmp(right.id.as_bytes()));

    Ok(NormalizedModel {
        model_version,
        project: project_document.map(|document| {
            let definition = &document.definitions[0];
            NormalizedDefinition {
                id: definition.id.clone(),
                path: document.path.clone(),
                node: definition.node.clone(),
                span: definition.span,
                // The project definition carries no normalized reference
                // surfaces, so normalization recorded no crumbs.
                crumbs: Vec::new(),
            }
        }),
        modules: normalized_modules,
        definitions: normalized_definitions,
    })
}

/// Render the terminal CLI envelope from one normalized aggregate (phase
/// 9) for the issue #20 cache pipeline: the exact `run` rendering, on the
/// already-loaded model.
pub(crate) fn render_model_envelope(
    model: &NormalizedModel,
    spans_requested: bool,
) -> DomainResult {
    render_load_output(model, spans_requested)
}

/// Render the terminal CLI envelope from one normalized aggregate (phase
/// 9): canonical model bytes, the optional sorted source map, and the
/// success line. Byte-identical to the pre-#8 rendering.
fn render_load_output(model: &NormalizedModel, spans_requested: bool) -> DomainResult {
    let model_version = model.model_version;
    let mut source_map: Vec<SourceMapEntry> = Vec::new();
    if spans_requested {
        if let Some(project) = &model.project {
            source_map.push(SourceMapEntry {
                path: project.path.clone(),
                semantic_id: Some(project.id.clone()),
                pointer: "/project".to_owned(),
                start: Position::of(project.span.start),
                end: Position::of(project.span.end),
            });
        }
        for (module_index, module) in model.modules.iter().enumerate() {
            source_map.push(SourceMapEntry {
                path: module.path.clone(),
                semantic_id: Some(module.id.clone()),
                pointer: format!("/modules/{module_index}"),
                start: Position::of(module.span.start),
                end: Position::of(module.span.end),
            });
            for (suffix, span) in &module.crumbs {
                source_map.push(SourceMapEntry {
                    path: module.path.clone(),
                    semantic_id: None,
                    pointer: format!("/modules/{module_index}/{suffix}"),
                    start: Position::of(span.start),
                    end: Position::of(span.end),
                });
            }
        }
        for (position, definition) in model.definitions.iter().enumerate() {
            source_map.push(SourceMapEntry {
                path: definition.path.clone(),
                semantic_id: Some(definition.id.clone()),
                pointer: format!("/definitions/{position}"),
                start: Position::of(definition.span.start),
                end: Position::of(definition.span.end),
            });
        }
    }

    let canonical = canonical_model_bytes(model);

    let mut json = String::from("{\"status\":\"valid\",\"modelVersion\":");
    json.push_str(&Canonical::Str(model_version.as_str().to_owned()).to_json());
    json.push_str(",\"model\":");
    json.push_str(&canonical);
    if spans_requested {
        source_map.sort_by(|left, right| {
            (left.path.clone(), left.pointer.clone(), left.start.byte).cmp(&(
                right.path.clone(),
                right.pointer.clone(),
                right.start.byte,
            ))
        });
        json.push_str(",\"sourceMap\":");
        json.push_str(&serde_json::to_string(&source_map).expect("source map serializes"));
    }
    json.push('}');

    let human = format!(
        "loaded model {}: {} modules, {} definitions",
        model_version.as_str(),
        model.modules.len(),
        model.definitions.len()
    );

    DomainResult::model(json, human)
}

/// The exact load-envelope success payload of one normalized aggregate
/// without a source map — byte-identical to what `run` renders into its
/// success envelope for a spans-free load. Authority derivations consume
/// this so a single-snapshot derivation (r6, codex F11 / devin F8: one
/// filesystem read produces the Model payload, the typed Model, and the
/// source pins) pins the exact same digest the two-read derivation did.
pub fn model_envelope_payload(model: &NormalizedModel) -> String {
    let mut json = String::from("{\"status\":\"valid\",\"modelVersion\":");
    json.push_str(&Canonical::Str(model.model_version.as_str().to_owned()).to_json());
    json.push_str(",\"model\":");
    json.push_str(&canonical_model_bytes(model));
    json.push('}');
    json
}

/// The exact canonical Model bytes of one normalized aggregate: compact
/// JSON with byte-sorted keys and semantic-ID-ordered collections — the
/// same bytes the load envelope renders (phase 9) and the same digest
/// input the Model-custody pins bind to.
pub fn canonical_model_bytes(model: &NormalizedModel) -> String {
    let project_canonical = model
        .project
        .as_ref()
        .map(|project| Canonical::from_node(&project.node));
    let modules_canonical: Vec<Canonical> = model
        .modules
        .iter()
        .map(|module| Canonical::from_node(&module.node))
        .collect();
    let definitions_canonical: Vec<Canonical> = model
        .definitions
        .iter()
        .map(|definition| Canonical::from_node(&definition.node))
        .collect();
    let mut canonical = String::new();
    canonical.push_str("{\"definitions\":");
    Canonical::Seq(definitions_canonical).write_json(&mut canonical);
    canonical.push_str(",\"modules\":");
    Canonical::Seq(modules_canonical).write_json(&mut canonical);
    if let Some(project) = &project_canonical {
        canonical.push_str(",\"project\":");
        project.write_json(&mut canonical);
    }
    canonical.push('}');
    canonical
}

fn structure_failure(outcome: crate::project_fs::StructureOutcome) -> DomainResult {
    match outcome {
        crate::project_fs::StructureOutcome::Invalid(reasons) => {
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
            diagnostic::failure(Status::Invalid, diagnostics)
        }
        crate::project_fs::StructureOutcome::Denied(reasons) => {
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
            diagnostic::failure(Status::Denied, diagnostics)
        }
        crate::project_fs::StructureOutcome::Valid(_) => unreachable!("valid has no failure"),
    }
}
