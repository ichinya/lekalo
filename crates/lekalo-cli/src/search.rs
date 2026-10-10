//! Issue #93 CLI: the `search`, `find`, and `search-index` surfaces.
//!
//! This binary owns confined reads, input admission, index persistence,
//! and envelope rendering; `lekalo_core::search` owns every typed
//! decision. The core never launches a process, never reads a network,
//! and never walks target source: `search-index build/update` reads the
//! canonical Model documents plus explicitly admitted producer files
//! only. Exit and stream ownership stay with `DomainResult`.

use clap::{Args, Subcommand};
use lekalo_core::search::{
    add_card, attach_effects, project_ir, Card, CardKind, Completeness, Confidence, FilterSet,
    Freshness, NativeBinding, Projection, QueryRequest, SearchIndex, SearchInputs, SearchStore,
    SignatureRef,
};
use lekalo_core::DomainResult;
use std::io::Read as _;

/// The bounded `--effect-scope` values.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum EffectScopeArg {
    Exact,
    Overlapping,
}

impl EffectScopeArg {
    fn scope(self) -> lekalo_core::search::EffectScope {
        match self {
            Self::Exact => lekalo_core::search::EffectScope::Exact,
            Self::Overlapping => lekalo_core::search::EffectScope::Overlapping,
        }
    }
}

/// The shared filter surface of `search` and `find` (repeatable values
/// of one flag OR; different flags AND; `find` requires one).
#[derive(Debug, Args)]
pub struct SearchFilters {
    /// Exact semantic IDs (repeatable).
    #[arg(long = "id", value_name = "ID")]
    pub id: Vec<String>,
    /// Card kinds (repeatable): command, query, entity, scenario, ...
    #[arg(long = "kind", value_name = "KIND")]
    pub kind: Vec<String>,
    /// Canonical module IDs (repeatable).
    #[arg(long = "module", value_name = "MODULE")]
    pub module: Vec<String>,
    /// Read selectors: `entity` or `entity.field` (repeatable).
    #[arg(long = "reads", value_name = "SUBJECT")]
    pub reads: Vec<String>,
    /// Write selectors: `entity` or `entity.field` (repeatable).
    #[arg(long = "writes", value_name = "SUBJECT")]
    pub writes: Vec<String>,
    /// Direct declared invocation targets (repeatable).
    #[arg(long = "calls", value_name = "ID")]
    pub calls: Vec<String>,
    /// Requirement IDs (repeatable).
    #[arg(long = "requirement", value_name = "ID")]
    pub requirement: Vec<String>,
    /// Requirement provider namespaces (repeatable).
    #[arg(long = "requirement-source", value_name = "SOURCE")]
    pub requirement_source: Vec<String>,
    /// Binding path selectors: exact relative path or directory prefix
    /// at component boundaries (repeatable).
    #[arg(long = "binding-path", value_name = "PATH")]
    pub binding_path: Vec<String>,
    /// Exact native symbol spellings (repeatable).
    #[arg(long = "native-symbol", value_name = "SYMBOL")]
    pub native_symbol: Vec<String>,
    /// Target IDs (repeatable).
    #[arg(long = "target", value_name = "TARGET")]
    pub target: Vec<String>,
    /// Directly used type identities (repeatable).
    #[arg(long = "type", value_name = "TYPE")]
    pub r#type: Vec<String>,
    /// Input-role type identities (repeatable).
    #[arg(long = "input-type", value_name = "TYPE")]
    pub input_type: Vec<String>,
    /// Output-role type identities (repeatable).
    #[arg(long = "output-type", value_name = "TYPE")]
    pub output_type: Vec<String>,
    /// Error identities (repeatable).
    #[arg(long = "error", value_name = "ERROR")]
    pub error: Vec<String>,
    /// Direct incident relation kinds; requires `--related-to`.
    #[arg(long = "relation", value_name = "KIND")]
    pub relation: Vec<String>,
    /// The opposite side of `--relation` (repeatable).
    #[arg(long = "related-to", value_name = "ID")]
    pub related_to: Vec<String>,
    /// Ownership states (`module` | `project`) (repeatable).
    #[arg(long = "ownership", value_name = "STATE")]
    pub ownership: Vec<String>,
    /// Portability states (repeatable).
    #[arg(long = "portability", value_name = "STATE")]
    pub portability: Vec<String>,
    /// Profile ids (repeatable).
    #[arg(long = "profile", value_name = "PROFILE")]
    pub profile: Vec<String>,
    /// Exact registered diagnostic rule IDs (repeatable).
    #[arg(long = "diagnostic", value_name = "RULE")]
    pub diagnostic: Vec<String>,
    /// Qualified waiver IDs (repeatable).
    #[arg(long = "waiver", value_name = "ID")]
    pub waiver: Vec<String>,
    /// Add visibly stale rows (never excluded/sensitive rows).
    #[arg(long = "include-stale")]
    pub include_stale: bool,
    /// The effect scope of `--reads`/`--writes` (default exact).
    #[arg(long = "effect-scope", value_enum, value_name = "SCOPE")]
    pub effect_scope: Option<EffectScopeArg>,
}

impl SearchFilters {
    /// The typed exact filter set.
    pub fn filters(&self) -> FilterSet {
        FilterSet {
            ids: self.id.clone(),
            kinds: self.kind.clone(),
            modules: self.module.clone(),
            reads: self.reads.clone(),
            writes: self.writes.clone(),
            calls: self.calls.clone(),
            requirements: self.requirement.clone(),
            requirement_sources: self.requirement_source.clone(),
            binding_paths: self.binding_path.clone(),
            native_symbols: self.native_symbol.clone(),
            targets: self.target.clone(),
            types: self.r#type.clone(),
            input_types: self.input_type.clone(),
            output_types: self.output_type.clone(),
            errors: self.error.clone(),
            relations: self.relation.clone(),
            related_to: self.related_to.clone(),
            ownerships: self.ownership.clone(),
            portabilities: self.portability.clone(),
            profiles: self.profile.clone(),
            diagnostics: self.diagnostic.clone(),
            waivers: self.waiver.clone(),
            include_stale: self.include_stale,
            effect_scope: self
                .effect_scope
                .map(|scope| scope.scope())
                .unwrap_or_default(),
            effect_scope_present: self.effect_scope.is_some(),
        }
    }
}

/// The shared page/project/input arguments.
#[derive(Debug, Args)]
pub struct PageArgs {
    /// Page size (1..=100, default 20).
    #[arg(long, value_name = "N")]
    pub limit: Option<usize>,
    /// Opaque continuation token of the previous page.
    #[arg(long, value_name = "TOKEN")]
    pub cursor: Option<String>,
    /// Project root selector, relative to the invocation directory.
    #[arg(long, value_name = "DIR")]
    pub project: Option<String>,
    /// The explicit input manifest of admitted typed producers.
    #[arg(long = "inputs", value_name = "FILE")]
    pub inputs: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum SearchIndexCommands {
    /// Build and publish one index generation from the admitted inputs.
    Build {
        #[command(flatten)]
        page: PageArgs,
    },
    /// Incrementally recompute the index from the admitted inputs.
    Update {
        #[command(flatten)]
        page: PageArgs,
    },
    /// The read-only status of the durable index store.
    Status {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
}

/// The admitted service context of one run: the loaded projection, its
/// immutable content digest, the per-domain admitted coverage, the
/// admitted input pins, and the run's security/policy digest.
struct Service {
    projection: Projection,
    project_id: Option<String>,
    coverage: Vec<lekalo_core::search::CoverageRow>,
    pins: Vec<lekalo_core::search::InputPin>,
    policy_digest: String,
    /// The posting cap this run admitted (the manifest's own bounded
    /// selection when a manifest was given, the global work cap
    /// otherwise). Limits propagate; they are never re-derived.
    cap: usize,
    /// Producer rows the projection could not attach (counted, never
    /// silent): surfaced as a `search.result-limit` warning.
    dropped_rows: u64,
}

/// Run one `search` command.
pub fn run_search(
    query: String,
    fuzzy: bool,
    page: &PageArgs,
    filters: &SearchFilters,
    no_cache: bool,
) -> DomainResult {
    let request = match QueryRequest::admit(
        Some(query),
        fuzzy,
        page.limit,
        page.cursor.clone(),
        filters.filters(),
    ) {
        Ok(request) => request,
        Err(set) => return DomainResult::invalid(set),
    };
    run_query(&request, page, no_cache)
}

/// Run one `find` command.
pub fn run_find(page: &PageArgs, filters: &SearchFilters, no_cache: bool) -> DomainResult {
    let request = match QueryRequest::admit(
        None,
        false,
        page.limit,
        page.cursor.clone(),
        filters.filters(),
    ) {
        Ok(request) => request,
        Err(set) => return DomainResult::invalid(set),
    };
    run_query(&request, page, no_cache)
}

/// The shared query path: load, admit, execute, and render.
fn run_query(request: &QueryRequest, page: &PageArgs, no_cache: bool) -> DomainResult {
    // A cursor binds an immutable persisted generation; under the
    // durable-store bypass no such generation exists, so the pair is
    // refused before any load (the research contract is verbatim).
    if no_cache && request.cursor.is_some() {
        return DomainResult::invalid(lekalo_core::search::diagnostic_query_invalid(
            "cursor",
            "no-cache-bypass",
        ));
    }
    let service = match assemble(page, no_cache) {
        Ok(service) => service,
        Err(result) => return result,
    };
    // A cursor page serves the validated durable generation, never a
    // recomputation: the token binds the persisted content digest, so
    // the persisted generation must exist, validate, and still match
    // the run's admitted policy (pins and coverage). A missing,
    // corrupt, or policy-drifted generation is a typed cursor refusal,
    // never a silent page from different bytes.
    let durable = if request.cursor.is_some() {
        let selection = lekalo_core::loader::LoadSelection {
            project: page
                .project
                .clone()
                .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
        };
        let root = match lekalo_core::doctor::project_root(&selection) {
            Ok(root) => root,
            Err(result) => return result,
        };
        let store = match SearchStore::resolve(&root) {
            Ok(store) => store,
            Err(_) => {
                return DomainResult::invalid(lekalo_core::search::diagnostic::cursor_invalid(
                    "generation-missing",
                ))
            }
        };
        match store
            .current()
            .ok()
            .flatten()
            .map(|digest| store.load(&digest))
        {
            Some(Ok(Some(document))) => Some(document),
            _ => {
                return DomainResult::invalid(lekalo_core::search::diagnostic::cursor_invalid(
                    "generation-missing",
                ))
            }
        }
    } else {
        None
    };
    let index = match &durable {
        Some(document) => SearchIndex {
            cards: document.cards.clone(),
            postings: document.postings.clone(),
            qualified_ids: document.qualified_ids.clone(),
        },
        None => {
            match lekalo_core::search::SearchIndex::from_projection(
                &service.projection,
                &service.pins,
                &service.policy_digest,
                service.project_id.as_deref(),
                service.cap,
            ) {
                Ok(index) => index,
                Err(set) => return DomainResult::invalid(set),
            }
        }
    };
    let document = snapshot_document(&service);
    let index_digest = match &durable {
        Some(durable) => durable.digest.clone(),
        None => format!("sha256:{}", lekalo_core::search::digest_of(&document)),
    };
    // The durable page must also match the run's admitted policy: pins
    // and coverage are bound into the policy digest, so replaying a
    // cursor with changed admitted inputs is a generation mismatch.
    if let Some(durable) = &durable {
        if durable.policy_digest != service.policy_digest {
            return DomainResult::invalid(lekalo_core::search::diagnostic::cursor_invalid(
                "generation-mismatch",
            ));
        }
    }
    // Cold queries publish their generation best-effort (cache-enabled
    // runs only): a cursor issued on the next page must always have a
    // persisted generation to bind. A read-only or denied cache home
    // degrades the page, never fails the query.
    if !no_cache && request.cursor.is_none() {
        let selection = lekalo_core::loader::LoadSelection {
            project: page
                .project
                .clone()
                .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
        };
        if let Ok(root) = lekalo_core::doctor::project_root(&selection) {
            if let Ok(store) = SearchStore::resolve(&root) {
                let mut publish = document.clone();
                publish.digest = index_digest.clone();
                let _ = store.publish(&publish);
            }
        }
    }
    // Coverage honesty: a needed domain that is not complete is warned
    // about once per domain so absence is never read as proof.
    let mut diagnostics: Vec<lekalo_core::diagnostics::Diagnostic> = Vec::new();
    for row in &service.coverage {
        if row.state != "complete" && lekalo_core::search::domain_needed(&row.domain, request) {
            diagnostics.extend(
                lekalo_core::search::emit_coverage_incomplete(&row.domain, row.state.as_str())
                    .as_slice()
                    .to_vec(),
            );
        }
    }
    if service.dropped_rows > 0 {
        diagnostics.extend(
            lekalo_core::search::emit_dropped_rows(service.dropped_rows)
                .as_slice()
                .to_vec(),
        );
    }
    let outcome = match lekalo_core::search::execute(
        &index,
        &index_digest,
        &service.policy_digest,
        service.project_id.as_deref(),
        request,
        service.coverage.clone(),
    ) {
        Ok(outcome) => outcome,
        Err(set) => return DomainResult::invalid(set),
    };
    diagnostics.extend(outcome.diagnostics);
    DomainResult::graph(outcome.json, outcome.human, diagnostics)
}

/// The durable snapshot document of one admitted run: the exact cards,
/// postings, qualified identities, input pins, and policy digest.
fn snapshot_document(service: &Service) -> lekalo_core::search::IndexDocument {
    let index = lekalo_core::search::SearchIndex::from_projection(
        &service.projection,
        &service.pins,
        &service.policy_digest,
        service.project_id.as_deref(),
        service.cap,
    );
    let (cards, postings, qualified_ids) = match index {
        Ok(index) => (index.cards, index.postings, index.qualified_ids),
        Err(_) => (Vec::new(), Default::default(), Default::default()),
    };
    lekalo_core::search::IndexDocument {
        schema_version: lekalo_core::search::INDEX_SCHEMA_VERSION.to_owned(),
        identity: lekalo_core::search::INDEX_IDENTITY.to_owned(),
        recipe: lekalo_core::search::RECIPE.to_owned(),
        project_id: service.project_id.clone(),
        digest: String::new(),
        inputs: service.pins.clone(),
        policy_digest: service.policy_digest.clone(),
        cards,
        postings,
        qualified_ids,
    }
}

impl Service {
    /// The admitted policy digest: the coverage facts plus the input
    /// pins of this run. Policy tightening (a different admitted
    /// coverage or pin set) rebinds every cursor coherently and is
    /// distinct from the index content digest.
    fn compute_policy_digest(&self) -> String {
        let mut input = String::from("search-policy/1\u{0}");
        for row in &self.coverage {
            input.push_str(&format!("{}={}\u{0}", row.domain, row.state));
        }
        for pin in &self.pins {
            input.push_str(&format!(
                "{}:{}:{}\u{0}",
                pin.kind,
                pin.path,
                pin.digest.as_deref().unwrap_or("")
            ));
        }
        format!(
            "sha256:{}",
            lekalo_core::search::sha256_of(input.as_bytes())
        )
    }
}

/// Load the compiled project and build the admitted projection.
///
/// The canonical Model/IR always participates; every producer beyond it
/// arrives through the explicitly admitted input manifest, parsed by
/// its own published decoder. Target source is never read.
///
/// Custody: the manifest resolves against the selected project root
/// (never the process working directory), and both the manifest and
/// every producer read is confined under the canonical root and bounded
/// before the buffer is allocated.
fn assemble(page: &PageArgs, no_cache: bool) -> Result<Service, DomainResult> {
    let selection = lekalo_core::loader::LoadSelection {
        project: page
            .project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let root =
        lekalo_core::doctor::project_root(&selection).map_err(|result: DomainResult| result)?;
    let (_model, compilation) = lekalo_core::cache::load_compiled(&selection, no_cache)?;
    let graph = lekalo_core::effects::build(&compilation.project).map_err(DomainResult::invalid)?;
    let cap = lekalo_core::search::MAX_POSTINGS;
    let mut projection = project_ir(&compilation.project, cap);
    attach_effects(&mut projection, &graph, cap);
    // Every producer domain starts `unknown`: absence of admitted
    // evidence is never silently read as proof of completeness (or of
    // absence). Admitted producers overwrite their own domain's row.
    let mut coverage: Vec<lekalo_core::search::CoverageRow> = vec![
        coverage_row("ir", "complete"),
        coverage_row("effects", "complete"),
        coverage_row("trace", "unknown"),
        coverage_row("requirements", "unknown"),
        coverage_row("bindings", "unknown"),
        coverage_row("diagnostics", "unknown"),
        coverage_row("waivers", "unknown"),
    ];
    let mut pins: Vec<lekalo_core::search::InputPin> = Vec::new();
    let mut dropped_rows: u64 = 0;
    let manifest_cap;
    match &page.inputs {
        Some(path) => {
            let manifest_path = resolve_manifest_path(&root, path)?;
            let bytes = read_bounded(&manifest_path).map_err(|detail| {
                DomainResult::invalid(lekalo_core::search::diagnostic_query_invalid(
                    "inputs", detail,
                ))
            })?;
            let manifest = SearchInputs::parse(&bytes).map_err(|error| match error {
                lekalo_core::search::input::ManifestError::Version(set) => {
                    DomainResult::unsupported_version(set)
                }
                lekalo_core::search::input::ManifestError::Invalid(set) => {
                    DomainResult::invalid(set)
                }
            })?;
            let project_id = compilation
                .project
                .project
                .as_ref()
                .map(|project| project.id.as_str().to_owned());
            if let Some(project) = &project_id {
                if manifest.project_id() != project.as_str() {
                    return Err(DomainResult::invalid(
                        lekalo_core::search::diagnostic_input_invalid("project-mismatch", None),
                    ));
                }
            }
            // Admission is the single custody barrier: every producer
            // byte is read exactly once, confined and bounded, verified
            // against its pins, and decoded by its published decoder.
            // The consumed pins ride the snapshot and the policy digest
            // so a generation can prove what it was built from.
            let admitted = admit_producers(
                &root,
                project_id.as_deref(),
                &manifest,
                &compilation.project,
                &mut projection,
                &mut coverage,
            )?;
            pins = admitted.pins;
            dropped_rows = admitted.dropped_rows;
            manifest_cap = manifest.max_postings();
        }
        None => {
            manifest_cap = cap;
        }
    }
    let run_cap = manifest_cap.min(lekalo_core::search::MAX_POSTINGS);
    let service = Service {
        projection,
        project_id: compilation
            .project
            .project
            .as_ref()
            .map(|project| project.id.as_str().to_owned()),
        coverage,
        policy_digest: String::new(),
        pins,
        cap: run_cap,
        dropped_rows,
    };
    Ok(Service {
        policy_digest: service.compute_policy_digest(),
        ..service
    })
}

/// Resolve one caller-supplied manifest spelling against the selected
/// project root: a relative `--inputs` never silently falls back to the
/// process working directory, and the resolved file must live inside
/// the canonical root (no foreign manifests, no aliases).
fn resolve_manifest_path(
    root: &std::path::Path,
    inputs: &str,
) -> Result<std::path::PathBuf, DomainResult> {
    let candidate = std::path::Path::new(inputs);
    let path = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let canonical_root = std::fs::canonicalize(root).map_err(|_| {
        DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
            "root-unresolvable",
            None,
        ))
    })?;
    let contained = std::fs::canonicalize(&path)
        .ok()
        .is_some_and(|resolved| resolved.starts_with(&canonical_root));
    if !contained {
        return Err(DomainResult::invalid(
            lekalo_core::search::diagnostic_input_invalid("path-confinement", Some("inputs")),
        ));
    }
    Ok(path)
}

/// One bounded file read: the cap applies before the buffer is
/// allocated (`take`), never after a full read.
fn read_bounded(path: &std::path::Path) -> Result<Vec<u8>, &'static str> {
    let file = std::fs::File::open(path).map_err(|_| "io")?;
    let limit = lekalo_core::search::MAX_INPUT_BYTES;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "io")?;
    if bytes.len() > limit {
        return Err("input-bytes");
    }
    Ok(bytes)
}

/// One confined bounded read under the canonical project root: the
/// resolved path (junctions and symlinks included) must stay inside the
/// root, and the read is bounded before allocation. Every producer byte
/// the search surface consumes passes through exactly this gate.
fn read_confined(
    canonical_root: &std::path::Path,
    path: &std::path::Path,
    producer: &'static str,
) -> Result<Vec<u8>, DomainResult> {
    let contained = std::fs::canonicalize(path)
        .ok()
        .is_some_and(|resolved| resolved.starts_with(canonical_root));
    if !contained {
        return Err(DomainResult::invalid(
            lekalo_core::search::diagnostic_input_invalid("path-confinement", Some(producer)),
        ));
    }
    read_bounded(path).map_err(|detail| {
        DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
            if detail == "input-bytes" {
                "producer-bytes"
            } else {
                "producer-unreadable"
            },
            Some(producer),
        ))
    })
}

fn coverage_row(domain: &'static str, state: &'static str) -> lekalo_core::search::CoverageRow {
    lekalo_core::search::CoverageRow {
        domain: domain.to_owned(),
        state: state.to_owned(),
    }
}

/// A producer document's project binding: refused with a bounded
/// producer diagnostic when it names a different project (custody: no
/// foreign evidence ever joins this project's view).
fn check_producer_project(declared: &str, project: &str) -> Result<(), DomainResult> {
    if declared != project {
        return Err(DomainResult::invalid(
            lekalo_core::search::diagnostic_input_invalid("project-mismatch", None),
        ));
    }
    Ok(())
}

/// Decode one producer document as exact UTF-8 JSON with duplicate
/// keys rejected before any decode (the closed-document convention of
/// every admitted search byte source).
fn producer_json(bytes: &[u8], producer: &str) -> Result<serde_json::Value, DomainResult> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
            "invalid-encoding",
            Some(producer),
        ))
    })?;
    lekalo_core::search::reject_duplicate_keys(text).map_err(|_| {
        DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
            "duplicate-key",
            Some(producer),
        ))
    })?;
    serde_json::from_str(text).map_err(|_| {
        DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
            "producer-malformed",
            Some(producer),
        ))
    })
}

/// The fully decoded effects producer envelope: the exact published
/// contract identity, its declared project, and every row decoded
/// through the closed published grammar.
struct EffectsEnvelope {
    project: String,
    rows: Vec<EffectRow>,
}

/// One closed-decoded effect row (the published `effect` grammar of
/// `lekalo/effects/v0.2.16`, member for member).
struct EffectRow {
    operation: String,
    kind: lekalo_core::effects::EffectKind,
    resource_id: String,
    resource_kind: lekalo_core::effects::ResourceKind,
    field: Option<String>,
    /// Validated against the closed ordinal grammar at decode; the
    /// projection itself keys on the operation and subject.
    #[allow(dead_code)]
    occurrence: u32,
    /// The evidence provenance, when the row carries one.
    evidence: Option<EvidenceProvenance>,
    /// True when the row's provenance is the canonical-ir shape.
    canonical_ir: bool,
}

/// The closed evidence provenance of one row.
struct EvidenceProvenance {
    trust: lekalo_core::effects::TrustState,
}

/// Run the effects producer document through its published typed
/// decoder (`lekalo/effects/v0.2.16`): the closed envelope grammar
/// (required members, no unknown members at any depth, closed enum
/// vocabularies, per-member bounds) and the envelope cross-checks
/// (metadata counts, project) are enforced before any row is projected.
/// A schema-invalid document is refused, never silently repaired or
/// partially skipped.
fn decode_effects_envelope(value: &serde_json::Value) -> Result<EffectsEnvelope, DomainResult> {
    const EFFECTS_IDENTITY: &str = lekalo_core::effects::version::IDENTITY;
    const EFFECTS_SCHEMA: &str = lekalo_core::effects::version::SCHEMA_VERSION;
    let Some(map) = value.as_object() else {
        return Err(producer_invalid("producer-malformed", "effects"));
    };
    const ENVELOPE_MEMBERS: [&str; 6] = [
        "schemaVersion",
        "identity",
        "modelVersion",
        "project",
        "metadata",
        "effects",
    ];
    for key in map.keys() {
        if !ENVELOPE_MEMBERS.contains(&key.as_str()) {
            return Err(producer_invalid("producer-shape", "effects"));
        }
    }
    for required in [
        "schemaVersion",
        "identity",
        "modelVersion",
        "metadata",
        "effects",
    ] {
        if !map.contains_key(required) {
            return Err(producer_invalid("producer-shape", "effects"));
        }
    }
    let identity = map["identity"].as_str().unwrap_or_default();
    let schema_version = map["schemaVersion"].as_str().unwrap_or_default();
    if identity != EFFECTS_IDENTITY || schema_version != EFFECTS_SCHEMA {
        return Err(DomainResult::invalid(
            lekalo_core::search::diagnostic_input_invalid("identity-mismatch", Some("effects")),
        ));
    }
    if !is_semver(map["modelVersion"].as_str().unwrap_or_default()) {
        return Err(producer_invalid("producer-shape", "effects"));
    }
    // The metadata object is closed and all five counts are required.
    const METADATA_MEMBERS: [&str; 5] = [
        "declaredCount",
        "detectedCount",
        "effectCount",
        "envelopeCount",
        "operationCount",
    ];
    let metadata = map["metadata"]
        .as_object()
        .ok_or_else(|| producer_invalid("producer-shape", "effects"))?;
    for key in metadata.keys() {
        if !METADATA_MEMBERS.contains(&key.as_str()) {
            return Err(producer_invalid("producer-shape", "effects"));
        }
    }
    for member in METADATA_MEMBERS {
        if metadata
            .get(member)
            .and_then(|count| count.as_u64())
            .is_none()
        {
            return Err(producer_invalid("producer-shape", "effects"));
        }
    }
    let rows_value = map["effects"]
        .as_array()
        .ok_or_else(|| producer_invalid("producer-shape", "effects"))?;
    let mut rows = Vec::with_capacity(rows_value.len());
    let mut detected = 0usize;
    let mut declared = 0usize;
    for row in rows_value {
        let decoded = decode_effects_row(row)?;
        if decoded.canonical_ir {
            declared += 1;
        } else {
            detected += 1;
        }
        rows.push(decoded);
    }
    // Envelope cross-check: the document's own metadata must describe
    // exactly the rows it carries and the typed decoder must accept
    // exactly those rows (a spliced/edited snapshot refuses).
    let count = |member: &str| metadata[member].as_u64().unwrap_or(u64::MAX);
    if count("declaredCount") != declared as u64
        || count("detectedCount") != detected as u64
        || count("effectCount") != rows.len() as u64
    {
        return Err(producer_invalid("producer-shape", "effects"));
    }
    // The declared rows are rebuilt from the canonical IR at admission,
    // so only the producer's own detected rows are custody-checked here;
    // a snapshot that also carries declared rows is a foreign shape.
    if declared > 0 {
        return Err(producer_invalid("producer-shape", "effects"));
    }
    let project = map["project"]
        .as_str()
        .ok_or_else(|| producer_invalid("producer-shape", "effects"))?;
    Ok(EffectsEnvelope {
        project: project.to_owned(),
        rows,
    })
}

/// Decode one effect row through the closed published grammar: required
/// members present, no unknown members, closed enum vocabularies, and
/// per-member bounds. Any deviation is a typed refusal.
fn decode_effects_row(row: &serde_json::Value) -> Result<EffectRow, DomainResult> {
    const ROW_MEMBERS: [&str; 11] = [
        "operation",
        "kind",
        "action",
        "resource",
        "field",
        "effect",
        "occurrence",
        "provenance",
        "confidence",
        "transactionGroup",
        "sensitivity",
    ];
    let Some(map) = row.as_object() else {
        return Err(producer_invalid("producer-malformed", "effects"));
    };
    for key in map.keys() {
        if !ROW_MEMBERS.contains(&key.as_str()) {
            return Err(producer_invalid("producer-malformed", "effects"));
        }
    }
    for required in ROW_MEMBERS {
        if !map.contains_key(required) {
            return Err(producer_invalid("producer-malformed", "effects"));
        }
    }
    let operation = map["operation"]
        .as_str()
        .and_then(lekalo_core::effects::OperationId::from_qualified)
        .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
    let kind = map["kind"]
        .as_str()
        .and_then(effects_kind_from_key)
        .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
    // The closed write-action vocabulary; present only when a string.
    if let Some(action) = map["action"].as_str() {
        if lekalo_core::effects::WriteAction::from_key(action).is_none() {
            return Err(producer_invalid("producer-malformed", "effects"));
        }
    } else if !map["action"].is_null() {
        return Err(producer_invalid("producer-malformed", "effects"));
    }
    let resource = map["resource"]
        .as_object()
        .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
    for key in resource.keys() {
        if !matches!(key.as_str(), "kind" | "id") {
            return Err(producer_invalid("producer-malformed", "effects"));
        }
    }
    let resource_kind = resource["kind"]
        .as_str()
        .and_then(effects_resource_kind_from_key)
        .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
    let resource_id = resource["id"]
        .as_str()
        .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
    if resource_id.is_empty() || resource_id.len() > 192 {
        return Err(producer_invalid("producer-malformed", "effects"));
    }
    let field = match map["field"].as_str() {
        Some(field) => {
            let valid = (1..=64).contains(&field.len())
                && field.starts_with(|first: char| first.is_ascii_lowercase())
                && field.chars().all(|character| {
                    character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || matches!(character, '_' | '-')
                });
            if !valid {
                return Err(producer_invalid("producer-malformed", "effects"));
            }
            Some(field.to_owned())
        }
        None if map["field"].is_null() => None,
        None => return Err(producer_invalid("producer-malformed", "effects")),
    };
    // The closed effect-origin grammar: `effect:<id>` or `#<ordinal>`.
    let effect = map["effect"]
        .as_str()
        .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
    let effect_origin = effect.strip_prefix("effect:").is_some_and(|rest| {
        !rest.is_empty()
            && rest.starts_with(|first: char| first.is_ascii_alphanumeric())
            && rest.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | ':' | '-')
            })
    }) || (effect.starts_with('#')
        && effect.len() > 1
        && effect[1..].bytes().all(|byte| byte.is_ascii_digit()));
    if !effect_origin {
        return Err(producer_invalid("producer-malformed", "effects"));
    }
    let occurrence = map["occurrence"]
        .as_u64()
        .filter(|occurrence| *occurrence <= u32::MAX as u64)
        .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?
        as u32;
    // The closed confidence vocabulary.
    if !matches!(
        map["confidence"].as_str(),
        Some("canonical" | "verified" | "extracted" | "inferred" | "unknown")
    ) {
        return Err(producer_invalid("producer-malformed", "effects"));
    }
    // transactionGroup: null or the namespaced spelling.
    if let Some(group) = map["transactionGroup"].as_str() {
        let valid = (3..=128).contains(&group.len())
            && group.split('/').count() == 2
            && group.split('/').all(|segment| {
                segment.starts_with(|first: char| first.is_ascii_lowercase())
                    && !segment.is_empty()
                    && segment.chars().all(|character| {
                        character.is_ascii_lowercase()
                            || character.is_ascii_digit()
                            || character == '-'
                    })
            });
        if !valid {
            return Err(producer_invalid("producer-malformed", "effects"));
        }
    } else if !map["transactionGroup"].is_null() {
        return Err(producer_invalid("producer-malformed", "effects"));
    }
    // sensitivity: null or the closed classification shape.
    if !map["sensitivity"].is_null() {
        let Some(sensitivity) = map["sensitivity"].as_object() else {
            return Err(producer_invalid("producer-malformed", "effects"));
        };
        if sensitivity.len() != 2
            || sensitivity
                .get("contract")
                .and_then(|value| value.as_str())
                .is_none()
            || !matches!(
                sensitivity.get("state").and_then(|value| value.as_str()),
                Some("classified" | "unclassified")
            )
        {
            return Err(producer_invalid("producer-malformed", "effects"));
        }
    }
    // The closed provenance pair: canonical-ir or evidence, each with
    // its own required member set and no others. Any other provenance
    // type is a refusal, never a skipped row.
    let provenance = map["provenance"]
        .as_object()
        .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
    let provenance_type = provenance
        .get("type")
        .and_then(|value| value.as_str())
        .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
    let mut evidence = None;
    let mut canonical_ir = false;
    match provenance_type {
        "canonical-ir" => {
            if provenance.len() != 5
                || !provenance.contains_key("irDigest")
                || !provenance.contains_key("role")
                || !provenance.contains_key("occurrence")
                || !provenance.contains_key("symbol")
                || !matches!(
                    provenance.get("role").and_then(|value| value.as_str()),
                    Some("query-reads" | "command-effect" | "effect-emits")
                )
                || !lekalo_core::search::is_sha256_digest(
                    provenance
                        .get("irDigest")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default(),
                )
            {
                return Err(producer_invalid("producer-malformed", "effects"));
            }
            canonical_ir = true;
        }
        "evidence" => {
            if provenance.len() != 6 {
                return Err(producer_invalid("producer-malformed", "effects"));
            }
            for (field, valid) in [
                (
                    "adapterId",
                    is_namespaced_adapter_id(
                        provenance
                            .get("adapterId")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default(),
                    ),
                ),
                (
                    "targetId",
                    is_namespaced_adapter_id(
                        provenance
                            .get("targetId")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default(),
                    ),
                ),
                (
                    "protocolVersion",
                    is_semver(
                        provenance
                            .get("protocolVersion")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default(),
                    ),
                ),
                (
                    "evidenceDigest",
                    lekalo_core::search::is_sha256_digest(
                        provenance
                            .get("evidenceDigest")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default(),
                    ),
                ),
            ] {
                if !valid {
                    return Err(producer_invalid("producer-malformed", "effects"));
                }
                let _ = field;
            }
            let trust = provenance
                .get("trust")
                .and_then(|value| value.as_str())
                .and_then(lekalo_core::effects::TrustState::from_key)
                .filter(|trust| *trust != lekalo_core::effects::TrustState::Rejected)
                .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
            evidence = Some(EvidenceProvenance { trust });
        }
        _ => return Err(producer_invalid("producer-malformed", "effects")),
    }
    Ok(EffectRow {
        operation: operation.semantic_id().to_owned(),
        kind,
        resource_id: resource_id.to_owned(),
        resource_kind,
        field,
        occurrence,
        evidence,
        canonical_ir,
    })
}

/// The namespaced adapter/target id grammar (`ns…/name`, at most 64
/// bytes) — the same shape the #14 decoder requires.
fn is_namespaced_adapter_id(text: &str) -> bool {
    if text.len() > 64 {
        return false;
    }
    match text.split_once('/') {
        Some((namespace, name)) => {
            !namespace.is_empty()
                && !name.is_empty()
                && namespace
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'.')
                && name
                    .bytes()
                    .next()
                    .is_some_and(|first| first.is_ascii_lowercase())
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        }
        None => false,
    }
}

/// Exact `major.minor.patch` without build metadata.
fn is_semver(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && text.len() <= 32
}

/// The closed effect-kind key lookup (the wire spelling is action-free).
fn effects_kind_from_key(key: &str) -> Option<lekalo_core::effects::EffectKind> {
    use lekalo_core::effects::EffectKind as Kind;
    use lekalo_core::effects::WriteAction;
    Some(match key {
        "read" => Kind::Read,
        "create" => Kind::Create,
        "update" => Kind::Update,
        "delete" => Kind::Delete,
        "write-field" => Kind::WriteField {
            action: WriteAction::Set,
        },
        "emit-event" => Kind::EmitEvent,
        "enqueue-job" => Kind::EnqueueJob,
        "external-call" => Kind::ExternalCall,
        "cache-read" => Kind::CacheRead,
        "cache-write" => Kind::CacheWrite,
        "cache-invalidate" => Kind::CacheInvalidate,
        "publish-output" => Kind::PublishOutput,
        "audit-log" => Kind::AuditLog,
        "transaction-boundary" => Kind::TransactionBoundary,
        _ => return None,
    })
}

/// The closed resource-kind key lookup.
fn effects_resource_kind_from_key(key: &str) -> Option<lekalo_core::effects::ResourceKind> {
    use lekalo_core::effects::ResourceKind as Kind;
    Some(match key {
        "canonical" => Kind::Canonical,
        "target-resource" => Kind::TargetResource,
        "external-service" => Kind::ExternalService,
        "cache" => Kind::Cache,
        "event" => Kind::Event,
        "job" => Kind::Job,
        "output" => Kind::Output,
        "audit" => Kind::Audit,
        _ => return None,
    })
}

/// Run the waivers producer document through its published typed
/// decoder: the governance store parser rejects duplicate keys, closed
/// shapes, registry pins, approval bindings, and lineage conflicts
/// before any waiver card is projected.
fn decode_waivers_store(
    _bytes: &[u8],
    value: &serde_json::Value,
) -> Result<lekalo_core::waivers::Store, DomainResult> {
    if value.get("schemaVersion").and_then(|value| value.as_str())
        != Some("lekalo/ai-lint-waivers/v0.6.5")
    {
        return Err(producer_invalid("identity-mismatch", "waivers"));
    }
    let store: lekalo_core::waivers::Store = lekalo_core::waivers::parse(_bytes)
        .map_err(|_| producer_invalid("producer-malformed", "waivers"))?;
    lekalo_core::waivers::validate_store(&store)
        .map_err(|_| producer_invalid("producer-shape", "waivers"))?;
    Ok(store)
}

/// The typed rows of the closed search diagnostic-set snapshot
/// (`lekalo/search-diagnostics/v0.8.0`).
struct DiagnosticsEnvelope {
    project: String,
    findings: Vec<DiagnosticFinding>,
}

/// One closed-decoded admitted diagnostic finding.
struct DiagnosticFinding {
    id: String,
    rule: String,
    subject: String,
    severity: String,
    disposition: String,
}

/// Decode one diagnostic-set snapshot through its closed grammar:
/// required members, no unknown members, closed severities and
/// dispositions, per-member bounds, and the envelope count cross-check.
/// Any deviation is a typed refusal, never a skipped finding.
fn decode_diagnostics_envelope(
    value: &serde_json::Value,
) -> Result<DiagnosticsEnvelope, DomainResult> {
    const IDENTITY: &str = "dev.lekalo.search-diagnostics@0.8.0";
    const SCHEMA: &str = "lekalo/search-diagnostics/v0.8.0";
    let Some(map) = value.as_object() else {
        return Err(producer_invalid("producer-malformed", "diagnostics"));
    };
    for key in map.keys() {
        if !matches!(
            key.as_str(),
            "schemaVersion" | "identity" | "project" | "metadata" | "findings"
        ) {
            return Err(producer_invalid("producer-shape", "diagnostics"));
        }
    }
    for required in [
        "schemaVersion",
        "identity",
        "project",
        "metadata",
        "findings",
    ] {
        if !map.contains_key(required) {
            return Err(producer_invalid("producer-shape", "diagnostics"));
        }
    }
    if map["schemaVersion"].as_str() != Some(SCHEMA) || map["identity"].as_str() != Some(IDENTITY) {
        return Err(DomainResult::invalid(
            lekalo_core::search::diagnostic_input_invalid("identity-mismatch", Some("diagnostics")),
        ));
    }
    let project = map["project"]
        .as_str()
        .ok_or_else(|| producer_invalid("producer-shape", "diagnostics"))?;
    let metadata = map["metadata"]
        .as_object()
        .ok_or_else(|| producer_invalid("producer-shape", "diagnostics"))?;
    if metadata.len() != 1
        || metadata
            .get("findingCount")
            .and_then(|count| count.as_u64())
            .is_none()
    {
        return Err(producer_invalid("producer-shape", "diagnostics"));
    }
    let rows = map["findings"]
        .as_array()
        .ok_or_else(|| producer_invalid("producer-shape", "diagnostics"))?;
    let mut findings = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(row) = row.as_object() else {
            return Err(producer_invalid("producer-malformed", "diagnostics"));
        };
        for key in row.keys() {
            if !matches!(
                key.as_str(),
                "id" | "rule" | "subject" | "severity" | "disposition"
            ) {
                return Err(producer_invalid("producer-malformed", "diagnostics"));
            }
        }
        for required in ["id", "rule", "subject", "severity", "disposition"] {
            if !row.contains_key(required) {
                return Err(producer_invalid("producer-malformed", "diagnostics"));
            }
        }
        let id = row["id"].as_str().unwrap_or_default();
        let bounded_id = !id.is_empty()
            && id.len() <= 64
            && id.starts_with(|first: char| first.is_ascii_alphanumeric())
            && id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
            });
        if !bounded_id {
            return Err(producer_invalid("producer-malformed", "diagnostics"));
        }
        let rule = row["rule"].as_str().unwrap_or_default();
        let bounded_rule = (3..=128).contains(&rule.len())
            && rule.split('.').count() >= 2
            && rule.split('.').all(|segment| {
                segment.starts_with(|first: char| first.is_ascii_lowercase())
                    && !segment.is_empty()
                    && segment.len() <= 64
                    && segment.chars().all(|character| {
                        character.is_ascii_lowercase()
                            || character.is_ascii_digit()
                            || character == '-'
                    })
            });
        if !bounded_rule {
            return Err(producer_invalid("producer-malformed", "diagnostics"));
        }
        let subject = row["subject"].as_str().unwrap_or_default();
        let bounded_subject = !subject.is_empty()
            && subject.len() <= 192
            && subject.starts_with(|first: char| first.is_ascii_lowercase())
            && subject.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || matches!(character, '_' | '.')
            });
        if !bounded_subject {
            return Err(producer_invalid("producer-malformed", "diagnostics"));
        }
        let severity = row["severity"].as_str().unwrap_or_default();
        if !matches!(severity, "error" | "warning" | "info") {
            return Err(producer_invalid("producer-malformed", "diagnostics"));
        }
        let disposition = row["disposition"].as_str().unwrap_or_default();
        if !matches!(disposition, "active" | "waived" | "resolved") {
            return Err(producer_invalid("producer-malformed", "diagnostics"));
        }
        findings.push(DiagnosticFinding {
            id: id.to_owned(),
            rule: rule.to_owned(),
            subject: subject.to_owned(),
            severity: severity.to_owned(),
            disposition: disposition.to_owned(),
        });
    }
    if metadata["findingCount"].as_u64() != Some(findings.len() as u64) {
        return Err(producer_invalid("producer-shape", "diagnostics"));
    }
    Ok(DiagnosticsEnvelope {
        project: project.to_owned(),
        findings,
    })
}

/// The typed rows of the closed artifact-ownership/profile snapshot
/// (`lekalo/search-ownership-profile/v0.8.0`).
struct OwnershipProfileEnvelope {
    project: String,
    artifacts: Vec<(String, String)>,
    profiles: Vec<(String, Vec<String>)>,
}

/// Decode one ownership/profile snapshot through its closed grammar:
/// required members, no unknown members, closed ownership vocabulary,
/// per-member bounds, and the envelope count cross-checks.
fn decode_ownership_profile_envelope(
    value: &serde_json::Value,
) -> Result<OwnershipProfileEnvelope, DomainResult> {
    const IDENTITY: &str = "dev.lekalo.search-ownership-profile@0.8.0";
    const SCHEMA: &str = "lekalo/search-ownership-profile/v0.8.0";
    let Some(map) = value.as_object() else {
        return Err(producer_invalid("producer-malformed", "ownership-profile"));
    };
    for key in map.keys() {
        if !matches!(
            key.as_str(),
            "schemaVersion" | "identity" | "project" | "metadata" | "artifacts" | "profiles"
        ) {
            return Err(producer_invalid("producer-shape", "ownership-profile"));
        }
    }
    for required in [
        "schemaVersion",
        "identity",
        "project",
        "metadata",
        "artifacts",
        "profiles",
    ] {
        if !map.contains_key(required) {
            return Err(producer_invalid("producer-shape", "ownership-profile"));
        }
    }
    if map["schemaVersion"].as_str() != Some(SCHEMA) || map["identity"].as_str() != Some(IDENTITY) {
        return Err(DomainResult::invalid(
            lekalo_core::search::diagnostic_input_invalid(
                "identity-mismatch",
                Some("ownership-profile"),
            ),
        ));
    }
    let project = map["project"]
        .as_str()
        .ok_or_else(|| producer_invalid("producer-shape", "ownership-profile"))?;
    let metadata = map["metadata"]
        .as_object()
        .ok_or_else(|| producer_invalid("producer-shape", "ownership-profile"))?;
    if metadata.len() != 2
        || metadata
            .get("artifactCount")
            .and_then(|count| count.as_u64())
            .is_none()
        || metadata
            .get("profileCount")
            .and_then(|count| count.as_u64())
            .is_none()
    {
        return Err(producer_invalid("producer-shape", "ownership-profile"));
    }
    let artifact_rows = map["artifacts"]
        .as_array()
        .ok_or_else(|| producer_invalid("producer-shape", "ownership-profile"))?;
    let mut artifacts = Vec::with_capacity(artifact_rows.len());
    for row in artifact_rows {
        let Some(row) = row.as_object() else {
            return Err(producer_invalid("producer-malformed", "ownership-profile"));
        };
        if row.len() != 2 || !row.contains_key("path") || !row.contains_key("ownership") {
            return Err(producer_invalid("producer-malformed", "ownership-profile"));
        }
        let path = row["path"].as_str().unwrap_or_default();
        let logical_path = !path.is_empty()
            && path.len() <= 512
            && path.split('/').all(|segment| {
                !segment.is_empty()
                    && segment.starts_with(|first: char| {
                        first.is_ascii_lowercase() || first.is_ascii_digit()
                    })
                    && segment.chars().all(|character| {
                        character.is_ascii_lowercase()
                            || character.is_ascii_digit()
                            || matches!(character, '.' | '_' | '-')
                    })
            });
        if !logical_path {
            return Err(producer_invalid("producer-malformed", "ownership-profile"));
        }
        let ownership = row["ownership"].as_str().unwrap_or_default();
        if !matches!(ownership, "module" | "project") {
            return Err(producer_invalid("producer-malformed", "ownership-profile"));
        }
        artifacts.push((path.to_owned(), ownership.to_owned()));
    }
    let profile_rows = map["profiles"]
        .as_array()
        .ok_or_else(|| producer_invalid("producer-shape", "ownership-profile"))?;
    let mut profiles = Vec::with_capacity(profile_rows.len());
    for row in profile_rows {
        let Some(row) = row.as_object() else {
            return Err(producer_invalid("producer-malformed", "ownership-profile"));
        };
        if row.len() != 2 || !row.contains_key("id") || !row.contains_key("members") {
            return Err(producer_invalid("producer-malformed", "ownership-profile"));
        }
        let id = row["id"].as_str().unwrap_or_default();
        let bounded_id = !id.is_empty()
            && id.len() <= 192
            && id.starts_with(|first: char| first.is_ascii_alphanumeric() || first == '_')
            && id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | ':' | '-')
            });
        if !bounded_id {
            return Err(producer_invalid("producer-malformed", "ownership-profile"));
        }
        let members = row["members"]
            .as_array()
            .ok_or_else(|| producer_invalid("producer-malformed", "ownership-profile"))?;
        let mut member_ids = Vec::with_capacity(members.len());
        for member in members {
            let Some(member) = member.as_str() else {
                return Err(producer_invalid("producer-malformed", "ownership-profile"));
            };
            let bounded_member = !member.is_empty()
                && member.len() <= 192
                && member.starts_with(|first: char| first.is_ascii_lowercase())
                && member.chars().all(|character| {
                    character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || matches!(character, '_' | '.')
                });
            if !bounded_member {
                return Err(producer_invalid("producer-malformed", "ownership-profile"));
            }
            member_ids.push(member.to_owned());
        }
        profiles.push((id.to_owned(), member_ids));
    }
    if metadata["artifactCount"].as_u64() != Some(artifacts.len() as u64)
        || metadata["profileCount"].as_u64() != Some(profiles.len() as u64)
    {
        return Err(producer_invalid("producer-shape", "ownership-profile"));
    }
    Ok(OwnershipProfileEnvelope {
        project: project.to_owned(),
        artifacts,
        profiles,
    })
}

/// One producer-custody refusal with a bounded producer reason.
fn producer_invalid(detail: &'static str, producer: &'static str) -> DomainResult {
    DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
        detail,
        Some(producer),
    ))
}

/// The admitted custody facts of one manifest: the consumed pins (each
/// with the exact live-bytes digest) and the counted unattachable rows.
struct Admitted {
    pins: Vec<lekalo_core::search::InputPin>,
    dropped_rows: u64,
}

/// Parse every admitted producer document with its own published
/// decoder and project the typed facts onto the shared projection.
///
/// Custody: every producer document must bind the declared project, its
/// published contract identity (when pinned), and its declared bytes
/// digest (when pinned); the single confined bounded read is both the
/// pin source and the decoded source (no prepass can read past the
/// confinement gate). Canonical effect subjects are validated against
/// the typed IR: a producer cannot invent a resource or field the Model
/// does not declare.
fn admit_producers(
    root: &std::path::Path,
    project_id: Option<&str>,
    manifest: &SearchInputs,
    project: &lekalo_core::ir::CompiledProject,
    projection: &mut Projection,
    coverage: &mut Vec<lekalo_core::search::CoverageRow>,
) -> Result<Admitted, DomainResult> {
    let canonical_root = std::fs::canonicalize(root).map_err(|_| {
        DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
            "root-unresolvable",
            None,
        ))
    })?;
    let cap = lekalo_core::search::MAX_POSTINGS;
    let mut states: Vec<(&'static str, &'static str)> = Vec::new();
    let mut dropped_effects: u64 = 0;
    let mut pins = Vec::with_capacity(manifest.entries().len());
    for entry in manifest.entries() {
        // The one confined bounded read: custody gate, pin source, and
        // decoded bytes are the exact same buffer.
        let path = root.join(&entry.path);
        let bytes = read_confined(&canonical_root, &path, entry.kind.key())?;
        let actual = format!("sha256:{}", lekalo_core::search::sha256_of(&bytes));
        if let Some(expected) = &entry.digest {
            if &actual != expected {
                return Err(DomainResult::invalid(
                    lekalo_core::search::diagnostic_input_invalid(
                        "digest-mismatch",
                        Some(entry.kind.key()),
                    ),
                ));
            }
        }
        pins.push(lekalo_core::search::InputPin {
            kind: entry.kind.key().to_owned(),
            path: entry.path.clone(),
            digest: Some(actual),
            identity: entry.identity.clone(),
        });
        match entry.kind {
            lekalo_core::search::ProducerKind::Trace => {
                let document = lekalo_core::trace::TraceManifest::parse(&bytes)
                    .map_err(DomainResult::invalid)?;
                if let Some(project) = project_id {
                    // The neutral trace names its project reference; a
                    // foreign manifest never joins this project's view.
                    check_producer_project(&document.manifest().project_ref, project)?;
                }
                let complete = project_trace(projection, &document, cap);
                states.push(("trace", if complete { "complete" } else { "partial" }));
                states.push((
                    "requirements",
                    if complete { "complete" } else { "partial" },
                ));
            }
            lekalo_core::search::ProducerKind::ObservedIndex => {
                let index =
                    lekalo_core::observed::parse_index(&bytes).map_err(DomainResult::invalid)?;
                if let Some(project) = project_id {
                    check_producer_project(&index.project, project)?;
                }
                // Per-record custody: every symbol's recorded provenance
                // revision must be the document's own revision. A record
                // rewritten beside a foreign revision is tampered
                // evidence, never a silently merged fact.
                let document_revision = &index.revision;
                for record in &index.symbols {
                    if record.provenance.revision != *document_revision {
                        return Err(DomainResult::invalid(
                            lekalo_core::search::diagnostic_input_invalid(
                                "revision-mismatch",
                                Some("observed-index"),
                            ),
                        ));
                    }
                }
                project_observed(projection, &index, cap);
                states.push(("bindings", "complete"));
            }
            lekalo_core::search::ProducerKind::Effects => {
                let value = producer_json(&bytes, "effects")?;
                let envelope = decode_effects_envelope(&value)?;
                if let Some(project) = project_id {
                    check_producer_project(&envelope.project, project)?;
                }
                // Canonical subjects are the IR's own vocabulary: an
                // evidence row naming an entity or field the Model does
                // not declare is refused, never attached beside its
                // neighbors.
                validate_effect_subjects(&envelope, project)?;
                let omitted = project_effects_document(projection, &envelope, cap);
                // Lost candidates are counted, never silent: rows the
                // projection cannot attach surface as a work-cap
                // warning with the admitting producer named.
                if omitted > 0 {
                    dropped_effects += omitted as u64;
                }
                states.push(("effects", "complete"));
            }
            lekalo_core::search::ProducerKind::Waivers => {
                let value = producer_json(&bytes, "waivers")?;
                let store = decode_waivers_store(&bytes, &value)?;
                if let Some(project) = project_id {
                    check_producer_project(&store.project_id, project)?;
                }
                project_waivers(projection, &store, cap)?;
                states.push(("waivers", "complete"));
            }
            lekalo_core::search::ProducerKind::Requirements => {
                let attachment = lekalo_core::requirements::RequirementsAttachment::parse(&bytes)
                    .map_err(DomainResult::invalid)?;
                if let Some(project) = project_id {
                    check_producer_project(attachment.project_id().as_str(), project)?;
                }
                project_requirements(projection, &attachment, cap);
                states.push(("requirements", "complete"));
            }
            lekalo_core::search::ProducerKind::ErrorContract => {
                let registry = lekalo_core::error_contract::ErrorRegistry::from_bytes(&bytes)
                    .map_err(DomainResult::invalid)?;
                // Project custody: the #62 owners' own validator confirms
                // the registry's bindings agree with THIS compiled project
                // (operations exist, kinds match, unions resolve); a
                // foreign or stale registry refuses here.
                let findings = lekalo_core::error_contract::validate(&registry, project);
                if !findings.as_slice().is_empty() {
                    return Err(producer_invalid("subject-unknown", "error-contract"));
                }
                project_error_contract(projection, &registry, cap);
            }
            lekalo_core::search::ProducerKind::Diagnostics => {
                let value = producer_json(&bytes, "diagnostics")?;
                let envelope = decode_diagnostics_envelope(&value)?;
                if let Some(project) = project_id {
                    check_producer_project(&envelope.project, project)?;
                }
                project_diagnostics(projection, &envelope, cap);
                states.push(("diagnostics", "complete"));
            }
            lekalo_core::search::ProducerKind::OwnershipProfile => {
                let value = producer_json(&bytes, "ownership-profile")?;
                let envelope = decode_ownership_profile_envelope(&value)?;
                if let Some(project) = project_id {
                    check_producer_project(&envelope.project, project)?;
                }
                project_ownership_profile(projection, &envelope, cap);
            }
        }
    }
    // One row per admitted domain: the worst observed state wins, so a
    // partial producer never advertises complete coverage.
    for (domain, state) in states {
        let existing = coverage.iter_mut().find(|row| row.domain == domain);
        match existing {
            Some(row) if row.state == "complete" && state != "complete" => {
                row.state = state.to_owned()
            }
            Some(row) if row.state == "unknown" => row.state = state.to_owned(),
            Some(_) => {}
            None => coverage.push(coverage_row(domain, state)),
        }
    }
    // Lost candidates are counted, never silent: the caller surfaces a
    // bounded work-cap warning beside the (explicitly partial) answer.
    Ok(Admitted {
        pins,
        dropped_rows: dropped_effects,
    })
}

/// Validate every evidence row's canonical subject against the typed
/// IR: a `canonical` resource must name a declared entity, and a named
/// field must be one of that entity's declared fields. A producer
/// cannot invent subjects the Model does not declare — invented
/// evidence would otherwise attach beside genuine facts undetected.
/// Target/external resources are adapter vocabulary and ride the row's
/// own custody.
fn validate_effect_subjects(
    envelope: &EffectsEnvelope,
    project: &lekalo_core::ir::CompiledProject,
) -> Result<(), DomainResult> {
    let entities: std::collections::BTreeMap<&str, Vec<&str>> = project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            lekalo_core::ir::Definition::Entity(entity) => Some((
                entity.id.as_str(),
                entity
                    .fields
                    .iter()
                    .map(|field| field.name.as_str())
                    .collect::<Vec<_>>(),
            )),
            _ => None,
        })
        .collect();
    for row in &envelope.rows {
        if row.resource_kind != lekalo_core::effects::ResourceKind::Canonical {
            continue;
        }
        let Some(fields) = entities.get(row.resource_id.as_str()) else {
            return Err(producer_invalid("subject-unknown", "effects"));
        };
        if let Some(field) = &row.field {
            if !fields.iter().any(|declared| declared == field) {
                return Err(producer_invalid("subject-unknown", "effects"));
            }
        }
    }
    Ok(())
}
/// Project the neutral trace relations onto requirement/symbol/gate
/// cards with per-relationship trust.
///
/// Relation endpoints resolve through a node-ID-to-card-key map built
/// from the nodes' typed fields (requirementId/gateId/scenarioId/
/// semanticId), never by re-parsing the node-ID spelling: a node whose
/// ID differs from its semantic identity (`scenario:planner-focus-flow`
/// carrying `scenarioId: planner.focus_flow`) joins exactly like any
/// other. Requirement-to-scenario/gate chains materialize as typed refs
/// on the joined cards, so `--requirement` + `--requirement-source`
/// finds scenarios and gates through the graph, not just definitions.
fn project_trace(
    projection: &mut Projection,
    manifest: &lekalo_core::trace::TraceManifest,
    cap: usize,
) -> bool {
    let complete = manifest.manifest().completeness.as_str() == "full";
    // The provider namespace of every requirement node: the
    // sourceSystem of its incident relations. Requirement external IDs
    // reused across providers stay separately qualified, never merged.
    let mut sources: std::collections::BTreeMap<String, String> = Default::default();
    for relation in manifest.relations() {
        let system = &relation.provenance.source_system;
        if !system.is_empty() {
            for endpoint in [&relation.from_node, &relation.to_node] {
                sources
                    .entry(endpoint.clone())
                    .or_insert_with(|| system.clone());
            }
        }
    }
    let requirement_key = |node_id: &str, requirement_id: &str| match sources.get(node_id) {
        Some(source) => format!("req:{source}:{requirement_id}"),
        None => format!("req:{requirement_id}"),
    };
    // The typed card key of each node resolves through the node map
    // built below from the nodes' own typed fields.
    let mut node_keys: std::collections::BTreeMap<String, String> = Default::default();
    for node in &manifest.manifest().nodes {
        let (kind, key, id) = match node.node_kind {
            lekalo_core::trace::NodeKind::Requirement => (
                CardKind::Requirement,
                requirement_key(
                    &node.node_id,
                    node.requirement_id.as_deref().unwrap_or(&node.node_id),
                ),
                node.requirement_id
                    .clone()
                    .unwrap_or_else(|| node.node_id.clone()),
            ),
            lekalo_core::trace::NodeKind::Gate => (
                CardKind::Gate,
                format!("gate:{}", node.gate_id.as_deref().unwrap_or(&node.node_id)),
                node.gate_id.clone().unwrap_or_else(|| node.node_id.clone()),
            ),
            lekalo_core::trace::NodeKind::Scenario => (
                CardKind::Scenario,
                format!(
                    "def:{}",
                    node.scenario_id.as_deref().unwrap_or(&node.node_id)
                ),
                node.scenario_id
                    .clone()
                    .unwrap_or_else(|| node.node_id.clone()),
            ),
            _ => continue,
        };
        node_keys.insert(node.node_id.clone(), key.clone());
        if projection.cards.contains_key(&key) {
            continue;
        }
        let card = Card {
            key: key.clone(),
            kind,
            subkind: None,
            module: None,
            id: Some(id.clone()),
            name: Some(id.rsplit([':', '/']).next().unwrap_or(&id).to_owned()),
            description: None,
            signatures: Vec::new(),
            relations: Vec::new(),
            effects: Vec::new(),
            bindings: Vec::new(),
            locations: Vec::new(),
            requirements: Vec::new(),
            scenarios: Vec::new(),
            gates: Vec::new(),
            portability: None,
            visibility: None,
            confidence: Confidence::Extracted,
            freshness: Freshness::AsRecorded,
            completeness: Completeness::Complete,
            omitted: 0,
            provenance: "trace".to_owned(),
        };
        add_card(projection, card, cap);
        // The bare external requirement ID is an exact qualified
        // identity of its provider-qualified card: a query naming the
        // recorded external spelling must never degrade to a lexical
        // tier (duplicates across providers stay ambiguous by rank).
        if let CardKind::Requirement = kind {
            lekalo_core::search::add_qualified_id(projection, &key, id.clone(), cap);
        }
    }
    // Symbol nodes resolve to existing definition cards (the canonical
    // IR owns them); record the typed join so relations reach them.
    for node in &manifest.manifest().nodes {
        if node.node_kind == lekalo_core::trace::NodeKind::Symbol {
            let key = format!(
                "def:{}",
                node.semantic_id.as_deref().unwrap_or(&node.node_id)
            );
            node_keys.insert(node.node_id.clone(), key);
        }
    }
    // Materialized typed joins: every relation between two resolved
    // nodes also lands as a typed ref (requirements/scenarios/gates
    // previews) on the joined cards, with the relation's own trust kept
    // per relationship.
    let relations: Vec<(String, String, String, String)> = manifest
        .relations()
        .map(|relation| {
            (
                relation.relation_kind.as_str().to_owned(),
                relation.from_node.clone(),
                relation.to_node.clone(),
                relation.provenance.source_system.clone(),
            )
        })
        .collect();
    let mut trace_graph: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (relation_kind, from, to, source) in &relations {
        let from_key = node_keys.get(from).cloned();
        let to_key = node_keys.get(to).cloned();
        let to_evidence = to_key.clone().unwrap_or_else(|| {
            evidence_key_of(to, source, &|node_id, requirement_id| {
                requirement_key(node_id, requirement_id)
            })
        });
        let from_evidence = from_key.clone().unwrap_or_else(|| {
            evidence_key_of(from, source, &|node_id, requirement_id| {
                requirement_key(node_id, requirement_id)
            })
        });
        for (side_key, other_key, direction) in [
            (from_key.clone(), to_evidence.clone(), "outgoing"),
            (to_key.clone(), from_evidence.clone(), "incoming"),
        ] {
            let Some(card_key) = side_key else {
                continue;
            };
            let Some(card) = projection.cards.get_mut(&card_key) else {
                continue;
            };
            let mut relations = card.relations.clone();
            relations.push(lekalo_core::search::RelationWitness {
                relation: relation_kind.clone(),
                other: other_key.clone(),
                direction: direction.to_owned(),
                trust: lekalo_core::search::Trust::Detected,
                confidence: Confidence::Verified,
            });
            relations.sort_by(|left, right| {
                (
                    left.relation.as_bytes(),
                    left.other.as_bytes(),
                    left.direction.as_bytes(),
                )
                    .cmp(&(
                        right.relation.as_bytes(),
                        right.other.as_bytes(),
                        right.direction.as_bytes(),
                    ))
            });
            if let Some(card) = projection.cards.get_mut(&card_key) {
                card.relations = relations;
            }
        }
        // Typed ref joins: the requirement/gate/scenario chain lands on
        // each joined card's bounded typed previews so selectors that
        // cross the chain resolve without re-deriving it. The closure is
        // transitive over the trace graph: a requirement reaches the
        // scenarios and gates that cover the symbols implementing it,
        // through any number of typed hops.
        if let (Some(from_key), Some(to_key)) = (&from_key, &to_key) {
            trace_graph
                .entry(from_key.clone())
                .or_default()
                .push(to_key.clone());
            trace_graph
                .entry(to_key.clone())
                .or_default()
                .push(from_key.clone());
        }
    }
    // Materialize the transitive typed refs from the trace graph.
    materialize_trace_refs(projection, &trace_graph, cap);
    complete
}

/// Materialize the transitive typed refs of one projected trace graph:
/// every card reachable from a requirement card carries that
/// requirement's ref, and likewise for scenario and gate cards. Lists
/// stay deduplicated, bounded, and omission-counted (never silent).
fn materialize_trace_refs(
    projection: &mut Projection,
    trace_graph: &std::collections::BTreeMap<String, Vec<String>>,
    cap: usize,
) {
    type RefList = fn(&mut Card) -> &mut Vec<String>;
    let kinds: Vec<(CardKind, RefList)> = vec![
        (CardKind::Requirement, |card| &mut card.requirements),
        (CardKind::Scenario, |card| &mut card.scenarios),
        (CardKind::Gate, |card| &mut card.gates),
    ];
    let _ = cap;
    let seeds: Vec<(String, CardKind)> = projection
        .cards
        .values()
        .filter(|card| {
            matches!(
                card.kind,
                CardKind::Requirement | CardKind::Scenario | CardKind::Gate
            )
        })
        .map(|card| (card.key.clone(), card.kind))
        .collect();
    for (seed_key, seed_kind) in seeds {
        let (reference, get_list) = match kinds.iter().find(|(kind, _)| *kind == seed_kind) {
            Some((_, get_list)) => {
                // The qualified key is the ref: a requirement ref keeps its
                // provider qualification (req:openspec:ID), so
                // --requirement-source matches through the chain.
                let reference = match seed_kind {
                    CardKind::Requirement => seed_key.clone(),
                    CardKind::Scenario => seed_key.trim_start_matches("def:").to_owned(),
                    CardKind::Gate => seed_key.trim_start_matches("gate:").to_owned(),
                    _ => continue,
                };
                (reference, *get_list)
            }
            None => continue,
        };
        // BFS over the trace graph; the seed itself already carries its
        // own kind's refs.
        let mut visited: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        visited.insert(seed_key.clone());
        let mut frontier = vec![seed_key.clone(); 1];
        while let Some(current) = frontier.pop() {
            for neighbor in trace_graph.get(&current).cloned().unwrap_or_default() {
                if visited.insert(neighbor.clone()) {
                    frontier.push(neighbor);
                }
            }
        }
        visited.remove(&seed_key);
        for reached in visited {
            let Some(card) = projection.cards.get_mut(&reached) else {
                continue;
            };
            let mut omitted = card.omitted;
            let list = get_list(card);
            if !list.contains(&reference) {
                lekalo_core::search::push_fact(list, &mut omitted, reference.clone());
                card.omitted = omitted;
            }
        }
    }
    // Keep every joined list deterministically ordered.
    for card in projection.cards.values_mut() {
        card.requirements.sort();
        card.scenarios.sort();
        card.gates.sort();
    }
}

/// The qualified key of one trace node reference, with the
/// requirement-source qualifier resolved by the caller. Node IDs whose
/// typed card was resolved ride the node map; this fallback only ever
/// sees unresolved references (artifact/test nodes and dangling IDs).
fn evidence_key_of(
    reference: &str,
    source: &str,
    requirement: &dyn Fn(&str, &str) -> String,
) -> String {
    let (kind, id) = reference.split_once(':').unwrap_or(("symbol", reference));
    match kind {
        "symbol" | "scenario" => format!("def:{id}"),
        "requirement" => {
            if source.is_empty() {
                requirement(reference, id)
            } else {
                format!("req:{source}:{id}")
            }
        }
        "gate" => format!("gate:{id}"),
        "binding" => format!("bind:{id}"),
        _ => format!("ref:{reference}"),
    }
}

/// Project the recorded native bindings of one observed index.
///
/// Custody and honesty rules:
/// - Every path-bearing identity (the stable key, the recorded
///   location, and each candidate's path and `path#symbol` spelling)
///   is admitted independently under the frozen privacy path grammar.
///   An allowed location never authorizes another field's sensitive
///   path, and a sensitive identity is withheld whole — never carried
///   in part through cards, postings, or qualified identities.
/// - The recorded currency state (`current`/`stale`/`unknown`) stays
///   the card's freshness: a stale record is excluded by default and
///   visible only under `--include-stale`, never laundered into an
///   as-recorded confirmed fact.
/// - Binding and definition cards carry the complete admitted candidate
///   set, so an exact `--native-symbol` lookup finds every recorded
///   spelling and cards never lose alternatives.
fn project_observed(
    projection: &mut Projection,
    index: &lekalo_core::observed::ObservedIndex,
    cap: usize,
) {
    let target = index.target.as_deref().unwrap_or(index.adapter.id.as_str());
    for record in &index.symbols {
        let key = format!("def:{}", record.id);
        // The recorded location is admitted under the same frozen
        // privacy path grammar as every identity: dotfiles, absolute
        // forms, and traversal segments are withheld with an explicit
        // completeness disclosure instead of leaking the subject.
        let location_path = record
            .location
            .as_ref()
            .map(|location| location.path.clone())
            .unwrap_or_default();
        let path_admitted = native_path_admitted(&location_path);
        // Every identity is checked on its own path part: the location
        // being allowed proves nothing about the stable key's path.
        let stable_key_admitted = record
            .stable_key
            .as_deref()
            .map(native_identity_admitted)
            .unwrap_or(false);
        let stable_key = if stable_key_admitted {
            record
                .stable_key
                .clone()
                .unwrap_or_else(|| record.id.clone())
        } else {
            record.id.clone()
        };
        // The complete admitted candidate set: path and symbol are
        // validated separately, so `path#symbol` spellings survive the
        // path grammar and every candidate stays exact-lookup-able.
        let admitted_candidates: Vec<&lekalo_core::observed::types::CandidateRecord> = record
            .candidates
            .iter()
            .filter(|candidate| native_identity_admitted(&candidate.native))
            .collect();
        let freshness = match record.state {
            lekalo_core::observed::BindingState::Current => Freshness::AsRecorded,
            lekalo_core::observed::BindingState::Stale => Freshness::Stale,
            lekalo_core::observed::BindingState::Unknown => Freshness::Unknown,
        };
        // Disclosure: a recorded identity the privacy grammar refuses is
        // withheld whole — the card discloses the withholding instead of
        // silently substituting a sanitized spelling.
        let identity_withheld = record.stable_key.is_some() && !stable_key_admitted;
        let card_complete = if !path_admitted || identity_withheld {
            Completeness::Withheld
        } else {
            Completeness::Complete
        };
        // The binding card exists alongside the definition card so a
        // native lookup finds recorded evidence either way.
        let mut bindings: Vec<NativeBinding> = Vec::new();
        if stable_key_admitted || path_admitted {
            bindings.push(NativeBinding {
                key: stable_key.clone(),
                target: target.to_owned(),
                path: if path_admitted {
                    location_path.clone()
                } else {
                    String::new()
                },
                symbol: record.stable_key.clone().filter(|_| stable_key_admitted),
                status: record.status.key().to_owned(),
            });
        }
        for candidate in &admitted_candidates {
            let binding = NativeBinding {
                key: candidate.native.clone(),
                target: target.to_owned(),
                path: candidate.path.clone(),
                symbol: Some(candidate.native.clone()),
                status: "inferred".to_owned(),
            };
            if !bindings.contains(&binding) {
                bindings.push(binding);
            }
        }
        bindings.sort_by(|left, right| {
            (&left.key[..], &left.path[..]).cmp(&(&right.key[..], &right.path[..]))
        });
        let binding_card = Card {
            key: format!("bind:{}", record.id),
            kind: CardKind::Binding,
            subkind: Some(record.kind.key().to_owned()),
            module: record
                .id
                .split_once('.')
                .map(|(module, _)| module.to_owned()),
            id: Some(record.id.clone()),
            name: Some(record.id.clone()),
            description: None,
            signatures: Vec::new(),
            relations: Vec::new(),
            effects: Vec::new(),
            bindings,
            locations: Vec::new(),
            requirements: Vec::new(),
            scenarios: Vec::new(),
            gates: record.gates.clone(),
            portability: None,
            visibility: None,
            confidence: Confidence::Extracted,
            freshness,
            completeness: card_complete,
            omitted: 0,
            provenance: "observed".to_owned(),
        };
        let binding_key = format!("bind:{}", record.id);
        add_card(projection, binding_card, cap);
        // Exact qualified identities: the stable binding key, the
        // admitted location path, and every admitted candidate spelling
        // (full `path#symbol` and bare symbol) all answer the
        // exact-qualified tier for both cards. Sensitive paths never
        // enter the identity set (the tier is byte-exact: an unadmitted
        // spelling is not a legal query identity).
        let identities: Vec<String> = stable_key_admitted
            .then(|| record.stable_key.clone().unwrap_or_default())
            .into_iter()
            .chain(path_admitted.then(|| location_path.clone()))
            .chain(admitted_candidates.iter().flat_map(|candidate| {
                let mut spellings = vec![candidate.native.clone()];
                if let Some((_, symbol)) = candidate.native.rsplit_once('#') {
                    spellings.push(symbol.to_owned());
                }
                spellings
            }))
            .collect();
        for identity in identities {
            lekalo_core::search::add_qualified_id(projection, &binding_key, identity.clone(), cap);
            lekalo_core::search::add_qualified_id(projection, &key, identity, cap);
        }
        // The definition card carries the same complete admitted
        // candidate set, so `--kind binding` style lookups and exact
        // filters see identical facts on both cards.
        let binding_card_bindings = projection
            .cards
            .get(&binding_key)
            .map(|binding_card| binding_card.bindings.clone())
            .unwrap_or_default();
        if let Some(card) = projection.cards.get_mut(&key) {
            let mut merged = card.bindings.clone();
            for binding in binding_card_bindings {
                if !merged.contains(&binding) {
                    merged.push(binding);
                }
            }
            merged.sort_by(|left, right| {
                (&left.key[..], &left.path[..]).cmp(&(&right.key[..], &right.path[..]))
            });
            card.bindings = merged;
            if freshness == Freshness::Stale {
                card.freshness = Freshness::Stale;
            } else if !path_admitted && card.completeness == Completeness::Complete {
                card.completeness = Completeness::Withheld;
            }
        }
    }
}

/// Whether one recorded native candidate path is admissible under the
/// frozen privacy path grammar (portable project-relative POSIX). A
/// sensitive/excluded spelling (dotfiles, absolute forms, traversal)
/// is never projected onto cards, postings, or identities.
fn native_path_admitted(path: &str) -> bool {
    path.is_empty() || lekalo_core::project_fs::path_violation(path).is_none()
}

/// Whether one whole path-bearing identity (a stable key spelling like
/// `apps/api/src/planner/focus-task.ts#focusTask`) is admissible: its
/// path part under the frozen privacy grammar. An allowed location on
/// the record never authorizes a different path smuggled into another
/// identity field.
fn native_identity_admitted(identity: &str) -> bool {
    let (path_part, symbol_part) = match identity.rsplit_once('#') {
        Some((path, symbol)) => (path, Some(symbol)),
        None => (identity, None),
    };
    if !native_path_admitted(path_part) {
        return false;
    }
    match symbol_part {
        Some(symbol) => {
            !symbol.is_empty()
                && symbol.len() <= 256
                && symbol
                    .chars()
                    .all(|character| !character.is_whitespace() && !character.is_control())
        }
        None => true,
    }
}

/// Project the admitted producer's decoded effect rows: the exact
/// detected evidence of the published effect-graph wire
/// (`lekalo/effects/v0.2.16`), consumed as typed rows from the closed
/// decoder.
///
/// Only evidence-provenance rows are projected here: the declared rows
/// are rebuilt from the canonical IR at admission, so copying them from
/// a saved snapshot could only ever double-count. Every row keeps its
/// exact subject (resource plus optional field) so an entity-wide
/// effect can never claim a field it did not name. Stale-trust rows are
/// excluded and counted (stale evidence is never silently served as
/// current), and rows that name an operation this projection does not
/// know are counted as omitted (lost candidates are counted, never
/// silently dropped).
fn project_effects_document(
    projection: &mut Projection,
    envelope: &EffectsEnvelope,
    cap: usize,
) -> usize {
    let mut witnesses: Vec<(String, lekalo_core::search::RelationWitness)> = Vec::new();
    let mut omitted = 0usize;
    for row in &envelope.rows {
        let card_key = format!("def:{}", row.operation);
        if !projection.cards.contains_key(&card_key) {
            omitted += 1;
            continue;
        }
        let Some(evidence) = &row.evidence else {
            continue;
        };
        // Stale evidence is excluded and counted: it stays visible
        // through the count, never as silently served current fact.
        if evidence.trust == lekalo_core::effects::TrustState::Stale {
            omitted += 1;
            continue;
        }
        let action = match row.kind.key() {
            "read" => "reads",
            "create" | "update" | "delete" | "write-field" => "writes",
            _ => continue,
        };
        let relation = match &row.field {
            Some(field) => format!("{action}:{}:{}", row.resource_id, field),
            None => format!("{action}:{}", row.resource_id),
        };
        let confidence = match evidence.trust {
            lekalo_core::effects::TrustState::Verified
            | lekalo_core::effects::TrustState::Current => Confidence::Verified,
            lekalo_core::effects::TrustState::Extracted => Confidence::Extracted,
            lekalo_core::effects::TrustState::Inferred => Confidence::Inferred,
            _ => Confidence::Unknown,
        };
        let witness = lekalo_core::search::RelationWitness {
            relation,
            other: format!("resource:{}", row.resource_id),
            direction: "outgoing".to_owned(),
            trust: lekalo_core::search::Trust::Detected,
            confidence,
        };
        witnesses.push((card_key, witness));
    }
    witnesses.sort_by(|left, right| {
        (
            left.0.as_bytes(),
            left.1.relation.as_bytes(),
            left.1.other.as_bytes(),
        )
            .cmp(&(
                right.0.as_bytes(),
                right.1.relation.as_bytes(),
                right.1.other.as_bytes(),
            ))
    });
    witnesses.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    for (card_key, witness) in witnesses {
        if let Some(card) = projection.cards.get_mut(&card_key) {
            if !card.effects.contains(&witness) {
                card.effects.push(witness.clone());
                for term in lekalo_core::search::effect_posting_terms(&witness.relation) {
                    lekalo_core::search::post_term(projection, &term, &card_key, cap);
                }
            }
        }
    }
    omitted
}

/// Project qualified waiver cards from one typed governance waiver
/// store. Active entries are current; revoked/superseded entries stay
/// visibly stale and are excluded unless the request explicitly
/// includes stale rows.
/// Project one requirements attachment: provider-qualified requirement
/// cards plus typed refs and relation witnesses on every linked symbol.
/// The attachment's own published decoder already validated the closed
/// wire; this join never widens it.
fn project_requirements(
    projection: &mut Projection,
    attachment: &lekalo_core::requirements::RequirementsAttachment,
    cap: usize,
) {
    for reference in attachment.references() {
        let key = format!("req:{}:{}", reference.source, reference.requirement);
        if !projection.cards.contains_key(&key) {
            let card = Card {
                key: key.clone(),
                kind: CardKind::Requirement,
                subkind: None,
                module: None,
                id: Some(reference.requirement.clone()),
                name: Some(
                    reference
                        .requirement
                        .rsplit([':', '/'])
                        .next()
                        .unwrap_or(&reference.requirement)
                        .to_owned(),
                ),
                description: None,
                signatures: Vec::new(),
                relations: Vec::new(),
                effects: Vec::new(),
                bindings: Vec::new(),
                locations: Vec::new(),
                requirements: Vec::new(),
                scenarios: Vec::new(),
                gates: Vec::new(),
                portability: None,
                visibility: None,
                confidence: Confidence::Extracted,
                freshness: Freshness::AsRecorded,
                completeness: Completeness::Complete,
                omitted: 0,
                provenance: "requirements".to_owned(),
            };
            add_card(projection, card, cap);
            // The bare external requirement ID is an exact qualified
            // identity of its provider-qualified card.
            lekalo_core::search::add_qualified_id(
                projection,
                &key,
                reference.requirement.clone(),
                cap,
            );
        }
        // The typed ref and the relation witness land on the symbol's
        // card: the attachment's relation is the recorded provenance.
        let symbol_key = format!("def:{}", reference.symbol);
        if let Some(card) = projection.cards.get_mut(&symbol_key) {
            if !card.requirements.contains(&reference.requirement) {
                let mut omitted = card.omitted;
                lekalo_core::search::push_fact(
                    &mut card.requirements,
                    &mut omitted,
                    reference.requirement.clone(),
                );
                card.omitted = omitted;
            }
            let witness = lekalo_core::search::RelationWitness {
                relation: match reference.relation {
                    lekalo_core::requirements::Relation::DerivedFrom => "derived-from".to_owned(),
                    lekalo_core::requirements::Relation::Implements => "implements".to_owned(),
                },
                other: key.clone(),
                direction: "outgoing".to_owned(),
                trust: lekalo_core::search::Trust::Detected,
                confidence: Confidence::Verified,
            };
            if !card.relations.contains(&witness) {
                card.relations.push(witness);
                card.relations.sort_by(|left, right| {
                    (
                        left.relation.as_bytes(),
                        left.other.as_bytes(),
                        left.direction.as_bytes(),
                    )
                        .cmp(&(
                            right.relation.as_bytes(),
                            right.other.as_bytes(),
                            right.direction.as_bytes(),
                        ))
                });
            }
        }
    }
}

/// Project one error-contract registry slice: every operation binding
/// lands as typed error-role signatures on the operation's card, so
/// `--error` resolves through the #62 owners' validated facts.
fn project_error_contract(
    projection: &mut Projection,
    registry: &lekalo_core::error_contract::ErrorRegistry,
    cap: usize,
) {
    let _ = cap;
    for binding in registry.bindings() {
        let card_key = format!("def:{}", binding.operation().as_str());
        let Some(card) = projection.cards.get_mut(&card_key) else {
            continue;
        };
        for error in binding.errors().members() {
            let signature = SignatureRef {
                role: "error".to_owned(),
                id: error.id().as_str().to_owned(),
            };
            if !card.signatures.contains(&signature) {
                card.signatures.push(signature);
            }
        }
        card.signatures.sort_by(|left, right| {
            (&left.role[..], &left.id[..]).cmp(&(&right.role[..], &right.id[..]))
        });
    }
}

/// Project one diagnostic-set snapshot: admitted diagnostic finding
/// cards keyed by finding id, rule identity, severity subkind, and a
/// typed `evidences` relation onto the subject card. Resolved findings
/// stay visibly stale (excluded by default, never erased).
fn project_diagnostics(projection: &mut Projection, envelope: &DiagnosticsEnvelope, cap: usize) {
    for finding in &envelope.findings {
        let key = format!("diag:{}", finding.id);
        let freshness = if finding.disposition == "resolved" {
            Freshness::Stale
        } else {
            Freshness::AsRecorded
        };
        let mut card = Card {
            key: key.clone(),
            kind: CardKind::Diagnostic,
            subkind: Some(finding.severity.clone()),
            module: finding
                .subject
                .split_once('.')
                .map(|(module, _)| module.to_owned()),
            id: Some(finding.rule.clone()),
            name: Some(finding.rule.clone()),
            description: None,
            signatures: Vec::new(),
            relations: Vec::new(),
            effects: Vec::new(),
            bindings: Vec::new(),
            locations: Vec::new(),
            requirements: Vec::new(),
            scenarios: Vec::new(),
            gates: Vec::new(),
            portability: None,
            visibility: None,
            confidence: Confidence::Extracted,
            freshness,
            completeness: Completeness::Complete,
            omitted: 0,
            provenance: "diagnostics".to_owned(),
        };
        card.relations.push(lekalo_core::search::RelationWitness {
            relation: "evidences".to_owned(),
            other: format!("def:{}", finding.subject),
            direction: "outgoing".to_owned(),
            trust: lekalo_core::search::Trust::Detected,
            confidence: Confidence::Extracted,
        });
        card.relations.sort_by(|left, right| {
            (
                left.relation.as_bytes(),
                left.other.as_bytes(),
                left.direction.as_bytes(),
            )
                .cmp(&(
                    right.relation.as_bytes(),
                    right.other.as_bytes(),
                    right.direction.as_bytes(),
                ))
        });
        add_card(projection, card, cap);
    }
}

/// Project one artifact-ownership/profile snapshot: artifact ownership
/// facts fill the declared ownership of definitions whose native
/// bindings live under the artifact path (the Model's own declared
/// visibility always wins), and profile memberships land as typed
/// `profile-member` relations so `--profile` resolves without ever
/// claiming conformance.
fn project_ownership_profile(
    projection: &mut Projection,
    envelope: &OwnershipProfileEnvelope,
    cap: usize,
) {
    let _ = cap;
    // Ownership: the definitions whose recorded native bindings live
    // under an owned artifact path inherit that artifact's ownership
    // when the Model itself did not declare one.
    let keys: Vec<String> = projection.cards.keys().cloned().collect();
    for (path, ownership) in &envelope.artifacts {
        for key in &keys {
            let Some(card) = projection.cards.get_mut(key) else {
                continue;
            };
            let under_artifact = card.bindings.iter().any(|binding| {
                binding.path == *path
                    || binding
                        .path
                        .strip_prefix(path.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            });
            if under_artifact && card.visibility.is_none() {
                card.visibility = Some(ownership.clone());
            }
        }
    }
    // Profiles: membership is a recorded fact, never a conformance
    // claim; the typed relation carries exactly the membership.
    for (profile, members) in &envelope.profiles {
        for member in members {
            let key = format!("def:{member}");
            if let Some(card) = projection.cards.get_mut(&key) {
                let witness = lekalo_core::search::RelationWitness {
                    relation: "profile-member".to_owned(),
                    other: format!("profile:{profile}"),
                    direction: "outgoing".to_owned(),
                    trust: lekalo_core::search::Trust::Detected,
                    confidence: Confidence::Extracted,
                };
                if !card.relations.contains(&witness) {
                    card.relations.push(witness);
                    card.relations.sort_by(|left, right| {
                        (
                            left.relation.as_bytes(),
                            left.other.as_bytes(),
                            left.direction.as_bytes(),
                        )
                            .cmp(&(
                                right.relation.as_bytes(),
                                right.other.as_bytes(),
                                right.direction.as_bytes(),
                            ))
                    });
                }
            }
        }
    }
}

fn project_waivers(
    projection: &mut Projection,
    store: &lekalo_core::waivers::Store,
    cap: usize,
) -> Result<(), DomainResult> {
    for entry in &store.entries {
        let id = entry.id.as_str();
        let lifecycle = &entry.lifecycle;
        let subject = Some(entry.subject.as_str()).filter(|subject| !subject.is_empty());
        let rule = match &entry.selector.kind {
            lekalo_core::waivers::SelectorKind::Rule => Some(entry.selector.id.as_str()),
            _ => None,
        };
        let freshness = if *lifecycle == lekalo_core::waivers::Lifecycle::Active {
            Freshness::Current
        } else {
            Freshness::Stale
        };
        let mut card = Card {
            key: format!("waiv:{id}"),
            kind: CardKind::Waiver,
            subkind: Some(lifecycle_key(lifecycle).to_owned()),
            module: None,
            id: Some(id.to_owned()),
            name: Some(id.to_owned()),
            description: None,
            signatures: Vec::new(),
            relations: Vec::new(),
            effects: Vec::new(),
            bindings: Vec::new(),
            locations: Vec::new(),
            requirements: Vec::new(),
            scenarios: Vec::new(),
            gates: Vec::new(),
            portability: None,
            visibility: None,
            confidence: Confidence::Extracted,
            freshness,
            completeness: Completeness::Complete,
            omitted: 0,
            provenance: "waivers".to_owned(),
        };
        // The waived finding's rule and subject ride as typed relations
        // so `--diagnostic` and subject queries see the waiver beside
        // the original finding (suppression is metadata, not erasure).
        if let Some(rule) = rule {
            card.relations.push(lekalo_core::search::RelationWitness {
                relation: "waives-rule".to_owned(),
                other: format!("diag:{rule}"),
                direction: "outgoing".to_owned(),
                trust: lekalo_core::search::Trust::Detected,
                confidence: Confidence::Extracted,
            });
        }
        if let Some(subject) = subject {
            card.relations.push(lekalo_core::search::RelationWitness {
                relation: "waives-subject".to_owned(),
                other: format!("def:{subject}"),
                direction: "outgoing".to_owned(),
                trust: lekalo_core::search::Trust::Detected,
                confidence: Confidence::Extracted,
            });
            if freshness == Freshness::Current {
                if let Some(target) = projection.cards.get_mut(&format!("def:{subject}")) {
                    let witness = lekalo_core::search::RelationWitness {
                        relation: "waived-by".to_owned(),
                        other: format!("waiv:{id}"),
                        direction: "incoming".to_owned(),
                        trust: lekalo_core::search::Trust::Detected,
                        confidence: Confidence::Extracted,
                    };
                    if !target.relations.contains(&witness) {
                        target.relations.push(witness);
                        target.relations.sort_by(|left, right| {
                            (
                                left.relation.as_bytes(),
                                left.other.as_bytes(),
                                left.direction.as_bytes(),
                            )
                                .cmp(&(
                                    right.relation.as_bytes(),
                                    right.other.as_bytes(),
                                    right.direction.as_bytes(),
                                ))
                        });
                    }
                }
            }
        }
        card.relations.sort_by(|left, right| {
            (
                left.relation.as_bytes(),
                left.other.as_bytes(),
                left.direction.as_bytes(),
            )
                .cmp(&(
                    right.relation.as_bytes(),
                    right.other.as_bytes(),
                    right.direction.as_bytes(),
                ))
        });
        add_card(projection, card, cap);
    }
    Ok(())
}

/// The exact wire spelling of one governance lifecycle state.
fn lifecycle_key(lifecycle: &lekalo_core::waivers::Lifecycle) -> &'static str {
    match lifecycle {
        lekalo_core::waivers::Lifecycle::Active => "active",
        lekalo_core::waivers::Lifecycle::Revoked => "revoked",
        lekalo_core::waivers::Lifecycle::Superseded => "superseded",
    }
}

/// Run one `search-index` command.
pub fn run_search_index(command: SearchIndexCommands, no_cache: bool) -> DomainResult {
    match command {
        SearchIndexCommands::Status { project } => {
            // The bypass refuses the durable store surface outright:
            // status IS the durable store's read-only oracle, so there
            // is no honest "no-cache" answer to define.
            if no_cache {
                return DomainResult::usage_error();
            }
            let selection = lekalo_core::loader::LoadSelection {
                project: project.or_else(|| std::env::var("LEKALO_PROJECT").ok()),
            };
            let root = match lekalo_core::doctor::project_root(&selection) {
                Ok(root) => root,
                Err(result) => return result,
            };
            let store = match SearchStore::resolve(&root) {
                Ok(store) => store,
                Err(_) => {
                    return DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
                        "store-denied",
                        None,
                    ))
                }
            };
            let status = store.status();
            // Freshness oracle: the generation's own recorded pins are
            // the truth, never a re-guessed manifest location. Each
            // pinned producer path is re-read confined under the
            // canonical project root and re-hashed; a missing or drifted
            // pin file is `search.index-stale`. The full admitted
            // assembly (canonical Model plus the pinned producers, under
            // the pinned admission semantics) is then recomputed exactly
            // the way build does and compared by content digest, so
            // model drift, policy drift, and removed manifests are all
            // detected. The verdict is independent of the caller's
            // working directory and of where the manifest happened to
            // live: a generation can only ever be `ready` when every
            // recorded pin re-verifies and the recomputed snapshot is
            // byte-identical. Admission failures surface as their own
            // diagnostics, never folded into a stale verdict.
            let mut diagnostics: Vec<lekalo_core::diagnostics::Diagnostic> = Vec::new();
            let mut state = status.state;
            if status.state == "ready" {
                let Some(document) = status.document.clone() else {
                    unreachable!("ready carries the validated generation");
                };
                match verify_generation_freshness(&root, &selection, &document) {
                    FreshnessVerdict::Fresh => state = "ready",
                    FreshnessVerdict::Drifted(pin) => {
                        diagnostics.extend(
                            lekalo_core::search::emit_index_stale(pin)
                                .as_slice()
                                .to_vec(),
                        );
                        state = "stale";
                    }
                    FreshnessVerdict::AdmissionRefused(result) => {
                        // The project no longer admits its own pinned
                        // inputs (or no longer compiles): that refusal,
                        // with its own diagnostics, IS the answer.
                        return result;
                    }
                }
            }
            let json = format!(
                "{{\"status\":\"valid\",\"searchIndex\":{{\"state\":\"{state}\",\"generation\":{},\"cards\":{},\"postings\":{}}}}}",
                status
                    .generation
                    .as_deref()
                    .map(|generation| format!("\"{generation}\""))
                    .unwrap_or_else(|| "null".to_owned()),
                status
                    .cards
                    .map(|cards| cards.to_string())
                    .unwrap_or_else(|| "null".to_owned()),
                status
                    .postings
                    .map(|postings| postings.to_string())
                    .unwrap_or_else(|| "null".to_owned()),
            );
            let human = format!(
                "search index {} : {}",
                state,
                status.generation.clone().unwrap_or_default()
            );
            DomainResult::graph(json, human, diagnostics)
        }
        command @ (SearchIndexCommands::Build { .. } | SearchIndexCommands::Update { .. }) => {
            let (SearchIndexCommands::Build { page } | SearchIndexCommands::Update { page }) =
                &command
            else {
                unreachable!("arm matches only build/update");
            };
            let is_update = matches!(command, SearchIndexCommands::Update { .. });
            let page = &page;
            if no_cache {
                // A build never runs under the bypass: publishing is
                // the whole point, and --no-cache bypasses every write.
                return DomainResult::usage_error();
            }
            let selection = lekalo_core::loader::LoadSelection {
                project: page
                    .project
                    .clone()
                    .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
            };
            let root = match lekalo_core::doctor::project_root(&selection) {
                Ok(root) => root,
                Err(result) => return result,
            };
            let store = match SearchStore::resolve(&root) {
                Ok(store) => store,
                Err(_) => {
                    return DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
                        "store-denied",
                        None,
                    ))
                }
            };
            let service = match assemble(page, no_cache) {
                Ok(service) => service,
                Err(result) => return result,
            };
            // The index assembles under the run's admitted cap BEFORE any
            // publication: a budget refusal is a typed denial, never a
            // silently emptied snapshot with a fresh digest.
            if let Err(set) = lekalo_core::search::SearchIndex::from_projection(
                &service.projection,
                &service.pins,
                &service.policy_digest,
                service.project_id.as_deref(),
                service.cap,
            ) {
                return DomainResult::invalid(set);
            }
            let document = snapshot_document(&service);
            let digest = lekalo_core::search::digest_of(&document);
            // Incremental semantics: when the recomputed content digest
            // equals the published generation AND that generation's
            // stored bytes validate, update is a no-op republish of that
            // same immutable generation (partition reuse). Poisoned,
            // aliased, or forged storage falls through to an atomic
            // republish that repairs the custody — a rebuild never
            // reports successful reuse over invalid bytes.
            let current_valid = store
                .current()
                .ok()
                .flatten()
                .map(|current| {
                    current == digest && store.load(&current).is_ok_and(|loaded| loaded.is_some())
                })
                .unwrap_or(false);
            if current_valid {
                let json = format!(
                    "{{\"status\":\"valid\",\"operation\":\"search-index-update\",\"digest\":\"sha256:{digest}\",\"cards\":{},\"reused\":true}}",
                    document.cards.len()
                );
                let human = format!(
                    "search index current : {} cards, digest sha256:{digest}",
                    document.cards.len()
                );
                return DomainResult::receipt(json, human);
            }
            let mut document = document;
            document.digest = format!("sha256:{digest}");
            if store.publish(&document).is_err() {
                return DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
                    "publish-failed",
                    None,
                ));
            }
            let json = format!(
                "{{\"status\":\"valid\",\"operation\":\"search-index-{}\",\"digest\":\"sha256:{digest}\",\"cards\":{}}}",
                if is_update { "update" } else { "build" },
                document.cards.len()
            );
            let human = format!(
                "search index built : {} cards, digest sha256:{digest}",
                document.cards.len()
            );
            DomainResult::receipt(json, human)
        }
    }
}

/// The verdict of the generation freshness oracle.
enum FreshnessVerdict {
    /// Every pin re-verifies and the recomputed snapshot is identical.
    Fresh,
    /// A recorded pin no longer matches (the bounded pin kind).
    Drifted(&'static str),
    /// The pinned inputs no longer admit: the refusal itself is the
    /// honest answer, with its own diagnostics.
    AdmissionRefused(DomainResult),
}

/// The freshness oracle of one published generation: re-hash every
/// recorded pin under the canonical project root, then recompute the
/// exact snapshot digest a fresh build would publish under the same
/// pinned admission semantics and compare by content.
fn verify_generation_freshness(
    root: &std::path::Path,
    selection: &lekalo_core::loader::LoadSelection,
    document: &lekalo_core::search::IndexDocument,
) -> FreshnessVerdict {
    let canonical_root = match std::fs::canonicalize(root) {
        Ok(canonical) => canonical,
        Err(_) => return FreshnessVerdict::Drifted("input-pin"),
    };
    for pin in &document.inputs {
        let path = root.join(&pin.path);
        match read_confined(&canonical_root, &path, "status") {
            Ok(bytes) => {
                let actual = format!("sha256:{}", lekalo_core::search::sha256_of(&bytes));
                if pin.digest.as_deref() != Some(actual.as_str()) {
                    return FreshnessVerdict::Drifted("input-pin");
                }
            }
            Err(_) => return FreshnessVerdict::Drifted("input-pin"),
        }
    }
    // Recompute the snapshot the way build would: the same canonical
    // Model assembly plus the pinned producers under the pinned
    // admission semantics (identity pins included), bounded reads, and
    // the manifest's own cap semantics. Any drift — model, producer
    // bytes, coverage, or policy — changes the content digest.
    let reconstructed = match reconstruct_manifest(document) {
        Some(manifest) => manifest,
        None => return FreshnessVerdict::Drifted("input-pin"),
    };
    let fresh = recompute_snapshot_digest(selection, &reconstructed);
    match fresh {
        Ok(digest) if digest == document.digest => FreshnessVerdict::Fresh,
        Ok(_) => FreshnessVerdict::Drifted("input-pin"),
        Err(result) => FreshnessVerdict::AdmissionRefused(result),
    }
}

/// Rebuild the closed input manifest from a generation's recorded pins:
/// kind, path, digest, and identity pins replay the exact admission
/// semantics the build ran under.
fn reconstruct_manifest(document: &lekalo_core::search::IndexDocument) -> Option<SearchInputs> {
    let mut json = String::from("{\"schemaVersion\":");
    json.push_str(&serde_json::to_string(lekalo_core::search::INPUTS_SCHEMA_VERSION).ok()?);
    json.push_str(",\"identity\":");
    json.push_str(&serde_json::to_string(lekalo_core::search::INPUTS_IDENTITY).ok()?);
    json.push_str(",\"projectId\":");
    json.push_str(&serde_json::to_string(document.project_id.as_deref().unwrap_or("")).ok()?);
    json.push_str(",\"inputs\":[");
    for (index, pin) in document.inputs.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str("{\"kind\":");
        json.push_str(&serde_json::to_string(&pin.kind).ok()?);
        json.push_str(",\"path\":");
        json.push_str(&serde_json::to_string(&pin.path).ok()?);
        if let Some(identity) = &pin.identity {
            json.push_str(",\"identity\":");
            json.push_str(&serde_json::to_string(identity).ok()?);
        }
        if let Some(digest) = &pin.digest {
            json.push_str(",\"digest\":");
            json.push_str(&serde_json::to_string(digest).ok()?);
        }
        json.push('}');
    }
    json.push_str("]}");
    SearchInputs::parse(json.as_bytes()).ok()
}

/// Recompute the exact snapshot digest a fresh build would publish for
/// the selected project under one reconstructed manifest: the same
/// admitted assembly the build path runs, hashed over its own snapshot
/// document. Admission failures surface as the refusal they are.
fn recompute_snapshot_digest(
    selection: &lekalo_core::loader::LoadSelection,
    manifest: &SearchInputs,
) -> Result<String, DomainResult> {
    let page = PageArgs {
        limit: None,
        cursor: None,
        project: selection.project.clone(),
        inputs: Some(manifest.project_id().to_owned()),
    };
    // Assemble resolves the (reconstructed) manifest through the same
    // confined read path as build — but the reconstructed manifest's
    // entries are already pinned to the recorded paths, so assemble the
    // service directly from the parsed manifest instead of re-resolving
    // a file that may legitimately no longer exist at a conventional
    // location.
    let service = assemble_with_manifest(selection, manifest)?;
    let _ = page;
    let document = snapshot_document(&service);
    Ok(format!(
        "sha256:{}",
        lekalo_core::search::digest_of(&document)
    ))
}

/// Assemble one admitted service from an already-parsed manifest (the
/// freshness oracle's re-admission): identical admission to `assemble`,
/// minus the manifest file re-read.
fn assemble_with_manifest(
    selection: &lekalo_core::loader::LoadSelection,
    manifest: &SearchInputs,
) -> Result<Service, DomainResult> {
    let root = lekalo_core::doctor::project_root(selection)?;
    // The freshness oracle is read-only: the bypass compile path never
    // touches the general compiler cache (the search store is only ever
    // read here), while the compilation itself stays deterministic, so
    // the recomputed digest is the one a build would publish.
    let (_model, compilation) = lekalo_core::cache::load_compiled(selection, true)?;
    let graph = lekalo_core::effects::build(&compilation.project).map_err(DomainResult::invalid)?;
    let cap = lekalo_core::search::MAX_POSTINGS;
    let mut projection = project_ir(&compilation.project, cap);
    attach_effects(&mut projection, &graph, cap);
    let mut coverage: Vec<lekalo_core::search::CoverageRow> = vec![
        coverage_row("ir", "complete"),
        coverage_row("effects", "complete"),
        coverage_row("trace", "unknown"),
        coverage_row("requirements", "unknown"),
        coverage_row("bindings", "unknown"),
        coverage_row("diagnostics", "unknown"),
        coverage_row("waivers", "unknown"),
    ];
    let project_id = compilation
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str().to_owned());
    if let Some(project) = &project_id {
        if manifest.project_id() != project.as_str() {
            return Err(DomainResult::invalid(
                lekalo_core::search::diagnostic_input_invalid("project-mismatch", None),
            ));
        }
    }
    let admitted = admit_producers(
        &root,
        project_id.as_deref(),
        manifest,
        &compilation.project,
        &mut projection,
        &mut coverage,
    )?;
    let run_cap = manifest
        .max_postings()
        .min(lekalo_core::search::MAX_POSTINGS);
    let service = Service {
        projection,
        project_id,
        coverage,
        policy_digest: String::new(),
        pins: admitted.pins,
        cap: run_cap,
        dropped_rows: admitted.dropped_rows,
    };
    Ok(Service {
        policy_digest: service.compute_policy_digest(),
        ..service
    })
}
