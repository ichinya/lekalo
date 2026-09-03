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
pub mod error;
pub mod frontends;
pub mod imports;
pub mod normalize;
pub mod project_docs;
pub mod source;

use std::path::PathBuf;

use canonical::Canonical;
use error::{bounded_import_echo, finalize_diagnostics, Diagnostic, LoadStatus};
use frontends::Node;
use project_docs::{decode, DocKind, Document, ModelVersion};
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

/// The terminal result of one load: exact JSON envelope bytes, the stable
/// human line, and the exit class.
pub struct LoadOutput {
    pub status: LoadStatus,
    pub json: String,
    pub human: String,
}

impl LoadOutput {
    fn failure(status: LoadStatus, diagnostics: Vec<Diagnostic>) -> Self {
        let status = if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "loader.path-escape")
            && status == LoadStatus::Invalid
        {
            // A path policy denial inside a failing phase outranks generic
            // invalidity: exit 3 stdout, matching the #4 protocol.
            LoadStatus::Denied
        } else {
            status
        };
        let finalized = finalize_diagnostics(diagnostics);
        let envelope = failure_envelope(status, &finalized);
        let codes = finalized
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let human = format!("{}: {}", status.as_str(), codes);
        Self {
            status,
            json: envelope,
            human,
        }
    }
}

/// Serialize the failure envelope with pretty indentation (matching the
/// accepted checker envelopes); success stays one compact line.
fn failure_envelope(status: LoadStatus, diagnostics: &[Diagnostic]) -> String {
    let mut out = String::from("{\n  \"status\": \"");
    out.push_str(status.as_str());
    out.push_str("\",\n  \"reasonCodes\": [");
    if diagnostics.is_empty() {
        out.push_str("]\n}");
        return out;
    }
    out.push('\n');
    for diagnostic in diagnostics {
        out.push_str("    ");
        out.push_str(&serde_json::to_string(diagnostic).expect("diagnostic serializes"));
        out.push_str(",\n");
    }
    // Trim the trailing comma of the last entry.
    out.truncate(out.len() - 2);
    out.push_str("\n  ]\n}\n");
    out
}

/// Run one load to completion. All input selection is relative to the
/// process working directory; the loader never writes and never reads
/// `.lekalo/**` as model input.
pub fn run(selection: &LoadSelection, spans_requested: bool) -> LoadOutput {
    let failure = |status, diagnostics: Vec<Diagnostic>| LoadOutput::failure(status, diagnostics);

    // Phase 1: selection/discovery.
    let root: PathBuf = match &selection.project {
        Some(selector) => {
            if let Some(code) = crate::project_fs::selection_violation(selector) {
                return failure(LoadStatus::Invalid, vec![Diagnostic::new(code)]);
            }
            match crate::project_fs::Fs::check_selection(
                selector,
                "structure.project-not-directory",
            ) {
                Ok(path) => path,
                Err(outcome) => return structure_failure(outcome),
            }
        }
        None => {
            let cwd = match std::env::current_dir() {
                Ok(cwd) => cwd,
                Err(_) => {
                    return failure(
                        LoadStatus::Invalid,
                        vec![Diagnostic::new("structure.root-unreadable")],
                    )
                }
            };
            match crate::project_fs::Fs::find_root(&cwd) {
                Ok(Some(path)) => path,
                Ok(None) => {
                    return failure(
                        LoadStatus::Invalid,
                        vec![Diagnostic::new("structure.root-not-found")],
                    )
                }
                Err(outcome) => return structure_failure(outcome),
            }
        }
    };

    // Phase 2: accepted #4 structure validation.
    let validation = crate::project_fs::Fs::validate_project(&root);
    let report = match validation {
        crate::project_fs::StructureOutcome::Valid(report) => report,
        crate::project_fs::StructureOutcome::Invalid(reasons) => {
            return structure_failure(crate::project_fs::StructureOutcome::Invalid(reasons))
        }
        crate::project_fs::StructureOutcome::Denied(reasons) => {
            return structure_failure(crate::project_fs::StructureOutcome::Denied(reasons))
        }
    };

    let fs = match crate::project_fs::Fs::open(&root) {
        Ok(fs) => fs,
        Err(_) => {
            return failure(
                LoadStatus::Invalid,
                vec![Diagnostic::new("structure.root-unreadable")],
            )
        }
    };

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
            // Absence is legal only for the seven optional kind files.
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
        return failure(LoadStatus::Invalid, diagnostics);
    }

    // Phase 3b: encoding gate, then Phase 4: parsing.
    let mut parsed_documents: Vec<(Document, frontends::Parsed)> = Vec::new();
    let mut parse_diagnostics: Vec<Diagnostic> = Vec::new();
    for source in &read_sources {
        let text = match Source::validate_encoding(&source.logical_path, &source.bytes) {
            Ok(text) => text,
            Err(diagnostic) => {
                parse_diagnostics.push(diagnostic);
                continue;
            }
        };
        let index = LineIndex::new(&text);
        match frontends::parse_document(&text, &index) {
            Ok(parsed) => {
                let kind = sources
                    .iter()
                    .find(|(logical, _)| *logical == source.logical_path)
                    .map(|(_, kind)| *kind)
                    .unwrap_or(DocKind::Symbol);
                match decode(&source.logical_path, kind, &parsed) {
                    Ok(document) => parsed_documents.push((document, parsed)),
                    Err(errors) => parse_diagnostics.extend(errors),
                }
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
    if !parse_diagnostics.is_empty() {
        return failure(LoadStatus::Invalid, parse_diagnostics);
    }

    // Phase 5/6: version gate across all documents.
    let mut unsupported: Vec<(String, String)> = Vec::new();
    let mut versions: Vec<(String, String)> = Vec::new();
    for (document, _) in &parsed_documents {
        if ModelVersion::parse_exact(&document.version).is_none() {
            unsupported.push((document.path.clone(), document.version.clone()));
        }
        versions.push((document.path.clone(), document.version.clone()));
    }
    if !unsupported.is_empty() {
        unsupported.sort();
        return failure(
            LoadStatus::UnsupportedVersion,
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
            LoadStatus::Invalid,
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

    // Phase 7: version-dispatched decoding: collisions, imports, graph.
    let mut collision_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut project_document: Option<Document> = None;
    let mut module_vertices: Vec<imports::ModuleVertex> = Vec::new();
    let mut module_documents: Vec<Document> = Vec::new();
    let mut symbol_documents: Vec<Document> = Vec::new();

    for (document, _) in &parsed_documents {
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
        return failure(LoadStatus::Invalid, collision_diagnostics);
    }

    // Import graph: missing imports, then cycles.
    let missing = imports::missing_imports(&module_vertices);
    if !missing.is_empty() {
        return failure(LoadStatus::Invalid, missing);
    }
    let cycles = imports::import_cycles(&module_vertices);
    if !cycles.is_empty() {
        return failure(LoadStatus::Invalid, cycles);
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

    let mut source_map: Vec<SourceMapEntry> = Vec::new();
    if spans_requested {
        // Project definition entry.
        if let Some(project) = &project_document {
            source_map.push(SourceMapEntry {
                path: project.path.clone(),
                semantic_id: Some(project.definitions[0].id.clone()),
                pointer: "/project".to_owned(),
                start: Position::of(project.definitions[0].span.start),
                end: Position::of(project.definitions[0].span.end),
            });
        }
    }

    let mut normalized_modules: Vec<(String, Node)> = Vec::new();
    for (module_index, (module_id, document)) in modules.iter_mut().enumerate() {
        let mut node = document.definitions[0].node.clone();
        let mut context = normalize::NormalizeContext::new(&symbol_index, module_id);
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
            return failure(LoadStatus::Invalid, diagnostics);
        }
        if spans_requested {
            source_map.push(SourceMapEntry {
                path: document.path.clone(),
                semantic_id: Some(document.definitions[0].id.clone()),
                pointer: format!("/modules/{module_index}"),
                start: Position::of(document.definitions[0].span.start),
                end: Position::of(document.definitions[0].span.end),
            });
            for (suffix, span) in &context.source_entries {
                source_map.push(SourceMapEntry {
                    path: document.path.clone(),
                    semantic_id: None,
                    pointer: format!("/modules/{module_index}/{suffix}"),
                    start: Position::of(span.start),
                    end: Position::of(span.end),
                });
            }
        }
        normalized_modules.push((module_id.clone(), node));
    }

    let mut normalized_definitions: Vec<(String, Document, Node)> = Vec::new();
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
                return failure(LoadStatus::Invalid, diagnostics);
            }
            normalized_definitions.push((definition.id.clone(), document.clone(), node));
        }
    }
    normalized_definitions.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

    if spans_requested {
        for (position, (id, document, definition)) in normalized_definitions.iter().enumerate() {
            source_map.push(SourceMapEntry {
                path: document.path.clone(),
                semantic_id: Some(id.clone()),
                pointer: format!("/definitions/{position}"),
                start: Position::of(definition.span.start),
                end: Position::of(definition.span.end),
            });
        }
        // Type/reference crumb entries point into sorted output positions;
        // they were recorded per-definition during normalization.
        // (Crumbs for symbols were attached above via context.source_entries
        // only for modules; symbols record during the same walk below.)
    }

    // Phase 9: canonical aggregate.
    let project_canonical = project_document
        .as_ref()
        .map(|document| Canonical::from_node(&document.definitions[0].node));
    let modules_canonical: Vec<Canonical> = normalized_modules
        .iter()
        .map(|(_, node)| Canonical::from_node(node))
        .collect();
    let definitions_canonical: Vec<Canonical> = normalized_definitions
        .iter()
        .map(|(_, _, node)| Canonical::from_node(node))
        .collect();

    let mut model = String::new();
    model.push_str("{\"definitions\":");
    Canonical::Seq(definitions_canonical).write_json(&mut model);
    model.push_str(",\"modules\":");
    Canonical::Seq(modules_canonical).write_json(&mut model);
    if let Some(project) = &project_canonical {
        model.push_str(",\"project\":");
        project.write_json(&mut model);
    }
    model.push('}');

    let mut json = String::from("{\"status\":\"valid\",\"modelVersion\":");
    json.push_str(&Canonical::Str(model_version.as_str().to_owned()).to_json());
    json.push_str(",\"model\":");
    json.push_str(&model);
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
        normalized_modules.len(),
        normalized_definitions.len()
    );

    LoadOutput {
        status: LoadStatus::Valid,
        json,
        human,
    }
}

fn structure_failure(outcome: crate::project_fs::StructureOutcome) -> LoadOutput {
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
            LoadOutput::failure(LoadStatus::Invalid, diagnostics)
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
            LoadOutput::failure(LoadStatus::Denied, diagnostics)
        }
        crate::project_fs::StructureOutcome::Valid(_) => unreachable!("valid has no failure"),
    }
}
